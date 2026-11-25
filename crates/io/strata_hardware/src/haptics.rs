//! Cross-platform unified tactile [`HapticsEngine`] for UI impacts, selection ticks,
//! notification feedback, and custom multi-segment vibration waveforms.

use std::sync::{Arc, Mutex};

/// Single timed impulse segment within a custom haptic waveform.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HapticSegment {
    pub delay_ms: u32,
    pub duration_ms: u32,
    /// Normalized intensity in `[0.0, 1.0]`.
    pub intensity: f32,
    /// Tactile crispness/frequency sharpness in `[0.0, 1.0]` (`0` = low rumble, `1` = crisp tap).
    pub sharpness: f32,
}

/// Standardized tactile feedback patterns mapped cleanly to macOS/iOS/Android/Windows/Linux backends.
#[derive(Debug, Clone, PartialEq)]
pub enum HapticPattern {
    ImpactLight,
    ImpactMedium,
    ImpactHeavy,
    SelectionTick,
    NotificationSuccess,
    NotificationWarning,
    NotificationError,
    CustomWaveform(Vec<HapticSegment>),
}

impl HapticPattern {
    /// Expands any semantic haptic pattern into timed [`HapticSegment`] pulses.
    pub fn to_segments(&self) -> Vec<HapticSegment> {
        match self {
            Self::ImpactLight => vec![HapticSegment {
                delay_ms: 0,
                duration_ms: 15,
                intensity: 0.35,
                sharpness: 0.7,
            }],
            Self::ImpactMedium => vec![HapticSegment {
                delay_ms: 0,
                duration_ms: 25,
                intensity: 0.65,
                sharpness: 0.55,
            }],
            Self::ImpactHeavy => vec![HapticSegment {
                delay_ms: 0,
                duration_ms: 40,
                intensity: 1.0,
                sharpness: 0.35,
            }],
            Self::SelectionTick => vec![HapticSegment {
                delay_ms: 0,
                duration_ms: 8,
                intensity: 0.25,
                sharpness: 0.95,
            }],
            Self::NotificationSuccess => vec![
                HapticSegment {
                    delay_ms: 0,
                    duration_ms: 20,
                    intensity: 0.5,
                    sharpness: 0.8,
                },
                HapticSegment {
                    delay_ms: 55,
                    duration_ms: 30,
                    intensity: 0.85,
                    sharpness: 0.7,
                },
            ],
            Self::NotificationWarning => vec![
                HapticSegment {
                    delay_ms: 0,
                    duration_ms: 35,
                    intensity: 0.75,
                    sharpness: 0.5,
                },
                HapticSegment {
                    delay_ms: 70,
                    duration_ms: 35,
                    intensity: 0.65,
                    sharpness: 0.5,
                },
            ],
            Self::NotificationError => vec![
                HapticSegment {
                    delay_ms: 0,
                    duration_ms: 40,
                    intensity: 0.95,
                    sharpness: 0.4,
                },
                HapticSegment {
                    delay_ms: 60,
                    duration_ms: 40,
                    intensity: 0.95,
                    sharpness: 0.4,
                },
                HapticSegment {
                    delay_ms: 60,
                    duration_ms: 60,
                    intensity: 1.0,
                    sharpness: 0.3,
                },
            ],
            Self::CustomWaveform(segs) => segs.clone(),
        }
    }
}

/// Pluggable backend sink receiving tactile haptic waveforms.
pub trait HapticsSink: Send + Sync {
    fn trigger(&self, pattern: &HapticPattern) -> Result<(), String>;
}

/// Deterministic mock haptics recorder for automated CI tests.
#[derive(Clone, Default)]
pub struct MockHapticsSink {
    history: Arc<Mutex<Vec<HapticPattern>>>,
}

impl MockHapticsSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn triggered_patterns(&self) -> Vec<HapticPattern> {
        self.history.lock().map(|h| h.clone()).unwrap_or_default()
    }

    pub fn clear(&self) {
        if let Ok(mut h) = self.history.lock() {
            h.clear();
        }
    }
}

impl HapticsSink for MockHapticsSink {
    fn trigger(&self, pattern: &HapticPattern) -> Result<(), String> {
        if let Ok(mut h) = self.history.lock() {
            h.push(pattern.clone());
        }
        Ok(())
    }
}

/// Unified tactile haptics engine routing feedback patterns to active OS actuators or custom sinks.
pub struct HapticsEngine {
    enabled: bool,
    sink: Option<Arc<dyn HapticsSink>>,
}

impl Default for HapticsEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl HapticsEngine {
    pub fn new() -> Self {
        Self {
            enabled: true,
            sink: None,
        }
    }

    pub fn with_sink(sink: Arc<dyn HapticsSink>) -> Self {
        Self {
            enabled: true,
            sink: Some(sink),
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Triggers a tactile haptic pattern. Returns `Ok(true)` if dispatched, `Ok(false)` if muted.
    pub fn trigger(&self, pattern: &HapticPattern) -> Result<bool, String> {
        if !self.enabled {
            return Ok(false);
        }
        if let Some(ref sink) = self.sink {
            sink.trigger(pattern)?;
            return Ok(true);
        }

        // Fallback: dispatch primary pulse to first rumble-capable gamepad if attached.
        let gamepads = crate::gamepad::poll_gamepads();
        if let Some(gp) = gamepads.into_iter().find(|g| g.supports_rumble) {
            let segs = pattern.to_segments();
            if let Some(first) = segs.first() {
                let low = first.intensity * (1.0 - first.sharpness * 0.5);
                let high = first.intensity * first.sharpness;
                let _ = crate::gamepad::set_gamepad_rumble(&gp.id, low, high, first.duration_ms);
                return Ok(true);
            }
        }
        Ok(false)
    }
}
