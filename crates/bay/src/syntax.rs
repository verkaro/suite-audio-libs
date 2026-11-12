//! The patch syntax (`SYNTAX.md`): parsing text into [`Statement`]s and printing them
//! back. Printing is canonical, and parsing what was printed gives the same statements.

use std::fmt;

use crate::model::{Direction, PortType};

/// What the brackets after a name select. All `None` means no brackets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selector {
    pub dir: Option<Direction>,
    pub ty: Option<PortType>,
    /// 1-based ordinals, `first..=last`.
    pub range: Option<(u32, u32)>,
}

impl Selector {
    pub fn is_empty(&self) -> bool {
        *self == Selector::default()
    }
}

/// A name with an optional selector: `REAPER`, `REAPER[out midi 2-3]`, `Carla:audio-out1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ref {
    pub name: String,
    pub sel: Selector,
}

impl Ref {
    pub fn new(name: impl Into<String>) -> Ref {
        Ref { name: name.into(), sel: Selector::default() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Statement {
    Connect { from: Ref, to: Ref },
    Disconnect { from: Ref, to: Ref },
    Alias { name: String, target: String },
    Exact { client: String },
    Bundle { name: String, target: Ref },
}

/// A statement and the 1-based line it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spanned {
    pub line: usize,
    pub stmt: Statement,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxError {
    pub line: usize,
    pub msg: String,
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.msg)
    }
}

impl SyntaxError {
    pub fn kind(&self) -> crate::error::ErrorKind {
        crate::error::ErrorKind::Syntax
    }
}

impl std::error::Error for SyntaxError {}

const KEYWORDS: [&str; 3] = ["alias", "exact", "bundle"];

fn is_special(c: char) -> bool {
    matches!(c, ' ' | '\t' | '[' | ']' | '>' | '!' | '"' | '#' | '=' | ';')
}

/// Parses a whole input. It's all or nothing: the first error stops it.
pub fn parse(text: &str) -> Result<Vec<Spanned>, SyntaxError> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let err = |msg: String| SyntaxError { line: i + 1, msg };
        let tokens = lex(line).map_err(err)?;
        for stmt in tokens.split(|t| *t == Token::Semi) {
            if !stmt.is_empty() {
                out.push(Spanned { line: i + 1, stmt: statement(stmt).map_err(err)? });
            }
        }
    }
    Ok(out)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Word { name: String, quoted: bool, sel: Option<Selector> },
    Gt,
    NotGt,
    Eq,
    Semi,
}

fn lex(line: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = line.char_indices().peekable();
    while let Some(&(start, c)) = chars.peek() {
        match c {
            ' ' | '\t' => {
                chars.next();
            }
            '#' => break,
            ';' | '>' | '=' => {
                chars.next();
                tokens.push(match c {
                    ';' => Token::Semi,
                    '>' => Token::Gt,
                    _ => Token::Eq,
                });
            }
            '!' => {
                chars.next();
                if chars.next_if(|&(_, c)| c == '>').is_none() {
                    return Err("expected > after !".into());
                }
                tokens.push(Token::NotGt);
            }
            '[' => return Err("a selector must follow a name directly".into()),
            ']' => return Err("unexpected ]".into()),
            _ => {
                let (name, quoted) = if c == '"' {
                    chars.next();
                    let mut name = String::new();
                    loop {
                        match chars.next() {
                            None => return Err("unterminated quote".into()),
                            Some((_, '"')) => break,
                            Some((_, '\\')) => match chars.next() {
                                Some((_, c @ ('"' | '\\'))) => name.push(c),
                                _ => return Err("a backslash in quotes escapes only \" or \\".into()),
                            },
                            Some((_, c)) => name.push(c),
                        }
                    }
                    (name, true)
                } else {
                    let mut end = start;
                    while let Some((i, c)) = chars.next_if(|&(_, c)| !is_special(c)) {
                        end = i + c.len_utf8();
                    }
                    (line[start..end].to_owned(), false)
                };
                let sel = if chars.next_if(|&(_, c)| c == '[').is_some() {
                    let mut body = String::new();
                    loop {
                        match chars.next() {
                            None => return Err("unterminated [".into()),
                            Some((_, ']')) => break,
                            Some((_, c)) => body.push(c),
                        }
                    }
                    Some(selector(&body)?)
                } else {
                    None
                };
                tokens.push(Token::Word { name, quoted, sel });
            }
        }
    }
    Ok(tokens)
}

fn selector(body: &str) -> Result<Selector, String> {
    let mut sel = Selector::default();
    let mut words = body.split_ascii_whitespace().peekable();
    if words.peek().is_none() {
        return Err("empty []".into());
    }
    for w in words {
        let twice = || format!("[{body}] says more than one {}", kind(w));
        match w {
            "out" | "in" => {
                let d = if w == "out" { Direction::Out } else { Direction::In };
                if sel.dir.replace(d).is_some() {
                    return Err(twice());
                }
            }
            "audio" | "midi" => {
                let t = if w == "audio" { PortType::Audio } else { PortType::Midi };
                if sel.ty.replace(t).is_some() {
                    return Err(twice());
                }
            }
            _ => {
                let r = range(w)
                    .ok_or_else(|| format!("{w:?} in [{body}] is not out, in, audio, midi or a range"))?;
                if sel.range.replace(r).is_some() {
                    return Err(twice());
                }
            }
        }
    }
    Ok(sel)
}

fn kind(word: &str) -> &'static str {
    match word {
        "out" | "in" => "direction",
        "audio" | "midi" => "type",
        _ => "range",
    }
}

/// `3` or `3-5`, 1-based, non-empty.
fn range(w: &str) -> Option<(u32, u32)> {
    let num = |s: &str| s.parse::<u32>().ok().filter(|&n| n >= 1 && s.bytes().all(|b| b.is_ascii_digit()));
    let (a, b) = match w.split_once('-') {
        Some((a, b)) => (num(a)?, num(b)?),
        None => (num(w)?, num(w)?),
    };
    (b >= a).then_some((a, b))
}

fn statement(tokens: &[Token]) -> Result<Statement, String> {
    let mut it = tokens.iter();
    let mut next = || it.next();
    let first = next().expect("statement() gets a non-empty slice");
    let keyword = match first {
        Token::Word { name, quoted: false, sel: None } if KEYWORDS.contains(&name.as_str()) => {
            Some(name.as_str())
        }
        _ => None,
    };
    let stmt = match keyword {
        Some("alias") => {
            let name = plain_name(next(), "an alias name")?;
            let target = plain_name(skip_eq(&mut next), "an alias target")?;
            Statement::Alias { name, target }
        }
        Some("exact") => Statement::Exact { client: plain_name(next(), "a client after exact")? },
        Some("bundle") => {
            let name = plain_name(next(), "a bundle name")?;
            let target = reference(skip_eq(&mut next), "a bundle's ports")?;
            if target.sel.dir.is_none() {
                return Err(format!("bundle {name} needs a direction: {}[out …] or [in …]", target.name));
            }
            Statement::Bundle { name, target }
        }
        _ => {
            let from = reference(Some(first), "a name")?;
            let connect = match next() {
                Some(Token::Gt) => true,
                Some(Token::NotGt) => false,
                _ => return Err("expected A > B, A !> B, alias, exact or bundle".into()),
            };
            let to = reference(next(), "a name after the arrow")?;
            if connect { Statement::Connect { from, to } } else { Statement::Disconnect { from, to } }
        }
    };
    match next() {
        None => Ok(stmt),
        Some(_) => Err("unexpected text after the statement (use ; to separate statements)".into()),
    }
}

/// Takes the optional `=` and returns the token after it.
fn skip_eq<'a>(next: &mut impl FnMut() -> Option<&'a Token>) -> Option<&'a Token> {
    match next() {
        Some(Token::Eq) => next(),
        t => t,
    }
}

fn reference(t: Option<&Token>, what: &str) -> Result<Ref, String> {
    match t {
        Some(Token::Word { name, sel, .. }) => Ok(Ref { name: name.clone(), sel: sel.unwrap_or_default() }),
        _ => Err(format!("expected {what}")),
    }
}

fn plain_name(t: Option<&Token>, what: &str) -> Result<String, String> {
    match t {
        Some(Token::Word { name, sel: None, .. }) => Ok(name.clone()),
        Some(Token::Word { name, .. }) => Err(format!("{name}: {what} takes no [ ]")),
        _ => Err(format!("expected {what}")),
    }
}

/// A name as it prints: bare when it can be, quoted otherwise.
pub struct Name<'a>(pub &'a str);

impl fmt::Display for Name<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = self.0;
        if !n.is_empty() && !n.contains(is_special) && !KEYWORDS.contains(&n) {
            return f.write_str(n);
        }
        f.write_str("\"")?;
        for c in n.chars() {
            if matches!(c, '"' | '\\') {
                f.write_str("\\")?;
            }
            write!(f, "{c}")?;
        }
        f.write_str("\"")
    }
}

impl fmt::Display for Selector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            return Ok(());
        }
        let mut words = Vec::new();
        words.extend(self.dir.map(|d| d.as_str().to_owned()));
        words.extend(self.ty.map(|t| t.as_str().to_owned()));
        words.extend(self.range.map(|(a, b)| if a == b { format!("{a}") } else { format!("{a}-{b}") }));
        write!(f, "[{}]", words.join(" "))
    }
}

impl fmt::Display for Ref {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", Name(&self.name), self.sel)
    }
}

impl fmt::Display for Statement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Statement::Connect { from, to } => write!(f, "{from} > {to}"),
            Statement::Disconnect { from, to } => write!(f, "{from} !> {to}"),
            Statement::Alias { name, target } => write!(f, "alias {} = {}", Name(name), Name(target)),
            Statement::Exact { client } => write!(f, "exact {}", Name(client)),
            Statement::Bundle { name, target } => write!(f, "bundle {} = {target}", Name(name)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(text: &str) -> Statement {
        let mut v = parse(text).unwrap();
        assert_eq!(v.len(), 1, "{text}");
        v.remove(0).stmt
    }

    fn err(text: &str) -> String {
        parse(text).unwrap_err().to_string()
    }

    fn r(name: &str, dir: Option<Direction>, ty: Option<PortType>, range: Option<(u32, u32)>) -> Ref {
        Ref { name: name.into(), sel: Selector { dir, ty, range } }
    }

    #[test]
    fn statements() {
        use Direction::*;
        use PortType::*;
        assert_eq!(
            one("REAPER[1-2] > phones"),
            Statement::Connect { from: r("REAPER", None, None, Some((1, 2))), to: Ref::new("phones") }
        );
        assert_eq!(
            one("a2j[midi 1]>REAPER[in midi]"),
            Statement::Connect {
                from: r("a2j", None, Some(Midi), Some((1, 1))),
                to: r("REAPER", Some(In), Some(Midi), None)
            }
        );
        assert_eq!(
            one("Carla:audio-out1 !> \"OBS Studio:in \\\"1\\\"\""),
            Statement::Disconnect { from: Ref::new("Carla:audio-out1"), to: Ref::new("OBS Studio:in \"1\"") }
        );
        assert_eq!(one("alias mic x.y-z"), Statement::Alias { name: "mic".into(), target: "x.y-z".into() });
        assert_eq!(one("alias mic = x"), Statement::Alias { name: "mic".into(), target: "x".into() });
        assert_eq!(one("exact REAPER # comment"), Statement::Exact { client: "REAPER".into() });
        assert_eq!(
            one("bundle drums = REAPER[5-12 out]"),
            Statement::Bundle { name: "drums".into(), target: r("REAPER", Some(Out), None, Some((5, 12))) }
        );
        // Quoted keywords are names.
        assert_eq!(one("\"alias\" > b"), Statement::Connect { from: Ref::new("alias"), to: Ref::new("b") });
    }

    #[test]
    fn lines_and_semicolons() {
        let v = parse("# header\n\na > b; c !> d;\n  exact a\n").unwrap();
        let lines: Vec<(usize, String)> = v.iter().map(|s| (s.line, s.stmt.to_string())).collect();
        assert_eq!(lines, [(3, "a > b".into()), (3, "c !> d".into()), (4, "exact a".into())]);
    }

    #[test]
    fn errors() {
        assert_eq!(err("a > b\na ! b"), "line 2: expected > after !");
        assert_eq!(err("a [1] > b"), "line 1: a selector must follow a name directly");
        assert_eq!(err("a[] > b"), "line 1: empty []");
        assert_eq!(err("a[0] > b"), "line 1: \"0\" in [0] is not out, in, audio, midi or a range");
        assert_eq!(err("a[3-2] > b"), "line 1: \"3-2\" in [3-2] is not out, in, audio, midi or a range");
        assert_eq!(err("a[1 2] > b"), "line 1: [1 2] says more than one range");
        assert_eq!(err("a[out in] > b"), "line 1: [out in] says more than one direction");
        assert_eq!(
            err("a > b c"),
            "line 1: unexpected text after the statement (use ; to separate statements)"
        );
        assert_eq!(err("a b"), "line 1: expected A > B, A !> B, alias, exact or bundle");
        assert_eq!(err("a >"), "line 1: expected a name after the arrow");
        assert_eq!(err("\"a > b"), "line 1: unterminated quote");
        assert_eq!(err("a[1 > b"), "line 1: unterminated [");
        assert_eq!(err("exact a[1]"), "line 1: a: a client after exact takes no [ ]");
        assert_eq!(err("bundle d = R[1-2]"), "line 1: bundle d needs a direction: R[out …] or [in …]");
        assert_eq!(err("alias = x"), "line 1: expected an alias name");
    }

    #[test]
    fn prints_canonically_and_reads_back() {
        let text = "REAPER[2-3 midi out] > \"my alias\"[1]\n\
                    x[4-4] !> \"exact\"\n\
                    alias \"a b\" \"c\\\\d\"\n\
                    exact \"has;semi\"\n\
                    bundle drums REAPER[in 1-8]\n";
        let printed: Vec<String> = parse(text).unwrap().iter().map(|s| s.stmt.to_string()).collect();
        assert_eq!(
            printed,
            [
                "REAPER[out midi 2-3] > \"my alias\"[1]",
                "x[4] !> \"exact\"",
                "alias \"a b\" = c\\d",
                "exact \"has;semi\"",
                "bundle drums = REAPER[in 1-8]",
            ]
        );
        let again: Vec<String> =
            parse(&printed.join("\n")).unwrap().iter().map(|s| s.stmt.to_string()).collect();
        assert_eq!(again, printed);
    }
}
