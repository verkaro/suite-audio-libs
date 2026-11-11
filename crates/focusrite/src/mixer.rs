//! Setting a Focusrite for a [`Plan`], and keeping its gains with the deck (`docs/monitor.md`):
//! a track heard live is heard at its level in each mix, from the interface's mixer; a track
//! on tape is off there, and heard in the tape returns.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::controls::{Controls, format_line};
use crate::interface::Focusrite;
use crate::panel::{TALK_DISABLED, TALK_OFF, TALK_ON};
use crate::plan::Plan;

/// What the deck says about what's heard.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Levels {
    /// Each engine mix's gains for each track, left and right, linear, as the tape's are (with
    /// the mixes' headroom).
    pub gains: Vec<Vec<(f32, f32)>>,
    /// Each track heard from its input.
    pub live: Vec<bool>,
    pub muted: Vec<bool>,
    pub soloed: Vec<bool>,
    /// MIX LEVEL, dB.
    pub mix_db: f64,
    /// MONO: the engine mix (the control room's) summed to mono, in every mix taking its
    /// levels (MAIN following it too); none, off.
    pub mono: Option<usize>,
    /// Each engine mix's gains for each of the plan's other inputs, left and right, linear; and
    /// those muted.
    pub inputs: Vec<Vec<(f32, f32)>>,
    pub inputs_muted: Vec<bool>,
    /// The line outputs whose volume Dexter sets (`docs/monitor-page.md`), by number, each its
    /// mix's output level in dB (−80 and below: off).
    pub outs: BTreeMap<usize, f64>,
    /// The engine mixes whose talkback is changed since the plan (`docs/screen.md`,
    /// "Talkback"), by number: heard in it, or not. Those left out are as planned.
    pub talkback: BTreeMap<usize, bool>,
    /// TALK is on: the mixes hearing the talkback have their switches on. On the 18i20 a
    /// mix's switch alone sends the mic, whatever the talkback's enum (found 2026-10-21), so
    /// they're off while it isn't.
    pub talking: bool,
    /// TALK's level (dB): the talkback's input in the mixes hearing it while TALK is on. The
    /// interface's own talkback lets that input in at its gain, so with none it's silent (found
    /// at the rig, 2026-10-21).
    pub talk_db: f64,
}

impl Levels {
    /// Every track off and on tape, in mixes engine mixes of tracks tracks, and inputs other
    /// inputs off.
    pub fn new(mixes: usize, tracks: usize, inputs: usize) -> Levels {
        Levels {
            gains: vec![vec![(0.0, 0.0); tracks]; mixes],
            live: vec![false; tracks],
            muted: vec![false; tracks],
            soloed: vec![false; tracks],
            mix_db: 0.0,
            mono: None,
            inputs: vec![vec![(0.0, 0.0); inputs]; mixes],
            inputs_muted: vec![false; inputs],
            outs: BTreeMap::new(),
            talkback: BTreeMap::new(),
            talking: false,
            talk_db: 0.0,
        }
    }
}

fn db(g: f32) -> Option<f64> {
    (g > 0.0).then(|| 20.0 * (g as f64).log10())
}

/// Each mono mix the plan uses, with its gain on every mixer input (raw), for levels: the tape
/// return at 0 dB; each track heard live (the first on its input), unmuted, and soloed where SOLO
/// is heard, at its gain and MIX LEVEL; each other input unmuted (and not where SOLO is heard
/// while something is soloed) at its gain and MIX LEVEL; everything else off. A print's input
/// is never heard in a mix that reaches a print's output (`docs/monitor.md`, "Prints"). With MONO, the
/// control room's mix and those following it have each input at the mean of its two gains on
/// both sides; its tape return is mono already (the callback made it so). While TALK is on, the
/// talkback's input at TALK's level in the mixes hearing it, and everything else [`TALK_DIM_DB`]
/// down there and in the control room's mix, and in those following them.
pub fn targets(f: &Focusrite, p: &Plan, lv: &Levels) -> Vec<(usize, Vec<i64>)> {
    let off = f.gain(None);
    let soloing = lv.soloed.iter().any(|&s| s);
    // The engine mixes hearing the talkback, and the control room's: while TALK is on, the mic
    // in the first, and everything but the mic dimmed in both (and those following them).
    let heard = |d: usize| {
        lv.talkback
            .get(&d)
            .copied()
            .unwrap_or(p.mixes.iter().any(|m| m.deck == d && !m.follows && m.talkback))
    };
    let control = p.mixes.iter().find(|m| m.solo).map(|m| m.deck);
    let mut out = Vec::new();
    for pm in &p.mixes {
        let dim = match lv.talking && (control == Some(pm.deck) || heard(pm.deck)) {
            true => TALK_DIM_DB,
            false => 0.0,
        };
        let mono = lv.mono == Some(pm.deck);
        let pick = |g: &(f32, f32), side: usize| match (mono, side) {
            (true, _) => (g.0 + g.1) / 2.0,
            (false, 0) => g.0,
            (false, _) => g.1,
        };
        for (side, &m) in pm.pair.iter().enumerate() {
            let mut row: Vec<Option<i64>> = vec![None; f.mixer_inputs.len()];
            if let Some(Some(r)) = p.returns.get(pm.deck) {
                row[r.inputs[side]] = Some(f.gain(Some(dim)));
            }
            for (i, input) in p.tracks.iter().enumerate() {
                let Some(input) = *input else { continue };
                let heard = lv.live.get(i) == Some(&true)
                    && lv.muted.get(i) != Some(&true)
                    && !(pm.solo && soloing && lv.soloed.get(i) != Some(&true));
                if !heard || row[input].is_some() || (pm.prints && p.is_print_input(input)) {
                    continue;
                }
                let g = lv.gains.get(pm.deck).and_then(|g| g.get(i)).map_or(0.0, |g| pick(g, side));
                row[input] = Some(f.gain(db(g).map(|d| d + lv.mix_db + dim)));
            }
            for (k, (input, _)) in p.others.iter().enumerate() {
                if lv.inputs_muted.get(k) == Some(&true)
                    || (pm.solo && soloing)
                    || row[*input].is_some()
                    || (pm.prints && p.is_print_input(*input))
                {
                    continue;
                }
                let g = lv.inputs.get(pm.deck).and_then(|g| g.get(k)).map_or(0.0, |g| pick(g, side));
                row[*input] = Some(f.gain(db(g).map(|d| d + lv.mix_db + dim)));
            }
            // The talkback's input: TALK's level in a mix hearing it while TALK is on; else off.
            if let Some(t) = p.talkback_input
                && lv.talking
                && !pm.follows
                && heard(pm.deck)
            {
                row[t] = Some(f.gain(Some(lv.talk_db)));
            }
            out.push((m, row.into_iter().map(|g| g.unwrap_or(off)).collect()));
        }
    }
    out
}

/// What TALK dims the rest of the control room's mix and of the mixes hearing it by (Warren,
/// 2026-10-21), in the mixer's gains: the box's own DIM is on the outputs, so it would dim the
/// mic too. Like any gain on the 18i20 it glides there, as the mic comes up: nothing goes
/// through Dexter, so nothing is a round trip late (Warren: latency before ramping speed).
pub const TALK_DIM_DB: f64 = -18.0;

/// A control's value, written only when it isn't that already. Never for the mixer's gains:
/// on the 18i20 (kernel 6.12, found 2026-10-13) reading them can have the driver reload its copy
/// of the mixer in a shifted layout, and a write then sends a whole mix from that copy.
fn set(c: &mut dyn Controls, numid: u32, v: i64) -> io::Result<bool> {
    if c.read(numid).ok().and_then(|x| x.first().copied()) == Some(v) {
        return Ok(false);
    }
    c.write(numid, &[v])?;
    Ok(true)
}

/// A control's value set to to, only when it's from now.
fn set_from(c: &mut dyn Controls, numid: u32, from: i64, to: i64) -> io::Result<bool> {
    if c.read(numid)?.first() != Some(&from) {
        return Ok(false);
    }
    c.write(numid, &[to])?;
    Ok(true)
}

/// The interface, set for a plan.
pub struct Mixer {
    c: Box<dyn Controls>,
    f: Focusrite,
    plan: Plan,
    /// Each planned mono mix's gains as last written.
    written: BTreeMap<usize, Vec<i64>>,
    /// Each line output's volume as last written, raw.
    written_outs: BTreeMap<usize, i64>,
    /// Each planned mono mix's talkback switch as last written.
    written_talk: BTreeMap<usize, bool>,
}

/// Writes the rig's loop alone, as plan has it (`docs/loopback.md`): ahead of the check, so it
/// measures on the path it's for, before the rest of the plan is written. Nothing without one.
pub fn write_loop(c: &mut dyn Controls, f: &Focusrite, plan: &Plan) -> io::Result<()> {
    let Some(l) = &plan.loopback else { return Ok(()) };
    if let Some(s) = plan.settings.iter().find(|s| s.what.ends_with("(the loop)")) {
        set(c, s.numid, s.value)?;
    }
    if let Some(o) = f.output(&l.out)
        && let Some(&(numid, src)) = plan.outputs.iter().find(|x| x.0 == o.numid)
    {
        set(c, numid, src as i64)?;
    }
    let line = l.out.strip_prefix("LINE ").and_then(|n| n.parse::<usize>().ok());
    if let Some(line) = line.and_then(|n| f.lines.iter().find(|x| x.n == n)) {
        if let Some(vc) = line.control {
            set(c, vc, 0)?;
        }
        if let Some(v) = line.volume {
            set(c, v.numid, v.max)?;
        }
        if let Some(m) = line.mute {
            set(c, m, 0)?;
        }
    }
    Ok(())
}

impl Mixer {
    /// Sets the interface for plan: the rig's settings, the gains of the mixes it uses off, then speaker
    /// switching, the line outputs taken off the knob, the mixer's inputs and the outputs; then
    /// the gains for levels.
    pub fn setup(mut c: Box<dyn Controls>, f: Focusrite, plan: Plan, lv: &Levels) -> io::Result<Mixer> {
        // The rig's settings first (`docs/interface.md`): the clock, what's recorded, the
        // inputs' switches and 48V.
        for s in &plan.settings {
            set(c.as_mut(), s.numid, s.value)?;
        }
        // Written without reading them first (see `set`).
        let off = f.gain(None);
        for pm in &plan.mixes {
            for &m in &pm.pair {
                for &g in &f.gains[m] {
                    c.write(g, &[off])?;
                }
            }
        }
        if let (Some(on), Some(numid)) = (plan.speaker_switching, f.speaker_switching) {
            let now = c.read(numid)?.first().copied().unwrap_or(0);
            if on != (now != 0) {
                c.write(numid, &[if on { 1 } else { 0 }])?;
            }
        }
        for &n in &plan.sw_lines {
            let Some(line) = f.lines.iter().find(|l| l.n == n) else { continue };
            if let Some(vc) = line.control {
                set(c.as_mut(), vc, 0)?;
            }
            if let Some(v) = line.volume {
                let raw = lv.outs.get(&n).map_or(v.max, |&db| v.raw(db));
                set(c.as_mut(), v.numid, raw)?;
            }
            if let Some(m) = line.mute {
                set(c.as_mut(), m, 0)?;
            }
        }
        for (&numid, &s) in f.mixer_inputs.iter().zip(&plan.mixer_inputs) {
            set(c.as_mut(), numid, s as i64)?;
        }
        for &(numid, s) in &plan.outputs {
            set(c.as_mut(), numid, s as i64)?;
        }
        // The talkback: heard in the mixes that ask for it, and not in the others Dexter uses
        // ([`Mixer::apply`]); not on (as an engine that stopped while TALK was could leave it).
        if let Some(tb) = f.talkback
            && c.read(tb)?.first() == Some(&TALK_ON)
        {
            c.write(tb, &[TALK_OFF])?;
        }
        let written =
            plan.mixes.iter().flat_map(|pm| pm.pair).map(|m| (m, vec![off; f.mixer_inputs.len()])).collect();
        let mut mixer =
            Mixer { c, f, plan, written, written_outs: BTreeMap::new(), written_talk: BTreeMap::new() };
        mixer.apply(lv)?;
        Ok(mixer)
    }

    /// Writes the gains, the line outputs' volumes and the talkback's switches that levels
    /// change; returns how many.
    pub fn apply(&mut self, lv: &Levels) -> io::Result<usize> {
        let mut n = 0;
        // The talkback's switches (read first, as a volume is): on in the mixes hearing it while
        // TALK is on; and the talkback switched on (`Off`: the TALK button works) when any mix
        // hears it.
        let mut heard = false;
        for pm in &self.plan.mixes {
            let chosen = !pm.follows && lv.talkback.get(&pm.deck).copied().unwrap_or(pm.talkback);
            heard |= chosen;
            let on = chosen && lv.talking;
            for &m in &pm.pair {
                let Some(&sw) = self.f.talkback_mixes.get(m) else { continue };
                if self.written_talk.get(&m) != Some(&on) {
                    set(self.c.as_mut(), sw, on as i64)?;
                    self.written_talk.insert(m, on);
                    n += 1;
                }
            }
        }
        if let Some(tb) = self.f.talkback
            && heard
            && set_from(self.c.as_mut(), tb, TALK_DISABLED, TALK_OFF)?
        {
            n += 1;
        }
        // The outputs' volumes: read first and written only when they differ (a volume, unlike
        // a gain, reads true).
        for (&line, &db) in &lv.outs {
            let Some(v) = self.f.lines.iter().find(|l| l.n == line).and_then(|l| l.volume) else { continue };
            let raw = v.raw(db);
            if self.written_outs.get(&line) != Some(&raw) {
                set(self.c.as_mut(), v.numid, raw)?;
                self.written_outs.insert(line, raw);
                n += 1;
            }
        }
        for (m, row) in targets(&self.f, &self.plan, lv) {
            let was = self.written.entry(m).or_default();
            was.resize(row.len(), i64::MIN);
            for (k, &g) in row.iter().enumerate() {
                if was[k] != g {
                    self.c.write(self.f.gains[m][k], &[g])?;
                    was[k] = g;
                    n += 1;
                }
            }
        }
        Ok(n)
    }

    /// Every talkback switch of the mixes the plan uses off: the mic heard nowhere (an engine
    /// ending while TALK is on).
    pub fn talkback_off(&mut self) -> io::Result<()> {
        for pm in &self.plan.mixes {
            for &m in &pm.pair {
                if let Some(&sw) = self.f.talkback_mixes.get(m) {
                    set(self.c.as_mut(), sw, 0)?;
                    self.written_talk.insert(m, false);
                }
            }
        }
        Ok(())
    }

    /// Takes a new plan for the same mixes (the tracks' inputs patched again,
    /// `docs/repatch.md`): each mixer input whose source changes goes off in every mix, then
    /// takes its new source; [`Mixer::apply`] brings the gains up after.
    pub fn replan(&mut self, plan: Plan) -> io::Result<()> {
        let off = self.f.gain(None);
        for (k, (&now, &want)) in self.plan.mixer_inputs.iter().zip(&plan.mixer_inputs).enumerate() {
            if now == want {
                continue;
            }
            for (&m, row) in self.written.iter_mut() {
                if let Some(g) = row.get_mut(k) {
                    self.c.write(self.f.gains[m][k], &[off])?;
                    *g = off;
                }
            }
            if let Some(&numid) = self.f.mixer_inputs.get(k) {
                set(self.c.as_mut(), numid, want as i64)?;
            }
        }
        self.plan = plan;
        Ok(())
    }

    /// Writes every gain of every mix the plan uses again, whole: each cell, then the first moved
    /// half a dB and back, so the driver sends each mix from a copy holding exactly what was
    /// written (after a Focusrite panel has scrambled it). Returns how many mixes.
    pub fn rewrite_all(&mut self) -> io::Result<usize> {
        for (&m, row) in &self.written {
            for (k, &g) in row.iter().enumerate().filter(|(_, g)| **g != i64::MIN) {
                self.c.write(self.f.gains[m][k], &[g])?;
            }
            if let Some(&g) = row.first().filter(|g| **g != i64::MIN) {
                let numid = self.f.gains[m][0];
                self.c.write(numid, &[if g > 0 { g - 1 } else { g + 1 }])?;
                self.c.write(numid, &[g])?;
            }
        }
        Ok(self.written.len())
    }

    pub fn plan(&self) -> &Plan {
        &self.plan
    }

    pub fn interface(&self) -> &Focusrite {
        &self.f
    }

    /// The card, for the tests.
    pub fn controls(&mut self) -> &mut dyn Controls {
        self.c.as_mut()
    }
}

/// Keeps every control the interface lets be written, with its value, at path (in the
/// controls' text form): once, the first time Dexter writes to it, so it can be put back.
/// Returns whether it was written now.
pub fn keep_found(c: &mut dyn Controls, path: &Path) -> io::Result<bool> {
    if path.exists() {
        return Ok(false);
    }
    let mut text =
        String::from("# The interface's controls as Dexter found them, before it first wrote to it.\n");
    for k in c.list()?.iter().filter(|k| k.writable && k.readable) {
        let v = c.read(k.numid)?;
        text += &format_line(k, &v);
        text.push('\n');
    }
    if let Some(d) = path.parent() {
        fs::create_dir_all(d)?;
    }
    let mut f = match fs::OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => return Ok(false),
        Err(e) => return Err(e),
    };
    f.write_all(text.as_bytes())?;
    f.sync_all()?;
    Ok(true)
}

/// Puts back the controls kept by [`keep_found`] (those still there and writable); returns
/// how many were written.
pub fn put_back(c: &mut dyn Controls, kept: &str) -> io::Result<usize> {
    let was = crate::Fake::parse(kept).map_err(io::Error::other)?;
    let mut n = 0;
    // Speaker switching first: it decides which line outputs can be written.
    let mut list = c.list()?;
    list.sort_by_key(|k| k.name != "Speaker Switching Playback Enum");
    for k in list {
        let Some(v) = was.get(&k.name) else { continue };
        if c.read(k.numid).ok().as_deref() == Some(v) {
            continue;
        }
        if c.write(k.numid, v).is_ok() {
            n += 1;
        }
    }
    Ok(n)
}

struct Shared {
    levels: Levels,
    /// Changes asked for, and written.
    asked: u64,
    done: u64,
    stop: bool,
    /// The Focusrite panel running beside the mixer, if one is ([`Watch`]), and how many times
    /// that has changed.
    panel: Option<String>,
    panel_moves: u64,
    /// A new plan for the mixer, taken before the next levels are written.
    replan: Option<Plan>,
}

/// A program watched beside the mixer: a Focusrite panel (alsa-scarlett-gui), which scrambles
/// the driver's copy of the mixer while it runs, so that every gain written sends a whole mix
/// wrong (`dexter_system::mixer_panels`). While one runs it's warned of; when it goes, every mix
/// is written again whole.
pub struct Watch {
    /// How often it's looked for.
    pub every: Duration,
    /// The panel running now, by name, if one is.
    pub check: Box<dyn FnMut() -> Option<String> + Send>,
    /// Says what's good to know (the panel gone, the mixes written again).
    pub say: Box<dyn Fn(String) + Send>,
}

/// The mixer kept with the deck on a thread of its own: the deck's calls change the levels,
/// and the thread writes what they change, never on JACK's thread.
pub struct Link {
    shared: Arc<(Mutex<Shared>, Condvar)>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Link {
    /// Starts keeping m's gains with levels from lv; warn says what went wrong (once, until it
    /// goes right again).
    pub fn start(m: Mixer, lv: Levels, warn: Box<dyn Fn(String) + Send>) -> Link {
        Link::watching(m, lv, warn, None)
    }

    /// As [`Link::start`], with a Focusrite panel watched for.
    pub fn watching(
        mut m: Mixer,
        lv: Levels,
        warn: Box<dyn Fn(String) + Send>,
        mut watch: Option<Watch>,
    ) -> Link {
        let shared = Arc::new((
            Mutex::new(Shared {
                levels: lv,
                asked: 1,
                done: 0,
                stop: false,
                panel: None,
                panel_moves: 0,
                replan: None,
            }),
            Condvar::new(),
        ));
        let s = shared.clone();
        let thread = thread::Builder::new()
            .name("focusrite".into())
            .spawn(move || {
                let (lock, cv) = &*s;
                let mut failing = false;
                let mut next_look = Instant::now();
                loop {
                    // The panel, looked for when it's time.
                    if let Some(w) = watch.as_mut()
                        && Instant::now() >= next_look
                    {
                        next_look = Instant::now() + w.every;
                        let now = (w.check)();
                        let was = {
                            let mut g = lock.lock().unwrap_or_else(|e| e.into_inner());
                            if g.panel == now {
                                None
                            } else {
                                g.panel_moves += 1;
                                Some(std::mem::replace(&mut g.panel, now.clone()))
                            }
                        };
                        match (was, now) {
                            (Some(_), Some(p)) => warn(format!(
                                "{p} is open: while it runs, the interface's mixes can reach it wrong; close it"
                            )),
                            (Some(Some(p)), None) => match m.rewrite_all() {
                                Ok(_) => (w.say)(format!("{p} closed: the interface's mixes written again")),
                                Err(e) => warn(format!("{p} closed, but the mixes couldn't be written again: {e}")),
                            },
                            _ => {}
                        }
                    }
                    let (lv, asked, replan) = {
                        let mut g = lock.lock().unwrap_or_else(|e| e.into_inner());
                        while g.done == g.asked && !g.stop {
                            match watch {
                                None => g = cv.wait(g).unwrap_or_else(|e| e.into_inner()),
                                Some(_) => {
                                    let left = next_look.saturating_duration_since(Instant::now());
                                    if left.is_zero() {
                                        break;
                                    }
                                    g = cv.wait_timeout(g, left).unwrap_or_else(|e| e.into_inner()).0;
                                }
                            }
                        }
                        if g.stop {
                            drop(g);
                            if let Err(e) = m.talkback_off() {
                                warn(format!("the talkback couldn't be switched off in every mix: {e}"));
                            }
                            return;
                        }
                        if g.done == g.asked {
                            continue; // time to look for the panel
                        }
                        (g.levels.clone(), g.asked, g.replan.take())
                    };
                    if let Some(plan) = replan
                        && let Err(e) = m.replan(plan)
                    {
                        warn(format!("the interface's mixer couldn't take the tracks' new inputs: {e}"));
                    }
                    match m.apply(&lv) {
                        Ok(_) => failing = false,
                        Err(e) if !failing => {
                            failing = true;
                            warn(format!("the interface's mixer isn't following the deck: {e}"));
                        }
                        Err(_) => {}
                    }
                    // Never read back to check: the gains read on the 18i20 aren't what it has (see
                    // `set`), and writing again what read otherwise scrambled the mixes it fixed
                    // (measured, crates/focusrite/tests/landing.rs).
                    lock.lock().unwrap_or_else(|e| e.into_inner()).done = asked;
                    cv.notify_all();
                }
            })
            .ok();
        Link { shared, thread }
    }

    /// The Focusrite panel running beside the mixer, if one is, and how many times that has
    /// changed.
    pub fn panel(&self) -> (Option<String>, u64) {
        let g = self.shared.0.lock().unwrap_or_else(|e| e.into_inner());
        (g.panel.clone(), g.panel_moves)
    }

    /// A new plan for the mixer (the tracks' inputs patched again), with the levels as f leaves
    /// them for it; the thread writes what changed.
    pub fn replan(&self, plan: Plan, f: impl FnOnce(&mut Levels)) {
        let (lock, cv) = &*self.shared;
        let mut g = lock.lock().unwrap_or_else(|e| e.into_inner());
        g.replan = Some(plan);
        f(&mut g.levels);
        g.asked += 1;
        cv.notify_all();
    }

    /// Changes the levels; the thread writes what changed.
    pub fn update(&self, f: impl FnOnce(&mut Levels)) {
        let (lock, cv) = &*self.shared;
        let mut g = lock.lock().unwrap_or_else(|e| e.into_inner());
        let before = g.levels.clone();
        f(&mut g.levels);
        if g.levels != before {
            g.asked += 1;
            cv.notify_all();
        }
    }

    /// The levels now.
    pub fn levels(&self) -> Levels {
        self.shared.0.lock().unwrap_or_else(|e| e.into_inner()).levels.clone()
    }

    /// Waits (up to 5 seconds) until everything changed so far is written.
    pub fn settle(&self) {
        let (lock, cv) = &*self.shared;
        let g = lock.lock().unwrap_or_else(|e| e.into_inner());
        let wait = std::time::Duration::from_secs(5);
        let _ = cv.wait_timeout_while(g, wait, |g| g.done < g.asked && !g.stop && self.thread.is_some());
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        {
            let (lock, cv) = &*self.shared;
            lock.lock().unwrap_or_else(|e| e.into_inner()).stop = true;
            cv.notify_all();
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
