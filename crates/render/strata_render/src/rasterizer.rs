//! Analytical Signed Distance Field (SDF) 2D Rasterizer & Headless Software FrameBuffer.

use crate::compositor::{DisplayList, DrawCommand};
use strata_layout::Rect;
use strata_style::types::{BoxShadow, Color};

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
}

impl FrameBuffer {
    /// Creates a new frame buffer initialized to transparent black.
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width * height * 4],
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
    pub fn set_pixel(&mut self, x: usize, y: usize, src: Color) {
        if x >= self.width || y >= self.height {
            return;
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
            for (row_idx, &row) in glyph.iter().enumerate() {
                    if row == 0 {
                        continue;
                    }
                    for col_idx in 0..8 {
                        if (row & (1 << (7 - col_idx))) != 0 {
                            let px = cursor_x + (col_idx as f32) * scale;
                            let py = cursor_y + (row_idx as f32) * scale;

                            if scale <= 1.0 {
                                let x = px.round() as usize;
                                let y = py.round() as usize;
                                if let Some(c) = clip {
                                    if !c.contains_point(px, py) {
                                        continue;
                                    }
                                }
                                self.set_pixel(x, y, color);
                            } else {
                                let min_x = px.floor() as usize;
                                let max_x = ((px + scale).ceil() as usize).min(self.width);
                                let min_y = py.floor() as usize;
                                let max_y = ((py + scale).ceil() as usize).min(self.height);

                                for y in min_y..max_y {
                                    for x in min_x..max_x {
                                        if let Some(c) = clip {
                                            if !c.contains_point(x as f32, y as f32) {
                                                continue;
                                            }
                                        }
                                        self.set_pixel(x, y, color);
                                    }
                                }
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

    /// Executes an entire DisplayList onto this FrameBuffer.
    pub fn execute_display_list(&mut self, list: &DisplayList) {
        let mut clip_stack: Vec<Rect> = Vec::new();
        let mut transform_stack: Vec<strata_primitives::geom::Matrix4x4> = Vec::new();
        let mut filter_stack: Vec<(Rect, strata_style::effects::BackdropFilter)> = Vec::new();

        for cmd in list.commands() {
            let current_clip = clip_stack.last().copied();

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

            match cmd {
                DrawCommand::DrawRect { bounds, color } => {
                    self.draw_rect(transform_rect_fn(*bounds), *color, current_clip);
                }
                DrawCommand::DrawRoundedRect {
                    bounds,
                    color,
                    border_radius,
                    border_width,
                    border_color,
                } => {
                    self.draw_rounded_rect(
                        transform_rect_fn(*bounds),
                        *color,
                        border_radius.top_left,
                        border_width.top,
                        *border_color,
                        current_clip,
                    );
                }
                DrawCommand::DrawShadow { bounds, shadow } => {
                    self.draw_shadow(transform_rect_fn(*bounds), shadow, current_clip);
                }
                DrawCommand::DrawBorder {
                    bounds,
                    widths,
                    color,
                    border_radius,
                    ..
                } => {
                    self.draw_rounded_rect(
                        transform_rect_fn(*bounds),
                        Color::TRANSPARENT,
                        border_radius.top_left,
                        widths.top,
                        Some(*color),
                        current_clip,
                    );
                }
                DrawCommand::DrawText {
                    origin,
                    text,
                    font_size,
                    color,
                    ..
                } => {
                    let pt = if let Some(m) = active_matrix {
                        m.transform_point(origin.0, origin.1)
                    } else {
                        *origin
                    };
                    self.draw_text(pt, text, *font_size, *color, current_clip);
                }
                DrawCommand::DrawViewport3D { bounds, scene } => {
                    crate::rasterizer3d::render_scene_3d(
                        self,
                        transform_rect_fn(*bounds),
                        scene,
                        current_clip,
                    );
                }
                DrawCommand::DrawCanvas2D { bounds, commands } => {
                    let tb = transform_rect_fn(*bounds);
                    crate::canvas2d::rasterize_canvas_commands(
                        self,
                        commands,
                        tb.x,
                        tb.y,
                        current_clip,
                    );
                }
                DrawCommand::DrawTextFragments { fragments } => {
                    for frag in fragments {
                        if !frag.text.is_empty() {
                            let fb = transform_rect_fn(frag.bounds);
                            self.draw_text(
                                (fb.x, fb.y),
                                &frag.text,
                                frag.style.font_size,
                                frag.style.color,
                                current_clip,
                            );
                        }
                    }
                }
                DrawCommand::DrawInsetShadow { bounds, shadow, .. } => {
                    self.draw_shadow(transform_rect_fn(*bounds), shadow, current_clip);
                }
                DrawCommand::DrawLinearGradient {
                    bounds,
                    stops,
                    border_radius,
                    ..
                } => {
                    if let Some(first) = stops.first() {
                        self.draw_rounded_rect(
                            transform_rect_fn(*bounds),
                            first.color,
                            border_radius.top_left,
                            0.0,
                            None,
                            current_clip,
                        );
                    }
                }
                DrawCommand::DrawRadialGradient {
                    bounds,
                    stops,
                    border_radius,
                    ..
                } => {
                    if let Some(first) = stops.first() {
                        self.draw_rounded_rect(
                            transform_rect_fn(*bounds),
                            first.color,
                            border_radius.top_left,
                            0.0,
                            None,
                            current_clip,
                        );
                    }
                }
                DrawCommand::PushBackdropFilter { .. } | DrawCommand::PopBackdropFilter => {}
                DrawCommand::PushFilter { bounds, filter } => {
                    filter_stack.push((*bounds, filter.clone()));
                }
                DrawCommand::PopFilter => {
                    if let Some((bounds, filter)) = filter_stack.pop() {
                        self.apply_filter(transform_rect_fn(bounds), &filter);
                    }
                }
                DrawCommand::PushClipPath { .. }
                | DrawCommand::PopClipPath
                | DrawCommand::PushBlendMode { .. }
                | DrawCommand::PopBlendMode
                | DrawCommand::PushIsolation { .. }
                | DrawCommand::PopIsolation
                | DrawCommand::PushOpacity { .. }
                | DrawCommand::PopOpacity
                | DrawCommand::DrawSvgPath { .. }
                | DrawCommand::DrawGpuPath { .. }
                | DrawCommand::DrawImage { .. } => {}
                DrawCommand::PushClip { bounds, .. } => {
                    let combined = match current_clip {
                        Some(prev) => prev
                            .intersect(*bounds)
                            .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)),
                        None => *bounds,
                    };
                    clip_stack.push(combined);
                }
                DrawCommand::PopClip => {
                    clip_stack.pop();
                }
                DrawCommand::PushTransform { matrix } => {
                    let combined = match transform_stack.last() {
                        Some(prev) => prev.multiply(matrix),
                        None => *matrix,
                    };
                    transform_stack.push(combined);
                }
                DrawCommand::PopTransform => {
                    transform_stack.pop();
                }
            }
        }
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

        for cmd in list.commands() {
            if let DrawCommand::PushClip { bounds, .. } = cmd {
                let combined = match clip_stack.last() {
                    Some(prev) => prev
                        .intersect(*bounds)
                        .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)),
                    None => *bounds,
                };
                clip_stack.push(combined);
                continue;
            } else if let DrawCommand::PopClip = cmd {
                clip_stack.pop();
                continue;
            } else if let DrawCommand::PushTransform { matrix } = cmd {
                let combined = match transform_stack.last() {
                    Some(prev) => prev.multiply(matrix),
                    None => *matrix,
                };
                transform_stack.push(combined);
                continue;
            } else if let DrawCommand::PopTransform = cmd {
                transform_stack.pop();
                continue;
            }

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

            let transformed_cmd_bounds = cmd.bounds().map(transform_rect_fn);
            if let Some(cmd_bounds) = transformed_cmd_bounds {
                if cmd_bounds.intersect(effective_clip).is_none() {
                    continue;
                }
            }

            match cmd {
                DrawCommand::DrawRect { bounds, color } => {
                    self.draw_rect(transform_rect_fn(*bounds), *color, Some(effective_clip));
                }
                DrawCommand::DrawRoundedRect {
                    bounds,
                    color,
                    border_radius,
                    border_width,
                    border_color,
                } => {
                    self.draw_rounded_rect(
                        transform_rect_fn(*bounds),
                        *color,
                        border_radius.top_left,
                        border_width.top,
                        *border_color,
                        Some(effective_clip),
                    );
                }
                DrawCommand::DrawShadow { bounds, shadow } => {
                    self.draw_shadow(transform_rect_fn(*bounds), shadow, Some(effective_clip));
                }
                DrawCommand::DrawBorder {
                    bounds,
                    widths,
                    color,
                    border_radius,
                    ..
                } => {
                    self.draw_rounded_rect(
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
                    self.draw_text((ox, oy), text, *font_size, *color, Some(effective_clip));
                }
                DrawCommand::DrawViewport3D { bounds, scene } => {
                    crate::rasterizer3d::render_scene_3d(
                        self,
                        transform_rect_fn(*bounds),
                        scene,
                        Some(effective_clip),
                    );
                }
                DrawCommand::DrawCanvas2D { bounds, commands } => {
                    let tb = transform_rect_fn(*bounds);
                    crate::canvas2d::rasterize_canvas_commands(
                        self,
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
                            self.draw_text(
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
                    self.draw_shadow(transform_rect_fn(*bounds), shadow, Some(effective_clip));
                }
                DrawCommand::DrawLinearGradient {
                    bounds,
                    stops,
                    border_radius,
                    ..
                } => {
                    if let Some(first) = stops.first() {
                        self.draw_rounded_rect(
                            transform_rect_fn(*bounds),
                            first.color,
                            border_radius.top_left,
                            0.0,
                            None,
                            Some(effective_clip),
                        );
                    }
                }
                DrawCommand::DrawRadialGradient {
                    bounds,
                    stops,
                    border_radius,
                    ..
                } => {
                    if let Some(first) = stops.first() {
                        self.draw_rounded_rect(
                            transform_rect_fn(*bounds),
                            first.color,
                            border_radius.top_left,
                            0.0,
                            None,
                            Some(effective_clip),
                        );
                    }
                }
                DrawCommand::PushBackdropFilter { .. } | DrawCommand::PopBackdropFilter => {}
                DrawCommand::PushFilter { bounds, filter } => {
                    filter_stack.push((*bounds, filter.clone()));
                }
                DrawCommand::PopFilter => {
                    if let Some((bounds, filter)) = filter_stack.pop() {
                        self.apply_filter(transform_rect_fn(bounds), &filter);
                    }
                }
                DrawCommand::PushClipPath { .. }
                | DrawCommand::PopClipPath
                | DrawCommand::PushBlendMode { .. }
                | DrawCommand::PopBlendMode
                | DrawCommand::PushIsolation { .. }
                | DrawCommand::PopIsolation
                | DrawCommand::PushOpacity { .. }
                | DrawCommand::PopOpacity
                | DrawCommand::DrawSvgPath { .. }
                | DrawCommand::DrawGpuPath { .. }
                | DrawCommand::DrawImage { .. } => {}
                DrawCommand::PushClip { .. }
                | DrawCommand::PopClip
                | DrawCommand::PushTransform { .. }
                | DrawCommand::PopTransform => unreachable!(),
            }
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
}
