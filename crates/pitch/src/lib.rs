//! A tone's pitch: YIN (de Cheveigne and Kawahara, 2002), then a long-lag refinement that
//! brings it to a few cents from A0 to C8. Written for dexter-like110, 2026-11-04. No
//! dependencies; every sum is plain arithmetic in a fixed order.
//!
//! A frame is `W = tau_max` samples, a hop 10 ms. The difference function is computed directly
//! (the sounds are short and the search band is narrow).

#[cfg(test)]
mod tests;

/// A pitch and how sure it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Estimate {
    pub hz: f64,
    /// 1 minus the median of the voiced frames' d' at their minimum, 0 to 1.
    pub clarity: f64,
    /// The voiced frames' share of all frames, 0 to 1.
    pub voiced: f64,
}

/// One frame's result: its pitch and the d' at its period.
struct Frame {
    hz: f64,
    dprime: f64,
}

/// The frames of `x`, one per hop of 10 ms, `None` where unvoiced. Empty if `x` holds none.
fn frames(x: &[f64], rate: u32, min_hz: f64, max_hz: f64) -> Vec<Option<Frame>> {
    let tau_min = ((rate as f64 / max_hz).floor() as usize).max(2);
    let tau_max = (rate as f64 / min_hz).ceil() as usize;
    let w = tau_max;
    let hop = ((0.010 * rate as f64).round() as usize).max(1);
    if tau_min >= tau_max || x.len() < w + tau_max {
        return Vec::new();
    }
    let n = (x.len() - w - tau_max) / hop + 1;
    let mut d = vec![0.0f64; tau_max + 2];
    let mut dp = vec![1.0f64; tau_max + 2];
    (0..n)
        .map(|i| {
            let o = i * hop;
            let a = &x[o..o + w];
            for tau in 1..=tau_max {
                let b = &x[o + tau..o + tau + w];
                let mut s = 0.0;
                for j in 0..w {
                    let e = a[j] - b[j];
                    s += e * e;
                }
                d[tau] = s;
            }
            let mut run = 0.0;
            dp[0] = 1.0;
            for tau in 1..=tau_max {
                run += d[tau];
                dp[tau] = if run == 0.0 { 1.0 } else { d[tau] * tau as f64 / run };
            }
            // The smallest tau past tau_min under 0.15, walked down to its minimum; else the
            // least d' if it is under 0.35.
            let mut pick = None;
            for tau in tau_min..=tau_max {
                if dp[tau] < 0.15 {
                    let mut t = tau;
                    while t < tau_max && dp[t + 1] < dp[t] {
                        t += 1;
                    }
                    pick = Some(t);
                    break;
                }
            }
            let tau = pick.or_else(|| {
                let t = (tau_min..=tau_max).min_by(|&a, &b| dp[a].total_cmp(&dp[b]))?;
                (dp[t] < 0.35).then_some(t)
            })?;
            let mut star = tau as f64;
            if tau > 1 && tau < tau_max {
                let (l, m, r) = (dp[tau - 1], dp[tau], dp[tau + 1]);
                let den = l - 2.0 * m + r;
                if den.abs() > 1e-12 {
                    star = tau as f64 + 0.5 * (l - r) / den;
                }
            }
            Some(Frame { hz: rate as f64 / star, dprime: dp[tau] })
        })
        .collect()
}

/// The median, the lower middle of an even count. `v` is sorted by the call.
fn median(v: &mut [f64]) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[(v.len() - 1) / 2]
}

/// The pitch of `x` (one channel at `rate`), searched between `min_hz` and `max_hz`; `None` if
/// under half the frames are voiced, or `x` is too short.
pub fn estimate(x: &[f64], rate: u32, min_hz: f64, max_hz: f64) -> Option<Estimate> {
    let fr = frames(x, rate, min_hz, max_hz);
    if fr.is_empty() {
        return None;
    }
    let voiced: Vec<&Frame> = fr.iter().flatten().collect();
    let share = voiced.len() as f64 / fr.len() as f64;
    if share < 0.5 {
        return None;
    }
    let coarse = median(&mut voiced.iter().map(|f| f.hz).collect::<Vec<_>>());
    let clarity = 1.0 - median(&mut voiced.iter().map(|f| f.dprime).collect::<Vec<_>>());
    Some(Estimate { hz: refine(x, rate, coarse), clarity, voiced: share })
}

/// The long-lag refinement: the lag, a whole number of periods, at which the signal best
/// repeats itself. A coarse period's error grows with the number of periods in the lag, so the
/// number doubles stage by stage, each stage correcting the period for the next.
fn refine(x: &[f64], rate: u32, coarse: f64) -> f64 {
    let corr = |l: usize| -> f64 {
        if l == 0 || l >= x.len() {
            return f64::NEG_INFINITY;
        }
        let (a, b) = (&x[..x.len() - l], &x[l..]);
        let (mut ab, mut aa, mut bb) = (0.0, 0.0, 0.0);
        for j in 0..a.len() {
            ab += a[j] * b[j];
            aa += a[j] * a[j];
            bb += b[j] * b[j];
        }
        if aa == 0.0 || bb == 0.0 { f64::NEG_INFINITY } else { ab / (aa * bb).sqrt() }
    };
    let mut t = rate as f64 / coarse;
    let kmax = (((x.len() / 2) as f64 / t).floor() as usize).max(1);
    let mut k = 1;
    loop {
        let l0 = (k as f64 * t).round() as i64;
        let lags: Vec<usize> = (l0 - 2..=l0 + 2).filter(|&l| l >= 1).map(|l| l as usize).collect();
        let Some((&best, _)) = lags.iter().map(|l| (l, corr(*l))).max_by(|a, b| a.1.total_cmp(&b.1)) else {
            return coarse;
        };
        let mut star = best as f64;
        if best >= 2 {
            let (l, m, r) = (corr(best - 1), corr(best), corr(best + 1));
            let den = l - 2.0 * m + r;
            if l.is_finite() && r.is_finite() && m.is_finite() && den.abs() > 1e-12 {
                star = best as f64 + 0.5 * (l - r) / den;
            }
        }
        t = star / k as f64;
        if k == kmax {
            break;
        }
        k = (k * 2).min(kmax);
    }
    let hz = rate as f64 / t;
    if (1200.0 * (hz / coarse).log2()).abs() > 20.0 { coarse } else { hz }
}

/// The frames' pitches, one per hop of 10 ms, `None` where unvoiced (for vibrato's LFO).
pub fn track(x: &[f64], rate: u32, min_hz: f64, max_hz: f64) -> Vec<Option<f64>> {
    frames(x, rate, min_hz, max_hz).into_iter().map(|f| f.map(|f| f.hz)).collect()
}
