//! Studio names: the rig's own, rarely changed (`alias booth-1 = system:capture_1`), kept
//! per box, so they hold only while that box is the one the names reach. bay keeps them in
//! `~/.config/bay/studio.patch` (Warren, 2026-11-27; bay-rs `docs/dev/DESIGN-studio-names.md`).
//!
//! The text is patch syntax in blocks: a line `interface ID` starts each box's names, ID being
//! the box's id as the suite's `as-interfaces` gives it (its `/dev/snd/by-id` name). Only `alias`
//! and `bundle` statements go in a block. Which boxes are here is the caller's to say: this
//! crate does no I/O ([`crate::Session::set_studio`]).

use std::fmt;

use crate::syntax::{self, Statement, SyntaxError};

/// One box's names: its id, and its statements in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Block {
    pub id: String,
    pub stmts: Vec<Statement>,
}

/// The studio names, box by box, in the file's order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Studio {
    pub blocks: Vec<Block>,
}

impl Studio {
    /// Reads `studio.patch`. A statement before the first `interface` line, or one that isn't
    /// `alias` or `bundle`, is an error on its line.
    pub fn parse(text: &str) -> Result<Studio, SyntaxError> {
        let mut studio = Studio::default();
        for (i, raw) in text.lines().enumerate() {
            let line = i + 1;
            let body = raw.split('#').next().unwrap_or("").trim();
            if body.is_empty() {
                continue;
            }
            if let Some(id) = body.strip_prefix("interface ") {
                let id = id.trim();
                if id.is_empty() || id.contains(char::is_whitespace) {
                    return Err(SyntaxError { line, msg: "interface takes one id".into() });
                }
                studio.blocks.push(Block { id: id.into(), stmts: Vec::new() });
                continue;
            }
            let Some(block) = studio.blocks.last_mut() else {
                return Err(SyntaxError {
                    line,
                    msg: "a studio name needs its box first: interface ID".into(),
                });
            };
            for s in syntax::parse(raw).map_err(|e| SyntaxError { line, msg: e.msg })? {
                if !matches!(s.stmt, Statement::Alias { .. } | Statement::Bundle { .. }) {
                    return Err(SyntaxError { line, msg: "only alias and bundle go in studio names".into() });
                }
                block.stmts.push(s.stmt);
            }
        }
        Ok(studio)
    }

    /// Adds `alias` and `bundle` statements to `id`'s block, made if there's none, each
    /// replacing a name the block already has.
    pub fn define(&mut self, id: &str, text: &str) -> Result<(), SyntaxError> {
        let mut new = Vec::new();
        for s in syntax::parse(text)? {
            if !matches!(s.stmt, Statement::Alias { .. } | Statement::Bundle { .. }) {
                return Err(SyntaxError {
                    line: s.line,
                    msg: "only alias and bundle go in studio names".into(),
                });
            }
            new.push(s.stmt);
        }
        let at = match self.blocks.iter().position(|b| b.id == id) {
            Some(at) => at,
            None => {
                self.blocks.push(Block { id: id.into(), stmts: Vec::new() });
                self.blocks.len() - 1
            }
        };
        let block = &mut self.blocks[at];
        for stmt in new {
            match block.stmts.iter().position(|s| name_of(s) == name_of(&stmt)) {
                Some(i) => block.stmts[i] = stmt,
                None => block.stmts.push(stmt),
            }
        }
        Ok(())
    }

    /// The block for `id`, if there's one.
    pub fn block(&self, id: &str) -> Option<&Block> {
        self.blocks.iter().find(|b| b.id == id)
    }
}

/// The text of `studio.patch`, which [`Studio::parse`] reads back the same.
impl fmt::Display for Studio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, b) in self.blocks.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            writeln!(f, "interface {}", b.id)?;
            for s in &b.stmts {
                writeln!(f, "{s}")?;
            }
        }
        Ok(())
    }
}

fn name_of(s: &Statement) -> Option<&str> {
    match s {
        Statement::Alias { name, .. } | Statement::Bundle { name, .. } => Some(name),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "# the rig\ninterface usb-Focusrite_Scarlett_2i2_USB_Y8A-00\nalias booth-1 = system:capture_1\n\
                        bundle mics = system[out 1-2]\n\ninterface usb-Other_Box_B-00\nalias phones = system:playback_1\n";

    #[test]
    fn reads_back_and_defines() {
        let mut studio = Studio::parse(TEXT).unwrap();
        assert_eq!(studio.blocks.len(), 2);
        assert_eq!(studio.blocks[0].stmts.len(), 2);
        assert_eq!(Studio::parse(&studio.to_string()).unwrap(), studio);
        // A name again replaces it; a new box gets a block.
        studio.define("usb-Focusrite_Scarlett_2i2_USB_Y8A-00", "alias booth-1 = system:capture_2").unwrap();
        studio.define("usb-New-00", "alias vox = system:capture_1").unwrap();
        assert_eq!(studio.blocks[0].stmts[0].to_string(), "alias booth-1 = system:capture_2");
        assert_eq!(studio.blocks.len(), 3);
        assert!(studio.define("usb-New-00", "a > b").is_err());
    }

    #[test]
    fn errors_say_where() {
        assert_eq!(Studio::parse("alias a = b\n").unwrap_err().line, 1);
        let e = Studio::parse("interface x\nREAPER > OBS\n").unwrap_err();
        assert_eq!((e.line, e.msg.as_str()), (2, "only alias and bundle go in studio names"));
        assert!(Studio::parse("interface\n").is_err());
    }
}
