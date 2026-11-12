//! The fake backend's text format, for fixtures like `testdata/studio.fake`:
//!
//! ```text
//! # comment
//! out REAPER:out1              # a port: direction, optional type (audio by default), name
//! in midi REAPER:MIDI Input
//! link REAPER:out1 > system:playback_1
//! ```
//!
//! Ports register in file order. A port name runs to the end of the line (or a ` #`
//! comment), so it may contain spaces. [`render`] prints a snapshot back in this format.

use std::fmt::{self, Write};

use crate::model::{Direction, PortType, Snapshot};

/// A problem at a 1-based line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixtureError {
    pub line: usize,
    pub msg: String,
}

impl fmt::Display for FixtureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.msg)
    }
}

impl std::error::Error for FixtureError {}

pub fn parse(text: &str) -> Result<Snapshot, FixtureError> {
    let mut b = Snapshot::builder();
    for (i, raw) in text.lines().enumerate() {
        let err = |msg: String| FixtureError { line: i + 1, msg };
        let line = match raw.find(" #") {
            Some(c) => &raw[..c],
            None if raw.trim_start().starts_with('#') => "",
            None => raw,
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (word, rest) = line.split_once([' ', '\t']).unwrap_or((line, ""));
        let rest = rest.trim_start();
        match word {
            "out" | "in" => {
                let dir = if word == "out" { Direction::Out } else { Direction::In };
                let (ty, name) = match rest.split_once([' ', '\t']) {
                    Some(("audio", n)) => (PortType::Audio, n.trim_start()),
                    Some(("midi", n)) => (PortType::Midi, n.trim_start()),
                    _ => (PortType::Audio, rest),
                };
                b.port(name, ty, dir).map_err(|e| err(e.to_string()))?;
            }
            "link" => {
                let (from, to) =
                    rest.split_once(" > ").ok_or_else(|| err("expected link A:b > C:d".into()))?;
                b.link(from.trim(), to.trim()).map_err(|e| err(e.to_string()))?;
            }
            _ => return Err(err(format!("unknown line kind {word:?}"))),
        }
    }
    Ok(b.build())
}

/// Prints ports in registration order, then links. Audio ports carry no type word.
pub fn render(snap: &Snapshot) -> String {
    let mut out = String::new();
    for (_, p) in snap.ports() {
        let ty = if p.ty() == PortType::Midi { "midi " } else { "" };
        let _ = writeln!(out, "{} {ty}{}", p.dir().as_str(), p.name());
    }
    for l in snap.links() {
        let _ = writeln!(out, "link {} > {}", snap.port(l.from).name(), snap.port(l.to).name());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const STUDIO: &str = include_str!("../testdata/studio.fake");

    #[test]
    fn studio_parses() {
        let s = parse(STUDIO).unwrap();
        assert_eq!(s.clients().len(), 6);
        assert_eq!(s.ports().len(), 24);
        assert_eq!(s.links().len(), 9);
        assert!(s.ports().all(|(_, p)| p.ty() == PortType::Audio));
    }

    #[test]
    fn round_trip() {
        let text = "out A:out 1\nout midi A:m\nin audio B:in\nin midi B:midi in # a comment\n\
                    # whole-line comment\n\nlink A:m > B:midi in\nlink A:out 1 > B:in\n";
        let s = parse(text).unwrap();
        let printed = render(&s);
        assert_eq!(
            printed,
            "out A:out 1\nout midi A:m\nin B:in\nin midi B:midi in\nlink A:out 1 > B:in\nlink A:m > B:midi in\n"
        );
        assert_eq!(render(&parse(&printed).unwrap()), printed);
        assert_eq!(
            render(&parse(STUDIO).unwrap()),
            render(&parse(&render(&parse(STUDIO).unwrap())).unwrap())
        );
    }

    #[test]
    fn errors_carry_lines() {
        let e = parse("out A:x\nlink A:x B:y\n").unwrap_err();
        assert_eq!(e.to_string(), "line 2: expected link A:b > C:d");
        let e = parse("out A:x\n\nfoo bar\n").unwrap_err();
        assert_eq!(e.line, 3);
        let e = parse("out A:x\nin midi B:y\nlink A:x > B:y\n").unwrap_err();
        assert_eq!(e.to_string(), "line 3: A:x and B:y carry different types");
    }
}
