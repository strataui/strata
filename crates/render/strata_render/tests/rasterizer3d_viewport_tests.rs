//! Rasterizer viewport placement and banding.

use strata_layout::Rect;
use strata_render::rasterizer::FrameBuffer;
use strata_render::rasterizer3d::render_scene_3d;
use strata_render::scene3d::{Camera3D, Geometry3D, Light3D, Material3D, Mesh3D, Scene3D};
use strata_style::types::Color;

fn inset_scene() -> Scene3D {
    let mut scene = Scene3D::new();
    scene.camera = Camera3D::Orthographic {
        left: -1.0,
        right: 1.0,
        top: 1.0,
        bottom: -1.0,
        near: 0.1,
        far: 100.0,
        position: [0.0, 0.0, 10.0],
        target: [0.0, 0.0, 0.0],
    };
    scene.add_light(Light3D::Ambient {
        color: Color::WHITE,
        intensity: 1.0,
    });
    scene.add_mesh(Mesh3D::new(
        Geometry3D::Plane {
            width: 2.0,
            height: 2.0,
        },
        Material3D::Basic {
            color: Color::rgb(240, 40, 40),
        },
    ));
    scene
}

/// A viewport with a nonzero origin must paint its interior and nothing else.
///
/// The banded rasterizer slices the framebuffer by rows, so a band has to be
/// offset by `viewport.y`. When it was not, the scene painted `viewport.y` rows
/// too high and could spill outside the viewport.
#[test]
fn nonzero_viewport_origin_paints_only_the_viewport() {
    let mut fb = FrameBuffer::new(200, 200);
    let viewport = Rect::new(50.0, 60.0, 100.0, 100.0);
    render_scene_3d(&mut fb, viewport, &inset_scene(), None);

    let painted = |x: usize, y: usize| fb.get_pixel(x, y).a() > 0.0;

    // Interior, well away from any edge.
    for (x, y) in [(100, 110), (75, 75), (125, 140)] {
        assert!(
            painted(x, y),
            "viewport interior ({x}, {y}) must be painted"
        );
    }
    // Points above the viewport reproduce the reported regression.
    for (x, y) in [(100, 55), (100, 10), (60, 59)] {
        assert!(
            !painted(x, y),
            "pixel ({x}, {y}) lies above the viewport and must stay clear"
        );
    }
    // Below and beside the viewport stay clear too.
    for (x, y) in [(100, 170), (49, 110), (151, 110)] {
        assert!(
            !painted(x, y),
            "pixel ({x}, {y}) lies outside the viewport and must stay clear"
        );
    }

    let inside = (60..160)
        .flat_map(|y| (50..150).map(move |x| (x, y)))
        .filter(|(x, y)| painted(*x, *y))
        .count();
    let outside = (0..200)
        .flat_map(|y| (0..200).map(move |x| (x, y)))
        .filter(|(x, y)| !(50..150).contains(x) || !(60..160).contains(y))
        .filter(|(x, y)| painted(*x, *y))
        .count();
    assert!(inside > 9_000, "the viewport interior must be filled");
    assert_eq!(outside, 0, "nothing may paint outside the viewport");
}

/// A small preview viewport takes the serial path; the result must match the
/// banded path exactly.
#[test]
fn serial_and_banded_paths_agree() {
    let mut banded = FrameBuffer::new(640, 480);
    let viewport = Rect::new(0.0, 0.0, 640.0, 480.0);
    render_scene_3d(&mut banded, viewport, &inset_scene(), None);

    // 64x64 is below the parallel area threshold, so this exercises the serial
    // path against the banded result above.
    let small = Rect::new(0.0, 0.0, 64.0, 64.0);
    let mut small_fb = FrameBuffer::new(64, 64);
    render_scene_3d(&mut small_fb, small, &inset_scene(), None);

    for y in 0..64 {
        for x in 0..64 {
            let a = banded.get_pixel(x, y);
            let b = small_fb.get_pixel(x, y);
            assert_eq!(
                (a.r(), a.g(), a.b(), a.a()),
                (b.r(), b.g(), b.b(), b.a()),
                "pixel ({x}, {y}) differs between paths"
            );
        }
    }
}

#[test]
fn test_evaluate_gpu_rte_vertex_accuracy() {
    use strata_render::camera64::{DVec3, EARTH_RADIUS_M};
    use strata_render::math3d::Mat4;
    use strata_render::rasterizer3d::evaluate_gpu_rte_vertex;

    let eye = DVec3::new(EARTH_RADIUS_M + 5000.0, 0.0, 0.0);
    let (eye_h, eye_l) = eye.split_high_low();

    let pos = DVec3::new(EARTH_RADIUS_M + 5000.25, 0.0, 0.0);
    let (pos_h, pos_l) = pos.split_high_low();

    let view_proj_rel = Mat4::IDENTITY;
    let clip = evaluate_gpu_rte_vertex(pos_h, pos_l, eye_h, eye_l, &view_proj_rel);

    assert!((clip.x - 0.25).abs() < 1e-6, "clip.x error: got {}", clip.x);
    assert!(clip.y.abs() < 1e-6, "clip.y should be 0");
    assert!(clip.z.abs() < 1e-6, "clip.z should be 0");
    assert!((clip.w - 1.0).abs() < 1e-6, "clip.w should be 1.0");
}

#[test]
fn test_rte_mesh_builder_interleaved_layout() {
    use strata_render::camera64::{DVec3, EARTH_RADIUS_M};
    use strata_render::terrain3d::RteMeshBuilder;

    let mut builder = RteMeshBuilder::with_capacity(4, 2);
    let p0 = DVec3::new(EARTH_RADIUS_M, 0.0, 0.0);
    let p1 = DVec3::new(EARTH_RADIUS_M + 10.0, 0.0, 0.0);
    let p2 = DVec3::new(EARTH_RADIUS_M + 10.0, 10.0, 0.0);
    let p3 = DVec3::new(EARTH_RADIUS_M, 10.0, 0.0);

    let v0 = builder.push_vertex(p0, [0.0, 0.0, 1.0], (0.0, 0.0));
    let v1 = builder.push_vertex(p1, [0.0, 0.0, 1.0], (1.0, 0.0));
    let v2 = builder.push_vertex(p2, [0.0, 0.0, 1.0], (1.0, 1.0));
    let v3 = builder.push_vertex(p3, [0.0, 0.0, 1.0], (0.0, 1.0));

    builder.push_quad(v0, v1, v2, v3);

    assert_eq!(builder.vertex_count(), 4);
    assert_eq!(builder.triangle_count(), 2);
    assert_eq!(builder.vertices().len(), 4 * RteMeshBuilder::STRIDE);
    assert_eq!(builder.indices().len(), 6);
}
