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
    let mut presenter =
        SoftwareSurfacePresenter::new(800, 600, SwapchainFormat::Bgra8Unorm);
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
    let mut presenter =
        SoftwareSurfacePresenter::new(800, 600, SwapchainFormat::Bgra8Unorm);
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
    let mut presenter =
        SoftwareSurfacePresenter::new(800, 600, SwapchainFormat::Bgra8Unorm);
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
    let mut presenter =
        SoftwareSurfacePresenter::new(800, 600, SwapchainFormat::Bgra8Unorm);
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
    let mut presenter =
        SoftwareSurfacePresenter::new(800, 600, SwapchainFormat::Bgra8Unorm);
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

