//! First- and second-order filters in `f64`, minimum-phase, for Tape's linear stages (its RFC,
//! S3, S5 and S7) and any tool that needs a shelf, a peak, a high-pass or a low-pass whose
//! coefficients come out the same on every machine. Written for the suite, 2026-11-01; second-order
//! high- and low-passes, shelves and Butterworth sections added 2026-11-02, for ducky.
//!
//! - [`First`]: a first-order section. A high shelf whose zero sits at a frequency and whose
//!   pole sits a gain above it ([`First::shelf`]); its exact inverse, from its own coefficients
//!   ([`First::inverse`]), so the pair nulls to rounding; a high-pass ([`First::highpass`]).
//! - [`Biquad`]: a second-order section. The cookbook's peak ([`Biquad::peak`]); a low-pass
//!   whose poles are the analog filter's exactly and whose magnitude matches the analog one at
//!   DC and at a chosen frequency ([`Biquad::lowpass_matched`], after Vicanek's matched
//!   designs), so its shape holds up to Nyquist even when its corner lies past it. Any analog
//!   second-order section by the bilinear transform prewarped at a frequency
//!   ([`Biquad::bilinear`]), and from it the second-order high-pass and low-pass, the
//!   cookbook's low and high shelves, and shelves whose poles sit at the corner and whose
//!   zeros sit a gain away from it ([`Biquad::low_shelf_poles_at`], [`Biquad::high_shelf_poles_at`]).
//! - [`q_from_octaves`] turns a bandwidth in octaves into a Q; [`butterworth`] gives the
//!   sections of an order-`n` Butterworth filter, to be cascaded from [`First::highpass`] and
//!   [`Biquad::highpass`] (or their low-pass kin).
//!
//! Coefficients go through `libm` and plain arithmetic, never `std`'s transcendentals, and
//! filtering is direct form I with the same operations in the same order, so a filter gives
//! the same bits on x86_64 and aarch64. `run` starts from rest; `process` goes on from a state,
//! so a long sound can be filtered a piece at a time.

use std::f64::consts::PI;

/// A first-order section, `(b0 + b1 z⁻¹) / (1 + a1 z⁻¹)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct First {
    pub b0: f64,
    pub b1: f64,
    pub a1: f64,
}

impl First {
    /// Passes everything unchanged.
    pub const IDENTITY: First = First { b0: 1.0, b1: 0.0, a1: 0.0 };

    /// A high shelf at `rate` Hz: 0 dB at DC, rising from its zero at `zero_hz` until it levels
    /// off at `gain_db`, its pole `10^(gain_db/20)` times the zero's frequency. By the bilinear
    /// transform, prewarped at the zero. 0 dB is the identity.
    pub fn shelf(zero_hz: f64, gain_db: f64, rate: f64) -> First {
        if gain_db == 0.0 {
            return First::IDENTITY;
        }
        let wz = 2.0 * PI * zero_hz;
        let wp = wz * libm::pow(10.0, gain_db / 20.0);
        let k = wz / libm::tan(wz / (2.0 * rate));
        let d = 1.0 + k / wp;
        First { b0: (1.0 + k / wz) / d, b1: (1.0 - k / wz) / d, a1: (1.0 - k / wp) / d }
    }

    /// The exact inverse, `(1/b0) (1 + a1 z⁻¹) / (1 + (b1/b0) z⁻¹)`: stable when this one's
    /// zero lies inside the unit circle, as a shelf's does.
    pub fn inverse(&self) -> First {
        First { b0: 1.0 / self.b0, b1: self.a1 / self.b0, a1: self.b1 / self.b0 }
    }

    /// A high-pass at `rate` Hz, −3 dB at `hz`, by the bilinear transform prewarped there.
    pub fn highpass(hz: f64, rate: f64) -> First {
        let wc = 2.0 * PI * hz;
        let k = wc / libm::tan(wc / (2.0 * rate));
        let d = k + wc;
        First { b0: k / d, b1: -k / d, a1: (wc - k) / d }
    }

    /// Filters `x` in place, from rest.
    pub fn run(&self, x: &mut [f64]) {
        self.process(x, &mut [0.0; 2]);
    }

    /// Filters `x` in place, going on from `state` (the last input and output) and leaving it
    /// where this piece ends: pieces one after another give the same bits as the whole.
    pub fn process(&self, x: &mut [f64], state: &mut [f64; 2]) {
        if *self == First::IDENTITY {
            return;
        }
        let [mut x1, mut y1] = *state;
        for v in x {
            let y = self.b0 * *v + self.b1 * x1 - self.a1 * y1;
            (x1, y1) = (*v, y);
            *v = y;
        }
        *state = [x1, y1];
    }

    /// The magnitude at `hz`, for a `rate` Hz filter.
    pub fn gain(&self, hz: f64, rate: f64) -> f64 {
        let w = 2.0 * PI * hz / rate;
        let (c, s) = (libm::cos(w), libm::sin(w));
        let n = (self.b0 + self.b1 * c, -self.b1 * s);
        let d = (1.0 + self.a1 * c, -self.a1 * s);
        ((n.0 * n.0 + n.1 * n.1) / (d.0 * d.0 + d.1 * d.1)).sqrt()
    }
}

/// A second-order section, `(b0 + b1 z⁻¹ + b2 z⁻²) / (1 + a1 z⁻¹ + a2 z⁻²)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Biquad {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

impl Biquad {
    /// Passes everything unchanged.
    pub const IDENTITY: Biquad = Biquad { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 };

    /// A peak (or dip) of `gain_db` at `hz`, of quality `q`, at `rate` Hz: the cookbook's
    /// (Bristow-Johnson), by the bilinear transform prewarped at `hz`. 0 dB is the identity.
    pub fn peak(hz: f64, q: f64, gain_db: f64, rate: f64) -> Biquad {
        if gain_db == 0.0 {
            return Biquad::IDENTITY;
        }
        let a = libm::pow(10.0, gain_db / 40.0);
        let w = 2.0 * PI * hz / rate;
        let (c, alpha) = (libm::cos(w), libm::sin(w) / (2.0 * q));
        let a0 = 1.0 + alpha / a;
        Biquad {
            b0: (1.0 + alpha * a) / a0,
            b1: -2.0 * c / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: -2.0 * c / a0,
            a2: (1.0 - alpha / a) / a0,
        }
    }

    /// A low-pass at `rate` Hz shaped as the analog `w0² / (s² + (w0/q) s + w0²)`, `w0 = 2π
    /// f0`: its poles the analog poles exactly (impulse invariance), its numerator `b0 + b1 z⁻¹`
    /// chosen so its magnitude is the analog magnitude at DC and at `match_hz` (below Nyquist),
    /// as Vicanek's matched designs do (*Matched Second Order Digital Filters*, 2016). `f0` may
    /// lie above Nyquist.
    pub fn lowpass_matched(f0: f64, q: f64, match_hz: f64, rate: f64) -> Biquad {
        let w0 = 2.0 * PI * f0 / rate; // the analog corner, radians a sample
        let zeta = 1.0 / (2.0 * q);
        let r = libm::exp(-zeta * w0);
        let a1 = if zeta < 1.0 {
            -2.0 * r * libm::cos(w0 * (1.0 - zeta * zeta).sqrt())
        } else if zeta == 1.0 {
            -2.0 * r
        } else {
            -2.0 * r * libm::cosh(w0 * (zeta * zeta - 1.0).sqrt())
        };
        let a2 = r * r;
        // |A(e^{jw})|² = A0 φ0 + A1 φ1 + A2 φ2, φ0 = cos²(w/2), φ1 = sin²(w/2), φ2 = 4 φ0 φ1.
        let (big0, big1, big2) =
            ((1.0 + a1 + a2) * (1.0 + a1 + a2), (1.0 - a1 + a2) * (1.0 - a1 + a2), -4.0 * a2);
        let wm = 2.0 * PI * match_hz / rate;
        let s = libm::sin(wm / 2.0);
        let (p1, p0) = (s * s, 1.0 - s * s);
        let den = big0 * p0 + big1 * p1 + big2 * 4.0 * p0 * p1;
        // The analog magnitude² at the matching frequency.
        let x = match_hz / f0;
        let want = 1.0 / ((1.0 - x * x) * (1.0 - x * x) + (x / q) * (x / q));
        // |b0 + b1 z⁻¹|² = (b0 + b1)² φ0 + (b0 − b1)² φ1: DC fixes the first, the match the second.
        let sum = 1.0 + a1 + a2;
        let diff = ((want * den - big0 * p0) / p1).max(0.0).sqrt();
        Biquad { b0: (sum + diff) / 2.0, b1: (sum - diff) / 2.0, b2: 0.0, a1, a2 }
    }

    /// Any analog second-order section, `(b[2] s² + b[1] s + b[0]) / (a[2] s² + a[1] s + a[0])`
    /// (coefficients in order of the power of `s`), by the bilinear transform at `rate` Hz
    /// prewarped at `f0`: `s` is normalised to the angular frequency `ω0 = 2π f0`, and is
    /// replaced by `(1 − z⁻¹) / (g (1 + z⁻¹))`, `g = tan(π f0 / rate)`. So the digital filter's
    /// magnitude at `f` is the analog one's at `s = j tan(π f / rate) / g`, and at `f0` it is
    /// the analog one's at `s = j` exactly. `f0` must lie between 0 and `rate / 2`, and `a`
    /// must describe a stable section.
    pub fn bilinear(b: [f64; 3], a: [f64; 3], f0: f64, rate: f64) -> Biquad {
        let g = libm::tan(PI * f0 / rate);
        let g2 = g * g;
        let a0 = a[2] + a[1] * g + a[0] * g2;
        Biquad {
            b0: (b[2] + b[1] * g + b[0] * g2) / a0,
            b1: 2.0 * (b[0] * g2 - b[2]) / a0,
            b2: (b[2] - b[1] * g + b[0] * g2) / a0,
            a1: 2.0 * (a[0] * g2 - a[2]) / a0,
            a2: (a[2] - a[1] * g + a[0] * g2) / a0,
        }
    }

    /// A second-order high-pass, `s² / (s² + s/q + 1)`, `−20 log10(q)` dB down at `hz`
    /// (−3 dB for `q = √½`), by [`Biquad::bilinear`].
    pub fn highpass(hz: f64, q: f64, rate: f64) -> Biquad {
        Biquad::bilinear([0.0, 0.0, 1.0], [1.0, 1.0 / q, 1.0], hz, rate)
    }

    /// A second-order low-pass, `1 / (s² + s/q + 1)`, by [`Biquad::bilinear`]. Its response is
    /// the analog one's, warped, so it falls to zero at Nyquist; see [`Biquad::lowpass_matched`]
    /// for one that follows the analog curve up to a chosen frequency.
    pub fn lowpass(hz: f64, q: f64, rate: f64) -> Biquad {
        Biquad::bilinear([1.0, 0.0, 0.0], [1.0, 1.0 / q, 1.0], hz, rate)
    }

    /// A low shelf of `gain_db` below `hz`, 0 dB above it: the cookbook's (Bristow-Johnson),
    /// `A (s² + (√A/q) s + A) / (A s² + (√A/q) s + 1)`, `A = 10^(gain_db/40)`, by
    /// [`Biquad::bilinear`]. Half the gain (in dB) is at `hz`. 0 dB is the identity.
    pub fn low_shelf(hz: f64, q: f64, gain_db: f64, rate: f64) -> Biquad {
        if gain_db == 0.0 {
            return Biquad::IDENTITY;
        }
        let a = libm::pow(10.0, gain_db / 40.0);
        let r = a.sqrt() / q;
        Biquad::bilinear([a * a, a * r, a], [1.0, r, a], hz, rate)
    }

    /// A high shelf of `gain_db` above `hz`, 0 dB below it: the cookbook's,
    /// `A (A s² + (√A/q) s + 1) / (s² + (√A/q) s + A)`, `A = 10^(gain_db/40)`, by
    /// [`Biquad::bilinear`]. Half the gain (in dB) is at `hz`. 0 dB is the identity.
    pub fn high_shelf(hz: f64, q: f64, gain_db: f64, rate: f64) -> Biquad {
        if gain_db == 0.0 {
            return Biquad::IDENTITY;
        }
        let a = libm::pow(10.0, gain_db / 40.0);
        let r = a.sqrt() / q;
        Biquad::bilinear([a, a * r, a * a], [a, r, 1.0], hz, rate)
    }

    /// A low shelf of `gain_db` below its corner, 0 dB above, whose poles sit at `hz` (of
    /// quality `q`, as a low-pass's would) and whose zeros sit at `A · hz`, `A = 10^(gain_db/40)`:
    /// `(s² + (A/q) s + A²) / (s² + s/q + 1)`, by [`Biquad::bilinear`]. So the shelf's
    /// shoulder moves with its gain, where the cookbook's stays at `hz`. 0 dB is the identity.
    pub fn low_shelf_poles_at(hz: f64, q: f64, gain_db: f64, rate: f64) -> Biquad {
        if gain_db == 0.0 {
            return Biquad::IDENTITY;
        }
        let a = libm::pow(10.0, gain_db / 40.0);
        Biquad::bilinear([a * a, a / q, 1.0], [1.0, 1.0 / q, 1.0], hz, rate)
    }

    /// A high shelf of `gain_db` above its corner, 0 dB below, whose poles sit at `hz` (of
    /// quality `q`) and whose zeros sit at `hz / A`, `A = 10^(gain_db/40)`:
    /// `(A² s² + (A/q) s + 1) / (s² + s/q + 1)`, by [`Biquad::bilinear`]. 0 dB is the identity.
    pub fn high_shelf_poles_at(hz: f64, q: f64, gain_db: f64, rate: f64) -> Biquad {
        if gain_db == 0.0 {
            return Biquad::IDENTITY;
        }
        let a = libm::pow(10.0, gain_db / 40.0);
        Biquad::bilinear([1.0, a / q, a * a], [1.0, 1.0 / q, 1.0], hz, rate)
    }

    /// Filters `x` in place, from rest.
    pub fn run(&self, x: &mut [f64]) {
        self.process(x, &mut [0.0; 4]);
    }

    /// Filters `x` in place, going on from `state` (the last two inputs and outputs) and
    /// leaving it where this piece ends: pieces one after another give the same bits as the
    /// whole.
    pub fn process(&self, x: &mut [f64], state: &mut [f64; 4]) {
        if *self == Biquad::IDENTITY {
            return;
        }
        let [mut x1, mut x2, mut y1, mut y2] = *state;
        for v in x {
            let y = self.b0 * *v + self.b1 * x1 + self.b2 * x2 - self.a1 * y1 - self.a2 * y2;
            (x2, x1, y2, y1) = (x1, *v, y1, y);
            *v = y;
        }
        *state = [x1, x2, y1, y2];
    }

    /// The magnitude at `hz`, for a `rate` Hz filter.
    pub fn gain(&self, hz: f64, rate: f64) -> f64 {
        let w = 2.0 * PI * hz / rate;
        let (c1, s1, c2, s2) = (libm::cos(w), libm::sin(w), libm::cos(2.0 * w), libm::sin(2.0 * w));
        let n = (self.b0 + self.b1 * c1 + self.b2 * c2, -self.b1 * s1 - self.b2 * s2);
        let d = (1.0 + self.a1 * c1 + self.a2 * c2, -self.a1 * s1 - self.a2 * s2);
        ((n.0 * n.0 + n.1 * n.1) / (d.0 * d.0 + d.1 * d.1)).sqrt()
    }
}

/// The Q of a bell or a band `bw` octaves wide (between its −3 dB points, or a bell's
/// half-gain points), by the analog relation `1/Q = 2^(bw/2) − 2^(−bw/2)`, not the cookbook's
/// bandwidth form, which allows for the digital warp. `bw` must be above 0.
pub fn q_from_octaves(bw: f64) -> f64 {
    libm::pow(2.0, bw / 2.0) / (libm::pow(2.0, bw) - 1.0)
}

/// The sections of an order-`n` Butterworth filter, to be cascaded: whether a first-order
/// section (at the corner) leads, which it does when `n` is odd; and the Q of each
/// second-order section (at the corner too), `1 / (2 sin((2k − 1) π / (2n)))` for
/// `k = 1..=n/2`, in that order. Order 3 is (`true`, `[1.0]`); order 4 is (`false`, `[1.3066,
/// 0.5412]`). Order 0 is the empty cascade.
pub fn butterworth(n: usize) -> (bool, Vec<f64>) {
    let qs = (1..=n / 2).map(|k| 1.0 / (2.0 * libm::sin((2 * k - 1) as f64 * PI / (2 * n) as f64))).collect();
    (n % 2 == 1, qs)
}

#[cfg(test)]
mod tests;
