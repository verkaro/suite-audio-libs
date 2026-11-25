//! Mackie Control's messages: a surface's events, and its LEDs and motors.

use crate::midi::Midi;

/// The first fader's touch note; the eight strips' follow, then the master's (`70`).
pub const TOUCH: u8 = 0x68;
/// The first V-Pot's control change; the eight strips' follow.
pub const VPOT: u8 = 0x10;
/// The jog wheel's control change.
pub const JOG: u8 = 0x3c;
/// The master fader's number: the strips' are 0–7.
pub const MASTER: u8 = 8;

/// What a surface did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A button pressed (`down`) or let go, by its note.
    Button { note: u8, down: bool },
    /// Fader n (0–7, `MASTER`) moved to value, 0–16383.
    Fader { n: u8, value: u16 },
    /// Fader n touched or let go.
    Touch { n: u8, on: bool },
    /// V-Pot n (0–7) turned by delta: right is positive, its size how fast.
    Knob { n: u8, delta: i8 },
    /// The jog wheel turned, as a knob.
    Jog(i8),
    /// Anything else, as it came.
    Other(Midi),
}

/// The message's meaning on a Mackie Control surface, which talks on channel 1 (the faders
/// on 1–9).
pub fn decode(m: Midi) -> Event {
    match m {
        Midi::NoteOn { ch: 0, note, vel } if (TOUCH..=TOUCH + MASTER).contains(&note) => {
            Event::Touch { n: note - TOUCH, on: vel > 0 }
        }
        Midi::NoteOff { ch: 0, note, .. } if (TOUCH..=TOUCH + MASTER).contains(&note) => {
            Event::Touch { n: note - TOUCH, on: false }
        }
        Midi::NoteOn { ch: 0, note, vel } => Event::Button { note, down: vel > 0 },
        Midi::NoteOff { ch: 0, note, .. } => Event::Button { note, down: false },
        Midi::Bend { ch, value } if ch <= MASTER => Event::Fader { n: ch, value },
        Midi::Cc { ch: 0, cc, value } if (VPOT..VPOT + 8).contains(&cc) => {
            Event::Knob { n: cc - VPOT, delta: relative(value) }
        }
        Midi::Cc { ch: 0, cc: JOG, value } => Event::Jog(relative(value)),
        m => Event::Other(m),
    }
}

/// A relative control's value: bit 6 set turns left, the six bits under it how far.
fn relative(v: u8) -> i8 {
    let n = (v & 0x3f) as i8;
    if v & 0x40 != 0 { -n } else { n }
}

/// What a button's LED shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lamp {
    Off,
    On,
    Blink,
}

/// What we send a surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Out {
    /// A button's LED, by its note.
    Led { note: u8, lamp: Lamp },
    /// Fader n's motor to value, 0–16383.
    Motor { n: u8, value: u16 },
}

impl Out {
    pub fn midi(self) -> Midi {
        match self {
            Out::Led { note, lamp } => {
                let vel = match lamp {
                    Lamp::Off => 0,
                    Lamp::On => 0x7f,
                    Lamp::Blink => 0x01,
                };
                Midi::NoteOn { ch: 0, note, vel }
            }
            Out::Motor { n, value } => Midi::Bend { ch: n, value: value.min(0x3fff) },
        }
    }
}

/// Everything in outs, as bytes, each message with its status byte.
pub fn encode(outs: &[Out]) -> Vec<u8> {
    outs.iter().flat_map(|o| o.midi().bytes()).collect()
}
