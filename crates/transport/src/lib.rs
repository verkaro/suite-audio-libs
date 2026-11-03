//! A clock that reads JACK's transport and never moves it (written fresh for ducky, 2026-11-03,
//! after the way `as-jack-keeper` opens and loses a client). [`Clock::jack`] is a JACK client
//! of no ports and no sync callback; a thread of its own polls the transport every 10 ms and
//! publishes the state and frame through atomics, so [`Clock::now`] never waits and never calls
//! JACK. It doesn't start the server, and it reattaches when the server comes back. While the
//! server is away `now` is `None`, never the frame it last saw. [`Clock::sim`] is the same
//! shape with no JACK: a transport of its own that the caller rolls and locates.
//!
//! This crate never asks JACK to start, stop or move the transport, registers no port and no sync
//! callback; a test (`clock_never_controls`) reads this crate's source for the calls that would.

mod jack_clock;
#[cfg(test)]
mod tests;

use std::sync::Mutex;
use std::time::Instant;

pub use jack_clock::RETRY;

/// The transport's state. JACK's three, and `Other` for what this build doesn't know.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Stopped,
    Rolling,
    Starting,
    Other,
}

/// Where the transport is: its state, its frame (JACK's are 32 bits) and the server's rate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub state: State,
    pub frame: u32,
    pub rate: u32,
}

/// A clock: JACK's transport read, or a simulated one.
pub struct Clock(Kind);

enum Kind {
    Jack(jack_clock::JackClock),
    Sim(Sim),
}

impl Clock {
    /// A client named `name` on the server `JACK_DEFAULT_SERVER` names (the default one if it
    /// isn't set). It never starts the server; while it can't open the client it tries again every
    /// second, and `now` is `None`.
    pub fn jack(name: &str) -> Clock {
        Clock(Kind::Jack(jack_clock::JackClock::new(name)))
    }

    /// A simulated clock at `rate`, stopped at frame 0.
    pub fn sim(rate: u32) -> Clock {
        Clock(Kind::Sim(Sim::new(rate)))
    }

    /// The transport now, or `None` while there is no server (JACK) to read.
    pub fn now(&self) -> Option<Position> {
        match &self.0 {
            Kind::Jack(j) => j.now(),
            Kind::Sim(s) => Some(s.now()),
        }
    }

    /// Whether this is the simulated clock.
    pub fn is_sim(&self) -> bool {
        matches!(self.0, Kind::Sim(_))
    }

    /// Rolls or halts the simulated clock where it is. Does nothing on a JACK clock.
    pub fn sim_roll(&self, on: bool) {
        if let Kind::Sim(s) = &self.0 {
            s.set_rolling(on);
        }
    }

    /// Moves the simulated clock. Does nothing on a JACK clock.
    pub fn sim_locate(&self, frame: u32) {
        if let Kind::Sim(s) = &self.0 {
            s.jump(frame);
        }
    }

    /// Ends the clock's thread and closes its client (unless the server is gone: then the
    /// client is forgotten, as closing would wait on a server that isn't there).
    pub fn stop(self) {
        if let Kind::Jack(j) = self.0 {
            j.end();
        }
    }
}

/// The simulated transport: where it was when it last changed, and when.
struct Sim {
    rate: u32,
    at: Mutex<SimAt>,
}

#[derive(Clone, Copy)]
struct SimAt {
    rolling: bool,
    frame: f64,
    since: Instant,
}

impl Sim {
    fn new(rate: u32) -> Sim {
        Sim { rate, at: Mutex::new(SimAt { rolling: false, frame: 0.0, since: Instant::now() }) }
    }

    fn frame_of(&self, a: &SimAt) -> f64 {
        let f =
            a.frame + if a.rolling { a.since.elapsed().as_secs_f64() * f64::from(self.rate) } else { 0.0 };
        f.min(f64::from(u32::MAX))
    }

    fn now(&self) -> Position {
        let a = *self.at.lock().expect("the sim's lock");
        Position {
            state: if a.rolling { State::Rolling } else { State::Stopped },
            frame: self.frame_of(&a) as u32,
            rate: self.rate,
        }
    }

    fn set_rolling(&self, on: bool) {
        let mut a = self.at.lock().expect("the sim's lock");
        let frame = self.frame_of(&a);
        *a = SimAt { rolling: on, frame, since: Instant::now() };
    }

    fn jump(&self, frame: u32) {
        let mut a = self.at.lock().expect("the sim's lock");
        *a = SimAt { rolling: a.rolling, frame: f64::from(frame), since: Instant::now() };
    }
}
