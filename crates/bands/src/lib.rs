//! A sound's power in bands spaced evenly on the ERB-number scale (Glasberg and Moore, 1990),
//! a frame every 10 ms, for any tool that needs to see where in frequency and when a sound's
//! energy lies. Written for the suite, 2026-11-02, for ducky; it builds on `realfft`.
//!
//! - [`Layout`]: the bands. Their edges are evenly spaced in ERB number between a low and a
//!   high frequency ([`Layout::erb`]); the window, the hop and which FFT bins belong to which
//!   band follow from the sample rate. Rates 44.1, 48, 88.2 and 96 kHz are supported; others,
//!   and layouts with a band that holds no bin, are refused.
//! - [`Analyzer`]: a sound pushed in pieces of any size comes out as frames of band powers,
//!   the same bits as if it had been pushed whole ([`analyze`]).
//!
//! **Frames.** The hop is `rate / 100` samples, exactly 10 ms. Frame `k` is centred on sample
//! `k · hop` and covers `[k · hop − window/2, k · hop + window/2)`, zeros outside the sound.
//! There are `ceil(len / hop)` frames. The window is a periodic Hann, `0.5 − 0.5 cos(2πn/N)`.
//!
//! **Power.** A band's power is `2 / (N Σw²) · Σ |X_k|²` over its bins. A sine of amplitude
//! `a` whose leakage lies wholly inside one band reads `a²/2` there, and stationary noise reads
//! its power in the band, `σ²` times the band's share of the spectrum. Bins outside the
//! layout's low and high frequencies, DC and Nyquist are dropped.
//!
//! Window and edges go through `libm`, never `std`'s transcendentals, and all arithmetic is
//! `f64` in a fixed order, so the same build gives the same bits. (`realfft` chooses a SIMD
//! path when it runs, so bits are not promised between machines.) Output is `f32`.

use std::ops::Range;
use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};

/// The ERB number of `hz` (Glasberg and Moore): `21.4 log10(1 + 0.00437 f)`.
pub fn erb_number(hz: f64) -> f64 {
    21.4 * libm::log10(1.0 + 0.00437 * hz)
}

/// The frequency of ERB number `e`, the inverse of [`erb_number`].
pub fn erb_hz(e: f64) -> f64 {
    (libm::pow(10.0, e / 21.4) - 1.0) / 0.00437
}

/// The bands, and the window, hop and bins that go with them at one sample rate.
#[derive(Clone, Debug)]
pub struct Layout {
    rate: u32,
    window: usize,
    hop: usize,
    edges: Vec<f64>,
    bins: Vec<Range<usize>>,
}

impl Layout {
    /// `bands` bands evenly spaced in ERB number from `lo_hz` to `hi_hz`, at `rate` Hz. The
    /// window is 2048 samples at 44.1 and 48 kHz and 4096 at 88.2 and 96 kHz; other rates are
    /// refused, as is a layout with a band that holds no FFT bin, or an edge above Nyquist.
    pub fn erb(rate: u32, bands: usize, lo_hz: f64, hi_hz: f64) -> Result<Layout, String> {
        let window = match rate {
            44_100 | 48_000 => 2048,
            88_200 | 96_000 => 4096,
            _ => return Err(format!("{rate} Hz isn't supported (44100, 48000, 88200 and 96000 are)")),
        };
        if bands == 0 {
            return Err("a layout needs at least one band".into());
        }
        if !(lo_hz > 0.0 && lo_hz < hi_hz && hi_hz <= rate as f64 / 2.0) {
            return Err(format!("the band range {lo_hz}-{hi_hz} Hz must lie within 0 and {} Hz", rate / 2));
        }
        let (e_lo, e_hi) = (erb_number(lo_hz), erb_number(hi_hz));
        let edges: Vec<f64> =
            (0..=bands).map(|b| erb_hz(e_lo + (e_hi - e_lo) * b as f64 / bands as f64)).collect();
        // Bin k (0 < k < N/2) is at k·rate/N Hz and belongs to band b if edge_b ≤ f < edge_{b+1}.
        // Frequency rises with k, so each band's bins are a run.
        let mut bins = vec![0..0; bands];
        for k in 1..window / 2 {
            let f = k as f64 * rate as f64 / window as f64;
            if f < edges[0] || f >= edges[bands] {
                continue;
            }
            let b = edges.partition_point(|&e| e <= f) - 1;
            if bins[b].is_empty() {
                bins[b] = k..k + 1;
            } else {
                bins[b].end = k + 1;
            }
        }
        if let Some(b) = bins.iter().position(|r| r.is_empty()) {
            return Err(format!(
                "band {b} ({:.1}-{:.1} Hz) holds no FFT bin at {rate} Hz",
                edges[b],
                edges[b + 1]
            ));
        }
        Ok(Layout { rate, window, hop: rate as usize / 100, edges, bins })
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// The FFT window, in samples.
    pub fn window(&self) -> usize {
        self.window
    }

    /// Samples from one frame to the next: `rate / 100`, exactly 10 ms.
    pub fn hop(&self) -> usize {
        self.hop
    }

    pub fn bands(&self) -> usize {
        self.bins.len()
    }

    /// The `bands() + 1` edges, in Hz, rising.
    pub fn edges_hz(&self) -> &[f64] {
        &self.edges
    }

    /// Each band's centre, the geometric mean of its edges.
    pub fn centres_hz(&self) -> Vec<f64> {
        self.edges.windows(2).map(|e| (e[0] * e[1]).sqrt()).collect()
    }

    /// The FFT bins that make up `band`.
    pub fn bin_range(&self, band: usize) -> Range<usize> {
        self.bins[band].clone()
    }

    /// The frames a sound of `len` samples makes: `ceil(len / hop)`.
    pub fn frames(&self, len: u64) -> u64 {
        len.div_ceil(self.hop as u64)
    }
}

/// Turns a sound, pushed in pieces of any size, into frames of band powers.
pub struct Analyzer {
    layout: Layout,
    fft: Arc<dyn RealToComplex<f64>>,
    window: Vec<f64>,
    /// `2 / (N Σw²)`.
    scale: f64,
    /// The samples from the start of the next frame's window: it starts with `window/2` zeros,
    /// the sound before sample 0.
    buf: Vec<f64>,
    frame: Vec<f64>,
    spectrum: Vec<Complex<f64>>,
    scratch: Vec<Complex<f64>>,
    /// Samples pushed so far, and frames written.
    pushed: u64,
    written: u64,
}

impl Analyzer {
    pub fn new(layout: &Layout) -> Analyzer {
        let n = layout.window;
        let window: Vec<f64> =
            (0..n).map(|i| 0.5 - 0.5 * libm::cos(2.0 * std::f64::consts::PI * i as f64 / n as f64)).collect();
        let sum_sq = window.iter().fold(0.0, |s, w| s + w * w);
        let fft = RealFftPlanner::<f64>::new().plan_fft_forward(n);
        let (spectrum, scratch) = (fft.make_output_vec(), fft.make_scratch_vec());
        Analyzer {
            layout: layout.clone(),
            fft,
            window,
            scale: 2.0 / (n as f64 * sum_sq),
            buf: vec![0.0; n / 2],
            frame: vec![0.0; n],
            spectrum,
            scratch,
            pushed: 0,
            written: 0,
        }
    }

    /// Takes the next samples of the sound and appends `bands()` values to `out` for each
    /// frame they complete.
    pub fn push(&mut self, x: &[f64], out: &mut Vec<f32>) {
        self.pushed += x.len() as u64;
        self.buf.extend_from_slice(x);
        // Frame k is whole once the sound reaches k·hop + window/2, which can't be past its end.
        let (n, hop) = (self.layout.window, self.layout.hop);
        let mut at = 0;
        while self.buf.len() - at >= n {
            self.emit(at, out);
            at += hop;
        }
        self.buf.drain(..at);
    }

    /// The frames still due once the sound has ended, with zeros past its end.
    pub fn finish(mut self, out: &mut Vec<f32>) {
        let (n, hop) = (self.layout.window, self.layout.hop);
        let due = self.layout.frames(self.pushed);
        let mut at = 0;
        while self.written < due {
            if self.buf.len() < at + n {
                self.buf.resize(at + n, 0.0);
            }
            self.emit(at, out);
            at += hop;
        }
    }

    /// Writes the frame whose window starts at `buf[at]`.
    fn emit(&mut self, at: usize, out: &mut Vec<f32>) {
        for ((f, w), x) in self.frame.iter_mut().zip(&self.window).zip(&self.buf[at..]) {
            *f = w * x;
        }
        self.fft
            .process_with_scratch(&mut self.frame, &mut self.spectrum, &mut self.scratch)
            .expect("the buffers are the plan's sizes");
        for range in &self.layout.bins {
            let sum = self.spectrum[range.clone()].iter().fold(0.0, |s, c| s + (c.re * c.re + c.im * c.im));
            out.push((self.scale * sum) as f32);
        }
        self.written += 1;
    }
}

/// A whole sound's band powers, `frames(len) × bands()` values, frame after frame.
pub fn analyze(x: &[f64], layout: &Layout) -> Vec<f32> {
    let mut out = Vec::with_capacity(layout.frames(x.len() as u64) as usize * layout.bands());
    let mut a = Analyzer::new(layout);
    a.push(x, &mut out);
    a.finish(&mut out);
    out
}

#[cfg(test)]
mod tests;
