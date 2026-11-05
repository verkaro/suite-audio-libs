//! The capture on a dummy JACK server of its own (never Warren's), with a client playing a
//! counter into it. Only with `SUITE_JACK_TEST=1`. The server is `suitetest-<pid>` (`jackd -n
//! suitetest-<pid> -r -d dummy -r 48000 -p 512 -C 2 -P 2`), stopped by its own pid.

use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use as_jack_capture::{Capture, Chunk, links};
use jack::{AudioOut, ClientOptions};

/// The tests set one process-wide environment variable: one at a time.
static ONE: Mutex<()> = Mutex::new(());

struct Jackd {
    child: Option<Child>,
}

impl Jackd {
    fn start(name: &str) -> Jackd {
        let child = Command::new("jackd")
            .args(["-n", name, "-r", "-d", "dummy", "-r", "48000", "-p", "512", "-C", "2", "-P", "2"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("jackd (jackd2) is installed");
        let j = Jackd { child: Some(child) };
        assert!(
            eventually(Duration::from_secs(10), || jack::Client::new("up", ClientOptions::NO_START_SERVER)
                .is_ok()),
            "the dummy server started"
        );
        j
    }

    fn stop(&mut self) {
        if let Some(mut c) = self.child.take() {
            // SAFETY: a signal to our own child, by its pid.
            unsafe { libc::kill(c.id() as i32, libc::SIGTERM) };
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(10) {
                if c.try_wait().ok().flatten().is_some() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

impl Drop for Jackd {
    fn drop(&mut self) {
        self.stop();
    }
}

fn eventually(within: Duration, mut f: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < within {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

type Process = Box<dyn FnMut(&jack::Client, &jack::ProcessScope) -> jack::Control + Send>;
type Source = jack::AsyncClient<(), jack::contrib::ClosureProcessHandler<(), Process>>;

const STEP: f32 = 1.0 / 16_777_216.0;

/// A client `src` playing a counter: left n / 2^24, right its negative, n from 1, one more a
/// frame, exact in a float.
fn source() -> Source {
    let (c, _) =
        jack::Client::new("src", ClientOptions::NO_START_SERVER | ClientOptions::USE_EXACT_NAME).unwrap();
    let mut outs = [
        c.register_port("out_L", AudioOut::default()).unwrap(),
        c.register_port("out_R", AudioOut::default()).unwrap(),
    ];
    let n = Arc::new(AtomicU64::new(1));
    let cb: Process = Box::new(move |_, ps| {
        let base = n.fetch_add(ps.n_frames() as u64, Ordering::Relaxed);
        let [l, r] = &mut outs;
        for (i, (a, b)) in l.as_mut_slice(ps).iter_mut().zip(r.as_mut_slice(ps)).enumerate() {
            let v = (base + i as u64) as f32 * STEP;
            *a = v;
            *b = -v;
        }
        jack::Control::Continue
    });
    c.activate_async((), jack::contrib::ClosureProcessHandler::new(cb)).unwrap()
}

fn want() -> std::collections::BTreeSet<(String, String)> {
    links("cap", &["src:out_L".to_string()], &["src:out_R".to_string()])
}

/// With a dummy server running, `f` on it. Skipped without `SUITE_JACK_TEST`.
fn with_server(f: impl FnOnce(&mut Option<Jackd>, &str)) {
    if std::env::var_os("SUITE_JACK_TEST").is_none() {
        eprintln!("skipped: set SUITE_JACK_TEST=1 to run it on a dummy JACK server");
        return;
    }
    let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
    let name = format!("suitetest-{}", std::process::id());
    // SAFETY: serialised by ONE; the harness's other threads are idle.
    unsafe { std::env::set_var("JACK_DEFAULT_SERVER", &name) };
    let mut jackd = Some(Jackd::start(&name));
    f(&mut jackd, &name);
}

/// Reads for `secs`, returning every frame, and what came that wasn't frames.
fn read_for(c: &mut Capture, secs: f64) -> (Vec<f32>, Vec<f32>, Vec<Chunk>) {
    let (mut l, mut r, mut other) = (vec![], vec![], vec![]);
    let start = Instant::now();
    while start.elapsed().as_secs_f64() < secs {
        match c.read(&mut l, &mut r, 4096) {
            Chunk::Frames(_) => {}
            o => other.push(o),
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    (l, r, other)
}

/// The frames from the first one both sides carry (the two connections are made one after the
/// other), each one step past the last, both sides.
fn assert_consecutive(l: &[f32], r: &[f32]) -> usize {
    let first =
        l.iter().zip(r).position(|(a, b)| *a != 0.0 && *b != 0.0).expect("the source's frames arrived");
    let l = &l[first..];
    let r = &r[first..];
    for i in 1..l.len() {
        assert_eq!(l[i] - l[i - 1], STEP, "left, frame {i}");
        assert_eq!(r[i] - r[i - 1], -STEP, "right, frame {i}");
    }
    l.len()
}

#[test]
fn dummy_ramp() {
    with_server(|_jackd, _| {
        let src = source();
        let mut c = Capture::jack("cap", want(), 2.0);
        assert!(eventually(Duration::from_secs(5), || c.status().attached.load(Ordering::Acquire)));
        assert_eq!(c.status().rate.load(Ordering::Relaxed), 48_000);
        let (l, r, _) = read_for(&mut c, 2.5);
        let n = assert_consecutive(&l, &r);
        assert!(n > 48_000, "two seconds read: {n}");
        c.stop();
        drop(src);
    });
}

#[test]
fn dummy_gap() {
    with_server(|_jackd, _| {
        let src = source();
        let mut c = Capture::jack("cap", want(), 0.5);
        assert!(eventually(Duration::from_secs(5), || c.status().attached.load(Ordering::Acquire)));
        // Not read for longer than the ring holds.
        std::thread::sleep(Duration::from_millis(1500));
        assert!(c.status().dropped.load(Ordering::Relaxed) > 0, "frames were dropped");
        let (mut l, mut r) = (vec![], vec![]);
        assert_eq!(c.read(&mut l, &mut r, 4096), Chunk::Gap, "the loss, once, first");
        assert!(l.is_empty());
        let (l, r, other) = read_for(&mut c, 1.0);
        assert!(other.is_empty(), "no second gap: {other:?}");
        assert!(assert_consecutive(&l, &r) > 24_000);
        c.stop();
        drop(src);
    });
}

#[test]
fn dummy_reattach() {
    with_server(|jackd, name| {
        let src = source();
        let mut c = Capture::jack("cap", want(), 2.0);
        assert!(eventually(Duration::from_secs(5), || c.status().attached.load(Ordering::Acquire)));
        let (l, r, _) = read_for(&mut c, 0.5);
        assert!(assert_consecutive(&l, &r) > 10_000);

        // The server stops under it: Away, once what was in the ring has been read.
        jackd.as_mut().unwrap().stop();
        assert!(
            eventually(Duration::from_secs(5), || !c.status().attached.load(Ordering::Acquire)),
            "noticed"
        );
        // The source went with the server: closing it would wait on a server that's gone.
        std::mem::forget(src);
        let (mut l, mut r) = (vec![], vec![]);
        assert!(eventually(Duration::from_secs(3), || c.read(&mut l, &mut r, 4096) == Chunk::Away), "Away");

        *jackd = Some(Jackd::start(name));
        let src = source();
        assert!(
            eventually(Duration::from_secs(5), || c.status().attached.load(Ordering::Acquire)),
            "reattached"
        );
        assert_eq!(c.status().session.load(Ordering::Relaxed), 2);
        // The new ring: a Gap first, then frames that follow one another.
        let (mut l, mut r) = (vec![], vec![]);
        let mut first = None;
        assert!(eventually(Duration::from_secs(3), || {
            let o = c.read(&mut l, &mut r, 4096);
            if first.is_none() && o != Chunk::Away && o != Chunk::Frames(0) {
                first = Some(o);
            }
            first.is_some()
        }));
        assert_eq!(first, Some(Chunk::Gap));
        let (l, r, other) = read_for(&mut c, 1.0);
        assert!(other.is_empty(), "{other:?}");
        assert!(assert_consecutive(&l, &r) > 24_000);
        c.stop();
        drop(src);
    });
}
