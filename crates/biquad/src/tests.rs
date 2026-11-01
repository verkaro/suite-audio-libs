//! Each design against its analog filter, and the shelf against its inverse.

use super::*;

fn db(v: f64) -> f64 {
    20.0 * libm::log10(v)
}

#[test]
fn a_shelf_follows_its_analog_curve_and_levels_off() {
    let rate = 384_000.0;
    let (zero, g) = (4547.28, 6.0); // 35 µs, +6 dB
    let f = First::shelf(zero, g, rate);
    let analog = |hz: f64| {
        let (wz, wp) = (hz / zero, hz / (zero * libm::pow(10.0, g / 20.0)));
        db(((1.0 + wz * wz) / (1.0 + wp * wp)).sqrt())
    };
    assert!(db(f.gain(1e-3, rate)).abs() < 1e-9, "0 dB at DC");
    for hz in [100.0, 1000.0, 4547.28, 10_000.0, 20_000.0] {
        assert!((db(f.gain(hz, rate)) - analog(hz)).abs() < 0.02, "{hz}: {}", db(f.gain(hz, rate)));
    }
    // At the zero, exactly the analog +3 dB-ish rise, by the prewarp.
    assert!((db(f.gain(zero, rate)) - analog(zero)).abs() < 1e-9);
    assert!((db(f.gain(150_000.0, rate)) - g).abs() < 0.1, "levelled off near +6");
    assert_eq!(First::shelf(zero, 0.0, rate), First::IDENTITY);
}

#[test]
fn a_shelf_and_its_inverse_null() {
    let rate = 384_000.0;
    let mut x: Vec<f64> = (0..20_000u64)
        .map(|k| {
            let h = k.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 11;
            h as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
        })
        .collect();
    let orig = x.clone();
    for (zero, g) in [(2273.64, 9.0), (9094.57, 3.0), (3183.1, 6.0)] {
        let f = First::shelf(zero, g, rate);
        f.run(&mut x);
        f.inverse().run(&mut x);
        let worst = x.iter().zip(&orig).fold(0.0f64, |m, (a, b)| m.max((a - b).abs()));
        assert!(db(worst) < -140.0, "{zero} {g}: {} dB", db(worst));
        x.copy_from_slice(&orig);
    }
}

#[test]
fn a_highpass_is_minus_3_at_its_corner() {
    let f = First::highpass(2.0, 48_000.0);
    assert!((db(f.gain(2.0, 48_000.0)) + 3.0103).abs() < 1e-6);
    assert!(db(f.gain(30.0, 48_000.0)).abs() < 0.02);
    assert!(db(f.gain(1e-4, 48_000.0)) < -80.0);
}

#[test]
fn a_peak_has_its_gain_at_its_frequency() {
    for rate in [44_100.0, 48_000.0, 192_000.0] {
        let p = Biquad::peak(50.0, 1.0, 2.0, rate);
        assert!((db(p.gain(50.0, rate)) - 2.0).abs() < 1e-9);
        assert!(db(p.gain(5000.0, rate)).abs() < 0.01);
        let d = Biquad::peak(100.0, 1.4, -1.0, rate);
        assert!((db(d.gain(100.0, rate)) + 1.0).abs() < 1e-9);
    }
}

#[test]
fn the_matched_lowpass_is_minus_3_where_asked_and_follows_the_analog() {
    let corner = (2.0f64.sqrt() - 1.0).sqrt(); // Q 0.5: −3 dB at this part of f0
    for rate in [44_100.0, 48_000.0, 88_200.0, 96_000.0, 176_400.0, 192_000.0] {
        for fh in [12_000.0, 16_000.0, 20_000.0] {
            let f0 = fh / corner;
            let lp = Biquad::lowpass_matched(f0, 0.5, fh, rate);
            assert!((db(lp.gain(fh, rate)) + 3.0103).abs() < 1e-6, "{rate} {fh}: {}", db(lp.gain(fh, rate)));
            assert!(db(lp.gain(1.0, rate)).abs() < 1e-6);
            // Below f_h, within a few tenths of the analog.
            for hz in [1000.0, 5000.0, 10_000.0] {
                let x = hz / f0;
                let analog = -db(1.0 + x * x);
                assert!(
                    (db(lp.gain(hz, rate)) - analog).abs() < 0.3,
                    "{rate} {fh} {hz}: {} vs {analog}",
                    db(lp.gain(hz, rate))
                );
            }
            // Monotone down to Nyquist: no shelf, no rise.
            let mut last = 1.0;
            for k in 1..100 {
                let g = lp.gain(rate / 2.0 * k as f64 / 100.0, rate);
                assert!(g <= last + 1e-12, "{rate} {fh}");
                last = g;
            }
        }
    }
}

#[test]
fn run_and_gain_agree() {
    // A tone through each section has the gain the formula says.
    let rate = 48_000.0;
    let hz = 3000.0;
    let tone =
        |n: usize| -> Vec<f64> { (0..n).map(|k| libm::sin(2.0 * PI * hz * k as f64 / rate)).collect() };
    // The amplitude from the second half's RMS: whole cycles, the start's transient gone.
    let peak = |x: &[f64]| {
        let h = &x[x.len() / 2..];
        (2.0 * h.iter().map(|v| v * v).sum::<f64>() / h.len() as f64).sqrt()
    };
    let mut x = tone(48_000);
    let lp = Biquad::lowpass_matched(16_000.0 / 0.6436, 0.5, 16_000.0, rate);
    lp.run(&mut x);
    assert!((peak(&x) - lp.gain(hz, rate)).abs() < 1e-4);
    let mut y = tone(48_000);
    let sh = First::shelf(2273.64, 9.0, rate);
    sh.run(&mut y);
    assert!((peak(&y) - sh.gain(hz, rate)).abs() < 1e-4);
}

#[test]
fn pieces_with_their_state_are_the_whole() {
    let x: Vec<f64> =
        (0..10_000).map(|k| libm::sin(k as f64 * 0.37) + 0.1 * libm::cos(k as f64 * 0.011)).collect();
    let (f, b) =
        (First::shelf(3000.0, 6.0, 48_000.0), Biquad::lowpass_matched(25_000.0, 0.5, 16_000.0, 48_000.0));
    let (mut wf, mut wb) = (x.clone(), x.clone());
    f.run(&mut wf);
    b.run(&mut wb);
    let (mut pf, mut pb) = (x.clone(), x);
    let (mut sf, mut sb) = ([0.0; 2], [0.0; 4]);
    for (cf, cb) in pf.chunks_mut(777).zip(pb.chunks_mut(777)) {
        f.process(cf, &mut sf);
        b.process(cb, &mut sb);
    }
    assert_eq!((pf, pb), (wf, wb));
}
