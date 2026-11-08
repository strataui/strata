//! Integration Tests: 2D Stacking Context Compositor, SDF Rasterizer, 3D Linear Algebra, and Viewport Engine.

#![allow(clippy::field_reassign_with_default)]

use strata_layout::{LayoutGeometry, LayoutNode, Rect};
use strata_primitives::geom::EdgeInsets;
use strata_render::prelude::*;
use strata_style::types::{Color, ComputedStyle, Position};

#[test]
fn test_mat4_linear_algebra_and_projections() {
    // 1. Identity
    let ident = Mat4::IDENTITY;
    let prod = ident.multiply(&ident);
    assert_eq!(ident, prod);

    // 2. Translation
    let t = Mat4::from_translation(Vec3::new(10.0, -5.0, 30.0));
    let p0 = t.transform_point(Vec3::ZERO);
    assert_eq!(p0, Vec3::new(10.0, -5.0, 30.0));

    // 3. Scaling
    let s = Mat4::from_scale(Vec3::new(2.0, 3.0, 4.0));
    let p1 = s.transform_point(Vec3::new(1.0, 1.0, 1.0));
    assert_eq!(p1, Vec3::new(2.0, 3.0, 4.0));

    // 4. Matrix Inversion
    let trs = Mat4::from_trs(
        Vec3::new(5.0, 2.0, -10.0),
        Vec3::new(0.1, 0.2, 0.3),
        Vec3::new(2.0, 2.0, 2.0),
    );
    let inv = trs.inverse().expect("Matrix should be invertible");
    let identity_approx = trs.multiply(&inv);

    for (a, b) in identity_approx.m.iter().zip(Mat4::IDENTITY.m.iter()) {
        assert!((a - b).abs() < 1e-4, "Mismatch: {} vs {}", a, b);
    }

    // 5. Perspective Projection
    let proj = Mat4::perspective(std::f32::consts::FRAC_PI_2, 16.0 / 9.0, 0.1, 100.0);
    assert!(proj.m[0] > 0.0);
    assert!(proj.m[5] > 0.0);
    assert_eq!(proj.m[11], -1.0);
}

#[test]
fn test_geometry3d_parametric_generators() {
    // 1. Box Geometry: 24 vertices, 36 indices
    let box_mesh = generate_box(2.0, 4.0, 6.0);
    assert_eq!(box_mesh.vertex_count(), 24);
    assert_eq!(box_mesh.indices.len(), 36);
    assert_eq!(box_mesh.triangle_count(), 12);

    // Verify all normals have unit length
    for v in &box_mesh.vertices {
        assert!((v.normal.length() - 1.0).abs() < 1e-5);
    }

    // 2. Sphere Geometry
    let sphere_mesh = generate_sphere(1.5, 16, 8);
    assert!(sphere_mesh.vertex_count() > 50);
    assert!(sphere_mesh.triangle_count() > 50);
    for v in &sphere_mesh.vertices {
        assert!((v.normal.length() - 1.0).abs() < 1e-4);
    }

    // 3. Plane Geometry
    let plane_mesh = generate_plane(10.0, 10.0, 2, 2);
    assert_eq!(plane_mesh.vertex_count(), 9); // (2 + 1) * (2 + 1)
    assert_eq!(plane_mesh.triangle_count(), 8); // 2 * 2 * 2
}

#[test]
fn test_screen_raycast_unproject() {
    let eye = Vec3::new(0.0, 0.0, 10.0);
    let target = Vec3::ZERO;
    let up = Vec3::UP;

    let view = Mat4::look_at(eye, target, up);
    let proj = Mat4::perspective(std::f32::consts::FRAC_PI_3, 1.0, 0.1, 1000.0);

    let inv_view = view.inverse().expect("View invertible");
    let inv_proj = proj.inverse().expect("Proj invertible");

    // Unproject center of screen (400, 300 on an 800x600 viewport)
    let ray = Ray3D::unproject(400.0, 300.0, 800.0, 600.0, &inv_proj, &inv_view);

    // Ray origin must be at the eye position
    assert!((ray.origin.x - eye.x).abs() < 1e-3);
    assert!((ray.origin.y - eye.y).abs() < 1e-3);
    assert!((ray.origin.z - eye.z).abs() < 1e-3);

    // Center ray must point directly down -Z axis
    assert!((ray.direction.x).abs() < 1e-3);
    assert!((ray.direction.y).abs() < 1e-3);
    assert!((ray.direction.z - (-1.0)).abs() < 1e-3);
}

#[test]
fn test_css_stacking_context_painting_order() {
    // Root container
    let mut root_style = ComputedStyle::default();
    root_style.background_color = Some(Color::rgb(20, 20, 20));

    // Child 1: z-index = 10 (positive layer 7)
    let mut child1_style = ComputedStyle::default();
    child1_style.position = Position::Relative;
    child1_style.z_index = Some(10);
    child1_style.background_color = Some(Color::rgb(255, 0, 0));

    // Child 2: z-index = -5 (negative layer 2)
    let mut child2_style = ComputedStyle::default();
    child2_style.position = Position::Relative;
    child2_style.z_index = Some(-5);
    child2_style.background_color = Some(Color::rgb(0, 0, 255));

    // Child 3: in-flow static block (layer 3)
    let mut child3_style = ComputedStyle::default();
    child3_style.position = Position::Static;
    child3_style.background_color = Some(Color::rgb(0, 255, 0));

    let c1 = LayoutNode::new(child1_style);
    let c2 = LayoutNode::new(child2_style);
    let c3 = LayoutNode::new(child3_style);

    let mut root = LayoutNode::with_children(root_style, vec![c1, c2, c3]);
    root.geometry = LayoutGeometry::from_border_box(
        Rect::new(0.0, 0.0, 500.0, 500.0),
        EdgeInsets::ZERO,
        EdgeInsets::ZERO,
        EdgeInsets::ZERO,
    );

    let display_list = Compositor::build_display_list(&root);

    // Filter draw commands by background color to inspect paint order
    let colors: Vec<Color> = display_list
        .commands()
        .iter()
        .filter_map(|cmd| match cmd {
            DrawCommand::DrawRect { color, .. } => Some(*color),
            DrawCommand::DrawRoundedRect { color, .. } => Some(*color),
            _ => None,
        })
        .collect();

    // Expected CSS 2.1 Appendix E order:
    // 1. Root background (Color(20, 20, 20))
    // 2. Negative z-index (Child 2: Blue)
    // 3. In-flow block (Child 3: Green)
    // 4. Positive z-index (Child 1: Red)
    assert_eq!(colors.len(), 4);
    assert_eq!(colors[0], Color::rgb(20, 20, 20)); // Layer 1 (Root)
    assert_eq!(colors[1], Color::rgb(0, 0, 255));   // Layer 2 (z-index: -5)
    assert_eq!(colors[2], Color::rgb(0, 255, 0));   // Layer 3 (In-flow Block)
    assert_eq!(colors[3], Color::rgb(255, 0, 0));   // Layer 7 (z-index: 10)
}

#[test]
fn test_analytical_sdf_rounded_rectangle() {
    let half_w = 50.0;
    let half_h = 30.0;
    let radius = 10.0;

    // Center point (0, 0): inside
    let d_center = strata_render::rasterizer::sd_rounded_rect(0.0, 0.0, half_w, half_h, radius);
    assert!(d_center < 0.0);
    assert_eq!(strata_render::rasterizer::sdf_coverage(d_center), 1.0);

    // Far outside (100, 100): outside
    let d_far = strata_render::rasterizer::sd_rounded_rect(100.0, 100.0, half_w, half_h, radius);
    assert!(d_far > 0.0);
    assert_eq!(strata_render::rasterizer::sdf_coverage(d_far), 0.0);

    // Exact corner apex (50, 30): corner rounded off, so apex is outside
    let d_corner_apex = strata_render::rasterizer::sd_rounded_rect(50.0, 30.0, half_w, half_h, radius);
    assert!(d_corner_apex > 0.0); // Apex cut by fillet radius
}

#[test]
fn test_framebuffer_2d_rasterization_and_alpha_blending() {
    let mut fb = FrameBuffer::new(50, 50);

    // Clear to white
    fb.clear(Color::WHITE);
    assert_eq!(fb.get_pixel(10, 10), Color::WHITE);

    // Draw opaque red rect from (10, 10) to (30, 30)
    fb.draw_rect(Rect::new(10.0, 10.0, 20.0, 20.0), Color::RED, None);
    assert_eq!(fb.get_pixel(15, 15), Color::RED);
    assert_eq!(fb.get_pixel(5, 5), Color::WHITE); // Outside remains white

    // Draw 50% semi-transparent black rect over the same area
    let semi_black = Color::rgba(0, 0, 0, 128); // a ~ 0.5
    fb.draw_rect(Rect::new(10.0, 10.0, 20.0, 20.0), semi_black, None);

    let blended = fb.get_pixel(15, 15);
    // Red (255) blended with 50% Black (0) -> approx 127
    assert!((blended.r() - 0.5).abs() < 0.05);
    assert_eq!(blended.g(), 0.0);
    assert_eq!(blended.b(), 0.0);
}

#[test]
fn test_3d_viewport_compositing_into_framebuffer() {
    let mut fb = FrameBuffer::new(100, 100);
    fb.clear(Color::BLACK);

    // Create a 3D scene with a red box mesh and directional light
    let mut scene = Scene3D::new();
    scene.camera = Camera3D::Perspective {
        fov_horizontal_deg: 60.0,
        near: 0.1,
        far: 100.0,
        position: [0.0, 0.0, 5.0],
        target: [0.0, 0.0, 0.0],
    };
    scene.add_light(Light3D::Directional {
        position: [0.0, 1.0, 1.0],
        color: Color::WHITE,
        intensity: 0.9,
        cast_shadow: false,
    });
    scene.add_light(Light3D::Ambient {
        color: Color::WHITE,
        intensity: 0.3,
    });
    scene.add_mesh(Mesh3D::new(
        Geometry3D::Box {
            width: 2.0,
            height: 2.0,
            depth: 2.0,
        },
        Material3D::Standard {
            color: Color::RED,
            roughness: 0.5,
            metalness: 0.0,
            emissive: None,
        },
    ));

    // Render viewport occupying (10, 10, 80, 80)
    let viewport = Rect::new(10.0, 10.0, 80.0, 80.0);
    strata_render::rasterizer3d::render_scene_3d(&mut fb, viewport, &scene, None);

    // Center of viewport (x=50, y=50) must hit the red box!
    let center_pixel = fb.get_pixel(50, 50);
    assert!(center_pixel.r() > 0.3, "Center should have red component: {:?}", center_pixel);
    assert_eq!(center_pixel.g(), 0.0);
    assert_eq!(center_pixel.b(), 0.0);

    // Pixel far outside the box (e.g. at 12, 12, top-left background of viewport)
    let bg_pixel = fb.get_pixel(12, 12);
    assert_eq!(bg_pixel, Color::BLACK);
}

#[test]
fn test_dirty_rect_damaged_rasterization() {
    let mut dl = DisplayList::new();

    // Background
    dl.push(DrawCommand::DrawRect {
        bounds: Rect::new(0.0, 0.0, 500.0, 500.0),
        color: Color::rgb(15, 23, 42),
    });

    // Card 1 (Damage target)
    dl.push(DrawCommand::DrawRect {
        bounds: Rect::new(20.0, 20.0, 100.0, 100.0),
        color: Color::rgb(59, 130, 246),
    });

    // Card 2 (Unrelated element)
    dl.push(DrawCommand::DrawRect {
        bounds: Rect::new(300.0, 300.0, 100.0, 100.0),
        color: Color::rgb(34, 197, 94),
    });

    // Full frame render
    let mut fb_full = FrameBuffer::new(500, 500);
    fb_full.execute_display_list(&dl);

    // Damaged render: start with empty buffer, render only Card 1 damage
    let mut fb_damaged = FrameBuffer::new(500, 500);
    let damage_rect = Rect::new(20.0, 20.0, 100.0, 100.0);
    fb_damaged.execute_display_list_damaged(&dl, damage_rect);

    // 1. Pixels inside damage rect must be pixel-for-pixel identical to full render
    for y in 20..120 {
        for x in 20..120 {
            assert_eq!(
                fb_damaged.get_pixel(x, y),
                fb_full.get_pixel(x, y),
                "Pixel mismatch at ({}, {})",
                x,
                y
            );
        }
    }

    // 2. Pixels in Card 2 (300..400) must NOT have been drawn in fb_damaged
    assert_eq!(fb_full.get_pixel(350, 350), Color::rgb(34, 197, 94));
    assert_eq!(fb_damaged.get_pixel(350, 350), Color::TRANSPARENT);
}

#[test]
fn test_framebuffer_clear_rect() {
    let mut fb = FrameBuffer::new(200, 200);
    fb.clear(Color::WHITE);

    fb.clear_rect(Rect::new(50.0, 50.0, 50.0, 50.0), Color::RED);

    // Inside cleared rect
    assert_eq!(fb.get_pixel(60, 60), Color::RED);
    assert_eq!(fb.get_pixel(99, 99), Color::RED);

    // Outside cleared rect
    assert_eq!(fb.get_pixel(40, 40), Color::WHITE);
    assert_eq!(fb.get_pixel(120, 120), Color::WHITE);
}

#[test]
fn test_framebuffer_draw_text() {
    let mut fb = FrameBuffer::new(200, 100);
    fb.clear(Color::BLACK);

    fb.draw_text((10.0, 10.0), "Hello Strata", 16.0, Color::WHITE, None);

    let mut white_count = 0;
    for y in 10..30 {
        for x in 10..150 {
            if fb.get_pixel(x, y) == Color::WHITE {
                white_count += 1;
            }
        }
    }
    assert!(white_count > 20, "Text should have rasterized pixels, found {}", white_count);
}

#[test]
fn test_display_list_spatial_viewport_culling() {
    use strata_style::types::{ComputedStyle, Display, Dimension, Position};
    use strata_render::compositor::Compositor;

    // Create a long root container with 100 vertical child cards
    let mut root_style = ComputedStyle::default();
    root_style.display = Display::Block;
    root_style.width = Dimension::Px(1000.0);
    let mut root = LayoutNode::new(root_style);
    root.geometry.border_box = Rect::new(0.0, 0.0, 1000.0, 10000.0);

    // Fixed header navbar at y=0, height=60
    let mut fixed_style = ComputedStyle::default();
    fixed_style.display = Display::Block;
    fixed_style.position = Position::Fixed;
    fixed_style.background_color = Some(Color::rgb(20, 20, 20));
    let mut fixed_node = LayoutNode::new(fixed_style);
    fixed_node.geometry.border_box = Rect::new(0.0, 0.0, 1000.0, 60.0);
    root.children.push(fixed_node);

    // 100 in-flow static cards, each 100px high, spaced 100px apart (y = 0, 100, 200, ... 9900)
    for i in 0..100 {
        let mut card_style = ComputedStyle::default();
        card_style.display = Display::Block;
        card_style.position = Position::Static;
        card_style.background_color = Some(Color::rgb((i % 255) as u8, 100, 150));
        let mut card = LayoutNode::new(card_style);
        card.geometry.border_box = Rect::new(0.0, (i as f32) * 100.0, 1000.0, 90.0);
        root.children.push(card);
    }

    // 1. Unculled display list: builds draw commands for ALL 101 nodes
    let dl_full = Compositor::build_display_list(&root);
    assert!(dl_full.commands.len() >= 101, "Full display list should contain all 101 commands, got {}", dl_full.commands.len());

    // 2. Viewport-culled display list at y=5000 (viewport: 5000..5800, buffer: 3800..7000)
    let viewport = Rect::new(0.0, 5000.0, 1000.0, 800.0);
    let dl_culled = Compositor::build_display_list_with_viewport(&root, Some(viewport));

    // Cards far above (e.g. y=0..3500) and far below (e.g. y=7500..9900) must be culled
    assert!(dl_culled.commands.len() < 40, "Culled display list should contain <40 commands, got {}", dl_culled.commands.len());

    // Fixed header navbar (position: fixed) must ALWAYS be preserved
    let has_fixed_nav = dl_culled.commands.iter().any(|cmd| {
        match cmd {
            strata_render::compositor::DrawCommand::DrawRect { bounds, color } => {
                bounds.y == 0.0 && *color == Color::rgb(20, 20, 20)
            }
            _ => false,
        }
    });
    assert!(has_fixed_nav, "Fixed header navbar must never be culled");
}

#[test]
fn test_content_visibility_hidden_and_contain_intrinsic_size() {
    use strata_style::types::{ComputedStyle, ContentVisibility, Dimension, Display};

    let mut style = ComputedStyle::default();
    style.display = Display::Block;
    style.content_visibility = ContentVisibility::Hidden;
    style.contain_intrinsic_width = Some(Dimension::Px(800.0));
    style.contain_intrinsic_height = Some(Dimension::Px(450.0));

    let mut node = LayoutNode::new(style);
    // Add child that should be completely skipped by layout
    let mut child_style = ComputedStyle::default();
    child_style.display = Display::Block;
    child_style.height = Dimension::Px(2000.0);
    node.children.push(LayoutNode::new(child_style));

    node.compute_layout(1000.0, 1000.0);

    // Height must match contain_intrinsic_height (450px) instead of child height (2000px)
    assert_eq!(node.geometry.border_box.height, 450.0);
    assert_eq!(node.geometry.border_box.width, 800.0);
}

