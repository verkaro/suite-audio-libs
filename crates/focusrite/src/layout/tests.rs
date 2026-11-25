//! The four layouts on the two fake rigs, as read from Warren's: the 18i20 on 2026-10-13, the
//! Clarett+ 8Pre on 2026-11-17.

use super::*;
use crate::{CLARETT_PLUS_8PRE, Fake, SCARLETT_18I20_GEN3};

fn rig(r: Rig) -> (Fake, Focusrite) {
    let text = match r {
        Rig::Scarlett18i20 => SCARLETT_18I20_GEN3,
        Rig::ClarettPlus8Pre => CLARETT_PLUS_8PRE,
    };
    let mut c = Fake::parse(text).unwrap();
    let f = Focusrite::read(&mut c).unwrap().unwrap();
    (c, f)
}

/// What a destination plays now, by name.
fn plays(c: &mut Fake, f: &Focusrite, dest: &str) -> String {
    let d = f.dest_named(dest).unwrap_or_else(|| panic!("no {dest}"));
    let s = c.read(f.dest_numid(d).unwrap()).unwrap()[0] as usize;
    f.sources[s].clone()
}

const RIGS: [Rig; 2] = [Rig::Scarlett18i20, Rig::ClarettPlus8Pre];
const WAYS: [Way; 2] = [Way::Internal, Way::External];

#[test]
fn every_layout_fits_its_rig_and_is_set_as_laid_out() {
    for r in RIGS {
        for w in WAYS {
            let (mut c, f) = rig(r);
            let l = Layout::of(r, w);
            l.apply(&mut c, &f).unwrap_or_else(|e| panic!("{r:?} {w:?}: {e}"));
            for (d, s) in &l.routes {
                assert_eq!(&plays(&mut c, &f, d), s, "{r:?} {w:?}: {d}");
            }
        }
    }
}

#[test]
fn written_twice_the_second_writes_nothing() {
    for r in RIGS {
        for w in WAYS {
            let (mut c, f) = rig(r);
            let l = Layout::of(r, w);
            assert!(!l.apply(&mut c, &f).unwrap().is_empty());
            assert_eq!(l.apply(&mut c, &f).unwrap(), vec![], "{r:?} {w:?}");
        }
    }
}

#[test]
fn no_line_or_adat_output_ever_carries_the_loop_or_another_test_channel() {
    for r in RIGS {
        for w in WAYS {
            let l = Layout::of(r, w);
            for (d, s) in &l.routes {
                if s == LOOP_SOURCE {
                    assert_eq!(d, LOOP_OUT, "{r:?} {w:?}: the loop's channel only to S/PDIF 1");
                }
                if d.starts_with("LINE ") || d.starts_with("ADAT ") {
                    assert!(
                        !RETURN.contains(&s.as_str()) && s != LOOP_SOURCE && s != "PCM 19",
                        "{r:?} {w:?}: {d} plays {s}"
                    );
                }
            }
            assert!(l.routes.iter().any(|(d, s)| d == LOOP_OUT && s == LOOP_SOURCE));
        }
    }
}

#[test]
fn the_sixteen_are_recorded_where_the_rig_records_them() {
    let (mut c, f) = rig(Rig::ClarettPlus8Pre);
    Layout::of(Rig::ClarettPlus8Pre, Way::Internal).apply(&mut c, &f).unwrap();
    assert_eq!(plays(&mut c, &f, "CAPTURE 1"), "Analogue 1");
    assert_eq!(plays(&mut c, &f, "CAPTURE 8"), "Analogue 8");
    assert_eq!(plays(&mut c, &f, "CAPTURE 9"), "Mix A");
    assert_eq!(plays(&mut c, &f, "CAPTURE 10"), "Mix B");
    assert_eq!(plays(&mut c, &f, "CAPTURE 11"), "ADAT 1");
    assert_eq!(plays(&mut c, &f, "CAPTURE 18"), "ADAT 8");
    // capstan's own figures for the 8Pre, from 0: tracks 0-7,10-17 and the mix on 8,9.
    let from0: Vec<usize> = Rig::ClarettPlus8Pre.track_captures().iter().map(|c| c - 1).collect();
    assert_eq!(from0, [0, 1, 2, 3, 4, 5, 6, 7, 10, 11, 12, 13, 14, 15, 16, 17]);
    assert_eq!(Rig::ClarettPlus8Pre.mix_captures().map(|c| c - 1), [8, 9]);

    let (mut c, f) = rig(Rig::Scarlett18i20);
    Layout::of(Rig::Scarlett18i20, Way::External).apply(&mut c, &f).unwrap();
    assert_eq!(plays(&mut c, &f, "CAPTURE 9"), "ADAT 1");
    assert_eq!(plays(&mut c, &f, "CAPTURE 16"), "ADAT 8");
    assert_eq!(plays(&mut c, &f, "CAPTURE 17"), "S/PDIF 1");
    assert_eq!(plays(&mut c, &f, "CAPTURE 19"), "Mix A");
    assert_eq!(plays(&mut c, &f, "CAPTURE 20"), "Mix B");
    // capstan's 18i20 default: tracks 0-15, the mix on 18,19.
    assert_eq!(Rig::Scarlett18i20.mix_captures().map(|c| c - 1), [18, 19]);
}

#[test]
fn external_sends_each_input_to_its_out_and_internal_the_mixes_to_the_lines() {
    for r in RIGS {
        let (mut c, f) = rig(r);
        Layout::of(r, Way::External).apply(&mut c, &f).unwrap();
        for n in 1..=8 {
            assert_eq!(plays(&mut c, &f, &format!("LINE {n}")), format!("Analogue {n}"), "{r:?}");
            assert_eq!(plays(&mut c, &f, &format!("ADAT {n}")), format!("ADAT {n}"), "{r:?}");
        }
        assert_eq!(plays(&mut c, &f, "LINE 9"), "Mix A");
        assert_eq!(plays(&mut c, &f, "LINE 10"), "Mix B");

        Layout::of(r, Way::Internal).apply(&mut c, &f).unwrap();
        for (line, mix) in [(1, "A"), (2, "B"), (7, "C"), (8, "D"), (9, "E"), (10, "F")] {
            assert_eq!(plays(&mut c, &f, &format!("LINE {line}")), format!("Mix {mix}"), "{r:?}");
        }
        for n in 3..=6 {
            assert_eq!(plays(&mut c, &f, &format!("LINE {n}")), "Off", "{r:?}");
        }
        for n in 1..=8 {
            assert_eq!(plays(&mut c, &f, &format!("ADAT {n}")), format!("PCM {}", n + 10), "{r:?}");
        }
    }
}

#[test]
fn the_return_comes_into_each_monitors_left_and_right_at_0_db_and_nowhere_else() {
    for r in RIGS {
        for w in WAYS {
            let (mut c, f) = rig(r);
            let l = Layout::of(r, w);
            l.apply(&mut c, &f).unwrap();
            let mixing = Mixing::read(&mut c, &f).unwrap();
            let (pl, pr) = (f.source("PCM 1").unwrap(), f.source("PCM 2").unwrap());
            for m in 0..f.gains.len() {
                let left = l.monitors.iter().any(|mon| mon.pair[0] == m);
                let right = l.monitors.iter().any(|mon| mon.pair[1] == m);
                let db = |s| mixing.gain(&f, m, s).and_then(|g| f.gain_db(g));
                assert_eq!(db(pl).map(f64::round), left.then_some(0.0), "{r:?} {w:?} mix {m} left");
                assert_eq!(db(pr).map(f64::round), right.then_some(0.0), "{r:?} {w:?} mix {m} right");
            }
        }
    }
}

#[test]
fn a_sources_gains_go_with_it_to_its_new_mixer_input() {
    let (mut c, f) = rig(Rig::ClarettPlus8Pre);
    // The box as found on 2026-11-17: MIXER 7 plays Analogue 3, heard in Mix A at 0 dB.
    let a3 = f.source("Analogue 3").unwrap();
    let before = Mixing::read(&mut c, &f).unwrap();
    assert_eq!(before.inputs()[6], a3, "the test data's MIXER 7");
    let was_in_a = before.gain(&f, 0, a3);
    assert!(was_in_a.is_some());
    Layout::of(Rig::ClarettPlus8Pre, Way::Internal).apply(&mut c, &f).unwrap();
    let after = Mixing::read(&mut c, &f).unwrap();
    assert_eq!(after.inputs()[2], a3, "Analogue 3 on MIXER 3");
    assert_eq!(after.gain(&f, 0, a3), was_in_a, "its gain in Mix A came with it");
    // A source not heard before isn't heard after: ADAT 1 wasn't in any mix.
    let adat1 = f.source("ADAT 1").unwrap();
    assert!((0..f.gains.len()).all(|m| after.gain(&f, m, adat1).is_none()));
}

#[test]
fn the_18i20s_talkback_input_is_left_as_found() {
    let (mut c, f) = rig(Rig::Scarlett18i20);
    let numid = f.mixer_inputs[24];
    let before = c.read(numid).unwrap();
    for w in WAYS {
        Layout::of(Rig::Scarlett18i20, w).apply(&mut c, &f).unwrap();
        assert_eq!(c.read(numid).unwrap(), before, "{w:?}");
    }
}

#[test]
fn each_channels_tape_has_a_playback_channel_of_its_own() {
    let l = Layout::of(Rig::ClarettPlus8Pre, Way::External);
    assert_eq!(l.tape_of(1), Some(("LINE 1", "PCM 3")));
    assert_eq!(l.tape_of(8), Some(("LINE 8", "PCM 10")));
    assert_eq!(l.tape_of(9), Some(("ADAT 1", "PCM 11")));
    assert_eq!(l.tape_of(16), Some(("ADAT 8", "PCM 18")));
    assert_eq!(l.tape_of(17), None);
    let mut tapes: Vec<&str> = l.channels.iter().map(|c| c.tape.as_str()).collect();
    tapes.sort();
    tapes.dedup();
    assert_eq!(tapes.len(), 16);
    assert!(tapes.iter().all(|t| !RETURN.contains(t) && *t != LOOP_SOURCE));
}

#[test]
fn a_rig_is_known_by_its_cards_name() {
    assert_eq!(Rig::named("Scarlett 18i20 USB"), Some(Rig::Scarlett18i20));
    assert_eq!(Rig::named("Clarett+ 8Pre"), Some(Rig::ClarettPlus8Pre));
    assert_eq!(Rig::named("Clarett 2Pre USB"), None);
    assert_eq!(Way::named("External"), Some(Way::External));
    assert_eq!(Way::named("as found"), None);
}

#[test]
fn a_layout_on_an_interface_without_its_parts_says_what_is_missing() {
    let mut c = Fake::parse(crate::CLARETT_2PRE).unwrap();
    let f = Focusrite::read(&mut c).unwrap().unwrap();
    let now = routes(&mut c, &f).unwrap();
    let mixing = Mixing::read(&mut c, &f).unwrap();
    let e = Layout::of(Rig::ClarettPlus8Pre, Way::Internal).writes(&f, &now, &mixing).unwrap_err();
    assert!(e.0.iter().any(|m| m == "no LINE 7"), "{e}");
}
