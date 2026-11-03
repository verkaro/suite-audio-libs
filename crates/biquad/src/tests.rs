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

// ---- the bilinear designs (added 2026-11-02, for ducky) ----

/// √½, the Q of a maximally flat second order; the golden ratio, the first Q of a fifth-order Butterworth.
const Q2: f64 = std::f64::consts::FRAC_1_SQRT_2;
const PHI: f64 = std::f64::consts::GOLDEN_RATIO;

/// Log-spaced frequencies, `n` of them, `lo` to `hi`.
fn freqs(n: usize, lo: f64, hi: f64) -> Vec<f64> {
    (0..n).map(|k| lo * libm::pow(hi / lo, k as f64 / (n - 1) as f64)).collect()
}

/// The analog section's magnitude at `s = jw`, `b` and `a` in order of the power of `s`.
fn analog(b: [f64; 3], a: [f64; 3], w: f64) -> f64 {
    let m = |c: [f64; 3]| ((c[0] - c[2] * w * w).powi(2) + (c[1] * w).powi(2)).sqrt();
    m(b) / m(a)
}

/// Each design with its analog prototype, as the spec's table writes them (`A = 10^(gain/40)`).
#[allow(clippy::type_complexity)]
fn designs(f0: f64, q: f64, gain: f64, rate: f64) -> Vec<(&'static str, Biquad, [f64; 3], [f64; 3])> {
    let a = libm::pow(10.0, gain / 40.0);
    let r = a.sqrt() / q;
    vec![
        ("highpass", Biquad::highpass(f0, q, rate), [0.0, 0.0, 1.0], [1.0, 1.0 / q, 1.0]),
        ("lowpass", Biquad::lowpass(f0, q, rate), [1.0, 0.0, 0.0], [1.0, 1.0 / q, 1.0]),
        ("low_shelf", Biquad::low_shelf(f0, q, gain, rate), [a * a, a * r, a], [1.0, r, a]),
        ("high_shelf", Biquad::high_shelf(f0, q, gain, rate), [a, a * r, a * a], [a, r, 1.0]),
        (
            "low_shelf_poles_at",
            Biquad::low_shelf_poles_at(f0, q, gain, rate),
            [a * a, a / q, 1.0],
            [1.0, 1.0 / q, 1.0],
        ),
        (
            "high_shelf_poles_at",
            Biquad::high_shelf_poles_at(f0, q, gain, rate),
            [1.0, a / q, a * a],
            [1.0, 1.0 / q, 1.0],
        ),
    ]
}

#[test]
fn each_design_is_its_analog_prototype_at_the_prewarped_frequency() {
    let rate = 48_000.0;
    for f0 in [60.0, 800.0, 4000.0, 12_000.0] {
        let g = libm::tan(PI * f0 / rate);
        for q in [0.5, Q2, 1.0, 2.5] {
            for gain in [-12.0, -3.0, 4.5, 9.0] {
                for (name, f, b, a) in designs(f0, q, gain, rate) {
                    for hz in freqs(24, 20.0, 20_000.0) {
                        let w = libm::tan(PI * hz / rate) / g;
                        let (got, want) = (f.gain(hz, rate), analog(b, a, w));
                        assert!(
                            ((got - want) / want).abs() < 1e-9,
                            "{name} f0 {f0} q {q} gain {gain} at {hz}: {got} vs {want}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn bilinear_of_the_peaks_prototype_is_the_peak() {
    for rate in [44_100.0, 48_000.0, 96_000.0] {
        for (hz, q, gain) in [(50.0, 1.0, 2.0), (1000.0, 0.7, -6.0), (8000.0, 4.0, 11.0), (100.0, 1.4, -1.0)]
        {
            let a = libm::pow(10.0, gain / 40.0);
            let got = Biquad::bilinear([1.0, a / q, 1.0], [1.0, 1.0 / (a * q), 1.0], hz, rate);
            let want = Biquad::peak(hz, q, gain, rate);
            for (g, w) in [
                (got.b0, want.b0),
                (got.b1, want.b1),
                (got.b2, want.b2),
                (got.a1, want.a1),
                (got.a2, want.a2),
            ] {
                assert!((g - w).abs() < 1e-12, "{rate} {hz} {q} {gain}: {got:?} vs {want:?}");
            }
        }
    }
}

#[test]
fn the_cookbook_shelves_have_the_cookbooks_coefficients() {
    // Bristow-Johnson's published formulas, written out here independently of `bilinear`.
    for rate in [44_100.0, 48_000.0, 96_000.0] {
        for (hz, q, gain) in [(100.0, Q2, 6.0), (250.0, 1.0, -9.0), (6000.0, 0.5, 3.0), (10_000.0, 2.0, -4.0)]
        {
            let a = libm::pow(10.0, gain / 40.0);
            let w = 2.0 * PI * hz / rate;
            let (c, al) = (libm::cos(w), libm::sin(w) / (2.0 * q));
            let t = 2.0 * a.sqrt() * al;
            let low = {
                let a0 = (a + 1.0) + (a - 1.0) * c + t;
                Biquad {
                    b0: a * ((a + 1.0) - (a - 1.0) * c + t) / a0,
                    b1: 2.0 * a * ((a - 1.0) - (a + 1.0) * c) / a0,
                    b2: a * ((a + 1.0) - (a - 1.0) * c - t) / a0,
                    a1: -2.0 * ((a - 1.0) + (a + 1.0) * c) / a0,
                    a2: ((a + 1.0) + (a - 1.0) * c - t) / a0,
                }
            };
            let high = {
                let a0 = (a + 1.0) - (a - 1.0) * c + t;
                Biquad {
                    b0: a * ((a + 1.0) + (a - 1.0) * c + t) / a0,
                    b1: -2.0 * a * ((a - 1.0) + (a + 1.0) * c) / a0,
                    b2: a * ((a + 1.0) + (a - 1.0) * c - t) / a0,
                    a1: 2.0 * ((a - 1.0) - (a + 1.0) * c) / a0,
                    a2: ((a + 1.0) - (a - 1.0) * c - t) / a0,
                }
            };
            for (name, got, want) in [
                ("low", Biquad::low_shelf(hz, q, gain, rate), low),
                ("high", Biquad::high_shelf(hz, q, gain, rate), high),
            ] {
                for (g, w) in [
                    (got.b0, want.b0),
                    (got.b1, want.b1),
                    (got.b2, want.b2),
                    (got.a1, want.a1),
                    (got.a2, want.a2),
                ] {
                    assert!((g - w).abs() < 1e-12, "{name} {rate} {hz} {q} {gain}: {got:?} vs {want:?}");
                }
            }
        }
    }
}

#[test]
fn shelves_have_their_dc_nyquist_and_corner_gains() {
    let rate = 48_000.0;
    for (hz, q) in [(200.0, Q2), (3000.0, 1.0), (9000.0, 0.6)] {
        for gain in [-10.0, -2.0, 5.0, 12.0] {
            let at = |f: &Biquad, x: f64| db(f.gain(x, rate));
            // Cookbook: the shelf's gain at one end, 0 dB at the other, half its gain at the corner.
            let (lo, hi) = (Biquad::low_shelf(hz, q, gain, rate), Biquad::high_shelf(hz, q, gain, rate));
            assert!((at(&lo, 1e-4) - gain).abs() < 1e-6 && at(&lo, rate / 2.0).abs() < 1e-6);
            assert!((at(&hi, rate / 2.0) - gain).abs() < 1e-6 && at(&hi, 1e-4).abs() < 1e-6);
            assert!((at(&lo, hz) - gain / 2.0).abs() < 1e-9 && (at(&hi, hz) - gain / 2.0).abs() < 1e-9);
            // Poles-at: the same ends. Its corner gain differs from the cookbook's.
            let (lo, hi) = (
                Biquad::low_shelf_poles_at(hz, q, gain, rate),
                Biquad::high_shelf_poles_at(hz, q, gain, rate),
            );
            assert!((at(&lo, 1e-4) - gain).abs() < 1e-6 && at(&lo, rate / 2.0).abs() < 1e-6);
            assert!((at(&hi, rate / 2.0) - gain).abs() < 1e-6 && at(&hi, 1e-4).abs() < 1e-6);
        }
    }
}

#[test]
fn poles_at_shelves_keep_their_poles_where_the_lowpass_has_them() {
    // The analog poles of s² + s/q + 1 map by z = (1 + g p) / (1 − g p) to the digital ones,
    // whatever the gain; the shelf's own denominator is a1 = −2 r cos θ, a2 = r².
    let rate = 48_000.0;
    for (hz, q) in [(100.0f64, Q2), (1000.0, 1.0), (5000.0, 3.0), (11_000.0, 0.8)] {
        let g = libm::tan(PI * hz / rate);
        let (pre, pim) = (-1.0 / (2.0 * q), (1.0 - 1.0 / (4.0 * q * q)).sqrt());
        // z = (1 + g p) / (1 − g p), p = pre + j pim.
        let (nr, ni, dr, di) = (1.0 + g * pre, g * pim, 1.0 - g * pre, -g * pim);
        let den = dr * dr + di * di;
        let (zr, zi) = ((nr * dr + ni * di) / den, (ni * dr - nr * di) / den);
        let (radius, angle) = ((zr * zr + zi * zi).sqrt(), libm::atan2(zi, zr));
        for gain in [-9.0, -1.0, 3.0, 14.0] {
            for f in [
                Biquad::low_shelf_poles_at(hz, q, gain, rate),
                Biquad::high_shelf_poles_at(hz, q, gain, rate),
            ] {
                assert!((f.a2.sqrt() - radius).abs() < 1e-12, "{hz} {q} {gain}: radius");
                let theta = libm::acos(-f.a1 / (2.0 * f.a2.sqrt()));
                assert!((theta - angle).abs() < 1e-12, "{hz} {q} {gain}: angle");
            }
        }
    }
    // And at high Q the angle is the corner's, to the warp: 2π f0 / rate.
    let f = Biquad::low_shelf_poles_at(1000.0, 30.0, 6.0, rate);
    let theta = libm::acos(-f.a1 / (2.0 * f.a2.sqrt()));
    assert!((theta / (2.0 * PI * 1000.0 / rate) - 1.0).abs() < 0.01, "{theta}");
}

#[test]
fn zero_db_shelves_are_the_identity() {
    for rate in [44_100.0, 96_000.0] {
        assert_eq!(Biquad::low_shelf(500.0, 0.7, 0.0, rate), Biquad::IDENTITY);
        assert_eq!(Biquad::high_shelf(500.0, 0.7, 0.0, rate), Biquad::IDENTITY);
        assert_eq!(Biquad::low_shelf_poles_at(500.0, 0.7, 0.0, rate), Biquad::IDENTITY);
        assert_eq!(Biquad::high_shelf_poles_at(500.0, 0.7, 0.0, rate), Biquad::IDENTITY);
    }
}

#[test]
fn a_second_order_pass_is_q_down_at_its_corner() {
    let rate = 48_000.0;
    for q in [0.5, Q2, 1.0, 3.0] {
        for hz in [80.0, 1000.0, 10_000.0] {
            let (hp, lp) = (Biquad::highpass(hz, q, rate), Biquad::lowpass(hz, q, rate));
            assert!((hp.gain(hz, rate) - q).abs() < 1e-9 && (lp.gain(hz, rate) - q).abs() < 1e-9);
        }
    }
    let hp = Biquad::highpass(100.0, Q2, 48_000.0);
    assert!((db(hp.gain(100.0, 48_000.0)) + 3.0).abs() < 0.02);
    // 12 dB an octave below, 0 dB an octave of decades above.
    assert!((db(hp.gain(50.0, 48_000.0)) + 12.3).abs() < 0.5);
    assert!(db(hp.gain(10_000.0, 48_000.0)).abs() < 0.01);
}

#[test]
fn butterworth_sections_are_the_formulas() {
    assert_eq!(butterworth(0), (false, vec![]));
    assert_eq!(butterworth(1), (true, vec![]));
    let (first, qs) = butterworth(2);
    assert!(!first && qs.len() == 1 && (qs[0] - 0.5f64.sqrt()).abs() < 1e-12);
    let (first, qs) = butterworth(3);
    assert!(first && qs.len() == 1 && (qs[0] - 1.0).abs() < 1e-12, "{qs:?}");
    let (first, qs) = butterworth(4);
    assert!(
        !first && qs.len() == 2 && (qs[0] - 1.3066).abs() < 1e-4 && (qs[1] - 0.5412).abs() < 1e-4,
        "{qs:?}"
    );
    let (first, qs) = butterworth(5);
    assert!(
        first && qs.len() == 2 && (qs[0] - PHI).abs() < 1e-12 && (qs[1] - 1.0 / PHI).abs() < 1e-12,
        "{qs:?}"
    );
}

#[test]
fn a_butterworth_cascade_is_flat_in_the_pass_and_3_db_down_at_the_corner() {
    let rate = 48_000.0;
    for n in 1..=8usize {
        let (first, qs) = butterworth(n);
        let fc = 400.0;
        let gain = |hz: f64| {
            let mut g = if first { First::highpass(fc, rate).gain(hz, rate) } else { 1.0 };
            for &q in &qs {
                g *= Biquad::highpass(fc, q, rate).gain(hz, rate);
            }
            g
        };
        assert!((db(gain(fc)) + 3.0103).abs() < 1e-6, "order {n}: {}", db(gain(fc)));
        assert!(db(gain(15_000.0)).abs() < 0.01, "order {n}");
        // n times 6 dB an octave, well below the corner.
        let slope = db(gain(fc / 32.0)) - db(gain(fc / 64.0));
        assert!((slope - 6.0206 * n as f64).abs() < 0.3, "order {n}: {slope}");
    }
    // The third order in particular, as the spec has it: first order plus Q 1.
    let g3 = First::highpass(100.0, 48_000.0).gain(100.0, 48_000.0)
        * Biquad::highpass(100.0, 1.0, 48_000.0).gain(100.0, 48_000.0);
    assert!((db(g3) + 3.01).abs() < 0.005, "{}", db(g3));
}

#[test]
fn q_from_octaves_is_the_analog_relation() {
    assert!((q_from_octaves(1.0) - 2.0f64.sqrt()).abs() < 1e-12);
    assert!((q_from_octaves(2.0) - 2.0 / 3.0).abs() < 1e-12);
    for bw in [0.1, 0.5, 1.0, 1.5, 3.0] {
        let want = 1.0 / (libm::pow(2.0, bw / 2.0) - libm::pow(2.0, -bw / 2.0));
        assert!((q_from_octaves(bw) - want).abs() < 1e-12 * want, "{bw}");
    }
    // The band's edges f0 · 2^(±bw/2) are where the analog band-pass is −3 dB: |W − 1/W| = 1/Q.
    let (bw, q) = (1.5, q_from_octaves(1.5));
    let w = libm::pow(2.0, bw / 2.0);
    assert!((1.0 / (w - 1.0 / w) - q).abs() < 1e-12);
}

#[test]
fn the_new_designs_in_pieces_are_the_whole() {
    let x: Vec<f64> =
        (0..10_000).map(|k| libm::sin(k as f64 * 0.37) + 0.1 * libm::cos(k as f64 * 0.011)).collect();
    for (_, f, _, _) in designs(1200.0, 0.9, 7.0, 48_000.0) {
        let mut whole = x.clone();
        f.run(&mut whole);
        let mut parts = x.clone();
        let mut st = [0.0; 4];
        for c in parts.chunks_mut(331) {
            f.process(c, &mut st);
        }
        assert_eq!(parts, whole);
    }
}
