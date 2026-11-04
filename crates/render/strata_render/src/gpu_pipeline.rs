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
        let (br, bg, bb, ba) = border_color.map(|c| c.to_f32()).unwrap_or((0.0, 0.0, 0.0, 0.0));

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
        unsafe {
            std::slice::from_raw_parts(
                (self as *const Self) as *const u8,
                Self::BYTE_SIZE,
            )
        }
    }

    /// Safe zero-copy byte slice view of a contiguous slice of quad instances.
    #[inline]
    pub fn slice_to_bytes(slice: &[Self]) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                slice.as_ptr() as *const u8,
                slice.len() * Self::BYTE_SIZE,
            )
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
        unsafe {
            std::slice::from_raw_parts(
                (self as *const Self) as *const u8,
                Self::BYTE_SIZE,
            )
        }
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
    pub fn from_display_list(list: &DisplayList, viewport_width: f32, viewport_height: f32) -> Self {
        Self::from_display_list_damaged(
            list,
            None,
            viewport_width,
            viewport_height,
        )
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
                    batch.push(GpuQuadInstance::from_rect(transform_rect_fn(*bounds), *color, current_clip));
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

/// High-level orchestration pipeline for compiling display lists into GPU instance buffers.
#[derive(Debug, Clone)]
pub struct GpuPresentationPipeline {
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub uniforms: GpuUniforms,
    pub batch: GpuBatch,
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
            total_uploaded_bytes: 0,
            compiled_frames: 0,
        }
    }

    pub fn resize(&mut self, width: f32, height: f32) {
        self.viewport_width = width;
        self.viewport_height = height;
        self.uniforms = GpuUniforms::new(width, height);
    }

    /// Compiles a frame from a DisplayList and optional damage rectangle into the internal batch buffer.
    /// Returns the number of instanced quads emitted.
    pub fn compile_frame(&mut self, list: &DisplayList, damage: Option<Rect>) -> usize {
        self.batch = GpuBatch::from_display_list_damaged(
            list,
            damage,
            self.viewport_width,
            self.viewport_height,
        );

        let byte_count = self.batch.as_bytes().len() + self.uniforms.as_bytes().len();
        self.total_uploaded_bytes += byte_count;
        self.compiled_frames += 1;

        self.batch.len()
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
}
