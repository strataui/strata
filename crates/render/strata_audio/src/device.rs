//! Hardware soundcard integration and real-time audio output stream via `cpal`.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crate::sample::AudioBuffer;

/// Operational state of the audio context and playback stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioContextState {
    Suspended,
    Running,
    Closed,
}

/// Errors arising from audio hardware device discovery, stream configuration, or playback.
#[derive(Debug, Clone, PartialEq)]
pub enum AudioError {
    DeviceNotFound(String),
    DeviceNotAvailable(String),
    UnsupportedFormat(String),
    StreamBuildError(String),
    StreamPlayError(String),
    ContextClosed,
}

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DeviceNotFound(msg) => write!(f, "Audio device not found: {msg}"),
            Self::DeviceNotAvailable(msg) => write!(f, "Audio device not available: {msg}"),
            Self::UnsupportedFormat(msg) => write!(f, "Unsupported audio format: {msg}"),
            Self::StreamBuildError(msg) => write!(f, "Failed to build audio stream: {msg}"),
            Self::StreamPlayError(msg) => write!(f, "Failed to play audio stream: {msg}"),
            Self::ContextClosed => write!(f, "Audio context is closed"),
        }
    }
}

impl std::error::Error for AudioError {}

/// Represents an active buffer currently playing through the mixer.
#[derive(Debug, Clone)]
pub struct ActiveBuffer {
    pub buffer: AudioBuffer,
    pub position: usize,
    pub gain: f32,
    pub looping: bool,
}

/// Real-time audio mixer feeding the hardware output stream callback.
pub struct AudioMixer {
    pub sample_rate: u32,
    pub channels: usize,
    pub master_gain: f32,
    pub state: AudioContextState,
    pub queue: VecDeque<ActiveBuffer>,
}

impl AudioMixer {
    pub fn new(sample_rate: u32, channels: usize) -> Self {
        Self {
            sample_rate,
            channels: channels.max(1),
            master_gain: 1.0,
            state: AudioContextState::Running,
            queue: VecDeque::new(),
        }
    }

    /// Enqueues an audio buffer for real-time hardware playback.
    pub fn enqueue(&mut self, buffer: AudioBuffer, gain: f32, looping: bool) {
        if !buffer.is_empty() {
            self.queue.push_back(ActiveBuffer {
                buffer,
                position: 0,
                gain,
                looping,
            });
        }
    }

    /// Clears all queued buffers.
    pub fn clear(&mut self) {
        self.queue.clear();
    }

    /// Returns the number of active buffers currently queued.
    pub fn active_buffer_count(&self) -> usize {
        self.queue.len()
    }

    /// Renders interleaved 32-bit floating point audio frames into `output`.
    pub fn render_interleaved_f32(&mut self, output: &mut [f32]) {
        output.fill(0.0);

        if self.state != AudioContextState::Running || self.queue.is_empty() {
            return;
        }

        let ch_count = self.channels;
        let frame_count = output.len() / ch_count;

        for frame_idx in 0..frame_count {
            let base_idx = frame_idx * ch_count;
            let mut finished_indices = Vec::new();

            for (buf_idx, active) in self.queue.iter_mut().enumerate() {
                let buf_len = active.buffer.length();
                if active.position < buf_len {
                    let src_channels = active.buffer.channel_count();
                    for ch in 0..ch_count {
                        // Map hardware channel to buffer channel (mono expands to stereo/surround)
                        let src_ch = if ch < src_channels { ch } else { 0 };
                        let sample = active.buffer.channels[src_ch][active.position];
                        output[base_idx + ch] += sample * active.gain * self.master_gain;
                    }
                    active.position += 1;
                    if active.position >= buf_len {
                        if active.looping {
                            active.position = 0;
                        } else {
                            finished_indices.push(buf_idx);
                        }
                    }
                }
            }

            // Remove finished non-looping buffers in reverse order
            for &idx in finished_indices.iter().rev() {
                self.queue.remove(idx);
            }
        }

        // Soft-clip output to prevent harsh digital clipping
        for s in output.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }
    }
}

/// Manages the cpal host output stream connection to the physical soundcard.
pub struct AudioHardwareStream {
    stream: cpal::Stream,
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
}

impl AudioHardwareStream {
    /// Opens the default system audio output device and starts the playback stream.
    pub fn open(
        mixer: Arc<Mutex<AudioMixer>>,
        preferred_sample_rate: Option<u32>,
    ) -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| AudioError::DeviceNotFound("No default output device on host".into()))?;

        let device_name = device
            .name()
            .unwrap_or_else(|_| "Default Audio Device".into());

        let default_config = device
            .default_output_config()
            .map_err(|e| AudioError::DeviceNotAvailable(e.to_string()))?;

        let sample_format = default_config.sample_format();
        let channels = default_config.channels();
        let sample_rate = preferred_sample_rate.unwrap_or(default_config.sample_rate().0);

        let config = cpal::StreamConfig {
            channels,
            sample_rate: cpal::SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        // Update mixer configuration to match hardware device channels and sample rate
        if let Ok(mut lock) = mixer.lock() {
            lock.sample_rate = sample_rate;
            lock.channels = channels as usize;
        }

        let err_fn = |err| eprintln!("strata_audio hardware stream error: {err}");

        let stream = match sample_format {
            cpal::SampleFormat::F32 => {
                let mixer_clone = mixer;
                device.build_output_stream(
                    &config,
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        if let Ok(mut lock) = mixer_clone.lock() {
                            lock.render_interleaved_f32(data);
                        }
                    },
                    err_fn,
                    None,
                )
            }
            cpal::SampleFormat::I16 => {
                let mixer_clone = mixer;
                let mut scratch = Vec::new();
                device.build_output_stream(
                    &config,
                    move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                        if scratch.len() != data.len() {
                            scratch.resize(data.len(), 0.0);
                        }
                        if let Ok(mut lock) = mixer_clone.lock() {
                            lock.render_interleaved_f32(&mut scratch);
                        }
                        for (out, &src) in data.iter_mut().zip(scratch.iter()) {
                            *out = (src * 32767.0).clamp(-32768.0, 32767.0) as i16;
                        }
                    },
                    err_fn,
                    None,
                )
            }
            cpal::SampleFormat::U16 => {
                let mixer_clone = mixer;
                let mut scratch = Vec::new();
                device.build_output_stream(
                    &config,
                    move |data: &mut [u16], _: &cpal::OutputCallbackInfo| {
                        if scratch.len() != data.len() {
                            scratch.resize(data.len(), 0.0);
                        }
                        if let Ok(mut lock) = mixer_clone.lock() {
                            lock.render_interleaved_f32(&mut scratch);
                        }
                        for (out, &src) in data.iter_mut().zip(scratch.iter()) {
                            let norm = (src * 0.5 + 0.5).clamp(0.0, 1.0);
                            *out = (norm * 65535.0) as u16;
                        }
                    },
                    err_fn,
                    None,
                )
            }
            other => return Err(AudioError::UnsupportedFormat(format!("{other:?}"))),
        }
        .map_err(|e| AudioError::StreamBuildError(e.to_string()))?;

        stream
            .play()
            .map_err(|e| AudioError::StreamPlayError(e.to_string()))?;

        Ok(Self {
            stream,
            device_name,
            sample_rate,
            channels,
        })
    }

    /// Resumes playback of the hardware output stream.
    pub fn play(&self) -> Result<(), AudioError> {
        self.stream
            .play()
            .map_err(|e| AudioError::StreamPlayError(e.to_string()))
    }

    /// Pauses playback of the hardware output stream.
    pub fn pause(&self) -> Result<(), AudioError> {
        self.stream
            .pause()
            .map_err(|e| AudioError::StreamPlayError(e.to_string()))
    }
}
