use super::*;
use std::fs;

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
