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
        Ok(Now {
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
        let input = |n: usize| self.f.inputs.iter().find(|i| i.n == n);
        let numid = match ch {
            Change::Inst(n, _) => input(*n).and_then(|i| i.inst),
            Change::Pad(n, _) => input(*n).and_then(|i| i.pad),
            Change::Air(n, _) => input(*n).and_then(|i| i.air),
            Change::Phantom(g, _) => self.f.phantom.iter().find(|(x, _)| x == g.trim()).map(|p| p.1),
            Change::Persists(_) => self.f.phantom_persists,
            Change::Dim(_) => self.f.dim,
            Change::Mute(_) => self.f.mute,
            Change::Talk(_) => unreachable!("set above"),
        };
        let on = match ch {
            Change::Inst(_, b) | Change::Pad(_, b) | Change::Air(_, b) | Change::Phantom(_, b) => *b,
            Change::Persists(b) | Change::Dim(b) | Change::Mute(b) | Change::Talk(b) => *b,
        };
        let Some(numid) = numid else {
            return Err(io::Error::new(io::ErrorKind::NotFound, "the interface hasn't that switch"));
        };
        self.c.write(numid, &[on as i64])
    }
}
