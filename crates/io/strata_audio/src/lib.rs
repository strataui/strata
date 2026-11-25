//! Full-duplex pure-Rust audio I/O, spatial 3D synthesizer, real-time voice DSP
//! (Adaptive Noise Cancellation, Acoustic Echo Cancellation, AGC, VAD, Dynamics, EQ),
//! multi-format codecs, streaming player, MIDI 1.0, and speech engine for Project Strata.
//!
//! # Examples
//!
//! ```rust
//! use strata_audio::{AudioBuffer, Oscillator, OscillatorType};
//!
//! let mut osc = Oscillator::new(OscillatorType::Sine, 440.0);
//! let samples = osc.render_samples(100, 44100);
//! assert_eq!(samples.len(), 100);
//!
//! let buf = AudioBuffer::new(2, 100, 44100);
//! assert_eq!(buf.channel_count(), 2);
//! ```

pub mod aec;
pub mod capture;
pub mod codecs;
pub mod context;
pub mod device;
pub mod dynamics;
pub mod fft;
pub mod filter;
#[cfg(feature = "reactive")]
pub mod hooks;
pub mod midi;
pub mod noise_cancel;
pub mod player;
pub mod resample;
pub mod ring_buffer;
pub mod sample;
pub mod spatial;
pub mod speech;
pub mod stft;
pub mod synth;
pub mod vad;
pub mod voice_pipeline;
pub mod wav;

pub use aec::{AecConfig, EchoCanceller};
pub use capture::{
    clear_latest_ambient_spl_dba, latest_ambient_spl_dba, AudioInputConfig, AudioInputStream,
    AudioLevelMeter, AudioMeterSnapshot, CaptureProcessorFn, MockAudioInput,
};
pub use codecs::{decode_audio_bytes, decode_audio_file, detect_audio_format, AudioCodecFormat};
pub use context::AudioContext;
pub use device::{
    default_input_device_info, default_output_device_info, list_audio_devices, list_input_devices,
    list_output_devices, ActiveBuffer, AudioCommand, AudioContextState, AudioDeviceDirection,
    AudioDeviceEvent, AudioDeviceInfo, AudioDeviceMonitor, AudioError, AudioHardwareStream,
    AudioMixer, AudioSampleFormat,
};
pub use dynamics::{AgcProcessor, CompressorLimiter, DeEsser, NoiseGate, ParametricEq};
pub use fft::{
    compute_magnitude_spectrum, fft_radix2, ifft_radix2, magnitudes_to_decibels, Complex, FftPlan,
};
pub use filter::{BiquadFilter, BiquadFilterType};
#[cfg(feature = "reactive")]
pub use hooks::{
    publish_audio_level_snapshot, refresh_audio_devices, use_audio_devices, use_audio_level,
    use_microphone, use_midi_ports, MicrophoneHandle,
};
pub use midi::{
    list_midi_ports, MidiMessage, MidiParser, MidiPortDirection, MidiPortInfo, MockMidiBus,
};
pub use noise_cancel::{NoiseSuppressionConfig, NoiseSuppressionPreset, NoiseSuppressor};
pub use player::{AudioPlayer, PlaybackState};
pub use resample::{resample_channel, ResampleQuality};
pub use ring_buffer::{Consumer, Producer, SpscRingBuffer};
pub use sample::AudioBuffer;
pub use spatial::{apply_fractional_delay, AudioListener, Panner3D, SpatializationMode, Vec3};
pub use speech::{
    MockSpeechRecognizer, MockSpeechSynthesizer, SpeechOptions, SpeechRecognitionResult,
    SpeechRecognizer, SpeechSynthesizer, SpokenUtterance,
};
pub use stft::StftProcessor;
pub use synth::{Oscillator, OscillatorType};
pub use vad::{VadConfig, VadDecision, VoiceActivityDetector};
pub use voice_pipeline::{VoicePipeline, VoicePipelineConfig, VoicePipelineStats};
pub use wav::{decode_wav_bytes, encode_wav_bytes, WavSampleFormat, WavSpec, WavWriter};

/// Convenient prelude re-exporting primary `strata-audio` types, DSP engines, MIDI, speech, and hooks.
pub mod prelude {
    pub use crate::aec::{AecConfig, EchoCanceller};
    pub use crate::capture::{
        latest_ambient_spl_dba, AudioInputConfig, AudioInputStream, AudioLevelMeter,
        AudioMeterSnapshot, MockAudioInput,
    };
    pub use crate::codecs::{
        decode_audio_bytes, decode_audio_file, detect_audio_format, AudioCodecFormat,
    };
    pub use crate::context::AudioContext;
    pub use crate::device::{
        list_audio_devices, list_input_devices, list_output_devices, AudioDeviceDirection,
        AudioDeviceEvent, AudioDeviceInfo, AudioDeviceMonitor, AudioError, AudioMixer,
    };
    pub use crate::dynamics::{AgcProcessor, CompressorLimiter, DeEsser, NoiseGate, ParametricEq};
    pub use crate::fft::{fft_radix2, ifft_radix2, Complex, FftPlan};
    pub use crate::filter::{BiquadFilter, BiquadFilterType};
    #[cfg(feature = "reactive")]
    pub use crate::hooks::{
        use_audio_devices, use_audio_level, use_microphone, use_midi_ports, MicrophoneHandle,
    };
    pub use crate::midi::{
        list_midi_ports, MidiMessage, MidiParser, MidiPortDirection, MidiPortInfo, MockMidiBus,
    };
    pub use crate::noise_cancel::{
        NoiseSuppressionConfig, NoiseSuppressionPreset, NoiseSuppressor,
    };
    pub use crate::player::{AudioPlayer, PlaybackState};
    pub use crate::sample::AudioBuffer;
    pub use crate::spatial::{AudioListener, Panner3D, SpatializationMode, Vec3};
    pub use crate::speech::{
        MockSpeechRecognizer, MockSpeechSynthesizer, SpeechOptions, SpeechRecognitionResult,
        SpeechRecognizer, SpeechSynthesizer, SpokenUtterance,
    };
    pub use crate::stft::StftProcessor;
    pub use crate::synth::{Oscillator, OscillatorType};
    pub use crate::vad::{VadConfig, VadDecision, VoiceActivityDetector};
    pub use crate::voice_pipeline::{VoicePipeline, VoicePipelineConfig, VoicePipelineStats};
    pub use crate::wav::{decode_wav_bytes, encode_wav_bytes, WavSampleFormat, WavWriter};
}
