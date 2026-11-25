//! The surface's rules, as pure functions of what's done, the time and the desk's state
//! (Dexter's `docs/faderport.md`, "The binding"). [`Controls`] is what any control does, whether a
//! surface's or a key on the screen: the strip selected, the knob's control, the toggles, the
//! strips linked.

pub mod surface;

use crate::desk::{Cmd, DeskState, Level, Mode, PAN_CENTRE, PAN_MAX, PAN_MIN, Taper};

/// What's done, on a surface or the screen's keys.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Input {
    /// PLAY/PAUSE: PLAY; playing, STOP where it is; recording, PLAY, a punch-out (Warren).
    PlayStop,
    Play,
    Stop,
    Rec,
    Rew,
    Ff,
    Rtz,
    /// The previous mark (−1) or the next (+1).
    Locate(i8),
    Mark,
    /// A mark where the counter is, or the one it's on removed.
    MarkToggle,
    /// The counter moved by steps of a tenth of a second, forward positive; fine, a
    /// hundredth.
    Nudge {
        steps: i32,
        fine: bool,
    },
    /// Strip n (from 1).
    Select(usize),
    /// The strip before (−1) or after (+1).
    Next(i8),
    /// The selected strip's fader to a position, 0 the bottom, 1 the top.
    Fader(f64),
    /// The selected strip's fader moved by part of its travel.
    FaderBy(f64),
    /// The selected strip's fader to 0 dB.
    Unity,
    /// The knob turned by steps, right positive; fine with SHIFT: the strip's PAN.
    Knob {
        steps: i32,
        fine: bool,
    },
    /// The selected strip's PAN back to the centre (SHIFT with a push, or a push held).
    PanCentre,
    Mute,
    Solo,
    Arm,
    /// The selected strip into the link, or out of it.
    Link,
    /// The faders and PAN to the desk's next mix.
    NextMix,
    /// The faders and PAN to the control room's mix.
    ControlMix,
}

/// The surface's own state: which strip, and the strips linked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Controls {
    /// The strip selected, from 1.
    pub strip: usize,
    /// The strips in the link, from 1, in order: a move on one moves the others alike.
    pub linked: Vec<usize>,
    /// Where each linked strip's fader would be, in hundredths of a dB, past −∞ (−100 dB)
    /// and +10 dB too: the balance a strip held at an end comes back to. Forgotten when it's
    /// unlinked, or when its fader isn't where this says (another moved it).
    pub held: Vec<(usize, i32)>,
}

impl Default for Controls {
    fn default() -> Controls {
        Controls { strip: 1, linked: Vec::new(), held: Vec::new() }
    }
}

/// Where linked faders' balance is kept: −∞ as −100 dB (the lowest taper's foot).
const FLOOR: i32 = -10_000;

/// A level as a linked fader's balance has it.
fn held_at(l: Level) -> i32 {
    if l == Level::OFF { FLOOR } else { l.0 }
}

/// The level a linked fader's balance shows, within the fader's ends.
fn shown(x: i32, t: Taper) -> Level {
    if x <= t.floor() { Level::OFF } else { Level(x.min(t.top().0)) }
}

impl Controls {
    /// What input asks of the desk, the selected strip's and, when it's linked, the other
    /// linked strips' alike: a fader by as many dB, the balance kept past the ends
    /// (`held`); the knob by as many steps (each keeping its offset), PAN back to the centre;
    /// MUTE, SOLO and ARM set as the selected strip's is. A strip without the control is
    /// passed over.
    pub fn act_all(&mut self, input: Input, s: &DeskState) -> Vec<Cmd> {
        let Some(c) = self.act(input, s) else { return vec![] };
        let n = self.strip;
        let mut out = vec![c];
        let Some(sel) = s.strips.get(n - 1).filter(|_| self.linked.contains(&n)) else { return out };
        // The selected fader's move in dB, from where its balance was.
        let by = match c {
            Cmd::Fader { level, .. } => {
                let by = held_at(level) - self.held_level(n, sel.fader, sel.has.taper);
                self.hold(n, held_at(level));
                by
            }
            _ => 0,
        };
        for m in self.linked.clone().into_iter().filter(|&m| m != n) {
            let Some(st) = s.strips.get(m - 1) else { continue };
            let has = &st.has;
            out.push(match c {
                Cmd::Fader { .. } if has.fader => {
                    let to = self.held_level(m, st.fader, has.taper) + by;
                    self.hold(m, to);
                    Cmd::Fader { n: m, level: shown(to, has.taper) }
                }
                Cmd::Pan { .. } if has.pan => match input {
                    Input::Knob { steps, fine } => Cmd::Pan { n: m, value: turned(st.pan, steps, fine) },
                    _ => Cmd::Pan { n: m, value: PAN_CENTRE },
                },
                Cmd::Mute { on, .. } if has.mute => Cmd::Mute { n: m, on },
                Cmd::Solo { on, .. } if has.solo => Cmd::Solo { n: m, on },
                Cmd::Arm { on, .. } if has.arm => Cmd::Arm { n: m, on },
                _ => continue,
            });
        }
        out
    }

    /// Where strip m's fader balance is: as held, if its fader is still where that shows,
    /// else where its fader is.
    fn held_level(&self, m: usize, fader: Level, t: Taper) -> i32 {
        match self.held.iter().find(|h| h.0 == m) {
            Some(&(_, x)) if shown(x, t) == fader => x,
            _ => held_at(fader),
        }
    }

    fn hold(&mut self, m: usize, x: i32) {
        self.held.retain(|h| h.0 != m);
        self.held.push((m, x));
    }

    /// What input asks of the desk for the selected strip alone, as it is; the selection and
    /// the link change here.
    pub fn act(&mut self, input: Input, s: &DeskState) -> Option<Cmd> {
        let n = self.strip;
        let strip = s.strips.get(n.wrapping_sub(1));
        self.keep_strip(s);
        Some(match input {
            Input::PlayStop => Cmd::Transport(if s.mode == Mode::Play { Mode::Stop } else { Mode::Play }),
            Input::Play => Cmd::Transport(Mode::Play),
            Input::Stop => Cmd::Transport(Mode::Stop),
            Input::Rec => Cmd::Transport(Mode::Rec),
            Input::Rew => Cmd::Transport(Mode::Rew),
            Input::Ff => Cmd::Transport(Mode::Ff),
            Input::Rtz => Cmd::Rtz,
            Input::Locate(d) => Cmd::Locate(d),
            Input::Mark => Cmd::Mark,
            Input::Nudge { steps, fine } => Cmd::Nudge(steps * if fine { 10 } else { 100 }),
            Input::MarkToggle => {
                let near = s.rate as f64 / 10.0;
                let on = s.marks.iter().any(|&m| (m as f64 - s.position as f64).abs() < near);
                if on { Cmd::Unmark } else { Cmd::Mark }
            }
            Input::Select(to) => {
                if (1..=s.strips.len()).contains(&to) {
                    self.strip = to;
                    self.keep_strip(s);
                }
                return None;
            }
            Input::Next(d) => {
                let to = n as i64 + d as i64;
                return self.act(Input::Select(to.clamp(1, s.strips.len().max(1) as i64) as usize), s);
            }
            // A control the strip hasn't (Dexter's tracks have no fader) does nothing.
            Input::Fader(_) | Input::FaderBy(_) | Input::Unity if !strip?.has.fader => return None,
            Input::Mute if !strip?.has.mute => return None,
            Input::Solo if !strip?.has.solo => return None,
            Input::Arm if !strip?.has.arm => return None,
            Input::Fader(p) => Cmd::Fader { n, level: strip?.has.taper.level_at(p.clamp(0.0, 1.0)) },
            Input::FaderBy(d) => {
                let t = strip?.has.taper;
                Cmd::Fader { n, level: t.level_at((t.position(strip?.fader) + d).clamp(0.0, 1.0)) }
            }
            Input::Unity => Cmd::Fader { n, level: Level::UNITY },
            Input::Knob { .. } | Input::PanCentre if !strip?.has.pan => return None,
            Input::Knob { steps, fine } => Cmd::Pan { n, value: turned(strip?.pan, steps, fine) },
            Input::PanCentre => Cmd::Pan { n, value: PAN_CENTRE },
            Input::NextMix => Cmd::NextMix,
            Input::ControlMix => Cmd::ControlMix,
            Input::Mute => Cmd::Mute { n, on: !strip?.mute },
            Input::Solo => Cmd::Solo { n, on: !strip?.solo },
            Input::Arm => Cmd::Arm { n, on: !strip?.arm },
            Input::Link => {
                strip?;
                match self.linked.iter().position(|&m| m == n) {
                    Some(i) => {
                        self.linked.remove(i);
                        self.held.retain(|h| h.0 != n);
                    }
                    None => {
                        self.linked.push(n);
                        self.linked.sort_unstable();
                    }
                }
                return None;
            }
        })
    }

    /// Keeps the selection on a strip there is.
    fn keep_strip(&mut self, s: &DeskState) {
        self.strip = self.strip.clamp(1, s.strips.len().max(1));
    }
}

/// PAN turned by steps from value: 5 hundredths a step (1 fine); within its range.
pub fn turned(value: i32, steps: i32, fine: bool) -> i32 {
    (value + steps * if fine { 1 } else { 5 }).clamp(PAN_MIN, PAN_MAX)
}

#[cfg(test)]
mod tests;
