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

/// FNV-1a over the taps' bits, every phase in order.
fn taps_hash(c: &Converter) -> u64 {
    let t = c.table.as_ref().expect("a small ratio keeps its taps");
    t.iter()
        .flat_map(|v| v.to_bits().to_le_bytes())
        .fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

#[test]
fn the_taps_are_the_same_on_every_machine() {
    // Tape's oversampling, up and back for each of its six rates (its RFC, S2 and S6): the taps
    // come out of libm and IEEE arithmetic, so these hashes hold on x86_64 and aarch64 alike.
    // The taps depend on the ratio alone, so each factor's pair repeats. A change here changes
    // every sound converted, and Tape's detent tables with it.
    let pinned: [(u32, u32, u64); 12] = [
        (44_100, 352_800, 0x1743cad5e6ff0dd),
        (352_800, 44_100, 0x445da6e79e884835),
        (48_000, 384_000, 0x1743cad5e6ff0dd),
        (384_000, 48_000, 0x445da6e79e884835),
        (88_200, 352_800, 0xd6e8773609c84c71),
        (352_800, 88_200, 0xd438ba5a1b9aef25),
        (96_000, 384_000, 0xd6e8773609c84c71),
        (384_000, 96_000, 0xd438ba5a1b9aef25),
        (176_400, 352_800, 0x348be144566dfe61),
        (352_800, 176_400, 0x759f13122d80cd30),
        (192_000, 384_000, 0x348be144566dfe61),
        (384_000, 192_000, 0x759f13122d80cd30),
    ];
    let got: Vec<_> = pinned.iter().map(|&(f, t, _)| (f, t, taps_hash(&Converter::new(f, t)))).collect();
    for (want, got) in pinned.iter().zip(&got) {
        assert_eq!(want, got, "every pair now: {got:#x?}");
    }
}

#[test]
fn a_piece_at_a_time_with_its_reach_is_the_whole_bit_for_bit() {
    for (from, to) in [(48_000, 384_000), (384_000, 48_000), (96_000, 384_000), (352_800, 176_400)] {
        let c = Converter::new(from, to);
        let (up, down) = c.ratio();
        let x = tone(997.0, from, 0.2, 0.5);
        let whole = c.run(&x);
        let (n, r) = (x.len(), c.reach());
        // Pieces of 1000 inputs (a whole number of `down`s), each with `reach` either side.
        let step = 1000 / down * down;
        let mut got = vec![];
        for a in (0..n).step_by(step) {
            let b = (a + step).min(n);
            let lo = a.saturating_sub(r) / down * down;
            let hi = (b + r).min(n);
            let part = c.run(&x[lo..hi]);
            let (first, last) = (a * up / down, c.len(b));
            got.extend_from_slice(&part[first - lo * up / down..last - lo * up / down]);
        }
        assert_eq!(got, whole, "{from} to {to}");
    }
}
