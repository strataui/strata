//! Automated 3D Viewport Presentation & Cairo Integration Suite.
//!
//! Validates that `DrawCommand::DrawViewport3D` is rendered via `rasterizer3d` into Cairo
//! graphics contexts and produces non-blank, rasterized geometry in window framebuffers.

#![cfg(target_os = "linux")]
#![allow(clippy::chunks_exact_to_as_chunks)]

use strata_layout::Rect;
use strata_platform::render_to_png_bytes;
use strata_render::compositor::{DisplayList, DrawCommand};
use strata_render::scene3d::{
    Background3D, Camera3D, Geometry3D, Light3D, Material3D, Mesh3D, Scene3D,
};
use strata_style::types::Color;

#[test]
fn test_draw_viewport_3d_cairo_presentation_renders_cube() {
    let mut scene = Scene3D::new().with_background(Background3D::Solid(Color::rgb(15, 23, 42))); // Deep slate blue

    // 1. Camera looking at origin
    scene.camera = Camera3D::Perspective {
        fov_horizontal_deg: 60.0,
        near: 0.1,
        far: 100.0,
        position: [2.0, 2.0, 2.0],
        target: [0.0, 0.0, 0.0],
    };

    // 2. Light illuminating cube
    scene.add_light(Light3D::Directional {
        position: [-1.0, 1.0, 1.0],
        color: Color::WHITE,
        intensity: 1.0,
        cast_shadow: false,
    });

    // 3. Bright Emerald Cube Mesh at origin
    let cube_mesh = Mesh3D::new(
        Geometry3D::Box {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        },
        Material3D::Basic {
            color: Color::rgb(16, 185, 129), // Emerald green
        },
    );
    scene.add_mesh(cube_mesh);

    // 4. Construct DisplayList with DrawViewport3D
    let mut dl = DisplayList::new();
    dl.push(DrawCommand::DrawViewport3D {
        bounds: Rect::new(0.0, 0.0, 160.0, 160.0),
        scene: scene.into(),
    });

    // 5. Render directly to Cairo PNG bytes
    let png_bytes = render_to_png_bytes(&dl, 160, 160, None)
        .expect("render_to_png_bytes must successfully render 3D viewport");

    assert!(
        !png_bytes.is_empty(),
        "Rendered PNG bytes must not be empty"
    );
    assert_eq!(
        &png_bytes[0..8],
        &[137, 80, 78, 71, 13, 10, 26, 10],
        "Must produce valid PNG header"
    );

    // 6. Decode PNG image to inspect pixel values directly
    let mut cursor = std::io::Cursor::new(&png_bytes);
    let mut surface =
        cairo::ImageSurface::create_from_png(&mut cursor).expect("Cairo must decode rendered PNG");

    let data = surface
        .data()
        .expect("ImageSurface data must be accessible");
    let mut found_emerald_pixels = false;
    let mut found_background_pixels = false;

    // Check pixels in ARgb32 format (B, G, R, A)
    for pixel in data.chunks_exact(4) {
        let b = pixel[0];
        let g = pixel[1];
        let r = pixel[2];
        let a = pixel[3];

        if a > 0 {
            // Emerald green check: Green channel significantly higher than Red and Blue
            if g > 60 && g > r && g > b {
                found_emerald_pixels = true;
            }
            // Background check: Slate blue (r ~ 15, g ~ 23, b ~ 42)
            if b > r && b > 20 && g < 50 {
                found_background_pixels = true;
            }
        }
    }

    assert!(
        found_emerald_pixels,
        "Rendered 3D scene must contain illuminated emerald cube pixels"
    );
    assert!(
        found_background_pixels,
        "Rendered 3D scene must contain slate blue background pixels"
    );
}

#[cfg(feature = "wgpu-backend")]
#[test]
fn test_retained_3d_wgpu_cairo_presentation_and_presenter() {
    use strata_platform::wgpu_surface::WgpuWindowPresenter;
    use strata_platform::PresentMode;
    use strata_render::scene3d::{RetainedCameraUniforms, RetainedScene3D};

    // 1. Validate WgpuWindowPresenter offscreen & overlay upload when adapter is present.
    if let Ok(mut presenter) = WgpuWindowPresenter::new_offscreen(PresentMode::Immediate) {
        assert_eq!(presenter.present_mode(), PresentMode::Immediate);
        let overlay_bgra = vec![255u8; 64 * 64 * 4];
        presenter.upload_ui_overlay_bgra(64, 64, &overlay_bgra);

        let mut cam = RetainedCameraUniforms {
            background: Background3D::Solid(Color::rgb(20, 40, 110)),
            ..Default::default()
        };
        let retained = RetainedScene3D::new(cam);
        let scene = Scene3D::new().with_retained_handle(retained.clone());
        let bgra = presenter
            .render_offscreen_bgra(&scene, 64, 64, true)
            .expect("offscreen bgra render");
        assert_eq!(bgra.len(), 64 * 64 * 4);
        assert!(
            bgra[0] > bgra[2],
            "solid blue background must render via wgpu"
        );

        // 2. Validate Cairo DisplayList bridge uses wgpu when retained_handle is attached,
        // and updates output when retained.camera_generation() advances on same Arc<Scene3D>.
        let scene_arc = std::sync::Arc::new(scene);
        let mut dl = DisplayList::new();
        dl.push(DrawCommand::DrawViewport3D {
            bounds: Rect::new(0.0, 0.0, 64.0, 64.0),
            scene: std::sync::Arc::clone(&scene_arc),
        });
        let png1 = render_to_png_bytes(&dl, 64, 64, None).expect("first retained png");
        assert_eq!(&png1[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);

        cam.background = Background3D::Solid(Color::rgb(180, 30, 30));
        retained.update_camera(cam);
        let png2 = render_to_png_bytes(&dl, 64, 64, None).expect("second retained png");
        assert_ne!(
            png1, png2,
            "cache key must include RetainedScene3D generations so updates re-render"
        );
    }
}
