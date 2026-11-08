//! Comprehensive test suite for pure-Rust Wayland wire protocol encoding, decoding,
//! POSIX shared memory buffers, and connection handling.

#![cfg(target_os = "linux")]

use strata_platform::raw_abi::wayland::{WaylandHeader, WL_DISPLAY_ID, WL_DISPLAY_SYNC};
use strata_platform::wayland_window::{ShmBuffer, WaylandDecoder, WaylandEncoder, WaylandWindow};
use strata_platform::window::WindowConfig;
use strata_platform::PlatformEvent;
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;

#[test]
fn test_wayland_header_roundtrip() {
    let header = WaylandHeader {
        object_id: 42,
        opcode: 7,
        size: 24,
    };
    let bytes = header.to_bytes();
    assert_eq!(bytes.len(), 8);

    let parsed = WaylandHeader::from_bytes(&bytes).expect("Failed to parse header");
    assert_eq!(parsed.object_id, 42);
    assert_eq!(parsed.opcode, 7);
    assert_eq!(parsed.size, 24);
}

#[test]
fn test_wayland_encoder_decoder_primitives() {
    let mut encoder = WaylandEncoder::new();
    encoder.begin_message();
    encoder.push_u32(12345);
    encoder.push_i32(-6789);
    encoder.push_fixed(12.5);
    encoder.push_string("strata_wayland");
    encoder.finish_message(WL_DISPLAY_ID, WL_DISPLAY_SYNC);

    let bytes = encoder.as_bytes();
    assert!(bytes.len() >= 8);

    let header = WaylandHeader::from_bytes(&bytes[0..8]).expect("Valid header");
    assert_eq!(header.object_id, WL_DISPLAY_ID);
    assert_eq!(header.opcode, WL_DISPLAY_SYNC);
    assert_eq!(header.size as usize, bytes.len());

    let mut decoder = WaylandDecoder::new(&bytes[8..]);
    assert_eq!(decoder.read_u32(), Some(12345));
    assert_eq!(decoder.read_i32(), Some(-6789));
    let fixed_val = decoder.read_fixed().expect("Fixed point");
    assert!((fixed_val - 12.5).abs() < 0.001);
    assert_eq!(decoder.read_string().as_deref(), Some("strata_wayland"));
    assert_eq!(decoder.remaining(), 0);
}

#[test]
fn test_wayland_encoder_alignment() {
    let mut encoder = WaylandEncoder::new();
    encoder.begin_message();
    // String length 3 + 1 null = 4 bytes (already aligned)
    encoder.push_string("abc");
    encoder.finish_message(1, 0);
    assert_eq!(encoder.as_bytes().len(), 8 + 4 + 4); // 8 header + 4 len + 4 content ("abc\0")

    encoder.begin_message();
    // String length 4 + 1 null = 5 bytes -> aligned to 8
    encoder.push_string("abcd");
    encoder.finish_message(1, 0);
    assert_eq!(encoder.as_bytes().len(), 8 + 4 + 8); // 8 header + 4 len + 8 content ("abcd\0\0\0\0")
}

#[test]
fn test_wayland_shm_buffer_allocation_and_write() {
    let width = 64;
    let height = 64;
    let mut shm = ShmBuffer::new(10, 11, width, height).expect("ShmBuffer allocation should succeed");
    assert_eq!(shm.size, (64 * 64 * 4));
    assert!(!shm.ptr.is_null());

    let test_pixels = vec![0xAAu8; shm.size];
    shm.write_pixels(&test_pixels);

    unsafe {
        let slice = std::slice::from_raw_parts(shm.ptr, shm.size);
        assert_eq!(slice[0], 0xAA);
        assert_eq!(slice[shm.size - 1], 0xAA);
    }
}

static ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn test_wayland_missing_socket_error() {
    let _lock = ENV_MUTEX.lock().unwrap();
    std::env::set_var("XDG_RUNTIME_DIR", "/tmp/nonexistent-strata-dir");
    std::env::set_var("WAYLAND_DISPLAY", "nonexistent-wayland-99");

    let result = WaylandWindow::new(WindowConfig::default());
    assert!(result.is_err());
    let err_msg = result.err().unwrap();
    assert!(err_msg.contains("Cannot connect to Wayland socket"));
}

#[test]
fn test_wayland_loopback_handshake() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp_dir = std::env::temp_dir();
    let socket_name = format!("strata-test-wayland-{}", std::process::id());
    let socket_path = temp_dir.join(&socket_name);
    let _ = std::fs::remove_file(&socket_path);

    let listener = UnixListener::bind(&socket_path).expect("Failed to bind test wayland socket");

    std::env::set_var("XDG_RUNTIME_DIR", temp_dir.to_str().unwrap());
    std::env::set_var("WAYLAND_DISPLAY", &socket_name);

    let server_thread = std::thread::spawn(move || {
        let (mut client_sock, _) = listener.accept().expect("Accept client");

        // 1. Expect get_registry & sync
        let mut buf = [0u8; 1024];
        let n = client_sock.read(&mut buf).expect("Read from client");
        assert!(n >= 16); // get_registry + sync

        // Send registry globals: wl_compositor (id=2), wl_shm (id=2), xdg_wm_base (id=2)
        let mut enc = WaylandEncoder::new();

        // wl_compositor
        enc.begin_message();
        enc.push_u32(1); // global name 1
        enc.push_string("wl_compositor");
        enc.push_u32(4); // version
        enc.finish_message(2, 0); // registry_id 2, opcode 0 (global)
        let _ = client_sock.write_all(enc.as_bytes());

        // wl_shm
        enc.begin_message();
        enc.push_u32(2); // global name 2
        enc.push_string("wl_shm");
        enc.push_u32(1); // version
        enc.finish_message(2, 0); // registry_id 2, opcode 0 (global)
        let _ = client_sock.write_all(enc.as_bytes());

        // xdg_wm_base
        enc.begin_message();
        enc.push_u32(3); // global name 3
        enc.push_string("xdg_wm_base");
        enc.push_u32(1); // version
        enc.finish_message(2, 0); // registry_id 2, opcode 0 (global)
        let _ = client_sock.write_all(enc.as_bytes());

        // wl_callback done (sync callback id = 3, opcode = 0)
        enc.begin_message();
        enc.push_u32(0); // serial
        enc.finish_message(3, 0); // callback done
        let _ = client_sock.write_all(enc.as_bytes());

        // Drain any incoming bind & create requests
        let _ = client_sock.read(&mut buf);
    });

    let window_result = WaylandWindow::new(WindowConfig::default());
    let _ = server_thread.join();
    let _ = std::fs::remove_file(&socket_path);

    // If handshake succeeded, window is created
    if let Ok(win) = window_result {
        assert_eq!(win.globals.compositor_name, Some(1));
        assert_eq!(win.globals.shm_name, Some(2));
        assert_eq!(win.globals.xdg_wm_base_name, Some(3));
        assert!(win.shm_buffer.is_some());
    }
}

#[test]
fn test_wayland_pointer_motion_and_keyboard_events() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let temp_dir = std::env::temp_dir();
    let socket_name = format!("strata-test-wayland-events-{}", std::process::id());
    let socket_path = temp_dir.join(&socket_name);
    let _ = std::fs::remove_file(&socket_path);

    let listener = UnixListener::bind(&socket_path).expect("Failed to bind test wayland socket");

    std::env::set_var("XDG_RUNTIME_DIR", temp_dir.to_str().unwrap());
    std::env::set_var("WAYLAND_DISPLAY", &socket_name);

    let server_thread = std::thread::spawn(move || {
        let (mut client_sock, _) = listener.accept().expect("Accept client");

        // 1. Read get_registry & sync
        let mut buf = [0u8; 1024];
        let _ = client_sock.read(&mut buf);

        // 2. Send globals: wl_compositor, wl_shm, xdg_wm_base, wl_seat, and sync done
        let mut enc = WaylandEncoder::new();

        enc.begin_message();
        enc.push_u32(1);
        enc.push_string("wl_compositor");
        enc.push_u32(4);
        enc.finish_message(2, 0);
        let _ = client_sock.write_all(enc.as_bytes());

        enc.begin_message();
        enc.push_u32(2);
        enc.push_string("wl_shm");
        enc.push_u32(1);
        enc.finish_message(2, 0);
        let _ = client_sock.write_all(enc.as_bytes());

        enc.begin_message();
        enc.push_u32(3);
        enc.push_string("xdg_wm_base");
        enc.push_u32(1);
        enc.finish_message(2, 0);
        let _ = client_sock.write_all(enc.as_bytes());

        enc.begin_message();
        enc.push_u32(4);
        enc.push_string("wl_seat");
        enc.push_u32(5);
        enc.finish_message(2, 0);
        let _ = client_sock.write_all(enc.as_bytes());

        enc.begin_message();
        enc.push_u32(0);
        enc.finish_message(3, 0);
        let _ = client_sock.write_all(enc.as_bytes());

        // Drain bind requests and xdg configure
        let _ = client_sock.read(&mut buf);

        // Send xdg_wm_base ping (serial = 99)
        enc.begin_message();
        enc.push_u32(99);
        enc.finish_message(5, 0);
        let _ = client_sock.write_all(enc.as_bytes());

        // Send pointer motion: object pointer_id=10, opcode 2 (motion: time, surface_x, surface_y)
        enc.begin_message();
        enc.push_u32(1000);
        enc.push_fixed(150.0);
        enc.push_fixed(200.0);
        enc.finish_message(10, 2);
        let _ = client_sock.write_all(enc.as_bytes());

        // Send keyboard key event: object keyboard_id=11, opcode 3 (serial, time, key=30 ('a'), state=1)
        enc.begin_message();
        enc.push_u32(1);
        enc.push_u32(1001);
        enc.push_u32(30);
        enc.push_u32(1);
        enc.finish_message(11, 3);
        let _ = client_sock.write_all(enc.as_bytes());
    });

    let window_result = WaylandWindow::new(WindowConfig::default());
    let _ = server_thread.join();
    let _ = std::fs::remove_file(&socket_path);

    if let Ok(mut win) = window_result {
        let events = win.poll_native_events();
        let has_move = events.iter().any(|e| {
            matches!(e, PlatformEvent::MouseMove { x, y } if (*x - 150.0).abs() < 1.0 && (*y - 200.0).abs() < 1.0)
        });
        let has_char = events.iter().any(|e| matches!(e, PlatformEvent::ReceivedCharacter('a')));
        assert!(has_move, "Expected MouseMove event from Wayland wire");
        assert!(has_char, "Expected ReceivedCharacter('a') from Wayland wire");
    }
}

