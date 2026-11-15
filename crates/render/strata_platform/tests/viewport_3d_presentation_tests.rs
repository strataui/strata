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
        scene,
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
