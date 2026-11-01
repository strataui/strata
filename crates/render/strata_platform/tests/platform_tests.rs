use strata_platform::prelude::*;

#[test]
fn test_window_mode_and_config() {
    let mut config = WindowConfig::default();
    assert_eq!(config.width, 1280);
    assert_eq!(config.height, 800);
    assert_eq!(config.mode, WindowMode::Windowed);
    assert_eq!(config.decoration, WindowDecoration::NativeOs);
    assert_eq!(config.theme, ThemeMode::System);

    config.mode = WindowMode::BorderlessFullscreen;
    config.decoration = WindowDecoration::ClientSideDecorations;
    config.theme = ThemeMode::Dark;
    config.transparent = true;

    assert_eq!(config.mode, WindowMode::BorderlessFullscreen);
    assert_eq!(config.decoration, WindowDecoration::ClientSideDecorations);
    assert_eq!(config.theme, ThemeMode::Dark);
    assert!(config.transparent);
}

#[test]
fn test_mock_platform_window_lifecycle_and_events() {
    let mut window = MockPlatformWindow::new_mock();
    assert_eq!(window.title(), "Strata Application");
    assert_eq!(window.size(), (1280, 800));
    assert_eq!(window.scale_factor(), 1.0);
    assert!(window.is_focused());
    assert_eq!(window.cursor(), CursorIcon::Default);

    window.set_title("Strata Custom Window");
    assert_eq!(window.title(), "Strata Custom Window");

    window.set_size(1920, 1080);
    assert_eq!(window.size(), (1920, 1080));

    window.set_scale_factor(2.0);
    assert_eq!(window.scale_factor(), 2.0);

    window.set_cursor(CursorIcon::Pointer);
    assert_eq!(window.cursor(), CursorIcon::Pointer);

    window.set_focused(false);
    assert!(!window.is_focused());

    // Redraw flag
    assert!(!window.is_redraw_requested());
    window.request_redraw();
    assert!(window.is_redraw_requested());
    assert!(window.take_redraw_request());
    assert!(!window.is_redraw_requested());

    // Event queue
    window.inject_event(PlatformEvent::MouseMove { x: 100.0, y: 150.0 });
    window.inject_event(PlatformEvent::MouseDown {
        button: MouseButton::Left,
        x: 100.0,
        y: 150.0,
    });
    window.inject_event(PlatformEvent::KeyDown {
        key: KeyCode::KeyA,
        modifiers: Modifiers {
            shift: true,
            ctrl: false,
            alt: false,
            meta: false,
        },
    });

    let polled = window.poll_events();
    assert_eq!(polled.len(), 6); // WindowResized, ScaleFactorChanged, WindowFocused, MouseMove, MouseDown, KeyDown
    assert_eq!(polled[0], PlatformEvent::WindowResized { width: 1920, height: 1080 });
    assert_eq!(polled[1], PlatformEvent::ScaleFactorChanged { scale_factor: 2.0 });
    assert_eq!(polled[2], PlatformEvent::WindowFocused(false));
    assert_eq!(polled[3], PlatformEvent::MouseMove { x: 100.0, y: 150.0 });

    assert!(window.poll_events().is_empty());
}

#[test]
fn test_mock_event_loop_dispatch() {
    let mut event_loop = MockEventLoop::new();
    event_loop.queue_event(PlatformEvent::CursorEntered);
    event_loop.queue_event(PlatformEvent::ReceivedCharacter('S'));
    event_loop.queue_event(PlatformEvent::CursorLeft);

    let mut received_chars = Vec::new();
    let mut cursor_entered = false;
    let mut cursor_left = false;

    event_loop.run(|event, control| match event {
        PlatformEvent::CursorEntered => cursor_entered = true,
        PlatformEvent::ReceivedCharacter(c) => received_chars.push(c),
        PlatformEvent::CursorLeft => {
            cursor_left = true;
            *control = EventLoopControl::Exit;
        }
        _ => {}
    });

    assert!(cursor_entered);
    assert!(cursor_left);
    assert_eq!(received_chars, vec!['S']);
}

#[test]
fn test_csd_hit_testing_right_aligned() {
    let csd = CsdGeometry::default(); // Right-aligned, titlebar: 32, button_width: 46, border: 6
    let w = 800.0;
    let h = 600.0;

    // Corners
    assert_eq!(csd.hit_test(w, h, 2.0, 2.0), CsdHitZone::ResizeCornerTopLeft);
    assert_eq!(csd.hit_test(w, h, 798.0, 2.0), CsdHitZone::ResizeCornerTopRight);
    assert_eq!(csd.hit_test(w, h, 2.0, 598.0), CsdHitZone::ResizeCornerBottomLeft);
    assert_eq!(csd.hit_test(w, h, 798.0, 598.0), CsdHitZone::ResizeCornerBottomRight);

    // Borders
    assert_eq!(csd.hit_test(w, h, 400.0, 2.0), CsdHitZone::ResizeBorderTop);
    assert_eq!(csd.hit_test(w, h, 400.0, 598.0), CsdHitZone::ResizeBorderBottom);
    assert_eq!(csd.hit_test(w, h, 2.0, 300.0), CsdHitZone::ResizeBorderLeft);
    assert_eq!(csd.hit_test(w, h, 798.0, 300.0), CsdHitZone::ResizeBorderRight);

    // Titlebar & Control buttons (w=800, right buttons: 800 - 46 = 754..800 Close, 708..754 Max, 662..708 Min)
    assert_eq!(csd.hit_test(w, h, 780.0, 16.0), CsdHitZone::CloseButton);
    assert_eq!(csd.hit_test(w, h, 730.0, 16.0), CsdHitZone::MaximizeButton);
    assert_eq!(csd.hit_test(w, h, 680.0, 16.0), CsdHitZone::MinimizeButton);
    assert_eq!(csd.hit_test(w, h, 300.0, 16.0), CsdHitZone::Titlebar);

    // Interior client area
    assert_eq!(csd.hit_test(w, h, 400.0, 300.0), CsdHitZone::None);

    // Outside bounds
    assert_eq!(csd.hit_test(w, h, -10.0, 20.0), CsdHitZone::None);
    assert_eq!(csd.hit_test(w, h, 850.0, 20.0), CsdHitZone::None);

    // Cursor mapping checks
    assert_eq!(CsdHitZone::CloseButton.cursor_icon(), CursorIcon::Pointer);
    assert_eq!(CsdHitZone::ResizeBorderTop.cursor_icon(), CursorIcon::ResizeN);
    assert_eq!(CsdHitZone::ResizeCornerTopLeft.cursor_icon(), CursorIcon::ResizeNw);
    assert_eq!(CsdHitZone::ResizeCornerTopRight.cursor_icon(), CursorIcon::ResizeNe);
}

#[test]
fn test_csd_hit_testing_left_aligned_macos() {
    let csd = CsdGeometry::macos_style(); // Left-aligned, button_width: 24, border: 5
    let w = 800.0;
    let h = 600.0;

    // Buttons on left: 0..24 Close, 24..48 Min, 48..72 Max
    assert_eq!(csd.hit_test(w, h, 15.0, 14.0), CsdHitZone::CloseButton);
    assert_eq!(csd.hit_test(w, h, 35.0, 14.0), CsdHitZone::MinimizeButton);
    assert_eq!(csd.hit_test(w, h, 55.0, 14.0), CsdHitZone::MaximizeButton);
    assert_eq!(csd.hit_test(w, h, 200.0, 14.0), CsdHitZone::Titlebar);
}

#[test]
fn test_system_tray_and_nested_menu_actions() {
    let dummy_icon = vec![255u8; 16 * 16 * 4];
    let mut tray = TrayIcon::new("Strata Engine", dummy_icon, 16, 16);

    assert_eq!(tray.tooltip, "Strata Engine");
    assert!(tray.visible);
    assert_eq!(tray.click_count, 0);

    tray.trigger_click();
    assert_eq!(tray.click_count, 1);

    let mut menu = TrayMenu::new();
    menu.add_item(1, "Open Window")
        .add_separator()
        .add_submenu(
            2,
            "Settings",
            vec![
                TrayMenuItem::item(21, "Dark Theme").with_checked(true),
                TrayMenuItem::item(22, "Disable Notifications").with_enabled(false),
            ],
        )
        .add_separator()
        .add_item(99, "Quit");

    tray.set_menu(Some(menu));

    // Test finding items
    assert!(tray.menu.as_ref().unwrap().find_item(1).is_some());
    assert!(tray.menu.as_ref().unwrap().find_item(21).is_some());
    assert!(tray.menu.as_ref().unwrap().find_item(100).is_none());

    // Trigger action on enabled item
    let ok = tray.trigger_menu_action(1);
    assert!(ok);
    assert_eq!(tray.last_action_id, Some(1));

    // Trigger action on nested disabled item
    let disabled_ok = tray.trigger_menu_action(22);
    assert!(!disabled_ok);
    assert_eq!(tray.last_action_id, Some(1)); // Unchanged
}

#[test]
fn test_platform_target_detection_properties() {
    let target = PlatformTarget::Linux;
    assert!(target.is_desktop());
    assert!(!target.is_mobile());
    assert!(!target.is_web());

    let mobile_target = PlatformTarget::Android;
    assert!(mobile_target.is_mobile());
    assert!(!mobile_target.is_desktop());

    let web_target = PlatformTarget::WebWasm;
    assert!(web_target.is_web());
}
