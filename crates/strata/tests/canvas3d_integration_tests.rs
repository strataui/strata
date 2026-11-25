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

#[test]
fn test_canvas3d_builder_parity_and_responsive_orbit_controls() {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use strata::render::compositor::DrawCommand;
    use strata::render::orbit_controls::OrbitControls;
    use strata_platform::PlatformEvent;

    let unit_clicks = Rc::new(Cell::new(0usize));
    let dbl_clicks = Rc::new(Cell::new(0usize));
    let ctx_menus = Rc::new(Cell::new(0usize));
    let enters = Rc::new(Cell::new(0usize));
    let leaves = Rc::new(Cell::new(0usize));
    let orbit_thetas: Rc<RefCell<Vec<f32>>> = Rc::new(RefCell::new(Vec::new()));

    let mut scene = Scene3D::new();
    scene.camera = Camera3D::Perspective {
        fov_horizontal_deg: 60.0,
        near: 0.1,
        far: 1000.0,
        position: [0.0, 0.0, 20.0],
        target: [0.0, 0.0, 0.0],
    };

    let build_root = {
        let unit_c = Rc::clone(&unit_clicks);
        let dbl_c = Rc::clone(&dbl_clicks);
        let ctx_c = Rc::clone(&ctx_menus);
        let ent_c = Rc::clone(&enters);
        let lve_c = Rc::clone(&leaves);
        let thetas = Rc::clone(&orbit_thetas);
        let cls = String::from("flex-1 h-full");
        rsx! {
            <div class="w-full h-full flex">
                <div class="w-[200px] h-full" />
                <Canvas3D
                    id="cad-3d"
                    key="cad-3d-key"
                    title="Perspective CAD View"
                    class={cls}
                    width={999.0}
                    height={999.0}
                    responsive={true}
                    scene={scene}
                    onclick={move || unit_c.set(unit_c.get() + 1)}
                    ondblclick={move |_ev: Event| dbl_c.set(dbl_c.get() + 1)}
                    oncontextmenu={move |_ev: Event| ctx_c.set(ctx_c.get() + 1)}
                    onmouseenter={move |_ev: Event| ent_c.set(ent_c.get() + 1)}
                    onmouseleave={move |_ev: Event| lve_c.set(lve_c.get() + 1)}
                    on_camera_change={move |ctrl: &OrbitControls| {
                        thetas.borrow_mut().push(ctrl.theta);
                    }}
                />
            </div>
        }
    };

    let mut runtime = AppRuntime::new(build_root, 800.0, 400.0, 1.0);
    runtime.set_hover_tooltip_delay(std::time::Duration::ZERO);

    // 1. `responsive(true)` omits width/height properties so flex sizes Canvas3D to 600x400 (not 999x999)
    let rect = runtime.query_element_rect("cad-3d").expect("cad-3d rect");
    assert!(
        (rect.width - 600.0).abs() < 0.5 && (rect.height - 400.0).abs() < 0.5,
        "Expected 600x400 flex size, got {}x{}",
        rect.width,
        rect.height
    );

    // 2. Title tooltip, mouseenter/mouseleave, 0-arg onclick, ondblclick, oncontextmenu
    runtime.update_hover(350.0, 200.0);
    assert_eq!(enters.get(), 1);
    assert!(runtime.poll_hover_tooltip());
    assert!(runtime.display_list.commands().iter().any(|c| matches!(
        c,
        DrawCommand::DrawText { text, .. } if text == "Perspective CAD View"
    )));

    assert!(runtime.dispatch_mouse_click(350.0, 200.0, 0, 1));
    assert_eq!(unit_clicks.get(), 1);
    assert!(runtime.dispatch_dblclick(350.0, 200.0));
    assert_eq!(dbl_clicks.get(), 1);
    assert!(runtime.dispatch_context_menu(350.0, 200.0));
    assert_eq!(ctx_menus.get(), 1);

    runtime.update_hover(50.0, 200.0);
    assert_eq!(leaves.get(), 1);

    // 3. `attach_orbit_controls_listeners` uses live `ev.width`/`ev.height`:
    // Drag 50px on 600px-wide canvas vs 250px-wide canvas after WindowResized(450, 400).
    runtime.dispatch_pointer_down(300.0, 200.0);
    runtime.dispatch_pointer_move(350.0, 200.0);
    runtime.dispatch_pointer_up(350.0, 200.0);
    let theta_wide = *orbit_thetas.borrow().last().expect("wide orbit theta");

    assert!(runtime.step_event(&PlatformEvent::WindowResized {
        width: 450,
        height: 400,
    }));
    let rect_narrow = runtime.query_element_rect("cad-3d").expect("narrow rect");
    assert!((rect_narrow.width - 250.0).abs() < 0.5);

    runtime.dispatch_pointer_down(300.0, 200.0);
    runtime.dispatch_pointer_move(350.0, 200.0);
    runtime.dispatch_pointer_up(350.0, 200.0);
    let theta_after = *orbit_thetas.borrow().last().expect("narrow orbit theta");
    let delta_wide = theta_wide.abs();
    let delta_narrow = (theta_after - theta_wide).abs();
    assert!(
        delta_narrow > delta_wide * 2.0,
        "Narrower responsive Canvas3D (250px) must rotate faster per pixel than 600px (narrow={}, wide={})",
        delta_narrow,
        delta_wide
    );
}

#[test]
fn test_canvas3d_builder_zero_arg_closures_across_all_event_setters() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let fired: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let push = |tag: &'static str, log: &Rc<RefCell<Vec<&'static str>>>| {
        let l = Rc::clone(log);
        move || l.borrow_mut().push(tag)
    };

    let canvas = Canvas3D::builder()
        .id("cad-3d-zero-arg")
        .width(320.0)
        .height(200.0)
        .onpointerdown(push("pd", &fired))
        .onmousedown(push("md", &fired))
        .onpointermove(push("pm", &fired))
        .onmousemove(push("mm", &fired))
        .onpointerup(push("pu", &fired))
        .onmouseup(push("mu", &fired))
        .onwheel(push("wh", &fired))
        .onresize(push("rs", &fired))
        .onclick(push("ck", &fired))
        .ondblclick(push("dc", &fired))
        .oncontextmenu(push("cm", &fired))
        .onmouseenter(push("me", &fired))
        .onmouseleave(push("ml", &fired))
        .build();

    let view = rsx! {
        <div class="w-[400px] h-[300px]">
            {canvas}
        </div>
    };

    let mut rt = AppRuntime::new(view, 400.0, 300.0, 1.0);
    assert!(fired.borrow().contains(&"rs"));
    fired.borrow_mut().clear();

    rt.update_hover(100.0, 100.0);
    assert!(rt.dispatch_pointer_down(100.0, 100.0));
    assert!(rt.dispatch_pointer_move(110.0, 105.0));
    assert!(rt.dispatch_pointer_up(110.0, 105.0));
    assert!(rt.dispatch_mouse_click(110.0, 105.0, 0, 1));
    assert!(rt.dispatch_dblclick(110.0, 105.0));
    assert!(rt.dispatch_context_menu(110.0, 105.0));
    assert!(rt.dispatch_scroll(110.0, 105.0, 0.0, 10.0));
    rt.update_hover(380.0, 280.0);

    let log = fired.borrow();
    for expected in [
        "me", "pd", "md", "pm", "mm", "pu", "mu", "ck", "dc", "cm", "wh", "ml",
    ] {
        assert!(
            log.contains(&expected),
            "Expected zero-arg handler `{expected}` to fire, got {:?}",
            *log
        );
    }
}


