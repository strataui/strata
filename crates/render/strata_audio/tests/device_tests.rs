use strata_audio::device::{AudioContextState, AudioError, AudioMixer};
use strata_audio::{AudioBuffer, AudioContext, Vec3};

#[test]
fn test_audio_mixer_interleaved_rendering() {
    let mut mixer = AudioMixer::new(44100, 2);
    assert_eq!(mixer.channels, 2);
    assert_eq!(mixer.sample_rate, 44100);
    assert_eq!(mixer.state, AudioContextState::Running);
    assert_eq!(mixer.active_buffer_count(), 0);

    // Render into empty mixer -> must output pure silence
    let mut output = [1.0f32; 8];
    mixer.render_interleaved_f32(&mut output);
    assert!(output.iter().all(|&s| s == 0.0), "Empty mixer must output silence");

    // Enqueue a stereo buffer
    let mut test_buf = AudioBuffer::new(2, 4, 44100);
    test_buf.channels[0] = vec![0.2, 0.4, 0.6, 0.8];
    test_buf.channels[1] = vec![-0.2, -0.4, -0.6, -0.8];

    mixer.enqueue(test_buf, 1.0, false);
    assert_eq!(mixer.active_buffer_count(), 1);

    // Render 4 stereo frames (8 samples)
    let mut output = [0.0f32; 8];
    mixer.render_interleaved_f32(&mut output);

    assert_eq!(output[0], 0.2);
    assert_eq!(output[1], -0.2);
    assert_eq!(output[2], 0.4);
    assert_eq!(output[3], -0.4);
    assert_eq!(output[4], 0.6);
    assert_eq!(output[5], -0.6);
    assert_eq!(output[6], 0.8);
    assert_eq!(output[7], -0.8);

    // Non-looping buffer should now be finished and removed
    assert_eq!(mixer.active_buffer_count(), 0);

    // Subsequent render outputs silence
    mixer.render_interleaved_f32(&mut output);
    assert!(output.iter().all(|&s| s == 0.0));
}

#[test]
fn test_audio_mixer_mono_expansion_and_looping() {
    let mut mixer = AudioMixer::new(44100, 2);

    // Mono buffer of 2 samples
    let mut mono_buf = AudioBuffer::new(1, 2, 44100);
    mono_buf.channels[0] = vec![0.5, 0.9];

    // Enqueue looping
    mixer.enqueue(mono_buf, 0.5, true);
    assert_eq!(mixer.active_buffer_count(), 1);

    // Render 4 stereo frames (8 samples)
    let mut output = [0.0f32; 8];
    mixer.render_interleaved_f32(&mut output);

    // Frame 0: mono 0.5 * 0.5 = 0.25 on both L and R
    assert!((output[0] - 0.25).abs() < 1e-5);
    assert!((output[1] - 0.25).abs() < 1e-5);
    // Frame 1: mono 0.9 * 0.5 = 0.45 on both L and R
    assert!((output[2] - 0.45).abs() < 1e-5);
    assert!((output[3] - 0.45).abs() < 1e-5);
    // Frame 2 (wrapped loop): back to 0.25
    assert!((output[4] - 0.25).abs() < 1e-5);
    assert!((output[5] - 0.25).abs() < 1e-5);
    // Frame 3 (wrapped loop): back to 0.45
    assert!((output[6] - 0.45).abs() < 1e-5);
    assert!((output[7] - 0.45).abs() < 1e-5);

    // Buffer remains active because it is looping
    assert_eq!(mixer.active_buffer_count(), 1);

    mixer.clear();
    assert_eq!(mixer.active_buffer_count(), 0);
}

#[test]
fn test_audio_mixer_suspended_state() {
    let mut mixer = AudioMixer::new(44100, 2);
    let mut buf = AudioBuffer::new(2, 4, 44100);
    buf.channels[0] = vec![1.0; 4];
    buf.channels[1] = vec![1.0; 4];
    mixer.enqueue(buf, 1.0, false);

    mixer.state = AudioContextState::Suspended;
    let mut output = [99.0f32; 8];
    mixer.render_interleaved_f32(&mut output);

    assert!(output.iter().all(|&s| s == 0.0), "Suspended mixer must output silence");
}

#[test]
fn test_audio_context_lifecycle_and_mixing() {
    let mut ctx = AudioContext::new(44100);
    assert_eq!(ctx.state(), AudioContextState::Running);

    ctx.set_master_gain(0.8);
    assert!((ctx.master_gain - 0.8).abs() < 1e-5);

    // Test tone enqueuing
    ctx.play_tone(440.0, 0.01); // 10ms of 440 Hz
    {
        let mixer = ctx.mixer();
        let guard = mixer.lock().unwrap();
        assert!(guard.active_buffer_count() > 0);
    }

    // Render via mixer
    let mut output = vec![0.0f32; 128];
    ctx.mixer().lock().unwrap().render_interleaved_f32(&mut output);
    assert!(output.iter().any(|&s| s.abs() > 0.0), "Synthesized tone must produce non-zero samples");

    // Suspend
    ctx.suspend().unwrap();
    assert_eq!(ctx.state(), AudioContextState::Suspended);

    // Resume
    ctx.resume().unwrap();
    assert_eq!(ctx.state(), AudioContextState::Running);

    // Close
    ctx.close().unwrap();
    assert_eq!(ctx.state(), AudioContextState::Closed);
    assert!(matches!(ctx.resume(), Err(AudioError::ContextClosed)));
}

#[test]
fn test_audio_context_spatial_tone_playback() {
    let ctx = AudioContext::new(44100);
    ctx.play_spatial_tone(880.0, Vec3::new(5.0, 0.0, -2.0), 0.02);

    let mixer = ctx.mixer();
    let mut lock = mixer.lock().unwrap();
    assert_eq!(lock.active_buffer_count(), 1);

    let mut output = vec![0.0f32; 256];
    lock.render_interleaved_f32(&mut output);
    assert!(output.iter().any(|&s| s.abs() > 0.0));
}

#[test]
fn test_audio_host_and_device_enumeration() {
    use cpal::traits::HostTrait;
    let host = cpal::default_host();
    let host_id = host.id();
    println!("Detected audio host: {:?}", host_id);

    // Enumerating devices must not panic
    if let Ok(devices) = host.output_devices() {
        for (i, dev) in devices.enumerate() {
            use cpal::traits::DeviceTrait;
            let name = dev.name().unwrap_or_else(|_| "Unknown".into());
            println!("Output device #{}: {}", i, name);
        }
    }
}
