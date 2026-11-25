use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use super::*;
use crate::surface::Opened;
use as_rawmidi::{Want, find_wanted};

#[test]
fn messages_from_bytes() {
    let mut p = Parser::default();
    let m = p.feed(&[0x90, 0x5e, 0x7f, 0x5e, 0x00, 0xe0, 0x00, 0x40, 0xb0, 0x10, 0x41, 0xc3, 0x05, 0x06]);
    assert_eq!(
        m,
        [
            Midi::NoteOn { ch: 0, note: 0x5e, vel: 0x7f },
            Midi::NoteOn { ch: 0, note: 0x5e, vel: 0 }, // running status
            Midi::Bend { ch: 0, value: 0x2000 },
            Midi::Cc { ch: 0, cc: 0x10, value: 0x41 },
            Midi::Program { ch: 3, program: 5 },
            Midi::Program { ch: 3, program: 6 },
        ]
    );
    for m in &m {
        assert_eq!(Parser::default().feed(&m.bytes()), std::slice::from_ref(m), "{m:?} round trip");
    }
}

#[test]
fn real_time_anywhere_and_sysex() {
    let mut p = Parser::default();
    // A clock inside a note, then a SysEx with a clock inside it too.
    let m = p.feed(&[0x90, 0x5e, 0xf8, 0x7f, 0xf0, 0x00, 0x00, 0x66, 0xf8, 0x14, 0xf7]);
    assert_eq!(
        m,
        [
            Midi::System(vec![0xf8]),
            Midi::NoteOn { ch: 0, note: 0x5e, vel: 0x7f },
            Midi::System(vec![0xf8]),
            Midi::SysEx(vec![0xf0, 0x00, 0x00, 0x66, 0x14, 0xf7]),
        ]
    );
    // A SysEx cut short by a status byte is dropped; data with no status is dropped; system
    // common messages have no running status.
    let m = p.feed(&[0xf0, 0x01, 0x90, 0x20, 0x7f, 0xf7, 0x05, 0xf2, 0x01, 0x02, 0x03, 0xf6]);
    assert_eq!(
        m,
        [
            Midi::NoteOn { ch: 0, note: 0x20, vel: 0x7f },
            Midi::System(vec![0xf2, 0x01, 0x02]),
            Midi::System(vec![0xf6]),
        ]
    );
}

#[test]
fn mackie_events() {
    let d = |b: &[u8]| Parser::default().feed(b).into_iter().map(decode).collect::<Vec<_>>();
    assert_eq!(
        d(&[0x90, 0x68, 0x7f, 0x90, 0x68, 0x00, 0x80, 0x70, 0x00]),
        [
            Event::Touch { n: 0, on: true },
            Event::Touch { n: 0, on: false },
            Event::Touch { n: mcu::MASTER, on: false }
        ]
    );
    assert_eq!(
        d(&[0x90, 0x5e, 0x7f, 0x90, 0x5e, 0x00, 0x80, 0x5d, 0x40]),
        [
            Event::Button { note: 0x5e, down: true },
            Event::Button { note: 0x5e, down: false },
            Event::Button { note: 0x5d, down: false }
        ]
    );
    assert_eq!(
        d(&[0xe0, 0x7f, 0x7f, 0xe8, 0x00, 0x00]),
        [Event::Fader { n: 0, value: 0x3fff }, Event::Fader { n: 8, value: 0 }]
    );
    assert_eq!(
        d(&[0xb0, 0x10, 0x01, 0x10, 0x41, 0x17, 0x4f, 0x3c, 0x03]),
        [
            Event::Knob { n: 0, delta: 1 },
            Event::Knob { n: 0, delta: -1 },
            Event::Knob { n: 7, delta: -15 },
            Event::Jog(3)
        ]
    );
    // Other channels, other controls and SysEx pass as they came.
    assert_eq!(
        d(&[0x91, 0x5e, 0x7f, 0xb0, 0x30, 0x01, 0xe9, 0x00, 0x00]),
        [
            Event::Other(Midi::NoteOn { ch: 1, note: 0x5e, vel: 0x7f }),
            Event::Other(Midi::Cc { ch: 0, cc: 0x30, value: 1 }),
            Event::Other(Midi::Bend { ch: 9, value: 0 })
        ]
    );
}

#[test]
fn leds_and_motor() {
    let outs = [
        Out::Led { note: 0x5e, lamp: Lamp::On },
        Out::Led { note: 0x5f, lamp: Lamp::Blink },
        Out::Led { note: 0x5d, lamp: Lamp::Off },
        Out::Motor { n: 0, value: 0x2000 },
        Out::Motor { n: 0, value: 0xffff },
    ];
    assert_eq!(hex(&encode(&outs)), "90 5E 7F 90 5F 01 90 5D 00 E0 00 40 E0 7F 7F");
}

#[test]
fn tables() {
    for dev in [&STANDARD, &FADERPORT_V2] {
        // A note is one control; a control may have two (the FaderPort's locate, with SHIFT
        // and in MARKER's mode).
        for (c, n) in dev.buttons {
            assert_eq!(dev.control(*n), Some(*c), "{} {c}: one control a note", dev.name);
        }
    }
    assert_eq!(STANDARD.note(Control::Play), Some(0x5e));
    assert_eq!(describe(&Event::Button { note: 0x5e, down: true }, &STANDARD), "PLAY down");
    assert_eq!(describe(&Event::Button { note: 0x7a, down: false }, &STANDARD), "note 7A up");
    assert_eq!(describe(&Event::Knob { n: 0, delta: -2 }, &STANDARD), "knob 1 -2");
}

/// The FaderPort's card is named `FP2`: the table's second name finds it. (Finding a port is
/// `as-rawmidi`'s, and tested there.)
#[test]
fn the_tables_names_find_the_faderport() {
    let root = std::env::temp_dir().join(format!("as-mcu-{}-find", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("dev/snd")).unwrap();
    std::fs::create_dir_all(root.join("proc/asound")).unwrap();
    std::fs::write(root.join("proc/asound/cards"), " 1 [FP2            ]: USB-Audio - PreSonus FP2\n")
        .unwrap();
    std::fs::write(root.join("dev/snd/midiC1D0"), "").unwrap();
    let wants: Vec<Want> = FADERPORT_V2.cards.iter().map(|c| Want::parse(c)).collect();
    assert_eq!(find_wanted(&root, &wants).map(|p| p.card), Some(1));
    let _ = std::fs::remove_dir_all(&root);
}

/// Hands out the test's ends of socket pairs, one per open, and Missing when there's none.
struct Fake(Receiver<UnixStream>);

impl Opener for Fake {
    fn open(&mut self) -> Result<Opened, Status> {
        let s = self.0.try_recv().map_err(|_| Status::Missing)?;
        Ok(("Fake".into(), Box::new(s.try_clone().unwrap()), Box::new(s)))
    }
}

/// A fake surface plugged in: the device's end.
fn plug(tx: &Sender<UnixStream>) -> UnixStream {
    let (ours, dev) = UnixStream::pair().unwrap();
    tx.send(ours).unwrap();
    dev.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    dev
}

fn next(rx: &Receiver<Note>) -> Note {
    rx.recv_timeout(Duration::from_secs(5)).expect("a note from the surface")
}

#[test]
fn kept_open() {
    let dir = std::env::temp_dir().join(format!("as-mcu-{}-log", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let log = Log::create(&dir.join("log")).unwrap();
    let (tx, plugged) = channel();
    let (surface, rx) = Surface::start(Fake(plugged), &FADERPORT_V2, Some(log), Duration::from_millis(10));
    assert_eq!(next(&rx), Note::Status(Status::Missing));
    assert!(!surface.send(&[Out::Led { note: 0x5e, lamp: Lamp::On }]), "nothing to send to");

    let mut dev = plug(&tx);
    assert_eq!(next(&rx), Note::Status(Status::Open("Fake".into())));
    // What the surface held while closed comes at once, and is dropped.
    dev.write_all(&[0x90, 0x5d, 0x7f, 0x90, 0x5d, 0x00]).unwrap();
    std::thread::sleep(STALE + Duration::from_millis(50));
    dev.write_all(&[0x90, 0x5e, 0x7f, 0xfe, 0x5e, 0x00, 0xe0, 0x00, 0x40]).unwrap();
    assert_eq!(next(&rx), Note::Event(Event::Button { note: 0x5e, down: true }));
    assert_eq!(
        next(&rx),
        Note::Event(Event::Button { note: 0x5e, down: false }),
        "active sensing passed over"
    );
    assert_eq!(next(&rx), Note::Event(Event::Fader { n: 0, value: 0x2000 }));
    assert!(surface.send(&[Out::Led { note: 0x5e, lamp: Lamp::On }, Out::Motor { n: 0, value: 0x3fff }]));
    let mut b = [0u8; 6];
    dev.read_exact(&mut b).unwrap();
    assert_eq!(hex(&b), "90 5E 7F E0 7F 7F");

    // Unplugged: missing until plugged in again, then open again.
    drop(dev);
    assert_eq!(next(&rx), Note::Status(Status::Missing));
    let mut dev = plug(&tx);
    assert_eq!(next(&rx), Note::Status(Status::Open("Fake".into())));
    std::thread::sleep(STALE + Duration::from_millis(50));
    dev.write_all(&[0xb0, 0x10, 0x41]).unwrap();
    assert_eq!(next(&rx), Note::Event(Event::Knob { n: 0, delta: -1 }));
    drop(surface);

    let log = std::fs::read_to_string(dir.join("log")).unwrap();
    for want in [
        "Missing",
        "Open(\"Fake\")",
        "in  90 5D 7F  STOP down (held while closed: dropped)",
        "in  90 5E 7F  PLAY down",
        "out 90 5E 7F  Led",
        "in  B0 10 41  knob 1 -1",
    ] {
        assert!(log.contains(want), "{want:?} in the log:\n{log}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_capture_checked() {
    let text =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/standard.txt")).unwrap();
    assert_eq!(capture::problems(&text), Vec::<String>::new());
    let bad = "button PLAY: 90 5E 7F\nfader touch: E0 00 00\nfader up: E0 00 40 E0 00 50\nknob one click right: B0 10 41\nknob push: B0 10 01\nled MUTE 10: n\n";
    assert_eq!(
        capture::problems(bad),
        [
            "button PLAY: not one note pressed and let go (90 5E 7F)",
            "fader touch: not fader 1 touched, then let go (E0 00 00)",
            "fader up: fader 1 didn't go from the bottom to the top (E0 00 40 E0 00 50)",
            "knob one click right: not knob 1 as Mackie Control has it (B0 10 41)",
            "knob push: not knob 1 as Mackie Control has it (B0 10 01)",
            "led MUTE 10: didn't answer (n)",
        ]
    );
}

/// Every surface's capture in the repository's `testdata/` decodes as Mackie Control has it,
/// but for what its findings explain.
#[test]
fn every_capture_decodes() {
    let top = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata");
    for e in std::fs::read_dir(&top).into_iter().flatten().flatten() {
        let Ok(text) = std::fs::read_to_string(e.path().join("capture.txt")) else { continue };
        let findings = std::fs::read_to_string(e.path().join("findings.txt")).unwrap_or_default();
        let p = capture::unexplained(&text, &findings);
        assert!(p.is_empty(), "{}:\n{}", e.path().display(), p.join("\n"));
    }
}

#[test]
fn findings_explain() {
    let text = "button PLAY: 90 5E 7F 90 5E 00\nbutton SHIFT:\nled blink 5E: n\n";
    assert_eq!(
        capture::unexplained(text, "button SHIFT: kept to itself\n"),
        ["led blink 5E: didn't answer (n)"]
    );
    assert_eq!(
        capture::unexplained(text, "button SHIFT: x\nled blink 5E: x\nbutton PLAY: x\n"),
        ["button PLAY: explains nothing in the capture"]
    );
}

/// The FaderPort V2's table is its capture's: each button Warren named, at the note it sent.
#[test]
fn the_faderport_table_is_its_capture() {
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/faderport-v2/capture.txt"),
    )
    .unwrap();
    let lines = capture::parse(&text);
    for (label, c) in [
        ("solo", Control::Solo),
        ("mute", Control::Mute),
        ("arm", Control::Arm),
        ("bypass", Control::Fine),
        ("prev", Control::PrevMark),
        ("next", Control::NextMark),
        ("loop", Control::Cycle),
        ("rewind", Control::Rew),
        ("fast-forward", Control::Ff),
        ("stop", Control::Stop),
        ("play-pause", Control::Play),
        ("record", Control::Rec),
    ] {
        let l = lines.iter().find(|l| l.what == format!("button {label}")).unwrap();
        let sent = match l.events().first() {
            Some(Event::Button { note, .. }) => *note,
            e => panic!("{label}: {e:?}"),
        };
        assert_eq!(FADERPORT_V2.note(c), Some(sent), "{label}");
    }
    assert_eq!(FADERPORT_V2.note(Control::Shift), None, "SHIFT is the unit's own");
    assert_eq!(FADERPORT_V2.button(0x13), Some((Control::Mute, 3)), "strip 4's MUTE");
    assert_eq!(STANDARD.button(0x13), None, "a surface that doesn't roam");
}
