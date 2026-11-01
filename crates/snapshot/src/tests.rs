//! A small table of two controls, as a tool would make one.

use super::*;

fn table() -> Table {
    let d =
        |key: &str, words: &str| Detent { key: key.into(), label: key.to_uppercase(), words: words.into() };
    Table {
        kind: "track".into(),
        version: "v1".into(),
        controls: vec![
            Control {
                key: "drive".into(),
                label: "Drive".into(),
                what: "how hard".into(),
                detents: vec![d("off", "none"), d("1", "light"), d("2", "firm")],
                default: "1".into(),
            },
            Control {
                key: "bias".into(),
                label: "Bias".into(),
                what: "character".into(),
                detents: vec![d("under", "edge"), d("nominal", "reference")],
                default: "nominal".into(),
            },
            Control {
                key: "hf".into(),
                label: "HF".into(),
                what: "top".into(),
                detents: vec![d("off", "full"), d("on", "gentle")],
                default: "on".into(),
            },
        ],
        collapse: vec![Collapse {
            control: "drive".into(),
            detent: "off".into(),
            ignored: vec!["bias".into()],
        }],
    }
}

const HOUSE: &str = r#"
schema   = 1
kind     = "track"
name     = "house"
note     = "Default"
detents  = "v1"

[controls]
drive    = "1"
bias     = "nominal"
hf       = "on"
"#;

#[test]
fn a_snapshot_reads_and_its_id_ignores_its_name() {
    let t = table();
    let s = Snapshot::parse(HOUSE, &[&t]).unwrap();
    assert_eq!(s, t.defaults("house").with_note("Default"));
    assert_eq!(
        s.canonical(&t),
        "controls.bias=nominal\ncontrols.drive=1\ncontrols.hf=on\ndetents=v1\nkind=track\nschema=1\n"
    );
    let renamed = Snapshot::parse(&HOUSE.replace("house", "warm").replace("Default", "x"), &[&t]).unwrap();
    assert_eq!(renamed.id(&t), s.id(&t));
    assert_eq!(s.id(&t).len(), 64);
    assert_eq!(s.short_id(&t), s.id(&t)[..8]);
    // Written back and read again, the same.
    assert_eq!(Snapshot::parse(&s.to_toml(&t), &[&t]).unwrap(), s);
}

#[test]
fn every_mistake_fails_loudly() {
    let t = table();
    let bad = |text: String, want: &str| {
        let e = Snapshot::parse(&text, &[&t]).unwrap_err();
        assert!(e.0.contains(want), "{want:?} not in {e}");
    };
    bad(HOUSE.replace("hf       = \"on\"\n", ""), "has no hf");
    bad(HOUSE.replace("hf       = \"on\"", "hf = \"on\"\nwow = \"some\""), "wow");
    bad(HOUSE.replace("note     = \"Default\"\n", ""), "note");
    bad(HOUSE.replace("drive    = \"1\"", "drive    = \"6\""), "isn't one of its detents");
    bad(HOUSE.replace("drive    = \"1\"", "drive    = 1"), "string");
    bad(HOUSE.replace("schema   = 1", "schema   = 2"), "schema 2");
    bad(HOUSE.replace("\"v1\"", "\"v9\""), "no detent table v9");
    bad(HOUSE.replace("\"track\"", "\"mixdown\""), "kind \"mixdown\"");
    bad(HOUSE.replace("name ", "nmae "), "nmae");
}

#[test]
fn a_collapsed_control_enters_the_id_as_a_dash() {
    let t = table();
    let off = |bias: &str| {
        let s = HOUSE
            .replace("drive    = \"1\"", "drive    = \"off\"")
            .replace("\"nominal\"", &format!("{bias:?}"));
        Snapshot::parse(&s, &[&t]).unwrap()
    };
    assert_eq!(off("under").id(&t), off("nominal").id(&t));
    assert!(off("under").canonical(&t).contains("controls.bias=-\n"));
    let on = Snapshot::parse(HOUSE, &[&t]).unwrap();
    let under = Snapshot::parse(&HOUSE.replace("\"nominal\"", "\"under\""), &[&t]).unwrap();
    assert_ne!(on.id(&t), under.id(&t), "with drive on, bias counts");
}

#[test]
fn every_combination_once() {
    let all = table().combinations();
    assert_eq!(all.len(), 3 * 2 * 2);
    assert_eq!(all[0]["drive"], "off");
    assert_eq!(all[11]["hf"], "on");
    let seen: std::collections::BTreeSet<_> = all.iter().collect();
    assert_eq!(seen.len(), 12);
}

impl Snapshot {
    fn with_note(mut self, note: &str) -> Snapshot {
        self.note = note.into();
        self
    }
}

#[test]
fn chained_collapses_give_one_sound_one_id() {
    let mut t = table();
    // drive off ignores bias; bias under ignores hf.
    t.collapse.push(Collapse { control: "bias".into(), detent: "under".into(), ignored: vec!["hf".into()] });
    let snap = |drive: &str, bias: &str, hf: &str| {
        let mut s = t.defaults("x");
        s.controls.insert("drive".into(), drive.into());
        s.controls.insert("bias".into(), bias.into());
        s.controls.insert("hf".into(), hf.into());
        s
    };
    // With drive off, bias does nothing, so neither does its collapse: hf counts.
    assert_eq!(snap("off", "under", "on").id(&t), snap("off", "nominal", "on").id(&t));
    assert_ne!(snap("off", "under", "on").id(&t), snap("off", "under", "off").id(&t));
    // With drive on and bias under, hf does nothing.
    assert_eq!(snap("1", "under", "on").id(&t), snap("1", "under", "off").id(&t));
}
