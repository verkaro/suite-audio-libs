//! The audio interfaces, by what they are: a USB interface by its `/dev/snd/by-id` name, which
//! carries its serial, so two boxes of the same model stay apart; a card with no serial (PCI,
//! onboard) by its `/dev/snd/by-path` name. Each is found again at its current `hw:N` whenever
//! JACK starts, wherever it's plugged in. Everything is read under a root, so tests fake `/proc`,
//! `/sys` and `/dev/snd`. It only reads.
//!
//! Moved in from Dexter's `dexter-system` (`src/interfaces.rs`, with `cards` and `limit` from its
//! `lib.rs`) at `e98d44f` (2026-11-11), for `as-jackd` and jackbay (`RFC-as-jackd.md`). As it was.
//! No JACK here, so a tool that never links JACK can name its card by the same id.

use std::fs;
use std::path::Path;

use serde::Serialize;

/// The rates offered for a card that doesn't list its own (PCI, onboard).
const COMMON_RATES: [u32; 4] = [44100, 48000, 88200, 96000];

/// An interface.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Interface {
    /// Its name in `/dev/snd/by-id`, else in `by-path`: what a rig and `dexter.yaml` keep.
    pub id: String,
    /// Its card number now: `hw:N`.
    pub card: u32,
    /// ALSA's id for the card, e.g. `USB`.
    pub alsa: String,
    /// E.g. `Clarett 2Pre USB`.
    pub name: String,
    /// The USB serial number; empty for a card without one.
    pub serial: String,
    /// Where it's plugged in: `USB port 2`, or `PCI 06:00.1`.
    pub port: String,
    pub usb: bool,
    /// The USB vendor and product, `1235:8215`; empty for a card that isn't USB. Not shown.
    #[serde(skip)]
    pub usb_id: String,
    /// The rates it takes, each with its inputs and outputs at that rate (0: not known).
    pub modes: Vec<Mode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Mode {
    pub rate: u32,
    pub inputs: u32,
    pub outputs: u32,
}

impl Interface {
    /// The inputs at rate, if known.
    pub fn inputs_at(&self, rate: u32) -> u32 {
        self.modes.iter().find(|m| m.rate == rate).map_or(0, |m| m.inputs)
    }
}

/// The card a `/dev/snd/by-*` link names: `../controlC3` is 3.
fn link_card(root: &Path, rel: &str) -> Option<u32> {
    let target = fs::read_link(root.join(rel)).ok()?;
    target.file_name()?.to_str()?.strip_prefix("controlC")?.parse().ok()
}

/// The names in a `/dev/snd/by-*` directory, with their cards.
fn links(root: &Path, dir: &str) -> Vec<(String, u32)> {
    let mut v: Vec<(String, u32)> = fs::read_dir(root.join(dir))
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let n = e.file_name().to_str()?.to_string();
                    let card = link_card(root, &format!("{dir}/{n}"))?;
                    Some((n, card))
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// Every interface there is now, by card number.
pub fn interfaces(root: &Path) -> Vec<Interface> {
    let cards = cards(&fs::read_to_string(root.join("proc/asound/cards")).unwrap_or_default());
    let by_id = links(root, "dev/snd/by-id");
    let by_path = links(root, "dev/snd/by-path");
    cards
        .into_iter()
        .filter_map(|(n, alsa, name)| {
            // by-path: the plain name, not USB's second (usbv2) one.
            let path = by_path
                .iter()
                .filter(|(_, c)| *c == n)
                .map(|(p, _)| p.clone())
                .min_by_key(|p| (p.contains("-usbv"), p.len()));
            let id = by_id.iter().find(|(_, c)| *c == n).map(|(i, _)| i.clone()).or_else(|| path.clone())?;
            let stream = fs::read_to_string(root.join(format!("proc/asound/card{n}/stream0"))).ok();
            let usb = stream.is_some()
                || id.starts_with("usb-")
                || path.as_deref().is_some_and(|p| p.contains("-usb"));
            let serial = fs::read_to_string(root.join(format!("sys/class/sound/card{n}/device/../serial")))
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            let usb_id = fs::read_to_string(root.join(format!("proc/asound/card{n}/usbid")))
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            let port = match &path {
                Some(p) if usb => p
                    .split_once("-usb-")
                    .and_then(|(_, r)| r.split(':').nth(1))
                    .map_or_else(|| "USB".to_string(), |port| format!("USB port {port}")),
                Some(p) => {
                    format!("PCI {}", p.strip_prefix("pci-0000:").or(p.strip_prefix("pci-")).unwrap_or(p))
                }
                None => String::new(),
            };
            let modes = match &stream {
                Some(s) => modes(s),
                None => COMMON_RATES.iter().map(|&rate| Mode { rate, inputs: 0, outputs: 0 }).collect(),
            };
            Some(Interface { id, card: n, alsa, name, serial, port, usb, usb_id, modes })
        })
        .collect()
}

/// The rates in a USB card's `stream0`, each with the most channels any of its alternate
/// settings has at that rate, capture and playback: the rates both directions take.
fn modes(stream: &str) -> Vec<Mode> {
    // (capture?, channels, rates) for each alternate setting.
    let mut sets: Vec<(bool, u32, Vec<u32>)> = Vec::new();
    let mut capture = None;
    let mut channels = 0;
    for l in stream.lines() {
        let t = l.trim();
        if t == "Playback:" {
            capture = Some(false);
        } else if t == "Capture:" {
            capture = Some(true);
        } else if let Some(c) = t.strip_prefix("Channels:") {
            channels = c.trim().parse().unwrap_or(0);
        } else if let (Some(r), Some(cap)) = (t.strip_prefix("Rates:"), capture) {
            sets.push((cap, channels, rates(r)));
        }
    }
    let most = |cap: bool, rate: u32| {
        sets.iter().filter(|(c, _, rs)| *c == cap && rs.contains(&rate)).map(|(_, ch, _)| *ch).max()
    };
    let has_playback = sets.iter().any(|(c, _, _)| !c);
    let mut all: Vec<u32> = sets.iter().flat_map(|(_, _, rs)| rs.iter().copied()).collect();
    all.sort_unstable();
    all.dedup();
    all.into_iter()
        .filter_map(|rate| {
            let inputs = most(true, rate)?;
            let outputs = most(false, rate);
            if has_playback && outputs.is_none() {
                return None;
            }
            Some(Mode { rate, inputs, outputs: outputs.unwrap_or(0) })
        })
        .collect()
}

/// `44100, 48000` or `8000 - 96000 (continuous)`.
fn rates(text: &str) -> Vec<u32> {
    let t = text.trim();
    if let Some((lo, rest)) = t.split_once(" - ") {
        let lo: u32 = lo.trim().parse().unwrap_or(0);
        let hi: u32 = rest.split_whitespace().next().and_then(|h| h.parse().ok()).unwrap_or(0);
        return [44100, 48000, 88200, 96000, 176400, 192000]
            .into_iter()
            .filter(|r| (lo..=hi).contains(r))
            .collect();
    }
    t.split(',').filter_map(|r| r.trim().parse().ok()).collect()
}

/// The card the interface called id is now, for `hw:N`; an error that says so if it isn't
/// there (unplugged, or switched off).
pub fn resolve(root: &Path, id: &str) -> Result<u32, String> {
    if id.contains('/') || id.starts_with('.') {
        return Err(format!("{id:?} isn't an interface's name"));
    }
    link_card(root, &format!("dev/snd/by-id/{id}"))
        .or_else(|| link_card(root, &format!("dev/snd/by-path/{id}")))
        .ok_or_else(|| format!("the interface {id} isn't there: is it plugged in and switched on?"))
}

/// The interface an ALSA device names (`hw:USB`, `hw:1`, `hw:CARD=USB,DEV=0`), as `~/.jackdrc`
/// and older projects have it.
pub fn by_device(root: &Path, device: &str) -> Option<Interface> {
    let d = device.strip_prefix("hw:").or(device.strip_prefix("plughw:"))?;
    let card = d.strip_prefix("CARD=").unwrap_or(d).split(',').next()?;
    interfaces(root).into_iter().find(|i| i.alsa == card || i.card.to_string() == card)
}

/// Whether JACK can run realtime: the realtime priority limit allows 70 or more.
pub fn realtime_allowed(root: &Path) -> bool {
    let limits = fs::read_to_string(root.join("proc/self/limits")).unwrap_or_default();
    matches!(limit(&limits, "Max realtime priority"), Some(None))
        || matches!(limit(&limits, "Max realtime priority"), Some(Some(p)) if p >= 70)
}

/// The sound cards in `/proc/asound/cards`: number, id and name (without the driver).
fn cards(text: &str) -> Vec<(u32, String, String)> {
    text.lines()
        .filter_map(|l| {
            let (n, rest) = l.trim_start().split_once(' ')?;
            let n = n.parse().ok()?;
            let id = rest.split_once('[')?.1.split_once(']')?.0.trim().to_string();
            // "USB-Audio - Clarett 2Pre USB": the driver, then the card's name.
            let full = rest.split_once("]: ")?.1.trim();
            let name = full.split_once(" - ").map_or(full, |(_, n)| n).to_string();
            Some((n, id, name))
        })
        .collect()
}

/// A soft limit from `/proc/self/limits`: None is unlimited.
fn limit(limits: &str, name: &str) -> Option<Option<u64>> {
    let rest = limits.lines().find_map(|l| l.strip_prefix(name))?;
    let soft = rest.split_whitespace().next()?;
    Some(if soft == "unlimited" { None } else { Some(soft.parse().ok()?) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A scratch root holding the given files.
    fn root(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let d = std::env::temp_dir().join(format!("as-interfaces-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        for (rel, text) in files {
            let p = d.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, text).unwrap();
        }
        d
    }

    const CLARETT_STREAM: &str = "Focusrite Clarett 2Pre USB at usb-0000:06:00.3-2, high speed : USB Audio\n\n\
    Playback:\n  Status: Running\n  Interface 1\n    Altset 1\n    Format: S32_LE\n    Channels: 4\n    Rates: 44100, 48000, 88200, 96000, 176400, 192000\n\n\
    Capture:\n  Status: Running\n  Interface 2\n    Altset 1\n    Channels: 12\n    Rates: 44100, 48000\n  Interface 2\n    Altset 2\n    Channels: 8\n    Rates: 44100, 48000, 88200, 96000\n  Interface 2\n    Altset 3\n    Channels: 2\n    Rates: 44100, 48000, 88200, 96000, 176400, 192000\n";

    const LIMITS: &str = "Limit                     Soft Limit           Hard Limit           Units     \n\
    Max locked memory         unlimited            unlimited            bytes     \n\
    Max realtime priority     95                   95                   \n";

    fn two_claretts() -> PathBuf {
        let r = root(
            "interfaces",
            &[
                (
                    "proc/asound/cards",
                    " 0 [USB            ]: USB-Audio - Clarett 2Pre USB\n                      Focusrite at usb-0000:06:00.3-2\n \
                     1 [Generic        ]: HDA-Intel - HD-Audio Generic\n                      HD-Audio Generic at 0xc0688000\n \
                     3 [USB_1          ]: USB-Audio - Clarett 2Pre USB\n                      Focusrite at usb-0000:06:00.3-4\n",
                ),
                ("proc/asound/card0/stream0", CLARETT_STREAM),
                ("proc/asound/card3/stream0", CLARETT_STREAM),
                ("proc/asound/card0/usbid", "1235:8208\n"),
                ("sys/class/sound/card0/serial", "00000001\n"),
                ("sys/class/sound/card3/serial", "00099999\n"),
                ("proc/self/limits", LIMITS),
            ],
        );
        for n in [0, 3] {
            fs::create_dir_all(r.join(format!("sys/class/sound/card{n}/device"))).unwrap();
        }
        fs::create_dir_all(r.join("dev/snd/by-id")).unwrap();
        fs::create_dir_all(r.join("dev/snd/by-path")).unwrap();
        let link = |to: &str, at: &str| std::os::unix::fs::symlink(to, r.join(at)).unwrap();
        link("../controlC0", "dev/snd/by-id/usb-Focusrite_Clarett_2Pre_USB_00000001-00");
        link("../controlC3", "dev/snd/by-id/usb-Focusrite_Clarett_2Pre_USB_00099999-00");
        link("../controlC0", "dev/snd/by-path/pci-0000:06:00.3-usb-0:2:1.0");
        link("../controlC0", "dev/snd/by-path/pci-0000:06:00.3-usbv2-0:2:1.0");
        link("../controlC3", "dev/snd/by-path/pci-0000:06:00.3-usb-0:4:1.0");
        link("../controlC1", "dev/snd/by-path/pci-0000:06:00.1");
        r
    }

    #[test]
    fn interfaces_by_what_they_are() {
        let r = two_claretts();
        let v = interfaces(&r);
        let ids: Vec<(&str, u32, &str, &str)> =
            v.iter().map(|i| (i.id.as_str(), i.card, i.serial.as_str(), i.port.as_str())).collect();
        assert_eq!(
            ids,
            [
                ("usb-Focusrite_Clarett_2Pre_USB_00000001-00", 0, "00000001", "USB port 2"),
                ("pci-0000:06:00.1", 1, "", "PCI 06:00.1"),
                ("usb-Focusrite_Clarett_2Pre_USB_00099999-00", 3, "00099999", "USB port 4"),
            ]
        );
        let c = &v[0];
        assert!(c.usb && !v[1].usb);
        assert_eq!((c.usb_id.as_str(), v[1].usb_id.as_str()), ("1235:8208", ""));
        assert_eq!((c.name.as_str(), c.alsa.as_str()), ("Clarett 2Pre USB", "USB"));
        // Every rate both directions take, with the most channels at each.
        let modes: Vec<(u32, u32, u32)> = c.modes.iter().map(|m| (m.rate, m.inputs, m.outputs)).collect();
        assert_eq!(
            modes,
            [(44100, 12, 4), (48000, 12, 4), (88200, 8, 4), (96000, 8, 4), (176400, 2, 4), (192000, 2, 4)]
        );
        assert_eq!(c.inputs_at(96000), 8);
        assert_eq!(v[1].modes.iter().map(|m| m.rate).collect::<Vec<_>>(), [44100, 48000, 88200, 96000]);

        assert_eq!(resolve(&r, "usb-Focusrite_Clarett_2Pre_USB_00099999-00"), Ok(3));
        assert_eq!(resolve(&r, "pci-0000:06:00.1"), Ok(1));
        assert!(resolve(&r, "usb-Other_1-00").unwrap_err().contains("isn't there: is it plugged in"));
        assert!(resolve(&r, "../../etc").is_err());
        assert_eq!(by_device(&r, "hw:USB").map(|i| i.card), Some(0));
        assert_eq!(by_device(&r, "hw:3").map(|i| i.card), Some(3));
        assert_eq!(by_device(&r, "hw:CARD=USB_1,DEV=0").map(|i| i.card), Some(3));
        assert_eq!(by_device(&r, "hw:Nope"), None);
        assert!(realtime_allowed(&r));
        let _ = fs::remove_dir_all(&r);
    }

    #[test]
    fn continuous_rates() {
        assert_eq!(rates(" 8000 - 96000 (continuous)"), [44100, 48000, 88200, 96000]);
    }

    /// This machine's interfaces, to look at: `cargo test -p as-interfaces show_interfaces -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn show_interfaces() {
        for i in interfaces(Path::new("/")) {
            println!("{i:?}");
        }
    }
}
