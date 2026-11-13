//! A four-band channel EQ in the manner of a console's (step 3 of `specs/shared.md`, 2026-11-12;
//! the spec is `docs/spec.md`, the shape `RFC-eq.md`). For sonorack first, then BRR, ducky, place
//! and irma. Written here, not moved.
//!
//! **high-pass → low → low-mid → high-mid → high → trim**, the same coefficients on both channels.
//! The low and high bands are shelves that switch to bells; the mids are bells that overlap in the
//! middle of a voice. Every bell and shelf is `as-biquad`'s matched design, so the top stays open
//! up to Nyquist; a bell's width follows its gain (proportional Q); a cut is the boost's exact
//! inverse. The high-pass is Butterworth, 12 or 18 dB an octave.
//!
//! - [`Layout`]: the bands and their controls. [`Layout::CHANNEL`] is the four-band channel EQ.
//! - [`Settings`]: a plain `Copy` struct, read and set by control id ([`Settings::get`],
//!   [`Settings::set_by_id`]), every value held to its range.
//! - [`Eq`]: the unit. `set` builds the new coefficients, runs them over the last 100 ms of input
//!   so they start settled, and the unit crossfades to them over 10 ms, so nothing clicks or
//!   thumps; the trim is a 5 ms ramp.
//! - [`response`], [`band_response`], [`marker`]: the curve, from the same coefficients the unit
//!   runs, with no unit and no audio.
//!
//! Numbers are `f64`, a sample at a time, transcendentals through `libm`, arithmetic in a fixed
//! order: the same input and settings give the same bits at any block size, on any machine.

use as_biquad::{Biquad, First};
use as_control::{Control, Kind, Ramp, Scale, Unit};
use std::f64::consts::PI;

/// The most bands a layout has (the 551E, later, has five and a high-pass).
pub const MAX_BANDS: usize = 6;

/// A shelf's Q: a bump of 0.9 dB past the corner at ±15 dB (`docs/spec.md`, decision 4).
pub const SHELF_Q: f64 = 1.0;

/// The base Q of the low and high bands in bell mode.
pub const BELL_Q: f64 = 0.7;

/// A crossfade's length, and a gain ramp's.
const FADE_S: f64 = 0.010;
const RAMP_S: f64 = 0.005;

/// How much input a new filter set is run over before it's faded to: 100 ms (Warren,
/// 2026-11-12), enough for a 30 Hz shelf at +15 dB to settle within 0.2%.
const PRE_ROLL_S: f64 = 0.100;

/// A filter state below this is set to exactly 0, so no denormal slows the thread.
const FLUSH: f64 = 1e-200;

/// The Q a bell runs at, for the Q set and its gain: the Q set at ±15 dB, broader toward 0 dB,
/// `Q · (0.4 + 0.6·|g|/15)`. On |g| only, so a boost and a cut are exact inverses.
pub fn q_eff(q: f64, gain_db: f64) -> f64 {
    q * (0.4 + 0.6 * gain_db.abs() / 15.0)
}

/// What a band is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BandKind {
    /// A Butterworth high-pass, 12 or 18 dB an octave; its frequency the −3 dB point.
    HighPass,
    /// A low shelf (half its gain at its frequency), or a bell of [`BELL_Q`] by its switch.
    LowShelf,
    /// A bell, its Q set.
    Bell,
    /// A high shelf, or a bell of [`BELL_Q`] by its switch.
    HighShelf,
}

/// One band of a layout.
#[derive(Clone, Copy, Debug)]
pub struct BandDef {
    /// The front of its controls' ids: `lmf` for `lmf.freq`.
    pub id: &'static str,
    /// "Low-mid".
    pub long: &'static str,
    pub kind: BandKind,
}

/// A list of bands, in the order they're run, and their controls.
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub name: &'static str,
    pub bands: &'static [BandDef],
    pub controls: &'static [Control],
}

impl PartialEq for Layout {
    fn eq(&self, other: &Layout) -> bool {
        self.name == other.name
    }
}

#[allow(clippy::too_many_arguments)] // the spec table's columns, in order
const fn value(
    id: &'static str,
    short: &'static str,
    long: &'static str,
    (lo, hi): (f64, f64),
    scale: Scale,
    (fine, coarse): (f64, f64),
    unit: Unit,
    default: f64,
) -> Control {
    Control {
        id,
        short,
        long,
        kind: Kind::Value { lo, hi, scale, fine, coarse, unit },
        default,
        detent: default,
    }
}

const fn switch(
    id: &'static str,
    short: &'static str,
    long: &'static str,
    positions: &'static [&'static str],
    default: f64,
) -> Control {
    Control { id, short, long, kind: Kind::Switch { positions, ratios: false }, default, detent: default }
}

const IN: &[&str] = &["out", "in"];
const BELL: &[&str] = &["shelf", "bell"];
const SLOPE: &[&str] = &["12", "18"];
const FREQ_STEP: (f64, f64) = (1.0 / 12.0, 1.0 / 3.0);
const DB_STEP: (f64, f64) = (0.5, 3.0);
const Q_STEP: (f64, f64) = (1.0 / 6.0, 0.5);
const GAIN: (f64, f64) = (-15.0, 15.0);

const CHANNEL_CONTROLS: &[Control] = &[
    switch("in", "IN", "In", IN, 1.0),
    switch("hpf.in", "IN", "High-pass in", IN, 0.0),
    value("hpf.freq", "FREQ", "High-pass frequency", (20.0, 400.0), Scale::Log, FREQ_STEP, Unit::Hz, 80.0),
    switch("hpf.slope", "SLOPE", "High-pass slope", SLOPE, 1.0),
    switch("lf.in", "IN", "Low in", IN, 1.0),
    value("lf.freq", "FREQ", "Low frequency", (30.0, 450.0), Scale::Log, FREQ_STEP, Unit::Hz, 100.0),
    value("lf.gain", "GAIN", "Low gain", GAIN, Scale::Linear, DB_STEP, Unit::Db, 0.0),
    switch("lf.bell", "BELL", "Low bell", BELL, 0.0),
    switch("lmf.in", "IN", "Low-mid in", IN, 1.0),
    value("lmf.freq", "FREQ", "Low-mid frequency", (200.0, 2500.0), Scale::Log, FREQ_STEP, Unit::Hz, 400.0),
    value("lmf.gain", "GAIN", "Low-mid gain", GAIN, Scale::Linear, DB_STEP, Unit::Db, 0.0),
    value("lmf.q", "Q", "Low-mid Q", (0.4, 4.0), Scale::Log, Q_STEP, Unit::Q, 1.0),
    switch("hmf.in", "IN", "High-mid in", IN, 1.0),
    value("hmf.freq", "FREQ", "High-mid frequency", (600.0, 7000.0), Scale::Log, FREQ_STEP, Unit::Hz, 3000.0),
    value("hmf.gain", "GAIN", "High-mid gain", GAIN, Scale::Linear, DB_STEP, Unit::Db, 0.0),
    value("hmf.q", "Q", "High-mid Q", (0.4, 4.0), Scale::Log, Q_STEP, Unit::Q, 1.0),
    switch("hf.in", "IN", "High in", IN, 1.0),
    value("hf.freq", "FREQ", "High frequency", (1500.0, 16000.0), Scale::Log, FREQ_STEP, Unit::Hz, 10000.0),
    value("hf.gain", "GAIN", "High gain", GAIN, Scale::Linear, DB_STEP, Unit::Db, 0.0),
    switch("hf.bell", "BELL", "High bell", BELL, 0.0),
    value("trim", "TRIM", "Trim", (-12.0, 12.0), Scale::Linear, DB_STEP, Unit::Db, 0.0),
];

impl Layout {
    /// The four-band channel EQ: high-pass, LF, LMF, HMF, HF, trim (`docs/spec.md`, "The bands").
    pub const CHANNEL: Layout = Layout {
        name: "channel",
        bands: &[
            BandDef { id: "hpf", long: "High-pass", kind: BandKind::HighPass },
            BandDef { id: "lf", long: "Low", kind: BandKind::LowShelf },
            BandDef { id: "lmf", long: "Low-mid", kind: BandKind::Bell },
            BandDef { id: "hmf", long: "High-mid", kind: BandKind::Bell },
            BandDef { id: "hf", long: "High", kind: BandKind::HighShelf },
        ],
        controls: CHANNEL_CONTROLS,
    };
}

/// The channel EQ's controls, [`Layout::CHANNEL`]'s.
pub fn controls() -> &'static [Control] {
    Layout::CHANNEL.controls
}

/// One band's settings. A field its layout gives no control is ignored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub on: bool,
    pub freq: f64,
    pub gain: f64,
    pub q: f64,
    /// A shelf's switch to a bell.
    pub bell: bool,
    /// A high-pass at 18 dB an octave, not 12.
    pub steep: bool,
}

const BAND: Band = Band { on: false, freq: 1000.0, gain: 0.0, q: 1.0, bell: false, steep: false };

/// The EQ's settings: a layout, its bands', the trim and In.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub layout: &'static Layout,
    /// The whole EQ in; out, the unit is its input to the bit.
    pub on: bool,
    /// In the layout's order; past its last band, unused.
    pub bands: [Band; MAX_BANDS],
    pub trim: f64,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings::new(&Layout::CHANNEL)
    }
}

fn flag(b: bool) -> f64 {
    if b { 1.0 } else { 0.0 }
}

impl Settings {
    /// A layout at its controls' defaults.
    pub fn new(layout: &'static Layout) -> Settings {
        let mut s = Settings { layout, on: true, bands: [BAND; MAX_BANDS], trim: 0.0 };
        for c in layout.controls {
            s.put(c.id, c.default);
        }
        s
    }

    /// The value of a control, by its id; `None` if the layout has no such control.
    pub fn get(&self, id: &str) -> Option<f64> {
        as_control::find(self.layout.controls, id)?;
        self.raw(id)
    }

    /// Sets a control by its id, held to its range (a switch's to a position). False if the
    /// layout has no such control.
    pub fn set_by_id(&mut self, id: &str, v: f64) -> bool {
        match as_control::find(self.layout.controls, id) {
            Some(c) => self.put(id, c.clamp(v)),
            None => false,
        }
    }

    /// Every control held to its range.
    pub fn held(&self) -> Settings {
        let mut s = *self;
        for c in self.layout.controls {
            if let Some(v) = s.raw(c.id) {
                s.put(c.id, c.clamp(v));
            }
        }
        s
    }

    fn band(&self, id: &str) -> Option<usize> {
        self.layout.bands.iter().position(|d| d.id == id)
    }

    fn raw(&self, id: &str) -> Option<f64> {
        match id {
            "in" => Some(flag(self.on)),
            "trim" => Some(self.trim),
            _ => {
                let (band, field) = id.split_once('.')?;
                let b = &self.bands[self.band(band)?];
                Some(match field {
                    "in" => flag(b.on),
                    "freq" => b.freq,
                    "gain" => b.gain,
                    "q" => b.q,
                    "bell" => flag(b.bell),
                    "slope" => flag(b.steep),
                    _ => return None,
                })
            }
        }
    }

    fn put(&mut self, id: &str, v: f64) -> bool {
        match id {
            "in" => self.on = v >= 0.5,
            "trim" => self.trim = v,
            _ => {
                let Some((band, field)) = id.split_once('.') else { return false };
                let Some(i) = self.band(band) else { return false };
                let b = &mut self.bands[i];
                match field {
                    "in" => b.on = v >= 0.5,
                    "freq" => b.freq = v,
                    "gain" => b.gain = v,
                    "q" => b.q = v,
                    "bell" => b.bell = v >= 0.5,
                    "slope" => b.steep = v >= 0.5,
                    _ => return false,
                }
            }
        }
        true
    }

    /// The same EQ, the trim aside: what a crossfade is between.
    fn same_filters(&self, other: &Settings) -> bool {
        Settings { trim: 0.0, ..*self } == Settings { trim: 0.0, ..*other }
    }
}

/// What the unit read over the last block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Readouts {
    /// The block's largest input sample, both channels, in dB (a sine of peak 1.0 reads 0).
    pub in_peak_db: f64,
    /// The block's largest output sample.
    pub out_peak_db: f64,
    /// Always 0: an EQ takes nothing away by level.
    pub gr_db: f64,
}

/// A level in dB, never below −300.
fn db(v: f64) -> f64 {
    20.0 * libm::log10(v.max(1e-15))
}

/// One band's sections, as run: none when the band is out or flat.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Stage {
    n: usize,
    sec: [Biquad; 2],
}

const EMPTY: Stage = Stage { n: 0, sec: [Biquad::IDENTITY; 2] };

fn first(f: First) -> Biquad {
    Biquad { b0: f.b0, b1: f.b1, b2: 0.0, a1: f.a1, a2: 0.0 }
}

fn stage(kind: BandKind, b: &Band, rate: f64) -> Stage {
    if !b.on {
        return EMPTY;
    }
    let one = |s: Biquad| Stage { n: 1, sec: [s, Biquad::IDENTITY] };
    let (f, g) = (b.freq, b.gain);
    match kind {
        // Butterworth's sections, as `as_biquad::butterworth` gives them, without its Vec.
        HighPass if b.steep => {
            let q = 1.0 / (2.0 * libm::sin(PI / 6.0));
            Stage { n: 2, sec: [first(First::highpass(f, rate)), Biquad::highpass(f, q, rate)] }
        }
        HighPass => one(Biquad::highpass(f, 1.0 / (2.0 * libm::sin(PI / 4.0)), rate)),
        _ if g == 0.0 => EMPTY,
        Bell => one(Biquad::peak_matched(f, q_eff(b.q, g), g, rate)),
        LowShelf | HighShelf if b.bell => one(Biquad::peak_matched(f, q_eff(BELL_Q, g), g, rate)),
        LowShelf => one(Biquad::low_shelf_matched(f, SHELF_Q, g, rate)),
        HighShelf => one(Biquad::high_shelf_matched(f, SHELF_Q, g, rate)),
    }
}
use BandKind::*;

/// Every band's sections.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Path {
    bands: usize,
    stage: [Stage; MAX_BANDS],
}

fn design(s: &Settings, rate: f64) -> Path {
    let mut p = Path { bands: s.layout.bands.len(), stage: [EMPTY; MAX_BANDS] };
    if s.on {
        for (i, d) in s.layout.bands.iter().enumerate() {
            p.stage[i] = stage(d.kind, &s.bands[i], rate);
        }
    }
    p
}

/// One channel's memory of a path: each band's sections' last two inputs and outputs.
#[derive(Clone, Copy, Debug)]
struct Mem {
    sec: [[[f64; 4]; 2]; MAX_BANDS],
}

const REST: Mem = Mem { sec: [[[0.0; 4]; 2]; MAX_BANDS] };

/// One sample through a path, as `Biquad::process` runs a section, a state that falls below
/// [`FLUSH`] set to 0.
#[inline]
fn run(p: &Path, m: &mut Mem, mut x: f64) -> f64 {
    for b in 0..p.bands {
        let st = &p.stage[b];
        for j in 0..st.n {
            let c = &st.sec[j];
            let s = &mut m.sec[b][j];
            let [x1, x2, y1, y2] = *s;
            let mut y = c.b0 * x + c.b1 * x1 + c.b2 * x2 - c.a1 * y1 - c.a2 * y2;
            if y.abs() < FLUSH {
                y = 0.0;
            }
            *s = [x, x1, y, y1];
            x = y;
        }
    }
    x
}

/// The EQ, one or two channels.
pub struct Eq {
    rate: f64,
    /// Two filter sets: `live` is heard; during a crossfade the other is faded to.
    paths: [Path; 2],
    /// Each set's memory, for each channel.
    mem: [[Mem; 2]; 2],
    live: usize,
    /// The settings each set was built from.
    built: [Settings; 2],
    /// A crossfade's samples done, while one runs.
    fading: Option<usize>,
    fade_len: usize,
    /// The last [`PRE_ROLL_S`] of input, each channel, oldest at `pos`.
    hist: [Vec<f64>; 2],
    pos: usize,
    /// Channels in use: 2 once `process2` has run.
    chans: usize,
    /// A setting that came during a crossfade, the latest.
    waiting: Option<Settings>,
    /// The latest settings sent, held.
    sent: Settings,
    trim: Ramp,
    readouts: Readouts,
    /// Nothing run yet: a setting is taken at once, with nothing to fade from.
    fresh: bool,
    /// The faded-to set starts from rest, without its pre-roll: for the test that shows why it's
    /// run.
    #[cfg(test)]
    from_rest: bool,
}

impl Eq {
    /// An EQ at `rate` Hz, at the channel layout's defaults (flat).
    pub fn new(rate: f64) -> Eq {
        let s = Settings::default();
        let p = design(&s, rate);
        Eq {
            rate,
            paths: [p; 2],
            mem: [[REST; 2]; 2],
            live: 0,
            built: [s; 2],
            fading: None,
            fade_len: libm::round(FADE_S * rate) as usize,
            hist: [0, 1].map(|_| vec![0.0; (libm::round(PRE_ROLL_S * rate) as usize).max(1)]),
            pos: 0,
            chans: 1,
            waiting: None,
            sent: s,
            trim: Ramp::new(1.0, libm::round(RAMP_S * rate) as usize),
            readouts: Readouts { in_peak_db: -300.0, out_peak_db: -300.0, gr_db: 0.0 },
            fresh: true,
            #[cfg(test)]
            from_rest: false,
        }
    }

    /// New settings, held to their ranges, in force from the first sample of the next `process`.
    /// A unit that has run nothing yet takes them at once.
    /// Any change but the trim builds the new coefficients, runs them over the last 100 ms of input,
    /// and crossfades to them over 10 ms; one
    /// that comes while a crossfade runs waits for it, the latest replacing any waiting. The trim
    /// ramps over 5 ms. Allocates nothing; may run on the audio thread.
    pub fn set(&mut self, s: &Settings) {
        let s = s.held();
        let trim = if s.on { libm::pow(10.0, s.trim / 20.0) } else { 1.0 };
        if trim != self.trim.target() {
            self.trim.retarget(trim);
        }
        self.sent = s;
        if self.fresh {
            self.trim.jump(trim);
            self.paths[self.live] = design(&s, self.rate);
            self.built[self.live] = s;
            return;
        }
        match self.fading {
            // Running: the setting waits, unless it's where the crossfade is going.
            Some(k) if k > 0 => {
                let to = &self.built[1 - self.live];
                self.waiting = if s.same_filters(to) { None } else { Some(s) };
            }
            // Started but not yet run: replaced, as if the first had never come.
            Some(_) => {
                self.fading = None;
                self.start(s);
            }
            None => self.start(s),
        }
    }

    /// Starts the crossfade to `s`, if its filters differ from the live set's.
    fn start(&mut self, s: Settings) {
        let live = self.live;
        if s.same_filters(&self.built[live]) {
            self.built[live] = s;
            return;
        }
        let p = design(&s, self.rate);
        if p == self.paths[live] {
            self.built[live] = s;
            return;
        }
        // The new set runs from rest over the last 100 ms of input, so it starts where it would
        // have been had it run all along: a 30 Hz shelf stepped joins its steady state, where a
        // copy of the old set's state, or rest, thumps (`docs/spec.md`, as built).
        let other = 1 - live;
        self.paths[other] = p;
        for c in 0..self.chans {
            let m = &mut self.mem[other][c];
            *m = REST;
            #[cfg(test)]
            if self.from_rest {
                continue;
            }
            let h = &self.hist[c];
            for &x in h[self.pos..].iter().chain(&h[..self.pos]) {
                run(&self.paths[other], m, x);
            }
        }
        self.built[other] = s;
        self.fading = Some(0);
    }

    /// The settings in force: the live set's, and the trim last sent.
    pub fn settings(&self) -> Settings {
        Settings { trim: self.sent.trim, ..self.built[self.live] }
    }

    /// Whether every crossfade and ramp has ended and nothing waits.
    pub fn settled(&self) -> bool {
        self.fading.is_none() && self.waiting.is_none() && self.trim.done()
    }

    /// What the last block read.
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

    /// Two channels, in place, the same filters on each, each its own state.
    pub fn process2(&mut self, l: &mut [f64], r: &mut [f64]) {
        self.chans = 2;
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
        self.readouts = Readouts { in_peak_db: db(pin), out_peak_db: db(pout), gr_db: 0.0 };
    }

    #[inline]
    fn tick<const C: usize>(&mut self, x: [f64; C]) -> [f64; C] {
        self.fresh = false;
        for c in 0..C {
            self.hist[c][self.pos] = x[c];
        }
        self.pos = if self.pos + 1 == self.hist[0].len() { 0 } else { self.pos + 1 };
        let live = self.live;
        let mut y = x;
        for c in 0..C {
            y[c] = run(&self.paths[live], &mut self.mem[live][c], x[c]);
        }
        if let Some(k) = self.fading {
            let (other, k) = (1 - live, k + 1);
            let mut z = x;
            for c in 0..C {
                z[c] = run(&self.paths[other], &mut self.mem[other][c], x[c]);
            }
            if k >= self.fade_len {
                y = z;
                self.end_fade();
            } else {
                self.fading = Some(k);
                let t = k as f64 / self.fade_len as f64;
                for c in 0..C {
                    y[c] = (1.0 - t) * y[c] + t * z[c];
                }
            }
        }
        let g = self.trim.advance();
        for v in &mut y {
            *v *= g;
        }
        y
    }

    fn end_fade(&mut self) {
        self.live = 1 - self.live;
        self.fading = None;
        if let Some(s) = self.waiting.take() {
            self.start(s);
        }
    }
}

/// The whole EQ's magnitude at `hz`, in dB, the trim with it: from the sections the unit runs,
/// so the curve drawn is what's heard. No unit, no state, no audio.
pub fn response(s: &Settings, rate: f64, hz: f64) -> f64 {
    let s = s.held();
    let bands: f64 = (0..s.layout.bands.len()).map(|b| band_response(&s, b, rate, hz)).sum();
    if s.on { bands + s.trim } else { bands }
}

/// One band's magnitude at `hz`, in dB: 0 where the band, or the whole EQ, is out.
pub fn band_response(s: &Settings, band: usize, rate: f64, hz: f64) -> f64 {
    let s = s.held();
    let Some(d) = s.layout.bands.get(band) else { return 0.0 };
    if !s.on {
        return 0.0;
    }
    let st = stage(d.kind, &s.bands[band], rate);
    st.sec[..st.n].iter().map(|c| db(c.gain(hz, rate))).sum()
}

/// Where to draw a band's marker: a bell's centre at its gain, a shelf's corner at half its gain,
/// the high-pass's corner at −3 dB. Where the band is set, in or out.
pub fn marker(s: &Settings, band: usize) -> (f64, f64) {
    let s = s.held();
    let Some(d) = s.layout.bands.get(band) else { return (0.0, 0.0) };
    let b = &s.bands[band];
    match d.kind {
        HighPass => (b.freq, -3.0),
        LowShelf | HighShelf if !b.bell => (b.freq, b.gain / 2.0),
        _ => (b.freq, b.gain),
    }
}

#[cfg(test)]
mod tests;
