//! Integration test suite verifying Client-Side Decorations (CSD),
//! titlebar dragging, double-click maximize/restore toggle, and multi-zone hit testing in Strata runtime.

use strata::prelude::*;
use strata::runtime::platforms::dispatch_native_platform_event;
use strata_platform::csd::CsdHitZone;
use strata_platform::window::{MockPlatformWindow, PlatformWindow, WindowConfig, WindowMode};
use strata_platform::{
    PlatformEvent, SoftwareSurfacePresenter, SwapchainCompositor, SwapchainFormat,
};

fn csd_sample_view() -> View {
    rsx! {
        <div class="w-full h-full bg-slate-900 text-white flex-col">
            <header class="w-full h-8 bg-slate-800 flex items-center px-3" data-app-region="drag">
                <span class="text-sm font-semibold">"Strata Frameless App"</span>
            </header>
            <main class="flex-1 p-4">
                <h1 class="text-2xl font-bold">"Client Content"</h1>
            </main>
        </div>
    }
}

#[test]
fn test_dispatch_native_window_maximized_event() {
    let mut runtime = AppRuntime::new(csd_sample_view(), 800.0, 600.0, 1.0);
    let mut compositor = SwapchainCompositor::new(800, 600, 1.0);
    let mut presenter = SoftwareSurfacePresenter::new(800, 600, SwapchainFormat::Bgra8Unorm);
    let mut hook = |_: &PlatformEvent, _: &mut AppRuntime| false;

    // Dispatch WindowMaximized(true)
    let max_event = PlatformEvent::WindowMaximized(true);
    let (rebuild, redraw, exit) = dispatch_native_platform_event(
        &max_event,
        &mut runtime,
        &mut compositor,
        &mut presenter,
        &mut hook,
    );
    assert!(rebuild);
    assert!(redraw);
    assert!(!exit);

    // Dispatch WindowMaximized(false)
    let restore_event = PlatformEvent::WindowMaximized(false);
    let (rebuild2, redraw2, exit2) = dispatch_native_platform_event(
        &restore_event,
        &mut runtime,
        &mut compositor,
        &mut presenter,
        &mut hook,
    );
    assert!(rebuild2);
    assert!(redraw2);
    assert!(!exit2);
}

#[test]
fn test_runtime_titlebar_drag_and_double_click_candidate() {
    let mut runtime = AppRuntime::new(csd_sample_view(), 800.0, 600.0, 1.0);

    // First click on titlebar region (y = 16.0 within titlebar)
    let dbl_click = runtime.begin_window_drag_candidate(200.0, 16.0);
    assert!(!dbl_click);
    assert!(runtime.pending_window_drag.is_some());

    // Movement exceeding threshold starts window drag and consumes pending drag
    let _ = runtime.poll_window_drag(210.0, 25.0);
    assert!(runtime.pending_window_drag.is_none());

    // Rapid double-click within 400ms threshold
    let _ = runtime.begin_window_drag_candidate(300.0, 16.0);
    let is_double = runtime.begin_window_drag_candidate(302.0, 17.0);
    assert!(is_double);
    assert!(runtime.last_drag_click.is_none());
}

#[test]
fn test_runtime_csd_controller_hit_testing() {
    let runtime = AppRuntime::new(csd_sample_view(), 1024.0, 768.0, 1.0);

    // Titlebar zone
    let titlebar_zone = runtime.csd_controller.hit_test(1024.0, 768.0, 400.0, 16.0);
    assert_eq!(titlebar_zone, CsdHitZone::Titlebar);

    // Top-left resize corner
    let corner_zone = runtime.csd_controller.hit_test(1024.0, 768.0, 2.0, 2.0);
    assert_eq!(corner_zone, CsdHitZone::ResizeCornerTopLeft);

    // Bottom resize border
    let bottom_zone = runtime.csd_controller.hit_test(1024.0, 768.0, 500.0, 766.0);
    assert_eq!(bottom_zone, CsdHitZone::ResizeBorderBottom);

    // Close button (right side)
    let close_zone = runtime.csd_controller.hit_test(1024.0, 768.0, 1000.0, 16.0);
    assert_eq!(close_zone, CsdHitZone::CloseButton);

    // Client body
    let client_zone = runtime.csd_controller.hit_test(1024.0, 768.0, 500.0, 400.0);
    assert_eq!(client_zone, CsdHitZone::None);
}

#[test]
fn test_mock_window_loop_csd_maximize_and_drag() {
    let config = WindowConfig {
        title: "CSD Integration Window".to_string(),
        width: 800,
        height: 600,
        mode: WindowMode::Windowed,
        ..Default::default()
    };
    let mut window = MockPlatformWindow::new(config);

    assert_eq!(window.mode(), WindowMode::Windowed);

    // Test maximize toggle
    let now_max = window.toggle_maximize();
    assert!(now_max);
    assert_eq!(window.mode(), WindowMode::Maximized);

    // Test restore toggle
    let now_restored = window.toggle_maximize();
    assert!(!now_restored);
    assert_eq!(window.mode(), WindowMode::Windowed);

    // Drain events
    let events = window.poll_events();
    assert!(events.contains(&PlatformEvent::WindowMaximized(true)));
    assert!(events.contains(&PlatformEvent::WindowMaximized(false)));
}

#[test]
fn test_no_drag_overrides_csd_titlebar_zone() {
    let view = rsx! {
        <div class="w-full h-full flex flex-col">
            <header class="w-full h-9 flex items-center" data-app-region="drag">
                <button class="w-[100px] h-9" data-app-region="no-drag">"File"</button>
                <div class="flex-1 h-9" data-app-region="drag" />
            </header>
        </div>
    };
    let mut runtime = AppRuntime::new(view, 800.0, 600.0, 1.0);

    // Clicking inside the 100px no-drag button MUST NOT begin a window drag candidate
    // even though y=18 lies within the 35px CsdHitZone::Titlebar strip.
    assert_eq!(
        runtime.csd_controller.hit_test(800.0, 600.0, 40.0, 18.0),
        CsdHitZone::Titlebar
    );
    let dbl = runtime.begin_window_drag_candidate(40.0, 18.0);
    assert!(!dbl);
    assert!(
        runtime.pending_window_drag.is_none(),
        "data-app-region=no-drag must suppress CSD titlebar drag"
    );

    // Clicking on the draggable spacer still initiates drag candidate
    let _ = runtime.begin_window_drag_candidate(300.0, 18.0);
    assert!(runtime.pending_window_drag.is_some());
}

#[test]
fn test_mousedown_multiclick_detail_and_mousemove_buttons() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let details: Rc<RefCell<Vec<i32>>> = Rc::new(RefCell::new(Vec::new()));
    let move_buttons: Rc<RefCell<Vec<u16>>> = Rc::new(RefCell::new(Vec::new()));

    let d_clone = details.clone();
    let m_clone = move_buttons.clone();
    let root = NodeBlueprint::element(
        Tag::Div,
        vec!["w-full h-full".to_string()],
        vec![
            Attribute::event_listener(EventType::MouseDown, move |e: Event| {
                d_clone.borrow_mut().push(e.detail);
            }),
            Attribute::event_listener(EventType::MouseMove, move |e: Event| {
                m_clone.borrow_mut().push(e.buttons);
            }),
        ],
        Vec::new(),
    );
    let mut runtime = AppRuntime::new(View::new(root), 800.0, 600.0, 1.0);

    // Hover move without pressing mouse: buttons must be 0
    runtime.is_mouse_down = false;
    let _ = runtime.dispatch_pointer_move_action(100.0, 100.0);
    assert_eq!(move_buttons.borrow().as_slice(), &[0]);

    // Rapid 1st, 2nd, 3rd clicks at same spot -> detail 1, 2, 3, then wraps to 1
    let mods = strata_platform::Modifiers::default();
    runtime.is_mouse_down = true;
    let _ = runtime.dispatch_pointer_down_action_with_button_and_modifiers(100.0, 100.0, 0, mods);
    let _ = runtime.dispatch_pointer_move_action(102.0, 100.0);
    runtime.is_mouse_down = false;
    let _ = runtime.dispatch_pointer_up_action_with_button(102.0, 100.0, 0);

    runtime.is_mouse_down = true;
    let _ = runtime.dispatch_pointer_down_action_with_button_and_modifiers(101.0, 100.0, 0, mods);
    runtime.is_mouse_down = false;
    let _ = runtime.dispatch_pointer_up_action_with_button(101.0, 100.0, 0);

    runtime.is_mouse_down = true;
    let _ = runtime.dispatch_pointer_down_action_with_button_and_modifiers(101.0, 101.0, 0, mods);
    runtime.is_mouse_down = false;
    let _ = runtime.dispatch_pointer_up_action_with_button(101.0, 101.0, 0);

    runtime.is_mouse_down = true;
    let _ = runtime.dispatch_pointer_down_action_with_button_and_modifiers(101.0, 101.0, 0, mods);

    assert_eq!(details.borrow().as_slice(), &[1, 2, 3, 1]);
    // Second move occurred while is_mouse_down == true -> buttons == 1
    assert_eq!(move_buttons.borrow().as_slice(), &[0, 1]);
}

#[test]
fn test_mouseenter_signal_mutation_marks_dirty_signals() {
    let active_menu = strata_dom::reactive::Signal::new("File");
    let sig_clone = active_menu.clone();
    let root = NodeBlueprint::element(
        Tag::Div,
        vec!["w-full h-full flex".to_string()],
        Vec::new(),
        vec![NodeBlueprint::element(
            Tag::Button,
            vec!["w-[80px] h-[30px]".to_string()],
            vec![Attribute::event_listener(EventType::MouseEnter, move |_| {
                sig_clone.set("Edit");
            })],
            vec![NodeBlueprint::text("Edit")],
        )],
    );
    let mut runtime = AppRuntime::new(View::new(root), 400.0, 200.0, 1.0);
    let _ = strata_dom::reactive::take_dirty_signals();

    let _ = runtime.update_hover(40.0, 15.0);
    assert_eq!(active_menu.get(), "Edit");
    assert!(
        strata_dom::reactive::has_dirty_signals(),
        "MouseEnter signal write must dirty reactive graph for immediate rebuild"
    );
    let _ = strata_dom::reactive::take_dirty_signals();
}

