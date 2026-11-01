use strata_layout::Rect;
use strata_render::compositor::{DisplayList, DrawCommand};
use strata_render::rasterizer::FrameBuffer;
use strata_render::swapchain::{
    SoftwareSurfacePresenter, SurfacePresenter, SwapchainCompositor, SwapchainFormat,
};
use strata_style::types::Color;

#[test]
fn test_software_surface_presenter_bgra_full_copy() {
    let width = 100;
    let height = 50;
    let mut fb = FrameBuffer::new(width, height);
    fb.clear(Color::rgba(255, 128, 64, 255)); // Red = 255, Green = 128, Blue = 64, Alpha = 255

    let mut presenter = SoftwareSurfacePresenter::new(width as u32, height as u32, SwapchainFormat::Bgra8Unorm);
    assert_eq!(presenter.surface_size(), (100, 50));
    assert_eq!(presenter.backing_buffer.len(), 100 * 50 * 4);

    presenter.present(&fb, None).expect("Present failed");
    assert_eq!(presenter.presented_frames, 1);
    assert_eq!(presenter.damage_clipped_frames, 0);

    // Verify BGRA layout in backing buffer
    // First pixel: Blue = 64, Green = 128, Red = 255, Alpha = 255
    assert_eq!(presenter.backing_buffer[0], 64);
    assert_eq!(presenter.backing_buffer[1], 128);
    assert_eq!(presenter.backing_buffer[2], 255);
    assert_eq!(presenter.backing_buffer[3], 255);
}

#[test]
fn test_software_surface_presenter_damage_scissoring() {
    let width = 100;
    let height = 100;
    let mut fb = FrameBuffer::new(width, height);
    fb.clear(Color::rgba(10, 20, 30, 255));

    let mut presenter = SoftwareSurfacePresenter::new(width as u32, height as u32, SwapchainFormat::Rgba8Unorm);
    // Initial full present
    presenter.present(&fb, None).expect("Initial present failed");

    // Modify a small region in fb (x: 20..30, y: 40..50)
    for y in 40..50 {
        for x in 20..30 {
            fb.set_pixel(x, y, Color::rgba(200, 200, 200, 255));
        }
    }

    // Present only damage region
    let damage = Rect::new(20.0, 40.0, 10.0, 10.0);
    presenter.present(&fb, Some(damage)).expect("Damage present failed");
    assert_eq!(presenter.presented_frames, 2);
    assert_eq!(presenter.damage_clipped_frames, 1);

    // Check that inside pixel was updated
    let inside_idx = (45 * 100 + 25) * 4;
    assert_eq!(presenter.backing_buffer[inside_idx], 200);
    assert_eq!(presenter.backing_buffer[inside_idx + 1], 200);

    // Check that outside pixel remained original
    let outside_idx = (5 * 100 + 5) * 4;
    assert_eq!(presenter.backing_buffer[outside_idx], 10);
    assert_eq!(presenter.backing_buffer[outside_idx + 1], 20);
    assert_eq!(presenter.backing_buffer[outside_idx + 2], 30);
}

#[test]
fn test_swapchain_compositor_clean_frame_skip_and_force_redraw() {
    let width = 640;
    let height = 480;
    let mut compositor = SwapchainCompositor::new(width, height, 1.0);
    let mut presenter = SoftwareSurfacePresenter::new(width, height, SwapchainFormat::Bgra8Unorm);

    let mut dl = DisplayList::new();
    dl.push(DrawCommand::DrawRect {
        bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
        color: Color::rgba(255, 0, 0, 255),
    });

    // First render with default initial damage
    let presented = compositor.render_and_present(&dl, &mut presenter, false).expect("First render failed");
    assert!(presented);
    assert_eq!(compositor.frame_count, 1);

    // Second render with no new damage -> should cleanly skip!
    let skipped = compositor.render_and_present(&dl, &mut presenter, false).expect("Second render failed");
    assert!(!skipped);
    assert_eq!(compositor.frame_count, 1); // Frame count not incremented on skip

    // Third render with force_redraw -> should render!
    let forced = compositor.render_and_present(&dl, &mut presenter, true).expect("Forced render failed");
    assert!(forced);
    assert_eq!(compositor.frame_count, 2);

    // Resize compositor
    compositor.resize(800, 600, 1.5);
    assert_eq!(compositor.width, 800);
    assert_eq!(compositor.height, 600);
    assert_eq!(compositor.scale_factor, 1.5);
    // Physical size: 800 * 1.5 = 1200, 600 * 1.5 = 900
    assert_eq!(compositor.framebuffer.width, 1200);
    assert_eq!(compositor.framebuffer.height, 900);
}
