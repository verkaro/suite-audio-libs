//! Turns the engine's peaks into what an LED ladder shows. The ballistics are a display
//! concern, so the engine only reports peaks, and every renderer draws the same result from
//! here. Moved into the suite from place's `place-meter`, itself Dexter's (`VENDORED.md`).

/// The dBFS at which each of the 16 LEDs lights, bottom to top (the user's choice, 2026-09-27):
/// 3 dB steps through the working range, finer near the top. Green to −9, amber from −6 to −2;
/// the top (red) LED warns from −1 dBFS and doesn't latch; a real clip is latched by the engine
/// and shown on the track number.
pub const THRESHOLDS: [f64; LEDS] = [
    -48.0, -42.0, -36.0, -30.0, -27.0, -24.0, -21.0, -18.0, -15.0, -12.0, -9.0, -6.0, -4.5, -3.0, -2.0, -1.0,
];

/// LEDs in a ladder.
pub const LEDS: usize = 16;

/// The deep scale (the user, 2026-09-27): for quiet sources and the noise floor, down to −80.
pub const THRESHOLDS_DEEP: [f64; LEDS] = [
    -80.0, -72.0, -64.0, -56.0, -48.0, -42.0, -36.0, -30.0, -24.0, -18.0, -12.0, -9.0, -6.0, -3.0, -2.0, -1.0,
];

/// The scale's thresholds: deep, or normal.
pub fn thresholds(deep: bool) -> &'static [f64; LEDS] {
    if deep { &THRESHOLDS_DEEP } else { &THRESHOLDS }
}

const FLOOR_DB: f64 = -120.0;

/// How the bar and the peak LED move.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ballistics {
    /// The bar's fall, dB/s.
    pub release: f64,
    /// How long the peak LED holds before it falls, seconds.
    pub hold: f64,
    /// The peak LED's fall after the hold, dB/s.
    pub peak_fall: f64,
    /// On the deep scale ([`THRESHOLDS_DEEP`]).
    pub deep: bool,
}

/// The ballistics without the user's settings.
pub const DEFAULT: Ballistics = Ballistics { release: 40.0, hold: 1.5, peak_fall: 20.0, deep: false };

/// One track's meter.
#[derive(Clone, Copy, Debug)]
pub struct Ladder {
    b: Ballistics,
    level: f64,
    peak: f64,
    hold: f64,
}

impl Ladder {
    /// A ladder at rest.
    pub fn new(b: Ballistics) -> Ladder {
        Ladder { b, level: FLOOR_DB, peak: FLOOR_DB, hold: 0.0 }
    }

    /// Feeds the peak since the last update, dt seconds ago. The attack is instant; the release
    /// and the peak's fall are rate-limited.
    pub fn update(&mut self, peak_db: f64, dt: f64) {
        self.level -= self.b.release * dt;
        if peak_db > self.level {
            self.level = peak_db;
        }
        if self.level < FLOOR_DB {
            self.level = FLOOR_DB;
        }
        if self.level >= self.peak {
            self.peak = self.level;
            self.hold = self.b.hold;
            return;
        }
        if self.hold > 0.0 {
            self.hold -= dt;
            return;
        }
        self.peak -= self.b.peak_fall * dt;
        if self.peak < self.level {
            self.peak = self.level;
        }
    }

    /// The LEDs lit from the bottom, 0–16.
    pub fn lit(&self) -> i32 {
        count(self.level, self.b.deep)
    }

    /// The peak-hold LED (0–15), or −1 for none.
    pub fn peak_led(&self) -> i32 {
        count(self.peak, self.b.deep) - 1
    }
}

fn count(db: f64, deep: bool) -> i32 {
    thresholds(deep).iter().filter(|&&t| db >= t).count() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attack_is_instant() {
        let mut l = Ladder::new(DEFAULT);
        l.update(-7.0, 1.0 / 30.0);
        assert_eq!(l.lit(), 11, "-48 to -9 lit; -6 not");
    }

    #[test]
    fn release_rate() {
        let mut l = Ladder::new(DEFAULT);
        l.update(-1.0, 1.0 / 30.0);
        for _ in 0..30 {
            l.update(-120.0, 1.0 / 30.0); // one second of silence: the bar falls 40 dB
        }
        assert!((-41.5..=-40.5).contains(&l.level), "level after 1 s = {:.1}, want about -41", l.level);
        let mut slow = Ladder::new(Ballistics { release: 10.0, ..DEFAULT });
        slow.update(-1.0, 1.0 / 30.0);
        for _ in 0..30 {
            slow.update(-120.0, 1.0 / 30.0);
        }
        assert!((-11.5..=-10.5).contains(&slow.level), "at 10 dB/s: {:.1}, want about -11", slow.level);
    }

    #[test]
    fn peak_hold_then_fall() {
        let mut l = Ladder::new(DEFAULT);
        l.update(-2.0, 0.01);
        for _ in 0..140 {
            l.update(-120.0, 0.01); // 1.4 s: still held
        }
        assert_eq!(l.peak_led(), 14, "during the hold: -2");
        for _ in 0..100 {
            l.update(-120.0, 0.01); // 1 s after the hold: fallen 18 dB and more
        }
        assert!(l.peak_led() <= 7, "after the fall (below -18): {}", l.peak_led());
    }

    #[test]
    fn top_led() {
        let mut l = Ladder::new(DEFAULT);
        l.update(-1.5, 0.01);
        assert_eq!(l.lit(), 15);
        l.update(-0.8, 0.01);
        assert_eq!(l.lit(), 16, "the top LED warns from -1 dB");
        let l = Ladder::new(DEFAULT);
        assert_eq!((l.lit(), l.peak_led()), (0, -1), "at rest");
    }

    #[test]
    fn deep_scale() {
        let mut l = Ladder::new(Ballistics { deep: true, ..DEFAULT });
        l.update(-60.0, 0.01);
        assert_eq!(l.lit(), 3, "-80 -72 -64");
        l.update(-0.5, 0.01);
        assert_eq!(l.lit(), 16);
        let mut n = Ladder::new(DEFAULT);
        n.update(-60.0, 0.01);
        assert_eq!(n.lit(), 0, "below the normal scale");
    }
}
