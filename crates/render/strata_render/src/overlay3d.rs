//! 2D overlay pass drawn over depth-tested 3D geometry.
//!
//! The Earth reference draws graticules, contours, trajectories, waypoints,
//! reticles, and labels as 2D primitives after the 3D pass. They share the 3D
//! camera projection, so a projected anchor lands on the same pixel as the mesh
//! vertex it annotates, and everything is clipped to the viewport.
//!
//! The pass is split in two on purpose:
//!
//! 1. Projection: scene-space anchors are projected with the f64 frame and
//!    reduced to screen-space commands. This step is deterministic and pure, so
//!    tests can assert anchor accuracy without reading pixels.
//! 2. Rasterization: the commands are drawn into a [`FrameBuffer`].

use crate::camera64::{DVec3, RenderFrame64};
use crate::font::get_glyph_or_fallback;
use crate::rasterizer::FrameBuffer;
use strata_layout::Rect;
use strata_style::types::Color;

/// A marker glyph drawn at a projected anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerKind {
    /// Filled square, the default waypoint glyph.
    Square,
    /// Filled circle.
    Circle,
    /// Diamond.
    Diamond,
    /// Hollow square: selected or focused entity.
    Ring,
}

/// Placement of a label relative to its projected anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelAnchor {
    /// Box centred on the anchor.
    Centre,
    /// Box starts at the anchor.
    Left,
    /// Box ends at the anchor.
    Right,
    /// Box sits above the anchor with a short gap.
    Above,
}

/// Text styling for an overlay label.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverlayTextStyle {
    pub color: Color,
    /// Integer scale of the 8x16 glyph cell.
    pub scale: u32,
}

impl Default for OverlayTextStyle {
    fn default() -> Self {
        Self {
            color: Color::rgba(241, 245, 249, 255),
            scale: 1,
        }
    }
}

/// A scene-space overlay primitive.
#[derive(Debug, Clone, PartialEq)]
pub enum OverlayPrimitive {
    /// Connected line segments through scene points.
    Polyline {
        points: Vec<DVec3>,
        color: Color,
        width: f32,
    },
    /// Circle of constant world radius around a scene-space centre.
    Circle {
        centre: DVec3,
        radius: f64,
        color: Color,
        width: f32,
    },
    /// Filled marker at a scene anchor.
    Marker {
        anchor: DVec3,
        kind: MarkerKind,
        color: Color,
        size: f32,
    },
    /// Text label at a scene anchor, optionally with a leader line back to the
    /// anchor.
    Label {
        anchor: DVec3,
        text: String,
        style: OverlayTextStyle,
        placement: LabelAnchor,
        leader: bool,
    },
    /// Screen-space line, for reticles and crosshairs.
    Line {
        a: [f32; 2],
        b: [f32; 2],
        color: Color,
        width: f32,
    },
}

/// A projected, screen-space overlay command.
#[derive(Debug, Clone, PartialEq)]
pub enum OverlayCommand {
    Polyline {
        points: Vec<[f32; 2]>,
        color: Color,
        width: f32,
    },
    Circle {
        centre: [f32; 2],
        radius: f32,
        color: Color,
        width: f32,
    },
    Marker {
        centre: [f32; 2],
        kind: MarkerKind,
        color: Color,
        size: f32,
    },
    Label {
        text: String,
        /// Top-left of the text box in screen space.
        origin: [f32; 2],
        style: OverlayTextStyle,
        /// Leader from the label box edge to the anchor, when requested.
        leader: Option<([f32; 2], [f32; 2])>,
    },
    Line {
        a: [f32; 2],
        b: [f32; 2],
        color: Color,
        width: f32,
    },
}

/// Overlay primitives plus an optional screen-space clip rectangle.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Overlay3D {
    pub primitives: Vec<OverlayPrimitive>,
    /// Clip in screen space. `None` clips to the draw viewport.
    pub clip: Option<Rect>,
}

impl Overlay3D {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, primitive: OverlayPrimitive) -> &mut Self {
        self.primitives.push(primitive);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.primitives.is_empty()
    }

    /// Projects every primitive into screen space with the shared 3D frame.
    ///
    /// Pieces behind the near plane are dropped, matching the geometry pass.
    pub fn project(&self, frame: &RenderFrame64) -> Vec<OverlayCommand> {
        let mut out = Vec::with_capacity(self.primitives.len());
        for primitive in &self.primitives {
            match primitive {
                OverlayPrimitive::Polyline {
                    points,
                    color,
                    width,
                } => {
                    let mut projected = Vec::with_capacity(points.len());
                    let mut visible = true;
                    for point in points {
                        let p = frame.project(*point);
                        if p.clipped {
                            visible = false;
                            break;
                        }
                        projected.push([p.x as f32, p.y as f32]);
                    }
                    if visible && projected.len() >= 2 {
                        out.push(OverlayCommand::Polyline {
                            points: projected,
                            color: *color,
                            width: *width,
                        });
                    }
                }
                OverlayPrimitive::Circle {
                    centre,
                    radius,
                    color,
                    width,
                } => {
                    let p = frame.project(*centre);
                    if p.clipped {
                        continue;
                    }
                    // Radius is projected through a second point one radius
                    // away along a screen-parallel axis so the circle shrinks
                    // with distance exactly like the geometry does.
                    let edge = frame.project(centre.add(DVec3::new(*radius, 0.0, 0.0)));
                    let screen_radius = if edge.clipped {
                        0.0
                    } else {
                        ((edge.x - p.x).hypot(edge.y - p.y)) as f32
                    };
                    out.push(OverlayCommand::Circle {
                        centre: [p.x as f32, p.y as f32],
                        radius: screen_radius,
                        color: *color,
                        width: *width,
                    });
                }
                OverlayPrimitive::Marker {
                    anchor,
                    kind,
                    color,
                    size,
                } => {
                    let p = frame.project(*anchor);
                    if p.clipped {
                        continue;
                    }
                    out.push(OverlayCommand::Marker {
                        centre: [p.x as f32, p.y as f32],
                        kind: *kind,
                        color: *color,
                        size: *size,
                    });
                }
                OverlayPrimitive::Label {
                    anchor,
                    text,
                    style,
                    placement,
                    leader,
                } => {
                    let p = frame.project(*anchor);
                    if p.clipped {
                        continue;
                    }
                    let (w, h) = text_size(text, style.scale);
                    let ax = p.x as f32;
                    let ay = p.y as f32;
                    let (ox, oy) = match placement {
                        LabelAnchor::Centre => (ax - w * 0.5, ay - h * 0.5),
                        LabelAnchor::Left => (ax, ay - h * 0.5),
                        LabelAnchor::Right => (ax - w, ay - h * 0.5),
                        LabelAnchor::Above => (ax - w * 0.5, ay - h - 6.0),
                    };
                    let leader_info = if *leader {
                        Some(([ax, ay], [ax, oy + h]))
                    } else {
                        None
                    };
                    out.push(OverlayCommand::Label {
                        text: text.clone(),
                        origin: [ox, oy],
                        style: *style,
                        leader: leader_info,
                    });
                }
                OverlayPrimitive::Line { a, b, color, width } => out.push(OverlayCommand::Line {
                    a: *a,
                    b: *b,
                    color: *color,
                    width: *width,
                }),
            }
        }
        out
    }

    /// Projects and rasterizes this overlay over already-rendered geometry,
    /// including text labels.
    pub fn render(&self, frame_buffer: &mut FrameBuffer, viewport: Rect, frame: &RenderFrame64) {
        self.render_with(frame_buffer, viewport, frame, draw_text);
    }

    /// Projects and rasterizes with an explicit text rasterizer.
    pub fn render_with(
        &self,
        frame_buffer: &mut FrameBuffer,
        viewport: Rect,
        frame: &RenderFrame64,
        text: TextRasterizer,
    ) {
        let commands = self.project(frame);
        rasterize_overlay_with_text(frame_buffer, viewport, self.clip, &commands, text);
    }

    /// Projects and rasterizes everything except text, for callers that have no
    /// text backend installed.
    pub fn render_geometry_only(
        &self,
        frame_buffer: &mut FrameBuffer,
        viewport: Rect,
        frame: &RenderFrame64,
    ) {
        let commands = self.project(frame);
        rasterize_overlay(frame_buffer, viewport, self.clip, &commands);
    }
}

/// Screen size of a text run in the 8x16 glyph cell.
pub fn text_size(text: &str, scale: u32) -> (f32, f32) {
    let scale = scale.max(1) as f32;
    let chars = text.chars().count() as f32;
    (chars * 8.0 * scale, 16.0 * scale)
}

/// Signature of a text rasterizer installed by the platform layer.
pub type TextRasterizer = fn(&mut FrameBuffer, Rect, [f32; 2], &str, OverlayTextStyle);

/// Rasterizes projected overlay commands into `frame_buffer`, clipped to
/// `clip` when present and to `viewport` always. Labels are drawn with the
/// built-in glyph rasterizer.
pub fn rasterize_overlay(
    frame_buffer: &mut FrameBuffer,
    viewport: Rect,
    clip: Option<Rect>,
    commands: &[OverlayCommand],
) {
    rasterize_overlay_with_text(frame_buffer, viewport, clip, commands, draw_text);
}

/// Rasterizes projected overlay commands with an explicit text rasterizer.
pub fn rasterize_overlay_with_text(
    frame_buffer: &mut FrameBuffer,
    viewport: Rect,
    clip: Option<Rect>,
    commands: &[OverlayCommand],
    text: TextRasterizer,
) {
    let Some(region) = viewport.intersect(clip.unwrap_or(viewport)) else {
        return;
    };
    if region.width <= 0.0 || region.height <= 0.0 {
        return;
    }

    for command in commands {
        match command {
            OverlayCommand::Polyline {
                points,
                color,
                width,
            } => {
                for pair in points.windows(2) {
                    draw_line(frame_buffer, region, pair[0], pair[1], *color, *width);
                }
            }
            OverlayCommand::Circle {
                centre,
                radius,
                color,
                width,
            } => draw_circle(frame_buffer, region, *centre, *radius, *color, *width),
            OverlayCommand::Marker {
                centre,
                kind,
                color,
                size,
            } => draw_marker(frame_buffer, region, *centre, *kind, *color, *size),
            OverlayCommand::Label {
                text: text_run,
                origin,
                style,
                leader,
            } => {
                if let Some((from, to)) = leader {
                    draw_line(frame_buffer, region, *from, *to, style.color, 1.0);
                }
                text(frame_buffer, region, *origin, text_run, *style);
            }
            OverlayCommand::Line { a, b, color, width } => {
                draw_line(frame_buffer, region, *a, *b, *color, *width)
            }
        }
    }
}

#[inline]
fn plot(frame_buffer: &mut FrameBuffer, region: Rect, x: i32, y: i32, color: Color) {
    let fx = x as f32 + 0.5;
    let fy = y as f32 + 0.5;
    if fx < region.x || fy < region.y || fx >= region.max_x() || fy >= region.max_y() {
        return;
    }
    if x < 0 || y < 0 {
        return;
    }
    frame_buffer.set_pixel(x as usize, y as usize, color);
}

/// Clips a segment to a rectangle with Liang-Barsky.
///
/// Projected globe-scale polylines can have endpoints millions of pixels
/// outside the frame. Clipping first keeps the Bresenham loop proportional to
/// the visible part instead of the full off-screen span.
fn clip_segment_to_rect(a: [f32; 2], b: [f32; 2], region: Rect) -> Option<([f32; 2], [f32; 2])> {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let p = [-dx, dx, -dy, dy];
    let q = [
        a[0] - region.x,
        region.max_x() - a[0],
        a[1] - region.y,
        region.max_y() - a[1],
    ];
    let mut t0 = 0.0f32;
    let mut t1 = 1.0f32;
    for index in 0..4 {
        if p[index].abs() < 1e-9 {
            if q[index] < 0.0 {
                return None;
            }
            continue;
        }
        let t = q[index] / p[index];
        if p[index] < 0.0 {
            t0 = t0.max(t);
        } else {
            t1 = t1.min(t);
        }
        if t0 > t1 {
            return None;
        }
    }
    Some((
        [a[0] + dx * t0, a[1] + dy * t0],
        [a[0] + dx * t1, a[1] + dy * t1],
    ))
}

/// Bresenham line with a square brush so widths above one pixel are visible.
fn draw_line(
    frame_buffer: &mut FrameBuffer,
    region: Rect,
    a: [f32; 2],
    b: [f32; 2],
    color: Color,
    width: f32,
) {
    let Some((a, b)) = clip_segment_to_rect(a, b, region) else {
        return;
    };
    let half = ((width.max(1.0) / 2.0).round() as i32).max(0);
    let (mut x0, mut y0) = (a[0].round() as i32, a[1].round() as i32);
    let (x1, y1) = (b[0].round() as i32, b[1].round() as i32);
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        for oy in -half..=half {
            for ox in -half..=half {
                plot(frame_buffer, region, x0 + ox, y0 + oy, color);
            }
        }
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

fn draw_circle(
    frame_buffer: &mut FrameBuffer,
    region: Rect,
    centre: [f32; 2],
    radius: f32,
    color: Color,
    width: f32,
) {
    if radius <= 0.5 {
        return;
    }
    let steps = ((radius * 6.0).ceil() as i32).max(16);
    let mut prev = [centre[0] + radius, centre[1]];
    for step in 1..=steps {
        let angle = step as f32 / steps as f32 * std::f32::consts::TAU;
        let next = [
            centre[0] + radius * angle.cos(),
            centre[1] + radius * angle.sin(),
        ];
        draw_line(frame_buffer, region, prev, next, color, width);
        prev = next;
    }
}

fn draw_marker(
    frame_buffer: &mut FrameBuffer,
    region: Rect,
    centre: [f32; 2],
    kind: MarkerKind,
    color: Color,
    size: f32,
) {
    let half = (size * 0.5).max(1.0);
    let cx = centre[0].round() as i32;
    let cy = centre[1].round() as i32;
    let h = half.round() as i32;
    match kind {
        MarkerKind::Square => {
            for y in -h..=h {
                for x in -h..=h {
                    plot(frame_buffer, region, cx + x, cy + y, color);
                }
            }
        }
        MarkerKind::Ring => {
            for y in -h..=h {
                for x in -h..=h {
                    let edge = x.abs() == h || y.abs() == h;
                    if edge {
                        plot(frame_buffer, region, cx + x, cy + y, color);
                    }
                }
            }
        }
        MarkerKind::Circle => {
            for y in -h..=h {
                for x in -h..=h {
                    if x * x + y * y <= h * h {
                        plot(frame_buffer, region, cx + x, cy + y, color);
                    }
                }
            }
        }
        MarkerKind::Diamond => {
            for y in -h..=h {
                for x in -h..=h {
                    if x.abs() + y.abs() <= h {
                        plot(frame_buffer, region, cx + x, cy + y, color);
                    }
                }
            }
        }
    }
}

/// Draws text with the built-in 8x16 cell font, scaled by integer steps.
pub fn draw_text(
    frame_buffer: &mut FrameBuffer,
    region: Rect,
    origin: [f32; 2],
    text: &str,
    style: OverlayTextStyle,
) {
    let scale = style.scale.max(1) as i32;
    let mut cursor_x = origin[0].round() as i32;
    let cursor_y = origin[1].round() as i32;
    for ch in text.chars() {
        let glyph = get_glyph_or_fallback(ch);
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..8 {
                if bits & (0x80 >> col) == 0 {
                    continue;
                }
                for sy in 0..scale {
                    for sx in 0..scale {
                        plot(
                            frame_buffer,
                            region,
                            cursor_x + col * scale + sx,
                            cursor_y + row as i32 * scale + sy,
                            style.color,
                        );
                    }
                }
            }
        }
        cursor_x += 8 * scale;
    }
}
