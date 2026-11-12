//! On the fake 18i20, as Warren's was read on 2026-10-13, with his mixes (`docs/monitor.md`).

use std::sync::{Arc, Mutex};

use crate::controls::Controls;
use crate::{
    Fake, Focusrite, Levels, Link, Mixer, Plan, Request, SCARLETT_18I20_GEN3, Settings, Switches, Want,
    Watch, keep_found, put_back,
};

fn fake() -> Fake {
    Fake::parse(SCARLETT_18I20_GEN3).unwrap()
}

/// A fake shared with the test, to look at while a mixer has it.
#[derive(Clone)]
struct Shared(Arc<Mutex<Fake>>);

impl Controls for Shared {
    fn list(&mut self) -> std::io::Result<Vec<crate::Control>> {
        self.0.lock().unwrap().list()
    }
    fn read(&mut self, numid: u32) -> std::io::Result<Vec<i64>> {
        self.0.lock().unwrap().read(numid)
    }
    fn write(&mut self, numid: u32, values: &[i64]) -> std::io::Result<()> {
        self.0.lock().unwrap().write(numid, values)
    }
}

impl Shared {
    fn get(&self, name: &str) -> i64 {
        self.0.lock().unwrap().get(name).unwrap_or_else(|| panic!("no control {name}"))[0]
    }
}

fn want(name: &str, hw: [&str; 2], deck: usize, follows: bool, control: bool) -> Want {
    Want { name: name.into(), hw: hw.map(String::from).to_vec(), deck, follows, control, talkback: false }
}

/// r with its last track (ADAT 8) gone, so the mixer has an input left for another (S/PDIF 1,
/// the 16th): Warren's fills all 25 with the talkback's.
fn one_spare(mut r: Request) -> Request {
    r.tracks.pop();
    r.direct_outs.pop();
    r
}

/// Warren's 18i20 (2026-10-13): CONTROL ROOM on headphones 2, MAIN following it on headphones
/// 1, three aux mixes out to the floor on lines 1–6; 16 tracks, the analogue inputs and the
/// OctoPre's on ADAT (capture channels 1–8 and 12–19).
fn warrens(speaker_switching: Option<bool>) -> Request {
    Request {
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
        speaker_switching,
        ..Request::default()
    }
}

#[test]
fn finds_the_18i20() {
    let f = Focusrite::read(&mut fake()).unwrap().unwrap();
    assert_eq!(f.mixer_inputs.len(), 25);
    assert_eq!(f.gains.len(), 12);
    assert_eq!(f.playbacks, 20);
    let names: Vec<&str> = f.outputs.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(names[..3], ["LINE 1", "LINE 2", "LINE 3"]);
    assert_eq!(names[10..12], ["ADAT 1", "ADAT 2"]);
    assert_eq!(names.len(), 20);
    assert_eq!(f.output("line 9").map(|o| o.source), f.source("PCM 9"));
    // Capture channels record Analogue 1–9, S/PDIF 1–2, ADAT 1–8; the 20th nothing.
    let rec: Vec<&str> = f.captures.iter().map(|&s| f.sources[s].as_str()).collect();
    assert_eq!(
        (rec[0], rec[8], rec[9], rec[11], rec[18], rec[19]),
        ("Analogue 1", "Analogue 9", "S/PDIF 1", "ADAT 1", "ADAT 8", "Off")
    );
    // −80 to +12 dB over 0 to 184.
    assert_eq!(
        (f.gain(None), f.gain(Some(-80.0)), f.gain(Some(0.0)), f.gain(Some(-6.0)), f.gain(Some(20.0))),
        (0, 0, 160, 148, 184)
    );
    assert_eq!(f.lines.len(), 10);
    assert!(f.speaker_switching.is_some());
    // Not a Focusrite: no mixer.
    let mut plain = Fake::parse("1\tMIXER\t0\tMaster Playback Volume\trw\tint\t1\t0,127,1\t100\n").unwrap();
    assert_eq!(Focusrite::read(&mut plain).unwrap(), None);
}

#[test]
fn plans_warrens_mixes() {
    let f = Focusrite::read(&mut fake()).unwrap().unwrap();
    let p = Plan::new(&f, &warrens(Some(false)));
    assert_eq!(p.notes, Vec::<String>::new());
    let pairs: Vec<(&str, [usize; 2], usize, bool)> =
        p.mixes.iter().map(|m| (m.name.as_str(), m.pair, m.deck, m.solo)).collect();
    assert_eq!(
        pairs,
        [
            ("CONTROL ROOM", [0, 1], 0, true),
            ("MAIN", [2, 3], 0, false),
            ("AUX 1", [4, 5], 1, false),
            ("AUX 2", [6, 7], 2, false),
            ("AUX 3", [8, 9], 3, false)
        ]
    );
    let routed = |out: &str| {
        let o = f.output(out).unwrap();
        let s = p.outputs.iter().find(|x| x.0 == o.numid).map(|x| x.1);
        s.map(|s| f.sources[s].clone())
    };
    assert_eq!(routed("LINE 9").as_deref(), Some("Mix A"));
    assert_eq!(routed("LINE 10").as_deref(), Some("Mix B"));
    assert_eq!(routed("LINE 8").as_deref(), Some("Mix D"));
    assert_eq!(routed("LINE 1").as_deref(), Some("Mix E"));
    assert_eq!(routed("LINE 6").as_deref(), Some("Mix J"));
    assert_eq!(routed("ADAT 1"), None, "outputs no mix names are left as they are");
    // The aux mixes' lines come off the knob; the control room's and MAIN's stay.
    assert_eq!(p.sw_lines, [1, 2, 3, 4, 5, 6]);
    // The tape returns on PCM 1–8 (S/PDIF and ADAT out play 11–20), at the mixer's last inputs.
    let returns: Vec<_> = p.returns.iter().map(|r| r.map(|r| (r.playback, r.inputs))).collect();
    assert_eq!(
        returns,
        [
            Some(([1, 2], [16, 17])),
            Some(([3, 4], [18, 19])),
            Some(([5, 6], [20, 21])),
            Some(([7, 8], [22, 23]))
        ]
    );
    // The 16 tracks' inputs first, then the tape returns: every input used. The last is the
    // talkback's, which the interface's own talkback plays: Analogue 9, the mic, never a strip.
    let src: Vec<&str> = p.mixer_inputs.iter().map(|&s| f.sources[s].as_str()).collect();
    assert_eq!(&src[..2], ["Analogue 1", "Analogue 2"]);
    assert_eq!(&src[8..10], ["ADAT 1", "ADAT 2"]);
    assert_eq!(&src[15..18], ["ADAT 8", "PCM 1", "PCM 2"]);
    assert_eq!(&src[23..], ["PCM 8", "Analogue 9"]);
    assert_eq!(p.tracks, (0..16).map(Some).collect::<Vec<_>>());
    assert!(p.others.is_empty());
    // The rig's layout (`mixer_inputs:`, a new 18i20 rig's): the same, each input in its place.
    let mut laid = warrens(None);
    laid.settings.mixer_inputs = (1..=8)
        .map(|n| (n, format!("Analogue {n}")))
        .chain((1..=8).map(|n| (n + 8, format!("ADAT {n}"))))
        .collect();
    let q = Plan::new(&f, &laid);
    assert_eq!(
        (&q.mixer_inputs, &q.tracks, &q.returns, &q.others),
        (&p.mixer_inputs, &p.tracks, &p.returns, &p.others)
    );
    assert!(q.notes.iter().all(|n| !n.contains("mixer")), "{:?}", q.notes);
}

/// A rig's `mixer_inputs:` that asks for what can't be: each said and left out, the tape
/// returns on the inputs left, a track whose input isn't there not heard live, and an input no
/// track records one of the other inputs.
#[test]
fn the_rigs_mixer_inputs() {
    let f = Focusrite::read(&mut fake()).unwrap().unwrap();
    let mut r = warrens(None);
    r.settings.loopback = Some(("S/PDIF 1".into(), "S/PDIF 1".into()));
    r.settings.mixer_inputs = (1..=8)
        .map(|n| (n, format!("Analogue {n}")))
        .chain((1..=7).map(|n| (n + 8, format!("ADAT {n}"))))
        .chain([
            (16, "S/PDIF 1".to_string()),
            (17, "s/pdif 2".into()),
            (18, "Analogue 9".into()),
            (19, "Mix A".into()),
            (25, "ADAT 8".into()),
            (26, "ADAT 8".into()),
        ])
        .collect();
    let p = Plan::new(&f, &r);
    for note in [
        "the rig's mixer input 16: S/PDIF 1 is the loop's: it's never heard",
        "the rig's mixer input 18: Analogue 9 is the talkback mic: it's heard only through TALK",
        "the rig's mixer input 19: Mix A isn't one of the interface's inputs",
        "the rig's mixer input 25: it's the talkback's: ADAT 8 isn't heard there",
        "the rig's mixer input 26: the interface's mixer has 25 inputs",
        "track 16's input (ADAT 8) isn't in the rig's mixer: it isn't heard live",
    ] {
        assert!(p.notes.contains(&note.to_string()), "{note:?} in {:?}", p.notes);
    }
    assert_eq!(p.tracks, (0..15).map(Some).chain([None]).collect::<Vec<_>>());
    assert_eq!(p.others, [(16, "S/PDIF 2".to_string())]);
    let at: Vec<[usize; 2]> = p.returns.iter().flatten().map(|r| r.inputs).collect();
    assert_eq!(at, [[15, 17], [18, 19], [20, 21], [22, 23]]);
    assert_eq!(p.mixer_inputs[24], f.source("Analogue 9").unwrap());
}

/// A rig's `outputs:` (the Clarett's DAs on ADAT, Warren, 2026-10-21): ADAT 1-8 play PCM 13-20
/// whatever the box was left as, so tracks 1-8's direct outs on playback 13-20 reach them; the
/// tape returns keep off those channels; an output a mix has, or the interface hasn't, is said.
#[test]
fn the_rigs_outputs() {
    let card = Shared(Arc::new(Mutex::new(fake())));
    let f = Focusrite::read(&mut card.clone()).unwrap().unwrap();
    // Left with ADAT 1 playing AUX 1's mix (Warren's, found 2026-10-21).
    let adat1 = f.output("ADAT 1").unwrap().numid;
    card.0.lock().unwrap().write(adat1, &[f.source("Mix E").unwrap() as i64]).unwrap();
    let mut r = warrens(Some(false));
    r.direct_outs = (13..=20).map(Some).chain([None; 8]).collect();
    r.settings.outputs = (1..=8)
        .map(|n| (format!("ADAT {n}"), format!("PCM {}", n + 12)))
        .chain([("LINE 1".to_string(), "PCM 1".to_string()), ("LINE 99".into(), "PCM 1".into())])
        .collect();
    let p = Plan::new(&f, &r);
    for note in [
        "the rig's output LINE 1: it's AUX 1's",
        "the rig's output LINE 99: the interface has no output LINE 99",
    ] {
        assert!(p.notes.contains(&note.to_string()), "{note:?} in {:?}", p.notes);
    }
    assert!(p.notes.iter().all(|n| !n.contains("direct out")), "{:?}", p.notes);
    assert!(p.cut_outs.is_empty());
    let returns: Vec<[usize; 2]> = p.returns.iter().flatten().map(|r| r.playback).collect();
    assert_eq!(returns, [[1, 2], [3, 4], [5, 6], [7, 8]]);
    // With the loop on S/PDIF 1 (playing PCM 20), ADAT 8 can't play it.
    let mut looped = r.clone();
    looped.settings.loopback = Some(("S/PDIF 1".into(), "S/PDIF 1".into()));
    let q = Plan::new(&f, &looped);
    let note = "the rig's output ADAT 8: PCM 20 is the loop's: its bursts aren't to be heard";
    assert!(q.notes.contains(&note.to_string()), "{:?}", q.notes);
    Mixer::setup(Box::new(card.clone()), f.clone(), p, &Levels::new(4, 16, 0)).unwrap();
    for n in 1..=8 {
        assert_eq!(
            card.get(&format!("ADAT Output {n} Playback Enum")),
            f.source(&format!("PCM {}", n + 12)).unwrap() as i64
        );
    }
    assert_eq!(card.get("Analogue Output 01 Playback Enum"), f.source("Mix E").unwrap() as i64, "AUX 1's");
}

/// A new rig's mixes for the 18i20 (`dexter_project::Rig::seed_known`): CONTROL ROOM on LINE 7
/// and 8, MAINS following it on 9 and 10, AUX 1 to 3 on lines 1–6, with the S/PDIF loop.
#[test]
fn plans_a_new_18i20_rigs_mixes() {
    let f = Focusrite::read(&mut fake()).unwrap().unwrap();
    let mut r = warrens(Some(false));
    r.mixes[0].hw = vec!["LINE 7".into(), "LINE 8".into()];
    r.mixes[1] = want("MAINS", ["LINE 9", "LINE 10"], 0, true, false);
    r.settings.loopback = Some(("S/PDIF 1".into(), "S/PDIF 1".into()));
    let p = Plan::new(&f, &r);
    assert_eq!(p.notes, Vec::<String>::new());
    let routed = |out: &str| {
        let o = f.output(out).unwrap();
        let s = p.outputs.iter().find(|x| x.0 == o.numid).map(|x| x.1);
        s.map(|s| f.sources[s].clone()).unwrap_or_default()
    };
    let lines: Vec<String> = (1..=10).map(|n| routed(&format!("LINE {n}"))).collect();
    assert_eq!(
        lines,
        ["Mix E", "Mix F", "Mix G", "Mix H", "Mix I", "Mix J", "Mix A", "Mix B", "Mix C", "Mix D"]
    );
    let solo: Vec<(&str, usize, bool)> = p.mixes.iter().map(|m| (m.name.as_str(), m.deck, m.solo)).collect();
    assert_eq!(solo[..2], [("CONTROL ROOM", 0, true), ("MAINS", 0, false)]);
    assert_eq!(p.loopback.as_ref().map(|l| (l.playback, l.capture)), Some((20, 20)));
    assert_eq!(p.tracks, (0..16).map(Some).collect::<Vec<_>>(), "every track heard live");
}

#[test]
fn says_what_doesnt_fit() {
    let f = Focusrite::read(&mut fake()).unwrap().unwrap();
    // Speaker switching left on: the aux mixes on lines 1–4 stay under the knob.
    let p = Plan::new(&f, &warrens(None));
    assert!(
        p.notes
            .iter()
            .any(|n| n == "AUX 1: LINE 1 stays under the monitor knob and ALT while speaker switching is on"),
        "{:?}",
        p.notes
    );
    // Eight mixes of their own: 16 returns leave 9 inputs for 16 tracks; and no mix for the
    // seventh. Track 20 records capture channel 20, which is off.
    let mut r = warrens(Some(false));
    let outs = ["LINE 1", "LINE 3", "LINE 5", "LINE 7", "LINE 9", "ADAT 1", "ADAT 3"];
    r.mixes = (outs.iter().enumerate())
        .map(|(k, o)| {
            let n: usize = o.rsplit(' ').next().unwrap().parse().unwrap();
            let right = format!("{} {}", o.rsplit_once(' ').unwrap().0, n + 1);
            want(&format!("M{k}"), [o, &right], k, false, k == 0)
        })
        .collect();
    r.mixes[1].hw.extend(["LINE 1".to_string(), "LINE 2".to_string()]);
    r.deck_mixes = 7;
    r.tracks.push(Some(20));
    let p = Plan::new(&f, &r);
    assert!(p.notes.contains(&"M6: the interface has no mix left for it".to_string()), "{:?}", p.notes);
    assert!(p.notes.contains(&"LINE 1 is M0's: M1 doesn't reach it".to_string()), "{:?}", p.notes);
    assert!(!p.notes.iter().any(|n| n.contains("tape return")), "{:?}", p.notes);
    assert!(
        p.notes.contains(&"track 17's capture channel 20 records nothing: its source is off".to_string())
    );
    // Six mixes, 12 returns, and the talkback's: 12 inputs for the tracks.
    assert!(
        p.notes.contains(&"track 13's input (ADAT 5) isn't in the mixer: it isn't heard live".to_string()),
        "{:?}",
        p.notes
    );
    assert!(!p.notes.iter().any(|n| n.starts_with("track 12's input")));
    assert_eq!(p.tracks.iter().flatten().count(), 12);
    assert!(p.others.is_empty());
}

/// Warren's synth project (2026-10-13): 16 tracks on capture channels 1–16, each with a direct
/// out on playback 1–16. Every pair a tape return could have is a direct out's: the tape takes
/// 1–8, and those direct outs aren't connected; 9 and 10 reach nothing now (LINE 9 and 10
/// play the control room); 11–16 still reach S/PDIF and ADAT out.
#[test]
fn tape_returns_and_direct_outs() {
    let f = Focusrite::read(&mut fake()).unwrap().unwrap();
    let mut r = warrens(Some(false));
    r.tracks = (1..=16).map(Some).collect();
    r.direct_outs = (1..=16).map(Some).collect();
    let p = Plan::new(&f, &r);
    let playback: Vec<_> = p.returns.iter().map(|r| r.map(|r| r.playback)).collect();
    assert_eq!(playback, [Some([1, 2]), Some([3, 4]), Some([5, 6]), Some([7, 8])]);
    assert_eq!(p.cut_outs, (0..8).collect::<Vec<_>>());
    assert!(
        p.notes.contains(
            &"track 1's direct out (playback 1) carries CONTROL ROOM's tape return: it isn't connected"
                .into()
        ),
        "{:?}",
        p.notes
    );
    assert!(p.notes.contains(
        &"track 8's direct out (playback 8) carries AUX 3's tape return: it isn't connected".into()
    ));
    assert!(p.notes.contains(&"track 9's direct out (playback 9) reaches no output".into()));
    assert!(!p.notes.iter().any(|n| n.starts_with("track 11's")), "{:?}", p.notes);
    // With direct outs only on 1–4, the tape keeps clear of them.
    r.direct_outs = (1..=4).map(Some).chain(std::iter::repeat(None)).take(16).collect();
    let p = Plan::new(&f, &r);
    let playback: Vec<_> = p.returns.iter().map(|r| r.map(|r| r.playback)).collect();
    assert_eq!(playback, [Some([5, 6]), Some([7, 8]), Some([9, 10]), Some([1, 2])]);
    assert_eq!(p.cut_outs, [0, 1]);
}

/// No mix on the interface: its mixer and outputs are left as found, and a track whose capture
/// channel records nothing is still said.
#[test]
fn no_mix_on_the_interface() {
    let mut card = fake();
    card.set("PCM 02 Capture Enum", &[0]);
    let f = Focusrite::read(&mut card).unwrap().unwrap();
    let r = Request { deck_mixes: 1, tracks: vec![Some(1), Some(2)], ..Request::default() };
    let p = Plan::new(&f, &r);
    assert!(p.mixes.is_empty() && p.outputs.is_empty() && p.mixer_inputs.is_empty() && p.others.is_empty());
    assert_eq!((&p.tracks[..], &p.returns[..]), (&[None, None][..], &[None][..]));
    assert_eq!(p.notes, ["track 2's capture channel 2 records nothing: its source is off"]);
    let shared = Shared(Arc::new(Mutex::new(card)));
    let values = |c: &mut Shared| {
        let list = c.list().unwrap();
        list.iter().map(|k| (k.name.clone(), c.read(k.numid).unwrap())).collect::<Vec<_>>()
    };
    let before = values(&mut shared.clone());
    Mixer::setup(Box::new(shared.clone()), f, p, &Levels::new(1, 2, 0)).unwrap();
    assert!(values(&mut shared.clone()) == before, "nothing written");
}

#[test]
fn sets_the_interface_and_follows_the_deck() {
    let card = Shared(Arc::new(Mutex::new(fake())));
    let f = Focusrite::read(&mut card.clone()).unwrap().unwrap();
    let plan = Plan::new(&f, &warrens(Some(false)));
    let mut lv = Levels::new(4, 16, 1);
    let mut m = Mixer::setup(Box::new(card.clone()), f.clone(), plan, &lv).unwrap();
    // Speaker switching off, so lines 1–4 could be taken off the knob.
    assert_eq!(card.get("Speaker Switching Playback Enum"), 0);
    assert_eq!(card.get("Line Out 01 Volume Control Playback Enum"), 0);
    assert_eq!(card.get("Line 01 (Monitor 1 L) Playback Volume"), 127);
    assert_eq!(card.get("Line Out 09 Volume Control Playback Enum"), 0, "as found");
    assert_eq!(card.get("Mixer Input 17 Capture Enum"), f.source("PCM 1").unwrap() as i64);
    assert_eq!(card.get("Mixer Input 25 Capture Enum"), f.source("Analogue 9").unwrap() as i64);
    assert_eq!(card.get("Analogue Output 09 Playback Enum"), f.source("Mix A").unwrap() as i64);
    assert_eq!(card.get("ADAT Output 1 Playback Enum"), f.source("PCM 13").unwrap() as i64);
    // Nothing live: only the tape returns, into their own mixes, and MAIN hears the control room's.
    assert_eq!(card.get("Mix A Input 17 Playback Volume"), 160);
    assert_eq!(card.get("Mix B Input 18 Playback Volume"), 160);
    assert_eq!(card.get("Mix A Input 18 Playback Volume"), 0);
    assert_eq!(card.get("Mix C Input 17 Playback Volume"), 160);
    assert_eq!(card.get("Mix E Input 19 Playback Volume"), 160);
    assert_eq!(card.get("Mix E Input 17 Playback Volume"), 0);
    // The talkback's input off in every mix: the interface's own talkback lets it in.
    for mix in "ABCDEFGHIJ".chars() {
        assert_eq!(card.get(&format!("Mix {mix} Input 25 Playback Volume")), 0, "Mix {mix}");
    }
    assert_eq!(card.get("Mix A Input 01 Playback Volume"), 0, "the gain found (160) is off");
    // Track 1 armed, centred at 0 dB in every mix (−3 dB a side), with −12 dB of headroom.
    let g = 0.5f32.sqrt() * 0.25;
    for mix in &mut lv.gains {
        mix[0] = (g, g);
        mix[1] = (g, 0.0);
    }
    lv.live[0] = true;
    m.apply(&lv).unwrap();
    assert_eq!(card.get("Mix A Input 01 Playback Volume"), 130, "−15 dB");
    assert_eq!(card.get("Mix B Input 01 Playback Volume"), 130);
    assert_eq!(card.get("Mix J Input 01 Playback Volume"), 130);
    assert_eq!(card.get("Mix A Input 02 Playback Volume"), 0, "track 2 is on tape");
    // MIX LEVEL +3 dB.
    lv.mix_db = 3.0;
    m.apply(&lv).unwrap();
    assert_eq!(card.get("Mix A Input 01 Playback Volume"), 136);
    // Track 2 live, hard left; SOLO on track 2: the control room hears it alone, MAIN both.
    lv.live[1] = true;
    lv.soloed[1] = true;
    m.apply(&lv).unwrap();
    assert_eq!(card.get("Mix A Input 01 Playback Volume"), 0);
    assert_eq!(card.get("Mix A Input 02 Playback Volume"), 136);
    assert_eq!(card.get("Mix B Input 02 Playback Volume"), 0, "hard left");
    assert_eq!(card.get("Mix C Input 01 Playback Volume"), 136);
    assert_eq!(card.get("Mix E Input 01 Playback Volume"), 136, "an aux mix never hears SOLO");
    // Muted: off everywhere.
    lv.muted[1] = true;
    lv.soloed[1] = false;
    let n = m.apply(&lv).unwrap();
    assert_eq!(card.get("Mix E Input 02 Playback Volume"), 0);
    assert_eq!(card.get("Mix A Input 01 Playback Volume"), 136);
    assert!(n > 0 && m.apply(&lv).unwrap() == 0, "only what changes is written");
}

#[test]
fn mono_sums_the_control_room_and_main() {
    let card = Shared(Arc::new(Mutex::new(fake())));
    let f = Focusrite::read(&mut card.clone()).unwrap().unwrap();
    let plan = Plan::new(&f, &one_spare(warrens(Some(false))));
    let mut lv = Levels::new(4, 16, 1);
    let mut m = Mixer::setup(Box::new(card.clone()), f.clone(), plan, &lv).unwrap();
    // Track 1 live hard left at −12 dB, track 2 centred, S/PDIF 1 hard right, in every mix.
    let c = 0.5f32.sqrt() * 0.25;
    for k in 0..4 {
        (lv.gains[k][0], lv.gains[k][1], lv.inputs[k][0]) = ((0.25, 0.0), (c, c), (0.0, 0.25));
    }
    lv.live[..2].fill(true);
    m.apply(&lv).unwrap();
    let get = |mix: &str, input: u32| card.get(&format!("Mix {mix} Input {input:02} Playback Volume"));
    assert_eq!((get("A", 1), get("B", 1)), (136, 0));
    // MONO on the control room's engine mix: CONTROL ROOM (A, B) and MAIN (C, D), each input at
    // the mean of its sides on both, a hard-panned one 6 dB down and a centred one as it was.
    lv.mono = Some(0);
    m.apply(&lv).unwrap();
    let half = f.gain(Some(20.0 * 0.125f64.log10()));
    for mix in ["A", "B", "C", "D"] {
        assert_eq!((get(mix, 1), get(mix, 2), get(mix, 16)), (half, 130, half), "Mix {mix}");
    }
    assert_eq!(
        (get("E", 1), get("F", 1), get("E", 16), get("F", 16)),
        (136, 0, 0, 136),
        "an aux stays stereo"
    );
    assert_eq!(get("A", 17), 160, "the tape return comes in as it was: the callback made it mono");
    lv.mono = None;
    m.apply(&lv).unwrap();
    assert_eq!((get("A", 1), get("B", 1), get("C", 16), get("D", 16)), (136, 0, 0, 136));
}

#[test]
fn the_link_follows_on_its_thread() {
    let card = Shared(Arc::new(Mutex::new(fake())));
    let f = Focusrite::read(&mut card.clone()).unwrap().unwrap();
    let plan = Plan::new(&f, &warrens(Some(false)));
    let lv = Levels::new(4, 16, 1);
    let m = Mixer::setup(Box::new(card.clone()), f, plan, &lv).unwrap();
    let link = Link::start(m, lv, Box::new(|w| panic!("{w}")));
    link.update(|l| {
        l.gains[1][3] = (1.0, 1.0);
        l.live[3] = true;
    });
    link.settle();
    assert_eq!(card.get("Mix E Input 04 Playback Volume"), 160);
    assert_eq!(card.get("Mix A Input 04 Playback Volume"), 0, "no level in the control room's");
    link.update(|l| l.live[3] = false);
    link.settle();
    assert_eq!(card.get("Mix E Input 04 Playback Volume"), 0);
}

#[test]
fn keeps_the_interface_as_found_and_puts_it_back() {
    let dir = std::env::temp_dir().join(format!("as-focusrite-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("focusrite").join("P9121K7348C24B.txt");
    let card = Shared(Arc::new(Mutex::new(fake())));
    let before = crate::dump(&mut card.clone()).unwrap();
    assert!(keep_found(&mut card.clone(), &path).unwrap());
    let f = Focusrite::read(&mut card.clone()).unwrap().unwrap();
    let plan = Plan::new(&f, &warrens(Some(false)));
    drop(Mixer::setup(Box::new(card.clone()), f, plan, &Levels::new(4, 16, 1)).unwrap());
    assert_ne!(crate::dump(&mut card.clone()).unwrap(), before);
    // Kept once: a second start leaves it as it was first found.
    assert!(!keep_found(&mut card.clone(), &path).unwrap());
    let kept = std::fs::read_to_string(&path).unwrap();
    let rows: Vec<Vec<&str>> =
        kept.lines().filter(|l| !l.starts_with('#')).map(|l| l.split('\t').collect()).collect();
    assert!(rows.iter().all(|r| r[4] == "rw"));
    assert!(rows.iter().any(|r| r[3] == "Mix A Input 01 Playback Volume"));
    assert!(put_back(&mut card.clone(), &kept).unwrap() > 0);
    for r in &rows {
        assert_eq!(card.get(r[3]).to_string(), r[8], "{}", r[3]);
    }
    // Speaker switching on again: lines 1–4 the knob's.
    assert_eq!(card.get("Line Out 01 Volume Control Playback Enum"), 1);
    assert!(!card.0.lock().unwrap().named("Line Out 01 Volume Control Playback Enum").unwrap().writable);
    let _ = std::fs::remove_dir_all(&dir);
}

/// SOLO let go and an input muted at once: the control room's mix has the input off, on both
/// sides (found live on the 18i20, 2026-10-13: the left stayed up).
#[test]
fn solo_off_then_mute_at_once() {
    let card = Shared(Arc::new(Mutex::new(fake())));
    let f = Focusrite::read(&mut card.clone()).unwrap().unwrap();
    let plan = Plan::new(&f, &one_spare(warrens(Some(false))));
    let input = plan.others[0].0 + 1;
    let mut lv = Levels::new(4, 16, 1);
    lv.inputs[0][0] = (0.7, 0.7);
    let m = Mixer::setup(Box::new(card.clone()), f, plan, &lv).unwrap();
    let link = Link::start(m, lv, Box::new(|w| panic!("{w}")));
    let a = format!("Mix A Input {input:02} Playback Volume");
    let b = format!("Mix B Input {input:02} Playback Volume");
    link.settle();
    assert_eq!((card.get(&a), card.get(&b)), (154, 154));
    link.update(|l| l.soloed[0] = true);
    link.settle();
    assert_eq!((card.get(&a), card.get(&b)), (0, 0));
    link.update(|l| l.soloed[0] = false);
    link.update(|l| l.inputs_muted[0] = true);
    link.settle();
    assert_eq!((card.get(&a), card.get(&b)), (0, 0));
}

/// A Focusrite panel beside the mixer: warned of while it runs, and when it goes every mix is
/// written again whole, so a gain it left wrong is put right.
#[test]
fn a_panel_watched_for() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let card = Shared(Arc::new(Mutex::new(fake())));
    let f = Focusrite::read(&mut card.clone()).unwrap().unwrap();
    let plan = Plan::new(&f, &one_spare(warrens(Some(false))));
    let input = plan.others[0].0 + 1;
    let mut lv = Levels::new(4, 16, 1);
    lv.inputs[0][0] = (0.7, 0.7);
    let m = Mixer::setup(Box::new(card.clone()), f, plan, &lv).unwrap();
    let open = Arc::new(AtomicBool::new(false));
    let said = Arc::new(Mutex::new(Vec::<String>::new()));
    let (o, w, s) = (open.clone(), said.clone(), said.clone());
    let watch = Watch {
        every: std::time::Duration::from_millis(5),
        check: Box::new(move || o.load(Ordering::Relaxed).then(|| "alsa-scarlett-gui".to_string())),
        say: Box::new(move |t| s.lock().unwrap().push(t)),
    };
    let link = Link::watching(m, lv, Box::new(move |t| w.lock().unwrap().push(t)), Some(watch));
    let wait = |want: Option<&str>| {
        let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while link.panel().0.as_deref() != want {
            assert!(std::time::Instant::now() < end, "the panel never seen as {want:?}");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    };
    let a = format!("Mix A Input {input:02} Playback Volume");
    link.settle();
    assert_eq!(card.get(&a), 154);
    open.store(true, Ordering::Relaxed);
    wait(Some("alsa-scarlett-gui"));
    assert_eq!(link.panel().1, 1);
    // What it scrambles, as the driver's copy would send it.
    card.0.lock().unwrap().set(&a, &[60]);
    open.store(false, Ordering::Relaxed);
    wait(None);
    let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while said.lock().unwrap().len() < 2 {
        assert!(std::time::Instant::now() < end, "{:?}", said.lock().unwrap());
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(card.get(&a), 154, "written again");
    assert_eq!(
        *said.lock().unwrap(),
        [
            "alsa-scarlett-gui is open: while it runs, the interface's mixes can reach it wrong; close it",
            "alsa-scarlett-gui closed: the interface's mixes written again",
        ]
    );
    assert_eq!(link.panel().1, 2);
}

/// A rig's settings (`docs/interface.md`) on the 18i20: written before the mixes, what's
/// recorded changes what's heard live, and what the interface hasn't is said.
#[test]
fn a_rigs_settings() {
    let card = Shared(Arc::new(Mutex::new(fake())));
    let f = Focusrite::read(&mut card.clone()).unwrap().unwrap();
    assert_eq!(f.inputs.len(), 8);
    assert_eq!(
        (f.inputs[0].inst.is_some(), f.inputs[2].inst.is_some(), f.inputs[7].air.is_some()),
        (true, false, true)
    );
    assert_eq!(f.phantom.iter().map(|p| p.0.as_str()).collect::<Vec<_>>(), ["1-4", "5-8"]);
    assert!(f.clock.is_some() && f.sync.is_some() && f.phantom_persists.is_some());
    let mut r = warrens(Some(false));
    r.tracks = (1..=16).map(Some).collect();
    r.direct_outs = vec![None; 16];
    r.settings = Settings {
        clock: Some("adat".into()),
        captures: (9..=16)
            .map(|c| (c, format!("ADAT {}", c - 8)))
            .chain([(25, "ADAT 1".into()), (20, "Wordclock".into())])
            .collect(),
        mixer_inputs: Vec::new(),
        outputs: Vec::new(),
        inputs: vec![
            (1, Switches { inst: Some(true), ..Switches::default() }),
            (3, Switches { air: Some(true), pad: Some(false), inst: Some(true) }),
        ],
        phantom: vec![("1-4".into(), true), ("9-12".into(), true)],
        phantom_persists: Some(false),
        loopback: None,
        prints: Vec::new(),
    };
    let plan = Plan::new(&f, &r);
    let what: Vec<&str> = plan.settings.iter().map(|s| s.what.as_str()).collect();
    assert_eq!(what[0], "the clock: ADAT");
    assert_eq!(what[1], "capture 9 records ADAT 1");
    assert!(
        what.contains(&"input 3 AIR on") && what.contains(&"input 1 INST on") && what.contains(&"48V 1-4 on"),
        "{what:?}"
    );
    for n in [
        "the rig's capture 25: the interface records 20 channels",
        "the rig's capture 20: Wordclock isn't one of the interface's sources",
        "the rig's input 3: the interface has no INST there",
        "the rig's 48V 9-12: the interface has no such switch",
    ] {
        assert!(plan.notes.iter().any(|x| x == n), "{n} in {:?}", plan.notes);
    }
    // Tracks 9–16 hear the OctoPre (ADAT 1–8) live now, where channel 9 was Analogue 9.
    assert_eq!(f.sources[plan.captures[8]], "ADAT 1");
    let heard: Vec<String> = plan.tracks[8..16]
        .iter()
        .map(|m| m.map_or("-".into(), |k| f.sources[plan.mixer_inputs[k]].clone()))
        .collect();
    assert_eq!(heard, (1..=8).map(|i| format!("ADAT {i}")).collect::<Vec<_>>());
    // Written, before anything else.
    let _m = Mixer::setup(Box::new(card.clone()), f.clone(), plan, &Levels::new(4, 16, 0)).unwrap();
    let item = |name: &str, want: &str| {
        let fake = card.0.lock().unwrap();
        let c = fake.named(name).unwrap().clone();
        let crate::Kind::Enum(items) = c.kind else { panic!("{name}") };
        drop(fake);
        items[card.get(name) as usize] == want
    };
    assert!(item("Clock Source Clock Source", "ADAT"));
    assert!(item("PCM 09 Capture Enum", "ADAT 1") && item("PCM 16 Capture Enum", "ADAT 8"));
    assert!(item("Line In 1 Level Capture Enum", "Inst"));
    assert_eq!(
        (
            card.get("Line In 3 Air Capture Switch"),
            card.get("Line In 3 Pad Capture Switch"),
            card.get("Line In 1-4 Phantom Power Capture Switch")
        ),
        (1, 0, 1)
    );
    assert_eq!(card.get("Phantom Power Persistence Capture Switch"), 0);
}

/// The panel (`docs/interface.md`): the 18i20's switches, 48V, clock, sync, DIM and MUTE read,
/// and set one at a time; what it hasn't is NotFound.
#[test]
fn the_panel() {
    use crate::{Change, Panel, SharedControls};
    let card = Shared(Arc::new(Mutex::new(fake())));
    let f = Focusrite::read(&mut card.clone()).unwrap().unwrap();
    let mut p = Panel::new(Box::new(SharedControls::new(Box::new(card.clone()))), f);
    let now = p.now().unwrap();
    assert_eq!(now.inputs.len(), 8);
    assert_eq!((now.inputs[0].inst, now.inputs[2].inst, now.inputs[2].air), (Some(false), None, Some(false)));
    assert_eq!(now.phantom, [("1-4".to_string(), false), ("5-8".to_string(), false)]);
    assert_eq!((now.clock.as_deref(), now.clocks.len(), now.sync), (Some("Internal"), 3, Some(true)));
    assert_eq!((now.dim, now.mute, now.persists), (Some(false), Some(false), Some(false)));
    assert_eq!(
        (now.captures.len(), now.captures[0].as_str(), now.captures[19].as_str()),
        (20, "Analogue 1", "Off")
    );
    p.set(&Change::Air(3, true)).unwrap();
    p.set(&Change::Inst(1, true)).unwrap();
    p.set(&Change::Phantom("5-8".into(), true)).unwrap();
    p.set(&Change::Dim(true)).unwrap();
    let now = p.now().unwrap();
    assert_eq!(
        (now.inputs[2].air, now.inputs[0].inst, now.phantom[1].1, now.dim),
        (Some(true), Some(true), true, Some(true))
    );
    assert_eq!(card.get("Line In 3 Air Capture Switch"), 1);
    assert_eq!(p.set(&Change::Air(9, true)).unwrap_err().kind(), std::io::ErrorKind::NotFound);
    assert_eq!(p.set(&Change::Inst(3, true)).unwrap_err().kind(), std::io::ErrorKind::NotFound);
    assert_eq!(
        p.set(&Change::Phantom("9-12".into(), true)).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
}

/// The talkback (`docs/screen.md`, "Talkback"): heard in the mixes that ask for it, changed at
/// run time, their switches on only while TALK is (a switch alone sends the mic); armed (`Off`)
/// when any mix hears it; switched on and off from the panel; every switch off at the end.
#[test]
fn the_talkback() {
    use crate::{Change, Panel, SharedControls};
    let card = Shared(Arc::new(Mutex::new(fake())));
    let f = Focusrite::read(&mut card.clone()).unwrap().unwrap();
    assert_eq!(f.talkback_mic.as_deref(), Some("Analogue 9"));
    let switch = |c: char| card.get(&format!("Talkback Mix {c} Playback Switch"));
    let mut r = warrens(Some(false));
    r.mixes[2].talkback = true;
    // Left on by an engine that stopped while TALK was: let go at the start, every switch off.
    card.0.lock().unwrap().write(f.talkback.unwrap(), &[2]).unwrap();
    card.0.lock().unwrap().write(f.talkback_mixes[0], &[1]).unwrap();
    let plan = Plan::new(&f, &r);
    let mut lv = Levels::new(4, 16, 1);
    let mut m = Mixer::setup(Box::new(card.clone()), f.clone(), plan.clone(), &lv).unwrap();
    assert_eq!(card.get("Talkback Playback Enum"), 1, "Off: the button works");
    assert_eq!((switch('E'), switch('F'), switch('A'), switch('C'), switch('G')), (0, 0, 0, 0, 0));
    // TALK: AUX 1's switches on, and the talkback's input (25, the mic) at TALK's level in it
    // alone; the rest dimmed 18 dB in AUX 1 and the control room's (and MAIN, following it),
    // the tape returns here; all as it was after.
    let mic = |c: char| card.get(&format!("Mix {c} Input 25 Playback Volume"));
    let tape = |c: char, n: u32| card.get(&format!("Mix {c} Input {n:02} Playback Volume"));
    let tapes = || (tape('A', 17), tape('B', 18), tape('C', 17), tape('E', 19), tape('F', 20), tape('G', 21));
    assert_eq!(card.get("Mixer Input 25 Capture Enum"), f.source("Analogue 9").unwrap() as i64);
    lv.talking = true;
    assert_eq!(m.apply(&lv).unwrap(), 10, "two switches, the mic in two mixes, six tape returns");
    assert_eq!((switch('E'), switch('F'), switch('A'), switch('C'), switch('G')), (1, 1, 0, 0, 0));
    assert_eq!((mic('E'), mic('F'), mic('A'), mic('C'), mic('G')), (160, 160, 0, 0, 0), "0 dB");
    assert_eq!(tapes(), (124, 124, 124, 124, 124, 160), "−18 dB but in AUX 2");
    lv.talk_db = -6.0;
    m.apply(&lv).unwrap();
    assert_eq!((mic('E'), mic('F')), (148, 148));
    lv.talking = false;
    m.apply(&lv).unwrap();
    assert_eq!((switch('E'), switch('F'), mic('E'), mic('F')), (0, 0, 0, 0));
    assert_eq!(tapes(), (160, 160, 160, 160, 160, 160));
    lv.talking = true;
    m.apply(&lv).unwrap();
    // AUX 2 hears it too, and AUX 1 no longer.
    lv.talkback.insert(2, true);
    lv.talkback.insert(1, false);
    assert_eq!(m.apply(&lv).unwrap(), 12, "four switches, the mic in four mixes, four tape returns");
    assert_eq!((switch('E'), switch('F'), switch('G'), switch('H')), (0, 0, 1, 1));
    assert_eq!((mic('E'), mic('F'), mic('G'), mic('H')), (0, 0, 148, 148));
    assert_eq!(tapes(), (124, 124, 124, 160, 160, 124), "the control room's still dimmed");
    // The control room's: never MAIN, which follows it.
    lv.talkback.insert(0, true);
    m.apply(&lv).unwrap();
    assert_eq!((switch('A'), switch('B'), switch('C'), switch('D')), (1, 1, 0, 0));
    assert_eq!((mic('A'), mic('B'), mic('C'), mic('D')), (148, 148, 0, 0));
    lv.talkback.insert(0, false);
    m.apply(&lv).unwrap();
    assert_eq!(m.apply(&lv).unwrap(), 0, "only what changes is written");
    // Disabled, with no mix hearing it: left so until one does.
    card.0.lock().unwrap().write(f.talkback.unwrap(), &[0]).unwrap();
    lv.talkback.insert(2, false);
    m.apply(&lv).unwrap();
    assert_eq!(card.get("Talkback Playback Enum"), 0);
    lv.talkback.insert(3, true);
    m.apply(&lv).unwrap();
    assert_eq!((card.get("Talkback Playback Enum"), switch('I'), switch('J')), (1, 1, 1));
    // The end: nowhere.
    m.talkback_off().unwrap();
    assert_eq!((switch('I'), switch('J')), (0, 0));
    // TALK, as the front panel's button.
    let mut p = Panel::new(Box::new(SharedControls::new(Box::new(card.clone()))), f);
    assert_eq!(p.now().unwrap().talk, Some(false));
    p.set(&Change::Talk(true)).unwrap();
    assert_eq!((card.get("Talkback Playback Enum"), p.now().unwrap().talk), (2, Some(true)));
    p.set(&Change::Talk(false)).unwrap();
    assert_eq!((card.get("Talkback Playback Enum"), p.now().unwrap().talk), (1, Some(false)));
}

/// The rig's loop (`docs/loopback.md`, "The rig's loop on a Focusrite"), S/PDIF out to in: on
/// the last channel each way, its output fed from that playback channel alone and that capture
/// channel recording its input; no tape return, direct out or track on its channels, and a
/// rig's capture there said.
#[test]
fn the_rigs_loop_is_kept_apart() {
    let card = Shared(Arc::new(Mutex::new(fake())));
    let f = Focusrite::read(&mut card.clone()).unwrap().unwrap();
    let mut r = warrens(Some(false));
    r.settings.loopback = Some(("S/PDIF 1".into(), "S/PDIF 1".into()));
    r.settings.captures = vec![(20, "Analogue 9".into())];
    r.tracks = (1..=8).chain(12..=19).chain([20]).map(Some).collect();
    r.direct_outs = vec![None; 17];
    r.direct_outs[16] = Some(20);
    let p = Plan::new(&f, &r);
    let l = p.loopback.clone().unwrap();
    assert_eq!((l.out.as_str(), l.input.as_str(), l.playback, l.capture), ("S/PDIF 1", "S/PDIF 1", 20, 20));
    assert!(p.returns.iter().flatten().all(|r| !r.playback.contains(&20)), "{:?}", p.returns);
    assert_eq!((&p.cut_ins[..], &p.cut_outs[..]), (&[16][..], &[16][..]));
    assert!(p.notes.contains(&"the rig's capture 20 is the loop's: Analogue 9 isn't recorded there".into()));
    assert!(
        p.notes.contains(
            &"track 17's input (capture 20) records S/PDIF 1, the loop's: it's left unpatched".into()
        ),
        "{:?}",
        p.notes
    );
    assert!(
        p.notes.contains(&"track 17's direct out (playback 20) is the loop's: it isn't connected".into())
    );
    assert!(!p.others.iter().any(|o| o.1 == "S/PDIF 1"), "never heard: {:?}", p.others);
    Mixer::setup(Box::new(card.clone()), f.clone(), p, &Levels::new(4, 17, 0)).unwrap();
    assert_eq!(card.get("S/PDIF Output 1 Playback Enum"), f.source("PCM 20").unwrap() as i64);
    assert_eq!(card.get("PCM 20 Capture Enum"), f.source("S/PDIF 1").unwrap() as i64);
    // At a rate whose USB stream carries 18 each way, it's on 18.
    let p = Plan::new(&f.clone().carrying(18, 18), &r);
    assert_eq!(p.loopback.map(|l| (l.playback, l.capture)), Some((18, 18)));
}

/// An analogue loop, LINE 6 into Analogue 8: the line off the knob at full level, AUX 3 kept off
/// it, and the track on capture 8 (Analogue 8 as found) left unpatched. An interface without
/// the loop's output says so, and there's no loop.
#[test]
fn an_analogue_loop() {
    let card = Shared(Arc::new(Mutex::new(fake())));
    let f = Focusrite::read(&mut card.clone()).unwrap().unwrap();
    let mut r = warrens(Some(false));
    r.settings.loopback = Some(("LINE 6".into(), "Analogue 8".into()));
    let p = Plan::new(&f, &r);
    assert!(p.notes.contains(&"LINE 6 is the loop's: AUX 3 doesn't reach it".into()), "{:?}", p.notes);
    assert!(p.sw_lines.contains(&6));
    assert_eq!(p.cut_ins, [7]);
    assert!(p.tracks[7].is_none());
    Mixer::setup(Box::new(card.clone()), f.clone(), p, &Levels::new(4, 16, 0)).unwrap();
    assert_eq!(card.get("Analogue Output 06 Playback Enum"), f.source("PCM 20").unwrap() as i64);
    assert_eq!(card.get("PCM 20 Capture Enum"), f.source("Analogue 8").unwrap() as i64);
    assert_eq!(card.get("Line Out 06 Volume Control Playback Enum"), 0);
    assert_eq!(card.get("Line 06 Playback Volume"), 127);
    r.settings.loopback = Some(("LINE 11".into(), "Analogue 8".into()));
    let p = Plan::new(&f, &r);
    assert!(
        p.loopback.is_none()
            && p.notes.contains(&"the rig's loop: the interface has no output LINE 11".into())
    );
}

/// Prints (`docs/monitor.md`, "Prints"): AUX 1 cabled from LINE 1 and 2 into Analogue 7 and 8,
/// tracks 7 and 8's. Armed, the two are heard live in every mix but AUX 1, whatever their
/// levels there; a chain of prints holds both inputs out of both mixes; an input that isn't a
/// track's is held the same way; an output playing a playback channel prints it.
#[test]
fn prints_are_never_heard_in_what_they_print() {
    let card = Shared(Arc::new(Mutex::new(fake())));
    let f = Focusrite::read(&mut card.clone()).unwrap().unwrap();
    let mut r = warrens(Some(false));
    r.settings.prints = vec![("LINE 1".into(), "Analogue 7".into()), ("LINE 2".into(), "analogue 8".into())];
    let p = Plan::new(&f, &r);
    assert_eq!(p.notes, Vec::<String>::new());
    let prints: Vec<(&str, &str, String)> =
        p.prints.iter().map(|x| (x.out.as_str(), x.input.as_str(), p.print_of(&f, x))).collect();
    assert_eq!(
        prints,
        [("LINE 1", "Analogue 7", "AUX 1 L".to_string()), ("LINE 2", "Analogue 8", "AUX 1 R".to_string())]
    );
    let held: Vec<&str> = p.mixes.iter().filter(|m| m.prints).map(|m| m.name.as_str()).collect();
    assert_eq!(held, ["AUX 1"]);
    let mut lv = Levels::new(4, 16, 1);
    let mut m = Mixer::setup(Box::new(card.clone()), f.clone(), p, &lv).unwrap();
    let get = |mix: &str, input: u32| card.get(&format!("Mix {mix} Input {input:02} Playback Volume"));
    // Tracks 7 and 8 armed (their inputs live), centred at −12 dB in every mix.
    let c = 0.5f32.sqrt() * 0.25;
    for mix in &mut lv.gains {
        (mix[6], mix[7]) = ((c, c), (c, c));
    }
    lv.live[6..8].fill(true);
    m.apply(&lv).unwrap();
    assert_eq!((get("E", 7), get("F", 7), get("E", 8), get("F", 8)), (0, 0, 0, 0), "never in AUX 1");
    assert_eq!((get("A", 7), get("B", 8), get("C", 7), get("G", 8)), (130, 130, 130, 130), "heard elsewhere");
    assert_eq!(get("E", 17), 0, "AUX 1's tape return is the control room's, not its own");
    assert_eq!(get("E", 19), 160, "and its own still comes in");

    // A chain: AUX 1 printed into Analogue 7, AUX 2 into Analogue 8. Each input is held out of
    // both mixes, or the two prints would feed each other round.
    r.settings.prints = vec![("LINE 1".into(), "Analogue 7".into()), ("LINE 3".into(), "Analogue 8".into())];
    let p = Plan::new(&f, &r);
    let held: Vec<&str> = p.mixes.iter().filter(|m| m.prints).map(|m| m.name.as_str()).collect();
    assert_eq!(held, ["AUX 1", "AUX 2"]);
    let mut m = Mixer::setup(Box::new(card.clone()), f.clone(), p, &lv).unwrap();
    m.apply(&lv).unwrap();
    assert_eq!((get("E", 7), get("E", 8), get("G", 7), get("G", 8)), (0, 0, 0, 0));
    assert_eq!((get("I", 7), get("I", 8)), (130, 130), "AUX 3 reaches no print");

    // S/PDIF 1, no track's (MIX's INPUTS), cabled from LINE 5: raised, it's heard but in AUX 3.
    r.settings.prints = vec![("LINE 5".into(), "S/PDIF 1".into())];
    let p = Plan::new(&f, &one_spare(r.clone()));
    assert_eq!(p.others, [(15, "S/PDIF 1".to_string())]);
    let mut m = Mixer::setup(Box::new(card.clone()), f.clone(), p, &lv).unwrap();
    for input in lv.inputs.iter_mut().take(4) {
        input[0] = (c, c);
    }
    m.apply(&lv).unwrap();
    assert_eq!((get("I", 16), get("J", 16)), (0, 0));
    assert_eq!((get("A", 16), get("E", 16)), (130, 130));

    // ADAT 1 out plays PCM 13 as found: a print of that playback channel, holding no mix.
    r.settings.prints = vec![("ADAT 1".into(), "S/PDIF 2".into())];
    let p = Plan::new(&f, &r);
    let x = &p.prints[0];
    assert_eq!((x.mix, x.playback, p.print_of(&f, x)), (None, Some(13), "PCM 13".to_string()));
    assert!(p.mixes.iter().all(|m| !m.prints));

    // What the interface hasn't, or the loop's cable, is said and left out.
    r.settings.prints = vec![("LINE 11".into(), "Analogue 7".into()), ("LINE 1".into(), "Mix A".into())];
    r.settings.loopback = Some(("LINE 6".into(), "Analogue 8".into()));
    r.settings.prints.push(("LINE 3".into(), "Analogue 8".into()));
    let p = Plan::new(&f, &r);
    assert!(p.prints.is_empty());
    for note in [
        "the rig's print from LINE 11: the interface has no output LINE 11",
        "the rig's print from LINE 1: Mix A isn't one of the interface's inputs",
        "the rig's print LINE 3 → Analogue 8 is on the loop's cable: it's left out",
    ] {
        assert!(p.notes.contains(&note.to_string()), "{note}: {:?}", p.notes);
    }
}
