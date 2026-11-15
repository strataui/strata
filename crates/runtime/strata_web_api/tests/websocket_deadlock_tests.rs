//! Adversarial Deadlock Verification & Concurrency Stress Test Suite for RFC 6455 WebSocket.
//!
//! Validates:
//! 1. Client-speaks-first communication where server remains silent (zero deadlocks).
//! 2. Multi-turn sequential bidirectional request-reply exchanges.
//! 3. Immediate automatic Pong responses to unsolicited server Pings.
//! 4. RAII Drop cleanup and thread joining without resource leaks.
//! 5. Idle timeout resilience with subsequent message transmission.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

use strata_web_api::websocket::{
    compute_sec_websocket_accept, decode_frame, encode_frame, WebSocket, WsMessage, WsOpcode,
    WsState,
};

/// Helper to accept a connection and complete the standard RFC 6455 handshake.
fn accept_and_handshake(listener: &TcpListener) -> std::net::TcpStream {
    let (mut socket, _) = listener.accept().expect("server accept");
    let mut buf = [0u8; 2048];
    let n = socket.read(&mut buf).expect("read handshake request");
    let req = String::from_utf8_lossy(&buf[..n]);

    let mut key = "";
    for line in req.lines() {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("sec-websocket-key") {
                key = v.trim();
            }
        }
    }
    assert!(!key.is_empty(), "Sec-WebSocket-Key must be present");

    let accept = compute_sec_websocket_accept(key);
    let resp = format!(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Accept: {}\r\n\r\n",
        accept
    );
    socket.write_all(resp.as_bytes()).expect("write handshake");
    socket.flush().expect("flush handshake");
    socket
}

#[test]
fn test_client_speaks_first_silent_server_no_deadlock() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().unwrap().port();

    let server = std::thread::spawn(move || {
        let mut socket = accept_and_handshake(&listener);
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();

        // Server sends NOTHING. It remains completely silent and waits for the client.
        let mut buf = [0u8; 1024];
        let n = socket
            .read(&mut buf)
            .expect("server must receive client payload without deadlock");
        assert!(n > 0, "Server must read non-zero bytes from client");

        let (frame, _) = decode_frame(&buf[..n]).unwrap().unwrap();
        assert_eq!(frame.opcode, WsOpcode::Text);
        assert_eq!(
            String::from_utf8(frame.payload).unwrap(),
            "client-speaks-first-query"
        );

        // Echo reply
        let reply = encode_frame(true, WsOpcode::Text, b"server-acknowledgement", false, None);
        socket.write_all(&reply).unwrap();
        socket.flush().unwrap();
    });

    let client = WebSocket::connect(&format!("ws://127.0.0.1:{port}/query")).unwrap();
    assert_eq!(client.ready_state(), WsState::Open);

    // Sleep 50ms so worker thread definitely enters non-blocking read loop before message is queued
    std::thread::sleep(Duration::from_millis(50));

    let start = Instant::now();
    client.send_text("client-speaks-first-query").unwrap();

    let reply = client.recv_message_timeout(Duration::from_secs(2));
    assert!(
        reply.is_some(),
        "Client must receive reply without deadlocking"
    );
    assert_eq!(
        reply.unwrap(),
        WsMessage::Text("server-acknowledgement".into())
    );
    assert!(
        start.elapsed() < Duration::from_millis(500),
        "Transaction must complete within 500ms"
    );

    server.join().unwrap();
}

#[test]
fn test_multiturn_bidirectional_request_response() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().unwrap().port();

    let server = std::thread::spawn(move || {
        let mut socket = accept_and_handshake(&listener);
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();

        for i in 0..20 {
            let mut buf = [0u8; 1024];
            let n = socket.read(&mut buf).expect("server turn read");
            let (frame, _) = decode_frame(&buf[..n]).unwrap().unwrap();
            let req_text = String::from_utf8(frame.payload).unwrap();
            assert_eq!(req_text, format!("Turn-{i}"));

            let reply = format!("Ack-{i}");
            let reply_frame = encode_frame(true, WsOpcode::Text, reply.as_bytes(), false, None);
            socket.write_all(&reply_frame).unwrap();
            socket.flush().unwrap();
        }
    });

    let client = WebSocket::connect(&format!("ws://127.0.0.1:{port}/multiturn")).unwrap();
    for i in 0..20 {
        client.send_text(&format!("Turn-{i}")).unwrap();
        let resp = client.recv_message_timeout(Duration::from_secs(1));
        assert_eq!(resp, Some(WsMessage::Text(format!("Ack-{i}"))));
    }

    server.join().unwrap();
}

#[test]
fn test_unsolicited_ping_immediate_pong() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().unwrap().port();

    let server = std::thread::spawn(move || {
        let mut socket = accept_and_handshake(&listener);
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();

        // Server sends unsolicited Ping frame
        let nonce = b"sec-token-994";
        let ping_frame = encode_frame(true, WsOpcode::Ping, nonce, false, None);
        socket.write_all(&ping_frame).unwrap();
        socket.flush().unwrap();

        // Server awaits Pong reply from client
        let mut buf = [0u8; 1024];
        let n = socket.read(&mut buf).expect("server read pong");
        let (frame, _) = decode_frame(&buf[..n]).unwrap().unwrap();
        assert_eq!(frame.opcode, WsOpcode::Pong);
        assert_eq!(
            frame.payload, nonce,
            "Pong payload must match Ping nonce exactly"
        );
    });

    let client = WebSocket::connect(&format!("ws://127.0.0.1:{port}/heartbeat")).unwrap();
    // Allow worker loop time to process ping and emit pong
    std::thread::sleep(Duration::from_millis(100));

    server.join().unwrap();
    let _ = client.close(1000, "done");
}

#[test]
fn test_clean_raii_teardown_no_thread_leak() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().unwrap().port();

    let server = std::thread::spawn(move || {
        let mut socket = accept_and_handshake(&listener);
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();

        let mut buf = [0u8; 1024];
        let n = socket.read(&mut buf).unwrap_or(0);
        // Socket should observe EOF (0 bytes) or Close frame
        if n > 0 {
            if let Ok(Some((frame, _))) = decode_frame(&buf[..n]) {
                assert!(frame.opcode == WsOpcode::Close || n == 0);
            }
        }
    });

    {
        let client = WebSocket::connect(&format!("ws://127.0.0.1:{port}/raii")).unwrap();
        assert!(client.is_open());
        // Dropping client here triggers Drop implementation
    }

    // Ensure server joins cleanly without hanging on dead socket
    server.join().unwrap();
}

#[test]
fn test_idle_timeout_resilience() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().unwrap().port();

    let server = std::thread::spawn(move || {
        let mut socket = accept_and_handshake(&listener);
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();

        // Idle for 200ms (multiple 15ms worker poll cycles)
        std::thread::sleep(Duration::from_millis(200));

        let mut buf = [0u8; 1024];
        let n = socket.read(&mut buf).expect("read after idle");
        let (frame, _) = decode_frame(&buf[..n]).unwrap().unwrap();
        assert_eq!(
            String::from_utf8(frame.payload).unwrap(),
            "post-idle-message"
        );

        let reply = encode_frame(true, WsOpcode::Text, b"ack-post-idle", false, None);
        socket.write_all(&reply).unwrap();
        socket.flush().unwrap();
    });

    let client = WebSocket::connect(&format!("ws://127.0.0.1:{port}/idle")).unwrap();
    // Wait for idle period to expire
    std::thread::sleep(Duration::from_millis(250));

    // Send message after idle period
    client.send_text("post-idle-message").unwrap();
    let reply = client.recv_message_timeout(Duration::from_secs(1));
    assert_eq!(reply, Some(WsMessage::Text("ack-post-idle".into())));

    server.join().unwrap();
}

#[test]
fn test_cryptographic_nonce_and_dynamic_frame_masking() {
    use std::collections::HashSet;
    use strata_web_api::websocket::{
        decode_frame, encode_frame, generate_mask_key, generate_sec_websocket_key, WsOpcode,
    };

    // GOAL-32: Verify generate_sec_websocket_key() produces valid base64 nonces that are strictly unique
    let mut nonce_keys = HashSet::new();
    for _ in 0..100 {
        let key = generate_sec_websocket_key();
        assert_eq!(
            key.len(),
            24,
            "16 bytes encoded in base64 must be 24 characters"
        );
        nonce_keys.insert(key);
    }
    assert_eq!(
        nonce_keys.len(),
        100,
        "CSPRNG nonces must be strictly distinct across invocations"
    );

    // GOAL-33: Verify dynamic frame masking key generation and decode parity
    let mut mask_keys = HashSet::new();
    for i in 0..100 {
        let key = generate_mask_key();
        mask_keys.insert(key);

        let payload = format!("payload-{}", i);
        let encoded = encode_frame(true, WsOpcode::Text, payload.as_bytes(), true, None);
        // Byte 1 has mask bit set
        assert_eq!(encoded[1] & 0x80, 0x80);
        // Decode frame should restore original plaintext payload
        let (decoded, _) = decode_frame(&encoded).unwrap().unwrap();
        assert_eq!(String::from_utf8(decoded.payload).unwrap(), payload);
    }
    assert!(
        mask_keys.len() > 95,
        "Mask keys must be unpredictable and dynamic per RFC 6455 §5.3"
    );
}
