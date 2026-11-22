//! W3C-Compatible Immediate-Mode 2D Canvas Engine (`CanvasRenderingContext2D`).
//!
//! Provides immediate-mode vector drawing, affine 2D transformations, cubic & quadratic Bézier
//! curves, dashed strokes, path clipping, text rendering, and analytical anti-aliased CPU/GPU
//! rasterization.

use crate::rasterizer::FrameBuffer;
use strata_layout::Rect;
pub use strata_primitives::geom::Transform2D;
use strata_style::types::Color;

/// Line termination cap style for [`Canvas2DContext::set_line_cap`].
pub type CanvasLineCap = strata_layout::StrokeLineCap;

/// Line segment junction style for [`Canvas2DContext::set_line_join`].
pub type CanvasLineJoin = strata_layout::StrokeLineJoin;

/// Measured text metrics returned by [`Canvas2DContext::measure_text`], modeled after HTML5
/// `TextMetrics`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CanvasTextMetrics {
    /// Width of the inline text advance box in CSS pixels.
    pub width: f32,
    /// Total height of the measured text line in CSS pixels.
    pub height: f32,
    /// Distance from the baseline up to the bounding box top.
    pub actual_bounding_box_ascent: f32,
    /// Distance from the baseline down to the bounding box bottom.
    pub actual_bounding_box_descent: f32,
    /// Font-wide ascent above the baseline.
    pub font_bounding_box_ascent: f32,
    /// Font-wide descent below the baseline.
    pub font_bounding_box_descent: f32,
}

/// Polymorphic corner radii input for [`Canvas2DContext::round_rect`], accepting a uniform scalar
/// `f32`, 1–4 element CSS/HTML5 arrays/slices (`[tl, tr, br, bl]`), or
/// [`strata_style::types::BorderRadius`].
pub trait IntoRoundRectRadii {
    fn into_round_rect_radii(self) -> [f32; 4];
}

impl IntoRoundRectRadii for f32 {
    #[inline]
    fn into_round_rect_radii(self) -> [f32; 4] {
        [self, self, self, self]
    }
}

impl IntoRoundRectRadii for [f32; 1] {
    #[inline]
    fn into_round_rect_radii(self) -> [f32; 4] {
        [self[0], self[0], self[0], self[0]]
    }
}

impl IntoRoundRectRadii for [f32; 2] {
    #[inline]
    fn into_round_rect_radii(self) -> [f32; 4] {
        [self[0], self[1], self[0], self[1]]
    }
}

impl IntoRoundRectRadii for [f32; 3] {
    #[inline]
    fn into_round_rect_radii(self) -> [f32; 4] {
        [self[0], self[1], self[2], self[1]]
    }
}

impl IntoRoundRectRadii for [f32; 4] {
    #[inline]
    fn into_round_rect_radii(self) -> [f32; 4] {
        self
    }
}

impl IntoRoundRectRadii for &[f32] {
    #[inline]
    fn into_round_rect_radii(self) -> [f32; 4] {
        match self {
            [] => [0.0; 4],
            [a] => [*a, *a, *a, *a],
            [a, b] => [*a, *b, *a, *b],
            [a, b, c] => [*a, *b, *c, *b],
            [a, b, c, d, ..] => [*a, *b, *c, *d],
        }
    }
}

impl IntoRoundRectRadii for strata_style::types::BorderRadius {
    #[inline]
    fn into_round_rect_radii(self) -> [f32; 4] {
        [
            self.top_left,
            self.top_right,
            self.bottom_right,
            self.bottom_left,
        ]
    }
}

/// Horizontal text alignment for [`Canvas2DContext::fill_text`] and [`Canvas2DContext::stroke_text`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CanvasTextAlign {
    #[default]
    Left,
    Center,
    Right,
    Start,
    End,
}

/// Vertical text baseline for [`Canvas2DContext::fill_text`] and [`Canvas2DContext::stroke_text`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CanvasTextBaseline {
    #[default]
    Alphabetic,
    Top,
    Middle,
    Bottom,
    Hanging,
    Ideographic,
}

/// Canvas path primitive verbs in transformed coordinates.
#[derive(Debug, Clone, PartialEq)]
pub enum CanvasPathVerb {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    QuadraticCurveTo {
        cpx: f32,
        cpy: f32,
        x: f32,
        y: f32,
    },
    BezierCurveTo {
        cp1x: f32,
        cp1y: f32,
        cp2x: f32,
        cp2y: f32,
        x: f32,
        y: f32,
    },
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
    /// Pushes the active backend graphics state (clip region, dash state, line cap/join).
    Save,
    /// Pops the most recently pushed backend graphics state.
    Restore,
    /// Sets the stroke dash pattern (empty clears dashing) and phase offset.
    SetLineDash {
        segments: Vec<f32>,
        offset: f32,
    },
    /// Sets the stroke line termination cap style (`Butt`, `Round`, `Square`).
    SetLineCap(CanvasLineCap),
    /// Sets the stroke corner join style (`Miter`, `Round`, `Bevel`).
    SetLineJoin(CanvasLineJoin),
    /// Intersects the active clip region with the given path until the matching `Restore`.
    ClipPath {
        verbs: Vec<CanvasPathVerb>,
    },
    /// Renders filled text at `(x, y)` in transformed canvas coordinates.
    FillText {
        text: String,
        x: f32,
        y: f32,
        color: Color,
        font: String,
        font_size: f32,
        font_family: String,
        font_weight: u16,
        italic: bool,
        text_align: CanvasTextAlign,
        text_baseline: CanvasTextBaseline,
        max_width: Option<f32>,
    },
    /// Renders outlined/stroked text at `(x, y)` in transformed canvas coordinates.
    StrokeText {
        text: String,
        x: f32,
        y: f32,
        color: Color,
        line_width: f32,
        font: String,
        font_size: f32,
        font_family: String,
        font_weight: u16,
        italic: bool,
        text_align: CanvasTextAlign,
        text_baseline: CanvasTextBaseline,
        max_width: Option<f32>,
    },
}

/// Parses a CSS/HTML5 Canvas `font` shorthand string (e.g. `"bold 12px monospace"`,
/// `"italic 600 11px/1.4 'JetBrains Mono', monospace"`, `"10pt sans-serif"`) into
/// `(font_size_px, font_family, font_weight, italic)`.
pub fn parse_canvas_font(font: &str) -> (f32, String, u16, bool) {
    let mut font_size = 12.0_f32;
    let mut font_weight = 400_u16;
    let mut italic = false;
    let mut family_start: Option<usize> = None;

    let bytes = font.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let token = &font[start..i];
        let lower = token.to_ascii_lowercase();

        if lower == "italic" || lower == "oblique" {
            italic = true;
            continue;
        }
        if lower == "bold" || lower == "bolder" {
            font_weight = 700;
            continue;
        }
        if lower == "lighter" {
            font_weight = 300;
            continue;
        }
        if lower == "normal" || lower == "small-caps" {
            continue;
        }
        if let Ok(w) = lower.parse::<u16>() {
            if (100..=900).contains(&w) {
                font_weight = w;
                continue;
            }
        }

        // Size token: `12px`, `12.5px/1.4`, `10pt`, `1rem`, `1em`
        let size_part = lower.split('/').next().unwrap_or("");
        let parsed_px = if let Some(num) = size_part.strip_suffix("px") {
            num.parse::<f32>().ok()
        } else if let Some(num) = size_part.strip_suffix("pt") {
            num.parse::<f32>().ok().map(|pt| pt * (4.0 / 3.0))
        } else if let Some(num) = size_part.strip_suffix("rem") {
            num.parse::<f32>().ok().map(|r| r * 16.0)
        } else if let Some(num) = size_part.strip_suffix("em") {
            num.parse::<f32>().ok().map(|e| e * 16.0)
        } else {
            None
        };

        if let Some(px) = parsed_px {
            if px.is_finite() && px > 0.0 {
                font_size = px;
                family_start = Some(i);
                break;
            }
        }
    }

    let raw_family = family_start.map(|idx| font[idx..].trim()).unwrap_or("");
    let cleaned_family = raw_family
        .split(',')
        .map(|s| s.trim().trim_matches('\'').trim_matches('"').trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ");

    let font_family = if cleaned_family.is_empty() {
        "sans-serif".to_string()
    } else {
        cleaned_family
    };

    (font_size, font_family, font_weight, italic)
}

#[derive(Debug, Clone, PartialEq)]
struct CanvasState {
    transform: Transform2D,
    fill_style: Color,
    stroke_style: Color,
    line_width: f32,
    line_cap: CanvasLineCap,
    line_join: CanvasLineJoin,
    global_alpha: f32,
    line_dash: Vec<f32>,
    line_dash_offset: f32,
    font: String,
    font_size: f32,
    font_family: String,
    font_weight: u16,
    font_italic: bool,
    text_align: CanvasTextAlign,
    text_baseline: CanvasTextBaseline,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            transform: Transform2D::IDENTITY,
            fill_style: Color::BLACK,
            stroke_style: Color::BLACK,
            line_width: 1.0,
            line_cap: CanvasLineCap::Butt,
            line_join: CanvasLineJoin::Miter,
            global_alpha: 1.0,
            line_dash: Vec::new(),
            line_dash_offset: 0.0,
            font: "12px sans-serif".to_string(),
            font_size: 12.0,
            font_family: "sans-serif".to_string(),
            font_weight: 400,
            font_italic: false,
            text_align: CanvasTextAlign::Left,
            text_baseline: CanvasTextBaseline::Alphabetic,
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
        self.commands.push(CanvasCommand::Save);
    }

    pub fn restore(&mut self) {
        if let Some(state) = self.state_stack.pop() {
            self.current_state = state;
            self.commands.push(CanvasCommand::Restore);
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

    /// Sets the stroke line termination cap style (`Butt`, `Round`, or `Square`).
    pub fn set_line_cap(&mut self, cap: CanvasLineCap) {
        self.current_state.line_cap = cap;
        self.commands.push(CanvasCommand::SetLineCap(cap));
    }

    /// Returns the currently active stroke line cap style.
    pub fn line_cap(&self) -> CanvasLineCap {
        self.current_state.line_cap
    }

    /// Sets the stroke corner join style (`Miter`, `Round`, or `Bevel`).
    pub fn set_line_join(&mut self, join: CanvasLineJoin) {
        self.current_state.line_join = join;
        self.commands.push(CanvasCommand::SetLineJoin(join));
    }

    /// Returns the currently active stroke corner join style.
    pub fn line_join(&self) -> CanvasLineJoin {
        self.current_state.line_join
    }

    pub fn set_global_alpha(&mut self, alpha: f32) {
        self.current_state.global_alpha = alpha.clamp(0.0, 1.0);
    }

    /// Sets the stroke dash pattern (HTML5 `ctx.setLineDash(segments)`).
    ///
    /// Negative or non-finite values cause the call to be ignored per the HTML5 Canvas 2D spec;
    /// an odd-length pattern is automatically concatenated with itself so dash/gap pairs repeat
    /// cleanly; an empty slice restores solid strokes.
    pub fn set_line_dash(&mut self, segments: &[f32]) {
        if segments.iter().any(|&v| !v.is_finite() || v < 0.0) {
            return;
        }
        let mut normalized = Vec::with_capacity(if segments.len() % 2 == 1 {
            segments.len() * 2
        } else {
            segments.len()
        });
        normalized.extend_from_slice(segments);
        if segments.len() % 2 == 1 {
            normalized.extend_from_slice(segments);
        }
        self.current_state.line_dash = normalized.clone();
        self.commands.push(CanvasCommand::SetLineDash {
            segments: normalized,
            offset: self.current_state.line_dash_offset,
        });
    }

    /// Returns the currently active stroke dash pattern.
    pub fn get_line_dash(&self) -> &[f32] {
        &self.current_state.line_dash
    }

    /// Sets the stroke dash phase offset (`ctx.lineDashOffset`).
    pub fn set_line_dash_offset(&mut self, offset: f32) {
        if !offset.is_finite() {
            return;
        }
        self.current_state.line_dash_offset = offset;
        self.commands.push(CanvasCommand::SetLineDash {
            segments: self.current_state.line_dash.clone(),
            offset,
        });
    }

    /// Returns the current stroke dash phase offset.
    pub fn line_dash_offset(&self) -> f32 {
        self.current_state.line_dash_offset
    }

    /// Sets the active CSS font shorthand (e.g. `"bold 12px monospace"`).
    pub fn set_font(&mut self, font: impl Into<String>) {
        let raw = font.into();
        let (size, family, weight, italic) = parse_canvas_font(&raw);
        self.current_state.font = raw;
        self.current_state.font_size = size;
        self.current_state.font_family = family;
        self.current_state.font_weight = weight;
        self.current_state.font_italic = italic;
    }

    /// Returns the active CSS font string.
    pub fn font(&self) -> &str {
        &self.current_state.font
    }

    /// Sets horizontal text alignment (`Left`, `Center`, `Right`, `Start`, `End`).
    pub fn set_text_align(&mut self, align: CanvasTextAlign) {
        self.current_state.text_align = align;
    }

    /// Sets vertical text baseline (`Alphabetic`, `Top`, `Middle`, `Bottom`, `Hanging`, `Ideographic`).
    pub fn set_text_baseline(&mut self, baseline: CanvasTextBaseline) {
        self.current_state.text_baseline = baseline;
    }

    /// Measures `text` using the active [`Self::set_font`] styling and registered platform
    /// text measurer (PangoCairo when available, OpenType/inline shaping fallback headlessly).
    pub fn measure_text(&self, text: &str) -> CanvasTextMetrics {
        measure_canvas_text_metrics(
            text,
            self.current_state.font_size,
            &self.current_state.font_family,
            self.current_state.font_weight,
            self.current_state.font_italic,
        )
    }

    fn apply_alpha(&self, color: Color) -> Color {
        if (self.current_state.global_alpha - 1.0).abs() < 1e-4 {
            color
        } else {
            let (r, g, b, a) = color.to_f32();
            Color::from_f32(r, g, b, a * self.current_state.global_alpha)
        }
    }

    fn transform_scale(&self) -> f32 {
        (self.current_state.transform.a.powi(2) + self.current_state.transform.b.powi(2)).sqrt()
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

    /// Appends a quadratic Bézier curve from the current point to `(x, y)` using control point `(cpx, cpy)`.
    pub fn quadratic_curve_to(&mut self, cpx: f32, cpy: f32, x: f32, y: f32) {
        let (tcpx, tcpy) = self.current_state.transform.transform_xy(cpx, cpy);
        let (tx, ty) = self.current_state.transform.transform_xy(x, y);
        self.current_path.push(CanvasPathVerb::QuadraticCurveTo {
            cpx: tcpx,
            cpy: tcpy,
            x: tx,
            y: ty,
        });
    }

    /// Appends a cubic Bézier curve from the current point to `(x, y)` using control points
    /// `(cp1x, cp1y)` and `(cp2x, cp2y)`.
    pub fn bezier_curve_to(
        &mut self,
        cp1x: f32,
        cp1y: f32,
        cp2x: f32,
        cp2y: f32,
        x: f32,
        y: f32,
    ) {
        let (tcp1x, tcp1y) = self.current_state.transform.transform_xy(cp1x, cp1y);
        let (tcp2x, tcp2y) = self.current_state.transform.transform_xy(cp2x, cp2y);
        let (tx, ty) = self.current_state.transform.transform_xy(x, y);
        self.current_path.push(CanvasPathVerb::BezierCurveTo {
            cp1x: tcp1x,
            cp1y: tcp1y,
            cp2x: tcp2x,
            cp2y: tcp2y,
            x: tx,
            y: ty,
        });
    }

    pub fn arc(
        &mut self,
        cx: f32,
        cy: f32,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
        ccw: bool,
    ) {
        let (tcx, tcy) = self.current_state.transform.transform_xy(cx, cy);
        let scale_x = self.transform_scale();
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

    /// Adds a rounded rectangle subpath `(x, y, w, h)` with corner `radii` (uniform `f32`,
    /// 1–4 element slice/array `[tl, tr, br, bl]`, or `BorderRadius`) per the HTML5 Canvas 2D
    /// `roundRect` specification.
    pub fn round_rect(&mut self, x: f32, y: f32, w: f32, h: f32, radii: impl IntoRoundRectRadii) {
        let mut r = radii.into_round_rect_radii();
        for v in &mut r {
            *v = if v.is_finite() { (*v).max(0.0) } else { 0.0 };
        }
        let aw = w.abs();
        let ah = h.abs();
        if aw <= 1e-6 || ah <= 1e-6 {
            self.rect(x, y, w, h);
            return;
        }
        let mut scale = 1.0_f32;
        let top = r[0] + r[1];
        let right = r[1] + r[2];
        let bottom = r[2] + r[3];
        let left = r[3] + r[0];
        if top > aw {
            scale = scale.min(aw / top);
        }
        if bottom > aw {
            scale = scale.min(aw / bottom);
        }
        if left > ah {
            scale = scale.min(ah / left);
        }
        if right > ah {
            scale = scale.min(ah / right);
        }
        let [tl, tr, br, bl] = [r[0] * scale, r[1] * scale, r[2] * scale, r[3] * scale];

        let (x0, x1) = if w >= 0.0 { (x, x + w) } else { (x + w, x) };
        let (y0, y1) = if h >= 0.0 { (y, y + h) } else { (y + h, y) };
        const KAPPA: f32 = 0.552_284_8;

        self.move_to(x0 + tl, y0);
        self.line_to(x1 - tr, y0);
        if tr > 0.0 {
            let k = tr * KAPPA;
            self.bezier_curve_to(x1 - tr + k, y0, x1, y0 + tr - k, x1, y0 + tr);
        }
        self.line_to(x1, y1 - br);
        if br > 0.0 {
            let k = br * KAPPA;
            self.bezier_curve_to(x1, y1 - br + k, x1 - br + k, y1, x1 - br, y1);
        }
        self.line_to(x0 + bl, y1);
        if bl > 0.0 {
            let k = bl * KAPPA;
            self.bezier_curve_to(x0 + bl - k, y1, x0, y1 - bl + k, x0, y1 - bl);
        }
        self.line_to(x0, y0 + tl);
        if tl > 0.0 {
            let k = tl * KAPPA;
            self.bezier_curve_to(x0, y0 + tl - k, x0 + tl - k, y0, x0 + tl, y0);
        }
        self.close_path();
    }

    pub fn close_path(&mut self) {
        self.current_path.push(CanvasPathVerb::Close);
    }

    /// Intersects the current clipping region with the current path.
    ///
    /// Wrap with [`Self::save`] and [`Self::restore`] to scope the clip region.
    pub fn clip(&mut self) {
        if self.current_path.is_empty() {
            return;
        }
        self.commands.push(CanvasCommand::ClipPath {
            verbs: self.current_path.clone(),
        });
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

    /// Renders filled text at `(x, y)` using the active `font`, `fill_style`, `text_align`, and `text_baseline`.
    pub fn fill_text(&mut self, text: impl Into<String>, x: f32, y: f32) {
        self.push_fill_text(text.into(), x, y, None);
    }

    /// Renders filled text at `(x, y)`, horizontally constrained to `max_width` pixels.
    pub fn fill_text_with_max_width(
        &mut self,
        text: impl Into<String>,
        x: f32,
        y: f32,
        max_width: f32,
    ) {
        self.push_fill_text(text.into(), x, y, Some(max_width.max(1.0)));
    }

    fn push_fill_text(&mut self, text: String, x: f32, y: f32, max_width: Option<f32>) {
        if text.is_empty() {
            return;
        }
        let (tx, ty) = self.current_state.transform.transform_xy(x, y);
        let scale = self.transform_scale().max(1e-3);
        let color = self.apply_alpha(self.current_state.fill_style);
        self.commands.push(CanvasCommand::FillText {
            text,
            x: tx,
            y: ty,
            color,
            font: self.current_state.font.clone(),
            font_size: self.current_state.font_size * scale,
            font_family: self.current_state.font_family.clone(),
            font_weight: self.current_state.font_weight,
            italic: self.current_state.font_italic,
            text_align: self.current_state.text_align,
            text_baseline: self.current_state.text_baseline,
            max_width: max_width.map(|mw| mw * scale),
        });
    }

    /// Renders stroked/outlined text at `(x, y)` using the active `font`, `stroke_style`, and `line_width`.
    pub fn stroke_text(&mut self, text: impl Into<String>, x: f32, y: f32) {
        self.push_stroke_text(text.into(), x, y, None);
    }

    /// Renders stroked/outlined text at `(x, y)`, horizontally constrained to `max_width` pixels.
    pub fn stroke_text_with_max_width(
        &mut self,
        text: impl Into<String>,
        x: f32,
        y: f32,
        max_width: f32,
    ) {
        self.push_stroke_text(text.into(), x, y, Some(max_width.max(1.0)));
    }

    fn push_stroke_text(&mut self, text: String, x: f32, y: f32, max_width: Option<f32>) {
        if text.is_empty() {
            return;
        }
        let (tx, ty) = self.current_state.transform.transform_xy(x, y);
        let scale = self.transform_scale().max(1e-3);
        let color = self.apply_alpha(self.current_state.stroke_style);
        self.commands.push(CanvasCommand::StrokeText {
            text,
            x: tx,
            y: ty,
            color,
            line_width: self.current_state.line_width,
            font: self.current_state.font.clone(),
            font_size: self.current_state.font_size * scale,
            font_family: self.current_state.font_family.clone(),
            font_weight: self.current_state.font_weight,
            italic: self.current_state.font_italic,
            text_align: self.current_state.text_align,
            text_baseline: self.current_state.text_baseline,
            max_width: max_width.map(|mw| mw * scale),
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
    pub fn rasterize(
        &self,
        fb: &mut FrameBuffer,
        origin_x: f32,
        origin_y: f32,
        clip: Option<Rect>,
    ) {
        rasterize_canvas_commands(fb, &self.commands, origin_x, origin_y, clip);
    }
}

/// Thread-safe payload handle wrapping a UI-thread post-layout `on_paint` callback
/// (`Rc<dyn Fn(&mut Canvas2DContext, f32, f32)>`).
///
/// Storing the `!Send` closure in a thread-local table keyed by an `Arc` drop-guard token allows
/// callers to capture reactive [`Signal`](strata_dom::reactive::Signal)s and `Rc` handles freely
/// while keeping `LayoutNode: Send + Sync` for parallel layout passes.
#[derive(Debug)]
pub struct Canvas2DPaintCallback {
    token: u64,
    origin_thread: std::thread::ThreadId,
}

type Canvas2DPaintFn = std::rc::Rc<dyn Fn(&mut Canvas2DContext, f32, f32)>;

thread_local! {
    static CANVAS2D_PAINT_HOOKS: std::cell::RefCell<std::collections::HashMap<u64, Canvas2DPaintFn>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

static NEXT_CANVAS2D_PAINT_TOKEN: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

impl Canvas2DPaintCallback {
    /// Registers a post-layout `on_paint` closure on the current thread and returns an `Arc`
    /// handle suitable for `Attribute::scene_payload_arc` / `ViewportSceneHandle`.
    pub fn new(cb: Canvas2DPaintFn) -> std::sync::Arc<Self> {
        let token = NEXT_CANVAS2D_PAINT_TOKEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let origin_thread = std::thread::current().id();
        CANVAS2D_PAINT_HOOKS.with(|hooks| {
            hooks.borrow_mut().insert(token, cb);
        });
        std::sync::Arc::new(Self {
            token,
            origin_thread,
        })
    }

    /// Invokes the registered paint closure with the resolved post-layout `(width, height)`
    /// dimensions and returns the recorded [`CanvasCommand`] list.
    pub fn paint(&self, width: f32, height: f32) -> Vec<CanvasCommand> {
        let mut ctx = Canvas2DContext::new(width.max(0.0), height.max(0.0));
        if std::thread::current().id() == self.origin_thread {
            let cb_opt = CANVAS2D_PAINT_HOOKS.with(|hooks| hooks.borrow().get(&self.token).cloned());
            if let Some(cb) = cb_opt {
                cb(&mut ctx, width, height);
            }
        }
        ctx.finish()
    }
}

impl Drop for Canvas2DPaintCallback {
    fn drop(&mut self) {
        if std::thread::current().id() == self.origin_thread {
            let _ = CANVAS2D_PAINT_HOOKS.try_with(|hooks| {
                if let Ok(mut map) = hooks.try_borrow_mut() {
                    map.remove(&self.token);
                }
            });
        }
    }
}

/// Flattens a slice of [`CanvasPathVerb`]s (including cubic/quadratic Bézier curves and arcs)
/// into translated polyline subpaths `Vec<Vec<(f32, f32)>>`.
pub fn flatten_canvas_verbs(
    verbs: &[CanvasPathVerb],
    origin_x: f32,
    origin_y: f32,
) -> Vec<Vec<(f32, f32)>> {
    let mut subpaths: Vec<Vec<(f32, f32)>> = Vec::new();
    let mut cur_sub: Vec<(f32, f32)> = Vec::new();
    let mut current = (origin_x, origin_y);
    let mut start = (origin_x, origin_y);

    for verb in verbs {
        match *verb {
            CanvasPathVerb::MoveTo(x, y) => {
                if cur_sub.len() >= 2 {
                    subpaths.push(std::mem::take(&mut cur_sub));
                } else {
                    cur_sub.clear();
                }
                current = (x + origin_x, y + origin_y);
                start = current;
                cur_sub.push(current);
            }
            CanvasPathVerb::LineTo(x, y) => {
                let next = (x + origin_x, y + origin_y);
                if cur_sub.is_empty() {
                    cur_sub.push(current);
                }
                cur_sub.push(next);
                current = next;
            }
            CanvasPathVerb::QuadraticCurveTo { cpx, cpy, x, y } => {
                let p0 = current;
                let p1 = (cpx + origin_x, cpy + origin_y);
                let p2 = (x + origin_x, y + origin_y);
                if cur_sub.is_empty() {
                    cur_sub.push(p0);
                }
                let approx_len = ((p1.0 - p0.0).hypot(p1.1 - p0.1)
                    + (p2.0 - p1.0).hypot(p2.1 - p1.1))
                .max(1.0);
                let steps = ((approx_len / 4.0).ceil() as usize).clamp(12, 64);
                for i in 1..=steps {
                    let t = i as f32 / steps as f32;
                    let mt = 1.0 - t;
                    let px = mt * mt * p0.0 + 2.0 * mt * t * p1.0 + t * t * p2.0;
                    let py = mt * mt * p0.1 + 2.0 * mt * t * p1.1 + t * t * p2.1;
                    cur_sub.push((px, py));
                }
                current = p2;
            }
            CanvasPathVerb::BezierCurveTo {
                cp1x,
                cp1y,
                cp2x,
                cp2y,
                x,
                y,
            } => {
                let p0 = current;
                let p1 = (cp1x + origin_x, cp1y + origin_y);
                let p2 = (cp2x + origin_x, cp2y + origin_y);
                let p3 = (x + origin_x, y + origin_y);
                if cur_sub.is_empty() {
                    cur_sub.push(p0);
                }
                let approx_len = ((p1.0 - p0.0).hypot(p1.1 - p0.1)
                    + (p2.0 - p1.0).hypot(p2.1 - p1.1)
                    + (p3.0 - p2.0).hypot(p3.1 - p2.1))
                .max(1.0);
                let steps = ((approx_len / 4.0).ceil() as usize).clamp(16, 80);
                for i in 1..=steps {
                    let t = i as f32 / steps as f32;
                    let mt = 1.0 - t;
                    let mt2 = mt * mt;
                    let t2 = t * t;
                    let px = mt2 * mt * p0.0
                        + 3.0 * mt2 * t * p1.0
                        + 3.0 * mt * t2 * p2.0
                        + t2 * t * p3.0;
                    let py = mt2 * mt * p0.1
                        + 3.0 * mt2 * t * p1.1
                        + 3.0 * mt * t2 * p2.1
                        + t2 * t * p3.1;
                    cur_sub.push((px, py));
                }
                current = p3;
            }
            CanvasPathVerb::Arc {
                cx,
                cy,
                radius,
                start_angle,
                end_angle,
                counter_clockwise,
            } => {
                let acx = cx + origin_x;
                let acy = cy + origin_y;
                let mut sweep = end_angle - start_angle;
                if counter_clockwise {
                    if sweep >= 0.0 {
                        sweep -= std::f32::consts::TAU;
                    }
                } else if sweep <= 0.0 && (end_angle - start_angle).abs() > 1e-5 {
                    sweep += std::f32::consts::TAU;
                }
                let steps = ((radius.abs() * sweep.abs() / 3.0).ceil() as usize).clamp(16, 96);
                for i in 0..=steps {
                    let a = start_angle + sweep * (i as f32 / steps as f32);
                    let pt = (acx + radius * a.cos(), acy + radius * a.sin());
                    if i == 0 && cur_sub.is_empty() {
                        start = pt;
                    }
                    cur_sub.push(pt);
                    current = pt;
                }
            }
            CanvasPathVerb::Close => {
                if !cur_sub.is_empty() && current != start {
                    cur_sub.push(start);
                    current = start;
                }
                if cur_sub.len() >= 2 {
                    subpaths.push(std::mem::take(&mut cur_sub));
                } else {
                    cur_sub.clear();
                }
            }
        }
    }

    if cur_sub.len() >= 2 {
        subpaths.push(cur_sub);
    }
    subpaths
}

/// Computes the axis-aligned bounding box of `verbs` shifted by `(origin_x, origin_y)`.
pub fn canvas_verbs_bounds(verbs: &[CanvasPathVerb], origin_x: f32, origin_y: f32) -> Option<Rect> {
    let subpaths = flatten_canvas_verbs(verbs, origin_x, origin_y);
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for sub in &subpaths {
        for &(x, y) in sub {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    if min_x.is_finite() && min_y.is_finite() && max_x >= min_x && max_y >= min_y {
        Some(Rect::new(min_x, min_y, max_x - min_x, max_y - min_y))
    } else {
        None
    }
}

/// Measures `text` using the active font styling and registered platform text measurer,
/// falling back to the inline headless OpenType/glyph estimator when no platform backend is registered.
pub fn measure_canvas_text_metrics(
    text: &str,
    font_size: f32,
    font_family: &str,
    font_weight: u16,
    italic: bool,
) -> CanvasTextMetrics {
    let size = font_size.max(0.0);
    if size <= 0.0 {
        return CanvasTextMetrics::default();
    }
    let primary_family = font_family
        .split(',')
        .next()
        .map(str::trim)
        .map(|s| s.trim_matches(|c| c == '\'' || c == '"'))
        .filter(|s| !s.is_empty())
        .unwrap_or("sans-serif");
    let family_atom = strata_primitives::atom::intern(primary_family);
    let mono = strata_layout::text::is_monospace_family(family_atom)
        || font_family.to_ascii_lowercase().contains("mono");
    let weight = strata_style::types::FontWeight::from_u16(font_weight);
    let style = if italic {
        strata_style::types::FontStyle::Italic
    } else {
        strata_style::types::FontStyle::Normal
    };
    let (width, height) = if text.is_empty() {
        let h = strata_layout::text::measure_text_bounds_styled_with_family(
            " ",
            size,
            1.0,
            None,
            mono,
            family_atom,
            weight,
            style,
        )
        .1;
        (0.0, h)
    } else {
        strata_layout::text::measure_text_bounds_styled_with_family(
            text,
            size,
            1.0,
            None,
            mono,
            family_atom,
            weight,
            style,
        )
    };
    let (font_ascent, font_descent) = if let Some(metrics_fn) = strata_layout::text::get_font_metrics() {
        let (asc, desc, _gap) = metrics_fn(family_atom, size, font_weight >= 600);
        (asc.max(0.0), desc.max(0.0))
    } else {
        (size * 0.8, size * 0.2)
    };
    CanvasTextMetrics {
        width,
        height: height.max(font_ascent + font_descent),
        actual_bounding_box_ascent: font_ascent,
        actual_bounding_box_descent: font_descent,
        font_bounding_box_ascent: font_ascent,
        font_bounding_box_descent: font_descent,
    }
}

#[derive(Clone)]
struct RasterState {
    clip: Option<Rect>,
    clip_polys: Vec<Vec<Vec<(f32, f32)>>>,
    line_dash: Vec<f32>,
    dash_offset: f32,
    line_cap: CanvasLineCap,
    line_join: CanvasLineJoin,
}

pub fn point_in_subpaths_even_odd(subpaths: &[Vec<(f32, f32)>], px: f32, py: f32) -> bool {
    let mut inside = false;
    for sub in subpaths {
        if sub.len() < 3 {
            continue;
        }
        let mut j = sub.len() - 1;
        for i in 0..sub.len() {
            let (xi, yi) = sub[i];
            let (xj, yj) = sub[j];
            let intersects =
                ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi);
            if intersects {
                inside = !inside;
            }
            j = i;
        }
    }
    inside
}

fn point_passes_clip(state: &RasterState, x: f32, y: f32) -> bool {
    if let Some(c) = state.clip {
        if x < c.x || x >= c.max_x() || y < c.y || y >= c.max_y() {
            return false;
        }
    }
    for poly in &state.clip_polys {
        if !point_in_subpaths_even_odd(poly, x, y) {
            return false;
        }
    }
    true
}

fn rasterize_fill_subpaths(
    fb: &mut FrameBuffer,
    subpaths: &[Vec<(f32, f32)>],
    color: Color,
    state: &RasterState,
) {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for sub in subpaths {
        for &(x, y) in sub {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    if !min_x.is_finite() || max_x <= min_x || max_y <= min_y {
        return;
    }

    let x0 = (min_x.floor() as isize).max(0) as usize;
    let x1 = (max_x.ceil() as usize).min(fb.width);
    let y0 = (min_y.floor() as isize).max(0) as usize;
    let y1 = (max_y.ceil() as usize).min(fb.height);

    for y in y0..y1 {
        let py = y as f32 + 0.5;
        for x in x0..x1 {
            let px = x as f32 + 0.5;
            if !point_passes_clip(state, px, py) {
                continue;
            }
            if point_in_subpaths_even_odd(subpaths, px, py) {
                fb.set_pixel(x, y, color);
            }
        }
    }
}

#[derive(Clone, Copy)]
struct StrokeSeg {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    tx: f32,
    ty: f32,
    nx: f32,
    ny: f32,
    len: f32,
    cap0: CanvasLineCap,
    cap1: CanvasLineCap,
}

impl StrokeSeg {
    fn from_pts(p0: (f32, f32), p1: (f32, f32), cap0: CanvasLineCap, cap1: CanvasLineCap) -> Self {
        let vx = p1.0 - p0.0;
        let vy = p1.1 - p0.1;
        let len = vx.hypot(vy);
        let (tx, ty) = if len > 1e-6 {
            (vx / len, vy / len)
        } else {
            (1.0, 0.0)
        };
        Self {
            x0: p0.0,
            y0: p0.1,
            x1: p1.0,
            y1: p1.1,
            tx,
            ty,
            nx: -ty,
            ny: tx,
            len,
            cap0,
            cap1,
        }
    }

    #[inline]
    fn signed_dist(&self, px: f32, py: f32, half_lw: f32) -> f32 {
        let wx = px - self.x0;
        let wy = py - self.y0;
        if self.len <= 1e-6 {
            return if self.cap0 == CanvasLineCap::Round || self.cap1 == CanvasLineCap::Round {
                wx.hypot(wy) - half_lw
            } else if self.cap0 == CanvasLineCap::Square || self.cap1 == CanvasLineCap::Square {
                let dx = wx.abs() - half_lw;
                let dy = wy.abs() - half_lw;
                dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0)
            } else {
                f32::INFINITY
            };
        }

        let s = wx * self.tx + wy * self.ty;
        let u = (wx * self.nx + wy * self.ny).abs();
        let s0 = if self.cap0 == CanvasLineCap::Square {
            -half_lw
        } else {
            0.0
        };
        let s1 = if self.cap1 == CanvasLineCap::Square {
            self.len + half_lw
        } else {
            self.len
        };
        let dx = (s0 - s).max(s - s1);
        let dy = u - half_lw;
        let mut sd = dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0);
        if self.cap0 == CanvasLineCap::Round {
            sd = sd.min(s.hypot(u) - half_lw);
        }
        if self.cap1 == CanvasLineCap::Round {
            sd = sd.min((s - self.len).hypot(u) - half_lw);
        }
        sd
    }

    #[inline]
    fn bounds_px(&self, half_lw: f32, fb_w: usize, fb_h: usize) -> (usize, usize, usize, usize) {
        let pad = half_lw * 1.5 + 2.0;
        let x0 = ((self.x0.min(self.x1) - pad).floor() as isize).max(0) as usize;
        let x1 = ((self.x0.max(self.x1) + pad).ceil() as usize).min(fb_w);
        let y0 = ((self.y0.min(self.y1) - pad).floor() as isize).max(0) as usize;
        let y1 = ((self.y0.max(self.y1) + pad).ceil() as usize).min(fb_h);
        (x0, y0, x1, y1)
    }
}

#[derive(Clone, Copy)]
struct StrokeJoin {
    jx: f32,
    jy: f32,
    t0x: f32,
    t0y: f32,
    t1x: f32,
    t1y: f32,
}

impl StrokeJoin {
    #[inline]
    fn signed_dist(&self, px: f32, py: f32, half_lw: f32, join: CanvasLineJoin) -> f32 {
        let dpx = px - self.jx;
        let dpy = py - self.jy;
        let dot_t = self.t0x * self.t1x + self.t0y * self.t1y;
        if dot_t > 0.9999 {
            return f32::INFINITY;
        }
        let cross = self.t0x * self.t1y - self.t0y * self.t1x;
        let sd_disk = dpx.hypot(dpy) - half_lw;
        if cross.abs() < 1e-4 && dot_t < 0.0 {
            return if join == CanvasLineJoin::Round {
                sd_disk
            } else {
                f32::INFINITY
            };
        }
        if join == CanvasLineJoin::Round {
            return sd_disk;
        }

        let sign = if cross > 0.0 { -1.0 } else { 1.0 };
        let n0x = sign * (-self.t0y);
        let n0y = sign * self.t0x;
        let n1x = sign * (-self.t1y);
        let n1y = sign * self.t1x;

        let sum_x = n0x + n1x;
        let sum_y = n0y + n1y;
        let sum_len = sum_x.hypot(sum_y);
        if sum_len <= 1e-5 {
            return f32::INFINITY;
        }
        let bis_x = sum_x / sum_len;
        let bis_y = sum_y / sum_len;

        let miter_ratio = 2.0 / sum_len;
        let use_bevel = join == CanvasLineJoin::Bevel || miter_ratio > 10.0;
        if use_bevel {
            let d_bevel = half_lw * sum_len * 0.5;
            let sd_bevel = dpx * bis_x + dpy * bis_y - d_bevel;
            sd_disk.max(sd_bevel)
        } else {
            let sd0 = dpx * n0x + dpy * n0y - half_lw;
            let sd1 = dpx * n1x + dpy * n1y - half_lw;
            let sd_back0 = -(dpx * self.t0x + dpy * self.t0y);
            let sd_back1 = dpx * self.t1x + dpy * self.t1y;
            let sd_wedge = sd0.max(sd1).max(sd_back0).max(sd_back1);
            sd_disk.min(sd_wedge)
        }
    }

    #[inline]
    fn bounds_px(
        &self,
        half_lw: f32,
        join: CanvasLineJoin,
        fb_w: usize,
        fb_h: usize,
    ) -> (usize, usize, usize, usize) {
        let pad = match join {
            CanvasLineJoin::Miter => half_lw * 10.0 + 2.0,
            CanvasLineJoin::Round | CanvasLineJoin::Bevel => half_lw + 2.0,
        };
        let x0 = ((self.jx - pad).floor() as isize).max(0) as usize;
        let x1 = ((self.jx + pad).ceil() as usize).min(fb_w);
        let y0 = ((self.jy - pad).floor() as isize).max(0) as usize;
        let y1 = ((self.jy + pad).ceil() as usize).min(fb_h);
        (x0, y0, x1, y1)
    }
}

fn rasterize_stroke_run(
    fb: &mut FrameBuffer,
    pts: &[(f32, f32)],
    closed: bool,
    color: Color,
    line_width: f32,
    state: &RasterState,
) {
    if pts.len() < 2 {
        return;
    }

    let mut clean: Vec<(f32, f32)> = Vec::with_capacity(pts.len());
    for &p in pts {
        if clean
            .last()
            .is_none_or(|&prev| (p.0 - prev.0).hypot(p.1 - prev.1) > 1e-4)
        {
            clean.push(p);
        }
    }
    if closed && clean.len() >= 2 {
        let first = clean[0];
        let last = *clean.last().unwrap();
        if (first.0 - last.0).hypot(first.1 - last.1) > 1e-4 {
            clean.push(first);
        }
    }
    if clean.len() < 2 {
        let seg = StrokeSeg::from_pts(pts[0], pts[0], state.line_cap, state.line_cap);
        rasterize_stroke_primitives(fb, &[seg], &[], color, line_width, state);
        return;
    }

    let seg_count = clean.len() - 1;
    let is_loop = closed && seg_count >= 2;
    let mut segs = Vec::with_capacity(seg_count);
    for (i, w) in clean.windows(2).enumerate() {
        let cap0 = if i == 0 && !is_loop {
            state.line_cap
        } else {
            CanvasLineCap::Butt
        };
        let cap1 = if i + 1 == seg_count && !is_loop {
            state.line_cap
        } else {
            CanvasLineCap::Butt
        };
        segs.push(StrokeSeg::from_pts(w[0], w[1], cap0, cap1));
    }

    let mut joins = Vec::with_capacity(if is_loop { seg_count } else { seg_count - 1 });
    for i in 1..seg_count {
        joins.push(StrokeJoin {
            jx: segs[i].x0,
            jy: segs[i].y0,
            t0x: segs[i - 1].tx,
            t0y: segs[i - 1].ty,
            t1x: segs[i].tx,
            t1y: segs[i].ty,
        });
    }
    if is_loop {
        let last = segs[seg_count - 1];
        joins.push(StrokeJoin {
            jx: segs[0].x0,
            jy: segs[0].y0,
            t0x: last.tx,
            t0y: last.ty,
            t1x: segs[0].tx,
            t1y: segs[0].ty,
        });
    }

    rasterize_stroke_primitives(fb, &segs, &joins, color, line_width, state);
}

fn rasterize_stroke_primitives(
    fb: &mut FrameBuffer,
    segs: &[StrokeSeg],
    joins: &[StrokeJoin],
    color: Color,
    line_width: f32,
    state: &RasterState,
) {
    if segs.is_empty() || color.a == 0 {
        return;
    }
    let half_lw = line_width * 0.5;

    if segs.len() == 1 && joins.is_empty() {
        let seg = segs[0];
        let (x0, y0, x1, y1) = seg.bounds_px(half_lw, fb.width, fb.height);
        for y in y0..y1 {
            let py = y as f32 + 0.5;
            if let Some(c) = state.clip {
                if py < c.y || py >= c.max_y() {
                    continue;
                }
            }
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                if !point_passes_clip(state, px, py) {
                    continue;
                }
                let sd = seg.signed_dist(px, py, half_lw);
                let cov = (0.5 - sd).clamp(0.0, 1.0);
                if cov > 0.005 {
                    fb.set_pixel(
                        x,
                        y,
                        Color::rgba(
                            color.r,
                            color.g,
                            color.b,
                            (color.a() * cov * 255.0).round() as u8,
                        ),
                    );
                }
            }
        }
        return;
    }

    let mut min_x = usize::MAX;
    let mut min_y = usize::MAX;
    let mut max_x = 0usize;
    let mut max_y = 0usize;
    let seg_boxes: Vec<(usize, usize, usize, usize)> = segs
        .iter()
        .map(|s| {
            let b = s.bounds_px(half_lw, fb.width, fb.height);
            min_x = min_x.min(b.0);
            min_y = min_y.min(b.1);
            max_x = max_x.max(b.2);
            max_y = max_y.max(b.3);
            b
        })
        .collect();
    let join_boxes: Vec<(usize, usize, usize, usize)> = joins
        .iter()
        .map(|j| {
            let b = j.bounds_px(half_lw, state.line_join, fb.width, fb.height);
            min_x = min_x.min(b.0);
            min_y = min_y.min(b.1);
            max_x = max_x.max(b.2);
            max_y = max_y.max(b.3);
            b
        })
        .collect();

    if let Some(c) = state.clip {
        min_x = min_x.max((c.x.floor() as isize).max(0) as usize);
        min_y = min_y.max((c.y.floor() as isize).max(0) as usize);
        max_x = max_x.min((c.max_x().ceil() as usize).min(fb.width));
        max_y = max_y.min((c.max_y().ceil() as usize).min(fb.height));
    }
    if min_x >= max_x || min_y >= max_y {
        return;
    }

    const TILE: usize = 256;
    let mut tile_sd: Vec<f32> = Vec::new();
    let mut ty0 = min_y;
    while ty0 < max_y {
        let ty1 = (ty0 + TILE).min(max_y);
        let th = ty1 - ty0;
        let mut tx0 = min_x;
        while tx0 < max_x {
            let tx1 = (tx0 + TILE).min(max_x);
            let tw = tx1 - tx0;
            let len = tw * th;
            tile_sd.clear();
            tile_sd.resize(len, f32::INFINITY);

            let mut touched = false;
            for (seg, &(sx0, sy0, sx1, sy1)) in segs.iter().zip(&seg_boxes) {
                let ox0 = tx0.max(sx0);
                let oy0 = ty0.max(sy0);
                let ox1 = tx1.min(sx1);
                let oy1 = ty1.min(sy1);
                if ox0 >= ox1 || oy0 >= oy1 {
                    continue;
                }
                touched = true;
                for y in oy0..oy1 {
                    let py = y as f32 + 0.5;
                    let row = (y - ty0) * tw;
                    for x in ox0..ox1 {
                        let px = x as f32 + 0.5;
                        let idx = row + (x - tx0);
                        let sd = seg.signed_dist(px, py, half_lw);
                        if sd < tile_sd[idx] {
                            tile_sd[idx] = sd;
                        }
                    }
                }
            }

            for (join, &(jx0, jy0, jx1, jy1)) in joins.iter().zip(&join_boxes) {
                let ox0 = tx0.max(jx0);
                let oy0 = ty0.max(jy0);
                let ox1 = tx1.min(jx1);
                let oy1 = ty1.min(jy1);
                if ox0 >= ox1 || oy0 >= oy1 {
                    continue;
                }
                touched = true;
                for y in oy0..oy1 {
                    let py = y as f32 + 0.5;
                    let row = (y - ty0) * tw;
                    for x in ox0..ox1 {
                        let px = x as f32 + 0.5;
                        let idx = row + (x - tx0);
                        let sd = join.signed_dist(px, py, half_lw, state.line_join);
                        if sd < tile_sd[idx] {
                            tile_sd[idx] = sd;
                        }
                    }
                }
            }

            if touched {
                for y in ty0..ty1 {
                    let py = y as f32 + 0.5;
                    let row = (y - ty0) * tw;
                    for x in tx0..tx1 {
                        let sd = tile_sd[row + (x - tx0)];
                        if sd >= 0.5 {
                            continue;
                        }
                        let px = x as f32 + 0.5;
                        if !point_passes_clip(state, px, py) {
                            continue;
                        }
                        let cov = (0.5 - sd).clamp(0.0, 1.0);
                        if cov > 0.005 {
                            fb.set_pixel(
                                x,
                                y,
                                Color::rgba(
                                    color.r,
                                    color.g,
                                    color.b,
                                    (color.a() * cov * 255.0).round() as u8,
                                ),
                            );
                        }
                    }
                }
            }

            tx0 = tx1;
        }
        ty0 = ty1;
    }
}

fn rasterize_polyline(
    fb: &mut FrameBuffer,
    pts: &[(f32, f32)],
    color: Color,
    line_width: f32,
    state: &RasterState,
) {
    if pts.len() < 2 {
        return;
    }

    let is_closed = pts.len() >= 3
        && (pts[0].0 - pts[pts.len() - 1].0).hypot(pts[0].1 - pts[pts.len() - 1].1) <= 1e-3;

    let period: f32 = state.line_dash.iter().copied().sum();
    if state.line_dash.is_empty() || period <= 1e-4 {
        rasterize_stroke_run(fb, pts, is_closed, color, line_width, state);
        return;
    }

    let mut phase = state.dash_offset.rem_euclid(period);
    let mut dash_idx = 0usize;
    while dash_idx < state.line_dash.len() && phase >= state.line_dash[dash_idx] {
        phase -= state.line_dash[dash_idx];
        dash_idx = (dash_idx + 1) % state.line_dash.len();
    }
    let mut rem = (state.line_dash[dash_idx] - phase).max(0.0);

    let mut runs: Vec<Vec<(f32, f32)>> = Vec::new();
    let mut cur_run: Vec<(f32, f32)> = Vec::new();
    let started_on = dash_idx.is_multiple_of(2) && rem > 1e-4;

    for seg in pts.windows(2) {
        let (x0, y0) = seg[0];
        let (x1, y1) = seg[1];
        let dx = x1 - x0;
        let dy = y1 - y0;
        let seg_len = dx.hypot(dy);
        if seg_len <= 1e-4 {
            continue;
        }
        let mut cursor = 0.0_f32;
        while cursor < seg_len {
            if rem <= 1e-4 {
                dash_idx = (dash_idx + 1) % state.line_dash.len();
                rem = state.line_dash[dash_idx];
                if !dash_idx.is_multiple_of(2) && cur_run.len() >= 2 {
                    runs.push(std::mem::take(&mut cur_run));
                } else if !dash_idx.is_multiple_of(2) {
                    cur_run.clear();
                }
                if rem <= 1e-4 {
                    break;
                }
            }
            let step = rem.min(seg_len - cursor);
            let t0 = cursor / seg_len;
            let t1 = (cursor + step) / seg_len;
            if dash_idx.is_multiple_of(2) {
                let p_start = (x0 + dx * t0, y0 + dy * t0);
                let p_end = (x0 + dx * t1, y0 + dy * t1);
                if cur_run.is_empty() {
                    cur_run.push(p_start);
                }
                cur_run.push(p_end);
            }
            cursor += step;
            rem -= step;
        }
    }
    let ended_on = !cur_run.is_empty();
    if cur_run.len() >= 2 {
        runs.push(cur_run);
    }

    if is_closed && started_on && ended_on && runs.len() >= 2 {
        let mut last = runs.pop().unwrap();
        let first = runs.remove(0);
        last.extend(first.into_iter().skip(1));
        rasterize_stroke_run(fb, &last, false, color, line_width, state);
    } else if is_closed && started_on && ended_on && runs.len() == 1 {
        rasterize_stroke_run(fb, &runs[0], true, color, line_width, state);
        return;
    }

    for run in &runs {
        rasterize_stroke_run(fb, run, false, color, line_width, state);
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
    let mut state = RasterState {
        clip,
        clip_polys: Vec::new(),
        line_dash: Vec::new(),
        dash_offset: 0.0,
        line_cap: CanvasLineCap::Butt,
        line_join: CanvasLineJoin::Miter,
    };
    let mut stack: Vec<RasterState> = Vec::new();

    for cmd in commands {
        match cmd {
            CanvasCommand::Save => {
                stack.push(state.clone());
            }
            CanvasCommand::Restore => {
                if let Some(prev) = stack.pop() {
                    state = prev;
                }
            }
            CanvasCommand::SetLineDash { segments, offset } => {
                state.line_dash = segments.clone();
                state.dash_offset = *offset;
            }
            CanvasCommand::SetLineCap(cap) => {
                state.line_cap = *cap;
            }
            CanvasCommand::SetLineJoin(join) => {
                state.line_join = *join;
            }
            CanvasCommand::ClipPath { verbs } => {
                let subpaths = flatten_canvas_verbs(verbs, origin_x, origin_y);
                if let Some(bbox) = canvas_verbs_bounds(verbs, origin_x, origin_y) {
                    state.clip = match state.clip {
                        Some(existing) => existing.intersect(bbox).or(Some(Rect::zero())),
                        None => Some(bbox),
                    };
                }
                if !subpaths.is_empty() {
                    state.clip_polys.push(subpaths);
                }
            }
            CanvasCommand::FillRect { bounds, color } => {
                let translated = Rect::new(
                    bounds.x + origin_x,
                    bounds.y + origin_y,
                    bounds.width,
                    bounds.height,
                );
                if state.clip_polys.is_empty() {
                    fb.draw_rect(translated, *color, state.clip);
                } else {
                    let poly = vec![vec![
                        (translated.x, translated.y),
                        (translated.max_x(), translated.y),
                        (translated.max_x(), translated.max_y()),
                        (translated.x, translated.max_y()),
                        (translated.x, translated.y),
                    ]];
                    rasterize_fill_subpaths(fb, &poly, *color, &state);
                }
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
            CanvasCommand::StrokeRect {
                bounds,
                color,
                line_width,
            } => {
                let translated = Rect::new(
                    bounds.x + origin_x,
                    bounds.y + origin_y,
                    bounds.width,
                    bounds.height,
                );
                if state.line_dash.is_empty() && state.clip_polys.is_empty() {
                    let lw = *line_width;
                    fb.draw_rect(
                        Rect::new(translated.x, translated.y, translated.width, lw),
                        *color,
                        state.clip,
                    );
                    fb.draw_rect(
                        Rect::new(translated.x, translated.max_y() - lw, translated.width, lw),
                        *color,
                        state.clip,
                    );
                    fb.draw_rect(
                        Rect::new(translated.x, translated.y, lw, translated.height),
                        *color,
                        state.clip,
                    );
                    fb.draw_rect(
                        Rect::new(translated.max_x() - lw, translated.y, lw, translated.height),
                        *color,
                        state.clip,
                    );
                } else {
                    let pts = [
                        (translated.x, translated.y),
                        (translated.max_x(), translated.y),
                        (translated.max_x(), translated.max_y()),
                        (translated.x, translated.max_y()),
                        (translated.x, translated.y),
                    ];
                    rasterize_polyline(fb, &pts, *color, *line_width, &state);
                }
            }
            CanvasCommand::DrawLine {
                p0,
                p1,
                color,
                line_width,
            } => {
                let pts = [(p0.0 + origin_x, p0.1 + origin_y), (p1.0 + origin_x, p1.1 + origin_y)];
                rasterize_polyline(fb, &pts, *color, *line_width, &state);
            }
            CanvasCommand::DrawCircle {
                center,
                radius,
                fill_color,
                stroke_color,
                line_width,
            } => {
                let cx = center.0 + origin_x;
                let cy = center.1 + origin_y;
                let r = *radius;
                let lw = *line_width;

                if let Some(fc) = fill_color {
                    let min_x = ((cx - r - lw).floor() as isize).max(0) as usize;
                    let max_x = ((cx + r + lw).ceil() as usize).min(fb.width);
                    let min_y = ((cy - r - lw).floor() as isize).max(0) as usize;
                    let max_y = ((cy + r + lw).ceil() as usize).min(fb.height);

                    for y in min_y..max_y {
                        let py = y as f32 + 0.5;
                        let dy = py - cy;
                        for x in min_x..max_x {
                            let px = x as f32 + 0.5;
                            if !point_passes_clip(&state, px, py) {
                                continue;
                            }
                            let dx = px - cx;
                            let dist = (dx * dx + dy * dy).sqrt();
                            let coverage = (0.5 + r - dist).clamp(0.0, 1.0);
                            if coverage > 0.005 {
                                fb.set_pixel(
                                    x,
                                    y,
                                    Color::rgba(
                                        fc.r,
                                        fc.g,
                                        fc.b,
                                        (fc.a() * coverage * 255.0) as u8,
                                    ),
                                );
                            }
                        }
                    }
                }

                if let Some(sc) = stroke_color {
                    if !state.line_dash.is_empty() {
                        let steps = ((r * std::f32::consts::TAU / 3.0).ceil() as usize).clamp(24, 128);
                        let pts: Vec<(f32, f32)> = (0..=steps)
                            .map(|i| {
                                let a = (i as f32 / steps as f32) * std::f32::consts::TAU;
                                (cx + r * a.cos(), cy + r * a.sin())
                            })
                            .collect();
                        rasterize_polyline(fb, &pts, *sc, lw, &state);
                    } else {
                        let min_x = ((cx - r - lw).floor() as isize).max(0) as usize;
                        let max_x = ((cx + r + lw).ceil() as usize).min(fb.width);
                        let min_y = ((cy - r - lw).floor() as isize).max(0) as usize;
                        let max_y = ((cy + r + lw).ceil() as usize).min(fb.height);
                        for y in min_y..max_y {
                            let py = y as f32 + 0.5;
                            let dy = py - cy;
                            for x in min_x..max_x {
                                let px = x as f32 + 0.5;
                                if !point_passes_clip(&state, px, py) {
                                    continue;
                                }
                                let dx = px - cx;
                                let dist = (dx * dx + dy * dy).sqrt();
                                let stroke_dist = (dist - r).abs();
                                let coverage = (0.5 + lw * 0.5 - stroke_dist).clamp(0.0, 1.0);
                                if coverage > 0.005 {
                                    fb.set_pixel(
                                        x,
                                        y,
                                        Color::rgba(
                                            sc.r,
                                            sc.g,
                                            sc.b,
                                            (sc.a() * coverage * 255.0) as u8,
                                        ),
                                    );
                                }
                            }
                        }
                    }
                }
            }
            CanvasCommand::DrawPath {
                verbs,
                fill_color,
                stroke_color,
                line_width,
            } => {
                let subpaths = flatten_canvas_verbs(verbs, origin_x, origin_y);
                if let Some(fc) = fill_color {
                    rasterize_fill_subpaths(fb, &subpaths, *fc, &state);
                }
                if let Some(sc) = stroke_color {
                    for sub in &subpaths {
                        rasterize_polyline(fb, sub, *sc, *line_width, &state);
                    }
                }
            }
            CanvasCommand::FillText {
                text,
                x,
                y,
                color,
                font_size,
                font_family,
                font_weight,
                italic,
                text_align,
                text_baseline,
                max_width,
                ..
            }
            | CanvasCommand::StrokeText {
                text,
                x,
                y,
                color,
                font_size,
                font_family,
                font_weight,
                italic,
                text_align,
                text_baseline,
                max_width,
                ..
            } => {
                let metrics = measure_canvas_text_metrics(
                    text,
                    *font_size,
                    font_family,
                    *font_weight,
                    *italic,
                );
                let measured_w = if let Some(mw) = *max_width {
                    metrics.width.min(mw.max(0.0))
                } else {
                    metrics.width
                };
                let est_h = metrics.height.max(*font_size);
                let tx = origin_x
                    + match text_align {
                        CanvasTextAlign::Left | CanvasTextAlign::Start => *x,
                        CanvasTextAlign::Center => *x - measured_w * 0.5,
                        CanvasTextAlign::Right | CanvasTextAlign::End => *x - measured_w,
                    };
                let ty = origin_y
                    + match text_baseline {
                        CanvasTextBaseline::Top | CanvasTextBaseline::Hanging => *y,
                        CanvasTextBaseline::Middle => *y - est_h * 0.5,
                        CanvasTextBaseline::Bottom | CanvasTextBaseline::Ideographic => *y - est_h,
                        CanvasTextBaseline::Alphabetic => *y - metrics.font_bounding_box_ascent,
                    };
                fb.draw_text((tx, ty), text, *font_size, *color, state.clip);
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

    #[test]
    fn test_canvas_bezier_dash_clip_and_text() {
        let (size, family, weight, italic) =
            parse_canvas_font("italic bold 14px/1.2 'JetBrains Mono', monospace");
        assert!((size - 14.0).abs() < 0.01);
        assert_eq!(family, "JetBrains Mono, monospace");
        assert_eq!(weight, 700);
        assert!(italic);

        // Dashed horizontal line leaves transparent gaps
        let mut fb = FrameBuffer::new(200, 100);
        let mut ctx = Canvas2DContext::new(200.0, 100.0);
        ctx.set_stroke_style(Color::rgb(255, 0, 0));
        ctx.set_line_width(2.0);
        ctx.set_line_dash(&[10.0, 10.0]);
        ctx.begin_path();
        ctx.move_to(0.0, 20.0);
        ctx.line_to(100.0, 20.0);
        ctx.stroke();
        ctx.rasterize(&mut fb, 0.0, 0.0, None);
        assert!(fb.get_pixel(5, 20).r > 200, "dash on-segment must be drawn");
        assert_eq!(fb.get_pixel(15, 20), Color::TRANSPARENT, "dash gap must stay empty");

        // Clip restricts fill_rect, and restore removes clip
        let mut fb_clip = FrameBuffer::new(100, 100);
        let mut c2 = Canvas2DContext::new(100.0, 100.0);
        c2.save();
        c2.begin_path();
        c2.rect(10.0, 10.0, 30.0, 30.0);
        c2.clip();
        c2.set_fill_style(Color::rgb(0, 200, 255));
        c2.fill_rect(0.0, 0.0, 100.0, 100.0);
        c2.restore();
        c2.set_fill_style(Color::rgb(255, 255, 0));
        c2.fill_rect(60.0, 60.0, 20.0, 20.0);
        c2.rasterize(&mut fb_clip, 0.0, 0.0, None);
        assert_eq!(fb_clip.get_pixel(25, 25), Color::rgb(0, 200, 255));
        assert_eq!(fb_clip.get_pixel(5, 5), Color::TRANSPARENT);
        assert_eq!(fb_clip.get_pixel(70, 70), Color::rgb(255, 255, 0));

        // Cubic & quadratic bezier + fill_text + stroke_text
        let mut fb_curve = FrameBuffer::new(160, 120);
        let mut c3 = Canvas2DContext::new(160.0, 120.0);
        c3.set_stroke_style(Color::rgb(50, 255, 50));
        c3.set_line_width(2.0);
        c3.begin_path();
        c3.move_to(10.0, 60.0);
        c3.quadratic_curve_to(50.0, 10.0, 90.0, 60.0);
        c3.bezier_curve_to(110.0, 90.0, 130.0, 90.0, 150.0, 60.0);
        c3.stroke();
        c3.set_font("bold 12px monospace");
        c3.set_text_align(CanvasTextAlign::Left);
        c3.set_text_baseline(CanvasTextBaseline::Top);
        c3.set_fill_style(Color::rgb(255, 255, 255));
        c3.fill_text("CLK_0", 10.0, 5.0);
        c3.stroke_text("OUT", 80.0, 5.0);
        c3.rasterize(&mut fb_curve, 0.0, 0.0, None);

        // Quadratic curve midpoint at t=0.5 is (50, 35)
        assert!(
            fb_curve.get_pixel(50, 35).g > 150,
            "quadratic curve midpoint should be lit"
        );
        // Text region (10..60, 5..20) should have non-transparent white pixels
        let mut lit_text = 0usize;
        for y in 5..22 {
            for x in 10..60 {
                if fb_curve.get_pixel(x, y).a > 0 {
                    lit_text += 1;
                }
            }
        }
        assert!(lit_text > 5, "fill_text should rasterize visible glyph pixels");
    }

    #[test]
    fn test_canvas_measure_text_metrics() {
        let mut ctx = Canvas2DContext::new(400.0, 200.0);
        ctx.set_font("12px monospace");
        let empty = ctx.measure_text("");
        assert_eq!(empty.width, 0.0);
        assert!(empty.height > 0.0);

        let short = ctx.measure_text("VREF");
        let long = ctx.measure_text("VREF_BUFFERED_OUT");
        assert!(short.width > 0.0);
        assert!(long.width > short.width * 2.0);
        assert!(short.font_bounding_box_ascent > 0.0);
        assert!(short.font_bounding_box_descent > 0.0);

        ctx.set_font("24px sans-serif");
        let bigger = ctx.measure_text("VREF");
        assert!(bigger.width > short.width * 1.4);
        assert!(bigger.height > short.height * 1.4);
    }

    #[test]
    fn test_canvas_round_rect_and_clip() {
        let mut fb = FrameBuffer::new(120, 80);
        let mut ctx = Canvas2DContext::new(120.0, 80.0);

        // Huge radius on a 40x20 box clamps to 10px capsule corners cleanly
        ctx.set_fill_style(Color::rgb(20, 180, 240));
        ctx.begin_path();
        ctx.round_rect(10.0, 10.0, 40.0, 20.0, 100.0);
        ctx.fill();

        // Asymmetric radii: sharp TL (0), rounded TR (8), sharp BR (0), rounded BL (8)
        ctx.save();
        ctx.begin_path();
        ctx.round_rect(60.0, 10.0, 40.0, 30.0, [0.0, 8.0, 0.0, 8.0]);
        ctx.clip();
        ctx.set_fill_style(Color::rgb(255, 160, 0));
        ctx.fill_rect(60.0, 10.0, 40.0, 30.0);
        ctx.restore();

        ctx.rasterize(&mut fb, 0.0, 0.0, None);

        // Center of capsule is filled
        assert_eq!(fb.get_pixel(30, 20), Color::rgb(20, 180, 240));
        // Outer corner (10, 10) of capsule is cut away
        assert_eq!(fb.get_pixel(10, 10), Color::TRANSPARENT);

        // Clipped asymmetric rect: sharp TL (60, 10) is filled, rounded TR (99, 10) is clipped
        assert_eq!(fb.get_pixel(60, 10), Color::rgb(255, 160, 0));
        assert_eq!(fb.get_pixel(99, 10), Color::TRANSPARENT);
        assert_eq!(fb.get_pixel(99, 39), Color::rgb(255, 160, 0));
        assert_eq!(fb.get_pixel(60, 39), Color::TRANSPARENT);
    }

    #[test]
    fn test_canvas_line_cap_and_line_join_and_save_restore() {
        let mut ctx = Canvas2DContext::new(120.0, 120.0);
        assert_eq!(ctx.line_cap(), CanvasLineCap::Butt);
        assert_eq!(ctx.line_join(), CanvasLineJoin::Miter);

        ctx.save();
        ctx.set_line_cap(CanvasLineCap::Square);
        ctx.set_line_join(CanvasLineJoin::Bevel);
        assert_eq!(ctx.line_cap(), CanvasLineCap::Square);
        assert_eq!(ctx.line_join(), CanvasLineJoin::Bevel);
        ctx.restore();
        assert_eq!(ctx.line_cap(), CanvasLineCap::Butt);
        assert_eq!(ctx.line_join(), CanvasLineJoin::Miter);

        // Caps test: horizontal line (20, 30) -> (60, 30), lw = 10 (half_lw = 5)
        let draw_cap = |cap: CanvasLineCap| {
            let mut fb = FrameBuffer::new(90, 60);
            let mut c = Canvas2DContext::new(90.0, 60.0);
            c.set_stroke_style(Color::rgb(255, 0, 0));
            c.set_line_width(10.0);
            c.set_line_cap(cap);
            c.begin_path();
            c.move_to(20.0, 30.0);
            c.line_to(60.0, 30.0);
            c.stroke();
            c.rasterize(&mut fb, 0.0, 0.0, None);
            fb
        };

        let fb_butt = draw_cap(CanvasLineCap::Butt);
        let fb_round = draw_cap(CanvasLineCap::Round);
        let fb_square = draw_cap(CanvasLineCap::Square);

        // Butt stops at x=20
        assert_eq!(fb_butt.get_pixel(18, 30), Color::TRANSPARENT);
        // Round extends along axis to x=17, but NOT out to corner (15, 26) (dist=5.70 > 5)
        assert!(fb_round.get_pixel(17, 30).a > 200);
        assert_eq!(fb_round.get_pixel(15, 26), Color::TRANSPARENT);
        // Square extends to corner (15, 26) (|-4.5| <= 5, |-3.5| <= 5)
        assert!(fb_square.get_pixel(15, 26).a > 200);

        // Joins test: 90-degree corner at (50, 30) with lw = 10 (half_lw = 5)
        let draw_join = |join: CanvasLineJoin| {
            let mut fb = FrameBuffer::new(90, 90);
            let mut c = Canvas2DContext::new(90.0, 90.0);
            c.set_stroke_style(Color::rgb(0, 255, 0));
            c.set_line_width(10.0);
            c.set_line_join(join);
            c.begin_path();
            c.move_to(10.0, 30.0);
            c.line_to(50.0, 30.0);
            c.line_to(50.0, 70.0);
            c.stroke();
            c.rasterize(&mut fb, 0.0, 0.0, None);
            fb
        };

        let fb_miter = draw_join(CanvasLineJoin::Miter);
        let fb_round_j = draw_join(CanvasLineJoin::Round);
        let fb_bevel = draw_join(CanvasLineJoin::Bevel);

        // Miter reaches outer corner (54, 26), while Round & Bevel do not
        assert!(fb_miter.get_pixel(54, 26).a > 200);
        assert_eq!(fb_round_j.get_pixel(54, 26), Color::TRANSPARENT);
        assert_eq!(fb_bevel.get_pixel(54, 26), Color::TRANSPARENT);

        // Round fills (53, 27) (dist ~4.24 < 5), whereas Bevel chord misses it
        assert!(fb_round_j.get_pixel(53, 27).a > 200);
        assert_eq!(fb_bevel.get_pixel(53, 27), Color::TRANSPARENT);
        // Inside bevel triangle (51, 29) is lit
        assert!(fb_bevel.get_pixel(51, 29).a > 200);
    }
}
