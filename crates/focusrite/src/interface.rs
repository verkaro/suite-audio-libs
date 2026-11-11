//! A Focusrite as its controls describe it: the router's sources and destinations, the mixer,
//! the line outputs' level control, speaker switching. Found by names, as the kernel's driver
//! gives them, never by a table per model.

use std::io;

use crate::controls::{Control, Controls, Kind};

/// An output the router feeds, by the name a rig gives it: `LINE 9`, `ADAT 1`, `S/PDIF 2`.
#[derive(Clone, Debug, PartialEq)]
pub struct Output {
    pub name: String,
    /// Its routing enum.
    pub numid: u32,
    /// Its source now, as an item of [`Focusrite::sources`].
    pub source: usize,
}

/// A line output's level: its volume control (`SW`: the program's; `HW`: the knob's), and its
/// volume and mute.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// From 1.
    pub n: usize,
    pub control: Option<u32>,
    pub volume: Option<Volume>,
    pub mute: Option<u32>,
}

/// A volume control: a line output's, or the monitor knob's (`Master HW Playback Volume`, read
/// only). On the 18i20, 0 to 127 for −127 to 0 dB.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Volume {
    pub numid: u32,
    pub min: i64,
    pub max: i64,
    /// Its range in hundredths of a dB, if the driver says.
    pub db: Option<(i64, i64)>,
}

impl Volume {
    /// The raw value for db (−80 and below: the bottom), held to the range; without a dB range,
    /// a step a dB down from the top.
    pub fn raw(&self, db: f64) -> i64 {
        if db <= -80.0 {
            return self.min;
        }
        let raw = match self.db {
            Some((lo, hi)) if hi > lo => {
                let (lo, hi) = (lo as f64 / 100.0, hi as f64 / 100.0);
                self.min as f64 + (db - lo) / (hi - lo) * (self.max - self.min) as f64
            }
            _ => self.max as f64 + db,
        };
        (raw.round() as i64).clamp(self.min, self.max)
    }

    /// The dB a raw value is (the bottom: −80, off).
    pub fn db(&self, raw: i64) -> f64 {
        if raw <= self.min {
            return -80.0;
        }
        let db = match self.db {
            Some((lo, hi)) if self.max > self.min => {
                let (lo, hi) = (lo as f64 / 100.0, hi as f64 / 100.0);
                lo + (raw - self.min) as f64 / (self.max - self.min) as f64 * (hi - lo)
            }
            _ => (raw - self.max) as f64,
        };
        db.max(-80.0)
    }
}

/// An input's switches, as the interface has them: each a control, written 1 for on (an enum's
/// first item past off).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InputControls {
    /// From 1.
    pub n: usize,
    /// `Line In N Level`: Line, else Inst.
    pub inst: Option<u32>,
    pub pad: Option<u32>,
    pub air: Option<u32>,
}

/// A Focusrite interface.
#[derive(Clone, Debug, PartialEq)]
pub struct Focusrite {
    /// Every routing enum's items, by number: `Off`, `Analogue 1`… `Mix A`… `PCM 1`…
    pub sources: Vec<String>,
    /// The mixer's inputs' routing enums, in order.
    pub mixer_inputs: Vec<u32>,
    /// Each mono mix's gain on each mixer input: `gains[mix][input]`.
    pub gains: Vec<Vec<u32>>,
    /// The gains' raw range, and its dB range in hundredths.
    pub gain_raw: (i64, i64),
    pub gain_db: (i64, i64),
    pub outputs: Vec<Output>,
    /// What each capture channel records (from 1), as an item of `sources`.
    pub captures: Vec<usize>,
    /// Each capture channel's routing enum, in the same order.
    pub capture_ids: Vec<u32>,
    /// The clock source's enum, and its items (`Internal`, `S/PDIF`, `ADAT`).
    pub clock: Option<(u32, Vec<String>)>,
    /// Whether the clock is locked (`Sync Status`: `Unlocked`, `Locked`).
    pub sync: Option<u32>,
    /// What the digital ports carry (`Digital I/O Mode`: `S/PDIF RCA`, `S/PDIF Optical`, `Dual
    /// ADAT` on the 18i20), its enum and items; one or the other, the interface blocking the rest.
    pub digital_mode: Option<(u32, Vec<String>)>,
    /// The inputs' switches, by input.
    pub inputs: Vec<InputControls>,
    /// 48V, by the inputs each switch powers (`1-4`), and whether it's kept through a power cycle.
    pub phantom: Vec<(String, u32)>,
    pub phantom_persists: Option<u32>,
    /// The control room's DIM and MUTE (the outputs under the knob).
    pub dim: Option<u32>,
    pub mute: Option<u32>,
    /// The playback channels (`PCM 1`… in the sources).
    pub playbacks: usize,
    pub lines: Vec<Line>,
    pub speaker_switching: Option<u32>,
    /// The monitor knob's level: what the outputs set to `HW` play at.
    pub knob: Option<Volume>,
    /// The talkback (`Disabled`, `Off`, `On`), and each mono mix's switch for hearing it.
    pub talkback: Option<u32>,
    pub talkback_mixes: Vec<u32>,
    /// The talkback mic, as a source in the mixer: the last analogue input (`Analogue 9` on the
    /// 18i20, the one Scarlett of the 3rd gen with a talkback).
    pub talkback_mic: Option<String>,
}

/// The number in a name between prefix and suffix: `Mixer Input 07 Capture Enum` is 7.
fn numbered(name: &str, prefix: &str, suffix: &str) -> Option<usize> {
    name.strip_prefix(prefix)?.strip_suffix(suffix)?.parse().ok()
}

/// A mono mix's letter (`Mix C Input 01 Playback Volume`: C is 2) and input (from 1).
fn mix_gain(name: &str) -> Option<(usize, usize)> {
    let rest = name.strip_prefix("Mix ")?;
    let mut chars = rest.chars();
    let letter = chars.next().filter(char::is_ascii_uppercase)?;
    let input = numbered(chars.as_str(), " Input ", " Playback Volume")?;
    Some((letter as usize - 'A' as usize, input))
}

/// The line a name is about (`Line 07 (Headphones 1 L) Playback Volume`: 7), and the rest.
fn line_of(name: &str) -> Option<(usize, &str)> {
    let rest = name.strip_prefix("Line ")?;
    let digits = rest.find(|c: char| !c.is_ascii_digit())?;
    let n = rest[..digits].parse().ok()?;
    Some((n, &rest[digits..]))
}

impl Focusrite {
    /// The interface, if the controls are a Focusrite's (a mixer with its routing), with its
    /// routing as it is now.
    pub fn read(c: &mut dyn Controls) -> io::Result<Option<Focusrite>> {
        let list = c.list()?;
        Ok(Focusrite::from_controls(&list, &mut |numid| c.read(numid)))
    }

    fn from_controls(
        list: &[Control],
        read: &mut dyn FnMut(u32) -> io::Result<Vec<i64>>,
    ) -> Option<Focusrite> {
        let mut first =
            |numid: u32| read(numid).ok().and_then(|v| v.first().copied()).unwrap_or(0).max(0) as usize;
        let mut mixer_inputs: Vec<(usize, u32)> = Vec::new();
        let mut gains: Vec<(usize, usize, u32)> = Vec::new();
        let mut outputs: Vec<(usize, Output)> = Vec::new();
        let mut captures: Vec<(usize, usize, u32)> = Vec::new();
        let mut lines: Vec<Line> = Vec::new();
        let (mut sources, mut gain_raw, mut gain_db, mut speaker_switching) = (None, None, None, None);
        let (mut talkback, mut talkback_mixes) = (None, Vec::new());
        let mut knob = None;
        let (mut clock, mut sync, mut phantom_persists, mut dim, mut mute) = (None, None, None, None, None);
        let mut digital_mode = None;
        let mut inputs: Vec<InputControls> = Vec::new();
        let mut phantom: Vec<(String, u32)> = Vec::new();
        let input = |inputs: &mut Vec<InputControls>, n: usize| -> usize {
            if let Some(i) = inputs.iter().position(|c| c.n == n) {
                return i;
            }
            inputs.push(InputControls { n, ..InputControls::default() });
            inputs.len() - 1
        };
        for k in list {
            let n = &k.name;
            if n == "Clock Source Clock Source" || n == "Clock Source" {
                if let Kind::Enum(items) = &k.kind {
                    clock = Some((k.numid, items.clone()));
                }
                continue;
            }
            if n == "Master HW Playback Volume" {
                if let Kind::Int { min, max, db, .. } = k.kind {
                    knob = Some(Volume { numid: k.numid, min, max, db });
                }
                continue;
            }
            if n == "Dim Playback Switch" {
                dim = Some(k.numid);
                continue;
            }
            if n == "Mute Playback Switch" {
                mute = Some(k.numid);
                continue;
            }
            if n == "Sync Status" {
                sync = Some(k.numid);
                continue;
            }
            if n == "Digital I/O Mode Capture Enum" {
                if let Kind::Enum(items) = &k.kind {
                    digital_mode = Some((k.numid, items.clone()));
                }
                continue;
            }
            if n == "Phantom Power Persistence Capture Switch" {
                phantom_persists = Some(k.numid);
                continue;
            }
            if let Some(g) =
                n.strip_prefix("Line In ").and_then(|r| r.strip_suffix(" Phantom Power Capture Switch"))
            {
                phantom.push((g.to_string(), k.numid));
                continue;
            }
            if let Some((i, which)) = ["Level", "Pad", "Air"].iter().find_map(|w| {
                let i = numbered(n, "Line In ", &format!(" {w} Capture Enum"))
                    .or_else(|| numbered(n, "Line In ", &format!(" {w} Capture Switch")))?;
                Some((i, *w))
            }) {
                let at = input(&mut inputs, i);
                let c = &mut inputs[at];
                match which {
                    "Level" => c.inst = Some(k.numid),
                    "Pad" => c.pad = Some(k.numid),
                    _ => c.air = Some(k.numid),
                }
                continue;
            }
            if let Some(i) = numbered(n, "Mixer Input ", " Capture Enum") {
                mixer_inputs.push((i, k.numid));
                if let Kind::Enum(items) = &k.kind {
                    sources = Some(items.clone());
                }
            } else if let Some((m, i)) = mix_gain(n) {
                gains.push((m, i, k.numid));
                if let Kind::Int { min, max, db: Some(db), .. } = k.kind {
                    gain_raw = Some((min, max));
                    gain_db = Some(db);
                }
            } else if let Some(i) = numbered(n, "PCM ", " Capture Enum") {
                captures.push((i, first(k.numid), k.numid));
            } else if let Some((order, name, i)) = [
                ("Analogue Output ", "LINE", 0),
                ("ADAT Output ", "ADAT", 1),
                ("S/PDIF Output ", "S/PDIF", 2),
            ]
            .iter()
            .find_map(|(p, out, o)| numbered(n, p, " Playback Enum").map(|i| (*o, *out, i)))
            {
                let out = Output { name: format!("{name} {i}"), numid: k.numid, source: first(k.numid) };
                outputs.push((order * 1000 + i, out));
            } else if let Some(i) = numbered(n, "Line Out ", " Volume Control Playback Enum") {
                line(&mut lines, i).control = Some(k.numid);
            } else if n == "Speaker Switching Playback Enum" {
                speaker_switching = Some(k.numid);
            } else if n == "Talkback Playback Enum" {
                talkback = Some(k.numid);
            } else if let Some(m) =
                n.strip_prefix("Talkback Mix ").and_then(|r| r.strip_suffix(" Playback Switch"))
            {
                if let Some(c) = m.chars().next().filter(|c| m.len() == 1 && c.is_ascii_uppercase()) {
                    talkback_mixes.push((c as usize - 'A' as usize, k.numid));
                }
            } else if let Some((i, rest)) = line_of(n) {
                if rest.ends_with(" Playback Volume") {
                    if let Kind::Int { min, max, db, .. } = k.kind {
                        line(&mut lines, i).volume = Some(Volume { numid: k.numid, min, max, db });
                    }
                } else if rest == " Mute Playback Switch" {
                    line(&mut lines, i).mute = Some(k.numid);
                }
            }
        }
        let sources = sources?;
        mixer_inputs.sort();
        let n_inputs = mixer_inputs.len();
        let n_mixes = gains.iter().map(|g| g.0 + 1).max()?;
        let mut grid = vec![vec![0u32; n_inputs]; n_mixes];
        for (m, i, numid) in gains {
            *grid.get_mut(m)?.get_mut(i.checked_sub(1)?)? = numid;
        }
        if n_inputs == 0 || grid.iter().flatten().any(|&g| g == 0) {
            return None;
        }
        outputs.sort_by_key(|o| o.0);
        captures.sort();
        lines.sort_by_key(|l| l.n);
        inputs.sort_by_key(|c| c.n);
        talkback_mixes.sort();
        let talkback_mic = talkback.and_then(|_| {
            sources
                .iter()
                .filter(|s| numbered(s, "Analogue ", "").is_some())
                .max_by_key(|s| numbered(s, "Analogue ", ""))
                .cloned()
        });
        Some(Focusrite {
            talkback,
            talkback_mic,
            talkback_mixes: talkback_mixes.into_iter().map(|t| t.1).collect(),
            playbacks: sources.iter().filter(|s| s.starts_with("PCM ")).count(),
            sources,
            mixer_inputs: mixer_inputs.into_iter().map(|m| m.1).collect(),
            gains: grid,
            gain_raw: gain_raw?,
            gain_db: gain_db?,
            outputs: outputs.into_iter().map(|o| o.1).collect(),
            capture_ids: captures.iter().map(|c| c.2).collect(),
            captures: captures.into_iter().map(|c| c.1).collect(),
            clock,
            sync,
            digital_mode,
            inputs,
            phantom,
            phantom_persists,
            dim,
            mute,
            lines,
            speaker_switching,
            knob,
        })
    }

    /// The interface as JACK has it at a rate whose USB stream carries fewer channels than the
    /// interface has (the 18i20's 18 of its 20 each way at 88.2 and 96 kHz): the first captures
    /// capture channels and playbacks playback channels. The routing still names them all.
    pub fn carrying(mut self, captures: usize, playbacks: usize) -> Focusrite {
        self.captures.truncate(captures);
        self.capture_ids.truncate(captures);
        self.playbacks = self.playbacks.min(playbacks);
        self
    }

    /// A source's item by name: `Analogue 1`, `PCM 3`, `Mix B`.
    pub fn source(&self, name: &str) -> Option<usize> {
        self.sources.iter().position(|s| s == name)
    }

    /// A source's item by name as a rig writes it, in any case: `adat 1`, `Off`.
    pub fn source_named(&self, name: &str) -> Option<usize> {
        let name = name.trim();
        self.sources.iter().position(|s| s.eq_ignore_ascii_case(name))
    }

    /// The output a rig names, as written or in any case: `LINE 9`, `line 9`.
    pub fn output(&self, name: &str) -> Option<&Output> {
        let name = name.trim();
        self.outputs.iter().find(|o| o.name.eq_ignore_ascii_case(name))
    }

    /// The playback channel (from 1) a source is, if it's one.
    pub fn playback_of(&self, item: usize) -> Option<usize> {
        self.sources.get(item)?.strip_prefix("PCM ")?.parse().ok()
    }

    /// A mono mix's name: `Mix A`.
    pub fn mix_name(m: usize) -> String {
        format!("Mix {}", (b'A' + m as u8) as char)
    }

    /// The raw gain for dB (None: off), held to the control's range.
    pub fn gain(&self, db: Option<f64>) -> i64 {
        let (min, max) = self.gain_raw;
        let (lo, hi) = (self.gain_db.0 as f64 / 100.0, self.gain_db.1 as f64 / 100.0);
        match db {
            Some(db) if db > lo && hi > lo => {
                let raw = min as f64 + (db - lo) / (hi - lo) * (max - min) as f64;
                (raw.round() as i64).clamp(min, max)
            }
            _ => min,
        }
    }
}

fn line(lines: &mut Vec<Line>, n: usize) -> &mut Line {
    if let Some(i) = lines.iter().position(|l| l.n == n) {
        return &mut lines[i];
    }
    lines.push(Line { n, control: None, volume: None, mute: None });
    lines.last_mut().unwrap()
}

/// A source's name for people: `ANALOGUE 9`, `ADAT 3`.
pub fn shown(source: &str) -> String {
    source.to_uppercase()
}
