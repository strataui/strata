# strata_audio

`strata_audio` is the pure-Rust spatial 3D audio synthesizer, digital signal processor (DSP), and audio graph execution engine of the Strata framework. Engineered without external C dependencies or dynamic audio library linkages, it delivers real-time 3D spatial panning, physical Doppler shift simulation, inverse distance attenuation, biquad IIR filtering, and Radix-2 Fast Fourier Transform (FFT) spectral analysis for acoustic cloud infrastructure alerts and telemetry sonification.

---

## Architectural Mission

Modern server management cockpits and operations centers benefit significantly from multi-sensory telemetry feedback. When managing thousands of distributed microservices, human visual attention is easily saturated by complex dashboards. Spatial auditory cues provide immediate directional awareness for critical system events:
- **Directional Fault Localization**: An operator wearing headphones immediately perceives which quadrant of a server cluster or topology graph generated an incident based on 3D spatial audio panning.
- **Physical Attenuation and Doppler Cues**: Rapidly scaling server clusters or approaching load spikes emit frequency-modulated acoustic signals indicating velocity and proximity.
- **Pure-Rust Determinism**: Unlike frameworks binding to heavyweight audio backends (like OpenAL, FMOD, or PortAudio), `strata_audio` runs natively across desktop, server headless CI, and WebAssembly with zero audio-driver lockup risks.

---

## Subsystem Architecture

```text
┌─────────────────────────────────────────────────────────────────┐
│               AudioContext (Graph Coordinator)                  │
│       - Sample Rate Management (e.g. 44,100 Hz / 48,000 Hz)     │
│       - Master Gain & Graph Mixing Topology                     │
└────────────────────────────────┬────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────┐
│               AudioListener & 3D Spatial Geometry               │
│   - Position Vector (x, y, z), Velocity Vector                  │
│   - Forward Orientation & Up Normal Vector                      │
└────────────────────────────────┬────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────┐
│              Parametric Waveform Synthesis (Oscillator)         │
│   - Waveforms: Sine, Square, Sawtooth, Triangle, WhiteNoise     │
│   - Cents Detuning Formula: f * 2^(cents / 1200)                │
└────────────────┬───────────────────────────────┬────────────────┘
                 │                               │
                 ▼                               ▼
┌─────────────────────────────────┐   ┌───────────────────────────┐
│      Digital Biquad Filter      │   │     3D Spatial Panner     │
│   - Lowpass, Highpass, Bandpass │   │   - W3C Inverse Distance  │
│   - Direct Form I IIR Equations │   │   - Equal-Power Stereo Pan│
│   - Robert Bristow-Johnson Coeff│   │   - Doppler Shift Factor  │
└────────────────┬────────────────┘   └─────────────┬─────────────┘
                 │                                  │
                 └────────────────┬─────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────┐
│                  AudioBuffer & FFT Spectral Core                │
│   - Multi-Channel PCM 32-bit Floating Point Samples [-1.0, 1.0] │
│   - RMS Energy & Peak Amplitude Diagnostics                     │
│   - In-Place Radix-2 Cooley-Tukey FFT & Hann Windowing          │
└─────────────────────────────────────────────────────────────────┘
```

---

## 3D Spatial Audio Geometry and Physics

Spatial audio in `strata_audio` models real-world acoustic propagation in three-dimensional Cartesian space.

### The Spatial Vector: `Vec3`

Spatial coordinates, directions, and velocities are represented by `strata_audio::spatial::Vec3`:

```rust
use strata_audio::spatial::Vec3;

let source_pos = Vec3::new(12.0, 0.0, -8.0);
let listener_pos = Vec3::new(0.0, 0.0, 0.0);
let delta = source_pos.sub(listener_pos);

assert_eq!(delta.length(), (144.0 + 64.0f32).sqrt());
```

### The Listener: `AudioListener`

The `AudioListener` represents the position and head orientation of the user or monitoring operator:

```rust
use strata_audio::spatial::{AudioListener, Vec3};

let listener = AudioListener {
    position: Vec3::new(0.0, 0.0, 0.0),
    forward: Vec3::new(0.0, 0.0, -1.0), // Looking forward into -Z
    up: Vec3::new(0.0, 1.0, 0.0),        // Up vector +Y
    velocity: Vec3::new(0.0, 0.0, 0.0),  // Stationary observer
};
```

### Distance Gain Attenuation

Sound intensity drops with distance according to the W3C Inverse Distance model:

$$\text{Gain}(d) = \frac{d_{\text{ref}}}{d_{\text{ref}} + \text{rolloff} \cdot (\operatorname{clamp}(d, d_{\text{ref}}, d_{\text{max}}) - d_{\text{ref}})}$$

Where:
- $d = \| \mathbf{P}_{\text{source}} - \mathbf{P}_{\text{listener}} \|$
- $d_{\text{ref}}$: Reference distance where gain equals $1.0$ (default $1.0 \text{ m}$).
- $d_{\text{max}}$: Maximum distance beyond which attenuation levels off.
- $\text{rolloff}$: Rolloff coefficient governing attenuation steepness.

### Equal-Power Stereo Panning

To place audio sources along the stereo soundstage without causing perceived loudness drops when panning between speakers, `strata_audio` calculates equal-power panning curves:

1. The listener's right axis vector is computed via vector cross product:
   $$\mathbf{R} = \frac{\mathbf{F}_{\text{listener}} \times \mathbf{U}_{\text{listener}}}{\| \mathbf{F}_{\text{listener}} \times \mathbf{U}_{\text{listener}} \|}$$
2. The normalized direction from listener to sound source is projected onto $\mathbf{R}$:
   $$\operatorname{pan} = \operatorname{clamp}\left(\frac{\mathbf{P}_{\text{source}} - \mathbf{P}_{\text{listener}}}{\| \mathbf{P}_{\text{source}} - \mathbf{P}_{\text{listener}} \|} \cdot \mathbf{R}, -1.0, 1.0\right)$$
3. Left and right channel gains are evaluated along trigonometric quarter-cycle arcs:
   $$\theta = (\operatorname{pan} + 1.0) \cdot \frac{\pi}{4}$$
   $$\text{Gain}_{\text{left}} = \cos(\theta), \quad \text{Gain}_{\text{right}} = \sin(\theta)$$

$$\text{Gain}_{\text{left}}^2 + \text{Gain}_{\text{right}}^2 = \cos^2(\theta) + \sin^2(\theta) = 1.0 \quad (\text{Constant Acoustic Power})$$

### Physical Doppler Frequency Shift

When an emitter or observer moves, wavefront compression produces a perceptible frequency shift:

$$f_{\text{observed}} = f_{\text{emitter}} \cdot \left(\frac{c - v_{\text{listener}}}{c - v_{\text{source}}}\right)$$

Where:
- $c = 343.3 \text{ m/s}$ (speed of sound in air at $20^\circ\text{C}$).
- $v_{\text{listener}} = \mathbf{V}_{\text{listener}} \cdot \hat{\mathbf{u}}$
- $v_{\text{source}} = \mathbf{V}_{\text{source}} \cdot \hat{\mathbf{u}}$
- $\hat{\mathbf{u}} = \frac{\mathbf{P}_{\text{listener}} - \mathbf{P}_{\text{source}}}{\| \mathbf{P}_{\text{listener}} - \mathbf{P}_{\text{source}} \|}$

```rust
use strata_audio::spatial::{AudioListener, Panner3D, Vec3};

let mut listener = AudioListener::default();
let mut panner = Panner3D::new(Vec3::new(10.0, 0.0, 0.0));

// Approaching source moving at 34.33 m/s (~123 km/h) towards stationary listener
panner.velocity = Vec3::new(-34.33, 0.0, 0.0);

let doppler_factor = panner.compute_doppler_factor(&listener);
println!("Doppler frequency scaling factor: {:.4}x", doppler_factor);
assert!(doppler_factor > 1.0); // Frequency shifts upward as emitter approaches
```

---

## Parametric Audio Synthesis (`Oscillator`)

`strata_audio::synth::Oscillator` synthesizes mathematically pure periodic waveforms and white noise:

```rust
use strata_audio::synth::{Oscillator, OscillatorType};

// Initialize an alarm tone at 440 Hz (Standard Concert Pitch A4)
let mut osc = Oscillator::new(OscillatorType::Sine, 440.0)
    .with_detune(25.0); // Detune +25 cents for harmonic warmth

// Synthesize 1024 mono audio samples at 44,100 Hz
let samples = osc.render_samples(1024, 44100);
assert_eq!(samples.len(), 1024);
```

### Supported Waveform Types

- `OscillatorType::Sine`: Pure single-frequency harmonic sine wave: $\sin(\omega t)$.
- `OscillatorType::Square`: Bipolar square wave ($\pm 1.0$).
- `OscillatorType::Sawtooth`: Linear ramp wave containing both even and odd harmonics.
- `OscillatorType::Triangle`: Symmetric linear triangular wave: $\frac{2}{\pi} \arcsin(\sin(\omega t))$.
- `OscillatorType::WhiteNoise`: High-entropy pseudo-random stochastic noise generated via linear congruential calculation.

---

## Digital Biquad IIR Filtering (`BiquadFilter`)

To sculpt audio frequency response (e.g. simulating acoustic damping through walls or high-frequency isolation for network alerts), `strata_audio::filter::BiquadFilter` implements a Direct Form I Infinite Impulse Response (IIR) filter:

$$y[n] = b_0 x[n] + b_1 x[n-1] + b_2 x[n-2] - a_1 y[n-1] - a_2 y[n-2]$$

Filter coefficients are computed using Robert Bristow-Johnson Audio EQ formulas:

```rust
use strata_audio::filter::{BiquadFilter, BiquadFilterType};

// Create a resonant lowpass filter cutting off frequencies above 1200 Hz with Q factor 2.0
let mut filter = BiquadFilter::new(
    BiquadFilterType::Lowpass,
    1200.0, // Cutoff in Hz
    2.0,    // Resonance Q factor
    44100,  // Sample rate in Hz
);

let filtered_sample = filter.process_sample(0.85);
```

Supported filter topologies:
- `BiquadFilterType::Lowpass`: Attenuates high frequencies above cutoff.
- `BiquadFilterType::Highpass`: Attenuates low frequencies below cutoff.
- `BiquadFilterType::Bandpass`: Passes only a narrow frequency band centered at cutoff.
- `BiquadFilterType::Notch`: Rejects a narrow band of frequencies (band-stop).

---

## Fast Fourier Transform (FFT) and Spectral Analysis

`strata_audio::fft` implements an in-place Radix-2 Decimation-In-Time (DIT) Fast Fourier Transform for real-time acoustic signal telemetry and frequency spectrum visualization.

```rust
use strata_audio::fft::{compute_magnitude_spectrum, magnitudes_to_decibels};

// Analyze 1024 audio samples
let fft_size = 1024;
let audio_samples = vec![0.0f32; fft_size];

// Compute magnitude spectrum across N/2 bins with Hann windowing
let magnitudes = compute_magnitude_spectrum(&audio_samples, fft_size);
assert_eq!(magnitudes.len(), 512);

// Convert linear amplitudes to decibels [dB]
let decibels = magnitudes_to_decibels(&magnitudes);
assert_eq!(decibels.len(), 512);
```

The Cooley-Tukey butterfly stage computes complex twiddle factor multiplications in-place:

$$W_N^k = e^{-i \frac{2\pi k}{N}} = \cos\left(\frac{2\pi k}{N}\right) - i \sin\left(\frac{2\pi k}{N}\right)$$

---

## Multi-Channel Audio Buffer (`AudioBuffer`)

`AudioBuffer` stores multi-channel 32-bit floating-point PCM audio data normalized in $[-1.0, 1.0]$.

```rust
use strata_audio::sample::AudioBuffer;

// 2 channels (Stereo), 44,100 samples (1.0 second duration), 44,100 Hz
let mut buffer = AudioBuffer::new(2, 44100, 44100);

assert_eq!(buffer.duration_seconds(), 1.0);
assert_eq!(buffer.channel_count(), 2);

// Apply gain scaling
buffer.apply_gain(0.75);

// Interleave stereo channels into flat buffer [L0, R0, L1, R1, ...]
let interleaved = buffer.interleave_stereo();
assert_eq!(interleaved.len(), 88200);
```

---

## Complete Working Rust Example: Directional Cloud Datacenter Incident Alarm

The following complete Rust example demonstrates configuring an acoustic telemetry monitor that renders a directional 3D alert for a failing server node:

```rust
use strata_audio::context::AudioContext;
use strata_audio::filter::{BiquadFilter, BiquadFilterType};
use strata_audio::spatial::{AudioListener, Panner3D, Vec3};
use strata_audio::synth::{Oscillator, OscillatorType};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    const SAMPLE_RATE: u32 = 44100;
    const SAMPLE_COUNT: usize = 2048; // ~46.4 milliseconds of audio

    // 1. Initialize Audio Context
    let mut ctx = AudioContext::new(SAMPLE_RATE);
    ctx.master_gain = 0.8;

    // 2. Configure Operator Listener Position (Center of Ops Room)
    ctx.listener = AudioListener {
        position: Vec3::new(0.0, 0.0, 0.0),
        forward: Vec3::new(0.0, 0.0, -1.0),
        up: Vec3::new(0.0, 1.0, 0.0),
        velocity: Vec3::default(),
    };

    // 3. Configure Incident Source Panner (Failing Server Rack: 15m to Right, 5m Forward)
    let failing_rack_position = Vec3::new(15.0, 0.0, -5.0);
    let mut panner = Panner3D::new(failing_rack_position);
    panner.ref_distance = 2.0;
    panner.rolloff_factor = 1.2;

    // 4. Configure Alarm Tone (880 Hz Sawtooth Waveform)
    let mut alarm_osc = Oscillator::new(OscillatorType::Sawtooth, 880.0);

    // 5. Apply Resonant Highpass Filter to simulate industrial urgency
    let mut filter = BiquadFilter::new(
        BiquadFilterType::Highpass,
        400.0,
        1.5,
        SAMPLE_RATE,
    );

    // 6. Synthesize and Spatialize Audio Source
    let rendered_audio = ctx.render_spatial_source(
        &mut alarm_osc,
        Some(&mut filter),
        &panner,
        SAMPLE_COUNT,
    );

    println!("Synthesized {} channels of audio.", rendered_audio.channel_count());
    println!("Peak amplitude: {:.4}", rendered_audio.peak_amplitude());
    println!("RMS acoustic energy level: {:.4}", rendered_audio.rms_level());

    // Verify directional stereo bias: sound source is to the right (+X)
    let left_rms = rendered_audio.channels[0].iter().map(|&s| s * s).sum::<f32>().sqrt();
    let right_rms = rendered_audio.channels[1].iter().map(|&s| s * s).sum::<f32>().sqrt();

    println!("Left Channel RMS: {:.4} | Right Channel RMS: {:.4}", left_rms, right_rms);
    assert!(right_rms > left_rms, "Audio must be panned to the right channel");

    Ok(())
}
```
