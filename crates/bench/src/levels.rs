//! What comes from the 1 kHz signals: the static curve and the harmonics from `steps`, and from
//! `bursts` and `hold` the reduction over time, its times, and the optical's memory.

use as_envelope::measure::tone;

use crate::signal::{Part, Signal, TONE_HZ, What};

/// One step of the static curve: the level played and the level that came back (the last
/// 100 ms of the step, by one DFT bin over whole cycles), and the harmonics there, each in dBFS,
/// `None` where it was under the floor given.
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub in_db: f64,
    pub out_db: f64,
    /// The second to the fifth.
    pub harmonics: [Option<f64>; 4],
}

/// The last 100 ms of each tone part of `s`, read in `y` (aligned with `s`): the static curve,
/// and the harmonics over `floor_db`.
pub fn steps(s: &Signal, y: &[f64], floor_db: f64) -> Vec<Step> {
    let len = (0.1 * s.rate as f64).round() as usize;
    s.parts
        .iter()
        .filter(|p| matches!(p.what, What::Tone { .. }) && p.len >= len)
        .map(|p| {
            let from = p.start + p.len - len;
            let read = |hz: f64| as_envelope::db(tone(y, hz, s.rate, from, len).0.max(1e-15));
            let harmonics = [2.0, 3.0, 4.0, 5.0].map(|m| {
                let hz = m * TONE_HZ;
                let h = read(hz);
                (hz < 0.5 * s.rate as f64 && h > floor_db).then_some(h)
            });
            Step { in_db: p.db, out_db: read(TONE_HZ), harmonics }
        })
        .collect()
}

/// The unit's gain where it does nothing: the mean of out − in over the `n` quietest steps.
pub fn rest_gain(curve: &[Step], n: usize) -> f64 {
    let mut by: Vec<&Step> = curve.iter().collect();
    by.sort_by(|a, b| a.in_db.total_cmp(&b.in_db));
    let take = &by[..n.min(by.len()).max(1)];
    take.iter().map(|s| s.out_db - s.in_db).sum::<f64>() / take.len() as f64
}

/// The reduction over time, dB (negative reduces): every half cycle of the tone, the largest
/// sample of `y` in it, against the level played there and the gain at rest. `(seconds from the
/// signal proper's start, reduction)`, over the tone parts of `s`.
pub fn reduction(s: &Signal, y: &[f64], rest_db: f64) -> Vec<(f64, f64)> {
    let r = s.rate as f64;
    let half = r / TONE_HZ / 2.0;
    let mut out = Vec::new();
    for p in s.parts.iter().filter(|p| matches!(p.what, What::Tone { .. })) {
        let mut k = 0usize;
        loop {
            let a = p.start + (k as f64 * half).round() as usize;
            let b = p.start + ((k + 1) as f64 * half).round() as usize;
            if b > p.start + p.len || b > y.len() {
                break;
            }
            let peak = y[a..b].iter().fold(0.0f64, |m, v| m.max(v.abs()));
            let t = (b - s.body) as f64 / r;
            out.push((t, as_envelope::db(peak.max(1e-15)) - p.db - rest_db));
            k += 1;
        }
    }
    out
}

/// How long the reduction took to move 63% of the way from where it was at `p`'s start to where
/// it settled at its end (the mean of its last 10 ms), seconds. An attack when `p` is louder
/// than before it, a release (the 37% left) when quieter. `None` if it didn't move.
pub fn time(env: &[(f64, f64)], s: &Signal, p: &Part) -> Option<f64> {
    let r = s.rate as f64;
    let (t0, t1) = ((p.start - s.body) as f64 / r, (p.start + p.len - s.body) as f64 / r);
    let inside: Vec<(f64, f64)> = env.iter().copied().filter(|(t, _)| *t > t0 && *t <= t1).collect();
    let before = env.iter().rev().find(|(t, _)| *t <= t0)?.1;
    let tail: Vec<f64> = inside.iter().filter(|(t, _)| *t > t1 - 0.010).map(|e| e.1).collect();
    if tail.is_empty() {
        return None;
    }
    let end = tail.iter().sum::<f64>() / tail.len() as f64;
    if (end - before).abs() < 0.1 {
        return None;
    }
    let mark = before + 0.632 * (end - before);
    let up = end > before;
    let mut prev = (t0, before);
    for &(t, g) in &inside {
        if (up && g >= mark) || (!up && g <= mark) {
            // Between the two half cycles, on a line.
            let f = if g == prev.1 { 0.0 } else { (mark - prev.1) / (g - prev.1) };
            return Some(prev.0 + f * (t - prev.0) - t0);
        }
        prev = (t, g);
    }
    None
}

/// The attack and release times of `bursts`: each high part's attack and each low part's release
/// after it (the first low part, the settling, left out).
pub fn burst_times(env: &[(f64, f64)], s: &Signal) -> (Vec<f64>, Vec<f64>) {
    let (mut attack, mut release) = (Vec::new(), Vec::new());
    for (i, p) in s.parts.iter().enumerate().skip(1) {
        let louder = p.db > s.parts[i - 1].db;
        if let Some(t) = time(env, s, p) {
            if louder { attack.push(t) } else { release.push(t) }
        }
    }
    (attack, release)
}

/// The mean and the spread (largest less smallest) of `x`.
pub fn mean_spread(x: &[f64]) -> (f64, f64) {
    if x.is_empty() {
        return (f64::NAN, f64::NAN);
    }
    let mean = x.iter().sum::<f64>() / x.len() as f64;
    let (lo, hi) = x.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(*v), b.max(*v)));
    (mean, hi - lo)
}

/// The release after `hold`: the time on its quiet tone after the loud one.
pub fn hold_release(env: &[(f64, f64)], s: &Signal) -> Option<f64> {
    let p = s.parts.get(1).filter(|p| matches!(p.what, What::Tone { .. }))?;
    time(env, s, p)
}
