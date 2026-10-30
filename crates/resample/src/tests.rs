//! Tones converted and held against the ideal at the new rate: up and down, near the top of the
//! band, past its Nyquist, a level, and rates whose ratio has no small terms.

use super::*;

/// A sine of `hz` at `rate`, `secs` long, `amp` at its peak.
fn tone(hz: f64, rate: u32, secs: f64, amp: f64) -> Vec<f64> {
    (0..(secs * rate as f64) as usize)
        .map(|k| (std::f64::consts::TAU * hz * k as f64 / rate as f64).sin() * amp)
        .collect()
}

/// The largest difference between `got` and `want`, away from the ends (a tenth of a second).
fn worst(got: &[f64], want: &[f64], rate: u32) -> f64 {
    let edge = rate as usize / 10;
    got[edge..got.len() - edge].iter().zip(&want[edge..]).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max)
}

fn db(x: f64) -> f64 {
    20.0 * x.log10()
}

#[test]
fn a_sound_converted_is_the_same_sound_at_the_new_rate() {
    for (from, to, hz) in [
        (48_000, 44_100, 1000.0),
        (44_100, 48_000, 1000.0),
        (96_000, 48_000, 1000.0),
        (192_000, 44_100, 1000.0),
        (44_100, 96_000, 19_500.0), // near the top: no images above it
        (48_000, 44_100, 19_000.0), // near the top coming down: flat
    ] {
        let got = convert(&tone(hz, from, 1.0, 0.5), from, to);
        assert_eq!(got.len(), to as usize, "{from} to {to}: a second");
        let err = worst(&got, &tone(hz, to, 1.0, 0.5), to);
        // Under what 24 bits hold (−144 dB), and then some.
        assert!(db(err / 0.5) < -145.0, "{from} to {to}, {hz} Hz: {:.1} dB", db(err / 0.5));
    }
}

#[test]
fn above_the_new_nyquist_is_gone() {
    let got = convert(&tone(23_000.0, 48_000, 1.0, 0.5), 48_000, 44_100);
    let peak = worst(&got, &vec![0.0; got.len()], 44_100);
    assert!(db(peak / 0.5) < -140.0, "{:.1} dB", db(peak / 0.5));
}

#[test]
fn a_level_holds_and_the_same_rate_is_the_same_samples() {
    let got = convert(&vec![0.25; 9600], 96_000, 48_000);
    assert!(got[1000..3800].iter().all(|v| (v - 0.25).abs() < 1e-12));
    let x = tone(440.0, 48_000, 0.1, 0.3);
    assert_eq!(convert(&x, 48_000, 48_000), x);
    assert!(convert(&[], 44_100, 48_000).is_empty());
}

#[test]
fn rates_with_no_small_ratio_are_worked_out_as_they_go() {
    // 44100 to 48001: 48001 phases, more than are kept. As exact, a little slower.
    let c = Converter::new(44_100, 48_001);
    assert!(c.table.is_none());
    let got = c.run(&tone(1000.0, 44_100, 0.3, 0.5));
    assert_eq!(got.len(), c.len(13_230));
    let err = worst(&got, &tone(1000.0, 48_001, 0.3, 0.5), 48_001);
    assert!(db(err / 0.5) < -145.0, "{:.1} dB", db(err / 0.5));
}

#[test]
fn one_converter_serves_every_channel_and_f32_too() {
    let c = Converter::new(48_000, 44_100);
    let (l, r) = (tone(1000.0, 48_000, 0.5, 0.5), tone(500.0, 48_000, 0.5, 0.25));
    assert_eq!(c.run(&l), convert(&l, 48_000, 44_100));
    assert_eq!(c.run(&r), convert(&r, 48_000, 44_100));
    let f: Vec<f32> = l.iter().map(|v| *v as f32).collect();
    let got = convert_f32(&f, 48_000, 44_100);
    let want = tone(1000.0, 44_100, 0.5, 0.5);
    let wide: Vec<f64> = got.iter().map(|v| *v as f64).collect();
    assert!(worst(&wide, &want, 44_100) < 1e-6, "f32's own precision");
}

#[test]
#[should_panic(expected = "more than 0 Hz")]
fn a_rate_of_nothing_is_refused() {
    Converter::new(0, 48_000);
}
