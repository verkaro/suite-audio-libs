//! A Focusrite interface run by the kernel's `scarlett2` driver: its routing and its mixer,
//! through the card's controls. Moved in from Dexter's `crates/focusrite` at `e98d44f`
//! (2026-11-10), for a Focusrite app of its own (`focusrite-ctrl`). The `docs/` named here and
//! below are Dexter's (`verkaro/dexter`), where it was written: `docs/monitor.md`, "On a
//! Focusrite interface", is the design.
//!
//! - [`card`]: the control device, through the kernel's ioctls.
//! - [`controls`]: the controls as listed, and [`Fake`], a card made from a list (the tests'
//!   18i20, `testdata/18i20-gen3.txt`, read from Warren's).
//! - [`interface`]: a Focusrite as its controls describe it.
//! - [`plan`]: what's set on it for a project's mixes and tracks.
//! - [`mixer`]: setting it, and keeping its gains with the deck on a thread of its own.
//! - [`panel`]: its own controls beside the mixer: the inputs' switches, 48V, the clock, DIM
//!   and MUTE (`docs/interface.md`).

pub mod card;
pub mod controls;
pub mod interface;
pub mod mixer;
pub mod panel;
pub mod plan;

pub use card::Card;
pub use controls::{Control, Controls, Fake, Kind, format_line};
pub use interface::{Focusrite, Line, Volume};
pub use mixer::{Levels, Link, Mixer, Watch, keep_found, put_back, targets, write_loop};
pub use panel::{Change, InputNow, Now, Panel, SharedControls};
pub use plan::{Loop, Plan, Print, Request, Setting, Settings, Switches, Want};

/// The 18i20 (3rd gen) as read from Warren's on 2026-10-13, for the tests.
pub const SCARLETT_18I20_GEN3: &str = include_str!("../testdata/18i20-gen3.txt");

/// Every control's line in the text form ([`controls`]), with its values now.
pub fn dump(c: &mut dyn Controls) -> std::io::Result<String> {
    let mut out = String::new();
    for k in c.list()? {
        let v = if k.readable && k.kind != Kind::Other {
            c.read(k.numid).unwrap_or_default()
        } else {
            Vec::new()
        };
        out += &format_line(&k, &v);
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
