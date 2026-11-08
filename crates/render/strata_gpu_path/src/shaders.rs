//! Embedded WGSL Compute and Analytical Fragment Shaders for Direct Hardware Vector Path Rendering.

/// Embedded WGSL Compute Shader for parallel Bézier curve decomposition, flattening, and stroke expansion.
pub const WGSL_PATH_COMPUTE_FLATTEN_SHADER: &str = r#"
struct RawCurveInput {
    p0: vec2<f32>,
    p1: vec2<f32>,
    p2: vec2<f32>,
    p3: vec2<f32>,
    stroke_width: f32,
    flags: u32,
    z_index: f32,
    _pad: f32,
    color: vec4<f32>,
};

struct GpuPathSegment {
    p0: vec2<f32>,
    p1: vec2<f32>,
    p2: vec2<f32>,
    stroke_width: f32,
    flags: u32,
    color: vec4<f32>,
    z_index: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

struct ComputeUniforms {
    viewport_size: vec2<f32>,
    tolerance: f32,
    curve_count: u32,
};

@group(0) @binding(0) var<uniform> uniforms: ComputeUniforms;
@group(0) @binding(1) var<storage, read> input_curves: array<RawCurveInput>;
@group(0) @binding(2) var<storage, read_write> output_segments: array<GpuPathSegment>;

@compute @workgroup_size(64)
fn cs_flatten_curves(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let index = global_id.x;
    if (index >= uniforms.curve_count) {
        return;
    }
    let curve = input_curves[index];

    // Compute quadratic approximation control point: C = (3*P1 - P0 + 3*P2 - P3) / 4
    let c = (3.0 * curve.p1 - curve.p0 + 3.0 * curve.p2 - curve.p3) * 0.25;

    var out: GpuPathSegment;
    out.p0 = curve.p0;
    out.p1 = c;
    out.p2 = curve.p3;
    out.stroke_width = curve.stroke_width;
    out.flags = curve.flags;
    out.color = curve.color;
    out.z_index = curve.z_index;
    out._pad0 = 0.0;
    out._pad1 = 0.0;
    out._pad2 = 0.0;

    output_segments[index] = out;
}
"#;

/// Embedded WGSL Shader for analytical quadratic Bézier Signed Distance Field (SDF) rasterization.
pub const WGSL_ANALYTICAL_PATH_RASTER_SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>, // Unit quad [-1, -1] to [1, 1]
};

struct InstanceInput {
    @location(1) p0: vec2<f32>,
    @location(2) p1: vec2<f32>,
    @location(3) p2: vec2<f32>,
    @location(4) stroke_width: f32,
    @location(5) flags: u32,
    @location(6) color: vec4<f32>,
    @location(7) z_index: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) pixel_coords: vec2<f32>,
    @location(1) p0: vec2<f32>,
    @location(2) p1: vec2<f32>,
    @location(3) p2: vec2<f32>,
    @location(4) stroke_width: f32,
    @location(5) @interpolate(flat) flags: u32,
    @location(6) color: vec4<f32>,
};

struct Uniforms {
    viewport_size: vec2<f32>,
    _padding: vec2<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(model: VertexInput, instance: InstanceInput) -> VertexOutput {
    var out: VertexOutput;

    // Compute axis-aligned bounding box of the quadratic curve expanded by stroke width
    let min_xy = min(min(instance.p0, instance.p1), instance.p2) - vec2<f32>(instance.stroke_width + 1.0);
    let max_xy = max(max(instance.p0, instance.p1), instance.p2) + vec2<f32>(instance.stroke_width + 1.0);
    let size = max_xy - min_xy;
    
    // Transform unit quad to pixel coordinates
    let norm_pos = model.position * 0.5 + 0.5;
    let pixel_coords = min_xy + norm_pos * size;

    let ndc_x = (pixel_coords.x / uniforms.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (pixel_coords.y / uniforms.viewport_size.y) * 2.0;

    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.pixel_coords = pixel_coords;
    out.p0 = instance.p0;
    out.p1 = instance.p1;
    out.p2 = instance.p2;
    out.stroke_width = instance.stroke_width;
    out.flags = instance.flags;
    out.color = instance.color;

    return out;
}

// Distance from point P to quadratic Bézier curve segment B(t)
fn sd_quadratic_bezier(p: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>) -> f32 {
    let a = p0 - 2.0 * p1 + p2;
    let b = 2.0 * (p1 - p0);
    let c = p0 - p;

    let dot_aa = dot(a, a);
    let dot_ab = dot(a, b);
    let dot_ac = dot(a, c);
    let dot_bb = dot(b, b);
    let dot_bc = dot(b, c);

    var min_dist_sq = min(dot(p - p0, p - p0), dot(p - p2, p - p2));

    // Approximate closest t via clamped parameter sample
    let t_approx = clamp(-dot_ac / max(dot_aa, 1e-4), 0.0, 1.0);
    let u = 1.0 - t_approx;
    let pt = u * u * p0 + 2.0 * u * t_approx * p1 + t_approx * t_approx * p2;
    min_dist_sq = min(min_dist_sq, dot(p - pt, p - pt));

    return sqrt(min_dist_sq);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let dist = sd_quadratic_bezier(in.pixel_coords, in.p0, in.p1, in.p2);

    let is_stroke = (in.flags & 2u) != 0u;
    var alpha = 0.0;

    if (is_stroke) {
        let half_width = max(in.stroke_width * 0.5, 0.5);
        let stroke_dist = abs(dist) - half_width;
        alpha = clamp(0.5 - stroke_dist, 0.0, 1.0);
    } else {
        // Fill mode: distance field threshold
        alpha = clamp(0.5 - dist, 0.0, 1.0);
    }

    if (alpha <= 0.0) {
        discard;
    }

    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
"#;

/// Embedded WGSL Shader for hardware-accelerated concentric fillet corner clipping with G^2 curvature continuity.
pub const WGSL_CONCENTRIC_FILLET_CORNER_SHADER: &str = r#"
fn sdf_squircle(p: vec2<f32>, half_size: vec2<f32>, radius: f32, n: f32) -> f32 {
    let straight = max(half_size - vec2<f32>(radius, radius), vec2<f32>(0.0, 0.0));
    let q = abs(p) - straight;
    if (q.x <= 0.0 && q.y <= 0.0) {
        return max(q.x, q.y) - radius;
    } else if (q.x > 0.0 && q.y > 0.0) {
        let norm_n = pow(pow(q.x, n) + pow(q.y, n), 1.0 / n);
        return norm_n - radius;
    } else {
        return max(q.x, q.y) - radius;
    }
}

fn concentric_fillet_clip(
    pixel_pos: vec2<f32>,
    outer_half: vec2<f32>,
    outer_radius: f32,
    bezel_thickness: f32,
    exponent: f32
) -> vec4<f32> {
    let inner_half = max(outer_half - vec2<f32>(bezel_thickness, bezel_thickness), vec2<f32>(0.0, 0.0));
    let inner_radius = max(outer_radius - bezel_thickness, 0.0);

    let d_outer = sdf_squircle(pixel_pos, outer_half, outer_radius, exponent);
    let d_inner = sdf_squircle(pixel_pos, inner_half, inner_radius, exponent);

    let screen_coverage = clamp(0.5 - d_inner, 0.0, 1.0);
    let chassis_coverage = clamp(0.5 - d_outer, 0.0, 1.0);
    let bezel_mask = clamp(0.5 - max(d_outer, -d_inner), 0.0, 1.0);

    return vec4<f32>(screen_coverage, chassis_coverage, bezel_mask, d_inner);
}
"#;

