//! One program at a time is the surface: an flock on a file the tool names, with its holder's
//! pid in it. The lock goes however the process ends. Dexter's is `<socket>.surface`, beside
//! its engine's socket (Dexter's `docs/faderport.md`, "The process").

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, Write};
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

/// The lock, held while this process is the surface.
pub struct Held {
    _file: File,
}

fn open(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", path.display())))
}

fn flock(f: &File) -> bool {
    // SAFETY: flock on a file descriptor this process owns.
    unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) == 0 }
}

fn pid_in(f: &mut File) -> Option<u32> {
    let mut s = String::new();
    f.rewind().ok()?;
    f.read_to_string(&mut s).ok()?;
    s.trim().parse().ok()
}

/// Takes the lock at path for this process: Ok(Held), or Err with the pid of the process that
/// has it.
pub fn take(path: &Path) -> io::Result<Result<Held, Option<u32>>> {
    let mut f = open(path)?;
    if !flock(&f) {
        return Ok(Err(pid_in(&mut f)));
    }
    f.set_len(0)?;
    f.rewind()?;
    write!(f, "{}", std::process::id())?;
    f.flush()?;
    Ok(Ok(Held { _file: f }))
}

/// The pid of the process holding the lock at path, if one does.
pub fn running(path: &Path) -> Option<u32> {
    let mut f = fs::File::open(path).ok()?;
    // SAFETY: flock on a file descriptor this process owns.
    let free = unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0;
    if free {
        return None;
    }
    pid_in(&mut f)
}

/// Ends the process holding the lock at path, if there is one: asked with SIGTERM, and waited
/// for up to five seconds. Whether there is none now.
pub fn stop(path: &Path) -> bool {
    let Some(pid) = running(path) else { return true };
    // SAFETY: a signal to a process that holds the surface's lock.
    unsafe { libc::kill(pid as i32, libc::SIGTERM) };
    let t = Instant::now();
    while running(path).is_some() {
        if t.elapsed() > Duration::from_secs(5) {
            return false;
        }
        thread::sleep(Duration::from_millis(50));
    }
    true
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("as-surface-{}-{name}", std::process::id()));
        let _ = fs::remove_file(&d);
        d
    }

    #[test]
    fn one_surface_at_a_time() {
        let s = scratch("lock");
        assert_eq!(running(&s), None);
        let held = take(&s).unwrap().unwrap_or_else(|pid| panic!("the lock is held by {pid:?}"));
        assert_eq!(running(&s), Some(std::process::id()));
        // Another process asking (a second flock, on a file of its own) is refused, with the pid.
        let again = take(&s).unwrap();
        assert_eq!(again.err(), Some(Some(std::process::id())));
        drop(held);
        assert_eq!(running(&s), None, "the lock goes with the holder");
        assert!(stop(&s), "nothing to stop");
        let _ = fs::remove_file(&s);
    }
}
