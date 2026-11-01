//! Audio buffer and PCM sample representation for synthesis and processing.

/// Multi-channel audio buffer storing 32-bit floating point PCM audio samples in range [-1.0, 1.0].
#[derive(Debug, Clone, PartialEq)]
pub struct AudioBuffer {
    pub sample_rate: u32,
    pub channels: Vec<Vec<f32>>,
}

impl AudioBuffer {
    pub fn new(channels: usize, length: usize, sample_rate: u32) -> Self {
        let mut chs = Vec::with_capacity(channels);
        for _ in 0..channels {
            chs.push(vec![0.0; length]);
        }
        Self {
            sample_rate,
            channels: chs,
        }
    }

    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    pub fn length(&self) -> usize {
        self.channels.first().map(|c| c.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.length() == 0
    }

    pub fn duration_seconds(&self) -> f32 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.length() as f32 / self.sample_rate as f32
        }
    }

    /// Computes the peak amplitude across all channels.
    pub fn peak_amplitude(&self) -> f32 {
        let mut peak = 0.0f32;
        for ch in &self.channels {
            for &s in ch {
                peak = peak.max(s.abs());
            }
        }
        peak
    }

    /// Computes the Root-Mean-Square (RMS) energy across all channels.
    pub fn rms_level(&self) -> f32 {
        let total_samples: usize = self.channels.iter().map(|c| c.len()).sum();
        if total_samples == 0 {
            return 0.0;
        }
        let sum_sq: f32 = self.channels.iter().flat_map(|c| c.iter()).map(|&s| s * s).sum();
        (sum_sq / total_samples as f32).sqrt()
    }

    /// Applies uniform gain scaling in-place.
    pub fn apply_gain(&mut self, gain: f32) {
        for ch in &mut self.channels {
            for s in ch {
                *s *= gain;
            }
        }
    }

    /// Mixes another audio buffer into this buffer.
    pub fn mix_from(&mut self, other: &AudioBuffer, gain: f32) {
        let min_chs = self.channels.len().min(other.channels.len());
        for i in 0..min_chs {
            let len = self.channels[i].len().min(other.channels[i].len());
            for j in 0..len {
                self.channels[i][j] += other.channels[i][j] * gain;
            }
        }
    }

    /// Interleaves stereo channels into a single slice [L0, R0, L1, R1, ...].
    pub fn interleave_stereo(&self) -> Vec<f32> {
        let len = self.length();
        let mut out = Vec::with_capacity(len * 2);
        let left = self.channels.first();
        let right = if self.channels.len() > 1 {
            Some(&self.channels[1])
        } else {
            left
        };

        if let (Some(l), Some(r)) = (left, right) {
            for i in 0..len {
                out.push(l[i]);
                out.push(r[i]);
            }
        }
        out
    }
}
