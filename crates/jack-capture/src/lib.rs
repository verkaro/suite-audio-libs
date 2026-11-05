//! A JACK client of two inputs, `in_L` and `in_R`, whose audio is read off JACK's thread
//! through a ring. Written for dexter-like110, 2026-11-04, after `as-jack-keeper` (which has
//! outputs and a player): the client is opened on the server `JACK_DEFAULT_SERVER` names, never
//! starting jackd and never opening a sound card; the connections it's told are wanted are made
//! whenever both ports exist and cut only when taken out of the wanted ones, never someone
//! else's; and when jackd goes away and comes back the client is opened again, with a new ring.
//! Dexter could record through it.
//!
//! The callback pushes a whole period into the ring, or none of it, counting the frames it had
//! no room for. [`Capture::read`] says [`Chunk::Gap`] whenever what's read next doesn't follow
//! what was read before (frames lost, or a new ring after JACK came back).

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use jack::{AudioIn, ClientOptions, Control, Port, ProcessScope};

#[cfg(test)]
mod tests;

/// The client's inputs.
pub const IN: [&str; 2] = ["in_L", "in_R"];
const AUDIO_TYPE: &str = "32 bit float mono audio";
/// How often the keeper looks at the graph.
const TICK: Duration = Duration::from_millis(100);
/// How often it tries to open the client while JACK is away.
const RETRY: Duration = Duration::from_secs(1);

/// What the capture and the rest share.
#[derive(Default)]
pub struct Status {
    pub attached: AtomicBool,
    pub rate: AtomicU32,
    pub period: AtomicU32,
    pub xruns: AtomicU64,
    /// Frames the ring had no room for, since the start.
    pub dropped: AtomicU64,
    /// One more each time the client is opened.
    pub session: AtomicU64,
}

/// What [`Capture::read`] found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chunk {
    /// This many frames appended to `l` and `r` (possibly none).
    Frames(usize),
    /// Frames were lost (the ring was full, or JACK went away and came back): what's read next
    /// doesn't follow what was read before. Returned once, before any frames after the loss.
    Gap,
    /// Nothing to read: JACK is away.
    Away,
}

/// The connections for these sources: `left[i]` to `name:in_L`, `right[i]` to `name:in_R`.
pub fn links(name: &str, left: &[String], right: &[String]) -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    for (input, from) in IN.iter().zip([left, right]) {
        for f in from {
            out.insert((f.clone(), format!("{name}:{input}")));
        }
    }
    out
}

/// The audio ports of the JACK server that give audio out, which the inputs can be connected
/// to, in the server's order (the sound card's first). A client is opened to look and closed at
/// once; `Err` says why if JACK isn't there.
pub fn sources() -> Result<Vec<String>, String> {
    let opts = ClientOptions::NO_START_SERVER;
    let (c, _) =
        jack::Client::new("as-jack-capture-ports", opts).map_err(|_| "JACK isn't running".to_string())?;
    let all = c.ports(None, Some(AUDIO_TYPE), jack::PortFlags::IS_OUTPUT);
    let physical = c.ports(None, Some(AUDIO_TYPE), jack::PortFlags::IS_OUTPUT | jack::PortFlags::IS_PHYSICAL);
    let mut out = physical.clone();
    out.extend(all.into_iter().filter(|p| !physical.contains(p)));
    Ok(out)
}

enum Msg {
    Links(BTreeSet<(String, String)>),
}

/// The capture, running.
pub struct Capture {
    status: Arc<Status>,
    tx: Sender<Msg>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    rings: Receiver<rtrb::Consumer<f32>>,
    ring: Option<rtrb::Consumer<f32>>,
    gap: bool,
    seen_dropped: u64,
}

impl Capture {
    /// The client `name`, opened now if JACK is there, else as soon as it is; its inputs kept
    /// connected from the `links` (source port, `name:in_L` or `name:in_R`); a ring of `seconds`
    /// at JACK's rate made each time it's opened.
    pub fn jack(name: &str, links: BTreeSet<(String, String)>, seconds: f64) -> Capture {
        let status = Arc::new(Status::default());
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let (ring_tx, rings) = channel();
        let k = Keeper {
            name: name.to_string(),
            status: status.clone(),
            stop: stop.clone(),
            rx,
            links,
            seconds,
            rings: ring_tx,
        };
        let thread = thread::Builder::new().name("capture".into()).spawn(move || keep(k)).expect("a thread");
        Capture { status, tx, stop, thread: Some(thread), rings, ring: None, gap: false, seen_dropped: 0 }
    }

    /// From now on, these connections.
    pub fn want(&self, links: BTreeSet<(String, String)>) {
        let _ = self.tx.send(Msg::Links(links));
    }

    pub fn status(&self) -> &Status {
        &self.status
    }

    /// Appends what's waiting (up to `max` frames) to `l` and `r`; never blocks.
    pub fn read(&mut self, l: &mut Vec<f32>, r: &mut Vec<f32>, max: usize) -> Chunk {
        // A new ring after JACK came back: what it holds doesn't follow the old.
        while let Ok(c) = self.rings.try_recv() {
            self.ring = Some(c);
            self.gap = true;
        }
        let dropped = self.status.dropped.load(Ordering::Acquire);
        let lost = dropped != self.seen_dropped;
        if self.gap || lost {
            self.seen_dropped = dropped;
            self.gap = false;
            // What's in the ring is on either side of the loss and can't be told apart: it's
            // dropped, so what's read next follows the loss.
            if let Some(c) = self.ring.as_mut() {
                let n = c.slots();
                if let Ok(chunk) = c.read_chunk(n) {
                    chunk.commit_all();
                }
            }
            return Chunk::Gap;
        }
        let Some(c) = self.ring.as_mut() else { return Chunk::Away };
        let n = (c.slots() / 2).min(max);
        if n == 0 {
            return if self.status.attached.load(Ordering::Acquire) { Chunk::Frames(0) } else { Chunk::Away };
        }
        let chunk = c.read_chunk(2 * n).expect("slots were counted");
        let (a, b) = chunk.as_slices();
        l.reserve(n);
        r.reserve(n);
        // The ring's capacity and every read are even, so a frame never straddles the two slices.
        for s in a.as_chunks::<2>().0.iter().chain(b.as_chunks::<2>().0) {
            l.push(s[0]);
            r.push(s[1]);
        }
        chunk.commit_all();
        Chunk::Frames(n)
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

impl Drop for Capture {
    fn drop(&mut self) {
        self.halt();
    }
}

/// The keeper's side.
struct Keeper {
    name: String,
    status: Arc<Status>,
    stop: Arc<AtomicBool>,
    rx: Receiver<Msg>,
    links: BTreeSet<(String, String)>,
    seconds: f64,
    rings: Sender<rtrb::Consumer<f32>>,
}

impl Keeper {
    fn stopping(&self) -> bool {
        self.stop.load(Ordering::Acquire)
    }

    /// Waits up to `d` for messages, taking all there are. Returns the connections that were
    /// wanted and no longer are, if the wanted ones changed.
    fn wait(&mut self, d: Duration) -> Option<Vec<(String, String)>> {
        let first = match self.rx.recv_timeout(d) {
            Ok(m) => m,
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => return None,
        };
        let mut cuts = Vec::new();
        let mut take = |k: &mut Keeper, m: Msg| match m {
            Msg::Links(new) => {
                cuts.extend(k.links.difference(&new).cloned());
                k.links = new;
            }
        };
        take(self, first);
        while let Ok(m) = self.rx.try_recv() {
            take(self, m);
        }
        Some(cuts)
    }

    fn note(&self, msg: &str) {
        eprintln!("{}: {msg}", self.name);
    }
}

/// The keeper: opens the client and keeps it, until stopped.
fn keep(mut k: Keeper) {
    let mut away_said = false;
    while !k.stopping() {
        match Session::open(&mut k) {
            Ok(s) => {
                away_said = false;
                s.run(&mut k);
            }
            Err(e) => {
                k.status.attached.store(false, Ordering::Release);
                if !away_said {
                    k.note(&format!("waiting for JACK: {e}"));
                    away_said = true;
                }
                // Sleep a tick at a time, taking messages, so a stop isn't kept waiting.
                let mut left = RETRY;
                while !k.stopping() && !left.is_zero() {
                    let t = left.min(TICK);
                    k.wait(t);
                    left = left.saturating_sub(t);
                }
            }
        }
    }
}

type Active = jack::AsyncClient<Notify, Handler>;

struct Session {
    active: Active,
    notices: Arc<Notices>,
}

impl Session {
    fn open(k: &mut Keeper) -> Result<Session, String> {
        let opts = ClientOptions::NO_START_SERVER | ClientOptions::USE_EXACT_NAME;
        let client = jack::Client::new(&k.name, opts).map(|(c, _)| c).map_err(|e| e.to_string())?;
        let reg =
            |name: &str| client.register_port(name, AudioIn::default()).map_err(|e| format!("{name}: {e}"));
        let ins = [reg(IN[0])?, reg(IN[1])?];
        let rate = client.sample_rate();
        let slots = 2 * (k.seconds * rate as f64).ceil() as usize;
        let (prod, cons) = rtrb::RingBuffer::new(slots);
        lock_memory();
        let notices = Arc::new(Notices { status: k.status.clone(), ..Notices::default() });
        let handler = Handler { ins, prod, status: k.status.clone() };
        let period = client.buffer_size();
        let active = client
            .activate_async(Notify(notices.clone()), handler)
            .map_err(|e| format!("can't activate the client: {e}"))?;
        k.status.rate.store(rate, Ordering::Relaxed);
        k.status.period.store(period, Ordering::Relaxed);
        k.status.session.fetch_add(1, Ordering::Release);
        let _ = k.rings.send(cons);
        k.status.attached.store(true, Ordering::Release);
        k.note(&format!("attached to JACK: {rate} Hz, {period} frames"));
        Ok(Session { active, notices })
    }

    /// Keeps the client until it's stopped or JACK goes away.
    fn run(self, k: &mut Keeper) {
        let mut seen = u64::MAX;
        let mut cuts: Vec<(String, String)> = Vec::new();
        let mut dirty = true;
        loop {
            if k.stopping() {
                k.status.attached.store(false, Ordering::Release);
                if let Ok((c, _, _)) = self.active.deactivate() {
                    drop(c);
                }
                return;
            }
            if self.notices.shut.load(Ordering::Acquire) {
                k.status.attached.store(false, Ordering::Release);
                k.note("JACK went away; waiting for it to come back");
                // The server is gone: closing would wait on it.
                std::mem::forget(self.active);
                return;
            }
            let g = self.notices.graph.load(Ordering::Acquire);
            if g != seen || dirty {
                seen = g;
                dirty = false;
                let c = self.active.as_client();
                for line in reconcile(c, &k.links, &cuts) {
                    k.note(&line);
                }
                cuts.clear();
            }
            if let Some(c) = k.wait(TICK) {
                cuts.extend(c);
                dirty = true;
            }
        }
    }
}

/// Makes every wanted connection whose ports both exist and isn't made, and cuts those in
/// `cuts`. Returns what it did, for the log.
fn reconcile(c: &jack::Client, links: &BTreeSet<(String, String)>, cuts: &[(String, String)]) -> Vec<String> {
    let mut said = Vec::new();
    let connected =
        |a: &str, b: &str| c.port_by_name(a).is_some_and(|p| p.is_connected_to(b).unwrap_or(false));
    for (a, b) in cuts {
        if connected(a, b) {
            let _ = c.disconnect_ports_by_name(a, b);
            said.push(format!("cut {a} > {b}"));
        }
    }
    for (a, b) in links {
        if c.port_by_name(a).is_some() && c.port_by_name(b).is_some() && !connected(a, b) {
            match c.connect_ports_by_name(a, b) {
                Ok(()) => said.push(format!("connected {a} > {b}")),
                Err(e) => said.push(format!("can't connect {a} > {b}: {e}")),
            }
        }
    }
    said
}

/// Locks the process's memory so far, the callback's included, where the limit allows. A
/// failure is quiet: the keeper's own client says so once.
fn lock_memory() {
    // SAFETY: mlockall only changes how the process's pages are kept.
    let _ = unsafe { libc::mlockall(libc::MCL_CURRENT) };
}

/// What JACK's notices leave for the keeper: counted or flagged only, since calling JACK from a
/// notification can deadlock.
#[derive(Default)]
struct Notices {
    status: Arc<Status>,
    graph: AtomicU64,
    shut: AtomicBool,
}

struct Notify(Arc<Notices>);

impl Notify {
    fn bump(&self) {
        self.0.graph.fetch_add(1, Ordering::Release);
    }
}

impl jack::NotificationHandler for Notify {
    fn xrun(&mut self, _: &jack::Client) -> Control {
        self.0.status.xruns.fetch_add(1, Ordering::Relaxed);
        Control::Continue
    }

    unsafe fn shutdown(&mut self, _: jack::ClientStatus, _: &str) {
        self.0.shut.store(true, Ordering::Release);
    }

    fn client_registration(&mut self, _: &jack::Client, _: &str, _: bool) {
        self.bump();
    }

    fn port_registration(&mut self, _: &jack::Client, _: jack::PortId, _: bool) {
        self.bump();
    }

    fn port_rename(&mut self, _: &jack::Client, _: jack::PortId, _: &str, _: &str) -> Control {
        self.bump();
        Control::Continue
    }

    fn ports_connected(&mut self, _: &jack::Client, _: jack::PortId, _: jack::PortId, _: bool) {
        self.bump();
    }

    fn sample_rate(&mut self, _: &jack::Client, rate: jack::Frames) -> Control {
        self.0.status.rate.store(rate, Ordering::Relaxed);
        Control::Continue
    }
}

/// The process callback: the two inputs' buffers, pushed whole into the ring or not at all.
/// Nothing here allocates, locks or blocks.
struct Handler {
    ins: [Port<AudioIn>; 2],
    prod: rtrb::Producer<f32>,
    status: Arc<Status>,
}

impl jack::ProcessHandler for Handler {
    fn process(&mut self, _: &jack::Client, ps: &ProcessScope) -> Control {
        let l = self.ins[0].as_slice(ps);
        let r = self.ins[1].as_slice(ps);
        let n = l.len().min(r.len());
        if self.prod.slots() >= 2 * n {
            if let Ok(chunk) = self.prod.write_chunk_uninit(2 * n) {
                chunk.fill_from_iter(l.iter().zip(r).flat_map(|(a, b)| [*a, *b]));
            }
        } else {
            self.status.dropped.fetch_add(n as u64, Ordering::Release);
        }
        Control::Continue
    }
}
