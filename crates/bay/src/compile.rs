//! Compiling statements to a [`Plan`]: the smallest set of links to add and delete to
//! bring a snapshot to what the statements say (`SYNTAX.md`, "Meaning of a patch").

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::error::{ErrorKind, Problem};
use crate::model::{ClientId, Direction, Link, PortId, Snapshot};
use crate::resolve::{Resolver, Scope};
use crate::syntax::{Ref, Selector, Spanned, Statement};

/// Links to delete and add. Execution deletes first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub del: BTreeSet<Link>,
    pub add: BTreeSet<Link>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.del.is_empty() && self.add.is_empty()
    }

    /// Every link the plan touches by full port name, in the order a backend runs them:
    /// deletions, then additions. An owner can check a plan with it before running it:
    /// Dexter refuses changes into armed tracks during REC.
    pub fn links_named(&self, snap: &Snapshot) -> Vec<NamedLink> {
        let named = |l: &Link, add| NamedLink {
            from: snap.port(l.from).name().to_owned(),
            to: snap.port(l.to).name().to_owned(),
            add,
        };
        self.del.iter().map(|l| named(l, false)).chain(self.add.iter().map(|l| named(l, true))).collect()
    }
}

/// A link by full `client:port` names, and whether a plan adds it (else deletes it).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NamedLink {
    pub from: String,
    pub to: String,
    pub add: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Treat every source client the statements mention as `exact` (`apply --exact`).
    pub exact_all: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompileError {
    pub line: usize,
    pub kind: ErrorKind,
    pub msg: String,
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.msg)
    }
}

impl std::error::Error for CompileError {}

/// Compiles one input's statements against a snapshot. Its `alias` and `bundle`
/// statements apply from where they stand to the end of the input and never reach
/// `session`. Later statements win for the same link.
pub fn compile(
    stmts: &[Spanned],
    snap: &Snapshot,
    session: &Scope,
    opts: Options,
) -> Result<Plan, CompileError> {
    let mut c = Compiler::new(snap, session, opts);
    c.input(stmts)?;
    Ok(c.finish())
}

/// Compiles one input, skipping the statements that don't compile ([`Compiler::input_tolerant`]).
pub fn compile_tolerant(
    stmts: &[Spanned],
    snap: &Snapshot,
    session: &Scope,
    opts: Options,
) -> (Plan, Vec<Skipped>) {
    let mut c = Compiler::new(snap, session, opts);
    let skipped = c.input_tolerant(stmts);
    (c.finish(), skipped)
}

/// A statement a tolerant compile left out, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skipped {
    /// Its place in the input, or None for an `exact` that `exact_all` implied.
    pub index: Option<usize>,
    pub stmt: Statement,
    pub error: CompileError,
}

/// Compiles several inputs in turn (staged edits, say). Each input has its own `alias`
/// and `bundle` scope, and later statements win across inputs.
pub struct Compiler<'a> {
    snap: &'a Snapshot,
    session: &'a Scope,
    opts: Options,
    /// The final say on each link a statement touched: true to have it, false not to.
    want: BTreeMap<Link, bool>,
    exact: BTreeSet<ClientId>,
}

impl<'a> Compiler<'a> {
    pub fn new(snap: &'a Snapshot, session: &'a Scope, opts: Options) -> Compiler<'a> {
        Compiler { snap, session, opts, want: BTreeMap::new(), exact: BTreeSet::new() }
    }

    /// Adds one input. On an error the compiler is left as it was before the input.
    pub fn input(&mut self, stmts: &[Spanned]) -> Result<(), CompileError> {
        let mut local = Scope::default();
        let mut want = self.want.clone();
        let mut exact = self.exact.clone();
        for s in stmts {
            self.statement(s, &mut local, &mut want, &mut exact)?;
        }
        self.want = want;
        self.exact = exact;
        Ok(())
    }

    /// Adds one input, skipping each statement that doesn't compile (a client that isn't
    /// there, say) and keeping the rest, with the input's own aliases in scope throughout.
    /// Restoring a snapshot needs this: an interface that's unplugged shouldn't stop the
    /// rest from coming back.
    ///
    /// A skipped statement never lets `exact` cut what it would have kept: a client that a
    /// skipped connect starts from isn't made exact (from its `exact` line or from
    /// `exact_all`), and that's reported as a skip of kind `withheld`. Deletions only
    /// ever come from statements that were read.
    pub fn input_tolerant(&mut self, stmts: &[Spanned]) -> Vec<Skipped> {
        let mut local = Scope::default();
        let (mut want, mut exact) = (std::mem::take(&mut self.want), std::mem::take(&mut self.exact));
        let before: BTreeSet<ClientId> = exact.clone();
        let mut skipped = Vec::new();
        for (index, s) in stmts.iter().enumerate() {
            if let Err(error) = self.statement(s, &mut local, &mut want, &mut exact) {
                skipped.push(Skipped { index: Some(index), stmt: s.stmt.clone(), error });
            }
        }

        // The clients the skipped connects start from, as far as they can still be named.
        let res = Resolver::new(self.snap, vec![&local, self.session]);
        let mut unread: BTreeMap<ClientId, usize> = BTreeMap::new();
        for k in &skipped {
            if let Statement::Connect { from, .. } = &k.stmt {
                let client = from.name.split_once(':').map_or(from.name.as_str(), |(c, _)| c);
                if let Ok(c) = res.client_of(client) {
                    *unread.entry(c).or_default() += 1;
                }
            }
        }
        for (&c, &n) in &unread {
            if before.contains(&c) || !exact.remove(&c) {
                continue;
            }
            let name = self.snap.client(c).name().to_owned();
            // Its own `exact` line, if it has one.
            let index = stmts.iter().position(
                |s| matches!(&s.stmt, Statement::Exact { client } if res.client_of(client).ok() == Some(c)),
            );
            let line = index.map_or(0, |i| stmts[i].line);
            let msg = format!("{n} statement(s) from it were skipped");
            let error = CompileError { line, kind: ErrorKind::Withheld, msg };
            skipped.push(Skipped { index, stmt: Statement::Exact { client: name }, error });
        }
        self.want = want;
        self.exact = exact;
        skipped
    }

    /// One statement. On an error nothing is changed.
    fn statement(
        &self,
        s: &Spanned,
        local: &mut Scope,
        want: &mut BTreeMap<Link, bool>,
        exact: &mut BTreeSet<ClientId>,
    ) -> Result<(), CompileError> {
        let (snap, session) = (self.snap, self.session);
        let err = |p: Problem| CompileError { line: s.line, kind: p.kind, msg: p.msg };
        match &s.stmt {
            Statement::Alias { name, target } => local.alias(name, target).map_err(err)?,
            Statement::Bundle { name, target } => {
                // Check it first, so a bad bundle is reported where it's declared.
                let mut with = local.clone();
                with.bundle(name, target).map_err(err)?;
                Resolver::new(snap, vec![&with, session]).bundle_decl(name, target).map_err(err)?;
                *local = with;
            }
            Statement::Exact { client } => {
                exact.insert(Resolver::new(snap, vec![&*local, session]).client_of(client).map_err(err)?);
            }
            Statement::Connect { from, to } | Statement::Disconnect { from, to } => {
                let res = Resolver::new(snap, vec![&*local, session]);
                let (outs, ins) = both_sides(snap, &res, from, to).map_err(err)?;
                if matches!(s.stmt, Statement::Connect { .. }) {
                    for link in pair(snap, &outs, &ins).map_err(err)? {
                        want.insert(link, true);
                    }
                } else {
                    for &from in &outs {
                        for &to in &ins {
                            let link = Link { from, to };
                            if snap.is_linked(from, to) || want.contains_key(&link) {
                                want.insert(link, false);
                            }
                        }
                    }
                }
                if self.opts.exact_all {
                    exact.extend(outs.iter().map(|&p| snap.port(p).client()));
                }
            }
        }
        Ok(())
    }

    pub fn finish(mut self) -> Plan {
        let snap = self.snap;
        for &c in &self.exact {
            for &from in snap.client(c).ports() {
                for to in snap.links_from(from) {
                    self.want.entry(Link { from, to }).or_insert(false);
                }
            }
        }
        let mut plan = Plan::default();
        for (link, on) in self.want {
            match (on, snap.is_linked(link.from, link.to)) {
                (true, false) => plan.add.insert(link),
                (false, true) => plan.del.insert(link),
                _ => false,
            };
        }
        plan
    }
}

/// Resolves both sides of `>`. When their types differ and one side has no type word,
/// that side follows the other's type (`a2j > REAPER` links MIDI).
fn both_sides(
    snap: &Snapshot,
    res: &Resolver,
    from: &Ref,
    to: &Ref,
) -> Result<(Vec<PortId>, Vec<PortId>), Problem> {
    let mut outs = res.ports(from, Direction::Out)?;
    let mut ins = res.ports(to, Direction::In)?;
    let (to_ty, from_ty) = (snap.port(ins[0]).ty(), snap.port(outs[0]).ty());
    if from_ty != to_ty {
        let retyped = |r: &Ref, ty| Ref { name: r.name.clone(), sel: Selector { ty: Some(ty), ..r.sel } };
        if to.sel.ty.is_none()
            && let Ok(p) = res.ports(&retyped(to, from_ty), Direction::In)
        {
            ins = p;
        } else if from.sel.ty.is_none()
            && let Ok(p) = res.ports(&retyped(from, to_ty), Direction::Out)
        {
            outs = p;
        }
    }
    Ok((outs, ins))
}

/// Pairs outputs with inputs for a connect: equal counts zip, one output fans out, one
/// input sums. Both ends of every pair must carry the same type.
pub fn pair(snap: &Snapshot, outs: &[PortId], ins: &[PortId]) -> Result<Vec<Link>, Problem> {
    let links: Vec<Link> = if outs.len() == ins.len() {
        outs.iter().zip(ins).map(|(&from, &to)| Link { from, to }).collect()
    } else if outs.len() == 1 {
        ins.iter().map(|&to| Link { from: outs[0], to }).collect()
    } else if ins.len() == 1 {
        outs.iter().map(|&from| Link { from, to: ins[0] }).collect()
    } else {
        return Err(Problem::new(
            ErrorKind::Pairing,
            format!("can't map {} outputs onto {} inputs; use ranges of equal size", outs.len(), ins.len()),
        ));
    };
    for l in &links {
        let (f, t) = (snap.port(l.from), snap.port(l.to));
        if f.ty() != t.ty() {
            let msg = format!("{} is {} but {} is {}", f.name(), f.ty().as_str(), t.name(), t.ty().as_str());
            return Err(Problem::new(ErrorKind::TypeMismatch, msg));
        }
    }
    Ok(links)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;
    use crate::syntax::parse;

    fn snap() -> Snapshot {
        let text = include_str!("../testdata/studio.fake").to_owned()
            + "out midi a2j:midi_capture_1\nin midi REAPER:MIDI in\n";
        fixture::parse(&text).unwrap()
    }

    /// The plan as `-`/`+` lines of full port names, deletions first.
    fn plan_with(text: &str, opts: Options) -> Result<Vec<String>, String> {
        let s = snap();
        let stmts = parse(text).map_err(|e| e.to_string())?;
        let plan = compile(&stmts, &s, &Scope::default(), opts).map_err(|e| e.to_string())?;
        let line =
            |sign: char, l: &Link| format!("{sign} {} > {}", s.port(l.from).name(), s.port(l.to).name());
        Ok(plan.del.iter().map(|l| line('-', l)).chain(plan.add.iter().map(|l| line('+', l))).collect())
    }

    fn plan(text: &str) -> Result<Vec<String>, String> {
        plan_with(text, Options::default())
    }

    #[test]
    fn connect_pairs() {
        assert_eq!(
            plan("REAPER[5-6] > OBS").unwrap(),
            ["+ REAPER:out5 > OBS:input_FL", "+ REAPER:out6 > OBS:input_FR"]
        );
        assert_eq!(
            plan("Firefox[2] > OBS").unwrap(),
            ["+ Firefox:output_FR > OBS:input_FL", "+ Firefox:output_FR > OBS:input_FR"]
        );
        assert_eq!(
            plan("REAPER[5-6] > OBS[2]").unwrap(),
            ["+ REAPER:out5 > OBS:input_FR", "+ REAPER:out6 > OBS:input_FR"]
        );
        assert_eq!(plan("a2j > rea").unwrap(), ["+ a2j:midi_capture_1 > REAPER:MIDI in"]);
        assert_eq!(
            plan("REAPER[midi] > OBS").unwrap_err(),
            "line 1: REAPER[midi]: REAPER has no midi outputs"
        );
        // An explicit type word doesn't follow the other side.
        assert_eq!(
            plan("a2j > REAPER[audio 1]").unwrap_err(),
            "line 1: a2j:midi_capture_1 is midi but REAPER:in1 is audio"
        );
        // Already linked: nothing to do.
        assert_eq!(plan("Carla > REAPER[3-4]").unwrap(), Vec::<String>::new());
        assert_eq!(
            plan("zz > OBS\nREAPER > OBS").unwrap_err(),
            "line 1: no client, bundle or alias matches zz"
        );
        assert_eq!(
            plan("REAPER > OBS").unwrap_err(),
            "line 1: can't map 6 outputs onto 2 inputs; use ranges of equal size"
        );
        assert_eq!(
            plan("a2j > OBS[1]").unwrap_err(),
            "line 1: a2j:midi_capture_1 is midi but OBS:input_FL is audio"
        );
    }

    #[test]
    fn plans_name_the_links_they_touch() {
        let s = snap();
        let plan = compile(
            &parse("Carla !> REAPER; REAPER[5-6] > OBS").unwrap(),
            &s,
            &Scope::default(),
            Options::default(),
        )
        .unwrap();
        let named: Vec<(String, String, bool)> =
            plan.links_named(&s).into_iter().map(|l| (l.from, l.to, l.add)).collect();
        let l = |f: &str, t: &str, add| (f.to_owned(), t.to_owned(), add);
        assert_eq!(
            named,
            [
                l("Carla:audio-out1", "REAPER:in3", false),
                l("Carla:audio-out2", "REAPER:in4", false),
                l("REAPER:out5", "OBS:input_FL", true),
                l("REAPER:out6", "OBS:input_FR", true),
            ]
        );
        // What an owner would check: which inputs the plan changes.
        let inputs: BTreeSet<String> = plan.links_named(&s).into_iter().map(|l| l.to).collect();
        assert!(inputs.contains("REAPER:in3") && !inputs.contains("REAPER:in1"));
        assert!(Plan::default().links_named(&s).is_empty());
    }

    #[test]
    fn tolerant_compile_skips_what_is_not_there() {
        let s = snap();
        let text = "alias fx = Carla\nexact Gone\nexact fx\nfx > REAPER[3-4]\nGone[1] > REAPER[1]\nbundle b = Gone[out 1-2]\nREAPER[5-6] > OBS\n";
        let (plan, skipped) =
            compile_tolerant(&parse(text).unwrap(), &s, &Scope::default(), Options::default());
        let lines: Vec<(usize, String)> =
            skipped.iter().map(|k| (k.error.line, k.error.kind.code().to_owned())).collect();
        assert_eq!(
            lines,
            [(2, "unknown_name".into()), (5, "unknown_name".into()), (6, "unknown_name".into())]
        );
        // The rest applies, with the file's alias in scope: Carla stays as it is (exact,
        // and restated), and REAPER 5-6 go to OBS.
        let named: Vec<String> =
            plan.links_named(&s).iter().map(|l| format!("{} {} {}", l.add, l.from, l.to)).collect();
        assert_eq!(named, ["true REAPER:out5 OBS:input_FL", "true REAPER:out6 OBS:input_FR"]);
        // The strict compile refuses the same text at its first problem.
        assert_eq!(
            compile(&parse(text).unwrap(), &s, &Scope::default(), Options::default()).unwrap_err().line,
            2
        );
    }

    #[test]
    fn disconnect_takes_every_link_between() {
        // 2 outputs, 4 inputs: no pairing needed.
        assert_eq!(
            plan("Carla !> REAPER").unwrap(),
            ["- Carla:audio-out1 > REAPER:in3", "- Carla:audio-out2 > REAPER:in4"]
        );
        assert_eq!(plan("REAPER[3] !> Carla").unwrap(), ["- REAPER:out3 > Carla:audio-in1"]);
        assert_eq!(plan("Firefox !> REAPER").unwrap(), Vec::<String>::new());
    }

    #[test]
    fn later_statements_win() {
        assert_eq!(plan("REAPER[5] > OBS[1]; REAPER[5] !> OBS").unwrap(), Vec::<String>::new());
        assert_eq!(
            plan("Carla !> REAPER; Carla[1] > REAPER[3]").unwrap(),
            ["- Carla:audio-out2 > REAPER:in4"]
        );
    }

    #[test]
    fn exact() {
        let phones = "alsa_output.usb-Focusrite_Scarlett_2i2_USB-00.analog-stereo";
        assert_eq!(
            plan("exact REAPER\nREAPER[1-2] > alsa_output").unwrap(),
            ["- REAPER:out3 > Carla:audio-in1", "- REAPER:out4 > Carla:audio-in2"]
        );
        // --exact covers every source client mentioned, disconnects included.
        let got =
            plan_with("REAPER[1-2] > alsa_output\nFirefox !> REAPER", Options { exact_all: true }).unwrap();
        assert_eq!(
            got,
            [
                "- REAPER:out3 > Carla:audio-in1",
                "- REAPER:out4 > Carla:audio-in2",
                "- Firefox:output_FL > OBS:input_FL",
            ]
        );
        assert!(plan(&format!("exact {phones}")).unwrap().is_empty());
    }

    #[test]
    fn patch_names() {
        assert_eq!(
            plan("alias fx = Carla\nbundle send = REAPER[out 5-6]\nsend > fx").unwrap(),
            ["+ REAPER:out5 > Carla:audio-in1", "+ REAPER:out6 > Carla:audio-in2"]
        );
        assert_eq!(
            plan("fx > OBS\nalias fx = Carla").unwrap_err(),
            "line 1: no client, bundle or alias matches fx"
        );
        assert_eq!(
            plan("alias fx = Carla\nalias fx = OBS").unwrap_err(),
            "line 2: fx is already defined differently"
        );
        assert_eq!(
            plan("bundle b = REAPER[out 5-9]").unwrap_err(),
            "line 1: REAPER[out 5-9]: REAPER has 6 audio outputs"
        );
    }
}
