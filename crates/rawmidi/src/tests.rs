use super::*;
use std::fs;
use std::io::Write;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("as-rawmidi-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn msg_bytes() {
    assert_eq!(msg::note_on(0, 60, 100), [0x90, 60, 100]);
    assert_eq!(msg::note_on(15, 108, 127), [0x9F, 108, 127]);
    assert_eq!(msg::note_off(0, 21), [0x80, 21, 0x40]);
    assert_eq!(msg::note_off(15, 60), [0x8F, 60, 0x40]);
    assert_eq!(msg::program(0, 98), [0xC0, 98]);
    assert_eq!(msg::program(15, 0), [0xCF, 0]);
    assert_eq!(msg::control(9, 7, 100), [0xB9, 7, 100]);
    assert_eq!(msg::all_sound_off(0), [0xB0, 120, 0]);
    assert_eq!(msg::all_notes_off(15), [0xBF, 123, 0]);
    // Masked: a channel past 15 and a value past 127 can't corrupt the status byte.
    assert_eq!(msg::note_on(16, 200, 255), [0x90, 72, 127]);
}

#[test]
fn ports_scratch() {
    let root = scratch("ports");
    fs::create_dir_all(root.join("proc/asound")).unwrap();
    fs::create_dir_all(root.join("dev/snd")).unwrap();
    fs::write(
        root.join("proc/asound/cards"),
        " 0 [PCH            ]: HDA-Intel - HDA Intel PCH\n                      HDA Intel PCH at 0x98814000 irq 60\n \
         2 [USB            ]: USB-Audio - Clarett 2Pre USB\n                      Focusrite Clarett 2Pre USB at usb-0000:00:14.0-2, high speed\n",
    )
    .unwrap();
    for f in ["midiC2D0", "midiC1D0", "controlC2", "pcmC0D0p"] {
        fs::write(root.join("dev/snd").join(f), b"").unwrap();
    }
    let p = ports(&root);
    assert_eq!(p.len(), 2);
    assert_eq!((p[0].card, p[0].device, p[0].name.as_str(), p[0].card_id.as_str()), (1, 0, "hw:1", ""));
    assert_eq!((p[1].card, p[1].device), (2, 0));
    assert_eq!(p[1].card_id, "USB");
    assert_eq!(p[1].name, "Clarett 2Pre USB");
    assert_eq!(find(&root, "USB", 0).unwrap().path, root.join("dev/snd/midiC2D0"));
    assert!(find(&root, "USB", 1).is_none());
    assert!(find(&root, "PCH", 0).is_none());
}

#[test]
fn send_writes() {
    let root = scratch("send");
    let path = root.join("midiC2D0");
    fs::write(&path, b"").unwrap();
    let mut out = Out::open(&path).unwrap();
    out.send(&msg::note_on(0, 60, 100)).unwrap();
    panic(&mut out, 0).unwrap();
    drop(out);
    let got = fs::read(&path).unwrap();
    assert_eq!(got.len(), 3 + 6 + 384);
    assert_eq!(&got[..3], &[0x90, 60, 100]);
    assert_eq!(&got[3..9], &[0xB0, 120, 0, 0xB0, 123, 0]);
    assert_eq!(&got[9..12], &[0x80, 0, 0x40]);
    assert_eq!(&got[got.len() - 3..], &[0x80, 127, 0x40]);
}

#[test]
fn refused_missing() {
    let root = scratch("missing");
    assert!(matches!(Out::open(&root.join("nope")), Err(Refused::Missing)));
    assert_eq!(Refused::Missing.to_string(), "no such port (unplugged?)");
}

/// A scratch root with the cards and ports of a machine with a FaderPort as card 1.
fn machine(name: &str) -> PathBuf {
    let root = scratch(name);
    fs::create_dir_all(root.join("dev/snd")).unwrap();
    fs::create_dir_all(root.join("proc/asound")).unwrap();
    fs::write(
        root.join("proc/asound/cards"),
        " 0 [PCH            ]: HDA-Intel - HDA Intel PCH\n                      HDA Intel PCH at 0xf7f10000 irq 32\n \
         1 [FP2            ]: USB-Audio - PreSonus FP2\n                      PreSonus PreSonus FP2 at usb-0000:00:14.0-2, full speed\n\
         10 [Interface      ]: USB-Audio - Interface\n                      Some Interface at usb-0000:00:14.0-3, high speed\n",
    )
    .unwrap();
    for f in ["midiC10D0", "midiC1D0", "pcmC0D0p", "controlC1"] {
        fs::write(root.join("dev/snd").join(f), "").unwrap();
    }
    root
}

#[test]
fn finding_by_what_is_wanted() {
    let root = machine("wanted");
    assert_eq!(
        cards(&root)[1],
        Card {
            number: 1,
            id: "FP2".into(),
            name: "PreSonus FP2".into(),
            long: "PreSonus PreSonus FP2 at usb-0000:00:14.0-2, full speed".into()
        }
    );
    let p = ports(&root);
    assert_eq!(
        p.iter().map(|p| (p.card, p.device, p.name.as_str())).collect::<Vec<_>>(),
        [(1, 0, "PreSonus FP2"), (10, 0, "Interface")]
    );
    let found = |w: &[&str]| {
        find_wanted(&root, &w.iter().map(|s| Want::parse(s)).collect::<Vec<_>>()).map(|p| p.card)
    };
    // The first of the wants that's there, by id, name or long name, any case.
    assert_eq!(found(&["FaderPort", "FP2"]), Some(1));
    assert_eq!(found(&["fp2"]), Some(1));
    assert_eq!(found(&["some interface at"]), Some(10));
    assert_eq!(found(&["hw:10,0"]), Some(10));
    assert_eq!(found(&["hw:10,0,0"]), Some(10));
    assert_eq!(found(&["hw:1"]), Some(1));
    assert_eq!(found(&["/dev/snd/midiC10D0"]), Some(10));
    assert_eq!(found(&["X-Touch"]), None);
    assert_eq!(found(&["hw:2,0"]), None);
    assert_eq!(Want::parse("hw:x"), Want::Name("hw:x".into()));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn open_reads_and_writes() {
    use std::io::Read;
    let root = scratch("both");
    let path = root.join("midiC1D0");
    fs::write(&path, [0x90, 0x5e, 0x7f]).unwrap();
    let (mut r, mut w) = open(&path).unwrap();
    let mut got = [0u8; 3];
    r.read_exact(&mut got).unwrap();
    assert_eq!(got, [0x90, 0x5e, 0x7f]);
    w.write_all(&[0x90, 0x5e, 0x00]).unwrap();
    assert_eq!(fs::read(&path).unwrap(), [0x90, 0x5e, 0x7f, 0x90, 0x5e, 0x00]);
    assert!(matches!(open(&root.join("nope")), Err(Refused::Missing)));
    assert!(!root.join("nope").exists(), "never created");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn refused_by_the_error() {
    let of = |e: i32| refused(io::Error::from_raw_os_error(e));
    assert!(matches!(of(libc::EBUSY), Refused::Busy));
    assert!(matches!(of(libc::EACCES), Refused::Denied));
    assert!(matches!(of(libc::EPERM), Refused::Denied));
    assert!(matches!(of(libc::ENODEV), Refused::Missing));
    assert!(matches!(of(libc::EIO), Refused::Other(_)));
}
