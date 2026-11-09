//! Pure-Rust spatial 3D audio synthesizer and DSP engine for Project Strata.
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

pub mod context;
pub mod device;
pub mod fft;
pub mod filter;
pub mod resample;
pub mod ring_buffer;
pub mod sample;
pub mod spatial;
pub mod synth;

pub use context::AudioContext;
pub use device::{
    ActiveBuffer, AudioCommand, AudioContextState, AudioError, AudioHardwareStream, AudioMixer,
};
pub use fft::{compute_magnitude_spectrum, fft_radix2, magnitudes_to_decibels, Complex};
pub use filter::{BiquadFilter, BiquadFilterType};
pub use resample::{resample_channel, ResampleQuality};
pub use ring_buffer::{Consumer, Producer, SpscRingBuffer};
pub use sample::AudioBuffer;
pub use spatial::{apply_fractional_delay, AudioListener, Panner3D, SpatializationMode, Vec3};
pub use synth::{Oscillator, OscillatorType};
