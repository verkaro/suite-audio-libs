//! The interface's own controls beside its mixer (`docs/interface.md`): its inputs' switches,
//! 48V, the clock and whether it's locked, DIM and MUTE, the talkback, and what each capture
//! channel records.
//! Read and set one at a time; never the mixer's gains, which read wrong (`crate::mixer`).

use std::io;
use std::sync::{Arc, Mutex};

use crate::controls::{Control, Controls};
use crate::interface::Focusrite;

/// `Talkback Playback Enum`'s items: `Disabled`, `Off` (the front panel's button works), `On`.
pub const TALK_DISABLED: i64 = 0;
pub const TALK_OFF: i64 = 1;
pub const TALK_ON: i64 = 2;

/// One card's controls shared between the mixer's thread and the panel, used one at a time.
#[derive(Clone)]
pub struct SharedControls(Arc<Mutex<Box<dyn Controls>>>);

impl SharedControls {
    pub fn new(c: Box<dyn Controls>) -> SharedControls {
        SharedControls(Arc::new(Mutex::new(c)))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Box<dyn Controls>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Controls for SharedControls {
    fn list(&mut self) -> io::Result<Vec<Control>> {
        self.lock().list()
    }

    fn read(&mut self, numid: u32) -> io::Result<Vec<i64>> {
        self.lock().read(numid)
    }

    fn write(&mut self, numid: u32, values: &[i64]) -> io::Result<()> {
        self.lock().write(numid, values)
    }
}

/// One change on the panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// An input (from 1) at instrument level, or line.
    Inst(usize, bool),
    Pad(usize, bool),
    Air(usize, bool),
    /// 48V on the inputs a switch powers (`1-4`).
    Phantom(String, bool),
    /// 48V kept through a power cycle.
    Persists(bool),
    Dim(bool),
    Mute(bool),
    /// The talkback switched on (`On`), as the front panel's TALK button does, or off (`Off`:
    /// the button still works; never `Disabled`).
    Talk(bool),
    /// A line (from 1) on the monitor knob (`HW`), or the program's (`SW`).
    LineHw(usize, bool),
    /// A line's volume, raw ([`crate::Volume`]); refused while it's on the knob.
    LineVolume(usize, i64),
    LineMute(usize, bool),
    /// The interface keeps its routing and mixes with no computer.
    Standalone(bool),
    /// The clock, by its item's name: `Internal`, `ADAT`.
    Clock(String),
    /// What the digital ports carry (the 18i20's `Digital I/O Mode`), by name.
    DigitalMode(String),
    /// Which S/PDIF input a Clarett takes, by name: `Optical`, `RCA`.
    SpdifSource(String),
}

/// A line output as it is; each absent where the interface hasn't it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LineNow {
    /// From 1.
    pub n: usize,
    /// The driver's name for it: `Monitor L`.
    pub label: Option<String>,
    /// On the monitor knob.
    pub hw: Option<bool>,
    /// Its volume, raw.
    pub volume: Option<i64>,
    pub mute: Option<bool>,
}

/// An input's switches as they are; each absent where the input hasn't it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputNow {
    /// From 1.
    pub n: usize,
    pub inst: Option<bool>,
    pub pad: Option<bool>,
    pub air: Option<bool>,
}

/// The panel as it is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Now {
    pub inputs: Vec<InputNow>,
    /// 48V by the inputs each switch powers.
    pub phantom: Vec<(String, bool)>,
    pub persists: Option<bool>,
    /// The clock (`Internal`), and those there are.
    pub clock: Option<String>,
    pub clocks: Vec<String>,
    /// The clock locked.
    pub sync: Option<bool>,
    pub dim: Option<bool>,
    pub mute: Option<bool>,
    /// The talkback on (`On`), from Dexter or the front panel's button.
    pub talk: Option<bool>,
    /// What each capture channel records (from 1), by source name.
    pub captures: Vec<String>,
    /// The monitor knob's level, raw ([`Focusrite::knob`]).
    pub knob: Option<i64>,
    pub lines: Vec<LineNow>,
    pub standalone: Option<bool>,
    pub firmware: Option<i64>,
    /// Each clock source, and whether it has a signal to lock to.
    pub validity: Vec<(String, bool)>,
    /// What the digital ports carry, and which S/PDIF input is taken, by name.
    pub digital_mode: Option<String>,
    pub spdif_source: Option<String>,
}

/// The interface's own controls.
pub struct Panel {
    c: Box<dyn Controls>,
    f: Focusrite,
}

impl Panel {
    pub fn new(c: Box<dyn Controls>, f: Focusrite) -> Panel {
        Panel { c, f }
    }

    pub fn interface(&self) -> &Focusrite {
        &self.f
    }

    fn get(&mut self, numid: Option<u32>) -> io::Result<Option<i64>> {
        numid.map(|n| self.c.read(n).map(|v| v.first().copied().unwrap_or(0))).transpose()
    }

    /// Reads it as it is.
    pub fn now(&mut self) -> io::Result<Now> {
        let on = |v: Option<i64>| v.map(|v| v != 0);
        let mut inputs = Vec::new();
        for i in self.f.inputs.clone() {
            inputs.push(InputNow {
                n: i.n,
                inst: on(self.get(i.inst)?),
                pad: on(self.get(i.pad)?),
                air: on(self.get(i.air)?),
            });
        }
        let mut phantom = Vec::new();
        for (g, numid) in self.f.phantom.clone() {
            phantom.push((g, on(self.get(Some(numid))?).unwrap_or(false)));
        }
        let (clock, clocks) = match self.f.clock.clone() {
            Some((numid, items)) => {
                let v = self.get(Some(numid))?.unwrap_or(0) as usize;
                (items.get(v).cloned(), items)
            }
            None => (None, Vec::new()),
        };
        let sync = self.get(self.f.sync)?.map(|v| v == 1);
        let mut captures = Vec::new();
        for numid in self.f.capture_ids.clone() {
            let v = self.get(Some(numid))?.unwrap_or(0) as usize;
            captures.push(self.f.sources.get(v).cloned().unwrap_or_default());
        }
        let mut lines = Vec::new();
        for l in self.f.lines.clone() {
            lines.push(LineNow {
                n: l.n,
                label: l.label.clone(),
                hw: on(self.get(l.control)?),
                volume: self.get(l.volume.map(|v| v.numid))?,
                mute: on(self.get(l.mute)?),
            });
        }
        let mut validity = Vec::new();
        for (clock, numid) in self.f.validity.clone() {
            validity.push((clock, on(self.get(Some(numid))?).unwrap_or(false)));
        }
        let (dm, ss) = (self.f.digital_mode.clone(), self.f.spdif_source.clone());
        let mut item = |e: Option<(u32, Vec<String>)>| -> io::Result<Option<String>> {
            let Some((numid, items)) = e else { return Ok(None) };
            let v = self.get(Some(numid))?.unwrap_or(0) as usize;
            Ok(items.get(v).cloned())
        };
        let digital_mode = item(dm)?;
        let spdif_source = item(ss)?;
        Ok(Now {
            digital_mode,
            spdif_source,
            lines,
            standalone: on(self.get(self.f.standalone)?),
            firmware: self.get(self.f.firmware)?,
            validity,
            inputs,
            phantom,
            persists: on(self.get(self.f.phantom_persists)?),
            clock,
            clocks,
            sync,
            dim: on(self.get(self.f.dim)?),
            mute: on(self.get(self.f.mute)?),
            talk: self.get(self.f.talkback)?.map(|v| v == TALK_ON),
            captures,
            knob: self.get(self.f.knob.map(|k| k.numid))?,
        })
    }

    /// Makes a change; an error that's `NotFound` when the interface hasn't that switch.
    pub fn set(&mut self, ch: &Change) -> io::Result<()> {
        if let Change::Talk(on) = ch {
            let Some(numid) = self.f.talkback else {
                return Err(io::Error::new(io::ErrorKind::NotFound, "the interface hasn't a talkback"));
            };
            return self.c.write(numid, &[if *on { TALK_ON } else { TALK_OFF }]);
        }
        // An enum by its item's name.
        let named = match ch {
            Change::Clock(s) => Some((self.f.clock.clone(), s, "a clock")),
            Change::DigitalMode(s) => Some((self.f.digital_mode.clone(), s, "a digital mode")),
            Change::SpdifSource(s) => Some((self.f.spdif_source.clone(), s, "an S/PDIF source")),
            _ => None,
        };
        if let Some((e, want, what)) = named {
            let Some((numid, items)) = e else {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("the interface hasn't {what} to set"),
                ));
            };
            let Some(i) = items.iter().position(|x| x.eq_ignore_ascii_case(want.trim())) else {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("no such item: {want}")));
            };
            return self.c.write(numid, &[i as i64]);
        }
        let line = |n: usize| self.f.lines.iter().find(|l| l.n == n);
        if let Change::LineVolume(n, raw) = ch {
            let Some(v) = line(*n).and_then(|l| l.volume) else {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "the interface hasn't that line's volume",
                ));
            };
            return self.c.write(v.numid, &[(*raw).clamp(v.min, v.max)]);
        }
        let input = |n: usize| self.f.inputs.iter().find(|i| i.n == n);
        let numid = match ch {
            Change::Inst(n, _) => input(*n).and_then(|i| i.inst),
            Change::Pad(n, _) => input(*n).and_then(|i| i.pad),
            Change::Air(n, _) => input(*n).and_then(|i| i.air),
            Change::Phantom(g, _) => self.f.phantom.iter().find(|(x, _)| x == g.trim()).map(|p| p.1),
            Change::Persists(_) => self.f.phantom_persists,
            Change::Dim(_) => self.f.dim,
            Change::Mute(_) => self.f.mute,
            Change::LineHw(n, _) => line(*n).and_then(|l| l.control),
            Change::LineMute(n, _) => line(*n).and_then(|l| l.mute),
            Change::Standalone(_) => self.f.standalone,
            Change::Talk(_) | Change::LineVolume(..) => unreachable!("set above"),
            Change::Clock(_) | Change::DigitalMode(_) | Change::SpdifSource(_) => unreachable!("set above"),
        };
        let on = match ch {
            Change::Inst(_, b) | Change::Pad(_, b) | Change::Air(_, b) | Change::Phantom(_, b) => *b,
            Change::LineHw(_, b) | Change::LineMute(_, b) => *b,
            Change::Persists(b) | Change::Dim(b) | Change::Mute(b) | Change::Talk(b) => *b,
            Change::Standalone(b) => *b,
            Change::LineVolume(..) | Change::Clock(_) | Change::DigitalMode(_) | Change::SpdifSource(_) => {
                unreachable!("set above")
            }
        };
        let Some(numid) = numid else {
            return Err(io::Error::new(io::ErrorKind::NotFound, "the interface hasn't that switch"));
        };
        self.c.write(numid, &[on as i64])
    }
}
