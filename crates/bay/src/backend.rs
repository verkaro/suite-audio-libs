//! What a backend does for bay: report the graph, run plans, and say when the graph may
//! have changed. The trait does no I/O itself; the fake backend ([`crate::fake`]) and
//! the JACK one (`as-bay-jack`) implement it.

use std::fmt;

use crate::compile::Plan;
use crate::model::{Link, Snapshot};

pub trait Backend {
    /// The graph now. Ports of types other than audio and MIDI are left out. Whether the
    /// backend's own client shows is the backend's choice: bay's JACK backend leaves `bay`
    /// out, and Dexter's would show Dexter's ports, which are the point there.
    fn snapshot(&mut self) -> Result<Snapshot, BackendError>;

    /// Runs `plan`, whose ids are `snap`'s: deletions, then additions, continuing past
    /// failures. What's already as wanted counts as applied.
    fn run(&mut self, snap: &Snapshot, plan: &Plan) -> Report;

    /// Goes up whenever the graph may have changed. Owners rebuild their snapshot when it
    /// moves.
    fn generation(&self) -> u64;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendError(pub String);

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl BackendError {
    pub fn kind(&self) -> crate::error::ErrorKind {
        crate::error::ErrorKind::Backend
    }
}

impl std::error::Error for BackendError {}

/// What running a plan did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// The part of the plan that holds now; this is what undo records.
    pub applied: Plan,
    pub failures: Vec<Failure>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub link: Link,
    /// Whether it was a connect (else a disconnect).
    pub connect: bool,
    pub reason: String,
}

/// Runs a plan through `op(from, to, connect)`, which takes full port names and returns
/// Ok when the link is as wanted afterwards: deletions first, then additions.
pub fn execute(
    snap: &Snapshot,
    plan: &Plan,
    mut op: impl FnMut(&str, &str, bool) -> Result<(), String>,
) -> Report {
    let mut report = Report::default();
    let steps = plan.del.iter().map(|l| (l, false)).chain(plan.add.iter().map(|l| (l, true)));
    for (&link, connect) in steps {
        match op(snap.port(link.from).name(), snap.port(link.to).name(), connect) {
            Ok(()) if connect => {
                report.applied.add.insert(link);
            }
            Ok(()) => {
                report.applied.del.insert(link);
            }
            Err(reason) => report.failures.push(Failure { link, connect, reason }),
        }
    }
    report
}
