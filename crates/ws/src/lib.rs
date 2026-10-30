//! A small server-side WebSocket (RFC 6455) over `std::net`: the handshake, text and binary
//! messages, fragments, ping and pong, and close. No extensions.
//!
//! Moved into the suite from place's `place-ws`, itself Dexter's by way of the mixer
//! (`VENDORED.md`).
//!
//! The HTTP request is read by the caller (a tool's server), which hands over its method, its
//! headers and the connection. [`upgrade`] answers the handshake and splits the connection into
//! a [`Reader`], for one thread, and a [`Writer`] that any thread can use.

use std::io::{self, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const OP_CONT: u8 = 0x0;
pub const OP_TEXT: u8 = 0x1;
pub const OP_BINARY: u8 = 0x2;
pub const OP_CLOSE: u8 = 0x8;
pub const OP_PING: u8 = 0x9;
pub const OP_PONG: u8 = 0xA;

/// The most a client may send in one message. Commands are tiny.
pub const MAX_MESSAGE: usize = 64 << 10;

const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// The `Sec-WebSocket-Accept` value for a client's key.
pub fn accept(key: &str) -> String {
    base64(&sha1(format!("{key}{GUID}").as_bytes()))
}

/// Whether a comma-separated header holds a token, ignoring case.
fn header_has(headers: &[(String, String)], key: &str, token: &str) -> bool {
    headers
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case(key))
        .any(|(_, v)| v.split(',').any(|p| p.trim().eq_ignore_ascii_case(token)))
}

fn header<'a>(headers: &'a [(String, String)], key: &str) -> &'a str {
    headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map_or("", |(_, v)| v.as_str())
}

/// Whether a request is a WebSocket handshake; if so, its key.
pub fn handshake_key<'a>(method: &str, headers: &'a [(String, String)]) -> Option<&'a str> {
    let key = header(headers, "Sec-WebSocket-Key");
    let ok = method == "GET"
        && header_has(headers, "Connection", "upgrade")
        && header_has(headers, "Upgrade", "websocket")
        && !key.is_empty()
        && header(headers, "Sec-WebSocket-Version") == "13";
    ok.then_some(key)
}

/// Completes the handshake for key (from [`handshake_key`]) and takes over the connection.
/// `reader` is the connection as the request was read from it, with anything buffered after it.
pub fn upgrade(key: &str, reader: BufReader<TcpStream>) -> io::Result<(Reader, Writer)> {
    let mut stream = reader.get_ref().try_clone()?;
    let resp = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
        accept(key)
    );
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    stream.write_all(resp.as_bytes())?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    reader.get_ref().set_read_timeout(None)?;
    let w = Writer(Arc::new(Mutex::new(stream)));
    Ok((Reader { r: reader, w: w.clone() }, w))
}

/// The sending side of a connection; clones share it.
#[derive(Clone)]
pub struct Writer(Arc<Mutex<TcpStream>>);

impl Writer {
    /// Sends one unfragmented message. Server frames are never masked. A write that takes more
    /// than 2 s fails.
    pub fn write(&self, op: u8, data: &[u8]) -> io::Result<()> {
        let mut b = Vec::with_capacity(data.len() + 10);
        b.push(0x80 | op);
        match data.len() {
            l if l < 126 => b.push(l as u8),
            l if l <= 0xFFFF => {
                b.push(126);
                b.extend_from_slice(&(l as u16).to_be_bytes());
            }
            l => {
                b.push(127);
                b.extend_from_slice(&(l as u64).to_be_bytes());
            }
        }
        b.extend_from_slice(data);
        let mut s = self.0.lock().unwrap_or_else(|e| e.into_inner());
        s.write_all(&b)
    }

    /// Closes the connection without a close handshake. The reader's next read fails.
    pub fn close(&self) {
        let s = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let _ = s.shutdown(std::net::Shutdown::Both);
    }

    fn close_with(&self, code: u16) {
        let _ = self.write(OP_CLOSE, &code.to_be_bytes());
    }
}

/// The receiving side, for one thread.
pub struct Reader {
    r: BufReader<TcpStream>,
    w: Writer,
}

struct Frame {
    fin: bool,
    op: u8,
    data: Vec<u8>,
}

fn invalid(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("ws: {}", msg.into()))
}

/// The error for a message over [`MAX_MESSAGE`].
pub fn is_too_big(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::InvalidData && e.to_string() == "ws: message too big"
}

impl Reader {
    fn read_frame(&mut self) -> io::Result<Frame> {
        let mut h = [0u8; 2];
        self.r.read_exact(&mut h)?;
        let (fin, op) = (h[0] & 0x80 != 0, h[0] & 0x0F);
        if h[0] & 0x70 != 0 {
            return Err(invalid("reserved bits set"));
        }
        if h[1] & 0x80 == 0 {
            return Err(invalid("client frame not masked"));
        }
        let l = match h[1] & 0x7F {
            126 => {
                let mut b = [0u8; 2];
                self.r.read_exact(&mut b)?;
                u16::from_be_bytes(b) as u64
            }
            127 => {
                let mut b = [0u8; 8];
                self.r.read_exact(&mut b)?;
                u64::from_be_bytes(b)
            }
            l => l as u64,
        };
        if l > MAX_MESSAGE as u64 {
            return Err(invalid("message too big"));
        }
        if op >= OP_CLOSE && (l > 125 || !fin) {
            return Err(invalid("bad control frame"));
        }
        let mut mask = [0u8; 4];
        self.r.read_exact(&mut mask)?;
        let mut data = vec![0u8; l as usize];
        self.r.read_exact(&mut data)?;
        for (i, b) in data.iter_mut().enumerate() {
            *b ^= mask[i % 4];
        }
        Ok(Frame { fin, op, data })
    }

    /// The next text or binary message. It answers pings, ignores pongs, and fails with
    /// `UnexpectedEof` once the peer closes.
    pub fn read(&mut self) -> io::Result<(u8, Vec<u8>)> {
        let mut msg: Option<Vec<u8>> = None;
        let mut msg_op = 0;
        loop {
            let f = match self.read_frame() {
                Ok(f) => f,
                Err(e) => {
                    if is_too_big(&e) {
                        self.w.close_with(1009);
                    }
                    return Err(e);
                }
            };
            match f.op {
                OP_PING => {
                    self.w.write(OP_PONG, &f.data)?;
                    continue;
                }
                OP_PONG => continue,
                OP_CLOSE => {
                    self.w.close_with(1000);
                    return Err(io::ErrorKind::UnexpectedEof.into());
                }
                OP_TEXT | OP_BINARY => {
                    if msg.is_some() {
                        return Err(invalid("new message inside a fragmented one"));
                    }
                    msg_op = f.op;
                    msg = Some(f.data);
                }
                OP_CONT => {
                    let Some(m) = msg.as_mut() else { return Err(invalid("continuation without a message")) };
                    if m.len() + f.data.len() > MAX_MESSAGE {
                        self.w.close_with(1009);
                        return Err(invalid("message too big"));
                    }
                    m.extend_from_slice(&f.data);
                }
                op => return Err(invalid(format!("unknown opcode {op}"))),
            }
            if f.fin {
                return Ok((msg_op, msg.take().unwrap_or_default()));
            }
        }
    }
}

/// SHA-1 (FIPS 180-1), for the handshake only.
pub fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in msg.chunks(64) {
        let mut w = [0u32; 80];
        for (i, c) in block.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([c[0], c[1], c[2], c[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let t = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(*wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        for (x, v) in h.iter_mut().zip([a, b, c, d, e]) {
            *x = x.wrapping_add(v);
        }
    }
    let mut out = [0u8; 20];
    for (i, v) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

/// Standard base64, with padding.
pub fn base64(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                s.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digests() {
        let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
        assert_eq!(hex(&sha1(b"")), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(hex(&sha1(b"abc")), "a9993e364706816aba3e25717850c26c9cd0d89d");
        let long = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        assert_eq!(hex(&sha1(long)), "84983e441c3bd26ebaae4aa1f95129e5e54670f1");
        assert_eq!(hex(&sha1(&[b'a'; 1000])), "291e9a6c66994949b57ba5e650361e98fc36b1ba");
        for (i, want) in ["", "Zg==", "Zm8=", "Zm9v", "Zm9vYg==", "Zm9vYmE=", "Zm9vYmFy"].iter().enumerate() {
            assert_eq!(base64(&b"foobar"[..i]), *want);
        }
    }

    #[test]
    fn accept_key() {
        // The example from RFC 6455, section 1.3.
        assert_eq!(accept("dGhlIHNhbXBsZSBub25jZQ=="), "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }

    #[test]
    fn handshakes() {
        let h = |pairs: &[(&str, &str)]| {
            pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<Vec<_>>()
        };
        let good = h(&[
            ("Upgrade", "websocket"),
            ("Connection", "keep-alive, Upgrade"),
            ("Sec-WebSocket-Key", "k"),
            ("Sec-WebSocket-Version", "13"),
        ]);
        assert_eq!(handshake_key("GET", &good), Some("k"));
        assert_eq!(handshake_key("POST", &good), None);
        assert_eq!(handshake_key("GET", &good[1..]), None);
        assert_eq!(handshake_key("GET", &h(&[("Connection", "close")])), None);
    }
}
