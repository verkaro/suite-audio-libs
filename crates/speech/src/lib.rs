//! Where a voice is speaking, from its band powers (`as-bands`): which frames are voiced, and
//! the phrases they make, judged against the stem's own noise floor so that room tone doesn't
//! count as speech. Written for the suite, 2026-11-02, for ducky; it builds on `as-bands`.
//!
//! 1. **Feature.** Each frame's power in the bands whose centres lie in a speech range
//!    (300–4000 Hz in ducky), in dB, clamped below at −120.
//! 2. **Floor.** A low percentile of the feature over all frames, nearest-rank. Digital silence
//!    clamps to −120 and counts, so a stem with pauses of zeros has a floor of −120.
//! 3. **Voiced.** A frame is voiced when its feature is above the floor by `above_floor_db`
//!    and above `absolute_db`.
//! 4. **Phrases.** Runs of unvoiced frames no longer than `hold_frames` between two voiced
//!    frames are filled; then runs shorter than `min_phrase_frames` are dropped. What remains
//!    are the phrases, `[start, end)` in frames.
//!
//! The arithmetic is `f64` through `libm`, in a fixed order.

use as_bands::Layout;

/// The detector's settings.
#[derive(Clone, Debug, PartialEq)]
pub struct Params {
    /// Bands whose centre lies in `[feature_lo_hz, feature_hi_hz]` make the feature.
    pub feature_lo_hz: f64,
    pub feature_hi_hz: f64,
    /// The floor is this percentile (1–100) of the feature, nearest-rank.
    pub floor_percentile: u32,
    pub above_floor_db: f64,
    pub absolute_db: f64,
    /// Longest run of unvoiced frames, between voiced ones, that a phrase holds across.
    pub hold_frames: usize,
    /// Shortest run of frames that makes a phrase.
    pub min_phrase_frames: usize,
}

/// What was found.
#[derive(Clone, Debug, PartialEq)]
pub struct Speech {
    /// The noise floor, in dB (−120 at the lowest).
    pub floor_db: f64,
    /// Each frame's own verdict, before holding and dropping short runs: a frame inside a
    /// phrase that was only held across, or a click too short for a phrase, may differ from
    /// its phrase's.
    pub voiced: Vec<bool>,
    /// The phrases, `[start, end)` in frames, in order, with no two touching.
    pub phrases: Vec<(usize, usize)>,
}

/// `10 log10(p)`, `p` floored at 1e-20, then clamped below at −120 dB.
fn feature_db(p: f64) -> f64 {
    (10.0 * libm::log10(p.max(1e-20))).max(-120.0)
}

/// Finds the voiced frames and phrases in `bands`, `layout.bands()` powers a frame, frame after
/// frame (as `as_bands::analyze` gives them). Panics if `bands` isn't a whole number of frames.
pub fn detect(bands: &[f32], layout: &Layout, p: &Params) -> Speech {
    let nb = layout.bands();
    assert!(
        bands.len().is_multiple_of(nb),
        "{} values isn't a whole number of {nb}-band frames",
        bands.len()
    );
    let in_range: Vec<bool> =
        layout.centres_hz().iter().map(|c| *c >= p.feature_lo_hz && *c <= p.feature_hi_hz).collect();
    let feature: Vec<f64> = bands
        .chunks(nb)
        .map(|f| {
            let sum = f.iter().zip(&in_range).filter(|(_, r)| **r).fold(0.0, |s, (v, _)| s + *v as f64);
            feature_db(sum)
        })
        .collect();
    let n = feature.len();

    // Nearest-rank percentile: index ceil(p/100 · n) − 1 of the sorted feature.
    let floor_db = if n == 0 {
        -120.0
    } else {
        let mut sorted = feature.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let rank = (p.floor_percentile as usize * n).div_ceil(100).clamp(1, n);
        sorted[rank - 1]
    };

    let voiced: Vec<bool> =
        feature.iter().map(|f| *f > floor_db + p.above_floor_db && *f > p.absolute_db).collect();

    // Hold across short gaps between voiced frames.
    let mut in_phrase = voiced.clone();
    let mut last_voiced: Option<usize> = None;
    for (i, v) in voiced.iter().enumerate() {
        if *v {
            if let Some(l) = last_voiced
                && i - l - 1 <= p.hold_frames
            {
                in_phrase[l + 1..i].fill(true);
            }
            last_voiced = Some(i);
        }
    }

    // Runs of phrase frames, dropping the short ones.
    let mut phrases = Vec::new();
    let mut i = 0;
    while i < n {
        if !in_phrase[i] {
            i += 1;
            continue;
        }
        let start = i;
        while i < n && in_phrase[i] {
            i += 1;
        }
        if i - start >= p.min_phrase_frames {
            phrases.push((start, i));
        }
    }
    Speech { floor_db, voiced, phrases }
}

#[cfg(test)]
mod tests;
