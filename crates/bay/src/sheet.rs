//! The bay sheet: one bay shared by several screens, each with its own view and its own
//! staged lines. The session (the names and undo) is everyone's; a screen shows all clients
//! or the tool's own, zooms into a cell, groups its ports as it likes, and stages lines by a
//! tap on a cell or by a sentence (TO, CUT, ONLY). APPLY makes a screen's lines, UNDO takes
//! back the last change, whoever made it. What a tool forbids or warns of is its [`Rules`].
//!
//! Written here 2026-11-11 from Dexter's `crates/patch` at `e98d44f`, its sheet without its
//! rules (`RFC-as-bay.md`, b2), for jackbay and later Dexter. A screen's own grouping is
//! jackbay's (`jackbay/docs/dev/SPEC.md`, 8.3). It needs no I/O: on the fake backend
//! ([`crate::fake`]) it runs a tool's `--sim` without JACK.

use std::collections::BTreeMap;
use std::fmt;

use crate::backend::{Backend, Failure};
use crate::bundle::{Cell, Grouping};
use crate::compile::{NamedLink, Options, Plan};
use crate::error::ErrorKind;
use crate::matrix::{self, CellKey, Change, Mark, Pattern, Toggle, View};
use crate::model::{PortType, Snapshot};
use crate::render;
use crate::session::Session;
use crate::stage::Stage;
use crate::syntax::{Ref, Selector, Statement};

/// A screen, by whatever number its tool gives it (a connection, a client id).
pub type ScreenId = usize;

/// A sentence's verb: the row TO the column, CUT from it, or ONLY into it (what else feeds
/// the column is cut).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    To,
    Cut,
    Only,
}

/// What a screen asks of the sheet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    /// The matrix as the screen's view shows it.
    Show,
    /// Sets the screen's view: every client (`all`) or the ones the tool shows; a cell to
    /// zoom into, port by port, or none; and how its ports are grouped, None being the
    /// sheet's own. Kept as given.
    View { all: bool, zoom: Option<CellKey>, grouping: Option<Grouping> },
    /// Stages what tapping the cell means, or unstages the line it staged.
    Toggle { cell: CellKey },
    /// Stages a sentence, the cell's row then its column, as one line.
    Stage { cell: CellKey, verb: Verb },
    /// Unstages the matrix's `staged` line at this index.
    Unstage { index: usize },
    /// Makes the screen's staged lines. Lines whose ports are gone stay staged. Warnings
    /// that ask (`Warning::ask`) refuse it, unless `anyway`.
    Apply { anyway: bool },
    /// Drops the screen's staged lines.
    Drop,
    /// Takes back the last change, whichever screen made it.
    Undo,
}

/// Why the sheet didn't do what it was asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// Bay's own: `kind` is an [`ErrorKind`]'s code (`type_mismatch`, `backend`, …).
    Patch {
        kind: String,
        text: String,
    },
    /// The tool's, from its [`Rules`], or the sheet's `reserved`.
    Rule {
        kind: String,
        text: String,
    },
    /// APPLY would bring about what these lines say, and wasn't told `anyway`.
    Ask(Vec<String>),
    Other(String),
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refused::Patch { text, .. } | Refused::Rule { text, .. } | Refused::Other(text) => {
                f.write_str(text)
            }
            Refused::Ask(lines) => f.write_str(&lines.join("; ")),
        }
    }
}

impl std::error::Error for Refused {}

fn patch(kind: ErrorKind, text: impl Into<String>) -> Refused {
    Refused::Patch { kind: kind.code().into(), text: text.into() }
}

/// Something a screen's staged lines would bring about, said before APPLY. One that asks
/// refuses APPLY until it's told `anyway`; one that doesn't is only said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Warning {
    pub text: String,
    pub ask: bool,
}

/// A tool's own rules over the sheet. Every one has a default that lets everything by.
pub trait Rules {
    /// May refuse a plan APPLY or UNDO would run against `snap`, with the tool's own
    /// refusal. Called before anything is run.
    fn check(&self, _plan: &Plan, _snap: &Snapshot) -> Result<(), Refused> {
        Ok(())
    }

    /// What the staged lines would bring about, the graph `now` and as it would be `after`.
    /// Called only when there's something staged.
    fn warnings(&self, _now: &Snapshot, _after: &Snapshot) -> Vec<Warning> {
        Vec::new()
    }

    /// Whether a client (by its raw name) shows while a screen isn't showing all.
    fn shown(&self, _client: &str) -> bool {
        true
    }

    /// Ports (by full name) nothing is patched to: hidden while a screen isn't showing all,
    /// and refused `reserved` in any view, after [`check`](Self::check).
    fn reserved(&self) -> Vec<String> {
        Vec::new()
    }

    /// Bundles with a port matching one of these come first.
    fn favourites(&self) -> Vec<Pattern> {
        Vec::new()
    }

    /// A line for the tool's log: what a change did, or (`warn`) what it couldn't.
    fn log(&self, _text: &str, _warn: bool) {}
}

/// Rules that let everything by.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoRules;

impl Rules for NoRules {}

/// A row or a column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Axis {
    /// The client as shown: a short name for a long one.
    pub client: String,
    /// A declared bundle's name, or one made from the ports (`capture_1/2`).
    pub label: String,
    /// It's a declared bundle (a tool's own, or a port named by [`Sheet::set_names`]).
    pub declared: bool,
    pub midi: bool,
    /// Its first port's full name, and how many ports.
    pub first: String,
    pub size: usize,
}

/// A staged line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Staged {
    pub mark: Mark,
    /// In bay's syntax.
    pub text: String,
    /// In the rows' and columns' labels, with the verbs: `capture_1 TO 2) Vln 2`,
    /// `capture_1 ONLY 2) Vln 2 · cuts capture_2`. As they were when it was staged.
    pub words: String,
    /// The links it makes and cuts, by full port name, alone against the graph now: for a
    /// tool that marks a line by where it goes. Empty when it's stale.
    pub links: Vec<NamedLink>,
}

/// The matrix as a screen shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Matrix {
    /// The grouping it was built with.
    pub grouping: Grouping,
    pub all: bool,
    /// The cell zoomed into, while the zoom applies.
    pub zoom: Option<CellKey>,
    pub rows: Vec<Axis>,
    pub cols: Vec<Axis>,
    /// A string a row, four characters a cell: connected now (`.` none, `#` full, `x`
    /// crossed, `+` partial, a space where the types differ), connected after the staged
    /// lines, the change (`=` none, `+` added, `-` cut, `*` both), and the tap (`s` stages,
    /// `u` unstages, `-` refused).
    pub cells: Vec<String>,
    pub staged: Vec<Staged>,
    /// Staged links out of view: added, and cut.
    pub unseen: (usize, usize),
    /// Bundles the view leaves out.
    pub hidden: usize,
    /// Declarations that didn't apply, a zoom whose bundles are gone.
    pub problems: Vec<String>,
    pub can_undo: bool,
    /// Moves when the graph changes, and with every apply, undo and change of names.
    pub count: u64,
    /// What the screen's staged lines would bring about ([`Rules::warnings`]).
    pub warnings: Vec<Warning>,
}

impl Matrix {
    /// Cell (r, c)'s four characters, or None past the edges.
    pub fn cell(&self, r: usize, c: usize) -> Option<&str> {
        self.cells.get(r)?.get(c * 4..c * 4 + 4)
    }

    /// Cell (r, c)'s key.
    pub fn key(&self, r: usize, c: usize) -> Option<CellKey> {
        let (row, col) = (self.rows.get(r)?, self.cols.get(c)?);
        Some((row.first.clone(), row.size, col.first.clone(), col.size))
    }
}

#[derive(Clone, Debug, Default)]
struct Screen {
    stage: Stage,
    /// Each staged line in words, as said when it was staged: a regroup doesn't change them.
    words: Vec<String>,
    all: bool,
    zoom: Option<CellKey>,
    grouping: Option<Grouping>,
}

pub struct Sheet<R: Rules = NoRules> {
    backend: Box<dyn Backend + Send>,
    rules: R,
    session: Session,
    snap: Snapshot,
    /// The backend's generation the snapshot was taken at; None: take it again.
    seen: Option<u64>,
    screens: BTreeMap<ScreenId, Screen>,
    /// Moves with every apply, undo and change of names, beside the backend's generation.
    changes: u64,
    /// Ports named by the tool ([`Sheet::set_names`]), and the names the snapshot placed.
    names: Vec<(String, String)>,
    named: Vec<String>,
}

impl Sheet<NoRules> {
    /// A sheet with no rules of a tool's, its ports grouped as `grouping` says unless a
    /// screen says otherwise.
    pub fn new(backend: Box<dyn Backend + Send>, grouping: Grouping) -> Sheet<NoRules> {
        Sheet::with_rules(backend, grouping, NoRules)
    }
}

impl<R: Rules> Sheet<R> {
    pub fn with_rules(backend: Box<dyn Backend + Send>, grouping: Grouping, rules: R) -> Sheet<R> {
        let mut session = Session::new();
        session.set_grouping(grouping);
        Sheet {
            backend,
            rules,
            session,
            snap: Snapshot::default(),
            seen: None,
            screens: BTreeMap::new(),
            changes: 0,
            names: Vec::new(),
            named: Vec::new(),
        }
    }

    pub fn rules(&self) -> &R {
        &self.rules
    }

    /// The rules, to change what they know (a deck's armed tracks).
    pub fn rules_mut(&mut self) -> &mut R {
        &mut self.rules
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    /// The sheet's own grouping, a screen's unless it says otherwise.
    pub fn grouping(&self) -> Grouping {
        self.session.grouping()
    }

    /// Moves when the graph changes, and with every apply, undo and change of names.
    pub fn count(&self) -> u64 {
        self.backend.generation() + self.changes
    }

    /// The graph now, taken again if it may have changed.
    pub fn snapshot(&mut self) -> Result<&Snapshot, Refused> {
        self.refresh()?;
        Ok(&self.snap)
    }

    /// Defines a tool's own names, from `bundle` and `alias` statements: a deck's tracks.
    /// All or nothing.
    pub fn define(&mut self, statement: &str) -> Result<(), Refused> {
        self.session.define(statement).map_err(|e| patch(e.kind(), e.msg()))?;
        self.changes += 1;
        Ok(())
    }

    /// Forgets a name [`define`](Self::define) made. False if there was none. A staged line
    /// naming it goes stale.
    pub fn undefine(&mut self, name: &str) -> bool {
        let gone = self.session.forget(name);
        if gone {
            self.changes += 1;
        }
        gone
    }

    /// Ports named after what their owner calls them (`system:capture_3` is `Analogue 3`),
    /// by full port name. Each is a bundle of the port's place among its client's, placed
    /// again with every snapshot. A name already taken leaves the port its own.
    pub fn set_names(&mut self, names: Vec<(String, String)>) {
        self.names = names;
        self.seen = None;
        self.changes += 1;
    }

    /// A screen has gone: its view and staged lines go with it.
    pub fn forget(&mut self, screen: ScreenId) {
        self.screens.remove(&screen);
    }

    /// Carries out a screen's request, and gives the matrix it sees after.
    pub fn op(&mut self, id: ScreenId, op: Op) -> Result<Matrix, Refused> {
        self.refresh()?;
        self.screens.entry(id).or_default();
        match op {
            Op::Show => {}
            Op::View { all, zoom, grouping } => {
                let s = self.screen_mut(id);
                (s.all, s.zoom, s.grouping) = (all, zoom, grouping);
            }
            Op::Toggle { cell } => self.toggle(id, cell)?,
            Op::Stage { cell, verb } => self.stage(id, cell, verb)?,
            Op::Unstage { index } => {
                if !self.unstage(id, index) {
                    return Err(Refused::Other(format!("there's no staged line {index}")));
                }
            }
            Op::Apply { anyway } => self.apply(id, anyway)?,
            Op::Drop => {
                let s = self.screen_mut(id);
                s.stage.clear();
                s.words.clear();
            }
            Op::Undo => self.undo()?,
        }
        self.refresh()?;
        Ok(self.matrix(id))
    }

    fn screen(&self, id: ScreenId) -> &Screen {
        &self.screens[&id]
    }

    fn screen_mut(&mut self, id: ScreenId) -> &mut Screen {
        self.screens.get_mut(&id).expect("op makes the screen")
    }

    /// Takes a new snapshot if the graph may have changed.
    fn refresh(&mut self) -> Result<(), Refused> {
        let generation = self.backend.generation();
        if self.seen != Some(generation) {
            self.snap = self.backend.snapshot().map_err(|e| patch(e.kind(), e.to_string()))?;
            self.seen = Some(generation);
            self.name_ports();
        }
        Ok(())
    }

    /// Names the ports [`set_names`](Self::set_names) gave, where the snapshot places them.
    fn name_ports(&mut self) {
        for name in self.named.drain(..) {
            self.session.forget(&name);
        }
        for (port, name) in &self.names {
            let taken = self.session.scope().bundles().any(|(b, _)| b == name);
            let Some(id) = self.snap.port_named(port).filter(|_| !taken) else { continue };
            let p = self.snap.port(id);
            let Some(place) = self.snap.ports_of(p.client(), p.ty(), p.dir()).iter().position(|&q| q == id)
            else {
                continue;
            };
            let place = place as u32 + 1;
            let sel = Selector { dir: Some(p.dir()), ty: Some(p.ty()), range: Some((place, place)) };
            let target = Ref { name: self.snap.client(p.client()).name().to_string(), sel };
            let stmt = Statement::Bundle { name: name.clone(), target };
            match self.session.define(&stmt.to_string()) {
                Ok(()) => self.named.push(name.clone()),
                Err(e) => self.rules.log(&format!("patch bay: {port} isn't named {name}: {}", e.msg()), true),
            }
        }
    }

    fn view(&self, s: &Screen) -> View {
        let hide = if s.all {
            Vec::new()
        } else {
            let clients = self.snap.clients().map(|(_, c)| c.name()).filter(|c| !self.rules.shown(c));
            clients.map(Pattern::new).chain(self.rules.reserved().iter().map(|p| Pattern::new(p))).collect()
        };
        View {
            zoom: s.zoom.clone(),
            filter: None,
            hide,
            favourites: self.rules.favourites(),
            grouping: s.grouping,
        }
    }

    fn build(&self, id: ScreenId) -> matrix::Matrix {
        let s = self.screen(id);
        matrix::Matrix::build(&self.snap, &self.session, &s.stage, &self.view(s))
    }

    /// The screen's matrix with every client shown and no zoom: where ONLY finds what it cuts
    /// and a line's words find its ends.
    fn wide(&self, id: ScreenId) -> matrix::Matrix {
        let s = self.screen(id);
        let all = View { hide: Vec::new(), zoom: None, ..self.view(s) };
        matrix::Matrix::build(&self.snap, &self.session, &s.stage, &all)
    }

    /// Stages a sentence as one line: the cell's row TO, CUT or ONLY its column. ONLY's cuts
    /// are what feeds the column with the staged lines made, among every client's bundles,
    /// not only the shown ones: what APPLY will do is in the line.
    fn stage(&mut self, id: ScreenId, key: CellKey, verb: Verb) -> Result<(), Refused> {
        let m = self.build(id);
        let Some((r, c)) = m.find(&key) else {
            return Err(Refused::Other("that cell isn't on the screen any more".into()));
        };
        let after = m.cell(r, c).after;
        if after == Cell::Types {
            return Err(patch(ErrorKind::TypeMismatch, "different port types"));
        }
        let (from, to) = (m.rows[r].typed.clone(), m.cols[c].typed.clone());
        let connected = after != Cell::Empty;
        let on = Statement::Connect { from: from.clone(), to: to.clone() };
        let off = |from| Statement::Disconnect { from, to: to.clone() };
        let stmts = match verb {
            Verb::To if after == Cell::Full => return Err(Refused::Other("already connected".into())),
            Verb::To => vec![on],
            Verb::Cut if !connected => return Err(Refused::Other("not connected".into())),
            Verb::Cut => vec![off(from)],
            Verb::Only => {
                let wide = self.wide(id);
                let (wide, c) = match wide.find(&key) {
                    Some((_, c)) => (&wide, c),
                    None => (&m, c),
                };
                let mut stmts: Vec<Statement> = (0..wide.rows.len())
                    .filter(|&o| {
                        (wide.rows[o].first.as_str(), wide.rows[o].bundle.ports.len()) != (&key.0, key.1)
                    })
                    .filter(|&o| !matches!(wide.cell(o, c).after, Cell::Empty | Cell::Types))
                    .map(|o| off(wide.rows[o].typed.clone()))
                    .collect();
                if after != Cell::Full {
                    stmts.push(on);
                }
                if stmts.is_empty() {
                    return Err(Refused::Other("already only that".into()));
                }
                stmts
            }
        };
        let text = stmts.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ");
        let (session, snap) = (&self.session, &self.snap);
        let s = self.screens.get_mut(&id).expect("op makes the screen");
        s.stage.add_at(session, snap, &text, None).map_err(|e| patch(e.kind(), e.msg()))?;
        self.say_last(id);
        Ok(())
    }

    fn unstage(&mut self, id: ScreenId, index: usize) -> bool {
        let s = self.screen_mut(id);
        let gone = s.stage.unstage(index);
        if gone {
            s.words.remove(index);
        }
        gone
    }

    /// Says the line just staged in words: its ends found among every client's bundles
    /// (ONLY's cuts can be ports this view hides), a shown client's by its label, another's
    /// by its client too.
    fn say_last(&mut self, id: ScreenId) {
        let w = self.wide(id);
        let named = |axes: &[matrix::Axis], r: &Ref| match axes.iter().find(|a| a.typed == *r) {
            Some(a) if self.rules.shown(&a.client) => a.bundle.label.clone(),
            Some(a) => format!("{} {}", a.shown, a.bundle.label),
            None => r.to_string(),
        };
        let said = match self.screen(id).stage.inputs().last() {
            Some(input) => {
                let stmts: Vec<&Statement> = input.stmts.iter().map(|s| &s.stmt).collect();
                words(&stmts, |r| named(&w.rows, r), |r| named(&w.cols, r))
            }
            None => return,
        };
        self.screen_mut(id).words.push(said);
    }

    fn toggle(&mut self, id: ScreenId, key: CellKey) -> Result<(), Refused> {
        let m = self.build(id);
        let Some((r, c)) = m.find(&key) else {
            return Err(Refused::Other("that cell isn't on the screen any more".into()));
        };
        match &m.cell(r, c).toggle {
            Ok(Toggle::Unstage(i)) => {
                self.unstage(id, *i);
            }
            Ok(Toggle::Stage(text)) => {
                let (session, snap) = (&self.session, &self.snap);
                let s = self.screens.get_mut(&id).expect("op makes the screen");
                s.stage.add_at(session, snap, text, Some(key)).map_err(|e| patch(e.kind(), e.msg()))?;
                self.say_last(id);
            }
            Err(problem) => return Err(patch(problem.kind, problem.msg.clone())),
        }
        Ok(())
    }

    /// The tool's rules, then the reserved ports.
    fn check(&self, plan: &Plan) -> Result<(), Refused> {
        self.rules.check(plan, &self.snap)?;
        let links = plan.links_named(&self.snap);
        let reserved = self.rules.reserved();
        if let Some(p) = reserved.iter().find(|p| links.iter().any(|l| l.from == **p || l.to == **p)) {
            return Err(Refused::Rule {
                kind: "reserved".into(),
                text: format!("{p} is reserved: nothing is patched to it"),
            });
        }
        Ok(())
    }

    fn warnings(&self, plan: &Plan) -> Vec<Warning> {
        if plan.is_empty() {
            return Vec::new();
        }
        self.rules.warnings(&self.snap, &self.snap.with_plan(plan))
    }

    fn apply(&mut self, id: ScreenId, anyway: bool) -> Result<(), Refused> {
        let (plan, _) = self.screen(id).stage.plan(&self.session, &self.snap);
        self.check(&plan)?;
        let asks: Vec<String> = self.warnings(&plan).into_iter().filter(|w| w.ask).map(|w| w.text).collect();
        if !anyway && !asks.is_empty() {
            return Err(Refused::Ask(asks));
        }
        let (session, snap) = (&self.session, &self.snap);
        let s = self.screens.get_mut(&id).expect("op makes the screen");
        let (plan, stale) = s.stage.take(session, snap);
        // The stale lines stay, and their words with them.
        let mut i = 0;
        s.words.retain(|_| {
            i += 1;
            stale.iter().any(|st| st.input == i - 1)
        });
        if plan.is_empty() {
            return Ok(());
        }
        let report = self.backend.run(&self.snap, &plan);
        self.session.record(&self.snap, &report.applied);
        self.logged("patch", &report.applied, &report.failures);
        self.changes += 1;
        self.seen = None;
        Ok(())
    }

    fn undo(&mut self) -> Result<(), Refused> {
        // Tried on a copy: a refused undo stays on the stack.
        let mut trial = self.session.clone();
        let Some(undo) = trial.undo(&self.snap) else {
            return Err(Refused::Other("nothing to undo".into()));
        };
        self.check(&undo.plan)?;
        self.session = trial;
        let report = self.backend.run(&self.snap, &undo.plan);
        self.logged("patch undo", &report.applied, &report.failures);
        if !undo.unmatched.is_empty() {
            self.rules.log(&format!("patch undo: gone: {}", undo.unmatched.join(", ")), true);
        }
        self.changes += 1;
        self.seen = None;
        Ok(())
    }

    /// Logs what a change did, as patch statements, and what it couldn't.
    fn logged(&self, what: &str, done: &Plan, failures: &[Failure]) {
        let stmts: Vec<String> = render::plan(&self.snap, done).iter().map(ToString::to_string).collect();
        if !stmts.is_empty() {
            self.rules.log(&format!("{what}: {}", stmts.join("; ")), false);
        }
        for f in failures {
            let (from, to) = (self.snap.port(f.link.from).name(), self.snap.port(f.link.to).name());
            let op = if f.connect { ">" } else { "!>" };
            self.rules.log(&format!("{what}: {from} {op} {to} failed: {}", f.reason), true);
        }
    }

    fn matrix(&mut self, id: ScreenId) -> Matrix {
        let m = self.build(id);
        let (plan, _) = self.screen(id).stage.plan(&self.session, &self.snap);
        let warnings = self.warnings(&plan);
        if !m.zoomed {
            self.screen_mut(id).zoom = None;
        }
        let s = self.screen(id);
        let axis = |a: &matrix::Axis| Axis {
            client: a.shown.clone(),
            label: a.bundle.label.clone(),
            declared: a.bundle.declared,
            midi: a.bundle.ty == PortType::Midi,
            first: a.first.clone(),
            size: a.bundle.ports.len(),
        };
        let cells = (0..m.rows.len())
            .map(|r| {
                let mut row = String::with_capacity(m.cols.len() * 4);
                for c in 0..m.cols.len() {
                    let v = m.cell(r, c);
                    row.push(state(v.now));
                    row.push(state(v.after));
                    row.push(match v.change {
                        Change::None => '=',
                        Change::Add => '+',
                        Change::Remove => '-',
                        Change::Both => '*',
                    });
                    row.push(match v.toggle {
                        Ok(Toggle::Stage(_)) => 's',
                        Ok(Toggle::Unstage(_)) => 'u',
                        Err(_) => '-',
                    });
                }
                row
            })
            .collect();
        let staged = (m.staged.iter().enumerate())
            .map(|(i, l)| Staged {
                mark: l.mark,
                text: l.text.clone(),
                words: s.words.get(i).cloned().unwrap_or_default(),
                links: match m.stale.iter().any(|st| st.input == i) {
                    true => Vec::new(),
                    false => (self.session.compile(&self.snap, &l.text, Options::default()))
                        .map(|p| p.links_named(&self.snap))
                        .unwrap_or_default(),
                },
            })
            .collect();
        Matrix {
            grouping: s.grouping.unwrap_or(self.session.grouping()),
            all: s.all,
            zoom: if m.zoomed { s.zoom.clone() } else { None },
            rows: m.rows.iter().map(axis).collect(),
            cols: m.cols.iter().map(axis).collect(),
            cells,
            staged,
            unseen: (m.unseen.add, m.unseen.del),
            hidden: m.hidden,
            problems: m.problems.clone(),
            can_undo: self.session.can_undo(),
            count: self.count(),
            warnings,
        }
    }
}

fn state(c: Cell) -> char {
    match c {
        Cell::Types => ' ',
        Cell::Empty => '.',
        Cell::Full => '#',
        Cell::Crossed => 'x',
        Cell::Partial => '+',
    }
}

/// A staged line in words: `capture_3 TO 1) Kick In`, `… CUT …`, and ONLY's (cuts into one
/// destination, then a connect into it) as said, with what it cuts:
/// `capture_3 ONLY 12) Vln · cuts capture_2`. from and to name a statement's ends.
fn words(stmts: &[&Statement], from: impl Fn(&Ref) -> String, to: impl Fn(&Ref) -> String) -> String {
    if let [cuts @ .., Statement::Connect { from: f, to: t }] = stmts
        && !cuts.is_empty()
        && cuts.iter().all(|s| matches!(s, Statement::Disconnect { to: c, .. } if c == t))
    {
        let cut: Vec<String> = (cuts.iter())
            .filter_map(|s| match s {
                Statement::Disconnect { from: x, .. } => Some(from(x)),
                _ => None,
            })
            .collect();
        return format!("{} ONLY {} · cuts {}", from(f), to(t), cut.join(", "));
    }
    (stmts.iter())
        .map(|s| match s {
            Statement::Connect { from: f, to: t } => format!("{} TO {}", from(f), to(t)),
            Statement::Disconnect { from: f, to: t } => format!("{} CUT {}", from(f), to(t)),
            other => other.to_string(),
        })
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests;
