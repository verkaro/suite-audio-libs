//! A Mackie Control surface playing a desk (Dexter's `docs/faderport.md`, "The binding"): its events
//! into [`Input`]s, and the desk's state into its LEDs and motor, sent only as they change.
//! Pure: the time is passed in.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::desk::{DeskState, Mode};
use as_mcu::{Control, Device, Event, Lamp, Out};

use super::{Controls, Input};

/// A press of REW or FF held this long is momentary: let go, it stops (Dexter's page).
pub const HOLD: Duration = Duration::from_millis(350);
/// After the hand moves the fader, the motor waits this long (some surfaces move without a
/// touch).
pub const HAND: Duration = Duration::from_millis(300);
/// The motor is sent at most this often.
pub const MOTOR_EVERY: Duration = Duration::from_millis(33);
/// A refused command's button blinks this long.
pub const REFUSED_FOR: Duration = Duration::from_millis(600);
/// An LED the binding blinks itself (the surface can't) is lit, then dark, this long each.
pub const BLINK: Duration = Duration::from_millis(250);
/// The knob's push held this long puts its control back where it starts (Warren: a second,
/// not half).
pub const PUSH_HOLD: Duration = Duration::from_secs(1);
/// Clicks of the timeline's knob this close together are a spin: its steps grow. The
/// FaderPort sends at most one every 100 ms and drops the clicks between (Warren's monitor
/// pass), so a click this soon after the last stands for several.
pub const SPIN_GAP: Duration = Duration::from_millis(150);
/// A spin's step: 4 from its second click, doubling every this many, to 16 a click.
pub const SPIN_EVERY: u32 = 4;
/// The motor isn't sent for less than this (of 16383): the taper's rounding, not a move.
const MOTOR_SLACK: u16 = 24;

/// The surface's side of things: SHIFT, where it is, the hand on the fader, a wind or a push
/// held, what it was sent.
pub struct Binding {
    dev: &'static Device,
    shift: bool,
    /// The knob in fine steps, for a surface with a button for it rather than SHIFT.
    fine: bool,
    /// The strip of Mackie Control's eight the surface is on (from 0), and the bank of eight
    /// it's in (from 0): a roaming surface's, the selected strip's place. The FaderPort's
    /// strip is where its SELECT is lit (Warren's logs): its PREV and NEXT go on from there,
    /// so lighting the selected strip's moves it.
    at: u8,
    bank: usize,
    touched: bool,
    hand_at: Option<Instant>,
    /// REW or FF's note, and when it went down.
    wind: Option<(u8, Instant)>,
    /// The knob's push, or MIX: its note, when it went down, and whether with SHIFT.
    push: Option<(u8, Instant, bool)>,
    /// The timeline's knob turning: its way, its last click, and how many clicks fast.
    spin: Option<(i32, Instant, u32)>,
    /// The button whose command was refused, until when it blinks.
    refused: Option<(u8, Instant)>,
    leds: HashMap<u8, Lamp>,
    /// The strip the LEDs were last sent for.
    leds_at: u8,
    motor: Option<(u16, Instant)>,
    /// What the motor was last sent for: the surface's strip, and the desk's (0 for none).
    motor_for: (u8, usize),
    /// When the LEDs were first sent: the binding's blinking keeps time from it.
    since: Option<Instant>,
}

/// Something done on the surface: the input, and the button it came from (to blink if the
/// desk refuses it).
pub type Done = (Input, Option<u8>);

impl Binding {
    pub fn new(dev: &'static Device) -> Binding {
        Binding {
            dev,
            shift: false,
            fine: false,
            at: 0,
            bank: 0,
            touched: false,
            hand_at: None,
            wind: None,
            push: None,
            spin: None,
            refused: None,
            leds: HashMap::new(),
            leds_at: 0,
            motor: None,
            motor_for: (0, 0),
            since: None,
        }
    }

    /// The surface came back (or first opened): it remembers nothing, so everything is sent
    /// again, and nothing's held.
    pub fn reset(&mut self) {
        let (at, bank, fine) = (self.at, self.bank, self.fine);
        *self = Binding { at, bank, fine, leds_at: at, ..Binding::new(self.dev) };
    }

    /// What an event does, with the desk as it is.
    pub fn event(&mut self, e: &Event, now: Instant, s: &DeskState) -> Vec<Done> {
        match *e {
            Event::Button { note, down } => self.button(note, down, now, s),
            Event::Fader { n, value } => {
                let Some((mut done, there)) = self.on(n, s) else { return vec![] };
                self.hand_at = Some(now);
                self.motor = Some((value, now));
                if there {
                    done.push((Input::Fader(value as f64 / 16383.0), None));
                }
                done
            }
            Event::Touch { n, on } => {
                let Some((done, _)) = self.on(n, s) else { return vec![] };
                self.touched = on;
                if !on {
                    self.hand_at = Some(now);
                }
                done
            }
            Event::Knob { n, delta } => {
                let Some((mut done, there)) = self.on(n, s) else { return vec![] };
                if there {
                    done.push((Input::Knob { steps: delta as i32, fine: self.shift || self.fine }, None));
                }
                done
            }
            // The jog wheel (the FaderPort's knob, in MARKER's or SCROLL's mode) goes mark to
            // mark (Warren).
            Event::Jog(delta) if delta != 0 => vec![(Input::Locate(delta.signum()), None)],
            _ => vec![],
        }
    }

    /// An event on strip n of the eight: none if it isn't this surface's to play (another
    /// strip of a surface that doesn't roam); else the selection it makes (a roaming surface
    /// moved: the strip at n in its bank), and whether the desk has the strip. Past the desk's
    /// last strip the selection stays, and the surface is sent back to it.
    fn on(&mut self, n: u8, s: &DeskState) -> Option<(Vec<Done>, bool)> {
        if !self.dev.roams {
            return (n == 0).then_some((vec![], true));
        }
        let strip = self.bank * 8 + n as usize + 1;
        let there = strip <= s.strips.len();
        let mut done = vec![];
        if n != self.at {
            self.at = n;
            self.touched = false;
            done.push((Input::Select(strip), None));
            if !there {
                // Every LED sent again: SELECT lit where the selection stayed brings it back.
                self.leds.clear();
            }
        }
        Some((done, there))
    }

    fn button(&mut self, note: u8, down: bool, now: Instant, s: &DeskState) -> Vec<Done> {
        let Some((c, n)) = self.dev.button(note) else { return vec![] };
        if c == Control::Shift {
            self.shift = down;
            return vec![];
        }
        let mut done = vec![];
        if c.on_strip() {
            let Some((sel, there)) = self.on(n, s) else { return vec![] };
            done = sel;
            if !there {
                return done;
            }
        }
        if !down {
            // A wind held long enough stops when it's let go, if it's still winding.
            let held = self.wind.take_if(|(n, _)| *n == note);
            let winding = matches!((c, s.mode), (Control::Rew, Mode::Rew) | (Control::Ff, Mode::Ff));
            if let Some((_, at)) = held
                && now - at >= HOLD
                && winding
            {
                done.push((Input::Stop, Some(note)));
            }
            // The knob's push held, or with SHIFT, puts PAN back to the centre (a tap does
            // nothing: a push by accident doesn't move it). MIX: the next mix, or, held or with
            // SHIFT, back to the control room's.
            if let Some((_, at, shift)) = self.push.take_if(|(n, ..)| *n == note) {
                let back = shift || now - at >= PUSH_HOLD;
                let input = match (c, back) {
                    (Control::Mix, false) => Some(Input::NextMix),
                    (Control::Mix, true) => Some(Input::ControlMix),
                    (_, true) => Some(Input::PanCentre),
                    (_, false) => None,
                };
                if let Some(input) = input {
                    done.push((input, Some(note)));
                }
            }
            return done;
        }
        let shift = self.shift;
        let i = match c {
            // PLAY/PAUSE, as the FaderPort labels it: rolling, it stops where it is.
            Control::Play => Input::PlayStop,
            Control::Stop if shift || s.mode == Mode::Stop => Input::Rtz,
            Control::Stop => Input::Stop,
            Control::Rec => Input::Rec,
            Control::Rew | Control::Ff => {
                self.wind = Some((note, now));
                if c == Control::Rew { Input::Rew } else { Input::Ff }
            }
            Control::Marker => Input::MarkToggle,
            Control::ChannelLeft if shift => Input::Locate(-1),
            Control::ChannelRight if shift => Input::Locate(1),
            Control::PrevMark => Input::Locate(-1),
            Control::TimeBack | Control::TimeOn => {
                // A note a click, at most ten a second (the FaderPort), so a spin's steps
                // grow here, at once; fine, they don't.
                let way = if c == Control::TimeOn { 1 } else { -1 };
                let run = match self.spin {
                    Some((w, at, run)) if w == way && now - at <= SPIN_GAP => run + 1,
                    _ => 0,
                };
                self.spin = Some((way, now, run));
                let fine = shift || self.fine;
                let steps = match run {
                    _ if fine => 1,
                    0 => 1,
                    r => (4 << ((r - 1) / SPIN_EVERY)).min(16),
                };
                Input::Nudge { steps: way * steps, fine }
            }
            Control::NextMark => Input::Locate(1),
            Control::ChannelLeft | Control::BankLeft => Input::Next(-1),
            Control::ChannelRight | Control::BankRight => Input::Next(1),
            Control::KnobPush | Control::Mix => {
                self.push = Some((note, now, shift));
                return done;
            }
            Control::Fine => {
                self.fine = !self.fine;
                return done;
            }
            Control::Arm => Input::Arm,
            Control::Mute => Input::Mute,
            Control::Solo => Input::Solo,
            Control::Link => Input::Link,
            Control::Shift | Control::Select | Control::Cycle => return done,
        };
        done.push((i, Some(note)));
        done
    }

    /// The desk refused what a button asked: it blinks a while.
    pub fn refused(&mut self, note: Option<u8>, now: Instant) {
        if let Some(n) = note {
            self.refused = Some((n, now + REFUSED_FOR));
        }
    }

    /// What the surface should be sent now: LEDs that changed, and the motor when it's free
    /// to move and the selected strip's fader isn't where it was last sent. A roaming surface
    /// is sent its strip's on the strip it's on; where the desk has none there, the strip's
    /// LEDs are dark but SELECT, and the fader at the bottom.
    pub fn outs(&mut self, now: Instant, s: &DeskState, c: &Controls) -> Vec<Out> {
        let mut out = Vec::new();
        let strip = s.strips.get(c.strip.wrapping_sub(1));
        // A roaming surface is where the selection is, in its bank of eight.
        if self.dev.roams {
            let i = c.strip.saturating_sub(1);
            (self.bank, self.at) = (i / 8, (i % 8) as u8);
        }
        let armed = s.strips.iter().any(|st| st.arm);
        let refused = self.refused.filter(|&(_, until)| now < until).map(|(n, _)| n);
        let on = |b: bool| if b { Lamp::On } else { Lamp::Off };
        let since = *self.since.get_or_insert(now);
        let lit = ((now - since).as_millis() / BLINK.as_millis()).is_multiple_of(2);
        // Moved: the strip it was on goes dark.
        if self.leds_at != self.at {
            for &(_, note) in self.dev.buttons.iter().filter(|b| b.0.on_strip()) {
                let old = note + self.leds_at;
                if self.leds.remove(&old).is_some_and(|l| l != Lamp::Off) {
                    out.push(Out::Led { note: old, lamp: Lamp::Off });
                }
            }
            self.leds_at = self.at;
        }
        for &(control, note) in self.dev.buttons {
            let note = if control.on_strip() { note + self.at } else { note };
            let mut lamp = match control {
                Control::Play => on(matches!(s.mode, Mode::Play | Mode::Rec)),
                Control::Stop => on(s.mode == Mode::Stop),
                Control::Rec if s.mode == Mode::Rec => Lamp::On,
                Control::Rec => {
                    if armed {
                        Lamp::Blink
                    } else {
                        Lamp::Off
                    }
                }
                Control::Rew => on(s.mode == Mode::Rew),
                Control::Ff => on(s.mode == Mode::Ff),
                Control::Shift => on(self.shift),
                Control::Fine => on(self.fine),
                // Lit where a roaming surface is, as a DAW lights it: the FaderPort's PREV and
                // NEXT go on from it (dark, it went astray: Warren's log).
                Control::Select => on(strip.is_some() || self.dev.roams),
                Control::Arm => on(strip.is_some_and(|st| st.arm)),
                Control::Mute => on(strip.is_some_and(|st| st.mute)),
                Control::Solo => on(strip.is_some_and(|st| st.solo)),
                Control::Link => on(strip.is_some() && c.linked.contains(&c.strip)),
                // Lit while the faders play a mix other than the control room's.
                Control::Mix => on(s.mix.as_ref().is_some_and(|m| !m.control)),
                _ => Lamp::Off,
            };
            if refused == Some(note) {
                lamp = Lamp::Blink;
            }
            if lamp == Lamp::Blink && !self.dev.blinks {
                lamp = on(lit);
            }
            if self.leds.get(&note) != Some(&lamp) {
                self.leds.insert(note, lamp);
                out.push(Out::Led { note, lamp });
            }
        }
        let want = match strip {
            Some(st) if st.has.fader => Some((st.has.taper.position(st.fader) * 16383.0).round() as u16),
            Some(_) => None,
            None => Some(0),
        };
        if let Some(want) = want
            && !self.touched
            && self.hand_at.is_none_or(|t| now - t >= HAND)
        {
            let to = (self.at, if strip.is_some() { c.strip } else { 0 });
            let due = match self.motor {
                None => true,
                Some((v, at)) => {
                    (self.motor_for != to || v.abs_diff(want) >= MOTOR_SLACK) && now - at >= MOTOR_EVERY
                }
            };
            if due {
                self.motor = Some((want, now));
                self.motor_for = to;
                out.push(Out::Motor { n: self.at, value: want });
            }
        }
        out
    }
}

#[cfg(test)]
#[path = "surface_tests.rs"]
mod tests;
