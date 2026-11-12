//! The pictures, as data: named series of points with their axes. The bench draws nothing (the
//! audio suite doesn't lean on the panel suite); a tool's front draws them with `lp-paint`, on
//! the panel and into PNG files beside the figures.

use crate::check::{Capture, Report};
use crate::figures::{Figures, Point};
use crate::signal::{Maker, Signal};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    Linear,
    Log,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Axis {
    pub label: String,
    pub unit: String,
    pub scale: Scale,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Series {
    /// "unit", "model", "at rest"…
    pub name: String,
    pub points: Vec<(f64, f64)>,
}

/// A note on a picture at a point: the curve's ratio, where the mark was found.
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub at: (f64, f64),
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    pub title: String,
    pub x: Axis,
    pub y: Axis,
    pub series: Vec<Series>,
    pub notes: Vec<Note>,
}

fn axis(label: &str, unit: &str, scale: Scale) -> Axis {
    Axis { label: label.into(), unit: unit.into(), scale }
}

fn series(name: &str, points: Vec<(f64, f64)>) -> Series {
    Series { name: name.into(), points }
}

/// The reduction over time of burst `n` (from the start of its loud part to the end of the quiet
/// one after), the unit's and, if given, the model's, each from [`crate::figures::reduction`].
pub fn reduction(s: &Signal, n: usize, unit: &[(f64, f64)], model: Option<&[(f64, f64)]>) -> Option<Picture> {
    // Part 0 settles; burst n is parts 2n+1 (loud) and 2n+2 (quiet).
    let (a, b) = (s.parts.get(2 * n + 1)?, s.parts.get(2 * n + 2)?);
    let r = s.rate as f64;
    let (t0, t1) = ((a.start - s.body) as f64 / r, (b.start + b.len - s.body) as f64 / r);
    let window = |e: &[(f64, f64)]| -> Vec<(f64, f64)> {
        e.iter().filter(|(t, _)| *t >= t0 && *t <= t1).map(|(t, g)| (t - t0, *g)).collect()
    };
    let mut out = vec![series("unit", window(unit))];
    if let Some(m) = model {
        out.push(series("model", window(m)));
    }
    Some(Picture {
        title: format!("{} burst {}", s.name, n + 1),
        x: axis("time", "s", Scale::Linear),
        y: axis("reduction", "dB", Scale::Linear),
        series: out,
        notes: vec![],
    })
}

/// The ratio at the top of a curve: input dB over output dB across its last `n` steps.
pub fn ratio(curve: &[Point], n: usize) -> Option<f64> {
    let top = &curve[curve.len().checked_sub(n.max(2))?..];
    let (a, b) = (top.first()?, top.last()?);
    let dout = b.out_db - a.out_db;
    (dout.abs() > 1e-9).then(|| (b.in_db - a.in_db) / dout)
}

/// The static curve, the unit's and the model's, with each one's ratio over its top 6 dB noted.
pub fn curve(unit: &Figures, model: Option<&Figures>) -> Picture {
    let mut out = Vec::new();
    let mut notes = Vec::new();
    for (name, f) in std::iter::once(("unit", unit)).chain(model.map(|m| ("model", m))) {
        out.push(series(name, f.curve.iter().map(|p| (p.in_db, p.out_db)).collect()));
        if let (Some(r), Some(p)) = (ratio(&f.curve, 7), f.curve.last()) {
            notes.push(Note { at: (p.in_db, p.out_db), text: format!("{name} {r:.1}:1") });
        }
    }
    Picture {
        title: "static curve".into(),
        x: axis("in", "dBFS", Scale::Linear),
        y: axis("out", "dBFS", Scale::Linear),
        series: out,
        notes,
    }
}

/// The response at rest and under reduction, the unit's and the model's.
pub fn response(unit: &Figures, model: Option<&Figures>) -> Picture {
    let mut out = Vec::new();
    for (name, f) in std::iter::once(("unit", unit)).chain(model.map(|m| ("model", m))) {
        for (how, r) in [("at rest", &f.response), ("reducing", &f.response_reducing)] {
            if !r.is_empty() {
                out.push(series(&format!("{name}, {how}"), r.iter().map(|[a, b]| (*a, *b)).collect()));
            }
        }
    }
    Picture {
        title: "response".into(),
        x: axis("frequency", "Hz", Scale::Log),
        y: axis("level", "dB", Scale::Linear),
        series: out,
        notes: vec![],
    }
}

/// The harmonics against level, each the unit's and the model's.
pub fn harmonics(unit: &Figures, model: Option<&Figures>) -> Picture {
    let mut out = Vec::new();
    for (name, f) in std::iter::once(("unit", unit)).chain(model.map(|m| ("model", m))) {
        for (k, get) in [
            (2, (|p: &Point| p.h2_db) as fn(&Point) -> Option<f64>),
            (3, |p: &Point| p.h3_db),
            (4, |p: &Point| p.h4_db),
            (5, |p: &Point| p.h5_db),
        ] {
            let pts: Vec<(f64, f64)> = f.curve.iter().filter_map(|p| get(p).map(|h| (p.in_db, h))).collect();
            if !pts.is_empty() {
                out.push(series(&format!("{name} h{k}"), pts));
            }
        }
    }
    Picture {
        title: "harmonics".into(),
        x: axis("in", "dBFS", Scale::Linear),
        y: axis("harmonic", "dBFS", Scale::Linear),
        series: out,
        notes: vec![],
    }
}

/// For a capture that failed: its start (to where the signal proper should begin), the mark as
/// played laid where it was found, and what the check said.
pub fn failed(s: &Signal, c: &Capture, report: &Report) -> Picture {
    let r = s.rate as f64;
    let lat = report.found.map_or(0, |f| f.latency);
    let end = (s.body + lat).min(c.samples.len());
    let took: Vec<(f64, f64)> =
        c.samples[..end].iter().enumerate().map(|(k, v)| (k as f64 / r, *v)).collect();
    let mark = Maker::new(s.rate, s.ceiling_db).mark();
    let laid: Vec<(f64, f64)> = mark.iter().enumerate().map(|(k, v)| ((k + lat) as f64 / r, *v)).collect();
    let notes = report
        .lines
        .iter()
        .filter(|l| !l.pass)
        .map(|l| Note { at: (lat as f64 / r, 0.0), text: format!("{}: {}", l.name, l.said) })
        .collect();
    Picture {
        title: format!("{}: REDO", s.name),
        x: axis("time", "s", Scale::Linear),
        y: axis("sample", "FS", Scale::Linear),
        series: vec![series("capture", took), series("mark, where found", laid)],
        notes,
    }
}
