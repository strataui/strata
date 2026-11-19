//! Hardware-accelerated `wgpu` 3D renderer with double-single RTE precision and
//! single-draw-call programmable vertex pulling for planetary/terrain tile pyramids.

use crate::scene3d::{
    Background3D, Camera3D, Geometry3D, Light3D, Material3D, Mesh3D, RetainedCameraUniforms,
    RetainedDynamicMesh, RetainedEnvironmentAssets, RetainedParticleBillboard,
    RetainedParticleLayer, RetainedParticleSprite, RetainedStar, RetainedTerrainFrame,
    RetainedTileMesh, RetainedTileTexture, RteTileVertex, Scene3D, TileKeyId,
};
use bytemuck::{Pod, Zeroable};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

#[inline]
fn gpu_perf_logging_enabled() -> bool {
    static PERF_ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *PERF_ENABLED.get_or_init(|| std::env::var_os("STRATA_PERF_LOG").is_some())
}

/// Default maximum new tile textures uploaded to the GPU atlas per frame during continuous flight.
pub const DEFAULT_TEXTURE_UPLOAD_BUDGET: usize = 512;
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

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuGeoInstance {
    anchor_high: [f32; 4], // xyz = split high f32 of anchor_ecef
    anchor_low: [f32; 4],  // xyz = split low f32 of anchor_ecef
    col0: [f32; 4],        // xyz = model col 0 (+X East -> world m), w = roughness
    col1: [f32; 4],        // xyz = model col 1 (+Y North -> world m), w = metalness
    col2: [f32; 4],        // xyz = model col 2 (+Z Up -> world m), w = unlit (1.0 or 0.0)
    base_color: [f32; 4],  // rgba
    emissive: [f32; 4],    // xyz = emissive rgb, w = accent_mix
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuParticleLayerUniforms {
    basis_east: [f32; 4],      // xyz = local ENU east, w = wrapped_phase.x
    basis_north: [f32; 4],     // xyz = local ENU north, w = wrapped_phase.y
    basis_up: [f32; 4],        // xyz = local ENU up, w = wrapped_phase.z
    cam_right_mpx: [f32; 4],   // xyz = camera right world vec, w = meters_per_px_at_1m
    cam_up_pad: [f32; 4],      // xyz = camera up world vec, w = 0.0
    altitude_bounds: [f32; 4], // x = eye_alt_msl, y = ground_alt_msl, z = ceiling_alt_msl, w = ceiling_fade_m
    volume_params: [f32; 4],   // x = extent_xy, y = extent_z, z = edge_fade_frac, w = near_fade_m
    motion_params: [f32; 4],   // x = fall_speed_mps, y = wind_e, z = wind_n, w = wrapped_time_s
    size_params: [f32; 4],     // x = width_m, y = height_m, z = velocity_stretch, w = spin_rad_s
    flutter_clamp: [f32; 4],   // x = flutter_amp_m, y = flutter_freq_hz, z = min_px, w = max_px
    color: [f32; 4],           // rgba
    meta: [u32; 4],            // x = sprite_mode, y = billboard_mode, z = seed, w = particle_count
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
    @location(2) ray_dir: vec3<f32>,
}

@vertex
fn vs_sky(@builtin(vertex_index) vid: u32) -> SkyOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -3.0),
        vec2<f32>(3.0, 1.0),
        vec2<f32>(-1.0, 1.0),
    );
    let p = positions[vid];
    let p4 = cam.inv_view_proj_rte * vec4<f32>(p, 0.5, 1.0);
    let w_safe = select(p4.w, select(-1e-6, 1e-6, p4.w >= 0.0), abs(p4.w) < 1e-6);
    var out: SkyOut;
    out.pos = vec4<f32>(p, 0.9999, 1.0);
    out.uv_y = clamp((1.0 - p.y) * 0.5, 0.0, 1.0);
    out.ndc = p;
    out.ray_dir = p4.xyz / w_safe;
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
    let rd = normalize(in.ray_dir);
    let rd_icrf = normalize((cam.icrf_from_ecef * vec4<f32>(rd, 0.0)).xyz);
    // Derivatives must remain in uniform control flow for WGSL validation.
    let dx_icrf = dpdx(rd_icrf);
    let dy_icrf = dpdy(rd_icrf);

    let space_t = 1.0 - smoothstep(0.0, 0.85, cam.env_params.z);
    let sun_dir = normalize(cam.sun_dir.xyz);

    var atmo_sky = vec3<f32>(0.0);
    if (cam.cloud_params.w < 0.5 || space_t < 0.999) {
        let sky_grad_2d = mix(cam.sky_top, cam.sky_bottom, clamp(in.uv_y, 0.0, 1.0));
        atmo_sky = sky_grad_2d.rgb;
        let eye_pos = cam.eye_high.xyz + cam.eye_low.xyz;
        let eye_r = length(eye_pos);
        if (eye_r > 1000000.0) {
            let eye_up = eye_pos / eye_r;
            let r_earth = 6378137.0;
            let horiz_sin = -sqrt(max(eye_r * eye_r - r_earth * r_earth, 0.0)) / max(eye_r, r_earth);
            let mu = dot(rd, eye_up);
            let elev = mu - horiz_sin;
            let h_elev = clamp(max(elev, 0.0), 0.0, 1.0);

            let day_scale = clamp(max(cam.sky_bottom.b, max(cam.sky_bottom.r, cam.sky_bottom.g)), 0.0, 1.0);
            let zenith_col = vec3<f32>(0.10, 0.25, 0.55) * day_scale;
            let mid_col = vec3<f32>(0.27, 0.53, 0.86) * day_scale;
            let horizon_mist = vec3<f32>(0.75, 0.86, 0.96) * day_scale;

            let zenith_t = pow(clamp(h_elev * 1.85, 0.0, 1.0), 0.58);
            let base_dome = mix(mid_col, zenith_col, zenith_t);
            let optical_haze = exp(-h_elev * 10.5);
            let mie_sun = pow(max(dot(rd, sun_dir), 0.0), 5.0) * (1.0 - smoothstep(0.0, 0.45, h_elev));
            let warm_mist = horizon_mist + vec3<f32>(0.12, 0.09, 0.04) * mie_sun * day_scale;
            let above_sky = mix(base_dome, warm_mist, clamp(optical_haze * 0.90, 0.0, 1.0));
            // Below the mathematical horizon, hold the soft horizon mist so valleys between distant
            // terrain tiles never expose a dark or hard-edged gap.
            let below_t = smoothstep(0.0, -0.06, elev);
            let ground_mist = vec3<f32>(0.64, 0.77, 0.90) * day_scale;
            atmo_sky = mix(above_sky, ground_mist, below_t);
        }
        if (cam.cloud_params.w < 0.5) {
            return vec4<f32>(atmo_sky, sky_grad_2d.a);
        }
    }

    // 1. Pitch-black deep space + J2000 ICRF telescope skybox cubemap (or procedural Milky Way fallback)
    var col = vec3<f32>(0.0);
    if (cam.ambient_color.w > 0.001 && space_t > 0.001) {
        let cube_dim = max(f32(textureDimensions(skybox_cube, 0).x), 1.0);
        let texel_sq = (cube_dim * cube_dim * 0.25) * max(dot(dx_icrf, dx_icrf), dot(dy_icrf, dy_icrf));
        let sky_lod = max(0.5 * log2(max(texel_sq, 1.0)), 0.0);
        let cube_s = textureSampleLevel(skybox_cube, env_sampler, rd_icrf, sky_lod);
        let sky_gain = cam.viewport_wh.z;
        if (cube_s.a > 0.01 && sky_gain > 0.001) {
            col += cube_s.rgb * sky_gain * space_t;
        } else {
            // Galactic North Pole & Galactic Center unit vectors (J2000)
            let gnp = vec3<f32>(-0.867666, -0.198076, 0.455984);
            let sin_b = dot(rd_icrf, gnp);
            if (abs(sin_b) < 0.35) {
                let gc = vec3<f32>(-0.054876, -0.873437, -0.483835);
                let core_w = 0.35 + 0.65 * max(dot(rd_icrf, gc), 0.0);
                let dust_n = noise3(rd_icrf * 7.5) * 0.65 + noise3(rd_icrf * 17.0) * 0.35;
                let rift = smoothstep(0.015, 0.14, abs(sin_b) + (dust_n - 0.5) * 0.09);
                let mw = exp(-pow(abs(sin_b) * 8.0, 1.5)) * core_w * (0.45 + 0.55 * dust_n) * mix(0.45, 1.0, rift);
                col += vec3<f32>(0.055, 0.072, 0.125) * mw * min(cam.ambient_color.w, 1.8) * space_t;
            }
        }
    }

    // Blend pitch-black space to 3D horizon-anchored daytime atmospheric sky when morphing down into 2.5D
    if (space_t < 0.999) {
        col = mix(atmo_sky, col, space_t);
    }

    // 2. Realistic Sun (photosphere limb darkening + solar corona + halo)
    if (cam.sun_dir.w > 0.5) {
        let sun_cos = clamp(dot(rd, sun_dir), -1.0, 1.0);
        if (sun_cos > 0.82) {
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
    }

    // 3. Realistic Moon (NASA LRO albedo + Lommel-Seeliger phase from Sun vector + Earthshine)
    if (cam.moon_dir.w > 0.5) {
        let m_dir = normalize(cam.moon_dir.xyz);
        let b_cos = dot(rd, m_dir);
        let r_moon = max(cam.env_params.w, 0.004);
        if (b_cos > 1.0 - 0.501 * r_moon * r_moon) {
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
        return vec4<f32>(0.0);
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
    @location(9) land_uv_approx: vec2<f32>,
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
    let ecef_p = (inst.center_high.xyz + inst.center_low.xyz) + v.pos_u.xyz;
    let dv = normalize(ecef_p);

    var out: TerrainVsOut;
    out.clip_pos = cam.view_proj_rte * vec4<f32>(world_rel, 1.0);
    out.normal = normalize(v.norm_v.xyz);
    out.uv = mix(inst.uv_rect.xy, inst.uv_rect.zw, raw_uv);
    out.tint = inst.tint;
    out.tex_layer = inst.tile_meta.y;
    out.unlit = inst.tile_meta.z;
    out.ecef_pos = ecef_p;
    out.view_rel = world_rel;
    out.night_uv = mix(inst.night_uv_rect.xy, inst.night_uv_rect.zw, raw_uv);
    out.night_layer = inst.tile_meta.w;
    out.land_uv_approx = vec2<f32>(
        atan2(dv.y, dv.x) * 0.15915494309 + 0.5,
        clamp(0.5 - asin(clamp(dv.z, -1.0, 1.0)) * 0.31830988618, 0.001, 0.999),
    );
    return out;
}

fn hash21(p: vec2<f32>) -> f32 {
    var q = fract(p * vec2<f32>(0.1031, 0.1030));
    q += dot(q, q.yx + 33.33);
    return fract((q.x + q.y) * q.x);
}

fn vnoise2(x: vec2<f32>) -> f32 {
    let i = floor(x);
    let f = fract(x);
    let u = f * f * (3.0 - 2.0 * f);
    let n00 = hash21(i);
    let n10 = hash21(i + vec2<f32>(1.0, 0.0));
    let n01 = hash21(i + vec2<f32>(0.0, 1.0));
    let n11 = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(n00, n10, u.x), mix(n01, n11, u.x), u.y);
}

fn shade_terrain(in: TerrainVsOut, base: vec4<f32>) -> vec4<f32> {
    if (in.unlit != 0u) {
        return base;
    }

    let n_mesh = normalize(in.normal);
    let sun_dir = normalize(cam.sun_dir.xyz);
    let view_dir = normalize(-in.view_rel);

    let dn_strength = clamp(cam.env_params.x, 0.0, 1.0);
    let city_strength = max(cam.env_params.y, 0.0);
    let blend_25d = clamp(cam.env_params.z, 0.0, 1.0);

    let flat_lit = vec3<f32>(0.98);

    let d = normalize(in.ecef_pos);
    let ndotl = dot(d, sun_dir);
    let slope_shade = clamp(1.0 + (dot(n_mesh, sun_dir) - ndotl) * 0.35 * blend_25d, 0.82, 1.18);

    let px_m = max(length(fwidth(d)) * 6378137.0, 6.0);
    var u = fract(in.land_uv_approx.x);
    var v = in.land_uv_approx.y;
    if ((d.x < -0.5 && abs(d.y) < 0.14) || abs(d.z) > 0.95) {
        u = fract(atan2(d.y, d.x) * 0.15915494309 + 0.5);
        v = clamp(0.5 - asin(clamp(d.z, -1.0, 1.0)) * 0.31830988618, 0.001, 0.999);
    }
    let env_lod = clamp(log2(max(px_m * 0.0001022, 1.0)), 0.0, 5.0);
    let env_mask = textureSampleLevel(land_mask_tex, terrain_sampler, vec2<f32>(u, v), env_lod);
    let on_land = smoothstep(0.18, 0.55, env_mask.r);

    let terminator = smoothstep(-0.14, 0.16, ndotl);
    let tn = ndotl * 7.5;
    let twilight = exp(-tn * tn) * vec3<f32>(0.42, 0.18, 0.06);
    let f1 = 1.0 - max(dot(d, view_dir), 0.0);
    let fresnel = f1 * f1 * f1;
    let atmo_rim = vec3<f32>(0.28, 0.56, 1.0) * fresnel * smoothstep(-0.18, 0.32, ndotl) * 0.45 * (1.0 - blend_25d);
    let ocean_night = vec3<f32>(0.048, 0.068, 0.108);
    let land_night = vec3<f32>(0.095, 0.120, 0.174);
    let night_fill = mix(ocean_night, land_night, on_land);
    let day_lit = (night_fill + cam.sun_color.rgb * terminator + twilight) * slope_shade;

    let lit = mix(flat_lit, day_lit, dn_strength);
    var rgb = base.rgb * lit + atmo_rim * dn_strength;

    if (city_strength > 0.001 && dn_strength > 0.001 && on_land > 0.001 && ndotl < 0.06) {
        let night_mask = smoothstep(0.06, -0.14, ndotl) * dn_strength * city_strength * on_land;
        if (night_mask > 0.001) {
            let eg = env_mask.g;
            let base_lin = select(
                0.0,
                max(eg * eg * (0.75 + 0.25 * eg) - 0.022, 0.0) * 1.0225,
                eg > 0.175,
            );
            var black_marble = base_lin;
            let detail_night_w = 1.0 - smoothstep(350.0, 1100.0, px_m);
            if (in.night_layer != 0u && detail_night_w > 0.005) {
                let nl = i32(in.night_layer);
                let luma_w = vec3<f32>(0.299, 0.587, 0.114);
                let c0 = dot(textureSampleLevel(terrain_atlas, terrain_sampler, in.night_uv, nl, 0.0).rgb, luma_w);
                var raw_tile = c0;
                if (c0 > 0.015) {
                    let hduv = 0.5 / 256.0;
                    let c1 = dot(textureSampleLevel(terrain_atlas, terrain_sampler, clamp(in.night_uv + vec2<f32>(hduv, hduv), vec2<f32>(0.0), vec2<f32>(1.0)), nl, 0.0).rgb, luma_w);
                    let c2 = dot(textureSampleLevel(terrain_atlas, terrain_sampler, clamp(in.night_uv - vec2<f32>(hduv, hduv), vec2<f32>(0.0), vec2<f32>(1.0)), nl, 0.0).rgb, luma_w);
                    raw_tile = c0 * 0.5 + (c1 + c2) * 0.25;
                }
                let tile_lin = max(raw_tile - 0.022, 0.0) * 1.0225;
                black_marble = mix(base_lin, tile_lin, detail_night_w);
            }
            if (black_marble > 0.00018) {
                let bm_srgb = sqrt(clamp(black_marble, 0.0, 1.0));
                let city_core = smoothstep(0.08, 0.48, bm_srgb);
                let corridor = smoothstep(0.02, 0.22, bm_srgb) * 0.22;
                var pinpoint = 0.86;
                var corridor_mod = 0.825;
                if (px_m < 4500.0) {
                    let p2 = vec2<f32>(d.x - d.z * 0.618034, d.y + d.z * 0.618034);
                    let med_w = 1.0 - smoothstep(1400.0, 4500.0, px_m);
                    let n_block = vnoise2(p2 * 2800.0);
                    let n_street = vnoise2(p2 * 10500.0);
                    let fine_w = 1.0 - smoothstep(160.0, 1200.0, px_m);
                    var sub_grid = 1.0;
                    if (fine_w > 0.02) {
                        let arterial = smoothstep(0.30, 0.88, vnoise2(p2 * 68000.0));
                        let fine_mask = 0.24 + 1.12 * arterial;
                        sub_grid = mix(1.0, fine_mask, fine_w);
                    }
                    let street_grid = smoothstep(0.24, 0.86, n_block * 0.55 + n_street * 0.45);
                    let detail_pin = mix(0.62, 1.38, street_grid * (0.62 + 0.38 * n_street)) * sub_grid;
                    pinpoint = mix(0.86, detail_pin, med_w);
                    corridor_mod = mix(0.825, (0.65 + 0.35 * n_block) * sub_grid, med_w);
                }
                let city_luma = (city_core * pinpoint + corridor * corridor_mod) * night_mask;
                let warm_amber = vec3<f32>(0.97, 0.74, 0.38);
                let warm_white = vec3<f32>(1.0, 0.90, 0.68);
                rgb += mix(warm_amber, warm_white, city_core) * city_luma * 0.92;
            }
        }
    }

    if (blend_25d > 0.01) {
        let view_dist = length(in.view_rel);
        let haze_start = smoothstep(3500.0, 52000.0, view_dist);
        let aerial_haze = clamp((1.0 - exp(-view_dist * 0.000018)) * haze_start * blend_25d * 0.68, 0.0, 0.72);
        let day_scale = clamp(max(cam.sky_bottom.b, max(cam.sky_bottom.r, cam.sky_bottom.g)), 0.0, 1.0);
        let horizon_haze_col = vec3<f32>(0.73, 0.85, 0.96) * day_scale * mix(vec3<f32>(1.0), clamp(lit * 1.15, vec3<f32>(0.35), vec3<f32>(1.0)), dn_strength);
        rgb = mix(rgb, horizon_haze_col, aerial_haze);
    }

    return vec4<f32>(rgb, base.a);
}

@fragment
fn fs_terrain_opaque(in: TerrainVsOut) -> @location(0) vec4<f32> {
    let sampled = textureSample(terrain_atlas, terrain_sampler, in.uv, i32(in.tex_layer));
    return shade_terrain(in, sampled * in.tint);
}

@fragment
fn fs_terrain(in: TerrainVsOut) -> @location(0) vec4<f32> {
    let sampled = textureSample(terrain_atlas, terrain_sampler, in.uv, i32(in.tex_layer));
    let base = sampled * in.tint;
    if (base.a <= 0.001) {
        discard;
    }
    return shade_terrain(in, base);
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
@group(0) @binding(3) var cloud_noise_tex: texture_2d<f32>;
@group(0) @binding(4) var cloud_noise_sampler: sampler;

struct CloudOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) unit_pos: vec3<f32>,
    @location(1) uv_par: vec4<f32>,
    @location(2) params_fs: vec4<f32>,
    @location(3) light_ts_term: vec4<f32>,
    @location(4) view_ts_haze: vec4<f32>,
    @location(5) sun_ts_lod: vec4<f32>,
    @location(6) grad_step_bic: vec4<f32>,
}

fn sample_cloud_density(uv: vec2<f32>, lod: f32) -> f32 {
    return textureSampleLevel(cloud_tex, cloud_sampler, uv, lod).r;
}

fn sample_cloud_bicubic_density(uv: vec2<f32>, dims: vec2<f32>, inv_dims: vec2<f32>) -> f32 {
    let tc = uv * dims - vec2<f32>(0.5, 0.5);
    let i = floor(tc);
    let f = clamp(tc - i, vec2<f32>(0.0), vec2<f32>(1.0));
    let f2 = f * f;
    let f3 = f2 * f;

    let omf = vec2<f32>(1.0) - f;
    let w0 = omf * omf * omf * (1.0 / 6.0);
    let w1 = (3.0 * f3 - 6.0 * f2 + vec2<f32>(4.0)) * (1.0 / 6.0);
    let w2 = (-3.0 * f3 + 3.0 * f2 + 3.0 * f + vec2<f32>(1.0)) * (1.0 / 6.0);
    let w3 = f3 * (1.0 / 6.0);

    let g0 = w0 + w1;
    let g1 = w2 + w3;
    let u0 = fract((i.x - 0.5 + w1.x / max(g0.x, 1e-5)) * inv_dims.x);
    let u1 = fract((i.x + 1.5 + w3.x / max(g1.x, 1e-5)) * inv_dims.x);
    let v0 = clamp((i.y - 0.5 + w1.y / max(g0.y, 1e-5)) * inv_dims.y, 0.001, 0.999);
    let v1 = clamp((i.y + 1.5 + w3.y / max(g1.y, 1e-5)) * inv_dims.y, 0.001, 0.999);

    let c00 = sample_cloud_density(vec2<f32>(u0, v0), 0.0);
    let c10 = sample_cloud_density(vec2<f32>(u1, v0), 0.0);
    let c01 = sample_cloud_density(vec2<f32>(u0, v1), 0.0);
    let c11 = sample_cloud_density(vec2<f32>(u1, v1), 0.0);

    let top = mix(c10, c00, g0.x);
    let bot = mix(c11, c01, g0.x);
    return mix(bot, top, g0.y);
}

fn cloud_density_to_height(d: f32) -> f32 {
    let x = clamp(d, 0.0, 1.0);
    let dome = smoothstep(0.06, 0.88, x);
    return dome * (0.65 + 0.35 * x * (2.0 - x));
}

@vertex
fn vs_cloud(@location(0) pos_u: vec4<f32>, @location(1) norm_v: vec4<f32>) -> CloudOut {
    let base_cloud_r = 6391500.0;
    let r_earth = 6378137.0;
    let d = normalize(pos_u.xyz);
    let eye_pos = cam.eye_high.xyz + cam.eye_low.xyz;
    let eye_r = max(length(eye_pos), 1.0);
    let eye_alt = max(eye_r - r_earth, 0.0);
    let eye_dir = eye_pos / eye_r;
    let below_clouds = select(0.0, 1.0, eye_r < base_cloud_r);
    let horiz_cos = r_earth / max(eye_r, r_earth);
    let above_shell = max(eye_r - base_cloud_r, 0.0);
    let vert_disp_w = smoothstep(18000.0, 75000.0, above_shell)
        * (1.0 - smoothstep(110000.0, 220000.0, above_shell));

    let u_unwrapped = atan2(d.y, d.x) * 0.15915494309 + 0.5;
    let v_unwrapped = clamp(0.5 - asin(clamp(d.z, -1.0, 1.0)) * 0.31830988618, 0.001, 0.999);

    var cloud_r = base_cloud_r;
    if (vert_disp_w > 0.001 && dot(d, eye_dir) > horiz_cos - 0.16) {
        let u_v = fract(u_unwrapped + cam.cloud_params.y * 0.15915494309);
        let h_vert = cloud_density_to_height(sample_cloud_density(vec2<f32>(u_v, v_unwrapped), 2.5));
        cloud_r = mix(base_cloud_r, 6390200.0 + h_vert * 4200.0, vert_disp_w);
    }

    let world_pos = d * cloud_r;
    let world_rel = (world_pos - cam.eye_high.xyz) - cam.eye_low.xyz;
    var clip = cam.view_proj_rte * vec4<f32>(world_rel, 1.0);
    if (clip.w > 0.0) {
        clip.z = min(clip.z, clip.w * 0.9995);
    }

    let opacity = clamp(cam.cloud_params.x + cam.viewport_wh.w, 0.0, 1.0);
    let detail_w = 1.0 - smoothstep(120000.0, 2200000.0, eye_alt);
    let mode_25d = smoothstep(0.15, 0.75, clamp(max(detail_w, below_clouds), 0.0, 1.0));
    let gate_25d = smoothstep(0.85, 0.995, mode_25d);
    let coarse_gate = mix(0.014, 0.212, gate_25d);

    let shell_pos = d * base_cloud_r;
    let view_vec = shell_pos - eye_pos;
    let view_dist = max(length(view_vec), 1.0);
    let view_dir = view_vec / view_dist;

    let above_fade = smoothstep(1500.0, 160000.0, max(eye_r - base_cloud_r, 0.0));
    let below_fade = smoothstep(1200.0, 9000.0, max(base_cloud_r - eye_r, 0.0));
    let plane_alt_fade = mix(0.24, 1.0, mix(above_fade, below_fade, below_clouds));
    let ray_prox_fade = mix(0.78, 1.0, smoothstep(600.0, 10000.0, view_dist));
    let horiz_sin = -sqrt(max(eye_r * eye_r - r_earth * r_earth, 0.0)) / max(eye_r, r_earth);
    let ray_elev = dot(view_dir, eye_dir) - horiz_sin;
    let sky_hemi_fade = mix(1.0, smoothstep(-0.015, 0.035, ray_elev), below_clouds);
    let cross_fade = plane_alt_fade * ray_prox_fade * sky_hemi_fade;

    let low_alt_factor = 1.0 - smoothstep(40000.0, 350000.0, eye_alt);
    let horizon_graze = abs(dot(view_dir, d));
    let dist_persp_fade = 1.0 - 0.58 * smoothstep(55000.0, 260000.0, view_dist) * below_clouds;
    let horizon_fade = mix(1.0, smoothstep(0.004, 0.055, horizon_graze), low_alt_factor) * dist_persp_fade;

    let sun_dir = normalize(cam.sun_dir.xyz);
    let ndotl = dot(d, sun_dir);
    let dn_strength = clamp(cam.env_params.x, 0.0, 1.0);
    let term = smoothstep(-0.14, 0.18, ndotl);
    let night_fade = mix(0.14, 1.0, mix(1.0, term, dn_strength));
    let lit_mask = mix(night_fade, 1.0, below_clouds);
    let layer_translucency = mix(1.0, 0.88, below_clouds);
    let common_alpha = opacity * lit_mask * cross_fade * horizon_fade * layer_translucency;

    let east_raw = vec3<f32>(-d.y, d.x, 0.0);
    let east_len = length(east_raw);
    let t_east = select(vec3<f32>(1.0, 0.0, 0.0), east_raw / max(east_len, 1e-5), east_len > 1e-4);
    let t_north = normalize(cross(d, t_east));

    let v_east = dot(view_dir, t_east);
    let v_north = dot(view_dir, t_north);
    let v_up = dot(view_dir, d);
    let v_rad = max(abs(v_up), 0.25);
    let par_amp = mix(0.0015, 0.00075, mode_25d) / v_rad;
    let east_scale = clamp(east_len, 0.25, 1.0);
    let par_vec = vec2<f32>(v_east * par_amp * east_scale, v_north * par_amp);

    let unlit_key = normalize(d * 0.52 + t_north * 0.62 - t_east * 0.58);
    let relief_light = normalize(mix(unlit_key, normalize(sun_dir + d * 0.28), dn_strength * 0.75));
    let light_ts = vec3<f32>(dot(relief_light, t_east), dot(relief_light, t_north), dot(relief_light, d));
    let sun_ts = vec3<f32>(dot(sun_dir, t_east), dot(sun_dir, t_north), ndotl);
    let uv_lod = clamp(0.6931472 * smoothstep(2200000.0, 14000000.0, eye_alt), 0.0, 3.0);
    let cloud_haze_t = smoothstep(25000.0, 180000.0, view_dist) * 0.65 * below_clouds;

    let dims = max(vec2<f32>(textureDimensions(cloud_tex, 0)), vec2<f32>(1.0));
    let step_scale = mix(1.75, 1.35, detail_w);
    let bicubic_w = 1.0 - smoothstep(70000.0, 220000.0, eye_alt);

    var out: CloudOut;
    out.clip_pos = clip;
    out.unit_pos = d;
    out.uv_par = vec4<f32>(u_unwrapped, v_unwrapped, par_vec.x, par_vec.y);
    out.params_fs = vec4<f32>(common_alpha, coarse_gate, detail_w, mode_25d);
    out.light_ts_term = vec4<f32>(light_ts, term);
    out.view_ts_haze = vec4<f32>(v_east, v_north, v_up, cloud_haze_t);
    out.sun_ts_lod = vec4<f32>(sun_ts, uv_lod);
    out.grad_step_bic = vec4<f32>(step_scale / dims.x, step_scale / dims.y, bicubic_w, below_clouds);
    return out;
}

@fragment
fn fs_cloud(in: CloudOut) -> @location(0) vec4<f32> {
    let common_alpha = in.params_fs.x;
    if (common_alpha <= 0.004) {
        return vec4<f32>(0.0);
    }

    let d = in.unit_pos;
    var u_unwrapped = in.uv_par.x;
    var v_base = in.uv_par.y;
    if ((d.x < -0.5 && abs(d.y) < 0.14) || abs(d.z) > 0.95) {
        let dn = normalize(d);
        u_unwrapped = atan2(dn.y, dn.x) * 0.15915494309 + 0.5;
        v_base = clamp(0.5 - asin(clamp(dn.z, -1.0, 1.0)) * 0.31830988618, 0.001, 0.999);
    }
    let u_base = fract(u_unwrapped + cam.cloud_params.y * 0.15915494309);
    let uv_lod = in.sun_ts_lod.w;

    // 128 KB mip-2 coarse check exits immediately in 16 VGPRs before Stage-2 VMEM burst.
    let d_coarse = sample_cloud_density(vec2<f32>(u_base, v_base), max(uv_lod, 2.0));
    if (d_coarse <= in.params_fs.y) {
        return vec4<f32>(0.0);
    }

    let detail_w = in.params_fs.z;
    let mode_25d = in.params_fs.w;
    let h_off = cloud_density_to_height(d_coarse) - 0.22;
    let u = fract(u_base - in.uv_par.z * h_off);
    let v = clamp(v_base + in.uv_par.w * h_off, 0.001, 0.999);

    let p_cloud2 = vec2<f32>(d.x - d.z * 0.618034, d.y + d.z * 0.618034);
    let noise_uv = p_cloud2 * 26.5625 + vec2<f32>(cam.cloud_params.z * 0.000005625, 0.0);

    // Single unbranched 4-fetch VMEM clause: all 4 texture samples issue in parallel.
    let d_c_lin = sample_cloud_density(vec2<f32>(u, v), uv_lod);
    let d_e = sample_cloud_density(vec2<f32>(fract(u + in.grad_step_bic.x), v), uv_lod);
    let d_n = sample_cloud_density(vec2<f32>(u, clamp(v - in.grad_step_bic.y, 0.001, 0.999)), uv_lod);
    let ns_all = textureSampleLevel(cloud_noise_tex, cloud_noise_sampler, noise_uv, 0.0);

    var d_c_base = d_c_lin;
    let bicubic_w = in.grad_step_bic.z;
    if (bicubic_w > 0.02) {
        let dims = max(vec2<f32>(textureDimensions(cloud_tex, 0)), vec2<f32>(1.0));
        d_c_base = mix(d_c_lin, sample_cloud_bicubic_density(vec2<f32>(u, v), dims, 1.0 / dims), bicubic_w);
    }
    let warp_delta = (ns_all.ba - vec2<f32>(0.5)) * (0.55 * detail_w);
    let d_c = clamp(
        d_c_base + warp_delta.x * (d_e - d_c_lin) + warp_delta.y * (d_n - d_c_lin),
        0.0,
        1.0,
    );

    let crisp_3d = d_c * (0.35 + 0.65 * d_c);
    let base_density = 0.56 * d_c + 0.22 * (d_e + d_n);
    let clear_cut = smoothstep(0.14, 0.54, base_density);
    let fbm = ns_all.x * 0.65 + ns_all.y * 0.35;
    let fbm_billow = (fbm - 0.5) * 0.16 * detail_w;
    let eroded = clamp(clear_cut * (0.44 + fbm * 1.16) - 0.13, 0.0, 1.0);
    let refined = mix(clear_cut, eroded, detail_w * 0.88);

    let sigmoid_approx = smoothstep(0.11, 0.75, refined);
    let hard_zero_gate = smoothstep(0.13, 0.31, refined);
    let mask_25d = clamp(sigmoid_approx * hard_zero_gate * 1.04, 0.0, 1.0);
    let mask_density = mix(crisp_3d, mask_25d, mode_25d);

    let alpha = mask_density * common_alpha;
    if (alpha <= 0.003) {
        return vec4<f32>(0.0);
    }

    let h_c = clamp(cloud_density_to_height(d_c) + fbm_billow, 0.0, 1.0);
    let h_e = cloud_density_to_height(d_e);
    let h_n = cloud_density_to_height(d_n);
    let h_smooth = clamp(0.52 * h_c + 0.24 * (h_e + h_n), 0.0, 1.0);

    let normal_amp = mix(3.4, 2.2, detail_w);
    let dh_east = (h_e - h_c) * 2.0;
    let dh_north = (h_n - h_c) * 2.0;
    let n_ts = normalize(vec3<f32>(-dh_east * normal_amp, -dh_north * normal_amp, 1.0));

    let light_ts = in.light_ts_term.xyz;
    let slope_lit = clamp(dot(n_ts, light_ts), 0.0, 1.0);
    let horizon_shadow = clamp(-(dh_east * light_ts.x - dh_north * light_ts.y) * 1.15, -0.28, 0.28);
    let height_ao = mix(0.68, 1.06, smoothstep(0.05, 0.85, h_smooth));
    let rim_base = 1.0 - clamp(abs(dot(in.view_ts_haze.xyz, n_ts)), 0.0, 1.0);
    let silver_rim = (rim_base * rim_base * rim_base)
        * smoothstep(0.12, 0.52, d_c)
        * (1.0 - smoothstep(0.58, 0.92, d_c))
        * 0.18;

    let below_clouds = in.grad_step_bic.w;
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

    let ndotl = in.sun_ts_lod.z;
    let sn = ndotl * 7.5;
    let sn2 = sn * sn;
    let sunset = (1.0 / (1.0 + sn2 + 0.5 * sn2 * sn2)) * vec3<f32>(0.48, 0.22, 0.08);
    let sun_slope = smoothstep(-0.10, 0.65, dot(n_ts, in.sun_ts_lod.xyz));
    let term = in.light_ts_term.w;
    let dn_strength = clamp(cam.env_params.x, 0.0, 1.0);
    let day_cloud = (vec3<f32>(0.04, 0.06, 0.10) + cam.sun_color.rgb * term * mix(0.76, 1.12, sun_slope) + sunset) * tex_tint;
    var rgb = mix(flat_cloud, day_cloud, dn_strength * (1.0 - below_clouds));

    let cloud_haze_t = in.view_ts_haze.w;
    if (cloud_haze_t > 0.001) {
        let day_scale = clamp(max(cam.sky_bottom.b, max(cam.sky_bottom.r, cam.sky_bottom.g)), 0.0, 1.0);
        rgb = mix(rgb, vec3<f32>(0.75, 0.86, 0.96) * day_scale, cloud_haze_t);
    }

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

const GEO_OBJECT_WGSL: &str = r#"
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

struct GpuGeoInstance {
    anchor_high: vec4<f32>,
    anchor_low: vec4<f32>,
    col0: vec4<f32>,
    col1: vec4<f32>,
    col2: vec4<f32>,
    base_color: vec4<f32>,
    emissive: vec4<f32>,
}

@group(0) @binding(0) var<uniform> cam: GpuCameraUniforms;
@group(0) @binding(1) var<storage, read> geo_instances: array<GpuGeoInstance>;

struct GeoVsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_rel: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) base_color: vec4<f32>,
    @location(3) emissive: vec4<f32>,
    @location(4) material_params: vec4<f32>,
}

@vertex
fn vs_geo_object(
    @location(0) pos_u: vec4<f32>,
    @location(1) norm_v: vec4<f32>,
    @builtin(instance_index) inst_idx: u32,
) -> GeoVsOut {
    let inst = geo_instances[inst_idx];
    let c0 = inst.col0.xyz;
    let c1 = inst.col1.xyz;
    let c2 = inst.col2.xyz;

    let center_rel = (inst.anchor_high.xyz - cam.eye_high.xyz)
                   + (inst.anchor_low.xyz - cam.eye_low.xyz);
    let local_offset = c0 * pos_u.x + c1 * pos_u.y + c2 * pos_u.z;
    let world_rel = center_rel + local_offset;

    // Exact cofactor (adjugate^T) normal transform: det(M) * (M^-1)^T * n.
    // Preserves surface perpendicularity under non-uniform scale and 2D/2.5D morphing.
    let adj0 = cross(c1, c2);
    let adj1 = cross(c2, c0);
    let adj2 = cross(c0, c1);
    var world_n = adj0 * norm_v.x + adj1 * norm_v.y + adj2 * norm_v.z;
    if (dot(world_n, world_n) < 1e-12) {
        world_n = c0 * norm_v.x + c1 * norm_v.y + c2 * norm_v.z;
    }

    var out: GeoVsOut;
    out.clip_pos = cam.view_proj_rte * vec4<f32>(world_rel, 1.0);
    out.world_rel = world_rel;
    out.normal = world_n;
    out.base_color = inst.base_color;
    out.emissive = inst.emissive;
    out.material_params = vec4<f32>(inst.col0.w, inst.col1.w, inst.col2.w, norm_v.w);
    return out;
}

@fragment
fn fs_geo_object(in: GeoVsOut) -> @location(0) vec4<f32> {
    let accent_t = clamp(in.material_params.w * in.emissive.w, 0.0, 1.0);
    let albedo = mix(in.base_color.rgb, vec3<f32>(0.97, 0.99, 1.0), accent_t);

    if (in.material_params.z >= 0.5) {
        return vec4<f32>(clamp(albedo + in.emissive.rgb, vec3<f32>(0.0), vec3<f32>(1.0)), in.base_color.a);
    }

    let len_sq = dot(in.normal, in.normal);
    let n = select(vec3<f32>(0.0, 0.0, 1.0), normalize(in.normal), len_sq > 1e-10);
    let view_dir = select(vec3<f32>(0.0, 0.0, 1.0), normalize(-in.world_rel), dot(in.world_rel, in.world_rel) > 1e-8);
    let sun_dir = normalize(cam.sun_dir.xyz);

    let roughness = clamp(in.material_params.x, 0.05, 1.0);
    let metalness = clamp(in.material_params.y, 0.0, 1.0);

    let dn = clamp(cam.env_params.x, 0.0, 1.0);
    let ndotl_sun = max(dot(n, sun_dir), 0.0);
    let ndotv = max(dot(n, view_dir), 0.0);
    let hemi = 0.5 + 0.5 * dot(n, view_dir);

    let studio_lit = vec3<f32>(0.42 + 0.24 * hemi) + vec3<f32>(0.52) * max(ndotl_sun, ndotv * 0.65);
    let day_lit = cam.ambient_color.rgb * (0.75 + 0.45 * hemi) + cam.sun_color.rgb * ndotl_sun;
    let diffuse_light = mix(studio_lit, day_lit, dn);

    let half_vec = select(view_dir, normalize(sun_dir + view_dir), dot(sun_dir + view_dir, sun_dir + view_dir) > 1e-6);
    let ndoth = max(dot(n, half_vec), 0.0);
    let shininess = mix(96.0, 8.0, roughness * roughness);
    let spec_strength = (1.0 - roughness * 0.75) * mix(0.22, 0.85, metalness);
    let f0 = mix(vec3<f32>(0.04), albedo, metalness);
    let spec = f0 * pow(ndoth, shininess) * spec_strength * max(ndotl_sun, 1.0 - dn);

    let diffuse_albedo = albedo * (1.0 - metalness * 0.65);
    let color = clamp(diffuse_albedo * diffuse_light + spec + in.emissive.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(color, in.base_color.a);
}
"#;

const PARTICLE_VOLUME_WGSL: &str = r#"
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

struct GpuParticleLayerUniforms {
    basis_east: vec4<f32>,
    basis_north: vec4<f32>,
    basis_up: vec4<f32>,
    cam_right_mpx: vec4<f32>,
    cam_up_pad: vec4<f32>,
    altitude_bounds: vec4<f32>,
    volume_params: vec4<f32>,
    motion_params: vec4<f32>,
    size_params: vec4<f32>,
    flutter_clamp: vec4<f32>,
    color: vec4<f32>,
    particle_meta: vec4<u32>,
}

@group(0) @binding(0) var<uniform> cam: GpuCameraUniforms;
@group(0) @binding(1) var<uniform> layer: GpuParticleLayerUniforms;
@group(1) @binding(0) var sprite_tex: texture_2d<f32>;
@group(1) @binding(1) var sprite_sampler: sampler;

struct ParticleVsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) @interpolate(flat) sprite_mode: u32,
}

fn pcg_u32(v: u32) -> u32 {
    let state = v * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

fn hash01(seed: u32) -> f32 {
    return f32(pcg_u32(seed)) * (1.0 / 4294967296.0);
}

@vertex
fn vs_particle(
    @builtin(vertex_index) vid: u32,
    @builtin(instance_index) inst_idx: u32,
) -> ParticleVsOut {
    var out: ParticleVsOut;
    out.clip_pos = vec4<f32>(2.0, 2.0, 2.0, 1.0);
    out.uv = vec2<f32>(0.5, 0.5);
    out.color = vec4<f32>(0.0);
    out.sprite_mode = layer.particle_meta.x;

    let base_seed = inst_idx * 4u + layer.particle_meta.z * 1664525u + 1013904223u;
    let r0 = hash01(base_seed + 0u);
    let r1 = hash01(base_seed + 1u);
    let r2 = hash01(base_seed + 2u);
    let r3 = hash01(base_seed + 3u);

    let extent_xy = max(layer.volume_params.x, 1.0);
    let extent_z = max(layer.volume_params.y, 1.0);
    let edge_fade_frac = clamp(layer.volume_params.z, 0.04, 0.48);
    let near_fade_m = max(layer.volume_params.w, 0.1);

    // Pre-wrapped f64->f32 toroidal phase keeps particles 100% locked to world ENU coordinates:
    // panning camera right (+East) shifts wrapped_phase.x down so drops stream cleanly left.
    let u_e = fract(r0 + layer.basis_east.w);
    let u_n = fract(r1 + layer.basis_north.w);
    let u_z = fract(r2 + layer.basis_up.w);

    var rel_e = (u_e - 0.5) * extent_xy;
    var rel_n = (u_n - 0.5) * extent_xy;
    let rel_z = (u_z - 0.5) * extent_z;

    let time_s = layer.motion_params.w;
    let flutter_amp = layer.flutter_clamp.x;
    if (flutter_amp > 0.0001) {
        let flutter_freq = layer.flutter_clamp.y;
        let phase = r3 * 6.2831853 + time_s * flutter_freq * (0.72 + 0.56 * fract(r0 * 17.31));
        let amp = flutter_amp * (0.55 + 0.90 * r3);
        rel_e += sin(phase) * amp;
        rel_n += cos(phase * 0.83 + r1 * 6.2831853) * amp;
    }

    let east = layer.basis_east.xyz;
    let north = layer.basis_north.xyz;
    let up = layer.basis_up.xyz;
    let center_rel = east * rel_e + north * rel_n + up * rel_z;

    let center_clip = cam.view_proj_rte * vec4<f32>(center_rel, 1.0);
    if (center_clip.w <= 0.05) {
        return out;
    }

    let dist = max(length(center_rel), 0.05);
    let nx = abs(rel_e) / (0.5 * extent_xy);
    let ny = abs(rel_n) / (0.5 * extent_xy);
    let nz = abs(rel_z) / (0.5 * extent_z);
    let box_r = max(max(nx, ny), nz);
    let edge_fade = 1.0 - smoothstep(1.0 - edge_fade_frac, 1.0, box_r);
    let near_fade = smoothstep(near_fade_m * 0.30, near_fade_m, dist);

    // Physical altitude bounds: clip smoothly at ground terrain surface and cloud ceiling.
    let particle_alt_msl = layer.altitude_bounds.x + rel_z;
    let ground_alt_msl = layer.altitude_bounds.y;
    let ceiling_alt_msl = layer.altitude_bounds.z;
    let ceiling_fade_m = max(layer.altitude_bounds.w, 1.0);
    let above_ground = smoothstep(ground_alt_msl - 0.5, ground_alt_msl + 1.5, particle_alt_msl);
    let below_ceiling = 1.0 - smoothstep(ceiling_alt_msl - ceiling_fade_m, ceiling_alt_msl, particle_alt_msl);

    let alpha_mul = edge_fade * near_fade * above_ground * below_ceiling;
    if (alpha_mul <= 0.003 || layer.color.a <= 0.003) {
        return out;
    }

    // 6-vertex quad corners in [-1, 1] x [-1, 1]
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(1.0, -1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, 1.0),
    );
    let corner = corners[vid % 6u];

    let cam_right = layer.cam_right_mpx.xyz;
    let cam_up = layer.cam_up_pad.xyz;
    let m_per_px = dist * max(layer.cam_right_mpx.w, 1e-6);
    let min_px = max(layer.flutter_clamp.z, 0.75);
    let max_px = max(layer.flutter_clamp.w, min_px);

    var right_axis = cam_right;
    var up_axis = cam_up;
    var hw = 0.05;
    var hh = 0.05;
    var uv = vec2<f32>(corner.x * 0.5 + 0.5, 0.5 - corner.y * 0.5);

    if (layer.particle_meta.y == 0u) {
        // VelocityAligned: align streak vertically along 3D wind + gravity velocity vector.
        let fall_v = layer.motion_params.x;
        let wind_e = layer.motion_params.y;
        let wind_n = layer.motion_params.z;
        let vel = east * wind_e + north * wind_n - up * fall_v;
        let speed = max(length(vel), 0.05);
        let v_dir = vel / speed;
        let view_dir = center_rel / dist;
        let side_raw = cross(v_dir, view_dir);
        let side_len = length(side_raw);
        let foreshorten = mix(0.26, 1.0, smoothstep(0.06, 0.42, side_len));

        if (side_len > 0.04) {
            right_axis = side_raw / side_len;
            up_axis = -v_dir;
        } else {
            right_axis = cam_right;
            up_axis = cam_up;
        }

        let size_var = 0.75 + 0.50 * r3;
        let raw_hw = max(layer.size_params.x * 0.5 * size_var, 0.004);
        let raw_hh = max((layer.size_params.y + speed * layer.size_params.z) * 0.5 * size_var, raw_hw);
        let max_streak_w_px = min(max_px * 0.12, max(min_px * 2.2, 3.2));
        hw = clamp(raw_hw, min_px * 0.5 * m_per_px, max_streak_w_px * 0.5 * m_per_px);
        hh = clamp(raw_hh, min_px * 0.5 * m_per_px, max_px * 0.5 * m_per_px) * foreshorten;
        hh = max(hh, hw * 1.15);
        // uv.y = 0 at leading bottom tip (corner.y = -1 -> +v_dir), uv.y = 1 at upper trailing tail
        uv = vec2<f32>(corner.x * 0.5 + 0.5, corner.y * 0.5 + 0.5);
    } else {
        // CameraFacing: 2D camera-aligned billboard with per-particle spin (snowflakes, sleet, custom textures).
        let spin_dir = select(-1.0, 1.0, (inst_idx & 1u) == 0u);
        let ang = r3 * 6.2831853 + time_s * layer.size_params.w * spin_dir;
        let s = sin(ang);
        let c = cos(ang);
        right_axis = cam_right * c - cam_up * s;
        up_axis = cam_right * s + cam_up * c;

        let size_var = 0.65 + 0.70 * r3;
        let raw_hw = max(layer.size_params.x * 0.5 * size_var, 0.005);
        let raw_hh = max(layer.size_params.y * 0.5 * size_var, 0.005);
        hw = clamp(raw_hw, min_px * 0.5 * m_per_px, max_px * 0.5 * m_per_px);
        hh = clamp(raw_hh, min_px * 0.5 * m_per_px, max_px * 0.5 * m_per_px);
        uv = vec2<f32>(corner.x * 0.5 + 0.5, 0.5 - corner.y * 0.5);
    }

    let world_rel = center_rel + right_axis * (corner.x * hw) + up_axis * (corner.y * hh);
    out.clip_pos = cam.view_proj_rte * vec4<f32>(world_rel, 1.0);
    out.uv = uv;

    // Day/night atmospheric lighting tint so precipitation responds naturally to sun elevation.
    let dn = clamp(cam.env_params.x, 0.0, 1.0);
    let sun_elev = dot(normalize(up), normalize(cam.sun_dir.xyz));
    let sun_w = smoothstep(-0.10, 0.28, sun_elev);
    let day_lit = mix(
        vec3<f32>(0.26, 0.31, 0.40),
        cam.ambient_color.rgb * 0.85 + cam.sun_color.rgb * 0.92,
        sun_w,
    );
    let lit = mix(vec3<f32>(0.96, 0.98, 1.0), clamp(day_lit, vec3<f32>(0.24), vec3<f32>(1.12)), dn);
    let depth_fade = 0.82 + 0.18 * (1.0 - clamp(dist / max(extent_xy, 1.0), 0.0, 1.0));
    out.color = vec4<f32>(layer.color.rgb * lit, layer.color.a * alpha_mul * depth_fade);
    return out;
}

@fragment
fn fs_particle(in: ParticleVsOut) -> @location(0) vec4<f32> {
    let p = in.uv * 2.0 - vec2<f32>(1.0);
    let r = length(p);

    // Pre-compute simplistic 6-fold mathematical snowflake SDF in uniform control flow for fwidth().
    let sector = 1.04719755; // pi / 3
    let safe_ang = select(0.0, atan2(p.y, p.x), r > 1e-4);
    let folded = abs(safe_ang - sector * floor(safe_ang / sector + 0.5));
    let su = r * cos(folded);
    let sv = r * sin(folded);
    let d_core = r - 0.18;
    let spoke_u = clamp(su, 0.0, 0.86);
    let d_spoke = length(vec2<f32>(su - spoke_u, sv)) - 0.062;
    let b_dir = vec2<f32>(0.5, 0.8660254);
    let b_rel = vec2<f32>(su - 0.44, sv);
    let b_proj = clamp(dot(b_rel, b_dir), 0.0, 0.29);
    let d_branch = length(b_rel - b_dir * b_proj) - 0.048;
    let d_crystal = min(d_core, min(d_spoke, d_branch));
    let aa_crystal = max(fwidth(d_crystal), 0.045);

    var out_col = in.color;
    if (in.sprite_mode == 0u) {
        let texel = textureSampleLevel(sprite_tex, sprite_sampler, in.uv, 0.0);
        out_col = texel * in.color;
    } else if (in.sprite_mode == 1u) {
        // Pure mathematical tapered 2D raindrop streak (t = 0 leading tip, t = 1 upper motion tail).
        let t = clamp(in.uv.y, 0.0, 1.0);
        let width_prof = mix(0.76, 0.20, smoothstep(0.08, 1.0, t)) * smoothstep(0.0, 0.08, t);
        let x_norm = abs(p.x) / max(width_prof, 0.04);
        let core = exp(-x_norm * x_norm * 2.5);
        let tail_fade = (1.0 - smoothstep(0.14, 1.0, t)) * smoothstep(0.0, 0.06, t);
        let head_glint = exp(-((p.y + 0.72) * (p.y + 0.72) * 14.0 + p.x * p.x * 5.5)) * 0.35;
        let mask = clamp(core * tail_fade + head_glint * core, 0.0, 1.0);
        out_col = vec4<f32>(in.color.rgb + vec3<f32>(head_glint * 0.25), in.color.a * mask);
    } else if (in.sprite_mode == 2u) {
        // Simplistic mathematical 6-fold 2D snowflake crystal (zero asset files).
        let crystal = 1.0 - smoothstep(-aa_crystal, aa_crystal, d_crystal);
        let soft_halo = (1.0 - smoothstep(0.18, 0.94, r)) * 0.26;
        let mask = clamp(max(crystal, soft_halo), 0.0, 1.0) * (1.0 - smoothstep(0.92, 1.0, r));
        out_col = vec4<f32>(in.color.rgb, in.color.a * mask);
    } else {
        // ProceduralPellet: compact translucent sleet / graupel ice pellet.
        let pr = length(vec2<f32>(p.x * 1.18, p.y * 0.85));
        let mask = (1.0 - smoothstep(0.35, 0.95, pr));
        let glint = exp(-pr * pr * 5.0) * 0.25;
        out_col = vec4<f32>(in.color.rgb + vec3<f32>(glint), in.color.a * mask);
    }

    if (out_col.a <= 0.004) {
        discard;
    }
    return out_col;
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
    let c = textureSample(overlay_tex, overlay_sampler, in.uv);
    if (c.a <= 0.001) {
        return vec4<f32>(0.0);
    }
    return c;
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
    ) -> (bool, bool) {
        let mut bind_group_dirty = false;
        let mut deferred_any = false;
        let mut uploaded = 0usize;

        // First mark already-resident entries for visible keys as fresh this frame so LRU never evicts a fallback.
        for mesh in meshes {
            if let Some(entry) = self.resident.get_mut(&mesh.key) {
                if entry.len == mesh.vertices.len() as u32 {
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
                deferred_any = true;
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
        (bind_group_dirty, deferred_any)
    }

    fn base_vertex(&self, key: TileKeyId, revision: u64, expected_len: Option<u32>) -> Option<u32> {
        let entry = self.resident.get(&key)?;
        if entry.revision == revision || expected_len.is_some_and(|len| entry.len == len) {
            Some(entry.offset)
        } else {
            None
        }
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
    ) -> bool {
        for tex in textures {
            if let Some(slot) = self.slots_by_id.get_mut(&tex.id) {
                if slot.revision == tex.revision {
                    slot.last_used_frame = frame_idx;
                }
            }
        }
        let mut uploaded = 0usize;
        let mut deferred_any = false;
        for tex in textures {
            if let Some(slot) = self.slots_by_id.get(&tex.id) {
                if slot.revision == tex.revision {
                    continue;
                }
            }
            if uploaded >= budget {
                deferred_any = true;
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
        deferred_any
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
    cloud_noise_view: wgpu::TextureView,
    moon_view: wgpu::TextureView,
    skybox_cube_view: wgpu::TextureView,
    env_sampler: wgpu::Sampler,
    cloud_noise_sampler: wgpu::Sampler,
    star_buffer: wgpu::Buffer,
    star_capacity: usize,
    star_count: u32,
}

struct SrgbDownsampleLuts {
    to_lin_div4_scaled: [f32; 256],
    to_srgb: [u8; 16384],
}

fn srgb_downsample_luts() -> &'static SrgbDownsampleLuts {
    static LUTS: std::sync::OnceLock<SrgbDownsampleLuts> = std::sync::OnceLock::new();
    LUTS.get_or_init(|| {
        const MAX_IDX: f32 = 16383.0;
        let mut to_lin_div4_scaled = [0.0f32; 256];
        for (i, slot) in to_lin_div4_scaled.iter_mut().enumerate() {
            let x = i as f32 / 255.0;
            let lin = if x <= 0.04045 {
                x / 12.92
            } else {
                ((x + 0.055) / 1.055).powf(2.4)
            };
            *slot = lin * (0.25 * MAX_IDX);
        }
        let mut to_srgb = [0u8; 16384];
        for (i, slot) in to_srgb.iter_mut().enumerate() {
            let x = (i as f32 / MAX_IDX).clamp(0.0, 1.0);
            let s = if x <= 0.0031308 {
                x * 12.92
            } else {
                1.055 * x.powf(1.0 / 2.4) - 0.055
            };
            *slot = (s * 255.0).round().clamp(0.0, 255.0) as u8;
        }
        SrgbDownsampleLuts {
            to_lin_div4_scaled,
            to_srgb,
        }
    })
}

fn downsample_srgb_rows(
    src: &[u8],
    w: u32,
    h: u32,
    nw: u32,
    y_start: u32,
    out_slice: &mut [u8],
    luts: &SrgbDownsampleLuts,
) {
    let w_usize = w as usize;
    let nw_usize = nw as usize;
    let row_stride = w_usize * 4;
    let lin = &luts.to_lin_div4_scaled;
    let srgb = &luts.to_srgb;
    let fast_x = w >= nw * 2;
    let y_count = out_slice.len() / (nw_usize * 4);

    for dy in 0..y_count {
        let y = y_start + dy as u32;
        let sy0 = (y * 2).min(h - 1) as usize;
        let sy1 = (y * 2 + 1).min(h - 1) as usize;
        let row0 = &src[sy0 * row_stride..(sy0 + 1) * row_stride];
        let row1 = &src[sy1 * row_stride..(sy1 + 1) * row_stride];
        let out_row = &mut out_slice[dy * nw_usize * 4..(dy + 1) * nw_usize * 4];

        if fast_x {
            for x in 0..nw_usize {
                let s = x * 8;
                let a = &row0[s..s + 8];
                let b = &row1[s..s + 8];
                let ir = (lin[a[0] as usize]
                    + lin[a[4] as usize]
                    + lin[b[0] as usize]
                    + lin[b[4] as usize]
                    + 0.5) as usize;
                let ig = (lin[a[1] as usize]
                    + lin[a[5] as usize]
                    + lin[b[1] as usize]
                    + lin[b[5] as usize]
                    + 0.5) as usize;
                let ib = (lin[a[2] as usize]
                    + lin[a[6] as usize]
                    + lin[b[2] as usize]
                    + lin[b[6] as usize]
                    + 0.5) as usize;
                let alpha = (a[3] as u32 + a[7] as u32 + b[3] as u32 + b[7] as u32 + 2) >> 2;
                let d = x * 4;
                out_row[d] = srgb[ir.min(16383)];
                out_row[d + 1] = srgb[ig.min(16383)];
                out_row[d + 2] = srgb[ib.min(16383)];
                out_row[d + 3] = alpha as u8;
            }
        } else {
            for x in 0..nw_usize {
                let sx0 = ((x * 2) as u32).min(w - 1) as usize * 4;
                let sx1 = ((x * 2 + 1) as u32).min(w - 1) as usize * 4;
                let ir = (lin[row0[sx0] as usize]
                    + lin[row0[sx1] as usize]
                    + lin[row1[sx0] as usize]
                    + lin[row1[sx1] as usize]
                    + 0.5) as usize;
                let ig = (lin[row0[sx0 + 1] as usize]
                    + lin[row0[sx1 + 1] as usize]
                    + lin[row1[sx0 + 1] as usize]
                    + lin[row1[sx1 + 1] as usize]
                    + 0.5) as usize;
                let ib = (lin[row0[sx0 + 2] as usize]
                    + lin[row0[sx1 + 2] as usize]
                    + lin[row1[sx0 + 2] as usize]
                    + lin[row1[sx1 + 2] as usize]
                    + 0.5) as usize;
                let alpha = (row0[sx0 + 3] as u32
                    + row0[sx1 + 3] as u32
                    + row1[sx0 + 3] as u32
                    + row1[sx1 + 3] as u32
                    + 2)
                    >> 2;
                let d = x * 4;
                out_row[d] = srgb[ir.min(16383)];
                out_row[d + 1] = srgb[ig.min(16383)];
                out_row[d + 2] = srgb[ib.min(16383)];
                out_row[d + 3] = alpha as u8;
            }
        }
    }
}

fn parallel_downsample_rgba8<F>(w: u32, h: u32, row_fn: F) -> (u32, u32, Vec<u8>)
where
    F: Fn(u32, &mut [u8]) + Sync,
{
    let nw = (w / 2).max(1);
    let nh = (h / 2).max(1);
    let mut out = vec![0u8; (nw as usize) * (nh as usize) * 4];
    if nw * nh >= 262_144 && nh >= 64 {
        let workers = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .clamp(2, 6);
        let rows_per_chunk = ((nh as usize) + workers - 1) / workers;
        let chunk_bytes = rows_per_chunk * (nw as usize) * 4;
        std::thread::scope(|s| {
            for (chunk_idx, chunk) in out.chunks_mut(chunk_bytes).enumerate() {
                let y_start = (chunk_idx * rows_per_chunk) as u32;
                let rf = &row_fn;
                s.spawn(move || rf(y_start, chunk));
            }
        });
    } else {
        row_fn(0, &mut out);
    }
    (nw, nh, out)
}

fn downsample_srgb_rgba8_half(src: &[u8], w: u32, h: u32) -> (u32, u32, Vec<u8>) {
    let luts = srgb_downsample_luts();
    let nw = (w / 2).max(1);
    parallel_downsample_rgba8(w, h, |y_start, chunk| {
        downsample_srgb_rows(src, w, h, nw, y_start, chunk, luts);
    })
}

fn downsample_linear_rows(
    src: &[u8],
    w: u32,
    h: u32,
    nw: u32,
    y_start: u32,
    out_slice: &mut [u8],
) {
    let w_usize = w as usize;
    let nw_usize = nw as usize;
    let row_stride = w_usize * 4;
    let fast_x = w >= nw * 2;
    let y_count = out_slice.len() / (nw_usize * 4);

    for dy in 0..y_count {
        let y = y_start + dy as u32;
        let sy0 = (y * 2).min(h - 1) as usize;
        let sy1 = (y * 2 + 1).min(h - 1) as usize;
        let row0 = &src[sy0 * row_stride..(sy0 + 1) * row_stride];
        let row1 = &src[sy1 * row_stride..(sy1 + 1) * row_stride];
        let out_row = &mut out_slice[dy * nw_usize * 4..(dy + 1) * nw_usize * 4];
        if fast_x {
            for x in 0..nw_usize {
                let s = x * 8;
                let a = &row0[s..s + 8];
                let b = &row1[s..s + 8];
                let d = x * 4;
                for c in 0..4 {
                    out_row[d + c] =
                        ((a[c] as u32 + a[4 + c] as u32 + b[c] as u32 + b[4 + c] as u32 + 2) >> 2)
                            as u8;
                }
            }
        } else {
            for x in 0..nw_usize {
                let sx0 = ((x * 2) as u32).min(w - 1) as usize * 4;
                let sx1 = ((x * 2 + 1) as u32).min(w - 1) as usize * 4;
                let d = x * 4;
                for c in 0..4 {
                    out_row[d + c] = ((row0[sx0 + c] as u32
                        + row0[sx1 + c] as u32
                        + row1[sx0 + c] as u32
                        + row1[sx1 + c] as u32
                        + 2)
                        >> 2) as u8;
                }
            }
        }
    }
}

fn downsample_linear_rgba8_half(src: &[u8], w: u32, h: u32) -> (u32, u32, Vec<u8>) {
    let nw = (w / 2).max(1);
    parallel_downsample_rgba8(w, h, |y_start, chunk| {
        downsample_linear_rows(src, w, h, nw, y_start, chunk);
    })
}

fn downsample_srgb_rgba8_half_single(src: &[u8], w: u32, h: u32) -> (u32, u32, Vec<u8>) {
    let luts = srgb_downsample_luts();
    let nw = (w / 2).max(1);
    let nh = (h / 2).max(1);
    let mut out = vec![0u8; (nw as usize) * (nh as usize) * 4];
    downsample_srgb_rows(src, w, h, nw, 0, &mut out, luts);
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
    let mut base_override: Option<[Vec<u8>; 6]> = None;

    if size > max_dim && size > 1 {
        let mut owned: [Vec<u8>; 6] = std::array::from_fn(|i| {
            let expected = (size as usize) * (size as usize) * 4;
            if faces[i].len() >= expected {
                faces[i][..expected].to_vec()
            } else {
                vec![0u8; expected]
            }
        });
        while size > max_dim && size > 1 {
            let cur_size = size;
            let mut next_size = size;
            let next_owned: [Vec<u8>; 6] = std::thread::scope(|s| {
                let handles: [_; 6] = std::array::from_fn(|i| {
                    let buf = owned[i].as_slice();
                    s.spawn(move || downsample_srgb_rgba8_half_single(buf, cur_size, cur_size))
                });
                handles.map(|h| {
                    let (nw, _, down) = h.join().expect("cube downsample thread panicked");
                    next_size = nw;
                    down
                })
            });
            owned = next_owned;
            size = next_size;
        }
        base_override = Some(owned);
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

    let build_face_mips = |base: &[u8]| -> Vec<(u32, Vec<u8>)> {
        let mut mips = Vec::with_capacity((mip_level_count.saturating_sub(1)) as usize);
        if mip_level_count <= 1 {
            return mips;
        }
        let expected0 = (size as usize) * (size as usize) * 4;
        if base.len() < expected0 {
            return mips;
        }
        let (mut cur_w, _, cur_buf) =
            downsample_srgb_rgba8_half_single(&base[..expected0], size, size);
        mips.push((cur_w, cur_buf));
        for _ in 2..mip_level_count {
            let prev = &mips.last().expect("pushed mip").1;
            let (nw, _, next) = downsample_srgb_rgba8_half_single(prev, cur_w, cur_w);
            cur_w = nw;
            mips.push((cur_w, next));
        }
        mips
    };

    // Generate mip levels 1..N across all 6 cube faces in parallel without copying mip 0.
    let base_refs: [&[u8]; 6] = match &base_override {
        Some(owned) => std::array::from_fn(|i| owned[i].as_slice()),
        None => std::array::from_fn(|i| faces[i].as_slice()),
    };
    let extra_mips: [Vec<(u32, Vec<u8>)>; 6] = if mip_level_count > 1 && size >= 64 {
        let _ = srgb_downsample_luts();
        std::thread::scope(|s| {
            let handles: [_; 6] = std::array::from_fn(|i| {
                let base = base_refs[i];
                s.spawn(move || build_face_mips(base))
            });
            handles.map(|h| h.join().expect("cube mip thread panicked"))
        })
    } else {
        std::array::from_fn(|i| build_face_mips(base_refs[i]))
    };

    for layer in 0..6u32 {
        let expected0 = (size as usize) * (size as usize) * 4;
        if base_refs[layer as usize].len() >= expected0 {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &base_refs[layer as usize][..expected0],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(size * 4),
                    rows_per_image: Some(size),
                },
                wgpu::Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: 1,
                },
            );
        }
        for (idx, (mip_w, buf)) in extra_mips[layer as usize].iter().enumerate() {
            let mip = (idx + 1) as u32;
            let expected = (*mip_w as usize) * (*mip_w as usize) * 4;
            if buf.len() >= expected {
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
                    &buf[..expected],
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(*mip_w * 4),
                        rows_per_image: Some(*mip_w),
                    },
                    wgpu::Extent3d {
                        width: *mip_w,
                        height: *mip_w,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }
    }

    tex.create_view(&wgpu::TextureViewDescriptor {
        label: Some(label),
        dimension: Some(wgpu::TextureViewDimension::Cube),
        ..Default::default()
    })
}

#[derive(Clone, Copy)]
enum MipGenMode {
    Srgb,
    LinearBox,
}

fn create_rgba8_2d_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> wgpu::TextureView {
    create_rgba8_2d_view_opts(device, queue, label, width, height, rgba, MipGenMode::Srgb)
}

fn create_rgba8_2d_view_opts(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    width: u32,
    height: u32,
    rgba: &[u8],
    mip_mode: MipGenMode,
) -> wgpu::TextureView {
    let max_dim = device.limits().max_texture_dimension_2d.max(1);
    let mut w = width.max(1);
    let mut h = height.max(1);
    let mut scaled: Option<Vec<u8>> = None;
    let initial_expected = (w as usize) * (h as usize) * 4;
    if rgba.len() >= initial_expected && (w > max_dim || h > max_dim) {
        let mut buf = rgba[..initial_expected].to_vec();
        while (w > max_dim || h > max_dim) && w > 1 && h > 1 {
            let (nw, nh, next) = match mip_mode {
                MipGenMode::LinearBox => downsample_linear_rgba8_half(&buf, w, h),
                MipGenMode::Srgb => downsample_srgb_rgba8_half(&buf, w, h),
            };
            w = nw;
            h = nh;
            buf = next;
        }
        scaled = Some(buf);
    }
    let data: &[u8] = scaled.as_deref().unwrap_or(rgba);

    let expected = (w as usize) * (h as usize) * 4;
    let max_mips = if matches!(mip_mode, MipGenMode::LinearBox) {
        6
    } else {
        5
    };
    let mip_level_count = if data.len() >= expected && w >= 64 && h >= 64 {
        (32 - w.min(h).leading_zeros()).min(max_mips)
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
                let (nw, nh, next) = match mip_mode {
                    MipGenMode::LinearBox => downsample_linear_rgba8_half(mip_bytes, cur_w, cur_h),
                    MipGenMode::Srgb => downsample_srgb_rgba8_half(mip_bytes, cur_w, cur_h),
                };
                cur_w = nw;
                cur_h = nh;
                mip_owned = Some(next);
            }
        }
    }
    tex.create_view(&Default::default())
}

fn downsample_cloud_r8_half(src: &[u8], w: u32, h: u32) -> (u32, u32, Vec<u8>) {
    let nw = (w / 2).max(1);
    let nh = (h / 2).max(1);
    let mut out = vec![0u8; (nw as usize) * (nh as usize)];
    let w_usize = w as usize;
    let nw_usize = nw as usize;
    for y in 0..(nh as usize) {
        let y0 = ((y * 2) as u32).min(h - 1) as usize * w_usize;
        let y1 = ((y * 2 + 1) as u32).min(h - 1) as usize * w_usize;
        let out_row = &mut out[y * nw_usize..(y + 1) * nw_usize];
        for (x, dst) in out_row.iter_mut().enumerate() {
            let x0 = ((x * 2) as u32).min(w - 1) as usize;
            let x1 = ((x * 2 + 1) as u32).min(w - 1) as usize;
            let a = src[y0 + x0] as u32;
            let b = src[y0 + x1] as u32;
            let c = src[y1 + x0] as u32;
            let d = src[y1 + x1] as u32;
            let avg = (a + b + c + d + 2) >> 2;
            let m = a.max(b).max(c).max(d);
            *dst = ((avg * 7 + m + 4) >> 3) as u8;
        }
    }
    (nw, nh, out)
}

fn create_cloud_density_r8_view(
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
    let initial_px = (w as usize) * (h as usize);
    let mut r8 = vec![0u8; initial_px];
    if rgba.len() >= initial_px * 4 {
        for (i, px) in rgba[..initial_px * 4].chunks_exact(4).enumerate() {
            let r = px[0];
            let g = px[1];
            let b = px[2];
            let a = px[3];
            if a == 255 {
                r8[i] = ((77 * r as u32 + 150 * g as u32 + 29 * b as u32 + 128) >> 8) as u8;
            } else if a > 0 {
                let inv = 1.0 / 255.0;
                let af = a as f32 * inv;
                let luma = (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) * inv;
                let t = ((af - 0.97) * (1.0 / 0.029)).clamp(0.0, 1.0);
                let sm = t * t * (3.0 - 2.0 * t);
                let d = af * (luma * sm + (1.0 - sm));
                r8[i] = (d.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
        }
    }
    while (w > max_dim || h > max_dim) && w > 1 && h > 1 {
        let (nw, nh, next) = downsample_cloud_r8_half(&r8, w, h);
        w = nw;
        h = nh;
        r8 = next;
    }

    let mip_level_count = if w >= 64 && h >= 64 {
        (32 - w.min(h).leading_zeros()).min(6)
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
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    let mut cur_w = w;
    let mut cur_h = h;
    let mut cur_buf = r8;
    for mip in 0..mip_level_count {
        let mip_exp = (cur_w as usize) * (cur_h as usize);
        if cur_buf.len() >= mip_exp {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &tex,
                    mip_level: mip,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &cur_buf[..mip_exp],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(cur_w),
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
            let (nw, nh, next) = downsample_cloud_r8_half(&cur_buf, cur_w, cur_h);
            cur_w = nw;
            cur_h = nh;
            cur_buf = next;
        }
    }
    tex.create_view(&Default::default())
}

fn create_cloud_noise_lut_view(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    const DIM: u32 = 128;
    let hash_cell = |ix: u32, iy: u32, period: u32, seed: u32| -> f32 {
        let x = ix % period;
        let y = iy % period;
        let mut h = x
            .wrapping_mul(0x9e37_79b1)
            .wrapping_add(y.wrapping_mul(0x85eb_ca77))
            .wrapping_add(seed.wrapping_mul(0xc2b2_ae3d));
        h ^= h >> 16;
        h = h.wrapping_mul(0x27d4_eb2f);
        h ^= h >> 15;
        (h & 0xffff) as f32 * (1.0 / 65535.0)
    };
    let sample_periodic = |u: f32, v: f32, period: u32, seed: u32| -> u8 {
        let x = u * (period as f32);
        let y = v * (period as f32);
        let ix = x.floor() as u32;
        let iy = y.floor() as u32;
        let fx = x - x.floor();
        let fy = y - y.floor();
        let ux = fx * fx * (3.0 - 2.0 * fx);
        let uy = fy * fy * (3.0 - 2.0 * fy);
        let n00 = hash_cell(ix, iy, period, seed);
        let n10 = hash_cell(ix + 1, iy, period, seed);
        let n01 = hash_cell(ix, iy + 1, period, seed);
        let n11 = hash_cell(ix + 1, iy + 1, period, seed);
        let nx0 = n00 + (n10 - n00) * ux;
        let nx1 = n01 + (n11 - n01) * ux;
        let val = nx0 + (nx1 - nx0) * uy;
        (val.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
    };

    let mut rgba = vec![0u8; (DIM * DIM * 4) as usize];
    let inv = 1.0 / (DIM as f32);
    for y in 0..DIM {
        let v = (y as f32 + 0.5) * inv;
        for x in 0..DIM {
            let u = (x as f32 + 0.5) * inv;
            let idx = ((y * DIM + x) * 4) as usize;
            rgba[idx] = sample_periodic(u, v, 32, 1);
            rgba[idx + 1] = sample_periodic(u, v, 96, 2);
            rgba[idx + 2] = sample_periodic(u, v, 32, 3);
            rgba[idx + 3] = sample_periodic(u, v, 32, 4);
        }
    }

    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("strata_env_cloud_noise_lut"),
        size: wgpu::Extent3d {
            width: DIM,
            height: DIM,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(DIM * 4),
            rows_per_image: Some(DIM),
        },
        wgpu::Extent3d {
            width: DIM,
            height: DIM,
            depth_or_array_layers: 1,
        },
    );
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

struct CachedParticleTexture {
    revision: u64,
    width: u32,
    height: u32,
    _texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

/// Hardware-accelerated `wgpu` 3D renderer capable of rendering directly into any `TextureView`
/// (swapchain surface) or offscreen RGBA target.
pub struct Wgpu3DRenderer {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    color_format: wgpu::TextureFormat,
    camera_buffer: wgpu::Buffer,
    sky_pipeline: wgpu::RenderPipeline,
    sky_opaque_pipeline: wgpu::RenderPipeline,
    sky_bind_group_layout: wgpu::BindGroupLayout,
    sky_bind_group: wgpu::BindGroup,
    star_pipeline: wgpu::RenderPipeline,
    star_bind_group_layout: wgpu::BindGroupLayout,
    star_bind_group: wgpu::BindGroup,
    cloud_pipeline: wgpu::RenderPipeline,
    cloud_inside_pipeline: wgpu::RenderPipeline,
    cloud_twosided_pipeline: wgpu::RenderPipeline,
    cloud_bind_group_layout: wgpu::BindGroupLayout,
    cloud_bind_group: wgpu::BindGroup,
    cloud_vbuf: wgpu::Buffer,
    cloud_ibuf: wgpu::Buffer,
    cloud_index_count: u32,
    env_state: GpuEnvironmentState,
    terrain_pipeline: wgpu::RenderPipeline,
    terrain_opaque_pipeline: wgpu::RenderPipeline,
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
    geo_object_pipeline: wgpu::RenderPipeline,
    geo_object_transparent_pipeline: wgpu::RenderPipeline,
    geo_object_bind_group_layout: wgpu::BindGroupLayout,
    geo_object_bind_group: wgpu::BindGroup,
    geo_instance_buffer: wgpu::Buffer,
    geo_instance_capacity: usize,
    geo_meshes: HashMap<u64, CachedDynamicGpuMesh>,
    particle_pipeline: wgpu::RenderPipeline,
    particle_common_bgl: wgpu::BindGroupLayout,
    particle_tex_bgl: wgpu::BindGroupLayout,
    particle_sampler: wgpu::Sampler,
    particle_uniform_buffer: wgpu::Buffer,
    particle_uniform_capacity: usize,
    particle_uniform_stride: u32,
    particle_common_bind_group: wgpu::BindGroup,
    particle_default_tex_bg: wgpu::BindGroup,
    particle_textures: HashMap<u64, CachedParticleTexture>,
    overlay_pipeline: wgpu::RenderPipeline,
    overlay_main_pass_pipeline: wgpu::RenderPipeline,
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
            mipmap_filter: wgpu::FilterMode::Linear,
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
        let cloud_noise_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("strata_3d_cloud_noise_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
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
        let cloud_map_view = create_cloud_density_r8_view(
            &device,
            &queue,
            "strata_env_cloud_fallback",
            1,
            1,
            &[255, 255, 255, 0],
        );
        let cloud_noise_view = create_cloud_noise_lut_view(&device, &queue);
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
            cloud_noise_view,
            moon_view,
            skybox_cube_view,
            env_sampler,
            cloud_noise_sampler,
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
        let make_sky_pipeline = |label: &str, blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
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
                        blend,
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
            })
        };
        let sky_pipeline = make_sky_pipeline(
            "strata_3d_sky_pipeline",
            Some(wgpu::BlendState::ALPHA_BLENDING),
        );
        let sky_opaque_pipeline = make_sky_pipeline("strata_3d_sky_opaque_pipeline", None);

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
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
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
                ],
            });
        let cloud_bind_group = Self::create_cloud_bind_group(
            &device,
            &cloud_bind_group_layout,
            "strata_3d_cloud_bg",
            &camera_buffer,
            &env_state.cloud_map_view,
            &env_state.env_sampler,
            &env_state.cloud_noise_view,
            &env_state.cloud_noise_sampler,
        );
        let cloud_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("strata_3d_cloud_pl"),
            bind_group_layouts: &[&cloud_bind_group_layout],
            push_constant_ranges: &[],
        });
        let make_cloud_pipeline = |label: &str, cull_mode: Option<wgpu::Face>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
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
                    cull_mode,
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
            })
        };
        let cloud_pipeline =
            make_cloud_pipeline("strata_3d_cloud_pipeline", Some(wgpu::Face::Back));
        let cloud_inside_pipeline =
            make_cloud_pipeline("strata_3d_cloud_inside_pipeline", Some(wgpu::Face::Front));
        let cloud_twosided_pipeline =
            make_cloud_pipeline("strata_3d_cloud_twosided_pipeline", None);
        let (ico_v, ico_i) = build_unit_icosphere(4);
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
        let make_terrain_pipeline = |label: &str, blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&terrain_layout),
                vertex: wgpu::VertexState {
                    module: &terrain_shader,
                    entry_point: Some("vs_terrain"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &terrain_shader,
                    entry_point: Some(if blend.is_none() {
                        "fs_terrain_opaque"
                    } else {
                        "fs_terrain"
                    }),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: color_format,
                        blend,
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
            })
        };
        let terrain_pipeline = make_terrain_pipeline(
            "strata_3d_terrain_pipeline",
            Some(wgpu::BlendState::ALPHA_BLENDING),
        );
        let terrain_opaque_pipeline =
            make_terrain_pipeline("strata_3d_terrain_opaque_pipeline", None);

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

        // Hardware-instanced geodetic 3D object pipeline (storage buffer + double-single RTE + 3x3 cofactor normals)
        let geo_object_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("strata_3d_geo_object_wgsl"),
            source: wgpu::ShaderSource::Wgsl(GEO_OBJECT_WGSL.into()),
        });
        let geo_instance_capacity = 256usize;
        let geo_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("strata_3d_geo_instances"),
            size: (geo_instance_capacity * std::mem::size_of::<GpuGeoInstance>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let geo_object_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("strata_3d_geo_object_bgl"),
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
        let geo_object_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("strata_3d_geo_object_bg"),
            layout: &geo_object_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: geo_instance_buffer.as_entire_binding(),
                },
            ],
        });
        let geo_object_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("strata_3d_geo_object_pl"),
            bind_group_layouts: &[&geo_object_bind_group_layout],
            push_constant_ranges: &[],
        });
        let make_geo_pipeline = |label: &str, depth_write_enabled: bool, blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&geo_object_layout),
                vertex: wgpu::VertexState {
                    module: &geo_object_shader,
                    entry_point: Some("vs_geo_object"),
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
                    module: &geo_object_shader,
                    entry_point: Some("fs_geo_object"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: color_format,
                        blend,
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
                    depth_write_enabled,
                    depth_compare: wgpu::CompareFunction::LessEqual,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            })
        };
        let geo_object_pipeline =
            make_geo_pipeline("strata_3d_geo_object_pipeline", true, None);
        let geo_object_transparent_pipeline = make_geo_pipeline(
            "strata_3d_geo_object_transparent_pipeline",
            false,
            Some(wgpu::BlendState::ALPHA_BLENDING),
        );

        // GPU-instanced 2D camera-local toroidal particle volume pipeline (rain, snow, custom 2D sprites)
        let particle_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("strata_3d_particle_volume_wgsl"),
            source: wgpu::ShaderSource::Wgsl(PARTICLE_VOLUME_WGSL.into()),
        });
        let ubo_align = device.limits().min_uniform_buffer_offset_alignment.max(256);
        let particle_u_size = std::mem::size_of::<GpuParticleLayerUniforms>() as u32;
        let particle_uniform_stride = particle_u_size.div_ceil(ubo_align) * ubo_align;
        let particle_uniform_capacity = 16usize;
        let particle_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("strata_3d_particle_uniforms"),
            size: (particle_uniform_capacity as u64) * (particle_uniform_stride as u64),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let particle_common_bgl =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("strata_3d_particle_common_bgl"),
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
                            min_binding_size: std::num::NonZeroU64::new(particle_u_size as u64),
                        },
                        count: None,
                    },
                ],
            });
        let particle_common_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("strata_3d_particle_common_bg"),
            layout: &particle_common_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &particle_uniform_buffer,
                        offset: 0,
                        size: std::num::NonZeroU64::new(particle_u_size as u64),
                    }),
                },
            ],
        });
        let particle_tex_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("strata_3d_particle_tex_bgl"),
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
        let particle_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("strata_3d_particle_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let default_p_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("strata_3d_particle_default_1x1"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &default_p_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[255u8, 255, 255, 255],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let default_p_view = default_p_tex.create_view(&Default::default());
        let particle_default_tex_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("strata_3d_particle_default_tex_bg"),
            layout: &particle_tex_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&default_p_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&particle_sampler),
                },
            ],
        });
        let particle_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("strata_3d_particle_pl"),
            bind_group_layouts: &[&particle_common_bgl, &particle_tex_bgl],
            push_constant_ranges: &[],
        });
        let particle_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("strata_3d_particle_pipeline"),
            layout: Some(&particle_layout),
            vertex: wgpu::VertexState {
                module: &particle_shader,
                entry_point: Some("vs_particle"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &particle_shader,
                entry_point: Some("fs_particle"),
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
        let overlay_main_pass_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("strata_2d_overlay_main_pass_pipeline"),
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
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: false,
                    depth_compare: wgpu::CompareFunction::Always,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
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
            sky_opaque_pipeline,
            sky_bind_group_layout,
            sky_bind_group,
            star_pipeline,
            star_bind_group_layout,
            star_bind_group,
            cloud_pipeline,
            cloud_inside_pipeline,
            cloud_twosided_pipeline,
            cloud_bind_group_layout,
            cloud_bind_group,
            cloud_vbuf,
            cloud_ibuf,
            cloud_index_count,
            env_state,
            terrain_pipeline,
            terrain_opaque_pipeline,
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
            geo_object_pipeline,
            geo_object_transparent_pipeline,
            geo_object_bind_group_layout,
            geo_object_bind_group,
            geo_instance_buffer,
            geo_instance_capacity,
            geo_meshes: HashMap::new(),
            particle_pipeline,
            particle_common_bgl,
            particle_tex_bgl,
            particle_sampler,
            particle_uniform_buffer,
            particle_uniform_capacity,
            particle_uniform_stride,
            particle_common_bind_group,
            particle_default_tex_bg,
            particle_textures: HashMap::new(),
            overlay_pipeline,
            overlay_main_pass_pipeline,
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

    #[allow(clippy::too_many_arguments)]
    fn create_cloud_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        label: &str,
        camera_buffer: &wgpu::Buffer,
        cloud_view: &wgpu::TextureView,
        cloud_sampler: &wgpu::Sampler,
        noise_view: &wgpu::TextureView,
        noise_sampler: &wgpu::Sampler,
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
                    resource: wgpu::BindingResource::TextureView(cloud_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(cloud_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(noise_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(noise_sampler),
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
        let perf = gpu_perf_logging_enabled();
        let mut terrain_rebind = false;

        if let Some(ref land) = env.land_mask {
            let key = Some((land.id, land.revision));
            if self.env_state.land_mask_id_rev != key {
                let t0 = std::time::Instant::now();
                self.env_state.land_mask_view = create_rgba8_2d_view_opts(
                    &self.device,
                    &self.queue,
                    "strata_env_land_mask",
                    land.width,
                    land.height,
                    &land.rgba8,
                    MipGenMode::LinearBox,
                );
                if perf {
                    eprintln!("[perf] env land_mask {}x{} -> {:.2}ms", land.width, land.height, t0.elapsed().as_secs_f64() * 1000.0);
                }
                self.env_state.land_mask_id_rev = key;
                terrain_rebind = true;
            }
        }

        if let Some(ref clouds) = env.cloud_map {
            let key = Some((clouds.id, clouds.revision));
            if self.env_state.cloud_map_id_rev != key {
                let t0 = std::time::Instant::now();
                self.env_state.cloud_map_view = create_cloud_density_r8_view(
                    &self.device,
                    &self.queue,
                    "strata_env_cloud_map",
                    clouds.width,
                    clouds.height,
                    &clouds.rgba8,
                );
                if perf {
                    eprintln!("[perf] env cloud_map {}x{} -> {:.2}ms", clouds.width, clouds.height, t0.elapsed().as_secs_f64() * 1000.0);
                }
                self.env_state.cloud_map_id_rev = key;
                self.cloud_bind_group = Self::create_cloud_bind_group(
                    &self.device,
                    &self.cloud_bind_group_layout,
                    "strata_3d_cloud_bg",
                    &self.camera_buffer,
                    &self.env_state.cloud_map_view,
                    &self.env_state.env_sampler,
                    &self.env_state.cloud_noise_view,
                    &self.env_state.cloud_noise_sampler,
                );
            }
        }

        let mut sky_rebind = false;

        if let Some(ref moon) = env.moon_texture {
            let key = Some((moon.id, moon.revision));
            if self.env_state.moon_tex_id_rev != key {
                let t0 = std::time::Instant::now();
                self.env_state.moon_view = create_rgba8_2d_view(
                    &self.device,
                    &self.queue,
                    "strata_env_moon_tex",
                    moon.width,
                    moon.height,
                    &moon.rgba8,
                );
                if perf {
                    eprintln!("[perf] env moon {}x{} -> {:.2}ms", moon.width, moon.height, t0.elapsed().as_secs_f64() * 1000.0);
                }
                self.env_state.moon_tex_id_rev = key;
                sky_rebind = true;
            }
        }

        match &env.skybox_cubemap {
            Some(cube) => {
                let key = Some((cube.id, cube.revision));
                if self.env_state.skybox_cube_id_rev != key {
                    let t0 = std::time::Instant::now();
                    self.env_state.skybox_cube_view = create_rgba8_cube_view(
                        &self.device,
                        &self.queue,
                        "strata_env_skybox_cube",
                        cube.face_size,
                        &cube.faces_rgba8,
                        wgpu::TextureFormat::Rgba8Unorm,
                    );
                    if perf {
                        eprintln!("[perf] env skybox 6x{} -> {:.2}ms", cube.face_size, t0.elapsed().as_secs_f64() * 1000.0);
                    }
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
        self.render_scene_and_overlay_to_view(
            scene,
            target_view,
            target_width,
            target_height,
            viewport,
            unlimited_upload_budget,
            None,
        );
    }

    /// Draws `scene` and optionally composites the cached 2D HUD `overlay_bind_group` into
    /// `target_view` using a single `CommandEncoder` and a single `queue.submit`.
    pub fn render_scene_and_overlay_to_view(
        &mut self,
        scene: &Scene3D,
        target_view: &wgpu::TextureView,
        target_width: u32,
        target_height: u32,
        viewport: Option<[f32; 4]>,
        unlimited_upload_budget: bool,
        overlay_bind_group: Option<&wgpu::BindGroup>,
    ) {
        let perf = gpu_perf_logging_enabled();
        let t_start = std::time::Instant::now();
        let mut t_env = 0.0f64;
        let mut t_slab = 0.0f64;
        let mut t_atlas = 0.0f64;
        let mut t_inst = 0.0f64;
        let mut t_dyn = 0.0f64;

        self.frame_index += 1;
        let frame_idx = self.frame_index;
        let mesh_budget = usize::MAX;
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
        let mut all_terrain_opaque = true;
        let mut grid_topology: Option<(u32, bool)> = None;
        let mut dynamic_draws: Vec<(u64, [f64; 3], [f32; 4], bool)> = Vec::new();
        let mut geo_opaque_batches: Vec<(u64, std::ops::Range<u32>)> = Vec::new();
        let mut geo_transparent_batches: Vec<(u64, std::ops::Range<u32>)> = Vec::new();
        let mut staged_geo_instances: Vec<GpuGeoInstance> = Vec::new();

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

            let tp0 = std::time::Instant::now();
            let env_dirty = if let Some(ref env) = frame.environment {
                self.upload_environment_assets(env)
            } else {
                false
            };
            t_env = tp0.elapsed().as_secs_f64() * 1000.0;

            let tp1 = std::time::Instant::now();
            let (bg_dirty, mesh_deferred) = self.slab.upload_meshes(
                &self.device,
                &self.queue,
                &frame.meshes,
                frame_idx,
                mesh_budget,
            );
            t_slab = tp1.elapsed().as_secs_f64() * 1000.0;

            let tp2 = std::time::Instant::now();
            let tex_deferred = self
                .atlas
                .upload_textures(&self.queue, &frame.textures, frame_idx, tex_budget);
            t_atlas = tp2.elapsed().as_secs_f64() * 1000.0;

            if mesh_deferred || tex_deferred {
                if let Some(retained) = scene.retained_handle() {
                    retained.request_gpu_continuation();
                }
            }

            let tp3 = std::time::Instant::now();
            if let Some(first_m) = frame.meshes.first() {
                grid_topology = Some((first_m.grid_res, first_m.has_skirts));
            }
            let expected_verts = frame.meshes.first().map(|m| m.vertices.len() as u32);
            let mut instances_gpu: Vec<GpuTileInstance> = Vec::with_capacity(frame.instances.len());
            for inst in &frame.instances {
                let Some(base_v) =
                    self.slab
                        .base_vertex(inst.key, inst.mesh_revision, expected_verts)
                else {
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
                if eff_tint[3] < 0.999 {
                    all_terrain_opaque = false;
                }
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
            t_inst = tp3.elapsed().as_secs_f64() * 1000.0;

            let tp4 = std::time::Instant::now();
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

            for asset in frame.geo_mesh_assets.iter() {
                if asset.vertices.is_empty() || asset.indices.is_empty() {
                    continue;
                }
                let vb_bytes: &[u8] = bytemuck::cast_slice(&asset.vertices);
                let ib_bytes: &[u8] = bytemuck::cast_slice(&asset.indices);
                let mut need_recreate = true;
                if let Some(existing) = self.geo_meshes.get_mut(&asset.id) {
                    if existing.revision == asset.revision {
                        need_recreate = false;
                    } else if existing.vbuf.size() >= vb_bytes.len() as u64
                        && existing.ibuf.size() >= ib_bytes.len() as u64
                    {
                        self.queue.write_buffer(&existing.vbuf, 0, vb_bytes);
                        self.queue.write_buffer(&existing.ibuf, 0, ib_bytes);
                        existing.revision = asset.revision;
                        existing.index_count = asset.indices.len() as u32;
                        need_recreate = false;
                    }
                }
                if need_recreate {
                    let vbuf = self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("strata_3d_geo_vb"),
                        size: (vb_bytes.len() as u64).next_power_of_two().max(256),
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    self.queue.write_buffer(&vbuf, 0, vb_bytes);
                    let ibuf = self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("strata_3d_geo_ib"),
                        size: (ib_bytes.len() as u64).next_power_of_two().max(64),
                        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    self.queue.write_buffer(&ibuf, 0, ib_bytes);
                    self.geo_meshes.insert(
                        asset.id,
                        CachedDynamicGpuMesh {
                            revision: asset.revision,
                            vbuf,
                            ibuf,
                            index_count: asset.indices.len() as u32,
                        },
                    );
                }
            }

            if !frame.geo_instances.is_empty() {
                let mut opaque_items: Vec<(u64, GpuGeoInstance)> =
                    Vec::with_capacity(frame.geo_instances.len());
                let mut transparent_items: Vec<(u64, GpuGeoInstance)> = Vec::new();

                for inst in frame.geo_instances.iter() {
                    if !self.geo_meshes.contains_key(&inst.mesh_id) {
                        continue;
                    }
                    let (ax_hi, ax_lo) = split_f64_to_f32(inst.anchor_ecef[0]);
                    let (ay_hi, ay_lo) = split_f64_to_f32(inst.anchor_ecef[1]);
                    let (az_hi, az_lo) = split_f64_to_f32(inst.anchor_ecef[2]);
                    let gpu_inst = GpuGeoInstance {
                        anchor_high: [ax_hi, ay_hi, az_hi, 0.0],
                        anchor_low: [ax_lo, ay_lo, az_lo, 0.0],
                        col0: [
                            inst.model_cols[0][0],
                            inst.model_cols[0][1],
                            inst.model_cols[0][2],
                            inst.roughness,
                        ],
                        col1: [
                            inst.model_cols[1][0],
                            inst.model_cols[1][1],
                            inst.model_cols[1][2],
                            inst.metalness,
                        ],
                        col2: [
                            inst.model_cols[2][0],
                            inst.model_cols[2][1],
                            inst.model_cols[2][2],
                            if inst.unlit { 1.0 } else { 0.0 },
                        ],
                        base_color: inst.base_color,
                        emissive: [
                            inst.emissive_rgb[0],
                            inst.emissive_rgb[1],
                            inst.emissive_rgb[2],
                            inst.accent_mix,
                        ],
                    };
                    if inst.base_color[3] >= 0.999 {
                        opaque_items.push((inst.mesh_id, gpu_inst));
                    } else {
                        transparent_items.push((inst.mesh_id, gpu_inst));
                    }
                }

                opaque_items.sort_by_key(|(mesh_id, _)| *mesh_id);
                staged_geo_instances.reserve(opaque_items.len() + transparent_items.len());

                let push_batches = |items: Vec<(u64, GpuGeoInstance)>,
                                    staged: &mut Vec<GpuGeoInstance>,
                                    batches: &mut Vec<(u64, std::ops::Range<u32>)>| {
                    for (mesh_id, gpu_inst) in items {
                        let idx = staged.len() as u32;
                        staged.push(gpu_inst);
                        if let Some((last_id, range)) = batches.last_mut() {
                            if *last_id == mesh_id && range.end == idx {
                                range.end = idx + 1;
                                continue;
                            }
                        }
                        batches.push((mesh_id, idx..(idx + 1)));
                    }
                };
                push_batches(opaque_items, &mut staged_geo_instances, &mut geo_opaque_batches);
                push_batches(
                    transparent_items,
                    &mut staged_geo_instances,
                    &mut geo_transparent_batches,
                );
            }
            t_dyn = tp4.elapsed().as_secs_f64() * 1000.0;
        }

        if !staged_geo_instances.is_empty() {
            if staged_geo_instances.len() > self.geo_instance_capacity {
                let new_cap = staged_geo_instances.len().next_power_of_two().max(256);
                self.geo_instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("strata_3d_geo_instances_grown"),
                    size: (new_cap * std::mem::size_of::<GpuGeoInstance>()) as u64,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.geo_instance_capacity = new_cap;
                self.geo_object_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("strata_3d_geo_object_bg_grown"),
                    layout: &self.geo_object_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: self.camera_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: self.geo_instance_buffer.as_entire_binding(),
                        },
                    ],
                });
            }
            self.queue.write_buffer(
                &self.geo_instance_buffer,
                0,
                bytemuck::cast_slice(&staged_geo_instances),
            );
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

        let mut particle_draws: Vec<(u32, Option<u64>)> = Vec::new();
        if let Some(frame) = terrain_frame_opt.as_ref() {
            if !frame.particle_layers.is_empty() {
                for tex in frame.particle_textures.iter() {
                    if tex.width == 0
                        || tex.height == 0
                        || tex.rgba8.len() < (tex.width as usize) * (tex.height as usize) * 4
                    {
                        continue;
                    }
                    let need_upload = match self.particle_textures.get(&tex.id) {
                        Some(cached) => {
                            cached.revision != tex.revision
                                || cached.width != tex.width
                                || cached.height != tex.height
                        }
                        None => true,
                    };
                    if need_upload {
                        let gpu_tex = self.device.create_texture(&wgpu::TextureDescriptor {
                            label: Some("strata_3d_particle_sprite_tex"),
                            size: wgpu::Extent3d {
                                width: tex.width,
                                height: tex.height,
                                depth_or_array_layers: 1,
                            },
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format: wgpu::TextureFormat::Rgba8Unorm,
                            usage: wgpu::TextureUsages::TEXTURE_BINDING
                                | wgpu::TextureUsages::COPY_DST,
                            view_formats: &[],
                        });
                        self.queue.write_texture(
                            wgpu::TexelCopyTextureInfo {
                                texture: &gpu_tex,
                                mip_level: 0,
                                origin: wgpu::Origin3d::ZERO,
                                aspect: wgpu::TextureAspect::All,
                            },
                            &tex.rgba8[..(tex.width as usize) * (tex.height as usize) * 4],
                            wgpu::TexelCopyBufferLayout {
                                offset: 0,
                                bytes_per_row: Some(tex.width * 4),
                                rows_per_image: Some(tex.height),
                            },
                            wgpu::Extent3d {
                                width: tex.width,
                                height: tex.height,
                                depth_or_array_layers: 1,
                            },
                        );
                        let view = gpu_tex.create_view(&Default::default());
                        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                            label: Some("strata_3d_particle_sprite_bg"),
                            layout: &self.particle_tex_bgl,
                            entries: &[
                                wgpu::BindGroupEntry {
                                    binding: 0,
                                    resource: wgpu::BindingResource::TextureView(&view),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 1,
                                    resource: wgpu::BindingResource::Sampler(
                                        &self.particle_sampler,
                                    ),
                                },
                            ],
                        });
                        self.particle_textures.insert(
                            tex.id,
                            CachedParticleTexture {
                                revision: tex.revision,
                                width: tex.width,
                                height: tex.height,
                                _texture: gpu_tex,
                                bind_group: bg,
                            },
                        );
                    }
                }

                let norm3 = |v: [f32; 3], fallback: [f32; 3]| -> [f32; 3] {
                    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
                    if l > 1e-6 {
                        [v[0] / l, v[1] / l, v[2] / l]
                    } else {
                        fallback
                    }
                };
                let cam_right = norm3(
                    [cam.view_rot[0][0], cam.view_rot[1][0], cam.view_rot[2][0]],
                    [1.0, 0.0, 0.0],
                );
                let cam_up = norm3(
                    [cam.view_rot[0][1], cam.view_rot[1][1], cam.view_rot[2][1]],
                    [0.0, 1.0, 0.0],
                );
                let meters_per_px_at_1m = 2.0 / (vp_h * cam.proj[1][1].abs().max(0.05));

                let active_layers: Vec<&RetainedParticleLayer> = frame
                    .particle_layers
                    .iter()
                    .filter(|l| l.particle_count > 0 && l.color[3] > 0.002)
                    .collect();
                if !active_layers.is_empty() {
                    let stride = self.particle_uniform_stride as usize;
                    if active_layers.len() > self.particle_uniform_capacity {
                        let new_cap = active_layers.len().next_power_of_two().max(16);
                        let u_size = std::mem::size_of::<GpuParticleLayerUniforms>() as u64;
                        self.particle_uniform_buffer =
                            self.device.create_buffer(&wgpu::BufferDescriptor {
                                label: Some("strata_3d_particle_uniforms_grown"),
                                size: (new_cap * stride) as u64,
                                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                                mapped_at_creation: false,
                            });
                        self.particle_uniform_capacity = new_cap;
                        self.particle_common_bind_group =
                            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                                label: Some("strata_3d_particle_common_bg_grown"),
                                layout: &self.particle_common_bgl,
                                entries: &[
                                    wgpu::BindGroupEntry {
                                        binding: 0,
                                        resource: self.camera_buffer.as_entire_binding(),
                                    },
                                    wgpu::BindGroupEntry {
                                        binding: 1,
                                        resource: wgpu::BindingResource::Buffer(
                                            wgpu::BufferBinding {
                                                buffer: &self.particle_uniform_buffer,
                                                offset: 0,
                                                size: std::num::NonZeroU64::new(u_size),
                                            },
                                        ),
                                    },
                                ],
                            });
                    }
                    let mut staged = vec![0u8; active_layers.len() * stride];
                    for (i, layer) in active_layers.iter().enumerate() {
                        let (sprite_mode, tex_id_opt) = match layer.sprite {
                            RetainedParticleSprite::Textured(tid) => (0u32, Some(tid)),
                            RetainedParticleSprite::ProceduralStreak => (1u32, None),
                            RetainedParticleSprite::ProceduralSnowflake => (2u32, None),
                            RetainedParticleSprite::ProceduralPellet => (3u32, None),
                        };
                        let billboard_mode = match layer.billboard {
                            RetainedParticleBillboard::VelocityAligned => 0u32,
                            RetainedParticleBillboard::CameraFacing => 1u32,
                        };
                        let gpu_u = GpuParticleLayerUniforms {
                            basis_east: [
                                layer.basis_east[0],
                                layer.basis_east[1],
                                layer.basis_east[2],
                                layer.wrapped_phase[0],
                            ],
                            basis_north: [
                                layer.basis_north[0],
                                layer.basis_north[1],
                                layer.basis_north[2],
                                layer.wrapped_phase[1],
                            ],
                            basis_up: [
                                layer.basis_up[0],
                                layer.basis_up[1],
                                layer.basis_up[2],
                                layer.wrapped_phase[2],
                            ],
                            cam_right_mpx: [
                                cam_right[0],
                                cam_right[1],
                                cam_right[2],
                                meters_per_px_at_1m,
                            ],
                            cam_up_pad: [cam_up[0], cam_up[1], cam_up[2], 0.0],
                            altitude_bounds: layer.altitude_bounds,
                            volume_params: layer.volume_params,
                            motion_params: layer.motion_params,
                            size_params: layer.size_params,
                            flutter_clamp: layer.flutter_clamp,
                            color: layer.color,
                            meta: [sprite_mode, billboard_mode, layer.seed, layer.particle_count],
                        };
                        let bytes = bytemuck::bytes_of(&gpu_u);
                        staged[i * stride..i * stride + bytes.len()].copy_from_slice(bytes);
                        particle_draws.push((layer.particle_count, tex_id_opt));
                    }
                    self.queue
                        .write_buffer(&self.particle_uniform_buffer, 0, &staged);
                }
            }
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

        let tp_pass = std::time::Instant::now();
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
                        store: wgpu::StoreOp::Discard,
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

            let terrain_first = draw_count > 0
                && all_terrain_opaque
                && !matches!(cam.background, Background3D::Transparent);

            if terrain_first {
                if let Some((ref idx_buf, idx_count)) = topology_res {
                    if idx_count > 0 {
                        pass.set_pipeline(&self.terrain_opaque_pipeline);
                        pass.set_bind_group(0, &self.terrain_bind_group, &[]);
                        pass.set_index_buffer(idx_buf.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..idx_count, 0, 0..draw_count as u32);
                    }
                }
            }

            if matches!(cam.background, Background3D::VerticalGradient { .. })
                || cam.stars_visible
                || cam.sun_visible
                || cam.moon_visible
            {
                let sky_opaque = cam.stars_visible || cam.sun_visible || cam.moon_visible;
                pass.set_pipeline(if sky_opaque {
                    &self.sky_opaque_pipeline
                } else {
                    &self.sky_pipeline
                });
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

            if !terrain_first && draw_count > 0 {
                if let Some((ref idx_buf, idx_count)) = topology_res {
                    if idx_count > 0 {
                        pass.set_pipeline(&self.terrain_pipeline);
                        pass.set_bind_group(0, &self.terrain_bind_group, &[]);
                        pass.set_index_buffer(idx_buf.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..idx_count, 0, 0..draw_count as u32);
                    }
                }
            }

            if !geo_opaque_batches.is_empty() {
                pass.set_pipeline(&self.geo_object_pipeline);
                pass.set_bind_group(0, &self.geo_object_bind_group, &[]);
                for (mesh_id, range) in &geo_opaque_batches {
                    let Some(gpu_mesh) = self.geo_meshes.get(mesh_id) else {
                        continue;
                    };
                    pass.set_vertex_buffer(0, gpu_mesh.vbuf.slice(..));
                    pass.set_index_buffer(gpu_mesh.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..gpu_mesh.index_count, 0, range.clone());
                }
            }

            if (cam.cloud_layer_opacity > 0.004 || cam.local_cloud_opacity > 0.004)
                && self.cloud_index_count > 0
            {
                let eye_r2 = cam.eye_ecef[0] * cam.eye_ecef[0]
                    + cam.eye_ecef[1] * cam.eye_ecef[1]
                    + cam.eye_ecef[2] * cam.eye_ecef[2];
                let cloud_pl = if eye_r2 > 6_396_000.0 * 6_396_000.0 {
                    &self.cloud_pipeline
                } else if eye_r2 < 6_390_000.0 * 6_390_000.0 {
                    &self.cloud_inside_pipeline
                } else {
                    &self.cloud_twosided_pipeline
                };
                pass.set_pipeline(cloud_pl);
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

            if !geo_transparent_batches.is_empty() {
                pass.set_pipeline(&self.geo_object_transparent_pipeline);
                pass.set_bind_group(0, &self.geo_object_bind_group, &[]);
                for (mesh_id, range) in &geo_transparent_batches {
                    let Some(gpu_mesh) = self.geo_meshes.get(mesh_id) else {
                        continue;
                    };
                    pass.set_vertex_buffer(0, gpu_mesh.vbuf.slice(..));
                    pass.set_index_buffer(gpu_mesh.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..gpu_mesh.index_count, 0, range.clone());
                }
            }

            if !particle_draws.is_empty() {
                pass.set_pipeline(&self.particle_pipeline);
                for (i, (count, tex_id_opt)) in particle_draws.iter().enumerate() {
                    let offset = (i as u32) * self.particle_uniform_stride;
                    pass.set_bind_group(0, &self.particle_common_bind_group, &[offset]);
                    let tex_bg = tex_id_opt
                        .and_then(|tid| self.particle_textures.get(&tid))
                        .map(|c| &c.bind_group)
                        .unwrap_or(&self.particle_default_tex_bg);
                    pass.set_bind_group(1, tex_bg, &[]);
                    pass.draw(0..6, 0..*count);
                }
            }

            if let Some(ov_bg) = overlay_bind_group {
                if viewport.is_some() {
                    let tw = target_width.max(1);
                    let th = target_height.max(1);
                    pass.set_viewport(0.0, 0.0, tw as f32, th as f32, 0.0, 1.0);
                    pass.set_scissor_rect(0, 0, tw, th);
                }
                pass.set_pipeline(&self.overlay_main_pass_pipeline);
                pass.set_bind_group(0, ov_bg, &[]);
                pass.draw(0..3, 0..1);
            }
        }
        let tp_sub = std::time::Instant::now();
        self.queue.submit(Some(encoder.finish()));
        if perf {
            let tot = t_start.elapsed().as_secs_f64() * 1000.0;
            if tot > 3.0 {
                eprintln!(
                    "[perf] render_scene_to_view tot={:.2}ms (env={:.2} slab={:.2} atlas={:.2} inst={:.2} dyn={:.2} rec={:.2} sub={:.2})",
                    tot,
                    t_env,
                    t_slab,
                    t_atlas,
                    t_inst,
                    t_dyn,
                    tp_sub.duration_since(tp_pass).as_secs_f64() * 1000.0,
                    tp_sub.elapsed().as_secs_f64() * 1000.0
                );
            }
        }
    }

    fn record_overlay_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
        overlay_bind_group: &wgpu::BindGroup,
    ) {
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
        self.record_overlay_pass(&mut encoder, target_view, overlay_bind_group);
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
        ..Default::default()
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
