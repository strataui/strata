//! AudioContext coordinator and master graph mixer.

use crate::filter::BiquadFilter;
use crate::sample::AudioBuffer;
use crate::spatial::{AudioListener, Panner3D};
use crate::synth::Oscillator;

/// The primary coordinator for audio graph synthesis, filtering, spatialization, and mixing.
pub struct AudioContext {
    pub sample_rate: u32,
    pub listener: AudioListener,
    pub master_gain: f32,
}

impl Default for AudioContext {
    fn default() -> Self {
        Self::new(44100)
    }
}

impl AudioContext {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            listener: AudioListener::default(),
            master_gain: 1.0,
        }
    }

    /// Renders a spatialized sound source into a stereo AudioBuffer.
    pub fn render_spatial_source(
        &self,
        oscillator: &mut Oscillator,
        filter: Option<&mut BiquadFilter>,
        panner: &Panner3D,
        sample_count: usize,
    ) -> AudioBuffer {
        // 1. Calculate Doppler-shifted frequency
        let doppler = panner.compute_doppler_factor(&self.listener);
        let orig_freq = oscillator.frequency;
        oscillator.frequency = orig_freq * doppler;

        // 2. Synthesize mono samples
        let mut mono_samples = oscillator.render_samples(sample_count, self.sample_rate);
        oscillator.frequency = orig_freq;

        // 3. Apply optional biquad filter
        if let Some(f) = filter {
            for sample in &mut mono_samples {
                *sample = f.process_sample(*sample);
            }
        }

        // 4. Calculate spatial distance attenuation & stereo panning
        let distance_gain = panner.compute_gain(&self.listener);
        let (left_pan, right_pan) = panner.compute_stereo_panning(&self.listener);

        let final_left_gain = distance_gain * left_pan * self.master_gain;
        let final_right_gain = distance_gain * right_pan * self.master_gain;

        let mut left_ch = Vec::with_capacity(sample_count);
        let mut right_ch = Vec::with_capacity(sample_count);

        for &s in &mono_samples {
            left_ch.push(s * final_left_gain);
            right_ch.push(s * final_right_gain);
        }

        AudioBuffer {
            sample_rate: self.sample_rate,
            channels: vec![left_ch, right_ch],
        }
    }
}
