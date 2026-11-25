//! Native OS Text-to-Speech (TTS) dispatcher and pluggable Speech-to-Text (STT) bridge.

use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

/// Voice configuration parameters for speech synthesis.
#[derive(Debug, Clone, PartialEq)]
pub struct SpeechOptions {
    /// Speech rate multiplier (`0.5` = slow, `1.0` = normal, `2.0` = fast).
    pub rate: f32,
    /// Pitch multiplier (`0.5` = low, `1.0` = normal, `2.0` = high).
    pub pitch: f32,
    /// Output volume in `[0.0, 1.0]`.
    pub volume: f32,
    /// Optional OS voice identifier or BCP-47 language code (e.g., `"en-US"`).
    pub voice: Option<String>,
}

impl Default for SpeechOptions {
    fn default() -> Self {
        Self {
            rate: 1.0,
            pitch: 1.0,
            volume: 1.0,
            voice: None,
        }
    }
}

/// Recorded utterance entry stored by [`MockSpeechSynthesizer`].
#[derive(Debug, Clone, PartialEq)]
pub struct SpokenUtterance {
    pub text: String,
    pub options: SpeechOptions,
}

/// Cross-platform native OS Text-to-Speech synthesizer (`spd-say`/`espeak-ng` on Linux,
/// `say` on macOS, SAPI on Windows).
pub struct SpeechSynthesizer {
    active_child: Mutex<Option<Child>>,
}

impl Default for SpeechSynthesizer {
    fn default() -> Self {
        Self::new()
    }
}

impl SpeechSynthesizer {
    pub fn new() -> Self {
        Self {
            active_child: Mutex::new(None),
        }
    }

    /// Speaks `text` asynchronously using the host OS speech synthesizer.
    pub fn speak(&self, text: &str, options: &SpeechOptions) -> Result<(), String> {
        if text.trim().is_empty() {
            return Ok(());
        }
        self.stop();

        #[cfg(target_os = "linux")]
        let child_res = {
            let rate_pct = ((options.rate.clamp(0.5, 2.0) - 1.0) * 100.0).round() as i32;
            let pitch_pct = ((options.pitch.clamp(0.5, 2.0) - 1.0) * 100.0).round() as i32;
            let mut spd = Command::new("spd-say");
            spd.args(["-r", &rate_pct.to_string(), "-p", &pitch_pct.to_string()]);
            if let Some(ref v) = options.voice {
                spd.args(["-l", v]);
            }
            spd.arg(text).stdout(Stdio::null()).stderr(Stdio::null());
            spd.spawn().or_else(|_| {
                let wpm = (175.0 * options.rate.clamp(0.5, 2.5)).round() as u32;
                let mut espeak = Command::new("espeak-ng");
                espeak.args(["-s", &wpm.to_string()]);
                if let Some(ref v) = options.voice {
                    espeak.args(["-v", v]);
                }
                espeak
                    .arg(text)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
            })
        };

        #[cfg(target_os = "macos")]
        let child_res = {
            let wpm = (180.0 * options.rate.clamp(0.5, 2.5)).round() as u32;
            let mut cmd = Command::new("say");
            cmd.args(["-r", &wpm.to_string()]);
            if let Some(ref v) = options.voice {
                cmd.args(["-v", v]);
            }
            cmd.arg(text)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
        };

        #[cfg(windows)]
        let child_res = {
            let sanitized = text.replace('\'', "''");
            let rate_sapi = ((options.rate.clamp(0.5, 2.0) - 1.0) * 5.0).round() as i32;
            let script = format!(
                "Add-Type -AssemblyName System.Speech; $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; $s.Rate = {}; $s.Speak('{}');",
                rate_sapi, sanitized
            );
            Command::new("powershell")
                .args(["-NoProfile", "-Command", &script])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
        };

        #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
        let child_res: Result<Child, std::io::Error> = Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "OS TTS unsupported on this target",
        ));

        let child = child_res.map_err(|e| format!("Failed to launch OS TTS backend: {e}"))?;
        if let Ok(mut guard) = self.active_child.lock() {
            *guard = Some(child);
        }
        Ok(())
    }

    /// Cancels any currently active speech utterance immediately.
    pub fn stop(&self) {
        if let Ok(mut guard) = self.active_child.lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

impl Drop for SpeechSynthesizer {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Deterministic in-memory TTS sink for headless CI & UI tests.
#[derive(Clone, Default)]
pub struct MockSpeechSynthesizer {
    history: Arc<Mutex<Vec<SpokenUtterance>>>,
}

impl MockSpeechSynthesizer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn speak(&self, text: &str, options: &SpeechOptions) {
        if let Ok(mut h) = self.history.lock() {
            h.push(SpokenUtterance {
                text: text.to_string(),
                options: options.clone(),
            });
        }
    }

    pub fn utterances(&self) -> Vec<SpokenUtterance> {
        self.history.lock().map(|h| h.clone()).unwrap_or_default()
    }

    pub fn clear(&self) {
        if let Ok(mut h) = self.history.lock() {
            h.clear();
        }
    }
}

/// Partial or final transcript event produced by a [`SpeechRecognizer`].
#[derive(Debug, Clone, PartialEq)]
pub struct SpeechRecognitionResult {
    pub transcript: String,
    pub is_final: bool,
    pub confidence: f32,
}

/// Pluggable streaming Speech-to-Text (STT) interface accepting denoised PCM frames from
/// [`crate::voice_pipeline::VoicePipeline`].
pub trait SpeechRecognizer: Send {
    fn feed_audio(
        &mut self,
        mono_samples: &[f32],
        sample_rate: u32,
    ) -> Option<SpeechRecognitionResult>;
    fn flush(&mut self) -> Option<SpeechRecognitionResult>;
    fn reset(&mut self);
}

/// Deterministic mock STT recognizer emitting scripted transcripts when voiced energy is fed.
#[derive(Debug, Default)]
pub struct MockSpeechRecognizer {
    scripted: Vec<String>,
    index: usize,
    accumulated_energy: f32,
    frames_seen: usize,
}

impl MockSpeechRecognizer {
    pub fn with_transcripts(transcripts: Vec<String>) -> Self {
        Self {
            scripted: transcripts,
            index: 0,
            accumulated_energy: 0.0,
            frames_seen: 0,
        }
    }
}

impl SpeechRecognizer for MockSpeechRecognizer {
    fn feed_audio(
        &mut self,
        mono_samples: &[f32],
        _sample_rate: u32,
    ) -> Option<SpeechRecognitionResult> {
        if mono_samples.is_empty() {
            return None;
        }
        let rms = (mono_samples.iter().map(|&s| s * s).sum::<f32>() / mono_samples.len() as f32)
            .sqrt();
        self.accumulated_energy += rms;
        self.frames_seen += 1;

        if self.frames_seen >= 2 && self.accumulated_energy > 0.05 && self.index < self.scripted.len()
        {
            let text = self.scripted[self.index].clone();
            self.index += 1;
            self.accumulated_energy = 0.0;
            self.frames_seen = 0;
            return Some(SpeechRecognitionResult {
                transcript: text,
                is_final: true,
                confidence: 0.98,
            });
        }
        None
    }

    fn flush(&mut self) -> Option<SpeechRecognitionResult> {
        if self.index < self.scripted.len() {
            let text = self.scripted[self.index].clone();
            self.index += 1;
            Some(SpeechRecognitionResult {
                transcript: text,
                is_final: true,
                confidence: 0.95,
            })
        } else {
            None
        }
    }

    fn reset(&mut self) {
        self.index = 0;
        self.accumulated_energy = 0.0;
        self.frames_seen = 0;
    }
}
