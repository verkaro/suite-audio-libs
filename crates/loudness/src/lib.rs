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
    let meters: Vec<Meter> = channels.iter().map(|c| Meter::whole(c, rate)).collect();
    integrated_from(&meters)
}

/// [`integrated`], from a meter a channel, each fed its whole channel.
pub fn integrated_from(meters: &[Meter]) -> Loudness {
    let powers: Vec<Blocks> = meters.iter().map(Meter::blocks).collect();
    let short = powers.first().is_some_and(|b| b.short);
    let z = joint(&powers.iter().collect::<Vec<_>>());
    let summed: Vec<f64> = z.iter().map(|v| v * meters.len() as f64).collect();
    Loudness { mean: gated(&z, short), sum: gated(&summed, short), short }
}

/// A linked group's loudness (Tape's RFC, S1): the mean power over every channel of every
/// file, block by block in time, each file at its own rate. A file shorter than the others
/// adds nothing past its end; one shorter than a block is its one block, at the start. Gated,
/// unless every file is shorter than a block.
pub fn group(files: &[(&[&[f64]], u32)]) -> Option<f64> {
    let meters: Vec<Vec<Meter>> =
        files.iter().map(|(chs, rate)| chs.iter().map(|c| Meter::whole(c, *rate)).collect()).collect();
    group_from(&meters.iter().map(Vec::as_slice).collect::<Vec<_>>())
}

/// [`group`], from each file's meters (one a channel), each fed its whole channel.
pub fn group_from(files: &[&[Meter]]) -> Option<f64> {
    let all: Vec<Blocks> = files.iter().flat_map(|m| m.iter().map(Meter::blocks)).collect();
    let short = all.iter().all(|b| b.short);
    gated(&joint(&all.iter().collect::<Vec<_>>()), short)
}

/// One channel's loudness, measured a piece at a time (added 2026-11-01, so a long file needs
/// no more memory than a piece): fed the channel in pieces of any size, it holds what
/// [`integrated`] and [`group`] measure of it whole, bit for bit.
#[derive(Clone, Debug)]
pub struct Meter {
    k: [as_biquad::Biquad; 2],
    state: [[f64; 4]; 2],
    /// Samples a step (100 ms) and a block (400 ms).
    step: usize,
    len: usize,
    /// The step being summed, and how many samples it has.
    acc: f64,
    in_step: usize,
    /// The last three whole steps, oldest first, and how many whole steps there have been.
    last: [f64; 3],
    steps: usize,
    /// Each block's mean square.
    z: Vec<f64>,
    /// Every sample's square summed, in order, and how many: a file shorter than a block.
    all: f64,
    count: usize,
}

impl Meter {
    pub fn new(rate: u32) -> Meter {
        Meter {
            k: k_sections(rate),
            state: [[0.0; 4]; 2],
            step: rate as usize / 10,
            len: rate as usize * 4 / 10,
            acc: 0.0,
            in_step: 0,
            last: [0.0; 3],
            steps: 0,
            z: vec![],
            all: 0.0,
            count: 0,
        }
    }

    /// A meter fed the whole of `x`, a piece at a time.
    fn whole(x: &[f64], rate: u32) -> Meter {
        let mut m = Meter::new(rate);
        for piece in x.chunks(1 << 16) {
            m.push(piece);
        }
        m
    }

    /// The channel's next piece.
    pub fn push(&mut self, x: &[f64]) {
        let mut y = x.to_vec();
        for (k, s) in self.k.iter().zip(&mut self.state) {
            k.process(&mut y, s);
        }
        for &v in &y {
            self.all += v * v;
            self.acc += v * v;
            self.in_step += 1;
            if self.in_step == self.step {
                let w = [self.last[0], self.last[1], self.last[2], self.acc];
                self.steps += 1;
                if self.steps >= 4 {
                    self.z.push((((w[0] + w[1]) + w[2]) + w[3]) / self.len as f64);
                }
                self.last = [w[1], w[2], w[3]];
                (self.acc, self.in_step) = (0.0, 0);
            }
        }
        self.count += y.len();
    }

    /// Its blocks' mean squares, as far as it's been fed.
    fn blocks(&self) -> Blocks {
        if self.count == 0 {
            return Blocks { z: vec![], short: true };
        }
        if self.count < self.len {
            return Blocks { z: vec![self.all / self.count as f64], short: true };
        }
        Blocks { z: self.z.clone(), short: false }
    }
}

/// A channel's true peak ([`true_peak`]), measured a piece at a time: fed the channel in pieces
/// of any size, [`PeakMeter::finish`] is what [`true_peak`] finds in it whole, bit for bit
/// (added 2026-11-01).
pub struct PeakMeter {
    up: usize,
    c: Option<as_resample::Converter>,
    reach: usize,
    /// What's held of the input, from sample `held0` on.
    held: Vec<f64>,
    held0: usize,
    /// The next piece to convert starts here; `n` samples have been fed.
    a: usize,
    n: usize,
    most: f64,
}

/// Samples a piece of [`true_peak`]'s conversion.
const PEAK_PIECE: usize = 1 << 16;

impl PeakMeter {
    pub fn new(rate: u32) -> PeakMeter {
        let up = (192_000u32).div_ceil(rate).max(1) as usize;
        let c = (up > 1).then(|| as_resample::Converter::new(rate, rate * up as u32));
        let reach = c.as_ref().map_or(0, |c| c.reach());
        PeakMeter { up, c, reach, held: vec![], held0: 0, a: 0, n: 0, most: 0.0 }
    }

    /// The channel's next piece.
    pub fn push(&mut self, x: &[f64]) {
        self.most = x.iter().fold(self.most, |m, v| f64::max(m, v.abs()));
        self.n += x.len();
        if self.c.is_some() {
            self.held.extend_from_slice(x);
            self.convert(false);
        }
    }

    /// The true peak, as a level.
    pub fn finish(mut self) -> f64 {
        if self.c.is_some() {
            self.convert(true);
        }
        self.most
    }

    /// Converts each piece whose reach has been fed (or, at the `end`, every piece left).
    fn convert(&mut self, end: bool) {
        let Some(c) = &self.c else { return };
        while self.a < self.n && (end || self.a + PEAK_PIECE + self.reach <= self.n) {
            let (a, b) = (self.a, (self.a + PEAK_PIECE).min(self.n));
            let lo = a.saturating_sub(self.reach);
            let hi = (b + self.reach).min(self.n);
            let part = c.run(&self.held[lo - self.held0..hi - self.held0]);
            let up = self.up;
            self.most =
                part[(a - lo) * up..(b - lo) * up].iter().fold(self.most, |m, v| f64::max(m, v.abs()));
            self.a = b;
            let keep = b.saturating_sub(self.reach);
            self.held.drain(..keep - self.held0);
            self.held0 = keep;
        }
    }
}

/// The true peak of a channel at `rate` Hz, as a level (1.0 full scale): the largest magnitude
/// of its samples at four times 48 kHz or more. Converted a piece at a time, each with the
/// converter's reach either side (`as-resample`, `Converter::reach`), so a long file needs no
/// more memory than a piece.
pub fn true_peak(x: &[f64], rate: u32) -> f64 {
    let mut m = PeakMeter::new(rate);
    for piece in x.chunks(PEAK_PIECE) {
        m.push(piece);
    }
    m.finish()
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
fn k_sections(rate: u32) -> [as_biquad::Biquad; 2] {
    let (c, d) = (2.0 * 48_000.0, 2.0 * rate as f64);
    let section = |k: [f64; 5]| -> as_biquad::Biquad {
        // H(s), from s = c(1 − z⁻¹)/(1 + z⁻¹): numerator and denominator as s², s, 1.
        let analog = |p0: f64, p1: f64, p2: f64| [p0 - p1 + p2, 2.0 * c * (p0 - p2), c * c * (p0 + p1 + p2)];
        let (n, m) = (analog(k[0], k[1], k[2]), analog(1.0, k[3], k[4]));
        // And forward at the new rate, s = d(1 − z⁻¹)/(1 + z⁻¹).
        let digital = |q: [f64; 3]| {
            [q[0] * d * d + q[1] * d + q[2], 2.0 * (q[2] - q[0] * d * d), q[0] * d * d - q[1] * d + q[2]]
        };
        let (b, a) = (digital(n), digital(m));
        as_biquad::Biquad {
            b0: b[0] / a[0],
            b1: b[1] / a[0],
            b2: b[2] / a[0],
            a1: a[1] / a[0],
            a2: a[2] / a[0],
        }
    };
    [section(K48[0]), section(K48[1])]
}

/// The K-weighted signal, whole (for the tests).
#[cfg(test)]
fn k_weight(x: &[f64], rate: u32) -> Vec<f64> {
    let mut y = x.to_vec();
    for k in k_sections(rate) {
        k.run(&mut y);
    }
    y
}

#[cfg(test)]
mod tests;
