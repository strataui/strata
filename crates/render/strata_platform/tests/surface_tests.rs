use std::time::{Duration, Instant};
use strata_platform::raw_abi::wayland::WaylandHeader;
use strata_platform::raw_abi::win32::*;
use strata_platform::raw_abi::cocoa::*;
use strata_platform::surface::{FramePacer, PresentMode, RefreshRate, SurfaceConfig, SurfaceHandle, SwapchainFormat};
use strata_platform::window::{LivePlatformWindow, PlatformWindow, WindowConfig, WindowMode};
use strata_platform::events::PlatformEvent;

#[test]
fn test_surface_config_physical_sizing_and_byte_calculations() {
    let mut config = SurfaceConfig {
        width: 1280,
        height: 800,
        scale_factor: 1.0,
        format: SwapchainFormat::Bgra8Unorm,
        present_mode: PresentMode::Fifo,
        refresh_rate: RefreshRate::Hz60,
    };

    assert_eq!(config.physical_size(), (1280, 800));
    assert_eq!(config.buffer_byte_size(), 1280 * 800 * 4);

    // Retina 2.0x DPR
    config.scale_factor = 2.0;
    assert_eq!(config.physical_size(), (2560, 1600));
    assert_eq!(config.buffer_byte_size(), 2560 * 1600 * 4);

    // 1.5x fractional DPR
    config.scale_factor = 1.5;
    assert_eq!(config.physical_size(), (1920, 1200));

    // HDR 16-bit float
    config.format = SwapchainFormat::Rgba16Float;
    assert_eq!(config.buffer_byte_size(), 1920 * 1200 * 8);

    // SurfaceHandle headless
    let handle = SurfaceHandle::new_headless(1280, 800, 2.0);
    assert_eq!(handle.width, 1280);
    assert_eq!(handle.height, 800);
    assert_eq!(handle.scale_factor(), 2.0);
}

#[test]
fn test_frame_pacer_refresh_intervals_and_ewma() {
    let pacer_60 = FramePacer::new(RefreshRate::Hz60);
    assert_eq!(pacer_60.target_refresh.as_hz(), 60);
    assert_eq!(pacer_60.frame_interval, Duration::from_nanos(16_666_666));

    let pacer_120 = FramePacer::new(RefreshRate::Hz120);
    assert_eq!(pacer_120.target_refresh.as_hz(), 120);
    assert_eq!(pacer_120.frame_interval, Duration::from_nanos(8_333_333));

    let mut pacer = FramePacer::new(RefreshRate::Hz60);
    let start = Instant::now();
    let duration = pacer.end_frame(start);
    assert_eq!(pacer.frame_count, 1);
    assert!(duration < Duration::from_millis(100));
}

#[test]
fn test_live_platform_window_lifecycle_and_events() {
    let mut win_config = WindowConfig::default();
    win_config.title = "Live Test Window".to_string();
    win_config.width = 1024;
    win_config.height = 768;

    let mut window = LivePlatformWindow::new(win_config);
    assert_eq!(window.title(), "Live Test Window");
    assert_eq!(window.size(), (1024, 768));
    assert_eq!(window.scale_factor(), 1.0);
    assert!(window.is_redraw_requested());

    // Clear initial redraw
    assert!(window.take_redraw_request());
    assert!(!window.is_redraw_requested());

    // Resize triggers event & redraw
    window.set_size(1920, 1080);
    assert_eq!(window.size(), (1920, 1080));
    assert!(window.is_redraw_requested());
    assert_eq!(window.surface_config().width, 1920);
    assert_eq!(window.surface_config().height, 1080);

    // Scale factor change
    window.set_scale_factor(2.0);
    assert_eq!(window.scale_factor(), 2.0);
    assert_eq!(window.surface_handle().scale_factor(), 2.0);

    // Event polling
    let events = window.poll_events();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0], PlatformEvent::WindowResized { width: 1920, height: 1080 });
    assert_eq!(events[1], PlatformEvent::ScaleFactorChanged { scale_factor: 2.0 });

    // Mode
    window.set_mode(WindowMode::BorderlessFullscreen);
    assert_eq!(window.mode(), WindowMode::BorderlessFullscreen);
}

#[test]
fn test_raw_abi_wayland_wire_protocol_header() {
    let header = WaylandHeader {
        object_id: 42,
        opcode: 3,
        size: 16,
    };

    let bytes = header.to_bytes();
    assert_eq!(bytes.len(), 8);

    let parsed = WaylandHeader::from_bytes(&bytes).expect("Failed to parse WaylandHeader");
    assert_eq!(parsed.object_id, 42);
    assert_eq!(parsed.opcode, 3);
    assert_eq!(parsed.size, 16);
}

#[test]
fn test_raw_abi_win32_and_cocoa_structures() {
    let pt = Point { x: 100, y: 200 };
    let msg = Msg {
        hwnd: Hwnd(0x1234),
        message: WM_PAINT,
        wparam: 0,
        lparam: 0,
        time: 1000,
        pt,
    };
    assert_eq!(msg.message, WM_PAINT);
    assert_eq!(msg.pt.x, 100);

    let ns_rect = NsRect::new(10.0, 20.0, 800.0, 600.0);
    assert_eq!(ns_rect.width, 800.0);
    assert_eq!(ns_rect.height, 600.0);
    assert_eq!(NS_WINDOW_STYLE_TITLED | NS_WINDOW_STYLE_CLOSABLE, 3);
}
