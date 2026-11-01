//! Detented settings, as Tape's RFC (section 6) has them, for any tool whose controls click
//! between a few named positions: a sound is a snapshot naming one position (a detent) for
//! each control, and what a position means lives in a versioned table, never in the snapshot.
//! Written for the suite, 2026-11-01.
//!
//! - **A table** ([`Table`]) names its controls, each control's detents with the plain words
//!   that say what each sounds like, and the default. Published tables never change; a
//!   correction is a new version.
//! - **A snapshot** ([`Snapshot`]) is a small TOML file, read strictly: an unknown key, a
//!   missing one, a value that isn't one of its control's detents, an unknown schema or table
//!   version, are each an error that says where. There are no silent defaults.
//! - **Its ID** is the SHA-256 of a canonical form of what makes the sound (the schema, the
//!   kind, the table's version and the controls), never its name or note. Where one detent
//!   makes others do nothing (a table's [`Collapse`]), those enter as `-`, so two snapshots
//!   that sound the same have the same ID.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt;

/// The snapshot format's versions read.
pub const SCHEMAS: [u32; 1] = [1];

/// One named position of a control.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detent {
    /// As written in a snapshot: `15-iec`.
    pub key: String,
    /// As people read it: `15 IEC`.
    pub label: String,
    /// What it sounds like, in plain words: the tooltip, and what a listener should hear.
    pub words: String,
}

/// A control and its detents, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Control {
    /// As written in a snapshot: `machine`.
    pub key: String,
    /// As people read it: `Machine`.
    pub label: String,
    /// What it does: `sets the tape speed and equalization standard`.
    pub what: String,
    pub detents: Vec<Detent>,
    /// The default detent's key.
    pub default: String,
}

impl Control {
    pub fn detent(&self, key: &str) -> Option<&Detent> {
        self.detents.iter().find(|d| d.key == key)
    }
}

/// Where one detent makes other controls do nothing: with `control` at `detent`, each of
/// `ignored` enters a snapshot's ID as `-`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Collapse {
    pub control: String,
    pub detent: String,
    pub ignored: Vec<String>,
}

/// A versioned detent table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    /// The kind of snapshot it serves: `track`.
    pub kind: String,
    /// `v1`.
    pub version: String,
    pub controls: Vec<Control>,
    pub collapse: Vec<Collapse>,
}

impl Table {
    pub fn control(&self, key: &str) -> Option<&Control> {
        self.controls.iter().find(|c| c.key == key)
    }

    /// Every combination of detents, as control key to detent key, in the table's order: the
    /// first control's detents slowest.
    pub fn combinations(&self) -> Vec<BTreeMap<String, String>> {
        let mut all = vec![BTreeMap::new()];
        for c in &self.controls {
            all = all
                .into_iter()
                .flat_map(|m| {
                    c.detents.iter().map(move |d| {
                        let mut m = m.clone();
                        m.insert(c.key.clone(), d.key.clone());
                        m
                    })
                })
                .collect();
        }
        all
    }

    /// The snapshot of every default.
    pub fn defaults(&self, name: &str) -> Snapshot {
        Snapshot {
            schema: 1,
            kind: self.kind.clone(),
            name: name.into(),
            note: String::new(),
            detents: self.version.clone(),
            controls: self.controls.iter().map(|c| (c.key.clone(), c.default.clone())).collect(),
        }
    }
}

/// A snapshot: one detent for each control of the table it names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub schema: u32,
    pub kind: String,
    pub name: String,
    pub note: String,
    /// The table's version: `v1`.
    pub detents: String,
    /// Control key to detent key, every control.
    pub controls: BTreeMap<String, String>,
}

/// What's wrong with a snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    schema: u32,
    kind: String,
    name: String,
    note: String,
    detents: String,
    controls: BTreeMap<String, String>,
}

impl Snapshot {
    /// Reads a snapshot strictly, against the tables there are: it must name one of them and
    /// give each of its controls one of its detents, and nothing else.
    pub fn parse(text: &str, tables: &[&Table]) -> Result<Snapshot, Error> {
        let f: File = toml::from_str(text).map_err(|e| Error(e.to_string().trim_end().to_string()))?;
        if !SCHEMAS.contains(&f.schema) {
            return Err(Error(format!("schema {} isn't one this build reads ({SCHEMAS:?})", f.schema)));
        }
        let table = tables.iter().find(|t| t.version == f.detents && t.kind == f.kind).ok_or_else(|| {
            let known: Vec<String> = tables.iter().map(|t| format!("{} {}", t.kind, t.version)).collect();
            Error(format!(
                "no detent table {} for kind {:?} (there are: {})",
                f.detents,
                f.kind,
                known.join(", ")
            ))
        })?;
        let s = Snapshot {
            schema: f.schema,
            kind: f.kind,
            name: f.name,
            note: f.note,
            detents: f.detents,
            controls: f.controls,
        };
        s.check(table)?;
        Ok(s)
    }

    /// Every control of `table` given one of its detents, and nothing else.
    pub fn check(&self, table: &Table) -> Result<(), Error> {
        // A key it doesn't know first: a misspelt control is also a missing one, and the
        // misspelling is the news.
        if let Some(k) = self.controls.keys().find(|k| table.control(k).is_none()) {
            return Err(Error(format!("[controls] has {k}, which isn't a control in {}", table.version)));
        }
        for c in &table.controls {
            match self.controls.get(&c.key) {
                None => return Err(Error(format!("[controls] has no {}", c.key))),
                Some(v) if c.detent(v).is_none() => {
                    let keys: Vec<&str> = c.detents.iter().map(|d| d.key.as_str()).collect();
                    return Err(Error(format!(
                        "{} = {v:?} isn't one of its detents in {} ({})",
                        c.key,
                        table.version,
                        keys.join(", ")
                    )));
                }
                Some(_) => {}
            }
        }
        Ok(())
    }

    /// The text hashed for the ID: one `key=value` line a key, sorted, the controls as
    /// `controls.<key>`, values as written, a collapsed control as `-`.
    pub fn canonical(&self, table: &Table) -> String {
        let mut lines: Vec<String> = vec![
            format!("detents={}", self.detents),
            format!("kind={}", self.kind),
            format!("schema={}", self.schema),
        ];
        // A collapse applies only from a control that isn't itself collapsed: worked out until
        // nothing changes, so chains (A ignores B, B ignores C) give one ID to one sound.
        let mut ignored: Vec<&String> = vec![];
        for _ in 0..=table.collapse.len() {
            let next: Vec<&String> = table
                .collapse
                .iter()
                .filter(|c| {
                    !ignored.contains(&&c.control) && self.controls.get(&c.control) == Some(&c.detent)
                })
                .flat_map(|c| &c.ignored)
                .collect();
            if next == ignored {
                break;
            }
            ignored = next;
        }
        for (k, v) in &self.controls {
            let v = if ignored.contains(&k) { "-" } else { v };
            lines.push(format!("controls.{k}={v}"));
        }
        lines.sort();
        lines.iter().map(|l| format!("{l}\n")).collect()
    }

    /// The ID: the canonical form's SHA-256, in hex.
    pub fn id(&self, table: &Table) -> String {
        Sha256::digest(self.canonical(table).as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
    }

    /// The first 8 hex digits of the ID, as tags and filenames carry it.
    pub fn short_id(&self, table: &Table) -> String {
        self.id(table)[..8].to_string()
    }

    /// The snapshot as a file, every key said, the controls in the table's order.
    pub fn to_toml(&self, table: &Table) -> String {
        let q = |s: &str| toml::Value::String(s.to_string()).to_string();
        let mut t = format!(
            "schema   = {}\nkind     = {}\nname     = {}\nnote     = {}\ndetents  = {}\n\n[controls]\n",
            self.schema,
            q(&self.kind),
            q(&self.name),
            q(&self.note),
            q(&self.detents)
        );
        for c in &table.controls {
            if let Some(v) = self.controls.get(&c.key) {
                t += &format!("{:<8} = {}\n", c.key, q(v));
            }
        }
        t
    }
}

#[cfg(test)]
mod tests;
