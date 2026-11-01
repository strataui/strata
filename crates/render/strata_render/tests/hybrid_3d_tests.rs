//! Integration Tests: Unified Hybrid 3D Scene Graph, OrbitControls, and Low-Level Shaders.

use strata_layout::Rect;
use strata_render::compositor::{DisplayList, DrawCommand};
use strata_render::custom_shader::{CustomShaderPipeline, WgslShaderSource};
use strata_render::math3d::{Mat4, Vec3};
use strata_render::orbit_controls::OrbitControls;
use strata_render::rasterizer::FrameBuffer;
use strata_render::scene3d::{
    Camera3D, Geometry3D, Light3D, Material3D, Mesh3D, Scene3D, Transform3D,
};
use strata_render::shared_target::{render_hybrid_frame, SharedRenderTarget};
use strata_style::types::Color;

#[test]
fn test_orbit_controls_interactive_simulation() {
    let mut camera = Camera3D::Perspective {
        fov: 60.0,
        aspect: 16.0 / 9.0,
        near: 0.1,
        far: 1000.0,
        position: [0.0, 5.0, 10.0],
        target: [0.0, 0.0, 0.0],
    };

    let mut controls = OrbitControls::from_camera(&camera);
    let initial_radius = controls.radius;
    assert!((initial_radius - 125.0f32.sqrt()).abs() < 1e-3);

    // 1. Simulate pointer rotate drag
    controls.on_pointer_down(100.0, 100.0, 0); // Button 0 = rotate
    controls.on_pointer_move(150.0, 120.0, 800.0, 600.0, 60.0);
    controls.on_pointer_up();

    // Advance damping simulation
    for _ in 0..30 {
        controls.tick(0.016);
    }
    controls.update_camera(&mut camera);

    if let Camera3D::Perspective { position, .. } = camera {
        let eye = Vec3::new(position[0], position[1], position[2]);
        assert_ne!(eye, Vec3::new(0.0, 5.0, 10.0), "Eye position must have rotated");
    }

    // 2. Simulate pointer right-click pan
    let pre_pan_target = controls.target;
    controls.on_pointer_down(200.0, 200.0, 2); // Button 2 = pan
    controls.on_pointer_move(240.0, 210.0, 800.0, 600.0, 60.0);
    controls.on_pointer_up();

    for _ in 0..30 {
        controls.tick(0.016);
    }
    assert_ne!(controls.target, pre_pan_target, "Target must have shifted via panning");

    // 3. Simulate mouse wheel zoom
    let pre_zoom_radius = controls.radius;
    controls.on_wheel(5.0); // Positive wheel zooms in
    for _ in 0..30 {
        controls.tick(0.016);
    }
    assert!(controls.radius < pre_zoom_radius, "Radius must decrease when zooming in");

    // View matrix should be valid and invertible
    let vm = controls.view_matrix();
    assert!(vm.inverse().is_some(), "View matrix must be non-singular and invertible");
}

#[test]
fn test_shared_render_target_telemetry_hud_overlay() {
    let surface_w = 600;
    let surface_h = 400;
    let mut main_surface = FrameBuffer::new(surface_w, surface_h);

    let viewport_bounds = Rect::new(0.0, 0.0, 600.0, 400.0);
    let mut target = SharedRenderTarget::new(viewport_bounds);

    // Build 3D telemetry scene
    let mut scene = Scene3D::new();
    scene.camera = Camera3D::Perspective {
        fov: 50.0,
        aspect: 600.0 / 400.0,
        near: 0.1,
        far: 500.0,
        position: [0.0, 2.0, 6.0],
        target: [0.0, 0.0, 0.0],
    };
    scene.add_light(Light3D::Ambient {
        color: Color::rgb(40, 50, 70),
        intensity: 0.4,
    });
    scene.add_light(Light3D::Directional {
        position: [5.0, 8.0, 5.0],
        color: Color::WHITE,
        intensity: 1.0,
        cast_shadow: true,
    });

    // Central server node
    scene.add_mesh(
        Mesh3D::new(
            Geometry3D::Box { width: 1.6, height: 1.6, depth: 1.6 },
            Material3D::Standard {
                color: Color::rgb(59, 130, 246), // Blue-500
                roughness: 0.3,
                metalness: 0.7,
                emissive: None,
            },
        )
        .with_transform(Transform3D {
            position: [0.0, 0.0, 0.0],
            rotation: [0.3, 0.5, 0.0],
            scale: [1.0, 1.0, 1.0],
        }),
    );

    // Construct 2D HUD DisplayList overlay with telemetry badges
    let mut overlay_dl = DisplayList::new();

    // Top-Left Altitude Telemetry Badge (20, 20, 120, 40) - Emerald
    overlay_dl.push(DrawCommand::DrawRect {
        bounds: Rect::new(20.0, 20.0, 120.0, 40.0),
        color: Color::rgb(16, 185, 129),
    });

    // Top-Right Latency Gauge Badge (460, 20, 120, 40) - Violet
    overlay_dl.push(DrawCommand::DrawRect {
        bounds: Rect::new(460.0, 20.0, 120.0, 40.0),
        color: Color::rgb(139, 92, 246),
    });

    // Bottom Action HUD Button (250, 340, 100, 36) - Rose
    overlay_dl.push(DrawCommand::DrawRect {
        bounds: Rect::new(250.0, 340.0, 100.0, 36.0),
        color: Color::rgb(244, 63, 94),
    });

    // Render hybrid 2D/3D frame
    render_hybrid_frame(&mut target, &scene, &overlay_dl, &mut main_surface, None);

    // Verification 1: Center of Altitude Badge must be Emerald
    let alt_pixel = main_surface.get_pixel(60, 40);
    assert_eq!(alt_pixel, Color::rgb(16, 185, 129));

    // Verification 2: Center of Latency Badge must be Violet
    let lat_pixel = main_surface.get_pixel(500, 40);
    assert_eq!(lat_pixel, Color::rgb(139, 92, 246));

    // Verification 3: Center of Action Button must be Rose
    let btn_pixel = main_surface.get_pixel(300, 358);
    assert_eq!(btn_pixel, Color::rgb(244, 63, 94));

    // Verification 4: Deep background corner must match 3D backdrop
    let corner_pixel = main_surface.get_pixel(5, 5);
    assert_eq!(corner_pixel, target.background_color);
}

#[test]
fn test_custom_wgsl_shader_pipeline_execution() {
    // 1. Validate PBR 3D shader
    let pbr_src = WgslShaderSource::pbr_3d();
    assert_eq!(pbr_src.name, "strata_pbr_3d");
    assert!(pbr_src.source.contains("fn vs_main"));
    assert!(pbr_src.source.contains("fn fs_main"));

    let mut pbr_pipeline = CustomShaderPipeline::new(pbr_src);
    assert!(pbr_pipeline.validate().is_ok());

    pbr_pipeline.set_uniform_f32("roughness", 0.45);
    pbr_pipeline.set_uniform_f32("metalness", 0.85);
    pbr_pipeline.set_uniform_vec4("base_color", [0.1, 0.6, 0.9, 1.0]);
    pbr_pipeline.set_uniform_mat4("view_proj", &Mat4::perspective(1.0, 1.77, 0.1, 100.0));

    assert_eq!(*pbr_pipeline.uniforms_f32.get("roughness").unwrap(), 0.45);
    assert_eq!(*pbr_pipeline.uniforms_f32.get("metalness").unwrap(), 0.85);

    // 2. Validate Compute shader
    let cs_src = WgslShaderSource::compute_telemetry();
    assert_eq!(cs_src.name, "strata_compute_telemetry");
    assert!(cs_src.source.contains("fn cs_main"));

    let cs_pipeline = CustomShaderPipeline::new(cs_src);
    assert!(cs_pipeline.validate().is_ok());
}
