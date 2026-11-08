//! An echo server over a real socket: messages, fragments with a ping
//! between them, and the close handshake.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

use as_ws::{OP_BINARY, OP_CLOSE, OP_CONT, OP_PING, OP_PONG, OP_TEXT, handshake_key, upgrade};

/// A masked client frame.
fn client_frame(fin: bool, op: u8, payload: &[u8]) -> Vec<u8> {
    let mut b = vec![op | if fin { 0x80 } else { 0 }];
    if payload.len() < 126 {
        b.push(0x80 | payload.len() as u8);
    } else {
        b.push(0x80 | 126);
        b.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    }
    let mask = [1u8, 2, 3, 4];
    b.extend_from_slice(&mask);
    b.extend(payload.iter().enumerate().map(|(i, c)| c ^ mask[i % 4]));
    b
}

fn server_frame(r: &mut BufReader<TcpStream>) -> (u8, Vec<u8>) {
    let mut h = [0u8; 2];
    r.read_exact(&mut h).unwrap();
    let mut l = (h[1] & 0x7F) as usize;
    if l == 126 {
        let mut b = [0u8; 2];
        r.read_exact(&mut b).unwrap();
        l = u16::from_be_bytes(b) as usize;
    }
    let mut d = vec![0u8; l];
    r.read_exact(&mut d).unwrap();
    (h[0] & 0x0F, d)
}

fn echo_server() -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap().to_string();
    thread::spawn(move || {
        let (s, _) = l.accept().unwrap();
        let mut r = BufReader::new(s);
        let mut first = String::new();
        r.read_line(&mut first).unwrap();
        let mut headers = Vec::new();
        loop {
            let mut l = String::new();
            r.read_line(&mut l).unwrap();
            let l = l.trim_end();
            if l.is_empty() {
                break;
            }
            let (k, v) = l.split_once(':').unwrap();
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
        let key = handshake_key("GET", &headers).unwrap().to_string();
        let (mut reader, writer) = upgrade(&key, r).unwrap();
        while let Ok((op, data)) = reader.read() {
            writer.write(op, &data).unwrap();
        }
    });
    addr
}

#[test]
fn echo_fragments_ping_close() {
    let addr = echo_server();
    let mut s = TcpStream::connect(&addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    s.write_all(b"GET / HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n").unwrap();
    let mut r = BufReader::new(s.try_clone().unwrap());
    let mut head = String::new();
    loop {
        let mut l = String::new();
        r.read_line(&mut l).unwrap();
        if l == "\r\n" {
            break;
        }
        head.push_str(&l);
    }
    assert!(
        head.starts_with("HTTP/1.1 101")
            && head.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo="),
        "{head}"
    );

    s.write_all(&client_frame(true, OP_TEXT, b"hello")).unwrap();
    assert_eq!(server_frame(&mut r), (OP_TEXT, b"hello".to_vec()), "echo");

    let long = "x".repeat(300);
    s.write_all(&client_frame(false, OP_BINARY, &long.as_bytes()[..100])).unwrap();
    s.write_all(&client_frame(true, OP_PING, b"p")).unwrap(); // a control frame between fragments
    assert_eq!(server_frame(&mut r), (OP_PONG, b"p".to_vec()), "pong");
    s.write_all(&client_frame(true, OP_CONT, &long.as_bytes()[100..])).unwrap();
    assert_eq!(server_frame(&mut r), (OP_BINARY, long.into_bytes()), "the fragments, echoed whole");

    s.write_all(&client_frame(true, OP_CLOSE, &[0x03, 0xE8])).unwrap();
    assert_eq!(server_frame(&mut r).0, OP_CLOSE, "the close is answered");
}
