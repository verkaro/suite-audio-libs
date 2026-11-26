//! The page's waveform in a browser: its window held to the table the panel's is
//! (`testdata/wave-view.txt`), and `kitWave` drawn and its pixels looked at. Only with
//! `SUITE_PAGE_TEST=1`: Falkon offscreen, driven by `tools/cdp.py`, with its home and every
//! `XDG_` folder in a scratch of the test's own, and no display, nor the session's bus.
//! `WAVE_SHOTS=DIR` keeps the screenshots, to look at.

use std::fs;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const CASES: &str = include_str!("../testdata/wave-view.txt");

/// What runs on the page, after the kit. `wave()` gives what's wrong, a line each.
const RUN: &str = r##"
const canvas = document.getElementById("c");
canvas.width = 800; canvas.height = 180;
kitCanvas(canvas);
setSkin("dark");
const wrong = [];
let held = 0;
function table(cases) {
  let win = null;
  for (const line of cases.split("\n")) {
    const [what, ...n] = line.split(/\s+/);
    if (what !== "open" && what !== "follow") continue;
    const [mark, step, from, to] = n.map(Number);
    win = what === "open" ? kitWaveOpen(mark, step) : kitWaveFollow(win, mark, step);
    if (!(Math.abs(win.from - from) < 1e-9 && Math.abs(win.to - to) < 1e-9)) wrong.push(line + ": " + JSON.stringify(win));
    held++;
  }
  const w = { from: -10, to: 30 };
  for (const [m, s] of [[NaN, 1], [20, Infinity]]) {
    const got = kitWaveFollow(w, m, s);
    if (got.from !== -10 || got.to !== 30) wrong.push("a window with no mark or step moved: " + JSON.stringify(got));
  }
}
// The columns across that have the colour somewhere in the box.
function across(box, color) {
  const [r, g, b] = rgb(color), d = canvas.getContext("2d").getImageData(box.x, box.y, box.w, box.h).data;
  const out = [];
  for (let x = 0; x < box.w; x++)
    for (let y = 0; y < box.h; y++) {
      const i = 4 * (y * box.w + x);
      if (d[i] === r && d[i + 1] === g && d[i + 2] === b) { out.push(box.x + x); break; }
    }
  return out;
}
function is(what, got, want, within) {
  if (!(Math.abs(got - want) <= (within || 0))) wrong.push(what + ": " + got + ", not " + want);
}
function drawn(floor) {
  const flat = (v) => Array(200).fill(v), whole = { from: 0, to: 1000, lo: flat(-0.5), hi: flat(0.5) };
  const r = { x: 0, y: 0, w: 400, h: 147 }, name = floor === undefined ? "straight" : "dB";
  const draw = (from, to) => {
    ctx.fillStyle = C.bg; ctx.fillRect(0, 0, canvas.width, canvas.height);
    return kitWave(r, { from, to, lo: flat(-0.5), hi: flat(0.5) }, { unit: "ms", floor, whole });
  };
  // A quarter of it before the start: no sound there, and no ground.
  let boxes = draw(-100, 300);
  const plot = kitWavePlot(boxes.window), quarter = plot.x + Math.floor(plot.w / 4);
  let sound = across(plot, C.tape);
  is(name + " starts", sound[0], quarter, 1);
  is(name + " runs to the right edge", sound[sound.length - 1], plot.x + plot.w - 1);
  is(name + " no ground before the start", across({ x: plot.x + 50, y: plot.y, w: quarter - plot.x - 52, h: plot.h }, C.rule).length, 0);
  // Past the end, and wholly past it.
  draw(900, 1100);
  sound = across(plot, C.tape);
  is(name + " ends", sound[sound.length - 1], plot.x + plot.w / 2 - 1, 1);
  draw(2000, 2400);
  is(name + " wholly past", across(plot, C.tape).length, 0);
  // A tail 4 dB over a dB picture's floor shows in the strip, under the window's mark.
  if (floor !== undefined) {
    const quiet = Array(200).fill(Math.pow(10, -86 / 20));
    ctx.fillStyle = C.bg; ctx.fillRect(0, 0, canvas.width, canvas.height);
    const b = kitWave(r, { from: 250, to: 500, lo: quiet, hi: quiet }, { floor, whole: { from: 0, to: 1000, lo: quiet, hi: quiet } });
    is("the quiet in the strip", across({ x: b.strip.x + 1, y: b.strip.y + 1, w: b.strip.w - 2, h: b.strip.h - 2 }, C.tape).length, 99);
    boxes = draw(-100, 300);
  }
  // The strip: the whole sound across, the window's place in the sound's colour.
  const inner = { x: boxes.strip.x + 1, y: boxes.strip.y + 1, w: boxes.strip.w - 2, h: boxes.strip.h - 2 };
  const lit = (from, to) => { draw(from, to); return across(inner, C.tape); };
  let at = lit(250, 500);
  is(name + " strip lit from", at[0] - inner.x, 100);
  is(name + " strip lit wide", at.length, 99);
  is(name + " strip whole across", at.length + across(inner, C.faint).length, inner.w);
  is(name + " a narrow window shows", lit(500, 500.04).length, 3);
  is(name + " past the end shows at the end", lit(5000, 5001)[0] - inner.x, 395);
}
function times() {
  const r = { x: 0, y: 0, w: 400, h: 147 }, { window: box, strip } = kitWaveSplit(r), whole = { from: 0, to: 1000 };
  is("the boxes", box.y + box.h + 3, strip.y);
  is("a tap at the left", kitWaveTimeAt(box, 100, 300, 1), 100);
  is("a tap past the right", kitWaveTimeAt(box, 100, 300, 5000), 300);
  is("a tap in the strip", kitWaveStripTimeAt(strip, whole, 200), 500, 2);
}
// For the screenshot: a click and a fall from it, a window at a mark and one past the end.
function shown(skin) {
  setSkin(skin);
  ctx.fillStyle = C.bg; ctx.fillRect(0, 0, canvas.width, canvas.height);
  const fall = (t) => t < 0 || t > 600 ? 0 : Math.pow(10, -3 * t / 600) * (0.6 + 0.4 * ((Math.round(t * 13) % 97) / 97));
  const picture = (from, to, n) => {
    const hi = Array.from({ length: n }, (_, k) => fall(from + (to - from) * k / n));
    return { from, to, hi, lo: hi.map((v) => -0.9 * v) };
  };
  const whole = picture(0, 600, 600);
  [[120, 0, undefined], [598, 405, -60]].forEach(([mark, x, floor]) => {
    const w = kitWaveOpen(mark, 5);
    kitWave({ x: x + 2, y: 4, w: 391, h: 170 }, picture(w.from, w.to, 400),
      { unit: "ms", floor, whole, markers: [{ at: mark, label: "MARK", color: C.amber }] });
  });
}
function wave(cases) {
  table(cases);
  drawn(undefined);
  drawn(-90);
  times();
  return "held " + held + "; wrong " + wrong.length + (wrong.length ? ": " + wrong.join(" | ") : "");
}
"##;

/// Falkon, ended by its own pid however the test ends.
struct Browser(Child);

impl Drop for Browser {
    fn drop(&mut self) {
        // SAFETY: a signal to our own child, by its pid.
        unsafe { libc::kill(self.0.id() as i32, libc::SIGTERM) };
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(10) {
            if self.0.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn folder(root: &Path, name: &str) -> PathBuf {
    let d = root.join(name);
    fs::create_dir_all(&d).unwrap();
    fs::set_permissions(&d, fs::Permissions::from_mode(0o700)).unwrap();
    d
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

#[test]
fn the_pages_window_and_waveform() {
    if std::env::var_os("SUITE_PAGE_TEST").is_none() {
        eprintln!("skipped: set SUITE_PAGE_TEST=1 to run it in Falkon, offscreen");
        return;
    }
    let root = std::env::temp_dir().join(format!("as-page-kit-wave-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let home = folder(&root, "home");
    let shots = std::env::var_os("WAVE_SHOTS").map_or_else(|| folder(&root, "shots"), PathBuf::from);
    let page = root.join("wave.html");
    // The kit as its own file, as a tool's server gives it: its first lines name the tag that
    // loads it, which would end a script it was written into.
    fs::write(root.join("page-kit.js"), as_page_kit::JS).unwrap();
    let html = format!(
        "<!doctype html><meta charset=utf-8><body style=margin:0><canvas id=c></canvas>\
         <script>window.errors=[];onerror=(m,f,l)=>errors.push(m+' line '+l)</script>\
         <script src=page-kit.js></script><script>{RUN}</script>"
    );
    fs::write(&page, html).unwrap();
    let url = format!("file://{}", page.display());
    let port = free_port();
    let falkon = Command::new("falkon")
        .args(["--no-remote", "--private-browsing", &url])
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", folder(&root, "config"))
        .env("XDG_CACHE_HOME", folder(&root, "cache"))
        .env("XDG_DATA_HOME", folder(&root, "data"))
        .env("XDG_STATE_HOME", folder(&root, "state"))
        .env("XDG_RUNTIME_DIR", folder(&root, "run"))
        .env("QT_QPA_PLATFORM", "offscreen")
        .env("QTWEBENGINE_REMOTE_DEBUGGING", port.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("falkon is installed");
    let _falkon = Browser(falkon);
    let t = Instant::now();
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(t.elapsed() < Duration::from_secs(30), "Falkon didn't open its debugging port");
        std::thread::sleep(Duration::from_millis(200));
    }
    std::thread::sleep(Duration::from_secs(3));

    let cases = format!(
        "(() => {{ try {{ return wave({}); }} catch (e) {{ return 'threw ' + e + ' / ' + errors.join('; '); }} }})()",
        json(CASES)
    );
    let steps = [
        format!("[\"nav\", {url:?}]"),
        format!("[\"eval\", {}]", json(&cases)),
        "[\"size\", 840, 220]".into(),
        "[\"eval\", \"shown('dark')\"]".into(),
        "[\"shot\", \"wave-dark\"]".into(),
        "[\"eval\", \"shown('seventies')\"]".into(),
        "[\"shot\", \"wave-seventies\"]".into(),
    ];
    let script = root.join("steps.json");
    fs::write(&script, format!("[{}]", steps.join(","))).unwrap();
    let cdp = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tools/cdp.py");
    let out = Command::new("timeout")
        .args(["60", "python3", "-I", cdp, &port.to_string()])
        .arg(&script)
        .arg(&shots)
        .output()
        .expect("python3");
    let said = String::from_utf8_lossy(&out.stdout);
    let held = CASES.lines().filter(|l| l.starts_with("open ") || l.starts_with("follow ")).count();
    assert!(
        out.status.success() && said.contains(&format!("held {held}; wrong 0")),
        "{said}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(shots.join("wave-seventies.png").exists(), "{said}");
    let _ = fs::remove_dir_all(&root);
}

/// A string as JSON writes it.
fn json(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 32 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
