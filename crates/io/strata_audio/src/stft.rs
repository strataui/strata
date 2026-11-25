//! Zero-allocation Short-Time Fourier Transform (STFT) overlap-add engine with exact
//! periodic square-root Hann Constant Overlap-Add (COLA) reconstruction.

use crate::fft::{Complex, FftPlan};
use std::f32::consts::PI;

/// Pre-allocated 50%-overlap Hann COLA STFT processor.
///
/// With analysis window $w[n] = \sqrt{0.5(1 - \cos(2\pi n / N))}$ and identical synthesis
/// window at hop $H = N/2$, $w[n]^2 + w[n + H]^2 \equiv 1$ for all $n \in [0, H)$,
/// yielding bit-accurate identity reconstruction with $H$-sample algorithmic latency
/// (e.g., 5.33 ms at `fft_size = 512`, 48 kHz; or 8.0 ms at `fft_size = 256`, 16 kHz).
pub struct StftProcessor {
    fft_size: usize,
    hop_size: usize,
    plan: FftPlan,
    window: Vec<f32>,
    /// Sliding analysis window holding the most recent `fft_size` samples.
    analysis_frame: Vec<f32>,
    /// Overlap-add tail from the previous synthesis frame (`hop_size` samples).
    ola_tail: Vec<f32>,
    /// Pre-allocated complex spectrum scratch buffer (`fft_size`).
    spectrum: Vec<Complex>,
    /// Pre-allocated staging FIFOs for arbitrary caller block sizes (zero alloc after init).
    in_fifo: Vec<f32>,
    in_fill: usize,
    out_fifo: Vec<f32>,
    out_read: usize,
    out_len: usize,
    hop_out_scratch: Vec<f32>,
}

impl StftProcessor {
    /// Creates a new 50%-overlap STFT engine for a power-of-two `fft_size` (>= 32).
    pub fn new(fft_size: usize) -> Self {
        assert!(
            fft_size >= 32 && fft_size.is_power_of_two(),
            "STFT fft_size must be a power of two >= 32"
        );
        let hop_size = fft_size / 2;
        let mut window = Vec::with_capacity(fft_size);
        for n in 0..fft_size {
            let hann = 0.5 * (1.0 - (2.0 * PI * n as f32 / fft_size as f32).cos());
            window.push(hann.max(0.0).sqrt());
        }

        // Pre-allocate generous FIFO ring capacity so arbitrary real-time block sizes never alloc.
        let fifo_cap = (fft_size * 8).max(4096);

        Self {
            fft_size,
            hop_size,
            plan: FftPlan::new(fft_size),
            window,
            analysis_frame: vec![0.0; fft_size],
            ola_tail: vec![0.0; hop_size],
            spectrum: vec![Complex::zero(); fft_size],
            in_fifo: vec![0.0; hop_size],
            in_fill: 0,
            out_fifo: vec![0.0; fifo_cap],
            out_read: 0,
            out_len: 0,
            hop_out_scratch: vec![0.0; hop_size],
        }
    }

    #[inline]
    pub const fn fft_size(&self) -> usize {
        self.fft_size
    }

    #[inline]
    pub const fn hop_size(&self) -> usize {
        self.hop_size
    }

    /// Returns the exact algorithmic frame latency in samples (`hop_size`).
    #[inline]
    pub const fn latency_samples(&self) -> usize {
        self.hop_size
    }

    /// Returns the algorithmic latency in milliseconds at `sample_rate`.
    #[inline]
    pub fn latency_ms(&self, sample_rate: u32) -> f32 {
        if sample_rate == 0 {
            0.0
        } else {
            (self.hop_size as f32 * 1000.0) / sample_rate as f32
        }
    }

    /// Resets all overlap-add and staging buffers to silence without deallocating.
    pub fn reset(&mut self) {
        self.analysis_frame.fill(0.0);
        self.ola_tail.fill(0.0);
        self.spectrum.fill(Complex::zero());
        self.in_fifo.fill(0.0);
        self.in_fill = 0;
        self.out_fifo.fill(0.0);
        self.out_read = 0;
        self.out_len = 0;
    }

    /// Processes exactly one hop (`hop_size` samples) through Window -> FFT -> `spectral_fn` ->
    /// IFFT -> Window -> Overlap-Add, performing **zero heap allocations**.
    ///
    /// Note: `spectral_fn` receives the full `fft_size` complex spectrum. For real-valued
    /// time-domain outputs, apply conjugate-symmetric gains (`spectrum[N - k] = spectrum[k].conj()`
    /// or multiply both `k` and `N - k` by the same real gain $G[k]$).
    pub fn process_hop<F>(&mut self, input_hop: &[f32], output_hop: &mut [f32], mut spectral_fn: F)
    where
        F: FnMut(&mut [Complex]),
    {
        let h = self.hop_size;
        let n = self.fft_size;
        debug_assert_eq!(input_hop.len(), h);
        debug_assert_eq!(output_hop.len(), h);

        // Shift analysis window left by hop_size and append new input hop.
        self.analysis_frame.copy_within(h..n, 0);
        self.analysis_frame[h..n].copy_from_slice(input_hop);

        // Apply square-root Hann analysis window into complex scratch buffer.
        for i in 0..n {
            self.spectrum[i] = Complex::new(self.analysis_frame[i] * self.window[i], 0.0);
        }

        // Forward FFT
        self.plan.forward(&mut self.spectrum);

        // User frequency-domain processing callback (Noise Suppression, EQ, etc.)
        spectral_fn(&mut self.spectrum);

        // Inverse FFT
        self.plan.inverse(&mut self.spectrum);

        // Synthesis window + 50% overlap-add
        for (i, out) in output_hop.iter_mut().enumerate().take(h) {
            let synth_first = self.spectrum[i].re * self.window[i];
            *out = self.ola_tail[i] + synth_first;
            self.ola_tail[i] = self.spectrum[i + h].re * self.window[i + h];
        }
    }

    /// Processes an arbitrary-length frame slice (`input.len() == output.len()`) with
    /// **zero heap allocations** by staging samples through pre-allocated hop FIFOs.
    pub fn process_frame<F>(&mut self, input: &[f32], output: &mut [f32], mut spectral_fn: F)
    where
        F: FnMut(&mut [Complex]),
    {
        debug_assert_eq!(input.len(), output.len());
        let h = self.hop_size;
        let cap = self.out_fifo.len();

        // Fast path: aligned exact-hop processing when staging FIFOs are empty.
        if self.in_fill == 0 && self.out_len == 0 && input.len().is_multiple_of(h) {
            for (in_chunk, out_chunk) in input.chunks_exact(h).zip(output.chunks_exact_mut(h)) {
                self.process_hop(in_chunk, out_chunk, &mut spectral_fn);
            }
            return;
        }

        for i in 0..input.len() {
            self.in_fifo[self.in_fill] = input[i];
            self.in_fill += 1;

            if self.in_fill == h {
                // Execute one STFT hop using disjoint borrows via split helper.
                let n = self.fft_size;
                self.analysis_frame.copy_within(h..n, 0);
                self.analysis_frame[h..n].copy_from_slice(&self.in_fifo);
                for k in 0..n {
                    self.spectrum[k] = Complex::new(self.analysis_frame[k] * self.window[k], 0.0);
                }
                self.plan.forward(&mut self.spectrum);
                spectral_fn(&mut self.spectrum);
                self.plan.inverse(&mut self.spectrum);
                for k in 0..h {
                    let s0 = self.spectrum[k].re * self.window[k];
                    self.hop_out_scratch[k] = self.ola_tail[k] + s0;
                    self.ola_tail[k] = self.spectrum[k + h].re * self.window[k + h];
                }

                // Push produced hop into circular output FIFO.
                for k in 0..h {
                    let write_idx = (self.out_read + self.out_len) % cap;
                    self.out_fifo[write_idx] = self.hop_out_scratch[k];
                    if self.out_len < cap {
                        self.out_len += 1;
                    } else {
                        self.out_read = (self.out_read + 1) % cap;
                    }
                }
                self.in_fill = 0;
            }

            if self.out_len > 0 {
                output[i] = self.out_fifo[self.out_read];
                self.out_read = (self.out_read + 1) % cap;
                self.out_len -= 1;
            } else {
                output[i] = 0.0;
            }
        }
    }
}
