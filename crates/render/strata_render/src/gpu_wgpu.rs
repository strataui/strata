//! Hardware-accelerated `wgpu` 3D renderer with double-single RTE precision and
//! single-draw-call programmable vertex pulling for planetary/terrain tile pyramids.

use crate::scene3d::{
    Background3D, Camera3D, Geometry3D, Light3D, Material3D, Mesh3D, RetainedCameraUniforms,
    RetainedDynamicMesh, RetainedEnvironmentAssets, RetainedOceanSurface,
    RetainedParticleBillboard, RetainedParticleLayer, RetainedParticleSprite, RetainedStar,
    RetainedTerrainFrame, RetainedTileMesh, RetainedTileTexture, RteTileVertex, Scene3D, TileKeyId,
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
    light_view_proj_rte: [[f32; 4]; 4],
    eye_high: [f32; 4],
    eye_low: [f32; 4],
    sun_dir: [f32; 4],            // xyz = sun dir, w = sun_visible (0/1)
    sun_color: [f32; 4],          // rgb = color, w = sun_angular_radius_rad
    ambient_color: [f32; 4],      // rgb = ambient, w = stars_intensity (0 when off)
    sky_top: [f32; 4],
    sky_bottom: [f32; 4],
    moon_dir: [f32; 4],           // xyz = moon dir, w = moon_visible (0/1)
    env_params: [f32; 4],         // x = day_night_strength, y = city_lights_intensity, z = local_25d_blend, w = moon_angular_radius_rad
    cloud_params: [f32; 4],       // x = cloud_layer_opacity, y = cloud_lon_offset_rad, z = time_s, w = space_sky_enabled (0/1)
    viewport_wh: [f32; 4],        // x = w, y = h, z = skybox_intensity, w = local_cloud_opacity
    atmo_25d_params: [f32; 4],    // x = realistic_25d_sky, y = night_stars_25d, z = terrain_25d_lighting, w = terrain_25d_shadows
    weather_opt_params: [f32; 4], // x = weather_darkness, y = weather_cloud_cover, z = weather_haze, w = shadow_radius_m
    cloud_tint_params: [f32; 4],  // xyz = cloud_tint_rgb, w = cloud_25d_cover_boost
    view_flags: [f32; 4],         // x = map_2d_morph (0..1), yzw = reserved
}

fn dot3_f64(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross3_f64(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm3_f64(a: [f64; 3], fallback: [f64; 3]) -> [f64; 3] {
    let l = dot3_f64(a, a).sqrt();
    if l > 1e-9 {
        [a[0] / l, a[1] / l, a[2] / l]
    } else {
        fallback
    }
}

/// Computes a texel-snapped RTE orthographic directional shadow matrix (`light_view_proj_rte`)
/// framing visible mountains and 3D objects around the camera. Returns `(matrix, shadow_radius_m, active)`.
fn compute_shadow_light_vp_rte(
    cam: &RetainedCameraUniforms,
    shadow_map_size: u32,
) -> ([[f32; 4]; 4], f32, bool) {
    let ident = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    if cam.terrain_25d_shadows <= 0.001 || cam.local_25d_blend <= 0.02 {
        return (ident, 0.0, false);
    }

    let sun = norm3_f64(
        [cam.sun_dir[0] as f64, cam.sun_dir[1] as f64, cam.sun_dir[2] as f64],
        [0.0, 0.0, 1.0],
    );
    let eye = cam.eye_ecef;
    let eye_r = dot3_f64(eye, eye).sqrt();
    let (eye_up, eye_alt, is_planet) = if eye_r > 1_000_000.0 {
        (
            [eye[0] / eye_r, eye[1] / eye_r, eye[2] / eye_r],
            (eye_r - 6_378_137.0).max(30.0),
            true,
        )
    } else {
        ([0.0, 1.0, 0.0], eye[1].abs().max(eye_r * 0.5).max(15.0), false)
    };

    let sun_elev = dot3_f64(sun, eye_up);
    if sun_elev <= 0.025 {
        return (ident, 0.0, false);
    }

    // Clamp grazing light angle slightly when constructing the shadow ortho box so distant peaks still fit cleanly.
    let radius = if is_planet {
        (eye_alt * 3.8).clamp(380.0, 28_000.0)
    } else {
        (eye_alt * 3.0).clamp(40.0, 1_000.0)
    };
    let depth_half = (radius * 2.4).max(if is_planet { 14_000.0 } else { 200.0 });

    // Camera forward vector in world space is `-row2(view_rot)` (column-major: row 2 = [view_rot[0][2], view_rot[1][2], view_rot[2][2]]).
    let cam_fwd = norm3_f64(
        [
            -(cam.view_rot[0][2] as f64),
            -(cam.view_rot[1][2] as f64),
            -(cam.view_rot[2][2] as f64),
        ],
        [0.0, 0.0, -1.0],
    );
    let fwd_dot_up = dot3_f64(cam_fwd, eye_up);
    let fwd_horiz = norm3_f64(
        [
            cam_fwd[0] - eye_up[0] * fwd_dot_up,
            cam_fwd[1] - eye_up[1] * fwd_dot_up,
            cam_fwd[2] - eye_up[2] * fwd_dot_up,
        ],
        [1.0, 0.0, 0.0],
    );

    // Center shadow frustum slightly ahead of camera and down toward ground relief.
    let fwd_shift = radius * 0.36;
    let down_shift = if is_planet {
        eye_alt.min(radius * 0.65)
    } else {
        eye_alt * 0.5
    };
    let center_rel = [
        fwd_horiz[0] * fwd_shift - eye_up[0] * down_shift,
        fwd_horiz[1] * fwd_shift - eye_up[1] * down_shift,
        fwd_horiz[2] * fwd_shift - eye_up[2] * down_shift,
    ];

    // Orthonormal light basis: l_right, l_up perpendicular to sun direction.
    let ref_up = if dot3_f64(sun, eye_up).abs() > 0.96 {
        fwd_horiz
    } else {
        eye_up
    };
    let l_right = norm3_f64(cross3_f64(ref_up, sun), [1.0, 0.0, 0.0]);
    let l_up = norm3_f64(cross3_f64(sun, l_right), [0.0, 1.0, 0.0]);

    // Snap world shadow center in light space to texel increments to prevent shimmer while panning.
    let world_center = [
        eye[0] + center_rel[0],
        eye[1] + center_rel[1],
        eye[2] + center_rel[2],
    ];
    let texel_size = (2.0 * radius) / (shadow_map_size.clamp(512, 4096) as f64);
    let cx_l = (dot3_f64(world_center, l_right) / texel_size).floor() * texel_size;
    let cy_l = (dot3_f64(world_center, l_up) / texel_size).floor() * texel_size;
    let cz_l = dot3_f64(world_center, sun);

    let eye_x_l = dot3_f64(eye, l_right);
    let eye_y_l = dot3_f64(eye, l_up);
    let eye_z_l = dot3_f64(eye, sun);

    let dx = eye_x_l - cx_l;
    let dy = eye_y_l - cy_l;
    let dz = eye_z_l - cz_l;

    // Map world_rel -> light NDC:
    // ndc_x = (dot(world_rel, l_right) + dx) / radius
    // ndc_y = (dot(world_rel, l_up) + dy) / radius
    // ndc_z = 0.5 - (dot(world_rel, sun) + dz) / (2 * depth_half)  (closer to sun = smaller depth in [0, 1])
    let sx = 1.0 / radius;
    let sy = 1.0 / radius;
    let sz = -0.5 / depth_half;

    let mat = [
        [
            (l_right[0] * sx) as f32,
            (l_up[0] * sy) as f32,
            (sun[0] * sz) as f32,
            0.0,
        ],
        [
            (l_right[1] * sx) as f32,
            (l_up[1] * sy) as f32,
            (sun[1] * sz) as f32,
            0.0,
        ],
        [
            (l_right[2] * sx) as f32,
            (l_up[2] * sy) as f32,
            (sun[2] * sz) as f32,
            0.0,
        ],
        [
            (dx * sx) as f32,
            (dy * sy) as f32,
            (0.5 + dz * sz) as f32,
            1.0,
        ],
    ];
    (mat, radius as f32, true)
}

impl GpuCameraUniforms {
    fn from_retained(
        cam: &RetainedCameraUniforms,
        viewport_w: f32,
        viewport_h: f32,
        light_view_proj_rte: [[f32; 4]; 4],
        shadow_radius_m: f32,
        shadows_active: bool,
    ) -> Self {
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
        let space_sky = if cam.stars_visible
            || cam.sun_visible
            || cam.moon_visible
            || cam.realistic_25d_sky > 0.001
            || (cam.map_2d_morph > 0.001 && !matches!(cam.background, Background3D::Solid(_)))
        {
            1.0
        } else if matches!(cam.background, Background3D::Solid(_)) {
            2.0
        } else {
            0.0
        };
        let stars_eff = if cam.stars_visible {
            cam.stars_intensity.max(0.0)
        } else {
            0.0
        };
        let shadow_strength = if shadows_active {
            cam.terrain_25d_shadows.clamp(0.0, 1.0)
        } else {
            0.0
        };
        Self {
            view_proj_rte: cam.view_proj_rte,
            inv_view_proj_rte: inv_vp,
            ecef_from_icrf: cam.ecef_from_icrf,
            icrf_from_ecef,
            light_view_proj_rte,
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
            atmo_25d_params: [
                cam.realistic_25d_sky.clamp(0.0, 1.0),
                cam.night_stars_25d.clamp(0.0, 1.0),
                cam.terrain_25d_lighting.clamp(0.0, 1.0),
                shadow_strength,
            ],
            weather_opt_params: [
                cam.weather_darkness.clamp(0.0, 1.0),
                cam.weather_cloud_cover.clamp(0.0, 1.0),
                cam.weather_haze.clamp(0.0, 1.0),
                shadow_radius_m.max(0.0),
            ],
            cloud_tint_params: [
                cam.cloud_tint_rgb[0].clamp(0.0, 2.0),
                cam.cloud_tint_rgb[1].clamp(0.0, 2.0),
                cam.cloud_tint_rgb[2].clamp(0.0, 2.0),
                cam.cloud_25d_cover_boost.clamp(0.0, 1.0),
            ],
            view_flags: [cam.map_2d_morph.clamp(0.0, 1.0), 0.0, 0.0, 0.0],
        }
    }
}

const OCEAN_WORLD_NOISE_PERIOD_M: f64 = 32_768.0;
const OCEAN_OCTAVE_ANGS: [f64; 8] = [0.04, -0.26, 0.42, -0.58, 0.77, -0.86, 0.21, -0.41];
const OCEAN_OCTAVE_WLS: [f64; 8] = [151.0, 97.0, 61.0, 38.0, 23.5, 13.7, 7.9, 4.3];
const OCEAN_OCTAVE_PHASES: [f64; 8] = [0.37, 2.18, 4.91, 1.09, 3.64, 5.47, 2.71, 0.83];

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuOceanUniforms {
    origin_high: [f32; 4],     // xyz = split high f32 of ocean origin_ecef, w = enabled (0/1)
    origin_low: [f32; 4],      // xyz = split low f32 of ocean origin_ecef, w = alt_fade
    basis_east: [f32; 4],      // xyz = local ENU east, w = wave_strength
    basis_north: [f32; 4],     // xyz = local ENU north, w = wave_scale
    basis_up: [f32; 4],        // xyz = local ENU up, w = glint_intensity
    train_a: [f32; 4],         // x = dir_e, y = dir_n, z = wind_speed_mps, w = phase_time_s
    train_b: [f32; 4],         // x = dir_e, y = dir_n, z = wind_speed_mps, w = phase_time_s
    blend_params: [f32; 4],    // x = weight_a, y = weight_b, z = opacity, w = coastal_shoaling (0/1)
    coastal_params: [f32; 4],  // x = shoaling_range_m, y = sdf_half_span_m, z = sdf_center_e, w = sdf_center_n
    world_meta: [f32; 4],      // x = wx mod P, y = wy mod P, z = origin_lat_rad, w = east_scale_25d (<0 => disabled)
    train_a_phases0: [f32; 4], // f64-reduced base phases for train_a octaves 0..4 in [0, TAU)
    train_a_phases1: [f32; 4], // f64-reduced base phases for train_a octaves 4..8 in [0, TAU)
    train_b_phases0: [f32; 4], // f64-reduced base phases for train_b octaves 0..4 in [0, TAU)
    train_b_phases1: [f32; 4], // f64-reduced base phases for train_b octaves 4..8 in [0, TAU)
}

impl GpuOceanUniforms {
    fn compute_train_base_phases(
        dir_en: [f64; 2],
        wind_speed_mps: f64,
        phase_time_s: f64,
        wave_scale: f64,
        world_origin_merc_m: [f64; 2],
    ) -> ([f32; 4], [f32; 4]) {
        let len_wind = 0.48 + (1.72 - 0.48) * (wind_speed_mps / 18.0).clamp(0.0, 1.3);
        let ws = wave_scale.max(0.15);
        let perp0 = [-dir_en[1], dir_en[0]];
        let wx = world_origin_merc_m[0];
        let wy = world_origin_merc_m[1];
        let mut p = [0.0f32; 8];
        for i in 0..8 {
            let wl = ((OCEAN_OCTAVE_WLS[i] * len_wind) / ws).max(1.4);
            let k = std::f64::consts::TAU / wl;
            let omega = (9.81 * k).sqrt();
            let (sa, ca) = OCEAN_OCTAVE_ANGS[i].sin_cos();
            let dx = dir_en[0] * ca + perp0[0] * sa;
            let dy = dir_en[1] * ca + perp0[1] * sa;
            let base = (dx * wx + dy * wy) * k - omega * phase_time_s + OCEAN_OCTAVE_PHASES[i];
            p[i] = base.rem_euclid(std::f64::consts::TAU) as f32;
        }
        ([p[0], p[1], p[2], p[3]], [p[4], p[5], p[6], p[7]])
    }

    fn disabled() -> Self {
        Self {
            origin_high: [0.0; 4],
            origin_low: [0.0; 4],
            basis_east: [1.0, 0.0, 0.0, 0.0],
            basis_north: [0.0, 1.0, 0.0, 1.0],
            basis_up: [0.0, 0.0, 1.0, 1.0],
            train_a: [1.0, 0.0, 8.0, 0.0],
            train_b: [1.0, 0.0, 8.0, 0.0],
            blend_params: [1.0, 0.0, 0.0, 0.0],
            coastal_params: [360.0, 1_500.0, 0.0, 0.0],
            world_meta: [0.0, 0.0, 0.0, -1.0],
            train_a_phases0: [0.0; 4],
            train_a_phases1: [0.0; 4],
            train_b_phases0: [0.0; 4],
            train_b_phases1: [0.0; 4],
        }
    }

    fn from_retained(ocean: &RetainedOceanSurface, has_sdf_tex: bool) -> Self {
        if !ocean.enabled || ocean.alt_fade <= 0.002 || ocean.opacity <= 0.002 {
            return Self::disabled();
        }
        let (ox_hi, ox_lo) = split_f64_to_f32(ocean.origin_ecef[0]);
        let (oy_hi, oy_lo) = split_f64_to_f32(ocean.origin_ecef[1]);
        let (oz_hi, oz_lo) = split_f64_to_f32(ocean.origin_ecef[2]);
        let norm2_f64 = |v: [f32; 2]| -> [f64; 2] {
            let vx = v[0] as f64;
            let vy = v[1] as f64;
            let l = (vx * vx + vy * vy).sqrt();
            if l > 1e-6 {
                [vx / l, vy / l]
            } else {
                [1.0, 0.0]
            }
        };
        let da64 = norm2_f64(ocean.train_a.dir_en);
        let db64 = norm2_f64(ocean.train_b.dir_en);
        let da = [da64[0] as f32, da64[1] as f32];
        let db = [db64[0] as f32, db64[1] as f32];
        let wave_scale = ocean.wave_scale.clamp(0.15, 6.0);
        let speed_a = ocean.train_a.wind_speed_mps.clamp(0.2, 60.0);
        let speed_b = ocean.train_b.wind_speed_mps.clamp(0.2, 60.0);

        let has_world = ocean.world_origin_merc_m[0].abs() > 1e-3
            || ocean.world_origin_merc_m[1].abs() > 1e-3
            || ocean.origin_lat_rad.abs() > 1e-5
            || ocean.origin_ecef.iter().any(|v| v.abs() > 100.0);
        let wx_mod = ocean.world_origin_merc_m[0].rem_euclid(OCEAN_WORLD_NOISE_PERIOD_M) as f32;
        let wy_mod = ocean.world_origin_merc_m[1].rem_euclid(OCEAN_WORLD_NOISE_PERIOD_M) as f32;
        let east_scale_meta = if has_world {
            ocean.east_scale.clamp(0.15, 1.0)
        } else {
            -1.0
        };
        let (ta_p0, ta_p1) = Self::compute_train_base_phases(
            da64,
            speed_a as f64,
            ocean.train_a.phase_time_s as f64,
            wave_scale as f64,
            ocean.world_origin_merc_m,
        );
        let (tb_p0, tb_p1) = Self::compute_train_base_phases(
            db64,
            speed_b as f64,
            ocean.train_b.phase_time_s as f64,
            wave_scale as f64,
            ocean.world_origin_merc_m,
        );

        Self {
            origin_high: [ox_hi, oy_hi, oz_hi, 1.0],
            origin_low: [ox_lo, oy_lo, oz_lo, ocean.alt_fade.clamp(0.0, 1.0)],
            basis_east: [
                ocean.basis_east[0],
                ocean.basis_east[1],
                ocean.basis_east[2],
                ocean.wave_strength.clamp(0.0, 4.0),
            ],
            basis_north: [
                ocean.basis_north[0],
                ocean.basis_north[1],
                ocean.basis_north[2],
                wave_scale,
            ],
            basis_up: [
                ocean.basis_up[0],
                ocean.basis_up[1],
                ocean.basis_up[2],
                ocean.glint_intensity.clamp(0.0, 4.0),
            ],
            train_a: [da[0], da[1], speed_a, ocean.train_a.phase_time_s],
            train_b: [db[0], db[1], speed_b, ocean.train_b.phase_time_s],
            blend_params: [
                ocean.train_a.weight.clamp(0.0, 1.0),
                ocean.train_b.weight.clamp(0.0, 1.0),
                ocean.opacity.clamp(0.0, 1.0),
                if ocean.coastal_shoaling && has_sdf_tex {
                    1.0
                } else {
                    0.0
                },
            ],
            coastal_params: [
                ocean.shoaling_range_m.max(20.0),
                ocean.sdf_half_span_m.max(50.0),
                ocean.sdf_center_en_m[0],
                ocean.sdf_center_en_m[1],
            ],
            world_meta: [wx_mod, wy_mod, ocean.origin_lat_rad, east_scale_meta],
            train_a_phases0: ta_p0,
            train_a_phases1: ta_p1,
            train_b_phases0: tb_p0,
            train_b_phases1: tb_p1,
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
    forward_bias: [f32; 4],    // xyz = world-space forward cone center offset, w = 0.0
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
    light_view_proj_rte: mat4x4<f32>,
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
    atmo_25d_params: vec4<f32>,
    weather_opt_params: vec4<f32>,
    cloud_tint_params: vec4<f32>,
    view_flags: vec4<f32>,
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

fn eval_physical_sky_25d(
    rd: vec3<f32>,
    eye_up: vec3<f32>,
    horiz_sin: f32,
    sun_dir: vec3<f32>,
    weather_dark: f32,
    cloud_cov: f32,
    haze: f32,
) -> vec3<f32> {
    let mu = dot(rd, eye_up);
    let elev = mu - horiz_sin;
    let h_elev = clamp(max(elev, 0.0), 0.0, 1.0);
    let sun_elev = dot(sun_dir, eye_up);

    // Kasten-Young optical air mass approximation for solar transmittance & Rayleigh scattering.
    let sun_deg = max(sun_elev * 57.29578, -2.0);
    let m_sun = 1.0 / (max(sun_elev, 0.008) + 0.15 * pow(max(sun_deg + 3.885, 0.45), -1.253));
    let sun_trans = exp(-vec3<f32>(0.085, 0.195, 0.46) * clamp(m_sun * 0.48, 0.0, 18.0));

    let day_t = smoothstep(-0.08, 0.24, sun_elev);
    let twilight_t = smoothstep(-0.22, -0.02, sun_elev) * (1.0 - smoothstep(0.02, 0.22, sun_elev));
    let golden_t = exp(-pow((sun_elev - 0.03) * 7.8, 2.0));

    // Noon zenith/mid/horizon -> Golden sunset/sunrise -> Twilight -> Deep midnight indigo.
    let noon_zenith = vec3<f32>(0.08, 0.23, 0.56);
    let noon_mid = vec3<f32>(0.24, 0.50, 0.86);
    let noon_horiz = vec3<f32>(0.72, 0.85, 0.96);

    let sunset_zenith = vec3<f32>(0.09, 0.13, 0.30);
    let sunset_mid = vec3<f32>(0.36, 0.26, 0.36);
    let sunset_horiz = vec3<f32>(0.96, 0.48, 0.19);

    let night_zenith = vec3<f32>(0.006, 0.011, 0.026);
    let night_mid = vec3<f32>(0.012, 0.020, 0.044);
    let night_horiz = vec3<f32>(0.024, 0.038, 0.068);

    let day_zen = mix(sunset_zenith, noon_zenith, smoothstep(0.02, 0.35, sun_elev));
    let day_mid_c = mix(sunset_mid, noon_mid, smoothstep(0.02, 0.35, sun_elev));
    let day_hor = mix(sunset_horiz, noon_horiz, smoothstep(0.02, 0.32, sun_elev));

    let zenith_c = mix(night_zenith, day_zen, day_t) + vec3<f32>(0.03, 0.04, 0.09) * twilight_t;
    let mid_c = mix(night_mid, day_mid_c, day_t) + vec3<f32>(0.10, 0.07, 0.12) * twilight_t;
    let horiz_c = mix(night_horiz, day_hor, day_t) + vec3<f32>(0.34, 0.16, 0.10) * twilight_t;

    let zenith_t = pow(clamp(h_elev * 1.85, 0.0, 1.0), 0.58);
    let dome = mix(mid_c, zenith_c, zenith_t);
    let optical_haze = exp(-h_elev * mix(10.0, 5.2, clamp(haze * 0.7 + weather_dark * 0.4, 0.0, 1.0)));

    // Henyey-Greenstein forward Mie scattering around the sun direction.
    let cos_theta = clamp(dot(rd, sun_dir), -1.0, 1.0);
    let g_mie = 0.76;
    let hg = (1.0 - g_mie * g_mie) / pow(max(1.0 + g_mie * g_mie - 2.0 * g_mie * cos_theta, 0.04), 1.5);
    let sun_above = smoothstep(-0.08, 0.06, sun_elev);
    let cloud_atten = 1.0 - clamp(weather_dark * 0.78 + cloud_cov * 0.35, 0.0, 0.92);
    let mie_glow = (vec3<f32>(0.98, 0.84, 0.62) * sun_trans + vec3<f32>(0.55, 0.24, 0.06) * golden_t)
        * clamp(hg * 0.048, 0.0, 1.35)
        * sun_above
        * cloud_atten
        * (0.35 + 0.65 * exp(-h_elev * 2.2));

    // Anti-solar / azimuth sunset band along the horizon toward the sun.
    let sun_az_glow = pow(max(cos_theta, 0.0), 3.0) * (golden_t * 0.75 + twilight_t * 0.55)
        * exp(-h_elev * 4.5) * cloud_atten;
    let sunset_band = vec3<f32>(1.0, 0.46, 0.16) * sun_az_glow * 0.65;

    var sky = mix(dome, horiz_c, clamp(optical_haze * 0.92, 0.0, 1.0)) + mie_glow + sunset_band;

    // Weather / overcast cloud darkening & slate desaturation.
    let storm_w = clamp(weather_dark * 0.82 + cloud_cov * 0.22, 0.0, 0.92);
    if (storm_w > 0.002) {
        let luma = dot(sky, vec3<f32>(0.299, 0.587, 0.114));
        let overcast_day = mix(vec3<f32>(0.20, 0.24, 0.29), vec3<f32>(0.42, 0.47, 0.54), optical_haze);
        let overcast_night = vec3<f32>(0.015, 0.020, 0.030);
        let overcast_col = mix(overcast_night, overcast_day, clamp(day_t + twilight_t * 0.35, 0.0, 1.0));
        let desaturated = mix(sky, vec3<f32>(luma) * vec3<f32>(0.88, 0.94, 1.02), storm_w * 0.65);
        sky = mix(desaturated, overcast_col, storm_w * 0.68);
    }

    let below_t = smoothstep(0.0, 0.06, -elev);
    let ground_mist = mix(
        mix(night_horiz, horiz_c * 0.88, clamp(day_t + twilight_t * 0.5, 0.0, 1.0)),
        vec3<f32>(0.26, 0.30, 0.36) * max(day_t, 0.08),
        storm_w * 0.70,
    );
    return mix(sky, ground_mist, below_t);
}

@fragment
fn fs_sky(in: SkyOut) -> @location(0) vec4<f32> {
    let rd = normalize(in.ray_dir);
    let rd_icrf = normalize((cam.icrf_from_ecef * vec4<f32>(rd, 0.0)).xyz);
    // Derivatives must remain in uniform control flow for WGSL validation.
    let dx_icrf = dpdx(rd_icrf);
    let dy_icrf = dpdy(rd_icrf);
    if (cam.cloud_params.w > 1.5) {
        return cam.sky_top;
    }

    let space_t = 1.0 - smoothstep(0.0, 0.85, cam.env_params.z);
    let map_2d = clamp(cam.view_flags.x, 0.0, 1.0);
    let morph_clear = 1.0 - smoothstep(0.0, 0.85, map_2d);
    let sun_dir = normalize(cam.sun_dir.xyz);
    let real_sky_w = clamp(cam.atmo_25d_params.x, 0.0, 1.0);
    let night_star_w = clamp(cam.atmo_25d_params.y, 0.0, 1.0);
    let weather_dark = clamp(cam.weather_opt_params.x, 0.0, 1.0);
    let cloud_cov = clamp(cam.weather_opt_params.y, 0.0, 1.0);
    let weather_haze = clamp(cam.weather_opt_params.z, 0.0, 1.0);

    let eye_pos = cam.eye_high.xyz + cam.eye_low.xyz;
    let eye_r = length(eye_pos);
    let eye_up = select(vec3<f32>(0.0, 1.0, 0.0), eye_pos / max(eye_r, 1.0), eye_r > 1000000.0);
    let r_earth = 6378137.0;
    let horiz_sin = select(
        0.0,
        -sqrt(max(eye_r * eye_r - r_earth * r_earth, 0.0)) / max(eye_r, r_earth),
        eye_r > 1000000.0,
    );
    let elev = dot(rd, eye_up) - horiz_sin;
    let sun_elev = dot(sun_dir, eye_up);

    var atmo_sky = vec3<f32>(0.0);
    if (cam.cloud_params.w < 0.5 || space_t < 0.999) {
        let sky_grad_2d = mix(cam.sky_top, cam.sky_bottom, clamp(in.uv_y, 0.0, 1.0));
        var classic_sky = sky_grad_2d.rgb;
        if (eye_r > 1000000.0) {
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
            let below_t = smoothstep(0.0, 0.06, -elev);
            let ground_mist = vec3<f32>(0.64, 0.77, 0.90) * day_scale;
            classic_sky = mix(above_sky, ground_mist, below_t);
        }
        // Subtle weather darkening even in classic mode if base weather API explicitly supplies storm_darkness.
        if (weather_dark > 0.002 && real_sky_w < 0.999) {
            let classic_storm = vec3<f32>(0.24, 0.29, 0.36);
            classic_sky = mix(classic_sky, classic_storm, weather_dark * 0.62);
        }
        if (real_sky_w > 0.001) {
            let phys_sky = eval_physical_sky_25d(rd, eye_up, horiz_sin, sun_dir, weather_dark, cloud_cov, weather_haze);
            atmo_sky = mix(classic_sky, phys_sky, real_sky_w);
        } else {
            atmo_sky = classic_sky;
        }
        atmo_sky = mix(sky_grad_2d.rgb, atmo_sky, morph_clear);
        if (cam.cloud_params.w < 0.5 && map_2d <= 0.001) {
            return vec4<f32>(atmo_sky, sky_grad_2d.a);
        }
    }

    // Effective skybox visibility: full in orbital space, plus smooth night-sky emergence in 2.5D when synced!
    let sky_clear_factor = 1.0 - clamp(cloud_cov * 0.85 + weather_dark * 0.55, 0.0, 0.98);
    let night_sun_t = 1.0 - smoothstep(-0.14, 0.04, sun_elev);
    let night_vis_25d = night_sun_t
        * night_star_w
        * sky_clear_factor
        * smoothstep(-0.005, 0.08, elev);
    let skybox_vis = max(space_t, night_vis_25d * (1.0 - space_t)) * morph_clear;

    // 1. Pitch-black deep space + J2000 ICRF telescope skybox cubemap (or procedural Milky Way fallback)
    var space_col = vec3<f32>(0.0);
    if (cam.ambient_color.w > 0.001 && skybox_vis > 0.001) {
        let cube_dim = max(f32(textureDimensions(skybox_cube, 0).x), 1.0);
        let texel_sq = (cube_dim * cube_dim * 0.25) * max(dot(dx_icrf, dx_icrf), dot(dy_icrf, dy_icrf));
        let sky_lod = max(0.5 * log2(max(texel_sq, 1.0)), 0.0);
        let cube_s = textureSampleLevel(skybox_cube, env_sampler, rd_icrf, sky_lod);
        let sky_gain = cam.viewport_wh.z;
        if (cube_s.a > 0.01 && sky_gain > 0.001) {
            space_col += cube_s.rgb * sky_gain * skybox_vis;
        } else {
            let gnp = vec3<f32>(-0.867666, -0.198076, 0.455984);
            let sin_b = dot(rd_icrf, gnp);
            if (abs(sin_b) < 0.35) {
                let gc = vec3<f32>(-0.054876, -0.873437, -0.483835);
                let core_w = 0.35 + 0.65 * max(dot(rd_icrf, gc), 0.0);
                let dust_n = noise3(rd_icrf * 7.5) * 0.65 + noise3(rd_icrf * 17.0) * 0.35;
                let rift = smoothstep(0.015, 0.14, abs(sin_b) + (dust_n - 0.5) * 0.09);
                let mw = exp(-pow(abs(sin_b) * 8.0, 1.5)) * core_w * (0.45 + 0.55 * dust_n) * mix(0.45, 1.0, rift);
                space_col += vec3<f32>(0.075, 0.095, 0.165) * mw * min(cam.ambient_color.w, 1.8) * skybox_vis;
            }
        }
    }

    // Blend pitch-black space to 3D horizon-anchored atmospheric sky when morphing down into 2.5D,
    // keeping night Milky Way cubemap cleanly additive over the dark 2.5D night sky!
    var col = space_col;
    if (space_t < 0.999) {
        col = mix(atmo_sky + space_col * 1.15, space_col, space_t);
    }

    // 2. Realistic Sun (photosphere limb darkening + solar corona + atmospheric aureole in 2.5D)
    if (cam.sun_dir.w > 0.5) {
        let sun_cos = clamp(dot(rd, sun_dir), -1.0, 1.0);
        if (sun_cos > 0.80) {
            let sun_ang = acos(sun_cos);
            let r_sun = max(cam.sun_color.w, 0.004);
            let above_horiz = mix(smoothstep(-0.025, 0.015, elev), 1.0, space_t);
            let atmo_sun_atten = mix(1.0 - clamp(weather_dark * 0.82 + cloud_cov * 0.45, 0.0, 0.95), 1.0, space_t);
            let sunset_warm = mix(
                mix(vec3<f32>(1.0, 0.56, 0.24), vec3<f32>(1.0, 0.98, 0.92), smoothstep(-0.02, 0.28, sun_elev)),
                vec3<f32>(1.0, 0.98, 0.95),
                space_t,
            );
            let sun_vis = above_horiz * atmo_sun_atten;
            if (sun_vis > 0.002) {
                if (sun_ang < r_sun * 1.08) {
                    let mu_s = sqrt(max(1.0 - (sun_ang / r_sun) * (sun_ang / r_sun), 0.0));
                    let aa = (1.0 - smoothstep(r_sun * 0.94, r_sun * 1.06, sun_ang)) * sun_vis;
                    let limb = mix(0.48, 1.0, pow(mu_s, 0.45));
                    col = mix(col, cam.sun_color.rgb * sunset_warm * (3.6 * limb), aa);
                }
                let corona = exp(-sun_ang / (r_sun * 2.4)) * 0.85 + exp(-sun_ang / (r_sun * 12.0)) * 0.14;
                col += cam.sun_color.rgb * sunset_warm * corona * mix(0.62, 1.0, space_t) * sun_vis;
            }
        }
    }

    // 3. Realistic Moon (NASA LRO albedo + Lommel-Seeliger phase from Sun vector + Earthshine)
    if (cam.moon_dir.w > 0.5) {
        let m_dir = normalize(cam.moon_dir.xyz);
        let b_cos = dot(rd, m_dir);
        let r_moon = max(cam.env_params.w, 0.004);
        let above_horiz_m = mix(smoothstep(-0.02, 0.02, elev), 1.0, space_t);
        if (above_horiz_m > 0.005 && b_cos > 1.0 - 0.501 * r_moon * r_moon) {
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
                let aa_m = (1.0 - smoothstep(r2 * 0.90, r2, d2)) * above_horiz_m;
                let moon_night_t = 1.0 - smoothstep(-0.10, 0.10, sun_elev);
                let moon_25d_vis = mix(0.32, 0.98, moon_night_t) * sky_clear_factor;
                col = mix(col, max(col, lit_moon), aa_m * mix(moon_25d_vis, 1.0, space_t));
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
    light_view_proj_rte: mat4x4<f32>,
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
    atmo_25d_params: vec4<f32>,
    weather_opt_params: vec4<f32>,
    cloud_tint_params: vec4<f32>,
    view_flags: vec4<f32>,
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

    let eye_pos = cam.eye_high.xyz + cam.eye_low.xyz;
    let eye_r = length(eye_pos);
    let eye_up = select(vec3<f32>(0.0, 1.0, 0.0), eye_pos / max(eye_r, 1.0), eye_r > 1000000.0);
    let r_earth = 6378137.0;
    let horiz_sin = select(
        0.0,
        -sqrt(max(eye_r * eye_r - r_earth * r_earth, 0.0)) / max(eye_r, r_earth),
        eye_r > 1000000.0,
    );
    let sun_elev = dot(normalize(cam.sun_dir.xyz), eye_up);
    let star_elev = dot(dir_ecef, eye_up) - horiz_sin;
    let sky_clear = 1.0 - clamp(cam.weather_opt_params.y * 0.85 + cam.weather_opt_params.x * 0.55, 0.0, 0.98);
    let night_sun_t = 1.0 - smoothstep(-0.14, 0.04, sun_elev);
    let night_vis_25d = night_sun_t
        * clamp(cam.atmo_25d_params.y, 0.0, 1.0)
        * sky_clear
        * smoothstep(0.002, 0.045, star_elev);
    let eff_vis = max(space_t, night_vis_25d * (1.0 - space_t));

    if (clip.w <= 0.001 || eff_vis <= 0.005 || cam.ambient_color.w <= 0.001) {
        out.clip_pos = vec4<f32>(2.0, 2.0, 2.0, 1.0);
        out.color_flux = vec4<f32>(0.0);
        return out;
    }

    let vmag = s.icrf_dir_mag.w;
    let boost_25d = mix(2.35, 1.0, space_t);
    let flux = clamp(pow(10.0, -0.32 * (vmag - 2.6)), 0.18, 3.6) * cam.ambient_color.w * eff_vis * boost_25d;
    let size_px = clamp(mix(1.85, 1.55, space_t) + (5.0 - vmag) * 0.58, 1.5, 4.8);
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
    light_view_proj_rte: mat4x4<f32>,
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
    atmo_25d_params: vec4<f32>,
    weather_opt_params: vec4<f32>,
    cloud_tint_params: vec4<f32>,
    view_flags: vec4<f32>,
}

struct GpuOceanUniforms {
    origin_high: vec4<f32>,
    origin_low: vec4<f32>,
    basis_east: vec4<f32>,
    basis_north: vec4<f32>,
    basis_up: vec4<f32>,
    train_a: vec4<f32>,
    train_b: vec4<f32>,
    blend_params: vec4<f32>,
    coastal_params: vec4<f32>,
    world_meta: vec4<f32>,
    train_a_phases0: vec4<f32>,
    train_a_phases1: vec4<f32>,
    train_b_phases0: vec4<f32>,
    train_b_phases1: vec4<f32>,
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
@group(0) @binding(6) var env_wrap_sampler: sampler;

@group(1) @binding(0) var shadow_map: texture_depth_2d;
@group(1) @binding(1) var shadow_sampler: sampler_comparison;
@group(1) @binding(2) var<uniform> ocean: GpuOceanUniforms;
@group(1) @binding(3) var coastal_sdf_tex: texture_2d<f32>;
@group(1) @binding(4) var coastal_sdf_sampler: sampler;

fn dir_to_equirect_uv(d: vec3<f32>) -> vec2<f32> {
    let xy_len_sq = dot(d.xy, d.xy);
    let az = select(0.0, atan2(d.y, d.x), xy_len_sq > 1e-10);
    return vec2<f32>(
        fract(az * 0.15915494309 + 0.5),
        clamp(0.5 - asin(clamp(d.z, -1.0, 1.0)) * 0.31830988618, 0.001, 0.999),
    );
}

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
    @location(10) world_mod_256: vec3<f32>,
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

// Wraps global Mercator noise offset seamlessly into [0, 32768) m for bit-exact pan/zoom stability.
fn ocean_noise_wrap(p_m: vec2<f32>) -> vec2<f32> {
    return fract(p_m * (1.0 / 32768.0)) * 32768.0;
}

// Maps local ENU metres near `ocean.origin` to invariant global Mercator delta metres
// so crossing quantized origin cells or 3D<->2.5D morph bands produces zero wave phase jumps.
fn ocean_merc_from_en(pos_en: vec2<f32>, blend_25d: f32) -> vec2<f32> {
    if (ocean.world_meta.w < 0.0) {
        return pos_en;
    }
    let orig_lat_rad = ocean.world_meta.z;
    let east_scale_25d = ocean.world_meta.w;
    let dlat_est = clamp(pos_en.y * (1.0 / 6378137.0), -0.4, 0.4);
    let cos_curved = max(cos(clamp(orig_lat_rad + dlat_est, -1.483, 1.483)), 0.15);
    let cos_eff_e = max(mix(cos_curved, east_scale_25d, blend_25d), 0.15);
    let cos_eff_n = max(cos(clamp(orig_lat_rad + 0.5 * dlat_est, -1.483, 1.483)), 0.15);
    return vec2<f32>(pos_en.x / cos_eff_e, pos_en.y / cos_eff_n);
}

// C2 quintic periodic 2D Perlin gradient noise rotated by integer Pythagorean matrix `rot = (rx, ry)`
// where `32768.0 * inv_cell == period` (exact integer wrapping across the 32768 m world tile boundary).
// `seed` decorrelates independent octaves after modulo wrapping without breaking periodicity.
// Returns vec3(value in [0,1], grad_x, grad_y) with normalized dimensionless gradient.
fn wrap_period2(i_raw: vec2<f32>, period: f32) -> vec2<f32> {
    let w = i_raw - floor(i_raw * (1.0 / period)) * period;
    let w_pos = select(w, w + vec2<f32>(period), w < vec2<f32>(0.0));
    return select(w_pos, w_pos - vec2<f32>(period), w_pos >= vec2<f32>(period));
}

fn pgrad2_periodic_rot(
    p_mod: vec2<f32>,
    inv_cell: f32,
    period: f32,
    rot: vec2<f32>,
    seed: vec2<f32>,
) -> vec3<f32> {
    let q = p_mod * inv_cell;
    let x = vec2<f32>(rot.x * q.x - rot.y * q.y, rot.y * q.x + rot.x * q.y);
    let i0_raw = floor(x);
    let f = fract(x);
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    let one_minus_f = 1.0 - f;
    let du = 30.0 * f * f * one_minus_f * one_minus_f;

    let a = wrap_period2(i0_raw, period);
    let b = wrap_period2(a + vec2<f32>(1.0), period);

    let ang00 = 6.2831853 * hash21(vec2<f32>(a.x, a.y) + seed);
    let ang10 = 6.2831853 * hash21(vec2<f32>(b.x, a.y) + seed);
    let ang01 = 6.2831853 * hash21(vec2<f32>(a.x, b.y) + seed);
    let ang11 = 6.2831853 * hash21(vec2<f32>(b.x, b.y) + seed);

    let g00 = vec2<f32>(cos(ang00), sin(ang00));
    let g10 = vec2<f32>(cos(ang10), sin(ang10));
    let g01 = vec2<f32>(cos(ang01), sin(ang01));
    let g11 = vec2<f32>(cos(ang11), sin(ang11));

    let v00 = dot(g00, f);
    let v10 = dot(g10, f - vec2<f32>(1.0, 0.0));
    let v01 = dot(g01, f - vec2<f32>(0.0, 1.0));
    let v11 = dot(g11, f - vec2<f32>(1.0, 1.0));

    let k1 = v10 - v00;
    let k2 = v01 - v00;
    let k3 = v00 - v10 - v01 + v11;
    let h = v00 + k1 * u.x + k2 * u.y + k3 * u.x * u.y;
    let val01 = clamp(h * 0.70 + 0.5, 0.0, 1.0);

    let gx_u = mix(mix(g00.x, g10.x, u.x), mix(g01.x, g11.x, u.x), u.y);
    let gy_u = mix(mix(g00.y, g10.y, u.x), mix(g01.y, g11.y, u.x), u.y);
    let dh_dx = gx_u + du.x * (k1 + k3 * u.y);
    let dh_dy = gy_u + du.y * (k2 + k3 * u.x);

    let inv_rlen = 1.0 / max(length(rot), 1.0);
    let grad_norm = vec2<f32>(rot.x * dh_dx + rot.y * dh_dy, -rot.y * dh_dx + rot.x * dh_dy) * inv_rlen;
    return vec3<f32>(val01, grad_norm.x, grad_norm.y);
}

// Returns Gerstner trochoidal wave displacement vec3(disp_east_m, disp_north_m, disp_up_m)
// for the 4 primary macro swells using world-locked phases and short-crested wave-group envelopes.
fn eval_ocean_vertex_train(
    d_merc: vec2<f32>,
    p_noise_base: vec2<f32>,
    wind_dir: vec2<f32>,
    wind_speed: f32,
    t: f32,
    wave_scale: f32,
    sample_scale_m: f32,
    sea_exposure: f32,
    phases0: vec4<f32>,
) -> vec3<f32> {
    let dir0 = normalize(wind_dir);
    let perp0 = vec2<f32>(-dir0.y, dir0.x);

    let drift_m = ocean_noise_wrap(p_noise_base - dir0 * (t * 0.42));
    let n_warp1 = pgrad2_periodic_rot(drift_m, 1.0 / 512.0, 64.0, vec2<f32>(3.0, 4.0), vec2<f32>(11.3, 27.9)).x;
    let n_warp2 = pgrad2_periodic_rot(drift_m, 1.0 / 512.0, 64.0, vec2<f32>(8.0, 15.0), vec2<f32>(53.1, 19.7)).x;
    let warp_vec = vec2<f32>(n_warp1 - 0.5, n_warp2 - 0.5);
    let group_macro = 0.52 + 0.88 * (0.62 * n_warp1 + 0.38 * n_warp2);

    // Physical Beaufort saturation curve: calm light air -> smooth ~1.38 asymptote at strong wind.
    let beaufort = clamp(1.38 * (1.0 - exp(-max(wind_speed, 0.3) / 10.5)), 0.10, 1.38);
    let len_wind = mix(0.48, 1.72, clamp(wind_speed / 18.0, 0.0, 1.3));

    var angs = array<f32, 4>(0.04, -0.26, 0.42, -0.58);
    var wls = array<f32, 4>(151.0, 97.0, 61.0, 38.0);
    var amps = array<f32, 4>(0.025, 0.023, 0.020, 0.017);
    var base_ph = array<f32, 4>(phases0.x, phases0.y, phases0.z, phases0.w);

    var disp = vec3<f32>(0.0);
    for (var i: u32 = 0u; i < 4u; i = i + 1u) {
        let wl = max((wls[i] * len_wind) / max(wave_scale, 0.15), 4.0);
        let aa_vs = 1.0 - smoothstep(wl * 0.32, wl * 1.05, sample_scale_m);
        if (aa_vs > 0.008) {
            let ca = cos(angs[i]);
            let sa = sin(angs[i]);
            let d = dir0 * ca + perp0 * sa;
            let k = 6.2831853 / wl;
            let p_eff = d_merc + warp_vec * min(wl * 0.48, 52.0);
            let ph = base_ph[i] + dot(d, p_eff) * k;
            let env_shift = mix(group_macro, 0.52 + 0.88 * n_warp2, select(0.18, 0.55, (i & 1u) == 1u));
            let steep = amps[i] * beaufort * sea_exposure * aa_vs * env_shift;
            let amp_m = (steep / k) * 0.56;
            let q_m = amp_m * 0.48;
            disp.x += d.x * (cos(ph) * q_m);
            disp.y += d.y * (cos(ph) * q_m);
            disp.z += sin(ph) * amp_m;
        }
    }
    return disp;
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
    var world_rel = center_rel + v.pos_u.xyz;
    let raw_uv = vec2<f32>(v.pos_u.w, v.norm_v.w);
    var ecef_p = (inst.center_high.xyz + inst.center_low.xyz) + v.pos_u.xyz;
    let dv = normalize(ecef_p);
    let land_uv = dir_to_equirect_uv(dv);

    let map_2d = clamp(cam.view_flags.x, 0.0, 1.0);
    let ocean_on = ocean.origin_high.w * ocean.origin_low.w * (1.0 - map_2d);
    if (ocean_on > 0.002 && inst.tile_meta.z == 0u) {
        let cam_minus_orig = (cam.eye_high.xyz - ocean.origin_high.xyz)
                           + (cam.eye_low.xyz - ocean.origin_low.xyz);
        let p_rel_orig = world_rel + cam_minus_orig;
        let east_v = ocean.basis_east.xyz;
        let north_v = ocean.basis_north.xyz;
        let up_v = ocean.basis_up.xyz;
        let pos_en = vec2<f32>(dot(p_rel_orig, east_v), dot(p_rel_orig, north_v));
        let rel_h = dot(p_rel_orig, up_v);
        let eye_pos = cam.eye_high.xyz + cam.eye_low.xyz;
        let eye_r = length(eye_pos);
        let blend_25d = clamp(cam.env_params.z, 0.0, 1.0) * (1.0 - map_2d);
        let curve_sag = select(0.0, 1.0, eye_r > 1000000.0)
            * (1.0 - blend_25d)
            * dot(pos_en, pos_en)
            / (2.0 * 6378137.0);
        let sea_elev = rel_h + curve_sag;
        let elev_water = 1.0 - smoothstep(-0.25, 2.2, sea_elev);
        let env_mask_vs = textureSampleLevel(land_mask_tex, env_wrap_sampler, land_uv, 0.0).r;
        let open_ocean_vs = 1.0 - smoothstep(0.04, 0.52, env_mask_vs);

        let half_span = max(ocean.coastal_params.y, 50.0);
        let sdf_uv = (pos_en - ocean.coastal_params.zw) / (2.0 * half_span) + vec2<f32>(0.5);
        let in_sdf = smoothstep(0.01, 0.09, sdf_uv.x) * (1.0 - smoothstep(0.91, 0.99, sdf_uv.x))
                   * smoothstep(0.01, 0.09, sdf_uv.y) * (1.0 - smoothstep(0.91, 0.99, sdf_uv.y));
        let sdf_s = textureSampleLevel(
            coastal_sdf_tex,
            coastal_sdf_sampler,
            clamp(sdf_uv, vec2<f32>(0.002), vec2<f32>(0.998)),
            0.0,
        );
        let use_sdf = ocean.blend_params.w * in_sdf;
        let shore_dist_norm = mix(1.0, sdf_s.g, use_sdf);
        let shore_taper = mix(1.0, smoothstep(0.02, 0.16, shore_dist_norm), use_sdf);
        let near_exp_vs = mix(0.32, 0.78, smoothstep(0.06, 0.88, shore_dist_norm));
        let open_exp_vs = mix(0.38, 1.0, smoothstep(0.05, 0.92, open_ocean_vs));
        let sea_exposure_vs = mix(
            1.0,
            max(near_exp_vs, open_exp_vs),
            clamp(use_sdf + (1.0 - open_ocean_vs) * 0.65, 0.0, 1.0),
        );
        let water_vs = clamp(mix(open_ocean_vs, sdf_s.r, use_sdf) * elev_water * shore_taper, 0.0, 1.0);

        if (water_vs > 0.005) {
            let view_dist = length(world_rel);
            let sample_scale_m = max(view_dist * 0.032, 4.0);
            let wave_strength = ocean.basis_east.w;
            let wave_scale = ocean.basis_north.w;
            let d_merc = ocean_merc_from_en(pos_en, blend_25d);
            let p_noise_base = ocean_noise_wrap(ocean.world_meta.xy + d_merc);
            let wa = ocean.blend_params.x;
            let wb = ocean.blend_params.y;
            let w_sum = max(wa + wb, 1e-4);
            var disp_enu = vec3<f32>(0.0);
            if (wa > 0.002) {
                disp_enu += eval_ocean_vertex_train(
                    d_merc,
                    p_noise_base,
                    ocean.train_a.xy,
                    ocean.train_a.z,
                    ocean.train_a.w,
                    wave_scale,
                    sample_scale_m,
                    sea_exposure_vs,
                    ocean.train_a_phases0,
                ) * wa;
            }
            if (wb > 0.002) {
                disp_enu += eval_ocean_vertex_train(
                    d_merc,
                    p_noise_base,
                    ocean.train_b.xy,
                    ocean.train_b.z,
                    ocean.train_b.w,
                    wave_scale,
                    sample_scale_m,
                    sea_exposure_vs,
                    ocean.train_b_phases0,
                ) * wb;
            }
            disp_enu = (disp_enu / w_sum) * wave_strength * water_vs * ocean_on * ocean.blend_params.z;
            let disp_world = east_v * disp_enu.x + north_v * disp_enu.y + up_v * disp_enu.z;
            world_rel += disp_world;
            ecef_p += disp_world;
        }
    }

    let mod_base = floor(inst.center_high.xyz * (1.0 / 256.0)) * 256.0;
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
    out.land_uv_approx = land_uv;
    out.world_mod_256 = (inst.center_high.xyz - mod_base) + inst.center_low.xyz + v.pos_u.xyz;
    return out;
}

fn sample_shadow_pcf(world_rel: vec3<f32>, n_mesh: vec3<f32>, sun_dir: vec3<f32>, shadow_radius: f32) -> f32 {
    let ndotl = max(dot(n_mesh, sun_dir), 0.0);
    let normal_bias = max(shadow_radius * 0.0016, 0.45) * (1.25 - 0.75 * ndotl);
    let biased_rel = world_rel + n_mesh * normal_bias;
    let lclip = cam.light_view_proj_rte * vec4<f32>(biased_rel, 1.0);
    let ndc = lclip.xyz / max(lclip.w, 1e-5);
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    let depth_ref = clamp(ndc.z - 0.00055, 0.0, 0.9999);

    let dims = max(vec2<f32>(textureDimensions(shadow_map)), vec2<f32>(1.0));
    let texel = 1.0 / dims;
    var acc = 0.0;
    acc += textureSampleCompareLevel(shadow_map, shadow_sampler, clamp(uv + vec2<f32>(-texel.x, -texel.y), vec2<f32>(0.001), vec2<f32>(0.999)), depth_ref);
    acc += textureSampleCompareLevel(shadow_map, shadow_sampler, clamp(uv + vec2<f32>(0.0, -texel.y), vec2<f32>(0.001), vec2<f32>(0.999)), depth_ref);
    acc += textureSampleCompareLevel(shadow_map, shadow_sampler, clamp(uv + vec2<f32>(texel.x, -texel.y), vec2<f32>(0.001), vec2<f32>(0.999)), depth_ref);
    acc += textureSampleCompareLevel(shadow_map, shadow_sampler, clamp(uv + vec2<f32>(-texel.x, 0.0), vec2<f32>(0.001), vec2<f32>(0.999)), depth_ref);
    acc += textureSampleCompareLevel(shadow_map, shadow_sampler, clamp(uv, vec2<f32>(0.001), vec2<f32>(0.999)), depth_ref);
    acc += textureSampleCompareLevel(shadow_map, shadow_sampler, clamp(uv + vec2<f32>(texel.x, 0.0), vec2<f32>(0.001), vec2<f32>(0.999)), depth_ref);
    acc += textureSampleCompareLevel(shadow_map, shadow_sampler, clamp(uv + vec2<f32>(-texel.x, texel.y), vec2<f32>(0.001), vec2<f32>(0.999)), depth_ref);
    acc += textureSampleCompareLevel(shadow_map, shadow_sampler, clamp(uv + vec2<f32>(0.0, texel.y), vec2<f32>(0.001), vec2<f32>(0.999)), depth_ref);
    acc += textureSampleCompareLevel(shadow_map, shadow_sampler, clamp(uv + vec2<f32>(texel.x, texel.y), vec2<f32>(0.001), vec2<f32>(0.999)), depth_ref);
    let pcf = acc * (1.0 / 9.0);

    // Smooth edge fade near the border of the orthographic shadow frustum so shadows never clip abruptly.
    let border = max(abs(ndc.x), abs(ndc.y));
    let in_bounds = (1.0 - smoothstep(0.84, 0.99, border)) * step(0.0, ndc.z) * step(ndc.z, 1.0);
    return mix(1.0, pcf, in_bounds);
}

fn eval_sky_reflection(
    rd: vec3<f32>,
    eye_up: vec3<f32>,
    sun_dir: vec3<f32>,
    real_sky_w: f32,
    weather_dark: f32,
    cloud_cov: f32,
) -> vec3<f32> {
    let h = clamp(max(dot(rd, eye_up), 0.0), 0.0, 1.0);
    let sun_elev = dot(sun_dir, eye_up);
    let day_t = smoothstep(-0.08, 0.24, sun_elev);
    let twilight_t = smoothstep(-0.22, -0.02, sun_elev) * (1.0 - smoothstep(0.02, 0.22, sun_elev));
    let golden_t = exp(-pow((sun_elev - 0.03) * 7.5, 2.0));

    let day_zen = mix(vec3<f32>(0.09, 0.14, 0.32), vec3<f32>(0.08, 0.24, 0.56), smoothstep(0.02, 0.35, sun_elev));
    let day_hor = mix(vec3<f32>(0.95, 0.48, 0.20), vec3<f32>(0.72, 0.85, 0.96), smoothstep(0.02, 0.32, sun_elev));
    let night_zen = vec3<f32>(0.009, 0.016, 0.036);
    let night_hor = vec3<f32>(0.026, 0.042, 0.075);

    let zen = mix(night_zen, day_zen, day_t) + vec3<f32>(0.04, 0.04, 0.10) * twilight_t;
    let hor = mix(night_hor, day_hor, day_t) + vec3<f32>(0.32, 0.15, 0.09) * twilight_t;
    var phys = mix(hor, zen, pow(h, 0.55));
    let sun_cos = max(dot(rd, sun_dir), 0.0);
    let cloud_atten = 1.0 - clamp(weather_dark * 0.78 + cloud_cov * 0.35, 0.0, 0.92);
    phys += vec3<f32>(1.0, 0.58, 0.26) * pow(sun_cos, 6.0) * (golden_t * 0.55 + day_t * 0.18) * cloud_atten;

    let storm_w = clamp(weather_dark * 0.82 + cloud_cov * 0.22, 0.0, 0.92);
    let overcast = mix(vec3<f32>(0.02, 0.025, 0.038), vec3<f32>(0.32, 0.37, 0.44), clamp(day_t + twilight_t * 0.35, 0.0, 1.0));
    phys = mix(phys, overcast, storm_w * 0.72);

    let classic = mix(vec3<f32>(0.68, 0.82, 0.95), vec3<f32>(0.14, 0.34, 0.66), pow(h, 0.55));
    return mix(classic, phys, real_sky_w);
}

// Evaluates 8 directional anti-aliased short-crested Gerstner swells plus 6 octaves of
// Pythagorean-rotated C2 periodic Perlin gradient-noise wavelets & capillary slopes.
// Uses world-fixed Mercator deltas (`d_merc`) and pre-reduced CPU f64 base phases (`phases0/1`) so
// ocean waves never jump or alter pattern across camera pan, zoom, or 3D<->2.5D morph transitions.
// Returns vec4(slope_east, slope_north, crest_steepness, subpixel_aa_roughness).
fn eval_ocean_train(
    d_merc: vec2<f32>,
    p_noise_base: vec2<f32>,
    wind_dir: vec2<f32>,
    wind_speed: f32,
    t: f32,
    wave_scale: f32,
    px_m: f32,
    sea_exposure: f32,
    phases0: vec4<f32>,
    phases1: vec4<f32>,
) -> vec4<f32> {
    let dir0 = normalize(wind_dir);
    let perp0 = vec2<f32>(-dir0.y, dir0.x);

    // Multi-scale advected Perlin gradient noise drives smooth wave packets ("sets"), domain warp, and gusts.
    let drift_macro = ocean_noise_wrap(p_noise_base - dir0 * (t * 0.42));
    let drift_cross = ocean_noise_wrap(p_noise_base - dir0 * (t * 0.65) + perp0 * (t * 0.22));
    let gw1 = pgrad2_periodic_rot(drift_macro, 1.0 / 512.0, 64.0, vec2<f32>(3.0, 4.0), vec2<f32>(11.3, 27.9));
    let gw2 = pgrad2_periodic_rot(drift_cross, 1.0 / 512.0, 64.0, vec2<f32>(8.0, 15.0), vec2<f32>(53.1, 19.7));
    let n_warp1 = gw1.x;
    let n_warp2 = gw2.x;
    let warp_vec = vec2<f32>(n_warp1 - 0.5, n_warp2 - 0.5);
    let group_macro = 0.48 + 0.96 * (0.60 * n_warp1 + 0.40 * n_warp2);

    let drift_mid = ocean_noise_wrap(p_noise_base - dir0 * (t * 0.85) - perp0 * (t * 0.18));
    let gc = pgrad2_periodic_rot(drift_mid, 1.0 / 256.0, 128.0, vec2<f32>(5.0, 12.0), vec2<f32>(103.5, 41.2));
    let drift_b = ocean_noise_wrap(p_noise_base - dir0 * (t * 1.15) + perp0 * (t * 0.31) + vec2<f32>(317.0, 911.0));
    let gd = pgrad2_periodic_rot(drift_b, 1.0 / 128.0, 256.0, vec2<f32>(15.0, 8.0), vec2<f32>(19.7, 131.3));
    let n_gust = 0.58 * gc.x + 0.42 * n_warp2;
    let group_chop = 0.42 + 1.12 * (0.55 * gc.x + 0.45 * gd.x);

    // Physical Beaufort asymptote: calm light ripples at 2 m/s, rolling swells at 7-12 m/s,
    // smoothly saturating toward 1.38 in gale winds instead of scaling linearly to extremes.
    let beaufort = clamp(1.38 * (1.0 - exp(-max(wind_speed, 0.3) / 10.5)), 0.10, 1.38);
    let len_wind = mix(0.48, 1.72, clamp(wind_speed / 18.0, 0.0, 1.3));

    // 8 incommensurable directional swells localized into short-crested wave packets so no grid stripes form.
    var angs = array<f32, 8>(0.04, -0.26, 0.42, -0.58, 0.77, -0.86, 0.21, -0.41);
    var wls = array<f32, 8>(151.0, 97.0, 61.0, 38.0, 23.5, 13.7, 7.9, 4.3);
    var amps = array<f32, 8>(0.025, 0.023, 0.020, 0.017, 0.014, 0.011, 0.0085, 0.006);
    var base_ph = array<f32, 8>(
        phases0.x, phases0.y, phases0.z, phases0.w,
        phases1.x, phases1.y, phases1.z, phases1.w,
    );

    var slope = vec2<f32>(0.0);
    var crest_acc = 0.0;
    let warp_mid = vec2<f32>(gc.x - 0.5, gd.x - 0.5);

    for (var i: u32 = 0u; i < 8u; i = i + 1u) {
        let wl = max((wls[i] * len_wind) / max(wave_scale, 0.15), 1.4);
        let aa_w = 1.0 - smoothstep(wl * 0.22, wl * 0.58, px_m);
        if (aa_w > 0.008) {
            let ca = cos(angs[i]);
            let sa = sin(angs[i]);
            let d = dir0 * ca + perp0 * sa;
            let k = 6.2831853 / wl;
            let w_m = select(min(wl * 0.28, 7.5), min(wl * 0.48, 52.0), i < 4u);
            let p_eff = d_merc + warp_vec * w_m + warp_mid * min(wl * 0.22, 11.0);
            let ph = base_ph[i] + dot(d, p_eff) * k;
            let s = sin(ph);
            let c = cos(ph);
            let sharp = 0.5 + 0.5 * s;
            let env_alt = 0.45 + 1.05 * select(gd.x, n_warp2, (i & 1u) == 1u);
            let env_base = select(group_chop, group_macro, i < 4u);
            let env = mix(env_base, env_alt, 0.35);
            let steep = amps[i] * beaufort * sea_exposure * aa_w * env;
            let d_wave = c * (0.72 + 0.56 * sharp);
            slope += d * (d_wave * steep);
            crest_acc += pow(sharp, 2.4) * steep * 4.2;
        }
    }

    // 6 isotropic periodic Perlin gradient wavelets (~102m down to ~0.94m) provide rich organic
    // ocean texture with zero square-cell artifacts and preserve distant roughness via AA deficit.
    var noise_slope = vec2<f32>(0.0);
    var aa_rough = 0.0;
    let noise_strength = beaufort * sea_exposure * (0.62 + 0.72 * n_gust);

    let cell_a = (512.0 / 5.0) / max(wave_scale, 0.25);
    let aa_a = 1.0 - smoothstep(cell_a * 0.25, cell_a * 0.85, px_m);
    if (aa_a > 0.008) {
        noise_slope += gw1.yz * (0.018 * noise_strength * aa_a);
    }

    let cell_b = (512.0 / 17.0) / max(wave_scale, 0.25);
    let aa_b = 1.0 - smoothstep(cell_b * 0.25, cell_b * 0.85, px_m);
    if (aa_b > 0.008) {
        noise_slope += gw2.yz * (0.021 * noise_strength * aa_b);
    }

    let cell_c = (256.0 / 13.0) / max(wave_scale, 0.25);
    let aa_c = 1.0 - smoothstep(cell_c * 0.25, cell_c * 0.85, px_m);
    if (aa_c > 0.008) {
        noise_slope += gc.yz * (0.022 * noise_strength * aa_c);
    }

    let cell_d = (128.0 / 17.0) / max(wave_scale, 0.25);
    let aa_d = 1.0 - smoothstep(cell_d * 0.25, cell_d * 0.85, px_m);
    if (aa_d > 0.008) {
        noise_slope += gd.yz * (0.020 * noise_strength * aa_d);
    }
    aa_rough += (1.0 - aa_d) * 0.028;

    let cell_e = (32.0 / 13.0) / max(wave_scale, 0.25);
    let aa_e = 1.0 - smoothstep(cell_e * 0.25, cell_e * 0.85, px_m);
    if (aa_e > 0.008) {
        let ge = pgrad2_periodic_rot(drift_mid, 1.0 / 32.0, 1024.0, vec2<f32>(12.0, 5.0), vec2<f32>(61.9, 97.1)).yz;
        noise_slope += ge * (0.017 * noise_strength * aa_e);
    }
    aa_rough += (1.0 - aa_e) * 0.042;

    let cell_f = (16.0 / 17.0) / max(wave_scale, 0.25);
    let aa_f = 1.0 - smoothstep(cell_f * 0.25, cell_f * 0.85, px_m);
    if (aa_f > 0.008) {
        let gf = pgrad2_periodic_rot(drift_b, 1.0 / 16.0, 2048.0, vec2<f32>(8.0, 15.0), vec2<f32>(149.1, 33.7)).yz;
        noise_slope += gf * (0.013 * noise_strength * aa_f);
    }
    aa_rough += (1.0 - aa_f) * 0.036;

    slope += noise_slope;
    crest_acc += clamp(length(noise_slope) * 1.45, 0.0, 0.36);
    return vec4<f32>(slope.x, slope.y, crest_acc, aa_rough * clamp(sea_exposure * beaufort, 0.15, 1.2));
}

fn hash31(p: vec3<f32>) -> f32 {
    var q = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    q += dot(q, q.yxz + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn vnoise3_periodic(p_mod_256: vec3<f32>, period: f32) -> f32 {
    let x = p_mod_256 * (period * (1.0 / 256.0));
    let i0 = floor(x);
    let f = fract(x);
    let u = f * f * (3.0 - 2.0 * f);
    let a = i0 - floor(i0 / period) * period;
    let i1_raw = i0 + vec3<f32>(1.0);
    let b = i1_raw - floor(i1_raw / period) * period;

    let c000 = hash31(vec3<f32>(a.x, a.y, a.z));
    let c100 = hash31(vec3<f32>(b.x, a.y, a.z));
    let c010 = hash31(vec3<f32>(a.x, b.y, a.z));
    let c110 = hash31(vec3<f32>(b.x, b.y, a.z));
    let c001 = hash31(vec3<f32>(a.x, a.y, b.z));
    let c101 = hash31(vec3<f32>(b.x, a.y, b.z));
    let c011 = hash31(vec3<f32>(a.x, b.y, b.z));
    let c111 = hash31(vec3<f32>(b.x, b.y, b.z));

    let x00 = mix(c000, c100, u.x);
    let x10 = mix(c010, c110, u.x);
    let x01 = mix(c001, c101, u.x);
    let x11 = mix(c011, c111, u.x);
    return mix(mix(x00, x10, u.y), mix(x01, x11, u.y), u.z);
}

fn eval_micro_height(p_mod_256: vec3<f32>, raw_px_m: f32) -> f32 {
    let w8 = 1.0 - smoothstep(1.2, 4.0, raw_px_m);
    let w2 = 1.0 - smoothstep(0.32, 1.0, raw_px_m);
    let w05 = 1.0 - smoothstep(0.08, 0.30, raw_px_m);
    let w01 = 1.0 - smoothstep(0.02, 0.08, raw_px_m);
    var h = (vnoise3_periodic(p_mod_256, 32.0) - 0.5) * 0.44 * w8;
    h += (vnoise3_periodic(p_mod_256, 128.0) - 0.5) * 0.30 * w2;
    if (w05 > 0.01) {
        h += (vnoise3_periodic(p_mod_256, 512.0) - 0.5) * 0.18 * w05;
    }
    if (w01 > 0.01) {
        h += (vnoise3_periodic(p_mod_256, 2048.0) - 0.5) * 0.10 * w01;
    }
    return h;
}

fn shade_terrain(in: TerrainVsOut, base: vec4<f32>) -> vec4<f32> {
    let map_2d = clamp(cam.view_flags.x, 0.0, 1.0);
    if (in.unlit != 0u || map_2d >= 0.99) {
        return base;
    }

    let n_mesh = normalize(in.normal);
    let sun_dir = normalize(cam.sun_dir.xyz);
    let view_dir = normalize(-in.view_rel);

    let dn_strength = clamp(cam.env_params.x, 0.0, 1.0);
    let city_strength = max(cam.env_params.y, 0.0);
    let blend_25d = clamp(cam.env_params.z, 0.0, 1.0) * (1.0 - map_2d);
    let real_sky_w = clamp(cam.atmo_25d_params.x, 0.0, 1.0);
    let night_star_w = clamp(cam.atmo_25d_params.y, 0.0, 1.0);
    let lit_25d_w = clamp(cam.atmo_25d_params.z, 0.0, 1.0);
    let shadow_w = clamp(cam.atmo_25d_params.w, 0.0, 1.0);
    let weather_dark = clamp(cam.weather_opt_params.x, 0.0, 1.0);
    let cloud_cov = clamp(cam.weather_opt_params.y, 0.0, 1.0);
    let weather_haze = clamp(cam.weather_opt_params.z, 0.0, 1.0);

    let d = normalize(in.ecef_pos);
    let eye_pos = cam.eye_high.xyz + cam.eye_low.xyz;
    let eye_r = length(eye_pos);
    let local_up = select(vec3<f32>(0.0, 1.0, 0.0), d, eye_r > 1000000.0);
    let ndotl = dot(local_up, sun_dir);

    // Keep derivatives & texture lookups in uniform control flow for WGSL validation.
    let raw_px_m = length(fwidth(in.view_rel));
    let px_m = max(raw_px_m, 0.15);
    let env_uv = dir_to_equirect_uv(d);
    let env_lod = clamp(log2(max(px_m * 0.0001022, 1.0)), 0.0, 5.0);
    let env_mask = textureSampleLevel(land_mask_tex, env_wrap_sampler, env_uv, env_lod);
    let on_land = smoothstep(0.18, 0.55, env_mask.r);

    // Close-ground (< ~50-80m) procedural micro-albedo grain & micro-normal bump on land surfaces.
    let is_water_est = clamp(
        (1.0 - smoothstep(0.08, 0.42, env_mask.r))
            + select(0.0, 0.85, (base.b - base.r > 0.015) && env_mask.r < 0.65),
        0.0,
        1.0,
    );
    let micro_w = (1.0 - smoothstep(0.05, 0.80, raw_px_m)) * (1.0 - is_water_est) * (1.0 - map_2d);
    var n_lit = n_mesh;
    var base_micro_rgb = base.rgb;
    if (micro_w > 0.005) {
        let up_ref = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(n_mesh.y) > 0.9);
        let t_e = normalize(cross(up_ref, n_mesh));
        let t_n = cross(n_mesh, t_e);
        let h0 = eval_micro_height(in.world_mod_256, raw_px_m);
        let eps = max(raw_px_m * 0.75, 0.06);
        let he = eval_micro_height(in.world_mod_256 + t_e * eps, raw_px_m);
        let hn = eval_micro_height(in.world_mod_256 + t_n * eps, raw_px_m);
        let slope_e = ((he - h0) / eps) * 0.16 * micro_w;
        let slope_n = ((hn - h0) / eps) * 0.16 * micro_w;
        n_lit = normalize(n_mesh - t_e * slope_e - t_n * slope_n);
        let albedo_mod = 1.0 + h0 * (0.24 * micro_w);
        base_micro_rgb = clamp(base.rgb * albedo_mod, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    let slope_ndotl = max(dot(n_lit, sun_dir), 0.0);

    // Hardware 9-tap PCF directional shadow sampling (uniform control flow).
    var shadow_factor = 1.0;
    if (shadow_w > 0.001) {
        let pcf = sample_shadow_pcf(in.view_rel, n_mesh, sun_dir, max(cam.weather_opt_params.w, 10.0));
        shadow_factor = mix(1.0, pcf, shadow_w * (1.0 - weather_dark * 0.58));
    }

    // --- Orbital 3D lighting ---
    let slope_shade = clamp(1.0 + (dot(n_lit, sun_dir) - ndotl) * 0.35 * blend_25d, 0.82, 1.18);
    let terminator = smoothstep(-0.14, 0.16, ndotl);
    let tn = ndotl * 7.5;
    let twilight = exp(-tn * tn) * vec3<f32>(0.42, 0.18, 0.06);
    let f1 = 1.0 - max(dot(local_up, view_dir), 0.0);
    let fresnel = f1 * f1 * f1;
    let atmo_rim = vec3<f32>(0.28, 0.56, 1.0)
        * fresnel
        * smoothstep(-0.18, 0.32, ndotl)
        * 0.45
        * (1.0 - blend_25d)
        * (1.0 - smoothstep(0.0, 0.55, map_2d));
    let ocean_night = vec3<f32>(0.048, 0.068, 0.108);
    let land_night = vec3<f32>(0.095, 0.120, 0.174);
    let night_fill = mix(ocean_night, land_night, on_land);
    let day_lit_3d = (night_fill + cam.sun_color.rgb * terminator + twilight) * slope_shade;
    let flat_lit = vec3<f32>(0.98);
    var lit = mix(flat_lit, day_lit_3d, dn_strength);

    // --- Opt-in Realistic 2.5D Mountain Lighting & Cast Shadows ---
    let local_lit_blend = lit_25d_w * blend_25d;
    if (local_lit_blend > 0.001) {
        let sun_elev = dot(local_up, sun_dir);
        let day_t = smoothstep(-0.08, 0.24, sun_elev);
        let golden_t = exp(-pow((sun_elev - 0.04) * 7.2, 2.0));
        let sun_tint = mix(vec3<f32>(1.0, 0.56, 0.26), vec3<f32>(1.02, 0.99, 0.94), smoothstep(0.01, 0.30, sun_elev));
        let weather_sun_atten = 1.0 - clamp(weather_dark * 0.70 + cloud_cov * 0.22, 0.0, 0.85);
        // Normalize diffuse relative to typical overhead imagery luminance so sunlit slopes look crisp,
        // shaded valley/mountain backs drop dramatically into cool blue ambient shadow!
        let direct_intensity = smoothstep(-0.04, 0.12, sun_elev) * weather_sun_atten;
        let slope_term = clamp(slope_ndotl / max(sun_elev, 0.28), 0.0, 1.38);
        let direct_sun = sun_tint * (0.76 * slope_term * direct_intensity * shadow_factor);

        let sky_hemi = 0.5 + 0.5 * dot(n_lit, local_up);
        let day_amb = mix(vec3<f32>(0.22, 0.28, 0.40), vec3<f32>(0.34, 0.42, 0.54), sky_hemi)
            * mix(1.0, 0.72, weather_dark);
        let sunset_amb = vec3<f32>(0.30, 0.22, 0.26) * golden_t * 0.55;
        let night_amb = mix(vec3<f32>(0.055, 0.075, 0.125), vec3<f32>(0.095, 0.120, 0.180), on_land)
            * (0.75 + 0.25 * sky_hemi);
        let ambient_25d = mix(night_amb, day_amb, day_t) + sunset_amb;
        let lit_25d = ambient_25d + direct_sun;
        lit = mix(lit, lit_25d, local_lit_blend);
    }

    var rgb = base_micro_rgb * lit + atmo_rim * dn_strength;

    // --- GPU-Native Directional Ocean Waves + Optional Coastal Periodic Surf ---
    let ocean_on = ocean.origin_high.w * ocean.origin_low.w * (1.0 - map_2d);
    if (ocean_on > 0.002) {
        // Compute high-precision RTE local tangent coordinates relative to ocean origin.
        let cam_minus_orig = (cam.eye_high.xyz - ocean.origin_high.xyz)
                           + (cam.eye_low.xyz - ocean.origin_low.xyz);
        let p_rel_orig = in.view_rel + cam_minus_orig;
        let east_v = ocean.basis_east.xyz;
        let north_v = ocean.basis_north.xyz;
        let up_v = ocean.basis_up.xyz;
        let pos_en = vec2<f32>(dot(p_rel_orig, east_v), dot(p_rel_orig, north_v));
        let rel_h = dot(p_rel_orig, up_v);

        // 5-tap C1-smooth coastal SDF sample eliminates 128x128 u8 bilinear facets along coastlines.
        let half_span = max(ocean.coastal_params.y, 50.0);
        let sdf_uv = (pos_en - ocean.coastal_params.zw) / (2.0 * half_span) + vec2<f32>(0.5);
        let in_sdf = smoothstep(0.01, 0.09, sdf_uv.x) * (1.0 - smoothstep(0.91, 0.99, sdf_uv.x))
                   * smoothstep(0.01, 0.09, sdf_uv.y) * (1.0 - smoothstep(0.91, 0.99, sdf_uv.y));
        let sdf_texel = 1.0 / max(vec2<f32>(textureDimensions(coastal_sdf_tex)), vec2<f32>(8.0));
        let sdf_d = sdf_texel * 0.70;
        let uv_c = clamp(sdf_uv, vec2<f32>(0.002), vec2<f32>(0.998));
        let sdf_c0 = textureSampleLevel(coastal_sdf_tex, coastal_sdf_sampler, uv_c, 0.0);
        let sdf_c1 = textureSampleLevel(coastal_sdf_tex, coastal_sdf_sampler, clamp(uv_c + vec2<f32>(sdf_d.x, sdf_d.y), vec2<f32>(0.002), vec2<f32>(0.998)), 0.0);
        let sdf_c2 = textureSampleLevel(coastal_sdf_tex, coastal_sdf_sampler, clamp(uv_c + vec2<f32>(-sdf_d.x, sdf_d.y), vec2<f32>(0.002), vec2<f32>(0.998)), 0.0);
        let sdf_c3 = textureSampleLevel(coastal_sdf_tex, coastal_sdf_sampler, clamp(uv_c + vec2<f32>(sdf_d.x, -sdf_d.y), vec2<f32>(0.002), vec2<f32>(0.998)), 0.0);
        let sdf_c4 = textureSampleLevel(coastal_sdf_tex, coastal_sdf_sampler, clamp(uv_c + vec2<f32>(-sdf_d.x, -sdf_d.y), vec2<f32>(0.002), vec2<f32>(0.998)), 0.0);
        let sdf_s = sdf_c0 * 0.50 + (sdf_c1 + sdf_c2 + sdf_c3 + sdf_c4) * 0.125;

        let use_sdf = ocean.blend_params.w * in_sdf;
        let curve_sag = select(0.0, 1.0, eye_r > 1000000.0)
            * (1.0 - blend_25d)
            * dot(pos_en, pos_en)
            / (2.0 * 6378137.0);
        let sea_elev = rel_h + curve_sag;
        let elev_water = 1.0 - smoothstep(0.6, 5.5, sea_elev);

        // Pixel-sharp water classification from satellite imagery & elevation; SDF fills bright foam holes only.
        let max_c = max(base.r, max(base.g, base.b));
        let min_c = min(base.r, min(base.g, base.b));
        let cool_px = select(0.0, 1.0, (base.b >= base.r - 0.010) && (base.b + 0.030 >= base.g) && ((base.b - base.r >= 0.012) || max_c < 0.42));
        let emerald_px = select(0.0, 1.0, (base.b >= base.r + 0.014) && (base.g >= base.r + 0.016) && base.r < 0.46);
        let surf_foam_px = select(0.0, 1.0, (min_c > 0.56) && (base.b >= base.r - 0.012) && (sea_elev < 0.90) && (env_mask.r < 0.72));
        let warm_land = select(0.0, 1.0, (base.r - base.b >= 0.022) || ((min_c > 0.64) && (base.r > base.b + 0.008)));
        let img_water = clamp((cool_px + emerald_px + surf_foam_px) * (1.0 - warm_land), 0.0, 1.0);

        let open_ocean_mask = 1.0 - smoothstep(0.04, 0.52, env_mask.r);
        let gpu_water = clamp(mix(img_water, 1.0, open_ocean_mask) * (1.0 - smoothstep(0.75, 0.96, env_mask.r) * (1.0 - img_water)) * elev_water, 0.0, 1.0);
        let sdf_surf_fill = clamp((sdf_s.r - 0.55) * 2.2, 0.0, 1.0)
                          * (1.0 - smoothstep(0.3, 2.0, sea_elev))
                          * (1.0 - warm_land);
        let water_mask = clamp(max(gpu_water, sdf_surf_fill * use_sdf), 0.0, 1.0);

        let swash_margin = use_sdf * (1.0 - smoothstep(36.0, 58.0, sdf_s.g * ocean.coastal_params.x))
                         * (1.0 - smoothstep(0.20, 1.55, sea_elev));

        if (water_mask > 0.01 || swash_margin > 0.01) {
            let shore_dist_norm = mix(1.0, sdf_s.g, use_sdf);
            let raw_sdir = sdf_s.ba * 2.0 - vec2<f32>(1.0);
            let sdir_len = length(raw_sdir);
            let shore_dir = select(vec2<f32>(1.0, 0.0), raw_sdir / max(sdir_len, 1e-4), sdir_len > 0.05);

            let shoal_boost = use_sdf * (1.0 - smoothstep(0.04, 0.45, shore_dist_norm));

            let wave_strength = ocean.basis_east.w;
            let wave_scale = ocean.basis_north.w;
            let glint_scale = ocean.basis_up.w;
            let d_merc = ocean_merc_from_en(pos_en, blend_25d);
            let p_noise_base = ocean_noise_wrap(ocean.world_meta.xy + d_merc);

            // Smooth noise-warped open-ocean fetch: calm in sheltered coves (~0.34x), gradually building
            // seaward to full capped open-sea swells (~1.0x) without shoreline-parallel contour bands.
            let d_m_raw = shore_dist_norm * ocean.coastal_params.x;
            let n_cw1 = pgrad2_periodic_rot(p_noise_base, 1.0 / 512.0, 64.0, vec2<f32>(3.0, 4.0), vec2<f32>(71.3, 13.9)).x;
            let n_cw2 = pgrad2_periodic_rot(p_noise_base, 1.0 / 256.0, 128.0, vec2<f32>(5.0, 12.0), vec2<f32>(29.1, 87.7)).x;
            let warp_fetch = (n_cw1 - 0.5) * 110.0 + (n_cw2 - 0.5) * 42.0;
            let d_fetch = max(d_m_raw + warp_fetch, 0.0);
            let open_fetch = 1.0 - smoothstep(0.02, 0.54, clamp(env_mask.r + (n_cw1 - 0.5) * 0.18, 0.0, 1.0));
            let near_fetch = mix(0.32, 0.78, smoothstep(24.0, 320.0, d_fetch));
            let far_fetch = mix(0.38, 1.0, smoothstep(0.05, 0.92, open_fetch));
            let sea_exposure = mix(1.0, max(near_fetch, far_fetch), clamp(use_sdf + (1.0 - open_ocean_mask) * 0.65, 0.0, 1.0));

            // Dual-state smooth wind cross-fade (zero phase-spin glitch on wind angle/speed change!).
            let wa = ocean.blend_params.x;
            let wb = ocean.blend_params.y;
            let w_sum = max(wa + wb, 1e-4);
            var wave_res = vec4<f32>(0.0);
            if (wa > 0.002) {
                wave_res += eval_ocean_train(
                    d_merc,
                    p_noise_base,
                    ocean.train_a.xy,
                    ocean.train_a.z,
                    ocean.train_a.w,
                    wave_scale,
                    px_m,
                    sea_exposure,
                    ocean.train_a_phases0,
                    ocean.train_a_phases1,
                ) * (wa / w_sum);
            }
            if (wb > 0.002) {
                wave_res += eval_ocean_train(
                    d_merc,
                    p_noise_base,
                    ocean.train_b.xy,
                    ocean.train_b.z,
                    ocean.train_b.w,
                    wave_scale,
                    px_m,
                    sea_exposure,
                    ocean.train_b_phases0,
                    ocean.train_b_phases1,
                ) * (wb / w_sum);
            }

            let eff_wind_speed = (ocean.train_a.z * wa + ocean.train_b.z * wb) / w_sum;
            let eff_t = (ocean.train_a.w * wa + ocean.train_b.w * wb) / w_sum;

            // Optional Coastal Refracted Breaking Surf & Rhythmic Come-and-Go Beach Swash!
            var coastal_foam = 0.0;
            var surf_slope = vec2<f32>(0.0);
            var swash_cov = 0.0;
            var wet_darken = 0.0;
            if (use_sdf > 0.01 && shore_dist_norm < 0.96) {
                let warp_surf = (n_cw1 - 0.5) * 58.0 + (n_cw2 - 0.5) * 24.0;
                let d_eff = max(d_m_raw + warp_surf, 0.0);
                let surf_wl = clamp(mix(28.0, 48.0, clamp(eff_wind_speed / 16.0, 0.0, 1.0)), 24.0, 52.0);
                let k_surf = 6.2831853 / surf_wl;
                // Period ~8-11 s real ocean surf period; +omega*t moves crests INWARD toward the beach.
                let omega_surf = sqrt(9.81 * k_surf) * 0.58;

                // Blend wave propagation direction toward shoreline only close inside <125m (Snell refraction)
                let refract_w = 1.0 - smoothstep(15.0, 125.0, d_eff);
                let inc_dir = normalize((ocean.train_a.xy * wa + ocean.train_b.xy * wb) / w_sum + vec2<f32>(1e-4, 0.0));
                let prop_to_shore = normalize(mix(inc_dir, shore_dir, clamp(0.35 + 0.65 * refract_w, 0.0, 1.0)));

                // Along-shore broken wave-packet envelope breaks continuous rings into organic surf arcs.
                let p_along = ocean_noise_wrap(p_noise_base - prop_to_shore * (eff_t * 1.4));
                let n_along = pgrad2_periodic_rot(p_along, 1.0 / 256.0, 128.0, vec2<f32>(15.0, 8.0), vec2<f32>(83.1, 47.3)).x;
                let crest_patch = smoothstep(0.24, 0.76, n_along);
                let set_group = (0.55 + 0.45 * sin(0.22 * k_surf * d_eff + 0.18 * omega_surf * eff_t + n_cw1 * 3.1))
                              * (0.45 + 0.90 * crest_patch);
                let phase_var = k_surf * d_eff + omega_surf * eff_t + (n_cw1 - 0.5) * 2.4;
                let surf_aa = 1.0 - smoothstep(surf_wl * 0.16, surf_wl * 0.55, px_m);

                // Subtle refracting shoaling swells & broken whitewater crests inside [10m .. 125m] (no 320m rings).
                let shoal_swell_zone = smoothstep(8.0, 24.0, d_m_raw) * (1.0 - smoothstep(65.0, 130.0, d_m_raw));
                let breaker_zone = smoothstep(10.0, 26.0, d_m_raw) * (1.0 - smoothstep(55.0, 115.0, d_m_raw));
                let s_wave = 0.5 + 0.5 * sin(phase_var);
                let crest_line = pow(s_wave, 5.2) * breaker_zone * set_group * surf_aa;
                let n_foam = pgrad2_periodic_rot(
                    ocean_noise_wrap(p_noise_base + prop_to_shore * (eff_t * 1.1)),
                    1.0 / 32.0,
                    1024.0,
                    vec2<f32>(12.0, 5.0),
                    vec2<f32>(119.3, 61.7),
                ).x;
                let foam_noise = 0.42 + 0.58 * n_foam;
                let wake_trail = pow(0.5 + 0.5 * sin(phase_var - 0.70), 2.8) * breaker_zone * set_group * 0.24 * surf_aa;
                coastal_foam += (crest_line * 0.72 + wake_trail) * foam_noise * use_sdf * water_mask;
                surf_slope += prop_to_shore * (cos(phase_var) * shoal_swell_zone * set_group * 0.042 * surf_aa * use_sdf);

                // Rhythmic Come-and-Go Shoreline Swash: washes UP onto wet beach margin (-8.5m) and slides BACK (+34m).
                if (swash_margin > 0.01) {
                    let swash_ph = omega_surf * 0.76 * eff_t + (n_cw1 - 0.5) * 4.8 + (n_along - 0.5) * 2.6;
                    let rush_raw = 0.5 + 0.5 * sin(swash_ph);
                    let surge = pow(rush_raw, 0.72);
                    let surge_vel = cos(swash_ph);
                    let beach_s = d_m_raw - max(sea_elev, -0.25) * 10.0;
                    let front_s = mix(34.0, -8.5, surge * clamp(0.68 + 0.52 * n_along, 0.48, 1.22));
                    let swash_sheet = smoothstep(front_s - 2.5, front_s + 8.5, beach_s)
                                    * (1.0 - smoothstep(34.0, 54.0, d_m_raw))
                                    * swash_margin;
                    let swash_lip = exp(-pow((beach_s - front_s) / 4.2, 2.0))
                                  * (0.58 + 0.42 * max(surge_vel, 0.0))
                                  * swash_margin;
                    let n_lace = pgrad2_periodic_rot(
                        ocean_noise_wrap(p_noise_base - shore_dir * (eff_t * 0.75)),
                        1.0 / 16.0,
                        2048.0,
                        vec2<f32>(8.0, 15.0),
                        vec2<f32>(137.5, 91.1),
                    ).x;
                    let lace = smoothstep(0.26, 0.74, n_lace);
                    let foam_amt = (swash_sheet * 0.44 * lace + swash_lip * 0.84) * surf_aa * use_sdf;
                    coastal_foam += foam_amt;
                    let wet_zone = (1.0 - smoothstep(-8.0, 6.0, -beach_s)) * (1.0 - smoothstep(14.0, 34.0, d_m_raw));
                    wet_darken = wet_zone * swash_margin * 0.24 * use_sdf;
                    swash_cov = clamp(swash_sheet * 0.62 + swash_lip * 0.90, 0.0, 0.94) * use_sdf;
                }
            }

            let cov_eff = clamp(max(water_mask, swash_cov), 0.0, 1.0);
            var raw_slope = (wave_res.xy * wave_strength + surf_slope) * cov_eff;
            // Miche breaking-steepness soft limiter caps surface slope cleanly at ~0.40 without flat clipping.
            let slope_len = length(raw_slope);
            if (slope_len > 0.16) {
                let over = slope_len - 0.16;
                let capped = 0.16 + 0.24 * (over / (0.24 + over));
                raw_slope *= capped / slope_len;
            }
            let n_water = normalize(up_v - east_v * raw_slope.x - north_v * raw_slope.y);

            // Physically based Fresnel sky reflection + grazing-scaled deep pelagic water absorption.
            let ndotv = max(dot(n_water, view_dir), 0.04);
            let f_schlick = 0.02 + 0.98 * pow(1.0 - ndotv, 5.0);
            let grazing_t = smoothstep(0.92, 0.28, ndotv);
            var refl_dir = reflect(-view_dir, n_water);
            let refl_up = dot(refl_dir, local_up);
            if (refl_up < 0.045) {
                refl_dir = normalize(refl_dir + local_up * (0.045 - refl_up));
            }
            let sky_refl = eval_sky_reflection(refl_dir, local_up, sun_dir, real_sky_w, weather_dark, cloud_cov);

            let sun_elev = dot(local_up, sun_dir);
            let day_t = smoothstep(-0.08, 0.24, sun_elev);
            let base_water = base.rgb * lit * (1.0 - wet_darken);
            let pelagic_col = mix(
                vec3<f32>(0.024, 0.096, 0.185),
                vec3<f32>(0.038, 0.168, 0.248),
                shoal_boost,
            ) * max(lit, vec3<f32>(0.10));
            // Scale pelagic absorption with grazing angle so steep top-down zooms below 520 km
            // preserve orbital ocean tone instead of darkening when Fresnel reflection is minimal.
            let pelagic_weight = mix(0.08, mix(0.24, 0.40, water_mask), grazing_t);
            let pelagic_blended = mix(base_water, pelagic_col, pelagic_weight);
            let luma_w = vec3<f32>(0.2126, 0.7152, 0.0722);
            let base_luma = dot(base_water, luma_w);
            let blended_luma = dot(pelagic_blended, luma_w);
            let steep_floor_scale = select(
                1.0,
                clamp(base_luma / max(blended_luma, 1e-4), 1.0, 1.35),
                blended_luma < base_luma,
            );
            let water_albedo = mix(pelagic_blended * steep_floor_scale, pelagic_blended, grazing_t);

            // Subsurface turquoise glow through sunlit wave crests & shallow coastal water.
            let sss = pow(clamp(wave_res.z * 0.72, 0.0, 1.0), 1.4) * smoothstep(-0.04, 0.30, sun_elev)
                    * (0.35 + 0.65 * shoal_boost) * shadow_factor * (1.0 - weather_dark * 0.72);
            let sss_col = vec3<f32>(0.04, 0.32, 0.36) * sss * 0.38;

            // Crisp GGX-like solar specular glitter path widened softly by distant AA roughness + silver moon glitter.
            let half_s = normalize(sun_dir + view_dir);
            let ndoth_s = max(dot(n_water, half_s), 0.0);
            let rough = clamp(mix(0.06, 0.20, clamp(eff_wind_speed / 22.0, 0.0, 1.0)) + wave_res.w, 0.05, 0.34);
            let spec_exp = mix(420.0, 32.0, min(rough * 2.9, 1.0));
            let sun_vis_w = smoothstep(-0.03, 0.12, sun_elev) * (1.0 - clamp(weather_dark * 0.85 + cloud_cov * 0.45, 0.0, 0.96));
            let sun_spec_col = mix(vec3<f32>(1.0, 0.56, 0.22), vec3<f32>(1.0, 0.97, 0.90), smoothstep(0.0, 0.28, sun_elev));
            let sun_glint = sun_spec_col * (pow(ndoth_s, spec_exp) * 2.3 + pow(ndoth_s, 22.0) * 0.15)
                          * sun_vis_w * shadow_factor * glint_scale * cov_eff;

            var moon_glint = vec3<f32>(0.0);
            if (cam.moon_dir.w > 0.5 && sun_elev < 0.02) {
                let m_dir = normalize(cam.moon_dir.xyz);
                let m_elev = dot(local_up, m_dir);
                if (m_elev > 0.02) {
                    let half_m = normalize(m_dir + view_dir);
                    let ndoth_m = max(dot(n_water, half_m), 0.0);
                    let night_w = (1.0 - smoothstep(-0.10, 0.02, sun_elev)) * (1.0 - cloud_cov * 0.75);
                    moon_glint = vec3<f32>(0.62, 0.74, 0.92) * pow(ndoth_m, 96.0) * 0.45 * night_w * glint_scale * cov_eff;
                }
            }

            // Open-sea whitecap foam gated by fetch & capped Beaufort curve + coastal surf foam.
            let wind_foam_thresh = clamp((eff_wind_speed - 8.0) / 13.0, 0.0, 0.85) * smoothstep(0.25, 0.85, sea_exposure);
            let chop_noise = pgrad2_periodic_rot(
                ocean_noise_wrap(p_noise_base - ocean.train_a.xy * (eff_t * 1.1)),
                1.0 / 32.0,
                1024.0,
                vec2<f32>(5.0, 12.0),
                vec2<f32>(173.1, 23.9),
            ).x;
            let whitecap = smoothstep(0.62, 1.12, wave_res.z * wave_strength * (0.72 + 0.56 * chop_noise))
                         * wind_foam_thresh * 0.52 * water_mask;
            let total_foam = clamp(whitecap + coastal_foam, 0.0, 0.86);
            let foam_sun_tint = mix(vec3<f32>(0.98, 0.78, 0.62), vec3<f32>(0.94, 0.97, 1.0), smoothstep(0.02, 0.28, sun_elev));
            let foam_col = mix(vec3<f32>(0.16, 0.20, 0.28), foam_sun_tint, clamp(day_t * 0.9 + 0.12, 0.12, 1.0))
                         * mix(1.0, 0.68, weather_dark);

            let refl_w = clamp(f_schlick * 0.86 + mix(0.06, 0.10, grazing_t), 0.0, 0.90);
            var ocean_rgb = mix(water_albedo + sss_col, sky_refl, refl_w)
                          + sun_glint + moon_glint;
            ocean_rgb = mix(ocean_rgb, foam_col, total_foam);

            let dry_wet_rgb = mix(rgb, rgb * (1.0 - wet_darken), step(0.001, wet_darken));
            let ocean_blend = clamp(cov_eff * ocean.blend_params.z * ocean_on, 0.0, 1.0);
            rgb = mix(dry_wet_rgb, ocean_rgb, ocean_blend);
        }
    }

    // --- Night City Lights (orbital 3D Black Marble + crisp close-2.5D street illumination) ---
    let eff_night_city = max(dn_strength * city_strength, local_lit_blend * night_star_w * city_strength);
    if (eff_night_city > 0.001 && on_land > 0.001 && ndotl < 0.06) {
        let night_mask = (1.0 - smoothstep(-0.14, 0.06, ndotl)) * eff_night_city * on_land;
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
                let orbital_city_w = 1.0 - smoothstep(0.08, 0.82, blend_25d);
                let local_city_w = smoothstep(0.12, 0.85, blend_25d);
                var pinpoint = 0.86;
                var corridor_mod = 0.825;
                if (orbital_city_w > 0.01 && px_m < 4500.0) {
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
                let orbital_luma = (city_core * pinpoint + corridor * corridor_mod) * night_mask * orbital_city_w * 0.92;
                let base_luma = dot(base.rgb, vec3<f32>(0.299, 0.587, 0.114));
                let urban_mat = smoothstep(0.18, 0.52, base_luma)
                    * (1.0 - smoothstep(0.03, 0.10, base.g - max(base.r, base.b)))
                    * (1.0 - smoothstep(0.04, 0.12, base.b - base.r));
                let street_hi = smoothstep(0.36, 0.72, base_luma);
                let local_luma = (bm_srgb * (0.14 + 0.36 * urban_mat + 0.44 * street_hi))
                    * night_mask * local_city_w * 0.58;
                let warm_amber = vec3<f32>(0.97, 0.74, 0.38);
                let warm_white = vec3<f32>(1.0, 0.90, 0.68);
                rgb += mix(warm_amber, warm_white, city_core) * orbital_luma
                     + mix(vec3<f32>(0.96, 0.68, 0.32), vec3<f32>(1.0, 0.88, 0.66), street_hi) * local_luma;
            }
        }
    }

    // --- Weather- & Sky-Matched Aerial Perspective / Precipitation Mist ---
    let morph_fade_2d = 1.0 - smoothstep(0.0, 0.55, map_2d);
    if (blend_25d > 0.01 && morph_fade_2d > 0.001) {
        let view_dist = length(in.view_rel);
        let haze_density = mix(0.000018, 0.00016, weather_haze);
        let haze_near = mix(3500.0, 420.0, weather_haze);
        let haze_far = mix(52000.0, 9500.0, weather_haze);
        let haze_start = smoothstep(haze_near, haze_far, view_dist);
        let aerial_haze = clamp((1.0 - exp(-view_dist * haze_density)) * haze_start * blend_25d * mix(0.68, 0.84, weather_haze), 0.0, 0.82) * morph_fade_2d;

        let day_scale = clamp(max(cam.sky_bottom.b, max(cam.sky_bottom.r, cam.sky_bottom.g)), 0.0, 1.0);
        let classic_haze = vec3<f32>(0.73, 0.85, 0.96) * day_scale * mix(vec3<f32>(1.0), clamp(lit * 1.15, vec3<f32>(0.35), vec3<f32>(1.0)), dn_strength);
        let horiz_dir = normalize(in.view_rel - local_up * dot(in.view_rel, local_up) + local_up * 0.02);
        let phys_haze = eval_sky_reflection(horiz_dir, local_up, sun_dir, 1.0, weather_dark, cloud_cov);
        let horizon_haze_col = mix(classic_haze, phys_haze, max(real_sky_w, weather_haze * 0.65));
        rgb = mix(rgb, horizon_haze_col, aerial_haze);
    }

    if (map_2d > 0.001) {
        rgb = mix(rgb, base.rgb, smoothstep(0.0, 0.95, map_2d));
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

const TERRAIN_SHADOW_WGSL: &str = r#"
struct GpuCameraUniforms {
    view_proj_rte: mat4x4<f32>,
    inv_view_proj_rte: mat4x4<f32>,
    ecef_from_icrf: mat4x4<f32>,
    icrf_from_ecef: mat4x4<f32>,
    light_view_proj_rte: mat4x4<f32>,
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
    atmo_25d_params: vec4<f32>,
    weather_opt_params: vec4<f32>,
    cloud_tint_params: vec4<f32>,
    view_flags: vec4<f32>,
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

@vertex
fn vs_terrain_shadow(
    @builtin(vertex_index) local_v: u32,
    @builtin(instance_index) inst_id: u32,
) -> @builtin(position) vec4<f32> {
    let inst = instances[inst_id];
    let v = vertices[inst.tile_meta.x + local_v];
    let center_rel = (inst.center_high.xyz - cam.eye_high.xyz)
                   + (inst.center_low.xyz - cam.eye_low.xyz);
    let world_rel = center_rel + v.pos_u.xyz;
    return cam.light_view_proj_rte * vec4<f32>(world_rel, 1.0);
}
"#;

const CLOUD_SHELL_WGSL: &str = r#"
struct GpuCameraUniforms {
    view_proj_rte: mat4x4<f32>,
    inv_view_proj_rte: mat4x4<f32>,
    ecef_from_icrf: mat4x4<f32>,
    icrf_from_ecef: mat4x4<f32>,
    light_view_proj_rte: mat4x4<f32>,
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
    atmo_25d_params: vec4<f32>,
    weather_opt_params: vec4<f32>,
    cloud_tint_params: vec4<f32>,
    view_flags: vec4<f32>,
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
    let cover_boost = clamp(cam.cloud_tint_params.w, 0.0, 1.0) * below_clouds;
    let coarse_gate = mix(0.014, mix(0.212, 0.015, cover_boost), gate_25d);

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
    let layer_translucency = mix(1.0, mix(0.88, 0.96, cover_boost), below_clouds);
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
    let relief_light = normalize(mix(unlit_key, normalize(sun_dir + d * 0.28), max(dn_strength, cam.atmo_25d_params.x) * 0.75));
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
    let d_xy_r = sqrt(max(d.x * d.x + d.y * d.y, 0.0));
    if ((d.x < 0.0 && abs(d.y) < max(d_xy_r * 0.45, 0.06)) || abs(d.z) > 0.85) {
        let dn = normalize(d);
        let az = select(0.0, atan2(dn.y, dn.x), dot(dn.xy, dn.xy) > 1e-10);
        u_unwrapped = az * 0.15915494309 + 0.5;
        v_base = clamp(0.5 - asin(clamp(dn.z, -1.0, 1.0)) * 0.31830988618, 0.001, 0.999);
    }
    let u_base = fract(u_unwrapped + cam.cloud_params.y * 0.15915494309);
    let uv_lod = in.sun_ts_lod.w;

    let cover_boost = clamp(cam.cloud_tint_params.w, 0.0, 1.0) * in.grad_step_bic.w;
    let d_coarse = sample_cloud_density(vec2<f32>(u_base, v_base), max(uv_lod, 2.0));
    if (d_coarse + cover_boost * 0.65 <= in.params_fs.y) {
        return vec4<f32>(0.0);
    }

    let detail_w = in.params_fs.z;
    let mode_25d = in.params_fs.w;
    let h_off = cloud_density_to_height(d_coarse) - 0.22;
    let u = fract(u_base - in.uv_par.z * h_off);
    let v = clamp(v_base + in.uv_par.w * h_off, 0.001, 0.999);

    let p_cloud2 = vec2<f32>(d.x - d.z * 0.618034, d.y + d.z * 0.618034);
    let noise_uv = p_cloud2 * 26.5625 + vec2<f32>(cam.cloud_params.z * 0.000005625, 0.0);

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
        d_c_base + warp_delta.x * (d_e - d_c_lin) + warp_delta.y * (d_n - d_c_lin) + cover_boost * 0.55,
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
    let sunset = (1.0 / (1.0 + sn2 + 0.5 * sn2 * sn2)) * vec3<f32>(0.54, 0.24, 0.09);
    let sun_slope = smoothstep(-0.10, 0.65, dot(n_ts, in.sun_ts_lod.xyz));
    let term = in.light_ts_term.w;
    let dn_strength = clamp(cam.env_params.x, 0.0, 1.0);
    let real_sky_w = clamp(cam.atmo_25d_params.x, 0.0, 1.0);
    let weather_dark = clamp(cam.weather_opt_params.x, 0.0, 1.0);

    let day_cloud = (vec3<f32>(0.04, 0.06, 0.10) + cam.sun_color.rgb * term * mix(0.76, 1.12, sun_slope) + sunset) * tex_tint;
    let below_lit_blend = max(dn_strength * (1.0 - below_clouds), real_sky_w * below_clouds);
    var rgb = mix(flat_cloud, day_cloud, below_lit_blend);

    // Unified Base Weather API cloud storm darkening & tint (bright cumulus -> brooding nimbostratus!).
    let storm_dark_w = clamp(weather_dark, 0.0, 1.0) * below_clouds;
    if (storm_dark_w > 0.002) {
        let day_t = mix(1.0, smoothstep(-0.08, 0.22, ndotl), real_sky_w);
        let storm_underbelly = mix(vec3<f32>(0.03, 0.04, 0.06), vec3<f32>(0.24, 0.28, 0.34), day_t)
                             * mix(0.76, 1.12, relief_factor);
        rgb = mix(rgb, storm_underbelly, storm_dark_w * 0.78);
    }
    rgb *= cam.cloud_tint_params.xyz;

    let cloud_haze_t = in.view_ts_haze.w;
    if (cloud_haze_t > 0.001) {
        let day_scale = clamp(max(cam.sky_bottom.b, max(cam.sky_bottom.r, cam.sky_bottom.g)), 0.0, 1.0);
        let classic_fog = vec3<f32>(0.75, 0.86, 0.96) * day_scale;
        let day_t = smoothstep(-0.08, 0.24, ndotl);
        let golden_t = exp(-pow((ndotl - 0.03) * 7.5, 2.0));
        var phys_fog = mix(vec3<f32>(0.022, 0.036, 0.066), mix(vec3<f32>(0.92, 0.48, 0.20), vec3<f32>(0.72, 0.85, 0.96), smoothstep(0.02, 0.32, ndotl)), day_t)
                     + vec3<f32>(0.25, 0.12, 0.06) * golden_t;
        phys_fog = mix(phys_fog, vec3<f32>(0.28, 0.32, 0.38) * max(day_t, 0.08), weather_dark * 0.72);
        rgb = mix(rgb, mix(classic_fog, phys_fog, real_sky_w), cloud_haze_t);
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
    light_view_proj_rte: mat4x4<f32>,
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
    atmo_25d_params: vec4<f32>,
    weather_opt_params: vec4<f32>,
    cloud_tint_params: vec4<f32>,
    view_flags: vec4<f32>,
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
    let dn = max(clamp(cam.env_params.x, 0.0, 1.0), clamp(cam.atmo_25d_params.z * cam.env_params.z, 0.0, 1.0));
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
    light_view_proj_rte: mat4x4<f32>,
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
    atmo_25d_params: vec4<f32>,
    weather_opt_params: vec4<f32>,
    cloud_tint_params: vec4<f32>,
    view_flags: vec4<f32>,
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
@group(1) @binding(0) var shadow_map: texture_depth_2d;
@group(1) @binding(1) var shadow_sampler: sampler_comparison;
@group(2) @binding(0) var geo_albedo_tex: texture_2d<f32>;
@group(2) @binding(1) var geo_albedo_sampler: sampler;

struct GeoVsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_rel: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) base_color: vec4<f32>,
    @location(3) emissive: vec4<f32>,
    @location(4) material_params: vec4<f32>,
    @location(5) uv: vec2<f32>,
}

@vertex
fn vs_geo_object(
    @location(0) pos_u: vec4<f32>,
    @location(1) norm_v: vec4<f32>,
    @location(2) vert_color: vec4<f32>,
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
    out.base_color = inst.base_color * vert_color;
    out.emissive = inst.emissive;
    out.material_params = vec4<f32>(inst.col0.w, inst.col1.w, inst.col2.w, norm_v.w);
    out.uv = vec2<f32>(pos_u.w, norm_v.w);
    return out;
}

fn sample_geo_shadow(world_rel: vec3<f32>, n: vec3<f32>, sun_dir: vec3<f32>, shadow_radius: f32) -> f32 {
    let ndotl = max(dot(n, sun_dir), 0.0);
    let normal_bias = max(shadow_radius * 0.0014, 0.35) * (1.2 - 0.7 * ndotl);
    let lclip = cam.light_view_proj_rte * vec4<f32>(world_rel + n * normal_bias, 1.0);
    let ndc = lclip.xyz / max(lclip.w, 1e-5);
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    let depth_ref = clamp(ndc.z - 0.0005, 0.0, 0.9999);
    let dims = max(vec2<f32>(textureDimensions(shadow_map)), vec2<f32>(1.0));
    let texel = 1.0 / dims;
    var acc = 0.0;
    acc += textureSampleCompareLevel(shadow_map, shadow_sampler, clamp(uv + vec2<f32>(-texel.x, -texel.y), vec2<f32>(0.001), vec2<f32>(0.999)), depth_ref);
    acc += textureSampleCompareLevel(shadow_map, shadow_sampler, clamp(uv + vec2<f32>(texel.x, -texel.y), vec2<f32>(0.001), vec2<f32>(0.999)), depth_ref);
    acc += textureSampleCompareLevel(shadow_map, shadow_sampler, clamp(uv, vec2<f32>(0.001), vec2<f32>(0.999)), depth_ref);
    acc += textureSampleCompareLevel(shadow_map, shadow_sampler, clamp(uv + vec2<f32>(-texel.x, texel.y), vec2<f32>(0.001), vec2<f32>(0.999)), depth_ref);
    acc += textureSampleCompareLevel(shadow_map, shadow_sampler, clamp(uv + vec2<f32>(texel.x, texel.y), vec2<f32>(0.001), vec2<f32>(0.999)), depth_ref);
    let border = max(abs(ndc.x), abs(ndc.y));
    let in_bounds = (1.0 - smoothstep(0.84, 0.99, border)) * step(0.0, ndc.z) * step(ndc.z, 1.0);
    return mix(1.0, acc * 0.2, in_bounds);
}

@fragment
fn fs_geo_object(in: GeoVsOut) -> @location(0) vec4<f32> {
    let tex = textureSample(geo_albedo_tex, geo_albedo_sampler, in.uv);
    let tinted = in.base_color * tex;
    let accent_t = clamp(in.material_params.w * in.emissive.w, 0.0, 1.0);
    let albedo = mix(tinted.rgb, vec3<f32>(0.97, 0.99, 1.0), accent_t);

    if (in.material_params.z >= 0.5) {
        return vec4<f32>(clamp(albedo + in.emissive.rgb, vec3<f32>(0.0), vec3<f32>(1.0)), tinted.a);
    }

    let len_sq = dot(in.normal, in.normal);
    let n = select(vec3<f32>(0.0, 0.0, 1.0), normalize(in.normal), len_sq > 1e-10);
    let view_dir = select(vec3<f32>(0.0, 0.0, 1.0), normalize(-in.world_rel), dot(in.world_rel, in.world_rel) > 1e-8);
    let sun_dir = normalize(cam.sun_dir.xyz);

    let roughness = clamp(in.material_params.x, 0.05, 1.0);
    let metalness = clamp(in.material_params.y, 0.0, 1.0);

    let dn = max(clamp(cam.env_params.x, 0.0, 1.0), clamp(cam.atmo_25d_params.z * cam.env_params.z, 0.0, 1.0));
    let shadow_w = clamp(cam.atmo_25d_params.w, 0.0, 1.0);
    var shadow_f = 1.0;
    if (shadow_w > 0.001) {
        shadow_f = mix(1.0, sample_geo_shadow(in.world_rel, n, sun_dir, max(cam.weather_opt_params.w, 10.0)), shadow_w * (1.0 - cam.weather_opt_params.x * 0.55));
    }

    let ndotl_sun = max(dot(n, sun_dir), 0.0) * shadow_f;
    let ndotv = max(dot(n, view_dir), 0.0);
    let hemi = 0.5 + 0.5 * dot(n, view_dir);

    let studio_lit = vec3<f32>(0.42 + 0.24 * hemi) + vec3<f32>(0.52) * max(ndotl_sun, ndotv * 0.65);
    let day_lit = cam.ambient_color.rgb * (0.75 + 0.45 * hemi) + cam.sun_color.rgb * ndotl_sun * (1.0 - cam.weather_opt_params.x * 0.55);
    let diffuse_light = mix(studio_lit, day_lit, dn);

    let half_vec = select(view_dir, normalize(sun_dir + view_dir), dot(sun_dir + view_dir, sun_dir + view_dir) > 1e-6);
    let ndoth = max(dot(n, half_vec), 0.0);
    let shininess = mix(96.0, 8.0, roughness * roughness);
    let spec_strength = (1.0 - roughness * 0.75) * mix(0.22, 0.85, metalness);
    let f0 = mix(vec3<f32>(0.04), albedo, metalness);
    let spec = f0 * pow(ndoth, shininess) * spec_strength * max(ndotl_sun, 1.0 - dn);

    let diffuse_albedo = albedo * (1.0 - metalness * 0.65);
    let color = clamp(diffuse_albedo * diffuse_light + spec + in.emissive.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(color, tinted.a);
}
"#;

const GEO_SHADOW_WGSL: &str = r#"
struct GpuCameraUniforms {
    view_proj_rte: mat4x4<f32>,
    inv_view_proj_rte: mat4x4<f32>,
    ecef_from_icrf: mat4x4<f32>,
    icrf_from_ecef: mat4x4<f32>,
    light_view_proj_rte: mat4x4<f32>,
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
    atmo_25d_params: vec4<f32>,
    weather_opt_params: vec4<f32>,
    cloud_tint_params: vec4<f32>,
    view_flags: vec4<f32>,
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

@vertex
fn vs_geo_shadow(
    @location(0) pos_u: vec4<f32>,
    @location(1) norm_v: vec4<f32>,
    @builtin(instance_index) inst_idx: u32,
) -> @builtin(position) vec4<f32> {
    let inst = geo_instances[inst_idx];
    let center_rel = (inst.anchor_high.xyz - cam.eye_high.xyz)
                   + (inst.anchor_low.xyz - cam.eye_low.xyz);
    let local_offset = inst.col0.xyz * pos_u.x + inst.col1.xyz * pos_u.y + inst.col2.xyz * pos_u.z;
    return cam.light_view_proj_rte * vec4<f32>(center_rel + local_offset, 1.0);
}
"#;

const PARTICLE_VOLUME_WGSL: &str = r#"
struct GpuCameraUniforms {
    view_proj_rte: mat4x4<f32>,
    inv_view_proj_rte: mat4x4<f32>,
    ecef_from_icrf: mat4x4<f32>,
    icrf_from_ecef: mat4x4<f32>,
    light_view_proj_rte: mat4x4<f32>,
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
    atmo_25d_params: vec4<f32>,
    weather_opt_params: vec4<f32>,
    cloud_tint_params: vec4<f32>,
    view_flags: vec4<f32>,
}

struct GpuParticleLayerUniforms {
    basis_east: vec4<f32>,
    basis_north: vec4<f32>,
    basis_up: vec4<f32>,
    forward_bias: vec4<f32>,
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
    let center_rel = east * rel_e + north * rel_n + up * rel_z + layer.forward_bias.xyz;

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
    let bias_alt = dot(layer.forward_bias.xyz, up);
    let particle_alt_msl = layer.altitude_bounds.x + rel_z + bias_alt;
    let ground_alt_msl = layer.altitude_bounds.y;
    let ceiling_alt_msl = layer.altitude_bounds.z;
    let ceiling_fade_m = max(layer.altitude_bounds.w, 1.0);
    let above_ground = smoothstep(ground_alt_msl - 0.5, ground_alt_msl + 2.5, particle_alt_msl);
    let below_ceiling = 1.0 - smoothstep(ceiling_alt_msl - ceiling_fade_m, ceiling_alt_msl, particle_alt_msl);

    let alpha_mul = edge_fade * near_fade * above_ground * below_ceiling;
    if (alpha_mul <= 0.003 || layer.color.a <= 0.003) {
        return out;
    }

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
        // VelocityAligned: align streak or far-field curtain vertically along 3D wind + gravity velocity vector.
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
        if (layer.particle_meta.x == 4u) {
            // ProceduralCurtain: wide translucent rain-shaft / snow-squall veil without narrow streak clamp.
            hw = clamp(raw_hw, min_px * 0.5 * m_per_px, max_px * 0.5 * m_per_px);
            hh = clamp(raw_hh, min_px * 0.5 * m_per_px, max_px * 0.9 * m_per_px) * mix(0.45, 1.0, foreshorten);
        } else {
            let max_streak_w_px = min(max_px * 0.12, max(min_px * 2.2, 3.2));
            hw = clamp(raw_hw, min_px * 0.5 * m_per_px, max_streak_w_px * 0.5 * m_per_px);
            hh = clamp(raw_hh, min_px * 0.5 * m_per_px, max_px * 0.5 * m_per_px) * foreshorten;
            hh = max(hh, hw * 1.15);
        }
        uv = vec2<f32>(corner.x * 0.5 + 0.5, corner.y * 0.5 + 0.5);
    } else {
        // CameraFacing: 2D camera-aligned billboard with per-particle spin.
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

    // Day/night/sunset lighting tint so precipitation responds naturally to sun elevation.
    let dn = max(clamp(cam.env_params.x, 0.0, 1.0), clamp(cam.atmo_25d_params.z * cam.env_params.z, 0.0, 1.0));
    let sun_elev = dot(normalize(up), normalize(cam.sun_dir.xyz));
    let sun_w = smoothstep(-0.10, 0.28, sun_elev);
    let golden_w = exp(-pow((sun_elev - 0.03) * 7.5, 2.0));
    let sun_warm = mix(vec3<f32>(1.0, 0.68, 0.42), cam.sun_color.rgb, smoothstep(0.0, 0.28, sun_elev));
    let day_lit = mix(
        vec3<f32>(0.22, 0.27, 0.38),
        cam.ambient_color.rgb * 0.85 + sun_warm * 0.92 + vec3<f32>(0.18, 0.09, 0.03) * golden_w,
        sun_w,
    );
    let lit = mix(vec3<f32>(0.96, 0.98, 1.0), clamp(day_lit, vec3<f32>(0.20), vec3<f32>(1.12)), dn);
    let depth_fade = 0.80 + 0.20 * (1.0 - clamp(dist / max(extent_xy, 1.0), 0.0, 1.0));
    out.color = vec4<f32>(layer.color.rgb * lit, layer.color.a * alpha_mul * depth_fade);
    return out;
}

@fragment
fn fs_particle(in: ParticleVsOut) -> @location(0) vec4<f32> {
    let p = in.uv * 2.0 - vec2<f32>(1.0);
    let r = length(p);

    // Pre-compute simplistic 6-fold mathematical snowflake SDF in uniform control flow for fwidth().
    let sector = 1.04719755;
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
        // Pure mathematical tapered 2D raindrop streak.
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
    } else if (in.sprite_mode == 4u) {
        // ProceduralCurtain: ultra-soft atmospheric regional rain-shaft / snow-squall mist veil.
        let x_env = exp(-p.x * p.x * 2.4);
        let y_env = smoothstep(0.0, 0.34, in.uv.y) * (1.0 - smoothstep(0.62, 1.0, in.uv.y));
        let soft_stria = 0.86 + 0.14 * sin(in.uv.x * 9.4248 + in.uv.y * 1.8) * cos(in.uv.x * 15.708 - in.uv.y * 2.4);
        let mask = clamp(x_env * y_env * soft_stria, 0.0, 1.0);
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
    grid_res: u32,
    has_skirts: bool,
    last_used_frame: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SlabAllocPlan {
    /// Satisfied in-place inside the existing buffer without reallocating.
    InPlace { offset: u32 },
    /// Replaced the backing buffer (either compacted at the same capacity or grown within `max_vertices`).
    /// `copies` are coalesced `(src_vertex_offset, dst_vertex_offset, vertex_count)` spans.
    Rebuild {
        new_capacity: u32,
        copies: Vec<(u32, u32, u32)>,
        offset: u32,
    },
    /// Live current-frame tiles plus `needed` exceed the device's hard `max_vertices` ceiling.
    Exhausted,
}

/// Pure free-list + contiguous LRU eviction + compaction planner for `GpuTileSlabAllocator`.
struct SlabRegionIndex {
    capacity_vertices: u32,
    max_vertices: u32,
    resident: HashMap<TileKeyId, SlabEntry>,
    free_blocks: Vec<(u32, u32)>, // (offset, len)
    bump_cursor: u32,
}

impl SlabRegionIndex {
    fn new(initial_capacity_vertices: u32, max_vertices: u32) -> Self {
        let max_v = max_vertices.max(1);
        let cap = initial_capacity_vertices.max(65_536).min(max_v);
        Self {
            capacity_vertices: cap,
            max_vertices: max_v,
            resident: HashMap::new(),
            free_blocks: Vec::new(),
            bump_cursor: 0,
        }
    }

    fn take_best_fit(&mut self, needed: u32) -> Option<u32> {
        let (idx, (off, len)) = self
            .free_blocks
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, (_, l))| *l >= needed)
            .min_by_key(|(_, (_, l))| *l)?;
        self.free_blocks.swap_remove(idx);
        if len > needed {
            self.free_blocks.push((off + needed, len - needed));
        }
        Some(off)
    }

    fn try_bump(&mut self, needed: u32) -> Option<u32> {
        let end = self.bump_cursor.checked_add(needed)?;
        if end <= self.capacity_vertices {
            let off = self.bump_cursor;
            self.bump_cursor = end;
            Some(off)
        } else {
            None
        }
    }

    fn reclaim_tail(&mut self) {
        while let Some(&(o, l)) = self.free_blocks.last() {
            if o.saturating_add(l) == self.bump_cursor {
                self.bump_cursor = o;
                self.free_blocks.pop();
            } else {
                break;
            }
        }
    }

    fn coalesce_free_blocks(&mut self) {
        if self.free_blocks.is_empty() {
            return;
        }
        if self.free_blocks.len() >= 2 {
            self.free_blocks.sort_unstable_by_key(|(o, _)| *o);
            let mut merged: Vec<(u32, u32)> = Vec::with_capacity(self.free_blocks.len());
            for (o, l) in self.free_blocks.drain(..) {
                if l == 0 {
                    continue;
                }
                if let Some(last) = merged.last_mut() {
                    if last.0.saturating_add(last.1) == o {
                        last.1 = last.1.saturating_add(l);
                        continue;
                    }
                }
                merged.push((o, l));
            }
            self.free_blocks = merged;
        }
        self.reclaim_tail();
    }

    /// Inserts `(off, len)` into an already-sorted, coalesced `free_blocks` list, merges with
    /// immediate neighbors, reclaims any trailing space into `bump_cursor`, and returns the
    /// merged contiguous length of the affected free span.
    fn insert_free_block_sorted(&mut self, off: u32, len: u32) -> u32 {
        if len == 0 {
            return 0;
        }
        let pos = self.free_blocks.partition_point(|&(o, _)| o < off);
        let idx = if pos > 0
            && self.free_blocks[pos - 1]
                .0
                .saturating_add(self.free_blocks[pos - 1].1)
                == off
        {
            self.free_blocks[pos - 1].1 = self.free_blocks[pos - 1].1.saturating_add(len);
            pos - 1
        } else {
            self.free_blocks.insert(pos, (off, len));
            pos
        };
        if idx + 1 < self.free_blocks.len()
            && self.free_blocks[idx]
                .0
                .saturating_add(self.free_blocks[idx].1)
                == self.free_blocks[idx + 1].0
        {
            let next_len = self.free_blocks[idx + 1].1;
            self.free_blocks[idx].1 = self.free_blocks[idx].1.saturating_add(next_len);
            self.free_blocks.remove(idx + 1);
        }
        let merged_len = self.free_blocks[idx].1;
        self.reclaim_tail();
        merged_len
    }

    fn plan_alloc(&mut self, needed: u32, current_frame: u64) -> SlabAllocPlan {
        if needed == 0 || needed > self.max_vertices {
            return SlabAllocPlan::Exhausted;
        }

        // 1. Reuse best-fit free block.
        if let Some(off) = self.take_best_fit(needed) {
            return SlabAllocPlan::InPlace { offset: off };
        }

        // 2. Bump allocate if space remains in the current slab.
        if let Some(off) = self.try_bump(needed) {
            return SlabAllocPlan::InPlace { offset: off };
        }

        // 3. Coalesce existing free spans and evict stale LRU tiles until a contiguous
        //    free block >= needed (or reclaimed tail space) actually exists.
        self.coalesce_free_blocks();
        if let Some(off) = self.take_best_fit(needed) {
            return SlabAllocPlan::InPlace { offset: off };
        }
        if let Some(off) = self.try_bump(needed) {
            return SlabAllocPlan::InPlace { offset: off };
        }

        let mut stale: Vec<(TileKeyId, SlabEntry)> = self
            .resident
            .iter()
            .filter(|(_, e)| e.last_used_frame < current_frame)
            .map(|(k, e)| (*k, *e))
            .collect();
        stale.sort_unstable_by_key(|(_, e)| (e.last_used_frame, e.offset));

        for (k, e) in stale {
            self.resident.remove(&k);
            let merged_len = self.insert_free_block_sorted(e.offset, e.len);
            if merged_len >= needed {
                if let Some(off) = self.take_best_fit(needed) {
                    return SlabAllocPlan::InPlace { offset: off };
                }
            }
            if let Some(off) = self.try_bump(needed) {
                return SlabAllocPlan::InPlace { offset: off };
            }
        }

        // 4. All stale tiles are evicted; compact resident tiles (and grow only if needed and
        //    strictly within `self.max_vertices` so we never exceed `max_storage_buffer_binding_size`).
        let live_vertices: u64 = self.resident.values().map(|e| e.len as u64).sum();
        let required_total = live_vertices.saturating_add(needed as u64);
        if required_total > self.max_vertices as u64 {
            return SlabAllocPlan::Exhausted;
        }

        let new_capacity = if required_total <= self.capacity_vertices as u64 {
            // Fragmentation compaction within the existing slab capacity (zero growth).
            self.capacity_vertices
        } else {
            let doubled = (self.capacity_vertices as u64).saturating_mul(2);
            let padded = required_total.saturating_add(65_536);
            doubled.max(padded).min(self.max_vertices as u64) as u32
        };

        let mut live_entries: Vec<(TileKeyId, u32, u32)> = self
            .resident
            .iter()
            .map(|(k, e)| (*k, e.offset, e.len))
            .collect();
        live_entries.sort_unstable_by_key(|&(_, old_off, _)| old_off);

        let mut copies: Vec<(u32, u32, u32)> = Vec::new();
        let mut dst_cursor = 0u32;
        for (key, old_off, len) in live_entries {
            if let Some(entry) = self.resident.get_mut(&key) {
                entry.offset = dst_cursor;
            }
            if len > 0 {
                if let Some(last) = copies.last_mut() {
                    if last.0.saturating_add(last.2) == old_off
                        && last.1.saturating_add(last.2) == dst_cursor
                    {
                        last.2 = last.2.saturating_add(len);
                    } else {
                        copies.push((old_off, dst_cursor, len));
                    }
                } else {
                    copies.push((old_off, dst_cursor, len));
                }
                dst_cursor = dst_cursor.saturating_add(len);
            }
        }

        self.free_blocks.clear();
        self.capacity_vertices = new_capacity;
        let alloc_offset = dst_cursor;
        self.bump_cursor = dst_cursor.saturating_add(needed);

        SlabAllocPlan::Rebuild {
            new_capacity,
            copies,
            offset: alloc_offset,
        }
    }
}

/// Free-list GPU vertex slab allocator holding tile-local RTC vertices across frames.
struct GpuTileSlabAllocator {
    buffer: wgpu::Buffer,
    index: SlabRegionIndex,
}

impl GpuTileSlabAllocator {
    fn max_vertices_for_device(device: &wgpu::Device) -> u32 {
        let limits = device.limits();
        let max_bytes =
            (limits.max_storage_buffer_binding_size as u64).min(limits.max_buffer_size);
        let vert_size = std::mem::size_of::<RteTileVertex>() as u64;
        ((max_bytes / vert_size).min(u32::MAX as u64)).max(1) as u32
    }

    fn new(device: &wgpu::Device, initial_capacity_vertices: u32) -> Self {
        let max_vertices = Self::max_vertices_for_device(device);
        let index = SlabRegionIndex::new(initial_capacity_vertices, max_vertices);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("strata_earth_tile_vertex_slab"),
            size: (index.capacity_vertices as u64) * (std::mem::size_of::<RteTileVertex>() as u64),
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        Self { buffer, index }
    }

    fn alloc_range(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        needed: u32,
        current_frame: u64,
    ) -> Option<(u32, bool)> {
        let vert_size = std::mem::size_of::<RteTileVertex>() as u64;
        debug_assert_eq!(vert_size % wgpu::COPY_BUFFER_ALIGNMENT, 0);

        match self.index.plan_alloc(needed, current_frame) {
            SlabAllocPlan::InPlace { offset } => Some((offset, false)),
            SlabAllocPlan::Rebuild {
                new_capacity,
                copies,
                offset,
            } => {
                let new_buf = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("strata_earth_tile_vertex_slab_grown"),
                    size: (new_capacity as u64) * vert_size,
                    usage: wgpu::BufferUsages::STORAGE
                        | wgpu::BufferUsages::COPY_DST
                        | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                });
                if !copies.is_empty() {
                    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("strata_earth_slab_compact_copy"),
                    });
                    for (src_v, dst_v, count_v) in copies {
                        enc.copy_buffer_to_buffer(
                            &self.buffer,
                            (src_v as u64) * vert_size,
                            &new_buf,
                            (dst_v as u64) * vert_size,
                            (count_v as u64) * vert_size,
                        );
                    }
                    queue.submit(Some(enc.finish()));
                }
                self.buffer = new_buf;
                Some((offset, true))
            }
            SlabAllocPlan::Exhausted => None,
        }
    }

    fn upload_meshes(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        meshes: &[Arc<RetainedTileMesh>],
        active_keys: impl IntoIterator<Item = TileKeyId>,
        frame_idx: u64,
        budget: usize,
    ) -> (bool, bool) {
        let mut bind_group_dirty = false;
        let mut deferred_any = false;
        let mut uploaded = 0usize;

        // First mark already-resident entries for visible/fallback keys as fresh this frame so LRU
        // never evicts an active tile or fallback while uploading new tiles.
        for key in active_keys {
            if let Some(entry) = self.index.resident.get_mut(&key) {
                entry.last_used_frame = frame_idx;
            }
        }
        for mesh in meshes {
            if let Some(entry) = self.index.resident.get_mut(&mesh.key) {
                if entry.len == mesh.vertices.len() as u32
                    && entry.grid_res == mesh.grid_res
                    && entry.has_skirts == mesh.has_skirts
                {
                    entry.last_used_frame = frame_idx;
                }
            }
        }

        for mesh in meshes {
            let needed = mesh.vertices.len() as u32;
            if needed == 0 {
                continue;
            }
            if let Some(entry) = self.index.resident.get(&mesh.key) {
                if entry.revision == mesh.revision
                    && entry.len == needed
                    && entry.grid_res == mesh.grid_res
                    && entry.has_skirts == mesh.has_skirts
                {
                    continue;
                }
            }
            if uploaded >= budget {
                deferred_any = true;
                continue;
            }
            if let Some(old) = self.index.resident.remove(&mesh.key) {
                self.index.free_blocks.push((old.offset, old.len));
                self.index.coalesce_free_blocks();
            }
            let Some((off, rebuilt)) = self.alloc_range(device, queue, needed, frame_idx) else {
                deferred_any = true;
                continue;
            };
            if rebuilt {
                bind_group_dirty = true;
            }
            let byte_offset = (off as u64) * (std::mem::size_of::<RteTileVertex>() as u64);
            queue.write_buffer(
                &self.buffer,
                byte_offset,
                bytemuck::cast_slice(&mesh.vertices),
            );
            self.index.resident.insert(
                mesh.key,
                SlabEntry {
                    revision: mesh.revision,
                    offset: off,
                    len: needed,
                    grid_res: mesh.grid_res,
                    has_skirts: mesh.has_skirts,
                    last_used_frame: frame_idx,
                },
            );
            uploaded += 1;
        }
        (bind_group_dirty, deferred_any)
    }

    fn resident_slot(&self, key: TileKeyId, revision: u64) -> Option<(u32, u32, bool)> {
        let entry = self.index.resident.get(&key)?;
        let _ = revision;
        Some((entry.offset, entry.grid_res, entry.has_skirts))
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
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layers: u32,
        color_format: wgpu::TextureFormat,
    ) -> Self {
        let layers = layers.max(16);
        let format = if color_format.is_srgb() {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };
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
            format,
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

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuGeoVertex {
    pos_u: [f32; 4],
    norm_v: [f32; 4],
    color: [f32; 4],
}

struct CachedDynamicGpuMesh {
    revision: u64,
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    index_count: u32,
    last_used_seq: u64,
}

struct CachedGeoGpuMesh {
    revision: u64,
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    index_count: u32,
    has_vertex_alpha: bool,
    albedo_tex_id: Option<u64>,
    last_used_seq: u64,
}

struct CachedGeoTexture {
    revision: u64,
    width: u32,
    height: u32,
    has_alpha: bool,
    _texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    last_used_seq: u64,
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
        let rows_per_chunk = (nh as usize).div_ceil(workers);
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

/// Hardware GPU pass durations resolved asynchronously from `wgpu::Features::TIMESTAMP_QUERY`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GpuPassTimings {
    /// True when the active adapter/device supports `wgpu::Features::TIMESTAMP_QUERY`.
    pub supported: bool,
    /// Directional shadow-map pass duration on GPU (microseconds).
    pub shadow_pass_us: u32,
    /// Main 3D scene pass (terrain/atmosphere/ocean/buildings/clouds/objects) duration on GPU (microseconds).
    pub scene_3d_pass_us: u32,
    /// Standalone 2D overlay / cached-3D blit+composite pass duration on GPU (microseconds).
    pub overlay_pass_us: u32,
    /// Combined 3D GPU duration (`shadow_pass_us + scene_3d_pass_us`).
    pub total_3d_gpu_us: u32,
}

const GPU_TS_SLOTS: usize = 3;
const GPU_TS_QUERY_COUNT: u32 = 6; // [0..2]=shadow, [2..4]=main 3D, [4..6]=overlay/blit
const GPU_TS_RESOLVE_STRIDE: u64 = 256; // WebGPU QUERY_RESOLVE_BUFFER_ALIGNMENT

struct TimestampSlot {
    query_set: wgpu::QuerySet,
    resolve_buffer: wgpu::Buffer,
    readback_buffer: wgpu::Buffer,
    mapped_ready: Arc<std::sync::atomic::AtomicBool>,
    in_flight: bool,
    wrote_shadow: bool,
    wrote_main: bool,
    wrote_overlay: bool,
}

struct GpuTimestampProfiler {
    slots: Vec<TimestampSlot>,
    next_slot: usize,
    timestamp_period_ns: f64,
    last_timings: GpuPassTimings,
}

impl GpuTimestampProfiler {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let mut slots = Vec::with_capacity(GPU_TS_SLOTS);
        let resolve_bytes = GPU_TS_RESOLVE_STRIDE * 3;
        let readback_bytes = (GPU_TS_QUERY_COUNT as u64) * 8;
        for i in 0..GPU_TS_SLOTS {
            let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some(&format!("strata_gpu_ts_query_set_{i}")),
                ty: wgpu::QueryType::Timestamp,
                count: GPU_TS_QUERY_COUNT,
            });
            let resolve_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("strata_gpu_ts_resolve_{i}")),
                size: resolve_bytes,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("strata_gpu_ts_readback_{i}")),
                size: readback_bytes,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            slots.push(TimestampSlot {
                query_set,
                resolve_buffer,
                readback_buffer,
                mapped_ready: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                in_flight: false,
                wrote_shadow: false,
                wrote_main: false,
                wrote_overlay: false,
            });
        }
        Self {
            slots,
            next_slot: 0,
            timestamp_period_ns: (queue.get_timestamp_period() as f64).max(0.0),
            last_timings: GpuPassTimings {
                supported: true,
                ..Default::default()
            },
        }
    }

    fn poll_ready(&mut self, device: &wgpu::Device) {
        device.poll(wgpu::Maintain::Poll);
        let period_ns = self.timestamp_period_ns;
        for slot in &mut self.slots {
            if !slot.in_flight
                || !slot
                    .mapped_ready
                    .load(std::sync::atomic::Ordering::Acquire)
            {
                continue;
            }
            {
                let view = slot.readback_buffer.slice(..).get_mapped_range();
                if view.len() >= (GPU_TS_QUERY_COUNT as usize) * 8 {
                    let read_u64 = |idx: usize| -> u64 {
                        let off = idx * 8;
                        u64::from_le_bytes(view[off..off + 8].try_into().unwrap_or([0; 8]))
                    };
                    let ticks_to_us = |start: u64, end: u64| -> u32 {
                        let delta = end.saturating_sub(start) as f64;
                        ((delta * period_ns) / 1000.0).round().clamp(0.0, u32::MAX as f64) as u32
                    };
                    if slot.wrote_shadow {
                        self.last_timings.shadow_pass_us = ticks_to_us(read_u64(0), read_u64(1));
                    } else if slot.wrote_main {
                        self.last_timings.shadow_pass_us = 0;
                    }
                    if slot.wrote_main {
                        self.last_timings.scene_3d_pass_us = ticks_to_us(read_u64(2), read_u64(3));
                        self.last_timings.total_3d_gpu_us = self
                            .last_timings
                            .shadow_pass_us
                            .saturating_add(self.last_timings.scene_3d_pass_us);
                    }
                    if slot.wrote_overlay {
                        self.last_timings.overlay_pass_us = ticks_to_us(read_u64(4), read_u64(5));
                    }
                }
            }
            slot.readback_buffer.unmap();
            slot.mapped_ready
                .store(false, std::sync::atomic::Ordering::Release);
            slot.in_flight = false;
            slot.wrote_shadow = false;
            slot.wrote_main = false;
            slot.wrote_overlay = false;
        }
    }

    fn acquire_idle_slot(&mut self, device: &wgpu::Device) -> Option<usize> {
        self.poll_ready(device);
        let n = self.slots.len();
        for off in 0..n {
            let idx = (self.next_slot + off) % n;
            if !self.slots[idx].in_flight {
                self.next_slot = (idx + 1) % n;
                let s = &mut self.slots[idx];
                s.wrote_shadow = false;
                s.wrote_main = false;
                s.wrote_overlay = false;
                return Some(idx);
            }
        }
        None
    }

    fn resolve_and_copy(&self, slot_idx: usize, encoder: &mut wgpu::CommandEncoder) {
        let slot = &self.slots[slot_idx];
        // Vulkan requires resolving ONLY query indices that were actually written in this submission;
        // resolving unwritten queries blocks forever on VK_QUERY_RESULT_WAIT_BIT.
        if slot.wrote_shadow {
            encoder.resolve_query_set(&slot.query_set, 0..2, &slot.resolve_buffer, 0);
            encoder.copy_buffer_to_buffer(&slot.resolve_buffer, 0, &slot.readback_buffer, 0, 16);
        }
        if slot.wrote_main {
            encoder.resolve_query_set(
                &slot.query_set,
                2..4,
                &slot.resolve_buffer,
                GPU_TS_RESOLVE_STRIDE,
            );
            encoder.copy_buffer_to_buffer(
                &slot.resolve_buffer,
                GPU_TS_RESOLVE_STRIDE,
                &slot.readback_buffer,
                16,
                16,
            );
        }
        if slot.wrote_overlay {
            encoder.resolve_query_set(
                &slot.query_set,
                4..6,
                &slot.resolve_buffer,
                GPU_TS_RESOLVE_STRIDE * 2,
            );
            encoder.copy_buffer_to_buffer(
                &slot.resolve_buffer,
                GPU_TS_RESOLVE_STRIDE * 2,
                &slot.readback_buffer,
                32,
                16,
            );
        }
    }

    fn begin_async_map(&mut self, slot_idx: usize) {
        let slot = &mut self.slots[slot_idx];
        if !slot.wrote_shadow && !slot.wrote_main && !slot.wrote_overlay {
            return;
        }
        slot.in_flight = true;
        slot.mapped_ready
            .store(false, std::sync::atomic::Ordering::Release);
        let ready = Arc::clone(&slot.mapped_ready);
        slot.readback_buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |res| {
                if res.is_ok() {
                    ready.store(true, std::sync::atomic::Ordering::Release);
                }
            });
    }
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
    terrain_shadow_pipeline: wgpu::RenderPipeline,
    terrain_bind_group_layout: wgpu::BindGroupLayout,
    terrain_bind_group: wgpu::BindGroup,
    terrain_sampler: wgpu::Sampler,
    shadow_ocean_bgl: wgpu::BindGroupLayout,
    shadow_ocean_bind_group: wgpu::BindGroup,
    shadow_sampler: wgpu::Sampler,
    _shadow_fallback_tex: wgpu::Texture,
    shadow_fallback_view: wgpu::TextureView,
    shadow_target: Option<(u32, wgpu::Texture, wgpu::TextureView)>,
    ocean_uniform_buffer: wgpu::Buffer,
    coastal_sdf_sampler: wgpu::Sampler,
    _coastal_sdf_fallback_tex: wgpu::Texture,
    coastal_sdf_fallback_view: wgpu::TextureView,
    coastal_sdf_cached: Option<(u64, u64, u32, u32, wgpu::Texture, wgpu::TextureView)>,
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
    geo_shadow_pipeline: wgpu::RenderPipeline,
    geo_object_bind_group_layout: wgpu::BindGroupLayout,
    geo_object_bind_group: wgpu::BindGroup,
    geo_tex_bgl: wgpu::BindGroupLayout,
    geo_sampler: wgpu::Sampler,
    geo_default_tex_bg: wgpu::BindGroup,
    geo_instance_buffer: wgpu::Buffer,
    geo_instance_capacity: usize,
    geo_meshes: HashMap<u64, CachedGeoGpuMesh>,
    geo_textures: HashMap<u64, CachedGeoTexture>,
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
    scene_blit_pipeline: wgpu::RenderPipeline,
    overlay_bind_group_layout: wgpu::BindGroupLayout,
    overlay_sampler: wgpu::Sampler,
    depth_target: Option<(u32, u32, wgpu::TextureView)>,
    offscreen: Option<OffscreenTarget>,
    timestamp_profiler: Option<GpuTimestampProfiler>,
    frame_index: u64,
    last_geometry_rev: u64,
    last_terrain_frame: Option<Arc<RetainedTerrainFrame>>,
    cached_fallback_frames: HashMap<(usize, u32, u32), (u64, Arc<RetainedTerrainFrame>)>,
    is_software_adapter: bool,
}

/// Preferred upper cap for storage buffer bindings requested from adapters (512 MiB).
const PREFERRED_MAX_STORAGE_BUFFER_BINDING_SIZE: u32 = 512 * 1024 * 1024;
/// Preferred upper cap for GPU buffer allocations requested from adapters (512 MiB).
const PREFERRED_MAX_BUFFER_SIZE: u64 = 512 * 1024 * 1024;

/// Returns adapter-supported [`wgpu::Limits`] requesting up to [`MAX_ATLAS_LAYERS`] texture array
/// layers and expanded storage/buffer size limits (up to 512 MiB when supported by the adapter).
pub fn preferred_wgpu_limits(adapter: &wgpu::Adapter) -> wgpu::Limits {
    let adapter_limits = adapter.limits();
    let mut limits = wgpu::Limits::default();
    limits.max_texture_array_layers = adapter_limits
        .max_texture_array_layers
        .min(MAX_ATLAS_LAYERS)
        .max(limits.max_texture_array_layers)
        .min(adapter_limits.max_texture_array_layers);
    limits.max_storage_buffer_binding_size = adapter_limits
        .max_storage_buffer_binding_size
        .min(PREFERRED_MAX_STORAGE_BUFFER_BINDING_SIZE)
        .max(limits.max_storage_buffer_binding_size)
        .min(adapter_limits.max_storage_buffer_binding_size);
    limits.max_buffer_size = adapter_limits
        .max_buffer_size
        .min(PREFERRED_MAX_BUFFER_SIZE)
        .max(limits.max_buffer_size)
        .min(adapter_limits.max_buffer_size);
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
        let optional_ts = adapter.features() & wgpu::Features::TIMESTAMP_QUERY;
        let first_try = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some(label),
                required_features: optional_ts,
                required_limits: required_limits.clone(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ));
        match first_try {
            Ok((device, queue)) => return Ok((adapter, device, queue)),
            Err(_) if !optional_ts.is_empty() => {
                // Gracefully fall back if driver advertises TIMESTAMP_QUERY but rejects device creation.
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
                        errors.push(format!(
                            "{} ({:?}/{:?}): {e}",
                            info.name, info.backend, info.device_type
                        ));
                    }
                }
            }
            Err(e) => {
                errors.push(format!(
                    "{} ({:?}/{:?}): {e}",
                    info.name, info.backend, info.device_type
                ));
            }
        }
    }

    Err(format!("request_device failed: {}", errors.join("; ")))
}

impl Wgpu3DRenderer {
    /// Creates a headless `Wgpu3DRenderer` using any available hardware or software Vulkan/GL adapter.
    pub fn new_headless(color_format: wgpu::TextureFormat) -> Result<Self, String> {
        let instance = wgpu::Instance::default();
        let (adapter, device, queue) =
            request_working_adapter_and_device(&instance, None, "strata_render_wgpu_device")?;
        let is_software = adapter.get_info().device_type == wgpu::DeviceType::Cpu;
        let mut renderer = Self::from_device(Arc::new(device), Arc::new(queue), color_format);
        renderer.is_software_adapter = is_software;
        Ok(renderer)
    }

    /// Returns `true` when this renderer was initialized on a CPU software rasterizer (WARP / Lavapipe).
    #[inline]
    pub fn is_software_adapter(&self) -> bool {
        self.is_software_adapter
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
        let atlas = GpuImageryAtlas::new(&device, &queue, atlas_layers, color_format);

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
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 6,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
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
            &env_state.env_sampler,
        );
        let shadow_ocean_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("strata_3d_shadow_ocean_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("strata_3d_shadow_cmp_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let shadow_fallback_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("strata_3d_shadow_fallback_1x1"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_fallback_view = shadow_fallback_tex.create_view(&wgpu::TextureViewDescriptor::default());
        // Clear 1x1 fallback depth texture to 1.0 once so comparisons always pass when shadows are off.
        {
            let mut init_enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("strata_3d_shadow_fallback_init"),
            });
            let _ = init_enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("strata_3d_shadow_fallback_clear"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &shadow_fallback_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            queue.submit(Some(init_enc.finish()));
        }

        let ocean_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("strata_3d_ocean_uniforms"),
            size: std::mem::size_of::<GpuOceanUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(
            &ocean_uniform_buffer,
            0,
            bytemuck::bytes_of(&GpuOceanUniforms::disabled()),
        );
        let coastal_sdf_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("strata_3d_coastal_sdf_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let coastal_sdf_fallback_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("strata_3d_coastal_sdf_fallback_1x1"),
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
                texture: &coastal_sdf_fallback_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[255u8, 255, 128, 128],
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
        let coastal_sdf_fallback_view =
            coastal_sdf_fallback_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let shadow_ocean_bind_group = Self::create_shadow_ocean_bind_group(
            &device,
            &shadow_ocean_bgl,
            &shadow_fallback_view,
            &shadow_sampler,
            &ocean_uniform_buffer,
            &coastal_sdf_fallback_view,
            &coastal_sdf_sampler,
        );

        let terrain_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("strata_3d_terrain_pl"),
            bind_group_layouts: &[&terrain_bind_group_layout, &shadow_ocean_bgl],
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

        let terrain_shadow_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("strata_3d_terrain_shadow_wgsl"),
            source: wgpu::ShaderSource::Wgsl(TERRAIN_SHADOW_WGSL.into()),
        });
        let terrain_shadow_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("strata_3d_terrain_shadow_pl"),
                bind_group_layouts: &[&terrain_bind_group_layout],
                push_constant_ranges: &[],
            });
        let terrain_shadow_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("strata_3d_terrain_shadow_pipeline"),
                layout: Some(&terrain_shadow_layout),
                vertex: wgpu::VertexState {
                    module: &terrain_shadow_shader,
                    entry_point: Some("vs_terrain_shadow"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: None,
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
                    bias: wgpu::DepthBiasState {
                        constant: 2,
                        slope_scale: 2.0,
                        clamp: 0.0,
                    },
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
        let geo_tex_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("strata_3d_geo_tex_bgl"),
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
        let geo_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("strata_3d_geo_albedo_sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let default_geo_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("strata_3d_geo_default_1x1_white"),
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
                texture: &default_geo_tex,
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
        let default_geo_view = default_geo_tex.create_view(&Default::default());
        let geo_default_tex_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("strata_3d_geo_default_tex_bg"),
            layout: &geo_tex_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&default_geo_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&geo_sampler),
                },
            ],
        });
        let geo_object_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("strata_3d_geo_object_pl"),
            bind_group_layouts: &[&geo_object_bind_group_layout, &shadow_ocean_bgl, &geo_tex_bgl],
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
                        array_stride: std::mem::size_of::<GpuGeoVertex>() as u64,
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
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 32,
                                shader_location: 2,
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
        let geo_shadow_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("strata_3d_geo_shadow_wgsl"),
            source: wgpu::ShaderSource::Wgsl(GEO_SHADOW_WGSL.into()),
        });
        let geo_shadow_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("strata_3d_geo_shadow_pl"),
            bind_group_layouts: &[&geo_object_bind_group_layout],
            push_constant_ranges: &[],
        });
        let geo_shadow_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("strata_3d_geo_shadow_pipeline"),
                layout: Some(&geo_shadow_layout),
                vertex: wgpu::VertexState {
                    module: &geo_shadow_shader,
                    entry_point: Some("vs_geo_shadow"),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<GpuGeoVertex>() as u64,
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
                fragment: None,
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
                    bias: wgpu::DepthBiasState {
                        constant: 2,
                        slope_scale: 2.0,
                        clamp: 0.0,
                    },
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

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
        let scene_blit_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("strata_cached_3d_scene_blit_pipeline"),
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
                        blend: None,
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
        let timestamp_profiler = if device
            .features()
            .contains(wgpu::Features::TIMESTAMP_QUERY)
        {
            Some(GpuTimestampProfiler::new(&device, &queue))
        } else {
            None
        };

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
            terrain_shadow_pipeline,
            terrain_bind_group_layout,
            terrain_bind_group,
            terrain_sampler,
            shadow_ocean_bgl,
            shadow_ocean_bind_group,
            shadow_sampler,
            _shadow_fallback_tex: shadow_fallback_tex,
            shadow_fallback_view,
            shadow_target: None,
            ocean_uniform_buffer,
            coastal_sdf_sampler,
            _coastal_sdf_fallback_tex: coastal_sdf_fallback_tex,
            coastal_sdf_fallback_view,
            coastal_sdf_cached: None,
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
            geo_shadow_pipeline,
            geo_object_bind_group_layout,
            geo_object_bind_group,
            geo_tex_bgl,
            geo_sampler,
            geo_default_tex_bg,
            geo_instance_buffer,
            geo_instance_capacity,
            geo_meshes: HashMap::new(),
            geo_textures: HashMap::new(),
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
            scene_blit_pipeline,
            overlay_bind_group_layout,
            overlay_sampler,
            depth_target: None,
            offscreen: None,
            timestamp_profiler,
            frame_index: 0,
            last_geometry_rev: 0,
            last_terrain_frame: None,
            cached_fallback_frames: HashMap::new(),
            is_software_adapter: false,
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

    /// Number of currently cached GPU geodetic object mesh buffers.
    pub fn cached_geo_mesh_count(&self) -> usize {
        self.geo_meshes.len()
    }

    /// Number of currently cached GPU geodetic object albedo textures.
    pub fn cached_geo_texture_count(&self) -> usize {
        self.geo_textures.len()
    }

    /// Polls any completed asynchronous GPU timestamp query readbacks without stalling.
    pub fn poll_gpu_timestamps(&mut self) {
        if let Some(ref mut profiler) = self.timestamp_profiler {
            profiler.poll_ready(&self.device);
        }
    }

    /// Returns the most recently resolved hardware GPU pass timings (or `supported: false`
    /// when `wgpu::Features::TIMESTAMP_QUERY` is unavailable on the active adapter).
    pub fn last_gpu_pass_timings(&self) -> GpuPassTimings {
        self.timestamp_profiler
            .as_ref()
            .map(|p| p.last_timings)
            .unwrap_or_default()
    }

    /// Total GPU buffer, texture atlas, and render-target memory allocated by this 3D renderer.
    pub fn gpu_memory_bytes(&self) -> u64 {
        let mut bytes = self.camera_buffer.size()
            + self.instance_buffer.size()
            + self.ocean_uniform_buffer.size()
            + self.dynamic_uniform_buffer.size()
            + self.geo_instance_buffer.size()
            + self.particle_uniform_buffer.size()
            + self.env_state.star_buffer.size()
            + self.cloud_vbuf.size()
            + self.cloud_ibuf.size()
            + self.slab.buffer.size()
            + (ATLAS_TILE_SIZE as u64)
                * (ATLAS_TILE_SIZE as u64)
                * 4
                * (self.atlas.layer_owner.len() as u64);

        for topo in self.topologies.values() {
            bytes += topo.buffer.size();
        }
        for dm in self.dynamic_meshes.values() {
            bytes += dm.vbuf.size() + dm.ibuf.size();
        }
        for gm in self.geo_meshes.values() {
            bytes += gm.vbuf.size() + gm.ibuf.size();
        }
        for gt in self.geo_textures.values() {
            bytes += (gt.width as u64) * (gt.height as u64) * 4;
        }
        for pt in self.particle_textures.values() {
            bytes += (pt.width as u64) * (pt.height as u64) * 4;
        }
        if let Some((sz, _, _)) = self.shadow_target {
            bytes += (sz as u64) * (sz as u64) * 4;
        }
        if let Some((_, _, w, h, _, _)) = self.coastal_sdf_cached {
            bytes += (w as u64) * (h as u64) * 4;
        }
        if let Some((w, h, _)) = self.depth_target {
            bytes += (w as u64) * (h as u64) * 4;
        }
        if let Some(ref off) = self.offscreen {
            bytes += (off.width as u64) * (off.height as u64) * 8
                + (off.padded_bytes_per_row as u64) * (off.height as u64);
        }
        bytes
    }

    fn ensure_geo_texture(
        &mut self,
        tex: &crate::scene3d::RetainedTileTexture,
        seq: u64,
    ) -> Option<(u64, bool)> {
        if tex.width == 0
            || tex.height == 0
            || tex.rgba8.len() < (tex.width as usize) * (tex.height as usize) * 4
        {
            return None;
        }
        let need_upload = match self.geo_textures.get_mut(&tex.id) {
            Some(existing) => {
                existing.last_used_seq = seq;
                if existing.revision == tex.revision
                    && existing.width == tex.width
                    && existing.height == tex.height
                {
                    return Some((tex.id, existing.has_alpha));
                }
                true
            }
            None => true,
        };
        if need_upload {
            let byte_len = (tex.width as usize) * (tex.height as usize) * 4;
            let bytes = &tex.rgba8[..byte_len];
            let has_alpha = bytes.chunks_exact(4).any(|px| px[3] < 255);
            let gpu_tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("strata_3d_geo_albedo_tex"),
                size: wgpu::Extent3d {
                    width: tex.width,
                    height: tex.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &gpu_tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                bytes,
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
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("strata_3d_geo_albedo_bg"),
                layout: &self.geo_tex_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.geo_sampler),
                    },
                ],
            });
            self.geo_textures.insert(
                tex.id,
                CachedGeoTexture {
                    revision: tex.revision,
                    width: tex.width,
                    height: tex.height,
                    has_alpha,
                    _texture: gpu_tex,
                    bind_group,
                    last_used_seq: seq,
                },
            );
            return Some((tex.id, has_alpha));
        }
        None
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

    #[allow(clippy::too_many_arguments)]
    fn create_shadow_ocean_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        shadow_view: &wgpu::TextureView,
        shadow_sampler: &wgpu::Sampler,
        ocean_buffer: &wgpu::Buffer,
        coastal_sdf_view: &wgpu::TextureView,
        coastal_sdf_sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("strata_3d_shadow_ocean_bg"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(shadow_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: ocean_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(coastal_sdf_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(coastal_sdf_sampler),
                },
            ],
        })
    }

    fn sync_shadow_and_ocean_resources(
        &mut self,
        shadows_active: bool,
        shadow_size: u32,
        ocean_opt: Option<&RetainedOceanSurface>,
    ) {
        let mut need_rebind = false;
        let clamped_shadow = shadow_size.clamp(512, 4096);
        if shadows_active {
            let needs_alloc = match self.shadow_target {
                Some((sz, _, _)) => sz != clamped_shadow,
                None => true,
            };
            if needs_alloc {
                let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("strata_3d_shadow_map"),
                    size: wgpu::Extent3d {
                        width: clamped_shadow,
                        height: clamped_shadow,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Depth32Float,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
                self.shadow_target = Some((clamped_shadow, tex, view));
                need_rebind = true;
            }
        }

        let mut has_sdf = false;
        if let Some(ocean) = ocean_opt {
            if ocean.enabled && ocean.coastal_shoaling {
                if let Some(ref sdf) = ocean.coastal_sdf {
                    let expected_bytes = (sdf.width as usize)
                        .saturating_mul(sdf.height as usize)
                        .saturating_mul(4);
                    if sdf.width > 0 && sdf.height > 0 && sdf.rgba8.len() == expected_bytes {
                        has_sdf = true;
                        let needs_upload = match self.coastal_sdf_cached {
                            Some((id, rev, w, h, _, _)) => {
                                id != sdf.id || rev != sdf.revision || w != sdf.width || h != sdf.height
                            }
                            None => true,
                        };
                        if needs_upload {
                            let same_dims = match self.coastal_sdf_cached {
                                Some((_, _, w, h, _, _)) => w == sdf.width && h == sdf.height,
                                None => false,
                            };
                            if same_dims {
                                if let Some((id, rev, w, h, ref tex, _)) = self.coastal_sdf_cached.as_mut() {
                                    self.queue.write_texture(
                                        wgpu::TexelCopyTextureInfo {
                                            texture: tex,
                                            mip_level: 0,
                                            origin: wgpu::Origin3d::ZERO,
                                            aspect: wgpu::TextureAspect::All,
                                        },
                                        &sdf.rgba8,
                                        wgpu::TexelCopyBufferLayout {
                                            offset: 0,
                                            bytes_per_row: Some(*w * 4),
                                            rows_per_image: Some(*h),
                                        },
                                        wgpu::Extent3d {
                                            width: *w,
                                            height: *h,
                                            depth_or_array_layers: 1,
                                        },
                                    );
                                    *id = sdf.id;
                                    *rev = sdf.revision;
                                }
                            } else {
                                let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                                    label: Some("strata_3d_coastal_sdf_tex"),
                                    size: wgpu::Extent3d {
                                        width: sdf.width,
                                        height: sdf.height,
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
                                        texture: &tex,
                                        mip_level: 0,
                                        origin: wgpu::Origin3d::ZERO,
                                        aspect: wgpu::TextureAspect::All,
                                    },
                                    &sdf.rgba8,
                                    wgpu::TexelCopyBufferLayout {
                                        offset: 0,
                                        bytes_per_row: Some(sdf.width * 4),
                                        rows_per_image: Some(sdf.height),
                                    },
                                    wgpu::Extent3d {
                                        width: sdf.width,
                                        height: sdf.height,
                                        depth_or_array_layers: 1,
                                    },
                                );
                                let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
                                self.coastal_sdf_cached =
                                    Some((sdf.id, sdf.revision, sdf.width, sdf.height, tex, view));
                                need_rebind = true;
                            }
                        }
                    }
                }
            }
            let gpu_ocean = GpuOceanUniforms::from_retained(ocean, has_sdf);
            self.queue.write_buffer(
                &self.ocean_uniform_buffer,
                0,
                bytemuck::bytes_of(&gpu_ocean),
            );
        } else {
            self.queue.write_buffer(
                &self.ocean_uniform_buffer,
                0,
                bytemuck::bytes_of(&GpuOceanUniforms::disabled()),
            );
        }

        if need_rebind {
            let shadow_view = self
                .shadow_target
                .as_ref()
                .map(|(_, _, v)| v)
                .unwrap_or(&self.shadow_fallback_view);
            let sdf_view = self
                .coastal_sdf_cached
                .as_ref()
                .map(|(_, _, _, _, _, v)| v)
                .unwrap_or(&self.coastal_sdf_fallback_view);
            self.shadow_ocean_bind_group = Self::create_shadow_ocean_bind_group(
                &self.device,
                &self.shadow_ocean_bgl,
                shadow_view,
                &self.shadow_sampler,
                &self.ocean_uniform_buffer,
                sdf_view,
                &self.coastal_sdf_sampler,
            );
        }
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
        env_wrap_sampler: &wgpu::Sampler,
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
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(env_wrap_sampler),
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
        self.render_scene_to_view_internal(
            scene,
            target_view,
            target_width,
            target_height,
            viewport,
            unlimited_upload_budget,
            true,
            false,
            overlay_bind_group,
        );
    }

    /// Renders `scene` into `target_view` inside `viewport` without clearing prior color pixels,
    /// allowing previously rendered 2D UI layers behind the 3D viewport to remain intact.
    pub fn render_scene_preserving_color_to_view(
        &mut self,
        scene: &Scene3D,
        target_view: &wgpu::TextureView,
        target_width: u32,
        target_height: u32,
        viewport: Option<[f32; 4]>,
        unlimited_upload_budget: bool,
    ) {
        self.render_scene_to_view_internal(
            scene,
            target_view,
            target_width,
            target_height,
            viewport,
            unlimited_upload_budget,
            false,
            true,
            None,
        );
    }

    /// Renders multiple 3D viewports (`(viewport_rect, scene)`) sequentially into `target_view`
    /// without clearing prior viewports, and composites `overlay_bind_group` once on the final pass.
    pub fn render_viewports_and_overlay_to_view(
        &mut self,
        viewports: &[([f32; 4], &Scene3D)],
        target_view: &wgpu::TextureView,
        target_width: u32,
        target_height: u32,
        unlimited_upload_budget: bool,
        overlay_bind_group: Option<&wgpu::BindGroup>,
    ) {
        if viewports.is_empty() {
            if let Some(ov_bg) = overlay_bind_group {
                self.composite_overlay_to_view(target_view, ov_bg);
            }
            return;
        }
        let multi = viewports.len() > 1;
        for (idx, &(vp, scene)) in viewports.iter().enumerate() {
            let is_first = idx == 0;
            let is_last = idx + 1 == viewports.len();
            self.render_scene_to_view_internal(
                scene,
                target_view,
                target_width,
                target_height,
                Some(vp),
                unlimited_upload_budget,
                is_first,
                multi,
                if is_last { overlay_bind_group } else { None },
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_scene_to_view_internal(
        &mut self,
        scene: &Scene3D,
        target_view: &wgpu::TextureView,
        target_width: u32,
        target_height: u32,
        viewport: Option<[f32; 4]>,
        unlimited_upload_budget: bool,
        clear_color_target: bool,
        multi_viewport_mode: bool,
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

        let vp_w_u32 = viewport
            .map(|v| v[2].max(1.0).round() as u32)
            .unwrap_or_else(|| target_width.max(1));
        let vp_h_u32 = viewport
            .map(|v| v[3].max(1.0).round() as u32)
            .unwrap_or_else(|| target_height.max(1));

        let (cam, terrain_frame_opt) = if let Some(retained) = scene.retained_handle() {
            let terrain = retained.snapshot_terrain();
            let cam = retained.camera();
            (cam, terrain)
        } else if let Some(terrain) = scene.retained_terrain() {
            (terrain.camera, Some(Arc::clone(terrain)))
        } else {
            let scene_key = (scene as *const Scene3D as usize, vp_w_u32, vp_h_u32);
            let frame = if let Some((seq, existing)) = self.cached_fallback_frames.get_mut(&scene_key)
            {
                *seq = frame_idx;
                Arc::clone(existing)
            } else {
                let built = Arc::new(fallback_frame_from_scene3d(
                    scene,
                    vp_w_u32 as f32,
                    vp_h_u32 as f32,
                ));
                self.cached_fallback_frames
                    .insert(scene_key, (frame_idx, Arc::clone(&built)));
                built
            };
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
        let (light_vp_rte, shadow_radius_m, shadows_active) =
            compute_shadow_light_vp_rte(&cam, cam.shadow_map_size);
        let gpu_cam = GpuCameraUniforms::from_retained(
            &cam,
            vp_w,
            vp_h,
            light_vp_rte,
            shadow_radius_m,
            shadows_active,
        );
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&gpu_cam));

        // 2. Upload incremental tile meshes/textures, environment assets, and prepare draw instances.
        let mut draw_count = 0usize;
        let mut all_terrain_opaque = true;
        let mut terrain_batches: Vec<((u32, bool), std::ops::Range<u32>)> = Vec::new();
        let mut dynamic_draws: Vec<(u64, [f64; 3], [f32; 4], bool)> = Vec::new();
        let mut geo_opaque_batches: Vec<(u64, Option<u64>, std::ops::Range<u32>)> = Vec::new();
        let mut geo_transparent_batches: Vec<(u64, Option<u64>, std::ops::Range<u32>)> = Vec::new();
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
                frame.instances.iter().map(|inst| inst.key),
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
            let mut tagged_instances: Vec<((u32, bool), GpuTileInstance)> =
                Vec::with_capacity(frame.instances.len());
            for inst in &frame.instances {
                let Some((base_v, grid_res, has_skirts)) =
                    self.slab.resident_slot(inst.key, inst.mesh_revision)
                else {
                    continue;
                };
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
                tagged_instances.push((
                    (grid_res, has_skirts),
                    GpuTileInstance {
                        center_high: [cx_hi, cy_hi, cz_hi, 0.0],
                        center_low: [cx_lo, cy_lo, cz_lo, 0.0],
                        uv_rect: inst.uv_rect,
                        night_uv_rect: inst.night_uv_rect,
                        tint: eff_tint,
                        tile_meta: [base_v, tex_layer, u32::from(inst.unlit), night_layer],
                    },
                ));
            }
            tagged_instances.sort_by_key(|(topo, _)| *topo);
            let max_storage_bytes = (self.device.limits().max_storage_buffer_binding_size as u64)
                .min(self.device.limits().max_buffer_size);
            let max_instances = ((max_storage_bytes
                / std::mem::size_of::<GpuTileInstance>() as u64)
                .min(usize::MAX as u64) as usize)
                .max(1);
            if tagged_instances.len() > max_instances {
                tagged_instances.truncate(max_instances);
            }
            let mut instances_gpu: Vec<GpuTileInstance> =
                Vec::with_capacity(tagged_instances.len());
            for (topo, gpu_inst) in tagged_instances {
                let idx = instances_gpu.len() as u32;
                instances_gpu.push(gpu_inst);
                if let Some((last_topo, range)) = terrain_batches.last_mut() {
                    if *last_topo == topo && range.end == idx {
                        range.end = idx + 1;
                        continue;
                    }
                }
                terrain_batches.push((topo, idx..(idx + 1)));
            }

            let mut instance_resized = false;
            if instances_gpu.len() > self.instance_capacity {
                let new_cap = instances_gpu
                    .len()
                    .next_power_of_two()
                    .max(1024)
                    .min(max_instances);
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
                    &self.env_state.env_sampler,
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
                    existing.last_used_seq = frame_idx;
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
                            last_used_seq: frame_idx,
                        },
                    );
                }
                dynamic_draws.push((dm.id, dm.anchor_ecef, dm.color, dm.unlit));
            }

            for asset in frame.geo_mesh_assets.iter() {
                if asset.vertices.is_empty() || asset.indices.is_empty() {
                    continue;
                }
                let asset_tex_id = asset
                    .albedo_texture
                    .as_deref()
                    .and_then(|tex| self.ensure_geo_texture(tex, frame_idx))
                    .map(|(tid, _)| tid);
                let mut need_upload = true;
                if let Some(existing) = self.geo_meshes.get_mut(&asset.id) {
                    existing.last_used_seq = frame_idx;
                    existing.albedo_tex_id = asset_tex_id;
                    if existing.revision == asset.revision {
                        need_upload = false;
                    }
                }
                if need_upload {
                    let mut packed_verts: Vec<GpuGeoVertex> =
                        Vec::with_capacity(asset.vertices.len());
                    let mut has_vertex_alpha = false;
                    let vcols = asset.vertex_colors.as_deref();
                    for (vi, v) in asset.vertices.iter().enumerate() {
                        let c = vcols
                            .and_then(|cols| cols.get(vi).copied())
                            .unwrap_or([1.0, 1.0, 1.0, 1.0]);
                        if c[3] < 0.999 {
                            has_vertex_alpha = true;
                        }
                        packed_verts.push(GpuGeoVertex {
                            pos_u: v.pos_u,
                            norm_v: v.norm_v,
                            color: c,
                        });
                    }
                    let vb_bytes: &[u8] = bytemuck::cast_slice(&packed_verts);
                    let ib_bytes: &[u8] = bytemuck::cast_slice(&asset.indices);
                    let mut need_recreate = true;
                    if let Some(existing) = self.geo_meshes.get_mut(&asset.id) {
                        if existing.vbuf.size() >= vb_bytes.len() as u64
                            && existing.ibuf.size() >= ib_bytes.len() as u64
                        {
                            self.queue.write_buffer(&existing.vbuf, 0, vb_bytes);
                            self.queue.write_buffer(&existing.ibuf, 0, ib_bytes);
                            existing.revision = asset.revision;
                            existing.index_count = asset.indices.len() as u32;
                            existing.has_vertex_alpha = has_vertex_alpha;
                            existing.albedo_tex_id = asset_tex_id;
                            existing.last_used_seq = frame_idx;
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
                            CachedGeoGpuMesh {
                                revision: asset.revision,
                                vbuf,
                                ibuf,
                                index_count: asset.indices.len() as u32,
                                has_vertex_alpha,
                                albedo_tex_id: asset_tex_id,
                                last_used_seq: frame_idx,
                            },
                        );
                    }
                }
            }

            if !frame.geo_instances.is_empty() {
                let mut opaque_items: Vec<(u64, Option<u64>, GpuGeoInstance)> =
                    Vec::with_capacity(frame.geo_instances.len());
                let mut transparent_items: Vec<(f64, u64, Option<u64>, GpuGeoInstance)> =
                    Vec::new();

                for inst in frame.geo_instances.iter() {
                    let inst_tex_info = inst
                        .albedo_texture
                        .as_deref()
                        .and_then(|tex| self.ensure_geo_texture(tex, frame_idx));
                    let Some((mesh_has_alpha, mesh_tex_id)) =
                        self.geo_meshes.get_mut(&inst.mesh_id).map(|m| {
                            m.last_used_seq = frame_idx;
                            (m.has_vertex_alpha, m.albedo_tex_id)
                        })
                    else {
                        continue;
                    };
                    let (eff_tex_id, tex_has_alpha) = if let Some((tid, t_alpha)) = inst_tex_info {
                        (Some(tid), t_alpha)
                    } else if let Some(tid) = mesh_tex_id {
                        let t_alpha = self
                            .geo_textures
                            .get_mut(&tid)
                            .map(|t| {
                                t.last_used_seq = frame_idx;
                                t.has_alpha
                            })
                            .unwrap_or(false);
                        (Some(tid), t_alpha)
                    } else {
                        (None, false)
                    };

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
                    let is_transparent =
                        inst.base_color[3] < 0.999 || mesh_has_alpha || tex_has_alpha;
                    if !is_transparent {
                        opaque_items.push((inst.mesh_id, eff_tex_id, gpu_inst));
                    } else {
                        let dx = inst.anchor_ecef[0] - cam.eye_ecef[0];
                        let dy = inst.anchor_ecef[1] - cam.eye_ecef[1];
                        let dz = inst.anchor_ecef[2] - cam.eye_ecef[2];
                        let dist_sq = dx * dx + dy * dy + dz * dz;
                        transparent_items.push((dist_sq, inst.mesh_id, eff_tex_id, gpu_inst));
                    }
                }

                opaque_items.sort_by_key(|(mesh_id, tex_id, _)| (*mesh_id, *tex_id));
                // Sort transparent instances back-to-front (largest distance from camera eye first).
                transparent_items.sort_by(|a, b| b.0.total_cmp(&a.0));
                staged_geo_instances.reserve(opaque_items.len() + transparent_items.len());

                let push_batches =
                    |items: Vec<(u64, Option<u64>, GpuGeoInstance)>,
                     staged: &mut Vec<GpuGeoInstance>,
                     batches: &mut Vec<(u64, Option<u64>, std::ops::Range<u32>)>| {
                        for (mesh_id, tex_id, gpu_inst) in items {
                            let idx = staged.len() as u32;
                            staged.push(gpu_inst);
                            if let Some((last_id, last_tex, range)) = batches.last_mut() {
                                if *last_id == mesh_id && *last_tex == tex_id && range.end == idx {
                                    range.end = idx + 1;
                                    continue;
                                }
                            }
                            batches.push((mesh_id, tex_id, idx..(idx + 1)));
                        }
                    };
                push_batches(
                    opaque_items,
                    &mut staged_geo_instances,
                    &mut geo_opaque_batches,
                );
                let sorted_transparent: Vec<(u64, Option<u64>, GpuGeoInstance)> = transparent_items
                    .into_iter()
                    .map(|(_, mid, tid, gi)| (mid, tid, gi))
                    .collect();
                push_batches(
                    sorted_transparent,
                    &mut staged_geo_instances,
                    &mut geo_transparent_batches,
                );
            }

            // Evict deleted/unreferenced geo meshes, geo textures, dynamic meshes, and fallback scenes
            // after a 32-frame grace window (prevents VRAM leaks while allowing multi-viewport sharing).
            const GEO_CACHE_EVICT_FRAMES: u64 = 32;
            self.geo_meshes
                .retain(|_, m| frame_idx.saturating_sub(m.last_used_seq) <= GEO_CACHE_EVICT_FRAMES);
            self.geo_textures
                .retain(|_, t| frame_idx.saturating_sub(t.last_used_seq) <= GEO_CACHE_EVICT_FRAMES);
            self.dynamic_meshes
                .retain(|_, m| frame_idx.saturating_sub(m.last_used_seq) <= GEO_CACHE_EVICT_FRAMES);
            self.cached_fallback_frames
                .retain(|_, (seq, _)| frame_idx.saturating_sub(*seq) <= GEO_CACHE_EVICT_FRAMES);

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
                            RetainedParticleSprite::ProceduralCurtain => (4u32, None),
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
                            forward_bias: [
                                layer.forward_bias_world[0],
                                layer.forward_bias_world[1],
                                layer.forward_bias_world[2],
                                0.0,
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

        let ocean_opt = terrain_frame_opt.as_ref().and_then(|f| f.ocean.as_deref());
        self.sync_shadow_and_ocean_resources(shadows_active, cam.shadow_map_size, ocean_opt);

        let topology_batches: Vec<(wgpu::Buffer, u32, std::ops::Range<u32>)> = terrain_batches
            .into_iter()
            .filter_map(|((g, s), range)| {
                let (buf, cnt) = self.ensure_topology(g, s);
                if cnt > 0 && !range.is_empty() {
                    Some((buf, cnt, range))
                } else {
                    None
                }
            })
            .collect();
        let depth_view = self.ensure_depth_view(target_width.max(1), target_height.max(1));

        let clear_op = if !clear_color_target {
            wgpu::LoadOp::Load
        } else if multi_viewport_mode {
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
        } else {
            match cam.background {
                Background3D::Transparent => wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                Background3D::Solid(c) => wgpu::LoadOp::Clear(wgpu::Color {
                    r: c.r() as f64,
                    g: c.g() as f64,
                    b: c.b() as f64,
                    a: c.a() as f64,
                }),
                Background3D::VerticalGradient { .. } => wgpu::LoadOp::Clear(wgpu::Color::BLACK),
            }
        };

        let tp_pass = std::time::Instant::now();
        let ts_slot = self
            .timestamp_profiler
            .as_mut()
            .and_then(|p| p.acquire_idle_slot(&self.device));
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("strata_3d_render_encoder"),
            });

        // Opt-in directional shadow map pass for 2.5D mountains & opaque 3D objects (0.00 ms when off).
        let mut wrote_shadow_ts = false;
        if shadows_active && (draw_count > 0 || !geo_opaque_batches.is_empty()) {
            if let Some((_, _, ref shadow_view)) = self.shadow_target {
                let shadow_ts = ts_slot.and_then(|slot_idx| {
                    self.timestamp_profiler.as_ref().map(|p| {
                        wrote_shadow_ts = true;
                        wgpu::RenderPassTimestampWrites {
                            query_set: &p.slots[slot_idx].query_set,
                            beginning_of_pass_write_index: Some(0),
                            end_of_pass_write_index: Some(1),
                        }
                    })
                });
                let mut spass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("strata_3d_shadow_pass"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: shadow_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: shadow_ts,
                    occlusion_query_set: None,
                });
                if !topology_batches.is_empty() {
                    spass.set_pipeline(&self.terrain_shadow_pipeline);
                    spass.set_bind_group(0, &self.terrain_bind_group, &[]);
                    for (idx_buf, idx_count, range) in &topology_batches {
                        spass.set_index_buffer(idx_buf.slice(..), wgpu::IndexFormat::Uint32);
                        spass.draw_indexed(0..*idx_count, 0, range.clone());
                    }
                }
                if !geo_opaque_batches.is_empty() {
                    spass.set_pipeline(&self.geo_shadow_pipeline);
                    spass.set_bind_group(0, &self.geo_object_bind_group, &[]);
                    for (mesh_id, _, range) in &geo_opaque_batches {
                        let Some(gpu_mesh) = self.geo_meshes.get(mesh_id) else {
                            continue;
                        };
                        spass.set_vertex_buffer(0, gpu_mesh.vbuf.slice(..));
                        spass.set_index_buffer(gpu_mesh.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                        spass.draw_indexed(0..gpu_mesh.index_count, 0, range.clone());
                    }
                }
            }
        }

        {
            let main_ts = ts_slot.and_then(|slot_idx| {
                self.timestamp_profiler.as_ref().map(|p| {
                    wgpu::RenderPassTimestampWrites {
                        query_set: &p.slots[slot_idx].query_set,
                        beginning_of_pass_write_index: Some(2),
                        end_of_pass_write_index: Some(3),
                    }
                })
            });
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
                timestamp_writes: main_ts,
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

            if terrain_first && !topology_batches.is_empty() {
                pass.set_pipeline(&self.terrain_opaque_pipeline);
                pass.set_bind_group(0, &self.terrain_bind_group, &[]);
                pass.set_bind_group(1, &self.shadow_ocean_bind_group, &[]);
                for (idx_buf, idx_count, range) in &topology_batches {
                    pass.set_index_buffer(idx_buf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..*idx_count, 0, range.clone());
                }
            }

            let need_scissored_solid_bg = (multi_viewport_mode || !clear_color_target)
                && matches!(cam.background, Background3D::Solid(_));
            if need_scissored_solid_bg
                || matches!(cam.background, Background3D::VerticalGradient { .. })
                || cam.stars_visible
                || cam.sun_visible
                || cam.moon_visible
                || cam.realistic_25d_sky > 0.001
            {
                let grad_opaque = match cam.background {
                    Background3D::VerticalGradient { top, bottom } => {
                        (multi_viewport_mode || !clear_color_target)
                            && top.a() >= 0.999
                            && bottom.a() >= 0.999
                    }
                    Background3D::Solid(c) => c.a() >= 0.999,
                    Background3D::Transparent => false,
                };
                let sky_opaque = grad_opaque
                    || cam.stars_visible
                    || cam.sun_visible
                    || cam.moon_visible
                    || cam.realistic_25d_sky > 0.001;
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
                && (cam.local_25d_blend < 0.85 || cam.night_stars_25d > 0.001)
            {
                pass.set_pipeline(&self.star_pipeline);
                pass.set_bind_group(0, &self.star_bind_group, &[]);
                pass.draw(0..6, 0..self.env_state.star_count);
            }

            if !terrain_first && !topology_batches.is_empty() {
                pass.set_pipeline(&self.terrain_pipeline);
                pass.set_bind_group(0, &self.terrain_bind_group, &[]);
                pass.set_bind_group(1, &self.shadow_ocean_bind_group, &[]);
                for (idx_buf, idx_count, range) in &topology_batches {
                    pass.set_index_buffer(idx_buf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..*idx_count, 0, range.clone());
                }
            }

            if !geo_opaque_batches.is_empty() {
                pass.set_pipeline(&self.geo_object_pipeline);
                pass.set_bind_group(0, &self.geo_object_bind_group, &[]);
                pass.set_bind_group(1, &self.shadow_ocean_bind_group, &[]);
                for (mesh_id, tex_id_opt, range) in &geo_opaque_batches {
                    let Some(gpu_mesh) = self.geo_meshes.get(mesh_id) else {
                        continue;
                    };
                    let tex_bg = tex_id_opt
                        .and_then(|tid| self.geo_textures.get(&tid))
                        .map(|t| &t.bind_group)
                        .unwrap_or(&self.geo_default_tex_bg);
                    pass.set_bind_group(2, tex_bg, &[]);
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
                pass.set_bind_group(1, &self.shadow_ocean_bind_group, &[]);
                for (mesh_id, tex_id_opt, range) in &geo_transparent_batches {
                    let Some(gpu_mesh) = self.geo_meshes.get(mesh_id) else {
                        continue;
                    };
                    let tex_bg = tex_id_opt
                        .and_then(|tid| self.geo_textures.get(&tid))
                        .map(|t| &t.bind_group)
                        .unwrap_or(&self.geo_default_tex_bg);
                    pass.set_bind_group(2, tex_bg, &[]);
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
        if let (Some(slot_idx), Some(ref mut profiler)) = (ts_slot, self.timestamp_profiler.as_mut())
        {
            profiler.slots[slot_idx].wrote_shadow = wrote_shadow_ts;
            profiler.slots[slot_idx].wrote_main = true;
            profiler.resolve_and_copy(slot_idx, &mut encoder);
        }
        self.queue.submit(Some(encoder.finish()));
        if let (Some(slot_idx), Some(ref mut profiler)) = (ts_slot, self.timestamp_profiler.as_mut())
        {
            profiler.begin_async_map(slot_idx);
        }
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

    /// Blits a cached offscreen 3D scene color attachment into `target_view` and composites
    /// an optional 2D UI overlay on top in a single lightweight fullscreen render pass.
    pub fn composite_cached_scene_and_overlay_to_view(
        &mut self,
        target_view: &wgpu::TextureView,
        cached_scene_bind_group: &wgpu::BindGroup,
        overlay_bind_group: Option<&wgpu::BindGroup>,
    ) {
        let ts_slot = self
            .timestamp_profiler
            .as_mut()
            .and_then(|p| p.acquire_idle_slot(&self.device));
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("strata_cached_3d_composite_encoder"),
            });
        {
            let ov_ts = ts_slot.and_then(|slot_idx| {
                self.timestamp_profiler.as_ref().map(|p| {
                    wgpu::RenderPassTimestampWrites {
                        query_set: &p.slots[slot_idx].query_set,
                        beginning_of_pass_write_index: Some(4),
                        end_of_pass_write_index: Some(5),
                    }
                })
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("strata_cached_3d_composite_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: ov_ts,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.scene_blit_pipeline);
            pass.set_bind_group(0, cached_scene_bind_group, &[]);
            pass.draw(0..3, 0..1);
            if let Some(ov_bg) = overlay_bind_group {
                pass.set_pipeline(&self.overlay_pipeline);
                pass.set_bind_group(0, ov_bg, &[]);
                pass.draw(0..3, 0..1);
            }
        }
        if let (Some(slot_idx), Some(ref mut profiler)) = (ts_slot, self.timestamp_profiler.as_mut())
        {
            profiler.slots[slot_idx].wrote_overlay = true;
            profiler.resolve_and_copy(slot_idx, &mut encoder);
        }
        self.queue.submit(Some(encoder.finish()));
        if let (Some(slot_idx), Some(ref mut profiler)) = (ts_slot, self.timestamp_profiler.as_mut())
        {
            profiler.begin_async_map(slot_idx);
        }
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

#[cfg(test)]
mod slab_tests {
    use super::*;

    fn tile_key(id: u32) -> TileKeyId {
        TileKeyId {
            level: 4,
            col: id,
            row: 0,
            face: 0,
        }
    }

    fn insert_resident(index: &mut SlabRegionIndex, id: u32, off: u32, len: u32, frame: u64) {
        index.resident.insert(
            tile_key(id),
            SlabEntry {
                revision: 1,
                offset: off,
                len,
                grid_res: 16,
                has_skirts: true,
                last_used_frame: frame,
            },
        );
    }

    #[test]
    fn test_slab_lru_evicts_until_contiguous_block_without_spurious_growth() {
        // Capacity = 65_536 vertices; cap = 131_072 vertices.
        // Fill the slab with interleaved stale (frame 1) and live (frame 2) tiles so
        // the first two oldest stale tiles are NOT adjacent (reclaimed >= needed, yet
        // no contiguous block of size 16_384 exists until additional stale tiles evict).
        let mut idx = SlabRegionIndex::new(65_536, 131_072);
        // Layout across [0 .. 65_536):
        // [0..8192)     stale (f=1, id=1)
        // [8192..16384) live  (f=2, id=2)
        // [16384..24576) stale (f=1, id=3)
        // [24576..32768) live  (f=2, id=4)
        // [32768..40960) stale (f=1, id=5)
        // [40960..49152) stale (f=1, id=6) <- adjacent to id=5! together = 16_384
        // [49152..65536) live  (f=2, id=7)
        insert_resident(&mut idx, 1, 0, 8_192, 1);
        insert_resident(&mut idx, 2, 8_192, 8_192, 2);
        insert_resident(&mut idx, 3, 16_384, 8_192, 1);
        insert_resident(&mut idx, 4, 24_576, 8_192, 2);
        insert_resident(&mut idx, 5, 32_768, 8_192, 1);
        insert_resident(&mut idx, 6, 40_960, 8_192, 1);
        insert_resident(&mut idx, 7, 49_152, 16_384, 2);
        idx.bump_cursor = 65_536;

        let plan = idx.plan_alloc(16_384, 2);
        assert_eq!(
            plan,
            SlabAllocPlan::InPlace { offset: 32_768 },
            "LRU must continue evicting until a real contiguous 16_384-vertex span exists rather than growing the slab"
        );
        assert_eq!(idx.capacity_vertices, 65_536);
        assert!(idx.resident.contains_key(&tile_key(2)));
        assert!(idx.resident.contains_key(&tile_key(4)));
        assert!(idx.resident.contains_key(&tile_key(7)));
    }

    #[test]
    fn test_slab_compacts_fragmented_live_tiles_without_exceeding_max_vertices() {
        // Hard ceiling == 65_536 vertices (simulating device max_storage_buffer_binding_size).
        let mut idx = SlabRegionIndex::new(65_536, 65_536);
        // Alternating live (f=5) and stale (f=1) 8192-vertex tiles:
        // Total live = 4 * 8192 = 32_768; total stale = 4 * 8192 = 32_768.
        // After evicting all 4 stale tiles, every hole is only 8192 vertices wide!
        // Requesting 20_000 vertices cannot fit in any single hole, yet 32_768 + 20_000 <= 65_536.
        for i in 0..8u32 {
            let frame = if i % 2 == 0 { 5 } else { 1 };
            insert_resident(&mut idx, i, i * 8_192, 8_192, frame);
        }
        idx.bump_cursor = 65_536;

        let plan = idx.plan_alloc(20_000, 5);
        match plan {
            SlabAllocPlan::Rebuild {
                new_capacity,
                copies,
                offset,
            } => {
                assert_eq!(new_capacity, 65_536, "must compact within hard max_vertices ceiling");
                assert_eq!(offset, 32_768);
                assert_eq!(
                    copies,
                    vec![
                        (0, 0, 8_192),
                        (16_384, 8_192, 8_192),
                        (32_768, 16_384, 8_192),
                        (49_152, 24_576, 8_192),
                    ]
                );
            }
            other => panic!("expected compacted Rebuild, got {other:?}"),
        }
        assert_eq!(idx.resident.get(&tile_key(0)).unwrap().offset, 0);
        assert_eq!(idx.resident.get(&tile_key(2)).unwrap().offset, 8_192);
        assert_eq!(idx.resident.get(&tile_key(4)).unwrap().offset, 16_384);
        assert_eq!(idx.resident.get(&tile_key(6)).unwrap().offset, 24_576);
        assert_eq!(idx.bump_cursor, 52_768);
        assert!(idx.free_blocks.is_empty());
    }

    #[test]
    fn test_slab_growth_never_exceeds_storage_binding_limit_and_returns_exhausted_when_full() {
        // Simulates default 128 MiB storage binding limit (4_194_304 vertices * 32 B).
        let max_v = (134_217_728u64 / std::mem::size_of::<RteTileVertex>() as u64) as u32;
        let mut idx = SlabRegionIndex::new(262_144, max_v);
        // Simulate slab already grown to 3_000_000 live vertices on frame 10.
        idx.capacity_vertices = 3_000_000;
        insert_resident(&mut idx, 1, 0, 3_000_000, 10);
        idx.bump_cursor = 3_000_000;

        // Doubling 3_000_000 -> 6_000_000 would exceed max_v (4_194_304); must clamp strictly to max_v!
        let plan = idx.plan_alloc(500_000, 10);
        match plan {
            SlabAllocPlan::Rebuild {
                new_capacity,
                offset,
                ..
            } => {
                assert_eq!(new_capacity, max_v);
                assert!(
                    (new_capacity as u64) * (std::mem::size_of::<RteTileVertex>() as u64)
                        <= 134_217_728,
                    "grown slab byte size must never exceed max_storage_buffer_binding_size"
                );
                assert_eq!(offset, 3_000_000);
            }
            other => panic!("expected capped Rebuild, got {other:?}"),
        }
        insert_resident(&mut idx, 2, 3_000_000, 500_000, 10);

        // Requesting another 1_000_000 vertices on the same frame exceeds max_v:
        // must return Exhausted cleanly instead of overflowing the storage binding limit.
        assert_eq!(idx.plan_alloc(1_000_000, 10), SlabAllocPlan::Exhausted);
    }
}

