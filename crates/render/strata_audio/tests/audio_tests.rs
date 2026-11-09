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

#[test]
fn test_audio_buffer_resampling_modes() {
    use strata_audio::ResampleQuality;

    // Generate 1 second of 440 Hz tone at 44100 Hz
    let mut osc = Oscillator::new(OscillatorType::Sine, 440.0);
    let original_samples = osc.render_samples(44100, 44100);
    let buf_44k = AudioBuffer {
        sample_rate: 44100,
        channels: vec![original_samples.clone(), original_samples],
    };

    // 1. Resample up to 48000 Hz using Cubic
    let buf_48k = buf_44k.resample(48000);
    assert_eq!(buf_48k.sample_rate, 48000);
    assert_eq!(buf_48k.channel_count(), 2);
    assert_eq!(buf_48k.length(), 48000);
    assert!((buf_48k.duration_seconds() - 1.0).abs() < 1e-4);

    // 2. Resample using Sinc
    let buf_sinc = buf_44k.resample_with_quality(48000, ResampleQuality::Sinc { taps: 16 });
    assert_eq!(buf_sinc.sample_rate, 48000);
    assert_eq!(buf_sinc.length(), 48000);

    // 3. Resample down to 22050 Hz using Linear
    let buf_22k = buf_44k.resample_with_quality(22050, ResampleQuality::Linear);
    assert_eq!(buf_22k.sample_rate, 22050);
    assert_eq!(buf_22k.length(), 22050);

    // Verify frequency peak remains at 440 Hz via FFT on resampled 48k signal
    let fft_size = 1024;
    let spectrum_48k = compute_magnitude_spectrum(&buf_48k.channels[0][..fft_size], fft_size);
    // Bin for 440 Hz in 48000 Hz with 1024 points: (440 * 1024) / 48000 = 9.38 -> bin 9
    let peak_bin_48k = spectrum_48k
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
        .map(|(idx, _)| idx)
        .unwrap();

    assert!((peak_bin_48k as i32 - 9).abs() <= 1, "Resampled peak bin must match 440 Hz tone in 48kHz spectrum");
}

#[test]
fn test_audio_mixer_automatic_resampling() {
    use strata_audio::AudioMixer;

    let mut mixer = AudioMixer::new(48000, 2);

    // Provide buffer at 24000 Hz (half of mixer rate)
    let mut buf = AudioBuffer::new(2, 240, 24000);
    buf.channels[0].fill(0.6);
    buf.channels[1].fill(0.4);

    mixer.enqueue(buf, 1.0, false);
    assert_eq!(mixer.active_buffer_count(), 1);

    // Active buffer inside mixer should have been resampled to 48000 Hz, with 480 samples
    let active = &mixer.queue[0];
    assert_eq!(active.buffer.sample_rate, 48000);
    assert_eq!(active.buffer.length(), 480);

    // Render 480 stereo frames
    let mut output = vec![0.0f32; 960];
    mixer.render_interleaved_f32(&mut output);

    // Verify samples rendered properly
    for i in 10..470 {
        assert!((output[i * 2] - 0.6).abs() < 1e-2, "Left channel resampled level");
        assert!((output[i * 2 + 1] - 0.4).abs() < 1e-2, "Right channel resampled level");
    }
}

#[test]
fn test_hrtf_spatial_binaural_convolution() {
    use strata_audio::SpatializationMode;

    let ctx = AudioContext::new(44100);
    let mut osc = Oscillator::new(OscillatorType::Sine, 1000.0);

    // Emitter strictly to the right (+X)
    let panner_right = Panner3D::new(Vec3::new(4.0, 0.0, 0.0)).with_mode(SpatializationMode::Hrtf);
    let buf_right = ctx.render_spatial_source(&mut osc, None, &panner_right, 512);

    assert_eq!(buf_right.channel_count(), 2);
    assert_eq!(buf_right.length(), 512);

    let left_rms = {
        let sum: f32 = buf_right.channels[0].iter().map(|s| s * s).sum();
        (sum / 512.0).sqrt()
    };
    let right_rms = {
        let sum: f32 = buf_right.channels[1].iter().map(|s| s * s).sum();
        (sum / 512.0).sqrt()
    };

    // Right ear is ipsilateral, Left ear is shadowed by head (ILD)
    assert!(right_rms > left_rms, "Ipsilateral ear must have higher RMS than shadowed ear: right={} left={}", right_rms, left_rms);

    // Check Woodworth ITD onset delay
    assert_eq!(buf_right.channels[0][0], 0.0, "Contralateral left ear should experience initial ITD arrival delay");

    // Test EqualPower fallback mode
    let panner_eq = Panner3D::new(Vec3::new(4.0, 0.0, 0.0)).with_mode(SpatializationMode::EqualPower);
    let buf_eq = ctx.render_spatial_source(&mut osc, None, &panner_eq, 512);
    assert_eq!(buf_eq.channel_count(), 2);
}
