//! The JACK side: a thread that opens a client of no ports, activates it (an unactivated client
//! keeps returning a stale frame after the server dies), polls the transport and publishes it.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use jack::{ClientOptions, Control, TransportState};

use crate::{Position, State};

/// How often the transport is read.
const POLL: Duration = Duration::from_millis(10);
/// How often opening the client is tried again while the server is away.
pub const RETRY: Duration = Duration::from_secs(1);

const VALID: u64 = 1 << 63;

/// What the thread publishes: one word (valid, state, frame) and the rate beside it.
#[derive(Default)]
pub(crate) struct Shared {
    word: AtomicU64,
    rate: AtomicU32,
    stopping: AtomicBool,
}

impl Shared {
    pub(crate) fn publish(&self, p: Option<Position>) {
        match p {
            Some(p) => {
                self.rate.store(p.rate, Ordering::Relaxed);
                let s = match p.state {
                    State::Stopped => 0u64,
                    State::Rolling => 1,
                    State::Starting => 2,
                    State::Other => 3,
                };
                self.word.store(VALID | (s << 32) | u64::from(p.frame), Ordering::Release);
            }
            None => self.word.store(0, Ordering::Release),
        }
    }

    pub(crate) fn read(&self) -> Option<Position> {
        let w = self.word.load(Ordering::Acquire);
        if w & VALID == 0 {
            return None;
        }
        let state = match (w >> 32) & 0xff {
            0 => State::Stopped,
            1 => State::Rolling,
            2 => State::Starting,
            _ => State::Other,
        };
        Some(Position { state, frame: w as u32, rate: self.rate.load(Ordering::Relaxed) })
    }
}

pub(crate) struct JackClock {
    shared: Arc<Shared>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl JackClock {
    pub(crate) fn new(name: &str) -> JackClock {
        let shared = Arc::new(Shared::default());
        let (s, name) = (shared.clone(), name.to_string());
        let thread = thread::Builder::new()
            .name("transport".into())
            .spawn(move || run(&s, &name))
            .expect("a thread for the clock");
        JackClock { shared, thread: Mutex::new(Some(thread)) }
    }

    pub(crate) fn now(&self) -> Option<Position> {
        self.shared.read()
    }

    pub(crate) fn end(self) {
        self.shared.stopping.store(true, Ordering::Release);
        if let Some(t) = self.thread.lock().expect("the thread's lock").take() {
            let _ = t.join();
        }
    }
}

impl Drop for JackClock {
    fn drop(&mut self) {
        // The thread ends by itself within a poll; it isn't waited for.
        self.shared.stopping.store(true, Ordering::Release);
    }
}

/// What JACK's notices leave: a flag. Calling JACK from a notification can deadlock.
struct Notify {
    shut: Arc<AtomicBool>,
    rate: Arc<AtomicU32>,
}

impl jack::NotificationHandler for Notify {
    unsafe fn shutdown(&mut self, _: jack::ClientStatus, _: &str) {
        self.shut.store(true, Ordering::Release);
    }

    fn sample_rate(&mut self, _: &jack::Client, rate: jack::Frames) -> Control {
        self.rate.store(rate, Ordering::Relaxed);
        Control::Continue
    }
}

/// The process callback: nothing at all (no allocation, no lock). The client has to be active.
struct Idle;

impl jack::ProcessHandler for Idle {
    fn process(&mut self, _: &jack::Client, _: &jack::ProcessScope) -> Control {
        Control::Continue
    }
}

fn pause(shared: &Shared, total: Duration) {
    let start = Instant::now();
    while start.elapsed() < total && !shared.stopping.load(Ordering::Acquire) {
        thread::sleep(POLL);
    }
}

fn run(shared: &Shared, name: &str) {
    while !shared.stopping.load(Ordering::Acquire) {
        // Never starts the server, and takes another name if this one is in use.
        let Ok((client, _)) = jack::Client::new(name, ClientOptions::NO_START_SERVER) else {
            pause(shared, RETRY);
            continue;
        };
        let shut = Arc::new(AtomicBool::new(false));
        let rate = Arc::new(AtomicU32::new(client.sample_rate()));
        let Ok(active) = client.activate_async(Notify { shut: shut.clone(), rate: rate.clone() }, Idle)
        else {
            pause(shared, RETRY);
            continue;
        };
        let ended = loop {
            if shut.load(Ordering::Acquire) {
                break false;
            }
            if shared.stopping.load(Ordering::Acquire) {
                break true;
            }
            match active.as_client().transport().query() {
                Ok(q) => {
                    let state = match q.state {
                        TransportState::Stopped => State::Stopped,
                        TransportState::Rolling => State::Rolling,
                        TransportState::Starting => State::Starting,
                    };
                    let rate = rate.load(Ordering::Relaxed);
                    shared.publish(Some(Position { state, frame: q.pos.frame(), rate }));
                }
                Err(_) => shared.publish(None),
            }
            thread::sleep(POLL);
        };
        shared.publish(None);
        if ended {
            if let Ok((c, _, _)) = active.deactivate() {
                drop(c);
            }
            return;
        }
        // The server is gone: closing the client would wait on it.
        std::mem::forget(active);
        pause(shared, RETRY);
    }
}
