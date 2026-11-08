use strata_audio::{
    compute_magnitude_spectrum, AudioBuffer, AudioContext, AudioListener, BiquadFilter,
    BiquadFilterType, Oscillator, OscillatorType, Panner3D, Vec3,
};

#[test]
fn test_audio_buffer_metrics_and_operations() {
    let mut buf = AudioBuffer::new(2, 44100, 44100);
    assert_eq!(buf.channel_count(), 2);
    assert_eq!(buf.length(), 44100);
    assert_eq!(buf.duration_seconds(), 1.0);

    // Set sample values
    buf.channels[0][0] = 0.5;
    buf.channels[1][0] = -0.8;

    assert!((buf.peak_amplitude() - 0.8).abs() < 1e-4);
    assert!(buf.rms_level() > 0.0);

    // Gain scaling
    buf.apply_gain(0.5);
    assert!((buf.peak_amplitude() - 0.4).abs() < 1e-4);

    // Interleaving
    let interleaved = buf.interleave_stereo();
    assert_eq!(interleaved.len(), 44100 * 2);
    assert!((interleaved[0] - 0.25).abs() < 1e-4);
    assert!((interleaved[1] - (-0.4)).abs() < 1e-4);
}

#[test]
fn test_parametric_oscillators() {
    let mut sine = Oscillator::new(OscillatorType::Sine, 440.0);
    let samples = sine.render_samples(100, 44100);
    assert_eq!(samples.len(), 100);
    assert!(samples.iter().all(|&s| (-1.0..=1.0).contains(&s)));

    // Detune by 100 cents (1 semitone higher)
    let detuned = Oscillator::new(OscillatorType::Sine, 440.0).with_detune(100.0);
    assert!(detuned.effective_frequency() > 440.0);

    let mut square = Oscillator::new(OscillatorType::Square, 440.0);
    let sq_samples = square.render_samples(50, 44100);
    assert!(sq_samples.iter().all(|&s| s == 1.0 || s == -1.0));
}

#[test]
fn test_biquad_lowpass_filter() {
    // Generate high-frequency tone (8000 Hz)
    let mut high_osc = Oscillator::new(OscillatorType::Sine, 8000.0);
    let mut buf = AudioBuffer {
        sample_rate: 44100,
        channels: vec![high_osc.render_samples(1000, 44100)],
    };

    let initial_rms = buf.rms_level();

    // Apply lowpass filter at 1000 Hz
    let mut lowpass = BiquadFilter::new(BiquadFilterType::Lowpass, 1000.0, std::f32::consts::FRAC_1_SQRT_2, 44100);
    lowpass.process_buffer(&mut buf);

    let filtered_rms = buf.rms_level();

    // High frequency signal must be heavily attenuated by 1000Hz lowpass filter
    assert!(filtered_rms < initial_rms * 0.2, "8000 Hz tone must be attenuated by 1000 Hz lowpass filter");
}

#[test]
fn test_spatial_3d_audio_physics() {
    let listener = AudioListener {
        position: Vec3::new(0.0, 0.0, 0.0),
        forward: Vec3::new(0.0, 0.0, -1.0),
        up: Vec3::new(0.0, 1.0, 0.0),
        velocity: Vec3::new(0.0, 0.0, 0.0),
    };

    // Source at ref distance 1.0
    let mut panner = Panner3D::new(Vec3::new(0.0, 0.0, -1.0));
    assert_eq!(panner.compute_gain(&listener), 1.0);

    // Source farther away at distance 5.0 has reduced gain
    panner.position = Vec3::new(0.0, 0.0, -5.0);
    let far_gain = panner.compute_gain(&listener);
    assert!(far_gain < 1.0);

    // Source approaching listener at 50 m/s has Doppler factor > 1.0
    panner.velocity = Vec3::new(0.0, 0.0, 50.0);
    let doppler = panner.compute_doppler_factor(&listener);
    assert!(doppler > 1.0);

    // Source strictly to the right (+X) has higher right channel gain
    panner.position = Vec3::new(10.0, 0.0, 0.0);
    let (left_gain, right_gain) = panner.compute_stereo_panning(&listener);
    assert!(right_gain > left_gain);
}

#[test]
fn test_cooley_tukey_fft_spectrum() {
    let sample_rate = 44100;
    let fft_size = 512;
    // 440 Hz tone
    let mut osc = Oscillator::new(OscillatorType::Sine, 440.0);
    let samples = osc.render_samples(fft_size, sample_rate);

    let spectrum = compute_magnitude_spectrum(&samples, fft_size);
    assert_eq!(spectrum.len(), fft_size / 2);

    // Bin index for 440 Hz: k = (440 * 512) / 44100 ~= 5.11 -> bin 5
    let peak_bin = spectrum
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
        .map(|(idx, _)| idx)
        .unwrap();

    assert!((peak_bin as i32 - 5).abs() <= 1, "Peak frequency bin must match 440 Hz tone");
}

#[test]
fn test_audio_context_spatial_rendering() {
    let ctx = AudioContext::new(44100);
    let mut osc = Oscillator::new(OscillatorType::Sine, 440.0);
    let panner = Panner3D::new(Vec3::new(5.0, 0.0, -5.0));

    let stereo_buf = ctx.render_spatial_source(&mut osc, None, &panner, 512);
    assert_eq!(stereo_buf.channel_count(), 2);
    assert_eq!(stereo_buf.length(), 512);
    assert!(stereo_buf.peak_amplitude() > 0.0);
}
