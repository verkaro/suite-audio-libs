//! scape's tests of the seam (its `crates/core/src/tests.rs`), on a sound as its channels.

use super::*;

const RATE: usize = 48000;

/// Noise from a seed: a sound that's unlike itself everywhere.
fn noise(n: usize, seed: u64) -> Vec<f32> {
    let mut z = seed;
    (0..n)
        .map(|_| {
            z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut x = z;
            x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            x ^= x >> 31;
            ((x >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0) as f32 * 0.5
        })
        .collect()
}

fn sine(n: usize, period: usize) -> Vec<f32> {
    (0..n).map(|k| (std::f64::consts::TAU * k as f64 / period as f64).sin() as f32 * 0.5).collect()
}

#[test]
fn the_laws_hold_the_level() {
    for x in [0.0, 0.1, 0.25, 0.5, 0.9, 1.0] {
        let (a, b) = Fade::Power.gains(x);
        assert!((a * a + b * b - 1.0).abs() < 1e-12);
        let (a, b) = Fade::Matched(0.0).gains(x);
        assert!((a * a + b * b - 1.0).abs() < 1e-12, "unlike sides: power");
        let (a, b) = Fade::Matched(1.0).gains(x);
        assert!((a + b - 1.0).abs() < 1e-12, "the same side: gain");
        let (a, b) = Fade::Matched(0.4).gains(x);
        assert!((a * a + b * b + 0.8 * a * b - 1.0).abs() < 1e-12);
    }
    assert_eq!(Fade::Matched(0.0).gains(0.0), (1.0, 0.0));
    assert_eq!(Fade::new(Law::Auto, -0.9), Fade::Matched(-0.5), "never so unlike it divides by nothing");
    assert_eq!(Fade::new(Law::Power, 1.0), Fade::Power);
}

#[test]
fn likeness_is_correlation() {
    let n = noise(10_000, 4);
    let mut doubled = n.clone();
    doubled.extend_from_slice(&n);
    let s = vec![doubled];
    // END at 20000 fades into the 1000 before LOOP at 10000: the same audio.
    let j = Join::new(Seam::Fwd, 0, 10_000, 20_000, 1000).unwrap();
    assert!((likeness(&s, &j) - 1.0).abs() < 1e-9);
    let j = Join::new(Seam::Fwd, 0, 5_000, 20_000, 1000).unwrap();
    assert!(likeness(&s, &j).abs() < 0.1);
    let neg: Vec<f32> = n.iter().map(|v| -v).collect();
    let s = vec![n.iter().chain(&neg).cloned().collect()];
    assert!((likeness(&s, &Join::new(Seam::Fwd, 0, 10_000, 20_000, 1000).unwrap()) + 1.0).abs() < 1e-9);
    assert_eq!(likeness(&s, &Join::new(Seam::Fwd, 0, 10_000, 20_000, 0).unwrap()), 0.0);
}

#[test]
fn joins_fit_their_loop() {
    let j = Join::new(Seam::Fwd, 0, 500, 1000, 300).unwrap();
    assert_eq!(
        (j.xf, j.wrap_to, j.time_round(), j.incoming(0)),
        (300, 500, 500, 200),
        "the audio before LOOP"
    );
    let j = Join::new(Seam::Fwd, 0, 100, 1000, 5000).unwrap();
    assert_eq!((j.xf, j.wrap_to, j.time_round()), (450, 550, 450), "not enough before LOOP: inside");
    let j = Join::new(Seam::Fwd, 0, 0, 1000, 5000).unwrap();
    assert_eq!((j.xf, j.wrap_to), (500, 500), "inside, half the loop");
    let j = Join::new(Seam::Bfly, 0, 100, 1000, 5000).unwrap();
    assert_eq!((j.xf, j.wrap_to, j.incoming(0), j.time_round()), (449, 101, 548, 899));
    let j = Join::new(Seam::Bfly, 0, 100, 1000, 0).unwrap();
    assert_eq!((j.seam, j.xf, j.wrap_to), (Seam::Bfly, 0, 100), "with no fade, a splice");
    assert_eq!(Join::new(Seam::Alt, 0, 999, 1000, 0), None);
    assert_eq!(Join::new(Seam::Fwd, 0, 1000, 1000, 0), None);
}

#[test]
fn find_finds_where_the_audio_carries_on() {
    // Noise in which the stretch around 15000 is a copy of the one around 3000: a splice from
    // 15000 to 3000 is seamless, and FIND finds it from either point.
    let mut n = noise(20_000, 5);
    let copy: Vec<f32> = n[2_500..3_500].to_vec();
    n[14_500..15_500].copy_from_slice(&copy);
    let s = vec![n];
    let j = Join::new(Seam::Fwd, 0, 3_137, 15_000, 0).unwrap();
    let f = find(&s, &j, 0, Point::Loop, 400, 400);
    assert_eq!(f.at, 3_000);
    assert!(f.score > 0.999 && f.was < 0.6, "{f:?}");
    let j = Join::new(Seam::Fwd, 0, 3_000, 14_789, 0).unwrap();
    assert_eq!(find(&s, &j, 0, Point::End, 400, 400).at, 15_000);
    // Already there, it stays.
    let j = Join::new(Seam::Fwd, 0, 3_000, 15_000, 0).unwrap();
    let f = find(&s, &j, 0, Point::Loop, 400, 400);
    assert_eq!((f.at, f.score), (3_000, f.was));
}

#[test]
fn find_on_a_tone_makes_whole_cycles() {
    let s = vec![sine(RATE, 100)];
    let j = Join::new(Seam::Fwd, 0, 1_000, 5_037, 0).unwrap();
    let f = find(&s, &j, 0, Point::End, 100, 96);
    assert_eq!((f.at - 1_000) % 100, 0, "{f:?}");
    assert!(f.score > 0.9999);
}

#[test]
fn find_for_alt_turns_where_the_wave_is_flat() {
    let s = vec![sine(RATE, 100)];
    let j = Join::new(Seam::Alt, 0, 1_010, 5_000, 0).unwrap();
    let f = find(&s, &j, 0, Point::Loop, 30, 0);
    assert_eq!(f.at % 50, 25, "{f:?}");
}

#[test]
fn find_keeps_room_for_the_fade() {
    let s = vec![noise(10_000, 6)];
    let j = Join::new(Seam::Fwd, 0, 4_000, 5_000, 300).unwrap();
    let f = find(&s, &j, 300, Point::Loop, 5_000, 200);
    assert!(f.at <= 5_000 - 602, "{f:?}");
    let f = find(&s, &j, 300, Point::End, 5_000, 200);
    assert!(f.at >= 4_000 + 602 && f.at <= 10_000, "{f:?}");
}

#[test]
fn find_matches_levels_in_noise() {
    // Noise that's loud from 6000 to 9000 and quiet elsewhere: a loop ending in the quiet at
    // 15000 shouldn't go back into the loud part. FIND takes LOOP out of it.
    let mut n = noise(20_000, 8);
    for v in &mut n[6_000..9_000] {
        *v *= 8.0;
    }
    let s = vec![n];
    let j = Join::new(Seam::Fwd, 0, 7_000, 15_000, 1_000).unwrap();
    let f = find(&s, &j, 1_000, Point::Loop, 3_000, 96);
    assert!(!(5_500..9_500).contains(&f.at) && f.score > f.was + 0.2, "{f:?}");
}
