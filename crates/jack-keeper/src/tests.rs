//! The keeper's logic without JACK: connections reconciled, ports sorted, and the simulation
//! playing what it's sent. place's (`VENDORED.md`), with a player of the tests' own.

use std::sync::atomic::AtomicU32;
use std::time::Instant;

use super::*;

struct Fake(BTreeSet<String>, BTreeSet<(String, String)>);

impl Graph for Fake {
    fn exists(&self, p: &str) -> bool {
        self.0.contains(p)
    }
    fn connected(&self, a: &str, b: &str) -> bool {
        self.1.contains(&(a.into(), b.into()))
    }
    fn connect(&mut self, a: &str, b: &str) -> Result<(), String> {
        self.1.insert((a.into(), b.into()));
        Ok(())
    }
    fn disconnect(&mut self, a: &str, b: &str) {
        self.1.remove(&(a.into(), b.into()));
    }
}

#[test]
fn reconcile_makes_what_it_can_and_cuts_only_what_was_wanted() {
    let ports = ["brr:out_L", "brr:out_R", "system:playback_1", "mixer:live_01"];
    let mut g = Fake(ports.iter().map(|s| s.to_string()).collect(), Default::default());
    g.1.insert(("brr:out_L".into(), "mixer:live_01".into())); // someone else's: kept
    let want = links("brr", &["system:playback_1".into(), "gone:in".into()], &["mixer:live_01".into()]);
    let said = reconcile(&mut g, &want, &[]);
    assert_eq!(said, vec!["connected brr:out_L > system:playback_1", "connected brr:out_R > mixer:live_01"]);
    let cut = [("brr:out_R".to_string(), "mixer:live_01".to_string())];
    let said = reconcile(&mut g, &links("brr", &["system:playback_1".into()], &[]), &cut);
    assert_eq!(said, vec!["cut brr:out_R > mixer:live_01"]);
    assert!(g.connected("brr:out_L", "mixer:live_01"), "never someone else's");
}

#[test]
fn ports_sort_the_sound_card_first_and_numbers_as_numbers() {
    let p = |n: &str, physical| PortInfo { name: n.into(), physical };
    let mut v = vec![
        p("dexter:in_02", false),
        p("other:in", false),
        p("system:playback_10", true),
        p("mixer:live_01", false),
        p("system:playback_2", true),
    ];
    sort_ports(&mut v);
    let names: Vec<&str> = v.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(
        names,
        ["system:playback_2", "system:playback_10", "mixer:live_01", "dexter:in_02", "other:in"]
    );
}

/// A steady value while told to play one; its last sample left in `last`.
pub(crate) struct Steady {
    value: f32,
    last: Arc<AtomicU32>,
    done: Option<Box<[f32]>>,
}

impl Player for Steady {
    type In = (f32, Option<Box<[f32]>>);
    type Out = Box<[f32]>;

    fn take(&mut self, (v, b): Self::In) {
        self.value = v;
        self.done = b;
    }

    fn period(&mut self, l: &mut [f32], r: &mut [f32]) {
        l.fill(self.value);
        r.fill(-self.value);
        self.last.store(l[l.len() - 1].to_bits(), Ordering::Relaxed);
    }

    fn done(&mut self) -> Option<Box<[f32]>> {
        self.done.take()
    }
}

#[test]
fn the_simulation_plays_what_its_sent_and_hands_back_what_its_done_with() {
    let last = Arc::new(AtomicU32::new(0));
    let l2 = last.clone();
    let audio =
        Audio::sim("brr", BTreeSet::new(), move || Steady { value: 0.0, last: l2.clone(), done: None });
    let start = Instant::now();
    while !audio.attached() {
        assert!(start.elapsed() < Duration::from_secs(3));
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(audio.rate(), SIM_RATE);
    thread::sleep(Duration::from_millis(50));
    assert_eq!(f32::from_bits(last.load(Ordering::Relaxed)), 0.0, "nothing unasked");
    audio.send((0.5, Some(vec![0.0; 1000].into_boxed_slice())));
    let start = Instant::now();
    while f32::from_bits(last.load(Ordering::Relaxed)) != 0.5 {
        assert!(start.elapsed() < Duration::from_secs(3), "played");
        thread::sleep(Duration::from_millis(10));
    }
    let (_, ports) = audio.ports();
    assert_eq!(ports[0].name, "system:playback_1");
    audio.stop();
}
