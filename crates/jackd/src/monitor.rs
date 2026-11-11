//! A client that only watches the server: its xruns, its graph changing, and its going away.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use jack::Control;

/// What the server's notices leave, read from any thread.
#[derive(Default)]
struct Seen {
    xruns: AtomicU64,
    graph: AtomicU64,
    shut_down: AtomicBool,
}

/// The notices, counted only: calling JACK from a notification can deadlock.
struct Notices(Arc<Seen>);

impl jack::NotificationHandler for Notices {
    fn xrun(&mut self, _: &jack::Client) -> Control {
        self.0.xruns.fetch_add(1, Ordering::Relaxed);
        Control::Continue
    }

    unsafe fn shutdown(&mut self, _: jack::ClientStatus, _: &str) {
        self.0.shut_down.store(true, Ordering::Release);
    }

    fn client_registration(&mut self, _: &jack::Client, _: &str, _: bool) {
        self.0.graph.fetch_add(1, Ordering::Relaxed);
    }

    fn port_registration(&mut self, _: &jack::Client, _: jack::PortId, _: bool) {
        self.0.graph.fetch_add(1, Ordering::Relaxed);
    }

    fn port_rename(&mut self, _: &jack::Client, _: jack::PortId, _: &str, _: &str) -> Control {
        self.0.graph.fetch_add(1, Ordering::Relaxed);
        Control::Continue
    }

    fn ports_connected(&mut self, _: &jack::Client, _: jack::PortId, _: jack::PortId, _: bool) {
        self.0.graph.fetch_add(1, Ordering::Relaxed);
    }

    fn graph_reorder(&mut self, _: &jack::Client) -> Control {
        self.0.graph.fetch_add(1, Ordering::Relaxed);
        Control::Continue
    }
}

/// A client with no ports, watching the running server.
pub struct Monitor {
    client: Option<jack::AsyncClient<Notices, ()>>,
    seen: Arc<Seen>,
    pub rate: u32,
    pub period: u32,
}

impl Monitor {
    /// Connects to the running server under name (never starting one), with no ports.
    pub fn open(name: &str) -> Result<Monitor, String> {
        let c = crate::probe(name)?;
        let (rate, period) = (c.sample_rate(), c.buffer_size());
        let seen = Arc::new(Seen::default());
        let client = c
            .activate_async(Notices(seen.clone()), ())
            .map_err(|e| format!("can't activate the JACK client: {e}"))?;
        Ok(Monitor { client: Some(client), seen, rate, period })
    }

    fn client(&self) -> &jack::Client {
        self.client.as_ref().expect("held until dropped").as_client()
    }

    /// The xruns since it opened.
    pub fn xruns(&self) -> u64 {
        self.seen.xruns.load(Ordering::Relaxed)
    }

    /// A count that moves whenever a client, a port or a connection comes or goes.
    pub fn graph(&self) -> u64 {
        self.seen.graph.load(Ordering::Relaxed)
    }

    /// Whether the server has gone away.
    pub fn shut_down(&self) -> bool {
        self.seen.shut_down.load(Ordering::Acquire)
    }

    /// JACK's DSP load, 0 to 100.
    pub fn dsp_load(&self) -> f32 {
        self.client().cpu_load()
    }

    /// The clients with ports, apart from the hardware.
    pub fn clients(&self) -> Vec<String> {
        crate::clients(self.client())
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        if let Some(c) = self.client.take()
            && self.shut_down()
        {
            std::mem::forget(c); // the server is gone: closing it would wait on it
        }
    }
}
