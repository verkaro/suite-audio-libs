//! The test rigs, written once for the three units (`docs/spec.md`, "The rigs, written once", and
//! `specs/shared.md`, "The test rigs every unit uses").

use as_envelope::measure::{spectrum, tone};
use as_envelope::signal::{faded, noise, sine, torture};

/// What a rig drives: any unit, one channel or two, with its reduction read after a block.
pub trait Unit {
    fn process(&mut self, x: &mut [f64]);
    fn process2(&mut self, l: &mut [f64], r: &mut [f64]);
    /// The reduction at the last block's end, in dB.
    fn gr_db(&self) -> f64;
}

pub fn level(db: f64) -> f64 {
    libm::pow(10.0, db / 20.0)
}

pub fn db(v: f64) -> f64 {
    20.0 * libm::log10(v.max(1e-15))
}

/// One level of the stepped sine: its input in dBFS and where to read it.
#[derive(Clone, Copy, Debug)]
pub struct Step {
    pub db: f64,
    pub from: usize,
    pub len: usize,
}

/// 1 kHz from −60 to 0 dBFS in 1 dB steps, each held `hold` seconds (at least 0.1), one
/// continuous sine; each read over its last 100 ms.
pub fn stepped_sine(rate: u32, hold: f64) -> (Vec<f64>, Vec<Step>) {
    let (h, len) = ((hold * rate as f64) as usize, rate as usize / 10);
    assert!(h >= len);
    let w = core::f64::consts::TAU * 1000.0 / rate as f64;
    let mut x = Vec::with_capacity(61 * h);
    let mut steps = Vec::new();
    for i in 0..=60 {
        let d = -60.0 + i as f64;
        let a = level(d);
        steps.push(Step { db: d, from: x.len() + h - len, len });
        for _ in 0..h {
            x.push(a * libm::sin(w * x.len() as f64));
        }
    }
    (x, steps)
}

/// Each step's output level in dBFS, sine-referenced, by its fundamental.
pub fn read_steps(y: &[f64], rate: u32, steps: &[Step]) -> Vec<(f64, f64)> {
    steps.iter().map(|s| (s.db, db(tone(y, 1000.0, rate, s.from, s.len).0))).collect()
}

/// A constant `lo` until `up`, `hi` until `down`, then `lo`, `n` samples in all.
pub fn dc_step(n: usize, up: usize, down: usize, lo: f64, hi: f64) -> Vec<f64> {
    (0..n).map(|k| if k >= up && k < down { hi } else { lo }).collect()
}

/// 1 kHz at −30 dBFS, −6 from `up` to `down`, then −30 again: the RFCs' burst.
pub fn burst(rate: u32, n: usize, up: usize, down: usize) -> Vec<f64> {
    let w = core::f64::consts::TAU * 1000.0 / rate as f64;
    (0..n)
        .map(|k| {
            let a = if k >= up && k < down { level(-6.0) } else { level(-30.0) };
            a * libm::sin(w * k as f64)
        })
        .collect()
}

fn run<U: Unit>(u: &mut U, l: &mut [f64], r: Option<&mut [f64]>, block: usize) {
    match r {
        None => l.chunks_mut(block).for_each(|b| u.process(b)),
        Some(r) => {
            for (a, b) in l.chunks_mut(block).zip(r.chunks_mut(block)) {
                u.process2(a, b);
            }
        }
    }
}

fn same(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits())
}

/// The same input through blocks of 1, 64 and 1,024, one channel and two, bit for bit equal; and
/// a run repeated on a fresh unit, bit for bit equal. `apply` is called before each block with
/// the block's first sample's position, so a unit can take settings at multiples of 1,024.
pub fn blocks<U: Unit>(make: impl Fn() -> U, apply: impl Fn(&mut U, usize), l: &[f64], r: &[f64]) {
    let go = |block: usize, two: bool| {
        let mut u = make();
        let (mut a, mut b) = (l.to_vec(), r.to_vec());
        let mut at = 0;
        while at < a.len() {
            let end = (at + block).min(a.len());
            apply(&mut u, at);
            if two {
                u.process2(&mut a[at..end], &mut b[at..end]);
            } else {
                u.process(&mut a[at..end]);
            }
            at = end;
        }
        (a, b)
    };
    for two in [false, true] {
        let first = go(1024, two);
        let again = go(1024, two);
        assert!(same(&first.0, &again.0) && same(&first.1, &again.1), "twice, two {two}");
        for block in [1, 64] {
            let (a, b) = go(block, two);
            assert!(same(&first.0, &a) && same(&first.1, &b), "block {block}, two {two}");
        }
    }
}

/// The torture set, silence, DC at ±1, full-scale noise and a −300 dBFS tail: every output finite,
/// one channel and two.
pub fn finite<U: Unit>(make: impl Fn() -> U, rate: u32) {
    let n = rate as usize;
    let mut signals = torture(rate, 1.0);
    signals.push(("silence", vec![0.0; n]));
    signals.push(("DC +1", vec![1.0; n]));
    signals.push(("DC −1", vec![-1.0; n]));
    signals.push(("full-scale noise", noise(n, 1.0, 9)));
    signals.push(("−300 dBFS", noise(n, 1e-15, 4)));
    for (name, x) in signals {
        let mut y = x.clone();
        run(&mut make(), &mut y, None, 256);
        assert!(y.iter().all(|v| v.is_finite()), "{name}");
        let (mut a, mut b) = (x.clone(), x.iter().map(|v| -0.5 * v).collect::<Vec<_>>());
        run(&mut make(), &mut a, Some(&mut b), 256);
        assert!(a.iter().chain(&b).all(|v| v.is_finite()), "{name}, two channels");
    }
}

/// A source in the left alone, the right alone, then both: one detector for both channels, so the
/// reduction is the same sample by sample in each run, the left-only run's left is the right-only
/// run's right, and the two-channel run's channels are equal.
pub fn linked<U: Unit>(make: impl Fn() -> U, x: &[f64]) {
    let n = x.len();
    let one_by_one = |l: &mut [f64], r: &mut [f64]| {
        let mut u = make();
        let mut gr = Vec::with_capacity(n);
        for k in 0..n {
            u.process2(&mut l[k..k + 1], &mut r[k..k + 1]);
            gr.push(u.gr_db());
        }
        gr
    };
    let (mut l1, mut r1) = (x.to_vec(), vec![0.0; n]);
    let g1 = one_by_one(&mut l1, &mut r1);
    let (mut l2, mut r2) = (vec![0.0; n], x.to_vec());
    let g2 = one_by_one(&mut l2, &mut r2);
    let (mut l3, mut r3) = (x.to_vec(), x.to_vec());
    let g3 = one_by_one(&mut l3, &mut r3);
    assert!(same(&g1, &g2), "left alone and right alone reduce alike");
    assert!(same(&l1, &r2), "left alone and right alone come out alike");
    assert!(same(&l3, &r3), "both channels alike");
    assert!(g1.iter().any(|&g| g > 0.0), "the source was reduced");
    assert!(g3.iter().zip(&g1).all(|(a, b)| a == b), "both: the same detector reading");
}

/// The largest sample-to-sample difference from `from` on.
pub fn largest_step(y: &[f64], from: usize) -> f64 {
    y[from..].windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f64::max)
}

/// The loudest bin above 4 kHz in 4,096-sample Hann windows hopping by 1,024, in dBFS.
pub fn loudest_above_4k(y: &[f64], from: usize, rate: u32) -> f64 {
    let n = 4096;
    let hann: Vec<f64> =
        (0..n).map(|k| 0.5 - 0.5 * libm::cos(core::f64::consts::TAU * k as f64 / n as f64)).collect();
    // Scaled so a sine's bin reads its peak: sine-referenced.
    let norm = n as f64 / hann.iter().sum::<f64>();
    let lo = libm::ceil(4000.0 * n as f64 / rate as f64) as usize;
    let mut worst = -300.0f64;
    let mut at = from;
    while at + n <= y.len() {
        let w: Vec<f64> = y[at..at + n].iter().zip(&hann).map(|(v, h)| v * h).collect();
        worst = spectrum(&w)[lo..].iter().map(|&m| db(m * norm)).fold(worst, f64::max);
        at += 1024;
    }
    worst
}

/// `specs/shared.md`'s held key: a 1 kHz sine at −12 dBFS, faded in, `set` with each value of
/// `path` in turn every 2 ms for a second, after a quarter second at its start. No step larger
/// than 1.1× the largest through the unit held at either end of the path, and nothing above 4 kHz
/// louder than −80 dBFS. `after` is handed the unit 50 ms after the last setting, for the unit's
/// own check that the last setting holds.
pub fn held_key<U: Unit>(
    make: impl Fn() -> U,
    set: impl Fn(&mut U, f64),
    after: impl Fn(&U, f64),
    rate: u32,
    path: &[f64],
    what: &str,
) {
    let r = rate as usize;
    let (pre, hop, changes) = (r / 4, (0.002 * rate as f64) as usize, 500);
    let total = pre + hop * changes + r / 2;
    let x = faded(sine(1000.0, level(-12.0), total, rate), r / 50);
    let held = |v: f64| {
        let mut u = make();
        set(&mut u, v);
        let mut y = x.clone();
        run(&mut u, &mut y, None, 1024);
        y
    };
    let mut u = make();
    set(&mut u, path[0]);
    let mut y = x.clone();
    u.process(&mut y[..pre]);
    let mut last = path[0];
    for i in 0..changes {
        last = path[i % path.len()];
        set(&mut u, last);
        let at = pre + i * hop;
        u.process(&mut y[at..at + hop]);
    }
    let end = pre + changes * hop;
    let then = end + (0.05 * rate as f64) as usize;
    u.process(&mut y[end..then]);
    after(&u, last);
    u.process(&mut y[then..]);

    let from = r / 10;
    let lo = path.iter().cloned().fold(f64::MAX, f64::min);
    let hi = path.iter().cloned().fold(f64::MIN, f64::max);
    let bound = 1.1 * largest_step(&held(lo), from).max(largest_step(&held(hi), from));
    let step = largest_step(&y, from);
    assert!(step <= bound, "{what} at {rate}: a step of {step:e} against {bound:e}");
    let burst = loudest_above_4k(&y, from, rate);
    assert!(burst < -80.0, "{what} at {rate}: {burst:.1} dB above 4 kHz");
}
