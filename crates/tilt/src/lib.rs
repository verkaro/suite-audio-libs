//! TILT: a layer made duller or brighter, as one slope in dB an octave through 1 kHz (Warren,
//! 2026-10-06: one knob). Below zero the highs come down and the lows up, as a distant sound or a
//! dull room; above zero the other way. It serves playback, as a crossfade does: a sound set back
//! in the scene, or noise shaped to match a recording's floor.
//!
//! The slope is a row of first-order shelves, a pole and a zero each, spread evenly by octave
//! from 100 Hz to 16 kHz: from about 250 Hz to 8 kHz the response falls (or rises) at the
//! slope, within a dB; it eases off towards the ends, and is flat beyond them. The level at 1 kHz is as it was. A tilt of zero is
//! no filter at all: a layer that isn't tilted plays bit for bit as before.
//!
//! Moved into the suite from scape's `crates/core/src/tilt.rs` (`VENDORED.md`); its glide is its
//! own constant, where scape's was its player's DECLICK.

use std::f64::consts::PI;

#[cfg(test)]
mod tests;

/// The steepest tilt either way, in dB an octave.
pub const MAX_TILT: f64 = 6.0;
/// Where a tilt leaves the level as it is, in Hz.
pub const PIVOT_HZ: f64 = 1000.0;
/// The band it tilts across, in octaves below and above the pivot: 100 Hz to 16 kHz.
const BELOW: f64 = std::f64::consts::LOG2_10;
const ABOVE: f64 = 4.0;
/// The shelves across the band.
const SECTIONS: usize = 6;
/// A tilt moved while playing glides over this many frames (scape's DECLICK), its shelves made
/// again every `STEP` of them.
pub const GLIDE: usize = 512;
const STEP: usize = 32;

/// One first-order shelf: its coefficients, and the last frame in and out, each channel.
#[derive(Clone, Copy, Default)]
struct Shelf {
    b0: f64,
    b1: f64,
    a1: f64,
    x1: [f64; 2],
    y1: [f64; 2],
}

/// A tilt, with the state that carries it from one block to the next: what it played depends
/// only on the frames, never on how many it's given at once.
#[derive(Clone, Copy)]
pub struct Tilt {
    /// The pivot, in cycles a frame (1 kHz at the rate played).
    pivot: f64,
    /// The slope now, where it glides from and to, and the frames of the glide gone.
    now: f64,
    from: f64,
    to: f64,
    t: usize,
    gain: f64,
    shelves: [Shelf; SECTIONS],
    /// Leaving flat: the shelves take the next frame as if it had always been so, and what
    /// they make is crossfaded in from what they're given over this many frames still.
    prime: bool,
    entering: usize,
    /// Above: the slope only above the pivot, nothing below it (the lows as they are), as
    /// distance dulls a sound ([`Tilt::above`]).
    above: bool,
}

impl Tilt {
    /// A tilt of `db` an octave (clamped to ±[`MAX_TILT`]), pivoting at `pivot` cycles a frame.
    pub fn new(db: f64, pivot: f64) -> Tilt {
        let db = clamp(db);
        let mut t = Tilt {
            pivot,
            now: db,
            from: db,
            to: db,
            t: GLIDE,
            gain: 1.0,
            shelves: Default::default(),
            prime: true,
            entering: 0,
            above: false,
        };
        t.design();
        t
    }

    /// A slope of `db` an octave above `pivot` only, from there to 16 times it, leaving what's
    /// below as it is: distance's dulling (Warren, 2026-10-06: a TILT dulling a distant sound
    /// "doesn't feel natural", it keeps "as much low end").
    pub fn above(db: f64, pivot: f64) -> Tilt {
        let mut t = Tilt::new(0.0, pivot);
        t.above = true;
        let db = clamp(db);
        (t.now, t.from, t.to) = (db, db, db);
        t.design();
        t
    }

    /// The slope it's at or going to.
    pub fn db(&self) -> f64 {
        self.to
    }

    /// Flat, and staying so: nothing to do.
    pub fn idle(&self) -> bool {
        self.now == 0.0 && self.to == 0.0
    }

    /// Another slope, glided to from where it is.
    pub fn set(&mut self, db: f64, pivot: f64) {
        let db = clamp(db);
        if db == self.to && pivot == self.pivot {
            return;
        }
        if self.idle() {
            // From flat it knows nothing of what played: it starts as if the next frame had
            // been there always, and is crossfaded in from the sound as it is, so nothing steps.
            self.prime = true;
            self.entering = GLIDE;
        }
        self.pivot = pivot;
        self.from = self.now;
        self.to = db;
        self.t = 0;
    }

    /// Tilts l and r, in place.
    pub fn run(&mut self, l: &mut [f32], r: &mut [f32]) {
        for k in 0..l.len() {
            if self.t < GLIDE && self.t.is_multiple_of(STEP) {
                let x = (self.t + STEP).min(GLIDE) as f64 / GLIDE as f64;
                self.now =
                    if self.t + STEP >= GLIDE { self.to } else { self.from + (self.to - self.from) * x };
                self.design();
            }
            if self.t < GLIDE {
                self.t += 1;
            }
            let (mut a, mut b) = (l[k] as f64, r[k] as f64);
            if self.prime {
                self.prime = false;
                for s in &mut self.shelves {
                    s.x1 = [a, b];
                    s.y1 = [a, b];
                }
            }
            for s in &mut self.shelves {
                a = s.step(0, a);
                b = s.step(1, b);
            }
            let (mut a, mut b) = (a * self.gain, b * self.gain);
            if self.entering > 0 {
                let x = 1.0 - self.entering as f64 / GLIDE as f64;
                a = l[k] as f64 * (1.0 - x) + a * x;
                b = r[k] as f64 * (1.0 - x) + b * x;
                self.entering -= 1;
            }
            l[k] = a as f32;
            r[k] = b as f32;
        }
    }

    /// The power it passes of white noise, at the slope now: its response squared, averaged
    /// from nothing to Nyquist.
    pub fn power(&self) -> f64 {
        const N: usize = 4096;
        (0..N).map(|k| self.response((k as f64 + 0.5) / N as f64 * 0.5)).sum::<f64>() / N as f64
    }

    /// Its response squared at f cycles a frame.
    fn response(&self, f: f64) -> f64 {
        let w = 2.0 * PI * f;
        let (c, s) = (w.cos(), -w.sin());
        let mut g = self.gain * self.gain;
        for sh in &self.shelves {
            let num = (sh.b0 + sh.b1 * c).powi(2) + (sh.b1 * s).powi(2);
            let den = (1.0 + sh.a1 * c).powi(2) + (sh.a1 * s).powi(2);
            g *= num / den;
        }
        g
    }

    /// The shelves for the slope now, and the gain that keeps the pivot where it was.
    fn design(&mut self) {
        let v = self.now;
        let lo = if self.above { self.pivot } else { self.pivot * (-BELOW).exp2() };
        let hi = (self.pivot * ABOVE.exp2()).min(0.45);
        let p = (hi / lo).log2().max(0.0) / SECTIONS as f64;
        // A pole and a zero this many octaves apart fall at 6 dB an octave between them, so
        // one of these in each `p` octaves falls at v on average.
        let d = (p * v.abs() / (20.0 * 2f64.log10())).min(p);
        let w = 2.0 * PI * self.pivot;
        let (mut re, mut im) = (1.0f64, 0.0f64);
        for (k, s) in self.shelves.iter_mut().enumerate() {
            let c = lo * (p * (k as f64 + 0.5)).exp2();
            let (fp, fz) = if v < 0.0 {
                (c * (-d / 2.0).exp2(), c * (d / 2.0).exp2())
            } else {
                (c * (d / 2.0).exp2(), c * (-d / 2.0).exp2())
            };
            let (wz, wp) = ((PI * fz).tan(), (PI * fp).tan());
            let a0 = 1.0 / wp + 1.0;
            s.b0 = (1.0 / wz + 1.0) / a0;
            s.b1 = (1.0 - 1.0 / wz) / a0;
            s.a1 = (1.0 - 1.0 / wp) / a0;
            // Its response at the pivot, multiplied in.
            let (cr, ci) = (w.cos(), -w.sin());
            let (nr, ni) = (s.b0 + s.b1 * cr, s.b1 * ci);
            let (dr, di) = (1.0 + s.a1 * cr, s.a1 * ci);
            let (hr, hi) =
                ((nr * dr + ni * di) / (dr * dr + di * di), (ni * dr - nr * di) / (dr * dr + di * di));
            (re, im) = (re * hr - im * hi, re * hi + im * hr);
        }
        // Above: flat below the pivot, where every shelf passes what it's given (as at DC).
        self.gain = if v == 0.0 || self.above { 1.0 } else { 1.0 / (re * re + im * im).sqrt() };
    }
}

impl Shelf {
    #[inline]
    fn step(&mut self, ch: usize, x: f64) -> f64 {
        let y = self.b0 * x + self.b1 * self.x1[ch] - self.a1 * self.y1[ch];
        self.x1[ch] = x;
        self.y1[ch] = y;
        y
    }
}

fn clamp(db: f64) -> f64 {
    if db.is_finite() { db.clamp(-MAX_TILT, MAX_TILT) } else { 0.0 }
}
