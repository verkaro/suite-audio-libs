//! A sitting, written down: the measuring list (what to capture, in order, and what Warren sets
//! on the unit for each), and a numbered folder holding the list, each take as a WAV, and a
//! manifest of what passed. A sitting stopped halfway resumes at the first capture without a
//! take that passed; a redo is kept beside the take before it.
//!
//! The captures are Warren's and stay under his own directory: never committed. The figures
//! made from them are what may be.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::check::{Capture, Report};
use crate::signal::{Maker, Signal};

/// The measuring list: a text file the voicing's step writes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct List {
    /// "Warren's LA-4, serial if known".
    pub unit: String,
    #[serde(rename = "capture")]
    pub captures: Vec<Entry>,
}

/// One capture of a list: its name, its signal and that signal's figures (levels in dB under the
/// ceiling, times in seconds; any left out take the usual), what to set on the unit, and which
/// inputs to keep.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub name: String,
    /// `silence`, `sweep`, `steps`, `bursts`, `hold` or `steady_sweep`.
    pub signal: String,
    /// What Warren sets by hand: "ratio 4, input 3".
    #[serde(default)]
    pub knobs: String,
    /// The inputs measured, `L` and `R`: the same signal goes to both outputs.
    #[serde(default = "both")]
    pub inputs: Vec<String>,
    pub secs: Option<f64>,
    pub level: Option<f64>,
    pub levels: Option<Vec<f64>>,
    pub hold: Option<f64>,
    pub low: Option<f64>,
    pub low_secs: Option<f64>,
    pub high: Option<f64>,
    pub high_secs: Option<f64>,
    pub repeats: Option<usize>,
    pub after: Option<f64>,
    pub after_secs: Option<f64>,
    pub hz: Option<f64>,
    pub tone: Option<f64>,
    pub settle: Option<f64>,
}

fn both() -> Vec<String> {
    vec!["L".into(), "R".into()]
}

impl Entry {
    /// The signal it plays, made at `m`'s rate and ceiling.
    pub fn signal(&self, m: &Maker) -> Result<Signal, String> {
        let or = |v: Option<f64>, d: f64| v.unwrap_or(d);
        let s = match self.signal.as_str() {
            "silence" => m.silence(or(self.secs, 5.0)),
            "sweep" => m.sweep(or(self.secs, 10.0), or(self.level, 0.0)),
            "steps" => m.steps(&self.levels.clone().unwrap_or_else(Maker::steps_db), or(self.hold, 0.3)),
            "bursts" => m.bursts(
                or(self.low, -40.0),
                or(self.low_secs, 1.5),
                or(self.high, -10.0),
                or(self.high_secs, 0.5),
                self.repeats.unwrap_or(3),
            ),
            "hold" => m.hold(
                or(self.level, -10.0),
                or(self.hold, 2.0),
                self.after.or(Some(-40.0)),
                or(self.after_secs, 6.0),
            ),
            "steady_sweep" => m.steady_sweep(
                or(self.hz, 100.0),
                or(self.tone, -6.0),
                or(self.settle, 1.0),
                or(self.level, -12.0),
                or(self.secs, 10.0),
            ),
            other => return Err(format!("{}: no signal called {other:?}", self.name)),
        };
        Ok(s)
    }

    /// Which inputs, as channel numbers: `L` is 0, `R` is 1.
    pub fn channels(&self) -> Result<Vec<usize>, String> {
        self.inputs
            .iter()
            .map(|i| match i.to_ascii_uppercase().as_str() {
                "L" => Ok(0),
                "R" => Ok(1),
                _ => Err(format!("{}: no input called {i:?} (L or R)", self.name)),
            })
            .collect()
    }
}

impl List {
    pub fn from_toml(text: &str) -> Result<List, String> {
        let l: List = toml::from_str(text).map_err(|e| e.to_string().trim_end().to_string())?;
        let m = Maker::new(48_000, crate::signal::CEILING_DB);
        for (i, e) in l.captures.iter().enumerate() {
            if l.captures[..i].iter().any(|f| f.name == e.name) {
                return Err(format!("{}: named twice", e.name));
            }
            e.signal(&m)?;
            e.channels()?;
        }
        Ok(l)
    }
}

/// One take: which capture, its file, what the check said, and the knobs as typed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Take {
    pub capture: String,
    pub file: String,
    pub pass: bool,
    #[serde(default)]
    pub failed: Vec<String>,
    #[serde(default)]
    pub knobs: String,
}

/// What a sitting has captured.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub rate: u32,
    pub ceiling_db: f64,
    #[serde(default, rename = "take")]
    pub takes: Vec<Take>,
}

/// A sitting, in its folder.
pub struct Sitting {
    dir: PathBuf,
    pub list: List,
    pub manifest: Manifest,
}

const LIST: &str = "list.toml";
const MANIFEST: &str = "manifest.toml";

impl Sitting {
    /// A new sitting in `parent`, as the next `sitting-NNN/`, with the list copied in.
    pub fn start(
        parent: impl AsRef<Path>,
        list_text: &str,
        rate: u32,
        ceiling_db: f64,
    ) -> io::Result<Sitting> {
        let list = List::from_toml(list_text).map_err(io::Error::other)?;
        let folder = as_keep::Folder::new(&parent, "sitting")?;
        as_keep::create_new(folder.path().join(LIST))?.write_all(list_text.as_bytes())?;
        let manifest = Manifest { rate, ceiling_db: ceiling_db.min(0.0), takes: vec![] };
        let text = toml::to_string(&manifest).map_err(io::Error::other)?;
        as_keep::create_new(folder.path().join(MANIFEST))?.write_all(text.as_bytes())?;
        let dir = folder.finish()?;
        Ok(Sitting { dir, list, manifest })
    }

    /// A sitting already begun, to go on with.
    pub fn open(dir: impl AsRef<Path>) -> io::Result<Sitting> {
        let dir = dir.as_ref().to_path_buf();
        let list = List::from_toml(&fs::read_to_string(dir.join(LIST))?).map_err(io::Error::other)?;
        let manifest = toml::from_str(&fs::read_to_string(dir.join(MANIFEST))?)
            .map_err(|e| io::Error::other(e.to_string()))?;
        Ok(Sitting { dir, list, manifest })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The signals' maker, at the sitting's rate and ceiling.
    pub fn maker(&self) -> Maker {
        Maker::new(self.manifest.rate, self.manifest.ceiling_db)
    }

    /// The first capture of the list with no take that passed, if any is left.
    pub fn next(&self) -> Option<usize> {
        self.list
            .captures
            .iter()
            .position(|e| !self.manifest.takes.iter().any(|t| t.pass && t.capture == e.name))
    }

    /// The takes of capture `i`, in order.
    pub fn takes(&self, i: usize) -> Vec<&Take> {
        let name = &self.list.captures[i].name;
        self.manifest.takes.iter().filter(|t| &t.capture == name).collect()
    }

    /// The take of capture `i` to reduce: its last that passed. A failed take is never used.
    pub fn passed(&self, i: usize) -> Option<&Take> {
        self.takes(i).into_iter().rev().find(|t| t.pass)
    }

    /// Keeps a take of capture `i`: the inputs it measures (one [`Capture`] each, in its order)
    /// as a new `name-NNN.wav` of 32-bit float, and a line in the manifest. Returns the file.
    pub fn keep(
        &mut self,
        i: usize,
        inputs: &[Capture],
        report: &Report,
        knobs: &str,
    ) -> io::Result<PathBuf> {
        let name = self.list.captures[i].name.clone();
        let (path, file) = as_keep::numbered(&self.dir, &name, "wav")?;
        let len = inputs.iter().map(|c| c.samples.len()).max().unwrap_or(0);
        let mut w = as_wav::Writer::with_chunks(
            file,
            path.clone(),
            self.manifest.rate,
            as_wav::Format::F32,
            inputs.len() as u16,
            &[],
        )?;
        let mut frame = Vec::with_capacity(len * inputs.len());
        for k in 0..len {
            for c in inputs {
                frame.push(c.samples.get(k).copied().unwrap_or(0.0) as f32);
            }
        }
        w.write(&frame)?;
        w.close()?;
        let file = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
        self.manifest.takes.push(Take {
            capture: name,
            file,
            pass: report.pass(),
            failed: report.failed().into_iter().map(String::from).collect(),
            knobs: knobs.to_string(),
        });
        let text = toml::to_string(&self.manifest).map_err(io::Error::other)?;
        as_keep::save(self.dir.join(MANIFEST), text.as_bytes())?;
        Ok(path)
    }

    /// A take's inputs read back, xruns 0 (a take with any failed its check and isn't used).
    pub fn read(&self, t: &Take) -> io::Result<Vec<Capture>> {
        let mut r = as_wav::Reader::open_any(self.dir.join(&t.file))?;
        let rate = self.manifest.rate;
        Ok(r.read_channels_f64()?.into_iter().map(|samples| Capture { rate, samples, xruns: 0 }).collect())
    }
}
