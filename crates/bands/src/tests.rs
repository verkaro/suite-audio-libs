//! The layout, the power scale, the frame timing, and pieces against the whole.

use super::*;
use as_envelope::signal;

const RATES: [u32; 4] = [44_100, 48_000, 88_200, 96_000];

fn layout(rate: u32) -> Layout {
    Layout::erb(rate, 24, 50.0, 16_000.0).unwrap()
}

fn db(p: f64) -> f64 {
    10.0 * libm::log10(p.max(1e-30))
}

/// A band's mean power over the frames from `from` on.
fn mean_band(bands: &[f32], nb: usize, b: usize, from: usize) -> f64 {
    let frames: Vec<f64> = bands.chunks(nb).skip(from).map(|f| f[b] as f64).collect();
    frames.iter().sum::<f64>() / frames.len() as f64
}

#[test]
fn erb_number_and_its_inverse() {
    for hz in [50.0, 100.0, 1000.0, 16_000.0] {
        assert!((erb_hz(erb_number(hz)) - hz).abs() < 1e-9 * hz);
    }
    // Glasberg and Moore: 1 kHz is 15.6 ERB.
    assert!((erb_number(1000.0) - 15.6).abs() < 0.1);
}

#[test]
fn the_edges_rise_and_every_band_holds_a_bin_at_every_rate() {
    for rate in RATES {
        let l = layout(rate);
        assert_eq!((l.rate(), l.bands(), l.edges_hz().len()), (rate, 24, 25));
        assert_eq!(l.hop() * 100, rate as usize, "an exact 10 ms");
        assert_eq!(l.window(), if rate < 50_000 { 2048 } else { 4096 });
        assert!(l.edges_hz().windows(2).all(|e| e[0] < e[1]), "{rate}: edges rise");
        assert!((l.edges_hz()[0] - 50.0).abs() < 1e-9 && (l.edges_hz()[24] - 16_000.0).abs() < 1e-6);
        let mut next = l.bin_range(0).start;
        for b in 0..24 {
            let r = l.bin_range(b);
            assert!(!r.is_empty(), "{rate}: band {b} holds a bin");
            assert_eq!(r.start, next, "{rate}: bands are runs of bins, one after another");
            next = r.end;
            // The bins are in the band, and their neighbours are not.
            let f = |k: usize| k as f64 * rate as f64 / l.window() as f64;
            assert!(f(r.start) >= l.edges_hz()[b] && f(r.end - 1) < l.edges_hz()[b + 1]);
            assert!(f(r.end) >= l.edges_hz()[b + 1] && f(r.start - 1) < l.edges_hz()[b]);
        }
        let c = l.centres_hz();
        assert!(c.iter().enumerate().all(|(b, c)| *c > l.edges_hz()[b] && *c < l.edges_hz()[b + 1]));
        // Even spacing in ERB number.
        let steps: Vec<f64> = l.edges_hz().windows(2).map(|e| erb_number(e[1]) - erb_number(e[0])).collect();
        assert!(steps.iter().all(|s| (s - steps[0]).abs() < 1e-9));
    }
}

#[test]
fn other_rates_and_empty_bands_are_refused() {
    for rate in [0, 8000, 22_050, 32_000, 47_999, 176_400, 192_000] {
        assert!(Layout::erb(rate, 24, 50.0, 16_000.0).is_err(), "{rate}");
    }
    // Too many bands for the bins at the bottom.
    let e = Layout::erb(48_000, 200, 50.0, 16_000.0).unwrap_err();
    assert!(e.contains("holds no FFT bin"), "{e}");
    assert!(Layout::erb(48_000, 0, 50.0, 16_000.0).is_err());
    assert!(Layout::erb(48_000, 24, 16_000.0, 50.0).is_err());
    assert!(Layout::erb(48_000, 24, 0.0, 16_000.0).is_err());
    assert!(Layout::erb(48_000, 24, 50.0, 30_000.0).is_err(), "past Nyquist");
}

// The spec (11.2) says: a sine of amplitude 0.5 at a band's centre reads 0.125 in that band within
// 0.1 dB, and less than 1e-6 of it two bands away. Measured, that holds for the sine's own band
// from the third bin up, and for "two bands away" only from band 20 up: a Hann window's sidelobes
// reach −46 dB (2.5e-5) at the distance of two of the narrow low bands, and band 0 loses part of
// its leakage to the bins below 50 Hz, which are dropped. These tests state what does hold. The
// layout and window are the spec's (8.1); Warren settled the tests on these figures, 2026-11-02.

#[test]
fn a_sine_reads_its_power_in_its_band() {
    for rate in RATES {
        let l = layout(rate);
        for (b, c) in l.centres_hz().iter().enumerate() {
            let x = signal::sine(*c, 0.5, rate as usize * 2, rate);
            let p = analyze(&x, &l);
            // Skip the frames that include the start.
            let read = |band: usize| mean_band(&p, 24, band, 20);
            let err = db(read(b)) - db(0.125);
            // A band of three bins or more holds the leakage: within 0.1 dB.
            if l.bin_range(b).len() >= 3 {
                assert!(err.abs() < 0.1, "{rate} band {b}: {err} dB");
            } else {
                assert!(err < 0.0 && err > -1.0, "{rate} band {b}: {err} dB");
            }
            // Two bands away: at least 40 dB down everywhere, and 60 dB down in the wide bands.
            for far in [b.wrapping_sub(2), b + 2] {
                if far < 24 {
                    let rel = read(far) / 0.125;
                    assert!(rel < 1e-4, "{rate} band {b} → {far}: {rel}");
                    if b >= 20 {
                        assert!(rel < 1e-6, "{rate} band {b} → {far}: {rel}");
                    }
                }
            }
        }
    }
}

#[test]
fn the_bands_together_hold_a_sines_power() {
    // Where the leakage lies inside 50 Hz–16 kHz, the power is conserved across the bands; only
    // band 0's sine loses some to the dropped bins below 50 Hz.
    for rate in [48_000, 96_000] {
        let l = layout(rate);
        for (b, c) in l.centres_hz().iter().enumerate() {
            let x = signal::sine(*c, 0.5, rate as usize * 2, rate);
            let p = analyze(&x, &l);
            let total: f64 = (0..24).map(|j| mean_band(&p, 24, j, 20)).sum();
            let err = db(total) - db(0.125);
            if b == 0 {
                assert!(err < 0.0 && err > -1.0, "{rate} band 0: {err} dB");
            } else {
                assert!(err.abs() < 0.01, "{rate} band {b}: {err} dB");
            }
        }
    }
}

#[test]
fn white_noise_reads_its_power_times_the_bands_share() {
    for rate in [48_000u32, 96_000] {
        let l = layout(rate);
        let x = signal::noise(rate as usize * 10, 0.5, 11);
        let var = x.iter().map(|v| v * v).sum::<f64>() / x.len() as f64;
        let p = analyze(&x, &l);
        let in_bins: usize = (0..24).map(|b| l.bin_range(b).len()).sum();
        let want = var * in_bins as f64 / (l.window() / 2) as f64;
        let total: f64 = (0..24).map(|b| mean_band(&p, 24, b, 0)).sum();
        assert!((total / want - 1.0).abs() < 0.02, "{rate}: {total} vs {want}");
        // Each band alone, too: its share is its own bins'.
        for b in 0..24 {
            let want = var * l.bin_range(b).len() as f64 / (l.window() / 2) as f64;
            let got = mean_band(&p, 24, b, 0);
            assert!((got / want - 1.0).abs() < 0.1, "{rate} band {b}: {got} vs {want}");
        }
    }
}

#[test]
fn pieces_of_any_size_give_the_same_bits_as_the_whole() {
    for rate in [48_000u32, 96_000] {
        let l = layout(rate);
        let n = if rate == 48_000 { 100_001 } else { 60_007 };
        let x: Vec<f64> = signal::noise(n, 0.5, 5)
            .iter()
            .zip(signal::sine(300.0, 0.3, n, rate))
            .map(|(a, b)| a + b)
            .collect();
        let whole = analyze(&x, &l);
        assert_eq!(whole.len() as u64, l.frames(n as u64) * 24);
        for piece in [1usize, 479, 480, 4096, 65_536] {
            // One sample at a time over the whole sound is slow in debug; the start and the end
            // are what a piece boundary can break, so the sound is cut shorter for that size.
            let (x, whole) = if piece == 1 {
                let m = 9_001;
                (&x[..m], analyze(&x[..m], &l))
            } else {
                (&x[..], whole.clone())
            };
            let mut out = Vec::new();
            let mut a = Analyzer::new(&l);
            for c in x.chunks(piece) {
                a.push(c, &mut out);
            }
            a.finish(&mut out);
            assert!(out == whole, "{rate}: pieces of {piece} differ");
        }
    }
}

#[test]
fn frame_count_and_timing() {
    let l = layout(48_000);
    let h = l.hop();
    assert_eq!(h, 480);
    assert_eq!(l.frames(0), 0);
    assert_eq!(l.frames(1), 1);
    assert_eq!(l.frames(480), 1);
    assert_eq!(l.frames(481), 2);
    for len in [0usize, 1, 479, 480, 481, 5000, 48_000, 48_001] {
        assert_eq!(analyze(&vec![0.1; len], &l).len() as u64, l.frames(len as u64) * 24, "len {len}");
    }
    // An impulse at sample k·hop peaks in frame k (the first, a middle one, the last).
    let len = 30 * h + 7; // 31 frames
    for k in [0usize, 1, 2, 15, 29, 30] {
        let x = signal::impulse(len, k * h, 1.0);
        let p = analyze(&x, &l);
        let frame_power: Vec<f64> = p.chunks(24).map(|f| f.iter().map(|v| *v as f64).sum()).collect();
        assert_eq!(frame_power.len(), 31);
        let peak =
            frame_power.iter().enumerate().fold(0, |m, (i, v)| if *v > frame_power[m] { i } else { m });
        assert_eq!(peak, k, "impulse at {}", k * h);
        // And the frames either side are lower, by the window: Hann at ±hop from its centre.
        if k > 0 && k < 30 {
            let w =
                |m: f64| (0.5 - 0.5 * libm::cos(2.0 * std::f64::consts::PI * (1024.0 + m) / 2048.0)).powi(2);
            let ratio = frame_power[k + 1] / frame_power[k];
            assert!((ratio / w(480.0) - 1.0).abs() < 1e-3, "{ratio} vs {}", w(480.0));
        }
    }
}

#[test]
fn silence_is_zero_and_a_louder_sound_is_louder() {
    let l = layout(44_100);
    assert!(analyze(&vec![0.0; 10_000], &l).iter().all(|v| *v == 0.0));
    let (quiet, loud) = (signal::noise(44_100, 0.1, 3), signal::noise(44_100, 0.2, 3));
    let (q, p) = (analyze(&quiet, &l), analyze(&loud, &l));
    for (a, b) in q.iter().zip(&p).skip(24 * 10).take(240) {
        assert!((*b as f64 / *a as f64 - 4.0).abs() < 1e-4);
    }
}
