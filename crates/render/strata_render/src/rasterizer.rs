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
                    if border_width > 0.0 && border_color.is_some() {
                        let inner_d = d + border_width;
                        let inner_alpha = sdf_coverage(inner_d);
                        let b_color = border_color.unwrap();

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
                let alpha = (-d / blur).exp().min(1.0).max(0.0) * shadow.color.a();

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

    /// Executes an entire DisplayList onto this FrameBuffer.
    pub fn execute_display_list(&mut self, list: &DisplayList) {
        let mut clip_stack: Vec<Rect> = Vec::new();

        for cmd in list.commands() {
            let current_clip = clip_stack.last().copied();

            match cmd {
                DrawCommand::DrawRect { bounds, color } => {
                    self.draw_rect(*bounds, *color, current_clip);
                }
                DrawCommand::DrawRoundedRect {
                    bounds,
                    color,
                    border_radius,
                    border_width,
                    border_color,
                } => {
                    self.draw_rounded_rect(
                        *bounds,
                        *color,
                        border_radius.top_left,
                        border_width.top,
                        *border_color,
                        current_clip,
                    );
                }
                DrawCommand::DrawShadow { bounds, shadow } => {
                    self.draw_shadow(*bounds, shadow, current_clip);
                }
                DrawCommand::DrawBorder {
                    bounds,
                    widths,
                    color,
                    border_radius,
                } => {
                    self.draw_rounded_rect(
                        *bounds,
                        Color::TRANSPARENT,
                        border_radius.top_left,
                        widths.top,
                        Some(*color),
                        current_clip,
                    );
                }
                DrawCommand::DrawText { .. } => {
                    // Handled by font atlas or software fallback
                }
                DrawCommand::DrawViewport3D { bounds, scene } => {
                    crate::rasterizer3d::render_scene_3d(self, *bounds, scene, current_clip);
                }
                DrawCommand::DrawCanvas2D { bounds, commands } => {
                    crate::canvas2d::rasterize_canvas_commands(self, commands, bounds.x, bounds.y, current_clip);
                }
                DrawCommand::PushClip { bounds, .. } => {
                    let combined = match current_clip {
                        Some(prev) => prev.intersect(bounds.clone()).unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)),
                        None => *bounds,
                    };
                    clip_stack.push(combined);
                }
                DrawCommand::PopClip => {
                    clip_stack.pop();
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

        for cmd in list.commands() {
            if let DrawCommand::PushClip { bounds, .. } = cmd {
                let combined = match clip_stack.last() {
                    Some(prev) => prev.intersect(*bounds).unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)),
                    None => *bounds,
                };
                clip_stack.push(combined);
                continue;
            } else if let DrawCommand::PopClip = cmd {
                clip_stack.pop();
                continue;
            }

            let effective_clip = match clip_stack.last() {
                Some(prev) => match prev.intersect(damage) {
                    Some(c) => c,
                    None => continue,
                },
                None => damage,
            };

            if let Some(cmd_bounds) = cmd.bounds() {
                if cmd_bounds.intersect(effective_clip).is_none() {
                    continue;
                }
            }

            match cmd {
                DrawCommand::DrawRect { bounds, color } => {
                    self.draw_rect(*bounds, *color, Some(effective_clip));
                }
                DrawCommand::DrawRoundedRect {
                    bounds,
                    color,
                    border_radius,
                    border_width,
                    border_color,
                } => {
                    self.draw_rounded_rect(
                        *bounds,
                        *color,
                        border_radius.top_left,
                        border_width.top,
                        *border_color,
                        Some(effective_clip),
                    );
                }
                DrawCommand::DrawShadow { bounds, shadow } => {
                    self.draw_shadow(*bounds, shadow, Some(effective_clip));
                }
                DrawCommand::DrawBorder {
                    bounds,
                    widths,
                    color,
                    border_radius,
                } => {
                    self.draw_rounded_rect(
                        *bounds,
                        Color::TRANSPARENT,
                        border_radius.top_left,
                        widths.top,
                        Some(*color),
                        Some(effective_clip),
                    );
                }
                DrawCommand::DrawText { .. } => {}
                DrawCommand::DrawViewport3D { bounds, scene } => {
                    crate::rasterizer3d::render_scene_3d(self, *bounds, scene, Some(effective_clip));
                }
                DrawCommand::DrawCanvas2D { bounds, commands } => {
                    crate::canvas2d::rasterize_canvas_commands(self, commands, bounds.x, bounds.y, Some(effective_clip));
                }
                DrawCommand::PushClip { .. } | DrawCommand::PopClip => unreachable!(),
            }
        }
    }

    fn compute_clamped_bounds(&self, bounds: Rect, clip: Option<Rect>) -> (usize, usize, usize, usize) {
        let rect = match clip {
            Some(c) => bounds.intersect(c).unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)),
            None => bounds,
        };

        let min_x = (rect.x.floor() as isize).max(0).min(self.width as isize) as usize;
        let max_x = (rect.max_x().ceil() as isize).max(0).min(self.width as isize) as usize;
        let min_y = (rect.y.floor() as isize).max(0).min(self.height as isize) as usize;
        let max_y = (rect.max_y().ceil() as isize).max(0).min(self.height as isize) as usize;

        (min_x, max_x, min_y, max_y)
    }
}
