//! Frame assembly: turn an [`EarthInput`] into a renderer-ready scene.

use crate::geodesy::{
    geodetic_to_ecef, level_for_altitude, resolve_tile_imagery, EarthCamera, Geodetic, TileKey,
    TileTextureResolution,
};
use crate::tiles::{TextureSlot, TileRegistry, TileSource};
use strata_layout::Rect;
use strata_render::camera64::{Camera64, DVec3, RenderFrame64};
use strata_render::overlay3d::{
    LabelAnchor, MarkerKind, Overlay3D, OverlayPrimitive, OverlayTextStyle,
};
use strata_render::scene3d::{
    Background3D, Camera3D, CelestialBody3D, CloudShell3D, Geometry3D, Material3D, Mesh3D, Scene3D,
    TexturedMaterial3D, Transform3D, UvTransform,
};
use strata_render::terrain3d::MeshBuilder;
use strata_render::texture3d::TextureId;
use strata_style::types::Color;

/// Which layers a frame draws.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerToggles {
    pub terrain: bool,
    pub buildings: bool,
    pub graticule: bool,
    pub labels: bool,
    pub clouds: bool,
    pub celestial: bool,
}

impl Default for LayerToggles {
    fn default() -> Self {
        Self {
            terrain: true,
            buildings: true,
            graticule: true,
            labels: true,
            clouds: true,
            celestial: true,
        }
    }
}

/// An entity marker drawn by the overlay pass.
#[derive(Debug, Clone, PartialEq)]
pub struct EntityMarker {
    pub position: Geodetic,
    pub label: String,
    pub color: Color,
}

/// Everything a frame needs.
#[derive(Debug, Clone, PartialEq)]
pub struct EarthInput {
    pub camera: EarthCamera,
    pub viewport: Rect,
    /// Simulation time in Unix seconds, used for sun and moon direction.
    pub sim_time_unix_s: f64,
    /// Cloud coverage, 0..=1.
    pub cloud_cover: f32,
    /// Daylight factor, 0..=1; the caller may derive it from `sim_time_unix_s`.
    pub daylight: f32,
    pub layers: LayerToggles,
    pub graticule_step_deg: f64,
    /// Highest tile level the view may request.
    pub max_tile_level: u8,
    /// Grid resolution of one terrain tile patch.
    pub tile_grid: u32,
    /// Patch size multiplier relative to the camera's ground coverage. Larger
    /// values mean fewer, bigger patches.
    pub patch_scale: f64,
    pub markers: Vec<EntityMarker>,
}

impl Default for EarthInput {
    fn default() -> Self {
        Self {
            camera: EarthCamera::default(),
            viewport: Rect::new(0.0, 0.0, 1440.0, 900.0),
            sim_time_unix_s: 0.0,
            cloud_cover: 0.0,
            daylight: 1.0,
            layers: LayerToggles::default(),
            graticule_step_deg: 30.0,
            max_tile_level: 18,
            tile_grid: 5,
            patch_scale: 0.25,
            markers: Vec::new(),
        }
    }
}

/// Per-frame statistics, published for the budget report.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameStats {
    pub tiles_requested: usize,
    pub tiles_exact: usize,
    pub tiles_from_ancestor: usize,
    pub terrain_triangles: usize,
    pub building_triangles: usize,
    pub overlay_primitives: usize,
}

/// The plugin's reusable frame builder.
///
/// One instance is held by the host and reused every frame so mesh buffers and
/// the texture registry keep their capacity.
#[derive(Debug)]
pub struct EarthScene {
    scene: Scene3D,
    terrain: MeshBuilder,
    buildings: MeshBuilder,
    tiles: TileRegistry,
    stats: FrameStats,
    patches: Vec<PatchFrame>,
    frame_width: f32,
    frame_height: f32,
}

impl Default for EarthScene {
    fn default() -> Self {
        Self::new()
    }
}

impl EarthScene {
    pub fn new() -> Self {
        Self {
            scene: Scene3D::new(),
            terrain: MeshBuilder::new(),
            buildings: MeshBuilder::new(),
            tiles: TileRegistry::new(),
            stats: FrameStats::default(),
            patches: Vec::new(),
            frame_width: 1440.0,
            frame_height: 900.0,
        }
    }

    /// Number of tile images currently uploaded.
    pub fn uploaded_tiles(&self) -> usize {
        self.tiles.len()
    }

    /// Statistics for the most recent build.
    pub fn stats(&self) -> FrameStats {
        self.stats
    }

    /// The scene produced by the most recent build.
    pub fn scene(&self) -> &Scene3D {
        &self.scene
    }

    /// The texture registry the renderer should sample from.
    pub fn tile_registry(&self) -> &TileRegistry {
        &self.tiles
    }

    /// The exact frame the renderer uses to project this scene's overlay.
    ///
    /// Building the frame from `EarthInput` directly can differ from this when
    /// the renderer re-derives the camera, so anchor maths should use this.
    pub fn renderer_overlay_frame(&self) -> RenderFrame64 {
        strata_render::rasterizer3d::overlay_frame(&self.scene, self.viewport_for_frame())
    }

    fn viewport_for_frame(&self) -> Rect {
        Rect::new(0.0, 0.0, self.frame_width, self.frame_height)
    }

    /// The f64 overlay frame matching the camera, for anchor projection.
    pub fn overlay_frame(&self, input: &EarthInput) -> RenderFrame64 {
        let placement = input.camera.placement();
        let camera = Camera64 {
            eye: placement.eye,
            target: placement.aim,
            up: placement.up,
            fov_y_deg: placement.fov_y_deg,
            near: placement.near_m,
            far: placement.far_m,
        };
        RenderFrame64::from_camera(
            &camera,
            input.viewport.width as f64,
            input.viewport.height as f64,
        )
    }

    /// Rebuilds the scene for this frame.
    ///
    /// Tile resolution is pure and can be inspected through [`FrameStats`] and
    /// [`EarthScene::tile_resolutions`] without any GPU work.
    pub fn build(&mut self, input: &EarthInput, source: &mut dyn TileSource) {
        let placement = input.camera.placement();
        let mut stats = FrameStats::default();

        // 1. Camera
        let scene_camera = Camera3D::Perspective {
            fov: placement.fov_y_deg as f32,
            aspect: (input.viewport.width / input.viewport.height.max(1.0)).max(0.01),
            near: placement.near_m as f32,
            far: placement.far_m as f32,
            position: placement.eye.as_f32(),
            target: placement.aim.as_f32(),
        };

        // 2. Background: daylight drives the sky ramp.
        let sky = sky_gradient(input.daylight);
        let mut scene = Scene3D::new();
        scene.camera = scene_camera;
        // Space lighting: a bright ambient fill plus a directional key from the
        // camera's side so terrain and buildings read without a sun shadow pass.
        scene.add_light(strata_render::scene3d::Light3D::Ambient {
            color: Color::rgba(150, 170, 200, 255),
            intensity: 0.25 + 0.2 * input.daylight.clamp(0.0, 1.0),
        });
        let key_dir = placement.eye.sub(placement.aim).normalize();
        scene.add_light(strata_render::scene3d::Light3D::Directional {
            position: [
                key_dir.x as f32 + 0.35,
                key_dir.y as f32 + 0.35,
                key_dir.z as f32 + 0.25,
            ],
            color: Color::rgba(255, 244, 214, 255),
            intensity: 0.15 + 0.2 * input.daylight.clamp(0.0, 1.0),
            cast_shadow: false,
        });
        scene = scene.with_background(sky);

        if input.layers.celestial {
            for body in celestial_bodies(input.sim_time_unix_s, input.daylight) {
                scene = scene.with_celestial(body);
            }
        }
        if input.layers.clouds {
            scene = scene.with_cloud_shell(CloudShell3D {
                centre: [0.0; 3],
                radius: 6_378_137.0 + 12_000.0,
                color: Color::rgba(255, 255, 255, 255),
                coverage: input.cloud_cover,
                opacity: 0.85,
            });
        }

        // 3. Terrain patches around the focus point. Patch size follows the
        // camera's ground coverage, so the frame budget and the rasterizer's
        // per-triangle clipping stay consistent at every zoom level.
        self.terrain.clear();
        self.buildings.clear();
        self.patches.clear();
        let origin = geodetic_to_ecef(input.camera.focus);
        self.terrain.set_origin(origin);
        self.buildings.set_origin(origin);

        if input.layers.terrain {
            let level = level_for_altitude(input.camera.altitude_m(), input.max_tile_level);
            let span_deg = tile_patch_span_deg(&input.camera, input.patch_scale);
            let bounds = surrounding_patches(input.camera.focus, span_deg);
            for patch in bounds {
                stats.tiles_requested += 1;
                let requested = requested_tile(&patch);
                let resolution = resolve_tile_imagery(requested, |candidate| {
                    matches!(source.imagery(candidate), Ok(Some(_)))
                });
                if let Some(resolution) = resolution {
                    if resolution.exact {
                        stats.tiles_exact += 1;
                    } else {
                        stats.tiles_from_ancestor += 1;
                    }
                }

                let relief = match source.elevation(requested) {
                    Ok(Some(elevation)) => height_of(&elevation),
                    _ => 0.0,
                };
                push_tile_patch(&mut self.terrain, patch.bounds, input.tile_grid.max(2), relief);
                if input.layers.buildings {
                    push_placeholder_buildings(&mut self.buildings, origin, patch.bounds, relief);
                }
                self.patches.push(PatchFrame {
                    key: patch.key,
                    bounds: patch.bounds,
                    resolution,
                });
            }
            let _ = level;
        }

        // 4. Upload resolved imagery and attach textured meshes.
        for patch in &self.patches {
            let Some(resolution) = &patch.resolution else {
                continue;
            };
            let slot = TextureSlot {
                key: resolution.source,
                revision: 0,
            };
            let texture = match self.tiles.get(slot) {
                Some(id) => Some(id),
                None => match source.imagery(resolution.source) {
                    Ok(Some(image)) => self.tiles.ensure(slot, &image),
                    _ => None,
                },
            };
            if let Some(texture) = texture {
                attach_tile_quad(&mut scene, patch.bounds, resolution, texture, origin, input);
            }
        }

        stats.terrain_triangles = self.terrain.triangle_count();
        stats.building_triangles = self.buildings.triangle_count();

        // 5. Overlay: graticule, markers, labels.
        let mut overlay = Overlay3D::new();
        if input.layers.graticule {
            push_graticule(&mut overlay, input.graticule_step_deg);
        }
        if input.layers.labels {
            for marker in &input.markers {
                overlay.push(OverlayPrimitive::Marker {
                    anchor: geodetic_to_ecef(marker.position),
                    kind: MarkerKind::Diamond,
                    color: marker.color,
                    size: 7.0,
                });
                overlay.push(OverlayPrimitive::Label {
                    anchor: geodetic_to_ecef(marker.position),
                    text: marker.label.clone(),
                    style: OverlayTextStyle {
                        color: marker.color,
                        scale: 1,
                    },
                    placement: LabelAnchor::Above,
                    leader: true,
                });
            }
        }
        stats.overlay_primitives = overlay.primitives.len();
        if !overlay.is_empty() {
            scene = scene.with_overlay(overlay);
        }

        self.frame_width = input.viewport.width;
        self.frame_height = input.viewport.height;
        self.scene = scene;
        self.stats = stats;
    }

    /// Terrain patches built during the most recent frame, in draw order.
    pub fn patches(&self) -> &[PatchFrame] {
        &self.patches
    }

    /// Tile resolutions computed during the most recent build, one per patch
    /// that had usable imagery.
    pub fn tile_resolutions(&self) -> Vec<(TileKey, TileTextureResolution)> {
        self.patches
            .iter()
            .filter_map(|p| p.resolution.as_ref().map(|r| (p.key, *r)))
            .collect()
    }

    /// Whether any built tile quad is a usable texture target. A tile quad is
    /// only attached when its image is uploaded, so `false` means imagery is
    /// still missing for every visible tile.
    pub fn has_textured_geometry(&self) -> bool {
        self.scene.meshes.iter().any(|mesh| {
            matches!(mesh.geometry, Geometry3D::CustomTextured { .. })
                && matches!(mesh.material, Material3D::Textured(_))
        })
    }
}

/// Shared terrain triangles: one patch per tile, two triangles per grid cell.
pub fn terrain_triangle_count(tile_count: usize, grid: u32) -> usize {
    let cells = (grid.max(2) as usize - 1).pow(2);
    tile_count * cells * 2
}

/// Shared building triangles: four walls plus the roof cap per footprint.
pub fn building_triangle_count(footprints: usize) -> usize {
    footprints * 10
}

fn height_of(elevation: &crate::tiles::TileElevation) -> f64 {
    if elevation.heights_m.is_empty() {
        return 0.0;
    }
    let sum: f64 = elevation.heights_m.iter().map(|h| *h as f64).sum();
    (sum / elevation.heights_m.len() as f64).max(0.0)
}

/// Daylight sky ramp: deep space to atmospheric zenith.
pub fn sky_gradient(daylight: f32) -> Background3D {
    let t = daylight.clamp(0.0, 1.0);
    let mix = |a: u8, b: u8| -> u8 {
        (a as f32 + (b as f32 - a as f32) * t)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Background3D::VerticalGradient {
        top: Color::rgba(mix(2, 30), mix(4, 64), mix(14, 140), 255),
        bottom: Color::rgba(mix(6, 120), mix(10, 180), mix(24, 255), 255),
    }
}

/// Sun and moon positions for a simulation time, in ECEF metres.
///
/// This is a deliberately small ephemeris: a solar and a lunar orbit model with
/// the right gross behaviour (matching seasonal latitude drift and monthly moon
/// phase) without a full almanac.
pub fn celestial_bodies(sim_time_unix_s: f64, daylight: f32) -> Vec<CelestialBody3D> {
    let days = sim_time_unix_s / 86_400.0;
    let sun_angle = (days * std::f64::consts::TAU / 365.256_363).rem_euclid(std::f64::consts::TAU);
    let moon_angle = (days * std::f64::consts::TAU / 27.321_661).rem_euclid(std::f64::consts::TAU);
    let tilt = 23.44f64.to_radians();

    let sun_dir = DVec3::new(
        sun_angle.cos(),
        sun_angle.sin() * tilt.sin(),
        sun_angle.sin() * tilt.cos(),
    );
    let moon_dir = DVec3::new(
        moon_angle.cos(),
        moon_angle.sin() * 0.9,
        moon_angle.sin() * 0.436,
    );

    let bright = daylight.clamp(0.0, 1.0);
    vec![
        CelestialBody3D {
            position: [
                sun_dir.normalize().x * 1.496e11,
                sun_dir.normalize().y * 1.496e11,
                sun_dir.normalize().z * 1.496e11,
            ],
            radius: 4.65e9,
            color: Color::rgba(255, 247, 226, 255),
            glow: 2.0 + bright * 2.0,
            depth_test: false,
        },
        CelestialBody3D {
            position: [
                moon_dir.normalize().x * 3.844e8,
                moon_dir.normalize().y * 3.844e8,
                moon_dir.normalize().z * 3.844e8,
            ],
            radius: 3.47e6,
            color: Color::rgba(203, 213, 225, 255),
            glow: 1.0,
            depth_test: false,
        },
    ]
}

/// Width of one terrain patch in degrees of longitude.
///
/// The software rasterizer evaluates z-buffer depth in f32, which loses
/// precision over a kilometre-scale span at planet altitude. Keeping the
/// displayed patches to a few hundred metres, scaled by `patch_scale`, keeps
/// the depth order stable; see the plan's budget section.
pub fn tile_patch_span_deg(camera: &EarthCamera, patch_scale: f64) -> f64 {
    let metres = camera.ground_span_m() * patch_scale;
    let degrees = metres / (crate::geodesy::wgs84::A * std::f64::consts::PI / 180.0);
    degrees.clamp(1e-6, 20.0)
}

/// One terrain patch in the frame.
#[derive(Debug, Clone, PartialEq)]
pub struct PatchFrame {
    /// Tile key used for imagery and reporting.
    pub key: TileKey,
    pub bounds: PatchBounds,
    pub resolution: Option<TileTextureResolution>,
}

/// The patch containing `focus` plus its eight neighbours.
///
/// `span_deg` is the patch width, as returned by [`tile_patch_span_deg`]. Keys
/// carry the enclosing tile level and index so texture resolution and reporting
/// stay in tile space.
pub fn surrounding_patches(focus: Geodetic, span_deg: f64) -> Vec<PatchFrame> {
    // A zoom level whose tiles are four times the patch width, so the nine
    // patches around the focus share at most a three-by-three block of tiles and
    // any neighbour can be resolved from a shared, wider ancestor.
    let level = ((1.0 / (span_deg.max(1e-9) * 4.0)).log2().round().max(0.0) as i64)
        .clamp(0, 30) as u8;
    let per_axis = 1u32 << level;
    let tile_span = (360.0 / per_axis as f64).min(span_deg.max(1e-6) * 4.0);
    let columns = (360.0 / tile_span).round().max(1.0) as i64;
    let cx = ((focus.lon_deg + 180.0) / tile_span).floor() as i64;
    let cy = ((90.0 - focus.lat_deg) / tile_span).floor() as i64;

    let mut out = Vec::with_capacity(9);
    for dy in -1..=1 {
        let row = cy + dy;
        if row < 0 || row >= columns {
            continue;
        }
        for dx in -1..=1 {
            let column = (cx + dx).rem_euclid(columns);
            let lon0 = -180.0 + column as f64 * tile_span;
            let lat1 = 90.0 - row as f64 * tile_span;
            out.push(PatchFrame {
                key: TileKey::new(level, column as u32, row as u32),
                bounds: PatchBounds {
                    lon0,
                    lon1: lon0 + tile_span,
                    lat0: lat1 - tile_span,
                    lat1,
                },
                resolution: None,
            });
        }
    }
    out
}

/// The tile whose imagery a patch wants: the top-left child of its key tile.
///
/// Requesting one level below the patch's enclosing tile means a patch resolves
/// exactly when its parent image is available, which is the first image any
/// loader can supply.
pub fn requested_tile(patch: &PatchFrame) -> TileKey {
    patch.key.child(0).unwrap_or(patch.key)
}

/// The tile at `focus` plus its eight neighbours at `level`.
pub fn surrounding_tiles(focus: Geodetic, level: u8) -> Vec<TileKey> {
    let per_axis = 1u32 << level;
    let lon = focus.lon_deg.rem_euclid(360.0) / 360.0;
    let lat = (90.0 - focus.lat_deg.clamp(-85.0, 85.0)) / 180.0;
    let cx = ((lon * per_axis as f64) as i64).rem_euclid(per_axis as i64);
    let cy = ((lat * per_axis as f64) as i64).clamp(0, per_axis as i64 - 1);

    let mut out = Vec::with_capacity(9);
    for dy in -1i64..=1 {
        for dx in -1i64..=1 {
            let x = (cx + dx).rem_euclid(per_axis as i64) as u32;
            let y = cy + dy;
            if (0..per_axis as i64).contains(&y) {
                out.push(TileKey::new(level, x, y as u32));
            }
        }
    }
    out
}

/// Geographic bounds of one terrain patch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PatchBounds {
    /// West edge longitude in degrees.
    pub lon0: f64,
    /// East edge longitude in degrees.
    pub lon1: f64,
    /// South edge latitude in degrees.
    pub lat0: f64,
    /// North edge latitude in degrees.
    pub lat1: f64,
}

fn push_tile_patch(builder: &mut MeshBuilder, bounds: PatchBounds, grid: u32, relief_m: f64) {
    let cols = grid.max(2) as usize;
    let rows = cols;
    let mut positions = Vec::with_capacity(cols * rows);
    for row in 0..rows {
        let v = row as f64 / (rows - 1) as f64;
        for col in 0..cols {
            let u = col as f64 / (cols - 1) as f64;
            let lon = bounds.lon0 + (bounds.lon1 - bounds.lon0) * u;
            let lat = (bounds.lat0 + (bounds.lat1 - bounds.lat0) * v).clamp(-89.9, 89.9);
            // A flat patch at the tile's relief height. Real height fields
            // replace this by sampling the elevation grid per vertex; the
            // builder and origin contract are identical.
            let surface = geodetic_to_ecef(Geodetic::new(lat, lon, 0.0));
            positions.push(surface.add(surface.normalize().scale(relief_m)));
        }
    }
    builder.push_grid(&positions, cols, rows, (0.0, 0.0, 1.0, 1.0));
}

fn push_placeholder_buildings(builder: &mut MeshBuilder, origin: DVec3, bounds: PatchBounds, relief_m: f64) {
    // Deterministic footprint at the patch centre; real footprints arrive with
    // the building source. Reporting them keeps the budget honest.
    let lon = (bounds.lon0 + bounds.lon1) * 0.5;
    let lat = (bounds.lat0 + bounds.lat1) * 0.5;
    let base = geodetic_to_ecef(Geodetic::new(lat, lon, relief_m)).sub(origin);
    builder.push_building(
        base,
        (40.0, 25.0),
        60.0,
        (0.0, 0.0, 0.5, 0.5),
        (0.5, 0.0, 1.0, 1.0),
    );
}

fn attach_tile_quad(
    scene: &mut Scene3D,
    bounds: PatchBounds,
    resolution: &TileTextureResolution,
    texture: TextureId,
    origin: DVec3,
    input: &EarthInput,
) {
    let cols = input.tile_grid.max(2);
    let rows = cols;
    let (u0, v0, u1, v1) = resolution.uv_rect;

    let mut vertices = Vec::with_capacity(cols as usize * rows as usize * 8);
    for row in 0..rows {
        let v = row as f64 / (rows - 1) as f64;
        for col in 0..cols {
            let u = col as f64 / (cols - 1) as f64;
            let p_lat = (bounds.lat0 + (bounds.lat1 - bounds.lat0) * v).clamp(-89.9, 89.9);
            let p_lon = bounds.lon0 + (bounds.lon1 - bounds.lon0) * u;
            let surface = geodetic_to_ecef(Geodetic::new(p_lat, p_lon, 0.0));
            let local = surface.sub(origin);
            // The sphere's outward radial is the surface normal.
            let normal = surface.normalize();
            vertices.extend_from_slice(&[
                local.x as f32,
                local.y as f32,
                local.z as f32,
                normal.x as f32,
                normal.y as f32,
                normal.z as f32,
                (u0 + (u1 - u0) * u) as f32,
                (v0 + (v1 - v0) * v) as f32,
            ]);
        }
    }
    let mut indices = Vec::with_capacity((cols as usize - 1) * (rows as usize - 1) * 6);
    for row in 0..rows - 1 {
        for col in 0..cols - 1 {
            let i = row * cols + col;
            indices.extend_from_slice(&[i, i + 1, i + 1 + cols, i, i + 1 + cols, i + cols]);
        }
    }

    scene.add_mesh(
        Mesh3D::new(
            Geometry3D::CustomTextured { vertices, indices },
            Material3D::Textured(
                TexturedMaterial3D::new(texture).with_uv_transform(UvTransform::IDENTITY),
            ),
        )
        .with_transform(Transform3D::default()),
    );
}

/// Builds a graticule of meridians and parallels at patch altitude.
pub fn push_graticule(overlay: &mut Overlay3D, step_deg: f64) {
    if step_deg <= 0.0 || step_deg > 90.0 {
        return;
    }
    const SAMPLES: usize = 72;
    let color = Color::rgba(148, 163, 184, 90);
    let altitude = 2_000.0;

    let mut meridian = step_deg;
    while meridian < 180.0 {
        for sign in [-1.0, 1.0] {
            let points = (0..=SAMPLES)
                .map(|i| {
                    let lat = -85.0 + 170.0 * i as f64 / SAMPLES as f64;
                    geodetic_to_ecef(Geodetic::new(lat, meridian * sign, altitude))
                })
                .collect();
            overlay.push(OverlayPrimitive::Polyline {
                points,
                color,
                width: 1.0,
            });
        }
        meridian += step_deg;
    }

    let mut parallel = 0.0;
    while parallel <= 90.0 - step_deg {
        for sign in [-1.0, 1.0] {
            let lat = parallel * sign;
            let points = (0..=SAMPLES)
                .map(|i| {
                    let lon = 180.0 * i as f64 / SAMPLES as f64;
                    geodetic_to_ecef(Geodetic::new(lat, lon, altitude))
                })
                .collect();
            overlay.push(OverlayPrimitive::Polyline {
                points,
                color,
                width: 1.0,
            });
        }
        parallel += step_deg;
    }
}
