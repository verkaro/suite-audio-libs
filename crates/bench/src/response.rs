//! The response from a sweep, by Farina's method: the capture convolved with the sweep's inverse
//! (the sweep backwards, falling 6 dB an octave, so the two make an impulse), which puts the
//! linear response at the sweep's end and each harmonic's ahead of it by `L·ln n`. Each is cut
//! out by a window, and divided by the played sweep's own, worked out the same way, so the fades
//! and the band's edges fall out.
//!
//! `realfft` chooses a SIMD path when it runs, so bits are the same for the same build, not
//! promised between machines (as `as-bands` says).

use as_envelope::signal::THIRDS;
use realfft::RealFftPlanner;
use realfft::num_complex::Complex;

/// The sweep's top: 20 kHz, or 0.45 of the rate (`as_envelope::corpus::sweep`).
pub fn top_hz(rate: u32) -> f64 {
    f64::min(20_000.0, 0.45 * rate as f64)
}

/// The sweep's `L`: its length over the log of its span. Harmonic `n` comes `L·ln n` early.
pub fn sweep_l(secs: f64, rate: u32) -> f64 {
    secs / libm::log(top_hz(rate) / 20.0)
}

/// Where the sweep's fade-in ends (100 ms in, `as_envelope::corpus::sweep`): a harmonic grows
/// faster than the level, so one from an input under this reads low.
pub fn fade_in_hz(secs: f64, rate: u32) -> f64 {
    20.0 * libm::exp(0.1 / sweep_l(secs, rate))
}

/// The inverse of a sweep of `secs` played as `x`: `x` backwards, scaled by `e^(−t/L)`.
fn inverse(x: &[f64], secs: f64, rate: u32) -> Vec<f64> {
    let l = sweep_l(secs, rate);
    let r = rate as f64;
    x.iter().rev().enumerate().map(|(k, v)| v * libm::exp(-(k as f64) / r / l)).collect()
}

/// `a` convolved with `b`, through the FFT.
fn convolve(a: &[f64], b: &[f64]) -> Vec<f64> {
    let len = a.len() + b.len() - 1;
    let n = len.next_power_of_two();
    let mut p = RealFftPlanner::<f64>::new();
    let (fwd, inv) = (p.plan_fft_forward(n), p.plan_fft_inverse(n));
    let spec = |x: &[f64]| {
        let mut buf = vec![0.0; n];
        buf[..x.len()].copy_from_slice(x);
        let mut out = fwd.make_output_vec();
        fwd.process(&mut buf, &mut out).expect("sizes from the plan");
        out
    };
    let (sa, sb) = (spec(a), spec(b));
    let mut prod: Vec<Complex<f64>> = sa.iter().zip(&sb).map(|(x, y)| x * y).collect();
    let mut out = vec![0.0; n];
    inv.process(&mut prod, &mut out).expect("sizes from the plan");
    out.truncate(len);
    let k = 1.0 / n as f64;
    out.iter_mut().for_each(|v| *v *= k);
    out
}

/// Magnitudes of `x`'s spectrum on `n` points (zero padded), bin `k` at `k·rate/n` Hz.
fn magnitudes(x: &[f64], n: usize) -> Vec<f64> {
    let mut p = RealFftPlanner::<f64>::new();
    let fwd = p.plan_fft_forward(n);
    let mut buf = vec![0.0; n];
    buf[..x.len()].copy_from_slice(x);
    let mut out = fwd.make_output_vec();
    fwd.process(&mut buf, &mut out).expect("sizes from the plan");
    out.iter().map(|c| c.norm()).collect()
}

/// `h[at − pre .. at + post]`, its ends tapered by half a Hann over a tenth of each side.
fn cut(h: &[f64], at: usize, pre: usize, post: usize) -> Vec<f64> {
    let from = at.saturating_sub(pre);
    let to = (at + post).min(h.len());
    let (tp, tq) = ((at - from) / 10, (to - at) / 10);
    h[from..to]
        .iter()
        .enumerate()
        .map(|(k, v)| {
            let from_start = k;
            let from_end = to - from - 1 - k;
            let g = |d: usize, t: usize| {
                if t == 0 || d >= t {
                    1.0
                } else {
                    0.5 - 0.5 * libm::cos(std::f64::consts::PI * d as f64 / t as f64)
                }
            };
            v * g(from_start, tp) * g(from_end, tq)
        })
        .collect()
}

/// A sweep's deconvolution: the linear response and the harmonics, as spectra.
pub struct Deconvolved {
    pub rate: u32,
    /// Points of each spectrum.
    pub n: usize,
    /// The linear response's magnitude, bin by bin (1.0 passes).
    pub linear: Vec<f64>,
    /// Harmonic 2 to 5: its magnitude against the sweep's at bin `k` (`k·rate/n` Hz, the
    /// harmonic's own frequency, so an input of `1/m` of it).
    pub harmonics: Vec<Vec<f64>>,
}

/// Deconvolves `y`, what came back of the sweep `x` (aligned: `y[k]` follows `x[k]`, and runs
/// on past `x`'s end for the response's tail).
pub fn deconvolve(x: &[f64], y: &[f64], secs: f64, rate: u32) -> Deconvolved {
    let inv = inverse(x, secs, rate);
    let (hx, hy) = (convolve(x, &inv), convolve(y, &inv));
    let at = x.len() - 1;
    let l = sweep_l(secs, rate) * rate as f64;
    // The linear part: from a quarter of the way back to the second harmonic, to 0.5 s on.
    let gap2 = l * libm::log(2.0);
    let (pre, post) = ((0.25 * gap2) as usize, (0.5 * rate as f64) as usize);
    let wx = cut(&hx, at, pre, post);
    let n = (pre + post).next_power_of_two() * 2;
    let ref_mag = magnitudes(&wx, n);
    let lin = magnitudes(&cut(&hy, at, pre, post), n);
    let linear = lin.iter().zip(&ref_mag).map(|(a, b)| if *b > 0.0 { a / b } else { 0.0 }).collect();
    // Harmonic m: at `at − L·ln m`, a window a quarter of the way to its neighbours each side.
    let harmonics = (2..=5)
        .map(|m: u32| {
            let c = at as f64 - l * libm::log(m as f64);
            let near = l * (libm::log(m as f64 + 1.0) - libm::log(m as f64));
            let (pre, post) = ((0.25 * near) as usize, (0.25 * near) as usize);
            let h = magnitudes(&cut(&hy, c.round() as usize, pre, post), n);
            h.iter().zip(&ref_mag).map(|(a, b)| if *b > 0.0 { a / b } else { 0.0 }).collect()
        })
        .collect();
    Deconvolved { rate, n, linear, harmonics }
}

/// `mag` (bins of `n` points) on the third-octave points from 20 Hz to the sweep's top, each the
/// power mean over a sixth of an octave around it, in dB.
pub fn thirds(mag: &[f64], n: usize, rate: u32) -> Vec<(f64, f64)> {
    let bin = rate as f64 / n as f64;
    let top = top_hz(rate);
    THIRDS
        .iter()
        .filter(|&&f| f <= top)
        .map(|&f| {
            let (lo, hi) = (f * libm::pow(2.0, -1.0 / 12.0), f * libm::pow(2.0, 1.0 / 12.0));
            let (a, b) = ((lo / bin).ceil() as usize, ((hi / bin).floor() as usize).min(mag.len() - 1));
            let (mut s, mut c) = (0.0, 0usize);
            for v in &mag[a.min(b)..=b] {
                s += v * v;
                c += 1;
            }
            (f, 10.0 * libm::log10((s / c as f64).max(1e-30)))
        })
        .collect()
}
