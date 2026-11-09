//! Comprehensive integration test suite for pure-Rust X11 wire protocol encoding,
//! decoding, SysV MIT-SHM shared memory presentation, and 32-byte hardware event dispatch.

#![cfg(target_os = "linux")]

use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use strata_platform::x11_window::{
    default_x11_socket_path, map_x11_pure_keycode, PureX11Window, X11ShmSegment, X11WireDecoder,
    X11WireEncoder, X11_OPCODE_CREATE_WINDOW, X11_OPCODE_QUERY_EXTENSION,
};
use strata_platform::window::WindowConfig;
use strata_platform::{KeyCode, MouseButton, PlatformEvent};

#[test]
fn test_x11_encoder_decoder_primitives() {
    let mut encoder = X11WireEncoder::new();
    encoder.begin_request(X11_OPCODE_CREATE_WINDOW, 0);
    encoder.push_u8(24);
    encoder.push_u16(1024);
    encoder.push_i16(-50);
    encoder.push_u32(0xDEADBEEF);
    encoder.push_padded_string("strata_x11");
    encoder.finish_request();

    let bytes = encoder.as_bytes();
    assert!(bytes.len() >= 4);
    assert_eq!(bytes.len() % 4, 0);

    let len_words = u16::from_ne_bytes(bytes[2..4].try_into().unwrap());
    assert_eq!(len_words as usize * 4, bytes.len());

    let mut decoder = X11WireDecoder::new(bytes);
    assert_eq!(decoder.read_u8(), Some(X11_OPCODE_CREATE_WINDOW));
    assert_eq!(decoder.read_u8(), Some(0));
    assert_eq!(decoder.read_u16(), Some(len_words));
    assert_eq!(decoder.read_u8(), Some(24));
    assert_eq!(decoder.read_u16(), Some(1024));
    assert_eq!(decoder.read_i16(), Some(-50));
    assert_eq!(decoder.read_u32(), Some(0xDEADBEEF));
    assert_eq!(decoder.read_bytes(10), Some("strata_x11".as_bytes()));
}

#[test]
fn test_x11_default_socket_path_parsing() {
    let path = default_x11_socket_path();
    assert!(path.starts_with("/tmp/.X11-unix/X"));
}

#[test]
fn test_x11_keycode_mapping() {
    // Escape: evdev 1, X11 keycode 9
    assert_eq!(map_x11_pure_keycode(9), (KeyCode::Escape, None));
    // Key 'A': evdev 30, X11 keycode 38
    assert_eq!(map_x11_pure_keycode(38), (KeyCode::KeyA, Some('a')));
    // Enter: evdev 28, X11 keycode 36
    assert_eq!(map_x11_pure_keycode(36), (KeyCode::Enter, Some('\n')));
    // Space: evdev 57, X11 keycode 65
    assert_eq!(map_x11_pure_keycode(65), (KeyCode::Space, Some(' ')));
    // ArrowLeft: evdev 105, X11 keycode 113
    assert_eq!(map_x11_pure_keycode(113), (KeyCode::ArrowLeft, None));
}

#[test]
fn test_x11_mit_shm_segment_allocation_and_write() {
    let shmseg_id = 42;
    let size = 64 * 64 * 4;
    let mut shm = X11ShmSegment::new(shmseg_id, size)
        .expect("SysV shared memory segment allocation should succeed");

    assert!(!shm.ptr.is_null());
    assert_eq!(shm.size, size);
    assert_eq!(shm.shmseg_id, 42);

    let test_pattern = vec![0x33u8; size];
    shm.write_pixels(&test_pattern);

    unsafe {
        let slice = std::slice::from_raw_parts(shm.ptr, size);
        assert_eq!(slice[0], 0x33);
        assert_eq!(slice[size - 1], 0x33);
    }
}

#[test]
fn test_x11_loopback_mock_server_handshake() {
    let temp_dir = std::env::temp_dir();
    let socket_name = format!("strata-test-x11-handshake-{}", std::process::id());
    let socket_path = temp_dir.join(&socket_name);
    let _ = std::fs::remove_file(&socket_path);

    let listener = UnixListener::bind(&socket_path).expect("Bind test socket");
    let socket_path_str = socket_path.to_str().unwrap().to_string();

    let server_thread = std::thread::spawn(move || {
        let (mut client_sock, _) = listener.accept().expect("Accept client");

        // 1. Read setup packet (12 bytes)
        let mut setup_pkt = [0u8; 12];
        client_sock.read_exact(&mut setup_pkt).expect("Read setup");
        assert_eq!(setup_pkt[0], b'l'); // Little-endian

        // 2. Synthesize setup reply
        // Vendor: "StrataMock\0\0" (12 bytes padded)
        // Formats: 1 format = 8 bytes
        // Screen: 40 bytes
        let vendor = b"StrataMock\0\0";
        let format_bytes = [0u8; 8];
        let mut screen_bytes = [0u8; 40];
        screen_bytes[0..4].copy_from_slice(&0x0000_0001u32.to_ne_bytes()); // root window ID
        screen_bytes[8..12].copy_from_slice(&0x00FFFFFFu32.to_ne_bytes()); // white pixel
        screen_bytes[12..16].copy_from_slice(&0x00000000u32.to_ne_bytes()); // black pixel
        screen_bytes[20..22].copy_from_slice(&1920u16.to_ne_bytes()); // width
        screen_bytes[22..24].copy_from_slice(&1080u16.to_ne_bytes()); // height
        screen_bytes[38] = 24; // depth

        let extra_length_bytes = 32 + vendor.len() + format_bytes.len() + screen_bytes.len();
        let extra_words = (extra_length_bytes / 4) as u16;

        let mut reply_header = [0u8; 8];
        reply_header[0] = 1; // Success
        reply_header[2..4].copy_from_slice(&11u16.to_ne_bytes());
        reply_header[6..8].copy_from_slice(&extra_words.to_ne_bytes());

        let mut extra_data = Vec::new();
        extra_data.extend_from_slice(&0x0000_0001u32.to_ne_bytes()); // release
        extra_data.extend_from_slice(&0x0200_0000u32.to_ne_bytes()); // resource_id_base
        extra_data.extend_from_slice(&0x001F_FFFFu32.to_ne_bytes()); // resource_id_mask
        extra_data.extend_from_slice(&256u32.to_ne_bytes()); // motion buffer size
        extra_data.extend_from_slice(&(vendor.len() as u16).to_ne_bytes()); // vendor length
        extra_data.extend_from_slice(&65535u16.to_ne_bytes()); // max req length
        extra_data.push(1); // num_screens
        extra_data.push(1); // num_formats
        extra_data.extend_from_slice(&[0u8; 10]); // format info & keycodes
        extra_data.extend_from_slice(vendor);
        extra_data.extend_from_slice(&format_bytes);
        extra_data.extend_from_slice(&screen_bytes);

        client_sock.write_all(&reply_header).unwrap();
        client_sock.write_all(&extra_data).unwrap();

        // 3. Receive QueryExtension("MIT-SHM")
        let mut query_req = [0u8; 16];
        let _ = client_sock.read_exact(&mut query_req);
        assert_eq!(query_req[0], X11_OPCODE_QUERY_EXTENSION);

        // Send 32-byte extension reply: not present
        let mut ext_reply = [0u8; 32];
        ext_reply[0] = 1; // reply
        ext_reply[8] = 0; // present = 0
        client_sock.write_all(&ext_reply).unwrap();

        // 4. Drain requests until client closes connection
        let mut drain_buf = [0u8; 1024];
        while let Ok(n) = client_sock.read(&mut drain_buf) {
            if n == 0 {
                break;
            }
        }
    });

    let config = WindowConfig {
        title: "Strata Pure X11 Test".to_string(),
        width: 800,
        height: 600,
        ..Default::default()
    };

    let window_result = PureX11Window::connect_to_socket(&socket_path_str, config);
    assert!(
        window_result.is_ok(),
        "Expected PureX11Window to connect and handshake successfully: {:?}",
        window_result.err()
    );
    let win = window_result.unwrap();
    assert_eq!(win.client.info.root_window, 1);
    assert_eq!(win.client.info.resource_id_base, 0x0200_0000);

    drop(win);
    let _ = server_thread.join();
    let _ = std::fs::remove_file(&socket_path);
}

#[test]
fn test_x11_32_byte_event_packet_decoding() {
    let temp_dir = std::env::temp_dir();
    let socket_name = format!("strata-test-x11-events-{}", std::process::id());
    let socket_path = temp_dir.join(&socket_name);
    let _ = std::fs::remove_file(&socket_path);

    let listener = UnixListener::bind(&socket_path).expect("Bind test socket");
    let socket_path_str = socket_path.to_str().unwrap().to_string();

    let server_thread = std::thread::spawn(move || {
        let (mut client_sock, _) = listener.accept().expect("Accept client");

        // Handshake
        let mut setup_pkt = [0u8; 12];
        client_sock.read_exact(&mut setup_pkt).unwrap();

        let vendor = b"Mock\0\0\0\0";
        let format_bytes = [0u8; 8];
        let mut screen_bytes = [0u8; 40];
        screen_bytes[0..4].copy_from_slice(&0x0000_0001u32.to_ne_bytes());
        screen_bytes[20..22].copy_from_slice(&1920u16.to_ne_bytes());
        screen_bytes[22..24].copy_from_slice(&1080u16.to_ne_bytes());

        let extra_length_bytes = 32 + vendor.len() + format_bytes.len() + screen_bytes.len();
        let extra_words = (extra_length_bytes / 4) as u16;

        let mut reply_header = [0u8; 8];
        reply_header[0] = 1;
        reply_header[2..4].copy_from_slice(&11u16.to_ne_bytes());
        reply_header[6..8].copy_from_slice(&extra_words.to_ne_bytes());

        let mut extra_data = Vec::new();
        extra_data.extend_from_slice(&1u32.to_ne_bytes());
        extra_data.extend_from_slice(&0x0100_0000u32.to_ne_bytes());
        extra_data.extend_from_slice(&0x000F_FFFFu32.to_ne_bytes());
        extra_data.extend_from_slice(&256u32.to_ne_bytes());
        extra_data.extend_from_slice(&(vendor.len() as u16).to_ne_bytes());
        extra_data.extend_from_slice(&65535u16.to_ne_bytes());
        extra_data.push(1);
        extra_data.push(1);
        extra_data.extend_from_slice(&[0u8; 10]);
        extra_data.extend_from_slice(vendor);
        extra_data.extend_from_slice(&format_bytes);
        extra_data.extend_from_slice(&screen_bytes);

        client_sock.write_all(&reply_header).unwrap();
        client_sock.write_all(&extra_data).unwrap();

        // Reply to QueryExtension
        let mut query = [0u8; 16];
        let _ = client_sock.read_exact(&mut query);
        let mut ext_reply = [0u8; 32];
        ext_reply[0] = 1;
        ext_reply[8] = 0;
        client_sock.write_all(&ext_reply).unwrap();

        // Drain window creation requests
        let mut drain = [0u8; 1024];
        let _ = client_sock.read(&mut drain);

        // 1. Send MotionNotify event (opcode 6, x=250, y=350)
        let mut motion_pkt = [0u8; 32];
        motion_pkt[0] = 6;
        motion_pkt[24..26].copy_from_slice(&250i16.to_ne_bytes());
        motion_pkt[26..28].copy_from_slice(&350i16.to_ne_bytes());
        client_sock.write_all(&motion_pkt).unwrap();

        // 2. Send KeyPress event (opcode 2, keycode 38 -> 'a')
        let mut key_pkt = [0u8; 32];
        key_pkt[0] = 2;
        key_pkt[1] = 38; // KeyA
        client_sock.write_all(&key_pkt).unwrap();

        // 3. Send ButtonPress event (opcode 4, button 1, x=100, y=120)
        let mut btn_pkt = [0u8; 32];
        btn_pkt[0] = 4;
        btn_pkt[1] = 1; // Left button
        btn_pkt[24..26].copy_from_slice(&100i16.to_ne_bytes());
        btn_pkt[26..28].copy_from_slice(&120i16.to_ne_bytes());
        client_sock.write_all(&btn_pkt).unwrap();
    });

    let config = WindowConfig::default();
    let mut win = PureX11Window::connect_to_socket(&socket_path_str, config).expect("Connect window");

    // Sleep briefly to allow server events to flush into socket
    std::thread::sleep(std::time::Duration::from_millis(50));

    let events = win.poll_native_events();
    let _ = server_thread.join();
    let _ = std::fs::remove_file(&socket_path);

    let has_motion = events.iter().any(|e| {
        matches!(e, PlatformEvent::MouseMove { x, y } if (*x - 250.0).abs() < 1.0 && (*y - 350.0).abs() < 1.0)
    });
    let has_key = events.iter().any(|e| matches!(e, PlatformEvent::KeyDown { key: KeyCode::KeyA, .. }));
    let has_char = events.iter().any(|e| matches!(e, PlatformEvent::ReceivedCharacter('a')));
    let has_click = events.iter().any(|e| matches!(e, PlatformEvent::MouseDown { button: MouseButton::Left, .. }));

    assert!(has_motion, "Expected MouseMove event from X11 wire");
    assert!(has_key, "Expected KeyDown(KeyA) event from X11 wire");
    assert!(has_char, "Expected ReceivedCharacter('a') event from X11 wire");
    assert!(has_click, "Expected MouseDown(Left) event from X11 wire");
}
