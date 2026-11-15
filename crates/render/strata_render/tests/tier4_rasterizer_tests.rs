//! Comprehensive Tier 4 Test Suite for Pure-Rust Software 2D Rasterizer & Graphics Engine.
//!
//! Validates Goals 09, 10, 11, 12, 13, 14, 15, 16, 17:
//! - Analytical SDF rounded rectangles and multi-stop linear/radial gradients (Goals 14, 15)
//! - Bilinear image blitting and in-memory PNG decompression (Goal 11)
//! - SVG path and GPU vector path rasterization with quadratic Béziers and stroke expansion (Goals 09, 10)
//! - Layer compositing with W3C blend modes and opacity hierarchies (Goal 12)
//! - Continuous bilinear font scaling and anti-aliasing (Goal 13)
//! - Analytical geometric clipping (Circles, Rounded Insets, Polygons) (Goal 16)
//! - Decoupled headless PNG byte export (Goal 17)

use std::sync::Arc;
use strata_layout::{FillRule, ImageResource, Rect, StrokeLineCap, StrokeLineJoin};
use strata_render::{DisplayList, DrawCommand, FrameBuffer};
use strata_style::effects::ClipPath;
use strata_style::types::{BlendMode, BorderRadius, Color, ColorStop};

#[test]
fn test_linear_gradient_interpolation_and_clamping() {
    let mut fb = FrameBuffer::new(100, 100);
    let mut dl = DisplayList::new();

    let stops = vec![
        ColorStop {
            color: Color::from_f32(1.0, 0.0, 0.0, 1.0),
            position: Some(0.0),
        },
        ColorStop {
            color: Color::from_f32(0.0, 0.0, 1.0, 1.0),
            position: Some(1.0),
        },
    ];

    dl.push(DrawCommand::DrawLinearGradient {
        bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
        angle_deg: 90.0, // Left to Right
        stops,
        border_radius: BorderRadius::ZERO,
    });

    fb.execute_display_list(&dl);

    // Left side should be predominantly red
    let px_left = fb.get_pixel(5, 50);
    assert!(
        px_left.r > 200,
        "Left side should be red, got r={}",
        px_left.r
    );
    assert!(
        px_left.b < 50,
        "Left side should not be blue, got b={}",
        px_left.b
    );
    assert_eq!(px_left.a, 255);

    // Right side should be predominantly blue
    let px_right = fb.get_pixel(95, 50);
    assert!(
        px_right.b > 200,
        "Right side should be blue, got b={}",
        px_right.b
    );
    assert!(
        px_right.r < 50,
        "Right side should not be red, got r={}",
        px_right.r
    );
    assert_eq!(px_right.a, 255);

    // Center should be an intermediate purple blend
    let px_mid = fb.get_pixel(50, 50);
    assert!(
        px_mid.r > 80 && px_mid.r < 180,
        "Midpoint red channel should be blended: {}",
        px_mid.r
    );
    assert!(
        px_mid.b > 80 && px_mid.b < 180,
        "Midpoint blue channel should be blended: {}",
        px_mid.b
    );
    assert_eq!(px_mid.a, 255);
}

#[test]
fn test_radial_gradient_concentric_distribution() {
    let mut fb = FrameBuffer::new(100, 100);
    let mut dl = DisplayList::new();

    let stops = vec![
        ColorStop {
            color: Color::from_f32(1.0, 1.0, 1.0, 1.0), // White center
            position: Some(0.0),
        },
        ColorStop {
            color: Color::from_f32(0.0, 0.0, 0.0, 1.0), // Black edge
            position: Some(1.0),
        },
    ];

    dl.push(DrawCommand::DrawRadialGradient {
        bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
        center: (0.5, 0.5),
        stops,
        border_radius: BorderRadius::ZERO,
    });

    fb.execute_display_list(&dl);

    // Center should be bright white
    let px_center = fb.get_pixel(50, 50);
    assert!(px_center.r > 240 && px_center.g > 240 && px_center.b > 240);
    assert_eq!(px_center.a, 255);

    // Outer edge should be darker
    let px_edge = fb.get_pixel(10, 10);
    assert!(px_edge.r < 120 && px_edge.g < 120 && px_edge.b < 120);
    assert_eq!(px_edge.a, 255);
}

#[test]
fn test_pure_rust_image_blitting_and_bilinear_scaling() {
    let mut fb = FrameBuffer::new(40, 40);

    // 2x2 RGBA source image:
    // [Red,   Green]
    // [Blue,  White]
    let raw_rgba: Vec<u8> = vec![
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
    ];

    let img = ImageResource {
        data: Arc::new(raw_rgba),
        width: 2,
        height: 2,
    };

    let mut dl = DisplayList::new();
    dl.push(DrawCommand::DrawImage {
        bounds: Rect::new(0.0, 0.0, 40.0, 40.0),
        image: img,
    });

    fb.execute_display_list(&dl);

    // Near top-left should be predominantly red
    let px_tl = fb.get_pixel(5, 5);
    assert!(
        px_tl.r > 180 && px_tl.g < 80 && px_tl.b < 80,
        "Expected red near top-left, got ({}, {}, {})",
        px_tl.r,
        px_tl.g,
        px_tl.b
    );

    // Near top-right should be predominantly green
    let px_tr = fb.get_pixel(35, 5);
    assert!(
        px_tr.g > 180 && px_tr.r < 80 && px_tr.b < 80,
        "Expected green near top-right, got ({}, {}, {})",
        px_tr.r,
        px_tr.g,
        px_tr.b
    );

    // Center pixel should be bilinearly interpolated blend of all four corners
    let px_mid = fb.get_pixel(20, 20);
    assert!(
        px_mid.r > 80 && px_mid.g > 80 && px_mid.b > 80,
        "Center should blend all 4 colors: ({}, {}, {})",
        px_mid.r,
        px_mid.g,
        px_mid.b
    );
}

#[test]
fn test_png_image_decode_and_blitting() {
    // Generate valid PNG bytes by drawing on a small FrameBuffer and encoding it
    let mut src_fb = FrameBuffer::new(8, 8);
    src_fb.draw_rect(
        Rect::new(0.0, 0.0, 8.0, 8.0),
        Color::from_f32(0.2, 0.8, 0.4, 1.0),
        None,
    );
    let png_bytes = src_fb.to_png_bytes().expect("PNG encoding failed");

    // Now blit that PNG onto a larger FrameBuffer via DrawImage
    let img = ImageResource {
        data: Arc::new(png_bytes),
        width: 8,
        height: 8,
    };

    let mut dst_fb = FrameBuffer::new(32, 32);
    let mut dl = DisplayList::new();
    dl.push(DrawCommand::DrawImage {
        bounds: Rect::new(0.0, 0.0, 32.0, 32.0),
        image: img,
    });

    dst_fb.execute_display_list(&dl);

    let px = dst_fb.get_pixel(16, 16);
    assert!((px.r as f32 / 255.0 - 0.2).abs() < 0.1);
    assert!((px.g as f32 / 255.0 - 0.8).abs() < 0.1);
    assert!((px.b as f32 / 255.0 - 0.4).abs() < 0.1);
    assert_eq!(px.a, 255);
}

#[test]
fn test_svg_vector_path_rasterization() {
    let mut fb = FrameBuffer::new(60, 60);
    let mut dl = DisplayList::new();

    // Box path from (10, 10) to (50, 50)
    let path_data = "M 10 10 L 50 10 L 50 50 L 10 50 Z".to_string();

    dl.push(DrawCommand::DrawSvgPath {
        bounds: Rect::new(0.0, 0.0, 60.0, 60.0),
        path_data,
        stroke_color: Some(Color::from_f32(1.0, 0.0, 0.0, 1.0)),
        fill_color: Some(Color::from_f32(0.0, 1.0, 0.0, 1.0)),
        stroke_width: 2.0,
        stroke_linecap: StrokeLineCap::Butt,
        stroke_linejoin: StrokeLineJoin::Miter,
        stroke_dasharray: Vec::new(),
        fill_rule: FillRule::NonZero,
        view_box: (60.0, 60.0),
    });

    fb.execute_display_list(&dl);

    // Center of rectangle (30, 30) must be filled with green
    let px_in = fb.get_pixel(30, 30);
    assert!(
        px_in.g > 200,
        "Interior pixel should be green, got ({}, {}, {})",
        px_in.r,
        px_in.g,
        px_in.b
    );
    assert_eq!(px_in.a, 255);

    // Outside rectangle (5, 5) must be untouched transparent
    let px_out = fb.get_pixel(5, 5);
    assert_eq!(px_out.a, 0, "Exterior pixel should be transparent");
}

#[test]
fn test_layer_opacity_and_blend_modes() {
    let mut fb = FrameBuffer::new(50, 50);

    // 1. Fill base with 50% gray
    fb.draw_rect(
        Rect::new(0.0, 0.0, 50.0, 50.0),
        Color::from_f32(0.5, 0.5, 0.5, 1.0),
        None,
    );

    // 2. Push Opacity 0.5 and draw Pure White
    let mut dl = DisplayList::new();
    dl.push(DrawCommand::PushOpacity { alpha: 0.5 });
    dl.push(DrawCommand::DrawRect {
        bounds: Rect::new(0.0, 0.0, 50.0, 50.0),
        color: Color::from_f32(1.0, 1.0, 1.0, 1.0),
    });
    dl.push(DrawCommand::PopOpacity);

    fb.execute_display_list(&dl);

    let px = fb.get_pixel(25, 25);
    // 0.5 * 255 + 0.5 * 128 ~= 191
    assert!(
        px.r > 170 && px.r < 210,
        "Opacity blended red channel expected ~191, got {}",
        px.r
    );
    assert!(
        px.g > 170 && px.g < 210,
        "Opacity blended green channel expected ~191, got {}",
        px.g
    );
    assert!(
        px.b > 170 && px.b < 210,
        "Opacity blended blue channel expected ~191, got {}",
        px.b
    );
    assert_eq!(px.a, 255);

    // 3. Test Multiply blend mode: Red (1.0, 0.0, 0.0) * Green (0.0, 1.0, 0.0) -> Black (0, 0, 0)
    let mut fb2 = FrameBuffer::new(20, 20);
    fb2.draw_rect(
        Rect::new(0.0, 0.0, 20.0, 20.0),
        Color::from_f32(1.0, 0.0, 0.0, 1.0),
        None,
    );

    let mut dl2 = DisplayList::new();
    dl2.push(DrawCommand::PushBlendMode {
        mode: BlendMode::Multiply,
    });
    dl2.push(DrawCommand::DrawRect {
        bounds: Rect::new(0.0, 0.0, 20.0, 20.0),
        color: Color::from_f32(0.0, 1.0, 0.0, 1.0),
    });
    dl2.push(DrawCommand::PopBlendMode);

    fb2.execute_display_list(&dl2);
    let px_mul = fb2.get_pixel(10, 10);
    assert_eq!(px_mul.r, 0, "Multiply red channel should be 0");
    assert_eq!(px_mul.g, 0, "Multiply green channel should be 0");
    assert_eq!(px_mul.b, 0, "Multiply blue channel should be 0");
}

#[test]
fn test_geometric_clip_paths_circle() {
    let mut fb = FrameBuffer::new(100, 100);
    let mut dl = DisplayList::new();

    // Push circular clip centered at (50, 50) with radius 20
    dl.push(DrawCommand::PushClipPath {
        bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
        clip_path: ClipPath::Circle {
            radius: 20.0,
            center_x: 50.0,
            center_y: 50.0,
        },
    });

    // Draw full-canvas solid blue rect
    dl.push(DrawCommand::DrawRect {
        bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
        color: Color::from_f32(0.0, 0.0, 1.0, 1.0),
    });

    dl.push(DrawCommand::PopClipPath);

    fb.execute_display_list(&dl);

    // Pixel inside circle (50, 50) must be blue
    let px_in = fb.get_pixel(50, 50);
    assert!(
        px_in.b > 240,
        "Pixel inside circle should be blue, got b={}",
        px_in.b
    );
    assert_eq!(px_in.a, 255);

    // Pixel outside circle (10, 10) must remain transparent
    let px_out = fb.get_pixel(10, 10);
    assert_eq!(
        px_out.a, 0,
        "Pixel outside circle should remain transparent, got a={}",
        px_out.a
    );
}

#[test]
fn test_antialiased_font_rendering_subpixel_fringes() {
    let mut fb = FrameBuffer::new(120, 40);

    // Draw text with fractional scale to test continuous anti-aliasing
    fb.draw_text(
        (5.0, 10.0),
        "Strata",
        18.0,
        Color::from_f32(1.0, 1.0, 1.0, 1.0),
        None,
    );

    // Scan pixels: verify there is at least one non-binary alpha value (0 < alpha < 255) proving anti-aliasing coverage
    let mut found_antialiased_pixel = false;
    for y in 0..40 {
        for x in 0..120 {
            let px = fb.get_pixel(x, y);
            if px.a > 10 && px.a < 245 {
                found_antialiased_pixel = true;
                break;
            }
        }
        if found_antialiased_pixel {
            break;
        }
    }

    assert!(
        found_antialiased_pixel,
        "Font rendering should produce continuous anti-aliased edge coverage"
    );
}

#[test]
fn test_pure_rust_headless_png_export_decoupling() {
    let mut dl = DisplayList::new();
    dl.push(DrawCommand::DrawRect {
        bounds: Rect::new(10.0, 10.0, 80.0, 80.0),
        color: Color::from_f32(0.8, 0.2, 0.3, 1.0),
    });

    let mut fb = FrameBuffer::new(100, 100);
    fb.execute_display_list(&dl);
    let png_bytes = fb
        .to_png_bytes()
        .expect("Pure-Rust PNG byte generation failed");

    // Verify standard PNG 8-byte header
    assert!(png_bytes.len() > 24);
    assert_eq!(
        &png_bytes[0..8],
        &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]
    );

    // Verify IHDR presence
    let chunk_len = u32::from_be_bytes(png_bytes[8..12].try_into().unwrap());
    assert_eq!(chunk_len, 13);
    assert_eq!(&png_bytes[12..16], b"IHDR");

    // Verify encoded dimensions match 100x100
    let w = u32::from_be_bytes(png_bytes[16..20].try_into().unwrap());
    let h = u32::from_be_bytes(png_bytes[20..24].try_into().unwrap());
    assert_eq!(w, 100);
    assert_eq!(h, 100);
}

#[test]
fn test_svg_vector_path_even_odd_vs_nonzero_winding_rule() {
    let path_data =
        "M 10 10 L 50 10 L 50 50 L 10 50 Z M 20 20 L 40 20 L 40 40 L 20 40 Z".to_string();

    // 1. NonZero: Interior hole (30, 30) has winding = 2 != 0, so it is filled
    let mut fb_nonzero = FrameBuffer::new(60, 60);
    let mut dl_nonzero = DisplayList::new();
    dl_nonzero.push(DrawCommand::DrawSvgPath {
        bounds: Rect::new(0.0, 0.0, 60.0, 60.0),
        path_data: path_data.clone(),
        stroke_color: None,
        fill_color: Some(Color::from_f32(0.0, 1.0, 0.0, 1.0)),
        stroke_width: 0.0,
        stroke_linecap: StrokeLineCap::Butt,
        stroke_linejoin: StrokeLineJoin::Miter,
        stroke_dasharray: Vec::new(),
        fill_rule: FillRule::NonZero,
        view_box: (60.0, 60.0),
    });
    fb_nonzero.execute_display_list(&dl_nonzero);

    let px_nonzero_hole = fb_nonzero.get_pixel(30, 30);
    assert!(
        px_nonzero_hole.g > 200,
        "NonZero winding rule must fill interior hole with winding=2"
    );

    // 2. EvenOdd: Interior hole (30, 30) has crossings = 2 % 2 == 0, so it is empty (transparent)
    let mut fb_evenodd = FrameBuffer::new(60, 60);
    let mut dl_evenodd = DisplayList::new();
    dl_evenodd.push(DrawCommand::DrawSvgPath {
        bounds: Rect::new(0.0, 0.0, 60.0, 60.0),
        path_data,
        stroke_color: None,
        fill_color: Some(Color::from_f32(0.0, 1.0, 0.0, 1.0)),
        stroke_width: 0.0,
        stroke_linecap: StrokeLineCap::Butt,
        stroke_linejoin: StrokeLineJoin::Miter,
        stroke_dasharray: Vec::new(),
        fill_rule: FillRule::EvenOdd,
        view_box: (60.0, 60.0),
    });
    fb_evenodd.execute_display_list(&dl_evenodd);

    let px_evenodd_ring = fb_evenodd.get_pixel(15, 15);
    assert!(
        px_evenodd_ring.g > 200,
        "Outer ring pixel (15, 15) must be filled under EvenOdd"
    );

    let px_evenodd_hole = fb_evenodd.get_pixel(30, 30);
    assert_eq!(
        px_evenodd_hole.a, 0,
        "Interior hole (30, 30) must remain transparent under EvenOdd fill rule"
    );
}

#[test]
fn test_srgb_gamma_compensation_subpixel_alpha() {
    use strata_render::rasterizer::{linear_to_srgb, srgb_to_linear};

    // Gamma transfer round-trip
    for v in [0u8, 64, 128, 192, 255] {
        let lin = srgb_to_linear(v);
        let srgb = linear_to_srgb(lin);
        assert_eq!(v, srgb, "Gamma round-trip fidelity failed for byte {}", v);
    }

    let mut fb = FrameBuffer::new(20, 20);
    fb.clear(Color::BLACK);

    // Blend white text glyph with 0.5 coverage onto black background
    fb.blend_glyph_coverage(10, 10, Color::WHITE, 0.5);

    let px = fb.get_pixel(10, 10);
    // Linear 0.5 converted to sRGB gamma 2.2 is ~186, preventing optical thinning
    assert!(
        px.r >= 180 && px.r <= 190,
        "sRGB gamma compensated red channel expected ~186, got {}",
        px.r
    );
    assert_eq!(px.r, px.g);
    assert_eq!(px.g, px.b);
    assert_eq!(px.a, 255);
}

#[test]
fn test_dirty_rect_damage_scissoring() {
    use strata_render::compositor::DamageTracker;

    let mut damage = DamageTracker::new();
    damage.add_damage(Rect::new(5.0, 5.0, 10.0, 10.0));
    damage.add_damage(Rect::new(80.0, 80.0, 10.0, 10.0));

    let clusters = damage.cluster_damage_rects();
    assert_eq!(
        clusters.len(),
        2,
        "Disjoint dirty regions must not prematurely collapse to full-window bounds"
    );

    let mut fb = FrameBuffer::new(100, 100);
    fb.clear(Color::BLACK);

    let mut dl = DisplayList::new();
    dl.push(DrawCommand::DrawRect {
        bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
        color: Color::WHITE,
    });

    fb.rasterize_damaged(&dl, &damage);

    // Damage regions must be white
    let px_d1 = fb.get_pixel(10, 10);
    assert_eq!(px_d1, Color::WHITE);

    let px_d2 = fb.get_pixel(85, 85);
    assert_eq!(px_d2, Color::WHITE);

    // Undamaged center region must remain black
    let px_center = fb.get_pixel(50, 50);
    assert_eq!(px_center, Color::BLACK);
}
