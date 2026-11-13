//! The de-esser (step 5 of `specs/shared.md`, 2026-11-12; the spec is `docs/spec-deess.md`, the
//! shape `RFC-deess.md`).
//!
//! A band-pass (a trapezoidal state-variable filter, an octave wide, unity at its centre) hears the
//! ess; two mean-square detectors read the band and the whole, and the reduction follows `ess`,
//! the band over the whole in dB, against the threshold: so the threshold is relative, and a quiet
//! voice is de-essed as a loud one is. Below −61 dBFS on the whole, nothing is taken.
//!
//! **Split** takes the reduction from the band alone, `y = x − (1 − g)·b`, a peaking cut exactly
//! `g` deep at the centre; **wide** takes it from the whole. **Listen** hears the band, or what's
//! taken. One detector for both channels.

use crate::{FADE_S, Fade, Knee, MeanSquare, RAMP_S, Ramp, gain, samples};
use as_control::{Control, Kind, Scale, Unit};
use std::f64::consts::PI;

#[cfg(test)]
mod tests;

/// The gain computer's ratio and knee (`docs/spec-deess.md`, 3).
pub const RATIO: f64 = 10.0;
pub const KNEE_DB: f64 = 6.0;

/// The floor on the whole's level, and how soft it is: the reduction is all there at
/// `FLOOR_DB`, none at `FLOOR_DB − FLOOR_SOFT_DB`.
pub const FLOOR_DB: f64 = -55.0;
pub const FLOOR_SOFT_DB: f64 = 6.0;

/// The detectors' times, in seconds.
pub const BAND_ATTACK: f64 = 0.5e-3;
pub const BAND_RELEASE: f64 = 40e-3;
pub const WHOLE_ATTACK: f64 = 0.5e-3;
pub const WHOLE_RELEASE: f64 = 100e-3;

/// The frequency's glide: linear on `log2(f)` over 10 ms, retargeted from where it is.
/// (`docs/spec-deess.md` had a one-pole; see its "As built".)
const GLIDE_S: f64 = 0.010;

/// A filter state below this is set to exactly 0.
const FLUSH: f64 = 1e-200;

/// The band-pass's Q: an octave between its −3 dB points.
pub fn band_q() -> f64 {
    as_biquad::q_from_octaves(1.0)
}

/// Split or wide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The reduction from the band alone.
    Split,
    /// The reduction from the whole.
    Wide,
}

/// What's heard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Listen {
    Off,
    /// The band the detector hears.
    Band,
    /// What the de-esser takes away.
    Taken,
}

const IN: &[&str] = &["out", "in"];
const MODE: &[&str] = &["split", "wide"];
const LISTEN: &[&str] = &["off", "band", "taken"];

const CONTROLS: &[Control] = &[
    Control {
        id: "in",
        short: "IN",
        long: "De-esser in",
        kind: Kind::Switch { positions: IN, ratios: false },
        default: 0.0,
        detent: 0.0,
    },
    Control {
        id: "freq",
        short: "FREQ",
        long: "De-esser frequency",
        kind: Kind::Value {
            lo: 3000.0,
            hi: 10000.0,
            scale: Scale::Log,
            fine: 1.0 / 12.0,
            coarse: 1.0 / 3.0,
            unit: Unit::Hz,
        },
        default: 6000.0,
        detent: 6000.0,
    },
    Control {
        id: "thr",
        short: "THR",
        long: "De-esser threshold",
        kind: Kind::Value {
            lo: -40.0,
            hi: 0.0,
            scale: Scale::Linear,
            fine: 1.0,
            coarse: 3.0,
            unit: Unit::Db,
        },
        default: -20.0,
        detent: -20.0,
    },
    Control {
        id: "range",
        short: "RANGE",
        long: "De-esser range",
        kind: Kind::Value { lo: 0.0, hi: 12.0, scale: Scale::Linear, fine: 0.5, coarse: 3.0, unit: Unit::Db },
        default: 6.0,
        detent: 6.0,
    },
    Control {
        id: "mode",
        short: "MODE",
        long: "De-esser mode",
        kind: Kind::Switch { positions: MODE, ratios: false },
        default: 0.0,
        detent: 0.0,
    },
    Control {
        id: "listen",
        short: "LISTEN",
        long: "De-esser listen",
        kind: Kind::Switch { positions: LISTEN, ratios: false },
        default: 0.0,
        detent: 0.0,
    },
];

/// The de-esser's controls (`docs/spec-deess.md`, "The controls").
pub fn controls() -> &'static [Control] {
    CONTROLS
}

/// The de-esser's settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub on: bool,
    pub freq: f64,
    pub thr: f64,
    pub range: f64,
    pub mode: Mode,
    pub listen: Listen,
}

impl Default for Settings {
    fn default() -> Settings {
        let mut s =
            Settings { on: false, freq: 0.0, thr: 0.0, range: 0.0, mode: Mode::Split, listen: Listen::Off };
        for c in CONTROLS {
            s.put(c.id, c.default);
        }
        s
    }
}

impl Settings {
    /// The value of a control, by its id.
    pub fn get(&self, id: &str) -> Option<f64> {
        Some(match id {
            "in" => f64::from(u8::from(self.on)),
            "freq" => self.freq,
            "thr" => self.thr,
            "range" => self.range,
            "mode" => self.mode as u8 as f64,
            "listen" => self.listen as u8 as f64,
            _ => return None,
        })
    }

    /// Sets a control by its id, held to its range. False if there's no such control.
    pub fn set_by_id(&mut self, id: &str, v: f64) -> bool {
        match as_control::find(CONTROLS, id) {
            Some(c) => self.put(id, c.clamp(v)),
            None => false,
        }
    }

    /// Every control held to its range.
    pub fn held(&self) -> Settings {
        let mut s = *self;
        for c in CONTROLS {
            let v = s.get(c.id).unwrap_or(c.default);
            s.put(c.id, c.clamp(v));
        }
        s
    }

    fn put(&mut self, id: &str, v: f64) -> bool {
        match id {
            "in" => self.on = v >= 0.5,
            "freq" => self.freq = v,
            "thr" => self.thr = v,
            "range" => self.range = v,
            "mode" => self.mode = if v >= 0.5 { Mode::Wide } else { Mode::Split },
            "listen" => {
                self.listen = match v {
                    v if v >= 1.5 => Listen::Taken,
                    v if v >= 0.5 => Listen::Band,
                    _ => Listen::Off,
                }
            }
            _ => return false,
        }
        true
    }

    fn path(&self) -> Path {
        Path { on: self.on, mode: self.mode, listen: self.listen }
    }
}

/// What the output is made of: In, Mode and Listen, crossfaded together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Path {
    on: bool,
    mode: Mode,
    listen: Listen,
}

impl Path {
    #[inline]
    fn out(self, x: f64, b: f64, g: f64) -> f64 {
        if !self.on {
            return x;
        }
        let y = match self.mode {
            Mode::Split => x - (1.0 - g) * b,
            Mode::Wide => g * x,
        };
        match self.listen {
            Listen::Off => y,
            Listen::Band => b,
            Listen::Taken => x - y,
        }
    }
}

/// What the last block read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Readouts {
    /// The block's largest input sample, both channels, in dB (a sine of peak 1.0 reads 0).
    pub in_peak_db: f64,
    /// The block's largest output sample.
    pub out_peak_db: f64,
    /// The reduction at the block's last sample, positive dB.
    pub gr_db: f64,
    /// Whether the reduction passed 0.1 dB at any sample of the block.
    pub acting: bool,
    /// The band over the whole now, in dB: what the threshold is compared with.
    pub ess_db: f64,
}

/// The band-pass's coefficients at a frequency: the trapezoidal SVF's (Zavalishin's, Simper's).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Svf {
    k: f64,
    a1: f64,
    a2: f64,
    a3: f64,
}

impl Svf {
    fn new(hz: f64, rate: f64) -> Svf {
        let g = libm::tan(PI * hz / rate);
        let k = 1.0 / band_q();
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        Svf { k, a1, a2, a3: g * a2 }
    }

    /// One sample through, `s` its two states: the band, scaled by `k` so the centre is unity.
    #[inline]
    fn run(&self, s: &mut [f64; 2], x: f64) -> f64 {
        let v3 = x - s[1];
        let v1 = self.a1 * s[0] + self.a2 * v3;
        let v2 = s[1] + self.a2 * s[0] + self.a3 * v3;
        s[0] = 2.0 * v1 - s[0];
        s[1] = 2.0 * v2 - s[1];
        for v in s.iter_mut() {
            if v.abs() < FLUSH {
                *v = 0.0;
            }
        }
        self.k * v1
    }
}

/// The split's magnitude in dB at `hz`, at a depth `g` (a gain, 1 for none), from the band-pass's
/// own digital transfer: the bell drawn is the bell heard. At `g = 1` it's 0 dB everywhere.
pub fn response(s: &Settings, rate: f64, hz: f64, g: f64) -> f64 {
    // The trapezoidal SVF is the bilinear transform of `k·s / (s² + k·s + 1)`, prewarped at its
    // centre: with `c = 1/tan(π·f0/rate)`, `k·c·(1 − z⁻²) / (a0 + a1·z⁻¹ + a2·z⁻²)`.
    let s = s.held();
    let c = 1.0 / libm::tan(PI * s.freq / rate);
    let k = 1.0 / band_q();
    let (a0, a1, a2) = (c * c + k * c + 1.0, 2.0 - 2.0 * c * c, c * c - k * c + 1.0);
    let w = 2.0 * PI * hz / rate;
    let (c1, s1, c2, s2) = (libm::cos(w), libm::sin(w), libm::cos(2.0 * w), libm::sin(2.0 * w));
    // e^{−jw} = c1 − j·s1.
    let (nr, ni) = (k * c * (1.0 - c2), k * c * s2);
    let (dr, di) = (a0 + a1 * c1 + a2 * c2, -(a1 * s1 + a2 * s2));
    let den = dr * dr + di * di;
    let (br, bi) = ((nr * dr + ni * di) / den, (ni * dr - nr * di) / den);
    let (hr, hi) = (1.0 - (1.0 - g) * br, -(1.0 - g) * bi);
    10.0 * libm::log10((hr * hr + hi * hi).max(1e-30))
}

/// The de-esser, one or two channels.
pub struct DeEsser {
    rate: f64,
    svf: Svf,
    /// Each channel's filter states.
    mem: [[f64; 2]; 2],
    /// The frequency in force, as `log2(Hz)`, gliding to the one last sent.
    lf: Ramp,
    band: MeanSquare,
    whole: MeanSquare,
    thr: Ramp,
    range: Ramp,
    path: Fade<Path>,
    sent: Settings,
    rho: f64,
    ess: f64,
    readouts: Readouts,
    /// Nothing run yet: a setting is taken at once.
    fresh: bool,
    /// A depth held, for the test that the bell is the bell `response` draws.
    #[cfg(test)]
    hold: Option<f64>,
}

impl DeEsser {
    /// A de-esser at `rate` Hz, at its defaults (out).
    pub fn new(rate: f64) -> DeEsser {
        let s = Settings::default();
        DeEsser {
            rate,
            svf: Svf::new(s.freq, rate),
            mem: [[0.0; 2]; 2],
            lf: Ramp::new(libm::log2(s.freq), samples(GLIDE_S, rate)),
            band: MeanSquare::new(rate, BAND_ATTACK, BAND_RELEASE),
            whole: MeanSquare::new(rate, WHOLE_ATTACK, WHOLE_RELEASE),
            thr: Ramp::new(s.thr, samples(RAMP_S, rate)),
            range: Ramp::new(s.range, samples(RAMP_S, rate)),
            path: Fade::new(s.path(), samples(FADE_S, rate)),
            sent: s,
            rho: 0.0,
            ess: 0.0,
            readouts: Readouts {
                in_peak_db: -300.0,
                out_peak_db: -300.0,
                gr_db: 0.0,
                acting: false,
                ess_db: 0.0,
            },
            fresh: true,
            #[cfg(test)]
            hold: None,
        }
    }

    /// New settings, held to their ranges, in force from the first sample of the next `process`.
    /// A unit that has run nothing yet takes them at once. Threshold and Range ramp in dB over
    /// 5 ms; In, Mode and Listen crossfade over 10 ms; the frequency glides on `log2(f)` over
    /// 10 ms. Allocates nothing; may run on the audio thread.
    pub fn set(&mut self, s: &Settings) {
        let s = s.held();
        let freq_moved = s.freq != self.sent.freq;
        self.sent = s;
        if self.fresh {
            self.thr.jump(s.thr);
            self.range.jump(s.range);
            self.path.jump(s.path());
            self.lf.jump(libm::log2(s.freq));
            self.svf = Svf::new(s.freq, self.rate);
            return;
        }
        if freq_moved {
            self.lf.retarget(libm::log2(s.freq));
        }
        if s.thr != self.thr.target() {
            self.thr.retarget(s.thr);
        }
        if s.range != self.range.target() {
            self.range.retarget(s.range);
        }
        self.path.set(s.path());
    }

    /// The settings last sent, held: in force once [`DeEsser::settled`].
    pub fn settings(&self) -> Settings {
        self.sent
    }

    /// Whether every ramp, crossfade and glide has ended and nothing waits.
    pub fn settled(&self) -> bool {
        self.thr.done() && self.range.done() && self.path.done() && self.lf.done()
    }

    /// What the last block read.
    pub fn readouts(&self) -> Readouts {
        self.readouts
    }

    /// One channel, in place.
    pub fn process(&mut self, x: &mut [f64]) {
        let (mut pin, mut pout, mut acting) = (0.0f64, 0.0f64, false);
        for v in x.iter_mut() {
            pin = pin.max(v.abs());
            let [y] = self.tick([*v]);
            acting |= self.rho > 0.1;
            pout = pout.max(y.abs());
            *v = y;
        }
        self.read(pin, pout, acting);
    }

    /// Two channels, in place, one detector for both.
    pub fn process2(&mut self, l: &mut [f64], r: &mut [f64]) {
        let (mut pin, mut pout, mut acting) = (0.0f64, 0.0f64, false);
        for (a, b) in l.iter_mut().zip(r.iter_mut()) {
            pin = pin.max(a.abs()).max(b.abs());
            let [y, z] = self.tick([*a, *b]);
            acting |= self.rho > 0.1;
            pout = pout.max(y.abs()).max(z.abs());
            (*a, *b) = (y, z);
        }
        self.read(pin, pout, acting);
    }

    fn read(&mut self, pin: f64, pout: f64, acting: bool) {
        self.readouts = Readouts {
            in_peak_db: crate::peak_db(pin),
            out_peak_db: crate::peak_db(pout),
            gr_db: self.rho,
            acting,
            ess_db: self.ess,
        };
    }

    #[inline]
    fn tick<const C: usize>(&mut self, x: [f64; C]) -> [f64; C] {
        self.fresh = false;
        if !self.lf.done() {
            let lf = self.lf.advance();
            // Landed, the frequency sent exactly, not `exp2` of its log.
            let hz = if self.lf.done() { self.sent.freq } else { libm::exp2(lf) };
            self.svf = Svf::new(hz, self.rate);
        }
        let mut b = [0.0; C];
        let (mut bsq, mut xsq) = (0.0f64, 0.0f64);
        for c in 0..C {
            b[c] = self.svf.run(&mut self.mem[c], x[c]);
            bsq = bsq.max(b[c] * b[c]);
            xsq = xsq.max(x[c] * x[c]);
        }
        self.band.feed(bsq);
        self.whole.feed(xsq);
        let whole = self.whole.db();
        self.ess = self.band.db() - whole;
        let knee = Knee { threshold: self.thr.advance(), ratio: RATIO, width: KNEE_DB };
        let r = knee.ff(self.ess).min(self.range.advance());
        let floor = ((whole - (FLOOR_DB - FLOOR_SOFT_DB)) / FLOOR_SOFT_DB).clamp(0.0, 1.0);
        self.rho = r * floor;
        let g = gain(self.rho);
        #[cfg(test)]
        let g = self.hold.unwrap_or(g);
        let (from, to, t) = self.path.advance();
        let mut y = x;
        for c in 0..C {
            y[c] = if t == 1.0 {
                to.out(x[c], b[c], g)
            } else {
                (1.0 - t) * from.out(x[c], b[c], g) + t * to.out(x[c], b[c], g)
            };
        }
        y
    }
}
