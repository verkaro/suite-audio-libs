//! The surface played against a desk: one loop owns the desk's controls;
//! the surface's events come to it from the device's thread, go through the binding to the
//! desk, and the desk's state goes back to the surface's LEDs and motor. It says what happens
//! in plain lines on stdout (Dexter's session keeps them in `surface.log`), and with a log file
//! every message.

use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use as_mcu::{Alsa, Device, Event, KNOWN, Lamp, Log, Note, Opener, Out, STANDARD, Status, Surface, describe};
use as_rawmidi::{Want, find_wanted};

use crate::binding::surface::Binding;
use crate::binding::{Controls, Input};
use crate::desk::{Desk, DeskState};

/// How often the loop looks at the desk when the surface says nothing.
const FRAME: Duration = Duration::from_millis(50);
/// How often the surface's port is looked for again once it has been there.
const RETRY: Duration = Duration::from_secs(1);

/// What to play and how it's told.
#[derive(Clone, Debug)]
pub struct Options {
    /// The surface: part of its card's name (`FaderPort`), `hw:N,M`, or its port's path. None:
    /// the first surface known.
    pub device: Option<String>,
    /// Every message in and out, with the time, into this file.
    pub log: Option<PathBuf>,
    /// Where `/proc/asound` and `/dev/snd` are looked for: `/` on a machine, the tests' scratch.
    pub root: PathBuf,
}

impl Default for Options {
    fn default() -> Options {
        Options { device: None, log: None, root: PathBuf::from("/") }
    }
}

/// What to look for, and the table to read it with: a known surface's when `device` names one of
/// its cards (or there's none), else Mackie Control's own.
fn which(device: Option<&str>) -> (Vec<Want>, &'static Device) {
    let Some(d) = device else {
        let wants = KNOWN.iter().flat_map(|k| k.cards.iter().map(|c| Want::Name(c.to_string()))).collect();
        return (wants, KNOWN[0]);
    };
    let lower = d.to_lowercase();
    let dev = KNOWN.iter().find(|k| k.cards.iter().any(|c| lower.contains(&c.to_lowercase()))).copied();
    (vec![Want::parse(d)], dev.unwrap_or(&STANDARD))
}

/// Whether the surface is plugged in: looked at once, so a session that finds none doesn't look
/// again (a button on the page does, when asked).
pub fn present(opts: &Options) -> bool {
    let (wants, _) = which(opts.device.as_deref());
    find_wanted(&opts.root, &wants).is_some()
}

pub fn status_line(st: &Status) -> String {
    match st {
        Status::Missing => "no surface found: plugged in, in MCU mode? (--device names it)".into(),
        Status::Busy(n) => format!("{n}: another program has it (a DAW?); trying again"),
        Status::NoAccess(n) => format!("{n}: not allowed to open it (is this user in the audio group?)"),
        Status::Failed(n, e) => format!("{n}: {e}"),
        Status::Open(n) => format!("{n}: open"),
    }
}

fn open_log(opts: &Options) -> io::Result<Option<Log>> {
    opts.log
        .as_deref()
        .map(|p| Log::create(p).map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", p.display()))))
        .transpose()
}

/// The surface alone, until stop is set: each event as it comes, by the control's name, and a
/// button's LED lit while it's held. The quick check that a surface is found and read.
pub fn monitor(opts: &Options, stop: &AtomicBool) -> io::Result<()> {
    let (wants, dev) = which(opts.device.as_deref());
    let (surface, notes) =
        Surface::start(Alsa { root: opts.root.clone(), wants }, dev, open_log(opts)?, RETRY);
    while !stop.load(Ordering::Relaxed) {
        match notes.recv_timeout(FRAME) {
            Ok(Note::Status(st)) => println!("{}", status_line(&st)),
            Ok(Note::Event(e)) => {
                println!("{}", describe(&e, dev));
                if let Event::Button { note, down } = e {
                    surface.send(&[Out::Led { note, lamp: if down { Lamp::On } else { Lamp::Off } }]);
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    Ok(())
}

/// Carries out an input, on the linked strips too; the first refusal's text if the desk refuses.
fn act(desk: &dyn Desk, controls: &mut Controls, i: Input) -> Option<String> {
    let cmds = controls.act_all(i, &desk.state());
    cmds.into_iter().filter_map(|c| desk.command(c).err()).map(|r| r.text).next()
}

/// The desk as a line, for the log: the transport, the mix, the strip the surface is on.
fn desk_line(s: &DeskState, c: &Controls) -> String {
    let mut out = format!("{:?} {}", s.mode, clock(s.position, s.rate)).to_lowercase();
    if let Some(m) = &s.mix {
        out += &format!(" mix {}", m.name);
    }
    out += &format!(" | strip {}", c.strip);
    if let Some(st) = s.strips.get(c.strip.wrapping_sub(1)) {
        out += &format!(" {}", st.name);
        if st.has.fader {
            out += &format!(" {}", st.fader);
        }
        if st.has.pan {
            out += &format!(" pan {}", st.pan);
        }
        for (on, name) in [(st.mute, "mute"), (st.solo, "solo"), (st.arm, "arm"), (st.recording, "rec")] {
            if on {
                out += &format!(" {name}");
            }
        }
    }
    if !c.linked.is_empty() {
        out += &format!(" | linked {:?}", c.linked);
    }
    out
}

/// A counter as `h:mm:ss.t`.
fn clock(frames: u64, rate: u32) -> String {
    let t = frames * 10 / rate.max(1) as u64;
    format!("{}:{:02}:{:02}.{}", t / 36_000, t / 600 % 60, t / 10 % 60, t % 10)
}

/// The surface played against the desk until stop is set.
pub fn run(opts: &Options, desk: &dyn Desk, stop: &AtomicBool) -> io::Result<()> {
    let (wants, dev) = which(opts.device.as_deref());
    run_on(Alsa { root: opts.root.clone(), wants }, dev, open_log(opts)?, desk, stop)
}

/// [`run`] on a port opened by opener: the tests', with a fake device.
pub fn run_on(
    opener: impl Opener,
    dev: &'static Device,
    log: Option<Log>,
    desk: &dyn Desk,
    stop: &AtomicBool,
) -> io::Result<()> {
    let (surface, notes) = Surface::start(opener, dev, log.clone(), RETRY);
    let mut controls = Controls::default();
    let mut binding = Binding::new(dev);
    // Whether the surface is there: it's on a strip only while it is.
    let mut open = false;
    let mut named = String::new();
    let mut logged = String::new();
    let mut next_frame = Instant::now();
    let say = |line: &str| {
        println!("{line}");
        if let Some(l) = &log {
            l.line(line);
        }
    };
    let refused = |text: &str| say(&format!("refused: {text}"));
    while !stop.load(Ordering::Relaxed) {
        let wait = next_frame.saturating_duration_since(Instant::now());
        match recv(&notes, wait) {
            Some(Note::Status(st)) => {
                open = matches!(st, Status::Open(_));
                if open {
                    binding.reset();
                }
                say(&format!("surface: {}", status_line(&st)));
            }
            Some(Note::Event(e)) => {
                let now = Instant::now();
                for (i, note) in binding.event(&e, now, &desk.state()) {
                    if let Some(r) = act(desk, &mut controls, i) {
                        refused(&r);
                        binding.refused(note, now);
                    }
                }
                surface.send(&binding.outs(now, &desk.state(), &controls));
                next_frame = now;
            }
            None => {}
        }
        let now = Instant::now();
        if now < next_frame {
            continue;
        }
        next_frame = now + FRAME;
        for r in desk.refusals() {
            refused(&r.text);
        }
        let state = desk.state();
        // A strip picked on the desk's screen: the surface goes there (its SELECT lit moves it).
        if let Some(n) = desk.picked() {
            controls.act(Input::Select(n), &state);
        }
        surface.send(&binding.outs(now, &state, &controls));
        desk.open(open);
        desk.focus((open && controls.strip <= state.strips.len()).then_some(controls.strip));
        let name = desk.name();
        if name != named {
            say(&format!("desk: {name}"));
            named = name;
        }
        // The desk's line, into the log as it changes (the counter aside), for a session reading
        // what the surface's messages did; while it rolls, every second.
        let still = desk_line(&DeskState { position: 0, ..state.clone() }, &controls);
        if still != logged {
            if let Some(l) = &log {
                l.line(&format!("desk {}", desk_line(&state, &controls)));
            }
            logged = still;
        }
    }
    Ok(())
}

/// The next note within wait, if one comes.
fn recv(notes: &Receiver<Note>, wait: Duration) -> Option<Note> {
    match notes.recv_timeout(wait) {
        Ok(n) => Some(n),
        Err(RecvTimeoutError::Timeout) => None,
        // The surface's thread is the only sender, and it never ends: wait out the frame.
        Err(RecvTimeoutError::Disconnected) => {
            std::thread::sleep(wait);
            None
        }
    }
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
