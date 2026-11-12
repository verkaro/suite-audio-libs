//! The bundle matrix as data: output bundles (rows) against input bundles (columns), each
//! cell's state now and with the staged edits, and what Enter would stage there. bay's TUI
//! and `grid` draw from it, and so would Dexter's page. A [`View`] zooms into one bundle
//! pair or filters the rows and columns.
//!
//! It's plain owned data, worked out when it's built: a screen rebuilds it when the
//! snapshot, the session or the view changes.

use crate::bundle::{Bundle, Cell};
use crate::compile::Plan;
use crate::error::{ErrorKind, Problem};
use crate::model::{ClientId, Direction, PortId, PortType, Snapshot};
use crate::resolve::Resolver;
use crate::session::Session;
use crate::stage::{Stage, Stale};
use crate::syntax::{Ref, Selector, Statement};
use crate::views::Names;

/// A cell by identity: the first port and size of its row and of its column. It survives
/// rebuilds that move rows around, and sizes keep a zoomed-in port cell apart from the
/// bundle cell that starts on the same port.
pub type CellKey = (String, usize, String, usize);

/// `rea` keeps the rows and columns matching `rea` (an axis with no match stays whole);
/// `rea > obs` keeps rows matching `rea` and columns matching `obs`, like the patch
/// syntax. A bundle matches on its client's name or alias, or its label, ignoring case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Filter {
    pub text: String,
    rows: String,
    cols: String,
    /// One term for both axes, where an axis with no match stays whole.
    both: bool,
}

impl Filter {
    /// None for an empty filter.
    pub fn parse(text: &str) -> Option<Filter> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        let (rows, cols, both) = match text.split_once('>') {
            Some((r, c)) => (r.trim(), c.trim(), false),
            None => (text, text, true),
        };
        Some(Filter { text: text.to_owned(), rows: rows.to_lowercase(), cols: cols.to_lowercase(), both })
    }
}

/// A name pattern for hiding and favourites. With a `:` it's matched against a port's full
/// name (`*:monitor_*`), without one against its client's name (`REAPER`). `*` matches
/// any run of characters, and case is ignored.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Pattern(String);

impl Pattern {
    pub fn new(text: &str) -> Pattern {
        Pattern(text.trim().to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether a port (by its full `client:port` name) matches.
    pub fn matches(&self, port: &str) -> bool {
        let subject = if self.0.contains(':') { port } else { port.split(':').next().unwrap_or(port) };
        glob(&self.0.to_lowercase(), &subject.to_lowercase())
    }
}

/// `*`-only globbing over whole strings.
fn glob(pat: &str, s: &str) -> bool {
    match pat.split_once('*') {
        None => pat == s,
        Some((head, rest)) => {
            let Some(mut s) = s.strip_prefix(head) else { return false };
            // Each later piece must follow in order; the last must end the string.
            let pieces: Vec<&str> = rest.split('*').collect();
            for (i, piece) in pieces.iter().enumerate() {
                if i + 1 == pieces.len() {
                    return s.len() >= piece.len() && s.ends_with(piece);
                }
                match s.find(piece) {
                    Some(at) => s = &s[at + piece.len()..],
                    None => return false,
                }
            }
            true
        }
    }
}

/// How a screen looks at the matrix. The hiding and favourite rules come from the owner
/// (bay's front ends pass bay's defaults, Dexter its own); nothing is hidden by a name
/// built into core. Hiding only changes what shows: never what a plan does.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct View {
    /// The bundle cell zoomed into: its ports become the rows and columns.
    pub zoom: Option<CellKey>,
    /// Ignored while zoomed.
    pub filter: Option<Filter>,
    /// A bundle whose ports all match one of these doesn't show (not while zoomed).
    pub hide: Vec<Pattern>,
    /// Bundles with a port matching one of these come first, in their order otherwise.
    pub favourites: Vec<Pattern>,
}

/// A row or column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Axis {
    pub bundle: Bundle,
    /// The client's raw name.
    pub client: String,
    /// The client as views print it: a session alias, a generated alias, or the raw name.
    pub shown: String,
    /// The bundle as someone can type it: its declared name, else the client (by a session
    /// alias or its raw name) with the type and range when it's not all the ports.
    pub typed: Ref,
    /// The first port's full name, which with the size keys a cell.
    pub first: String,
    /// For a row: the staged input that is just `exact` for this client, which `x`
    /// unstages.
    pub exact: Option<usize>,
    /// It matches one of the view's favourites.
    pub favourite: bool,
}

/// Staged changes the view doesn't show: links to or from ports that are hidden, filtered
/// out, or outside the zoom. A screen says so, since hiding never changes what applying
/// does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Unseen {
    pub add: usize,
    pub del: usize,
}

/// How the staged edits change a cell's links.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    None,
    Add,
    Remove,
    Both,
}

/// What Enter does on a cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Toggle {
    /// Stage this input.
    Stage(String),
    /// Unstage staged input n, which this cell made.
    Unstage(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellView {
    pub now: Cell,
    pub after: Cell,
    pub change: Change,
    /// Refused as `type_mismatch` (different port types) or `pairing` (bundles that can't
    /// pair, and aren't partly linked either).
    pub toggle: Result<Toggle, Problem>,
}

/// A staged input as a footer shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// Only connects.
    Add,
    /// Only disconnects.
    Remove,
    /// Both, or definitions too.
    Mixed,
    /// It no longer compiles against the snapshot (its ports are gone); it stays staged.
    Stale,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StagedLine {
    pub mark: Mark,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct Matrix {
    pub rows: Vec<Axis>,
    pub cols: Vec<Axis>,
    /// Row by row.
    cells: Vec<CellView>,
    /// The names the rows and columns are shown by, for printing statements.
    pub names: Names,
    /// The stage's inputs, in order.
    pub staged: Vec<StagedLine>,
    /// The staged edits compiled against the snapshot, and the inputs that went stale.
    pub plan: Plan,
    pub stale: Vec<Stale>,
    /// Whether the view's zoom applies. False when its bundles are gone: the rows and
    /// columns are then the bundles, and `problems` says so.
    pub zoomed: bool,
    /// Session bundles that didn't apply, and a zoom whose bundles are gone.
    pub problems: Vec<String>,
    /// The clients (raw names) the staged edits make `exact`, in client order.
    pub exact: Vec<String>,
    /// How many bundles the view's hide rules leave out.
    pub hidden: usize,
    /// Staged changes out of view.
    pub unseen: Unseen,
}

impl Matrix {
    pub fn build(snap: &Snapshot, session: &Session, stage: &Stage, view: &View) -> Matrix {
        let (bundles, mut problems) = session.bundles(snap);
        let names = session.names(snap);
        let axis = |b: &Bundle| {
            let client = snap.client(b.client).name();
            Axis {
                typed: typed(snap, &names, b),
                client: client.to_owned(),
                shown: names.get(client).to_owned(),
                first: snap.port(b.ports[0]).name().to_owned(),
                favourite: b
                    .ports
                    .iter()
                    .any(|&p| view.favourites.iter().any(|f| f.matches(snap.port(p).name()))),
                bundle: b.clone(),
                exact: None,
            }
        };
        let side = |dir| {
            let mut v: Vec<Axis> =
                bundles.iter().filter(|(_, b)| b.dir == dir).map(|(_, b)| axis(b)).collect();
            v.sort_by_key(|a| (!a.favourite, a.bundle.ty));
            v
        };
        let (mut rows, mut cols) = (side(Direction::Out), side(Direction::In));
        let hidden_by_rules = |a: &Axis| {
            !view.hide.is_empty()
                && a.bundle.ports.iter().all(|&p| view.hide.iter().any(|h| h.matches(snap.port(p).name())))
        };

        let mut zoomed = false;
        if let Some(key) = &view.zoom {
            let find = |v: &[Axis], first: &str, len| {
                v.iter().position(|a| a.first == first && a.bundle.ports.len() == len)
            };
            match (find(&rows, &key.0, key.1), find(&cols, &key.2, key.3)) {
                (Some(r), Some(c)) => {
                    let ports = |a: &Axis| -> Vec<Axis> {
                        a.bundle.ports.iter().map(|&p| axis(&one(snap, &a.bundle, p))).collect()
                    };
                    (rows, cols) = (ports(&rows[r]), ports(&cols[c]));
                    zoomed = true;
                }
                _ => problems.push("the zoomed-in bundles are gone".into()),
            }
        }
        let mut hidden = 0;
        if !zoomed {
            for v in [&mut rows, &mut cols] {
                let before = v.len();
                v.retain(|a| !hidden_by_rules(a));
                hidden += before - v.len();
            }
        }
        if let (Some(f), false) = (&view.filter, zoomed) {
            let keep = |v: &mut Vec<Axis>, term: &str| {
                if term.is_empty() {
                    return;
                }
                let hit = |a: &Axis| {
                    [&a.client, &a.shown, &a.bundle.label].iter().any(|n| n.to_lowercase().contains(term))
                };
                if !f.both || v.iter().any(hit) {
                    v.retain(hit);
                }
            };
            keep(&mut rows, &f.rows);
            keep(&mut cols, &f.cols);
        }

        // Which clients the staged edits make exact, and which staged input is just that.
        let res = Resolver::new(snap, vec![session.scope()]);
        let mut exact: Vec<ClientId> = Vec::new();
        for (i, input) in stage.inputs().iter().enumerate() {
            for s in &input.stmts {
                if let Statement::Exact { client } = &s.stmt
                    && let Ok(c) = res.client_of(client)
                {
                    exact.push(c);
                    if input.stmts.len() == 1 {
                        for row in rows.iter_mut().filter(|a| a.bundle.client == c) {
                            row.exact = Some(i);
                        }
                    }
                }
            }
        }
        exact.sort();
        exact.dedup();
        let exact = exact.iter().map(|&c| snap.client(c).name().to_owned()).collect();

        let (plan, stale) = stage.plan(session, snap);
        let after = snap.with_plan(&plan);
        let shown = |v: &[Axis]| {
            v.iter()
                .flat_map(|a| a.bundle.ports.iter().copied())
                .collect::<std::collections::HashSet<PortId>>()
        };
        let (outs, ins) = (shown(&rows), shown(&cols));
        let out_of_view = |l: &&crate::model::Link| !outs.contains(&l.from) || !ins.contains(&l.to);
        let unseen = Unseen {
            add: plan.add.iter().filter(out_of_view).count(),
            del: plan.del.iter().filter(out_of_view).count(),
        };
        let keys: Vec<Option<&CellKey>> = stage.inputs().iter().map(|s| s.key.as_ref()).collect();
        let mut cells = Vec::with_capacity(rows.len() * cols.len());
        for row in &rows {
            for col in &cols {
                let key =
                    (row.first.clone(), row.bundle.ports.len(), col.first.clone(), col.bundle.ports.len());
                let staged_on = keys.iter().position(|k| *k == Some(&key));
                cells.push(cell(snap, &after, row, col, staged_on));
            }
        }
        let staged = stage
            .inputs()
            .iter()
            .enumerate()
            .map(|(i, input)| {
                let all = |f: fn(&Statement) -> bool| input.stmts.iter().all(|s| f(&s.stmt));
                let mark = if stale.iter().any(|s| s.input == i) {
                    Mark::Stale
                } else if all(|s| matches!(s, Statement::Connect { .. })) {
                    Mark::Add
                } else if all(|s| matches!(s, Statement::Disconnect { .. })) {
                    Mark::Remove
                } else {
                    Mark::Mixed
                };
                StagedLine { mark, text: input.text() }
            })
            .collect();
        Matrix { rows, cols, cells, names, staged, plan, stale, zoomed, problems, exact, hidden, unseen }
    }

    pub fn cell(&self, r: usize, c: usize) -> &CellView {
        &self.cells[r * self.cols.len() + c]
    }

    pub fn key(&self, r: usize, c: usize) -> CellKey {
        let (row, col) = (&self.rows[r], &self.cols[c]);
        (row.first.clone(), row.bundle.ports.len(), col.first.clone(), col.bundle.ports.len())
    }

    /// What `x` does on row r: stage `exact` for its client (its outputs then go only where
    /// the staged edits say), or unstage the input that did.
    pub fn exact_toggle(&self, r: usize) -> Toggle {
        let row = &self.rows[r];
        match row.exact {
            Some(i) => Toggle::Unstage(i),
            None => {
                let stmt = Statement::Exact { client: self.names.typable(&row.client).to_owned() };
                Toggle::Stage(stmt.to_string())
            }
        }
    }

    /// Where the cell with this key is, if it's shown.
    pub fn find(&self, key: &CellKey) -> Option<(usize, usize)> {
        let at = |v: &[Axis], first: &str, len| {
            v.iter().position(|a| a.first == first && a.bundle.ports.len() == len)
        };
        Some((at(&self.rows, &key.0, key.1)?, at(&self.cols, &key.2, key.3)?))
    }
}

/// One port of a bundle, as a bundle of one named by the port.
fn one(snap: &Snapshot, b: &Bundle, p: PortId) -> Bundle {
    Bundle { label: snap.port(p).short_name().to_owned(), declared: false, ports: vec![p], ..b.clone() }
}

fn typed(snap: &Snapshot, names: &Names, b: &Bundle) -> Ref {
    if b.declared {
        return Ref::new(&b.label);
    }
    let client = snap.client(b.client).name();
    let all = snap.ports_of(b.client, b.ty, b.dir);
    let (first, last) = (snap.port(b.ports[0]).ordinal(), snap.port(b.ports[b.ports.len() - 1]).ordinal());
    let sel = Selector {
        dir: None,
        ty: (b.ty == PortType::Midi).then_some(PortType::Midi),
        range: (b.ports.len() != all.len()).then_some((first, last)),
    };
    Ref { name: names.typable(client).to_owned(), sel }
}

/// A cell's states, the staged change, and what Enter does there: full → `A !> B`, empty
/// → `A > B`, partial or crossed → `A !> B; A > B` (exactly the full pairing), on a cell
/// with a staged input of its own → unstage it.
fn cell(snap: &Snapshot, after: &Snapshot, row: &Axis, col: &Axis, staged_on: Option<usize>) -> CellView {
    let (now, later) = (Cell::of(snap, &row.bundle, &col.bundle), Cell::of(after, &row.bundle, &col.bundle));
    let links = |s: &Snapshot| -> Vec<(PortId, PortId)> {
        let mut v = Vec::new();
        for &f in &row.bundle.ports {
            for &t in &col.bundle.ports {
                if s.is_linked(f, t) {
                    v.push((f, t));
                }
            }
        }
        v
    };
    let (before, then) = (links(snap), links(after));
    let added = then.iter().any(|l| !before.contains(l));
    let removed = before.iter().any(|l| !then.contains(l));
    let change = match (added, removed) {
        (false, false) => Change::None,
        (true, false) => Change::Add,
        (false, true) => Change::Remove,
        (true, true) => Change::Both,
    };

    let toggle = if let Some(i) = staged_on {
        Ok(Toggle::Unstage(i))
    } else {
        let (n, m) = (row.bundle.ports.len(), col.bundle.ports.len());
        let pairs = n == m || n == 1 || m == 1;
        let off = Statement::Disconnect { from: row.typed.clone(), to: col.typed.clone() };
        let on = Statement::Connect { from: row.typed.clone(), to: col.typed.clone() };
        match later {
            Cell::Types => Err(Problem::new(ErrorKind::TypeMismatch, "different port types")),
            Cell::Full => Ok(Toggle::Stage(off.to_string())),
            Cell::Empty if pairs => Ok(Toggle::Stage(on.to_string())),
            Cell::Empty => {
                Err(Problem::new(ErrorKind::Pairing, format!("can't pair {n} outputs with {m} inputs")))
            }
            _ if pairs => Ok(Toggle::Stage(format!("{off}; {on}"))),
            _ => Ok(Toggle::Stage(off.to_string())),
        }
    };
    CellView { now, after: later, change, toggle }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;

    fn studio() -> Snapshot {
        fixture::parse(include_str!("../testdata/studio.fake")).unwrap()
    }

    fn at(m: &Matrix, row: &str, col: &str) -> (usize, usize) {
        let r = m.rows.iter().position(|a| a.bundle.label == row).unwrap();
        let c = m.cols.iter().position(|a| a.bundle.label == col).unwrap();
        (r, c)
    }

    fn toggle(m: &Matrix, row: &str, col: &str) -> Result<Toggle, String> {
        let (r, c) = at(m, row, col);
        m.cell(r, c).toggle.clone().map_err(|p| format!("{}: {}", p.kind, p.msg))
    }

    #[test]
    fn axes_and_cells() {
        let s = studio();
        let m = Matrix::build(&s, &Session::new(), &Stage::new(), &View::default());
        let rows: Vec<String> = m.rows.iter().map(|a| format!("{} {}", a.shown, a.bundle.label)).collect();
        assert_eq!(rows[0], "focusrite-in capture_FL/FR");
        assert_eq!(m.rows.len(), 7);
        assert_eq!(m.cols.len(), 5);
        let (r, c) = at(&m, "out1/2", "playback_FL/FR");
        assert_eq!((m.cell(r, c).now, m.cell(r, c).change), (Cell::Full, Change::None));
        assert_eq!(m.rows[r].typed.to_string(), "REAPER[1-2]");
        // A generated alias can't be typed: the raw name is.
        assert_eq!(
            m.cols[c].typed.to_string(),
            "alsa_output.usb-Focusrite_Scarlett_2i2_USB-00.analog-stereo"
        );
        assert_eq!(m.find(&m.key(r, c)), Some((r, c)));
    }

    #[test]
    fn toggles() {
        let s = studio();
        let session = Session::new();
        let mut stage = Stage::new();
        let m = Matrix::build(&s, &session, &stage, &View::default());
        assert_eq!(toggle(&m, "out5/6", "input_FL/FR"), Ok(Toggle::Stage("REAPER[5-6] > OBS".into())));
        assert_eq!(toggle(&m, "out3/4", "audio-in1/2"), Ok(Toggle::Stage("REAPER[3-4] !> Carla".into())));
        let fx = "alsa_input.usb-Focusrite_Scarlett_2i2_USB-00.analog-stereo";
        assert_eq!(
            toggle(&m, "capture_FL/FR", "in1/2"),
            Ok(Toggle::Stage(format!("{fx} !> REAPER[1-2]; {fx} > REAPER[1-2]")))
        );

        // Staged from a cell: that cell unstages it, and shows the change.
        let (r, c) = at(&m, "out5/6", "input_FL/FR");
        stage.add_at(&session, &s, "REAPER[5-6] > OBS", Some(m.key(r, c))).unwrap();
        let m = Matrix::build(&s, &session, &stage, &View::default());
        assert_eq!(toggle(&m, "out5/6", "input_FL/FR"), Ok(Toggle::Unstage(0)));
        assert_eq!((m.cell(r, c).after, m.cell(r, c).change), (Cell::Full, Change::Add));
        assert_eq!(m.staged, [StagedLine { mark: Mark::Add, text: "REAPER[5-6] > OBS".into() }]);

        // Bundles that can't pair, and different types.
        let mut session = Session::new();
        session.define("bundle three = REAPER[out 1-3]").unwrap();
        let s2 =
            fixture::parse(&(include_str!("../testdata/studio.fake").to_owned() + "in midi M:m\n")).unwrap();
        let m = Matrix::build(&s2, &session, &stage, &View::default());
        assert_eq!(toggle(&m, "three", "in3/4"), Err("pairing: can't pair 3 outputs with 2 inputs".into()));
        assert_eq!(toggle(&m, "three", "m"), Err("type_mismatch: different port types".into()));
    }

    #[test]
    fn exact_rows() {
        let s = studio();
        let session = Session::new();
        let mut stage = Stage::new();
        let m = Matrix::build(&s, &session, &stage, &View::default());
        let (r, _) = at(&m, "out3/4", "audio-in1/2");
        assert_eq!(m.exact_toggle(r), Toggle::Stage("exact REAPER".into()));
        stage.add(&session, &s, "exact REAPER").unwrap();
        let m = Matrix::build(&s, &session, &stage, &View::default());
        assert_eq!(m.exact, ["REAPER"]);
        // Every REAPER row knows the input; x on any of them unstages it.
        assert!(m.rows.iter().filter(|a| a.client == "REAPER").all(|a| a.exact == Some(0)));
        assert_eq!(m.exact_toggle(r), Toggle::Unstage(0));
        // REAPER's links go: the cells show it.
        let (r, c) = at(&m, "out3/4", "audio-in1/2");
        assert_eq!(
            (m.cell(r, c).now, m.cell(r, c).after, m.cell(r, c).change),
            (Cell::Full, Cell::Empty, Change::Remove)
        );
        // Keeping one: Enter on its cell now stages the link again.
        assert_eq!(m.cell(r, c).toggle, Ok(Toggle::Stage("REAPER[3-4] > Carla".into())));
        // An exact inside a bigger input counts, but x doesn't unstage that input.
        let mut stage = Stage::new();
        stage.add(&session, &s, "exact Carla; Carla > REAPER[3-4]").unwrap();
        let m = Matrix::build(&s, &session, &stage, &View::default());
        assert_eq!(m.exact, ["Carla"]);
        let (r, _) = at(&m, "audio-out1/2", "in3/4");
        assert_eq!(m.exact_toggle(r), Toggle::Stage("exact Carla".into()));
    }

    #[test]
    fn patterns() {
        let p = |t: &str, port: &str| Pattern::new(t).matches(port);
        assert!(p("*:monitor_*", "alsa_output.usb-X.analog-stereo:monitor_FL"));
        assert!(!p("*:monitor_*", "alsa_output.usb-X.analog-stereo:playback_FL"));
        assert!(p("reaper", "REAPER:out1"));
        assert!(!p("rea", "REAPER:out1"), "a whole name, not a substring");
        assert!(p("rea*", "REAPER:out1"));
        assert!(p("*:*_FL", "OBS:input_FL") && !p("*:*_FL", "OBS:input_FR"));
        assert!(p("a:*b*c", "a:xbyc") && !p("a:*b*c", "a:xcyb"));
        assert!(p("*", "anything:at_all"));
    }

    #[test]
    fn hiding_and_favourites() {
        let s = studio();
        let session = Session::new();
        let mut stage = Stage::new();
        let view = View {
            hide: vec![Pattern::new("*:monitor_*")],
            favourites: vec![Pattern::new("Firefox"), Pattern::new("OBS")],
            ..View::default()
        };
        let m = Matrix::build(&s, &session, &stage, &view);
        let labels = |v: &[Axis]| v.iter().map(|a| a.bundle.label.clone()).collect::<Vec<_>>();
        assert_eq!(m.hidden, 1);
        assert_eq!(
            labels(&m.rows),
            ["output_FL/FR", "capture_FL/FR", "out1/2", "out3/4", "out5/6", "audio-out1/2"]
        );
        assert_eq!(labels(&m.cols)[0], "input_FL/FR");
        assert!(m.rows[0].favourite && !m.rows[1].favourite);

        // A staged change to a hidden port is counted, not dropped.
        stage.add(&session, &s, "alsa_output[1] > REAPER[1]").unwrap();
        let m = Matrix::build(&s, &session, &stage, &view);
        assert_eq!(m.unseen, Unseen { add: 1, del: 0 });
        assert_eq!(m.plan.add.len(), 1, "the plan itself is unchanged");
        // Shown again, it's in view.
        let m = Matrix::build(&s, &session, &stage, &View { hide: vec![], ..view.clone() });
        assert_eq!((m.hidden, m.unseen), (0, Unseen::default()));
        // jack2's system client is never hidden unless a rule names it.
        let sys =
            fixture::parse("out system:capture_1\nout system:capture_2\nin system:playback_1\n").unwrap();
        assert_eq!(Matrix::build(&sys, &Session::new(), &Stage::new(), &view).hidden, 0);
    }

    #[test]
    fn zoom_and_filter() {
        let s = studio();
        let session = Session::new();
        let stage = Stage::new();
        let m = Matrix::build(&s, &session, &stage, &View::default());
        let (r, c) = at(&m, "out5/6", "input_FL/FR");
        let zoom = View { zoom: Some(m.key(r, c)), ..View::default() };
        let z = Matrix::build(&s, &session, &stage, &zoom);
        assert!(z.zoomed);
        let labels = |v: &[Axis]| v.iter().map(|a| a.bundle.label.clone()).collect::<Vec<_>>();
        assert_eq!(
            (labels(&z.rows), labels(&z.cols)),
            (vec!["out5".into(), "out6".into()], vec!["input_FL".into(), "input_FR".into()])
        );
        assert_eq!(z.cell(1, 0).toggle, Ok(Toggle::Stage("REAPER[6] > OBS[1]".into())));

        let gone = View { zoom: Some(("Nope:x".into(), 2, "OBS:input_FL".into(), 2)), ..View::default() };
        let g = Matrix::build(&s, &session, &stage, &gone);
        assert!(!g.zoomed);
        assert_eq!(g.problems, ["the zoomed-in bundles are gone"]);
        assert_eq!(g.rows.len(), 7);

        let f = Matrix::build(
            &s,
            &session,
            &stage,
            &View { filter: Filter::parse("rea > obs"), ..View::default() },
        );
        assert_eq!(
            (labels(&f.rows), labels(&f.cols)),
            (vec!["out1/2".into(), "out3/4".into(), "out5/6".into()], vec!["input_FL/FR".into()])
        );
        let f =
            Matrix::build(&s, &session, &stage, &View { filter: Filter::parse("fire"), ..View::default() });
        assert_eq!((f.rows.len(), f.cols.len()), (1, 5));
        assert!(Filter::parse("  ").is_none());
    }
}
