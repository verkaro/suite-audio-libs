//! The Clarett 2Pre as read on 2026-11-10, the meters on both fakes, and the interface set by
//! hand ([`crate::hand`]).

use crate::controls::Controls;
use crate::{
    CLARETT_2PRE, Change, Dest, Fake, Focusrite, MixError, Mixing, Panel, SCARLETT_18I20_GEN3, route, routes,
};

fn card(text: &str) -> (Fake, Focusrite) {
    let mut c = Fake::parse(text).unwrap();
    let f = Focusrite::read(&mut c).unwrap().expect("a Focusrite");
    (c, f)
}

fn two_pre() -> (Fake, Focusrite) {
    card(CLARETT_2PRE)
}

#[test]
fn the_2pre_is_what_it_has() {
    let (_, f) = two_pre();
    assert_eq!(f.inputs.len(), 2);
    for i in &f.inputs {
        assert!(i.inst.is_some() && i.air.is_some(), "INST and AIR on input {}", i.n);
        assert!(i.pad.is_none(), "no PAD");
    }
    assert!(f.phantom.is_empty() && f.phantom_persists.is_none(), "48V is the front panel's");
    assert_eq!(f.gains.len(), 10, "mixes A–J");
    assert_eq!(f.mixer_inputs.len(), 18);
    let outs: Vec<&str> = f.outputs.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(outs, ["LINE 1", "LINE 2", "LINE 3", "LINE 4"]);
    assert_eq!(f.capture_ids.len(), 12);
    assert_eq!(f.playbacks, 4);
    let labels: Vec<_> = f.lines.iter().map(|l| l.label.as_deref().unwrap()).collect();
    assert_eq!(labels, ["Monitor L", "Monitor R", "Headphones L", "Headphones R"]);
    assert!(f.lines.iter().all(|l| l.control.is_some() && l.volume.is_some() && l.mute.is_some()));
    assert!(f.talkback.is_none() && f.speaker_switching.is_none() && f.digital_mode.is_none());
    assert!(f.dim.is_some() && f.mute.is_some() && f.knob.is_some() && f.sync.is_some());
    assert!(f.standalone.is_some() && f.firmware.is_some());
    let clocks: Vec<&str> = f.validity.iter().map(|v| v.0.as_str()).collect();
    assert_eq!(clocks, ["Internal", "S/PDIF", "ADAT"]);
    let m = f.meter.expect("meters");
    assert_eq!((m.count, m.max), (34, 4095));
    assert_eq!(f.meter_dests.len(), 34, "4 outputs, 18 mixer inputs, 12 captures");
}

#[test]
fn the_18i20_has_its_meters_too() {
    let (_, f) = card(SCARLETT_18I20_GEN3);
    assert_eq!(f.meter.unwrap().count, 65);
    assert_eq!(f.meter_dests.len(), 65);
    assert_eq!(f.lines[6].label.as_deref(), Some("Headphones 1 L"));
    // The driver's order puts S/PDIF before ADAT; the outputs' list has ADAT first.
    let spdif = f.dest_named("S/PDIF 1").unwrap();
    let adat = f.dest_named("ADAT 1").unwrap();
    assert_eq!(f.meter_of(spdif), Some(10));
    assert_eq!(f.meter_of(adat), Some(12));
    assert_eq!(f.meter_of(Dest::MixerInput(1)), Some(20));
    assert_eq!(f.meter_of(Dest::Capture(1)), Some(45));
}

/// The driver's rule (`mixer_scarlett2.c`, `scarlett2_update_meter_level_map`): each value is
/// the level of the source feeding its destination, so one source reads the same everywhere it
/// goes, and a destination playing `Off` reads 0. Both cards' readings keep it, which is what
/// says the order is right.
#[test]
fn a_source_reads_the_same_everywhere_it_goes() {
    for text in [CLARETT_2PRE, SCARLETT_18I20_GEN3] {
        let (mut c, f) = card(text);
        let meters = f.read_meters(&mut c).unwrap();
        let playing = routes(&mut c, &f).unwrap();
        let mut seen = std::collections::BTreeMap::new();
        let mut lit = 0;
        for (d, &s) in f.dests().into_iter().zip(&playing) {
            let m = meters[f.meter_of(d).unwrap()];
            if s == f.off() {
                assert_eq!(m, 0, "{} plays Off", f.dest_name(d));
                continue;
            }
            assert_eq!(*seen.entry(s).or_insert(m), m, "{} at {}", f.sources[s], f.dest_name(d));
            lit += (m > 0) as usize;
        }
        assert!(lit > 4, "there was signal when it was read");
    }
}

#[test]
fn a_meter_in_db() {
    let (_, f) = two_pre();
    let m = f.meter.unwrap();
    assert_eq!(m.db(4095), 0.0);
    assert!((m.db(2048) - -6.02).abs() < 0.01);
    assert_eq!(m.db(0), crate::METER_FLOOR_DB);
    assert!(m.full(4095) && !m.full(4094));
}

#[test]
fn the_lines_and_standalone() {
    let (c, f) = two_pre();
    let mut p = Panel::new(Box::new(c), f);
    let now = p.now().unwrap();
    assert_eq!(now.lines.len(), 4);
    assert_eq!(now.lines[0].label.as_deref(), Some("Monitor L"));
    assert_eq!(
        (now.lines[0].hw, now.lines[0].volume, now.lines[0].mute),
        (Some(false), Some(127), Some(false))
    );
    assert_eq!((now.standalone, now.firmware), (Some(true), Some(1246)));
    assert!(now.validity.iter().all(|v| v.1));

    p.set(&Change::LineVolume(3, 100)).unwrap();
    p.set(&Change::LineMute(4, true)).unwrap();
    p.set(&Change::Standalone(false)).unwrap();
    let now = p.now().unwrap();
    assert_eq!(now.lines[2].volume, Some(100));
    assert_eq!(now.lines[3].mute, Some(true));
    assert_eq!(now.standalone, Some(false));

    // On the knob, a line's volume is the knob's, as the driver has it.
    p.set(&Change::LineHw(3, true)).unwrap();
    assert_eq!(p.now().unwrap().lines[2].hw, Some(true));
    assert!(p.set(&Change::LineVolume(3, 90)).is_err(), "refused while on the knob");
    assert!(p.set(&Change::LineVolume(9, 90)).is_err(), "no line 9 on the 2Pre");
    assert!(p.set(&Change::Pad(1, true)).is_err(), "no PAD on the 2Pre");
}

#[test]
fn routes_by_hand() {
    let (mut c, f) = two_pre();
    let line3 = f.dest_named("line 3").unwrap();
    assert_eq!(f.dest_name(line3), "LINE 3");
    route(&mut c, &f, line3, f.source("PCM 1").unwrap()).unwrap();
    assert_eq!(c.get("Analogue Output 03 Playback Enum"), Some(&[f.source("PCM 1").unwrap() as i64][..]));
    route(&mut c, &f, Dest::Capture(5), f.source("ADAT 3").unwrap()).unwrap();
    assert_eq!(c.get("PCM 05 Capture Enum"), Some(&[f.source("ADAT 3").unwrap() as i64][..]));
    let mix = f.source("Mix A").unwrap();
    assert!(matches!(route(&mut c, &f, Dest::MixerInput(1), mix), Err(MixError::MixIntoMixer)));
    route(&mut c, &f, line3, mix).unwrap();
    assert_eq!(f.dest_named("MIXER 18"), Some(Dest::MixerInput(18)));
    assert_eq!(f.dest_named("CAPTURE 12"), Some(Dest::Capture(12)));
    assert_eq!(f.dest_named("MIXER 19"), None);
}

/// The 2Pre with every mixer input at `Off` and every gain off: a mixer nothing uses.
fn empty_mixer() -> (Fake, Focusrite, Mixing) {
    empty(CLARETT_2PRE)
}

fn empty(text: &str) -> (Fake, Focusrite, Mixing) {
    let (mut c, f) = card(text);
    for &numid in &f.mixer_inputs {
        c.write(numid, &[0]).unwrap();
    }
    for &numid in f.gains.iter().flatten() {
        c.write(numid, &[0]).unwrap();
    }
    c.writes.clear();
    let m = Mixing::read(&mut c, &f).unwrap();
    (c, f, m)
}

#[test]
fn a_source_shares_one_mixer_input_between_mixes() {
    let (mut c, f, mut m) = empty_mixer();
    let a1 = f.source("Analogue 1").unwrap();
    let pcm1 = f.source("PCM 1").unwrap();
    let unity = f.gain(Some(0.0));
    assert_eq!(m.add(&mut c, &f, a1, 0, unity).unwrap(), 0);
    assert_eq!(m.add(&mut c, &f, a1, 1, f.gain(Some(-6.0))).unwrap(), 0, "the same input");
    assert_eq!(m.add(&mut c, &f, pcm1, 0, unity).unwrap(), 1, "the next free one");
    assert_eq!(c.get("Mixer Input 01 Capture Enum"), Some(&[a1 as i64][..]));
    assert_eq!(c.get("Mixer Input 02 Capture Enum"), Some(&[pcm1 as i64][..]));
    assert_eq!(c.get("Mix B Input 01 Playback Volume"), Some(&[f.gain(Some(-6.0))][..]));
    let heard: Vec<usize> = m.heard(&f, 0).iter().map(|h| h.0).collect();
    assert_eq!(heard, [a1, pcm1]);
    assert!((f.gain_db(m.gain(&f, 1, a1).unwrap()).unwrap() - -6.0).abs() < 0.5);

    // Cut from one mix, it stays on its input for the other; cut from both, the input is freed.
    assert!(m.cut(&mut c, &f, a1, 0).unwrap());
    assert_eq!(c.get("Mixer Input 01 Capture Enum"), Some(&[a1 as i64][..]));
    assert!(!m.cut(&mut c, &f, a1, 0).unwrap(), "not in it now");
    assert!(m.cut(&mut c, &f, a1, 1).unwrap());
    assert_eq!(c.get("Mixer Input 01 Capture Enum"), Some(&[0][..]), "Off once no mix hears it");
    assert_eq!(m.gain(&f, 1, a1), None);
}

#[test]
fn only_cuts_the_rest() {
    let (mut c, f, mut m) = empty_mixer();
    let unity = f.gain(Some(0.0));
    let [a1, a2, pcm1] = ["Analogue 1", "Analogue 2", "PCM 1"].map(|s| f.source(s).unwrap());
    m.add(&mut c, &f, a1, 2, unity).unwrap();
    m.add(&mut c, &f, a2, 2, unity).unwrap();
    m.add(&mut c, &f, a2, 3, unity).unwrap();
    let cut = m.only(&mut c, &f, pcm1, 2, unity).unwrap();
    assert_eq!(cut, [a1, a2]);
    let heard: Vec<usize> = m.heard(&f, 2).iter().map(|h| h.0).collect();
    assert_eq!(heard, [pcm1]);
    assert_eq!(m.gain(&f, 3, a2), Some(unity), "the other mix keeps it");
    assert!(!m.inputs().contains(&a1), "Analogue 1's input freed");
}

/// The 2Pre has 16 sources the mixer can take and 18 inputs, so it never fills: the 18i20 has
/// 39 for 25.
#[test]
fn a_full_mixer_refuses_and_reuses_what_no_mix_hears() {
    let (_, f) = two_pre();
    assert_eq!((1..f.sources.len()).filter(|&s| !f.is_mix(s)).count(), 16);
    let (mut c, f, mut m) = empty(SCARLETT_18I20_GEN3);
    let unity = f.gain(Some(0.0));
    let sources: Vec<usize> =
        (1..f.sources.len()).filter(|&s| !f.is_mix(s)).take(f.mixer_inputs.len() + 1).collect();
    for &s in &sources[..25] {
        m.add(&mut c, &f, s, 0, unity).unwrap();
    }
    let last = *sources.last().unwrap();
    let e = m.add(&mut c, &f, last, 0, unity).unwrap_err();
    assert!(matches!(e, MixError::Full(25)));
    assert_eq!(e.to_string(), "the mixer's 25 inputs are all used");
    // An input whose source no mix hears is taken, even before it's set Off.
    m.set_gain(&mut c, &f, 0, 4, f.gain_raw.0).unwrap();
    assert_eq!(m.place(&f, last).unwrap(), (4, true));
    assert_eq!(m.add(&mut c, &f, last, 1, unity).unwrap(), 4);
    assert!(matches!(m.add(&mut c, &f, f.source("Mix C").unwrap(), 0, unity), Err(MixError::MixIntoMixer)));
    assert!(matches!(m.add(&mut c, &f, f.off(), 0, unity), Err(MixError::NoSource)));
    assert!(matches!(m.add(&mut c, &f, last, 12, unity), Err(MixError::NoMix)));
}

#[test]
fn writes_only_what_changes_and_all_of_it_again() {
    let (mut c, f, mut m) = empty_mixer();
    let unity = f.gain(Some(0.0));
    let a1 = f.source("Analogue 1").unwrap();
    m.add(&mut c, &f, a1, 0, unity).unwrap();
    let n = c.writes.len();
    m.add(&mut c, &f, a1, 0, unity).unwrap();
    assert_eq!(c.writes.len(), n, "nothing changed, nothing written");
    // The driver's copy scrambled, as by a panel beside it: written again whole.
    c.write(f.gains[0][0], &[0]).unwrap();
    assert_eq!(m.write_all(&mut c, &f).unwrap(), 18 + 10 * 18);
    assert_eq!(c.get("Mix A Input 01 Playback Volume"), Some(&[unity][..]));
    assert_eq!(Mixing::read(&mut c, &f).unwrap(), m);
}
