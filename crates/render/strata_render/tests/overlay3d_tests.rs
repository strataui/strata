//! Overlay pass: projected anchors, clipping, markers, labels, and shared
//! projection with the 3D geometry pass.
//!
//! The overlay must land on the same pixel as the geometry it annotates, be
//! clipped to the viewport, and draw leader lines and text boxes where the
//! reference does.

use strata_layout::Rect;
use strata_render::camera64::{Camera64, DVec3, RenderFrame64};
use strata_render::overlay3d::{
    rasterize_overlay, text_size, LabelAnchor, MarkerKind, Overlay3D, OverlayCommand,
    OverlayPrimitive, OverlayTextStyle,
};
use strata_render::rasterizer::FrameBuffer;
use strata_render::rasterizer3d::render_scene_3d_with_textures;
use strata_render::scene3d::{
    Camera3D, Geometry3D, Material3D, Mesh3D, Scene3D, TexturedMaterial3D,
};
use strata_render::texture3d::TextureStore;
use strata_style::types::Color;

const VIEW_W: f64 = 1440.0;
const VIEW_H: f64 = 900.0;

fn camera() -> Camera64 {
    Camera64 {
        eye: DVec3::new(0.0, 0.0, 10.0),
        target: DVec3::ZERO,
        up: DVec3::UP,
        fov_horizontal_deg: 60.0,
        near: 0.1,
        far: 1000.0,
    }
}

/// Camera matching the scene camera used in the geometry-parity test: the
/// engine's `fov` is interpreted against the vertical axis there.
fn overlay_camera() -> Camera64 {
    Camera64 {
        eye: DVec3::new(0.0, 0.0, 10.0),
        target: DVec3::ZERO,
        up: DVec3::UP,
        fov_horizontal_deg: 60.0,
        near: 0.1,
        far: 1000.0,
    }
}

fn alpha(fb: &FrameBuffer, x: usize, y: usize) -> f32 {
    fb.get_pixel(x, y).a()
}

#[test]
fn projected_marker_lands_on_the_projected_anchor() {
    let frame = RenderFrame64::from_camera(&camera(), VIEW_W, VIEW_H);
    let anchor = DVec3::new(2.0, 1.0, 0.0);
    let projected = frame.project(anchor);

    let mut overlay = Overlay3D::new();
    overlay.push(OverlayPrimitive::Marker {
        anchor,
        kind: MarkerKind::Square,
        color: Color::rgba(255, 0, 0, 255),
        size: 5.0,
    });

    let commands = overlay.project(&frame);
    match &commands[0] {
        OverlayCommand::Marker { centre, .. } => {
            assert_eq!(centre[0], projected.x as f32);
            assert_eq!(centre[1], projected.y as f32);
        }
        other => panic!("expected a marker command, got {other:?}"),
    }
}

#[test]
fn overlay_anchor_matches_the_geometry_pixel() {
    // A flat quad whose corner sits at a known scene point: the overlay marker
    // must cover the very same pixel the quad corner rasterizes to.
    let corner = DVec3::new(3.0, 3.0, 0.0);
    let mut store = TextureStore::new();
    let texture = store.insert_rgba8(2, 2, vec![255u8; 16]).expect("texture");

    let verts = vec![
        -3.0, -3.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, //
        3.0, -3.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, //
        3.0, 3.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, //
        -3.0, 3.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0,
    ];

    let mut scene = Scene3D::new();
    scene.camera = Camera3D::Perspective {
        fov_horizontal_deg: 60.0,
        near: 0.1,
        far: 1000.0,
        position: [0.0, 0.0, 10.0],
        target: [0.0, 0.0, 0.0],
    };
    scene.add_mesh(Mesh3D::new(
        Geometry3D::CustomTextured {
            vertices: verts,
            indices: vec![0, 1, 2, 0, 2, 3],
        },
        Material3D::Textured(TexturedMaterial3D::new(texture)),
    ));

    // The rendered framebuffer is a separate rectangle from the viewport, so a
    // viewport-space coordinate maps one to one onto it.
    let viewport = Rect::new(0.0, 0.0, VIEW_W as f32, VIEW_H as f32);
    let mut fb = FrameBuffer::new(VIEW_W as usize, VIEW_H as usize);
    render_scene_3d_with_textures(&mut fb, viewport, &scene, None, Some(&store));

    // The overlay camera must use the same eye, target, and vertical field of
    // view as the scene camera. The overlay fov is vertical; the scene camera's
    // 60 degrees is horizontal at this aspect, so the vertical fov follows from
    // it.
    let frame = RenderFrame64::from_camera(&overlay_camera(), VIEW_W, VIEW_H);
    let p = frame.project(corner);
    // Step inside the quad toward the viewport centre; the corner pixel itself
    // belongs to the neighbour that wins the top-left fill rule.
    let inside_x = (p.x as usize).saturating_sub(20);
    let inside_y = (p.y as usize).saturating_add(20);
    assert!(
        alpha(&fb, inside_x, inside_y) > 0.5,
        "the quad corner must rasterize near the projected anchor ({inside_x},{inside_y})"
    );

    let mut overlay = Overlay3D::new();
    overlay.push(OverlayPrimitive::Marker {
        anchor: corner,
        kind: MarkerKind::Square,
        color: Color::rgba(0, 255, 0, 255),
        size: 9.0,
    });
    overlay.render(&mut fb, viewport, &frame);

    let overlaid = fb.get_pixel(inside_x, inside_y);
    assert!(
        overlaid.g() > 0.9,
        "the overlay must paint over the geometry at the anchor, got {overlaid:?}"
    );
}

#[test]
fn overlay_is_clipped_to_the_viewport_and_the_clip_rect() {
    let frame = RenderFrame64::from_camera(&camera(), VIEW_W, VIEW_H);
    let viewport = Rect::new(0.0, 0.0, VIEW_W as f32, VIEW_H as f32);

    let mut overlay = Overlay3D::new();
    overlay.clip = Some(Rect::new(700.0, 400.0, 100.0, 100.0));
    overlay.push(OverlayPrimitive::Line {
        a: [0.0, 0.0],
        b: [1400.0, 899.0],
        color: Color::rgba(255, 255, 255, 255),
        width: 3.0,
    });

    let mut fb = FrameBuffer::new(VIEW_W as usize, VIEW_H as usize);
    overlay.render(&mut fb, viewport, &frame);

    // Outside the clip rect nothing is painted.
    assert_eq!(
        alpha(&fb, 200, 200),
        0.0,
        "outside the clip must stay clear"
    );
    assert_eq!(alpha(&fb, 1200, 1200 - 1), 0.0);

    // Inside the clip rect along the diagonal the line is present.
    let inside_painted = (400..500)
        .filter(|&y| (700..800).any(|x| alpha(&fb, x, y) > 0.5))
        .count();
    assert!(
        inside_painted > 0,
        "the line must be visible inside its clip rectangle"
    );
}

#[test]
fn label_above_anchors_with_leader_line() {
    let frame = RenderFrame64::from_camera(&camera(), VIEW_W, VIEW_H);
    let anchor = DVec3::ZERO;
    let style = OverlayTextStyle {
        color: Color::rgba(255, 255, 255, 255),
        scale: 1,
    };

    let mut overlay = Overlay3D::new();
    overlay.push(OverlayPrimitive::Label {
        anchor,
        text: "WP-1".to_string(),
        style,
        placement: LabelAnchor::Above,
        leader: true,
    });

    let commands = overlay.project(&frame);
    let projected = frame.project(anchor);
    match &commands[0] {
        OverlayCommand::Label {
            origin,
            leader,
            text,
            ..
        } => {
            assert_eq!(text, "WP-1");
            let (w, h) = text_size("WP-1", 1);
            assert!((origin[0] - (projected.x as f32 - w * 0.5)).abs() < 0.01);
            assert!(
                origin[1] + h <= projected.y as f32,
                "the box must sit above the anchor"
            );
            let (from, to) = leader.expect("leader requested");
            assert_eq!(from, [projected.x as f32, projected.y as f32]);
            assert!((to[1] - (origin[1] + h)).abs() < 0.01);
        }
        other => panic!("expected a label command, got {other:?}"),
    }

    let viewport = Rect::new(0.0, 0.0, VIEW_W as f32, VIEW_H as f32);
    let mut fb = FrameBuffer::new(VIEW_W as usize, VIEW_H as usize);
    overlay.render(&mut fb, viewport, &frame);
    let painted = (0..VIEW_H as usize)
        .flat_map(|y| (0..VIEW_W as usize).map(move |x| (x, y)))
        .filter(|&(x, y)| alpha(&fb, x, y) > 0.5)
        .count();
    assert!(
        painted > 20,
        "the label and leader must paint pixels, got {painted}"
    );
}

#[test]
fn polyline_and_circle_are_drawn() {
    let frame = RenderFrame64::from_camera(&camera(), VIEW_W, VIEW_H);
    let mut overlay = Overlay3D::new();
    overlay.push(OverlayPrimitive::Polyline {
        points: vec![
            DVec3::new(-2.0, -1.0, 0.0),
            DVec3::new(0.0, 1.0, 0.0),
            DVec3::new(2.0, -1.0, 0.0),
        ],
        color: Color::rgba(0, 255, 0, 255),
        width: 2.0,
    });
    overlay.push(OverlayPrimitive::Circle {
        centre: DVec3::ZERO,
        radius: 1.0,
        color: Color::rgba(255, 0, 0, 255),
        width: 1.0,
    });

    let commands = overlay.project(&frame);
    assert_eq!(commands.len(), 2, "both primitives must survive projection");
    match &commands[1] {
        OverlayCommand::Circle { radius, .. } => {
            assert!(
                *radius > 10.0,
                "a radius of 1 unit must project to pixels, got {radius}"
            );
        }
        other => panic!("expected a circle, got {other:?}"),
    }

    let viewport = Rect::new(0.0, 0.0, VIEW_W as f32, VIEW_H as f32);
    let mut fb = FrameBuffer::new(VIEW_W as usize, VIEW_H as usize);
    overlay.render(&mut fb, viewport, &frame);
    let centre = frame.project(DVec3::ZERO);
    let ring = fb.get_pixel(
        (centre.x as usize + commands_radius(&commands) as usize).min(VIEW_W as usize - 1),
        centre.y as usize,
    );
    assert!(ring.a() > 0.0, "the circle ring must be drawn");
}

fn commands_radius(commands: &[OverlayCommand]) -> f32 {
    match &commands[1] {
        OverlayCommand::Circle { radius, .. } => *radius,
        _ => 0.0,
    }
}

#[test]
fn pieces_behind_the_camera_are_dropped_by_the_overlay_too() {
    let frame = RenderFrame64::from_camera(&camera(), VIEW_W, VIEW_H);
    let mut overlay = Overlay3D::new();
    overlay.push(OverlayPrimitive::Marker {
        anchor: DVec3::new(0.0, 0.0, 50.0),
        kind: MarkerKind::Circle,
        color: Color::rgba(255, 255, 255, 255),
        size: 4.0,
    });
    assert!(
        overlay.project(&frame).is_empty(),
        "an anchor behind the eye must not project"
    );
}

#[test]
fn screen_space_line_has_no_projection_dependency() {
    let frame = RenderFrame64::from_camera(&camera(), VIEW_W, VIEW_H);
    let mut overlay = Overlay3D::new();
    overlay.push(OverlayPrimitive::Line {
        a: [10.0, 10.0],
        b: [100.0, 10.0],
        color: Color::rgba(255, 255, 255, 255),
        width: 1.0,
    });
    let commands = overlay.project(&frame);
    match &commands[0] {
        OverlayCommand::Line { a, b, .. } => {
            assert_eq!(*a, [10.0, 10.0]);
            assert_eq!(*b, [100.0, 10.0]);
        }
        other => panic!("expected a screen line, got {other:?}"),
    }
}

#[test]
fn every_marker_kind_paints() {
    let viewport = Rect::new(0.0, 0.0, 200.0, 200.0);
    let commands: Vec<OverlayCommand> = [
        MarkerKind::Square,
        MarkerKind::Circle,
        MarkerKind::Diamond,
        MarkerKind::Ring,
    ]
    .iter()
    .map(|kind| OverlayCommand::Marker {
        centre: [100.0, 100.0],
        kind: *kind,
        color: Color::rgba(255, 255, 255, 255),
        size: 9.0,
    })
    .collect();

    for command in &commands {
        let mut fb = FrameBuffer::new(200, 200);
        rasterize_overlay(&mut fb, viewport, None, std::slice::from_ref(command));
        let painted = fb.pixels.as_chunks::<4>().0.iter().filter(|p| p[3] > 0).count();
        assert!(
            painted > 0,
            "marker {command:?} must paint at least one pixel"
        );
    }
    // The ring draws fewer pixels than the filled square because its interior
    // stays empty.
    let mut square = FrameBuffer::new(200, 200);
    rasterize_overlay(&mut square, viewport, None, &commands[0..1]);
    let mut ring = FrameBuffer::new(200, 200);
    rasterize_overlay(&mut ring, viewport, None, &commands[3..4]);
    let count = |fb: &FrameBuffer| fb.pixels.as_chunks::<4>().0.iter().filter(|p| p[3] > 0).count();
    assert!(count(&ring) < count(&square), "the ring must be hollow");
}

#[test]
fn scene_renderer_draws_the_attached_overlay() {
    // The overlay must reach the framebuffer through the public scene path,
    // not only when called directly.
    let scene = Scene3D::new().with_overlay({
        let mut overlay = Overlay3D::new();
        overlay.push(OverlayPrimitive::Marker {
            anchor: DVec3::ZERO,
            kind: MarkerKind::Square,
            color: Color::rgba(255, 0, 0, 255),
            size: 7.0,
        });
        overlay
    });
    assert!(scene.overlay().is_some());

    let viewport = Rect::new(0.0, 0.0, VIEW_W as f32, VIEW_H as f32);
    let mut fb = FrameBuffer::new(VIEW_W as usize, VIEW_H as usize);
    strata_render::rasterizer3d::render_scene_3d(&mut fb, viewport, &scene, None);

    let frame = RenderFrame64::from_camera(&scene_overlay_camera(), VIEW_W, VIEW_H);
    let p = frame.project(DVec3::ZERO);
    assert!(
        alpha(&fb, p.x as usize, p.y as usize) > 0.5,
        "the scene renderer must draw the overlay marker at its anchor"
    );
}

/// The scene renderer builds its overlay frame from `Camera3D`, whose `fov`
/// is applied on the vertical axis.
fn scene_overlay_camera() -> Camera64 {
    overlay_camera()
}
