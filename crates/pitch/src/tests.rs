use super::*;
use std::f64::consts::TAU;

const RATE: u32 = 48_000;

fn key_hz(k: u32) -> f64 {
    440.0 * 2f64.powf((k as f64 - 69.0) / 12.0)
}

/// White noise in -1..1, from a seed.
fn noise(n: usize, seed: u64) -> Vec<f64> {
    let mut z = seed;
    (0..n)
        .map(|_| {
            z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut x = z;
            x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            x ^= x >> 31;
            (x >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
        })
        .collect()
}

/// A tone of harmonics `h0..=h1`, each 1/h, with `db` of white noise under it.
fn tone(hz: f64, h0: u32, h1: u32, secs: f64, noise_db: f64) -> Vec<f64> {
    let n = (secs * RATE as f64) as usize;
    let nz = noise(n, 7);
    let g = 10f64.powf(noise_db / 20.0);
    (0..n)
        .map(|i| {
            let ph = hz * i as f64 / RATE as f64;
            let s: f64 = (h0..=h1)
                .filter(|&h| h as f64 * hz < 20_000.0)
                .map(|h| (TAU * h as f64 * ph).sin() / h as f64)
                .sum();
            0.25 * s + g * nz[i]
        })
        .collect()
}

fn cents(a: f64, b: f64) -> f64 {
    1200.0 * (a / b).log2()
}

fn band(hz: f64) -> (f64, f64) {
    ((hz / 2.2).max(20.0), (hz * 2.2).min(10_000.0))
}

#[test]
fn sines() {
    for k in 21..=108 {
        for off in [-30.0, -15.0, 0.0, 15.0, 30.0] {
            let hz = key_hz(k) * 2f64.powf(off / 1200.0);
            let x = tone(hz, 1, 1, 0.5, -60.0);
            let (lo, hi) = band(key_hz(k));
            let e = estimate(&x, RATE, lo, hi).unwrap_or_else(|| panic!("key {k} {off}"));
            assert!(cents(e.hz, hz).abs() < 2.0, "key {k} {off}: {} vs {hz}", e.hz);
        }
    }
}

#[test]
fn harmonic() {
    for k in (21..=108).step_by(3) {
        let hz = key_hz(k);
        let x = tone(hz, 1, 30, 0.5, -60.0);
        let (lo, hi) = band(hz);
        let e = estimate(&x, RATE, lo, hi).unwrap();
        assert!(cents(e.hz, hz).abs() < 2.0, "key {k}: {} vs {hz}", e.hz);
    }
}

#[test]
fn missing_fundamental() {
    for k in 21..=60 {
        let hz = key_hz(k);
        let x = tone(hz, 2, 10, 0.5, -60.0);
        let (lo, hi) = band(hz);
        let e = estimate(&x, RATE, lo, hi).unwrap();
        assert!(cents(e.hz, hz).abs() < 2.0, "key {k}: {} vs {hz}", e.hz);
    }
}

#[test]
fn vibrato() {
    let hz = key_hz(60);
    let n = RATE as usize;
    let mut ph = 0.0;
    let x: Vec<f64> = (0..n)
        .map(|i| {
            let f = hz * 2f64.powf(15.0 * (TAU * 5.5 * i as f64 / RATE as f64).sin() / 1200.0);
            ph += f / RATE as f64;
            0.25 * (TAU * ph).sin()
        })
        .collect();
    let (lo, hi) = band(hz);
    let e = estimate(&x, RATE, lo, hi).unwrap();
    assert!(cents(e.hz, hz).abs() < 3.0, "{} vs {hz}", e.hz);
    let t: Vec<f64> = track(&x, RATE, lo, hi).into_iter().flatten().map(|f| cents(f, hz)).collect();
    assert!(t.len() > 90);
    let spread = t.iter().cloned().fold(f64::MIN, f64::max) - t.iter().cloned().fold(f64::MAX, f64::min);
    assert!((25.0..=35.0).contains(&spread), "spread {spread}");
}

#[test]
fn noise_has_no_pitch() {
    let x = noise(24_000, 3);
    assert_eq!(estimate(&x, RATE, 100.0, 400.0), None);
}

#[test]
fn short() {
    // One and a half periods of A0 are fewer than a frame needs.
    let x = tone(27.5, 1, 1, 1.5 / 27.5, -90.0);
    assert!(x.len() < 3_840);
    assert_eq!(estimate(&x, RATE, 25.0, 60.0), None);
}

#[test]
fn speed() {
    let x = tone(27.5, 1, 1, 0.5, -60.0);
    let t = std::time::Instant::now();
    let e = estimate(&x, RATE, 20.0, 60.5).unwrap();
    assert!(t.elapsed().as_secs_f64() < 1.0, "{:?}", t.elapsed());
    assert!(cents(e.hz, 27.5).abs() < 2.0);
}
