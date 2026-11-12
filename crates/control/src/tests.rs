use super::*;

// Controls written as the specs' tables have them (`specs/eq.md`, `opto.md`, `fet.md`), so the
// rules are tried on every kind the units will declare. The units' own lists are checked in
// their own crates.

#[allow(clippy::too_many_arguments)] // the table's columns, in order
const fn value(
    id: &'static str,
    lo: f64,
    hi: f64,
    scale: Scale,
    fine: f64,
    coarse: f64,
    unit: Unit,
    default: f64,
) -> Control {
    Control {
        id,
        short: "X",
        long: "A control",
        kind: Kind::Value { lo, hi, scale, fine, coarse, unit },
        default,
        detent: default,
    }
}

const FREQ: Control = value("lmf.freq", 200.0, 2500.0, Scale::Log, 1.0 / 12.0, 1.0 / 3.0, Unit::Hz, 400.0);
const HPF: Control = value("hpf.freq", 20.0, 400.0, Scale::Log, 1.0 / 12.0, 1.0 / 3.0, Unit::Hz, 80.0);
const HF: Control = value("hf.freq", 1500.0, 16000.0, Scale::Log, 1.0 / 12.0, 1.0 / 3.0, Unit::Hz, 10000.0);
const GAIN: Control = value("lmf.gain", -15.0, 15.0, Scale::Linear, 0.5, 3.0, Unit::Db, 0.0);
const Q: Control = value("lmf.q", 0.4, 4.0, Scale::Log, 1.0 / 6.0, 0.5, Unit::Q, 1.0);
const THR: Control = value("thr", -40.0, 0.0, Scale::Linear, 1.0, 3.0, Unit::Db, -20.0);
const INPUT: Control = value("input", 0.0, 40.0, Scale::Linear, 0.5, 3.0, Unit::Db, 0.0);
const RATIO: Control = value("knee.ratio", 1.0, 20.0, Scale::Log, 0.25, 1.0, Unit::Ratio, 4.0);

// The FET's attack dial (`fet.md`): 800 µs at 1 to 20 µs at 7, log between.
fn attack(d: f64) -> f64 {
    800e-6 * libm::pow(20.0 / 800.0, (d - 1.0) / 6.0)
}
fn attack_dial(t: f64) -> f64 {
    1.0 + 6.0 * libm::log(t / 800e-6) / libm::log(20.0 / 800.0)
}
const ATTACK: Control = value(
    "attack",
    1.0,
    7.0,
    Scale::Linear,
    0.1,
    1.0,
    Unit::Dial(Law { shows: Shows::Seconds, value: attack, dial: attack_dial }),
    3.0,
);

// The optical's Reduce (`opto.md`): `T = 10 − 50 · (reduce/100)^0.6` dBFS.
fn reduce(r: f64) -> f64 {
    10.0 - 50.0 * libm::pow(r / 100.0, 0.6)
}
fn reduce_dial(t: f64) -> f64 {
    100.0 * libm::pow(((10.0 - t) / 50.0).max(0.0), 1.0 / 0.6)
}
const REDUCE: Control = Control {
    detent: 0.0,
    ..value(
        "reduce",
        0.0,
        100.0,
        Scale::Linear,
        1.0,
        5.0,
        Unit::Dial(Law { shows: Shows::Dbfs, value: reduce, dial: reduce_dial }),
        40.0,
    )
};

const fn switch(id: &'static str, positions: &'static [&'static str], ratios: bool, default: f64) -> Control {
    Control {
        id,
        short: "S",
        long: "A switch",
        kind: Kind::Switch { positions, ratios },
        default,
        detent: default,
    }
}

const IN: Control = switch("in", &["out", "in"], false, 1.0);
const BELL: Control = switch("lf.bell", &["shelf", "bell"], false, 0.0);
const LISTEN: Control = switch("listen", &["off", "band", "taken"], false, 0.0);
const OPTO_RATIO: Control = switch("ratio", &["2A", "2", "4", "8", "12", "20"], true, 2.0);
const FET_RATIO: Control = switch("ratio", &["4", "8", "12", "20", "ALL"], true, 0.0);

const ALL: &[Control] = &[FREQ, HPF, HF, GAIN, Q, THR, INPUT, ATTACK, REDUCE, IN, BELL, LISTEN, OPTO_RATIO];

#[test]
fn every_kind_passes_the_check() {
    check(ALL).unwrap();
    check(&[FET_RATIO, RATIO]).unwrap();
}

#[test]
fn the_check_catches_a_bad_description() {
    let mut c = GAIN;
    c.kind =
        Kind::Value { lo: -15.0, hi: 15.0, scale: Scale::Linear, fine: 0.5, coarse: 1.75, unit: Unit::Db };
    assert!(check(&[c]).unwrap_err().contains("whole number"));
    assert!(check(&[GAIN, GAIN]).unwrap_err().contains("twice"));
    let mut c = GAIN;
    c.detent = 20.0;
    assert!(check(&[c]).is_err());
    let mut c = Q;
    c.kind = Kind::Value { lo: 0.0, hi: 4.0, scale: Scale::Log, fine: 1.0 / 6.0, coarse: 0.5, unit: Unit::Q };
    assert!(check(&[c]).is_err());
    let mut c = IN;
    c.default = 2.0;
    assert!(check(&[c]).is_err());
}

#[test]
fn the_grid_is_anchored_at_the_detent() {
    // A band's default is on its own grid, and the first step lands on a clean value.
    assert_eq!(FREQ.step(400.0, 12, Size::Fine), 800.0);
    assert_eq!(FREQ.step(400.0, 1, Size::Coarse), FREQ.step(400.0, 4, Size::Fine));
    assert_eq!(FREQ.step(400.0, -12, Size::Fine), 200.0);
    assert_eq!(Q.step(1.0, 6, Size::Fine), 2.0);
    assert_eq!(Q.step(1.0, 2, Size::Coarse), 2.0);
    assert_eq!(GAIN.step(0.0, 1, Size::Fine), 0.5);
    assert_eq!(GAIN.step(0.0, -1, Size::Coarse), -3.0);
    assert_eq!(ATTACK.step(3.0, 4, Size::Fine), 3.4);
    assert_eq!(ATTACK.step(3.0, -1, Size::Coarse), 2.0);
}

#[test]
fn stepping_from_off_the_grid() {
    // 0.3 dB: up to 0.5, down to 0.
    assert_eq!(GAIN.step(0.3, 1, Size::Fine), 0.5);
    assert_eq!(GAIN.step(0.3, -1, Size::Fine), 0.0);
    // Coarse from off the grid: m fine steps from the grid point behind it.
    assert_eq!(GAIN.step(0.3, 1, Size::Coarse), 3.0);
    assert_eq!(GAIN.step(0.3, -1, Size::Coarse), -2.5);
    // 1,000 Hz is off the LMF's grid from 400 Hz: the next points either side.
    let up = FREQ.step(1000.0, 1, Size::Fine);
    let down = FREQ.step(1000.0, -1, Size::Fine);
    assert!(up > 1000.0 && up < 1000.0 * libm::pow(2.0, 1.0 / 12.0));
    assert!(down < 1000.0 && down > 1000.0 / libm::pow(2.0, 1.0 / 12.0));
    assert_eq!(FREQ.step(up, -1, Size::Fine), down);
}

#[test]
fn the_ends() {
    // A step past the range lands on the end; a step back goes to the grid point below it.
    assert_eq!(FREQ.step(2400.0, 1, Size::Coarse), 2500.0);
    let below = FREQ.step(2500.0, -1, Size::Fine);
    assert_eq!(below, FREQ.grid(libm::floor(12.0 * libm::log2(2500.0 / 400.0))));
    assert!(below < 2500.0);
    assert_eq!(FREQ.step(200.0, -3, Size::Fine), 200.0);
    assert_eq!(GAIN.step(15.0, 1, Size::Coarse), 15.0);
    assert_eq!(GAIN.step(14.0, 1, Size::Coarse), 15.0);
    assert_eq!(GAIN.step(15.0, -1, Size::Fine), 14.5);
    assert_eq!(HPF.step(400.0, 100, Size::Fine), 400.0);
    // Switches stop at their ends, no wrap, either size.
    assert_eq!(LISTEN.step(2.0, 1, Size::Fine), 2.0);
    assert_eq!(LISTEN.step(0.0, -1, Size::Coarse), 0.0);
    assert_eq!(LISTEN.step(0.0, 1, Size::Coarse), 1.0);
    assert_eq!(LISTEN.step(0.0, 5, Size::Fine), 2.0);
}

#[test]
fn up_and_back_from_anywhere_on_the_grid() {
    for c in [FREQ, HPF, HF, GAIN, Q, ATTACK, REDUCE] {
        for k in -40..40 {
            let v = c.grid(k as f64);
            let Kind::Value { lo, hi, .. } = c.kind else { unreachable!() };
            if !(lo..=hi).contains(&v) {
                continue;
            }
            for n in 1..5 {
                for size in [Size::Fine, Size::Coarse] {
                    let there = c.step(v, n, size);
                    if there == hi {
                        continue;
                    }
                    assert_eq!(c.step(there, -n, size), v, "{} from {v}, {n} {size:?}", c.id);
                }
            }
        }
    }
}

#[test]
fn shown() {
    assert_eq!(HPF.format(80.0), "80 Hz");
    assert_eq!(FREQ.format(450.0), "450 Hz");
    assert_eq!(FREQ.format(1200.0), "1.20 kHz");
    assert_eq!(HF.format(16000.0), "16.0 kHz");
    assert_eq!(HPF.format(HPF.step(80.0, -1, Size::Fine)), "75.5 Hz");
    assert_eq!(FREQ.format(999.7), "1.00 kHz");
    assert_eq!(GAIN.format(3.0), "+3.0 dB");
    assert_eq!(GAIN.format(-4.5), "\u{2212}4.5 dB");
    assert_eq!(GAIN.format(0.0), "0.0 dB");
    assert_eq!(GAIN.format(-0.02), "0.0 dB");
    assert_eq!(Q.format(1.4), "Q 1.40");
    assert_eq!(RATIO.format(4.0), "4:1");
    assert_eq!(RATIO.format(2.5), "2.5:1");
    assert_eq!(OPTO_RATIO.format(0.0), "2A");
    assert_eq!(OPTO_RATIO.format(2.0), "4:1");
    assert_eq!(FET_RATIO.format(4.0), "ALL");
    assert_eq!(ATTACK.format(1.0), "1 (800 µs)");
    assert_eq!(ATTACK.format(7.0), "7 (20.0 µs)");
    assert_eq!(ATTACK.format(4.0), "4 (126 µs)");
    assert_eq!(ATTACK.format(3.4), "3.4 (183 µs)");
    assert_eq!(REDUCE.format(40.0), "40 (\u{2212}18.9 dBFS)");
    assert_eq!(REDUCE.format(0.0), "0 (+10.0 dBFS)");
    assert_eq!(IN.format(1.0), "in");
    assert_eq!(BELL.format(0.0), "shelf");
}

#[test]
fn read_forgivingly() {
    for t in ["1200", "1.2k", "1k2", "1.2 kHz", "1200hz", "1.2KHZ", " 1 200 Hz "] {
        assert_eq!(FREQ.parse(t), Some(1200.0), "{t}");
    }
    for t in ["3", "+3", "3dB", "3 db", "+3.0 dB"] {
        assert_eq!(GAIN.parse(t), Some(3.0), "{t}");
    }
    for t in ["-4.5", "\u{2212}4.5", "\u{2212}4.5 dB", "-4.5dB"] {
        assert_eq!(GAIN.parse(t), Some(-4.5), "{t}");
    }
    for t in ["1.4", "Q1.4", "q 1.4", "Q 1.40"] {
        assert_eq!(Q.parse(t), Some(1.4), "{t}");
    }
    for t in ["4", "4:1", " 4 : 1 "] {
        assert_eq!(RATIO.parse(t), Some(4.0), "{t}");
    }
    // A ratio switch: by name, by what it shows, or a unique prefix.
    assert_eq!(OPTO_RATIO.parse("2a"), Some(0.0));
    assert_eq!(OPTO_RATIO.parse("2"), Some(1.0));
    assert_eq!(OPTO_RATIO.parse("2:1"), Some(1.0));
    assert_eq!(OPTO_RATIO.parse("12:1"), Some(4.0));
    assert_eq!(FET_RATIO.parse("all"), Some(4.0));
    assert_eq!(FET_RATIO.parse("a"), Some(4.0));
    assert_eq!(LISTEN.parse("TAK"), Some(2.0));
    assert_eq!(LISTEN.parse("Band"), Some(1.0));
    assert_eq!(BELL.parse("s"), Some(0.0));
    // A dial: a bare number sets the dial; a time lands on the nearest 0.1 of it, and shows.
    assert_eq!(ATTACK.parse("3.4"), Some(3.4));
    for t in ["330us", "0.33ms", "330 µs", "330 μs", "330 US"] {
        let d = ATTACK.parse(t).unwrap();
        assert_eq!(d, 2.4, "{t}");
        assert_eq!(ATTACK.format(d), "2.4 (338 µs)");
    }
    assert_eq!(REDUCE.parse("-18.9 dBFS"), Some(40.0));
    assert_eq!(REDUCE.parse("40"), Some(40.0));
}

#[test]
fn a_reading_that_shows_as_a_grid_point_is_that_point() {
    // The LMF's grid point above 1 kHz is near 1,008 Hz and shows as 1.01 kHz: that text reads
    // back to it exactly, while a number typed is the number.
    let g = FREQ.step(1000.0, 1, Size::Fine);
    assert_eq!(FREQ.format(g), "1.01 kHz");
    assert_eq!(FREQ.parse("1.01 kHz"), Some(g));
    assert_eq!(FREQ.parse("1.01KHZ"), Some(g));
    assert_eq!(FREQ.parse("1.01k"), Some(1010.0));
    assert_eq!(FREQ.parse("1010"), Some(1010.0));
    let g = ATTACK.grid(4.0);
    assert_eq!(ATTACK.parse(&ATTACK.format(g)), Some(g));
}

#[test]
fn out_of_range_is_held_and_nonsense_refused() {
    assert_eq!(GAIN.parse("40"), Some(15.0));
    assert_eq!(GAIN.parse("-99 dB"), Some(-15.0));
    assert_eq!(FREQ.parse("20k"), Some(2500.0));
    assert_eq!(FREQ.parse("5"), Some(200.0));
    assert_eq!(ATTACK.parse("1s"), Some(1.0));
    assert_eq!(ATTACK.parse("1ns"), None);
    for t in ["", "  ", "loud", "1.2.3", "k", "3 Hz", "nan", "inf", "-inf", "1k2k", "q", "x4"] {
        assert_eq!(GAIN.parse(t), None, "{t:?}");
    }
    for t in ["k", "abc", "1kk", "1.2k3"] {
        assert_eq!(FREQ.parse(t), None, "{t:?}");
    }
    assert_eq!(LISTEN.parse("x"), None);
    assert_eq!(OPTO_RATIO.parse("1"), Some(4.0)); // "12", the one name starting with 1
    assert_eq!(OPTO_RATIO.parse("3"), None);
    assert_eq!(GAIN.clamp(f64::NAN), 0.0);
    assert_eq!(GAIN.clamp(f64::INFINITY), 15.0);
    assert_eq!(LISTEN.clamp(1.6), 2.0);
    assert_eq!(LISTEN.clamp(-3.0), 0.0);
    assert_eq!(GAIN.format(99.0), "+15.0 dB");
}

#[test]
fn found_by_id() {
    assert_eq!(find(ALL, "lmf.q").unwrap().short, "X");
    assert!(find(ALL, "nope").is_none());
}
