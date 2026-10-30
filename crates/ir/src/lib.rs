//! An IR made ready to play (written for the suite, 2026-10-29, after place's `docs/design.md`,
//! "Reading an IR" and "The outside's space"): a sound's channels in, as a send of one signal
//! hears them out.
//!
//! - [`fold`]: four-channel true stereo in Ardour's order (L→L, L→R, R→L, R→R) fed one signal
//!   on both inputs is stereo: left L→L + R→L, right L→R + R→R.
//! - [`unit_energy`]: the gain that plays it at unit energy (its taps squared, summed, the
//!   channels averaged, made 1), so a send's dB mean the same in any room, and noise sent in
//!   comes back at the level it went.
//! - [`onset`]: its first frame within 20 dB of its peak.
//! - [`echoes`]: the IR from its onset, its direct sound left out (the first 2.5 ms, then faded
//!   in over a millisecond), for a recording that is its own direct sound: sent through the IR's
//!   too, it would reach the output twice, a little apart, and comb.

#[cfg(test)]
mod tests;

/// How long an IR's direct sound is taken to be, after its onset, seconds.
pub const DIRECT: f64 = 0.0025;
/// How long the echoes fade in over, after it, seconds.
pub const FADE: f64 = 0.001;

/// True stereo folded to stereo; one or two channels as they are. Any other number: none.
pub fn fold(c: Vec<Vec<f32>>) -> Option<Vec<Vec<f32>>> {
    match c.len() {
        1 | 2 => Some(c),
        4 => {
            let l = c[0].iter().zip(&c[2]).map(|(a, b)| a + b).collect();
            let r = c[1].iter().zip(&c[3]).map(|(a, b)| a + b).collect();
            Some(vec![l, r])
        }
        _ => None,
    }
}

/// The gain that brings `c` to unit energy (1 for silence).
pub fn unit_energy(c: &[Vec<f32>]) -> f32 {
    if c.is_empty() {
        return 1.0;
    }
    let e: f64 =
        c.iter().map(|ch| ch.iter().map(|v| (*v as f64).powi(2)).sum::<f64>()).sum::<f64>() / c.len() as f64;
    if e > 0.0 { (1.0 / e.sqrt()) as f32 } else { 1.0 }
}

/// The first frame whose energy, every channel summed, is within 20 dB of the peak's.
pub fn onset(c: &[Vec<f32>]) -> usize {
    let n = c.first().map_or(0, Vec::len);
    let e = |k: usize| c.iter().map(|ch| (ch[k] as f64).powi(2)).sum::<f64>();
    let peak = (0..n).map(e).fold(0.0, f64::max);
    (0..n).position(|k| e(k) >= peak / 100.0).unwrap_or(0)
}

/// The IR's echoes at `rate`: from its onset, the first [`DIRECT`] left out and the next
/// [`FADE`] faded in (a raised cosine), the rest as it is. Its level isn't touched: scale it by
/// the whole IR's [`unit_energy`].
pub fn echoes(c: &[Vec<f32>], rate: u32) -> Vec<Vec<f32>> {
    let from = onset(c);
    let gap = (DIRECT * rate as f64).round() as usize;
    let fade = ((FADE * rate as f64).round() as usize).max(1);
    c.iter()
        .map(|ch| {
            ch[from.min(ch.len())..]
                .iter()
                .enumerate()
                .map(|(k, v)| {
                    if k < gap {
                        0.0
                    } else if k < gap + fade {
                        let x = (k - gap) as f64 / fade as f64;
                        v * (0.5 - 0.5 * (std::f64::consts::PI * x).cos()) as f32
                    } else {
                        *v
                    }
                })
                .collect()
        })
        .collect()
}
