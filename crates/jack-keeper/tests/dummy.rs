//! The keeper on a dummy JACK server of its own (never Warren's): silent until sent something,
//! its value at a probe's inputs, a connection cut, and the server stopped and started again
//! under it, which reattaches and stays silent until asked. Only with `SUITE_JACK_TEST=1`.
//! place's (`VENDORED.md`), with a player of its own.
//!
//! The server is `suitetest-<pid>` (`jackd -n suitetest-<pid> -r -d dummy -r 48000 -p 512 -C 16
//! -P 2`), stopped by its own pid.

use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use as_jack_keeper::{Audio, Player, links};
use jack::{AudioIn, ClientOptions};

/// A dummy jackd, stopped by its pid when dropped.
struct Jackd {
    child: Option<Child>,
}

impl Jackd {
    fn start(name: &str) -> Jackd {
        let child = Command::new("jackd")
            .args(["-n", name, "-r", "-d", "dummy", "-r", "48000", "-p", "512", "-C", "16", "-P", "2"])
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

/// An input's last sample, as f32 bits.
type Cell = Arc<AtomicU32>;

fn get(c: &Cell) -> f32 {
    f32::from_bits(c.load(Ordering::Relaxed))
}

type Process = Box<dyn FnMut(&jack::Client, &jack::ProcessScope) -> jack::Control + Send>;
type Client = jack::AsyncClient<(), jack::contrib::ClosureProcessHandler<(), Process>>;

/// The probe: in_L and in_R, each's last sample.
fn probe() -> (Client, [Cell; 2]) {
    let (c, _) =
        jack::Client::new("probe", ClientOptions::NO_START_SERVER | ClientOptions::USE_EXACT_NAME).unwrap();
    let ins: Vec<_> =
        ["in_L", "in_R"].iter().map(|n| c.register_port(n, AudioIn::default()).unwrap()).collect();
    let cells: [Cell; 2] = std::array::from_fn(|_| Arc::new(AtomicU32::new(0)));
    let out = cells.clone();
    let cb: Process = Box::new(move |_, ps| {
        for (p, c) in ins.iter().zip(&out) {
            let s = p.as_slice(ps);
            c.store(s[s.len() - 1].to_bits(), Ordering::Relaxed);
        }
        jack::Control::Continue
    });
    (c.activate_async((), jack::contrib::ClosureProcessHandler::new(cb)).unwrap(), cells)
}

fn connected(a: &str, b: &str) -> bool {
    let Ok((c, _)) = jack::Client::new("look", ClientOptions::NO_START_SERVER) else { return false };
    c.port_by_name(a).is_some_and(|p| p.is_connected_to(b).unwrap_or(false))
}

fn exists(port: &str) -> bool {
    let Ok((c, _)) = jack::Client::new("look", ClientOptions::NO_START_SERVER) else { return false };
    c.port_by_name(port).is_some()
}

fn near(v: f32, want: f32) -> bool {
    (v - want).abs() < 1e-6
}

/// A steady value, left, and its opposite, right.
struct Steady(f32);

impl Player for Steady {
    type In = f32;
    type Out = ();
    fn take(&mut self, v: f32) {
        self.0 = v;
    }
    fn period(&mut self, l: &mut [f32], r: &mut [f32]) {
        l.fill(self.0);
        r.fill(-self.0);
    }
    fn done(&mut self) -> Option<()> {
        None
    }
}

#[test]
fn on_a_dummy_server() {
    if std::env::var_os("SUITE_JACK_TEST").is_none() {
        eprintln!("skipped: set SUITE_JACK_TEST=1 to run it on a dummy JACK server");
        return;
    }
    let name = format!("suitetest-{}", std::process::id());
    // SAFETY: the only test in this binary, before any thread of ours starts.
    unsafe { std::env::set_var("JACK_DEFAULT_SERVER", &name) };
    let mut jackd = Jackd::start(&name);
    let (pr, cells) = probe();
    let mut pr = Some(pr);
    let [l, r] = &cells;
    let secs = Duration::from_secs;
    let to = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let both = || links("keeper", &to(&["probe:in_L"]), &to(&["probe:in_R"]));

    let audio = Audio::jack("keeper", both(), || Steady(0.0));
    assert!(eventually(secs(5), || audio.attached()), "attached");
    assert_eq!(audio.rate(), 48_000);
    assert!(eventually(secs(3), || connected("keeper:out_L", "probe:in_L")
        && connected("keeper:out_R", "probe:in_R")));
    assert!(!connected("keeper:out_L", "system:playback_1"), "connected nowhere else");
    let (_, ports) = audio.ports();
    assert_eq!(ports[0].name, "system:playback_1", "the sound card's first");
    assert!(!ports.iter().any(|p| p.name.starts_with("keeper:") || p.name.starts_with("system:capture")));

    // Nothing comes out until it's sent something.
    std::thread::sleep(Duration::from_millis(300));
    assert!(near(get(l), 0.0) && near(get(r), 0.0));
    audio.send(0.5);
    assert!(eventually(secs(3), || near(get(l), 0.5) && near(get(r), -0.5)), "{} {}", get(l), get(r));

    // A connection cut.
    audio.want(links("keeper", &[], &to(&["probe:in_R"])));
    assert!(eventually(secs(3), || !connected("keeper:out_L", "probe:in_L")), "cut");
    assert!(connected("keeper:out_R", "probe:in_R"), "the other kept");
    audio.want(both());

    // The server stops under it, and starts again: it reattaches by itself, with a new player,
    // silent until sent something.
    jackd.stop();
    assert!(eventually(secs(5), || !audio.attached()), "noticed JACK going away");
    // The probe went with it: closing it would wait on a server that's gone.
    std::mem::forget(pr.take());
    let mut jackd = Jackd::start(&name);
    let (pr2, cells) = probe();
    assert!(eventually(secs(5), || audio.attached()), "reattached");
    assert!(eventually(secs(3), || connected("keeper:out_L", "probe:in_L")), "connected again");
    std::thread::sleep(Duration::from_millis(300));
    assert!(near(get(&cells[0]), 0.0), "not playing unasked");
    audio.send(0.25);
    assert!(eventually(secs(3), || near(get(&cells[0]), 0.25)), "playing again: {}", get(&cells[0]));

    audio.stop();
    assert!(eventually(secs(3), || !exists("keeper:out_L")), "closed");
    drop(pr2);
    jackd.stop();
}
