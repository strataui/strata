# strata_earth: Earth Scene Plugin

`strata_earth` builds a renderer-ready Earth scene from a camera, a tile source,
and layer toggles. It is additive: it only uses the public `strata_render` API, so
existing 3D users are unaffected, and it has no `egui`, widget, or shell-command
dependency.

## Quick start

```rust
use strata_earth::{EarthCamera, EarthInput, EarthScene, Geodetic, MemoryTileSource};
use strata_layout::Rect;

let input = EarthInput {
    camera: EarthCamera {
        focus: Geodetic::new(37.7749, -122.4194, 0.0),
        eye_offset_m: [0.0, 0.0, 12_000.0],
        ..Default::default()
    },
    viewport: Rect::new(0.0, 0.0, 1440.0, 900.0),
    ..Default::default()
};

let mut source = MemoryTileSource::new();
let mut scene = EarthScene::new();
scene.build(&input, &mut source);

// `scene.scene()` is a `Scene3D`; `scene.tile_registry().store()` holds uploads.
```

Rebuild one `EarthScene` per frame: its mesh buffers and texture registry keep
their capacity, and a steady-state rebuild does not allocate.

## Engine capabilities it relies on

| Capability | Engine surface |
| --- | --- |
| Textured meshes | `Material3D::Textured`, `Geometry3D::CustomTextured`, `strata_render::texture3d` |
| f64 camera and RTC | `strata_render::camera64::{Camera64, RenderFrame64}` |
| 2D overlay | `strata_render::overlay3d::Overlay3D` |
| Terrain and buildings | `strata_render::terrain3d::MeshBuilder` |
| Sky, sun/moon, clouds | `Background3D`, `CelestialBody3D`, `CloudShell3D` on `Scene3D` |

## Layers and statistics

`LayerToggles` switch terrain, buildings, graticule, labels, clouds, and celestial
bodies. `EarthScene::stats()` reports requested tiles, exact versus ancestor
resolutions, terrain and building triangle counts, and overlay primitive counts.

## Tiles

The plugin separates the decision from the upload:

- `resolve_tile_imagery` is pure: given a target tile and an `is_loaded` probe it
  returns the nearest loaded ancestor plus the exact UV sub-rect.
- `TileSource` supplies decoded imagery and elevation. Implement it over your
  own cache and transport; the crate never spawns a process.
- `TileRegistry` maps a `(tile, revision)` slot onto a stable `TextureId`.

Ancestors count once the parent image exists, because the crate requests the
first child of each patch's enclosing tile.

## Overlay text

`strata_render` does not ship a platform text backend for 3D overlays. Install one
per thread:

```rust
strata_render::scene3d::register_overlay_text_rasterizer(my_rasterizer);
```

A default 8x16 glyph rasterizer is used by `Overlay3D::render` when no backend is
installed.

## Golden captures

```bash
cargo run -p strata_earth_golden --release -- target/earth-goldens
```

Renders four regimes (full globe, mid morph, terrain at Z18, night/twilight),
writes a PNG plus a metrics sidecar, and exits non-zero if two consecutive runs
are not bit-identical.

## Budget

Measured at 1440x900 with nine terrain patches, nine building prisms, the
graticule, and markers: median 4.23 ms per frame in an optimized test profile,
against the 16.6 ms target. Ancestor texture lookups and mesh rebuilds are
included in the measurement.
