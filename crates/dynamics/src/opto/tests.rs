//! `docs/spec-opto.md`, "Pinned by tests".

use super::*;
use crate::rigs::{self, level, same};
use as_envelope::corpus::{phrases, speech};
use as_envelope::signal::sine;

const RATES: [u32; 3] = [44_100, 48_000, 96_000];

/// The ratio switch's positions, by name.
const P2A: f64 = 0.0;
const P4: f64 = 2.0;
const P20: f64 = 5.0;

impl rigs::Unit for Opto {
    fn process(&mut self, x: &mut [f64]) {
        Opto::process(self, x);
    }
    fn process2(&mut self, l: &mut [f64], r: &mut [f64]) {
        Opto::process2(self, l, r);
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

fn unit(rate: u32, s: &Settings) -> Opto {
    let mut u = Opto::new(rate as f64);
    u.set(s);
    u
}

fn through(rate: u32, s: &Settings, x: &[f64]) -> Vec<f64> {
    let mut u = unit(rate, s);
    let mut y = x.to_vec();
    y.chunks_mut(1024).for_each(|b| u.process(b));
    y
}

/// The reduction at every sample of `x`, and the `held` readout.
fn rhos(rate: u32, s: &Settings, x: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let mut u = unit(rate, s);
    let (mut gr, mut held) = (Vec::with_capacity(x.len()), Vec::with_capacity(x.len()));
    for &v in x {
        u.process(&mut [v]);
        gr.push(u.readouts().gr_db);
        held.push(u.readouts().held);
    }
    (gr, held)
}

/// A constant that the light reads at `db`, sine-referenced: `10·log10(2a²)`, 3 dB under a sine of
/// that peak.
fn dc(db: f64) -> f64 {
    libm::sqrt(0.5 * libm::pow(10.0, db / 10.0))
}

/// The 63% time of a rise from `at`, against the mean over the last 100 ms before `end`.
fn t63(rho: &[f64], at: usize, end: usize, rate: u32) -> f64 {
    let w = rate as usize / 10;
    let fin = rho[end - w..end].iter().sum::<f64>() / w as f64;
    let k = (at..end).find(|&k| rho[k] >= fin * (1.0 - libm::exp(-1.0))).unwrap();
    (k + 1 - at) as f64 / rate as f64
}

/// The 63% time of the DC step from silence to `over` dB over the threshold.
fn attack_on_dc(rate: u32, s: &Settings, over: f64) -> f64 {
    let (at, n) = (rate as usize / 10, rate as usize * 11 / 10);
    let x: Vec<f64> = (0..n).map(|k| if k >= at { dc(s.threshold() + over) } else { 0.0 }).collect();
    t63(&rhos(rate, s, &x).0, at, n, rate)
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
    assert_eq!(s.position().ratio, 4.0);
    let reduce = as_control::find(controls(), "reduce").unwrap();
    assert_eq!(reduce.format(40.0), "40 (−18.9 dBFS)");
    assert_eq!((threshold(0.0), threshold(100.0)), (10.0, -40.0));
    for r in [0.0, 13.0, 40.0, 77.0, 100.0] {
        assert!((reduce_for(threshold(r)) - r).abs() < 1e-9);
    }
    let ratio = as_control::find(controls(), "ratio").unwrap();
    assert_eq!((ratio.format(0.0), ratio.format(5.0)), ("2A".to_string(), "20:1".to_string()));
    s.set_by_id("ratio", 9.0);
    assert_eq!(s.position().ratio, 20.0);
    let t = Settings { rack: Rack { gain: 99.0, ratio: 40, ..s.rack }, ..s };
    assert_eq!((t.held().rack.gain, t.held().rack.ratio), (30.0, 5));
}

/// The stepped sine, each step held 5 s, at Reduce 60, Gain 0, at one position: within 0.3 dB of
/// the curve. Streamed a step at a time, keeping each step's last 100 ms.
fn static_curve(position: f64) {
    let rate = 48_000;
    let s = with(&[("reduce", 60.0), ("ratio", position)]);
    let mut u = unit(rate, &s);
    let mut worst = 0.0f64;
    rigs::each_step(rate, 5.0, |d, mut x| {
        x.chunks_mut(1024).for_each(|b| u.process(b));
        let out = rigs::read_tail(&x, rate);
        let want = curve(&s, d);
        worst = worst.max((out - want).abs());
        assert!((out - want).abs() < 0.3, "{position} at {d} dBFS: {out:.3} against {want:.3}");
    });
    eprintln!("the curve at position {position}: within {worst:.3} dB");
}

#[test]
fn the_static_curve_2a() {
    static_curve(0.0);
}
#[test]
fn the_static_curve_2() {
    static_curve(1.0);
}
#[test]
fn the_static_curve_4() {
    static_curve(2.0);
}
#[test]
fn the_static_curve_8() {
    static_curve(3.0);
}
#[test]
fn the_static_curve_12() {
    static_curve(4.0);
}
#[test]
fn the_static_curve_20() {
    static_curve(5.0);
}

#[test]
fn the_attack() {
    let rate = 48_000;
    let s = with(&[("reduce", 60.0), ("ratio", P4)]);
    let t = attack_on_dc(rate, &s, 6.0);
    eprintln!("the attack, 6 dB over at 4:1: 63% in {:.2} ms", t * 1e3);
    assert!((9e-3..=15e-3).contains(&t), "{t}");
    // The burst, −30 to −6 dBFS, with the threshold at −12 so −6 is the same 6 dB over.
    let s = with(&[("reduce", reduce_for(-12.0)), ("ratio", P4)]);
    let dc_t = attack_on_dc(rate, &s, 6.0);
    let (up, down, n) = (rate as usize / 2, rate as usize * 3 / 2, rate as usize * 2);
    let (rho, _) = rhos(rate, &s, &rigs::burst(rate, n, up, down));
    let base = rho[up - 1];
    let w = rate as usize / 10;
    let fin = rho[down - w..down].iter().sum::<f64>() / w as f64;
    let k = (up..down).find(|&k| rho[k] >= base + (fin - base) * (1.0 - libm::exp(-1.0))).unwrap();
    let burst = (k + 1 - up) as f64 / rate as f64;
    eprintln!("on the burst: {:.2} ms against {:.2} on DC", burst * 1e3, dc_t * 1e3);
    assert!((burst / dc_t - 1.0).abs() < 0.25, "{burst} against {dc_t}");
}

#[test]
fn faster_when_pushed_harder() {
    let rate = 48_000;
    for position in [P4, P20] {
        let s = with(&[("reduce", 60.0), ("ratio", position)]);
        let (hard, soft) = (attack_on_dc(rate, &s, 12.0), attack_on_dc(rate, &s, 3.0));
        eprintln!("position {position}: 63% in {:.2} ms 12 dB over, {:.2} at 3", hard * 1e3, soft * 1e3);
        assert!(hard <= 0.75 * soft, "{hard} against {soft}");
    }
}

#[test]
fn a_two_part_release() {
    let rate = 48_000;
    let s = with(&[("reduce", 60.0), ("ratio", P4)]);
    let (fall, n) = (rate as usize, rate as usize * 5);
    let x: Vec<f64> = (0..n).map(|k| if k < fall { dc(s.threshold() + 12.0) } else { 0.0 }).collect();
    let (rho, _) = rhos(rate, &s, &x);
    let top = rho[fall - 1];
    let half = (fall..n).find(|&k| rho[k] <= 0.5 * top).unwrap();
    let ninety = (fall..n).find(|&k| rho[k] <= 0.1 * top).unwrap();
    let (h, l) = ((half - fall) as f64 / rate as f64, (ninety - half) as f64 / rate as f64);
    eprintln!("from {top:.2} dB: half back in {:.1} ms, then 90% back {:.2} s later", h * 1e3, l);
    assert!(h <= 0.080, "{h}");
    assert!(l >= 0.5, "{l}");
}

/// The time from the fall back to 3 dB of reduction, after a −6 dBFS tone held `secs`.
fn memory(rate: u32, position: f64, secs: f64) -> f64 {
    let s = with(&[("reduce", 60.0), ("ratio", position)]);
    let (fall, n) = ((secs * rate as f64) as usize, (secs * rate as f64) as usize + 20 * rate as usize);
    let mut x = sine(1000.0, level(-6.0), n, rate);
    x[fall..].iter_mut().for_each(|v| *v = 0.0);
    let (rho, _) = rhos(rate, &s, &x);
    let k = (fall..n).find(|&k| rho[k] <= 3.0).unwrap();
    (k - fall) as f64 / rate as f64
}

#[test]
fn the_memory_holds_after_a_long_push() {
    let rate = 48_000;
    let mut ratios = vec![];
    for position in [P4, P2A] {
        let (short, long) = (memory(rate, position, 0.1), memory(rate, position, 2.0));
        eprintln!("position {position}: back to 3 dB in {short:.3} s after 0.1 s, {long:.3} s after 2 s");
        assert!(long >= 1.5 * short, "{long} against {short}");
        ratios.push(long / short);
    }
    assert!(ratios[1] > ratios[0], "2A remembers more: {ratios:?}");
    // 2A's knee is the widest and its memory's scale the smallest.
    let p = RACK.positions;
    assert!(p[1..].iter().all(|q| q.knee < p[0].knee && q.memory > p[0].memory));
}

#[test]
fn no_pumping() {
    let rate = 48_000;
    let c = 1.0 - libm::exp(-1.0 / (RACK.fast * rate as f64));
    for (name, x) in [
        ("speech", speech(110.0, level(-12.0), 4.0, 0.2, rate, 3)),
        ("phrases", phrases(180.0, level(-12.0), 6.0, 1.2, 0.6, rate, 5)),
    ] {
        for position in 0..6 {
            let (rho, _) = rhos(rate, &with(&[("reduce", 60.0), ("ratio", position as f64)]), &x);
            assert!(rho.iter().any(|&r| r > 3.0), "{name} at {position}: it worked");
            for k in 1..rho.len() {
                let fell = rho[k - 1] - rho[k];
                let bound = 1.05 * rho[k - 1] * c + 1e-12;
                assert!(fell <= bound, "{name} at {position}, sample {k}: fell {fell:e} against {bound:e}");
            }
        }
    }
}

#[test]
fn under_the_threshold_and_out_are_the_input_to_the_bit() {
    let rate = 48_000;
    let s = with(&[("reduce", 60.0)]);
    let x = speech(110.0, level(s.threshold() - 30.0), 3.0, 0.2, rate, 7);
    for position in 0..6 {
        let s = with(&[("reduce", 60.0), ("ratio", position as f64)]);
        assert!(same(&x, &through(rate, &s, &x)), "{position}");
    }
    let x = speech(110.0, 1.0, 3.0, 0.2, rate, 7);
    let out = Settings { on: false, ..with(&[("reduce", 100.0), ("gain", 12.0)]) };
    assert!(same(&x, &through(rate, &out, &x)));
}

#[test]
fn held_rises_with_time_and_falls_to_nothing() {
    let rate = 48_000;
    let s = with(&[("reduce", 60.0), ("ratio", P2A)]);
    // Two seconds of a −6 dBFS tone at 2A, the most memory, then a minute of silence.
    let (fall, n) = (2 * rate as usize, 62 * rate as usize);
    let mut x = sine(1000.0, level(-6.0), n, rate);
    x[fall..].iter_mut().for_each(|v| *v = 0.0);
    let mut u = unit(rate, &s);
    let mut held = vec![];
    for b in x.chunks_mut(rate as usize / 2) {
        u.process(b);
        held.push(u.readouts().held);
    }
    eprintln!("held: {:?} … {:?}", &held[..4], &held[held.len() - 2..]);
    assert!(held[0] > 0.0 && held[1] > held[0] && held[3] > held[1], "rises: {:?}", &held[..4]);
    assert_eq!(*held.last().unwrap(), 0.0, "nothing after a minute");
    assert_eq!(u.readouts().gr_db, 0.0);
}

#[test]
fn never_past_40_db() {
    let rate = 48_000;
    let s = with(&[("reduce", 100.0), ("ratio", P20)]);
    let mut x = vec![1.0; rate as usize];
    x.extend(rigs::burst(rate, rate as usize, 0, rate as usize).iter().map(|v| v * 16.0));
    let (rho, _) = rhos(rate, &s, &x);
    let most = rho.iter().cloned().fold(0.0, f64::max);
    assert!(most <= MAX_GR_DB && most > 30.0, "{most}");
}

use crate::MAX_GR_DB;

#[test]
fn a_held_key_glides() {
    for rate in RATES {
        let grid = |id: &str| rigs::grid(controls(), id);
        let base = with(&[]);
        for (id, path) in [
            ("reduce", grid("reduce")),
            ("gain", grid("gain")),
            ("ratio", vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 4.0, 3.0, 2.0, 1.0]),
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
        let n = rate as usize * 2;
        let l = speech(110.0, 0.7, 2.0, 0.2, rate, 1);
        let r = phrases(180.0, 0.5, 2.0, 0.5, 0.3, rate, 2);
        assert_eq!((l.len(), r.len()), (n, n));
        let apply = |u: &mut Opto, at: usize| {
            if at.is_multiple_of(1024) {
                let i = at / 1024;
                u.set(&with(&[
                    ("reduce", (i % 11) as f64 * 9.0),
                    ("gain", (i % 4) as f64 * 2.0),
                    ("ratio", (i / 5 % 6) as f64),
                    ("in", if i % 13 == 6 { 0.0 } else { 1.0 }),
                ]));
            }
        };
        rigs::blocks(|| unit(rate, &with(&[("reduce", 60.0)])), apply, &l, &r);
    }
}

#[test]
fn every_output_is_finite() {
    for rate in RATES {
        rigs::finite(|| unit(rate, &with(&[("reduce", 100.0), ("ratio", P20), ("gain", 30.0)])), rate);
    }
}

#[test]
fn one_light_for_both_channels() {
    let x = speech(110.0, 0.7, 1.0, 0.2, 48_000, 4);
    rigs::linked(|| unit(48_000, &with(&[("reduce", 60.0)])), &x);
}

#[test]
fn a_tail_to_silence_takes_no_longer() {
    let rate = 48_000;
    let mut u = unit(rate, &with(&[("reduce", 60.0)]));
    rigs::tail(&mut u, &speech(110.0, 0.7, 1.0, 0.2, rate, 9), rate);
}

/// The slow envelope's charge. Corrected by the loop's slope as the spec writes, though outside the
/// loop while the fast one leads, it might have charged slower than the attack, and a short word let
/// go by the fast part alone. Measured (2026-11-12): it charges with the whole, within a
/// millisecond, since early on half the target overshoots as the whole's does. Pinned as found.
#[test]
fn the_slow_part_charges_with_the_whole() {
    let rate = 48_000;
    for (position, over) in [(P4, 6.0), (P4, 12.0), (P20, 6.0), (P20, 12.0)] {
        let s = with(&[("reduce", 60.0), ("ratio", position)]);
        let (at, n) = (rate as usize / 10, rate as usize * 31 / 10);
        let mut u = unit(rate, &s);
        let mut slow = vec![];
        for k in 0..n {
            u.process(&mut [if k >= at { dc(s.threshold() + over) } else { 0.0 }]);
            slow.push(u.cell.slow.rho());
        }
        let t = t63(&slow, at, n, rate);
        let fast = attack_on_dc(rate, &s, over);
        eprintln!(
            "position {position}, {over} dB over: the slow part 63% in {:.0} ms, the whole {:.1} ms",
            t * 1e3,
            fast * 1e3
        );
        assert!((t / fast - 1.0).abs() < 0.1, "{t} against {fast}");
    }
}

/// The memory charges from a reduction as small as 0.05 dB: its floor applies only as it falls.
#[test]
fn the_memory_charges_from_a_whisker() {
    for rate in RATES {
        let s = with(&[("reduce", 60.0), ("ratio", P4)]);
        // A constant 6.74 dB under the threshold, in the 15 dB knee's foot: about 0.05 dB taken.
        let x = vec![dc(s.threshold() - 6.74); 2 * rate as usize];
        let mut u = unit(rate, &s);
        let mut y = x.clone();
        u.process(&mut y);
        let r = u.readouts();
        assert!(r.gr_db > 0.02 && r.gr_db < 0.1, "{rate}: {}", r.gr_db);
        assert!(r.held > 0.0, "{rate}: held {}", r.held);
    }
}
