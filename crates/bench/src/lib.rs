//! Measuring hardware against a model (`docs/spec.md`). Written here, not moved, 2026-11-12, for
//! sonorack's LA-4 and KT76 voicings, and every "play something known through it and see what
//! comes back" in the family.
//!
//! - [`picture`]: the pictures, as data, for a front to draw.
//! - [`sitting`]: the measuring list, and a sitting's folder: takes, manifest, resume, redo.
//! - [`signal`]: what's played, each with its sync mark, held under the ceiling.
//! - [`align`]: the mark found in a capture: the latency, how sure, the polarity.
//! - [`response`]: a sweep deconvolved (Farina's method): the response and the harmonics.
//! - [`check`]: a capture, the loopback's calibration, and the check on every capture.
//! - [`figures`]: captures reduced to figures, the calibration divided out, as TOML.
//! - [`jack`]: the client: two outs, two ins, a capture played and recorded from one cycle.
//! - [`levels`]: from the 1 kHz signals: the static curve, harmonics, the reduction over time and its times.

pub mod align;
pub mod check;
pub mod figures;
pub mod jack;
pub mod levels;
pub mod picture;
pub mod response;
pub mod signal;
pub mod sitting;

/// Plays `s` through a model offline, as the bench would through the interface: both channels
/// the signal and a second of silence, handed to `unit` whole. What comes back is a capture of
/// each channel, to be checked and reduced the same way as a real one.
pub fn run_model(s: &signal::Signal, unit: &mut impl FnMut(&mut [f64], &mut [f64])) -> [check::Capture; 2] {
    let mut l = s.samples.clone();
    l.resize(l.len() + s.rate as usize, 0.0);
    let mut r = l.clone();
    unit(&mut l, &mut r);
    [l, r].map(|samples| check::Capture { rate: s.rate, samples, xruns: 0 })
}

/// `text` as a new file in `dir`, `stem-NNN.toml`, never over one, flushed to the disk.
fn keep_toml(
    text: &str,
    dir: impl AsRef<std::path::Path>,
    stem: &str,
) -> std::io::Result<std::path::PathBuf> {
    use std::io::Write;
    let (path, mut f) = as_keep::numbered(dir, stem, "toml")?;
    f.write_all(text.as_bytes())?;
    f.sync_all()?;
    Ok(path)
}

#[cfg(test)]
mod tests;
