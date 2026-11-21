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

    // Compute axis-aligned bounding box of the quadratic curve or triangle expanded cleanly
    let is_triangle = (instance.flags & 1024u) != 0u;
    let expand = select(instance.stroke_width * 1.5 + 2.0, 2.0, is_triangle);
    let min_xy = min(min(instance.p0, instance.p1), instance.p2) - vec2<f32>(expand);
    let max_xy = max(max(instance.p0, instance.p1), instance.p2) + vec2<f32>(expand);
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
    let dot_aa = dot(a, a);
    if (dot_aa <= 1e-4) {
        let v = p2 - p0;
        let vv = dot(v, v);
        let t_lin = select(0.0, clamp(dot(p - p0, v) / max(vv, 1e-6), 0.0, 1.0), vv > 1e-6);
        let proj = p0 + t_lin * v;
        return length(p - proj);
    }

    var best_t = 0.0;
    var min_dist_sq = dot(p - p0, p - p0);
    for (var s = 1; s <= 4; s = s + 1) {
        let ts = f32(s) * 0.25;
        let us = 1.0 - ts;
        let ps = us * us * p0 + 2.0 * us * ts * p1 + ts * ts * p2;
        let d_sq = dot(p - ps, p - ps);
        if (d_sq < min_dist_sq) {
            min_dist_sq = d_sq;
            best_t = ts;
        }
    }

    var t = best_t;
    for (var iter = 0; iter < 3; iter = iter + 1) {
        let u = 1.0 - t;
        let pt = u * u * p0 + 2.0 * u * t * p1 + t * t * p2;
        let d1 = 2.0 * u * (p1 - p0) + 2.0 * t * (p2 - p1);
        let diff = pt - p;
        let f = dot(diff, d1);
        let df = dot(d1, d1) + 2.0 * dot(diff, a);
        if (abs(df) > 1e-4) {
            t = clamp(t - f / df, 0.0, 1.0);
        }
        let u2 = 1.0 - t;
        let pt2 = u2 * u2 * p0 + 2.0 * u2 * t * p1 + t * t * p2;
        min_dist_sq = min(min_dist_sq, dot(p - pt2, p - pt2));
    }

    return sqrt(min_dist_sq);
}

// Signed distance from point P to 2D triangle (p0, p1, p2) (< 0 inside, > 0 outside)
fn sd_triangle(p: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>) -> f32 {
    let e0 = p1 - p0;
    let e1 = p2 - p1;
    let e2 = p0 - p2;
    let v0 = p - p0;
    let v1 = p - p1;
    let v2 = p - p2;
    let pq0 = v0 - e0 * clamp(dot(v0, e0) / max(dot(e0, e0), 1e-6), 0.0, 1.0);
    let pq1 = v1 - e1 * clamp(dot(v1, e1) / max(dot(e1, e1), 1e-6), 0.0, 1.0);
    let pq2 = v2 - e2 * clamp(dot(v2, e2) / max(dot(e2, e2), 1e-6), 0.0, 1.0);
    let s = sign(e0.x * e2.y - e0.y * e2.x);
    let d = min(
        min(
            vec2<f32>(dot(pq0, pq0), s * (v0.x * e0.y - v0.y * e0.x)),
            vec2<f32>(dot(pq1, pq1), s * (v1.x * e1.y - v1.y * e1.x))
        ),
        vec2<f32>(dot(pq2, pq2), s * (v2.x * e2.y - v2.y * e2.x))
    );
    if (d.y > 0.0) {
        return -sqrt(d.x);
    }
    return sqrt(d.x);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if ((in.flags & 1024u) != 0u) {
        let tri_dist = sd_triangle(in.pixel_coords, in.p0, in.p1, in.p2);
        let alpha = clamp(0.5 - tri_dist, 0.0, 1.0);
        if (alpha <= 0.0) {
            discard;
        }
        return vec4<f32>(in.color.rgb, in.color.a * alpha);
    }

    let dist = sd_quadratic_bezier(in.pixel_coords, in.p0, in.p1, in.p2);

    let is_stroke = (in.flags & 2u) != 0u;
    var alpha = 0.0;

    if (is_stroke) {
        let half_width = max(in.stroke_width * 0.5, 0.5);
        var stroke_dist = dist - half_width;

        let span = in.p2 - in.p0;
        let span_sq = dot(span, span);
        let d10 = in.p1 - in.p0;
        let d21 = in.p2 - in.p1;
        let has_dir = span_sq > 1e-6 || dot(d10, d10) > 1e-6 || dot(d21, d21) > 1e-6;
        if (has_dir) {
            let cap_round = (in.flags & 16u) != 0u;
            let cap_square = (in.flags & 32u) != 0u;
            let start_open = (in.flags & 256u) != 0u;
            let end_open = (in.flags & 512u) != 0u;

            let raw_t0 = select(select(d21, span, span_sq > 1e-6), d10, dot(d10, d10) > 1e-6);
            let t0 = normalize(raw_t0);
            let raw_t1 = select(select(d10, span, span_sq > 1e-6), d21, dot(d21, d21) > 1e-6);
            let t1 = normalize(raw_t1);

            let dp0 = in.pixel_coords - in.p0;
            let s0 = dot(dp0, t0);
            let start_round = start_open && cap_round;
            if (!start_round) {
                let n0 = vec2<f32>(-t0.y, t0.x);
                let u0 = abs(dot(dp0, n0));
                let ext0 = select(0.0, half_width, start_open && cap_square);
                let end_plane_0 = -s0 - ext0;
                if (s0 < 0.0) {
                    let q0 = vec2<f32>(end_plane_0, u0 - half_width);
                    let box0 = length(max(q0, vec2<f32>(0.0, 0.0))) + min(max(q0.x, q0.y), 0.0);
                    stroke_dist = max(stroke_dist, box0);
                } else {
                    stroke_dist = max(stroke_dist, end_plane_0);
                }
            }

            let dp2 = in.pixel_coords - in.p2;
            let s1 = dot(dp2, t1);
            let end_round = end_open && cap_round;
            if (!end_round) {
                let n1 = vec2<f32>(-t1.y, t1.x);
                let u1 = abs(dot(dp2, n1));
                let ext1 = select(0.0, half_width, end_open && cap_square);
                let end_plane_1 = s1 - ext1;
                if (s1 > 0.0) {
                    let q1 = vec2<f32>(end_plane_1, u1 - half_width);
                    let box1 = length(max(q1, vec2<f32>(0.0, 0.0))) + min(max(q1.x, q1.y), 0.0);
                    stroke_dist = max(stroke_dist, box1);
                } else {
                    stroke_dist = max(stroke_dist, end_plane_1);
                }
            }
        }

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
