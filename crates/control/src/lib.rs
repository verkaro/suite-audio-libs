//! A unit's controls as data (step 1 of `specs/shared.md`, 2026-11-12; the spec is
//! `docs/spec.md`). `RFC-eq.md` asked for every control to be described once, so a panel's keys,
//! a typed value, a page and later a FaderPort's encoder all step the same way and read the same.
//! A crate of its own because `as-eq` and `as-dynamics` both need it, and sonofader2's desks will
//! read the same descriptions.
//!
//! It holds no audio and no unit's controls: each unit declares its own list with it, as
//! `pub fn controls() -> &'static [Control]`, and its tests run [`check`] over that list.
//!
//! Values are `f64` everywhere; a switch's value is its position's index. Stepping lands on a grid
//! anchored at the detent and is always computed from a whole number of steps, never by adding a
//! step to the last value, so up n and down n land exactly back. A value out of range is held to
//! it, never refused; text that can't be read is the one refusal.
//!
//! Written here, not moved.

#[cfg(test)]
mod tests;

/// One control of a unit.
#[derive(Clone, Copy, Debug)]
pub struct Control {
    /// Never changes: settings files and protocols use it. Local to the unit (`lf.freq`, `thr`).
    pub id: &'static str,
    /// A panel's label: `FREQ`, `GAIN`, `Q`, `BELL`, `IN`.
    pub short: &'static str,
    /// For `?` and a page's title text: "Low-mid frequency".
    pub long: &'static str,
    pub kind: Kind,
    pub default: f64,
    /// Where "back to the middle" goes. A value's grid is anchored here.
    pub detent: f64,
}

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    /// A value on a line or a log scale. `fine` and `coarse` are in the value's own units on a
    /// line, in octaves of the value on a log scale; `coarse` is a whole number of `fine`.
    Value { lo: f64, hi: f64, scale: Scale, fine: f64, coarse: f64, unit: Unit },
    /// Named positions; the value is the position's index. With `ratios`, a position named by a
    /// number is shown as a ratio (`4` as `4:1`), and read either way.
    Switch { positions: &'static [&'static str], ratios: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    Linear,
    /// Steps in octaves of the value.
    Log,
}

/// How a value is shown and read.
#[derive(Clone, Copy, Debug)]
pub enum Unit {
    /// `80 Hz`, `1.20 kHz`, three figures.
    Hz,
    /// `+3.0 dB`, `−4.5 dB`, `0.0 dB`.
    Db,
    /// `Q 1.40`.
    Q,
    /// `4:1`, `2.5:1`.
    Ratio,
    /// A plain number, as many decimals as the fine step needs.
    Plain,
    /// A dial, shown with what it means: `3 (234 µs)`, `40 (−18.9 dBFS)`.
    Dial(Law),
}

/// A dial's law: what a position of the dial means, and back.
#[derive(Clone, Copy, Debug)]
pub struct Law {
    pub shows: Shows,
    /// The dial's position to the quantity it sets.
    pub value: fn(f64) -> f64,
    /// The quantity back to the dial's position.
    pub dial: fn(f64) -> f64,
}

/// The quantity a dial sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shows {
    /// A time, in seconds: shown in µs, ms or s; read from `330us`, `0.33ms`, `330 µs`, `1.1s`.
    Seconds,
    /// A level, in dBFS: shown `−18.9 dBFS`; read from `-18.9dbfs`.
    Dbfs,
}

/// A step's size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    Fine,
    Coarse,
}

/// How near a whole number of steps a value must sit to count as on the grid.
const ON_GRID: f64 = 1e-6;

/// The control with this id in a list.
pub fn find<'a>(list: &'a [Control], id: &str) -> Option<&'a Control> {
    list.iter().find(|c| c.id == id)
}

impl Control {
    /// `v` held to the control's range. A switch's is rounded to a position. Not a number gives
    /// the default.
    pub fn clamp(&self, v: f64) -> f64 {
        if v.is_nan() {
            return self.default;
        }
        match self.kind {
            Kind::Value { lo, hi, .. } => v.clamp(lo, hi),
            Kind::Switch { positions, .. } => v.round().clamp(0.0, (positions.len() - 1) as f64),
        }
    }

    /// `v` stepped `n` times (up if positive, down if negative). A step from a value off the grid
    /// goes to the next grid point that way; a step past an end lands on the end. A switch moves
    /// one position a step, either size, and stops at its ends.
    pub fn step(&self, v: f64, n: i32, size: Size) -> f64 {
        let mut v = self.clamp(v);
        for _ in 0..n.unsigned_abs() {
            v = self.step_once(v, n > 0, size);
        }
        v
    }

    fn step_once(&self, v: f64, up: bool, size: Size) -> f64 {
        match self.kind {
            Kind::Switch { .. } => self.clamp(if up { v + 1.0 } else { v - 1.0 }),
            Kind::Value { .. } => {
                let m = match size {
                    Size::Fine => 1.0,
                    Size::Coarse => self.coarse_steps(),
                };
                let k = self.k_of(v);
                let r = k.round();
                let on = (k - r).abs() < ON_GRID;
                let next = if up {
                    (if on { r } else { k.floor() }) + m
                } else {
                    (if on { r } else { k.ceil() }) - m
                };
                self.clamp(self.grid(next))
            }
        }
    }

    /// The value `k` fine steps from the detent, not held to the range. A switch's is `k` itself.
    pub fn grid(&self, k: f64) -> f64 {
        match self.kind {
            Kind::Switch { .. } => k,
            Kind::Value { scale, fine, .. } => {
                let d = match per(fine) {
                    Some(n) => k / n,
                    None => k * fine,
                };
                match scale {
                    Scale::Linear => self.detent + d,
                    Scale::Log => self.detent * libm::pow(2.0, d),
                }
            }
        }
    }

    /// How many fine steps from the detent `v` sits: a whole number if it's on the grid.
    fn k_of(&self, v: f64) -> f64 {
        match self.kind {
            Kind::Switch { .. } => v,
            Kind::Value { scale, fine, .. } => {
                let d = match scale {
                    Scale::Linear => v - self.detent,
                    Scale::Log => libm::log2(v / self.detent),
                };
                match per(fine) {
                    Some(n) => d * n,
                    None => d / fine,
                }
            }
        }
    }

    /// Fine steps in a coarse one: 1 for a switch.
    fn coarse_steps(&self) -> f64 {
        match self.kind {
            Kind::Value { fine, coarse, .. } => (coarse / fine).round(),
            Kind::Switch { .. } => 1.0,
        }
    }

    /// `v` as text, after holding it to the range.
    pub fn format(&self, v: f64) -> String {
        let v = self.clamp(v);
        match self.kind {
            Kind::Switch { positions, ratios } => show_position(positions[v as usize], ratios),
            Kind::Value { fine, unit, .. } => match unit {
                Unit::Hz => hz(v),
                Unit::Db => signed(v, "dB"),
                Unit::Q => format!("Q {v:.2}"),
                Unit::Ratio => format!("{}:1", trimmed(v, 2)),
                Unit::Plain => trimmed(v, decimals(fine)),
                Unit::Dial(law) => {
                    let x = (law.value)(v);
                    let means = match law.shows {
                        Shows::Seconds => seconds(x),
                        Shows::Dbfs => signed(x, "dBFS"),
                    };
                    format!("{} ({means})", trimmed(v, decimals(fine)))
                }
            },
        }
    }

    /// Text to a value, held to the range. Forgiving of case, spaces, a `+`, and the real minus.
    /// A number typed is that number; text exactly as `format` shows a grid point is that grid
    /// point, so what's shown reads back to the value it came from (`1.20 kHz` to the LMF's
    /// point near 1,199 Hz, `1200` to 1,200 Hz). `None` if the text can't be read.
    pub fn parse(&self, text: &str) -> Option<f64> {
        let t = normal(text);
        if t.is_empty() {
            return None;
        }
        match self.kind {
            Kind::Switch { positions, ratios } => read_position(positions, ratios, &t),
            Kind::Value { unit, .. } => {
                // A dial's shown form carries its meaning after it, in brackets.
                let bare = match unit {
                    Unit::Dial(_) => t.split_once('(').map_or(t.as_str(), |(a, _)| a),
                    _ => t.as_str(),
                };
                if let Unit::Dial(law) = unit
                    && let Some(x) = read_meaning(law.shows, bare)
                {
                    // The quantity typed: the nearest fine step of the dial.
                    let d = (law.dial)(x);
                    if !d.is_finite() {
                        return None;
                    }
                    return Some(self.clamp(self.grid(self.k_of(self.clamp(d)).round())));
                }
                let v = self.clamp(read_value(unit, bare)?);
                Some(self.snap(v, &t))
            }
        }
    }

    /// The grid point either side of `v` that shows as `typed`, or `v` if neither does.
    fn snap(&self, v: f64, typed: &str) -> f64 {
        let Kind::Value { lo, hi, .. } = self.kind else {
            return v;
        };
        let k = self.k_of(v);
        [k.floor(), k.ceil()]
            .into_iter()
            .map(|c| self.grid(c))
            .find(|&g| (lo..=hi).contains(&g) && normal(&self.format(g)) == typed)
            .unwrap_or(v)
    }
}

/// Text as it's compared: no spaces, the real minus a hyphen, lower case.
fn normal(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| if c == '\u{2212}' { '-' } else { c })
        .collect::<String>()
        .to_lowercase()
}

/// `n` if a step is `1/n` of a unit (or an octave) for a whole `n`, so a grid point is `k/n`, and
/// a whole number of units lands exactly on it.
fn per(fine: f64) -> Option<f64> {
    let n = (1.0 / fine).round();
    (n >= 1.0 && (1.0 / fine - n).abs() < 1e-9).then_some(n)
}

/// Decimals a step needs to be shown: 0.5 needs one, 1 none.
fn decimals(fine: f64) -> usize {
    (0..=6)
        .find(|&d| {
            let s = fine * libm::pow(10.0, d as f64);
            (s - s.round()).abs() < 1e-9
        })
        .unwrap_or(6)
}

/// `v` with at most `d` decimals, the trailing zeros dropped: `3`, `3.4`, `2.5`.
fn trimmed(v: f64, d: usize) -> String {
    let s = format!("{v:.d$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    minus(if s == "-0" { "0".into() } else { s })
}

/// The real minus for a hyphen.
fn minus(s: String) -> String {
    s.replace('-', "\u{2212}")
}

/// `+3.0 dB`, `−4.5 dB`, `0.0 dB`: one decimal, a sign unless it shows as zero.
fn signed(v: f64, unit: &str) -> String {
    let s = format!("{:.1}", v.abs());
    if s == "0.0" {
        format!("0.0 {unit}")
    } else if v > 0.0 {
        format!("+{s} {unit}")
    } else {
        format!("\u{2212}{s} {unit}")
    }
}

/// `x > 0` to three significant figures: `234`, `20.0`, `1.10`, `0.400`.
fn sig3(x: f64) -> String {
    let e = libm::floor(libm::log10(x)) as i32;
    let d = (2 - e).max(0) as usize;
    let s = format!("{x:.d$}");
    // Rounding up into the next decade (999.6 to "1000") takes one figure fewer.
    let next = libm::pow(10.0, (e + 1) as f64);
    if s.parse::<f64>().unwrap_or(0.0) >= next && d > 0 { format!("{x:.*}", d - 1) } else { s }
}

/// Three figures, in Hz below 1 kHz (`80 Hz`, `75.5 Hz`, `450 Hz`) and kHz above (`1.20 kHz`,
/// `16.0 kHz`).
fn hz(v: f64) -> String {
    if v <= 0.0 {
        return "0 Hz".into();
    }
    let s = sig3(v);
    if s.parse::<f64>().unwrap_or(0.0) >= 1000.0 {
        format!("{} kHz", sig3(v / 1000.0))
    } else {
        let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
        format!("{s} Hz")
    }
}

/// A time in seconds, three figures: `20.0 µs`, `234 µs`, `50.0 ms`, `1.10 s`.
fn seconds(t: f64) -> String {
    if t <= 0.0 {
        return "0 s".into();
    }
    for (scale, unit) in [(1e6, "µs"), (1e3, "ms")] {
        let s = sig3(t * scale);
        if s.parse::<f64>().unwrap_or(0.0) < 1000.0 {
            return format!("{s} {unit}");
        }
    }
    format!("{} s", sig3(t))
}

fn show_position(name: &str, ratios: bool) -> String {
    if ratios && name.parse::<f64>().is_ok() { format!("{name}:1") } else { name.to_string() }
}

/// A position by its name or what it shows (case ignored), else by a unique prefix of a name.
fn read_position(positions: &[&str], ratios: bool, t: &str) -> Option<f64> {
    let lower = |s: &str| s.to_lowercase();
    if let Some(i) = positions.iter().position(|p| lower(p) == t || lower(&show_position(p, ratios)) == t) {
        return Some(i as f64);
    }
    let mut found = positions.iter().enumerate().filter(|(_, p)| lower(p).starts_with(t));
    match (found.next(), found.next()) {
        (Some((i, _)), None) => Some(i as f64),
        _ => None,
    }
}

/// A plain number, finite.
fn number(t: &str) -> Option<f64> {
    if t.is_empty() || t.chars().any(|c| c.is_ascii_alphabetic() && c != 'e') {
        return None;
    }
    t.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// What a dial means, typed with its unit (`330us`, `-18.9dbfs`). `None` for a bare number, which
/// sets the dial.
fn read_meaning(shows: Shows, t: &str) -> Option<f64> {
    match shows {
        Shows::Dbfs => number(t.strip_suffix("dbfs")?),
        Shows::Seconds => {
            for (suffix, scale) in [("us", 1e-6), ("µs", 1e-6), ("μs", 1e-6), ("ms", 1e-3)] {
                if let Some(n) = t.strip_suffix(suffix) {
                    return number(n).map(|v| v * scale);
                }
            }
            number(t.strip_suffix('s')?)
        }
    }
}

/// A value typed for its unit.
fn read_value(unit: Unit, t: &str) -> Option<f64> {
    match unit {
        Unit::Hz => {
            let t = t.strip_suffix("hz").unwrap_or(t);
            match t.split_once('k') {
                None => number(t),
                Some((a, "")) => number(a).map(|v| v * 1000.0),
                // `1k2`: the k is the point.
                Some((a, b))
                    if !a.is_empty()
                        && a.chars().all(|c| c.is_ascii_digit())
                        && b.chars().all(|c| c.is_ascii_digit()) =>
                {
                    number(&format!("{a}.{b}")).map(|v| v * 1000.0)
                }
                Some(_) => None,
            }
        }
        Unit::Db => number(t.strip_suffix("db").unwrap_or(t)),
        Unit::Q => number(t.strip_prefix('q').unwrap_or(t)),
        Unit::Ratio => number(t.strip_suffix(":1").unwrap_or(t)),
        Unit::Plain | Unit::Dial(_) => number(t),
    }
}

/// Checks a unit's list of controls: each description is sound (ranges, steps, detents, ids), and
/// every control steps and reads as `docs/spec.md` says. Each unit's tests run it over its own
/// list, so a new control can't break the rules unseen.
pub fn check(list: &[Control]) -> Result<(), String> {
    for (i, c) in list.iter().enumerate() {
        if c.id.is_empty() || c.short.is_empty() || c.long.is_empty() {
            return Err(format!("control {i}: an empty id or label"));
        }
        if list[..i].iter().any(|o| o.id == c.id) {
            return Err(format!("{}: the id twice", c.id));
        }
        check_one(c).map_err(|e| format!("{}: {e}", c.id))?;
    }
    Ok(())
}

fn check_one(c: &Control) -> Result<(), String> {
    match c.kind {
        Kind::Switch { positions, .. } => {
            let n = positions.len();
            if n < 2 {
                return Err("a switch with fewer than two positions".into());
            }
            for (name, v) in [("default", c.default), ("detent", c.detent)] {
                if v != v.round() || v < 0.0 || v >= n as f64 {
                    return Err(format!("its {name} isn't a position"));
                }
            }
            for i in 0..n {
                let v = i as f64;
                if c.parse(&c.format(v)) != Some(v) {
                    return Err(format!("position {i} doesn't read back"));
                }
                if c.step(v, 1, Size::Fine) != (v + 1.0).min((n - 1) as f64)
                    || c.step(v, -1, Size::Fine) != (v - 1.0).max(0.0)
                {
                    return Err(format!("position {i} doesn't step by one"));
                }
            }
        }
        Kind::Value { lo, hi, scale, fine, coarse, .. } => {
            if !(lo.is_finite() && hi.is_finite() && lo < hi) {
                return Err("its range is empty or not finite".into());
            }
            if scale == Scale::Log && lo <= 0.0 {
                return Err("a log scale reaching 0".into());
            }
            if !(fine > 0.0 && coarse >= fine) {
                return Err("its steps aren't positive, coarse at least fine".into());
            }
            let m = coarse / fine;
            if (m - m.round()).abs() > 1e-9 {
                return Err("coarse isn't a whole number of fine steps".into());
            }
            for (name, v) in [("default", c.default), ("detent", c.detent)] {
                if !(lo..=hi).contains(&v) {
                    return Err(format!("its {name} is out of range"));
                }
            }
            for size in [Size::Fine, Size::Coarse] {
                let m = if size == Size::Fine { 1.0 } else { m };
                for dir in [1, -1] {
                    // Every n whose last step still lands inside the range.
                    let mut n = 1;
                    while (lo..=hi).contains(&c.grid(dir as f64 * n as f64 * m)) {
                        let there = c.step(c.detent, dir * n, size);
                        if there != c.grid(dir as f64 * n as f64 * m) {
                            return Err(format!("{n} {size:?} steps don't land on the grid"));
                        }
                        if c.step(there, -dir * n, size) != c.detent {
                            return Err(format!("{n} {size:?} steps and back don't return"));
                        }
                        n += 1;
                    }
                }
            }
            let mut points = vec![lo, hi];
            let (mut k, mut dir) = (0.0, 1.0);
            loop {
                let g = c.grid(k);
                if (lo..=hi).contains(&g) {
                    points.push(g);
                    k += dir;
                } else if dir > 0.0 {
                    (k, dir) = (-1.0, -1.0);
                } else {
                    break;
                }
            }
            for v in points {
                let shown = c.format(v);
                if c.parse(&shown) != Some(v) {
                    return Err(format!("{v} shows as {shown:?}, which doesn't read back"));
                }
            }
        }
    }
    Ok(())
}
