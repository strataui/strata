//! Analytical GPU Font Curve Shaders and In-VRAM Text Rasterization (Slug / Loop-Blinn algorithm).
//!
//! Provides hardware-accelerated resolution-independent vector font rendering by evaluating
//! quadratic Bézier curves and winding numbers directly in fragment shaders.

use crate::curve::{QuadraticBezier, Rect2D};
use crate::font_cache::AnalyticGlyph;

/// Embedded WGSL shader for Slug / Loop-Blinn analytical GPU font curve rasterization.
pub const WGSL_SLUG_FONT_SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>, // Unit quad [0, 0] to [1, 1]
};

struct InstanceInput {
    @location(1) bounds: vec4<f32>,      // [x, y, width, height] in screen pixels
    @location(2) color: vec4<f32>,       // Text color RGBA
    @location(3) curve_range: vec2<u32>, // [start_index, count] in glyph curve storage buffer
    @location(4) clip_rect: vec4<f32>,   // Scissor clipping rectangle
    @location(5) glyph_bounds: vec4<f32>,// Normalized [min_x, min_y, max_x, max_y] of glyph design space
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) pixel_pos: vec2<f32>,
    @location(1) glyph_coord: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) @interpolate(flat) curve_range: vec2<u32>,
    @location(4) clip_rect: vec4<f32>,
    @location(5) @interpolate(flat) pixel_to_glyph_scale: f32,
};

struct Uniforms {
    viewport_size: vec2<f32>,
    dpr: f32,
    _pad: f32,
};

struct GpuGlyphCurve {
    p0: vec2<f32>,
    p1: vec2<f32>,
    p2: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<storage, read> glyph_curves: array<GpuGlyphCurve>;

@vertex
fn vs_main(model: VertexInput, instance: InstanceInput) -> VertexOutput {
    var out: VertexOutput;

    let pixel_coords = instance.bounds.xy + model.position * instance.bounds.zw;
    let ndc_x = (pixel_coords.x / uniforms.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (pixel_coords.y / uniforms.viewport_size.y) * 2.0;

    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.pixel_pos = pixel_coords;

    // Map unit quad to normalized glyph design coordinates
    let glyph_size = instance.glyph_bounds.zw - instance.glyph_bounds.xy;
    out.glyph_coord = instance.glyph_bounds.xy + model.position * glyph_size;

    out.color = instance.color;
    out.curve_range = instance.curve_range;
    out.clip_rect = instance.clip_rect;
    out.pixel_to_glyph_scale = glyph_size.y / max(instance.bounds.w, 1.0);

    return out;
}

// Solves quadratic equation A*t^2 + B*t + C = 0 returning number of valid roots in [0, 1]
fn solve_quadratic_roots(a: f32, b: f32, c: f32) -> vec2<f32> {
    if (abs(a) < 1e-6) {
        if (abs(b) < 1e-6) {
            return vec2<f32>(-1.0, -1.0);
        }
        let t = -c / b;
        if (t >= 0.0 && t <= 1.0) {
            return vec2<f32>(t, -1.0);
        }
        return vec2<f32>(-1.0, -1.0);
    }

    let discr = b * b - 4.0 * a * c;
    if (discr < 0.0) {
        return vec2<f32>(-1.0, -1.0);
    }

    let sqrt_d = sqrt(discr);
    let t1 = (-b - sqrt_d) / (2.0 * a);
    let t2 = (-b + sqrt_d) / (2.0 * a);

    var r1 = -1.0;
    var r2 = -1.0;
    if (t1 >= 0.0 && t1 <= 1.0) {
        r1 = t1;
    }
    if (t2 >= 0.0 && t2 <= 1.0) {
        r2 = t2;
    }
    return vec2<f32>(r1, r2);
}

// Distance from point P to quadratic segment
fn dist_to_segment(p: vec2<f32>, p0: vec2<f32>, p2: vec2<f32>) -> f32 {
    let v = p2 - p0;
    let w = p - p0;
    let c1 = dot(w, v);
    let c2 = dot(v, v);
    let t = clamp(c1 / max(c2, 1e-6), 0.0, 1.0);
    let proj = p0 + t * v;
    return length(p - proj);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // 1. Scissor clipping
    if (in.pixel_pos.x < in.clip_rect.x || in.pixel_pos.x > in.clip_rect.x + in.clip_rect.z ||
        in.pixel_pos.y < in.clip_rect.y || in.pixel_pos.y > in.clip_rect.y + in.clip_rect.w) {
        discard;
    }

    let p = in.glyph_coord;
    var winding: i32 = 0;
    var min_dist: f32 = 1e6;

    let start = in.curve_range.x;
    let count = in.curve_range.y;

    for (var i = 0u; i < count; i = i + 1u) {
        let curve = glyph_curves[start + i];
        let p0 = curve.p0;
        let p1 = curve.p1;
        let p2 = curve.p2;

        // Quadratic coefficients for y(t) = P.y
        // y(t) = (1-t)^2 y0 + 2t(1-t) y1 + t^2 y2
        //      = (y0 - 2y1 + y2) t^2 + 2(y1 - y0) t + (y0 - P.y)
        let a = p0.y - 2.0 * p1.y + p2.y;
        let b = 2.0 * (p1.y - p0.y);
        let c = p0.y - p.y;

        let roots = solve_quadratic_roots(a, b, c);

        // Process root 1
        if (roots.x >= 0.0) {
            let t = roots.x;
            let u = 1.0 - t;
            let x = u * u * p0.x + 2.0 * u * t * p1.x + t * t * p2.x;
            if (x > p.x) {
                let dy = 2.0 * a * t + b;
                if (dy > 0.0) {
                    winding = winding + 1;
                } else if (dy < 0.0) {
                    winding = winding - 1;
                }
            }
        }

        // Process root 2
        if (roots.y >= 0.0) {
            let t = roots.y;
            let u = 1.0 - t;
            let x = u * u * p0.x + 2.0 * u * t * p1.x + t * t * p2.x;
            if (x > p.x) {
                let dy = 2.0 * a * t + b;
                if (dy > 0.0) {
                    winding = winding + 1;
                } else if (dy < 0.0) {
                    winding = winding - 1;
                }
            }
        }

        // Compute approximate minimum distance to curve for anti-aliasing
        let d = dist_to_segment(p, p0, p2);
        min_dist = min(min_dist, d);
    }

    // Convert glyph-space distance to screen pixels
    let pixel_dist = min_dist / max(in.pixel_to_glyph_scale, 1e-4);

    let inside = (winding != 0);
    var alpha: f32 = 0.0;
    if (inside) {
        alpha = clamp(0.5 + pixel_dist, 0.0, 1.0);
    } else {
        alpha = clamp(0.5 - pixel_dist, 0.0, 1.0);
    }

    if (alpha <= 0.001) {
        discard;
    }

    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
"#;

/// GPU representation of a glyph quad instance.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpuGlyphInstance {
    pub bounds: [f32; 4],       // [x, y, width, height] in screen pixels
    pub color: [f32; 4],        // RGBA
    pub curve_range: [u32; 2],  // [start_index, count] in curve storage buffer
    pub clip_rect: [f32; 4],    // [x, y, width, height]
    pub glyph_bounds: [f32; 4], // [min_x, min_y, max_x, max_y] design space
}

/// GPU representation of a packed quadratic Bézier curve contour.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpuGlyphCurve {
    pub p0: [f32; 2],
    pub p1: [f32; 2],
    pub p2: [f32; 2],
    pub _pad: [f32; 2],
}

impl GpuGlyphCurve {
    /// Constructs a packed GPU curve from a `QuadraticBezier`.
    pub fn from_quadratic(curve: &QuadraticBezier) -> Self {
        Self {
            p0: [curve.p0.x, curve.p0.y],
            p1: [curve.p1.x, curve.p1.y],
            p2: [curve.p2.x, curve.p2.y],
            _pad: [0.0, 0.0],
        }
    }
}

/// VRAM-ready batch buffer packing analytical glyph instances and curves.
#[derive(Debug, Default, Clone)]
pub struct SlugGlyphBuffer {
    pub instances: Vec<GpuGlyphInstance>,
    pub curves: Vec<GpuGlyphCurve>,
}

impl SlugGlyphBuffer {
    /// Creates a new empty glyph buffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears both instance and curve buffers for the next frame.
    pub fn clear(&mut self) {
        self.instances.clear();
        self.curves.clear();
    }

    /// Appends an analytical glyph to the draw batch.
    pub fn push_glyph(
        &mut self,
        glyph: &AnalyticGlyph,
        screen_bounds: Rect2D,
        color: [f32; 4],
        clip_rect: Rect2D,
    ) {
        let start_index = self.curves.len() as u32;
        let count = glyph.curves.len() as u32;

        for c in &glyph.curves {
            self.curves.push(GpuGlyphCurve::from_quadratic(c));
        }

        let instance = GpuGlyphInstance {
            bounds: [
                screen_bounds.min_x,
                screen_bounds.min_y,
                screen_bounds.width(),
                screen_bounds.height(),
            ],
            color,
            curve_range: [start_index, count],
            clip_rect: [
                clip_rect.min_x,
                clip_rect.min_y,
                clip_rect.width(),
                clip_rect.height(),
            ],
            glyph_bounds: [
                glyph.bounds.min_x,
                glyph.bounds.min_y,
                glyph.bounds.max_x,
                glyph.bounds.max_y,
            ],
        };

        self.instances.push(instance);
    }

    /// Returns byte slice of instance buffer for direct GPU upload.
    pub fn instances_as_bytes(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                self.instances.as_ptr() as *const u8,
                self.instances.len() * std::mem::size_of::<GpuGlyphInstance>(),
            )
        }
    }

    /// Returns byte slice of curve storage buffer for direct GPU upload.
    pub fn curves_as_bytes(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                self.curves.as_ptr() as *const u8,
                self.curves.len() * std::mem::size_of::<GpuGlyphCurve>(),
            )
        }
    }
}
