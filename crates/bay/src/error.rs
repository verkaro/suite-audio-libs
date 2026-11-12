//! What kind of error something is, as a stable code an owner can act on (Dexter carries
//! them inside its own refusals). The codes don't change once published; the messages
//! beside them may. `SYNTAX.md`, "Errors", lists them.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// Text that doesn't parse, or brackets where they can't go.
    Syntax,
    /// No client, port, bundle or alias by that name.
    UnknownName,
    /// A forgiving name that matches more than one client or port.
    AmbiguousName,
    /// An output where an input goes, or the other way round.
    WrongSide,
    /// A range or selection past the ports there are, or none at all.
    OutOfRange,
    /// Counts that can't be paired (`3 outputs onto 2 inputs`).
    Pairing,
    /// Audio against MIDI, or a type word that disagrees with a bundle.
    TypeMismatch,
    /// A name defined again, differently, in the same input.
    Redefined,
    /// Aliases that go round in circles, or too many hops.
    AliasLoop,
    /// Something other than `alias` or `bundle` where only names may be defined.
    NotADefinition,
    /// The backend couldn't be reached or did something wrong.
    Backend,
    /// Not applied because statements it depends on were skipped (a tolerant restore's
    /// `exact` for a client whose links couldn't all be read).
    Withheld,
}

impl ErrorKind {
    pub const ALL: [ErrorKind; 12] = [
        ErrorKind::Syntax,
        ErrorKind::UnknownName,
        ErrorKind::AmbiguousName,
        ErrorKind::WrongSide,
        ErrorKind::OutOfRange,
        ErrorKind::Pairing,
        ErrorKind::TypeMismatch,
        ErrorKind::Redefined,
        ErrorKind::AliasLoop,
        ErrorKind::NotADefinition,
        ErrorKind::Backend,
        ErrorKind::Withheld,
    ];

    /// The stable snake_case code.
    pub fn code(self) -> &'static str {
        match self {
            ErrorKind::Syntax => "syntax",
            ErrorKind::UnknownName => "unknown_name",
            ErrorKind::AmbiguousName => "ambiguous_name",
            ErrorKind::WrongSide => "wrong_side",
            ErrorKind::OutOfRange => "out_of_range",
            ErrorKind::Pairing => "pairing",
            ErrorKind::TypeMismatch => "type_mismatch",
            ErrorKind::Redefined => "redefined",
            ErrorKind::AliasLoop => "alias_loop",
            ErrorKind::NotADefinition => "not_a_definition",
            ErrorKind::Backend => "backend",
            ErrorKind::Withheld => "withheld",
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// An error from resolving, pairing or defining names: its kind and a message for people.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub kind: ErrorKind,
    pub msg: String,
}

impl Problem {
    pub fn new(kind: ErrorKind, msg: impl Into<String>) -> Problem {
        Problem { kind, msg: msg.into() }
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.msg)
    }
}

impl std::error::Error for Problem {}

#[cfg(test)]
mod tests {
    use super::*;

    /// The published codes. Changing one breaks owners that act on it: add, don't change.
    #[test]
    fn codes_are_stable() {
        let codes: Vec<&str> = ErrorKind::ALL.iter().map(|k| k.code()).collect();
        assert_eq!(
            codes,
            [
                "syntax",
                "unknown_name",
                "ambiguous_name",
                "wrong_side",
                "out_of_range",
                "pairing",
                "type_mismatch",
                "redefined",
                "alias_loop",
                "not_a_definition",
                "backend",
                "withheld",
            ]
        );
    }
}
