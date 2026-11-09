//! Analytical Signed Distance Field (SDF) 2D Rasterizer & Headless Software FrameBuffer.

use std::fs::File;
use std::io::Write;
use std::path::Path;
use crate::compositor::{DisplayList, DrawCommand};
use strata_layout::Rect;
use strata_style::effects::ClipPath;
use strata_style::types::{BlendMode, BoxShadow, Color, ColorStop};

/// Analytical Signed Distance Field for a rounded rectangle.
///
/// Mathematical formula:
/// Given point `p` relative to center of box with half-extents `b` and corner radius `r`:
/// `q = |p| - b + r`
/// `d = length(max(q, 0)) + min(max(q.x, q.y), 0) - r`
#[inline(always)]
pub fn sd_rounded_rect(px: f32, py: f32, half_w: f32, half_h: f32, r: f32) -> f32 {
    let r = r.min(half_w).min(half_h).max(0.0);
    let qx = px.abs() - half_w + r;
    let qy = py.abs() - half_h + r;

    let len = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    let inside = qx.max(qy).min(0.0);
    len + inside - r
}

/// Computes pixel coverage weight [0.0, 1.0] from signed distance.
#[inline(always)]
pub fn sdf_coverage(distance: f32) -> f32 {
    (0.5 - distance).clamp(0.0, 1.0)
}

/// Headless in-memory RGBA pixel canvas for software rasterization and automated testing.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameBuffer {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>, // 4 bytes per pixel: [R, G, B, A]
    pub clip_mask_stack: Vec<Vec<u8>>,
}

impl FrameBuffer {
    /// Creates a new frame buffer initialized to transparent black.
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width * height * 4],
            clip_mask_stack: Vec::new(),
        }
    }

    /// Clears the entire buffer with a solid color.
    pub fn clear(&mut self, color: Color) {
        let (r, g, b, a) = color.to_rgba8();
        for chunk in self.pixels.chunks_exact_mut(4) {
            chunk[0] = r;
            chunk[1] = g;
            chunk[2] = b;
            chunk[3] = a;
        }
    }

    /// Clears an axis-aligned rectangular region with a solid color.
    pub fn clear_rect(&mut self, rect: Rect, color: Color) {
        let (min_x, max_x, min_y, max_y) = self.compute_clamped_bounds(rect, None);
        let (r, g, b, a) = color.to_rgba8();
        for y in min_y..max_y {
            let row_start = (y * self.width + min_x) * 4;
            let row_end = (y * self.width + max_x) * 4;
            for chunk in self.pixels[row_start..row_end].chunks_exact_mut(4) {
                chunk[0] = r;
                chunk[1] = g;
                chunk[2] = b;
                chunk[3] = a;
            }
        }
    }

    /// Reads a pixel at (x, y).
    #[inline]
    pub fn get_pixel(&self, x: usize, y: usize) -> Color {
        if x >= self.width || y >= self.height {
            return Color::TRANSPARENT;
        }
        let idx = (y * self.width + x) * 4;
        Color::rgba(
            self.pixels[idx],
            self.pixels[idx + 1],
            self.pixels[idx + 2],
            self.pixels[idx + 3],
        )
    }

    /// Safe zero-copy byte slice view of raw RGBA pixel data.
    #[inline]
    pub fn as_raw_bytes(&self) -> &[u8] {
        &self.pixels
    }

    /// Writes a pixel at (x, y) with standard alpha blending (Porter-Duff Over).
    #[inline]
    pub fn set_pixel(&mut self, x: usize, y: usize, mut src: Color) {
        if x >= self.width || y >= self.height {
            return;
        }
        if let Some(mask) = self.clip_mask_stack.last() {
            let mask_val = mask[y * self.width + x];
            if mask_val == 0 {
                return;
            }
            if mask_val < 255 {
                let cov = mask_val as f32 / 255.0;
                src = Color::from_f32(src.r(), src.g(), src.b(), src.a() * cov);
            }
        }
        let src_a = src.a();
        if src_a <= 0.001 {
            return;
        }

        let idx = (y * self.width + x) * 4;
        let dst_r = self.pixels[idx] as f32 / 255.0;
        let dst_g = self.pixels[idx + 1] as f32 / 255.0;
        let dst_b = self.pixels[idx + 2] as f32 / 255.0;
        let dst_a = self.pixels[idx + 3] as f32 / 255.0;

        let out_a = src_a + dst_a * (1.0 - src_a);
        if out_a > 1e-5 {
            let out_r = (src.r() * src_a + dst_r * dst_a * (1.0 - src_a)) / out_a;
            let out_g = (src.g() * src_a + dst_g * dst_a * (1.0 - src_a)) / out_a;
            let out_b = (src.b() * src_a + dst_b * dst_a * (1.0 - src_a)) / out_a;

            self.pixels[idx] = (out_r * 255.0).round() as u8;
            self.pixels[idx + 1] = (out_g * 255.0).round() as u8;
            self.pixels[idx + 2] = (out_b * 255.0).round() as u8;
            self.pixels[idx + 3] = (out_a * 255.0).round() as u8;
        }
    }

    /// Renders an axis-aligned solid rectangle.
    pub fn draw_rect(&mut self, bounds: Rect, color: Color, clip: Option<Rect>) {
        let (min_x, max_x, min_y, max_y) = self.compute_clamped_bounds(bounds, clip);

        for y in min_y..max_y {
            for x in min_x..max_x {
                self.set_pixel(x, y, color);
            }
        }
    }

    /// Renders an anti-aliased rounded rectangle via Signed Distance Fields.
    pub fn draw_rounded_rect(
        &mut self,
        bounds: Rect,
        color: Color,
        radius: f32,
        border_width: f32,
        border_color: Option<Color>,
        clip: Option<Rect>,
    ) {
        let (min_x, max_x, min_y, max_y) = self.compute_clamped_bounds(bounds, clip);
        let center_x = bounds.x + bounds.width * 0.5;
        let center_y = bounds.y + bounds.height * 0.5;
        let half_w = bounds.width * 0.5;
        let half_h = bounds.height * 0.5;

        for y in min_y..max_y {
            let py = y as f32 + 0.5 - center_y;
            for x in min_x..max_x {
                let px = x as f32 + 0.5 - center_x;

                let d = sd_rounded_rect(px, py, half_w, half_h, radius);
                let alpha = sdf_coverage(d);

                if alpha > 0.0 {
                    if let Some(b_color) = border_color.filter(|_| border_width > 0.0) {
                        let inner_d = d + border_width;
                        let inner_alpha = sdf_coverage(inner_d);

                        // Blend border with inner fill
                        let effective_color = Color::rgba(
                            (b_color.r() * 255.0) as u8,
                            (b_color.g() * 255.0) as u8,
                            (b_color.b() * 255.0) as u8,
                            (b_color.a() * alpha * (1.0 - inner_alpha) * 255.0) as u8,
                        );
                        self.set_pixel(x, y, effective_color);

                        let fill_color = Color::rgba(
                            (color.r() * 255.0) as u8,
                            (color.g() * 255.0) as u8,
                            (color.b() * 255.0) as u8,
                            (color.a() * inner_alpha * 255.0) as u8,
                        );
                        self.set_pixel(x, y, fill_color);
                    } else {
                        let blended = Color::rgba(
                            (color.r() * 255.0) as u8,
                            (color.g() * 255.0) as u8,
                            (color.b() * 255.0) as u8,
                            (color.a() * alpha * 255.0) as u8,
                        );
                        self.set_pixel(x, y, blended);
                    }
                }
            }
        }
    }

    /// Renders a CSS box-shadow using smoothed SDF drop shadow approximation.
    pub fn draw_shadow(&mut self, bounds: Rect, shadow: &BoxShadow, clip: Option<Rect>) {
        if shadow.color.a() <= 0.001 {
            return;
        }

        let shadow_bounds = Rect::new(
            bounds.x + shadow.offset_x - shadow.spread_radius - shadow.blur_radius,
            bounds.y + shadow.offset_y - shadow.spread_radius - shadow.blur_radius,
            bounds.width + (shadow.spread_radius + shadow.blur_radius) * 2.0,
            bounds.height + (shadow.spread_radius + shadow.blur_radius) * 2.0,
        );

        let (min_x, max_x, min_y, max_y) = self.compute_clamped_bounds(shadow_bounds, clip);
        let center_x = bounds.x + shadow.offset_x + bounds.width * 0.5;
        let center_y = bounds.y + shadow.offset_y + bounds.height * 0.5;
        let half_w = bounds.width * 0.5 + shadow.spread_radius;
        let half_h = bounds.height * 0.5 + shadow.spread_radius;

        let blur = shadow.blur_radius.max(1.0);

        for y in min_y..max_y {
            let py = y as f32 + 0.5 - center_y;
            for x in min_x..max_x {
                let px = x as f32 + 0.5 - center_x;

                let d = sd_rounded_rect(px, py, half_w, half_h, 0.0);
                // Gaussian blur roll-off approximation:
                let alpha = (-d / blur).exp().clamp(0.0, 1.0) * shadow.color.a();

                if alpha > 0.005 {
                    let c = Color::rgba(
                        (shadow.color.r() * 255.0) as u8,
                        (shadow.color.g() * 255.0) as u8,
                        (shadow.color.b() * 255.0) as u8,
                        (alpha * 255.0) as u8,
                    );
                    self.set_pixel(x, y, c);
                }
            }
        }
    }

    /// Renders text using the embedded 8x16 bitmap font.
    pub fn draw_text(
        &mut self,
        origin: (f32, f32),
        text: &str,
        font_size: f32,
        color: Color,
        clip: Option<Rect>,
    ) {
        if color.a() <= 0.001 {
            return;
        }

        let scale = (font_size / 16.0).max(0.65);
        let char_w = 8.0 * scale;
        let char_h = 16.0 * scale;

        let mut cursor_x = origin.0;
        let mut cursor_y = origin.1;

        for ch in text.chars() {
            if ch == '\n' {
                cursor_y += char_h * 1.25;
                cursor_x = origin.0;
                continue;
            }

            let glyph = crate::font::get_glyph_or_fallback(ch);
            if (scale - 1.0).abs() < 1e-4 && cursor_x.fract().abs() < 1e-4 && cursor_y.fract().abs() < 1e-4 {
                let base_x = cursor_x.round() as usize;
                let base_y = cursor_y.round() as usize;
                for (row_idx, &row) in glyph.iter().enumerate() {
                    if row == 0 {
                        continue;
                    }
                    let y = base_y + row_idx;
                    if y >= self.height {
                        continue;
                    }
                    for col_idx in 0..8 {
                        if (row & (1 << (7 - col_idx))) != 0 {
                            let x = base_x + col_idx;
                            if x >= self.width {
                                continue;
                            }
                            if let Some(c) = clip {
                                if !c.contains_point(x as f32 + 0.5, y as f32 + 0.5) {
                                    continue;
                                }
                            }
                            self.set_pixel(x, y, color);
                        }
                    }
                }
            } else {
                let min_x = (cursor_x.floor() as isize).max(0).min(self.width as isize) as usize;
                let max_x = ((cursor_x + char_w).ceil() as isize).max(0).min(self.width as isize) as usize;
                let min_y = (cursor_y.floor() as isize).max(0).min(self.height as isize) as usize;
                let max_y = ((cursor_y + char_h).ceil() as isize).max(0).min(self.height as isize) as usize;

                for y in min_y..max_y {
                    let py = y as f32 + 0.5;
                    let gy = (py - cursor_y) / scale;
                    if gy < 0.0 || gy >= 16.0 {
                        continue;
                    }
                    let row_f = gy.clamp(0.0, 15.99);
                    let r0 = row_f.floor() as usize;
                    let r1 = (r0 + 1).min(15);
                    let fr = row_f - r0 as f32;

                    for x in min_x..max_x {
                        let px = x as f32 + 0.5;
                        let gx = (px - cursor_x) / scale;
                        if gx < 0.0 || gx >= 8.0 {
                            continue;
                        }
                        if let Some(c) = clip {
                            if !c.contains_point(px, py) {
                                continue;
                            }
                        }

                        let col_f = gx.clamp(0.0, 7.99);
                        let c0 = col_f.floor() as usize;
                        let c1 = (c0 + 1).min(7);
                        let fc = col_f - c0 as f32;

                        let b00 = if (glyph[r0] & (1 << (7 - c0))) != 0 { 1.0f32 } else { 0.0f32 };
                        let b10 = if (glyph[r0] & (1 << (7 - c1))) != 0 { 1.0f32 } else { 0.0f32 };
                        let b01 = if (glyph[r1] & (1 << (7 - c0))) != 0 { 1.0f32 } else { 0.0f32 };
                        let b11 = if (glyph[r1] & (1 << (7 - c1))) != 0 { 1.0f32 } else { 0.0f32 };

                        let top = b00 * (1.0 - fc) + b10 * fc;
                        let bot = b01 * (1.0 - fc) + b11 * fc;
                        let cov = top * (1.0 - fr) + bot * fr;

                        if cov > 0.01 {
                            let mut blended = color;
                            let out_a = blended.a() * cov;
                            blended = Color::from_f32(blended.r(), blended.g(), blended.b(), out_a);
                            self.set_pixel(x, y, blended);
                        }
                    }
                }
            }
            cursor_x += char_w;
        }
    }

    /// Applies CSS filter operations to the specified rectangle in the FrameBuffer.
    pub fn apply_filter(&mut self, bounds: Rect, filter: &strata_style::effects::BackdropFilter) {
        if filter.is_empty() {
            return;
        }
        let (min_x, max_x, min_y, max_y) = self.compute_clamped_bounds(bounds, None);
        if min_x >= max_x || min_y >= max_y {
            return;
        }

        for op in &filter.operations {
            match *op {
                strata_style::effects::FilterOp::Brightness(factor) => {
                    for y in min_y..max_y {
                        let row_start = (y * self.width + min_x) * 4;
                        let row_end = (y * self.width + max_x) * 4;
                        for chunk in self.pixels[row_start..row_end].chunks_exact_mut(4) {
                            chunk[0] = ((chunk[0] as f32 * factor).round() as u32).min(255) as u8;
                            chunk[1] = ((chunk[1] as f32 * factor).round() as u32).min(255) as u8;
                            chunk[2] = ((chunk[2] as f32 * factor).round() as u32).min(255) as u8;
                        }
                    }
                }
                strata_style::effects::FilterOp::Contrast(factor) => {
                    for y in min_y..max_y {
                        let row_start = (y * self.width + min_x) * 4;
                        let row_end = (y * self.width + max_x) * 4;
                        for chunk in self.pixels[row_start..row_end].chunks_exact_mut(4) {
                            for ch in &mut chunk[..3] {
                                let norm = *ch as f32 / 255.0;
                                let adjusted = ((norm - 0.5) * factor + 0.5).clamp(0.0, 1.0);
                                *ch = (adjusted * 255.0).round() as u8;
                            }
                        }
                    }
                }
                strata_style::effects::FilterOp::Grayscale(amt) => {
                    let amt = amt.clamp(0.0, 1.0);
                    let inv = 1.0 - amt;
                    for y in min_y..max_y {
                        let row_start = (y * self.width + min_x) * 4;
                        let row_end = (y * self.width + max_x) * 4;
                        for chunk in self.pixels[row_start..row_end].chunks_exact_mut(4) {
                            let r = chunk[0] as f32;
                            let g = chunk[1] as f32;
                            let b = chunk[2] as f32;
                            let gray = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                            chunk[0] = (r * inv + gray * amt).round() as u8;
                            chunk[1] = (g * inv + gray * amt).round() as u8;
                            chunk[2] = (b * inv + gray * amt).round() as u8;
                        }
                    }
                }
                strata_style::effects::FilterOp::Invert(amt) => {
                    let amt = amt.clamp(0.0, 1.0);
                    let inv = 1.0 - amt;
                    for y in min_y..max_y {
                        let row_start = (y * self.width + min_x) * 4;
                        let row_end = (y * self.width + max_x) * 4;
                        for chunk in self.pixels[row_start..row_end].chunks_exact_mut(4) {
                            chunk[0] = (chunk[0] as f32 * inv + (255 - chunk[0]) as f32 * amt)
                                .round() as u8;
                            chunk[1] = (chunk[1] as f32 * inv + (255 - chunk[1]) as f32 * amt)
                                .round() as u8;
                            chunk[2] = (chunk[2] as f32 * inv + (255 - chunk[2]) as f32 * amt)
                                .round() as u8;
                        }
                    }
                }
                strata_style::effects::FilterOp::Sepia(amt) => {
                    let amt = amt.clamp(0.0, 1.0);
                    let inv = 1.0 - amt;
                    for y in min_y..max_y {
                        let row_start = (y * self.width + min_x) * 4;
                        let row_end = (y * self.width + max_x) * 4;
                        for chunk in self.pixels[row_start..row_end].chunks_exact_mut(4) {
                            let r = chunk[0] as f32;
                            let g = chunk[1] as f32;
                            let b = chunk[2] as f32;
                            let sr = (0.393 * r + 0.769 * g + 0.189 * b).min(255.0);
                            let sg = (0.349 * r + 0.686 * g + 0.168 * b).min(255.0);
                            let sb = (0.272 * r + 0.534 * g + 0.131 * b).min(255.0);
                            chunk[0] = (r * inv + sr * amt).round() as u8;
                            chunk[1] = (g * inv + sg * amt).round() as u8;
                            chunk[2] = (b * inv + sb * amt).round() as u8;
                        }
                    }
                }
                strata_style::effects::FilterOp::Opacity(amt) => {
                    let amt = amt.clamp(0.0, 1.0);
                    for y in min_y..max_y {
                        let row_start = (y * self.width + min_x) * 4;
                        let row_end = (y * self.width + max_x) * 4;
                        for chunk in self.pixels[row_start..row_end].chunks_exact_mut(4) {
                            chunk[3] = (chunk[3] as f32 * amt).round() as u8;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Renders an anti-aliased circle.
    pub fn draw_circle(
        &mut self,
        cx: f32,
        cy: f32,
        radius: f32,
        color: Color,
        clip: Option<Rect>,
    ) {
        if radius <= 0.0 || color.a() <= 0.001 {
            return;
        }
        let bounds = Rect::new(cx - radius, cy - radius, radius * 2.0, radius * 2.0);
        let (min_x, max_x, min_y, max_y) = self.compute_clamped_bounds(bounds, clip);

        for y in min_y..max_y {
            let py = y as f32 + 0.5 - cy;
            for x in min_x..max_x {
                let px = x as f32 + 0.5 - cx;
                let d = (px * px + py * py).sqrt() - radius;
                let cov = sdf_coverage(d);
                if cov > 0.001 {
                    let mut blended = color;
                    let out_a = blended.a() * cov;
                    blended = Color::from_f32(blended.r(), blended.g(), blended.b(), out_a);
                    self.set_pixel(x, y, blended);
                }
            }
        }
    }

    /// Renders a multi-stop linear gradient with analytical corner clipping.
    pub fn draw_linear_gradient(
        &mut self,
        bounds: Rect,
        angle_deg: f32,
        stops: &[ColorStop],
        radius: f32,
        clip: Option<Rect>,
    ) {
        if stops.is_empty() || bounds.width <= 0.0 || bounds.height <= 0.0 {
            return;
        }
        let norm_stops = normalize_color_stops(stops);
        let (min_x, max_x, min_y, max_y) = self.compute_clamped_bounds(bounds, clip);
        let center_x = bounds.x + bounds.width * 0.5;
        let center_y = bounds.y + bounds.height * 0.5;
        let half_w = bounds.width * 0.5;
        let half_h = bounds.height * 0.5;

        let rad = angle_deg.to_radians();
        let dx = rad.sin();
        let dy = -rad.cos();
        let length = (bounds.width * dx.abs() + bounds.height * dy.abs()).max(1e-4);

        for y in min_y..max_y {
            let py = y as f32 + 0.5 - center_y;
            for x in min_x..max_x {
                let px = x as f32 + 0.5 - center_x;

                let d = sd_rounded_rect(px, py, half_w, half_h, radius);
                let cov = sdf_coverage(d);
                if cov <= 0.001 {
                    continue;
                }

                let proj = px * dx + py * dy;
                let t = (proj + length * 0.5) / length;
                let col = sample_gradient(&norm_stops, t);

                if cov >= 0.999 {
                    self.set_pixel(x, y, col);
                } else {
                    let mut blended = col;
                    let out_a = blended.a() * cov;
                    blended = Color::from_f32(blended.r(), blended.g(), blended.b(), out_a);
                    self.set_pixel(x, y, blended);
                }
            }
        }
    }

    /// Renders a multi-stop radial gradient with analytical corner clipping.
    pub fn draw_radial_gradient(
        &mut self,
        bounds: Rect,
        center: (f32, f32),
        stops: &[ColorStop],
        radius: f32,
        clip: Option<Rect>,
    ) {
        if stops.is_empty() || bounds.width <= 0.0 || bounds.height <= 0.0 {
            return;
        }
        let norm_stops = normalize_color_stops(stops);
        let (min_x, max_x, min_y, max_y) = self.compute_clamped_bounds(bounds, clip);
        let center_x = bounds.x + bounds.width * 0.5;
        let center_y = bounds.y + bounds.height * 0.5;
        let half_w = bounds.width * 0.5;
        let half_h = bounds.height * 0.5;

        let focal_x = if center.0 <= 1.0 { bounds.x + bounds.width * center.0 } else { center.0 };
        let focal_y = if center.1 <= 1.0 { bounds.y + bounds.height * center.1 } else { center.1 };
        let rx = half_w.max(1e-4);
        let ry = half_h.max(1e-4);

        for y in min_y..max_y {
            let py = y as f32 + 0.5 - center_y;
            let actual_y = y as f32 + 0.5;
            for x in min_x..max_x {
                let px = x as f32 + 0.5 - center_x;
                let actual_x = x as f32 + 0.5;

                let d = sd_rounded_rect(px, py, half_w, half_h, radius);
                let cov = sdf_coverage(d);
                if cov <= 0.001 {
                    continue;
                }

                let ndx = (actual_x - focal_x) / rx;
                let ndy = (actual_y - focal_y) / ry;
                let t = (ndx * ndx + ndy * ndy).sqrt();
                let col = sample_gradient(&norm_stops, t);

                if cov >= 0.999 {
                    self.set_pixel(x, y, col);
                } else {
                    let mut blended = col;
                    let out_a = blended.a() * cov;
                    blended = Color::from_f32(blended.r(), blended.g(), blended.b(), out_a);
                    self.set_pixel(x, y, blended);
                }
            }
        }
    }

    /// Renders an image with bilinear scaling and alpha blending.
    pub fn draw_image(
        &mut self,
        bounds: Rect,
        image: &strata_layout::ImageResource,
        clip: Option<Rect>,
    ) {
        if bounds.width <= 0.0 || bounds.height <= 0.0 || image.width == 0 || image.height == 0 {
            return;
        }

        let raw_pixels = decode_or_get_rgba(&image.data, image.width as usize, image.height as usize);
        let src_w = image.width as usize;
        let src_h = image.height as usize;
        if raw_pixels.len() < src_w * src_h * 4 {
            return;
        }

        let (min_x, max_x, min_y, max_y) = self.compute_clamped_bounds(bounds, clip);

        for y in min_y..max_y {
            let v = ((y as f32 + 0.5 - bounds.y) / bounds.height * src_h as f32 - 0.5)
                .clamp(0.0, (src_h - 1) as f32);
            let y_low = v.floor() as usize;
            let y_high = (y_low + 1).min(src_h - 1);
            let fy = v - v.floor();

            for x in min_x..max_x {
                let u = ((x as f32 + 0.5 - bounds.x) / bounds.width * src_w as f32 - 0.5)
                    .clamp(0.0, (src_w - 1) as f32);
                let x_low = u.floor() as usize;
                let x_high = (x_low + 1).min(src_w - 1);
                let fx = u - u.floor();

                let idx_tl = (y_low * src_w + x_low) * 4;
                let idx_tr = (y_low * src_w + x_high) * 4;
                let idx_bl = (y_high * src_w + x_low) * 4;
                let idx_br = (y_high * src_w + x_high) * 4;

                let r_t = (raw_pixels[idx_tl] as f32 * (1.0 - fx) + raw_pixels[idx_tr] as f32 * fx) / 255.0;
                let g_t = (raw_pixels[idx_tl + 1] as f32 * (1.0 - fx) + raw_pixels[idx_tr + 1] as f32 * fx) / 255.0;
                let b_t = (raw_pixels[idx_tl + 2] as f32 * (1.0 - fx) + raw_pixels[idx_tr + 2] as f32 * fx) / 255.0;
                let a_t = (raw_pixels[idx_tl + 3] as f32 * (1.0 - fx) + raw_pixels[idx_tr + 3] as f32 * fx) / 255.0;

                let r_b = (raw_pixels[idx_bl] as f32 * (1.0 - fx) + raw_pixels[idx_br] as f32 * fx) / 255.0;
                let g_b = (raw_pixels[idx_bl + 1] as f32 * (1.0 - fx) + raw_pixels[idx_br + 1] as f32 * fx) / 255.0;
                let b_b = (raw_pixels[idx_bl + 2] as f32 * (1.0 - fx) + raw_pixels[idx_br + 2] as f32 * fx) / 255.0;
                let a_b = (raw_pixels[idx_bl + 3] as f32 * (1.0 - fx) + raw_pixels[idx_br + 3] as f32 * fx) / 255.0;

                let r = r_t * (1.0 - fy) + r_b * fy;
                let g = g_t * (1.0 - fy) + g_b * fy;
                let b = b_t * (1.0 - fy) + b_b * fy;
                let a = a_t * (1.0 - fy) + a_b * fy;

                self.set_pixel(x, y, Color::from_f32(r, g, b, a));
            }
        }
    }

    /// Renders an SVG vector path with fill and stroke.
    pub fn draw_svg_path(
        &mut self,
        bounds: Rect,
        path_data: &str,
        stroke_color: Option<Color>,
        fill_color: Option<Color>,
        stroke_width: f32,
        stroke_linecap: strata_layout::StrokeLineCap,
        stroke_linejoin: strata_layout::StrokeLineJoin,
        _stroke_dasharray: &[f32],
        _fill_rule: strata_layout::FillRule,
        view_box: (f32, f32),
        clip: Option<Rect>,
    ) {
        let path = strata_gpu_path::PathBuilder::from_svg(path_data);
        let scale_x = if view_box.0 > 0.0 { bounds.width / view_box.0 } else { 1.0 };
        let scale_y = if view_box.1 > 0.0 { bounds.height / view_box.1 } else { 1.0 };

        let quads = path.to_quadratics(0.25);
        if quads.is_empty() {
            return;
        }

        let transformed_quads: Vec<strata_gpu_path::QuadraticBezier> = quads.into_iter().map(|q| {
            strata_gpu_path::QuadraticBezier::new(
                strata_gpu_path::Point2D::new(bounds.x + q.p0.x * scale_x, bounds.y + q.p0.y * scale_y),
                strata_gpu_path::Point2D::new(bounds.x + q.p1.x * scale_x, bounds.y + q.p1.y * scale_y),
                strata_gpu_path::Point2D::new(bounds.x + q.p2.x * scale_x, bounds.y + q.p2.y * scale_y),
            )
        }).collect();

        if let Some(fc) = fill_color {
            if fc.a() > 0.001 {
                self.rasterize_curves_fill(&transformed_quads, bounds, fc, clip);
            }
        }

        if let Some(sc) = stroke_color {
            if sc.a() > 0.001 && stroke_width > 0.0 {
                let cap = match stroke_linecap {
                    strata_layout::StrokeLineCap::Round => strata_gpu_path::LineCap::Round,
                    strata_layout::StrokeLineCap::Square => strata_gpu_path::LineCap::Square,
                    _ => strata_gpu_path::LineCap::Butt,
                };
                let join = match stroke_linejoin {
                    strata_layout::StrokeLineJoin::Round => strata_gpu_path::LineJoin::Round,
                    strata_layout::StrokeLineJoin::Bevel => strata_gpu_path::LineJoin::Bevel,
                    _ => strata_gpu_path::LineJoin::Miter,
                };
                let stroke_opts = strata_gpu_path::StrokeOptions {
                    width: stroke_width * (scale_x + scale_y) * 0.5,
                    miter_limit: 4.0,
                    cap,
                    join,
                };
                let stroked_quads = strata_gpu_path::expand_stroke_quadratics(&transformed_quads, &stroke_opts);
                let inflated = Rect::new(
                    bounds.x - stroke_width,
                    bounds.y - stroke_width,
                    bounds.width + stroke_width * 2.0,
                    bounds.height + stroke_width * 2.0,
                );
                self.rasterize_curves_fill(&stroked_quads, inflated, sc, clip);
            }
        }
    }

    /// Renders a GPU vector path with fill and stroke.
    pub fn draw_gpu_path(
        &mut self,
        bounds: Rect,
        path: &strata_gpu_path::Path,
        stroke: Option<strata_gpu_path::StrokeOptions>,
        fill_color: Option<Color>,
        stroke_color: Option<Color>,
        transform: Option<strata_primitives::geom::Matrix4x4>,
        clip: Option<Rect>,
    ) {
        let quads = path.to_quadratics(0.25);
        if quads.is_empty() {
            return;
        }

        let transformed_quads: Vec<strata_gpu_path::QuadraticBezier> = quads.into_iter().map(|q| {
            let p0 = if let Some(ref m) = transform {
                let (tx, ty) = m.transform_point(q.p0.x, q.p0.y);
                strata_gpu_path::Point2D::new(bounds.x + tx, bounds.y + ty)
            } else {
                strata_gpu_path::Point2D::new(bounds.x + q.p0.x, bounds.y + q.p0.y)
            };
            let p1 = if let Some(ref m) = transform {
                let (tx, ty) = m.transform_point(q.p1.x, q.p1.y);
                strata_gpu_path::Point2D::new(bounds.x + tx, bounds.y + ty)
            } else {
                strata_gpu_path::Point2D::new(bounds.x + q.p1.x, bounds.y + q.p1.y)
            };
            let p2 = if let Some(ref m) = transform {
                let (tx, ty) = m.transform_point(q.p2.x, q.p2.y);
                strata_gpu_path::Point2D::new(bounds.x + tx, bounds.y + ty)
            } else {
                strata_gpu_path::Point2D::new(bounds.x + q.p2.x, bounds.y + q.p2.y)
            };
            strata_gpu_path::QuadraticBezier::new(p0, p1, p2)
        }).collect();

        if let Some(fc) = fill_color {
            if fc.a() > 0.001 {
                self.rasterize_curves_fill(&transformed_quads, bounds, fc, clip);
            }
        }

        if let Some(sc) = stroke_color {
            if sc.a() > 0.001 {
                let opts = stroke.unwrap_or_else(|| strata_gpu_path::StrokeOptions::new(1.0));
                let stroked_quads = strata_gpu_path::expand_stroke_quadratics(&transformed_quads, &opts);
                let inflated = Rect::new(
                    bounds.x - opts.width,
                    bounds.y - opts.width,
                    bounds.width + opts.width * 2.0,
                    bounds.height + opts.width * 2.0,
                );
                self.rasterize_curves_fill(&stroked_quads, inflated, sc, clip);
            }
        }
    }

    fn rasterize_curves_fill(
        &mut self,
        curves: &[strata_gpu_path::QuadraticBezier],
        bounds: Rect,
        color: Color,
        clip: Option<Rect>,
    ) {
        if curves.is_empty() || color.a() <= 0.001 {
            return;
        }

        let mut min_x_f = bounds.x;
        let mut min_y_f = bounds.y;
        let mut max_x_f = bounds.max_x();
        let mut max_y_f = bounds.max_y();

        for q in curves {
            let bb = q.bounding_box();
            min_x_f = min_x_f.min(bb.min_x);
            min_y_f = min_y_f.min(bb.min_y);
            max_x_f = max_x_f.max(bb.max_x);
            max_y_f = max_y_f.max(bb.max_y);
        }

        let effective_bounds = Rect::new(min_x_f, min_y_f, max_x_f - min_x_f, max_y_f - min_y_f);
        let (min_x, max_x, min_y, max_y) = self.compute_clamped_bounds(effective_bounds, clip);

        for y in min_y..max_y {
            let py = y as f32;
            for x in min_x..max_x {
                let px = x as f32;
                let mut hits = 0;
                let sub_offsets = [
                    (0.25, 0.25),
                    (0.75, 0.25),
                    (0.25, 0.75),
                    (0.75, 0.75),
                ];
                for (ox, oy) in sub_offsets {
                    let pt = strata_gpu_path::Point2D::new(px + ox, py + oy);
                    if strata_gpu_path::is_point_inside_closed_path(pt, curves) {
                        hits += 1;
                    }
                }
                if hits > 0 {
                    let cov = hits as f32 / 4.0;
                    let mut blended = color;
                    let out_a = blended.a() * cov;
                    blended = Color::from_f32(blended.r(), blended.g(), blended.b(), out_a);
                    self.set_pixel(x, y, blended);
                }
            }
        }
    }

    /// Pushes an arbitrary geometric clip mask.
    pub fn push_clip_path(&mut self, bounds: Rect, clip_path: &ClipPath) {
        let mut mask = vec![0u8; self.width * self.height];

        match clip_path {
            ClipPath::None => {
                mask.fill(255);
            }
            ClipPath::Circle { radius, center_x, center_y } => {
                let cx = bounds.x + center_x;
                let cy = bounds.y + center_y;
                let r = *radius;
                for y in 0..self.height {
                    let py = y as f32 + 0.5 - cy;
                    for x in 0..self.width {
                        let px = x as f32 + 0.5 - cx;
                        let d = (px * px + py * py).sqrt() - r;
                        let cov = sdf_coverage(d);
                        mask[y * self.width + x] = (cov * 255.0).round() as u8;
                    }
                }
            }
            ClipPath::Ellipse { radius_x, radius_y, center_x, center_y } => {
                let cx = bounds.x + center_x;
                let cy = bounds.y + center_y;
                let rx = (*radius_x).max(1e-4);
                let ry = (*radius_y).max(1e-4);
                for y in 0..self.height {
                    let py = (y as f32 + 0.5 - cy) / ry;
                    for x in 0..self.width {
                        let px = (x as f32 + 0.5 - cx) / rx;
                        let d = (px * px + py * py).sqrt() - 1.0;
                        let cov = (0.5 - d * rx.min(ry)).clamp(0.0, 1.0);
                        mask[y * self.width + x] = (cov * 255.0).round() as u8;
                    }
                }
            }
            ClipPath::Inset { top, right, bottom, left, border_radius } => {
                let ix = bounds.x + left;
                let iy = bounds.y + top;
                let iw = (bounds.width - left - right).max(0.0);
                let ih = (bounds.height - top - bottom).max(0.0);
                let half_w = iw * 0.5;
                let half_h = ih * 0.5;
                let cx = ix + half_w;
                let cy = iy + half_h;
                for y in 0..self.height {
                    let py = y as f32 + 0.5 - cy;
                    for x in 0..self.width {
                        let px = x as f32 + 0.5 - cx;
                        let d = sd_rounded_rect(px, py, half_w, half_h, *border_radius);
                        let cov = sdf_coverage(d);
                        mask[y * self.width + x] = (cov * 255.0).round() as u8;
                    }
                }
            }
            ClipPath::Polygon(pts) => {
                if pts.len() >= 3 {
                    let poly_pts: Vec<(f32, f32)> = pts.iter().map(|p| (bounds.x + p.x, bounds.y + p.y)).collect();
                    for y in 0..self.height {
                        let py = y as f32 + 0.5;
                        for x in 0..self.width {
                            let px = x as f32 + 0.5;
                            let mut inside = false;
                            let n = poly_pts.len();
                            let mut j = n - 1;
                            for i in 0..n {
                                let (xi, yi) = poly_pts[i];
                                let (xj, yj) = poly_pts[j];
                                if ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi) {
                                    inside = !inside;
                                }
                                j = i;
                            }
                            if inside {
                                mask[y * self.width + x] = 255;
                            }
                        }
                    }
                }
            }
            ClipPath::Path(svg_data) => {
                let path = strata_gpu_path::PathBuilder::from_svg(svg_data);
                let curves = path.to_quadratics(0.2);
                let offset_curves: Vec<strata_gpu_path::QuadraticBezier> = curves.into_iter().map(|q| {
                    strata_gpu_path::QuadraticBezier::new(
                        strata_gpu_path::Point2D::new(q.p0.x + bounds.x, q.p0.y + bounds.y),
                        strata_gpu_path::Point2D::new(q.p1.x + bounds.x, q.p1.y + bounds.y),
                        strata_gpu_path::Point2D::new(q.p2.x + bounds.x, q.p2.y + bounds.y),
                    )
                }).collect();

                for y in 0..self.height {
                    let py = y as f32 + 0.5;
                    for x in 0..self.width {
                        let pt = strata_gpu_path::Point2D::new(x as f32 + 0.5, py);
                        if strata_gpu_path::is_point_inside_closed_path(pt, &offset_curves) {
                            mask[y * self.width + x] = 255;
                        }
                    }
                }
            }
        }

        if let Some(parent) = self.clip_mask_stack.last() {
            for i in 0..mask.len() {
                mask[i] = ((mask[i] as u16 * parent[i] as u16) / 255) as u8;
            }
        }

        self.clip_mask_stack.push(mask);
    }

    /// Pops the active geometric clip mask.
    pub fn pop_clip_path(&mut self) {
        self.clip_mask_stack.pop();
    }

    /// Convenience alias for execute_display_list.
    pub fn render_display_list(&mut self, list: &DisplayList) {
        self.execute_display_list(list);
    }

    /// Executes an entire DisplayList onto this FrameBuffer.
    pub fn execute_display_list(&mut self, list: &DisplayList) {
        let full_bounds = Rect::new(0.0, 0.0, self.width as f32, self.height as f32);
        self.execute_display_list_damaged(list, full_bounds);
    }

    /// Executes drawing commands strictly scissored to the specified damage rectangle.
    /// Commands that do not intersect damage are skipped in O(1).
    pub fn execute_display_list_damaged(&mut self, list: &DisplayList, damage: Rect) {
        if damage.width <= 0.0 || damage.height <= 0.0 {
            return;
        }

        let mut clip_stack: Vec<Rect> = Vec::new();
        let mut transform_stack: Vec<strata_primitives::geom::Matrix4x4> = Vec::new();
        let mut filter_stack: Vec<(Rect, strata_style::effects::BackdropFilter)> = Vec::new();
        let mut layer_stack: Vec<LayerState> = Vec::new();

        for cmd in list.commands() {
            // 1. Process state stack push/pop operations
            match cmd {
                DrawCommand::PushClip { bounds, .. } => {
                    let combined = match clip_stack.last() {
                        Some(prev) => prev
                            .intersect(*bounds)
                            .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)),
                        None => *bounds,
                    };
                    clip_stack.push(combined);
                    continue;
                }
                DrawCommand::PopClip => {
                    clip_stack.pop();
                    continue;
                }
                DrawCommand::PushTransform { matrix } => {
                    let combined = match transform_stack.last() {
                        Some(prev) => prev.multiply(matrix),
                        None => *matrix,
                    };
                    transform_stack.push(combined);
                    continue;
                }
                DrawCommand::PopTransform => {
                    transform_stack.pop();
                    continue;
                }
                DrawCommand::PushFilter { bounds, filter } => {
                    filter_stack.push((*bounds, filter.clone()));
                    continue;
                }
                DrawCommand::PopFilter => {
                    if let Some((bounds, filter)) = filter_stack.pop() {
                        let active_matrix = transform_stack.last();
                        let tb = if let Some(mat) = active_matrix {
                            if !mat.is_identity() {
                                let (x, y, w, h) = mat.transform_bounds(bounds.x, bounds.y, bounds.width, bounds.height);
                                Rect::new(x, y, w, h)
                            } else {
                                bounds
                            }
                        } else {
                            bounds
                        };
                        let target = if let Some(top) = layer_stack.last_mut() {
                            &mut top.buffer
                        } else {
                            &mut *self
                        };
                        target.apply_filter(tb, &filter);
                    }
                    continue;
                }
                DrawCommand::PushBackdropFilter { .. } => {
                    continue;
                }
                DrawCommand::PopBackdropFilter => {
                    continue;
                }
                DrawCommand::PushOpacity { alpha } => {
                    layer_stack.push(LayerState {
                        buffer: FrameBuffer::new(self.width, self.height),
                        opacity: *alpha,
                        blend_mode: BlendMode::Normal,
                    });
                    continue;
                }
                DrawCommand::PopOpacity => {
                    if let Some(popped) = layer_stack.pop() {
                        let effective_clip = match clip_stack.last() {
                            Some(prev) => prev.intersect(damage),
                            None => Some(damage),
                        };
                        let target = if let Some(top) = layer_stack.last_mut() {
                            &mut top.buffer
                        } else {
                            &mut *self
                        };
                        composite_layer(target, &popped.buffer, popped.opacity, popped.blend_mode, effective_clip);
                    }
                    continue;
                }
                DrawCommand::PushBlendMode { mode } => {
                    layer_stack.push(LayerState {
                        buffer: FrameBuffer::new(self.width, self.height),
                        opacity: 1.0,
                        blend_mode: *mode,
                    });
                    continue;
                }
                DrawCommand::PopBlendMode => {
                    if let Some(popped) = layer_stack.pop() {
                        let effective_clip = match clip_stack.last() {
                            Some(prev) => prev.intersect(damage),
                            None => Some(damage),
                        };
                        let target = if let Some(top) = layer_stack.last_mut() {
                            &mut top.buffer
                        } else {
                            &mut *self
                        };
                        composite_layer(target, &popped.buffer, popped.opacity, popped.blend_mode, effective_clip);
                    }
                    continue;
                }
                DrawCommand::PushIsolation { .. } => {
                    layer_stack.push(LayerState {
                        buffer: FrameBuffer::new(self.width, self.height),
                        opacity: 1.0,
                        blend_mode: BlendMode::Normal,
                    });
                    continue;
                }
                DrawCommand::PopIsolation => {
                    if let Some(popped) = layer_stack.pop() {
                        let effective_clip = match clip_stack.last() {
                            Some(prev) => prev.intersect(damage),
                            None => Some(damage),
                        };
                        let target = if let Some(top) = layer_stack.last_mut() {
                            &mut top.buffer
                        } else {
                            &mut *self
                        };
                        composite_layer(target, &popped.buffer, popped.opacity, popped.blend_mode, effective_clip);
                    }
                    continue;
                }
                DrawCommand::PushClipPath { bounds, clip_path } => {
                    let active_matrix = transform_stack.last();
                    let tb = if let Some(mat) = active_matrix {
                        if !mat.is_identity() {
                            let (x, y, w, h) = mat.transform_bounds(bounds.x, bounds.y, bounds.width, bounds.height);
                            Rect::new(x, y, w, h)
                        } else {
                            *bounds
                        }
                    } else {
                        *bounds
                    };
                    let target = if let Some(top) = layer_stack.last_mut() {
                        &mut top.buffer
                    } else {
                        &mut *self
                    };
                    target.push_clip_path(tb, clip_path);
                    continue;
                }
                DrawCommand::PopClipPath => {
                    let target = if let Some(top) = layer_stack.last_mut() {
                        &mut top.buffer
                    } else {
                        &mut *self
                    };
                    target.pop_clip_path();
                    continue;
                }
                _ => {}
            }

            // 2. Compute effective damage / clip rectangle
            let effective_clip = match clip_stack.last() {
                Some(prev) => match prev.intersect(damage) {
                    Some(c) => c,
                    None => continue,
                },
                None => damage,
            };

            let active_matrix = transform_stack.last();
            let transform_rect_fn = |r: Rect| -> Rect {
                if let Some(mat) = active_matrix {
                    if !mat.is_identity() {
                        let (x, y, w, h) = mat.transform_bounds(r.x, r.y, r.width, r.height);
                        return Rect::new(x, y, w, h);
                    }
                }
                r
            };

            // 3. Skip drawing commands whose bounds do not intersect effective clip
            let transformed_cmd_bounds = cmd.bounds().map(transform_rect_fn);
            if let Some(cmd_bounds) = transformed_cmd_bounds {
                if cmd_bounds.intersect(effective_clip).is_none() {
                    continue;
                }
            }

            // 4. Dispatch drawing commands to target
            let target = if let Some(top) = layer_stack.last_mut() {
                &mut top.buffer
            } else {
                &mut *self
            };

            match cmd {
                DrawCommand::DrawRect { bounds, color } => {
                    target.draw_rect(transform_rect_fn(*bounds), *color, Some(effective_clip));
                }
                DrawCommand::DrawRoundedRect {
                    bounds,
                    color,
                    border_radius,
                    border_width,
                    border_color,
                } => {
                    target.draw_rounded_rect(
                        transform_rect_fn(*bounds),
                        *color,
                        border_radius.top_left,
                        border_width.top,
                        *border_color,
                        Some(effective_clip),
                    );
                }
                DrawCommand::DrawShadow { bounds, shadow } => {
                    target.draw_shadow(transform_rect_fn(*bounds), shadow, Some(effective_clip));
                }
                DrawCommand::DrawBorder {
                    bounds,
                    widths,
                    color,
                    border_radius,
                    ..
                } => {
                    target.draw_rounded_rect(
                        transform_rect_fn(*bounds),
                        Color::TRANSPARENT,
                        border_radius.top_left,
                        widths.top,
                        Some(*color),
                        Some(effective_clip),
                    );
                }
                DrawCommand::DrawText {
                    origin,
                    text,
                    font_size,
                    color,
                    ..
                } => {
                    let (ox, oy) = if let Some(m) = active_matrix {
                        m.transform_point(origin.0, origin.1)
                    } else {
                        *origin
                    };
                    target.draw_text((ox, oy), text, *font_size, *color, Some(effective_clip));
                }
                DrawCommand::DrawViewport3D { bounds, scene } => {
                    crate::rasterizer3d::render_scene_3d(
                        target,
                        transform_rect_fn(*bounds),
                        scene,
                        Some(effective_clip),
                    );
                }
                DrawCommand::DrawCanvas2D { bounds, commands } => {
                    let tb = transform_rect_fn(*bounds);
                    crate::canvas2d::rasterize_canvas_commands(
                        target,
                        commands,
                        tb.x,
                        tb.y,
                        Some(effective_clip),
                    );
                }
                DrawCommand::DrawTextFragments { fragments } => {
                    for frag in fragments {
                        if !frag.text.is_empty() {
                            let fb = transform_rect_fn(frag.bounds);
                            target.draw_text(
                                (fb.x, fb.y),
                                &frag.text,
                                frag.style.font_size,
                                frag.style.color,
                                Some(effective_clip),
                            );
                        }
                    }
                }
                DrawCommand::DrawInsetShadow { bounds, shadow, .. } => {
                    target.draw_shadow(transform_rect_fn(*bounds), shadow, Some(effective_clip));
                }
                DrawCommand::DrawLinearGradient {
                    bounds,
                    angle_deg,
                    stops,
                    border_radius,
                } => {
                    target.draw_linear_gradient(
                        transform_rect_fn(*bounds),
                        *angle_deg,
                        stops,
                        border_radius.top_left,
                        Some(effective_clip),
                    );
                }
                DrawCommand::DrawRadialGradient {
                    bounds,
                    center,
                    stops,
                    border_radius,
                } => {
                    target.draw_radial_gradient(
                        transform_rect_fn(*bounds),
                        *center,
                        stops,
                        border_radius.top_left,
                        Some(effective_clip),
                    );
                }
                DrawCommand::DrawImage { bounds, image } => {
                    target.draw_image(transform_rect_fn(*bounds), image, Some(effective_clip));
                }
                DrawCommand::DrawSvgPath {
                    bounds,
                    path_data,
                    stroke_color,
                    fill_color,
                    stroke_width,
                    stroke_linecap,
                    stroke_linejoin,
                    stroke_dasharray,
                    fill_rule,
                    view_box,
                } => {
                    target.draw_svg_path(
                        transform_rect_fn(*bounds),
                        path_data,
                        *stroke_color,
                        *fill_color,
                        *stroke_width,
                        *stroke_linecap,
                        *stroke_linejoin,
                        stroke_dasharray,
                        *fill_rule,
                        *view_box,
                        Some(effective_clip),
                    );
                }
                DrawCommand::DrawGpuPath {
                    bounds,
                    path,
                    stroke,
                    fill_color,
                    stroke_color,
                    transform,
                } => {
                    let mut combined_transform = *transform;
                    if let Some(m) = active_matrix {
                        combined_transform = Some(match combined_transform {
                            Some(t) => m.multiply(&t),
                            None => *m,
                        });
                    }
                    target.draw_gpu_path(
                        transform_rect_fn(*bounds),
                        path,
                        *stroke,
                        *fill_color,
                        *stroke_color,
                        combined_transform,
                        Some(effective_clip),
                    );
                }
                _ => {}
            }
        }

        // Flush any remaining active layers
        while let Some(popped) = layer_stack.pop() {
            let target = if let Some(top) = layer_stack.last_mut() {
                &mut top.buffer
            } else {
                &mut *self
            };
            composite_layer(target, &popped.buffer, popped.opacity, popped.blend_mode, None);
        }
    }

    fn compute_clamped_bounds(
        &self,
        bounds: Rect,
        clip: Option<Rect>,
    ) -> (usize, usize, usize, usize) {
        let rect = match clip {
            Some(c) => bounds.intersect(c).unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)),
            None => bounds,
        };

        let min_x = (rect.x.floor() as isize).max(0).min(self.width as isize) as usize;
        let max_x = (rect.max_x().ceil() as isize)
            .max(0)
            .min(self.width as isize) as usize;
        let min_y = (rect.y.floor() as isize).max(0).min(self.height as isize) as usize;
        let max_y = (rect.max_y().ceil() as isize)
            .max(0)
            .min(self.height as isize) as usize;

        (min_x, max_x, min_y, max_y)
    }

    /// Encodes the current RGBA framebuffer into a standard, spec-compliant PNG byte buffer.
    pub fn to_png_bytes(&self) -> Result<Vec<u8>, String> {
        if self.width == 0 || self.height == 0 {
            return Err("FrameBuffer dimensions must be non-zero to encode PNG".to_string());
        }

        let mut out = Vec::new();
        // 1. PNG 8-byte signature
        out.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);

        // 2. IHDR Chunk (13 bytes)
        let mut ihdr_data = Vec::with_capacity(13);
        ihdr_data.extend_from_slice(&(self.width as u32).to_be_bytes());
        ihdr_data.extend_from_slice(&(self.height as u32).to_be_bytes());
        ihdr_data.push(8); // Bit depth: 8 bits per channel
        ihdr_data.push(6); // Color type: 6 = RGBA
        ihdr_data.push(0); // Compression method: 0 = Deflate
        ihdr_data.push(0); // Filter method: 0 = Standard
        ihdr_data.push(0); // Interlace method: 0 = None
        write_png_chunk(&mut out, b"IHDR", &ihdr_data);

        // 3. Raw Scanlines with Filter None (0)
        let row_stride = self.width * 4;
        let mut raw_scanlines = Vec::with_capacity(self.height * (1 + row_stride));
        for y in 0..self.height {
            raw_scanlines.push(0); // Filter type: None
            let row_start = y * row_stride;
            raw_scanlines.extend_from_slice(&self.pixels[row_start..row_start + row_stride]);
        }

        // 4. Deflate zlib stream (RFC 1950 & RFC 1951)
        let mut deflate_stream = Vec::new();
        deflate_stream.push(0x78); // CMF: Deflate, 32K window
        deflate_stream.push(0x01); // FLG: No preset dict, check bits

        const CHUNK_SIZE: usize = 65535;
        let total = raw_scanlines.len();
        let mut offset = 0;
        while offset < total {
            let chunk_len = (total - offset).min(CHUNK_SIZE);
            let is_last = (offset + chunk_len) >= total;
            let bfinal_btype: u8 = if is_last { 0x01 } else { 0x00 };
            deflate_stream.push(bfinal_btype);

            let len_u16 = chunk_len as u16;
            let nlen_u16 = !len_u16;
            deflate_stream.extend_from_slice(&len_u16.to_le_bytes());
            deflate_stream.extend_from_slice(&nlen_u16.to_le_bytes());
            deflate_stream.extend_from_slice(&raw_scanlines[offset..offset + chunk_len]);
            offset += chunk_len;
        }

        let adler = adler32(&raw_scanlines);
        deflate_stream.extend_from_slice(&adler.to_be_bytes());

        // 5. IDAT Chunk
        write_png_chunk(&mut out, b"IDAT", &deflate_stream);

        // 6. IEND Chunk
        write_png_chunk(&mut out, b"IEND", &[]);

        Ok(out)
    }

    /// Exports the current framebuffer to a PNG image file at the specified path.
    pub fn export_png<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        let bytes = self.to_png_bytes().map_err(std::io::Error::other)?;
        let mut file = File::create(path)?;
        file.write_all(&bytes)?;
        file.flush()?;
        Ok(())
    }

    /// Computes a 64-bit difference hash (dHash) for visual regression testing and perceptual diffing.
    #[allow(clippy::needless_range_loop)]
    pub fn compute_phash(&self) -> u64 {
        if self.width == 0 || self.height == 0 {
            return 0;
        }

        let mut grid = [[0.0f32; 9]; 8];
        for y in 0..8 {
            for x in 0..9 {
                let px = (((x as f32 + 0.5) * self.width as f32 / 9.0).floor() as usize)
                    .min(self.width - 1);
                let py = (((y as f32 + 0.5) * self.height as f32 / 8.0).floor() as usize)
                    .min(self.height - 1);
                let idx = (py * self.width + px) * 4;
                let r = self.pixels[idx] as f32;
                let g = self.pixels[idx + 1] as f32;
                let b = self.pixels[idx + 2] as f32;
                grid[y][x] = 0.299 * r + 0.587 * g + 0.114 * b;
            }
        }

        let mut hash = 0u64;
        for y in 0..8 {
            for x in 0..8 {
                if grid[y][x] > grid[y][x + 1] {
                    hash |= 1u64 << (y * 8 + x);
                }
            }
        }
        hash
    }

    /// Computes the Hamming distance (number of differing bits) between two perceptual hashes.
    pub fn phash_distance(hash_a: u64, hash_b: u64) -> u32 {
        (hash_a ^ hash_b).count_ones()
    }
}

const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;
        while j < 8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
};

fn adler32(data: &[u8]) -> u32 {
    let mut s1 = 1u32;
    let mut s2 = 0u32;
    for &b in data {
        s1 = (s1 + b as u32) % 65521;
        s2 = (s2 + s1) % 65521;
    }
    (s2 << 16) | s1
}

fn write_png_chunk(out: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(chunk_type);
    out.extend_from_slice(data);

    let mut crc = 0xFFFF_FFFFu32;
    for &byte in chunk_type {
        let idx = ((crc ^ (byte as u32)) & 0xFF) as usize;
        crc = (crc >> 8) ^ CRC_TABLE[idx];
    }
    for &byte in data {
        let idx = ((crc ^ (byte as u32)) & 0xFF) as usize;
        crc = (crc >> 8) ^ CRC_TABLE[idx];
    }
    let final_crc = !crc;
    out.extend_from_slice(&final_crc.to_be_bytes());
}

struct LayerState {
    buffer: FrameBuffer,
    opacity: f32,
    blend_mode: BlendMode,
}

fn composite_layer(
    dst: &mut FrameBuffer,
    src: &FrameBuffer,
    opacity: f32,
    blend_mode: BlendMode,
    clip: Option<Rect>,
) {
    if opacity <= 0.0 {
        return;
    }
    let opacity = opacity.clamp(0.0, 1.0);
    let (min_x, max_x, min_y, max_y) = if let Some(c) = clip {
        dst.compute_clamped_bounds(c, None)
    } else {
        (0, dst.width.min(src.width), 0, dst.height.min(src.height))
    };

    for y in min_y..max_y {
        for x in min_x..max_x {
            let src_idx = (y * src.width + x) * 4;
            let src_raw_a = src.pixels[src_idx + 3] as f32 / 255.0;
            if src_raw_a <= 0.001 {
                continue;
            }

            let clip_cov = if let Some(mask) = dst.clip_mask_stack.last() {
                mask[y * dst.width + x] as f32 / 255.0
            } else {
                1.0
            };
            if clip_cov <= 0.001 {
                continue;
            }

            let cs_r = src.pixels[src_idx] as f32 / 255.0;
            let cs_g = src.pixels[src_idx + 1] as f32 / 255.0;
            let cs_b = src.pixels[src_idx + 2] as f32 / 255.0;
            let alpha_s = (src_raw_a * opacity * clip_cov).clamp(0.0, 1.0);

            let dst_idx = (y * dst.width + x) * 4;
            let cb_r = dst.pixels[dst_idx] as f32 / 255.0;
            let cb_g = dst.pixels[dst_idx + 1] as f32 / 255.0;
            let cb_b = dst.pixels[dst_idx + 2] as f32 / 255.0;
            let alpha_b = dst.pixels[dst_idx + 3] as f32 / 255.0;

            let b_r = blend_channel(cb_r, cs_r, blend_mode);
            let b_g = blend_channel(cb_g, cs_g, blend_mode);
            let b_b = blend_channel(cb_b, cs_b, blend_mode);

            let alpha_o = alpha_s + alpha_b * (1.0 - alpha_s);
            if alpha_o > 1e-5 {
                let out_r = ((1.0 - alpha_b) * alpha_s * cs_r
                    + (1.0 - alpha_s) * alpha_b * cb_r
                    + alpha_b * alpha_s * b_r)
                    / alpha_o;
                let out_g = ((1.0 - alpha_b) * alpha_s * cs_g
                    + (1.0 - alpha_s) * alpha_b * cb_g
                    + alpha_b * alpha_s * b_g)
                    / alpha_o;
                let out_b = ((1.0 - alpha_b) * alpha_s * cs_b
                    + (1.0 - alpha_s) * alpha_b * cb_b
                    + alpha_b * alpha_s * b_b)
                    / alpha_o;

                dst.pixels[dst_idx] = (out_r.clamp(0.0, 1.0) * 255.0).round() as u8;
                dst.pixels[dst_idx + 1] = (out_g.clamp(0.0, 1.0) * 255.0).round() as u8;
                dst.pixels[dst_idx + 2] = (out_b.clamp(0.0, 1.0) * 255.0).round() as u8;
                dst.pixels[dst_idx + 3] = (alpha_o.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }
}

fn blend_channel(cb: f32, cs: f32, mode: BlendMode) -> f32 {
    match mode {
        BlendMode::Normal => cs,
        BlendMode::Multiply => cb * cs,
        BlendMode::Screen => cb + cs - cb * cs,
        BlendMode::Darken => cb.min(cs),
        BlendMode::Lighten => cb.max(cs),
        BlendMode::Difference => (cb - cs).abs(),
        BlendMode::Exclusion => cb + cs - 2.0 * cb * cs,
        BlendMode::Overlay => {
            if cb <= 0.5 {
                2.0 * cb * cs
            } else {
                1.0 - 2.0 * (1.0 - cb) * (1.0 - cs)
            }
        }
        BlendMode::HardLight => {
            if cs <= 0.5 {
                2.0 * cb * cs
            } else {
                1.0 - 2.0 * (1.0 - cb) * (1.0 - cs)
            }
        }
        BlendMode::SoftLight => {
            if cs <= 0.5 {
                cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb)
            } else {
                let d = if cb <= 0.25 {
                    ((16.0 * cb - 12.0) * cb + 4.0) * cb
                } else {
                    cb.sqrt()
                };
                cb + (2.0 * cs - 1.0) * (d - cb)
            }
        }
        BlendMode::ColorDodge => {
            if cb == 0.0 {
                0.0
            } else if cs >= 1.0 {
                1.0
            } else {
                (cb / (1.0 - cs)).min(1.0)
            }
        }
        BlendMode::ColorBurn => {
            if cb >= 1.0 {
                1.0
            } else if cs <= 0.0 {
                0.0
            } else {
                1.0 - ((1.0 - cb) / cs).min(1.0)
            }
        }
        _ => cs,
    }
}

fn normalize_color_stops(stops: &[ColorStop]) -> Vec<(f32, Color)> {
    if stops.is_empty() {
        return Vec::new();
    }
    if stops.len() == 1 {
        return vec![(0.0, stops[0].color), (1.0, stops[0].color)];
    }

    let mut result: Vec<(f32, Color)> = stops.iter().map(|s| (s.position.unwrap_or(-1.0), s.color)).collect();

    // 1. First stop default is 0.0
    if result[0].0 < 0.0 {
        result[0].0 = 0.0;
    }
    // 2. Last stop default is 1.0
    let last_idx = result.len() - 1;
    if result[last_idx].0 < 0.0 {
        result[last_idx].0 = 1.0;
    }

    // 3. Interpolate missing intermediate positions
    let mut i = 0;
    while i < result.len() {
        if result[i].0 < 0.0 {
            let start_idx = i - 1;
            let start_pos = result[start_idx].0;
            let mut end_idx = i;
            while end_idx < result.len() && result[end_idx].0 < 0.0 {
                end_idx += 1;
            }
            let end_pos = if end_idx < result.len() { result[end_idx].0 } else { 1.0 };
            let count = (end_idx - start_idx) as f32;
            let step = (end_pos - start_pos) / count;
            for j in (start_idx + 1)..end_idx {
                result[j].0 = start_pos + step * (j - start_idx) as f32;
            }
            i = end_idx;
        } else {
            i += 1;
        }
    }

    // 4. Ensure monotonicity
    let mut current_max = 0.0f32;
    for entry in &mut result {
        if entry.0 < current_max {
            entry.0 = current_max;
        } else {
            current_max = entry.0;
        }
    }

    result
}

fn sample_gradient(stops: &[(f32, Color)], t: f32) -> Color {
    if stops.is_empty() {
        return Color::TRANSPARENT;
    }
    let t = t.clamp(0.0, 1.0);
    if t <= stops[0].0 {
        return stops[0].1;
    }
    if t >= stops.last().unwrap().0 {
        return stops.last().unwrap().1;
    }

    for i in 0..stops.len() - 1 {
        let (p0, c0) = stops[i];
        let (p1, c1) = stops[i + 1];
        if t >= p0 && t <= p1 {
            let factor = if p1 > p0 { (t - p0) / (p1 - p0) } else { 0.0 };
            let r = c0.r() + (c1.r() - c0.r()) * factor;
            let g = c0.g() + (c1.g() - c0.g()) * factor;
            let b = c0.b() + (c1.b() - c0.b()) * factor;
            let a = c0.a() + (c1.a() - c0.a()) * factor;
            return Color::from_f32(r, g, b, a);
        }
    }

    stops.last().unwrap().1
}

fn decode_or_get_rgba(data: &[u8], width: usize, height: usize) -> Vec<u8> {
    if data.len() == width * height * 4 {
        return data.to_vec();
    }
    if data.len() >= 8 && data[0..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        if let Ok(decoded) = decode_png_to_rgba(data, width, height) {
            return decoded;
        }
    }
    if data.len() == width * height * 3 {
        let mut out = Vec::with_capacity(width * height * 4);
        for chunk in data.chunks_exact(3) {
            out.push(chunk[0]);
            out.push(chunk[1]);
            out.push(chunk[2]);
            out.push(255);
        }
        return out;
    }

    vec![0u8; width * height * 4]
}

fn decode_png_to_rgba(data: &[u8], expected_w: usize, expected_h: usize) -> Result<Vec<u8>, String> {
    if data.len() < 8 || data[0..8] != [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        return Err("Invalid PNG signature".to_string());
    }

    let mut cursor = 8;
    let mut width = 0usize;
    let mut height = 0usize;
    let mut color_type = 6u8;
    let mut idat_data = Vec::new();

    while cursor + 12 <= data.len() {
        let chunk_len = u32::from_be_bytes(data[cursor..cursor + 4].try_into().unwrap()) as usize;
        let chunk_type = &data[cursor + 4..cursor + 8];
        let chunk_payload = &data[cursor + 8..cursor + 8 + chunk_len];
        cursor += 12 + chunk_len;

        if chunk_type == b"IHDR" {
            if chunk_len < 13 {
                return Err("IHDR chunk too short".to_string());
            }
            width = u32::from_be_bytes(chunk_payload[0..4].try_into().unwrap()) as usize;
            height = u32::from_be_bytes(chunk_payload[4..8].try_into().unwrap()) as usize;
            color_type = chunk_payload[9];
        } else if chunk_type == b"IDAT" {
            idat_data.extend_from_slice(chunk_payload);
        } else if chunk_type == b"IEND" {
            break;
        }
    }

    if width == 0 || height == 0 {
        width = expected_w;
        height = expected_h;
    }

    let decompressed = miniz_oxide::inflate::decompress_to_vec_zlib(&idat_data)
        .map_err(|e| format!("PNG zlib inflation failed: {e:?}"))?;

    let bpp: usize = match color_type {
        0 => 1,
        2 => 3,
        3 => 1,
        4 => 2,
        6 => 4,
        _ => 4,
    };

    let stride = 1 + width * bpp;
    if decompressed.len() < height * stride {
        return Err("Decompressed PNG data truncated".to_string());
    }

    let mut uncompressed_rows: Vec<u8> = vec![0u8; height * width * bpp];
    let mut prev_row: Vec<u8> = vec![0u8; width * bpp];

    for y in 0..height {
        let filter_type = decompressed[y * stride];
        let raw_row = &decompressed[y * stride + 1..y * stride + stride];
        let row_out_start = y * width * bpp;
        let row_out = &mut uncompressed_rows[row_out_start..row_out_start + width * bpp];

        match filter_type {
            0 => {
                row_out.copy_from_slice(raw_row);
            }
            1 => {
                for x in 0..raw_row.len() {
                    let a = if x >= bpp { row_out[x - bpp] } else { 0 };
                    row_out[x] = raw_row[x].wrapping_add(a);
                }
            }
            2 => {
                for x in 0..raw_row.len() {
                    let b = prev_row[x];
                    row_out[x] = raw_row[x].wrapping_add(b);
                }
            }
            3 => {
                for x in 0..raw_row.len() {
                    let a = if x >= bpp { row_out[x - bpp] as u16 } else { 0 };
                    let b = prev_row[x] as u16;
                    row_out[x] = raw_row[x].wrapping_add(((a + b) / 2) as u8);
                }
            }
            4 => {
                for x in 0..raw_row.len() {
                    let a = if x >= bpp { row_out[x - bpp] as i32 } else { 0 };
                    let b = prev_row[x] as i32;
                    let c = if x >= bpp { prev_row[x - bpp] as i32 } else { 0 };
                    let p = a + b - c;
                    let pa = (p - a).abs();
                    let pb = (p - b).abs();
                    let pc = (p - c).abs();
                    let pr = if pa <= pb && pa <= pc { a } else if pb <= pc { b } else { c };
                    row_out[x] = raw_row[x].wrapping_add(pr as u8);
                }
            }
            _ => {
                row_out.copy_from_slice(raw_row);
            }
        }

        prev_row.copy_from_slice(row_out);
    }

    let mut rgba_out = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        let row_start = y * width * bpp;
        for x in 0..width {
            let px_start = row_start + x * bpp;
            match color_type {
                6 => {
                    rgba_out.extend_from_slice(&uncompressed_rows[px_start..px_start + 4]);
                }
                2 => {
                    rgba_out.push(uncompressed_rows[px_start]);
                    rgba_out.push(uncompressed_rows[px_start + 1]);
                    rgba_out.push(uncompressed_rows[px_start + 2]);
                    rgba_out.push(255);
                }
                0 => {
                    let g = uncompressed_rows[px_start];
                    rgba_out.push(g);
                    rgba_out.push(g);
                    rgba_out.push(g);
                    rgba_out.push(255);
                }
                4 => {
                    let g = uncompressed_rows[px_start];
                    let a = uncompressed_rows[px_start + 1];
                    rgba_out.push(g);
                    rgba_out.push(g);
                    rgba_out.push(g);
                    rgba_out.push(a);
                }
                _ => {
                    rgba_out.extend_from_slice(&[255, 255, 255, 255]);
                }
            }
        }
    }

    Ok(rgba_out)
}
