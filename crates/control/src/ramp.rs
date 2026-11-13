//! A setting's ramp, so nothing clicks (`specs/shared.md`, "Ramps"): linear from where it is to
//! its target over a whole number of samples, the same at any block size. A gain ramps in
//! amplitude, a level setting in dB; both are this. Moved here from `as-eq` 2026-11-12, when
//! `as-dynamics` became its second user. No audio: a value a sample.

/// A value moving linearly to a target over `n` samples.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ramp {
    now: f64,
    from: f64,
    to: f64,
    k: usize,
    n: usize,
}

impl Ramp {
    /// At `value`, settled, ramping over `n` samples when retargeted (`round(t · rate)`).
    pub fn new(value: f64, n: usize) -> Ramp {
        Ramp { now: value, from: value, to: value, k: n, n }
    }

    /// A new target: a new ramp of `n` samples from where the value is (at once, if `n` is 0).
    pub fn retarget(&mut self, to: f64) {
        if self.n == 0 {
            self.jump(to);
        } else {
            (self.from, self.to, self.k) = (self.now, to, 0);
        }
    }

    /// At `value` at once, settled.
    pub fn jump(&mut self, value: f64) {
        (self.now, self.from, self.to, self.k) = (value, value, value, self.n);
    }

    /// The next sample's value. The last step lands on the target exactly.
    #[inline]
    pub fn advance(&mut self) -> f64 {
        if self.k < self.n {
            self.k += 1;
            self.now = if self.k == self.n {
                self.to
            } else {
                self.from + (self.to - self.from) * (self.k as f64 / self.n as f64)
            };
        }
        self.now
    }

    /// The value now.
    pub fn now(&self) -> f64 {
        self.now
    }

    /// Where it's going.
    pub fn target(&self) -> f64 {
        self.to
    }

    /// Whether it has reached its target.
    pub fn done(&self) -> bool {
        self.k == self.n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ramp_lands_on_its_target_and_retargets_from_where_it_is() {
        let mut r = Ramp::new(1.0, 4);
        assert!(r.done());
        assert_eq!(r.advance(), 1.0);
        r.retarget(0.0);
        assert_eq!([r.advance(), r.advance()], [0.75, 0.5]);
        r.retarget(1.0);
        assert!(!r.done());
        assert_eq!([r.advance(), r.advance(), r.advance(), r.advance()], [0.625, 0.75, 0.875, 1.0]);
        assert!(r.done());
        assert_eq!(r.advance(), 1.0);
        r.jump(-6.0);
        assert!(r.done());
        assert_eq!((r.now(), r.target()), (-6.0, -6.0));
    }

    #[test]
    fn a_ramp_of_no_samples_is_a_jump() {
        let mut r = Ramp::new(0.0, 0);
        r.retarget(3.0);
        assert!(r.done());
        assert_eq!((r.now(), r.advance()), (3.0, 3.0));
    }
}
