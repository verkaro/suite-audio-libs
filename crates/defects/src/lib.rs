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
            channel(x, rate).into_iter().map(move |mut d| {
                d.channel = c;
                d
            })
        })
        .collect()
}

fn channel(x: &[f64], rate: u32) -> Vec<Defect> {
    let n = x.len();
    let half = ((AROUND * rate as f64) / 2.0) as usize;
    let within = (WITHIN * rate as f64).ceil() as usize;
    // Sums of squares to every point, in order: the same additions on every machine.
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
        // The surroundings, the millisecond either side of k left out.
        let (lo, hi) = (k.saturating_sub(half), (k + half + 1).min(n));
        let (elo, ehi) = (k.saturating_sub(within), (k + within + 1).min(n));
        let count = (hi - lo) - (ehi - elo);
        let rms = if count == 0 {
            0.0
        } else {
            (f64::max(0.0, energy(lo, hi) - energy(elo, ehi)) / count as f64).sqrt()
        };
        if x[k].abs() > ratio * rms && x[k] != 0.0 {
            // The excursion: the run of samples more than 6 dB above the surroundings.
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
}
