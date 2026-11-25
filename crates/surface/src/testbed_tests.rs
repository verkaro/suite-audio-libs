use std::time::{Duration, Instant};

use super::*;

const RATE: u32 = 48_000;

/// The testbed at t0, and a clock to move it along.
fn bed() -> (Testbed, impl Fn(f64) -> Instant) {
    let t0 = Instant::now();
    (Testbed::new(RATE, t0), move |s: f64| t0 + Duration::from_secs_f64(s))
}

fn secs(s: f64) -> u64 {
    (s * RATE as f64).round() as u64
}

#[test]
fn two_strips_at_unity() {
    let (b, at) = bed();
    let s = b.state_at(at(0.0));
    assert_eq!((s.mode, s.position, s.rate), (Mode::Stop, 0, RATE));
    assert_eq!(s.strips.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["1", "2"]);
    let one = &s.strips[0];
    assert_eq!(one.fader, Level::UNITY);
    assert_eq!(one.has, Has::all());
    assert_eq!((one.pan, one.kind), (0, Kind::Track));
}

#[test]
fn the_transport_runs() {
    let (b, at) = bed();
    b.command_at(Cmd::Transport(Mode::Play), at(0.0)).unwrap();
    assert_eq!(b.state_at(at(2.0)).position, secs(2.0));
    b.command_at(Cmd::Transport(Mode::Ff), at(2.0)).unwrap();
    assert_eq!(b.state_at(at(3.0)).position, secs(2.0 + WIND));
    b.command_at(Cmd::Transport(Mode::Rew), at(3.0)).unwrap();
    // REW stops at zero.
    let s = b.state_at(at(10.0));
    assert_eq!((s.mode, s.position), (Mode::Stop, 0));
    b.command_at(Cmd::Transport(Mode::Rec), at(10.0)).unwrap();
    b.command_at(Cmd::Transport(Mode::Stop), at(11.0)).unwrap();
    let s = b.state_at(at(20.0));
    assert_eq!((s.mode, s.position), (Mode::Stop, secs(1.0)));
}

#[test]
fn refused_while_recording_as_dexter_does() {
    let (b, at) = bed();
    b.command_at(Cmd::Transport(Mode::Rec), at(0.0)).unwrap();
    for c in [Cmd::Transport(Mode::Ff), Cmd::Transport(Mode::Rew), Cmd::Rtz, Cmd::Locate(-1), Cmd::Locate(1)]
    {
        assert_eq!(b.command_at(c, at(1.0)).unwrap_err().code, "recording", "{c:?}");
    }
    assert_eq!(b.state_at(at(1.0)).mode, Mode::Rec);
    // PLAY punches out; marking and arming go on.
    b.command_at(Cmd::Mark, at(1.0)).unwrap();
    b.command_at(Cmd::Arm { n: 2, on: true }, at(1.0)).unwrap();
    let s = b.state_at(at(1.0));
    assert_eq!((s.strips[0].recording, s.strips[1].recording), (false, true));
    b.command_at(Cmd::Transport(Mode::Play), at(2.0)).unwrap();
    assert!(!b.state_at(at(2.0)).strips[1].recording);
}

#[test]
fn marks_and_locating() {
    let (b, at) = bed();
    b.command_at(Cmd::Transport(Mode::Play), at(0.0)).unwrap();
    for t in [1.0, 1.05, 3.0, 5.0] {
        b.command_at(Cmd::Mark, at(t)).unwrap();
    }
    assert_eq!(
        b.state_at(at(5.0)).marks,
        [secs(1.0), secs(3.0), secs(5.0)],
        "none within a tenth of another"
    );
    // Just past a mark, back goes to the one before; while playing, it plays on.
    b.command_at(Cmd::Locate(-1), at(5.05)).unwrap();
    let s = b.state_at(at(5.05));
    assert_eq!((s.mode, s.position), (Mode::Play, secs(3.0)));
    b.command_at(Cmd::Locate(1), at(5.05)).unwrap();
    assert_eq!(b.state_at(at(5.05)).position, secs(5.0));
    assert_eq!(b.command_at(Cmd::Locate(1), at(6.0)).unwrap_err().code, "no_mark");
    // Located from a wind, it stops there.
    b.command_at(Cmd::Transport(Mode::Ff), at(6.0)).unwrap();
    b.command_at(Cmd::Locate(-1), at(7.0)).unwrap();
    let s = b.state_at(at(9.0));
    assert_eq!((s.mode, s.position), (Mode::Stop, secs(5.0)));
    b.command_at(Cmd::Locate(-1), at(9.0)).unwrap();
    b.command_at(Cmd::Locate(-1), at(9.0)).unwrap();
    b.command_at(Cmd::Locate(-1), at(9.0)).unwrap();
    assert_eq!(b.state_at(at(9.0)).position, 0, "before the first mark, zero");
    b.command_at(Cmd::Transport(Mode::Play), at(9.0)).unwrap();
    b.command_at(Cmd::Rtz, at(10.0)).unwrap();
    let s = b.state_at(at(10.0));
    assert_eq!((s.mode, s.position), (Mode::Play, 0));
}

/// Unmark, as Dexter's: the mark within a tenth of a second of the counter, only stopped.
#[test]
fn unmarking() {
    let (b, at) = bed();
    b.command_at(Cmd::Transport(Mode::Play), at(0.0)).unwrap();
    b.command_at(Cmd::Mark, at(1.0)).unwrap();
    b.command_at(Cmd::Mark, at(2.0)).unwrap();
    assert_eq!(b.command_at(Cmd::Unmark, at(2.0)).unwrap_err().code, "rolling");
    b.command_at(Cmd::Transport(Mode::Rec), at(2.5)).unwrap();
    assert_eq!(b.command_at(Cmd::Unmark, at(2.5)).unwrap_err().code, "recording");
    b.command_at(Cmd::Transport(Mode::Stop), at(2.5)).unwrap();
    assert_eq!(b.command_at(Cmd::Unmark, at(2.5)).unwrap_err().code, "not_on_mark");
    b.command_at(Cmd::Locate(-1), at(3.0)).unwrap();
    b.command_at(Cmd::Unmark, at(3.0)).unwrap();
    assert_eq!(b.state_at(at(3.0)).marks, [secs(1.0)]);
}

/// The counter nudged: back not past zero, rolling on, stopping a wind; refused in REC.
#[test]
fn nudging() {
    let (b, at) = bed();
    b.command_at(Cmd::Nudge(250), at(0.0)).unwrap();
    assert_eq!(b.state_at(at(0.0)).position, secs(0.25));
    b.command_at(Cmd::Nudge(-1000), at(0.0)).unwrap();
    assert_eq!(b.state_at(at(0.0)).position, 0, "not past zero");
    b.command_at(Cmd::Transport(Mode::Play), at(0.0)).unwrap();
    b.command_at(Cmd::Nudge(-500), at(2.0)).unwrap();
    let s = b.state_at(at(3.0));
    assert_eq!((s.mode, s.position), (Mode::Play, secs(2.5)), "rolling on");
    b.command_at(Cmd::Transport(Mode::Ff), at(3.0)).unwrap();
    b.command_at(Cmd::Nudge(100), at(3.0)).unwrap();
    let s = b.state_at(at(4.0));
    assert_eq!((s.mode, s.position), (Mode::Stop, secs(2.6)), "a wind stops");
    b.command_at(Cmd::Transport(Mode::Rec), at(4.0)).unwrap();
    assert_eq!(b.command_at(Cmd::Nudge(100), at(4.0)).unwrap_err().code, "recording");
}

#[test]
fn strips_set() {
    let (b, at) = bed();
    let now = at(0.0);
    b.command_at(Cmd::Fader { n: 1, level: Level(-650) }, now).unwrap();
    b.command_at(Cmd::Fader { n: 2, level: Level(2000) }, now).unwrap();
    b.command_at(Cmd::Pan { n: 1, value: -40 }, now).unwrap();
    b.command_at(Cmd::Pan { n: 2, value: 9999 }, now).unwrap();
    b.command_at(Cmd::Mute { n: 1, on: true }, now).unwrap();
    b.command_at(Cmd::Solo { n: 2, on: true }, now).unwrap();
    let s = b.state_at(now);
    assert_eq!((s.strips[0].fader, s.strips[1].fader), (Level(-650), Level(600)), "+6 dB at most");
    assert_eq!(s.strips[0].pan, -40);
    assert_eq!(s.strips[1].pan, 100, "clamped to its range");
    assert!(s.strips[0].mute && !s.strips[0].solo && s.strips[1].solo);
    b.command_at(Cmd::Fader { n: 1, level: Level::OFF }, now).unwrap();
    assert_eq!(b.state_at(now).strips[0].fader, Level::OFF);
    for n in [0, 3] {
        assert_eq!(b.command_at(Cmd::Mute { n, on: true }, now).unwrap_err().code, "no_track");
    }
}
