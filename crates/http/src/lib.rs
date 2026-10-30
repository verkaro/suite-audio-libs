//! Just enough HTTP/1.1 for a page: one request per connection, then `Connection: close`.
//! Headers must arrive within 5 s. Moved into the suite from dexter-foley's, which had it from
//! place, from Dexter by way of the mixer (`VENDORED.md`); the byte ranges a sound is played
//! with ([`file`]) are dexter-foley's, and [`listen`] place's.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// The most a request's line and headers may take.
const MAX_HEADER: usize = 1 << 20;
/// Bodies are read up to this: a sound's tags, with room for a long description.
pub const MAX_BODY: usize = 64 << 10;

pub struct Request {
    pub method: String,
    /// Without the query.
    pub path: String,
    /// After the `?`, as it came.
    pub query: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn header(&self, key: &str) -> &str {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map_or("", |(_, v)| v.as_str())
    }
}

/// Reads one request. The body is read up to [`MAX_BODY`], and only when it has a
/// Content-Length.
pub fn read_request(r: &mut BufReader<TcpStream>) -> io::Result<Request> {
    r.get_ref().set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut total = 0;
    let mut line = |r: &mut BufReader<TcpStream>| -> io::Result<String> {
        let mut l = Vec::new();
        let n = r.by_ref().take((MAX_HEADER - total) as u64 + 1).read_until(b'\n', &mut l)?;
        total += n;
        if n == 0 || total > MAX_HEADER || !l.ends_with(b"\n") {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "bad request"));
        }
        let s =
            String::from_utf8(l).map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "bad request"))?;
        Ok(s.trim_end_matches(['\r', '\n']).to_string())
    };
    let first = line(r)?;
    let mut parts = first.split(' ');
    let (Some(method), Some(target), Some(version)) = (parts.next(), parts.next(), parts.next()) else {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "bad request line"));
    };
    if !version.starts_with("HTTP/1.") {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "not HTTP/1"));
    }
    let mut headers = Vec::new();
    loop {
        let l = line(r)?;
        if l.is_empty() {
            break;
        }
        let Some((k, v)) = l.split_once(':') else {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "bad header"));
        };
        headers.push((k.trim().to_string(), v.trim().to_string()));
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let mut req = Request {
        method: method.to_string(),
        path: path.to_string(),
        query: query.to_string(),
        headers,
        body: Vec::new(),
    };
    let len: usize = req.header("Content-Length").parse().unwrap_or(0);
    if len > 0 {
        r.get_ref().set_read_timeout(Some(Duration::from_secs(10)))?;
        r.by_ref().take(len.min(MAX_BODY) as u64).read_to_end(&mut req.body)?;
    }
    Ok(req)
}

/// Whether a request may come from where it says: refuses a browser's request made by another
/// site's page (its `Origin` isn't this `Host`), and a `Host` that names this machine by a
/// public domain, which only DNS rebinding would send here. The page itself, other
/// screens on the LAN (by address, `localhost`, or a local name such as `studio.local`) and
/// clients that aren't browsers (no `Origin`) all pass.
pub fn same_site(req: &Request) -> bool {
    let host = req.header("Host");
    if !host.is_empty() && !local_host(host) {
        return false;
    }
    let origin = req.header("Origin");
    origin.is_empty() || origin.strip_prefix("http://").is_some_and(|o| o.eq_ignore_ascii_case(host))
}

/// Suffixes of names on a local network: mDNS's, the reserved ones, and home routers'.
const LOCAL_SUFFIXES: [&str; 8] =
    [".localhost", ".local", ".lan", ".home", ".home.arpa", ".internal", ".localdomain", ".fritz.box"];

/// A `Host` header's name (without its port) is an address, `localhost`, a single label, this
/// machine's own name with any domain, or a name under a suffix of a local network.
fn local_host(host: &str) -> bool {
    let name = match host.strip_prefix('[') {
        Some(v6) => return v6.split_once(']').is_some_and(|(a, _)| a.parse::<std::net::Ipv6Addr>().is_ok()),
        None => host.rsplit_once(':').map_or(host, |(n, _)| n),
    };
    let name = name.trim_end_matches('.').to_ascii_lowercase();
    let first = name.split('.').next().unwrap_or_default();
    name.parse::<std::net::Ipv4Addr>().is_ok()
        || !name.contains('.')
        || LOCAL_SUFFIXES.iter().any(|s| name.ends_with(s))
        || std::fs::read_to_string("/proc/sys/kernel/hostname")
            .is_ok_and(|h| h.trim().eq_ignore_ascii_case(first))
}

/// A query's parameter, percent-decoded (`+` as a space).
pub fn param(query: &str, key: &str) -> Option<String> {
    let raw = query.split('&').find_map(|kv| kv.strip_prefix(key)?.strip_prefix('='))?;
    let b = raw.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                let h =
                    std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok());
                match h {
                    Some(v) => {
                        out.push(v);
                        i += 2;
                    }
                    None => out.push(b'%'),
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    Some(String::from_utf8_lossy(&out).into_owned())
}

/// Writes a whole response and closes. HEAD gets the headers only.
pub fn respond(w: &mut TcpStream, head_only: bool, code: u16, headers: &[(&str, &str)], body: &[u8]) {
    let reason = match code {
        200 => "OK",
        206 => "Partial Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        416 => "Range Not Satisfiable",
        _ => "Internal Server Error",
    };
    let mut out = format!("HTTP/1.1 {code} {reason}\r\n");
    for (k, v) in headers {
        out.push_str(&format!("{k}: {v}\r\n"));
    }
    out.push_str(&format!("Content-Length: {}\r\nConnection: close\r\n\r\n", body.len()));
    let _ = w.set_write_timeout(Some(Duration::from_secs(10)));
    let mut b = out.into_bytes();
    if !head_only {
        b.extend_from_slice(body);
    }
    let _ = w.write_all(&b);
    let _ = w.flush();
    let _ = w.shutdown(std::net::Shutdown::Write);
}

/// The head of an answer whose body follows as it's made, its length unknown: the body ends
/// when the connection closes.
pub fn stream_head(w: &mut TcpStream, headers: &[(&str, &str)]) -> io::Result<()> {
    let mut out = "HTTP/1.1 200 OK\r\n".to_string();
    for (k, v) in headers {
        out.push_str(&format!("{k}: {v}\r\n"));
    }
    out.push_str("Cache-Control: no-store\r\nConnection: close\r\n\r\n");
    w.set_write_timeout(Some(Duration::from_secs(60)))?;
    w.write_all(out.as_bytes())
}

/// `Content-Disposition` for a file the browser saves as `name`.
pub fn attachment(name: &str) -> String {
    let plain: String =
        name.chars().map(|c| if c.is_ascii_graphic() || c == ' ' { c } else { '_' }).collect();
    let plain = plain.replace(['"', '\\'], "_");
    let encoded: String = name
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    format!("attachment; filename=\"{plain}\"; filename*=UTF-8''{encoded}")
}

/// A file, whole or the one byte range asked for (`Range: bytes=A-B`, `A-` or `-N`), so a
/// browser can seek in a sound while it plays.
pub fn file(w: &mut TcpStream, req: &Request, path: &std::path::Path, content_type: &str) {
    let head_only = req.method == "HEAD";
    let Ok(mut f) = std::fs::File::open(path) else { return error(w, 404, "not found", &[]) };
    let Ok(len) = f.metadata().map(|m| m.len()) else { return error(w, 500, "unreadable", &[]) };
    let (code, from, to) = match range(req.header("Range"), len) {
        Some(Some((a, b))) => (206, a, b),
        Some(None) => {
            let all = format!("bytes */{len}");
            return error(w, 416, "no such range", &[("Content-Range", &all)]);
        }
        None => (200, 0, len.saturating_sub(1)),
    };
    let n = if len == 0 { 0 } else { to + 1 - from };
    let mut body = vec![0u8; n as usize];
    if !head_only
        && (io::Seek::seek(&mut f, io::SeekFrom::Start(from)).is_err() || f.read_exact(&mut body).is_err())
    {
        return error(w, 500, "unreadable", &[]);
    }
    let cr = format!("bytes {from}-{to}/{len}");
    let mut h =
        vec![("Content-Type", content_type), ("Accept-Ranges", "bytes"), ("Cache-Control", "no-cache")];
    if code == 206 {
        h.push(("Content-Range", &cr));
    }
    if head_only {
        // The length is the range's, with no body.
        let mut out = format!("HTTP/1.1 {code} OK\r\n");
        for (k, v) in &h {
            out.push_str(&format!("{k}: {v}\r\n"));
        }
        out.push_str(&format!("Content-Length: {n}\r\nConnection: close\r\n\r\n"));
        let _ = w.write_all(out.as_bytes());
        return;
    }
    respond(w, false, code, &h, &body);
}

/// A `Range` header's one byte range within `len` bytes: `None` for no range (or one this
/// doesn't read, which gets the whole file), `Some(None)` for one past the end.
fn range(header: &str, len: u64) -> Option<Option<(u64, u64)>> {
    let spec = header.strip_prefix("bytes=")?;
    if spec.contains(',') {
        return None;
    }
    let (a, b) = spec.split_once('-')?;
    let (a, b) = (a.trim(), b.trim());
    let r = match (a.parse::<u64>().ok(), b.parse::<u64>().ok()) {
        (Some(a), Some(b)) if a <= b => (a, b.min(len.saturating_sub(1))),
        (Some(a), None) if b.is_empty() => (a, len.saturating_sub(1)),
        (None, Some(n)) if a.is_empty() && n > 0 => (len.saturating_sub(n), len.saturating_sub(1)),
        _ => return None,
    };
    Some((r.0 < len).then_some(r))
}

/// A plain-text error.
pub fn error(w: &mut TcpStream, code: u16, msg: &str, extra: &[(&str, &str)]) {
    let mut h = vec![("Content-Type", "text/plain; charset=utf-8"), ("X-Content-Type-Options", "nosniff")];
    h.extend_from_slice(extra);
    respond(w, false, code, &h, format!("{msg}\n").as_bytes());
}

/// Listens on `addr`: `:8130` is every interface, IPv6 and IPv4.
pub fn listen(addr: &str) -> io::Result<std::net::TcpListener> {
    use std::net::{SocketAddr, TcpListener, ToSocketAddrs};
    if let Some(port) = addr.strip_prefix(':') {
        return TcpListener::bind(format!("[::]:{port}"))
            .or_else(|_| TcpListener::bind(format!("0.0.0.0:{port}")));
    }
    let addrs: Vec<SocketAddr> = addr.to_socket_addrs()?.collect();
    TcpListener::bind(&addrs[..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(host: &str, origin: &str) -> Request {
        let mut headers = vec![("Host".to_string(), host.to_string())];
        if !origin.is_empty() {
            headers.push(("Origin".to_string(), origin.to_string()));
        }
        Request { method: "GET".into(), path: "/ws".into(), query: String::new(), headers, body: Vec::new() }
    }

    #[test]
    fn same_site_passes_the_page_and_the_lan() {
        for (host, origin) in [
            ("localhost:8080", "http://localhost:8080"),
            ("127.0.0.1:8080", "http://127.0.0.1:8080"),
            ("192.168.1.20:8080", "http://192.168.1.20:8080"),
            ("[::1]:8080", "http://[::1]:8080"),
            ("studio.local:8080", "http://Studio.local:8080"),
            ("studio:8080", "http://studio:8080"),
            ("box.home.arpa", "http://box.home.arpa"),
            ("studio.fritz.box:8080", "http://studio.fritz.box:8080"),
            ("localhost:8080", ""), // not a browser, or a browser's own GET
            ("x", ""),
            ("", ""),
        ] {
            assert!(same_site(&req(host, origin)), "{host} {origin}");
        }
    }

    #[test]
    fn same_site_refuses_other_sites_and_rebinding() {
        for (host, origin) in [
            ("localhost:8080", "http://evil.example"),
            ("localhost:8080", "http://localhost:9090"),
            ("localhost:8080", "null"),
            ("localhost:8080", "https://localhost:8080"),
            ("evil.example:8080", "http://evil.example:8080"), // DNS rebinding
            ("evil.example:8080", ""),
            ("[not-an-address]:8080", ""),
        ] {
            assert!(!same_site(&req(host, origin)), "{host} {origin}");
        }
    }

    #[test]
    fn ranges() {
        assert_eq!(range("", 100), None);
        assert_eq!(range("bytes=0-", 100), Some(Some((0, 99))));
        assert_eq!(range("bytes=10-19", 100), Some(Some((10, 19))));
        assert_eq!(range("bytes=90-200", 100), Some(Some((90, 99))));
        assert_eq!(range("bytes=-10", 100), Some(Some((90, 99))));
        assert_eq!(range("bytes=100-", 100), Some(None));
        assert_eq!(range("bytes=0-1,5-6", 100), None);
        assert_eq!(range("items=0-1", 100), None);
    }

    #[test]
    fn same_site_knows_this_machine_by_name() {
        let me = std::fs::read_to_string("/proc/sys/kernel/hostname").unwrap();
        let host = format!("{}.example.org:8080", me.trim());
        assert!(same_site(&req(&host, &format!("http://{host}"))), "{host}");
    }
}
