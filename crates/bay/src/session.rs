//! The session: the one owner of state that outlives a snapshot (`PLAN.md` §3). It
//! holds what every screen shares, the user's names and undo, and it's passed explicitly;
//! each screen's staged edits are a [`Stage`](crate::Stage) of its own. It does
//! no I/O: the front end reads and writes `aliases.patch` and runs plans on a backend.

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::bundle::{Bundles, Grouping};
use crate::compile::{CompileError, Compiler, Options, Plan};
use crate::error::ErrorKind;
use crate::identity::match_ports;
use crate::model::{Link, Snapshot};
use crate::resolve::{Definition, Resolver, Scope};
use crate::syntax::{self, Statement, SyntaxError};
use crate::views::Names;

/// How many undo steps are kept.
const UNDO_DEPTH: usize = 100;

/// Client names longer than this get a generated alias when printed.
const LONG_NAME: usize = 20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Syntax(SyntaxError),
    Compile(CompileError),
    /// Session names take only `alias` and `bundle` statements.
    NotADefinition {
        line: usize,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Syntax(e) => e.fmt(f),
            Error::Compile(e) => e.fmt(f),
            Error::NotADefinition { line } => write!(f, "line {line}: only alias and bundle define names"),
        }
    }
}

impl Error {
    pub fn kind(&self) -> ErrorKind {
        match self {
            Error::Syntax(e) => e.kind(),
            Error::Compile(e) => e.kind,
            Error::NotADefinition { .. } => ErrorKind::NotADefinition,
        }
    }

    /// The 1-based line of the input it's on.
    pub fn line(&self) -> usize {
        match self {
            Error::Syntax(e) => e.line,
            Error::Compile(e) => e.line,
            Error::NotADefinition { line } => *line,
        }
    }

    /// The message, without the line.
    pub fn msg(&self) -> &str {
        match self {
            Error::Syntax(e) => &e.msg,
            Error::Compile(e) => &e.msg,
            Error::NotADefinition { .. } => "only alias and bundle define names",
        }
    }
}

impl std::error::Error for Error {}

/// What undo will do, and the ports of the undone step that are gone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Undo {
    pub plan: Plan,
    /// Full names of old ports with no counterpart in the current snapshot.
    pub unmatched: Vec<String>,
}

#[derive(Clone, Debug)]
struct Step {
    /// The snapshot the step ran against; `inverse` uses its ids.
    snap: Snapshot,
    inverse: Plan,
}

#[derive(Clone, Debug, Default)]
pub struct Session {
    scope: Scope,
    undo: Vec<Step>,
    /// How ports nobody declared are grouped into bundles.
    grouping: Grouping,
}

impl Session {
    pub fn new() -> Session {
        Session::default()
    }

    pub fn scope(&self) -> &Scope {
        &self.scope
    }

    /// How ports nobody declared are grouped: by their names (the default), or each on its own.
    pub fn set_grouping(&mut self, g: Grouping) {
        self.grouping = g;
    }

    pub fn grouping(&self) -> Grouping {
        self.grouping
    }

    /// Loads `aliases.patch`: its `alias` and `bundle` lines define session names, later
    /// lines replacing earlier ones. Other statements are skipped; their lines are
    /// returned.
    pub fn load_names(&mut self, text: &str) -> Result<Vec<usize>, SyntaxError> {
        let mut skipped = Vec::new();
        for s in syntax::parse(text)? {
            match definition(s.stmt) {
                Some(d) => self.scope.set(d),
                None => skipped.push(s.line),
            }
        }
        Ok(skipped)
    }

    /// Defines session names from typed `alias` and `bundle` statements (the TUI's
    /// `:alias`). All or nothing.
    pub fn define(&mut self, text: &str) -> Result<(), Error> {
        let mut defs = Vec::new();
        for s in syntax::parse(text).map_err(Error::Syntax)? {
            defs.push(definition(s.stmt).ok_or(Error::NotADefinition { line: s.line })?);
        }
        for d in defs {
            self.scope.set(d);
        }
        Ok(())
    }

    /// Forgets a session name. False if there was none.
    pub fn forget(&mut self, name: &str) -> bool {
        self.scope.remove(name)
    }

    /// The session names in `aliases.patch` form: aliases, then bundles, each sorted.
    pub fn names_text(&self) -> String {
        let mut aliases: Vec<_> = self.scope.aliases().collect();
        aliases.sort();
        let mut bundles: Vec<_> = self.scope.bundles().collect();
        bundles.sort_by_key(|(n, _)| *n);
        let mut out = String::new();
        for (name, target) in aliases {
            out += &format!("{}\n", Statement::Alias { name: name.into(), target: target.into() });
        }
        for (name, target) in bundles {
            out += &format!("{}\n", Statement::Bundle { name: name.into(), target: target.clone() });
        }
        out
    }

    /// The names views print clients by: the shortest user alias that resolves to the
    /// client, else a generated alias for a long name, else the raw name.
    pub fn names(&self, snap: &Snapshot) -> Names {
        let res = Resolver::new(snap, vec![&self.scope]);
        let mut user: HashMap<String, String> = HashMap::new();
        for (alias, _) in self.scope.aliases() {
            let Ok(c) = res.client_of(alias) else { continue };
            let client = snap.client(c).name();
            let better = |cur: &String| (alias.len(), alias) < (cur.len(), cur.as_str());
            if client != alias && user.get(client).is_none_or(better) {
                user.insert(client.to_owned(), alias.to_owned());
            }
        }

        let mut taken: HashSet<String> = snap.clients().map(|(_, c)| c.name().to_owned()).collect();
        taken.extend(self.scope.aliases().map(|(n, _)| n.to_owned()));
        taken.extend(self.scope.bundles().map(|(n, _)| n.to_owned()));
        let mut names = Names::default();
        for (_, c) in snap.clients() {
            if let Some(alias) = user.get(c.name()) {
                names.alias(c.name(), alias);
            } else if let Some(base) = short_name(c.name()) {
                let alias = (1..)
                    .map(|n| if n == 1 { base.clone() } else { format!("{base}-{n}") })
                    .find(|a| !taken.contains(a))
                    .expect("some suffix is free");
                taken.insert(alias.clone());
                names.generated(c.name(), &alias);
            }
        }
        names
    }

    /// The snapshot's bundles with the session's declarations, and the declarations that
    /// didn't apply.
    pub fn bundles(&self, snap: &Snapshot) -> (Bundles, Vec<String>) {
        let (decls, problems) = Resolver::new(snap, vec![&self.scope]).bundle_decls();
        let mut problems: Vec<String> = problems.into_iter().map(|p| p.msg).collect();
        let (bundles, more) = Bundles::derive_with(snap, &decls, self.grouping);
        problems.extend(more.iter().map(ToString::to_string));
        (bundles, problems)
    }

    /// Compiles one input (a patch file for `apply`) with the session's names.
    pub fn compile(&self, snap: &Snapshot, text: &str, opts: Options) -> Result<Plan, Error> {
        let stmts = syntax::parse(text).map_err(Error::Syntax)?;
        let mut c = Compiler::new(snap, &self.scope, opts);
        c.input(&stmts).map_err(Error::Compile)?;
        Ok(c.finish())
    }

    /// Compiles one input with the session's names, skipping the statements that don't
    /// compile (restoring a snapshot with a client missing). Syntax errors still stop it.
    pub fn compile_tolerant(
        &self,
        snap: &Snapshot,
        text: &str,
        opts: Options,
    ) -> Result<(Plan, Vec<crate::compile::Skipped>), SyntaxError> {
        let stmts = syntax::parse(text)?;
        Ok(crate::compile::compile_tolerant(&stmts, snap, &self.scope, opts))
    }

    /// Records what a backend actually changed in `before`, so undo can reverse it. Only
    /// forward steps are recorded, not the undo plans themselves.
    pub fn record(&mut self, before: &Snapshot, done: &Plan) {
        if done.is_empty() {
            return;
        }
        let inverse = Plan { del: done.add.clone(), add: done.del.clone() };
        self.undo.push(Step { snap: before.clone(), inverse });
        if self.undo.len() > UNDO_DEPTH {
            self.undo.remove(0);
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Pops the latest step and returns the plan that reverses it against `snap`. Ports
    /// carry over by [`match_ports`]. Links whose ports are gone are left out and their
    /// ports reported, and links already as wanted are dropped.
    pub fn undo(&mut self, snap: &Snapshot) -> Option<Undo> {
        let step = self.undo.pop()?;
        let (old_b, _) = self.bundles(&step.snap);
        let (new_b, _) = self.bundles(snap);
        let m = match_ports(&step.snap, &old_b, snap, &new_b);
        let mut unmatched = Vec::new();
        let mut map = |l: &Link| {
            let ends = [l.from, l.to].map(|p| m.get(p).map(|(n, _)| n));
            for (old, new) in [l.from, l.to].iter().zip(ends) {
                let name = step.snap.port(*old).name().to_owned();
                if new.is_none() && !unmatched.contains(&name) {
                    unmatched.push(name);
                }
            }
            match ends {
                [Some(from), Some(to)] => Some(Link { from, to }),
                _ => None,
            }
        };
        let del =
            step.inverse.del.iter().filter_map(&mut map).filter(|l| snap.is_linked(l.from, l.to)).collect();
        let add =
            step.inverse.add.iter().filter_map(&mut map).filter(|l| !snap.is_linked(l.from, l.to)).collect();
        Some(Undo { plan: Plan { del, add }, unmatched })
    }
}

fn definition(stmt: Statement) -> Option<Definition> {
    match stmt {
        Statement::Alias { name, target } => Some(Definition::Alias { name, target }),
        Statement::Bundle { name, target } => Some(Definition::Bundle { name, target }),
        _ => None,
    }
}

/// Words that say nothing about which device a name is.
const STOP: [&str; 11] =
    ["usb", "pci", "platform", "alsa", "input", "output", "card", "device", "stereo", "mono", "surround"];

/// A short alias base for a long client name, or None when it's short enough.
/// `alsa_output.usb-Focusrite_Scarlett_2i2_USB-00.analog-stereo` → `focusrite-out`:
/// a `<kind>_input.`/`<kind>_output.` prefix gives `-in`/`-out`, and the first telling
/// word of the rest (3+ characters with a letter, not a stop word) is the base, else the
/// kind (`bluez`).
fn short_name(name: &str) -> Option<String> {
    if name.chars().count() <= LONG_NAME {
        return None;
    }
    let (kind, dir, rest) = match name.split_once('.') {
        Some((head, rest)) if head.ends_with("_input") => (head.strip_suffix("_input"), Some("in"), rest),
        Some((head, rest)) if head.ends_with("_output") => (head.strip_suffix("_output"), Some("out"), rest),
        _ => (None, None, name),
    };
    let word = rest
        .split(|c: char| !c.is_alphanumeric())
        .find(|w| {
            w.chars().count() >= 3
                && w.chars().any(char::is_alphabetic)
                && !STOP.contains(&w.to_lowercase().as_str())
        })
        .or(kind)?;
    let base: String = word.to_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    if base.is_empty() {
        return None;
    }
    let short = match dir {
        Some(d) => format!("{base}-{d}"),
        None => base,
    };
    (short.len() < name.len()).then_some(short)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;
    use crate::stage::Stage;
    use crate::views::{self, Layout};

    const FOCUSRITE_IN: &str = "alsa_input.usb-Focusrite_Scarlett_2i2_USB-00.analog-stereo";
    const FOCUSRITE_OUT: &str = "alsa_output.usb-Focusrite_Scarlett_2i2_USB-00.analog-stereo";

    fn studio() -> Snapshot {
        fixture::parse(include_str!("../testdata/studio.fake")).unwrap()
    }

    /// The session's grouping reaches its bundles: mono makes every undeclared port its own.
    #[test]
    fn grouping() {
        let snap = studio();
        let mut s = Session::new();
        let guessed = s.bundles(&snap).0.iter().count();
        s.set_grouping(Grouping::Mono);
        assert_eq!(s.grouping(), Grouping::Mono);
        let (mono, _) = s.bundles(&snap);
        assert!(mono.iter().count() > guessed, "{guessed} bundles guessed");
        assert!(mono.iter().all(|(_, b)| b.ports.len() == 1));
    }

    /// `snap` with `plan` applied, as a backend would.
    fn apply(snap: &Snapshot, plan: &Plan) -> Snapshot {
        let mut b = Snapshot::builder();
        for (_, p) in snap.ports() {
            b.port(p.name(), p.ty(), p.dir()).unwrap();
        }
        for l in snap.links().iter().filter(|l| !plan.del.contains(l)).chain(&plan.add) {
            b.link(snap.port(l.from).name(), snap.port(l.to).name()).unwrap();
        }
        b.build()
    }

    fn show(snap: &Snapshot, plan: &Plan) -> String {
        views::statements(crate::render::plan(snap, plan), &Names::default())
    }

    /// One real input for each error kind, with its code: together they cover them all.
    #[test]
    fn every_error_kind_has_a_cause() {
        let s = studio();
        let mut session = Session::new();
        let compile = |text: &str| Session::new().compile(&s, text, Options::default()).unwrap_err().kind();
        let cases = [
            ("syntax", compile("REAPER[1 > OBS")),
            ("syntax", compile("Firefox:output_FL[1] > OBS")),
            ("unknown_name", compile("zz > OBS")),
            ("ambiguous_name", compile("alsa > REAPER")),
            ("wrong_side", compile("REAPER[in] > OBS")),
            ("wrong_side", compile("OBS:input_FL > REAPER")),
            ("out_of_range", compile("REAPER[7] > OBS")),
            ("out_of_range", compile("OBS > REAPER")),
            ("pairing", compile("REAPER > OBS")),
            ("type_mismatch", compile("bundle b = REAPER[out 1-2]; b[midi] > OBS")),
            ("redefined", compile("alias x = Carla; alias x = OBS")),
            ("alias_loop", compile("alias l = l; l > OBS")),
            ("not_a_definition", session.define("REAPER > OBS").unwrap_err().kind()),
            ("backend", crate::backend::BackendError("gone".into()).kind()),
            ("withheld", {
                let (_, skipped) = Session::new()
                    .compile_tolerant(&s, "exact REAPER\nREAPER:out1 > Gone:in", Options::default())
                    .unwrap();
                skipped.last().unwrap().error.kind
            }),
        ];
        for (code, kind) in &cases {
            assert_eq!(kind.code(), *code);
        }
        let seen: HashSet<ErrorKind> = cases.iter().map(|(_, k)| *k).collect();
        assert!(ErrorKind::ALL.iter().all(|k| seen.contains(k)), "a kind with no case");
    }

    #[test]
    fn short_names() {
        assert_eq!(short_name(FOCUSRITE_OUT).as_deref(), Some("focusrite-out"));
        assert_eq!(short_name(FOCUSRITE_IN).as_deref(), Some("focusrite-in"));
        assert_eq!(short_name("alsa_output.pci-0000_00_1f.3.analog-stereo").as_deref(), Some("analog-out"));
        assert_eq!(short_name("bluez_output.00_1B_66_AA_BB_CC.1").as_deref(), Some("bluez-out"));
        assert_eq!(short_name("Midi-Through Port-0 long").as_deref(), Some("midi"));
        assert_eq!(short_name("REAPER"), None);
    }

    #[test]
    fn names_prefer_user_aliases_then_generated() {
        let s = studio();
        let mut session = Session::new();
        session.define("alias phones = alsa_out; alias headphones = alsa_out; alias daw = REAPER").unwrap();
        let names = session.names(&s);
        assert_eq!(names.get(FOCUSRITE_OUT), "phones");
        assert_eq!(names.get(FOCUSRITE_IN), "focusrite-in");
        assert_eq!(names.get("REAPER"), "daw");
        assert_eq!(names.get("Carla"), "Carla");

        // Generated names avoid session names and each other.
        let mut session = Session::new();
        session.define("alias focusrite-in = Carla").unwrap();
        let names = session.names(&s);
        assert_eq!(names.get(FOCUSRITE_IN), "focusrite-in-2");
        assert_eq!(names.get(FOCUSRITE_OUT), "focusrite-out");
    }

    #[test]
    fn ls_with_session_names_reads_back() {
        let s = studio();
        let mut session = Session::new();
        session.define("alias fx = Carla\nbundle drums = REAPER[out 3-4]").unwrap();
        let text = views::ls(&s, &session.names(&s), Layout::default());
        assert!(text.starts_with(&format!("alias focusrite-in = {FOCUSRITE_IN}\n")), "{text}");
        assert!(text.contains("REAPER[3-4] > fx\n"), "{text}");
        assert!(session.compile(&s, &text, Options { exact_all: true }).unwrap().is_empty());
        // Also with a fresh session, which has none of these names.
        assert!(Session::new().compile(&s, &text, Options { exact_all: true }).unwrap().is_empty());
    }

    #[test]
    fn names_file() {
        let mut session = Session::new();
        let skipped = session
            .load_names("alias b = x\n# c\nA > B\nbundle d = R[out 1-2]\nalias a = y\nalias b = z\n")
            .unwrap();
        assert_eq!(skipped, [3]);
        assert_eq!(session.names_text(), "alias a = y\nalias b = z\nbundle d = R[out 1-2]\n");
        assert_eq!(
            session.define("A > B").unwrap_err().to_string(),
            "line 1: only alias and bundle define names"
        );
        assert!(session.forget("d"));
        assert!(!session.forget("d"));
        // A bundle replaces an alias of the same name.
        session.define("bundle a = R[in]").unwrap();
        assert_eq!(session.names_text(), "alias b = z\nbundle a = R[in]\n");
    }

    #[test]
    fn session_bundles() {
        let s = studio();
        let mut session = Session::new();
        session.define("bundle stems = REAPER[out 3-6]; bundle gone = Nope[out 1-2]").unwrap();
        let (b, problems) = session.bundles(&s);
        assert_eq!(problems, ["no client, bundle or alias matches Nope"]);
        let out3 = s.port_named("REAPER:out3").unwrap();
        assert_eq!(b.get(b.of_port(out3).0).label, "stems");
    }

    #[test]
    fn staging() {
        let s = studio();
        let session = Session::new();
        let mut stage = Stage::new();
        let plan = stage.add(&session, &s, "REAPER[5-6] > OBS").unwrap();
        assert_eq!(show(&s, &plan), "REAPER[5-6] > OBS\n");
        // Each input has its own aliases; later inputs win.
        let plan = stage.add(&session, &s, "alias o = OBS; REAPER[6] !> o").unwrap();
        assert_eq!(show(&s, &plan), "REAPER[5] > OBS[1]\n");
        assert!(stage.add(&session, &s, "o > REAPER").is_err());
        assert_eq!(stage.inputs().len(), 2);
        assert_eq!(stage.text(), "REAPER[5-6] > OBS\nalias o = OBS; REAPER[6] !> o\n");

        // Against a snapshot without OBS, the inputs that name it go stale but stay staged.
        let no_obs = fixture::parse(
            &include_str!("../testdata/studio.fake")
                .lines()
                .filter(|l| !l.contains("OBS"))
                .map(|l| format!("{l}\n"))
                .collect::<String>(),
        )
        .unwrap();
        let (plan, stale) = stage.plan(&session, &no_obs);
        assert!(plan.is_empty());
        assert_eq!(stale.iter().map(|x| x.input).collect::<Vec<_>>(), [0, 1]);
        assert_eq!(stage.inputs().len(), 2);

        assert!(stage.unstage_last());
        let (plan, stale) = stage.take(&session, &s);
        assert_eq!((show(&s, &plan).as_str(), stale.len()), ("REAPER[5-6] > OBS\n", 0));
        assert!(stage.inputs().is_empty());

        // Taking the staged edits keeps the stale ones staged.
        stage.add(&session, &no_obs, "REAPER[5-6] > OBS").unwrap_err();
        stage.add(&session, &s, "REAPER[5-6] > OBS").unwrap();
        stage.add(&session, &s, "Firefox[2] > REAPER[1]").unwrap();
        let (plan, stale) = stage.take(&session, &no_obs);
        assert_eq!(show(&no_obs, &plan), "Firefox[2] > REAPER[1]\n");
        assert_eq!(stale.iter().map(|x| x.input).collect::<Vec<_>>(), [0]);
        assert_eq!(stage.text(), "REAPER[5-6] > OBS\n");
    }

    #[test]
    fn undo_reverses_across_snapshots() {
        let s = studio();
        let mut session = Session::new();
        let plan = session.compile(&s, "Carla !> REAPER; REAPER[5-6] > OBS", Options::default()).unwrap();
        let after = apply(&s, &plan);
        session.record(&s, &plan);

        let undo = session.undo(&after).unwrap();
        assert_eq!(show(&after, &undo.plan), "REAPER !> OBS\nCarla > REAPER[3-4]\n");
        assert!(undo.unmatched.is_empty());
        assert!(!session.can_undo());

        // REAPER re-registers with renamed ports and OBS is gone: undo still finds REAPER's
        // ports by bundle position, and reports OBS's.
        session.record(&s, &plan);
        let text: String = fixture::render(&after)
            .lines()
            .filter(|l| !l.contains("OBS"))
            .map(|l| l.replace("REAPER:out", "REAPER:track").replace("REAPER:in", "REAPER:rec") + "\n")
            .collect();
        let later = fixture::parse(&text).unwrap();
        let undo = session.undo(&later).unwrap();
        assert_eq!(show(&later, &undo.plan), "Carla > REAPER[3-4]\n");
        assert_eq!(undo.unmatched, ["OBS:input_FL", "OBS:input_FR"]);
        assert!(session.undo(&later).is_none());
    }
}
