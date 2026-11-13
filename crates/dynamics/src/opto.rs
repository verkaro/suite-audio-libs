//! The optical compressor (step 6 of `specs/shared.md`, 2026-11-12; the spec is
//! `docs/spec-opto.md`, the shape `RFC-opto.md`). Its RACK voicing; the LA-4's comes after
//! Warren's unit is measured.
//!
//! Feedback, by the core's loop: a light (a 1 ms mean square of the cell's output) drives a cell
//! whose reduction is carried by two envelopes, a fast one toward the target and a slow one toward
//! half of it, the reduction the larger. The attack is faster the harder the light is driven; the
//! fast part lets half go in about 40 ms and the slow part holds the rest for 0.5 s to 5 s, longer
//! the more the cell has been working (its memory). Both are corrected for the loop's gain, so the
//! cell's times are its own at any level: no pumping on dialogue.

use crate::{FADE_S, Fade, Follower, Knee, MeanSquare, RAMP_S, Ramp, gain, one_pole, samples};
use as_control::{Control, Kind, Law, Scale, Shows, Unit};

#[cfg(test)]
mod tests;

/// One position of the ratio switch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Position {
    pub ratio: f64,
    /// The knee's whole width, dB.
    pub knee: f64,
    /// What the attack time is multiplied by, after it's held to its range.
    pub attack: f64,
    /// The memory's scale `M`, dB: `held = min(m / M, 1)`.
    pub memory: f64,
}

/// A voicing's figures (`docs/spec.md`, "Voicings"): every time, law and position, and the controls
/// it's set by. A second voicing is a second `Figures`, not a second compressor.
#[derive(Clone, Copy, Debug)]
pub struct Figures {
    pub name: &'static str,
    /// The light: a symmetric one-pole on the square, seconds.
    pub light: f64,
    /// The attack: `attack · sqrt(attack_at / max(o, 0.1))`, held between `attack_min` and
    /// `attack_max`, `o` the output's level over the threshold, dB.
    pub attack: f64,
    pub attack_at: f64,
    pub attack_min: f64,
    pub attack_max: f64,
    /// The fast envelope's release, seconds.
    pub fast: f64,
    /// The slow envelope's release, `slow · 10^held`, seconds.
    pub slow: f64,
    /// The memory: a symmetric one-pole on the reduction, seconds.
    pub memory: f64,
    pub positions: &'static [Position],
    pub controls: &'static [Control],
}

/// Reduce's law: the threshold, dBFS, for Reduce 0 to 100.
pub fn threshold(reduce: f64) -> f64 {
    10.0 - 50.0 * libm::pow((reduce / 100.0).clamp(0.0, 1.0), 0.6)
}

/// Reduce for a threshold: the law back.
pub fn reduce_for(threshold: f64) -> f64 {
    100.0 * libm::pow(((10.0 - threshold) / 50.0).max(0.0), 1.0 / 0.6)
}

const IN: &[&str] = &["out", "in"];
const RATIOS: &[&str] = &["2A", "2", "4", "8", "12", "20"];

const RACK_CONTROLS: &[Control] = &[
    Control {
        id: "in",
        short: "IN",
        long: "Optical in",
        kind: Kind::Switch { positions: IN, ratios: false },
        default: 0.0,
        detent: 0.0,
    },
    Control {
        id: "reduce",
        short: "REDUCE",
        long: "Optical reduce",
        kind: Kind::Value {
            lo: 0.0,
            hi: 100.0,
            scale: Scale::Linear,
            fine: 1.0,
            coarse: 5.0,
            unit: Unit::Dial(Law { shows: Shows::Dbfs, value: threshold, dial: reduce_for }),
        },
        default: 40.0,
        detent: 0.0,
    },
    Control {
        id: "gain",
        short: "GAIN",
        long: "Optical gain",
        kind: Kind::Value {
            lo: -10.0,
            hi: 30.0,
            scale: Scale::Linear,
            fine: 0.5,
            coarse: 3.0,
            unit: Unit::Db,
        },
        default: 0.0,
        detent: 0.0,
    },
    Control {
        id: "ratio",
        short: "RATIO",
        long: "Optical ratio",
        kind: Kind::Switch { positions: RATIOS, ratios: true },
        default: 2.0,
        detent: 2.0,
    },
];

/// The RACK voicing (`docs/spec-opto.md`): starting figures, chosen to meet the RFC's shape; their
/// character is Warren's ear's.
pub const RACK: Figures = Figures {
    name: "rack",
    light: 1e-3,
    attack: 10e-3,
    attack_at: 6.0,
    attack_min: 3e-3,
    attack_max: 30e-3,
    fast: 0.060,
    slow: 0.5,
    memory: 2.0,
    positions: &[
        Position { ratio: 3.0, knee: 20.0, attack: 1.5, memory: 6.0 },
        Position { ratio: 2.0, knee: 18.0, attack: 1.0, memory: 10.0 },
        Position { ratio: 4.0, knee: 15.0, attack: 1.0, memory: 10.0 },
        Position { ratio: 8.0, knee: 13.0, attack: 1.0, memory: 10.0 },
        Position { ratio: 12.0, knee: 12.0, attack: 1.0, memory: 10.0 },
        Position { ratio: 20.0, knee: 11.0, attack: 1.0, memory: 10.0 },
    ],
    controls: RACK_CONTROLS,
};

/// The optical's controls, RACK's.
pub fn controls() -> &'static [Control] {
    RACK.controls
}

/// Which voicing is in. RACK is all that's built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Voicing {
    #[default]
    Rack,
}

impl Voicing {
    pub fn figures(self) -> &'static Figures {
        match self {
            Voicing::Rack => &RACK,
        }
    }
}

/// RACK's knobs. The LA-4's will sit beside them, so flipping finds each as it was left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rack {
    pub reduce: f64,
    /// Make-up, dB.
    pub gain: f64,
    /// The ratio switch's position, an index into [`RACK`]'s positions.
    pub ratio: usize,
}

/// The optical's settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub on: bool,
    pub voicing: Voicing,
    pub rack: Rack,
}

impl Default for Settings {
    fn default() -> Settings {
        let mut s =
            Settings { on: false, voicing: Voicing::Rack, rack: Rack { reduce: 0.0, gain: 0.0, ratio: 0 } };
        for c in s.voicing.figures().controls {
            s.put(c.id, c.default);
        }
        s
    }
}

impl Settings {
    /// The value of a control of the voicing in, by its id.
    pub fn get(&self, id: &str) -> Option<f64> {
        as_control::find(self.voicing.figures().controls, id)?;
        Some(match id {
            "in" => f64::from(u8::from(self.on)),
            "reduce" => self.rack.reduce,
            "gain" => self.rack.gain,
            "ratio" => self.rack.ratio as f64,
            _ => return None,
        })
    }

    /// Sets a control of the voicing in by its id, held to its range. False if it has none such.
    pub fn set_by_id(&mut self, id: &str, v: f64) -> bool {
        match as_control::find(self.voicing.figures().controls, id) {
            Some(c) => self.put(id, c.clamp(v)),
            None => false,
        }
    }

    /// Every control held to its range.
    pub fn held(&self) -> Settings {
        let mut s = *self;
        for c in self.voicing.figures().controls {
            let v = match c.id {
                "ratio" => s.rack.ratio.min(RACK.positions.len() - 1) as f64,
                _ => s.get(c.id).unwrap_or(c.default),
            };
            s.put(c.id, c.clamp(v));
        }
        s
    }

    fn put(&mut self, id: &str, v: f64) -> bool {
        match id {
            "in" => self.on = v >= 0.5,
            "reduce" => self.rack.reduce = v,
            "gain" => self.rack.gain = v,
            "ratio" => self.rack.ratio = v as usize,
            _ => return false,
        }
        true
    }

    /// The threshold, dBFS.
    pub fn threshold(&self) -> f64 {
        threshold(self.rack.reduce)
    }

    /// The ratio switch's position.
    pub fn position(&self) -> Position {
        let f = self.voicing.figures();
        f.positions[self.rack.ratio.min(f.positions.len() - 1)]
    }

    fn knee(&self) -> Knee {
        let p = self.position();
        Knee { threshold: self.threshold(), ratio: p.ratio, width: p.knee }
    }
}

/// The static curve: the output level for a steady input level, dB, at the ratio and Reduce set,
/// with Gain. Off the audio thread.
pub fn curve(s: &Settings, level_db: f64) -> f64 {
    let s = s.held();
    s.knee().curve(level_db) + s.rack.gain
}

/// What the last block read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Readouts {
    pub in_peak_db: f64,
    pub out_peak_db: f64,
    /// The reduction at the block's last sample, positive dB.
    pub gr_db: f64,
    /// The memory, 0 to 1: how much the slow release is held.
    pub held: f64,
}

/// The cell: the light, two envelopes and the memory.
#[derive(Clone, Copy, Debug)]
struct Cell {
    light: MeanSquare,
    fast: Follower,
    slow: Follower,
    mem: f64,
    mem_c: f64,
    held: f64,
    /// The `held` the slow release was last timed for.
    timed: f64,
}

/// The memory falling below this, in dB, is set to exactly 0: `held` is then 0 after a minute of silence,
/// where 1e-12 needs longer (`docs/spec-opto.md`, "As built"). It moves `τ_s` by parts in 10⁷.
const MEM_FLOOR: f64 = 1e-6;

impl Cell {
    fn new(f: &Figures, rate: f64) -> Cell {
        Cell {
            light: MeanSquare::new(rate, f.light, f.light),
            fast: Follower::new(rate, f.attack, f.fast, true),
            slow: Follower::new(rate, f.attack, f.slow, true),
            mem: 0.0,
            mem_c: one_pole(f.memory, rate),
            held: 0.0,
            timed: 0.0,
        }
    }

    /// One sample: the light hears the cell's last output (the larger square), and the reduction
    /// moves. Returns it, dB.
    #[inline]
    fn step(&mut self, heard: f64, knee: &Knee, p: &Position, f: &Figures, rate: f64) -> f64 {
        self.light.feed(heard);
        let l = self.light.db();
        let (target, slope) = (knee.fb(l), knee.fb_slope(l));
        let (fast_up, slow_up) = (target > self.fast.rho(), 0.5 * target > self.slow.rho());
        if fast_up || slow_up {
            let o = (l - knee.threshold).max(0.1);
            let tau = (f.attack * libm::sqrt(f.attack_at / o)).clamp(f.attack_min, f.attack_max) * p.attack;
            if fast_up {
                self.fast.attack_time(rate, tau);
            }
            if slow_up {
                self.slow.attack_time(rate, tau);
            }
        }
        if !slow_up && self.held != self.timed {
            self.slow.release_time(rate, f.slow * libm::pow(10.0, self.held));
            self.timed = self.held;
        }
        let rho = self.fast.step(target, slope).max(self.slow.step(0.5 * target, slope));
        // The floor while falling only, as the reduction's is: a rise of `mem_c·ρ` a sample is
        // under it for a small ρ, and would never start.
        let falling = rho < self.mem;
        self.mem += self.mem_c * (rho - self.mem);
        if falling && self.mem < MEM_FLOOR {
            self.mem = 0.0;
        }
        self.held = (self.mem / p.memory).min(1.0);
        rho
    }
}

/// The optical, one or two channels.
pub struct Opto {
    rate: f64,
    cell: Cell,
    /// The cell's last output's larger square, before make-up.
    heard: f64,
    thr: Ramp,
    makeup: Ramp,
    on: Fade<bool>,
    sent: Settings,
    rho: f64,
    readouts: Readouts,
    fresh: bool,
}

impl Opto {
    /// An optical at `rate` Hz, at its defaults (out).
    pub fn new(rate: f64) -> Opto {
        let s = Settings::default();
        Opto {
            rate,
            cell: Cell::new(s.voicing.figures(), rate),
            heard: 0.0,
            thr: Ramp::new(s.threshold(), samples(RAMP_S, rate)),
            makeup: Ramp::new(1.0, samples(RAMP_S, rate)),
            on: Fade::new(s.on, samples(FADE_S, rate)),
            sent: s,
            rho: 0.0,
            readouts: Readouts { in_peak_db: -300.0, out_peak_db: -300.0, gr_db: 0.0, held: 0.0 },
            fresh: true,
        }
    }

    /// New settings, held to their ranges, in force from the first sample of the next `process`.
    /// A unit that has run nothing yet takes them at once. The threshold ramps in dB and Gain in
    /// amplitude over 5 ms; In crossfades over 10 ms; a new ratio is a new target, which the cell
    /// glides to. Allocates nothing; may run on the audio thread.
    pub fn set(&mut self, s: &Settings) {
        let s = s.held();
        self.sent = s;
        let (t, m) = (s.threshold(), libm::pow(10.0, s.rack.gain / 20.0));
        if self.fresh {
            self.thr.jump(t);
            self.makeup.jump(m);
            self.on.jump(s.on);
            return;
        }
        if t != self.thr.target() {
            self.thr.retarget(t);
        }
        if m != self.makeup.target() {
            self.makeup.retarget(m);
        }
        self.on.set(s.on);
    }

    /// The settings last sent, held: in force once [`Opto::settled`].
    pub fn settings(&self) -> Settings {
        self.sent
    }

    /// Whether every ramp and crossfade has ended and nothing waits. (The cell's own gliding isn't
    /// a setting's.)
    pub fn settled(&self) -> bool {
        self.thr.done() && self.makeup.done() && self.on.done()
    }

    pub fn readouts(&self) -> Readouts {
        self.readouts
    }

    /// One channel, in place.
    pub fn process(&mut self, x: &mut [f64]) {
        let (mut pin, mut pout) = (0.0f64, 0.0f64);
        for v in x.iter_mut() {
            pin = pin.max(v.abs());
            let [y] = self.tick([*v]);
            pout = pout.max(y.abs());
            *v = y;
        }
        self.read(pin, pout);
    }

    /// Two channels, in place, one light for both.
    pub fn process2(&mut self, l: &mut [f64], r: &mut [f64]) {
        let (mut pin, mut pout) = (0.0f64, 0.0f64);
        for (a, b) in l.iter_mut().zip(r.iter_mut()) {
            pin = pin.max(a.abs()).max(b.abs());
            let [y, z] = self.tick([*a, *b]);
            pout = pout.max(y.abs()).max(z.abs());
            (*a, *b) = (y, z);
        }
        self.read(pin, pout);
    }

    fn read(&mut self, pin: f64, pout: f64) {
        self.readouts = Readouts {
            in_peak_db: crate::peak_db(pin),
            out_peak_db: crate::peak_db(pout),
            gr_db: self.rho,
            held: self.cell.held,
        };
    }

    #[inline]
    fn tick<const C: usize>(&mut self, x: [f64; C]) -> [f64; C] {
        self.fresh = false;
        let f = self.sent.voicing.figures();
        let p = self.sent.position();
        let knee = Knee { threshold: self.thr.advance(), ratio: p.ratio, width: p.knee };
        self.rho = self.cell.step(self.heard, &knee, &p, f, self.rate);
        let g = gain(self.rho);
        let cell = x.map(|v| v * g);
        self.heard = cell.iter().fold(0.0f64, |m, v| m.max(v * v));
        let m = self.makeup.advance();
        let (from, to, t) = self.on.advance();
        let mut y = x;
        for c in 0..C {
            let pick = |on: bool| if on { cell[c] * m } else { x[c] };
            y[c] = if t == 1.0 { pick(to) } else { (1.0 - t) * pick(from) + t * pick(to) };
        }
        y
    }
}
