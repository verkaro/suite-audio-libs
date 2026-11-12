//! The sheet on the fake backend. Dexter's sheet tests, moved here with a graph of the same
//! shape written out (its `studio` stays Dexter's), its rules' own tests left with Dexter; a
//! `Rules` that refuses and one that warns; and a screen's own grouping (jackbay's SPEC 8.3)
//! on jackbay's `--sim` graph.

use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use super::*;
use crate::backend::{BackendError, Report};
use crate::fake::Fake;

/// A deck, as Dexter's graph is under `--sim`: the interface `sim`, two tracks patched from
/// its captures, the monitor to its playbacks, two direct outs, and two more clients.
const DECK: &str = "\
out sim:capture_1
out sim:capture_2
in sim:playback_1
in sim:playback_2
out dexter:monitor_L
out dexter:monitor_R
in dexter:in_01
in dexter:in_02
out dexter:out_01
out dexter:out_02
out metro:click
out midi metro:midi_out
in reverb:in_L
in reverb:in_R
out reverb:out_L
out reverb:out_R
link sim:capture_1 > dexter:in_01
link sim:capture_2 > dexter:in_02
link dexter:monitor_L > sim:playback_1
link dexter:monitor_R > sim:playback_2
";

/// jackbay's `--sim` (`jackbay/Sheet.md`): nothing connected.
const SIM: &str = "\
out system:capture_1
out system:capture_2
in system:playback_1
in system:playback_2
out scape:out_L
out scape:out_R
out brr:out_1
out brr:out_2
";

/// The fake, kept in reach of the test once the sheet has it.
#[derive(Clone)]
struct Shared(Arc<Mutex<Fake>>);

impl Backend for Shared {
    fn snapshot(&mut self) -> Result<Snapshot, BackendError> {
        self.0.lock().unwrap().snapshot()
    }
    fn run(&mut self, snap: &Snapshot, plan: &Plan) -> Report {
        self.0.lock().unwrap().run(snap, plan)
    }
    fn generation(&self) -> u64 {
        self.0.lock().unwrap().generation()
    }
}

fn shared(fixture: &str) -> Shared {
    Shared(Arc::new(Mutex::new(Fake::from_fixture(fixture).unwrap())))
}

/// A deck's rules, as Dexter's are in shape: its own client and the interface's shown, its
/// own first; and for the tests, ports refused, warnings asked and said, a log.
#[derive(Default)]
struct Deck {
    refuse: Vec<String>,
    asks: Vec<String>,
    says: Vec<String>,
    reserved: Vec<String>,
    log: RefCell<Vec<(String, bool)>>,
}

impl Rules for Deck {
    fn check(&self, plan: &Plan, snap: &Snapshot) -> Result<(), Refused> {
        let links = plan.links_named(snap);
        match links.iter().find(|l| self.refuse.contains(&l.to)) {
            Some(l) => Err(Refused::Rule { kind: "armed_input".into(), text: format!("{} is armed", l.to) }),
            None => Ok(()),
        }
    }

    /// A link made into one of `asks` asks; into one of `says` is said.
    fn warnings(&self, now: &Snapshot, after: &Snapshot) -> Vec<Warning> {
        let new = |port: &String| {
            let (Some(a), n) = (after.port_named(port), now.port_named(port)) else { return false };
            after.links_to(a).count() > n.map_or(0, |n| now.links_to(n).count())
        };
        let asks =
            self.asks.iter().filter(|p| new(p)).map(|p| Warning { text: format!("{p} hears it"), ask: true });
        let says =
            self.says.iter().filter(|p| new(p)).map(|p| Warning { text: format!("{p} sounds"), ask: false });
        asks.chain(says).collect()
    }

    fn shown(&self, client: &str) -> bool {
        client == "dexter" || client == "sim"
    }

    fn reserved(&self) -> Vec<String> {
        self.reserved.clone()
    }

    fn favourites(&self) -> Vec<Pattern> {
        vec![Pattern::new("dexter")]
    }

    fn log(&self, text: &str, warn: bool) {
        self.log.borrow_mut().push((text.to_string(), warn));
    }
}

/// Dexter's mono bay over [`DECK`], its tracks and the monitor named.
fn deck(rules: Deck) -> (Sheet<Deck>, Shared) {
    let backend = shared(DECK);
    let mut sheet = Sheet::with_rules(Box::new(backend.clone()), Grouping::Mono, rules);
    for stmt in [
        "bundle \"CONTROL ROOM L\" = dexter[out 1]",
        "bundle \"CONTROL ROOM R\" = dexter[out 2]",
        "bundle \"1) Vln 1\" = dexter[in 1]",
        "bundle \"2) Vln 2\" = dexter[in 2]",
        "bundle \"1) Vln 1 out\" = dexter[out 3]",
        "bundle \"2) Vln 2 out\" = dexter[out 4]",
    ] {
        sheet.define(stmt).unwrap();
    }
    (sheet, backend)
}

fn all() -> Op {
    Op::View { all: true, zoom: None, grouping: None }
}

fn at(m: &Matrix, row: &str, col: &str) -> (usize, usize) {
    let find = |v: &[Axis], name: &str| {
        v.iter()
            .position(|a| a.label == name || a.first == name)
            .unwrap_or_else(|| panic!("no {name}: {v:?}"))
    };
    (find(&m.rows, row), find(&m.cols, col))
}

fn cell(m: &Matrix, row: &str, col: &str) -> String {
    let (r, c) = at(m, row, col);
    m.cell(r, c).unwrap().to_string()
}

fn key(m: &Matrix, row: &str, col: &str) -> CellKey {
    let (r, c) = at(m, row, col);
    m.key(r, c).unwrap()
}

fn toggle<R: Rules>(
    s: &mut Sheet<R>,
    id: ScreenId,
    m: &Matrix,
    row: &str,
    col: &str,
) -> Result<Matrix, Refused> {
    s.op(id, Op::Toggle { cell: key(m, row, col) })
}

fn stage<R: Rules>(
    s: &mut Sheet<R>,
    m: &Matrix,
    row: &str,
    col: &str,
    verb: Verb,
) -> Result<Matrix, Refused> {
    s.op(0, Op::Stage { cell: key(m, row, col), verb })
}

fn texts(m: &Matrix) -> Vec<(Mark, &str)> {
    m.staged.iter().map(|l| (l.mark, l.text.as_str())).collect()
}

fn words(m: &Matrix) -> Vec<&str> {
    m.staged.iter().map(|l| l.words.as_str()).collect()
}

fn link(from: &str, to: &str, add: bool) -> NamedLink {
    NamedLink { from: from.into(), to: to.into(), add }
}

fn linked(b: &Shared, from: &str, to: &str) -> bool {
    let snap = b.0.lock().unwrap().snapshot().unwrap();
    let (Some(f), Some(t)) = (snap.port_named(from), snap.port_named(to)) else { return false };
    snap.links_from(f).any(|x| x == t)
}

/// The tool's view (its client and the interface, its own first) and every client's.
#[test]
fn a_view_by_the_rules() {
    let (mut s, _) = deck(Deck::default());
    let m = s.op(0, Op::Show).unwrap();
    let cols: Vec<(&str, &str, bool)> =
        m.cols.iter().map(|a| (a.client.as_str(), a.label.as_str(), a.declared)).collect();
    assert_eq!(
        cols,
        [
            ("dexter", "1) Vln 1", true),
            ("dexter", "2) Vln 2", true),
            ("sim", "playback_1", false),
            ("sim", "playback_2", false)
        ]
    );
    let rows: Vec<&str> = m.rows.iter().map(|a| a.label.as_str()).collect();
    assert_eq!(
        rows,
        ["CONTROL ROOM L", "CONTROL ROOM R", "1) Vln 1 out", "2) Vln 2 out", "capture_1", "capture_2"]
    );
    assert!(m.rows.iter().chain(&m.cols).all(|a| a.size == 1), "mono");
    assert_eq!((m.grouping, m.all), (Grouping::Mono, false));
    assert_eq!(m.hidden, 6, "reverb's two ins and two outs, the click and the MIDI out");
    let wide = s.op(0, all()).unwrap();
    assert!(wide.all && wide.hidden == 0 && wide.rows.iter().any(|a| a.client == "metro" && a.midi));
    assert!(m.problems.is_empty() && m.warnings.is_empty(), "{:?}", m.problems);
}

#[test]
fn stage_apply_undo() {
    let (mut s, b) = deck(Deck::default());
    let m = s.op(0, all()).unwrap();
    assert_eq!(cell(&m, "sim:capture_1", "1) Vln 1"), "##=s");
    assert_eq!(cell(&m, "metro:click", "2) Vln 2"), "..=s");

    let count = s.count();
    let m = toggle(&mut s, 0, &m, "metro:click", "2) Vln 2").unwrap();
    assert_eq!(cell(&m, "metro:click", "2) Vln 2"), ".#+u");
    assert_eq!(texts(&m), [(Mark::Add, "metro > \"2) Vln 2\"")]);
    assert_eq!(words(&m), ["metro click TO 2) Vln 2"], "a client not shown by its name too");
    assert_eq!(m.staged[0].links, [link("metro:click", "dexter:in_02", true)]);
    assert_eq!(s.count(), count, "staging moves nothing for other screens");

    // Another screen sees the graph, not the first one's staging.
    let other = s.op(1, all()).unwrap();
    assert_eq!(cell(&other, "metro:click", "2) Vln 2"), "..=s");
    assert!(other.staged.is_empty());

    let m = s.op(0, Op::Apply { anyway: false }).unwrap();
    assert_eq!(cell(&m, "metro:click", "2) Vln 2"), "##=s");
    assert!(m.staged.is_empty() && m.can_undo && s.count() > count);
    assert!(linked(&b, "metro:click", "dexter:in_02"));
    let other = s.op(1, Op::Show).unwrap();
    assert_eq!(cell(&other, "metro:click", "2) Vln 2"), "##=s");

    // Undo is shared: the other screen undoes it.
    let other = s.op(1, Op::Undo).unwrap();
    assert_eq!(cell(&other, "metro:click", "2) Vln 2"), "..=s");
    assert!(!other.can_undo);
    assert_eq!(s.op(1, Op::Undo).unwrap_err(), Refused::Other("nothing to undo".into()));

    // A tap on a staged cell unstages; DROP drops the rest.
    let m = toggle(&mut s, 0, &m, "metro:click", "1) Vln 1").unwrap();
    let m = toggle(&mut s, 0, &m, "metro:click", "1) Vln 1").unwrap();
    assert!(m.staged.is_empty());
    let m = toggle(&mut s, 0, &m, "sim:capture_1", "1) Vln 1").unwrap();
    assert_eq!(cell(&m, "sim:capture_1", "1) Vln 1"), "#.-u");
    let m = s.op(0, Op::Drop).unwrap();
    assert!(m.staged.is_empty());
    assert_eq!(cell(&m, "sim:capture_1", "1) Vln 1"), "##=s");
}

#[test]
fn bays_refusals() {
    let (mut s, _) = deck(Deck::default());
    let m = s.op(0, all()).unwrap();
    let e = toggle(&mut s, 0, &m, "metro:midi_out", "1) Vln 1").unwrap_err();
    assert!(matches!(&e, Refused::Patch { kind, .. } if kind == "type_mismatch"), "{e:?}");
    assert_eq!(&cell(&m, "metro:midi_out", "1) Vln 1")[3..], "-");
    let gone = ("nobody:1".to_string(), 1, "dexter:in_01".to_string(), 1);
    assert_eq!(
        s.op(0, Op::Toggle { cell: gone }).unwrap_err(),
        Refused::Other("that cell isn't on the screen any more".into())
    );
}

#[test]
fn zoom_and_forget() {
    let backend = shared(DECK);
    let mut s = Sheet::new(Box::new(backend), Grouping::Guess);
    let m = s.op(0, all()).unwrap();
    // Guessed: the monitor is one row of two; a zoom is port by port.
    assert_eq!(cell(&m, "dexter:monitor_L", "sim:playback_1"), "##=s");
    let k = key(&m, "dexter:monitor_L", "sim:playback_1");
    assert_eq!((k.1, k.3), (2, 2));
    let z = s.op(0, Op::View { all: true, zoom: Some(k.clone()), grouping: None }).unwrap();
    assert_eq!(z.zoom, Some(k));
    assert_eq!(z.cells, ["##=s..=s", "..=s##=s"]);
    let z = toggle(&mut s, 0, &z, "dexter:monitor_L", "sim:playback_2").unwrap();
    assert_eq!(z.staged.len(), 1);

    // A screen that goes takes its view and staging with it.
    s.forget(0);
    let m = s.op(0, Op::Show).unwrap();
    assert!(!m.all && m.zoom.is_none() && m.staged.is_empty());
}

/// Sentences: each verb one staged line; ONLY cuts what else feeds the column, hidden
/// clients' ports too; one that changes nothing refused; a line unstaged by its index;
/// APPLY and UNDO take a sentence as one change.
#[test]
fn sentences_stage_a_line_each() {
    use Verb::{Cut, Only, To};
    let (mut s, b) = deck(Deck::default());
    let m = s.op(0, Op::Show).unwrap();

    let m = stage(&mut s, &m, "sim:capture_1", "2) Vln 2", To).unwrap();
    assert_eq!(texts(&m), [(Mark::Add, "sim[1] > \"2) Vln 2\"")]);
    assert_eq!(cell(&m, "sim:capture_1", "2) Vln 2"), ".#+s", "a tap on it stages a toggle of its own");
    let m = stage(&mut s, &m, "sim:capture_1", "1) Vln 1", Cut).unwrap();
    assert_eq!(texts(&m)[1], (Mark::Remove, "sim[1] !> \"1) Vln 1\""));
    assert_eq!(words(&m), ["capture_1 TO 2) Vln 2", "capture_1 CUT 1) Vln 1"]);

    for (row, col, verb, why) in [
        ("sim:capture_2", "2) Vln 2", To, "already connected"),
        ("sim:capture_1", "2) Vln 2", To, "already connected"),
        ("sim:capture_2", "1) Vln 1", Cut, "not connected"),
        ("sim:capture_1", "1) Vln 1", Cut, "not connected"),
    ] {
        assert_eq!(stage(&mut s, &m, row, col, verb).unwrap_err(), Refused::Other(why.into()), "{row} {col}");
    }
    assert_eq!(s.op(0, Op::Show).unwrap().staged.len(), 2);

    let m = s.op(0, Op::Unstage { index: 0 }).unwrap();
    assert_eq!(texts(&m), [(Mark::Remove, "sim[1] !> \"1) Vln 1\"")]);
    assert_eq!(
        s.op(0, Op::Unstage { index: 1 }).unwrap_err(),
        Refused::Other("there's no staged line 1".into())
    );
    s.op(0, Op::Drop).unwrap();

    // Two sources into track 2, the click hidden from this view: ONLY cuts both, and connects.
    let wide = s.op(0, all()).unwrap();
    toggle(&mut s, 0, &wide, "metro:click", "2) Vln 2").unwrap();
    s.op(0, Op::Apply { anyway: false }).unwrap();
    let m = s.op(0, Op::View { all: false, zoom: None, grouping: None }).unwrap();
    assert!(m.rows.iter().all(|a| a.client != "metro"));
    let m = stage(&mut s, &m, "sim:capture_1", "2) Vln 2", Only).unwrap();
    assert_eq!(
        texts(&m),
        [(Mark::Mixed, "sim[2] !> \"2) Vln 2\"; metro !> \"2) Vln 2\"; sim[1] > \"2) Vln 2\"")]
    );
    assert_eq!(words(&m), ["capture_1 ONLY 2) Vln 2 · cuts capture_2, metro click"]);
    assert_eq!(m.unseen, (0, 1), "the click's cut is out of view");
    assert_eq!(cell(&m, "sim:capture_2", "2) Vln 2"), "#.-s");
    assert_eq!(
        stage(&mut s, &m, "sim:capture_1", "2) Vln 2", Only).unwrap_err(),
        Refused::Other("already only that".into())
    );
    // With FROM already connected, ONLY only cuts.
    s.op(0, Op::Drop).unwrap();
    let m = stage(&mut s, &m, "sim:capture_2", "2) Vln 2", Only).unwrap();
    assert_eq!(texts(&m), [(Mark::Remove, "metro !> \"2) Vln 2\"")]);
    assert_eq!(words(&m), ["metro click CUT 2) Vln 2"]);

    // One change: APPLY makes it, UNDO takes it back whole.
    let m = s.op(0, Op::Drop).unwrap();
    stage(&mut s, &m, "sim:capture_1", "2) Vln 2", Only).unwrap();
    s.op(0, Op::Apply { anyway: false }).unwrap();
    assert!(linked(&b, "sim:capture_1", "dexter:in_02"));
    assert!(!linked(&b, "metro:click", "dexter:in_02") && !linked(&b, "sim:capture_2", "dexter:in_02"));
    s.op(0, Op::Undo).unwrap();
    let wide = s.op(0, all()).unwrap();
    assert_eq!(cell(&wide, "metro:click", "2) Vln 2"), "##=s");
    assert_eq!(cell(&wide, "sim:capture_2", "2) Vln 2"), "##=s");
    assert_eq!(cell(&wide, "sim:capture_1", "2) Vln 2"), "..=s");

    let e = stage(&mut s, &wide, "metro:midi_out", "1) Vln 1", To).unwrap_err();
    assert!(matches!(&e, Refused::Patch { kind, .. } if kind == "type_mismatch"), "{e:?}");
}

/// The names: ports by their owner's (a name taken leaves a port its own), placed again with
/// each snapshot; a tool's own name changed, a line naming the old one stale and kept.
#[test]
fn names_after_each_snapshot() {
    let (mut s, b) = deck(Deck::default());
    s.set_names(vec![
        ("sim:capture_2".into(), "Analogue 2".into()),
        ("sim:capture_1".into(), "1) Vln 1".into()),
        ("sim:playback_2".into(), "LINE 2".into()),
        ("sim:capture_9".into(), "Analogue 9".into()),
    ]);
    let m = s.op(0, Op::Show).unwrap();
    let labels = |v: &[Axis], client: &str| -> Vec<String> {
        v.iter().filter(|a| a.client == client).map(|a| a.label.clone()).collect()
    };
    assert_eq!(labels(&m.rows, "sim"), ["capture_1", "Analogue 2"], "a track's name is taken");
    assert_eq!(labels(&m.cols, "sim"), ["playback_1", "LINE 2"]);
    assert!(m.problems.is_empty(), "{:?}", m.problems);

    let m = stage(&mut s, &m, "Analogue 2", "1) Vln 1", Verb::Only).unwrap();
    assert_eq!(texts(&m), [(Mark::Mixed, "sim[1] !> \"1) Vln 1\"; \"Analogue 2\" > \"1) Vln 1\"")]);
    assert_eq!(words(&m), ["Analogue 2 ONLY 1) Vln 1 · cuts capture_1"]);
    assert_eq!(
        m.staged[0].links,
        [link("sim:capture_1", "dexter:in_01", false), link("sim:capture_2", "dexter:in_01", true)]
    );
    let m = stage(&mut s, &m, "2) Vln 2 out", "LINE 2", Verb::To).unwrap();
    assert_eq!(words(&m)[1], "2) Vln 2 out TO LINE 2");

    // The graph changes under it: a port before capture_2 comes and goes, and its name follows.
    b.0.lock().unwrap().register("sim:capture_0", PortType::Audio, crate::model::Direction::Out).unwrap();
    assert!(b.0.lock().unwrap().unregister("sim:capture_1"));
    let m = s.op(0, Op::Show).unwrap();
    assert_eq!(labels(&m.rows, "sim"), ["Analogue 2", "capture_0"]);
    let named = m.rows.iter().find(|a| a.label == "Analogue 2").expect("still named");
    assert_eq!(named.first, "sim:capture_2");
    // bay's `sim[1]` is a place, not a port: the first line's cut now names capture_2.
    assert_ne!(m.staged[0].mark, Mark::Stale);
    assert_eq!(words(&m)[0], "Analogue 2 ONLY 1) Vln 1 · cuts capture_1", "as said when staged");

    // A tool's name changed: the line naming the old one goes stale, and stays.
    let count = s.count();
    assert!(s.undefine("2) Vln 2 out"));
    assert!(s.count() > count);
    s.define("bundle \"2) Viola out\" = dexter[out 4]").unwrap();
    let m = s.op(0, Op::Show).unwrap();
    assert_eq!(m.staged[1].mark, Mark::Stale);
    assert!(m.staged[1].links.is_empty());
    let m = s.op(0, Op::Apply { anyway: false }).unwrap();
    assert!(linked(&b, "sim:capture_2", "dexter:in_01"));
    assert_eq!(
        texts(&m),
        [(Mark::Stale, "\"2) Vln 2 out\" > \"LINE 2\"")],
        "a stale line stays, to be unstaged"
    );
    assert_eq!(words(&m), ["2) Vln 2 out TO LINE 2"], "with its words");
    assert!(!s.undefine("2) Vln 2 out"));
}

/// A tool's `check` refuses APPLY, and UNDO, with its own refusal; both keep what they had.
#[test]
fn a_rule_that_refuses() {
    let (mut s, _) = deck(Deck::default());
    let m = s.op(0, all()).unwrap();
    toggle(&mut s, 0, &m, "metro:click", "2) Vln 2").unwrap();
    s.rules_mut().refuse = vec!["dexter:in_02".into()];
    let e = s.op(0, Op::Apply { anyway: true }).unwrap_err();
    assert_eq!(e, Refused::Rule { kind: "armed_input".into(), text: "dexter:in_02 is armed".into() });
    assert_eq!(s.op(0, Op::Show).unwrap().staged.len(), 1, "a refused apply keeps the lines");

    // Elsewhere goes ahead; an undo into the refused port doesn't, and stays on the stack.
    s.op(0, Op::Drop).unwrap();
    toggle(&mut s, 0, &m, "metro:click", "1) Vln 1").unwrap();
    s.op(0, Op::Apply { anyway: false }).unwrap();
    s.rules_mut().refuse.clear();
    toggle(&mut s, 0, &m, "metro:click", "2) Vln 2").unwrap();
    s.op(0, Op::Apply { anyway: false }).unwrap();
    s.rules_mut().refuse = vec!["dexter:in_02".into()];
    assert!(matches!(s.op(0, Op::Undo).unwrap_err(), Refused::Rule { .. }));
    s.rules_mut().refuse.clear();
    let m = s.op(0, Op::Undo).unwrap();
    assert_eq!(cell(&m, "metro:click", "2) Vln 2"), "..=s", "the refused undo was kept");
    assert_eq!(cell(&m, "metro:click", "1) Vln 1"), "##=s");

    let log = s.rules().log.borrow();
    assert!(log.iter().any(|(l, w)| l == "patch: metro > dexter[2]" && !w), "{log:?}");
    assert!(log.iter().any(|(l, _)| l.starts_with("patch undo: ")), "{log:?}");
}

/// A tool's warnings: one that asks refuses APPLY until `anyway`; one that's only said
/// doesn't; neither is said for what's there already.
#[test]
fn a_rule_that_warns() {
    let rules =
        Deck { asks: vec!["reverb:in_L".into()], says: vec!["sim:playback_2".into()], ..Deck::default() };
    let (mut s, b) = deck(rules);
    let m = s.op(0, all()).unwrap();
    let m = toggle(&mut s, 0, &m, "dexter:monitor_L", "reverb:in_L").unwrap();
    assert_eq!(m.warnings, [Warning { text: "reverb:in_L hears it".into(), ask: true }]);
    assert_eq!(s.op(1, Op::Show).unwrap().warnings, [], "another screen's lines are its own");
    assert_eq!(
        s.op(0, Op::Apply { anyway: false }).unwrap_err(),
        Refused::Ask(vec!["reverb:in_L hears it".into()])
    );
    assert_eq!(s.op(0, Op::Show).unwrap().staged.len(), 1, "kept staged");
    let m = s.op(0, Op::Apply { anyway: true }).unwrap();
    assert!(m.warnings.is_empty() && m.staged.is_empty());
    assert!(linked(&b, "dexter:monitor_L", "reverb:in_L"));

    let m = toggle(&mut s, 0, &m, "dexter:out_01", "sim:playback_2").unwrap();
    assert_eq!(m.warnings, [Warning { text: "sim:playback_2 sounds".into(), ask: false }]);
    s.op(0, Op::Apply { anyway: false }).unwrap();
    assert!(linked(&b, "dexter:out_01", "sim:playback_2"));
}

/// Reserved ports: hidden from the tool's view, shown in all, refused in any; the tool's own
/// refusal first.
#[test]
fn reserved_ports_are_kept_out() {
    let (mut s, _) = deck(Deck { reserved: vec!["sim:playback_2".into()], ..Deck::default() });
    let m = s.op(0, Op::Show).unwrap();
    assert!(m.cols.iter().all(|a| a.first != "sim:playback_2"), "{:?}", m.cols);
    assert!(m.cols.iter().any(|a| a.first == "sim:playback_1"));
    let m = s.op(0, all()).unwrap();
    toggle(&mut s, 0, &m, "1) Vln 1 out", "sim:playback_2").unwrap();
    let e = s.op(0, Op::Apply { anyway: true }).unwrap_err();
    assert_eq!(
        e,
        Refused::Rule {
            kind: "reserved".into(),
            text: "sim:playback_2 is reserved: nothing is patched to it".into()
        }
    );
    s.rules_mut().refuse = vec!["sim:playback_2".into()];
    assert!(
        matches!(s.op(0, Op::Apply { anyway: true }).unwrap_err(), Refused::Rule { kind, .. } if kind == "armed_input")
    );
    s.op(0, Op::Drop).unwrap();
    toggle(&mut s, 0, &m, "1) Vln 1 out", "sim:playback_1").unwrap();
    s.op(0, Op::Apply { anyway: true }).unwrap();
}

/// jackbay's `--sim`: in pairs, nothing connected, a line's links saying where it goes.
#[test]
fn jackbays_sim_in_pairs() {
    let mut s = Sheet::new(Box::new(shared(SIM)), Grouping::Guess);
    let m = s.op(0, Op::Show).unwrap();
    let labels = |v: &[Axis]| v.iter().map(|a| format!("{} {}", a.client, a.label)).collect::<Vec<_>>();
    assert_eq!(labels(&m.rows), ["system capture_1/2", "scape out_L/R", "brr out_1/2"]);
    assert_eq!(labels(&m.cols), ["system playback_1/2"]);
    assert!(m.cells.iter().all(|r| r == "..=s"));
    let m = toggle(&mut s, 0, &m, "scape:out_L", "system:playback_1").unwrap();
    assert_eq!(
        m.staged[0].links,
        [link("scape:out_L", "system:playback_1", true), link("scape:out_R", "system:playback_2", true)]
    );
}

/// A screen's own grouping (SPEC 8.3): two screens apart on one graph; a line staged in pairs
/// means the same each side alone; ONLY on one side cuts only what feeds that port; None
/// goes back to the sheet's own.
#[test]
fn a_screens_own_grouping() {
    let b = shared(SIM);
    let mut s = Sheet::new(Box::new(b.clone()), Grouping::Guess);
    let mono = Op::View { all: false, zoom: None, grouping: Some(Grouping::Mono) };
    let pairs = s.op(0, Op::Show).unwrap();
    let alone = s.op(1, mono.clone()).unwrap();
    assert_eq!((pairs.grouping, alone.grouping), (Grouping::Guess, Grouping::Mono));
    assert_eq!((pairs.rows.len(), alone.rows.len()), (3, 6));
    assert_eq!((pairs.cols.len(), alone.cols.len()), (1, 2));
    assert_eq!(s.op(0, Op::Show).unwrap().rows.len(), 3, "still in pairs");

    // Staged in pairs, then each side alone: the same line, its cells marked both sides.
    let m = toggle(&mut s, 0, &pairs, "scape:out_L", "system:playback_1").unwrap();
    let before = m.staged.clone();
    let m = s.op(0, mono.clone()).unwrap();
    assert_eq!(
        m.staged.iter().map(|l| (&l.text, &l.words)).collect::<Vec<_>>(),
        before.iter().map(|l| (&l.text, &l.words)).collect::<Vec<_>>()
    );
    assert_eq!(cell(&m, "scape:out_L", "system:playback_1"), ".#+s");
    assert_eq!(cell(&m, "scape:out_R", "system:playback_2"), ".#+s");
    assert_eq!(cell(&m, "scape:out_L", "system:playback_2"), "..=s");
    s.op(0, Op::Apply { anyway: false }).unwrap();
    assert!(linked(&b, "scape:out_L", "system:playback_1") && linked(&b, "scape:out_R", "system:playback_2"));

    // brr too, in pairs on the other screen; then ONLY brr's left into the left, alone.
    let other = s.op(1, Op::View { all: false, zoom: None, grouping: None }).unwrap();
    assert_eq!(other.grouping, Grouping::Guess, "None is the sheet's own");
    toggle(&mut s, 1, &other, "brr:out_1", "system:playback_1").unwrap();
    s.op(1, Op::Apply { anyway: false }).unwrap();
    let m = s.op(0, Op::Show).unwrap();
    let m = stage(&mut s, &m, "brr:out_1", "system:playback_1", Verb::Only).unwrap();
    assert_eq!(m.staged[0].links, [link("scape:out_L", "system:playback_1", false)]);
    assert_eq!(words(&m), ["out_L CUT playback_1"], "brr's already there: ONLY only cuts");
    s.op(0, Op::Apply { anyway: false }).unwrap();
    assert!(!linked(&b, "scape:out_L", "system:playback_1"));
    assert!(linked(&b, "scape:out_R", "system:playback_2") && linked(&b, "brr:out_2", "system:playback_2"));
}
