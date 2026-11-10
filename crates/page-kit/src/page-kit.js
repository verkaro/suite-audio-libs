"use strict";
// The Dexter family's page kit (suite-audio-libs, crates/page-kit; moved in from place's page,
// itself scape's and Dexter's, VENDORED.md): the skins, the LEDs and their ladders, keys,
// readouts, lamps and text, drawn on one canvas. A page loads it before its own script
// (<script src="/page-kit.js"></script>), gives it its canvas (kitCanvas), and draws with it;
// what's tapped lands in HITS, the last added on top, for the page to clear each draw.

let ctx = null;
const HITS = [];
function kitCanvas(canvas) { ctx = canvas.getContext("2d"); }

// ---------- the common keys ----------
// The family's keys (STYLE.md, "Keys"), as data: each key by its KeyboardEvent.key, and what it
// means. Not a handler: what s does is each tool's own. A tool lists its keys in keys.json, and
// as_page_kit::check_keys holds that list to this table, which it reads from here as JSON.
const KEYS = [
  {"key": "?", "means": "the keys, over the screen"},
  {"key": ":", "means": "the bar: what this screen does"},
  {"key": "Escape", "means": "back out of what's open"},
  {"key": " ", "means": "play and stop"},
  {"key": "Tab", "means": "the next area or view"},
  {"key": "ArrowLeft", "means": "along what's there"},
  {"key": "ArrowRight", "means": "along what's there"},
  {"key": "ArrowUp", "means": "along what's there"},
  {"key": "ArrowDown", "means": "along what's there"},
  {"key": "q", "means": "quit this client"},
  {"key": "s", "means": "stop, then to the start"},
  {"key": "j", "means": "the next"},
  {"key": "k", "means": "the one before"},
  {"key": "h", "means": "back, a nudge by ear"},
  {"key": "l", "means": "on, a nudge by ear"},
  {"key": "H", "means": "the first"},
  {"key": "L", "means": "the last"},
  {"key": "/", "means": "search"},
  {"key": "x", "means": "take this away"},
  {"key": "u", "means": "undo"},
  {"key": "i", "means": "start changing this in place"},
  {"key": "Enter", "means": "open or choose; keep what's typed"},
  {"key": "y", "means": "yes to a question"}
];
const KEY_NAMES = { " ": "Space", Escape: "Esc", ArrowLeft: "←", ArrowRight: "→", ArrowUp: "↑", ArrowDown: "↓", Backspace: "⌫", PageUp: "PgUp", PageDown: "PgDn" };
const keyName = (k) => KEY_NAMES[k] || k;

// ---------- the skins ----------
// dark; STUDIO, a 1970s multitrack's blue-grey with cream keys; 70s, a broadcast console's
// olive with walnut cheeks; and a warm light one for daylight. key: on a skin with light keys,
// what's drawn on a key in place of text, dim, faint, amber, red and green. lit / unlit: the
// meters' LEDs, bottom up. glowLeds: the LEDs drawn glowing, with bloom (how much their light
// spills onto the panel).
function ladderColors(green, amber, red) {
  return Array.from({ length: 16 }, (_, k) => (k < 11 ? green : k < 15 ? amber : red));
}
const SKINS = {
  dark: {
    page: "#07060a", bg: "#0f0e0b", panel: "#1b1914", column: "#1b1914", rule: "#37322a", btn: "#2a261f", btnHi: "#3a352c",
    btnEdge: "#4a443a", text: "#e9e1cc", dim: "#9a917c", faint: "#5d564a",
    green: "#56d364", amber: "#ffb020", red: "#ff4b3e", lampOff: "#3a342a",
    well: "#0a0907", readout: "#ffb020", ghost: "#2a2314", glow: "#ffb02088", glowLeds: true, bloom: 0.85,
    onRed: "#1a0806", armText: "#ff8a7f", armIdle: "#8f8672", tape: "#5aa9e6",
    overlay: "#0a0907", menu: "#24211b", shade: "#000000aa", shadow: "#000000cc",
    lit: ladderColors("#56d364", "#ffb020", "#ff4b3e"),
    unlit: ladderColors("#1d2a1f", "#352a15", "#361b17"),
  },
  studio: {
    page: "#3f464e", bg: "#464e57", panel: "#5b646e", column: "#525a63", rule: "#78828c", btn: "#e6dcc6", btnHi: "#d4c7aa",
    btnEdge: "#2e343a", text: "#eef0f2", dim: "#b9c0c7", faint: "#8d969f",
    green: "#7ee08a", amber: "#ffc24a", red: "#ff6b5e", lampOff: "#bdb198",
    well: "#1a1310", readout: "#ff6a2b", ghost: "#33201a", glow: "#ff6a2b99",
    onRed: "#1a0806", armText: "#b8281c", armIdle: "#6b6252", tape: "#9ad4ff",
    overlay: "#2f353c", menu: "#4a525b", menuHi: "#66707a", shade: "#2a2f35cc", shadow: "#000000aa",
    lit: ladderColors("#5ee06e", "#ffc24a", "#ff5a4a"),
    unlit: ladderColors("#4a5a58", "#5e5a4c", "#5e4b4e"),
    key: { text: "#1f1d19", dim: "#5a5347", faint: "#a3997f", amber: "#b35c00", red: "#c0271a", green: "#1f8a3a" },
  },
  seventies: {
    page: "#2e2b1f", bg: "#38352a", panel: "#6b6843", column: "#5b3f2c", rule: "#8f8b62", btn: "#e2d1a4", btnHi: "#d2be8b",
    btnEdge: "#2b2618", text: "#eedcae", dim: "#cbba8f", faint: "#a39873", tile: "#b9a777",
    green: "#b6f07a", amber: "#ffb347", red: "#ff6a4d", lampOff: "#b9a57a",
    well: "#0c1311", readout: "#5ff2d0", ghost: "#15302a", glow: "#5ff2d099", glowLeds: true, bloom: 0.7,
    onRed: "#1a0806", armText: "#9c2615", armIdle: "#4e4530", tape: "#5ff2d0",
    overlay: "#3d3b27", menu: "#4d4a30", menuHi: "#6b6843", shade: "#2e2b1fcc", shadow: "#000000aa",
    lit: ladderColors("#a6f06a", "#ffb347", "#ff5a3d"),
    unlit: ladderColors("#545937", "#6b5b39", "#6b4635"),
    key: { text: "#231f14", dim: "#5e5540", faint: "#aa9e7b", amber: "#b0520a", red: "#b8291a", green: "#3f7d1f" },
  },
  light: {
    page: "#aeaaa2", bg: "#bab6ae", panel: "#a39e95", column: "#a39e95", rule: "#9a948a", btn: "#d3cfc7", btnHi: "#c4bdb0",
    btnEdge: "#857e72", text: "#29251e", dim: "#554e43", faint: "#6c6457",
    green: "#22803a", amber: "#a35f00", red: "#b02d20", lampOff: "#aaa396",
    well: "#b9cbd2", readout: "#1f3a4a", ghost: "#a9bcc4", glow: null,
    onRed: "#fff1ea", armText: "#b02d20", armIdle: "#6f6758", tape: "#1f5f8a",
    overlay: "#d3cfc7", menu: "#d3cfc7", shade: "#bab6aecc", shadow: "#00000040",
    lit: ladderColors("#2a9a40", "#d58d12", "#d03a2a"),
    unlit: ladderColors("#adbaa9", "#c7b692", "#c6a79d"),
  },
};
const SKIN_ORDER = ["dark", "studio", "seventies", "light"];
const SKIN_NAMES = { dark: "DARK", studio: "STUDIO", seventies: "70s", light: "LIGHT" };
const C = {};
let SKIN = "dark";
// The skin by name, the page's background with it.
function setSkin(name) {
  SKIN = SKINS[name] ? name : "dark";
  for (const k of Object.keys(C)) delete C[k]; // not every skin has key, menuHi, tile
  Object.assign(C, SKINS[SKIN]);
  document.documentElement.style.background = C.page;
  document.body.style.background = C.page;
}
// The skin chosen, kept in this browser under store.
function chooseSkin(name, store) {
  setSkin(name);
  try { localStorage.setItem(store, SKIN); } catch (e) { /* this browser only */ }
}
// The skin kept under store, else dark.
function savedSkin(store) {
  try { return localStorage.getItem(store) || "dark"; } catch (e) { return "dark"; }
}
// A color drawn on a key: the skin's key colors, where it has them.
function onKey(c) {
  const K = C.key;
  if (!K) return c;
  return c === C.text ? K.text : c === C.dim ? K.dim : c === C.faint ? K.faint : c === C.amber ? K.amber : c === C.red ? K.red : c === C.green ? K.green : c;
}



// ---------- drawing ----------
// The LEDs, as Dexter draws them: each drawn once per skin, size and scale into a small image,
// stamped by the meters. A lit one is a hot core going out to the color, with bloom; an unlit
// one a tinted lens with a glint.
const LED = { key: "", cache: new Map() };
const BLOOM_MARGIN = 5;
function rgb(hex) { const n = parseInt(hex.slice(1, 7), 16); return [n >> 16, (n >> 8) & 255, n & 255]; }
function css([r, g, b], a) { return "rgba(" + Math.round(r) + "," + Math.round(g) + "," + Math.round(b) + "," + (a === undefined ? 1 : a) + ")"; }
function mix(a, b, t) { return a.map((v, i) => v + (b[i] - v) * t); }
function vivid(c) {
  const [r, g, b] = c.map((v) => v / 255), max = Math.max(r, g, b), min = Math.min(r, g, b);
  const l = (max + min) / 2, d = max - min;
  if (d === 0) return c;
  const hue = max === r ? ((g - b) / d + 6) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  const L = l * 0.9, C2 = 1 - Math.abs(2 * L - 1), X = C2 * (1 - Math.abs((hue % 2) - 1)), m = L - C2 / 2;
  const [r1, g1, b1] = [[C2, X, 0], [X, C2, 0], [0, C2, X], [0, X, C2], [X, 0, C2], [C2, 0, X]][Math.floor(hue) % 6];
  return [r1 + m, g1 + m, b1 + m].map((v) => v * 255);
}
function led(kind, color, x, y, w, h) {
  const px = scale * (window.devicePixelRatio || 1);
  const key = SKIN + "|" + px;
  if (LED.key !== key) { LED.key = key; LED.cache.clear(); }
  const id = kind + color + Math.round(w * 10) + "x" + Math.round(h * 10);
  let img = LED.cache.get(id);
  if (!img) { img = ledImage(kind, rgb(color), w, h, px); LED.cache.set(id, img); }
  const m = kind === "bloom" ? BLOOM_MARGIN : 0;
  ctx.drawImage(img, x - m, y - m, w + 2 * m, h + 2 * m);
}
function ledImage(kind, c, w, h, px) {
  const m = kind === "bloom" ? BLOOM_MARGIN : 0;
  const cv = document.createElement("canvas");
  cv.width = Math.max(1, Math.ceil((w + 2 * m) * px)); cv.height = Math.max(1, Math.ceil((h + 2 * m) * px));
  const g = cv.getContext("2d");
  const Wd = w * px, Ht = h * px, M = m * px, R = Math.min(1.5 * px, Ht / 3);
  const shape = (gx, x0, y0) => { gx.beginPath(); gx.roundRect(x0, y0, Wd, Ht, R); };
  if (kind === "bloom") {
    const off = cv.width + 10;
    g.shadowColor = css(vivid(c), C.bloom === undefined ? 0.6 : C.bloom);
    g.shadowBlur = 3.5 * px;
    g.shadowOffsetX = off;
    shape(g, M - off, M); g.fillStyle = "#000"; g.fill();
  } else if (kind === "lit") {
    const edge = vivid(c), core = mix(c, [255, 255, 255], C.core === undefined ? 0.6 : C.core);
    g.save();
    shape(g, 0, 0); g.clip();
    g.translate(Wd / 2, Ht / 2); g.scale(Wd / 2, Ht / 2);
    const grad = g.createRadialGradient(0, 0, 0, 0, 0, 1);
    grad.addColorStop(0, css(core)); grad.addColorStop(0.45, css(mix(core, c, 0.6)));
    grad.addColorStop(0.85, css(c)); grad.addColorStop(1, css(edge));
    g.fillStyle = grad; g.fillRect(-1.5, -1.5, 3, 3);
    g.restore();
  } else {
    const grad = g.createLinearGradient(0, 0, 0, Ht);
    grad.addColorStop(0, css(mix(c, [255, 255, 255], 0.08))); grad.addColorStop(1, css(mix(c, [0, 0, 0], 0.18)));
    shape(g, 0, 0); g.fillStyle = grad; g.fill();
    g.fillStyle = "rgba(255,255,255,0.14)"; g.fillRect(R, Math.max(1, px * 0.8), Wd - 2 * R, Math.max(1, px * 0.8));
  }
  return cv;
}

function rrect(r, radius, fill, stroke) {
  ctx.beginPath();
  ctx.roundRect(r.x + 0.5, r.y + 0.5, r.w - 1, r.h - 1, radius);
  if (fill) { ctx.fillStyle = fill; ctx.fill(); }
  if (stroke) { ctx.strokeStyle = stroke; ctx.lineWidth = 1; ctx.stroke(); }
}
function text(str, x, y, font, color, align) {
  ctx.font = font; ctx.fillStyle = color; ctx.textAlign = align || "left"; ctx.textBaseline = "middle";
  ctx.fillText(str, x, y);
}
// str cut with … to fit w.
function fit(str, w, font) {
  ctx.font = font;
  if (ctx.measureText(str).width <= w) return str;
  let s = str;
  while (s.length > 1 && ctx.measureText(s + "…").width > w) s = s.slice(0, -1);
  return s + "…";
}
function lamp(cx, cy, r, color) {
  if (color) { ctx.beginPath(); ctx.arc(cx, cy, r + 3, 0, 7); ctx.fillStyle = color + "33"; ctx.fill(); }
  ctx.beginPath(); ctx.arc(cx, cy, r, 0, 7); ctx.fillStyle = color || C.lampOff; ctx.fill();
}

// A tappable region, the last added on top.
function hit(rect, id) { HITS.push({ rect, id }); }
// The section the keys drawn next fall in, for the bar's levels; null for none.
let KIT_SECTION = null;
function kitSection(name) { KIT_SECTION = name; }
// What the bar calls a key: its label without what's only drawn (arrows, "(Enter)").
function kitBarName(label) { return label.replace(/\(.*?\)/g, "").replace(/[^\p{L}\p{N} ]/gu, "").trim().replace(/\s+/g, " ").toLowerCase(); }
// A key: lit (a color: filled with it), or plain. o.name is what the bar calls it, for a key whose
// label isn't a word; o.bar false leaves it out of the bar. A key disabled still lands in HITS,
// empty, so the bar shows it dimmed; a pointer can't reach it.
function key(r, label, id, o) {
  const opt = o || {};
  const lit = opt.lit;
  rrect(r, 3, lit || C.btn, lit || C.btnEdge);
  const color = lit ? C.onRed : onKey(opt.disabled ? C.faint : opt.color || C.text);
  text(label, r.x + r.w / 2, r.y + r.h / 2 + 0.5, opt.font || "700 10px sans-serif", color, "center");
  if (id) {
    hit(opt.disabled ? { x: r.x, y: r.y, w: 0, h: 0 } : r, id);
    Object.assign(HITS[HITS.length - 1], { name: opt.name || kitBarName(label), section: KIT_SECTION, disabled: !!opt.disabled, bar: opt.bar !== false });
  }
}
// `?`: a tool's keys (its keys.json) for the screens named, in that order, in the box r.
// The page closes it; a click or Esc is the usual way.
function kitKeysOver(list, screens, r) {
  const font = "700 11px sans-serif", col = (r.w - 32) / 3, line = 18;
  ctx.fillStyle = C.shade; ctx.fillRect(0, 0, ctx.canvas.width, ctx.canvas.height);
  rrect(r, 6, C.overlay, C.rule);
  text("THE KEYS", r.x + 16, r.y + 20, "800 13px sans-serif", C.text);
  text("Esc or ? closes", r.x + r.w - 16, r.y + 20, "700 10px sans-serif", C.faint, "right");
  let x = r.x + 16, y = r.y + 50;
  const next = () => { y += line; if (y > r.y + r.h - 16) { y = r.y + 50; x += col; } };
  for (const sc of screens) {
    const here = list.filter((e) => e.screen === sc);
    if (!here.length) continue;
    text(sc.toUpperCase(), x, y, "800 10px sans-serif", C.amber);
    next();
    for (const e of here) {
      text(keyName(e.key), x, y, "700 11px monospace", C.text);
      text(fit(e.does + (e.note ? " · " + e.note : ""), col - 62, font), x + 48, y, font, C.dim);
      next();
    }
    next();
  }
}
// ---------- the command bar ----------
// STYLE.md, "The command bar": vi's command line, along the footer, in place of a pop-up. It takes
// every key while it's open. The page calls kitBar.key first thing in its key handler, kitBar.tap
// for a "bar:" hit, kitBar.letGo for a click anywhere else, and kitBar.draw last.
//
// An option is { name, faint (dimmed: refused as a mouse would be), act (a function), level (the
// next level's options, or a function giving them), ask (words: asked first, y does it) }.
const KIT_NOT_KEYS = /^(Shift|Control|Alt|AltGraph|Meta|CapsLock|NumLock|ScrollLock|Dead|Unidentified)$/;
const kitBar = {
  s: null,
  isOpen() { return this.s !== null; },
  use() { return this.s && this.s.use; },
  close() { this.s = null; },
  // One key: y does it, anything else lets it go.
  ask(said, yes) { this.s = { use: "ask", said, yes }; },
  // The very next key decides, with no timer: `letter` takes it up, Esc only closes it, and any
  // other key goes on and does what it does. `said` may be a function, for what's still coming.
  offer(said, letter, word, take) { this.s = { use: "offer", said, letter, word, take }; },
  // A line typed. check(value) is what's wrong with it, or null; while wrong the bar stays open.
  // It never closes by itself.
  type(said, value, done, check) { this.s = { use: "type", said, value, done, check, wrong: null }; },
  // A choice or a command: type to narrow, Tab the next, Enter does it, Esc up a level.
  choose(options) { this.s = { use: "choose", levels: [options], path: [], typed: "", sel: -1, wrong: null, opened: performance.now() }; },
  // The screen's levels, as the keys drawn last time: a section is a level and its keys the options
  // in it; a key in no section is an option at the top. `extra`: levels of the tool's own, after.
  // Called between draws, so HITS is the screen as it stands.
  screen(tap, extra) {
    const top = [], levels = new Map();
    for (const h of HITS) {
      if (!h.bar || !h.name) continue;
      const o = { name: h.name, faint: h.disabled, act: () => tap(h.id) };
      if (!h.section) { top.push(o); continue; }
      if (!levels.has(h.section)) { const level = []; levels.set(h.section, level); top.push({ name: h.section, level }); }
      levels.get(h.section).push(o);
    }
    return top.concat(extra || []);
  },
  matches() {
    const b = this.s, level = b.levels[b.levels.length - 1], t = b.typed.toLowerCase();
    return t ? level.filter((o) => o.name.startsWith(t)) : level;
  },
  take(o) {
    const b = this.s;
    if (o.faint) { b.wrong = o.name + ": not now"; return; }
    if (o.level) {
      b.levels.push(typeof o.level === "function" ? o.level() : o.level);
      b.path.push(o.name); b.typed = ""; b.sel = -1; b.wrong = null; b.opened = performance.now();
      return;
    }
    if (o.ask) { this.ask(o.ask, o.act); return; }
    this.s = null;
    o.act();
  },
  enter() {
    const b = this.s;
    if (b.use === "type") {
      const why = b.check ? b.check(b.value) : null;
      if (why) { b.wrong = why; return; }
      this.s = null;
      b.done(b.value);
      return;
    }
    const m = this.matches();
    if (b.typed && !m.length) { b.wrong = "no " + b.typed + " here"; return; }
    if (b.typed || b.sel >= 0) this.take(m[Math.max(0, b.sel)]);
  },
  escape() {
    const b = this.s;
    if (b.use !== "choose" || !b.path.length) { this.s = null; return; }
    b.levels.pop(); b.path.pop(); b.typed = ""; b.sel = -1; b.wrong = null; b.opened = performance.now();
  },
  // True when the bar took the key: only an offer lets one go on.
  key(e) {
    const b = this.s, k = e.key;
    if (!b) return false;
    if (KIT_NOT_KEYS.test(k)) return true;
    // A held key's repeat isn't the next key: it would close what it just opened.
    if (e.repeat && (b.use === "ask" || b.use === "offer")) return true;
    if (b.use === "ask") {
      this.s = null;
      if (k === "y") b.yes();
      return true;
    }
    if (b.use === "offer") {
      this.s = null;
      if (k === b.letter) { b.take(); return true; }
      return k === "Escape";
    }
    if (k === "Escape") { this.escape(); return true; }
    if (k === "Enter") { this.enter(); return true; }
    if (b.use === "type") {
      if (k === "Backspace") b.value = b.value.slice(0, -1);
      else if (k.length === 1) b.value += k;
      b.wrong = null;
      return true;
    }
    if (k === "Tab") {
      const n = this.matches().length;
      if (n) b.sel = b.sel < 0 ? (e.shiftKey ? n - 1 : 0) : (b.sel + (e.shiftKey ? n - 1 : 1)) % n;
      return true;
    }
    // Backspace on an empty bar does nothing, so a held one can't run on into the tool's.
    if (k === "Backspace") b.typed = b.typed.slice(0, -1);
    else if (k.length === 1) b.typed += k;
    else return true;
    b.sel = -1; b.wrong = null;
    return true;
  },
  // A click on the bar: true. For "bar:" hits. An option is ignored just after a level opens, so
  // the second half of a double-click doesn't land on what's appeared under the pointer.
  tap(id) {
    const b = this.s, what = id.slice(4);
    if (!b) return;
    if (what === "yes") { this.s = null; b.yes(); }
    else if (what === "offer") { this.s = null; b.take(); }
    else if (what === "done") this.enter();
    else if (what === "esc") this.escape();
    else if (what.startsWith("opt:") && performance.now() - b.opened >= 300) this.take(b.levels[b.levels.length - 1][+what.slice(4)]);
  },
  // A click anywhere else: a line being typed stays; anything else is let go.
  letGo() { if (this.s && this.s.use !== "type") this.s = null; },
  // Along r. `idle`: what's said while it's closed.
  draw(r, idle) {
    const b = this.s, font = "700 13px monospace", y = r.y + r.h / 2 + 0.5, end = r.x + r.w - 10;
    rrect(r, 3, C.well, b ? C.amber : C.rule);
    if (!b) { if (idle) text(idle, r.x + 10, y, "700 11px sans-serif", C.faint); return; }
    let x = r.x + 10;
    const width = (str) => { ctx.font = font; return ctx.measureText(str).width; };
    const say = (str, color, id) => {
      const w = width(str);
      text(str, x, y, font, color);
      if (id) hit({ x: x - 5, y: r.y, w: w + 10, h: r.h }, id);
      x += w;
    };
    const said = typeof b.said === "function" ? b.said() : b.said;
    if (b.use === "ask") { say(said + "  ", C.text); say("y", C.amber, "bar:yes"); say("  anything else lets it go", C.faint); return; }
    if (b.use === "offer") { say(said + "  ", C.text); say(b.letter + " " + b.word, C.dim, "bar:offer"); return; }
    if (b.use === "type") {
      say(said + "  ", C.dim); say(b.value + "▌", C.readout);
      if (b.wrong) say("   " + b.wrong, C.red);
      key({ x: r.x + r.w - 142, y: r.y + 2, w: 70, h: r.h - 4 }, "DONE", "bar:done", { bar: false });
      key({ x: r.x + r.w - 68, y: r.y + 2, w: 64, h: r.h - 4 }, "CANCEL", "bar:esc", { bar: false });
      return;
    }
    say(":" + b.path.join(" ") + (b.path.length ? " " : ""), C.amber);
    if (b.typed) {
      const m = this.matches()[Math.max(0, b.sel)];
      say(b.typed + "▌", C.readout);
      if (m) say(m.name.slice(b.typed.length), C.dim);
      if (b.wrong) say("   " + b.wrong, C.red);
      return;
    }
    // Greyed, and clickable; scrolled so the one Tab's on shows.
    const level = b.levels[b.levels.length - 1], gap = 18;
    if (!b.path.length) x += 8;
    let first = 0;
    const upTo = (a) => level.slice(a, b.sel + 1).reduce((t, o) => t + width(o.name) + gap, 0);
    while (b.sel >= 0 && first < b.sel && x + upTo(first) > end - 20) first++;
    for (let i = first; i < level.length; i++) {
      if (x + width(level[i].name) > end - 20) { say("…", C.faint); break; }
      say(level[i].name, i === b.sel ? C.text : level[i].faint ? C.faint : C.dim, "bar:opt:" + i);
      x += gap;
    }
    if (b.wrong) { x += 4; say(b.wrong, C.red); }
  },
};

// A ladder of 16 LEDs, bottom up.
function ladder(r, m) {
  const gap = 2, h = (r.h - 15 * gap) / 16;
  const on = (k) => m && (k < m.lit || k === m.peak);
  const at = (k) => r.y + r.h - (k + 1) * h - k * gap;
  if (C.glowLeds) {
    for (let k = 0; k < 16; k++) if (!on(k)) led("lens", C.unlit[k], r.x, at(k), r.w, h);
    for (let k = 0; k < 16; k++) if (on(k)) led("bloom", C.lit[k], r.x, at(k), r.w, h);
    for (let k = 0; k < 16; k++) if (on(k)) led("lit", C.lit[k], r.x, at(k), r.w, h);
  } else {
    for (let k = 0; k < 16; k++) {
      const y = at(k);
      if (on(k)) { ctx.fillStyle = C.lit[k] + "40"; ctx.fillRect(r.x - 2, y - 1, r.w + 4, h + 2); }
      ctx.fillStyle = on(k) ? C.lit[k] : C.unlit[k];
      ctx.fillRect(r.x, y, r.w, h);
    }
  }
}
function label(str, x, y, color, align) { text(str, x, y, "700 9px sans-serif", color || C.faint, align); }
// A readout: a well with a number in it.
function readout(r, str, color) {
  rrect(r, 2, C.well, C.rule);
  text(str, r.x + r.w - 8, r.y + r.h / 2 + 0.5, "700 13px monospace", color || C.readout, "right");
}

