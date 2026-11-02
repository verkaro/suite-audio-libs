//! Defects: things in a recording that were never sound (clicks, single-sample errors,
//! conversion glitches), found so they can be fixed upstream. Tape's `analyze` reports them
//! (its RFC, S1), and the spike-repair tool will repair them (Appendix A), both through this
//! crate so they agree on what a defect is. Written for the suite, 2026-11-01.
//!
//! The heuristic: a sample more than 20 dB above the RMS of the 10 ms around it (the
//! excursion itself left out of that RMS), whose excursion (the run of samples more than 6 dB
//! above that RMS) is over within 1 ms. A real transient (a door, a footstep, a consonant)
//! goes on longer; a click doesn't. It's a heuristic, not a test: real Foley (glass, keys, a
//! tongue click) can trip it, so a defect is a place to listen, not a verdict.

/// How far above its surroundings a sample must be, dB.
pub const ABOVE_DB: f64 = 20.0;
/// The surroundings, seconds, centred on the sample.
pub const AROUND: f64 = 0.010;
/// The longest excursion that counts, seconds.
pub const WITHIN: f64 = 0.001;

/// A defect: where, and how high.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Defect {
    pub channel: usize,
    /// Its highest sample.
    pub frame: usize,
    /// That sample, as a level (1.0 full scale).
    pub peak: f64,
    /// How far above its surroundings' RMS, dB; infinite in digital silence.
    pub above_db: f64,
}

/// Every defect in a sound's channels, at `rate` Hz, in order of channel and then frame.
pub fn find(channels: &[&[f64]], rate: u32) -> Vec<Defect> {
    channels
        .iter()
        .enumerate()
        .flat_map(|(c, x)| {
            let mut f = Finder::new(c, rate);
            for piece in x.chunks(1 << 16) {
                f.push(piece);
            }
            f.finish()
        })
        .collect()
}

/// One channel's defects, found a piece at a time (added 2026-11-01, so a long file needs no
/// more memory than a piece): fed the channel in pieces of any size, it finds what [`find`]
/// finds in it whole, bit for bit.
#[derive(Clone, Debug)]
pub struct Finder {
    channel: usize,
    half: usize,
    within: usize,
    ratio: f64,
    /// The samples held, from sample `base` on, and the sums of squares to every point from
    /// `base` on (`sums[0]` is the sum before sample `base`).
    x: Vec<f64>,
    sums: Vec<f64>,
    base: usize,
    /// Samples fed; the next sample to look at.
    n: usize,
    k: usize,
    found: Vec<Defect>,
}

impl Finder {
    /// For channel `channel` (as each defect will say), at `rate` Hz.
    pub fn new(channel: usize, rate: u32) -> Finder {
        Finder {
            channel,
            half: ((AROUND * rate as f64) / 2.0) as usize,
            within: (WITHIN * rate as f64).ceil() as usize,
            ratio: libm::pow(10.0, ABOVE_DB / 20.0),
            x: vec![],
            sums: vec![0.0],
            base: 0,
            n: 0,
            k: 0,
            found: vec![],
        }
    }

    /// The channel's next piece.
    pub fn push(&mut self, x: &[f64]) {
        // Sums of squares to every point, in order: the same additions on every machine.
        for &v in x {
            let last = *self.sums.last().unwrap();
            self.sums.push(last + v * v);
        }
        self.x.extend_from_slice(x);
        self.n += x.len();
        self.scan(false);
    }

    /// Every defect in the channel, in order.
    pub fn finish(mut self) -> Vec<Defect> {
        self.scan(true);
        self.found
    }

    /// Looks at each sample whose surroundings have all been fed (at the `end`, every one left).
    fn scan(&mut self, end: bool) {
        let (n, half, within, base) = (self.n, self.half, self.within, self.base);
        let x = |i: usize| self.x[i - base];
        let energy = |a: usize, b: usize| self.sums[b - base] - self.sums[a - base];
        let mut k = self.k;
        while k < n && (end || k + half < n) {
            // The surroundings, the millisecond either side of k left out.
            let (lo, hi) = (k.saturating_sub(half), (k + half + 1).min(n));
            let (elo, ehi) = (k.saturating_sub(within), (k + within + 1).min(n));
            let count = (hi - lo) - (ehi - elo);
            let rms = if count == 0 {
                0.0
            } else {
                (f64::max(0.0, energy(lo, hi) - energy(elo, ehi)) / count as f64).sqrt()
            };
            if x(k).abs() > self.ratio * rms && x(k) != 0.0 {
                // The excursion: the run of samples more than 6 dB above the surroundings.
                let loud = |v: f64| v.abs() > 2.0 * rms;
                let (mut a, mut b) = (k, k + 1);
                while a > 0 && loud(x(a - 1)) && k - a <= within {
                    a -= 1;
                }
                while b < n && loud(x(b)) && b - a <= within {
                    b += 1;
                }
                if b - a <= within {
                    let peak_at = (a..b).max_by(|&i, &j| x(i).abs().total_cmp(&x(j).abs())).unwrap();
                    let above =
                        if rms == 0.0 { f64::INFINITY } else { 20.0 * libm::log10(x(peak_at).abs() / rms) };
                    self.found.push(Defect {
                        channel: self.channel,
                        frame: peak_at,
                        peak: x(peak_at),
                        above_db: above,
                    });
                }
                k = b.max(k + 1);
                continue;
            }
            k += 1;
        }
        self.k = k;
        // Keep what's still to be looked back at: the surroundings of the next sample.
        let keep = k.saturating_sub(half + within + 1);
        if keep > self.base + (1 << 16) {
            self.x.drain(..keep - self.base);
            self.sums.drain(..keep - self.base);
            self.base = keep;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bed(n: usize) -> Vec<f64> {
        (0..n).map(|k| 0.05 * libm::sin(k as f64 * 0.0571) + 0.01 * libm::sin(k as f64 * 1.3)).collect()
    }

    #[test]
    fn a_click_in_a_bed_is_found_where_it_is() {
        let mut x = bed(48_000);
        x[10_000] += 0.9;
        x[30_000] -= 0.6;
        x[30_001] -= 0.3;
        let d = find(&[&x], 48_000);
        assert_eq!(d.len(), 2, "{d:?}");
        assert_eq!((d[0].frame, d[1].frame), (10_000, 30_000));
        assert!(d[0].above_db > 20.0 && d[0].above_db < 40.0, "{}", d[0].above_db);
    }

    #[test]
    fn a_real_transient_is_not_a_defect() {
        // A hit decaying over 30 ms out of a quiet bed.
        let mut x = bed(48_000);
        for k in 0..1440 {
            x[20_000 + k] += 0.9 * libm::exp(-(k as f64) / 300.0) * libm::sin(k as f64 * 0.3);
        }
        assert_eq!(find(&[&x], 48_000), vec![]);
        // And a hit out of digital silence.
        let mut y = vec![0.0; 48_000];
        for k in 0..2000 {
            y[5000 + k] = 0.8 * libm::exp(-(k as f64) / 500.0) * libm::sin(k as f64 * 0.2);
        }
        assert_eq!(find(&[&y], 48_000), vec![]);
    }

    #[test]
    fn a_lone_sample_in_silence_is_infinitely_above() {
        let mut x = vec![0.0; 9600];
        x[4000] = 0.01;
        let d = find(&[&vec![0.0; 9600], &x], 96_000);
        assert_eq!(d.len(), 1);
        assert_eq!((d[0].channel, d[0].frame, d[0].above_db), (1, 4000, f64::INFINITY));
    }

    /// The channel as it was searched whole before the finder (2026-11-01): its oracle.
    fn whole(x: &[f64], rate: u32) -> Vec<Defect> {
        let n = x.len();
        let half = ((AROUND * rate as f64) / 2.0) as usize;
        let within = (WITHIN * rate as f64).ceil() as usize;
        let mut sums = Vec::with_capacity(n + 1);
        sums.push(0.0);
        for v in x {
            sums.push(sums.last().unwrap() + v * v);
        }
        let energy = |a: usize, b: usize| sums[b] - sums[a];
        let ratio = libm::pow(10.0, ABOVE_DB / 20.0);
        let mut found = vec![];
        let mut k = 0;
        while k < n {
            let (lo, hi) = (k.saturating_sub(half), (k + half + 1).min(n));
            let (elo, ehi) = (k.saturating_sub(within), (k + within + 1).min(n));
            let count = (hi - lo) - (ehi - elo);
            let rms = if count == 0 {
                0.0
            } else {
                (f64::max(0.0, energy(lo, hi) - energy(elo, ehi)) / count as f64).sqrt()
            };
            if x[k].abs() > ratio * rms && x[k] != 0.0 {
                let loud = |v: f64| v.abs() > 2.0 * rms;
                let (mut a, mut b) = (k, k + 1);
                while a > 0 && loud(x[a - 1]) && k - a <= within {
                    a -= 1;
                }
                while b < n && loud(x[b]) && b - a <= within {
                    b += 1;
                }
                if b - a <= within {
                    let peak_at = (a..b).max_by(|&i, &j| x[i].abs().total_cmp(&x[j].abs())).unwrap();
                    let above =
                        if rms == 0.0 { f64::INFINITY } else { 20.0 * libm::log10(x[peak_at].abs() / rms) };
                    found.push(Defect { channel: 0, frame: peak_at, peak: x[peak_at], above_db: above });
                }
                k = b.max(k + 1);
                continue;
            }
            k += 1;
        }
        found
    }

    #[test]
    fn a_finder_fed_in_pieces_finds_what_the_whole_search_did() {
        // Clicks everywhere, near the ends, in silence and close together.
        let mut x = bed(300_000);
        for (i, at) in [0usize, 3, 239, 240, 241, 10_000, 65_535, 65_536, 65_600, 131_072, 299_990, 299_999]
            .iter()
            .enumerate()
        {
            x[*at] += if i % 2 == 0 { 0.8 } else { -0.7 };
        }
        for v in &mut x[200_000..210_000] {
            *v = 0.0;
        }
        x[205_000] = 0.5;
        let want = whole(&x, 48_000);
        assert!(want.len() >= 7, "{want:?}");
        for piece in [1, 97, 4800, 65_536, 1 << 20] {
            let mut f = Finder::new(0, 48_000);
            for c in x.chunks(piece) {
                f.push(c);
            }
            assert_eq!(f.finish(), want, "{piece}");
        }
        assert_eq!(find(&[&x], 48_000), want);
    }
}
