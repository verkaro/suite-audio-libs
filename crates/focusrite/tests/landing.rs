//! Do a fader's moves land on the interface's mixer? Measured, not read back: on the 18i20
//! (kernel 6.12, 2026-10-13), while a Focusrite panel was open, the gains read after writes came
//! from the driver's copy of the mixer scrambled (89 of 300 cells in `reading_back`; none with
//! the panel closed), and a write sends a whole mix from that copy. Only with `SUITE_FOCUSRITE_TEST=1` and `SUITE_FOCUSRITE_JACK=1`, on an
//! 18i20 3rd gen with JACK running on it: a tone from PCM 20 into mixer inputs, two mixes hearing
//! only those, sent to capture 19 and 20, and their levels measured. Every control is put back as
//! it was found. `cargo test -p as-focusrite --test landing -- --nocapture --test-threads 1`;
//! for `fader_moves_land`, LANDING_MIXES=AB measures mixes A and B (else K and L),
//! LANDING_READER=1 has another program read a gain after every step, and LANDING_CHECK=1 does
//! what Dexter's Link did until 2026-10-13 (read back, and write again what read otherwise).

use std::collections::HashMap;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use as_focusrite::{Card, Controls, Kind, keep_found, put_back};

fn wanted() -> bool {
    let on = |k: &str| std::env::var(k).is_ok_and(|v| v == "1");
    on("SUITE_FOCUSRITE_TEST") && on("SUITE_FOCUSRITE_JACK")
}

fn focusrite() -> Option<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir("/dev/snd/by-id")
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("usb-Focusrite")))
        .collect();
    v.sort();
    v.into_iter().next()
}

/// The tone's peak: −30 dBFS.
const AMP: f32 = 0.031_622_78;

/// The card's controls by name.
struct Rig {
    card: Card,
    ids: HashMap<String, (u32, Kind)>,
}

impl Rig {
    fn id(&self, name: &str) -> u32 {
        self.ids.get(name).unwrap_or_else(|| panic!("no control {name}")).0
    }
    fn set(&mut self, name: &str, v: i64) {
        let id = self.id(name);
        self.card.write(id, &[v]).unwrap();
    }
    fn set_item(&mut self, name: &str, item: &str) {
        let (id, kind) = self.ids.get(name).unwrap_or_else(|| panic!("no control {name}")).clone();
        let Kind::Enum(items) = kind else { panic!("{name} isn't an enum") };
        let i = items.iter().position(|x| x == item).unwrap_or_else(|| panic!("{name} has no {item}"));
        self.card.write(id, &[i as i64]).unwrap();
    }
    fn gain(&mut self, mix: char, input: usize, v: i64) {
        self.set(&format!("Mix {mix} Input {input:02} Playback Volume"), v);
    }
    /// A whole mix, cells 1 to 25, sent to the interface: every cell written, then the first
    /// moved away and back, so the driver sends the row from a copy that holds exactly these.
    fn commit(&mut self, mix: char, cells: &[i64; 25]) {
        for (i, &v) in cells.iter().enumerate() {
            self.gain(mix, i + 1, v);
        }
        self.gain(mix, 1, if cells[0] == 0 { 1 } else { 0 });
        self.gain(mix, 1, cells[0]);
    }
}

/// The bench's JACK callback.
type Process = Box<dyn FnMut(&jack::Client, &jack::ProcessScope) -> jack::Control + Send>;

/// Sums of squares from the JACK thread: (mix x, mix y, frames).
type Sums = rtrb::Consumer<(f64, f64, usize)>;

/// The two mixes' gains, dB, over the next ms milliseconds (anything before dropped).
fn measure(rx: &mut Sums, ms: u64) -> (f64, f64) {
    while rx.pop().is_ok() {}
    let (mut k, mut l, mut n) = (0.0, 0.0, 0usize);
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        while let Ok((a, b, f)) = rx.pop() {
            k += a;
            l += b;
            n += f;
        }
        thread::sleep(Duration::from_millis(2));
    }
    let tone = (AMP as f64) / 2f64.sqrt();
    let db = |s: f64| if s <= 0.0 { -120.0 } else { 20.0 * ((s / n as f64).sqrt() / tone).log10() };
    (db(k), db(l))
}

/// A mixer value's dB: 0 off, then −80 dB in half-dB steps.
fn value_db(v: i64) -> f64 {
    if v == 0 { -120.0 } else { v as f64 / 2.0 - 80.0 }
}

/// A fader's travel and a pan's, as the gains of a pair: (left, right) mixer values.
fn pair(db: f64, pan: f64) -> (i64, i64) {
    let a = (pan + 1.0) * std::f64::consts::FRAC_PI_4;
    let v = |g: f64| {
        let d = db + 20.0 * g.max(1e-6).log10();
        ((d + 80.0) * 2.0).round().clamp(0.0, 184.0) as i64
    };
    (v(a.cos() * 2f64.sqrt()), v(a.sin() * 2f64.sqrt()))
}

/// dB as a linear gain, and back: what two cells in phase add up to.
fn sum_db(a: f64, b: f64) -> f64 {
    let lin = |d: f64| if d <= -119.0 { 0.0 } else { 10f64.powf(d / 20.0) };
    let s = lin(a) + lin(b);
    if s == 0.0 { -120.0 } else { 20.0 * s.log10() }
}

/// How long the 18i20 takes to reach a new gain: it glides, it doesn't step.
const SETTLE: Duration = Duration::from_millis(1500);

/// The interface kept as found, f run on it, and put back.
fn on_the_interface(f: impl FnOnce(&std::path::Path) + std::panic::UnwindSafe) {
    if !wanted() {
        return;
    }
    let path = focusrite().expect("no Focusrite in /dev/snd/by-id");
    let mut card = Card::open(&path).unwrap();
    if card.name().unwrap().0 != "Scarlett 18i20 USB" {
        eprintln!("not an 18i20: nothing written");
        return;
    }
    let scratch = std::env::temp_dir().join(format!("as-focusrite-landing-{}", std::process::id()));
    let kept_at = scratch.join("found.txt");
    let _ = std::fs::remove_dir_all(&scratch);
    keep_found(&mut card, &kept_at).unwrap();
    let kept = std::fs::read_to_string(&kept_at).unwrap();
    let p = path.clone();
    let result = std::panic::catch_unwind(move || f(&p));
    let mut card = Card::open(&path).unwrap();
    let n = put_back(&mut card, &kept).unwrap();
    eprintln!("put back: {n} controls");
    let _ = std::fs::remove_dir_all(&scratch);
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

/// The bench: PCM 20 into each of the mixer inputs, which no other mix hears; mixes x and y
/// hearing nothing yet, sent to capture 19 and 20 and to no output; a tone out and the two in.
struct Bench {
    rig: Rig,
    rx: Sums,
    x: char,
    y: char,
    _client: jack::AsyncClient<(), jack::contrib::ClosureProcessHandler<(), Process>>,
}

impl Bench {
    fn new(path: &std::path::Path, x: char, y: char, inputs: &[usize]) -> Bench {
        let mut card = Card::open(path).unwrap();
        let ids =
            card.list().unwrap().into_iter().map(|c| (c.name.clone(), (c.numid, c.kind.clone()))).collect();
        let mut rig = Rig { card, ids };
        let outs: Vec<String> = rig
            .ids
            .keys()
            .filter(|n| n.contains(" Output ") && n.ends_with("Playback Enum"))
            .cloned()
            .collect();
        let (mx, my) = (format!("Mix {x}"), format!("Mix {y}"));
        for name in outs {
            let (id, kind) = rig.ids[&name].clone();
            let Kind::Enum(items) = kind else { continue };
            let now = rig.card.read(id).unwrap()[0] as usize;
            if items.get(now).is_some_and(|i| *i == mx || *i == my) {
                rig.set_item(&name, "Off");
            }
        }
        for &i in inputs {
            rig.set_item(&format!("Mixer Input {i:02} Capture Enum"), "PCM 20");
        }
        for m in "ABCDEFGHIJKL".chars() {
            for &i in inputs {
                rig.gain(m, i, 0);
            }
        }
        for m in [x, y] {
            rig.commit(m, &[0; 25]);
        }
        rig.set_item("PCM 19 Capture Enum", &mx);
        rig.set_item("PCM 20 Capture Enum", &my);

        let (client, _) =
            jack::Client::new("dxlanding", jack::ClientOptions::NO_START_SERVER).expect("JACK isn't running");
        let mut tone = client.register_port("tone", jack::AudioOut::default()).unwrap();
        let x_in = client.register_port("x", jack::AudioIn::default()).unwrap();
        let y_in = client.register_port("y", jack::AudioIn::default()).unwrap();
        let (mut tx, rx) = rtrb::RingBuffer::<(f64, f64, usize)>::new(8192);
        let step = 2.0 * std::f32::consts::PI * 1000.0 / client.sample_rate() as f32;
        let mut phase = 0.0f32;
        let cb: Process = Box::new(move |_: &jack::Client, ps: &jack::ProcessScope| {
            for s in tone.as_mut_slice(ps) {
                *s = AMP * phase.sin();
                phase = (phase + step) % (2.0 * std::f32::consts::PI);
            }
            let sq = |b: &[f32]| b.iter().map(|&v| (v as f64) * (v as f64)).sum::<f64>();
            let (a, b) = (x_in.as_slice(ps), y_in.as_slice(ps));
            let _ = tx.push((sq(a), sq(b), a.len()));
            jack::Control::Continue
        });
        let active = client.activate_async((), jack::contrib::ClosureProcessHandler::new(cb)).unwrap();
        let c = active.as_client();
        c.connect_ports_by_name("dxlanding:tone", "system:playback_20").unwrap();
        c.connect_ports_by_name("system:capture_19", "dxlanding:x").unwrap();
        c.connect_ports_by_name("system:capture_20", "dxlanding:y").unwrap();
        thread::sleep(Duration::from_millis(300));
        Bench { rig, rx, x, y, _client: active }
    }

    fn measure(&mut self, ms: u64) -> (f64, f64) {
        measure(&mut self.rx, ms)
    }
}

/// How the 18i20 moves to a new gain: measured every 100 ms after a step.
#[test]
fn settling() {
    on_the_interface(|path| {
        let mut b = Bench::new(path, 'K', 'L', &[24]);
        for (from, to) in [(0, 160), (160, 120), (120, 160)] {
            b.rig.gain('K', 24, from);
            thread::sleep(SETTLE * 2);
            b.rig.gain('K', 24, to);
            let t0 = Instant::now();
            let mut line = format!("{:.0} → {:.0} dB:", value_db(from), value_db(to));
            for _ in 0..25 {
                let (k, _) = b.measure(100);
                line += &format!(" {:.0}ms {k:.1}", t0.elapsed().as_millis());
            }
            eprintln!("{line}");
        }
    });
}

/// Two mixer inputs carrying the same tone add up in a mix, and each alone is as written.
#[test]
fn two_inputs_add() {
    on_the_interface(|path| {
        let mut b = Bench::new(path, 'K', 'L', &[24, 1]);
        let mut cells = [0i64; 25];
        for (c24, c1) in [(160, 0), (0, 140), (160, 140), (120, 120)] {
            cells[23] = c24;
            cells[0] = c1;
            b.rig.commit('K', &cells);
            thread::sleep(SETTLE);
            let (k, l) = b.measure(300);
            let want = sum_db(value_db(c24), value_db(c1));
            eprintln!(
                "K input 24 {:.1}, input 1 {:.1}: {k:.1} dB (want {want:.1}); L {l:.1}",
                value_db(c24),
                value_db(c1)
            );
            assert!((k - want).abs() < 0.5, "K {k:.1}, want {want:.1}");
        }
    });
}

/// A fader and a pan moving together on input 24 of mixes K and L (left and right), written as
/// Dexter writes them (the cells that change, left then right, every 33 ms), beside a steady
/// input 1 at −20 dB in both, as a tape return; then still, and both mixes measured against
/// where it ended. With LANDING_READER=1, another program reads a gain after every step, as
/// alsa-scarlett-gui does on every change.
#[test]
fn fader_moves_land() {
    on_the_interface(|path| {
        let reader = std::env::var("LANDING_READER").is_ok_and(|v| v == "1");
        // What Dexter's Link did until 2026-10-13 once the moves stopped for 40 ms: read every
        // gain it had written, and write again those that read otherwise, up to three times.
        let checking = std::env::var("LANDING_CHECK").is_ok_and(|v| v == "1");
        let m: Vec<char> = std::env::var("LANDING_MIXES").unwrap_or("KL".into()).chars().collect();
        let mut b = Bench::new(path, m[0], m[1], &[24, 1]);
        let (x, y) = (b.x, b.y);
        let mut cells = [0i64; 25];
        cells[0] = 120;
        b.rig.commit(x, &cells);
        b.rig.commit(y, &cells);
        let read_one = b.rig.id(&format!("Mix {x} Input 02 Playback Volume"));
        let trials = 9;
        let mut wrong = 0;
        for t in 0..trials {
            let end_db = -30.0 + 3.0 * t as f64;
            let end_pan = -0.8 + 0.2 * t as f64;
            let steps = 45;
            for s in 0..=steps {
                let f = s as f64 / steps as f64;
                let db = -40.0 + (end_db + 40.0) * f + 4.0 * (f * 17.0).sin();
                let pan = end_pan * f + 0.3 * (f * 11.0).sin() * (1.0 - f);
                let (vx, vy) = if s == steps { pair(end_db, end_pan) } else { pair(db, pan) };
                b.rig.gain(x, 24, vx);
                b.rig.gain(y, 24, vy);
                if reader {
                    let _ = b.rig.card.read(read_one);
                }
                thread::sleep(Duration::from_millis(33));
            }
            let (vx, vy) = pair(end_db, end_pan);
            if checking {
                for _ in 0..3 {
                    thread::sleep(Duration::from_millis(40));
                    let mut again = 0;
                    for (m, v) in [(x, vx), (y, vy)] {
                        let mut want = cells;
                        want[23] = v;
                        for (i, &w) in want.iter().enumerate() {
                            let id = b.rig.id(&format!("Mix {m} Input {:02} Playback Volume", i + 1));
                            if b.rig.card.read(id).unwrap()[0] != w {
                                b.rig.card.write(id, &[w]).unwrap();
                                again += 1;
                            }
                        }
                    }
                    if again == 0 {
                        break;
                    }
                }
            }
            thread::sleep(SETTLE);
            let (mx, my) = b.measure(300);
            let (ex, ey) = (sum_db(value_db(vx), -20.0), sum_db(value_db(vy), -20.0));
            let ok = (mx - ex).abs().max((my - ey).abs()) <= 0.5;
            wrong += !ok as usize;
            eprintln!(
                "  trial {t}: {x} {mx:.1} (want {ex:.1}), {y} {my:.1} (want {ey:.1}){}",
                if ok { "" } else { "  WRONG" }
            );
        }
        eprintln!("another reader {reader}, the old check {checking}: {wrong} of {trials} ended wrong");
        assert_eq!(wrong, 0);
    });
}

/// Every gain of every mix written, then read back: how many read otherwise.
#[test]
fn reading_back() {
    on_the_interface(|path| {
        let mut card = Card::open(path).unwrap();
        let list = card.list().unwrap();
        let id = |m: char, i: usize| {
            let n = format!("Mix {m} Input {i:02} Playback Volume");
            list.iter().find(|c| c.name == n).unwrap().numid
        };
        let want = |mi: usize, i: usize| (1 + (mi * 25 + i) % 180) as i64;
        let mixes: Vec<char> = "ABCDEFGHIJKL".chars().collect();
        for round in 0..3 {
            for (mi, &m) in mixes.iter().enumerate() {
                for i in 1..=25 {
                    card.write(id(m, i), &[want(mi, i) + round]).unwrap();
                }
            }
            thread::sleep(Duration::from_millis(200));
            let mut bad = Vec::new();
            for (mi, &m) in mixes.iter().enumerate() {
                for i in 1..=25 {
                    let got = card.read(id(m, i)).unwrap()[0];
                    if got != want(mi, i) + round {
                        bad.push(format!("{m}{i}"));
                    }
                }
            }
            eprintln!("round {round}: {} of 300 read otherwise {:?}", bad.len(), &bad[..bad.len().min(12)]);
        }
    });
}
