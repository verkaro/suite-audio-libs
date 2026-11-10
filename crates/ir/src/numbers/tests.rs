//! The numbers' tests, with synthetic IRs only, made to have the numbers they're described by
//! (from dexter-place's `library/src/tests.rs`).

use std::f64::consts::PI;

use super::*;

/// Gaussian numbers from a seed, the same every run: xorshift and Box–Muller.
struct Rng(u64);

impl Rng {
    fn uniform(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        ((self.0 >> 11) as f64 + 0.5) / (1u64 << 53) as f64
    }

    fn gauss(&mut self) -> f64 {
        let (a, b) = (self.uniform(), self.uniform());
        (-2.0 * a.ln()).sqrt() * (2.0 * PI * b).cos()
    }
}

/// Gaussian noise falling 60 dB in `rt` seconds, `secs` long, over a floor `floor` dB under its
/// start.
fn room(seed: u64, rate: u32, rt: f64, secs: f64, floor: f64) -> Vec<f64> {
    let mut rng = Rng(seed);
    let n = (secs * rate as f64) as usize;
    let f = 10f64.powf(floor / 20.0);
    (0..n)
        .map(|k| 0.5 * (rng.gauss() * 10f64.powf(-3.0 * k as f64 / rt / rate as f64) + f * rng.gauss()))
        .collect()
}

fn f32s(x: &[f64]) -> Vec<f32> {
    x.iter().map(|v| *v as f32).collect()
}

fn near(got: f64, want: f64, share: f64, what: &str) {
    assert!((got - want).abs() <= want.abs() * share, "{what}: {got}, wanted {want} within {share}");
}

/// The numbers of `draws` rooms alike but for their noise, each from its own seed. A room's
/// numbers swing from one draw of noise to the next as a real room's do from one measurement to
/// the next (an octave over the first 80 ms is only some thirty degrees of freedom), so what's
/// held to the sum on paper is their mean.
fn draws(seed: u64, draws: u64, make: impl Fn(u64) -> Vec<f64>, rate: u32) -> Vec<Numbers> {
    (0..draws).map(|s| describe(&[f32s(&make(seed + s))], rate)).collect()
}

fn mean(ns: &[Numbers], f: impl Fn(&Numbers) -> f64) -> f64 {
    ns.iter().map(f).sum::<f64>() / ns.len() as f64
}

#[test]
fn a_rooms_rt_is_found_through_its_noise_floor() {
    let rate = 48000;
    for (rt, floor) in [(0.4, -80.0), (1.2, -70.0), (2.5, -65.0)] {
        let ns = draws(7, 12, |s| room(s, rate, rt, rt * 1.6 + 0.5, floor), rate);
        assert!(ns.iter().all(|n| n.rt.is_some_and(|r| r.over == 30)), "T30 at {rt}");
        near(mean(&ns, |n| n.rt.unwrap().secs), rt, 0.03, &format!("RT of {rt}"));
        near(mean(&ns, |n| n.edt.unwrap()), rt, 0.05, &format!("EDT of {rt}"));
        for (b, hz) in BANDS.iter().enumerate() {
            near(mean(&ns, |n| n.bands[b].rt.unwrap().secs), rt, 0.04, &format!("RT of {rt} at {hz} Hz"));
        }
        near(mean(&ns, |n| n.range), -floor, 0.1, "range");
    }
}

#[test]
fn decay_by_band_is_told_apart() {
    // A room ringing longer in the lows: each band's own noise, decaying at its own rate, split
    // by the bands' own weights.
    let rate = 48000;
    let rts = [2.0, 1.8, 1.6, 1.4, 1.2, 1.0, 0.8];
    let secs = 3.5;
    let make = |seed: u64| {
        let n = (secs * rate as f64) as usize;
        let mut sum = vec![0.0f64; n];
        for (b, rt) in rts.iter().enumerate() {
            let x = room(seed * 16 + b as u64, rate, *rt, secs, -90.0);
            for (s, v) in sum.iter_mut().zip(band_pass(&x, rate, BANDS[b])) {
                *s += v;
            }
        }
        sum
    };
    let ns = draws(1, 6, make, rate);
    for (b, rt) in rts.iter().enumerate() {
        // Neighbours leak into each band through its skirts, pulling it a little toward theirs.
        near(mean(&ns, |n| n.bands[b].rt.unwrap().secs), *rt, 0.05, &format!("{} Hz", BANDS[b]));
    }
    near(mean(&ns, |n| n.br.unwrap()), 3.8 / 3.0, 0.05, "warm");
    near(mean(&ns, |n| n.brilliance.unwrap()), 2.2 / 3.0, 0.05, "brilliance");
}

/// `x` through the octave band at `hz`, the way the numbers split it.
fn band_pass(x: &[f64], rate: u32, hz: f64) -> Vec<f64> {
    let len = 2 * x.len();
    let mut p = realfft::RealFftPlanner::<f64>::new();
    let (fwd, inv) = (p.plan_fft_forward(len), p.plan_fft_inverse(len));
    let mut t = vec![0.0; len];
    t[..x.len()].copy_from_slice(x);
    let mut f = fwd.make_output_vec();
    fwd.process(&mut t, &mut f).unwrap();
    for (k, v) in f.iter_mut().enumerate() {
        let fk = k as f64 * rate as f64 / len as f64;
        *v *= weight(fk, hz) / len as f64;
    }
    f[0].im = 0.0;
    f.last_mut().unwrap().im = 0.0;
    inv.process(&mut f, &mut t).unwrap();
    t.truncate(x.len());
    t
}

#[test]
fn clarity_and_centre_time_of_a_plain_decay() {
    let (rate, rt) = (48000, 1.0);
    let ns = draws(40, 24, |s| room(s, rate, rt, 2.5, -100.0), rate);
    // Energy falls as e^(−kt), k = 6 ln 10 / RT.
    let k = 6.0 * 10f64.ln() / rt;
    let c = |t: f64| 10.0 * ((1.0 - (-k * t).exp()) / (-k * t).exp()).log10();
    let (c50, c80) = (mean(&ns, |n| n.c50), mean(&ns, |n| n.c80));
    assert!((c50 - c(0.050)).abs() < 0.3, "C50 {c50} wanted {}", c(0.050));
    assert!((c80 - c(0.080)).abs() < 0.3, "C80 {c80} wanted {}", c(0.080));
    near(mean(&ns, |n| n.ts), 1000.0 / k, 0.02, "Ts");
    assert!(ns.iter().all(|n| n.pre < 1.0), "no leading silence");
}

#[test]
fn a_shallow_ir_says_so_rather_than_guess() {
    let rate = 48000;
    let t20 = describe(&[f32s(&room(5, rate, 1.0, 2.0, -40.0))], rate);
    assert_eq!(t20.rt.map(|r| r.over), Some(20), "40 dB of range: T20 only");
    near(t20.rt.unwrap().secs, 1.0, 0.08, "T20");
    let none = describe(&[f32s(&room(5, rate, 1.0, 2.0, -25.0))], rate);
    assert_eq!(none.rt, None, "25 dB of range: no RT");
}

#[test]
fn leading_silence_is_measured_and_left_out_of_the_rest() {
    let rate = 48000;
    let mut ir = vec![0.0; 2400]; // 50 ms
    ir.extend(room(9, rate, 0.8, 2.0, -90.0));
    let n = describe(&[f32s(&ir)], rate);
    assert!((n.pre - 50.0).abs() < 0.5, "pre {}", n.pre);
    near(n.rt.unwrap().secs, 0.8, 0.05, "RT");
}

#[test]
fn where_the_tail_starts() {
    let rate = 48000;
    // A few reflections, then a dense tail from 60 ms.
    let mut ir = vec![0.0f64; (2.0 * rate as f64) as usize];
    for (ms, g) in
        [(0.0, 1.0), (7.0, 0.6), (13.0, -0.5), (21.0, 0.45), (29.0, -0.4), (38.0, 0.3), (47.0, 0.3)]
    {
        ir[(ms / 1000.0 * rate as f64) as usize] = g;
    }
    let start = (0.060 * rate as f64) as usize;
    for (k, v) in room(13, rate, 1.0, 2.0, -90.0).iter().enumerate().take(ir.len() - start) {
        ir[start + k] += 0.2 * v * 10f64.powf(-3.0 * start as f64 / rate as f64);
    }
    let n = describe(&[f32s(&ir)], rate);
    let mix = n.mix.expect("a tail");
    assert!((55.0..90.0).contains(&mix), "mix {mix}");
    // A room that's all tail from the start.
    let n = describe(&[f32s(&room(17, rate, 1.0, 2.0, -90.0))], rate);
    // The earliest it can say: half the window and half the smoothing.
    assert_eq!(n.mix, Some(15.0), "all tail");
}

#[test]
fn width_is_the_sides_correlation() {
    let rate = 48000;
    let a = f32s(&room(21, rate, 1.0, 2.0, -90.0));
    let b = f32s(&room(22, rate, 1.0, 2.0, -90.0));
    let same = describe(&[a.clone(), a.clone()], rate);
    assert!(same.corr > 0.999, "{}", same.corr);
    let wide = describe(&[a.clone(), b], rate);
    assert!(wide.corr.abs() < 0.05, "{}", wide.corr);
    assert_eq!(describe(&[a], rate).corr, 1.0);
}

#[test]
fn an_ir_too_short_or_silent_says_nothing_rather_than_fail() {
    for ir in [vec![1.0f32], vec![0.0; 4800], vec![0.5; 3]] {
        let n = describe(std::slice::from_ref(&ir), 48000);
        assert_eq!(n.rt, None, "{ir:?}");
        assert_eq!(n.mix, None);
        let n = describe(&[ir.clone(), ir], 44100);
        assert!(n.edt.is_none() && n.bands.iter().all(|b| b.rt.is_none()));
    }
}
