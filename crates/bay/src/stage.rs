//! One screen's staged edits. The [`Session`] holds what screens share (names, bundle
//! declarations, undo); a `Stage` holds the inputs one screen has staged and not yet
//! applied, compiled with the session's names whenever they're read. An owner serving
//! several screens keeps one `Stage` per screen and one `Session` for all of them.

use crate::compile::{CompileError, Compiler, Options, Plan};
use crate::matrix::CellKey;
use crate::model::Snapshot;
use crate::session::{Error, Session};
use crate::syntax::{self, Spanned};

/// One staged input: its statements, and the matrix cell it toggles if it came from one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Staged {
    pub stmts: Vec<Spanned>,
    pub key: Option<CellKey>,
}

impl Staged {
    /// The statements on one line.
    pub fn text(&self) -> String {
        self.stmts.iter().map(|s| s.stmt.to_string()).collect::<Vec<_>>().join("; ")
    }
}

/// A staged input that no longer compiles against the current snapshot. It stays staged
/// (the client may come back) but is left out of the plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stale {
    /// Index into [`Stage::inputs`].
    pub input: usize,
    pub error: CompileError,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stage {
    /// One entry per staged input (a typed line, a cursor action).
    inputs: Vec<Staged>,
}

impl Stage {
    pub fn new() -> Stage {
        Stage::default()
    }

    /// Stages an input if it compiles against `snap` after the inputs already staged, with
    /// the session's names. Returns the staged plan.
    pub fn add(&mut self, session: &Session, snap: &Snapshot, text: &str) -> Result<Plan, Error> {
        self.add_at(session, snap, text, None)
    }

    /// Stages an input, noting the matrix cell it toggles (so that cell can unstage it).
    pub fn add_at(
        &mut self,
        session: &Session,
        snap: &Snapshot,
        text: &str,
        key: Option<CellKey>,
    ) -> Result<Plan, Error> {
        let stmts = syntax::parse(text).map_err(Error::Syntax)?;
        let plan = {
            let mut c = self.compiler(session, snap).0;
            c.input(&stmts).map_err(Error::Compile)?;
            c.finish()
        };
        if !stmts.is_empty() {
            self.inputs.push(Staged { stmts, key });
        }
        Ok(plan)
    }

    pub fn inputs(&self) -> &[Staged] {
        &self.inputs
    }

    pub fn is_empty(&self) -> bool {
        self.inputs.is_empty()
    }

    /// The staged inputs as statements, one input per line.
    pub fn text(&self) -> String {
        self.inputs.iter().map(|i| i.text() + "\n").collect()
    }

    /// The staged edits compiled against `snap` with the session's names, and the inputs
    /// that no longer compile.
    pub fn plan(&self, session: &Session, snap: &Snapshot) -> (Plan, Vec<Stale>) {
        let (c, stale) = self.compiler(session, snap);
        (c.finish(), stale)
    }

    fn compiler<'a>(&'a self, session: &'a Session, snap: &'a Snapshot) -> (Compiler<'a>, Vec<Stale>) {
        let mut c = Compiler::new(snap, session.scope(), Options::default());
        let mut stale = Vec::new();
        for (input, Staged { stmts, .. }) in self.inputs.iter().enumerate() {
            if let Err(error) = c.input(stmts) {
                stale.push(Stale { input, error });
            }
        }
        (c, stale)
    }

    /// Drops one staged input. False if there's no such input.
    pub fn unstage(&mut self, input: usize) -> bool {
        if input >= self.inputs.len() {
            return false;
        }
        self.inputs.remove(input);
        true
    }

    /// Drops the latest staged input. False if nothing was staged.
    pub fn unstage_last(&mut self) -> bool {
        self.inputs.pop().is_some()
    }

    /// Drops every staged input.
    pub fn clear(&mut self) {
        self.inputs.clear();
    }

    /// Takes the staged edits to run: the plan against `snap`. Stale inputs are left out
    /// of it and stay staged (their client may come back); the returned `Stale`s index the
    /// inputs as they were before the call, and the kept ones stay in that order.
    pub fn take(&mut self, session: &Session, snap: &Snapshot) -> (Plan, Vec<Stale>) {
        let (plan, stale) = self.plan(session, snap);
        let mut i = 0;
        self.inputs.retain(|_| {
            let keep = stale.iter().any(|s| s.input == i);
            i += 1;
            keep
        });
        (plan, stale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;

    fn studio() -> Snapshot {
        fixture::parse(include_str!("../testdata/studio.fake")).unwrap()
    }

    /// Two screens share the session's names, and each stages, plans and takes its own.
    #[test]
    fn screens_stage_apart() {
        let s = studio();
        let mut session = Session::new();
        session.define("alias o = OBS").unwrap();
        let (mut a, mut b) = (Stage::new(), Stage::new());
        a.add(&session, &s, "REAPER[5-6] > o").unwrap();
        b.add(&session, &s, "Firefox[2] > REAPER[1]").unwrap();
        assert_eq!(a.text(), "REAPER[5-6] > o\n");
        assert_eq!(b.text(), "Firefox[2] > REAPER[1]\n");

        let (plan, stale) = a.take(&session, &s);
        assert!(stale.is_empty() && plan.add.len() == 2);
        assert!(a.is_empty());
        assert_eq!(b.inputs().len(), 1, "the other screen's edits stay");

        // A name the session forgets makes the inputs that use it stale, on every stage.
        a.add(&session, &s, "REAPER[5-6] > o").unwrap();
        assert!(session.forget("o"));
        assert_eq!(a.plan(&session, &s).1.len(), 1);
        assert!(b.plan(&session, &s).1.is_empty());
    }
}
