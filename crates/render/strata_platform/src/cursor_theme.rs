//! System Cursor Theme and Xcursor parser.
//!
//! Loads native cursor themes (defaulting to the host's active GNOME/Yaru cursor pack)
//! and provides rasterization utilities for the emulated MCP cursor.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use crate::window::CursorIcon;

/// Decoded 32-bit ARGB/BGRA cursor bitmap with hotspot coordinates.
#[derive(Debug, Clone)]
pub struct CursorBitmap {
    pub width: u32,
    pub height: u32,
    pub xhot: u32,
    pub yhot: u32,
    pub pixels: Vec<u8>, // 4 bytes per pixel: [B, G, R, A]
}

impl CursorBitmap {
    /// Creates a cursor bitmap from raw pixel buffer with explicit hotspot.
    pub fn new(width: u32, height: u32, xhot: u32, yhot: u32, pixels: Vec<u8>) -> Self {
        Self {
            width,
            height,
            xhot,
            yhot,
            pixels,
        }
    }

    /// Parses an Xcursor binary image from a byte slice.
    pub fn parse_xcursor(data: &[u8], target_size: u32) -> Option<Self> {
        if data.len() < 16 {
            return None;
        }
        let magic = u32::from_le_bytes(data[0..4].try_into().ok()?);
        if magic != 0x72756358 {
            // Magic 'Xcur' in little endian
            return None;
        }

        let ntoc = u32::from_le_bytes(data[12..16].try_into().ok()?);
        let mut best_pos = None;
        let mut best_diff = u32::MAX;

        for i in 0..ntoc as usize {
            let offset = 16 + i * 12;
            if offset + 12 > data.len() {
                break;
            }
            let ctype = u32::from_le_bytes(data[offset..offset + 4].try_into().ok()?);
            let subtype = u32::from_le_bytes(data[offset + 4..offset + 8].try_into().ok()?);
            let pos = u32::from_le_bytes(data[offset + 8..offset + 12].try_into().ok()?);

            if ctype == 0xfffd0002 {
                // Image chunk
                let diff = subtype.abs_diff(target_size);
                if diff < best_diff {
                    best_diff = diff;
                    best_pos = Some(pos as usize);
                    if diff == 0 {
                        break;
                    }
                }
            }
        }

        let pos = best_pos?;
        if pos + 36 > data.len() {
            return None;
        }

        let width = u32::from_le_bytes(data[pos + 16..pos + 20].try_into().ok()?);
        let height = u32::from_le_bytes(data[pos + 20..pos + 24].try_into().ok()?);
        let xhot = u32::from_le_bytes(data[pos + 24..pos + 28].try_into().ok()?);
        let yhot = u32::from_le_bytes(data[pos + 28..pos + 32].try_into().ok()?);

        let pixel_bytes = (width as usize) * (height as usize) * 4;
        let pixel_start = pos + 36;
        if pixel_start + pixel_bytes > data.len() {
            return None;
        }

        let pixels = data[pixel_start..pixel_start + pixel_bytes].to_vec();

        Some(Self {
            width,
            height,
            xhot,
            yhot,
            pixels,
        })
    }

    /// Loads an Xcursor binary file from disk.
    pub fn from_file<P: AsRef<Path>>(path: P, target_size: u32) -> Option<Self> {
        let mut file = File::open(path).ok()?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).ok()?;
        Self::parse_xcursor(&buffer, target_size)
    }
}

/// Active Cursor Theme holding loaded native and fallback cursor shapes.
#[derive(Debug, Clone)]
pub struct CursorTheme {
    pub name: String,
    pub size: u32,
    cursors: HashMap<CursorIcon, CursorBitmap>,
}

impl Default for CursorTheme {
    fn default() -> Self {
        Self::load_active()
    }
}

impl CursorTheme {
    /// Detects the host window manager cursor theme (e.g. Yaru) and loads standard sizes.
    pub fn load_active() -> Self {
        let theme_name = detect_host_cursor_theme().unwrap_or_else(|| "Yaru".to_string());
        Self::load(&theme_name, 24)
    }

    /// Loads a cursor theme by name and size.
    pub fn load(theme_name: &str, size: u32) -> Self {
        let mut cursors = HashMap::new();

        // 1. Try loading from host theme directories
        let search_dirs = [
            PathBuf::from(format!("/usr/share/icons/{}/cursors", theme_name)),
            dirs_icon_theme(theme_name),
            PathBuf::from("/usr/share/icons/Yaru/cursors"),
            PathBuf::from("/usr/share/icons/default/cursors"),
        ];

        let icon_mappings = [
            (CursorIcon::Default, &["default", "arrow", "left_ptr"][..]),
            (CursorIcon::Pointer, &["pointer", "pointing_hand", "hand2", "hand"][..]),
            (CursorIcon::Text, &["text", "xterm", "ibeam"][..]),
            (CursorIcon::Crosshair, &["crosshair", "cross"][..]),
            (CursorIcon::Move, &["move", "pointer-move", "size_all"][..]),
            (CursorIcon::NotAllowed, &["not-allowed", "forbidden", "crossed_circle"][..]),
            (CursorIcon::Grab, &["grab", "openhand"][..]),
            (CursorIcon::Grabbing, &["grabbing", "closedhand"][..]),
            (CursorIcon::Wait, &["wait", "watch"][..]),
        ];

        for (icon, filenames) in icon_mappings {
            let mut loaded = None;
            for dir in &search_dirs {
                if !dir.exists() {
                    continue;
                }
                for fname in filenames {
                    let path = dir.join(fname);
                    if path.exists() {
                        if let Some(bmp) = CursorBitmap::from_file(&path, size) {
                            loaded = Some(bmp);
                            break;
                        }
                    }
                }
                if loaded.is_some() {
                    break;
                }
            }

            // If not found on disk, use embedded authentic Yaru fallback
            let final_bmp = loaded.unwrap_or_else(|| Self::embedded_fallback(icon));
            cursors.insert(icon, final_bmp);
        }

        Self {
            name: theme_name.to_string(),
            size,
            cursors,
        }
    }

    /// Returns the cursor bitmap for a specific icon style.
    pub fn get(&self, icon: CursorIcon) -> &CursorBitmap {
        if let Some(bmp) = self.cursors.get(&icon) {
            bmp
        } else {
            self.cursors.get(&CursorIcon::Default).expect("Default cursor must exist")
        }
    }

    /// Renders the cursor into a Cairo graphics context with opacity and press indicator.
    #[cfg(target_os = "linux")]
    pub fn render_cairo(
        &self,
        cr: &cairo::Context,
        icon: CursorIcon,
        x: f32,
        y: f32,
        opacity: f32,
        is_down: bool,
    ) {
        if opacity <= 0.001 {
            return;
        }

        let bmp = self.get(icon);
        let draw_x = (x - bmp.xhot as f32) as f64;
        let draw_y = (y - bmp.yhot as f32) as f64;

        if let Ok(mut surface) = cairo::ImageSurface::create(
            cairo::Format::ARgb32,
            bmp.width as i32,
            bmp.height as i32,
        ) {
            if let Ok(mut data) = surface.data() {
                if data.len() == bmp.pixels.len() {
                    data.copy_from_slice(&bmp.pixels);
                }
            }

            let _ = cr.save();

            // When mouse is pressed down, draw subtle touch feedback ring
            if is_down {
                cr.set_source_rgba(0.23, 0.51, 0.96, 0.45 * opacity as f64);
                cr.arc(x as f64, y as f64, 7.0, 0.0, 2.0 * std::f64::consts::PI);
                let _ = cr.fill();

                cr.set_source_rgba(1.0, 1.0, 1.0, 0.8 * opacity as f64);
                cr.set_line_width(1.5);
                cr.arc(x as f64, y as f64, 7.0, 0.0, 2.0 * std::f64::consts::PI);
                let _ = cr.stroke();
            }

            let _ = cr.set_source_surface(&surface, draw_x, draw_y);
            if (opacity - 1.0).abs() < 0.001 {
                let _ = cr.paint();
            } else {
                let _ = cr.paint_with_alpha(opacity as f64);
            }

            let _ = cr.restore();
        }
    }

    /// Renders the cursor into a software BGRA32 pixel buffer with alpha blending.
    pub fn render_software(
        &self,
        buffer: &mut [u32],
        buf_width: usize,
        buf_height: usize,
        icon: CursorIcon,
        x: f32,
        y: f32,
        opacity: f32,
        is_down: bool,
    ) {
        if opacity <= 0.001 {
            return;
        }

        let bmp = self.get(icon);
        let start_x = (x as i32) - (bmp.xhot as i32);
        let start_y = (y as i32) - (bmp.yhot as i32);

        // Render touch indicator ring if mouse down
        if is_down {
            let cx = x as i32;
            let cy = y as i32;
            let r_sq = 49; // 7px radius squared
            for dy in -7..=7 {
                for dx in -7..=7 {
                    let d = dx * dx + dy * dy;
                    if d <= r_sq {
                        let px = cx + dx;
                        let py = cy + dy;
                        if px >= 0 && px < buf_width as i32 && py >= 0 && py < buf_height as i32 {
                            let idx = (py as usize) * buf_width + (px as usize);
                            if let Some(target) = buffer.get_mut(idx) {
                                // Blend translucent blue touch highlight
                                let ring_alpha = if d > 25 { 0.8 * opacity } else { 0.45 * opacity };
                                *target = blend_pixel_bgra(*target, 0x3b82f6, ring_alpha);
                            }
                        }
                    }
                }
            }
        }

        for cy in 0..bmp.height as i32 {
            for cx in 0..bmp.width as i32 {
                let target_x = start_x + cx;
                let target_y = start_y + cy;

                if target_x >= 0
                    && target_x < buf_width as i32
                    && target_y >= 0
                    && target_y < buf_height as i32
                {
                    let src_idx = ((cy as usize) * (bmp.width as usize) + (cx as usize)) * 4;
                    let b = bmp.pixels[src_idx] as u32;
                    let g = bmp.pixels[src_idx + 1] as u32;
                    let r = bmp.pixels[src_idx + 2] as u32;
                    let a = bmp.pixels[src_idx + 3] as f32 / 255.0;

                    let final_alpha = (a * opacity).clamp(0.0, 1.0);
                    if final_alpha > 0.001 {
                        let dst_idx = (target_y as usize) * buf_width + (target_x as usize);
                        if let Some(dst) = buffer.get_mut(dst_idx) {
                            let src_color = (r << 16) | (g << 8) | b;
                            *dst = blend_pixel_bgra(*dst, src_color, final_alpha);
                        }
                    }
                }
            }
        }
    }

    /// Authentic embedded Yaru cursor pixel data fallback.
    fn embedded_fallback(icon: CursorIcon) -> CursorBitmap {
        match icon {
            CursorIcon::Pointer => CursorBitmap::new(
                24,
                24,
                8,
                5,
                include_bytes!("../assets/yaru/pointer.bin").to_vec(),
            ),
            CursorIcon::Text => CursorBitmap::new(
                24,
                24,
                11,
                12,
                include_bytes!("../assets/yaru/text.bin").to_vec(),
            ),
            CursorIcon::Crosshair => CursorBitmap::new(
                24,
                24,
                11,
                11,
                include_bytes!("../assets/yaru/crosshair.bin").to_vec(),
            ),
            CursorIcon::NotAllowed => CursorBitmap::new(
                24,
                24,
                12,
                12,
                include_bytes!("../assets/yaru/not-allowed.bin").to_vec(),
            ),
            CursorIcon::Grab => CursorBitmap::new(
                24,
                24,
                11,
                7,
                include_bytes!("../assets/yaru/grab.bin").to_vec(),
            ),
            CursorIcon::Grabbing => CursorBitmap::new(
                24,
                24,
                12,
                11,
                include_bytes!("../assets/yaru/grabbing.bin").to_vec(),
            ),
            _ => CursorBitmap::new(
                24,
                24,
                4,
                4,
                include_bytes!("../assets/yaru/default.bin").to_vec(),
            ),
        }
    }
}

/// Blends a source RGB color onto a destination BGRA/ARGB 32-bit pixel.
#[inline]
fn blend_pixel_bgra(dst: u32, src_rgb: u32, alpha: f32) -> u32 {
    let dst_r = ((dst >> 16) & 0xFF) as f32;
    let dst_g = ((dst >> 8) & 0xFF) as f32;
    let dst_b = (dst & 0xFF) as f32;

    let src_r = ((src_rgb >> 16) & 0xFF) as f32;
    let src_g = ((src_rgb >> 8) & 0xFF) as f32;
    let src_b = (src_rgb & 0xFF) as f32;

    let out_r = (src_r * alpha + dst_r * (1.0 - alpha)).clamp(0.0, 255.0) as u32;
    let out_g = (src_g * alpha + dst_g * (1.0 - alpha)).clamp(0.0, 255.0) as u32;
    let out_b = (src_b * alpha + dst_b * (1.0 - alpha)).clamp(0.0, 255.0) as u32;

    0xFF000000 | (out_r << 16) | (out_g << 8) | out_b
}

fn dirs_icon_theme(theme_name: &str) -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home)
            .join(".icons")
            .join(theme_name)
            .join("cursors")
    } else {
        PathBuf::from("/tmp")
    }
}

fn detect_host_cursor_theme() -> Option<String> {
    if let Ok(theme) = std::env::var("XCURSOR_THEME") {
        if !theme.is_empty() {
            return Some(theme);
        }
    }

    #[cfg(target_os = "linux")]
    {
        // Try reading gsettings if available
        if let Ok(output) = std::process::Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", "cursor-theme"])
            .output()
        {
            if output.status.success() {
                let s = String::from_utf8_lossy(&output.stdout);
                let cleaned = s.trim().trim_matches('\'').trim_matches('"');
                if !cleaned.is_empty() {
                    return Some(cleaned.to_string());
                }
            }
        }
    }

    None
}
