//! jackd started, found, watched and stopped, on a dummy server of the test's own,
//! `asjackdtest-<pid>`, never Warren's. Only with `SUITE_JACK_TEST=1`. Every jackd it starts is
//! stopped by its own pid, however the test ends.

use std::path::PathBuf;
use std::time::Duration;

use as_jackd::{Dummy, Follow, Monitor, Settings, Start, eventually, find, holder, log_path, run_start};
use jack::{AudioIn, ClientOptions};

/// Stops whatever jackd has this test's name, by its pid, when the test ends.
struct Leave;

impl Drop for Leave {
    fn drop(&mut self) {
        if let Some((pid, _)) = find() {
            // SAFETY: a signal to the jackd serving this test's own server name, by its pid.
            unsafe { libc::kill(pid, libc::SIGTERM) };
            eventually(Duration::from_secs(10), || find().is_none());
        }
    }
}

fn scratch() -> PathBuf {
    let d = std::env::temp_dir().join(format!("as-jackd-dummy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn on_a_dummy_server() {
    if std::env::var_os("SUITE_JACK_TEST").is_none() {
        eprintln!("skipped: set SUITE_JACK_TEST=1 to run it on a dummy JACK server");
        return;
    }
    let dir = scratch();
    // SAFETY: the only test in this binary, before any thread of ours starts.
    unsafe {
        std::env::set_var("JACK_DEFAULT_SERVER", Dummy::name("asjackd"));
        std::env::set_var("XDG_STATE_HOME", dir.join("state"));
    }
    assert!(find().is_none(), "nothing has this test's name yet");
    let _leave = Leave;
    let log = log_path();
    assert_eq!(log, dir.join("state/jack/jackd.log"));
    let secs = Duration::from_secs;

    // A start that fails says why, and leaves nothing running.
    let bad = Start {
        settings: Settings { driver: "nosuchdriver".into(), ..Settings::default() },
        ..Start::default()
    };
    let mut out = Vec::new();
    let e = as_jackd::start(&bad, "asjackd-probe", &log, &mut out).unwrap_err();
    assert!(e.contains("jackd stopped straight away"), "{e}");
    assert!(find().is_none());

    // Started from settings on the dummy driver, with a setup command run first.
    let flag = dir.join("setup-ran");
    let good = Start {
        settings: Settings { realtime: false, ..Settings::default() }.on_dummy(4, 2),
        interface: None,
        setup: vec![format!("touch {}", flag.display())],
    };
    let mut out = Vec::new();
    let from = as_jackd::start(&good, "asjackd-probe", &log, &mut out).unwrap();
    let said = String::from_utf8(out).unwrap();
    assert!(flag.exists() && said.contains("JACK is running"), "{said}");
    let (pid, cmd) = find().expect("found by its name");
    assert!(cmd.contains(&format!("-n {}", Dummy::name("asjackd"))) && cmd.contains("-d dummy"), "{cmd}");
    assert_eq!(as_jackd::running_driver().as_deref(), Some("dummy"));
    assert_eq!(as_jackd::running_alsa_device(), None);
    assert_eq!(as_jackd::running_io(), Some((0, 0)));
    assert!(as_jackd::started(pid).is_some());
    assert_eq!(run_start(&log), from, "this run's marker, as start left it");
    let mut follow = Follow::new(&log, from);
    assert!(follow.lines()[0].starts_with("=== "), "the marker first, the banner left out");

    // Watched: its rate and period, a client coming and going.
    let m = Monitor::open("asjackd-watch").unwrap();
    assert_eq!((m.rate, m.period), (48_000, 512));
    assert!(!m.shut_down());
    let graph = m.graph();
    let (visitor, _) = jack::Client::new("visitor", ClientOptions::NO_START_SERVER).unwrap();
    let _port = visitor.register_port("in", AudioIn::default()).unwrap();
    assert!(eventually(secs(3), || m.clients().contains(&"visitor".to_string())));
    assert!(eventually(secs(3), || m.graph() > graph));
    assert_eq!(holder("asjackd-ask", &["dexter", "visitor"]).as_deref(), Some("visitor"));
    assert_eq!(holder("asjackd-ask", &["dexter"]), None);

    // Refused while a named client is attached; stopped once it's gone.
    let e = as_jackd::stop("asjackd-stop", &["visitor"], false).unwrap_err();
    assert!(e.contains("visitor is attached"), "{e}");
    assert!(find().is_some(), "still running");
    drop(_port);
    drop(visitor);
    assert!(eventually(secs(3), || !m.clients().contains(&"visitor".to_string())));
    as_jackd::stop("asjackd-stop", &["visitor"], false).unwrap();
    assert!(find().is_none(), "gone");
    assert!(eventually(secs(5), || m.shut_down()), "the monitor saw it go");
    drop(m);
    assert_eq!(as_jackd::stop("asjackd-stop", &[], false).unwrap_err(), "no jackd is running");

    // The tests' own server, stopped by its pid when dropped.
    let d = Dummy::start("asjackd");
    let pid = d.pid().unwrap() as i32;
    assert_eq!(find().map(|(p, _)| p), Some(pid));
    drop(d);
    assert!(find().is_none(), "dropped and stopped");

    let _ = std::fs::remove_dir_all(&dir);
}
