//! Gain reduction for the family's shaping units (step 4 of `specs/shared.md`, 2026-11-12; the
//! spec is `docs/spec.md`, the shape `RFC-eq-dynamics.md`). For sonorack first, then BRR, ducky,
//! place and irma. Written here, not moved.
//!
//! The units are its modules, one a step: [`deess`], the de-esser (step 5); `opto` and `fet` to
//! come. The core they share:
//!
//! - [`Knee`]: the gain computer, in dB, with a quadratic soft knee, fed forward ([`Knee::ff`]) or
//!   back ([`Knee::fb`]), and the static curve a panel draws ([`Knee::curve`]).
//! - [`Follower`]: the reduction moving toward its target, by an attack corrected for the loop's
//!   gain, so a feedback compressor's attack is the dial's time at any ratio and can't oscillate.
//! - [`peak_db`], [`MeanSquare`]: the detectors, sine-referenced.
//! - [`gain`]: a reduction as a gain, exactly 1.0 at none, so a unit at rest passes to the bit.
//! - [`Fade`]: the 10 ms equal-gain crossfade (In, a mode, a voicing), which runs to its end, the
//!   latest target waiting. A gain or a level setting ramps by [`as_control::Ramp`].
//!
//! Numbers are `f64`, a sample at a time, transcendentals through `libm`, arithmetic in a fixed
//! order: the same input and settings give the same bits at any block size, on any machine.

pub use as_control::Ramp;

pub mod deess;

#[cfg(test)]
mod rigs;
#[cfg(test)]
mod tests;

/// The most reduction any unit takes, in dB.
pub const MAX_GR_DB: f64 = 40.0;

/// A crossfade's length, and a ramp's.
pub const FADE_S: f64 = 0.010;
pub const RAMP_S: f64 = 0.005;

/// A releasing reduction below this, in dB, is set to exactly 0.
const RHO_FLOOR: f64 = 1e-12;

/// `t · rate` in whole samples, as every ramp and fade counts.
pub fn samples(t: f64, rate: f64) -> usize {
    libm::round(t * rate) as usize
}

/// A gain computer: a threshold and a knee width in dB, a ratio from input to output.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Knee {
    pub threshold: f64,
    pub ratio: f64,
    /// The knee's whole width, centred on the threshold. 0 is a hard knee.
    pub width: f64,
}

impl Knee {
    /// Fed forward: the reduction for an input level `x`, ≥ 0, exactly 0 below the knee. Its
    /// straight part has slope `1 − 1/R`.
    pub fn ff(&self, x: f64) -> f64 {
        self.shape(x, 1.0 - 1.0 / self.ratio)
    }

    /// Its slope `dr/dx`.
    pub fn ff_slope(&self, x: f64) -> f64 {
        self.slope(x, 1.0 - 1.0 / self.ratio)
    }

    /// Fed back: the reduction for the **output's** level `y`, exactly 0 below the knee. For a
    /// ratio `R` from input to output, its straight part has slope `R − 1`.
    pub fn fb(&self, y: f64) -> f64 {
        self.shape(y, self.ratio - 1.0)
    }

    /// Its slope `s(y) = dr/dy`: 0 below the knee, rising through it, `R − 1` above. What the
    /// loop's attack is corrected by.
    pub fn fb_slope(&self, y: f64) -> f64 {
        self.slope(y, self.ratio - 1.0)
    }

    /// The feedback form's static transfer: the output level for a steady input level `x`, solving
    /// `x = y + r(y)` by bisection, and never more than [`MAX_GR_DB`] under `x`, as the
    /// [`Follower`] holds it. Off the audio thread; no state. Above the knee it is `T + (x − T)/R`.
    pub fn curve(&self, x: f64) -> f64 {
        // The reduction held is between 0 and MAX_GR_DB, so y is between x − MAX_GR_DB and x;
        // y + r(y) rises strictly with y (its slope is at least 1). Where the solution would be
        // further under, the bisection ends on x − MAX_GR_DB.
        let (mut lo, mut hi) = (x - MAX_GR_DB, x);
        for _ in 0..60 {
            let mid = 0.5 * (lo + hi);
            if mid + self.fb(mid) < x {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        0.5 * (lo + hi)
    }

    /// The feed-forward form's static transfer: `x − r(x)`, the reduction held to [`MAX_GR_DB`].
    pub fn curve_ff(&self, x: f64) -> f64 {
        x - self.ff(x).min(MAX_GR_DB)
    }

    fn shape(&self, v: f64, k: f64) -> f64 {
        let (d, w) = (v - self.threshold, self.width);
        if d <= -0.5 * w {
            0.0
        } else if d < 0.5 * w {
            k * (d + 0.5 * w) * (d + 0.5 * w) / (2.0 * w)
        } else {
            k * d
        }
    }

    fn slope(&self, v: f64, k: f64) -> f64 {
        let (d, w) = (v - self.threshold, self.width);
        if d <= -0.5 * w {
            0.0
        } else if d < 0.5 * w {
            k * (d + 0.5 * w) / w
        } else {
            k
        }
    }
}

/// A peak level: `20·log10|v|`, never below −300 dB. A sine of peak 1.0 reads 0.
#[inline]
pub fn peak_db(v: f64) -> f64 {
    20.0 * libm::log10(v.abs().max(1e-15))
}

/// A mean-square detector: branching one-poles on the square (the attack's when the square is
/// above the state, the release's otherwise), read sine-referenced. Two channels feed it the
/// larger of their squares.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeanSquare {
    ms: f64,
    attack: f64,
    release: f64,
}

/// A one-pole's step for a time constant: `1 − exp(−1/(τ·rate))`.
pub fn one_pole(tau: f64, rate: f64) -> f64 {
    1.0 - libm::exp(-1.0 / (tau * rate))
}

impl MeanSquare {
    /// At rest, with attack and release time constants in seconds.
    pub fn new(rate: f64, attack: f64, release: f64) -> MeanSquare {
        MeanSquare { ms: 0.0, attack: one_pole(attack, rate), release: one_pole(release, rate) }
    }

    /// One sample's square in.
    #[inline]
    pub fn feed(&mut self, sq: f64) {
        let c = if sq > self.ms { self.attack } else { self.release };
        self.ms += c * (sq - self.ms);
        // The floor, so the state never goes denormal: below it reads −300 dB anyway.
        if self.ms < 1e-30 {
            self.ms = 0.0;
        }
    }

    /// The level, `10·log10(2·ms)`, never below −300 dB.
    #[inline]
    pub fn db(&self) -> f64 {
        10.0 * libm::log10((2.0 * self.ms).max(1e-30))
    }

    /// The state, the mean square.
    pub fn ms(&self) -> f64 {
        self.ms
    }
}

/// A reduction in dB moving toward its target, a sample at a time (`docs/spec.md`, "The feedback
/// loop").
///
/// The attack is `c_a = (1 − p)/(1 + s)`, with `p = exp(−1/(τ_a·rate))` and `s` the gain
/// computer's slope where the detector is: in a feedback loop the reduction then closes with a
/// pole at exactly `p` at any ratio, so a step measures the dial's time and the loop is stable at
/// any setting. Fed forward, `s` is 0 and it's a plain one-pole. The release is a plain one-pole,
/// or corrected the same way when the unit asks ([`Follower::new`]'s `corrected`): uncorrected, a
/// feedback compressor's release depends on the programme, as the units the RFCs name do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Follower {
    rho: f64,
    /// `1 − p` for the attack and the release.
    attack: f64,
    release: f64,
    corrected: bool,
}

impl Follower {
    /// At rest (no reduction), with attack and release times in seconds.
    pub fn new(rate: f64, attack: f64, release: f64, corrected: bool) -> Follower {
        let mut f = Follower { rho: 0.0, attack: 0.0, release: 0.0, corrected };
        f.times(rate, attack, release);
        f
    }

    /// New times, from the next sample. Calls `exp`: a unit calls it when a time is set, not per
    /// sample.
    pub fn times(&mut self, rate: f64, attack: f64, release: f64) {
        self.attack = one_pole(attack, rate);
        self.release = one_pole(release, rate);
    }

    /// One sample: toward `target` (dB of reduction, ≥ 0), with `slope` the gain computer's slope
    /// at the level the detector just read (0 fed forward). Returns the reduction, held to
    /// [`MAX_GR_DB`]. The target isn't held: so the loop's climb stays geometric however far over
    /// the threshold a step lands.
    #[inline]
    pub fn step(&mut self, target: f64, slope: f64) -> f64 {
        if target > self.rho {
            self.rho += self.attack / (1.0 + slope) * (target - self.rho);
            self.rho = self.rho.min(MAX_GR_DB);
        } else {
            let c = if self.corrected { self.release / (1.0 + slope) } else { self.release };
            self.rho += c * (target - self.rho);
            if self.rho < RHO_FLOOR {
                self.rho = 0.0;
            }
        }
        self.rho
    }

    /// The reduction now, in dB.
    pub fn rho(&self) -> f64 {
        self.rho
    }

    /// Set to a reduction at once (a unit starting settled, or a test).
    pub fn reset(&mut self, rho: f64) {
        self.rho = rho;
    }
}

/// A reduction in dB as a gain: `10^(−ρ/20)`, and exactly 1.0 at `ρ == 0`, without a call.
#[inline]
pub fn gain(rho: f64) -> f64 {
    if rho == 0.0 { 1.0 } else { libm::pow(10.0, -rho / 20.0) }
}

/// The 10 ms equal-gain crossfade (`specs/shared.md`, "Ramps"): between two states of a unit (In
/// and out, a mode, a voicing), `(1 − t)·from + t·to`, `t = k/N`. It runs to its end; a target
/// that comes meanwhile waits, the latest replacing any waiting, and starts the next fade when it
/// ends. A target set before the fade has run a sample replaces it, as if the first never came.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fade<T> {
    from: T,
    to: T,
    k: usize,
    n: usize,
    waiting: Option<T>,
}

impl<T: Copy + PartialEq> Fade<T> {
    /// Settled at `at`, fading over `n` samples (`samples(FADE_S, rate)`).
    pub fn new(at: T, n: usize) -> Fade<T> {
        Fade { from: at, to: at, k: n, n, waiting: None }
    }

    /// A new target, in force from the next sample.
    pub fn set(&mut self, v: T) {
        if self.k == self.n {
            if v != self.to {
                (self.from, self.to, self.k) = (self.to, v, 0);
            }
        } else if self.k == 0 {
            // Started but not yet run: replaced.
            if v == self.from {
                (self.to, self.k) = (v, self.n);
            } else {
                self.to = v;
            }
        } else {
            self.waiting = if v == self.to { None } else { Some(v) };
        }
    }

    /// Settled at `v` at once: nothing to fade from (a unit that has run nothing yet).
    pub fn jump(&mut self, v: T) {
        (self.from, self.to, self.k, self.waiting) = (v, v, self.n, None);
    }

    /// The next sample: what it fades from, to, and `t`, the weight of `to`. Settled, `t` is 1.0
    /// and `from` is `to`.
    #[inline]
    pub fn advance(&mut self) -> (T, T, f64) {
        if self.k == self.n
            && let Some(w) = self.waiting.take()
        {
            (self.from, self.to, self.k) = (self.to, w, 0);
        }
        if self.k < self.n {
            self.k += 1;
            if self.k == self.n {
                self.from = self.to;
                return (self.to, self.to, 1.0);
            }
            return (self.from, self.to, self.k as f64 / self.n as f64);
        }
        (self.to, self.to, 1.0)
    }

    /// The state in force once the fade ends: the latest target, waiting or running.
    pub fn target(&self) -> T {
        self.waiting.unwrap_or(self.to)
    }

    /// Whether no fade runs or waits.
    pub fn done(&self) -> bool {
        self.k == self.n && self.waiting.is_none()
    }

    /// Whether a fade is running: a unit needs `from`'s path only while it is.
    pub fn fading(&self) -> bool {
        self.k < self.n
    }
}
