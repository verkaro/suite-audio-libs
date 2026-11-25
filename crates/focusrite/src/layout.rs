//! The rigs' layouts (vkro, decision 10 of `dexter-net-research`'s design brief, Warren
//! 2026-11-17): the Scarlett 18i20 3rd Gen and the Clarett+ 8Pre, each with an OctoPre over
//! ADAT, sixteen channels, laid out for one of two ways of monitoring.
//!
//! - **Internal:** the interface's mixer makes the monitor mixes from the live inputs and the
//!   tape return: the control room on LINE 1·2, phones 1 on LINE 7·8, phones 2 on LINE 9·10.
//! - **External:** an analogue board makes them, fed by the sixteen channels out direct, LINE
//!   1–8 and ADAT 1–8, each carrying its input routed in the interface with no latency. While
//!   the tape plays, a track's out carries the tape in its place (step 4, [`Layout::tape_of`]).
//!
//! Dexter's values where Dexter settled them (`dexter/docs/interface.md`, `loopback.md`): the
//! loop on the last playback channel, `PCM 20`, to S/PDIF 1 and nowhere else; the OctoPre's
//! ADAT outs from `PCM 11`–`18`; the tape return on `PCM 1`·`2`; the mixer's inputs 1–8
//! Analogue 1–8, 9–16 ADAT 1–8, then the return. No test signal is ever routed to a line or
//! ADAT output (Warren, 2026-11-17): only S/PDIF 1 carries the loop.
//!
//! A layout is data ([`Layout::of`]) and what it takes to set it on a box as it is now
//! ([`Layout::writes`]): only the controls that differ, so writing it twice writes nothing the
//! second time. The mixer's gains go with their sources: a source moved to another mixer input
//! keeps the gains it had in each mix, so a layout written over a box someone balanced keeps
//! the balance. Writing a layout is chosen, never assumed: the caller decides when.

use std::fmt;
use std::io;

use crate::controls::Controls;
use crate::hand::{Dest, Mixing, routes};
use crate::interface::Focusrite;

/// The interfaces laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rig {
    /// The Scarlett 18i20 3rd Gen: 20 captures, 25 mixer inputs (the last the talkback's), 12
    /// mixes.
    Scarlett18i20,
    /// The Clarett+ 8Pre: 18 captures, 18 mixer inputs, 10 mixes.
    ClarettPlus8Pre,
}

impl Rig {
    /// The rig a card is, by the name the card gives (`Scarlett 18i20 USB`, `Clarett+ 8Pre`).
    pub fn named(card: &str) -> Option<Rig> {
        let c = card.to_lowercase();
        if c.contains("18i20") {
            Some(Rig::Scarlett18i20)
        } else if c.contains("clarett+ 8pre") || c.contains("clarett+8pre") {
            Some(Rig::ClarettPlus8Pre)
        } else {
            None
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Rig::Scarlett18i20 => "Scarlett 18i20 3rd Gen",
            Rig::ClarettPlus8Pre => "Clarett+ 8Pre",
        }
    }

    /// Each track's capture channel, from 1: 1–8 the interface's own inputs, 9–16 the
    /// OctoPre's.
    pub fn track_captures(self) -> [usize; 16] {
        let mut c = [0; 16];
        for (t, slot) in c.iter_mut().enumerate() {
            *slot = match (self, t) {
                (_, 0..=7) => t + 1,
                // The 8Pre's captures 9 and 10 carry the mix stream (and the loop, while the
                // check borrows them), so the OctoPre's are 11–18.
                (Rig::ClarettPlus8Pre, _) => t + 3,
                (Rig::Scarlett18i20, _) => t + 1,
            };
        }
        c
    }

    /// The capture channels, from 1, that carry the mix stream.
    pub fn mix_captures(self) -> [usize; 2] {
        match self {
            Rig::ClarettPlus8Pre => [9, 10],
            Rig::Scarlett18i20 => [19, 20],
        }
    }

    /// The capture channel, from 1, the loop returns on: the 18i20's own; the 8Pre's mix
    /// stream's left, borrowed for the seconds of a check.
    pub fn loop_capture(self) -> usize {
        match self {
            Rig::ClarettPlus8Pre => 9,
            Rig::Scarlett18i20 => 17,
        }
    }
}

/// The way the monitor mixes are made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Way {
    Internal,
    External,
}

impl Way {
    pub fn word(self) -> &'static str {
        match self {
            Way::Internal => "internal",
            Way::External => "external",
        }
    }

    pub fn named(w: &str) -> Option<Way> {
        match w.trim().to_lowercase().as_str() {
            "internal" => Some(Way::Internal),
            "external" => Some(Way::External),
            _ => None,
        }
    }
}

/// The playback channel the loop plays from, by name: the last, as Dexter's was.
pub const LOOP_SOURCE: &str = "PCM 20";
/// The output the loop's cable leaves from.
pub const LOOP_OUT: &str = "S/PDIF 1";
/// The tape return's two playback channels, by name.
pub const RETURN: [&str; 2] = ["PCM 1", "PCM 2"];

/// A monitor mix on the interface: a pair of its mono mixes (0 is Mix A) and the line outputs
/// they play.
#[derive(Clone, Debug, PartialEq)]
pub struct Monitor {
    pub name: &'static str,
    pub pair: [usize; 2],
    pub lines: [usize; 2],
}

/// One of the sixteen channels.
#[derive(Clone, Debug, PartialEq)]
pub struct Channel {
    /// From 1.
    pub n: usize,
    /// Its input, by the router's name: `Analogue 3`, `ADAT 5`.
    pub input: String,
    /// The capture channel that records it, from 1.
    pub capture: usize,
    /// Its out to the board in the external way: `LINE 3`, `ADAT 5`.
    pub out: String,
    /// The playback channel its tape goes out on, by name (`PCM 5`), when the out carries the
    /// tape in place of the input.
    pub tape: String,
}

/// A rig laid out for a way of monitoring: what each destination plays, and the tape return's
/// gains in the mixes it uses.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub rig: Rig,
    pub way: Way,
    pub channels: Vec<Channel>,
    /// The monitor mixes made in the interface; in the external way, only the clients' mix
    /// stream's pair.
    pub monitors: Vec<Monitor>,
    /// Each destination's source, by the router's names: (`CAPTURE 11`, `ADAT 1`),
    /// (`MIXER 17`, `PCM 1`), (`LINE 1`, `Mix A`).
    pub routes: Vec<(String, String)>,
}

/// One control to write, and what it is, for a log.
#[derive(Clone, Debug, PartialEq)]
pub struct Write {
    pub numid: u32,
    pub value: i64,
    pub what: String,
}

/// Why a layout can't be set on this interface.
#[derive(Debug, PartialEq)]
pub struct Unfit(pub Vec<String>);

impl fmt::Display for Unfit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the layout doesn't fit this interface: {}", self.0.join("; "))
    }
}

impl Layout {
    /// The layout of `rig` for `way`.
    pub fn of(rig: Rig, way: Way) -> Layout {
        let captures = rig.track_captures();
        let channels: Vec<Channel> = (1..=16)
            .map(|n| {
                let (input, out, tape) = if n <= 8 {
                    // Channels 1–8's tape on PCM 3–10: PCM 1·2 are the return's.
                    (format!("Analogue {n}"), format!("LINE {n}"), format!("PCM {}", n + 2))
                } else {
                    (format!("ADAT {}", n - 8), format!("ADAT {}", n - 8), format!("PCM {}", n + 2))
                };
                Channel { n, input, capture: captures[n - 1], out, tape }
            })
            .collect();
        let monitors = match way {
            Way::Internal => vec![
                Monitor { name: "CONTROL RM", pair: [0, 1], lines: [1, 2] },
                Monitor { name: "PHONES 1", pair: [2, 3], lines: [7, 8] },
                Monitor { name: "PHONES 2", pair: [4, 5], lines: [9, 10] },
            ],
            // The clients' mix stream: the inputs and the tape, made in the interface while the
            // board makes the room's; heard on phones 2, which stay headphones (Warren,
            // 2026-11-17: the sixteen outs are LINE 1–8 and ADAT 1–8). That phones 2 hear the
            // mix stream is assumed, not settled.
            Way::External => vec![Monitor { name: "MIX STREAM", pair: [0, 1], lines: [9, 10] }],
        };
        let mut r: Vec<(String, String)> = Vec::new();
        let mut set = |d: String, s: String| r.push((d, s));
        // What's recorded.
        for c in &channels {
            set(format!("CAPTURE {}", c.capture), c.input.clone());
        }
        let [ml, mr] = rig.mix_captures();
        set(format!("CAPTURE {ml}"), "Mix A".into());
        set(format!("CAPTURE {mr}"), "Mix B".into());
        if rig == Rig::Scarlett18i20 {
            set(format!("CAPTURE {}", rig.loop_capture()), "S/PDIF 1".into());
            set("CAPTURE 18".into(), "Off".into());
        }
        // The mixer: the sixteen inputs, then the return; the 18i20's spare inputs off and its
        // talkback's (the last) left as found.
        for c in &channels {
            set(format!("MIXER {}", c.n), c.input.clone());
        }
        set("MIXER 17".into(), RETURN[0].into());
        set("MIXER 18".into(), RETURN[1].into());
        if rig == Rig::Scarlett18i20 {
            for n in 19..=24 {
                set(format!("MIXER {n}"), "Off".into());
            }
        }
        // The outputs: each monitor's lines, then the rest.
        for m in &monitors {
            for side in 0..2 {
                set(format!("LINE {}", m.lines[side]), Focusrite::mix_name(m.pair[side]));
            }
        }
        match way {
            Way::Internal => {
                for n in 3..=6 {
                    set(format!("LINE {n}"), "Off".into());
                }
                for c in channels.iter().filter(|c| c.n > 8) {
                    set(c.out.clone(), c.tape.clone());
                }
            }
            Way::External => {
                for c in &channels {
                    set(c.out.clone(), c.input.clone());
                }
            }
        }
        set(LOOP_OUT.into(), LOOP_SOURCE.into());
        set("S/PDIF 2".into(), "Off".into());
        Layout { rig, way, channels, monitors, routes: r }
    }

    /// What an out plays while its channel's tape plays: the channel's playback channel
    /// (step 4's auto input). None for a channel the layout hasn't.
    pub fn tape_of(&self, channel: usize) -> Option<(&str, &str)> {
        let c = self.channels.iter().find(|c| c.n == channel)?;
        Some((c.out.as_str(), c.tape.as_str()))
    }

    /// The writes that set this layout on `f`, as `now` (each destination's source, in
    /// [`Focusrite::dests`]' order) and `mixing` have it; only what differs. The mixer's gains go
    /// with their sources, and the tape return comes into each monitor's mixes at 0 dB, left
    /// into the left. `Err` names what the interface hasn't.
    pub fn writes(&self, f: &Focusrite, now: &[usize], mixing: &Mixing) -> Result<Vec<Write>, Unfit> {
        let all = f.dests();
        let mut missing = Vec::new();
        let mut want: Vec<(Dest, usize)> = Vec::new();
        for (d, s) in &self.routes {
            match (f.dest_named(d), f.source(s)) {
                (Some(dest), Some(src)) => want.push((dest, src)),
                (None, _) => missing.push(format!("no {d}")),
                (_, None) => missing.push(format!("no source {s}")),
            }
        }
        let used: Vec<usize> = self.monitors.iter().flat_map(|m| m.pair).collect();
        if let Some(m) = used.iter().find(|&&m| m >= f.gains.len()) {
            missing.push(format!("no {}", Focusrite::mix_name(*m)));
        }
        if !missing.is_empty() {
            return Err(Unfit(missing));
        }
        let mut out = Vec::new();
        // The routes: outputs and captures as asked; the mixer's inputs below, with the gains.
        for &(d, s) in &want {
            if matches!(d, Dest::MixerInput(_)) {
                continue;
            }
            let k = all.iter().position(|x| *x == d).expect("a destination of its own");
            if now.get(k) != Some(&s) {
                out.push(Write {
                    numid: f.dest_numid(d).expect("named above"),
                    value: s as i64,
                    what: format!("{} plays {}", f.dest_name(d), f.sources[s]),
                });
            }
        }
        // The mixer: each input's source, and each mix's gain on it, carried from wherever the
        // source was.
        let inputs = mixing.inputs();
        let gains = mixing.gains();
        let mut next_inputs = inputs.to_vec();
        for &(d, s) in &want {
            if let Dest::MixerInput(n) = d {
                next_inputs[n - 1] = s;
            }
        }
        let returns = [f.source(RETURN[0]), f.source(RETURN[1])];
        for (i, &s) in next_inputs.iter().enumerate() {
            if inputs[i] != s {
                out.push(Write {
                    numid: f.mixer_inputs[i],
                    value: s as i64,
                    what: format!("MIXER {} plays {}", i + 1, f.sources[s]),
                });
            }
            for (m, row) in gains.iter().enumerate() {
                let was = if s == f.off() {
                    f.gain_raw.0
                } else {
                    // The gain the source had in this mix, on whichever input it was.
                    inputs.iter().position(|&x| x == s).map_or(f.gain_raw.0, |j| row[j])
                };
                let side = returns.iter().position(|&r| r == Some(s));
                let to = match side {
                    // The return: 0 dB into its side of each monitor's pair, off elsewhere.
                    Some(side) if self.monitors.iter().any(|mon| mon.pair[side] == m) => f.gain(Some(0.0)),
                    Some(_) => f.gain_raw.0,
                    None => was,
                };
                if row[i] != to {
                    out.push(Write {
                        numid: f.gains[m][i],
                        value: to,
                        what: format!(
                            "{} hears MIXER {} ({}) at {}",
                            Focusrite::mix_name(m),
                            i + 1,
                            f.sources[s],
                            f.gain_db(to).map_or("off".to_string(), |db| format!("{db:.1} dB"))
                        ),
                    });
                }
            }
        }
        Ok(out)
    }

    /// Sets the layout on the interface through `c`: reads what's there, writes what differs.
    /// Returns what was written.
    pub fn apply(&self, c: &mut dyn Controls, f: &Focusrite) -> io::Result<Vec<Write>> {
        let now = routes(c, f)?;
        let mixing = Mixing::read(c, f)?;
        let writes = self
            .writes(f, &now, &mixing)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
        for w in &writes {
            c.write(w.numid, &[w.value])?;
        }
        Ok(writes)
    }
}

#[cfg(test)]
mod tests;
