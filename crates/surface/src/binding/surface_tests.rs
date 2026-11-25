use std::time::{Duration, Instant};

use crate::desk::{Cmd, Desk, Level};
use crate::testbed::Testbed;
use as_mcu::{FADERPORT_V2, STANDARD};

use super::*;

const PLAY: u8 = 0x5e;
const STOP: u8 = 0x5d;
const REC: u8 = 0x5f;
const REW: u8 = 0x5b;
const FF: u8 = 0x5c;
const SHIFT: u8 = 0x46;
const CH_LEFT: u8 = 0x30;
const CH_RIGHT: u8 = 0x31;
const PUSH: u8 = 0x20;
const ARM: u8 = 0x00;
const MUTE: u8 = 0x10;

fn state() -> DeskState {
    Testbed::new(48_000, Instant::now()).state()
}

fn press(b: &mut Binding, note: u8, at: Instant, s: &DeskState) -> Vec<Input> {
    b.event(&Event::Button { note, down: true }, at, s).into_iter().map(|d| d.0).collect()
}

fn release(b: &mut Binding, note: u8, at: Instant, s: &DeskState) -> Vec<Input> {
    b.event(&Event::Button { note, down: false }, at, s).into_iter().map(|d| d.0).collect()
}

fn ms(t: Instant, n: u64) -> Instant {
    t + Duration::from_millis(n)
}

#[test]
fn buttons() {
    let mut b = Binding::new(&STANDARD);
    let (s, t) = (state(), Instant::now());
    let rolling = DeskState { mode: Mode::Play, ..s.clone() };
    assert_eq!(press(&mut b, PLAY, t, &s), [Input::PlayStop], "PLAY, or a pause while rolling");
    assert_eq!(release(&mut b, PLAY, t, &s), []);
    assert_eq!(press(&mut b, STOP, t, &rolling), [Input::Stop]);
    assert_eq!(press(&mut b, STOP, t, &s), [Input::Rtz], "STOP while stopped");
    assert_eq!(press(&mut b, REC, t, &s), [Input::Rec]);
    assert_eq!(press(&mut b, CH_RIGHT, t, &s), [Input::Next(1)]);
    // The knob's push does its work when it's let go: held a second, PAN back; a tap, nothing.
    assert_eq!(press(&mut b, PUSH, t, &s), []);
    assert_eq!(release(&mut b, PUSH, ms(t, 900), &s), [], "a tap");
    assert_eq!(press(&mut b, PUSH, t, &s), []);
    assert_eq!(release(&mut b, PUSH, ms(t, 1000), &s), [Input::PanCentre], "held a second");
    assert_eq!(press(&mut b, ARM, t, &s), [Input::Arm]);
    assert_eq!(press(&mut b, MUTE, t, &s), [Input::Mute]);
    assert_eq!(press(&mut b, 0x7a, t, &s), [], "not on the table");
    // With SHIFT held: RTZ, locate, PAN reset, the knob fine.
    assert_eq!(press(&mut b, SHIFT, t, &s), []);
    assert_eq!(press(&mut b, STOP, t, &rolling), [Input::Rtz]);
    assert_eq!(press(&mut b, CH_LEFT, t, &s), [Input::Locate(-1)]);
    assert_eq!(press(&mut b, CH_RIGHT, t, &s), [Input::Locate(1)]);
    assert_eq!(press(&mut b, PUSH, t, &s), []);
    assert_eq!(release(&mut b, PUSH, t, &s), [Input::PanCentre]);
    let k = |b: &mut Binding, e| b.event(&e, t, &s).into_iter().map(|d| d.0).collect::<Vec<_>>();
    assert_eq!(k(&mut b, Event::Knob { n: 0, delta: -2 }), [Input::Knob { steps: -2, fine: true }]);
    assert_eq!(release(&mut b, SHIFT, t, &s), []);
    assert_eq!(k(&mut b, Event::Jog(3)), [Input::Locate(1)], "the jog wheel, mark to mark");
    assert_eq!(k(&mut b, Event::Jog(-1)), [Input::Locate(-1)]);
    assert_eq!(press(&mut b, 0x54, t, &s), [Input::MarkToggle], "MARKER");
    assert_eq!(k(&mut b, Event::Knob { n: 1, delta: 1 }), [], "only the first strip's");
    assert_eq!(press(&mut b, MUTE + 1, t, &s), [], "only the first strip's");
    // The button's note comes with what it does, to blink if refused.
    assert_eq!(b.event(&Event::Button { note: REC, down: true }, t, &s), [(Input::Rec, Some(REC))]);
}

/// The FaderPort V2 (Warren's capture and passes): no SHIFT to see, so SECTION marks, SHIFT
/// with PREV and NEXT (4C, 4F), or PREV and NEXT in MARKER's mode (54, 55), locate, BYPASS
/// is fine; and it roams the eight strips.
#[test]
fn the_faderport() {
    let mut b = Binding::new(&FADERPORT_V2);
    let (s, t) = (state(), Instant::now());
    assert_eq!(press(&mut b, 0x58, t, &s), [Input::MarkToggle], "SECTION");
    assert_eq!(press(&mut b, 0x50, t, &s), [Input::Link], "LINK");
    assert_eq!(press(&mut b, 0x4c, t, &s), [Input::Locate(-1)]);
    assert_eq!(press(&mut b, 0x4f, t, &s), [Input::Locate(1)]);
    assert_eq!(press(&mut b, 0x54, t, &s), [Input::Locate(-1)], "PREV in MARKER's mode");
    assert_eq!(press(&mut b, 0x55, t, &s), [Input::Locate(1)], "NEXT in MARKER's mode");
    // SCROLL latched with SHIFT: the knob a click through the timeline, PREV and NEXT marks.
    assert_eq!(press(&mut b, 0x60, t, &s), [Input::Locate(-1)]);
    assert_eq!(press(&mut b, 0x61, t, &s), [Input::Locate(1)]);
    assert_eq!(press(&mut b, 0x62, t, &s), [Input::Nudge { steps: -1, fine: false }]);
    assert_eq!(press(&mut b, 0x63, t, &s), [Input::Nudge { steps: 1, fine: false }]);
    assert_eq!(press(&mut b, 0x64, t, &s), [], "ZOOM, in and out of it");
    assert_eq!(press(&mut b, 0x46, t, &s), [], "no SHIFT");
    assert_eq!(press(&mut b, CH_RIGHT, t, &s), [], "no CHANNEL");
    let k = |b: &mut Binding, e| b.event(&e, t, &s).into_iter().map(|d| d.0).collect::<Vec<_>>();
    assert_eq!(press(&mut b, 0x4e, t, &s), [], "BYPASS: fine");
    assert_eq!(k(&mut b, Event::Knob { n: 0, delta: 1 }), [Input::Knob { steps: 1, fine: true }]);
    assert_eq!(press(&mut b, 0x4e, t, &s), [], "and back");
    assert_eq!(k(&mut b, Event::Knob { n: 0, delta: 1 }), [Input::Knob { steps: 1, fine: false }]);

    // NEXT: SELECT on strip 2, which selects it; then its MUTE, the knob, the fader.
    assert_eq!(press(&mut b, 0x19, t, &s), [Input::Select(2)]);
    assert_eq!(press(&mut b, MUTE + 1, t, &s), [Input::Mute]);
    assert_eq!(k(&mut b, Event::Knob { n: 1, delta: -1 }), [Input::Knob { steps: -1, fine: false }]);
    assert_eq!(k(&mut b, Event::Fader { n: 1, value: 0 }), [Input::Fader(0.0)]);
    // On to strip 3, which the testbed hasn't: nothing done there (the selection stays).
    assert_eq!(press(&mut b, 0x1a, t, &s), [Input::Select(3)]);
    assert_eq!(press(&mut b, MUTE + 2, t, &s), []);
    assert_eq!(k(&mut b, Event::Fader { n: 2, value: 8192 }), []);
    // Back to strip 1 by its fader alone (a touch came first).
    assert_eq!(k(&mut b, Event::Touch { n: 0, on: true }), [Input::Select(1)]);
}

/// The timeline's knob spun: clicks close together step 4 at once, doubling every four, to
/// 16; a pause or the other way starts again; fine, they don't grow.
#[test]
fn a_spin_takes_bigger_steps() {
    let mut b = Binding::new(&FADERPORT_V2);
    let (s, t) = (state(), Instant::now());
    let spin = |b: &mut Binding, note, from: u64, clicks: u64| -> Vec<i32> {
        (0..clicks)
            .flat_map(|i| press(b, note, ms(t, from + i * 100), &s))
            .map(|i| if let Input::Nudge { steps, .. } = i { steps } else { 0 })
            .collect()
    };
    let on = spin(&mut b, 0x63, 0, 22);
    assert_eq!(on, [1, 4, 4, 4, 4, 8, 8, 8, 8, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16]);
    // A flick the other way, as the FaderPort sends it: seven notes 100 ms apart, 3.3 s.
    let back = spin(&mut b, 0x62, 2200, 7);
    assert_eq!(back, [-1, -4, -4, -4, -4, -8, -8], "the other way");
    assert_eq!(back.iter().sum::<i32>(), -33);
    assert_eq!(spin(&mut b, 0x62, 3000, 1), [-1], "after a pause");
    press(&mut b, 0x4e, t, &s); // BYPASS: fine
    assert_eq!(spin(&mut b, 0x63, 4000, 6), [1; 6]);
    assert_eq!(press(&mut b, 0x63, ms(t, 4600), &s), [Input::Nudge { steps: 1, fine: true }]);
}

/// A roaming surface is sent its strip's LEDs and motor on the strip it's on, the selected
/// strip's place in its bank of eight; past the desk's last strip, it's sent back.
#[test]
fn the_faderport_roams() {
    let bed = Testbed::new(48_000, Instant::now());
    let mut b = Binding::new(&FADERPORT_V2);
    let mut c = Controls::default();
    let t = Instant::now();
    bed.command(Cmd::Mute { n: 2, on: true }).unwrap();
    bed.command(Cmd::Fader { n: 2, level: Level(-1000) }).unwrap();
    b.outs(t, &bed.state(), &c);
    let go = |b: &mut Binding, c: &mut Controls, note: u8, at| {
        for (i, _) in b.event(&Event::Button { note, down: true }, at, &bed.state()) {
            c.act(i, &bed.state());
        }
        b.outs(at, &bed.state(), c)
    };
    // NEXT, to strip 2: strip 1's lit LEDs dark, all of strip 2's on its notes, the motor
    // on E1.
    let o = go(&mut b, &mut c, 0x19, ms(t, 100));
    assert_eq!(c.strip, 2);
    let off = Lamp::Off;
    assert_eq!(
        leds(o.clone()),
        [(0x01, off), (0x09, off), (0x11, Lamp::On), (0x18, off), (0x19, Lamp::On), (0x21, off)]
    );
    assert!(o.contains(&Out::Motor { n: 1, value: 10376 }), "{o:?}");
    // NEXT, to strip 3: none there, so strip 2 stays, and every LED is sent again: SELECT
    // lit on strip 2 brings the FaderPort back.
    let o = go(&mut b, &mut c, 0x1a, ms(t, 200));
    assert_eq!(c.strip, 2);
    assert!(leds(o.clone()).contains(&(0x19, Lamp::On)), "{o:?}");
    assert!(leds(o).iter().all(|&(n, _)| n != 0x1a));
}

/// Past eight strips (Dexter's tracks): the FaderPort's place is the selected strip's in its
/// bank; PREV and NEXT past its ends (BANK ◀ ▶) go on to the next bank.
#[test]
fn banks_of_eight() {
    let mut s = state();
    s.strips = (0..10).map(|_| s.strips[0].clone()).collect();
    let mut b = Binding::new(&FADERPORT_V2);
    let mut c = Controls { strip: 8, ..Controls::default() };
    let t = Instant::now();
    // Each step a second after the last, so the motor's free to go.
    let mut at = 0;
    let mut go = |b: &mut Binding, c: &mut Controls, e: Event| {
        at += 1000;
        for (i, _) in b.event(&e, ms(t, at), &s) {
            c.act(i, &s);
        }
        b.outs(ms(t, at), &s, c)
    };
    let o = go(&mut b, &mut c, Event::Knob { n: 7, delta: 1 });
    assert!(leds(o.clone()).contains(&(0x1f, Lamp::On)), "strip 8: the eighth SELECT");
    // NEXT on the eighth: BANK ▶, strip 9, the first of the next bank: its SELECT, its motor.
    let o = go(&mut b, &mut c, Event::Button { note: 0x2f, down: true });
    assert_eq!(c.strip, 9);
    assert!(leds(o.clone()).contains(&(0x18, Lamp::On)) && leds(o.clone()).contains(&(0x1f, Lamp::Off)));
    assert!(o.iter().any(|o| matches!(o, Out::Motor { n: 0, .. })), "{o:?}");
    // Its second strip's MUTE is strip 10's.
    let done = b.event(&Event::Button { note: MUTE + 1, down: true }, t, &s);
    assert_eq!(done.iter().map(|d| d.0).collect::<Vec<_>>(), [Input::Select(10), Input::Mute]);
    c.act(Input::Select(10), &s);
    // PREV on the first: BANK ◀, back to strip 8.
    c.act(Input::Select(9), &s);
    go(&mut b, &mut c, Event::Knob { n: 0, delta: 1 });
    go(&mut b, &mut c, Event::Button { note: 0x2e, down: true });
    assert_eq!(c.strip, 8);
}

#[test]
fn rew_and_ff_tap_or_hold() {
    let mut b = Binding::new(&STANDARD);
    let (mut s, t) = (state(), Instant::now());
    // A tap latches: nothing on the release.
    assert_eq!(press(&mut b, FF, t, &s), [Input::Ff]);
    s.mode = Mode::Ff;
    assert_eq!(release(&mut b, FF, ms(t, 200), &s), []);
    // Held, it stops on the release.
    assert_eq!(press(&mut b, REW, ms(t, 1000), &s), [Input::Rew]);
    s.mode = Mode::Rew;
    assert_eq!(release(&mut b, REW, ms(t, 1000 + 350), &s), [Input::Stop]);
    // Held, but the wind already ended (at zero, or PLAY pressed): nothing.
    assert_eq!(press(&mut b, REW, ms(t, 2000), &s), [Input::Rew]);
    s.mode = Mode::Stop;
    assert_eq!(release(&mut b, REW, ms(t, 3000), &s), []);
}

#[test]
fn the_motor_never_fights_the_hand() {
    let bed = Testbed::new(48_000, Instant::now());
    let mut b = Binding::new(&FADERPORT_V2);
    let c = Controls::default();
    let t = Instant::now();
    let motor = |o: Vec<Out>| {
        o.into_iter().find_map(|o| if let Out::Motor { value, .. } = o { Some(value) } else { None })
    };
    // First: everything sent, the motor to 0 dB (three quarters of the way).
    assert_eq!(motor(b.outs(t, &bed.state(), &c)), Some(13106));
    assert_eq!(motor(b.outs(ms(t, 100), &bed.state(), &c)), None, "where it was");

    // Touched and moved: the desk follows, the motor isn't sent.
    b.event(&Event::Touch { n: 0, on: true }, ms(t, 200), &bed.state());
    let done = b.event(&Event::Fader { n: 0, value: 8192 }, ms(t, 210), &bed.state());
    assert_eq!(done, [(Input::Fader(8192.0 / 16383.0), None)]);
    bed.command(Cmd::Fader { n: 1, level: Level(-1000) }).unwrap();
    bed.command(Cmd::Fader { n: 1, level: Level(-2000) }).unwrap(); // and elsewhere, a jump
    assert_eq!(motor(b.outs(ms(t, 2000), &bed.state(), &c)), None, "touched");
    // Let go: still nothing for a moment, then it goes where the desk is.
    b.event(&Event::Touch { n: 0, on: false }, ms(t, 2100), &bed.state());
    assert_eq!(motor(b.outs(ms(t, 2200), &bed.state(), &c)), None);
    assert_eq!(motor(b.outs(ms(t, 2400), &bed.state(), &c)), Some(8009), "-20 dB");

    // Moved with no touch (some surfaces): the motor waits after each move.
    b.event(&Event::Fader { n: 0, value: 13106 }, ms(t, 3000), &bed.state());
    bed.command(Cmd::Fader { n: 1, level: Level::UNITY }).unwrap();
    assert_eq!(motor(b.outs(ms(t, 3100), &bed.state(), &c)), None);
    assert_eq!(motor(b.outs(ms(t, 3400), &bed.state(), &c)), None, "the hand left it where the desk is");

    // Moves from elsewhere: at most one a frame of 33 ms.
    bed.command(Cmd::Fader { n: 1, level: Level(-1000) }).unwrap();
    assert_eq!(motor(b.outs(ms(t, 4000), &bed.state(), &c)), Some(10376));
    bed.command(Cmd::Fader { n: 1, level: Level(-2000) }).unwrap();
    assert_eq!(motor(b.outs(ms(t, 4010), &bed.state(), &c)), None);
    assert_eq!(motor(b.outs(ms(t, 4040), &bed.state(), &c)), Some(8009));

    // Another strip selected: the motor goes to its fader.
    let two = Controls { strip: 2, ..Controls::default() };
    assert_eq!(motor(b.outs(ms(t, 5000), &bed.state(), &two)), Some(13106));
    // The surface came back: sent again.
    b.reset();
    assert_eq!(motor(b.outs(ms(t, 6000), &bed.state(), &two)), Some(13106));
}

fn leds(o: Vec<Out>) -> Vec<(u8, Lamp)> {
    let mut v: Vec<(u8, Lamp)> = o
        .into_iter()
        .filter_map(|o| if let Out::Led { note, lamp } = o { Some((note, lamp)) } else { None })
        .collect();
    v.sort_by_key(|x| x.0);
    v
}

#[test]
fn leds_as_they_change() {
    let bed = Testbed::new(48_000, Instant::now());
    let mut b = Binding::new(&STANDARD);
    let c = Controls::default();
    let t = Instant::now();
    let first = leds(b.outs(t, &bed.state(), &c));
    assert_eq!(first.len(), STANDARD.buttons.len(), "every LED, first");
    assert!(first.contains(&(STOP, Lamp::On)) && first.contains(&(PLAY, Lamp::Off)));
    assert_eq!(leds(b.outs(t, &bed.state(), &c)), [], "then only what changes");

    bed.command(Cmd::Arm { n: 1, on: true }).unwrap();
    assert_eq!(leds(b.outs(t, &bed.state(), &c)), [(ARM, Lamp::On), (REC, Lamp::Blink)]);
    bed.command(Cmd::Transport(Mode::Rec)).unwrap();
    assert_eq!(leds(b.outs(t, &bed.state(), &c)), [(STOP, Lamp::Off), (PLAY, Lamp::On), (REC, Lamp::On)]);
    // A refusal blinks its button, then it's back as it was.
    b.refused(Some(FF), t);
    assert_eq!(leds(b.outs(ms(t, 10), &bed.state(), &c)), [(FF, Lamp::Blink)]);
    assert_eq!(leds(b.outs(ms(t, 700), &bed.state(), &c)), [(FF, Lamp::Off)]);
    // SHIFT lights while it's held; the strip's LEDs follow the selection.
    let later = ms(t, 800);
    b.event(&Event::Button { note: SHIFT, down: true }, later, &bed.state());
    assert_eq!(leds(b.outs(later, &bed.state(), &c)), [(SHIFT, Lamp::On)]);
    let two = Controls { strip: 2, ..Controls::default() };
    assert_eq!(leds(b.outs(later, &bed.state(), &two)), [(ARM, Lamp::Off)]);
}

#[test]
fn blinking_for_a_surface_that_cant() {
    let bed = Testbed::new(48_000, Instant::now());
    let mut b = Binding::new(&FADERPORT_V2);
    let c = Controls::default();
    let t = Instant::now();
    b.outs(t, &bed.state(), &c);
    bed.command(Cmd::Arm { n: 1, on: true }).unwrap();
    // REC lit, dark, lit again, a BLINK each; never sent 01.
    assert_eq!(leds(b.outs(ms(t, 10), &bed.state(), &c)), [(ARM, Lamp::On), (REC, Lamp::On)]);
    assert_eq!(leds(b.outs(ms(t, 200), &bed.state(), &c)), []);
    assert_eq!(leds(b.outs(ms(t, 260), &bed.state(), &c)), [(REC, Lamp::Off)]);
    assert_eq!(leds(b.outs(ms(t, 510), &bed.state(), &c)), [(REC, Lamp::On)]);
    // A refusal the same, and back as it was.
    b.refused(Some(FF), ms(t, 1000));
    assert_eq!(leds(b.outs(ms(t, 1000), &bed.state(), &c)), [(FF, Lamp::On)]);
    assert_eq!(leds(b.outs(ms(t, 1250), &bed.state(), &c)), [(FF, Lamp::Off), (REC, Lamp::Off)]);
    assert_eq!(leds(b.outs(ms(t, 1500), &bed.state(), &c)), [(FF, Lamp::On), (REC, Lamp::On)]);
    assert_eq!(leds(b.outs(ms(t, 1650), &bed.state(), &c)), [(FF, Lamp::Off)]);
}
