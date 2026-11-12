//! The JACK backend on a dummy server of its own, never the user's: `as_jackd::Dummy`,
//! `asbaytest-<pid>`, stopped by its own pid with SIGTERM. It runs only with
//! `SUITE_JACK_TEST=1`: `SUITE_JACK_TEST=1 cargo test -p as-bay-jack`. With
//! `JACK_DEFAULT_SERVER` set to that name and no server starting, a client can only reach the
//! dummy one; if it doesn't come up, the test fails rather than reaching the user's.
//!
//! Dexter's, where it was `BAY_JACK_TEST=1` on a dummy of the test's own making.

use std::sync::Arc;
use std::time::{Duration, Instant};

use as_bay::{Backend, Options, Plan, PortType, Session, Snapshot};
use as_bay_jack::{GraphNotices, Jack, run_on, snapshot_of};
use as_jackd::Dummy;
use jack::{AudioIn, AudioOut, Client, ClientOptions, MidiOut};

/// Waits until the backend's generation passes `since`.
fn wait_past(jack: &Jack, since: u64) {
    let start = Instant::now();
    while jack.generation() <= since {
        assert!(start.elapsed() < Duration::from_secs(3), "no notification from JACK");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn plan(session: &Session, snap: &Snapshot, text: &str) -> Plan {
    session.compile(snap, text, Options::default()).unwrap()
}

#[test]
fn live_graph() {
    if std::env::var_os("SUITE_JACK_TEST").is_none() {
        eprintln!("skipped: set SUITE_JACK_TEST=1 to run it on a dummy JACK server of its own");
        return;
    }
    // SAFETY: the only test in this binary, before any thread of ours starts; libjack reads
    // JACK_DEFAULT_SERVER when a client opens.
    unsafe { std::env::set_var("JACK_DEFAULT_SERVER", Dummy::name("asbay")) };
    let _dummy = Dummy::start("asbay");
    let mut jack = Jack::open().expect("the dummy server");
    assert!(jack.name().starts_with("bay"), "{}", jack.name());
    let g = jack.generation();

    // The dummy's own ports: 16 captures and 2 playbacks, not the user's rig.
    let snap = jack.snapshot().unwrap();
    let system = snap.client_named("system").expect("the dummy's system client");
    assert_eq!(snap.client(system).ports().len(), 18);

    let (client, _) = Client::new("bay-test", ClientOptions::NO_START_SERVER).unwrap();
    let name = client.name().to_owned();
    let _o1 = client.register_port("out_1", AudioOut::default()).unwrap();
    let _o2 = client.register_port("out_2", AudioOut::default()).unwrap();
    let _i1 = client.register_port("in_1", AudioIn::default()).unwrap();
    let _i2 = client.register_port("in_2", AudioIn::default()).unwrap();
    let _m = client.register_port("midi_out", MidiOut::default()).unwrap();
    let active = client.activate_async((), ()).unwrap();
    wait_past(&jack, g);

    let snap = jack.snapshot().unwrap();
    let c = snap.client_named(&name).expect("the test client is in the snapshot");
    let ports: Vec<(String, u32, PortType)> = snap
        .client(c)
        .ports()
        .iter()
        .map(|&p| (snap.port(p).short_name().to_owned(), snap.port(p).ordinal(), snap.port(p).ty()))
        .collect();
    assert_eq!(
        ports,
        [
            ("out_1".to_owned(), 1, PortType::Audio),
            ("out_2".into(), 2, PortType::Audio),
            ("in_1".into(), 1, PortType::Audio),
            ("in_2".into(), 2, PortType::Audio),
            ("midi_out".into(), 1, PortType::Midi),
        ]
    );
    assert!(snap.client_named(jack.name()).is_none(), "bay's own client is left out");

    let session = Session::new();
    let connect = plan(&session, &snap, &format!("{name} > {name}; system[1-2] > {name}"));
    assert_eq!(connect.add.len(), 4);
    let g = jack.generation();
    let report = jack.run(&snap, &connect);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(report.applied, connect);
    wait_past(&jack, g);
    let linked = jack.snapshot().unwrap();
    assert!(plan(&session, &linked, &format!("{name} > {name}; system[1-2] > {name}")).is_empty());

    // Running it again: already linked counts as applied.
    assert_eq!(jack.run(&snap, &connect).applied, connect);

    let disconnect = plan(&session, &linked, &format!("{name} !> {name}; system !> {name}"));
    assert_eq!(disconnect.del.len(), 4);
    let report = jack.run(&linked, &disconnect);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    // And again: not linked counts as applied.
    let report = jack.run(&linked, &disconnect);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert!(jack.snapshot().unwrap().links().is_empty());

    drop(active);

    // A tool's own name, as jackbay's will be.
    let named = Jack::open_as("asbay-named").expect("the dummy server");
    assert!(named.name().starts_with("asbay-named"), "{}", named.name());
    drop(named);

    // A program that owns its client (as Dexter's engine does) uses the functions over it,
    // with its own handler bumping a GraphNotices, and sees its own ports.
    let notices = Arc::new(GraphNotices::default());
    let (owned, _) = Client::new("owner", ClientOptions::NO_START_SERVER).unwrap();
    let owner = owned.name().to_owned();
    let _oi1 = owned.register_port("in_01", AudioIn::default()).unwrap();
    let _oi2 = owned.register_port("in_02", AudioIn::default()).unwrap();
    let owned = owned.activate_async(Counting(notices.clone()), ()).unwrap();
    let client = owned.as_client();
    let snap = snapshot_of(client, None);
    assert!(snap.client_named(&owner).is_some(), "its own ports show");
    let g = notices.generation();
    let patch = plan(&session, &snap, &format!("system[1-2] > {owner}"));
    let report = run_on(client, &snap, &patch);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let start = Instant::now();
    while notices.generation() <= g {
        assert!(start.elapsed() < Duration::from_secs(3), "the owner's handler saw no notice");
        std::thread::sleep(Duration::from_millis(10));
    }
    let after = snapshot_of(client, None);
    assert!(plan(&session, &after, &format!("system[1-2] > {owner}")).is_empty(), "linked");
    drop(owned);
}

/// The owner's own notification handler, bumping its GraphNotices.
struct Counting(Arc<GraphNotices>);

impl jack::NotificationHandler for Counting {
    fn ports_connected(&mut self, _: &Client, _: jack::PortId, _: jack::PortId, _: bool) {
        self.0.bump();
    }
}
