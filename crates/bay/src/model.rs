//! The graph model: clients, ports with type and direction, links, and the immutable
//! [`Snapshot`] that holds them. Ids are handles into one snapshot only; identity across
//! snapshots is [`crate::identity`]'s job.

use std::collections::{BTreeSet, HashMap};
use std::fmt;

/// A port's signal type. Ordinals count each type separately, so MIDI ports never shift
/// audio ordinals. Ports of any other JACK type are left out of the model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PortType {
    Audio,
    Midi,
}

impl PortType {
    pub fn as_str(self) -> &'static str {
        match self {
            PortType::Audio => "audio",
            PortType::Midi => "midi",
        }
    }
}

/// Which way signal flows through a port: out of a client (a source) or into it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Direction {
    Out,
    In,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Out => "out",
            Direction::In => "in",
        }
    }
}

/// A port's handle within one snapshot: its index in registration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PortId(pub(crate) u32);

/// A client's handle within one snapshot: its index in order of first registration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClientId(pub(crate) u32);

impl PortId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

impl ClientId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Port {
    /// The full `client:port` name, which is the port's key across snapshots.
    name: String,
    /// Where the port part starts in `name` (after the first `:`).
    split: usize,
    client: ClientId,
    ty: PortType,
    dir: Direction,
    /// 1-based position among the client's ports of the same type and direction.
    ordinal: u32,
}

impl Port {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn client_name(&self) -> &str {
        &self.name[..self.split - 1]
    }

    /// The part after the client name, e.g. `playback_FL`.
    pub fn short_name(&self) -> &str {
        &self.name[self.split..]
    }

    pub fn client(&self) -> ClientId {
        self.client
    }

    pub fn ty(&self) -> PortType {
        self.ty
    }

    pub fn dir(&self) -> Direction {
        self.dir
    }

    pub fn ordinal(&self) -> u32 {
        self.ordinal
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Client {
    name: String,
    /// All of the client's ports, in registration order.
    ports: Vec<PortId>,
}

impl Client {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn ports(&self) -> &[PortId] {
        &self.ports
    }
}

/// A connection from an output port to an input port of the same type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Link {
    pub from: PortId,
    pub to: PortId,
}

/// The graph at one moment. It's immutable; the live model builds a new one on change.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    clients: Vec<Client>,
    ports: Vec<Port>,
    links: BTreeSet<Link>,
    by_port: HashMap<String, PortId>,
    by_client: HashMap<String, ClientId>,
}

impl Snapshot {
    pub fn builder() -> SnapshotBuilder {
        SnapshotBuilder::default()
    }

    pub fn clients(&self) -> impl ExactSizeIterator<Item = (ClientId, &Client)> {
        self.clients.iter().enumerate().map(|(i, c)| (ClientId(i as u32), c))
    }

    pub fn client(&self, id: ClientId) -> &Client {
        &self.clients[id.index()]
    }

    pub fn client_named(&self, name: &str) -> Option<ClientId> {
        self.by_client.get(name).copied()
    }

    pub fn ports(&self) -> impl ExactSizeIterator<Item = (PortId, &Port)> {
        self.ports.iter().enumerate().map(|(i, p)| (PortId(i as u32), p))
    }

    pub fn port(&self, id: PortId) -> &Port {
        &self.ports[id.index()]
    }

    pub fn port_named(&self, name: &str) -> Option<PortId> {
        self.by_port.get(name).copied()
    }

    /// A client's ports of one type and direction, in ordinal order (`[n]` is index n-1).
    pub fn ports_of(&self, client: ClientId, ty: PortType, dir: Direction) -> Vec<PortId> {
        let ports = self.client(client).ports.iter().copied();
        ports.filter(|&p| self.port(p).ty == ty && self.port(p).dir == dir).collect()
    }

    pub fn links(&self) -> &BTreeSet<Link> {
        &self.links
    }

    /// This snapshot with a plan's links deleted and added: what it will look like.
    pub fn with_plan(&self, plan: &crate::compile::Plan) -> Snapshot {
        let mut s = self.clone();
        for l in &plan.del {
            s.links.remove(l);
        }
        s.links.extend(plan.add.iter().copied());
        s
    }

    pub fn is_linked(&self, from: PortId, to: PortId) -> bool {
        self.links.contains(&Link { from, to })
    }

    /// The inputs an output feeds.
    pub fn links_from(&self, from: PortId) -> impl Iterator<Item = PortId> + '_ {
        let lo = Link { from, to: PortId(0) };
        let hi = Link { from, to: PortId(u32::MAX) };
        self.links.range(lo..=hi).map(|l| l.to)
    }

    /// The outputs feeding an input.
    pub fn links_to(&self, to: PortId) -> impl Iterator<Item = PortId> + '_ {
        self.links.iter().filter(move |l| l.to == to).map(|l| l.from)
    }
}

/// Builds a [`Snapshot`] port by port and link by link, as a backend reports them.
#[derive(Debug, Default)]
pub struct SnapshotBuilder {
    snap: Snapshot,
}

impl SnapshotBuilder {
    /// Registers a port by its full `client:port` name.
    pub fn port(&mut self, name: &str, ty: PortType, dir: Direction) -> Result<PortId, ModelError> {
        let split = match name.find(':') {
            Some(i) if i > 0 && i + 1 < name.len() => i + 1,
            _ => return Err(ModelError::BadPortName(name.to_owned())),
        };
        if self.snap.by_port.contains_key(name) {
            return Err(ModelError::DuplicatePort(name.to_owned()));
        }
        let s = &mut self.snap;
        let client_name = &name[..split - 1];
        let client = match s.by_client.get(client_name) {
            Some(&c) => c,
            None => {
                let c = ClientId(s.clients.len() as u32);
                s.clients.push(Client { name: client_name.to_owned(), ports: Vec::new() });
                s.by_client.insert(client_name.to_owned(), c);
                c
            }
        };
        let siblings = &s.clients[client.index()].ports;
        let ordinal = 1 + siblings
            .iter()
            .filter(|&&p| s.ports[p.index()].ty == ty && s.ports[p.index()].dir == dir)
            .count();
        let id = PortId(s.ports.len() as u32);
        s.ports.push(Port { name: name.to_owned(), split, client, ty, dir, ordinal: ordinal as u32 });
        s.clients[client.index()].ports.push(id);
        s.by_port.insert(name.to_owned(), id);
        Ok(id)
    }

    /// Links two registered ports by name. Linking twice is not an error.
    pub fn link(&mut self, from: &str, to: &str) -> Result<(), ModelError> {
        let find = |n: &str| self.snap.port_named(n).ok_or_else(|| ModelError::UnknownPort(n.to_owned()));
        let (f, t) = (find(from)?, find(to)?);
        let (fp, tp) = (self.snap.port(f), self.snap.port(t));
        if fp.dir != Direction::Out {
            return Err(ModelError::NotAnOutput(from.to_owned()));
        }
        if tp.dir != Direction::In {
            return Err(ModelError::NotAnInput(to.to_owned()));
        }
        if fp.ty != tp.ty {
            return Err(ModelError::TypeMismatch { from: from.to_owned(), to: to.to_owned() });
        }
        self.snap.links.insert(Link { from: f, to: t });
        Ok(())
    }

    pub fn build(self) -> Snapshot {
        self.snap
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelError {
    BadPortName(String),
    DuplicatePort(String),
    UnknownPort(String),
    NotAnOutput(String),
    NotAnInput(String),
    TypeMismatch { from: String, to: String },
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelError::BadPortName(n) => write!(f, "{n:?} is not a client:port name"),
            ModelError::DuplicatePort(n) => write!(f, "port {n} is already registered"),
            ModelError::UnknownPort(n) => write!(f, "no port {n}"),
            ModelError::NotAnOutput(n) => write!(f, "{n} is not an output"),
            ModelError::NotAnInput(n) => write!(f, "{n} is not an input"),
            ModelError::TypeMismatch { from, to } => write!(f, "{from} and {to} carry different types"),
        }
    }
}

impl std::error::Error for ModelError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn small() -> Snapshot {
        let mut b = Snapshot::builder();
        b.port("A:out1", PortType::Audio, Direction::Out).unwrap();
        b.port("A:midi", PortType::Midi, Direction::Out).unwrap();
        b.port("A:out2", PortType::Audio, Direction::Out).unwrap();
        b.port("A:in1", PortType::Audio, Direction::In).unwrap();
        b.port("B:in:odd", PortType::Audio, Direction::In).unwrap();
        b.port("B:midi_in", PortType::Midi, Direction::In).unwrap();
        b.link("A:out2", "B:in:odd").unwrap();
        b.link("A:midi", "B:midi_in").unwrap();
        b.build()
    }

    #[test]
    fn ordinals_count_type_and_direction_separately() {
        let s = small();
        let ord = |n: &str| s.port(s.port_named(n).unwrap()).ordinal();
        assert_eq!([ord("A:out1"), ord("A:midi"), ord("A:out2"), ord("A:in1")], [1, 1, 2, 1]);
        let a = s.client_named("A").unwrap();
        let outs = s.ports_of(a, PortType::Audio, Direction::Out);
        assert_eq!(outs, [s.port_named("A:out1").unwrap(), s.port_named("A:out2").unwrap()]);
    }

    #[test]
    fn names_split_at_the_first_colon() {
        let s = small();
        let p = s.port(s.port_named("B:in:odd").unwrap());
        assert_eq!((p.client_name(), p.short_name()), ("B", "in:odd"));
    }

    #[test]
    fn links_both_ways() {
        let s = small();
        let (out2, inp) = (s.port_named("A:out2").unwrap(), s.port_named("B:in:odd").unwrap());
        assert!(s.is_linked(out2, inp));
        assert_eq!(s.links_from(out2).collect::<Vec<_>>(), [inp]);
        assert_eq!(s.links_to(inp).collect::<Vec<_>>(), [out2]);
    }

    #[test]
    fn builder_rejects_bad_input() {
        let mut b = Snapshot::builder();
        b.port("A:o", PortType::Audio, Direction::Out).unwrap();
        b.port("A:m", PortType::Midi, Direction::In).unwrap();
        b.port("A:i", PortType::Audio, Direction::In).unwrap();
        for bad in ["noclient", ":p", "c:"] {
            assert_eq!(
                b.port(bad, PortType::Audio, Direction::Out),
                Err(ModelError::BadPortName(bad.into()))
            );
        }
        assert!(matches!(b.port("A:o", PortType::Audio, Direction::In), Err(ModelError::DuplicatePort(_))));
        assert!(matches!(b.link("A:i", "A:o"), Err(ModelError::NotAnOutput(_))));
        assert!(matches!(b.link("A:o", "A:o"), Err(ModelError::NotAnInput(_))));
        assert!(matches!(b.link("A:o", "A:m"), Err(ModelError::TypeMismatch { .. })));
        assert!(matches!(b.link("A:o", "Z:z"), Err(ModelError::UnknownPort(_))));
    }
}
