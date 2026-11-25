//! Phase 4 integration tests: multi-format audio codecs, streaming AudioPlayer, MIDI 1.0
//! wire parser/serializer & loopback bus, and OS Speech TTS/STT mocks.

use std::f32::consts::PI;
use std::time::Duration;
use strata_audio::codecs::{decode_audio_bytes, detect_audio_format, AudioCodecFormat};
use strata_audio::midi::{MidiMessage, MidiParser, MockMidiBus};
use strata_audio::player::{AudioPlayer, PlaybackState};
use strata_audio::sample::AudioBuffer;
use strata_audio::speech::{
    MockSpeechRecognizer, MockSpeechSynthesizer, SpeechOptions, SpeechRecognizer,
};
use strata_audio::wav::{encode_wav_bytes, WavSampleFormat};

#[test]
fn test_codec_format_sniffing_and_wav_decoding() {
    assert_eq!(detect_audio_format(b"fLaC\x00\x00", None), AudioCodecFormat::Flac);
    assert_eq!(detect_audio_format(b"OggS\x00\x02", None), AudioCodecFormat::OggVorbis);
    assert_eq!(detect_audio_format(b"ID3\x04\x00", None), AudioCodecFormat::Mp3);
    assert_eq!(detect_audio_format(b"\x00\x00\x00\x18ftypM4A ", None), AudioCodecFormat::Mp4);
    assert_eq!(detect_audio_format(b"unknown", Some(".mp3")), AudioCodecFormat::Mp3);

    let buf = AudioBuffer {
        sample_rate: 24_000,
        channels: vec![vec![0.5; 480]],
    };
    let wav = encode_wav_bytes(&buf, WavSampleFormat::Pcm16).unwrap();
    assert_eq!(detect_audio_format(&wav, None), AudioCodecFormat::Wav);

    let decoded = decode_audio_bytes(&wav, Some("wav")).unwrap();
    assert_eq!(decoded.sample_rate, 24_000);
    assert_eq!(decoded.length(), 480);
    assert!((decoded.channels[0][100] - 0.5).abs() < 1e-3);
}

#[test]
fn test_streaming_audio_player_seek_rate_loop_mute() {
    let sr = 48_000u32;
    let frames = 48_000usize; // 1.0 second
    let samples: Vec<f32> = (0..frames)
        .map(|i| (2.0 * PI * 440.0 * i as f32 / sr as f32).sin() * 0.6)
        .collect();
    let buf = AudioBuffer {
        sample_rate: sr,
        channels: vec![samples],
    };

    let mut player = AudioPlayer::new();
    assert_eq!(player.state(), PlaybackState::Idle);
    player.load_buffer(buf);
    assert_eq!(player.state(), PlaybackState::Paused);
    assert!((player.duration().as_secs_f64() - 1.0).abs() < 1e-3);

    // Seek to 0.25s and play at 2.0x speed for 0.25s output -> advances 0.5s to 0.75s!
    player.seek(Duration::from_millis(250));
    player.set_playback_rate(2.0);
    player.play();

    let mut out = vec![0.0f32; 12_000 * 2]; // 0.25s stereo @ 48 kHz
    player.render_interleaved_f32(&mut out, sr, 2);
    assert!((player.current_time().as_secs_f64() - 0.75).abs() < 0.01);
    assert!(out.iter().any(|&s| s.abs() > 0.3));

    // Mute outputs exact zeros while still advancing playback clock
    player.set_muted(true);
    player.render_interleaved_f32(&mut out, sr, 2);
    assert!(out.iter().all(|&s| s == 0.0));
    assert_eq!(player.state(), PlaybackState::Ended);

    // Looping wraps seamlessly without entering Ended
    player.set_muted(false);
    player.set_looping(true);
    player.play();
    player.render_interleaved_f32(&mut out, sr, 2);
    assert_eq!(player.state(), PlaybackState::Playing);
}

#[test]
fn test_midi_parser_running_status_interleaved_realtime_and_sysex() {
    let bus = MockMidiBus::new();
    let mut parser = MidiParser::new();

    // 1. Standard structured round-trip
    let sent = vec![
        MidiMessage::NoteOn {
            channel: 2,
            note: 60,
            velocity: 100,
        },
        MidiMessage::PitchBend {
            channel: 2,
            value: -4096,
        },
        MidiMessage::ControlChange {
            channel: 0,
            controller: 64,
            value: 127,
        },
        MidiMessage::SysEx(vec![0x7E, 0x7F, 0x06, 0x01]),
        MidiMessage::NoteOff {
            channel: 2,
            note: 60,
            velocity: 0,
        },
    ];
    for m in &sent {
        bus.send_message(m);
    }
    let received = bus.receive_messages(&mut parser);
    assert_eq!(received, sent);

    // 2. Raw wire edge cases: Running status + NoteOn vel=0 -> NoteOff + TimingClock INSIDE SysEx!
    let raw_wire = [
        0x91, 64, 110, // NoteOn ch1 note 64 vel 110 (sets running status 0x91)
        67, 0, // Running status 0x91, vel 0 -> normalized NoteOff ch1 note 67 vel 0
        0xF0, 0x43, 0xF8, 0x12, 0xF7, // SysEx [0x43, 0x12] interrupted mid-stream by TimingClock 0xF8!
    ];
    bus.send_raw(&raw_wire);
    let parsed = bus.receive_messages(&mut parser);
    assert_eq!(
        parsed,
        vec![
            MidiMessage::NoteOn {
                channel: 1,
                note: 64,
                velocity: 110
            },
            MidiMessage::NoteOff {
                channel: 1,
                note: 67,
                velocity: 0
            },
            MidiMessage::TimingClock,
            MidiMessage::SysEx(vec![0x43, 0x12]),
        ]
    );
}

#[test]
fn test_mock_speech_tts_and_stt_bridge() {
    let tts = MockSpeechSynthesizer::new();
    let opts = SpeechOptions {
        rate: 1.25,
        pitch: 1.0,
        volume: 0.9,
        voice: Some("en-US".into()),
    };
    tts.speak("Altitude nominal, 120 meters", &opts);
    let log = tts.utterances();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].text, "Altitude nominal, 120 meters");
    assert_eq!(log[0].options.rate, 1.25);

    let mut stt = MockSpeechRecognizer::with_transcripts(vec!["engage return to home".into()]);
    let voiced = vec![0.25f32; 480];
    assert!(stt.feed_audio(&voiced, 48_000).is_none());
    let res = stt.feed_audio(&voiced, 48_000).unwrap();
    assert!(res.is_final);
    assert_eq!(res.transcript, "engage return to home");
}
