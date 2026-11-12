//! The undo stack file shared by the line CLI and the TUI (`undo.patch`), as text in and
//! text out: the front ends read and write the file. It's patch syntax: each step is a
//! `# undo <time> <what ran>` header, then the inverse of what was applied with full
//! port names, which survive ordinals shifting.

use crate::compile::{Compiler, Options, Plan};
use crate::model::Snapshot;
use crate::render;
use crate::resolve::Scope;
use crate::syntax::{self, Statement, SyntaxError};

/// How many steps are kept.
pub const DEPTH: usize = 100;

const HEADER: &str = "# undo ";

/// The statements that reverse `applied` (which ran against `snap`).
pub fn inverse_body(snap: &Snapshot, applied: &Plan) -> String {
    let inverse = Plan { del: applied.add.clone(), add: applied.del.clone() };
    render::port_statements(snap, &inverse).iter().map(|s| format!("{s}\n")).collect()
}

/// `text` with a step appended, dropping the oldest past [`DEPTH`].
pub fn push(text: &str, when: &str, what: &str, body: &str) -> String {
    let (prefix, mut steps) = split(text);
    steps.push(format!("{HEADER}{when} {}\n{body}", what.replace('\n', "; ")));
    let skip = steps.len().saturating_sub(DEPTH);
    prefix + &steps[skip..].concat()
}

/// The last step's header line and statements.
pub fn last(text: &str) -> Option<(String, String)> {
    let (_, steps) = split(text);
    steps.last().map(|b| {
        let (head, body) = b.split_once('\n').unwrap_or((b, ""));
        (head.to_owned(), body.to_owned())
    })
}

/// `text` without its last step.
pub fn pop(text: &str) -> String {
    let (prefix, mut steps) = split(text);
    steps.pop();
    prefix + &steps.concat()
}

/// Compiles a step's statements one by one against `snap`, so that one whose ports are
/// gone is set aside (returned) and the rest still apply.
pub fn plan(snap: &Snapshot, scope: &Scope, body: &str) -> Result<(Plan, Vec<Statement>), SyntaxError> {
    let mut c = Compiler::new(snap, scope, Options::default());
    let mut gone = Vec::new();
    for s in syntax::parse(body)? {
        if c.input(std::slice::from_ref(&s)).is_err() {
            gone.push(s.stmt);
        }
    }
    Ok((c.finish(), gone))
}

/// Text before the first header, and the steps (each ending in a newline).
fn split(text: &str) -> (String, Vec<String>) {
    let mut prefix = String::new();
    let mut steps: Vec<String> = Vec::new();
    for line in text.lines() {
        if line.starts_with(HEADER) {
            steps.push(String::new());
        }
        let target = steps.last_mut().unwrap_or(&mut prefix);
        target.push_str(line);
        target.push('\n');
    }
    (prefix, steps)
}

/// Seconds since the epoch as UTC, like `2026-09-25 19:04:05Z`.
pub fn timestamp(secs: u64) -> String {
    let (days, rest) = (secs / 86_400, secs % 86_400);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}Z", rest / 3_600, rest / 60 % 60, rest % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;

    #[test]
    fn steps() {
        let (prefix, steps) = split("# kept\n# undo a\nx > y\n# undo b\nz !> w\n");
        assert_eq!(prefix, "# kept\n");
        assert_eq!(steps, ["# undo a\nx > y\n", "# undo b\nz !> w\n"]);

        let mut text = String::new();
        assert_eq!(last(&text), None);
        for i in 0..DEPTH + 3 {
            text = push(&text, "t", &format!("step {i}\nmore"), &format!("a{i} > b\n"));
        }
        assert_eq!(
            last(&text),
            Some((format!("# undo t step {}; more", DEPTH + 2), format!("a{} > b\n", DEPTH + 2)))
        );
        assert_eq!(split(&text).1.len(), DEPTH);
        assert!(last(&pop(&text)).unwrap().1.starts_with(&format!("a{}", DEPTH + 1)));
    }

    #[test]
    fn plans_skip_gone_ports() {
        let s = fixture::parse(include_str!("../testdata/studio.fake")).unwrap();
        let (plan, gone) =
            plan(&s, &Scope::default(), "Carla:audio-out1 !> REAPER:in3\nOBS:gone > REAPER:in1\n").unwrap();
        assert_eq!(plan.del.len(), 1);
        assert_eq!(gone.iter().map(ToString::to_string).collect::<Vec<_>>(), ["OBS:gone > REAPER:in1"]);
    }

    #[test]
    fn timestamps() {
        assert_eq!(timestamp(0), "1970-01-01 00:00:00Z");
        assert_eq!(timestamp(1_790_000_000), "2026-09-21 14:13:20Z");
        assert_eq!(timestamp(951_782_400), "2000-02-29 00:00:00Z");
    }
}
