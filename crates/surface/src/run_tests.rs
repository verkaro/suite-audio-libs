//! The loop end to end: a fake FaderPort on a pair of sockets, and the testbed. No real MIDI
//! device is opened. Dexter's test, whose desk was its adapter on a simulated engine.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use as_mcu::surface::Opened;
use as_mcu::{Control, FADERPORT_V2, Opener, Status};

use super::*;
use crate::desk::{Cmd, Mode, Refusal};
use crate::testbed::Testbed;

struct Fake(Receiver<UnixStream>);

impl Opener for Fake {
    fn open(&mut self) -> Result<Opened, Status> {
        let s = self.0.try_recv().map_err(|_| Status::Missing)?;
        Ok(("Fake".into(), Box::new(s.try_clone().unwrap()), Box::new(s)))
    }
}

fn plug(tx: &Sender<UnixStream>) -> UnixStream {
    let (ours, dev) = UnixStream::pair().unwrap();
    tx.send(ours).unwrap();
    dev.set_read_timeout(Some(Duration::from_millis(50))).unwrap();
    dev
}

/// Waits until f is true, or fails.
fn until(what: &str, mut f: impl FnMut() -> bool) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(5), "{what}");
        thread::sleep(Duration::from_millis(5));
    }
}

fn note_on(note: u8, down: bool) -> [u8; 3] {
    [0x90, note, if down { 0x7f } else { 0 }]
}

/// The testbed, with what a desk with a screen adds: where it's told the surface is, and a
/// strip picked there.
struct Screen {
    bed: Testbed,
    open: AtomicBool,
    focus: Mutex<Option<usize>>,
    pick: Mutex<Option<usize>>,
}

impl Desk for Screen {
    fn state(&self) -> DeskState {
        self.bed.state()
    }
    fn command(&self, c: Cmd) -> Result<(), Refusal> {
        self.bed.command(c)
    }
    fn name(&self) -> String {
        self.bed.name()
    }
    fn open(&self, open: bool) {
        self.open.store(open, Ordering::Relaxed);
    }
    fn focus(&self, strip: Option<usize>) {
        *self.focus.lock().unwrap() = strip;
    }
    fn picked(&self) -> Option<usize> {
        self.pick.lock().unwrap().take()
    }
}

#[test]
fn the_surface_plays_the_testbed() {
    let desk = Arc::new(Screen {
        bed: Testbed::new(48_000, Instant::now()),
        open: AtomicBool::new(false),
        focus: Mutex::new(None),
        pick: Mutex::new(None),
    });
    let stop = Arc::new(AtomicBool::new(false));

    let (tx, rx) = channel();
    let mut dev = plug(&tx);
    let looping = {
        let (desk, stop) = (desk.clone(), stop.clone());
        thread::spawn(move || run_on(Fake(rx), &FADERPORT_V2, None, desk.as_ref(), &stop).unwrap())
    };

    // What the surface is sent comes in on dev: its LEDs and motor, collected as they come.
    let sent = Arc::new(Mutex::new(Vec::<u8>::new()));
    let reader = {
        let (sent, stop, mut dev) = (sent.clone(), stop.clone(), dev.try_clone().unwrap());
        thread::spawn(move || {
            let mut buf = [0u8; 256];
            while !stop.load(Ordering::Relaxed) {
                if let Ok(n) = dev.read(&mut buf) {
                    sent.lock().unwrap().extend_from_slice(&buf[..n]);
                }
            }
        })
    };

    // What a surface sends in its first 100 ms is what it kept from before, and is dropped.
    thread::sleep(as_mcu::STALE + Duration::from_millis(100));
    let play = FADERPORT_V2.note(Control::Play).unwrap();
    let stop_key = FADERPORT_V2.note(Control::Stop).unwrap();
    // PLAY on the surface plays the engine; the surface's PLAY LED comes on.
    dev.write_all(&note_on(play, true)).unwrap();
    dev.write_all(&note_on(play, false)).unwrap();
    until("the desk plays", || desk.state().mode == Mode::Play);
    let lit = |led: u8| sent.lock().unwrap().windows(3).any(|w| w == [0x90, led, 0x7f]);
    until("PLAY's LED", || lit(play));
    // STOP stops it.
    dev.write_all(&note_on(stop_key, true)).unwrap();
    dev.write_all(&note_on(stop_key, false)).unwrap();
    until("the desk stops", || desk.state().mode == Mode::Stop);

    // A move of the fader sets the selected strip's level: 0.6 of the travel is −12 dB, the
    // testbed's taper being Dexter's (a pitch bend, 14 bits, on channel 1).
    let pos = (0.6f64 * 16383.0).round() as u16;
    dev.write_all(&[0xe0, (pos & 0x7f) as u8, (pos >> 7) as u8]).unwrap();
    until("the fader's level", || desk.state().strips[0].fader.db() == Some(-12.0));

    // ARM on the surface arms the selected strip.
    let arm = FADERPORT_V2.note(Control::Arm).unwrap();
    dev.write_all(&note_on(arm, true)).unwrap();
    dev.write_all(&note_on(arm, false)).unwrap();
    until("the strip is armed", || desk.state().strips[0].arm);
    // And the desk is told where the surface is: open, on strip 1.
    until("the desk's mark", || desk.open.load(Ordering::Relaxed) && *desk.focus.lock().unwrap() == Some(1));

    // A strip picked on the desk's screen: the surface goes there, SELECT lit on its strip of
    // the eight, and says so.
    *desk.pick.lock().unwrap() = Some(2);
    let select = FADERPORT_V2.note(Control::Select).unwrap();
    until("the surface's strip", || *desk.focus.lock().unwrap() == Some(2));
    until("SELECT on strip 2", || lit(select + 1));
    // ARM is that strip's now, on its note there.
    dev.write_all(&note_on(arm + 1, true)).unwrap();
    dev.write_all(&note_on(arm + 1, false)).unwrap();
    until("strip 2 is armed", || desk.state().strips[1].arm);

    stop.store(true, Ordering::Relaxed);
    drop(dev);
    looping.join().unwrap();
    reader.join().unwrap();
}
