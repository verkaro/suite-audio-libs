//! A FLAC's tags (moved into the suite from dexter-foley's `foley-tags`, 2026-11-09,
//! `VENDORED.md`): its Vorbis comments read, and rewritten so a change never risks the audio. A new file is written beside the old: every
//! metadata block but the comments as it was, the new comments, and the audio copied byte for
//! byte. It's read back and checked against the old, the change is appended to the journal,
//! and only then is it renamed over the old. The format: https://xiph.org/flac/format.html.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

mod journal;
#[cfg(test)]
mod tests;

pub use journal::{Change, Entry, How, append as journal_append, read as journal_read, utc};

const STREAMINFO: u8 = 0;
const VORBIS_COMMENT: u8 = 4;

/// A file's Vorbis comments: the encoder's vendor string, and each field in the file's order.
/// A field can be there more than once (KEYWORDS); names are matched without regard to case.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tags {
    pub vendor: String,
    pub fields: Vec<(String, String)>,
}

impl Tags {
    /// Every value of `key`, in order.
    pub fn get(&self, key: &str) -> Vec<&str> {
        self.fields.iter().filter(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str()).collect()
    }

    /// Replaces every value of `key` with `values` (none: it's removed), where its first value was,
    /// or at the end. The name is written in capitals.
    pub fn set(&mut self, key: &str, values: &[String]) {
        // Nothing before the first value is removed, so it's still where the new ones go.
        let at =
            self.fields.iter().position(|(k, _)| k.eq_ignore_ascii_case(key)).unwrap_or(self.fields.len());
        self.fields.retain(|(k, _)| !k.eq_ignore_ascii_case(key));
        let key = key.to_ascii_uppercase();
        let new = values.iter().map(|v| (key.clone(), v.clone()));
        self.fields.splice(at..at, new);
    }

    /// The block's body: lengths little-endian, each string's before it.
    fn encode(&self) -> Vec<u8> {
        let mut b = vec![];
        let put = |b: &mut Vec<u8>, s: &[u8]| {
            b.extend_from_slice(&(s.len() as u32).to_le_bytes());
            b.extend_from_slice(s);
        };
        put(&mut b, self.vendor.as_bytes());
        b.extend_from_slice(&(self.fields.len() as u32).to_le_bytes());
        for (k, v) in &self.fields {
            put(&mut b, format!("{k}={v}").as_bytes());
        }
        b
    }

    fn decode(body: &[u8]) -> io::Result<Tags> {
        let mut at = 0usize;
        let mut take = |n: usize| -> io::Result<&[u8]> {
            let s = body
                .get(at..at.saturating_add(n))
                .ok_or_else(|| bad("its Vorbis comments run past their block"))?;
            at += n;
            Ok(s)
        };
        let len = |b: &[u8]| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize;
        let n = len(take(4)?);
        let vendor = utf8(take(n)?)?;
        let count = len(take(4)?);
        let mut fields = vec![];
        for _ in 0..count {
            let n = len(take(4)?);
            let f = utf8(take(n)?)?;
            let (k, v) = f.split_once('=').ok_or_else(|| bad("a Vorbis comment has no ="))?;
            fields.push((k.to_string(), v.to_string()));
        }
        Ok(Tags { vendor, fields })
    }
}

/// Whether `key` can name a Vorbis comment: printable ASCII but `=`.
pub fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.bytes().all(|b| (0x20..=0x7D).contains(&b) && b != b'=')
}

fn utf8(b: &[u8]) -> io::Result<String> {
    String::from_utf8(b.to_vec()).map_err(|_| bad("a Vorbis comment isn't UTF-8"))
}

fn bad(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why.to_string())
}

/// A metadata block: its type, and its body as it was.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Block {
    kind: u8,
    body: Vec<u8>,
}

/// What's read of a FLAC before its audio: its blocks, and where the audio starts.
struct Head {
    blocks: Vec<Block>,
    audio_at: u64,
}

impl Head {
    fn read(r: &mut impl Read) -> io::Result<Head> {
        let mut magic = [0u8; 4];
        r.read_exact(&mut magic).map_err(|_| bad("it isn't a FLAC file"))?;
        if &magic[..3] == b"ID3" {
            return Err(bad("it starts with an ID3 tag, which FLAC files shouldn't have"));
        }
        if &magic != b"fLaC" {
            return Err(bad("it isn't a FLAC file"));
        }
        let mut blocks = vec![];
        let mut at = 4u64;
        loop {
            let mut h = [0u8; 4];
            r.read_exact(&mut h).map_err(|_| bad("its metadata ends early"))?;
            let kind = h[0] & 0x7F;
            if kind == 127 {
                return Err(bad("it has an invalid metadata block"));
            }
            let len = u32::from_be_bytes([0, h[1], h[2], h[3]]) as usize;
            let mut body = vec![0u8; len];
            r.read_exact(&mut body).map_err(|_| bad("its metadata ends early"))?;
            at += 4 + len as u64;
            blocks.push(Block { kind, body });
            if h[0] & 0x80 != 0 {
                break;
            }
        }
        if blocks[0].kind != STREAMINFO || blocks[0].body.len() != 34 {
            return Err(bad("its first block isn't STREAMINFO"));
        }
        if blocks.iter().filter(|b| b.kind == VORBIS_COMMENT).count() > 1 {
            return Err(bad("it has more than one block of Vorbis comments"));
        }
        Ok(Head { blocks, audio_at: at })
    }

    fn tags(&self) -> io::Result<Tags> {
        match self.blocks.iter().find(|b| b.kind == VORBIS_COMMENT) {
            Some(b) => Tags::decode(&b.body),
            None => Ok(Tags::default()),
        }
    }

    /// The blocks with `comments` in place of the old (or after STREAMINFO, if there were none).
    fn with(&self, comments: Vec<u8>) -> Vec<Block> {
        let mut blocks = self.blocks.clone();
        let new = Block { kind: VORBIS_COMMENT, body: comments };
        match blocks.iter().position(|b| b.kind == VORBIS_COMMENT) {
            Some(i) => blocks[i] = new,
            None => blocks.insert(1, new),
        }
        blocks
    }
}

/// `fLaC` and the blocks, the last marked so.
fn head_bytes(blocks: &[Block]) -> Vec<u8> {
    let mut out = b"fLaC".to_vec();
    for (i, b) in blocks.iter().enumerate() {
        let last = if i + 1 == blocks.len() { 0x80 } else { 0 };
        let len = (b.body.len() as u32).to_be_bytes();
        out.extend_from_slice(&[last | b.kind, len[1], len[2], len[3]]);
        out.extend_from_slice(&b.body);
    }
    out
}

/// A FLAC stream in memory with `tags` as its comments, in place of any it had: every other
/// block and the audio as they were (import, of a stream just encoded or a FLAC copied).
pub fn with_tags(flac: &[u8], tags: &Tags) -> io::Result<Vec<u8>> {
    let head = Head::read(&mut &flac[..])?;
    let comments = tags.encode();
    if comments.len() >= 1 << 24 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "the tags are too long for one block"));
    }
    let mut out = head_bytes(&head.with(comments));
    out.extend_from_slice(&flac[head.audio_at as usize..]);
    Ok(out)
}

/// A FLAC's tags.
pub fn read(path: impl AsRef<Path>) -> io::Result<Tags> {
    Head::read(&mut BufReader::new(File::open(path)?))?.tags()
}

/// Where a write has got to, for the tests to interrupt it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    /// The new file's metadata is written, its audio not yet.
    Metadata,
    /// The new file is written and checked, and the journal's entry is appended.
    Journalled,
}

/// Sets each field in `edits` (its values replacing the old; none removes it), as one change. If
/// anything changes, the file is rewritten safely and the change appended to `journal` before
/// the new file replaces the old. What changed is returned; nothing, if nothing did. No field is
/// refused for what it's called: a tool with fields of its own (a serial) checks before it calls.
pub fn set(
    path: impl AsRef<Path>,
    edits: &[(String, Vec<String>)],
    journal: &Path,
) -> io::Result<Vec<Change>> {
    set_as(path, edits, journal, &How::new("set"))
}

/// [`set`], journalled as `how`.
pub fn set_as(
    path: impl AsRef<Path>,
    edits: &[(String, Vec<String>)],
    journal: &Path,
    how: &How,
) -> io::Result<Vec<Change>> {
    set_with(path.as_ref(), edits, journal, how, &mut |_| Ok(()))
}

fn set_with(
    path: &Path,
    edits: &[(String, Vec<String>)],
    journal: &Path,
    how: &How,
    stage: &mut dyn FnMut(Stage) -> io::Result<()>,
) -> io::Result<Vec<Change>> {
    for (k, _) in edits {
        if !valid_key(k) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("{k:?} can't name a tag")));
        }
    }
    let mut f = File::open(path)?;
    let seen = f.metadata()?;
    let head = Head::read(&mut BufReader::new(&mut f))?;
    let old = head.tags()?;
    let mut new = old.clone();
    let mut changes = vec![];
    for (k, values) in edits {
        new.set(k, values);
    }
    let mut keys: Vec<String> = vec![];
    for (k, _) in edits {
        let k = k.to_ascii_uppercase();
        if !keys.contains(&k) {
            keys.push(k);
        }
    }
    for k in keys {
        let (before, after) = (old.get(&k), new.get(&k));
        if before != after {
            let own = |v: Vec<&str>| v.into_iter().map(String::from).collect();
            changes.push(Change { field: k, before: own(before), after: own(after) });
        }
    }
    if changes.is_empty() {
        return Ok(changes);
    }
    let comments = new.encode();
    if comments.len() >= 1 << 24 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "the tags are too long for one block"));
    }
    let blocks = head.with(comments);
    let tmp = temp_beside(path)?;
    let done = (|| {
        write_new(tmp.0, &blocks, &mut f, head.audio_at, stage)?;
        check(&tmp.1, &blocks, &mut f, head.audio_at)?;
        let now = fs::metadata(path)?;
        if now.len() != seen.len() || now.modified()? != seen.modified()? {
            return Err(io::Error::other(
                "it changed while its tags were being written; nothing was changed",
            ));
        }
        fs::set_permissions(&tmp.1, seen.permissions())?;
        let serial = old.get("SERIAL").first().map(|s| s.to_string());
        journal::append(journal, &Entry::new(path, serial, how.clone(), changes.clone()))?;
        stage(Stage::Journalled)?;
        fs::rename(&tmp.1, path)?;
        File::open(path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new(".")))?.sync_all()
    })();
    if let Err(e) = done {
        // The new file is ours, half made or unchecked; the old one is as it was.
        let _ = fs::remove_file(&tmp.1);
        return Err(e);
    }
    Ok(changes)
}

/// A new, empty file beside `path` for its replacement: `.NAME.tags-N`, hidden, never one that's
/// there already.
fn temp_beside(path: &Path) -> io::Result<(File, PathBuf)> {
    let name = path.file_name().ok_or_else(|| bad("it has no file name"))?.to_string_lossy();
    let dir = path.parent().unwrap_or(Path::new(""));
    let start = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.subsec_nanos()) as u64;
    for n in 0..1000u64 {
        let p = dir.join(format!(".{name}.tags-{}-{}", std::process::id(), start + n));
        match OpenOptions::new().write(true).create_new(true).open(&p) {
            Ok(f) => return Ok((f, p)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::other("no name free for the new file"))
}

fn write_new(
    out: File,
    blocks: &[Block],
    old: &mut File,
    audio_at: u64,
    stage: &mut dyn FnMut(Stage) -> io::Result<()>,
) -> io::Result<()> {
    let mut w = BufWriter::with_capacity(256 << 10, out);
    w.write_all(&head_bytes(blocks))?;
    w.flush()?;
    stage(Stage::Metadata)?;
    old.seek(SeekFrom::Start(audio_at))?;
    io::copy(old, &mut w)?;
    let f = w.into_inner().map_err(|e| e.into_error())?;
    f.sync_all()
}

/// The new file read back: its blocks the ones meant, and its audio the old's, byte for byte.
fn check(tmp: &Path, blocks: &[Block], old: &mut File, audio_at: u64) -> io::Result<()> {
    let mut r = BufReader::with_capacity(256 << 10, File::open(tmp)?);
    let head = Head::read(&mut r)?;
    if head.blocks != blocks {
        return Err(io::Error::other("the new file's metadata isn't what was written; nothing was changed"));
    }
    old.seek(SeekFrom::Start(audio_at))?;
    let mut a = BufReader::with_capacity(256 << 10, old);
    let (mut x, mut y) = (vec![0u8; 64 << 10], vec![0u8; 64 << 10]);
    loop {
        let n = fill(&mut a, &mut x)?;
        let m = fill(&mut r, &mut y)?;
        if n != m || x[..n] != y[..m] {
            return Err(io::Error::other("the new file's audio isn't the old's; nothing was changed"));
        }
        if n == 0 {
            return Ok(());
        }
    }
}

/// Reads until `buf` is full or the file ends.
fn fill(r: &mut impl Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}
