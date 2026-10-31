//! Sample-rate conversion for the Dexter tools (`README.md`): a sound at another rate, up or
//! down, through a windowed sinc, as sox's `rate -v` does it. Made for dexter-foley's exports
//! and its JACK outs as `foley-rate`, and moved into the suite (`VENDORED.md`). It has no
//! dependencies.
//!
//! Each output sample is read at its exact place between the input's: the rates' ratio in
//! lowest terms gives every phase, each with taps of its own, worked out in full, so nothing is
//! approximated from a table.
//!
//! - **Flat to 91% of the lower rate's Nyquist** (20 kHz at 44.1 kHz), linear in phase: nothing
//!   moves in time.
//! - **At least 140 dB down from that Nyquist up** (a Kaiser window, β 14.5): converting down
//!   aliases nothing, and converting up adds no images.
//!
//! ```
//! let tone: Vec<f64> = (0..96_000).map(|k| (k as f64 * 0.05).sin() * 0.5).collect();
//! let at_48k = as_resample::convert(&tone, 96_000, 48_000);
//! assert_eq!(at_48k.len(), 48_000);
//! ```
//!
//! [`read`] reads a stream at any speed instead, as a sampler tunes: its place and speed free,
//! its kernel from a table, about 100 dB clean.

pub mod read;

/// Where the passband ends, a part of the lower rate's Nyquist.
pub const PASS: f64 = 0.91;
/// The stopband's depth, dB.
pub const DEPTH: f64 = 140.0;
/// More phases than this (rates whose ratio has no small terms, 44100 to 48001, say) are worked
/// out for each sample as it's needed, not kept: slower, as exact.
const MAX_TABLE: usize = 4096;

/// `x` (one channel at `from` Hz) at `to` Hz: as long as it was, its first sample where it was.
pub fn convert(x: &[f64], from: u32, to: u32) -> Vec<f64> {
    Converter::new(from, to).run(x)
}

/// [`convert`], for `f32` samples (worked in `f64`).
pub fn convert_f32(x: &[f32], from: u32, to: u32) -> Vec<f32> {
    Converter::new(from, to).run_f32(x)
}

/// A conversion from one rate to another, its taps worked out once, for as many channels as
/// there are.
pub struct Converter {
    up: usize,
    down: usize,
    shape: Shape,
    /// Every phase's taps, one after another (none: worked out as needed).
    table: Option<Vec<f64>>,
}

impl Converter {
    /// From `from` Hz to `to` Hz. Both must be more than 0.
    pub fn new(from: u32, to: u32) -> Converter {
        assert!(from > 0 && to > 0, "a rate is more than 0 Hz");
        let g = gcd(from, to);
        let (up, down) = ((to / g) as usize, (from / g) as usize);
        let shape = Shape::new(from as f64, to as f64);
        let table = (from != to && up <= MAX_TABLE).then(|| {
            let mut all = Vec::with_capacity(up * shape.width());
            for p in 0..up {
                all.extend(shape.taps(p as f64 / up as f64));
            }
            all
        });
        Converter { up, down, shape, table }
    }

    /// How many samples `n` at the input's rate come to at the output's.
    pub fn len(&self, n: usize) -> usize {
        (n * self.up).div_ceil(self.down)
    }

    /// One channel converted. The same rate both ways gives the same samples.
    pub fn run(&self, x: &[f64]) -> Vec<f64> {
        if self.up == self.down || x.is_empty() {
            return x.to_vec();
        }
        let (width, reach, len) = (self.shape.width(), self.shape.reach as i64, x.len() as i64);
        let mut own = vec![];
        (0..self.len(x.len()))
            .map(|k| {
                // Output k is at input k·down/up: a whole sample and a phase.
                let at = k * self.down;
                let (whole, phase) = ((at / self.up) as i64, at % self.up);
                let h = match &self.table {
                    Some(t) => &t[phase * width..(phase + 1) * width],
                    None => {
                        own = self.shape.taps(phase as f64 / self.up as f64);
                        &own[..]
                    }
                };
                let first = whole - reach;
                let (a, b) = (first.max(0), (first + width as i64).min(len));
                if a >= b {
                    return 0.0;
                }
                let (h, x) = (&h[(a - first) as usize..(b - first) as usize], &x[a as usize..b as usize]);
                dot(h, x)
            })
            .collect()
    }

    /// [`Converter::run`], for `f32` samples.
    pub fn run_f32(&self, x: &[f32]) -> Vec<f32> {
        if self.up == self.down {
            return x.to_vec();
        }
        let wide: Vec<f64> = x.iter().map(|v| *v as f64).collect();
        self.run(&wide).into_iter().map(|v| v as f32).collect()
    }
}

/// The filter: its cutoff, its window, and how far it reaches either side.
struct Shape {
    /// The cutoff, a part of the input's rate.
    cut: f64,
    /// The window's half-length, input samples, and its β.
    half: f64,
    beta: f64,
    reach: usize,
}

impl Shape {
    fn new(from: f64, to: f64) -> Shape {
        let nyquist = from.min(to) / 2.0;
        let (pass, stop) = (PASS * nyquist, nyquist);
        // The cutoff halfway across the transition; the window as long as Kaiser's formula
        // asks for that transition at that depth.
        let cut = (pass + stop) / 2.0 / from;
        let seconds = (DEPTH - 7.95) / (2.285 * std::f64::consts::TAU * (stop - pass));
        let half = seconds / 2.0 * from;
        Shape { cut, half, beta: 0.1102 * (DEPTH - 8.7), reach: half.ceil() as usize }
    }

    fn width(&self) -> usize {
        2 * self.reach + 1
    }

    /// The taps for an output `frac` of the way past an input sample: from `reach` samples
    /// before it to `reach` after, summing to 1, so a level holds exactly.
    fn taps(&self, frac: f64) -> Vec<f64> {
        let mut h: Vec<f64> = (0..self.width())
            .map(|j| {
                let t = (j as f64 - self.reach as f64) - frac;
                if t.abs() >= self.half {
                    return 0.0;
                }
                let w = bessel_i0(self.beta * (1.0 - (t / self.half).powi(2)).sqrt());
                2.0 * self.cut * sinc(2.0 * self.cut * t) * w
            })
            .collect();
        let sum: f64 = h.iter().sum();
        for v in &mut h {
            *v /= sum;
        }
        h
    }
}

/// The sum of h·x, eight at a time, so it vectorises: the same sums in the same order, every
/// time (as scape's TUNE sums).
#[inline]
fn dot(h: &[f64], x: &[f64]) -> f64 {
    let ((hc, hr), (xc, xr)) = (h.as_chunks::<8>(), x.as_chunks::<8>());
    let mut acc = [0.0f64; 8];
    for (a, b) in hc.iter().zip(xc) {
        for k in 0..8 {
            acc[k] += a[k] * b[k];
        }
    }
    let lanes = ((acc[0] + acc[4]) + (acc[1] + acc[5])) + ((acc[2] + acc[6]) + (acc[3] + acc[7]));
    hr.iter().zip(xr).fold(lanes, |s, (a, b)| s + a * b)
}

fn sinc(x: f64) -> f64 {
    if x == 0.0 { 1.0 } else { (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x) }
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

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

#[cfg(test)]
mod tests;
