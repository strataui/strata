//! High-level streaming media [`AudioPlayer`] with play/pause/stop/seek, playback-rate
//! pitch/speed scaling, mute/volume, looping, and zero-alloc pull rendering.

use crate::codecs::{decode_audio_bytes, decode_audio_file};
use crate::device::AudioError;
use crate::sample::AudioBuffer;
use std::path::Path;
use std::time::Duration;

/// Playback lifecycle state of an [`AudioPlayer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Idle,
    Playing,
    Paused,
    Ended,
}

/// High-level streaming audio player suitable for `<audio>` DOM elements and media apps.
#[derive(Debug, Clone)]
pub struct AudioPlayer {
    buffer: Option<AudioBuffer>,
    /// Fractional sample cursor (enables arbitrary smooth playback_rate scaling).
    cursor_frames: f64,
    state: PlaybackState,
    volume: f32,
    muted: bool,
    looping: bool,
    playback_rate: f32,
}

impl Default for AudioPlayer {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioPlayer {
    pub fn new() -> Self {
        Self {
            buffer: None,
            cursor_frames: 0.0,
            state: PlaybackState::Idle,
            volume: 1.0,
            muted: false,
            looping: false,
            playback_rate: 1.0,
        }
    }

    /// Loads an already-decoded [`AudioBuffer`] and resets cursor to `0.0s`.
    pub fn load_buffer(&mut self, buffer: AudioBuffer) {
        self.buffer = Some(buffer);
        self.cursor_frames = 0.0;
        self.state = PlaybackState::Paused;
    }

    /// Decodes and loads encoded audio bytes (WAV, or MP3/FLAC/OGG/AAC/MP4 with `codecs`).
    pub fn load_bytes(&mut self, bytes: &[u8], hint_ext: Option<&str>) -> Result<(), AudioError> {
        let buf = decode_audio_bytes(bytes, hint_ext)?;
        self.load_buffer(buf);
        Ok(())
    }

    /// Decodes and loads an audio file from disk.
    pub fn load_file(&mut self, path: &Path) -> Result<(), AudioError> {
        let buf = decode_audio_file(path)?;
        self.load_buffer(buf);
        Ok(())
    }

    pub fn play(&mut self) {
        if self.buffer.is_some() {
            if self.state == PlaybackState::Ended {
                self.cursor_frames = 0.0;
            }
            self.state = PlaybackState::Playing;
        }
    }

    pub fn pause(&mut self) {
        if self.state == PlaybackState::Playing {
            self.state = PlaybackState::Paused;
        }
    }

    pub fn stop(&mut self) {
        self.cursor_frames = 0.0;
        self.state = if self.buffer.is_some() {
            PlaybackState::Paused
        } else {
            PlaybackState::Idle
        };
    }

    pub fn seek(&mut self, position: Duration) {
        if let Some(ref buf) = self.buffer {
            let max_frame = buf.length().saturating_sub(1) as f64;
            let target = (position.as_secs_f64() * buf.sample_rate as f64).clamp(0.0, max_frame);
            self.cursor_frames = target;
            if self.state == PlaybackState::Ended {
                self.state = PlaybackState::Paused;
            }
        }
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 2.0);
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
    }

    pub fn is_muted(&self) -> bool {
        self.muted
    }

    pub fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
    }

    pub fn is_looping(&self) -> bool {
        self.looping
    }

    /// Sets playback speed multiplier in `[0.25, 4.0]` (default: `1.0`).
    pub fn set_playback_rate(&mut self, rate: f32) {
        self.playback_rate = rate.clamp(0.25, 4.0);
    }

    pub fn playback_rate(&self) -> f32 {
        self.playback_rate
    }

    pub fn state(&self) -> PlaybackState {
        self.state
    }

    pub fn duration(&self) -> Duration {
        match &self.buffer {
            Some(buf) if buf.sample_rate > 0 => {
                Duration::from_secs_f64(buf.length() as f64 / buf.sample_rate as f64)
            }
            _ => Duration::ZERO,
        }
    }

    pub fn current_time(&self) -> Duration {
        match &self.buffer {
            Some(buf) if buf.sample_rate > 0 => {
                Duration::from_secs_f64(self.cursor_frames / buf.sample_rate as f64)
            }
            _ => Duration::ZERO,
        }
    }

    /// Pulls interleaved `f32` samples at `output_sample_rate` and `output_channels`
    /// with zero heap allocations and linear interpolation across playback-rate / rate ratios.
    pub fn render_interleaved_f32(
        &mut self,
        output: &mut [f32],
        output_sample_rate: u32,
        output_channels: usize,
    ) {
        output.fill(0.0);
        if self.state != PlaybackState::Playing || output_channels == 0 || output_sample_rate == 0 {
            return;
        }
        let Some(ref buf) = self.buffer else {
            return;
        };
        let src_len = buf.length();
        let src_ch = buf.channel_count();
        if src_len == 0 || src_ch == 0 {
            return;
        }

        let gain = if self.muted { 0.0 } else { self.volume };
        let step =
            (buf.sample_rate as f64 / output_sample_rate as f64) * self.playback_rate as f64;
        let frames = output.len() / output_channels;

        for f in 0..frames {
            if self.cursor_frames >= src_len as f64 {
                if self.looping {
                    self.cursor_frames %= src_len as f64;
                } else {
                    self.cursor_frames = src_len as f64;
                    self.state = PlaybackState::Ended;
                    break;
                }
            }

            let i0 = self.cursor_frames.floor() as usize;
            let frac = (self.cursor_frames - i0 as f64) as f32;
            let i1 = if i0 + 1 < src_len {
                i0 + 1
            } else if self.looping {
                0
            } else {
                i0
            };

            let base = f * output_channels;
            for ch in 0..output_channels {
                let s_ch = if ch < src_ch { ch } else { 0 };
                let s0 = buf.channels[s_ch][i0];
                let s1 = buf.channels[s_ch][i1];
                let interp = s0 + (s1 - s0) * frac;
                output[base + ch] = (interp * gain).clamp(-1.0, 1.0);
            }

            self.cursor_frames += step;
        }
    }
}
