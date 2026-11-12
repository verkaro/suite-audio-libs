//! The patch bay's core: the graph model, bundles, patch syntax, compile, render and session
//! state. No I/O and no audio dependencies; backends and front ends sit on top.
//!
//! Moved in from Dexter's `bay-core` at `e98d44f` (2026-11-11), for jackbay (`RFC-as-bay.md`,
//! b1). It's Warren's bay, bay-rs's `crates/core` at `af5c0c7`, which Dexter vendored on
//! 2026-09-29. As it was but for its name. The `PLAN.md` and `SYNTAX.md` its comments name are
//! bay-rs's own, not brought here yet: `PLAN.md` §3 is the design centre, and `SYNTAX.md` the
//! patch syntax, which `syntax.rs` and its tests say meanwhile.

pub mod backend;
pub mod bundle;
pub mod compile;
pub mod error;
pub mod fake;
pub mod fixture;
pub mod identity;
pub mod matrix;
pub mod model;
pub mod render;
pub mod resolve;
pub mod session;
pub mod stage;
pub mod syntax;
pub mod undolog;
pub mod views;

pub use backend::{Backend, BackendError, Failure, Report};
pub use bundle::{Bundle, BundleDecl, BundleId, Bundles, Cell, DeclProblem, Grouping};
pub use compile::{CompileError, Compiler, NamedLink, Options, Plan, Skipped, compile, compile_tolerant};
pub use error::{ErrorKind, Problem};
pub use identity::{MatchKind, Matching, match_ports};
pub use matrix::{
    Axis, CellKey, CellView, Change, Filter, Mark, Matrix, Pattern, StagedLine, Toggle, Unseen, View,
};
pub use model::{
    Client, ClientId, Direction, Link, ModelError, Port, PortId, PortType, Snapshot, SnapshotBuilder,
};
pub use resolve::{Definition, Resolver, Scope};
pub use session::{Session, Undo};
pub use stage::{Stage, Staged, Stale};
pub use syntax::{Ref, Selector, Spanned, Statement, SyntaxError, parse};
pub use views::{Layout, Names, SnapshotFilter};
