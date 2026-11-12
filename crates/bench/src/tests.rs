use as_envelope::{db, measure, signal::noise};

use crate::align;
use crate::signal::{CEILING_DB, Maker, Signal, mark};

const RATES: [u32; 3] = [44_100, 48_000, 96_000];

fn all(m: &Maker) -> Vec<(Signal, f64)> {
    let c = m.ceiling_db;
    vec![
        (m.silence(5.0), crate::signal::MARK_DB.min(c)),
        (m.sweep(10.0, 0.0), c),
        (m.steps(&Maker::steps_db(), 0.3), c),
        (m.bursts(-30.0, 0.5, -10.0, 0.3, 3), c - 10.0),
        (m.hold(-6.0, 2.0, Some(-40.0), 3.0), c - 6.0),
        (
            m.steady_sweep(100.0, -6.0, 1.0, -12.0, 5.0),
            c + db(as_envelope::level(-6.0) + as_envelope::level(-12.0)),
        ),
    ]
}

#[test]
fn every_signal_peaks_where_said_and_never_over_the_ceiling() {
    for rate in RATES {
        for ceiling in [CEILING_DB, -12.0, -30.0] {
            let m = Maker::new(rate, ceiling);
            for (s, want) in all(&m) {
                let peak = db(measure::peak(&s.samples));
                assert!(peak <= ceiling + 1e-9, "{} at {rate}: {peak} over {ceiling}", s.name);
                // steady_sweep's two parts add only near where their peaks meet: within 1 dB.
                let tol = if s.name == "steady_sweep" { 1.0 } else { 0.01 };
                // The mark is in front of every signal, louder than a quiet one.
                let want = want.min(ceiling).max(crate::signal::MARK_DB.min(ceiling));
                assert!((peak - want).abs() < tol, "{} at {rate}, {ceiling}: {peak} not {want}", s.name);
            }
        }
    }
}

#[test]
fn signals_are_as_long_as_said_and_the_same_twice() {
    for rate in RATES {
        let m = Maker::new(rate, CEILING_DB);
        let r = rate as f64;
        let around = (0.020 * r).round() as usize + (0.2 * r).round() as usize + (0.5 * r).round() as usize;
        let s = m.silence(5.0);
        assert_eq!(s.samples.len(), around + (5.0 * r) as usize);
        let s = m.steps(&Maker::steps_db(), 0.3);
        assert_eq!(s.parts.len(), 61);
        assert_eq!(s.samples.len(), around + 61 * (0.3 * r).round() as usize);
        assert_eq!(s.parts[0].db, CEILING_DB - 60.0);
        assert_eq!(s.parts[60].db, CEILING_DB);
        let b = m.bursts(-30.0, 0.5, -10.0, 0.3, 3);
        assert_eq!(b.parts.len(), 7);
        for (a, b) in all(&m).iter().zip(all(&m).iter()) {
            assert!(a.0.samples.iter().zip(&b.0.samples).all(|(x, y)| x.to_bits() == y.to_bits()));
        }
    }
}

/// `x` delayed by `d` samples, whole part by shifting, fraction by a fixed windowed-sinc filter.
fn delayed(x: &[f64], d: f64) -> Vec<f64> {
    let whole = d.floor() as usize;
    let frac = d - d.floor();
    let mut y = vec![0.0; x.len() + whole + 64];
    if frac == 0.0 {
        y[whole..whole + x.len()].copy_from_slice(x);
        return y;
    }
    let half = 16i64;
    let taps: Vec<f64> = (-half..=half)
        .map(|k| {
            let t = k as f64 - frac;
            let w = 0.5 + 0.5 * (std::f64::consts::PI * t / (half as f64 + 1.0)).cos();
            let s =
                if t == 0.0 { 1.0 } else { (std::f64::consts::PI * t).sin() / (std::f64::consts::PI * t) };
            s * w
        })
        .collect();
    for (n, v) in x.iter().enumerate() {
        for (j, t) in taps.iter().enumerate() {
            let at = n as i64 + whole as i64 + j as i64 - half;
            if at >= 0 && (at as usize) < y.len() {
                y[at as usize] += v * t;
            }
        }
    }
    y
}

#[test]
fn the_mark_is_found_to_the_sample() {
    for rate in RATES {
        let m = Maker::new(rate, CEILING_DB);
        let mk = m.mark();
        for s in [m.sweep(10.0, 0.0), m.steps(&Maker::steps_db(), 0.1)] {
            for d in [0.0, 1.0, 17.0, 480.0, 1234.25, 2500.75, 5000.0] {
                for invert in [false, true] {
                    let mut x = delayed(&s.samples, d);
                    let n = noise(x.len(), as_envelope::level(-60.0), 7);
                    for (v, e) in x.iter_mut().zip(&n) {
                        *v = if invert { -*v } else { *v } + e;
                    }
                    let f = align::find(&x, &mk, rate).expect("found");
                    assert_eq!(f.latency, d.round() as usize, "{} at {rate}, {d}", s.name);
                    assert_eq!(f.positive, !invert);
                    assert!(f.ratio >= 10.0, "{} at {rate}, {d}: ratio {}", s.name, f.ratio);
                }
            }
        }
    }
}

#[test]
fn the_mark_is_tapered() {
    let x = mark(48_000, -20.0);
    assert_eq!(x.len(), 960);
    assert!(x[0].abs() < 1e-12);
    assert!((db(measure::peak(&x)) + 20.0).abs() < 0.01);
}

/// A sweep through `unit`, deconvolved, on third-octave points.
fn through(rate: u32, unit: impl Fn(&mut Vec<f64>)) -> Vec<(f64, f64)> {
    let m = Maker::new(rate, CEILING_DB);
    let s = m.sweep(10.0, 0.0);
    let p = s.parts[0];
    let x = s.samples[p.start..p.start + p.len].to_vec();
    let mut y = s.samples[p.start..].to_vec();
    unit(&mut y);
    let d = crate::response::deconvolve(&x, &y, 10.0, rate);
    crate::response::thirds(&d.linear, d.n, rate)
}

#[test]
fn a_gain_reads_flat() {
    for rate in RATES {
        for (f, g) in through(rate, |y| y.iter_mut().for_each(|v| *v *= 0.5)) {
            if (30.0..=15_000.0).contains(&f) {
                assert!((g - db(0.5)).abs() < 0.05, "{rate}: {f} Hz reads {g}");
            }
        }
    }
}

#[test]
fn a_peak_reads_as_its_gain_says() {
    for rate in RATES {
        let bq = as_biquad::Biquad::peak(1000.0, 0.7, 6.0, rate as f64);
        for (f, g) in through(rate, |y| bq.run(y)) {
            if (30.0..=15_000.0).contains(&f) {
                let want = db(bq.gain(f, rate as f64));
                assert!((g - want).abs() < 0.1, "{rate}: {f} Hz reads {g}, not {want}");
            }
        }
    }
}

/// A tone's harmonic `m` through `f`, by a direct Fourier sum over one cycle of 8,192 points:
/// the closed form, not the bench's reduction.
fn fourier(f: impl Fn(f64) -> f64, amp: f64, m: usize) -> f64 {
    let n = 8192;
    let (mut re, mut im) = (0.0, 0.0);
    for k in 0..n {
        let th = std::f64::consts::TAU * k as f64 / n as f64;
        let v = f(amp * th.sin());
        re += v * (m as f64 * th).cos();
        im += v * (m as f64 * th).sin();
    }
    2.0 * (re * re + im * im).sqrt() / n as f64
}

#[test]
fn a_tanh_stage_gives_its_series() {
    let bias = 0.2;
    let stage = move |v: f64| (v + bias).tanh() - bias.tanh();
    for rate in RATES {
        let m = Maker::new(rate, CEILING_DB);
        let s = m.steps(&Maker::steps_db(), 0.3);
        let y: Vec<f64> = s.samples.iter().map(|v| stage(*v)).collect();
        let curve = crate::levels::steps(&s, &y, -300.0);
        let mut seen = 0;
        for st in &curve {
            let a = as_envelope::level(st.in_db);
            for (i, h) in st.harmonics.iter().enumerate() {
                let want = db(fourier(stage, a, i + 2));
                if want < -90.0 {
                    continue;
                }
                let got = h.expect("over the floor");
                assert!((got - want).abs() < 0.5, "{rate}: {} dBFS, h{}: {got} not {want}", st.in_db, i + 2);
                seen += 1;
            }
            let want = db(fourier(stage, a, 1));
            assert!((st.out_db - want).abs() < 0.01, "{rate}: fundamental at {}", st.in_db);
        }
        assert!(seen > 40, "{rate}: only {seen} harmonics over −90 dBFS");
    }
}

/// A plain feed-forward compressor: a peak over the last cycle, a soft knee, its gain smoothed
/// in dB by one pole each way, so the times are the poles'.
struct Comp {
    thr: f64,
    ratio: f64,
    knee: f64,
    attack: f64,
    release: f64,
    rate: f64,
}

impl Comp {
    fn target(&self, x: f64) -> f64 {
        let (t, r, w) = (self.thr, self.ratio, self.knee);
        let over = x - t;
        let y = if 2.0 * over < -w {
            x
        } else if 2.0 * over.abs() <= w {
            x + (1.0 / r - 1.0) * (over + w / 2.0).powi(2) / (2.0 * w)
        } else {
            t + over / r
        };
        y - x
    }

    fn run(&self, x: &[f64]) -> Vec<f64> {
        let p = (self.rate / 1000.0).ceil() as usize + 1;
        let (aa, ar) = ((-1.0 / (self.attack * self.rate)).exp(), (-1.0 / (self.release * self.rate)).exp());
        let mut g = 0.0;
        (0..x.len())
            .map(|n| {
                let peak = x[n.saturating_sub(p - 1)..=n].iter().fold(0.0f64, |m, v| m.max(v.abs()));
                let want = self.target(db(peak.max(1e-15)));
                let a = if want < g { aa } else { ar };
                g = want + a * (g - want);
                x[n] * as_envelope::level(g)
            })
            .collect()
    }
}

#[test]
fn a_compressor_reads_as_written() {
    if slow() {
        return;
    }
    for rate in RATES {
        let c = Comp { thr: -30.0, ratio: 4.0, knee: 6.0, attack: 0.050, release: 0.200, rate: rate as f64 };
        let m = Maker::new(rate, CEILING_DB);
        let s = m.steps(&Maker::steps_db(), 1.5);
        let curve = crate::levels::steps(&s, &c.run(&s.samples), -300.0);
        for st in &curve {
            let want = st.in_db + c.target(st.in_db);
            assert!((st.out_db - want).abs() < 0.2, "{rate}: {} in, {} out, not {want}", st.in_db, st.out_db);
        }
        let rest = crate::levels::rest_gain(&curve, 5);
        let b = m.bursts(-40.0, 1.5, -10.0, 0.5, 3);
        let env = crate::levels::reduction(&b, &c.run(&b.samples), rest);
        let (attack, release) = crate::levels::burst_times(&env, &b);
        assert_eq!((attack.len(), release.len()), (3, 3));
        for a in attack {
            assert!((a / c.attack - 1.0).abs() < 0.05, "{rate}: attack {a}");
        }
        for r in release {
            assert!((r / c.release - 1.0).abs() < 0.05, "{rate}: release {r}");
        }
    }
}

/// A slow test runs only with `FAMILY_SLOW` set, as CI and `family.sh check --slow` do (Warren,
/// 2026-11-12). Skipped, it says so.
fn slow() -> bool {
    if std::env::var_os("FAMILY_SLOW").is_some() {
        return false;
    }
    eprintln!("skipped: slow, set FAMILY_SLOW=1 to run it");
    true
}

/// The compressor above with a memory: its release slows the longer it's been reducing, from
/// `fast` to `slow`, with `mem` the time constant of how long counts.
fn with_memory(c: &Comp, fast: f64, slow: f64, mem: f64, x: &[f64]) -> Vec<f64> {
    let p = (c.rate / 1000.0).ceil() as usize + 1;
    let aa = (-1.0 / (c.attack * c.rate)).exp();
    let (mut g, mut held) = (0.0, 0.0f64);
    (0..x.len())
        .map(|n| {
            let peak = x[n.saturating_sub(p - 1)..=n].iter().fold(0.0f64, |m, v| m.max(v.abs()));
            let want = c.target(db(peak.max(1e-15)));
            let a = if want < g {
                held += 1.0 / c.rate;
                aa
            } else {
                let tau = fast + (slow - fast) * (1.0 - (-held / mem).exp());
                (-1.0 / (tau * c.rate)).exp()
            };
            if want >= -0.01 && g >= -0.01 {
                held = 0.0;
            }
            g = want + a * (g - want);
            x[n] * as_envelope::level(g)
        })
        .collect()
}

#[test]
fn a_memory_reads_as_written() {
    let rate = 48_000;
    let c = Comp { thr: -30.0, ratio: 4.0, knee: 6.0, attack: 0.005, release: 0.0, rate: rate as f64 };
    let (fast, slow, mem) = (0.080, 0.800, 1.0);
    let m = Maker::new(rate, CEILING_DB);
    let release = |secs: f64| {
        let s = m.hold(-10.0, secs, Some(-40.0), 6.0);
        let env = crate::levels::reduction(&s, &with_memory(&c, fast, slow, mem, &s.samples), 0.0);
        crate::levels::hold_release(&env, &s).expect("a release")
    };
    // The closed form: held for the hold, less the attack's start (counted while reducing).
    let tau = |held: f64| fast + (slow - fast) * (1.0 - (-held / mem).exp());
    let want = tau(2.0) / tau(0.1);
    let got = release(2.0) / release(0.1);
    assert!((got / want - 1.0).abs() < 0.10, "memory ratio {got}, not {want}");
}

mod checks {
    use super::*;
    use crate::check::{Calibration, Capture, calibrate, check};

    const LAT: usize = 100;

    /// The loopback: `s` delayed by `lat`, scaled by `gain`, with noise at −100 dBFS, and a
    /// second of tail.
    fn loop_back(s: &Signal, lat: usize, gain: f64) -> Capture {
        let mut y = vec![0.0; lat];
        y.extend(s.samples.iter().map(|v| v * gain));
        y.extend(std::iter::repeat_n(0.0, s.rate as usize));
        let n = noise(y.len(), as_envelope::level(-100.0), 3);
        y.iter_mut().zip(&n).for_each(|(v, e)| *v += e);
        Capture { rate: s.rate, samples: y, xruns: 0 }
    }

    fn cal(m: &Maker) -> Calibration {
        let (a, b, c) = (m.silence(5.0), m.sweep(10.0, 0.0), m.steps(&Maker::steps_db(), 0.3));
        calibrate(
            "loopback",
            (&a, &loop_back(&a, LAT, 0.9)),
            (&b, &loop_back(&b, LAT, 0.9)),
            (&c, &loop_back(&c, LAT, 0.9)),
        )
        .expect("calibrated")
    }

    #[test]
    fn the_calibration_reads_the_loopback() {
        let m = Maker::new(48_000, CEILING_DB);
        let k = cal(&m);
        assert_eq!(Calibration::from_toml(&k.to_toml()), Ok(k.clone()));
        let dir = std::env::temp_dir().join(format!("as-bench-{}-cal", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch");
        let a = k.write(&dir, "loopback-2026-11-12").expect("kept");
        let b = k.write(&dir, "loopback-2026-11-12").expect("kept beside");
        assert!(a.ends_with("loopback-2026-11-12-001.toml") && b.ends_with("loopback-2026-11-12-002.toml"));
        assert_eq!(Calibration::from_toml(&std::fs::read_to_string(&a).expect("read")), Ok(k.clone()));
        std::fs::remove_dir_all(&dir).expect("cleaned");
        assert_eq!(k.latency, LAT);
        assert!(k.positive);
        assert!((k.gain_db - db(0.9)).abs() < 0.001, "gain {}", k.gain_db);
        assert!((k.floor_db + 100.0).abs() < 3.0, "floor {}", k.floor_db);
        for [f, g] in &k.response {
            if (30.0..=15_000.0).contains(f) {
                assert!((g - db(0.9)).abs() < 0.05, "{f} Hz reads {g}");
            }
        }
    }

    #[test]
    fn each_fault_fails_its_line_alone() {
        let m = Maker::new(48_000, CEILING_DB);
        let k = cal(&m);
        let s = m.sweep(10.0, 0.0);
        let good = loop_back(&s, LAT, 0.9);
        let r = check(&s, &good, Some(&k));
        assert!(r.pass(), "{r}");
        let fault = |name: &str, c: Capture, line: &str| {
            let r = check(&s, &c, Some(&k));
            assert_eq!(r.failed(), vec![line], "{name}:\n{r}");
        };
        fault("clipped", loop_back(&s, LAT, 2.0), "clipped");
        fault("too quiet", loop_back(&s, LAT, as_envelope::level(-40.0)), "level");
        fault("inverted", loop_back(&s, LAT, -0.9), "polarity");
        let mut short = good.clone();
        short.samples.truncate(s.samples.len());
        fault("cut short", short, "length");
        fault("mis-aligned", loop_back(&s, LAT + 50, 0.9), "aligned");
        let mut noisy = good.clone();
        let n = noise(noisy.samples.len(), as_envelope::level(-40.0), 9);
        noisy.samples.iter_mut().zip(&n).for_each(|(v, e)| *v += e);
        fault("noisy", noisy, "signal over noise");
        fault("xruns", Capture { xruns: 1, ..good.clone() }, "xruns");
    }
}

mod figures {
    use super::*;
    use crate::figures::{self as fig, Figures};

    #[test]
    fn a_model_reduces_to_figures_that_survive_the_file() {
        if slow() {
            return;
        }
        let rate = 48_000;
        let c = Comp { thr: -30.0, ratio: 4.0, knee: 6.0, attack: 0.050, release: 0.200, rate: rate as f64 };
        let m = Maker::new(rate, CEILING_DB);
        let mut unit = |l: &mut [f64], r: &mut [f64]| {
            let (a, b) = (c.run(l), c.run(r));
            l.copy_from_slice(&a);
            r.copy_from_slice(&b);
        };
        let st = m.steps(&Maker::steps_db(), 1.5);
        let [cap, right] = crate::run_model(&st, &mut unit);
        assert_eq!(cap, right);
        assert!(crate::check::check(&st, &cap, None).pass());
        let curve = fig::curve(&st, &cap, None).expect("aligned");
        let rest = fig::rest_gain(&curve);
        assert!(rest.abs() < 0.01, "rest {rest}");
        let b = m.bursts(-40.0, 1.5, -10.0, 0.5, 3);
        let times = fig::times(&b, &crate::run_model(&b, &mut unit)[0], None, rest).expect("aligned");
        assert!((times.attack_mean / 0.050 - 1.0).abs() < 0.05);
        assert!(times.attack_spread < 0.002);
        let sw = m.sweep(10.0, 0.0);
        let response = fig::response(&sw, &crate::run_model(&sw, &mut unit)[0], None).expect("aligned");
        let f = Figures {
            unit: "a test compressor".into(),
            date: "2026-11-12".into(),
            response,
            curve,
            times: Some(times),
            ..Default::default()
        };
        let back = Figures::from_toml(&f.to_toml()).expect("reads back");
        assert_eq!(back, f);
        let d = f.against(&back);
        assert!(d.len() > 60 && d.iter().all(|d| d.diff == 0.0));
        let mut other = f.clone();
        other.curve[60].out_db += 1.0;
        let d = f.against(&other);
        assert_eq!(d.iter().filter(|d| d.diff != 0.0).count(), 1);
    }
}

mod sittings {
    use super::*;
    use crate::check::{Capture, check};
    use crate::sitting::{List, Sitting};

    const LIST: &str = r#"
unit = "a test unit"

[[capture]]
name = "floor"
signal = "silence"
secs = 1.0
inputs = ["L"]

[[capture]]
name = "curve"
signal = "steps"
levels = [-30.0, -20.0, -10.0, 0.0]
hold = 0.2
knobs = "ratio 4, input 3"
"#;

    fn scratch(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("as-bench-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("scratch");
        d
    }

    #[test]
    fn a_list_is_read_and_a_bad_one_said() {
        let l = List::from_toml(LIST).expect("reads");
        assert_eq!(l.captures.len(), 2);
        assert_eq!(l.captures[0].channels(), Ok(vec![0]));
        assert_eq!(l.captures[1].channels(), Ok(vec![0, 1]));
        let s = l.captures[1].signal(&Maker::new(48_000, CEILING_DB)).expect("a signal");
        assert_eq!(s.parts.len(), 4);
        assert!(List::from_toml(&LIST.replace("\"steps\"", "\"stairs\"")).unwrap_err().contains("stairs"));
        assert!(
            List::from_toml(&LIST.replace("\"curve\"", "\"floor\"")).unwrap_err().contains("named twice")
        );
        assert!(List::from_toml(&LIST.replace("hold = 0.2", "hodl = 0.2")).is_err());
    }

    #[test]
    fn a_sitting_resumes_and_keeps_its_redos() {
        let dir = scratch("sitting");
        let mut s = Sitting::start(&dir, LIST, 48_000, CEILING_DB).expect("started");
        assert!(s.dir().ends_with("sitting-001"));
        assert_eq!(s.next(), Some(0));
        let m = s.maker();
        let play = |s: &Sitting, i: usize, gain: f64| {
            let sig = s.list.captures[i].signal(&m).expect("a signal");
            let [l, r] = crate::run_model(&sig, &mut |l: &mut [f64], r: &mut [f64]| {
                l.iter_mut().chain(r.iter_mut()).for_each(|v| *v *= gain)
            });
            let rep = check(&sig, &l, None);
            (vec![l, r], rep)
        };
        let (caps, rep) = play(&s, 0, 0.5);
        assert!(rep.pass(), "{rep}");
        s.keep(0, &caps[..1], &rep, "").expect("kept");
        assert_eq!(s.next(), Some(1));
        // Too quiet: kept, failed, and the capture still next.
        let (caps, rep) = play(&s, 1, 0.001);
        assert!(!rep.pass());
        s.keep(1, &caps, &rep, "ratio 4, input 3").expect("kept");
        assert_eq!(s.next(), Some(1));
        assert!(s.passed(1).is_none());
        // Stopped here, and opened again.
        let path = s.dir().to_path_buf();
        drop(s);
        let mut s = Sitting::open(&path).expect("opened");
        assert_eq!(s.next(), Some(1));
        let (caps, rep) = play(&s, 1, 0.5);
        assert!(rep.pass(), "{rep}");
        s.keep(1, &caps, &rep, "ratio 4, input 3").expect("kept");
        assert_eq!(s.next(), None);
        let t = s.takes(1);
        assert_eq!((t.len(), t[0].file.as_str(), t[1].file.as_str()), (2, "curve-001.wav", "curve-002.wav"));
        let back: Vec<Capture> = s.read(s.passed(1).expect("passed")).expect("read");
        assert_eq!(back.len(), 2);
        assert!(back[0].samples.iter().zip(&caps[0].samples).all(|(a, b)| *a == (*b as f32) as f64));
        let s2 = Sitting::start(&dir, LIST, 48_000, CEILING_DB).expect("a second");
        assert!(s2.dir().ends_with("sitting-002"));
        std::fs::remove_dir_all(&dir).expect("cleaned");
    }

    #[test]
    fn pictures_carry_both_sides() {
        use crate::figures::{Figures, Point};
        use crate::picture;
        let pt = |i: f64, o: f64| Point {
            in_db: i,
            out_db: o,
            h2_db: Some(o - 40.0),
            h3_db: None,
            h4_db: None,
            h5_db: None,
        };
        let unit = Figures {
            curve: (-60..=0)
                .map(|i| {
                    let i = i as f64;
                    pt(i, if i > -30.0 { -30.0 + (i + 30.0) / 4.0 } else { i })
                })
                .collect(),
            response: vec![[1000.0, 0.0]],
            ..Default::default()
        };
        let c = picture::curve(&unit, Some(&unit));
        assert_eq!(c.series.len(), 2);
        assert_eq!(c.notes[0].text, "unit 4.0:1");
        assert_eq!(picture::harmonics(&unit, None).series[0].name, "unit h2");
        assert_eq!(picture::response(&unit, Some(&unit)).series.len(), 2);
        let m = Maker::new(48_000, CEILING_DB);
        let b = m.bursts(-40.0, 0.5, -10.0, 0.2, 3);
        let env = crate::levels::reduction(&b, &b.samples, 0.0);
        let p = picture::reduction(&b, 2, &env, Some(&env)).expect("burst 3");
        assert_eq!(p.series.len(), 2);
        let last = p.series[0].points.last().expect("points").0;
        assert!((last - 0.7).abs() < 0.01, "{last}");
        assert!(picture::reduction(&b, 3, &env, None).is_none());
        let s = m.sweep(10.0, 0.0);
        let cap = crate::check::Capture { rate: 48_000, samples: vec![0.0; 1000], xruns: 0 };
        let rep = crate::check::check(&s, &cap, None);
        let f = picture::failed(&s, &cap, &rep);
        assert!(f.notes.iter().any(|n| n.text.starts_with("aligned")));
    }
}

#[test]
fn the_bench_never_connects_a_port() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut read = 0;
    for e in std::fs::read_dir(&dir).expect("src") {
        let path = e.expect("an entry").path();
        if path.file_name().is_some_and(|n| n == "tests.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("source");
        for call in ["connect_ports", ".connect(", "disconnect", "reconcile", "JackGraph"] {
            assert!(!text.contains(call), "{} calls {call}", path.display());
        }
        read += 1;
    }
    assert!(read >= 9, "read {read} files");
}

#[test]
fn a_tanh_stage_through_the_sweep_gives_its_series() {
    let bias = 0.2;
    let stage = move |v: f64| (v + bias).tanh() - bias.tanh();
    for rate in RATES {
        let s = Maker::new(rate, CEILING_DB).sweep(10.0, 0.0);
        let [c, _] = crate::run_model(&s, &mut |l: &mut [f64], r: &mut [f64]| {
            l.iter_mut().chain(r.iter_mut()).for_each(|v| *v = stage(*v))
        });
        let a = as_envelope::level(CEILING_DB);
        let h = crate::figures::sweep_harmonics(&s, &c).expect("aligned");
        let mut seen = 0;
        for [m, f, g] in h {
            if (100.0..=15_000.0).contains(&f) {
                let want = db(fourier(stage, a, m as usize) / a);
                assert!((g - want).abs() < 0.5, "{rate}: h{m} at {f} Hz reads {g}, not {want}");
                seen += 1;
            }
        }
        assert!(seen > 60, "{rate}: {seen}");
    }
}

#[test]
fn under_a_steady_tone_the_response_leaves_the_tone_out() {
    for rate in RATES {
        let st = Maker::new(rate, CEILING_DB).steady_sweep(100.0, -6.0, 1.0, -12.0, 10.0);
        let [c, _] = crate::run_model(&st, &mut |l: &mut [f64], r: &mut [f64]| {
            l.iter_mut().chain(r.iter_mut()).for_each(|v| *v *= 0.5)
        });
        let r = crate::figures::response(&st, &c, None).expect("aligned");
        assert!(!r.iter().any(|[f, _]| *f == 100.0), "the tone's point is left out");
        for [f, g] in &r {
            if (30.0..=15_000.0).contains(f) {
                assert!((g - db(0.5)).abs() < 0.05, "{rate}: {f} Hz reads {g}");
            }
        }
    }
}
