//! Comprehensive integration test suite for Client-Side Decorations (CSD),
//! multi-zone hit testing, edge resize arbitration, window dragging,
//! and native platform protocol translations across X11, Wayland, and Win32.

use strata_platform::csd::{ControlPlacement, CsdController, CsdGeometry, CsdHitZone};
use strata_platform::window::{
    CursorIcon, MockPlatformWindow, PlatformWindow, WindowConfig, WindowMode,
};
use strata_platform::PlatformEvent;

#[test]
fn test_csd_geometry_creation_and_bounds() {
    let geom = CsdGeometry::new(32.0, 46.0, 32.0, 6.0, ControlPlacement::Right);
    assert_eq!(geom.titlebar_height, 32.0);
    assert_eq!(geom.button_width, 46.0);
    assert_eq!(geom.button_height, 32.0);
    assert_eq!(geom.resize_border_thickness, 6.0);
    assert_eq!(geom.placement, ControlPlacement::Right);

    let macos = CsdGeometry::macos_style();
    assert_eq!(macos.placement, ControlPlacement::Left);
    assert_eq!(macos.titlebar_height, 28.0);
}

#[test]
fn test_csd_multi_zone_hit_testing() {
    let geom = CsdGeometry::new(40.0, 40.0, 40.0, 8.0, ControlPlacement::Right);
    let (w, h) = (1000.0, 800.0);

    // Corners
    assert_eq!(
        geom.hit_test(w, h, 2.0, 2.0),
        CsdHitZone::ResizeCornerTopLeft
    );
    assert_eq!(
        geom.hit_test(w, h, 998.0, 2.0),
        CsdHitZone::ResizeCornerTopRight
    );
    assert_eq!(
        geom.hit_test(w, h, 2.0, 798.0),
        CsdHitZone::ResizeCornerBottomLeft
    );
    assert_eq!(
        geom.hit_test(w, h, 998.0, 798.0),
        CsdHitZone::ResizeCornerBottomRight
    );

    // Borders
    assert_eq!(geom.hit_test(w, h, 500.0, 3.0), CsdHitZone::ResizeBorderTop);
    assert_eq!(
        geom.hit_test(w, h, 500.0, 797.0),
        CsdHitZone::ResizeBorderBottom
    );
    assert_eq!(
        geom.hit_test(w, h, 3.0, 400.0),
        CsdHitZone::ResizeBorderLeft
    );
    assert_eq!(
        geom.hit_test(w, h, 997.0, 400.0),
        CsdHitZone::ResizeBorderRight
    );

    // Window controls (Right side by default: Close, Maximize, Minimize)
    // Close button: 1000 - 40 = 960 to 1000
    assert_eq!(geom.hit_test(w, h, 970.0, 20.0), CsdHitZone::CloseButton);
    // Maximize button: 960 - 40 = 920 to 960
    assert_eq!(geom.hit_test(w, h, 930.0, 20.0), CsdHitZone::MaximizeButton);
    // Minimize button: 920 - 40 = 880 to 920
    assert_eq!(geom.hit_test(w, h, 890.0, 20.0), CsdHitZone::MinimizeButton);

    // Titlebar drag area
    assert_eq!(geom.hit_test(w, h, 300.0, 20.0), CsdHitZone::Titlebar);

    // Client area (outside titlebar and resize borders)
    assert_eq!(geom.hit_test(w, h, 500.0, 400.0), CsdHitZone::None);

    // Outside bounds
    assert_eq!(geom.hit_test(w, h, -5.0, 20.0), CsdHitZone::None);
    assert_eq!(geom.hit_test(w, h, 1050.0, 20.0), CsdHitZone::None);
}

#[test]
fn test_csd_macos_hit_testing() {
    let geom = CsdGeometry::macos_style();
    let (w, h) = (800.0, 600.0);

    // Left controls: Close (0..24), Minimize (24..48), Maximize (48..72)
    assert_eq!(geom.hit_test(w, h, 10.0, 14.0), CsdHitZone::CloseButton);
    assert_eq!(geom.hit_test(w, h, 30.0, 14.0), CsdHitZone::MinimizeButton);
    assert_eq!(geom.hit_test(w, h, 60.0, 14.0), CsdHitZone::MaximizeButton);
    assert_eq!(geom.hit_test(w, h, 200.0, 14.0), CsdHitZone::Titlebar);
}

#[test]
fn test_csd_cursor_mappings() {
    assert_eq!(
        CsdHitZone::ResizeCornerTopLeft.cursor_icon(),
        CursorIcon::ResizeNw
    );
    assert_eq!(
        CsdHitZone::ResizeCornerBottomRight.cursor_icon(),
        CursorIcon::ResizeNw
    );
    assert_eq!(
        CsdHitZone::ResizeCornerTopRight.cursor_icon(),
        CursorIcon::ResizeNe
    );
    assert_eq!(
        CsdHitZone::ResizeCornerBottomLeft.cursor_icon(),
        CursorIcon::ResizeNe
    );
    assert_eq!(
        CsdHitZone::ResizeBorderTop.cursor_icon(),
        CursorIcon::ResizeN
    );
    assert_eq!(
        CsdHitZone::ResizeBorderBottom.cursor_icon(),
        CursorIcon::ResizeN
    );
    assert_eq!(
        CsdHitZone::ResizeBorderLeft.cursor_icon(),
        CursorIcon::ResizeE
    );
    assert_eq!(
        CsdHitZone::ResizeBorderRight.cursor_icon(),
        CursorIcon::ResizeE
    );
    assert_eq!(CsdHitZone::Titlebar.cursor_icon(), CursorIcon::Default);
    assert_eq!(CsdHitZone::None.cursor_icon(), CursorIcon::Default);
    assert_eq!(CsdHitZone::CloseButton.cursor_icon(), CursorIcon::Pointer);
    assert_eq!(
        CsdHitZone::MaximizeButton.cursor_icon(),
        CursorIcon::Pointer
    );
    assert_eq!(
        CsdHitZone::MinimizeButton.cursor_icon(),
        CursorIcon::Pointer
    );
}

#[test]
fn test_csd_x11_moveresize_mappings() {
    assert_eq!(
        CsdHitZone::ResizeCornerTopLeft.to_x11_moveresize_direction(),
        Some(0)
    );
    assert_eq!(
        CsdHitZone::ResizeBorderTop.to_x11_moveresize_direction(),
        Some(1)
    );
    assert_eq!(
        CsdHitZone::ResizeCornerTopRight.to_x11_moveresize_direction(),
        Some(2)
    );
    assert_eq!(
        CsdHitZone::ResizeBorderRight.to_x11_moveresize_direction(),
        Some(3)
    );
    assert_eq!(
        CsdHitZone::ResizeCornerBottomRight.to_x11_moveresize_direction(),
        Some(4)
    );
    assert_eq!(
        CsdHitZone::ResizeBorderBottom.to_x11_moveresize_direction(),
        Some(5)
    );
    assert_eq!(
        CsdHitZone::ResizeCornerBottomLeft.to_x11_moveresize_direction(),
        Some(6)
    );
    assert_eq!(
        CsdHitZone::ResizeBorderLeft.to_x11_moveresize_direction(),
        Some(7)
    );
    assert_eq!(CsdHitZone::Titlebar.to_x11_moveresize_direction(), Some(8));
    assert_eq!(CsdHitZone::None.to_x11_moveresize_direction(), None);
    assert_eq!(CsdHitZone::CloseButton.to_x11_moveresize_direction(), None);
}

#[test]
fn test_csd_wayland_resize_edge_mappings() {
    assert_eq!(
        CsdHitZone::ResizeBorderTop.to_wayland_resize_edge(),
        Some(1)
    );
    assert_eq!(
        CsdHitZone::ResizeBorderBottom.to_wayland_resize_edge(),
        Some(2)
    );
    assert_eq!(
        CsdHitZone::ResizeBorderLeft.to_wayland_resize_edge(),
        Some(4)
    );
    assert_eq!(
        CsdHitZone::ResizeCornerTopLeft.to_wayland_resize_edge(),
        Some(5)
    );
    assert_eq!(
        CsdHitZone::ResizeCornerBottomLeft.to_wayland_resize_edge(),
        Some(6)
    );
    assert_eq!(
        CsdHitZone::ResizeBorderRight.to_wayland_resize_edge(),
        Some(8)
    );
    assert_eq!(
        CsdHitZone::ResizeCornerTopRight.to_wayland_resize_edge(),
        Some(9)
    );
    assert_eq!(
        CsdHitZone::ResizeCornerBottomRight.to_wayland_resize_edge(),
        Some(10)
    );
    assert_eq!(CsdHitZone::Titlebar.to_wayland_resize_edge(), None);
    assert_eq!(CsdHitZone::None.to_wayland_resize_edge(), None);
}

#[test]
fn test_csd_win32_nchittest_mappings() {
    assert_eq!(CsdHitZone::None.to_win32_nchittest(), 1); // HTCLIENT
    assert_eq!(CsdHitZone::Titlebar.to_win32_nchittest(), 2); // HTCAPTION
    assert_eq!(CsdHitZone::ResizeBorderLeft.to_win32_nchittest(), 10); // HTLEFT
    assert_eq!(CsdHitZone::ResizeBorderRight.to_win32_nchittest(), 11); // HTRIGHT
    assert_eq!(CsdHitZone::ResizeBorderTop.to_win32_nchittest(), 12); // HTTOP
    assert_eq!(CsdHitZone::ResizeCornerTopLeft.to_win32_nchittest(), 13); // HTTOPLEFT
    assert_eq!(CsdHitZone::ResizeCornerTopRight.to_win32_nchittest(), 14); // HTTOPRIGHT
    assert_eq!(CsdHitZone::ResizeBorderBottom.to_win32_nchittest(), 15); // HTBOTTOM
    assert_eq!(CsdHitZone::ResizeCornerBottomLeft.to_win32_nchittest(), 16); // HTBOTTOMLEFT
    assert_eq!(CsdHitZone::ResizeCornerBottomRight.to_win32_nchittest(), 17); // HTBOTTOMRIGHT
    assert_eq!(CsdHitZone::CloseButton.to_win32_nchittest(), 20); // HTCLOSE
    assert_eq!(CsdHitZone::MinimizeButton.to_win32_nchittest(), 8); // HTMINBUTTON
    assert_eq!(CsdHitZone::MaximizeButton.to_win32_nchittest(), 9); // HTMAXBUTTON
}

#[test]
fn test_csd_controller_drag_tracking() {
    let geom = CsdGeometry::new(30.0, 40.0, 30.0, 5.0, ControlPlacement::Right);
    let mut controller = CsdController::new(geom);

    // Initial state: not dragging
    assert!(!controller.is_dragging);
    assert_eq!(controller.active_zone, CsdHitZone::None);

    // Press down in titlebar
    let zone = controller.on_pointer_down(800.0, 600.0, 200.0, 15.0);
    assert_eq!(zone, CsdHitZone::Titlebar);
    assert_eq!(controller.active_zone, CsdHitZone::Titlebar);

    // Small movement below threshold (threshold is 4.0)
    assert!(!controller.should_start_drag(202.0, 16.0));
    assert!(!controller.is_dragging);

    // Movement exceeding threshold initiates drag
    assert!(controller.should_start_drag(210.0, 25.0));
    assert!(controller.is_dragging);

    // Release pointer terminates drag
    controller.on_pointer_up();
    assert!(!controller.is_dragging);
    assert_eq!(controller.active_zone, CsdHitZone::None);
    assert_eq!(controller.drag_start_pos, None);
}

#[test]
fn test_mock_platform_window_csd_lifecycle() {
    let config = WindowConfig {
        title: "CSD Test Window".to_string(),
        width: 1024,
        height: 768,
        mode: WindowMode::Windowed,
        ..Default::default()
    };
    let mut window = MockPlatformWindow::new(config);

    assert_eq!(window.mode(), WindowMode::Windowed);

    // Toggle maximize (first time -> Maximized)
    let is_max = window.toggle_maximize();
    assert!(is_max);
    assert_eq!(window.mode(), WindowMode::Maximized);

    // Verify PlatformEvent::WindowMaximized was dispatched
    let events = window.poll_events();
    assert!(events
        .iter()
        .any(|e| matches!(e, PlatformEvent::WindowMaximized(true))));

    // Toggle maximize again (second time -> Windowed)
    let is_max2 = window.toggle_maximize();
    assert!(!is_max2);
    assert_eq!(window.mode(), WindowMode::Windowed);

    let events2 = window.poll_events();
    assert!(events2
        .iter()
        .any(|e| matches!(e, PlatformEvent::WindowMaximized(false))));

    // Verify begin_window_drag and begin_window_resize execute without panic
    window.begin_window_drag(200.0, 15.0);
    window.begin_window_resize(CsdHitZone::ResizeBorderBottom, 500.0, 765.0);
}
