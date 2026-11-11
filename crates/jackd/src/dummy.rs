//! The tests' JACK server: a dummy one of the test's own, never the user's.

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::{Settings, probe, server_name};

/// A dummy jackd, `<tool>test-<pid>`, at 48 kHz with periods of 512, 16 inputs and 2 outputs,
/// not realtime. Stopped with SIGTERM by its own pid when dropped.
pub struct Dummy {
    child: Option<Child>,
    name: String,
}

impl Dummy {
    /// The server's name for tool: `<tool>test-<pid>`.
    pub fn name(tool: &str) -> String {
        format!("{tool}test-{}", std::process::id())
    }

    /// Starts it and waits until it answers. `JACK_DEFAULT_SERVER` must already name it
    /// ([`Dummy::name`]), set by the test before any thread of its own starts, or this panics:
    /// so no client of the test can reach another server.
    pub fn start(tool: &str) -> Dummy {
        let name = Dummy::name(tool);
        assert_eq!(server_name(), name, "set JACK_DEFAULT_SERVER to Dummy::name first");
        let settings = Settings { realtime: false, ..Settings::default() }.on_dummy(16, 2);
        let args = settings.args(&name);
        let child = Command::new(&args[0])
            .args(&args[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("jackd (jackd2) is installed");
        let mut d = Dummy { child: Some(child), name };
        if !eventually(Duration::from_secs(10), || probe("up").is_ok()) {
            d.stop();
            panic!("the dummy server {} didn't start", d.name);
        }
        d
    }

    /// Its pid, while it runs.
    pub fn pid(&self) -> Option<u32> {
        self.child.as_ref().map(|c| c.id())
    }

    /// Stops it with SIGTERM and waits up to 10 s; only then SIGKILL, since it's the test's own.
    pub fn stop(&mut self) {
        if let Some(mut c) = self.child.take() {
            // SAFETY: a signal to our own child, by its pid.
            unsafe { libc::kill(c.id() as i32, libc::SIGTERM) };
            if eventually(Duration::from_secs(10), || c.try_wait().ok().flatten().is_some()) {
                return;
            }
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

impl Drop for Dummy {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Whether f comes true within the time, asked every 20 ms.
pub fn eventually(within: Duration, mut f: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < within {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    f()
}
