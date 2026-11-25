//! Hardware soundcard integration and real-time audio output stream via `cpal`.

#[cfg(feature = "hardware")]
use crate::ring_buffer::{Consumer, SpscRingBuffer};
use crate::ring_buffer::Producer;
use crate::sample::AudioBuffer;
#[cfg(feature = "hardware")]
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Default capacity of the lock-free SPSC audio command ring buffer.
pub const DEFAULT_COMMAND_QUEUE_CAPACITY: usize = 512;

/// Commands passed from the application thread to the real-time audio thread.
#[derive(Debug)]
pub enum AudioCommand {
    Enqueue(ActiveBuffer),
    SetMasterGain(f32),
    SetState(AudioContextState),
    Clear,
}

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
    ///
    /// If the buffer's sample rate differs from the hardware mixer's sample rate,
    /// it is automatically resampled using high-quality cubic interpolation.
    pub fn enqueue(&mut self, buffer: AudioBuffer, gain: f32, looping: bool) {
        if !buffer.is_empty() {
            let buffer = if buffer.sample_rate != 0 && buffer.sample_rate != self.sample_rate {
                buffer.resample(self.sample_rate)
            } else {
                buffer
            };
            self.queue.push_back(ActiveBuffer {
                buffer,
                position: 0,
                gain,
                looping,
            });
        }
    }

    /// Enqueues an active buffer directly, resampling to mixer sample rate if needed.
    pub fn enqueue_active(&mut self, mut active: ActiveBuffer) {
        if !active.buffer.is_empty() {
            if active.buffer.sample_rate != 0 && active.buffer.sample_rate != self.sample_rate {
                active.buffer = active.buffer.resample(self.sample_rate);
            }
            self.queue.push_back(active);
        }
    }

    /// Enqueues an audio buffer with explicit resampling quality control.
    pub fn enqueue_resampled(
        &mut self,
        buffer: AudioBuffer,
        gain: f32,
        looping: bool,
        quality: crate::resample::ResampleQuality,
    ) {
        if !buffer.is_empty() {
            let buffer = if buffer.sample_rate != 0 && buffer.sample_rate != self.sample_rate {
                buffer.resample_with_quality(self.sample_rate, quality)
            } else {
                buffer
            };
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
    ///
    /// Performs zero heap allocations on the real-time audio thread.
    pub fn render_interleaved_f32(&mut self, output: &mut [f32]) {
        output.fill(0.0);

        if self.state != AudioContextState::Running || self.queue.is_empty() {
            return;
        }

        let ch_count = self.channels;
        let frame_count = output.len() / ch_count;

        for active in self.queue.iter_mut() {
            let buf_len = active.buffer.length();
            if buf_len == 0 {
                continue;
            }
            let src_channels = active.buffer.channel_count();
            let scale = active.gain * self.master_gain;
            for frame_idx in 0..frame_count {
                if active.position >= buf_len {
                    if active.looping {
                        active.position = 0;
                    } else {
                        break;
                    }
                }
                let base_idx = frame_idx * ch_count;
                for ch in 0..ch_count {
                    // Map hardware channel to buffer channel (mono expands to stereo/surround)
                    let src_ch = if ch < src_channels { ch } else { 0 };
                    let sample = active.buffer.channels[src_ch][active.position];
                    output[base_idx + ch] += sample * scale;
                }
                active.position += 1;
                if active.position >= buf_len && active.looping {
                    active.position = 0;
                }
            }
        }

        // Remove finished non-looping buffers in-place without allocating.
        self.queue
            .retain(|active| active.looping || active.position < active.buffer.length());

        // Soft-clip output to prevent harsh digital clipping
        for s in output.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }
    }
}

/// Direction supported by a host audio endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioDeviceDirection {
    Input,
    Output,
    Duplex,
}

/// Hardware PCM sample format reported by an audio endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioSampleFormat {
    F32,
    I16,
    U16,
    Unknown,
}

/// Metadata describing a discovered physical or virtual audio endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDeviceInfo {
    /// Stable identifier (name-based on cpal hosts).
    pub id: String,
    /// Human-readable device display name.
    pub name: String,
    /// Supported I/O direction.
    pub direction: AudioDeviceDirection,
    /// `true` if this is the host default device for its direction.
    pub is_default: bool,
    /// Preferred default sample rate in Hz.
    pub default_sample_rate: u32,
    /// Minimum supported sample rate in Hz.
    pub min_sample_rate: u32,
    /// Maximum supported sample rate in Hz.
    pub max_sample_rate: u32,
    /// Default channel count.
    pub channels: u16,
    /// Preferred sample format.
    pub sample_format: AudioSampleFormat,
    /// `true` if this endpoint appears to be a system loopback / monitor sink.
    pub is_loopback: bool,
}

/// Hot-plug device topology change event emitted by [`AudioDeviceMonitor`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioDeviceEvent {
    Added(AudioDeviceInfo),
    Removed(String),
    DefaultInputChanged(Option<String>),
    DefaultOutputChanged(Option<String>),
}

/// Enumerates all host audio input devices.
pub fn list_input_devices() -> Vec<AudioDeviceInfo> {
    #[cfg(feature = "hardware")]
    {
        let host = cpal::default_host();
        let default_name = host
            .default_input_device()
            .and_then(|d| d.name().ok());
        let mut out = Vec::new();
        if let Ok(devices) = host.input_devices() {
            for dev in devices {
                let name = dev.name().unwrap_or_else(|_| "Unknown Input".into());
                let is_default = default_name.as_deref() == Some(name.as_str());
                let lower = name.to_lowercase();
                let is_loopback = lower.contains("monitor")
                    || lower.contains("loopback")
                    || lower.contains("stereo mix")
                    || lower.contains("blackhole");
                let (default_sr, min_sr, max_sr, ch, fmt) = match dev.default_input_config() {
                    Ok(cfg) => {
                        let sr = cfg.sample_rate().0;
                        let fmt = match cfg.sample_format() {
                            cpal::SampleFormat::F32 => AudioSampleFormat::F32,
                            cpal::SampleFormat::I16 => AudioSampleFormat::I16,
                            cpal::SampleFormat::U16 => AudioSampleFormat::U16,
                            _ => AudioSampleFormat::Unknown,
                        };
                        (sr, sr, sr, cfg.channels(), fmt)
                    }
                    Err(_) => (48_000, 8_000, 96_000, 1, AudioSampleFormat::F32),
                };
                out.push(AudioDeviceInfo {
                    id: format!("in:{name}"),
                    name,
                    direction: AudioDeviceDirection::Input,
                    is_default,
                    default_sample_rate: default_sr,
                    min_sample_rate: min_sr,
                    max_sample_rate: max_sr,
                    channels: ch,
                    sample_format: fmt,
                    is_loopback,
                });
            }
        }
        out
    }
    #[cfg(not(feature = "hardware"))]
    {
        Vec::new()
    }
}

/// Enumerates all host audio output devices.
pub fn list_output_devices() -> Vec<AudioDeviceInfo> {
    #[cfg(feature = "hardware")]
    {
        let host = cpal::default_host();
        let default_name = host
            .default_output_device()
            .and_then(|d| d.name().ok());
        let mut out = Vec::new();
        if let Ok(devices) = host.output_devices() {
            for dev in devices {
                let name = dev.name().unwrap_or_else(|_| "Unknown Output".into());
                let is_default = default_name.as_deref() == Some(name.as_str());
                let (default_sr, min_sr, max_sr, ch, fmt) = match dev.default_output_config() {
                    Ok(cfg) => {
                        let sr = cfg.sample_rate().0;
                        let fmt = match cfg.sample_format() {
                            cpal::SampleFormat::F32 => AudioSampleFormat::F32,
                            cpal::SampleFormat::I16 => AudioSampleFormat::I16,
                            cpal::SampleFormat::U16 => AudioSampleFormat::U16,
                            _ => AudioSampleFormat::Unknown,
                        };
                        (sr, sr, sr, cfg.channels(), fmt)
                    }
                    Err(_) => (48_000, 8_000, 96_000, 2, AudioSampleFormat::F32),
                };
                out.push(AudioDeviceInfo {
                    id: format!("out:{name}"),
                    name,
                    direction: AudioDeviceDirection::Output,
                    is_default,
                    default_sample_rate: default_sr,
                    min_sample_rate: min_sr,
                    max_sample_rate: max_sr,
                    channels: ch,
                    sample_format: fmt,
                    is_loopback: false,
                });
            }
        }
        out
    }
    #[cfg(not(feature = "hardware"))]
    {
        Vec::new()
    }
}

/// Enumerates all discovered audio devices (both input and output).
pub fn list_audio_devices() -> Vec<AudioDeviceInfo> {
    let mut devices = list_output_devices();
    devices.extend(list_input_devices());
    devices
}

/// Returns metadata for the host's default input device, if any.
pub fn default_input_device_info() -> Option<AudioDeviceInfo> {
    list_input_devices().into_iter().find(|d| d.is_default)
}

/// Returns metadata for the host's default output device, if any.
pub fn default_output_device_info() -> Option<AudioDeviceInfo> {
    list_output_devices().into_iter().find(|d| d.is_default)
}

/// Stateful audio device hot-plug detector comparing snapshots over time.
#[derive(Debug, Default)]
pub struct AudioDeviceMonitor {
    known: Vec<AudioDeviceInfo>,
    default_input: Option<String>,
    default_output: Option<String>,
}

impl AudioDeviceMonitor {
    /// Creates a new monitor seeded with the current host device snapshot.
    pub fn new() -> Self {
        let initial = list_audio_devices();
        Self::from_snapshot(initial)
    }

    /// Creates a monitor seeded from an explicit device snapshot (deterministic tests).
    pub fn from_snapshot(devices: Vec<AudioDeviceInfo>) -> Self {
        let default_input = devices
            .iter()
            .find(|d| d.direction == AudioDeviceDirection::Input && d.is_default)
            .map(|d| d.id.clone());
        let default_output = devices
            .iter()
            .find(|d| d.direction == AudioDeviceDirection::Output && d.is_default)
            .map(|d| d.id.clone());
        Self {
            known: devices,
            default_input,
            default_output,
        }
    }

    /// Polls the live OS audio host and returns any hot-plug events since the last poll.
    pub fn poll_host(&mut self) -> Vec<AudioDeviceEvent> {
        self.diff_against(list_audio_devices())
    }

    /// Diffs a new snapshot against the previously known topology, returning hot-plug events.
    pub fn diff_against(&mut self, current: Vec<AudioDeviceInfo>) -> Vec<AudioDeviceEvent> {
        let mut events = Vec::new();

        for dev in &current {
            if !self.known.iter().any(|k| k.id == dev.id) {
                events.push(AudioDeviceEvent::Added(dev.clone()));
            }
        }
        for prev in &self.known {
            if !current.iter().any(|c| c.id == prev.id) {
                events.push(AudioDeviceEvent::Removed(prev.id.clone()));
            }
        }

        let new_def_in = current
            .iter()
            .find(|d| d.direction == AudioDeviceDirection::Input && d.is_default)
            .map(|d| d.id.clone());
        if new_def_in != self.default_input {
            events.push(AudioDeviceEvent::DefaultInputChanged(new_def_in.clone()));
            self.default_input = new_def_in;
        }

        let new_def_out = current
            .iter()
            .find(|d| d.direction == AudioDeviceDirection::Output && d.is_default)
            .map(|d| d.id.clone());
        if new_def_out != self.default_output {
            events.push(AudioDeviceEvent::DefaultOutputChanged(new_def_out.clone()));
            self.default_output = new_def_out;
        }

        self.known = current;
        events
    }

    /// Returns the most recent known device snapshot.
    pub fn devices(&self) -> &[AudioDeviceInfo] {
        &self.known
    }
}

/// Manages the cpal host output stream connection to the physical soundcard.
pub struct AudioHardwareStream {
    #[cfg(feature = "hardware")]
    stream: cpal::Stream,
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub command_tx: Option<Arc<Mutex<Producer<AudioCommand>>>>,
}

#[cfg(feature = "hardware")]
#[inline]
fn push_far_end_mono(tap: &mut Option<Producer<f32>>, interleaved: &[f32], channels: usize) {
    if let Some(ref mut tx) = tap {
        let ch = channels.max(1);
        let frames = interleaved.len() / ch;
        let inv_ch = 1.0 / ch as f32;
        for f in 0..frames {
            let mut sum = 0.0f32;
            let base = f * ch;
            for c in 0..ch {
                sum += interleaved[base + c];
            }
            let _ = tx.push(sum * inv_ch);
        }
    }
}

impl AudioHardwareStream {
    /// Sends a command to the audio thread via the lock-free SPSC ring buffer.
    pub fn send_command(&self, cmd: AudioCommand) -> Result<(), AudioCommand> {
        if let Some(ref tx_lock) = self.command_tx {
            if let Ok(mut tx) = tx_lock.lock() {
                return tx.push(cmd);
            }
        }
        Err(cmd)
    }

    /// Opens the default system audio output device and starts the playback stream.
    pub fn open(
        mixer: Arc<Mutex<AudioMixer>>,
        preferred_sample_rate: Option<u32>,
    ) -> Result<Self, AudioError> {
        Self::open_device(None, mixer, preferred_sample_rate, None)
    }

    /// Opens a specific (or default if `None`) output device with an optional lock-free
    /// mono far-end reference tap for Acoustic Echo Cancellation (AEC).
    pub fn open_device(
        device_id: Option<&str>,
        mixer: Arc<Mutex<AudioMixer>>,
        preferred_sample_rate: Option<u32>,
        far_end_tap: Option<Producer<f32>>,
    ) -> Result<Self, AudioError> {
        #[cfg(feature = "hardware")]
        {
            let (command_tx, command_rx) =
                SpscRingBuffer::<AudioCommand>::new(DEFAULT_COMMAND_QUEUE_CAPACITY).split();
            Self::open_internal(
                device_id,
                Some(mixer),
                command_rx,
                Some(Arc::new(Mutex::new(command_tx))),
                preferred_sample_rate,
                far_end_tap,
            )
        }
        #[cfg(not(feature = "hardware"))]
        {
            let _ = (device_id, mixer, preferred_sample_rate, far_end_tap);
            Err(AudioError::DeviceNotAvailable(
                "strata-audio compiled without `hardware` feature".into(),
            ))
        }
    }

    /// Opens the hardware stream with pure lock-free SPSC command communication.
    pub fn open_lock_free(
        preferred_sample_rate: Option<u32>,
    ) -> Result<(Self, Producer<AudioCommand>), AudioError> {
        #[cfg(feature = "hardware")]
        {
            let (command_tx, command_rx) =
                SpscRingBuffer::<AudioCommand>::new(DEFAULT_COMMAND_QUEUE_CAPACITY).split();
            let stream =
                Self::open_internal(None, None, command_rx, None, preferred_sample_rate, None)?;
            Ok((stream, command_tx))
        }
        #[cfg(not(feature = "hardware"))]
        {
            let _ = preferred_sample_rate;
            Err(AudioError::DeviceNotAvailable(
                "strata-audio compiled without `hardware` feature".into(),
            ))
        }
    }

    #[cfg(feature = "hardware")]
    fn open_internal(
        device_id: Option<&str>,
        mixer: Option<Arc<Mutex<AudioMixer>>>,
        mut command_rx: Consumer<AudioCommand>,
        command_tx: Option<Arc<Mutex<Producer<AudioCommand>>>>,
        preferred_sample_rate: Option<u32>,
        mut far_end_tap: Option<Producer<f32>>,
    ) -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = if let Some(wanted) = device_id {
            let trimmed = wanted.strip_prefix("out:").unwrap_or(wanted);
            host.output_devices()
                .map_err(|e| AudioError::DeviceNotAvailable(e.to_string()))?
                .find(|d| {
                    d.name()
                        .map(|n| n == trimmed || n.contains(trimmed))
                        .unwrap_or(false)
                })
                .ok_or_else(|| {
                    AudioError::DeviceNotFound(format!("Output device '{wanted}' not found"))
                })?
        } else {
            host.default_output_device()
                .ok_or_else(|| AudioError::DeviceNotFound("No default output device on host".into()))?
        };

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

        let mut local_mixer = AudioMixer::new(sample_rate, channels as usize);

        // Update shared mixer configuration if present
        if let Some(ref m) = mixer {
            if let Ok(mut lock) = m.lock() {
                lock.sample_rate = sample_rate;
                lock.channels = channels as usize;
                local_mixer.master_gain = lock.master_gain;
                local_mixer.state = lock.state;
            }
        }

        let err_fn = |err| eprintln!("strata_audio hardware stream error: {err}");
        let ch_usize = channels as usize;

        let stream = match sample_format {
            cpal::SampleFormat::F32 => {
                let mixer_fallback = mixer.clone();
                device.build_output_stream(
                    &config,
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        while let Some(cmd) = command_rx.pop() {
                            match cmd {
                                AudioCommand::Enqueue(active) => local_mixer.enqueue_active(active),
                                AudioCommand::SetMasterGain(gain) => local_mixer.master_gain = gain,
                                AudioCommand::SetState(state) => local_mixer.state = state,
                                AudioCommand::Clear => local_mixer.clear(),
                            }
                        }
                        if let Some(ref m) = mixer_fallback {
                            if let Ok(mut lock) = m.try_lock() {
                                while let Some(item) = lock.queue.pop_front() {
                                    local_mixer.enqueue_active(item);
                                }
                                local_mixer.master_gain = lock.master_gain;
                                local_mixer.state = lock.state;
                            }
                        }
                        local_mixer.render_interleaved_f32(data);
                        push_far_end_mono(&mut far_end_tap, data, ch_usize);
                    },
                    err_fn,
                    None,
                )
            }
            cpal::SampleFormat::I16 => {
                let mixer_fallback = mixer.clone();
                let mut scratch = Vec::new();
                device.build_output_stream(
                    &config,
                    move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                        if scratch.len() != data.len() {
                            scratch.resize(data.len(), 0.0);
                        }
                        while let Some(cmd) = command_rx.pop() {
                            match cmd {
                                AudioCommand::Enqueue(active) => local_mixer.enqueue_active(active),
                                AudioCommand::SetMasterGain(gain) => local_mixer.master_gain = gain,
                                AudioCommand::SetState(state) => local_mixer.state = state,
                                AudioCommand::Clear => local_mixer.clear(),
                            }
                        }
                        if let Some(ref m) = mixer_fallback {
                            if let Ok(mut lock) = m.try_lock() {
                                while let Some(item) = lock.queue.pop_front() {
                                    local_mixer.enqueue_active(item);
                                }
                                local_mixer.master_gain = lock.master_gain;
                                local_mixer.state = lock.state;
                            }
                        }
                        local_mixer.render_interleaved_f32(&mut scratch);
                        push_far_end_mono(&mut far_end_tap, &scratch, ch_usize);
                        for (out, &src) in data.iter_mut().zip(scratch.iter()) {
                            *out = (src * 32767.0).clamp(-32768.0, 32767.0) as i16;
                        }
                    },
                    err_fn,
                    None,
                )
            }
            cpal::SampleFormat::U16 => {
                let mixer_fallback = mixer.clone();
                let mut scratch = Vec::new();
                device.build_output_stream(
                    &config,
                    move |data: &mut [u16], _: &cpal::OutputCallbackInfo| {
                        if scratch.len() != data.len() {
                            scratch.resize(data.len(), 0.0);
                        }
                        while let Some(cmd) = command_rx.pop() {
                            match cmd {
                                AudioCommand::Enqueue(active) => local_mixer.enqueue_active(active),
                                AudioCommand::SetMasterGain(gain) => local_mixer.master_gain = gain,
                                AudioCommand::SetState(state) => local_mixer.state = state,
                                AudioCommand::Clear => local_mixer.clear(),
                            }
                        }
                        if let Some(ref m) = mixer_fallback {
                            if let Ok(mut lock) = m.try_lock() {
                                while let Some(item) = lock.queue.pop_front() {
                                    local_mixer.enqueue_active(item);
                                }
                                local_mixer.master_gain = lock.master_gain;
                                local_mixer.state = lock.state;
                            }
                        }
                        local_mixer.render_interleaved_f32(&mut scratch);
                        push_far_end_mono(&mut far_end_tap, &scratch, ch_usize);
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
            command_tx,
        })
    }

    /// Resumes playback of the hardware output stream.
    pub fn play(&self) -> Result<(), AudioError> {
        #[cfg(feature = "hardware")]
        {
            self.stream
                .play()
                .map_err(|e| AudioError::StreamPlayError(e.to_string()))
        }
        #[cfg(not(feature = "hardware"))]
        {
            Err(AudioError::DeviceNotAvailable(
                "strata-audio compiled without `hardware` feature".into(),
            ))
        }
    }

    /// Pauses playback of the hardware output stream.
    pub fn pause(&self) -> Result<(), AudioError> {
        #[cfg(feature = "hardware")]
        {
            self.stream
                .pause()
                .map_err(|e| AudioError::StreamPlayError(e.to_string()))
        }
        #[cfg(not(feature = "hardware"))]
        {
            Err(AudioError::DeviceNotAvailable(
                "strata-audio compiled without `hardware` feature".into(),
            ))
        }
    }
}
