//! The interface set by hand, a control at a time, for an app of its own (focusrite-ctrl,
//! 2026-11-10): what each destination plays, a mix's gains, and the mixer's inputs shared out by
//! need, as Dexter's [`crate::Plan`] shares them for a project.
//!
//! A destination is an output, a mixer input or a capture channel: each plays one source, an
//! item of [`Focusrite::sources`]. A mono mix hears every source on a mixer input whose gain in
//! it isn't off. A source in several mixes has one mixer input, shared, and the input is set
//! back to `Off` once no mix hears it.
//!
//! The gains are read from the driver once, at the start, and never again: what's read is the
//! driver's copy, which a Focusrite panel beside this scrambles (`docs/monitor.md`,
//! "alsa-scarlett-gui, closed"). [`Mixing`] keeps what it wrote.

use std::fmt;
use std::io;

use crate::controls::Controls;
use crate::interface::Focusrite;

/// A routing destination.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Dest {
    /// An output, by its place in [`Focusrite::outputs`] (from 0): `LINE 1`, `ADAT 1`.
    Output(usize),
    /// A mixer input, from 1.
    MixerInput(usize),
    /// A capture channel, what the computer records, from 1.
    Capture(usize),
}

impl Focusrite {
    /// Every destination: the outputs, the mixer's inputs, the capture channels.
    pub fn dests(&self) -> Vec<Dest> {
        (0..self.outputs.len())
            .map(Dest::Output)
            .chain((1..=self.mixer_inputs.len()).map(Dest::MixerInput))
            .chain((1..=self.capture_ids.len()).map(Dest::Capture))
            .collect()
    }

    /// A destination's routing enum.
    pub fn dest_numid(&self, d: Dest) -> Option<u32> {
        match d {
            Dest::Output(i) => self.outputs.get(i).map(|o| o.numid),
            Dest::MixerInput(n) => self.mixer_inputs.get(n.checked_sub(1)?).copied(),
            Dest::Capture(n) => self.capture_ids.get(n.checked_sub(1)?).copied(),
        }
    }

    /// A destination's name for people: `LINE 1`, `MIXER 3`, `CAPTURE 2`.
    pub fn dest_name(&self, d: Dest) -> String {
        match d {
            Dest::Output(i) => self.outputs.get(i).map(|o| o.name.clone()).unwrap_or_default(),
            Dest::MixerInput(n) => format!("MIXER {n}"),
            Dest::Capture(n) => format!("CAPTURE {n}"),
        }
    }

    /// The destination a name is, in any case.
    pub fn dest_named(&self, name: &str) -> Option<Dest> {
        let name = name.trim();
        self.dests().into_iter().find(|&d| self.dest_name(d).eq_ignore_ascii_case(name))
    }

    /// Where a destination's level is among the meters, if the interface has them.
    pub fn meter_of(&self, d: Dest) -> Option<usize> {
        self.meter?;
        let numid = self.dest_numid(d)?;
        self.meter_dests.iter().position(|&m| m == numid)
    }

    /// The meters now, raw ([`crate::Meter`]); empty without them.
    pub fn read_meters(&self, c: &mut dyn Controls) -> io::Result<Vec<i64>> {
        match self.meter {
            Some(m) => c.read(m.numid),
            None => Ok(Vec::new()),
        }
    }

    /// The dB a raw gain is; None: off, the bottom.
    pub fn gain_db(&self, raw: i64) -> Option<f64> {
        let (min, max) = self.gain_raw;
        if raw <= min || max <= min {
            return None;
        }
        let (lo, hi) = (self.gain_db.0 as f64 / 100.0, self.gain_db.1 as f64 / 100.0);
        Some(lo + (raw - min) as f64 / (max - min) as f64 * (hi - lo))
    }

    /// Whether a source is one of the mixer's own mixes, which can't feed the mixer.
    pub fn is_mix(&self, source: usize) -> bool {
        self.sources.get(source).is_some_and(|s| s.starts_with("Mix "))
    }

    /// `Off`'s item.
    pub fn off(&self) -> usize {
        self.source("Off").unwrap_or(0)
    }
}

/// What every destination plays now, in [`Focusrite::dests`]' order.
pub fn routes(c: &mut dyn Controls, f: &Focusrite) -> io::Result<Vec<usize>> {
    let mut out = Vec::new();
    for d in f.dests() {
        let numid = f.dest_numid(d).expect("a destination of its own");
        out.push(c.read(numid)?.first().copied().unwrap_or(0).max(0) as usize);
    }
    Ok(out)
}

/// Why the mixer can't do what's asked.
#[derive(Debug)]
pub enum MixError {
    /// Every one of the mixer's inputs is in use, by its count.
    Full(usize),
    /// A mix into the mixer.
    MixIntoMixer,
    /// `Off`, or no such source.
    NoSource,
    NoMix,
    Io(io::Error),
}

impl fmt::Display for MixError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MixError::Full(n) => write!(f, "the mixer's {n} inputs are all used"),
            MixError::MixIntoMixer => write!(f, "a mix can't feed the mixer"),
            MixError::NoSource => write!(f, "no such source"),
            MixError::NoMix => write!(f, "no such mix"),
            MixError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for MixError {}

impl From<io::Error> for MixError {
    fn from(e: io::Error) -> MixError {
        MixError::Io(e)
    }
}

/// Sets a destination to play a source. A mix into a mixer input is refused.
pub fn route(c: &mut dyn Controls, f: &Focusrite, d: Dest, source: usize) -> Result<(), MixError> {
    if source >= f.sources.len() {
        return Err(MixError::NoSource);
    }
    if matches!(d, Dest::MixerInput(_)) && f.is_mix(source) {
        return Err(MixError::MixIntoMixer);
    }
    let numid = f.dest_numid(d).ok_or(MixError::NoSource)?;
    c.write(numid, &[source as i64])?;
    Ok(())
}

/// The mixer as it's been set: what each input hears, and each mono mix's gains, raw, as
/// written. `gains[mix][input]`, inputs from 0.
#[derive(Clone, Debug, PartialEq)]
pub struct Mixing {
    inputs: Vec<usize>,
    gains: Vec<Vec<i64>>,
}

impl Mixing {
    /// The mixer as the driver has it: once, at the start (the module's header).
    pub fn read(c: &mut dyn Controls, f: &Focusrite) -> io::Result<Mixing> {
        let mut inputs = Vec::new();
        for &numid in &f.mixer_inputs {
            inputs.push(c.read(numid)?.first().copied().unwrap_or(0).max(0) as usize);
        }
        let mut gains = Vec::new();
        for mix in &f.gains {
            let mut row = Vec::new();
            for &numid in mix {
                row.push(c.read(numid)?.first().copied().unwrap_or(f.gain_raw.0));
            }
            gains.push(row);
        }
        Ok(Mixing { inputs, gains })
    }

    /// The mixer as kept (by a server, between runs).
    pub fn from_parts(inputs: Vec<usize>, gains: Vec<Vec<i64>>) -> Mixing {
        Mixing { inputs, gains }
    }

    /// What each input hears.
    pub fn inputs(&self) -> &[usize] {
        &self.inputs
    }

    /// Each mix's gains, raw.
    pub fn gains(&self) -> &[Vec<i64>] {
        &self.gains
    }

    fn off(&self, f: &Focusrite, raw: i64) -> bool {
        raw <= f.gain_raw.0
    }

    /// Whether no mix hears an input.
    pub fn unused(&self, f: &Focusrite, input: usize) -> bool {
        self.gains.iter().all(|m| m.get(input).is_none_or(|&g| self.off(f, g)))
    }

    /// The sources a mix hears: each with its input (from 0) and its gain, raw, in the
    /// inputs' order.
    pub fn heard(&self, f: &Focusrite, mix: usize) -> Vec<(usize, usize, i64)> {
        let Some(gains) = self.gains.get(mix) else { return Vec::new() };
        self.inputs
            .iter()
            .enumerate()
            .filter(|&(i, &s)| s != f.off() && !self.off(f, gains[i]))
            .map(|(i, &s)| (s, i, gains[i]))
            .collect()
    }

    /// A source's gain in a mix, raw, if the mix hears it.
    pub fn gain(&self, f: &Focusrite, mix: usize, source: usize) -> Option<i64> {
        self.heard(f, mix).into_iter().find(|h| h.0 == source).map(|h| h.2)
    }

    /// The input a source would take to go into a mix, and whether its enum changes: the one
    /// that hears it already, else one at `Off`, else one no mix hears.
    pub fn place(&self, f: &Focusrite, source: usize) -> Result<(usize, bool), MixError> {
        if source == f.off() || source >= f.sources.len() {
            return Err(MixError::NoSource);
        }
        if f.is_mix(source) {
            return Err(MixError::MixIntoMixer);
        }
        if let Some(i) = self.inputs.iter().position(|&s| s == source) {
            return Ok((i, false));
        }
        if let Some(i) = self.inputs.iter().position(|&s| s == f.off()) {
            return Ok((i, true));
        }
        (0..self.inputs.len())
            .find(|&i| self.unused(f, i))
            .map(|i| (i, true))
            .ok_or(MixError::Full(self.inputs.len()))
    }

    /// Writes one gain, if it's changed; returns whether it was written.
    pub fn set_gain(
        &mut self,
        c: &mut dyn Controls,
        f: &Focusrite,
        mix: usize,
        input: usize,
        raw: i64,
    ) -> io::Result<bool> {
        let raw = raw.clamp(f.gain_raw.0, f.gain_raw.1);
        let numid = *f
            .gains
            .get(mix)
            .and_then(|m| m.get(input))
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no such gain"))?;
        if self.gains[mix][input] == raw {
            return Ok(false);
        }
        c.write(numid, &[raw])?;
        self.gains[mix][input] = raw;
        Ok(true)
    }

    fn set_input(
        &mut self,
        c: &mut dyn Controls,
        f: &Focusrite,
        input: usize,
        source: usize,
    ) -> io::Result<()> {
        if self.inputs[input] != source {
            c.write(f.mixer_inputs[input], &[source as i64])?;
            self.inputs[input] = source;
        }
        Ok(())
    }

    /// A mix hears a source too, at raw; returns the input it's on.
    pub fn add(
        &mut self,
        c: &mut dyn Controls,
        f: &Focusrite,
        source: usize,
        mix: usize,
        raw: i64,
    ) -> Result<usize, MixError> {
        if mix >= self.gains.len() {
            return Err(MixError::NoMix);
        }
        let (input, _) = self.place(f, source)?;
        self.set_input(c, f, input, source)?;
        self.set_gain(c, f, mix, input, raw)?;
        Ok(input)
    }

    /// A mix no longer hears a source; its input set `Off` when no mix hears it. Returns
    /// whether it was heard.
    pub fn cut(
        &mut self,
        c: &mut dyn Controls,
        f: &Focusrite,
        source: usize,
        mix: usize,
    ) -> Result<bool, MixError> {
        if mix >= self.gains.len() {
            return Err(MixError::NoMix);
        }
        let mut was = false;
        for i in 0..self.inputs.len() {
            if self.inputs[i] != source || source == f.off() {
                continue;
            }
            was |= !self.off(f, self.gains[mix][i]);
            self.set_gain(c, f, mix, i, f.gain_raw.0)?;
            if self.unused(f, i) {
                self.set_input(c, f, i, f.off())?;
            }
        }
        Ok(was)
    }

    /// A mix hears a source alone, at raw: every other source cut from it. Returns those cut.
    pub fn only(
        &mut self,
        c: &mut dyn Controls,
        f: &Focusrite,
        source: usize,
        mix: usize,
        raw: i64,
    ) -> Result<Vec<usize>, MixError> {
        if mix >= self.gains.len() {
            return Err(MixError::NoMix);
        }
        self.place(f, source)?;
        let others: Vec<usize> =
            self.heard(f, mix).into_iter().map(|h| h.0).filter(|&s| s != source).collect();
        for &s in &others {
            self.cut(c, f, s, mix)?;
        }
        self.add(c, f, source, mix, raw)?;
        Ok(others)
    }

    /// Writes every input's source and every gain again, whole: after a Focusrite panel has
    /// scrambled the driver's copy. Returns how many controls were written.
    pub fn write_all(&self, c: &mut dyn Controls, f: &Focusrite) -> io::Result<usize> {
        let mut n = 0;
        for (i, &s) in self.inputs.iter().enumerate() {
            c.write(f.mixer_inputs[i], &[s as i64])?;
            n += 1;
        }
        for (m, row) in self.gains.iter().enumerate() {
            for (i, &g) in row.iter().enumerate() {
                c.write(f.gains[m][i], &[g])?;
                n += 1;
            }
        }
        Ok(n)
    }
}
