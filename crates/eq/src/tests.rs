//! `docs/spec.md`, "Pinned by tests", and `specs/shared.md`'s rigs.

use super::*;
use as_envelope::measure::{null, peak, spectrum, tone};
use as_envelope::signal::{faded, noise, sine, torture};

const RATES: [u32; 3] = [44_100, 48_000, 96_000];

fn ch() -> Settings {
    Settings::default()
}

fn with(pairs: &[(&str, f64)]) -> Settings {
    let mut s = ch();
    for &(id, v) in pairs {
        assert!(s.set_by_id(id, v), "{id}");
    }
    s
}

fn band(id: &str) -> usize {
    Layout::CHANNEL.bands.iter().position(|d| d.id == id).unwrap()
}

/// `x` through a fresh unit set to `s`, in blocks of `block`.
fn through(s: &Settings, rate: u32, x: &[f64], block: usize) -> Vec<f64> {
    let mut eq = Eq::new(rate as f64);
    eq.set(s);
    let mut y = x.to_vec();
    for b in y.chunks_mut(block) {
        eq.process(b);
    }
    y
}

/// `n` frequencies from `lo` to `hi`, evenly on a log scale, each a whole number of 5 Hz so 0.2 s
/// holds whole cycles.
fn across(lo: f64, hi: f64, n: usize) -> Vec<f64> {
    (0..n)
        .map(|k| 5.0 * libm::round(lo * libm::pow(hi / lo, k as f64 / (n - 1) as f64) / 5.0))
        .map(|f| f.clamp(lo, hi))
        .collect()
}

/// A sine's gain through the unit, in dB: settled for 0.5 s, read over 0.2 s.
fn sine_db(s: &Settings, rate: u32, hz: f64) -> f64 {
    let (settle, len) = (rate as usize / 2, rate as usize / 5);
    let y = through(s, rate, &sine(hz, 0.5, settle + len, rate), 1024);
    20.0 * libm::log10(tone(&y, hz, rate, settle, len).0 / 0.5)
}

/// The steady gain of a constant (`nyquist` false) or of ±1 alternating (true), in dB.
fn edge_db(s: &Settings, rate: u32, nyquist: bool) -> f64 {
    let n = rate as usize;
    let x: Vec<f64> = (0..n).map(|k| if nyquist && k % 2 == 1 { -0.5 } else { 0.5 }).collect();
    let y = through(s, rate, &x, 1024);
    20.0 * libm::log10(y[n - 1000..].iter().fold(0.0f64, |m, v| m.max(v.abs())) / 0.5)
}

#[test]
fn the_controls_pass_the_check() {
    as_control::check(controls()).unwrap();
    // Every control reads and sets by its id, and only those.
    let mut s = ch();
    for c in controls() {
        assert_eq!(s.get(c.id), Some(c.default), "{}", c.id);
        assert!(s.set_by_id(c.id, c.default));
    }
    assert_eq!(s.get("lf.q"), None, "the LF has no Q to set");
    assert!(!s.set_by_id("lf.q", 2.0));
    assert!(!s.set_by_id("nope", 1.0));
}

#[test]
fn settings_are_held_to_their_ranges() {
    let mut s = ch();
    s.set_by_id("lmf.freq", 10.0);
    s.set_by_id("hf.gain", 40.0);
    s.set_by_id("hpf.slope", 7.0);
    s.set_by_id("trim", f64::NAN);
    assert_eq!(s.get("lmf.freq"), Some(200.0));
    assert_eq!(s.get("hf.gain"), Some(15.0));
    assert_eq!(s.get("hpf.slope"), Some(1.0));
    assert_eq!(s.get("trim"), Some(0.0), "not a number is the default");
    // A field set by hand is held when the unit takes it.
    let mut t = ch();
    t.bands[band("lmf")].q = 99.0;
    t.trim = -40.0;
    let mut eq = Eq::new(48_000.0);
    eq.set(&t);
    eq.process(&mut [0.0; 1024]);
    assert_eq!(eq.settings().get("lmf.q"), Some(4.0));
    assert_eq!(eq.settings().get("trim"), Some(-12.0));
}

#[test]
fn every_band_reads_its_dial() {
    for rate in RATES {
        let r = rate as f64;
        let mut worst: f64 = 0.0;
        let mut check = |what: &str, got: f64, want: f64, tol: f64| {
            worst = worst.max((got - want).abs());
            assert!((got - want).abs() < tol, "{what} at {rate}: {got} against {want}");
        };
        for g in [-15.0, -9.0, -3.0, 3.0, 9.0, 15.0] {
            // Bells at their centres: the mids, and the low and high bands switched to bells.
            for (id, lo, hi, bell) in [
                ("lmf", 200.0, 2500.0, false),
                ("hmf", 600.0, 7000.0, false),
                ("lf", 30.0, 450.0, true),
                ("hf", 1500.0, 16000.0, true),
            ] {
                for f in across(lo, hi, 5) {
                    let mut s = with(&[(&format!("{id}.freq"), f), (&format!("{id}.gain"), g)]);
                    if bell {
                        s.set_by_id(&format!("{id}.bell"), 1.0);
                    }
                    let what = format!("{id} bell {f} Hz {g} dB");
                    check(&what, response(&s, r, f), g, 0.1);
                    check(&what, sine_db(&s, rate, f), g, 0.1);
                }
            }
            // The low shelf: the dial at DC, half of it at the corner.
            for f in across(30.0, 450.0, 5) {
                let s = with(&[("lf.freq", f), ("lf.gain", g)]);
                let what = format!("lf shelf {f} Hz {g} dB");
                check(&what, response(&s, r, 1e-6), g, 0.1);
                check(&what, edge_db(&s, rate, false), g, 0.1);
                check(&what, response(&s, r, f), g / 2.0, 0.1);
                check(&what, sine_db(&s, rate, f), g / 2.0, 0.1);
            }
            // The high shelf: half the dial at the corner, where the corner is at or below an
            // eighth of the rate; at Nyquist, the analog shelf there, and the dial itself where the
            // corner is at or below a twentieth (the analog shelf is still 0.13 dB over its plateau
            // at 8 times its corner, at ±15 dB).
            for f in across(1500.0, 16000.0, 5).into_iter().filter(|&f| f <= r / 8.0) {
                let s = with(&[("hf.freq", f), ("hf.gain", g)]);
                let what = format!("hf shelf {f} Hz {g} dB");
                let nyquist = analog_high_shelf_db(f, g, r / 2.0);
                check(&what, response(&s, r, r / 2.0), nyquist, 1e-6);
                check(&what, edge_db(&s, rate, true), nyquist, 1e-6);
                if f <= r / 20.0 {
                    check(&what, edge_db(&s, rate, true), g, 0.1);
                }
                check(&what, response(&s, r, f), g / 2.0, 0.25);
                check(&what, sine_db(&s, rate, f), g / 2.0, 0.25);
            }
        }
        // The high-pass, −3 dB at its corner, both slopes.
        for slope in [0.0, 1.0] {
            for f in across(20.0, 400.0, 5) {
                let s = with(&[("hpf.in", 1.0), ("hpf.freq", f), ("hpf.slope", slope)]);
                let what = format!("hpf {f} Hz slope {slope}");
                check(&what, response(&s, r, f), -3.0103, 0.1);
                check(&what, sine_db(&s, rate, f), -3.0103, 0.1);
            }
        }
        eprintln!("{rate}: worst reading {worst:.4} dB");
    }
}

fn analog_high_shelf_db(f0: f64, g: f64, hz: f64) -> f64 {
    let a = libm::pow(10.0, g.abs() / 40.0);
    let (x, r) = (hz / f0, a.sqrt() / SHELF_Q);
    let num = (1.0 - a * x * x) * (1.0 - a * x * x) + r * r * x * x;
    let den = (a - x * x) * (a - x * x) + r * r * x * x;
    let d = 20.0 * libm::log10(a) + 10.0 * libm::log10(num / den);
    if g < 0.0 { -d } else { d }
}

fn analog_bell_db(f0: f64, q: f64, g: f64, hz: f64) -> f64 {
    let a = libm::pow(10.0, g.abs() / 40.0);
    let x = hz / f0;
    let sq = |c1: f64| (1.0 - x * x) * (1.0 - x * x) + c1 * c1 * x * x;
    let d = 10.0 * libm::log10(sq(a / q) / sq(1.0 / (a * q)));
    if g < 0.0 { -d } else { d }
}

#[test]
fn the_hf_bell_at_16_khz_keeps_its_shape() {
    let r = 44_100.0;
    for g in [-15.0, 15.0] {
        let s = with(&[("hf.freq", 16_000.0), ("hf.gain", g), ("hf.bell", 1.0)]);
        let mut worst: f64 = 0.0;
        for k in 0..=400 {
            let hz = 20.0 * libm::pow(1000.0, k as f64 / 400.0);
            worst =
                worst.max((response(&s, r, hz) - analog_bell_db(16_000.0, q_eff(BELL_Q, g), g, hz)).abs());
        }
        eprintln!("16 kHz bell at {g} dB: worst {worst:.3} dB");
        assert!(worst < 1.8, "{worst}");
    }
}

#[test]
fn flat_is_a_null() {
    let x = noise(48_000, 1.0, 3);
    for s in [
        ch(),
        with(&[("lf.in", 0.0), ("lmf.in", 0.0), ("hmf.in", 0.0), ("hf.in", 0.0)]),
        with(&[("lf.bell", 1.0), ("lmf.freq", 2000.0), ("hmf.q", 4.0), ("hf.freq", 2000.0)]),
        // Out, whatever the bands and trim say.
        with(&[("in", 0.0), ("lmf.gain", 9.0), ("hpf.in", 1.0), ("trim", 6.0)]),
    ] {
        for rate in RATES {
            let y = through(&s, rate, &x, 1024);
            assert!(x.iter().zip(&y).all(|(a, b)| a.to_bits() == b.to_bits()), "{s:?}");
        }
    }
}

#[test]
fn the_whole_is_the_product_of_its_bands() {
    let mut rng = as_envelope::signal::Rng::new(7);
    let mut u = || 0.5 + 0.5 * rng.uniform();
    for _ in 0..200 {
        let mut s = ch();
        for c in controls() {
            let v = match c.kind {
                Kind::Value { lo, hi, .. } => lo + (hi - lo) * u(),
                Kind::Switch { .. } => libm::round(u()),
            };
            s.set_by_id(c.id, v);
        }
        s.on = true;
        for rate in RATES {
            for k in 0..50 {
                let hz = (20.0 * libm::pow(1000.0, k as f64 / 50.0)).min(0.49 * rate as f64);
                let sum: f64 = (0..5).map(|b| band_response(&s, b, rate as f64, hz)).sum::<f64>() + s.trim;
                assert!((response(&s, rate as f64, hz) - sum).abs() < 1e-9);
            }
        }
    }
}

#[test]
fn boost_and_cut_are_inverses() {
    let x = as_envelope::signal::impulse(48_000, 10, 1.0);
    let pairs = |id: &str, f: f64, extra: &[(&str, f64)], g: f64| {
        let mut s = with(&[(&format!("{id}.freq"), f), (&format!("{id}.gain"), g)]);
        for &(i, v) in extra {
            s.set_by_id(i, v);
        }
        s
    };
    for rate in RATES {
        let mut cases = vec![];
        for q in [0.4, 0.7, 1.0, 2.0, 4.0] {
            cases.push(("lmf", 400.0, vec![("lmf.q", q)]));
            cases.push(("hmf", 3000.0, vec![("hmf.q", q)]));
        }
        for (id, fs) in [("lf", [30.0, 100.0, 450.0]), ("hf", [1500.0, 5000.0, 16000.0])] {
            for f in fs {
                cases.push((id, f, vec![]));
                cases.push((id, f, vec![(if id == "lf" { "lf.bell" } else { "hf.bell" }, 1.0)]));
            }
        }
        for (id, f, extra) in cases {
            let up = through(&pairs(id, f, &extra, 9.0), rate, &x, 1024);
            let back = through(&pairs(id, f, &extra, -9.0), rate, &up, 1024);
            let n = null(&x, &back);
            assert!(n < 1e-9, "{id} {f} {extra:?} at {rate}: {n:e}");
        }
    }
}

/// Where a bell's response falls to `half` (in size), searched from `from`, its centre, toward `to`.
fn half_point(s: &Settings, r: f64, half: f64, from: f64, to: f64) -> f64 {
    let (mut a, mut b) = (from, to);
    for _ in 0..80 {
        let m = (a * b).sqrt();
        if response(s, r, m).abs() > half.abs() {
            a = m;
        } else {
            b = m;
        }
    }
    (a * b).sqrt()
}

#[test]
fn a_bells_width_follows_its_gain() {
    let r = 48_000.0;
    for q in [0.5, 1.0, 4.0] {
        for g in [-15.0, -9.0, -3.0, 3.0, 9.0, 15.0] {
            let s = with(&[("lmf.freq", 1000.0), ("lmf.gain", g), ("lmf.q", q)]);
            let lo = half_point(&s, r, g / 2.0, 1000.0, 10.0);
            let hi = half_point(&s, r, g / 2.0, 1000.0, 23_000.0);
            let bw = libm::log2(hi / lo);
            let want = 2.0 * libm::asinh(1.0 / (2.0 * q_eff(q, g))) / std::f64::consts::LN_2;
            assert!((bw / want - 1.0).abs() < 0.05, "Q {q} {g} dB: {bw} octaves against {want}");
        }
    }
}

#[test]
fn the_shelf_bumps_a_little() {
    let r = 48_000.0;
    for (g, lo, hi) in [(15.0, 0.8, 1.0), (-15.0, 0.8, 1.0), (3.0, 0.18, 0.28), (-3.0, 0.18, 0.28)] {
        let s = with(&[("lf.freq", 100.0), ("lf.gain", g)]);
        let (mut top, mut bottom) = (f64::MIN, f64::MAX);
        for k in 0..=4000 {
            let v = response(&s, r, 5.0 * libm::pow(4000.0, k as f64 / 4000.0));
            (top, bottom) = (top.max(v), bottom.min(v));
        }
        // Past the plateau, and past 0 dB on the other side.
        let (over, under) = if g > 0.0 { (top - g, -bottom) } else { (g - bottom, top) };
        eprintln!("{g} dB: bump {over:.3}, dip {under:.3}");
        assert!(over > lo && over < hi, "{g}: bump {over}");
        assert!(under > lo && under < hi, "{g}: dip {under}");
    }
}

#[test]
fn the_mids_overlap() {
    for rate in RATES {
        let s = with(&[("lmf.freq", 1000.0), ("lmf.gain", 6.0), ("hmf.freq", 1000.0), ("hmf.gain", 6.0)]);
        assert!((response(&s, rate as f64, 1000.0) - 12.0).abs() < 0.1);
        assert!((sine_db(&s, rate, 1000.0) - 12.0).abs() < 0.1);
    }
}

#[test]
fn the_marker_sits_where_the_band_is_set() {
    let s = with(&[
        ("lf.gain", 6.0),
        ("lmf.gain", -4.0),
        ("hf.gain", 3.0),
        ("hf.bell", 1.0),
        ("hpf.freq", 120.0),
    ]);
    assert_eq!(marker(&s, band("hpf")), (120.0, -3.0));
    assert_eq!(marker(&s, band("lf")), (100.0, 3.0));
    assert_eq!(marker(&s, band("lmf")), (400.0, -4.0));
    assert_eq!(marker(&s, band("hf")), (10_000.0, 3.0));
}

#[test]
fn the_readouts_read_the_block() {
    let mut eq = Eq::new(48_000.0);
    eq.set(&with(&[("trim", -6.0)]));
    let mut x = sine(1000.0, 0.5, 4800, 48_000);
    eq.process(&mut x);
    let mut x = sine(1000.0, 0.5, 4800, 48_000);
    eq.process(&mut x);
    let r = eq.readouts();
    assert!((r.in_peak_db + 6.02).abs() < 0.01, "{r:?}");
    assert!((r.out_peak_db + 12.02).abs() < 0.01, "{r:?}");
    assert_eq!(r.gr_db, 0.0);
    eq.process(&mut []);
    assert_eq!(eq.readouts().in_peak_db, -300.0);
}

// --- The rigs (`specs/shared.md`, "The test rigs every unit uses").

/// The largest sample-to-sample difference in `y[from..]`.
fn largest_step(y: &[f64], from: usize) -> f64 {
    y[from..].windows(2).fold(0.0f64, |m, w| m.max((w[1] - w[0]).abs()))
}

/// The loudest bin above 4 kHz in 4,096-sample Hann windows hopping by 1,024 over `y[from..]`, in
/// dB, a sine reading its peak.
fn loudest_above_4k(y: &[f64], from: usize, rate: u32) -> f64 {
    let n = 4096;
    let w: Vec<f64> = (0..n).map(|k| 0.5 - 0.5 * libm::cos(2.0 * PI * k as f64 / n as f64)).collect();
    let norm = n as f64 / w.iter().sum::<f64>();
    let first = (4000.0 * n as f64 / rate as f64).ceil() as usize;
    let mut worst = f64::MIN;
    let mut at = from;
    while at + n <= y.len() {
        let x: Vec<f64> = y[at..at + n].iter().zip(&w).map(|(a, b)| a * b).collect();
        for v in &spectrum(&x)[first..] {
            worst = worst.max(20.0 * libm::log10((v * norm).max(1e-20)));
        }
        at += 1024;
    }
    worst
}

/// `specs/shared.md`'s held key: a 1 kHz sine at −12 dBFS, `id` set from `path` every 2 ms for a
/// second (from `base`), after a quarter second at the path's start.
fn held_key(rate: u32, base: Settings, id: &str, path: &[f64]) {
    let r = rate as usize;
    let (pre, hop, changes) = (r / 4, (0.002 * rate as f64) as usize, 500);
    let total = pre + hop * changes + r + r / 4;
    let x = faded(sine(1000.0, level(-12.0), total, rate), r / 50);
    let set = |v: f64| {
        let mut s = base;
        s.set_by_id(id, v);
        s
    };
    let mut eq = Eq::new(rate as f64);
    eq.set(&set(path[0]));
    let mut y = x.clone();
    eq.process(&mut y[..pre]);
    let mut last = path[0];
    for i in 0..changes {
        last = path[i % path.len()];
        eq.set(&set(last));
        let at = pre + i * hop;
        eq.process(&mut y[at..at + hop]);
    }
    let end = pre + changes * hop;
    let after = end + (0.05 * rate as f64) as usize;
    eq.process(&mut y[end..after]);
    assert!(eq.settled(), "{id}: settled 50 ms after");
    assert_eq!(eq.settings(), set(last).held(), "{id}: the last setting holds");
    eq.process(&mut y[after..]);

    let from = r / 10;
    let lo = path.iter().cloned().fold(f64::MAX, f64::min);
    let hi = path.iter().cloned().fold(f64::MIN, f64::max);
    let bound = 1.1
        * largest_step(&through(&set(lo), rate, &x, 1024), from)
            .max(largest_step(&through(&set(hi), rate, &x, 1024), from));
    let step = largest_step(&y, from);
    assert!(step <= bound, "{id} at {rate}: a step of {step:e} against {bound:e}");
    let burst = loudest_above_4k(&y, from, rate);
    assert!(burst < -80.0, "{id} at {rate}: {burst:.1} dB above 4 kHz");
    // A fresh unit set to the last setting, a second on: the same within 1e-6.
    let fresh = through(&set(last), rate, &x, 1024);
    let tail = total - r / 4;
    let n = null(&y[tail..], &fresh[tail..]);
    assert!(n < 1e-6, "{id} at {rate}: {n:e} from a fresh unit");
    eprintln!(
        "{id} at {rate}: step {:.3} of the bound, {burst:.1} dB above 4 kHz, {n:.1e} off",
        step / bound
    );
}

fn level(db: f64) -> f64 {
    libm::pow(10.0, db / 20.0)
}

/// A control's grid by fine steps from its low end to its high end and back.
fn sweep(id: &str) -> Vec<f64> {
    let c = as_control::find(controls(), id).unwrap();
    let Kind::Value { lo, hi, .. } = c.kind else { panic!() };
    let mut up = vec![lo];
    while *up.last().unwrap() < hi {
        up.push(c.step(*up.last().unwrap(), 1, as_control::Size::Fine));
    }
    let down: Vec<f64> = up.iter().rev().skip(1).cloned().collect();
    up.extend(down);
    up
}

#[test]
fn a_held_key_glides() {
    for rate in RATES {
        // The low-mid cut, so the sweep's loudest is at its ends.
        held_key(rate, with(&[("lmf.gain", -12.0)]), "lmf.freq", &sweep("lmf.freq"));
        held_key(rate, ch(), "hf.gain", &sweep("hf.gain"));
        held_key(rate, with(&[("hpf.in", 1.0)]), "hpf.freq", &sweep("hpf.freq"));
        held_key(rate, with(&[("lf.freq", 450.0), ("lf.gain", 15.0)]), "lf.bell", &[0.0, 1.0]);
    }
}

#[test]
fn a_step_crossfades_to_the_new_set() {
    for rate in RATES {
        let r = rate as usize;
        let n = 2 * r;
        let x = faded(sine(1000.0, level(-12.0), n, rate), r / 50);
        let at = 12 * 1024;
        let up = with(&[("lmf.gain", 12.0)]);
        let mut eq = Eq::new(rate as f64);
        let mut y = x.clone();
        eq.process(&mut y[..at]);
        eq.set(&up);
        eq.process(&mut y[at..]);
        let from = r / 10;
        let bound = 1.1
            * largest_step(&through(&ch(), rate, &x, 1024), from)
                .max(largest_step(&through(&up, rate, &x, 1024), from));
        assert!(largest_step(&y, from) <= bound, "{rate}: a step");
        let burst = loudest_above_4k(&y, from, rate);
        assert!(burst < -80.0, "{rate}: {burst:.1} dB above 4 kHz");
        // From 200 ms after the step, the new set's steady output.
        let settled = at + r / 5;
        let new = through(&up, rate, &x, 1024);
        let d = null(&y[settled..], &new[settled..]);
        assert!(d < 1e-9, "{rate}: {d:e}");
    }
}

/// A 40 Hz sine at −6 dBFS through the LF shelf at 30 Hz, stepped from `g0` to `g1` dB: the loudest
/// bin above 4 kHz, the largest the output strays from the old and new sets' steady outputs
/// crossfaded, and its peak after the step against theirs.
fn shelf_step(rate: u32, g0: f64, g1: f64, from_rest: bool) -> (f64, f64, f64) {
    let r = rate as usize;
    let x = faded(sine(40.0, 0.5, r, rate), r / 50);
    let (old, new) =
        (with(&[("lf.freq", 30.0), ("lf.gain", g0)]), with(&[("lf.freq", 30.0), ("lf.gain", g1)]));
    let at = 12 * 1024;
    let mut eq = Eq::new(rate as f64);
    eq.from_rest = from_rest;
    eq.set(&old);
    let mut y = x.clone();
    eq.process(&mut y[..at]);
    eq.set(&new);
    eq.process(&mut y[at..]);
    let (a, b) = (through(&old, rate, &x, 1024), through(&new, rate, &x, 1024));
    let fade = libm::round(0.01 * rate as f64) as usize;
    let ideal: Vec<f64> = (0..r)
        .map(|k| {
            let t = if k < at { 0.0 } else { ((k - at + 1) as f64 / fade as f64).min(1.0) };
            (1.0 - t) * a[k] + t * b[k]
        })
        .collect();
    let steady = peak(&a[at..]).max(peak(&b[at..]));
    (loudest_above_4k(&y, r / 10, rate), null(&y, &ideal), peak(&y[at..]) / steady)
}

#[test]
fn the_new_set_starts_settled() {
    for rate in RATES {
        for g1 in [6.0, 15.0] {
            let (burst, off, over) = shelf_step(rate, 0.0, g1, false);
            let (_, off_rest, over_rest) = shelf_step(rate, 0.0, g1, true);
            eprintln!(
                "{rate}, 0 to {g1} dB: {burst:.1} dB above 4 kHz, {off:.4} off, peak {over:.4} of steady; from rest {off_rest:.4} off, peak {over_rest:.3}"
            );
            assert!(burst < -80.0, "{rate}: {burst:.1} dB above 4 kHz");
            assert!(off < 0.005, "{rate}, 0 to {g1}: {off} off the steady crossfade");
            assert!(over < 1.005, "{rate}, 0 to {g1}: peak {over} of steady");
            // Without the pre-roll it thumps: what the test is for.
            assert!(
                off_rest > 0.03 && over_rest > 1.05,
                "{rate}, 0 to {g1}: from rest {off_rest}, {over_rest}"
            );
        }
    }
}

/// A stream of settings at multiples of 1,024 samples, the trim and In among them.
fn stream() -> Vec<(usize, Settings)> {
    vec![
        (0, with(&[("hpf.in", 1.0), ("lf.gain", 6.0), ("lmf.gain", -9.0), ("lmf.q", 3.0), ("hf.gain", 4.0)])),
        (
            4 * 1024,
            with(&[("hpf.in", 1.0), ("hpf.slope", 0.0), ("lf.bell", 1.0), ("lf.gain", -6.0), ("trim", 4.0)]),
        ),
        (5 * 1024, with(&[("hmf.gain", 12.0), ("hmf.freq", 6000.0), ("trim", -3.0)])),
        (9 * 1024, with(&[("in", 0.0), ("hmf.gain", 12.0)])),
        (12 * 1024, with(&[("hf.bell", 1.0), ("hf.gain", -15.0), ("hf.freq", 16000.0)])),
        (13 * 1024, with(&[("hf.bell", 1.0), ("hf.gain", 15.0), ("hf.freq", 16000.0), ("lmf.gain", 3.0)])),
    ]
}

fn run_stream(rate: u32, l: &[f64], r: &[f64], block: usize, two: bool) -> (Vec<f64>, Vec<f64>) {
    let mut eq = Eq::new(rate as f64);
    let (mut a, mut b) = (l.to_vec(), r.to_vec());
    let s = stream();
    let mut at = 0;
    while at < a.len() {
        for (k, set) in &s {
            if *k == at {
                eq.set(set);
            }
        }
        let end = (at + block).min(a.len());
        if two {
            eq.process2(&mut a[at..end], &mut b[at..end]);
        } else {
            eq.process(&mut a[at..end]);
        }
        at = end;
    }
    (a, b)
}

fn same_bits(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits())
}

#[test]
fn the_same_bits_at_any_block_size_and_twice() {
    for rate in RATES {
        let n = 16 * 1024;
        let (l, r) = (noise(n, 0.5, 1), sine(300.0, 0.5, n, rate));
        let (a, b) = run_stream(rate, &l, &r, 1024, true);
        for block in [1, 64, 1024] {
            let (c, d) = run_stream(rate, &l, &r, block, true);
            assert!(same_bits(&a, &c) && same_bits(&b, &d), "{rate}, blocks of {block}");
            let (m, _) = run_stream(rate, &l, &r, block, false);
            assert!(same_bits(&a, &m), "{rate}, one channel, blocks of {block}");
        }
        // The right channel is the left's filters on its own input.
        let (rr, _) = run_stream(rate, &r, &l, 64, false);
        assert!(same_bits(&b, &rr), "{rate}: the right channel alone");
        assert!(null(&a, &l) > 0.01, "the stream did something");
    }
}

#[test]
fn every_output_is_finite() {
    let rate = 48_000;
    let n = rate as usize;
    let mut signals = torture(rate, 1.0);
    signals.push(("silence", vec![0.0; n]));
    signals.push(("DC +1", vec![1.0; n]));
    signals.push(("DC −1", vec![-1.0; n]));
    signals.push(("−300 dBFS", noise(n, 1e-15, 4)));
    for (name, x) in signals {
        let (y, _) = run_stream(rate, &x, &x, 256, true);
        assert!(y.iter().all(|v| v.is_finite()), "{name}");
    }
}

#[test]
fn a_tail_to_silence_takes_no_longer() {
    use std::time::Instant;
    let rate = 48_000;
    // Bands whose memory falls past the denormals within the ten seconds, unflushed.
    let s = with(&[
        ("hpf.in", 1.0),
        ("hpf.freq", 400.0),
        ("lf.freq", 450.0),
        ("lf.gain", 15.0),
        ("lmf.freq", 1000.0),
        ("lmf.gain", 9.0),
        ("hmf.gain", -9.0),
        ("hf.gain", 6.0),
    ]);
    let mut eq = Eq::new(rate as f64);
    eq.set(&s);
    // A second of noise, then ten falling past −300 dBFS to silence, timed a block at a time.
    let x = noise(rate as usize, 1.0, 9);
    let blocks = (rate as usize / 1024) + 10 * rate as usize / 1024;
    let mut times = vec![];
    for k in 0..blocks {
        let mut l = [0.0; 1024];
        if let Some(b) = x.get(k * 1024..(k + 1) * 1024) {
            l.copy_from_slice(b);
        }
        let mut r = l;
        let t = Instant::now();
        eq.process2(&mut l, &mut r);
        times.push(t.elapsed().as_nanos());
    }
    let median = |t: &[u128]| {
        let mut t = t.to_vec();
        t.sort();
        t[t.len() / 2]
    };
    let (start, end) = (median(&times[..40]), median(&times[times.len() - 90..]));
    eprintln!("a block: {start} ns at the start, {end} ns at the end");
    assert!(end < 2 * start.max(1), "{end} ns against {start}");
    let mem = eq.mem[eq.live][0].sec;
    assert!(
        mem.iter().flatten().flatten().all(|&v| v == 0.0 || v.abs() >= FLUSH),
        "no state below the flush"
    );
    assert!(mem.iter().flatten().flatten().all(|&v| v == 0.0), "fallen to 0");
}

#[test]
fn out_is_the_input_to_the_bit() {
    for rate in RATES {
        let n = rate as usize;
        let x = noise(n, 0.7, 5);
        let busy = with(&[("hpf.in", 1.0), ("lf.gain", 6.0), ("hmf.gain", -9.0), ("trim", 5.0)]);
        let mut eq = Eq::new(rate as f64);
        eq.set(&busy);
        let mut y = x.clone();
        eq.process(&mut y[..n / 2]);
        eq.set(&Settings { on: false, ..busy });
        eq.process(&mut y[n / 2..]);
        // 10 ms for the crossfade, 5 for the trim: out from there on.
        let from = n / 2 + n / 50;
        assert!(same_bits(&x[from..], &y[from..]), "{rate}");
    }
}
