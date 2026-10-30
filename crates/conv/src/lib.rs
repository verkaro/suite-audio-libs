//! Convolution with an IR, one channel in and one out. Moved into the suite from place's
//! `crates/core/src/conv.rs` (`VENDORED.md`; place's `docs/design.md`, "Convolution").
//!
//! Partitioned, with no latency, in two sizes. The IR's first `p` taps are worked out directly,
//! a frame at a time. The next, to `32p`, in partitions of `p` through the FFT (overlap-save);
//! the rest, a long hall's tail, in partitions of `16p`, which take a sixteenth of the
//! multiplies a frame. Each block's work is spread over the frames of the block before it
//! (every partition but the first of each size uses only input that's already complete), so a
//! period of JACK's does about the same work whatever it lands on, never a whole block's at
//! once. Every sum is made in the same order however the frames come, so what comes out at a
//! frame depends only on the input up to it, never on how many frames it's given at once, and
//! the convolved paths line up with the dry ones, with no delay to make up anywhere.
//!
//! Made off JACK's thread (the IR's spectra are worked out in [`Convolver::new`]); it never
//! allocates once made.

use std::sync::Arc;

#[cfg(test)]
mod tests;

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

/// The partition to use, in frames: 256, about 5 ms at 48 kHz. Its direct part costs this many
/// multiplies a frame.
pub const PARTITION: usize = 256;

/// The long partitions, in short ones.
const LONG: usize = 16;

/// `n` of `T::default()`, every page of it written now: memory that's only been asked for is
/// mapped on its first touch, and that touch mustn't be on JACK's thread. Written volatile, so
/// the compiler can't fold it back into an allocation of untouched zeros.
pub fn touched<T: Default + Clone>(n: usize) -> Vec<T> {
    let mut v = vec![T::default(); n];
    for x in v.iter_mut() {
        // SAFETY: a valid, aligned element of v.
        unsafe { std::ptr::write_volatile(x, T::default()) };
    }
    v
}

pub struct Convolver {
    p: usize,
    /// The first `p` taps, reversed, worked out directly.
    head: Vec<f32>,
    /// The last `2p` frames in, for the head.
    lin: Vec<f32>,
    /// Where in the block of `p` it is.
    pos: usize,
    /// The partitions of `p`, from tap `p`; of `16p`, from tap `32p`.
    short: Option<Level>,
    long: Option<Level>,
}

/// One size of partition: its blocks of input, their spectra, and its share of the output.
struct Level {
    l: usize,
    /// Whether its first partition starts one block in (`p`), so that block's spectrum and its
    /// first product are made at the block's end; the long ones start two blocks in (`32p`),
    /// and all their work is done in the block before.
    eager: bool,
    /// Its partitions, as spectra (scaled for the inverse FFT).
    parts: Vec<Vec<Complex<f32>>>,
    /// The block before and the block coming in, `2l` frames; and, for the long ones, the two
    /// just complete, kept for their spectrum while the next comes in.
    lin: Vec<f32>,
    win: Vec<f32>,
    pos: usize,
    /// The spectra of the last blocks in, newest at `newest`, one for each partition.
    fdl: Vec<Vec<Complex<f32>>>,
    newest: usize,
    acc: Vec<Complex<f32>>,
    /// What it adds to the block coming in, and to the next.
    cur: Vec<f32>,
    next: Vec<f32>,
    /// Of the next block's work (`units` steps), how much is done.
    done: usize,
    units: usize,
    fwd: Arc<dyn RealToComplex<f32>>,
    inv: Arc<dyn ComplexToReal<f32>>,
    time: Vec<f32>,
    freq: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
}

impl Level {
    /// `taps` in partitions of `l`, the first `l·(1 + !eager)` frames in.
    fn new(taps: &[f32], l: usize, eager: bool, planner: &mut RealFftPlanner<f32>) -> Option<Level> {
        if taps.is_empty() {
            return None;
        }
        let fwd = planner.plan_fft_forward(2 * l);
        let inv = planner.plan_fft_inverse(2 * l);
        let mut scratch = touched(fwd.get_scratch_len().max(inv.get_scratch_len()));
        let bins = l + 1;
        let scale = 1.0 / (2 * l) as f32;
        let parts: Vec<Vec<Complex<f32>>> = taps
            .chunks(l)
            .map(|part| {
                let mut t = vec![0.0; 2 * l];
                for (d, s) in t.iter_mut().zip(part) {
                    *d = *s * scale;
                }
                let mut f = vec![Complex::default(); bins];
                fwd.process_with_scratch(&mut t, &mut f, &mut scratch).expect("fft sizes");
                f
            })
            .collect();
        let n = parts.len();
        // The eager one's next block: its partitions but the first; the long one's: the
        // spectrum, every partition, and the inverse.
        let units = if eager { n - 1 } else { n + 2 };
        Some(Level {
            l,
            eager,
            parts,
            lin: touched(2 * l),
            win: touched(2 * l),
            pos: 0,
            fdl: (0..n).map(|_| touched(bins)).collect(),
            newest: 0,
            acc: touched(bins),
            cur: touched(l),
            next: touched(l),
            // The long one has nothing complete to work on in the first block.
            done: if eager { 0 } else { units },
            units,
            fwd,
            inv,
            time: touched(2 * l),
            freq: touched(bins),
            scratch,
        })
    }

    /// A frame in; what this level adds to it out.
    #[inline]
    fn push(&mut self, x: f32) -> f32 {
        self.lin[self.l + self.pos] = x;
        let y = self.cur[self.pos];
        self.pos += 1;
        y
    }

    /// The share of the next block's work due by now, then the block's end if it's come.
    fn progress(&mut self) {
        let due = (self.units * self.pos).div_ceil(self.l);
        while self.done < due {
            self.unit(self.done);
            self.done += 1;
        }
        if self.pos == self.l {
            self.block();
        }
    }

    /// acc += the spectrum `k` blocks before the newest, times partition `part`.
    fn mac(&mut self, k: usize, part: usize) {
        let n = self.fdl.len();
        let x = &self.fdl[(self.newest + n - k) % n];
        for ((a, x), h) in self.acc.iter_mut().zip(x).zip(&self.parts[part]) {
            *a += x * h;
        }
    }

    /// The spectrum of the window in `time`, as the newest.
    fn spectrum(&mut self) {
        let n = self.fdl.len();
        self.newest = (self.newest + 1) % n;
        let s = &mut self.fdl[self.newest];
        self.fwd.process_with_scratch(&mut self.time, s, &mut self.scratch).expect("fft sizes");
    }

    /// The sum so far, back to time, into `out`; the sum begun again.
    fn inverse(&mut self, eager: bool) {
        self.freq.copy_from_slice(&self.acc);
        // The inverse wants the imaginary parts at DC and Nyquist nothing; they're nothing
        // but rounding here.
        self.freq[0].im = 0.0;
        self.freq[self.l].im = 0.0;
        self.inv.process_with_scratch(&mut self.freq, &mut self.time, &mut self.scratch).expect("fft sizes");
        let out = if eager { &mut self.cur } else { &mut self.next };
        out.copy_from_slice(&self.time[self.l..]);
        self.acc.fill(Complex::default());
    }

    /// Step `u` of the next block's work.
    fn unit(&mut self, u: usize) {
        if self.eager {
            // Partition u + 1 meets the block u before the newest (the newest is the block
            // just complete; partition 0 meets the one coming in, at its end).
            self.mac(u, u + 1);
        } else if u == 0 {
            self.time.copy_from_slice(&self.win);
            self.spectrum();
        } else if u <= self.parts.len() {
            // Partition u − 1, two blocks and more in, meets the block u − 1 before the newest.
            self.mac(u - 1, u - 1);
        } else {
            self.inverse(false);
        }
    }

    /// A block of input complete.
    fn block(&mut self) {
        self.pos = 0;
        if self.eager {
            self.time.copy_from_slice(&self.lin);
            self.spectrum();
            self.mac(0, 0);
            self.inverse(true);
        } else {
            self.win.copy_from_slice(&self.lin);
            std::mem::swap(&mut self.cur, &mut self.next);
        }
        self.done = 0;
        self.lin.copy_within(self.l.., 0);
    }
}

impl Convolver {
    /// A convolver with `ir`, in partitions of `p` frames (more than 0).
    pub fn new(ir: &[f32], p: usize) -> Convolver {
        assert!(p > 0);
        let mut planner = RealFftPlanner::<f32>::new();
        let mut head = vec![0.0; p];
        for (j, h) in ir.iter().take(p).enumerate() {
            head[p - 1 - j] = *h;
        }
        let long_from = 2 * LONG * p;
        let short = ir.get(p..long_from.min(ir.len())).unwrap_or(&[]);
        let long = ir.get(long_from..).unwrap_or(&[]);
        Convolver {
            p,
            head,
            lin: touched(2 * p),
            pos: 0,
            short: Level::new(short, p, true, &mut planner),
            long: Level::new(long, LONG * p, false, &mut planner),
        }
    }

    /// The partition, in frames.
    pub fn partition(&self) -> usize {
        self.p
    }

    /// Convolves `x` into `y` (the same length), carrying on from the last call.
    pub fn run(&mut self, x: &[f32], y: &mut [f32]) {
        assert_eq!(x.len(), y.len());
        let p = self.p;
        let mut k = 0;
        while k < x.len() {
            let n = (p - self.pos).min(x.len() - k);
            for i in 0..n {
                let v = x[k + i];
                self.lin[p + self.pos] = v;
                let mut out = dot(&self.head, &self.lin[self.pos + 1..self.pos + 1 + p]);
                if let Some(s) = &mut self.short {
                    out += s.push(v);
                }
                if let Some(l) = &mut self.long {
                    out += l.push(v);
                }
                y[k + i] = out;
                self.pos += 1;
            }
            k += n;
            for level in [&mut self.short, &mut self.long].into_iter().flatten() {
                level.progress();
            }
            if self.pos == p {
                self.pos = 0;
                self.lin.copy_within(p.., 0);
            }
        }
    }
}

/// a · b, in eight running sums, always in the same order.
fn dot(a: &[f32], b: &[f32]) -> f32 {
    let mut s = [0.0f32; 8];
    let ((ca, ra), (cb, rb)) = (a.as_chunks::<8>(), b.as_chunks::<8>());
    for (x, y) in ca.iter().zip(cb) {
        for i in 0..8 {
            s[i] += x[i] * y[i];
        }
    }
    let mut t = ((s[0] + s[1]) + (s[2] + s[3])) + ((s[4] + s[5]) + (s[6] + s[7]));
    for (x, y) in ra.iter().zip(rb) {
        t += x * y;
    }
    t
}
