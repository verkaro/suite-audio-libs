//! A waveform's numbers: for any span of a sound and any number of columns, the lowest and
//! highest sample in each. From memory ([`picture`], with an IR's decay if asked), from a WAV a
//! block at a time ([`file_picture`]), and a whole file at a coarse resolution
//! ([`overview`]), kept in a tool's cache if it likes ([`cached_overview`]). Numbers only:
//! whoever draws it chooses straight or dB. For looking, never for playing.
//!
//! `picture` is dexter-irma's `core/src/wave.rs` at `4fc58b4`, moved in 2026-11-25
//! (`../../RFC-waveform.md`, w1; `../../VENDORED.md`); the rest is written here. It's drawn by
//! `lp-paint`'s waveform on a panel and by `as-page-kit`'s `kitWave` on a page.
//!
//! # The message
//!
//! A tool's server pictures for its clients by these names, and its `page-protocol.md` says the
//! rest: the units of `from` and `to` (irma's are ms), what else it sends, and when.
//!
//! - **`view`**, from a client: `from`, `to`, `columns` (up to [`MAX_COLUMNS`]). The span to
//!   picture, which may run past the sound's ends.
//! - **`wave`**, the answer: `from`, `to` (the span pictured), `lo`, `hi` (one number a column,
//!   full scale 1), and, for a view with an overview strip, `whole`: the picture of the whole
//!   sound, as `from`, `to` (where it starts and ends, in the same units), `lo` and `hi`. A tool
//!   may send `whole` only when the client hasn't the one that goes with this sound; the client
//!   keeps the last it had.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

mod cache;
pub use cache::{Cached, cached_overview};

#[cfg(test)]
mod tests;

/// The most columns a `view` asks for.
pub const MAX_COLUMNS: usize = 4000;
/// An overview's columns, where a tool has no reason for another number: more than a screen is
/// wide, and a small message.
pub const OVERVIEW_COLUMNS: usize = 2000;
/// The frames read from a file at a time.
const BLOCK: usize = 1 << 16;

/// One span of a sound, a column at a time.
#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    /// The lowest and highest sample in each column, every channel together. 0 where the span
    /// has no sound.
    pub lo: Vec<f32>,
    pub hi: Vec<f32>,
    /// An IR's decay at each column's start, dB: the energy still to come over the energy from
    /// the onset on. None before the onset, and past the sound's end. Empty when no onset was
    /// given.
    pub decay: Vec<Option<f32>>,
}

/// The picture of `c` (one `Vec` a channel, as long as each other) from frame `from` to `to`,
/// which may run past its ends (nothing is there), in `columns` columns. `onset`, an IR's, asks
/// for its decay counted from there; that takes the whole sound, whatever the span.
pub fn picture(c: &[Vec<f32>], from: f64, to: f64, columns: usize, onset: Option<usize>) -> Picture {
    let frames = c.first().map_or(0, Vec::len);
    let mut cols = Columns::new(frames, from, to, columns);
    cols.feed(0, c);
    let decay = onset.map_or_else(Vec::new, |onset| decay(c, &cols.ranges, onset));
    let (lo, hi) = cols.finish();
    Picture { lo, hi, decay }
}

/// Schroeder's integral, backwards, read at each column's first frame.
fn decay(c: &[Vec<f32>], ranges: &[Option<(usize, usize)>], onset: usize) -> Vec<Option<f32>> {
    let frames = c.first().map_or(0, Vec::len);
    // The energy from each frame on.
    let mut left = vec![0.0f64; frames + 1];
    for k in (0..frames).rev() {
        left[k] = left[k + 1] + c.iter().map(|ch| (ch[k] as f64).powi(2)).sum::<f64>();
    }
    let whole = left.get(onset.min(frames)).copied().unwrap_or(0.0);
    ranges
        .iter()
        .map(|r| {
            let (first, _) = (*r)?;
            (first >= onset && whole > 0.0 && left[first] > 0.0)
                .then(|| (10.0 * (left[first] / whole).log10()) as f32)
        })
        .collect()
}

/// The columns of a span being filled: the one place a column's frames are worked out, so a
/// sound read a block at a time pictures as it does whole.
struct Columns {
    /// Each column's frames, `first..last`; None where there's no sound. Neither end ever goes
    /// back from one column to the next.
    ranges: Vec<Option<(usize, usize)>>,
    lo: Vec<f32>,
    hi: Vec<f32>,
    /// The first column a later block can still reach.
    next: usize,
}

impl Columns {
    fn new(frames: usize, from: f64, to: f64, columns: usize) -> Columns {
        let columns = columns.max(1);
        let (from, to) = (from.min(to), to.max(from));
        let width = (to - from).max(1e-9) / columns as f64;
        let ranges = (0..columns)
            .map(|col| {
                if !from.is_finite() || !width.is_finite() {
                    return None;
                }
                let a = from + col as f64 * width;
                let b = a + width;
                // The frames whose place falls in the column: at least the one it starts on, so
                // a zoom past one frame a column still draws.
                let first = a.floor().max(0.0) as usize;
                let last = (b.ceil().max(0.0) as usize).max(first.saturating_add(1)).min(frames);
                (!(a < 0.0 && b <= 0.0 || first >= frames)).then_some((first, last))
            })
            .collect();
        Columns { ranges, lo: vec![f32::MAX; columns], hi: vec![f32::MIN; columns], next: 0 }
    }

    /// The frames the columns need, `first..last`; None when the span has no sound.
    fn needs(&self) -> Option<(usize, usize)> {
        let mut with = self.ranges.iter().flatten();
        let first = *with.next()?;
        Some((first.0, with.last().map_or(first.1, |r| r.1)))
    }

    /// Takes a block, whose first frame is `pos`. Blocks come in order.
    fn feed(&mut self, pos: usize, block: &[Vec<f32>]) {
        let end = pos + block.first().map_or(0, Vec::len);
        while self.next < self.ranges.len() && self.ranges[self.next].is_none_or(|(_, last)| last <= pos) {
            self.next += 1;
        }
        for col in self.next..self.ranges.len() {
            let Some((first, last)) = self.ranges[col] else { continue };
            if first >= end {
                break;
            }
            let (a, b) = (first.max(pos) - pos, last.min(end) - pos);
            for ch in block {
                for v in &ch[a..b] {
                    self.lo[col] = self.lo[col].min(*v);
                    self.hi[col] = self.hi[col].max(*v);
                }
            }
        }
    }

    /// The lowest and highest of each column; 0 where nothing came.
    fn finish(mut self) -> (Vec<f32>, Vec<f32>) {
        for (l, h) in self.lo.iter_mut().zip(&mut self.hi) {
            if *l > *h {
                (*l, *h) = (0.0, 0.0);
            }
        }
        (self.lo, self.hi)
    }
}

/// A picture of part of a file, or of all of it, with what the file is.
#[derive(Clone, Debug, PartialEq)]
pub struct FilePicture {
    pub rate: u32,
    /// The file's length, in frames.
    pub frames: u64,
    pub lo: Vec<f32>,
    pub hi: Vec<f32>,
}

fn is_flac(path: &Path) -> io::Result<bool> {
    let mut magic = [0u8; 4];
    let n = File::open(path)?.read(&mut magic)?;
    Ok(n == 4 && &magic == b"fLaC")
}

/// The picture of the WAV at `path` from frame `from` to `to`, in `columns` columns: what
/// [`picture`] gives for the same sound in memory, its samples as `f32`. Only the span's frames
/// are read, a block at a time. A FLAC is refused ([`io::ErrorKind::Unsupported`]): a span of
/// one waits on seeking in `as-flac`, since without it a close window would decode the whole
/// file up to it. [`overview`] reads either.
pub fn file_picture(path: impl AsRef<Path>, from: f64, to: f64, columns: usize) -> io::Result<FilePicture> {
    let path = path.as_ref();
    if is_flac(path)? {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("{}: a span of a FLAC can't be read yet (as-flac has no seek)", path.display()),
        ));
    }
    wav_picture(path, Some((from, to)), columns, BLOCK).map(|p| p.0)
}

/// The picture of the whole of the WAV or FLAC at `path`, in `columns` columns: the strip that
/// shows where a window is. It reads the file through, a block at a time, so for a long one keep
/// it ([`cached_overview`]). A FLAC that doesn't say its length is refused.
pub fn overview(path: impl AsRef<Path>, columns: usize) -> io::Result<FilePicture> {
    let path = path.as_ref();
    if is_flac(path)? {
        flac_overview(path, columns, BLOCK)
    } else {
        wav_picture(path, None, columns, BLOCK).map(|p| p.0)
    }
}

fn single(v: &[Vec<f64>]) -> Vec<Vec<f32>> {
    v.iter().map(|c| c.iter().map(|&s| s as f32).collect()).collect()
}

/// A WAV's span (the whole file for None), `block` frames at a time; and the frames read.
fn wav_picture(
    path: &Path,
    span: Option<(f64, f64)>,
    columns: usize,
    block: usize,
) -> io::Result<(FilePicture, u64)> {
    let mut r = as_wav::Reader::open_any(path)?;
    let (rate, frames) = (r.rate(), r.frames());
    let (from, to) = span.unwrap_or((0.0, frames as f64));
    let mut cols = Columns::new(frames as usize, from, to, columns);
    let mut read = 0;
    if let Some((mut pos, end)) = cols.needs() {
        let mut buf: Vec<Vec<f64>> = vec![Vec::new(); r.channels() as usize];
        while pos < end {
            buf.iter_mut().for_each(Vec::clear);
            let got = r.read_f64_at(pos as u64, block.min(end - pos), &mut buf)?;
            if got == 0 {
                break;
            }
            cols.feed(pos, &single(&buf));
            pos += got;
            read += got as u64;
        }
    }
    let (lo, hi) = cols.finish();
    Ok((FilePicture { rate, frames, lo, hi }, read))
}

fn flac_overview(path: &Path, columns: usize, block: usize) -> io::Result<FilePicture> {
    let bad =
        |e: claxon::Error| io::Error::new(io::ErrorKind::InvalidData, format!("{}: {e}", path.display()));
    let mut r = as_flac::Frames::open(path).map_err(bad)?;
    let info = r.info();
    if info.frames == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: it doesn't say its length", path.display()),
        ));
    }
    let mut cols = Columns::new(info.frames as usize, 0.0, info.frames as f64, columns);
    let mut buf: Vec<Vec<f64>> = vec![Vec::new(); info.channels as usize];
    let mut pos = 0;
    loop {
        buf.iter_mut().for_each(Vec::clear);
        let got = r.read(block, &mut buf).map_err(bad)?;
        if got == 0 || pos >= info.frames as usize {
            break;
        }
        // No further than the length it said.
        let keep = got.min(info.frames as usize - pos);
        buf.iter_mut().for_each(|c| c.truncate(keep));
        cols.feed(pos, &single(&buf));
        pos += got;
    }
    let (lo, hi) = cols.finish();
    Ok(FilePicture { rate: info.rate, frames: info.frames, lo, hi })
}
