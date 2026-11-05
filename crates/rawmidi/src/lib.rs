//! ALSA raw MIDI out: the ports under `/dev/snd` found by card, a port opened for writing, and
//! the messages a keyboard needs (note on and off, program change, controllers, panic), written
//! as bytes. No dependency but `libc`. Written for dexter-like110, 2026-11-04, after the way
//! dexter-surface finds its ports (`crates/mcu/src/port.rs`).
//!
//! Every message carries its status byte (running status is never used). Channels are 0 to 15
//! here; a page shows them as 1 to 16.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests;

/// A raw MIDI port: `/dev/snd/midiC<card>D<device>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Port {
    pub path: PathBuf,
    pub card: u32,
    pub device: u32,
    /// The card's id, in `/proc/asound/cards`' brackets, trimmed (`USB`).
    pub card_id: String,
    /// The card's name after " - " (`Clarett 2Pre USB`); `hw:N` if the card isn't listed.
    pub name: String,
}

/// The cards `root/proc/asound/cards` lists: number, id, name.
fn cards(root: &Path) -> Vec<(u32, String, String)> {
    let text = fs::read_to_string(root.join("proc/asound/cards")).unwrap_or_default();
    let mut out = Vec::new();
    for line in text.lines() {
        // ` 2 [USB            ]: USB-Audio - Clarett 2Pre USB`; the line under it is indented.
        let t = line.trim_start();
        let Some((num, rest)) = t.split_once(' ') else { continue };
        let (Ok(number), Some(rest)) = (num.parse::<u32>(), rest.trim_start().strip_prefix('[')) else {
            continue;
        };
        let Some((id, rest)) = rest.split_once(']') else { continue };
        let name = rest.split_once(" - ").map_or("", |(_, n)| n).trim();
        out.push((number, id.trim().to_string(), name.to_string()));
    }
    out
}

/// Every raw MIDI port under `root` (`/` on a machine, a scratch folder in tests): the files
/// `root/dev/snd/midiC*D*`, named from `root/proc/asound/cards`, sorted by card then device.
pub fn ports(root: &Path) -> Vec<Port> {
    let cards = cards(root);
    let mut out: Vec<Port> = fs::read_dir(root.join("dev/snd"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let file = e.file_name().into_string().ok()?;
            let (c, d) = file.strip_prefix("midiC")?.split_once('D')?;
            let (card, device) = (c.parse().ok()?, d.parse().ok()?);
            let listed = cards.iter().find(|k| k.0 == card);
            Some(Port {
                path: e.path(),
                card,
                device,
                card_id: listed.map_or(String::new(), |k| k.1.clone()),
                name: listed.map_or(format!("hw:{card}"), |k| k.2.clone()),
            })
        })
        .collect();
    out.sort_by_key(|p| (p.card, p.device));
    out
}

/// The port with this card id and device, if it's there now.
pub fn find(root: &Path, card_id: &str, device: u32) -> Option<Port> {
    ports(root).into_iter().find(|p| p.card_id == card_id && p.device == device)
}

/// Why a port couldn't be opened.
#[derive(Debug)]
pub enum Refused {
    Missing,
    Busy,
    Denied,
    Other(io::Error),
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refused::Missing => write!(f, "no such port (unplugged?)"),
            Refused::Busy => write!(f, "held by another program (jackd started with -X raw, or a2jmidid?)"),
            Refused::Denied => write!(f, "not allowed (is this user in the audio group?)"),
            Refused::Other(e) => write!(f, "{e}"),
        }
    }
}

fn refused(e: io::Error) -> Refused {
    match e.raw_os_error() {
        Some(libc::ENOENT | libc::ENODEV | libc::ENXIO) => Refused::Missing,
        Some(libc::EBUSY) => Refused::Busy,
        Some(libc::EACCES | libc::EPERM) => Refused::Denied,
        _ => Refused::Other(e),
    }
}

/// A port open for writing.
pub struct Out {
    file: File,
}

impl Out {
    /// Opens the port write-only (without `O_NONBLOCK`), never creating it.
    pub fn open(path: &Path) -> Result<Out, Refused> {
        OpenOptions::new().write(true).open(path).map(|file| Out { file }).map_err(refused)
    }

    /// Writes one message whole, and nothing else.
    pub fn send(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.file.write_all(bytes)
    }
}

/// The messages. Values are masked to what a byte can say (`key & 0x7F`, `ch & 0x0F`).
pub mod msg {
    pub fn note_on(ch: u8, key: u8, vel: u8) -> [u8; 3] {
        [0x90 | (ch & 0x0F), key & 0x7F, vel & 0x7F]
    }
    pub fn note_off(ch: u8, key: u8) -> [u8; 3] {
        [0x80 | (ch & 0x0F), key & 0x7F, 0x40]
    }
    pub fn program(ch: u8, p: u8) -> [u8; 2] {
        [0xC0 | (ch & 0x0F), p & 0x7F]
    }
    pub fn control(ch: u8, cc: u8, v: u8) -> [u8; 3] {
        [0xB0 | (ch & 0x0F), cc & 0x7F, v & 0x7F]
    }
    pub fn all_sound_off(ch: u8) -> [u8; 3] {
        control(ch, 120, 0)
    }
    pub fn all_notes_off(ch: u8) -> [u8; 3] {
        control(ch, 123, 0)
    }
}

/// All sound off, all notes off, then a note off for every key 0 to 127 (cheap keyboards ignore
/// the controllers): 2 + 128 messages.
pub fn panic(out: &mut Out, ch: u8) -> io::Result<()> {
    out.send(&msg::all_sound_off(ch))?;
    out.send(&msg::all_notes_off(ch))?;
    for key in 0..=127 {
        out.send(&msg::note_off(ch, key))?;
    }
    Ok(())
}
