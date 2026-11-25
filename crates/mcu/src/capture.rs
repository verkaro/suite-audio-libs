//! A capture (Dexter's `docs/faderport-capture.md`): what each control of a surface sent, and
//! whether its LEDs and motor answered, as Dexter's `contrib/capture-surface.sh` writes it (its
//! `capture.sh` before the move into Dexter), and what doesn't decode as Mackie
//! Control would have it. Its findings (`findings.txt`, beside it) explain what doesn't.

use crate::mcu::{Event, decode};
use crate::midi::Parser;

/// One line: `button PLAY: 90 5E 7F 90 5E 00`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// What was captured: `button PLAY`, `fader up`, `led PLAY 5E`…
    pub what: String,
    /// The bytes it sent, or Warren's answer for the LEDs and the motor.
    pub got: String,
}

impl Line {
    /// The events in the bytes.
    pub fn events(&self) -> Vec<Event> {
        let bytes: Vec<u8> =
            self.got.split_whitespace().filter_map(|h| u8::from_str_radix(h, 16).ok()).collect();
        Parser::default().feed(&bytes).into_iter().map(decode).collect()
    }
}

/// The lines of a capture, comments left out.
pub fn parse(text: &str) -> Vec<Line> {
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| l.split_once(':'))
        .map(|(w, g)| Line { what: w.trim().into(), got: g.trim().into() })
        .collect()
}

/// What in a capture isn't as Mackie Control has it: none when all of it is.
pub fn problems(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut bad = |l: &Line, why: &str| out.push(format!("{}: {why} ({})", l.what, l.got));
    for l in parse(text) {
        let ev = l.events();
        let what = l.what.as_str();
        if what.starts_with("button ") {
            match ev.as_slice() {
                [Event::Button { note: a, down: true }, Event::Button { note: b, down: false }] if a == b => {
                }
                _ => bad(&l, "not one note pressed and let go"),
            }
        } else if what == "fader touch" {
            if !(ev.first() == Some(&Event::Touch { n: 0, on: true })
                && ev.last() == Some(&Event::Touch { n: 0, on: false }))
            {
                bad(&l, "not fader 1 touched, then let go");
            }
        } else if what == "fader up" || what == "fader down" {
            let v: Vec<u16> = ev
                .iter()
                .filter_map(|e| if let Event::Fader { n: 0, value } = e { Some(*value) } else { None })
                .collect();
            let (lo, hi) = (v.iter().min().copied().unwrap_or(0), v.iter().max().copied().unwrap_or(0));
            if v.len() < 2 || lo > 0x0100 || hi < 0x3f00 {
                bad(&l, "fader 1 didn't go from the bottom to the top");
            }
        } else if let Some(way) = what.strip_prefix("knob ") {
            let d: Vec<i8> = ev
                .iter()
                .filter_map(|e| if let Event::Knob { n: 0, delta } = e { Some(*delta) } else { None })
                .collect();
            let ok = match way {
                "push" => matches!(ev.first(), Some(Event::Button { down: true, .. })),
                w if w.contains("right") => !d.is_empty() && d.iter().all(|&x| x > 0),
                w if w.contains("left") => !d.is_empty() && d.iter().all(|&x| x < 0),
                _ => true,
            };
            if !ok {
                bad(&l, "not knob 1 as Mackie Control has it");
            }
        } else if (what.starts_with("led ") || what.starts_with("motor ")) && l.got != "y" {
            bad(&l, "didn't answer");
        }
    }
    out
}

/// What in a capture isn't as Mackie Control has it and isn't explained by its findings (lines
/// `what: why`, as the capture's), and the findings that explain nothing there.
pub fn unexplained(text: &str, findings: &str) -> Vec<String> {
    let found = parse(findings);
    let probs = problems(text);
    let mut out: Vec<String> = probs
        .iter()
        .filter(|p| !found.iter().any(|f| p.starts_with(&format!("{}: ", f.what))))
        .cloned()
        .collect();
    for f in &found {
        if !probs.iter().any(|p| p.starts_with(&format!("{}: ", f.what))) {
            out.push(format!("{}: explains nothing in the capture", f.what));
        }
    }
    out
}
