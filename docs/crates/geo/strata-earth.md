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
their capacity, and a steady-state rebuild does not allocate. Terrain is emitted
as one batched textured mesh and all building prisms as one mesh, so a populated
frame is about ten draws rather than one per patch.

## Engine capabilities it relies on

| Capability | Engine surface |
| --- | --- |
| Textured meshes | `Material3D::Textured`, `Material3D::TexturedBatch`, `Geometry3D::CustomTextured`, `strata_render::texture3d` |
| f64 camera and RTC | `strata_render::camera64::{Camera64, RenderFrame64}` |
| 2D overlay | `strata_render::overlay3d::Overlay3D` |
| Terrain and buildings | `strata_render::terrain3d::MeshBuilder` |
| Sky, sun/moon, clouds | `Background3D`, `CelestialBody3D`, `CloudShell3D` on `Scene3D` |

## Depth policy

Depth precision is a function of the near-to-far ratio, not of the far plane
alone. `EarthCamera::placement` therefore chooses a range from the altitude:

- below `GLOBE_ALTITUDE_M` (2 400 km) the far plane reaches the local horizon
  plus the cloud shell and a margin;
- at or above it the far plane reaches the limb twice over;
- the near plane is `altitude / 100`, never below the caller's requested near
  plane (1 m by default).

A terrain camera four kilometres up gets near 40 m and far about 389 km, not the
51 000 km planet-scale far plane. That is what lets large terrain patches pass
the f32 depth test; the rasterizer keeps the same `[0, 1]` depth convention and
needs no reversed-Z or f64 stage.

Because the near plane follows altitude, anything closer than `altitude / 100`
is clipped: 40 m at a 4 km camera. That is appropriate for terrain, but a host
that renders traffic, an ego model, or a near-ground object must raise
`EarthCamera::near_m` to the nearest expected geometry and keep the far plane
out at the local horizon.

The camera and all geometry are expressed relative to the focus point. The scene
camera is narrowed to f32 only after the f64 subtraction, which is the same RTC
contract `RenderFrame64` uses for the overlay.

`EarthCamera::fov_y_deg` is vertical, matching the reference's fixed focal
length. The engine camera declares `fov_horizontal_deg` and derives the
projection from the viewport aspect, so the plugin converts at the boundary with
`horizontal_fov_deg`. `Camera3D::Perspective` no longer has an `aspect` field:
the viewport is authoritative. A regression test projects a point at half the
reference vertical FOV and requires it to land on the top edge.

## Layers and statistics

`LayerToggles` switch terrain, buildings, graticule, labels, clouds, and celestial
bodies. `EarthScene::stats()` reports requested tiles, exact versus ancestor
resolutions, terrain and building triangle counts, overlay primitive counts, and
draw calls.

## Clouds and celestial bodies

The software rasterizer draws the sky, terrain, buildings, and overlay; it does
not draw cloud shells or celestial discs. `EarthScene` still declares them in the
`Scene3D` for a host pass, with the sun and moon at `depth_test: false` because
they sit beyond any terrain far plane. A cloud shell is a declaration too: at
terrain altitude its Earth-scale radius is far outside the local far plane, so
enabling clouds does not cover or clip the terrain frame.

## Terrain and imagery

Each patch is a tile-aligned grid (default 9x9 at zoom 18) with per-vertex
elevation sampled bilinearly from `TileElevation`. Normals come from the height
field, so relief catches the light. `TileElevation::sample_bilinear_with_gradient`
preserves the reference heightmap behaviour for hillshading.

The plugin separates the decision from the upload:

- `resolve_tile_imagery` is pure: given a target tile and an `is_loaded` probe it
  returns the nearest loaded ancestor plus the exact UV sub-rect.
- `TileSource` supplies decoded imagery and elevation. Implement it over your
  own cache and transport; the crate never spawns a process.

The globe and mid-morph regimes currently draw the same 3x3 patch sheet as the
ground regimes. At 12 000 km that sheet is smaller than the Earth's apparent
disc, so there is no limb and no far-side occlusion. The chosen parity path is a
real sphere surface at Earth radius, tessellated at the tile grid, with the
descriptor gaining a `surface` field to describe it. Until that lands, the
acceptance pair is `terrain_z18` and `night_twilight`; globe and mid-morph deltas
are held.

Cloud shells are declarations today: the software rasterizer has no cloud pass,
so parity captures use `cloud_cover = 0` and the first comparison covers
geometry, UVs, and the sky ramp. A translucent shell pass is the follow-up before
clouds ship.
- `TileRegistry` maps a `(tile, revision)` slot onto a stable `TextureId`.

Patches that have no imagery yet draw with a neutral fallback texture, so the
terrain silhouette is visible while tiles stream in.

## Overlay text

`strata_render` does not ship a platform text backend for 3D overlays. Install one
per thread:

```rust
strata_render::scene3d::register_overlay_text_rasterizer(my_rasterizer);
```

A default 8x16 glyph rasterizer is used by `Overlay3D::render` when no backend is
installed.

## Golden captures and descriptors

```bash
cargo run -p strata_earth_golden --release -- target/earth-goldens
```

Renders four regimes (full globe, mid morph, terrain at Z18, night/twilight),
writes a PNG, a metrics sidecar, and a canonical JSON descriptor, and exits
non-zero if two consecutive runs differ in either pixels or descriptor.

The sidecar's `terrain_layer_cover_px` is structural: the capture renders the
same frame again with `layers.terrain = false` and counts the pixels that
change. The Z18 regime must cover at least half the frame, so a silently missing
terrain layer fails the run.

The descriptor (`strata_earth::descriptor::FrameDescriptor`) records everything
that decides the frame: camera placement, the near/far policy, resolved tile keys
and UV sub-rects, patch bounds, layer toggles, markers, sky ramp, and celestial
and cloud declarations. It is canonical JSON, so two runs serialize identically.

```json
{
  "format": "strata-earth-frame",
  "version": 1,
  "viewport": [1440, 900],
  "camera": {
    "focus": [37.6, -122, 0],
    "aim_offset_m": [0, 0, 0],
    "eye_offset_m": [0, 0, 4000],
    "fov_y_deg": 60,
    "near_m": 40,
    "far_m": 388884.14834571414
  },
  "patches": [
    {
      "key": [13, 1318, 2383],
      "requested": [13, 1318, 2383],
      "bounds": [-122.080078125, -122.0361328125, 37.6171875, 37.63916015625],
      "uv_rect": [0, 0, 1, 1],
      "exact": true
    }
  ],
  "sky": { "top": [30, 64, 140, 255], "bottom": [120, 180, 255, 255] },
  "celestial": [],
  "clouds": null
}
```

`FrameDescriptor::from_json_str` parses it back, `earth_input()` rebuilds the
input, and `replay()` rebuilds the scene and checks that every tile key and UV
sub-rect matches. `strata_earth::json` is a small in-house reader/writer; no JSON
crate is added to the workspace.

`replay` is deliberately strict, for tamper detection. A reference descriptor may
legitimately resolve different tiles, request different keys, or declare another
depth range, so parity reporting uses `compare()` instead. It returns a
`DescriptorDelta` with per-patch missing/extra/UV entries, a `CameraDelta` of
near, far, FOV, and eye differences, and a `DeltaSummary` of counts. It never
fails and never rejects a descriptor whose keys differ from the plugin resolver.

## Budget

Measured at 1440x900 with nine terrain patches at a 9x9 grid with relief, 36
building prisms, the visible-hemisphere graticule, eight markers, and the sky. Terrain is one batched
mesh and buildings are one mesh.

| Profile | Median across runs | Gate | Strict target |
| --- | --- | --- | --- |
| test profile (`cargo test`) | 5.7-6.1 ms | 25 ms | 16.6 ms |
| release | 5.0-5.3 ms | 25 ms | 16.6 ms |

The release figure moves with host load because the rasterizer uses every
available core. The per-run gate allows 25 ms so a noisy CI neighbour does not
fail an unrelated change; the strict 16.6 ms figure is kept as an ignored
benchmark (`--ignored strict_frame_budget`) for a quiet machine.

Small viewports stay serial below 8 192 pixels of viewport area. At 320x200 with
1 152 terrain triangles the serial path measured 2.8 ms against 1.4 ms banded, so
the threshold sits at 64x64 (0.65 ms serial against 0.73 ms banded) rather than
covering a typical preview widget.

## Viewport placement

The banded rasterizer slices framebuffer rows starting at `viewport.y`. A viewport
inset in a window therefore paints its own rows, not rows measured from the
framebuffer origin, and a regression test covers a nonzero `viewport.x` and
`viewport.y` with interior and exterior assertions. Band rows are clamped to the
viewport height so the pixel and depth slices always agree.

## Graticule

Graticule polylines are clipped to the eye-facing hemisphere and cut at the
horizon. A far-side meridian no longer draws across the globe silhouette.

The rasterizer splits the viewport into row bands across the available cores;
each band clips triangles to the viewport and its own rows, so the work is
disjoint and the output stays deterministic.
