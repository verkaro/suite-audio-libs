//! `docs/spec.md`, "Pinned by tests, for the core", on a test unit built from the core alone: a
//! feedback compressor with a peak detector, its threshold ramped in dB, a make-up gain and In.

use super::*;
use crate::rigs::{self, Unit, level};
use as_envelope::signal::noise;

const RATES: [u32; 3] = [44_100, 48_000, 96_000];

/// The test unit's settings.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Set {
    threshold: f64,
    ratio: f64,
    width: f64,
    attack: f64,
    release: f64,
    makeup_db: f64,
    on: bool,
}

const SET: Set =
    Set { threshold: -30.0, ratio: 4.0, width: 2.0, attack: 0.001, release: 0.1, makeup_db: 0.0, on: true };

/// A feedback compressor on the core, nothing else: `docs/spec.md`'s loop, written out.
struct Comp {
    rate: f64,
    knee: Knee,
    follower: Follower,
    threshold: Ramp,
    makeup: Ramp,
    on: Fade<bool>,
    /// The level of the cell's last output, before make-up.
    heard: f64,
    sent: Set,
    gr: f64,
}

impl Comp {
    fn new(rate: u32, s: Set) -> Comp {
        let r = rate as f64;
        Comp {
            rate: r,
            knee: Knee { threshold: s.threshold, ratio: s.ratio, width: s.width },
            follower: Follower::new(r, s.attack, s.release, false),
            threshold: Ramp::new(s.threshold, samples(RAMP_S, r)),
            makeup: Ramp::new(level(s.makeup_db), samples(RAMP_S, r)),
            on: Fade::new(s.on, samples(FADE_S, r)),
            heard: -300.0,
            sent: s,
            gr: 0.0,
        }
    }

    fn set(&mut self, s: Set) {
        if s.threshold != self.threshold.target() {
            self.threshold.retarget(s.threshold);
        }
        if level(s.makeup_db) != self.makeup.target() {
            self.makeup.retarget(level(s.makeup_db));
        }
        self.on.set(s.on);
        (self.knee.ratio, self.knee.width) = (s.ratio, s.width);
        self.follower.times(self.rate, s.attack, s.release);
        self.sent = s;
    }

    fn settled(&self) -> bool {
        self.threshold.done() && self.makeup.done() && self.on.done()
    }

    #[inline]
    fn tick<const C: usize>(&mut self, x: [f64; C]) -> [f64; C] {
        self.knee.threshold = self.threshold.advance();
        let l = self.heard;
        let rho = self.follower.step(self.knee.fb(l), self.knee.fb_slope(l));
        let g = gain(rho);
        let cell = x.map(|v| v * g);
        self.heard = peak_db(cell.iter().fold(0.0f64, |m, v| m.max(v.abs())));
        let m = self.makeup.advance();
        let (from, to, t) = self.on.advance();
        let mut y = x;
        for c in 0..C {
            let pick = |on: bool| if on { cell[c] * m } else { x[c] };
            y[c] = if t == 1.0 { pick(to) } else { (1.0 - t) * pick(from) + t * pick(to) };
        }
        self.gr = rho;
        y
    }
}

impl Unit for Comp {
    fn process(&mut self, x: &mut [f64]) {
        for v in x.iter_mut() {
            [*v] = self.tick([*v]);
        }
    }

    fn process2(&mut self, l: &mut [f64], r: &mut [f64]) {
        for (a, b) in l.iter_mut().zip(r.iter_mut()) {
            [*a, *b] = self.tick([*a, *b]);
        }
    }

    fn gr_db(&self) -> f64 {
        self.gr
    }
}

/// A plain gain, for the rigs' own test.
struct Plain(f64);

impl Unit for Plain {
    fn process(&mut self, x: &mut [f64]) {
        x.iter_mut().for_each(|v| *v *= self.0);
    }
    fn process2(&mut self, l: &mut [f64], r: &mut [f64]) {
        self.process(l);
        self.process(r);
    }
    fn gr_db(&self) -> f64 {
        0.0
    }
}

/// The reduction at each sample of `x`, through a fresh test unit.
fn reductions(rate: u32, s: Set, x: &[f64]) -> Vec<f64> {
    let mut u = Comp::new(rate, s);
    let mut y = x.to_vec();
    y.chunks_mut(1)
        .map(|b| {
            u.process(b);
            u.gr_db()
        })
        .collect()
}

fn knees() -> Vec<Knee> {
    let mut v = Vec::new();
    for ratio in [1.5, 2.0, 4.0, 8.0, 20.0] {
        for width in [0.0, 1.0, 6.0, 12.0] {
            v.push(Knee { threshold: -24.0, ratio, width });
        }
    }
    v
}

#[test]
fn the_gain_computers_are_nothing_below_the_knee() {
    for k in knees() {
        for x in [-300.0, -60.0, -24.0 - 0.5 * k.width, -24.0 - 0.5 * k.width - 1e-9] {
            assert_eq!(k.ff(x).to_bits(), 0.0f64.to_bits(), "{k:?} at {x}");
            assert_eq!(k.fb(x).to_bits(), 0.0f64.to_bits(), "{k:?} at {x}");
            assert_eq!(k.fb_slope(x), 0.0);
            assert_eq!(k.curve_ff(x), x);
        }
    }
}

#[test]
fn the_gain_computers_are_smooth_through_the_knee() {
    let d = 1e-12;
    for k in knees() {
        for edge in [k.threshold - 0.5 * k.width, k.threshold + 0.5 * k.width] {
            for (r, s) in [
                (&(|v| k.ff(v)) as &dyn Fn(f64) -> f64, &(|v| k.ff_slope(v)) as &dyn Fn(f64) -> f64),
                (&|v| k.fb(v), &|v| k.fb_slope(v)),
            ] {
                assert!((r(edge + d) - r(edge - d)).abs() < 1e-9, "{k:?} at {edge}: a step");
                if k.width > 0.0 {
                    assert!((s(edge + d) - s(edge - d)).abs() < 1e-9, "{k:?} at {edge}: a kink");
                }
            }
        }
        // The slopes are the reductions' derivatives, through the knee and beyond.
        for i in 0..200 {
            let v = k.threshold - 10.0 + 0.1 * i as f64 + 0.013;
            let h = 1e-6;
            let num = (k.fb(v + h) - k.fb(v - h)) / (2.0 * h);
            assert!((num - k.fb_slope(v)).abs() < 1e-5, "{k:?} at {v}: {num}");
            let num = (k.ff(v + h) - k.ff(v - h)) / (2.0 * h);
            assert!((num - k.ff_slope(v)).abs() < 1e-5, "{k:?} at {v}: {num}");
        }
        // The straight parts.
        let above = k.threshold + 0.5 * k.width + 3.0;
        assert!((k.ff_slope(above) - (1.0 - 1.0 / k.ratio)).abs() < 1e-15);
        assert!((k.fb_slope(above) - (k.ratio - 1.0)).abs() < 1e-15);
        assert!((k.ff(above) - (1.0 - 1.0 / k.ratio) * (above - k.threshold)).abs() < 1e-12);
        assert!((k.fb(above) - (k.ratio - 1.0) * (above - k.threshold)).abs() < 1e-12);
    }
}

#[test]
fn the_curve_is_the_ratio_above_the_knee_and_rises_everywhere() {
    for k in knees() {
        let mut last = f64::MIN;
        for i in 0..=1000 {
            let x = -80.0 + 0.08 * i as f64;
            let y = k.curve(x);
            assert!(y > last, "{k:?} at {x}: not rising");
            last = y;
            // Feedback at steady state: x = y + r(y), within the bisection's reach.
            let held = (x - y - k.fb(y)).abs() < 1e-9 || (x - y - MAX_GR_DB).abs() < 1e-9;
            assert!(held, "{k:?} at {x}");
            if x - k.threshold >= 0.5 * k.width * k.ratio
                && (x - k.threshold) * (1.0 - 1.0 / k.ratio) < MAX_GR_DB
            {
                let want = k.threshold + (x - k.threshold) / k.ratio;
                assert!((y - want).abs() < 1e-9, "{k:?} at {x}: {y} against {want}");
            }
        }
        // Held to 40 dB of reduction.
        assert!((k.curve(200.0) - (200.0 - MAX_GR_DB)).abs() < 1e-9 || k.ratio < 1.3);
    }
}

#[test]
fn no_reduction_is_unity_to_the_bit() {
    assert_eq!(gain(0.0).to_bits(), 1.0f64.to_bits());
    assert!((gain(6.0) - level(-6.0)).abs() < 1e-15);
    // A unit under its knee passes its input to the bit, and so does a unit out.
    for s in [SET, Set { on: false, ..SET }] {
        let x = noise(48_000, level(-40.0), 3);
        let mut y = x.clone();
        let mut u = Comp::new(48_000, s);
        u.process(&mut y);
        assert!(x.iter().zip(&y).all(|(a, b)| a.to_bits() == b.to_bits()), "{s:?}");
    }
    // Out passes to the bit even while the detector hears a signal far over.
    let x = noise(48_000, 0.9, 5);
    let mut y = x.clone();
    let mut u = Comp::new(48_000, Set { on: false, ..SET });
    u.process(&mut y);
    assert!(x.iter().zip(&y).all(|(a, b)| a.to_bits() == b.to_bits()));
    assert!(u.gr_db() > 10.0, "the detector ran while out");
}

/// `docs/spec.md`: on DC, from silence to a level whose settled output is above the knee, the
/// remaining error shrinks by exactly `p` each sample, at any ratio, rate and attack.
#[test]
fn the_loop_closes_with_the_dials_pole() {
    for rate in RATES {
        for ratio in [2.0, 4.0, 20.0] {
            for attack in [20e-6, 100e-6, 1e-3, 10e-3, 30e-3] {
                let s = Set { ratio, attack, release: 1.0, width: 2.0, threshold: -30.0, ..SET };
                // 0 dBFS DC, 30 dB over: settled 30/R over the threshold, above the knee.
                let final_ = 30.0 * (1.0 - 1.0 / ratio);
                let n = rate as usize / 2;
                let rho = reductions(rate, s, &rigs::dc_step(n, 100, n, 0.0, 1.0));
                let p = libm::exp(-1.0 / (attack * rate as f64));
                let mut checked = 0;
                for k in 101..n - 1 {
                    let (e0, e1) = (final_ - rho[k], final_ - rho[k + 1]);
                    if e1.abs() < 1e-4 {
                        break;
                    }
                    let q = e1 / e0;
                    assert!((q - p).abs() < 1e-9, "{rate} {ratio}:1 {attack}: {q} against {p} at {k}");
                    checked += 1;
                }
                assert!(checked > 0);
                let over = rho.iter().cloned().fold(0.0, f64::max) - final_;
                assert!(over < 0.01, "{rate} {ratio}:1 {attack}: over by {over} dB");
                assert!((rho[n - 1] - final_).abs() < 1e-6 || attack >= 30e-3);
            }
        }
    }
}

/// Released from a step that falls below the knee, the reduction falls by the release's pole.
#[test]
fn a_release_below_the_knee_reads_the_dial() {
    for rate in RATES {
        for release in [0.05, 0.3, 1.1] {
            let s = Set { release, attack: 1e-3, ratio: 8.0, ..SET };
            let (up, down, n) = (100, rate as usize / 2, rate as usize * 2);
            let rho = reductions(rate, s, &rigs::dc_step(n, up, down, level(-80.0), 1.0));
            let p = libm::exp(-1.0 / (release * rate as f64));
            // The output falls under the knee at once; a sample later the detector hears it.
            for k in down + 2..down + 2000 {
                let q = rho[k + 1] / rho[k];
                assert!((q - p).abs() < 1e-12, "{rate} {release}: {q} against {p}");
            }
            // Time to 63% of the way down, from the first sample that heard the fall.
            let start = rho[down + 1];
            let k = (down + 1..n).find(|&k| rho[k] <= start * libm::exp(-1.0)).unwrap();
            let t = (k - down - 1) as f64 / rate as f64;
            assert!((t - release).abs() <= 1.5 / rate as f64, "{rate} {release}: {t}");
        }
    }
}

/// The attack's 63% time on the DC step is the dial's; on the burst, slower by the duty cycle.
#[test]
fn the_attack_reads_its_dial_on_dc_and_near_it_on_a_burst() {
    let rate = 48_000;
    for attack in [1e-3, 5e-3, 20e-3] {
        let s = Set { attack, release: 1.0, ratio: 4.0, threshold: -24.0, width: 0.0, ..SET };
        let (up, down, n) = (rate as usize / 10, rate as usize * 6 / 10, rate as usize);
        let t63 = |rho: &[f64]| {
            let tail = &rho[down - 480..down];
            let final_ = tail.iter().sum::<f64>() / tail.len() as f64;
            let start = rho[up - 1];
            let k = (up..down).find(|&k| rho[k] >= start + (final_ - start) * (1.0 - libm::exp(-1.0)));
            (k.unwrap() - up) as f64 / rate as f64
        };
        let dc = t63(&reductions(rate, s, &rigs::dc_step(n, up, down, level(-30.0), level(-6.0))));
        assert!((dc - attack).abs() <= 2.0 / rate as f64, "{attack}: on DC {dc}");
        let sine = t63(&reductions(rate, s, &rigs::burst(rate, n, up, down)));
        eprintln!("attack {attack}: DC {dc}, burst {sine}, {:.2}", sine / dc);
        // A peak detector pulls only while the sine is near its crests, so the burst reads the
        // duty cycle as much as the time, 1.5 to 1.7 times the DC figure from 1 to 20 ms
        // (measured 2026-11-12): why timing is pinned on DC. Pinned as found, for the FET.
        assert!((1.4..1.8).contains(&(sine / dc)), "{attack}: on the burst {sine} against {dc}");
    }
}

#[test]
fn the_stepped_sine_reads_a_plain_gain() {
    for rate in RATES {
        let (x, steps) = rigs::stepped_sine(rate, 0.2);
        let mut y = x.clone();
        Plain(level(-6.0)).process(&mut y);
        for (i, o) in rigs::read_steps(&y, rate, &steps) {
            assert!((o - i + 6.0).abs() < 0.001, "{rate} at {i}: {o}");
        }
    }
}

#[test]
fn the_stepped_sine_reads_the_curve() {
    // A peak detector on a sine pulls the reduction toward the crest's only near the crests, and
    // lets go between them: it settles under the static curve's reduction by about
    // (τ_a/τ_r) of it. At 1 ms and 1 s that's within 0.15 dB.
    let rate = 48_000;
    let s = Set { attack: 1e-3, release: 1.0, ..SET };
    let k = Knee { threshold: s.threshold, ratio: s.ratio, width: s.width };
    let (x, steps) = rigs::stepped_sine(rate, 1.0);
    let mut y = x.clone();
    Comp::new(rate, s).process(&mut y);
    for (i, o) in rigs::read_steps(&y, rate, &steps) {
        assert!((o - k.curve(i)).abs() < 0.15, "at {i}: {o} against {}", k.curve(i));
    }
}

#[test]
fn the_same_bits_at_any_block_size() {
    for rate in RATES {
        let n = rate as usize * 2;
        let l = noise(n, 0.5, 1);
        let r = noise(n, 0.3, 2);
        // A setting every 1,024 samples, at the block starts every size shares.
        let apply = |u: &mut Comp, at: usize| {
            if at.is_multiple_of(1024) {
                let i = (at / 1024) as f64;
                u.set(Set {
                    threshold: -40.0 + (i % 7.0) * 4.0,
                    makeup_db: i % 3.0,
                    on: (at / 1024) % 11 != 5,
                    ..SET
                });
            }
        };
        rigs::blocks(|| Comp::new(rate, SET), apply, &l, &r);
    }
}

#[test]
fn every_output_is_finite() {
    for rate in RATES {
        rigs::finite(|| Comp::new(rate, Set { attack: 20e-6, ratio: 20.0, ..SET }), rate);
    }
}

#[test]
fn one_detector_for_both_channels() {
    let x = rigs::burst(48_000, 48_000, 10_000, 30_000);
    rigs::linked(|| Comp::new(48_000, SET), &x);
}

#[test]
fn a_held_key_glides() {
    for rate in RATES {
        let after = |u: &Comp, _: f64| assert!(u.settled(), "settled 50 ms after");
        let steps = |lo: f64, hi: f64, by: f64| {
            let up: Vec<f64> = (0..).map(|i| lo + by * i as f64).take_while(|&v| v <= hi).collect();
            up.iter().chain(up.iter().rev().skip(1)).cloned().collect::<Vec<_>>()
        };
        // Slow enough that the unit's own ripple on the sine stays under −80 dB above 4 kHz:
        // what's measured is the ramp. (At 1 ms the peak detector's ripple alone reads −79.8 at
        // 96 kHz.)
        let slow = Set { attack: 10e-3, release: 0.5, ..SET };
        rigs::held_key(
            || Comp::new(rate, slow),
            |u, v| u.set(Set { threshold: v, ..slow }),
            |u, v| {
                after(u, v);
                assert_eq!(u.threshold.now(), v);
            },
            rate,
            &steps(-40.0, 0.0, 0.5),
            "threshold",
        );
        rigs::held_key(
            || Comp::new(rate, SET),
            |u, v| u.set(Set { makeup_db: v, ..SET }),
            |u, v| {
                after(u, v);
                assert_eq!(u.makeup.now(), level(v));
            },
            rate,
            &steps(0.0, 12.0, 0.5),
            "make-up",
        );
        rigs::held_key(
            || Comp::new(rate, SET),
            |u, v| u.set(Set { on: v > 0.5, ..SET }),
            |u, v| {
                after(u, v);
                assert_eq!(u.on.target(), v > 0.5);
            },
            rate,
            &[1.0, 0.0],
            "in",
        );
    }
}

#[test]
fn a_mean_square_reads_a_sine_at_its_peak_and_rises_by_its_attack() {
    let rate = 48_000;
    // Symmetric and slow, it reads a sine's mean square: −6 dBFS reads −6. (Branching, with a
    // fast attack, it rides the square's crests and reads up to 3 dB more: the de-esser's, by its
    // spec.)
    let mut m = MeanSquare::new(rate as f64, 0.05, 0.05);
    assert_eq!(m.db(), -300.0);
    let x = rigs::burst(rate, rate as usize, 0, rate as usize);
    for v in &x {
        m.feed(v * v);
    }
    assert!((m.db() + 6.0).abs() < 0.1, "{}", m.db());
    let mut m = MeanSquare::new(rate as f64, 0.5e-3, 40e-3);
    let k = (0..).find(|_| {
        m.feed(1.0);
        m.ms() >= 1.0 - libm::exp(-1.0)
    });
    assert_eq!(k.unwrap() + 1, 24, "0.5 ms at 48 kHz");
    // It falls to exactly 0, never denormal.
    for _ in 0..rate * 20 {
        m.feed(0.0);
    }
    assert_eq!(m.ms(), 0.0);
    assert_eq!(peak_db(0.0), -300.0);
    assert_eq!(peak_db(-1.0), 0.0);
}

#[test]
fn a_fade_runs_to_its_end_and_the_latest_waits() {
    let mut f = Fade::new(0u8, 4);
    assert!(f.done() && !f.fading());
    assert_eq!(f.advance(), (0, 0, 1.0));
    f.set(1);
    f.set(2); // before a sample ran: replaces 1
    assert_eq!(f.advance(), (0, 2, 0.25));
    f.set(3);
    f.set(4); // waits, replacing 3
    assert_eq!(f.target(), 4);
    assert_eq!([f.advance(), f.advance()], [(0, 2, 0.5), (0, 2, 0.75)]);
    assert_eq!(f.advance(), (2, 2, 1.0));
    assert_eq!(f.advance(), (2, 4, 0.25));
    f.set(4); // where it's going: nothing waits
    for _ in 0..3 {
        f.advance();
    }
    assert!(f.done());
    assert_eq!(f.advance(), (4, 4, 1.0));
    // Sent back to where it was before a sample ran: no fade.
    f.set(1);
    f.set(4);
    assert!(f.done());
    f.jump(7);
    assert_eq!((f.target(), f.advance()), (7, (7, 7, 1.0)));
}

#[test]
fn a_tail_to_silence_takes_no_longer() {
    let rate = 48_000;
    let mut u = Comp::new(rate, Set { release: 0.2, ..SET });
    rigs::tail(&mut u, &noise(rate as usize, 1.0, 9), rate);
    assert_eq!(u.follower.rho(), 0.0, "released to exactly 0");
}
