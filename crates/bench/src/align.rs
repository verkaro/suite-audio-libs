//! Finding the sync mark in a capture: the round trip's latency, how sure, and the polarity.

use crate::signal::MARK_GAP_SECS;

/// Where the mark was found.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Found {
    /// The capture's sample where the played mark's first sample came back.
    pub latency: usize,
    /// The correlation's peak against the largest found more than 1 ms from it.
    pub ratio: f64,
    /// The mark came back the way it went.
    pub positive: bool,
}

/// Looks for `mark` in `x` at every lag up to the silence after it (200 ms), so the signal proper
/// never falls under the template. The peak is the largest correlation by
/// magnitude; the "next" is the largest more than 1 ms either side of it, outside the lobe the
/// mark's own carrier makes. `None` if `x` is too short to hold the mark, or nothing in it correlates at all.
pub fn find(x: &[f64], mark: &[f64], rate: u32) -> Option<Found> {
    let m = mark.len();
    if x.len() < m {
        return None;
    }
    let lags = (libm::round(MARK_GAP_SECS * rate as f64) as usize).min(x.len() - m + 1);
    let corr: Vec<f64> = (0..lags)
        .map(|lag| {
            let mut s = 0.0;
            for (a, b) in x[lag..lag + m].iter().zip(mark) {
                s += a * b;
            }
            s
        })
        .collect();
    let (mut at, mut best) = (0, 0.0f64);
    for (k, c) in corr.iter().enumerate() {
        if c.abs() > best {
            best = c.abs();
            at = k;
        }
    }
    if best <= 0.0 {
        return None;
    }
    let guard = libm::round(0.001 * rate as f64) as usize;
    let next = corr
        .iter()
        .enumerate()
        .filter(|(k, _)| k.abs_diff(at) > guard)
        .fold(0.0f64, |n, (_, c)| n.max(c.abs()));
    let ratio = if next > 0.0 { best / next } else { f64::INFINITY };
    Some(Found { latency: at, ratio, positive: corr[at] > 0.0 })
}
