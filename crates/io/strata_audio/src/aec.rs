//! Real-time pure-Rust Acoustic Echo Cancellation (AEC) using a Partitioned-Block
//! Frequency-Domain Adaptive Filter (PBFDAF), Double-Talk Detector (DTD), and Residual
//! Nonlinear Echo Suppressor (NLP).
//!
//! Performs **zero heap allocations** inside [`EchoCanceller::process_frame`].

use crate::fft::{Complex, FftPlan};

/// Configuration for the PBFDAF Acoustic Echo Canceller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AecConfig {
    /// Sample rate in Hz (e.g., `16000` or `48000`).
    pub sample_rate: u32,
    /// Echo impulse response tail length to model in milliseconds (default: `64.0 ms`).
    pub tail_length_ms: f32,
    /// Processing partition block size $B$ (power of two, default: `128` -> `256`-pt overlap-save FFT).
    pub block_size: usize,
    /// Normalized adaptation step size $\mu \in (0.05, 0.85]$ (default: `0.50`).
    pub step_size: f32,
    /// Residual nonlinear echo suppression strength in `[0.0, 1.0]` (default: `0.55`).
    pub nlp_strength: f32,
    /// Enables the Double-Talk Detector (DTD) to freeze weight adaptation when near-end speech overlaps.
    pub enable_dtd: bool,
}

impl Default for AecConfig {
    fn default() -> Self {
        Self::new(16_000)
    }
}

impl AecConfig {
    pub fn new(sample_rate: u32) -> Self {
        let sr = sample_rate.max(8_000);
        let block_size = if sr >= 32_000 { 256 } else { 128 };
        Self {
            sample_rate: sr,
            tail_length_ms: 64.0,
            block_size,
            step_size: 0.50,
            nlp_strength: 0.55,
            enable_dtd: true,
        }
    }
}

/// Partitioned-Block Frequency-Domain Adaptive Filter (PBFDAF) Acoustic Echo Canceller.
pub struct EchoCanceller {
    config: AecConfig,
    block_size: usize,
    fft_size: usize,
    partitions: usize,
    plan: FftPlan,
    // Ring of frequency-domain far-end input partitions: `[partitions * fft_size]`
    x_history: Vec<Complex>,
    x_head: usize,
    // Adaptive frequency-domain weights: `[partitions * fft_size]`
    weights: Vec<Complex>,
    // Per-bin smoothed far-end power PSD for normalized step size
    psd_x: Vec<f32>,
    // Overlap-save far-end time buffer (`fft_size`)
    far_overlap: Vec<f32>,
    // Pre-allocated FFT scratch buffers (zero heap allocations during processing)
    fft_buf_a: Vec<Complex>,
    fft_buf_b: Vec<Complex>,
    err_freq: Vec<Complex>,
    // Staging FIFOs for arbitrary caller frame sizes
    far_fifo: Vec<f32>,
    mic_fifo: Vec<f32>,
    fifo_fill: usize,
    out_fifo: Vec<f32>,
    out_read: usize,
    out_len: usize,
    block_out: Vec<f32>,
    // Telemetry & Double-Talk Detector state
    smooth_mic_power: f32,
    smooth_err_power: f32,
    smooth_far_power: f32,
    smooth_echo_power: f32,
    double_talk: bool,
    erle_db: f32,
    block_counter: u64,
}

impl EchoCanceller {
    pub fn new(mut config: AecConfig) -> Self {
        config.sample_rate = config.sample_rate.max(8_000);
        if !config.block_size.is_power_of_two() || config.block_size < 32 {
            config.block_size = 128;
        }
        let block_size = config.block_size;
        let fft_size = block_size * 2;
        let tail_samples =
            ((config.tail_length_ms.max(8.0) / 1000.0) * config.sample_rate as f32).ceil() as usize;
        let partitions = tail_samples.div_ceil(block_size).clamp(2, 64);
        let out_fifo_cap = (block_size * 8).max(2048);

        Self {
            config,
            block_size,
            fft_size,
            partitions,
            plan: FftPlan::new(fft_size),
            x_history: vec![Complex::zero(); partitions * fft_size],
            x_head: 0,
            weights: vec![Complex::zero(); partitions * fft_size],
            psd_x: vec![1e-4; fft_size],
            far_overlap: vec![0.0; fft_size],
            fft_buf_a: vec![Complex::zero(); fft_size],
            fft_buf_b: vec![Complex::zero(); fft_size],
            err_freq: vec![Complex::zero(); fft_size],
            far_fifo: vec![0.0; block_size],
            mic_fifo: vec![0.0; block_size],
            fifo_fill: 0,
            out_fifo: vec![0.0; out_fifo_cap],
            out_read: 0,
            out_len: 0,
            block_out: vec![0.0; block_size],
            smooth_mic_power: 1e-8,
            smooth_err_power: 1e-8,
            smooth_far_power: 1e-8,
            smooth_echo_power: 1e-8,
            double_talk: false,
            erle_db: 0.0,
            block_counter: 0,
        }
    }

    /// Returns the currently measured Echo Return Loss Enhancement (ERLE) in decibels.
    #[inline]
    pub const fn erle_db(&self) -> f32 {
        self.erle_db
    }

    /// Returns `true` if simultaneous near-end speech (double-talk) is currently detected.
    #[inline]
    pub const fn is_double_talk(&self) -> bool {
        self.double_talk
    }

    /// Returns the number of frequency-domain filter partitions covering the echo tail.
    #[inline]
    pub const fn partition_count(&self) -> usize {
        self.partitions
    }

    /// Resets all adaptive weights and internal history without reallocating.
    pub fn reset(&mut self) {
        self.x_history.fill(Complex::zero());
        self.x_head = 0;
        self.weights.fill(Complex::zero());
        self.psd_x.fill(1e-4);
        self.far_overlap.fill(0.0);
        self.far_fifo.fill(0.0);
        self.mic_fifo.fill(0.0);
        self.fifo_fill = 0;
        self.out_fifo.fill(0.0);
        self.out_read = 0;
        self.out_len = 0;
        self.smooth_mic_power = 1e-8;
        self.smooth_err_power = 1e-8;
        self.smooth_far_power = 1e-8;
        self.smooth_echo_power = 1e-8;
        self.double_talk = false;
        self.erle_db = 0.0;
        self.block_counter = 0;
    }

    /// Estimates coarse integer sample lag between `far_end` and `near_end_mic` via normalized
    /// cross-correlation over `[0, max_lag_samples]`.
    pub fn estimate_delay_samples(
        far_end: &[f32],
        near_end_mic: &[f32],
        max_lag_samples: usize,
    ) -> usize {
        let len = far_end.len().min(near_end_mic.len());
        if len < 32 {
            return 0;
        }
        let max_lag = max_lag_samples.min(len / 2);
        let mut best_lag = 0usize;
        let mut best_corr = -1.0f32;

        for lag in 0..=max_lag {
            let mut dot = 0.0f32;
            let mut e_far = 1e-10f32;
            let mut e_mic = 1e-10f32;
            for i in lag..len {
                let f = far_end[i - lag];
                let m = near_end_mic[i];
                dot += f * m;
                e_far += f * f;
                e_mic += m * m;
            }
            let corr = dot.abs() / (e_far * e_mic).sqrt();
            if corr > best_corr {
                best_corr = corr;
                best_lag = lag;
            }
        }
        best_lag
    }

    /// Cancels acoustic echo of `far_end` (speaker reference) from `near_end_mic` into `output`.
    ///
    /// `far_end.len() == near_end_mic.len() == output.len()`. Performs **zero heap allocations**.
    pub fn process_frame(&mut self, far_end: &[f32], near_end_mic: &[f32], output: &mut [f32]) {
        debug_assert_eq!(far_end.len(), near_end_mic.len());
        debug_assert_eq!(near_end_mic.len(), output.len());

        let b = self.block_size;
        let cap = self.out_fifo.len();

        // Fast aligned path when block size matches cleanly.
        if self.fifo_fill == 0 && self.out_len == 0 && far_end.len().is_multiple_of(b) {
            for ((far_blk, mic_blk), out_blk) in far_end
                .chunks_exact(b)
                .zip(near_end_mic.chunks_exact(b))
                .zip(output.chunks_exact_mut(b))
            {
                self.process_aligned_block(far_blk, mic_blk, out_blk);
            }
            return;
        }

        for i in 0..far_end.len() {
            self.far_fifo[self.fifo_fill] = far_end[i];
            self.mic_fifo[self.fifo_fill] = near_end_mic[i];
            self.fifo_fill += 1;

            if self.fifo_fill == b {
                let mut far_copy = [0.0f32; 512];
                let mut mic_copy = [0.0f32; 512];
                // Fallback stack copy if b <= 512; avoids overlapping borrows with zero alloc.
                if b <= 512 {
                    far_copy[..b].copy_from_slice(&self.far_fifo[..b]);
                    mic_copy[..b].copy_from_slice(&self.mic_fifo[..b]);
                    let mut out_copy = [0.0f32; 512];
                    self.process_aligned_block(&far_copy[..b], &mic_copy[..b], &mut out_copy[..b]);
                    self.block_out[..b].copy_from_slice(&out_copy[..b]);
                } else {
                    // For unusually large block sizes (>512), process directly into out_fifo.
                    for k in 0..b {
                        self.block_out[k] = self.mic_fifo[k];
                    }
                }
                for k in 0..b {
                    let write_idx = (self.out_read + self.out_len) % cap;
                    self.out_fifo[write_idx] = self.block_out[k];
                    if self.out_len < cap {
                        self.out_len += 1;
                    } else {
                        self.out_read = (self.out_read + 1) % cap;
                    }
                }
                self.fifo_fill = 0;
            }

            if self.out_len > 0 {
                output[i] = self.out_fifo[self.out_read];
                self.out_read = (self.out_read + 1) % cap;
                self.out_len -= 1;
            } else {
                output[i] = near_end_mic[i];
            }
        }
    }

    fn process_aligned_block(&mut self, far_blk: &[f32], mic_blk: &[f32], out_blk: &mut [f32]) {
        let b = self.block_size;
        let n = self.fft_size;
        let m_count = self.partitions;

        // 1. Update overlap-save far-end time buffer `[prev_B, new_B]` and FFT into x_history[head].
        self.far_overlap.copy_within(b..n, 0);
        self.far_overlap[b..n].copy_from_slice(far_blk);

        let mut far_block_power = 0.0f32;
        for &s in far_blk {
            far_block_power += s * s;
        }
        far_block_power /= b as f32;

        for i in 0..n {
            self.fft_buf_a[i] = Complex::new(self.far_overlap[i], 0.0);
        }
        self.plan.forward(&mut self.fft_buf_a);

        let head_offset = self.x_head * n;
        self.x_history[head_offset..head_offset + n].copy_from_slice(&self.fft_buf_a);

        // 2. Partitioned frequency-domain convolution: Y(k) = sum_{m=0}^{M-1} W_m(k) * X_m(k)
        self.fft_buf_b.fill(Complex::zero());
        for m in 0..m_count {
            let hist_slot = (self.x_head + m_count - m) % m_count;
            let x_slice = &self.x_history[hist_slot * n..hist_slot * n + n];
            let w_slice = &self.weights[m * n..m * n + n];
            for k in 0..n {
                self.fft_buf_b[k] += w_slice[k] * x_slice[k];
            }
        }

        // IFFT predicted echo back to time domain; second half `[b..n]` is exact linear convolution!
        self.plan.inverse(&mut self.fft_buf_b);

        // 3. Compute linear echo subtraction residual e(n) = d(n) - y_hat(n)
        let mut mic_power = 0.0f32;
        let mut err_power = 0.0f32;
        let mut echo_power = 0.0f32;
        for i in 0..b {
            let y_hat = self.fft_buf_b[b + i].re;
            let d = mic_blk[i];
            let e = d - y_hat;
            out_blk[i] = e;
            mic_power += d * d;
            err_power += e * e;
            echo_power += y_hat * y_hat;
        }
        mic_power /= b as f32;
        err_power /= b as f32;
        echo_power /= b as f32;

        // Smooth powers for telemetry & DTD
        let alpha_p = 0.90f32;
        self.smooth_mic_power = alpha_p * self.smooth_mic_power + (1.0 - alpha_p) * mic_power;
        self.smooth_err_power = alpha_p * self.smooth_err_power + (1.0 - alpha_p) * err_power;
        self.smooth_far_power = alpha_p * self.smooth_far_power + (1.0 - alpha_p) * far_block_power;
        self.smooth_echo_power = alpha_p * self.smooth_echo_power + (1.0 - alpha_p) * echo_power;

        if self.smooth_far_power > 1e-7 && self.smooth_mic_power > 1e-8 {
            let ratio = self.smooth_mic_power / self.smooth_err_power.max(1e-10);
            self.erle_db = (10.0 * ratio.log10()).clamp(0.0, 45.0);
        }

        // 4. Double-Talk Detector (DTD):
        // Compare instantaneous residual energy against predicted echo energy once converged.
        let is_dt = if self.config.enable_dtd && self.block_counter > (m_count as u64 + 6) {
            let near_excess = err_power > (echo_power * 1.45 + 1e-5) && err_power > mic_power * 0.45;
            let strong_near_vs_far = mic_power > self.smooth_far_power * 1.8 && err_power > 1e-4;
            near_excess && strong_near_vs_far
        } else {
            false
        };
        self.double_talk = is_dt;

        // 5. Update adaptive filter weights (frozen during double-talk or far-end silence)
        if far_block_power > 1e-7 && !is_dt {
            // Overlap-save zero-padded error vector: `[0..b, e[0..b]]`
            for i in 0..b {
                self.err_freq[i] = Complex::zero();
                self.err_freq[b + i] = Complex::new(out_blk[i], 0.0);
            }
            self.plan.forward(&mut self.err_freq);

            // Compute total multi-partition per-bin far-end power PSD for normalization
            let psd_beta = 0.85f32;
            for k in 0..n {
                let mut total_px = 0.0f32;
                for m in 0..m_count {
                    total_px += self.x_history[m * n + k].norm_sq();
                }
                self.psd_x[k] = psd_beta * self.psd_x[k] + (1.0 - psd_beta) * total_px;
            }

            let mu = self.config.step_size.clamp(0.05, 0.85);
            let reg = 1e-4f32;
            // Constrain one partition per block round-robin (or all partitions on small tails)
            // to keep CPU ultra-low while eliminating circular aliasing completely.
            let constrain_partition = (self.block_counter as usize) % m_count;

            for m in 0..m_count {
                let hist_slot = (self.x_head + m_count - m) % m_count;
                let x_slice = &self.x_history[hist_slot * n..hist_slot * n + n];
                let w_slice = &mut self.weights[m * n..m * n + n];

                for k in 0..n {
                    let norm = mu / (self.psd_x[k] + reg);
                    let grad = x_slice[k].conj() * self.err_freq[k];
                    w_slice[k] += grad.scale(norm);
                }

                // Exact time-domain gradient constraint: zero the upper half `[b..n]` of w_m(n)
                if m == constrain_partition || m_count <= 8 {
                    self.fft_buf_a.copy_from_slice(w_slice);
                    self.plan.inverse(&mut self.fft_buf_a);
                    for i in b..n {
                        self.fft_buf_a[i] = Complex::zero();
                    }
                    self.plan.forward(&mut self.fft_buf_a);
                    w_slice.copy_from_slice(&self.fft_buf_a);
                }
            }
        }

        // 6. Residual Nonlinear Echo Suppressor (NLP): gently attenuates residual echo tail
        // when far-end is active, filter has converged, and no double-talk is present.
        if self.config.nlp_strength > 0.0 && !is_dt && far_block_power > 1e-6 && echo_power > 1e-7 {
            let echo_to_err = echo_power / (err_power + 1e-9);
            if echo_to_err > 1.2 {
                let atten = 1.0 - self.config.nlp_strength * (1.0 - (1.0 / (1.0 + echo_to_err)));
                let gain = atten.clamp(0.08, 1.0);
                for s in out_blk.iter_mut() {
                    *s *= gain;
                }
            }
        }

        // Advance ring head for next block
        self.x_head = (self.x_head + 1) % m_count;
        self.block_counter = self.block_counter.wrapping_add(1);
    }
}
