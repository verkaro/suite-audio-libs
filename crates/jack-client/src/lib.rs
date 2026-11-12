//! What every JACK client in the suite does alike, in one place (Warren, 2026-11-12, when
//! `as-bench` made a third client): opening the client on the server `JACK_DEFAULT_SERVER`
//! names, never starting jackd; locking memory; JACK's notices counted or flagged for the thread
//! that keeps the client; and the wanted connections kept. Written here, from the copies
//! `as-jack-keeper` and `as-jack-capture` each had.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use jack::{ClientOptions, Control, PortFlags};

#[cfg(test)]
mod tests;

/// The client `name`, exactly, on the running server, never starting one. `Err` says why in
/// words: no jackd, another client of the name, or JACK's own reason.
pub fn open(name: &str) -> Result<jack::Client, String> {
    let opts = ClientOptions::NO_START_SERVER | ClientOptions::USE_EXACT_NAME;
    match jack::Client::new(name, opts) {
        Ok((c, _)) => Ok(c),
        Err(e) => Err(why_not(name, &e.to_string())),
    }
}

/// Why the client couldn't be opened, in words.
fn why_not(name: &str, e: &str) -> String {
    // jack2 doesn't say a taken exact name was the reason, so ask who's there.
    let probe = jack::Client::new(&format!("{name}-probe"), ClientOptions::NO_START_SERVER);
    if let Ok((p, _)) = probe {
        let mine = format!("^{name}:");
        if !p.ports(Some(&mine), None, PortFlags::empty()).is_empty() {
            return format!("another {name} is running on this JACK server");
        }
        return format!("can't open the JACK client ({e})");
    }
    "is jackd running?".into()
}

/// Locks the process's memory so far, the callback's included, where the limit allows. The
/// first time it can't, in this process, `note` is told why.
pub fn lock_memory(note: impl FnOnce(&str)) {
    static SAID: AtomicBool = AtomicBool::new(false);
    // SAFETY: mlockall only changes how the process's pages are kept.
    let locked = unsafe { libc::mlockall(libc::MCL_CURRENT) } == 0;
    if !locked && !SAID.swap(true, Ordering::Relaxed) {
        note("couldn't lock memory (ulimit -l); an xrun is likelier under memory pressure");
    }
}

/// Where a client's xruns and rate are kept, for its notices to count into.
pub trait Counts: Send + Sync + 'static {
    fn xrun(&self);
    fn rate(&self, rate: u32);
}

/// What JACK's notices leave for the thread that keeps the client: counted or flagged only,
/// since calling JACK from a notification can deadlock.
pub struct Notices<S> {
    pub status: Arc<S>,
    /// Moves whenever a client, a port or a connection comes or goes.
    pub graph: AtomicU64,
    /// JACK went away.
    pub shut: AtomicBool,
}

impl<S> Notices<S> {
    pub fn new(status: Arc<S>) -> Arc<Notices<S>> {
        Arc::new(Notices { status, graph: AtomicU64::new(0), shut: AtomicBool::new(false) })
    }
}

/// The notification handler: hands JACK's notices to [`Notices`].
pub struct Notify<S>(pub Arc<Notices<S>>);

impl<S> Notify<S> {
    fn bump(&self) {
        self.0.graph.fetch_add(1, Ordering::Release);
    }
}

impl<S: Counts> jack::NotificationHandler for Notify<S> {
    fn xrun(&mut self, _: &jack::Client) -> Control {
        self.0.status.xrun();
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
        self.0.status.rate(rate);
        Control::Continue
    }
}

/// A graph of ports and connections, JACK's or a simulation's.
pub trait Graph {
    fn exists(&self, port: &str) -> bool;
    fn connected(&self, from: &str, to: &str) -> bool;
    fn connect(&mut self, from: &str, to: &str) -> Result<(), String>;
    fn disconnect(&mut self, from: &str, to: &str);
}

/// JACK's graph, through a client.
pub struct JackGraph<'a>(pub &'a jack::Client);

impl Graph for JackGraph<'_> {
    fn exists(&self, port: &str) -> bool {
        self.0.port_by_name(port).is_some()
    }

    fn connected(&self, from: &str, to: &str) -> bool {
        self.0.port_by_name(from).is_some_and(|p| p.is_connected_to(to).unwrap_or(false))
    }

    fn connect(&mut self, from: &str, to: &str) -> Result<(), String> {
        self.0.connect_ports_by_name(from, to).map_err(|e| e.to_string())
    }

    fn disconnect(&mut self, from: &str, to: &str) {
        let _ = self.0.disconnect_ports_by_name(from, to);
    }
}

/// Makes every wanted connection whose ports both exist and isn't made, and cuts those in
/// `cuts` (connections that were wanted and no longer are), never anyone else's. Returns what
/// it did, for the log.
pub fn reconcile(
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
