//! `docs/spec-fet.md`, "Pinned by tests".

use super::*;
use crate::rigs::{self, level, same};
use as_envelope::corpus::{impacts, speech};
use as_envelope::measure::tone;
use as_envelope::measure::{thd, worst_product};
use as_envelope::signal::sine;

const RATES: [u32; 3] = [44_100, 48_000, 96_000];
const ALL: f64 = 4.0;

fn with(pairs: &[(&str, f64)]) -> Settings {
    let mut s = Settings { on: true, ..Settings::default() };
    for &(id, v) in pairs {
        assert!(s.set_by_id(id, v), "{id}");
    }
    s
}

fn unit(rate: u32, s: &Settings) -> Fet {
    let mut u = Fet::new(rate as f64);
    u.set(s);
    u
}

impl rigs::Unit for Fet {
    fn process(&mut self, x: &mut [f64]) {
        Fet::process(self, x);
    }
    fn process2(&mut self, l: &mut [f64], r: &mut [f64]) {
        Fet::process2(self, l, r);
    }
    fn gr_db(&self) -> f64 {
        self.readouts().gr_db
    }
}

/// The reduction at every sample of `x`.
fn rhos(rate: u32, s: &Settings, x: &[f64]) -> Vec<f64> {
    let mut u = unit(rate, s);
    x.iter()
        .map(|&v| {
            u.process(&mut [v]);
            u.readouts().gr_db
        })
        .collect()
}

#[test]
fn the_controls_and_the_dials() {
    as_control::check(controls()).unwrap();
    let mut s = Settings::default();
    for c in controls() {
        assert_eq!(s.get(c.id), Some(c.default), "{}", c.id);
        assert!(s.set_by_id(c.id, c.default));
    }
    assert!(!s.on, "out by default");
    let find = |id| as_control::find(controls(), id).unwrap();
    assert_eq!(find("attack").format(3.0), "3 (234 µs)");
    assert_eq!(find("release").format(5.0), "5 (140 ms)");
    assert_eq!(find("ratio").format(0.0), "4:1");
    assert_eq!(find("ratio").format(ALL), "ALL");
    // Typing a time lands on the nearest fine step of the dial, and shows it.
    let d = find("attack").parse("330us").unwrap();
    assert_eq!(d, 2.4);
    assert_eq!(find("attack").format(d), "2.4 (338 µs)");
    for (dial, a, r) in [(1.0, 800e-6, 1.1), (4.0, 126e-6, 0.235), (7.0, 20e-6, 0.050)] {
        assert!((attack_time(dial) / a - 1.0).abs() < 0.01, "{dial}");
        assert!((release_time(dial) / r - 1.0).abs() < 0.01, "{dial}");
        assert!((attack_dial(attack_time(dial)) - dial).abs() < 1e-9);
        assert!((release_dial(release_time(dial)) - dial).abs() < 1e-9);
    }
    let t = Settings { rack: Rack { input: 99.0, ratio: 9, ..s.rack }, ..s };
    assert_eq!((t.held().rack.input, t.held().rack.ratio), (40.0, 4));
}

/// The stepped sine, steps held 2 s, ATK 4, REL 1, Input +20, at one button. The cell (the stage
/// skipped) reads `curve()` within 0.1 dB; the whole unit reads `curve()` and the stage's own pull
/// on the fundamental at that level and reduction, within 0.1. Streamed a step at a time.
fn static_curve(button: f64) {
    let rate = 48_000;
    let s = with(&[("input", 20.0), ("attack", 4.0), ("release", 1.0), ("ratio", button)]);
    let (mut cell, mut whole) = (unit(rate, &s), unit(rate, &s));
    cell.bare = true;
    let (mut worst, mut most_pull) = (0.0f64, 0.0f64);
    rigs::each_step(rate, 2.0, |d, x| {
        let (mut a, mut b) = (x.clone(), x);
        a.chunks_mut(1024).for_each(|c| cell.process(c));
        b.chunks_mut(1024).for_each(|c| whole.process(c));
        let want = curve(&s, d);
        let got = rigs::read_tail(&a, rate);
        worst = worst.max((got - want).abs());
        assert!((got - want).abs() < 0.1, "{button}, the cell at {d}: {got:.3} against {want:.3}");
        let pull = stage_pull(level(want), whole.readouts().gr_db);
        most_pull = most_pull.min(pull);
        let got = rigs::read_tail(&b, rate);
        assert!(
            (got - want - pull).abs() < 0.1,
            "{button}, the whole at {d}: {got:.3} against {:.3}",
            want + pull
        );
    });
    eprintln!(
        "button {button}: the cell within {worst:.3} dB of the curve; the stage pulls up to {most_pull:.2} dB"
    );
}

/// The stage's gain on a sine's fundamental, dB, at a peak `a` and a reduction `rho`.
fn stage_pull(a: f64, rho: f64) -> f64 {
    let n = 4800;
    let x: Vec<f64> =
        (0..n).map(|k| stage(&RACK, a * libm::sin(std::f64::consts::TAU * k as f64 / 48.0), rho)).collect();
    20.0 * libm::log10(tone(&x, 1000.0, 48_000, 0, n).0 / a)
}

#[test]
fn the_static_curve_4() {
    static_curve(0.0);
}
#[test]
fn the_static_curve_8() {
    static_curve(1.0);
}
#[test]
fn the_static_curve_12() {
    static_curve(2.0);
}
#[test]
fn the_static_curve_20() {
    static_curve(3.0);
}
#[test]
fn the_static_curve_all() {
    static_curve(ALL);
}

/// The fitted pole of a DC attack from silence, the core's exact test: driven `over` dB over the
/// threshold, the error against the unheld final value shrinks by `p` a sample, checked while the
/// reduction is under 40 dB and the error over 1e-4. Returns the attack time the pole means, or
/// `None` where the first sample's step already passes 40 dB (ALL at ATK 7 at 44.1 and 48 kHz:
/// `(1 − p)` of 52 dB), and then checks the reduction is held there.
fn attack_pole(rate: u32, button: f64, dial: f64, over: f64, input: f64) -> Option<f64> {
    let s = with(&[("input", input), ("attack", dial), ("release", 1.0), ("ratio", button)]);
    let b = s.button();
    let fin = over * (1.0 - 1.0 / b.ratio);
    let n = rate as usize / 10;
    let x = vec![level(s.threshold() + over - input); n];
    let rho = rhos(rate, &s, &x);
    let p = libm::exp(-1.0 / (s.times().0 * rate as f64));
    if (1.0 - p) * fin >= MAX_GR_DB {
        assert_eq!(rho[1], MAX_GR_DB, "{rate} button {button} ATK {dial}: held at once");
        return None;
    }
    let (mut sum, mut count) = (0.0, 0);
    for k in 0..n - 1 {
        let (e0, e1) = (fin - rho[k], fin - rho[k + 1]);
        if rho[k + 1] >= MAX_GR_DB || e1.abs() < 1e-4 {
            break;
        }
        let q = e1 / e0;
        assert!((q - p).abs() < 1e-9, "{rate} button {button} ATK {dial}: {q} against {p} at {k}");
        (sum, count) = (sum + q, count + 1);
    }
    assert!(count > 0, "{rate} button {button} ATK {dial}: nothing checked");
    Some(-1.0 / (rate as f64 * libm::log(sum / count as f64)))
}

use crate::MAX_GR_DB;

#[test]
fn the_attack_is_the_dials() {
    for rate in RATES {
        for dial in [1.0, 4.0, 7.0] {
            // 32 dB over: settled above the widest of the four knees, under 40 dB taken.
            for button in 0..4 {
                assert!(attack_pole(rate, button as f64, dial, 32.0, 20.0).is_some());
            }
            // ALL settles past 40 dB for any step above its knee: driven 55 over, and checked on
            // the way to 40.
            attack_pole(rate, ALL, dial, 55.0, 40.0);
        }
    }
}

/// The fitted pole of a release from `over` dB over to 40 dB under the threshold, once under the
/// knee. Returns the release time it means.
fn release_pole(rate: u32, button: f64, dial: f64) -> f64 {
    let s = with(&[("input", 20.0), ("attack", 7.0), ("release", dial), ("ratio", button)]);
    let (fall, n) = (rate as usize / 10, rate as usize / 10 + 4000);
    let (hi, lo) = (level(s.threshold() + 20.0 - 20.0), level(s.threshold() - 40.0 - 20.0));
    let x: Vec<f64> = (0..n).map(|k| if k < fall { hi } else { lo }).collect();
    let rho = rhos(rate, &s, &x);
    let p = libm::exp(-1.0 / (s.times().1 * rate as f64));
    // A sample after the fall the detector hears the output under the knee.
    let mut sum = 0.0;
    for k in fall + 2..n - 1 {
        let q = rho[k + 1] / rho[k];
        assert!((q - p).abs() < 1e-9, "{rate} button {button} REL {dial}: {q} against {p}");
        sum += q;
    }
    -1.0 / (rate as f64 * libm::log(sum / (n - fall - 3) as f64))
}

#[test]
fn the_release_is_the_dials_under_the_knee() {
    for rate in RATES {
        for dial in [1.0, 4.0, 7.0] {
            for button in 0..5 {
                release_pole(rate, button as f64, dial);
            }
        }
    }
}

#[test]
fn all_is_its_own() {
    let rate = 48_000;
    for dial in [1.0, 4.0, 7.0] {
        // ALL's attack at ATK 7 (10 µs) passes 40 dB in its first sample at 48 kHz: read at 96.
        let r = if dial == 7.0 { 96_000 } else { rate };
        let a20 = attack_pole(r, 3.0, dial, 32.0, 20.0).unwrap();
        let aall = attack_pole(r, ALL, dial, 55.0, 40.0).unwrap();
        assert!((aall / a20 - 0.5).abs() < 0.005, "attack {aall} against {a20}");
        let (r20, rall) = (release_pole(rate, 3.0, dial), release_pole(rate, ALL, dial));
        assert!((rall / r20 - 0.5).abs() < 0.005, "release {rall} against {r20}");
    }
    // The threshold and knee, read off `curve()`: the knee's foot `E` is where the reduction
    // starts, and a point in the knee, `x = y + (R − 1)(y − E)²/(2W)`, gives its width. (Fed back,
    // ALL's straight part starts 85 dB over its threshold, past the 40 dB hold: it can't be read
    // from there.)
    for (button, t, w, r) in [(3.0, -18.0, 3.0, 20.0), (ALL, -15.0, 10.0, 20.0)] {
        let s = with(&[("ratio", button)]);
        let reduces = |x: f64| curve(&s, x) < x - 1e-9;
        let (mut lo, mut hi) = (-60.0, 0.0);
        for _ in 0..80 {
            let mid = 0.5 * (lo + hi);
            if reduces(mid) { hi = mid } else { lo = mid }
        }
        let foot = hi;
        let x = foot + 4.0;
        let y = curve(&s, x);
        let knee = (r - 1.0) * (y - foot) * (y - foot) / (2.0 * (x - y));
        let thr = foot + 0.5 * knee;
        assert!((thr / t - 1.0).abs() < 0.01, "button {button}: threshold {thr}");
        assert!((knee / w - 1.0).abs() < 0.01, "button {button}: knee {knee}");
    }
}

/// The RFC's burst, −30 to −6 dBFS at Input +20, REL 1, 4:1, starting at a zero crossing going
/// up. A peak detector pulls only near the crests, so the burst reads the duty cycle as much as
/// the time: pinned as found at ATK 1 (1.59 times the dial, measured 2026-11-12), recorded at ATK 4
/// (1.81, the sine's quarter period to its first crest as much as the 126 µs). The DC test above is
/// the exact pin.
#[test]
fn the_burst_as_found() {
    let rate = 48_000;
    for (dial, lo, hi) in [(1.0, 1.4, 1.8), (4.0, 1.0, 3.0)] {
        let s = with(&[("input", 20.0), ("attack", dial), ("release", 1.0)]);
        let (up, down, n) = (rate as usize / 2, rate as usize, rate as usize * 3 / 2);
        let rho = rhos(rate, &s, &rigs::burst(rate, n, up, down));
        let base = rho[up - 1];
        let fin = rho[down - 4800..down].iter().sum::<f64>() / 4800.0;
        let k = (up..down).find(|&k| rho[k] >= base + (fin - base) * (1.0 - libm::exp(-1.0))).unwrap();
        let t = (k + 1 - up) as f64 / rate as f64 / attack_time(dial);
        eprintln!("the burst at ATK {dial}: {t:.2} times the dial");
        assert!((lo..hi).contains(&t), "{t}");
    }
}

#[test]
fn switching_in_is_never_louder() {
    let rate = 48_000;
    let win = rate as usize / 20;
    let rms = |x: &[f64]| libm::sqrt(x.iter().map(|v| v * v).sum::<f64>() / x.len() as f64);
    let peak = |x: &[f64]| x.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    let mut most = -300.0f64;
    for (name, x) in
        [("speech", speech(110.0, 1.0, 3.0, 0.2, rate, 3)), ("impacts", impacts(1.0, 3.0, rate, 4))]
    {
        for d in [-40.0, -30.0, -20.0, -10.0, 0.0] {
            let x: Vec<f64> = x.iter().map(|v| v * level(d)).collect();
            let y = {
                let mut u = unit(rate, &with(&[]));
                let mut y = x.clone();
                y.chunks_mut(1024).for_each(|c| u.process(c));
                y
            };
            for (a, b) in x.chunks(win).zip(y.chunks(win)) {
                if peak(a) == 0.0 {
                    continue;
                }
                let (dp, dr) = (20.0 * libm::log10(peak(b) / peak(a)), 20.0 * libm::log10(rms(b) / rms(a)));
                most = most.max(dp).max(dr);
                assert!(dp <= 0.1 && dr <= 0.1, "{name} at {d}: peak {dp:+.3}, rms {dr:+.3}");
            }
        }
    }
    eprintln!("switched in, never more than {most:+.4} dB");
}

#[test]
fn the_stage() {
    let rate = 48_000;
    let run = |a: f64, rho: f64| {
        let n = 4800;
        let x: Vec<f64> = (0..n)
            .map(|k| stage(&RACK, a * libm::sin(std::f64::consts::TAU * k as f64 / 48.0), rho))
            .collect();
        thd(&x, 1000.0, rate, 0, n)
    };
    assert_eq!(stage(&RACK, 0.0, 0.0), 0.0);
    assert_eq!(stage(&RACK, 0.0, 20.0), 0.0);
    let quiet = run(level(-18.0), 0.0);
    assert!(quiet < 0.003, "{quiet}");
    let mut last = quiet;
    for d in -17..=0 {
        let t = run(level(d as f64), 0.0);
        assert!(t > last, "at {d}: {t} after {last}");
        last = t;
    }
    assert!((0.01..=0.06).contains(&last), "at 0 dBFS: {last}");
    for d in [-18.0, -12.0, -6.0, 0.0] {
        assert!(run(level(d), 10.0) > run(level(d), 0.0), "at {d}");
    }
    // The second harmonic about 10 dB under the third at 0 dBFS.
    let n = 4800;
    let x: Vec<f64> =
        (0..n).map(|k| stage(&RACK, libm::sin(std::f64::consts::TAU * k as f64 / 48.0), 0.0)).collect();
    let (h2, h3) = (tone(&x, 2000.0, rate, 0, n).0, tone(&x, 3000.0, rate, 0, n).0);
    let under = 20.0 * libm::log10(h3 / h2);
    eprintln!(
        "the stage: THD {:.3}% at −18 dBFS, {:.2}% at 0; H2 {under:.1} dB under H3",
        quiet * 100.0,
        last * 100.0
    );
    assert!((6.0..14.0).contains(&under), "{under}");
}

#[test]
fn aliasing() {
    // At 44.1 kHz, where a 1 kHz tone's folded harmonics don't land on its own harmonics.
    let rate = 44_100;
    for (hz, d, pinned) in [(1000.0, -18.0, true), (5000.0, 0.0, false)] {
        let n = rate as usize * 3 / 2;
        let x = sine(hz, level(d), n, rate);
        let mut y = x.clone();
        let mut u = unit(rate, &with(&[]));
        y.chunks_mut(1024).for_each(|c| u.process(c));
        let tail = &y[rate as usize / 2..];
        let fund = tone(tail, hz, rate, 0, tail.len()).0;
        let (ratio, at) = worst_product(tail, hz, rate, 2);
        let worst = 20.0 * libm::log10(ratio * fund);
        eprintln!("{hz} Hz at {d} dBFS: the worst product {worst:.1} dBFS at {at:.0} Hz");
        if pinned {
            assert!(worst < -80.0, "{worst}");
        }
    }
}

#[test]
fn out_is_the_input_to_the_bit() {
    let rate = 48_000;
    let x = speech(110.0, 1.0, 2.0, 0.2, rate, 5);
    let s = Settings { on: false, ..with(&[("input", 40.0), ("output", 20.0), ("ratio", ALL)]) };
    let mut y = x.clone();
    unit(rate, &s).process(&mut y);
    assert!(same(&x, &y));
}

#[test]
fn never_past_40_db() {
    let rate = 48_000;
    let s = with(&[("input", 40.0), ("ratio", ALL), ("attack", 7.0)]);
    let mut x = vec![1.0; rate as usize];
    x.extend(rigs::burst(rate, rate as usize, 0, rate as usize).iter().map(|v| v * 16.0));
    let most = rhos(rate, &s, &x).into_iter().fold(0.0, f64::max);
    assert!(most <= MAX_GR_DB && most > 39.0, "{most}");
}

#[test]
fn a_held_key_glides() {
    for rate in RATES {
        let grid = |id: &str| rigs::grid(controls(), id);
        let base = with(&[]);
        for (id, path) in [
            ("input", grid("input")),
            ("output", grid("output")),
            ("attack", grid("attack")),
            ("release", grid("release")),
            ("ratio", vec![0.0, 1.0, 2.0, 3.0, 4.0, 3.0, 2.0, 1.0]),
        ] {
            let at = |v: f64| {
                let mut s = base;
                s.set_by_id(id, v);
                s
            };
            rigs::held_key(
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
        let l = speech(110.0, 0.9, 2.0, 0.2, rate, 1);
        let r = impacts(0.8, 2.0, rate, 2);
        let apply = |u: &mut Fet, at: usize| {
            if at.is_multiple_of(1024) {
                let i = at / 1024;
                u.set(&with(&[
                    ("input", (i % 9) as f64 * 5.0),
                    ("output", (i % 5) as f64 * -2.0),
                    ("attack", 1.0 + (i % 7) as f64),
                    ("release", 7.0 - (i % 4) as f64),
                    ("ratio", (i / 3 % 5) as f64),
                    ("in", if i % 13 == 6 { 0.0 } else { 1.0 }),
                ]));
            }
        };
        rigs::blocks(|| unit(rate, &with(&[("input", 20.0)])), apply, &l, &r);
    }
}

#[test]
fn every_output_is_finite() {
    for rate in RATES {
        rigs::finite(
            || unit(rate, &with(&[("input", 40.0), ("ratio", ALL), ("attack", 7.0), ("output", 20.0)])),
            rate,
        );
    }
}

#[test]
fn one_detector_for_both_channels() {
    let x = speech(110.0, 0.9, 1.0, 0.2, 48_000, 4);
    rigs::linked(|| unit(48_000, &with(&[("input", 20.0)])), &x);
}

/// The 2 Hz high-pass takes about 37 s to fall to 1e-200; what's pinned is that it never sits
/// under it unflushed.
#[test]
fn a_tail_to_silence_takes_no_longer() {
    let rate = 48_000;
    let mut u = unit(rate, &with(&[("input", 20.0)]));
    rigs::tail(&mut u, &speech(110.0, 0.9, 1.0, 0.2, rate, 9), rate);
    assert!(u.hp_mem.iter().flatten().all(|&v| v == 0.0 || v.abs() >= 1e-200), "flushed");
}

/// The high-pass's job: the stage's DC out. A 0 dBFS sine with the stage working hard: the
/// output's mean over whole cycles, after 2 s, is nil.
#[test]
fn the_stage_leaves_no_dc() {
    let rate = 48_000;
    let s = with(&[("input", 40.0), ("output", 20.0), ("ratio", ALL), ("attack", 1.0), ("release", 1.0)]);
    let x = sine(1000.0, 1.0, 3 * rate as usize, rate);
    let mut y = x.clone();
    unit(rate, &s).process(&mut y);
    let tail = &y[2 * rate as usize..];
    let mean = tail.iter().sum::<f64>() / tail.len() as f64;
    let peak = tail.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    let bare = {
        let mut u = unit(rate, &s);
        u.hp = First::IDENTITY;
        let mut z = x.clone();
        u.process(&mut z);
        z[2 * rate as usize..].iter().sum::<f64>() / tail.len() as f64
    };
    eprintln!(
        "the stage's DC: {:.1} dB under the peak unfiltered, {:.1} filtered",
        20.0 * libm::log10(bare.abs() / peak),
        20.0 * libm::log10(mean.abs().max(1e-300) / peak)
    );
    assert!(bare.abs() > 1e-4 * peak, "the stage puts DC in: {bare}");
    assert!(mean.abs() < 1e-6 * peak, "{mean}");
}
