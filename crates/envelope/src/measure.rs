//! Measurements.

use std::f64::consts::TAU;

/// A tone's amplitude and phase at `hz` in `x[from..from + len]`: one bin of a DFT, exact for
/// whole cycles.
pub fn tone(x: &[f64], hz: f64, rate: u32, from: usize, len: usize) -> (f64, f64) {
    let w = TAU * hz / rate as f64;
    let (mut re, mut im) = (0.0, 0.0);
    for (k, v) in x[from..from + len].iter().enumerate() {
        let a = w * (from + k) as f64;
        re += v * libm::cos(a);
        im -= v * libm::sin(a);
    }
    let (re, im) = (2.0 * re / len as f64, 2.0 * im / len as f64);
    ((re * re + im * im).sqrt(), libm::atan2(im, re))
}

/// Total harmonic distortion at `hz`: the harmonics from the second up to Nyquist, against the
/// fundamental, as a ratio (0.01 is 1%).
pub fn thd(x: &[f64], hz: f64, rate: u32, from: usize, len: usize) -> f64 {
    let fundamental = tone(x, hz, rate, from, len).0;
    let mut sum = 0.0;
    let mut k = 2.0;
    while k * hz < rate as f64 / 2.0 {
        let a = tone(x, k * hz, rate, from, len).0;
        sum += a * a;
        k += 1.0;
    }
    sum.sqrt() / fundamental
}

/// The magnitude spectrum of `x` (as many samples as given) as amplitudes: bin `k` is
/// `k · rate / len` Hz, a sine of amplitude `a` on a bin reads `a`.
pub fn spectrum(x: &[f64]) -> Vec<f64> {
    let n = x.len();
    let mut planner = realfft::RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(n);
    let mut input = x.to_vec();
    let mut out = fft.make_output_vec();
    fft.process(&mut input, &mut out).expect("sizes from the plan");
    out.iter().map(|c| 2.0 * c.norm() / n as f64).collect()
}

/// The largest product in `x` that isn't `hz` or one of its harmonics, against `hz`'s own
/// amplitude, as a ratio: `x` a whole number of `hz`'s periods, so every harmonic sits on a
/// bin. Bins within `guard` of a harmonic aren't counted, nor anything below 20 Hz (DC, and a
/// high-pass settling: not products anyone hears).
pub fn worst_product(x: &[f64], hz: f64, rate: u32, guard: usize) -> (f64, f64) {
    let s = spectrum(x);
    let bin = rate as f64 / x.len() as f64;
    let h = (hz / bin).round() as usize;
    let fundamental = s[h];
    let audible = (20.0 / bin).ceil() as usize;
    let mut worst = (0.0, 0.0);
    for (k, &a) in s.iter().enumerate().skip(audible.max(guard + 1)) {
        let m = (k + h / 2) / h;
        let harmonic = m >= 1 && k.abs_diff(m * h) <= guard;
        if !harmonic && a > worst.0 {
            worst = (a, k as f64 * bin);
        }
    }
    (worst.0 / fundamental, worst.1)
}

/// The largest difference between two signals, as a level.
pub fn null(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).fold(0.0f64, |w, (x, y)| w.max((x - y).abs()))
}

/// The largest magnitude.
pub fn peak(x: &[f64]) -> f64 {
    x.iter().fold(0.0f64, |w, v| w.max(v.abs()))
}

/// The mean: DC.
pub fn dc(x: &[f64]) -> f64 {
    if x.is_empty() { 0.0 } else { x.iter().sum::<f64>() / x.len() as f64 }
}

/// Where the largest magnitude is.
pub fn peak_at(x: &[f64]) -> usize {
    (0..x.len()).max_by(|&a, &b| x[a].abs().total_cmp(&x[b].abs())).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::sine;

    #[test]
    fn a_tone_reads_its_amplitude_and_thd_its_harmonics() {
        let r = 48_000;
        let x: Vec<f64> = sine(1000.0, 0.5, r as usize, r)
            .iter()
            .zip(sine(3000.0, 0.005, r as usize, r))
            .map(|(a, b)| a + b)
            .collect();
        let (a, _) = tone(&x, 1000.0, r, 0, r as usize);
        assert!((a - 0.5).abs() < 1e-12);
        assert!((thd(&x, 1000.0, r, 0, r as usize) - 0.01).abs() < 1e-9);
    }

    #[test]
    fn a_product_that_isnt_a_harmonic_is_found() {
        let r = 48_000;
        let x: Vec<f64> = sine(15_000.0, 0.5, 4800, r)
            .iter()
            .zip(sine(9000.0, 0.5e-4, 4800, r))
            .zip(sine(30_000.0 - 24_000.0, 0.0, 4800, r))
            .map(|((a, b), c)| a + b + c)
            .collect();
        let (ratio, at) = worst_product(&x, 15_000.0, r, 0);
        assert!((crate::db(ratio) + 80.0).abs() < 0.01, "{}", crate::db(ratio));
        assert_eq!(at, 9000.0);
    }
}
