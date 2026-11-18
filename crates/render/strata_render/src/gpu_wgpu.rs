//! Hardware-accelerated `wgpu` 3D renderer with double-single RTE precision and
//! single-draw-call programmable vertex pulling for planetary/terrain tile pyramids.

use crate::scene3d::{
    Background3D, Camera3D, Geometry3D, Light3D, Material3D, Mesh3D, RetainedCameraUniforms,
    RetainedDynamicMesh, RetainedEnvironmentAssets, RetainedStar, RetainedTerrainFrame,
    RetainedTileMesh, RetainedTileTexture, RteTileVertex, Scene3D, TileKeyId,
};
use bytemuck::{Pod, Zeroable};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

/// Default maximum new tile textures uploaded to the GPU atlas per frame during continuous flight.
pub const DEFAULT_TEXTURE_UPLOAD_BUDGET: usize = 1024;
/// Default maximum new tile meshes copied to the GPU storage slab per frame.
pub const DEFAULT_MESH_UPLOAD_BUDGET: usize = 512;
/// Standard terrain imagery tile dimension in the GPU `Texture2DArray` atlas.
pub const ATLAS_TILE_SIZE: u32 = 256;
/// Maximum imagery layers pre-allocated in the GPU `Texture2DArray` atlas.
pub const MAX_ATLAS_LAYERS: u32 = 1024;

/// Splits a 64-bit float into `(hi, lo)` 32-bit floats for GPU double-single emulation.
#[inline]
pub fn split_f64_to_f32(v: f64) -> (f32, f32) {
    let hi = v as f32;
    let lo = (v - hi as f64) as f32;
    (hi, lo)
}

fn transpose_mat4(m: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    [
        [m[0][0], m[1][0], m[2][0], m[3][0]],
        [m[0][1], m[1][1], m[2][1], m[3][1]],
        [m[0][2], m[1][2], m[2][2], m[3][2]],
        [m[0][3], m[1][3], m[2][3], m[3][3]],
    ]
}

fn invert_mat4(m: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let a = [
        m[0][0] as f64,
        m[0][1] as f64,
        m[0][2] as f64,
        m[0][3] as f64,
        m[1][0] as f64,
        m[1][1] as f64,
        m[1][2] as f64,
        m[1][3] as f64,
        m[2][0] as f64,
        m[2][1] as f64,
        m[2][2] as f64,
        m[2][3] as f64,
        m[3][0] as f64,
        m[3][1] as f64,
        m[3][2] as f64,
        m[3][3] as f64,
    ];
    let mut inv = [0.0f64; 16];
    inv[0] = a[5] * a[10] * a[15] - a[5] * a[11] * a[14] - a[9] * a[6] * a[15]
        + a[9] * a[7] * a[14]
        + a[13] * a[6] * a[11]
        - a[13] * a[7] * a[10];
    inv[4] = -a[4] * a[10] * a[15] + a[4] * a[11] * a[14] + a[8] * a[6] * a[15]
        - a[8] * a[7] * a[14]
        - a[12] * a[6] * a[11]
        + a[12] * a[7] * a[10];
    inv[8] = a[4] * a[9] * a[15] - a[4] * a[11] * a[13] - a[8] * a[5] * a[15]
        + a[8] * a[7] * a[13]
        + a[12] * a[5] * a[11]
        - a[12] * a[7] * a[9];
    inv[12] = -a[4] * a[9] * a[14] + a[4] * a[10] * a[13] + a[8] * a[5] * a[14]
        - a[8] * a[6] * a[13]
        - a[12] * a[5] * a[10]
        + a[12] * a[6] * a[9];
    inv[1] = -a[1] * a[10] * a[15] + a[1] * a[11] * a[14] + a[9] * a[2] * a[15]
        - a[9] * a[3] * a[14]
        - a[13] * a[2] * a[11]
        + a[13] * a[3] * a[10];
    inv[5] = a[0] * a[10] * a[15] - a[0] * a[11] * a[14] - a[8] * a[2] * a[15]
        + a[8] * a[3] * a[14]
        + a[12] * a[2] * a[11]
        - a[12] * a[3] * a[10];
    inv[9] = -a[0] * a[9] * a[15] + a[0] * a[11] * a[13] + a[8] * a[1] * a[15]
        - a[8] * a[3] * a[13]
        - a[12] * a[1] * a[11]
        + a[12] * a[3] * a[9];
    inv[13] = a[0] * a[9] * a[14] - a[0] * a[10] * a[13] - a[8] * a[1] * a[14]
        + a[8] * a[2] * a[13]
        + a[12] * a[1] * a[10]
        - a[12] * a[2] * a[9];
    inv[2] = a[1] * a[6] * a[15] - a[1] * a[7] * a[14] - a[5] * a[2] * a[15]
        + a[5] * a[3] * a[14]
        + a[13] * a[2] * a[7]
        - a[13] * a[3] * a[6];
    inv[6] = -a[0] * a[6] * a[15] + a[0] * a[7] * a[14] + a[4] * a[2] * a[15]
        - a[4] * a[3] * a[14]
        - a[12] * a[2] * a[7]
        + a[12] * a[3] * a[6];
    inv[10] = a[0] * a[5] * a[15] - a[0] * a[7] * a[13] - a[4] * a[1] * a[15]
        + a[4] * a[3] * a[13]
        + a[12] * a[1] * a[7]
        - a[12] * a[3] * a[5];
    inv[14] = -a[0] * a[5] * a[14] + a[0] * a[6] * a[13] + a[4] * a[1] * a[14]
        - a[4] * a[2] * a[13]
        - a[12] * a[1] * a[6]
        + a[12] * a[2] * a[5];
    inv[3] = -a[1] * a[6] * a[11] + a[1] * a[7] * a[10] + a[5] * a[2] * a[11]
        - a[5] * a[3] * a[10]
        - a[9] * a[2] * a[7]
        + a[9] * a[3] * a[6];
    inv[7] = a[0] * a[6] * a[11] - a[0] * a[7] * a[10] - a[4] * a[2] * a[11]
        + a[4] * a[3] * a[10]
        + a[8] * a[2] * a[7]
        - a[8] * a[3] * a[6];
    inv[11] = -a[0] * a[5] * a[11] + a[0] * a[7] * a[9] + a[4] * a[1] * a[11]
        - a[4] * a[3] * a[9]
        - a[8] * a[1] * a[7]
        + a[8] * a[3] * a[5];
    inv[15] = a[0] * a[5] * a[10] - a[0] * a[6] * a[9] - a[4] * a[1] * a[10]
        + a[4] * a[2] * a[9]
        + a[8] * a[1] * a[6]
        - a[8] * a[2] * a[5];

    let det = a[0] * inv[0] + a[1] * inv[4] + a[2] * inv[8] + a[3] * inv[12];
    if det.abs() < 1e-24 {
        return [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
    }
    let inv_det = 1.0 / det;
    let mut out = [[0.0f32; 4]; 4];
    for c in 0..4 {
        for r in 0..4 {
            out[c][r] = (inv[c * 4 + r] * inv_det) as f32;
        }
    }
    out
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuCameraUniforms {
    view_proj_rte: [[f32; 4]; 4],
    inv_view_proj_rte: [[f32; 4]; 4],
    ecef_from_icrf: [[f32; 4]; 4],
    icrf_from_ecef: [[f32; 4]; 4],
    eye_high: [f32; 4],
    eye_low: [f32; 4],
    sun_dir: [f32; 4],       // xyz = sun dir, w = sun_visible (0/1)
    sun_color: [f32; 4],     // rgb = color, w = sun_angular_radius_rad
    ambient_color: [f32; 4], // rgb = ambient, w = stars_intensity (0 when off)
    sky_top: [f32; 4],
    sky_bottom: [f32; 4],
    moon_dir: [f32; 4],     // xyz = moon dir, w = moon_visible (0/1)
    env_params: [f32; 4], // x = day_night_strength, y = city_lights_intensity, z = local_25d_blend, w = moon_angular_radius_rad
    cloud_params: [f32; 4], // x = cloud_layer_opacity, y = cloud_lon_offset_rad, z = time_s, w = space_sky_enabled (0/1)
    viewport_wh: [f32; 4],  // x = w, y = h, z = 0, w = 0
}

impl GpuCameraUniforms {
    fn from_retained(cam: &RetainedCameraUniforms, viewport_w: f32, viewport_h: f32) -> Self {
        let (ex_hi, ex_lo) = split_f64_to_f32(cam.eye_ecef[0]);
        let (ey_hi, ey_lo) = split_f64_to_f32(cam.eye_ecef[1]);
        let (ez_hi, ez_lo) = split_f64_to_f32(cam.eye_ecef[2]);
        let (sky_top, sky_bottom) = match cam.background {
            Background3D::Transparent => ([0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 0.0]),
            Background3D::Solid(c) => {
                let rgba = [c.r(), c.g(), c.b(), c.a()];
                (rgba, rgba)
            }
            Background3D::VerticalGradient { top, bottom } => (
                [top.r(), top.g(), top.b(), top.a()],
                [bottom.r(), bottom.g(), bottom.b(), bottom.a()],
            ),
        };
        let inv_vp = invert_mat4(cam.view_proj_rte);
        let icrf_from_ecef = transpose_mat4(cam.ecef_from_icrf);
        let space_sky = if cam.stars_visible || cam.sun_visible || cam.moon_visible {
            1.0
        } else {
            0.0
        };
        let stars_eff = if cam.stars_visible {
            cam.stars_intensity.max(0.0)
        } else {
            0.0
        };
        Self {
            view_proj_rte: cam.view_proj_rte,
            inv_view_proj_rte: inv_vp,
            ecef_from_icrf: cam.ecef_from_icrf,
            icrf_from_ecef,
            eye_high: [ex_hi, ey_hi, ez_hi, 0.0],
            eye_low: [ex_lo, ey_lo, ez_lo, 0.0],
            sun_dir: [
                cam.sun_dir[0],
                cam.sun_dir[1],
                cam.sun_dir[2],
                if cam.sun_visible { 1.0 } else { 0.0 },
            ],
            sun_color: [
                cam.sun_color[0],
                cam.sun_color[1],
                cam.sun_color[2],
                cam.sun_angular_radius_rad,
            ],
            ambient_color: [
                cam.ambient_color[0],
                cam.ambient_color[1],
                cam.ambient_color[2],
                stars_eff,
            ],
            sky_top,
            sky_bottom,
            moon_dir: [
                cam.moon_dir[0],
                cam.moon_dir[1],
                cam.moon_dir[2],
                if cam.moon_visible { 1.0 } else { 0.0 },
            ],
            env_params: [
                cam.day_night_strength,
                cam.city_lights_intensity,
                cam.local_25d_blend,
                cam.moon_angular_radius_rad,
            ],
            cloud_params: [
                cam.cloud_layer_opacity,
                cam.cloud_lon_offset_rad,
                cam.time_s,
                space_sky,
            ],
            viewport_wh: [
                viewport_w.max(1.0),
                viewport_h.max(1.0),
                cam.skybox_intensity.max(0.0),
                cam.local_cloud_opacity.max(0.0),
            ],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuStar {
    icrf_dir_mag: [f32; 4], // xyz = J2000 ICRF unit dir, w = vmag
    color: [f32; 4],        // rgba
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuTileInstance {
    center_high: [f32; 4],
    center_low: [f32; 4],
    uv_rect: [f32; 4],
    night_uv_rect: [f32; 4],
    tint: [f32; 4],
    tile_meta: [u32; 4], // x = base_vertex, y = atlas_layer, z = unlit (0/1), w = night_atlas_layer
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuDynamicUniforms {
    anchor_high: [f32; 4],
    anchor_low: [f32; 4],
    color: [f32; 4],
    dyn_meta: [u32; 4], // x = unlit, y = use_vertex_color, z = use_texture, w = atlas_layer
}

/// Builds a shared triangle index buffer for a terrain grid `(grid_res x grid_res)` with optional skirts.
pub fn build_terrain_grid_indices(grid_res: u32, has_skirts: bool) -> Vec<u32> {
    let n = grid_res.max(2);
    let cells = n - 1;
    let skirt_quads = if has_skirts { cells * 4 } else { 0 };
    let mut indices = Vec::with_capacity(((cells * cells + skirt_quads) * 6) as usize);

    for r in 0..cells {
        for c in 0..cells {
            let i00 = r * n + c;
            let i10 = i00 + 1;
            let i01 = (r + 1) * n + c;
            let i11 = i01 + 1;
            indices.extend_from_slice(&[i00, i10, i11, i00, i11, i01]);
        }
    }

    if has_skirts {
        let base = n * n;
        // South edge (row = 0, facing -North): s1 -> s0 -> k0 -> k1
        for c in 0..cells {
            let s0 = c;
            let s1 = c + 1;
            let k0 = base + c;
            let k1 = base + c + 1;
            indices.extend_from_slice(&[s1, s0, k0, s1, k0, k1]);
        }
        // East edge (col = n-1, facing +East): s1 -> s0 -> k0 -> k1
        let r_base = base + n;
        for r in 0..cells {
            let s0 = r * n + (n - 1);
            let s1 = (r + 1) * n + (n - 1);
            let k0 = r_base + r;
            let k1 = r_base + r + 1;
            indices.extend_from_slice(&[s1, s0, k0, s1, k0, k1]);
        }
        // North edge (row = n-1, facing +North): s0 -> s1 -> k1 -> k0
        let b_base = base + n * 2;
        for c in 0..cells {
            let s0 = (n - 1) * n + c;
            let s1 = s0 + 1;
            let k0 = b_base + c;
            let k1 = b_base + c + 1;
            indices.extend_from_slice(&[s0, s1, k1, s0, k1, k0]);
        }
        // West edge (col = 0, facing -East): s0 -> s1 -> k1 -> k0
        let l_base = base + n * 3;
        for r in 0..cells {
            let s0 = r * n;
            let s1 = (r + 1) * n;
            let k0 = l_base + r;
            let k1 = l_base + r + 1;
            indices.extend_from_slice(&[s0, s1, k1, s0, k1, k0]);
        }
    }

    indices
}

const SKY_WGSL: &str = r#"
struct GpuCameraUniforms {
    view_proj_rte: mat4x4<f32>,
    inv_view_proj_rte: mat4x4<f32>,
    ecef_from_icrf: mat4x4<f32>,
    icrf_from_ecef: mat4x4<f32>,
    eye_high: vec4<f32>,
    eye_low: vec4<f32>,
    sun_dir: vec4<f32>,
    sun_color: vec4<f32>,
    ambient_color: vec4<f32>,
    sky_top: vec4<f32>,
    sky_bottom: vec4<f32>,
    moon_dir: vec4<f32>,
    env_params: vec4<f32>,
    cloud_params: vec4<f32>,
    viewport_wh: vec4<f32>,
}

@group(0) @binding(0) var<uniform> cam: GpuCameraUniforms;
@group(0) @binding(1) var moon_tex: texture_2d<f32>;
@group(0) @binding(2) var env_sampler: sampler;
@group(0) @binding(3) var skybox_cube: texture_cube<f32>;

struct SkyOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv_y: f32,
    @location(1) ndc: vec2<f32>,
}

@vertex
fn vs_sky(@builtin(vertex_index) vid: u32) -> SkyOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -3.0),
        vec2<f32>(3.0, 1.0),
        vec2<f32>(-1.0, 1.0),
    );
    let p = positions[vid];
    var out: SkyOut;
    out.pos = vec4<f32>(p, 0.9999, 1.0);
    out.uv_y = clamp((1.0 - p.y) * 0.5, 0.0, 1.0);
    out.ndc = p;
    return out;
}

fn hash31(p: vec3<f32>) -> f32 {
    let q = fract(p * 0.1031);
    let r = q + vec3<f32>(dot(q, q.yzx + vec3<f32>(33.33)));
    return fract((r.x + r.y) * r.z);
}

fn noise3(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (vec3<f32>(3.0) - 2.0 * f);
    let a = hash31(i);
    let b = hash31(i + vec3<f32>(1.0, 0.0, 0.0));
    let c = hash31(i + vec3<f32>(0.0, 1.0, 0.0));
    let d = hash31(i + vec3<f32>(1.0, 1.0, 0.0));
    let e = hash31(i + vec3<f32>(0.0, 0.0, 1.0));
    let f1 = hash31(i + vec3<f32>(1.0, 0.0, 1.0));
    let g = hash31(i + vec3<f32>(0.0, 1.0, 1.0));
    let h = hash31(i + vec3<f32>(1.0, 1.0, 1.0));
    return mix(
        mix(mix(a, b, u.x), mix(c, d, u.x), u.y),
        mix(mix(e, f1, u.x), mix(g, h, u.x), u.y),
        u.z,
    );
}

@fragment
fn fs_sky(in: SkyOut) -> @location(0) vec4<f32> {
    let sky_grad = mix(cam.sky_top, cam.sky_bottom, clamp(in.uv_y, 0.0, 1.0));
    let p4 = cam.inv_view_proj_rte * vec4<f32>(in.ndc, 0.5, 1.0);
    let w_safe = select(p4.w, select(-1e-6, 1e-6, p4.w >= 0.0), abs(p4.w) < 1e-6);
    let rd = normalize(p4.xyz / w_safe);
    let rd_icrf = normalize((cam.icrf_from_ecef * vec4<f32>(rd, 0.0)).xyz);
    let cube_dim = max(f32(textureDimensions(skybox_cube, 0).x), 1.0);
    let texel_px = (cube_dim * 0.5) * length(fwidth(rd_icrf));
    let sky_lod = max(log2(max(texel_px, 1.0)), 0.0);

    if (cam.cloud_params.w < 0.5) {
        return sky_grad;
    }

    let space_t = 1.0 - smoothstep(0.0, 0.85, cam.env_params.z);

    // 1. Pitch-black deep space + J2000 ICRF telescope skybox cubemap (or procedural Milky Way fallback)
    var col = vec3<f32>(0.0);
    if (cam.ambient_color.w > 0.001 && space_t > 0.001) {
        let cube_s = textureSampleLevel(skybox_cube, env_sampler, rd_icrf, sky_lod);
        let sky_gain = cam.viewport_wh.z;
        if (cube_s.a > 0.01 && sky_gain > 0.001) {
            col += cube_s.rgb * sky_gain * space_t;
        } else {
            // Galactic North Pole & Galactic Center unit vectors (J2000)
            let gnp = vec3<f32>(-0.867666, -0.198076, 0.455984);
            let gc = vec3<f32>(-0.054876, -0.873437, -0.483835);
            let sin_b = dot(rd_icrf, gnp);
            let core_w = 0.35 + 0.65 * max(dot(rd_icrf, gc), 0.0);
            let dust_n = noise3(rd_icrf * 7.5) * 0.65 + noise3(rd_icrf * 17.0) * 0.35;
            let rift = smoothstep(0.015, 0.14, abs(sin_b) + (dust_n - 0.5) * 0.09);
            let mw = exp(-pow(abs(sin_b) * 8.0, 1.5)) * core_w * (0.45 + 0.55 * dust_n) * mix(0.45, 1.0, rift);
            col += vec3<f32>(0.055, 0.072, 0.125) * mw * min(cam.ambient_color.w, 1.8) * space_t;
        }
    }

    // Blend pitch-black space to daytime sky when morphing down into 2.5D
    col = mix(sky_grad.rgb, col, space_t);

    // 2. Realistic Sun (photosphere limb darkening + solar corona + halo)
    let sun_dir = normalize(cam.sun_dir.xyz);
    if (cam.sun_dir.w > 0.5) {
        let sun_cos = clamp(dot(rd, sun_dir), -1.0, 1.0);
        let sun_ang = acos(sun_cos);
        let r_sun = max(cam.sun_color.w, 0.004);
        if (sun_ang < r_sun * 1.08) {
            let mu = sqrt(max(1.0 - (sun_ang / r_sun) * (sun_ang / r_sun), 0.0));
            let aa = 1.0 - smoothstep(r_sun * 0.94, r_sun * 1.06, sun_ang);
            let limb = mix(0.48, 1.0, pow(mu, 0.45));
            col = mix(col, cam.sun_color.rgb * (3.6 * limb), aa);
        }
        let corona = exp(-sun_ang / (r_sun * 2.4)) * 0.85 + exp(-sun_ang / (r_sun * 12.0)) * 0.13;
        col += cam.sun_color.rgb * vec3<f32>(1.0, 0.90, 0.72) * corona * mix(0.45, 1.0, space_t);
    }

    // 3. Realistic Moon (NASA LRO albedo + Lommel-Seeliger phase from Sun vector + Earthshine)
    if (cam.moon_dir.w > 0.5) {
        let m_dir = normalize(cam.moon_dir.xyz);
        let b_cos = dot(rd, m_dir);
        if (b_cos > 0.0) {
            let r_moon = max(cam.env_params.w, 0.004);
            let sin_r = sin(r_moon);
            let r2 = sin_r * sin_r;
            let d2 = max(1.0 - b_cos * b_cos, 0.0);
            if (d2 < r2) {
                let z_m = sqrt(max(r2 - d2, 0.0));
                let n_perp = rd - m_dir * b_cos;
                let n_moon = normalize(n_perp / sin_r - m_dir * (z_m / sin_r));

                let up_icrf = normalize((cam.ecef_from_icrf * vec4<f32>(0.0, 0.0, 1.0, 0.0)).xyz);
                let ref_up = select(vec3<f32>(0.0, 1.0, 0.0), up_icrf, abs(dot(up_icrf, m_dir)) < 0.98);
                let m_right = normalize(cross(m_dir, ref_up));
                let m_up = normalize(cross(m_right, m_dir));
                let lx = dot(n_moon, m_right);
                let ly = dot(n_moon, m_up);
                let lz = dot(n_moon, -m_dir);
                let u = atan2(lx, lz) / 6.28318530718 + 0.5;
                let v = clamp(0.5 - asin(clamp(ly, -1.0, 1.0)) / 3.14159265359, 0.001, 0.999);
                let albedo = textureSampleLevel(moon_tex, env_sampler, vec2<f32>(u, v), 0.0).rgb;

                let ndotl_m = max(dot(n_moon, sun_dir), 0.0);
                let ndotv_m = max(dot(n_moon, -rd), 0.02);
                let lommel = ndotl_m / (ndotl_m + 0.36 * ndotv_m + 0.06);
                let earthshine = vec3<f32>(0.024, 0.032, 0.048);
                let lit_moon = albedo * (cam.sun_color.rgb * lommel * 1.45 + earthshine);
                let aa_m = 1.0 - smoothstep(r2 * 0.90, r2, d2);
                col = mix(col, lit_moon, aa_m * mix(0.75, 1.0, space_t));
            }
        }
    }

    return vec4<f32>(col, 1.0);
}
"#;

const STARS_WGSL: &str = r#"
struct GpuCameraUniforms {
    view_proj_rte: mat4x4<f32>,
    inv_view_proj_rte: mat4x4<f32>,
    ecef_from_icrf: mat4x4<f32>,
    icrf_from_ecef: mat4x4<f32>,
    eye_high: vec4<f32>,
    eye_low: vec4<f32>,
    sun_dir: vec4<f32>,
    sun_color: vec4<f32>,
    ambient_color: vec4<f32>,
    sky_top: vec4<f32>,
    sky_bottom: vec4<f32>,
    moon_dir: vec4<f32>,
    env_params: vec4<f32>,
    cloud_params: vec4<f32>,
    viewport_wh: vec4<f32>,
}

struct GpuStar {
    icrf_dir_mag: vec4<f32>,
    color: vec4<f32>,
}

@group(0) @binding(0) var<uniform> cam: GpuCameraUniforms;
@group(0) @binding(1) var<storage, read> stars: array<GpuStar>;

struct StarVsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) local_uv: vec2<f32>,
    @location(1) color_flux: vec4<f32>,
}

@vertex
fn vs_star(
    @builtin(vertex_index) vid: u32,
    @builtin(instance_index) inst_id: u32,
) -> StarVsOut {
    var offsets = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(1.0, -1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, 1.0),
    );
    let q = offsets[vid];
    let s = stars[inst_id];
    let dir_ecef = normalize((cam.ecef_from_icrf * vec4<f32>(s.icrf_dir_mag.xyz, 0.0)).xyz);
    var clip = cam.view_proj_rte * vec4<f32>(dir_ecef, 0.0);

    var out: StarVsOut;
    out.local_uv = q;
    let space_t = 1.0 - smoothstep(0.0, 0.80, cam.env_params.z);
    if (clip.w <= 0.001 || space_t <= 0.005 || cam.ambient_color.w <= 0.001) {
        out.clip_pos = vec4<f32>(2.0, 2.0, 2.0, 1.0);
        out.color_flux = vec4<f32>(0.0);
        return out;
    }

    let vmag = s.icrf_dir_mag.w;
    let flux = clamp(pow(10.0, -0.32 * (vmag - 2.6)), 0.14, 3.2) * cam.ambient_color.w * space_t;
    let size_px = clamp(1.55 + (5.0 - vmag) * 0.55, 1.4, 4.6);
    clip.z = clip.w * 0.99985;
    let px_ndc = (q * size_px / max(cam.viewport_wh.xy, vec2<f32>(1.0, 1.0))) * 2.0;
    clip.x += px_ndc.x * clip.w;
    clip.y += px_ndc.y * clip.w;

    out.clip_pos = clip;
    out.color_flux = vec4<f32>(s.color.rgb, flux);
    return out;
}

@fragment
fn fs_star(in: StarVsOut) -> @location(0) vec4<f32> {
    let r2 = dot(in.local_uv, in.local_uv);
    if (r2 > 1.0) {
        discard;
    }
    let airy = exp(-r2 * 4.8);
    return vec4<f32>(in.color_flux.rgb * (in.color_flux.a * airy), airy);
}
"#;

const TERRAIN_PULL_WGSL: &str = r#"
struct GpuCameraUniforms {
    view_proj_rte: mat4x4<f32>,
    inv_view_proj_rte: mat4x4<f32>,
    ecef_from_icrf: mat4x4<f32>,
    icrf_from_ecef: mat4x4<f32>,
    eye_high: vec4<f32>,
    eye_low: vec4<f32>,
    sun_dir: vec4<f32>,
    sun_color: vec4<f32>,
    ambient_color: vec4<f32>,
    sky_top: vec4<f32>,
    sky_bottom: vec4<f32>,
    moon_dir: vec4<f32>,
    env_params: vec4<f32>,
    cloud_params: vec4<f32>,
    viewport_wh: vec4<f32>,
}

struct GpuTileVertex {
    pos_u: vec4<f32>,
    norm_v: vec4<f32>,
}

struct GpuTileInstance {
    center_high: vec4<f32>,
    center_low: vec4<f32>,
    uv_rect: vec4<f32>,
    night_uv_rect: vec4<f32>,
    tint: vec4<f32>,
    tile_meta: vec4<u32>,
}

@group(0) @binding(0) var<uniform> cam: GpuCameraUniforms;
@group(0) @binding(1) var<storage, read> vertices: array<GpuTileVertex>;
@group(0) @binding(2) var<storage, read> instances: array<GpuTileInstance>;
@group(0) @binding(3) var terrain_atlas: texture_2d_array<f32>;
@group(0) @binding(4) var terrain_sampler: sampler;
@group(0) @binding(5) var land_mask_tex: texture_2d<f32>;

struct TerrainVsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) tint: vec4<f32>,
    @location(3) @interpolate(flat) tex_layer: u32,
    @location(4) @interpolate(flat) unlit: u32,
    @location(5) ecef_pos: vec3<f32>,
    @location(6) view_rel: vec3<f32>,
    @location(7) night_uv: vec2<f32>,
    @location(8) @interpolate(flat) night_layer: u32,
}

@vertex
fn vs_terrain(
    @builtin(vertex_index) local_v: u32,
    @builtin(instance_index) inst_id: u32,
) -> TerrainVsOut {
    let inst = instances[inst_id];
    let v = vertices[inst.tile_meta.x + local_v];
    let center_rel = (inst.center_high.xyz - cam.eye_high.xyz)
                   + (inst.center_low.xyz - cam.eye_low.xyz);
    let world_rel = center_rel + v.pos_u.xyz;
    let raw_uv = vec2<f32>(v.pos_u.w, v.norm_v.w);

    var out: TerrainVsOut;
    out.clip_pos = cam.view_proj_rte * vec4<f32>(world_rel, 1.0);
    out.normal = normalize(v.norm_v.xyz);
    out.uv = mix(inst.uv_rect.xy, inst.uv_rect.zw, raw_uv);
    out.tint = inst.tint;
    out.tex_layer = inst.tile_meta.y;
    out.unlit = inst.tile_meta.z;
    out.ecef_pos = (inst.center_high.xyz + inst.center_low.xyz) + v.pos_u.xyz;
    out.view_rel = world_rel;
    out.night_uv = mix(inst.night_uv_rect.xy, inst.night_uv_rect.zw, raw_uv);
    out.night_layer = inst.tile_meta.w;
    return out;
}

fn hash31(p: vec3<f32>) -> f32 {
    var q = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    q += dot(q, q.yxz + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn vnoise3(x: vec3<f32>) -> f32 {
    let i = floor(x);
    let f = fract(x);
    let u = f * f * (3.0 - 2.0 * f);
    let n000 = hash31(i + vec3<f32>(0.0, 0.0, 0.0));
    let n100 = hash31(i + vec3<f32>(1.0, 0.0, 0.0));
    let n010 = hash31(i + vec3<f32>(0.0, 1.0, 0.0));
    let n110 = hash31(i + vec3<f32>(1.0, 1.0, 0.0));
    let n001 = hash31(i + vec3<f32>(0.0, 0.0, 1.0));
    let n101 = hash31(i + vec3<f32>(1.0, 0.0, 1.0));
    let n011 = hash31(i + vec3<f32>(0.0, 1.0, 1.0));
    let n111 = hash31(i + vec3<f32>(1.0, 1.0, 1.0));
    let nx00 = mix(n000, n100, u.x);
    let nx10 = mix(n010, n110, u.x);
    let nx01 = mix(n001, n101, u.x);
    let nx11 = mix(n011, n111, u.x);
    let nxy0 = mix(nx00, nx10, u.y);
    let nxy1 = mix(nx01, nx11, u.y);
    return mix(nxy0, nxy1, u.z);
}

@fragment
fn fs_terrain(in: TerrainVsOut) -> @location(0) vec4<f32> {
    let sampled = textureSample(terrain_atlas, terrain_sampler, in.uv, i32(in.tex_layer));
    let base = sampled * in.tint;
    if (base.a <= 0.001) {
        discard;
    }
    if (in.unlit != 0u) {
        return base;
    }

    let n_mesh = normalize(in.normal);
    let sun_dir = normalize(cam.sun_dir.xyz);
    let view_dir = normalize(-in.view_rel);

    let dn_strength = clamp(cam.env_params.x, 0.0, 1.0);
    let city_strength = max(cam.env_params.y, 0.0);
    let blend_25d = clamp(cam.env_params.z, 0.0, 1.0);

    // Unified shadowless daylight lighting (used when sun lighting is toggled off OR in 2.5D near ground)
    let flat_lit = vec3<f32>(0.98);

    // Per-pixel continuous planetary normal `d` ensures day/night terminator, twilight,
    // and atmosphere rim are 100% smooth and unified across tile boundaries and LOD levels.
    let d = normalize(in.ecef_pos);
    let ndotl = dot(d, sun_dir);
    let slope_shade = clamp(1.0 + (dot(n_mesh, sun_dir) - ndotl) * 0.35 * blend_25d, 0.82, 1.18);

    let px_m = max(length(fwidth(d)) * 6378137.0, 6.0);
    let lon = atan2(d.y, d.x);
    let sin_lat = clamp(d.z, -1.0, 1.0);
    let u = fract(lon / 6.28318530718 + 0.5);
    let v = clamp(0.5 - asin(sin_lat) / 3.14159265359, 0.001, 0.999);
    let env_mask = textureSampleLevel(land_mask_tex, terrain_sampler, vec2<f32>(u, v), 0.0);
    let on_land = smoothstep(0.18, 0.55, env_mask.r);

    // Physical orbital solar terminator + gentle Google Earth moonlight over continents & oceans
    let terminator = smoothstep(-0.14, 0.16, ndotl);
    let twilight = exp(-pow(ndotl * 7.5, 2.0)) * vec3<f32>(0.42, 0.18, 0.06);
    let fresnel = pow(1.0 - max(dot(d, view_dir), 0.0), 3.2);
    let atmo_rim = vec3<f32>(0.28, 0.56, 1.0) * fresnel * smoothstep(-0.18, 0.32, ndotl) * 0.45 * (1.0 - blend_25d);
    let ocean_night = vec3<f32>(0.048, 0.068, 0.108);
    let land_night = vec3<f32>(0.095, 0.120, 0.174);
    let night_fill = mix(ocean_night, land_night, on_land);
    let day_lit = (night_fill + cam.sun_color.rgb * terminator + twilight) * slope_shade;

    let lit = mix(flat_lit, day_lit, dn_strength);
    var rgb = base.rgb * lit + atmo_rim * dn_strength;

    // Multi-resolution NASA Black Marble (streamed z=5..8 pyramid + 4K base) in unified linear space
    if (city_strength > 0.001 && dn_strength > 0.001 && on_land > 0.001) {
        let night_mask = smoothstep(0.06, -0.14, ndotl) * dn_strength * city_strength * on_land;
        if (night_mask > 0.001) {
            // `land_mask_tex` is Rgba8Unorm (sRGB-encoded bytes), whereas `terrain_atlas` is Rgba8UnormSrgb
            // (linear-decoded). Convert both to the same linear scale and strip NASA's non-city desert floor.
            let base_lin = max(pow(max(env_mask.g, 0.0), 2.2) - 0.022, 0.0) / 0.978;
            var black_marble = base_lin;
            let detail_night_w = 1.0 - smoothstep(350.0, 1100.0, px_m);
            if (in.night_layer != 0u && detail_night_w > 0.005) {
                let duv = 1.0 / 256.0;
                let nl = i32(in.night_layer);
                let luma_w = vec3<f32>(0.299, 0.587, 0.114);
                let c0 = dot(textureSampleLevel(terrain_atlas, terrain_sampler, in.night_uv, nl, 0.0).rgb, luma_w);
                let cx0 = dot(textureSampleLevel(terrain_atlas, terrain_sampler, clamp(in.night_uv - vec2<f32>(duv, 0.0), vec2<f32>(0.0), vec2<f32>(1.0)), nl, 0.0).rgb, luma_w);
                let cx1 = dot(textureSampleLevel(terrain_atlas, terrain_sampler, clamp(in.night_uv + vec2<f32>(duv, 0.0), vec2<f32>(0.0), vec2<f32>(1.0)), nl, 0.0).rgb, luma_w);
                let cy0 = dot(textureSampleLevel(terrain_atlas, terrain_sampler, clamp(in.night_uv - vec2<f32>(0.0, duv), vec2<f32>(0.0), vec2<f32>(1.0)), nl, 0.0).rgb, luma_w);
                let cy1 = dot(textureSampleLevel(terrain_atlas, terrain_sampler, clamp(in.night_uv + vec2<f32>(0.0, duv), vec2<f32>(0.0), vec2<f32>(1.0)), nl, 0.0).rgb, luma_w);
                let raw_tile = (c0 * 2.0 + cx0 + cx1 + cy0 + cy1) * (1.0 / 6.0);
                let tile_lin = max(raw_tile - 0.022, 0.0) / 0.978;
                black_marble = mix(base_lin, tile_lin, detail_night_w);
            }
            let bm_srgb = pow(clamp(black_marble, 0.0, 1.0), 0.45454545);
            let city_core = smoothstep(0.08, 0.48, bm_srgb);
            let corridor = smoothstep(0.02, 0.22, bm_srgb) * 0.22;
            let n_block = vnoise3(d * 2800.0);
            let n_street = vnoise3(d * 9200.0);
            let n_bulb = vnoise3(d * 26000.0);
            let fine_w = 1.0 - smoothstep(160.0, 1400.0, px_m);
            let street_w = 1.0 - smoothstep(35.0, 320.0, px_m);
            var sub_grid = 1.0;
            if (fine_w > 0.01) {
                let arterial = smoothstep(0.32, 0.88, vnoise3(d * 68000.0));
                let lamps = smoothstep(0.45, 0.94, vnoise3(d * 195000.0));
                let fine_mask = mix(0.28 + 0.95 * arterial, 0.14 + 1.35 * arterial * (0.45 + 0.55 * lamps), street_w);
                sub_grid = mix(1.0, fine_mask, fine_w);
            }
            let street_grid = smoothstep(0.24, 0.86, n_block * 0.55 + n_street * 0.45);
            let pinpoint = mix(0.62, 1.38, street_grid * (0.55 + 0.45 * n_bulb)) * sub_grid;
            let city_luma = (city_core * pinpoint + corridor * (0.65 + 0.35 * n_block) * sub_grid) * night_mask;
            let warm_amber = vec3<f32>(0.97, 0.74, 0.38);
            let warm_white = vec3<f32>(1.0, 0.90, 0.68);
            rgb += mix(warm_amber, warm_white, city_core) * city_luma * 0.92;
        }
    }

    return vec4<f32>(rgb, base.a);
}
"#;

const CLOUD_SHELL_WGSL: &str = r#"
struct GpuCameraUniforms {
    view_proj_rte: mat4x4<f32>,
    inv_view_proj_rte: mat4x4<f32>,
    ecef_from_icrf: mat4x4<f32>,
    icrf_from_ecef: mat4x4<f32>,
    eye_high: vec4<f32>,
    eye_low: vec4<f32>,
    sun_dir: vec4<f32>,
    sun_color: vec4<f32>,
    ambient_color: vec4<f32>,
    sky_top: vec4<f32>,
    sky_bottom: vec4<f32>,
    moon_dir: vec4<f32>,
    env_params: vec4<f32>,
    cloud_params: vec4<f32>,
    viewport_wh: vec4<f32>,
}

@group(0) @binding(0) var<uniform> cam: GpuCameraUniforms;
@group(0) @binding(1) var cloud_tex: texture_2d<f32>;
@group(0) @binding(2) var cloud_sampler: sampler;

struct CloudOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) unit_pos: vec3<f32>,
}

fn cloud_hash31(p: vec3<f32>) -> f32 {
    var q = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    q += dot(q, q.yxz + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn cloud_noise(x: vec3<f32>) -> f32 {
    let i = floor(x);
    let f = fract(x);
    let u = f * f * (3.0 - 2.0 * f);
    let n000 = cloud_hash31(i);
    let n100 = cloud_hash31(i + vec3<f32>(1.0, 0.0, 0.0));
    let n010 = cloud_hash31(i + vec3<f32>(0.0, 1.0, 0.0));
    let n110 = cloud_hash31(i + vec3<f32>(1.0, 1.0, 0.0));
    let n001 = cloud_hash31(i + vec3<f32>(0.0, 0.0, 1.0));
    let n101 = cloud_hash31(i + vec3<f32>(1.0, 0.0, 1.0));
    let n011 = cloud_hash31(i + vec3<f32>(0.0, 1.0, 1.0));
    let n111 = cloud_hash31(i + vec3<f32>(1.0, 1.0, 1.0));
    return mix(
        mix(mix(n000, n100, u.x), mix(n010, n110, u.x), u.y),
        mix(mix(n001, n101, u.x), mix(n011, n111, u.x), u.y),
        u.z,
    );
}

fn sample_cloud_rgba_raw(uv: vec2<f32>, lod: f32) -> vec4<f32> {
    return textureSampleLevel(cloud_tex, cloud_sampler, uv, lod);
}

fn cloud_rgba_to_density(s: vec4<f32>) -> f32 {
    // Smoothly support both RGBA alpha-masked PNG cloud maps and opaque RGB JPEG maps without
    // hard threshold steps across texel boundaries.
    let rgb_luma = dot(s.rgb, vec3<f32>(0.299, 0.587, 0.114));
    return s.a * mix(rgb_luma, 1.0, 1.0 - smoothstep(0.97, 0.999, s.a));
}

fn sample_cloud_density(uv: vec2<f32>, lod: f32) -> f32 {
    return cloud_rgba_to_density(sample_cloud_rgba_raw(uv, lod));
}

// Fast 4-tap hardware-bilinear cubic B-spline interpolation: eliminates C1 texel grid Mach bands
// when viewing 1K/2K/4K/8K equirectangular cloud textures at close range.
fn sample_cloud_bicubic_rgba(uv: vec2<f32>, dims: vec2<f32>) -> vec4<f32> {
    let tc = uv * dims - vec2<f32>(0.5, 0.5);
    let i = floor(tc);
    let f = clamp(tc - i, vec2<f32>(0.0), vec2<f32>(1.0));
    let f2 = f * f;
    let f3 = f2 * f;

    let w0 = (vec2<f32>(1.0) - f) * (vec2<f32>(1.0) - f) * (vec2<f32>(1.0) - f) * (1.0 / 6.0);
    let w1 = (3.0 * f3 - 6.0 * f2 + vec2<f32>(4.0)) * (1.0 / 6.0);
    let w2 = (-3.0 * f3 + 3.0 * f2 + 3.0 * f + vec2<f32>(1.0)) * (1.0 / 6.0);
    let w3 = f3 * (1.0 / 6.0);

    let g0 = w0 + w1;
    let g1 = w2 + w3;
    let inv_dims = 1.0 / dims;
    let u0 = fract((i.x - 0.5 + w1.x / max(g0.x, 1e-5)) * inv_dims.x);
    let u1 = fract((i.x + 1.5 + w3.x / max(g1.x, 1e-5)) * inv_dims.x);
    let v0 = clamp((i.y - 0.5 + w1.y / max(g0.y, 1e-5)) * inv_dims.y, 0.001, 0.999);
    let v1 = clamp((i.y + 1.5 + w3.y / max(g1.y, 1e-5)) * inv_dims.y, 0.001, 0.999);

    let c00 = sample_cloud_rgba_raw(vec2<f32>(u0, v0), 0.0);
    let c10 = sample_cloud_rgba_raw(vec2<f32>(u1, v0), 0.0);
    let c01 = sample_cloud_rgba_raw(vec2<f32>(u0, v1), 0.0);
    let c11 = sample_cloud_rgba_raw(vec2<f32>(u1, v1), 0.0);

    let top = mix(c10, c00, g0.x);
    let bot = mix(c11, c01, g0.x);
    return mix(bot, top, g0.y);
}

fn cloud_density_to_height(d: f32) -> f32 {
    let x = clamp(d, 0.0, 1.0);
    let dome = smoothstep(0.06, 0.88, x);
    return dome * (0.62 + 0.38 * sqrt(x));
}

fn sample_cloud_filtered_density(uv: vec2<f32>, dims: vec2<f32>, uv_lod: f32, detail_w: f32) -> f32 {
    let d_lod = sample_cloud_density(uv, uv_lod);
    if (detail_w <= 0.01) {
        return d_lod;
    }
    let d_bic = cloud_rgba_to_density(sample_cloud_bicubic_rgba(uv, dims));
    return mix(d_lod, d_bic, detail_w);
}

@vertex
fn vs_cloud(@location(0) pos_u: vec4<f32>, @location(1) norm_v: vec4<f32>) -> CloudOut {
    let d = normalize(pos_u.xyz);
    let u_v = fract((atan2(d.y, d.x) + cam.cloud_params.y) / 6.28318530718 + 0.5);
    let v_v = clamp(0.5 - asin(clamp(d.z, -1.0, 1.0)) / 3.14159265359, 0.001, 0.999);
    // Smooth 5-tap mip-filtered heightmap displacement so cloud banks have physical 3D altitude relief.
    let eps = vec2<f32>(0.0018, 0.0018);
    let c0 = sample_cloud_density(vec2<f32>(u_v, v_v), 1.5);
    let ce = sample_cloud_density(vec2<f32>(fract(u_v + eps.x), v_v), 1.5);
    let cw = sample_cloud_density(vec2<f32>(fract(u_v - eps.x), v_v), 1.5);
    let cn = sample_cloud_density(vec2<f32>(u_v, clamp(v_v - eps.y, 0.001, 0.999)), 1.5);
    let cs = sample_cloud_density(vec2<f32>(u_v, clamp(v_v + eps.y, 0.001, 0.999)), 1.5);
    let h_vert = cloud_density_to_height(c0 * 0.40 + (ce + cw + cn + cs) * 0.15);

    let cloud_r = 6390200.0 + h_vert * 4200.0;
    let world_pos = d * cloud_r;
    let world_rel = (world_pos - cam.eye_high.xyz) - cam.eye_low.xyz;
    var out: CloudOut;
    out.clip_pos = cam.view_proj_rte * vec4<f32>(world_rel, 1.0);
    out.unit_pos = d;
    return out;
}

@fragment
fn fs_cloud(in: CloudOut) -> @location(0) vec4<f32> {
    let base_cloud_r = 6391500.0;
    let opacity = clamp(cam.cloud_params.x + cam.viewport_wh.w, 0.0, 1.0);
    if (opacity <= 0.004) {
        discard;
    }

    let eye_pos = cam.eye_high.xyz + cam.eye_low.xyz;
    let eye_r = max(length(eye_pos), 1.0);
    let eye_alt = max(eye_r - 6378137.0, 0.0);
    let below_clouds = select(0.0, 1.0, eye_r < base_cloud_r);

    let d = normalize(in.unit_pos);
    let shell_pos = d * base_cloud_r;
    let view_vec = shell_pos - eye_pos;
    let view_dist = max(length(view_vec), 1.0);
    let view_dir = view_vec / view_dist;

    // Physical plane-proximity transparency: fades smoothly as the camera approaches the cloud deck
    // (~13.4 km), but retains a non-zero translucent floor (~20-24%) so clouds never become 100%
    // transparent when flying close to or through them.
    let above_fade = smoothstep(1500.0, 160000.0, max(eye_r - base_cloud_r, 0.0));
    let below_fade = smoothstep(1200.0, 9000.0, max(base_cloud_r - eye_r, 0.0));
    let plane_alt_fade = mix(0.24, 1.0, mix(above_fade, below_fade, below_clouds));
    let ray_prox_fade = mix(0.78, 1.0, smoothstep(600.0, 10000.0, view_dist));
    let eye_up = eye_pos / eye_r;
    let sky_hemi_fade = mix(1.0, max(0.22, smoothstep(-0.22, 0.02, dot(view_dir, eye_up))), below_clouds);
    let cross_fade = plane_alt_fade * ray_prox_fade * sky_hemi_fade;
    if (cross_fade <= 0.004) {
        discard;
    }

    // Soften low-altitude oblique horizon limb so the cloud shell never ends in a hard polygonal bar.
    let low_alt_factor = 1.0 - smoothstep(40000.0, 350000.0, eye_alt);
    let horizon_graze = abs(dot(view_dir, d));
    let horizon_fade = mix(1.0, smoothstep(0.010, 0.14, horizon_graze), low_alt_factor);
    if (horizon_fade <= 0.004) {
        discard;
    }

    let east_raw = vec3<f32>(-d.y, d.x, 0.0);
    let east_len = length(east_raw);
    let t_east = select(vec3<f32>(1.0, 0.0, 0.0), east_raw / max(east_len, 1e-5), east_len > 1e-4);
    let t_north = normalize(cross(d, t_east));

    let u_base = fract((atan2(d.y, d.x) + cam.cloud_params.y) / 6.28318530718 + 0.5);
    let v_base = clamp(0.5 - asin(clamp(d.z, -1.0, 1.0)) / 3.14159265359, 0.001, 0.999);
    let dims = max(vec2<f32>(textureDimensions(cloud_tex, 0)), vec2<f32>(1.0));
    let du = 1.0 / dims.x;
    let dv = 1.0 / dims.y;
    let uv_lod = clamp(log2(max(dims.x / 1024.0, 1.0)) * smoothstep(2200000.0, 14000000.0, eye_alt), 0.0, 3.0);

    let detail_w = 1.0 - smoothstep(120000.0, 2200000.0, eye_alt);
    let mode_25d = smoothstep(0.15, 0.75, clamp(max(detail_w, below_clouds), 0.0, 1.0));

    var u = u_base;
    var v = v_base;
    if (detail_w > 0.01) {
        let warp_wind = vec3<f32>(cam.cloud_params.z * 0.00012, 0.0, 0.0);
        let wu = cloud_noise(d * 520.0 + warp_wind) - 0.5;
        let wv = cloud_noise(d * 520.0 + vec3<f32>(19.3, 7.1, 31.7) + warp_wind) - 0.5;
        let warp_amp = 0.75 * detail_w;
        u = fract(u_base + wu * du * warp_amp);
        v = clamp(v_base + wv * dv * warp_amp, 0.001, 0.999);
    }

    // Smooth 2-step view-ray heightmap parallax derived from cloud transparency/density
    let v_east = dot(view_dir, t_east);
    let v_north = dot(view_dir, t_north);
    let v_rad = max(abs(dot(view_dir, d)), 0.25);
    let h_pre = cloud_density_to_height(sample_cloud_density(vec2<f32>(u, v), uv_lod + 0.6));
    let par_amp = mix(0.0015, 0.00075, mode_25d);
    let par1 = (h_pre - 0.22) * par_amp / v_rad;
    let u_p1 = fract(u - v_east * par1 * clamp(east_len, 0.25, 1.0));
    let v_p1 = clamp(v + v_north * par1, 0.001, 0.999);
    let h_mid = 0.5 * (h_pre + cloud_density_to_height(sample_cloud_filtered_density(vec2<f32>(u_p1, v_p1), dims, uv_lod, detail_w)));
    let par2 = (h_mid - 0.22) * par_amp / v_rad;
    u = fract(u - v_east * par2 * clamp(east_len, 0.25, 1.0));
    v = clamp(v + v_north * par2, 0.001, 0.999);

    let s_lod = sample_cloud_rgba_raw(vec2<f32>(u, v), uv_lod);
    let s_c = select(s_lod, mix(s_lod, sample_cloud_bicubic_rgba(vec2<f32>(u, v), dims), detail_w), detail_w > 0.01);
    let d_c = cloud_rgba_to_density(s_c);

    // Sample smoothed heightmap neighborhood for C1-continuous 3D cloud normals & self-shadowing
    let grad_step_u = du * mix(1.75, 1.35, detail_w);
    let grad_step_v = dv * mix(1.75, 1.35, detail_w);
    let d_e = sample_cloud_filtered_density(vec2<f32>(fract(u + grad_step_u), v), dims, uv_lod, detail_w);
    let d_w = sample_cloud_filtered_density(vec2<f32>(fract(u - grad_step_u), v), dims, uv_lod, detail_w);
    let d_n = sample_cloud_filtered_density(vec2<f32>(u, clamp(v - grad_step_v, 0.001, 0.999)), dims, uv_lod, detail_w);
    let d_s = sample_cloud_filtered_density(vec2<f32>(u, clamp(v + grad_step_v, 0.001, 0.999)), dims, uv_lod, detail_w);

    var fbm_billow = 0.0;
    let base_density = 0.44 * d_c + 0.14 * (d_e + d_w + d_n + d_s);
    let clear_cut = smoothstep(0.14, 0.54, base_density);
    var refined = clear_cut;
    if (detail_w > 0.005 && clear_cut > 0.004) {
        let wind = vec3<f32>(cam.cloud_params.z * 0.00018, 0.0, 0.0);
        let n1 = cloud_noise(d * 850.0 + wind);
        let n2 = cloud_noise(d * 2400.0 + wind * 1.4);
        let n3 = cloud_noise(d * 6800.0 - wind * 0.8);
        let fbm = n1 * 0.55 + n2 * 0.30 + n3 * 0.15;
        fbm_billow = (fbm - 0.5) * 0.16 * detail_w;
        let eroded = clamp(clear_cut * (0.44 + fbm * 1.16) - 0.13, 0.0, 1.0);
        refined = mix(clear_cut, eroded, detail_w * 0.88);
    }

    let h_c = clamp(cloud_density_to_height(d_c) + fbm_billow, 0.0, 1.0);
    let h_e = cloud_density_to_height(d_e);
    let h_w = cloud_density_to_height(d_w);
    let h_n = cloud_density_to_height(d_n);
    let h_s = cloud_density_to_height(d_s);
    let h_smooth = clamp(0.40 * h_c + 0.15 * (h_e + h_w + h_n + h_s), 0.0, 1.0);

    // Full 3D space uses rich sculpted optical contrast; 2.5D below clouds applies wispy FBM + steep sigmoid cutout
    let crisp_3d = pow(clamp(d_c, 0.0, 1.0), 1.68);
    let sigmoid_curve = 1.0 / (1.0 + exp(-(refined - 0.43) * 11.0));
    let hard_zero_gate = smoothstep(0.13, 0.31, refined);
    let mask_25d = clamp(sigmoid_curve * hard_zero_gate * 1.04, 0.0, 1.0);
    let mask_density = mix(crisp_3d, mask_25d, mode_25d);

    let sun_dir = normalize(cam.sun_dir.xyz);
    let ndotl = dot(d, sun_dir);
    let dn_strength = clamp(cam.env_params.x, 0.0, 1.0);
    let term = smoothstep(-0.14, 0.18, ndotl);
    let night_fade = mix(0.14, 1.0, mix(1.0, term, dn_strength));
    let lit_mask = mix(night_fade, 1.0, below_clouds);
    let layer_translucency = mix(1.0, 0.88, below_clouds);
    let alpha = mask_density * opacity * lit_mask * cross_fade * horizon_fade * layer_translucency;
    if (alpha <= 0.003) {
        discard;
    }

    // Construct 3D heightmap surface normal in ECEF from height gradients
    let normal_amp = mix(3.4, 2.2, detail_w);
    let grad_east = (h_e - h_w) * normal_amp;
    let grad_north = (h_n - h_s) * normal_amp;
    let n_cloud = normalize(d - t_east * grad_east - t_north * grad_north);

    // Effective directional light for heightmap relief (uses solar direction when day/night is on,
    // and blends a grazing northwest sky key-light so heightmap relief is always visible in unlit mode).
    let unlit_key = normalize(d * 0.52 + t_north * 0.62 - t_east * 0.58);
    let relief_light = normalize(mix(unlit_key, normalize(sun_dir + d * 0.28), dn_strength * 0.75));
    let l_east = dot(relief_light, t_east);
    let l_north = dot(relief_light, t_north);
    let sun_uv = vec2<f32>(
        fract(u + l_east * du * 2.2),
        clamp(v - l_north * dv * 2.2, 0.001, 0.999),
    );
    let h_sun = cloud_density_to_height(sample_cloud_filtered_density(sun_uv, dims, uv_lod, detail_w));

    let slope_lit = clamp(dot(n_cloud, relief_light), 0.0, 1.0);
    let horizon_shadow = clamp((h_c - h_sun) * 2.1, -0.28, 0.28);
    let height_ao = mix(0.68, 1.06, smoothstep(0.05, 0.85, h_smooth));
    let silver_rim = pow(1.0 - clamp(abs(dot(view_dir, n_cloud)), 0.0, 1.0), 2.4)
        * smoothstep(0.12, 0.52, d_c)
        * (1.0 - smoothstep(0.58, 0.92, d_c))
        * 0.18;

    let relief_factor = clamp(0.55 + slope_lit * 0.45 + horizon_shadow * 0.65, 0.0, 1.0);
    let tex_tint = clamp(height_ao * (0.86 + horizon_shadow * 0.45) + silver_rim, 0.68, 1.08);
    let shade_base = mix(
        vec3<f32>(0.62, 0.69, 0.79),
        vec3<f32>(0.995, 0.998, 1.0),
        clamp(h_smooth * 0.62 + relief_factor * 0.46, 0.0, 1.0),
    ) * tex_tint;
    let below_shade = mix(
        vec3<f32>(0.70, 0.76, 0.85),
        vec3<f32>(0.99, 0.995, 1.0),
        clamp((1.0 - h_smooth * 0.35) * 0.45 + relief_factor * 0.55, 0.0, 1.0),
    ) * clamp(tex_tint * 1.03, 0.74, 1.06);
    let flat_cloud = mix(shade_base, below_shade, below_clouds);

    let sunset = exp(-pow(ndotl * 7.5, 2.0)) * vec3<f32>(0.48, 0.22, 0.08);
    let sun_slope = smoothstep(-0.10, 0.65, dot(n_cloud, sun_dir));
    let day_cloud = (vec3<f32>(0.04, 0.06, 0.10) + cam.sun_color.rgb * term * mix(0.76, 1.12, sun_slope) + sunset) * tex_tint;
    let rgb = mix(flat_cloud, day_cloud, dn_strength * (1.0 - below_clouds));
    return vec4<f32>(rgb, alpha);
}
"#;

const DYNAMIC_MESH_WGSL: &str = r#"
struct GpuCameraUniforms {
    view_proj_rte: mat4x4<f32>,
    inv_view_proj_rte: mat4x4<f32>,
    ecef_from_icrf: mat4x4<f32>,
    icrf_from_ecef: mat4x4<f32>,
    eye_high: vec4<f32>,
    eye_low: vec4<f32>,
    sun_dir: vec4<f32>,
    sun_color: vec4<f32>,
    ambient_color: vec4<f32>,
    sky_top: vec4<f32>,
    sky_bottom: vec4<f32>,
    moon_dir: vec4<f32>,
    env_params: vec4<f32>,
    cloud_params: vec4<f32>,
    viewport_wh: vec4<f32>,
}

struct GpuDynamicUniforms {
    anchor_high: vec4<f32>,
    anchor_low: vec4<f32>,
    color: vec4<f32>,
    dyn_meta: vec4<u32>,
}

@group(0) @binding(0) var<uniform> cam: GpuCameraUniforms;
@group(0) @binding(1) var<uniform> dyn_u: GpuDynamicUniforms;

struct DynVsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) @interpolate(flat) unlit: u32,
}

@vertex
fn vs_dynamic(
    @location(0) pos_u: vec4<f32>,
    @location(1) norm_v: vec4<f32>,
) -> DynVsOut {
    let center_rel = (dyn_u.anchor_high.xyz - cam.eye_high.xyz)
                   + (dyn_u.anchor_low.xyz - cam.eye_low.xyz);
    let world_rel = center_rel + pos_u.xyz;
    var out: DynVsOut;
    out.clip_pos = cam.view_proj_rte * vec4<f32>(world_rel, 1.0);
    out.normal = norm_v.xyz;
    out.color = dyn_u.color;
    out.unlit = dyn_u.dyn_meta.x;
    return out;
}

@fragment
fn fs_dynamic(in: DynVsOut) -> @location(0) vec4<f32> {
    if (in.unlit != 0u) {
        return in.color;
    }
    let len_sq = dot(in.normal, in.normal);
    let n = select(vec3<f32>(0.0, 1.0, 0.0), normalize(in.normal), len_sq > 0.0001);
    let ndotl = max(dot(n, normalize(cam.sun_dir.xyz)), 0.0);
    let dn = clamp(cam.env_params.x, 0.0, 1.0);
    let flat_lit = vec3<f32>(0.85) + vec3<f32>(0.22) * ndotl;
    let day_lit = cam.ambient_color.rgb + cam.sun_color.rgb * ndotl;
    let lit = mix(flat_lit, day_lit, dn);
    return vec4<f32>(in.color.rgb * lit, in.color.a);
}
"#;

const OVERLAY_COMPOSITE_WGSL: &str = r#"
@group(0) @binding(0) var overlay_tex: texture_2d<f32>;
@group(0) @binding(1) var overlay_sampler: sampler;

struct OverlayOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_overlay(@builtin(vertex_index) vid: u32) -> OverlayOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -3.0),
        vec2<f32>(3.0, 1.0),
        vec2<f32>(-1.0, 1.0),
    );
    let p = positions[vid];
    var out: OverlayOut;
    out.pos = vec4<f32>(p, 0.0, 1.0);
    out.uv = vec2<f32>((p.x + 1.0) * 0.5, (1.0 - p.y) * 0.5);
    return out;
}

@fragment
fn fs_overlay(in: OverlayOut) -> @location(0) vec4<f32> {
    return textureSample(overlay_tex, overlay_sampler, in.uv);
}
"#;

#[derive(Clone, Copy)]
struct SlabEntry {
    revision: u64,
    offset: u32,
    len: u32,
    last_used_frame: u64,
}

/// Free-list GPU vertex slab allocator holding tile-local RTC vertices across frames.
struct GpuTileSlabAllocator {
    buffer: wgpu::Buffer,
    capacity_vertices: u32,
    resident: HashMap<TileKeyId, SlabEntry>,
    free_blocks: Vec<(u32, u32)>, // (offset, len)
    bump_cursor: u32,
}

impl GpuTileSlabAllocator {
    fn new(device: &wgpu::Device, initial_capacity_vertices: u32) -> Self {
        let cap = initial_capacity_vertices.max(65_536);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("strata_earth_tile_vertex_slab"),
            size: (cap as u64) * (std::mem::size_of::<RteTileVertex>() as u64),
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        Self {
            buffer,
            capacity_vertices: cap,
            resident: HashMap::new(),
            free_blocks: Vec::new(),
            bump_cursor: 0,
        }
    }

    fn alloc_range(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        needed: u32,
        current_frame: u64,
    ) -> (u32, bool) {
        // 1. Reuse best-fit free block.
        if let Some((idx, (off, len))) = self
            .free_blocks
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, (_, l))| *l >= needed)
            .min_by_key(|(_, (_, l))| *l)
        {
            self.free_blocks.swap_remove(idx);
            if len > needed {
                self.free_blocks.push((off + needed, len - needed));
            }
            return (off, false);
        }

        // 2. Bump allocate if space remains.
        if self.bump_cursor + needed <= self.capacity_vertices {
            let off = self.bump_cursor;
            self.bump_cursor += needed;
            return (off, false);
        }

        // 3. Evict LRU tiles not used this frame.
        let mut stale: Vec<(TileKeyId, SlabEntry)> = self
            .resident
            .iter()
            .filter(|(_, e)| e.last_used_frame < current_frame)
            .map(|(k, e)| (*k, *e))
            .collect();
        stale.sort_by_key(|(_, e)| e.last_used_frame);

        let mut reclaimed = 0u32;
        for (k, e) in stale {
            self.resident.remove(&k);
            self.free_blocks.push((e.offset, e.len));
            reclaimed += e.len;
            if reclaimed >= needed {
                break;
            }
        }
        self.coalesce_free_blocks();

        if let Some((idx, (off, len))) = self
            .free_blocks
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, (_, l))| *l >= needed)
            .min_by_key(|(_, (_, l))| *l)
        {
            self.free_blocks.swap_remove(idx);
            if len > needed {
                self.free_blocks.push((off + needed, len - needed));
            }
            return (off, false);
        }

        // 4. Grow buffer if still tight.
        let new_cap = (self.capacity_vertices * 2).max(self.bump_cursor + needed + 65_536);
        let new_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("strata_earth_tile_vertex_slab_grown"),
            size: (new_cap as u64) * (std::mem::size_of::<RteTileVertex>() as u64),
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let copy_bytes = (self.bump_cursor as u64) * (std::mem::size_of::<RteTileVertex>() as u64);
        if copy_bytes > 0 {
            let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("strata_earth_slab_grow_copy"),
            });
            enc.copy_buffer_to_buffer(&self.buffer, 0, &new_buf, 0, copy_bytes);
            queue.submit(Some(enc.finish()));
        }
        self.buffer = new_buf;
        self.capacity_vertices = new_cap;
        let off = self.bump_cursor;
        self.bump_cursor += needed;
        (off, true)
    }

    fn coalesce_free_blocks(&mut self) {
        if self.free_blocks.len() < 2 {
            return;
        }
        self.free_blocks.sort_unstable_by_key(|(o, _)| *o);
        let mut merged: Vec<(u32, u32)> = Vec::with_capacity(self.free_blocks.len());
        for (o, l) in self.free_blocks.drain(..) {
            if let Some(last) = merged.last_mut() {
                if last.0 + last.1 == o {
                    last.1 += l;
                    continue;
                }
            }
            merged.push((o, l));
        }
        // Reclaim tail back into bump_cursor.
        while let Some(&(o, l)) = merged.last() {
            if o + l == self.bump_cursor {
                self.bump_cursor = o;
                merged.pop();
            } else {
                break;
            }
        }
        self.free_blocks = merged;
    }

    fn upload_meshes(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        meshes: &[Arc<RetainedTileMesh>],
        frame_idx: u64,
        budget: usize,
    ) -> bool {
        let mut bind_group_dirty = false;
        let mut uploaded = 0usize;

        // First mark already-resident matching revisions as fresh this frame.
        for mesh in meshes {
            if let Some(entry) = self.resident.get_mut(&mesh.key) {
                if entry.revision == mesh.revision && entry.len == mesh.vertices.len() as u32 {
                    entry.last_used_frame = frame_idx;
                }
            }
        }

        for mesh in meshes {
            let needed = mesh.vertices.len() as u32;
            if needed == 0 {
                continue;
            }
            if let Some(entry) = self.resident.get(&mesh.key) {
                if entry.revision == mesh.revision && entry.len == needed {
                    continue;
                }
            }
            if uploaded >= budget {
                continue;
            }
            if let Some(old) = self.resident.remove(&mesh.key) {
                self.free_blocks.push((old.offset, old.len));
                self.coalesce_free_blocks();
            }
            let (off, grew) = self.alloc_range(device, queue, needed, frame_idx);
            if grew {
                bind_group_dirty = true;
            }
            let byte_offset = (off as u64) * (std::mem::size_of::<RteTileVertex>() as u64);
            queue.write_buffer(
                &self.buffer,
                byte_offset,
                bytemuck::cast_slice(&mesh.vertices),
            );
            self.resident.insert(
                mesh.key,
                SlabEntry {
                    revision: mesh.revision,
                    offset: off,
                    len: needed,
                    last_used_frame: frame_idx,
                },
            );
            uploaded += 1;
        }
        bind_group_dirty
    }

    fn base_vertex(&self, key: TileKeyId, revision: u64) -> Option<u32> {
        self.resident
            .get(&key)
            .filter(|e| e.revision == revision)
            .map(|e| e.offset)
    }
}

#[derive(Clone, Copy)]
struct AtlasSlot {
    tex_id: u64,
    revision: u64,
    layer: u32,
    last_used_frame: u64,
}

/// LRU `Texture2DArray` tile imagery atlas (layer 0 = solid white fallback).
struct GpuImageryAtlas {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    slots_by_id: HashMap<u64, AtlasSlot>,
    layer_owner: Vec<Option<u64>>,
}

impl GpuImageryAtlas {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, layers: u32) -> Self {
        let layers = layers.max(16);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("strata_earth_imagery_atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_TILE_SIZE,
                height: ATLAS_TILE_SIZE,
                depth_or_array_layers: layers,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        // Layer 0: solid white fallback texture.
        let white = vec![255u8; (ATLAS_TILE_SIZE * ATLAS_TILE_SIZE * 4) as usize];
        Self::write_layer(queue, &texture, 0, ATLAS_TILE_SIZE, ATLAS_TILE_SIZE, &white);

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("strata_earth_imagery_atlas_view"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        Self {
            _texture: texture,
            view,
            slots_by_id: HashMap::new(),
            layer_owner: vec![None; layers as usize],
        }
    }

    fn write_layer(
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        layer: u32,
        w: u32,
        h: u32,
        rgba: &[u8],
    ) {
        if w == ATLAS_TILE_SIZE
            && h == ATLAS_TILE_SIZE
            && rgba.len() >= (ATLAS_TILE_SIZE * ATLAS_TILE_SIZE * 4) as usize
        {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &rgba[..(ATLAS_TILE_SIZE * ATLAS_TILE_SIZE * 4) as usize],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(ATLAS_TILE_SIZE * 4),
                    rows_per_image: Some(ATLAS_TILE_SIZE),
                },
                wgpu::Extent3d {
                    width: ATLAS_TILE_SIZE,
                    height: ATLAS_TILE_SIZE,
                    depth_or_array_layers: 1,
                },
            );
            return;
        }
        // Nearest/bilinear resample into ATLAS_TILE_SIZE x ATLAS_TILE_SIZE on CPU edge case.
        let mut resampled = vec![255u8; (ATLAS_TILE_SIZE * ATLAS_TILE_SIZE * 4) as usize];
        if w > 0 && h > 0 && rgba.len() >= (w * h * 4) as usize {
            for dy in 0..ATLAS_TILE_SIZE {
                let sy = ((dy as u64 * h as u64) / ATLAS_TILE_SIZE as u64).min(h as u64 - 1) as u32;
                for dx in 0..ATLAS_TILE_SIZE {
                    let sx =
                        ((dx as u64 * w as u64) / ATLAS_TILE_SIZE as u64).min(w as u64 - 1) as u32;
                    let si = ((sy * w + sx) * 4) as usize;
                    let di = ((dy * ATLAS_TILE_SIZE + dx) * 4) as usize;
                    resampled[di..di + 4].copy_from_slice(&rgba[si..si + 4]);
                }
            }
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: layer,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &resampled,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ATLAS_TILE_SIZE * 4),
                rows_per_image: Some(ATLAS_TILE_SIZE),
            },
            wgpu::Extent3d {
                width: ATLAS_TILE_SIZE,
                height: ATLAS_TILE_SIZE,
                depth_or_array_layers: 1,
            },
        );
    }

    fn upload_textures(
        &mut self,
        queue: &wgpu::Queue,
        textures: &[Arc<RetainedTileTexture>],
        frame_idx: u64,
        budget: usize,
    ) {
        for tex in textures {
            if let Some(slot) = self.slots_by_id.get_mut(&tex.id) {
                if slot.revision == tex.revision {
                    slot.last_used_frame = frame_idx;
                }
            }
        }
        let mut uploaded = 0usize;
        for tex in textures {
            if let Some(slot) = self.slots_by_id.get(&tex.id) {
                if slot.revision == tex.revision {
                    continue;
                }
            }
            if uploaded >= budget {
                continue;
            }
            let target_layer = if let Some(existing) = self.slots_by_id.get(&tex.id) {
                existing.layer
            } else if let Some((idx, _)) = self
                .layer_owner
                .iter()
                .enumerate()
                .skip(1)
                .find(|(_, owner)| owner.is_none())
            {
                idx as u32
            } else if let Some((victim_id, victim_layer)) = self
                .slots_by_id
                .values()
                .filter(|s| s.last_used_frame < frame_idx)
                .min_by_key(|s| s.last_used_frame)
                .map(|s| (s.tex_id, s.layer))
            {
                self.slots_by_id.remove(&victim_id);
                victim_layer
            } else {
                continue;
            };

            Self::write_layer(
                queue,
                &self._texture,
                target_layer,
                tex.width,
                tex.height,
                &tex.rgba8,
            );
            self.layer_owner[target_layer as usize] = Some(tex.id);
            self.slots_by_id.insert(
                tex.id,
                AtlasSlot {
                    tex_id: tex.id,
                    revision: tex.revision,
                    layer: target_layer,
                    last_used_frame: frame_idx,
                },
            );
            uploaded += 1;
        }
    }

    #[inline]
    fn layer_for(&self, tex_id: Option<u64>, revision: u64) -> u32 {
        let Some(id) = tex_id else {
            return 0;
        };
        let _ = revision;
        self.slots_by_id.get(&id).map(|s| s.layer).unwrap_or(0)
    }
}

struct CachedIndexTopology {
    buffer: wgpu::Buffer,
    index_count: u32,
}

struct CachedDynamicGpuMesh {
    revision: u64,
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    index_count: u32,
}

struct OffscreenTarget {
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    color: wgpu::Texture,
    color_view: wgpu::TextureView,
    _depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
    readback: wgpu::Buffer,
    padded_bytes_per_row: u32,
}

struct GpuEnvironmentState {
    land_mask_id_rev: Option<(u64, u64)>,
    cloud_map_id_rev: Option<(u64, u64)>,
    moon_tex_id_rev: Option<(u64, u64)>,
    skybox_cube_id_rev: Option<(u64, u64)>,
    stars_ptr: usize,
    stars_len: usize,
    land_mask_view: wgpu::TextureView,
    cloud_map_view: wgpu::TextureView,
    moon_view: wgpu::TextureView,
    skybox_cube_view: wgpu::TextureView,
    env_sampler: wgpu::Sampler,
    star_buffer: wgpu::Buffer,
    star_capacity: usize,
    star_count: u32,
}

fn srgb_u8_to_linear(c: u8) -> f32 {
    let x = c as f32 / 255.0;
    if x <= 0.04045 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb_u8(c: f32) -> u8 {
    let x = c.clamp(0.0, 1.0);
    let s = if x <= 0.0031308 {
        x * 12.92
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    };
    (s * 255.0).round().clamp(0.0, 255.0) as u8
}

fn downsample_srgb_rgba8_half(src: &[u8], w: u32, h: u32) -> (u32, u32, Vec<u8>) {
    let nw = (w / 2).max(1);
    let nh = (h / 2).max(1);
    let mut out = vec![0u8; (nw as usize) * (nh as usize) * 4];
    for y in 0..nh {
        let sy0 = (y * 2).min(h - 1) as usize;
        let sy1 = (y * 2 + 1).min(h - 1) as usize;
        for x in 0..nw {
            let sx0 = (x * 2).min(w - 1) as usize;
            let sx1 = (x * 2 + 1).min(w - 1) as usize;
            let coords = [(sx0, sy0), (sx1, sy0), (sx0, sy1), (sx1, sy1)];
            let mut lr = 0.0f32;
            let mut lg = 0.0f32;
            let mut lb = 0.0f32;
            let mut la = 0.0f32;
            for &(sx, sy) in &coords {
                let idx = (sy * (w as usize) + sx) * 4;
                lr += srgb_u8_to_linear(src[idx]);
                lg += srgb_u8_to_linear(src[idx + 1]);
                lb += srgb_u8_to_linear(src[idx + 2]);
                la += src[idx + 3] as f32 / 255.0;
            }
            let dst = ((y as usize) * (nw as usize) + (x as usize)) * 4;
            out[dst] = linear_to_srgb_u8(lr * 0.25);
            out[dst + 1] = linear_to_srgb_u8(lg * 0.25);
            out[dst + 2] = linear_to_srgb_u8(lb * 0.25);
            out[dst + 3] = ((la * 0.25) * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    (nw, nh, out)
}

fn create_rgba8_cube_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    face_size: u32,
    faces: &[std::sync::Arc<Vec<u8>>; 6],
    format: wgpu::TextureFormat,
) -> wgpu::TextureView {
    let max_dim = device.limits().max_texture_dimension_2d.max(1);
    let mut size = face_size.max(1);
    let mut layer_bufs: Vec<Vec<u8>> = faces
        .iter()
        .map(|f| {
            let expected = (size as usize) * (size as usize) * 4;
            if f.len() >= expected {
                f[..expected].to_vec()
            } else {
                vec![0u8; expected]
            }
        })
        .collect();
    while size > max_dim && size > 1 {
        let mut next_size = size;
        layer_bufs = layer_bufs
            .into_iter()
            .map(|buf| {
                let (nw, _, down) = downsample_srgb_rgba8_half(&buf, size, size);
                next_size = nw;
                down
            })
            .collect();
        size = next_size;
    }

    let mip_level_count = if size > 1 {
        32 - size.leading_zeros()
    } else {
        1
    };
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 6,
        },
        mip_level_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    for layer in 0..6u32 {
        let mut cur_w = size;
        let mut cur_buf = std::mem::take(&mut layer_bufs[layer as usize]);
        for mip in 0..mip_level_count {
            let expected = (cur_w as usize) * (cur_w as usize) * 4;
            if cur_buf.len() >= expected {
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &tex,
                        mip_level: mip,
                        origin: wgpu::Origin3d {
                            x: 0,
                            y: 0,
                            z: layer,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    &cur_buf[..expected],
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(cur_w * 4),
                        rows_per_image: Some(cur_w),
                    },
                    wgpu::Extent3d {
                        width: cur_w,
                        height: cur_w,
                        depth_or_array_layers: 1,
                    },
                );
            }
            if mip + 1 < mip_level_count {
                let (nw, _, next) = downsample_srgb_rgba8_half(&cur_buf, cur_w, cur_w);
                cur_w = nw;
                cur_buf = next;
            }
        }
    }

    tex.create_view(&wgpu::TextureViewDescriptor {
        label: Some(label),
        dimension: Some(wgpu::TextureViewDimension::Cube),
        ..Default::default()
    })
}

fn create_rgba8_2d_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> wgpu::TextureView {
    let max_dim = device.limits().max_texture_dimension_2d.max(1);
    let mut w = width.max(1);
    let mut h = height.max(1);
    let mut scaled: Option<Vec<u8>> = None;
    let initial_expected = (w as usize) * (h as usize) * 4;
    if rgba.len() >= initial_expected && (w > max_dim || h > max_dim) {
        let mut buf = rgba[..initial_expected].to_vec();
        while (w > max_dim || h > max_dim) && w > 1 && h > 1 {
            let (nw, nh, next) = downsample_srgb_rgba8_half(&buf, w, h);
            w = nw;
            h = nh;
            buf = next;
        }
        scaled = Some(buf);
    }
    let data: &[u8] = scaled.as_deref().unwrap_or(rgba);

    let expected = (w as usize) * (h as usize) * 4;
    let can_mip = data.len() >= expected && w >= 64 && h >= 64;
    let mip_level_count = if can_mip {
        (32 - w.min(h).leading_zeros()).min(5)
    } else {
        1
    };

    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    if data.len() >= expected {
        let mut cur_w = w;
        let mut cur_h = h;
        let mut mip_owned: Option<Vec<u8>> = None;
        for mip in 0..mip_level_count {
            let mip_bytes = mip_owned.as_deref().unwrap_or(&data[..expected]);
            let mip_exp = (cur_w as usize) * (cur_h as usize) * 4;
            if mip_bytes.len() >= mip_exp {
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &tex,
                        mip_level: mip,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &mip_bytes[..mip_exp],
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(cur_w * 4),
                        rows_per_image: Some(cur_h),
                    },
                    wgpu::Extent3d {
                        width: cur_w,
                        height: cur_h,
                        depth_or_array_layers: 1,
                    },
                );
            }
            if mip + 1 < mip_level_count {
                let (nw, nh, next) = downsample_srgb_rgba8_half(mip_bytes, cur_w, cur_h);
                cur_w = nw;
                cur_h = nh;
                mip_owned = Some(next);
            }
        }
    }
    tex.create_view(&Default::default())
}

fn build_unit_icosphere(subdivisions: u32) -> (Vec<RteTileVertex>, Vec<u32>) {
    let t = (1.0f32 + 5.0f32.sqrt()) * 0.5;
    let raw = [
        [-1.0, t, 0.0],
        [1.0, t, 0.0],
        [-1.0, -t, 0.0],
        [1.0, -t, 0.0],
        [0.0, -1.0, t],
        [0.0, 1.0, t],
        [0.0, -1.0, -t],
        [0.0, 1.0, -t],
        [t, 0.0, -1.0],
        [t, 0.0, 1.0],
        [-t, 0.0, -1.0],
        [-t, 0.0, 1.0],
    ];
    let norm = |p: [f32; 3]| -> [f32; 3] {
        let l = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt().max(1e-6);
        [p[0] / l, p[1] / l, p[2] / l]
    };
    let mut verts: Vec<[f32; 3]> = raw.iter().map(|p| norm(*p)).collect();
    let mut tris: Vec<[u32; 3]> = vec![
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];

    for _ in 0..subdivisions {
        let mut mid_cache: HashMap<(u32, u32), u32> = HashMap::new();
        let mut get_mid = |a: u32, b: u32, vs: &mut Vec<[f32; 3]>| -> u32 {
            let key = if a < b { (a, b) } else { (b, a) };
            if let Some(&idx) = mid_cache.get(&key) {
                return idx;
            }
            let pa = vs[a as usize];
            let pb = vs[b as usize];
            let m = norm([
                (pa[0] + pb[0]) * 0.5,
                (pa[1] + pb[1]) * 0.5,
                (pa[2] + pb[2]) * 0.5,
            ]);
            let idx = vs.len() as u32;
            vs.push(m);
            mid_cache.insert(key, idx);
            idx
        };
        let mut next_tris = Vec::with_capacity(tris.len() * 4);
        for [a, b, c] in tris {
            let ab = get_mid(a, b, &mut verts);
            let bc = get_mid(b, c, &mut verts);
            let ca = get_mid(c, a, &mut verts);
            next_tris.push([a, ab, ca]);
            next_tris.push([b, bc, ab]);
            next_tris.push([c, ca, bc]);
            next_tris.push([ab, bc, ca]);
        }
        tris = next_tris;
    }

    let rte_verts: Vec<RteTileVertex> = verts
        .into_iter()
        .map(|p| RteTileVertex::new(p, p, [0.0, 0.0]))
        .collect();
    let mut indices = Vec::with_capacity(tris.len() * 3);
    for [a, b, c] in tris {
        indices.push(a);
        indices.push(b);
        indices.push(c);
    }
    (rte_verts, indices)
}

/// Hardware-accelerated `wgpu` 3D renderer capable of rendering directly into any `TextureView`
/// (swapchain surface) or offscreen RGBA target.
pub struct Wgpu3DRenderer {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    color_format: wgpu::TextureFormat,
    camera_buffer: wgpu::Buffer,
    sky_pipeline: wgpu::RenderPipeline,
    sky_bind_group_layout: wgpu::BindGroupLayout,
    sky_bind_group: wgpu::BindGroup,
    star_pipeline: wgpu::RenderPipeline,
    star_bind_group_layout: wgpu::BindGroupLayout,
    star_bind_group: wgpu::BindGroup,
    cloud_pipeline: wgpu::RenderPipeline,
    cloud_bind_group_layout: wgpu::BindGroupLayout,
    cloud_bind_group: wgpu::BindGroup,
    cloud_vbuf: wgpu::Buffer,
    cloud_ibuf: wgpu::Buffer,
    cloud_index_count: u32,
    env_state: GpuEnvironmentState,
    terrain_pipeline: wgpu::RenderPipeline,
    terrain_bind_group_layout: wgpu::BindGroupLayout,
    terrain_bind_group: wgpu::BindGroup,
    terrain_sampler: wgpu::Sampler,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    slab: GpuTileSlabAllocator,
    atlas: GpuImageryAtlas,
    topologies: HashMap<(u32, bool), CachedIndexTopology>,
    dynamic_pipeline: wgpu::RenderPipeline,
    dynamic_bind_group_layout: wgpu::BindGroupLayout,
    dynamic_uniform_buffer: wgpu::Buffer,
    dynamic_uniform_capacity: usize,
    dynamic_uniform_stride: u32,
    dynamic_bind_group: wgpu::BindGroup,
    dynamic_meshes: HashMap<u64, CachedDynamicGpuMesh>,
    overlay_pipeline: wgpu::RenderPipeline,
    overlay_bind_group_layout: wgpu::BindGroupLayout,
    overlay_sampler: wgpu::Sampler,
    depth_target: Option<(u32, u32, wgpu::TextureView)>,
    offscreen: Option<OffscreenTarget>,
    frame_index: u64,
    last_geometry_rev: u64,
    last_terrain_frame: Option<Arc<RetainedTerrainFrame>>,
    last_scene_ptr: usize,
    cached_fallback_frame: Option<Arc<RetainedTerrainFrame>>,
}

/// Returns adapter-supported [`wgpu::Limits`] requesting up to [`MAX_ATLAS_LAYERS`] texture array layers.
pub fn preferred_wgpu_limits(adapter: &wgpu::Adapter) -> wgpu::Limits {
    let mut limits = wgpu::Limits::default();
    limits.max_texture_array_layers = adapter
        .limits()
        .max_texture_array_layers
        .min(MAX_ATLAS_LAYERS)
        .max(limits.max_texture_array_layers);
    limits
}

/// Acquires the first compatible [`wgpu::Adapter`] whose `request_device` succeeds,
/// preferring discrete GPUs first and automatically failing over across integrated/software
/// adapters when the primary GPU driver reports device loss or resource exhaustion.
pub fn request_working_adapter_and_device(
    instance: &wgpu::Instance,
    compatible_surface: Option<&wgpu::Surface<'_>>,
    label: &str,
) -> Result<(wgpu::Adapter, wgpu::Device, wgpu::Queue), String> {
    let mut candidates: Vec<wgpu::Adapter> = Vec::new();
    let mut seen: Vec<(u32, u32, wgpu::DeviceType, wgpu::Backend, String)> = Vec::new();
    let mut push_unique = |adapter: wgpu::Adapter| {
        if let Some(surface) = compatible_surface {
            if !adapter.is_surface_supported(surface) {
                return;
            }
        }
        let info = adapter.get_info();
        let key = (
            info.vendor,
            info.device,
            info.device_type,
            info.backend,
            info.name,
        );
        if !seen.contains(&key) {
            seen.push(key);
            candidates.push(adapter);
        }
    };

    if let Some(preferred) = pollster::block_on(instance.request_adapter(
        &wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface,
            force_fallback_adapter: false,
        },
    )) {
        push_unique(preferred);
    }

    let mut enumerated: Vec<wgpu::Adapter> = instance
        .enumerate_adapters(wgpu::Backends::all())
        .into_iter()
        .collect();
    enumerated.sort_by_key(|a| {
        let info = a.get_info();
        let dev_rank = match info.device_type {
            wgpu::DeviceType::DiscreteGpu => 0u8,
            wgpu::DeviceType::IntegratedGpu => 1,
            wgpu::DeviceType::VirtualGpu => 2,
            wgpu::DeviceType::Cpu => 3,
            wgpu::DeviceType::Other => 4,
        };
        let backend_rank = match info.backend {
            wgpu::Backend::Vulkan => 0u8,
            wgpu::Backend::Metal | wgpu::Backend::Dx12 => 1,
            wgpu::Backend::Gl => 2,
            _ => 3,
        };
        (dev_rank, backend_rank)
    });
    for adapter in enumerated {
        push_unique(adapter);
    }

    if candidates.is_empty() {
        return Err(if compatible_surface.is_some() {
            "no compatible wgpu adapter for window surface".to_string()
        } else {
            "no wgpu adapter available".to_string()
        });
    }

    let mut errors = Vec::new();
    for adapter in candidates {
        let info = adapter.get_info();
        let required_limits = preferred_wgpu_limits(&adapter);
        match pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some(label),
                required_features: wgpu::Features::empty(),
                required_limits,
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        )) {
            Ok((device, queue)) => return Ok((adapter, device, queue)),
            Err(e) => {
                errors.push(format!("{} ({:?}/{:?}): {e}", info.name, info.backend, info.device_type));
            }
        }
    }

    Err(format!("request_device failed: {}", errors.join("; ")))
}

impl Wgpu3DRenderer {
    /// Creates a headless `Wgpu3DRenderer` using any available hardware or software Vulkan/GL adapter.
    pub fn new_headless(color_format: wgpu::TextureFormat) -> Result<Self, String> {
        let instance = wgpu::Instance::default();
        let (_adapter, device, queue) =
            request_working_adapter_and_device(&instance, None, "strata_render_wgpu_device")?;
        Ok(Self::from_device(
            Arc::new(device),
            Arc::new(queue),
            color_format,
        ))
    }

    /// Creates a `Wgpu3DRenderer` bound to an existing `wgpu::Device` & `wgpu::Queue` (e.g., from a window surface).
    pub fn from_device(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        color_format: wgpu::TextureFormat,
    ) -> Self {
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("strata_3d_camera_uniforms"),
            size: std::mem::size_of::<GpuCameraUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let initial_instances = 1024usize;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("strata_3d_tile_instances"),
            size: (initial_instances * std::mem::size_of::<GpuTileInstance>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let slab = GpuTileSlabAllocator::new(&device, 262_144);
        let atlas_layers = MAX_ATLAS_LAYERS.min(device.limits().max_texture_array_layers.max(1));
        let atlas = GpuImageryAtlas::new(&device, &queue, atlas_layers);

        let terrain_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("strata_3d_terrain_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        let env_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("strata_3d_env_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let land_mask_view = create_rgba8_2d_view(
            &device,
            &queue,
            "strata_env_land_fallback",
            1,
            1,
            &[255, 255, 255, 255],
        );
        let cloud_map_view = create_rgba8_2d_view(
            &device,
            &queue,
            "strata_env_cloud_fallback",
            1,
            1,
            &[255, 255, 255, 0],
        );
        let moon_view = create_rgba8_2d_view(
            &device,
            &queue,
            "strata_env_moon_fallback",
            1,
            1,
            &[190, 190, 190, 255],
        );
        let black_face = Arc::new(vec![0u8, 0, 0, 0]);
        let skybox_cube_view = create_rgba8_cube_view(
            &device,
            &queue,
            "strata_env_skybox_fallback",
            1,
            &[
                Arc::clone(&black_face),
                Arc::clone(&black_face),
                Arc::clone(&black_face),
                Arc::clone(&black_face),
                Arc::clone(&black_face),
                black_face,
            ],
            wgpu::TextureFormat::Rgba8Unorm,
        );
        let star_capacity = 64usize;
        let star_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("strata_3d_stars"),
            size: (star_capacity * std::mem::size_of::<GpuStar>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let env_state = GpuEnvironmentState {
            land_mask_id_rev: None,
            cloud_map_id_rev: None,
            moon_tex_id_rev: None,
            skybox_cube_id_rev: None,
            stars_ptr: 0,
            stars_len: 0,
            land_mask_view,
            cloud_map_view,
            moon_view,
            skybox_cube_view,
            env_sampler,
            star_buffer,
            star_capacity,
            star_count: 0,
        };

        // Sky + Moon + Sun + Milky Way pass
        let sky_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("strata_3d_sky_wgsl"),
            source: wgpu::ShaderSource::Wgsl(SKY_WGSL.into()),
        });
        let sky_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("strata_3d_sky_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::Cube,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });
        let sky_bind_group = Self::create_sky_bind_group(
            &device,
            &sky_bind_group_layout,
            &camera_buffer,
            &env_state.moon_view,
            &env_state.env_sampler,
            &env_state.skybox_cube_view,
        );
        let sky_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("strata_3d_sky_pl"),
            bind_group_layouts: &[&sky_bind_group_layout],
            push_constant_ranges: &[],
        });
        let sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("strata_3d_sky_pipeline"),
            layout: Some(&sky_layout),
            vertex: wgpu::VertexState {
                module: &sky_shader,
                entry_point: Some("vs_sky"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &sky_shader,
                entry_point: Some("fs_sky"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Scientific J2000 starfield pass
        let star_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("strata_3d_stars_wgsl"),
            source: wgpu::ShaderSource::Wgsl(STARS_WGSL.into()),
        });
        let star_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("strata_3d_stars_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });
        let star_bind_group = Self::create_star_bind_group(
            &device,
            &star_bind_group_layout,
            &camera_buffer,
            &env_state.star_buffer,
        );
        let star_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("strata_3d_stars_pl"),
            bind_group_layouts: &[&star_bind_group_layout],
            push_constant_ranges: &[],
        });
        let star_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("strata_3d_star_pipeline"),
            layout: Some(&star_layout),
            vertex: wgpu::VertexState {
                module: &star_shader,
                entry_point: Some("vs_star"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &star_shader,
                entry_point: Some("fs_star"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // 3D global cloud shell pipeline + static icosphere
        let cloud_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("strata_3d_cloud_shell_wgsl"),
            source: wgpu::ShaderSource::Wgsl(CLOUD_SHELL_WGSL.into()),
        });
        let cloud_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("strata_3d_cloud_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let cloud_bind_group = Self::create_tex2d_bind_group(
            &device,
            &cloud_bind_group_layout,
            "strata_3d_cloud_bg",
            &camera_buffer,
            &env_state.cloud_map_view,
            &env_state.env_sampler,
        );
        let cloud_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("strata_3d_cloud_pl"),
            bind_group_layouts: &[&cloud_bind_group_layout],
            push_constant_ranges: &[],
        });
        let cloud_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("strata_3d_cloud_pipeline"),
            layout: Some(&cloud_layout),
            vertex: wgpu::VertexState {
                module: &cloud_shader,
                entry_point: Some("vs_cloud"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<RteTileVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 16,
                            shader_location: 1,
                        },
                    ],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &cloud_shader,
                entry_point: Some("fs_cloud"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                front_face: wgpu::FrontFace::Ccw,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let (ico_v, ico_i) = build_unit_icosphere(6);
        let cloud_vbuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("strata_3d_cloud_vb"),
            size: (ico_v.len() * std::mem::size_of::<RteTileVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&cloud_vbuf, 0, bytemuck::cast_slice(&ico_v));
        let cloud_ibuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("strata_3d_cloud_ib"),
            size: (ico_i.len() * 4) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&cloud_ibuf, 0, bytemuck::cast_slice(&ico_i));
        let cloud_index_count = ico_i.len() as u32;

        // Terrain vertex-pulling pipeline
        let terrain_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("strata_3d_terrain_pull_wgsl"),
            source: wgpu::ShaderSource::Wgsl(TERRAIN_PULL_WGSL.into()),
        });
        let terrain_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("strata_3d_terrain_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });
        let terrain_bind_group = Self::create_terrain_bind_group(
            &device,
            &terrain_bind_group_layout,
            &camera_buffer,
            &slab.buffer,
            &instance_buffer,
            &atlas.view,
            &terrain_sampler,
            &env_state.land_mask_view,
        );
        let terrain_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("strata_3d_terrain_pl"),
            bind_group_layouts: &[&terrain_bind_group_layout],
            push_constant_ranges: &[],
        });
        let terrain_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("strata_3d_terrain_pipeline"),
            layout: Some(&terrain_layout),
            vertex: wgpu::VertexState {
                module: &terrain_shader,
                entry_point: Some("vs_terrain"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &terrain_shader,
                entry_point: Some("fs_terrain"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Back),
                front_face: wgpu::FrontFace::Ccw,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Dynamic RTC mesh pipeline (buildings, entities, fallback meshes)
        let dynamic_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("strata_3d_dynamic_wgsl"),
            source: wgpu::ShaderSource::Wgsl(DYNAMIC_MESH_WGSL.into()),
        });
        let align = device.limits().min_uniform_buffer_offset_alignment.max(256);
        let dyn_size = std::mem::size_of::<GpuDynamicUniforms>() as u32;
        let dynamic_uniform_stride = dyn_size.div_ceil(align) * align;
        let dynamic_uniform_capacity = 64usize;
        let dynamic_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("strata_3d_dynamic_uniforms"),
            size: (dynamic_uniform_capacity * dynamic_uniform_stride as usize) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let dynamic_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("strata_3d_dynamic_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: true,
                            min_binding_size: std::num::NonZeroU64::new(std::mem::size_of::<
                                GpuDynamicUniforms,
                            >(
                            )
                                as u64),
                        },
                        count: None,
                    },
                ],
            });
        let dynamic_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("strata_3d_dynamic_bg"),
            layout: &dynamic_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &dynamic_uniform_buffer,
                        offset: 0,
                        size: std::num::NonZeroU64::new(
                            std::mem::size_of::<GpuDynamicUniforms>() as u64
                        ),
                    }),
                },
            ],
        });
        let dynamic_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("strata_3d_dynamic_pl"),
            bind_group_layouts: &[&dynamic_bind_group_layout],
            push_constant_ranges: &[],
        });
        let dynamic_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("strata_3d_dynamic_pipeline"),
            layout: Some(&dynamic_layout),
            vertex: wgpu::VertexState {
                module: &dynamic_shader,
                entry_point: Some("vs_dynamic"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<RteTileVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 16,
                            shader_location: 1,
                        },
                    ],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &dynamic_shader,
                entry_point: Some("fs_dynamic"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                front_face: wgpu::FrontFace::Ccw,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // 2D overlay composite pipeline
        let overlay_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("strata_2d_overlay_wgsl"),
            source: wgpu::ShaderSource::Wgsl(OVERLAY_COMPOSITE_WGSL.into()),
        });
        let overlay_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("strata_2d_overlay_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let overlay_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("strata_2d_overlay_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let overlay_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("strata_2d_overlay_pl"),
            bind_group_layouts: &[&overlay_bind_group_layout],
            push_constant_ranges: &[],
        });
        let overlay_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("strata_2d_overlay_pipeline"),
            layout: Some(&overlay_layout),
            vertex: wgpu::VertexState {
                module: &overlay_shader,
                entry_point: Some("vs_overlay"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &overlay_shader,
                entry_point: Some("fs_overlay"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            device,
            queue,
            color_format,
            camera_buffer,
            sky_pipeline,
            sky_bind_group_layout,
            sky_bind_group,
            star_pipeline,
            star_bind_group_layout,
            star_bind_group,
            cloud_pipeline,
            cloud_bind_group_layout,
            cloud_bind_group,
            cloud_vbuf,
            cloud_ibuf,
            cloud_index_count,
            env_state,
            terrain_pipeline,
            terrain_bind_group_layout,
            terrain_bind_group,
            terrain_sampler,
            instance_buffer,
            instance_capacity: initial_instances,
            slab,
            atlas,
            topologies: HashMap::new(),
            dynamic_pipeline,
            dynamic_bind_group_layout,
            dynamic_uniform_buffer,
            dynamic_uniform_capacity,
            dynamic_uniform_stride,
            dynamic_bind_group,
            dynamic_meshes: HashMap::new(),
            overlay_pipeline,
            overlay_bind_group_layout,
            overlay_sampler,
            depth_target: None,
            offscreen: None,
            frame_index: 0,
            last_geometry_rev: 0,
            last_terrain_frame: None,
            last_scene_ptr: 0,
            cached_fallback_frame: None,
        }
    }

    pub fn device(&self) -> &Arc<wgpu::Device> {
        &self.device
    }

    pub fn queue(&self) -> &Arc<wgpu::Queue> {
        &self.queue
    }

    pub fn color_format(&self) -> wgpu::TextureFormat {
        self.color_format
    }

    fn create_tex2d_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        label: &str,
        camera_buffer: &wgpu::Buffer,
        tex_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(tex_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    fn create_sky_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        camera_buffer: &wgpu::Buffer,
        moon_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        skybox_cube_view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("strata_3d_sky_bg"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(moon_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(skybox_cube_view),
                },
            ],
        })
    }

    fn create_star_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        camera_buffer: &wgpu::Buffer,
        star_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("strata_3d_stars_bg"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: star_buffer.as_entire_binding(),
                },
            ],
        })
    }

    fn create_terrain_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        camera_buffer: &wgpu::Buffer,
        slab_buffer: &wgpu::Buffer,
        instance_buffer: &wgpu::Buffer,
        atlas_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        land_mask_view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("strata_3d_terrain_bg"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: slab_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: instance_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(land_mask_view),
                },
            ],
        })
    }

    fn upload_environment_assets(&mut self, env: &RetainedEnvironmentAssets) -> bool {
        let mut terrain_rebind = false;

        if let Some(ref land) = env.land_mask {
            let key = Some((land.id, land.revision));
            if self.env_state.land_mask_id_rev != key {
                self.env_state.land_mask_view = create_rgba8_2d_view(
                    &self.device,
                    &self.queue,
                    "strata_env_land_mask",
                    land.width,
                    land.height,
                    &land.rgba8,
                );
                self.env_state.land_mask_id_rev = key;
                terrain_rebind = true;
            }
        }

        if let Some(ref clouds) = env.cloud_map {
            let key = Some((clouds.id, clouds.revision));
            if self.env_state.cloud_map_id_rev != key {
                self.env_state.cloud_map_view = create_rgba8_2d_view(
                    &self.device,
                    &self.queue,
                    "strata_env_cloud_map",
                    clouds.width,
                    clouds.height,
                    &clouds.rgba8,
                );
                self.env_state.cloud_map_id_rev = key;
                self.cloud_bind_group = Self::create_tex2d_bind_group(
                    &self.device,
                    &self.cloud_bind_group_layout,
                    "strata_3d_cloud_bg",
                    &self.camera_buffer,
                    &self.env_state.cloud_map_view,
                    &self.env_state.env_sampler,
                );
            }
        }

        let mut sky_rebind = false;

        if let Some(ref moon) = env.moon_texture {
            let key = Some((moon.id, moon.revision));
            if self.env_state.moon_tex_id_rev != key {
                self.env_state.moon_view = create_rgba8_2d_view(
                    &self.device,
                    &self.queue,
                    "strata_env_moon_tex",
                    moon.width,
                    moon.height,
                    &moon.rgba8,
                );
                self.env_state.moon_tex_id_rev = key;
                sky_rebind = true;
            }
        }

        match &env.skybox_cubemap {
            Some(cube) => {
                let key = Some((cube.id, cube.revision));
                if self.env_state.skybox_cube_id_rev != key {
                    self.env_state.skybox_cube_view = create_rgba8_cube_view(
                        &self.device,
                        &self.queue,
                        "strata_env_skybox_cube",
                        cube.face_size,
                        &cube.faces_rgba8,
                        wgpu::TextureFormat::Rgba8Unorm,
                    );
                    self.env_state.skybox_cube_id_rev = key;
                    sky_rebind = true;
                }
            }
            None => {
                if self.env_state.skybox_cube_id_rev.is_some() {
                    let black_face = Arc::new(vec![0u8, 0, 0, 0]);
                    self.env_state.skybox_cube_view = create_rgba8_cube_view(
                        &self.device,
                        &self.queue,
                        "strata_env_skybox_fallback",
                        1,
                        &[
                            Arc::clone(&black_face),
                            Arc::clone(&black_face),
                            Arc::clone(&black_face),
                            Arc::clone(&black_face),
                            Arc::clone(&black_face),
                            black_face,
                        ],
                        wgpu::TextureFormat::Rgba8Unorm,
                    );
                    self.env_state.skybox_cube_id_rev = None;
                    sky_rebind = true;
                }
            }
        }

        if sky_rebind {
            self.sky_bind_group = Self::create_sky_bind_group(
                &self.device,
                &self.sky_bind_group_layout,
                &self.camera_buffer,
                &self.env_state.moon_view,
                &self.env_state.env_sampler,
                &self.env_state.skybox_cube_view,
            );
        }

        if let Some(ref stars) = env.stars {
            let ptr = Arc::as_ptr(stars) as usize;
            let len = stars.len();
            if self.env_state.stars_ptr != ptr || self.env_state.stars_len != len {
                let gpu_stars: Vec<GpuStar> = stars
                    .iter()
                    .map(|s: &RetainedStar| GpuStar {
                        icrf_dir_mag: [s.icrf_dir[0], s.icrf_dir[1], s.icrf_dir[2], s.vmag],
                        color: s.color,
                    })
                    .collect();
                if gpu_stars.len() > self.env_state.star_capacity {
                    let new_cap = gpu_stars.len().next_power_of_two().max(64);
                    self.env_state.star_buffer =
                        self.device.create_buffer(&wgpu::BufferDescriptor {
                            label: Some("strata_3d_stars_grown"),
                            size: (new_cap * std::mem::size_of::<GpuStar>()) as u64,
                            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                            mapped_at_creation: false,
                        });
                    self.env_state.star_capacity = new_cap;
                    self.star_bind_group = Self::create_star_bind_group(
                        &self.device,
                        &self.star_bind_group_layout,
                        &self.camera_buffer,
                        &self.env_state.star_buffer,
                    );
                }
                if !gpu_stars.is_empty() {
                    self.queue.write_buffer(
                        &self.env_state.star_buffer,
                        0,
                        bytemuck::cast_slice(&gpu_stars),
                    );
                }
                self.env_state.stars_ptr = ptr;
                self.env_state.stars_len = len;
                self.env_state.star_count = gpu_stars.len() as u32;
            }
        }

        terrain_rebind
    }

    fn ensure_topology(&mut self, grid_res: u32, has_skirts: bool) -> (wgpu::Buffer, u32) {
        if let Some(cached) = self.topologies.get(&(grid_res, has_skirts)) {
            return (cached.buffer.clone(), cached.index_count);
        }
        let indices = build_terrain_grid_indices(grid_res, has_skirts);
        let count = indices.len() as u32;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("strata_3d_grid_topology"),
            size: (indices.len() * 4) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !indices.is_empty() {
            self.queue
                .write_buffer(&buffer, 0, bytemuck::cast_slice(&indices));
        }
        self.topologies.insert(
            (grid_res, has_skirts),
            CachedIndexTopology {
                buffer: buffer.clone(),
                index_count: count,
            },
        );
        (buffer, count)
    }

    fn ensure_depth_view(&mut self, width: u32, height: u32) -> wgpu::TextureView {
        if let Some((w, h, view)) = &self.depth_target {
            if *w == width && *h == height {
                return view.clone();
            }
        }
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("strata_3d_depth"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        self.depth_target = Some((width, height, view.clone()));
        view
    }

    /// Creates an RGBA `wgpu::BindGroup` for compositing a 2D UI HUD texture over a 3D frame.
    pub fn create_overlay_bind_group(&self, overlay_view: &wgpu::TextureView) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("strata_2d_overlay_bg"),
            layout: &self.overlay_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(overlay_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.overlay_sampler),
                },
            ],
        })
    }

    /// Draws `scene` (or its [`RetainedScene3D`] / [`RetainedTerrainFrame`]) directly into `target_view`.
    ///
    /// When `viewport` is `Some([x, y, w, h])`, restricts rendering to that sub-rectangle of the
    /// render target and uses `w/h` as the projection aspect ratio.
    pub fn render_scene_to_view(
        &mut self,
        scene: &Scene3D,
        target_view: &wgpu::TextureView,
        target_width: u32,
        target_height: u32,
        viewport: Option<[f32; 4]>,
        unlimited_upload_budget: bool,
    ) {
        self.frame_index += 1;
        let frame_idx = self.frame_index;
        let mesh_budget = if unlimited_upload_budget || frame_idx <= 2 {
            usize::MAX
        } else {
            DEFAULT_MESH_UPLOAD_BUDGET
        };
        let tex_budget = if unlimited_upload_budget || frame_idx <= 2 {
            usize::MAX
        } else {
            DEFAULT_TEXTURE_UPLOAD_BUDGET
        };

        let (cam, terrain_frame_opt) = if let Some(retained) = scene.retained_handle() {
            let terrain = retained.snapshot_terrain();
            let cam = retained.camera();
            (cam, terrain)
        } else if let Some(terrain) = scene.retained_terrain() {
            (terrain.camera, Some(Arc::clone(terrain)))
        } else {
            let scene_ptr = scene as *const Scene3D as usize;
            if scene_ptr != self.last_scene_ptr || self.cached_fallback_frame.is_none() {
                self.last_scene_ptr = scene_ptr;
                self.cached_fallback_frame = Some(Arc::new(fallback_frame_from_scene3d(
                    scene,
                    target_width.max(1) as f32,
                    target_height.max(1) as f32,
                )));
            }
            let frame = self.cached_fallback_frame.clone().unwrap();
            (frame.camera, Some(frame))
        };

        // 1. Update camera uniform buffer unconditionally (O(1) tiny uniform write).
        let vp_w = viewport
            .map(|v| v[2])
            .unwrap_or_else(|| target_width.max(1) as f32)
            .max(1.0);
        let vp_h = viewport
            .map(|v| v[3])
            .unwrap_or_else(|| target_height.max(1) as f32)
            .max(1.0);
        let gpu_cam = GpuCameraUniforms::from_retained(&cam, vp_w, vp_h);
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&gpu_cam));

        // 2. Upload incremental tile meshes/textures, environment assets, and prepare draw instances.
        let mut draw_count = 0usize;
        let mut grid_topology: Option<(u32, bool)> = None;
        let mut dynamic_draws: Vec<(u64, [f64; 3], [f32; 4], bool)> = Vec::new();

        if let Some(frame) = terrain_frame_opt.as_ref() {
            let frame_changed = frame.geometry_revision != self.last_geometry_rev
                || self
                    .last_terrain_frame
                    .as_ref()
                    .map(|p| !Arc::ptr_eq(p, frame))
                    .unwrap_or(true);
            let _ = frame_changed;
            self.last_geometry_rev = frame.geometry_revision;
            self.last_terrain_frame = Some(Arc::clone(frame));

            let env_dirty = if let Some(ref env) = frame.environment {
                self.upload_environment_assets(env)
            } else {
                false
            };

            let bg_dirty = self.slab.upload_meshes(
                &self.device,
                &self.queue,
                &frame.meshes,
                frame_idx,
                mesh_budget,
            );
            self.atlas
                .upload_textures(&self.queue, &frame.textures, frame_idx, tex_budget);

            let mut instances_gpu: Vec<GpuTileInstance> = Vec::with_capacity(frame.instances.len());
            for inst in &frame.instances {
                let Some(base_v) = self.slab.base_vertex(inst.key, inst.mesh_revision) else {
                    continue;
                };
                if grid_topology.is_none() {
                    if let Some(m) = frame.meshes.iter().find(|m| m.key == inst.key) {
                        grid_topology = Some((m.grid_res, m.has_skirts));
                    }
                }
                let (cx_hi, cx_lo) = split_f64_to_f32(inst.center_ecef[0]);
                let (cy_hi, cy_lo) = split_f64_to_f32(inst.center_ecef[1]);
                let (cz_hi, cz_lo) = split_f64_to_f32(inst.center_ecef[2]);
                let tex_layer = self.atlas.layer_for(inst.texture_id, inst.texture_revision);
                let night_layer = self
                    .atlas
                    .layer_for(inst.night_texture_id, inst.night_texture_revision);
                let eff_tint = if inst.texture_id.is_some() && tex_layer == 0 {
                    [0.14, 0.19, 0.27, 1.0]
                } else {
                    inst.tint
                };
                instances_gpu.push(GpuTileInstance {
                    center_high: [cx_hi, cy_hi, cz_hi, 0.0],
                    center_low: [cx_lo, cy_lo, cz_lo, 0.0],
                    uv_rect: inst.uv_rect,
                    night_uv_rect: inst.night_uv_rect,
                    tint: eff_tint,
                    tile_meta: [base_v, tex_layer, u32::from(inst.unlit), night_layer],
                });
            }

            let mut instance_resized = false;
            if instances_gpu.len() > self.instance_capacity {
                let new_cap = instances_gpu.len().next_power_of_two().max(1024);
                self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("strata_3d_tile_instances_grown"),
                    size: (new_cap * std::mem::size_of::<GpuTileInstance>()) as u64,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.instance_capacity = new_cap;
                instance_resized = true;
            }
            if bg_dirty || instance_resized || env_dirty {
                self.terrain_bind_group = Self::create_terrain_bind_group(
                    &self.device,
                    &self.terrain_bind_group_layout,
                    &self.camera_buffer,
                    &self.slab.buffer,
                    &self.instance_buffer,
                    &self.atlas.view,
                    &self.terrain_sampler,
                    &self.env_state.land_mask_view,
                );
            }
            if !instances_gpu.is_empty() {
                self.queue.write_buffer(
                    &self.instance_buffer,
                    0,
                    bytemuck::cast_slice(&instances_gpu),
                );
                draw_count = instances_gpu.len();
            }

            for dm in &frame.dynamic_meshes {
                if dm.vertices.is_empty() || dm.indices.is_empty() {
                    continue;
                }
                let vb_bytes: &[u8] = bytemuck::cast_slice(&dm.vertices);
                let ib_bytes: &[u8] = bytemuck::cast_slice(&dm.indices);
                let mut need_recreate = true;
                if let Some(existing) = self.dynamic_meshes.get_mut(&dm.id) {
                    if existing.revision == dm.revision {
                        need_recreate = false;
                    } else if existing.vbuf.size() >= vb_bytes.len() as u64
                        && existing.ibuf.size() >= ib_bytes.len() as u64
                    {
                        self.queue.write_buffer(&existing.vbuf, 0, vb_bytes);
                        self.queue.write_buffer(&existing.ibuf, 0, ib_bytes);
                        existing.revision = dm.revision;
                        existing.index_count = dm.indices.len() as u32;
                        need_recreate = false;
                    }
                }
                if need_recreate {
                    let vbuf = self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("strata_3d_dyn_vb"),
                        size: (vb_bytes.len() as u64).next_power_of_two().max(256),
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    self.queue.write_buffer(&vbuf, 0, vb_bytes);
                    let ibuf = self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("strata_3d_dyn_ib"),
                        size: (ib_bytes.len() as u64).next_power_of_two().max(64),
                        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    self.queue.write_buffer(&ibuf, 0, ib_bytes);
                    self.dynamic_meshes.insert(
                        dm.id,
                        CachedDynamicGpuMesh {
                            revision: dm.revision,
                            vbuf,
                            ibuf,
                            index_count: dm.indices.len() as u32,
                        },
                    );
                }
                dynamic_draws.push((dm.id, dm.anchor_ecef, dm.color, dm.unlit));
            }
        }

        if !dynamic_draws.is_empty() {
            let stride = self.dynamic_uniform_stride as usize;
            if dynamic_draws.len() > self.dynamic_uniform_capacity {
                let new_cap = dynamic_draws.len().next_power_of_two().max(64);
                self.dynamic_uniform_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("strata_3d_dynamic_uniforms_grown"),
                    size: (new_cap * stride) as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.dynamic_uniform_capacity = new_cap;
                self.dynamic_bind_group =
                    self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("strata_3d_dynamic_bg_grown"),
                        layout: &self.dynamic_bind_group_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: self.camera_buffer.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                    buffer: &self.dynamic_uniform_buffer,
                                    offset: 0,
                                    size: std::num::NonZeroU64::new(std::mem::size_of::<
                                        GpuDynamicUniforms,
                                    >(
                                    )
                                        as u64),
                                }),
                            },
                        ],
                    });
            }
            let mut staged = vec![0u8; dynamic_draws.len() * stride];
            for (i, (_, anchor, color, unlit)) in dynamic_draws.iter().enumerate() {
                let (ax_hi, ax_lo) = split_f64_to_f32(anchor[0]);
                let (ay_hi, ay_lo) = split_f64_to_f32(anchor[1]);
                let (az_hi, az_lo) = split_f64_to_f32(anchor[2]);
                let dyn_u = GpuDynamicUniforms {
                    anchor_high: [ax_hi, ay_hi, az_hi, 0.0],
                    anchor_low: [ax_lo, ay_lo, az_lo, 0.0],
                    color: *color,
                    dyn_meta: [u32::from(*unlit), 0, 0, 0],
                };
                let bytes = bytemuck::bytes_of(&dyn_u);
                staged[i * stride..i * stride + bytes.len()].copy_from_slice(bytes);
            }
            self.queue
                .write_buffer(&self.dynamic_uniform_buffer, 0, &staged);
        }

        let topology_res = grid_topology.map(|(g, s)| self.ensure_topology(g, s));
        let depth_view = self.ensure_depth_view(target_width.max(1), target_height.max(1));

        let clear_op = match cam.background {
            Background3D::Transparent => wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            Background3D::Solid(c) => wgpu::LoadOp::Clear(wgpu::Color {
                r: c.r() as f64,
                g: c.g() as f64,
                b: c.b() as f64,
                a: c.a() as f64,
            }),
            Background3D::VerticalGradient { .. } => wgpu::LoadOp::Clear(wgpu::Color::BLACK),
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("strata_3d_render_encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("strata_3d_main_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: clear_op,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            if let Some([vx, vy, vw, vh]) = viewport {
                let x = vx.max(0.0).min((target_width.saturating_sub(1)) as f32);
                let y = vy.max(0.0).min((target_height.saturating_sub(1)) as f32);
                let max_w = (target_width as f32 - x).max(1.0);
                let max_h = (target_height as f32 - y).max(1.0);
                let w = vw.max(1.0).min(max_w);
                let h = vh.max(1.0).min(max_h);
                let xi = (x.floor() as u32).min(target_width.saturating_sub(1));
                let yi = (y.floor() as u32).min(target_height.saturating_sub(1));
                let wi = (w.round() as u32)
                    .max(1)
                    .min(target_width.saturating_sub(xi).max(1));
                let hi = (h.round() as u32)
                    .max(1)
                    .min(target_height.saturating_sub(yi).max(1));
                pass.set_viewport(x, y, w, h, 0.0, 1.0);
                pass.set_scissor_rect(xi, yi, wi, hi);
            }

            if matches!(cam.background, Background3D::VerticalGradient { .. })
                || cam.stars_visible
                || cam.sun_visible
                || cam.moon_visible
            {
                pass.set_pipeline(&self.sky_pipeline);
                pass.set_bind_group(0, &self.sky_bind_group, &[]);
                pass.draw(0..3, 0..1);
            }

            if cam.stars_visible
                && cam.stars_intensity > 0.001
                && self.env_state.star_count > 0
                && cam.local_25d_blend < 0.85
            {
                pass.set_pipeline(&self.star_pipeline);
                pass.set_bind_group(0, &self.star_bind_group, &[]);
                pass.draw(0..6, 0..self.env_state.star_count);
            }

            if draw_count > 0 {
                if let Some((ref idx_buf, idx_count)) = topology_res {
                    if idx_count > 0 {
                        pass.set_pipeline(&self.terrain_pipeline);
                        pass.set_bind_group(0, &self.terrain_bind_group, &[]);
                        pass.set_index_buffer(idx_buf.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..idx_count, 0, 0..draw_count as u32);
                    }
                }
            }

            if (cam.cloud_layer_opacity > 0.004 || cam.local_cloud_opacity > 0.004)
                && self.cloud_index_count > 0
            {
                pass.set_pipeline(&self.cloud_pipeline);
                pass.set_bind_group(0, &self.cloud_bind_group, &[]);
                pass.set_vertex_buffer(0, self.cloud_vbuf.slice(..));
                pass.set_index_buffer(self.cloud_ibuf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.cloud_index_count, 0, 0..1);
            }

            if !dynamic_draws.is_empty() {
                pass.set_pipeline(&self.dynamic_pipeline);
                for (i, (id, _, _, _)) in dynamic_draws.iter().enumerate() {
                    let Some(gpu_mesh) = self.dynamic_meshes.get(id) else {
                        continue;
                    };
                    let offset = (i as u32) * self.dynamic_uniform_stride;
                    pass.set_bind_group(0, &self.dynamic_bind_group, &[offset]);
                    pass.set_vertex_buffer(0, gpu_mesh.vbuf.slice(..));
                    pass.set_index_buffer(gpu_mesh.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..gpu_mesh.index_count, 0, 0..1);
                }
            }
        }
        self.queue.submit(Some(encoder.finish()));
    }

    /// Composites a GPU 2D UI HUD overlay bind group on top of `target_view` (preserving alpha).
    pub fn composite_overlay_to_view(
        &self,
        target_view: &wgpu::TextureView,
        overlay_bind_group: &wgpu::BindGroup,
    ) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("strata_2d_overlay_encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("strata_2d_overlay_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.overlay_pipeline);
            pass.set_bind_group(0, overlay_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
    }

    /// Renders `scene` into a persistent offscreen texture and performs GPU->CPU readback only
    /// when `readback` is `true` (pure GPU timing benchmarks pass `readback = false`).
    pub fn render_offscreen(
        &mut self,
        scene: &Scene3D,
        width: u32,
        height: u32,
        readback: bool,
        unlimited_upload_budget: bool,
    ) -> Result<Vec<u8>, String> {
        let w = width.max(1);
        let h = height.max(1);
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let unpadded = w * 4;
        let padded = unpadded.div_ceil(align) * align;

        let recreate = self
            .offscreen
            .as_ref()
            .map(|o| o.width != w || o.height != h || o.format != self.color_format)
            .unwrap_or(true);
        if recreate {
            let color = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("strata_3d_offscreen_color"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.color_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let color_view = color.create_view(&Default::default());
            let depth = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("strata_3d_offscreen_depth"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let depth_view = depth.create_view(&Default::default());
            let rb = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("strata_3d_offscreen_readback"),
                size: (padded * h) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            self.offscreen = Some(OffscreenTarget {
                width: w,
                height: h,
                format: self.color_format,
                color,
                color_view,
                _depth: depth,
                depth_view,
                readback: rb,
                padded_bytes_per_row: padded,
            });
        }

        let target_view = self.offscreen.as_ref().unwrap().color_view.clone();
        self.depth_target = Some((w, h, self.offscreen.as_ref().unwrap().depth_view.clone()));
        self.render_scene_to_view(scene, &target_view, w, h, None, unlimited_upload_budget);

        if !readback {
            self.device.poll(wgpu::Maintain::Wait);
            return Ok(Vec::new());
        }

        let off = self.offscreen.as_ref().unwrap();
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("strata_3d_readback_encoder"),
            });
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &off.color,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &off.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(off.padded_bytes_per_row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(enc.finish()));

        let slice = off.readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx.send(res);
        });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .map_err(|e| format!("readback recv error: {e}"))?
            .map_err(|e| format!("readback map error: {e:?}"))?;

        let data = slice.get_mapped_range();
        let mut out = vec![0u8; (w * h * 4) as usize];
        for row in 0..h as usize {
            let src_start = row * off.padded_bytes_per_row as usize;
            let dst_start = row * (w as usize * 4);
            out[dst_start..dst_start + w as usize * 4]
                .copy_from_slice(&data[src_start..src_start + w as usize * 4]);
        }
        drop(data);
        off.readback.unmap();
        Ok(out)
    }
}

/// Converts a regular [`Scene3D`] (primitives/custom meshes) into a [`RetainedTerrainFrame`]
/// so standard 3D scenes also accelerate seamlessly through [`Wgpu3DRenderer`].
pub fn fallback_frame_from_scene3d(
    scene: &Scene3D,
    viewport_w: f32,
    viewport_h: f32,
) -> RetainedTerrainFrame {
    let camera = retained_camera_from_scene3d(scene, viewport_w, viewport_h);
    let mut dynamic_meshes = Vec::new();
    for (idx, mesh) in scene.meshes.iter().enumerate() {
        if let Some((verts, inds, color, unlit)) = mesh3d_to_rte(mesh) {
            dynamic_meshes.push(RetainedDynamicMesh {
                id: idx as u64 + 1,
                revision: 1,
                anchor_ecef: [
                    mesh.transform.position[0] as f64,
                    mesh.transform.position[1] as f64,
                    mesh.transform.position[2] as f64,
                ],
                vertices: Arc::new(verts),
                indices: Arc::new(inds),
                color,
                unlit,
            });
        }
    }
    RetainedTerrainFrame {
        camera,
        geometry_revision: 1,
        meshes: Vec::new(),
        instances: Vec::new(),
        textures: Vec::new(),
        dynamic_meshes,
        environment: None,
    }
}

fn retained_camera_from_scene3d(
    scene: &Scene3D,
    width: f32,
    height: f32,
) -> RetainedCameraUniforms {
    let aspect = (width.max(1.0) / height.max(1.0)).max(0.01);
    let (eye, target, proj) = match scene.camera {
        Camera3D::Perspective {
            fov_horizontal_deg,
            near,
            far,
            position,
            target,
        } => {
            let hfov_rad = fov_horizontal_deg.to_radians().max(0.05);
            let half_w = (hfov_rad * 0.5).tan();
            let vfov_rad = 2.0 * (half_w / aspect).atan();
            (
                position,
                target,
                perspective_rh_zo(vfov_rad, aspect, near.max(0.01), far.max(near + 1.0)),
            )
        }
        Camera3D::Orthographic {
            left,
            right,
            top,
            bottom,
            near,
            far,
            position,
            target,
        } => (
            position,
            target,
            ortho_rh_zo(left, right, bottom, top, near, far),
        ),
    };

    let up_hint = scene.camera_up().unwrap_or([0.0, 1.0, 0.0]);
    let view_rot = look_rot_rh(eye, target, up_hint);
    let view_proj_rte = mul_mat4(proj, view_rot);

    let mut sun_dir = [0.577_350_26f32, 0.577_350_26, 0.577_350_26];
    let mut sun_color = [1.0f32, 0.98, 0.95];
    let mut ambient_color = [0.22f32, 0.24, 0.28];
    for light in &scene.lights {
        match light {
            Light3D::Ambient { color, intensity } => {
                ambient_color = [
                    color.r() * intensity,
                    color.g() * intensity,
                    color.b() * intensity,
                ];
            }
            Light3D::Directional {
                position,
                color,
                intensity,
                ..
            } => {
                let len = (position[0] * position[0]
                    + position[1] * position[1]
                    + position[2] * position[2])
                    .sqrt()
                    .max(1e-6);
                sun_dir = [position[0] / len, position[1] / len, position[2] / len];
                sun_color = [
                    color.r() * intensity,
                    color.g() * intensity,
                    color.b() * intensity,
                ];
            }
            Light3D::Point { .. } => {}
        }
    }

    RetainedCameraUniforms {
        proj,
        view_rot,
        view_proj_rte,
        eye_ecef: [eye[0] as f64, eye[1] as f64, eye[2] as f64],
        sun_dir,
        sun_color,
        ambient_color,
        background: scene.background(),
        near: 0.1,
        far: 100_000_000.0,
        ..Default::default()
    }
}

fn perspective_rh_zo(vfov_rad: f32, aspect: f32, near: f32, far: f32) -> [[f32; 4]; 4] {
    let f = 1.0 / (vfov_rad * 0.5).tan();
    let nf = 1.0 / (near - far);
    [
        [f / aspect, 0.0, 0.0, 0.0],
        [0.0, f, 0.0, 0.0],
        [0.0, 0.0, far * nf, -1.0],
        [0.0, 0.0, (far * near) * nf, 0.0],
    ]
}

fn ortho_rh_zo(l: f32, r: f32, b: f32, t: f32, n: f32, f: f32) -> [[f32; 4]; 4] {
    let rml = (r - l).max(1e-5);
    let tmb = (t - b).max(1e-5);
    let fmn = (f - n).max(1e-5);
    [
        [2.0 / rml, 0.0, 0.0, 0.0],
        [0.0, 2.0 / tmb, 0.0, 0.0],
        [0.0, 0.0, -1.0 / fmn, 0.0],
        [-(r + l) / rml, -(t + b) / tmb, -n / fmn, 1.0],
    ]
}

fn look_rot_rh(eye: [f32; 3], target: [f32; 3], up_hint: [f64; 3]) -> [[f32; 4]; 4] {
    let fx = (target[0] - eye[0]) as f64;
    let fy = (target[1] - eye[1]) as f64;
    let fz = (target[2] - eye[2]) as f64;
    let flen = (fx * fx + fy * fy + fz * fz).sqrt().max(1e-9);
    let f = [fx / flen, fy / flen, fz / flen];
    let mut s = [
        f[1] * up_hint[2] - f[2] * up_hint[1],
        f[2] * up_hint[0] - f[0] * up_hint[2],
        f[0] * up_hint[1] - f[1] * up_hint[0],
    ];
    let mut slen = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt();
    if slen < 1e-6 {
        let alt = if f[1].abs() < 0.9 {
            [0.0, 1.0, 0.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        s = [
            f[1] * alt[2] - f[2] * alt[1],
            f[2] * alt[0] - f[0] * alt[2],
            f[0] * alt[1] - f[1] * alt[0],
        ];
        slen = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt().max(1e-9);
    }
    let s = [s[0] / slen, s[1] / slen, s[2] / slen];
    let u = [
        s[1] * f[2] - s[2] * f[1],
        s[2] * f[0] - s[0] * f[2],
        s[0] * f[1] - s[1] * f[0],
    ];
    [
        [s[0] as f32, u[0] as f32, -f[0] as f32, 0.0],
        [s[1] as f32, u[1] as f32, -f[1] as f32, 0.0],
        [s[2] as f32, u[2] as f32, -f[2] as f32, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn mul_mat4(a: [[f32; 4]; 4], b: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut out = [[0.0f32; 4]; 4];
    for c in 0..4 {
        for r in 0..4 {
            out[c][r] =
                a[0][r] * b[c][0] + a[1][r] * b[c][1] + a[2][r] * b[c][2] + a[3][r] * b[c][3];
        }
    }
    out
}

fn mesh3d_to_rte(mesh: &Mesh3D) -> Option<(Vec<RteTileVertex>, Vec<u32>, [f32; 4], bool)> {
    let (color, unlit) = match &mesh.material {
        Material3D::Standard { color, .. } | Material3D::Physical { color, .. } => {
            ([color.r(), color.g(), color.b(), color.a()], false)
        }
        Material3D::Basic { color } => ([color.r(), color.g(), color.b(), color.a()], true),
        Material3D::Textured(tm) => (
            [
                tm.tint.r(),
                tm.tint.g(),
                tm.tint.b(),
                tm.tint.a() * tm.opacity,
            ],
            !tm.lit,
        ),
        Material3D::TexturedBatch(tb) => {
            let tint = tb
                .draws
                .first()
                .map(|d| d.material.tint)
                .unwrap_or(strata_style::Color::WHITE);
            ([tint.r(), tint.g(), tint.b(), tint.a()], false)
        }
        Material3D::Shader { .. } => ([0.8, 0.8, 0.8, 1.0], false),
    };

    let scale = mesh.transform.scale;
    let map_pos = |x: f32, y: f32, z: f32| [x * scale[0], y * scale[1], z * scale[2]];

    match &mesh.geometry {
        Geometry3D::Custom { vertices, indices } => {
            let mut out_v = Vec::with_capacity(vertices.len() / 6);
            for chunk in vertices.chunks_exact(6) {
                out_v.push(RteTileVertex::new(
                    map_pos(chunk[0], chunk[1], chunk[2]),
                    [chunk[3], chunk[4], chunk[5]],
                    [0.0, 0.0],
                ));
            }
            Some((out_v, indices.clone(), color, unlit))
        }
        Geometry3D::CustomTextured { vertices, indices } => {
            let mut out_v = Vec::with_capacity(vertices.len() / 8);
            for chunk in vertices.chunks_exact(8) {
                out_v.push(RteTileVertex::new(
                    map_pos(chunk[0], chunk[1], chunk[2]),
                    [chunk[3], chunk[4], chunk[5]],
                    [chunk[6], chunk[7]],
                ));
            }
            Some((out_v, indices.clone(), color, unlit))
        }
        Geometry3D::Box {
            width,
            height,
            depth,
        } => {
            let hw = width * 0.5;
            let hh = height * 0.5;
            let hd = depth * 0.5;
            let corners = [
                [-hw, -hh, -hd],
                [hw, -hh, -hd],
                [hw, hh, -hd],
                [-hw, hh, -hd],
                [-hw, -hh, hd],
                [hw, -hh, hd],
                [hw, hh, hd],
                [-hw, hh, hd],
            ];
            let faces: [([usize; 4], [f32; 3]); 6] = [
                ([4, 5, 6, 7], [0.0, 0.0, 1.0]),
                ([1, 0, 3, 2], [0.0, 0.0, -1.0]),
                ([5, 1, 2, 6], [1.0, 0.0, 0.0]),
                ([0, 4, 7, 3], [-1.0, 0.0, 0.0]),
                ([7, 6, 2, 3], [0.0, 1.0, 0.0]),
                ([0, 1, 5, 4], [0.0, -1.0, 0.0]),
            ];
            let mut verts = Vec::with_capacity(24);
            let mut inds = Vec::with_capacity(36);
            for (f_idx, (c, n)) in faces.into_iter().enumerate() {
                let b = (f_idx * 4) as u32;
                verts.push(RteTileVertex::new(
                    map_pos(corners[c[0]][0], corners[c[0]][1], corners[c[0]][2]),
                    n,
                    [0.0, 0.0],
                ));
                verts.push(RteTileVertex::new(
                    map_pos(corners[c[1]][0], corners[c[1]][1], corners[c[1]][2]),
                    n,
                    [1.0, 0.0],
                ));
                verts.push(RteTileVertex::new(
                    map_pos(corners[c[2]][0], corners[c[2]][1], corners[c[2]][2]),
                    n,
                    [1.0, 1.0],
                ));
                verts.push(RteTileVertex::new(
                    map_pos(corners[c[3]][0], corners[c[3]][1], corners[c[3]][2]),
                    n,
                    [0.0, 1.0],
                ));
                inds.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
            }
            Some((verts, inds, color, unlit))
        }
        Geometry3D::Plane { width, height } => {
            let hw = width * 0.5;
            let hh = height * 0.5;
            let verts = vec![
                RteTileVertex::new(map_pos(-hw, 0.0, -hh), [0.0, 1.0, 0.0], [0.0, 0.0]),
                RteTileVertex::new(map_pos(hw, 0.0, -hh), [0.0, 1.0, 0.0], [1.0, 0.0]),
                RteTileVertex::new(map_pos(hw, 0.0, hh), [0.0, 1.0, 0.0], [1.0, 1.0]),
                RteTileVertex::new(map_pos(-hw, 0.0, hh), [0.0, 1.0, 0.0], [0.0, 1.0]),
            ];
            Some((verts, vec![0, 1, 2, 0, 2, 3], color, unlit))
        }
        Geometry3D::Sphere {
            radius,
            width_segments,
            height_segments,
        } => {
            let ws = (*width_segments).max(6);
            let hs = (*height_segments).max(4);
            let mut verts = Vec::new();
            let mut inds = Vec::new();
            for iy in 0..=hs {
                let v = iy as f32 / hs as f32;
                let phi = v * std::f32::consts::PI;
                for ix in 0..=ws {
                    let u = ix as f32 / ws as f32;
                    let theta = u * std::f32::consts::TAU;
                    let nx = -phi.sin() * theta.cos();
                    let ny = phi.cos();
                    let nz = phi.sin() * theta.sin();
                    verts.push(RteTileVertex::new(
                        map_pos(nx * radius, ny * radius, nz * radius),
                        [nx, ny, nz],
                        [u, v],
                    ));
                }
            }
            for iy in 0..hs {
                for ix in 0..ws {
                    let a = iy * (ws + 1) + ix;
                    let b = a + 1;
                    let c = (iy + 1) * (ws + 1) + ix;
                    let d = c + 1;
                    inds.extend_from_slice(&[a, c, d, a, d, b]);
                }
            }
            Some((verts, inds, color, unlit))
        }
        Geometry3D::Cylinder {
            radius_top,
            radius_bottom,
            height,
            radial_segments,
        } => {
            let segs = (*radial_segments).max(6);
            let hh = height * 0.5;
            let mut verts = Vec::new();
            let mut inds = Vec::new();
            for i in 0..=segs {
                let u = i as f32 / segs as f32;
                let a = u * std::f32::consts::TAU;
                let (sa, ca) = a.sin_cos();
                verts.push(RteTileVertex::new(
                    map_pos(ca * radius_top, hh, sa * radius_top),
                    [ca, 0.0, sa],
                    [u, 0.0],
                ));
                verts.push(RteTileVertex::new(
                    map_pos(ca * radius_bottom, -hh, sa * radius_bottom),
                    [ca, 0.0, sa],
                    [u, 1.0],
                ));
            }
            for i in 0..segs {
                let t0 = i * 2;
                let b0 = t0 + 1;
                let t1 = t0 + 2;
                let b1 = t0 + 3;
                inds.extend_from_slice(&[t0, b0, b1, t0, b1, t1]);
            }
            Some((verts, inds, color, unlit))
        }
    }
}

// Keep unused collection imports suppressed cleanly if any.
#[allow(dead_code)]
fn _assert_collections_used(_: BTreeMap<u64, u64>, _: HashSet<u64>) {}
