//! AudioContext coordinator and master graph mixer connected to host soundcards.

use std::sync::{Arc, Mutex};
use crate::device::{AudioContextState, AudioError, AudioHardwareStream, AudioMixer};
use crate::filter::BiquadFilter;
use crate::sample::AudioBuffer;
use crate::spatial::{AudioListener, Panner3D, Vec3};
use crate::synth::{Oscillator, OscillatorType};

/// The primary coordinator for audio graph synthesis, filtering, spatialization, and host soundcard output.
pub struct AudioContext {
    pub sample_rate: u32,
    pub listener: AudioListener,
    pub master_gain: f32,
    mixer: Arc<Mutex<AudioMixer>>,
    hardware: Mutex<Option<AudioHardwareStream>>,
}

impl Default for AudioContext {
    fn default() -> Self {
        Self::new(44100)
    }
}

impl AudioContext {
    /// Creates a new AudioContext.
    ///
    /// Automatically attempts to connect to the host's default audio output device (soundcard).
    /// If no audio device is available (such as in headless or CI environments), it gracefully
    /// operates in software-only mode without error.
    pub fn new(sample_rate: u32) -> Self {
        let mixer = Arc::new(Mutex::new(AudioMixer::new(sample_rate, 2)));
        let hardware = AudioHardwareStream::open(mixer.clone(), Some(sample_rate)).ok();

        let effective_rate = if let Some(ref hw) = hardware {
            hw.sample_rate
        } else {
            sample_rate
        };

        Self {
            sample_rate: effective_rate,
            listener: AudioListener::default(),
            master_gain: 1.0,
            mixer,
            hardware: Mutex::new(hardware),
        }
    }

    /// Strict constructor that requires a valid, accessible physical audio output device.
    pub fn with_device(preferred_sample_rate: Option<u32>) -> Result<Self, AudioError> {
        let sample_rate = preferred_sample_rate.unwrap_or(44100);
        let mixer = Arc::new(Mutex::new(AudioMixer::new(sample_rate, 2)));
        let hw = AudioHardwareStream::open(mixer.clone(), preferred_sample_rate)?;
        let effective_rate = hw.sample_rate;

        Ok(Self {
            sample_rate: effective_rate,
            listener: AudioListener::default(),
            master_gain: 1.0,
            mixer,
            hardware: Mutex::new(Some(hw)),
        })
    }

    /// Attempts to connect to the hardware audio output device if not already active.
    pub fn init_hardware(&self) -> Result<(), AudioError> {
        let mut hw_guard = self.hardware.lock().unwrap();
        if hw_guard.is_none() {
            let hw = AudioHardwareStream::open(self.mixer.clone(), Some(self.sample_rate))?;
            *hw_guard = Some(hw);
        }
        Ok(())
    }

    /// Returns `true` if connected to a real hardware audio output stream.
    pub fn has_hardware_output(&self) -> bool {
        self.hardware.lock().unwrap().is_some()
    }

    /// Returns the name of the active audio hardware output device, if present.
    pub fn device_name(&self) -> Option<String> {
        self.hardware.lock().unwrap().as_ref().map(|hw| hw.device_name.clone())
    }

    /// Returns the current lifecycle state of the audio context.
    pub fn state(&self) -> AudioContextState {
        self.mixer.lock().unwrap().state
    }

    /// Resumes audio rendering and playback on the hardware soundcard.
    pub fn resume(&self) -> Result<(), AudioError> {
        let mut mixer = self.mixer.lock().unwrap();
        if mixer.state == AudioContextState::Closed {
            return Err(AudioError::ContextClosed);
        }
        mixer.state = AudioContextState::Running;

        let hw = self.hardware.lock().unwrap();
        if let Some(ref stream) = *hw {
            stream.play()?;
        }
        Ok(())
    }

    /// Suspends audio rendering and pauses playback on the hardware soundcard.
    pub fn suspend(&self) -> Result<(), AudioError> {
        let mut mixer = self.mixer.lock().unwrap();
        if mixer.state == AudioContextState::Closed {
            return Err(AudioError::ContextClosed);
        }
        mixer.state = AudioContextState::Suspended;

        let hw = self.hardware.lock().unwrap();
        if let Some(ref stream) = *hw {
            stream.pause()?;
        }
        Ok(())
    }

    /// Closes the audio context, terminating hardware stream handles and freeing resources.
    pub fn close(&self) -> Result<(), AudioError> {
        let mut mixer = self.mixer.lock().unwrap();
        mixer.state = AudioContextState::Closed;
        mixer.clear();

        let mut hw = self.hardware.lock().unwrap();
        *hw = None;
        Ok(())
    }

    /// Sets the master volume gain for all audio routed through this context.
    pub fn set_master_gain(&mut self, gain: f32) {
        self.master_gain = gain;
        if let Ok(mut mixer) = self.mixer.lock() {
            mixer.master_gain = gain;
        }
    }

    /// Enqueues an audio buffer for immediate hardware playback through the soundcard.
    pub fn play_buffer(&self, buffer: AudioBuffer) {
        self.play_buffer_with_gain(buffer, 1.0);
    }

    /// Enqueues an audio buffer with custom gain for hardware playback.
    pub fn play_buffer_with_gain(&self, buffer: AudioBuffer, gain: f32) {
        if let Ok(mut mixer) = self.mixer.lock() {
            mixer.enqueue(buffer, gain, false);
        }
    }

    /// Enqueues an audio buffer to loop continuously on the soundcard.
    pub fn play_buffer_looping(&self, buffer: AudioBuffer, gain: f32) {
        if let Ok(mut mixer) = self.mixer.lock() {
            mixer.enqueue(buffer, gain, true);
        }
    }

    /// Synthesizes and plays a pure sine test tone through the soundcard for `duration_secs`.
    pub fn play_tone(&self, frequency: f32, duration_secs: f32) {
        let sample_count = (self.sample_rate as f32 * duration_secs) as usize;
        let mut osc = Oscillator::new(OscillatorType::Sine, frequency);
        let samples = osc.render_samples(sample_count, self.sample_rate);

        let buffer = AudioBuffer {
            sample_rate: self.sample_rate,
            channels: vec![samples.clone(), samples],
        };
        self.play_buffer(buffer);
    }

    /// Synthesizes and plays a 3D spatialized tone positioned relative to the listener.
    pub fn play_spatial_tone(&self, frequency: f32, position: Vec3, duration_secs: f32) {
        let sample_count = (self.sample_rate as f32 * duration_secs) as usize;
        let mut osc = Oscillator::new(OscillatorType::Sine, frequency);
        let panner = Panner3D::new(position);

        let buffer = self.render_spatial_source(&mut osc, None, &panner, sample_count);
        self.play_buffer(buffer);
    }

    /// Returns a reference-counted handle to the underlying real-time audio mixer.
    pub fn mixer(&self) -> Arc<Mutex<AudioMixer>> {
        self.mixer.clone()
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
