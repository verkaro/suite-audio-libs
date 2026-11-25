//! A Mackie Control surface (first, the PreSonus FaderPort V2) played against a desk:
//! [`desk`], the seam, what a surface plays; [`binding`], the surface's rules, pure functions
//! of its events, the time and the desk's state; [`run`], the loop between them; [`testbed`], a
//! silent desk to try them on; and [`lock`], so one program at a time is the surface. A tool
//! brings its adapter, a [`desk::Desk`]. Dexter's `crates/surface` less its adapter, at
//! `e98d44f`, itself from dexter-surface at `87ea53a`, moved 2026-11-25
//! (`../../RFC-as-surface.md`, `../../VENDORED.md`). The rules are described in Dexter's
//! `docs/faderport.md`.

pub mod binding;
pub mod desk;
pub mod lock;
pub mod run;
pub mod testbed;
