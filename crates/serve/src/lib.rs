//! A tool's server, the part every tool has (written for the suite, 2026-11-09, for
//! dexter-irma; BRR, Tape and ducky each hold their own of it, alike, until they move).
//!
//! Over [`as_http`] and [`as_ws`]: the listener, the checks on every connection (the site it
//! came from, `GET` and `HEAD` only), the WebSocket at `/ws`, the clients and what's sent to
//! them, and a tick. A tool brings what's its own through [`Tool`]: what a client is told on
//! connecting, what a command does, the words of a refusal, and any other path it serves.
//!
//! The order of two locks is fixed, so a tool can't deadlock on it: **the tool's own state
//! first, then the clients.** [`Tool::hello`] is handed a way to admit the client, to call
//! while it still holds its state: nothing sent to everyone can then fall between what the
//! client was told on connecting and its place among the clients. [`Clients::send`] is safe to
//! call with the state held, and the crate never calls into the tool with the clients held.
//!
//! A client that doesn't take what it's sent is closed, never left with part of the state: its
//! queue holds [`QUEUE`] messages, and one more ends it. It connects again and is told
//! everything afresh.

use std::collections::HashMap;
use std::io::{self, BufReader};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

pub use as_http::Request;
use as_ws::OP_TEXT;

#[cfg(test)]
mod tests;

/// How many messages can wait for one client.
pub const QUEUE: usize = 256;
/// How long the listener sleeps when nobody is connecting.
const IDLE: Duration = Duration::from_millis(50);

/// One client's number, from 1, never given twice.
pub type Id = u64;

struct Client {
    tx: SyncSender<Arc<[u8]>>,
    writer: as_ws::Writer,
}

/// The clients connected, and the way to send to them. Cheap to clone: clones share it.
#[derive(Clone, Default)]
pub struct Clients(Arc<Mutex<HashMap<Id, Client>>>);

impl Clients {
    pub fn new() -> Clients {
        Clients::default()
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<Id, Client>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// How many are connected.
    pub fn count(&self) -> usize {
        self.lock().len()
    }

    /// Sends `text` to every client. One that can't take it is closed.
    pub fn send(&self, text: &str) {
        let data: Arc<[u8]> = text.as_bytes().into();
        let mut clients = self.lock();
        clients.retain(|_, c| Self::queue(c, data.clone()));
    }

    /// Sends `text` to one client, if it's still there.
    pub fn send_to(&self, id: Id, text: &str) {
        let mut clients = self.lock();
        if let Some(c) = clients.get(&id)
            && !Self::queue(c, text.as_bytes().into())
        {
            clients.remove(&id);
        }
    }

    /// Queues a message. False if the client is to be forgotten: it was closed here because
    /// its queue is full, or it has gone already.
    fn queue(c: &Client, data: Arc<[u8]>) -> bool {
        match c.tx.try_send(data) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                c.writer.close();
                false
            }
            Err(TrySendError::Disconnected(_)) => false,
        }
    }

    /// Closes every client, for a server that's ending.
    pub fn close(&self) {
        for (_, c) in self.lock().drain() {
            c.writer.close();
        }
    }
}

/// A client being admitted: handed to [`Tool::hello`], which calls [`Joining::admit`] once.
pub struct Joining<'a> {
    clients: &'a Clients,
    id: Id,
    from: &'a str,
    client: Option<Client>,
}

impl Joining<'_> {
    /// Its number.
    pub fn id(&self) -> Id {
        self.id
    }

    /// The address it connected from (`127.0.0.1`, `192.168.1.20`), or `somewhere` when that
    /// couldn't be had: for a tool that lets only some addresses change things.
    pub fn from(&self) -> &str {
        self.from
    }

    /// Sends the client `hello`, in order, and puts it among the clients. Call it holding the
    /// tool's state, so what the client is told is the state as everyone else next hears it
    /// change.
    pub fn admit(&mut self, hello: Vec<String>) {
        let Some(c) = self.client.take() else { return };
        for m in hello {
            if !Clients::queue(&c, m.into_bytes().into()) {
                return;
            }
        }
        self.clients.lock().insert(self.id, c);
    }
}

/// What a tool brings to its server.
pub trait Tool: Send + Sync + 'static {
    /// A client has connected: admit it with everything it's to be told, by calling
    /// [`Joining::admit`] while holding whatever that's read from.
    fn hello(&self, joining: &mut Joining);

    /// A message from a client. An error is the reason it's refused, in words.
    fn command(&self, from: Id, data: &[u8]) -> Result<(), String>;

    /// The message that tells a client its command was refused, and why. Each tool's protocol
    /// has its own.
    fn refusal(&self, why: &str) -> String;

    /// Called [`Server::serve`]'s `ticks` a second, on a thread of its own.
    fn tick(&self) {}

    /// Any path but `/ws`, already checked for its site and its method (`GET` or `HEAD`): answer
    /// it on `w` and say true, or say false for a 404.
    fn get(&self, _req: &Request, _w: &mut TcpStream) -> bool {
        false
    }

    /// Something worth a line on stderr: a client come or gone, a listener's error.
    fn note(&self, _msg: &str) {}
}

/// A tool's server. Cheap to clone.
pub struct Server<T: Tool> {
    tool: Arc<T>,
    clients: Clients,
    stop: Arc<AtomicBool>,
    next: Arc<AtomicU64>,
}

impl<T: Tool> Clone for Server<T> {
    fn clone(&self) -> Server<T> {
        Server {
            tool: self.tool.clone(),
            clients: self.clients.clone(),
            stop: self.stop.clone(),
            next: self.next.clone(),
        }
    }
}

impl<T: Tool> Server<T> {
    /// A server for `tool`, sending to `clients`: the tool keeps a clone of them to send with.
    pub fn new(tool: Arc<T>, clients: Clients) -> Server<T> {
        Server { tool, clients, stop: Arc::new(AtomicBool::new(false)), next: Arc::new(AtomicU64::new(1)) }
    }

    /// Serves on `l` until [`Server::stop`]: a thread for each connection, and one that calls
    /// [`Tool::tick`] `ticks` times a second (none, for 0).
    pub fn serve(&self, l: TcpListener, ticks: u32) -> io::Result<()> {
        l.set_nonblocking(true)?;
        if ticks > 0 {
            let (tool, stop) = (self.tool.clone(), self.stop.clone());
            thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    tool.tick();
                    thread::sleep(Duration::from_millis(1000 / ticks as u64));
                }
            });
        }
        while !self.stop.load(Ordering::Relaxed) {
            match l.accept() {
                Ok((s, _)) => {
                    s.set_nonblocking(false)?;
                    let srv = self.clone();
                    thread::spawn(move || srv.connection(s));
                }
                Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock) => thread::sleep(IDLE),
                Err(e) if matches!(e.kind(), io::ErrorKind::Interrupted) => {}
                Err(e) => self.tool.note(&format!("accept: {e}")),
            }
        }
        self.clients.close();
        Ok(())
    }

    /// Ends [`Server::serve`], within a moment, and closes every client.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    pub fn stopping(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    fn connection(&self, s: TcpStream) {
        let Ok(mut w) = s.try_clone() else { return };
        let from = s.peer_addr().map_or("somewhere".to_string(), |a| a.ip().to_canonical().to_string());
        let mut r = BufReader::new(s);
        let Ok(req) = as_http::read_head(&mut r) else {
            return as_http::error(&mut w, 400, "400 Bad Request", &[]);
        };
        if !as_http::same_site(&req) {
            return as_http::error(&mut w, 403, "requests from another site are refused", &[]);
        }
        if req.method != "GET" && req.method != "HEAD" {
            return as_http::error(&mut w, 405, "Method Not Allowed", &[("Allow", "GET, HEAD")]);
        }
        if req.path == "/ws" {
            return match as_ws::handshake_key(&req.method, &req.headers).map(str::to_string) {
                Some(key) => self.client(&key, r, &from),
                None => as_http::error(&mut w, 400, "websocket upgrade required", &[]),
            };
        }
        if !self.tool.get(&req, &mut w) {
            as_http::error(&mut w, 404, "404 page not found", &[]);
        }
    }

    /// One client, from its upgrade until it goes: this thread reads, another writes.
    fn client(&self, key: &str, r: BufReader<TcpStream>, from: &str) {
        let Ok((mut reader, writer)) = as_ws::upgrade(key, r) else { return };
        let (tx, rx) = sync_channel::<Arc<[u8]>>(QUEUE);
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let mut joining =
            Joining { clients: &self.clients, id, from, client: Some(Client { tx, writer: writer.clone() }) };
        self.tool.hello(&mut joining);
        // A tool that didn't admit it has turned it away: with nothing holding the queue's
        // sending end, the writer below ends at once.
        drop(joining);
        self.tool.note(&format!("a client connected from {from} ({} now)", self.clients.count()));
        let w = writer.clone();
        let writing = thread::spawn(move || {
            for data in rx {
                if w.write(OP_TEXT, &data).is_err() {
                    break;
                }
            }
            w.close();
        });
        while let Ok((op, data)) = reader.read() {
            if op != OP_TEXT {
                continue;
            }
            if let Err(why) = self.tool.command(id, &data) {
                self.clients.send_to(id, &self.tool.refusal(&why));
            }
        }
        // Forgetting it drops the queue's sending end, which ends the writer once it has sent
        // what was waiting.
        self.clients.lock().remove(&id);
        writer.close();
        let _ = writing.join();
        self.tool.note(&format!("the client from {from} left ({} now)", self.clients.count()));
    }
}
