//! Integration tests connecting live AppRuntime to Chrome DevTools Protocol (CDP) Bridge and Server.

use strata::prelude::*;
use strata::runtime::AppRuntime;
use strata_devtools::cdp::{CdpDispatcher, WebSocketCodec, WebSocketOpcode};
use strata_devtools::CdpServer;

#[test]
fn test_app_runtime_cdp_live_inspection_over_websocket() {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    // 1. Create a live AppRuntime with an RSX view
    let view = rsx! {
        <div class="p-4 flex bg-neutral-900">
            <button class="px-2 py-1">"Click me"</button>
            <span>"Ready"</span>
        </div>
    };
    let mut runtime = AppRuntime::new(view, 800.0, 600.0, 1.0);
    runtime.tick_frame();

    // 2. Initialize CDP dispatcher and sync live state from AppRuntime
    let dispatcher = CdpDispatcher::new();
    runtime.sync_to_devtools(&dispatcher);

    // 3. Start CdpServer on an ephemeral port
    let mut server = CdpServer::start(dispatcher, 0).expect("Start CDP server");
    let port = server.port();
    assert!(port > 0);

    // 4. Connect via TCP, perform RFC 6455 WebSocket handshake
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("Connect to CDP server");
    stream
        .set_read_timeout(Some(Duration::from_millis(1000)))
        .unwrap();

    let upgrade = "GET /devtools/page/1 HTTP/1.1\r\n\
                   Host: 127.0.0.1\r\n\
                   Upgrade: websocket\r\n\
                   Connection: Upgrade\r\n\
                   Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
                   Sec-WebSocket-Version: 13\r\n\r\n";
    stream.write_all(upgrade.as_bytes()).unwrap();

    let mut buf = [0u8; 1024];
    let n = stream.read(&mut buf).unwrap();
    let handshake_resp = String::from_utf8_lossy(&buf[..n]);
    assert!(handshake_resp.starts_with("HTTP/1.1 101 Switching Protocols"));
    assert!(handshake_resp.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo="));

    // 5. Query DOM.getDocument over WebSocket
    let doc_req = b"{\"id\":1,\"method\":\"DOM.getDocument\"}";
    let mask = [0xAA, 0xBB, 0xCC, 0xDD];
    let mut doc_frame = vec![0x81, 0x80 | (doc_req.len() as u8)];
    doc_frame.extend_from_slice(&mask);
    for (i, &b) in doc_req.iter().enumerate() {
        doc_frame.push(b ^ mask[i % 4]);
    }
    stream.write_all(&doc_frame).unwrap();

    let mut resp_buf = [0u8; 4096];
    let m = stream.read(&mut resp_buf).unwrap();
    let (frame, _) = WebSocketCodec::decode_frame(&resp_buf[..m]).expect("Decode frame");
    assert_eq!(frame.opcode, WebSocketOpcode::Text);
    let doc_json = std::str::from_utf8(&frame.payload).unwrap();
    assert!(doc_json.contains("\"id\":1"));
    assert!(doc_json.contains("\"nodeName\":\"#document\""));
    assert!(doc_json.contains("\"nodeName\":\"DIV\""));
    assert!(doc_json.contains("\"nodeName\":\"BUTTON\""));
    assert!(doc_json.contains("\"nodeName\":\"SPAN\""));

    // 6. Query Performance.getMetrics over WebSocket
    let perf_req = b"{\"id\":2,\"method\":\"Performance.getMetrics\"}";
    let mut perf_frame = vec![0x81, 0x80 | (perf_req.len() as u8)];
    perf_frame.extend_from_slice(&mask);
    for (i, &b) in perf_req.iter().enumerate() {
        perf_frame.push(b ^ mask[i % 4]);
    }
    stream.write_all(&perf_frame).unwrap();

    let m2 = stream.read(&mut resp_buf).unwrap();
    let (frame2, _) = WebSocketCodec::decode_frame(&resp_buf[..m2]).expect("Decode frame 2");
    assert_eq!(frame2.opcode, WebSocketOpcode::Text);
    let perf_json = std::str::from_utf8(&frame2.payload).unwrap();
    assert!(perf_json.contains("\"id\":2"));
    assert!(perf_json.contains("\"name\":\"LayoutCount\""));
    assert!(perf_json.contains("\"name\":\"Frames\""));

    server.stop();
}

#[test]
fn test_app_runtime_devtools_hud_f12_toggle_and_profiler_timing() {
    use strata_platform::{KeyCode, Modifiers, PlatformEvent};

    let view = rsx! {
        <div class="p-4 bg-gray-900">
            <h1 class="text-xl">"HUD Profiler Integration"</h1>
        </div>
    };
    let mut runtime = AppRuntime::new(view, 1024.0, 768.0, 1.0);

    // Initialize DevTools on runtime
    let dt = runtime.enable_devtools();
    assert!(!dt.hud.state.is_visible);

    // Send F12 key down event to AppRuntime
    let f12 = PlatformEvent::KeyDown {
        key: KeyCode::F12,
        modifiers: Modifiers::default(),
    };
    let intercepted = runtime.step_event(&f12);
    assert!(intercepted, "F12 key event must be intercepted by DevTools");
    assert!(runtime.devtools().unwrap().hud.state.is_visible, "HUD must become visible upon F12");

    // Ticking frame with HUD visible appends HUD overlay to display list
    runtime.tick_frame();
    if let Some(ref mut dt) = runtime.devtools {
        dt.hud.render_overlay(Some(&dt.profiler), &mut runtime.display_list);
    }

    // Verify display list contains HUD commands
    assert!(
        runtime.display_list.len() > 10,
        "Display list must include HUD background, nav bar, and diagnostics"
    );
}

