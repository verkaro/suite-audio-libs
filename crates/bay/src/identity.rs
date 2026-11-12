//! Port identity across snapshots. A port's key is its full `client:port` name. When the
//! name is gone (a renamed port, a client back with a new serial suffix), the port is
//! matched by its position within the corresponding bundle. Whatever is left is reported
//! as unmatched, never guessed.
//!
//! This carries session state (undo, staged edits, the TUI's cursor) from one snapshot to
//! the next. A patch file has no old snapshot, so its names simply resolve or don't.

use crate::bundle::{Bundle, Bundles};
use crate::model::{ClientId, PortId, Snapshot};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchKind {
    /// Same full name, type and direction.
    Name,
    /// Same position in the corresponding bundle of the corresponding client.
    BundlePosition,
}

/// Where each port of an old snapshot went in a new one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Matching {
    /// Indexed by old port.
    map: Vec<Option<(PortId, MatchKind)>>,
}

impl Matching {
    pub fn get(&self, old: PortId) -> Option<(PortId, MatchKind)> {
        self.map[old.index()]
    }

    /// The old ports with no counterpart, in registration order.
    pub fn unmatched(&self) -> impl Iterator<Item = PortId> + '_ {
        self.map.iter().enumerate().filter(|(_, m)| m.is_none()).map(|(i, _)| PortId(i as u32))
    }
}

/// Matches every port of `old` to at most one port of `new`, and no new port twice.
pub fn match_ports(old: &Snapshot, old_b: &Bundles, new: &Snapshot, new_b: &Bundles) -> Matching {
    let mut map = vec![None; old.ports().len()];
    let mut claimed = vec![false; new.ports().len()];

    for (id, p) in old.ports() {
        if let Some(n) = new.port_named(p.name()) {
            let np = new.port(n);
            if np.ty() == p.ty() && np.dir() == p.dir() {
                map[id.index()] = Some((n, MatchKind::Name));
                claimed[n.index()] = true;
            }
        }
    }

    for (_, ob) in old_b.iter() {
        if ob.ports.iter().all(|p| map[p.index()].is_some()) {
            continue;
        }
        let Some(nc) = counterpart(old, ob.client, new) else { continue };
        let Some(nb) = corresponding_bundle(old_b, ob, new_b, nc) else { continue };
        for (pos, &op) in ob.ports.iter().enumerate() {
            let np = nb.ports[pos];
            if map[op.index()].is_none() && !claimed[np.index()] {
                map[op.index()] = Some((np, MatchKind::BundlePosition));
                claimed[np.index()] = true;
            }
        }
    }
    Matching { map }
}

/// The new client standing for an old one: the same name, else the single new client
/// whose name differs only by a `-N` serial suffix and isn't an old client's name itself.
fn counterpart(old: &Snapshot, client: ClientId, new: &Snapshot) -> Option<ClientId> {
    let name = old.client(client).name();
    if let Some(c) = new.client_named(name) {
        return Some(c);
    }
    let base = serial_base(name);
    let mut found =
        new.clients().filter(|(_, c)| serial_base(c.name()) == base && old.client_named(c.name()).is_none());
    match (found.next(), found.next()) {
        (Some((c, _)), None) => Some(c),
        _ => None,
    }
}

/// `Firefox-12` → `Firefox`. A name that is all suffix is left alone.
fn serial_base(name: &str) -> &str {
    let stem = name.trim_end_matches(|c: char| c.is_ascii_digit());
    match stem.strip_suffix('-') {
        Some(base) if !base.is_empty() && stem.len() < name.len() => base,
        _ => name,
    }
}

/// The same-sized bundle with the same label, else the same-sized one at the same index
/// among the client's bundles of that type and direction.
fn corresponding_bundle<'a>(
    old_b: &Bundles,
    ob: &Bundle,
    new_b: &'a Bundles,
    nc: ClientId,
) -> Option<&'a Bundle> {
    let candidates: Vec<&Bundle> = new_b.of_client(nc, ob.ty, ob.dir).map(|(_, b)| b).collect();
    let fits = |b: &Bundle| b.ports.len() == ob.ports.len();
    if let Some(b) = candidates.iter().find(|b| fits(b) && b.label == ob.label) {
        return Some(b);
    }
    let index = old_b.of_client(ob.client, ob.ty, ob.dir).position(|(_, b)| std::ptr::eq(b, ob))?;
    candidates.get(index).copied().filter(|b| fits(b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Direction, PortType};

    fn snap(names: &[&str]) -> (Snapshot, Bundles) {
        let mut b = Snapshot::builder();
        for n in names {
            b.port(n, PortType::Audio, Direction::Out).unwrap();
        }
        let s = b.build();
        let (bundles, _) = Bundles::derive(&s, &[]);
        (s, bundles)
    }

    /// Old port name → new port name and how, or None.
    fn table(old: &[&str], new: &[&str]) -> Vec<(String, Option<(String, MatchKind)>)> {
        let ((os, ob), (ns, nb)) = (snap(old), snap(new));
        let m = match_ports(&os, &ob, &ns, &nb);
        os.ports()
            .map(|(id, p)| (p.name().to_owned(), m.get(id).map(|(n, k)| (ns.port(n).name().to_owned(), k))))
            .collect()
    }

    fn row(old: &str, new: Option<(&str, MatchKind)>) -> (String, Option<(String, MatchKind)>) {
        (old.to_owned(), new.map(|(n, k)| (n.to_owned(), k)))
    }

    #[test]
    fn names_first_then_bundle_position() {
        use MatchKind::*;
        // out3/out4 renamed to drums_L/drums_R: same bundle index, same size.
        let t =
            table(&["R:out1", "R:out2", "R:out3", "R:out4"], &["R:out1", "R:out2", "R:drums_L", "R:drums_R"]);
        assert_eq!(
            t,
            [
                row("R:out1", Some(("R:out1", Name))),
                row("R:out2", Some(("R:out2", Name))),
                row("R:out3", Some(("R:drums_L", BundlePosition))),
                row("R:out4", Some(("R:drums_R", BundlePosition))),
            ]
        );
    }

    #[test]
    fn serial_suffixed_clients() {
        use MatchKind::*;
        let t = table(
            &["Firefox-3:output_FL", "Firefox-3:output_FR"],
            &["Firefox-7:output_FL", "Firefox-7:output_FR"],
        );
        assert_eq!(
            t,
            [
                row("Firefox-3:output_FL", Some(("Firefox-7:output_FL", BundlePosition))),
                row("Firefox-3:output_FR", Some(("Firefox-7:output_FR", BundlePosition))),
            ]
        );
        // Two candidates: no guess.
        let t = table(&["Firefox-3:out_L"], &["Firefox-7:out_L", "Firefox-8:out_L"]);
        assert_eq!(t, [row("Firefox-3:out_L", None)]);
    }

    #[test]
    fn unmatched_is_reported() {
        let ((os, ob), (ns, nb)) = (snap(&["A:x", "A:out1", "A:out2", "B:y"]), snap(&["A:out1"]));
        let m = match_ports(&os, &ob, &ns, &nb);
        let names: Vec<&str> = m.unmatched().map(|p| os.port(p).name()).collect();
        // out1/2 no longer fits a 1-port bundle, but out1 still matches by name.
        assert_eq!(names, ["A:x", "A:out2", "B:y"]);
    }

    #[test]
    fn serial_base_cases() {
        assert_eq!(serial_base("Firefox-12"), "Firefox");
        assert_eq!(serial_base("Firefox"), "Firefox");
        assert_eq!(serial_base("-12"), "-12");
        assert_eq!(serial_base("a-b-"), "a-b-");
    }
}
