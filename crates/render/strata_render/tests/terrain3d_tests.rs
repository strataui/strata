//! Terrain, relief, and building mesh construction at scale.
//!
//! The Earth view rebuilds tile height fields, relief skirts, and building
//! prisms every frame. These tests pin the geometry contract and, more
//! importantly, that a steady-state rebuild does not allocate.

use strata_render::camera64::DVec3;
use strata_render::scene3d::Geometry3D;
use strata_render::terrain3d::MeshBuilder;

fn grid_positions(cols: usize, rows: usize, step: f64) -> Vec<DVec3> {
    let mut out = Vec::with_capacity(cols * rows);
    for row in 0..rows {
        for col in 0..cols {
            out.push(DVec3::new(col as f64 * step, row as f64 * step, 0.0));
        }
    }
    out
}

#[test]
fn grid_patch_produces_two_triangles_per_cell() {
    let cols = 9;
    let rows = 9;
    let positions = grid_positions(cols, rows, 10.0);
    let mut builder = MeshBuilder::with_capacity(cols * rows, (cols - 1) * (rows - 1) * 2);
    builder.push_grid(&positions, cols, rows, (0.0, 0.0, 1.0, 1.0));

    assert_eq!(builder.vertex_count(), cols * rows);
    assert_eq!(builder.triangle_count(), (cols - 1) * (rows - 1) * 2);
    assert!(!builder.is_empty());
}

#[test]
fn grid_patch_rejects_malformed_input() {
    let mut builder = MeshBuilder::new();
    builder.push_grid(&[], 0, 0, (0.0, 0.0, 1.0, 1.0));
    builder.push_grid(&grid_positions(4, 4, 1.0), 4, 4, (0.0, 0.0, 1.0, 1.0));
    builder.push_grid(&grid_positions(5, 4, 1.0), 4, 4, (0.0, 0.0, 1.0, 1.0));
    // Both malformed calls are no-ops, so the valid 4x4 patch is the only mesh.
    assert_eq!(builder.triangle_count(), 3 * 3 * 2);
}

#[test]
fn uv_rect_maps_across_the_patch() {
    let cols = 2;
    let rows = 2;
    let positions = grid_positions(cols, rows, 1.0);
    let mut builder = MeshBuilder::new();
    builder.push_grid(&positions, cols, rows, (0.25, 0.5, 0.75, 1.0));
    let geometry = builder.finish().expect("geometry");
    let Geometry3D::CustomTextured { vertices, .. } = geometry else {
        panic!("expected textured geometry");
    };
    // First vertex UV starts at the rect origin, last ends at its far corner.
    assert!((vertices[6] - 0.25).abs() < 1e-6);
    assert!((vertices[7] - 0.5).abs() < 1e-6);
    let len = vertices.len();
    assert!((vertices[len - 2] - 0.75).abs() < 1e-6);
    assert!((vertices[len - 1] - 1.0).abs() < 1e-6);
}

#[test]
fn rtc_origin_is_subtracted_before_narrowing() {
    let origin = DVec3::new(6_378_137.0, 0.0, 0.0);
    let mut builder = MeshBuilder::new();
    builder.set_origin(origin);
    let a = builder.push_vertex(origin, [0.0, 0.0, 1.0], (0.0, 0.0));
    let b = builder.push_vertex(
        origin.add(DVec3::new(1.0, 0.0, 0.0)),
        [0.0, 0.0, 1.0],
        (1.0, 0.0),
    );
    let c = builder.push_vertex(
        origin.add(DVec3::new(0.0, 1.0, 0.0)),
        [0.0, 0.0, 1.0],
        (0.0, 1.0),
    );
    builder.push_triangle(a, b, c);
    let geometry = builder.finish().expect("geometry");
    let Geometry3D::CustomTextured { vertices, .. } = geometry else {
        panic!("expected textured geometry");
    };
    assert_eq!(vertices[0], 0.0, "the RTC origin must become zero");
    assert_eq!(
        vertices[8], 1.0,
        "the neighbour must survive as exactly one metre"
    );
}

#[test]
fn wall_and_building_append_expected_triangle_counts() {
    let mut builder = MeshBuilder::new();
    let top = vec![
        DVec3::new(0.0, 0.0, 10.0),
        DVec3::new(100.0, 0.0, 10.0),
        DVec3::new(200.0, 0.0, 10.0),
    ];
    let bottom = vec![
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(100.0, 0.0, 0.0),
        DVec3::new(200.0, 0.0, 0.0),
    ];
    builder.push_wall(&top, &bottom, (0.0, 0.0, 1.0, 1.0));
    assert_eq!(
        builder.triangle_count(),
        4,
        "two segments, two triangles each"
    );

    let before = builder.triangle_count();
    builder.push_building(
        DVec3::new(50.0, 50.0, 0.0),
        (6.0, 4.0),
        30.0,
        (0.0, 0.0, 0.5, 0.5),
        (0.5, 0.0, 1.0, 0.5),
    );
    // Four walls (8 triangles) plus a roof cap (2 triangles).
    assert_eq!(builder.triangle_count() - before, 10);
}

#[test]
fn degenerate_buildings_are_skipped() {
    let mut builder = MeshBuilder::new();
    builder.push_building(
        DVec3::ZERO,
        (0.0, 4.0),
        30.0,
        (0.0, 0.0, 1.0, 1.0),
        (0.0, 0.0, 1.0, 1.0),
    );
    builder.push_building(
        DVec3::ZERO,
        (6.0, 4.0),
        0.0,
        (0.0, 0.0, 1.0, 1.0),
        (0.0, 0.0, 1.0, 1.0),
    );
    assert!(
        builder.is_empty(),
        "zero-area or zero-height buildings must not emit triangles"
    );
    assert!(builder.finish().is_none());
}

#[test]
fn steady_state_rebuild_does_not_allocate() {
    // A scene-sized rebuild: 64 terrain tiles of 33x33 vertices, four relief
    // walls each, and 512 buildings.
    let cols = 33;
    let rows = 33;
    let positions = grid_positions(cols, rows, 100.0);
    let top: Vec<DVec3> = (0..33)
        .map(|i| DVec3::new(i as f64 * 100.0, 0.0, 50.0))
        .collect();
    let bottom: Vec<DVec3> = (0..33)
        .map(|i| DVec3::new(i as f64 * 100.0, 0.0, 0.0))
        .collect();

    let mut builder = MeshBuilder::new();
    let rebuild = |builder: &mut MeshBuilder| {
        builder.clear();
        for _ in 0..64 {
            builder.push_grid(&positions, cols, rows, (0.0, 0.0, 1.0, 1.0));
            for _ in 0..4 {
                builder.push_wall(&top, &bottom, (0.0, 0.0, 1.0, 1.0));
            }
            for b in 0..512 {
                builder.push_building(
                    DVec3::new((b % 32) as f64 * 100.0, (b / 32) as f64 * 100.0, 0.0),
                    (6.0, 4.0),
                    30.0,
                    (0.0, 0.0, 0.5, 0.5),
                    (0.5, 0.0, 1.0, 0.5),
                );
            }
        }
    };

    // Warm up so the buffers reach their steady-state capacity.
    rebuild(&mut builder);
    let vertex_capacity = builder.vertex_capacity();
    let index_capacity = builder.index_capacity();
    let triangles = builder.triangle_count();
    assert!(
        triangles > 100_000,
        "the workload must be representative, got {triangles} triangles"
    );

    rebuild(&mut builder);
    assert_eq!(
        builder.vertex_capacity(),
        vertex_capacity,
        "a steady-state rebuild must not grow the vertex buffer"
    );
    assert_eq!(
        builder.index_capacity(),
        index_capacity,
        "a steady-state rebuild must not grow the index buffer"
    );
    assert_eq!(builder.triangle_count(), triangles);
}

#[test]
fn quad_and_triangle_reject_degenerate_indices() {
    let mut builder = MeshBuilder::new();
    // Degenerate triangle and fully degenerate quad: nothing is emitted.
    builder.push_triangle(0, 0, 1);
    builder.push_quad(2, 2, 2, 2);
    assert_eq!(builder.triangle_count(), 0);

    // A collapsed quad emits only its surviving triangle.
    builder.push_quad(0, 1, 1, 3);
    assert_eq!(builder.triangle_count(), 1);

    // A regular quad emits both triangles.
    builder.push_quad(0, 1, 2, 3);
    assert_eq!(builder.triangle_count(), 3);
}
