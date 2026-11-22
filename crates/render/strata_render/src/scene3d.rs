//! Unified 3D Engine: High-level Three.js ergonomics paired seamlessly with low-level Vulkan/OpenGL hardware pipelines.

#![allow(unused_variables)]

use crate::overlay3d::Overlay3D;
use std::collections::BTreeMap;
use strata_style::Color;

pub use crate::texture3d::{
    SamplerDesc, TextureData, TextureDesc, TextureId, TextureStore, UvTransform,
};

/// 3D Spatial Transformation
#[derive(Debug, Clone, PartialEq)]
pub struct Transform3D {
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
}

impl Default for Transform3D {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0],
            scale: [1.0, 1.0, 1.0],
        }
    }
}

impl Transform3D {
    pub const fn from_position(position: [f32; 3]) -> Self {
        Self {
            position,
            rotation: [0.0, 0.0, 0.0],
            scale: [1.0, 1.0, 1.0],
        }
    }
}

/// 3D Camera Abstractions
#[derive(Debug, Clone, PartialEq)]
pub enum Camera3D {
    Perspective {
        /// Horizontal field of view in degrees. The projection matrix derives
        /// the vertical field of view from the viewport's aspect ratio.
        fov_horizontal_deg: f32,
        near: f32,
        far: f32,
        position: [f32; 3],
        target: [f32; 3],
    },
    Orthographic {
        left: f32,
        right: f32,
        top: f32,
        bottom: f32,
        near: f32,
        far: f32,
        position: [f32; 3],
        target: [f32; 3],
    },
}

impl Default for Camera3D {
    fn default() -> Self {
        Self::Perspective {
            fov_horizontal_deg: 60.0,
            near: 0.1,
            far: 1000.0,
            position: [0.0, 2.0, 5.0],
            target: [0.0, 0.0, 0.0],
        }
    }
}

/// 3D Lighting
#[derive(Debug, Clone, PartialEq)]
pub enum Light3D {
    Ambient {
        color: Color,
        intensity: f32,
    },
    Directional {
        position: [f32; 3],
        color: Color,
        intensity: f32,
        cast_shadow: bool,
    },
    Point {
        position: [f32; 3],
        color: Color,
        intensity: f32,
        distance: f32,
    },
}

/// 3D Geometry: Supports both parametric Three.js-style primitives and low-level custom vertex buffers.
#[derive(Debug, Clone, PartialEq)]
pub enum Geometry3D {
    Box {
        width: f32,
        height: f32,
        depth: f32,
    },
    Sphere {
        radius: f32,
        width_segments: u32,
        height_segments: u32,
    },
    Plane {
        width: f32,
        height: f32,
    },
    Cylinder {
        radius_top: f32,
        radius_bottom: f32,
        height: f32,
        radial_segments: u32,
    },
    Custom {
        vertices: Vec<f32>,
        indices: Vec<u32>,
    },
    /// Custom vertex buffer with explicit texture coordinates.
    ///
    /// `vertices` is an eight-float stride: `x, y, z, nx, ny, nz, u, v`. Use this
    /// for tile quads, relief walls, and building prisms whose UVs are authored
    /// per corner rather than derived from the primitive generator.
    CustomTextured {
        vertices: Vec<f32>,
        indices: Vec<u32>,
    },
}

/// Textured surface material.
#[derive(Debug, Clone, PartialEq)]
pub struct TexturedMaterial3D {
    /// Texture sampled per fragment.
    pub texture: TextureId,
    /// Multiplied with the sampled RGBA before lighting.
    pub tint: Color,
    /// Sub-rect of the texture addressed by the mesh UVs.
    pub uv_transform: UvTransform,
    /// Whether the surface receives scene lighting. Tiles are usually unlit
    /// because the imagery already carries shading.
    pub lit: bool,
    /// How much of the sampled alpha to honour, 0..=1.
    pub opacity: f32,
}

/// One index range of a batched textured mesh, with its own material.
#[derive(Debug, Clone, PartialEq)]
pub struct TexturedDraw3D {
    pub material: TexturedMaterial3D,
    /// First index in the shared index buffer.
    pub index_start: u32,
    /// Number of indices, a multiple of three.
    pub index_count: u32,
}

impl TexturedDraw3D {
    pub fn new(material: TexturedMaterial3D, index_start: u32, index_count: u32) -> Self {
        Self {
            material,
            index_start,
            index_count,
        }
    }
}

/// A textured mesh with several texture draws sharing one vertex and index
/// buffer.
///
/// Terrain patches each need their own image, but setting up one mesh per patch
/// repeats the vertex transform for every patch. A batch transforms the shared
/// buffer once and switches textures per index range.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TexturedBatchMaterial3D {
    pub draws: Vec<TexturedDraw3D>,
}

impl TexturedBatchMaterial3D {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, draw: TexturedDraw3D) -> &mut Self {
        self.draws.push(draw);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.draws.is_empty()
    }
}

impl TexturedMaterial3D {
    pub fn new(texture: TextureId) -> Self {
        Self {
            texture,
            tint: Color::rgba(255, 255, 255, 255),
            uv_transform: UvTransform::IDENTITY,
            lit: false,
            opacity: 1.0,
        }
    }

    pub fn with_uv_transform(mut self, transform: UvTransform) -> Self {
        self.uv_transform = transform;
        self
    }

    pub fn with_tint(mut self, tint: Color) -> Self {
        self.tint = tint;
        self
    }
}

/// Low-Level Hardware Shader Source
#[derive(Debug, Clone, PartialEq)]
pub enum ShaderSource {
    SpirV(Vec<u32>),
    Glsl(String),
    Wgsl(String),
    Msl(String),
}

/// 3D Material: Unifies high-level PBR workflows with low-level custom hardware shaders.
#[derive(Debug, Clone, PartialEq)]
pub enum Material3D {
    /// High-Level Physically Based Rendering (PBR) Metallic-Roughness workflow
    Standard {
        color: Color,
        roughness: f32,
        metalness: f32,
        emissive: Option<Color>,
    },
    /// High-Level PBR with Clearcoat extensions
    Physical {
        color: Color,
        roughness: f32,
        metalness: f32,
        clearcoat: f32,
        transmission: f32,
    },
    /// High-Level Unlit flat material
    Basic { color: Color },
    /// Low-Level Custom Hardware Shader Material
    Shader {
        vertex_shader: Option<ShaderSource>,
        fragment_shader: Option<ShaderSource>,
        glsl_fragment: Option<String>,
        uniforms: BTreeMap<String, f32>,
    },
    /// Textured surface sampled from a [`TextureId`].
    Textured(TexturedMaterial3D),
    /// Several textured index ranges sharing one vertex buffer.
    TexturedBatch(TexturedBatchMaterial3D),
}

impl Default for Material3D {
    fn default() -> Self {
        Self::Standard {
            color: Color::rgba(255, 255, 255, 255),
            roughness: 0.5,
            metalness: 0.0,
            emissive: None,
        }
    }
}

/// A 3D Mesh in the Scene Graph: Seamlessly pairs high-level or low-level geometry with materials.
#[derive(Debug, Clone, PartialEq)]
pub struct Mesh3D {
    pub geometry: Geometry3D,
    pub material: Material3D,
    pub transform: Transform3D,
}

impl Mesh3D {
    pub fn new(geometry: Geometry3D, material: Material3D) -> Self {
        Self {
            geometry,
            material,
            transform: Transform3D::default(),
        }
    }

    pub fn with_transform(mut self, transform: Transform3D) -> Self {
        self.transform = transform;
        self
    }
}

/// What the rasterizer fills before any geometry is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Background3D {
    /// Leave the existing framebuffer contents untouched.
    #[default]
    Transparent,
    /// A single colour.
    Solid(Color),
    /// A vertical gradient from `top` to `bottom` over the viewport.
    VerticalGradient { top: Color, bottom: Color },
}

impl Background3D {
    /// Returns the colour at a normalized vertical position `t` (0 = top).
    pub fn color_at(self, t: f32) -> Option<Color> {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Transparent => None,
            Self::Solid(color) => Some(color),
            Self::VerticalGradient { top, bottom } => {
                let mix = |a: f32, b: f32| -> u8 {
                    ((a * (1.0 - t) + b * t) * 255.0).round().clamp(0.0, 255.0) as u8
                };
                Some(Color::rgba(
                    mix(top.r(), bottom.r()),
                    mix(top.g(), bottom.g()),
                    mix(top.b(), bottom.b()),
                    mix(top.a(), bottom.a()),
                ))
            }
        }
    }
}

/// An emissive celestial body drawn as a billboard: the sun, the moon, or a
/// sensor anomaly marker.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CelestialBody3D {
    /// Scene-space position of the body centre.
    pub position: [f64; 3],
    /// Apparent radius in scene units.
    pub radius: f64,
    pub color: Color,
    /// Soft edge width in pixels; 0 draws a hard disc.
    pub glow: f32,
    /// Whether the body participates in depth testing. The sun and stars
    /// usually do not.
    pub depth_test: bool,
}

/// A translucent spherical shell drawn over the surface: clouds, a night arc,
/// or a coverage wash.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CloudShell3D {
    /// Scene-space centre of the shell.
    pub centre: [f64; 3],
    /// Radius of the shell in scene units.
    pub radius: f64,
    pub color: Color,
    /// Fraction of the shell that is cloud, 0..=1.
    pub coverage: f32,
    /// Shell opacity at full coverage, 0..=1.
    pub opacity: f32,
}

impl CloudShell3D {
    /// Opacity after applying the coverage ramp.
    ///
    /// The reference ramps cloud opacity from clear to full as density rises;
    /// clamping here keeps every consumer on the same curve.
    pub fn effective_opacity(&self) -> f32 {
        (self.coverage.clamp(0.0, 1.0) * self.opacity.clamp(0.0, 1.0)).clamp(0.0, 1.0)
    }
}

/// Compact, backend-agnostic identifier for a terrain tile in a quadtree or flat-patch pyramid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TileKeyId {
    pub level: u8,
    pub col: u32,
    pub row: u32,
    pub face: u8,
}

impl TileKeyId {
    #[inline]
    pub const fn new(level: u8, col: u32, row: u32, face: u8) -> Self {
        Self {
            level,
            col,
            row,
            face,
        }
    }

    #[inline]
    pub const fn quadtree(level: u8, col: u32, row: u32) -> Self {
        Self::new(level, col, row, 0)
    }
}

/// Tile-local RTC terrain vertex packed into two `vec4<f32>`s for GPU vertex pulling.
///
/// `pos_u = [local_x, local_y, local_z, u]` relative to the tile's `center_ecef`,
/// `norm_v = [nx, ny, nz, v]`. Using two 16-byte vectors eliminates WGSL `vec3` alignment padding.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "wgpu-backend", derive(bytemuck::Pod, bytemuck::Zeroable))]
pub struct RteTileVertex {
    pub pos_u: [f32; 4],
    pub norm_v: [f32; 4],
}

impl RteTileVertex {
    #[inline]
    pub const fn new(pos: [f32; 3], normal: [f32; 3], uv: [f32; 2]) -> Self {
        Self {
            pos_u: [pos[0], pos[1], pos[2], uv[0]],
            norm_v: [normal[0], normal[1], normal[2], uv[1]],
        }
    }

    #[inline]
    pub const fn position(self) -> [f32; 3] {
        [self.pos_u[0], self.pos_u[1], self.pos_u[2]]
    }

    #[inline]
    pub const fn normal(self) -> [f32; 3] {
        [self.norm_v[0], self.norm_v[1], self.norm_v[2]]
    }

    #[inline]
    pub const fn uv(self) -> [f32; 2] {
        [self.pos_u[3], self.norm_v[3]]
    }
}

/// Persistent GPU-ready tile geometry computed once per `(TileKeyId, revision)`.
#[derive(Debug, Clone, PartialEq)]
pub struct RetainedTileMesh {
    pub key: TileKeyId,
    pub revision: u64,
    pub center_ecef: [f64; 3],
    pub grid_res: u32,
    pub has_skirts: bool,
    pub vertices: std::sync::Arc<Vec<RteTileVertex>>,
    pub bounding_radius: f32,
}

/// Persistent GPU-ready tile imagery payload computed once per `(id, revision)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetainedTileTexture {
    pub id: u64,
    pub revision: u64,
    pub width: u32,
    pub height: u32,
    pub rgba8: std::sync::Arc<Vec<u8>>,
}

/// Lightweight per-frame tile draw instance referencing cached geometry and imagery.
#[derive(Debug, Clone, PartialEq)]
pub struct RetainedTileInstance {
    pub key: TileKeyId,
    pub mesh_revision: u64,
    pub center_ecef: [f64; 3],
    pub texture_id: Option<u64>,
    pub texture_revision: u64,
    pub uv_rect: [f32; 4],
    pub tint: [f32; 4],
    pub unlit: bool,
    pub night_texture_id: Option<u64>,
    pub night_texture_revision: u64,
    pub night_uv_rect: [f32; 4],
}

impl Default for RetainedTileInstance {
    fn default() -> Self {
        Self {
            key: TileKeyId::quadtree(0, 0, 0),
            mesh_revision: 0,
            center_ecef: [0.0; 3],
            texture_id: None,
            texture_revision: 0,
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            tint: [1.0, 1.0, 1.0, 1.0],
            unlit: false,
            night_texture_id: None,
            night_texture_revision: 0,
            night_uv_rect: [0.0, 0.0, 1.0, 1.0],
        }
    }
}

/// Dynamic or non-grid RTC geometry (buildings, vector ribbons, trajectories) anchored in ECEF.
#[derive(Debug, Clone, PartialEq)]
pub struct RetainedDynamicMesh {
    pub id: u64,
    pub revision: u64,
    pub anchor_ecef: [f64; 3],
    pub vertices: std::sync::Arc<Vec<RteTileVertex>>,
    pub indices: std::sync::Arc<Vec<u32>>,
    pub color: [f32; 4],
    pub unlit: bool,
}

/// Canonical local-space 3D mesh asset uploaded once to GPU VRAM and shared across any number of
/// [`RetainedGeoInstance`]s.
///
/// Vertex coordinates are authored in local model metres (`+X` East, `+Y` North, `+Z` Up).
#[derive(Debug, Clone, PartialEq)]
pub struct RetainedMeshAsset {
    pub id: u64,
    pub revision: u64,
    pub vertices: std::sync::Arc<Vec<RteTileVertex>>,
    pub indices: std::sync::Arc<Vec<u32>>,
    /// Optional per-vertex RGBA colors (`len == vertices.len()`); `None` defaults to white `[1,1,1,1]`.
    pub vertex_colors: Option<std::sync::Arc<Vec<[f32; 4]>>>,
    /// Optional default base-color albedo texture bound when an instance does not specify its own.
    pub albedo_texture: Option<std::sync::Arc<RetainedTileTexture>>,
}

/// Hardware-instanced 3D object anchored in double-precision ECEF / morphed planetary space.
///
/// `model_cols` is a column-major 3x3 linear transform mapping local model coordinates (`+X`, `+Y`, `+Z`)
/// to world relative metres (combining scale, local heading/pitch/roll, local ENU basis, and 2D/2.5D
/// surface morphing).
#[derive(Debug, Clone, PartialEq)]
pub struct RetainedGeoInstance {
    pub mesh_id: u64,
    pub anchor_ecef: [f64; 3],
    pub model_cols: [[f32; 3]; 3],
    pub base_color: [f32; 4],
    pub emissive_rgb: [f32; 3],
    pub roughness: f32,
    pub metalness: f32,
    pub unlit: bool,
    /// Optional white-core accent mix (`0.0` default; multiplied by vertex `uv.y` in shader).
    pub accent_mix: f32,
    /// Optional per-instance albedo RGBA8 texture overriding [`RetainedMeshAsset::albedo_texture`].
    pub albedo_texture: Option<std::sync::Arc<RetainedTileTexture>>,
}

impl Default for RetainedGeoInstance {
    fn default() -> Self {
        Self {
            mesh_id: 0,
            anchor_ecef: [0.0; 3],
            model_cols: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            base_color: [1.0, 1.0, 1.0, 1.0],
            emissive_rgb: [0.0, 0.0, 0.0],
            roughness: 0.45,
            metalness: 0.1,
            unlit: false,
            accent_mix: 0.0,
            albedo_texture: None,
        }
    }
}

/// 2D sprite rendering mode for a GPU-instanced [`RetainedParticleLayer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RetainedParticleSprite {
    /// Sample a custom 2D RGBA8 texture identified by `texture_id` inside
    /// [`RetainedTerrainFrame::particle_textures`].
    Textured(u64),
    /// Pure mathematical 2D motion-tapered raindrop streak (zero texture fetches).
    #[default]
    ProceduralStreak,
    /// Pure mathematical simplistic 6-fold geometric snowflake crystal (zero texture fetches).
    ProceduralSnowflake,
    /// Pure mathematical soft translucent sleet/graupel ice pellet (zero texture fetches).
    ProceduralPellet,
    /// Pure mathematical wide-area falling rain/snow virga curtain sheet for far-field depth illusion.
    ProceduralCurtain,
}

/// Billboard alignment strategy for a GPU-instanced [`RetainedParticleLayer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RetainedParticleBillboard {
    /// Aligns the 2D quad's vertical axis with the particle's 3D apparent velocity vector
    /// and horizontal axis perpendicular to both velocity and camera view ray.
    #[default]
    VelocityAligned,
    /// Camera-facing 2D sprite aligned with camera right/up vectors with per-particle 2D spin.
    CameraFacing,
}

/// GPU-instanced camera-local toroidal 2D billboard particle volume.
///
/// All `particle_count` particles are procedurally distributed, toroidally wrapped in
/// world-locked East-North-Up (ENU) space around the camera eye (optionally shifted forward
/// along the camera view cone via `forward_bias_world`), depth-tested against terrain and
/// 3D objects, and billboarded entirely on the GPU in one draw call.
#[derive(Debug, Clone, PartialEq)]
pub struct RetainedParticleLayer {
    pub id: u64,
    pub particle_count: u32,
    pub sprite: RetainedParticleSprite,
    pub billboard: RetainedParticleBillboard,
    /// Local orthonormal unit East vector in world / ECEF coordinates at the camera eye.
    pub basis_east: [f32; 3],
    /// Local orthonormal unit North vector in world / ECEF coordinates at the camera eye.
    pub basis_north: [f32; 3],
    /// Local orthonormal unit Up vector in world / ECEF coordinates at the camera eye.
    pub basis_up: [f32; 3],
    /// Optional world-space offset shifting the toroidal wrap box forward into the camera view cone.
    pub forward_bias_world: [f32; 3],
    /// Pre-wrapped `f64 -> f32` fractional toroidal offsets in `[0.0, 1.0)` encoding
    /// `(wind_enu * time_s - camera_world_phase_enu) / volume_extent_enu`.
    pub wrapped_phase: [f32; 3],
    /// `[eye_alt_msl_m, ground_alt_msl_m, ceiling_alt_msl_m, ceiling_fade_band_m]`.
    pub altitude_bounds: [f32; 4],
    /// `[extent_xy_m, extent_z_m, box_edge_fade_frac, near_lens_fade_m]`.
    pub volume_params: [f32; 4],
    /// `[fall_speed_mps, wind_east_mps, wind_north_mps, wrapped_time_s]`.
    pub motion_params: [f32; 4],
    /// `[width_m, height_m, velocity_stretch, spin_rad_s]`.
    pub size_params: [f32; 4],
    /// `[flutter_amp_m, flutter_freq_hz, min_px_size, max_px_size]`.
    pub flutter_clamp: [f32; 4],
    /// Multiplicative RGBA tint (alpha modulates global opacity & cloud envelope).
    pub color: [f32; 4],
    /// Deterministic per-layer hash seed offset.
    pub seed: u32,
}

impl Default for RetainedParticleLayer {
    fn default() -> Self {
        Self {
            id: 0,
            particle_count: 0,
            sprite: RetainedParticleSprite::ProceduralStreak,
            billboard: RetainedParticleBillboard::VelocityAligned,
            basis_east: [1.0, 0.0, 0.0],
            basis_north: [0.0, 1.0, 0.0],
            basis_up: [0.0, 0.0, 1.0],
            forward_bias_world: [0.0, 0.0, 0.0],
            wrapped_phase: [0.0, 0.0, 0.0],
            altitude_bounds: [100.0, 0.0, 5_500.0, 900.0],
            volume_params: [48.0, 36.0, 0.22, 1.4],
            motion_params: [26.0, 3.0, 1.0, 0.0],
            size_params: [0.045, 0.95, 0.032, 0.0],
            flutter_clamp: [0.0, 0.0, 1.4, 52.0],
            color: [0.80, 0.89, 1.0, 0.72],
            seed: 1,
        }
    }
}

/// One directional wind-driven ocean wave train for smooth dual-state wind cross-fading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RetainedOceanWaveTrain {
    /// Normalized `[east, north]` wave propagation direction.
    pub dir_en: [f32; 2],
    /// Wind speed in m/s governing wave height, wavelength dispersion, and whitecaps.
    pub wind_speed_mps: f32,
    /// Continuous wrapped phase clock in seconds for this wave train.
    pub phase_time_s: f32,
    /// Cross-fade blend weight in `[0.0..=1.0]`.
    pub weight: f32,
}

impl Default for RetainedOceanWaveTrain {
    fn default() -> Self {
        Self {
            dir_en: [0.866, 0.5],
            wind_speed_mps: 8.0,
            phase_time_s: 0.0,
            weight: 1.0,
        }
    }
}

/// GPU-native per-pixel ocean wave spectrum & optional coastal shoreline shoaling descriptor.
///
/// Shaded directly in the terrain water pass with zero floating CPU polygons, Schlick Fresnel
/// reflections of the physical 2.5D sky, GGX solar/lunar glitter, and optional coastal SDF surf.
#[derive(Debug, Clone, PartialEq)]
pub struct RetainedOceanSurface {
    pub enabled: bool,
    /// RTE anchor origin in ECEF / morphed world metres (quantized near camera focus).
    pub origin_ecef: [f64; 3],
    /// Local orthonormal unit East vector at `origin_ecef`.
    pub basis_east: [f32; 3],
    /// Local orthonormal unit North vector at `origin_ecef`.
    pub basis_north: [f32; 3],
    /// Local orthonormal unit Up vector at `origin_ecef`.
    pub basis_up: [f32; 3],
    /// Primary (outgoing or steady-state) directional wave train.
    pub train_a: RetainedOceanWaveTrain,
    /// Secondary (incoming during wind direction/speed transition) wave train.
    pub train_b: RetainedOceanWaveTrain,
    /// Overall wave slope / height strength multiplier (`1.0` default).
    pub wave_strength: f32,
    /// Spatial frequency multiplier (`1.0` default).
    pub wave_scale: f32,
    /// Solar/lunar specular glitter intensity multiplier (`1.0` default).
    pub glint_intensity: f32,
    /// Surface shading blend opacity over underlying water imagery (`0.82` default).
    pub opacity: f32,
    /// Altitude visibility fade in `[0.0..=1.0]`.
    pub alt_fade: f32,
    /// Enables optional coastal shoreline refraction, breaking surf lines, and periodic swash.
    pub coastal_shoaling: bool,
    /// Distance in metres from shoreline over which shoaling & periodic surf occur (`360.0` default).
    pub shoaling_range_m: f32,
    /// World half-span in metres covered by `coastal_sdf` around `sdf_center_en_m`.
    pub sdf_half_span_m: f32,
    /// Offset `[east_m, north_m]` of the coastal SDF texture center relative to `origin_ecef`.
    pub sdf_center_en_m: [f32; 2],
    /// Optional `256x256` RGBA8 shoreline SDF texture (`R=water_mask, G=shore_dist_norm, BA=shore_dir_en`).
    pub coastal_sdf: Option<std::sync::Arc<RetainedTileTexture>>,
}

impl Default for RetainedOceanSurface {
    fn default() -> Self {
        Self {
            enabled: false,
            origin_ecef: [0.0; 3],
            basis_east: [1.0, 0.0, 0.0],
            basis_north: [0.0, 1.0, 0.0],
            basis_up: [0.0, 0.0, 1.0],
            train_a: RetainedOceanWaveTrain::default(),
            train_b: RetainedOceanWaveTrain {
                weight: 0.0,
                ..RetainedOceanWaveTrain::default()
            },
            wave_strength: 1.0,
            wave_scale: 1.0,
            glint_intensity: 1.0,
            opacity: 0.82,
            alt_fade: 1.0,
            coastal_shoaling: false,
            shoaling_range_m: 360.0,
            sdf_half_span_m: 1_500.0,
            sdf_center_en_m: [0.0, 0.0],
            coastal_sdf: None,
        }
    }
}

/// One real catalog star in J2000 ICRF celestial coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RetainedStar {
    /// Unit direction vector in J2000 ICRF equatorial coordinates.
    pub icrf_dir: [f32; 3],
    /// Apparent visual magnitude (Vmag; lower = brighter).
    pub vmag: f32,
    /// Linear sRGB Planck blackbody color `(r, g, b)` + `1.0` alpha.
    pub color: [f32; 4],
}

/// 6-face RGBA8 cubemap texture uploadable to `wgpu` (`TextureViewDimension::Cube`).
#[derive(Debug, Clone, PartialEq)]
pub struct RetainedCubemapTexture {
    pub id: u64,
    pub revision: u64,
    /// Width and height of each square cubemap face in texels.
    pub face_size: u32,
    /// 6 faces in WebGPU/Vulkan array-layer order: `[+X, -X, +Y, -Y, +Z, -Z]`.
    pub faces_rgba8: [std::sync::Arc<Vec<u8>>; 6],
}

/// Persistent GPU-uploadable planetary & space environment assets (land mask, cloud map, Moon, star catalog, space skybox).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RetainedEnvironmentAssets {
    pub revision: u64,
    /// Equirectangular land mask (land=255, ocean=0 in red/alpha channels) for deterministic land-only city lights.
    pub land_mask: Option<std::sync::Arc<RetainedTileTexture>>,
    /// Global 3D equirectangular cloud map texture (alpha or luminance channel).
    pub cloud_map: Option<std::sync::Arc<RetainedTileTexture>>,
    /// Equirectangular Moon surface albedo texture (NASA LRO).
    pub moon_texture: Option<std::sync::Arc<RetainedTileTexture>>,
    /// Real bright star catalog (e.g. Yale BSC5 in J2000 ICRF).
    pub stars: Option<std::sync::Arc<Vec<RetainedStar>>>,
    /// J2000 ICRF telescope space skybox cubemap (`[+X, -X, +Y, -Y, +Z, -Z]`).
    pub skybox_cubemap: Option<std::sync::Arc<RetainedCubemapTexture>>,
}

/// Camera and lighting uniforms needed for O(1) RTE GPU frame presentation without scene rebuilds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RetainedCameraUniforms {
    /// Column-major projection matrix.
    pub proj: [[f32; 4]; 4],
    /// Column-major camera rotation matrix with camera positioned at world origin `[0, 0, 0]`.
    pub view_rot: [[f32; 4]; 4],
    /// Pre-multiplied column-major `proj * view_rot` matrix.
    pub view_proj_rte: [[f32; 4]; 4],
    /// High-precision eye position in world / ECEF coordinates.
    pub eye_ecef: [f64; 3],
    /// World-space directional sun direction (pointing from surface toward light).
    pub sun_dir: [f32; 3],
    pub sun_color: [f32; 3],
    pub ambient_color: [f32; 3],
    pub background: Background3D,
    pub near: f32,
    pub far: f32,
    /// Column-major J2000 ICRF -> ECEF rotation matrix (GMST sidereal rotation).
    pub ecef_from_icrf: [[f32; 4]; 4],
    /// Normalized ECEF direction toward the Moon.
    pub moon_dir: [f32; 3],
    /// Physical day/night terminator strength (`0.0` = flat daylight, `1.0` = full terminator, fades to 0 in 2.5D).
    pub day_night_strength: f32,
    /// Deterministic land-masked night city-lights intensity (`0.0` = off, auto-fades out in 2.5D).
    pub city_lights_intensity: f32,
    /// Seamless 3D -> 2.5D ground morph factor (`0.0` = 3D space globe, `1.0` = 2.5D local ground).
    pub local_25d_blend: f32,
    /// Render physical limb-darkened Sun disc & corona (`true` by default).
    pub sun_visible: bool,
    /// Visual Sun angular radius in radians (default ~`0.018` rad).
    pub sun_angular_radius_rad: f32,
    /// Render NASA textured Moon with real orbital lunar phase (`true` by default).
    pub moon_visible: bool,
    /// Visual Moon angular radius in radians (default ~`0.022` rad).
    pub moon_angular_radius_rad: f32,
    /// Render scientific deep-space starfield skybox (`true` by default).
    pub stars_visible: bool,
    /// Brightness multiplier for starfield & Milky Way band.
    pub stars_intensity: f32,
    /// Brightness multiplier for real telescope skybox cubemap (`1.0` default).
    pub skybox_intensity: f32,
    /// Opacity of global 3D cloud-map shell (`0.0` = off; auto-fades out in 2.5D).
    pub cloud_layer_opacity: f32,
    /// Opacity of local cropped satellite overhead cloud canopy in low-altitude / 2.5D view.
    pub local_cloud_opacity: f32,
    /// Global cloud-map longitudinal scroll offset in radians.
    pub cloud_lon_offset_rad: f32,
    /// Simulation / animation time in seconds.
    pub time_s: f32,
    /// Opt-in physical Rayleigh/Mie 2.5D sky scattering weight (`0.0` = classic flat blue 2.5D sky, `1.0` = full physical sky).
    pub realistic_25d_sky: f32,
    /// Opt-in 2.5D night-sky star & Milky Way visibility sync (`0.0` = off, `1.0` = stars appear at night above horizon).
    pub night_stars_25d: f32,
    /// Opt-in physical 2.5D directional sun & DEM relief hillshading weight (`0.0` = flat 2.5D terrain, `1.0` = lit).
    pub terrain_25d_lighting: f32,
    /// Opt-in hardware directional shadow-map pass & 9-tap PCF mountain/object shadow strength (`0.0` = disabled).
    pub terrain_25d_shadows: f32,
    /// Directional shadow map resolution in pixels (`2048` default; lazily allocated only when shadows > 0).
    pub shadow_map_size: u32,
    /// Unified weather optical storm/overcast darkness (`0.0` = clear sky, `1.0` = brooding storm overcast).
    pub weather_darkness: f32,
    /// Unified weather cloud cover fraction (`0.0..=1.0`, attenuating sun glare & night stars).
    pub weather_cloud_cover: f32,
    /// Unified weather / precipitation atmospheric visibility haze (`0.0..=1.0`).
    pub weather_haze: f32,
    /// Weather-modulated cloud RGB tint multiplier (`[1.0, 1.0, 1.0]` neutral).
    pub cloud_tint_rgb: [f32; 3],
    /// Weather-driven overhead 2.5D cloud coverage boost (`0.0..=1.0`).
    pub cloud_25d_cover_boost: f32,
    /// Flat 2D map morph factor (`0.0` = 3D/2.5D perspective scene, `1.0` = flat 2D map imagery).
    pub map_2d_morph: f32,
}

impl Default for RetainedCameraUniforms {
    fn default() -> Self {
        let ident = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        Self {
            proj: ident,
            view_rot: ident,
            view_proj_rte: ident,
            eye_ecef: [0.0, 0.0, 5.0],
            sun_dir: [0.577_350_26, 0.577_350_26, 0.577_350_26],
            sun_color: [1.0, 0.98, 0.95],
            ambient_color: [0.22, 0.24, 0.28],
            background: Background3D::Transparent,
            near: 0.1,
            far: 100_000_000.0,
            ecef_from_icrf: ident,
            moon_dir: [-0.577_350_26, 0.577_350_26, 0.577_350_26],
            day_night_strength: 1.0,
            city_lights_intensity: 0.0,
            local_25d_blend: 0.0,
            sun_visible: false,
            sun_angular_radius_rad: 0.018,
            moon_visible: false,
            moon_angular_radius_rad: 0.022,
            stars_visible: false,
            stars_intensity: 1.0,
            skybox_intensity: 1.0,
            cloud_layer_opacity: 0.0,
            local_cloud_opacity: 0.0,
            cloud_lon_offset_rad: 0.0,
            time_s: 0.0,
            realistic_25d_sky: 0.0,
            night_stars_25d: 0.0,
            terrain_25d_lighting: 0.0,
            terrain_25d_shadows: 0.0,
            shadow_map_size: 2048,
            weather_darkness: 0.0,
            weather_cloud_cover: 0.0,
            weather_haze: 0.0,
            cloud_tint_rgb: [1.0, 1.0, 1.0],
            cloud_25d_cover_boost: 0.0,
            map_2d_morph: 0.0,
        }
    }
}

/// Snapshot of retained terrain tiles, textures, dynamic meshes, instanced geo objects,
/// GPU-instanced 2D camera-local particle volumes, GPU ocean surface, and camera parameters.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RetainedTerrainFrame {
    pub camera: RetainedCameraUniforms,
    pub geometry_revision: u64,
    /// Monotonic revision of `geo_mesh_assets` / `geo_instances`, used to prevent stale background
    /// tile worker snapshots from overwriting newer main-thread `EarthWorld` updates.
    pub geo_revision: u64,
    pub meshes: Vec<std::sync::Arc<RetainedTileMesh>>,
    pub instances: Vec<RetainedTileInstance>,
    pub textures: Vec<std::sync::Arc<RetainedTileTexture>>,
    pub dynamic_meshes: Vec<RetainedDynamicMesh>,
    pub geo_mesh_assets: std::sync::Arc<Vec<std::sync::Arc<RetainedMeshAsset>>>,
    pub geo_instances: std::sync::Arc<Vec<RetainedGeoInstance>>,
    pub particle_layers: std::sync::Arc<Vec<RetainedParticleLayer>>,
    pub particle_textures: std::sync::Arc<Vec<std::sync::Arc<RetainedTileTexture>>>,
    pub ocean: Option<std::sync::Arc<RetainedOceanSurface>>,
    pub environment: Option<std::sync::Arc<RetainedEnvironmentAssets>>,
}

#[derive(Debug)]
struct RetainedSceneState {
    camera: RetainedCameraUniforms,
    terrain: Option<std::sync::Arc<RetainedTerrainFrame>>,
    fallback_scene: Option<std::sync::Arc<Scene3D>>,
}

#[derive(Debug)]
struct RetainedScene3DInner {
    camera_generation: std::sync::atomic::AtomicU64,
    geometry_generation: std::sync::atomic::AtomicU64,
    overlay_generation: std::sync::atomic::AtomicU64,
    geo_revision: std::sync::atomic::AtomicU64,
    state: std::sync::RwLock<RetainedSceneState>,
}

/// Thread-safe retained 3D scene handle shared between host state (camera/worker) and `Canvas3D`.
///
/// Calling [`RetainedScene3D::update_camera`] updates camera uniforms in place and bumps only
/// `camera_generation`, allowing the GPU backend to re-render at uncapped framerates without
/// rebuilding DOM nodes, layout, display lists, or tile meshes.
#[derive(Clone)]
pub struct RetainedScene3D {
    inner: std::sync::Arc<RetainedScene3DInner>,
}

impl std::fmt::Debug for RetainedScene3D {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (cam, geom, overlay) = self.generations();
        f.debug_struct("RetainedScene3D")
            .field("camera_generation", &cam)
            .field("geometry_generation", &geom)
            .field("overlay_generation", &overlay)
            .finish()
    }
}

impl PartialEq for RetainedScene3D {
    fn eq(&self, other: &Self) -> bool {
        if std::sync::Arc::ptr_eq(&self.inner, &other.inner) {
            return true;
        }
        let Ok(a) = self.inner.state.read() else {
            return false;
        };
        let Ok(b) = other.inner.state.read() else {
            return false;
        };
        a.camera == b.camera && a.terrain == b.terrain && a.fallback_scene == b.fallback_scene
    }
}

impl RetainedScene3D {
    pub fn new(camera: RetainedCameraUniforms) -> Self {
        Self {
            inner: std::sync::Arc::new(RetainedScene3DInner {
                camera_generation: std::sync::atomic::AtomicU64::new(1),
                geometry_generation: std::sync::atomic::AtomicU64::new(0),
                overlay_generation: std::sync::atomic::AtomicU64::new(0),
                geo_revision: std::sync::atomic::AtomicU64::new(0),
                state: std::sync::RwLock::new(RetainedSceneState {
                    camera,
                    terrain: None,
                    fallback_scene: None,
                }),
            }),
        }
    }

    /// Monotonic revision of the latest published `geo_mesh_assets` / `geo_instances`.
    #[inline]
    pub fn geo_revision(&self) -> u64 {
        self.inner
            .geo_revision
            .load(std::sync::atomic::Ordering::Acquire)
    }

    pub fn update_camera(&self, camera: RetainedCameraUniforms) {
        if let Ok(mut guard) = self.inner.state.write() {
            guard.camera = camera;
        }
        self.inner
            .camera_generation
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }

    pub fn update_terrain(&self, terrain: std::sync::Arc<RetainedTerrainFrame>) {
        if let Ok(mut guard) = self.inner.state.write() {
            guard.camera = terrain.camera;
            let rev = terrain.geo_revision;
            guard.terrain = Some(terrain);
            self.inner
                .geo_revision
                .fetch_max(rev, std::sync::atomic::Ordering::AcqRel);
        }
        self.inner
            .geometry_generation
            .fetch_add(1, std::sync::atomic::Ordering::Release);
        self.inner
            .camera_generation
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }

    /// Publishes a new terrain tile frame from a background LOD worker while preserving the live camera,
    /// any main-thread camera-locked particle layers or GPU ocean state when omitted, and any newer
    /// main-thread `geo_mesh_assets`/`geo_instances` (preventing 1-frame rubber-banding).
    pub fn update_terrain_preserving_camera(&self, terrain: std::sync::Arc<RetainedTerrainFrame>) {
        if let Ok(mut guard) = self.inner.state.write() {
            if let Some(ref existing) = guard.terrain {
                let keep_particles =
                    terrain.particle_layers.is_empty() && !existing.particle_layers.is_empty();
                let keep_ocean = terrain.ocean.is_none() && existing.ocean.is_some();
                let keep_geo =
                    existing.geo_revision > 0 && terrain.geo_revision < existing.geo_revision;
                if keep_particles || keep_ocean || keep_geo {
                    let mut merged = (*terrain).clone();
                    if keep_particles {
                        merged.particle_layers = existing.particle_layers.clone();
                        merged.particle_textures = existing.particle_textures.clone();
                    }
                    if keep_ocean {
                        merged.ocean = existing.ocean.clone();
                    }
                    if keep_geo {
                        merged.geo_revision = existing.geo_revision;
                        merged.geo_mesh_assets = existing.geo_mesh_assets.clone();
                        merged.geo_instances = existing.geo_instances.clone();
                    }
                    let rev = merged.geo_revision;
                    guard.terrain = Some(std::sync::Arc::new(merged));
                    self.inner
                        .geo_revision
                        .fetch_max(rev, std::sync::atomic::Ordering::AcqRel);
                } else {
                    let rev = terrain.geo_revision;
                    guard.terrain = Some(terrain);
                    self.inner
                        .geo_revision
                        .fetch_max(rev, std::sync::atomic::Ordering::AcqRel);
                }
            } else {
                let rev = terrain.geo_revision;
                guard.terrain = Some(terrain);
                self.inner
                    .geo_revision
                    .fetch_max(rev, std::sync::atomic::Ordering::AcqRel);
            }
        }
        self.inner
            .geometry_generation
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }

    /// Atomically publishes a new terrain tile frame together with its matching camera uniforms
    /// while preserving main-thread camera-locked particle layers, GPU ocean state, and newer
    /// main-thread `geo_mesh_assets`/`geo_instances` when omitted or stale.
    ///
    /// Used on the single boundary frame switching between spherical/tangent 3D and the canonical
    /// 2D flat-map plane so terrain geometry and camera never mismatch even transiently.
    pub fn update_terrain_with_camera_preserving_overlays(
        &self,
        terrain: std::sync::Arc<RetainedTerrainFrame>,
    ) {
        if let Ok(mut guard) = self.inner.state.write() {
            guard.camera = terrain.camera;
            if let Some(ref existing) = guard.terrain {
                let keep_particles =
                    terrain.particle_layers.is_empty() && !existing.particle_layers.is_empty();
                let keep_ocean = terrain.ocean.is_none() && existing.ocean.is_some();
                let keep_geo =
                    existing.geo_revision > 0 && terrain.geo_revision < existing.geo_revision;
                if keep_particles || keep_ocean || keep_geo {
                    let mut merged = (*terrain).clone();
                    if keep_particles {
                        merged.particle_layers = existing.particle_layers.clone();
                        merged.particle_textures = existing.particle_textures.clone();
                    }
                    if keep_ocean {
                        merged.ocean = existing.ocean.clone();
                    }
                    if keep_geo {
                        merged.geo_revision = existing.geo_revision;
                        merged.geo_mesh_assets = existing.geo_mesh_assets.clone();
                        merged.geo_instances = existing.geo_instances.clone();
                    }
                    let rev = merged.geo_revision;
                    guard.terrain = Some(std::sync::Arc::new(merged));
                    self.inner
                        .geo_revision
                        .fetch_max(rev, std::sync::atomic::Ordering::AcqRel);
                } else {
                    let rev = terrain.geo_revision;
                    guard.terrain = Some(terrain);
                    self.inner
                        .geo_revision
                        .fetch_max(rev, std::sync::atomic::Ordering::AcqRel);
                }
            } else {
                let rev = terrain.geo_revision;
                guard.terrain = Some(terrain);
                self.inner
                    .geo_revision
                    .fetch_max(rev, std::sync::atomic::Ordering::AcqRel);
            }
        }
        self.inner
            .geometry_generation
            .fetch_add(1, std::sync::atomic::Ordering::Release);
        self.inner
            .camera_generation
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }

    /// Marks the retained 3D scene dirty for one more frame when per-frame GPU texture/mesh upload budgets deferred remaining work.
    pub fn request_gpu_continuation(&self) {
        self.inner
            .camera_generation
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }

    /// Updates camera uniforms and replaces only dynamic 3D overlays (e.g. orbital satellites)
    /// without rebuilding or re-allocating cached terrain tiles, textures, particle volumes, ocean, or environment assets.
    pub fn update_camera_and_dynamic_meshes(
        &self,
        camera: RetainedCameraUniforms,
        dynamic_meshes: Vec<RetainedDynamicMesh>,
    ) {
        if let Ok(mut guard) = self.inner.state.write() {
            guard.camera = camera;
            if let Some(ref existing) = guard.terrain {
                guard.terrain = Some(std::sync::Arc::new(RetainedTerrainFrame {
                    camera,
                    geometry_revision: existing.geometry_revision,
                    geo_revision: existing.geo_revision,
                    meshes: existing.meshes.clone(),
                    instances: existing.instances.clone(),
                    textures: existing.textures.clone(),
                    dynamic_meshes,
                    geo_mesh_assets: existing.geo_mesh_assets.clone(),
                    geo_instances: existing.geo_instances.clone(),
                    particle_layers: existing.particle_layers.clone(),
                    particle_textures: existing.particle_textures.clone(),
                    ocean: existing.ocean.clone(),
                    environment: existing.environment.clone(),
                }));
            }
        }
        self.inner
            .camera_generation
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }

    /// Updates hardware-instanced geodetic objects in O(1) TerrainFrame shallow-clone time without
    /// rebuilding terrain tiles, textures, particle volumes, ocean, or environment state.
    pub fn update_geo_objects(
        &self,
        geo_mesh_assets: std::sync::Arc<Vec<std::sync::Arc<RetainedMeshAsset>>>,
        geo_instances: std::sync::Arc<Vec<RetainedGeoInstance>>,
    ) {
        if let Ok(mut guard) = self.inner.state.write() {
            let camera = guard.camera;
            let next_geo_rev = self
                .inner
                .geo_revision
                .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
                .wrapping_add(1)
                .max(1);
            if let Some(ref existing) = guard.terrain {
                guard.terrain = Some(std::sync::Arc::new(RetainedTerrainFrame {
                    camera,
                    geometry_revision: existing.geometry_revision,
                    geo_revision: next_geo_rev,
                    meshes: existing.meshes.clone(),
                    instances: existing.instances.clone(),
                    textures: existing.textures.clone(),
                    dynamic_meshes: existing.dynamic_meshes.clone(),
                    geo_mesh_assets,
                    geo_instances,
                    particle_layers: existing.particle_layers.clone(),
                    particle_textures: existing.particle_textures.clone(),
                    ocean: existing.ocean.clone(),
                    environment: existing.environment.clone(),
                }));
            } else {
                guard.terrain = Some(std::sync::Arc::new(RetainedTerrainFrame {
                    camera,
                    geometry_revision: 1,
                    geo_revision: next_geo_rev,
                    geo_mesh_assets,
                    geo_instances,
                    ..Default::default()
                }));
            }
        }
        self.inner
            .geometry_generation
            .fetch_add(1, std::sync::atomic::Ordering::Release);
        self.inner
            .camera_generation
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }

    /// Updates camera uniforms, dynamic 3D overlays, and hardware-instanced geodetic objects atomically.
    pub fn update_camera_and_world_overlays(
        &self,
        camera: RetainedCameraUniforms,
        dynamic_meshes: Vec<RetainedDynamicMesh>,
        geo_mesh_assets: std::sync::Arc<Vec<std::sync::Arc<RetainedMeshAsset>>>,
        geo_instances: std::sync::Arc<Vec<RetainedGeoInstance>>,
    ) {
        if let Ok(mut guard) = self.inner.state.write() {
            guard.camera = camera;
            let next_geo_rev = self
                .inner
                .geo_revision
                .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
                .wrapping_add(1)
                .max(1);
            if let Some(ref existing) = guard.terrain {
                guard.terrain = Some(std::sync::Arc::new(RetainedTerrainFrame {
                    camera,
                    geometry_revision: existing.geometry_revision,
                    geo_revision: next_geo_rev,
                    meshes: existing.meshes.clone(),
                    instances: existing.instances.clone(),
                    textures: existing.textures.clone(),
                    dynamic_meshes,
                    geo_mesh_assets,
                    geo_instances,
                    particle_layers: existing.particle_layers.clone(),
                    particle_textures: existing.particle_textures.clone(),
                    ocean: existing.ocean.clone(),
                    environment: existing.environment.clone(),
                }));
            } else if !geo_instances.is_empty() || !dynamic_meshes.is_empty() {
                guard.terrain = Some(std::sync::Arc::new(RetainedTerrainFrame {
                    camera,
                    geometry_revision: 1,
                    geo_revision: next_geo_rev,
                    dynamic_meshes,
                    geo_mesh_assets,
                    geo_instances,
                    ..Default::default()
                }));
            }
        }
        self.inner
            .camera_generation
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }

    /// Updates camera uniforms and hardware-instanced geodetic objects atomically while preserving
    /// existing terrain meshes, textures, particle volumes, ocean, and dynamic 3D overlays.
    pub fn update_camera_and_geo_objects(
        &self,
        camera: RetainedCameraUniforms,
        geo_mesh_assets: std::sync::Arc<Vec<std::sync::Arc<RetainedMeshAsset>>>,
        geo_instances: std::sync::Arc<Vec<RetainedGeoInstance>>,
    ) {
        if let Ok(mut guard) = self.inner.state.write() {
            guard.camera = camera;
            let next_geo_rev = self
                .inner
                .geo_revision
                .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
                .wrapping_add(1)
                .max(1);
            if let Some(ref existing) = guard.terrain {
                guard.terrain = Some(std::sync::Arc::new(RetainedTerrainFrame {
                    camera,
                    geometry_revision: existing.geometry_revision,
                    geo_revision: next_geo_rev,
                    meshes: existing.meshes.clone(),
                    instances: existing.instances.clone(),
                    textures: existing.textures.clone(),
                    dynamic_meshes: existing.dynamic_meshes.clone(),
                    geo_mesh_assets,
                    geo_instances,
                    particle_layers: existing.particle_layers.clone(),
                    particle_textures: existing.particle_textures.clone(),
                    ocean: existing.ocean.clone(),
                    environment: existing.environment.clone(),
                }));
            } else if !geo_instances.is_empty() {
                guard.terrain = Some(std::sync::Arc::new(RetainedTerrainFrame {
                    camera,
                    geometry_revision: 1,
                    geo_revision: next_geo_rev,
                    geo_mesh_assets,
                    geo_instances,
                    ..Default::default()
                }));
            }
        }
        self.inner
            .camera_generation
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }

    /// Updates only GPU-instanced 2D camera-local particle volumes in O(1) without touching terrain,
    /// dynamic meshes, ocean, or geodetic objects.
    pub fn update_particles(
        &self,
        particle_layers: std::sync::Arc<Vec<RetainedParticleLayer>>,
        particle_textures: std::sync::Arc<Vec<std::sync::Arc<RetainedTileTexture>>>,
    ) {
        if let Ok(mut guard) = self.inner.state.write() {
            let camera = guard.camera;
            if let Some(ref existing) = guard.terrain {
                guard.terrain = Some(std::sync::Arc::new(RetainedTerrainFrame {
                    camera,
                    geometry_revision: existing.geometry_revision,
                    geo_revision: existing.geo_revision,
                    meshes: existing.meshes.clone(),
                    instances: existing.instances.clone(),
                    textures: existing.textures.clone(),
                    dynamic_meshes: existing.dynamic_meshes.clone(),
                    geo_mesh_assets: existing.geo_mesh_assets.clone(),
                    geo_instances: existing.geo_instances.clone(),
                    particle_layers,
                    particle_textures,
                    ocean: existing.ocean.clone(),
                    environment: existing.environment.clone(),
                }));
            } else if !particle_layers.is_empty() {
                guard.terrain = Some(std::sync::Arc::new(RetainedTerrainFrame {
                    camera,
                    geometry_revision: 1,
                    particle_layers,
                    particle_textures,
                    ..Default::default()
                }));
            }
        }
        self.inner
            .camera_generation
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }

    /// Updates camera uniforms and GPU-instanced 2D camera-local particle volumes atomically in O(1).
    pub fn update_camera_and_particles(
        &self,
        camera: RetainedCameraUniforms,
        particle_layers: std::sync::Arc<Vec<RetainedParticleLayer>>,
        particle_textures: std::sync::Arc<Vec<std::sync::Arc<RetainedTileTexture>>>,
    ) {
        if let Ok(mut guard) = self.inner.state.write() {
            guard.camera = camera;
            if let Some(ref existing) = guard.terrain {
                guard.terrain = Some(std::sync::Arc::new(RetainedTerrainFrame {
                    camera,
                    geometry_revision: existing.geometry_revision,
                    geo_revision: existing.geo_revision,
                    meshes: existing.meshes.clone(),
                    instances: existing.instances.clone(),
                    textures: existing.textures.clone(),
                    dynamic_meshes: existing.dynamic_meshes.clone(),
                    geo_mesh_assets: existing.geo_mesh_assets.clone(),
                    geo_instances: existing.geo_instances.clone(),
                    particle_layers,
                    particle_textures,
                    ocean: existing.ocean.clone(),
                    environment: existing.environment.clone(),
                }));
            } else if !particle_layers.is_empty() {
                guard.terrain = Some(std::sync::Arc::new(RetainedTerrainFrame {
                    camera,
                    geometry_revision: 1,
                    particle_layers,
                    particle_textures,
                    ..Default::default()
                }));
            }
        }
        self.inner
            .camera_generation
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }

    /// Updates camera uniforms and GPU ocean surface descriptor atomically in O(1).
    pub fn update_camera_and_ocean(
        &self,
        camera: RetainedCameraUniforms,
        ocean: Option<std::sync::Arc<RetainedOceanSurface>>,
    ) {
        if let Ok(mut guard) = self.inner.state.write() {
            guard.camera = camera;
            if let Some(ref existing) = guard.terrain {
                guard.terrain = Some(std::sync::Arc::new(RetainedTerrainFrame {
                    camera,
                    geometry_revision: existing.geometry_revision,
                    geo_revision: existing.geo_revision,
                    meshes: existing.meshes.clone(),
                    instances: existing.instances.clone(),
                    textures: existing.textures.clone(),
                    dynamic_meshes: existing.dynamic_meshes.clone(),
                    geo_mesh_assets: existing.geo_mesh_assets.clone(),
                    geo_instances: existing.geo_instances.clone(),
                    particle_layers: existing.particle_layers.clone(),
                    particle_textures: existing.particle_textures.clone(),
                    ocean,
                    environment: existing.environment.clone(),
                }));
            } else if ocean.is_some() {
                guard.terrain = Some(std::sync::Arc::new(RetainedTerrainFrame {
                    camera,
                    geometry_revision: 1,
                    ocean,
                    ..Default::default()
                }));
            }
        }
        self.inner
            .camera_generation
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }

    /// Updates camera uniforms, particle volumes, and GPU ocean surface descriptor atomically in O(1).
    pub fn update_camera_and_particles_and_ocean(
        &self,
        camera: RetainedCameraUniforms,
        particle_layers: std::sync::Arc<Vec<RetainedParticleLayer>>,
        particle_textures: std::sync::Arc<Vec<std::sync::Arc<RetainedTileTexture>>>,
        ocean: Option<std::sync::Arc<RetainedOceanSurface>>,
    ) {
        if let Ok(mut guard) = self.inner.state.write() {
            guard.camera = camera;
            if let Some(ref existing) = guard.terrain {
                guard.terrain = Some(std::sync::Arc::new(RetainedTerrainFrame {
                    camera,
                    geometry_revision: existing.geometry_revision,
                    geo_revision: existing.geo_revision,
                    meshes: existing.meshes.clone(),
                    instances: existing.instances.clone(),
                    textures: existing.textures.clone(),
                    dynamic_meshes: existing.dynamic_meshes.clone(),
                    geo_mesh_assets: existing.geo_mesh_assets.clone(),
                    geo_instances: existing.geo_instances.clone(),
                    particle_layers,
                    particle_textures,
                    ocean,
                    environment: existing.environment.clone(),
                }));
            } else if !particle_layers.is_empty() || ocean.is_some() {
                guard.terrain = Some(std::sync::Arc::new(RetainedTerrainFrame {
                    camera,
                    geometry_revision: 1,
                    particle_layers,
                    particle_textures,
                    ocean,
                    ..Default::default()
                }));
            }
        }
        self.inner
            .camera_generation
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }

    /// Updates camera uniforms, hardware-instanced geodetic objects, and particle volumes atomically.
    pub fn update_camera_and_geo_and_particles(
        &self,
        camera: RetainedCameraUniforms,
        geo_mesh_assets: std::sync::Arc<Vec<std::sync::Arc<RetainedMeshAsset>>>,
        geo_instances: std::sync::Arc<Vec<RetainedGeoInstance>>,
        particle_layers: std::sync::Arc<Vec<RetainedParticleLayer>>,
        particle_textures: std::sync::Arc<Vec<std::sync::Arc<RetainedTileTexture>>>,
    ) {
        if let Ok(mut guard) = self.inner.state.write() {
            guard.camera = camera;
            let next_geo_rev = self
                .inner
                .geo_revision
                .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
                .wrapping_add(1)
                .max(1);
            if let Some(ref existing) = guard.terrain {
                guard.terrain = Some(std::sync::Arc::new(RetainedTerrainFrame {
                    camera,
                    geometry_revision: existing.geometry_revision,
                    geo_revision: next_geo_rev,
                    meshes: existing.meshes.clone(),
                    instances: existing.instances.clone(),
                    textures: existing.textures.clone(),
                    dynamic_meshes: existing.dynamic_meshes.clone(),
                    geo_mesh_assets,
                    geo_instances,
                    particle_layers,
                    particle_textures,
                    ocean: existing.ocean.clone(),
                    environment: existing.environment.clone(),
                }));
            } else if !geo_instances.is_empty() || !particle_layers.is_empty() {
                guard.terrain = Some(std::sync::Arc::new(RetainedTerrainFrame {
                    camera,
                    geometry_revision: 1,
                    geo_revision: next_geo_rev,
                    geo_mesh_assets,
                    geo_instances,
                    particle_layers,
                    particle_textures,
                    ..Default::default()
                }));
            }
        }
        self.inner
            .camera_generation
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }

    /// Updates camera uniforms, dynamic overlays, geodetic objects, and particle volumes atomically.
    pub fn update_camera_and_world_and_particles(
        &self,
        camera: RetainedCameraUniforms,
        dynamic_meshes: Vec<RetainedDynamicMesh>,
        geo_mesh_assets: std::sync::Arc<Vec<std::sync::Arc<RetainedMeshAsset>>>,
        geo_instances: std::sync::Arc<Vec<RetainedGeoInstance>>,
        particle_layers: std::sync::Arc<Vec<RetainedParticleLayer>>,
        particle_textures: std::sync::Arc<Vec<std::sync::Arc<RetainedTileTexture>>>,
    ) {
        if let Ok(mut guard) = self.inner.state.write() {
            guard.camera = camera;
            let next_geo_rev = self
                .inner
                .geo_revision
                .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
                .wrapping_add(1)
                .max(1);
            if let Some(ref existing) = guard.terrain {
                guard.terrain = Some(std::sync::Arc::new(RetainedTerrainFrame {
                    camera,
                    geometry_revision: existing.geometry_revision,
                    geo_revision: next_geo_rev,
                    meshes: existing.meshes.clone(),
                    instances: existing.instances.clone(),
                    textures: existing.textures.clone(),
                    dynamic_meshes,
                    geo_mesh_assets,
                    geo_instances,
                    particle_layers,
                    particle_textures,
                    ocean: existing.ocean.clone(),
                    environment: existing.environment.clone(),
                }));
            } else if !geo_instances.is_empty()
                || !dynamic_meshes.is_empty()
                || !particle_layers.is_empty()
            {
                guard.terrain = Some(std::sync::Arc::new(RetainedTerrainFrame {
                    camera,
                    geometry_revision: 1,
                    geo_revision: next_geo_rev,
                    dynamic_meshes,
                    geo_mesh_assets,
                    geo_instances,
                    particle_layers,
                    particle_textures,
                    ..Default::default()
                }));
            }
        }
        self.inner
            .camera_generation
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }

    /// Updates camera uniforms, dynamic overlays, geodetic objects, particle volumes, and GPU ocean atomically.
    pub fn update_camera_and_world_and_particles_and_ocean(
        &self,
        camera: RetainedCameraUniforms,
        dynamic_meshes: Vec<RetainedDynamicMesh>,
        geo_mesh_assets: std::sync::Arc<Vec<std::sync::Arc<RetainedMeshAsset>>>,
        geo_instances: std::sync::Arc<Vec<RetainedGeoInstance>>,
        particle_layers: std::sync::Arc<Vec<RetainedParticleLayer>>,
        particle_textures: std::sync::Arc<Vec<std::sync::Arc<RetainedTileTexture>>>,
        ocean: Option<std::sync::Arc<RetainedOceanSurface>>,
    ) {
        if let Ok(mut guard) = self.inner.state.write() {
            guard.camera = camera;
            let next_geo_rev = self
                .inner
                .geo_revision
                .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
                .wrapping_add(1)
                .max(1);
            if let Some(ref existing) = guard.terrain {
                guard.terrain = Some(std::sync::Arc::new(RetainedTerrainFrame {
                    camera,
                    geometry_revision: existing.geometry_revision,
                    geo_revision: next_geo_rev,
                    meshes: existing.meshes.clone(),
                    instances: existing.instances.clone(),
                    textures: existing.textures.clone(),
                    dynamic_meshes,
                    geo_mesh_assets,
                    geo_instances,
                    particle_layers,
                    particle_textures,
                    ocean,
                    environment: existing.environment.clone(),
                }));
            } else if !geo_instances.is_empty()
                || !dynamic_meshes.is_empty()
                || !particle_layers.is_empty()
                || ocean.is_some()
            {
                guard.terrain = Some(std::sync::Arc::new(RetainedTerrainFrame {
                    camera,
                    geometry_revision: 1,
                    geo_revision: next_geo_rev,
                    dynamic_meshes,
                    geo_mesh_assets,
                    geo_instances,
                    particle_layers,
                    particle_textures,
                    ocean,
                    ..Default::default()
                }));
            }
        }
        self.inner
            .camera_generation
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }

    pub fn set_fallback_scene(&self, scene: std::sync::Arc<Scene3D>) {
        if let Ok(mut guard) = self.inner.state.write() {
            guard.fallback_scene = Some(scene);
        }
        self.inner
            .overlay_generation
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }

    #[inline]
    pub fn camera_generation(&self) -> u64 {
        self.inner
            .camera_generation
            .load(std::sync::atomic::Ordering::Acquire)
    }

    #[inline]
    pub fn geometry_generation(&self) -> u64 {
        self.inner
            .geometry_generation
            .load(std::sync::atomic::Ordering::Acquire)
    }

    #[inline]
    pub fn overlay_generation(&self) -> u64 {
        self.inner
            .overlay_generation
            .load(std::sync::atomic::Ordering::Acquire)
    }

    #[inline]
    pub fn generations(&self) -> (u64, u64, u64) {
        (
            self.camera_generation(),
            self.geometry_generation(),
            self.overlay_generation(),
        )
    }

    pub fn camera(&self) -> RetainedCameraUniforms {
        self.inner
            .state
            .read()
            .map(|g| g.camera)
            .unwrap_or_default()
    }

    pub fn snapshot_terrain(&self) -> Option<std::sync::Arc<RetainedTerrainFrame>> {
        self.inner.state.read().ok().and_then(|g| g.terrain.clone())
    }

    pub fn fallback_scene(&self) -> Option<std::sync::Arc<Scene3D>> {
        self.inner
            .state
            .read()
            .ok()
            .and_then(|g| g.fallback_scene.clone())
    }
}

/// Top-Level 3D Scene representation embedded in a Strata Viewport.
///
/// The background, celestial, cloud, overlay, and texture store state are private so adding
/// them does not break struct literals written by existing callers. Use the
/// `with_*` builders; the accessors below read them back.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Scene3D {
    pub camera: Camera3D,
    pub lights: Vec<Light3D>,
    pub meshes: Vec<Mesh3D>,
    background: Background3D,
    celestial: Vec<CelestialBody3D>,
    cloud_shells: Vec<CloudShell3D>,
    overlay: Option<Overlay3D>,
    /// Camera up hint in world space. `None` uses the renderer's fallback.
    camera_up: Option<[f64; 3]>,
    /// Optional shared texture registry resolved automatically by software, Cairo, and GPU viewport rasterizers.
    textures: Option<std::sync::Arc<crate::texture3d::TextureStore>>,
    /// Optional retained terrain snapshot attached for zero-rebuild GPU vertex pulling.
    retained_terrain: Option<std::sync::Arc<RetainedTerrainFrame>>,
    /// Optional retained scene handle allowing out-of-band O(1) camera/worker updates.
    retained_handle: Option<RetainedScene3D>,
}

impl Scene3D {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_retained_terrain(mut self, terrain: std::sync::Arc<RetainedTerrainFrame>) -> Self {
        self.retained_terrain = Some(terrain);
        self
    }

    pub fn set_retained_terrain(&mut self, terrain: Option<std::sync::Arc<RetainedTerrainFrame>>) {
        self.retained_terrain = terrain;
    }

    pub fn retained_terrain(&self) -> Option<&std::sync::Arc<RetainedTerrainFrame>> {
        self.retained_terrain.as_ref()
    }

    pub fn with_retained_handle(mut self, handle: RetainedScene3D) -> Self {
        self.retained_handle = Some(handle);
        self
    }

    pub fn set_retained_handle(&mut self, handle: Option<RetainedScene3D>) {
        self.retained_handle = handle;
    }

    pub fn retained_handle(&self) -> Option<&RetainedScene3D> {
        self.retained_handle.as_ref()
    }

    pub fn add_mesh(&mut self, mesh: Mesh3D) {
        self.meshes.push(mesh);
    }

    pub fn add_light(&mut self, light: Light3D) {
        self.lights.push(light);
    }

    /// Replaces the background fill used before geometry is drawn.
    pub fn with_background(mut self, background: Background3D) -> Self {
        self.background = background;
        self
    }

    /// Adds an emissive celestial body.
    pub fn with_celestial(mut self, body: CelestialBody3D) -> Self {
        self.celestial.push(body);
        self
    }

    /// Adds a translucent cloud shell.
    pub fn with_cloud_shell(mut self, shell: CloudShell3D) -> Self {
        self.cloud_shells.push(shell);
        self
    }

    /// Attaches the overlay pass drawn after geometry.
    pub fn with_overlay(mut self, overlay: Overlay3D) -> Self {
        self.overlay = Some(overlay);
        self
    }

    pub fn overlay(&self) -> Option<&Overlay3D> {
        self.overlay.as_ref()
    }

    /// Returns a mutable reference to the scene's 3D overlay pass, initializing an empty
    /// [`Overlay3D`] if none is attached yet.
    pub fn overlay_mut(&mut self) -> &mut Overlay3D {
        self.overlay.get_or_insert_with(Overlay3D::new)
    }

    /// Attaches a shared [`crate::texture3d::TextureStore`] carried with the scene through
    /// `DrawCommand::DrawViewport3D` into live window and offscreen rasterizers.
    pub fn with_textures(
        mut self,
        textures: std::sync::Arc<crate::texture3d::TextureStore>,
    ) -> Self {
        self.textures = Some(textures);
        self
    }

    /// Sets or clears the shared [`crate::texture3d::TextureStore`] attached to this scene.
    pub fn set_textures(
        &mut self,
        textures: Option<std::sync::Arc<crate::texture3d::TextureStore>>,
    ) {
        self.textures = textures;
    }

    /// Borrows the shared [`crate::texture3d::TextureStore`] handle attached to this scene, if any.
    pub fn textures(&self) -> Option<&std::sync::Arc<crate::texture3d::TextureStore>> {
        self.textures.as_ref()
    }

    /// Borrows the underlying [`crate::texture3d::TextureStore`] attached to this scene, if any.
    pub fn textures_deref(&self) -> Option<&crate::texture3d::TextureStore> {
        self.textures.as_deref()
    }

    /// Sets the camera's up direction. A downward-looking camera has no unique
    /// screen up from the world Y axis, so scenes that care about roll should
    /// declare it, for example the local east-north-up frame at a ground focus.
    pub fn with_camera_up(mut self, up: [f64; 3]) -> Self {
        self.camera_up = Some(up);
        self
    }

    pub fn camera_up(&self) -> Option<[f64; 3]> {
        self.camera_up
    }

    pub fn background(&self) -> Background3D {
        self.background
    }

    pub fn celestial(&self) -> &[CelestialBody3D] {
        &self.celestial
    }

    pub fn cloud_shells(&self) -> &[CloudShell3D] {
        &self.cloud_shells
    }
}

/// Callback that rasterizes a text run into the framebuffer.
///
/// The 3D overlay pass needs text, but `strata_render` must not depend on the
/// platform text backend. Consumers install a rasterizer, exactly like the
/// layout crate installs a text measurer.
pub type TextRasterizer = crate::overlay3d::TextRasterizer;

thread_local! {
    static TEXT_RASTERIZER: std::cell::Cell<Option<TextRasterizer>> = const { std::cell::Cell::new(None) };
}

/// Installs the overlay text rasterizer for this thread.
pub fn register_overlay_text_rasterizer(rasterizer: TextRasterizer) {
    TEXT_RASTERIZER.with(|slot| slot.set(Some(rasterizer)));
}

/// Returns the installed overlay text rasterizer, if any.
pub fn overlay_text_rasterizer() -> Option<TextRasterizer> {
    TEXT_RASTERIZER.with(|slot| slot.get())
}

// ---------------------------------------------------------------------------
// Declarative & Programmatic 3D RSX Viewport Components
// ---------------------------------------------------------------------------

use strata_dom::{Attribute, NodeBlueprint, Tag, View};
use strata_macro::component;

/// Conversion trait allowing RSX `<Canvas3D scene={...}>` and `<SceneViewport scene={...}>`
/// props to accept owned [`Scene3D`], `Option<Scene3D>`, or zero-copy `Arc<Scene3D>` handles.
pub trait IntoScene3DProp {
    fn into_scene3d_prop(self) -> Option<std::sync::Arc<Scene3D>>;
}

impl IntoScene3DProp for Scene3D {
    fn into_scene3d_prop(self) -> Option<std::sync::Arc<Scene3D>> {
        Some(std::sync::Arc::new(self))
    }
}

impl IntoScene3DProp for Option<Scene3D> {
    fn into_scene3d_prop(self) -> Option<std::sync::Arc<Scene3D>> {
        self.map(std::sync::Arc::new)
    }
}

impl IntoScene3DProp for std::sync::Arc<Scene3D> {
    fn into_scene3d_prop(self) -> Option<std::sync::Arc<Scene3D>> {
        Some(self)
    }
}

impl IntoScene3DProp for &std::sync::Arc<Scene3D> {
    fn into_scene3d_prop(self) -> Option<std::sync::Arc<Scene3D>> {
        Some(std::sync::Arc::clone(self))
    }
}

impl IntoScene3DProp for Option<std::sync::Arc<Scene3D>> {
    fn into_scene3d_prop(self) -> Option<std::sync::Arc<Scene3D>> {
        self
    }
}

impl IntoScene3DProp for Option<&std::sync::Arc<Scene3D>> {
    fn into_scene3d_prop(self) -> Option<std::sync::Arc<Scene3D>> {
        self.map(std::sync::Arc::clone)
    }
}

#[allow(non_snake_case)]
pub fn Canvas3D(class: Option<&'static str>, children: Option<Vec<NodeBlueprint>>) -> View {
    let mut builder = Canvas3D::builder();
    if let Some(c) = class {
        builder = builder.class(c);
    }
    if let Some(ch) = children {
        builder = builder.children(ch);
    }
    builder.build()
}

#[allow(non_snake_case)]
pub struct Canvas3D {}

impl Canvas3D {
    pub fn builder() -> Canvas3DBuilder {
        Canvas3DBuilder::default()
    }
}

/// Conversion trait allowing `on_camera_change` on 3D viewports (`Canvas3D`, `SceneViewport`)
/// to accept closures, `Rc<dyn Fn(&OrbitControls)>`, or `Option<Rc<dyn Fn(&OrbitControls)>>`.
pub trait IntoCameraChangeProp {
    fn into_camera_change_prop(
        self,
    ) -> Option<std::rc::Rc<dyn Fn(&crate::orbit_controls::OrbitControls)>>;
}

impl IntoCameraChangeProp for Option<std::rc::Rc<dyn Fn(&crate::orbit_controls::OrbitControls)>> {
    fn into_camera_change_prop(
        self,
    ) -> Option<std::rc::Rc<dyn Fn(&crate::orbit_controls::OrbitControls)>> {
        self
    }
}

impl IntoCameraChangeProp for std::rc::Rc<dyn Fn(&crate::orbit_controls::OrbitControls)> {
    fn into_camera_change_prop(
        self,
    ) -> Option<std::rc::Rc<dyn Fn(&crate::orbit_controls::OrbitControls)>> {
        Some(self)
    }
}

impl<F: Fn(&crate::orbit_controls::OrbitControls) + 'static> IntoCameraChangeProp for F {
    fn into_camera_change_prop(
        self,
    ) -> Option<std::rc::Rc<dyn Fn(&crate::orbit_controls::OrbitControls)>> {
        Some(std::rc::Rc::new(self))
    }
}

/// Attaches built-in `OrbitControls` pointer drag (orbit/pan) and wheel (zoom) listeners
/// to a 3D canvas attribute list, invoking `cb` whenever the camera state changes.
pub fn attach_orbit_controls_listeners(
    attrs: &mut Vec<Attribute>,
    scene: Option<&Scene3D>,
    width: Option<f32>,
    height: Option<f32>,
    cb: std::rc::Rc<dyn Fn(&crate::orbit_controls::OrbitControls)>,
) {
    use std::cell::RefCell;
    use std::rc::Rc;
    use strata_dom::{Event, EventType};

    let mut initial_controls = match scene {
        Some(s) => crate::orbit_controls::OrbitControls::from_camera(&s.camera),
        None => crate::orbit_controls::OrbitControls::default(),
    };
    initial_controls.enable_damping = false;
    let fov_deg = match scene.map(|s| &s.camera) {
        Some(Camera3D::Perspective {
            fov_horizontal_deg, ..
        }) => *fov_horizontal_deg,
        _ => 60.0,
    };
    let fallback_vw = width.unwrap_or(800.0).max(1.0);
    let fallback_vh = height.unwrap_or(600.0).max(1.0);
    let controls = Rc::new(RefCell::new(initial_controls));

    let ctrl_down = Rc::clone(&controls);
    attrs.push(Attribute::event_listener(
        EventType::PointerDown,
        move |ev: Event| {
            let btn = ev.button.max(0) as u8;
            ctrl_down
                .borrow_mut()
                .on_pointer_down(ev.client_x as f32, ev.client_y as f32, btn);
        },
    ));

    let ctrl_move = Rc::clone(&controls);
    let cb_move = Rc::clone(&cb);
    attrs.push(Attribute::event_listener(
        EventType::PointerMove,
        move |ev: Event| {
            let changed = {
                let mut c = ctrl_move.borrow_mut();
                let active = c.active_mode != crate::orbit_controls::OrbitInteractionMode::None;
                if active {
                    let vw = if ev.width > 0.0 {
                        (ev.width as f32).max(1.0)
                    } else {
                        fallback_vw
                    };
                    let vh = if ev.height > 0.0 {
                        (ev.height as f32).max(1.0)
                    } else {
                        fallback_vh
                    };
                    c.on_pointer_move(ev.client_x as f32, ev.client_y as f32, vw, vh, fov_deg);
                }
                active
            };
            if changed {
                let c = ctrl_move.borrow();
                cb_move(&c);
            }
        },
    ));

    let ctrl_up = Rc::clone(&controls);
    attrs.push(Attribute::event_listener(
        EventType::PointerUp,
        move |_ev: Event| {
            ctrl_up.borrow_mut().on_pointer_up();
        },
    ));

    let ctrl_wheel = Rc::clone(&controls);
    let cb_wheel = Rc::clone(&cb);
    attrs.push(Attribute::event_listener(
        EventType::Wheel,
        move |ev: Event| {
            ev.default_prevented.set(true);
            {
                let mut c = ctrl_wheel.borrow_mut();
                c.on_wheel(ev.delta_y as f32);
            }
            let c = ctrl_wheel.borrow();
            cb_wheel(&c);
        },
    ));
}

#[derive(Default)]
pub struct Canvas3DBuilder {
    pub scene: Option<std::sync::Arc<Scene3D>>,
    pub retained: Option<RetainedScene3D>,
    pub uncapped: bool,
    pub responsive: bool,
    pub class: Option<strata_dom::ClassList>,
    pub style: Option<String>,
    pub id: Option<String>,
    pub title: Option<String>,
    pub __strata_key: Option<String>,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub on_camera_change: Option<std::rc::Rc<dyn Fn(&crate::orbit_controls::OrbitControls)>>,
    pub events: Vec<Attribute>,
    pub children: Option<Vec<NodeBlueprint>>,
}

impl Canvas3DBuilder {
    pub fn scene(mut self, scene: impl IntoScene3DProp) -> Self {
        self.scene = scene.into_scene3d_prop();
        self
    }

    pub fn class(mut self, class: impl strata_dom::IntoClassList) -> Self {
        self.class = Some(class.into_class_list());
        self
    }

    pub fn style(mut self, style: impl Into<String>) -> Self {
        self.style = Some(style.into());
        self
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn title(mut self, title: impl strata_dom::IntoOptionalString) -> Self {
        self.title = title.into_optional_string();
        self
    }

    pub fn key(mut self, key: impl std::fmt::Display) -> Self {
        self.__strata_key = Some(key.to_string());
        self
    }

    pub fn width(mut self, width: impl Into<Option<f32>>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Option<f32>>) -> Self {
        self.height = height.into();
        self
    }

    /// When `true`, omits inline `width`/`height` attributes so CSS/flex classes
    /// (`w-full h-full`, `flex-1`, split panes) control the resolved viewport size.
    pub fn responsive(mut self, responsive: bool) -> Self {
        self.responsive = responsive;
        self
    }

    pub fn on_camera_change(mut self, cb: impl IntoCameraChangeProp) -> Self {
        self.on_camera_change = cb.into_camera_change_prop();
        self
    }

    pub fn on_camera_change_fn(
        mut self,
        cb: impl Fn(&crate::orbit_controls::OrbitControls) + 'static,
    ) -> Self {
        self.on_camera_change = Some(std::rc::Rc::new(cb));
        self
    }

    fn push_event<Args>(
        mut self,
        ty: strata_dom::EventType,
        handler: impl strata_dom::EventHandlerMarker<Args> + 'static,
    ) -> Self {
        self.events.push(Attribute::event_listener(ty, handler));
        self
    }

    pub fn onmousedown(self, handler: impl Fn(strata_dom::Event) + 'static) -> Self {
        self.push_event(strata_dom::EventType::MouseDown, handler)
    }

    pub fn onmousedown_fn(self, handler: impl Fn(strata_dom::Event) + 'static) -> Self {
        self.onmousedown(handler)
    }

    pub fn onmousemove(self, handler: impl Fn(strata_dom::Event) + 'static) -> Self {
        self.push_event(strata_dom::EventType::MouseMove, handler)
    }

    pub fn onmousemove_fn(self, handler: impl Fn(strata_dom::Event) + 'static) -> Self {
        self.onmousemove(handler)
    }

    pub fn onmouseup(self, handler: impl Fn(strata_dom::Event) + 'static) -> Self {
        self.push_event(strata_dom::EventType::MouseUp, handler)
    }

    pub fn onmouseup_fn(self, handler: impl Fn(strata_dom::Event) + 'static) -> Self {
        self.onmouseup(handler)
    }

    pub fn onmouseenter<Args>(
        self,
        handler: impl strata_dom::EventHandlerMarker<Args> + 'static,
    ) -> Self {
        self.push_event(strata_dom::EventType::MouseEnter, handler)
    }

    pub fn onmouseenter_fn(self, handler: impl Fn(strata_dom::Event) + 'static) -> Self {
        self.push_event(strata_dom::EventType::MouseEnter, handler)
    }

    pub fn onmouseleave<Args>(
        self,
        handler: impl strata_dom::EventHandlerMarker<Args> + 'static,
    ) -> Self {
        self.push_event(strata_dom::EventType::MouseLeave, handler)
    }

    pub fn onmouseleave_fn(self, handler: impl Fn(strata_dom::Event) + 'static) -> Self {
        self.push_event(strata_dom::EventType::MouseLeave, handler)
    }

    pub fn onwheel(self, handler: impl Fn(strata_dom::Event) + 'static) -> Self {
        self.push_event(strata_dom::EventType::Wheel, handler)
    }

    pub fn onwheel_fn(self, handler: impl Fn(strata_dom::Event) + 'static) -> Self {
        self.onwheel(handler)
    }

    pub fn onpointerdown(self, handler: impl Fn(strata_dom::Event) + 'static) -> Self {
        self.push_event(strata_dom::EventType::PointerDown, handler)
    }

    pub fn onpointerdown_fn(self, handler: impl Fn(strata_dom::Event) + 'static) -> Self {
        self.onpointerdown(handler)
    }

    pub fn onpointermove(self, handler: impl Fn(strata_dom::Event) + 'static) -> Self {
        self.push_event(strata_dom::EventType::PointerMove, handler)
    }

    pub fn onpointermove_fn(self, handler: impl Fn(strata_dom::Event) + 'static) -> Self {
        self.onpointermove(handler)
    }

    pub fn onpointerup(self, handler: impl Fn(strata_dom::Event) + 'static) -> Self {
        self.push_event(strata_dom::EventType::PointerUp, handler)
    }

    pub fn onpointerup_fn(self, handler: impl Fn(strata_dom::Event) + 'static) -> Self {
        self.onpointerup(handler)
    }

    pub fn onclick<Args>(self, handler: impl strata_dom::EventHandlerMarker<Args> + 'static) -> Self {
        self.push_event(strata_dom::EventType::Click, handler)
    }

    pub fn onclick_fn(self, handler: impl Fn(strata_dom::Event) + 'static) -> Self {
        self.push_event(strata_dom::EventType::Click, handler)
    }

    pub fn ondblclick<Args>(
        self,
        handler: impl strata_dom::EventHandlerMarker<Args> + 'static,
    ) -> Self {
        self.push_event(strata_dom::EventType::DblClick, handler)
    }

    pub fn ondblclick_fn(self, handler: impl Fn(strata_dom::Event) + 'static) -> Self {
        self.push_event(strata_dom::EventType::DblClick, handler)
    }

    pub fn oncontextmenu<Args>(
        self,
        handler: impl strata_dom::EventHandlerMarker<Args> + 'static,
    ) -> Self {
        self.push_event(strata_dom::EventType::ContextMenu, handler)
    }

    pub fn oncontextmenu_fn(self, handler: impl Fn(strata_dom::Event) + 'static) -> Self {
        self.push_event(strata_dom::EventType::ContextMenu, handler)
    }

    pub fn onresize(self, handler: impl Fn(strata_dom::Event) + 'static) -> Self {
        self.push_event(strata_dom::EventType::Resize, handler)
    }

    pub fn onresize_fn(self, handler: impl Fn(strata_dom::Event) + 'static) -> Self {
        self.onresize(handler)
    }

    /// Binds a persistent [`RetainedScene3D`] handle for out-of-band zero-rebuild GPU updates.
    pub fn retained(mut self, handle: impl Into<Option<RetainedScene3D>>) -> Self {
        self.retained = handle.into();
        self
    }

    /// Opts this 3D canvas into uncapped vsync-off presentation when supported by the surface.
    pub fn uncapped(mut self, uncapped: bool) -> Self {
        self.uncapped = uncapped;
        self
    }

    pub fn children(mut self, children: impl strata_dom::IntoChildren<Vec<NodeBlueprint>>) -> Self {
        self.children = Some(children.into_children());
        self
    }

    pub fn build(self) -> View {
        let render = move || {
            let _scope = strata_dom::ComponentScopeGuard::enter(concat!(
                module_path!(),
                "::Canvas3D"
            ));
            let is_responsive =
                self.responsive || (self.width.is_none() && self.height.is_none());

            let mut attrs = Vec::with_capacity(6 + self.events.len());
            attrs.push(Attribute::property("data-strata-canvas3d", "true"));
            if self.uncapped {
                attrs.push(Attribute::property("data-strata-uncapped", "true"));
            }
            if let Some(id) = self.id {
                attrs.push(Attribute::property("id", id));
            }
            if let Some(title) = self.title {
                attrs.push(Attribute::property("title", title));
            }
            if !is_responsive {
                if let Some(w) = self.width {
                    attrs.push(Attribute::property("width", w.to_string()));
                }
                if let Some(h) = self.height {
                    attrs.push(Attribute::property("height", h.to_string()));
                }
            }
            if let Some(style) = self.style {
                attrs.push(Attribute::property("style", style));
            }
            attrs.extend(self.events);
            let effective_scene = match (self.scene, self.retained) {
                (Some(mut scene), Some(retained)) => {
                    if scene.retained_handle().is_none() {
                        let s = std::sync::Arc::make_mut(&mut scene);
                        s.set_retained_handle(Some(retained));
                    }
                    Some(scene)
                }
                (Some(scene), None) => Some(scene),
                (None, Some(retained)) => {
                    let mut scene = retained
                        .fallback_scene()
                        .map(|s| (*s).clone())
                        .unwrap_or_default();
                    scene.set_retained_handle(Some(retained));
                    Some(std::sync::Arc::new(scene))
                }
                (None, None) => None,
            };
            if let Some(cb) = self.on_camera_change {
                attach_orbit_controls_listeners(
                    &mut attrs,
                    effective_scene.as_deref(),
                    if is_responsive { None } else { self.width },
                    if is_responsive { None } else { self.height },
                    cb,
                );
            }
            if let Some(scene) = effective_scene {
                let payload: std::sync::Arc<dyn std::any::Any + Send + Sync> = scene;
                attrs.push(Attribute::scene_payload_arc(payload));
            }
            View::new(NodeBlueprint::element(
                Tag::Canvas,
                self.class.unwrap_or_default(),
                attrs,
                self.children.unwrap_or_default(),
            ))
        };

        if let Some(k) = self.__strata_key {
            strata_dom::with_component_key(k, render)
        } else {
            render()
        }
    }
}

#[component]
pub fn PerspectiveCamera(
    fov: Option<f32>,
    aspect: Option<f32>,
    near: Option<f32>,
    far: Option<f32>,
    position: Option<[f32; 3]>,
    target: Option<[f32; 3]>,
) -> View {
    let mut attrs = Vec::new();
    if let Some(f) = fov {
        attrs.push(Attribute::property("data-fov", f.to_string()));
    }
    if let Some(a) = aspect {
        attrs.push(Attribute::property("data-aspect", a.to_string()));
    }
    if let Some(n) = near {
        attrs.push(Attribute::property("data-near", n.to_string()));
    }
    if let Some(fa) = far {
        attrs.push(Attribute::property("data-far", fa.to_string()));
    }
    if let Some(p) = position {
        attrs.push(Attribute::property(
            "data-position",
            format!("{},{},{}", p[0], p[1], p[2]),
        ));
    }
    if let Some(t) = target {
        attrs.push(Attribute::property(
            "data-target",
            format!("{},{},{}", t[0], t[1], t[2]),
        ));
    }
    View::new(NodeBlueprint::element(
        Tag::Custom("PerspectiveCamera"),
        strata_dom::ClassList::default(),
        attrs,
        Vec::new(),
    ))
}

#[component]
pub fn OrthographicCamera(
    left: Option<f32>,
    right: Option<f32>,
    top: Option<f32>,
    bottom: Option<f32>,
    near: Option<f32>,
    far: Option<f32>,
    position: Option<[f32; 3]>,
    target: Option<[f32; 3]>,
) -> View {
    let mut attrs = Vec::new();
    if let Some(v) = left {
        attrs.push(Attribute::property("data-left", v.to_string()));
    }
    if let Some(v) = right {
        attrs.push(Attribute::property("data-right", v.to_string()));
    }
    if let Some(v) = top {
        attrs.push(Attribute::property("data-top", v.to_string()));
    }
    if let Some(v) = bottom {
        attrs.push(Attribute::property("data-bottom", v.to_string()));
    }
    if let Some(v) = near {
        attrs.push(Attribute::property("data-near", v.to_string()));
    }
    if let Some(v) = far {
        attrs.push(Attribute::property("data-far", v.to_string()));
    }
    if let Some(p) = position {
        attrs.push(Attribute::property(
            "data-position",
            format!("{},{},{}", p[0], p[1], p[2]),
        ));
    }
    if let Some(t) = target {
        attrs.push(Attribute::property(
            "data-target",
            format!("{},{},{}", t[0], t[1], t[2]),
        ));
    }
    View::new(NodeBlueprint::element(
        Tag::Custom("OrthographicCamera"),
        strata_dom::ClassList::default(),
        attrs,
        Vec::new(),
    ))
}

#[component]
pub fn AmbientLight(color: Option<&'static str>, intensity: Option<f32>) -> View {
    let mut attrs = Vec::new();
    if let Some(c) = color {
        attrs.push(Attribute::property("data-color", c));
    }
    if let Some(i) = intensity {
        attrs.push(Attribute::property("data-intensity", i.to_string()));
    }
    View::new(NodeBlueprint::element(
        Tag::Custom("AmbientLight"),
        strata_dom::ClassList::default(),
        attrs,
        Vec::new(),
    ))
}

#[component]
pub fn DirectionalLight(
    position: Option<[f32; 3]>,
    color: Option<&'static str>,
    intensity: Option<f32>,
    cast_shadow: Option<bool>,
) -> View {
    let mut attrs = Vec::new();
    if let Some(p) = position {
        attrs.push(Attribute::property(
            "data-position",
            format!("{},{},{}", p[0], p[1], p[2]),
        ));
    }
    if let Some(c) = color {
        attrs.push(Attribute::property("data-color", c));
    }
    if let Some(i) = intensity {
        attrs.push(Attribute::property("data-intensity", i.to_string()));
    }
    if let Some(s) = cast_shadow {
        attrs.push(Attribute::property("data-cast-shadow", s.to_string()));
    }
    View::new(NodeBlueprint::element(
        Tag::Custom("DirectionalLight"),
        strata_dom::ClassList::default(),
        attrs,
        Vec::new(),
    ))
}

#[component]
pub fn PointLight(
    position: Option<[f32; 3]>,
    color: Option<&'static str>,
    intensity: Option<f32>,
    distance: Option<f32>,
) -> View {
    let mut attrs = Vec::new();
    if let Some(p) = position {
        attrs.push(Attribute::property(
            "data-position",
            format!("{},{},{}", p[0], p[1], p[2]),
        ));
    }
    if let Some(c) = color {
        attrs.push(Attribute::property("data-color", c));
    }
    if let Some(i) = intensity {
        attrs.push(Attribute::property("data-intensity", i.to_string()));
    }
    if let Some(d) = distance {
        attrs.push(Attribute::property("data-distance", d.to_string()));
    }
    View::new(NodeBlueprint::element(
        Tag::Custom("PointLight"),
        strata_dom::ClassList::default(),
        attrs,
        Vec::new(),
    ))
}

#[component]
pub fn Mesh(
    position: Option<[f32; 3]>,
    rotation: Option<[f32; 3]>,
    scale: Option<[f32; 3]>,
    children: Option<Vec<NodeBlueprint>>,
) -> View {
    let mut attrs = Vec::new();
    if let Some(p) = position {
        attrs.push(Attribute::property(
            "data-position",
            format!("{},{},{}", p[0], p[1], p[2]),
        ));
    }
    if let Some(r) = rotation {
        attrs.push(Attribute::property(
            "data-rotation",
            format!("{},{},{}", r[0], r[1], r[2]),
        ));
    }
    if let Some(s) = scale {
        attrs.push(Attribute::property(
            "data-scale",
            format!("{},{},{}", s[0], s[1], s[2]),
        ));
    }
    View::new(NodeBlueprint::element(
        Tag::Custom("Mesh"),
        strata_dom::ClassList::default(),
        attrs,
        children.unwrap_or_default(),
    ))
}

#[component]
pub fn BoxGeometry(width: Option<f32>, height: Option<f32>, depth: Option<f32>) -> View {
    let mut attrs = Vec::new();
    if let Some(w) = width {
        attrs.push(Attribute::property("data-width", w.to_string()));
    }
    if let Some(h) = height {
        attrs.push(Attribute::property("data-height", h.to_string()));
    }
    if let Some(d) = depth {
        attrs.push(Attribute::property("data-depth", d.to_string()));
    }
    View::new(NodeBlueprint::element(
        Tag::Custom("BoxGeometry"),
        strata_dom::ClassList::default(),
        attrs,
        Vec::new(),
    ))
}

#[component]
pub fn SphereGeometry(
    radius: Option<f32>,
    width_segments: Option<u32>,
    height_segments: Option<u32>,
) -> View {
    let mut attrs = Vec::new();
    if let Some(r) = radius {
        attrs.push(Attribute::property("data-radius", r.to_string()));
    }
    if let Some(w) = width_segments {
        attrs.push(Attribute::property("data-width-segments", w.to_string()));
    }
    if let Some(h) = height_segments {
        attrs.push(Attribute::property("data-height-segments", h.to_string()));
    }
    View::new(NodeBlueprint::element(
        Tag::Custom("SphereGeometry"),
        strata_dom::ClassList::default(),
        attrs,
        Vec::new(),
    ))
}

#[component]
pub fn PlaneGeometry(width: Option<f32>, height: Option<f32>) -> View {
    let mut attrs = Vec::new();
    if let Some(w) = width {
        attrs.push(Attribute::property("data-width", w.to_string()));
    }
    if let Some(h) = height {
        attrs.push(Attribute::property("data-height", h.to_string()));
    }
    View::new(NodeBlueprint::element(
        Tag::Custom("PlaneGeometry"),
        strata_dom::ClassList::default(),
        attrs,
        Vec::new(),
    ))
}

#[component]
pub fn CylinderGeometry(
    radius_top: Option<f32>,
    radius_bottom: Option<f32>,
    height: Option<f32>,
    radial_segments: Option<u32>,
) -> View {
    let mut attrs = Vec::new();
    if let Some(rt) = radius_top {
        attrs.push(Attribute::property("data-radius-top", rt.to_string()));
    }
    if let Some(rb) = radius_bottom {
        attrs.push(Attribute::property("data-radius-bottom", rb.to_string()));
    }
    if let Some(h) = height {
        attrs.push(Attribute::property("data-height", h.to_string()));
    }
    if let Some(rs) = radial_segments {
        attrs.push(Attribute::property("data-radial-segments", rs.to_string()));
    }
    View::new(NodeBlueprint::element(
        Tag::Custom("CylinderGeometry"),
        strata_dom::ClassList::default(),
        attrs,
        Vec::new(),
    ))
}

#[component]
pub fn StandardMaterial(
    color: Option<&'static str>,
    roughness: Option<f32>,
    metalness: Option<f32>,
) -> View {
    let mut attrs = Vec::new();
    if let Some(c) = color {
        attrs.push(Attribute::property("data-color", c));
    }
    if let Some(r) = roughness {
        attrs.push(Attribute::property("data-roughness", r.to_string()));
    }
    if let Some(m) = metalness {
        attrs.push(Attribute::property("data-metalness", m.to_string()));
    }
    View::new(NodeBlueprint::element(
        Tag::Custom("StandardMaterial"),
        strata_dom::ClassList::default(),
        attrs,
        Vec::new(),
    ))
}

#[component]
pub fn ShaderMaterial(
    glsl_fragment: Option<&'static str>,
    uniform_time: Option<f32>,
    uniforms: Option<strata_js::JsValue>,
) -> View {
    let mut attrs = Vec::new();
    if let Some(g) = glsl_fragment {
        attrs.push(Attribute::property("data-glsl", g));
    }
    if let Some(t) = uniform_time {
        attrs.push(Attribute::property("data-time", t.to_string()));
    }
    View::new(NodeBlueprint::element(
        Tag::Custom("ShaderMaterial"),
        strata_dom::ClassList::default(),
        attrs,
        Vec::new(),
    ))
}

fn get_3d_attr<'a>(attrs: &'a [strata_dom::Attribute], key: &str) -> Option<&'a str> {
    attrs.iter().find_map(|attr| {
        if let strata_dom::Attribute::Property { key: k, value: v } = attr {
            if k == key {
                return Some(v.as_str());
            }
        }
        None
    })
}

fn parse_3d_vec3(s: &str) -> Option<[f32; 3]> {
    let mut parts = s.split(',');
    let x = parts.next()?.trim().parse::<f32>().ok()?;
    let y = parts.next()?.trim().parse::<f32>().ok()?;
    let z = parts.next()?.trim().parse::<f32>().ok()?;
    Some([x, y, z])
}

/// Extracts an `Arc<Scene3D>` from a `<Canvas3D>` or viewport [`NodeBlueprint`]:
/// returns a zero-copy `Arc::clone` when an explicit `Attribute::ScenePayload` is attached
/// with no declarative child overrides, or compiles declarative children when present.
pub fn extract_scene3d_arc_from_blueprint(
    canvas_node: &NodeBlueprint,
) -> Option<std::sync::Arc<Scene3D>> {
    let NodeBlueprint::Element {
        attributes,
        children,
        ..
    } = canvas_node
    else {
        return None;
    };

    let mut payload_scene: Option<std::sync::Arc<Scene3D>> = None;
    for attr in attributes {
        if let strata_dom::Attribute::ScenePayload(any) = attr {
            if let Ok(arc_scene) = any.clone().downcast::<Scene3D>() {
                payload_scene = Some(arc_scene);
                break;
            }
            if let Some(nested_arc) = any.downcast_ref::<std::sync::Arc<Scene3D>>() {
                payload_scene = Some(std::sync::Arc::clone(nested_arc));
                break;
            }
        }
    }

    if let Some(arc_scene) = payload_scene {
        if children.is_empty() {
            return Some(arc_scene);
        }
        return Some(std::sync::Arc::new(compile_scene3d_from_blueprint(
            canvas_node,
        )));
    }

    Some(std::sync::Arc::new(compile_scene3d_from_blueprint(
        canvas_node,
    )))
}

/// Compiles a `<Canvas3D>` blueprint subtree (including any attached programmatic `ScenePayload`)
/// into a populated [`Scene3D`] instance.
pub fn compile_scene3d_from_blueprint(canvas_node: &NodeBlueprint) -> Scene3D {
    let (attributes, children) = match canvas_node {
        NodeBlueprint::Element {
            attributes,
            children,
            ..
        } => (attributes, children),
        _ => return Scene3D::new(),
    };

    let mut scene = attributes
        .iter()
        .find_map(|attr| match attr {
            strata_dom::Attribute::ScenePayload(any) => {
                any.downcast_ref::<Scene3D>().cloned().or_else(|| {
                    any.downcast_ref::<std::sync::Arc<Scene3D>>()
                        .map(|a| (**a).clone())
                })
            }
            _ => None,
        })
        .unwrap_or_default();

    for child in children {
        if let NodeBlueprint::Element {
            tag,
            attributes,
            children: mesh_children,
            ..
        } = child
        {
            match tag {
                Tag::Custom("PerspectiveCamera") => {
                    let fov = get_3d_attr(attributes, "data-fov")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(60.0);
                    let near = get_3d_attr(attributes, "data-near")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0.1);
                    let far = get_3d_attr(attributes, "data-far")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(1000.0);
                    let position = get_3d_attr(attributes, "data-position")
                        .and_then(parse_3d_vec3)
                        .unwrap_or([0.0, 2.0, 5.0]);
                    let target = get_3d_attr(attributes, "data-target")
                        .and_then(parse_3d_vec3)
                        .unwrap_or([0.0, 0.0, 0.0]);
                    scene.camera = Camera3D::Perspective {
                        fov_horizontal_deg: fov,
                        near,
                        far,
                        position,
                        target,
                    };
                }
                Tag::Custom("OrthographicCamera") => {
                    let left = get_3d_attr(attributes, "data-left")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(-5.0);
                    let right = get_3d_attr(attributes, "data-right")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(5.0);
                    let top = get_3d_attr(attributes, "data-top")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(5.0);
                    let bottom = get_3d_attr(attributes, "data-bottom")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(-5.0);
                    let near = get_3d_attr(attributes, "data-near")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0.1);
                    let far = get_3d_attr(attributes, "data-far")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(1000.0);
                    let position = get_3d_attr(attributes, "data-position")
                        .and_then(parse_3d_vec3)
                        .unwrap_or([0.0, 2.0, 5.0]);
                    let target = get_3d_attr(attributes, "data-target")
                        .and_then(parse_3d_vec3)
                        .unwrap_or([0.0, 0.0, 0.0]);
                    scene.camera = Camera3D::Orthographic {
                        left,
                        right,
                        top,
                        bottom,
                        near,
                        far,
                        position,
                        target,
                    };
                }
                Tag::Custom("AmbientLight") => {
                    let color_str = get_3d_attr(attributes, "data-color").unwrap_or("#ffffff");
                    let color = strata_style::parse_color(color_str).unwrap_or(Color::WHITE);
                    let intensity = get_3d_attr(attributes, "data-intensity")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(1.0);
                    scene.add_light(Light3D::Ambient { color, intensity });
                }
                Tag::Custom("DirectionalLight") => {
                    let pos = get_3d_attr(attributes, "data-position")
                        .and_then(parse_3d_vec3)
                        .unwrap_or([5.0, 10.0, 7.0]);
                    let color_str = get_3d_attr(attributes, "data-color").unwrap_or("#ffffff");
                    let color = strata_style::parse_color(color_str).unwrap_or(Color::WHITE);
                    let intensity = get_3d_attr(attributes, "data-intensity")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(1.0);
                    let cast_shadow = get_3d_attr(attributes, "data-cast-shadow")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(false);
                    scene.add_light(Light3D::Directional {
                        position: pos,
                        color,
                        intensity,
                        cast_shadow,
                    });
                }
                Tag::Custom("PointLight") => {
                    let pos = get_3d_attr(attributes, "data-position")
                        .and_then(parse_3d_vec3)
                        .unwrap_or([0.0, 2.0, 0.0]);
                    let color_str = get_3d_attr(attributes, "data-color").unwrap_or("#ffffff");
                    let color = strata_style::parse_color(color_str).unwrap_or(Color::WHITE);
                    let intensity = get_3d_attr(attributes, "data-intensity")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(1.0);
                    let distance = get_3d_attr(attributes, "data-distance")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(10.0);
                    scene.add_light(Light3D::Point {
                        position: pos,
                        color,
                        intensity,
                        distance,
                    });
                }
                Tag::Custom("Mesh") => {
                    let position = get_3d_attr(attributes, "data-position")
                        .and_then(parse_3d_vec3)
                        .unwrap_or([0.0, 0.0, 0.0]);
                    let rotation = get_3d_attr(attributes, "data-rotation")
                        .and_then(parse_3d_vec3)
                        .unwrap_or([0.0, 0.0, 0.0]);
                    let scale = get_3d_attr(attributes, "data-scale")
                        .and_then(parse_3d_vec3)
                        .unwrap_or([1.0, 1.0, 1.0]);
                    let transform = Transform3D {
                        position,
                        rotation,
                        scale,
                    };

                    let mut geometry = Geometry3D::Box {
                        width: 1.0,
                        height: 1.0,
                        depth: 1.0,
                    };
                    let mut material = Material3D::default();

                    for sub in mesh_children {
                        if let NodeBlueprint::Element {
                            tag: sub_tag,
                            attributes: sub_attrs,
                            ..
                        } = sub
                        {
                            match sub_tag {
                                Tag::Custom("BoxGeometry") => {
                                    let width = get_3d_attr(sub_attrs, "data-width")
                                        .and_then(|v| v.parse().ok())
                                        .unwrap_or(1.0);
                                    let height = get_3d_attr(sub_attrs, "data-height")
                                        .and_then(|v| v.parse().ok())
                                        .unwrap_or(1.0);
                                    let depth = get_3d_attr(sub_attrs, "data-depth")
                                        .and_then(|v| v.parse().ok())
                                        .unwrap_or(1.0);
                                    geometry = Geometry3D::Box {
                                        width,
                                        height,
                                        depth,
                                    };
                                }
                                Tag::Custom("SphereGeometry") => {
                                    let radius = get_3d_attr(sub_attrs, "data-radius")
                                        .and_then(|v| v.parse().ok())
                                        .unwrap_or(1.0);
                                    let width_segments =
                                        get_3d_attr(sub_attrs, "data-width-segments")
                                            .and_then(|v| v.parse().ok())
                                            .unwrap_or(16);
                                    let height_segments =
                                        get_3d_attr(sub_attrs, "data-height-segments")
                                            .and_then(|v| v.parse().ok())
                                            .unwrap_or(16);
                                    geometry = Geometry3D::Sphere {
                                        radius,
                                        width_segments,
                                        height_segments,
                                    };
                                }
                                Tag::Custom("PlaneGeometry") => {
                                    let width = get_3d_attr(sub_attrs, "data-width")
                                        .and_then(|v| v.parse().ok())
                                        .unwrap_or(1.0);
                                    let height = get_3d_attr(sub_attrs, "data-height")
                                        .and_then(|v| v.parse().ok())
                                        .unwrap_or(1.0);
                                    geometry = Geometry3D::Plane { width, height };
                                }
                                Tag::Custom("CylinderGeometry") => {
                                    let radius_top = get_3d_attr(sub_attrs, "data-radius-top")
                                        .and_then(|v| v.parse().ok())
                                        .unwrap_or(1.0);
                                    let radius_bottom =
                                        get_3d_attr(sub_attrs, "data-radius-bottom")
                                            .and_then(|v| v.parse().ok())
                                            .unwrap_or(1.0);
                                    let height = get_3d_attr(sub_attrs, "data-height")
                                        .and_then(|v| v.parse().ok())
                                        .unwrap_or(1.0);
                                    let radial_segments =
                                        get_3d_attr(sub_attrs, "data-radial-segments")
                                            .and_then(|v| v.parse().ok())
                                            .unwrap_or(16);
                                    geometry = Geometry3D::Cylinder {
                                        radius_top,
                                        radius_bottom,
                                        height,
                                        radial_segments,
                                    };
                                }
                                Tag::Custom("StandardMaterial") => {
                                    let color_str =
                                        get_3d_attr(sub_attrs, "data-color").unwrap_or("#ffffff");
                                    let color = strata_style::parse_color(color_str)
                                        .unwrap_or(Color::WHITE);
                                    let roughness = get_3d_attr(sub_attrs, "data-roughness")
                                        .and_then(|v| v.parse().ok())
                                        .unwrap_or(0.5);
                                    let metalness = get_3d_attr(sub_attrs, "data-metalness")
                                        .and_then(|v| v.parse().ok())
                                        .unwrap_or(0.0);
                                    material = Material3D::Standard {
                                        color,
                                        roughness,
                                        metalness,
                                        emissive: None,
                                    };
                                }
                                Tag::Custom("ShaderMaterial") => {
                                    let glsl =
                                        get_3d_attr(sub_attrs, "data-glsl").map(|s| s.to_string());
                                    let time = get_3d_attr(sub_attrs, "data-time")
                                        .and_then(|v| v.parse().ok())
                                        .unwrap_or(0.0);
                                    let mut uniforms = BTreeMap::new();
                                    uniforms.insert("u_time".to_string(), time);
                                    material = Material3D::Shader {
                                        vertex_shader: None,
                                        fragment_shader: None,
                                        glsl_fragment: glsl,
                                        uniforms,
                                    };
                                }
                                _ => {}
                            }
                        }
                    }

                    scene.add_mesh(Mesh3D {
                        geometry,
                        material,
                        transform,
                    });
                }
                _ => {}
            }
        }
    }

    scene
}
