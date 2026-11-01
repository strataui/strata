//! Parametric audio oscillators and wave synthesizers.

use std::f32::consts::PI;

/// Waveform type of an audio oscillator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OscillatorType {
    Sine,
    Square,
    Sawtooth,
    Triangle,
    WhiteNoise,
}

/// Parametric audio oscillator node producing periodic or stochastic waveforms.
#[derive(Debug, Clone)]
pub struct Oscillator {
    pub osc_type: OscillatorType,
    pub frequency: f32,
    pub detune_cents: f32,
    pub phase: f32,
    seed: u32,
}

impl Oscillator {
    pub fn new(osc_type: OscillatorType, frequency: f32) -> Self {
        Self {
            osc_type,
            frequency,
            detune_cents: 0.0,
            phase: 0.0,
            seed: 123456789,
        }
    }

    pub fn with_detune(mut self, cents: f32) -> Self {
        self.detune_cents = cents;
        self
    }

    /// Computes effective frequency factoring in detune in cents: f * 2^(cents / 1200).
    pub fn effective_frequency(&self) -> f32 {
        self.frequency * (2.0f32).powf(self.detune_cents / 1200.0)
    }

    /// Synthesizes `count` audio samples at the specified `sample_rate`.
    pub fn render_samples(&mut self, count: usize, sample_rate: u32) -> Vec<f32> {
        let mut buffer = Vec::with_capacity(count);
        let freq = self.effective_frequency();
        let phase_inc = (2.0 * PI * freq) / sample_rate as f32;

        for _ in 0..count {
            let sample = match self.osc_type {
                OscillatorType::Sine => self.phase.sin(),
                OscillatorType::Square => {
                    if self.phase.sin() >= 0.0 {
                        1.0
                    } else {
                        -1.0
                    }
                }
                OscillatorType::Sawtooth => {
                    // Normalize phase in [0, 2pi] to [-1.0, 1.0]
                    let norm = self.phase / (2.0 * PI);
                    2.0 * (norm - norm.floor()) - 1.0
                }
                OscillatorType::Triangle => {
                    // Triangle wave: 2/pi * asin(sin(phase))
                    (2.0 / PI) * self.phase.sin().asin()
                }
                OscillatorType::WhiteNoise => {
                    // Linear congruential pseudo-random generator
                    self.seed = self.seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    (self.seed as f32 / u32::MAX as f32) * 2.0 - 1.0
                }
            };

            buffer.push(sample);
            self.phase = (self.phase + phase_inc) % (2.0 * PI);
        }

        buffer
    }
}
