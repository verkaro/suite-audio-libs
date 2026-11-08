//! The client's half: a WebSocket (RFC 6455) opened to a tool's server, for a panel or anything
//! else that talks to one. The handshake, text and binary messages in with fragments joined,
//! pings answered, and messages out, masked as a client's must be. No TLS: the servers are on
//! this machine or the studio's network.
//!
//! Moved into the suite from dexter-kms's `src/ws.rs` (2026-11-07), for Tape's panel and the
//! ones after it. Written there for the deck server's `/ws`.
//!
//! [`connect`] answers with a [`Reader`], for one thread, and a [`Writer`] that any thread can
//! use. Reading waits as long as it takes, unless a timeout is set on the reader: a server that
//! sends steadily (the deck's 30 frames a second) can be thought gone when it goes quiet.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use crate::{OP_BINARY, OP_CLOSE, OP_CONT, OP_PING, OP_PONG, OP_TEXT, accept, base64};

/// The most a server may send in one message: a layout can be large.
pub const MAX_IN: usize = 16 << 20;

/// How long the server has to answer the handshake.
const HANDSHAKE: Duration = Duration::from_secs(5);

/// A whole message from the server.
#[derive(Debug, PartialEq, Eq)]
pub enum Msg {
    /// Text, as UTF-8 (anything else in it replaced).
    Text(String),
    Binary(Vec<u8>),
}

/// The receiving side, for one thread.
pub struct Reader {
    r: BufReader<TcpStream>,
    w: Writer,
}

/// The sending side; clones share it.
#[derive(Clone)]
pub struct Writer(Arc<Mutex<TcpStream>>);

/// Opens `path` on the server at `addr` (`host:port`) as a WebSocket. Fails if the server
/// doesn't answer the handshake within 5 s, refuses it, or answers it wrongly.
pub fn connect(addr: &str, path: &str) -> io::Result<(Reader, Writer)> {
    let s = TcpStream::connect(addr)?;
    s.set_nodelay(true)?;
    s.set_read_timeout(Some(HANDSHAKE))?;
    let mut w = s.try_clone()?;
    let key = base64(&nonce().to_le_bytes().into_iter().chain(nonce().to_le_bytes()).collect::<Vec<_>>());
    write!(
        w,
        "GET {path} HTTP/1.1\r\nHost: {addr}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
         Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
    )?;
    let mut r = BufReader::new(s.try_clone()?);
    let mut status = String::new();
    r.read_line(&mut status)?;
    if !status.starts_with("HTTP/1.1 101") {
        return Err(io::Error::other(format!("{addr}{path}: {}", status.trim())));
    }
    let mut accepted = None;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line)? == 0 || line == "\r\n" {
            break;
        }
        if let Some((k, v)) = line.split_once(':')
            && k.trim().eq_ignore_ascii_case("Sec-WebSocket-Accept")
        {
            accepted = Some(v.trim().to_string());
        }
    }
    if accepted.as_deref() != Some(accept(&key).as_str()) {
        return Err(io::Error::other(format!("{addr}{path}: the handshake's answer is wrong")));
    }
    s.set_read_timeout(None)?;
    let w = Writer(Arc::new(Mutex::new(w)));
    Ok((Reader { r, w: w.clone() }, w))
}

impl Writer {
    pub fn send_text(&self, text: &str) -> io::Result<()> {
        self.send(OP_TEXT, text.as_bytes())
    }

    pub fn send_binary(&self, data: &[u8]) -> io::Result<()> {
        self.send(OP_BINARY, data)
    }

    /// Says goodbye (1000) and closes the connection. The reader's next read fails.
    pub fn close(&self) {
        let _ = self.send(OP_CLOSE, &1000u16.to_be_bytes());
        let s = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let _ = s.shutdown(std::net::Shutdown::Both);
    }

    fn send(&self, op: u8, data: &[u8]) -> io::Result<()> {
        let mut f = Vec::with_capacity(data.len() + 14);
        f.push(0x80 | op);
        let n = data.len();
        if n < 126 {
            f.push(0x80 | n as u8);
        } else if n < 65536 {
            f.push(0x80 | 126);
            f.extend((n as u16).to_be_bytes());
        } else {
            f.push(0x80 | 127);
            f.extend((n as u64).to_be_bytes());
        }
        let mask = (nonce() as u32).to_be_bytes();
        f.extend(mask);
        f.extend(data.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        self.0.lock().unwrap_or_else(|e| e.into_inner()).write_all(&f)
    }
}

impl Reader {
    /// A read that waits longer than `t` fails (`None`: never). Every clone of the connection
    /// shares it.
    pub fn set_read_timeout(&self, t: Option<Duration>) -> io::Result<()> {
        self.r.get_ref().set_read_timeout(t)
    }

    /// The next whole message. Pings are answered and pongs passed over on the way. An error
    /// when the connection ends: `ConnectionAborted` when the server closed it.
    pub fn read(&mut self) -> io::Result<Msg> {
        let mut msg = Vec::new();
        let mut kind = None;
        loop {
            let mut h = [0u8; 2];
            self.r.read_exact(&mut h)?;
            let fin = h[0] & 0x80 != 0;
            let op = h[0] & 0x0f;
            let mut len = (h[1] & 0x7f) as u64;
            if len == 126 {
                let mut b = [0u8; 2];
                self.r.read_exact(&mut b)?;
                len = u16::from_be_bytes(b) as u64;
            } else if len == 127 {
                let mut b = [0u8; 8];
                self.r.read_exact(&mut b)?;
                len = u64::from_be_bytes(b);
            }
            // A server's frames aren't masked, but one that is is read all the same.
            let mask = if h[1] & 0x80 != 0 {
                let mut m = [0u8; 4];
                self.r.read_exact(&mut m)?;
                Some(m)
            } else {
                None
            };
            if len + msg.len() as u64 > MAX_IN as u64 {
                return Err(io::Error::other("message too long"));
            }
            let mut data = vec![0u8; len as usize];
            self.r.read_exact(&mut data)?;
            if let Some(m) = mask {
                data.iter_mut().enumerate().for_each(|(i, b)| *b ^= m[i % 4]);
            }
            match op {
                OP_CLOSE => {
                    let _ = self.w.send(OP_CLOSE, &data[..data.len().min(2)]);
                    return Err(io::Error::new(io::ErrorKind::ConnectionAborted, "closed by the server"));
                }
                OP_PING => {
                    self.w.send(OP_PONG, &data)?;
                    continue;
                }
                OP_PONG => continue,
                OP_CONT if kind.is_some() => msg.extend(data),
                OP_TEXT | OP_BINARY if kind.is_none() => {
                    kind = Some(op);
                    msg = data;
                }
                op => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("ws: unexpected opcode {op}"),
                    ));
                }
            }
            if fin {
                return Ok(match kind {
                    Some(OP_TEXT) => Msg::Text(String::from_utf8_lossy(&msg).into_owned()),
                    _ => Msg::Binary(msg),
                });
            }
        }
    }
}

/// 64 bits that differ every call and every run, for the handshake's key and the masks.
/// Masking is for proxies, not secrecy: this needn't be strong, only not fixed.
fn nonce() -> u64 {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let t = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    let mut z = t ^ (std::process::id() as u64) << 32 ^ COUNT.fetch_add(1, Ordering::Relaxed);
    // splitmix64's finish.
    z = z.wrapping_add(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    /// A listener on a free port, and its address.
    fn listen() -> (TcpListener, String) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let a = l.local_addr().unwrap().to_string();
        (l, a)
    }

    /// The request's head, as the method and the headers.
    fn read_request(r: &mut BufReader<TcpStream>) -> (String, Vec<(String, String)>) {
        let mut line = String::new();
        r.read_line(&mut line).unwrap();
        let method = line.split(' ').next().unwrap().to_string();
        let mut headers = Vec::new();
        loop {
            let mut l = String::new();
            r.read_line(&mut l).unwrap();
            if l == "\r\n" {
                break;
            }
            let (k, v) = l.split_once(':').unwrap();
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
        (method, headers)
    }

    /// One unmasked frame, as a server sends.
    fn frame(fin: bool, op: u8, data: &[u8]) -> Vec<u8> {
        let mut b = vec![op | if fin { 0x80 } else { 0 }];
        match data.len() {
            l if l < 126 => b.push(l as u8),
            l if l < 65536 => {
                b.push(126);
                b.extend((l as u16).to_be_bytes());
            }
            l => {
                b.push(127);
                b.extend((l as u64).to_be_bytes());
            }
        }
        b.extend_from_slice(data);
        b
    }

    /// One frame from the client: whether it was masked, its opcode, its data unmasked.
    fn client_frame(r: &mut impl Read) -> (bool, u8, Vec<u8>) {
        let mut h = [0u8; 2];
        r.read_exact(&mut h).unwrap();
        let mut l = (h[1] & 0x7f) as usize;
        if l == 126 {
            let mut b = [0u8; 2];
            r.read_exact(&mut b).unwrap();
            l = u16::from_be_bytes(b) as usize;
        }
        let masked = h[1] & 0x80 != 0;
        let mut m = [0u8; 4];
        if masked {
            r.read_exact(&mut m).unwrap();
        }
        let mut d = vec![0u8; l];
        r.read_exact(&mut d).unwrap();
        d.iter_mut().enumerate().for_each(|(i, b)| *b ^= m[i % 4]);
        (masked, h[0] & 0x0f, d)
    }

    #[test]
    fn talks_to_the_suites_own_server() {
        let (l, addr) = listen();
        let server = thread::spawn(move || {
            let (s, _) = l.accept().unwrap();
            let mut r = BufReader::new(s);
            let (method, headers) = read_request(&mut r);
            let key = crate::handshake_key(&method, &headers).expect("a handshake").to_string();
            let (mut rd, w) = crate::upgrade(&key, r).unwrap();
            w.write(OP_TEXT, b"hello").unwrap();
            // What the client sends comes back as binary, and a big one after it.
            let (op, got) = rd.read().unwrap();
            assert_eq!(op, OP_TEXT);
            w.write(OP_BINARY, &got).unwrap();
            w.write(OP_BINARY, &vec![7u8; 70_000]).unwrap();
            let (op, got) = rd.read().unwrap();
            assert_eq!((op, got), (OP_BINARY, vec![1, 2, 3]));
        });
        let (mut r, w) = connect(&addr, "/ws").unwrap();
        assert_eq!(r.read().unwrap(), Msg::Text("hello".into()));
        // Long enough for a 16-bit length; the server refuses an unmasked frame.
        let long = "x".repeat(1000);
        w.send_text(&long).unwrap();
        assert_eq!(r.read().unwrap(), Msg::Binary(long.into_bytes()));
        assert_eq!(r.read().unwrap(), Msg::Binary(vec![7u8; 70_000]));
        w.send_binary(&[1, 2, 3]).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn fragments_pings_and_the_close() {
        let (l, addr) = listen();
        let server = thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut r = BufReader::new(s.try_clone().unwrap());
            let (method, headers) = read_request(&mut r);
            assert_eq!(method, "GET");
            let key = crate::handshake_key(&method, &headers).expect("a handshake");
            let answer = format!(
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
                 sec-websocket-accept: {}\r\n\r\n",
                accept(key)
            );
            s.write_all(answer.as_bytes()).unwrap();
            // A ping in the middle of a fragmented message, then a close.
            let mut out = frame(false, OP_TEXT, b"hel");
            out.extend(frame(true, OP_PING, b"p1"));
            out.extend(frame(false, OP_CONT, b"lo, "));
            out.extend(frame(true, OP_CONT, "wörld".as_bytes()));
            out.extend(frame(true, OP_PONG, b""));
            out.extend(frame(true, OP_BINARY, &[0, 255]));
            out.extend(frame(true, OP_CLOSE, &1001u16.to_be_bytes()));
            s.write_all(&out).unwrap();
            assert_eq!(client_frame(&mut r), (true, OP_PONG, b"p1".to_vec()));
            assert_eq!(client_frame(&mut r), (true, OP_CLOSE, 1001u16.to_be_bytes().to_vec()));
        });
        let (mut r, _w) = connect(&addr, "/ws").unwrap();
        assert_eq!(r.read().unwrap(), Msg::Text("hello, wörld".into()));
        assert_eq!(r.read().unwrap(), Msg::Binary(vec![0, 255]));
        let e = r.read().unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::ConnectionAborted);
        server.join().unwrap();
    }

    /// A server that answers the handshake with `answer`, given the request's key.
    fn answering(answer: fn(&str) -> String) -> String {
        let (l, addr) = listen();
        thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut r = BufReader::new(s.try_clone().unwrap());
            let (method, headers) = read_request(&mut r);
            let key = crate::handshake_key(&method, &headers).unwrap_or("").to_string();
            s.write_all(answer(&key).as_bytes()).unwrap();
            // Held open, so a client that waited would wait.
            thread::sleep(Duration::from_secs(1));
        });
        addr
    }

    #[test]
    fn a_refused_or_wrong_handshake_fails() {
        let addr = answering(|_| "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".into());
        let e = connect(&addr, "/ws").err().unwrap();
        assert!(e.to_string().ends_with("/ws: HTTP/1.1 404 Not Found"), "{e}");
        let addr = answering(|_| {
            "HTTP/1.1 101 Switching Protocols\r\nSec-WebSocket-Accept: bm90IGl0\r\n\r\n".into()
        });
        let e = connect(&addr, "/ws").err().unwrap();
        assert!(e.to_string().contains("answer is wrong"), "{e}");
        let addr = answering(|_| "HTTP/1.1 101 Switching Protocols\r\n\r\n".into());
        assert!(connect(&addr, "/ws").is_err());
    }

    #[test]
    fn keys_and_masks_differ() {
        let n: Vec<u64> = (0..100).map(|_| nonce()).collect();
        let mut d = n.clone();
        d.sort();
        d.dedup();
        assert_eq!(d.len(), n.len());
    }
}
