//! Bundles: groups of one client's ports (same type and direction) treated as one unit.
//! JACK gives no grouping, so [`Bundles::derive`] guesses from port names, and declared
//! bundles ([`BundleDecl`], from the session) take their ports first.
//!
//! The heuristics, for audio ports in ordinal order:
//! - Channel positions with a common stem form one bundle (`capture_FL`, `capture_FR`;
//!   also `FC`, `LFE`, `RL`, `RR`, `SL`, `SR`, in any order).
//! - `L` then `R`, or `left` then `right`, with a common stem pair up (`out_L`, `out_R`).
//! - Numbers pair odd with the next even with a common stem (`out1`+`out2`, `capture_3`+
//!   `capture_4`); an odd one out stays single.
//!
//! Everything else, and every MIDI port, is a bundle of one.
//!
//! With [`Grouping::Mono`] there are no heuristics: every port nobody declared is a bundle
//! of one (a multitrack recorder's patch bay, where every channel is its own).

use std::fmt;

use crate::model::{ClientId, Direction, PortId, PortType, Snapshot};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BundleId(u32);

impl BundleId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bundle {
    /// The declared name, or a description derived from the port names (`capture_FL/FR`).
    pub label: String,
    pub declared: bool,
    pub client: ClientId,
    pub ty: PortType,
    pub dir: Direction,
    /// In ordinal order, contiguous.
    pub ports: Vec<PortId>,
}

/// A user's bundle: ports `first..=last` (ordinals) of one client, type and direction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BundleDecl {
    pub name: String,
    pub client: String,
    pub ty: PortType,
    pub dir: Direction,
    pub first: u32,
    pub last: u32,
}

/// Why a declaration didn't apply to a snapshot. The rest still apply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeclProblem {
    NoClient { name: String },
    OutOfRange { name: String, have: u32 },
    Overlaps { name: String, with: String },
}

impl fmt::Display for DeclProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeclProblem::NoClient { name } => write!(f, "bundle {name}: no such client"),
            DeclProblem::OutOfRange { name, have } => write!(f, "bundle {name}: the client has {have} ports"),
            DeclProblem::Overlaps { name, with } => write!(f, "bundle {name} overlaps bundle {with}"),
        }
    }
}

/// Every port of a snapshot in exactly one bundle.
#[derive(Clone, Debug, Default)]
pub struct Bundles {
    list: Vec<Bundle>,
    /// Indexed by port: its bundle and 0-based position in it.
    of_port: Vec<(BundleId, u32)>,
}

/// How ports nobody declared are grouped.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Grouping {
    /// By their names (the heuristics above): `capture_1/2`, `out_L/R`.
    #[default]
    Guess,
    /// Each a bundle of one.
    Mono,
}

impl Bundles {
    /// Groups the snapshot's ports: declarations first, heuristics for the rest.
    pub fn derive(snap: &Snapshot, decls: &[BundleDecl]) -> (Bundles, Vec<DeclProblem>) {
        Bundles::derive_with(snap, decls, Grouping::Guess)
    }

    /// Groups the snapshot's ports: declarations first, the rest as grouping says.
    pub fn derive_with(
        snap: &Snapshot,
        decls: &[BundleDecl],
        grouping: Grouping,
    ) -> (Bundles, Vec<DeclProblem>) {
        let mut problems = Vec::new();
        let mut out = Bundles { list: Vec::new(), of_port: vec![(BundleId(0), 0); snap.ports().len()] };
        for (cid, _) in snap.clients() {
            for ty in [PortType::Audio, PortType::Midi] {
                for dir in [Direction::Out, Direction::In] {
                    let ports = snap.ports_of(cid, ty, dir);
                    if !ports.is_empty() {
                        out.group(snap, cid, ty, dir, &ports, decls, grouping, &mut problems);
                    }
                }
            }
        }
        for d in decls {
            if snap.client_named(&d.client).is_none() {
                problems.push(DeclProblem::NoClient { name: d.name.clone() });
            }
        }
        (out, problems)
    }

    #[allow(clippy::too_many_arguments)]
    fn group(
        &mut self,
        snap: &Snapshot,
        client: ClientId,
        ty: PortType,
        dir: Direction,
        ports: &[PortId],
        decls: &[BundleDecl],
        grouping: Grouping,
        problems: &mut Vec<DeclProblem>,
    ) {
        // Which declaration (by index into `decls`) owns each port, if any.
        let mut owner: Vec<Option<usize>> = vec![None; ports.len()];
        let name = snap.client(client).name();
        for (di, d) in decls.iter().enumerate() {
            if d.client != name || d.ty != ty || d.dir != dir {
                continue;
            }
            if d.first < 1 || d.last < d.first || d.last as usize > ports.len() {
                problems.push(DeclProblem::OutOfRange { name: d.name.clone(), have: ports.len() as u32 });
                continue;
            }
            let span = d.first as usize - 1..d.last as usize;
            if let Some(o) = owner[span.clone()].iter().flatten().next() {
                problems.push(DeclProblem::Overlaps { name: d.name.clone(), with: decls[*o].name.clone() });
                continue;
            }
            owner[span].fill(Some(di));
        }

        let mut i = 0;
        while i < ports.len() {
            let len = match owner[i] {
                Some(di) => {
                    let d = &decls[di];
                    let len = (d.last - d.first + 1) as usize;
                    self.push(Bundle {
                        label: d.name.clone(),
                        declared: true,
                        client,
                        ty,
                        dir,
                        ports: ports[i..i + len].to_vec(),
                    });
                    len
                }
                None => {
                    let free = owner[i..].iter().take_while(|o| o.is_none()).count();
                    let names: Vec<&str> =
                        ports[i..i + free].iter().map(|&p| snap.port(p).short_name()).collect();
                    let (len, label) = if ty == PortType::Audio && grouping == Grouping::Guess {
                        guess(&names)
                    } else {
                        (1, names[0].to_owned())
                    };
                    self.push(Bundle {
                        label,
                        declared: false,
                        client,
                        ty,
                        dir,
                        ports: ports[i..i + len].to_vec(),
                    });
                    len
                }
            };
            i += len;
        }
    }

    fn push(&mut self, b: Bundle) {
        let id = BundleId(self.list.len() as u32);
        for (pos, p) in b.ports.iter().enumerate() {
            self.of_port[p.index()] = (id, pos as u32);
        }
        self.list.push(b);
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (BundleId, &Bundle)> {
        self.list.iter().enumerate().map(|(i, b)| (BundleId(i as u32), b))
    }

    pub fn get(&self, id: BundleId) -> &Bundle {
        &self.list[id.index()]
    }

    /// The port's bundle and its 0-based position in it.
    pub fn of_port(&self, port: PortId) -> (BundleId, u32) {
        self.of_port[port.index()]
    }

    /// One client's bundles of a type and direction, in ordinal order.
    pub fn of_client(
        &self,
        client: ClientId,
        ty: PortType,
        dir: Direction,
    ) -> impl Iterator<Item = (BundleId, &Bundle)> {
        self.iter().filter(move |(_, b)| b.client == client && b.ty == ty && b.dir == dir)
    }
}

/// How an output bundle and an input bundle are linked: the matrix cell between them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    /// Different port types: no cell.
    Types,
    Empty,
    /// Exactly the pairing `A > B` would make (zip, fan-out or fan-in).
    Full,
    /// Exactly the channels swapped: equal sizes of two or more, i → n-1-i.
    Crossed,
    /// Any other links.
    Partial,
}

impl Cell {
    pub fn of(snap: &Snapshot, row: &Bundle, col: &Bundle) -> Cell {
        if row.ty != col.ty {
            return Cell::Types;
        }
        let have: Vec<(usize, usize)> = row
            .ports
            .iter()
            .enumerate()
            .flat_map(|(i, &f)| {
                col.ports
                    .iter()
                    .enumerate()
                    .filter(move |&(_, &t)| snap.is_linked(f, t))
                    .map(move |(j, _)| (i, j))
            })
            .collect();
        let (n, m) = (row.ports.len(), col.ports.len());
        let full: Vec<(usize, usize)> = if n == m {
            (0..n).map(|i| (i, i)).collect()
        } else if n == 1 {
            (0..m).map(|j| (0, j)).collect()
        } else if m == 1 {
            (0..n).map(|i| (i, 0)).collect()
        } else {
            Vec::new()
        };
        if have.is_empty() {
            Cell::Empty
        } else if have == full {
            Cell::Full
        } else if n == m && n >= 2 && have == (0..n).map(|i| (i, n - 1 - i)).collect::<Vec<_>>() {
            Cell::Crossed
        } else {
            Cell::Partial
        }
    }
}

/// How the suffix of a port name places it in a group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Chan {
    /// A surround position: FL, FR, FC, LFE, RL, RR, SL, SR.
    Pos(u8),
    /// Left (false) or right (true), from `L`/`R` or `left`/`right`.
    Side(bool),
    Num(u64),
}

const POSITIONS: [&str; 8] = ["FL", "FR", "FC", "LFE", "RL", "RR", "SL", "SR"];

/// Splits `capture_FL` into (`capture_`, Pos(FL)).
fn channel(name: &str) -> Option<(&str, Chan)> {
    let digits = name.len() - name.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    if digits > 0 {
        let (stem, n) = name.split_at(name.len() - digits);
        return n.parse().ok().map(|n| (stem, Chan::Num(n)));
    }
    // A word suffix counts only after a separator, or as the whole name.
    let word = |suffix: &str| -> Option<&str> {
        let cut = name.len().checked_sub(suffix.len())?;
        if !name.is_char_boundary(cut) || !name[cut..].eq_ignore_ascii_case(suffix) {
            return None;
        }
        let stem = &name[..cut];
        (stem.is_empty() || stem.ends_with(['_', '-', '.', ' '])).then_some(stem)
    };
    for (i, p) in POSITIONS.iter().enumerate() {
        if let Some(stem) = word(p) {
            return Some((stem, Chan::Pos(i as u8)));
        }
    }
    for (s, right) in [("left", false), ("right", true), ("L", false), ("R", true)] {
        if let Some(stem) = word(s) {
            return Some((stem, Chan::Side(right)));
        }
    }
    None
}

/// Guesses the bundle starting at `names[0]`: how many ports it takes, and its label.
fn guess(names: &[&str]) -> (usize, String) {
    let single = (1, names[0].to_owned());
    let Some((stem, first)) = channel(names[0]) else { return single };
    let same_stem = |n: &&str| channel(n).filter(|(s, _)| s.eq_ignore_ascii_case(stem)).map(|(_, c)| c);
    let len = match first {
        Chan::Pos(_) => {
            let mut seen = vec![first];
            for c in names[1..].iter().map_while(same_stem) {
                if !matches!(c, Chan::Pos(_)) || seen.contains(&c) {
                    break;
                }
                seen.push(c);
            }
            seen.len()
        }
        Chan::Side(false) => {
            1 + names.get(1).and_then(same_stem).is_some_and(|c| c == Chan::Side(true)) as usize
        }
        Chan::Num(n) if n % 2 == 1 => {
            1 + names.get(1).and_then(same_stem).is_some_and(|c| c == Chan::Num(n + 1)) as usize
        }
        _ => 1,
    };
    if len == 1 {
        return single;
    }
    let suffixes: Vec<&str> = names[..len].iter().map(|n| &n[stem.len()..]).collect();
    (len, format!("{stem}{}", suffixes.join("/")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(ports: &[(&str, PortType, Direction)]) -> Snapshot {
        let mut b = Snapshot::builder();
        for &(n, t, d) in ports {
            b.port(n, t, d).unwrap();
        }
        b.build()
    }

    fn outs(names: &[&str]) -> Snapshot {
        let ports: Vec<_> = names.iter().map(|n| (*n, PortType::Audio, Direction::Out)).collect();
        snap(&ports)
    }

    fn labels(s: &Snapshot, decls: &[BundleDecl]) -> Vec<String> {
        let (b, problems) = Bundles::derive(s, decls);
        assert_eq!(problems, []);
        b.iter().map(|(_, b)| format!("{}:{}", b.label, b.ports.len())).collect()
    }

    /// Mono: every port nobody declared is a bundle of its own; declarations still hold.
    #[test]
    fn mono() {
        let s = outs(&["c:capture_1", "c:capture_2", "c:out_L", "c:out_R", "c:capture_FL", "c:capture_FR"]);
        let decls = [BundleDecl {
            name: "pair".into(),
            client: "c".into(),
            ty: PortType::Audio,
            dir: Direction::Out,
            first: 3,
            last: 4,
        }];
        let (b, problems) = Bundles::derive_with(&s, &decls, Grouping::Mono);
        assert_eq!(problems, []);
        let got: Vec<String> = b.iter().map(|(_, b)| format!("{}:{}", b.label, b.ports.len())).collect();
        assert_eq!(got, ["capture_1:1", "capture_2:1", "pair:2", "capture_FL:1", "capture_FR:1"]);
        assert_eq!(labels(&s, &[]), ["capture_1/2:2", "out_L/R:2", "capture_FL/FR:2"], "guessing, as before");
    }

    #[test]
    fn heuristics() {
        let s = outs(&["c:capture_FL", "c:capture_FR", "c:out_L", "c:out_R", "c:left", "c:right"]);
        assert_eq!(labels(&s, &[]), ["capture_FL/FR:2", "out_L/R:2", "left/right:2"]);

        let s = outs(&["c:out1", "c:out2", "c:out3", "c:out4", "c:out5"]);
        assert_eq!(labels(&s, &[]), ["out1/2:2", "out3/4:2", "out5:1"]);

        // Even-first numbers don't pair; odd pairs only with the next number.
        let s = outs(&["c:ch2", "c:ch3", "c:ch4", "c:ch6"]);
        assert_eq!(labels(&s, &[]), ["ch2:1", "ch3/4:2", "ch6:1"]);

        let s = outs(&["c:p_FL", "c:p_FR", "c:p_RL", "c:p_RR", "c:p_FC", "c:p_LFE", "c:mono"]);
        assert_eq!(labels(&s, &[]), ["p_FL/FR/RL/RR/FC/LFE:6", "mono:1"]);

        // Stems must match, and L/R need a separator.
        let s = outs(&["c:a_L", "c:b_R", "c:DRL", "c:DRR"]);
        assert_eq!(labels(&s, &[]), ["a_L:1", "b_R:1", "DRL:1", "DRR:1"]);
    }

    #[test]
    fn midi_and_directions_stay_apart() {
        let s = snap(&[
            ("c:out1", PortType::Audio, Direction::Out),
            ("c:in1", PortType::Audio, Direction::In),
            ("c:out2", PortType::Audio, Direction::Out),
            ("c:m1", PortType::Midi, Direction::Out),
            ("c:m2", PortType::Midi, Direction::Out),
        ]);
        assert_eq!(labels(&s, &[]), ["out1/2:2", "in1:1", "m1:1", "m2:1"]);
    }

    #[test]
    fn declarations_take_their_ports_first() {
        let names: Vec<String> = (1..=8).map(|n| format!("R:out{n}")).collect();
        let s = outs(&names.iter().map(String::as_str).collect::<Vec<_>>());
        let decl = |name: &str, first, last| BundleDecl {
            name: name.into(),
            client: "R".into(),
            ty: PortType::Audio,
            dir: Direction::Out,
            first,
            last,
        };
        // [2-5] leaves out1 single and re-pairs from out7 on.
        assert_eq!(labels(&s, &[decl("drums", 2, 5)]), ["out1:1", "drums:4", "out6:1", "out7/8:2"]);

        let (b, problems) = Bundles::derive(&s, &[decl("a", 1, 4), decl("b", 3, 6), decl("c", 7, 9)]);
        assert_eq!(
            problems,
            [
                DeclProblem::Overlaps { name: "b".into(), with: "a".into() },
                DeclProblem::OutOfRange { name: "c".into(), have: 8 },
            ]
        );
        assert_eq!(b.iter().count(), 3);
        let out6 = s.port_named("R:out6").unwrap();
        let (id, pos) = b.of_port(out6);
        assert_eq!((b.get(id).label.as_str(), pos), ("out5/6", 1));

        let (_, problems) = Bundles::derive(&s, &[BundleDecl { client: "gone".into(), ..decl("g", 1, 2) }]);
        assert_eq!(problems, [DeclProblem::NoClient { name: "g".into() }]);
    }

    #[test]
    fn cells() {
        let s = crate::fixture::parse(
            "out A:out_L\nout A:out_R\nout M:mono\nin B:in_L\nin B:in_R\nin midi C:m\n\
             link A:out_L > B:in_R\nlink A:out_R > B:in_L\nlink M:mono > B:in_L\nlink M:mono > B:in_R\n",
        )
        .unwrap();
        let (b, _) = Bundles::derive(&s, &[]);
        let find = |label: &str| b.iter().find(|(_, x)| x.label == label).unwrap().1;
        assert_eq!(Cell::of(&s, find("out_L/R"), find("in_L/R")), Cell::Crossed);
        assert_eq!(Cell::of(&s, find("mono"), find("in_L/R")), Cell::Full);
        assert_eq!(Cell::of(&s, find("out_L/R"), find("m")), Cell::Types);
        let s2 =
            crate::fixture::parse("out A:out_L\nout A:out_R\nin B:in_L\nin B:in_R\nlink A:out_L > B:in_L\n")
                .unwrap();
        let (b2, _) = Bundles::derive(&s2, &[]);
        let (r, c) = (
            b2.get(b2.of_port(s2.port_named("A:out_L").unwrap()).0),
            b2.get(b2.of_port(s2.port_named("B:in_L").unwrap()).0),
        );
        assert_eq!(Cell::of(&s2, r, c), Cell::Partial);
        let s3 = crate::fixture::parse("out A:out_L\nout A:out_R\nin B:in_L\nin B:in_R\n").unwrap();
        let (b3, _) = Bundles::derive(&s3, &[]);
        assert_eq!(Cell::of(&s3, b3.get(BundleId(0)), b3.get(BundleId(1))), Cell::Empty);
    }

    #[test]
    fn studio_bundles() {
        let s = crate::fixture::parse(include_str!("../testdata/studio.fake")).unwrap();
        let got: Vec<String> = Bundles::derive(&s, &[])
            .0
            .iter()
            .map(|(_, b)| {
                format!(
                    "{} {} {}",
                    s.client(b.client).name().split('.').next().unwrap(),
                    b.dir.as_str(),
                    b.label
                )
            })
            .collect();
        assert_eq!(
            got,
            [
                "alsa_input out capture_FL/FR",
                "alsa_output out monitor_FL/FR",
                "alsa_output in playback_FL/FR",
                "REAPER out out1/2",
                "REAPER out out3/4",
                "REAPER out out5/6",
                "REAPER in in1/2",
                "REAPER in in3/4",
                "Carla out audio-out1/2",
                "Carla in audio-in1/2",
                "Firefox out output_FL/FR",
                "OBS in input_FL/FR",
            ]
        );
    }
}
