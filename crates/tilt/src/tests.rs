//! scape's tests of TILT (its `crates/core/src/tests.rs`), on the filter alone.

use super::*;
use std::f64::consts::TAU;

const RATE: usize = 48000;

fn tilted_db(db: f64, f: f64) -> f64 {
    let n = RATE;
    let x: Vec<f32> = (0..n).map(|k| (TAU * f * k as f64 / RATE as f64).sin() as f32).collect();
    let (mut l, mut r) = (x.clone(), x.clone());
    Tilt::new(db, PIVOT_HZ / RATE as f64).run(&mut l, &mut r);
    let rms = |v: &[f32]| (v.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / v.len() as f64).sqrt();
    assert_eq!(l, r, "both sides alike");
    20.0 * (rms(&l[n / 2..]) / rms(&x[n / 2..])).log10()
}

#[test]
fn tilt_is_a_slope_through_1_khz() {
    for db in [-6.0, -3.0, -1.0, 1.5, 3.0, 6.0] {
        assert!(tilted_db(db, 1000.0).abs() < 0.05, "{db}: the pivot as it was");
        for f in [250.0, 500.0, 2000.0, 4000.0, 8000.0] {
            let want = db * (f / 1000.0f64).log2();
            let got = tilted_db(db, f);
            assert!((got - want).abs() < 1.0, "{db} dB/oct at {f} Hz: {got:.2}, not {want:.2}");
        }
        // Flat outside its band: no more than at its ends.
        let (lo, hi) = (tilted_db(db, 30.0), tilted_db(db, 20_000.0));
        assert!((lo - db * -3.32).abs() < 2.0 && (hi - db * 4.0).abs() < 4.0, "{db}: {lo:.2} {hi:.2}");
    }
}

#[test]
fn no_tilt_is_no_filter() {
    let x: Vec<f32> = (0..10_000).map(|k| ((k * 7919 % 1000) as f32 / 500.0 - 1.0) * 0.5).collect();
    let (mut l, mut r) = (x.clone(), x.clone());
    Tilt::new(0.0, PIVOT_HZ / RATE as f64).run(&mut l, &mut r);
    assert!(l == x && r == x);
}

#[test]
fn a_tilt_moved_live_glides_from_flat_without_a_step() {
    // A 2 kHz tone, then tilted down hard: the first frames after are what they'd have been,
    // then it moves off them a little at a time.
    let x: Vec<f32> = (0..5000).map(|k| (TAU * k as f64 / 24.0).sin() as f32 * 0.5).collect();
    let mut t = Tilt::new(0.0, PIVOT_HZ / RATE as f64);
    let (mut l, mut r) = (x.clone(), x.clone());
    t.run(&mut l[..1000], &mut r[..1000]);
    t.set(-MAX_TILT, PIVOT_HZ / RATE as f64);
    t.run(&mut l[1000..], &mut r[1000..]);
    let (x, y) = (&x[1000..], &l[1000..]);
    let first = (x[0] - y[0]).abs();
    let most = x.iter().zip(y).take(GLIDE).map(|(p, q)| (p - q).abs()).fold(0.0f32, f32::max);
    assert!(first < 0.01, "no step: {first}");
    assert!(most > 0.01, "it moves: {most}");
    let jump = |v: &[f32]| v.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
    assert!(jump(y) < jump(x) * 2.0, "{} {}", jump(y), jump(x));
}

#[test]
fn the_same_at_any_block_size() {
    let x: Vec<f32> = (0..9000).map(|k| ((k * 7919 % 1000) as f32 / 500.0 - 1.0) * 0.5).collect();
    let run = |block: usize| {
        let mut t = Tilt::new(-3.0, PIVOT_HZ / RATE as f64);
        let (mut l, mut r) = (x.clone(), x.clone());
        for (a, b) in l.chunks_mut(block).zip(r.chunks_mut(block)) {
            t.run(a, b);
        }
        l
    };
    assert!(run(64) == run(1000) && run(64) == run(9000));
}
