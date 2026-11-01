//! Loudness, as ITU-R BS.1770-4 measures it (the EBU R128 method), for Tape's calibration (its
//! RFC, S1) and any tool that needs a file's loudness, its true peak, or a trim to a reference.
//! Written for the suite, 2026-11-01.
//!
//! - **K-weighting at any rate:** the standard's 48 kHz filter taken back to its analog
//!   prototype and forward again at the file's rate, so every rate measures as 48 kHz does.
//! - **Gated:** 400 ms blocks every 100 ms, the absolute gate at −70 LUFS, the relative 10 LU
//!   under the loudness of the blocks that pass it.
//! - **Two ways of adding channels.** BS.1770 sums the channels' powers ([`Loudness::sum`]), so a
//!   stereo file reads 3 LU louder than one of its channels alone. A tape machine sees each
//!   track's own level, so Tape takes their mean ([`Loudness::mean`], its amendment A1): a
//!   stereo file reads as loud as each channel, and a group of files as loud as one of them.
//!   For mono the two are the same. Every channel is weighted 1 (stems carry no layout).
//! - **Short files:** one under a block (400 ms) is measured as one block, its whole length,
//!   ungated (A8), so a one-shot hit isn't read as silence.
//! - **True peak** from the samples at four times 48 kHz or more, through `as-resample`.
//!
//! Everything goes through `libm` and plain arithmetic in a fixed order, so the numbers are
//! the same on every machine.
//!
//! ```
//! // A full-scale 1 kHz sine reads −3.01 LUFS.
//! let x: Vec<f64> = (0..48_000).map(|k| libm::sin(std::f64::consts::TAU * 1000.0 * k as f64 / 48_000.0)).collect();
//! let l = as_loudness::integrated(&[&x], 48_000);
//! assert!((l.mean.unwrap() + 3.01).abs() < 0.01);
//! ```

/// The absolute gate, LUFS: blocks quieter are left out, and a file with none louder is silent.
pub const ABSOLUTE_GATE: f64 = -70.0;
/// The relative gate, LU under the loudness of the blocks past the absolute gate.
pub const RELATIVE_GATE: f64 = -10.0;
/// The most a trim moves a file, dB either way.
pub const CLAMP: f64 = 24.0;

/// A file's integrated loudness, both ways of adding its channels. `None`: silent (nothing
/// past the absolute gate).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Loudness {
    /// The channels' mean power (Tape's A1), LUFS.
    pub mean: Option<f64>,
    /// The channels' summed power (BS.1770 itself), LUFS.
    pub sum: Option<f64>,
    /// Shorter than one block: measured whole, ungated.
    pub short: bool,
}

/// What a file needs to reach a reference.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Calibration {
    /// Quieter than the absolute gate: passed through untouched.
    Silent,
    /// `db` of gain to reach the reference (clamped to ±[`CLAMP`], and `clamped` says so).
    Trim { db: f64, clamped: bool },
}

/// The trim from a measured loudness (`None`: silent) to `reference`, LUFS.
pub fn calibrate(measured: Option<f64>, reference: f64) -> Calibration {
    match measured {
        None => Calibration::Silent,
        Some(m) => {
            let want = reference - m;
            let db = want.clamp(-CLAMP, CLAMP);
            Calibration::Trim { db, clamped: db != want }
        }
    }
}

/// A file's loudness: its channels, all one length, at `rate` Hz.
pub fn integrated(channels: &[&[f64]], rate: u32) -> Loudness {
    let powers: Vec<Blocks> = channels.iter().map(|c| blocks(c, rate)).collect();
    let short = powers.first().is_some_and(|b| b.short);
    let z = joint(&powers.iter().collect::<Vec<_>>());
    let summed: Vec<f64> = z.iter().map(|v| v * channels.len() as f64).collect();
    Loudness { mean: gated(&z, short), sum: gated(&summed, short), short }
}

/// A linked group's loudness (Tape's RFC, S1): the mean power over every channel of every
/// file, block by block in time, each file at its own rate. A file shorter than the others
/// adds nothing past its end; one shorter than a block is its one block, at the start. Gated,
/// unless every file is shorter than a block.
pub fn group(files: &[(&[&[f64]], u32)]) -> Option<f64> {
    let all: Vec<Blocks> =
        files.iter().flat_map(|(chs, rate)| chs.iter().map(|c| blocks(c, *rate))).collect();
    let short = all.iter().all(|b| b.short);
    gated(&joint(&all.iter().collect::<Vec<_>>()), short)
}

/// The true peak of a channel at `rate` Hz, as a level (1.0 full scale): the largest magnitude
/// of its samples at four times 48 kHz or more. Converted a piece at a time, each with the
/// converter's reach either side (`as-resample`, `Converter::reach`), so a long file needs no
/// more memory than a piece.
pub fn true_peak(x: &[f64], rate: u32) -> f64 {
    let sample = x.iter().fold(0.0, |m, v| f64::max(m, v.abs()));
    let up = (192_000u32).div_ceil(rate).max(1) as usize;
    if up == 1 || x.is_empty() {
        return sample;
    }
    let c = as_resample::Converter::new(rate, rate * up as u32);
    let (r, piece) = (c.reach(), 1 << 16);
    let mut most = sample;
    for a in (0..x.len()).step_by(piece) {
        let b = (a + piece).min(x.len());
        let lo = a.saturating_sub(r);
        let part = c.run(&x[lo..(b + r).min(x.len())]);
        most = part[(a - lo) * up..(b - lo) * up].iter().fold(most, |m, v| f64::max(m, v.abs()));
    }
    most
}

/// A level as dB (1.0 is 0 dB), −∞ for 0.
pub fn db(level: f64) -> f64 {
    20.0 * libm::log10(level)
}

/// One channel's K-weighted mean square in each block.
struct Blocks {
    z: Vec<f64>,
    short: bool,
}

/// The K-weighted mean squares of 400 ms blocks every 100 ms; or, shorter than a block, one of
/// the whole channel.
fn blocks(x: &[f64], rate: u32) -> Blocks {
    let y = k_weight(x, rate);
    let (len, step) = (rate as usize * 4 / 10, rate as usize / 10);
    if y.is_empty() {
        return Blocks { z: vec![], short: true };
    }
    // Sums of squares a step at a time, then four steps to a block: the same additions in the
    // same order on every machine.
    let sq = |s: &[f64]| s.iter().fold(0.0, |a, v| a + v * v);
    if y.len() < len {
        return Blocks { z: vec![sq(&y) / y.len() as f64], short: true };
    }
    let steps: Vec<f64> = y.chunks(step).filter(|c| c.len() == step).map(sq).collect();
    let z = steps.windows(4).map(|w| (((w[0] + w[1]) + w[2]) + w[3]) / len as f64).collect();
    Blocks { z, short: false }
}

/// The channels' mean power in each block of time; a channel past its end adds nothing.
fn joint(chs: &[&Blocks]) -> Vec<f64> {
    let most = chs.iter().map(|b| b.z.len()).max().unwrap_or(0);
    (0..most)
        .map(|j| {
            let (sum, n) =
                chs.iter().filter_map(|b| b.z.get(j)).fold((0.0, 0usize), |(s, n), v| (s + v, n + 1));
            sum / n as f64
        })
        .collect()
}

fn lufs(z: f64) -> f64 {
    -0.691 + 10.0 * libm::log10(z)
}

/// The gated loudness of block powers (ungated if `short`), `None` if nothing passes.
fn gated(z: &[f64], short: bool) -> Option<f64> {
    let mean = |v: &[f64]| (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64);
    let past: Vec<f64> = z.iter().copied().filter(|&v| lufs(v) > ABSOLUTE_GATE).collect();
    if short {
        return mean(&past).map(lufs);
    }
    let relative = lufs(mean(&past)?) + RELATIVE_GATE;
    let kept: Vec<f64> = past.into_iter().filter(|&v| lufs(v) > relative).collect();
    mean(&kept).map(lufs)
}

/// BS.1770-4's K-weighting at 48 kHz, its tables 1 and 2: the shelf, then the high-pass, each
/// `[b0, b1, b2, a1, a2]`.
const K48: [[f64; 5]; 2] = [
    [1.53512485958697, -2.69169618940638, 1.19839281085285, -1.69065929318241, 0.73248077421585],
    [1.0, -2.0, 1.0, -1.99004745483398, 0.99007225036621],
];

/// The K-weighting filter at `rate`: each of the standard's 48 kHz sections taken back to the
/// analog filter it's the bilinear transform of, and brought forward again at `rate`. At 48
/// kHz that's the standard's own; at the other rates the response follows it to within a few
/// thousandths of a dB at 1 kHz (the transform's warping, which differs by rate).
fn k_weight(x: &[f64], rate: u32) -> Vec<f64> {
    let (c, d) = (2.0 * 48_000.0, 2.0 * rate as f64);
    let section = |k: [f64; 5]| -> [f64; 5] {
        // H(s), from s = c(1 − z⁻¹)/(1 + z⁻¹): numerator and denominator as s², s, 1.
        let analog = |p0: f64, p1: f64, p2: f64| [p0 - p1 + p2, 2.0 * c * (p0 - p2), c * c * (p0 + p1 + p2)];
        let (n, m) = (analog(k[0], k[1], k[2]), analog(1.0, k[3], k[4]));
        // And forward at the new rate, s = d(1 − z⁻¹)/(1 + z⁻¹).
        let digital = |q: [f64; 3]| {
            [q[0] * d * d + q[1] * d + q[2], 2.0 * (q[2] - q[0] * d * d), q[0] * d * d - q[1] * d + q[2]]
        };
        let (b, a) = (digital(n), digital(m));
        [b[0] / a[0], b[1] / a[0], b[2] / a[0], a[1] / a[0], a[2] / a[0]]
    };
    let mut y = x.to_vec();
    for k in K48 {
        let [b0, b1, b2, a1, a2] = section(k);
        as_biquad::Biquad { b0, b1, b2, a1, a2 }.run(&mut y);
    }
    y
}

#[cfg(test)]
mod tests;
