//! A stream read at any speed, as a sampler plays a sound tuned: each frame read out at its own
//! place between the stream's frames, through a windowed sinc whose passband narrows when the
//! stream's read faster than a frame a frame, so a sound tuned up doesn't alias, and one tuned
//! down doesn't image. Made for BRR's TUNE (written fresh; dexter-scape's `tune.rs` the lesson).
//!
//! Where [`crate::Converter`] works every phase out in full, this reads the kernel from a table,
//! between its entries in a straight line, so any place and any speed costs the same, and a
//! speed that glides is as cheap as one that holds:
//!
//! - **Flat to 82% of Nyquist** (19.6 kHz at 48 kHz), linear in phase, at a speed of 1 or less;
//!   faster, the band narrows by the speed.
//! - **About 100 dB down past Nyquist** (a Kaiser window, β 10, 32 zero crossings each side):
//!   about 100 dB clean in all, where scape's was about 70. The converter's 140 would cost too
//!   much a voice, live.
//! - **The same frames every time:** what's read at a place depends only on the place, the
//!   speed and the stream, never on what was read before, so a block of any size reads the
//!   same.
//!
//! Nothing allocates once the kernel's made ([`warm`], off any realtime thread).
//!
//! ```
//! use as_resample::read::Weights;
//! // A second of a 1 kHz tone, read a fifth faster: 1.2 kHz, 40000 frames.
//! let tone: Vec<f32> = (0..48_000).map(|k| (k as f32 * 0.1309).sin() * 0.5).collect();
//! let mut w = Weights::new();
//! let out: Vec<f32> = (0..40_000).map(|k| { w.at(k as f64 * 1.2, 1.2); w.read(&tone, 0) }).collect();
//! assert_eq!(out.len(), 40_000);
//! ```

use std::sync::LazyLock;

/// The fastest and slowest speeds read, either way: 4, two octaves.
pub const MOST: f64 = 4.0;
/// Zero crossings either side of the kernel's centre.
const ZC: usize = 32;
/// The kernel's table: entries a zero crossing, read between them in a straight line.
const OVER: usize = 1024;
/// The cutoff, a part of Nyquist (at a speed of 1 or less): the window's slope lies across it,
/// ending at Nyquist.
const CUTOFF: f64 = 0.9;
/// The Kaiser window's β: about 100 dB down.
const BETA: f64 = 10.0;
/// The most frames a read reaches either side of its place, at the fastest.
pub const REACH: usize = (ZC as f64 * MOST / CUTOFF) as usize + 1;
/// The most frames a read weighs: [`REACH`] either side, and the frame it's on.
pub const TAPS: usize = 2 * REACH + 2;

/// The kernel from its centre out, `OVER` entries a zero crossing, ending in 0s.
static KERNEL: LazyLock<Box<[f32]>> = LazyLock::new(|| {
    let n = ZC * OVER;
    let edge = bessel_i0(BETA);
    (0..n + 2)
        .map(|i| {
            let u = i as f64 / OVER as f64;
            if u >= ZC as f64 {
                return 0.0;
            }
            let x = std::f64::consts::PI * u;
            let sinc = if i == 0 { 1.0 } else { x.sin() / x };
            let w = u / ZC as f64;
            (sinc * bessel_i0(BETA * (1.0 - w * w).sqrt()) / edge) as f32
        })
        .collect()
});

/// Makes the kernel, before anything on a realtime thread reads through it.
pub fn warm() {
    LazyLock::force(&KERNEL);
}

/// Semitones as a speed: 12 is twice as fast.
pub fn speed(semitones: f64) -> f64 {
    (semitones / 12.0).exp2()
}

/// The weights for reading a stream at one place: frame [`Weights::first`] weighed by the first
/// of them, and on. They sum to 1, so a level holds.
pub struct Weights {
    w: [f32; TAPS],
    first: i64,
    n: usize,
}

impl Default for Weights {
    fn default() -> Weights {
        Weights::new()
    }
}

impl Weights {
    /// None yet: [`Weights::at`] makes them.
    pub fn new() -> Weights {
        Weights { w: [0.0; TAPS], first: 0, n: 0 }
    }

    /// The weights for reading at `place` (a frame of the stream, and a part of the next),
    /// stepping through it `speed` frames a frame (kept within [`MOST`] either way).
    #[inline]
    pub fn at(&mut self, place: f64, speed: f64) {
        let k = &**KERNEL;
        let speed = speed.clamp(MOST.recip(), MOST);
        // Zero crossings a frame of the stream: the cutoff, narrowed by the speed past 1.
        let c = CUTOFF / speed.max(1.0);
        let reach = ZC as f64 / c;
        let first = (place - reach).ceil() as i64;
        let n = ((place + reach).floor() as i64 - first + 1).clamp(0, TAPS as i64) as usize;
        let per = c * OVER as f64;
        let mut sum = 0.0f32;
        for (j, w) in self.w[..n].iter_mut().enumerate() {
            let u = ((first + j as i64) as f64 - place).abs() * per;
            let i = u as usize;
            *w = if i + 1 >= k.len() {
                0.0
            } else {
                let f = (u - i as f64) as f32;
                k[i] + (k[i + 1] - k[i]) * f
            };
            sum += *w;
        }
        let g = if sum == 0.0 { 0.0 } else { sum.recip() };
        for w in &mut self.w[..n] {
            *w *= g;
        }
        (self.first, self.n) = (first, n);
    }

    /// The stream's first frame weighed.
    pub fn first(&self) -> i64 {
        self.first
    }

    /// The weights, from [`Weights::first`] on.
    pub fn weights(&self) -> &[f32] {
        &self.w[..self.n]
    }

    /// The stream read through the weights: `x` is the stream from its frame `from` on, and
    /// everything outside it silence.
    #[inline]
    pub fn read(&self, x: &[f32], from: i64) -> f32 {
        let (a, b) = (self.first.max(from), (self.first + self.n as i64).min(from + x.len() as i64));
        if a >= b {
            return 0.0;
        }
        let w = &self.w[(a - self.first) as usize..(b - self.first) as usize];
        dot(w, &x[(a - from) as usize..(b - from) as usize])
    }
}

/// The sum of w·x, eight at a time, so it vectorises: the same sums in the same order, every
/// time.
#[inline]
pub fn dot(w: &[f32], x: &[f32]) -> f32 {
    let ((wc, wr), (xc, xr)) = (w.as_chunks::<8>(), x.as_chunks::<8>());
    let mut acc = [0.0f32; 8];
    for (a, b) in wc.iter().zip(xc) {
        for k in 0..8 {
            acc[k] += a[k] * b[k];
        }
    }
    let lanes = ((acc[0] + acc[4]) + (acc[1] + acc[5])) + ((acc[2] + acc[6]) + (acc[3] + acc[7]));
    wr.iter().zip(xr).fold(lanes, |s, (a, b)| s + a * b)
}

/// The modified Bessel function of the first kind, order 0: the Kaiser window's.
fn bessel_i0(x: f64) -> f64 {
    let (mut sum, mut term, mut k) = (1.0, 1.0, 1.0);
    while term > sum * 1e-17 {
        term *= (x / (2.0 * k)) * (x / (2.0 * k));
        sum += term;
        k += 1.0;
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 48_000.0;

    fn tone(hz: f64, n: usize) -> Vec<f32> {
        (0..n).map(|k| ((std::f64::consts::TAU * hz * k as f64 / RATE).sin() * 0.5) as f32).collect()
    }

    /// The stream read at `speed` from 0, `n` frames.
    fn read(x: &[f32], speed: f64, n: usize) -> Vec<f32> {
        let mut w = Weights::new();
        (0..n)
            .map(|k| {
                w.at(k as f64 * speed, speed);
                w.read(x, 0)
            })
            .collect()
    }

    fn db(x: f64) -> f64 {
        20.0 * x.log10()
    }

    /// The worst difference from a tone of `hz` at 0.5, away from the ends, in dB of it.
    fn worst(got: &[f32], hz: f64) -> f64 {
        let edge = 1000;
        let err = got[edge..got.len() - edge]
            .iter()
            .enumerate()
            .map(|(k, v)| {
                let want = (std::f64::consts::TAU * hz * (k + edge) as f64 / RATE).sin() * 0.5;
                (*v as f64 - want).abs()
            })
            .fold(0.0, f64::max);
        db(err / 0.5)
    }

    #[test]
    fn a_tone_read_faster_or_slower_is_the_tone_higher_or_lower_100_db_clean() {
        for (hz, semis) in
            [(1000.0, 0.0), (1000.0, 7.0), (440.0, 24.0), (1000.0, -24.0), (5000.0, 3.5), (18_000.0, -0.3)]
        {
            let s = speed(semis);
            let n = 24_000;
            let x = tone(hz, (n as f64 * s) as usize + 2 * REACH);
            let got = read(&x, s, n);
            let e = worst(&got, hz * s);
            assert!(e < -100.0, "{hz} Hz at {semis} semitones: {e:.1} dB");
        }
    }

    #[test]
    fn read_faster_nothing_above_nyquist_folds_back() {
        // 15 kHz an octave up would be 30 kHz: gone, not folded to 18 kHz.
        let x = tone(15_000.0, 48_000 + 2 * REACH);
        let got = read(&x, 2.0, 20_000);
        let peak = got[1000..19_000].iter().map(|v| v.abs() as f64).fold(0.0, f64::max);
        assert!(db(peak / 0.5) < -100.0, "{:.1} dB", db(peak / 0.5));
    }

    #[test]
    fn a_level_holds_and_a_whole_frame_at_a_speed_of_1_is_that_frame() {
        let x = vec![0.25f32; 4000];
        for s in [0.25, 0.7, 1.0, 1.3, 4.0] {
            let got = read(&x, s, (3000.0 / s) as usize);
            let mid = &got[(REACH as f64 / s) as usize + 1..got.len() - (REACH as f64 / s) as usize - 1];
            assert!(mid.iter().all(|v| (v - 0.25).abs() < 1e-6), "speed {s}");
        }
        let t = tone(1000.0, 2000);
        let mut w = Weights::new();
        w.at(700.0, 1.0);
        assert!((w.read(&t, 0) - t[700]).abs() < 1e-6);
    }

    #[test]
    fn what_is_read_at_a_place_is_the_same_however_it_was_reached() {
        let x = tone(3000.0, 9000);
        let mut a = Weights::new();
        let mut b = Weights::new();
        a.at(1234.5678, 1.7);
        b.at(17.0, 0.5);
        b.at(1234.5678, 1.7);
        assert_eq!(a.read(&x, 0).to_bits(), b.read(&x, 0).to_bits());
        // Part of the stream given, from where it starts: the same, for the frames it has.
        assert_eq!(a.read(&x, 0).to_bits(), a.read(&x[1000..2000], 1000).to_bits());
    }

    #[test]
    fn outside_the_stream_is_silence() {
        let x = vec![1.0f32; 100];
        let mut w = Weights::new();
        w.at(-500.0, 1.0);
        assert_eq!(w.read(&x, 0), 0.0);
        w.at(5000.0, 3.0);
        assert_eq!(w.read(&x, 0), 0.0);
        assert!(w.weights().len() <= TAPS);
    }
}
