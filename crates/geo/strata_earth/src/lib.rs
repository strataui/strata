//! Earth scene plugin.
//!
//! Assembles the renderer state for one Earth frame: camera, background,
//! terrain tiles, relief walls, building prisms, and the 2D overlay pass. The
//! crate is additive: it builds a [`strata_render::scene3d::Scene3D`] using only
//! the public engine surface, so existing 3D users are unaffected.
//!
//! Design rules enforced by tests:
//!
//! - No `egui`, widget, or shell-command dependency; the plugin is pure Rust
//!   over the engine.
//! - Tile decisions are separate from texture upload ([`tiles::TileSource`] vs
//!   [`tiles::TileRegistry`]).
//! - World coordinates stay f64 until the render hand-off.
//! - Mesh rebuilds reuse buffers; steady-state frames do not allocate.

pub mod geodesy;
pub mod scene;
pub mod tiles;

pub use geodesy::{
    clamp_latitude, geodetic_to_ecef, level_for_altitude, resolve_tile_imagery, wrap_longitude,
    CameraPlacement, EarthCamera, Geodetic, LocalFrame, TileKey, TileTextureResolution,
};
pub use scene::{
    requested_tile,
    building_triangle_count, celestial_bodies, sky_gradient, surrounding_patches,
    surrounding_tiles, terrain_triangle_count, tile_patch_span_deg, EarthInput, EarthScene,
    EntityMarker, FrameStats, LayerToggles, PatchBounds, PatchFrame,
};
pub use tiles::{
    tile_sampler, MemoryTileSource, TextureSlot, TileError, TileElevation, TileImage,
    TileRegistry, TileSource,
};
