//! The patch bay's JACK backend, which covers PipeWire through pipewire-jack (bay-rs's
//! `PLAN.md` §4).
//!
//! Moved in from Dexter's `bay-jack` at `e98d44f` (2026-11-11), for jackbay (`RFC-as-bay.md`,
//! b1). It's Warren's bay, bay-rs's `crates/jack` at `af5c0c7`, which Dexter vendored on
//! 2026-09-29. As it was but for its name, and [`Jack::open_as`], added here.
//!
//! JACK's notification callbacks only bump a counter ([`GraphNotices`]): calling JACK from
//! them can deadlock. Owners rebuild a whole snapshot from the port list when it moves.
//!
//! [`snapshot_of`] and [`run_on`] work over any `jack::Client`, so a program that owns its
//! client (Dexter's engine) uses them directly, with its own handler bumping a
//! [`GraphNotices`]. [`Jack`] is bay's own backend over them.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use as_bay::backend::execute;
use as_bay::{Backend, BackendError, Direction, Plan, PortType, Report, Snapshot};
use jack::{Client, ClientOptions, ClientStatus, Control, PortFlags, PortId};

/// What [`Jack::open`] asks to be called; JACK adds a suffix when it's taken.
pub const CLIENT_NAME: &str = "bay";

const AUDIO: &str = "32 bit float mono audio";
const MIDI: &str = "8 bit raw midi";

/// A counter of graph notices (ports and clients coming and going, renames, connections).
/// Any JACK notification handler can bump it: bay's own, or a program that owns its client
/// and uses bay's functions over it. An owner takes a new snapshot when it moves. It's an
/// atomic, so bumping it from JACK's notification thread never blocks.
#[derive(Debug, Default)]
pub struct GraphNotices {
    generation: AtomicU64,
}

impl GraphNotices {
    pub fn bump(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
}

/// The graph as `client` sees it: every audio and MIDI port and the links between them,
/// in JACK's port order (the ordinal order, `PLAN.md` §4). Ports whose full names start with
/// `skip` are left out: a client's own, say (`"bay:"`), or nothing. It only reads: it never
/// opens, activates or closes a client, or registers anything, so it works over a client
/// the caller owns.
pub fn snapshot_of(client: &Client, skip: Option<&str>) -> Snapshot {
    let mut b = Snapshot::builder();
    let mut outs = Vec::new();
    for name in client.ports(None, None, PortFlags::empty()) {
        if skip.is_some_and(|p| name.starts_with(p)) {
            continue;
        }
        // Gone since the listing: skip it; the notices have moved, so the next snapshot
        // catches up.
        let Some(port) = client.port_by_name(&name) else { continue };
        let ty = match port.port_type().as_deref() {
            Ok(AUDIO) => PortType::Audio,
            Ok(MIDI) => PortType::Midi,
            _ => continue,
        };
        let flags = port.flags();
        let dir = if flags.contains(PortFlags::IS_OUTPUT) {
            Direction::Out
        } else if flags.contains(PortFlags::IS_INPUT) {
            Direction::In
        } else {
            continue;
        };
        if b.port(&name, ty, dir).is_ok() && dir == Direction::Out {
            outs.push((name, port));
        }
    }
    for (name, port) in outs {
        for to in port.get_connections() {
            // A link to a port left out (skipped, or another type) just isn't shown.
            let _ = b.link(&name, &to);
        }
    }
    b.build()
}

/// Runs `plan`, whose ids are `snap`'s, through `client`: deletions, then additions,
/// continuing past failures. A link already as wanted counts as applied. Like
/// [`snapshot_of`], it works over a client the caller owns and registers nothing.
pub fn run_on(client: &Client, snap: &Snapshot, plan: &Plan) -> Report {
    execute(snap, plan, |from, to, connect| {
        let linked =
            || client.port_by_name(from).is_some_and(|p| p.get_connections().iter().any(|c| c == to));
        if connect {
            match client.connect_ports_by_name(from, to) {
                Ok(()) | Err(jack::Error::PortAlreadyConnected(..)) => Ok(()),
                Err(e) => Err(e.to_string()),
            }
        } else {
            match client.disconnect_ports_by_name(from, to) {
                Ok(()) => Ok(()),
                // JACK doesn't say why; not being linked is what was wanted.
                Err(_) if client.port_by_name(from).is_some() && !linked() => Ok(()),
                Err(e) => Err(e.to_string()),
            }
        }
    })
}

/// bay's own backend: a client of its own, named `bay` or as the tool says
/// ([`Jack::open_as`]), over [`snapshot_of`] and [`run_on`], leaving its own ports out.
pub struct Jack {
    client: jack::AsyncClient<Notify, ()>,
    /// `<our name>:`, to leave our own ports out.
    own: String,
    state: Arc<State>,
}

#[derive(Default)]
struct State {
    notices: GraphNotices,
    shut_down: AtomicBool,
}

impl Jack {
    /// Connects to the running server (never starting one), as `bay`.
    pub fn open() -> Result<Jack, BackendError> {
        Jack::open_as(CLIENT_NAME)
    }

    /// Connects to the running server (never starting one) under a tool's own name, as
    /// jackbay's is `jackbay`. JACK adds a suffix when it's taken: [`Jack::name`] says.
    pub fn open_as(name: &str) -> Result<Jack, BackendError> {
        let (client, _) = Client::new(name, ClientOptions::NO_START_SERVER)
            .map_err(|e| BackendError(format!("can't reach a JACK server: {e}")))?;
        let own = format!("{}:", client.name());
        let state = Arc::new(State::default());
        let client = client
            .activate_async(Notify { state: state.clone() }, ())
            .map_err(|e| BackendError(format!("can't activate the JACK client: {e}")))?;
        Ok(Jack { client, own, state })
    }

    /// The name JACK gave us.
    pub fn name(&self) -> &str {
        self.client.as_client().name()
    }

    fn alive(&self) -> Result<&Client, BackendError> {
        if self.state.shut_down.load(Ordering::Acquire) {
            return Err(BackendError("the JACK server went away".into()));
        }
        Ok(self.client.as_client())
    }
}

impl Backend for Jack {
    fn snapshot(&mut self) -> Result<Snapshot, BackendError> {
        Ok(snapshot_of(self.alive()?, Some(&self.own)))
    }

    fn run(&mut self, snap: &Snapshot, plan: &Plan) -> Report {
        match self.alive() {
            Ok(client) => run_on(client, snap, plan),
            Err(e) => execute(snap, plan, |_, _, _| Err(e.0.clone())),
        }
    }

    fn generation(&self) -> u64 {
        self.state.notices.generation()
    }
}

/// Bumps the notices on every graph notice, and notes the server going away.
struct Notify {
    state: Arc<State>,
}

impl Notify {
    fn bump(&self) {
        self.state.notices.bump();
    }
}

impl jack::NotificationHandler for Notify {
    unsafe fn shutdown(&mut self, _: ClientStatus, _: &str) {
        self.state.shut_down.store(true, Ordering::Release);
        self.bump();
    }

    fn client_registration(&mut self, _: &Client, _: &str, _: bool) {
        self.bump();
    }

    fn port_registration(&mut self, _: &Client, _: PortId, _: bool) {
        self.bump();
    }

    fn port_rename(&mut self, _: &Client, _: PortId, _: &str, _: &str) -> Control {
        self.bump();
        Control::Continue
    }

    fn ports_connected(&mut self, _: &Client, _: PortId, _: PortId, _: bool) {
        self.bump();
    }

    fn graph_reorder(&mut self, _: &Client) -> Control {
        self.bump();
        Control::Continue
    }
}
