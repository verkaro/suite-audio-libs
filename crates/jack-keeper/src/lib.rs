//! A JACK client of two outputs, kept (moved into the suite from place's `crates/jack`, itself
//! scape's, `VENDORED.md`; made generic over what plays, where place's played its own engine).
//!
//! [`Audio`] runs the keeper, a thread of its own. It opens the client on the server
//! `JACK_DEFAULT_SERVER` names (never starting jackd, never opening a sound card), keeps the
//! connections it's told are wanted (made whenever both their ports exist, cut only when taken
//! out of the wanted ones, never someone else's), and opens the client again when jackd comes
//! back. Each time it opens the client it makes a [`Player`] at JACK's rate, which plays on JACK's thread:
//! what's [`Audio::send`]t reaches it through a ring, and what it's done with comes back through
//! another, to be dropped on the keeper's thread. What's sent while JACK is away is dropped:
//! nothing plays when it comes back unasked. `--sim` ([`Audio::sim`]) runs the same player on a
//! timer, with a graph of its own.

use std::collections::{BTreeSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

mod client;
mod sim;

#[cfg(test)]
mod tests;

pub use sim::{PERIOD as SIM_PERIOD, RATE as SIM_RATE};

/// The client's outputs.
pub const OUT: [&str; 2] = ["out_L", "out_R"];
/// How often the keeper looks at the graph, and tries JACK again while it's away.
const TICK: Duration = Duration::from_millis(100);
/// How often it tries to open the client while JACK is away.
const RETRY: Duration = Duration::from_secs(1);
/// How many messages can wait for the callback, and things done with wait to be dropped.
const RING: usize = 64;

/// What plays on JACK's thread. Its methods are called there, so none of them may allocate,
/// lock, block or make a syscall.
pub trait Player: Send + 'static {
    /// What's sent to it.
    type In: Send + 'static;
    /// What it's done with (an engine replaced), handed back to be dropped off JACK's thread.
    type Out: Send + 'static;
    /// Something sent.
    fn take(&mut self, m: Self::In);
    /// A period's frames, left and right (the same length).
    fn period(&mut self, l: &mut [f32], r: &mut [f32]);
    /// Something it's done with, if any.
    fn done(&mut self) -> Option<Self::Out>;
}

/// A port of the graph that takes audio in, other than the client's own: somewhere the outputs
/// can go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortInfo {
    /// The full name, `system:playback_1`.
    pub name: String,
    /// A sound card's.
    pub physical: bool,
}

/// What the keeper and the rest share.
#[derive(Default)]
pub struct Status {
    /// The client is open and running.
    pub attached: AtomicBool,
    pub rate: AtomicU32,
    pub period: AtomicU32,
    /// How long after a period's written it's heard: the outputs' playback latency, frames (the
    /// most of the two; 0 simulated, or connected nowhere).
    pub latency: AtomicU32,
    /// xruns since it started.
    pub xruns: AtomicU64,
    /// Moves when the ports change.
    pub ports_gen: AtomicU64,
    ports: Mutex<Vec<PortInfo>>,
}

impl Status {
    fn set_ports(&self, mut p: Vec<PortInfo>) {
        sort_ports(&mut p);
        let mut now = self.ports.lock().unwrap_or_else(|e| e.into_inner());
        if *now != p {
            *now = p;
            self.ports_gen.fetch_add(1, Ordering::Release);
        }
    }
}

/// The sound card's first, then the mixer's, then Dexter's, then the rest; by name within each,
/// numbers in order (`playback_2` before `playback_10`).
pub fn sort_ports(p: &mut [PortInfo]) {
    p.sort_by_cached_key(|i| {
        let rank = if i.physical {
            0
        } else if i.name.starts_with("mixer:") {
            1
        } else if i.name.starts_with("dexter:") {
            2
        } else {
            3
        };
        (rank, natural(&i.name))
    });
}

/// A name with its runs of digits padded, so they sort as numbers.
fn natural(s: &str) -> String {
    let mut out = String::new();
    let mut digits = String::new();
    for c in s.chars().chain(std::iter::once('\0')) {
        if c.is_ascii_digit() {
            digits.push(c);
            continue;
        }
        if !digits.is_empty() {
            out.push_str(&format!("{digits:0>10}"));
            digits.clear();
        }
        if c != '\0' {
            out.push(c);
        }
    }
    out
}

/// The connections for each output's destinations, for a client named `me`.
pub fn links(me: &str, left: &[String], right: &[String]) -> BTreeSet<(String, String)> {
    let mut l = BTreeSet::new();
    for (out, to) in OUT.iter().zip([left, right]) {
        for d in to {
            l.insert((format!("{me}:{out}"), d.clone()));
        }
    }
    l
}

/// What the keeper needs of a graph, JACK's or the simulation's.
trait Graph {
    fn exists(&self, port: &str) -> bool;
    fn connected(&self, from: &str, to: &str) -> bool;
    fn connect(&mut self, from: &str, to: &str) -> Result<(), String>;
    fn disconnect(&mut self, from: &str, to: &str);
}

/// Makes every wanted connection whose ports both exist and isn't made, and cuts those in
/// cuts. Returns what it did, for the log.
fn reconcile(
    g: &mut dyn Graph,
    links: &BTreeSet<(String, String)>,
    cuts: &[(String, String)],
) -> Vec<String> {
    let mut said = Vec::new();
    for (a, b) in cuts {
        if g.connected(a, b) {
            g.disconnect(a, b);
            said.push(format!("cut {a} > {b}"));
        }
    }
    for (a, b) in links {
        if g.exists(a) && g.exists(b) && !g.connected(a, b) {
            match g.connect(a, b) {
                Ok(()) => said.push(format!("connected {a} > {b}")),
                Err(e) => said.push(format!("can't connect {a} > {b}: {e}")),
            }
        }
    }
    said
}

/// What the keeper is told.
enum Msg<I> {
    /// The connections wanted, each from a port to a port, by full name.
    Links(BTreeSet<(String, String)>),
    /// For the player.
    Play(I),
}

/// The audio, running.
pub struct Audio<P: Player> {
    status: Arc<Status>,
    tx: Sender<Msg<P::In>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl<P: Player> Audio<P> {
    /// The JACK client `name`, opened now if JACK is there, else as soon as it is; `make` makes
    /// its player each time it's opened, at JACK's rate.
    pub fn jack(
        name: &str,
        links: BTreeSet<(String, String)>,
        make: impl FnMut(u32) -> P + Send + 'static,
    ) -> Audio<P> {
        Audio::start(name, links, Box::new(make), client::keep)
    }

    /// No JACK: fake ports, the player run on a timer at [`SIM_RATE`] in periods of
    /// [`SIM_PERIOD`], its output going nowhere.
    pub fn sim(
        name: &str,
        links: BTreeSet<(String, String)>,
        make: impl FnMut(u32) -> P + Send + 'static,
    ) -> Audio<P> {
        Audio::start(name, links, Box::new(make), sim::keep)
    }

    fn start(
        name: &str,
        links: BTreeSet<(String, String)>,
        make: Box<dyn FnMut(u32) -> P + Send>,
        keep: fn(Keeper<P>),
    ) -> Audio<P> {
        let status = Arc::new(Status::default());
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let k = Keeper {
            name: name.to_string(),
            status: status.clone(),
            stop: stop.clone(),
            rx,
            links,
            waiting: VecDeque::new(),
            make,
        };
        let thread = thread::Builder::new().name("keeper".into()).spawn(move || keep(k)).expect("a thread");
        Audio { status, tx, stop, thread: Some(thread) }
    }

    /// From now on, these connections.
    pub fn want(&self, links: BTreeSet<(String, String)>) {
        let _ = self.tx.send(Msg::Links(links));
    }

    /// For the player. Dropped while JACK is away.
    pub fn send(&self, m: P::In) {
        let _ = self.tx.send(Msg::Play(m));
    }

    pub fn status(&self) -> &Status {
        &self.status
    }

    pub fn attached(&self) -> bool {
        self.status.attached.load(Ordering::Acquire)
    }

    pub fn rate(&self) -> u32 {
        self.status.rate.load(Ordering::Relaxed)
    }

    /// A period, frames.
    pub fn period(&self) -> u32 {
        self.status.period.load(Ordering::Relaxed)
    }

    /// How long after a period's written it's heard, frames ([`Status::latency`]).
    pub fn latency(&self) -> u32 {
        self.status.latency.load(Ordering::Relaxed)
    }

    /// Where the outputs can go (in [`sort_ports`]' order), and the number that moves when they
    /// change.
    pub fn ports(&self) -> (u64, Vec<PortInfo>) {
        let p = self.status.ports.lock().unwrap_or_else(|e| e.into_inner());
        (self.status.ports_gen.load(Ordering::Acquire), p.clone())
    }

    /// Closes the client.
    pub fn stop(mut self) {
        self.halt();
    }

    fn halt(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl<P: Player> Drop for Audio<P> {
    fn drop(&mut self) {
        self.halt();
    }
}

/// The player on JACK's thread, with its rings.
struct Rt<P: Player> {
    player: P,
    rx: rtrb::Consumer<P::In>,
    back: rtrb::Producer<P::Out>,
    /// Done with, waiting for room in the ring back.
    held: Option<P::Out>,
}

impl<P: Player> Rt<P> {
    fn period(&mut self, l: &mut [f32], r: &mut [f32]) {
        while let Ok(m) = self.rx.pop() {
            self.player.take(m);
        }
        self.player.period(l, r);
        while let Some(o) = self.held.take().or_else(|| self.player.done()) {
            if let Err(rtrb::PushError::Full(o)) = self.back.push(o) {
                self.held = Some(o);
                break;
            }
        }
    }
}

/// The keeper's ends of the rings.
struct Handle<P: Player> {
    tx: rtrb::Producer<P::In>,
    back: rtrb::Consumer<P::Out>,
}

fn pair<P: Player>(player: P) -> (Rt<P>, Handle<P>) {
    let (tx, rx) = rtrb::RingBuffer::new(RING);
    let (btx, brx) = rtrb::RingBuffer::new(RING);
    (Rt { player, rx, back: btx, held: None }, Handle { tx, back: brx })
}

/// The keeper's side.
struct Keeper<P: Player> {
    name: String,
    status: Arc<Status>,
    stop: Arc<AtomicBool>,
    rx: Receiver<Msg<P::In>>,
    links: BTreeSet<(String, String)>,
    /// What's waiting for the player.
    waiting: VecDeque<P::In>,
    make: Box<dyn FnMut(u32) -> P + Send>,
}

impl<P: Player> Keeper<P> {
    fn stopping(&self) -> bool {
        self.stop.load(Ordering::Acquire)
    }

    /// Waits up to d for messages, taking all there are. Returns the cuts the new connections
    /// imply (wanted ones no longer wanted), if they changed.
    fn wait(&mut self, d: Duration) -> Option<Vec<(String, String)>> {
        let first = match self.rx.recv_timeout(d) {
            Ok(m) => m,
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => return None,
        };
        let mut cuts: Option<Vec<(String, String)>> = None;
        let mut take = |k: &mut Keeper<P>, m: Msg<P::In>| match m {
            Msg::Links(new) => {
                cuts.get_or_insert_default().extend(k.links.difference(&new).cloned());
                k.links = new;
            }
            Msg::Play(m) => k.waiting.push_back(m),
        };
        take(self, first);
        while let Ok(m) = self.rx.try_recv() {
            take(self, m);
        }
        cuts
    }

    /// Sleeps up to d, a tick at a time, unless stopping. What's sent to play meanwhile is
    /// dropped.
    fn sleep(&mut self, d: Duration) -> Vec<(String, String)> {
        let mut cuts = Vec::new();
        let mut left = d;
        while !self.stopping() && !left.is_zero() {
            let t = left.min(TICK);
            if let Some(c) = self.wait(t) {
                cuts.extend(c);
            }
            self.waiting.clear();
            left = left.saturating_sub(t);
        }
        cuts
    }

    /// Hands what's waiting to the player, and drops what it's handed back.
    fn forward(&mut self, h: &mut Handle<P>) {
        while h.back.pop().is_ok() {}
        while let Some(m) = self.waiting.pop_front() {
            if let Err(rtrb::PushError::Full(m)) = h.tx.push(m) {
                // Next tick.
                self.waiting.push_front(m);
                break;
            }
        }
    }

    /// A note on stderr, under the client's name.
    fn note(&self, msg: &str) {
        eprintln!("{}: {msg}", self.name);
    }
}
