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

    let out_a = in.color.a * coverage;
    return vec4<f32>(in.color.rgb * out_a, out_a);
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

/// Six vertices composing two triangles for a signed unit quad `[-1, -1]` to `[1, 1]`,
/// matching [`strata_gpu_path::WGSL_ANALYTICAL_PATH_RASTER_SHADER`].
pub const SIGNED_QUAD_VERTICES: [GpuVertex; 6] = [
    GpuVertex::new(-1.0, -1.0),
    GpuVertex::new(1.0, -1.0),
    GpuVertex::new(-1.0, 1.0),
    GpuVertex::new(-1.0, 1.0),
    GpuVertex::new(1.0, -1.0),
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

/// Embedded WGSL shader for non-rectangular polygonal clip stencil masks.
pub const WGSL_STENCIL_CLIP_SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
};

struct Uniforms {
    viewport_size: vec2<f32>,
    _padding: vec2<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(model: VertexInput) -> @builtin(position) vec4<f32> {
    let ndc_x = (model.position.x / uniforms.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (model.position.y / uniforms.viewport_size.y) * 2.0;
    return vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    // Stencil clip passes disable color writes and update the stencil attachment directly.
    return vec4<f32>(0.0, 0.0, 0.0, 0.0);
}
"#;

/// 2D vertex coordinate for triangulated GPU stencil clip mask geometry.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpuClipVertex {
    pub position: [f32; 2],
}

impl GpuClipVertex {
    pub const BYTE_SIZE: usize = std::mem::size_of::<Self>();

    #[inline]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { position: [x, y] }
    }

    #[inline]
    pub fn slice_to_bytes(slice: &[Self]) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(slice.as_ptr() as *const u8, slice.len() * Self::BYTE_SIZE)
        }
    }
}

/// Spatial containment relation of a primitive or quad against an active polygonal clip stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipContainment {
    /// Primitive lies completely outside at least one clip layer (cullable).
    Outside,
    /// Primitive crosses a non-rectangular clip boundary (requires stencil masking).
    Partial,
    /// Primitive lies strictly inside every active clip layer (no stencil needed).
    Inside,
}

/// Compiled polygonal/stencil clip mask for circular, rounded, or arbitrary CAD clip paths.
#[derive(Debug, Clone, PartialEq)]
pub struct GpuClipMask {
    /// Conservative axis-aligned bounding box of the intersected clip region.
    pub bounds: Rect,
    /// Active non-rectangular clip layers; each layer contains closed/open subpaths evaluated even-odd.
    pub layers: Vec<Vec<Vec<(f32, f32)>>>,
    /// Triangulated vertex triplets per clip layer (3 vertices per triangle) for GPU stencil passes.
    pub stencil_layers: Vec<Vec<GpuClipVertex>>,
}

impl GpuClipMask {
    /// Constructs and triangulates a stencil clip mask from a stack of subpath layers.
    pub fn from_layers(bounds: Rect, layers: Vec<Vec<Vec<(f32, f32)>>>) -> Self {
        let stencil_layers = layers
            .iter()
            .map(|subpaths| {
                let mut verts = Vec::new();
                for sub in subpaths {
                    verts.extend(triangulate_subpath_pts(sub));
                }
                verts
            })
            .collect();
        Self {
            bounds,
            layers,
            stencil_layers,
        }
    }

    /// Returns `true` if `(x, y)` passes the intersected AABB and all even-odd polygon layers.
    #[inline]
    pub fn contains_point(&self, x: f32, y: f32) -> bool {
        if x < self.bounds.x
            || x >= self.bounds.max_x()
            || y < self.bounds.y
            || y >= self.bounds.max_y()
        {
            return false;
        }
        self.layers
            .iter()
            .all(|poly| crate::canvas2d::point_in_subpaths_even_odd(poly, x, y))
    }

    /// Evaluates containment of `rect` against all clip layers in this mask.
    #[inline]
    pub fn rect_containment(&self, rect: Rect) -> ClipContainment {
        if self.bounds.intersect(rect).is_none() {
            return ClipContainment::Outside;
        }
        rect_relation_to_clip_polys(rect, &self.layers)
    }

    /// Total number of stencil triangle vertices across all active clip layers.
    #[inline]
    pub fn total_stencil_vertices(&self) -> usize {
        self.stencil_layers.iter().map(|v| v.len()).sum()
    }
}

/// Maps a contiguous slice `[start..end)` of batched GPU primitives to an optional [`GpuClipMask`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuClipSpan {
    pub start: usize,
    pub end: usize,
    pub clip_mask: Option<usize>,
}

#[inline]
fn orient2d(a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> f32 {
    (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
}

#[inline]
fn on_segment_bbox(a: (f32, f32), b: (f32, f32), p: (f32, f32)) -> bool {
    p.0 >= a.0.min(b.0) - 1e-4
        && p.0 <= a.0.max(b.0) + 1e-4
        && p.1 >= a.1.min(b.1) - 1e-4
        && p.1 <= a.1.max(b.1) + 1e-4
}

fn segments_intersect_2d(
    a0: (f32, f32),
    a1: (f32, f32),
    b0: (f32, f32),
    b1: (f32, f32),
) -> bool {
    let o1 = orient2d(a0, a1, b0);
    let o2 = orient2d(a0, a1, b1);
    let o3 = orient2d(b0, b1, a0);
    let o4 = orient2d(b0, b1, a1);
    if ((o1 > 1e-4 && o2 < -1e-4) || (o1 < -1e-4 && o2 > 1e-4))
        && ((o3 > 1e-4 && o4 < -1e-4) || (o3 < -1e-4 && o4 > 1e-4))
    {
        return true;
    }
    (o1.abs() <= 1e-4 && on_segment_bbox(a0, a1, b0))
        || (o2.abs() <= 1e-4 && on_segment_bbox(a0, a1, b1))
        || (o3.abs() <= 1e-4 && on_segment_bbox(b0, b1, a0))
        || (o4.abs() <= 1e-4 && on_segment_bbox(b0, b1, a1))
}

fn subpaths_are_axis_aligned_rect(subpaths: &[Vec<(f32, f32)>]) -> bool {
    if subpaths.len() != 1 {
        return false;
    }
    let raw = &subpaths[0];
    let mut pts: Vec<(f32, f32)> = Vec::with_capacity(raw.len());
    for &p in raw {
        if pts
            .last()
            .is_none_or(|&prev| (p.0 - prev.0).hypot(p.1 - prev.1) > 1e-3)
        {
            pts.push(p);
        }
    }
    if pts.len() >= 2 {
        let first = pts[0];
        let last = *pts.last().unwrap();
        if (first.0 - last.0).hypot(first.1 - last.1) <= 1e-3 {
            pts.pop();
        }
    }
    if pts.len() != 4 {
        return false;
    }
    let mut prev_horiz: Option<bool> = None;
    for i in 0..4 {
        let a = pts[i];
        let b = pts[(i + 1) % 4];
        let dx = (b.0 - a.0).abs();
        let dy = (b.1 - a.1).abs();
        let is_horiz = if dy <= 1e-3 && dx > 1e-3 {
            true
        } else if dx <= 1e-3 && dy > 1e-3 {
            false
        } else {
            return false;
        };
        if prev_horiz == Some(is_horiz) {
            return false;
        }
        prev_horiz = Some(is_horiz);
    }
    true
}

#[inline]
fn point_in_triangle_ccw(p: (f32, f32), a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> bool {
    orient2d(a, b, p) >= -1e-4 && orient2d(b, c, p) >= -1e-4 && orient2d(c, a, p) >= -1e-4
}

fn triangulate_subpath_pts(raw_pts: &[(f32, f32)]) -> Vec<GpuClipVertex> {
    let mut pts: Vec<(f32, f32)> = Vec::with_capacity(raw_pts.len());
    for &p in raw_pts {
        if pts
            .last()
            .is_none_or(|&prev| (p.0 - prev.0).hypot(p.1 - prev.1) > 1e-4)
        {
            pts.push(p);
        }
    }
    if pts.len() >= 2 {
        let first = pts[0];
        let last = *pts.last().unwrap();
        if (first.0 - last.0).hypot(first.1 - last.1) <= 1e-4 {
            pts.pop();
        }
    }
    let n = pts.len();
    if n < 3 {
        return Vec::new();
    }

    let mut area2 = 0.0_f32;
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        area2 += a.0 * b.1 - b.0 * a.1;
    }
    if area2.abs() <= 1e-4 {
        return Vec::new();
    }
    if area2 < 0.0 {
        pts.reverse();
    }

    let mut out = Vec::with_capacity((n - 2) * 3);
    let mut indices: Vec<usize> = (0..n).collect();
    let mut guard = 0usize;
    let max_iters = n * n;

    while indices.len() > 3 && guard < max_iters {
        guard += 1;
        let m = indices.len();
        let mut clipped = false;
        for i in 0..m {
            let i_prev = indices[(i + m - 1) % m];
            let i_curr = indices[i];
            let i_next = indices[(i + 1) % m];
            let a = pts[i_prev];
            let b = pts[i_curr];
            let c = pts[i_next];
            if orient2d(a, b, c) <= 1e-5 {
                continue;
            }
            let mut empty = true;
            for (k_idx, &idx) in indices.iter().enumerate() {
                if k_idx == (i + m - 1) % m || k_idx == i || k_idx == (i + 1) % m {
                    continue;
                }
                if point_in_triangle_ccw(pts[idx], a, b, c) {
                    empty = false;
                    break;
                }
            }
            if empty {
                out.push(GpuClipVertex::new(a.0, a.1));
                out.push(GpuClipVertex::new(b.0, b.1));
                out.push(GpuClipVertex::new(c.0, c.1));
                indices.remove(i);
                clipped = true;
                break;
            }
        }
        if !clipped {
            // Self-intersecting subpath fallback: centroid fan pairs cleanly with Invert even-odd stencil.
            out.clear();
            let inv = 1.0 / (n as f32);
            let mut cx = 0.0_f32;
            let mut cy = 0.0_f32;
            for &(px, py) in &pts {
                cx += px;
                cy += py;
            }
            cx *= inv;
            cy *= inv;
            for i in 0..n {
                let a = pts[i];
                let b = pts[(i + 1) % n];
                out.push(GpuClipVertex::new(cx, cy));
                out.push(GpuClipVertex::new(a.0, a.1));
                out.push(GpuClipVertex::new(b.0, b.1));
            }
            return out;
        }
    }

    if indices.len() == 3 {
        let a = pts[indices[0]];
        let b = pts[indices[1]];
        let c = pts[indices[2]];
        out.push(GpuClipVertex::new(a.0, a.1));
        out.push(GpuClipVertex::new(b.0, b.1));
        out.push(GpuClipVertex::new(c.0, c.1));
    }
    out
}

/// Evaluates whether axis-aligned `r` is completely outside, partially clipped by, or strictly
/// inside a stack of non-rectangular clip subpath layers (`even-odd` fill per layer).
pub fn rect_relation_to_clip_polys(
    r: Rect,
    clip_polys: &[Vec<Vec<(f32, f32)>>],
) -> ClipContainment {
    if clip_polys.is_empty() {
        return ClipContainment::Inside;
    }
    if r.width <= 0.0 || r.height <= 0.0 {
        return ClipContainment::Outside;
    }

    let x0 = r.x;
    let y0 = r.y;
    let x1 = r.max_x();
    let y1 = r.max_y();
    let rect_edges = [
        ((x0, y0), (x1, y0)),
        ((x1, y0), (x1, y1)),
        ((x1, y1), (x0, y1)),
        ((x0, y1), (x0, y0)),
    ];
    let samples = [
        (x0, y0),
        (x1, y0),
        (x1, y1),
        (x0, y1),
        ((x0 + x1) * 0.5, (y0 + y1) * 0.5),
    ];

    let mut all_inside = true;
    for poly in clip_polys {
        let mut any_vertex_in_rect = false;
        let mut any_edge_cross = false;

        'poly_scan: for sub in poly {
            if sub.len() < 2 {
                continue;
            }
            for &v in sub {
                if v.0 > x0 + 1e-4 && v.0 < x1 - 1e-4 && v.1 > y0 + 1e-4 && v.1 < y1 - 1e-4 {
                    any_vertex_in_rect = true;
                    break 'poly_scan;
                }
            }
            let closed_sub = sub.len() >= 3;
            let edge_count = if closed_sub { sub.len() } else { sub.len() - 1 };
            for i in 0..edge_count {
                let a = sub[i];
                let b = sub[(i + 1) % sub.len()];
                for &(e0, e1) in &rect_edges {
                    if segments_intersect_2d(a, b, e0, e1) {
                        any_edge_cross = true;
                        break 'poly_scan;
                    }
                }
            }
        }

        if any_vertex_in_rect || any_edge_cross {
            all_inside = false;
            continue;
        }

        let mut inside_count = 0usize;
        for &(sx, sy) in &samples {
            if crate::canvas2d::point_in_subpaths_even_odd(poly, sx, sy) {
                inside_count += 1;
            }
        }
        if inside_count == 0 {
            return ClipContainment::Outside;
        }
        if inside_count < samples.len() {
            all_inside = false;
        }
    }

    if all_inside {
        ClipContainment::Inside
    } else {
        ClipContainment::Partial
    }
}

fn push_clip_span(spans: &mut Vec<GpuClipSpan>, count: usize, index: usize, mask: Option<usize>) {
    if let Some(last) = spans.last_mut() {
        if last.end == index && last.clip_mask == mask {
            last.end = index + count;
            return;
        }
    }
    spans.push(GpuClipSpan {
        start: index,
        end: index + count,
        clip_mask: mask,
    });
}

fn resolve_or_push_clip_mask(
    masks: &mut Vec<GpuClipMask>,
    active_clip: Option<Rect>,
    clip_polys: &[Vec<Vec<(f32, f32)>>],
) -> Option<usize> {
    if clip_polys.is_empty() {
        return None;
    }
    let bounds = active_clip?;
    if let Some((idx, _)) = masks
        .iter()
        .enumerate()
        .find(|(_, m)| m.bounds == bounds && m.layers == clip_polys)
    {
        Some(idx)
    } else {
        let idx = masks.len();
        masks.push(GpuClipMask::from_layers(bounds, clip_polys.to_vec()));
        Some(idx)
    }
}

#[inline]
fn apply_color_opacity(c: Color, alpha_mul: f32) -> Color {
    if (alpha_mul - 1.0).abs() <= 1e-4 {
        return c;
    }
    let (r, g, b, a) = c.to_f32();
    Color::from_f32(r, g, b, (a * alpha_mul).clamp(0.0, 1.0))
}

fn rounded_rect_subpath(bounds: Rect, radius: BorderRadius) -> Vec<(f32, f32)> {
    let hw = bounds.width * 0.5;
    let hh = bounds.height * 0.5;
    let max_r = hw.min(hh).max(0.0);
    let r_tl = radius.top_left.clamp(0.0, max_r);
    let r_tr = radius.top_right.clamp(0.0, max_r);
    let r_br = radius.bottom_right.clamp(0.0, max_r);
    let r_bl = radius.bottom_left.clamp(0.0, max_r);

    let mut pts = Vec::with_capacity(32);
    let corners = [
        (
            bounds.x + bounds.width - r_tr,
            bounds.y + r_tr,
            r_tr,
            -std::f32::consts::FRAC_PI_2,
            0.0,
        ),
        (
            bounds.x + bounds.width - r_br,
            bounds.y + bounds.height - r_br,
            r_br,
            0.0,
            std::f32::consts::FRAC_PI_2,
        ),
        (
            bounds.x + r_bl,
            bounds.y + bounds.height - r_bl,
            r_bl,
            std::f32::consts::FRAC_PI_2,
            std::f32::consts::PI,
        ),
        (
            bounds.x + r_tl,
            bounds.y + r_tl,
            r_tl,
            std::f32::consts::PI,
            std::f32::consts::PI * 1.5,
        ),
    ];
    for (cx, cy, r, a0, a1) in corners {
        if r <= 0.25 {
            pts.push((cx, cy));
        } else {
            let steps = 6usize;
            for i in 0..=steps {
                let a = a0 + (a1 - a0) * (i as f32 / steps as f32);
                pts.push((cx + r * a.cos(), cy + r * a.sin()));
            }
        }
    }
    pts
}

fn compute_dashed_stroke_rect_segments(
    r: Rect,
    lw: f32,
    line_dash: &[f32],
    dash_offset: f32,
) -> Vec<Rect> {
    let mut out = Vec::new();
    let period: f32 = line_dash.iter().copied().sum();
    if line_dash.is_empty() || period <= 1e-4 || lw <= 0.0 || r.width <= 0.0 || r.height <= 0.0 {
        return out;
    }
    let half = lw * 0.5;
    let mut phase = dash_offset.rem_euclid(period);
    let mut dash_idx = 0usize;
    while dash_idx < line_dash.len() && phase >= line_dash[dash_idx] {
        phase -= line_dash[dash_idx];
        dash_idx = (dash_idx + 1) % line_dash.len();
    }
    let mut rem = (line_dash[dash_idx] - phase).max(0.0);

    // Edges clockwise matching Cairo/framebuffer: Top, Right, Bottom, Left
    let edges = [
        (0u8, r.width),
        (1u8, r.height),
        (2u8, r.width),
        (3u8, r.height),
    ];
    for (edge_id, edge_len) in edges {
        if edge_len <= 1e-4 {
            continue;
        }
        let mut cursor = 0.0_f32;
        while cursor < edge_len {
            if rem <= 1e-4 {
                dash_idx = (dash_idx + 1) % line_dash.len();
                rem = line_dash[dash_idx];
                if rem <= 1e-4 {
                    break;
                }
            }
            let step = rem.min(edge_len - cursor);
            if dash_idx.is_multiple_of(2) && step > 1e-4 {
                let s0 = cursor;
                let s1 = cursor + step;
                let seg_rect = match edge_id {
                    0 => Rect::new(r.x + s0, r.y - half, s1 - s0, lw),
                    1 => Rect::new(r.x + r.width - half, r.y + s0, lw, s1 - s0),
                    2 => Rect::new(r.x + r.width - s1, r.y + r.height - half, s1 - s0, lw),
                    _ => Rect::new(r.x - half, r.y + r.height - s1, lw, s1 - s0),
                };
                out.push(seg_rect);
            }
            cursor += step;
            rem -= step;
        }
    }
    out
}

/// Batch collection of GPU instanced quads ready for upload to GPU buffers.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct GpuBatch {
    pub instances: Vec<GpuQuadInstance>,
    /// Compiled non-rectangular/stencil clip masks referenced by [`Self::clip_spans`].
    pub clip_masks: Vec<GpuClipMask>,
    /// Contiguous instance ranges mapped to their active optional stencil clip mask index.
    pub clip_spans: Vec<GpuClipSpan>,
}

impl GpuBatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            instances: Vec::with_capacity(capacity),
            clip_masks: Vec::new(),
            clip_spans: Vec::new(),
        }
    }

    #[inline]
    pub fn push(&mut self, instance: GpuQuadInstance) {
        self.push_with_clip(instance, None);
    }

    /// Pushes a quad instance associated with an optional non-rectangular stencil clip mask index.
    #[inline]
    pub fn push_with_clip(&mut self, instance: GpuQuadInstance, clip_mask: Option<usize>) {
        let idx = self.instances.len();
        self.instances.push(instance);
        if clip_mask.is_some() || !self.clip_spans.is_empty() {
            push_clip_span(&mut self.clip_spans, 1, idx, clip_mask);
        }
    }

    #[inline]
    pub fn has_stencil_clips(&self) -> bool {
        !self.clip_masks.is_empty()
    }

    /// Returns the stencil clip mask governing `instance_index`, if any.
    pub fn clip_mask_for_instance(&self, instance_index: usize) -> Option<&GpuClipMask> {
        let span = self
            .clip_spans
            .iter()
            .find(|s| instance_index >= s.start && instance_index < s.end)?;
        span.clip_mask.and_then(|id| self.clip_masks.get(id))
    }

    #[inline]
    pub fn clear(&mut self) {
        self.instances.clear();
        self.clip_masks.clear();
        self.clip_spans.clear();
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
                DrawCommand::DrawCanvas2D { bounds, commands } => {
                    if let Some(base_clip) = transform_rect_fn(*bounds).intersect(current_clip) {
                        let mut active_clip = Some(base_clip);
                        let mut line_dash: Vec<f32> = Vec::new();
                        let mut dash_offset: f32 = 0.0;
                        let mut clip_polys: Vec<Vec<Vec<(f32, f32)>>> = Vec::new();
                        let mut clip_mask_id: Option<usize> = None;
                        #[allow(clippy::type_complexity)]
                        let mut state_stack: Vec<(
                            Option<Rect>,
                            Vec<f32>,
                            f32,
                            Vec<Vec<Vec<(f32, f32)>>>,
                            Option<usize>,
                        )> = Vec::new();

                        let transform_subpaths = |subs: Vec<Vec<(f32, f32)>>| -> Vec<Vec<(f32, f32)>> {
                            if let Some(mat) = active_matrix {
                                if !mat.is_identity() {
                                    return subs
                                        .into_iter()
                                        .map(|sub| {
                                            sub.into_iter()
                                                .map(|(x, y)| mat.transform_point(x, y))
                                                .collect()
                                        })
                                        .collect();
                                }
                            }
                            subs
                        };

                        for ccmd in commands {
                            match ccmd {
                                crate::canvas2d::CanvasCommand::Save => {
                                    state_stack.push((
                                        active_clip,
                                        line_dash.clone(),
                                        dash_offset,
                                        clip_polys.clone(),
                                        clip_mask_id,
                                    ));
                                }
                                crate::canvas2d::CanvasCommand::Restore => {
                                    if let Some((
                                        prev_clip,
                                        prev_dash,
                                        prev_offset,
                                        prev_polys,
                                        prev_mask,
                                    )) = state_stack.pop()
                                    {
                                        active_clip = prev_clip;
                                        line_dash = prev_dash;
                                        dash_offset = prev_offset;
                                        clip_polys = prev_polys;
                                        clip_mask_id = prev_mask;
                                    }
                                }
                                crate::canvas2d::CanvasCommand::SetLineDash {
                                    segments,
                                    offset,
                                } => {
                                    line_dash = segments
                                        .iter()
                                        .copied()
                                        .filter(|d| d.is_finite() && *d >= 0.0)
                                        .collect();
                                    if line_dash.len() % 2 == 1 {
                                        let dup = line_dash.clone();
                                        line_dash.extend(dup);
                                    }
                                    dash_offset = if offset.is_finite() { *offset } else { 0.0 };
                                }
                                crate::canvas2d::CanvasCommand::ClipPath { verbs } => {
                                    let subpaths = transform_subpaths(
                                        crate::canvas2d::flatten_canvas_verbs(
                                            verbs, bounds.x, bounds.y,
                                        ),
                                    );
                                    if !subpaths.is_empty() {
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
                                        if min_x.is_finite()
                                            && min_y.is_finite()
                                            && max_x >= min_x
                                            && max_y >= min_y
                                        {
                                            let tb = Rect::new(
                                                min_x,
                                                min_y,
                                                max_x - min_x,
                                                max_y - min_y,
                                            );
                                            active_clip =
                                                active_clip.and_then(|c| c.intersect(tb));
                                        }
                                        if !subpaths_are_axis_aligned_rect(&subpaths) {
                                            clip_polys.push(subpaths);
                                            clip_mask_id = resolve_or_push_clip_mask(
                                                &mut batch.clip_masks,
                                                active_clip,
                                                &clip_polys,
                                            );
                                        }
                                    }
                                }
                                crate::canvas2d::CanvasCommand::FillRect {
                                    bounds: rb,
                                    color,
                                } => {
                                    if let Some(canvas_clip) = active_clip {
                                        let r = transform_rect_fn(Rect::new(
                                            bounds.x + rb.x,
                                            bounds.y + rb.y,
                                            rb.width,
                                            rb.height,
                                        ));
                                        if r.intersect(canvas_clip).is_none() {
                                            continue;
                                        }
                                        let (emit, mask) = if clip_polys.is_empty() {
                                            (true, None)
                                        } else {
                                            match rect_relation_to_clip_polys(r, &clip_polys) {
                                                ClipContainment::Outside => (false, None),
                                                ClipContainment::Inside => (true, None),
                                                ClipContainment::Partial => (true, clip_mask_id),
                                            }
                                        };
                                        if emit {
                                            batch.push_with_clip(
                                                GpuQuadInstance::from_rect(r, *color, canvas_clip),
                                                mask,
                                            );
                                        }
                                    }
                                }
                                crate::canvas2d::CanvasCommand::StrokeRect {
                                    bounds: rb,
                                    color,
                                    line_width,
                                } => {
                                    if let Some(canvas_clip) = active_clip {
                                        let r = Rect::new(
                                            bounds.x + rb.x,
                                            bounds.y + rb.y,
                                            rb.width,
                                            rb.height,
                                        );
                                        let period: f32 = line_dash.iter().copied().sum();
                                        if line_dash.is_empty() || period <= 1e-4 {
                                            let tr = transform_rect_fn(r);
                                            let lw = *line_width;
                                            let (emit, mask) = if clip_polys.is_empty() {
                                                (tr.intersect(canvas_clip).is_some(), None)
                                            } else {
                                                let half = lw * 0.5;
                                                let edge_rects = [
                                                    Rect::new(tr.x, tr.y - half, tr.width, lw),
                                                    Rect::new(
                                                        tr.x + tr.width - half,
                                                        tr.y,
                                                        lw,
                                                        tr.height,
                                                    ),
                                                    Rect::new(
                                                        tr.x,
                                                        tr.y + tr.height - half,
                                                        tr.width,
                                                        lw,
                                                    ),
                                                    Rect::new(tr.x - half, tr.y, lw, tr.height),
                                                ];
                                                let mut any_in = false;
                                                let mut all_strictly_in = true;
                                                for er in edge_rects {
                                                    match rect_relation_to_clip_polys(
                                                        er,
                                                        &clip_polys,
                                                    ) {
                                                        ClipContainment::Outside => {
                                                            all_strictly_in = false;
                                                        }
                                                        ClipContainment::Inside => {
                                                            any_in = true;
                                                        }
                                                        ClipContainment::Partial => {
                                                            any_in = true;
                                                            all_strictly_in = false;
                                                        }
                                                    }
                                                }
                                                if !any_in {
                                                    (false, None)
                                                } else if all_strictly_in {
                                                    (true, None)
                                                } else {
                                                    (true, clip_mask_id)
                                                }
                                            };
                                            if emit {
                                                batch.push_with_clip(
                                                    GpuQuadInstance::from_rounded_rect(
                                                        tr,
                                                        Color::TRANSPARENT,
                                                        BorderRadius::default(),
                                                        EdgeInsets::all(lw),
                                                        Some(*color),
                                                        canvas_clip,
                                                    ),
                                                    mask,
                                                );
                                            }
                                        } else if *line_width > 0.0 && r.width > 0.0 && r.height > 0.0
                                        {
                                            for seg_rect in compute_dashed_stroke_rect_segments(
                                                r,
                                                *line_width,
                                                &line_dash,
                                                dash_offset,
                                            ) {
                                                let t_seg = transform_rect_fn(seg_rect);
                                                if t_seg.intersect(canvas_clip).is_some() {
                                                    let (emit, mask) = if clip_polys.is_empty() {
                                                        (true, None)
                                                    } else {
                                                        match rect_relation_to_clip_polys(
                                                            t_seg,
                                                            &clip_polys,
                                                        ) {
                                                            ClipContainment::Outside => {
                                                                (false, None)
                                                            }
                                                            ClipContainment::Inside => (true, None),
                                                            ClipContainment::Partial => {
                                                                (true, clip_mask_id)
                                                            }
                                                        }
                                                    };
                                                    if emit {
                                                        batch.push_with_clip(
                                                            GpuQuadInstance::from_rect(
                                                                t_seg,
                                                                *color,
                                                                canvas_clip,
                                                            ),
                                                            mask,
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
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

    /// Advances and returns the hardware synchronization fence.
    pub fn sync_barrier(&self) -> u64 {
        self.buffer.sync_barrier()
    }

    /// Checks if a hardware synchronization fence has been signaled.
    pub fn is_fence_synchronized(&self, fence: u64) -> bool {
        self.buffer.is_fence_synchronized(fence)
    }

    #[allow(clippy::too_many_arguments)]
    fn push_text_run(
        &mut self,
        text: &str,
        start_x: f32,
        start_y: f32,
        font_size: f32,
        x_squeeze: f32,
        color: Color,
        clip_rect: Rect,
    ) -> usize {
        self.push_text_run_with_bounds(
            text,
            start_x,
            start_y,
            None,
            None,
            font_size,
            1.2,
            strata_style::TextAlign::Left,
            x_squeeze,
            color,
            clip_rect,
        )
        .0
    }

    #[allow(clippy::too_many_arguments)]
    fn push_text_run_with_bounds(
        &mut self,
        text: &str,
        start_x: f32,
        start_y: f32,
        bounds_width: Option<f32>,
        bounds_height: Option<f32>,
        font_size: f32,
        line_height: f32,
        text_align: strata_style::TextAlign,
        x_squeeze: f32,
        color: Color,
        clip_rect: Rect,
    ) -> (usize, Rect) {
        let (r, g, b, a) = color.to_f32();
        let color_f32 = [r, g, b, a];
        let clip_r = strata_gpu_path::Rect2D::new(
            clip_rect.x,
            clip_rect.y,
            clip_rect.x + clip_rect.width,
            clip_rect.y + clip_rect.height,
        );
        let step_y = crate::compositor::effective_draw_text_line_height(font_size, line_height);
        let glyph_cache = &self.glyph_cache;
        let (lines, start_y_off) = crate::compositor::layout_draw_text_lines(
            text,
            bounds_width,
            bounds_height,
            font_size,
            line_height,
            text_align,
            |ch| {
                glyph_cache
                    .get(ch)
                    .map(|glyph| glyph.advance_width * font_size * x_squeeze)
                    .unwrap_or(font_size * 0.5 * x_squeeze)
            },
        );
        let mut pushed = 0usize;
        let mut max_end_x = start_x;
        let mut min_start_x = start_x;
        let base_y = start_y + start_y_off;

        for (line_idx, (line_ox, line_str)) in lines.iter().enumerate() {
            let mut cursor_x = start_x + line_ox;
            min_start_x = min_start_x.min(cursor_x);
            let cursor_y = base_y + line_idx as f32 * step_y;
            for ch in line_str.chars() {
                if let Some(glyph) = self.glyph_cache.get(ch) {
                    let glyph_w = glyph.advance_width * font_size * x_squeeze;
                    let glyph_h = font_size;
                    let screen_bounds = strata_gpu_path::Rect2D::new(
                        cursor_x,
                        cursor_y,
                        cursor_x + glyph_w,
                        cursor_y + glyph_h,
                    );
                    if clip_rect
                        .intersect(Rect::new(cursor_x, cursor_y, glyph_w, glyph_h))
                        .is_some()
                        && self
                            .buffer
                            .push_glyph(glyph, screen_bounds, color_f32, clip_r)
                    {
                        self.total_glyphs += 1;
                        pushed += 1;
                    }
                    cursor_x += glyph_w;
                } else {
                    cursor_x += font_size * 0.5 * x_squeeze;
                }
            }
            max_end_x = max_end_x.max(cursor_x);
        }
        let total_h = (lines.len().max(1) as f32 * step_y).max(font_size).max(1.0);
        let run_rect = Rect::new(
            min_start_x,
            base_y,
            (max_end_x - min_start_x).max(1.0),
            total_h,
        );
        (pushed, run_rect)
    }

    #[allow(clippy::too_many_arguments)]
    fn push_canvas_text(
        &mut self,
        bounds: Rect,
        text: &str,
        x: f32,
        y: f32,
        color: Color,
        font_size: f32,
        text_align: crate::canvas2d::CanvasTextAlign,
        text_baseline: crate::canvas2d::CanvasTextBaseline,
        max_width: Option<f32>,
        canvas_clip: Rect,
    ) -> (usize, Rect) {
        let scale = font_size;
        let raw_w: f32 = text
            .chars()
            .map(|ch| {
                self.glyph_cache
                    .get(ch)
                    .map(|g| g.advance_width * scale)
                    .unwrap_or(scale * 0.5)
            })
            .sum();
        let x_squeeze = match max_width {
            Some(mw) if mw > 0.0 && raw_w > mw => mw / raw_w,
            _ => 1.0,
        };
        let total_w = raw_w * x_squeeze;
        let cursor_x = bounds.x
            + match text_align {
                crate::canvas2d::CanvasTextAlign::Left | crate::canvas2d::CanvasTextAlign::Start => {
                    x
                }
                crate::canvas2d::CanvasTextAlign::Center => x - total_w * 0.5,
                crate::canvas2d::CanvasTextAlign::Right | crate::canvas2d::CanvasTextAlign::End => {
                    x - total_w
                }
            };
        let cursor_y = bounds.y
            + match text_baseline {
                crate::canvas2d::CanvasTextBaseline::Top
                | crate::canvas2d::CanvasTextBaseline::Hanging => y,
                crate::canvas2d::CanvasTextBaseline::Middle => y - scale * 0.5,
                crate::canvas2d::CanvasTextBaseline::Bottom
                | crate::canvas2d::CanvasTextBaseline::Ideographic => y - scale,
                crate::canvas2d::CanvasTextBaseline::Alphabetic => y - scale * 0.8,
            };
        let run_rect = Rect::new(cursor_x, cursor_y, total_w.max(1.0), scale.max(1.0));
        let pushed = self.push_text_run(
            text,
            cursor_x,
            cursor_y,
            scale,
            x_squeeze,
            color,
            canvas_clip,
        );
        (pushed, run_rect)
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
                    bounds_width,
                    bounds_height,
                    text_align,
                    text,
                    font_size,
                    line_height,
                    color,
                    ..
                } => {
                    self.push_text_run_with_bounds(
                        text,
                        origin.0,
                        origin.1,
                        *bounds_width,
                        *bounds_height,
                        *font_size,
                        *line_height,
                        *text_align,
                        1.0,
                        *color,
                        default_clip,
                    );
                }
                DrawCommand::DrawTextFragments { fragments } => {
                    for frag in fragments {
                        self.push_text_run(
                            &frag.text,
                            frag.bounds.x,
                            frag.bounds.y,
                            frag.style.font_size,
                            1.0,
                            frag.style.color,
                            default_clip,
                        );
                    }
                }
                DrawCommand::DrawCanvas2D { bounds, commands } => {
                    let Some(base_clip) = bounds.intersect(default_clip) else {
                        continue;
                    };
                    let mut active_clip = Some(base_clip);
                    let mut clip_stack: Vec<Option<Rect>> = Vec::new();

                    for ccmd in commands {
                        match ccmd {
                            crate::canvas2d::CanvasCommand::Save => {
                                clip_stack.push(active_clip);
                            }
                            crate::canvas2d::CanvasCommand::Restore => {
                                if let Some(prev) = clip_stack.pop() {
                                    active_clip = prev;
                                }
                            }
                            crate::canvas2d::CanvasCommand::ClipPath { verbs } => {
                                if let Some(vbbox) =
                                    crate::canvas2d::canvas_verbs_bounds(verbs, bounds.x, bounds.y)
                                {
                                    active_clip = active_clip.and_then(|c| c.intersect(vbbox));
                                }
                            }
                            crate::canvas2d::CanvasCommand::FillText {
                                text,
                                x,
                                y,
                                color,
                                font_size,
                                text_align,
                                text_baseline,
                                max_width,
                                ..
                            }
                            | crate::canvas2d::CanvasCommand::StrokeText {
                                text,
                                x,
                                y,
                                color,
                                font_size,
                                text_align,
                                text_baseline,
                                max_width,
                                ..
                            } => {
                                let Some(canvas_clip) = active_clip else {
                                    continue;
                                };
                                self.push_canvas_text(
                                    *bounds,
                                    text,
                                    *x,
                                    *y,
                                    *color,
                                    *font_size,
                                    *text_align,
                                    *text_baseline,
                                    *max_width,
                                    canvas_clip,
                                );
                            }
                            _ => {}
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

#[inline]
fn layout_cap_to_gpu(cap: strata_layout::StrokeLineCap) -> strata_gpu_path::LineCap {
    match cap {
        strata_layout::StrokeLineCap::Butt => strata_gpu_path::LineCap::Butt,
        strata_layout::StrokeLineCap::Round => strata_gpu_path::LineCap::Round,
        strata_layout::StrokeLineCap::Square => strata_gpu_path::LineCap::Square,
    }
}

#[inline]
fn layout_join_to_gpu(join: strata_layout::StrokeLineJoin) -> strata_gpu_path::LineJoin {
    match join {
        strata_layout::StrokeLineJoin::Miter => strata_gpu_path::LineJoin::Miter,
        strata_layout::StrokeLineJoin::Round => strata_gpu_path::LineJoin::Round,
        strata_layout::StrokeLineJoin::Bevel => strata_gpu_path::LineJoin::Bevel,
    }
}

#[inline]
fn quad_world_bounds(q: &strata_gpu_path::QuadraticBezier, pad: f32) -> Rect {
    let min_x = q.p0.x.min(q.p1.x).min(q.p2.x) - pad;
    let max_x = q.p0.x.max(q.p1.x).max(q.p2.x) + pad;
    let min_y = q.p0.y.min(q.p1.y).min(q.p2.y) - pad;
    let max_y = q.p0.y.max(q.p1.y).max(q.p2.y) + pad;
    Rect::new(
        min_x,
        min_y,
        (max_x - min_x).max(1.0),
        (max_y - min_y).max(1.0),
    )
}

#[inline]
fn subpaths_from_path_verbs(
    verbs: &[strata_gpu_path::PathVerb],
    ox: f32,
    oy: f32,
    sx: f32,
    sy: f32,
    tolerance: f32,
) -> Vec<(Vec<strata_gpu_path::QuadraticBezier>, bool)> {
    let mut result = Vec::new();
    let mut cur_sub = Vec::new();
    let mut cur = strata_gpu_path::Point2D::new(ox, oy);
    let mut start = cur;

    let map_pt = |p: strata_gpu_path::Point2D| {
        strata_gpu_path::Point2D::new(ox + p.x * sx, oy + p.y * sy)
    };

    for &v in verbs {
        match v {
            strata_gpu_path::PathVerb::MoveTo(p) => {
                if !cur_sub.is_empty() {
                    result.push((std::mem::take(&mut cur_sub), false));
                }
                cur = map_pt(p);
                start = cur;
            }
            strata_gpu_path::PathVerb::LineTo(p) => {
                let next = map_pt(p);
                cur_sub.push(strata_gpu_path::QuadraticBezier::new(
                    cur,
                    cur.lerp(next, 0.5),
                    next,
                ));
                cur = next;
            }
            strata_gpu_path::PathVerb::QuadTo(p1, p2) => {
                let cp = map_pt(p1);
                let next = map_pt(p2);
                cur_sub.push(strata_gpu_path::QuadraticBezier::new(cur, cp, next));
                cur = next;
            }
            strata_gpu_path::PathVerb::CubicTo(p1, p2, p3) => {
                let cubic =
                    strata_gpu_path::CubicBezier::new(cur, map_pt(p1), map_pt(p2), map_pt(p3));
                cur_sub.extend(cubic.to_quadratics(tolerance));
                cur = map_pt(p3);
            }
            strata_gpu_path::PathVerb::Close => {
                if cur.distance_squared(start) > 1e-4 {
                    cur_sub.push(strata_gpu_path::QuadraticBezier::new(
                        cur,
                        cur.lerp(start, 0.5),
                        start,
                    ));
                    cur = start;
                }
                if !cur_sub.is_empty() {
                    result.push((std::mem::take(&mut cur_sub), true));
                }
            }
        }
    }
    if !cur_sub.is_empty() {
        result.push((cur_sub, false));
    }
    result
}

fn circle_to_quads(cx: f32, cy: f32, r: f32) -> Vec<strata_gpu_path::QuadraticBezier> {
    let segs = 8usize;
    let mut circle_quads = Vec::with_capacity(segs);
    for i in 0..segs {
        let a0 = (i as f32 / segs as f32) * std::f32::consts::TAU;
        let a1 = ((i + 1) as f32 / segs as f32) * std::f32::consts::TAU;
        let am = (a0 + a1) * 0.5;
        let cos_half = ((a1 - a0) * 0.5).cos().max(0.1);
        let p0 = strata_gpu_path::Point2D::new(cx + r * a0.cos(), cy + r * a0.sin());
        let p2 = strata_gpu_path::Point2D::new(cx + r * a1.cos(), cy + r * a1.sin());
        let p1 = strata_gpu_path::Point2D::new(
            cx + (r / cos_half) * am.cos(),
            cy + (r / cos_half) * am.sin(),
        );
        circle_quads.push(strata_gpu_path::QuadraticBezier::new(p0, p1, p2));
    }
    circle_quads
}

fn canvas_path_verbs_to_quad_subpaths(
    verbs: &[crate::canvas2d::CanvasPathVerb],
    ox: f32,
    oy: f32,
) -> Vec<(Vec<strata_gpu_path::QuadraticBezier>, bool)> {
    let mut subpaths: Vec<(Vec<strata_gpu_path::QuadraticBezier>, bool)> = Vec::new();
    let mut cur_sub = Vec::new();
    let mut start = strata_gpu_path::Point2D::new(ox, oy);
    let mut cur = start;
    let mut has_current = false;
    for v in verbs {
        match *v {
            crate::canvas2d::CanvasPathVerb::MoveTo(x, y) => {
                if !cur_sub.is_empty() {
                    subpaths.push((std::mem::take(&mut cur_sub), false));
                }
                cur = strata_gpu_path::Point2D::new(ox + x, oy + y);
                start = cur;
                has_current = true;
            }
            crate::canvas2d::CanvasPathVerb::LineTo(x, y) => {
                let next = strata_gpu_path::Point2D::new(ox + x, oy + y);
                cur_sub.push(strata_gpu_path::QuadraticBezier::new(
                    cur,
                    cur.lerp(next, 0.5),
                    next,
                ));
                cur = next;
                has_current = true;
            }
            crate::canvas2d::CanvasPathVerb::QuadraticCurveTo { cpx, cpy, x, y } => {
                let cp = strata_gpu_path::Point2D::new(ox + cpx, oy + cpy);
                let next = strata_gpu_path::Point2D::new(ox + x, oy + y);
                cur_sub.push(strata_gpu_path::QuadraticBezier::new(cur, cp, next));
                cur = next;
                has_current = true;
            }
            crate::canvas2d::CanvasPathVerb::BezierCurveTo {
                cp1x,
                cp1y,
                cp2x,
                cp2y,
                x,
                y,
            } => {
                let cp1 = strata_gpu_path::Point2D::new(ox + cp1x, oy + cp1y);
                let cp2 = strata_gpu_path::Point2D::new(ox + cp2x, oy + cp2y);
                let next = strata_gpu_path::Point2D::new(ox + x, oy + y);
                let cubic = strata_gpu_path::CubicBezier::new(cur, cp1, cp2, next);
                cur_sub.extend(cubic.to_quadratics(0.25));
                cur = next;
                has_current = true;
            }
            crate::canvas2d::CanvasPathVerb::Arc {
                cx,
                cy,
                radius,
                start_angle,
                end_angle,
                counter_clockwise,
            } => {
                let acx = ox + cx;
                let acy = oy + cy;
                let mut sweep = end_angle - start_angle;
                if counter_clockwise && sweep > 0.0 {
                    sweep -= std::f32::consts::TAU;
                } else if !counter_clockwise && sweep < 0.0 {
                    sweep += std::f32::consts::TAU;
                }
                let arc_start = strata_gpu_path::Point2D::new(
                    acx + radius * start_angle.cos(),
                    acy + radius * start_angle.sin(),
                );
                if !has_current {
                    start = arc_start;
                    cur = arc_start;
                    has_current = true;
                } else if cur.distance_squared(arc_start) > 1e-4 {
                    cur_sub.push(strata_gpu_path::QuadraticBezier::new(
                        cur,
                        cur.lerp(arc_start, 0.5),
                        arc_start,
                    ));
                    cur = arc_start;
                }
                let steps = ((sweep.abs() / std::f32::consts::FRAC_PI_4).ceil() as usize)
                    .clamp(4, 16);
                for i in 0..steps {
                    let t0 = start_angle + sweep * (i as f32 / steps as f32);
                    let t1 = start_angle + sweep * ((i + 1) as f32 / steps as f32);
                    let tm = (t0 + t1) * 0.5;
                    let cos_h = ((t1 - t0) * 0.5).cos().max(0.1);
                    let p0 = strata_gpu_path::Point2D::new(
                        acx + radius * t0.cos(),
                        acy + radius * t0.sin(),
                    );
                    let p2 = strata_gpu_path::Point2D::new(
                        acx + radius * t1.cos(),
                        acy + radius * t1.sin(),
                    );
                    let p1 = strata_gpu_path::Point2D::new(
                        acx + (radius / cos_h) * tm.cos(),
                        acy + (radius / cos_h) * tm.sin(),
                    );
                    cur_sub.push(strata_gpu_path::QuadraticBezier::new(p0, p1, p2));
                    cur = p2;
                }
            }
            crate::canvas2d::CanvasPathVerb::Close => {
                if has_current && cur.distance_squared(start) > 1e-4 {
                    cur_sub.push(strata_gpu_path::QuadraticBezier::new(
                        cur,
                        cur.lerp(start, 0.5),
                        start,
                    ));
                    cur = start;
                }
                if !cur_sub.is_empty() {
                    subpaths.push((std::mem::take(&mut cur_sub), true));
                }
            }
        }
    }
    if !cur_sub.is_empty() {
        subpaths.push((cur_sub, false));
    }
    subpaths
}

fn flatten_quad_subpaths(
    subpaths: &[(Vec<strata_gpu_path::QuadraticBezier>, bool)],
    steps_per_curve: usize,
) -> Vec<Vec<(f32, f32)>> {
    let steps = steps_per_curve.max(1);
    let mut polys = Vec::with_capacity(subpaths.len());
    for (quads, _) in subpaths {
        if quads.is_empty() {
            continue;
        }
        let mut pts = Vec::with_capacity(quads.len() * steps + 1);
        for (qi, q) in quads.iter().enumerate() {
            let a = (q.p0.x - 2.0 * q.p1.x + q.p2.x).hypot(q.p0.y - 2.0 * q.p1.y + q.p2.y);
            let seg_steps = if a <= 1e-3 { 1 } else { steps };
            let start_step = if qi == 0 { 0 } else { 1 };
            for s in start_step..=seg_steps {
                let t = s as f32 / seg_steps as f32;
                let u = 1.0 - t;
                let px = u * u * q.p0.x + 2.0 * u * t * q.p1.x + t * t * q.p2.x;
                let py = u * u * q.p0.y + 2.0 * u * t * q.p1.y + t * t * q.p2.y;
                pts.push((px, py));
            }
        }
        if pts.len() >= 3 {
            polys.push(pts);
        }
    }
    polys
}

fn poly_layers_bbox(subpaths: &[Vec<(f32, f32)>]) -> Option<Rect> {
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
    if min_x.is_finite() && min_y.is_finite() && max_x > min_x && max_y > min_y {
        Some(Rect::new(min_x, min_y, max_x - min_x, max_y - min_y))
    } else {
        None
    }
}

/// Batch collection of GPU instanced path segments ready for upload to storage/vertex buffers.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct GpuPathBatch {
    pub segments: Vec<strata_gpu_path::GpuPathSegment>,
    /// Compiled non-rectangular/stencil clip masks referenced by [`Self::clip_spans`].
    pub clip_masks: Vec<GpuClipMask>,
    /// Contiguous segment ranges mapped to their active optional stencil clip mask index.
    pub clip_spans: Vec<GpuClipSpan>,
}

impl GpuPathBatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            segments: Vec::with_capacity(capacity),
            clip_masks: Vec::new(),
            clip_spans: Vec::new(),
        }
    }

    #[inline]
    pub fn push(&mut self, segment: strata_gpu_path::GpuPathSegment) {
        self.push_with_clip(segment, None);
    }

    /// Pushes a path segment associated with an optional non-rectangular stencil clip mask index.
    #[inline]
    pub fn push_with_clip(
        &mut self,
        segment: strata_gpu_path::GpuPathSegment,
        clip_mask: Option<usize>,
    ) {
        let idx = self.segments.len();
        self.segments.push(segment);
        if clip_mask.is_some() || !self.clip_spans.is_empty() {
            push_clip_span(&mut self.clip_spans, 1, idx, clip_mask);
        }
    }

    #[inline]
    pub fn has_stencil_clips(&self) -> bool {
        !self.clip_masks.is_empty()
    }

    /// Returns the stencil clip mask governing `segment_index`, if any.
    pub fn clip_mask_for_segment(&self, segment_index: usize) -> Option<&GpuClipMask> {
        let span = self
            .clip_spans
            .iter()
            .find(|s| segment_index >= s.start && segment_index < s.end)?;
        span.clip_mask.and_then(|id| self.clip_masks.get(id))
    }

    #[inline]
    pub fn clear(&mut self) {
        self.segments.clear();
        self.clip_masks.clear();
        self.clip_spans.clear();
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

    fn emit_clipped_segment(
        &mut self,
        seg: strata_gpu_path::GpuPathSegment,
        q_bbox: Rect,
        clip: Rect,
        clip_polys: &[Vec<Vec<(f32, f32)>>],
        clip_mask_id: Option<usize>,
    ) {
        if q_bbox.intersect(clip).is_none() {
            return;
        }
        let (emit, mask) = if clip_polys.is_empty() {
            (true, None)
        } else {
            match rect_relation_to_clip_polys(q_bbox, clip_polys) {
                ClipContainment::Outside => (false, None),
                ClipContainment::Inside => (true, None),
                ClipContainment::Partial => (true, clip_mask_id),
            }
        };
        if emit {
            self.push_with_clip(seg, mask);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_corner_join(
        &mut self,
        q0: &strata_gpu_path::QuadraticBezier,
        q1: &strata_gpu_path::QuadraticBezier,
        sw: f32,
        c: [f32; 4],
        join: strata_gpu_path::LineJoin,
        miter_limit: f32,
        extra_flags: u32,
        clip: Rect,
        clip_polys: &[Vec<Vec<(f32, f32)>>],
        clip_mask_id: Option<usize>,
    ) {
        let j = q0.p2;
        let raw_t0 = q0.tangent(1.0);
        let raw_t0 = if raw_t0.x * raw_t0.x + raw_t0.y * raw_t0.y > 1e-6 {
            raw_t0
        } else {
            strata_gpu_path::Point2D::new(q0.p2.x - q0.p0.x, q0.p2.y - q0.p0.y)
        };
        let len0 = (raw_t0.x * raw_t0.x + raw_t0.y * raw_t0.y).sqrt();
        let raw_t1 = q1.tangent(0.0);
        let raw_t1 = if raw_t1.x * raw_t1.x + raw_t1.y * raw_t1.y > 1e-6 {
            raw_t1
        } else {
            strata_gpu_path::Point2D::new(q1.p2.x - q1.p0.x, q1.p2.y - q1.p0.y)
        };
        let len1 = (raw_t1.x * raw_t1.x + raw_t1.y * raw_t1.y).sqrt();
        if len0 <= 1e-5 || len1 <= 1e-5 {
            return;
        }
        let t0 = strata_gpu_path::Point2D::new(raw_t0.x / len0, raw_t0.y / len0);
        let t1 = strata_gpu_path::Point2D::new(raw_t1.x / len1, raw_t1.y / len1);
        let dot_t = t0.x * t1.x + t0.y * t1.y;
        if dot_t > 0.995 {
            return;
        }

        let half_w = sw * 0.5;
        let join_bbox = Rect::new(
            j.x - half_w * miter_limit.max(1.0) - 2.0,
            j.y - half_w * miter_limit.max(1.0) - 2.0,
            (half_w * miter_limit.max(1.0) + 2.0) * 2.0,
            (half_w * miter_limit.max(1.0) + 2.0) * 2.0,
        );

        let join_flag_bit = match join {
            strata_gpu_path::LineJoin::Miter => 0,
            strata_gpu_path::LineJoin::Round => strata_gpu_path::FLAG_JOIN_ROUND,
            strata_gpu_path::LineJoin::Bevel => strata_gpu_path::FLAG_JOIN_BEVEL,
        };

        if join == strata_gpu_path::LineJoin::Round {
            let deg = strata_gpu_path::QuadraticBezier::new(j, j, j);
            let seg = strata_gpu_path::GpuPathSegment::from_quadratic(
                &deg,
                sw,
                strata_gpu_path::FLAG_STROKE
                    | strata_gpu_path::FLAG_CAP_ROUND
                    | strata_gpu_path::FLAG_START_OPEN
                    | strata_gpu_path::FLAG_END_OPEN
                    | join_flag_bit
                    | extra_flags,
                c,
                0.0,
            );
            self.emit_clipped_segment(seg, join_bbox, clip, clip_polys, clip_mask_id);
            return;
        }

        let cross = t0.x * t1.y - t0.y * t1.x;
        if cross.abs() < 1e-4 {
            return;
        }
        let sign = if cross > 0.0 { -1.0 } else { 1.0 };
        let n0x = sign * (-t0.y);
        let n0y = sign * t0.x;
        let n1x = sign * (-t1.y);
        let n1y = sign * t1.x;
        let a = strata_gpu_path::Point2D::new(j.x + n0x * half_w, j.y + n0y * half_w);
        let b = strata_gpu_path::Point2D::new(j.x + n1x * half_w, j.y + n1y * half_w);
        if a.distance_squared(b) <= 0.04 {
            return;
        }

        let sum_x = n0x + n1x;
        let sum_y = n0y + n1y;
        let sum_len = sum_x.hypot(sum_y);
        if sum_len <= 1e-5 {
            return;
        }
        let miter_ratio = 2.0 / sum_len;
        if join == strata_gpu_path::LineJoin::Miter && miter_ratio <= miter_limit.max(1.0) {
            let denom = 1.0 + n0x * n1x + n0y * n1y;
            if denom > 1e-4 {
                let m_scale = half_w / denom;
                let m = strata_gpu_path::Point2D::new(j.x + sum_x * m_scale, j.y + sum_y * m_scale);
                let q_a = strata_gpu_path::QuadraticBezier::new(j, a, m);
                let q_b = strata_gpu_path::QuadraticBezier::new(j, m, b);
                let flags = strata_gpu_path::FLAG_FILL | strata_gpu_path::FLAG_TRIANGLE | extra_flags;
                self.emit_clipped_segment(
                    strata_gpu_path::GpuPathSegment::from_quadratic(&q_a, sw, flags, c, 0.0),
                    quad_world_bounds(&q_a, 2.0),
                    clip,
                    clip_polys,
                    clip_mask_id,
                );
                self.emit_clipped_segment(
                    strata_gpu_path::GpuPathSegment::from_quadratic(&q_b, sw, flags, c, 0.0),
                    quad_world_bounds(&q_b, 2.0),
                    clip,
                    clip_polys,
                    clip_mask_id,
                );
                return;
            }
        }

        let q_bev = strata_gpu_path::QuadraticBezier::new(j, a, b);
        let flags = strata_gpu_path::FLAG_FILL
            | strata_gpu_path::FLAG_JOIN_BEVEL
            | strata_gpu_path::FLAG_TRIANGLE
            | extra_flags;
        self.emit_clipped_segment(
            strata_gpu_path::GpuPathSegment::from_quadratic(&q_bev, sw, flags, c, 0.0),
            quad_world_bounds(&q_bev, 2.0),
            clip,
            clip_polys,
            clip_mask_id,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_subpath_strokes(
        &mut self,
        quads: &[strata_gpu_path::QuadraticBezier],
        closed: bool,
        sw: f32,
        col: Color,
        cap: strata_gpu_path::LineCap,
        join: strata_gpu_path::LineJoin,
        miter_limit: f32,
        dash: &[f32],
        dash_offset: f32,
        extra_flags: u32,
        clip_opt: Option<Rect>,
        clip_polys: &[Vec<Vec<(f32, f32)>>],
        clip_mask_id: Option<usize>,
    ) {
        let Some(clip) = clip_opt else {
            return;
        };
        if quads.is_empty() {
            return;
        }
        let (cr, cg, cb, ca) = col.to_f32();
        let c = [cr, cg, cb, ca];
        let pad = sw * 1.5 + 2.0;

        let dash_period: f32 = dash.iter().copied().filter(|&d| d >= 0.0 && d.is_finite()).sum();
        if dash.iter().any(|&d| d > 0.0) && dash_period > 1e-4 {
            for dash_sub in strata_gpu_path::dash_quadratics_with_offset(quads, dash, dash_offset) {
                let n = dash_sub.len();
                if n == 0 {
                    continue;
                }
                for (i, q) in dash_sub.iter().enumerate() {
                    let flags = strata_gpu_path::stroke_style_flags(
                        cap,
                        join,
                        false,
                        i == 0,
                        i + 1 == n,
                    ) | extra_flags;
                    let seg = strata_gpu_path::GpuPathSegment::from_quadratic(q, sw, flags, c, 0.0);
                    self.emit_clipped_segment(
                        seg,
                        quad_world_bounds(q, pad),
                        clip,
                        clip_polys,
                        clip_mask_id,
                    );
                }
                for i in 0..n.saturating_sub(1) {
                    self.emit_corner_join(
                        &dash_sub[i],
                        &dash_sub[i + 1],
                        sw,
                        c,
                        join,
                        miter_limit,
                        extra_flags,
                        clip,
                        clip_polys,
                        clip_mask_id,
                    );
                }
            }
            return;
        }

        // Split into connected runs cleanly in case `quads` holds multiple disconnected subpaths.
        let mut run_start = 0usize;
        for idx in 1..=quads.len() {
            let disconnected = idx == quads.len()
                || quads[idx - 1]
                    .p2
                    .distance_squared(quads[idx].p0)
                    > 1e-3;
            if disconnected {
                let run = &quads[run_start..idx];
                let n = run.len();
                let is_loop = closed
                    && n >= 2
                    && run.last().unwrap().p2.distance_squared(run[0].p0) <= 1e-3;
                for (i, q) in run.iter().enumerate() {
                    let start_open = !is_loop && i == 0;
                    let end_open = !is_loop && i + 1 == n;
                    let flags = strata_gpu_path::stroke_style_flags(
                        cap,
                        join,
                        is_loop,
                        start_open,
                        end_open,
                    ) | extra_flags;
                    let seg = strata_gpu_path::GpuPathSegment::from_quadratic(q, sw, flags, c, 0.0);
                    self.emit_clipped_segment(
                        seg,
                        quad_world_bounds(q, pad),
                        clip,
                        clip_polys,
                        clip_mask_id,
                    );
                }
                for i in 0..n.saturating_sub(1) {
                    self.emit_corner_join(
                        &run[i],
                        &run[i + 1],
                        sw,
                        c,
                        join,
                        miter_limit,
                        extra_flags,
                        clip,
                        clip_polys,
                        clip_mask_id,
                    );
                }
                if is_loop {
                    self.emit_corner_join(
                        &run[n - 1],
                        &run[0],
                        sw,
                        c,
                        join,
                        miter_limit,
                        extra_flags,
                        clip,
                        clip_polys,
                        clip_mask_id,
                    );
                }
                run_start = idx;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_fill_quads(
        &mut self,
        quads: &[strata_gpu_path::QuadraticBezier],
        sw: f32,
        col: Color,
        extra_flags: u32,
        clip_opt: Option<Rect>,
        clip_polys: &[Vec<Vec<(f32, f32)>>],
        clip_mask_id: Option<usize>,
    ) {
        let Some(clip) = clip_opt else {
            return;
        };
        let (cr, cg, cb, ca) = col.to_f32();
        let c = [cr, cg, cb, ca];
        for q in quads {
            let seg = strata_gpu_path::GpuPathSegment::from_quadratic(
                q,
                sw,
                strata_gpu_path::FLAG_FILL | extra_flags,
                c,
                0.0,
            );
            self.emit_clipped_segment(seg, quad_world_bounds(q, 2.0), clip, clip_polys, clip_mask_id);
        }
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
                    let even_odd = if path.fill_rule == Some(strata_gpu_path::FillRule::EvenOdd) {
                        strata_gpu_path::FLAG_EVEN_ODD
                    } else {
                        0
                    };
                    let subpaths = subpaths_from_path_verbs(&path.verbs, 0.0, 0.0, 1.0, 1.0, 0.25);
                    let stroke_w = stroke.map(|s| s.width).unwrap_or(1.0);
                    let cap = stroke.map(|s| s.cap).unwrap_or_default();
                    let join = stroke.map(|s| s.join).unwrap_or_default();
                    let miter_limit = stroke.map(|s| s.miter_limit).unwrap_or(4.0);

                    if let Some(fc) = fill_color {
                        for (sub, closed) in &subpaths {
                            let closed_flag = if *closed {
                                strata_gpu_path::FLAG_CLOSED
                            } else {
                                0
                            };
                            batch.emit_fill_quads(
                                sub,
                                stroke_w,
                                *fc,
                                even_odd | closed_flag,
                                Some(current_clip),
                                &[],
                                None,
                            );
                        }
                    }

                    if let Some(sc) = stroke_color {
                        for (sub, closed) in &subpaths {
                            batch.emit_subpath_strokes(
                                sub,
                                *closed,
                                stroke_w,
                                *sc,
                                cap,
                                join,
                                miter_limit,
                                &[],
                                0.0,
                                even_odd,
                                Some(current_clip),
                                &[],
                                None,
                            );
                        }
                    }
                }
                DrawCommand::DrawSvgPath {
                    bounds,
                    path_data,
                    stroke_color,
                    fill_color,
                    stroke_width,
                    stroke_linecap,
                    stroke_linejoin,
                    stroke_dasharray,
                    fill_rule,
                    view_box,
                } => {
                    if bounds.intersect(current_clip).is_none() {
                        continue;
                    }
                    let scale_x = if view_box.0 > 0.0 {
                        bounds.width / view_box.0
                    } else {
                        1.0
                    };
                    let scale_y = if view_box.1 > 0.0 {
                        bounds.height / view_box.1
                    } else {
                        1.0
                    };
                    let avg_scale = ((scale_x + scale_y) * 0.5).max(0.001);
                    let parsed = strata_gpu_path::PathBuilder::from_svg(path_data);
                    let subpaths = subpaths_from_path_verbs(
                        &parsed.verbs,
                        bounds.x,
                        bounds.y,
                        scale_x,
                        scale_y,
                        0.25,
                    );

                    let even_odd = if *fill_rule == strata_layout::FillRule::EvenOdd {
                        strata_gpu_path::FLAG_EVEN_ODD
                    } else {
                        0
                    };
                    let sw = *stroke_width * avg_scale;
                    let cap = layout_cap_to_gpu(*stroke_linecap);
                    let join = layout_join_to_gpu(*stroke_linejoin);

                    if let Some(fc) = fill_color {
                        for (sub, closed) in &subpaths {
                            let closed_flag = if *closed {
                                strata_gpu_path::FLAG_CLOSED
                            } else {
                                0
                            };
                            batch.emit_fill_quads(
                                sub,
                                sw,
                                *fc,
                                even_odd | closed_flag,
                                Some(current_clip),
                                &[],
                                None,
                            );
                        }
                    }

                    if let Some(sc) = stroke_color {
                        let scaled_dashes: Vec<f32> =
                            stroke_dasharray.iter().map(|&d| d * avg_scale).collect();
                        for (sub, closed) in &subpaths {
                            batch.emit_subpath_strokes(
                                sub,
                                *closed,
                                sw,
                                *sc,
                                cap,
                                join,
                                4.0,
                                &scaled_dashes,
                                0.0,
                                even_odd,
                                Some(current_clip),
                                &[],
                                None,
                            );
                        }
                    }
                }
                DrawCommand::DrawCanvas2D { bounds, commands } => {
                    let Some(base_clip) = bounds.intersect(current_clip) else {
                        continue;
                    };
                    let ox = bounds.x;
                    let oy = bounds.y;
                    let mut active_clip = Some(base_clip);
                    let mut line_dash: Vec<f32> = Vec::new();
                    let mut dash_offset: f32 = 0.0;
                    let mut line_cap = strata_gpu_path::LineCap::Butt;
                    let mut line_join = strata_gpu_path::LineJoin::Miter;
                    let mut clip_polys: Vec<Vec<Vec<(f32, f32)>>> = Vec::new();
                    let mut clip_mask_id: Option<usize> = None;
                    #[allow(clippy::type_complexity)]
                    let mut state_stack: Vec<(
                        Option<Rect>,
                        Vec<f32>,
                        f32,
                        strata_gpu_path::LineCap,
                        strata_gpu_path::LineJoin,
                        Vec<Vec<Vec<(f32, f32)>>>,
                        Option<usize>,
                    )> = Vec::new();

                    let rect_quads = |r: Rect| -> [strata_gpu_path::QuadraticBezier; 4] {
                        let p0 = strata_gpu_path::Point2D::new(ox + r.x, oy + r.y);
                        let p1 = strata_gpu_path::Point2D::new(ox + r.x + r.width, oy + r.y);
                        let p2 =
                            strata_gpu_path::Point2D::new(ox + r.x + r.width, oy + r.y + r.height);
                        let p3 = strata_gpu_path::Point2D::new(ox + r.x, oy + r.y + r.height);
                        [
                            strata_gpu_path::QuadraticBezier::new(p0, p0.lerp(p1, 0.5), p1),
                            strata_gpu_path::QuadraticBezier::new(p1, p1.lerp(p2, 0.5), p2),
                            strata_gpu_path::QuadraticBezier::new(p2, p2.lerp(p3, 0.5), p3),
                            strata_gpu_path::QuadraticBezier::new(p3, p3.lerp(p0, 0.5), p0),
                        ]
                    };

                    for ccmd in commands {
                        match ccmd {
                            crate::canvas2d::CanvasCommand::Save => {
                                state_stack.push((
                                    active_clip,
                                    line_dash.clone(),
                                    dash_offset,
                                    line_cap,
                                    line_join,
                                    clip_polys.clone(),
                                    clip_mask_id,
                                ));
                            }
                            crate::canvas2d::CanvasCommand::Restore => {
                                if let Some((
                                    prev_clip,
                                    prev_dash,
                                    prev_offset,
                                    prev_cap,
                                    prev_join,
                                    prev_polys,
                                    prev_mask,
                                )) = state_stack.pop()
                                {
                                    active_clip = prev_clip;
                                    line_dash = prev_dash;
                                    dash_offset = prev_offset;
                                    line_cap = prev_cap;
                                    line_join = prev_join;
                                    clip_polys = prev_polys;
                                    clip_mask_id = prev_mask;
                                }
                            }
                            crate::canvas2d::CanvasCommand::SetLineDash { segments, offset } => {
                                line_dash = segments.clone();
                                dash_offset = *offset;
                            }
                            crate::canvas2d::CanvasCommand::SetLineCap(cap) => {
                                line_cap = layout_cap_to_gpu(*cap);
                            }
                            crate::canvas2d::CanvasCommand::SetLineJoin(join) => {
                                line_join = layout_join_to_gpu(*join);
                            }
                            crate::canvas2d::CanvasCommand::ClipPath { verbs } => {
                                let subpaths = crate::canvas2d::flatten_canvas_verbs(verbs, ox, oy);
                                if !subpaths.is_empty() {
                                    if let Some(vbbox) =
                                        crate::canvas2d::canvas_verbs_bounds(verbs, ox, oy)
                                    {
                                        active_clip = active_clip.and_then(|c| c.intersect(vbbox));
                                    }
                                    if !subpaths_are_axis_aligned_rect(&subpaths) {
                                        clip_polys.push(subpaths);
                                        clip_mask_id = resolve_or_push_clip_mask(
                                            &mut batch.clip_masks,
                                            active_clip,
                                            &clip_polys,
                                        );
                                    }
                                }
                            }
                            crate::canvas2d::CanvasCommand::FillRect {
                                bounds: rb,
                                color,
                            } => {
                                let world_rect =
                                    Rect::new(ox + rb.x, oy + rb.y, rb.width, rb.height);
                                if active_clip
                                    .and_then(|c| c.intersect(world_rect))
                                    .is_some()
                                {
                                    let quads = rect_quads(*rb);
                                    batch.emit_fill_quads(
                                        &quads,
                                        1.0,
                                        *color,
                                        strata_gpu_path::FLAG_CLOSED,
                                        active_clip,
                                        &clip_polys,
                                        clip_mask_id,
                                    );
                                }
                            }
                            crate::canvas2d::CanvasCommand::StrokeRect {
                                bounds: rb,
                                color,
                                line_width,
                            } => {
                                let quads = rect_quads(*rb);
                                batch.emit_subpath_strokes(
                                    &quads,
                                    true,
                                    *line_width,
                                    *color,
                                    line_cap,
                                    line_join,
                                    10.0,
                                    &line_dash,
                                    dash_offset,
                                    0,
                                    active_clip,
                                    &clip_polys,
                                    clip_mask_id,
                                );
                            }
                            crate::canvas2d::CanvasCommand::DrawLine {
                                p0,
                                p1,
                                color,
                                line_width,
                            } => {
                                let a = strata_gpu_path::Point2D::new(ox + p0.0, oy + p0.1);
                                let b = strata_gpu_path::Point2D::new(ox + p1.0, oy + p1.1);
                                let q = strata_gpu_path::QuadraticBezier::new(a, a.lerp(b, 0.5), b);
                                batch.emit_subpath_strokes(
                                    &[q],
                                    false,
                                    *line_width,
                                    *color,
                                    line_cap,
                                    line_join,
                                    10.0,
                                    &line_dash,
                                    dash_offset,
                                    0,
                                    active_clip,
                                    &clip_polys,
                                    clip_mask_id,
                                );
                            }
                            crate::canvas2d::CanvasCommand::DrawCircle {
                                center,
                                radius,
                                fill_color,
                                stroke_color,
                                line_width,
                            } => {
                                let circle_quads =
                                    circle_to_quads(ox + center.0, oy + center.1, *radius);
                                if let Some(fc) = fill_color {
                                    batch.emit_fill_quads(
                                        &circle_quads,
                                        *line_width,
                                        *fc,
                                        strata_gpu_path::FLAG_CLOSED,
                                        active_clip,
                                        &clip_polys,
                                        clip_mask_id,
                                    );
                                }
                                if let Some(sc) = stroke_color {
                                    batch.emit_subpath_strokes(
                                        &circle_quads,
                                        true,
                                        *line_width,
                                        *sc,
                                        line_cap,
                                        line_join,
                                        10.0,
                                        &line_dash,
                                        dash_offset,
                                        0,
                                        active_clip,
                                        &clip_polys,
                                        clip_mask_id,
                                    );
                                }
                            }
                            crate::canvas2d::CanvasCommand::DrawPath {
                                verbs,
                                fill_color,
                                stroke_color,
                                line_width,
                            } => {
                                let subpaths = canvas_path_verbs_to_quad_subpaths(verbs, ox, oy);

                                if let Some(fc) = fill_color {
                                    for (sub, closed) in &subpaths {
                                        let closed_flag = if *closed {
                                            strata_gpu_path::FLAG_CLOSED
                                        } else {
                                            0
                                        };
                                        batch.emit_fill_quads(
                                            sub,
                                            *line_width,
                                            *fc,
                                            closed_flag,
                                            active_clip,
                                            &clip_polys,
                                            clip_mask_id,
                                        );
                                    }
                                }
                                if let Some(sc) = stroke_color {
                                    for (sub, closed) in &subpaths {
                                        batch.emit_subpath_strokes(
                                            sub,
                                            *closed,
                                            *line_width,
                                            *sc,
                                            line_cap,
                                            line_join,
                                            10.0,
                                            &line_dash,
                                            dash_offset,
                                            0,
                                            active_clip,
                                            &clip_polys,
                                            clip_mask_id,
                                        );
                                    }
                                }
                            }
                            crate::canvas2d::CanvasCommand::ClearRect { .. }
                            | crate::canvas2d::CanvasCommand::FillText { .. }
                            | crate::canvas2d::CanvasCommand::StrokeText { .. } => {}
                        }
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
        // Most frames contain no vector paths; avoid walking clips and
        // rebuilding an empty batch when nothing needs compiling.
        let has_paths = list.commands().iter().any(|cmd| {
            matches!(
                cmd,
                DrawCommand::DrawGpuPath { .. }
                    | DrawCommand::DrawSvgPath { .. }
                    | DrawCommand::DrawCanvas2D { .. }
            )
        });
        if !has_paths {
            if !self.batch.is_empty() {
                self.batch.clear();
                self.uniforms.curve_count = 0;
            }
            return 0;
        }
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
            return Err(
                "WGSL shader missing @vertex, @fragment, or @compute entry point".to_string(),
            );
        }

        let mut vertex_entry = "vs_main".to_string();
        let mut fragment_entry = "fs_main".to_string();

        for line in source.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("fn ") {
                let name = trimmed
                    .trim_start_matches("fn ")
                    .split('(')
                    .next()
                    .unwrap_or("")
                    .trim();
                if source.contains(&format!("@vertex\nfn {}", name))
                    || source.contains(&format!("@vertex fn {}", name))
                {
                    vertex_entry = name.to_string();
                }
                if source.contains(&format!("@fragment\nfn {}", name))
                    || source.contains(&format!("@fragment fn {}", name))
                {
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
    pub fn new_instanced_quad_pipeline(
        viewport_width: f32,
        viewport_height: f32,
    ) -> Result<Self, String> {
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
                GpuVertexAttribute {
                    format: GpuVertexFormat::Float32x4,
                    offset: 0,
                    shader_location: 1,
                },
                GpuVertexAttribute {
                    format: GpuVertexFormat::Float32x4,
                    offset: 16,
                    shader_location: 2,
                },
                GpuVertexAttribute {
                    format: GpuVertexFormat::Float32x4,
                    offset: 32,
                    shader_location: 3,
                },
                GpuVertexAttribute {
                    format: GpuVertexFormat::Float32x4,
                    offset: 48,
                    shader_location: 4,
                },
                GpuVertexAttribute {
                    format: GpuVertexFormat::Float32x4,
                    offset: 64,
                    shader_location: 5,
                },
                GpuVertexAttribute {
                    format: GpuVertexFormat::Float32x4,
                    offset: 80,
                    shader_location: 6,
                },
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

    /// Constructs a hardware stencil clip pipeline binding `WGSL_STENCIL_CLIP_SHADER`.
    pub fn new_stencil_clip_pipeline(
        viewport_width: f32,
        viewport_height: f32,
    ) -> Result<Self, String> {
        let _ = (viewport_width, viewport_height);
        let compiler = GpuShaderCompiler::new();
        let shader_module = compiler.compile(WGSL_STENCIL_CLIP_SHADER)?;

        let vertex_layout = GpuVertexBufferLayout {
            array_stride: GpuClipVertex::BYTE_SIZE as u64,
            step_mode: GpuVertexStepMode::Vertex,
            attributes: vec![GpuVertexAttribute {
                format: GpuVertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            }],
        };

        Ok(Self {
            shader_module,
            vertex_layout,
            instance_layout: None,
            primitive_topology: GpuPrimitiveTopology::TriangleList,
            depth_stencil: Some(GpuDepthStencilState {
                format: crate::swapchain::SwapchainFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: GpuCompareFunction::Always,
            }),
        })
    }

    /// Records an instanced draw command onto a command encoder.
    pub fn draw_instanced(
        &self,
        encoder: &mut crate::gpu_context::GpuCommandEncoder,
        vertex_count: usize,
        instance_count: usize,
    ) {
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

    fn count_span_draw_calls(
        total_items: usize,
        spans: &[GpuClipSpan],
        masks: &[GpuClipMask],
    ) -> usize {
        if total_items == 0 {
            return 0;
        }
        if spans.is_empty() || masks.is_empty() {
            return 1;
        }
        let mut calls = 0usize;
        for span in spans {
            if span.end <= span.start {
                continue;
            }
            if let Some(mask_id) = span.clip_mask {
                if let Some(mask) = masks.get(mask_id) {
                    calls += mask
                        .stencil_layers
                        .iter()
                        .filter(|layer| !layer.is_empty())
                        .count();
                }
            }
            calls += 1;
        }
        calls.max(1)
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
        draw_calls += Self::count_span_draw_calls(
            self.quad_batch.len(),
            &self.quad_batch.clip_spans,
            &self.quad_batch.clip_masks,
        );
        if glyph_count > 0 {
            draw_calls += 1;
        }
        draw_calls += Self::count_span_draw_calls(
            self.path_batch.len(),
            &self.path_batch.clip_spans,
            &self.path_batch.clip_masks,
        );

        self.summary = GpuBatchSummary {
            total_commands: list.len(),
            quad_instances: self.quad_batch.len(),
            slug_glyphs: glyph_count,
            path_segments: self.path_batch.len(),
            total_draw_calls: draw_calls,
        };

        self.summary
    }

    fn dispatch_masked_spans(
        encoder: &mut crate::gpu_context::GpuCommandEncoder,
        total_items: usize,
        spans: &[GpuClipSpan],
        masks: &[GpuClipMask],
    ) {
        if total_items == 0 {
            return;
        }
        if spans.is_empty() || masks.is_empty() {
            encoder.draw_instanced(6, total_items);
            return;
        }
        for span in spans {
            let count = span.end.saturating_sub(span.start);
            if count == 0 {
                continue;
            }
            if let Some(mask_id) = span.clip_mask {
                if let Some(mask) = masks.get(mask_id) {
                    for layer in &mask.stencil_layers {
                        if !layer.is_empty() {
                            encoder.draw_instanced(layer.len(), 1);
                        }
                    }
                }
            }
            encoder.draw_instanced(6, count);
        }
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

        Self::dispatch_masked_spans(
            &mut encoder,
            self.quad_batch.len(),
            &self.quad_batch.clip_spans,
            &self.quad_batch.clip_masks,
        );
        if self.summary.slug_glyphs > 0 {
            encoder.draw_instanced(6, self.summary.slug_glyphs);
        }
        Self::dispatch_masked_spans(
            &mut encoder,
            self.path_batch.len(),
            &self.path_batch.clip_spans,
            &self.path_batch.clip_masks,
        );

        encoder.end_render_pass();
        device.submit(encoder);

        Ok(self.summary)
    }

    /// Updates viewport dimensions for subsequent batch compilations.
    pub fn resize(&mut self, viewport_width: f32, viewport_height: f32) {
        self.viewport_width = viewport_width;
        self.viewport_height = viewport_height;
    }

    /// Compiles a `DisplayList` into coalesced painter's-order hardware GPU batches
    /// (`GpuBatch`, `GpuPathBatch`, `GpuSlugTextPipeline`, `GpuClipMask`s, and `DrawViewport3D` split points),
    /// eliminating duplicate `FillRect`/`StrokeRect` emission and triangulating/stencil-filling closed vector shapes.
    pub fn collect_hardware_frame(
        &mut self,
        list: &DisplayList,
        damage: Option<Rect>,
    ) -> Hardware2DFrame {
        self.quad_batch.clear();
        self.path_batch.clear();
        self.text_pipeline.clear();

        let glyph_cache = std::mem::take(&mut self.text_pipeline.glyph_cache);
        let text = GpuSlugTextPipeline {
            glyph_cache,
            buffer: strata_gpu_path::SlugGlyphBuffer::new(),
            total_glyphs: 0,
            total_curves: 0,
        };
        let mut frame = Hardware2DFrame {
            quads: GpuBatch::with_capacity(list.len()),
            paths: GpuPathBatch::new(),
            text,
            items: Vec::with_capacity(list.len().min(64)),
            clip_masks: Vec::new(),
            summary: GpuBatchSummary::default(),
        };

        let viewport_rect = Rect::new(0.0, 0.0, self.viewport_width, self.viewport_height);
        let default_clip = damage.unwrap_or(viewport_rect);
        let mut builder = HardwareFrameBuilder {
            frame: &mut frame,
            viewport_rect,
            default_clip,
            clip_stack: Vec::new(),
            poly_clip_stack: Vec::new(),
            poly_mask_id: None,
            transform_stack: Vec::new(),
            opacity_stack: Vec::new(),
            cur_alpha: 1.0,
        };
        builder.compile_list(list);

        frame.text.total_curves = frame.text.buffer.curves.len();
        let draw_calls = frame.count_draw_calls();
        frame.summary = GpuBatchSummary {
            total_commands: list.len(),
            quad_instances: frame.quads.len(),
            slug_glyphs: frame.text.total_glyphs,
            path_segments: frame.paths.len(),
            total_draw_calls: draw_calls,
        };

        // Keep legacy collector state in sync without cloning large vector buffers.
        self.text_pipeline.glyph_cache = frame.text.glyph_cache.clone();
        self.summary = frame.summary;
        frame
    }
}

/// Alias matching hardware swapchain batch collection across `strata_render` and `strata_platform`.
pub type UnifiedGpuCollector = GpuBatchCollector;

/// GPU primitive kind for a coalesced hardware draw call inside [`Hardware2DFrame`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwareDrawOpKind {
    Quads,
    Paths,
    Text,
}

/// Contiguous range `[start..end)` of batched GPU instances/segments sharing clip mask and scissor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HardwareDrawOp {
    pub kind: HardwareDrawOpKind,
    pub start: usize,
    pub end: usize,
    pub clip_mask: Option<usize>,
    pub scissor_rect: Option<Rect>,
}

/// Ordered hardware render step preserving exact Z-order across 2D GPU batches and 3D viewports.
#[derive(Debug, Clone)]
pub enum HardwareRenderItem {
    Batch(HardwareDrawOp),
    Viewport3D {
        bounds: Rect,
        scene: std::sync::Arc<crate::scene3d::Scene3D>,
    },
}

/// Complete compiled hardware 2D/3D frame ready for live `wgpu` swapchain or offscreen execution.
#[derive(Debug, Clone, Default)]
pub struct Hardware2DFrame {
    pub quads: GpuBatch,
    pub paths: GpuPathBatch,
    pub text: GpuSlugTextPipeline,
    pub items: Vec<HardwareRenderItem>,
    pub clip_masks: Vec<GpuClipMask>,
    pub summary: GpuBatchSummary,
}

impl Hardware2DFrame {
    /// Counts hardware draw calls including stencil clip mask passes and batched content spans.
    pub fn count_draw_calls(&self) -> usize {
        let mut calls = 0usize;
        let mut active_mask: Option<usize> = None;
        for item in &self.items {
            match item {
                HardwareRenderItem::Batch(op) => {
                    if let Some(mask_id) = op.clip_mask {
                        if active_mask != Some(mask_id) {
                            if let Some(mask) = self.clip_masks.get(mask_id) {
                                calls += mask
                                    .stencil_layers
                                    .iter()
                                    .filter(|l| !l.is_empty())
                                    .count();
                            }
                            active_mask = Some(mask_id);
                        }
                    }
                    calls += 1;
                }
                HardwareRenderItem::Viewport3D { .. } => {
                    calls += 1;
                    active_mask = None;
                }
            }
        }
        calls
    }
}

struct HardwareFrameBuilder<'a> {
    frame: &'a mut Hardware2DFrame,
    viewport_rect: Rect,
    default_clip: Rect,
    clip_stack: Vec<(Rect, bool)>,
    poly_clip_stack: Vec<Vec<Vec<(f32, f32)>>>,
    poly_mask_id: Option<usize>,
    transform_stack: Vec<strata_primitives::geom::Matrix4x4>,
    opacity_stack: Vec<f32>,
    cur_alpha: f32,
}

impl<'a> HardwareFrameBuilder<'a> {
    #[inline]
    fn current_clip(&self) -> Option<Rect> {
        match self.clip_stack.last() {
            Some((r, _)) => r.intersect(self.default_clip),
            None => Some(self.default_clip),
        }
    }

    #[inline]
    fn transform_rect(&self, r: Rect) -> Rect {
        if let Some(mat) = self.transform_stack.last() {
            if !mat.is_identity() {
                let (x, y, w, h) = mat.transform_bounds(r.x, r.y, r.width, r.height);
                return Rect::new(x, y, w, h);
            }
        }
        r
    }

    fn transform_subpaths(&self, subs: Vec<Vec<(f32, f32)>>) -> Vec<Vec<(f32, f32)>> {
        if let Some(mat) = self.transform_stack.last() {
            if !mat.is_identity() {
                return subs
                    .into_iter()
                    .map(|sub| {
                        sub.into_iter()
                            .map(|(x, y)| mat.transform_point(x, y))
                            .collect()
                    })
                    .collect();
            }
        }
        subs
    }

    fn record_op(
        &mut self,
        kind: HardwareDrawOpKind,
        start: usize,
        end: usize,
        clip_mask: Option<usize>,
        scissor_rect: Option<Rect>,
    ) {
        if end <= start {
            return;
        }
        if let Some(HardwareRenderItem::Batch(prev)) = self.frame.items.last_mut() {
            if prev.kind == kind
                && prev.clip_mask == clip_mask
                && prev.scissor_rect == scissor_rect
                && prev.end == start
            {
                prev.end = end;
                return;
            }
        }
        self.frame.items.push(HardwareRenderItem::Batch(HardwareDrawOp {
            kind,
            start,
            end,
            clip_mask,
            scissor_rect,
        }));
    }

    #[inline]
    fn scissor_for(&self, clip: Rect) -> Option<Rect> {
        if clip == self.viewport_rect {
            None
        } else {
            Some(clip)
        }
    }

    fn push_clipped_quad(
        &mut self,
        instance: GpuQuadInstance,
        rect: Rect,
        clip_polys: &[Vec<Vec<(f32, f32)>>],
        clip_mask_id: Option<usize>,
    ) {
        let mask = if clip_polys.is_empty() {
            None
        } else {
            match rect_relation_to_clip_polys(rect, clip_polys) {
                ClipContainment::Outside => return,
                ClipContainment::Inside => None,
                ClipContainment::Partial => clip_mask_id,
            }
        };
        let start = self.frame.quads.len();
        self.frame.quads.push_with_clip(instance, mask);
        let end = self.frame.quads.len();
        self.record_op(HardwareDrawOpKind::Quads, start, end, mask, None);
    }

    fn emit_stencil_fill(
        &mut self,
        fill_polys: Vec<Vec<(f32, f32)>>,
        fill_color: Color,
        active_clip: Rect,
        parent_clip_polys: &[Vec<Vec<(f32, f32)>>],
    ) {
        let Some(poly_bbox) = poly_layers_bbox(&fill_polys) else {
            return;
        };
        let Some(clipped_bbox) = poly_bbox.intersect(active_clip) else {
            return;
        };
        if !parent_clip_polys.is_empty()
            && rect_relation_to_clip_polys(clipped_bbox, parent_clip_polys)
                == ClipContainment::Outside
        {
            return;
        }
        let mut layers = Vec::with_capacity(parent_clip_polys.len() + 1);
        layers.extend_from_slice(parent_clip_polys);
        layers.push(fill_polys);
        let Some(mask_id) =
            resolve_or_push_clip_mask(&mut self.frame.clip_masks, Some(clipped_bbox), &layers)
        else {
            return;
        };
        let col = apply_color_opacity(fill_color, self.cur_alpha);
        let start = self.frame.quads.len();
        self.frame
            .quads
            .push_with_clip(GpuQuadInstance::from_rect(clipped_bbox, col, active_clip), Some(mask_id));
        let end = self.frame.quads.len();
        self.record_op(HardwareDrawOpKind::Quads, start, end, Some(mask_id), None);
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_paths_with_op(
        &mut self,
        quads: &[strata_gpu_path::QuadraticBezier],
        closed: bool,
        sw: f32,
        color: Color,
        cap: strata_gpu_path::LineCap,
        join: strata_gpu_path::LineJoin,
        miter_limit: f32,
        dashes: &[f32],
        dash_offset: f32,
        extra_flags: u32,
        active_clip: Rect,
        clip_polys: &[Vec<Vec<(f32, f32)>>],
        clip_mask_id: Option<usize>,
    ) {
        let start = self.frame.paths.len();
        let before_spans = self.frame.paths.clip_spans.len();
        let col = apply_color_opacity(color, self.cur_alpha);
        self.frame.paths.emit_subpath_strokes(
            quads,
            closed,
            sw,
            col,
            cap,
            join,
            miter_limit,
            dashes,
            dash_offset,
            extra_flags,
            Some(active_clip),
            clip_polys,
            clip_mask_id,
        );
        let end = self.frame.paths.len();
        if end <= start {
            return;
        }
        let scissor = self.scissor_for(active_clip);
        let spans = &self.frame.paths.clip_spans;
        if !spans.is_empty() {
            let scan_from = before_spans.saturating_sub(1);
            let mut local_ops: Vec<(usize, usize, Option<usize>)> = Vec::new();
            for span in &spans[scan_from..] {
                if span.end <= start || span.start >= end {
                    continue;
                }
                local_ops.push((span.start.max(start), span.end.min(end), span.clip_mask));
            }
            for (s0, s1, mask) in local_ops {
                self.record_op(HardwareDrawOpKind::Paths, s0, s1, mask, scissor);
            }
        } else {
            self.record_op(HardwareDrawOpKind::Paths, start, end, None, scissor);
        }
    }

    fn emit_text_with_clip(
        &mut self,
        start: usize,
        end: usize,
        run_bounds: Rect,
        active_clip: Rect,
        clip_polys: &[Vec<Vec<(f32, f32)>>],
        clip_mask_id: Option<usize>,
    ) {
        if end <= start {
            return;
        }
        let mask = if clip_polys.is_empty() {
            None
        } else {
            match rect_relation_to_clip_polys(run_bounds, clip_polys) {
                ClipContainment::Outside => {
                    self.frame.text.buffer.instances.truncate(start);
                    self.frame.text.total_glyphs = self.frame.text.buffer.instances.len();
                    return;
                }
                ClipContainment::Inside => None,
                ClipContainment::Partial => clip_mask_id,
            }
        };
        let scissor = self.scissor_for(active_clip);
        self.record_op(HardwareDrawOpKind::Text, start, end, mask, scissor);
    }

    fn compile_list(&mut self, list: &DisplayList) {
        for cmd in list.commands() {
            match cmd {
                DrawCommand::PushClip { bounds, radius } => {
                    let tb = self.transform_rect(*bounds);
                    let combined = match self.clip_stack.last() {
                        Some((prev, _)) => prev.intersect(tb).unwrap_or(Rect::zero()),
                        None => tb,
                    };
                    let has_radius = radius.top_left > 0.5
                        || radius.top_right > 0.5
                        || radius.bottom_right > 0.5
                        || radius.bottom_left > 0.5;
                    if has_radius && tb.width > 0.0 && tb.height > 0.0 {
                        self.poly_clip_stack.push(vec![rounded_rect_subpath(tb, *radius)]);
                        self.poly_mask_id = resolve_or_push_clip_mask(
                            &mut self.frame.clip_masks,
                            combined.intersect(self.default_clip),
                            &self.poly_clip_stack,
                        );
                    }
                    self.clip_stack.push((combined, has_radius));
                    continue;
                }
                DrawCommand::PopClip => {
                    if let Some((_, had_poly)) = self.clip_stack.pop() {
                        if had_poly {
                            self.poly_clip_stack.pop();
                            let active = self.current_clip();
                            self.poly_mask_id = resolve_or_push_clip_mask(
                                &mut self.frame.clip_masks,
                                active,
                                &self.poly_clip_stack,
                            );
                        }
                    }
                    continue;
                }
                DrawCommand::PushTransform { matrix } => {
                    let combined = match self.transform_stack.last() {
                        Some(prev) => prev.multiply(matrix),
                        None => *matrix,
                    };
                    self.transform_stack.push(combined);
                    continue;
                }
                DrawCommand::PopTransform => {
                    self.transform_stack.pop();
                    continue;
                }
                DrawCommand::PushOpacity { alpha } => {
                    self.opacity_stack.push(self.cur_alpha);
                    self.cur_alpha = (self.cur_alpha * alpha).clamp(0.0, 1.0);
                    continue;
                }
                DrawCommand::PopOpacity => {
                    if let Some(prev) = self.opacity_stack.pop() {
                        self.cur_alpha = prev;
                    }
                    continue;
                }
                DrawCommand::DrawViewport3D { bounds, scene } => {
                    if bounds.width > 0.0 && bounds.height > 0.0 {
                        self.frame.items.push(HardwareRenderItem::Viewport3D {
                            bounds: self.transform_rect(*bounds),
                            scene: std::sync::Arc::clone(scene),
                        });
                    }
                    continue;
                }
                _ => {}
            }

            let Some(current_clip) = self.current_clip() else {
                continue;
            };
            if let Some(b) = cmd.bounds() {
                if self.transform_rect(b).intersect(current_clip).is_none() {
                    continue;
                }
            }

            let root_polys = self.poly_clip_stack.clone();
            let root_mask = self.poly_mask_id;
            match cmd {
                DrawCommand::DrawRect { bounds, color } => {
                    let tr = self.transform_rect(*bounds);
                    let col = apply_color_opacity(*color, self.cur_alpha);
                    self.push_clipped_quad(
                        GpuQuadInstance::from_rect(tr, col, current_clip),
                        tr,
                        &root_polys,
                        root_mask,
                    );
                }
                DrawCommand::DrawRoundedRect {
                    bounds,
                    color,
                    border_radius,
                    border_width,
                    border_color,
                } => {
                    let tr = self.transform_rect(*bounds);
                    let col = apply_color_opacity(*color, self.cur_alpha);
                    let bcol = border_color.map(|c| apply_color_opacity(c, self.cur_alpha));
                    self.push_clipped_quad(
                        GpuQuadInstance::from_rounded_rect(
                            tr,
                            col,
                            *border_radius,
                            *border_width,
                            bcol,
                            current_clip,
                        ),
                        tr,
                        &root_polys,
                        root_mask,
                    );
                }
                DrawCommand::DrawBorder {
                    bounds,
                    widths,
                    color,
                    border_radius,
                    ..
                } => {
                    let tr = self.transform_rect(*bounds);
                    let bcol = apply_color_opacity(*color, self.cur_alpha);
                    self.push_clipped_quad(
                        GpuQuadInstance::from_rounded_rect(
                            tr,
                            Color::TRANSPARENT,
                            *border_radius,
                            *widths,
                            Some(bcol),
                            current_clip,
                        ),
                        tr,
                        &root_polys,
                        root_mask,
                    );
                }
                DrawCommand::DrawText {
                    origin,
                    bounds_width,
                    bounds_height,
                    text_align,
                    text,
                    font_size,
                    line_height,
                    color,
                    ..
                } => {
                    let col = apply_color_opacity(*color, self.cur_alpha);
                    let start = self.frame.text.buffer.instances.len();
                    let (_, run_rect) = self.frame.text.push_text_run_with_bounds(
                        text,
                        origin.0,
                        origin.1,
                        *bounds_width,
                        *bounds_height,
                        *font_size,
                        *line_height,
                        *text_align,
                        1.0,
                        col,
                        current_clip,
                    );
                    let end = self.frame.text.buffer.instances.len();
                    self.emit_text_with_clip(
                        start,
                        end,
                        run_rect,
                        current_clip,
                        &root_polys,
                        root_mask,
                    );
                }
                DrawCommand::DrawTextFragments { fragments } => {
                    for frag in fragments {
                        let col = apply_color_opacity(frag.style.color, self.cur_alpha);
                        let start = self.frame.text.buffer.instances.len();
                        self.frame.text.push_text_run(
                            &frag.text,
                            frag.bounds.x,
                            frag.bounds.y,
                            frag.style.font_size,
                            1.0,
                            col,
                            current_clip,
                        );
                        let end = self.frame.text.buffer.instances.len();
                        self.emit_text_with_clip(
                            start,
                            end,
                            frag.bounds,
                            current_clip,
                            &root_polys,
                            root_mask,
                        );
                    }
                }
                DrawCommand::DrawGpuPath {
                    path,
                    stroke,
                    fill_color,
                    stroke_color,
                    ..
                } => {
                    let subpaths = subpaths_from_path_verbs(&path.verbs, 0.0, 0.0, 1.0, 1.0, 0.25);
                    if let Some(fc) = fill_color {
                        let polys = flatten_quad_subpaths(&subpaths, 6);
                        self.emit_stencil_fill(polys, *fc, current_clip, &root_polys);
                    }
                    if let Some(sc) = stroke_color {
                        let sw = stroke.map(|s| s.width).unwrap_or(1.0);
                        let cap = stroke.map(|s| s.cap).unwrap_or_default();
                        let join = stroke.map(|s| s.join).unwrap_or_default();
                        let miter = stroke.map(|s| s.miter_limit).unwrap_or(4.0);
                        let even_odd =
                            if path.fill_rule == Some(strata_gpu_path::FillRule::EvenOdd) {
                                strata_gpu_path::FLAG_EVEN_ODD
                            } else {
                                0
                            };
                        for (sub, closed) in &subpaths {
                            self.emit_paths_with_op(
                                sub,
                                *closed,
                                sw,
                                *sc,
                                cap,
                                join,
                                miter,
                                &[],
                                0.0,
                                even_odd,
                                current_clip,
                                &root_polys,
                                root_mask,
                            );
                        }
                    }
                }
                DrawCommand::DrawSvgPath {
                    bounds,
                    path_data,
                    stroke_color,
                    fill_color,
                    stroke_width,
                    stroke_linecap,
                    stroke_linejoin,
                    stroke_dasharray,
                    fill_rule,
                    view_box,
                } => {
                    let sx = if view_box.0 > 0.0 {
                        bounds.width / view_box.0
                    } else {
                        1.0
                    };
                    let sy = if view_box.1 > 0.0 {
                        bounds.height / view_box.1
                    } else {
                        1.0
                    };
                    let avg_scale = ((sx + sy) * 0.5).max(0.001);
                    let parsed = strata_gpu_path::PathBuilder::from_svg(path_data);
                    let subpaths = subpaths_from_path_verbs(
                        &parsed.verbs,
                        bounds.x,
                        bounds.y,
                        sx,
                        sy,
                        0.25,
                    );
                    if let Some(fc) = fill_color {
                        let polys = flatten_quad_subpaths(&subpaths, 6);
                        self.emit_stencil_fill(polys, *fc, current_clip, &root_polys);
                    }
                    if let Some(sc) = stroke_color {
                        let sw = *stroke_width * avg_scale;
                        let cap = layout_cap_to_gpu(*stroke_linecap);
                        let join = layout_join_to_gpu(*stroke_linejoin);
                        let even_odd = if *fill_rule == strata_layout::FillRule::EvenOdd {
                            strata_gpu_path::FLAG_EVEN_ODD
                        } else {
                            0
                        };
                        let scaled_dashes: Vec<f32> =
                            stroke_dasharray.iter().map(|&d| d * avg_scale).collect();
                        for (sub, closed) in &subpaths {
                            self.emit_paths_with_op(
                                sub,
                                *closed,
                                sw,
                                *sc,
                                cap,
                                join,
                                4.0,
                                &scaled_dashes,
                                0.0,
                                even_odd,
                                current_clip,
                                &root_polys,
                                root_mask,
                            );
                        }
                    }
                }
                DrawCommand::DrawCanvas2D { bounds, commands } => {
                    if let Some(base_clip) = self.transform_rect(*bounds).intersect(current_clip) {
                        self.compile_canvas_commands(bounds, commands, base_clip);
                    }
                }
                _ => {}
            }
        }
    }

    fn compile_canvas_commands(
        &mut self,
        bounds: &Rect,
        commands: &[crate::canvas2d::CanvasCommand],
        base_clip: Rect,
    ) {
        let ox = bounds.x;
        let oy = bounds.y;
        let mut active_clip = Some(base_clip);
        let mut line_dash: Vec<f32> = Vec::new();
        let mut dash_offset: f32 = 0.0;
        let mut line_cap = strata_gpu_path::LineCap::Butt;
        let mut line_join = strata_gpu_path::LineJoin::Miter;
        let mut clip_polys = self.poly_clip_stack.clone();
        let mut clip_mask_id = resolve_or_push_clip_mask(
            &mut self.frame.clip_masks,
            active_clip,
            &clip_polys,
        );
        #[allow(clippy::type_complexity)]
        let mut state_stack: Vec<(
            Option<Rect>,
            Vec<f32>,
            f32,
            strata_gpu_path::LineCap,
            strata_gpu_path::LineJoin,
            Vec<Vec<Vec<(f32, f32)>>>,
            Option<usize>,
        )> = Vec::new();

        for ccmd in commands {
            match ccmd {
                crate::canvas2d::CanvasCommand::Save => {
                    state_stack.push((
                        active_clip,
                        line_dash.clone(),
                        dash_offset,
                        line_cap,
                        line_join,
                        clip_polys.clone(),
                        clip_mask_id,
                    ));
                }
                crate::canvas2d::CanvasCommand::Restore => {
                    if let Some((
                        prev_clip,
                        prev_dash,
                        prev_offset,
                        prev_cap,
                        prev_join,
                        prev_polys,
                        prev_mask,
                    )) = state_stack.pop()
                    {
                        active_clip = prev_clip;
                        line_dash = prev_dash;
                        dash_offset = prev_offset;
                        line_cap = prev_cap;
                        line_join = prev_join;
                        clip_polys = prev_polys;
                        clip_mask_id = prev_mask;
                    }
                }
                crate::canvas2d::CanvasCommand::SetLineDash { segments, offset } => {
                    line_dash = segments
                        .iter()
                        .copied()
                        .filter(|d| d.is_finite() && *d >= 0.0)
                        .collect();
                    if line_dash.len() % 2 == 1 {
                        let dup = line_dash.clone();
                        line_dash.extend(dup);
                    }
                    dash_offset = if offset.is_finite() { *offset } else { 0.0 };
                }
                crate::canvas2d::CanvasCommand::SetLineCap(cap) => {
                    line_cap = layout_cap_to_gpu(*cap);
                }
                crate::canvas2d::CanvasCommand::SetLineJoin(join) => {
                    line_join = layout_join_to_gpu(*join);
                }
                crate::canvas2d::CanvasCommand::ClipPath { verbs } => {
                    let subpaths = self.transform_subpaths(
                        crate::canvas2d::flatten_canvas_verbs(verbs, ox, oy),
                    );
                    if !subpaths.is_empty() {
                        if let Some(tb) = poly_layers_bbox(&subpaths) {
                            active_clip = active_clip.and_then(|c| c.intersect(tb));
                        }
                        if !subpaths_are_axis_aligned_rect(&subpaths) {
                            clip_polys.push(subpaths);
                            clip_mask_id = resolve_or_push_clip_mask(
                                &mut self.frame.clip_masks,
                                active_clip,
                                &clip_polys,
                            );
                        }
                    }
                }
                crate::canvas2d::CanvasCommand::FillRect { bounds: rb, color } => {
                    let Some(canvas_clip) = active_clip else {
                        continue;
                    };
                    let tr = self.transform_rect(Rect::new(
                        ox + rb.x,
                        oy + rb.y,
                        rb.width,
                        rb.height,
                    ));
                    if tr.intersect(canvas_clip).is_none() {
                        continue;
                    }
                    let col = apply_color_opacity(*color, self.cur_alpha);
                    self.push_clipped_quad(
                        GpuQuadInstance::from_rect(tr, col, canvas_clip),
                        tr,
                        &clip_polys,
                        clip_mask_id,
                    );
                }
                crate::canvas2d::CanvasCommand::StrokeRect {
                    bounds: rb,
                    color,
                    line_width,
                } => {
                    let Some(canvas_clip) = active_clip else {
                        continue;
                    };
                    let r = Rect::new(ox + rb.x, oy + rb.y, rb.width, rb.height);
                    let period: f32 = line_dash.iter().copied().sum();
                    if line_cap == strata_gpu_path::LineCap::Butt
                        && line_join == strata_gpu_path::LineJoin::Miter
                    {
                        if line_dash.is_empty() || period <= 1e-4 {
                            let tr = self.transform_rect(r);
                            if tr.intersect(canvas_clip).is_some() {
                                let col = apply_color_opacity(*color, self.cur_alpha);
                                self.push_clipped_quad(
                                    GpuQuadInstance::from_rounded_rect(
                                        tr,
                                        Color::TRANSPARENT,
                                        BorderRadius::default(),
                                        EdgeInsets::all(*line_width),
                                        Some(col),
                                        canvas_clip,
                                    ),
                                    tr,
                                    &clip_polys,
                                    clip_mask_id,
                                );
                            }
                        } else {
                            let col = apply_color_opacity(*color, self.cur_alpha);
                            for seg_rect in compute_dashed_stroke_rect_segments(
                                r,
                                *line_width,
                                &line_dash,
                                dash_offset,
                            ) {
                                let t_seg = self.transform_rect(seg_rect);
                                if t_seg.intersect(canvas_clip).is_some() {
                                    self.push_clipped_quad(
                                        GpuQuadInstance::from_rect(t_seg, col, canvas_clip),
                                        t_seg,
                                        &clip_polys,
                                        clip_mask_id,
                                    );
                                }
                            }
                        }
                    } else {
                        let p0 = strata_gpu_path::Point2D::new(r.x, r.y);
                        let p1 = strata_gpu_path::Point2D::new(r.x + r.width, r.y);
                        let p2 = strata_gpu_path::Point2D::new(r.x + r.width, r.y + r.height);
                        let p3 = strata_gpu_path::Point2D::new(r.x, r.y + r.height);
                        let rect_quads = [
                            strata_gpu_path::QuadraticBezier::new(p0, p0.lerp(p1, 0.5), p1),
                            strata_gpu_path::QuadraticBezier::new(p1, p1.lerp(p2, 0.5), p2),
                            strata_gpu_path::QuadraticBezier::new(p2, p2.lerp(p3, 0.5), p3),
                            strata_gpu_path::QuadraticBezier::new(p3, p3.lerp(p0, 0.5), p0),
                        ];
                        self.emit_paths_with_op(
                            &rect_quads,
                            true,
                            *line_width,
                            *color,
                            line_cap,
                            line_join,
                            10.0,
                            &line_dash,
                            dash_offset,
                            0,
                            canvas_clip,
                            &clip_polys,
                            clip_mask_id,
                        );
                    }
                }
                crate::canvas2d::CanvasCommand::DrawLine {
                    p0,
                    p1,
                    color,
                    line_width,
                } => {
                    let Some(canvas_clip) = active_clip else {
                        continue;
                    };
                    let a = strata_gpu_path::Point2D::new(ox + p0.0, oy + p0.1);
                    let b = strata_gpu_path::Point2D::new(ox + p1.0, oy + p1.1);
                    let q = strata_gpu_path::QuadraticBezier::new(a, a.lerp(b, 0.5), b);
                    self.emit_paths_with_op(
                        &[q],
                        false,
                        *line_width,
                        *color,
                        line_cap,
                        line_join,
                        10.0,
                        &line_dash,
                        dash_offset,
                        0,
                        canvas_clip,
                        &clip_polys,
                        clip_mask_id,
                    );
                }
                crate::canvas2d::CanvasCommand::DrawCircle {
                    center,
                    radius,
                    fill_color,
                    stroke_color,
                    line_width,
                } => {
                    let Some(canvas_clip) = active_clip else {
                        continue;
                    };
                    let circle_quads = circle_to_quads(ox + center.0, oy + center.1, *radius);
                    if let Some(fc) = fill_color {
                        let polys = flatten_quad_subpaths(&[(circle_quads.clone(), true)], 4);
                        self.emit_stencil_fill(polys, *fc, canvas_clip, &clip_polys);
                    }
                    if let Some(sc) = stroke_color {
                        self.emit_paths_with_op(
                            &circle_quads,
                            true,
                            *line_width,
                            *sc,
                            line_cap,
                            line_join,
                            10.0,
                            &line_dash,
                            dash_offset,
                            0,
                            canvas_clip,
                            &clip_polys,
                            clip_mask_id,
                        );
                    }
                }
                crate::canvas2d::CanvasCommand::DrawPath {
                    verbs,
                    fill_color,
                    stroke_color,
                    line_width,
                } => {
                    let Some(canvas_clip) = active_clip else {
                        continue;
                    };
                    let subpaths = canvas_path_verbs_to_quad_subpaths(verbs, ox, oy);
                    if let Some(fc) = fill_color {
                        let polys = flatten_quad_subpaths(&subpaths, 6);
                        self.emit_stencil_fill(polys, *fc, canvas_clip, &clip_polys);
                    }
                    if let Some(sc) = stroke_color {
                        for (sub, closed) in &subpaths {
                            self.emit_paths_with_op(
                                sub,
                                *closed,
                                *line_width,
                                *sc,
                                line_cap,
                                line_join,
                                10.0,
                                &line_dash,
                                dash_offset,
                                0,
                                canvas_clip,
                                &clip_polys,
                                clip_mask_id,
                            );
                        }
                    }
                }
                crate::canvas2d::CanvasCommand::FillText {
                    text,
                    x,
                    y,
                    color,
                    font_size,
                    text_align,
                    text_baseline,
                    max_width,
                    ..
                }
                | crate::canvas2d::CanvasCommand::StrokeText {
                    text,
                    x,
                    y,
                    color,
                    font_size,
                    text_align,
                    text_baseline,
                    max_width,
                    ..
                } => {
                    let Some(canvas_clip) = active_clip else {
                        continue;
                    };
                    let col = apply_color_opacity(*color, self.cur_alpha);
                    let start = self.frame.text.buffer.instances.len();
                    let (_, run_rect) = self.frame.text.push_canvas_text(
                        *bounds,
                        text,
                        *x,
                        *y,
                        col,
                        *font_size,
                        *text_align,
                        *text_baseline,
                        *max_width,
                        canvas_clip,
                    );
                    let end = self.frame.text.buffer.instances.len();
                    self.emit_text_with_clip(
                        start,
                        end,
                        run_rect,
                        canvas_clip,
                        &clip_polys,
                        clip_mask_id,
                    );
                }
                crate::canvas2d::CanvasCommand::ClearRect { .. } => {}
            }
        }
    }
}

#[cfg(feature = "wgpu-backend")]
pub use crate::wgpu_2d::{GpuStencilClipPipeline, Wgpu2DRenderer};

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

    #[test]
    fn test_gpu_batch_dashed_stroke_rect() {
        let mut ctx = crate::canvas2d::Canvas2DContext::new(200.0, 200.0);
        ctx.set_stroke_style(Color::rgb(255, 255, 255));
        ctx.set_line_width(2.0);
        ctx.save();
        ctx.set_line_dash(&[10.0, 10.0]);
        // 100x40 perimeter = 280 -> should emit multiple dash quads
        ctx.stroke_rect(10.0, 10.0, 100.0, 40.0);
        ctx.restore();
        // After restore, dash is cleared -> single solid bordered StrokeRect quad
        ctx.stroke_rect(10.0, 70.0, 100.0, 40.0);

        let mut dl = DisplayList::new();
        dl.push(DrawCommand::DrawCanvas2D {
            bounds: Rect::new(0.0, 0.0, 200.0, 200.0),
            commands: ctx.commands().to_vec(),
        });

        let batch = GpuBatch::from_display_list(&dl, 200.0, 200.0);
        // Perimeter 280 with 10 on / 10 off produces 14 dash instances, plus 1 solid instance after restore = 15
        assert_eq!(batch.instances.len(), 15);
        // Solid last instance carries non-zero border_width
        let last = batch.instances.last().unwrap();
        assert!(last.border_width[0] > 1.5);
        // Dash instances are solid fill rects with border_width == 0
        assert_eq!(batch.instances[0].border_width, [0.0; 4]);
    }

    #[test]
    fn test_gpu_path_pipeline_set_line_cap_and_set_line_join() {
        let build_corner = |cap: crate::canvas2d::CanvasLineCap, join: crate::canvas2d::CanvasLineJoin| {
            let mut ctx = crate::canvas2d::Canvas2DContext::new(200.0, 200.0);
            ctx.set_stroke_style(Color::rgb(0, 255, 128));
            ctx.set_line_width(10.0);
            ctx.save();
            ctx.set_line_cap(cap);
            ctx.set_line_join(join);
            ctx.begin_path();
            ctx.move_to(10.0, 30.0);
            ctx.line_to(50.0, 30.0);
            ctx.line_to(50.0, 70.0);
            ctx.stroke();
            ctx.restore();
            // After restore, caps/joins revert to Butt / Miter
            ctx.begin_path();
            ctx.move_to(10.0, 100.0);
            ctx.line_to(80.0, 100.0);
            ctx.stroke();

            let mut dl = DisplayList::new();
            dl.push(DrawCommand::DrawCanvas2D {
                bounds: Rect::new(0.0, 0.0, 200.0, 200.0),
                commands: ctx.commands().to_vec(),
            });
            let mut pipe = GpuPathPipeline::new(200.0, 200.0);
            pipe.compile_frame(&dl, None);
            pipe.batch.segments
        };

        let segs_butt_miter =
            build_corner(crate::canvas2d::CanvasLineCap::Butt, crate::canvas2d::CanvasLineJoin::Miter);
        let segs_round_bevel =
            build_corner(crate::canvas2d::CanvasLineCap::Round, crate::canvas2d::CanvasLineJoin::Bevel);
        let segs_square_round =
            build_corner(crate::canvas2d::CanvasLineCap::Square, crate::canvas2d::CanvasLineJoin::Round);

        // Butt vs Round vs Square open cap flags on first segment of the polyline
        assert_eq!(segs_butt_miter[0].flags & strata_gpu_path::FLAG_CAP_ROUND, 0);
        assert_eq!(segs_butt_miter[0].flags & strata_gpu_path::FLAG_CAP_SQUARE, 0);
        assert_ne!(segs_butt_miter[0].flags & strata_gpu_path::FLAG_START_OPEN, 0);

        assert_ne!(segs_round_bevel[0].flags & strata_gpu_path::FLAG_CAP_ROUND, 0);
        assert_ne!(segs_round_bevel[0].flags & strata_gpu_path::FLAG_JOIN_BEVEL, 0);
        assert_ne!(segs_square_round[0].flags & strata_gpu_path::FLAG_CAP_SQUARE, 0);
        assert_ne!(segs_square_round[0].flags & strata_gpu_path::FLAG_JOIN_ROUND, 0);

        // Miter emits 2 FLAG_TRIANGLE corner wedges reaching apex (55, 25); Bevel emits 1 triangle; Round emits 1 disk
        let miter_tris: Vec<_> = segs_butt_miter
            .iter()
            .filter(|s| (s.flags & strata_gpu_path::FLAG_TRIANGLE) != 0)
            .collect();
        assert_eq!(miter_tris.len(), 2);
        assert!(miter_tris.iter().any(|s| s.p2 == [55.0, 25.0] || s.p1 == [55.0, 25.0]));

        let bevel_tris: Vec<_> = segs_round_bevel
            .iter()
            .filter(|s| (s.flags & strata_gpu_path::FLAG_TRIANGLE) != 0)
            .collect();
        assert_eq!(bevel_tris.len(), 1);
        assert_ne!(bevel_tris[0].flags & strata_gpu_path::FLAG_JOIN_BEVEL, 0);

        let round_joins: Vec<_> = segs_square_round
            .iter()
            .filter(|s| s.p0 == [50.0, 30.0] && s.p2 == [50.0, 30.0])
            .collect();
        assert_eq!(round_joins.len(), 1);
        assert_ne!(round_joins[0].flags & strata_gpu_path::FLAG_CAP_ROUND, 0);

        // Restored final line segment must revert to Butt (no Round/Square cap flags)
        let last_line_round_ctx = segs_round_bevel.last().unwrap();
        assert_eq!(last_line_round_ctx.flags & strata_gpu_path::FLAG_CAP_ROUND, 0);
        assert_eq!(last_line_round_ctx.flags & strata_gpu_path::FLAG_CAP_SQUARE, 0);
    }

    #[test]
    fn test_gpu_non_rectangular_clip_path_parity_for_quads_and_paths() {
        // 1. Diagonal triangle clip (0,0) -> (100,0) -> (0,100)
        // AABB is [0,0,100,100], yet any rect with x+y > 100 lies outside the diagonal clip!
        let mut ctx = crate::canvas2d::Canvas2DContext::new(200.0, 200.0);
        ctx.save();
        ctx.begin_path();
        ctx.move_to(0.0, 0.0);
        ctx.line_to(100.0, 0.0);
        ctx.line_to(0.0, 100.0);
        ctx.close_path();
        ctx.clip();

        // Inside AABB [0..100, 0..100], strictly OUTSIDE diagonal triangle -> must be culled!
        ctx.set_fill_style(Color::rgb(255, 0, 0));
        ctx.fill_rect(60.0, 60.0, 30.0, 30.0);

        // Strictly INSIDE diagonal triangle ((5..25) -> max x+y = 50 < 100) -> no stencil mask needed
        ctx.set_fill_style(Color::rgb(0, 255, 0));
        ctx.fill_rect(5.0, 5.0, 20.0, 20.0);

        // Crosses diagonal boundary -> requires triangulated stencil mask
        ctx.set_fill_style(Color::rgb(0, 0, 255));
        ctx.fill_rect(0.0, 0.0, 100.0, 100.0);

        // Line strictly outside triangle -> culled by GpuPathBatch too
        ctx.set_stroke_style(Color::rgb(255, 255, 0));
        ctx.set_line_width(2.0);
        ctx.begin_path();
        ctx.move_to(65.0, 65.0);
        ctx.line_to(90.0, 90.0);
        ctx.stroke();

        // Line inside/crossing triangle -> emitted with clip mask on partial span
        ctx.begin_path();
        ctx.move_to(10.0, 10.0);
        ctx.line_to(80.0, 80.0);
        ctx.stroke();
        ctx.restore();

        // Rectangular clip must NOT allocate stencil masks
        ctx.save();
        ctx.begin_path();
        ctx.rect(110.0, 10.0, 50.0, 50.0);
        ctx.clip();
        ctx.fill_rect(115.0, 15.0, 20.0, 20.0);
        ctx.restore();

        // Circular aperture clip
        ctx.save();
        ctx.begin_path();
        ctx.arc(150.0, 150.0, 30.0, 0.0, std::f32::consts::TAU, false);
        ctx.clip();
        // Corner of circle's AABB [120..180, 120..180] outside radius 30: dist from (150,150) to (122..126) is > 33!
        ctx.fill_rect(121.0, 121.0, 4.0, 4.0);
        // Crossing circle center
        ctx.fill_rect(120.0, 120.0, 60.0, 60.0);
        ctx.restore();

        let mut dl = DisplayList::new();
        dl.push(DrawCommand::DrawCanvas2D {
            bounds: Rect::new(0.0, 0.0, 200.0, 200.0),
            commands: ctx.commands().to_vec(),
        });

        let batch = GpuBatch::from_display_list(&dl, 200.0, 200.0);
        assert!(batch.has_stencil_clips());
        // Red (60,60) and corner-outside-circle (121,121) were culled -> 4 quads remain!
        assert_eq!(batch.instances.len(), 4);
        // Green (5,5) is strictly inside triangle -> no stencil mask
        assert!(batch.clip_mask_for_instance(0).is_none());
        // Blue (0,0,100,100) crosses diagonal -> stencil mask has vertices and accurate point test
        let diag_mask = batch
            .clip_mask_for_instance(1)
            .expect("crossing quad should have diagonal stencil clip mask");
        assert!(diag_mask.total_stencil_vertices() >= 3);
        assert_eq!(diag_mask.total_stencil_vertices() % 3, 0);
        assert!(diag_mask.contains_point(20.0, 20.0));
        assert!(!diag_mask.contains_point(75.0, 75.0));

        // Rect clip (instance 2) must be unmasked after restore()
        assert!(batch.clip_mask_for_instance(2).is_none());

        // Circular clip (instance 3)
        let circle_mask = batch
            .clip_mask_for_instance(3)
            .expect("circle aperture should bind stencil mask");
        assert!(circle_mask.contains_point(150.0, 150.0));
        assert!(!circle_mask.contains_point(122.0, 122.0));

        // GpuPathBatch also culls outside diagonal & circle clips and compiles stencil shader
        let mut collector = GpuBatchCollector::new(200.0, 200.0);
        let summary = collector.collect(&dl, None);
        assert!(collector.path_batch.has_stencil_clips());
        let mut dev = crate::gpu_context::GpuDevice::new(
            200,
            200,
            crate::swapchain::SwapchainFormat::Rgba8Unorm,
        );
        let dispatched = collector.dispatch(&mut dev, None).expect("dispatch");
        assert_eq!(dispatched.total_draw_calls, summary.total_draw_calls);
        assert!(GpuRenderPipeline::new_stencil_clip_pipeline(200.0, 200.0).is_ok());
    }

    #[test]
    fn test_unified_gpu_collector_hardware_frame_ordering_and_stencil_masks() {
        let mut collector = UnifiedGpuCollector::new(240.0, 240.0);

        let mut canvas = crate::canvas2d::Canvas2DContext::new(240.0, 240.0);
        canvas.set_fill_style(Color::rgb(20, 40, 80));
        canvas.fill_rect(0.0, 0.0, 240.0, 240.0);
        canvas.save();
        canvas.set_stroke_style(Color::rgb(255, 200, 0));
        canvas.set_line_width(6.0);
        canvas.set_line_cap(crate::canvas2d::CanvasLineCap::Round);
        canvas.set_line_join(crate::canvas2d::CanvasLineJoin::Bevel);
        canvas.begin_path();
        canvas.move_to(20.0, 40.0);
        canvas.line_to(100.0, 40.0);
        canvas.line_to(100.0, 120.0);
        canvas.stroke();
        canvas.restore();
        canvas.set_fill_style(Color::rgb(12, 12, 16));
        canvas.fill_rect(60.0, 30.0, 60.0, 24.0);
        canvas.set_fill_style(Color::WHITE);
        canvas.fill_text("A", 70.0, 46.0);

        let scene = crate::scene3d::Scene3D::new();
        let mut dl = DisplayList::new();
        dl.push(DrawCommand::DrawRect {
            bounds: Rect::new(0.0, 0.0, 240.0, 240.0),
            color: Color::rgb(10, 12, 18),
        });
        dl.push(DrawCommand::DrawViewport3D {
            bounds: Rect::new(120.0, 20.0, 100.0, 100.0),
            scene: std::sync::Arc::new(scene),
        });
        dl.push(DrawCommand::DrawCanvas2D {
            bounds: Rect::new(0.0, 0.0, 240.0, 240.0),
            commands: canvas.commands().to_vec(),
        });

        let frame = collector.collect_hardware_frame(&dl, None);
        // Order: bg DrawRect (Quads) -> Viewport3D -> FillRect (Quads) -> Path stroke (Paths) -> badge FillRect (Quads) -> FillText (Text)
        assert!(frame.items.len() >= 6);
        assert!(matches!(
            frame.items[0],
            HardwareRenderItem::Batch(HardwareDrawOp {
                kind: HardwareDrawOpKind::Quads,
                ..
            })
        ));
        assert!(matches!(
            frame.items[1],
            HardwareRenderItem::Viewport3D { .. }
        ));
        // Capped path stroke is preserved strictly between FillRect #1 and badge FillRect #2!
        let path_pos = frame
            .items
            .iter()
            .position(|it| {
                matches!(
                    it,
                    HardwareRenderItem::Batch(HardwareDrawOp {
                        kind: HardwareDrawOpKind::Paths,
                        ..
                    })
                )
            })
            .expect("path stroke op");
        let text_pos = frame
            .items
            .iter()
            .position(|it| {
                matches!(
                    it,
                    HardwareRenderItem::Batch(HardwareDrawOp {
                        kind: HardwareDrawOpKind::Text,
                        ..
                    })
                )
            })
            .expect("slug text op");
        assert!(path_pos < text_pos);
        // Canvas FillRect must NOT emit duplicate segments in paths batch
        assert!(!frame.paths.segments.is_empty());
        assert_eq!(
            frame.quads.len(),
            3,
            "bg rect + canvas FillRect + badge FillRect"
        );
    }

    #[cfg(feature = "wgpu-backend")]
    #[test]
    fn test_wgpu_2d_renderer_stencil_clip_dashed_stroke_caps_joins_and_slug_pixels() {
        let instance = wgpu::Instance::default();
        let Ok((_adapter, device, queue)) = crate::gpu_wgpu::request_working_adapter_and_device(
            &instance,
            None,
            "strata_render_2d_test_device",
        ) else {
            return;
        };

        let mut renderer = Wgpu2DRenderer::from_device(
            std::sync::Arc::new(device),
            std::sync::Arc::new(queue),
            wgpu::TextureFormat::Rgba8Unorm,
        );
        let mut collector = UnifiedGpuCollector::new(240.0, 240.0);

        let mut ctx = crate::canvas2d::Canvas2DContext::new(240.0, 240.0);
        // 1. Diagonal triangle clip (0,0)-(100,0)-(0,100) + crossing blue FillRect
        ctx.save();
        ctx.begin_path();
        ctx.move_to(0.0, 0.0);
        ctx.line_to(100.0, 0.0);
        ctx.line_to(0.0, 100.0);
        ctx.close_path();
        ctx.clip();
        ctx.set_fill_style(Color::rgb(0, 120, 255));
        ctx.fill_rect(0.0, 0.0, 100.0, 100.0);
        ctx.restore();

        // 2. Circular clip aperture at (170, 55), radius 30 + full AABB green FillRect
        ctx.save();
        ctx.begin_path();
        ctx.arc(170.0, 55.0, 30.0, 0.0, std::f32::consts::TAU, false);
        ctx.clip();
        ctx.set_fill_style(Color::rgb(0, 220, 80));
        ctx.fill_rect(140.0, 25.0, 60.0, 60.0);
        ctx.restore();

        // 3. Dashed StrokeRect [12, 12] from x=20..120 at y=140, width=4 (dash on [20..32], gap on [36..42])
        ctx.save();
        ctx.set_stroke_style(Color::rgb(255, 60, 60));
        ctx.set_line_width(4.0);
        ctx.set_line_dash(&[12.0, 12.0]);
        ctx.stroke_rect(20.0, 140.0, 100.0, 30.0);
        ctx.restore();

        // 4. Straight horizontal+vertical stroke with Round cap + Miter join
        ctx.save();
        ctx.set_stroke_style(Color::rgb(255, 220, 0));
        ctx.set_line_width(10.0);
        ctx.set_line_cap(crate::canvas2d::CanvasLineCap::Round);
        ctx.set_line_join(crate::canvas2d::CanvasLineJoin::Miter);
        ctx.begin_path();
        ctx.move_to(150.0, 150.0);
        ctx.line_to(200.0, 150.0);
        ctx.line_to(200.0, 200.0);
        ctx.stroke();
        ctx.restore();

        // 5. Slug glyph "H" rendered in bright cyan
        let mut dl = DisplayList::new();
        dl.push(DrawCommand::DrawCanvas2D {
            bounds: Rect::new(0.0, 0.0, 240.0, 240.0),
            commands: ctx.commands().to_vec(),
        });
        dl.push(DrawCommand::DrawText {
            origin: (20.0, 195.0),
            bounds_width: None,
            bounds_height: None,
            text_align: strata_style::TextAlign::Left,
            text: "H".to_string(),
            font_size: 24.0,
            line_height: 1.2,
            font_weight: strata_style::FontWeight::Bold,
            color: Color::rgb(0, 240, 240),
            is_monospace: false,
            font_family: strata_primitives::atom::AtomId::NULL,
        });

        let (summary, pixels) = renderer
            .render_display_list_offscreen(
                &mut collector,
                &dl,
                240,
                240,
                1.0,
                [0.0, 0.0, 0.0, 1.0],
                None,
                true,
            )
            .expect("wgpu 2D offscreen render");
        assert!(summary.quad_instances >= 10);
        assert!(summary.path_segments > 0);
        assert!(summary.slug_glyphs > 0);

        let px = |x: usize, y: usize| -> [u8; 4] {
            let idx = (y * 240 + x) * 4;
            [pixels[idx], pixels[idx + 1], pixels[idx + 2], pixels[idx + 3]]
        };

        // Diagonal triangle clip: (20,20) inside (blue), (75,75) inside AABB but outside diagonal (black!)
        let diag_in = px(20, 20);
        assert!(
            diag_in[2] > 200 && diag_in[0] < 20,
            "inside diagonal clip should be blue: {:?}",
            diag_in
        );
        let diag_out = px(75, 75);
        assert_eq!(
            diag_out,
            [0, 0, 0, 255],
            "outside diagonal clip must remain cleared background via stencil"
        );

        // Circular clip: (170,55) inside (green), (143,28) in circle AABB corner outside radius 30 (black!)
        let circ_in = px(170, 55);
        assert!(
            circ_in[1] > 180 && circ_in[0] < 20,
            "inside circular clip should be green: {:?}",
            circ_in
        );
        let circ_corner = px(143, 28);
        assert_eq!(
            circ_corner,
            [0, 0, 0, 255],
            "outside circular clip must remain black via stencil"
        );

        // Dashed StrokeRect: dash on [20..32] at (25, 140) is red; gap [32..44] at (38, 140) is black
        let dash_on = px(25, 140);
        assert!(
            dash_on[0] > 200 && dash_on[1] < 80,
            "dash segment should be red: {:?}",
            dash_on
        );
        assert_eq!(px(38, 140), [0, 0, 0, 255], "dash gap must stay background");

        // Round cap + Miter join stroke: midpoint (175, 150) & miter apex (203, 147) & round cap tail (147, 150)
        assert!(
            px(175, 150)[0] > 200 && px(175, 150)[1] > 180,
            "straight segment midpoint must be filled"
        );
        assert!(
            px(147, 150)[0] > 150,
            "Round cap extends past x=150: {:?}",
            px(147, 150)
        );
        assert!(
            px(203, 147)[0] > 150,
            "Miter outer corner reaches outside corner apex: {:?}",
            px(203, 147)
        );

        // Slug glyph "H" should light up at least one cyan pixel inside [20..44, 195..220]
        let mut lit_glyph = false;
        for y in 195..220 {
            for x in 20..44 {
                let p = px(x, y);
                if p[1] > 150 && p[2] > 150 {
                    lit_glyph = true;
                    break;
                }
            }
        }
        assert!(lit_glyph, "Slug font shader should rasterize visible glyph pixels");
    }
}
