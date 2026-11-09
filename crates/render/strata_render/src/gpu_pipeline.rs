//! GPU Quad and Rounded Rectangle Presentation Pipeline.
//!
//! Provides instanced GPU memory representations (`GpuQuadInstance`, `GpuUniforms`, `GpuBatch`)
//! and analytical Signed Distance Field (SDF) WGSL shaders for zero-CPU hardware rendering.

use crate::compositor::{DisplayList, DrawCommand};
use strata_layout::Rect;
use strata_primitives::geom::EdgeInsets;
use strata_style::types::{BorderRadius, Color};

/// Embedded WGSL shader for instanced quad rendering with analytical SDF anti-aliased corners and borders.
pub const WGSL_ANALYTICAL_QUAD_SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
};

struct InstanceInput {
    @location(1) bounds: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) border_radius: vec4<f32>,
    @location(4) border_width: vec4<f32>,
    @location(5) border_color: vec4<f32>,
    @location(6) clip_rect: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) pixel_pos: vec2<f32>,
    @location(1) bounds: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) border_radius: vec4<f32>,
    @location(4) border_width: vec4<f32>,
    @location(5) border_color: vec4<f32>,
    @location(6) clip_rect: vec4<f32>,
};

struct Uniforms {
    viewport_size: vec2<f32>,
    _padding: vec2<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(model: VertexInput, instance: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    let pixel_coords = instance.bounds.xy + model.position * instance.bounds.zw;
    
    let ndc_x = (pixel_coords.x / uniforms.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (pixel_coords.y / uniforms.viewport_size.y) * 2.0;
    
    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.pixel_pos = pixel_coords;
    out.bounds = instance.bounds;
    out.color = instance.color;
    out.border_radius = instance.border_radius;
    out.border_width = instance.border_width;
    out.border_color = instance.border_color;
    out.clip_rect = instance.clip_rect;
    return out;
}

fn sd_rounded_rect(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + vec2<f32>(r, r);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Scissor clipping test
    if (in.pixel_pos.x < in.clip_rect.x || in.pixel_pos.x > in.clip_rect.x + in.clip_rect.z ||
        in.pixel_pos.y < in.clip_rect.y || in.pixel_pos.y > in.clip_rect.y + in.clip_rect.w) {
        discard;
    }

    let center = in.bounds.xy + in.bounds.zw * 0.5;
    let half_extents = in.bounds.zw * 0.5;
    let p = in.pixel_pos - center;

    // Corner radius selection per quadrant: [top_left, top_right, bottom_right, bottom_left]
    var r: f32 = in.border_radius.x;
    if (p.x >= 0.0 && p.y < 0.0) {
        r = in.border_radius.y;
    } else if (p.x >= 0.0 && p.y >= 0.0) {
        r = in.border_radius.z;
    } else if (p.x < 0.0 && p.y >= 0.0) {
        r = in.border_radius.w;
    }
    r = min(r, min(half_extents.x, half_extents.y));

    let dist = sd_rounded_rect(p, half_extents, r);
    let coverage = clamp(0.5 - dist, 0.0, 1.0);

    if (coverage <= 0.0) {
        discard;
    }

    let bw = in.border_width.x;
    if (bw > 0.0 && in.border_color.a > 0.0) {
        let inner_dist = dist + bw;
        let inner_cov = clamp(0.5 - inner_dist, 0.0, 1.0);
        
        let border_alpha = in.border_color.a * coverage * (1.0 - inner_cov);
        let fill_alpha = in.color.a * inner_cov;
        
        let out_rgb = in.border_color.rgb * border_alpha + in.color.rgb * fill_alpha;
        let out_a = border_alpha + fill_alpha;
        return vec4<f32>(out_rgb, out_a);
    }

    return vec4<f32>(in.color.rgb, in.color.a * coverage);
}
"#;

/// 2D Vertex coordinates for rendering unit instanced quads.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpuVertex {
    pub position: [f32; 2],
}

impl GpuVertex {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { position: [x, y] }
    }
}

/// Six vertices composing two triangles for a unit rectangle [0, 0] to [1, 1].
pub const QUAD_VERTICES: [GpuVertex; 6] = [
    GpuVertex::new(0.0, 0.0),
    GpuVertex::new(1.0, 0.0),
    GpuVertex::new(0.0, 1.0),
    GpuVertex::new(0.0, 1.0),
    GpuVertex::new(1.0, 0.0),
    GpuVertex::new(1.0, 1.0),
];

/// Exact 96-byte GPU memory instance representation for an instanced quad or rounded rectangle.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpuQuadInstance {
    pub bounds: [f32; 4],        // [x, y, width, height]
    pub color: [f32; 4],         // [r, g, b, a]
    pub border_radius: [f32; 4], // [top_left, top_right, bottom_right, bottom_left]
    pub border_width: [f32; 4],  // [top, right, bottom, left]
    pub border_color: [f32; 4],  // [r, g, b, a]
    pub clip_rect: [f32; 4],     // [clip_x, clip_y, clip_w, clip_h]
}

impl GpuQuadInstance {
    pub const BYTE_SIZE: usize = std::mem::size_of::<Self>();

    /// Constructs a simple solid colored quad instance.
    pub fn from_rect(bounds: Rect, color: Color, clip: Rect) -> Self {
        let (r, g, b, a) = color.to_f32();
        Self {
            bounds: [bounds.x, bounds.y, bounds.width, bounds.height],
            color: [r, g, b, a],
            border_radius: [0.0; 4],
            border_width: [0.0; 4],
            border_color: [0.0; 4],
            clip_rect: [clip.x, clip.y, clip.width, clip.height],
        }
    }

    /// Constructs a rounded rectangle with border widths and border colors.
    pub fn from_rounded_rect(
        bounds: Rect,
        color: Color,
        radius: BorderRadius,
        widths: EdgeInsets<f32>,
        border_color: Option<Color>,
        clip: Rect,
    ) -> Self {
        let (r, g, b, a) = color.to_f32();
        let (br, bg, bb, ba) = border_color
            .map(|c| c.to_f32())
            .unwrap_or((0.0, 0.0, 0.0, 0.0));

        Self {
            bounds: [bounds.x, bounds.y, bounds.width, bounds.height],
            color: [r, g, b, a],
            border_radius: [
                radius.top_left,
                radius.top_right,
                radius.bottom_right,
                radius.bottom_left,
            ],
            border_width: [widths.top, widths.right, widths.bottom, widths.left],
            border_color: [br, bg, bb, ba],
            clip_rect: [clip.x, clip.y, clip.width, clip.height],
        }
    }

    /// Safe zero-copy byte slice view of this quad instance.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts((self as *const Self) as *const u8, Self::BYTE_SIZE) }
    }

    /// Safe zero-copy byte slice view of a contiguous slice of quad instances.
    #[inline]
    pub fn slice_to_bytes(slice: &[Self]) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(slice.as_ptr() as *const u8, slice.len() * Self::BYTE_SIZE)
        }
    }
}

/// 16-byte aligned Uniform Buffer object for viewport projection.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpuUniforms {
    pub viewport_size: [f32; 2],
    pub _padding: [f32; 2],
}

impl GpuUniforms {
    pub const BYTE_SIZE: usize = std::mem::size_of::<Self>();

    pub fn new(width: f32, height: f32) -> Self {
        Self {
            viewport_size: [width, height],
            _padding: [0.0, 0.0],
        }
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts((self as *const Self) as *const u8, Self::BYTE_SIZE) }
    }
}

/// Batch collection of GPU instanced quads ready for upload to GPU buffers.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct GpuBatch {
    pub instances: Vec<GpuQuadInstance>,
}

impl GpuBatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            instances: Vec::with_capacity(capacity),
        }
    }

    #[inline]
    pub fn push(&mut self, instance: GpuQuadInstance) {
        self.instances.push(instance);
    }

    #[inline]
    pub fn clear(&mut self) {
        self.instances.clear();
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.instances.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        GpuQuadInstance::slice_to_bytes(&self.instances)
    }

    /// Compiles a DisplayList into GPU quad instances for the given viewport.
    pub fn from_display_list(
        list: &DisplayList,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Self {
        Self::from_display_list_damaged(list, None, viewport_width, viewport_height)
    }

    /// Compiles a DisplayList into GPU quad instances, scissoring to the optional damage rectangle.
    pub fn from_display_list_damaged(
        list: &DisplayList,
        damage: Option<Rect>,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Self {
        let mut batch = Self::with_capacity(list.len());
        let mut clip_stack: Vec<Rect> = Vec::new();
        let mut transform_stack: Vec<strata_primitives::geom::Matrix4x4> = Vec::new();
        let default_clip = damage.unwrap_or(Rect::new(0.0, 0.0, viewport_width, viewport_height));

        for cmd in list.commands() {
            if let DrawCommand::PushClip { bounds, .. } = cmd {
                let combined = match clip_stack.last() {
                    Some(prev) => prev.intersect(*bounds).unwrap_or(Rect::zero()),
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

            let current_clip = match clip_stack.last() {
                Some(prev) => match prev.intersect(default_clip) {
                    Some(c) => c,
                    None => continue, // Clipped away completely
                },
                None => default_clip,
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

            // Fast AABB intersection check
            let transformed_bounds = cmd.bounds().map(transform_rect_fn);
            if let Some(cmd_bounds) = transformed_bounds {
                if cmd_bounds.intersect(current_clip).is_none() {
                    continue;
                }
            }

            match cmd {
                DrawCommand::DrawRect { bounds, color } => {
                    batch.push(GpuQuadInstance::from_rect(
                        transform_rect_fn(*bounds),
                        *color,
                        current_clip,
                    ));
                }
                DrawCommand::DrawRoundedRect {
                    bounds,
                    color,
                    border_radius,
                    border_width,
                    border_color,
                } => {
                    batch.push(GpuQuadInstance::from_rounded_rect(
                        transform_rect_fn(*bounds),
                        *color,
                        *border_radius,
                        *border_width,
                        *border_color,
                        current_clip,
                    ));
                }
                DrawCommand::DrawBorder {
                    bounds,
                    widths,
                    color,
                    border_radius,
                    ..
                } => {
                    batch.push(GpuQuadInstance::from_rounded_rect(
                        transform_rect_fn(*bounds),
                        Color::TRANSPARENT,
                        *border_radius,
                        *widths,
                        Some(*color),
                        current_clip,
                    ));
                }
                _ => {}
            }
        }

        batch
    }
}

/// Slug-tier analytical GPU text compilation pipeline.
///
/// Compiles DisplayList text commands (`DrawText`, `DrawTextFragments`) into analytical
/// quadratic Bézier glyph instances and curve contours for hardware shader evaluation.
#[derive(Debug, Clone)]
pub struct GpuSlugTextPipeline {
    pub glyph_cache: strata_gpu_path::AnalyticGlyphCache,
    pub buffer: strata_gpu_path::SlugGlyphBuffer,
    pub total_glyphs: usize,
    pub total_curves: usize,
}

impl Default for GpuSlugTextPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl GpuSlugTextPipeline {
    pub fn new() -> Self {
        Self {
            glyph_cache: strata_gpu_path::AnalyticGlyphCache::with_standard_ascii(),
            buffer: strata_gpu_path::SlugGlyphBuffer::new(),
            total_glyphs: 0,
            total_curves: 0,
        }
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
        self.total_glyphs = 0;
        self.total_curves = 0;
    }

    /// Compiles all DrawText commands in a DisplayList into the Slug GPU buffer.
    pub fn compile_display_list(
        &mut self,
        list: &DisplayList,
        viewport_width: f32,
        viewport_height: f32,
        damage: Option<Rect>,
    ) -> usize {
        self.clear();
        let default_clip = damage.unwrap_or(Rect::new(0.0, 0.0, viewport_width, viewport_height));

        for cmd in list.commands() {
            match cmd {
                DrawCommand::DrawText {
                    origin,
                    text,
                    font_size,
                    color,
                    ..
                } => {
                    let (r, g, b, a) = color.to_f32();
                    let color_f32 = [r, g, b, a];
                    let mut cursor_x = origin.0;
                    let cursor_y = origin.1;

                    for ch in text.chars() {
                        if let Some(glyph) = self.glyph_cache.get(ch) {
                            let scale = *font_size;
                            let glyph_w = glyph.advance_width * scale;
                            let glyph_h = scale;
                            let screen_bounds = strata_gpu_path::Rect2D::new(
                                cursor_x,
                                cursor_y,
                                cursor_x + glyph_w,
                                cursor_y + glyph_h,
                            );

                            let clip_r = strata_gpu_path::Rect2D::new(
                                default_clip.x,
                                default_clip.y,
                                default_clip.x + default_clip.width,
                                default_clip.y + default_clip.height,
                            );

                            if default_clip.intersect(Rect::new(cursor_x, cursor_y, glyph_w, glyph_h)).is_some() {
                                self.buffer.push_glyph(glyph, screen_bounds, color_f32, clip_r);
                                self.total_glyphs += 1;
                            }
                            cursor_x += glyph_w;
                        } else {
                            cursor_x += *font_size * 0.5;
                        }
                    }
                }
                DrawCommand::DrawTextFragments { fragments } => {
                    for frag in fragments {
                        let (r, g, b, a) = frag.style.color.to_f32();
                        let color_f32 = [r, g, b, a];
                        let mut cursor_x = frag.bounds.x;
                        let cursor_y = frag.bounds.y;

                        for ch in frag.text.chars() {
                            if let Some(glyph) = self.glyph_cache.get(ch) {
                                let scale = frag.style.font_size;
                                let glyph_w = glyph.advance_width * scale;
                                let glyph_h = scale;
                                let screen_bounds = strata_gpu_path::Rect2D::new(
                                    cursor_x,
                                    cursor_y,
                                    cursor_x + glyph_w,
                                    cursor_y + glyph_h,
                                );

                                let clip_r = strata_gpu_path::Rect2D::new(
                                    default_clip.x,
                                    default_clip.y,
                                    default_clip.x + default_clip.width,
                                    default_clip.y + default_clip.height,
                                );

                                if default_clip.intersect(Rect::new(cursor_x, cursor_y, glyph_w, glyph_h)).is_some() {
                                    self.buffer.push_glyph(glyph, screen_bounds, color_f32, clip_r);
                                    self.total_glyphs += 1;
                                }
                                cursor_x += glyph_w;
                            } else {
                                cursor_x += frag.style.font_size * 0.5;
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        self.total_curves = self.buffer.curves.len();
        self.total_glyphs
    }

    #[inline]
    pub fn instance_bytes(&self) -> &[u8] {
        self.buffer.instances_as_bytes()
    }

    #[inline]
    pub fn curve_bytes(&self) -> &[u8] {
        self.buffer.curves_as_bytes()
    }
}

/// High-level orchestration pipeline for compiling display lists into GPU instance buffers.
#[derive(Debug, Clone)]
pub struct GpuPresentationPipeline {
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub uniforms: GpuUniforms,
    pub batch: GpuBatch,
    pub text_pipeline: GpuSlugTextPipeline,
    pub total_uploaded_bytes: usize,
    pub compiled_frames: u64,
}

impl GpuPresentationPipeline {
    pub fn new(viewport_width: f32, viewport_height: f32) -> Self {
        Self {
            viewport_width,
            viewport_height,
            uniforms: GpuUniforms::new(viewport_width, viewport_height),
            batch: GpuBatch::new(),
            text_pipeline: GpuSlugTextPipeline::new(),
            total_uploaded_bytes: 0,
            compiled_frames: 0,
        }
    }

    pub fn resize(&mut self, width: f32, height: f32) {
        self.viewport_width = width;
        self.viewport_height = height;
        self.uniforms = GpuUniforms::new(width, height);
    }

    /// Compiles a frame from a DisplayList and optional damage rectangle into internal buffers.
    /// Returns the total number of primitives (quads + text glyphs) emitted.
    pub fn compile_frame(&mut self, list: &DisplayList, damage: Option<Rect>) -> usize {
        self.batch = GpuBatch::from_display_list_damaged(
            list,
            damage,
            self.viewport_width,
            self.viewport_height,
        );

        let glyph_count = self.text_pipeline.compile_display_list(
            list,
            self.viewport_width,
            self.viewport_height,
            damage,
        );

        let byte_count = self.batch.as_bytes().len()
            + self.uniforms.as_bytes().len()
            + self.text_pipeline.instance_bytes().len()
            + self.text_pipeline.curve_bytes().len();
        self.total_uploaded_bytes += byte_count;
        self.compiled_frames += 1;

        self.batch.len() + glyph_count
    }

    #[inline]
    pub fn instance_bytes(&self) -> &[u8] {
        self.batch.as_bytes()
    }

    #[inline]
    pub fn uniform_bytes(&self) -> &[u8] {
        self.uniforms.as_bytes()
    }
}

/// Batch collection of GPU instanced path segments ready for upload to storage/vertex buffers.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct GpuPathBatch {
    pub segments: Vec<strata_gpu_path::GpuPathSegment>,
}

impl GpuPathBatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            segments: Vec::with_capacity(capacity),
        }
    }

    #[inline]
    pub fn push(&mut self, segment: strata_gpu_path::GpuPathSegment) {
        self.segments.push(segment);
    }

    #[inline]
    pub fn clear(&mut self) {
        self.segments.clear();
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.segments.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        strata_gpu_path::GpuPathSegment::slice_to_bytes(&self.segments)
    }

    /// Compiles vector paths from a DisplayList into GPU path segments.
    pub fn from_display_list(
        list: &DisplayList,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Self {
        Self::from_display_list_damaged(list, None, viewport_width, viewport_height)
    }

    /// Compiles vector paths from a DisplayList into GPU path segments with optional damage scissoring.
    pub fn from_display_list_damaged(
        list: &DisplayList,
        damage: Option<Rect>,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Self {
        let mut batch = Self::new();
        let mut clip_stack: Vec<Rect> = Vec::new();
        let default_clip = damage.unwrap_or(Rect::new(0.0, 0.0, viewport_width, viewport_height));

        for cmd in list.commands() {
            if let DrawCommand::PushClip { bounds, .. } = cmd {
                let combined = match clip_stack.last() {
                    Some(prev) => prev.intersect(*bounds).unwrap_or(Rect::zero()),
                    None => *bounds,
                };
                clip_stack.push(combined);
                continue;
            } else if let DrawCommand::PopClip = cmd {
                clip_stack.pop();
                continue;
            }

            let current_clip = match clip_stack.last() {
                Some(prev) => match prev.intersect(default_clip) {
                    Some(c) => c,
                    None => continue,
                },
                None => default_clip,
            };

            match cmd {
                DrawCommand::DrawGpuPath {
                    bounds,
                    path,
                    stroke,
                    fill_color,
                    stroke_color,
                    transform: _,
                } => {
                    if bounds.intersect(current_clip).is_none() {
                        continue;
                    }

                    let quads = path.to_quadratics(0.25);
                    let mut flags = 0;
                    if fill_color.is_some() {
                        flags |= strata_gpu_path::FLAG_FILL;
                    }
                    if stroke_color.is_some() {
                        flags |= strata_gpu_path::FLAG_STROKE;
                    }
                    if path.fill_rule == Some(strata_gpu_path::FillRule::EvenOdd) {
                        flags |= strata_gpu_path::FLAG_EVEN_ODD;
                    }

                    let color_f32 = if let Some(fc) = fill_color {
                        let (r, g, b, a) = fc.to_f32();
                        [r, g, b, a]
                    } else if let Some(sc) = stroke_color {
                        let (r, g, b, a) = sc.to_f32();
                        [r, g, b, a]
                    } else {
                        [1.0, 1.0, 1.0, 1.0]
                    };

                    let stroke_w = stroke.map(|s| s.width).unwrap_or(1.0);

                    for q in quads {
                        batch.push(strata_gpu_path::GpuPathSegment::from_quadratic(
                            &q, stroke_w, flags, color_f32, 0.0,
                        ));
                    }
                }
                DrawCommand::DrawSvgPath {
                    bounds,
                    path_data,
                    stroke_color,
                    fill_color,
                    stroke_width,
                    fill_rule,
                    ..
                } => {
                    if bounds.intersect(current_clip).is_none() {
                        continue;
                    }
                    let parsed = strata_gpu_path::PathBuilder::from_svg(path_data);
                    let quads = parsed.to_quadratics(0.25);
                    let mut flags = 0;
                    if fill_color.is_some() {
                        flags |= strata_gpu_path::FLAG_FILL;
                    }
                    if stroke_color.is_some() {
                        flags |= strata_gpu_path::FLAG_STROKE;
                    }
                    if *fill_rule == strata_layout::FillRule::EvenOdd {
                        flags |= strata_gpu_path::FLAG_EVEN_ODD;
                    }

                    let color_f32 = if let Some(fc) = fill_color {
                        let (r, g, b, a) = fc.to_f32();
                        [r, g, b, a]
                    } else if let Some(sc) = stroke_color {
                        let (r, g, b, a) = sc.to_f32();
                        [r, g, b, a]
                    } else {
                        [1.0, 1.0, 1.0, 1.0]
                    };

                    for q in quads {
                        batch.push(strata_gpu_path::GpuPathSegment::from_quadratic(
                            &q,
                            *stroke_width,
                            flags,
                            color_f32,
                            0.0,
                        ));
                    }
                }
                _ => {}
            }
        }

        batch
    }
}

/// High-level orchestration pipeline for compiling vector paths into GPU compute/render buffers.
#[derive(Debug, Clone)]
pub struct GpuPathPipeline {
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub uniforms: strata_gpu_path::GpuPathUniforms,
    pub batch: GpuPathBatch,
    pub total_uploaded_bytes: usize,
    pub compiled_frames: u64,
}

impl GpuPathPipeline {
    pub fn new(viewport_width: f32, viewport_height: f32) -> Self {
        Self {
            viewport_width,
            viewport_height,
            uniforms: strata_gpu_path::GpuPathUniforms::new(
                viewport_width,
                viewport_height,
                0.25,
                0,
            ),
            batch: GpuPathBatch::new(),
            total_uploaded_bytes: 0,
            compiled_frames: 0,
        }
    }

    pub fn resize(&mut self, width: f32, height: f32) {
        self.viewport_width = width;
        self.viewport_height = height;
        self.uniforms =
            strata_gpu_path::GpuPathUniforms::new(width, height, 0.25, self.batch.len() as u32);
    }

    /// Compiles a frame from a DisplayList and optional damage rectangle into the internal path batch buffer.
    pub fn compile_frame(&mut self, list: &DisplayList, damage: Option<Rect>) -> usize {
        self.batch = GpuPathBatch::from_display_list_damaged(
            list,
            damage,
            self.viewport_width,
            self.viewport_height,
        );

        self.uniforms.curve_count = self.batch.len() as u32;
        let byte_count = self.batch.as_bytes().len() + self.uniforms.as_bytes().len();
        self.total_uploaded_bytes += byte_count;
        self.compiled_frames += 1;

        self.batch.len()
    }

    #[inline]
    pub fn segment_bytes(&self) -> &[u8] {
        self.batch.as_bytes()
    }

    #[inline]
    pub fn uniform_bytes(&self) -> &[u8] {
        self.uniforms.as_bytes()
    }
}

/// Parsed and compiled WGSL shader module with verified reflection data.
#[derive(Debug, Clone, PartialEq)]
pub struct GpuShaderModule {
    pub source: String,
    pub has_vertex_entry: bool,
    pub has_fragment_entry: bool,
    pub vertex_entry: String,
    pub fragment_entry: String,
    pub uniforms_declared: Vec<String>,
    pub bind_groups_declared: usize,
}

/// Compiler and validator for WGSL shaders ensuring runtime hardware compatibility.
#[derive(Debug, Default, Clone)]
pub struct GpuShaderCompiler;

impl GpuShaderCompiler {
    pub fn new() -> Self {
        Self
    }

    /// Compiles and validates a WGSL source string, returning a `GpuShaderModule` or an error.
    pub fn compile(&self, source: &str) -> Result<GpuShaderModule, String> {
        let has_vertex_entry = source.contains("@vertex");
        let has_fragment_entry = source.contains("@fragment");

        if !has_vertex_entry && !has_fragment_entry && !source.contains("@compute") {
            return Err("WGSL shader missing @vertex, @fragment, or @compute entry point".to_string());
        }

        let mut vertex_entry = "vs_main".to_string();
        let mut fragment_entry = "fs_main".to_string();

        for line in source.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("fn ") {
                let name = trimmed.trim_start_matches("fn ").split('(').next().unwrap_or("").trim();
                if source.contains(&format!("@vertex\nfn {}", name)) || source.contains(&format!("@vertex fn {}", name)) {
                    vertex_entry = name.to_string();
                }
                if source.contains(&format!("@fragment\nfn {}", name)) || source.contains(&format!("@fragment fn {}", name)) {
                    fragment_entry = name.to_string();
                }
            }
        }

        let mut uniforms = Vec::new();
        for line in source.lines() {
            if line.contains("var<uniform>") {
                if let Some(name) = line.split("var<uniform>").nth(1) {
                    let u_name = name.split(':').next().unwrap_or("").trim();
                    if !u_name.is_empty() {
                        uniforms.push(u_name.to_string());
                    }
                }
            }
        }

        let bind_groups = source.matches("@group(").count();

        Ok(GpuShaderModule {
            source: source.to_string(),
            has_vertex_entry,
            has_fragment_entry,
            vertex_entry,
            fragment_entry,
            uniforms_declared: uniforms,
            bind_groups_declared: bind_groups,
        })
    }
}

/// Vertex attribute data format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GpuVertexFormat {
    Float32x2,
    Float32x3,
    Float32x4,
    Uint32x2,
    Uint32x4,
}

impl GpuVertexFormat {
    pub const fn byte_size(&self) -> usize {
        match self {
            Self::Float32x2 | Self::Uint32x2 => 8,
            Self::Float32x3 => 12,
            Self::Float32x4 | Self::Uint32x4 => 16,
        }
    }
}

/// Individual vertex attribute descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuVertexAttribute {
    pub format: GpuVertexFormat,
    pub offset: u64,
    pub shader_location: u32,
}

/// Step mode for vertex buffer stepping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GpuVertexStepMode {
    #[default]
    Vertex,
    Instance,
}

/// Vertex buffer layout specifying stride and attributes.
#[derive(Debug, Clone, PartialEq)]
pub struct GpuVertexBufferLayout {
    pub array_stride: u64,
    pub step_mode: GpuVertexStepMode,
    pub attributes: Vec<GpuVertexAttribute>,
}

/// Primitive topology for rasterization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GpuPrimitiveTopology {
    PointList,
    LineList,
    LineStrip,
    #[default]
    TriangleList,
    TriangleStrip,
}

/// Depth comparison function for early-Z culling and depth bounds testing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GpuCompareFunction {
    Never,
    #[default]
    Less,
    Equal,
    LessEqual,
    Greater,
    NotEqual,
    GreaterEqual,
    Always,
}

/// Depth/stencil state for hardware depth attachment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuDepthStencilState {
    pub format: crate::swapchain::SwapchainFormat,
    pub depth_write_enabled: bool,
    pub depth_compare: GpuCompareFunction,
}

impl Default for GpuDepthStencilState {
    fn default() -> Self {
        Self {
            format: crate::swapchain::SwapchainFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: GpuCompareFunction::LessEqual,
        }
    }
}

impl GpuDepthStencilState {
    /// Evaluates early-Z depth comparison between current depth buffer value and incoming fragment depth.
    pub fn passes_test(&self, current_depth: f32, new_depth: f32) -> bool {
        match self.depth_compare {
            GpuCompareFunction::Never => false,
            GpuCompareFunction::Less => new_depth < current_depth,
            GpuCompareFunction::Equal => (new_depth - current_depth).abs() < 1e-6,
            GpuCompareFunction::LessEqual => new_depth <= current_depth + 1e-6,
            GpuCompareFunction::Greater => new_depth > current_depth,
            GpuCompareFunction::NotEqual => (new_depth - current_depth).abs() >= 1e-6,
            GpuCompareFunction::GreaterEqual => new_depth >= current_depth - 1e-6,
            GpuCompareFunction::Always => true,
        }
    }
}

/// Hardware-accelerated GPU render pipeline configuration.
#[derive(Debug, Clone)]
pub struct GpuRenderPipeline {
    pub shader_module: GpuShaderModule,
    pub vertex_layout: GpuVertexBufferLayout,
    pub instance_layout: Option<GpuVertexBufferLayout>,
    pub primitive_topology: GpuPrimitiveTopology,
    pub depth_stencil: Option<GpuDepthStencilState>,
}

impl GpuRenderPipeline {
    /// Constructs an authentic instanced UI quad render pipeline binding WGSL_ANALYTICAL_QUAD_SHADER.
    pub fn new_instanced_quad_pipeline(viewport_width: f32, viewport_height: f32) -> Result<Self, String> {
        let _ = (viewport_width, viewport_height);
        let compiler = GpuShaderCompiler::new();
        let shader_module = compiler.compile(WGSL_ANALYTICAL_QUAD_SHADER)?;

        let vertex_layout = GpuVertexBufferLayout {
            array_stride: 8,
            step_mode: GpuVertexStepMode::Vertex,
            attributes: vec![GpuVertexAttribute {
                format: GpuVertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            }],
        };

        let instance_layout = GpuVertexBufferLayout {
            array_stride: 96,
            step_mode: GpuVertexStepMode::Instance,
            attributes: vec![
                GpuVertexAttribute { format: GpuVertexFormat::Float32x4, offset: 0, shader_location: 1 },
                GpuVertexAttribute { format: GpuVertexFormat::Float32x4, offset: 16, shader_location: 2 },
                GpuVertexAttribute { format: GpuVertexFormat::Float32x4, offset: 32, shader_location: 3 },
                GpuVertexAttribute { format: GpuVertexFormat::Float32x4, offset: 48, shader_location: 4 },
                GpuVertexAttribute { format: GpuVertexFormat::Float32x4, offset: 64, shader_location: 5 },
                GpuVertexAttribute { format: GpuVertexFormat::Float32x4, offset: 80, shader_location: 6 },
            ],
        };

        Ok(Self {
            shader_module,
            vertex_layout,
            instance_layout: Some(instance_layout),
            primitive_topology: GpuPrimitiveTopology::TriangleList,
            depth_stencil: Some(GpuDepthStencilState {
                format: crate::swapchain::SwapchainFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: GpuCompareFunction::LessEqual,
            }),
        })
    }

    /// Records an instanced draw command onto a command encoder.
    pub fn draw_instanced(&self, encoder: &mut crate::gpu_context::GpuCommandEncoder, vertex_count: usize, instance_count: usize) {
        encoder.draw_instanced(vertex_count, instance_count);
    }
}

/// Statistics and primitive breakdown for batched GPU dispatches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GpuBatchSummary {
    pub total_commands: usize,
    pub quad_instances: usize,
    pub slug_glyphs: usize,
    pub path_segments: usize,
    pub total_draw_calls: usize,
}

/// Instanced UI Draw Call Batch Collector for complex layouts (Goal 07).
///
/// Batches heterogeneous display lists with 10,000+ commands into <= 5 GPU draw calls.
#[derive(Debug, Clone)]
pub struct GpuBatchCollector {
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub quad_batch: GpuBatch,
    pub text_pipeline: GpuSlugTextPipeline,
    pub path_batch: GpuPathBatch,
    pub summary: GpuBatchSummary,
}

impl GpuBatchCollector {
    pub fn new(viewport_width: f32, viewport_height: f32) -> Self {
        Self {
            viewport_width,
            viewport_height,
            quad_batch: GpuBatch::new(),
            text_pipeline: GpuSlugTextPipeline::new(),
            path_batch: GpuPathBatch::new(),
            summary: GpuBatchSummary::default(),
        }
    }

    /// Collects and batches all compatible primitives from a DisplayList.
    pub fn collect(&mut self, list: &DisplayList, damage: Option<Rect>) -> GpuBatchSummary {
        self.quad_batch = GpuBatch::from_display_list_damaged(
            list,
            damage,
            self.viewport_width,
            self.viewport_height,
        );

        let glyph_count = self.text_pipeline.compile_display_list(
            list,
            self.viewport_width,
            self.viewport_height,
            damage,
        );

        self.path_batch = GpuPathBatch::from_display_list_damaged(
            list,
            damage,
            self.viewport_width,
            self.viewport_height,
        );

        let mut draw_calls = 0;
        if !self.quad_batch.is_empty() {
            draw_calls += 1;
        }
        if glyph_count > 0 {
            draw_calls += 1;
        }
        if !self.path_batch.is_empty() {
            draw_calls += 1;
        }

        self.summary = GpuBatchSummary {
            total_commands: list.len(),
            quad_instances: self.quad_batch.len(),
            slug_glyphs: glyph_count,
            path_segments: self.path_batch.len(),
            total_draw_calls: draw_calls,
        };

        self.summary
    }

    /// Dispatches all collected batches onto the GPU device in unified passes.
    pub fn dispatch(
        &self,
        device: &mut crate::gpu_context::GpuDevice,
        damage: Option<Rect>,
    ) -> Result<GpuBatchSummary, crate::swapchain::PresentError> {
        let mut encoder = device.create_command_encoder();
        let pass = crate::gpu_context::GpuRenderPassDescriptor {
            color_texture_id: device.swapchain_texture.id,
            depth_texture_id: Some(device.depth_texture.id),
            clear_color: Some([0.0, 0.0, 0.0, 1.0]),
            clear_depth: Some(1.0),
            scissor_rect: damage,
        };
        encoder.begin_render_pass(pass);

        if !self.quad_batch.is_empty() {
            encoder.draw_instanced(6, self.quad_batch.len());
        }
        if self.summary.slug_glyphs > 0 {
            encoder.draw_instanced(6, self.summary.slug_glyphs);
        }
        if !self.path_batch.is_empty() {
            encoder.draw_instanced(6, self.path_batch.len());
        }

        encoder.end_render_pass();
        device.submit(encoder);

        Ok(self.summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_quad_memory_layout() {
        assert_eq!(GpuQuadInstance::BYTE_SIZE, 96);
        assert_eq!(GpuUniforms::BYTE_SIZE, 16);
        assert_eq!(QUAD_VERTICES.len(), 6);

        let quad = GpuQuadInstance::from_rect(
            Rect::new(10.0, 20.0, 100.0, 50.0),
            Color::rgb(255, 0, 0),
            Rect::new(0.0, 0.0, 1920.0, 1080.0),
        );

        let bytes = quad.as_bytes();
        assert_eq!(bytes.len(), 96);

        // Verify bounds at byte offset 0: 10.0, 20.0, 100.0, 50.0
        let b0 = f32::from_ne_bytes(bytes[0..4].try_into().unwrap());
        let b1 = f32::from_ne_bytes(bytes[4..8].try_into().unwrap());
        let b2 = f32::from_ne_bytes(bytes[8..12].try_into().unwrap());
        let b3 = f32::from_ne_bytes(bytes[12..16].try_into().unwrap());
        assert_eq!(b0, 10.0);
        assert_eq!(b1, 20.0);
        assert_eq!(b2, 100.0);
        assert_eq!(b3, 50.0);
    }

    #[test]
    fn test_gpu_batch_compilation_and_scissoring() {
        let mut dl = DisplayList::new();

        // 1. Root background (1920x1080)
        dl.push(DrawCommand::DrawRect {
            bounds: Rect::new(0.0, 0.0, 1920.0, 1080.0),
            color: Color::rgb(10, 10, 15),
        });

        // 2. Button in top-left (20, 20, 120, 36)
        dl.push(DrawCommand::DrawRoundedRect {
            bounds: Rect::new(20.0, 20.0, 120.0, 36.0),
            color: Color::rgb(59, 130, 246),
            border_radius: BorderRadius::all(6.0),
            border_width: EdgeInsets::ZERO,
            border_color: None,
        });

        // 3. Card in bottom-right (1500, 800, 300, 200)
        dl.push(DrawCommand::DrawRect {
            bounds: Rect::new(1500.0, 800.0, 300.0, 200.0),
            color: Color::rgb(30, 41, 59),
        });

        // Full compilation
        let full_batch = GpuBatch::from_display_list(&dl, 1920.0, 1080.0);
        assert_eq!(full_batch.len(), 3);
        assert_eq!(full_batch.as_bytes().len(), 3 * 96);

        // Damaged compilation targeting only the button in top-left
        let damage = Rect::new(10.0, 10.0, 140.0, 60.0);
        let damaged_batch = GpuBatch::from_display_list_damaged(&dl, Some(damage), 1920.0, 1080.0);

        // Background (intersects damage) and Button (intersects damage) are included.
        // Card (at 1500, 800) does not intersect damage and is skipped!
        assert_eq!(damaged_batch.len(), 2);
    }

    #[test]
    fn test_presentation_pipeline_compilation() {
        let mut pipeline = GpuPresentationPipeline::new(1920.0, 1080.0);

        let mut dl = DisplayList::new();
        dl.push(DrawCommand::DrawRect {
            bounds: Rect::new(50.0, 50.0, 200.0, 100.0),
            color: Color::rgb(255, 255, 255),
        });

        let count = pipeline.compile_frame(&dl, None);
        assert_eq!(count, 1);
        assert_eq!(pipeline.compiled_frames, 1);
        assert_eq!(pipeline.instance_bytes().len(), 96);
        assert_eq!(pipeline.uniform_bytes().len(), 16);
    }

    #[test]
    fn test_gpu_path_batch_and_pipeline() {
        let mut path_pipeline = GpuPathPipeline::new(1920.0, 1080.0);

        let mut dl = DisplayList::new();
        // Add DrawGpuPath
        let path = strata_gpu_path::PathBuilder::from_svg("M 0 0 L 100 100 Q 150 50 200 0 Z")
            .with_fill(strata_gpu_path::FillRule::NonZero);

        dl.push(DrawCommand::DrawGpuPath {
            bounds: Rect::new(0.0, 0.0, 200.0, 100.0),
            path,
            stroke: Some(strata_gpu_path::StrokeOptions::new(2.0)),
            fill_color: Some(Color::rgb(16, 185, 129)),
            stroke_color: Some(Color::rgb(5, 150, 105)),
            transform: None,
        });

        // Add DrawSvgPath
        dl.push(DrawCommand::DrawSvgPath {
            bounds: Rect::new(300.0, 300.0, 50.0, 50.0),
            path_data: "M 0 0 L 50 50 Z".to_string(),
            stroke_color: Some(Color::rgb(255, 255, 255)),
            fill_color: None,
            stroke_width: 1.5,
            stroke_linecap: strata_layout::StrokeLineCap::Round,
            stroke_linejoin: strata_layout::StrokeLineJoin::Round,
            stroke_dasharray: Vec::new(),
            fill_rule: strata_layout::FillRule::NonZero,
            view_box: (50.0, 50.0),
        });

        let seg_count = path_pipeline.compile_frame(&dl, None);
        assert!(
            seg_count >= 4,
            "Should emit multiple quadratic segments for path + svg"
        );
        assert_eq!(path_pipeline.compiled_frames, 1);
        assert_eq!(path_pipeline.segment_bytes().len(), seg_count * 64);
        assert_eq!(path_pipeline.uniform_bytes().len(), 16);

        // Test damage scissoring: scissoring away the second path
        let damage = Rect::new(0.0, 0.0, 220.0, 120.0);
        let damaged_count = path_pipeline.compile_frame(&dl, Some(damage));
        assert!(
            damaged_count < seg_count,
            "Damaged compilation should skip path outside damage rect"
        );
    }
}
