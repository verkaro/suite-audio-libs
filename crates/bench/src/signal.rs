//! The signals a capture plays: each a sync mark, the signal, and half a second of silence,
//! with where each part lies and the level it was played at.

use std::f64::consts::{PI, TAU};

use as_envelope::{corpus, level};

/// The ceiling when Warren hasn't typed one: out of a +4 interface, 0 dBFS can be +18 to +24 dBu.
pub const CEILING_DB: f64 = -6.0;
/// The sync mark: a linear chirp from 2 kHz to 8 kHz over 20 ms, at −20 dBFS (or the ceiling,
/// if that's lower), its ends tapered over 2 ms, then 200 ms of silence.
pub const MARK_SECS: f64 = 0.020;
pub const MARK_DB: f64 = -20.0;
pub const MARK_GAP_SECS: f64 = 0.200;
/// The silence after every signal.
pub const TAIL_SECS: f64 = 0.5;
/// The tone the level signals are made of.
pub const TONE_HZ: f64 = 1000.0;

/// What a stretch of a signal is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum What {
    /// Nothing.
    Silence,
    /// Farina's sweep, 20 Hz to 20 kHz (or 0.45 of the rate), over `secs`.
    Sweep { secs: f64 },
    /// A tone at `hz`.
    Tone { hz: f64 },
    /// A sweep over a steady tone: `tone_db` is the tone's level, the part's level the sweep's.
    SteadySweep { secs: f64, hz: f64, tone_db: f64 },
}

/// A stretch of a signal: from `start`, `len` samples, played at `db` dBFS (a tone's or a
/// sweep's peak; −300 for silence).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Part {
    pub start: usize,
    pub len: usize,
    pub db: f64,
    pub what: What,
}

/// A signal ready to play.
#[derive(Clone, Debug, PartialEq)]
pub struct Signal {
    pub name: String,
    pub rate: u32,
    pub ceiling_db: f64,
    pub samples: Vec<f64>,
    /// The mark is at 0; the signal proper starts here.
    pub body: usize,
    /// The signal proper, part by part, in order (the tail's silence not counted).
    pub parts: Vec<Part>,
}

/// Makes signals at a rate under a ceiling. Every level given to it is **dB under the ceiling**
/// (0 is the ceiling), so nothing it makes can pass the ceiling.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Maker {
    pub rate: u32,
    pub ceiling_db: f64,
}

impl Maker {
    pub fn new(rate: u32, ceiling_db: f64) -> Maker {
        Maker { rate, ceiling_db: ceiling_db.min(0.0) }
    }

    fn n(&self, secs: f64) -> usize {
        libm::round(secs * self.rate as f64) as usize
    }

    /// A level under the ceiling as dBFS, held to the ceiling.
    fn dbfs(&self, under: f64) -> f64 {
        self.ceiling_db + under.min(0.0)
    }

    /// The mark alone, as it's played.
    pub fn mark(&self) -> Vec<f64> {
        mark(self.rate, MARK_DB.min(self.ceiling_db))
    }

    fn signal(&self, name: &str, build: impl FnOnce(&Maker, &mut Vec<f64>, &mut Vec<Part>)) -> Signal {
        let mut samples = self.mark();
        samples.resize(samples.len() + self.n(MARK_GAP_SECS), 0.0);
        let body = samples.len();
        let mut parts = Vec::new();
        build(self, &mut samples, &mut parts);
        samples.resize(samples.len() + self.n(TAIL_SECS), 0.0);
        Signal { name: name.to_string(), rate: self.rate, ceiling_db: self.ceiling_db, samples, body, parts }
    }

    /// `secs` of nothing: the noise floor.
    pub fn silence(&self, secs: f64) -> Signal {
        self.signal("silence", |m, x, p| {
            let len = m.n(secs);
            p.push(Part { start: x.len(), len, db: -300.0, what: What::Silence });
            x.resize(x.len() + len, 0.0);
        })
    }

    /// Farina's exponential sweep, 20 Hz to 20 kHz, over `secs`, at `under` dB under the
    /// ceiling (`as_envelope::corpus::sweep`, its peak set exactly).
    pub fn sweep(&self, secs: f64, under: f64) -> Signal {
        self.signal("sweep", |m, x, p| {
            let db = m.dbfs(under);
            let s = to_peak(corpus::sweep(1.0, secs, m.rate), db);
            p.push(Part { start: x.len(), len: s.len(), db, what: What::Sweep { secs } });
            x.extend(s);
        })
    }

    /// The 1 kHz tone at each of `levels` (dB under the ceiling), each held `hold` s, its phase
    /// running on from one to the next.
    pub fn steps(&self, levels: &[f64], hold: f64) -> Signal {
        let segs: Vec<(f64, f64)> = levels.iter().map(|&l| (l, hold)).collect();
        self.signal("steps", |m, x, p| m.tones(x, p, &segs))
    }

    /// The usual steps: −60 to 0 under the ceiling, in 1 dB steps.
    pub fn steps_db() -> Vec<f64> {
        (-60..=0).map(f64::from).collect()
    }

    /// 1 kHz at `low` for `low_secs`, then `repeats` times `high` for `high_secs` and `low`
    /// for `low_secs` (levels under the ceiling).
    pub fn bursts(&self, low: f64, low_secs: f64, high: f64, high_secs: f64, repeats: usize) -> Signal {
        let mut segs = vec![(low, low_secs)];
        for _ in 0..repeats {
            segs.push((high, high_secs));
            segs.push((low, low_secs));
        }
        self.signal("bursts", |m, x, p| m.tones(x, p, &segs))
    }

    /// 1 kHz at `high` for `secs`, then at `after` for `after_secs`: a quiet tone the release can
    /// be seen on, or `None` for silence (levels under the ceiling).
    pub fn hold(&self, high: f64, secs: f64, after: Option<f64>, after_secs: f64) -> Signal {
        self.signal("hold", |m, x, p| match after {
            Some(a) => m.tones(x, p, &[(high, secs), (a, after_secs)]),
            None => {
                m.tones(x, p, &[(high, secs)]);
                let len = m.n(after_secs);
                p.push(Part { start: x.len(), len, db: -300.0, what: What::Silence });
                x.resize(x.len() + len, 0.0);
            }
        })
    }

    /// A tone of `hz` at `tone` held `settle` s, then with Farina's sweep at `sweep` added for
    /// `secs` (levels under the ceiling; the two together held to it).
    pub fn steady_sweep(&self, hz: f64, tone: f64, settle: f64, sweep: f64, secs: f64) -> Signal {
        self.signal("steady_sweep", |m, x, p| {
            let (mut ta, mut sa) = (level(m.dbfs(tone)), level(m.dbfs(sweep)));
            let top = level(m.ceiling_db);
            if ta + sa > top {
                let k = top / (ta + sa);
                ta *= k;
                sa *= k;
            }
            let (ns, nw) = (m.n(settle), corpus::sweep(1.0, secs, m.rate).len());
            let start = x.len();
            let w = TAU * hz / m.rate as f64;
            let sw = to_peak(corpus::sweep(1.0, secs, m.rate), 0.0);
            for k in 0..ns + nw {
                let s = if k >= ns { sa * sw[k - ns] } else { 0.0 };
                x.push(ta * libm::sin(w * k as f64) + s);
            }
            let (tdb, sdb) = (as_envelope::db(ta), as_envelope::db(sa));
            p.push(Part { start, len: ns, db: tdb, what: What::Tone { hz } });
            p.push(Part {
                start: start + ns,
                len: nw,
                db: sdb,
                what: What::SteadySweep { secs, hz, tone_db: tdb },
            });
        })
    }

    /// The 1 kHz tone through `segs` of (level under the ceiling, seconds), its phase running on.
    fn tones(&self, x: &mut Vec<f64>, p: &mut Vec<Part>, segs: &[(f64, f64)]) {
        let w = TAU * TONE_HZ / self.rate as f64;
        let mut k = 0usize;
        for &(under, secs) in segs {
            let db = self.dbfs(under);
            let (a, len) = (level(db), self.n(secs));
            p.push(Part { start: x.len(), len, db, what: What::Tone { hz: TONE_HZ } });
            for _ in 0..len {
                x.push(a * libm::sin(w * k as f64));
                k += 1;
            }
        }
    }
}

/// `x` scaled so its peak is `db` dBFS exactly.
fn to_peak(x: Vec<f64>, db: f64) -> Vec<f64> {
    let k = level(db) / as_envelope::measure::peak(&x);
    x.into_iter().map(|v| v * k).collect()
}

/// The sync mark at `db` dBFS: a linear chirp from 2 kHz to 8 kHz over 20 ms, its ends tapered
/// over 2 ms by a raised cosine, so its correlation has low sidelobes.
pub fn mark(rate: u32, db: f64) -> Vec<f64> {
    let (f0, f1) = (2000.0, 8000.0);
    let n = libm::round(MARK_SECS * rate as f64) as usize;
    let taper = libm::round(0.002 * rate as f64) as usize;
    let a = level(db);
    let rate = rate as f64;
    (0..n)
        .map(|k| {
            let t = k as f64 / rate;
            let phase = TAU * (f0 * t + 0.5 * (f1 - f0) / MARK_SECS * t * t);
            let edge = k.min(n - 1 - k);
            let g = if edge < taper { 0.5 - 0.5 * libm::cos(PI * edge as f64 / taper as f64) } else { 1.0 };
            a * g * libm::sin(phase)
        })
        .collect()
}
