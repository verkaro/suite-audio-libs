//! ALSA raw MIDI: the ports under `/dev/snd` found by card, a port opened for writing, or for
//! reading and writing, and the messages a keyboard needs (note on and off, program change,
//! controllers, panic), written as bytes. No dependency but `libc`. Written for dexter-like110,
//! 2026-11-04, after the way dexter-surface finds its ports. 2026-11-25: [`Card`], [`Want`],
//! [`find_wanted`] and [`open`] from Dexter's `crates/mcu/src/port.rs`, for `as-mcu`.
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

/// A sound card, as `/proc/asound/cards` has it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Card {
    pub number: u32,
    /// Its short id, in the brackets, trimmed (`FP2`).
    pub id: String,
    /// Its name after the driver and " - " (`PreSonus FP2`).
    pub name: String,
    /// The line under it.
    pub long: String,
}

/// The cards `root/proc/asound/cards` lists, in its order: the suite's one reader of that file
/// (`as-interfaces` takes its cards from here).
pub fn cards(root: &Path) -> Vec<Card> {
    let text = fs::read_to_string(root.join("proc/asound/cards")).unwrap_or_default();
    let mut out: Vec<Card> = Vec::new();
    for line in text.lines() {
        // ` 2 [USB            ]: USB-Audio - Clarett 2Pre USB`, then the long name indented.
        let t = line.trim_start();
        let head = t
            .split_once(' ')
            .and_then(|(num, rest)| Some((num.parse::<u32>().ok()?, rest.trim_start().strip_prefix('[')?)));
        if let Some((number, rest)) = head {
            let Some((id, rest)) = rest.split_once(']') else { continue };
            // ": USB-Audio - Clarett 2Pre USB": the driver, then the card's name.
            let full = rest.trim_start_matches(':');
            let name = full.split_once(" - ").map_or(full, |(_, n)| n).trim();
            out.push(Card { number, id: id.trim().into(), name: name.into(), long: String::new() });
        } else if let Some(c) = out.last_mut().filter(|c| c.long.is_empty()) {
            c.long = t.trim().into();
        }
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
            let listed = cards.iter().find(|k| k.number == card);
            Some(Port {
                path: e.path(),
                card,
                device,
                card_id: listed.map_or(String::new(), |k| k.id.clone()),
                name: listed.map_or(format!("hw:{card}"), |k| k.name.clone()),
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

/// A port as a person names it, as on a command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Want {
    /// Part of a card's id, name or long name, any case.
    Name(String),
    /// `hw:N,M` (a subdevice after it is ignored).
    Hw(u32, u32),
    /// A port's path.
    Path(PathBuf),
}

impl Want {
    pub fn parse(s: &str) -> Want {
        if s.starts_with('/') {
            return Want::Path(s.into());
        }
        let hw = s.strip_prefix("hw:").and_then(|r| {
            let mut n = r.split(',').map(|x| x.trim().parse::<u32>());
            match (n.next(), n.next()) {
                (Some(Ok(c)), Some(Ok(d))) => Some(Want::Hw(c, d)),
                (Some(Ok(c)), None) => Some(Want::Hw(c, 0)),
                _ => None,
            }
        });
        hw.unwrap_or_else(|| Want::Name(s.into()))
    }
}

/// The first port under root that any of wants names, in the order of wants.
pub fn find_wanted(root: &Path, wants: &[Want]) -> Option<Port> {
    let ports = ports(root);
    let cards = cards(root);
    let named = |p: &Port, n: &str| {
        let n = n.to_lowercase();
        cards
            .iter()
            .filter(|c| c.number == p.card)
            .any(|c| [&c.id, &c.name, &c.long].iter().any(|s| s.to_lowercase().contains(&n)))
    };
    wants
        .iter()
        .find_map(|w| {
            ports.iter().find(|p| match w {
                Want::Name(n) => named(p, n),
                Want::Hw(c, d) => p.card == *c && p.device == *d,
                Want::Path(path) => p.path.ends_with(path.strip_prefix("/").unwrap_or(path)),
            })
        })
        .cloned()
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

/// Opens a port for reading and writing (without `O_NONBLOCK`), never creating it: a reader and
/// a writer on the one opening, for a surface that's read and written to.
pub fn open(path: &Path) -> Result<(File, File), Refused> {
    let f = OpenOptions::new().read(true).write(true).open(path).map_err(refused)?;
    let w = f.try_clone().map_err(refused)?;
    Ok((f, w))
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
