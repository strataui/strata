//! Digital biquad IIR filters for frequency domain equalization and shaping.

use std::f32::consts::PI;
use crate::sample::AudioBuffer;

/// Type of biquad filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BiquadFilterType {
    Lowpass,
    Highpass,
    Bandpass,
    Notch,
}

/// Direct Form I digital biquad filter.
#[derive(Debug, Clone)]
pub struct BiquadFilter {
    pub filter_type: BiquadFilterType,
    pub cutoff: f32,
    pub q: f32,
    pub sample_rate: u32,
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl BiquadFilter {
    pub fn new(filter_type: BiquadFilterType, cutoff: f32, q: f32, sample_rate: u32) -> Self {
        let mut filter = Self {
            filter_type,
            cutoff,
            q,
            sample_rate,
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        };
        filter.recalculate_coefficients();
        filter
    }

    /// Recalculates normalized filter coefficients using Robert Bristow-Johnson Audio EQ formulas.
    pub fn recalculate_coefficients(&mut self) {
        let omega = 2.0 * PI * (self.cutoff / self.sample_rate as f32).clamp(0.0001, 0.499);
        let sin_omega = omega.sin();
        let cos_omega = omega.cos();
        let alpha = sin_omega / (2.0 * self.q.max(0.001));

        let (b0_raw, b1_raw, b2_raw, a0_raw, a1_raw, a2_raw) = match self.filter_type {
            BiquadFilterType::Lowpass => {
                let b = (1.0 - cos_omega) / 2.0;
                (b, 1.0 - cos_omega, b, 1.0 + alpha, -2.0 * cos_omega, 1.0 - alpha)
            }
            BiquadFilterType::Highpass => {
                let b = (1.0 + cos_omega) / 2.0;
                (b, -(1.0 + cos_omega), b, 1.0 + alpha, -2.0 * cos_omega, 1.0 - alpha)
            }
            BiquadFilterType::Bandpass => {
                (alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cos_omega, 1.0 - alpha)
            }
            BiquadFilterType::Notch => {
                (1.0, -2.0 * cos_omega, 1.0, 1.0 + alpha, -2.0 * cos_omega, 1.0 - alpha)
            }
        };

        let inv_a0 = 1.0 / a0_raw;
        self.b0 = b0_raw * inv_a0;
        self.b1 = b1_raw * inv_a0;
        self.b2 = b2_raw * inv_a0;
        self.a1 = a1_raw * inv_a0;
        self.a2 = a2_raw * inv_a0;
    }

    /// Processes a single audio sample through the filter.
    pub fn process_sample(&mut self, input: f32) -> f32 {
        let output = self.b0 * input + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1 - self.a2 * self.y2;

        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = output;

        output
    }

    /// Processes an entire audio buffer in-place.
    pub fn process_buffer(&mut self, buffer: &mut AudioBuffer) {
        for ch in &mut buffer.channels {
            for sample in ch {
                *sample = self.process_sample(*sample);
            }
        }
    }

    pub fn reset_state(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}
