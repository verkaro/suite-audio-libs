//! The opening between the outside and the room (moved into the suite from place's
//! `crates/core/src/opening.rs`, `VENDORED.md`; place's `docs/design.md`, "Sets from the library"):
//! three stops, SHUT, CRACKED and OPEN, each three numbers Warren sets: what comes through the
//! wall, what comes past the opening, and what of it goes into the room.
//!
//! Through the wall comes the outside muffled: a low-pass falling 6 dB an octave above the
//! wall's corner (the mass law of a single wall), wide across it. Past the opening comes the
//! outside as it is, at the opening on the stage. Each is put there by a matrix of gains,
//! never a delay, and the wall's filter never turns the sound's phase by more than a quarter
//! turn, so wherever the two meet they add to at least the louder of them: nothing combs.
//! Between two stops, every route's gain is the stops' crossfaded: the amount is exactly a
//! crossfade between the stops' stems, as the DAW makes one. (Warren, 2026-10-10: the proof of
//! concept's lows alone through the wall, split off at 250 Hz, let no crowd through it.)

use std::f64::consts::{FRAC_PI_4, PI};

#[cfg(test)]
mod tests;

/// What a wall is made of, named, and its corner, Hz: how muffled it is. Any corner may be set.
pub const WALLS: [(&str, f64); 5] =
    [("paper", 3500.0), ("glass", 800.0), ("plaster", 300.0), ("brick", 120.0), ("concrete", 80.0)];
/// A wall unsaid: glass, a shut window.
pub const WALL_HZ: f64 = 800.0;

/// A wall's corner by its name.
pub fn wall_named(name: &str) -> Option<f64> {
    WALLS.iter().find(|(n, _)| *n == name).map(|(_, hz)| *hz)
}
/// The distance the stops' levels are heard at, metres: nearer, louder; further, quieter.
pub const REF_DISTANCE: f64 = 2.0;

/// One stop: what comes through, dB (None: nothing).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stop {
    /// The lows through the wall, wide.
    pub wall: Option<f64>,
    /// The whole sound past the opening, at the opening.
    pub opening: Option<f64>,
    /// What comes in, sent to the room's IR.
    pub room: Option<f64>,
}

/// The three stops, and where CRACKED is between SHUT (0) and OPEN (1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stops {
    pub cracked_at: f64,
    pub shut: Stop,
    pub cracked: Stop,
    pub open: Stop,
}

impl Default for Stops {
    /// From the proof of concept's curve: shut, the lows through the wall at −25 dB; cracked,
    /// the opening at −7 dB, a crack narrow, not quiet; open, the opening whole.
    fn default() -> Stops {
        Stops {
            cracked_at: 0.2,
            shut: Stop { wall: Some(-25.0), opening: None, room: Some(-10.0) },
            cracked: Stop { wall: Some(-30.0), opening: Some(-7.0), room: Some(-10.0) },
            open: Stop { wall: None, opening: Some(0.0), room: Some(-10.0) },
        }
    }
}

impl Stops {
    /// The stops by name, with where each is.
    pub fn named(&self) -> [(&'static str, f64, &Stop); 3] {
        [("shut", 0.0, &self.shut), ("cracked", self.cracked_at, &self.cracked), ("open", 1.0, &self.open)]
    }

    /// Where `amount` falls: the stops either side, and how far from the first to the second.
    fn between(&self, amount: f64) -> (&Stop, &Stop, f64) {
        let a = amount.clamp(0.0, 1.0);
        let c = self.cracked_at.clamp(1e-6, 1.0 - 1e-6);
        if a <= c {
            (&self.shut, &self.cracked, a / c)
        } else {
            (&self.cracked, &self.open, (a - c) / (1.0 - c))
        }
    }
}

/// The opening as heard: where it is on the stage and how wide it looks (from the mic), how much
/// the direct sound is turned up or down for the mic's distance, the wall it's in, and its stops.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Opening {
    /// The wall's corner, Hz ([`WALLS`]).
    pub wall_hz: f64,
    /// −1 (left) to 1 (right).
    pub pan: f64,
    /// How much of the stage it spans, 0 to 2.
    pub width: f64,
    /// The direct sound for the mic's distance, dB (0 at [`REF_DISTANCE`]).
    pub direct: f64,
    pub stops: Stops,
}

impl Default for Opening {
    fn default() -> Opening {
        Opening { wall_hz: WALL_HZ, pan: 0.0, width: 0.25, direct: 0.0, stops: Stops::default() }
    }
}

/// Where an opening `size` metres across, heard from `distance` metres off at `side` (−1 left
/// to 1 right), is on the stage: (pan, width, direct dB). Its width is the angle it fills, a
/// quarter turn being the whole stage (2); the direct sound falls 6 dB each time the distance
/// doubles from [`REF_DISTANCE`].
pub fn heard_from(size: f64, distance: f64, side: f64) -> (f64, f64, f64) {
    let d = distance.clamp(0.25, 50.0);
    let angle = 2.0 * (size.max(0.0) / (2.0 * d)).atan();
    let width = (angle / (PI / 4.0)).clamp(0.0, 2.0);
    (side.clamp(-1.0, 1.0), width, -20.0 * (d / REF_DISTANCE).log10())
}

/// Two by two gains, `[[l→L, r→L], [l→R, r→R]]`: a stereo image put on the stage.
pub type Matrix = [[f64; 2]; 2];

/// Constant-power gains (to L, to R) for a position from −1 to 1.
pub fn pan_law(p: f64) -> (f64, f64) {
    let t = (p.clamp(-1.0, 1.0) + 1.0) * FRAC_PI_4;
    (t.cos(), t.sin())
}

/// The outside's left and right at the opening's edges.
pub fn window(pan: f64, width: f64) -> Matrix {
    let w = width.clamp(0.0, 2.0) / 2.0;
    let (ll, lr) = pan_law(pan - w);
    let (rl, rr) = pan_law(pan + w);
    [[ll, rl], [lr, rr]]
}

/// The outside's left and right across the whole wall, as they are.
pub const WIDE: Matrix = [[1.0, 0.0], [0.0, 1.0]];

/// What the opening does at an amount: the wall's matrix (on the outside muffled) and the
/// opening's (on the outside as it is), to what's heard direct and to what's sent into the room
/// (summed to one there).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gains {
    pub wall: Matrix,
    pub open: Matrix,
    pub send_wall: Matrix,
    pub send_open: Matrix,
}

/// A level in dB, or nothing, as a gain.
fn gain(d: Option<f64>) -> f64 {
    d.map_or(0.0, db)
}

/// A stop's routes, each the product of its gains: through the wall heard direct, past the
/// opening heard direct, through the wall into the room, past the opening into the room.
fn routes(s: &Stop, direct: f64) -> [f64; 4] {
    let (w, o, r) = (gain(s.wall), gain(s.opening), gain(s.room));
    [w * direct, o * direct, w * r, o * r]
}

/// The gains at `amount` (0 SHUT to 1 OPEN) for `op`, its opening at `win` ([`window`]). Between
/// two stops, each route is theirs crossfaded, so the whole is the stops' sound crossfaded.
pub fn gains(amount: f64, op: &Opening, win: &Matrix) -> Gains {
    let (a, b, t) = op.stops.between(amount);
    let direct = db(op.direct);
    let (ra, rb) = (routes(a, direct), routes(b, direct));
    let p: [f64; 4] = std::array::from_fn(|k| (1.0 - t) * ra[k] + t * rb[k]);
    let z = [[0.0; 2]; 2];
    let mut g = Gains { wall: z, open: z, send_wall: z, send_open: z };
    for i in 0..2 {
        for j in 0..2 {
            g.wall[i][j] = p[0] * WIDE[i][j];
            g.open[i][j] = p[1] * win[i][j];
            g.send_wall[i][j] = p[2] * WIDE[i][j];
            g.send_open[i][j] = p[3] * win[i][j];
        }
    }
    g
}

/// What the page shows of the opening at `amount`, dB (−120 at the least): through the wall,
/// past the opening, into the room, each the stops' crossfaded.
pub fn levels(amount: f64, stops: &Stops) -> [f64; 3] {
    let (a, b, t) = stops.between(amount);
    let to_db = |g: f64| if g > 1e-6 { 20.0 * g.log10() } else { -120.0 };
    let mix = |x: Option<f64>, y: Option<f64>| to_db((1.0 - t) * gain(x) + t * gain(y));
    [mix(a.wall, b.wall), mix(a.opening, b.opening), mix(a.room, b.room)]
}

/// dB to a gain.
pub fn db(d: f64) -> f64 {
    10f64.powf(d / 20.0)
}

/// The wall: a first-order low-pass (the bilinear transform of the analogue one, prewarped),
/// −3 dB at its corner and 6 dB an octave down above it; its phase from 0 to a quarter turn
/// behind, never more.
#[derive(Clone, Copy, Debug, Default)]
pub struct Wall {
    b: f64,
    a: f64,
    x: f64,
    y: f64,
}

impl Wall {
    /// A wall with its corner at `hz`, at `rate`.
    pub fn new(hz: f64, rate: u32) -> Wall {
        let k = (PI * (hz / rate as f64).clamp(1e-6, 0.4999)).tan();
        Wall { b: k / (1.0 + k), a: (k - 1.0) / (k + 1.0), x: 0.0, y: 0.0 }
    }

    /// Another corner, carrying on from where this one is (no step in what it holds).
    pub fn retuned(&self, hz: f64, rate: u32) -> Wall {
        Wall { x: self.x, y: self.y, ..Wall::new(hz, rate) }
    }

    #[inline]
    pub fn step(&mut self, x: f64) -> f64 {
        let y = self.b * (x + self.x) - self.a * self.y;
        self.x = x;
        self.y = y;
        y
    }

    /// Its response at `f` cycles a frame: (real, imaginary).
    pub fn response(&self, f: f64) -> (f64, f64) {
        let (c, s) = ((2.0 * PI * f).cos(), -(2.0 * PI * f).sin());
        // (b + b·z⁻¹) / (1 + a·z⁻¹), z⁻¹ = c + js
        let (nr, ni) = (self.b * (1.0 + c), self.b * s);
        let (dr, di) = (1.0 + self.a * c, self.a * s);
        let d = dr * dr + di * di;
        ((nr * dr + ni * di) / d, (ni * dr - nr * di) / d)
    }
}
