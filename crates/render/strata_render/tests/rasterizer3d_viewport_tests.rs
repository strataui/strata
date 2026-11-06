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
