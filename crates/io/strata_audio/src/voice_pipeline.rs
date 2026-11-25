//! Unified zero-allocation real-time Voice Enhancement Pipeline chaining:
//! `HPF -> AEC -> Adaptive NoiseSuppressor -> Multi-Feature VAD -> NoiseGate -> ParametricEQ -> DeEsser -> AGC -> Limiter`.

use crate::aec::{AecConfig, EchoCanceller};
use crate::capture::CaptureProcessorFn;
use crate::dynamics::{AgcProcessor, CompressorLimiter, DeEsser, NoiseGate, ParametricEq};
use crate::filter::{BiquadFilter, BiquadFilterType};
use crate::noise_cancel::{NoiseSuppressionPreset, NoiseSuppressor};
use crate::vad::{VadConfig, VadDecision, VoiceActivityDetector};

/// High-level configuration for [`VoicePipeline`].
#[derive(Debug, Clone, PartialEq)]
pub struct VoicePipelineConfig {
    pub sample_rate: u32,
    pub enable_highpass: bool,
    pub highpass_hz: f32,
    pub enable_aec: bool,
    pub aec_tail_ms: f32,
    pub noise_suppression: NoiseSuppressionPreset,
    pub enable_vad: bool,
    pub enable_noise_gate: bool,
    pub gate_open_db: f32,
    pub gate_close_db: f32,
    pub enable_eq: bool,
    pub enable_de_esser: bool,
    pub enable_agc: bool,
    pub agc_target_dbfs: f32,
    pub enable_limiter: bool,
}

impl VoicePipelineConfig {
    /// Clean full-duplex conference call / VoIP / AI voice agent preset (`< 10 ms` latency).
    pub fn telephony(sample_rate: u32) -> Self {
        Self {
            sample_rate: sample_rate.max(8_000),
            enable_highpass: true,
            highpass_hz: 80.0,
            enable_aec: true,
            aec_tail_ms: 64.0,
            noise_suppression: NoiseSuppressionPreset::Medium,
            enable_vad: true,
            enable_noise_gate: false,
            gate_open_db: -42.0,
            gate_close_db: -50.0,
            enable_eq: true,
            enable_de_esser: true,
            enable_agc: true,
            agc_target_dbfs: -18.0,
            enable_limiter: true,
        }
    }

    /// Studio / podcast voice mastering preset with vocal presence EQ, De-Esser, and Broadcast NS.
    pub fn podcast(sample_rate: u32) -> Self {
        Self {
            sample_rate: sample_rate.max(8_000),
            enable_highpass: true,
            highpass_hz: 85.0,
            enable_aec: false,
            aec_tail_ms: 64.0,
            noise_suppression: NoiseSuppressionPreset::Broadcast,
            enable_vad: true,
            enable_noise_gate: true,
            gate_open_db: -40.0,
            gate_close_db: -48.0,
            enable_eq: true,
            enable_de_esser: true,
            enable_agc: true,
            agc_target_dbfs: -16.0,
            enable_limiter: true,
        }
    }

    /// High-noise robotics / UAV ground-station intercom preset.
    pub fn noisy_cockpit(sample_rate: u32) -> Self {
        Self {
            sample_rate: sample_rate.max(8_000),
            enable_highpass: true,
            highpass_hz: 120.0,
            enable_aec: true,
            aec_tail_ms: 96.0,
            noise_suppression: NoiseSuppressionPreset::High,
            enable_vad: true,
            enable_noise_gate: true,
            gate_open_db: -38.0,
            gate_close_db: -46.0,
            enable_eq: true,
            enable_de_esser: false,
            enable_agc: true,
            agc_target_dbfs: -16.0,
            enable_limiter: true,
        }
    }

    /// Zero-modification bit-exact pass-through preset.
    pub fn raw_passthrough(sample_rate: u32) -> Self {
        Self {
            sample_rate: sample_rate.max(8_000),
            enable_highpass: false,
            highpass_hz: 80.0,
            enable_aec: false,
            aec_tail_ms: 64.0,
            noise_suppression: NoiseSuppressionPreset::Off,
            enable_vad: false,
            enable_noise_gate: false,
            gate_open_db: -60.0,
            gate_close_db: -65.0,
            enable_eq: false,
            enable_de_esser: false,
            enable_agc: false,
            agc_target_dbfs: -18.0,
            enable_limiter: false,
        }
    }
}

/// Real-time telemetry returned by each [`VoicePipeline::process_frame`] call.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct VoicePipelineStats {
    pub vad: VadDecision,
    pub estimated_snr_db: f32,
    pub erle_db: f32,
    pub double_talk: bool,
    pub agc_gain_db: f32,
    pub limiter_reduction_db: f32,
    pub algorithmic_latency_ms: f32,
}

/// Real-time zero-allocation Voice Enhancement Pipeline.
pub struct VoicePipeline {
    config: VoicePipelineConfig,
    hpf: BiquadFilter,
    aec: EchoCanceller,
    ns: NoiseSuppressor,
    vad: VoiceActivityDetector,
    gate: NoiseGate,
    eq: ParametricEq,
    de_esser: DeEsser,
    agc: AgcProcessor,
    limiter: CompressorLimiter,
    /// Pre-allocated intermediate scratch buffers (zero heap allocations during `process_frame`).
    scratch_a: Vec<f32>,
    scratch_b: Vec<f32>,
}

impl VoicePipeline {
    /// Constructs a new pre-allocated [`VoicePipeline`] ready for real-time audio callbacks.
    pub fn new(config: VoicePipelineConfig) -> Self {
        let sr = config.sample_rate.max(8_000);
        let mut aec_cfg = AecConfig::new(sr);
        aec_cfg.tail_length_ms = config.aec_tail_ms;

        // Pre-allocate scratch capacity for frames up to 4096 samples (85 ms @ 48 kHz) so
        // typical 128–1024 sample real-time audio callbacks perform 0 heap allocations.
        let scratch_cap = 4096usize;

        Self {
            hpf: BiquadFilter::new(BiquadFilterType::Highpass, config.highpass_hz, 0.707, sr),
            aec: EchoCanceller::new(aec_cfg),
            ns: NoiseSuppressor::with_preset(config.noise_suppression, sr),
            vad: VoiceActivityDetector::new(VadConfig::new(sr)),
            gate: NoiseGate::new(
                sr,
                config.gate_open_db,
                config.gate_close_db,
                120.0,
                5.0,
                90.0,
                -30.0,
            ),
            eq: ParametricEq::voice_presence_preset(sr),
            de_esser: DeEsser::new(sr, 6_500.0, -24.0, 9.0),
            agc: AgcProcessor::with_params(sr, config.agc_target_dbfs, 18.0, -12.0, -45.0, 25.0, 350.0),
            limiter: CompressorLimiter::voice_default(sr),
            scratch_a: vec![0.0; scratch_cap],
            scratch_b: vec![0.0; scratch_cap],
            config,
        }
    }

    /// Updates the Noise Suppression preset dynamically without reallocating.
    pub fn set_noise_suppression(&mut self, preset: NoiseSuppressionPreset) {
        self.config.noise_suppression = preset;
        self.ns.set_preset(preset);
    }

    /// Toggles Acoustic Echo Cancellation at runtime without reallocating.
    pub fn set_aec_enabled(&mut self, enabled: bool) {
        self.config.enable_aec = enabled;
    }

    /// Toggles Automatic Gain Control at runtime.
    pub fn set_agc_enabled(&mut self, enabled: bool) {
        self.config.enable_agc = enabled;
    }

    /// Returns total algorithmic latency of the pipeline in milliseconds.
    pub fn algorithmic_latency_ms(&self) -> f32 {
        if self.ns.is_enabled() {
            self.ns.algorithmic_latency_ms()
        } else {
            0.0
        }
    }

    /// Resets all DSP stages cleanly without reallocating.
    pub fn reset(&mut self) {
        self.hpf.reset_state();
        self.aec.reset();
        self.ns.reset();
        self.vad.reset();
        self.eq.reset();
        self.agc.reset();
        self.limiter.reset();
    }

    /// Processes a mono microphone frame (`mic.len() == output.len()`) with optional synchronized
    /// speaker `far_end` reference for AEC.
    ///
    /// Guarantees **zero heap allocations** for all frames `<= 4096` samples.
    pub fn process_frame(
        &mut self,
        mic: &[f32],
        far_end: Option<&[f32]>,
        output: &mut [f32],
    ) -> VoicePipelineStats {
        let len = mic.len();
        debug_assert_eq!(len, output.len());
        if len == 0 {
            return VoicePipelineStats::default();
        }
        if len > self.scratch_a.len() {
            self.scratch_a.resize(len, 0.0);
            self.scratch_b.resize(len, 0.0);
        }

        let buf_a = &mut self.scratch_a[..len];
        let buf_b = &mut self.scratch_b[..len];

        // 1. High-pass DC & rumble removal
        buf_a.copy_from_slice(mic);
        if self.config.enable_highpass {
            self.hpf.process_slice_in_place(buf_a);
        }

        // 2. Acoustic Echo Cancellation (when far-end reference is available)
        if self.config.enable_aec {
            if let Some(far) = far_end {
                if far.len() == len {
                    self.aec.process_frame(far, buf_a, buf_b);
                    buf_a.copy_from_slice(buf_b);
                }
            }
        }

        // 3. Adaptive Noise Cancellation (STFT + MCRA + Decision-Directed MMSE/Wiener)
        if self.ns.is_enabled() {
            self.ns.process_frame(buf_a, output);
        } else {
            output.copy_from_slice(buf_a);
        }

        // 4. Multi-Feature Voice Activity Detection on denoised signal
        let vad_decision = if self.config.enable_vad {
            self.vad.analyze_frame(output)
        } else {
            VadDecision::default()
        };

        // 5. Optional Hysteresis Noise Gate
        if self.config.enable_noise_gate {
            self.gate.process_in_place(output);
        }

        // 6. Optional Vocal Presence Parametric EQ
        if self.config.enable_eq {
            self.eq.process_in_place(output);
        }

        // 7. Optional Sibilance De-Esser
        if self.config.enable_de_esser {
            self.de_esser.process_in_place(output);
        }

        // 8. Optional VAD-gated Automatic Gain Control
        if self.config.enable_agc {
            let speech_gate = if self.config.enable_vad {
                Some(vad_decision.is_speech)
            } else {
                None
            };
            self.agc.process_in_place(output, speech_gate);
        }

        // 9. Optional Soft-Knee Compressor & Brickwall True-Peak Limiter
        if self.config.enable_limiter {
            self.limiter.process_in_place(output);
        }

        VoicePipelineStats {
            vad: vad_decision,
            estimated_snr_db: self.ns.estimated_snr_db(),
            erle_db: self.aec.erle_db(),
            double_talk: self.aec.is_double_talk(),
            agc_gain_db: self.agc.current_gain_db(),
            limiter_reduction_db: self.limiter.gain_reduction_db(),
            algorithmic_latency_ms: self.algorithmic_latency_ms(),
        }
    }

    /// Converts this [`VoicePipeline`] into an inline capture callback suitable for
    /// [`crate::capture::AudioInputStream::open_with_processor`] or
    /// [`crate::capture::MockAudioInput::set_processor`].
    pub fn into_capture_processor(mut self) -> CaptureProcessorFn {
        Box::new(move |input: &[f32], output: &mut [f32]| {
            let _ = self.process_frame(input, None, output);
        })
    }
}
