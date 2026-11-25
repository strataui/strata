//! Multi-feature real-time Voice Activity Detector (VAD) combining 300–3400 Hz speech-band
//! adaptive SNR, spectral flatness (Wiener entropy), zero-crossing rate, and hangover hysteresis.
//!
//! Performs **zero heap allocations** inside [`VoiceActivityDetector::analyze_frame`].

use crate::fft::{Complex, FftPlan};
use std::f32::consts::PI;

/// Configuration for the multi-feature [`VoiceActivityDetector`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VadConfig {
    /// Sample rate in Hz (`8000`–`48000`).
    pub sample_rate: u32,
    /// Analysis FFT size (power of two; default `256` at 16 kHz, `512` at 48 kHz).
    pub fft_size: usize,
    /// Speech-band SNR threshold in dB above tracked noise floor (default: `5.5 dB`).
    pub snr_threshold_db: f32,
    /// Absolute minimum speech-band dBFS to prevent false triggers on pure digital silence (`-52.0`).
    pub min_energy_dbfs: f32,
    /// Hangover duration in milliseconds keeping `is_speech == true` across brief inter-syllable gaps.
    pub hangover_ms: f32,
    /// Consecutive speech frames required to transition from silence -> speech (`1` or `2`).
    pub attack_frames: u32,
}

impl VadConfig {
    pub fn new(sample_rate: u32) -> Self {
        let sr = sample_rate.max(8_000);
        let fft_size = if sr >= 32_000 { 512 } else { 256 };
        Self {
            sample_rate: sr,
            fft_size,
            snr_threshold_db: 5.5,
            min_energy_dbfs: -52.0,
            hangover_ms: 180.0,
            attack_frames: 2,
        }
    }
}

impl Default for VadConfig {
    fn default() -> Self {
        Self::new(48_000)
    }
}

/// Output decision and acoustic telemetry produced for each analyzed frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct VadDecision {
    /// Final smoothed voice activity decision after attack and hangover hysteresis.
    pub is_speech: bool,
    /// Continuous speech probability estimate in `[0.0, 1.0]`.
    pub probability: f32,
    /// Speech-band (300–3400 Hz) RMS level in dBFS.
    pub speech_band_energy_db: f32,
    /// Estimated instantaneous speech-band SNR over tracked noise floor in dB.
    pub snr_db: f32,
    /// Spectral flatness (Wiener entropy) in `[0.0, 1.0]` (`0` = tonal speech, `1` = white noise).
    pub spectral_flatness: f32,
    /// Frame zero-crossing rate in `[0.0, 1.0]`.
    pub zero_crossing_rate: f32,
}

/// Zero-allocation multi-feature Voice Activity Detector.
pub struct VoiceActivityDetector {
    config: VadConfig,
    plan: FftPlan,
    window: Vec<f32>,
    spectrum: Vec<Complex>,
    bin_low: usize,
    bin_high: usize,
    noise_floor_power: f32,
    initialized: bool,
    consecutive_speech: u32,
    hangover_remaining: u32,
    is_speech: bool,
}

impl VoiceActivityDetector {
    pub fn new(mut config: VadConfig) -> Self {
        config.sample_rate = config.sample_rate.max(8_000);
        if !config.fft_size.is_power_of_two() || config.fft_size < 64 {
            config.fft_size = 256;
        }
        let n = config.fft_size;
        let mut window = Vec::with_capacity(n);
        for i in 0..n {
            window.push(0.5 * (1.0 - (2.0 * PI * i as f32 / n as f32).cos()));
        }

        let bin_hz = config.sample_rate as f32 / n as f32;
        let bin_low = ((200.0 / bin_hz).floor() as usize).clamp(1, n / 2 - 2);
        let bin_high = ((3_400.0 / bin_hz).ceil() as usize).clamp(bin_low + 1, n / 2 - 1);

        Self {
            config,
            plan: FftPlan::new(n),
            window,
            spectrum: vec![Complex::zero(); n],
            bin_low,
            bin_high,
            noise_floor_power: 1e-7,
            initialized: false,
            consecutive_speech: 0,
            hangover_remaining: 0,
            is_speech: false,
        }
    }

    pub fn reset(&mut self) {
        self.noise_floor_power = 1e-7;
        self.initialized = false;
        self.consecutive_speech = 0;
        self.hangover_remaining = 0;
        self.is_speech = false;
    }

    /// Analyzes a mono audio frame and returns a [`VadDecision`] with **zero heap allocations**.
    pub fn analyze_frame(&mut self, frame: &[f32]) -> VadDecision {
        if frame.is_empty() {
            return VadDecision::default();
        }

        let n = self.config.fft_size;
        // 1. Compute Zero-Crossing Rate (ZCR)
        let mut crossings = 0usize;
        for i in 1..frame.len() {
            if (frame[i] >= 0.0) != (frame[i - 1] >= 0.0) {
                crossings += 1;
            }
        }
        let zcr = crossings as f32 / frame.len().max(1) as f32;

        // 2. Window & forward FFT (zero-padded or truncated to `fft_size`)
        let copy_len = frame.len().min(n);
        for i in 0..copy_len {
            self.spectrum[i] = Complex::new(frame[i] * self.window[i], 0.0);
        }
        for i in copy_len..n {
            self.spectrum[i] = Complex::zero();
        }
        self.plan.forward(&mut self.spectrum);

        // 3. Speech-band energy & spectral flatness (geometric mean / arithmetic mean)
        let band_bins = (self.bin_high - self.bin_low + 1).max(1);
        let mut band_power_sum = 0.0f32;
        let mut log_power_sum = 0.0f32;
        let norm = (n as f32) * (n as f32);

        for k in self.bin_low..=self.bin_high {
            let p = (self.spectrum[k].norm_sq() / norm).max(1e-12);
            band_power_sum += p;
            log_power_sum += p.ln();
        }

        let arith_mean = band_power_sum / band_bins as f32;
        let geom_mean = (log_power_sum / band_bins as f32).exp();
        let flatness = (geom_mean / arith_mean.max(1e-12)).clamp(0.0, 1.0);

        let speech_band_energy_db = (10.0 * band_power_sum.max(1e-12).log10()).clamp(-120.0, 0.0);

        // 4. Adaptive noise floor tracking (only tracks down quickly, rises very slowly during non-tonal frames)
        if !self.initialized {
            // Seed floor safely low if initial frame is already tonal speech; otherwise lock to noise.
            self.noise_floor_power = if flatness < 0.35 && speech_band_energy_db > -35.0 {
                (band_power_sum * 0.05).max(1e-8)
            } else {
                band_power_sum.max(1e-8)
            };
            self.initialized = true;
        } else if band_power_sum < self.noise_floor_power {
            // Track noise drops rapidly
            self.noise_floor_power = 0.80 * self.noise_floor_power + 0.20 * band_power_sum;
        } else if flatness > 0.45 {
            // Flat broadband noise: adapt floor smoothly upward
            self.noise_floor_power = 0.96 * self.noise_floor_power + 0.04 * band_power_sum;
        } else {
            // Tonal / potential speech: crawl very slowly so long speech never raises floor
            self.noise_floor_power = 0.998 * self.noise_floor_power + 0.002 * band_power_sum;
        }

        let snr_db =
            (10.0 * (band_power_sum / self.noise_floor_power.max(1e-12)).log10()).clamp(-20.0, 60.0);

        // 5. Combine acoustic cues into continuous speech probability [0, 1]
        let energy_ok = speech_band_energy_db >= self.config.min_energy_dbfs;
        let snr_score = ((snr_db - (self.config.snr_threshold_db - 2.0)) / 8.0).clamp(0.0, 1.0);
        // Human voiced/formant speech has low spectral flatness (< 0.45), whereas white noise is ~0.6-0.95.
        let tonality_score = ((0.62 - flatness) / 0.42).clamp(0.0, 1.0);
        // Human speech ZCR sits well below pure high-frequency hiss (< 0.38).
        let zcr_score = if zcr < 0.42 { 1.0 } else { 0.35 };

        let probability = if !energy_ok {
            0.0
        } else {
            (0.60 * snr_score + 0.30 * tonality_score + 0.10 * zcr_score).clamp(0.0, 1.0)
        };

        let raw_speech = energy_ok
            && (snr_db >= self.config.snr_threshold_db
                || (tonality_score > 0.65 && snr_db >= self.config.snr_threshold_db * 0.6));

        let frame_ms = (frame.len() as f32 * 1000.0) / self.config.sample_rate as f32;
        let hangover_frames = (self.config.hangover_ms / frame_ms.max(1.0)).ceil() as u32;

        if raw_speech {
            self.consecutive_speech = self.consecutive_speech.saturating_add(1);
            if self.consecutive_speech >= self.config.attack_frames.max(1) {
                self.is_speech = true;
                self.hangover_remaining = hangover_frames;
            }
        } else {
            self.consecutive_speech = 0;
            if self.hangover_remaining > 0 {
                self.hangover_remaining -= 1;
            } else {
                self.is_speech = false;
            }
        }

        VadDecision {
            is_speech: self.is_speech,
            probability,
            speech_band_energy_db,
            snr_db,
            spectral_flatness: flatness,
            zero_crossing_rate: zcr,
        }
    }
}
