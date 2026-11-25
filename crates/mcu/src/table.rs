//! A device's table: its controls by what they're for, and the notes they are. The code
//! knows only Mackie Control; what's particular to a surface is here.

use std::fmt;

/// What a button is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Control {
    Play,
    Stop,
    Rec,
    Rew,
    Ff,
    Cycle,
    Marker,
    Shift,
    ChannelLeft,
    ChannelRight,
    BankLeft,
    BankRight,
    Arm,
    Solo,
    Mute,
    Select,
    KnobPush,
    /// Locate to the previous mark, or the next: a surface without SHIFT has buttons for it.
    PrevMark,
    NextMark,
    /// The knob in fine steps, toggled: a surface without SHIFT has a button for it.
    Fine,
    /// The counter a step back, or on: the FaderPort's knob in its timeline mode, as notes.
    TimeBack,
    TimeOn,
    /// The strip into the link, or out of it.
    Link,
    /// The faders to the desk's next mix; held, or with SHIFT, the control room's.
    Mix,
}

impl Control {
    /// Whether it's one of a strip's buttons: on Mackie Control, eight of each, strip n's
    /// note the first strip's and n.
    pub fn on_strip(self) -> bool {
        matches!(self, Control::Arm | Control::Solo | Control::Mute | Control::Select | Control::KnobPush)
    }
}

impl fmt::Display for Control {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Control::Play => "PLAY",
            Control::Stop => "STOP",
            Control::Rec => "REC",
            Control::Rew => "REW",
            Control::Ff => "FF",
            Control::Cycle => "CYCLE",
            Control::Marker => "MARKER",
            Control::Shift => "SHIFT",
            Control::ChannelLeft => "CHANNEL <",
            Control::ChannelRight => "CHANNEL >",
            Control::BankLeft => "BANK <",
            Control::BankRight => "BANK >",
            Control::Arm => "ARM",
            Control::Solo => "SOLO",
            Control::Mute => "MUTE",
            Control::Select => "SELECT",
            Control::KnobPush => "KNOB PUSH",
            Control::PrevMark => "MARK <",
            Control::NextMark => "MARK >",
            Control::Fine => "FINE",
            Control::TimeBack => "TIME <",
            Control::TimeOn => "TIME >",
            Control::Link => "LINK",
            Control::Mix => "MIX",
        })
    }
}

/// A surface's table.
#[derive(Debug)]
pub struct Device {
    /// What it's called on the screen.
    pub name: &'static str,
    /// Parts of the names ALSA gives its card (`/proc/asound/cards`), to find it by.
    pub cards: &'static [&'static str],
    /// Its buttons, and strip 1's, by their notes.
    pub buttons: &'static [(Control, u8)],
    /// Whether it blinks an LED itself when sent `01`: if not, the binding blinks it.
    pub blinks: bool,
    /// Whether it's one strip that moves itself along Mackie Control's eight (PREV and
    /// NEXT), saying where by the strip its events come from: SELECT, the fader, the knob.
    pub roams: bool,
}

impl Device {
    /// The control a note is, on this surface.
    pub fn control(&self, note: u8) -> Option<Control> {
        self.buttons.iter().find(|b| b.1 == note).map(|b| b.0)
    }

    /// The control a note is, and the strip it's on (from 0): a strip's buttons on another of
    /// the eight than the first are only a roaming surface's.
    pub fn button(&self, note: u8) -> Option<(Control, u8)> {
        if let Some(c) = self.control(note) {
            return Some((c, 0));
        }
        let c = self.control(note & !7).filter(|c| c.on_strip() && self.roams && note < 0x28)?;
        Some((c, note & 7))
    }

    /// The note a control is, if this surface has it.
    pub fn note(&self, c: Control) -> Option<u8> {
        self.buttons.iter().find(|b| b.0 == c).map(|b| b.1)
    }
}

/// Mackie Control's own map (Logic Control's), strip 1's buttons as the strip's.
const STANDARD_BUTTONS: &[(Control, u8)] = &[
    (Control::Arm, 0x00),
    (Control::Solo, 0x08),
    (Control::Mute, 0x10),
    (Control::Select, 0x18),
    (Control::KnobPush, 0x20),
    (Control::BankLeft, 0x2e),
    (Control::BankRight, 0x2f),
    (Control::ChannelLeft, 0x30),
    (Control::ChannelRight, 0x31),
    (Control::Shift, 0x46),
    (Control::Mix, 0x4a),
    (Control::Marker, 0x54),
    (Control::Cycle, 0x56),
    (Control::Rew, 0x5b),
    (Control::Ff, 0x5c),
    (Control::Stop, 0x5d),
    (Control::Play, 0x5e),
    (Control::Rec, 0x5f),
];

/// Any Mackie Control surface, found only by `--device`.
pub const STANDARD: Device =
    Device { name: "Mackie Control", cards: &[], buttons: STANDARD_BUTTONS, blinks: true, roams: false };

/// The FaderPort V2's buttons, from Warren's capture and monitor passes (2026-10-12): the
/// strip's and the transport's as the standard map has them. SHIFT, the knob's modes (PAN,
/// CHANNEL, SCROLL…) and MARKER send nothing of their own; PREV and NEXT move the strip
/// (SELECT; BANK ◀ ▶ past its first and eighth, going on to the next bank), with SHIFT held send `4C` and `4F`, and in MARKER's mode `54` and `55`
/// (Warren's shakedown); there the knob is the jog wheel and its push sends nothing. SCROLL
/// latched with SHIFT (bright, `64` in and out) is the timeline: the knob a click `62` back
/// or `63` on, PREV and NEXT `60` and `61`. CLICK and SECTION are plain buttons (`57`, `58`).
/// SECTION marks, BYPASS is fine; LINK links strips, the same with SHIFT or without (Warren).
/// READ steps through Dexter's mixes (Warren, 2026-10-13).
const FADERPORT_V2_BUTTONS: &[(Control, u8)] = &[
    (Control::Arm, 0x00),
    (Control::Solo, 0x08),
    (Control::Mute, 0x10),
    (Control::Select, 0x18),
    (Control::KnobPush, 0x20),
    (Control::BankLeft, 0x2e),
    (Control::BankRight, 0x2f),
    (Control::PrevMark, 0x4c),
    (Control::Fine, 0x4e),
    (Control::NextMark, 0x4f),
    (Control::Mix, 0x4a),
    (Control::Link, 0x50),
    (Control::PrevMark, 0x54),
    (Control::NextMark, 0x55),
    (Control::Cycle, 0x56),
    (Control::Marker, 0x58),
    (Control::Rew, 0x5b),
    (Control::Ff, 0x5c),
    (Control::Stop, 0x5d),
    (Control::Play, 0x5e),
    (Control::Rec, 0x5f),
    (Control::PrevMark, 0x60),
    (Control::NextMark, 0x61),
    (Control::TimeBack, 0x62),
    (Control::TimeOn, 0x63),
];

/// PreSonus's single-fader FaderPort (2018), in MCU mode (`testdata/faderport-v2/`): one
/// strip that roams the eight, and sent `01`, an LED doesn't blink.
pub const FADERPORT_V2: Device = Device {
    name: "FaderPort V2",
    cards: &["FaderPort", "FP2"],
    buttons: FADERPORT_V2_BUTTONS,
    blinks: false,
    roams: true,
};

/// The surfaces looked for without `--device`, in order.
pub const KNOWN: &[&Device] = &[&FADERPORT_V2];
