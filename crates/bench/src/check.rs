//! A capture, the loopback's calibration, and the check run on every capture at once.

use std::fmt;

use serde::{Deserialize, Serialize};

use as_envelope::db;

use crate::align::{self, Found};
use crate::response;
use crate::signal::{Maker, Signal, What};

/// What came back of one signal: one channel of the interface's inputs, from the cycle it began
/// to play. The JACK path and [`crate::run_model`] both make these, so they go through the same
/// check and reduction.
#[derive(Clone, Debug, PartialEq)]
pub struct Capture {
    pub rate: u32,
    pub samples: Vec<f64>,
    /// xruns while it played.
    pub xruns: u64,
}

/// The loopback's figures, which every later capture is checked and corrected by. Kept as a file,
/// named by date.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Calibration {
    pub name: String,
    pub rate: u32,
    /// The round trip, samples.
    pub latency: usize,
    /// The mark came back the way it went.
    pub positive: bool,
    /// dBFS out to dBFS in, at 1 kHz, on the step at the ceiling.
    pub gain_db: f64,
    /// The noise floor, sine-referenced dBFS (a sine of this peak has the noise's power).
    pub floor_db: f64,
    /// The interface's own THD on the step at the ceiling, dB under the fundamental.
    pub thd_db: f64,
    /// The interface's response on the third-octave points, `[Hz, dB]`.
    pub response: Vec<[f64; 2]>,
}

impl Calibration {
    pub fn to_toml(&self) -> String {
        toml::to_string(self).expect("a calibration is plain data")
    }

    pub fn from_toml(text: &str) -> Result<Calibration, String> {
        toml::from_str(text).map_err(|e| e.to_string().trim_end().to_string())
    }

    /// Kept as a new file in `dir`, named by `stem` (the date, say: `loopback-2026-11-12`), as
    /// `stem-NNN.toml`, never over one.
    pub fn write(&self, dir: impl AsRef<std::path::Path>, stem: &str) -> std::io::Result<std::path::PathBuf> {
        crate::keep_toml(&self.to_toml(), dir, stem)
    }
}

/// A capture aligned: `y[k]` is what came back of the signal's sample `k`.
pub fn aligned<'a>(c: &'a Capture, f: &Found) -> &'a [f64] {
    &c.samples[f.latency.min(c.samples.len())..]
}

/// The level of `x`, sine-referenced dBFS: `10·log10(2·mean square)`.
pub fn power_db(x: &[f64]) -> f64 {
    if x.is_empty() {
        return -300.0;
    }
    let ms = x.iter().map(|v| v * v).sum::<f64>() / x.len() as f64;
    10.0 * libm::log10((2.0 * ms).max(1e-30))
}

/// The calibration from the loopback's `silence`, `sweep` and `steps` captures, each with the
/// signal played. `Err` names a capture whose mark wasn't found.
pub fn calibrate(
    name: &str,
    (silence, s_cap): (&Signal, &Capture),
    (sweep, w_cap): (&Signal, &Capture),
    (steps, t_cap): (&Signal, &Capture),
) -> Result<Calibration, String> {
    let rate = sweep.rate;
    let mark = Maker::new(rate, sweep.ceiling_db).mark();
    let find =
        |s: &Signal, c: &Capture| align::find(&c.samples, &mark, rate).ok_or(format!("{}: no mark", s.name));
    let fw = find(sweep, w_cap)?;
    let y = aligned(w_cap, &fw);
    let p = sweep.parts[0];
    let What::Sweep { secs } = p.what else { return Err("sweep: not a sweep".into()) };
    let x = &sweep.samples[p.start..p.start + p.len];
    let d = response::deconvolve(x, &y[p.start.min(y.len())..], secs, rate);
    let resp = response::thirds(&d.linear, d.n, rate).into_iter().map(|(f, g)| [f, g]).collect();
    let fs = find(silence, s_cap)?;
    let ys = aligned(s_cap, &fs);
    let q = silence.parts[0];
    let floor_db = power_db(&ys[q.start.min(ys.len())..(q.start + q.len).min(ys.len())]);
    let ft = find(steps, t_cap)?;
    let yt = aligned(t_cap, &ft);
    let top = crate::levels::steps(steps, yt, -300.0);
    let last = top.last().ok_or("steps: no steps")?;
    let len = (0.1 * rate as f64).round() as usize;
    let tp = steps.parts.last().expect("steps");
    let thd = as_envelope::measure::thd(yt, crate::signal::TONE_HZ, rate, tp.start + tp.len - len, len);
    Ok(Calibration {
        name: name.to_string(),
        rate,
        latency: fw.latency,
        positive: fw.positive,
        gain_db: last.out_db - last.in_db,
        floor_db,
        thd_db: db(thd.max(1e-15)),
        response: resp,
    })
}

/// One line of the check.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub name: &'static str,
    pub pass: bool,
    /// What was found, and on a fail, what to look at.
    pub said: String,
}

/// The check on one capture: a line each, then PASS or REDO.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub lines: Vec<Line>,
    /// Where the mark was found, if it was.
    pub found: Option<Found>,
}

impl Report {
    pub fn pass(&self) -> bool {
        self.lines.iter().all(|l| l.pass)
    }

    /// The names of the lines that failed.
    pub fn failed(&self) -> Vec<&'static str> {
        self.lines.iter().filter(|l| !l.pass).map(|l| l.name).collect()
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for l in &self.lines {
            writeln!(f, "{:<18} {}  {}", l.name, if l.pass { "ok  " } else { "FAIL" }, l.said)?;
        }
        write!(f, "{}", if self.pass() { "PASS" } else { "REDO" })
    }
}

/// Over this, the capture clipped.
pub const CLIP_DB: f64 = -0.1;
/// The return's peak belongs between these.
pub const LEVEL_DB: (f64, f64) = (-40.0, -1.0);
/// The mark's correlation peak against the next.
pub const MARK_RATIO: f64 = 10.0;
/// Samples the latency may differ from the calibration's.
pub const LATENCY_SLACK: usize = 2;
/// The signal over the noise, dB.
pub const OVER_NOISE_DB: f64 = 40.0;

/// Checks `c`, the capture of `s`, against the calibration (`None` while calibrating: the latency
/// and the polarity aren't compared, and the floor is the capture's own).
pub fn check(s: &Signal, c: &Capture, cal: Option<&Calibration>) -> Report {
    let rate = s.rate;
    let mut lines = Vec::new();
    let mut line = |name, pass, said: String| lines.push(Line { name, pass, said });
    let peak = db(as_envelope::measure::peak(&c.samples).max(1e-15));
    let clipped = peak >= CLIP_DB;
    line(
        "clipped",
        !clipped,
        if clipped {
            format!("peak {peak:.1} dBFS: lower the ceiling or the unit's output")
        } else {
            format!("peak {peak:.1} dBFS")
        },
    );
    // A clipped capture is said once, by the line above.
    let level_ok = clipped || (LEVEL_DB.0..=LEVEL_DB.1).contains(&peak);
    let level_said = if level_ok {
        format!("peak {peak:.1} dBFS")
    } else if peak < LEVEL_DB.0 {
        format!("peak {peak:.1} dBFS, too quiet: is the unit's output up, and patched to the bench's in?")
    } else {
        format!("peak {peak:.1} dBFS, too hot: lower the ceiling or the unit's output")
    };
    line("level", level_ok, level_said);
    let mark = Maker::new(rate, s.ceiling_db).mark();
    let found = align::find(&c.samples, &mark, rate);
    match found {
        None => line("aligned", false, "no mark: is the unit's in patched?".into()),
        Some(f) => {
            let near = cal.is_none_or(|k| f.latency.abs_diff(k.latency) <= LATENCY_SLACK);
            let ok = f.ratio >= MARK_RATIO && near;
            let mut said = format!("latency {} samples, mark {:.0}× the next", f.latency, f.ratio);
            if let Some(k) = cal {
                said += &format!(" (calibration {})", k.latency);
            }
            if !ok {
                said += ": not aligned: is the unit's in patched, and is it the calibration's interface?";
            }
            line("aligned", ok, said);
        }
    }
    match (found, cal) {
        (Some(f), Some(k)) => {
            let ok = f.positive == k.positive;
            let said = if ok {
                "as the calibration's".into()
            } else {
                "inverted: check the unit's phase switch and the cables".into()
            };
            line("polarity", ok, said);
        }
        (Some(f), None) => {
            line("polarity", true, if f.positive { "as played".into() } else { "inverted".into() })
        }
        (None, _) => line("polarity", false, "no mark to tell by".into()),
    }
    let lat = found.map_or(0, |f| f.latency);
    let y = &c.samples[lat.min(c.samples.len())..];
    // The capture's own floor: the last 100 ms of the silence before the signal proper.
    let gap = (0.1 * rate as f64) as usize;
    let own = power_db(&y[s.body.saturating_sub(gap).min(y.len())..s.body.min(y.len())]);
    let floor = cal.map_or(own, |k| k.floor_db.max(own));
    let sounding: Vec<f64> = s
        .parts
        .iter()
        .filter(|p| !matches!(p.what, What::Silence))
        .flat_map(|p| y[p.start.min(y.len())..(p.start + p.len).min(y.len())].iter().copied())
        .collect();
    if sounding.is_empty() {
        line("signal over noise", true, format!("floor {floor:.1} dBFS"));
    } else {
        let over = power_db(&sounding) - floor;
        let ok = over >= OVER_NOISE_DB;
        let mut said = format!("{over:.1} dB over the floor ({floor:.1} dBFS)");
        if !ok {
            said += ": noisy: a hum or a loose cable, or the unit's output too low";
        }
        line("signal over noise", ok, said);
    }
    line(
        "xruns",
        c.xruns == 0,
        if c.xruns == 0 {
            "none".into()
        } else {
            format!("{}: close what else is running, and redo", c.xruns)
        },
    );
    let want = s.samples.len() + lat;
    let ok = c.samples.len() >= want;
    line(
        "length",
        ok,
        if ok {
            "all of it".into()
        } else {
            format!("{} of {want} samples: it stopped early", c.samples.len())
        },
    );
    Report { lines, found }
}
