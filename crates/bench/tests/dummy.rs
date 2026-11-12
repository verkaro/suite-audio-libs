//! The bench on a dummy JACK server of its own (`as_jackd::Dummy`, `benchtest-<pid>`), never
//! Warren's. Only with `SUITE_JACK_TEST=1`. One test, so one server: the loopback, a known unit
//! between the ports, and an xrun forced by a slow client. The test patches; the bench never does.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use as_bench::check::{Calibration, Capture, check};
use as_bench::figures;
use as_bench::jack::{Bench, IN, OUT};
use as_bench::signal::{CEILING_DB, Maker, Signal};
use as_jackd::Dummy;
use jack::{AudioIn, AudioOut, ClientOptions, Control, Port, ProcessScope};

/// A feed-forward compressor a sample at a time: a peak over the last 49 samples, a soft knee,
/// its gain smoothed in dB by one pole each way.
#[derive(Clone)]
struct Comp {
    ring: [f64; 49],
    at: usize,
    g: f64,
    aa: f64,
    ar: f64,
}

impl Comp {
    fn new(rate: f64) -> Comp {
        Comp {
            ring: [0.0; 49],
            at: 0,
            g: 0.0,
            aa: (-1.0 / (0.020 * rate)).exp(),
            ar: (-1.0 / (0.100 * rate)).exp(),
        }
    }

    fn target(x: f64) -> f64 {
        let (t, r, w) = (-30.0, 4.0, 6.0);
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

    fn tick(&mut self, x: f64) -> f64 {
        self.ring[self.at] = x.abs();
        self.at = (self.at + 1) % self.ring.len();
        let peak = self.ring.iter().fold(0.0f64, |m, v| m.max(*v));
        let want = Comp::target(20.0 * peak.max(1e-15).log10());
        let a = if want < self.g { self.aa } else { self.ar };
        self.g = want + a * (self.g - want);
        x * 10f64.powf(self.g / 20.0)
    }
}

/// The unit between the ports: `unit:in` through the compressor to `unit:out`.
struct Unit {
    i: Port<AudioIn>,
    o: Port<AudioOut>,
    c: Comp,
}

impl jack::ProcessHandler for Unit {
    fn process(&mut self, _: &jack::Client, ps: &ProcessScope) -> Control {
        let (i, o) = (self.i.as_slice(ps), self.o.as_mut_slice(ps));
        for (y, x) in o.iter_mut().zip(i) {
            *y = self.c.tick(*x as f64) as f32;
        }
        Control::Continue
    }
}

/// A client that, once asked, takes 60 ms over one period.
struct Slow(Arc<AtomicBool>);

impl jack::ProcessHandler for Slow {
    fn process(&mut self, _: &jack::Client, _: &ProcessScope) -> Control {
        if self.0.swap(false, Ordering::AcqRel) {
            std::thread::sleep(Duration::from_millis(60));
        }
        Control::Continue
    }
}

fn client(name: &str) -> jack::Client {
    jack::Client::new(name, ClientOptions::NO_START_SERVER).expect("a client").0
}

fn take(bench: &mut Bench, s: &Signal) -> Capture {
    let [l, _] = bench.capture(s, Duration::from_secs(5)).expect("captured");
    l
}

#[test]
fn on_a_dummy_server() {
    if std::env::var("SUITE_JACK_TEST").as_deref() != Ok("1") {
        eprintln!("skipped: set SUITE_JACK_TEST=1 to run on a dummy JACK server");
        return;
    }
    // SAFETY: set before any thread of the test's own starts.
    unsafe { std::env::set_var("JACK_DEFAULT_SERVER", Dummy::name("bench")) };
    let _server = Dummy::start("bench");
    let mut bench = Bench::open("bench", |m| eprintln!("{m}")).expect("opened");
    let rate = bench.rate();
    let m = Maker::new(rate, CEILING_DB);
    let patch = client("patch");
    let port = |c: &str, p: &str| format!("{c}:{p}");

    // Connected nowhere: what comes back is silence, and the check says no mark.
    let st = m.steps(&[-40.0, -30.0, -20.0, -10.0, 0.0], 0.3);
    let c = take(&mut bench, &st);
    assert!(c.samples.iter().all(|v| *v == 0.0), "the bench connected nothing");
    assert!(check(&st, &c, None).failed().contains(&"aligned"));

    // The loopback: the same latency every time, and 0 dB.
    for (o, i) in OUT.iter().zip(IN) {
        patch.connect_ports_by_name(&port("bench", o), &port("bench", i)).expect("patched");
    }
    let (sil, sw) = (m.silence(1.0), m.sweep(10.0, 0.0));
    let (cs, cw, ct) = (take(&mut bench, &sil), take(&mut bench, &sw), take(&mut bench, &st));
    let cal: Calibration =
        as_bench::check::calibrate("loopback", (&sil, &cs), (&sw, &cw), (&st, &ct)).expect("calibrated");
    assert!(cal.gain_db.abs() < 0.001, "gain {}", cal.gain_db);
    for _ in 0..3 {
        let c = take(&mut bench, &st);
        let r = check(&st, &c, Some(&cal));
        assert!(r.pass(), "{r}");
        assert_eq!(r.found.expect("found").latency, cal.latency);
    }

    // A known unit between them: its figures as offline.
    for (o, i) in OUT.iter().zip(IN) {
        patch.disconnect_ports_by_name(&port("bench", o), &port("bench", i)).expect("unpatched");
    }
    let u = client("unit");
    let (i, o) = (
        u.register_port("in", AudioIn::default()).expect("in"),
        u.register_port("out", AudioOut::default()).expect("out"),
    );
    let unit = u.activate_async((), Unit { i, o, c: Comp::new(rate as f64) }).expect("unit runs");
    patch.connect_ports_by_name("bench:bench_out_L", "unit:in").expect("patched");
    patch.connect_ports_by_name("unit:out", "bench:bench_in_L").expect("patched");
    let offline = |s: &Signal| {
        let mut c = Comp::new(rate as f64);
        // As JACK has it: f32 in and out.
        as_bench::run_model(s, &mut |l: &mut [f64], _: &mut [f64]| {
            l.iter_mut().for_each(|v| *v = c.tick(*v as f32 as f64) as f32 as f64)
        })[0]
            .clone()
    };
    let jc = take(&mut bench, &st);
    assert!(check(&st, &jc, None).pass(), "{}", check(&st, &jc, None));
    let (a, b) = (
        figures::curve(&st, &jc, None).expect("aligned"),
        figures::curve(&st, &offline(&st), None).expect("aligned"),
    );
    for (p, q) in a.iter().zip(&b) {
        assert!(
            (p.out_db - q.out_db).abs() < 0.05,
            "{} in: {} by JACK, {} offline",
            p.in_db,
            p.out_db,
            q.out_db
        );
    }
    let rest = figures::rest_gain(&a);
    let bu = m.bursts(-40.0, 0.6, -10.0, 0.3, 3);
    let tj = figures::times(&bu, &take(&mut bench, &bu), None, rest).expect("aligned");
    let to = figures::times(&bu, &offline(&bu), None, rest).expect("aligned");
    for (x, y) in [(tj.attack_mean, to.attack_mean), (tj.release_mean, to.release_mean)] {
        assert!((x / y - 1.0).abs() < 0.02, "{x} by JACK, {y} offline");
    }
    drop(unit);

    // An xrun forced: the check says so.
    let ask = Arc::new(AtomicBool::new(false));
    let slow = client("slow").activate_async((), Slow(ask.clone())).expect("slow runs");
    for (o, i) in OUT.iter().zip(IN) {
        patch.connect_ports_by_name(&port("bench", o), &port("bench", i)).expect("patched");
    }
    let s = m.silence(0.5);
    let t = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        ask.store(true, Ordering::Release);
    });
    let c = take(&mut bench, &s);
    t.join().expect("joined");
    assert!(c.xruns > 0, "no xrun counted");
    assert!(check(&s, &c, Some(&cal)).failed().contains(&"xruns"));
    drop(slow);
    bench.close();
}
