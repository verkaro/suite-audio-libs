//! The JACK client: opening it (never starting jackd), the callback, the graph as the keeper
//! sees it, and noticing JACK going away. place's (`VENDORED.md`), its player generic.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use as_jack_client::{JackGraph, Notices, Notify, lock_memory, reconcile};
use jack::{AudioOut, Control, Port, PortFlags, ProcessScope};

use crate::{Handle, Keeper, OUT, Player, PortInfo, RETRY, Rt, Status, TICK, pair};

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

type Active<P> = jack::AsyncClient<Notify<Status>, Handler<P>>;

struct Session<P: Player> {
    active: Active<P>,
    notices: Arc<Notices<Status>>,
    handle: Handle<P>,
}

impl<P: Player> Session<P> {
    fn open(k: &mut Keeper<P>) -> Result<Session<P>, String> {
        let client = as_jack_client::open(&k.name)?;
        let reg =
            |name: &str| client.register_port(name, AudioOut::default()).map_err(|e| format!("{name}: {e}"));
        let out = [reg(OUT[0])?, reg(OUT[1])?];
        let (rt, handle) = pair((k.make)(client.sample_rate()));
        let handler = Handler { out, rt };
        lock_memory(|m| k.note(m));
        let notices = Notices::new(k.status.clone());
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
