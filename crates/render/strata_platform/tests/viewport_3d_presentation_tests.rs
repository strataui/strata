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

#[test]
fn test_gtk_gpu_2d_opt_in_default_and_toggle() {
    let initial = strata_platform::gtk_gpu_2d_enabled();
    // Unless STRATA_GPU_2D is explicitly set in process env, Desktop GTK defaults to Cairo + Pango for 2D.
    if std::env::var_os("STRATA_GPU_2D").is_none() {
        assert!(!initial);
    }
    strata_platform::gtk_set_gpu_2d_enabled(true);
    assert!(strata_platform::gtk_gpu_2d_enabled());
    strata_platform::gtk_set_gpu_2d_enabled(false);
    assert!(!strata_platform::gtk_gpu_2d_enabled());
}

#[cfg(feature = "wgpu-backend")]
#[test]
fn test_wgpu_swapchain_presenter_gpu_2d_stencil_clips_and_hybrid_viewport() {
    use strata_platform::{
        gtk_gpu_2d_enabled, PresentMode, SurfaceConfig, SurfacePresenter, WgpuSwapchainPresenter,
    };
    use strata_render::scene3d::{RetainedCameraUniforms, RetainedScene3D};
    use strata_render::{Canvas2DContext, CanvasLineCap, CanvasLineJoin, FrameBuffer, SwapchainFormat};

    let _ = gtk_gpu_2d_enabled();
    let cfg = SurfaceConfig {
        width: 240,
        height: 240,
        scale_factor: 1.0,
        format: SwapchainFormat::Bgra8Unorm,
        present_mode: PresentMode::Immediate,
        ..Default::default()
    };
    let Ok(mut swapchain) = WgpuSwapchainPresenter::new_offscreen(cfg) else {
        return;
    };

    // 1. Construct hybrid DisplayList:
    //    - Background dark slate card behind 3D viewport
    //    - 3D Scene viewport (solid blue) at top-right [120, 10, 110, 110]
    //    - Foreground HUD <Canvas2D> with diagonal ClipPath, dashed StrokeRect, and Round/Bevel stroke
    let cam = RetainedCameraUniforms {
        background: Background3D::Solid(Color::rgb(25, 70, 210)),
        ..Default::default()
    };
    let retained = RetainedScene3D::new(cam);
    let scene = Scene3D::new().with_retained_handle(retained);

    let mut hud = Canvas2DContext::new(240.0, 240.0);
    // Diagonal triangle clip [0,0]->[100,0]->[0,100] + crossing orange FillRect
    hud.save();
    hud.begin_path();
    hud.move_to(0.0, 0.0);
    hud.line_to(100.0, 0.0);
    hud.line_to(0.0, 100.0);
    hud.close_path();
    hud.clip();
    hud.set_fill_style(Color::rgb(255, 130, 0));
    hud.fill_rect(0.0, 0.0, 100.0, 100.0);
    hud.restore();

    // Dashed StrokeRect along bottom [20, 180, 100, 30]
    hud.save();
    hud.set_stroke_style(Color::rgb(240, 50, 120));
    hud.set_line_width(4.0);
    hud.set_line_dash(&[12.0, 12.0]);
    hud.stroke_rect(20.0, 180.0, 100.0, 30.0);
    hud.restore();

    // Capped stroke along [140..210, 170]
    hud.save();
    hud.set_stroke_style(Color::rgb(0, 220, 180));
    hud.set_line_width(8.0);
    hud.set_line_cap(CanvasLineCap::Round);
    hud.set_line_join(CanvasLineJoin::Bevel);
    hud.begin_path();
    hud.move_to(140.0, 170.0);
    hud.line_to(210.0, 170.0);
    hud.stroke();
    hud.restore();

    let mut dl = DisplayList::new();
    dl.push(DrawCommand::DrawRect {
        bounds: Rect::new(0.0, 0.0, 240.0, 240.0),
        color: Color::rgb(12, 16, 24),
    });
    dl.push(DrawCommand::DrawViewport3D {
        bounds: Rect::new(120.0, 10.0, 110.0, 110.0),
        scene: std::sync::Arc::new(scene),
    });
    dl.push(DrawCommand::DrawCanvas2D {
        bounds: Rect::new(0.0, 0.0, 240.0, 240.0),
        commands: hud.commands().to_vec(),
    });

    let (summary, bgra) = swapchain
        .render_display_list_to_bgra(&dl, None, [0.0, 0.0, 0.0, 1.0])
        .expect("WgpuSwapchainPresenter::render_display_list_to_bgra");
    assert!(summary.quad_instances >= 6);
    assert!(summary.path_segments > 0);
    assert_eq!(swapchain.last_batch_summary(), summary);
    assert_eq!(swapchain.presented_frames, 1);

    let px_bgra = |x: usize, y: usize| -> [u8; 4] {
        let i = (y * 240 + x) * 4;
        [bgra[i], bgra[i + 1], bgra[i + 2], bgra[i + 3]]
    };

    // Inside diagonal clip (20,20): orange (B<20, G~130, R=255)
    let in_diag = px_bgra(20, 20);
    assert!(
        in_diag[2] > 200 && in_diag[0] < 30,
        "inside diagonal HUD clip should be orange BGRA: {:?}",
        in_diag
    );
    // Outside diagonal clip in same AABB (75,75): dark background card (12, 16, 24) preserved!
    let out_diag = px_bgra(75, 75);
    assert!(
        out_diag[2] < 40 && out_diag[1] < 40 && out_diag[0] < 50,
        "outside diagonal clip must show background card instead of orange: {:?}",
        out_diag
    );

    // 3D viewport region (175, 65): blue sky/bg (B > R)
    let vp_px = px_bgra(175, 65);
    assert!(
        vp_px[0] > 160 && vp_px[2] < 60,
        "3D viewport interior should render blue Scene3D background: {:?}",
        vp_px
    );

    // Dashed StrokeRect dash on (25, 180) vs gap (38, 180)
    assert!(px_bgra(25, 180)[2] > 180, "dash on pixel should be rose/red");
    assert!(
        px_bgra(38, 180)[2] < 40,
        "dash gap must stay dark card background"
    );

    // Round cap stroke at (175, 170) & round cap extension (137, 170)
    assert!(px_bgra(175, 170)[1] > 180);
    assert!(px_bgra(137, 170)[1] > 120);

    // 2. Verify SurfacePresenter trait & present_display_list path also advance metrics cleanly
    swapchain
        .present_display_list(&dl, Some(Rect::new(0.0, 0.0, 240.0, 240.0)), [0.0; 4])
        .expect("present_display_list");
    assert_eq!(swapchain.presented_frames, 2);
    assert_eq!(swapchain.damage_clipped_frames, 1);

    let fb = FrameBuffer::new(240, 240);
    swapchain.present(&fb, None).expect("SurfacePresenter::present");
    assert_eq!(swapchain.presented_frames, 3);
    assert_eq!(swapchain.surface_size(), (240, 240));
}
