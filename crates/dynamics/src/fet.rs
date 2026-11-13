//! The FET compressor (step 7 of `specs/shared.md`, 2026-11-12; the spec is `docs/spec-fet.md`,
//! the shape `RFC-fet.md`). Its RACK voicing; the KT76's comes after Warren's unit is measured.
//!
//! **Input → cell → stage (its DC taken out at 2 Hz) → Output.** Feedback, by the core's loop: the peak of the
//! cell's last output, before the stage, drives a reduction whose attack (20 µs to 800 µs) is
//! corrected for the loop's gain, so the dial's time is what a step measures at any ratio, and
//! whose release (50 ms to 1.1 s) isn't, so it lets go faster while the signal stays hot. Then an
//! offset tanh stage, unity for a quiet signal, its drive rising with the reduction: more colour
//! the harder it works. ALL is the all-buttons state.

use crate::{FADE_S, Fade, Follower, Knee, RAMP_S, Ramp, gain, peak_db, samples};
use as_biquad::First;
use as_control::{Control, Kind, Law, Scale, Shows, Unit};

#[cfg(test)]
mod tests;

/// One ratio button.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Button {
    pub ratio: f64,
    /// The knee's whole width, dB.
    pub knee: f64,
    /// Added to the threshold, dB.
    pub threshold: f64,
    /// What both times are multiplied by.
    pub times: f64,
}

/// A voicing's figures (`docs/spec.md`, "Voicings").
#[derive(Clone, Copy, Debug)]
pub struct Figures {
    pub name: &'static str,
    /// The threshold at Input 0, dBFS, peak, on the output side.
    pub threshold: f64,
    /// The stage: `k = drive · (1 + per_db · ρ)`, offset `b`.
    pub drive: f64,
    pub per_db: f64,
    pub offset: f64,
    /// The DC-blocking high-pass after the stage, Hz.
    pub highpass: f64,
    pub buttons: &'static [Button],
    pub controls: &'static [Control],
}

/// The attack dial's law: 1 is 800 µs, 7 is 20 µs, log between.
pub fn attack_time(dial: f64) -> f64 {
    800e-6 * libm::pow(20.0 / 800.0, (dial - 1.0) / 6.0)
}

/// The attack dial for a time.
pub fn attack_dial(t: f64) -> f64 {
    1.0 + 6.0 * libm::log(t.max(1e-9) / 800e-6) / libm::log(20.0 / 800.0)
}

/// The release dial's law: 1 is 1.1 s, 7 is 50 ms, log between.
pub fn release_time(dial: f64) -> f64 {
    1.1 * libm::pow(0.05 / 1.1, (dial - 1.0) / 6.0)
}

/// The release dial for a time.
pub fn release_dial(t: f64) -> f64 {
    1.0 + 6.0 * libm::log(t.max(1e-9) / 1.1) / libm::log(0.05 / 1.1)
}

const ON: &[&str] = &["out", "in"];
const RATIOS: &[&str] = &["4", "8", "12", "20", "ALL"];

const RACK_CONTROLS: &[Control] = &[
    Control {
        id: "in",
        short: "ON",
        long: "FET in",
        kind: Kind::Switch { positions: ON, ratios: false },
        default: 0.0,
        detent: 0.0,
    },
    Control {
        id: "input",
        short: "IN",
        long: "FET input",
        kind: Kind::Value { lo: 0.0, hi: 40.0, scale: Scale::Linear, fine: 0.5, coarse: 3.0, unit: Unit::Db },
        default: 0.0,
        detent: 0.0,
    },
    Control {
        id: "output",
        short: "OUT",
        long: "FET output",
        kind: Kind::Value {
            lo: -20.0,
            hi: 20.0,
            scale: Scale::Linear,
            fine: 0.5,
            coarse: 3.0,
            unit: Unit::Db,
        },
        default: 0.0,
        detent: 0.0,
    },
    Control {
        id: "attack",
        short: "ATK",
        long: "FET attack",
        kind: Kind::Value {
            lo: 1.0,
            hi: 7.0,
            scale: Scale::Linear,
            fine: 0.1,
            coarse: 1.0,
            unit: Unit::Dial(Law { shows: Shows::Seconds, value: attack_time, dial: attack_dial }),
        },
        default: 3.0,
        detent: 3.0,
    },
    Control {
        id: "release",
        short: "REL",
        long: "FET release",
        kind: Kind::Value {
            lo: 1.0,
            hi: 7.0,
            scale: Scale::Linear,
            fine: 0.1,
            coarse: 1.0,
            unit: Unit::Dial(Law { shows: Shows::Seconds, value: release_time, dial: release_dial }),
        },
        default: 5.0,
        detent: 5.0,
    },
    Control {
        id: "ratio",
        short: "RATIO",
        long: "FET ratio",
        kind: Kind::Switch { positions: RATIOS, ratios: true },
        default: 0.0,
        detent: 0.0,
    },
];

/// The RACK voicing (`docs/spec-fet.md`): the 1176's published figures, and starting figures for
/// the stage; their character is Warren's ear's.
pub const RACK: Figures = Figures {
    name: "rack",
    threshold: -18.0,
    drive: 0.6,
    per_db: 0.05,
    offset: 0.05,
    highpass: 2.0,
    buttons: &[
        Button { ratio: 4.0, knee: 6.0, threshold: 0.0, times: 1.0 },
        Button { ratio: 8.0, knee: 5.0, threshold: 0.0, times: 1.0 },
        Button { ratio: 12.0, knee: 4.0, threshold: 0.0, times: 1.0 },
        Button { ratio: 20.0, knee: 3.0, threshold: 0.0, times: 1.0 },
        Button { ratio: 20.0, knee: 10.0, threshold: 3.0, times: 0.5 },
    ],
    controls: RACK_CONTROLS,
};

/// The FET's controls, RACK's.
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

/// RACK's knobs. The KT76's will sit beside them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rack {
    /// dB.
    pub input: f64,
    /// dB.
    pub output: f64,
    /// The dials, 1 to 7.
    pub attack: f64,
    pub release: f64,
    /// The button in, an index into [`RACK`]'s buttons (4 is ALL).
    pub ratio: usize,
}

/// The FET's settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub on: bool,
    pub voicing: Voicing,
    pub rack: Rack,
}

impl Default for Settings {
    fn default() -> Settings {
        let rack = Rack { input: 0.0, output: 0.0, attack: 1.0, release: 1.0, ratio: 0 };
        let mut s = Settings { on: false, voicing: Voicing::Rack, rack };
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
            "input" => self.rack.input,
            "output" => self.rack.output,
            "attack" => self.rack.attack,
            "release" => self.rack.release,
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
                "ratio" => s.rack.ratio.min(RACK.buttons.len() - 1) as f64,
                _ => s.get(c.id).unwrap_or(c.default),
            };
            s.put(c.id, c.clamp(v));
        }
        s
    }

    fn put(&mut self, id: &str, v: f64) -> bool {
        match id {
            "in" => self.on = v >= 0.5,
            "input" => self.rack.input = v,
            "output" => self.rack.output = v,
            "attack" => self.rack.attack = v,
            "release" => self.rack.release = v,
            "ratio" => self.rack.ratio = v as usize,
            _ => return false,
        }
        true
    }

    /// The button in.
    pub fn button(&self) -> Button {
        let f = self.voicing.figures();
        f.buttons[self.rack.ratio.min(f.buttons.len() - 1)]
    }

    /// The threshold, dBFS, peak, on the output side: the voicing's, and the button's offset.
    pub fn threshold(&self) -> f64 {
        self.voicing.figures().threshold + self.button().threshold
    }

    /// The attack and release in force, seconds: the dials', and the button's multiple.
    pub fn times(&self) -> (f64, f64) {
        let m = self.button().times;
        (attack_time(self.rack.attack) * m, release_time(self.rack.release) * m)
    }

    fn knee(&self) -> Knee {
        let b = self.button();
        Knee { threshold: self.threshold(), ratio: b.ratio, width: b.knee }
    }
}

/// The static curve: the output level for a steady input level, dB, peak, at the Input and button
/// set, Output included, the stage not (a colour, not a level). Off the audio thread.
pub fn curve(s: &Settings, level_db: f64) -> f64 {
    let s = s.held();
    s.knee().curve(level_db + s.rack.input) + s.rack.output
}

/// The stage: an offset tanh, unity at 0 and passing 0, its drive rising with the reduction `rho`
/// (dB). `stage(0, ρ) = 0` exactly.
#[inline]
pub fn stage(f: &Figures, v: f64, rho: f64) -> f64 {
    let k = f.drive * (1.0 + f.per_db * rho);
    let tb = libm::tanh(k * f.offset);
    (libm::tanh(k * (v + f.offset)) - tb) / (k * (1.0 - tb * tb))
}

/// What the last block read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Readouts {
    pub in_peak_db: f64,
    pub out_peak_db: f64,
    /// The reduction at the block's last sample, positive dB.
    pub gr_db: f64,
}

/// A filter state below this is set to exactly 0.
const FLUSH: f64 = 1e-200;

/// The FET, one or two channels.
pub struct Fet {
    rate: f64,
    follower: Follower,
    /// The peak level of the cell's last output, before the stage, dB.
    heard: f64,
    input: Ramp,
    output: Ramp,
    thr: Ramp,
    on: Fade<bool>,
    hp: First,
    /// The high-pass's last input and output, each channel.
    hp_mem: [[f64; 2]; 2],
    sent: Settings,
    rho: f64,
    readouts: Readouts,
    fresh: bool,
    /// The stage and its high-pass skipped: the cell alone, for the tests.
    #[cfg(test)]
    bare: bool,
}

impl Fet {
    /// A FET at `rate` Hz, at its defaults (out).
    pub fn new(rate: f64) -> Fet {
        let s = Settings::default();
        let (a, r) = s.times();
        Fet {
            rate,
            follower: Follower::new(rate, a, r, false),
            heard: -300.0,
            input: Ramp::new(1.0, samples(RAMP_S, rate)),
            output: Ramp::new(1.0, samples(RAMP_S, rate)),
            thr: Ramp::new(s.threshold(), samples(RAMP_S, rate)),
            on: Fade::new(s.on, samples(FADE_S, rate)),
            hp: First::highpass(s.voicing.figures().highpass, rate),
            hp_mem: [[0.0; 2]; 2],
            sent: s,
            rho: 0.0,
            readouts: Readouts { in_peak_db: -300.0, out_peak_db: -300.0, gr_db: 0.0 },
            fresh: true,
            #[cfg(test)]
            bare: false,
        }
    }

    /// New settings, held to their ranges, in force from the first sample of the next `process`.
    /// A unit that has run nothing yet takes them at once. Input and Output ramp in amplitude over
    /// 5 ms, the threshold (ALL's is 3 dB up) in dB; In crossfades over 10 ms; new times and a new
    /// button's ratio and knee take effect at once, the reduction gliding to its new target.
    /// Allocates nothing; may run on the audio thread.
    pub fn set(&mut self, s: &Settings) {
        let s = s.held();
        let (a, r) = s.times();
        if s.times() != self.sent.times() || self.fresh {
            self.follower.times(self.rate, a, r);
        }
        self.sent = s;
        let (i, o, t) = (db_gain(s.rack.input), db_gain(s.rack.output), s.threshold());
        if self.fresh {
            self.input.jump(i);
            self.output.jump(o);
            self.thr.jump(t);
            self.on.jump(s.on);
            return;
        }
        if i != self.input.target() {
            self.input.retarget(i);
        }
        if o != self.output.target() {
            self.output.retarget(o);
        }
        if t != self.thr.target() {
            self.thr.retarget(t);
        }
        self.on.set(s.on);
    }

    /// The settings last sent, held: in force once [`Fet::settled`].
    pub fn settings(&self) -> Settings {
        self.sent
    }

    /// Whether every ramp and crossfade has ended and nothing waits.
    pub fn settled(&self) -> bool {
        self.input.done() && self.output.done() && self.thr.done() && self.on.done()
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

    /// Two channels, in place, one detector for both.
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
        self.readouts = Readouts { in_peak_db: peak_db(pin), out_peak_db: peak_db(pout), gr_db: self.rho };
    }

    #[inline]
    fn tick<const C: usize>(&mut self, x: [f64; C]) -> [f64; C] {
        self.fresh = false;
        let f = self.sent.voicing.figures();
        let b = self.sent.button();
        let knee = Knee { threshold: self.thr.advance(), ratio: b.ratio, width: b.knee };
        let l = self.heard;
        self.rho = self.follower.step(knee.fb(l), knee.fb_slope(l));
        let g = gain(self.rho);
        let ig = self.input.advance();
        let og = self.output.advance();
        let (from, to, t) = self.on.advance();
        let mut peak = 0.0f64;
        let mut y = x;
        for c in 0..C {
            let v = x[c] * ig * g;
            peak = peak.max(v.abs());
            // The stage, and the 2 Hz high-pass on what it adds alone (its DC), its state flushed:
            // the input's own low end isn't filtered, so the filter's settling after a low onset
            // can't lift a quiet tail (`docs/spec-fet.md`, "As built").
            let d = stage(f, v, self.rho) - v;
            let m = &mut self.hp_mem[c];
            let h = self.hp.b0 * d + self.hp.b1 * m[0] - self.hp.a1 * m[1];
            let flush = |v: f64| if v.abs() < FLUSH { 0.0 } else { v };
            *m = [flush(d), flush(h)];
            #[cfg(test)]
            let w = if self.bare { v } else { v + m[1] };
            #[cfg(not(test))]
            let w = v + m[1];
            let wet = w * og;
            let pick = |on: bool| if on { wet } else { x[c] };
            y[c] = if t == 1.0 { pick(to) } else { (1.0 - t) * pick(from) + t * pick(to) };
        }
        self.heard = peak_db(peak);
        y
    }
}

fn db_gain(db: f64) -> f64 {
    libm::pow(10.0, db / 20.0)
}
