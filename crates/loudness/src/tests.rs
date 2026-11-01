//! Against EBU Tech 3341's synthetic cases and BS.1770's own coefficients.

use super::*;

fn sine(hz: f64, dbfs: f64, secs: f64, rate: u32) -> Vec<f64> {
    let a = libm::pow(10.0, dbfs / 20.0);
    (0..(secs * rate as f64) as usize)
        .map(|k| a * libm::sin(std::f64::consts::TAU * hz * k as f64 / rate as f64))
        .collect()
}

#[test]
fn k_weighting_is_bs1770s_at_48k() {
    // Back and forth at the same rate is the standard's filter: its impulse response, as
    // the two sections give it straight.
    let x: Vec<f64> = std::iter::once(1.0).chain(std::iter::repeat_n(0.0, 99)).collect();
    let want = biquad(&biquad(&x, K48[0]), K48[1]);
    let got = k_weight(&x, 48_000);
    assert!(got.iter().zip(&want).all(|(a, b)| (a - b).abs() < 1e-12), "{:?} {:?}", &got[..3], &want[..3]);
}

#[test]
fn a_full_scale_sine_reads_minus_3_at_every_rate() {
    for rate in [44_100, 48_000, 88_200, 96_000, 176_400, 192_000] {
        let x = sine(997.0, 0.0, 5.0, rate);
        let l = integrated(&[&x], rate);
        assert!((l.mean.unwrap() + 3.010).abs() < 0.005, "{rate}: {:?}", l);
        assert_eq!(l.mean, l.sum, "mono: the same both ways");
    }
}

#[test]
fn tech_3341_cases_1_to_5() {
    let r = 48_000;
    // 1: stereo, 1 kHz at −23 dBFS each: −23.0 LUFS by the standard; its mean, 3 dB less.
    let x = sine(1000.0, -23.0, 20.0, r);
    let l = integrated(&[&x, &x], r);
    assert!((l.sum.unwrap() + 23.0).abs() < 0.1, "{l:?}");
    assert!((l.mean.unwrap() + 26.01).abs() < 0.1, "{l:?}");
    // 2: −33 dBFS: −33.0.
    let x = sine(1000.0, -33.0, 20.0, r);
    assert!((integrated(&[&x, &x], r).sum.unwrap() + 33.0).abs() < 0.1);
    // 3: −36, −23, −36 dBFS for 10, 60, 10 s: −23.0, the quiet parts gated out.
    // 4: −72, −36, −23, −36, −72 for 10, 10, 60, 10, 10 s: −23.0.
    // 5: −26, −20, −26 for 20, 20.1, 20 s: −23.0.
    let cases: [&[(f64, f64)]; 3] = [
        &[(-36.0, 10.0), (-23.0, 60.0), (-36.0, 10.0)],
        &[(-72.0, 10.0), (-36.0, 10.0), (-23.0, 60.0), (-36.0, 10.0), (-72.0, 10.0)],
        &[(-26.0, 20.0), (-20.0, 20.1), (-26.0, 20.0)],
    ];
    for (i, parts) in cases.iter().enumerate() {
        let x: Vec<f64> = parts.iter().flat_map(|&(d, s)| sine(1000.0, d, s, r)).collect();
        let got = integrated(&[&x, &x], r).sum.unwrap();
        assert!((got + 23.0).abs() < 0.1, "case {}: {got}", i + 3);
    }
}

#[test]
fn silence_and_the_absolute_gate() {
    let r = 48_000;
    let zero = vec![0.0; r as usize * 2];
    assert_eq!(integrated(&[&zero], r).mean, None);
    let faint = sine(1000.0, -75.0, 2.0, r);
    assert_eq!(integrated(&[&faint], r).mean, None, "under −70 LUFS");
    assert_eq!(calibrate(None, -20.0), Calibration::Silent);
}

#[test]
fn a_file_shorter_than_a_block_is_one_block_ungated() {
    let r = 48_000;
    let hit = sine(1000.0, -10.0, 0.25, r);
    let l = integrated(&[&hit], r);
    assert!(l.short);
    assert!((l.mean.unwrap() + 13.01).abs() < 0.05, "{l:?}");
    assert_eq!(integrated(&[&[]], r).mean, None);
}

#[test]
fn trims_reach_the_reference_and_clamp() {
    assert_eq!(calibrate(Some(-23.2), -20.0), Calibration::Trim { db: -20.0 - -23.2, clamped: false });
    assert_eq!(calibrate(Some(-60.0), -20.0), Calibration::Trim { db: 24.0, clamped: true });
    assert_eq!(calibrate(Some(10.0), -20.0), Calibration::Trim { db: -24.0, clamped: true });
}

#[test]
fn a_group_reads_as_loud_as_one_of_its_files() {
    let r = 48_000;
    let boom = sine(220.0, -20.0, 5.0, r);
    let lav = sine(220.0, -20.0, 3.0, 96_000);
    let alone = integrated(&[&boom], r).mean.unwrap();
    let both = group(&[(&[&boom], r), (&[&lav, &lav], 96_000)]).unwrap();
    assert!((both - alone).abs() < 0.02, "{both} vs {alone}");
    // Two at different levels: between them, as their powers' mean.
    let quiet = sine(220.0, -30.0, 5.0, r);
    let g = group(&[(&[&boom], r), (&[&quiet], r)]).unwrap();
    let want = lufs(
        (libm::pow(10.0, (alone + 0.691) / 10.0) + libm::pow(10.0, (alone - 10.0 + 0.691) / 10.0)) / 2.0,
    );
    assert!((g - want).abs() < 0.02, "{g} vs {want}");
}

#[test]
fn true_peak_finds_the_peak_between_samples() {
    // A sine at a quarter of the rate, phased so every sample sits at ±0.707 of its peak.
    let r = 48_000;
    // Faded in and out over 10 ms, so its edges don't ring.
    let fade = |k: usize| f64::min(1.0, f64::min(k as f64, (4799 - k) as f64) / 480.0);
    let x: Vec<f64> = (0..4800)
        .map(|k| {
            fade(k)
                * libm::sin(
                    std::f64::consts::TAU * 12_000.0 * k as f64 / r as f64 + std::f64::consts::FRAC_PI_4,
                )
        })
        .collect();
    let sample_peak = x.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    assert!((sample_peak - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-3);
    let tp = true_peak(&x, r);
    assert!((tp - 1.0).abs() < 0.001, "{tp}");
    assert_eq!(db(1.0), 0.0);
}
