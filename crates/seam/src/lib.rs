//! A LOOP's seam: what meets what where END goes back to LOOP (a [`Join`]), how alike the two
//! sides are ([`likeness`]), the crossfade's gains ([`Fade`]), and [`find`], which moves a point
//! to the best join near it. The voice plays from a `Join`, so what's measured is what's heard.
//!
//! Moved into the suite from scape's `crates/core/src/seam.rs` (`VENDORED.md`); a sound is its
//! channels, one slice each, all one length, where scape had its `Sample`.

use std::f64::consts::FRAC_PI_2;

#[cfg(test)]
mod tests;

/// A sound's length in frames: its first channel's.
fn frames(s: &[Vec<f32>]) -> usize {
    s.first().map_or(0, Vec::len)
}

/// How a LOOP goes round.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Seam {
    /// Forward, END crossfaded into the audio before LOOP (or from it, when there's none before).
    #[default]
    Fwd,
    /// Forward to END, backward to LOOP, forward again: no jump at all, the sound's direction
    /// given up.
    Alt,
    /// Butterfly: the fade's incoming side is the audio from LOOP played backward, turning at
    /// LOOP to go forward. Both sides of the turn are the same audio mirrored, so the join holds
    /// wherever LOOP is; and it needs no audio before LOOP.
    Bfly,
}

/// The crossfade's law as set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Law {
    /// Chosen from how alike the two sides are ([`likeness`]): the level holds through the fade.
    #[default]
    Auto,
    /// Equal power: for two sides that are unlike (noise: wind, rain, crickets).
    Power,
    /// Equal gain: for two sides that are alike (a drone, a hum).
    Gain,
}

/// The crossfade's gains, the law resolved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fade {
    Power,
    Gain,
    /// Holds the level of two sides correlated by r: equal power at 0, equal gain at 1.
    Matched(f64),
}

impl Fade {
    /// The law as set, resolved for sides of the given likeness.
    pub fn new(law: Law, likeness: f64) -> Fade {
        match law {
            Law::Power => Fade::Power,
            Law::Gain => Fade::Gain,
            Law::Auto => Fade::Matched(likeness.clamp(-0.5, 1.0)),
        }
    }

    /// The outgoing and incoming gains, x from 0 (all outgoing) to 1.
    #[inline]
    pub fn gains(self, x: f64) -> (f64, f64) {
        match self {
            Fade::Power => ((x * FRAC_PI_2).cos(), (x * FRAC_PI_2).sin()),
            Fade::Gain => (1.0 - x, x),
            Fade::Matched(r) => {
                // a·A + b·B has the level of A and B when a² + b² + 2r·a·b = 1.
                let (a, b) = (1.0 - x, x);
                let d = (a * a + b * b + 2.0 * r * a * b).sqrt();
                (a / d, b / d)
            }
        }
    }
}

/// Where a LOOP's seam is and what meets there, in frames of the sample.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Join {
    pub seam: Seam,
    pub start: usize,
    pub loop_at: usize,
    pub end: usize,
    /// The crossfade as played: the setting, shortened to fit (0 for ALT).
    pub xf: usize,
    /// Where the fade's incoming side starts, and whether it runs backward.
    incoming: usize,
    reverse: bool,
    /// Where the loop carries on after the fade.
    pub wrap_to: usize,
}

impl Join {
    /// The join for these points and crossfade; `None` when the loop has no room to go round.
    ///
    /// FWD fades the last `xf` before END into the `xf` before LOOP, when the sample has them
    /// (they may lie before START), and carries on from LOOP. When it hasn't, it fades into the
    /// `xf` from LOOP on and carries on from LOOP + `xf`. BFLY fades into the `xf` from LOOP on,
    /// backward, and carries on from LOOP + 1, the turn. Either way what follows the fade is the
    /// very audio it faded into.
    pub fn new(seam: Seam, start: usize, loop_at: usize, end: usize, xf: usize) -> Option<Join> {
        let loop_at = loop_at.clamp(start, end);
        let span = end - loop_at;
        let join = |xf, incoming, reverse, wrap_to| Join {
            seam,
            start,
            loop_at,
            end,
            xf,
            incoming,
            reverse,
            wrap_to,
        };
        match seam {
            Seam::Alt if span >= 2 => Some(join(0, 0, false, loop_at)),
            Seam::Alt => None,
            _ if span == 0 => None,
            Seam::Bfly if xf.min((span - 1) / 2) > 0 => {
                let xf = xf.min((span - 1) / 2);
                Some(join(xf, loop_at + xf - 1, true, loop_at + 1))
            }
            _ => {
                let pre = xf.min(span);
                Some(if loop_at >= pre {
                    join(pre, loop_at - pre, false, loop_at)
                } else {
                    let xf = xf.min(span / 2);
                    join(xf, loop_at, false, loop_at + xf)
                })
            }
        }
    }

    /// Where the fade starts.
    pub fn fade_start(&self) -> usize {
        self.end - self.xf
    }

    /// The incoming side's frame k frames into the fade.
    #[inline]
    pub fn incoming(&self, k: usize) -> usize {
        if self.reverse { self.incoming - k } else { self.incoming + k }
    }

    /// The frames from one seam to the next, after the first time round (for ALT, one way).
    pub fn time_round(&self) -> usize {
        match self.seam {
            Seam::Alt => self.end - self.loop_at - 1,
            _ => self.end - self.wrap_to,
        }
    }
}

/// How alike the fade's two sides are, as played: their correlation over the fade, every
/// channel, from −1 to 1 (0 when there's no fade, or either side is silent).
pub fn likeness(s: &[Vec<f32>], j: &Join) -> f64 {
    let (mut ab, mut aa, mut bb) = (0.0f64, 0.0f64, 0.0f64);
    for c in s {
        for k in 0..j.xf {
            let (a, b) = (c[j.fade_start() + k] as f64, c[j.incoming(k)] as f64);
            ab += a * b;
            aa += a * a;
            bb += b * b;
        }
    }
    if aa == 0.0 || bb == 0.0 { 0.0 } else { ab / (aa * bb).sqrt() }
}

/// Which point [`find`] moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Point {
    Loop,
    End,
}

/// What [`find`] found: where the point goes, how good the join is there, and how good it was.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Found {
    pub at: usize,
    pub score: f64,
    pub was: f64,
}

/// Moves LOOP or END to the best join within `window` frames either side, as Dexter's TRIM `f`
/// moves a mark into the quiet. FWD and BFLY: half how alike the wave is around END and around
/// LOOP over `w` frames centred on each (so a splice there carries on as if it were never cut:
/// a tone's whole cycles), half how alike their levels are over the fade's length, `xf`, or
/// `w` when there's no fade (so wind doesn't go round from a gust into a lull: noise is never
/// alike in its wave, only in its level). ALT: where the wave is flattest, so the turn doesn't
/// kink it. A point already within 0.005 of the best stays where it is. The loop keeps room
/// for its fade: END − LOOP at least 2·`xf` + 2.
pub fn find(s: &[Vec<f32>], j: &Join, xf: usize, point: Point, window: usize, w: usize) -> Found {
    let n = frames(s);
    let room = 2 * xf + 2;
    // Running sums of every channel's energy, so a window's level costs the same at any length.
    let mut energy = Vec::with_capacity(n + 1);
    energy.push(0.0f64);
    for k in 0..n {
        let e: f64 = s.iter().map(|c| (c[k] as f64).powi(2)).sum();
        energy.push(energy[k] + e);
    }
    // ALT turns at LOOP and at END − 1, each on its own: only the point moving counts.
    let score = |l: usize, e: usize| match (j.seam, point) {
        (Seam::Alt, Point::Loop) => flatness(s, l),
        (Seam::Alt, Point::End) => flatness(s, e.saturating_sub(1)),
        _ => 0.5 * sameness(s, l, e, w) + 0.5 * levels(&energy, l, e, xf.max(w)),
    };
    let (now, lo, hi) = match point {
        Point::Loop => (
            j.loop_at,
            j.loop_at.saturating_sub(window).max(j.start),
            (j.loop_at + window).min(j.end.saturating_sub(room)),
        ),
        Point::End => (j.end, (j.end.saturating_sub(window)).max(j.loop_at + room), (j.end + window).min(n)),
    };
    let at = |p: usize| match point {
        Point::Loop => score(p, j.end),
        Point::End => score(j.loop_at, p),
    };
    let was = at(now);
    let mut best = Found { at: now, score: was, was };
    for p in lo..=hi {
        let v = at(p);
        if v > best.score || (v == best.score && p.abs_diff(now) < best.at.abs_diff(now)) {
            best.at = p;
            best.score = v;
        }
    }
    if was >= best.score - 0.005 { Found { at: now, score: was, was } } else { best }
}

/// How much the `w` frames around e are like those around l, from −1 to 1: 1 − their squared
/// difference over their energy (so a match in shape but not in level scores less). Silence
/// matches silence.
fn sameness(s: &[Vec<f32>], l: usize, e: usize, w: usize) -> f64 {
    let n = frames(s);
    let half = (w / 2).max(1);
    let before = half.min(l).min(e);
    let after = half.min(n - l.min(n)).min(n - e.min(n));
    if before + after == 0 {
        return splice(s, l, e, w);
    }
    let (mut d, mut en) = (0.0f64, 0.0f64);
    for c in s {
        for i in 0..before + after {
            let (a, b) = (c[e - before + i] as f64, c[l - before + i] as f64);
            d += (a - b) * (a - b);
            en += a * a + b * b;
        }
    }
    if en == 0.0 { 1.0 } else { 1.0 - d / en }
}

/// A loop of the whole sample, with nothing around its points to compare: whether the jump
/// from END back to LOOP carries the wave on, from 0 to 1. The frame after END, as the last
/// two foretell it (where the slope takes it), against LOOP's; the miss measured against the
/// level over the `w` frames either side.
fn splice(s: &[Vec<f32>], l: usize, e: usize, w: usize) -> f64 {
    let n = frames(s);
    if e < 2 || l >= n {
        return 1.0;
    }
    let w = w.max(2).min(e).min(n - l);
    let (mut miss, mut en) = (0.0f64, 0.0f64);
    for c in s {
        let next = 2.0 * c[e - 1] as f64 - c[e - 2] as f64;
        miss += (c[l] as f64 - next).powi(2);
        en +=
            c[e - w..e].iter().chain(&c[l..l + w]).map(|v| (*v as f64).powi(2)).sum::<f64>() / (2 * w) as f64;
    }
    if en == 0.0 { 1.0 } else { (1.0 - miss / (2.0 * en)).max(-1.0) }
}

/// Where the two windows whose levels are compared start, and their length: the `w` frames
/// centred on e and on l, cut to what the sample has around both. A loop of the whole sample
/// has nothing around its points: then the `w` frames before END against the `w` from LOOP,
/// what goes round meets.
fn windows(n: usize, l: usize, e: usize, w: usize) -> (usize, usize, usize) {
    let half = (w / 2).max(1);
    let before = half.min(l).min(e);
    let after = half.min(n - l.min(n)).min(n - e.min(n));
    if before + after > 0 {
        return (e - before, l - before, before + after);
    }
    let len = w.max(1).min(e).min(n - l.min(n));
    (e - len, l, len)
}

/// How alike the levels are over the `w` frames around e and around l, from 0 to 1: the
/// quieter one's energy over the louder's, from the running sums. Silence matches silence.
fn levels(energy: &[f64], l: usize, e: usize, w: usize) -> f64 {
    let (from_e, from_l, len) = windows(energy.len() - 1, l, e, w);
    let sum = |from: usize| (energy[from + len] - energy[from]).max(0.0);
    let (a, b) = (sum(from_e), sum(from_l));
    if a.max(b) <= 1e-12 { 1.0 } else { a.min(b) / a.max(b) }
}

/// How flat the wave is at p, from 0 to 1: 1 where it neither rises nor falls.
fn flatness(s: &[Vec<f32>], p: usize) -> f64 {
    let n = frames(s);
    if n < 3 {
        return 1.0;
    }
    let p = p.clamp(1, n - 2);
    let slope: f64 = s.iter().map(|c| (c[p + 1] as f64 - c[p - 1] as f64).abs()).sum::<f64>();
    1.0 - slope / (4.0 * s.len() as f64)
}
