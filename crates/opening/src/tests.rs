//! place's tests of the opening (its `crates/core/src/tests.rs`): the wall never notches with
//! the opening, falls 6 dB an octave, and the levels shown are the gains played.

use super::*;

fn db_of(x: f64) -> f64 {
    20.0 * x.log10()
}

#[test]
fn the_wall_and_the_opening_never_notch() {
    // Wherever the wall's copy (muffled) and the opening's (as it is) meet, with any gains,
    // they add to at least the louder of the two: the wall turns the phase a quarter turn at
    // most, so nothing combs.
    for (_, hz) in WALLS {
        let w = Wall::new(hz, 48000);
        for k in 1..800 {
            let f = 0.5 * k as f64 / 800.0 * 0.999;
            let (re, im) = w.response(f);
            assert!(re >= -1e-12, "{hz} Hz: more than a quarter turn at {f}");
            for (a, b) in [(1.0, 1.0), (0.3, 1.0), (1.0, 0.05), (0.056, 0.45)] {
                let m = ((a * re + b).powi(2) + (a * im).powi(2)).sqrt();
                let louder = (a * (re * re + im * im).sqrt()).max(b);
                assert!(m >= louder * (1.0 - 1e-12), "{hz} Hz, {a}·wall + {b}: {m} under {louder} at {f}");
            }
        }
    }
}

#[test]
fn the_wall_falls_6_db_an_octave_above_its_corner() {
    let w = Wall::new(800.0, 48000);
    let at = |hz: f64| {
        let (re, im) = w.response(hz / 48000.0);
        10.0 * (re * re + im * im).log10()
    };
    assert!(at(50.0).abs() < 0.05, "flat below");
    assert!((at(800.0) + 3.01).abs() < 0.1, "−3 dB at the corner: {}", at(800.0));
    assert!((at(3200.0) - at(6400.0) - 6.0).abs() < 0.5, "an octave up, 6 dB down");
}

#[test]
fn the_levels_shown_are_the_gains_played() {
    let win = window(-0.7, 0.25);
    let op = Opening::default();
    let db_or_floor = |x: f64| if x > 1e-6 { db_of(x) } else { -120.0 };
    for a in [0.0, 0.05, 0.2, 0.5, 1.0] {
        let g = gains(a, &op, &win);
        let [wall, past, _] = levels(a, &op.stops);
        assert!((db_or_floor(g.wall[0][0]) - wall).abs() < 1e-9, "{a}: wall");
        assert!((db_or_floor(g.open[0][0] / win[0][0]) - past).abs() < 1e-9, "{a}: past the opening");
    }
    let s = Stops::default();
    let near = |x: [f64; 3], y: [f64; 3]| x.iter().zip(&y).all(|(a, b)| (a - b).abs() < 1e-9);
    assert!(near(levels(0.0, &s), [-25.0, -120.0, -10.0]));
    assert!(near(levels(s.cracked_at, &s), [-30.0, -7.0, -10.0]));
    assert!(near(levels(1.0, &s), [-120.0, 0.0, -10.0]));
}

#[test]
fn between_two_stops_each_route_is_theirs_crossfaded() {
    let op = Opening::default();
    let win = window(0.3, 0.5);
    let (a, b) = (gains(op.stops.cracked_at, &op, &win), gains(1.0, &op, &win));
    let t = 0.4;
    let m = gains(op.stops.cracked_at + t * (1.0 - op.stops.cracked_at), &op, &win);
    for i in 0..2 {
        for j in 0..2 {
            let want = (1.0 - t) * a.open[i][j] + t * b.open[i][j];
            assert!((m.open[i][j] - want).abs() < 1e-12);
            let want = (1.0 - t) * a.send_wall[i][j] + t * b.send_wall[i][j];
            assert!((m.send_wall[i][j] - want).abs() < 1e-12);
        }
    }
}
