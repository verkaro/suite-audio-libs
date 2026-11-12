//! The keeper's logic without JACK: ports sorted, and the simulation playing what it's sent.
//! place's (`VENDORED.md`), with a player of the tests' own.

use std::sync::atomic::AtomicU32;
use std::time::Instant;

use super::*;

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
        Audio::sim("brr", BTreeSet::new(), move |_| Steady { value: 0.0, last: l2.clone(), done: None });
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
