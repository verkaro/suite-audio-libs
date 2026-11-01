//! WAV files, written so that a crash or power cut leaves a file every program can read up to
//! the last [`Writer::sync`], and read back in any of the formats written (and 16- or 32-bit PCM
//! from elsewhere). The bytes written are pinned by the files in `testdata/`.
//!
//! Moved into the suite from dexter-foley's `foley-wav` (`VENDORED.md`), itself place's, scape's
//! and Dexter's; scape's change is [`Writer::create_channels`], for stereo renders.
//!
//! For Tape (its RFC, S0 and file rule 5): 64-bit float read, every format read into `f64`
//! exactly ([`Reader::read_channels_f64`]), a file's other chunks kept ([`Reader::chunks`]) and
//! written back ([`Writer::with_chunks`]), BWF's `bext` read and written ([`Bext`]), and RF64
//! refused by name.

mod bext;
pub use bext::Bext;

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

/// Keeps a file's sample data under the 4 GiB a WAV header can describe. At 48 kHz mono that's
/// about 7.7 hours in 24-bit, 11.6 in 16-bit, 5.8 in float.
pub const MAX_DATA: u64 = 4_000_000_000;

/// A file's sample format.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Format {
    /// 24-bit PCM: exactly a 24-bit converter's samples.
    #[default]
    S24,
    /// 16-bit PCM, with TPDF dither.
    S16,
    /// 32-bit float: exactly what JACK delivers.
    F32,
}

impl Format {
    /// Reads a format's name in a project's settings: `s24`, `s16` or `f32`.
    pub fn parse(name: &str) -> Option<Format> {
        match name {
            "s24" => Some(Format::S24),
            "s16" => Some(Format::S16),
            "f32" => Some(Format::F32),
            _ => None,
        }
    }

    /// The size of one sample, and so of one (mono) frame.
    pub fn bytes(self) -> u64 {
        match self {
            Format::S24 => 3,
            Format::S16 => 2,
            Format::F32 => 4,
        }
    }
}

/// How people read the format, e.g. "24-bit" (on the deck's status bar).
impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Format::S24 => "24-bit",
            Format::S16 => "16-bit",
            Format::F32 => "32-bit float",
        })
    }
}

/// The TPDF dither's starting state, so 16-bit files come out the same every time.
const DITHER_SEED: u32 = 0x9E37_79B9;

/// One of a file's chunks, other than the ones that make it a WAV (`fmt `, `fact`, `data`): its
/// four-letter name and its bytes, as they were (`bext`, `iXML`, `LIST` and the rest).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    pub id: [u8; 4],
    pub body: Vec<u8>,
}

/// A WAV being written, mono or interleaved. Its header is kept up to date by [`Writer::sync`].
///
/// Dropping a Writer without [`Writer::close`] syncs it (not durably) and leaves it unpadded,
/// as a crash after a sync would.
pub struct Writer {
    path: PathBuf,
    w: BufWriter<File>,
    format: Format,
    channels: u16,
    dither: u32,
    hdr_len: u64,
    /// The offset of the fact chunk's frame count (float only).
    fact_at: Option<u64>,
    data: u64,
    frames: u64,
    scratch: Vec<u8>,
    closed: bool,
}

impl Writer {
    /// Makes a new mono file. It never overwrites one.
    pub fn create(path: impl AsRef<Path>, rate: u32, format: Format) -> io::Result<Writer> {
        Writer::create_channels(path, rate, format, 1)
    }

    /// Makes a new file of `channels` interleaved channels. It never overwrites one.
    pub fn create_channels(
        path: impl AsRef<Path>,
        rate: u32,
        format: Format,
        channels: u16,
    ) -> io::Result<Writer> {
        let path = path.as_ref().to_path_buf();
        let f = OpenOptions::new().write(true).create_new(true).open(&path)?;
        Writer::with_chunks(f, path, rate, format, channels, &[])
    }

    /// Writes into a file already made, new and empty (as `as-keep`'s `numbered` makes one, at
    /// `path`), with `chunks` between the format and the samples, each as it was.
    pub fn with_chunks(
        mut f: File,
        path: PathBuf,
        rate: u32,
        format: Format,
        channels: u16,
        chunks: &[Chunk],
    ) -> io::Result<Writer> {
        assert!(channels >= 1);
        let mut h = Vec::with_capacity(58);
        h.extend_from_slice(b"RIFF\0\0\0\0WAVEfmt ");
        let mut fact_at = None;
        if format == Format::F32 {
            // WAVE_FORMAT_IEEE_FLOAT, which needs the fact chunk.
            h.extend_from_slice(&18u32.to_le_bytes());
            h.extend_from_slice(&3u16.to_le_bytes());
            h.extend_from_slice(&channels.to_le_bytes());
            h.extend_from_slice(&rate.to_le_bytes());
            h.extend_from_slice(&rate.wrapping_mul(4 * channels as u32).to_le_bytes());
            h.extend_from_slice(&(4 * channels).to_le_bytes());
            h.extend_from_slice(&32u16.to_le_bytes());
            h.extend_from_slice(&0u16.to_le_bytes());
            h.extend_from_slice(b"fact");
            h.extend_from_slice(&4u32.to_le_bytes());
            fact_at = Some(h.len() as u64);
            h.extend_from_slice(&0u32.to_le_bytes());
        } else {
            let n = format.bytes() as u32;
            h.extend_from_slice(&16u32.to_le_bytes());
            h.extend_from_slice(&1u16.to_le_bytes()); // PCM
            h.extend_from_slice(&channels.to_le_bytes());
            h.extend_from_slice(&rate.to_le_bytes());
            h.extend_from_slice(&rate.wrapping_mul(n * channels as u32).to_le_bytes());
            h.extend_from_slice(&(n as u16 * channels).to_le_bytes());
            h.extend_from_slice(&(8 * n as u16).to_le_bytes());
        }
        for c in chunks {
            h.extend_from_slice(&c.id);
            h.extend_from_slice(&(c.body.len() as u32).to_le_bytes());
            h.extend_from_slice(&c.body);
            if c.body.len() % 2 == 1 {
                h.push(0); // RIFF pads a chunk to an even length
            }
        }
        h.extend_from_slice(b"data\0\0\0\0");
        // The header goes straight to the file, so from the start it's a WAV file that readers
        // (and a crash) see as empty, not as garbage.
        f.write_all(&h)?;
        Ok(Writer {
            path,
            w: BufWriter::with_capacity(256 << 10, f),
            format,
            channels,
            dither: DITHER_SEED,
            hdr_len: h.len() as u64,
            fact_at,
            data: 0,
            frames: 0,
            scratch: Vec::new(),
            closed: false,
        })
    }

    /// The size of a frame: 3 for 24-bit, 2 for 16-bit, 4 for float, times the channels.
    pub fn bytes_per_frame(&self) -> u64 {
        self.format.bytes() * self.channels as u64
    }

    /// Appends samples, interleaved when there's more than one channel: whole frames only.
    pub fn write(&mut self, s: &[f32]) -> io::Result<()> {
        let mut b = std::mem::take(&mut self.scratch);
        b.clear();
        match self.format {
            Format::F32 => b.extend(s.iter().flat_map(|v| v.to_le_bytes())),
            Format::S16 => self.dither = s16(s, self.dither, &mut b),
            Format::S24 => s24(s.iter().map(|&v| v as f64), &mut b),
        }
        let result = self.append(&b);
        self.scratch = b;
        result
    }

    /// [`Writer::write`], from `f64`: rounded once, straight to the format (float rounds to the
    /// nearest `f32`; 24-bit to the nearest step, clipping at its limits).
    pub fn write_f64(&mut self, s: &[f64]) -> io::Result<()> {
        let mut b = std::mem::take(&mut self.scratch);
        b.clear();
        match self.format {
            Format::F32 => b.extend(s.iter().flat_map(|&v| (v as f32).to_le_bytes())),
            Format::S16 => {
                let narrow: Vec<f32> = s.iter().map(|&v| v as f32).collect();
                self.dither = s16(&narrow, self.dither, &mut b)
            }
            Format::S24 => s24(s.iter().copied(), &mut b),
        }
        let result = self.append(&b);
        self.scratch = b;
        result
    }

    /// Adds n frames of zeros (digital silence is all-zero in every format).
    pub fn write_silence(&mut self, n: usize) -> io::Result<()> {
        let mut b = std::mem::take(&mut self.scratch);
        b.clear();
        b.resize(n * self.bytes_per_frame() as usize, 0);
        let result = self.append(&b);
        self.scratch = b;
        result
    }

    fn append(&mut self, b: &[u8]) -> io::Result<()> {
        // A BufWriter takes all or reports an error, so the counts stay whole frames.
        self.w.write_all(b)?;
        self.data += b.len() as u64;
        self.frames += b.len() as u64 / self.bytes_per_frame();
        Ok(())
    }

    /// Writes out buffered audio and updates the header to match. With `durable`, it also waits
    /// for the data to reach the disk.
    pub fn sync(&mut self, durable: bool) -> io::Result<()> {
        self.w.flush()?;
        let f = self.w.get_ref();
        let riff = self.hdr_len - 8 + self.data + self.data % 2;
        f.write_all_at(&(riff as u32).to_le_bytes(), 4)?;
        if let Some(at) = self.fact_at {
            f.write_all_at(&(self.frames as u32).to_le_bytes(), at)?;
        }
        f.write_all_at(&(self.data as u32).to_le_bytes(), self.hdr_len - 4)?;
        if durable {
            f.sync_all()?;
        }
        Ok(())
    }

    /// Pads the data to an even length, as RIFF requires, syncs durably and closes the file. The
    /// error names the file.
    pub fn close(mut self) -> io::Result<()> {
        self.closed = true;
        let mut result = Ok(());
        if self.data % 2 == 1 {
            result = self.w.write_all(&[0]);
        }
        let synced = self.sync(true);
        result = result.and(synced);
        result.map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", self.path.display())))
    }

    /// The number of frames written so far.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// The number of bytes of sample data written so far.
    pub fn data_bytes(&self) -> u64 {
        self.data
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        if !self.closed {
            let _ = self.sync(false);
        }
    }
}

/// JACK hands a 24-bit converter's samples over as n / 2^23, so 2^23 gets them back exactly
/// (f32 holds 24 bits, and the scaling is a power of two). Only a software source can go past
/// full scale; that clips at the 24-bit limits.
fn s24(s: impl ExactSizeIterator<Item = f64>, b: &mut Vec<u8>) {
    let start = b.len();
    b.resize(start + s.len() * 3, 0);
    for (o, v) in b[start..].as_chunks_mut::<3>().0.iter_mut().zip(s) {
        let f = v * 8_388_608.0;
        let x = if f >= 8_388_607.0 {
            8_388_607
        } else if f <= -8_388_608.0 {
            -8_388_608
        } else if f >= 0.0 {
            (f + 0.5) as i32
        } else {
            (f - 0.5) as i32
        };
        *o = [x as u8, (x >> 8) as u8, (x >> 16) as u8];
    }
}

/// Converts to 16-bit with TPDF dither: noise of up to one 16-bit step, triangular in shape,
/// added before rounding. The converter's own noise sits below a 16-bit step, so without it quiet
/// passages would be distorted rather than just noisier; with it, the error is a steady floor at
/// about -96 dBFS. Returns the dither's new state.
fn s16(s: &[f32], mut r: u32, b: &mut Vec<u8>) -> u32 {
    let mut next = || {
        r ^= r << 13;
        r ^= r >> 17;
        r ^= r << 5;
        r
    };
    let start = b.len();
    b.resize(start + s.len() * 2, 0);
    for (o, &v) in b[start..].as_chunks_mut::<2>().0.iter_mut().zip(s) {
        // Two uniform values (xorshift32) make a triangular one in (-1, 1).
        let a = next();
        let c = next();
        let d = (a as f64 + c as f64) / 4_294_967_296.0 - 1.0;
        let f = v as f64 * 32768.0 + d;
        let x = if f >= 32767.0 {
            32767
        } else if f <= -32768.0 {
            -32768
        } else if f >= 0.0 {
            (f + 0.5) as i32
        } else {
            (f - 0.5) as i32
        };
        *o = (x as i16).to_le_bytes();
    }
    r
}

/// Reads a mono WAV file in any of the formats [`Writer`] writes, and 16- or 32-bit PCM; or,
/// opened with [`Reader::open_any`] for import, one of any number of channels.
pub struct Reader {
    f: File,
    rate: u32,
    float: bool,
    bits: u16,
    channels: u16,
    /// The offset of the samples.
    data: u64,
    frames: u64,
    raw: Vec<u8>,
    /// Every other chunk: its name, where its body starts, how long it is.
    others: Vec<([u8; 4], u64, u64)>,
}

impl Reader {
    /// Reads the header. A file cut short by a crash reads up to where it ends. The error names
    /// the file.
    pub fn open(path: impl AsRef<Path>) -> io::Result<Reader> {
        Reader::open_with(path.as_ref(), false)
    }

    /// Opens a WAV file of any number of channels, for import: [`Reader::read_channels`] reads
    /// them apart.
    pub fn open_any(path: impl AsRef<Path>) -> io::Result<Reader> {
        Reader::open_with(path.as_ref(), true)
    }

    fn open_with(path: &Path, any_channels: bool) -> io::Result<Reader> {
        let name = path.file_name().unwrap_or(path.as_os_str()).to_string_lossy().into_owned();
        let fail = |msg: String| io::Error::new(io::ErrorKind::InvalidData, format!("{name}: {msg}"));
        let f = File::open(path)?;
        let size = f.metadata()?.len();
        let mut hdr = [0u8; 12];
        if f.read_exact_at(&mut hdr, 0).is_ok() && &hdr[..4] == b"RF64" {
            return Err(fail("an RF64 file (a WAV past 4 GB), which isn't read".into()));
        }
        if f.read_exact_at(&mut hdr, 0).is_err() || &hdr[..4] != b"RIFF" || &hdr[8..] != b"WAVE" {
            return Err(fail("not a WAV file".into()));
        }
        let (mut format, mut channels, mut rate, mut bits) = (0u16, 0u16, 0u32, 0u16);
        let mut data = None;
        let mut others = vec![];
        // Chunks are read only as far as the RIFF header says the file goes: past it lie a
        // recorder's padding, or audio written after the header's last update (a crash), and
        // neither is a chunk. A header that says nothing (never synced) ends at the data.
        let riff = u32::from_le_bytes(hdr[4..8].try_into().unwrap()) as u64;
        let end = if riff == 0 { size } else { size.min(8 + riff) };
        let mut off = 12u64;
        while off + 8 <= end {
            let mut ch = [0u8; 8];
            f.read_exact_at(&mut ch, off)?;
            let mut len = u32::from_le_bytes(ch[4..].try_into().unwrap()) as u64;
            let body = off + 8;
            match &ch[..4] {
                b"fmt " => {
                    let mut fm = [0u8; 40];
                    let n = len.min(40).min(size - body) as usize;
                    f.read_exact_at(&mut fm[..n], body)?;
                    let u16_at = |i: usize| u16::from_le_bytes([fm[i], fm[i + 1]]);
                    format = u16_at(0);
                    channels = u16_at(2);
                    rate = u32::from_le_bytes(fm[4..8].try_into().unwrap());
                    bits = u16_at(14);
                    if format == 0xFFFE && len >= 26 {
                        // WAVE_FORMAT_EXTENSIBLE: the real format starts the GUID.
                        format = u16_at(24);
                    }
                }
                b"data" => {
                    // A file cut short by a crash may claim more than it has, and one never
                    // synced claims nothing.
                    let avail = size - body;
                    let cut = len > avail || len == 0;
                    if cut {
                        len = avail;
                    }
                    data = Some((body, len));
                    if cut || riff == 0 {
                        break; // nothing after the data can be trusted as a chunk
                    }
                }
                b"fact" => {}
                id => others.push((id.try_into().unwrap(), body, len.min(size - body))),
            }
            off = body + len + len % 2;
        }
        let Some((data, len)) = data else { return Err(fail("no audio data".into())) };
        if channels == 0 || (channels != 1 && !any_channels) {
            return Err(fail(format!("{channels} channels; only mono files can be played")));
        }
        let float = format == 3 && matches!(bits, 32 | 64);
        if !float && !(format == 1 && matches!(bits, 16 | 24 | 32)) {
            return Err(fail(format!("unsupported sample format ({format}, {bits} bits)")));
        }
        let frame = bits as u64 / 8 * channels as u64;
        Ok(Reader { f, rate, float, bits, channels, data, frames: len / frame, raw: Vec::new(), others })
    }

    /// Every chunk but `fmt `, `fact` and `data`, in the file's order, each as it was.
    pub fn chunks(&self) -> io::Result<Vec<Chunk>> {
        self.others
            .iter()
            .map(|&(id, at, len)| {
                let mut body = vec![0; len as usize];
                self.f.read_exact_at(&mut body, at)?;
                Ok(Chunk { id, body })
            })
            .collect()
    }

    /// The whole file, each channel's samples as `f64`, exactly as stored: integers over their
    /// full scale (2^15, 2^23, 2^31), floats as they are.
    pub fn read_channels_f64(&mut self) -> io::Result<Vec<Vec<f64>>> {
        let (bps, ch) = (self.bits as usize / 8, self.channels as usize);
        let mut outs: Vec<Vec<f64>> = (0..ch).map(|_| Vec::with_capacity(self.frames as usize)).collect();
        let block = 1 << 16;
        let mut pos = 0u64;
        while pos < self.frames {
            let n = (block as u64).min(self.frames - pos) as usize;
            self.raw.resize(n * bps * ch, 0);
            self.f.read_exact_at(&mut self.raw, self.data + pos * (bps * ch) as u64)?;
            for frame in self.raw.chunks_exact(bps * ch) {
                for (o, b) in outs.iter_mut().zip(frame.chunks_exact(bps)) {
                    o.push(sample_f64(b, self.float));
                }
            }
            pos += n as u64;
        }
        Ok(outs)
    }

    /// Converts frames from `pos` into outs, one per channel (as many frames as the first holds),
    /// with zeros past the end (and where the file can't be read).
    pub fn read_channels(&mut self, pos: i64, outs: &mut [Vec<f32>]) {
        let want = outs.first().map_or(0, Vec::len);
        for o in outs.iter_mut() {
            o.fill(0.0);
        }
        if pos < 0 || pos as u64 >= self.frames {
            return;
        }
        let (bps, ch) = (self.bits as usize / 8, self.channels as usize);
        let n = (want as u64).min(self.frames - pos as u64) as usize;
        self.raw.resize(n * bps * ch, 0);
        let got = read_at_most(&self.f, &mut self.raw, self.data + pos as u64 * (bps * ch) as u64);
        for (k, frame) in self.raw[..got - got % (bps * ch)].chunks_exact(bps * ch).enumerate() {
            for (c, b) in frame.chunks_exact(bps).enumerate() {
                if let Some(o) = outs.get_mut(c) {
                    o[k] = sample(b, self.float);
                }
            }
        }
    }

    /// The file's channels.
    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// Converts `out.len()` frames from `pos` into out, with zeros past the end (and where the
    /// file can't be read).
    pub fn read(&mut self, pos: i64, out: &mut [f32]) {
        out.fill(0.0);
        if pos < 0 || pos as u64 >= self.frames {
            return;
        }
        let pos = pos as u64;
        let n = (out.len() as u64).min(self.frames - pos) as usize;
        let bps = self.bits as usize / 8;
        self.raw.resize(n * bps, 0);
        let got = read_at_most(&self.f, &mut self.raw, self.data + pos * bps as u64);
        for (o, b) in out.iter_mut().zip(self.raw[..got - got % bps].chunks_exact(bps)) {
            *o = sample(b, self.float);
        }
    }

    /// The file's length in frames.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// The file's sample rate.
    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Whether its samples are 32-bit float.
    pub fn is_float(&self) -> bool {
        self.float
    }

    /// Its bits a sample, as stored: 16, 24 or 32, or 64 (float).
    pub fn bits(&self) -> u16 {
        self.bits
    }

    /// Where its samples are, and how many bytes of whole frames there are, for reading them
    /// as they're stored (import).
    pub fn data(&self) -> (u64, u64) {
        (self.data, self.frames * self.bits as u64 / 8 * self.channels as u64)
    }

    /// The file's format, as it reads: 32-bit integer PCM (which Dexter reads but never writes)
    /// comes out as floats, so it counts as F32.
    pub fn format(&self) -> Format {
        match (self.float, self.bits) {
            (false, 16) => Format::S16,
            (false, 24) => Format::S24,
            _ => Format::F32,
        }
    }
}

/// One sample's bytes as a float: 16-, 24- or 32-bit PCM, or 32- or 64-bit float.
fn sample(b: &[u8], float: bool) -> f32 {
    if float {
        return sample_f64(b, true) as f32;
    }
    match b.len() {
        2 => i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0,
        3 => (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0,
        _ => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0,
    }
}

/// One sample's bytes as an `f64`, exactly.
fn sample_f64(b: &[u8], float: bool) -> f64 {
    match (float, b.len()) {
        (true, 4) => f32::from_le_bytes(b.try_into().unwrap()) as f64,
        (true, _) => f64::from_le_bytes(b.try_into().unwrap()),
        (false, 2) => i16::from_le_bytes([b[0], b[1]]) as f64 / 32768.0,
        (false, 3) => (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f64 / 8_388_608.0,
        (false, _) => i32::from_le_bytes(b.try_into().unwrap()) as f64 / 2_147_483_648.0,
    }
}

/// Reads as much of buf as the file has at off, and returns how much.
fn read_at_most(f: &File, buf: &mut [u8], off: u64) -> usize {
    let mut got = 0;
    while got < buf.len() {
        match f.read_at(&mut buf[got..], off + got as u64) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    got
}

#[cfg(test)]
mod tests;
