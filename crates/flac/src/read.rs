//! FLAC read, through `claxon` (written for the suite, 2026-10-29, at BRR's code review: a sound
//! read by more than one sibling is read the same way by all): any bit depth, any number of
//! channels, apart, as floats at 1.0 full scale.

use claxon::frame::FrameReader;
use claxon::input::ReadBytes;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

/// A FLAC read whole.
#[derive(Clone, Debug, PartialEq)]
pub struct Flac {
    pub rate: u32,
    pub bits: u32,
    /// One `Vec` a channel, all one length: a stream cut short mid-frame keeps its whole frames.
    pub channels: Vec<Vec<f32>>,
}

/// What a FLAC is, from its STREAMINFO alone, without reading its samples (added at BRR's
/// review, 2026-10-31: a library's listing had parsed the header itself).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Info {
    pub rate: u32,
    pub channels: u16,
    pub bits: u32,
    /// Frames (samples a channel), 0 when the stream doesn't say.
    pub frames: u64,
}

/// Reads only the STREAMINFO of the FLAC at `path`.
pub fn info(path: impl AsRef<Path>) -> Result<Info, claxon::Error> {
    let s = claxon::FlacReader::open(path)?.streaminfo();
    Ok(Info {
        rate: s.sample_rate,
        channels: s.channels as u16,
        bits: s.bits_per_sample,
        frames: s.samples.unwrap_or(0),
    })
}

/// Reads the FLAC at `path` whole.
pub fn read(path: impl AsRef<Path>) -> Result<Flac, claxon::Error> {
    let (rate, bits, channels) = read_as(path, |s, scale| (s as f64 * scale) as f32)?;
    Ok(Flac { rate, bits, channels })
}

/// A FLAC read whole, exactly: its integers over their full scale, as `f64` (for Tape, whose
/// processing is all `f64`; added 2026-11-01).
#[derive(Clone, Debug, PartialEq)]
pub struct Flac64 {
    pub rate: u32,
    pub bits: u32,
    pub channels: Vec<Vec<f64>>,
}

/// Reads the FLAC at `path` whole, exactly. `claxon` reads up to 24 bits a sample; a 32-bit
/// stream is refused (its frames say so in a way `claxon` takes for a reserved value).
pub fn read_f64(path: impl AsRef<Path>) -> Result<Flac64, claxon::Error> {
    let (rate, bits, channels) = read_as(path, |s, scale| s as f64 * scale)?;
    Ok(Flac64 { rate, bits, channels })
}

fn read_as<T>(
    path: impl AsRef<Path>,
    f: impl Fn(i32, f64) -> T,
) -> Result<(u32, u32, Vec<Vec<T>>), claxon::Error> {
    let mut r = claxon::FlacReader::open(path)?;
    let info = r.streaminfo();
    let ch = info.channels as usize;
    let scale = 1.0 / (1u64 << (info.bits_per_sample - 1)) as f64;
    let mut channels: Vec<Vec<T>> =
        (0..ch).map(|_| Vec::with_capacity(info.samples.unwrap_or(0) as usize)).collect();
    for (k, s) in r.samples().enumerate() {
        match s {
            Ok(s) => channels[k % ch].push(f(s, scale)),
            // Cut short: what's whole so far.
            Err(claxon::Error::IoError(_)) => break,
            Err(e) => return Err(e),
        }
    }
    let whole = channels.iter().map(Vec::len).min().unwrap_or(0);
    for c in &mut channels {
        c.truncate(whole);
    }
    Ok((info.sample_rate, info.bits_per_sample, channels))
}

/// Where a frame starts: its first sample, and its first byte in the file. Kept by [`Frames`] for
/// every frame it decodes, so a reader can be opened again there ([`Frames::open_at`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mark {
    pub frame: u64,
    pub byte: u64,
}

/// A FLAC read a frame at a time, exactly, as [`read_f64`] reads it whole: so a long file needs
/// no more memory than a frame, and can be read again from any frame it has passed (added for
/// Tape, 2026-11-01). A stream cut short mid-frame ends at its last whole frame.
pub struct Frames {
    info: Info,
    scale: f64,
    input: Tracked,
    /// The frame decoded last, its channels one after another, and how much of it is handed out.
    block: Vec<i32>,
    len: usize,
    used: usize,
    /// The next sample handed out.
    at: u64,
    marks: Vec<Mark>,
    done: bool,
}

impl Frames {
    /// Opens the FLAC at `path` at its first frame. 32-bit streams are refused, as by [`read_f64`].
    pub fn open(path: impl AsRef<Path>) -> Result<Frames, claxon::Error> {
        let path = path.as_ref();
        let found = info(path)?;
        let mut f = File::open(path)?;
        let start = audio_start(&mut f)?;
        Frames::at_byte(f, found, Mark { frame: 0, byte: start })
    }

    /// Opens the FLAC at `path` at a frame a reader of it marked.
    pub fn open_at(path: impl AsRef<Path>, mark: Mark) -> Result<Frames, claxon::Error> {
        let path = path.as_ref();
        let found = info(path)?;
        Frames::at_byte(File::open(path)?, found, mark)
    }

    fn at_byte(mut f: File, info: Info, mark: Mark) -> Result<Frames, claxon::Error> {
        f.seek(SeekFrom::Start(mark.byte))?;
        Ok(Frames {
            info,
            scale: 1.0 / (1u64 << (info.bits - 1)) as f64,
            input: Tracked { f, buf: vec![0; 1 << 16], pos: 0, len: 0, start: mark.byte },
            block: vec![],
            len: 0,
            used: 0,
            at: mark.frame,
            marks: vec![],
            done: false,
        })
    }

    /// What the stream is, from its STREAMINFO.
    pub fn info(&self) -> Info {
        self.info
    }

    /// The next sample [`Frames::read`] hands out.
    pub fn at(&self) -> u64 {
        self.at
    }

    /// Where each frame decoded so far starts, in order.
    pub fn marks(&self) -> &[Mark] {
        &self.marks
    }

    /// Up to `n` frames, appended to `outs` (one a channel). Returns how many: fewer than `n`
    /// only at the end of the stream.
    pub fn read(&mut self, n: usize, outs: &mut [Vec<f64>]) -> Result<usize, claxon::Error> {
        let mut got = 0;
        while got < n {
            if self.used == self.len && !self.next_frame()? {
                break;
            }
            let take = (n - got).min(self.len - self.used);
            for (c, o) in outs.iter_mut().enumerate() {
                let s = &self.block[c * self.len + self.used..][..take];
                o.extend(s.iter().map(|&v| v as f64 * self.scale));
            }
            (self.used, got, self.at) = (self.used + take, got + take, self.at + take as u64);
        }
        Ok(got)
    }

    /// Decodes the next frame; false at the end (or where the stream is cut short).
    fn next_frame(&mut self) -> Result<bool, claxon::Error> {
        if self.done {
            return Ok(false);
        }
        let byte = self.input.at();
        let buffer = std::mem::take(&mut self.block);
        match FrameReader::new(&mut self.input).read_next_or_eof(buffer) {
            Ok(Some(b)) => {
                if b.channels() != self.info.channels as u32 {
                    return Err(claxon::Error::FormatError("a frame's channels differ from the stream's"));
                }
                self.marks.push(Mark { frame: self.at, byte });
                self.len = b.duration() as usize;
                self.used = 0;
                self.block = b.into_buffer();
                Ok(true)
            }
            Ok(None) | Err(claxon::Error::IoError(_)) => {
                self.done = true;
                Ok(false)
            }
            Err(e) => Err(e),
        }
    }
}

/// Where the first frame starts: past `fLaC` and every metadata block.
fn audio_start(f: &mut File) -> io::Result<u64> {
    let mut h = [0u8; 4];
    f.read_exact(&mut h)?;
    if &h != b"fLaC" {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "not a FLAC stream"));
    }
    let mut at = 4u64;
    loop {
        f.seek(SeekFrom::Start(at))?;
        f.read_exact(&mut h)?;
        at += 4 + u32::from_be_bytes([0, h[1], h[2], h[3]]) as u64;
        if h[0] & 0x80 != 0 {
            return Ok(at);
        }
    }
}

/// A file read for `claxon`'s frame decoder, knowing which byte it's at.
struct Tracked {
    f: File,
    buf: Vec<u8>,
    pos: usize,
    len: usize,
    /// The file's byte at `buf[0]`.
    start: u64,
}

impl Tracked {
    fn at(&self) -> u64 {
        self.start + self.pos as u64
    }

    /// Whether there's a byte to read, reading more if need be.
    fn ready(&mut self) -> io::Result<bool> {
        if self.pos < self.len {
            return Ok(true);
        }
        self.start += self.len as u64;
        (self.pos, self.len) = (0, 0);
        loop {
            match self.f.read(&mut self.buf) {
                Ok(n) => {
                    self.len = n;
                    return Ok(n > 0);
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
    }
}

fn eof() -> io::Error {
    io::Error::new(io::ErrorKind::UnexpectedEof, "unexpected eof")
}

impl ReadBytes for Tracked {
    fn read_u8(&mut self) -> io::Result<u8> {
        self.read_u8_or_eof()?.ok_or_else(eof)
    }

    fn read_u8_or_eof(&mut self) -> io::Result<Option<u8>> {
        if !self.ready()? {
            return Ok(None);
        }
        self.pos += 1;
        Ok(Some(self.buf[self.pos - 1]))
    }

    fn read_into(&mut self, out: &mut [u8]) -> io::Result<()> {
        let mut k = 0;
        while k < out.len() {
            if !self.ready()? {
                return Err(eof());
            }
            let n = (out.len() - k).min(self.len - self.pos);
            out[k..k + n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
            (k, self.pos) = (k + n, self.pos + n);
        }
        Ok(())
    }

    fn skip(&mut self, amount: u32) -> io::Result<()> {
        let mut left = amount as usize;
        while left > 0 {
            if !self.ready()? {
                return Err(eof());
            }
            let n = left.min(self.len - self.pos);
            (left, self.pos) = (left - n, self.pos + n);
        }
        Ok(())
    }
}
