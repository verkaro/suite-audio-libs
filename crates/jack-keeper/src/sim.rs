//! `--sim`: no JACK. A fake graph (an interface's eight outputs, the mixer's sixteen LIVE
//! inputs, Dexter's sixteen tracks), and the same player run on a timer at 48 kHz in periods of
//! 512, its output going nowhere but its meters. For showing a page without an interface, and
//! for the tests. place's (`VENDORED.md`).

use std::collections::BTreeSet;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use crate::{Graph, Keeper, OUT, Player, PortInfo, pair, reconcile};

pub const RATE: u32 = 48_000;
pub const PERIOD: usize = 512;

/// The simulated graph's ports that take audio in.
fn studio() -> Vec<PortInfo> {
    let mut p = Vec::new();
    for i in 1..=8 {
        p.push(PortInfo { name: format!("system:playback_{i}"), physical: true });
    }
    for i in 1..=16 {
        p.push(PortInfo { name: format!("mixer:live_{i:02}"), physical: false });
        p.push(PortInfo { name: format!("dexter:in_{i:02}"), physical: false });
    }
    p
}

struct SimGraph {
    ports: BTreeSet<String>,
    links: BTreeSet<(String, String)>,
}

impl Graph for SimGraph {
    fn exists(&self, port: &str) -> bool {
        self.ports.contains(port)
    }

    fn connected(&self, from: &str, to: &str) -> bool {
        self.links.contains(&(from.to_string(), to.to_string()))
    }

    fn connect(&mut self, from: &str, to: &str) -> Result<(), String> {
        self.links.insert((from.to_string(), to.to_string()));
        Ok(())
    }

    fn disconnect(&mut self, from: &str, to: &str) {
        self.links.remove(&(from.to_string(), to.to_string()));
    }
}

/// The keeper, for `--sim`.
pub(crate) fn keep<P: Player>(mut k: Keeper<P>) {
    let others = studio();
    k.status.set_ports(others.clone());
    k.status.rate.store(RATE, Ordering::Relaxed);
    k.status.period.store(PERIOD as u32, Ordering::Relaxed);
    let mut ports: BTreeSet<String> = others.into_iter().map(|p| p.name).collect();
    for o in OUT {
        ports.insert(format!("{}:{o}", k.name));
    }
    let mut graph = SimGraph { ports, links: BTreeSet::new() };
    let (mut rt, mut handle) = pair((k.make)());
    k.status.attached.store(true, Ordering::Release);
    k.note(&format!("simulating JACK: {RATE} Hz, {PERIOD} frames"));
    let (mut l, mut r) = (vec![0.0f32; PERIOD], vec![0.0f32; PERIOD]);
    let mut cuts = Vec::new();
    let mut dirty = true;
    let period = Duration::from_secs_f64(PERIOD as f64 / RATE as f64);
    let mut next = Instant::now();
    while !k.stopping() {
        if let Some(c) = k.wait(Duration::ZERO) {
            cuts.extend(c);
            dirty = true;
        }
        if dirty {
            dirty = false;
            for line in reconcile(&mut graph, &k.links, &cuts) {
                k.note(&line);
            }
            cuts.clear();
        }
        k.forward(&mut handle);
        rt.period(&mut l, &mut r);
        next += period;
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        } else if now - next > Duration::from_millis(200) {
            next = now; // fell behind (a suspended machine): don't race to catch up
        }
    }
    k.status.attached.store(false, Ordering::Release);
}
