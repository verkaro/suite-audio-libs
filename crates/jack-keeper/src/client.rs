//! The JACK client: opening it (never starting jackd), the callback, the graph as the keeper
//! sees it, and noticing JACK going away. place's (`VENDORED.md`), its player generic.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use jack::{AudioOut, ClientOptions, Control, Port, PortFlags, ProcessScope};

use crate::{Graph, Handle, Keeper, OUT, Player, PortInfo, RETRY, Rt, TICK, pair, reconcile};

const AUDIO_TYPE: &str = "32 bit float mono audio";

/// The keeper, for JACK: opens the client and keeps it, until stopped.
pub(crate) fn keep<P: Player>(mut k: Keeper<P>) {
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
                k.sleep(RETRY);
            }
        }
    }
}

type Active<P> = jack::AsyncClient<Notify, Handler<P>>;

struct Session<P: Player> {
    active: Active<P>,
    notices: Arc<Notices>,
    handle: Handle<P>,
}

impl<P: Player> Session<P> {
    fn open(k: &mut Keeper<P>) -> Result<Session<P>, String> {
        let opts = ClientOptions::NO_START_SERVER | ClientOptions::USE_EXACT_NAME;
        let client = match jack::Client::new(&k.name, opts) {
            Ok((c, _)) => c,
            Err(e) => return Err(why_not(&k.name, &e.to_string())),
        };
        let reg =
            |name: &str| client.register_port(name, AudioOut::default()).map_err(|e| format!("{name}: {e}"));
        let out = [reg(OUT[0])?, reg(OUT[1])?];
        let (rt, handle) = pair((k.make)(client.sample_rate()));
        let handler = Handler { out, rt };
        lock_memory(k);
        let notices = Arc::new(Notices { status: k.status.clone(), ..Notices::default() });
        let (rate, period) = (client.sample_rate(), client.buffer_size());
        let active = client
            .activate_async(Notify(notices.clone()), handler)
            .map_err(|e| format!("can't activate the client: {e}"))?;
        k.status.rate.store(rate, Ordering::Relaxed);
        k.status.period.store(period, Ordering::Relaxed);
        k.status.attached.store(true, Ordering::Release);
        k.note(&format!("attached to JACK: {rate} Hz, {period} frames"));
        Ok(Session { active, notices, handle })
    }

    /// Keeps the client until it's stopped or JACK goes away.
    fn run(mut self, k: &mut Keeper<P>) {
        let mut seen = u64::MAX;
        let mut cuts = Vec::new();
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
            k.forward(&mut self.handle);
            let g = self.notices.graph.load(Ordering::Acquire);
            if g != seen || dirty {
                seen = g;
                dirty = false;
                k.status.set_ports(graph_ports(self.active.as_client()));
                k.status.latency.store(latency(self.active.as_client(), &k.name), Ordering::Relaxed);
                let mut graph = JackGraph(self.active.as_client());
                for line in reconcile(&mut graph, &k.links, &cuts) {
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

/// The outputs' playback latency as JACK has it, frames: the most of the two.
fn latency(c: &jack::Client, me: &str) -> u32 {
    OUT.iter()
        .filter_map(|o| c.port_by_name(&format!("{me}:{o}")))
        .map(|p| p.get_latency_range(jack::LatencyType::Playback).1)
        .max()
        .unwrap_or(0)
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

/// Locks the process's memory so far, the callback's included, where the limit allows.
fn lock_memory<P: Player>(k: &Keeper<P>) {
    static SAID: AtomicBool = AtomicBool::new(false);
    // SAFETY: mlockall only changes how the process's pages are kept.
    let locked = unsafe { libc::mlockall(libc::MCL_CURRENT) } == 0;
    if !locked && !SAID.swap(true, Ordering::Relaxed) {
        k.note("couldn't lock memory (ulimit -l); an xrun is likelier under memory pressure");
    }
}

/// Every audio port of the graph that takes audio in, but the client's own.
fn graph_ports(c: &jack::Client) -> Vec<PortInfo> {
    let physical = c.ports(None, Some(AUDIO_TYPE), PortFlags::IS_PHYSICAL);
    let mine = format!("{}:", c.name());
    c.ports(None, Some(AUDIO_TYPE), PortFlags::IS_INPUT)
        .into_iter()
        .filter(|n| !n.starts_with(&mine))
        .map(|n| PortInfo { physical: physical.contains(&n), name: n })
        .collect()
}

struct JackGraph<'a>(&'a jack::Client);

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

/// What JACK's notices leave for the keeper: counted or flagged only, since calling JACK from
/// a notification can deadlock.
#[derive(Default)]
struct Notices {
    status: Arc<crate::Status>,
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

/// The process callback: the two outputs' buffers, handed to the player.
struct Handler<P: Player> {
    out: [Port<AudioOut>; 2],
    rt: Rt<P>,
}

impl<P: Player> jack::ProcessHandler for Handler<P> {
    fn process(&mut self, _: &jack::Client, ps: &ProcessScope) -> Control {
        let [l, r] = &mut self.out;
        self.rt.period(l.as_mut_slice(ps), r.as_mut_slice(ps));
        Control::Continue
    }
}
