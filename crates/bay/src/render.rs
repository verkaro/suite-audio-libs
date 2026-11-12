//! Printing link sets as statements (`SYNTAX.md`, "Printed form"). Links compress to few
//! range statements: zip runs, then fan-outs, then fan-ins, then singles.
//!
//! The main invariant: what this prints compiles back to the same links against the same
//! snapshot. For now refs are raw client names. Session names (aliases, generated
//! aliases) come with the session in step 3.

use std::collections::{BTreeMap, BTreeSet};

use crate::compile::Plan;
use crate::model::{ClientId, Direction, Link, PortType, Snapshot};
use crate::syntax::{Ref, Selector, Statement};

/// Connect statements that make `links`.
pub fn connects(snap: &Snapshot, links: &BTreeSet<Link>) -> Vec<Statement> {
    render(snap, links, None)
}

/// Disconnect statements that remove `links`, which are links of `snap`. A `!>` removes
/// every link between its two sides, so a range statement is used only where it catches
/// no link of `snap` outside `links`.
pub fn disconnects(snap: &Snapshot, links: &BTreeSet<Link>) -> Vec<Statement> {
    render(snap, links, Some(links))
}

/// A plan as statements: its deletions, then its additions.
pub fn plan(snap: &Snapshot, plan: &Plan) -> Vec<Statement> {
    let mut out = disconnects(snap, &plan.del);
    out.extend(connects(snap, &plan.add));
    out
}

/// A plan link by link with full port names (deletions, then additions), which survives
/// ordinals shifting. The line CLI's undo file uses it.
pub fn port_statements(snap: &Snapshot, plan: &Plan) -> Vec<Statement> {
    plan.links_named(snap)
        .into_iter()
        .map(|l| {
            let (from, to) = (Ref::new(l.from), Ref::new(l.to));
            if l.add { Statement::Connect { from, to } } else { Statement::Disconnect { from, to } }
        })
        .collect()
}

/// One output client, one input client, one type.
type Group = (ClientId, ClientId, PortType);

fn render(snap: &Snapshot, links: &BTreeSet<Link>, del: Option<&BTreeSet<Link>>) -> Vec<Statement> {
    // 0-based ordinal pairs per group, in client order.
    let mut groups: BTreeMap<Group, BTreeSet<(u32, u32)>> = BTreeMap::new();
    for l in links {
        let (f, t) = (snap.port(l.from), snap.port(l.to));
        groups
            .entry((f.client(), t.client(), f.ty()))
            .or_default()
            .insert((f.ordinal() - 1, t.ordinal() - 1));
    }
    let mut out = Vec::new();
    for ((a, b, ty), mut left) in groups {
        let g = Grp { snap, a, b, ty, del };
        if del.is_some() && g.whole(&left) {
            out.push(g.stmt((0, g.count(Direction::Out) - 1), (0, g.count(Direction::In) - 1)));
            continue;
        }
        out.extend(g.zips(&mut left));
        out.extend(g.fans(&mut left, false));
        out.extend(g.fans(&mut left, true));
        out.extend(left.iter().map(|&(i, j)| g.stmt((i, i), (j, j))));
    }
    out
}

struct Grp<'a> {
    snap: &'a Snapshot,
    a: ClientId,
    b: ClientId,
    ty: PortType,
    del: Option<&'a BTreeSet<Link>>,
}

impl Grp<'_> {
    fn count(&self, dir: Direction) -> u32 {
        let c = if dir == Direction::Out { self.a } else { self.b };
        self.snap.ports_of(c, self.ty, dir).len() as u32
    }

    fn link(&self, i: u32, j: u32) -> Link {
        let from = self.snap.ports_of(self.a, self.ty, Direction::Out)[i as usize];
        let to = self.snap.ports_of(self.b, self.ty, Direction::In)[j as usize];
        Link { from, to }
    }

    /// For a disconnect: does `A[is] !> B[js]` catch only links being deleted?
    fn safe(&self, is: (u32, u32), js: (u32, u32)) -> bool {
        let Some(del) = self.del else { return true };
        (is.0..=is.1).all(|i| {
            (js.0..=js.1).all(|j| {
                let l = self.link(i, j);
                !self.snap.is_linked(l.from, l.to) || del.contains(&l)
            })
        })
    }

    /// Every link of the snapshot in this group is being deleted.
    fn whole(&self, left: &BTreeSet<(u32, u32)>) -> bool {
        let outs = self.snap.ports_of(self.a, self.ty, Direction::Out);
        let ins = self.snap.ports_of(self.b, self.ty, Direction::In);
        let existing = outs.iter().flat_map(|&f| self.snap.links_from(f).filter(|t| ins.contains(t))).count();
        existing == left.len()
    }

    /// Runs of (i, j), (i+1, j+1), … of at least two.
    fn zips(&self, left: &mut BTreeSet<(u32, u32)>) -> Vec<Statement> {
        let mut out = Vec::new();
        let starts: Vec<(u32, u32)> = left.iter().copied().collect();
        for (i, j) in starts {
            let mut k = (0..).take_while(|&k| left.contains(&(i + k, j + k))).count() as u32;
            while k >= 2 && !self.safe((i, i + k - 1), (j, j + k - 1)) {
                k -= 1;
            }
            if k >= 2 {
                for n in 0..k {
                    left.remove(&(i + n, j + n));
                }
                out.push(self.stmt((i, i + k - 1), (j, j + k - 1)));
            }
        }
        out
    }

    /// One output to a run of inputs (or, with `sum`, a run of outputs into one input).
    fn fans(&self, left: &mut BTreeSet<(u32, u32)>, sum: bool) -> Vec<Statement> {
        let key = |&(i, j): &(u32, u32)| if sum { (j, i) } else { (i, j) };
        let pairs: BTreeSet<(u32, u32)> = left.iter().map(key).collect();
        let mut out = Vec::new();
        let mut it = pairs.iter().copied().peekable();
        while let Some((fixed, start)) = it.next() {
            let mut end = start;
            while it.next_if(|&(f, n)| f == fixed && n == end + 1).is_some() {
                end += 1;
            }
            if end > start {
                for n in start..=end {
                    left.remove(&key(&(fixed, n)));
                }
                let run = (start, end);
                out.push(if sum { self.stmt(run, (fixed, fixed)) } else { self.stmt((fixed, fixed), run) });
            }
        }
        out
    }

    fn stmt(&self, is: (u32, u32), js: (u32, u32)) -> Statement {
        let from = self.side(self.a, Direction::Out, is);
        let to = self.side(self.b, Direction::In, js);
        if self.del.is_some() { Statement::Disconnect { from, to } } else { Statement::Connect { from, to } }
    }

    /// A client ref for 0-based ordinals `lo..=hi`: no range when it's all of them, and
    /// `midi` always written for MIDI.
    fn side(&self, c: ClientId, dir: Direction, (lo, hi): (u32, u32)) -> Ref {
        let all = lo == 0 && hi + 1 == self.count(dir);
        let sel = Selector {
            dir: None,
            ty: (self.ty == PortType::Midi).then_some(PortType::Midi),
            range: (!all).then_some((lo + 1, hi + 1)),
        };
        Ref { name: self.snap.client(c).name().to_owned(), sel }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::{Options, compile};
    use crate::fixture;
    use crate::resolve::Scope;
    use crate::syntax::parse;

    const EXTRA: &str = "out midi a2j:cap1\nout midi a2j:cap2\nin midi REAPER:MIDI in 1\nin midi REAPER:MIDI in 2\n\
                         out Mixer:o1\nout Mixer:o2\nout Mixer:o3\nin Sink:i1\nin Sink:i2\nin Sink:i3\n";

    fn studio() -> Snapshot {
        fixture::parse(&(include_str!("../testdata/studio.fake").to_owned() + EXTRA)).unwrap()
    }

    fn text(stmts: &[Statement]) -> String {
        stmts.iter().map(|s| format!("{s}\n")).collect()
    }

    /// Same ports as `snap`, and exactly `links`.
    fn with_links(snap: &Snapshot, links: &BTreeSet<Link>) -> Snapshot {
        let mut b = Snapshot::builder();
        for (_, p) in snap.ports() {
            b.port(p.name(), p.ty(), p.dir()).unwrap();
        }
        for l in links {
            b.link(snap.port(l.from).name(), snap.port(l.to).name()).unwrap();
        }
        b.build()
    }

    fn compiled(snap: &Snapshot, text: &str, opts: Options) -> Plan {
        compile(&parse(text).unwrap(), snap, &Scope::default(), opts).unwrap()
    }

    #[test]
    fn studio_prints_compressed() {
        let s = studio();
        let fx = "alsa_input.usb-Focusrite_Scarlett_2i2_USB-00.analog-stereo";
        let out = "alsa_output.usb-Focusrite_Scarlett_2i2_USB-00.analog-stereo";
        assert_eq!(
            text(&connects(&s, s.links())),
            format!(
                "{fx}[1] > REAPER[1-2]\nREAPER[1-2] > {out}\nREAPER[3-4] > Carla\nCarla > REAPER[3-4]\nFirefox[1] > OBS[1]\n"
            )
        );
    }

    #[test]
    fn shapes() {
        let s = studio();
        let links = |t: &str| compiled(&s, t, Options::default()).add;
        let show = |t: &str| text(&connects(&s, &links(t)));
        assert_eq!(show("Mixer > Sink"), "Mixer > Sink\n");
        assert_eq!(show("Mixer[1] > Sink; Mixer[3] > Sink[3]"), "Mixer[1] > Sink\nMixer[3] > Sink[3]\n");
        assert_eq!(show("Mixer > Sink[2]"), "Mixer > Sink[2]\n");
        assert_eq!(
            show("Mixer[2] > Sink[1]; Mixer[1] > Sink[3]"),
            "Mixer[1] > Sink[3]\nMixer[2] > Sink[1]\n"
        );
        assert_eq!(show("a2j > REAPER"), "a2j[midi] > REAPER[midi]\n");
    }

    #[test]
    fn port_statements_read_back() {
        let s = studio();
        let want = compiled(&s, "Carla !> REAPER; Mixer[1] > Sink; a2j > REAPER", Options::default());
        let printed = text(&port_statements(&s, &want));
        assert!(printed.starts_with("Carla:audio-out1 !> REAPER:in3\n"), "{printed}");
        assert!(printed.contains("\"REAPER:MIDI in 1\""), "{printed}");
        assert_eq!(compiled(&s, &printed, Options::default()), want);
    }

    #[test]
    fn disconnects_avoid_crossed_links() {
        // Mixer 1>1, 2>2, 1>2 exist; deleting 1>1 and 2>2 must not print Mixer[1-2] !> Sink[1-2].
        let s = studio();
        let s = with_links(
            &s,
            &compiled(&s, "Mixer[1-2] > Sink[1-2]; Mixer[1] > Sink[2]", Options::default()).add,
        );
        let del = compiled(&s, "Mixer[1] !> Sink[1]; Mixer[2] !> Sink[2]", Options::default()).del;
        let printed = text(&disconnects(&s, &del));
        assert_eq!(printed, "Mixer[1] !> Sink[1]\nMixer[2] !> Sink[2]\n");
        // Everything between two clients goes as one statement.
        let all = compiled(&s, "Mixer !> Sink", Options::default()).del;
        assert_eq!(text(&disconnects(&s, &all)), "Mixer !> Sink\n");
    }

    /// A small xorshift, so the property test needs no dependencies.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn chance(&mut self, one_in: u64) -> bool {
            self.next().is_multiple_of(one_in)
        }
    }

    /// Every same-typed out→in link the snapshot's ports allow.
    fn possible(s: &Snapshot) -> Vec<Link> {
        let outs: Vec<_> = s.ports().filter(|(_, p)| p.dir() == Direction::Out).collect();
        let ins: Vec<_> = s.ports().filter(|(_, p)| p.dir() == Direction::In).collect();
        let mut v = Vec::new();
        for (f, fp) in &outs {
            for (t, tp) in &ins {
                if fp.ty() == tp.ty() {
                    v.push(Link { from: *f, to: *t });
                }
            }
        }
        v
    }

    #[test]
    fn round_trips() {
        let base = studio();
        let possible = possible(&base);
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        for round in 0..300 {
            let density = 2 + round % 8;
            let links: BTreeSet<Link> = possible.iter().copied().filter(|_| rng.chance(density)).collect();
            let s = with_links(&base, &links);

            // ls: the links print, and read back as no change (also under --exact) ...
            let ls = text(&connects(&s, s.links()));
            assert!(compiled(&s, &ls, Options::default()).is_empty(), "{ls}");
            assert!(compiled(&s, &ls, Options { exact_all: true }).is_empty(), "{ls}");
            // ... and rebuild them from nothing.
            let empty = with_links(&base, &BTreeSet::new());
            assert_eq!(&compiled(&empty, &ls, Options::default()).add, s.links(), "{ls}");

            // A random plan prints and compiles back to itself.
            let want = Plan {
                del: s.links().iter().copied().filter(|_| rng.chance(3)).collect(),
                add: possible
                    .iter()
                    .copied()
                    .filter(|l| !s.links().contains(l) && rng.chance(density))
                    .collect(),
            };
            let printed = text(&plan(&s, &want));
            assert_eq!(compiled(&s, &printed, Options::default()), want, "{printed}");
        }
    }
}
