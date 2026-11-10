//! The journal: one line of JSON for each change, appended
//! and never rewritten. A line says when, which file and serial, what made the change (and its
//! batch, if it was one of many), and each field before and after, which is enough to undo it.
//! A line's number, from 1, is its id.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// One field changed: its values before and after (none: it wasn't there, or was removed).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub field: String,
    pub before: Vec<String>,
    pub after: Vec<String>,
}

/// What made a change: `import`, `set` (the command line), `tag` (TAG), `batch` (many at once:
/// `flag`, `retire` and the rest name themselves), `undo`; the batch it was one of; and, for an
/// undo, what it undid (a line's id, or `batch:ID`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct How {
    pub what: String,
    pub batch: Option<String>,
    pub undoes: Option<String>,
}

impl How {
    pub fn new(what: &str) -> How {
        How { what: what.to_string(), ..How::default() }
    }
}

/// One change to one file, as the journal has it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// UTC, `2026-10-28T17:04:05Z`.
    pub when: String,
    /// The file's name, without its folder: the journal is beside it.
    pub file: String,
    /// Its SERIAL, if it had one.
    pub serial: Option<String>,
    pub how: How,
    pub changes: Vec<Change>,
}

impl Entry {
    pub fn new(path: &Path, serial: Option<String>, how: How, changes: Vec<Change>) -> Entry {
        let file = path.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
        Entry { when: utc(SystemTime::now()), file, serial, how, changes }
    }

    /// The journal's line, without its newline. Its names are in the order of the alphabet.
    pub fn line(&self) -> String {
        let list = |v: &[String]| format!("[{}]", v.iter().map(|s| quote(s)).collect::<Vec<_>>().join(","));
        let changes: Vec<String> = self
            .changes
            .iter()
            .map(|c| {
                format!(
                    r#"{{"after":{},"before":{},"field":{}}}"#,
                    list(&c.after),
                    list(&c.before),
                    quote(&c.field)
                )
            })
            .collect();
        let mut out = String::from("{");
        if let Some(b) = &self.how.batch {
            out += &format!(r#""batch":{},"#, quote(b));
        }
        out += &format!(r#""changes":[{}],"file":{},"#, changes.join(","), quote(&self.file));
        out += &format!(r#""serial":{},"#, self.serial.as_deref().map_or("null".into(), quote));
        if let Some(u) = &self.how.undoes {
            out += &format!(r#""undoes":{},"#, quote(u));
        }
        out += &format!(r#""what":{},"when":{}}}"#, quote(&self.how.what), quote(&self.when));
        out
    }

    /// A line read back. One from before `what` was written is an `import` if it gave the sound
    /// its SERIAL, and a `set` otherwise.
    pub fn parse(line: &str) -> Option<Entry> {
        let v = Json::parse(line)?;
        let strings = |v: &Json| -> Option<Vec<String>> {
            v.list()?.iter().map(|s| s.text().map(String::from)).collect()
        };
        let changes = v
            .get("changes")?
            .list()?
            .iter()
            .map(|c| {
                Some(Change {
                    field: c.get("field")?.text()?.to_string(),
                    before: strings(c.get("before")?)?,
                    after: strings(c.get("after")?)?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let text = |k: &str| v.get(k).and_then(Json::text).map(String::from);
        Some(Entry {
            when: text("when")?,
            file: text("file")?,
            serial: text("serial"),
            how: How {
                what: text("what").unwrap_or_else(|| {
                    let imported = changes.iter().any(|c| c.field == "SERIAL" && c.before.is_empty());
                    (if imported { "import" } else { "set" }).into()
                }),
                batch: text("batch"),
                undoes: text("undoes"),
            },
            changes,
        })
    }
}

/// A string as JSON writes it.
fn quote(s: &str) -> String {
    let mut o = String::from('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// As much of JSON as a journal's line has: objects, lists, strings and null. Numbers and
/// booleans are read past as null, so a line with more in it than this crate wrote still reads.
#[derive(Debug, PartialEq)]
enum Json {
    Null,
    Text(String),
    List(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    fn parse(s: &str) -> Option<Json> {
        let b: Vec<char> = s.chars().collect();
        let mut at = 0;
        let v = Json::value(&b, &mut at)?;
        Json::space(&b, &mut at);
        (at == b.len()).then_some(v)
    }

    fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(o) => o.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    fn text(&self) -> Option<&str> {
        match self {
            Json::Text(s) => Some(s),
            _ => None,
        }
    }

    fn list(&self) -> Option<&[Json]> {
        match self {
            Json::List(l) => Some(l),
            _ => None,
        }
    }

    fn space(b: &[char], at: &mut usize) {
        while b.get(*at).is_some_and(|c| c.is_ascii_whitespace()) {
            *at += 1;
        }
    }

    fn value(b: &[char], at: &mut usize) -> Option<Json> {
        Json::space(b, at);
        match *b.get(*at)? {
            '"' => Json::string(b, at).map(Json::Text),
            '[' => {
                *at += 1;
                let mut items = vec![];
                loop {
                    Json::space(b, at);
                    if *b.get(*at)? == ']' {
                        *at += 1;
                        return Some(Json::List(items));
                    }
                    if !items.is_empty() {
                        (*b.get(*at)? == ',').then_some(())?;
                        *at += 1;
                    }
                    items.push(Json::value(b, at)?);
                }
            }
            '{' => {
                *at += 1;
                let mut items = vec![];
                loop {
                    Json::space(b, at);
                    if *b.get(*at)? == '}' {
                        *at += 1;
                        return Some(Json::Object(items));
                    }
                    if !items.is_empty() {
                        (*b.get(*at)? == ',').then_some(())?;
                        *at += 1;
                        Json::space(b, at);
                    }
                    let k = Json::string(b, at)?;
                    Json::space(b, at);
                    (*b.get(*at)? == ':').then_some(())?;
                    *at += 1;
                    items.push((k, Json::value(b, at)?));
                }
            }
            _ => {
                // null, true, false or a number: read to its end.
                let from = *at;
                while b.get(*at).is_some_and(|c| c.is_ascii_alphanumeric() || "+-.".contains(*c)) {
                    *at += 1;
                }
                (*at > from).then_some(Json::Null)
            }
        }
    }

    fn string(b: &[char], at: &mut usize) -> Option<String> {
        (*b.get(*at)? == '"').then_some(())?;
        *at += 1;
        let mut out = String::new();
        loop {
            let c = *b.get(*at)?;
            *at += 1;
            match c {
                '"' => return Some(out),
                '\\' => {
                    let e = *b.get(*at)?;
                    *at += 1;
                    match e {
                        'n' => out.push('\n'),
                        'r' => out.push('\r'),
                        't' => out.push('\t'),
                        'b' => out.push('\u{8}'),
                        'f' => out.push('\u{c}'),
                        'u' => {
                            let unit = |at: &mut usize| -> Option<u32> {
                                let h: String = b.get(*at..*at + 4)?.iter().collect();
                                *at += 4;
                                u32::from_str_radix(&h, 16).ok()
                            };
                            let hi = unit(at)?;
                            let cp = if (0xD800..0xDC00).contains(&hi) {
                                (b.get(*at..*at + 2)? == ['\\', 'u']).then_some(())?;
                                *at += 2;
                                let lo = unit(at)?;
                                0x10000 + ((hi - 0xD800) << 10) + lo.checked_sub(0xDC00)?
                            } else {
                                hi
                            };
                            out.push(char::from_u32(cp)?);
                        }
                        other => out.push(other),
                    }
                }
                c => out.push(c),
            }
        }
    }
}

/// Every line of the journal at `path`, in order, the entry it holds or `None` for one that
/// doesn't read (a power cut while it was being appended), so that a line's place is still its
/// id. No journal: no lines.
pub fn read(path: &Path) -> io::Result<Vec<Option<Entry>>> {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e),
    };
    Ok(String::from_utf8_lossy(&bytes).lines().map(Entry::parse).collect())
}

/// Appends `entry` to the journal at `path`, made if it isn't there, and syncs it. If the last
/// line was cut short, the entry starts a line of its own after it.
pub fn append(path: &Path, entry: &Entry) -> io::Result<()> {
    let mut f = OpenOptions::new().read(true).append(true).create(true).open(path)?;
    let len = f.metadata()?.len();
    let mut last = *b"\n";
    if len > 0 {
        std::os::unix::fs::FileExt::read_exact_at(&f, &mut last, len - 1)?;
    }
    let lead = if last[0] == b'\n' { "" } else { "\n" };
    f.write_all(format!("{lead}{}\n", entry.line()).as_bytes())?;
    f.sync_data()
}

/// A time as UTC to the second: `2026-10-28T17:04:05Z`.
pub fn utc(t: SystemTime) -> String {
    let s = t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (days, secs) = (s.div_euclid(86_400), s.rem_euclid(86_400));
    // The civil date from days since 1970 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", secs / 3600, secs % 3600 / 60, secs % 60)
}
