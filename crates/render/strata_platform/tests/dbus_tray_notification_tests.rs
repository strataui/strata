//! Comprehensive integration test suite for pure-Rust D-Bus wire protocol encoding,
//! SASL authentication negotiation, desktop notification dispatch,
//! and thread-safe TrayEventChannel dispatching.

#![cfg(target_os = "linux")]

use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::time::Duration;
use strata_platform::dbus::{
    format_sasl_auth_request, hex_encode_bytes, DbusClient, DbusMessageDecoder, DbusMessageEncoder,
    DBUS_MESSAGE_TYPE_METHOD_CALL,
};
use strata_platform::events::PlatformEvent;
use strata_platform::tray::{TrayEvent, TrayEventChannel};

#[test]
fn test_dbus_sasl_hex_encoding() {
    assert_eq!(hex_encode_bytes(b"1000"), "31303030");
    assert_eq!(hex_encode_bytes(b"root"), "726f6f74");

    let payload = format_sasl_auth_request(1000);
    assert_eq!(payload, b"\0AUTH EXTERNAL 31303030\r\nBEGIN\r\n");
}

#[test]
fn test_dbus_wire_encoder_decoder_primitives() {
    let mut encoder = DbusMessageEncoder::new();
    encoder.push_u8(42);
    encoder.push_u16(1024);
    encoder.push_u32(0xDEADBEEF);
    encoder.push_i32(-999);
    encoder.push_string("strata_dbus");
    encoder.push_object_path("/org/strata/Test");
    encoder.push_signature("s");

    let bytes = encoder.as_bytes();
    let mut decoder = DbusMessageDecoder::new(bytes);

    assert_eq!(decoder.read_u8(), Some(42));
    assert_eq!(decoder.read_u16(), Some(1024));
    assert_eq!(decoder.read_u32(), Some(0xDEADBEEF));
    assert_eq!(decoder.read_i32(), Some(-999));
    assert_eq!(decoder.read_string(), Some("strata_dbus".to_string()));
    assert_eq!(decoder.read_string(), Some("/org/strata/Test".to_string()));
}

#[test]
fn test_dbus_method_call_binary_framing() {
    let mut encoder = DbusMessageEncoder::new();
    let body_str = "Notification Test";
    let mut body_encoder = DbusMessageEncoder::new();
    body_encoder.push_string(body_str);

    encoder.encode_method_call(
        1,
        Some("org.freedesktop.Notifications"),
        "/org/freedesktop/Notifications",
        Some("org.freedesktop.Notifications"),
        "Notify",
        Some("s"),
        body_encoder.as_bytes(),
    );

    let bytes = encoder.as_bytes();
    assert!(bytes.len() >= 16);

    // Fixed 16-byte header inspection
    assert_eq!(bytes[0], b'l'); // Little-endian
    assert_eq!(bytes[1], DBUS_MESSAGE_TYPE_METHOD_CALL);
    assert_eq!(bytes[2], 0); // Flags
    assert_eq!(bytes[3], 1); // Version 1

    let body_len = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let serial = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    let fields_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;

    assert_eq!(serial, 1);
    assert_eq!(body_len, body_encoder.as_bytes().len());

    let mut header_total = 16 + fields_len;
    if !header_total.is_multiple_of(8) {
        header_total += 8 - (header_total % 8);
    }

    assert_eq!(bytes.len(), header_total + body_len);
}

#[test]
fn test_dbus_loopback_mock_server_handshake_and_method_call() {
    let temp_dir = std::env::temp_dir();
    let socket_path = temp_dir.join(format!("strata-dbus-test-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&socket_path);

    let listener = UnixListener::bind(&socket_path).expect("Failed to bind mock D-Bus socket");

    let server_handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("Accept failed");
        stream
            .set_read_timeout(Some(Duration::from_millis(1500)))
            .unwrap();

        // 1. Read SASL auth request
        let mut auth_buf = [0u8; 64];
        let mut auth_read = 0;
        while auth_read < auth_buf.len() {
            match stream.read(&mut auth_buf[auth_read..auth_read + 1]) {
                Ok(1) => {
                    auth_read += 1;
                    if auth_buf[..auth_read].ends_with(b"\r\n") {
                        break;
                    }
                }
                _ => break,
            }
        }
        let auth_str = String::from_utf8_lossy(&auth_buf[..auth_read]);
        assert!(auth_str.contains("AUTH EXTERNAL"));

        // Respond with OK
        stream.write_all(b"OK 0123456789abcdef\r\n").unwrap();

        // Read BEGIN\r\n
        let mut begin_buf = [0u8; 7];
        stream.read_exact(&mut begin_buf).unwrap();
        assert_eq!(&begin_buf, b"BEGIN\r\n");

        // 2. Read DBus.Hello method call
        let mut header = [0u8; 16];
        stream.read_exact(&mut header).unwrap();
        assert_eq!(header[0], b'l');
        assert_eq!(header[1], DBUS_MESSAGE_TYPE_METHOD_CALL);

        let body_len = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
        let fields_len = u32::from_le_bytes(header[12..16].try_into().unwrap()) as usize;
        let mut header_total = 16 + fields_len;
        if !header_total.is_multiple_of(8) {
            header_total += 8 - (header_total % 8);
        }
        let remaining = header_total - 16 + body_len;
        let mut rest = vec![0u8; remaining];
        stream.read_exact(&mut rest).unwrap();

        // Send DBus.Hello reply with unique name ":1.42"
        let mut reply_body = DbusMessageEncoder::new();
        reply_body.push_string(":1.42");

        let mut reply_encoder = DbusMessageEncoder::new();
        reply_encoder.encode_method_return(
            100, // server serial
            1,   // reply to serial 1
            Some(":1.42"),
            Some("s"),
            reply_body.as_bytes(),
        );

        stream.write_all(reply_encoder.as_bytes()).unwrap();
    });

    let client = DbusClient::connect_to_path(socket_path.to_str().unwrap())
        .expect("DbusClient connection and handshake failed");

    assert_eq!(client.unique_name, Some(":1.42".to_string()));

    server_handle.join().unwrap();
    let _ = std::fs::remove_file(socket_path);
}

#[test]
fn test_tray_event_channel_lifecycle() {
    let (channel, receiver) = TrayEventChannel::new();

    // Initial state: empty
    assert_eq!(receiver.try_recv(), None);

    // Dispatch events
    channel.dispatch(TrayEvent::LeftClick);
    channel.dispatch(TrayEvent::RightClick);
    channel.dispatch(TrayEvent::DoubleClick);
    channel.dispatch(TrayEvent::MenuItemTriggered(42));

    // Verify reception in order
    assert_eq!(receiver.try_recv(), Some(TrayEvent::LeftClick));
    assert_eq!(receiver.try_recv(), Some(TrayEvent::RightClick));
    assert_eq!(receiver.try_recv(), Some(TrayEvent::DoubleClick));
    assert_eq!(receiver.try_recv(), Some(TrayEvent::MenuItemTriggered(42)));
    assert_eq!(receiver.try_recv(), None);
}

#[test]
fn test_tray_event_channel_as_sink() {
    let (channel, receiver) = TrayEventChannel::new();
    let sink = channel.as_sink();

    sink(PlatformEvent::TrayIconClicked);
    sink(PlatformEvent::TrayMenuItemClicked(99));

    assert_eq!(
        receiver.recv_timeout(Duration::from_millis(500)),
        Ok(TrayEvent::LeftClick)
    );
    assert_eq!(
        receiver.recv_timeout(Duration::from_millis(500)),
        Ok(TrayEvent::MenuItemTriggered(99))
    );
}
