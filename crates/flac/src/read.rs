//! FLAC read, through `claxon` (written for the suite, 2026-10-29, at BRR's code review: a sound
//! read by more than one sibling is read the same way by all): any bit depth, any number of
//! channels, apart, as floats at 1.0 full scale.

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
    let mut r = claxon::FlacReader::open(path)?;
    let info = r.streaminfo();
    let ch = info.channels as usize;
    let scale = 1.0 / (1u64 << (info.bits_per_sample - 1)) as f32;
    let mut channels = vec![Vec::with_capacity(info.samples.unwrap_or(0) as usize); ch];
    for (k, s) in r.samples().enumerate() {
        match s {
            Ok(s) => channels[k % ch].push(s as f32 * scale),
            // Cut short: what's whole so far.
            Err(claxon::Error::IoError(_)) => break,
            Err(e) => return Err(e),
        }
    }
    let whole = channels.iter().map(Vec::len).min().unwrap_or(0);
    for c in &mut channels {
        c.truncate(whole);
    }
    Ok(Flac { rate: info.sample_rate, bits: info.bits_per_sample, channels })
}
