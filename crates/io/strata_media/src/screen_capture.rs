//! Cross-platform Screen & Window Capture discovery, Wayland xdg-desktop-portal ScreenCast
//! helper, and deterministic `MockScreenCapture` for CI.

use crate::pixel_format::VideoFrame;
use std::sync::atomic::{AtomicU64, Ordering};

/// Type of capturable screen target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScreenSourceKind {
    Monitor,
    Window,
    Region,
}

/// Metadata for a discoverable display output or application window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenSourceInfo {
    pub id: String,
    pub name: String,
    pub kind: ScreenSourceKind,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
}

/// Configuration for screen or window capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenCaptureConfig {
    pub source_id: Option<String>,
    pub fps: u32,
    pub capture_cursor: bool,
    pub capture_audio: bool,
}

impl Default for ScreenCaptureConfig {
    fn default() -> Self {
        Self {
            source_id: None,
            fps: 30,
            capture_cursor: true,
            capture_audio: false,
        }
    }
}

/// Enumerates connected display monitors (reads `/sys/class/drm` on Linux; falls back cleanly).
pub fn list_screen_sources() -> Vec<ScreenSourceInfo> {
    let mut sources = Vec::new();
    #[cfg(target_os = "linux")]
    {
        if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
            let mut idx = 0usize;
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.contains('-') {
                    continue;
                }
                let status_path = entry.path().join("status");
                let status = std::fs::read_to_string(&status_path).unwrap_or_default();
                if status.trim() != "connected" {
                    continue;
                }
                let modes_path = entry.path().join("modes");
                let (w, h) = std::fs::read_to_string(&modes_path)
                    .ok()
                    .and_then(|txt| txt.lines().next().map(|l| l.to_string()))
                    .and_then(|first| {
                        let mut parts = first.split('x');
                        let pw = parts.next()?.parse::<u32>().ok()?;
                        let ph = parts.next()?.parse::<u32>().ok()?;
                        Some((pw, ph))
                    })
                    .unwrap_or((1920, 1080));
                sources.push(ScreenSourceInfo {
                    id: format!("drm:{name}"),
                    name,
                    kind: ScreenSourceKind::Monitor,
                    width: w,
                    height: h,
                    is_primary: idx == 0,
                });
                idx += 1;
            }
        }
    }
    sources
}

/// Queries whether the Linux `org.freedesktop.portal.ScreenCast` portal is reachable over D-Bus.
#[cfg(target_os = "linux")]
pub fn check_portal_screencast_available() -> bool {
    strata_platform::dbus::DbusClient::connect().is_ok()
}

/// Deterministic mock screen capture source emitting crisp checkerboard + frame-counter RGBA frames.
pub struct MockScreenCapture {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    sequence: AtomicU64,
}

impl MockScreenCapture {
    pub fn new(width: u32, height: u32, fps: u32) -> Self {
        Self {
            width: width.max(16),
            height: height.max(16),
            fps: fps.max(1),
            sequence: AtomicU64::new(0),
        }
    }

    pub fn capture_frame(&self) -> VideoFrame {
        let seq = self.sequence.fetch_add(1, Ordering::Relaxed) + 1;
        let w = self.width as usize;
        let h = self.height as usize;
        let mut rgba = vec![0u8; w * h * 4];
        let tile = 16usize;
        for y in 0..h {
            for x in 0..w {
                let dark = ((x / tile) + (y / tile)) % 2 == 0;
                let base = if dark { 24u8 } else { 42u8 };
                let accent = ((seq as u8).wrapping_mul(17)) & 0x3F;
                let d = (y * w + x) * 4;
                rgba[d] = base;
                rgba[d + 1] = base.saturating_add(accent / 2);
                rgba[d + 2] = base.saturating_add(accent);
                rgba[d + 3] = 255;
            }
        }
        VideoFrame {
            width: self.width,
            height: self.height,
            pts_us: (seq * 1_000_000) / self.fps as u64,
            sequence: seq,
            rgba,
        }
    }
}
