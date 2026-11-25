//! MIDI's bytes into messages and back: running status, SysEx, and real-time bytes anywhere.

/// A MIDI message. Channels are 0–15 (channel 1 is 0).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Midi {
    NoteOff {
        ch: u8,
        note: u8,
        vel: u8,
    },
    NoteOn {
        ch: u8,
        note: u8,
        vel: u8,
    },
    PolyPressure {
        ch: u8,
        note: u8,
        value: u8,
    },
    Cc {
        ch: u8,
        cc: u8,
        value: u8,
    },
    Program {
        ch: u8,
        program: u8,
    },
    Pressure {
        ch: u8,
        value: u8,
    },
    /// 0–16383, 8192 the middle.
    Bend {
        ch: u8,
        value: u16,
    },
    /// `F0` to `F7`, both included.
    SysEx(Vec<u8>),
    /// Any other system message, its status byte first.
    System(Vec<u8>),
}

impl Midi {
    /// The message as it goes on the wire, with its status byte.
    pub fn bytes(&self) -> Vec<u8> {
        let lo = |v: u16| (v & 0x7f) as u8;
        match *self {
            Midi::NoteOff { ch, note, vel } => vec![0x80 | ch, note, vel],
            Midi::NoteOn { ch, note, vel } => vec![0x90 | ch, note, vel],
            Midi::PolyPressure { ch, note, value } => vec![0xa0 | ch, note, value],
            Midi::Cc { ch, cc, value } => vec![0xb0 | ch, cc, value],
            Midi::Program { ch, program } => vec![0xc0 | ch, program],
            Midi::Pressure { ch, value } => vec![0xd0 | ch, value],
            Midi::Bend { ch, value } => vec![0xe0 | ch, lo(value), lo(value >> 7)],
            Midi::SysEx(ref b) | Midi::System(ref b) => b.clone(),
        }
    }
}

/// Bytes as hex, a space between: `90 5E 7F`.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

/// The longest SysEx kept; the rest of a longer one is dropped.
const SYSEX_MAX: usize = 4096;

/// Turns bytes as they arrive into messages.
#[derive(Debug, Default)]
pub struct Parser {
    status: Option<u8>,
    data: Vec<u8>,
    sysex: Option<Vec<u8>>,
}

impl Parser {
    /// The next byte; a message when it completes one.
    pub fn push(&mut self, b: u8) -> Option<Midi> {
        if b >= 0xf8 {
            // Real-time: anywhere, even inside another message, which it leaves as it was.
            return Some(Midi::System(vec![b]));
        }
        if let Some(sx) = &mut self.sysex {
            if b < 0x80 {
                if sx.len() < SYSEX_MAX {
                    sx.push(b);
                }
                return None;
            }
            let mut sx = self.sysex.take().expect("in a SysEx");
            if b == 0xf7 {
                sx.push(b);
                return Some(Midi::SysEx(sx));
            }
            // Another status byte ends a SysEx that never got its F7: it's dropped, and this
            // byte starts what follows.
        }
        if b >= 0x80 {
            self.data.clear();
            self.status = None;
            match b {
                0xf0 => self.sysex = Some(vec![b]),
                0xf7 => {} // an F7 with no SysEx
                0xf1..=0xf3 => self.status = Some(b),
                0xf4..=0xf6 => return Some(Midi::System(vec![b])),
                _ => self.status = Some(b),
            }
            return None;
        }
        let st = self.status?; // data with no status: dropped
        self.data.push(b);
        if self.data.len() < data_len(st) {
            return None;
        }
        let d = std::mem::take(&mut self.data);
        if st >= 0xf0 {
            self.status = None; // system messages have no running status
        }
        Some(message(st, &d))
    }

    /// Every message in bytes.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Midi> {
        bytes.iter().filter_map(|&b| self.push(b)).collect()
    }
}

fn data_len(status: u8) -> usize {
    match status {
        0xf1 | 0xf3 => 1,
        0xf2 => 2,
        _ if status & 0xf0 == 0xc0 || status & 0xf0 == 0xd0 => 1,
        _ => 2,
    }
}

fn message(st: u8, d: &[u8]) -> Midi {
    let ch = st & 0x0f;
    match st & 0xf0 {
        0x80 => Midi::NoteOff { ch, note: d[0], vel: d[1] },
        0x90 => Midi::NoteOn { ch, note: d[0], vel: d[1] },
        0xa0 => Midi::PolyPressure { ch, note: d[0], value: d[1] },
        0xb0 => Midi::Cc { ch, cc: d[0], value: d[1] },
        0xc0 => Midi::Program { ch, program: d[0] },
        0xd0 => Midi::Pressure { ch, value: d[0] },
        0xe0 => Midi::Bend { ch, value: d[0] as u16 | (d[1] as u16) << 7 },
        _ => Midi::System([&[st], d].concat()),
    }
}
