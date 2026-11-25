//! The testbed: two silent strips and a simulated transport, a desk of its own, refusing
//! where a deck would (as Dexter does). A tool tries the surface's rules on it with no tool of
//! its own, and no device but the surface.

use std::sync::Mutex;
use std::time::Instant;

use crate::desk::{
    Cmd, Desk, DeskState, Has, Kind, Level, Mode, PAN_CENTRE, PAN_MAX, PAN_MIN, Refusal, Strip, Taper,
};

/// How fast FF and REW wind.
pub const WIND: f64 = 8.0;
/// Marks closer than this (a tenth of a second) are one; locating back from within it goes
/// to the mark before.
const NEAR: f64 = 0.1;

/// The testbed's desk.
pub struct Testbed(Mutex<Inner>);

struct Inner {
    mode: Mode,
    /// Frames, kept fractional while winding.
    position: f64,
    rate: u32,
    marks: Vec<u64>,
    strips: Vec<Strip>,
    /// When position was last brought up to date.
    at: Instant,
}

impl Testbed {
    /// Two strips, `1` and `2`, at 0 dB with everything centred; stopped at zero.
    pub fn new(rate: u32, now: Instant) -> Testbed {
        let strip = |n: usize| Strip {
            name: n.to_string(),
            kind: Kind::Track,
            has: Has::all(),
            fader: Level::UNITY,
            pan: PAN_CENTRE,
            mute: false,
            solo: false,
            arm: false,
            recording: false,
        };
        Testbed(Mutex::new(Inner {
            mode: Mode::Stop,
            position: 0.0,
            rate,
            marks: Vec::new(),
            strips: vec![strip(1), strip(2)],
            at: now,
        }))
    }

    pub fn state_at(&self, now: Instant) -> DeskState {
        let mut g = self.0.lock().unwrap_or_else(|e| e.into_inner());
        g.run(now);
        DeskState {
            mode: g.mode,
            position: g.position as u64,
            rate: g.rate,
            marks: g.marks.clone(),
            strips: g
                .strips
                .iter()
                .map(|s| Strip { recording: s.arm && g.mode == Mode::Rec, ..s.clone() })
                .collect(),
            mix: None,
        }
    }

    pub fn command_at(&self, c: Cmd, now: Instant) -> Result<(), Refusal> {
        let mut g = self.0.lock().unwrap_or_else(|e| e.into_inner());
        g.run(now);
        g.command(c)
    }
}

impl Desk for Testbed {
    fn state(&self) -> DeskState {
        self.state_at(Instant::now())
    }

    fn command(&self, c: Cmd) -> Result<(), Refusal> {
        self.command_at(c, Instant::now())
    }

    fn name(&self) -> String {
        "testbed".into()
    }
}

fn recording() -> Refusal {
    Refusal::new("recording", "not while recording")
}

impl Inner {
    /// The counter brought up to now.
    fn run(&mut self, now: Instant) {
        let dt = now.saturating_duration_since(self.at).as_secs_f64() * self.rate as f64;
        self.at = now;
        match self.mode {
            Mode::Play | Mode::Rec => self.position += dt,
            Mode::Ff => self.position += dt * WIND,
            Mode::Rew => {
                self.position -= dt * WIND;
                if self.position <= 0.0 {
                    self.position = 0.0;
                    self.mode = Mode::Stop;
                }
            }
            Mode::Stop => {}
        }
    }

    fn strip(&mut self, n: usize) -> Result<&mut Strip, Refusal> {
        n.checked_sub(1)
            .and_then(|i| self.strips.get_mut(i))
            .ok_or_else(|| Refusal::new("no_track", "no such track"))
    }

    /// Stopped where a wind was, or rolling on where play was.
    fn moved(&mut self, to: f64) {
        self.position = to;
        if matches!(self.mode, Mode::Ff | Mode::Rew) {
            self.mode = Mode::Stop;
        }
    }

    fn command(&mut self, c: Cmd) -> Result<(), Refusal> {
        let rec = self.mode == Mode::Rec;
        let near = NEAR * self.rate as f64;
        match c {
            Cmd::Transport(Mode::Ff | Mode::Rew) if rec => return Err(recording()),
            Cmd::Transport(m) => self.mode = m,
            Cmd::Rtz | Cmd::Locate(_) | Cmd::Nudge(_) if rec => return Err(recording()),
            Cmd::Nudge(ms) => self.moved((self.position + ms as f64 * self.rate as f64 / 1000.0).max(0.0)),
            Cmd::Rtz => self.moved(0.0),
            Cmd::Locate(dir) if dir < 0 => {
                let to = self.marks.iter().rev().find(|&&m| (m as f64) < self.position - near).copied();
                self.moved(to.unwrap_or(0) as f64);
            }
            Cmd::Locate(_) => {
                let Some(to) = self.marks.iter().find(|&&m| m as f64 > self.position).copied() else {
                    return Err(Refusal::new("no_mark", "no next mark"));
                };
                self.moved(to as f64);
            }
            Cmd::Mark => {
                let at = self.position as u64;
                if !self.marks.iter().any(|&m| (m as f64 - at as f64).abs() < near) {
                    self.marks.push(at);
                    self.marks.sort_unstable();
                }
            }
            Cmd::Unmark if rec => return Err(recording()),
            Cmd::Unmark if self.mode != Mode::Stop => {
                return Err(Refusal::new("rolling", "stop the deck first"));
            }
            Cmd::Unmark => {
                let Some(i) = self.marks.iter().position(|&m| (m as f64 - self.position).abs() < near) else {
                    return Err(Refusal::new("not_on_mark", "no mark here"));
                };
                self.marks.remove(i);
            }
            Cmd::Fader { n, level } => {
                self.strip(n)?.fader =
                    if level == Level::OFF { level } else { level.min(Taper::DEXTER.top()) };
            }
            Cmd::Pan { n, value } => self.strip(n)?.pan = value.clamp(PAN_MIN, PAN_MAX),
            Cmd::Mute { n, on } => self.strip(n)?.mute = on,
            Cmd::Solo { n, on } => self.strip(n)?.solo = on,
            Cmd::Arm { n, on } => self.strip(n)?.arm = on,
            Cmd::NextMix | Cmd::ControlMix => return Err(Refusal::new("no_mix", "the testbed has one mix")),
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "testbed_tests.rs"]
mod tests;
