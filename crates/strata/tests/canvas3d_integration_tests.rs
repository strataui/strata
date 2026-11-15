//! Integration tests for GOAL-03: Real 3D Viewport in RSX and Display List Pipeline.

use strata::prelude::*;
use strata_render::compositor::DrawCommand;

#[test]
fn test_canvas3d_declarative_pipeline_in_runtime() {
    let view = rsx! {
        <div class="flex flex-col w-full h-full p-4 bg-slate-900">
            <h1 class="text-xl font-bold text-white mb-2">"3D Scene Test"</h1>
            <Canvas3D class="w-full h-96">
                <PerspectiveCamera
                    fov={60.0}
                    near={0.1}
                    far={1000.0}
                    position={[0.0, 5.0, 10.0]}
                    target={[0.0, 0.0, 0.0]}
                />
                <AmbientLight color="#ffffff" intensity={0.5} />
                <DirectionalLight
                    position={[1.0, 2.0, 1.0]}
                    color="#ffffff"
                    intensity={1.0}
                />
                <Mesh position={[0.0, 1.0, 0.0]}>
                    <BoxGeometry width={2.0} height={2.0} depth={2.0} />
                    <StandardMaterial color="#3b82f6" roughness={0.4} metalness={0.1} />
                </Mesh>
            </Canvas3D>
        </div>
    };

    let mut runtime = AppRuntime::new(view, 800.0, 600.0, 1.0);
    runtime.tick_frame();

    // Verify display list contains a DrawViewport3D command
    let viewport_cmd = runtime
        .display_list
        .commands
        .iter()
        .find(|cmd| matches!(cmd, DrawCommand::DrawViewport3D { .. }));

    assert!(
        viewport_cmd.is_some(),
        "Expected display list to contain DrawViewport3D command from <Canvas3D>"
    );

    if let Some(DrawCommand::DrawViewport3D { bounds, scene }) = viewport_cmd {
        // Bounds should match computed layout (800 wide minus padding, h-96 is 384px)
        assert!(
            bounds.width > 700.0,
            "Expected width > 700, got {}",
            bounds.width
        );
        assert_eq!(
            bounds.height, 384.0,
            "Expected height 384px (h-96), got {}",
            bounds.height
        );

        // Verify camera
        match &scene.camera {
            Camera3D::Perspective {
                fov_horizontal_deg,
                near,
                far,
                position,
                target,
            } => {
                assert!((fov_horizontal_deg - 60.0).abs() < 1e-4);
                assert!((near - 0.1).abs() < 1e-4);
                assert!((far - 1000.0).abs() < 1e-4);
                assert_eq!(*position, [0.0, 5.0, 10.0]);
                assert_eq!(*target, [0.0, 0.0, 0.0]);
            }
            _ => panic!("Expected Perspective camera in scene"),
        }

        // Verify lights
        assert_eq!(scene.lights.len(), 2, "Expected 2 lights in compiled scene");
        let has_ambient = scene.lights.iter().any(
            |l| matches!(l, Light3D::Ambient { intensity, .. } if (*intensity - 0.5).abs() < 1e-4),
        );
        assert!(has_ambient, "Expected ambient light with intensity 0.5");

        // Verify meshes
        assert_eq!(scene.meshes.len(), 1, "Expected 1 mesh in compiled scene");
        let mesh = &scene.meshes[0];
        assert_eq!(mesh.transform.position, [0.0, 1.0, 0.0]);
        match &mesh.geometry {
            Geometry3D::Box {
                width,
                height,
                depth,
            } => {
                assert_eq!(*width, 2.0);
                assert_eq!(*height, 2.0);
                assert_eq!(*depth, 2.0);
            }
            _ => panic!("Expected Box geometry on mesh"),
        }
        match &mesh.material {
            Material3D::Standard {
                roughness,
                metalness,
                ..
            } => {
                assert!((roughness - 0.4).abs() < 1e-4);
                assert!((metalness - 0.1).abs() < 1e-4);
            }
            _ => panic!("Expected Standard material on mesh"),
        }
    }
}

#[test]
fn test_scene_viewport_programmatic_scene_textures_and_hud_stacking_order() {
    use std::sync::Arc;
    use strata_render::rasterizer::FrameBuffer;
    use strata_render::scene3d::{Background3D, TexturedMaterial3D};
    use strata_render::texture3d::TextureStore;
    use strata_style::types::Color;
    use strata_ui::scene3d::{HudCard, SceneViewport};

    // 1. Build a TextureStore with a bright vivid cyan-blue satellite tile (R=16, G=196, B=248)
    // and attach a Textured mesh whose fallback tint is white, while an untextured fallback
    // without TextureStore would produce pure white instead of (16, 196, 248).
    let mut store = TextureStore::new();
    let tex_pixels = vec![
        16, 196, 248, 255, 16, 196, 248, 255, 16, 196, 248, 255, 16, 196, 248, 255,
    ];
    let tex_id = store
        .insert_rgba8(2, 2, tex_pixels)
        .expect("2x2 RGBA8 texture must insert");

    // Full-viewport quad facing +Z camera at z = 0; layout per vertex: x, y, z, nx, ny, nz, u, v
    let quad_geom = Geometry3D::CustomTextured {
        vertices: vec![
            -10.0, -10.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, //
            10.0, -10.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, //
            10.0, 10.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, //
            -10.0, 10.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0,
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
    };
    let mut tex_mat = TexturedMaterial3D::new(tex_id);
    tex_mat.tint = Color::rgba(255, 255, 255, 255);
    tex_mat.lit = false;

    let mut scene = Scene3D::new()
        .with_background(Background3D::Solid(Color::rgba(5, 8, 15, 255)))
        .with_textures(Arc::new(store));
    scene.camera = Camera3D::Perspective {
        fov_horizontal_deg: 60.0,
        near: 0.1,
        far: 100.0,
        position: [0.0, 0.0, 5.0],
        target: [0.0, 0.0, 0.0],
    };
    scene.add_mesh(Mesh3D::new(quad_geom, Material3D::Textured(tex_mat)));
    assert!(scene.overlay_mut().is_empty());

    let shared_scene = Arc::new(scene);

    // 2. Render <SceneViewport scene={&shared_scene}> inside a flexbox dock with both:
    //    - an in-viewport 2D HUD bar at the top (bright red #ff2040, z-10)
    //    - a Top-Layer <Portal> context menu overlay
    let hud_color_hex =
        "position: relative; z-index: 10; background-color: #ff2040; width: 200px; height: 60px;";
    let view = rsx! {
        <div class="flex flex-row w-full h-full">
            <div class="w-48 h-full bg-slate-900">"Sidebar"</div>
            <div class="flex-1 h-full">
                <SceneViewport scene={&shared_scene} class="w-full h-full overflow-hidden">
                    <div style={hud_color_hex}>
                        <HudCard title="Telemetry HUD">"ALT 1240m"</HudCard>
                    </div>
                </SceneViewport>
            </div>
            <strata_dom::Portal>
                <div style="position: fixed; left: 260px; top: 120px; width: 120px; height: 50px; background-color: #20ff60;">
                    "Portal Menu"
                </div>
            </strata_dom::Portal>
        </div>
    };

    let mut runtime = AppRuntime::new(view, 800.0, 600.0, 1.0);
    runtime.tick_frame();

    // 3. Verify DrawViewport3D is emitted ONCE and precedes both the z-10 HUD rect and Top-Layer Portal rect!
    let vp_indices: Vec<usize> = runtime
        .display_list
        .commands
        .iter()
        .enumerate()
        .filter_map(|(i, cmd)| matches!(cmd, DrawCommand::DrawViewport3D { .. }).then_some(i))
        .collect();
    assert_eq!(
        vp_indices.len(),
        1,
        "Expected exactly 1 DrawViewport3D command, got {:?}",
        vp_indices
    );
    let vp_idx = vp_indices[0];

    let hud_idx = runtime
        .display_list
        .commands
        .iter()
        .position(|cmd| {
            matches!(
                cmd,
                DrawCommand::DrawRect { color, .. }
                    if color.r == 0xff && color.g == 0x20 && color.b == 0x40
            )
        })
        .expect("Expected z-10 HUD #ff2040 DrawRect in display list");
    let portal_idx = runtime
        .display_list
        .commands
        .iter()
        .position(|cmd| {
            matches!(
                cmd,
                DrawCommand::DrawRect { color, .. }
                    if color.r == 0x20 && color.g == 0xff && color.b == 0x60
            )
        })
        .expect("Expected Portal #20ff60 DrawRect in display list");

    assert!(
        vp_idx < hud_idx,
        "3D viewport (cmd #{vp_idx}) must paint BEFORE z-10 2D HUD overlay (cmd #{hud_idx})"
    );
    assert!(
        hud_idx < portal_idx,
        "z-10 2D HUD overlay (cmd #{hud_idx}) must paint BEFORE Top-Layer Portal (cmd #{portal_idx})"
    );

    // 4. Execute the DisplayList into a FrameBuffer and verify:
    //    (a) Uncovered 3D viewport pixels show the satellite texture colour (16, 196, 248), NOT white (255, 255, 255)!
    //    (b) Pixels inside the z-10 HUD rect show #ff2040 on top of the 3D scene!
    //    (c) Pixels inside the Top-Layer Portal rect show #20ff60 on top of the 3D scene!
    let mut fb = FrameBuffer::new(800, 600);
    fb.execute_display_list(&runtime.display_list);

    let pixel_at = |fb: &FrameBuffer, x: usize, y: usize| -> [u8; 4] {
        let off = (y * fb.width + x) * 4;
        [
            fb.pixels[off],
            fb.pixels[off + 1],
            fb.pixels[off + 2],
            fb.pixels[off + 3],
        ]
    };

    // Uncovered 3D viewport center (sidebar is 192px wide, so x=500, y=400 is squarely inside the 3D canvas)
    let scene_px = pixel_at(&fb, 500, 400);
    assert!(
        (scene_px[0] as i32 - 16).abs() <= 2
            && (scene_px[1] as i32 - 196).abs() <= 2
            && (scene_px[2] as i32 - 248).abs() <= 2,
        "Expected satellite texture RGB ~(16, 196, 248) at uncovered 3D pixel, got {:?}",
        scene_px
    );

    // HUD box is unpadded: sidebar=192, so HUD #ff2040 spans x=[192..392], y=[0..60].
    // Sample near right edge of HUD bar (x=395, y=30) away from inner HudCard so we hit pure #ff2040.
    let hud_px = pixel_at(&fb, 385, 30);
    assert!(
        hud_px[0] > hud_px[1] && hud_px[0] > hud_px[2] && hud_px[0] > 40,
        "2D HUD overlay (red bar, translucent card) must composite ON TOP of the 3D scene, got {:?}",
        hud_px
    );

    // Top-Layer Portal spans x=[260..380], y=[120..170]; sample near right edge (x=370, y=160)
    let portal_px = pixel_at(&fb, 370, 160);
    assert_eq!(
        [portal_px[0], portal_px[1], portal_px[2]],
        [0x20, 0xff, 0x60],
        "Top-Layer Portal must composite ON TOP of the 3D scene, got {:?}",
        portal_px
    );
}

#[test]
fn test_scene_viewport_on_camera_change_pointer_and_wheel_interaction() {
    use std::cell::RefCell;
    use std::rc::Rc;
    use strata_render::orbit_controls::OrbitControls;
    use strata_ui::scene3d::SceneViewport;

    let mut scene = Scene3D::new();
    scene.camera = Camera3D::Perspective {
        fov_horizontal_deg: 60.0,
        near: 0.1,
        far: 1000.0,
        position: [0.0, 0.0, 20.0],
        target: [0.0, 0.0, 0.0],
    };

    let observed: Rc<RefCell<Vec<(f32, f32, f32)>>> = Rc::new(RefCell::new(Vec::new()));
    let observed_cb = Rc::clone(&observed);

    let view = rsx! {
        <SceneViewport
            scene={scene}
            width={600.0}
            height={400.0}
            on_camera_change={move |ctrl: &OrbitControls| {
                observed_cb.borrow_mut().push((ctrl.theta, ctrl.phi, ctrl.radius));
            }}
        />
    };

    let mut runtime = AppRuntime::new(view, 600.0, 400.0, 1.0);
    runtime.tick_frame();

    // Drag across the 3D viewport to orbit rotate
    runtime.dispatch_pointer_down(300.0, 200.0);
    runtime.dispatch_pointer_move(360.0, 230.0);
    runtime.dispatch_pointer_up(360.0, 230.0);

    assert_eq!(
        observed.borrow().len(),
        1,
        "Expected on_camera_change to fire on pointer drag"
    );
    let (theta_after_drag, _phi_after_drag, radius_after_drag) = observed.borrow()[0];
    assert!(
        theta_after_drag.abs() > 0.05,
        "Expected non-zero orbit theta rotation after drag, got {}",
        theta_after_drag
    );
    assert!((radius_after_drag - 20.0).abs() < 1e-3);

    // Wheel scroll over the 3D viewport to zoom in
    let wheel_handled = runtime.dispatch_scroll(300.0, 200.0, 0.0, 5.0);
    assert!(
        wheel_handled,
        "SceneViewport wheel listener should handle scroll"
    );
    assert_eq!(
        observed.borrow().len(),
        2,
        "Expected on_camera_change to fire on wheel zoom"
    );
    let (_theta_after_zoom, _phi_after_zoom, radius_after_zoom) = observed.borrow()[1];
    assert!(
        (radius_after_zoom - 20.0).abs() > 0.1,
        "Expected orbit radius to change on wheel zoom, got {}",
        radius_after_zoom
    );
}

#[test]
fn test_canvas3d_pointer_and_wheel_listeners_fire_and_request_rebuild() {
    use std::cell::Cell;
    use std::rc::Rc;
    use strata_dom::Event;
    use strata_platform::PlatformEvent;

    let down_count = Rc::new(Cell::new(0u32));
    let move_count = Rc::new(Cell::new(0u32));
    let up_count = Rc::new(Cell::new(0u32));
    let wheel_delta = Rc::new(Cell::new(0.0f64));

    let down_c = Rc::clone(&down_count);
    let move_c = Rc::clone(&move_count);
    let up_c = Rc::clone(&up_count);
    let wheel_c = Rc::clone(&wheel_delta);

    let scene = Scene3D::new();
    let view = rsx! {
        <Canvas3D
            scene={scene}
            width={600.0}
            height={400.0}
            onmousedown={move |_ev: Event| {
                down_c.set(down_c.get() + 1);
            }}
            onmousemove={move |_ev: Event| {
                move_c.set(move_c.get() + 1);
            }}
            onmouseup={move |_ev: Event| {
                up_c.set(up_c.get() + 1);
            }}
            onwheel={move |ev: Event| {
                wheel_c.set(wheel_c.get() + ev.delta_y);
            }}
        />
    };

    let mut runtime = AppRuntime::new(view, 600.0, 400.0, 1.0);
    runtime.tick_frame();

    assert!(runtime.step_event(&PlatformEvent::MouseDown {
        x: 300.0,
        y: 200.0,
        button: strata_platform::MouseButton::Left,
    }));
    assert_eq!(down_count.get(), 1);

    runtime.step_event(&PlatformEvent::MouseMove { x: 320.0, y: 210.0 });
    assert!(move_count.get() >= 1);

    runtime.step_event(&PlatformEvent::MouseUp {
        x: 320.0,
        y: 210.0,
        button: strata_platform::MouseButton::Left,
    });
    assert_eq!(up_count.get(), 1);

    let scroll_handled = runtime.step_event(&PlatformEvent::Scroll {
        delta_x: 0.0,
        delta_y: -3.5,
        is_momentum: false,
    });
    assert!(
        scroll_handled,
        "PlatformEvent::Scroll over Canvas3D with onwheel listener must be handled even without a scrollbar"
    );
    assert!((wheel_delta.get() - (-3.5)).abs() < 1e-6);

    let (wheel_handled, scroll_changed) =
        runtime.dispatch_smooth_scroll_action(320.0, 210.0, 0.0, 2.0);
    assert!(wheel_handled);
    assert!(!scroll_changed);
    assert!((wheel_delta.get() - (-1.5)).abs() < 1e-6);
}

#[test]
fn test_canvas3d_on_camera_change_orbit_and_wheel() {
    use std::cell::RefCell;
    use std::rc::Rc;
    use strata_platform::PlatformEvent;
    use strata_render::orbit_controls::OrbitControls;

    let mut scene = Scene3D::new();
    scene.camera = Camera3D::Perspective {
        fov_horizontal_deg: 60.0,
        near: 0.1,
        far: 1000.0,
        position: [0.0, 0.0, 20.0],
        target: [0.0, 0.0, 0.0],
    };

    let observed: Rc<RefCell<Vec<(f32, f32, f32)>>> = Rc::new(RefCell::new(Vec::new()));
    let observed_cb = Rc::clone(&observed);

    let view = rsx! {
        <Canvas3D
            scene={scene}
            width={600.0}
            height={400.0}
            on_camera_change={move |ctrl: &OrbitControls| {
                observed_cb.borrow_mut().push((ctrl.theta, ctrl.phi, ctrl.radius));
            }}
        />
    };

    let mut runtime = AppRuntime::new(view, 600.0, 400.0, 1.0);
    runtime.tick_frame();

    runtime.dispatch_pointer_down(300.0, 200.0);
    runtime.dispatch_pointer_move(360.0, 230.0);
    runtime.dispatch_pointer_up(360.0, 230.0);

    assert_eq!(observed.borrow().len(), 1);
    assert!(observed.borrow()[0].0.abs() > 0.05);

    assert!(runtime.step_event(&PlatformEvent::Scroll {
        delta_x: 0.0,
        delta_y: 5.0,
        is_momentum: false,
    }));
    assert_eq!(observed.borrow().len(), 2);
    assert!((observed.borrow()[1].2 - 20.0).abs() > 0.1);
}

#[test]
fn test_canvas3d_retained_uncapped_skips_display_list_recompiles() {
    use strata_platform::{SoftwareSurfacePresenter, SwapchainCompositor, SwapchainFormat};
    use strata_render::scene3d::{RetainedCameraUniforms, RetainedScene3D};

    let retained = RetainedScene3D::new(RetainedCameraUniforms::default());
    let canvas_el = Canvas3D::builder()
        .retained(retained.clone())
        .uncapped(true)
        .width(640.0)
        .height(480.0)
        .build();

    let view = rsx! {
        <div class="w-full h-full">
            {canvas_el}
        </div>
    };

    let mut runtime = AppRuntime::new(view, 640.0, 480.0, 1.0);
    assert!(
        runtime.has_uncapped_canvas3d(),
        "uncapped Canvas3D attribute should be detected by AppRuntime"
    );

    let mut compositor = SwapchainCompositor::new(640, 480, 1.0);
    let mut presenter = SoftwareSurfacePresenter::new(640, 480, SwapchainFormat::Bgra8Unorm);

    runtime
        .render_frame(&mut compositor, &mut presenter, true)
        .expect("initial present");
    assert!(
        !runtime.poll_retained_3d_dirty(),
        "initial render_frame must acknowledge retained 3D generations"
    );

    let initial_skipped = runtime.compiles_skipped();

    // Update camera on RetainedScene3D directly 10 times without marking 2D layout/DOM dirty.
    for i in 0..10u32 {
        let cam = RetainedCameraUniforms {
            eye_ecef: [(i + 1) as f64 * 1.5, 0.0, 5.0],
            ..Default::default()
        };
        retained.update_camera(cam);
        assert!(runtime.poll_retained_3d_dirty());

        let presented = runtime
            .render_frame(&mut compositor, &mut presenter, false)
            .expect("camera-only present");
        assert!(
            presented,
            "camera update must trigger GPU/swapchain present"
        );
        assert!(
            !runtime.poll_retained_3d_dirty(),
            "presented generations must be acknowledged cleanly"
        );
    }

    assert_eq!(
        runtime.compiles_skipped(),
        initial_skipped + 10,
        "all 10 camera-only updates must skip 2D DOM/layout/display-list compilation"
    );
}
