//! What Dexter sets on a Focusrite for a project (`docs/monitor.md`, "On a Focusrite
//! interface"): a pair of the interface's mixes for each of its mixes, their outputs, a tape
//! return for each of the engine's mixes, and the mixer's inputs shared out by need.

use crate::interface::{Focusrite, shown};

/// A mix as the project has it.
#[derive(Clone, Debug, PartialEq)]
pub struct Want {
    pub name: String,
    /// The interface's outputs it reaches, left, right, and another pair if it goes two places.
    pub hw: Vec<String>,
    /// The engine's mix whose levels and tape return it takes: its own, or the one it follows.
    pub deck: usize,
    /// It follows another, so SOLO and the talkback aren't heard in it.
    pub follows: bool,
    /// The control room's, which SOLO is heard in.
    pub control: bool,
    /// The talkback is heard in it while TALK is held (never in a mix that follows another).
    pub talkback: bool,
}

/// What a rig sets on the interface at the engine's start (`docs/interface.md`); each left out
/// is as the interface has it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Settings {
    /// `internal`, `adat` or `spdif`.
    pub clock: Option<String>,
    /// What capture channels (from 1) record, by source name.
    pub captures: Vec<(usize, String)>,
    /// What the mixer's inputs (from 1) hear, by source name; empty: shared out by need.
    pub mixer_inputs: Vec<(usize, String)>,
    /// What outputs no mix names play, by the interface's names: `("ADAT 1", "PCM 11")`.
    pub outputs: Vec<(String, String)>,
    /// Each input's switches, by its number.
    pub inputs: Vec<(usize, Switches)>,
    /// 48V by the interface's groups (`1-4`).
    pub phantom: Vec<(String, bool)>,
    pub phantom_persists: Option<bool>,
    /// The loop that stays plugged in: its output and input by the interface's names
    /// (`S/PDIF 1`, `S/PDIF 1`; `docs/loopback.md`).
    pub loopback: Option<(String, String)>,
    /// The prints: each output cabled to an input, by the interface's names (`LINE 1`,
    /// `Analogue 7`; `docs/monitor.md`, "Prints").
    pub prints: Vec<(String, String)>,
}

/// An input's switches; each left out is as the interface has it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Switches {
    pub inst: Option<bool>,
    pub pad: Option<bool>,
    pub air: Option<bool>,
}

/// A control the rig sets: its id and value, and what it is, for the log.
#[derive(Clone, Debug, PartialEq)]
pub struct Setting {
    pub numid: u32,
    pub value: i64,
    pub what: String,
}

/// What a project asks of the interface.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Request {
    pub mixes: Vec<Want>,
    /// The engine's mixes (those that follow none).
    pub deck_mixes: usize,
    /// Each track's capture channel (from 1), when its input is one of the interface's.
    pub tracks: Vec<Option<usize>>,
    /// Each track's direct out's playback channel (from 1), when its `out:` is one of the
    /// interface's.
    pub direct_outs: Vec<Option<usize>>,
    pub speaker_switching: Option<bool>,
    /// The rig's settings.
    pub settings: Settings,
}

/// A mix on the interface: the pair of mono mixes it's made in.
#[derive(Clone, Debug, PartialEq)]
pub struct PlanMix {
    pub name: String,
    pub deck: usize,
    /// Its left and right mono mixes (0 is Mix A).
    pub pair: [usize; 2],
    /// SOLO is heard in it: the control room's own mix.
    pub solo: bool,
    /// It follows another, so it never hears the talkback.
    pub follows: bool,
    /// The talkback is heard in it.
    pub talkback: bool,
    /// The outputs it reaches.
    pub outputs: Vec<String>,
    /// It reaches a print's output, so no print's input is heard in it: the cable would bring
    /// it back into itself.
    pub prints: bool,
}

/// An engine mix's tape return: the playback channels it's sent on (from 1), and the mixer
/// inputs it comes in on (from 0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Return {
    pub playback: [usize; 2],
    pub inputs: [usize; 2],
}

/// A print as planned (`docs/monitor.md`, "Prints"): an output cabled to an input, and what
/// the output plays.
#[derive(Clone, Debug, PartialEq)]
pub struct Print {
    /// The output and the input, by the interface's names: `LINE 1`, `Analogue 7`.
    pub out: String,
    pub input: String,
    /// The input, as an item of the interface's sources.
    pub source: usize,
    /// What the output plays: one of the plan's mixes (by its place in `mixes`) and its side (0
    /// left, 1 right), or a playback channel (from 1), or neither (an input, off).
    pub mix: Option<(usize, usize)>,
    pub playback: Option<usize>,
}

/// The rig's loop as planned: on the last playback and capture channel (from 1) the interface
/// carries, its output fed from that playback channel alone and that capture channel recording
/// its input.
#[derive(Clone, Debug, PartialEq)]
pub struct Loop {
    pub out: String,
    pub input: String,
    pub playback: usize,
    pub capture: usize,
}

/// What's set on the interface.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    /// Each mixer input's source, as an item of the interface's sources (0: off).
    pub mixer_inputs: Vec<usize>,
    /// The outputs routed: their routing enums, and their sources.
    pub outputs: Vec<(u32, usize)>,
    pub mixes: Vec<PlanMix>,
    /// Each engine mix's tape return, if it's on the interface.
    pub returns: Vec<Option<Return>>,
    /// Each track's mixer input, when it's heard live.
    pub tracks: Vec<Option<usize>>,
    /// The other inputs in the mixer: their mixer input and source.
    pub others: Vec<(usize, String)>,
    /// The talkback's mixer input, the mic's (the last), on an interface with a talkback.
    pub talkback_input: Option<usize>,
    /// The line outputs taken off the knob (set to SW at full level): an aux mix's.
    pub sw_lines: Vec<usize>,
    /// The tracks whose direct outs aren't connected: their channels carry tape returns, or
    /// the loop.
    pub cut_outs: Vec<usize>,
    /// The rig's loop, kept out of everything else.
    pub loopback: Option<Loop>,
    /// The tracks whose inputs aren't connected: they'd record the loop.
    pub cut_ins: Vec<usize>,
    /// The prints, as the outputs are routed.
    pub prints: Vec<Print>,
    pub speaker_switching: Option<bool>,
    /// The rig's settings, in the order they're written: the clock, what's recorded, the
    /// inputs' switches, 48V.
    pub settings: Vec<Setting>,
    /// What each capture channel records once they're written (from 1), as an item of the
    /// interface's sources.
    pub captures: Vec<usize>,
    /// What didn't fit, or isn't there, said plainly.
    pub notes: Vec<String>,
}

impl Plan {
    /// What to set on f for r.
    pub fn new(f: &Focusrite, r: &Request) -> Plan {
        let mut p = Plan { speaker_switching: r.speaker_switching, ..Plan::default() };
        p.settings(f, &r.settings);
        p.plan_loop(f, &r.settings);
        let n_mixes = f.gains.len();

        // A pair of mono mixes for each mix that reaches an output.
        let mut next = 0;
        for w in r.mixes.iter().filter(|w| !w.hw.is_empty()) {
            if next + 1 >= n_mixes {
                p.notes.push(format!("{}: the interface has no mix left for it", w.name));
                continue;
            }
            p.mixes.push(PlanMix {
                name: w.name.clone(),
                deck: w.deck,
                pair: [next, next + 1],
                solo: w.control && !w.follows,
                follows: w.follows,
                talkback: w.talkback && !w.follows,
                outputs: Vec::new(),
                prints: false,
            });
            next += 2;
        }

        // Their outputs. An aux mix's line outputs come off the knob.
        let control = r.mixes.iter().find(|w| w.control).map(|w| w.deck);
        let mut taken: Vec<(String, String)> =
            p.loopback.iter().map(|l| (l.out.clone(), "the loop".to_string())).collect();
        for w in r.mixes.iter().filter(|w| !w.hw.is_empty()) {
            let Some(pm) = p.mixes.iter_mut().find(|m| m.name == w.name) else { continue };
            for (j, name) in w.hw.iter().enumerate() {
                let Some(o) = f.output(name) else {
                    p.notes.push(format!("{}: the interface has no output {name}", w.name));
                    continue;
                };
                if let Some((_, by)) = taken.iter().find(|(t, _)| *t == o.name) {
                    p.notes.push(format!("{} is {by}'s: {} doesn't reach it", o.name, w.name));
                    continue;
                }
                taken.push((o.name.clone(), w.name.clone()));
                let mix = Focusrite::mix_name(pm.pair[j % 2]);
                p.outputs.push((o.numid, f.source(&mix).unwrap_or(0)));
                pm.outputs.push(o.name.clone());
                let aux = Some(w.deck) != control;
                if let Some(n) = o.name.strip_prefix("LINE ").and_then(|n| n.parse::<usize>().ok())
                    && aux
                {
                    p.sw_lines.push(n);
                    if n <= 4 && r.speaker_switching != Some(false) && f.speaker_switching.is_some() {
                        p.notes.push(format!(
                            "{}: {} stays under the monitor knob and ALT while speaker switching is on",
                            w.name, o.name
                        ));
                    }
                }
            }
        }

        // What the rig has the other outputs play (`outputs:`): never one a mix or the loop has.
        for (out, src) in &r.settings.outputs {
            let Some(o) = f.output(out) else {
                p.notes.push(format!("the rig's output {out}: the interface has no output {out}"));
                continue;
            };
            if let Some((_, by)) = taken.iter().find(|(t, _)| *t == o.name) {
                p.notes.push(format!("the rig's output {}: it's {by}'s", o.name));
                continue;
            }
            let Some(s) = f.source_named(src) else {
                p.notes
                    .push(format!("the rig's output {}: {src} isn't one of the interface's sources", o.name));
                continue;
            };
            if p.loopback.as_ref().is_some_and(|l| f.playback_of(s) == Some(l.playback)) {
                p.notes.push(format!(
                    "the rig's output {}: {} is the loop's: its bursts aren't to be heard",
                    o.name, f.sources[s]
                ));
                continue;
            }
            taken.push((o.name.clone(), "the rig".to_string()));
            p.outputs.push((o.numid, s));
        }

        p.plan_prints(f, &r.settings);

        // The tape returns, on the lowest pairs of playback channels no output plays, as the
        // plan routes them or as found: those no direct out uses first, else those, whose direct
        // outs then aren't connected (a track would come back into the mixes at full level).
        let plays = |o: &crate::interface::Output| {
            p.outputs.iter().find(|x| x.0 == o.numid).map_or(o.source, |x| x.1)
        };
        let busy: Vec<usize> = (f.outputs.iter())
            .filter_map(|o| f.playback_of(plays(o)))
            .chain(p.loopback.iter().map(|l| l.playback))
            .collect();
        let used = |c: usize| r.direct_outs.contains(&Some(c));
        let pairs: Vec<usize> =
            (1..f.playbacks).step_by(2).filter(|c| !busy.contains(c) && !busy.contains(&(c + 1))).collect();
        let (clear, shared): (Vec<usize>, Vec<usize>) =
            pairs.iter().partition(|&&c| !used(c) && !used(c + 1));
        let mut free = clear.into_iter().chain(shared);
        let mut returns: Vec<Option<[usize; 2]>> = vec![None; r.deck_mixes];
        for (d, ret) in returns.iter_mut().enumerate() {
            if !p.mixes.iter().any(|m| m.deck == d) {
                continue;
            }
            match free.next() {
                Some(c) => *ret = Some([c, c + 1]),
                None => {
                    let name = p.mixes.iter().find(|m| m.deck == d).map_or(String::new(), |m| m.name.clone());
                    p.notes.push(format!("{name}: no playback channels left for its tape return"));
                }
            }
        }

        // The direct outs: on a tape return's channel, not connected; on a channel no output
        // plays any more, said.
        let name_of =
            |d: usize| p.mixes.iter().find(|m| m.deck == d).map_or(String::new(), |m| m.name.clone());
        for (i, c) in r.direct_outs.iter().enumerate() {
            let Some(c) = *c else { continue };
            if p.loopback.as_ref().is_some_and(|l| l.playback == c) {
                p.cut_outs.push(i);
                p.notes.push(format!(
                    "track {}'s direct out (playback {c}) is the loop's: it isn't connected",
                    i + 1
                ));
                continue;
            }
            if let Some(d) = returns.iter().position(|ret| ret.is_some_and(|pc| pc.contains(&c))) {
                p.cut_outs.push(i);
                p.notes.push(format!(
                    "track {}'s direct out (playback {c}) carries {}'s tape return: it isn't connected",
                    i + 1,
                    name_of(d)
                ));
                continue;
            }
            let plays: Vec<&str> = (f.outputs.iter())
                .filter(|o| {
                    let s = p.outputs.iter().find(|x| x.0 == o.numid).map_or(o.source, |x| x.1);
                    f.playback_of(s) == Some(c)
                })
                .map(|o| o.name.as_str())
                .collect();
            if plays.is_empty() {
                p.notes.push(format!("track {}'s direct out (playback {c}) reaches no output", i + 1));
            }
        }

        // The mixer's inputs. On an interface with a talkback the last is the talkback's: its
        // own talkback plays that input (alsa-scarlett-gui's docs), so it's the mic, its gains
        // off in every mix; anywhere else, or with a gain, the mic is heard whatever TALK says
        // (found at the rig, 2026-10-21).
        let n_inputs = f.mixer_inputs.len();
        let mic =
            f.talkback.and(f.talkback_mic.as_deref()).and_then(|m| f.source(m)).filter(|_| n_inputs > 0);
        let usable = n_inputs - usize::from(mic.is_some());
        p.tracks = vec![None; r.tracks.len()];
        let mut track_source: Vec<Option<usize>> = vec![None; r.tracks.len()];
        for (i, c) in r.tracks.iter().enumerate() {
            let Some(c) = *c else { continue };
            let Some(&s) = p.captures.get(c - 1) else { continue };
            if let Some(l) = &p.loopback
                && (l.capture == c || f.sources[s] == l.input)
            {
                p.cut_ins.push(i);
                p.notes.push(format!(
                    "track {}'s input (capture {c}) records {}, the loop's: it's left unpatched",
                    i + 1,
                    l.input
                ));
                continue;
            }
            let name = &f.sources[s];
            if s == 0 {
                p.notes.push(format!(
                    "track {}'s capture channel {c} records nothing: its source is off",
                    i + 1
                ));
                continue;
            }
            if !is_input(name) {
                continue; // a mix or a playback channel recorded: nothing to hear live
            }
            if Some(s) == mic {
                p.notes.push(format!(
                    "track {}'s input ({}) is the talkback mic: it's heard only through TALK",
                    i + 1,
                    shown(name)
                ));
                continue;
            }
            track_source[i] = Some(s);
        }
        // No mix on the interface: its mixer is left as found.
        if p.mixes.is_empty() {
            p.returns = vec![None; r.deck_mixes];
            return p;
        }
        p.mixer_inputs = vec![0; n_inputs];
        if let Some(m) = mic {
            p.mixer_inputs[usable] = m;
            p.talkback_input = Some(usable);
        }
        // The rig's inputs where it says (`mixer_inputs:`), the tape returns on those it leaves;
        // else shared out by need.
        let free = match r.settings.mixer_inputs.is_empty() {
            true => p.share_out(f, &track_source, usable, 2 * returns.iter().flatten().count()),
            false => p.lay_out(f, &r.settings.mixer_inputs, &track_source, mic, usable),
        };
        let mut pairs = free.as_chunks::<2>().0.iter();
        p.returns = vec![None; r.deck_mixes];
        for (d, pair) in returns.iter().enumerate() {
            let Some(pc) = pair else { continue };
            let Some(&[a, b]) = pairs.next() else {
                let name = p.mixes.iter().find(|m| m.deck == d).map_or(String::new(), |m| m.name.clone());
                p.notes.push(format!("{name}: no mixer inputs left for its tape return"));
                continue;
            };
            for (k, &c) in [a, b].iter().zip(pc) {
                p.mixer_inputs[*k] = f.source(&format!("PCM {c}")).unwrap_or(0);
            }
            p.returns[d] = Some(Return { playback: *pc, inputs: [a, b] });
        }
        p
    }
}

impl Plan {
    /// The mixer's first `usable` inputs shared out by need: the tracks' inputs, then the rest
    /// of the interface's (never the loop's: its bursts aren't to be heard; never the talkback
    /// mic), leaving the last for the tape returns. Returns those left for them.
    fn share_out(
        &mut self,
        f: &Focusrite,
        track_source: &[Option<usize>],
        usable: usize,
        n_returns: usize,
    ) -> Vec<usize> {
        let room = usable.saturating_sub(n_returns);
        let mut live: Vec<usize> = Vec::new();
        for &s in track_source.iter().flatten() {
            if !live.contains(&s) {
                live.push(s);
            }
        }
        let looped = self.loopback.as_ref().map(|l| l.input.as_str());
        let mic = f.talkback.and(f.talkback_mic.as_deref());
        let others: Vec<usize> = (0..f.sources.len())
            .filter(|&s| {
                let name = Some(f.sources[s].as_str());
                is_input(&f.sources[s]) && !live.contains(&s) && looped != name && mic != name
            })
            .collect();
        let fits = live.len().min(room);
        for (i, s) in track_source.iter().enumerate() {
            let Some(s) = s else { continue };
            match live.iter().position(|x| x == s).filter(|&k| k < fits) {
                Some(k) => self.tracks[i] = Some(k),
                None => self.notes.push(format!(
                    "track {}'s input ({}) isn't in the mixer: it isn't heard live",
                    i + 1,
                    shown(&f.sources[*s])
                )),
            }
        }
        for (k, &s) in live.iter().take(fits).enumerate() {
            self.mixer_inputs[k] = s;
        }
        for (k, &s) in others.iter().take(room - fits).enumerate() {
            self.mixer_inputs[fits + k] = s;
            self.others.push((fits + k, f.sources[s].clone()));
        }
        (room..usable).collect()
    }

    /// The mixer's inputs as the rig lays them out (`mixer_inputs:`, from 1): each track heard
    /// live where its input is, the inputs no track records as the other inputs; what the
    /// interface hasn't, the loop's input and the talkback mic said and left out. Returns those
    /// left, in order, for the tape returns.
    fn lay_out(
        &mut self,
        f: &Focusrite,
        layout: &[(usize, String)],
        track_source: &[Option<usize>],
        mic: Option<usize>,
        usable: usize,
    ) -> Vec<usize> {
        let looped = self.loopback.as_ref().map(|l| l.input.clone());
        for (n, name) in layout {
            let k = n - 1;
            let s = f.source_named(name).filter(|&s| is_input(&f.sources[s]));
            let why = match s {
                _ if k >= f.mixer_inputs.len() => {
                    format!("the interface's mixer has {} inputs", f.mixer_inputs.len())
                }
                _ if k >= usable => format!("it's the talkback's: {name} isn't heard there"),
                None => format!("{name} isn't one of the interface's inputs"),
                Some(s) if looped.as_deref() == Some(f.sources[s].as_str()) => {
                    format!("{name} is the loop's: it's never heard")
                }
                Some(s) if Some(s) == mic => {
                    format!("{name} is the talkback mic: it's heard only through TALK")
                }
                Some(s) => {
                    self.mixer_inputs[k] = s;
                    continue;
                }
            };
            self.notes.push(format!("the rig's mixer input {n}: {why}"));
        }
        for (i, s) in track_source.iter().enumerate() {
            let Some(s) = *s else { continue };
            match self.mixer_inputs[..usable].iter().position(|&x| x == s) {
                Some(k) => self.tracks[i] = Some(k),
                None => self.notes.push(format!(
                    "track {}'s input ({}) isn't in the rig's mixer: it isn't heard live",
                    i + 1,
                    shown(&f.sources[s])
                )),
            }
        }
        for k in 0..usable {
            let s = self.mixer_inputs[k];
            if s != 0 && !track_source.contains(&Some(s)) {
                self.others.push((k, f.sources[s].clone()));
            }
        }
        (0..usable).filter(|&k| self.mixer_inputs[k] == 0).collect()
    }

    /// The rig's settings as writes, and what's recorded once they're made; what the interface
    /// hasn't is said.
    fn settings(&mut self, f: &Focusrite, s: &Settings) {
        self.captures = f.captures.clone();
        if let Some(want) = &s.clock {
            let item = |items: &[String]| {
                let w = want.replace('/', "").to_lowercase();
                items.iter().position(|i| i.replace('/', "").to_lowercase() == w)
            };
            match &f.clock {
                Some((numid, items)) => match item(items) {
                    Some(v) => self.settings.push(Setting {
                        numid: *numid,
                        value: v as i64,
                        what: format!("the clock: {}", items[v]),
                    }),
                    None => self.notes.push(format!("the rig's clock: the interface has no {want} clock")),
                },
                None => self.notes.push("the rig's clock: the interface has no clock to set".into()),
            }
        }
        let looped = s.loopback.is_some().then_some(f.capture_ids.len());
        for (c, name) in &s.captures {
            if looped == Some(*c) {
                self.notes.push(format!("the rig's capture {c} is the loop's: {name} isn't recorded there"));
                continue;
            }
            let id = c.checked_sub(1).and_then(|i| f.capture_ids.get(i));
            let (Some(&numid), Some(src)) = (id, f.source_named(name)) else {
                self.notes.push(match id {
                    None => format!(
                        "the rig's capture {c}: the interface records {} channels",
                        f.capture_ids.len()
                    ),
                    Some(_) => format!("the rig's capture {c}: {name} isn't one of the interface's sources"),
                });
                continue;
            };
            self.captures[c - 1] = src;
            self.settings.push(Setting {
                numid,
                value: src as i64,
                what: format!("capture {c} records {}", f.sources[src]),
            });
        }
        for (n, w) in &s.inputs {
            let ctl = f.inputs.iter().find(|i| i.n == *n);
            for (label, on, numid) in [
                ("INST", w.inst, ctl.and_then(|c| c.inst)),
                ("PAD", w.pad, ctl.and_then(|c| c.pad)),
                ("AIR", w.air, ctl.and_then(|c| c.air)),
            ] {
                let Some(on) = on else { continue };
                match numid {
                    Some(numid) => self.settings.push(Setting {
                        numid,
                        value: on as i64,
                        what: format!("input {n} {label} {}", if on { "on" } else { "off" }),
                    }),
                    None => {
                        self.notes.push(format!("the rig's input {n}: the interface has no {label} there"))
                    }
                }
            }
        }
        for (g, on) in &s.phantom {
            match f.phantom.iter().find(|(x, _)| x == g.trim()) {
                Some((_, numid)) => self.settings.push(Setting {
                    numid: *numid,
                    value: *on as i64,
                    what: format!("48V {g} {}", if *on { "on" } else { "off" }),
                }),
                None => self.notes.push(format!("the rig's 48V {g}: the interface has no such switch")),
            }
        }
        if let Some(on) = s.phantom_persists {
            match f.phantom_persists {
                Some(numid) => self.settings.push(Setting {
                    numid,
                    value: on as i64,
                    what: format!("48V kept through a power cycle: {}", if on { "yes" } else { "no" }),
                }),
                None => self.notes.push("the rig's phantom_persists: the interface hasn't it".into()),
            }
        }
    }
}

impl Plan {
    /// The rig's loop, on the last channel each way the interface carries: its output fed from
    /// that playback channel, that capture channel recording its input, and a line output taken
    /// off the knob at full level. What the interface hasn't is said, and there's no loop.
    fn plan_loop(&mut self, f: &Focusrite, s: &Settings) {
        let Some((out, input)) = &s.loopback else { return };
        let (playback, capture) = (f.playbacks, f.capture_ids.len());
        let o = f.output(out);
        let (Some(o), Some(src), Some(pcm), Some(&id)) = (
            o,
            f.source_named(input),
            f.source(&format!("PCM {playback}")),
            capture.checked_sub(1).and_then(|i| f.capture_ids.get(i)),
        ) else {
            self.notes.push(match (o, f.source_named(input)) {
                (None, _) => format!("the rig's loop: the interface has no output {out}"),
                (_, None) => format!("the rig's loop: {input} isn't one of the interface's inputs"),
                _ => "the rig's loop: the interface has no playback or capture channel for it".into(),
            });
            return;
        };
        self.outputs.push((o.numid, pcm));
        if let Some(n) = o.name.strip_prefix("LINE ").and_then(|n| n.parse::<usize>().ok()) {
            self.sw_lines.push(n);
        }
        self.captures[capture - 1] = src;
        self.settings.push(Setting {
            numid: id,
            value: src as i64,
            what: format!("capture {capture} records {} (the loop)", f.sources[src]),
        });
        self.loopback = Some(Loop { out: o.name.clone(), input: f.sources[src].clone(), playback, capture });
    }

    /// The rig's prints, once the outputs are routed: what each output plays, and the mixes
    /// reaching any of them marked, so no print's input is heard in those.
    fn plan_prints(&mut self, f: &Focusrite, s: &Settings) {
        for (out, input) in &s.prints {
            let (Some(o), Some(src)) = (f.output(out), f.source_named(input)) else {
                self.notes.push(match f.output(out) {
                    None => format!("the rig's print from {out}: the interface has no output {out}"),
                    Some(_) => {
                        format!("the rig's print from {out}: {input} isn't one of the interface's inputs")
                    }
                });
                continue;
            };
            if !is_input(&f.sources[src]) {
                self.notes
                    .push(format!("the rig's print from {out}: {input} isn't one of the interface's inputs"));
                continue;
            }
            if self.loopback.as_ref().is_some_and(|l| l.out == o.name || l.input == f.sources[src]) {
                self.notes
                    .push(format!("the rig's print {out} → {input} is on the loop's cable: it's left out"));
                continue;
            }
            let plays = self.outputs.iter().find(|x| x.0 == o.numid).map_or(o.source, |x| x.1);
            let mix = (f.sources.get(plays))
                .and_then(|n| n.strip_prefix("Mix "))
                .and_then(|l| l.chars().next())
                .map(|l| (l as u8).wrapping_sub(b'A') as usize)
                .and_then(|m| {
                    let k = self.mixes.iter().position(|pm| pm.pair.contains(&m))?;
                    Some((k, self.mixes[k].pair.iter().position(|&x| x == m)?))
                });
            if let Some((k, _)) = mix {
                self.mixes[k].prints = true;
            }
            self.prints.push(Print {
                out: o.name.clone(),
                input: f.sources[src].clone(),
                source: src,
                mix,
                playback: f.playback_of(plays),
            });
        }
    }

    /// Whether mixer input k carries a print's input, which a mix reaching a print's output
    /// never hears.
    pub fn is_print_input(&self, k: usize) -> bool {
        self.mixer_inputs.get(k).is_some_and(|s| self.prints.iter().any(|x| x.source == *s))
    }

    /// What a print records, for people: `AUX 1 L`, `PCM 3`, or what the output plays.
    pub fn print_of(&self, f: &Focusrite, x: &Print) -> String {
        match (x.mix, x.playback) {
            (Some((k, side)), _) => format!("{} {}", self.mixes[k].name, if side == 0 { "L" } else { "R" }),
            (None, Some(c)) => format!("PCM {c}"),
            (None, None) => {
                let o = f.output(&x.out);
                let plays = o.map(|o| self.outputs.iter().find(|y| y.0 == o.numid).map_or(o.source, |y| y.1));
                plays.and_then(|s| f.sources.get(s)).map_or("nothing".into(), String::clone)
            }
        }
    }
}

/// A source that's one of the interface's inputs (not off, a mix or a playback channel).
fn is_input(name: &str) -> bool {
    ["Analogue ", "ADAT ", "S/PDIF "].iter().any(|p| name.starts_with(p))
}
