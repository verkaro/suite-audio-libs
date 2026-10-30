"use strict";
// The Dexter family's page kit (suite-audio-libs, crates/page-kit; moved in from place's page,
// itself scape's and Dexter's, VENDORED.md): the skins, the LEDs and their ladders, keys,
// readouts, lamps and text, drawn on one canvas. A page loads it before its own script
// (<script src="/page-kit.js"></script>), gives it its canvas (kitCanvas), and draws with it;
// what's tapped lands in HITS, the last added on top, for the page to clear each draw.

let ctx = null;
const HITS = [];
function kitCanvas(canvas) { ctx = canvas.getContext("2d"); }

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
// A key: lit (a color: filled with it), or plain.
function key(r, label, id, o) {
  const opt = o || {};
  const lit = opt.lit;
  rrect(r, 3, lit || C.btn, lit || C.btnEdge);
  const color = lit ? C.onRed : onKey(opt.disabled ? C.faint : opt.color || C.text);
  text(label, r.x + r.w / 2, r.y + r.h / 2 + 0.5, opt.font || "700 10px sans-serif", color, "center");
  if (id && !opt.disabled) hit(r, id);
}
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

