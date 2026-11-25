//! The seam: a desk is what a surface plays, a transport and strips, as state to read and
//! commands to send. A tool is a desk through an adapter of its own, as Dexter's engine is
//! (Dexter's `docs/faderport.md`, "The desk"); [`crate::testbed`] is one with no sound.

use std::fmt;
use std::hash::{Hash, Hasher};

/// A level in hundredths of a dB; `Level::OFF` is −∞.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Level(pub i32);

impl Level {
    pub const OFF: Level = Level(i32::MIN);
    pub const UNITY: Level = Level(0);

    /// dB, to the hundredth; at or below −100 dB is off.
    pub fn from_db(db: f64) -> Level {
        if db <= -100.0 { Level::OFF } else { Level((db * 100.0).round() as i32) }
    }

    /// dB, or none for −∞.
    pub fn db(self) -> Option<f64> {
        (self != Level::OFF).then_some(self.0 as f64 / 100.0)
    }
}

impl fmt::Display for Level {
    /// `-6.5`, `+2.0`, `0.0`, `-inf`: a tenth of a dB.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.db() {
            None => f.write_str("-inf"),
            Some(d) if d > 0.0 => write!(f, "+{d:.1}"),
            Some(d) => write!(f, "{:.1}", if d.abs() < 0.05 { 0.0 } else { d }),
        }
    }
}

/// A fader's travel: points of position (0 the bottom, 1 the top) against dB, straight between
/// them; the bottom is off. A desk says each strip's in [`Has`], so a fader moves on the surface
/// as it does on the desk's own screen.
#[derive(Clone, Copy, Debug)]
pub struct Taper {
    points: &'static [(f64, f64)],
}

/// At or under this the fader is off (the FaderPort's motor goes just shy of the bottom).
const OFF_AT: f64 = 0.012;

impl Taper {
    /// Dexter's, as its MIX page has it (its `FADER_LAW`): −80 dB (off) to +6, 0 dB at four
    /// fifths.
    pub const DEXTER: Taper =
        Taper::new(&[(0.0, -80.0), (0.15, -50.0), (0.35, -30.0), (0.6, -12.0), (0.8, 0.0), (1.0, 6.0)]);

    /// A desk's own law. Panics (in a `const`, fails to build) unless there are two points at
    /// least, from position 0 to 1, position and dB both rising, the bottom no lower than
    /// −100 dB.
    pub const fn new(points: &'static [(f64, f64)]) -> Taper {
        assert!(points.len() >= 2, "a taper has two points at least");
        assert!(points[0].0 == 0.0 && points[points.len() - 1].0 == 1.0, "a taper goes from 0 to 1");
        assert!(points[0].1 >= -100.0, "a taper's bottom is -100 dB at lowest");
        let mut i = 1;
        while i < points.len() {
            assert!(
                points[i].0 > points[i - 1].0 && points[i].1 > points[i - 1].1,
                "a taper rises, in position and in dB"
            );
            i += 1;
        }
        Taper { points }
    }

    /// Its points, position against dB.
    pub fn points(self) -> &'static [(f64, f64)] {
        self.points
    }

    /// The bottom's level, in hundredths of a dB: at or under it is off.
    pub fn floor(self) -> i32 {
        (self.points[0].1 * 100.0).round() as i32
    }

    /// The level at the top.
    pub fn top(self) -> Level {
        Level::from_db(self.points[self.points.len() - 1].1)
    }

    /// The level at a fader's position, to a tenth of a dB.
    pub fn level_at(self, pos: f64) -> Level {
        if pos <= OFF_AT {
            return Level::OFF;
        }
        for w in self.points.windows(2) {
            let ((p0, d0), (p1, d1)) = (w[0], w[1]);
            if pos <= p1 {
                let db = d0 + (d1 - d0) * (pos - p0) / (p1 - p0);
                return Level::from_db((db * 10.0).round() / 10.0);
            }
        }
        self.top()
    }

    /// Where a fader stands at a level; at or below the bottom's, off.
    pub fn position(self, l: Level) -> f64 {
        let pts = self.points;
        let Some(db) = l.db().filter(|&d| d > pts[0].1) else { return 0.0 };
        for w in pts.windows(2) {
            let ((p0, d0), (p1, d1)) = (w[0], w[1]);
            if db <= d1 {
                return (p0 + (p1 - p0) * (db - d0) / (d1 - d0)).max(OFF_AT + 1e-9);
            }
        }
        1.0
    }
}

impl Default for Taper {
    fn default() -> Taper {
        Taper::DEXTER
    }
}

impl PartialEq for Taper {
    fn eq(&self, o: &Taper) -> bool {
        self.points == o.points
    }
}

// `new` lets no NaN in.
impl Eq for Taper {}

impl Hash for Taper {
    fn hash<H: Hasher>(&self, h: &mut H) {
        for (p, d) in self.points {
            // −0 and 0 are equal, so they hash alike.
            ((p + 0.0).to_bits(), (d + 0.0).to_bits()).hash(h);
        }
    }
}

/// What the transport's doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    Stop,
    Play,
    Rec,
    Ff,
    Rew,
}

/// The knob's PAN: −100 (left) to 100 (right), in hundredths, and where SHIFT with a push (or a
/// push held) puts it back.
pub const PAN_MIN: i32 = -100;
pub const PAN_MAX: i32 = 100;
pub const PAN_CENTRE: i32 = 0;

/// What a strip is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A track, in the mix the faders play.
    #[default]
    Track,
    /// An input of the interface that isn't a track's (a Focusrite's mixer): its fader, PAN and
    /// MUTE in the mix.
    Input,
    /// An output's master level: a mix's, MAIN, CONTROL ROOM or an AUX. Reserved: no desk has
    /// them yet (Dexter's `docs/faderport.md`, "Output levels"), so none is listed; a surface reaches
    /// them as it reaches any strip, past the tracks and the inputs, with the fader alone.
    Out,
}

/// Which controls a strip has: a desk without one leaves it out, and the surface skips it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Has {
    pub fader: bool,
    /// The fader's travel.
    pub taper: Taper,
    /// PAN, on the knob.
    pub pan: bool,
    pub mute: bool,
    pub solo: bool,
    pub arm: bool,
}

impl Has {
    pub fn all() -> Has {
        Has { fader: true, taper: Taper::DEXTER, pan: true, mute: true, solo: true, arm: true }
    }
}

/// A strip as it's set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Strip {
    pub name: String,
    pub kind: Kind,
    pub has: Has,
    pub fader: Level,
    /// PAN, in hundredths (−100 left, 100 right).
    pub pan: i32,
    pub mute: bool,
    pub solo: bool,
    pub arm: bool,
    /// Armed while the transport records.
    pub recording: bool,
}

/// The whole desk, as it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeskState {
    pub mode: Mode,
    /// The counter, in frames.
    pub position: u64,
    /// Frames a second.
    pub rate: u32,
    /// Sorted.
    pub marks: Vec<u64>,
    pub strips: Vec<Strip>,
    /// The mix the faders and PAN play, on a desk with more than one (Dexter's).
    pub mix: Option<Mix>,
}

/// One of a desk's mixes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mix {
    pub name: String,
    /// The control room's, where SOLO is heard: the one played to begin with.
    pub control: bool,
}

/// What a surface asks of a desk. Strips are numbered from 1, as the pages show them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmd {
    Transport(Mode),
    Rtz,
    /// The previous mark (−1) or the next (+1).
    Locate(i8),
    Mark,
    /// The mark within a tenth of a second of the counter removed; only while stopped.
    Unmark,
    /// The counter moved by milliseconds, back if negative (not past zero): rolling, it rolls
    /// on from there; winding, it stops there.
    Nudge(i32),
    Fader {
        n: usize,
        level: Level,
    },
    /// A strip's PAN, in hundredths.
    Pan {
        n: usize,
        value: i32,
    },
    Mute {
        n: usize,
        on: bool,
    },
    Solo {
        n: usize,
        on: bool,
    },
    Arm {
        n: usize,
        on: bool,
    },
    /// The faders and PAN to the next mix, round to the first.
    NextMix,
    /// The faders and PAN to the control room's mix.
    ControlMix,
}

/// A command refused: the desk's code for why, and a text for the screen. The surface blinks the
/// button on any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub code: &'static str,
    pub text: String,
}

impl Refusal {
    pub fn new(code: &'static str, text: &str) -> Refusal {
        Refusal { code, text: text.into() }
    }
}

/// What a surface plays.
pub trait Desk: Send + Sync {
    fn state(&self) -> DeskState;
    fn command(&self, c: Cmd) -> Result<(), Refusal>;
    /// What it is, for the screen: `testbed`, or a tool's project, or where it's looked for.
    fn name(&self) -> String;
    /// Refusals that came after their command was sent (the mixer answers only to refuse),
    /// each once.
    fn refusals(&self) -> Vec<Refusal> {
        Vec::new()
    }
    /// The strip the surface is on (from 1), or none while it's on none (unplugged, or past
    /// the last strip): said often, for a desk that shows it (Dexter's page frames that
    /// track), which passes it on as it changes.
    fn focus(&self, _strip: Option<usize>) {}
    /// Whether the surface is open (plugged in, its port held), said before each `focus`, for a
    /// desk that shows it (Dexter's page's FP light).
    fn open(&self, _open: bool) {}
    /// A strip picked on the desk's own screen since last asked (Dexter's page's strip cursor),
    /// for the surface to go to: each pick once.
    fn picked(&self) -> Option<usize> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels() {
        assert_eq!(Level::from_db(-6.5), Level(-650));
        assert_eq!(Level::from_db(-120.0), Level::OFF);
        assert_eq!(Level(-650).to_string(), "-6.5");
        assert_eq!(Level(200).to_string(), "+2.0");
        assert_eq!(Level(0).to_string(), "0.0");
        assert_eq!(Level::OFF.to_string(), "-inf");
    }

    #[test]
    fn dexters_taper() {
        let t = Taper::DEXTER;
        assert_eq!(
            (t.level_at(0.0), t.level_at(0.8), t.level_at(1.0)),
            (Level::OFF, Level::UNITY, Level(600))
        );
        assert_eq!((t.level_at(0.6), t.level_at(0.35)), (Level(-1200), Level(-3000)));
        assert_eq!(
            (t.position(Level::UNITY), t.position(Level(600)), t.position(Level(1000))),
            (0.8, 1.0, 1.0)
        );
        assert_eq!(t.position(Level(-8000)), 0.0, "-80 dB is off");
        for x in -750..=60 {
            let l = Level(x * 10);
            assert_eq!(t.level_at(t.position(l)), l, "{l}");
        }
    }

    /// A second desk's law is its own, and moves as its points say.
    #[test]
    fn a_desks_own_taper() {
        const PLAIN: Taper = Taper::new(&[(0.0, -60.0), (0.5, -20.0), (1.0, 0.0)]);
        assert_eq!((PLAIN.floor(), PLAIN.top()), (-6000, Level::UNITY));
        assert_eq!(
            (PLAIN.level_at(0.0), PLAIN.level_at(0.5), PLAIN.level_at(0.75)),
            (Level::OFF, Level(-2000), Level(-1000))
        );
        assert_eq!((PLAIN.position(Level(-2000)), PLAIN.position(Level(600))), (0.5, 1.0));
        assert_ne!(PLAIN, Taper::DEXTER);
        assert_eq!(Taper::default(), Taper::DEXTER);
    }

    #[test]
    #[should_panic(expected = "a taper rises")]
    fn a_taper_that_falls_is_refused() {
        Taper::new(&[(0.0, -60.0), (0.5, 0.0), (1.0, -10.0)]);
    }
}
