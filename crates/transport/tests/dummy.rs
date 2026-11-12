//! The clock on a dummy JACK server of its own (never Warren's): it follows a transport located
//! and started by a test client, and when the server is stopped under it reads `None`, never a
//! stale frame, then reattaches when the server comes back. Only with `SUITE_JACK_TEST=1`.
//!
//! The server is `as_jackd::Dummy`'s, `transporttest-<pid>`, stopped by its own pid. The test client ends the way `as-jack-keeper`'s do.

use std::time::Duration;

use as_jackd::{Dummy, eventually};
use as_transport::{Clock, State};
use jack::ClientOptions;

/// The dummy server is `transporttest-<pid>` (`as_jackd::Dummy`).
const TOOL: &str = "transport";

#[test]
fn on_a_dummy_server() {
    if std::env::var_os("SUITE_JACK_TEST").is_none() {
        eprintln!("skipped: set SUITE_JACK_TEST=1 to run it on a dummy JACK server");
        return;
    }
    let name = Dummy::name(TOOL);
    // SAFETY: the only test in this binary, before any thread of ours starts.
    unsafe { std::env::set_var("JACK_DEFAULT_SERVER", &name) };

    // The clock starts first: it waits for a server that isn't there yet, and reads None.
    let clock = Clock::jack("clocktest");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(clock.now(), None, "no server, no position");

    let mut jackd = Dummy::start(TOOL);
    assert!(eventually(Duration::from_secs(5), || clock.now().is_some()), "attached within a retry");
    let p = clock.now().unwrap();
    assert_eq!((p.state, p.rate), (State::Stopped, 48_000));

    // clock_follows: a test client locates to 96000 and starts.
    let (tc, _) = jack::Client::new("mover", ClientOptions::NO_START_SERVER).unwrap();
    tc.transport().locate(96_000).unwrap();
    assert!(eventually(Duration::from_millis(500), || clock.now().is_some_and(|p| p.frame == 96_000)));
    tc.transport().start().unwrap();
    assert!(
        eventually(Duration::from_millis(500), || clock
            .now()
            .is_some_and(|p| p.state == State::Rolling && p.frame >= 96_000)),
        "rolling from 96000: {:?}",
        clock.now()
    );
    std::thread::sleep(Duration::from_millis(500));
    let p = clock.now().unwrap();
    assert!(
        p.frame >= 96_000 + 20_000 && p.frame < 96_000 + 40_000,
        "advanced about half a second: {}",
        p.frame
    );
    tc.transport().stop().unwrap();
    assert!(eventually(Duration::from_millis(500), || clock
        .now()
        .is_some_and(|p| p.state == State::Stopped)));

    // clock_dies_and_returns: the server stops under it.
    tc.transport().start().unwrap();
    assert!(eventually(Duration::from_millis(500), || clock
        .now()
        .is_some_and(|p| p.state == State::Rolling)));
    jackd.stop();
    // Never a stale frame after this: once None, always None until the server is back.
    assert!(eventually(Duration::from_secs(2), || clock.now().is_none()), "None within 2 s");
    for _ in 0..20 {
        assert_eq!(clock.now(), None, "no stale frame");
        std::thread::sleep(Duration::from_millis(20));
    }
    // The test client went with the server: closing it would wait on one that's gone.
    std::mem::forget(tc);

    let mut jackd = Dummy::start(TOOL);
    assert!(eventually(Duration::from_secs(5), || clock.now().is_some()), "reattached within 5 s");
    assert_eq!(clock.now().map(|p| p.state), Some(State::Stopped), "a new server is stopped at 0");

    clock.stop();
    jackd.stop();
}
