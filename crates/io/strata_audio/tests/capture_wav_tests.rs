//! Phase 2 integration tests: full-duplex capture, A-weighted SPL metering, hot-plug device
//! monitoring, pure-Rust WAV encoding/decoding, and reactive Strata audio hooks.

use strata_audio::capture::{
    clear_latest_ambient_spl_dba, latest_ambient_spl_dba, AudioInputConfig, MockAudioInput,
};
use strata_audio::device::{
    AudioDeviceDirection, AudioDeviceEvent, AudioDeviceInfo, AudioDeviceMonitor, AudioMixer,
    AudioSampleFormat,
};
use strata_audio::sample::AudioBuffer;
use strata_audio::wav::{decode_wav_bytes, encode_wav_bytes, WavSampleFormat, WavWriter};
use std::f32::consts::PI;

fn make_stereo_sine(sample_rate: u32, frames: usize, freq_l: f32, freq_r: f32) -> AudioBuffer {
    let mut left = Vec::with_capacity(frames);
    let mut right = Vec::with_capacity(frames);
    for i in 0..frames {
        let t = i as f32 / sample_rate as f32;
        left.push((2.0 * PI * freq_l * t).sin() * 0.75);
        right.push((2.0 * PI * freq_r * t).sin() * 0.5);
    }
    AudioBuffer {
        sample_rate,
        channels: vec![left, right],
    }
}

#[test]
fn test_wav_roundtrip_pcm16_pcm24_float32() {
    let src = make_stereo_sine(48_000, 1024, 440.0, 880.0);

    // 1. Float32 exact round-trip
    let f32_bytes = encode_wav_bytes(&src, WavSampleFormat::Float32).unwrap();
    let f32_dec = decode_wav_bytes(&f32_bytes).unwrap();
    assert_eq!(f32_dec.sample_rate, 48_000);
    assert_eq!(f32_dec.channel_count(), 2);
    assert_eq!(f32_dec.length(), 1024);
    for ch in 0..2 {
        for i in 0..1024 {
            assert!((f32_dec.channels[ch][i] - src.channels[ch][i]).abs() < 1e-6);
        }
    }

    // 2. PCM24 studio precision (< -130 dB quantization error)
    let p24_bytes = encode_wav_bytes(&src, WavSampleFormat::Pcm24).unwrap();
    let p24_dec = decode_wav_bytes(&p24_bytes).unwrap();
    for ch in 0..2 {
        for i in 0..1024 {
            assert!((p24_dec.channels[ch][i] - src.channels[ch][i]).abs() < 1e-5);
        }
    }

    // 3. PCM16 CD precision
    let p16_bytes = encode_wav_bytes(&src, WavSampleFormat::Pcm16).unwrap();
    let p16_dec = decode_wav_bytes(&p16_bytes).unwrap();
    for ch in 0..2 {
        for i in 0..1024 {
            assert!((p16_dec.channels[ch][i] - src.channels[ch][i]).abs() < 1e-3);
        }
    }
}

#[test]
fn test_wav_streaming_writer_and_unknown_chunk_skipping() {
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut writer = WavWriter::new(&mut cursor, 1, 16_000, WavSampleFormat::Pcm16).unwrap();
        let block = vec![0.25f32; 800];
        writer.write_interleaved_f32(&block).unwrap();
        writer.write_interleaved_f32(&block).unwrap();
        assert_eq!(writer.frames_written(), 1600);
        assert!((writer.duration_seconds() - 0.1).abs() < 1e-6);
        writer.finalize().unwrap();
    }

    let mut raw = cursor.into_inner();
    // Inject a JUNK metadata chunk right before `data` to verify resilient chunk scanning.
    let junk_chunk = b"JUNK\x04\x00\x00\x00ABCD";
    let mut patched = Vec::with_capacity(raw.len() + junk_chunk.len());
    patched.extend_from_slice(&raw[..36]);
    patched.extend_from_slice(junk_chunk);
    patched.extend_from_slice(&raw[36..]);
    let new_riff_len = (patched.len() - 8) as u32;
    patched[4..8].copy_from_slice(&new_riff_len.to_le_bytes());
    raw = patched;

    let decoded = decode_wav_bytes(&raw).unwrap();
    assert_eq!(decoded.sample_rate, 16_000);
    assert_eq!(decoded.length(), 1600);
    assert!((decoded.channels[0][500] - 0.25).abs() < 1e-3);
}

#[test]
fn test_mock_microphone_capture_metering_spl_and_inline_processor() {
    clear_latest_ambient_spl_dba();
    let cfg = AudioInputConfig {
        sample_rate: 48_000,
        channels: 1,
        calibration_spl_offset_db: 94.0,
        ..Default::default()
    };
    let mic = MockAudioInput::new(cfg);

    // Attach inline processor scaling amplitude by 0.5x
    mic.set_processor(Some(Box::new(|input, output| {
        for (o, &i) in output.iter_mut().zip(input.iter()) {
            *o = i * 0.5;
        }
    })));

    // Inject 1000 Hz sine @ 0.8 amplitude -> processor outputs 0.4 amplitude (~0.283 RMS, -10.97 dBFS)
    mic.inject_sine(1_000.0, 0.8, 1024);

    let snap = mic.meter().snapshot();
    assert!(snap.rms > 0.25 && snap.rms < 0.32, "unexpected rms: {}", snap.rms);
    assert!(snap.peak > 0.38 && snap.peak < 0.42, "unexpected peak: {}", snap.peak);
    assert!(snap.dbfs > -13.0 && snap.dbfs < -9.0, "unexpected dbfs: {}", snap.dbfs);
    assert!(snap.spl_dba > 70.0 && snap.spl_dba < 90.0, "unexpected spl: {}", snap.spl_dba);
    assert!(!snap.clipped);
    assert!(latest_ambient_spl_dba().is_some());

    // Read captured samples back from lock-free ring buffer
    let mut read_buf = vec![0.0f32; 512];
    let n = mic.read_samples(&mut read_buf);
    assert_eq!(n, 512);
    assert!(read_buf.iter().any(|&s| s.abs() > 0.3));

    // Verify mute silences output & meter
    mic.set_muted(true);
    mic.inject_sine(1_000.0, 0.9, 512);
    let muted_snap = mic.meter().snapshot();
    assert_eq!(muted_snap.rms, 0.0);
    assert_eq!(muted_snap.dbfs, -120.0);
}

#[test]
fn test_audio_device_monitor_hotplug_diff() {
    let dev_spk = AudioDeviceInfo {
        id: "out:Built-in Speakers".into(),
        name: "Built-in Speakers".into(),
        direction: AudioDeviceDirection::Output,
        is_default: true,
        default_sample_rate: 48_000,
        min_sample_rate: 44_100,
        max_sample_rate: 96_000,
        channels: 2,
        sample_format: AudioSampleFormat::F32,
        is_loopback: false,
    };
    let dev_usb_mic = AudioDeviceInfo {
        id: "in:USB Studio Mic".into(),
        name: "USB Studio Mic".into(),
        direction: AudioDeviceDirection::Input,
        is_default: true,
        default_sample_rate: 48_000,
        min_sample_rate: 48_000,
        max_sample_rate: 48_000,
        channels: 1,
        sample_format: AudioSampleFormat::F32,
        is_loopback: false,
    };

    let mut monitor = AudioDeviceMonitor::from_snapshot(vec![dev_spk.clone()]);
    // Plug in USB mic
    let ev1 = monitor.diff_against(vec![dev_spk.clone(), dev_usb_mic.clone()]);
    assert!(ev1.contains(&AudioDeviceEvent::Added(dev_usb_mic.clone())));
    assert!(ev1.contains(&AudioDeviceEvent::DefaultInputChanged(Some(
        dev_usb_mic.id.clone()
    ))));

    // Unplug USB mic
    let ev2 = monitor.diff_against(vec![dev_spk]);
    assert!(ev2.contains(&AudioDeviceEvent::Removed(dev_usb_mic.id)));
    assert!(ev2.contains(&AudioDeviceEvent::DefaultInputChanged(None)));
}

#[test]
fn test_audio_mixer_looping_and_oneshot_completion_clean() {
    let mut mixer = AudioMixer::new(48_000, 2);
    let short_buf = AudioBuffer {
        sample_rate: 48_000,
        channels: vec![vec![0.25; 4], vec![0.25; 4]],
    };
    let loop_buf = AudioBuffer {
        sample_rate: 48_000,
        channels: vec![vec![0.5; 4], vec![0.5; 4]],
    };
    mixer.enqueue(short_buf, 1.0, false);
    mixer.enqueue(loop_buf, 1.0, true);
    assert_eq!(mixer.active_buffer_count(), 2);

    let mut out = vec![0.0f32; 16]; // 8 stereo frames
    mixer.render_interleaved_f32(&mut out);

    // First 4 frames should sum 0.25 + 0.5 = 0.75; next 4 frames only looping buffer = 0.5.
    assert!((out[0] - 0.75).abs() < 1e-5);
    assert!((out[6] - 0.75).abs() < 1e-5);
    assert!((out[8] - 0.50).abs() < 1e-5);
    assert!((out[14] - 0.50).abs() < 1e-5);
    assert_eq!(mixer.active_buffer_count(), 1);
}

#[test]
#[cfg(feature = "reactive")]
fn test_reactive_microphone_and_level_hooks() {
    use strata_audio::hooks::{use_audio_level, use_microphone};

    let mic = use_microphone(AudioInputConfig::default());
    assert!(!mic.is_recording().get());

    let mock = mic.start_mock();
    assert!(mic.is_recording().get());

    mock.inject_sine(440.0, 0.5, 512);
    let snap = mic.poll_meter();
    assert!(snap.rms > 0.3);
    assert!(mic.is_speaking().get());
    assert!((use_audio_level().get().rms - snap.rms).abs() < 1e-5);

    mic.set_muted(true);
    mock.inject_sine(440.0, 0.5, 512);
    mic.poll_meter();
    assert!(!mic.is_speaking().get());

    mic.stop();
    assert!(!mic.is_recording().get());
}
