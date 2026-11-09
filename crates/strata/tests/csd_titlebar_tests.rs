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
