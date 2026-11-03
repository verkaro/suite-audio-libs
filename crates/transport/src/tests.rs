use std::time::{Duration, Instant};

use super::*;

#[test]
fn clock_sim() {
    let c = Clock::sim(48_000);
    assert_eq!(c.now(), Some(Position { state: State::Stopped, frame: 0, rate: 48_000 }));
    c.sim_roll(true);
    let t = Instant::now();
    std::thread::sleep(Duration::from_millis(500));
    let p = c.now().expect("a sim always reads");
    let want = t.elapsed().as_secs_f64() * 48_000.0;
    assert_eq!(p.state, State::Rolling);
    assert!((f64::from(p.frame) - 24_000.0).abs() <= 2_400.0, "{} frames", p.frame);
    assert!((f64::from(p.frame) - want).abs() <= 480.0, "{} frames, {want} by the watch", p.frame);
}

#[test]
fn clock_sim_locates_and_stops_where_it_is() {
    let c = Clock::sim(48_000);
    c.sim_locate(96_000);
    assert_eq!(c.now().map(|p| (p.state, p.frame)), Some((State::Stopped, 96_000)));
    c.sim_roll(true);
    c.sim_locate(48_000);
    std::thread::sleep(Duration::from_millis(100));
    c.sim_roll(false);
    let p = c.now().expect("reads");
    assert!(p.frame >= 48_000 + 3_000 && p.frame < 48_000 + 9_600, "{}", p.frame);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(c.now(), Some(p), "stopped holds still");
    c.sim_locate(u32::MAX);
    c.sim_roll(true);
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(c.now().map(|p| p.frame), Some(u32::MAX), "held at the last frame, not wrapped");
}

/// The clock reads and never moves: none of the calls that would appear in this crate's source.
#[test]
fn clock_never_controls() {
    for (file, src) in [("lib.rs", include_str!("lib.rs")), ("jack_clock.rs", include_str!("jack_clock.rs"))]
    {
        for bad in [".start(", ".stop(", ".locate(", ".reposition(", "register_port", "set_sync_callback"] {
            assert!(!src.contains(bad), "{file} contains {bad}");
        }
    }
}

#[test]
fn the_published_word_round_trips() {
    let s = jack_clock::Shared::default();
    assert_eq!(s.read(), None);
    for st in [State::Stopped, State::Rolling, State::Starting, State::Other] {
        let p = Position { state: st, frame: u32::MAX - 7, rate: 44_100 };
        s.publish(Some(p));
        assert_eq!(s.read(), Some(p));
    }
    s.publish(None);
    assert_eq!(s.read(), None);
}
