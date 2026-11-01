//! Integration Tests: Kinetic Momentum Scrolling, Touch Physics, and Canvas2D Engine.

use strata_layout::Rect;
use strata_render::canvas2d::Canvas2DContext;
use strata_render::compositor::{DisplayList, DrawCommand};
use strata_render::kinetic_scroll::KineticScrollController;
use strata_render::rasterizer::FrameBuffer;
use strata_style::types::Color;

#[test]
fn test_canvas2d_display_list_execution() {
    let mut fb = FrameBuffer::new(400, 300);
    let mut ctx = Canvas2DContext::new(400.0, 300.0);

    // Draw blue background rectangle
    ctx.set_fill_style(Color::rgb(15, 23, 42));
    ctx.fill_rect(0.0, 0.0, 400.0, 300.0);

    // Draw green telemetry circle
    ctx.begin_path();
    ctx.arc(200.0, 150.0, 50.0, 0.0, std::f32::consts::TAU, false);
    ctx.set_fill_style(Color::rgb(34, 197, 94));
    ctx.fill();

    let commands = ctx.finish();
    let mut dl = DisplayList::new();
    dl.push(DrawCommand::DrawCanvas2D {
        bounds: Rect::new(0.0, 0.0, 400.0, 300.0),
        commands,
    });

    fb.execute_display_list(&dl);

    // Center of circle (200, 150) must be green
    let center = fb.get_pixel(200, 150);
    assert!(center.g() > 0.5, "Center should be green: {:?}", center);

    // Corner (10, 10) must be background blue
    let corner = fb.get_pixel(10, 10);
    assert_eq!(corner, Color::rgb(15, 23, 42));
}

#[test]
fn test_canvas2d_oscilloscope_waveform_trace() {
    let mut fb = FrameBuffer::new(500, 200);
    let mut ctx = Canvas2DContext::new(500.0, 200.0);

    // Clear background
    ctx.set_fill_style(Color::rgb(10, 10, 15));
    ctx.fill_rect(0.0, 0.0, 500.0, 200.0);

    // Draw simulated sine oscilloscope signal waveform
    ctx.set_stroke_style(Color::rgb(59, 130, 246)); // Blue-500
    ctx.set_line_width(2.0);
    ctx.begin_path();

    for x in 0..500 {
        let x_f = x as f32;
        let y_f = 100.0 + (x_f * 0.05).sin() * 50.0;
        if x == 0 {
            ctx.move_to(x_f, y_f);
        } else {
            ctx.line_to(x_f, y_f);
        }
    }
    ctx.stroke();

    ctx.rasterize(&mut fb, 0.0, 0.0, None);

    // At x=0, sin(0) = 0 => y = 100
    let wave_pixel = fb.get_pixel(0, 100);
    assert!(wave_pixel.b() > 0.4, "Waveform point at (0, 100) must have blue component: {:?}", wave_pixel);

    // Far from waveform (x=0, y=10) should be dark background
    let bg_pixel = fb.get_pixel(0, 10);
    assert_eq!(bg_pixel, Color::rgb(10, 10, 15));
}

#[test]
fn test_kinetic_momentum_and_overscroll_recovery() {
    let mut scroll = KineticScrollController::new(400.0, 800.0, 400.0, 3000.0);
    assert_eq!(scroll.scroll_offset(), (0.0, 0.0));

    // Pointer down at t=0.0
    scroll.on_pointer_down(0.0);

    // Drag upwards by 400px at t=0.1
    scroll.on_pointer_move(0.0, 400.0, 0.1);
    assert_eq!(scroll.scroll_offset().1, 400.0);

    // Release with strong fling velocity (2500 px/s) at t=0.12
    scroll.on_pointer_up(0.0, 2500.0, 0.12);
    assert!(scroll.is_animating());

    // Advance 60 FPS frames until coasting completes or reaches bottom overscroll
    for _ in 0..120 {
        scroll.tick(0.016);
    }

    // Must be in valid scroll range [0, 2200]
    let (ox, oy) = scroll.scroll_offset();
    assert_eq!(ox, 0.0);
    assert!(oy > 400.0, "Should have coasted forward: {}", oy);
    assert!(oy <= 2200.0, "Should not exceed max content scroll: {}", oy);
}
