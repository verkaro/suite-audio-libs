# Spec: `as-control`, the controls as data

Moved here from `specs/shared.md` when the crate was made (step 1, 2026-11-12). The rules every
unit keeps are still there. "As built", at the end, says what the builder chose and where it
differs.

`RFC-eq.md` asked for every control to be described as data, so a panel's keys, a typed value, a
page and later a FaderPort encoder all step alike, and left open where the description lives. **A
crate of its own, `as-control`,** because `as-eq` and `as-dynamics` both need it and the suite keeps
one of each thing; a module in one of them would make the other depend on it for no audio reason.
sonofader2's desks will read the same descriptions, so it has a third user coming.

It holds no audio and no unit's controls: each unit crate declares its own with it.

### A control

```rust
pub struct Control {
    pub id: &'static str,       // never changes: settings files and protocols use it
    pub short: &'static str,    // a panel's label: "FREQ", "GAIN", "Q", "BELL", "IN"
    pub long: &'static str,     // for `?` and a page's title text
    pub kind: Kind,
    pub default: f64,
    pub detent: f64,            // where "back to the middle" goes
}

pub enum Kind {
    /// A value on a line or a log scale.
    Value { lo: f64, hi: f64, scale: Scale, fine: f64, coarse: f64, unit: Unit },
    /// Named positions; the value is the position's index.
    Switch { positions: &'static [&'static str] },
}

pub enum Scale { Linear, Log }   // Log steps in octaves of the value
pub enum Unit { Hz, Db, Q, Ratio, Plain, Dial(DialLaw), /* … as a unit needs */ }
```

The shape is a sketch: the names and fields are the builder's, in the suite's plain voice. What it
must do is below.

**Values are `f64` everywhere,** a switch's being its position index. A unit's `Settings` gives
`get(id) -> Option<f64>` and `set_by_id(id, v)` (held to range), so a panel, a snapshot file and a
protocol work from ids alone. A unit lists its controls as `pub fn controls() -> &'static [Control]`
(per voicing, later). Ids are local to the unit (`lf.freq`, `thr`); a tool puts its unit's name in
front (`eq.lf.freq`) in its own files.

### Stepping

- **The grid is anchored at the detent.** A linear value's grid is `detent + k·fine`; a log value's
  is `detent · 2^(k·fine)` (fine in octaves). `k` is an integer, and a stepped value is always
  computed from `k`, never by adding a step to the last value, so up n and down n land exactly back.
- **Coarse is a whole number of fine steps,** on the same grid. Every control below keeps this.
- **From a value off the grid** (typed, or clamped), a step up goes to the next grid point above,
  a step down to the next below.
- **At the ends,** a step that would pass the range lands on the end itself. A step back from the
  end goes to the grid point below it.
- **To the detent:** one call.
- **Switches** step one position, and stop at the ends (no wrap).

`RFC-eq.md` anchored a frequency's grid at the range's low end. **Differs from the RFC:** anchored at
the detent instead, so a band's default (100 Hz, 400 Hz, 3 kHz, 10 kHz) is on its own grid, and the
first step from a fresh rack lands on a clean value. The low end isn't on any grid of round values
anyway.

### Format and parse

One pair of functions both ways, per unit kind:

| Unit | Shows | Reads |
|---|---|---|
| Hz | `80 Hz`, `450 Hz`, `1.20 kHz`, `16.0 kHz` (three figures) | `1200`, `1.2k`, `1k2`, `1.2 kHz`, `1200hz`, case and spaces ignored |
| dB | `+3.0 dB`, `−4.5 dB`, `0.0 dB` | `3`, `+3`, `3dB`, `-4.5`, `−4.5` (the real minus too) |
| Q | `Q 1.40` | `1.4`, `Q1.4`, `q 1.4` |
| Ratio | `4:1`, and a switch's own names (`ALL`, `2A`) | `4`, `4:1`, the names |
| Dial | `3 (234 µs)` | `3` sets the dial; `330us`, `0.33ms`, `330 µs` set the time and land on the nearest fine step of the dial |
| Switch | the position's name | the name, case ignored, or a unique prefix |

A value typed out of range is held to it and shown held, never refused. Text that can't be read
is the one refusal: `parse` returns `None` and the panel says so.

### Tests

- For every control of every unit (each unit's test runs this over its own list): from the detent,
  n steps up then n down returns exactly, for every n that stays inside the range; coarse the same.
- `format` then `parse` returns the value, for every grid point of every value control and every
  switch position.
- The forgiving inputs above each parse as said.
- Out of range is held; nonsense is `None`.
- Each coarse step is a whole number of fine steps (a test over every control's description, so a
  new control can't break it).

## As built (2026-11-12)

- **The types** are the sketch's: `Control`, `Kind::{Value, Switch}`, `Scale`, `Unit::{Hz, Db, Q,
  Ratio, Plain, Dial(Law)}`. A `Law` is two plain functions, the dial to what it sets and back,
  and what it shows (`Shows::Seconds` or `Shows::Dbfs`). The optical's Reduce is a dial in dBFS:
  `40 (−18.9 dBFS)`. A ratio switch is a `Switch` with `ratios: true`: a position named by a number
  shows as `4:1`; the others (`2A`, `ALL`) as named.
- **The calls:** `clamp`, `step(v, n, Size::Fine | Coarse)`, `grid(k)`, `format`, `parse`, and
  `find(list, id)`. The detent is the field; "to the detent" is `c.detent`.
- **`check(list)`** runs every rule above over a unit's list (the steps and back, each coarse a
  whole number of fine, format and parse both ways at every grid point and both ends, every
  switch position, unique ids, defaults and detents in range). Each unit's tests call it on its
  own `controls()`.
- **A grid point is computed as `k/n`** when a step is `1/n` of a unit or an octave, so whole
  units land exactly: 400 Hz up twelve fine steps is 800 to the bit, dial 3 up four is 3.4.
- **Format then parse.** Three figures can't hold a grid point exactly (1,198.6 Hz shows as
  `1.20 kHz`), and the spec asks both that `1200` read as 1,200 Hz and that what's shown read
  back to its grid point. So: **text exactly as `format` shows a grid point either side of the
  value read (spaces, case and the minus aside) is that grid point; any other number is the
  number.** `1.20 kHz` reads 1,198.6; `1200` and `1.2k` read 1,200. A dial's shown form reads back
  with its bracket (`2.4 (338 µs)`).
- **Showing:** Hz under 1 kHz drops trailing zeros (`80 Hz`, `75.5 Hz`), as the table's `80 Hz`
  has it; kHz keeps three figures (`1.20 kHz`). A dial and a plain value show the decimals the
  fine step needs, trailing zeros dropped (`3`, `3.4`). Minus is the real one, U+2212, read as a
  hyphen too; a panel's font without it goes through `Pen::first_of`.
- **Not a number** given to `clamp` (so to `step` and `format`) is the default. `inf` and `nan`
  typed are refused, as text that can't be read.
- **A switch's coarse step** is one position, as fine.
