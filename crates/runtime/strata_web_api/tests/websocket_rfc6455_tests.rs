//! W3C / RFC 6455 WebSocket Framing, SHA-1 Handshake, and State Machine tests.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

use strata_web_api::websocket::{
    base64_encode, compute_sec_websocket_accept, decode_frame, encode_frame, sha1, WebSocket,
    WsMessage, WsOpcode, WsState,
};

#[test]
fn test_rfc6455_sha1_and_accept_vector() {
    // Official RFC 6455 Section 1.3 Test Vector:
    // Client Key: dGhlIHNhbXBsZSBub25jZQ==
    // Expected Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=
    let client_key = "dGhlIHNhbXBsZSBub25jZQ==";
    let accept = compute_sec_websocket_accept(client_key);
    assert_eq!(
        accept, "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=",
        "SHA-1 Sec-WebSocket-Accept must match RFC 6455 specification"
    );

    // Standard SHA-1 test vector
    let empty_sha1 = sha1(b"");
    let hex: String = empty_sha1.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(hex, "da39a3ee5e6b4b0d3255bfef95601890afd80709");

    let fox_sha1 = sha1(b"The quick brown fox jumps over the lazy dog");
    let fox_hex: String = fox_sha1.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(fox_hex, "2fd4e1c67a2d28fced849ee1bb76e7391b93eb12");
}

#[test]
fn test_base64_encoding_vectors() {
    assert_eq!(base64_encode(b""), "");
    assert_eq!(base64_encode(b"f"), "Zg==");
    assert_eq!(base64_encode(b"fo"), "Zm8=");
    assert_eq!(base64_encode(b"foo"), "Zm9v");
    assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
    assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
    assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
}

#[test]
fn test_rfc6455_frame_roundtrip() {
    // 1. Masked text frame
    let text_payload = "Strata Pure-Rust WebSocket Engine";
    let encoded = encode_frame(
        true,
        WsOpcode::Text,
        text_payload.as_bytes(),
        true,
        Some([0x12, 0x34, 0x56, 0x78]),
    );

    let (frame, consumed) = decode_frame(&encoded)
        .expect("decode must succeed")
        .expect("frame must be complete");
    assert_eq!(consumed, encoded.len());
    assert!(frame.fin);
    assert_eq!(frame.opcode, WsOpcode::Text);
    assert_eq!(String::from_utf8(frame.payload).unwrap(), text_payload);

    // 2. Control frame: Ping
    let ping_payload = b"heartbeat";
    let ping_encoded = encode_frame(true, WsOpcode::Ping, ping_payload, false, None);
    let (ping_frame, _) = decode_frame(&ping_encoded).unwrap().unwrap();
    assert_eq!(ping_frame.opcode, WsOpcode::Ping);
    assert_eq!(ping_frame.payload, ping_payload);

    // 3. Medium payload (126-byte extended length header)
    let medium_data = vec![0xABu8; 1000];
    let medium_encoded = encode_frame(true, WsOpcode::Binary, &medium_data, true, None);
    let (medium_frame, _) = decode_frame(&medium_encoded).unwrap().unwrap();
    assert_eq!(medium_frame.payload.len(), 1000);
    assert_eq!(medium_frame.payload, medium_data);
}

#[test]
fn test_loopback_websocket_handshake_and_exchange() {
    // Spin up an ephemeral loopback WebSocket server
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().unwrap().port();

    let server_handle = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().expect("accept connection");
        let mut buf = [0u8; 2048];
        let n = socket.read(&mut buf).expect("read handshake");
        let req = String::from_utf8_lossy(&buf[..n]);

        // Find Sec-WebSocket-Key
        let mut key = "";
        for line in req.lines() {
            if let Some((k, v)) = line.split_once(':') {
                if k.trim().eq_ignore_ascii_case("sec-websocket-key") {
                    key = v.trim();
                }
            }
        }
        assert!(!key.is_empty(), "Server must receive Sec-WebSocket-Key");

        let accept = compute_sec_websocket_accept(key);
        let resp = format!(
            "HTTP/1.1 101 Switching Protocols\r\n\
             Upgrade: websocket\r\n\
             Connection: Upgrade\r\n\
             Sec-WebSocket-Accept: {}\r\n\r\n",
            accept
        );
        socket
            .write_all(resp.as_bytes())
            .expect("write handshake response");
        socket.flush().expect("flush handshake response");

        // Read client frame (Ping or Text)
        let mut frame_buf = [0u8; 1024];
        let fn_bytes = socket.read(&mut frame_buf).expect("read client frame");
        let (frame, _) = decode_frame(&frame_buf[..fn_bytes]).unwrap().unwrap();
        assert_eq!(frame.opcode, WsOpcode::Text);
        let received_text = String::from_utf8(frame.payload).unwrap();
        assert_eq!(received_text, "Hello WebSocket Server!");

        // Echo response back to client unmasked
        let reply = encode_frame(
            true,
            WsOpcode::Text,
            b"Echo: Hello WebSocket Server!",
            false,
            None,
        );
        socket.write_all(&reply).expect("write reply");
        socket.flush().expect("flush reply");

        // Keep socket open briefly for client receipt
        std::thread::sleep(Duration::from_millis(100));
    });

    // Connect client
    let url = format!("ws://127.0.0.1:{port}/chat");
    let client = WebSocket::connect(&url).expect("client connect must succeed");
    assert_eq!(client.ready_state(), WsState::Open);
    assert!(client.is_open());

    client
        .send_text("Hello WebSocket Server!")
        .expect("send text");

    let received = client.recv_message_timeout(Duration::from_secs(3));
    assert!(received.is_some(), "Client must receive echo message");
    match received.unwrap() {
        WsMessage::Text(t) => assert_eq!(t, "Echo: Hello WebSocket Server!"),
        other => panic!("Unexpected message type: {other:?}"),
    }

    client.close(1000, "Normal closure").expect("close");
    server_handle.join().expect("server thread join");
}

#[test]
fn test_rfc6455_fragmented_text_frames_reassembly() {
    let ws = WebSocket::new("ws://example.com/stream");

    // Frame 1: Initial fragmented text frame (FIN = false)
    let f1 = encode_frame(false, WsOpcode::Text, b"Hello ", false, None);
    // Frame 2: Continuation frame (FIN = false)
    let f2 = encode_frame(false, WsOpcode::Continuation, b"Fragmented ", false, None);
    // Frame 3: Final continuation frame (FIN = true)
    let f3 = encode_frame(true, WsOpcode::Continuation, b"World!", false, None);

    let mut stream = Vec::new();
    stream.extend_from_slice(&f1);
    stream.extend_from_slice(&f2);
    stream.extend_from_slice(&f3);

    let consumed = ws
        .ingest_incoming_bytes(&stream)
        .expect("ingest must succeed");
    assert_eq!(consumed, stream.len());

    let msg = ws.poll_message().expect("must receive reassembled message");
    match msg {
        WsMessage::Text(s) => assert_eq!(s, "Hello Fragmented World!"),
        other => panic!("Expected WsMessage::Text, got {other:?}"),
    }
    assert!(ws.poll_message().is_none());
}

#[test]
fn test_rfc6455_fragmented_binary_with_interleaved_ping() {
    let ws = WebSocket::new("ws://example.com/binary-stream");

    // Frame 1: Binary fragment (FIN = false)
    let f1 = encode_frame(false, WsOpcode::Binary, &[0x01, 0x02, 0x03], false, None);
    // Frame 2: Interleaved Ping control frame (FIN = true) per RFC 6455 §5.4
    let f2 = encode_frame(true, WsOpcode::Ping, b"heartbeat", false, None);
    // Frame 3: Final Binary continuation frame (FIN = true)
    let f3 = encode_frame(
        true,
        WsOpcode::Continuation,
        &[0x04, 0x05, 0x06],
        false,
        None,
    );

    let mut stream = Vec::new();
    stream.extend_from_slice(&f1);
    stream.extend_from_slice(&f2);
    stream.extend_from_slice(&f3);

    ws.ingest_incoming_bytes(&stream)
        .expect("ingest must succeed");

    // Ping control message should be processed immediately
    let m1 = ws.poll_message().expect("first message");
    match m1 {
        WsMessage::Ping(p) => assert_eq!(p, b"heartbeat"),
        other => panic!("Expected Ping, got {other:?}"),
    }

    // Reassembled binary message should follow intact
    let m2 = ws.poll_message().expect("second message");
    match m2 {
        WsMessage::Binary(b) => assert_eq!(b, vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06]),
        other => panic!("Expected Binary, got {other:?}"),
    }
    assert!(ws.poll_message().is_none());
}

#[test]
fn test_rfc6455_unexpected_continuation_frame_returns_protocol_error() {
    let ws = WebSocket::new("ws://example.com/test");

    // Orphaned continuation frame with no prior unfinalized message
    let orphan = encode_frame(true, WsOpcode::Continuation, b"orphan payload", false, None);
    let res = ws.ingest_incoming_bytes(&orphan);
    assert!(
        res.is_err(),
        "Orphaned continuation frame must return ProtocolError"
    );
    match res.unwrap_err() {
        strata_web_api::websocket::WsError::ProtocolError(msg) => {
            assert!(msg.contains("Unexpected continuation frame"));
        }
        other => panic!("Expected ProtocolError, got {other:?}"),
    }
}

#[test]
fn test_rfc6455_server_masked_frame_rejected() {
    let ws = WebSocket::new("ws://example.com/test");
    // Server frame masked (RFC 6455 §5.1 violation)
    let masked_server_frame = encode_frame(
        true,
        WsOpcode::Text,
        b"Illegal masked server message",
        true,
        Some([0xAA, 0xBB, 0xCC, 0xDD]),
    );

    let res = ws.ingest_incoming_bytes(&masked_server_frame);
    assert!(res.is_err(), "Masked frame from server must be rejected");
    match res.unwrap_err() {
        strata_web_api::websocket::WsError::ProtocolError(msg) => {
            assert!(msg.contains("must not be masked"));
        }
        other => panic!("Expected ProtocolError, got {other:?}"),
    }
}

#[test]
fn test_rfc6455_control_frame_fragmentation_rejected() {
    // Control frame with fin = false (RFC 6455 §5.5 violation)
    let fragmented_ping = encode_frame(false, WsOpcode::Ping, b"ping", false, None);
    let res = decode_frame(&fragmented_ping);
    assert!(res.is_err(), "Fragmented control frame must return error");
    match res.unwrap_err() {
        strata_web_api::websocket::WsError::ProtocolError(msg) => {
            assert!(msg.contains("Control frames must not be fragmented"));
        }
        other => panic!("Expected ProtocolError, got {other:?}"),
    }
}

#[test]
fn test_rfc6455_control_frame_oversized_payload_rejected() {
    // Control frame with payload > 125 bytes (RFC 6455 §5.5 violation)
    let large_payload = vec![0x42; 126];
    let oversized_ping = encode_frame(true, WsOpcode::Ping, &large_payload, false, None);
    let res = decode_frame(&oversized_ping);
    assert!(res.is_err(), "Oversized control frame must return error");
    match res.unwrap_err() {
        strata_web_api::websocket::WsError::ProtocolError(msg) => {
            assert!(msg.contains("payload length > 125 bytes"));
        }
        other => panic!("Expected ProtocolError, got {other:?}"),
    }
}

#[test]
fn test_rfc6455_new_data_frame_during_active_fragmentation_rejected() {
    let ws = WebSocket::new("ws://example.com/test");

    // Start a fragmented text message (FIN = false)
    let f1 = encode_frame(false, WsOpcode::Text, b"Part 1...", false, None);
    ws.ingest_incoming_bytes(&f1)
        .expect("initial fragment accepted");

    // Receive a new Binary frame instead of Continuation frame (RFC 6455 §5.4 violation)
    let f2 = encode_frame(
        true,
        WsOpcode::Binary,
        b"interleaved new stream",
        false,
        None,
    );
    let res = ws.ingest_incoming_bytes(&f2);
    assert!(res.is_err(), "Interleaved new data frame must be rejected");
    match res.unwrap_err() {
        strata_web_api::websocket::WsError::ProtocolError(msg) => {
            assert!(msg.contains("Cannot start new data frame"));
        }
        other => panic!("Expected ProtocolError, got {other:?}"),
    }
}

#[test]
fn test_rfc6455_client_fragmented_send_and_reassembly() {
    let ws = WebSocket::new("ws://example.com/test");
    ws.set_open();

    // Send a 150KB payload using automatic fragmentation (> 64KB DEFAULT_MAX_FRAME_SIZE)
    let large_text = "Strata RFC 6455 WebSocket High Throughput Streaming Test ".repeat(2600); // ~150 KB
    assert!(large_text.len() > 65536);

    ws.send_text(&large_text).expect("send_text must succeed");

    let queued_frames = ws.drain_outgoing_frames();
    assert!(
        queued_frames.len() >= 3,
        "Payload should be split into multiple frames"
    );

    // Verify frames properties: first has opcode Text, continuation has Continuation, only last has FIN
    for (i, frame_bytes) in queued_frames.iter().enumerate() {
        let (decoded, consumed) = decode_frame(frame_bytes).unwrap().unwrap();
        assert_eq!(consumed, frame_bytes.len());
        assert!(
            decoded.masked,
            "Client frames must be masked (RFC 6455 §5.1)"
        );

        if i == 0 {
            assert_eq!(decoded.opcode, WsOpcode::Text);
            assert!(!decoded.fin);
        } else if i == queued_frames.len() - 1 {
            assert_eq!(decoded.opcode, WsOpcode::Continuation);
            assert!(decoded.fin);
        } else {
            assert_eq!(decoded.opcode, WsOpcode::Continuation);
            assert!(!decoded.fin);
        }
    }

    // Now re-assemble using a receiving WebSocket (simulate server forwarding unmasked to another client)
    let receiver = WebSocket::new("ws://example.com/receiver");
    for frame_bytes in queued_frames {
        let (decoded, _) = decode_frame(&frame_bytes).unwrap().unwrap();
        // Server would forward unmasked frames to client
        let unmasked_frame =
            encode_frame(decoded.fin, decoded.opcode, &decoded.payload, false, None);
        receiver
            .ingest_incoming_bytes(&unmasked_frame)
            .expect("ingest must succeed");
    }

    let reassembled = receiver.poll_message().expect("message must be ready");
    match reassembled {
        WsMessage::Text(t) => assert_eq!(t, large_text),
        other => panic!("Expected Text message, got {other:?}"),
    }
}
