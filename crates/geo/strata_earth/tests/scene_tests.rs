//! Frame assembly: block accounting, the four camera regimes, the frame budget,
//! and the dependency-surface contract.
//!
//! The golden images are produced by `examples/strata_earth_golden`; this suite
//! checks the numbers those goldens depend on (tile resolution, block counts,
//! deterministic scene output) and the frame budget at 1440x900.

use std::time::Instant;
use strata_earth::{
    surrounding_tiles, tile_sampler, EarthInput, EarthScene, EntityMarker, Geodetic,
    MemoryTileSource, TileImage, TileKey,
};
use strata_render::overlay3d::OverlayPrimitive;
use strata_render::rasterizer::FrameBuffer;
use strata_render::rasterizer3d::render_scene_3d_with_textures;
use strata_render::scene3d::{Background3D, Geometry3D, Material3D};
use strata_style::types::Color;

const W: usize = 1440;
const H: usize = 900;

fn solid_tile(r: u8, g: u8, b: u8) -> TileImage {
    TileImage {
        width: 2,
        height: 2,
        pixels: (0..4).flat_map(|_| [r, g, b, 255]).collect(),
    }
}

fn source_for(keys: &[TileKey], image: &TileImage) -> MemoryTileSource {
    let mut source = MemoryTileSource::new();
    for key in keys {
        source.insert_imagery(*key, image.clone());
    }
    source
}

fn viewpoint(altitude_m: f64) -> EarthInput {
    EarthInput {
        camera: strata_earth::EarthCamera {
            focus: Geodetic::new(37.7749, -122.4194, 0.0),
            eye_offset_m: [0.0, 0.0, altitude_m],
            ..Default::default()
        },
        viewport: strata_layout::Rect::new(0.0, 0.0, W as f32, H as f32),
        tile_grid: 5,
        ..Default::default()
    }
}

fn render_scene(input: &EarthInput, scene: &EarthScene) -> FrameBuffer {
    let mut fb = FrameBuffer::new(W, H);
    render_scene_3d_with_textures(
        &mut fb,
        input.viewport,
        scene.scene(),
        None,
        Some(scene.tile_registry().store()),
    );
    fb
}

#[test]
fn full_globe_regime_resolves_tiles_and_draws_sky() {
    let input = viewpoint(12_000_000.0);
    let span = strata_earth::tile_patch_span_deg(&input.camera, input.patch_scale);
    let patches = strata_earth::surrounding_patches(input.camera.focus, span);
    let keys: Vec<TileKey> = patches
        .iter()
        .map(strata_earth::requested_tile)
        .collect();
    let mut source = source_for(&keys, &solid_tile(40, 90, 60));
    let mut scene = EarthScene::new();
    scene.build(&input, &mut source);

    assert!(scene.has_textured_geometry(), "tiles must reach the scene");
    assert_eq!(scene.stats().tiles_requested, keys.len());
    assert_eq!(scene.stats().tiles_exact, keys.len());

    let fb = render_scene(&input, &scene);
    assert!(
        fb.get_pixel(W / 2, 10).a() > 0.5,
        "the sky must fill the top edge"
    );
}

#[test]
fn missing_tiles_fall_back_to_the_loaded_ancestor() {
    // Load a single level-0 world image; every patch asks for a deeper tile.
    let mut input = viewpoint(4_000.0);
    input.patch_scale = 0.002;
    let span = strata_earth::tile_patch_span_deg(&input.camera, input.patch_scale);
    // The requested-tile parent chain ends at this shared ancestor.
    let world = strata_earth::requested_tile(
        &strata_earth::surrounding_patches(input.camera.focus, span)[0],
    )
    .parent()
    .expect("an ancestor above the requested tiles");
    assert!(world.level > 0, "the test needs an ancestor above the world");
    let mut source = source_for(&[world], &solid_tile(200, 40, 40));

    let mut scene = EarthScene::new();
    scene.build(&input, &mut source);

    assert!(
        scene.stats().tiles_from_ancestor > 0,
        "unloaded patches must resolve to an ancestor, stats were {:?}",
        scene.stats()
    );
    assert!(scene.has_textured_geometry(), "ancestor imagery must reach the scene");
    for (_, resolution) in scene.tile_resolutions() {
        assert_eq!(resolution.source, world);
        let (u0, v0, u1, v1) = resolution.uv_rect;
        assert!(u1 > u0 && v1 > v0, "the sub-rect must be non-empty");
        assert!(u0 >= 0.0 && u1 <= 1.0 && v0 >= 0.0 && v1 <= 1.0);
    }
}

#[test]
fn no_imagery_leaves_the_scene_textureless_but_renders() {
    let input = viewpoint(120_000.0);
    let mut source = MemoryTileSource::new();
    let mut scene = EarthScene::new();
    scene.build(&input, &mut source);
    assert!(!scene.has_textured_geometry());
    assert_eq!(scene.stats().tiles_from_ancestor, 0);
    let fb = render_scene(&input, &scene);
    assert!(
        fb.get_pixel(W / 2, H / 2).a() > 0.5,
        "the sky must still draw"
    );
}

#[test]
fn terrain_regime_uses_terrain_grid_and_relief() {
    let mut input = viewpoint(2_500.0);
    input.tile_grid = 9;
    let span = strata_earth::tile_patch_span_deg(&input.camera, input.patch_scale);
    let keys: Vec<TileKey> = strata_earth::surrounding_patches(input.camera.focus, span)
        .iter()
        .map(strata_earth::requested_tile)
        .collect();
    let mut source = source_for(&keys, &solid_tile(60, 120, 70));
    // Give every tile a relief so the height path runs.
    source.insert_elevation(
        keys[0],
        strata_earth::TileElevation {
            width: 2,
            height: 2,
            heights_m: vec![420.0, 410.0, 400.0, 395.0],
        },
    );
    let mut scene = EarthScene::new();
    scene.build(&input, &mut source);

    let expected = strata_earth::scene::terrain_triangle_count(keys.len(), 9);
    assert_eq!(scene.stats().terrain_triangles, expected);
    assert_eq!(
        scene.stats().building_triangles,
        strata_earth::scene::building_triangle_count(keys.len())
    );
}

#[test]
fn night_regime_darkens_the_sky_ramp() {
    let day = strata_earth::scene::sky_gradient(1.0);
    let night = strata_earth::scene::sky_gradient(0.0);
    let Background3D::VerticalGradient {
        top: day_top,
        bottom: day_bottom,
    } = day
    else {
        panic!("expected a gradient");
    };
    let Background3D::VerticalGradient {
        top: night_top,
        bottom: night_bottom,
    } = night
    else {
        panic!("expected a gradient");
    };
    assert!(day_top.r() > night_top.r());
    assert!(day_bottom.b() > night_bottom.b());
}

#[test]
fn scene_build_is_deterministic() {
    let input = viewpoint(50_000.0);
    let keys = surrounding_tiles(input.camera.focus, 8);
    let make = || {
        let mut source = source_for(&keys, &solid_tile(10, 20, 30));
        let mut scene = EarthScene::new();
        scene.build(&input, &mut source);
        (scene.scene().clone(), scene.stats())
    };
    let (a, a_stats) = make();
    let (b, b_stats) = make();
    assert_eq!(a, b, "the same input must build the same scene");
    assert_eq!(a_stats, b_stats);
}

#[test]
fn graticule_and_markers_reach_the_overlay() {
    let mut input = viewpoint(9_000_000.0);
    input.markers.push(EntityMarker {
        position: Geodetic::new(37.7749, -122.4194, 0.0),
        label: "SF".to_string(),
        color: Color::rgba(56, 189, 248, 255),
    });
    let mut source = MemoryTileSource::new();
    let mut scene = EarthScene::new();
    scene.build(&input, &mut source);

    let overlay = scene.scene().overlay().expect("overlay attached");
    assert!(
        overlay
            .primitives
            .iter()
            .any(|p| matches!(p, OverlayPrimitive::Marker { .. })),
        "the marker must be present"
    );
    assert!(
        overlay
            .primitives
            .iter()
            .any(|p| matches!(p, OverlayPrimitive::Polyline { .. })),
        "the graticule must be present"
    );
    assert_eq!(scene.stats().overlay_primitives, overlay.primitives.len());

    // Project with the frame the renderer will use for this scene, so the
    // overlay and the geometry share a camera.
    let frame = scene.renderer_overlay_frame();
    let projected = overlay.project(&frame);
    let markers = projected
        .iter()
        .filter(|c| matches!(c, strata_render::overlay3d::OverlayCommand::Marker { .. }))
        .count();
    assert_eq!(
        markers, 1,
        "exactly one marker should project onto the visible globe"
    );
}

#[test]
fn layer_toggles_remove_their_content() {
    let mut input = viewpoint(9_000_000.0);
    input.layers.graticule = false;
    input.layers.labels = false;
    let mut source = MemoryTileSource::new();
    let mut scene = EarthScene::new();
    scene.build(&input, &mut source);
    assert!(
        scene.scene().overlay().is_none(),
        "no overlay when both toggles are off"
    );

    input.layers.terrain = false;
    input.layers.buildings = false;
    scene.build(&input, &mut source);
    assert_eq!(scene.stats().terrain_triangles, 0);
    assert_eq!(scene.stats().building_triangles, 0);
}

#[test]
fn tile_registry_reuses_handles_and_evicts() {
    let key = TileKey::new(4, 3, 2);
    let mut registry = strata_earth::TileRegistry::new();
    let slot = strata_earth::TextureSlot { key, revision: 0 };
    let image = solid_tile(1, 2, 3);
    let first = registry.ensure(slot, &image).expect("upload");
    let second = registry.ensure(slot, &image).expect("reuse");
    assert_eq!(first, second, "the same slot must reuse its handle");
    assert_eq!(registry.len(), 1);

    let evicted = registry.retain(&std::collections::HashSet::new());
    assert_eq!(evicted, 1);
    assert!(registry.is_empty());
    assert!(registry.store().get(first).is_none());

    let sampler = tile_sampler();
    assert_eq!(
        sampler.filter,
        strata_render::texture3d::FilterMode::Bilinear
    );
}

#[test]
fn frame_budget_at_1440x900_is_within_16_6_ms() {
    // Full picture: 9 tiles, terrain, buildings, graticule, markers, sky.
    let mut input = viewpoint(3_000.0);
    input.tile_grid = 9;
    input.markers = (0..8)
        .map(|i| EntityMarker {
            position: Geodetic::new(37.0 + i as f64 * 0.2, -122.0 + i as f64 * 0.2, 0.0),
            label: format!("node-{i}"),
            color: Color::rgba(56, 189, 248, 255),
        })
        .collect();
    let keys = surrounding_tiles(input.camera.focus, 18);
    let mut source = source_for(&keys, &solid_tile(40, 90, 60));
    let mut scene = EarthScene::new();

    // Warm-up: buffers, textures, and the rasterizer caches.
    scene.build(&input, &mut source);
    let mut fb = FrameBuffer::new(W, H);
    render_scene_3d_with_textures(
        &mut fb,
        input.viewport,
        scene.scene(),
        None,
        Some(scene.tile_registry().store()),
    );

    let mut samples = Vec::with_capacity(30);
    for _ in 0..30 {
        let start = Instant::now();
        scene.build(&input, &mut source);
        render_scene_3d_with_textures(
            &mut fb,
            input.viewport,
            scene.scene(),
            None,
            Some(scene.tile_registry().store()),
        );
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = samples[samples.len() / 2];
    let stats = scene.stats();

    println!(
        "earth frame budget: median {median:.2} ms over {} frames (terrain {} tris, buildings {} tris, overlay {} prims, {} tiles)",
        samples.len(),
        stats.terrain_triangles,
        stats.building_triangles,
        stats.overlay_primitives,
        stats.tiles_requested
    );
    assert!(
        stats.terrain_triangles > 500 && stats.building_triangles > 50,
        "the budget must cover a populated scene"
    );
    assert!(
        median <= 16.6,
        "median frame time must stay within the 16.6 ms budget, was {median:.2} ms"
    );
}

#[test]
fn plugin_has_no_widget_or_shell_dependency() {
    // Structural check of the dependency surface. The plugin may only depend on
    // engine crates; no egui, widget, or process-spawning crate is allowed.
    let manifest = include_str!("../Cargo.toml");
    for banned in [
        "egui",
        "strata-egui",
        "strata_egui",
        "winit",
        "git2",
        "reqwest",
    ] {
        assert!(
            !manifest.to_lowercase().contains(&banned.to_lowercase()),
            "the plugin manifest must not depend on {banned}"
        );
    }
    // And no source file may shell out.
    for entry in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src")).expect("src dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("source");
        for banned in ["std::process::Command", "Command::new", "egui::"] {
            assert!(
                !source.contains(banned),
                "{} must not reference {banned}",
                path.display()
            );
        }
    }
}

#[test]
fn textured_meshes_are_the_only_geometry_the_plugin_emits() {
    let input = viewpoint(9_000_000.0);
    let keys = surrounding_tiles(input.camera.focus, 4);
    let mut source = source_for(&keys, &solid_tile(0, 0, 0));
    let mut scene = EarthScene::new();
    scene.build(&input, &mut source);
    for mesh in &scene.scene().meshes {
        assert!(
            matches!(mesh.geometry, Geometry3D::CustomTextured { .. }),
            "tile geometry must carry UVs"
        );
        assert!(
            matches!(mesh.material, Material3D::Textured(_)),
            "tile geometry must be textured"
        );
    }
}
