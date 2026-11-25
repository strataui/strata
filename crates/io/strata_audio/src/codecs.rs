//! Multi-format pure-Rust audio codec decoding (built-in zero-dep WAV + optional `symphonia`
//! for MP3, FLAC, OGG/Vorbis, AAC, M4A/MP4, and PCM).

use crate::device::AudioError;
use crate::sample::AudioBuffer;
use crate::wav::decode_wav_bytes;
use std::path::Path;

/// Detected audio container / codec format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioCodecFormat {
    Wav,
    Mp3,
    Flac,
    OggVorbis,
    Aac,
    Mp4,
    Unknown,
}

/// Sniffs audio container format from magic header bytes and optional file extension hint.
pub fn detect_audio_format(bytes: &[u8], hint_ext: Option<&str>) -> AudioCodecFormat {
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        return AudioCodecFormat::Wav;
    }
    if bytes.len() >= 4 && &bytes[0..4] == b"fLaC" {
        return AudioCodecFormat::Flac;
    }
    if bytes.len() >= 4 && &bytes[0..4] == b"OggS" {
        return AudioCodecFormat::OggVorbis;
    }
    if bytes.len() >= 3 && &bytes[0..3] == b"ID3" {
        return AudioCodecFormat::Mp3;
    }
    if bytes.len() >= 2 && bytes[0] == 0xFF && (bytes[1] & 0xE0) == 0xE0 {
        // Distinguish ADTS AAC vs MPEG Layer III frame sync when possible
        if (bytes[1] & 0x06) == 0x00 {
            return AudioCodecFormat::Aac;
        }
        return AudioCodecFormat::Mp3;
    }
    if bytes.len() >= 8 && &bytes[4..8] == b"ftyp" {
        return AudioCodecFormat::Mp4;
    }

    if let Some(ext) = hint_ext {
        match ext.trim_start_matches('.').to_ascii_lowercase().as_str() {
            "wav" | "wave" => return AudioCodecFormat::Wav,
            "mp3" => return AudioCodecFormat::Mp3,
            "flac" => return AudioCodecFormat::Flac,
            "ogg" | "oga" => return AudioCodecFormat::OggVorbis,
            "aac" | "adts" => return AudioCodecFormat::Aac,
            "m4a" | "mp4" => return AudioCodecFormat::Mp4,
            _ => {}
        }
    }

    AudioCodecFormat::Unknown
}

/// Decodes encoded audio bytes (WAV always supported; MP3/FLAC/OGG/AAC/MP4 under `codecs` feature)
/// into a normalized floating-point [`AudioBuffer`].
pub fn decode_audio_bytes(
    bytes: &[u8],
    hint_ext: Option<&str>,
) -> Result<AudioBuffer, AudioError> {
    let fmt = detect_audio_format(bytes, hint_ext);
    if fmt == AudioCodecFormat::Wav {
        return decode_wav_bytes(bytes);
    }

    #[cfg(feature = "codecs")]
    {
        decode_with_symphonia(bytes, hint_ext)
    }
    #[cfg(not(feature = "codecs"))]
    {
        Err(AudioError::UnsupportedFormat(format!(
            "Audio format {:?} requires `strata-audio` feature `codecs`",
            fmt
        )))
    }
}

/// Loads and decodes an audio file from disk using its extension and header magic.
pub fn decode_audio_file(path: &Path) -> Result<AudioBuffer, AudioError> {
    let bytes = std::fs::read(path)
        .map_err(|e| AudioError::DeviceNotAvailable(format!("Failed to read audio file: {e}")))?;
    let ext = path.extension().and_then(|e| e.to_str());
    decode_audio_bytes(&bytes, ext)
}

#[cfg(feature = "codecs")]
fn decode_with_symphonia(bytes: &[u8], hint_ext: Option<&str>) -> Result<AudioBuffer, AudioError> {
    use std::io::Cursor;
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
    use symphonia::core::errors::Error as SymphoniaError;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let cursor = Cursor::new(bytes.to_vec());
    let mss = MediaSourceStream::new(Box::new(cursor), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = hint_ext {
        hint.with_extension(ext.trim_start_matches('.'));
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| AudioError::UnsupportedFormat(format!("Codec probe failed: {e}")))?;

    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| AudioError::UnsupportedFormat("No decodable audio track found".into()))?;

    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(44_100);
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| AudioError::UnsupportedFormat(format!("Decoder init failed: {e}")))?;

    let mut channels: Vec<Vec<f32>> = Vec::new();
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(pkt) => pkt,
            Err(SymphoniaError::IoError(err))
                if err.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(SymphoniaError::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(err) => {
                return Err(AudioError::UnsupportedFormat(format!(
                    "Packet read error: {err}"
                )));
            }
        };

        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                let ch_count = spec.channels.count().max(1);
                if channels.is_empty() {
                    channels = vec![Vec::new(); ch_count];
                }
                let buf = sample_buf.get_or_insert_with(|| {
                    SampleBuffer::<f32>::new(decoded.capacity() as u64, spec)
                });
                buf.copy_interleaved_ref(decoded);
                let interleaved = buf.samples();
                let frames = interleaved.len() / ch_count;
                for f in 0..frames {
                    for (ch, ch_vec) in channels.iter_mut().enumerate().take(ch_count) {
                        ch_vec.push(interleaved[f * ch_count + ch]);
                    }
                }
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(err) => {
                return Err(AudioError::UnsupportedFormat(format!(
                    "Audio decode error: {err}"
                )));
            }
        }
    }

    if channels.is_empty() {
        channels.push(Vec::new());
    }

    Ok(AudioBuffer {
        sample_rate,
        channels,
    })
}
