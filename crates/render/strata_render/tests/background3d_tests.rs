//! Background, celestial, and cloud-shell declarations.
//!
//! The Earth view needs a sky that is filled before geometry, a sun and moon
//! drawn over it, and a translucent cloud shell over the surface. These tests
//! pin the fill order, the ramp endpoints, and the coverage maths.

use strata_layout::Rect;
use strata_render::rasterizer::FrameBuffer;
use strata_render::rasterizer3d::render_scene_3d;
use strata_render::scene3d::{Background3D, Camera3D, CelestialBody3D, CloudShell3D, Scene3D};
use strata_style::types::Color;

fn camera() -> Camera3D {
    Camera3D::Perspective {
        fov_horizontal_deg: 60.0,
        near: 0.1,
        far: 1000.0,
        position: [0.0, 0.0, 10.0],
        target: [0.0, 0.0, 0.0],
    }
}

fn rgba8(c: Color) -> (u8, u8, u8, u8) {
    (
        (c.r() * 255.0).round() as u8,
        (c.g() * 255.0).round() as u8,
        (c.b() * 255.0).round() as u8,
        (c.a() * 255.0).round() as u8,
    )
}

#[test]
fn solid_background_fills_the_whole_viewport() {
    let mut scene = Scene3D::new();
    scene.camera = camera();
    scene = scene.with_background(Background3D::Solid(Color::rgba(3, 7, 18, 255)));

    let viewport = Rect::new(0.0, 0.0, 32.0, 24.0);
    let mut fb = FrameBuffer::new(32, 24);
    render_scene_3d(&mut fb, viewport, &scene, None);

    assert_eq!(rgba8(fb.get_pixel(0, 0)), (3, 7, 18, 255));
    assert_eq!(rgba8(fb.get_pixel(31, 23)), (3, 7, 18, 255));
}

#[test]
fn vertical_gradient_hits_both_endpoints() {
    let top = Color::rgba(2, 6, 23, 255);
    let bottom = Color::rgba(30, 58, 138, 255);
    let background = Background3D::VerticalGradient { top, bottom };
    assert_eq!(rgba8(background.color_at(0.0).unwrap()), rgba8(top));
    assert_eq!(rgba8(background.color_at(1.0).unwrap()), rgba8(bottom));

    // Mid-ramp is within the +-2/255 tolerance of the linear blend.
    let mid = rgba8(background.color_at(0.5).unwrap());
    assert!((mid.0 as i32 - 16).abs() <= 2, "red mid-ramp was {}", mid.0);
    assert!(
        (mid.1 as i32 - 32).abs() <= 2,
        "green mid-ramp was {}",
        mid.1
    );
    assert!(
        (mid.2 as i32 - 80).abs() <= 2,
        "blue mid-ramp was {}",
        mid.2
    );
}

#[test]
fn gradient_is_rasterized_top_to_bottom() {
    let mut scene = Scene3D::new();
    scene.camera = camera();
    scene = scene.with_background(Background3D::VerticalGradient {
        top: Color::rgba(0, 0, 0, 255),
        bottom: Color::rgba(255, 255, 255, 255),
    });

    let viewport = Rect::new(0.0, 0.0, 16.0, 16.0);
    let mut fb = FrameBuffer::new(16, 16);
    render_scene_3d(&mut fb, viewport, &scene, None);

    let top = rgba8(fb.get_pixel(0, 0)).0;
    let bottom = rgba8(fb.get_pixel(0, 15)).0;
    assert!(top < 20, "the top of the gradient must be dark, got {top}");
    assert!(bottom > 235, "the bottom must be light, got {bottom}");
}

#[test]
fn transparent_background_leaves_the_buffer_untouched() {
    let mut scene = Scene3D::new();
    scene.camera = camera();
    assert_eq!(scene.background(), Background3D::Transparent);

    let viewport = Rect::new(0.0, 0.0, 8.0, 8.0);
    let mut fb = FrameBuffer::new(8, 8);
    render_scene_3d(&mut fb, viewport, &scene, None);
    assert_eq!(fb.get_pixel(4, 4).a(), 0.0);
}

#[test]
fn celestial_bodies_and_shells_are_recorded_in_order() {
    let scene = Scene3D::new()
        .with_celestial(CelestialBody3D {
            position: [1.0e8, 0.0, 0.0],
            radius: 1.4e6,
            color: Color::rgba(255, 247, 232, 255),
            glow: 2.0,
            depth_test: false,
        })
        .with_celestial(CelestialBody3D {
            position: [-3.8e8, 0.0, 0.0],
            radius: 8.7e5,
            color: Color::rgba(203, 213, 225, 255),
            glow: 1.0,
            depth_test: false,
        })
        .with_cloud_shell(CloudShell3D {
            centre: [0.0; 3],
            radius: 6_378_137.0 + 12_000.0,
            color: Color::rgba(255, 255, 255, 255),
            coverage: 0.5,
            opacity: 0.8,
        });

    let bodies = scene.celestial();
    assert_eq!(bodies.len(), 2);
    assert!(
        bodies[0].radius > bodies[1].radius,
        "the sun is the larger disc"
    );
    assert!(!bodies[0].depth_test);

    let shells = scene.cloud_shells();
    assert_eq!(shells.len(), 1);
    assert!((shells[0].radius - 6_390_137.0).abs() < 1.0);
}

#[test]
fn cloud_opacity_ramps_with_coverage() {
    let shell = CloudShell3D {
        centre: [0.0; 3],
        radius: 1.0,
        color: Color::WHITE,
        coverage: 0.0,
        opacity: 0.9,
    };
    assert_eq!(shell.effective_opacity(), 0.0);
    assert!(
        (CloudShell3D {
            coverage: 0.5,
            ..shell
        }
        .effective_opacity()
            - 0.45)
            .abs()
            < 1e-6
    );
    assert!(
        (CloudShell3D {
            coverage: 1.0,
            ..shell
        }
        .effective_opacity()
            - 0.9)
            .abs()
            < 1e-6
    );
    assert_eq!(
        CloudShell3D {
            coverage: 2.0,
            opacity: 2.0,
            ..shell
        }
        .effective_opacity(),
        1.0,
        "coverage and opacity must clamp"
    );
}

#[test]
fn background_precedes_geometry() {
    use strata_render::rasterizer3d::render_scene_3d_with_textures;
    use strata_render::scene3d::{Geometry3D, Material3D, Mesh3D, TexturedMaterial3D};
    use strata_render::texture3d::TextureStore;

    let source = strata_render::texture3d::TextureData {
        desc: strata_render::texture3d::TextureDesc {
            width: 1,
            height: 1,
            format: Default::default(),
        },
        pixels: vec![255u8, 0, 0, 255].into(),
    };
    let mut store = TextureStore::new();
    let id = store
        .insert(source, strata_render::texture3d::SamplerDesc::default())
        .expect("texture");

    let verts = vec![
        -1.0, -1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, //
        1.0, -1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, //
        1.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, //
        -1.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0,
    ];
    let mut scene = Scene3D::new();
    scene.camera = Camera3D::Perspective {
        fov_horizontal_deg: 60.0,
        near: 0.1,
        far: 100.0,
        position: [0.0, 0.0, 4.0],
        target: [0.0, 0.0, 0.0],
    };
    scene.add_mesh(Mesh3D::new(
        Geometry3D::CustomTextured {
            vertices: verts,
            indices: vec![0, 1, 2, 0, 2, 3],
        },
        Material3D::Textured(TexturedMaterial3D::new(id)),
    ));
    scene = scene.with_background(Background3D::Solid(Color::rgba(0, 0, 255, 255)));

    let viewport = Rect::new(0.0, 0.0, 64.0, 64.0);
    let mut fb = FrameBuffer::new(64, 64);
    render_scene_3d_with_textures(&mut fb, viewport, &scene, None, Some(&store));

    let corner = rgba8(fb.get_pixel(0, 0));
    let centre = rgba8(fb.get_pixel(32, 32));
    assert_eq!(
        corner,
        (0, 0, 255, 255),
        "the background must fill empty pixels"
    );
    assert_eq!(
        centre,
        (255, 0, 0, 255),
        "the mesh must draw over the background"
    );
}
