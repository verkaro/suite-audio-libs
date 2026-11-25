use std::time::Instant;

use super::*;
use crate::desk::Desk;
use crate::testbed::Testbed;

fn level_at(p: f64) -> Level {
    Taper::DEXTER.level_at(p)
}

fn position(l: Level) -> f64 {
    Taper::DEXTER.position(l)
}

fn state() -> DeskState {
    Testbed::new(48_000, Instant::now()).state()
}

#[test]
fn transport() {
    let mut c = Controls::default();
    let mut s = state();
    assert_eq!(c.act(Input::PlayStop, &s), Some(Cmd::Transport(Mode::Play)));
    s.mode = Mode::Play;
    assert_eq!(c.act(Input::PlayStop, &s), Some(Cmd::Transport(Mode::Stop)), "a pause");
    s.mode = Mode::Rec;
    assert_eq!(c.act(Input::PlayStop, &s), Some(Cmd::Transport(Mode::Play)), "a punch-out, rolling on");
    s.mode = Mode::Ff;
    assert_eq!(c.act(Input::PlayStop, &s), Some(Cmd::Transport(Mode::Play)));
    assert_eq!(c.act(Input::Locate(-1), &s), Some(Cmd::Locate(-1)));
    // A mark where there's none within a tenth of a second; on one, it's removed.
    s.marks = vec![48_000];
    s.position = 48_000 + 4_700;
    assert_eq!(c.act(Input::MarkToggle, &s), Some(Cmd::Unmark));
    s.position = 48_000 + 4_800;
    assert_eq!(c.act(Input::MarkToggle, &s), Some(Cmd::Mark));
    // Nudged a tenth of a second a step; fine, a hundredth.
    assert_eq!(c.act(Input::Nudge { steps: -2, fine: false }, &s), Some(Cmd::Nudge(-200)));
    assert_eq!(c.act(Input::Nudge { steps: 1, fine: true }, &s), Some(Cmd::Nudge(10)));
}

/// Linked strips move alike: a fader by as many dB, the knob by as many steps, each keeping
/// its offset; PAN back on all; MUTE, SOLO and ARM set as the selected one.
#[test]
fn linking() {
    let mut c = Controls::default();
    let mut s = state();
    s.strips[1].fader = Level(-1000);
    s.strips[1].pan = 30;
    // Alone in the link, a strip moves alone.
    assert_eq!(c.act_all(Input::Link, &s), []);
    assert_eq!(c.linked, [1]);
    assert_eq!(c.act_all(Input::Mute, &s), [Cmd::Mute { n: 1, on: true }]);
    c.act_all(Input::Select(2), &s);
    c.act_all(Input::Link, &s);
    assert_eq!(c.linked, [1, 2]);
    // From strip 2: the knob, the fader, MUTE, PAN back.
    assert_eq!(
        c.act_all(Input::Knob { steps: 1, fine: false }, &s),
        [Cmd::Pan { n: 2, value: 35 }, Cmd::Pan { n: 1, value: 5 }]
    );
    let to = level_at(position(Level(-1000)) + 0.1);
    assert_eq!(
        c.act_all(Input::FaderBy(0.1), &s),
        [Cmd::Fader { n: 2, level: to }, Cmd::Fader { n: 1, level: Level(to.0 + 1000) }],
        "10 dB apart, as they were"
    );
    s.strips[0].mute = true;
    assert_eq!(
        c.act_all(Input::Mute, &s),
        [Cmd::Mute { n: 2, on: true }, Cmd::Mute { n: 1, on: true }],
        "alike"
    );
    assert_eq!(c.act_all(Input::PanCentre, &s), [Cmd::Pan { n: 2, value: 0 }, Cmd::Pan { n: 1, value: 0 }]);
    // A strip without the control is passed over (Dexter's have no faders).
    s.strips[0].has.fader = false;
    assert_eq!(c.act_all(Input::FaderBy(0.1), &s).len(), 1);
    // The selected strip without it: nothing at all.
    s.strips[1].has.fader = false;
    s.strips[1].has.solo = false;
    assert_eq!(c.act_all(Input::FaderBy(0.1), &s), []);
    assert_eq!(c.act_all(Input::Solo, &s), []);
    // Out of the link, strip 2 moves alone again.
    c.act_all(Input::Link, &s);
    assert_eq!(c.linked, [1]);
    assert_eq!(c.act_all(Input::Arm, &s), [Cmd::Arm { n: 2, on: true }]);
}

/// Linked faders keep their balance in dB past the ends: one held at −∞ or +6 comes back
/// off it, the balance as it was, when the others come back. Forgotten when it's unlinked,
/// or moved by another.
#[test]
fn a_linked_balance_held_past_the_ends() {
    let mut c = Controls::default();
    let mut s = state();
    s.strips[0].fader = Level(-1000);
    for i in [Input::Link, Input::Select(2), Input::Link] {
        c.act_all(i, &s);
    }
    // Strip 2 from 0 dB, strip 1 10 dB under it; each move as the desk takes it. The faders,
    // strip 1's and strip 2's.
    let fader = |c: &mut Controls, s: &mut DeskState, l: Level| {
        for cmd in c.act_all(Input::Fader(position(l)), s) {
            if let Cmd::Fader { n, level } = cmd {
                s.strips[n - 1].fader = level;
            }
        }
        (s.strips[0].fader, s.strips[1].fader)
    };
    assert_eq!(fader(&mut c, &mut s, Level::OFF), (Level::OFF, Level::OFF), "both down");
    assert_eq!(fader(&mut c, &mut s, Level(-2000)), (Level(-3000), Level(-2000)), "back, 10 dB under");
    assert_eq!(fader(&mut c, &mut s, Level(600)), (Level(-400), Level(600)));
    // Strip 1 selected, up: strip 2 held at +6, then back off it, 10 dB over.
    c.act_all(Input::Select(1), &s);
    assert_eq!(fader(&mut c, &mut s, Level(500)), (Level(500), Level(600)), "held at the top");
    assert_eq!(fader(&mut c, &mut s, Level(-1000)), (Level(-1000), Level(0)), "back off it, 10 dB over");
    // Moved by another (the mixer's page): its balance is where it's put.
    s.strips[1].fader = Level(-2000);
    assert_eq!(fader(&mut c, &mut s, Level(-500)), (Level(-500), Level(-1500)));
    // Unlinked and linked again: forgotten.
    c.act_all(Input::Link, &s);
    assert!(c.held.iter().all(|h| h.0 != 1));
}

#[test]
fn selection_and_toggles() {
    let mut c = Controls::default();
    let mut s = state();
    assert_eq!(c.act(Input::Arm, &s), Some(Cmd::Arm { n: 1, on: true }));
    assert_eq!(c.act(Input::Next(1), &s), None);
    assert_eq!(c.strip, 2);
    assert_eq!(c.act(Input::Next(1), &s), None);
    assert_eq!(c.strip, 2, "no strip 3");
    s.strips[1].mute = true;
    assert_eq!(c.act(Input::Mute, &s), Some(Cmd::Mute { n: 2, on: false }));
    assert_eq!(c.act(Input::Solo, &s), Some(Cmd::Solo { n: 2, on: true }));
    assert_eq!(c.act(Input::Select(9), &s), None);
    assert_eq!(c.strip, 2);
    c.act(Input::Select(1), &s);
    assert_eq!(c.strip, 1);
}

#[test]
fn the_fader() {
    let mut c = Controls::default();
    let s = state();
    assert_eq!(c.act(Input::Fader(0.8), &s), Some(Cmd::Fader { n: 1, level: Level::UNITY }));
    assert_eq!(c.act(Input::Fader(0.0), &s), Some(Cmd::Fader { n: 1, level: Level::OFF }));
    assert_eq!(
        c.act(Input::FaderBy(0.25), &s),
        Some(Cmd::Fader { n: 1, level: Level(600) }),
        "+6 at the top"
    );
    assert_eq!(c.act(Input::FaderBy(-0.25), &s), Some(Cmd::Fader { n: 1, level: Level(-1560) }));
    assert_eq!(c.act(Input::Unity, &s), Some(Cmd::Fader { n: 1, level: Level::UNITY }));
}

#[test]
fn the_knob() {
    let mut c = Controls::default();
    let mut s = state();
    let pan = |c: &mut Controls, s: &DeskState, i| match c.act(i, s) {
        Some(Cmd::Pan { value, .. }) => value,
        other => panic!("{other:?}"),
    };
    assert_eq!(pan(&mut c, &s, Input::Knob { steps: 3, fine: false }), 15, "five hundredths a step");
    assert_eq!(pan(&mut c, &s, Input::Knob { steps: -2, fine: true }), -2, "a hundredth fine");
    s.strips[0].pan = 40;
    assert_eq!(pan(&mut c, &s, Input::PanCentre), 0);
    assert_eq!(turned(98, 1, false), 100, "within its range");
    assert_eq!(turned(0, -1, true), -1);

    // A strip without PAN (an output's master level): the knob does nothing there.
    s.strips[0].has.pan = false;
    assert_eq!(c.act(Input::Knob { steps: 1, fine: false }, &s), None);
    assert_eq!(c.act(Input::PanCentre, &s), None);
}
