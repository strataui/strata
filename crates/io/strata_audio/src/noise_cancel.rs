//! Real-time pure-Rust Adaptive Noise Cancellation / Speech Enhancement (MCRA + Ephraim-Malah
//! Decision-Directed MMSE-STSA / Wiener Spectral Suppression + Transient Click Damper).
//!
//! Designed for zero heap allocations on real-time audio threads and < 10 ms algorithmic latency.

use crate::fft::Complex;
use crate::stft::StftProcessor;

/// Ergonomic presets for real-time adaptive noise suppression.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NoiseSuppressionPreset {
    /// Complete pass-through bypass.
    Off,
    /// Gentle room hiss / light fan reduction (`-12 dB` floor).
    Low,
    /// Balanced conference call suppression (`-20 dB` floor).
    Medium,
    /// Aggressive noisy environment / cafe / HVAC suppression (`-28 dB` floor).
    High,
    /// Studio / broadcast / industrial cockpit suppression (`-36 dB` floor + click suppression).
    Broadcast,
    /// Explicit fine-grained configuration.
    Custom(NoiseSuppressionConfig),
}

/// Fine-grained parameters controlling the MCRA + Decision-Directed noise suppressor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoiseSuppressionConfig {
    /// Input audio sample rate in Hz (e.g., `16000` or `48000`).
    pub sample_rate: u32,
    /// STFT FFT size (power of two; defaults to `512` at 48 kHz = 5.33 ms latency, `256` at 16 kHz = 8 ms).
    pub fft_size: usize,
    /// Maximum attenuation floor in dB (negative value, e.g., `-22.0`).
    pub min_gain_db: f32,
    /// Spectral over-subtraction factor (`1.0` = standard Wiener, `1.5..=2.4` = aggressive).
    pub over_subtraction: f32,
    /// Ephraim-Malah Decision-Directed a priori SNR smoothing (`0.95..=0.985`).
    pub dd_alpha: f32,
    /// Base noise PSD recursive smoothing factor (`0.80..=0.95`).
    pub noise_alpha: f32,
    /// Subtle comfort noise floor in dBFS (e.g., `-75.0`, or `-120.0` to disable).
    pub comfort_noise_dbfs: f32,
    /// Enables broadband transient impulse / keyboard-click attenuation.
    pub transient_suppression: bool,
}

impl NoiseSuppressionConfig {
    pub fn from_preset(preset: NoiseSuppressionPreset, sample_rate: u32) -> Self {
        let sr = sample_rate.max(8_000);
        let fft_size = if sr >= 32_000 { 512 } else { 256 };
        match preset {
            NoiseSuppressionPreset::Off => Self {
                sample_rate: sr,
                fft_size,
                min_gain_db: 0.0,
                over_subtraction: 0.0,
                dd_alpha: 0.98,
                noise_alpha: 0.85,
                comfort_noise_dbfs: -120.0,
                transient_suppression: false,
            },
            NoiseSuppressionPreset::Low => Self {
                sample_rate: sr,
                fft_size,
                min_gain_db: -12.0,
                over_subtraction: 1.15,
                dd_alpha: 0.97,
                noise_alpha: 0.85,
                comfort_noise_dbfs: -78.0,
                transient_suppression: false,
            },
            NoiseSuppressionPreset::Medium => Self {
                sample_rate: sr,
                fft_size,
                min_gain_db: -22.0,
                over_subtraction: 1.6,
                dd_alpha: 0.98,
                noise_alpha: 0.85,
                comfort_noise_dbfs: -82.0,
                transient_suppression: true,
            },
            NoiseSuppressionPreset::High => Self {
                sample_rate: sr,
                fft_size,
                min_gain_db: -30.0,
                over_subtraction: 2.1,
                dd_alpha: 0.982,
                noise_alpha: 0.84,
                comfort_noise_dbfs: -88.0,
                transient_suppression: true,
            },
            NoiseSuppressionPreset::Broadcast => Self {
                sample_rate: sr,
                fft_size,
                min_gain_db: -38.0,
                over_subtraction: 2.5,
                dd_alpha: 0.985,
                noise_alpha: 0.82,
                comfort_noise_dbfs: -96.0,
                transient_suppression: true,
            },
            NoiseSuppressionPreset::Custom(mut cfg) => {
                cfg.sample_rate = cfg.sample_rate.max(8_000);
                if !cfg.fft_size.is_power_of_two() || cfg.fft_size < 32 {
                    cfg.fft_size = fft_size;
                }
                cfg
            }
        }
    }
}

/// Real-time adaptive noise suppressor using STFT + MCRA noise tracking + Ephraim-Malah
/// Decision-Directed Wiener/MMSE-STSA spectral gain estimation.
///
/// Performs **zero heap allocations** inside [`Self::process_frame`].
pub struct NoiseSuppressor {
    config: NoiseSuppressionConfig,
    enabled: bool,
    stft: StftProcessor,
    bins: usize,
    // MCRA state arrays (all pre-allocated to `bins = fft_size / 2 + 1`)
    noise_psd: Vec<f32>,
    smoothed_psd: Vec<f32>,
    freq_smoothed: Vec<f32>,
    s_min: Vec<f32>,
    s_tmp: Vec<f32>,
    speech_prob: Vec<f32>,
    prev_gain: Vec<f32>,
    prev_post_snr: Vec<f32>,
    gain_buf: Vec<f32>,
    // Counters & telemetry
    frame_index: u64,
    min_window_frames: u64,
    prev_hf_energy: f32,
    rng_state: u32,
    last_snr_db: f32,
    last_noise_rms: f32,
}

impl NoiseSuppressor {
    /// Creates a new noise suppressor configured by a preset and sample rate.
    pub fn with_preset(preset: NoiseSuppressionPreset, sample_rate: u32) -> Self {
        let enabled = !matches!(preset, NoiseSuppressionPreset::Off);
        let config = NoiseSuppressionConfig::from_preset(preset, sample_rate);
        let mut ns = Self::new(config);
        ns.enabled = enabled;
        ns
    }

    /// Creates a new noise suppressor with explicit [`NoiseSuppressionConfig`].
    pub fn new(config: NoiseSuppressionConfig) -> Self {
        let fft_size = config.fft_size;
        let bins = fft_size / 2 + 1;
        let hop_ms = (fft_size as f32 * 0.5 * 1000.0) / config.sample_rate.max(8_000) as f32;
        // MCRA local-minimum window ~0.75 seconds so noise adapts smoothly while preserving long vowels.
        let min_window_frames = (750.0 / hop_ms.max(1.0)).round().clamp(25.0, 200.0) as u64;

        Self {
            config,
            enabled: config.min_gain_db < -0.01 && config.over_subtraction > 0.0,
            stft: StftProcessor::new(fft_size),
            bins,
            noise_psd: vec![1e-8; bins],
            smoothed_psd: vec![1e-8; bins],
            freq_smoothed: vec![1e-8; bins],
            s_min: vec![1e-8; bins],
            s_tmp: vec![1e-8; bins],
            speech_prob: vec![0.0; bins],
            prev_gain: vec![1.0; bins],
            prev_post_snr: vec![1.0; bins],
            gain_buf: vec![1.0; bins],
            frame_index: 0,
            min_window_frames,
            prev_hf_energy: 1e-8,
            rng_state: 0x9E37_79B9,
            last_snr_db: 0.0,
            last_noise_rms: 0.0,
        }
    }

    /// Updates suppression strength on the fly without reallocating buffers.
    pub fn set_preset(&mut self, preset: NoiseSuppressionPreset) {
        self.enabled = !matches!(preset, NoiseSuppressionPreset::Off);
        let updated = NoiseSuppressionConfig::from_preset(preset, self.config.sample_rate);
        self.config.min_gain_db = updated.min_gain_db;
        self.config.over_subtraction = updated.over_subtraction;
        self.config.dd_alpha = updated.dd_alpha;
        self.config.noise_alpha = updated.noise_alpha;
        self.config.comfort_noise_dbfs = updated.comfort_noise_dbfs;
        self.config.transient_suppression = updated.transient_suppression;
    }

    /// Returns whether noise cancellation is currently active.
    #[inline]
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Returns the algorithmic frame latency in milliseconds (`hop_size / sample_rate`).
    #[inline]
    pub fn algorithmic_latency_ms(&self) -> f32 {
        self.stft.latency_ms(self.config.sample_rate)
    }

    /// Returns the most recently estimated wideband SNR in dB.
    #[inline]
    pub const fn estimated_snr_db(&self) -> f32 {
        self.last_snr_db
    }

    /// Returns the estimated ambient noise floor RMS magnitude.
    #[inline]
    pub const fn noise_floor_rms(&self) -> f32 {
        self.last_noise_rms
    }

    /// Resets adaptive noise trackers and STFT buffers without reallocating.
    pub fn reset(&mut self) {
        self.stft.reset();
        self.noise_psd.fill(1e-8);
        self.smoothed_psd.fill(1e-8);
        self.freq_smoothed.fill(1e-8);
        self.s_min.fill(1e-8);
        self.s_tmp.fill(1e-8);
        self.speech_prob.fill(0.0);
        self.prev_gain.fill(1.0);
        self.prev_post_snr.fill(1.0);
        self.gain_buf.fill(1.0);
        self.frame_index = 0;
        self.prev_hf_energy = 1e-8;
        self.last_snr_db = 0.0;
        self.last_noise_rms = 0.0;
    }

    /// Processes a mono frame (`input.len() == output.len()`) with **zero heap allocations**.
    pub fn process_frame(&mut self, input: &[f32], output: &mut [f32]) {
        debug_assert_eq!(input.len(), output.len());
        if !self.enabled {
            output.copy_from_slice(input);
            return;
        }

        // Pull field borrows cleanly so the STFT closure mutates state without reallocating.
        let bins = self.bins;
        let fft_size = self.config.fft_size;
        let min_gain = 10.0f32.powf(self.config.min_gain_db / 20.0).clamp(0.005, 1.0);
        let over_sub = self.config.over_subtraction.max(0.5);
        let dd_alpha = self.config.dd_alpha.clamp(0.5, 0.995);
        let alpha_d = self.config.noise_alpha.clamp(0.5, 0.99);
        let comfort_amp = if self.config.comfort_noise_dbfs <= -115.0 {
            0.0
        } else {
            10.0f32.powf(self.config.comfort_noise_dbfs / 20.0)
        };
        let transient_on = self.config.transient_suppression;
        let min_window_frames = self.min_window_frames;
        let sr = self.config.sample_rate as f32;

        let noise_psd = &mut self.noise_psd;
        let smoothed_psd = &mut self.smoothed_psd;
        let freq_smoothed = &mut self.freq_smoothed;
        let s_min = &mut self.s_min;
        let s_tmp = &mut self.s_tmp;
        let speech_prob = &mut self.speech_prob;
        let prev_gain = &mut self.prev_gain;
        let prev_post_snr = &mut self.prev_post_snr;
        let gain_buf = &mut self.gain_buf;
        let frame_index = &mut self.frame_index;
        let prev_hf_energy = &mut self.prev_hf_energy;
        let rng_state = &mut self.rng_state;
        let last_snr_db = &mut self.last_snr_db;
        let last_noise_rms = &mut self.last_noise_rms;

        self.stft.process_frame(input, output, |spectrum: &mut [Complex]| {
            let f_idx = *frame_index;
            *frame_index = f_idx.wrapping_add(1);

            // 1. Instantaneous power spectrum |Y(k, l)|^2 and 3-bin frequency smoothing.
            for k in 0..bins {
                let p = spectrum[k].norm_sq();
                let p_prev = if k > 0 { spectrum[k - 1].norm_sq() } else { p };
                let p_next = if k + 1 < bins {
                    spectrum[k + 1].norm_sq()
                } else {
                    p
                };
                freq_smoothed[k] = 0.25 * p_prev + 0.5 * p + 0.25 * p_next;
            }

            // 2. MCRA adaptive noise floor tracking.
            // Fast bootstrap over the first 6 frames (~32-48 ms) locks onto ambient noise quickly.
            let alpha_s = 0.80f32;
            let reset_min = f_idx > 0 && f_idx.is_multiple_of(min_window_frames);
            let mut total_sig_power = 0.0f32;
            let mut total_noise_power = 0.0f32;
            let mut hf_energy = 0.0f32;
            let hf_start_bin = ((3500.0 * fft_size as f32) / sr) as usize;

            for k in 0..bins {
                let inst_p = spectrum[k].norm_sq();
                total_sig_power += inst_p;
                if k >= hf_start_bin {
                    hf_energy += inst_p;
                }

                if f_idx == 0 {
                    smoothed_psd[k] = freq_smoothed[k].max(1e-10);
                    s_min[k] = smoothed_psd[k];
                    s_tmp[k] = smoothed_psd[k];
                    noise_psd[k] = inst_p.max(1e-10);
                } else {
                    smoothed_psd[k] =
                        alpha_s * smoothed_psd[k] + (1.0 - alpha_s) * freq_smoothed[k];

                    if reset_min {
                        s_min[k] = s_tmp[k].min(smoothed_psd[k]).max(1e-10);
                        s_tmp[k] = smoothed_psd[k].max(1e-10);
                    } else {
                        s_min[k] = s_min[k].min(smoothed_psd[k]).max(1e-10);
                        s_tmp[k] = s_tmp[k].min(smoothed_psd[k]).max(1e-10);
                    }

                    // Ratio of current smoothed energy to tracked local minimum.
                    let sr_ratio = smoothed_psd[k] / (s_min[k] + 1e-10);
                    // Speech presence indicator threshold delta = 4.0 (approx +6 dB above floor).
                    let indicator = if sr_ratio > 4.0 { 1.0f32 } else { 0.0f32 };
                    speech_prob[k] = 0.2 * speech_prob[k] + 0.8 * indicator;

                    // Early calibration phase learns noise floor rapidly unless strong speech burst.
                    let eff_alpha_d = if f_idx < 8 {
                        0.60 + 0.35 * speech_prob[k]
                    } else {
                        alpha_d + (1.0 - alpha_d) * speech_prob[k]
                    };
                    noise_psd[k] =
                        (eff_alpha_d * noise_psd[k] + (1.0 - eff_alpha_d) * inst_p).max(1e-10);
                }
                total_noise_power += noise_psd[k];
            }

            // 3. Optional broadband transient click/pop damper (mechanical keyboard clicks).
            let mut transient_scale = 1.0f32;
            if transient_on && f_idx > 4 {
                let hf_ratio = hf_energy / (*prev_hf_energy + 1e-9);
                if hf_ratio > 18.0 && hf_energy > total_noise_power * 0.25 {
                    transient_scale = 0.30;
                }
            }
            *prev_hf_energy = 0.85 * *prev_hf_energy + 0.15 * hf_energy;

            // 4. Ephraim-Malah Decision-Directed a priori SNR + Parametric Wiener gain.
            for k in 0..bins {
                let inst_p = spectrum[k].norm_sq();
                let lambda_d = (noise_psd[k] * over_sub).max(1e-10);
                let gamma = (inst_p / lambda_d).clamp(0.0, 10_000.0);

                // Decision-Directed a priori SNR estimate xi(k, l)
                let prev_clean_snr = prev_gain[k] * prev_gain[k] * prev_post_snr[k];
                let ml_snr = (gamma - 1.0).max(0.0);
                let xi = (dd_alpha * prev_clean_snr + (1.0 - dd_alpha) * ml_snr).max(1e-4);

                // Parametric MMSE/Wiener spectral amplitude gain:
                // G = (xi / (1 + xi)) with slight perceptual exponent smoothing to eliminate musical noise.
                let wiener = xi / (1.0 + xi);
                // Smooth perceptual suppression curve between Wiener amplitude & MMSE-STSA.
                let mut g = wiener.powf(0.85);

                // Apply soft speech presence probability weighting for residual noise pockets.
                let q_floor = 0.25 + 0.75 * speech_prob[k];
                g *= q_floor.max(wiener);

                if k >= hf_start_bin {
                    g *= transient_scale;
                }

                g = g.clamp(min_gain, 1.0);
                gain_buf[k] = g;
                prev_post_snr[k] = gamma;
            }

            // 5. Light 3-bin spectral gain smoothing prevents isolated musical-noise tones.
            for k in 0..bins {
                let g_prev = if k > 0 { gain_buf[k - 1] } else { gain_buf[k] };
                let g_curr = gain_buf[k];
                let g_next = if k + 1 < bins {
                    gain_buf[k + 1]
                } else {
                    gain_buf[k]
                };
                let smooth_g = (0.2 * g_prev + 0.6 * g_curr + 0.2 * g_next).clamp(min_gain, 1.0);
                prev_gain[k] = smooth_g;

                let mut c = spectrum[k].scale(smooth_g);

                // Optional subtle comfort noise injection on suppressed bins.
                if comfort_amp > 0.0 && smooth_g < min_gain * 1.5 && k > 1 && k + 1 < bins {
                    *rng_state ^= *rng_state << 13;
                    *rng_state ^= *rng_state >> 17;
                    *rng_state ^= *rng_state << 5;
                    let phase = (*rng_state as f32 / u32::MAX as f32) * std::f32::consts::TAU;
                    let cn = comfort_amp * (fft_size as f32).sqrt();
                    c.re += cn * phase.cos();
                    c.im += cn * phase.sin();
                }

                spectrum[k] = c;
                // Maintain exact Hermitian conjugate symmetry for real-valued IFFT output.
                if k > 0 && k < fft_size / 2 {
                    spectrum[fft_size - k] = c.conj();
                }
            }

            // 6. Telemetry update
            let clean_power = (total_sig_power - total_noise_power).max(1e-10);
            *last_snr_db = (10.0 * (clean_power / total_noise_power.max(1e-10)).log10())
                .clamp(-30.0, 60.0);
            *last_noise_rms = (total_noise_power / (fft_size as f32 * fft_size as f32)).sqrt();
        });
    }
}
