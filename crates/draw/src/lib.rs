//! Variation from a seed, the family's rule for it (moved into the suite from scape's
//! `crates/core/src/lib.rs`, `VENDORED.md`): a pass's draws are a pure function of the seed, the
//! voice (a layer, a lane), the pass's number and what's drawn. A hash of the four, never a
//! running state, so one voice's draws don't depend on the block size, the render's length or
//! any other voice, and a render made again at another length comes out the same.

#[cfg(test)]
mod tests;

/// A number in [0, 1) that depends only on its arguments: the variation's one source. `what`
/// keeps a pass's draws apart (its level from its pan); a caller numbers them as it likes, below
/// 256.
pub fn draw(seed: u64, voice: u64, pass: u64, what: u64) -> f64 {
    let h = mix(seed ^ mix(voice.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ mix(pass) ^ what << 56));
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// The seed a voice draws from: the whole's, mixed with the voice's own when it has one (0 for
/// none), so a voice can be drawn again alone.
pub fn own_seed(seed: u64, own: u64) -> u64 {
    if own == 0 { seed } else { seed ^ mix(own ^ 0x5CA9_E5EE_D000_0000) }
}

/// A value within `spread` of `v`, either way, from `u` in [0, 1).
pub fn spread(v: f64, spread: f64, u: f64) -> f64 {
    v + spread * (2.0 * u - 1.0)
}

/// A value from `lo` to `hi`, from `u` in [0, 1).
pub fn between(lo: f64, hi: f64, u: f64) -> f64 {
    lo + (hi - lo) * u
}

/// One of `n` (more than 0), never `last` when there are two or more to choose from.
pub fn pick(n: usize, last: Option<usize>, u: f64) -> usize {
    match last {
        Some(l) if n > 1 && l < n => {
            let k = ((u * (n - 1) as f64) as usize).min(n - 2);
            if k >= l { k + 1 } else { k }
        }
        _ => ((u * n as f64) as usize).min(n - 1),
    }
}

/// SplitMix64's finaliser.
pub fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}
