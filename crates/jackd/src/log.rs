//! jackd's log: where it is, where this run's lines begin, and its new lines without the noise.

use std::fs;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Duration;

use as_localtime::Local;

use crate::find::{find, process_start};

/// The family's jackd log, whoever starts JACK: `$XDG_STATE_HOME/jack/jackd.log`, else under
/// `~/.local/state` (Warren, 2026-11-11). So whoever started it, another tool finds this run's
/// lines.
pub fn log_path() -> PathBuf {
    let state = std::env::var_os("XDG_STATE_HOME")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state"));
    state.join("jack").join("jackd.log")
}

/// The line [`crate::start`] writes before a run's output: `=== 2026-11-11 10:00:00 jackd …`.
pub(crate) fn marker(args: &[String]) -> String {
    format!("=== {} {}", Local::now().date_time(), args.join(" "))
}

/// Where the running server's output begins in log: at the marker [`crate::start`] wrote for it,
/// if it started it (within 5 s of the process's start). Otherwise, at the end.
pub fn run_start(log: &Path) -> u64 {
    let Ok(data) = fs::read_to_string(log) else { return 0 };
    let end = data.len() as u64;
    let Some((pid, _)) = find() else { return end };
    let Some(started) = process_start(pid) else { return end };
    let Some(i) = data.rfind("=== ") else { return end };
    if i > 0 && data.as_bytes()[i - 1] != b'\n' {
        return end;
    }
    let stamp = data.get(i + 4..i + 23).unwrap_or_default();
    let Some(t) = Local::parse_date_time(stamp).map(|l| l.to_system_time()) else { return end };
    let apart = started.duration_since(t).unwrap_or_else(|e| e.duration());
    if apart > Duration::from_secs(5) { end } else { i as u64 }
}

/// Whether a jackd log line says nothing about this run: the banner, and lines it prints on
/// every start.
pub fn noise(l: &str) -> bool {
    const PREFIXES: [&str; 11] = [
        "jack_get_descriptor",
        "Copyright ",
        "jackdmp comes with",
        "This is free software",
        "under certain conditions",
        "self-connect-mode is",
        "ALSA: final selected sample format",
        "audio_reservation_init",
        "audio_reservation_finish",
        "Acquire audio card",
        "Released audio card",
    ];
    let digit_after =
        |p: &str| l.strip_prefix(p).is_some_and(|r| r.starts_with(|c: char| c.is_ascii_digit()));
    PREFIXES.iter().any(|p| l.starts_with(p))
        || digit_after("jackdmp ")
        || l.strip_prefix("ALSA: use ").is_some_and(|r| {
            let d = r.trim_start_matches(|c: char| c.is_ascii_digit());
            d.len() < r.len() && d.starts_with(" periods")
        })
}

/// The log from offset from, without the noise, each line indented: for error messages.
pub fn tail(log: &Path, from: u64) -> String {
    let Ok(data) = fs::read_to_string(log) else { return String::new() };
    let Some(rest) = data.get(from as usize..) else { return String::new() };
    rest.trim_end_matches('\n')
        .lines()
        .filter(|l| !noise(l))
        .map(|l| format!("  {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The log's new lines, as they arrive, without the noise. A partial line waits for the next
/// read.
pub struct Follow {
    log: PathBuf,
    from: u64,
}

impl Follow {
    /// From offset from: [`run_start`]'s, or [`crate::start`]'s.
    pub fn new(log: &Path, from: u64) -> Follow {
        Follow { log: log.to_path_buf(), from }
    }

    /// The whole lines written since the last read.
    pub fn lines(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        let Ok(mut f) = fs::File::open(&self.log) else { return out };
        if f.seek(SeekFrom::Start(self.from)).is_err() {
            return out;
        }
        let mut r = BufReader::new(f);
        let mut line = String::new();
        loop {
            line.clear();
            match r.read_line(&mut line) {
                Ok(n) if n > 0 && line.ends_with('\n') => {
                    self.from += n as u64;
                    let l = line.trim_end_matches('\n');
                    if !l.is_empty() && !noise(l) {
                        out.push(l.to_string());
                    }
                }
                _ => return out,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("as-jackd-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn log_noise() {
        for l in [
            "jackdmp 1.9.22",
            "Copyright 2001-2005 Paul Davis and others.",
            "ALSA: use 3 periods for capture",
            "Acquire audio card Audio2",
        ] {
            assert!(noise(l), "{l}");
        }
        for l in [
            "jackdmp is starting",
            "ALSA: cannot set hardware parameters",
            "JackAudioDriver::ProcessAsync: read error, skip cycle",
            "ALSA: use of",
        ] {
            assert!(!noise(l), "{l}");
        }
    }

    #[test]
    fn tails() {
        let d = scratch("tails");
        let log = d.join("jackd.log");
        fs::write(
            &log,
            "old line\n=== 2026-09-25 12:00:00 jackd -R -d alsa\njackdmp 1.9.22\nALSA: cannot open hw:USB\n",
        )
        .unwrap();
        assert_eq!(tail(&log, 9), "  === 2026-09-25 12:00:00 jackd -R -d alsa\n  ALSA: cannot open hw:USB");
        assert_eq!(tail(&log, 9999), "");
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn follows() {
        let d = scratch("follows");
        let log = d.join("jackd.log");
        fs::write(&log, "old\n").unwrap();
        let mut f = Follow::new(&log, 4);
        assert!(f.lines().is_empty());
        let mut w = fs::OpenOptions::new().append(true).open(&log).unwrap();
        use std::io::Write as _;
        write!(w, "jackdmp 1.9.22\nxrun\npart").unwrap();
        assert_eq!(f.lines(), ["xrun"]);
        writeln!(w, "ial").unwrap();
        assert_eq!(f.lines(), ["partial"], "a partial line waits");
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_marker_reads_back() {
        let m = marker(&["jackd".into(), "-R".into()]);
        assert!(m.starts_with("=== ") && m.ends_with(" jackd -R"));
        assert!(Local::parse_date_time(&m[4..23]).is_some());
    }
}
