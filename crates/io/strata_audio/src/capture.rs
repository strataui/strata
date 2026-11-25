//! Full-duplex audio input capture (microphone + loopback), live IEC A-weighted metering,
//! and deterministic mock audio input for headless CI testing.

use crate::device::AudioError;
use crate::fft::compute_magnitude_spectrum;
use crate::ring_buffer::{Consumer, Producer, SpscRingBuffer};
#[cfg(feature = "hardware")]
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::f32::consts::PI;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Default lock-free audio capture ring buffer capacity (~1 second @ 48 kHz stereo).
pub const DEFAULT_CAPTURE_RING_CAPACITY: usize = 96_000;

/// Global latest A-weighted ambient SPL reading (dBA) updated by active input meters.
/// Stored as `f32::to_bits()`, with `u32::MAX` sentinel representing "unavailable".
static LATEST_AMBIENT_SPL_BITS: AtomicU32 = AtomicU32::new(u32::MAX);

/// Returns the most recent A-weighted ambient SPL reading (dBA) captured by `strata-audio`,
/// or `None` if no capture stream has reported a reading yet.
///
/// This allows `strata-device` to bridge live microphone SPL without introducing a crate cycle.
pub fn latest_ambient_spl_dba() -> Option<f32> {
    let bits = LATEST_AMBIENT_SPL_BITS.load(Ordering::Relaxed);
    if bits == u32::MAX {
        None
    } else {
        Some(f32::from_bits(bits))
    }
}

/// Resets the global ambient SPL cache (primarily for deterministic tests).
pub fn clear_latest_ambient_spl_dba() {
    LATEST_AMBIENT_SPL_BITS.store(u32::MAX, Ordering::Relaxed);
}

/// Configuration for opening a hardware or mock audio capture stream.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioInputConfig {
    /// Optional specific input device ID or name substring. `None` opens the host default.
    pub device_id: Option<String>,
    /// Target capture sample rate in Hz (default: `48000`).
    pub sample_rate: u32,
    /// Desired channel count (1 = mono speech, 2 = stereo).
    pub channels: u16,
    /// Request system audio loopback / monitor capture instead of physical microphone.
    pub loopback: bool,
    /// Capacity of the lock-free capture SPSC ring buffer (in interleaved samples).
    pub ring_capacity_samples: usize,
    /// Calibration offset converting `0 dBFS` full-scale digital amplitude to physical SPL (`dBA`).
    /// Typical consumer USB/laptop MEMS microphones sit around `94.0` to `104.0 dB SPL` at 0 dBFS.
    pub calibration_spl_offset_db: f32,
}

impl Default for AudioInputConfig {
    fn default() -> Self {
        Self {
            device_id: None,
            sample_rate: 48_000,
            channels: 1,
            loopback: false,
            ring_capacity_samples: DEFAULT_CAPTURE_RING_CAPACITY,
            calibration_spl_offset_db: 94.0,
        }
    }
}

/// Instantaneous snapshot of real-time input audio levels and spectral bins.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioMeterSnapshot {
    /// Root-mean-square amplitude in `[0.0, 1.0]`.
    pub rms: f32,
    /// Instantaneous block peak amplitude in `[0.0, 1.0]`.
    pub peak: f32,
    /// Decaying peak-hold amplitude in `[0.0, 1.0]`.
    pub peak_hold: f32,
    /// RMS level in decibels relative to full scale (`[-120.0, 0.0] dBFS`).
    pub dbfs: f32,
    /// Approximate IEC 61672 A-weighted sound pressure level (`dBA`).
    pub spl_dba: f32,
    /// `true` if any sample in the last block reached `>= 0.999` full-scale.
    pub clipped: bool,
    /// Active sample rate in Hz.
    pub sample_rate: u32,
    /// Total audio frames metered since stream start.
    pub frames_processed: u64,
    /// 64-bin normalized linear magnitude spectrum for live waveform/EQ visualizers.
    pub spectrum_64: [f32; 64],
}

impl Default for AudioMeterSnapshot {
    fn default() -> Self {
        Self {
            rms: 0.0,
            peak: 0.0,
            peak_hold: 0.0,
            dbfs: -120.0,
            spl_dba: 0.0,
            clipped: false,
            sample_rate: 48_000,
            frames_processed: 0,
            spectrum_64: [0.0; 64],
        }
    }
}

/// Lightweight 2-stage IEC 61672 A-weighting approximation filter (high-pass ~200 Hz + high-shelf roll-off).
#[derive(Debug, Clone)]
struct AWeightingFilter {
    hp_y1: f32,
    hp_x1: f32,
    hp_alpha: f32,
    lp_y1: f32,
    lp_alpha: f32,
}

impl AWeightingFilter {
    fn new(sample_rate: u32) -> Self {
        let sr = sample_rate.max(8_000) as f32;
        // High-pass corner around 220 Hz attenuates sub-bass rumble per A-weighting curve.
        let rc_hp = 1.0 / (2.0 * PI * 220.0);
        let dt = 1.0 / sr;
        let hp_alpha = rc_hp / (rc_hp + dt);
        // Low-pass corner around 12 kHz models upper-ear roll-off.
        let rc_lp = 1.0 / (2.0 * PI * 12_000.0);
        let lp_alpha = dt / (rc_lp + dt);
        Self {
            hp_y1: 0.0,
            hp_x1: 0.0,
            hp_alpha,
            lp_y1: 0.0,
            lp_alpha,
        }
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let hp = self.hp_alpha * (self.hp_y1 + x - self.hp_x1);
        self.hp_x1 = x;
        self.hp_y1 = hp;
        let lp = self.lp_y1 + self.lp_alpha * (hp - self.lp_y1);
        self.lp_y1 = lp;
        lp
    }
}

/// Thread-safe, lock-free real-time audio level and spectrum meter.
pub struct AudioLevelMeter {
    sample_rate: u32,
    calibration_offset_db: f32,
    rms_bits: AtomicU32,
    peak_bits: AtomicU32,
    peak_hold_bits: AtomicU32,
    dbfs_bits: AtomicU32,
    spl_dba_bits: AtomicU32,
    clipped: AtomicBool,
    frames_processed: AtomicU64,
    a_weight: Mutex<AWeightingFilter>,
    spectrum: Mutex<[f32; 64]>,
    fft_ring: Mutex<Vec<f32>>,
}

impl AudioLevelMeter {
    pub fn new(sample_rate: u32, calibration_offset_db: f32) -> Self {
        Self {
            sample_rate: sample_rate.max(8_000),
            calibration_offset_db,
            rms_bits: AtomicU32::new(0.0f32.to_bits()),
            peak_bits: AtomicU32::new(0.0f32.to_bits()),
            peak_hold_bits: AtomicU32::new(0.0f32.to_bits()),
            dbfs_bits: AtomicU32::new((-120.0f32).to_bits()),
            spl_dba_bits: AtomicU32::new(0.0f32.to_bits()),
            clipped: AtomicBool::new(false),
            frames_processed: AtomicU64::new(0),
            a_weight: Mutex::new(AWeightingFilter::new(sample_rate)),
            spectrum: Mutex::new([0.0; 64]),
            fft_ring: Mutex::new(vec![0.0; 256]),
        }
    }

    /// Feeds interleaved PCM samples into the meter and updates all atomic indicators.
    pub fn analyze_interleaved(&self, samples: &[f32], channels: usize) {
        if samples.is_empty() {
            return;
        }
        let ch = channels.max(1);
        let frames = samples.len() / ch;
        let mut sum_sq = 0.0f32;
        let mut a_sum_sq = 0.0f32;
        let mut peak = 0.0f32;
        let mut clipped = false;

        if let Ok(mut aw) = self.a_weight.try_lock() {
            for f in 0..frames {
                let mut mono = 0.0f32;
                for c in 0..ch {
                    let s = samples[f * ch + c];
                    let abs = s.abs();
                    if abs > peak {
                        peak = abs;
                    }
                    if abs >= 0.999 {
                        clipped = true;
                    }
                    sum_sq += s * s;
                    mono += s;
                }
                mono /= ch as f32;
                let weighted = aw.process(mono);
                a_sum_sq += weighted * weighted;
            }
        } else {
            for &s in samples {
                let abs = s.abs();
                peak = peak.max(abs);
                if abs >= 0.999 {
                    clipped = true;
                }
                sum_sq += s * s;
            }
            a_sum_sq = sum_sq / ch as f32;
        }

        let rms = (sum_sq / samples.len() as f32).sqrt();
        let a_rms = (a_sum_sq / frames.max(1) as f32).sqrt();
        let dbfs = if rms <= 1e-6 {
            -120.0
        } else {
            (20.0 * rms.log10()).clamp(-120.0, 0.0)
        };
        let a_dbfs = if a_rms <= 1e-6 {
            -120.0
        } else {
            (20.0 * a_rms.log10()).clamp(-120.0, 0.0)
        };
        let spl_dba = (self.calibration_offset_db + a_dbfs).max(0.0);

        let prev_hold = f32::from_bits(self.peak_hold_bits.load(Ordering::Relaxed));
        let decay = 0.92f32;
        let peak_hold = peak.max(prev_hold * decay);

        self.rms_bits.store(rms.to_bits(), Ordering::Relaxed);
        self.peak_bits.store(peak.to_bits(), Ordering::Relaxed);
        self.peak_hold_bits
            .store(peak_hold.to_bits(), Ordering::Relaxed);
        self.dbfs_bits.store(dbfs.to_bits(), Ordering::Relaxed);
        self.spl_dba_bits
            .store(spl_dba.to_bits(), Ordering::Relaxed);
        self.clipped.store(clipped, Ordering::Relaxed);
        self.frames_processed
            .fetch_add(frames as u64, Ordering::Relaxed);

        // Publish globally for decoupled strata-device ambient SPL bridge.
        LATEST_AMBIENT_SPL_BITS.store(spl_dba.to_bits(), Ordering::Relaxed);

        // Update 64-bin spectrum on a non-blocking try_lock basis (never blocks audio thread).
        if let (Ok(mut ring), Ok(mut bins)) = (self.fft_ring.try_lock(), self.spectrum.try_lock()) {
            let take = frames.min(256);
            if take > 0 {
                ring.rotate_left(take);
                let start = 256 - take;
                for i in 0..take {
                    let f_idx = frames - take + i;
                    let mut mono = 0.0f32;
                    for c in 0..ch {
                        mono += samples[f_idx * ch + c];
                    }
                    ring[start + i] = mono / ch as f32;
                }
                let mags = compute_magnitude_spectrum(&ring, 256);
                // 256-pt FFT produces 128 bins; downsample 2:1 into 64 clean display bins.
                for b in 0..64 {
                    let m0 = mags[b * 2];
                    let m1 = mags[b * 2 + 1];
                    bins[b] = 0.5 * (m0 + m1);
                }
            }
        }
    }

    /// Reads an instantaneous snapshot of all meter metrics.
    pub fn snapshot(&self) -> AudioMeterSnapshot {
        let spectrum_64 = self.spectrum.lock().map(|g| *g).unwrap_or([0.0; 64]);
        AudioMeterSnapshot {
            rms: f32::from_bits(self.rms_bits.load(Ordering::Relaxed)),
            peak: f32::from_bits(self.peak_bits.load(Ordering::Relaxed)),
            peak_hold: f32::from_bits(self.peak_hold_bits.load(Ordering::Relaxed)),
            dbfs: f32::from_bits(self.dbfs_bits.load(Ordering::Relaxed)),
            spl_dba: f32::from_bits(self.spl_dba_bits.load(Ordering::Relaxed)),
            clipped: self.clipped.load(Ordering::Relaxed),
            sample_rate: self.sample_rate,
            frames_processed: self.frames_processed.load(Ordering::Relaxed),
            spectrum_64,
        }
    }
}

/// Callback signature for optional inline real-time capture DSP processing (e.g., `VoicePipeline`).
pub type CaptureProcessorFn = Box<dyn FnMut(&[f32], &mut [f32]) + Send + 'static>;

/// Real-time hardware microphone / system loopback audio capture stream.
pub struct AudioInputStream {
    #[cfg(feature = "hardware")]
    stream: cpal::Stream,
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub is_loopback: bool,
    muted: Arc<AtomicBool>,
    dropped_samples: Arc<AtomicU64>,
    meter: Arc<AudioLevelMeter>,
    consumer: Mutex<Consumer<f32>>,
}

impl AudioInputStream {
    /// Opens a physical microphone or loopback audio input stream via `cpal`.
    pub fn open(config: AudioInputConfig) -> Result<Self, AudioError> {
        Self::open_with_processor(config, None)
    }

    /// Opens an audio input stream with an optional inline real-time DSP processor
    /// (such as [`crate::voice_pipeline::VoicePipeline`]).
    pub fn open_with_processor(
        config: AudioInputConfig,
        processor: Option<CaptureProcessorFn>,
    ) -> Result<Self, AudioError> {
        #[cfg(feature = "hardware")]
        {
            Self::open_hardware(config, processor)
        }
        #[cfg(not(feature = "hardware"))]
        {
            let _ = (config, processor);
            Err(AudioError::DeviceNotAvailable(
                "strata-audio compiled without `hardware` feature".into(),
            ))
        }
    }

    #[cfg(feature = "hardware")]
    fn open_hardware(
        config: AudioInputConfig,
        processor: Option<CaptureProcessorFn>,
    ) -> Result<Self, AudioError> {
        let host = cpal::default_host();

        let device = if let Some(ref wanted) = config.device_id {
            host.input_devices()
                .map_err(|e| AudioError::DeviceNotAvailable(e.to_string()))?
                .find(|d| {
                    d.name()
                        .map(|n| n == *wanted || n.contains(wanted.as_str()))
                        .unwrap_or(false)
                })
                .ok_or_else(|| AudioError::DeviceNotFound(format!("Input device '{wanted}' not found")))?
        } else if config.loopback {
            // Linux PulseAudio/PipeWire exposes loopback monitors as input devices containing "monitor".
            // On macOS/Windows, virtual loopback devices often contain "loopback", "monitor", or "blackhole".
            host.input_devices()
                .ok()
                .and_then(|mut iter| {
                    iter.find(|d| {
                        d.name()
                            .map(|n| {
                                let lower = n.to_lowercase();
                                lower.contains("monitor")
                                    || lower.contains("loopback")
                                    || lower.contains("stereo mix")
                                    || lower.contains("blackhole")
                            })
                            .unwrap_or(false)
                    })
                })
                .or_else(|| host.default_input_device())
                .ok_or_else(|| AudioError::DeviceNotFound("No loopback/input device available".into()))?
        } else {
            host.default_input_device()
                .ok_or_else(|| AudioError::DeviceNotFound("No default input device on host".into()))?
        };

        let device_name = device
            .name()
            .unwrap_or_else(|_| "Default Input Device".into());

        let default_cfg = device
            .default_input_config()
            .map_err(|e| AudioError::DeviceNotAvailable(e.to_string()))?;

        let sample_format = default_cfg.sample_format();
        let channels = config.channels.clamp(1, default_cfg.channels().max(1));
        let sample_rate = if config.sample_rate > 0 {
            config.sample_rate
        } else {
            default_cfg.sample_rate().0
        };

        let stream_config = cpal::StreamConfig {
            channels,
            sample_rate: cpal::SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let ring_cap = config
            .ring_capacity_samples
            .max(1024)
            .next_power_of_two();
        let (mut producer, consumer) = SpscRingBuffer::<f32>::new(ring_cap).split();

        let muted = Arc::new(AtomicBool::new(false));
        let dropped_samples = Arc::new(AtomicU64::new(0));
        let meter = Arc::new(AudioLevelMeter::new(
            sample_rate,
            config.calibration_spl_offset_db,
        ));

        let muted_cb = muted.clone();
        let dropped_cb = dropped_samples.clone();
        let meter_cb = meter.clone();
        let ch_usize = channels as usize;
        let err_fn = |err| eprintln!("strata_audio input stream error: {err}");

        let stream = match sample_format {
            cpal::SampleFormat::F32 => {
                let mut proc_opt = processor;
                let mut processed = Vec::new();
                device.build_input_stream(
                    &stream_config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        process_and_push(
                            data,
                            ch_usize,
                            &muted_cb,
                            &meter_cb,
                            &dropped_cb,
                            &mut producer,
                            &mut proc_opt,
                            &mut processed,
                        );
                    },
                    err_fn,
                    None,
                )
            }
            cpal::SampleFormat::I16 => {
                let mut proc_opt = processor;
                let mut norm_buf = Vec::new();
                let mut processed = Vec::new();
                device.build_input_stream(
                    &stream_config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        norm_buf.clear();
                        norm_buf.extend(data.iter().map(|&s| s as f32 / 32768.0));
                        process_and_push(
                            &norm_buf,
                            ch_usize,
                            &muted_cb,
                            &meter_cb,
                            &dropped_cb,
                            &mut producer,
                            &mut proc_opt,
                            &mut processed,
                        );
                    },
                    err_fn,
                    None,
                )
            }
            cpal::SampleFormat::U16 => {
                let mut proc_opt = processor;
                let mut norm_buf = Vec::new();
                let mut processed = Vec::new();
                device.build_input_stream(
                    &stream_config,
                    move |data: &[u16], _: &cpal::InputCallbackInfo| {
                        norm_buf.clear();
                        norm_buf.extend(data.iter().map(|&s| (s as f32 / 32767.5) - 1.0));
                        process_and_push(
                            &norm_buf,
                            ch_usize,
                            &muted_cb,
                            &meter_cb,
                            &dropped_cb,
                            &mut producer,
                            &mut proc_opt,
                            &mut processed,
                        );
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
            is_loopback: config.loopback,
            muted,
            dropped_samples,
            meter,
            consumer: Mutex::new(consumer),
        })
    }

    /// Starts or resumes hardware capture.
    pub fn start(&self) -> Result<(), AudioError> {
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

    /// Pauses hardware capture.
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

    /// Mutes or unmutes captured audio (stream remains active emitting silence).
    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
    }

    /// Returns whether capture is currently muted.
    pub fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }

    /// Returns total samples dropped due to consumer ring buffer overrun.
    pub fn dropped_samples(&self) -> u64 {
        self.dropped_samples.load(Ordering::Relaxed)
    }

    /// Returns the live atomic [`AudioLevelMeter`] attached to this capture stream.
    pub fn meter(&self) -> Arc<AudioLevelMeter> {
        self.meter.clone()
    }

    /// Reads up to `out.len()` interleaved `f32` PCM samples from the lock-free capture buffer.
    pub fn read_samples(&self, out: &mut [f32]) -> usize {
        let Ok(mut rx) = self.consumer.lock() else {
            return 0;
        };
        let mut count = 0;
        for slot in out.iter_mut() {
            if let Some(sample) = rx.pop() {
                *slot = sample;
                count += 1;
            } else {
                break;
            }
        }
        count
    }
}

#[allow(clippy::too_many_arguments)]
fn process_and_push(
    input: &[f32],
    channels: usize,
    muted: &AtomicBool,
    meter: &AudioLevelMeter,
    dropped: &AtomicU64,
    producer: &mut Producer<f32>,
    processor: &mut Option<CaptureProcessorFn>,
    scratch: &mut Vec<f32>,
) {
    if scratch.len() != input.len() {
        scratch.resize(input.len(), 0.0);
    }

    if muted.load(Ordering::Relaxed) {
        scratch.fill(0.0);
    } else if let Some(ref mut proc_fn) = processor {
        proc_fn(input, scratch);
    } else {
        scratch.copy_from_slice(input);
    }

    meter.analyze_interleaved(scratch, channels);

    let mut drop_count = 0u64;
    for &s in scratch.iter() {
        if producer.push(s).is_err() {
            drop_count += 1;
        }
    }
    if drop_count > 0 {
        dropped.fetch_add(drop_count, Ordering::Relaxed);
    }
}

/// Deterministic mock microphone input stream for headless CI, DSP testing, and UI previews.
pub struct MockAudioInput {
    pub sample_rate: u32,
    pub channels: u16,
    muted: AtomicBool,
    dropped_samples: AtomicU64,
    meter: Arc<AudioLevelMeter>,
    producer: Mutex<Producer<f32>>,
    consumer: Mutex<Consumer<f32>>,
    processor: Mutex<Option<CaptureProcessorFn>>,
    scratch: Mutex<Vec<f32>>,
}

impl MockAudioInput {
    pub fn new(config: AudioInputConfig) -> Self {
        let cap = config
            .ring_capacity_samples
            .max(1024)
            .next_power_of_two();
        let (producer, consumer) = SpscRingBuffer::<f32>::new(cap).split();
        Self {
            sample_rate: config.sample_rate.max(8_000),
            channels: config.channels.max(1),
            muted: AtomicBool::new(false),
            dropped_samples: AtomicU64::new(0),
            meter: Arc::new(AudioLevelMeter::new(
                config.sample_rate.max(8_000),
                config.calibration_spl_offset_db,
            )),
            producer: Mutex::new(producer),
            consumer: Mutex::new(consumer),
            processor: Mutex::new(None),
            scratch: Mutex::new(Vec::new()),
        }
    }

    /// Attaches an inline real-time DSP processor to the mock capture pipeline.
    pub fn set_processor(&self, processor: Option<CaptureProcessorFn>) {
        if let Ok(mut guard) = self.processor.lock() {
            *guard = processor;
        }
    }

    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
    }

    pub fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }

    pub fn meter(&self) -> Arc<AudioLevelMeter> {
        self.meter.clone()
    }

    pub fn dropped_samples(&self) -> u64 {
        self.dropped_samples.load(Ordering::Relaxed)
    }

    /// Injects interleaved PCM samples through the identical mute -> DSP -> meter -> ring path.
    pub fn inject_samples(&self, samples: &[f32]) {
        if let (Ok(mut prod), Ok(mut proc_opt), Ok(mut scratch)) = (
            self.producer.lock(),
            self.processor.lock(),
            self.scratch.lock(),
        ) {
            process_and_push(
                samples,
                self.channels as usize,
                &self.muted,
                &self.meter,
                &self.dropped_samples,
                &mut prod,
                &mut proc_opt,
                &mut scratch,
            );
        }
    }

    /// Injects a pure sine tone across all channels for `frames` sample frames.
    pub fn inject_sine(&self, frequency_hz: f32, amplitude: f32, frames: usize) {
        let ch = self.channels as usize;
        let mut buf = Vec::with_capacity(frames * ch);
        let sr = self.sample_rate as f32;
        for f in 0..frames {
            let s = (2.0 * PI * frequency_hz * f as f32 / sr).sin() * amplitude;
            for _ in 0..ch {
                buf.push(s);
            }
        }
        self.inject_samples(&buf);
    }

    /// Reads up to `out.len()` interleaved PCM samples from the mock capture ring buffer.
    pub fn read_samples(&self, out: &mut [f32]) -> usize {
        let Ok(mut rx) = self.consumer.lock() else {
            return 0;
        };
        let mut count = 0;
        for slot in out.iter_mut() {
            if let Some(sample) = rx.pop() {
                *slot = sample;
                count += 1;
            } else {
                break;
            }
        }
        count
    }
}
