//! A card's controls as ALSA lists them, and [`Fake`], a card made from a list of them: the
//! tests' 18i20, and `--sim`'s.
//!
//! The list's text form, one control a line, tab-separated: numid, interface, index, name,
//! access (`rw`, `r`, `w` or `-`), type (`bool`, `int`, `enum`, `other`), the count of values,
//! the type's detail (an int's `min,max,step`, and `;min,max` its dB range in hundredths; an
//! enum's items, `|` between them), and the values (`,` between them).

use std::collections::BTreeMap;
use std::io;

/// What a control holds.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Bool,
    /// Its range, and its dB range in hundredths of a dB when the kernel gives one: min is
    /// db.0, max db.1, linear in dB between.
    Int {
        min: i64,
        max: i64,
        step: i64,
        db: Option<(i64, i64)>,
    },
    /// Its items' names.
    Enum(Vec<String>),
    /// Bytes or IEC958: listed, never read or written here.
    Other,
}

/// A control.
#[derive(Clone, Debug, PartialEq)]
pub struct Control {
    pub numid: u32,
    /// `MIXER`, `CARD`, `PCM`…
    pub iface: String,
    pub index: u32,
    /// As `amixer` names it: `Mix A Input 01 Playback Volume`.
    pub name: String,
    pub readable: bool,
    pub writable: bool,
    pub kind: Kind,
    /// Its count of values.
    pub count: u32,
}

impl Control {
    /// The enum's item called name.
    pub fn item(&self, name: &str) -> Option<i64> {
        match &self.kind {
            Kind::Enum(items) => items.iter().position(|i| i == name).map(|i| i as i64),
            _ => None,
        }
    }
}

/// A card's controls, to read and write.
pub trait Controls: Send {
    /// Every control, as the card lists them now (a control's access can change: a line's
    /// volume is written only while its volume control is `SW`).
    fn list(&mut self) -> io::Result<Vec<Control>>;
    fn read(&mut self, numid: u32) -> io::Result<Vec<i64>>;
    fn write(&mut self, numid: u32, values: &[i64]) -> io::Result<()>;
}

/// A card made from a list of controls (the text form above), which keeps what's written to it.
#[derive(Clone, Debug, Default)]
pub struct Fake {
    controls: Vec<Control>,
    values: BTreeMap<u32, Vec<i64>>,
    /// Every write, in order: numid and values.
    pub writes: Vec<(u32, Vec<i64>)>,
}

impl Fake {
    /// The card a list's text describes.
    pub fn parse(text: &str) -> Result<Fake, String> {
        let mut f = Fake::default();
        for (n, line) in text.lines().enumerate() {
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let (c, v) = parse_line(line).map_err(|e| format!("line {}: {e}", n + 1))?;
            f.values.insert(c.numid, v);
            f.controls.push(c);
        }
        Ok(f)
    }

    /// The control called name.
    pub fn named(&self, name: &str) -> Option<&Control> {
        self.controls.iter().find(|c| c.name == name)
    }

    /// The values of the control called name.
    pub fn get(&self, name: &str) -> Option<&[i64]> {
        self.named(name).and_then(|c| self.values.get(&c.numid)).map(Vec::as_slice)
    }

    /// Sets a control as the interface itself would (the front panel, another program).
    pub fn set(&mut self, name: &str, values: &[i64]) {
        if let Some(c) = self.named(name) {
            self.values.insert(c.numid, values.to_vec());
        }
    }
}

impl Controls for Fake {
    fn list(&mut self) -> io::Result<Vec<Control>> {
        Ok(self.controls.clone())
    }

    fn read(&mut self, numid: u32) -> io::Result<Vec<i64>> {
        self.values.get(&numid).cloned().ok_or_else(|| io::Error::from_raw_os_error(libc::ENOENT))
    }

    fn write(&mut self, numid: u32, values: &[i64]) -> io::Result<()> {
        let Some(c) = self.controls.iter().find(|c| c.numid == numid).cloned() else {
            return Err(io::Error::from_raw_os_error(libc::ENOENT));
        };
        if !c.writable || values.len() != c.count as usize {
            return Err(io::Error::from_raw_os_error(if c.writable { libc::EINVAL } else { libc::EPERM }));
        }
        let ok = match &c.kind {
            Kind::Bool => values.iter().all(|v| (0..=1).contains(v)),
            Kind::Int { min, max, .. } => values.iter().all(|v| (min..=max).contains(&v)),
            Kind::Enum(items) => values.iter().all(|&v| v >= 0 && (v as usize) < items.len()),
            Kind::Other => false,
        };
        if !ok {
            return Err(io::Error::from_raw_os_error(libc::EINVAL));
        }
        // As the driver does: a line out's volume and mute are the program's to set only while
        // its volume control is SW (item 0), else the knob's.
        if let Some(line) =
            c.name.strip_prefix("Line Out ").and_then(|r| r.strip_suffix(" Volume Control Playback Enum"))
        {
            let sw = values[0] == 0;
            let prefix = format!("Line {line} ");
            for v in self.controls.iter_mut() {
                if v.name.starts_with(&prefix)
                    && (v.name.ends_with(" Playback Volume") || v.name.ends_with(" Mute Playback Switch"))
                {
                    v.writable = sw;
                }
            }
        }
        // And with speaker switching on, lines 1–4 are the knob's, and stay so.
        if c.name == "Speaker Switching Playback Enum" {
            let on = values[0] != 0;
            for k in 1..=4 {
                let vc = format!("Line Out {k:02} Volume Control Playback Enum");
                if on {
                    self.set(&vc, &[1]);
                }
                let line = format!("Line {k:02} ");
                for v in self.controls.iter_mut() {
                    let knob = v.name.starts_with(&line)
                        && (v.name.ends_with(" Playback Volume")
                            || v.name.ends_with(" Mute Playback Switch"));
                    if v.name == vc {
                        v.writable = !on;
                    } else if knob && on {
                        v.writable = false;
                    }
                }
            }
        }
        self.values.insert(numid, values.to_vec());
        self.writes.push((numid, values.to_vec()));
        Ok(())
    }
}

/// A control and its values as a line of the text form.
pub fn format_line(c: &Control, values: &[i64]) -> String {
    let access = match (c.readable, c.writable) {
        (true, true) => "rw",
        (true, false) => "r",
        (false, true) => "w",
        (false, false) => "-",
    };
    let (kind, detail) = match &c.kind {
        Kind::Bool => ("bool", String::new()),
        Kind::Int { min, max, step, db } => {
            let mut d = format!("{min},{max},{step}");
            if let Some((lo, hi)) = db {
                d += &format!(";{lo},{hi}");
            }
            ("int", d)
        }
        Kind::Enum(items) => ("enum", items.join("|")),
        Kind::Other => ("other", String::new()),
    };
    let values: Vec<String> = values.iter().map(i64::to_string).collect();
    format!(
        "{}\t{}\t{}\t{}\t{access}\t{kind}\t{}\t{detail}\t{}",
        c.numid,
        c.iface,
        c.index,
        c.name,
        c.count,
        values.join(",")
    )
}

fn parse_line(line: &str) -> Result<(Control, Vec<i64>), String> {
    let f: Vec<&str> = line.split('\t').collect();
    if f.len() != 9 {
        return Err(format!("{} fields, not 9", f.len()));
    }
    let num = |s: &str| s.parse::<i64>().map_err(|_| format!("{s:?} isn't a number"));
    let kind = match f[5] {
        "bool" => Kind::Bool,
        "int" => {
            let (range, db) = match f[7].split_once(';') {
                Some((r, d)) => (r, Some(d)),
                None => (f[7], None),
            };
            let r: Vec<i64> = range.split(',').map(num).collect::<Result<_, _>>()?;
            let db = match db {
                Some(d) => {
                    let d: Vec<i64> = d.split(',').map(num).collect::<Result<_, _>>()?;
                    (d.len() == 2).then(|| (d[0], d[1])).ok_or("a dB range is two numbers")?.into()
                }
                None => None,
            };
            if r.len() != 3 {
                return Err("an int's range is min,max,step".into());
            }
            Kind::Int { min: r[0], max: r[1], step: r[2], db }
        }
        "enum" => Kind::Enum(f[7].split('|').map(str::to_string).collect()),
        "other" => Kind::Other,
        k => return Err(format!("no such type: {k}")),
    };
    let values =
        if f[8].is_empty() { Vec::new() } else { f[8].split(',').map(num).collect::<Result<_, _>>()? };
    let c = Control {
        numid: num(f[0])? as u32,
        iface: f[1].to_string(),
        index: num(f[2])? as u32,
        name: f[3].to_string(),
        readable: f[4].contains('r'),
        writable: f[4].contains('w'),
        kind,
        count: num(f[6])? as u32,
    };
    Ok((c, values))
}
