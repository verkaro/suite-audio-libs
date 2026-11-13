//! `docs/spec-deess.md`, "Pinned by tests".

use super::*;
use crate::rigs::{self, level, same};
use as_envelope::measure::tone;
use as_envelope::signal::{Rng, sine};

const RATES: [u32; 3] = [44_100, 48_000, 96_000];

impl rigs::Unit for DeEsser {
    fn process(&mut self, x: &mut [f64]) {
        DeEsser::process(self, x);
    }
    fn process2(&mut self, l: &mut [f64], r: &mut [f64]) {
        DeEsser::process2(self, l, r);
    }
    fn gr_db(&self) -> f64 {
        self.readouts().gr_db
    }
}

fn with(pairs: &[(&str, f64)]) -> Settings {
    let mut s = Settings { on: true, ..Settings::default() };
    for &(id, v) in pairs {
        assert!(s.set_by_id(id, v), "{id}");
    }
    s
}

fn unit(rate: u32, s: &Settings) -> DeEsser {
    let mut u = DeEsser::new(rate as f64);
    u.set(s);
    u
}

fn through(rate: u32, s: &Settings, x: &[f64]) -> Vec<f64> {
    let mut u = unit(rate, s);
    let mut y = x.to_vec();
    y.chunks_mut(1024).for_each(|b| u.process(b));
    y
}

/// Mean square, sine-referenced, in dB.
fn ms_db(x: &[f64]) -> f64 {
    10.0 * libm::log10(2.0 * x.iter().map(|v| v * v).sum::<f64>() / x.len() as f64)
}

/// The band-pass at 6 kHz, from rest: the band the detector hears.
fn band_of(rate: u32, x: &[f64]) -> Vec<f64> {
    let f = Svf::new(6000.0, rate as f64);
    let mut s = [0.0; 2];
    x.iter().map(|&v| f.run(&mut s, v)).collect()
}

/// The ess: noise in exactly the octave about 6 kHz (flat, random phases, nothing outside it),
/// `n` samples, at `db` dBFS sine-referenced.
fn ess(rate: u32, n: usize, db: f64) -> Vec<f64> {
    let mut planner = realfft::RealFftPlanner::<f64>::new();
    let inv = planner.plan_fft_inverse(n);
    let mut spec = inv.make_input_vec();
    let mut rng = Rng::new(17);
    let (lo, hi) = (6000.0 / 2f64.sqrt(), 6000.0 * 2f64.sqrt());
    for (k, c) in spec.iter_mut().enumerate() {
        let f = k as f64 * rate as f64 / n as f64;
        let ph = std::f64::consts::PI * rng.uniform();
        if f >= lo && f <= hi {
            *c = realfft::num_complex::Complex::new(libm::cos(ph), libm::sin(ph));
        }
    }
    let mut x = inv.make_output_vec();
    inv.process(&mut spec, &mut x).unwrap();
    let k = level(db - ms_db(&x));
    x.iter().map(|v| v * k).collect()
}

/// The voice under it: 220 Hz and its harmonics to 2 kHz, falling as 1/h, at `db` dBFS
/// sine-referenced: the prototype's, whose readings `docs/spec-deess.md` gives (the pair's `ess`
/// reads −9.1 dB, truly −9.7, here as there). Alone, the detectors read its `ess` near −19 dB:
/// over the default threshold (the spec's "As built").
fn voice(rate: u32, n: usize, db: f64) -> Vec<f64> {
    let mut x = vec![0.0; n];
    for h in 1..=9 {
        for (v, s) in x.iter_mut().zip(sine(220.0 * h as f64, 1.0 / h as f64, n, rate)) {
            *v += s;
        }
    }
    let k = level(db - ms_db(&x));
    x.iter().map(|v| v * k).collect()
}

/// The RFC's pair: the ess at −20 dBFS over the voice at −12, `shift` dB on both.
fn pair(rate: u32, n: usize, shift: f64) -> Vec<f64> {
    let (e, v) = (ess(rate, n, -20.0 + shift), voice(rate, n, -12.0 + shift));
    e.iter().zip(&v).map(|(a, b)| a + b).collect()
}

/// The band's power out against in over the last second, in dB.
fn band_drop(rate: u32, x: &[f64], y: &[f64]) -> f64 {
    let last = x.len() - rate as usize;
    ms_db(&band_of(rate, y)[last..]) - ms_db(&band_of(rate, x)[last..])
}

#[test]
fn the_controls_pass_the_check() {
    as_control::check(controls()).unwrap();
    let mut s = Settings::default();
    for c in controls() {
        assert_eq!(s.get(c.id), Some(c.default), "{}", c.id);
        assert!(s.set_by_id(c.id, c.default));
    }
    assert!(!s.on, "out by default");
    assert!(!s.set_by_id("nope", 1.0));
    s.set_by_id("freq", 20000.0);
    s.set_by_id("range", -3.0);
    s.set_by_id("listen", 7.0);
    assert_eq!((s.freq, s.range, s.listen), (10000.0, 0.0, Listen::Taken));
    let t = Settings { thr: -90.0, ..s };
    assert_eq!(t.held().thr, -40.0);
}

#[test]
fn split_takes_the_band_and_leaves_the_voice() {
    let rate = 48_000;
    let s = with(&[("thr", -40.0), ("range", 6.0)]);
    let x = pair(rate, 2 * rate as usize, 0.0);
    let mut u = unit(rate, &s);
    let mut y = x.clone();
    y.chunks_mut(1024).for_each(|b| u.process(b));
    let gr = u.readouts().gr_db;
    assert!((gr - 6.0).abs() < 0.05, "{gr}");
    let g = gain(gr);

    // The band's power falls by what the bell takes from the band's spectrum.
    let last = x.len() - rate as usize;
    let b = &band_of(rate, &x)[last..];
    let spec = as_envelope::measure::spectrum(b);
    let (mut num, mut den) = (0.0, 0.0);
    for (k, m) in spec.iter().enumerate() {
        let f = k as f64 * rate as f64 / b.len() as f64;
        let r = level(response(&s, rate as f64, f, g));
        (num, den) = (num + m * m * r * r, den + m * m);
    }
    let want = 10.0 * libm::log10(num / den);
    let got = band_drop(rate, &x, &y);
    assert!((got - want).abs() < 0.3, "the band fell {got:.2} dB against {want:.2}");

    // Each harmonic moves by the bell at its frequency.
    for h in 1..=9 {
        let f = 220.0 * h as f64;
        let moved = 20.0
            * libm::log10(
                tone(&y, f, rate, last, rate as usize).0 / tone(&x, f, rate, last, rate as usize).0,
            );
        let want = response(&s, rate as f64, f, g);
        assert!((moved - want).abs() < 0.05, "{f} Hz moved {moved:.3} against {want:.3}");
        if f <= 1000.0 {
            assert!(moved.abs() < 0.1, "{f} Hz moved {moved:.3}");
        }
    }
    eprintln!("split: the band {got:.2} dB (bell predicts {want:.2})");
}

#[test]
fn wide_takes_the_whole() {
    let rate = 48_000;
    let s = with(&[("thr", -40.0), ("range", 6.0), ("mode", 1.0)]);
    let x = pair(rate, 2 * rate as usize, 0.0);
    let y = through(rate, &s, &x);
    let last = x.len() - rate as usize;
    let fell = ms_db(&y[last..]) - ms_db(&x[last..]);
    assert!((fell + 6.0).abs() < 0.3, "{fell}");
}

#[test]
fn the_voice_alone_is_left_alone() {
    let rate = 48_000;
    // At −16: the voice's `ess` reads −18.5 to −19 dB, under the knee's foot at −19 (at the
    // default −20 it's over, and takes up to 1.5 dB: the spec's "As built").
    let x = voice(rate, 2 * rate as usize, -12.0);
    let y = through(rate, &with(&[("thr", -16.0)]), &x);
    let last = x.len() - rate as usize;
    for h in 1..=9 {
        let f = 220.0 * h as f64;
        let moved = 20.0
            * libm::log10(
                tone(&y, f, rate, last, rate as usize).0 / tone(&x, f, rate, last, rate as usize).0,
            );
        assert!(moved.abs() < 0.01, "{f} Hz moved {moved}");
    }
}

#[test]
fn the_threshold_is_relative() {
    let rate = 48_000;
    let s = with(&[("thr", -14.0), ("range", 12.0)]);
    let n = 2 * rate as usize;
    let (loud, quiet) = (pair(rate, n, 0.0), pair(rate, n, -20.0));
    let a = band_drop(rate, &loud, &through(rate, &s, &loud));
    let b = band_drop(rate, &quiet, &through(rate, &s, &quiet));
    assert!(a < -1.0, "it acted: {a}");
    assert!((a - b).abs() < 0.3, "{a:.2} loud, {b:.2} 20 dB down");
}

#[test]
fn at_rest_is_the_input_to_the_bit() {
    let rate = 48_000;
    let n = rate as usize;
    // Split, the voice alone, a threshold no ess reaches.
    let x = voice(rate, n, -12.0);
    assert!(same(&x, &through(rate, &with(&[("thr", 0.0)]), &x)));
    // Out, with the pair far over.
    let x = pair(rate, n, 0.0);
    let s = Settings { thr: -40.0, ..Settings::default() };
    assert!(same(&x, &through(rate, &s, &x)));
    // The floor: the pair 60 dB down.
    let x = pair(rate, n, -60.0);
    assert!(same(&x, &through(rate, &with(&[("thr", -40.0)]), &x)));
}

#[test]
fn no_lisp_at_the_centre() {
    let rate = 48_000;
    let x = sine(6000.0, 1.0, rate as usize, rate);
    let mut u = unit(rate, &with(&[("thr", -40.0), ("range", 6.0)]));
    let mut y = x.clone();
    let mut most = 0.0f64;
    for b in y.chunks_mut(1) {
        u.process(b);
        most = most.max(u.readouts().gr_db);
    }
    assert!(most <= 6.05, "{most}");
    let at = 20.0 * libm::log10(tone(&y, 6000.0, rate, rate as usize / 2, rate as usize / 2).0);
    assert!(at >= -6.05, "{at}");
}

#[test]
fn the_bell_is_the_bell_drawn() {
    for rate in RATES {
        let r = rate as f64;
        for depth in [3.0, 6.0, 12.0] {
            let s = with(&[]);
            let g = level(-depth);
            for i in 0..=24 {
                let f = 5.0 * libm::round(200.0 * libm::pow(100.0, i as f64 / 24.0) / 5.0);
                let mut u = unit(rate, &s);
                u.hold = Some(g);
                let (settle, len) = (rate as usize / 4, rate as usize / 5);
                let mut y = sine(f, 0.5, settle + len, rate);
                u.process(&mut y);
                let got = 20.0 * libm::log10(tone(&y, f, rate, settle, len).0 / 0.5);
                let want = response(&s, r, f, g);
                assert!((got - want).abs() < 0.1, "{rate} {depth} dB at {f} Hz: {got:.3} against {want:.3}");
            }
            assert_eq!(response(&s, r, 6000.0, 1.0), 0.0);
            assert!((response(&s, r, 6000.0, g) + depth).abs() < 1e-9);
        }
    }
}

#[test]
fn listen_hears_the_band_and_what_is_taken() {
    let rate = 48_000;
    let x = pair(rate, rate as usize, 0.0);
    let base = with(&[("thr", -30.0)]);
    let band = through(rate, &Settings { listen: Listen::Band, ..base }, &x);
    let want = band_of(rate, &x);
    assert!(band.iter().zip(&want).all(|(a, b)| (a - b).abs() < 1e-12));
    let taken = through(rate, &Settings { listen: Listen::Taken, ..base }, &x);
    let y = through(rate, &base, &x);
    assert!(taken.iter().zip(x.iter().zip(&y)).all(|(t, (a, b))| (t - (a - b)).abs() < 1e-12));
    // And after a crossfade to it, too.
    let mut u = unit(rate, &base);
    let mut z = x.clone();
    u.process(&mut z[..4800]);
    u.set(&Settings { listen: Listen::Band, ..base });
    u.process(&mut z[4800..]);
    assert!(z[4800 + 480..].iter().zip(&want[4800 + 480..]).all(|(a, b)| (a - b).abs() < 1e-12));
}

#[test]
fn the_readout_reads_the_band_over_the_whole() {
    let rate = 48_000;
    let x = pair(rate, 2 * rate as usize, 0.0);
    let last = x.len() - rate as usize;
    let truth = ms_db(&band_of(rate, &x)[last..]) - ms_db(&x[last..]);
    let mut u = unit(rate, &with(&[]));
    let mut y = x.clone();
    let mut reads = vec![];
    for (i, b) in y.chunks_mut(64).enumerate() {
        u.process(b);
        if i * 64 >= last {
            reads.push(u.readouts().ess_db);
        }
    }
    let mean = reads.iter().sum::<f64>() / reads.len() as f64;
    assert!((mean - truth).abs() < 1.0, "reads {mean:.2} against {truth:.2}");
    eprintln!("ess reads {mean:.2} dB, truly {truth:.2}");
}

#[test]
fn the_timing() {
    let rate = 48_000;
    let (on, off, n) = (rate as usize / 2, rate as usize * 3 / 2, rate as usize * 3);
    let p = pair(rate, n, 0.0);
    let x: Vec<f64> = p.iter().enumerate().map(|(k, v)| if k >= on && k < off { *v } else { 0.0 }).collect();
    let run = |s: &Settings| {
        let mut u = unit(rate, s);
        let (mut ms, mut gr) = (vec![], vec![]);
        for &v in &x {
            u.process(&mut [v]);
            ms.push(u.band.ms());
            gr.push(u.readouts().gr_db);
        }
        (ms, gr)
    };
    let (ms, gr) = run(&with(&[("thr", -40.0)]));
    // The band's detector, on a step in its power: 63% in 0.5 ms, within 5%.
    let mut d = unit(rate, &with(&[])).band;
    let k = (0..).find(|_| {
        d.feed(1.0);
        d.ms() >= 1.0 - libm::exp(-1.0)
    });
    let t = (k.unwrap() + 1) as f64 / rate as f64;
    assert!((t / BAND_ATTACK - 1.0).abs() < 0.05, "{t}");
    // On the pair it rides the band's crests, settling about 7 dB over its true power, so 63% of
    // its own settled state isn't its time; it reaches 63% of the band's true power within one
    // attack time.
    let b = band_of(rate, &x);
    let truth = b[on + 4800..off].iter().map(|v| v * v).sum::<f64>() / (off - on - 4800) as f64;
    let k = (on..off).find(|&k| ms[k] >= truth * (1.0 - libm::exp(-1.0))).unwrap();
    let t = (k + 1 - on) as f64 / rate as f64;
    eprintln!("the band's detector: 63% of the band's power in {:.3} ms", t * 1e3);
    assert!(t <= BAND_ATTACK, "{t}");
    // Fully set within five attack times.
    let set = (on..off).find(|&k| gr[k] == 6.0).unwrap();
    assert!((set - on) as f64 <= 5.0 * BAND_ATTACK * rate as f64, "set in {} samples", set - on);
    // Fully released within five release times, at a threshold the ess is a little over.
    let (_, gr) = run(&with(&[("thr", -14.0), ("range", 12.0)]));
    let free = (off..n).find(|&k| gr[k..].iter().all(|&g| g == 0.0)).unwrap();
    let t = (free - off) as f64 / rate as f64;
    eprintln!("released in {:.1} ms", t * 1e3);
    assert!(t <= 5.0 * BAND_RELEASE, "{t}");
}

#[test]
fn a_held_key_glides() {
    for rate in RATES {
        let x = pair(rate, rigs::held_key_len(rate), 0.0);
        let base = with(&[("thr", -30.0)]);
        let grid = |id: &str| rigs::grid(controls(), id);
        for (id, path) in [
            ("freq", grid("freq")),
            ("thr", grid("thr")),
            ("range", grid("range")),
            ("mode", vec![0.0, 1.0]),
            ("listen", vec![0.0, 1.0, 2.0, 1.0]),
        ] {
            let at = |v: f64| {
                let mut s = base;
                s.set_by_id(id, v);
                s
            };
            rigs::held_key_on(
                &x,
                12_000.0,
                || unit(rate, &base),
                |u, v| u.set(&at(v)),
                |u, v| {
                    assert!(u.settled(), "{id}: settled 50 ms after");
                    assert_eq!(u.settings(), at(v), "{id}: the last setting holds");
                },
                rate,
                &path,
                id,
            );
        }
    }
}

#[test]
fn the_same_bits_at_any_block_size() {
    for rate in RATES {
        let n = rate as usize;
        let l = pair(rate, n, 0.0);
        let r: Vec<f64> = pair(rate, n, -6.0).iter().rev().cloned().collect();
        let apply = |u: &mut DeEsser, at: usize| {
            if at.is_multiple_of(1024) {
                let i = at / 1024;
                u.set(&with(&[
                    ("thr", -40.0 + (i % 9) as f64 * 4.0),
                    ("freq", 3000.0 + (i % 5) as f64 * 1500.0),
                    ("mode", (i / 7 % 2) as f64),
                    ("listen", (i / 11 % 3) as f64),
                    ("in", if i % 13 == 6 { 0.0 } else { 1.0 }),
                ]));
            }
        };
        rigs::blocks(|| unit(rate, &with(&[])), apply, &l, &r);
    }
}

#[test]
fn every_output_is_finite() {
    for rate in RATES {
        rigs::finite(|| unit(rate, &with(&[("thr", -40.0), ("range", 12.0)])), rate);
    }
}

#[test]
fn one_detector_for_both_channels() {
    let x = pair(48_000, 48_000, 0.0);
    rigs::linked(|| unit(48_000, &with(&[("thr", -30.0)])), &x);
}

/// And the filters fallen to 0, the detectors with them.
#[test]
fn a_tail_to_silence_takes_no_longer() {
    let rate = 48_000;
    let mut u = unit(rate, &with(&[("thr", -30.0)]));
    rigs::tail(&mut u, &pair(rate, rate as usize, 0.0), rate);
    assert!(u.mem.iter().flatten().all(|&v| v == 0.0), "fallen to 0");
    assert_eq!((u.band.ms(), u.whole.ms()), (0.0, 0.0));
}
