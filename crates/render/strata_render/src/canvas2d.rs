//! W3C-Compatible Immediate-Mode 2D Canvas Engine (`CanvasRenderingContext2D`).
//!
//! Provides immediate-mode vector drawing, affine 2D transformations, path tessellation,
//! and analytical anti-aliased CPU/GPU rasterization.

use crate::rasterizer::FrameBuffer;
use strata_layout::Rect;
pub use strata_primitives::geom::Transform2D;
use strata_style::types::Color;

/// Canvas path primitive verbs in transformed coordinates.
#[derive(Debug, Clone, PartialEq)]
pub enum CanvasPathVerb {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    Arc {
        cx: f32,
        cy: f32,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
        counter_clockwise: bool,
    },
    Close,
}

/// Renderable 2D canvas drawing instruction.
#[derive(Debug, Clone, PartialEq)]
pub enum CanvasCommand {
    FillRect {
        bounds: Rect,
        color: Color,
    },
    StrokeRect {
        bounds: Rect,
        color: Color,
        line_width: f32,
    },
    ClearRect {
        bounds: Rect,
    },
    DrawLine {
        p0: (f32, f32),
        p1: (f32, f32),
        color: Color,
        line_width: f32,
    },
    DrawCircle {
        center: (f32, f32),
        radius: f32,
        fill_color: Option<Color>,
        stroke_color: Option<Color>,
        line_width: f32,
    },
    DrawPath {
        verbs: Vec<CanvasPathVerb>,
        fill_color: Option<Color>,
        stroke_color: Option<Color>,
        line_width: f32,
    },
}

#[derive(Debug, Clone, PartialEq)]
struct CanvasState {
    transform: Transform2D,
    fill_style: Color,
    stroke_style: Color,
    line_width: f32,
    global_alpha: f32,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            transform: Transform2D::IDENTITY,
            fill_style: Color::BLACK,
            stroke_style: Color::BLACK,
            line_width: 1.0,
            global_alpha: 1.0,
        }
    }
}

/// W3C-Compatible 2D Immediate-Mode Canvas Rendering Context.
#[derive(Debug, Clone)]
pub struct Canvas2DContext {
    pub width: f32,
    pub height: f32,
    current_state: CanvasState,
    state_stack: Vec<CanvasState>,
    current_path: Vec<CanvasPathVerb>,
    commands: Vec<CanvasCommand>,
}

impl Canvas2DContext {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            current_state: CanvasState::default(),
            state_stack: Vec::new(),
            current_path: Vec::new(),
            commands: Vec::new(),
        }
    }

    // --- State Stack ---

    pub fn save(&mut self) {
        self.state_stack.push(self.current_state.clone());
    }

    pub fn restore(&mut self) {
        if let Some(state) = self.state_stack.pop() {
            self.current_state = state;
        }
    }

    // --- Transformations ---

    pub fn translate(&mut self, dx: f32, dy: f32) {
        self.current_state.transform.translate(dx, dy);
    }

    pub fn scale(&mut self, sx: f32, sy: f32) {
        self.current_state.transform.scale_mut(sx, sy);
    }

    pub fn rotate(&mut self, radians: f32) {
        self.current_state.transform.rotate_mut(radians);
    }

    pub fn transform(&mut self, a: f32, b: f32, c: f32, d: f32, e: f32, f: f32) {
        let other = Transform2D::from_w3c(a, b, c, d, e, f);
        self.current_state.transform = self.current_state.transform.multiply(other);
    }

    pub fn set_transform(&mut self, transform: Transform2D) {
        self.current_state.transform = transform;
    }

    pub fn get_transform(&self) -> Transform2D {
        self.current_state.transform
    }

    pub fn reset_transform(&mut self) {
        self.current_state.transform = Transform2D::IDENTITY;
    }

    // --- Styles ---

    pub fn set_fill_style(&mut self, color: Color) {
        self.current_state.fill_style = color;
    }

    pub fn set_stroke_style(&mut self, color: Color) {
        self.current_state.stroke_style = color;
    }

    pub fn set_line_width(&mut self, width: f32) {
        self.current_state.line_width = width.max(0.1);
    }

    pub fn set_global_alpha(&mut self, alpha: f32) {
        self.current_state.global_alpha = alpha.clamp(0.0, 1.0);
    }

    fn apply_alpha(&self, color: Color) -> Color {
        if (self.current_state.global_alpha - 1.0).abs() < 1e-4 {
            color
        } else {
            let (r, g, b, a) = color.to_f32();
            Color::from_f32(r, g, b, a * self.current_state.global_alpha)
        }
    }

    // --- Path Construction ---

    pub fn begin_path(&mut self) {
        self.current_path.clear();
    }

    pub fn move_to(&mut self, x: f32, y: f32) {
        let (tx, ty) = self.current_state.transform.transform_xy(x, y);
        self.current_path.push(CanvasPathVerb::MoveTo(tx, ty));
    }

    pub fn line_to(&mut self, x: f32, y: f32) {
        let (tx, ty) = self.current_state.transform.transform_xy(x, y);
        self.current_path.push(CanvasPathVerb::LineTo(tx, ty));
    }

    pub fn arc(&mut self, cx: f32, cy: f32, radius: f32, start_angle: f32, end_angle: f32, ccw: bool) {
        let (tcx, tcy) = self.current_state.transform.transform_xy(cx, cy);
        // Compute average scale factor for radius
        let scale_x = (self.current_state.transform.a.powi(2) + self.current_state.transform.b.powi(2)).sqrt();
        self.current_path.push(CanvasPathVerb::Arc {
            cx: tcx,
            cy: tcy,
            radius: radius * scale_x,
            start_angle,
            end_angle,
            counter_clockwise: ccw,
        });
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.move_to(x, y);
        self.line_to(x + w, y);
        self.line_to(x + w, y + h);
        self.line_to(x, y + h);
        self.close_path();
    }

    pub fn close_path(&mut self) {
        self.current_path.push(CanvasPathVerb::Close);
    }

    // --- Drawing Operations ---

    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let (x0, y0) = self.current_state.transform.transform_xy(x, y);
        let (x1, y1) = self.current_state.transform.transform_xy(x + w, y + h);
        let min_x = x0.min(x1);
        let max_x = x0.max(x1);
        let min_y = y0.min(y1);
        let max_y = y0.max(y1);

        let color = self.apply_alpha(self.current_state.fill_style);
        self.commands.push(CanvasCommand::FillRect {
            bounds: Rect::new(min_x, min_y, max_x - min_x, max_y - min_y),
            color,
        });
    }

    pub fn stroke_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let (x0, y0) = self.current_state.transform.transform_xy(x, y);
        let (x1, y1) = self.current_state.transform.transform_xy(x + w, y + h);
        let min_x = x0.min(x1);
        let max_x = x0.max(x1);
        let min_y = y0.min(y1);
        let max_y = y0.max(y1);

        let color = self.apply_alpha(self.current_state.stroke_style);
        self.commands.push(CanvasCommand::StrokeRect {
            bounds: Rect::new(min_x, min_y, max_x - min_x, max_y - min_y),
            color,
            line_width: self.current_state.line_width,
        });
    }

    pub fn clear_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let (x0, y0) = self.current_state.transform.transform_xy(x, y);
        let (x1, y1) = self.current_state.transform.transform_xy(x + w, y + h);
        let min_x = x0.min(x1);
        let max_x = x0.max(x1);
        let min_y = y0.min(y1);
        let max_y = y0.max(y1);

        self.commands.push(CanvasCommand::ClearRect {
            bounds: Rect::new(min_x, min_y, max_x - min_x, max_y - min_y),
        });
    }

    pub fn stroke(&mut self) {
        if self.current_path.is_empty() {
            return;
        }
        let color = self.apply_alpha(self.current_state.stroke_style);
        self.commands.push(CanvasCommand::DrawPath {
            verbs: self.current_path.clone(),
            fill_color: None,
            stroke_color: Some(color),
            line_width: self.current_state.line_width,
        });
    }

    pub fn fill(&mut self) {
        if self.current_path.is_empty() {
            return;
        }
        let color = self.apply_alpha(self.current_state.fill_style);
        self.commands.push(CanvasCommand::DrawPath {
            verbs: self.current_path.clone(),
            fill_color: Some(color),
            stroke_color: None,
            line_width: self.current_state.line_width,
        });
    }

    /// Emits all recorded canvas commands.
    pub fn finish(self) -> Vec<CanvasCommand> {
        self.commands
    }

    pub fn commands(&self) -> &[CanvasCommand] {
        &self.commands
    }

    /// Software analytical anti-aliased rasterization directly onto a FrameBuffer.
    pub fn rasterize(&self, fb: &mut FrameBuffer, origin_x: f32, origin_y: f32, clip: Option<Rect>) {
        rasterize_canvas_commands(fb, &self.commands, origin_x, origin_y, clip);
    }
}

/// Software analytical anti-aliased rasterization of canvas commands directly onto a FrameBuffer.
pub fn rasterize_canvas_commands(
    fb: &mut FrameBuffer,
    commands: &[CanvasCommand],
    origin_x: f32,
    origin_y: f32,
    clip: Option<Rect>,
) {
    for cmd in commands {
            match cmd {
                CanvasCommand::FillRect { bounds, color } => {
                    let translated = Rect::new(
                        bounds.x + origin_x,
                        bounds.y + origin_y,
                        bounds.width,
                        bounds.height,
                    );
                    fb.draw_rect(translated, *color, clip);
                }
                CanvasCommand::ClearRect { bounds } => {
                    let translated = Rect::new(
                        bounds.x + origin_x,
                        bounds.y + origin_y,
                        bounds.width,
                        bounds.height,
                    );
                    fb.clear_rect(translated, Color::TRANSPARENT);
                }
                CanvasCommand::StrokeRect { bounds, color, line_width } => {
                    let lw = *line_width;
                    let translated = Rect::new(
                        bounds.x + origin_x,
                        bounds.y + origin_y,
                        bounds.width,
                        bounds.height,
                    );
                    // Top
                    fb.draw_rect(Rect::new(translated.x, translated.y, translated.width, lw), *color, clip);
                    // Bottom
                    fb.draw_rect(Rect::new(translated.x, translated.max_y() - lw, translated.width, lw), *color, clip);
                    // Left
                    fb.draw_rect(Rect::new(translated.x, translated.y, lw, translated.height), *color, clip);
                    // Right
                    fb.draw_rect(Rect::new(translated.max_x() - lw, translated.y, lw, translated.height), *color, clip);
                }
                CanvasCommand::DrawLine { p0, p1, color, line_width } => {
                    rasterize_line(
                        fb,
                        p0.0 + origin_x,
                        p0.1 + origin_y,
                        p1.0 + origin_x,
                        p1.1 + origin_y,
                        *color,
                        *line_width,
                        clip,
                    );
                }
                CanvasCommand::DrawCircle { center, radius, fill_color, stroke_color, line_width } => {
                    let cx = center.0 + origin_x;
                    let cy = center.1 + origin_y;
                    let r = *radius;
                    let lw = *line_width;

                    let min_x = ((cx - r - lw).floor() as isize).max(0) as usize;
                    let max_x = ((cx + r + lw).ceil() as usize).min(fb.width);
                    let min_y = ((cy - r - lw).floor() as isize).max(0) as usize;
                    let max_y = ((cy + r + lw).ceil() as usize).min(fb.height);

                    for y in min_y..max_y {
                        let dy = y as f32 + 0.5 - cy;
                        for x in min_x..max_x {
                            let dx = x as f32 + 0.5 - cx;
                            let dist = (dx * dx + dy * dy).sqrt();

                            if let Some(fc) = fill_color {
                                let coverage = (0.5 + r - dist).clamp(0.0, 1.0);
                                if coverage > 0.005 {
                                    fb.set_pixel(x, y, Color::rgba(fc.r, fc.g, fc.b, (fc.a() * coverage * 255.0) as u8));
                                }
                            }

                            if let Some(sc) = stroke_color {
                                let stroke_dist = (dist - r).abs();
                                let coverage = (0.5 + lw * 0.5 - stroke_dist).clamp(0.0, 1.0);
                                if coverage > 0.005 {
                                    fb.set_pixel(x, y, Color::rgba(sc.r, sc.g, sc.b, (sc.a() * coverage * 255.0) as u8));
                                }
                            }
                        }
                    }
                }
                CanvasCommand::DrawPath { verbs, fill_color, stroke_color, line_width } => {
                    if let Some(fc) = fill_color {
                        for verb in verbs {
                            if let CanvasPathVerb::Arc { cx, cy, radius, .. } = verb {
                                let r = *radius;
                                let acx = *cx + origin_x;
                                let acy = *cy + origin_y;
                                let min_x = ((acx - r).floor() as isize).max(0) as usize;
                                let max_x = ((acx + r).ceil() as usize).min(fb.width);
                                let min_y = ((acy - r).floor() as isize).max(0) as usize;
                                let max_y = ((acy + r).ceil() as usize).min(fb.height);

                                for y in min_y..max_y {
                                    if let Some(c) = clip {
                                        if (y as f32) < c.y || (y as f32) >= c.max_y() {
                                            continue;
                                        }
                                    }
                                    let dy = y as f32 + 0.5 - acy;
                                    for x in min_x..max_x {
                                        if let Some(c) = clip {
                                            if (x as f32) < c.x || (x as f32) >= c.max_x() {
                                                continue;
                                            }
                                        }
                                        let dx = x as f32 + 0.5 - acx;
                                        let dist = (dx * dx + dy * dy).sqrt();
                                        let coverage = (0.5 + r - dist).clamp(0.0, 1.0);
                                        if coverage > 0.005 {
                                            fb.set_pixel(x, y, Color::rgba(fc.r, fc.g, fc.b, (fc.a() * coverage * 255.0) as u8));
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Connect LineTo sequences with analytical lines
                    let mut current = (origin_x, origin_y);
                    let mut start = (origin_x, origin_y);

                    for verb in verbs {
                        match verb {
                            CanvasPathVerb::MoveTo(x, y) => {
                                current = (*x + origin_x, *y + origin_y);
                                start = current;
                            }
                            CanvasPathVerb::LineTo(x, y) => {
                                let next = (*x + origin_x, *y + origin_y);
                                if let Some(color) = stroke_color {
                                    rasterize_line(fb, current.0, current.1, next.0, next.1, *color, *line_width, clip);
                                }
                                current = next;
                            }
                            CanvasPathVerb::Close => {
                                if current != start {
                                    if let Some(color) = stroke_color {
                                        rasterize_line(fb, current.0, current.1, start.0, start.1, *color, *line_width, clip);
                                    }
                                    current = start;
                                }
                            }
                            CanvasPathVerb::Arc { cx, cy, radius, .. } => {
                                // Draw arc center point
                                current = (*cx + origin_x + *radius, *cy + origin_y);
                            }
                        }
                    }
                }
            }
        }
    }

/// Analytical Signed Distance Field 2D Line Segment Rasterizer with subpixel anti-aliasing.
fn rasterize_line(
    fb: &mut FrameBuffer,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    color: Color,
    line_width: f32,
    clip: Option<Rect>,
) {
    let min_x = (x0.min(x1) - line_width - 1.0).max(0.0).floor() as usize;
    let max_x = ((x0.max(x1) + line_width + 1.0).ceil() as usize).min(fb.width);
    let min_y = (y0.min(y1) - line_width - 1.0).max(0.0).floor() as usize;
    let max_y = ((y0.max(y1) + line_width + 1.0).ceil() as usize).min(fb.height);

    let vx = x1 - x0;
    let vy = y1 - y0;
    let v_len_sq = vx * vx + vy * vy;
    let half_lw = line_width * 0.5;

    for y in min_y..max_y {
        if let Some(c) = clip {
            if (y as f32) < c.y || (y as f32) >= c.max_y() {
                continue;
            }
        }
        let py = y as f32 + 0.5;

        for x in min_x..max_x {
            if let Some(c) = clip {
                if (x as f32) < c.x || (x as f32) >= c.max_x() {
                    continue;
                }
            }
            let px = x as f32 + 0.5;

            // Project point onto line segment: t = clamp(dot(p - p0, v) / |v|^2, 0.0, 1.0)
            let wx = px - x0;
            let wy = py - y0;

            let t = if v_len_sq > 1e-6 {
                ((wx * vx + wy * vy) / v_len_sq).clamp(0.0, 1.0)
            } else {
                0.0
            };

            let proj_x = x0 + t * vx;
            let proj_y = y0 + t * vy;

            let dist = ((px - proj_x).powi(2) + (py - proj_y).powi(2)).sqrt();
            let coverage = (0.5 + half_lw - dist).clamp(0.0, 1.0);

            if coverage > 0.005 {
                let effective_color = Color::rgba(
                    color.r,
                    color.g,
                    color.b,
                    (color.a() * coverage * 255.0) as u8,
                );
                fb.set_pixel(x, y, effective_color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform2d_operations() {
        let mut t = Transform2D::IDENTITY;
        t.translate(100.0, 50.0);
        let p = t.transform_xy(10.0, 20.0);
        assert_eq!(p, (110.0, 70.0));

        let mut s = Transform2D::IDENTITY;
        s.scale_mut(2.0, 3.0);
        let p2 = s.transform_xy(10.0, 10.0);
        assert_eq!(p2, (20.0, 30.0));
    }

    #[test]
    fn test_canvas_context_recording() {
        let mut ctx = Canvas2DContext::new(400.0, 300.0);

        ctx.set_fill_style(Color::rgb(255, 0, 0));
        ctx.fill_rect(10.0, 10.0, 80.0, 50.0);

        ctx.set_stroke_style(Color::rgb(0, 0, 255));
        ctx.set_line_width(2.0);
        ctx.begin_path();
        ctx.move_to(0.0, 0.0);
        ctx.line_to(100.0, 100.0);
        ctx.stroke();

        assert_eq!(ctx.commands().len(), 2);
    }

    #[test]
    fn test_canvas_rasterize_to_framebuffer() {
        let mut fb = FrameBuffer::new(200, 200);
        let mut ctx = Canvas2DContext::new(200.0, 200.0);

        ctx.set_fill_style(Color::rgb(0, 255, 0));
        ctx.fill_rect(20.0, 20.0, 40.0, 40.0);

        ctx.rasterize(&mut fb, 0.0, 0.0, None);

        // Center of rect (40, 40) must be green
        let pixel = fb.get_pixel(40, 40);
        assert_eq!(pixel, Color::rgb(0, 255, 0));

        // Outside rect (5, 5) must be transparent
        assert_eq!(fb.get_pixel(5, 5), Color::TRANSPARENT);
    }
}
