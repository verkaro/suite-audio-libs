//! Provenance for a tool that processes recordings (Tape's RFC, S9 and file rules 2 and 4): a
//! line in BWF's CodingHistory saying what was done, a suffix on the filename saying the same
//! in brief, a sidecar log beside the file, and a check that refuses a file the tool has
//! already processed. Written for the suite, 2026-11-01. The `bext` chunk itself is
//! `as-wav`'s.
//!
//! ```
//! let tag = as_provenance::line("dexter-tape", "0.1.0", &[("snapshot", "house@358b8865"), ("trim", "+3.2dB")]);
//! assert_eq!(tag, "T=dexter-tape/0.1.0;snapshot=house@358b8865;trim=+3.2dB");
//! assert!(as_provenance::tagged(&[tag], "dexter-tape").is_some());
//! assert_eq!(as_provenance::sanitize("warm 15 ips!", 32), "warm-15-ips-");
//! ```

use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// A CodingHistory line: `T=<tool>/<version>`, then `;key=value` for each field.
pub fn line(tool: &str, version: &str, fields: &[(&str, &str)]) -> String {
    let mut l = format!("T={tool}/{version}");
    for (k, v) in fields {
        l += &format!(";{k}={v}");
    }
    l
}

/// The first CodingHistory line `tool` wrote, if any: the file is already its output.
pub fn tagged<'a>(history: &'a [String], tool: &str) -> Option<&'a String> {
    let prefix = format!("T={tool}/");
    history.iter().find(|l| l.starts_with(&prefix))
}

/// Free text made safe for a filename: ASCII letters, digits, `-` and `_` kept; anything
/// else becomes `-`, a run of them one `-`; cut to `most` characters.
pub fn sanitize(name: &str, most: usize) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        let keep = ch.is_ascii_alphanumeric() || ch == '_' || ch == '-';
        if keep {
            out.push(ch);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.chars().take(most).collect()
}

/// An output's name before its number: `<input stem>__<word>-<name>-<id>`, e.g.
/// `dx_take3__tape-warm15-3f9a12c4`; the number is `as-keep`'s `numbered`.
pub fn output_stem(input: &Path, word: &str, name: &str, id: &str) -> String {
    let stem = input.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    format!("{stem}__{word}-{name}-{id}")
}

/// Where an output's sidecar log goes: beside it, `.json` for its extension.
pub fn sidecar(output: &Path) -> PathBuf {
    output.with_extension("json")
}

/// Writes an output's sidecar log, created new, never over anything.
pub fn write_sidecar(output: &Path, json: &str) -> io::Result<PathBuf> {
    let path = sidecar(output);
    let mut f = as_keep::create_new(&path)?;
    f.write_all(json.as_bytes())?;
    f.sync_all()?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_sanitised_as_the_rfc_says() {
        assert_eq!(sanitize("house", 32), "house");
        assert_eq!(sanitize("Kim's boom (close)", 32), "Kim-s-boom-close-");
        assert_eq!(sanitize("über/../x", 32), "-ber-x");
        assert_eq!(sanitize(&"a".repeat(40), 32).len(), 32);
        assert_eq!(sanitize("a_b-c", 32), "a_b-c");
    }

    #[test]
    fn an_output_is_named_from_its_input() {
        let s = output_stem(Path::new("ep04/stems/dx_take3.wav"), "tape", "warm15", "3f9a12c4");
        assert_eq!(s, "dx_take3__tape-warm15-3f9a12c4");
        assert_eq!(sidecar(Path::new("o/dx__tape-x-1-001.wav")), Path::new("o/dx__tape-x-1-001.json"));
    }

    #[test]
    fn only_the_tools_own_line_counts() {
        let h = vec!["A=PCM,F=48000".to_string(), "T=dexter-repair/0.1.0;fixed=3".to_string()];
        assert_eq!(tagged(&h, "dexter-tape"), None);
        assert_eq!(tagged(&h, "dexter-repair"), Some(&h[1]));
        assert_eq!(tagged(&["T=dexter-taper/1".to_string()], "dexter-tape"), None);
    }

    #[test]
    fn a_sidecar_is_never_written_over() {
        let d = std::env::temp_dir().join(format!("as-provenance-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let out = d.join("a-001.wav");
        let p = write_sidecar(&out, "{}").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "{}");
        assert!(write_sidecar(&out, "{\"again\":1}").is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}
