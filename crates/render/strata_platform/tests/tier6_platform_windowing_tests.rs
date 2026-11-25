//! Tier 6 Integration Test Suite: Native Platform Windowing & OS Event Pumps
//!
//! Verifies Goals 20, 21, 22, 23, 25, 26, 27, and 28.

use strata_platform::csd::{ControlPlacement, CsdController, CsdGeometry, CsdHitZone};
use strata_platform::events::PlatformEvent;
use strata_platform::raw_abi::cocoa::{CvReturn, CvTimeStamp, NsRect, K_CV_RETURN_SUCCESS};
#[cfg(target_os = "linux")]
use strata_platform::raw_abi::wayland::WaylandHeader;
use strata_platform::raw_abi::win32::{Point, Rect};
use strata_platform::tray::{
    clear_delivered_notifications, get_delivered_notifications, DesktopNotification, TrayEvent,
    TrayEventChannel, TrayMenu, TrayMenuItem,
};
use strata_platform::vibrancy::{
    apply_vibrancy, clear_vibrancy, VibrancyConfig, VibrancyEffect, VibrancyError,
};
use strata_platform::window::{
    LivePlatformWindow, MockPlatformWindow, MonitorInfo, PlatformWindow, WindowConfig,
    WindowDecoration,
};

#[cfg(target_os = "linux")]
use strata_platform::wayland_window::{
    WaylandDecoder, WaylandEncoder, WL_DISPLAY_GET_REGISTRY, WL_DISPLAY_ID,
};

#[cfg(target_os = "linux")]
use strata_platform::x11_window::{
    X11ShmSegment, X11WireDecoder, X11WireEncoder, X11_OPCODE_CREATE_WINDOW,
};

// ============================================================================
// Goal 20: Native Pure-Rust Wayland Protocol Client Integration
// ============================================================================

#[test]
fn test_goal_20_pure_rust_wayland_wire_protocol_and_shm_pools() {
    #[cfg(target_os = "linux")]
    {
        // 1. Verify wire protocol header encoding
        let mut encoder = WaylandEncoder::new();
        encoder.begin_message();
        encoder.push_u32(42); // registry_id
        encoder.finish_message(WL_DISPLAY_ID, WL_DISPLAY_GET_REGISTRY);

        let bytes = encoder.as_bytes();
        assert_eq!(bytes.len(), 12); // 8 header + 4 payload

        let header = WaylandHeader::from_bytes(bytes).expect("Valid Wayland header");
        assert_eq!(header.object_id, WL_DISPLAY_ID);
        assert_eq!(header.opcode, WL_DISPLAY_GET_REGISTRY);
        assert_eq!(header.size, 12);

        // 2. Verify wire protocol decoding
        let mut decoder = WaylandDecoder::new(&bytes[8..]);
        assert_eq!(decoder.read_u32(), Some(42));
        assert_eq!(decoder.remaining(), 0);

        // 3. Verify Wayland string 4-byte padding
        let mut str_encoder = WaylandEncoder::new();
        str_encoder.begin_message();
        str_encoder.push_string("wl_compositor");
        str_encoder.finish_message(2, 0);
        let str_bytes = str_encoder.as_bytes();
        assert_eq!(str_bytes.len() % 4, 0);

        let mut str_decoder = WaylandDecoder::new(&str_bytes[8..]);
        assert_eq!(str_decoder.read_string(), Some("wl_compositor".to_string()));

        // 4. Verify Wayland fixed-point encoding (24.8)
        let mut fixed_encoder = WaylandEncoder::new();
        fixed_encoder.begin_message();
        fixed_encoder.push_fixed(128.5);
        fixed_encoder.finish_message(10, 0);

        let mut fixed_decoder = WaylandDecoder::new(&fixed_encoder.as_bytes()[8..]);
        let raw_fixed = fixed_decoder.read_i32().expect("fixed i32");
        let float_val = raw_fixed as f64 / 256.0;
        assert!((float_val - 128.5).abs() < 0.001);

        // 5. Verify POSIX shm buffer allocation and pixel write
        let mut shm = strata_platform::wayland_window::ShmBuffer::new(100, 200, 64, 64)
            .expect("ShmBuffer allocation");
        assert_eq!(shm.width, 64);
        assert_eq!(shm.height, 64);
        assert_eq!(shm.size, 64 * 64 * 4);

        let test_pixels = vec![0xAA; 64 * 64 * 4];
        shm.write_pixels(&test_pixels);

        unsafe {
            let read_slice = std::slice::from_raw_parts(shm.ptr, shm.size);
            assert_eq!(read_slice[0], 0xAA);
            assert_eq!(read_slice[shm.size - 1], 0xAA);
        }
    }
}

// ============================================================================
// Goal 21: Native Pure-Rust X11 Protocol Integration (xcb / MIT-SHM)
// ============================================================================

#[test]
fn test_goal_21_pure_rust_x11_mit_shm_presentation_and_event_decoding() {
    #[cfg(target_os = "linux")]
    {
        // 1. Verify X11 wire protocol request encoding
        let mut encoder = X11WireEncoder::new();
        encoder.begin_request(X11_OPCODE_CREATE_WINDOW, 24); // depth 24
        encoder.push_u32(0x00400001); // window id
        encoder.push_u32(0x00000100); // parent
        encoder.push_i16(100); // x
        encoder.push_i16(200); // y
        encoder.push_u16(800); // width
        encoder.push_u16(600); // height
        encoder.push_u16(0); // border width
        encoder.push_u16(1); // class (InputOutput)
        encoder.push_u32(0); // visual
        encoder.push_u32(0); // value mask
        encoder.finish_request();

        let req_bytes = encoder.as_bytes();
        assert_eq!(req_bytes[0], X11_OPCODE_CREATE_WINDOW);
        assert_eq!(req_bytes[1], 24);
        assert_eq!(req_bytes.len() % 4, 0);

        let words_len = u16::from_ne_bytes(req_bytes[2..4].try_into().unwrap());
        assert_eq!(words_len as usize, req_bytes.len() / 4);

        // 2. Verify X11 32-byte event packet decoding (ConfigureNotify opcode = 22)
        let mut event_packet = [0u8; 32];
        event_packet[0] = 22; // ConfigureNotify
        event_packet[8..12].copy_from_slice(&0x00400001u32.to_ne_bytes()); // window
        event_packet[16..18].copy_from_slice(&50i16.to_ne_bytes()); // x
        event_packet[18..20].copy_from_slice(&75i16.to_ne_bytes()); // y
        event_packet[20..22].copy_from_slice(&1024u16.to_ne_bytes()); // width
        event_packet[22..24].copy_from_slice(&768u16.to_ne_bytes()); // height

        let mut decoder = X11WireDecoder::new(&event_packet);
        let opcode = decoder.read_u8().unwrap();
        assert_eq!(opcode, 22);

        let _ = decoder.read_bytes(7);
        let window_id = decoder.read_u32().unwrap();
        assert_eq!(window_id, 0x00400001);

        let _ = decoder.read_bytes(4);
        let x = decoder.read_i16().unwrap();
        let y = decoder.read_i16().unwrap();
        let width = decoder.read_u16().unwrap();
        let height = decoder.read_u16().unwrap();
        assert_eq!((x, y), (50, 75));
        assert_eq!((width, height), (1024, 768));

        // 3. Verify MIT-SHM SysV shared memory segment allocation
        let mut shm_segment =
            X11ShmSegment::new(0x00400002, 128 * 128 * 4).expect("MIT-SHM segment allocation");
        assert_eq!(shm_segment.size, 128 * 128 * 4);

        let fill_pixels = vec![0x33; 128 * 128 * 4];
        shm_segment.write_pixels(&fill_pixels);

        unsafe {
            let read_slice = std::slice::from_raw_parts(shm_segment.ptr, shm_segment.size);
            assert_eq!(read_slice[0], 0x33);
            assert_eq!(read_slice[shm_segment.size - 1], 0x33);
        }
    }
}

// ============================================================================
// Goal 22: Authentic Win32 Platform Window & Message Dispatch Pipeline
// ============================================================================

#[test]
fn test_goal_22_win32_window_wndproc_message_dispatch_and_high_dpi() {
    // 1. Verify Win32 geometry ABI conversions
    let win_rect = Rect {
        left: 100,
        top: 200,
        right: 900,
        bottom: 800,
    };
    let geom_rect: strata_primitives::geom::Rect<f32> = win_rect.into();
    assert_eq!(geom_rect.x(), 100.0);
    assert_eq!(geom_rect.y(), 200.0);
    assert_eq!(geom_rect.width(), 800.0);
    assert_eq!(geom_rect.height(), 600.0);

    let roundtrip_rect: Rect = geom_rect.into();
    assert_eq!(roundtrip_rect, win_rect);

    let win_pt = Point { x: 450, y: 350 };
    let geom_pt: strata_primitives::geom::Point<f32> = win_pt.into();
    assert_eq!(geom_pt.x, 450.0);
    assert_eq!(geom_pt.y, 350.0);

    let roundtrip_pt: Point = geom_pt.into();
    assert_eq!(roundtrip_pt, win_pt);

    // 2. Verify Win32 message dispatch simulation via mock platform window
    let config = WindowConfig {
        title: "Win32 Test Application".to_string(),
        width: 1024,
        height: 768,
        decoration: WindowDecoration::ClientSideDecorations,
        ..Default::default()
    };

    let mut window = MockPlatformWindow::new(config);
    assert_eq!(window.title(), "Win32 Test Application");
    assert_eq!(window.size(), (1024, 768));
    assert_eq!(window.scale_factor(), 1.0);

    // Simulate WM_DPICHANGED (192 DPI -> 2.0x DPR)
    window.set_scale_factor(2.0);
    window.set_size(2048, 1536);

    let events = window.poll_events();
    assert_eq!(events.len(), 2);
    assert_eq!(
        events[0],
        PlatformEvent::ScaleFactorChanged { scale_factor: 2.0 }
    );
    assert_eq!(
        events[1],
        PlatformEvent::WindowResized {
            width: 2048,
            height: 1536
        }
    );

    // 3. Verify WM_NCHITTEST mapping via CsdHitZone
    let csd = CsdGeometry::default();
    let hit_title = csd.hit_test(1024.0, 768.0, 500.0, 15.0);
    assert_eq!(hit_title, CsdHitZone::Titlebar);
    assert_eq!(hit_title.to_win32_nchittest(), 2); // HTCAPTION

    let hit_close = csd.hit_test(1024.0, 768.0, 1000.0, 15.0);
    assert_eq!(hit_close, CsdHitZone::CloseButton);
    assert_eq!(hit_close.to_win32_nchittest(), 20); // HTCLOSE

    let hit_client = csd.hit_test(1024.0, 768.0, 500.0, 300.0);
    assert_eq!(hit_client, CsdHitZone::None);
    assert_eq!(hit_client.to_win32_nchittest(), 1); // HTCLIENT
}

// ============================================================================
// Goal 23: Authentic macOS Cocoa / AppKit Window & VSync RunLoop
// ============================================================================

#[test]
fn test_goal_23_macos_cocoa_nswindow_cvdisplaylink_pacing() {
    // 1. Verify Cocoa ABI structures
    let ns_rect = NsRect::new(50.0, 100.0, 1280.0, 800.0);
    assert_eq!(ns_rect.x, 50.0);
    assert_eq!(ns_rect.y, 100.0);
    assert_eq!(ns_rect.width, 1280.0);
    assert_eq!(ns_rect.height, 800.0);

    // 2. Verify CoreVideo CVTimeStamp representation
    let timestamp = CvTimeStamp {
        version: 0,
        video_time_scale: 120,
        video_time: 1,
        host_time: 1_000_000,
        rate_scalar: 1.0,
        video_refresh_period: 8_333_333, // 120 Hz interval (8.33 ms)
        smpte_time: [0; 16],
        flags: 0,
        reserved: 0,
    };
    assert_eq!(timestamp.video_time_scale, 120);
    assert_eq!(timestamp.video_refresh_period, 8_333_333);
    assert_eq!(K_CV_RETURN_SUCCESS, 0 as CvReturn);

    // 3. Verify macOS ProMotion 120Hz frame pacing controller
    #[cfg(target_os = "macos")]
    {
        let mut pacer = strata_platform::macos_window::CvDisplayLinkPacer::new();
        assert_eq!(pacer.target_fps(), 120.0);
        assert!(!pacer.is_running());

        pacer.start().expect("Start CVDisplayLink");
        assert!(pacer.is_running());

        pacer.set_target_fps(60.0);
        assert_eq!(pacer.target_fps(), 60.0);

        pacer.stop().expect("Stop CVDisplayLink");
        assert!(!pacer.is_running());
    }

    // 4. Verify macOS CSD left-aligned control layout
    let mac_csd = CsdGeometry::macos_style();
    assert_eq!(mac_csd.placement, ControlPlacement::Left);
    let hit_close = mac_csd.hit_test(800.0, 600.0, 10.0, 14.0);
    assert_eq!(hit_close, CsdHitZone::CloseButton);
}

// ============================================================================
// Goal 25: Dynamic Multi-Monitor High-DPI Scaling and Hot-Plug Resizing
// ============================================================================

#[test]
fn test_goal_25_dynamic_multi_monitor_high_dpi_scaling_and_hotplug() {
    let config = WindowConfig {
        width: 1280,
        height: 720,
        ..Default::default()
    };
    let mut window = MockPlatformWindow::new(config);

    // Check default monitor configuration
    assert_eq!(window.scale_factor(), 1.0);
    let primary = window.primary_monitor().expect("Primary monitor present");
    assert_eq!(primary.name, "Primary Display");
    assert_eq!(primary.scale_factor, 1.0);
    assert!(primary.is_primary);

    // Hot-plug secondary 4K HiDPI display
    let secondary = MonitorInfo::hidpi_4k();
    window.add_monitor(secondary.clone());

    let monitors = window.available_monitors();
    assert_eq!(monitors.len(), 2);
    assert_eq!(monitors[1].name, "4K UHD Display");
    assert_eq!(monitors[1].scale_factor, 2.0);

    // Migrate window from Primary Display to 4K Display (1.0x -> 2.0x scaling)
    window.move_to_monitor(2);
    assert_eq!(window.scale_factor(), 2.0);

    let current = window.current_monitor().expect("Current monitor updated");
    assert_eq!(current.id, 2);
    assert_eq!(current.scale_factor, 2.0);

    // Verify ScaleFactorChanged event emission
    let events = window.poll_events();
    assert!(events.contains(&PlatformEvent::ScaleFactorChanged { scale_factor: 2.0 }));

    // Test LivePlatformWindow multi-monitor queries
    let live_window = LivePlatformWindow::new(WindowConfig::default());
    assert_eq!(live_window.scale_factor(), 1.0);
    assert!(live_window.primary_monitor().is_some());
    assert_eq!(live_window.available_monitors().len(), 1);
}

// ============================================================================
// Goal 26: Client-Side Window Decorations (CSD) Multi-Zone Hit Testing
// ============================================================================

#[test]
fn test_goal_26_csd_multi_zone_hit_testing_and_native_drag_passthrough() {
    let csd = CsdGeometry::default();
    let width = 1000.0;
    let height = 700.0;

    // Corner hit tests
    assert_eq!(
        csd.hit_test(width, height, 2.0, 2.0),
        CsdHitZone::ResizeCornerTopLeft
    );
    assert_eq!(
        csd.hit_test(width, height, 998.0, 2.0),
        CsdHitZone::ResizeCornerTopRight
    );
    assert_eq!(
        csd.hit_test(width, height, 2.0, 698.0),
        CsdHitZone::ResizeCornerBottomLeft
    );
    assert_eq!(
        csd.hit_test(width, height, 998.0, 698.0),
        CsdHitZone::ResizeCornerBottomRight
    );

    // Border hit tests
    assert_eq!(
        csd.hit_test(width, height, 500.0, 2.0),
        CsdHitZone::ResizeBorderTop
    );
    assert_eq!(
        csd.hit_test(width, height, 500.0, 698.0),
        CsdHitZone::ResizeBorderBottom
    );
    assert_eq!(
        csd.hit_test(width, height, 2.0, 350.0),
        CsdHitZone::ResizeBorderLeft
    );
    assert_eq!(
        csd.hit_test(width, height, 998.0, 350.0),
        CsdHitZone::ResizeBorderRight
    );

    // Right-aligned button hit tests
    assert_eq!(
        csd.hit_test(width, height, 970.0, 16.0),
        CsdHitZone::CloseButton
    );
    assert_eq!(
        csd.hit_test(width, height, 930.0, 16.0),
        CsdHitZone::MaximizeButton
    );
    assert_eq!(
        csd.hit_test(width, height, 880.0, 16.0),
        CsdHitZone::MinimizeButton
    );
    assert_eq!(
        csd.hit_test(width, height, 500.0, 16.0),
        CsdHitZone::Titlebar
    );

    // Verify X11 _NET_WM_MOVERESIZE direction indices
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

    // Verify Wayland xdg_toplevel resize edge bitmasks
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

    // Drag controller hysteresis tracking
    let mut controller = CsdController::new(csd);
    let zone = controller.on_pointer_down(width, height, 500.0, 16.0);
    assert_eq!(zone, CsdHitZone::Titlebar);
    assert!(!controller.should_start_drag(502.0, 17.0)); // <= 4.0 threshold
    assert!(controller.should_start_drag(510.0, 25.0)); // > 4.0 threshold
    assert!(controller.is_dragging);

    controller.on_pointer_up();
    assert!(!controller.is_dragging);
}

// ============================================================================
// Goal 27: Cross-Platform System Tray & Desktop Notification Protocols
// ============================================================================

#[test]
fn test_goal_27_cross_platform_system_tray_and_desktop_notifications() {
    clear_delivered_notifications();
    assert_eq!(get_delivered_notifications().len(), 0);

    // 1. Build and dispatch DesktopNotification
    let notif = DesktopNotification::new("Project Strata", "Build Succeeded")
        .with_icon("dialog-information")
        .with_timeout(3000)
        .with_category("system.build");

    assert_eq!(notif.title, "Project Strata");
    assert_eq!(notif.body, "Build Succeeded");
    assert_eq!(notif.icon.as_deref(), Some("dialog-information"));
    assert_eq!(notif.timeout_ms, 3000);
    assert_eq!(notif.category.as_deref(), Some("system.build"));

    let id = notif.show().expect("Desktop notification dispatch");
    assert!(id >= 1);

    let delivered = get_delivered_notifications();
    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0].title, "Project Strata");
    assert_eq!(delivered[0].body, "Build Succeeded");

    // 2. Verify System Tray context menu hierarchy
    let mut menu = TrayMenu::new();
    menu.add_item(101, "Open Window");
    menu.add_separator();
    menu.add_submenu(
        102,
        "Themes",
        vec![
            TrayMenuItem::item(201, "Dark").with_checked(true),
            TrayMenuItem::item(202, "Light"),
        ],
    );
    menu.add_separator();
    menu.add_item(999, "Quit");

    assert_eq!(menu.items.len(), 5);
    assert!(menu.find_item(101).is_some());
    assert!(menu.find_item(201).is_some());
    assert!(menu.find_item(201).unwrap().checked);
    assert!(menu.find_item(999).is_some());

    // 3. Verify TrayEventChannel reactive communication
    let (channel, receiver) = TrayEventChannel::new();
    channel.dispatch(TrayEvent::LeftClick);
    channel.dispatch(TrayEvent::MenuItemTriggered(101));

    assert_eq!(receiver.try_recv(), Some(TrayEvent::LeftClick));
    assert_eq!(receiver.try_recv(), Some(TrayEvent::MenuItemTriggered(101)));
    assert_eq!(receiver.try_recv(), None);
}

// ============================================================================
// Goal 28: Operating System Vibrant / Acrylic / Mica Backdrop Blur
// ============================================================================

#[test]
fn test_goal_28_operating_system_vibrancy_mica_acrylic_backdrop_blur() {
    // 1. Verify VibrancyEffect host detection properties
    assert!(VibrancyEffect::None.is_supported_on_current_host());

    #[cfg(windows)]
    {
        assert!(VibrancyEffect::Mica.is_supported_on_current_host());
        assert!(VibrancyEffect::MicaAlt.is_supported_on_current_host());
        assert!(VibrancyEffect::Acrylic.is_supported_on_current_host());
        assert!(VibrancyEffect::BlurBehind.is_supported_on_current_host());
    }

    #[cfg(target_os = "macos")]
    {
        assert!(VibrancyEffect::UnderWindowBackground.is_supported_on_current_host());
        assert!(VibrancyEffect::HudWindow.is_supported_on_current_host());
        assert!(VibrancyEffect::FullScreenUI.is_supported_on_current_host());
    }

    // 2. Verify VibrancyConfig builder
    let config = VibrancyConfig::new(VibrancyEffect::Mica).with_dark_mode(true);
    assert_eq!(config.effect, VibrancyEffect::Mica);
    assert_eq!(config.dark_mode, Some(true));

    // 3. Verify invalid handle returns strongly typed error
    let res = apply_vibrancy(0, config);
    assert_eq!(res, Err(VibrancyError::InvalidWindowHandle));

    let clear_res = clear_vibrancy(0);
    assert_eq!(clear_res, Err(VibrancyError::InvalidWindowHandle));

    // 4. Verify window config integration with vibrancy
    let win_config = WindowConfig::default().with_vibrancy(VibrancyEffect::Acrylic);
    assert_eq!(win_config.vibrancy, VibrancyEffect::Acrylic);
}

#[test]
fn test_goal_20_win32_raw_abi_and_event_loop_soundness() {
    use strata_platform::raw_abi::win32::*;

    // 1. Verify standard Win32 window message identifiers
    assert_eq!(WM_NULL, 0x0000);
    assert_eq!(WM_CREATE, 0x0001);
    assert_eq!(WM_DESTROY, 0x0002);
    assert_eq!(WM_SIZE, 0x0005);
    assert_eq!(WM_PAINT, 0x000F);
    assert_eq!(WM_CLOSE, 0x0010);
    assert_eq!(WM_QUIT, 0x0012);
    assert_eq!(WM_KEYDOWN, 0x0100);
    assert_eq!(WM_KEYUP, 0x0101);
    assert_eq!(WM_MOUSEMOVE, 0x0200);
    assert_eq!(WM_LBUTTONDOWN, 0x0201);
    assert_eq!(WM_LBUTTONUP, 0x0202);

    // 2. Verify DXGI 1.6 flip model constants
    assert_eq!(DXGI_SWAP_EFFECT_DISCARD, 0);
    assert_eq!(DXGI_SWAP_EFFECT_FLIP_DISCARD, 4);
    assert_eq!(DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING, 2048);

    // 3. Verify Point and Rect conversions
    let pt = Point { x: 100, y: 200 };
    let prim_pt: strata_primitives::geom::Point<f32> = pt.into();
    assert_eq!(prim_pt.x, 100.0);
    assert_eq!(prim_pt.y, 200.0);

    let rect = Rect {
        left: 10,
        top: 20,
        right: 110,
        bottom: 220,
    };
    let prim_rect: strata_primitives::geom::Rect<f32> = rect.into();
    assert_eq!(prim_rect.x(), 10.0);
    assert_eq!(prim_rect.y(), 20.0);
    assert_eq!(prim_rect.width(), 100.0);
    assert_eq!(prim_rect.height(), 200.0);

    // 4. Verify Win32 Msg payload layout
    let msg = Msg {
        hwnd: Hwnd(0x1234),
        message: WM_PAINT,
        wparam: 0,
        lparam: 0,
        time: 123456,
        pt,
    };
    assert_eq!(msg.hwnd.0, 0x1234);
    assert_eq!(msg.message, WM_PAINT);
}

#[test]
fn test_window_anchor_bottom_right_workarea_docking_and_config_flags() {
    use strata_platform::window::WindowAnchor;

    // Simulate a 1920x1080 primary display with a 28px top bar and 40px bottom panel
    let (wa_x, wa_y, wa_w, wa_h) = (0, 28, 1920, 1012);

    let cfg = WindowConfig {
        title: "Strata Dock".to_string(),
        width: 420,
        height: 680,
        resizable: false,
        decoration: WindowDecoration::FramelessTransparent,
        transparent: true,
        always_on_top: true,
        dock_bottom_right: true,
        ..Default::default()
    };

    assert_eq!(cfg.effective_anchor(), Some(WindowAnchor::BottomRight));
    assert!(cfg.is_docked());
    assert_eq!(
        cfg.compute_anchored_position(wa_x, wa_y, wa_w, wa_h),
        Some((1920 - 420, 28 + 1012 - 680))
    );

    let via_builder = WindowConfig {
        width: 420,
        height: 680,
        ..Default::default()
    }
    .with_anchor(WindowAnchor::BottomRight);
    assert!(via_builder.dock_bottom_right);
    assert!(via_builder.is_docked());
    assert_eq!(
        via_builder.compute_anchored_position(wa_x, wa_y, wa_w, wa_h),
        Some((1500, 360))
    );

    assert_eq!(
        WindowAnchor::TopLeft.compute_position(wa_x, wa_y, wa_w, wa_h, 420, 680),
        (0, 28)
    );
    assert_eq!(
        WindowAnchor::TopRight.compute_position(wa_x, wa_y, wa_w, wa_h, 420, 680),
        (1500, 28)
    );
    assert_eq!(
        WindowAnchor::BottomLeft.compute_position(wa_x, wa_y, wa_w, wa_h, 420, 680),
        (0, 360)
    );
    assert!(!WindowAnchor::Center.is_sticky_dock());
}

#[cfg(all(target_os = "linux", feature = "gtk-compat"))]
#[test]
fn test_transparent_cairo_clear_preserves_rounded_corner_alpha() {
    use strata_layout::Rect as LayoutRect;
    use strata_platform::cairo;
    use strata_primitives::geom::EdgeInsets;
    use strata_render::{DisplayList, DrawCommand};
    use strata_style::types::{BorderRadius, Color};

    let mut surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, 64, 64).expect("create ARgb32 surface");
    {
        let cr = cairo::Context::new(&surface).expect("create cairo context");
        // Paint non-zero garbage first to verify Source operator truly clears alpha to 0
        cr.set_source_rgb(0.035, 0.035, 0.043);
        let _ = cr.paint();

        // Clear with Source rgba(0, 0, 0, 0) then restore Over operator (same sequence as launch.rs)
        cr.set_operator(cairo::Operator::Source);
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
        let _ = cr.paint();
        cr.set_operator(cairo::Operator::Over);

        let mut dl = DisplayList::new();
        dl.push(DrawCommand::DrawRoundedRect {
            bounds: LayoutRect::new(0.0, 0.0, 64.0, 64.0),
            color: Color::rgba(24, 24, 28, 255),
            border_radius: BorderRadius::new(20.0, 0.0, 0.0, 0.0),
            border_width: EdgeInsets::all(0.0),
            border_color: None,
        });
        strata_platform::render_display_list_to_cairo(&cr, &dl);
    }

    let stride = surface.stride() as usize;
    let data = surface.data().expect("surface data");
    // Top-left pixel (0, 0) lies outside the 20px top-left rounded corner and must remain transparent
    let alpha_top_left = data[3];
    assert_eq!(
        alpha_top_left, 0,
        "rounded-tl corner outside arc must have alpha 0, got {}",
        alpha_top_left
    );
    // Sharp corners (top-right, bottom-right, bottom-left) and center must remain opaque (alpha 255)
    let alpha_top_right = data[63 * 4 + 3];
    let alpha_bottom_right = data[63 * stride + 63 * 4 + 3];
    let alpha_bottom_left = data[63 * stride + 3];
    let alpha_center = data[32 * stride + 32 * 4 + 3];
    assert_eq!(alpha_top_right, 255);
    assert_eq!(alpha_bottom_right, 255);
    assert_eq!(alpha_bottom_left, 255);
    assert_eq!(alpha_center, 255);
}
