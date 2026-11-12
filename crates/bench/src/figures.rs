//! Captures reduced to figures, with the calibration divided out, and the figures as a small
//! TOML file: the one thing of a sitting that may be committed (Warren, 2026-11-12). The unit's
//! and a model's go through the same functions here.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::check::{Calibration, Capture, aligned};
use crate::levels;
use crate::response;
use crate::signal::{Maker, Signal, What};

/// What a sitting says about itself, written with its figures.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Notes {
    /// The interface, and its settings.
    pub interface: String,
    /// The calibration used, by name.
    pub calibration: String,
    pub ceiling_db: f64,
    /// Each capture's knob positions, as typed: `"steps" = "ratio 4, input 3"`.
    pub knobs: BTreeMap<String, String>,
}

/// One step of the static curve, dBFS, with the harmonics there (absent under the floor).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub in_db: f64,
    pub out_db: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h2_db: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h3_db: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h4_db: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h5_db: Option<f64>,
}

/// Attack (63%) and release (37% left) times, seconds: each repeat, their mean and spread.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Times {
    pub attack: Vec<f64>,
    pub release: Vec<f64>,
    pub attack_mean: f64,
    pub attack_spread: f64,
    pub release_mean: f64,
    pub release_spread: f64,
}

/// The release after a short hold and after a long one, and their ratio.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Memory {
    pub short_hold: f64,
    pub long_hold: f64,
    pub release_short: f64,
    pub release_long: f64,
    pub ratio: f64,
}

/// A unit's figures, or a model's.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Figures {
    /// "Warren's LA-4, serial if known", or the model's name.
    pub unit: String,
    pub date: String,
    pub notes: Notes,
    /// `[Hz, dB]` on the third-octave points, at rest.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub response: Vec<[f64; 2]>,
    /// The same, under the reduction `steady_sweep` holds it at.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub response_reducing: Vec<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub curve: Vec<Point>,
    /// `[m, Hz, dB]` from the sweep ([`sweep_harmonics`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sweep_harmonics: Vec<[f64; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub times: Option<Times>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<Memory>,
}

/// One figure of two sets that differs: what, where, the unit's, the model's, and model − unit.
#[derive(Clone, Debug, PartialEq)]
pub struct Difference {
    pub what: String,
    pub at: f64,
    pub unit: f64,
    pub model: f64,
    pub diff: f64,
}

impl Figures {
    pub fn to_toml(&self) -> String {
        toml::to_string(self).expect("figures are plain data")
    }

    pub fn from_toml(text: &str) -> Result<Figures, String> {
        toml::from_str(text).map_err(|e| e.to_string().trim_end().to_string())
    }

    /// Written as a new file in `dir`, `stem-NNN.toml`, never over one.
    pub fn write(&self, dir: impl AsRef<Path>, stem: &str) -> io::Result<PathBuf> {
        crate::keep_toml(&self.to_toml(), dir, stem)
    }

    /// Every figure the two have both of, with the model's less the unit's: the response by
    /// frequency, the curve by input level, the times and the memory by name.
    pub fn against(&self, model: &Figures) -> Vec<Difference> {
        let mut out = Vec::new();
        let mut d = |what: &str, at: f64, u: f64, m: f64| {
            out.push(Difference { what: what.into(), at, unit: u, model: m, diff: m - u })
        };
        for (what, a, b) in [
            ("response", &self.response, &model.response),
            ("response_reducing", &self.response_reducing, &model.response_reducing),
        ] {
            for [f, u] in a {
                if let Some([_, m]) = b.iter().find(|[g, _]| g == f) {
                    d(what, *f, *u, *m);
                }
            }
        }
        for [n, f, u] in &self.sweep_harmonics {
            if let Some([_, _, m]) = model.sweep_harmonics.iter().find(|[k, g, _]| k == n && g == f) {
                d(&format!("sweep h{n}"), *f, *u, *m);
            }
        }
        for p in &self.curve {
            if let Some(q) = model.curve.iter().find(|q| (q.in_db - p.in_db).abs() < 1e-9) {
                d("curve", p.in_db, p.out_db, q.out_db);
                for (n, a, b) in [
                    (2, p.h2_db, q.h2_db),
                    (3, p.h3_db, q.h3_db),
                    (4, p.h4_db, q.h4_db),
                    (5, p.h5_db, q.h5_db),
                ] {
                    if let (Some(a), Some(b)) = (a, b) {
                        d(&format!("h{n}"), p.in_db, a, b);
                    }
                }
            }
        }
        if let (Some(a), Some(b)) = (&self.times, &model.times) {
            d("attack", 0.0, a.attack_mean, b.attack_mean);
            d("release", 0.0, a.release_mean, b.release_mean);
        }
        if let (Some(a), Some(b)) = (&self.memory, &model.memory) {
            d("memory", 0.0, a.ratio, b.ratio);
        }
        out
    }
}

/// The capture aligned by its own mark, or `None` if it has none.
fn align(s: &Signal, c: &Capture) -> Option<Vec<f64>> {
    let mark = Maker::new(s.rate, s.ceiling_db).mark();
    let f = crate::align::find(&c.samples, &mark, s.rate)?;
    Some(aligned(c, &f).to_vec())
}

/// A `sweep` or `steady_sweep` capture deconvolved, with the steady tone's frequency if any.
fn deconvolved(s: &Signal, c: &Capture) -> Option<(response::Deconvolved, Option<f64>, f64)> {
    let y = align(s, c)?;
    let p = s.parts.iter().find(|p| matches!(p.what, What::Sweep { .. } | What::SteadySweep { .. }))?;
    let (x, secs, tone) = match p.what {
        What::Sweep { secs } => (s.samples[p.start..p.start + p.len].to_vec(), secs, None),
        // The steady tone is played under the sweep: the reference is the sweep alone, at its level.
        What::SteadySweep { secs, hz, .. } => {
            let alone = Maker::new(s.rate, 0.0).sweep(secs, p.db);
            let q = alone.parts[0];
            (alone.samples[q.start..q.start + q.len].to_vec(), secs, Some(hz))
        }
        _ => return None,
    };
    Some((response::deconvolve(&x, &y[p.start.min(y.len())..], secs, s.rate), tone, secs))
}

/// The response from a `sweep` or `steady_sweep` capture, with the calibration's divided out.
/// Under `steady_sweep`, the point whose sixth of an octave holds the steady tone is left out:
/// the tone isn't the sweep's, and reads there as a bump.
pub fn response(s: &Signal, c: &Capture, cal: Option<&Calibration>) -> Option<Vec<[f64; 2]>> {
    let (d, tone, _) = deconvolved(s, c)?;
    let sixth = libm::pow(2.0, 1.0 / 12.0);
    let clear = |f: f64| tone.is_none_or(|t| t < f / sixth || t > f * sixth);
    Some(
        response::thirds(&d.linear, d.n, s.rate)
            .into_iter()
            .filter(|(f, _)| clear(*f))
            .filter_map(|(f, g)| match cal {
                None => Some([f, g]),
                Some(k) => k.response.iter().find(|[h, _]| *h == f).map(|[_, r]| [f, g - r]),
            })
            .collect(),
    )
}

/// The harmonics from a `sweep` capture: `[m, Hz, dB]`, harmonic `m` (2 to 5) at its own
/// frequency on the third-octave points, against the sweep's level (so a harmonic `a_m` of a
/// tone of `A` reads `a_m / A`), from an input of `Hz / m`. Points whose input's sixth of an
/// octave reaches into the sweep's fade-in are left out: they read low. The interface's own aren't taken off: compare with its calibration's THD.
pub fn sweep_harmonics(s: &Signal, c: &Capture) -> Option<Vec<[f64; 3]>> {
    let (d, _, secs) = deconvolved(s, c)?;
    let low = response::fade_in_hz(secs, s.rate) * libm::pow(2.0, 1.0 / 12.0);
    let mut out = Vec::new();
    for (i, h) in d.harmonics.iter().enumerate() {
        let m = (i + 2) as f64;
        for (f, g) in response::thirds(h, d.n, s.rate) {
            if f / m >= low {
                out.push([m, f, g]);
            }
        }
    }
    Some(out)
}

/// The static curve and harmonics from a `steps` capture, the interface's gain taken off, and
/// harmonics believed only 10 dB over the calibration's floor.
pub fn curve(s: &Signal, c: &Capture, cal: Option<&Calibration>) -> Option<Vec<Point>> {
    let y = align(s, c)?;
    let (gain, floor) = cal.map_or((0.0, -300.0), |k| (k.gain_db, k.floor_db + 10.0));
    Some(
        levels::steps(s, &y, floor)
            .into_iter()
            .map(|st| {
                let h = st.harmonics.map(|h| h.map(|v| v - gain));
                Point {
                    in_db: st.in_db,
                    out_db: st.out_db - gain,
                    h2_db: h[0],
                    h3_db: h[1],
                    h4_db: h[2],
                    h5_db: h[3],
                }
            })
            .collect(),
    )
}

/// The unit's gain at rest, from the curve's five quietest steps.
pub fn rest_gain(curve: &[Point]) -> f64 {
    let steps: Vec<levels::Step> = curve
        .iter()
        .map(|p| levels::Step { in_db: p.in_db, out_db: p.out_db, harmonics: [None; 4] })
        .collect();
    levels::rest_gain(&steps, 5)
}

/// The reduction over time, `(seconds, dB)`, from a `bursts` or `hold` capture.
pub fn reduction(
    s: &Signal,
    c: &Capture,
    cal: Option<&Calibration>,
    rest_db: f64,
) -> Option<Vec<(f64, f64)>> {
    let y = align(s, c)?;
    let gain = cal.map_or(0.0, |k| k.gain_db);
    Some(levels::reduction(s, &y, rest_db + gain))
}

/// The times from a `bursts` capture.
pub fn times(s: &Signal, c: &Capture, cal: Option<&Calibration>, rest_db: f64) -> Option<Times> {
    let env = reduction(s, c, cal, rest_db)?;
    let (attack, release) = levels::burst_times(&env, s);
    let (am, asp) = levels::mean_spread(&attack);
    let (rm, rsp) = levels::mean_spread(&release);
    Some(Times {
        attack,
        release,
        attack_mean: am,
        attack_spread: asp,
        release_mean: rm,
        release_spread: rsp,
    })
}

/// The memory from two `hold` captures, the short first.
pub fn memory(
    (short, sc): (&Signal, &Capture),
    (long, lc): (&Signal, &Capture),
    cal: Option<&Calibration>,
    rest_db: f64,
) -> Option<Memory> {
    let rel = |s: &Signal, c: &Capture| levels::hold_release(&reduction(s, c, cal, rest_db)?, s);
    let (a, b) = (rel(short, sc)?, rel(long, lc)?);
    let held = |s: &Signal| s.parts[0].len as f64 / s.rate as f64;
    Some(Memory {
        short_hold: held(short),
        long_hold: held(long),
        release_short: a,
        release_long: b,
        ratio: b / a,
    })
}
