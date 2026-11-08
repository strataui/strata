//! Pure-Rust spatial 3D audio synthesizer and DSP engine for Project Strata.

pub mod context;
pub mod device;
pub mod fft;
pub mod filter;
pub mod sample;
pub mod spatial;
pub mod synth;

pub use context::AudioContext;
pub use device::{
    ActiveBuffer, AudioContextState, AudioError, AudioHardwareStream, AudioMixer,
};
pub use fft::{compute_magnitude_spectrum, fft_radix2, magnitudes_to_decibels, Complex};
pub use filter::{BiquadFilter, BiquadFilterType};
pub use sample::AudioBuffer;
pub use spatial::{AudioListener, Panner3D, Vec3};
pub use synth::{Oscillator, OscillatorType};
