//! Comprehensive HTTP/2 Framing, HPACK Header Compression, and Protocol Exchange Tests.

use std::io::{Cursor, Read, Write};
use std::net::TcpListener;
use std::thread;

use strata_web_api::http2::{
    decode_hpack_headers, decode_huffman_string, encode_hpack_headers, execute_http2_request,
    HpackDecoder, Http2ConnectionSettings, Http2Frame, DEFAULT_MAX_FRAME_SIZE, FLAG_ACK,
    FLAG_END_HEADERS, FLAG_END_STREAM, FLAG_PADDED, FRAME_DATA, FRAME_GOAWAY, FRAME_HEADERS,
    FRAME_PING, FRAME_RST_STREAM, FRAME_SETTINGS, FRAME_WINDOW_UPDATE, HTTP2_PREFACE,
    SETTINGS_MAX_FRAME_SIZE,
};
use strata_web_api::{FetchError, ParsedUrl, RequestInit};

#[test]
fn test_rfc7541_huffman_decoding_and_dynamic_table_c41_c42() {
    // RFC 7541 Appendix C.4.1 Huffman vector: "www.example.com"
    // Hex: f1e3c2e5f23a6ba0ab90f4ff
    let c41_huffman = [0xf1, 0xe3, 0xc2, 0xe5, 0xf2, 0x3a, 0x6b, 0xa0, 0xab, 0x90, 0xf4, 0xff];
    let decoded_str = decode_huffman_string(&c41_huffman).expect("Failed to decode C.4.1 Huffman vector");
    assert_eq!(decoded_str, "www.example.com");

    // RFC 7541 Appendix C.4.1 Full Header Block:
    // 82 86 84 41 8c f1 e3 c2 e5 f2 3a 6b a0 ab 90 f4 ff
    let c41_block = [
        0x82, 0x86, 0x84, 0x41, 0x8c, 0xf1, 0xe3, 0xc2, 0xe5, 0xf2, 0x3a, 0x6b, 0xa0, 0xab, 0x90, 0xf4, 0xff,
    ];
    let mut decoder = HpackDecoder::new();
    let headers = decoder.decode_header_block(&c41_block).expect("Decode C.4.1 block");

    assert_eq!(headers.len(), 4);
    assert_eq!(headers[0], (":method".to_string(), "GET".to_string()));
    assert_eq!(headers[1], (":scheme".to_string(), "http".to_string()));
    assert_eq!(headers[2], (":path".to_string(), "/".to_string()));
    assert_eq!(headers[3], (":authority".to_string(), "www.example.com".to_string()));

    // Verify dynamic table has 1 entry: ":authority: www.example.com" at index 62
    assert_eq!(decoder.dynamic_table.len(), 1);
    assert_eq!(decoder.dynamic_table[0], (":authority".to_string(), "www.example.com".to_string()));
    let (dyn_k, dyn_v) = decoder.get_header(62).expect("Header at index 62");
    assert_eq!(dyn_k, ":authority");
    assert_eq!(dyn_v, "www.example.com");

    // RFC 7541 Appendix C.4.2 Second Request referencing dynamic table index 62:
    // 82 86 84 be 58 86 a8 eb 10 64 9c bf
    // 'be' is index 62 in dynamic table!
    // '58' is Literal indexed with indexed name 24 (cache-control), and value 'no-cache'
    let c42_block = [
        0x82, 0x86, 0x84, 0xbe, 0x58, 0x86, 0xa8, 0xeb, 0x10, 0x64, 0x9c, 0xbf,
    ];
    let headers2 = decoder.decode_header_block(&c42_block).expect("Decode C.4.2 block");
    assert_eq!(headers2.len(), 5);
    assert_eq!(headers2[0], (":method".to_string(), "GET".to_string()));
    assert_eq!(headers2[1], (":scheme".to_string(), "http".to_string()));
    assert_eq!(headers2[2], (":path".to_string(), "/".to_string()));
    assert_eq!(headers2[3], (":authority".to_string(), "www.example.com".to_string()));
    assert_eq!(headers2[4], ("cache-control".to_string(), "no-cache".to_string()));
}

#[test]
fn test_http2_frame_encode_decode_roundtrip() {
    let payload = b"Hello HTTP/2 World!".to_vec();
    let frame = Http2Frame::new(FRAME_DATA, FLAG_END_STREAM, 1, payload.clone());

    let encoded = frame.encode();
    assert_eq!(encoded.len(), 9 + payload.len());

    let mut cursor = Cursor::new(encoded);
    let decoded = Http2Frame::read_from(&mut cursor).expect("Failed to read Http2Frame");

    assert_eq!(decoded.frame_type, FRAME_DATA);
    assert_eq!(decoded.flags, FLAG_END_STREAM);
    assert_eq!(decoded.stream_id, 1);
    assert_eq!(decoded.length, payload.len() as u32);
    assert_eq!(decoded.payload, payload);
}

#[test]
fn test_http2_settings_and_ping_frame_encoding() {
    // Empty settings frame (initial client settings)
    let settings = Http2Frame::new(FRAME_SETTINGS, 0, 0, Vec::new());
    let encoded_settings = settings.encode();
    assert_eq!(encoded_settings.len(), 9);

    let mut cursor = Cursor::new(encoded_settings);
    let read_settings = Http2Frame::read_from(&mut cursor).unwrap();
    assert_eq!(read_settings.frame_type, FRAME_SETTINGS);
    assert_eq!(read_settings.stream_id, 0);
    assert_eq!(read_settings.flags, 0);

    // Settings ACK
    let settings_ack = Http2Frame::new(FRAME_SETTINGS, FLAG_ACK, 0, Vec::new());
    assert_eq!(settings_ack.flags & FLAG_ACK, FLAG_ACK);

    // PING frame (8 bytes opaque data)
    let ping_data = [1, 2, 3, 4, 5, 6, 7, 8].to_vec();
    let ping = Http2Frame::new(FRAME_PING, 0, 0, ping_data.clone());
    let mut cursor = Cursor::new(ping.encode());
    let read_ping = Http2Frame::read_from(&mut cursor).unwrap();
    assert_eq!(read_ping.frame_type, FRAME_PING);
    assert_eq!(read_ping.payload, ping_data);

    // GOAWAY frame (last-stream-id 0, error code NO_ERROR 0)
    let mut goaway_payload = Vec::new();
    goaway_payload.extend_from_slice(&0u32.to_be_bytes()); // last_stream_id
    goaway_payload.extend_from_slice(&0u32.to_be_bytes()); // error_code
    let goaway = Http2Frame::new(FRAME_GOAWAY, 0, 0, goaway_payload);
    let mut cursor = Cursor::new(goaway.encode());
    let read_goaway = Http2Frame::read_from(&mut cursor).unwrap();
    assert_eq!(read_goaway.frame_type, FRAME_GOAWAY);
    assert_eq!(read_goaway.payload.len(), 8);
}

#[test]
fn test_hpack_static_table_and_literal_roundtrip() {
    let headers = vec![
        (":method", "GET"),
        (":path", "/index.html"),
        (":scheme", "https"),
        (":status", "200"),
        ("content-type", "application/json; charset=utf-8"),
        ("x-custom-strata-header", "StrataEngine/1.0"),
    ];

    let encoded_block = encode_hpack_headers(&headers);
    assert!(!encoded_block.is_empty());

    let decoded = decode_hpack_headers(&encoded_block).expect("Failed to decode HPACK block");

    // Check all headers are present and correctly matched
    for (expected_key, expected_val) in headers {
        let found = decoded
            .iter()
            .find(|(k, v)| k == expected_key && v == expected_val);
        assert!(
            found.is_some(),
            "Expected header '{expected_key}: {expected_val}' was not found in decoded HPACK: {decoded:?}"
        );
    }
}

#[test]
fn test_hpack_status_and_custom_headers_roundtrip() {
    let headers = vec![
        (":status", "404"),
        ("content-encoding", "gzip"),
        ("cache-control", "no-cache, no-store"),
        ("server", "Strata-Rust-Engine"),
    ];

    let encoded = encode_hpack_headers(&headers);
    let decoded = decode_hpack_headers(&encoded).unwrap();

    let map: std::collections::HashMap<String, String> = decoded.into_iter().collect();
    assert_eq!(map.get(":status").map(|s| s.as_str()), Some("404"));
    assert_eq!(map.get("content-encoding").map(|s| s.as_str()), Some("gzip"));
    assert_eq!(map.get("cache-control").map(|s| s.as_str()), Some("no-cache, no-store"));
    assert_eq!(map.get("server").map(|s| s.as_str()), Some("Strata-Rust-Engine"));
}

#[test]
fn test_http2_client_server_exchange_with_gzip_decompression() {
    use flate2::write::GzEncoder;
    use flate2::Compression;

    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind loopback TCP");
    let port = listener.local_addr().unwrap().port();

    let server_handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("Failed to accept connection");

        // 1. Read HTTP/2 Client Preface (24 bytes)
        let mut preface = [0u8; 24];
        stream.read_exact(&mut preface).expect("Failed to read preface");
        assert_eq!(&preface, HTTP2_PREFACE);

        // 2. Read Client SETTINGS frame
        let client_settings = Http2Frame::read_from(&mut stream).expect("Failed to read client settings");
        assert_eq!(client_settings.frame_type, FRAME_SETTINGS);

        // 3. Send Server SETTINGS frame & ACK client settings
        let server_settings = Http2Frame::new(FRAME_SETTINGS, 0, 0, Vec::new());
        stream.write_all(&server_settings.encode()).unwrap();
        let settings_ack = Http2Frame::new(FRAME_SETTINGS, FLAG_ACK, 0, Vec::new());
        stream.write_all(&settings_ack.encode()).unwrap();
        stream.flush().unwrap();

        // 4. Read Client SETTINGS ACK and HEADERS frames (which may arrive in either order)
        let mut headers_payload = None;
        for _ in 0..2 {
            let frame = Http2Frame::read_from(&mut stream).expect("Read client frame");
            if frame.frame_type == FRAME_SETTINGS && (frame.flags & FLAG_ACK) != 0 {
                // Client acknowledged server settings
            } else if frame.frame_type == FRAME_HEADERS {
                assert_eq!(frame.stream_id, 1);
                headers_payload = Some(frame.payload);
            }
        }
        let headers_payload = headers_payload.expect("Headers frame must be received");

        let decoded_headers = decode_hpack_headers(&headers_payload).expect("Decode client headers");
        let client_method = decoded_headers.iter().find(|(k, _)| k == ":method").map(|(_, v)| v.as_str());
        let client_path = decoded_headers.iter().find(|(k, _)| k == ":path").map(|(_, v)| v.as_str());
        assert_eq!(client_method, Some("GET"));
        assert_eq!(client_path, Some("/api/status"));

        // 5. Send Server HEADERS frame (stream 1)
        let response_headers = vec![
            (":status", "200"),
            ("content-type", "application/json"),
            ("content-encoding", "gzip"),
        ];
        let hpack_block = encode_hpack_headers(&response_headers);
        let resp_headers_frame = Http2Frame::new(FRAME_HEADERS, FLAG_END_HEADERS, 1, hpack_block);
        stream.write_all(&resp_headers_frame.encode()).unwrap();

        // 6. Gzip compress JSON payload and send in DATA frame with END_STREAM
        let raw_json = br#"{"status":"healthy","engine":"strata","version":2}"#;
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(raw_json).unwrap();
        let compressed_payload = encoder.finish().unwrap();

        let data_frame = Http2Frame::new(FRAME_DATA, FLAG_END_STREAM, 1, compressed_payload);
        stream.write_all(&data_frame.encode()).unwrap();
        stream.flush().unwrap();
        let _ = stream.shutdown(std::net::Shutdown::Write);
        let mut drain = [0u8; 128];
        while let Ok(n) = stream.read(&mut drain) {
            if n == 0 {
                break;
            }
        }
    });

    let mut client_stream = std::net::TcpStream::connect(format!("127.0.0.1:{port}"))
        .expect("Failed to connect client stream");

    let parsed_url = ParsedUrl {
        scheme: "https".to_string(),
        host: "127.0.0.1".to_string(),
        port,
        path_and_query: "/api/status".to_string(),
    };

    let req = RequestInit {
        method: "GET".to_string(),
        ..Default::default()
    };

    let response = execute_http2_request(&mut client_stream, &req, &parsed_url)
        .expect("HTTP/2 request failed");

    assert_eq!(response.status, 200);
    assert_eq!(response.status_text, "OK");
    assert_eq!(response.headers.get("content-type"), Some("application/json"));
    assert_eq!(response.headers.get("content-encoding"), Some("gzip"));

    let body_text = response.text().expect("Failed to parse response body text");
    assert_eq!(body_text, r#"{"status":"healthy","engine":"strata","version":2}"#);

    drop(client_stream);
    server_handle.join().expect("Server thread panicked");
}

#[test]
fn test_http2_frame_size_error_rejection_without_allocation() {
    // Construct a frame header claiming length 32,768 (exceeding initial 16,384 limit)
    let length = 32_768u32;
    let header = [
        ((length >> 16) & 0xFF) as u8,
        ((length >> 8) & 0xFF) as u8,
        (length & 0xFF) as u8,
        FRAME_DATA,
        0, // flags
        0, 0, 0, 1, // stream id 1
    ];
    let mut cursor = Cursor::new(header);
    let result = Http2Frame::read_from_bounded(&mut cursor, DEFAULT_MAX_FRAME_SIZE);

    assert!(result.is_err(), "Should reject frame exceeding max_frame_size");
    match result.unwrap_err() {
        FetchError::NetworkError(msg) => {
            assert!(
                msg.contains("FRAME_SIZE_ERROR"),
                "Error message should mention FRAME_SIZE_ERROR: {msg}"
            );
        }
        other => panic!("Unexpected error type: {other:?}"),
    }
}

#[test]
fn test_http2_settings_stream_id_zero_enforcement() {
    let bad_settings = Http2Frame::new(FRAME_SETTINGS, 0, 1, Vec::new());
    let mut cursor = Cursor::new(bad_settings.encode());
    let err = Http2Frame::read_from(&mut cursor).unwrap_err();
    assert!(format!("{err:?}").contains("PROTOCOL_ERROR"));
}

#[test]
fn test_http2_settings_payload_multiple_of_six() {
    let bad_settings = Http2Frame::new(FRAME_SETTINGS, 0, 0, vec![1, 2, 3, 4, 5]); // 5 bytes
    let mut cursor = Cursor::new(bad_settings.encode());
    let err = Http2Frame::read_from(&mut cursor).unwrap_err();
    assert!(format!("{err:?}").contains("FRAME_SIZE_ERROR"));
}

#[test]
fn test_http2_settings_ack_must_be_empty() {
    let bad_ack = Http2Frame::new(FRAME_SETTINGS, FLAG_ACK, 0, vec![0; 6]);
    let mut cursor = Cursor::new(bad_ack.encode());
    let err = Http2Frame::read_from(&mut cursor).unwrap_err();
    assert!(format!("{err:?}").contains("FRAME_SIZE_ERROR"));
}

#[test]
fn test_http2_ping_frame_exact_eight_bytes() {
    let ping_too_short = Http2Frame::new(FRAME_PING, 0, 0, vec![0; 7]);
    let mut cursor = Cursor::new(ping_too_short.encode());
    assert!(Http2Frame::read_from(&mut cursor).is_err());

    let ping_too_long = Http2Frame::new(FRAME_PING, 0, 0, vec![0; 9]);
    let mut cursor = Cursor::new(ping_too_long.encode());
    assert!(Http2Frame::read_from(&mut cursor).is_err());

    let ping_bad_stream = Http2Frame::new(FRAME_PING, 0, 3, vec![0; 8]);
    let mut cursor = Cursor::new(ping_bad_stream.encode());
    assert!(Http2Frame::read_from(&mut cursor).is_err());
}

#[test]
fn test_http2_rst_stream_length_and_stream_id() {
    let rst_stream_zero = Http2Frame::new(FRAME_RST_STREAM, 0, 0, vec![0; 4]);
    let mut cursor = Cursor::new(rst_stream_zero.encode());
    assert!(Http2Frame::read_from(&mut cursor).is_err());

    let rst_bad_len = Http2Frame::new(FRAME_RST_STREAM, 0, 1, vec![0; 3]);
    let mut cursor = Cursor::new(rst_bad_len.encode());
    assert!(Http2Frame::read_from(&mut cursor).is_err());
}

#[test]
fn test_http2_window_update_zero_increment_rejection() {
    let win_zero = Http2Frame::new(FRAME_WINDOW_UPDATE, 0, 0, vec![0, 0, 0, 0]);
    let mut cursor = Cursor::new(win_zero.encode());
    let err = Http2Frame::read_from(&mut cursor).unwrap_err();
    assert!(format!("{err:?}").contains("PROTOCOL_ERROR"));
}

#[test]
fn test_http2_settings_max_frame_size_negotiation() {
    let mut settings = Http2ConnectionSettings::default();
    assert_eq!(settings.max_frame_size, 16_384);

    // Apply SETTINGS frame advertising 65,536 bytes (parameter 0x5)
    let mut payload = Vec::new();
    payload.extend_from_slice(&SETTINGS_MAX_FRAME_SIZE.to_be_bytes());
    payload.extend_from_slice(&65_536u32.to_be_bytes());

    settings.apply_settings(&payload).expect("Valid SETTINGS payload");
    assert_eq!(settings.max_frame_size, 65_536);

    // Rejects below 16,384
    let mut bad_payload = Vec::new();
    bad_payload.extend_from_slice(&SETTINGS_MAX_FRAME_SIZE.to_be_bytes());
    bad_payload.extend_from_slice(&8_192u32.to_be_bytes());
    assert!(settings.apply_settings(&bad_payload).is_err());
}

#[test]
fn test_http2_padded_data_frame_encoding_and_invariants() {
    let mut payload = Vec::new();
    payload.push(4u8); // pad length
    payload.extend_from_slice(b"HELLO");
    payload.extend_from_slice(&[0u8; 4]); // 4 bytes of padding

    let frame = Http2Frame::new(FRAME_DATA, FLAG_PADDED, 1, payload);
    assert_eq!(frame.flags & FLAG_PADDED, FLAG_PADDED);
    assert_eq!(frame.stream_id, 1);
    assert!(frame.validate_invariants().is_ok());

    // Stream 0 is forbidden for DATA frames
    let bad_stream_data = Http2Frame::new(FRAME_DATA, 0, 0, b"TEST".to_vec());
    assert!(bad_stream_data.validate_invariants().is_err());
}

#[test]
fn test_http2_flow_control_window_tracking() {
    use strata_web_api::http2::FlowControlWindow;

    let mut win = FlowControlWindow::new(65_535);
    assert_eq!(win.current, 65_535);

    // Consume 10,000 bytes
    win.consume(10_000).expect("consume within window");
    assert_eq!(win.current, 55_535);

    // Update with 20,000 increment
    win.update(20_000).expect("valid update");
    assert_eq!(win.current, 75_535);

    // Zero increment must fail with PROTOCOL_ERROR
    let zero_update = win.update(0);
    assert!(zero_update.is_err(), "Zero increment must return protocol error");

    // Overflow above 2^31 - 1 must fail with FLOW_CONTROL_ERROR
    let overflow_update = win.update(0x7FFF_FFFF);
    assert!(overflow_update.is_err(), "Exceeding 2^31-1 must return flow control error");

    // Exceeding available window must fail
    let mut small_win = FlowControlWindow::new(100);
    assert!(small_win.consume(200).is_err(), "Overconsuming must return flow control error");
}



