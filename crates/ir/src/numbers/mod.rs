//! An IR described in numbers, enough to choose a room by: after ISO 3382-1, worked out at the
//! IR's own rate (moved into the suite from dexter-place's `core/src/numbers.rs`, 2026-11-09,
//! `VENDORED.md`). A sound is its channels, `&[Vec<f32>]`, where place had its `Sound`.

use std::f64::consts::PI;

use crate::unit_energy;
use realfft::RealFftPlanner;
use realfft::num_complex::Complex;

#[cfg(test)]
mod tests;

/// The octave bands' centres, Hz.
pub const BANDS: [f64; 7] = [125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0];
/// The 500 Hz and 1 kHz bands, whose mean is the single number.
const MID: [usize; 2] = [2, 3];

/// A reverberation time and how it was measured.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rt {
    /// Seconds to fall 60 dB.
    pub secs: f64,
    /// 30 (T30, from −5 to −35 dB) or 20 (T20, from −5 to −25 dB).
    pub over: u8,
}

/// One octave band's numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub hz: f64,
    pub rt: Option<Rt>,
    /// Early decay time, seconds: 0 to −10 dB, ×6.
    pub edt: Option<f64>,
    /// Its energy, dB, over the 1 kHz band's.
    pub tone: f64,
    /// Early over late energy, dB: to 50 ms, to 80 ms.
    pub c50: f64,
    pub c80: f64,
    /// Its peak over its noise floor, dB.
    pub range: f64,
}

/// The numbers.
#[derive(Clone, Debug, PartialEq)]
pub struct Numbers {
    pub rate: u32,
    pub channels: usize,
    pub seconds: f64,
    /// The 500 Hz and 1 kHz bands' mean: the single number.
    pub rt: Option<Rt>,
    pub edt: Option<f64>,
    pub bands: [Band; 7],
    /// Warmth: (RT₁₂₅ + RT₂₅₀) / (RT₅₀₀ + RT₁ₖ).
    pub br: Option<f64>,
    /// Brilliance: (RT₂ₖ + RT₄ₖ) / (RT₅₀₀ + RT₁ₖ).
    pub brilliance: Option<f64>,
    pub c50: f64,
    pub c80: f64,
    /// Centre time, ms from the onset.
    pub ts: f64,
    /// Where the tail starts, ms from the onset: normalised echo density reaching 1.
    pub mix: Option<f64>,
    /// The sides' correlation in the tail: 1 alike (or mono), 0 unrelated.
    pub corr: f64,
    /// Leading silence: the onset, ms.
    pub pre: f64,
    /// The whole IR's peak over its noise floor, dB.
    pub range: f64,
    /// The file's peak, dBFS.
    pub peak: f64,
    /// The gain that plays it at unit energy, dB.
    pub gain: f64,
}

fn db(x: f64) -> f64 {
    if x > 0.0 { 10.0 * x.log10() } else { -300.0 }
}

/// The IR's numbers. `sound` at `rate`, one channel or two.
pub fn describe(sound: &[Vec<f32>], rate: u32) -> Numbers {
    let n = sound.first().map_or(0, Vec::len);
    let fs = rate as f64;
    let energy = |chans: &[Vec<f64>]| -> Vec<f64> {
        (0..n).map(|k| chans.iter().map(|c| c[k] * c[k]).sum()).collect()
    };
    let chans: Vec<Vec<f64>> = sound.iter().map(|c| c.iter().map(|v| *v as f64).collect()).collect();
    let e = energy(&chans);
    let onset = onset(&e);
    let whole = Decay::of(&e, onset, rate);

    // The bands, split with no phase.
    let len = (2 * n).max(2);
    let mut planner = RealFftPlanner::<f64>::new();
    let fwd = planner.plan_fft_forward(len);
    let inv = planner.plan_fft_inverse(len);
    let spectra: Vec<Vec<Complex<f64>>> = chans
        .iter()
        .map(|c| {
            let mut t = vec![0.0; len];
            t[..n].copy_from_slice(c);
            let mut f = fwd.make_output_vec();
            fwd.process(&mut t, &mut f).expect("fft sizes");
            f
        })
        .collect();
    let decays: Vec<(Decay, f64)> = BANDS
        .iter()
        .map(|&hz| {
            let banded: Vec<Vec<f64>> = spectra
                .iter()
                .map(|s| {
                    let mut f: Vec<Complex<f64>> = s
                        .iter()
                        .enumerate()
                        .map(|(k, v)| v * weight(k as f64 * fs / len as f64, hz) / len as f64)
                        .collect();
                    f[0].im = 0.0;
                    if let Some(last) = f.last_mut() {
                        last.im = 0.0;
                    }
                    let mut t = vec![0.0; len];
                    inv.process(&mut f, &mut t).expect("fft sizes");
                    t.truncate(n);
                    t
                })
                .collect();
            let be = energy(&banded);
            let total = be.iter().sum::<f64>();
            (Decay::of(&be, onset, rate), total)
        })
        .collect();
    let ref_energy = decays[3].1;
    let bands: [Band; 7] = std::array::from_fn(|b| {
        let (d, total) = &decays[b];
        Band {
            hz: BANDS[b],
            rt: d.rt(),
            edt: d.edt(),
            tone: db(*total) - db(ref_energy),
            c50: d.clarity(0.050),
            c80: d.clarity(0.080),
            range: d.range,
        }
    });
    let mid = |f: &dyn Fn(&Band) -> Option<f64>| -> Option<f64> {
        Some((f(&bands[MID[0]])? + f(&bands[MID[1]])?) / 2.0)
    };
    let rt = mid(&|b| b.rt.map(|r| r.secs))
        .map(|secs| Rt { secs, over: bands[MID[0]].rt.unwrap().over.min(bands[MID[1]].rt.unwrap().over) });
    let ratio = |a: usize, b: usize| {
        let mid = mid(&|b| b.rt.map(|r| r.secs))?;
        Some((bands[a].rt?.secs + bands[b].rt?.secs) / (2.0 * mid))
    };
    let mix = mixing(&chans, onset, rate);
    let tail_from = onset + mix.map_or((0.080 * fs) as usize, |m| (m / 1000.0 * fs) as usize);
    let peak = sound.iter().flatten().fold(0.0f32, |m, v| m.max(v.abs())) as f64;
    Numbers {
        rate,
        channels: sound.len(),
        seconds: n as f64 / fs,
        rt,
        edt: mid(&|b| b.edt),
        bands,
        br: ratio(0, 1),
        brilliance: ratio(4, 5),
        c50: (bands[MID[0]].c50 + bands[MID[1]].c50) / 2.0,
        c80: (bands[MID[0]].c80 + bands[MID[1]].c80) / 2.0,
        ts: whole.centre() * 1000.0,
        mix,
        corr: correlation(&chans, tail_from, whole.cut),
        pre: onset as f64 / fs * 1000.0,
        range: whole.range,
        peak: 2.0 * db(peak),
        gain: 2.0 * db(unit_energy(sound) as f64),
    }
}

/// A band's amplitude weight at `f` Hz: cos(π/2 · h(x)), x octaves from its centre, to one
/// octave each side, h(x) = x − sin(2πx)/2π a step that leaves both ends flat. Squared,
/// neighbours sum to 1 (h(1 − x) = 1 − h(x)) and cross at −3 dB at the band's edges; flat at
/// its ends, the filter rings briefly, so a short room's decay isn't lengthened by it.
pub fn weight(f: f64, centre: f64) -> f64 {
    if f <= 0.0 {
        return 0.0;
    }
    let x = (f / centre).log2().abs();
    if x >= 1.0 { 0.0 } else { (PI / 2.0 * (x - (2.0 * PI * x).sin() / (2.0 * PI))).cos() }
}

/// Where an IR's sound starts (its first frame within 20 dB of its peak), where its decay meets
/// its noise (after Lundeby), and its peak over that noise, dB: what an IR made from a recording
/// is cut to.
pub fn span(sound: &[Vec<f32>], rate: u32) -> (usize, usize, f64) {
    let n = sound.first().map_or(0, Vec::len);
    let e: Vec<f64> = (0..n).map(|k| sound.iter().map(|c| (c[k] as f64).powi(2)).sum()).collect();
    let start = onset(&e);
    let d = Decay::of(&e, start, rate);
    (start, d.cut, d.range)
}

/// The first frame within 20 dB of the peak.
pub fn onset(e: &[f64]) -> usize {
    let peak = e.iter().cloned().fold(0.0, f64::max);
    e.iter().position(|v| *v >= peak / 100.0).unwrap_or(0)
}

/// A decay: Schroeder's backward integral, cut where it meets the noise floor (after Lundeby)
/// and made up beyond the cut by the fitted line.
struct Decay {
    rate: f64,
    /// Where the decay is cut: the fitted line meets the floor.
    cut: usize,
    /// The energy from each frame on, from the onset to the cut (and what's made up beyond).
    from: Vec<f64>,
    /// The peak over the floor, smoothed, dB.
    range: f64,
    /// The time-weighted energy from the onset on, for centre time.
    moment: f64,
}

/// A least-squares line through `(x, y)`: `(intercept, slope)`.
fn line(pts: &[(f64, f64)]) -> Option<(f64, f64)> {
    let n = pts.len() as f64;
    if pts.len() < 2 {
        return None;
    }
    let (sx, sy) = pts.iter().fold((0.0, 0.0), |(a, b), (x, y)| (a + x, b + y));
    let (mx, my) = (sx / n, sy / n);
    let (sxx, sxy) =
        pts.iter().fold((0.0, 0.0), |(a, b), (x, y)| (a + (x - mx) * (x - mx), b + (x - mx) * (y - my)));
    if sxx == 0.0 {
        return None;
    }
    let b = sxy / sxx;
    Some((my - b * mx, b))
}

/// Mean energy over `e[a..b]`, as dB.
fn mean_db(e: &[f64], a: usize, b: usize) -> f64 {
    let b = b.min(e.len());
    if b <= a {
        return -300.0;
    }
    db(e[a..b].iter().sum::<f64>() / (b - a) as f64)
}

impl Decay {
    fn of(e: &[f64], onset: usize, rate: u32) -> Decay {
        let fs = rate as f64;
        let n = e.len();
        let last_tenth = n - (n / 10).max(1);
        let mut noise = mean_db(e, last_tenth, n);
        let mut win = ((0.010 * fs) as usize).max(1);
        // The smoothed energy: (centre frame, dB) for each window from the onset.
        let smooth = |win: usize| -> Vec<(f64, f64)> {
            (onset..n)
                .step_by(win)
                .map(|a| (a as f64 + win.min(n - a) as f64 / 2.0, mean_db(e, a, a + win)))
                .collect()
        };
        let peak = smooth(win).iter().map(|p| p.1).fold(-300.0, f64::max);
        let mut cut = n;
        let mut fit: Option<(f64, f64)> = None;
        for _ in 0..5 {
            let s = smooth(win);
            // Down to 10 dB above the floor.
            let end = s.iter().position(|p| p.1 < noise + 10.0).unwrap_or(s.len());
            let Some((a, b)) = line(&s[..end.max(2).min(s.len())]) else { break };
            if b >= 0.0 {
                break;
            }
            fit = Some((a, b));
            let cross = (((noise - a) / b).max(onset as f64 + 1.0)).min(n as f64) as usize;
            // Five windows for each 10 dB of decay.
            win = ((10.0 / -b / 5.0) as usize).clamp(1, (0.050 * fs) as usize);
            // The floor again, from 5 dB of decay past the cut, or the last tenth.
            let from = (cross + (5.0 / -b) as usize).min(last_tenth);
            noise = mean_db(e, from, n);
            let moved = cross.abs_diff(cut);
            cut = cross;
            if moved < win {
                break;
            }
        }
        // What's beyond the cut, made up by the line: its energy from the cut on.
        let (beyond, beyond_moment) = match fit {
            Some((a, b)) if cut < n => {
                let r = 10f64.powf(b / 10.0);
                let at = 10f64.powf((a + b * cut as f64) / 10.0);
                let k = (cut - onset) as f64;
                (at / (1.0 - r), at * (k / (1.0 - r) + r / ((1.0 - r) * (1.0 - r))))
            }
            _ => (0.0, 0.0),
        };
        let cut = cut.max(onset + 1).min(n);
        let mut from = vec![0.0; cut - onset + 1];
        from[cut - onset] = beyond;
        let mut moment = beyond_moment;
        for k in (onset..cut).rev() {
            from[k - onset] = from[k - onset + 1] + e[k];
            moment += (k - onset) as f64 * e[k];
        }
        Decay { rate: fs, cut, from, range: peak - noise, moment }
    }

    /// The decay curve at frame `onset + k`, dB.
    fn level(&self, k: usize) -> f64 {
        db(self.from[k] / self.from[0])
    }

    /// The slope between `hi` and `lo` dB, as seconds to fall 60, if the curve gets to `lo`.
    fn slope(&self, hi: f64, lo: f64) -> Option<f64> {
        let a = (0..self.from.len()).position(|k| self.level(k) <= hi)?;
        let b = (a..self.from.len()).position(|k| self.level(k) <= lo)? + a;
        let pts: Vec<(f64, f64)> = (a..=b).map(|k| (k as f64, self.level(k))).collect();
        let (_, s) = line(&pts)?;
        (s < 0.0).then(|| -60.0 / s / self.rate)
    }

    fn rt(&self) -> Option<Rt> {
        if self.range >= 45.0
            && let Some(secs) = self.slope(-5.0, -35.0)
        {
            return Some(Rt { secs, over: 30 });
        }
        if self.range >= 35.0 {
            return self.slope(-5.0, -25.0).map(|secs| Rt { secs, over: 20 });
        }
        None
    }

    fn edt(&self) -> Option<f64> {
        if self.range < 20.0 {
            return None;
        }
        self.slope(0.0, -10.0)
    }

    /// Early over late, the early part `t` seconds from the onset, dB.
    fn clarity(&self, t: f64) -> f64 {
        let k = ((t * self.rate) as usize).min(self.from.len() - 1);
        db(self.from[0] - self.from[k]) - db(self.from[k])
    }

    /// Centre time, seconds.
    fn centre(&self) -> f64 {
        if self.from[0] > 0.0 { self.moment / self.from[0] / self.rate } else { 0.0 }
    }
}

/// Where the tail starts, ms from the onset: the first time the channels' mean normalised echo
/// density (Abel and Huang, in a 20 ms Hann window stepped a millisecond at a time), smoothed
/// over 10 ms, comes within 5% of 1. In a diffuse tail it wanders about 1 by a tenth either
/// way; the smoothing keeps a chance swing among the early reflections from counting.
fn mixing(chans: &[Vec<f64>], onset: usize, rate: u32) -> Option<f64> {
    let p = eta_profile(chans, onset, rate);
    let w = ((0.020 * rate as f64) as usize).max(3);
    let hop = ((0.001 * rate as f64) as usize).max(1);
    const SMOOTH: usize = 10;
    (0..p.len().saturating_sub(SMOOTH - 1))
        .position(|i| p[i..i + SMOOTH].iter().sum::<f64>() / SMOOTH as f64 >= 0.95)
        .map(|i| ((i + SMOOTH / 2) * hop + w / 2) as f64 / rate as f64 * 1000.0)
}

/// The echo density profile, a millisecond at a time from the onset.
fn eta_profile(chans: &[Vec<f64>], onset: usize, rate: u32) -> Vec<f64> {
    let mut out = vec![];
    let fs = rate as f64;
    let w = ((0.020 * fs) as usize).max(3);
    let hop = ((0.001 * fs) as usize).max(1);
    let hann: Vec<f64> = (0..w).map(|k| 0.5 - 0.5 * (2.0 * PI * (k as f64 + 0.5) / w as f64).cos()).collect();
    let sum_w: f64 = hann.iter().sum();
    let gaussian = 0.317_310_507_862_914_2; // erfc(1/√2)
    let n = chans[0].len();
    let mut a = onset;
    while a + w <= n {
        let mut eta = 0.0;
        for c in chans {
            let x = &c[a..a + w];
            let var = x.iter().zip(&hann).map(|(v, h)| h * v * v).sum::<f64>() / sum_w;
            let sd = var.sqrt();
            let over: f64 = x.iter().zip(&hann).filter(|(v, _)| v.abs() > sd).map(|(_, h)| h).sum();
            eta += over / sum_w / gaussian;
        }
        eta /= chans.len() as f64;
        out.push(eta);
        a += hop;
    }
    out
}

/// The two sides' correlation from frame `a` to `b`: 1 for a mono IR.
fn correlation(chans: &[Vec<f64>], a: usize, b: usize) -> f64 {
    if chans.len() < 2 || a >= b {
        return 1.0;
    }
    let (l, r) = (&chans[0][a..b], &chans[1][a..b]);
    let lr: f64 = l.iter().zip(r).map(|(x, y)| x * y).sum();
    let ll: f64 = l.iter().map(|x| x * x).sum();
    let rr: f64 = r.iter().map(|x| x * x).sum();
    if ll > 0.0 && rr > 0.0 { lr / (ll * rr).sqrt() } else { 1.0 }
}
