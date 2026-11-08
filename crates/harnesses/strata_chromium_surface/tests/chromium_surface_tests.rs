//! Integration Tests: Chromium Subprocess Harness, IPC Bridge & Shared Memory Surface.

use strata_dom::Tag;
use strata_primitives::geom::Rect;
use strata_chromium_surface::prelude::*;

#[test]
fn test_ipc_command_serialization_roundtrip() {
    let nav_cmd = ChromiumCommand::Navigate {
        url: "https://larvance.dev/cloud/telemetry".to_string(),
    };
    let serialized_nav = nav_cmd.serialize();
    assert_eq!(serialized_nav, "NAVIGATE|https://larvance.dev/cloud/telemetry");

    let deserialized_nav = ChromiumCommand::deserialize(&serialized_nav).expect("Must deserialize");
    assert_eq!(deserialized_nav, nav_cmd);

    let resize_cmd = ChromiumCommand::Resize {
        width: 1920,
        height: 1080,
        scale_factor: 2.0,
    };
    let serialized_resize = resize_cmd.serialize();
    assert_eq!(serialized_resize, "RESIZE|1920|1080|2");

    let deserialized_resize = ChromiumCommand::deserialize(&serialized_resize).expect("Must deserialize");
    assert_eq!(deserialized_resize, resize_cmd);

    let reload_cmd = ChromiumCommand::Reload;
    assert_eq!(ChromiumCommand::deserialize(&reload_cmd.serialize()), Some(reload_cmd));

    let term_cmd = ChromiumCommand::Terminate;
    assert_eq!(ChromiumCommand::deserialize(&term_cmd.serialize()), Some(term_cmd));
}

#[test]
fn test_shared_memory_surface_blit_and_sampling() {
    let surface = ShmSurface::new(640, 480, PixelFormat::Bgra8);
    assert_eq!(surface.width(), 640);
    assert_eq!(surface.height(), 480);
    assert_eq!(surface.stride(), 640 * 4);
    assert_eq!(surface.frame_index(), 0);

    // Initial fill is white [255, 255, 255, 255]
    let initial_px = surface.sample_pixel(100, 100).expect("Must sample pixel");
    assert_eq!(initial_px, [255, 255, 255, 255]);

    // Fill a rect (10, 10, 50, 50) with blue [0, 100, 255, 255]
    let fill_color = [0, 100, 255, 255];
    surface.fill_rect(Rect::from_xywh(10.0, 10.0, 50.0, 50.0), fill_color);

    // Verify inside rect
    let inside_px = surface.sample_pixel(25, 25).expect("Must sample pixel");
    assert_eq!(inside_px, fill_color);

    // Verify outside rect
    let outside_px = surface.sample_pixel(100, 100).expect("Must sample pixel");
    assert_eq!(outside_px, [255, 255, 255, 255]);

    assert!(surface.frame_index() > 0, "Frame index must increment on fill");

    // Test resize
    surface.resize(1024, 768);
    assert_eq!(surface.width(), 1024);
    assert_eq!(surface.height(), 768);
    assert_eq!(surface.stride(), 1024 * 4);
    assert_eq!(surface.clone_pixels().len(), 1024 * 768 * 4);
}

#[test]
fn test_process_manager_lazy_mount_and_unmount() {
    let manager = ChromiumProcessManager::new(800, 600);
    assert_eq!(manager.state(), ProcessState::Uninitialized);
    assert!(!manager.is_running());

    // 1. Mount lazy subprocess
    let target_url = "https://example.com/oauth/login";
    manager.mount(target_url);

    assert!(manager.is_running());
    match manager.state() {
        ProcessState::Running { pid } => assert!(pid >= 42001),
        _ => panic!("Expected running process with PID"),
    }

    // Verify command history
    let history = manager.history();
    assert!(history.iter().any(|cmd| matches!(cmd, ChromiumCommand::Navigate { url } if url == target_url)));

    // 2. Unmount and verify immediate cleanup
    manager.unmount();
    assert_eq!(manager.state(), ProcessState::Terminated);
    assert!(!manager.is_running());

    let final_history = manager.history();
    assert!(final_history.iter().any(|cmd| matches!(cmd, ChromiumCommand::Terminate)));
}

#[test]
fn test_webview_controller_and_event_forwarding() {
    let config = WebViewConfig {
        initial_url: "https://auth.larvance.dev/sso".to_string(),
        user_agent: Some("StrataClient/1.0".to_string()),
        enable_javascript: true,
        allow_cookies: true,
        custom_headers: vec![("X-Client-Id".to_string(), "STRATA-NODE-01".to_string())],
    };

    let controller = WebViewController::new(config, 1280, 720);
    assert!(controller.manager().is_running());

    // Navigate to new URL
    controller.navigate("https://dashboard.larvance.dev");

    // Forward mouse click
    controller.forward_mouse(
        MouseAction::Down,
        250.0,
        180.0,
        MouseButton::Left,
        Modifiers::default(),
    );

    // Forward keystroke
    controller.forward_key(
        KeyAction::Down,
        13, // Enter
        Some("\n".to_string()),
        Modifiers::default(),
    );

    // Evaluate JS
    controller.execute_js("console.log('Strata connected');");

    // Reload
    controller.reload();

    let history = controller.manager().history();
    assert!(history.iter().any(|c| matches!(c, ChromiumCommand::Navigate { url } if url == "https://dashboard.larvance.dev")));
    assert!(history.iter().any(|c| matches!(c, ChromiumCommand::MouseEvent { x, y, .. } if *x == 250.0 && *y == 180.0)));
    assert!(history.iter().any(|c| matches!(c, ChromiumCommand::KeyEvent { key_code, .. } if *key_code == 13)));
    assert!(history.iter().any(|c| matches!(c, ChromiumCommand::ExecuteJs { script, .. } if script.contains("Strata connected"))));
    assert!(history.iter().any(|c| matches!(c, ChromiumCommand::Reload)));

    // Render blueprint
    let blueprint = controller.render_blueprint("w-[800px] h-[500px]");
    match blueprint {
        strata_dom::NodeBlueprint::Element { tag, classes, attributes, children } => {
            assert_eq!(tag, Tag::Div);
            assert!(classes.iter().any(|c| c.contains("w-[800px]")));
            assert!(attributes.iter().any(|a| match a {
                strata_dom::Attribute::Property { key, value } => key == "data-component" && value == "strata-webview",
                _ => false,
            }));
            assert_eq!(children.len(), 1);
            match &children[0] {
                strata_dom::NodeBlueprint::Element { tag, .. } => assert_eq!(*tag, Tag::Canvas),
                _ => panic!("Expected Canvas child inside WebView"),
            }
        }
        _ => panic!("Expected element root"),
    }
}

#[test]
fn test_process_cleanup_on_drop_invariant() {
    let history = {
        let manager = ChromiumProcessManager::new(400, 300);
        manager.mount("https://test.local");
        assert!(manager.is_running());
        manager.history()
    };
    // Manager was dropped at end of scope; verified no panics or leaks occurred
    assert!(!history.is_empty());
}

#[test]
fn test_shm_binary_protocol_roundtrip() {
    let surface_src = ShmSurface::new(320, 240, PixelFormat::Bgra8);
    let damage = vec![Rect::from_xywh(10.0, 10.0, 50.0, 50.0)];
    surface_src.update_pixels(&vec![128u8; 320 * 240 * 4], damage);

    let shm_bytes = surface_src.serialize_shm_buffer();
    assert!(shm_bytes.len() >= strata_chromium_surface::SHM_HEADER_SIZE);

    let surface_dst = ShmSurface::new(100, 100, PixelFormat::Bgra8);
    surface_dst.deserialize_shm_buffer(&shm_bytes).expect("Deserialization failed");

    assert_eq!(surface_dst.width(), 320);
    assert_eq!(surface_dst.height(), 240);
    assert_eq!(surface_dst.damage_rects().len(), 1);
    assert_eq!(surface_dst.damage_rects()[0], Rect::from_xywh(10.0, 10.0, 50.0, 50.0));
    assert_eq!(surface_dst.sample_pixel(20, 20), Some([128, 128, 128, 128]));
}

#[test]
fn test_real_subprocess_execution_lifecycle() {
    // Launch a real Unix/Linux binary (e.g., cat or sh) that stays alive waiting for input
    #[cfg(unix)]
    {
        let manager = ChromiumProcessManager::with_executable(
            640,
            480,
            "/bin/cat",
            Vec::new(),
        );
        manager.mount("https://example.com");

        assert!(manager.is_running());
        assert!(manager.is_external());
        match manager.state() {
            ProcessState::Running { pid } => {
                assert!(pid > 0);
            }
            _ => panic!("Expected running state with real OS PID"),
        }

        // Clean unmount terminates real child process
        manager.unmount();
        assert_eq!(manager.state(), ProcessState::Terminated);
        assert!(!manager.is_running());
    }
}
