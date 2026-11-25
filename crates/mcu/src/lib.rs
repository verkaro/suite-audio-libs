//! Mackie Control: the bytes a surface sends decoded into its events, the LEDs and the motor
//! encoded back, a device's table, and the raw MIDI port it's on, kept open. The port is found
//! and opened by `as-rawmidi`. Dexter's `crates/mcu` at `e98d44f`, itself from dexter-surface
//! at `87ea53a`, moved 2026-11-25 (`../../RFC-as-surface.md`, `../../VENDORED.md`). What a
//! surface plays and its rules are `as-surface`.

pub mod capture;
pub mod mcu;
pub mod midi;
pub mod surface;
pub mod table;

pub use mcu::{Event, Lamp, Out, decode, encode};
pub use midi::{Midi, Parser, hex};
pub use surface::{Alsa, Log, Note, Opener, STALE, Status, Surface, describe};
pub use table::{Control, Device, FADERPORT_V2, KNOWN, STANDARD};

#[cfg(test)]
mod tests;
