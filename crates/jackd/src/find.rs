//! The running jackd, found in `/proc` by the server's name, and what its command line says.

use std::fs;
use std::io::Read;
use std::time::SystemTime;

/// The server's name: `JACK_DEFAULT_SERVER`, else `default`. libjack finds a client's server
/// by the same variable, so a test's dummy server and the user's never meet.
pub fn server_name() -> String {
    std::env::var("JACK_DEFAULT_SERVER").ok().filter(|n| !n.is_empty()).unwrap_or_else(|| "default".into())
}

/// The jackd serving [`server_name`]: its pid and command line.
pub fn find() -> Option<(i32, String)> {
    let want = server_name();
    for e in fs::read_dir("/proc").ok()?.flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<i32>().ok()) else { continue };
        let comm = fs::read_to_string(e.path().join("comm")).unwrap_or_default();
        if comm.trim() != "jackd" || zombie(pid) {
            continue;
        }
        let mut raw = Vec::new();
        if fs::File::open(e.path().join("cmdline")).and_then(|mut f| f.read_to_end(&mut raw)).is_err() {
            continue;
        }
        let args: Vec<String> = raw
            .split(|&b| b == 0)
            .filter(|a| !a.is_empty())
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .collect();
        if name_in(&args) == want {
            return Some((pid, args.join(" ")));
        }
    }
    None
}

/// Whether pid has exited and not yet been reaped: `/proc/PID/stat`'s state, which follows the
/// command's closing parenthesis. A zombie jackd has an empty command line, which would read
/// as the default server's.
pub(crate) fn zombie(pid: i32) -> bool {
    fs::read_to_string(format!("/proc/{pid}/stat"))
        .is_ok_and(|s| s.rsplit_once(')').is_some_and(|(_, rest)| rest.trim_start().starts_with('Z')))
}

/// Whether a process has ended. A jackd this process started stays a zombie until it's
/// collected, and a zombie still answers kill(pid, 0): so it's collected here if it's ours, and a
/// zombie counts as gone.
pub(crate) fn gone(pid: i32) -> bool {
    // SAFETY: waitpid with WNOHANG only collects pid if it's our child that has ended.
    if unsafe { libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG) } == pid {
        return true;
    }
    fs::metadata(format!("/proc/{pid}")).is_err() || zombie(pid)
}

/// The server name in a jackd command line: `-n NAME`, `--name NAME` or `-nNAME` before the
/// driver's options (where `-n` means periods).
fn name_in(args: &[String]) -> String {
    let mut name = "default".to_string();
    for (i, a) in args.iter().enumerate() {
        if a.starts_with("-d") {
            break;
        }
        if (a == "-n" || a == "--name") && i + 1 < args.len() {
            name = args[i + 1].clone();
        } else if let Some(n) = a.strip_prefix("-n").filter(|n| !n.is_empty()) {
            name = n.to_string();
        }
    }
    name
}

/// The running server's -I and -O, from its command line; None if it has none there.
pub fn running_io() -> Option<(i64, i64)> {
    let (_, cmd) = find()?;
    let args: Vec<&str> = cmd.split_whitespace().collect();
    let d = args.iter().position(|a| a.starts_with("-d"))?;
    let (mut i, mut o) = (0, 0);
    let mut k = d;
    while k < args.len() {
        let a = args[k];
        let val = |flag: &str| -> Option<i64> {
            if a == flag {
                args.get(k + 1).and_then(|v| v.parse().ok())
            } else {
                a.strip_prefix(flag).and_then(|v| v.parse().ok())
            }
        };
        if let Some(v) = val("-I") {
            i = v;
        }
        if let Some(v) = val("-O") {
            o = v;
        }
        k += 1;
    }
    Some((i, o))
}

/// The ALSA device the running jackd is on (`-d alsa -d hw:3`), if it's on ALSA.
pub fn running_alsa_device() -> Option<String> {
    let (_, cmd) = find()?;
    alsa_device(&cmd.split(' ').map(String::from).collect::<Vec<_>>())
}

/// The driver the running jackd uses (`alsa`, `dummy`…).
pub fn running_driver() -> Option<String> {
    let (_, cmd) = find()?;
    let args: Vec<String> = cmd.split(' ').map(String::from).collect();
    driver(&mut args.iter().skip(1))
}

/// The driver in a jackd command line, the rest of it after.
fn driver<'a>(it: &mut impl Iterator<Item = &'a String>) -> Option<String> {
    // The server's options, up to the driver.
    loop {
        let a = it.next()?;
        if a == "-d" || a == "--driver" {
            return it.next().cloned();
        }
        if let Some(d) = a.strip_prefix("--driver=").or(a.strip_prefix("-d").filter(|d| !d.is_empty())) {
            return Some(d.to_string());
        }
    }
}

/// The ALSA device in a jackd command line: the driver's `-d DEV`, `-dDEV` or `--device DEV`
/// after `-d alsa`; `hw:0`, ALSA's default, when it names none. None for another driver.
fn alsa_device(args: &[String]) -> Option<String> {
    let mut it = args.iter().skip(1);
    if driver(&mut it)? != "alsa" {
        return None;
    }
    while let Some(a) = it.next() {
        if a == "-d" || a == "--device" {
            return it.next().cloned();
        }
        if let Some(d) = a.strip_prefix("--device=").or(a.strip_prefix("-d").filter(|d| !d.is_empty())) {
            return Some(d.to_string());
        }
    }
    Some("hw:0".into())
}

/// When a process started, in clock ticks since boot (`/proc/PID/stat`, field 22): with its pid,
/// it names one start of a server, since pids are reused.
pub fn started(pid: i32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The fields after the command's closing parenthesis start at field 3.
    stat.rsplit_once(')')?.1.split_whitespace().nth(22 - 3)?.parse().ok()
}

/// When a process started, from its /proc entry.
pub(crate) fn process_start(pid: i32) -> Option<SystemTime> {
    fs::metadata(format!("/proc/{pid}")).and_then(|m| m.modified()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn a_zombie_is_seen_and_gone() {
        let mut child = Command::new("true").spawn().unwrap();
        let pid = child.id() as i32;
        let deadline = Instant::now() + Duration::from_secs(5);
        while !zombie(pid) {
            assert!(Instant::now() < deadline, "true never exited");
            thread::sleep(Duration::from_millis(10));
        }
        assert!(gone(pid)); // and reaped by it, as our own child
        assert!(!zombie(pid) && child.try_wait().is_err());
        assert!(!zombie(std::process::id() as i32));
    }

    #[test]
    fn start_ticks() {
        let me = std::process::id() as i32;
        assert!(started(me).is_some_and(|t| t > 0));
        assert_eq!(started(me), started(me));
        assert_eq!(started(-1), None);
    }

    #[test]
    fn server_names() {
        let n = |s: &str| name_in(&s.split(' ').map(String::from).collect::<Vec<_>>());
        assert_eq!(n("jackd -R -d alsa -d hw:USB -r 48000 -p 512 -n 3"), "default", "-n after -d is periods");
        assert_eq!(n("jackd -n dextertest-5 -r -d dummy"), "dextertest-5");
        assert_eq!(n("jackd --name rig -R -d alsa"), "rig");
        assert_eq!(n("jackd -nrig -R -dalsa"), "rig");
    }

    #[test]
    fn alsa_devices() {
        let d = |s: &str| alsa_device(&s.split(' ').map(String::from).collect::<Vec<_>>());
        assert_eq!(d("jackd -R -d alsa -d hw:USB -r 48000 -p 512 -n 3").as_deref(), Some("hw:USB"));
        assert_eq!(d("jackd -n rig -R -dalsa -dhw:3 -r 48000").as_deref(), Some("hw:3"));
        assert_eq!(d("jackd --driver=alsa --device=hw:CARD=USB,DEV=0").as_deref(), Some("hw:CARD=USB,DEV=0"));
        assert_eq!(d("jackd -R -d alsa -r 48000 -p 256").as_deref(), Some("hw:0"), "ALSA's default");
        assert_eq!(d("jackd -n dextertest -r -d dummy -C 16 -P 2"), None);
        assert_eq!(d("jackd -R"), None);
        let dr = |s: &str| driver(&mut s.split(' ').map(String::from).collect::<Vec<_>>().iter().skip(1));
        assert_eq!(dr("jackd -n dextertest -r -d dummy -C 16 -P 2").as_deref(), Some("dummy"));
        assert_eq!(dr("jackd -R -dalsa -dhw:3").as_deref(), Some("alsa"));
        assert_eq!(dr("jackd -R"), None);
    }
}
