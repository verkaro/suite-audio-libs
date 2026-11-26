//! A long file's overview, kept in a tool's own state folder (`../../RFC-waveform.md`,
//! question 2): keyed by the file's path, size and time, made the first time it's looked at, and
//! deletable with nothing lost. Never beside its source, and never over another file.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::UNIX_EPOCH;

use sha2::{Digest, Sha256};

use crate::{FilePicture, overview};

/// A cache file's first bytes. Moved when its layout or [`crate::picture`]'s numbers do, so an
/// old file is made again.
const MAGIC: &[u8; 8] = b"ASWAVE1\n";
/// The magic, the key, the rate and the frames.
const HEAD: usize = 8 + 32 + 4 + 8;

/// An overview from the cache, or made now.
#[derive(Debug)]
pub struct Cached {
    pub picture: FilePicture,
    /// It was read from the cache.
    pub found: bool,
    /// Why one made now wasn't kept: the cache can't be written, or the file changed while it
    /// was read. The picture is good either way.
    pub not_kept: Option<io::Error>,
}

/// What a cache file is of: the source's path, size and time, and the columns.
fn key(source: &Path, columns: usize) -> io::Result<[u8; 32]> {
    let m = fs::metadata(source)?;
    let time = m.modified()?.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let mut h = Sha256::new();
    let path = source.as_os_str().as_encoded_bytes();
    h.update((path.len() as u64).to_le_bytes());
    h.update(path);
    h.update(m.len().to_le_bytes());
    h.update(time.to_le_bytes());
    h.update((columns as u64).to_le_bytes());
    Ok(h.finalize().into())
}

fn file_in(cache: &Path, key: &[u8; 32]) -> PathBuf {
    let name: String = key[..16].iter().map(|b| format!("{b:02x}")).collect();
    cache.join(format!("{name}.wave"))
}

fn parse(bytes: &[u8], key: &[u8; 32], columns: usize) -> Option<FilePicture> {
    if bytes.len() != HEAD + 8 * columns || &bytes[..8] != MAGIC || &bytes[8..40] != key {
        return None;
    }
    let rate = u32::from_le_bytes(bytes[40..44].try_into().ok()?);
    let frames = u64::from_le_bytes(bytes[44..52].try_into().ok()?);
    let mut numbers = bytes[HEAD..].as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b));
    let lo = numbers.by_ref().take(columns).collect();
    let hi = numbers.collect();
    Some(FilePicture { rate, frames, lo, hi })
}

fn bytes(p: &FilePicture, key: &[u8; 32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEAD + 8 * p.lo.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(key);
    out.extend_from_slice(&p.rate.to_le_bytes());
    out.extend_from_slice(&p.frames.to_le_bytes());
    for v in p.lo.iter().chain(&p.hi) {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Writes under a name of its own, then takes the cache file's, only if it's free.
fn keep(file: &Path, content: &[u8]) -> io::Result<()> {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let tmp =
        file.with_extension(format!("new-{}-{}", std::process::id(), COUNT.fetch_add(1, Ordering::Relaxed)));
    let done = (|| {
        as_keep::create_new(&tmp)?.write_all(content)?;
        as_keep::rename_new(&tmp, file)
    })();
    if done.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    match done {
        // Another made it meanwhile.
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        done => done,
    }
}

/// [`overview`] of `source`, kept in the folder `cache` (made if need be): read from there if
/// it was made before from the file as it is now. `cache` is the tool's own, in its state
/// folder: the source's own folder is refused.
pub fn cached_overview(
    source: impl AsRef<Path>,
    cache: impl AsRef<Path>,
    columns: usize,
) -> io::Result<Cached> {
    let (source, cache) = (source.as_ref(), cache.as_ref());
    let columns = columns.max(1);
    let source = source.canonicalize()?;
    if cache.canonicalize().ok().as_deref() == source.parent() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{}: an overview isn't kept beside its source", cache.display()),
        ));
    }
    let key_now = key(&source, columns)?;
    let file = file_in(cache, &key_now);
    let held = fs::read(&file).ok();
    if let Some(picture) = held.as_deref().and_then(|b| parse(b, &key_now, columns)) {
        return Ok(Cached { picture, found: true, not_kept: None });
    }
    let picture = overview(&source, columns)?;
    let kept = (|| {
        if key(&source, columns)? != key_now {
            return Err(io::Error::other("the file changed while it was read"));
        }
        fs::create_dir_all(cache)?;
        // One that's there and didn't read is a cache file gone bad: made again.
        if held.is_some() {
            fs::remove_file(&file)?;
        }
        keep(&file, &bytes(&picture, &key_now))
    })();
    Ok(Cached { picture, found: false, not_kept: kept.err() })
}
