//! Comprehensive test suite verifying native platform event dispatching,
//! window resizing, smooth scrolling, double-click, context menus,
//! caret blinking, key handling, and frame pacing across desktop and mobile backends.

use strata::prelude::*;
use strata::runtime::platforms::dispatch_native_platform_event;
use strata_platform::{
    KeyCode, Modifiers, MouseButton, PlatformEvent, SoftwareSurfacePresenter, SwapchainCompositor,
    SwapchainFormat,
};

fn sample_view() -> View {
    rsx! {
        <div class="w-full h-full p-4 bg-slate-900 text-white flex-col">
            <span class="text-xl font-bold">"Strata Native Windowing"</span>
            <input class="w-64 h-10 border border-slate-700 p-2" placeholder="Search..." />
            <button class="px-4 py-2 bg-blue-600 rounded">"Submit"</button>
        </div>
    }
}

#[test]
fn test_dispatch_native_platform_window_resize_and_scale() {
    let mut runtime = AppRuntime::new(sample_view(), 800.0, 600.0, 1.0);
    let mut compositor = SwapchainCompositor::new(800, 600, 1.0);
    let mut presenter = SoftwareSurfacePresenter::new(800, 600, SwapchainFormat::Bgra8Unorm);
    let mut hook = |_: &PlatformEvent, _: &mut AppRuntime| false;

    let resize_event = PlatformEvent::WindowResized {
        width: 1024,
        height: 768,
    };
    let (rebuild, redraw, exit) = dispatch_native_platform_event(
        &resize_event,
        &mut runtime,
        &mut compositor,
        &mut presenter,
        &mut hook,
    );
    assert!(!rebuild);
    assert!(redraw);
    assert!(!exit);
    assert_eq!(runtime.viewport_width, 1024.0);
    assert_eq!(runtime.viewport_height, 768.0);
    assert_eq!(presenter.width, 1024);
    assert_eq!(presenter.height, 768);

    let scale_event = PlatformEvent::ScaleFactorChanged { scale_factor: 2.0 };
    let (rebuild, redraw, exit) = dispatch_native_platform_event(
        &scale_event,
        &mut runtime,
        &mut compositor,
        &mut presenter,
        &mut hook,
    );
    assert!(!rebuild);
    assert!(redraw);
    assert!(!exit);
    assert_eq!(runtime.dpr, 2.0);
}

#[test]
fn test_dispatch_native_platform_scroll_events() {
    let scroll_view = rsx! {
        <div class="w-full h-48 overflow-y-scroll">
            <div class="w-full h-96">
                <span>"Scrollable content tall enough to scroll"</span>
            </div>
        </div>
    };
    let mut runtime = AppRuntime::new(scroll_view, 800.0, 600.0, 1.0);
    let mut compositor = SwapchainCompositor::new(800, 600, 1.0);
    let mut presenter = SoftwareSurfacePresenter::new(800, 600, SwapchainFormat::Bgra8Unorm);
    let mut hook = |_: &PlatformEvent, _: &mut AppRuntime| false;

    runtime.last_mouse_pos = (50.0, 50.0);
    let scroll_event = PlatformEvent::Scroll {
        delta_x: 0.0,
        delta_y: 50.0,
        is_momentum: false,
    };
    let (rebuild, redraw, exit) = dispatch_native_platform_event(
        &scroll_event,
        &mut runtime,
        &mut compositor,
        &mut presenter,
        &mut hook,
    );
    assert!(!rebuild);
    assert!(redraw);
    assert!(!exit);
}

#[test]
fn test_dispatch_native_platform_mouse_and_double_click() {
    let mut runtime = AppRuntime::new(sample_view(), 800.0, 600.0, 1.0);
    let mut compositor = SwapchainCompositor::new(800, 600, 1.0);
    let mut presenter = SoftwareSurfacePresenter::new(800, 600, SwapchainFormat::Bgra8Unorm);
    let mut hook = |_: &PlatformEvent, _: &mut AppRuntime| false;

    let move_event = PlatformEvent::MouseMove { x: 50.0, y: 50.0 };
    let (_rebuild, _redraw, exit) = dispatch_native_platform_event(
        &move_event,
        &mut runtime,
        &mut compositor,
        &mut presenter,
        &mut hook,
    );
    assert!(!exit);
    assert_eq!(runtime.last_mouse_pos, (50.0, 50.0));

    let dbl_click = PlatformEvent::DoubleClick {
        button: MouseButton::Left,
        x: 50.0,
        y: 50.0,
    };
    let (_rebuild, _redraw, exit) = dispatch_native_platform_event(
        &dbl_click,
        &mut runtime,
        &mut compositor,
        &mut presenter,
        &mut hook,
    );
    assert!(!exit);

    let right_click = PlatformEvent::MouseDown {
        button: MouseButton::Right,
        x: 50.0,
        y: 50.0,
    };
    let (_rebuild, _redraw, exit) = dispatch_native_platform_event(
        &right_click,
        &mut runtime,
        &mut compositor,
        &mut presenter,
        &mut hook,
    );
    assert!(!exit);

    let mouse_up = PlatformEvent::MouseUp {
        button: MouseButton::Left,
        x: 50.0,
        y: 50.0,
    };
    let (_rebuild, _redraw, exit) = dispatch_native_platform_event(
        &mouse_up,
        &mut runtime,
        &mut compositor,
        &mut presenter,
        &mut hook,
    );
    assert!(!exit);
}

#[test]
fn test_dispatch_native_platform_keyboard_events() {
    let mut runtime = AppRuntime::new(sample_view(), 800.0, 600.0, 1.0);
    let mut compositor = SwapchainCompositor::new(800, 600, 1.0);
    let mut presenter = SoftwareSurfacePresenter::new(800, 600, SwapchainFormat::Bgra8Unorm);
    let mut hook = |_: &PlatformEvent, _: &mut AppRuntime| false;

    let key_down = PlatformEvent::KeyDown {
        key: KeyCode::KeyA,
        modifiers: Modifiers::default(),
    };
    let (_rebuild, _redraw, exit) = dispatch_native_platform_event(
        &key_down,
        &mut runtime,
        &mut compositor,
        &mut presenter,
        &mut hook,
    );
    assert!(!exit);

    let char_event = PlatformEvent::ReceivedCharacter('a');
    let (_rebuild, _redraw, exit) = dispatch_native_platform_event(
        &char_event,
        &mut runtime,
        &mut compositor,
        &mut presenter,
        &mut hook,
    );
    assert!(!exit);

    let key_up = PlatformEvent::KeyUp {
        key: KeyCode::KeyA,
        modifiers: Modifiers::default(),
    };
    let (_rebuild, _redraw, exit) = dispatch_native_platform_event(
        &key_up,
        &mut runtime,
        &mut compositor,
        &mut presenter,
        &mut hook,
    );
    assert!(!exit);
}

#[test]
fn test_dispatch_native_platform_window_close() {
    let mut runtime = AppRuntime::new(sample_view(), 800.0, 600.0, 1.0);
    let mut compositor = SwapchainCompositor::new(800, 600, 1.0);
    let mut presenter = SoftwareSurfacePresenter::new(800, 600, SwapchainFormat::Bgra8Unorm);
    let mut hook = |_: &PlatformEvent, _: &mut AppRuntime| false;

    let close_event = PlatformEvent::WindowCloseRequested;
    let (_rebuild, _redraw, should_exit) = dispatch_native_platform_event(
        &close_event,
        &mut runtime,
        &mut compositor,
        &mut presenter,
        &mut hook,
    );
    assert!(should_exit);
}

#[test]
fn test_compute_idle_sleep_pacing_states() {
    let mut runtime = AppRuntime::new(sample_view(), 800.0, 600.0, 1.0);

    // 1. Truly idle state -> 200ms sleep for maximum power efficiency
    let idle_sleep = runtime.compute_idle_sleep(false);
    assert_eq!(idle_sleep, std::time::Duration::from_millis(200));

    // 2. Active frame event -> 16ms sleep (60fps frame budget)
    let active_sleep = runtime.compute_idle_sleep(true);
    assert_eq!(active_sleep, std::time::Duration::from_millis(16));

    // 3. Text input focused -> 50ms sleep for responsive caret blinking
    let input_state = strata_dom::text_input::TextInputState::new();
    runtime.active_focus = Some(strata::runtime::ActiveInput {
        input_index: 0,
        input_id: None,
        entity_id: None,
        state: input_state,
        on_input: None,
        on_change: None,
        on_submit: None,
        on_blur: None,
    });
    let focused_sleep = runtime.compute_idle_sleep(false);
    assert_eq!(focused_sleep, std::time::Duration::from_millis(50));
    runtime.active_focus = None;

    // 4. Smooth scroll active -> 16ms sleep
    runtime.active_smooth_scroll = Some(strata::runtime::SmoothScrollTracker {
        container_path: vec![0],
        container_id: String::new(),
        is_id: false,
        start_y: 0.0,
        target_y: 100.0,
        current_y: 0.0,
        initial_velocity: 500.0,
        start_time: std::time::Instant::now(),
        last_event_time: std::time::Instant::now(),
        omega: 12.0,
        max_scroll_y: 500.0,
        available_travel: 400.0,
        bounds_y: 0.0,
    });
    let scroll_sleep = runtime.compute_idle_sleep(false);
    assert_eq!(scroll_sleep, std::time::Duration::from_millis(16));
    runtime.active_smooth_scroll = None;

    // 5. Scrollbar drag active -> 16ms sleep
    runtime.scrollbar_drag = Some(strata::runtime::ScrollbarDrag {
        is_id: false,
        id: String::new(),
        path: vec![0],
        start_pointer_y: 50.0,
        start_scroll_y: 10.0,
        available_travel: 100.0,
        max_scroll_y: 200.0,
    });
    let drag_sleep = runtime.compute_idle_sleep(false);
    assert_eq!(drag_sleep, std::time::Duration::from_millis(16));
    runtime.scrollbar_drag = None;

    // 6. Window drag candidate active -> 16ms sleep
    runtime.pending_window_drag = Some((100.0, 50.0));
    let win_drag_sleep = runtime.compute_idle_sleep(false);
    assert_eq!(win_drag_sleep, std::time::Duration::from_millis(16));
    runtime.pending_window_drag = None;

    // 7. Dirty reactive signal -> 16ms sleep
    strata_dom::reactive::mark_dirty();
    assert!(strata_dom::reactive::has_dirty_signals());
    let dirty_sleep = runtime.compute_idle_sleep(false);
    assert_eq!(dirty_sleep, std::time::Duration::from_millis(16));
    let _ = strata_dom::reactive::take_dirty_signals();
}

#[test]
fn test_dynamic_grab_and_grabbing_cursor_on_component() {
    use strata::prelude::rsx;
    use strata_platform::window::CursorIcon;

    let build = |grabbing: bool| {
        let cls = if grabbing {
            "absolute inset-0 w-full h-full z-0 cursor-grabbing"
        } else {
            "absolute inset-0 w-full h-full z-0 cursor-grab"
        };
        rsx! {
            <div class="relative w-[800px] h-[600px]">
                <div class={cls}>""</div>
                <button class="absolute top-0 left-0 z-10 w-[100px] h-[40px]">"Action"</button>
            </div>
        }
    };

    let mut runtime = AppRuntime::new(build(false), 800.0, 600.0, 1.0);
    let mut compositor = SwapchainCompositor::new(800, 600, 1.0);
    let mut presenter = SoftwareSurfacePresenter::new(800, 600, SwapchainFormat::Bgra8Unorm);
    let mut hook = |_: &PlatformEvent, _: &mut AppRuntime| false;

    // Hovering open canvas shows Grab; hovering overlay button shows Pointer.
    dispatch_native_platform_event(
        &PlatformEvent::MouseMove { x: 400.0, y: 300.0 },
        &mut runtime,
        &mut compositor,
        &mut presenter,
        &mut hook,
    );
    assert_eq!(runtime.current_cursor, CursorIcon::Grab);

    dispatch_native_platform_event(
        &PlatformEvent::MouseMove { x: 40.0, y: 20.0 },
        &mut runtime,
        &mut compositor,
        &mut presenter,
        &mut hook,
    );
    assert_eq!(runtime.current_cursor, CursorIcon::Pointer);

    // Move back over canvas, press down -> dynamic view swap updates cursor to Grabbing; release -> Grab.
    dispatch_native_platform_event(
        &PlatformEvent::MouseMove { x: 400.0, y: 300.0 },
        &mut runtime,
        &mut compositor,
        &mut presenter,
        &mut hook,
    );
    assert_eq!(runtime.current_cursor, CursorIcon::Grab);

    runtime.update_view(build(true));
    dispatch_native_platform_event(
        &PlatformEvent::MouseDown {
            button: MouseButton::Left,
            x: 400.0,
            y: 300.0,
        },
        &mut runtime,
        &mut compositor,
        &mut presenter,
        &mut hook,
    );
    assert_eq!(runtime.current_cursor, CursorIcon::Grabbing);

    runtime.update_view(build(false));
    dispatch_native_platform_event(
        &PlatformEvent::MouseUp {
            button: MouseButton::Left,
            x: 400.0,
            y: 300.0,
        },
        &mut runtime,
        &mut compositor,
        &mut presenter,
        &mut hook,
    );
    assert_eq!(runtime.current_cursor, CursorIcon::Grab);
}

#[test]
fn test_canvas3d_grab_grabbing_with_overlay_and_hook() {
    use strata::prelude::rsx;
    use strata_platform::window::CursorIcon;
    use strata_render::scene3d::{Canvas3D, RetainedScene3D};

    let retained = RetainedScene3D::new(Default::default());
    let build = |grabbing: bool| {
        let cls = if grabbing {
            "absolute inset-0 w-full h-full z-0 cursor-grabbing"
        } else {
            "absolute inset-0 w-full h-full z-0 cursor-grab"
        };
        let canvas = Canvas3D::builder()
            .retained(retained.clone())
            .class(cls)
            .build();
        rsx! {
            <div class="relative w-full h-full bg-black overflow-hidden">
                {canvas}
                <div class="absolute inset-0 z-10 pointer-events-none">
                    <div class="text-white">"Label"</div>
                </div>
            </div>
        }
    };

    let mut runtime = AppRuntime::new(build(false), 1280.0, 800.0, 1.0);
    let mut compositor = SwapchainCompositor::new(1280, 800, 1.0);
    let mut presenter = SoftwareSurfacePresenter::new(1280, 800, SwapchainFormat::Bgra8Unorm);
    let mut dragging = false;
    let mut hook = |ev: &PlatformEvent, _: &mut AppRuntime| -> bool {
        match ev {
            PlatformEvent::MouseDown { .. } => {
                dragging = true;
                true
            }
            PlatformEvent::MouseUp { .. } => {
                dragging = false;
                true
            }
            _ => false,
        }
    };

    dispatch_native_platform_event(
        &PlatformEvent::MouseMove { x: 640.0, y: 400.0 },
        &mut runtime,
        &mut compositor,
        &mut presenter,
        &mut hook,
    );
    assert_eq!(runtime.current_cursor, CursorIcon::Grab);

    let (rebuild, _, _) = dispatch_native_platform_event(
        &PlatformEvent::MouseDown {
            button: MouseButton::Left,
            x: 640.0,
            y: 400.0,
        },
        &mut runtime,
        &mut compositor,
        &mut presenter,
        &mut hook,
    );
    assert!(rebuild);
    runtime.update_view(build(dragging));
    let cur_after_rebuild = AppRuntime::hit_test_cursor(
        &runtime.root_blueprint,
        &runtime.layout_root,
        runtime.last_mouse_pos.0,
        runtime.last_mouse_pos.1,
    );
    assert_eq!(cur_after_rebuild, CursorIcon::Grabbing);
}

#[test]
fn test_right_click_dispatches_onmousedown_and_onmouseup_with_button_and_size() {
    use std::cell::RefCell;
    use std::rc::Rc;
    use strata_render::scene3d::{RetainedCameraUniforms, RetainedScene3D};

    let retained = RetainedScene3D::new(RetainedCameraUniforms::default());
    let log: Rc<RefCell<Vec<(&'static str, i16, f64, f64, f64, f64)>>> =
        Rc::new(RefCell::new(Vec::new()));
    let log_down = log.clone();
    let log_up = log.clone();

    let canvas = Canvas3D::builder()
        .retained(retained)
        .width(640.0)
        .height(400.0)
        .onmousedown(move |ev| {
            log_down.borrow_mut().push((
                "down",
                ev.button,
                ev.offset_x,
                ev.offset_y,
                ev.width,
                ev.height,
            ));
        })
        .onmouseup(move |ev| {
            log_up.borrow_mut().push((
                "up",
                ev.button,
                ev.offset_x,
                ev.offset_y,
                ev.width,
                ev.height,
            ));
        })
        .build();

    let view = rsx! {
        <div class="w-full h-full flex flex-col">
            <div class="w-full h-10">
                <button class="w-32 h-10">"Top"</button>
            </div>
            {canvas}
        </div>
    };

    let mut runtime = AppRuntime::new(view, 800.0, 600.0, 1.0);
    runtime.tick_frame();
    let mut compositor = SwapchainCompositor::new(800, 600, 1.0);
    let mut presenter = SoftwareSurfacePresenter::new(800, 600, SwapchainFormat::Bgra8Unorm);
    let mut hook = |_: &PlatformEvent, _: &mut AppRuntime| false;

    assert!(!runtime.target_is_retained_canvas3d(20.0, 15.0));
    assert!(runtime.target_is_retained_canvas3d(120.0, 100.0));

    dispatch_native_platform_event(
        &PlatformEvent::MouseDown {
            button: MouseButton::Right,
            x: 120.0,
            y: 100.0,
        },
        &mut runtime,
        &mut compositor,
        &mut presenter,
        &mut hook,
    );
    dispatch_native_platform_event(
        &PlatformEvent::MouseUp {
            button: MouseButton::Right,
            x: 140.0,
            y: 110.0,
        },
        &mut runtime,
        &mut compositor,
        &mut presenter,
        &mut hook,
    );

    let events = log.borrow();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].0, "down");
    assert_eq!(events[0].1, 2);
    assert!((events[0].2 - 120.0).abs() < 1e-3);
    assert!((events[0].3 - 60.0).abs() < 1e-3);
    assert!((events[0].4 - 640.0).abs() < 1e-3);
    assert!((events[0].5 - 400.0).abs() < 1e-3);

    assert_eq!(events[1].0, "up");
    assert_eq!(events[1].1, 2);
    assert!((events[1].4 - 640.0).abs() < 1e-3);
    assert!((events[1].5 - 400.0).abs() < 1e-3);
}

#[test]
fn test_canvas3d_pointer_events_none_overlay_fast_path_and_hover_reconcile() {
    use std::cell::Cell;
    use std::rc::Rc;
    use strata::render::scene3d::{RetainedCameraUniforms, RetainedScene3D};

    let move_hits = Rc::new(Cell::new(0usize));
    let wheel_hits = Rc::new(Cell::new(0usize));
    let retained = RetainedScene3D::new(RetainedCameraUniforms::default());
    let shared_scene = std::sync::Arc::new(Scene3D::default().with_retained_handle(retained));

    let make_view = {
        let move_hits = Rc::clone(&move_hits);
        let wheel_hits = Rc::clone(&wheel_hits);
        let shared_scene = std::sync::Arc::clone(&shared_scene);
        move || {
            let move_cb = Rc::clone(&move_hits);
            let wheel_cb = Rc::clone(&wheel_hits);
            let canvas = Canvas3D::builder()
                .width(1280.0)
                .height(800.0)
                .scene(&shared_scene)
                .onmousemove(move |_| {
                    move_cb.set(move_cb.get() + 1);
                })
                .onwheel(move |_| {
                    wheel_cb.set(wheel_cb.get() + 1);
                })
                .build();

            rsx! {
                <div class="relative w-full h-full">
                    {canvas}
                    <div class="absolute inset-0 z-10 pointer-events-none overflow-hidden">
                        <div class="w-40 h-12 pointer-events-auto">
                            <span>"144 FPS"</span>
                        </div>
                    </div>
                </div>
            }
        }
    };

    let mut runtime = AppRuntime::new(make_view(), 1280.0, 800.0, 1.0);
    runtime.tick_frame();
    let mut compositor = SwapchainCompositor::new(1280, 800, 1.0);
    let mut presenter = SoftwareSurfacePresenter::new(1280, 800, SwapchainFormat::Bgra8Unorm);
    let mut hook = |_: &PlatformEvent, _: &mut AppRuntime| false;

    assert!(runtime.target_is_retained_canvas3d(640.0, 400.0));
    assert!(!runtime.target_is_retained_canvas3d(20.0, 20.0));

    let _ = strata_dom::reactive::take_dirty_signals();
    let sig = strata_dom::reactive::Signal::new(0u64);
    sig.set(1);
    assert!(strata_dom::reactive::has_dirty_signals());
    let _ = strata_dom::reactive::take_dirty_signals();
    assert!(!strata_dom::reactive::has_dirty_signals());

    let (rebuild, _redraw, exit) = dispatch_native_platform_event(
        &PlatformEvent::MouseMove { x: 640.0, y: 400.0 },
        &mut runtime,
        &mut compositor,
        &mut presenter,
        &mut hook,
    );
    assert!(!exit);
    assert!(!rebuild);
    assert_eq!(move_hits.get(), 1);
    assert!(runtime.current_hover_path.is_some());

    let (rebuild, _redraw, exit) = dispatch_native_platform_event(
        &PlatformEvent::Scroll {
            delta_x: 0.0,
            delta_y: -120.0,
            is_momentum: false,
        },
        &mut runtime,
        &mut compositor,
        &mut presenter,
        &mut hook,
    );
    assert!(!exit);
    assert!(!rebuild);
    assert_eq!(wheel_hits.get(), 1);

    assert!(!runtime.compile_display_list_if_dirty() || !runtime.is_paint_dirty());
    runtime.update_view(make_view());
    assert!(runtime.last_update_reconciled());
}

#[test]
fn test_reconcile_preserves_and_propagates_inherited_text_color() {
    use strata_render::compositor::DrawCommand;

    let build = |cls: &str, r: u8, g: u8, b: u8, alpha: &str, label: &str, with_dot: bool| -> View {
        let style_str = format!(
            "left: 120px; top: 80px; background-color: rgba(0,0,0,0.25); color: rgba({r},{g},{b},{alpha}); font-size: 13px;"
        );
        let span_cls = format!("select-none {}", cls);
        let text_owned = label.to_string();
        if with_dot {
            rsx! {
                <div class="relative w-full h-full bg-black">
                    <div class="absolute flex items-center gap-1.5" style={style_str}>
                        <div class="w-1.5 h-1.5 rounded-full bg-white" />
                        <span class={span_cls}>{text_owned}</span>
                    </div>
                </div>
            }
        } else {
            rsx! {
                <div class="relative w-full h-full bg-black">
                    <div class="absolute flex items-center gap-1.5" style={style_str}>
                        <span class={span_cls}>{text_owned}</span>
                    </div>
                </div>
            }
        }
    };

    let extract_text_colors = |rt: &AppRuntime| -> Vec<strata_primitives::Rgba> {
        let mut out = Vec::new();
        for cmd in rt.display_list.commands() {
            match cmd {
                DrawCommand::DrawText { color, .. } => out.push(*color),
                DrawCommand::DrawTextFragments { fragments } => {
                    for f in fragments {
                        out.push(f.style.color);
                    }
                }
                _ => {}
            }
        }
        out
    };

    // 1. Start near horizon (faint white alpha=0.05) with Continent class.
    let mut rt = AppRuntime::new(
        build("font-semibold tracking-widest", 255, 255, 255, "0.05", "EUROPE", false),
        1440.0,
        900.0,
        1.0,
    );

    // 2. Parent style fades to bright alpha=0.95 while child span & text are unchanged:
    //    inherited alpha MUST propagate down to text (not stay stuck at 0.05!).
    rt.update_view(build(
        "font-semibold tracking-widest",
        255,
        255,
        255,
        "0.95",
        "EUROPE",
        false,
    ));
    assert!(rt.last_update_reconciled());
    let cols1 = extract_text_colors(&rt);
    assert!(!cols1.is_empty());
    for c in &cols1 {
        assert_eq!((c.r, c.g, c.b), (255, 255, 255));
        assert!(c.a > 220, "expected bright propagated alpha, got {c:?}");
    }

    // 3. Child span changes class (Continent -> Ocean italic) AND parent color changes to light-blue:
    //    rebuilt shallow span MUST inherit light-blue from parent, NEVER Rgba::BLACK!
    rt.update_view(build(
        "italic tracking-wider",
        148,
        197,
        255,
        "0.92",
        "ATLANTIC OCEAN",
        false,
    ));
    assert!(rt.last_update_reconciled());
    let cols2 = extract_text_colors(&rt);
    assert!(!cols2.is_empty());
    for c in &cols2 {
        assert_eq!(
            (c.r, c.g, c.b),
            (148, 197, 255),
            "reconciled span/text lost parent color and turned {c:?}"
        );
        assert!(c.a > 210);
    }

    // 4. Child count grows (None -> CityDot with leading dot + span):
    //    newly appended child subtree MUST inherit parent color, NEVER Rgba::BLACK!
    rt.update_view(build(
        "font-medium",
        245,
        235,
        210,
        "0.96",
        "Istanbul",
        true,
    ));
    assert!(rt.last_update_reconciled());
    let cols3 = extract_text_colors(&rt);
    assert!(!cols3.is_empty());
    for c in &cols3 {
        assert_eq!(
            (c.r, c.g, c.b),
            (245, 235, 210),
            "appended child subtree lost parent color and turned {c:?}"
        );
        assert!(c.a > 220);
    }
}

