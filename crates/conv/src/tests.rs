//! place's tests of the convolver (its `crates/core/src/tests.rs`), with synthetic IRs.

use super::*;

/// Numbers from a seed, the same every run: xorshift, −1 to 1.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        ((self.0 >> 40) as f32 / (1u64 << 23) as f32) - 1.0
    }

    fn noise(&mut self, n: usize) -> Vec<f32> {
        (0..n).map(|_| self.next()).collect()
    }
}

/// Decaying noise: a synthetic room's IR, `n` frames, falling 60 dB over them.
fn decaying(seed: u64, n: usize) -> Vec<f32> {
    let mut rng = Rng(seed);
    (0..n).map(|k| rng.next() * 10f32.powf(-3.0 * k as f32 / n as f32) * 0.3).collect()
}

fn direct(x: &[f32], h: &[f32]) -> Vec<f32> {
    (0..x.len())
        .map(|n| (0..h.len().min(n + 1)).map(|j| h[j] as f64 * x[n - j] as f64).sum::<f64>() as f32)
        .collect()
}

fn convolve(ir: &[f32], p: usize, x: &[f32], block: usize) -> Vec<f32> {
    let mut c = Convolver::new(ir, p);
    let mut y = vec![0.0; x.len()];
    for (xs, ys) in x.chunks(block).zip(y.chunks_mut(block)) {
        c.run(xs, ys);
    }
    y
}

#[test]
fn convolution_matches_direct_at_any_partition_and_block() {
    let x = Rng(1).noise(3000);
    for &n in &[1, 5, 64, 100, 257, 1000] {
        let h = decaying(n as u64, n);
        let want = direct(&x, &h);
        for &p in &[1, 16, 64, 256] {
            for &b in &[1, 7, 64, 333, 3000] {
                let y = convolve(&h, p, &x, b);
                let err = y.iter().zip(&want).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
                assert!(err < 1e-4, "IR {n}, partition {p}, block {b}: off by {err}");
            }
        }
    }
}

#[test]
fn a_long_tail_in_long_partitions_matches_direct_too() {
    // Past 32 partitions the tail is worked in partitions 16 times as long.
    let x = Rng(5).noise(6000);
    for (n, p) in [(40, 1), (3000, 16), (5000, 64)] {
        let h = decaying(n as u64 + 7, n);
        let want = direct(&x, &h);
        for b in [1, 13, 64, 1000, 6000] {
            let y = convolve(&h, p, &x, b);
            let err = y.iter().zip(&want).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
            assert!(err < 1e-4, "IR {n}, partition {p}, block {b}: off by {err}");
        }
    }
}

#[test]
fn convolution_is_the_same_at_any_block() {
    let x = Rng(2).noise(5000);
    for (h, p) in [(decaying(3, 2000), 64), (decaying(4, 4000), 16)] {
        let a = convolve(&h, p, &x, 1);
        for b in [3, 64, 500, 5000] {
            assert_eq!(a, convolve(&h, p, &x, b), "partition {p}, block {b}");
        }
    }
}

#[test]
fn an_impulse_passes_a_sound_bit_for_bit() {
    let x = Rng(4).noise(2000);
    assert_eq!(convolve(&[1.0], 64, &x, 100), x);
    let mut long = vec![0.0; 700];
    long[0] = 1.0;
    assert_eq!(convolve(&long, 64, &x, 100), x);
}
