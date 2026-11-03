//! The synthetic corpus (Tape's RFC, section 10): sounds like a drama's stems, made from code,
//! the same every time, at any rate. Speech-like voices are pulse trains (a glottis) through
//! formant resonances (a vocal tract), moving between vowels in syllables; sibilants are
//! fricative noise; plosives are short bursts. Levels run from whispered to shouted.

use crate::signal::{Rng, noise, sine};
use as_biquad::Biquad;
use std::f64::consts::TAU;

/// One item: a name, its channels, at its rate.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub name: &'static str,
    pub rate: u32,
    pub channels: Vec<Vec<f64>>,
}

/// Formants of a few vowels: (Hz, Q) for F1–F3.
const VOWELS: [[(f64, f64); 3]; 4] = [
    [(730.0, 8.0), (1090.0, 10.0), (2440.0, 12.0)], // a
    [(270.0, 6.0), (2290.0, 12.0), (3010.0, 14.0)], // i
    [(300.0, 6.0), (870.0, 8.0), (2240.0, 12.0)],   // u
    [(530.0, 7.0), (1840.0, 10.0), (2480.0, 12.0)], // e
];

/// A voice: a pulse train at `pitch` (with a slow drift), through each vowel in turn, a
/// syllable each `syllable` s, its level shaped up and down; `amp` sets the level.
pub fn voice(pitch: f64, amp: f64, secs: f64, syllable: f64, rate: u32, seed: u64) -> Vec<f64> {
    let n = (secs * rate as f64) as usize;
    let per = (syllable * rate as f64) as usize;
    // Harmonics up to 12 kHz, or nine tenths of Nyquist if that's lower.
    let top = f64::min(12_000.0, 0.45 * rate as f64);
    let mut r = Rng::new(seed);
    let mut phase = 0.0;
    // The glottal source: harmonics falling at 12 dB an octave, band-limited, its pitch
    // gliding by a few percent across each syllable.
    let source: Vec<f64> = (0..n)
        .map(|k| {
            let t = k as f64 / rate as f64;
            let f = pitch * (1.0 + 0.04 * libm::sin(TAU * t / (2.0 * syllable)));
            phase += f / rate as f64;
            // sin(hθ) for every h by the recurrence s_h = 2 cos θ s_(h−1) − s_(h−2): one sin
            // and one cos a sample, the rest multiplies and adds.
            let theta = TAU * phase;
            let (s1, c) = (libm::sin(theta), libm::cos(theta));
            let (mut prev, mut cur) = (0.0, s1);
            let mut v = 0.0;
            let mut h = 1.0;
            while h * f < top {
                v += cur / (h * h);
                (prev, cur) = (cur, 2.0 * c * cur - prev);
                h += 1.0;
            }
            v
        })
        .collect();
    let mut out = vec![0.0; n];
    for (s, start) in (0..n).step_by(per).enumerate() {
        let end = (start + per).min(n);
        let vowel = VOWELS[(s + (r.next_u64() % 4) as usize) % 4];
        let mut seg: Vec<f64> = source[start..end].to_vec();
        let mut acc = vec![0.0; seg.len()];
        for (i, &(hz, q)) in vowel.iter().enumerate() {
            let mut f = seg.clone();
            Biquad::peak(hz, q, 24.0 - 6.0 * i as f64, rate as f64).run(&mut f);
            for (a, v) in acc.iter_mut().zip(&f) {
                *a += v;
            }
        }
        seg.copy_from_slice(&acc);
        // A syllable's shape: up quickly, down more slowly, a gap at its end.
        let len = seg.len() as f64;
        for (k, v) in seg.iter_mut().enumerate() {
            let u = k as f64 / len;
            let shape = if u < 0.1 {
                u / 0.1
            } else if u < 0.75 {
                1.0 - 0.4 * (u - 0.1)
            } else {
                (0.74 * (1.0 - u) / 0.25).max(0.0)
            };
            *v *= shape;
        }
        out[start..end].copy_from_slice(&seg);
    }
    let peak = crate::measure::peak(&out);
    out.iter().map(|v| v / peak * amp).collect()
}

/// A voice with the lip radiation speech has: [`voice`] through a first-order high shelf
/// (zero at 250 Hz, +30 dB), then scaled to peak `amp`. Against 1 kHz its third-octave levels
/// read about +15.5 dB at 250 Hz, +9.8 at 500, −3.5 at 2 kHz, −9.0 at 4, −12.0 at 6.3 and
/// −13.9 at 8, near the long-term spectrum of real speech (a little heavy below 300 Hz). Added
/// 2026-11-02, for ducky; `voice` itself is unchanged, because Tape's tests pin it.
pub fn speech(pitch: f64, amp: f64, secs: f64, syllable: f64, rate: u32, seed: u64) -> Vec<f64> {
    let mut x = voice(pitch, amp, secs, syllable, rate, seed);
    as_biquad::First::shelf(250.0, 30.0, rate as f64).run(&mut x);
    let peak = crate::measure::peak(&x);
    x.iter().map(|v| v / peak * amp).collect()
}

/// [`speech`] gated into phrases: `on` s of voice, then `off` s of zeros, again and again,
/// starting with voice. A phrase starts every `on + off` s (to the sample), so with `on` 2.5 and
/// `off` 0.8, phrase `k` starts at `k · 3.3` s. The gate is hard, and the voice is the speech
/// as it would have been, so no phrase is scaled to `amp` on its own.
pub fn phrases(pitch: f64, amp: f64, secs: f64, on: f64, off: f64, rate: u32, seed: u64) -> Vec<f64> {
    let mut x = speech(pitch, amp, secs, 0.22, rate, seed);
    let (on_n, period) = ((on * rate as f64).round() as usize, ((on + off) * rate as f64).round() as usize);
    for (i, v) in x.iter_mut().enumerate() {
        if i % period >= on_n {
            *v = 0.0;
        }
    }
    x
}

/// Fricative noise: bursts of `burst` s, every `every` s, band-passed high (4–10 kHz).
pub fn sibilants(amp: f64, secs: f64, burst: f64, every: f64, rate: u32, seed: u64) -> Vec<f64> {
    let n = (secs * rate as f64) as usize;
    let mut x = noise(n, 1.0, seed);
    // An [s]: most of its energy 5–9 kHz, rising steeply from 3.5 kHz and falling steeply past
    // 10 kHz (two second-order sections each way), as measured fricatives do.
    for _ in 0..2 {
        as_biquad::First::highpass(3500.0, rate as f64).run(&mut x);
        as_biquad::First::highpass(3500.0, rate as f64).run(&mut x);
        // The fall past 10 kHz only where there's room above it.
        if rate as f64 / 2.0 > 12_000.0 {
            Biquad::lowpass_matched(10_000.0 / 0.6436, 0.5, 10_000.0, rate as f64).run(&mut x);
        }
    }
    Biquad::peak(7000.0, 1.2, 4.0, rate as f64).run(&mut x);
    let (b, e) = ((burst * rate as f64) as usize, (every * rate as f64) as usize);
    for (k, v) in x.iter_mut().enumerate() {
        let p = k % e;
        let g = if p < b { libm::sin(std::f64::consts::PI * p as f64 / b as f64) } else { 0.0 };
        *v *= g;
    }
    let peak = crate::measure::peak(&x);
    x.iter().map(|v| v / peak * amp).collect()
}

/// Plosives: a click and a short low burst, every `every` s.
pub fn plosives(amp: f64, secs: f64, every: f64, rate: u32, seed: u64) -> Vec<f64> {
    let n = (secs * rate as f64) as usize;
    let e = (every * rate as f64) as usize;
    let mut r = Rng::new(seed);
    let mut x = vec![0.0; n];
    for start in (e / 2..n).step_by(e) {
        for k in 0..(0.04 * rate as f64) as usize {
            if start + k >= n {
                break;
            }
            let t = k as f64 / rate as f64;
            let env = libm::exp(-t / 0.008);
            x[start + k] = env * (0.7 * libm::sin(TAU * 120.0 * t) + 0.3 * r.uniform());
        }
    }
    x.iter().map(|v| v * amp).collect()
}

/// A whisper: noise through a vowel's formants, falling past 4 kHz as breath does, in
/// syllables; `amp` its peak.
pub fn whisper(amp: f64, secs: f64, rate: u32, seed: u64) -> Vec<f64> {
    let n = (secs * rate as f64) as usize;
    let w = noise(n, 1.0, seed);
    let mut acc = vec![0.0; n];
    for (i, &(hz, q)) in VOWELS[0].iter().enumerate() {
        let mut f = w.clone();
        Biquad::peak(hz, q / 2.0, 20.0 - 4.0 * i as f64, rate as f64).run(&mut f);
        for (a, v) in acc.iter_mut().zip(&f) {
            *a += v;
        }
    }
    Biquad::lowpass_matched(4000.0, 0.5, 2500.0, rate as f64).run(&mut acc);
    let per = (0.35 * rate as f64) as usize;
    for (k, v) in acc.iter_mut().enumerate() {
        *v *= libm::sin(std::f64::consts::PI * (k % per) as f64 / per as f64);
    }
    let peak = crate::measure::peak(&acc);
    acc.iter().map(|v| v / peak * amp).collect()
}

/// An LF-heavy impact (a door slam): a decaying thud at 60 Hz and its noise.
pub fn impacts(amp: f64, secs: f64, rate: u32, seed: u64) -> Vec<f64> {
    let n = (secs * rate as f64) as usize;
    let mut r = Rng::new(seed);
    let mut x = vec![0.0; n];
    for start in [n / 8, n / 2] {
        for k in 0..(0.6 * rate as f64) as usize {
            if start + k >= n {
                break;
            }
            let t = k as f64 / rate as f64;
            x[start + k] =
                libm::exp(-t / 0.12) * libm::sin(TAU * 60.0 * t) + 0.4 * libm::exp(-t / 0.02) * r.uniform();
        }
    }
    let peak = crate::measure::peak(&x);
    x.iter().map(|v| v / peak * amp).collect()
}

/// A log sweep from 20 Hz to 20 kHz (or 0.45 of the rate), at `amp`, faded in over two
/// cycles of its first frequency (100 ms) and out over 10 ms.
pub fn sweep(amp: f64, secs: f64, rate: u32) -> Vec<f64> {
    let n = (secs * rate as f64) as usize;
    let (f1, f2) = (20.0, f64::min(20_000.0, 0.45 * rate as f64));
    let l = secs / libm::log(f2 / f1);
    let mut x: Vec<f64> = (0..n)
        .map(|k| amp * libm::sin(TAU * f1 * l * (libm::exp(k as f64 / rate as f64 / l) - 1.0)))
        .collect();
    let (fade_in, fade_out) = ((0.1 * rate as f64) as usize, (0.01 * rate as f64) as usize);
    let ramp = |k: usize, of: usize| 0.5 - 0.5 * libm::cos(std::f64::consts::PI * k as f64 / of as f64);
    for (k, v) in x.iter_mut().enumerate().take(fade_in) {
        *v *= ramp(k, fade_in);
    }
    for (k, v) in x.iter_mut().rev().enumerate().take(fade_out) {
        *v *= ramp(k, fade_out);
    }
    x
}

/// A bed: a slow chord, stereo, its two sides a little apart.
pub fn bed(amp: f64, secs: f64, rate: u32) -> Vec<Vec<f64>> {
    let n = (secs * rate as f64) as usize;
    let side = |detune: f64| -> Vec<f64> {
        let chord = [110.0, 164.81, 220.0, 277.18, 329.63];
        let mut x = vec![0.0; n];
        for (i, f) in chord.iter().enumerate() {
            for (k, v) in sine(f * detune, 1.0 / (1 + i) as f64, n, rate).into_iter().enumerate() {
                x[k] += v;
            }
        }
        let peak = crate::measure::peak(&x);
        crate::signal::faded(x.iter().map(|v| v / peak * amp).collect(), rate as usize / 10)
    };
    vec![side(1.0), side(1.003)]
}

/// Room tone: pink-ish noise, quiet.
pub fn room_tone(amp: f64, secs: f64, rate: u32, seed: u64) -> Vec<f64> {
    let n = (secs * rate as f64) as usize;
    let w = noise(n, 1.0, seed);
    // Paul Kellet's economy pink filter.
    let (mut b0, mut b1, mut b2) = (0.0, 0.0, 0.0);
    let x: Vec<f64> = w
        .iter()
        .map(|&v| {
            b0 = 0.99765 * b0 + v * 0.0990460;
            b1 = 0.96300 * b1 + v * 0.2965164;
            b2 = 0.57000 * b2 + v * 1.0526913;
            b0 + b1 + b2 + v * 0.1848
        })
        .collect();
    let peak = crate::measure::peak(&x);
    x.iter().map(|v| v / peak * amp).collect()
}

/// The whole corpus at `rate`, `secs` an item: names as the RFC lists them.
pub fn all(rate: u32, secs: f64) -> Vec<Item> {
    let mono = |name, x| Item { name, rate, channels: vec![x] };
    let one = voice(140.0, 0.25, secs, 0.32, rate, 1);
    let lowpass = |mut x: Vec<f64>| {
        Biquad::lowpass_matched(9000.0, 0.7, 6000.0, rate as f64).run(&mut x);
        x
    };
    let presence = |mut x: Vec<f64>| {
        Biquad::peak(4000.0, 0.8, 5.0, rate as f64).run(&mut x);
        x.iter().map(|v| v * 1.6).collect()
    };
    let shout: Vec<f64> = {
        let v = voice(210.0, 0.7, secs, 0.4, rate, 4);
        let s = sibilants(0.35, secs, 0.12, 0.8, rate, 5);
        v.iter().zip(&s).map(|(a, b)| a + b).collect()
    };
    vec![
        mono("voice, low, conversational", voice(95.0, 0.2, secs, 0.3, rate, 2)),
        mono("voice, high, projected", voice(260.0, 0.45, secs, 0.28, rate, 3)),
        mono("voice, shouted, with esses", shout),
        mono("whisper", whisper(0.03, secs, rate, 6)),
        mono("sibilants", sibilants(0.3, secs, 0.15, 0.5, rate, 7)),
        mono("plosives", plosives(0.5, secs, 0.4, rate, 8)),
        mono("boom", lowpass(one.clone())),
        mono("lav", presence(one)),
        mono("sweep", sweep(0.3, secs, rate)),
        mono("impacts", impacts(0.8, secs, rate, 9)),
        Item { name: "bed (stereo)", rate, channels: bed(0.2, secs, rate) },
        mono("room tone", room_tone(0.001, secs, rate, 10)),
        mono("silence", vec![0.0; (secs * rate as f64) as usize]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Third-octave band level (dB) of the first 2^18 samples, as the review's `ltass2` does.
    fn third_octave_db(x: &[f64], centre: f64, rate: u32) -> f64 {
        let n = 1 << 18;
        let s = crate::measure::spectrum(&x[..n]);
        let (lo, hi) = (centre * libm::pow(2.0, -1.0 / 6.0), centre * libm::pow(2.0, 1.0 / 6.0));
        let p: f64 = s
            .iter()
            .enumerate()
            .filter(|(k, _)| {
                let f = *k as f64 * rate as f64 / n as f64;
                f >= lo && f < hi
            })
            .map(|(_, a)| a * a)
            .sum();
        10.0 * libm::log10(p)
    }

    #[test]
    fn speech_has_the_long_term_spectrum_of_speech() {
        let x = speech(130.0, 0.5, 8.0, 0.22, 48_000, 7);
        assert_eq!(x.len(), 8 * 48_000);
        assert!((crate::measure::peak(&x) - 0.5).abs() < 1e-12, "scaled to peak amp");
        let at_1k = third_octave_db(&x, 1000.0, 48_000);
        for (hz, want) in
            [(250.0, 15.5), (500.0, 9.8), (2000.0, -3.5), (4000.0, -9.0), (6300.0, -12.0), (8000.0, -13.9)]
        {
            let got = third_octave_db(&x, hz, 48_000) - at_1k;
            assert!((got - want).abs() <= 0.2, "{hz} Hz: {got:+.1} dB against {want:+.1}");
        }
        // The same every time, and `voice` is as it was.
        assert_eq!(x, speech(130.0, 0.5, 8.0, 0.22, 48_000, 7));
        assert_ne!(x, speech(130.0, 0.5, 8.0, 0.22, 48_000, 8));
    }

    #[test]
    fn phrases_are_speech_gated() {
        let rate = 48_000;
        let (on, off) = (2.5, 0.8);
        let x = phrases(130.0, 0.5, 33.0, on, off, rate, 7);
        let s = speech(130.0, 0.5, 33.0, 0.22, rate, 7);
        assert_eq!(x.len(), 33 * rate as usize);
        for (i, (a, b)) in x.iter().zip(&s).enumerate() {
            let t = i as f64 / rate as f64;
            let in_phrase = t - (t / 3.3).floor() * 3.3 < on - 1e-9;
            if in_phrase {
                assert_eq!(a, b, "sample {i}");
            } else if t - (t / 3.3).floor() * 3.3 > on + 1e-9 {
                assert_eq!(*a, 0.0, "sample {i}");
            }
        }
        // Ten phrases, each starting with voice, with zeros between.
        let loud = |from: f64, to: f64| {
            x[(from * rate as f64) as usize..(to * rate as f64) as usize].iter().any(|v| v.abs() > 0.01)
        };
        for k in 0..10 {
            let t0 = k as f64 * 3.3;
            assert!(loud(t0 + 0.05, t0 + 0.5), "phrase {k} starts with voice");
            assert!(!loud(t0 + on + 0.001, t0 + on + off - 0.001), "pause {k} is zeros");
        }
        assert!(crate::measure::peak(&x) <= 0.5 + 1e-12);
    }

    #[test]
    fn the_corpus_is_the_same_every_time_and_sane() {
        let a = all(48_000, 1.0);
        assert_eq!(a, all(48_000, 1.0));
        assert_eq!(a.len(), 13);
        for it in &a {
            for c in &it.channels {
                assert_eq!(c.len(), 48_000, "{}", it.name);
                assert!(c.iter().all(|v| v.is_finite() && v.abs() < 1.0), "{}", it.name);
            }
        }
        assert!(a.iter().find(|i| i.name == "silence").unwrap().channels[0].iter().all(|&v| v == 0.0));
        // At every rate.
        for rate in [44_100, 96_000, 192_000] {
            assert!(all(rate, 0.5).iter().all(|i| i.channels[0].len() == rate as usize / 2));
        }
    }
}
