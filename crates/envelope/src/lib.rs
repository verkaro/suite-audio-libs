//! What an audio process's acceptance tests need (Tape's RFC, section 5 and 10): test signals
//! made from code, a synthetic corpus of speech-like and other sounds, and the measurements
//! that say whether a process stayed within its limits. Written for the suite, 2026-11-01.
//!
//! Everything is generated from code, the same every time (noise from a fixed-seed generator),
//! so a corpus never needs storing and a test never depends on a recording.
//!
//! - [`signal`]: sines with whole cycles, stepped log sines, impulses, noise, the torture set.
//! - [`measure`]: a tone's amplitude and phase, THD, products that aren't harmonics, nulls.
//! - [`corpus`]: voices (pulse trains through formants, whispered to shouted), sibilants,
//!   plosives, a boom and a lav of one voice, a sweep, impacts, a bed, room tone, silence.

pub mod corpus;
pub mod measure;
pub mod signal;

/// A level as dB (1.0 is 0 dB).
pub fn db(level: f64) -> f64 {
    20.0 * libm::log10(level)
}

/// dB as a level.
pub fn level(db: f64) -> f64 {
    libm::pow(10.0, db / 20.0)
}
