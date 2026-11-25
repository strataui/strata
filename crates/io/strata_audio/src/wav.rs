//! Pure-Rust zero-dependency RIFF/WAVE encoder, decoder, and streaming disk recorder.
//!
//! Supports PCM 8-bit unsigned, PCM 16-bit signed, PCM 24-bit signed, PCM 32-bit signed,
//! and IEEE 754 32-bit float across arbitrary channel counts and sample rates.

use crate::device::AudioError;
use crate::sample::AudioBuffer;
use std::io::{Seek, SeekFrom, Write};

/// Encoding sample format for WAV export and streaming recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WavSampleFormat {
    /// 16-bit signed little-endian integer PCM (standard CD quality).
    Pcm16,
    /// 24-bit signed little-endian integer PCM (studio mastering).
    Pcm24,
    /// 32-bit IEEE 754 floating-point PCM (lossless headroom).
    Float32,
}

impl WavSampleFormat {
    #[inline]
    pub const fn bits_per_sample(self) -> u16 {
        match self {
            Self::Pcm16 => 16,
            Self::Pcm24 => 24,
            Self::Float32 => 32,
        }
    }

    #[inline]
    pub const fn bytes_per_sample(self) -> u16 {
        self.bits_per_sample() / 8
    }

    #[inline]
    pub const fn format_tag(self) -> u16 {
        match self {
            Self::Pcm16 | Self::Pcm24 => 1, // WAVE_FORMAT_PCM
            Self::Float32 => 3,             // WAVE_FORMAT_IEEE_FLOAT
        }
    }
}

/// Metadata parsed from a WAV `fmt ` chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WavSpec {
    pub channels: u16,
    pub sample_rate: u32,
    pub bits_per_sample: u16,
    pub is_float: bool,
}

/// Encodes an [`AudioBuffer`] into a complete RIFF/WAVE byte vector.
pub fn encode_wav_bytes(
    buffer: &AudioBuffer,
    format: WavSampleFormat,
) -> Result<Vec<u8>, AudioError> {
    let channels = buffer.channel_count();
    if channels == 0 || channels > u16::MAX as usize {
        return Err(AudioError::UnsupportedFormat(format!(
            "Invalid channel count for WAV encoding: {channels}"
        )));
    }
    if buffer.sample_rate == 0 {
        return Err(AudioError::UnsupportedFormat(
            "Sample rate must be non-zero for WAV encoding".into(),
        ));
    }

    let frames = buffer.length();
    let bytes_per_sample = format.bytes_per_sample() as usize;
    let data_bytes = frames * channels * bytes_per_sample;
    let mut cursor = std::io::Cursor::new(Vec::with_capacity(44 + data_bytes));

    {
        let mut writer = WavWriter::new(&mut cursor, channels as u16, buffer.sample_rate, format)?;
        let mut interleaved = Vec::with_capacity(frames * channels);
        for f in 0..frames {
            for ch in 0..channels {
                interleaved.push(buffer.channels[ch][f]);
            }
        }
        writer.write_interleaved_f32(&interleaved)?;
        writer.finalize()?;
    }

    Ok(cursor.into_inner())
}

/// Decodes a RIFF/WAVE byte slice into a normalized floating-point [`AudioBuffer`].
pub fn decode_wav_bytes(bytes: &[u8]) -> Result<AudioBuffer, AudioError> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(AudioError::UnsupportedFormat(
            "Not a valid RIFF/WAVE header".into(),
        ));
    }

    let mut pos = 12usize;
    let mut spec: Option<WavSpec> = None;
    let mut data_slice: Option<&[u8]> = None;

    while pos + 8 <= bytes.len() {
        let chunk_id = &bytes[pos..pos + 4];
        let chunk_size =
            u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        pos += 8;

        let available = bytes.len().saturating_sub(pos);
        let effective_size = chunk_size.min(available);
        let payload = &bytes[pos..pos + effective_size];

        if chunk_id == b"fmt " {
            if payload.len() < 16 {
                return Err(AudioError::UnsupportedFormat(
                    "WAV fmt chunk is truncated".into(),
                ));
            }
            let audio_format = u16::from_le_bytes([payload[0], payload[1]]);
            let channels = u16::from_le_bytes([payload[2], payload[3]]);
            let sample_rate =
                u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]);
            let bits_per_sample = u16::from_le_bytes([payload[14], payload[15]]);

            // Support WAVE_FORMAT_EXTENSIBLE (0xFFFE) by inspecting subformat GUID first 2 bytes.
            let (is_float, valid_fmt) = match audio_format {
                1 => (false, true),
                3 => (true, true),
                0xFFFE if payload.len() >= 40 => {
                    let sub_tag = u16::from_le_bytes([payload[24], payload[25]]);
                    match sub_tag {
                        1 => (false, true),
                        3 => (true, true),
                        _ => (false, false),
                    }
                }
                _ => (false, false),
            };

            if !valid_fmt || channels == 0 || sample_rate == 0 {
                return Err(AudioError::UnsupportedFormat(format!(
                    "Unsupported WAV format tag {audio_format}, channels={channels}, rate={sample_rate}"
                )));
            }

            spec = Some(WavSpec {
                channels,
                sample_rate,
                bits_per_sample,
                is_float,
            });
        } else if chunk_id == b"data" {
            data_slice = Some(payload);
        }

        // RIFF chunks are word-aligned (2-byte padded if odd length).
        pos += effective_size + (chunk_size & 1);
    }

    let spec = spec.ok_or_else(|| AudioError::UnsupportedFormat("WAV missing fmt chunk".into()))?;
    let data =
        data_slice.ok_or_else(|| AudioError::UnsupportedFormat("WAV missing data chunk".into()))?;

    let bytes_per_sample = (spec.bits_per_sample / 8) as usize;
    let frame_stride = bytes_per_sample * spec.channels as usize;
    if frame_stride == 0 {
        return Err(AudioError::UnsupportedFormat(
            "Invalid WAV frame stride".into(),
        ));
    }

    let frame_count = data.len() / frame_stride;
    let mut channels = vec![Vec::with_capacity(frame_count); spec.channels as usize];

    for frame_idx in 0..frame_count {
        let frame_base = frame_idx * frame_stride;
        for ch in 0..spec.channels as usize {
            let offset = frame_base + ch * bytes_per_sample;
            let sample = match (spec.is_float, spec.bits_per_sample) {
                (false, 8) => (data[offset] as f32 - 128.0) / 128.0,
                (false, 16) => {
                    let v = i16::from_le_bytes([data[offset], data[offset + 1]]);
                    v as f32 / 32768.0
                }
                (false, 24) => {
                    // Sign-extend 24-bit little-endian signed integer to i32
                    let b0 = data[offset] as u32;
                    let b1 = (data[offset + 1] as u32) << 8;
                    let b2 = (data[offset + 2] as u32) << 16;
                    let raw = b0 | b1 | b2;
                    let signed = if raw & 0x0080_0000 != 0 {
                        (raw | 0xFF00_0000) as i32
                    } else {
                        raw as i32
                    };
                    signed as f32 / 8_388_608.0
                }
                (false, 32) => {
                    let v = i32::from_le_bytes([
                        data[offset],
                        data[offset + 1],
                        data[offset + 2],
                        data[offset + 3],
                    ]);
                    (v as f64 / 2_147_483_648.0) as f32
                }
                (true, 32) => f32::from_le_bytes([
                    data[offset],
                    data[offset + 1],
                    data[offset + 2],
                    data[offset + 3],
                ]),
                (true, 64) => f64::from_le_bytes([
                    data[offset],
                    data[offset + 1],
                    data[offset + 2],
                    data[offset + 3],
                    data[offset + 4],
                    data[offset + 5],
                    data[offset + 6],
                    data[offset + 7],
                ]) as f32,
                _ => {
                    return Err(AudioError::UnsupportedFormat(format!(
                        "Unsupported WAV bit depth {} (float={})",
                        spec.bits_per_sample, spec.is_float
                    )));
                }
            };
            channels[ch].push(sample);
        }
    }

    Ok(AudioBuffer {
        sample_rate: spec.sample_rate,
        channels,
    })
}

/// Streaming RIFF/WAVE writer for live microphone and mixer recording.
pub struct WavWriter<W: Write + Seek> {
    writer: W,
    channels: u16,
    sample_rate: u32,
    format: WavSampleFormat,
    data_bytes_written: u64,
    header_start: u64,
    finalized: bool,
}

impl<W: Write + Seek> WavWriter<W> {
    /// Creates a new streaming WAV writer and emits a placeholder 44-byte RIFF header.
    pub fn new(
        mut writer: W,
        channels: u16,
        sample_rate: u32,
        format: WavSampleFormat,
    ) -> Result<Self, AudioError> {
        if channels == 0 || sample_rate == 0 {
            return Err(AudioError::UnsupportedFormat(
                "WAV writer requires non-zero channels and sample rate".into(),
            ));
        }

        let header_start = writer
            .stream_position()
            .map_err(|e| AudioError::StreamBuildError(format!("WAV seek failed: {e}")))?;

        let block_align = channels * format.bytes_per_sample();
        let byte_rate = sample_rate * block_align as u32;

        let mut header = [0u8; 44];
        header[0..4].copy_from_slice(b"RIFF");
        header[4..8].copy_from_slice(&36u32.to_le_bytes());
        header[8..12].copy_from_slice(b"WAVE");
        header[12..16].copy_from_slice(b"fmt ");
        header[16..20].copy_from_slice(&16u32.to_le_bytes());
        header[20..22].copy_from_slice(&format.format_tag().to_le_bytes());
        header[22..24].copy_from_slice(&channels.to_le_bytes());
        header[24..28].copy_from_slice(&sample_rate.to_le_bytes());
        header[28..32].copy_from_slice(&byte_rate.to_le_bytes());
        header[32..34].copy_from_slice(&block_align.to_le_bytes());
        header[34..36].copy_from_slice(&format.bits_per_sample().to_le_bytes());
        header[36..40].copy_from_slice(b"data");
        header[40..44].copy_from_slice(&0u32.to_le_bytes());

        writer
            .write_all(&header)
            .map_err(|e| AudioError::StreamPlayError(format!("WAV header write failed: {e}")))?;

        Ok(Self {
            writer,
            channels,
            sample_rate,
            format,
            data_bytes_written: 0,
            header_start,
            finalized: false,
        })
    }

    /// Returns total sample frames written so far.
    pub fn frames_written(&self) -> u64 {
        let stride = self.channels as u64 * self.format.bytes_per_sample() as u64;
        self.data_bytes_written.checked_div(stride).unwrap_or(0)
    }

    /// Returns total recorded duration in seconds.
    pub fn duration_seconds(&self) -> f64 {
        self.frames_written() as f64 / self.sample_rate as f64
    }

    /// Appends interleaved `f32` samples in `[-1.0, 1.0]` to the WAV stream.
    pub fn write_interleaved_f32(&mut self, samples: &[f32]) -> Result<(), AudioError> {
        let mut scratch = Vec::with_capacity(samples.len() * self.format.bytes_per_sample() as usize);
        match self.format {
            WavSampleFormat::Pcm16 => {
                for &s in samples {
                    let clamped = s.clamp(-1.0, 1.0);
                    let q = (clamped * 32767.0).round() as i16;
                    scratch.extend_from_slice(&q.to_le_bytes());
                }
            }
            WavSampleFormat::Pcm24 => {
                for &s in samples {
                    let clamped = s.clamp(-1.0, 1.0);
                    let q = (clamped as f64 * 8_388_607.0).round() as i32;
                    let bytes = q.to_le_bytes();
                    scratch.extend_from_slice(&bytes[0..3]);
                }
            }
            WavSampleFormat::Float32 => {
                for &s in samples {
                    scratch.extend_from_slice(&s.to_le_bytes());
                }
            }
        }

        self.writer
            .write_all(&scratch)
            .map_err(|e| AudioError::StreamPlayError(format!("WAV data write failed: {e}")))?;
        self.data_bytes_written += scratch.len() as u64;
        Ok(())
    }

    /// Back-patches RIFF and `data` chunk sizes and flushes the underlying stream.
    pub fn finalize(&mut self) -> Result<(), AudioError> {
        if self.finalized {
            return Ok(());
        }
        let data_len_u32 = self.data_bytes_written.min(u32::MAX as u64) as u32;
        let riff_len_u32 = data_len_u32.saturating_add(36);

        self.writer
            .seek(SeekFrom::Start(self.header_start + 4))
            .map_err(|e| AudioError::StreamPlayError(format!("WAV seek failed: {e}")))?;
        self.writer
            .write_all(&riff_len_u32.to_le_bytes())
            .map_err(|e| AudioError::StreamPlayError(format!("WAV riff size patch failed: {e}")))?;

        self.writer
            .seek(SeekFrom::Start(self.header_start + 40))
            .map_err(|e| AudioError::StreamPlayError(format!("WAV seek failed: {e}")))?;
        self.writer
            .write_all(&data_len_u32.to_le_bytes())
            .map_err(|e| AudioError::StreamPlayError(format!("WAV data size patch failed: {e}")))?;

        self.writer
            .seek(SeekFrom::End(0))
            .map_err(|e| AudioError::StreamPlayError(format!("WAV seek end failed: {e}")))?;
        self.writer
            .flush()
            .map_err(|e| AudioError::StreamPlayError(format!("WAV flush failed: {e}")))?;
        self.finalized = true;
        Ok(())
    }
}

impl<W: Write + Seek> Drop for WavWriter<W> {
    fn drop(&mut self) {
        let _ = self.finalize();
    }
}
