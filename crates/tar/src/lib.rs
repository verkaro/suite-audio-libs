//! A tar written and read as it streams (POSIX ustar), for packing a tool's work to archive it
//! or move it between machines (BRR's packs, step 14). Written fresh. A path too long for
//! ustar's fields, or a file of 8 GiB or more, gets a PAX record before it, as GNU and BSD tar
//! read them; reading takes PAX's `path` and `size`, and GNU's long names. Folders and regular
//! files only: links and devices are read as [`Kind::Other`], to be skipped.
//!
//! ```
//! let mut t = as_tar::Writer::new(Vec::new());
//! t.dir("pack/", 0).unwrap();
//! t.file("pack/hello.txt", 5, 0, &mut &b"hello"[..]).unwrap();
//! let bytes = t.finish().unwrap();
//! let mut r = as_tar::Reader::new(&bytes[..]);
//! let e = r.next_entry().unwrap().unwrap();
//! assert_eq!((e.path.as_str(), e.kind), ("pack/", as_tar::Kind::Dir));
//! let e = r.next_entry().unwrap().unwrap();
//! let mut out = Vec::new();
//! r.copy(&e, &mut out).unwrap();
//! assert_eq!(out, b"hello");
//! ```

use std::io::{self, Read, Write};

const BLOCK: usize = 512;
/// The largest size ustar's 11 octal digits hold.
const MAX_OCTAL: u64 = (1 << 33) - 1;

/// What an entry is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    File,
    Dir,
    Other,
}

/// An entry's header, as read.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub path: String,
    pub kind: Kind,
    pub size: u64,
    pub mtime: u64,
}

/// A tar being written to `W`.
pub struct Writer<W: Write> {
    w: W,
}

fn octal(field: &mut [u8], v: u64) {
    let n = field.len() - 1;
    let s = format!("{v:0n$o}");
    field[..n].copy_from_slice(s.as_bytes());
    field[n] = 0;
}

fn header(name: &[u8], prefix: &[u8], kind: u8, size: u64, mtime: u64) -> [u8; BLOCK] {
    let mut h = [0u8; BLOCK];
    h[..name.len()].copy_from_slice(name);
    octal(&mut h[100..108], if kind == b'5' { 0o755 } else { 0o644 });
    octal(&mut h[108..116], 0);
    octal(&mut h[116..124], 0);
    octal(&mut h[124..136], size.min(MAX_OCTAL));
    octal(&mut h[136..148], mtime.min(MAX_OCTAL));
    h[156] = kind;
    h[257..263].copy_from_slice(b"ustar\0");
    h[263..265].copy_from_slice(b"00");
    h[345..345 + prefix.len()].copy_from_slice(prefix);
    h[148..156].fill(b' ');
    let sum: u32 = h.iter().map(|&b| b as u32).sum();
    let s = format!("{sum:06o}\0 ");
    h[148..156].copy_from_slice(s.as_bytes());
    h
}

/// `path` as ustar's name and prefix, if it fits them.
fn split(path: &[u8]) -> Option<(&[u8], &[u8])> {
    if path.len() <= 100 {
        return Some((path, &[]));
    }
    (0..path.len()).rev().filter(|&i| path[i] == b'/').find_map(|i| {
        let (prefix, name) = (&path[..i], &path[i + 1..]);
        (prefix.len() <= 155 && name.len() <= 100 && !name.is_empty()).then_some((name, prefix))
    })
}

/// A PAX record, `LEN key=value\n`, its length counting itself.
fn record(key: &str, value: &str) -> String {
    let rest = key.len() + value.len() + 3;
    let mut n = rest + 1;
    while n.to_string().len() + rest != n {
        n = n.to_string().len() + rest;
    }
    format!("{n} {key}={value}\n")
}

impl<W: Write> Writer<W> {
    pub fn new(w: W) -> Writer<W> {
        Writer { w }
    }

    fn entry(&mut self, path: &str, kind: u8, size: u64, mtime: u64) -> io::Result<()> {
        let b = path.as_bytes();
        let fits = split(b);
        let mut pax = String::new();
        if fits.is_none() {
            pax += &record("path", path);
        }
        if size > MAX_OCTAL {
            pax += &record("size", &size.to_string());
        }
        if !pax.is_empty() {
            let h = header(b"././@PaxHeader", &[], b'x', pax.len() as u64, mtime);
            self.w.write_all(&h)?;
            self.w.write_all(pax.as_bytes())?;
            self.pad(pax.len() as u64)?;
        }
        let (name, prefix) = fits.unwrap_or((&b[b.len().saturating_sub(100)..], &[]));
        self.w.write_all(&header(name, prefix, kind, size, mtime))
    }

    fn pad(&mut self, size: u64) -> io::Result<()> {
        let n = (BLOCK - (size % BLOCK as u64) as usize) % BLOCK;
        self.w.write_all(&[0u8; BLOCK][..n])
    }

    /// A folder (its path ends in `/`).
    pub fn dir(&mut self, path: &str, mtime: u64) -> io::Result<()> {
        let p = if path.ends_with('/') { path.to_string() } else { format!("{path}/") };
        self.entry(&p, b'5', 0, mtime)
    }

    /// A file of `size` bytes, read from `data` as it's written. Short of its size, an error.
    pub fn file(&mut self, path: &str, size: u64, mtime: u64, data: &mut impl Read) -> io::Result<()> {
        self.entry(path, b'0', size, mtime)?;
        let n = io::copy(&mut data.take(size), &mut self.w)?;
        if n != size {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, format!("{path}: {n} bytes of {size}")));
        }
        self.pad(size)
    }

    /// The two empty blocks that end a tar; the writer handed back.
    pub fn finish(mut self) -> io::Result<W> {
        self.w.write_all(&[0u8; 2 * BLOCK])?;
        self.w.flush()?;
        Ok(self.w)
    }
}

/// A tar being read from `R`, an entry at a time.
pub struct Reader<R: Read> {
    r: R,
    /// What's left of the last entry's data and padding, unread.
    left: u64,
}

fn bad(m: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, m.into())
}

fn parse_octal(f: &[u8]) -> io::Result<u64> {
    let s: String = f.iter().take_while(|&&b| b != 0).map(|&b| b as char).collect();
    let s = s.trim();
    if s.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(s, 8).map_err(|_| bad(format!("a tar header's number: {s:?}")))
}

fn text(f: &[u8]) -> String {
    String::from_utf8_lossy(&f[..f.iter().position(|&b| b == 0).unwrap_or(f.len())]).into_owned()
}

impl<R: Read> Reader<R> {
    pub fn new(r: R) -> Reader<R> {
        Reader { r, left: 0 }
    }

    fn block(&mut self) -> io::Result<Option<[u8; BLOCK]>> {
        let mut h = [0u8; BLOCK];
        let mut got = 0;
        while got < BLOCK {
            let n = self.r.read(&mut h[got..])?;
            if n == 0 {
                return if got == 0 { Ok(None) } else { Err(bad("a tar cut short")) };
            }
            got += n;
        }
        Ok(Some(h))
    }

    fn skip(&mut self) -> io::Result<()> {
        let n = io::copy(&mut (&mut self.r).take(self.left), &mut io::sink())?;
        if n != self.left {
            return Err(bad("a tar cut short"));
        }
        self.left = 0;
        Ok(())
    }

    fn data(&mut self, size: u64) -> io::Result<Vec<u8>> {
        let mut v = Vec::new();
        (&mut self.r).take(size).read_to_end(&mut v)?;
        if v.len() as u64 != size {
            return Err(bad("a tar cut short"));
        }
        self.left = (BLOCK as u64 - size % BLOCK as u64) % BLOCK as u64;
        self.skip()?;
        Ok(v)
    }

    /// The next entry (what was left of the last is skipped), or none at the tar's end.
    pub fn next_entry(&mut self) -> io::Result<Option<Entry>> {
        self.skip()?;
        let (mut path, mut size): (Option<String>, Option<u64>) = (None, None);
        loop {
            let Some(h) = self.block()? else { return Ok(None) };
            if h.iter().all(|&b| b == 0) {
                return Ok(None);
            }
            let want = parse_octal(&h[148..156])?;
            let sum: u64 =
                h.iter().enumerate().map(|(i, &b)| if (148..156).contains(&i) { 32 } else { b as u64 }).sum();
            if want != sum {
                return Err(bad("a tar header's checksum is wrong"));
            }
            let n = parse_octal(&h[124..136])?;
            match h[156] {
                b'x' => {
                    let pax = self.data(n)?;
                    for rec in String::from_utf8_lossy(&pax).lines() {
                        let Some((_, kv)) = rec.split_once(' ') else { continue };
                        match kv.split_once('=') {
                            Some(("path", v)) => path = Some(v.to_string()),
                            Some(("size", v)) => size = v.parse().ok(),
                            _ => {}
                        }
                    }
                }
                b'L' => {
                    let name = self.data(n)?;
                    path = Some(text(&name));
                }
                b'g' => {
                    self.data(n)?;
                }
                t => {
                    let ustar = &h[257..262] == b"ustar";
                    let name = text(&h[..100]);
                    let prefix = if ustar { text(&h[345..500]) } else { String::new() };
                    let whole = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
                    let size = size.unwrap_or(n);
                    let kind = match t {
                        b'0' | 0 | b'7' => Kind::File,
                        b'5' => Kind::Dir,
                        _ => Kind::Other,
                    };
                    let path = path.unwrap_or(whole);
                    let kind = if kind == Kind::File && path.ends_with('/') { Kind::Dir } else { kind };
                    let data = if kind == Kind::Dir { 0 } else { size };
                    self.left = data + (BLOCK as u64 - data % BLOCK as u64) % BLOCK as u64;
                    return Ok(Some(Entry { path, kind, size: data, mtime: parse_octal(&h[136..148])? }));
                }
            }
        }
    }

    /// The entry just read, its data written to `w`.
    pub fn copy(&mut self, e: &Entry, w: &mut impl Write) -> io::Result<u64> {
        let n = io::copy(&mut (&mut self.r).take(e.size), w)?;
        if n != e.size {
            return Err(bad(format!("{}: cut short", e.path)));
        }
        self.left -= n;
        self.skip()?;
        Ok(n)
    }
}

#[cfg(test)]
mod tests;
