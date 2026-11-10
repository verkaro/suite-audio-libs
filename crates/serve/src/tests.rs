//! Over a real socket, with the suite's own client.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::time::{Duration, Instant};

use as_ws::client::{self, Msg};

use super::*;

/// A tool that counts: its state is a number, a command adds to it or is refused.
struct Counter {
    clients: Clients,
    n: Mutex<i64>,
    ticks: AtomicUsize,
    notes: Mutex<Vec<String>>,
}

impl Tool for Counter {
    fn hello(&self, joining: &mut Joining) {
        let n = self.n.lock().unwrap();
        joining.admit(vec!["hello".into(), format!("n {n}")]);
    }

    fn command(&self, from: Id, data: &[u8]) -> Result<(), String> {
        let text = String::from_utf8_lossy(data);
        let by: i64 = text.parse().map_err(|_| format!("{text} isn't a number"))?;
        let mut n = self.n.lock().unwrap();
        *n += by;
        // Sent with the state held: the order the crate promises is safe.
        self.clients.send(&format!("n {n} by {from}"));
        Ok(())
    }

    fn refusal(&self, why: &str) -> String {
        format!("refused: {why}")
    }

    fn tick(&self) {
        self.ticks.fetch_add(1, Ordering::Relaxed);
    }

    fn get(&self, req: &Request, w: &mut TcpStream) -> bool {
        if req.path != "/" {
            return false;
        }
        as_http::respond(w, req.method == "HEAD", 200, &[("Content-Type", "text/plain")], b"a counter\n");
        true
    }

    fn note(&self, msg: &str) {
        self.notes.lock().unwrap().push(msg.to_string());
    }
}

struct Running {
    server: Server<Counter>,
    tool: Arc<Counter>,
    addr: String,
    thread: Option<thread::JoinHandle<()>>,
}

fn start() -> Running {
    let clients = Clients::new();
    let tool = Arc::new(Counter {
        clients: clients.clone(),
        n: Mutex::new(0),
        ticks: AtomicUsize::new(0),
        notes: Mutex::new(vec![]),
    });
    let server = Server::new(tool.clone(), clients);
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap().to_string();
    let s = server.clone();
    let thread = Some(thread::spawn(move || s.serve(l, 50).unwrap()));
    Running { server, tool, addr, thread }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.server.stop();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn connect(addr: &str) -> (client::Reader, client::Writer) {
    let (r, w) = client::connect(addr, "/ws").unwrap();
    r.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    (r, w)
}

fn text(r: &mut client::Reader) -> String {
    match r.read().unwrap() {
        Msg::Text(t) => t,
        Msg::Binary(_) => panic!("binary"),
    }
}

/// Waits for something to become true, for what happens on another thread.
fn until(what: &str, f: impl Fn() -> bool) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(5), "waited 5 s for {what}");
        thread::sleep(Duration::from_millis(5));
    }
}

fn http(addr: &str, request: &str) -> String {
    let mut s = TcpStream::connect(addr).unwrap();
    s.write_all(request.as_bytes()).unwrap();
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    out
}

#[test]
fn a_client_is_told_everything_then_hears_each_change() {
    let run = start();
    let (mut a, a_w) = connect(&run.addr);
    assert_eq!((text(&mut a), text(&mut a)), ("hello".into(), "n 0".into()));
    a_w.send_text("5").unwrap();
    assert_eq!(text(&mut a), "n 5 by 1");
    // A second client starts from the state as it is, and both hear what follows.
    let (mut b, b_w) = connect(&run.addr);
    assert_eq!((text(&mut b), text(&mut b)), ("hello".into(), "n 5".into()));
    assert_eq!(run.server.clients.count(), 2);
    b_w.send_text("-2").unwrap();
    assert_eq!((text(&mut a), text(&mut b)), ("n 3 by 2".into(), "n 3 by 2".into()));
    // A refusal goes to the one that asked, in the tool's own words, and nothing changes.
    b_w.send_text("lots").unwrap();
    assert_eq!(text(&mut b), "refused: lots isn't a number");
    a_w.send_text("1").unwrap();
    assert_eq!(text(&mut a), "n 4 by 1", "a heard no refusal");
    assert_eq!(text(&mut b), "n 4 by 1");
    // One leaves and is forgotten; the other carries on.
    b_w.close();
    until("the client to be forgotten", || run.server.clients.count() == 1);
    a_w.send_text("1").unwrap();
    assert_eq!(text(&mut a), "n 5 by 1");
    until("the notes", || run.tool.notes.lock().unwrap().iter().any(|n| n.contains("left (1 now)")));
    assert!(run.tool.ticks.load(Ordering::Relaxed) > 0, "it ticks");
}

#[test]
fn every_connection_is_checked_before_the_tool_sees_it() {
    let run = start();
    let get = |path: &str, extra: &str| {
        http(&run.addr, &format!("GET {path} HTTP/1.1\r\nHost: {}\r\n{extra}\r\n", run.addr))
    };
    let ok = get("/", "");
    assert!(ok.starts_with("HTTP/1.1 200") && ok.ends_with("a counter\n"), "{ok}");
    assert!(get("/nothing", "").starts_with("HTTP/1.1 404"));
    assert!(get("/ws", "").starts_with("HTTP/1.1 400"), "no upgrade asked for");
    // Another site's page, by its Origin: refused, the tool's own path too.
    assert!(get("/", "Origin: http://elsewhere.example\r\n").starts_with("HTTP/1.1 403"));
    assert!(get("/ws", "Origin: http://elsewhere.example\r\n").starts_with("HTTP/1.1 403"));
    let post =
        http(&run.addr, &format!("POST / HTTP/1.1\r\nHost: {}\r\nContent-Length: 0\r\n\r\n", run.addr));
    assert!(post.starts_with("HTTP/1.1 405"), "{post}");
    let head = http(&run.addr, &format!("HEAD / HTTP/1.1\r\nHost: {}\r\n\r\n", run.addr));
    assert!(head.starts_with("HTTP/1.1 200") && !head.contains("a counter"), "{head}");
    assert!(http(&run.addr, "nonsense\r\n\r\n").starts_with("HTTP/1.1 400"));
}

#[test]
fn a_client_that_doesnt_listen_is_closed_and_the_rest_arent_held_up() {
    let run = start();
    let (mut a, a_w) = connect(&run.addr);
    text(&mut a);
    text(&mut a);
    // One that connects and never reads. The server sends far more than its queue and the
    // socket between them hold.
    let (_deaf, _deaf_w) = connect(&run.addr);
    until("both to be there", || run.server.clients.count() == 2);
    let big = "x".repeat(60_000);
    // The one that listens takes each as it's sent, and is never kept waiting by the other.
    for _ in 0..QUEUE * 4 {
        run.tool.clients.send(&big);
        assert_eq!(text(&mut a).len(), 60_000);
    }
    until("the deaf one to be closed", || run.server.clients.count() == 1);
    a_w.send_text("1").unwrap();
    assert_eq!(text(&mut a), "n 1 by 1");
}

#[test]
fn stopping_ends_the_serving_and_closes_the_clients() {
    let mut run = start();
    let (mut a, _a_w) = connect(&run.addr);
    text(&mut a);
    text(&mut a);
    run.server.stop();
    run.thread.take().unwrap().join().unwrap();
    assert!(run.server.stopping());
    assert!(a.read().is_err(), "the client is told it's over");
    assert_eq!(run.server.clients.count(), 0);
}
