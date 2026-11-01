//! Test signals.

use std::f64::consts::TAU;

/// A sine of `hz` at `amp` (peak), `n` samples at `rate`, starting at phase 0.
pub fn sine(hz: f64, amp: f64, n: usize, rate: u32) -> Vec<f64> {
    (0..n).map(|k| amp * libm::sin(TAU * hz * k as f64 / rate as f64)).collect()
}

/// `x` faded in and out over `n` samples each, by a raised cosine.
pub fn faded(mut x: Vec<f64>, n: usize) -> Vec<f64> {
    let len = x.len();
    for k in 0..n.min(len / 2) {
        let g = 0.5 - 0.5 * libm::cos(std::f64::consts::PI * k as f64 / n as f64);
        x[k] *= g;
        x[len - 1 - k] *= g;
    }
    x
}

/// One segment of a stepped sweep: where it starts, how long it settles, how long it's
/// measured.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step {
    pub hz: f64,
    pub start: usize,
    pub settle: usize,
    pub measure: usize,
}

/// The third-octave centres from 20 Hz to 20 kHz, each a whole number of 5 Hz, so 0.2 s holds
/// whole cycles of every one.
pub const THIRDS: [f64; 31] = [
    20.0, 25.0, 30.0, 40.0, 50.0, 65.0, 80.0, 100.0, 125.0, 160.0, 200.0, 250.0, 315.0, 400.0, 500.0, 630.0,
    800.0, 1000.0, 1250.0, 1600.0, 2000.0, 2500.0, 3150.0, 4000.0, 5000.0, 6300.0, 8000.0, 10_000.0,
    12_500.0, 16_000.0, 20_000.0,
];

/// A stepped sweep: each of `freqs` as a sine of `amp`, faded in over 20 ms, settled for
/// `settle` s and measured over `measure` s (whole cycles of each, if `measure` is a whole
/// number of 0.2 s), one after another. Returns the signal and where each step lies.
pub fn stepped(freqs: &[f64], amp: f64, settle: f64, measure: f64, rate: u32) -> (Vec<f64>, Vec<Step>) {
    let (s, m) = ((settle * rate as f64) as usize, (measure * rate as f64) as usize);
    let mut x = Vec::with_capacity(freqs.len() * (s + m));
    let mut steps = vec![];
    for &hz in freqs {
        steps.push(Step { hz, start: x.len(), settle: s, measure: m });
        let mut seg = sine(hz, amp, s + m, rate);
        let fade = (0.02 * rate as f64) as usize;
        for (k, v) in seg.iter_mut().take(fade).enumerate() {
            *v *= 0.5 - 0.5 * libm::cos(std::f64::consts::PI * k as f64 / fade as f64);
        }
        x.extend(seg);
    }
    (x, steps)
}

/// A unit impulse at `at` in `n` samples of zero, `amp` high.
pub fn impulse(n: usize, at: usize, amp: f64) -> Vec<f64> {
    let mut x = vec![0.0; n];
    x[at] = amp;
    x
}

/// White noise, uniform in (−amp, amp), from a fixed seed: the same every time.
pub fn noise(n: usize, amp: f64, seed: u64) -> Vec<f64> {
    let mut r = Rng::new(seed);
    (0..n).map(|_| amp * r.uniform()).collect()
}

/// A fixed-seed generator (SplitMix64): the same numbers on every machine.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in (−1, 1).
    pub fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    }
}

/// The torture set (Tape's E12): each a name and a mono signal at `rate`, `secs` long.
pub fn torture(rate: u32, secs: f64) -> Vec<(&'static str, Vec<f64>)> {
    let n = (secs * rate as f64) as usize;
    let square: Vec<f64> =
        (0..n).map(|k| if (k * 200 / rate as usize) % 2 == 0 { 1.0 } else { -1.0 }).collect();
    let mut impulses = vec![0.0; n];
    for k in (rate as usize / 10..n).step_by(rate as usize / 7) {
        impulses[k] = if k % 2 == 0 { 1.0 } else { -1.0 };
    }
    let step: Vec<f64> = (0..n).map(|k| if k >= n / 3 { 0.9 } else { 0.0 }).collect();
    let mut spikes = sine(440.0, 0.1, n, rate);
    for (i, k) in (rate as usize / 8..n).step_by(rate as usize / 5).enumerate() {
        spikes[k] = crate::level(6.0 * (1 + i % 4) as f64); // +6 to +24 dBFS
    }
    vec![
        ("full-scale square", square),
        ("full-scale white noise", noise(n, 1.0, 12)),
        ("impulses", impulses),
        ("DC step", step),
        ("0 dBFS 20 kHz sine", sine(20_000.0, 1.0, n, rate)),
        ("isolated float spikes to +24 dBFS", spikes),
    ]
}
