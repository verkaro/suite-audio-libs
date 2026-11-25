//! A surface kept open: its events as they come, what we send it, and its port looked for
//! again whenever it goes (unplugged, or another program let go of it).

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use as_rawmidi::{Refused, Want, find_wanted, open};

use crate::mcu::{Event, Out, decode, encode};
use crate::midi::{Midi, Parser, hex};
use crate::table::Device;

/// What comes this soon after the port opens was held by the surface while no one had it
/// (the FaderPort V2 keeps it, and hands it all over at once): presses long gone, dropped.
pub const STALE: Duration = Duration::from_millis(100);

/// Where the surface is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// Not found: not plugged in, or not named as `--device` says.
    Missing,
    /// Found, but another program has it.
    Busy(String),
    /// Found, but not ours to open (the user isn't in the `audio` group).
    NoAccess(String),
    /// Found but not opened, for another reason.
    Failed(String, String),
    /// Open: its name. Everything we'd shown on it must be sent again.
    Open(String),
}

/// What the surface tells us.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Note {
    Status(Status),
    Event(Event),
}

/// The port as a reader and a writer, or where it is instead.
pub type Opened = (String, Box<dyn Read + Send>, Box<dyn Write + Send>);

/// Opens the surface's port: the real one, or the tests'.
pub trait Opener: Send + 'static {
    fn open(&mut self) -> Result<Opened, Status>;
}

/// The surface's port under root (`/` on a machine), as wants names it.
pub struct Alsa {
    pub root: PathBuf,
    pub wants: Vec<Want>,
}

impl Opener for Alsa {
    fn open(&mut self) -> Result<Opened, Status> {
        let p = find_wanted(&self.root, &self.wants).ok_or(Status::Missing)?;
        match open(&p.path) {
            Ok((r, w)) => Ok((p.name, Box::new(r), Box::new(w))),
            // Gone between being found and being opened.
            Err(Refused::Missing) => Err(Status::Missing),
            Err(Refused::Busy) => Err(Status::Busy(p.name)),
            Err(Refused::Denied) => Err(Status::NoAccess(p.name)),
            Err(Refused::Other(e)) => Err(Status::Failed(p.name, e.to_string())),
        }
    }
}

/// Every message in and out, with the time since the start, as bytes and as meant.
#[derive(Clone)]
pub struct Log(Arc<Mutex<(File, Instant)>>);

impl Log {
    pub fn create(path: &Path) -> io::Result<Log> {
        Ok(Log(Arc::new(Mutex::new((File::create(path)?, Instant::now())))))
    }

    pub fn line(&self, what: &str) {
        let mut g = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let t = g.1.elapsed().as_secs_f64();
        let _ = writeln!(g.0, "{t:10.3} {what}");
    }
}

/// An event as words, with its control's name on this device.
pub fn describe(e: &Event, dev: &Device) -> String {
    let named = |note: u8| match dev.button(note) {
        Some((c, 0)) => c.to_string(),
        Some((c, n)) => format!("{c} {}", n + 1),
        None => format!("note {note:02X}"),
    };
    match e {
        Event::Button { note, down } => format!("{} {}", named(*note), if *down { "down" } else { "up" }),
        Event::Fader { n, value } => format!("fader {} {value}", n + 1),
        Event::Touch { n, on } => format!("fader {} {}", n + 1, if *on { "touched" } else { "let go" }),
        Event::Knob { n, delta } => format!("knob {} {delta:+}", n + 1),
        Event::Jog(d) => format!("jog {d:+}"),
        Event::Other(m) => format!("other {}", hex(&m.bytes())),
    }
}

/// The surface, kept open by a thread of its own until dropped.
pub struct Surface {
    writer: Arc<Mutex<Option<Box<dyn Write + Send>>>>,
    stop: Arc<AtomicBool>,
    log: Option<Log>,
}

impl Surface {
    /// Opens the surface with opener, trying again every retry while it can't, and every
    /// retry after it goes. Its notes come on the receiver.
    pub fn start(
        mut opener: impl Opener,
        dev: &'static Device,
        log: Option<Log>,
        retry: Duration,
    ) -> (Surface, Receiver<Note>) {
        let (tx, rx) = channel();
        let writer: Arc<Mutex<Option<Box<dyn Write + Send>>>> = Arc::default();
        let stop = Arc::new(AtomicBool::new(false));
        let (w, s, l) = (writer.clone(), stop.clone(), log.clone());
        thread::spawn(move || {
            let mut last: Option<Status> = None;
            while !s.load(Ordering::Acquire) {
                match opener.open() {
                    Ok((name, reader, writer)) => {
                        *w.lock().unwrap_or_else(|e| e.into_inner()) = Some(writer);
                        let st = Status::Open(name);
                        if !tell(&tx, &l, st.clone()) {
                            return;
                        }
                        last = Some(st);
                        if !read(reader, &tx, &l, dev, &s) {
                            return;
                        }
                        *w.lock().unwrap_or_else(|e| e.into_inner()) = None;
                    }
                    Err(st) => {
                        if last.as_ref() != Some(&st) {
                            if !tell(&tx, &l, st.clone()) {
                                return;
                            }
                            last = Some(st);
                        }
                        thread::sleep(retry);
                    }
                }
            }
        });
        (Surface { writer, stop, log }, rx)
    }

    /// Sends outs, if the surface is open; false when it isn't (they're dropped).
    pub fn send(&self, outs: &[Out]) -> bool {
        if outs.is_empty() {
            return true;
        }
        let mut g = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        let Some(w) = g.as_mut() else { return false };
        if let Some(l) = &self.log {
            for o in outs {
                l.line(&format!("out {:<9} {o:?}", hex(&o.midi().bytes())));
            }
        }
        if w.write_all(&encode(outs)).and_then(|_| w.flush()).is_err() {
            *g = None; // the reader notices too, and looks for it again
            return false;
        }
        true
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

/// Logs and passes on a status; false when no one's listening.
fn tell(tx: &Sender<Note>, log: &Option<Log>, st: Status) -> bool {
    if let Some(l) = log {
        l.line(&format!("{st:?}"));
    }
    tx.send(Note::Status(st)).is_ok()
}

/// Reads until the port goes; false when no one's listening, or we're stopping.
fn read(
    mut r: Box<dyn Read + Send>,
    tx: &Sender<Note>,
    log: &Option<Log>,
    dev: &Device,
    stop: &AtomicBool,
) -> bool {
    let mut parser = Parser::default();
    let mut buf = [0u8; 256];
    let opened = Instant::now();
    loop {
        let n = match r.read(&mut buf) {
            Ok(0) | Err(_) => return !stop.load(Ordering::Acquire),
            Ok(n) => n,
        };
        let stale = opened.elapsed() < STALE;
        for m in parser.feed(&buf[..n]) {
            if matches!(m, Midi::System(ref b) if b == &[0xfe]) {
                continue; // active sensing, a few times a second from some surfaces
            }
            let bytes = m.bytes();
            let e = decode(m);
            if let Some(l) = log {
                let held = if stale { " (held while closed: dropped)" } else { "" };
                l.line(&format!("in  {:<9} {}{held}", hex(&bytes), describe(&e, dev)));
            }
            if stale {
                continue;
            }
            if tx.send(Note::Event(e)).is_err() {
                return false;
            }
        }
    }
}
