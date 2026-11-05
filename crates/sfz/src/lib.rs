//! An SFZ instrument written as text, and checked as it's written, so a name or a value that
//! would break a sampler's reading is an error here, not a silent bad file. Written for
//! dexter-like110, 2026-11-04; scape may read SFZ one day, so the types are plain.

use std::fmt;

#[cfg(test)]
mod tests;

/// An SFZ file: comments first, then sections.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sfz {
    /// Each written as `// ` and the line.
    pub comments: Vec<String>,
    /// Headers in order, each with its opcodes in order.
    pub sections: Vec<Section>,
}

/// A header (`control`, `global`, `region`) and its opcodes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    pub header: String,
    pub opcodes: Vec<(String, String)>,
}

/// What can't be written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Invalid {
    Header(String),
    Opcode(String),
    Value(String),
}

impl fmt::Display for Invalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Invalid::Header(s) => write!(f, "not a header: {s:?}"),
            Invalid::Opcode(s) => write!(f, "not an opcode name: {s:?}"),
            Invalid::Value(s) => write!(f, "not a value: {s:?}"),
        }
    }
}

impl std::error::Error for Invalid {}

impl Sfz {
    /// The file's text. Headers are `[a-z]+` and opcode names `[a-z0-9_]+`; a value is ASCII,
    /// not empty, and has no newline, `<`, `=` or `//`; a comment has no newline and is ASCII.
    pub fn to_text(&self) -> Result<String, Invalid> {
        let mut out = String::new();
        for c in &self.comments {
            if !c.is_ascii() || c.contains(['\n', '\r']) {
                return Err(Invalid::Value(c.clone()));
            }
            out.push_str("// ");
            out.push_str(c);
            out.push('\n');
        }
        for s in &self.sections {
            if s.header.is_empty() || !s.header.bytes().all(|b| b.is_ascii_lowercase()) {
                return Err(Invalid::Header(s.header.clone()));
            }
            out.push('<');
            out.push_str(&s.header);
            out.push('>');
            for (name, value) in &s.opcodes {
                if name.is_empty()
                    || !name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                {
                    return Err(Invalid::Opcode(name.clone()));
                }
                if value.is_empty()
                    || !value.is_ascii()
                    || value.contains(['\n', '\r', '<', '='])
                    || value.contains("//")
                {
                    return Err(Invalid::Value(value.clone()));
                }
                out.push(' ');
                out.push_str(name);
                out.push('=');
                out.push_str(value);
            }
            out.push('\n');
        }
        Ok(out)
    }
}
