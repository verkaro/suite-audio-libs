//! The JACK server itself: jackd started from settings, found, watched and stopped, its log, and
//! a dummy server for tests.
//!
//! Moved in from Dexter's `crates/jack/src/jackd.rs` at `e98d44f` (2026-11-11), for jackbay and
//! for Dexter's step 8 (`RFC-as-jackd.md`), with what was Dexter's now the caller's: the settings
//! ([`Settings`], Dexter's `JackConfig`, and its `jackd_args`), the interface and setup commands
//! ([`Start`]), the probe's client name, and the clients that refuse a stop. [`Monitor`] has a
//! notification handler of its own, [`Follow`] reads the log, and [`Dummy`] is the tests' server,
//! new here.
//!
//! The server is the one `JACK_DEFAULT_SERVER` names, else `default` ([`server_name`]). libjack
//! finds a client's server the same way, and the `jack` crate can't name another, so it's read,
//! not passed. Nothing here opens a sound card: jackd does.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

mod dummy;
mod find;
mod log;
mod monitor;
mod settings;

pub use dummy::{Dummy, eventually};
pub use find::{find, running_alsa_device, running_driver, running_io, server_name, started};
pub use log::{Follow, log_path, noise, run_start, tail};
pub use monitor::Monitor;
pub use settings::{Settings, Start};

use find::gone;

/// A client of the running server, never starting one, under name (or a name like it, if it's
/// taken).
pub fn probe(name: &str) -> Result<jack::Client, String> {
    jack::Client::new(name, jack::ClientOptions::NO_START_SERVER)
        .map(|(c, _)| c)
        .map_err(|e| format!("can't connect to JACK ({e}); is jackd running?"))
}

/// The clients with ports, apart from the hardware (`system`), by name.
pub fn clients(c: &jack::Client) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for p in c.ports(None, None, jack::PortFlags::empty()) {
        let name = p.split(':').next().unwrap_or_default().to_string();
        if name != "system" && !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// The first of names attached to the running server, if any: who has JACK. jackbay asks for
/// Dexter's engine, `dexter` (Warren, 2026-11-11).
pub fn holder(probe_name: &str, names: &[&str]) -> Option<String> {
    let c = probe(probe_name).ok()?;
    let here = clients(&c);
    names.iter().find(|n| here.iter().any(|h| h == *n)).map(|n| n.to_string())
}

/// Runs start's setup commands and starts jackd with its settings, in its own session and with
/// its output appended to log after a dated marker, so closing the terminal or losing ssh leaves
/// it running. It returns once the server answers probe_name's client, with the offset in log
/// where this run's output begins. A jackd that stops at once, or doesn't answer within 10 s
/// (then it's sent SIGTERM), is an error with the log's tail.
pub fn start(start: &Start, probe_name: &str, log: &Path, out: &mut dyn Write) -> Result<u64, String> {
    if start.settings.realtime {
        let mut lim = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        // SAFETY: getrlimit only writes the struct it's given.
        if unsafe { libc::getrlimit(libc::RLIMIT_RTPRIO, &mut lim) } == 0 && lim.rlim_cur < 70 {
            let _ = writeln!(
                out,
                "warning: the realtime priority limit (ulimit -r) is {}, so JACK can't run realtime.\n         Is this user in the audio group, and logged in again since?",
                lim.rlim_cur
            );
        }
    }
    for c in &start.setup {
        let _ = writeln!(out, "setup: {c}");
        let _ = out.flush();
        match Command::new("sh").arg("-c").arg(c).status() {
            Ok(st) if st.success() => {}
            Ok(st) => {
                let _ = writeln!(out, "warning: setup command failed ({st}); starting JACK anyway");
            }
            Err(e) => {
                let _ = writeln!(out, "warning: setup command failed ({e}); starting JACK anyway");
            }
        }
    }

    if let Some(dir) = log.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let mut logf = OpenOptions::new()
        .append(true)
        .create(true)
        .open(log)
        .map_err(|e| format!("{}: {e}", log.display()))?;
    let from = logf.metadata().map(|m| m.len()).unwrap_or(0);
    // The interface, wherever it's plugged in now.
    let mut settings = start.settings.clone();
    if let Some(id) = start.interface.as_deref().filter(|_| settings.driver == "alsa") {
        let n = as_interfaces::resolve(Path::new("/"), id)?;
        settings.device = format!("hw:{n}");
        let _ = writeln!(out, "the interface {id} is hw:{n}");
    }
    let args = settings.args(&server_name());
    let _ = writeln!(logf, "{}", log::marker(&args));
    let _ = writeln!(out, "{}", args.join(" "));

    let mut cmd = Command::new(&args[0]);
    cmd.args(&args[1..]).stdin(Stdio::null());
    cmd.stdout(logf.try_clone().map_err(|e| e.to_string())?).stderr(logf);
    // SAFETY: setsid is async-signal-safe: a session of its own, so no hangup from this terminal.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let mut child = cmd.spawn().map_err(|e| format!("{}: {e}", args[0]))?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        thread::sleep(Duration::from_millis(200));
        if let Ok(Some(_)) = child.try_wait() {
            return Err(format!("jackd stopped straight away:\n{}", tail(log, from)));
        }
        if probe(probe_name).is_ok() {
            let _ = writeln!(out, "JACK is running (pid {}), log in {}", child.id(), log.display());
            // jackd outlives a command that exits, and init reaps it then. A server that lasts
            // waits on it here: a jackd stopped by another process must not stay a zombie that
            // find takes for the server.
            thread::spawn(move || child.wait());
            return Ok(from);
        }
        if Instant::now() > deadline {
            // SAFETY: a signal to the jackd just started, by its pid.
            unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
            return Err(format!("JACK didn't come up within 10 s:\n{}", tail(log, from)));
        }
    }
}

/// Stops the running jackd with SIGTERM, by the pid found, and waits up to 5 s for it to go. It
/// refuses while any client in refuse is attached, unless force.
pub fn stop(probe_name: &str, refuse: &[&str], force: bool) -> Result<(), String> {
    let Some((pid, _)) = find() else { return Err("no jackd is running".into()) };
    if !force && let Some(c) = holder(probe_name, refuse) {
        return Err(format!("{c} is attached to JACK; stop it first"));
    }
    // SAFETY: a signal to the jackd found by its name, by its pid.
    if unsafe { libc::kill(pid, libc::SIGTERM) } != 0 {
        return Err(format!("can't stop jackd (pid {pid}): {}", std::io::Error::last_os_error()));
    }
    for _ in 0..50 {
        if gone(pid) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(format!("jackd (pid {pid}) is still running 5 s after being asked to stop"))
}
