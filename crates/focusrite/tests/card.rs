//! On the interface itself, only with `SUITE_FOCUSRITE_TEST=1` (Warren allowed tests to write
//! to it, 2026-10-13): the first Focusrite found in `/dev/snd/by-id`. Every control written is
//! put back as it was.

use std::path::PathBuf;

use as_focusrite::{Card, Fake, dump};

fn wanted() -> bool {
    std::env::var("SUITE_FOCUSRITE_TEST").is_ok_and(|v| v == "1")
}

/// The first Focusrite's control device.
fn focusrite() -> Option<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir("/dev/snd/by-id")
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("usb-Focusrite")))
        .collect();
    v.sort();
    v.into_iter().next()
}

#[test]
fn reads_the_interface() {
    if !wanted() {
        return;
    }
    let path = focusrite().expect("no Focusrite in /dev/snd/by-id");
    let mut card = Card::open(&path).unwrap();
    let (name, driver) = card.name().unwrap();
    eprintln!("{}: {name} ({driver})", path.display());
    let text = dump(&mut card).unwrap();
    assert!(text.contains("\tMix A Input 01 Playback Volume\t"), "{text}");
    // BLESS=1 on an 18i20 3rd gen: its controls as the fake's.
    if std::env::var("BLESS").is_ok_and(|v| v == "1") && name == "Scarlett 18i20 USB" {
        let out = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/18i20-gen3.txt");
        std::fs::write(out, &text).unwrap();
    }
}

/// Warren's mixes on the 18i20 (`docs/monitor.md`), with nothing heard live (only the tape
/// returns, so a mic on an input isn't sent anywhere), then everything put back as found.
#[test]
fn sets_the_interface_and_puts_it_back() {
    use as_focusrite::{Controls, Focusrite, Levels, Mixer, Plan, Request, Want, keep_found, put_back};
    if !wanted() {
        return;
    }
    let path = focusrite().expect("no Focusrite in /dev/snd/by-id");
    let mut card = Card::open(&path).unwrap();
    if card.name().unwrap().0 != "Scarlett 18i20 USB" {
        eprintln!("not an 18i20: nothing written");
        return;
    }
    let scratch = std::env::temp_dir().join(format!("as-focusrite-card-{}", std::process::id()));
    let kept_at = scratch.join("found.txt");
    assert!(keep_found(&mut card, &kept_at).unwrap());
    let kept = std::fs::read_to_string(&kept_at).unwrap();
    let f = Focusrite::read(&mut card).unwrap().expect("a Focusrite");
    let want = |name: &str, hw: [&str; 2], deck: usize, follows: bool, control: bool| Want {
        name: name.into(),
        hw: hw.map(String::from).to_vec(),
        deck,
        follows,
        control,
        talkback: false,
    };
    let r = Request {
        mixes: vec![
            want("CONTROL ROOM", ["LINE 9", "LINE 10"], 0, false, true),
            want("MAIN", ["LINE 7", "LINE 8"], 0, true, false),
            want("AUX 1", ["LINE 1", "LINE 2"], 1, false, false),
            want("AUX 2", ["LINE 3", "LINE 4"], 2, false, false),
            want("AUX 3", ["LINE 5", "LINE 6"], 3, false, false),
        ],
        deck_mixes: 4,
        tracks: (1..=8).chain(12..=19).map(Some).collect(),
        direct_outs: vec![None; 16],
        speaker_switching: Some(false),
        ..Request::default()
    };
    let plan = Plan::new(&f, &r);
    eprintln!("notes: {:?}", plan.notes);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut m =
            Mixer::setup(Box::new(Card::open(&path).unwrap()), f.clone(), plan, &Levels::new(4, 16, 1))
                .unwrap();
        let c = m.controls();
        let get = |c: &mut dyn Controls, name: &str| {
            let k = c.list().unwrap().into_iter().find(|k| k.name == name).unwrap();
            (c.read(k.numid).unwrap()[0], k.writable)
        };
        assert_eq!(get(c, "Speaker Switching Playback Enum").0, 0);
        assert_eq!(get(c, "Line Out 01 Volume Control Playback Enum"), (0, true), "lines 1–4 off the knob");
        assert_eq!(get(c, "Line 01 (Monitor 1 L) Playback Volume"), (127, true));
        assert_eq!(get(c, "Mixer Input 18 Capture Enum").0, f.source("PCM 1").unwrap() as i64);
        assert_eq!(get(c, "Analogue Output 09 Playback Enum").0, f.source("Mix A").unwrap() as i64);
        assert_eq!(get(c, "Mix A Input 18 Playback Volume").0, 160);
        assert_eq!(get(c, "Mix C Input 18 Playback Volume").0, 160, "MAIN hears the control room's tape");
        assert_eq!(get(c, "Mix A Input 01 Playback Volume").0, 0, "nothing live");
    }));
    let mut card = Card::open(&path).unwrap();
    let n = put_back(&mut card, &kept).unwrap();
    eprintln!("{n} controls put back");
    let now = Fake::parse(&dump(&mut card).unwrap()).unwrap();
    let was = Fake::parse(&kept).unwrap();
    for line in kept.lines().filter(|l| !l.starts_with('#')) {
        let name = line.split('\t').nth(3).unwrap();
        assert_eq!(now.get(name), was.get(name), "{name} put back");
    }
    let _ = std::fs::remove_dir_all(&scratch);
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

/// Puts the interface back from a file of its controls as found
/// (Dexter keeps one in `~/.local/state/dexter/focusrite/SERIAL.txt`):
/// `SUITE_FOCUSRITE_PUT_BACK=PATH`, until a page has a key for it.
#[test]
fn puts_back_from_a_file() {
    use as_focusrite::put_back;
    let Ok(file) = std::env::var("SUITE_FOCUSRITE_PUT_BACK") else { return };
    let kept = std::fs::read_to_string(&file).unwrap();
    let path = focusrite().expect("no Focusrite in /dev/snd/by-id");
    let mut card = Card::open(&path).unwrap();
    let n = put_back(&mut card, &kept).unwrap();
    eprintln!("{n} controls put back from {file}");
    let now = Fake::parse(&dump(&mut card).unwrap()).unwrap();
    let was = Fake::parse(&kept).unwrap();
    for line in kept.lines().filter(|l| !l.starts_with('#')) {
        let name = line.split('\t').nth(3).unwrap();
        assert_eq!(now.get(name), was.get(name), "{name} put back");
    }
}

/// A rig's settings on the 18i20 (`docs/interface.md`): what channels 9–16 record (the OctoPre's
/// ADAT 1–8), AIR and PAD on input 3, INST on input 1, written as the plan says and read back,
/// then everything put back as found. Never 48V (a mic plugged in may not want it) nor the
/// clock (JACK runs on it): those are checked on the fake.
#[test]
fn sets_a_rigs_settings() {
    use as_focusrite::{Controls, Focusrite, Plan, Request, Settings, Switches, keep_found, put_back};
    if !wanted() {
        return;
    }
    let path = focusrite().expect("no Focusrite in /dev/snd/by-id");
    let mut card = Card::open(&path).unwrap();
    if card.name().unwrap().0 != "Scarlett 18i20 USB" {
        eprintln!("not an 18i20: nothing written");
        return;
    }
    let scratch = std::env::temp_dir().join(format!("as-focusrite-rig-{}", std::process::id()));
    let kept_at = scratch.join("found.txt");
    let _ = std::fs::remove_dir_all(&scratch);
    keep_found(&mut card, &kept_at).unwrap();
    let kept = std::fs::read_to_string(&kept_at).unwrap();
    let result = std::panic::catch_unwind(|| {
        let mut c = Card::open(&path).unwrap();
        let f = Focusrite::read(&mut c).unwrap().unwrap();
        let r = Request {
            settings: Settings {
                captures: (9..=16).map(|ch| (ch, format!("ADAT {}", ch - 8))).collect(),
                inputs: vec![
                    (1, Switches { inst: Some(true), ..Switches::default() }),
                    (3, Switches { air: Some(true), pad: Some(true), ..Switches::default() }),
                ],
                ..Settings::default()
            },
            ..Request::default()
        };
        let plan = Plan::new(&f, &r);
        assert!(plan.notes.is_empty(), "{:?}", plan.notes);
        assert_eq!(plan.settings.len(), 11);
        for s in &plan.settings {
            c.write(s.numid, &[s.value]).unwrap();
        }
        for s in &plan.settings {
            assert_eq!(c.read(s.numid).unwrap()[0], s.value, "{}", s.what);
        }
        let now = Focusrite::read(&mut c).unwrap().unwrap();
        assert_eq!(now.sources[now.captures[8]], "ADAT 1");
        assert_eq!(now.sources[now.captures[15]], "ADAT 8");
    });
    let mut card = Card::open(&path).unwrap();
    let n = put_back(&mut card, &kept).unwrap();
    eprintln!("{n} controls put back");
    let now = Fake::parse(&dump(&mut card).unwrap()).unwrap();
    let was = Fake::parse(&kept).unwrap();
    for name in [
        "PCM 09 Capture Enum",
        "PCM 16 Capture Enum",
        "Line In 3 Air Capture Switch",
        "Line In 1 Level Capture Enum",
    ] {
        assert_eq!(now.get(name), was.get(name), "{name} put back");
    }
    let _ = std::fs::remove_dir_all(&scratch);
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}
