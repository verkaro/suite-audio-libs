//! The fake backend: an in-memory graph loaded from the fixture format ([`crate::fixture`]),
//! for tests and for `bay --fake FILE`. Tests can change the graph behind bay's back
//! (register, unregister, rename, link) and make chosen ports fail.

use std::collections::{BTreeSet, HashSet};

use crate::backend::{Backend, BackendError, Report, execute};
use crate::compile::Plan;
use crate::fixture::{self, FixtureError};
use crate::model::{Direction, ModelError, PortType, Snapshot};

#[derive(Clone, Debug, Default)]
pub struct Fake {
    /// Registration order.
    ports: Vec<(String, PortType, Direction)>,
    links: BTreeSet<(String, String)>,
    fail: HashSet<String>,
    generation: u64,
}

impl Fake {
    pub fn from_fixture(text: &str) -> Result<Fake, FixtureError> {
        let snap = fixture::parse(text)?;
        let ports = snap.ports().map(|(_, p)| (p.name().to_owned(), p.ty(), p.dir())).collect();
        let name = |p| snap.port(p).name().to_owned();
        let links = snap.links().iter().map(|l| (name(l.from), name(l.to))).collect();
        Ok(Fake { ports, links, ..Fake::default() })
    }

    /// The graph in the fixture format, for writing back.
    pub fn to_fixture(&self) -> String {
        fixture::render(&self.build())
    }

    /// Makes every connect and disconnect touching `port` fail.
    pub fn fail_on(&mut self, port: &str) {
        self.fail.insert(port.to_owned());
    }

    pub fn register(&mut self, name: &str, ty: PortType, dir: Direction) -> Result<(), ModelError> {
        // Check it the way a snapshot would.
        let mut b = Snapshot::builder();
        for (n, t, d) in &self.ports {
            b.port(n, *t, *d)?;
        }
        b.port(name, ty, dir)?;
        self.ports.push((name.to_owned(), ty, dir));
        self.generation += 1;
        Ok(())
    }

    /// Removes a port and its links. False if there was none.
    pub fn unregister(&mut self, name: &str) -> bool {
        let before = self.ports.len();
        self.ports.retain(|(n, _, _)| n != name);
        self.links.retain(|(f, t)| f != name && t != name);
        self.generation += 1;
        self.ports.len() < before
    }

    /// Renames a port in place, keeping its links and position.
    pub fn rename(&mut self, old: &str, new: &str) -> bool {
        let Some(p) = self.ports.iter_mut().find(|(n, _, _)| n == old) else { return false };
        p.0 = new.to_owned();
        self.links = std::mem::take(&mut self.links)
            .into_iter()
            .map(|(f, t)| {
                (if f == old { new.to_owned() } else { f }, if t == old { new.to_owned() } else { t })
            })
            .collect();
        self.generation += 1;
        true
    }

    /// Changes a link from outside, as another patchbay would.
    pub fn set_link(&mut self, from: &str, to: &str, on: bool) {
        let l = (from.to_owned(), to.to_owned());
        if on {
            self.links.insert(l);
        } else {
            self.links.remove(&l);
        }
        self.generation += 1;
    }

    fn build(&self) -> Snapshot {
        let mut b = Snapshot::builder();
        for (n, t, d) in &self.ports {
            b.port(n, *t, *d).expect("register checked it");
        }
        for (f, t) in &self.links {
            b.link(f, t).expect("links are checked when made");
        }
        b.build()
    }
}

impl Backend for Fake {
    fn snapshot(&mut self) -> Result<Snapshot, BackendError> {
        Ok(self.build())
    }

    fn run(&mut self, snap: &Snapshot, plan: &Plan) -> Report {
        let current = self.build();
        let report = execute(snap, plan, |from, to, connect| {
            if self.fail.contains(from) || self.fail.contains(to) {
                return Err("the fake backend was told to fail".into());
            }
            let (Some(f), Some(t)) = (current.port_named(from), current.port_named(to)) else {
                return Err("no such port".into());
            };
            let (fp, tp) = (current.port(f), current.port(t));
            if connect && (fp.dir() != Direction::Out || tp.dir() != Direction::In || fp.ty() != tp.ty()) {
                return Err("these ports can't be linked".into());
            }
            let l = (from.to_owned(), to.to_owned());
            if connect {
                self.links.insert(l);
            } else {
                self.links.remove(&l);
            }
            Ok(())
        });
        self.generation += 1;
        report
    }

    fn generation(&self) -> u64 {
        self.generation
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::Options;
    use crate::session::Session;
    use crate::stage::Stage;
    use crate::views::{self, Names};

    const STUDIO: &str = include_str!("../testdata/studio.fake");

    fn text(snap: &Snapshot, plan: &Plan) -> String {
        views::statements(crate::render::plan(snap, plan), &Names::default())
    }

    #[test]
    fn runs_plans_and_writes_back() {
        let mut fake = Fake::from_fixture(STUDIO).unwrap();
        assert_eq!(fake.to_fixture(), fixture::render(&fixture::parse(STUDIO).unwrap()));
        let s = fake.snapshot().unwrap();
        let session = Session::new();
        let plan = session.compile(&s, "Carla !> REAPER; REAPER[5-6] > OBS", Options::default()).unwrap();
        let g = fake.generation();
        let report = fake.run(&s, &plan);
        assert_eq!(report.applied, plan);
        assert!(report.failures.is_empty());
        assert!(fake.generation() > g);
        let after = fake.snapshot().unwrap();
        assert!(
            session
                .compile(&after, "Carla !> REAPER; REAPER[5-6] > OBS", Options::default())
                .unwrap()
                .is_empty()
        );
        let reread = Fake::from_fixture(&fake.to_fixture()).unwrap().snapshot().unwrap();
        assert_eq!(fixture::render(&reread), fixture::render(&after));
    }

    #[test]
    fn partial_failure_and_undo() {
        let mut fake = Fake::from_fixture(STUDIO).unwrap();
        fake.fail_on("OBS:input_FR");
        let s = fake.snapshot().unwrap();
        let mut session = Session::new();
        let plan = session.compile(&s, "Carla !> REAPER; REAPER[5-6] > OBS", Options::default()).unwrap();
        let report = fake.run(&s, &plan);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(s.port(report.failures[0].link.to).name(), "OBS:input_FR");
        assert_eq!(text(&s, &report.applied), "Carla !> REAPER\nREAPER[5] > OBS[1]\n");

        // Undo reverses only what was applied.
        session.record(&s, &report.applied);
        let now = fake.snapshot().unwrap();
        let undo = session.undo(&now).unwrap();
        assert_eq!(text(&now, &undo.plan), "REAPER !> OBS\nCarla > REAPER[3-4]\n");
        fake.run(&now, &undo.plan);
        assert_eq!(fixture::render(&fake.snapshot().unwrap()), fixture::render(&s));
    }

    #[test]
    fn staged_edits_survive_re_registration() {
        let mut fake = Fake::from_fixture(STUDIO).unwrap();
        let session = Session::new();
        let mut stage = Stage::new();
        stage.add(&session, &fake.snapshot().unwrap(), "REAPER[5-6] > OBS").unwrap();

        // OBS goes away: the edit goes stale but stays staged.
        let g = fake.generation();
        assert!(fake.unregister("OBS:input_FL") && fake.unregister("OBS:input_FR"));
        assert!(fake.generation() > g);
        let (plan, stale) = stage.plan(&session, &fake.snapshot().unwrap());
        assert!(plan.is_empty() && stale.len() == 1);

        // It comes back under new port names; the statement applies again.
        fake.register("OBS:in_L", PortType::Audio, Direction::In).unwrap();
        fake.register("OBS:in_R", PortType::Audio, Direction::In).unwrap();
        let s = fake.snapshot().unwrap();
        let (plan, stale) = stage.take(&session, &s);
        assert!(stale.is_empty());
        let report = fake.run(&s, &plan);
        assert_eq!(text(&s, &report.applied), "REAPER[5-6] > OBS\n");
    }

    #[test]
    fn outside_changes() {
        let mut fake = Fake::from_fixture(STUDIO).unwrap();
        assert!(fake.rename("REAPER:out1", "REAPER:master_L"));
        fake.set_link("REAPER:out5", "OBS:input_FL", true);
        let s = fake.snapshot().unwrap();
        assert!(s.is_linked(
            s.port_named("REAPER:master_L").unwrap(),
            s.port_named("alsa_output.usb-Focusrite_Scarlett_2i2_USB-00.analog-stereo:playback_FL").unwrap()
        ));
        assert!(s.is_linked(s.port_named("REAPER:out5").unwrap(), s.port_named("OBS:input_FL").unwrap()));
        assert!(matches!(
            fake.register("REAPER:out2", PortType::Audio, Direction::Out),
            Err(ModelError::DuplicatePort(_))
        ));
    }
}
