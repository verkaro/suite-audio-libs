//! The fixture voice in phrases, with and without room tone; and the rules on made-up frames.

use super::*;
use as_bands::{Layout, analyze};
use as_envelope::{corpus, signal};

const RATE: u32 = 48_000;

/// ducky's starting values (`ducky-fit-1`'s `[speech]`, hop 10 ms).
fn params() -> Params {
    Params {
        feature_lo_hz: 300.0,
        feature_hi_hz: 4000.0,
        floor_percentile: 10,
        above_floor_db: 12.0,
        absolute_db: -70.0,
        hold_frames: 35,
        min_phrase_frames: 10,
    }
}

fn layout() -> Layout {
    Layout::erb(RATE, 24, 50.0, 16_000.0).unwrap()
}

fn detect_sound(x: &[f64]) -> Speech {
    let l = layout();
    detect(&analyze(x, &l), &l, &params())
}

/// The fixture: 10 phrases of 2.5 s every 3.3 s.
fn fixture() -> Vec<f64> {
    corpus::phrases(130.0, 0.5, 33.0, 2.5, 0.8, RATE, 7)
}

fn check_ten_phrases(s: &Speech) {
    assert_eq!(s.phrases.len(), 10, "{:?}", s.phrases);
    for (k, (a, z)) in s.phrases.iter().enumerate() {
        let (start, end) = (*a as f64 * 0.01, *z as f64 * 0.01);
        let t0 = k as f64 * 3.3;
        assert!((start - t0).abs() <= 0.05, "phrase {k} starts at {start}, not {t0}");
        assert!(
            end >= t0 + 2.5 - 0.05 && end <= t0 + 2.5 + 0.4,
            "phrase {k} ends at {end}, not {}",
            t0 + 2.5
        );
    }
}

#[test]
fn the_fixture_voice_has_ten_phrases() {
    let s = detect_sound(&fixture());
    check_ten_phrases(&s);
    assert_eq!(s.voiced.len(), 3300);
    // Pauses of zeros: the floor is digital silence.
    assert_eq!(s.floor_db, -120.0);
    // Voiced plus held frames are 0.70–0.80 of the whole.
    let in_phrases: usize = s.phrases.iter().map(|(a, z)| z - a).sum();
    let share = in_phrases as f64 / 3300.0;
    assert!((0.70..=0.80).contains(&share), "{share}");
    // Voiced frames lie in phrases here, and most phrase frames are voiced.
    assert!(
        s.voiced.iter().enumerate().all(|(i, v)| !*v || s.phrases.iter().any(|(a, z)| (*a..*z).contains(&i)))
    );
    let voiced = s.voiced.iter().filter(|v| **v).count();
    assert!(voiced as f64 / in_phrases as f64 > 0.5 && voiced <= in_phrases);
}

#[test]
fn room_tone_in_the_pauses_changes_nothing_much() {
    let clean = detect_sound(&fixture());
    let tone = corpus::room_tone(0.001, 33.0, RATE, 9); // −60 dBFS peak
    let noisy: Vec<f64> = fixture().iter().zip(&tone).map(|(a, b)| a + b).collect();
    let s = detect_sound(&noisy);
    assert!(s.floor_db > -100.0, "the floor is the room tone's: {}", s.floor_db);
    check_ten_phrases(&s);
    for ((a, z), (ca, cz)) in s.phrases.iter().zip(&clean.phrases) {
        assert!(
            (*a as f64 - *ca as f64).abs() * 0.01 <= 0.05 && (*z as f64 - *cz as f64).abs() * 0.01 <= 0.05
        );
    }
}

#[test]
fn silence_has_no_phrase() {
    let s = detect_sound(&vec![0.0; 10 * RATE as usize]);
    assert!(s.phrases.is_empty() && s.voiced.iter().all(|v| !v));
    assert_eq!(s.floor_db, -120.0);
    // Nor does nothing at all.
    let s = detect_sound(&[]);
    assert!(s.phrases.is_empty() && s.voiced.is_empty() && s.floor_db == -120.0);
}

#[test]
fn a_click_has_no_phrase() {
    // 50 ms of noise in two seconds of silence, at several levels: heard, but with the window's
    // spread it fills 8 or 9 frames, short of `min_phrase_frames` (10).
    for amp in [0.05, 0.3, 0.9] {
        let mut x = vec![0.0; 2 * RATE as usize];
        for (i, v) in signal::noise(RATE as usize / 20, amp, 4).iter().enumerate() {
            x[RATE as usize + i] = *v;
        }
        let s = detect_sound(&x);
        let voiced = s.voiced.iter().filter(|v| **v).count();
        assert!(voiced > 0, "the click at {amp} is heard");
        assert!(voiced < 10, "{amp}: {voiced} frames");
        assert!(s.phrases.is_empty(), "{amp}: {:?}", s.phrases);
    }
}

// ---- the rules, on frames made up ----

/// Frames of `dbs` in one in-range band (index 10: ~1 kHz) and, to show they're ignored, a
/// loud 100 Hz band and a loud 12 kHz band.
fn frames(dbs: &[f64]) -> (Vec<f32>, Layout) {
    let l = layout();
    let c = l.centres_hz();
    let (low, mid, high) = (0, c.iter().position(|c| *c > 1000.0).unwrap(), 23);
    assert!(c[low] < 300.0 && c[high] > 4000.0);
    let mut out = vec![0.0f32; dbs.len() * 24];
    for (t, d) in dbs.iter().enumerate() {
        out[t * 24 + mid] = libm::pow(10.0, d / 10.0) as f32;
        out[t * 24 + low] = 1.0;
        out[t * 24 + high] = 1.0;
    }
    (out, l)
}

#[test]
fn only_bands_in_the_range_make_the_feature() {
    let (b, l) = frames(&[-100.0; 40]);
    let s = detect(&b, &l, &params());
    assert!(s.phrases.is_empty() && s.voiced.iter().all(|v| !v), "a loud 100 Hz and 12 kHz are not speech");
    assert!((s.floor_db + 100.0).abs() < 1e-4, "the feature is the 1 kHz band alone: {}", s.floor_db);
    // The range's ends are inclusive: a band centred in [lo, hi] counts. Band 0 holds 1.0 (0 dB).
    let c = l.centres_hz();
    let p = Params { feature_lo_hz: c[0], feature_hi_hz: c[0], ..params() };
    assert!(detect(&b, &l, &p).floor_db.abs() < 1e-4);
    // Just past band 0's centre, band 0 is out; band 1 holds nothing, so the feature clamps at −120.
    let p = Params { feature_lo_hz: c[0] + 1.0, feature_hi_hz: c[1], ..params() };
    assert_eq!(detect(&b, &l, &p).floor_db, -120.0);
}

#[test]
fn the_floor_is_the_nearest_rank_percentile() {
    // 10 frames at −60 … −51 dB: the 10th percentile is index ceil(0.1 · 10) − 1 = 0.
    let (b, l) = frames(&(0..10).map(|k| -60.0 + k as f64).collect::<Vec<_>>());
    let p = Params { absolute_db: -200.0, ..params() };
    assert!((detect(&b, &l, &p).floor_db + 60.0).abs() < 1e-4);
    // 11 frames: ceil(1.1) − 1 = 1, the second lowest.
    let (b, l) = frames(&(0..11).map(|k| -60.0 + k as f64).collect::<Vec<_>>());
    assert!((detect(&b, &l, &p).floor_db + 59.0).abs() < 1e-4);
    // The 100th percentile is the loudest, and the 1st with one frame is that frame.
    let p100 = Params { floor_percentile: 100, ..p.clone() };
    assert!((detect(&b, &l, &p100).floor_db + 50.0).abs() < 1e-4);
    let (b, l) = frames(&[-33.0]);
    assert!((detect(&b, &l, &p).floor_db + 33.0).abs() < 1e-4);
}

#[test]
fn voiced_needs_the_margin_over_the_floor_and_the_absolute_level() {
    // Floor −90 (the quiet frames). Loud frames at −80 pass the absolute level (−70)? No: −80 < −70.
    let mut dbs = vec![-90.0; 50];
    dbs[20] = -80.0; // 10 dB over the floor, under both
    dbs[21] = -75.0; // 15 over the floor, under the absolute
    dbs[22] = -65.0; // over both
    dbs[30] = -78.0; // 12 over exactly is not "above"
    let (b, l) = frames(&dbs);
    let p = Params { absolute_db: -76.0, min_phrase_frames: 1, ..params() };
    let v = detect(&b, &l, &p).voiced;
    let on: Vec<usize> = (0..50).filter(|i| v[*i]).collect();
    // −75 > −76 and > −78: yes; −65: yes; −80: no; −78 (12 over −90, not above): no.
    assert_eq!(on, vec![21, 22]);
}

#[test]
fn gaps_are_held_up_to_the_limit_and_short_runs_dropped() {
    let (loud, quiet) = (-40.0, -100.0);
    let build = |parts: &[(usize, f64)]| -> Vec<f64> {
        parts.iter().flat_map(|(n, d)| std::iter::repeat_n(*d, *n)).collect()
    };
    // Voiced 20, gap of exactly 35, voiced 20: one phrase. A gap of 36: two.
    for (gap, want) in [(35usize, vec![(30, 105)]), (36, vec![(30, 50), (86, 106)])] {
        let dbs = build(&[(30, quiet), (20, loud), (gap, quiet), (20, loud), (30, quiet)]);
        let (b, l) = frames(&dbs);
        assert_eq!(detect(&b, &l, &params()).phrases, want, "gap {gap}");
    }
    // A leading or trailing gap is never held: runs begin and end on voiced frames.
    let dbs = build(&[(5, quiet), (12, loud), (5, quiet)]);
    let (b, l) = frames(&dbs);
    assert_eq!(detect(&b, &l, &params()).phrases, vec![(5, 17)]);
    // 9 frames: dropped. 10: kept (min_phrase_frames 10).
    for (n, kept) in [(9usize, false), (10, true)] {
        let dbs = build(&[(30, quiet), (n, loud), (30, quiet)]);
        let (b, l) = frames(&dbs);
        let s = detect(&b, &l, &params());
        assert_eq!(s.phrases.len(), usize::from(kept), "{n} frames");
        assert_eq!(s.voiced.iter().filter(|v| **v).count(), n, "voiced is each frame's own verdict");
    }
    // Two short voiced runs a hold apart join into a phrase long enough to keep.
    let dbs = build(&[(30, quiet), (6, loud), (20, quiet), (6, loud), (30, quiet)]);
    let (b, l) = frames(&dbs);
    assert_eq!(detect(&b, &l, &params()).phrases, vec![(30, 62)]);
}

#[test]
#[should_panic(expected = "whole number")]
fn a_partial_frame_is_a_mistake() {
    detect(&[0.0; 25], &layout(), &params());
}
