//! Comprehensive integration and unit test suite for the Strata MCP Server,
//! authentic Yaru cursor overlay engine, action batching, and debug build policy.

use serde_json::json;
use std::time::{Duration, Instant};
use strata::prelude::*;
use strata_mcp::protocol::*;
use strata_mcp::server::McpServer;
use strata_platform::{CursorIcon as PlatformCursorIcon, CursorTheme, McpCursorState};

#[test]
fn test_mcp_server_initialize_and_tools_list() {
    let server = McpServer::default();

    // 1. Initialize
    let init_req = JsonRpcRequest::new(
        Some(json!(1)),
        "initialize",
        Some(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "StrataTestClient", "version": "1.0.0" }
        })),
    );
    let init_resp = server.handle_request(&init_req).expect("Response expected");
    assert!(init_resp.is_success());
    let init_result = init_resp.result.unwrap();
    assert_eq!(init_result["serverInfo"]["name"], "strata-mcp");

    // 2. Tools list
    let list_req = JsonRpcRequest::new(Some(json!(2)), "tools/list", None);
    let list_resp = server.handle_request(&list_req).expect("Response expected");
    assert!(list_resp.is_success());
    let tools = list_resp.result.unwrap()["tools"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(tools.len(), 15);

    let tool_names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(tool_names.contains(&"strata_session_info"));
    assert!(tool_names.contains(&"strata_run_app"));
    assert!(tool_names.contains(&"strata_take_screenshot"));
    assert!(tool_names.contains(&"strata_mouse_move"));
    assert!(tool_names.contains(&"strata_mouse_click"));
    assert!(tool_names.contains(&"strata_mouse_down"));
    assert!(tool_names.contains(&"strata_mouse_up"));
    assert!(tool_names.contains(&"strata_mouse_scroll"));
    assert!(tool_names.contains(&"strata_mouse_wheel"));
    assert!(tool_names.contains(&"strata_keyboard_type"));
    assert!(tool_names.contains(&"strata_keyboard_key"));
    assert!(tool_names.contains(&"strata_key_press"));
    assert!(tool_names.contains(&"strata_execute_actions"));
    assert!(tool_names.contains(&"strata_inspect_element"));
    assert!(tool_names.contains(&"strata_get_perf_profile"));

    // Verify ToolCallResult error serializes camelCase `isError` per MCP specification
    let err_json = serde_json::to_value(ToolCallResult::error("boom")).unwrap();
    assert_eq!(err_json["isError"], json!(true));
    assert!(err_json.get("is_error").is_none());
}

#[test]
fn test_mcp_cursor_motion_and_2_5s_fade_physics() {
    let mut cursor = McpCursorState::new(100.0, 100.0);
    assert_eq!(cursor.current_x, 100.0);
    assert_eq!(cursor.current_y, 100.0);
    assert_eq!(cursor.fade_opacity, 1.0);
    assert!(cursor.is_visible());

    // 1. Instant jump
    cursor.move_to(350.0, 420.0, false);
    assert_eq!(cursor.current_x, 350.0);
    assert_eq!(cursor.current_y, 420.0);
    assert!(!cursor.is_moving);

    // 2. Smooth movement (cubic ease-out)
    cursor.move_to(800.0, 600.0, true);
    assert!(cursor.is_moving);
    assert_eq!(cursor.start_x, 350.0);
    assert_eq!(cursor.start_y, 420.0);
    assert_eq!(cursor.target_x, 800.0);
    assert_eq!(cursor.target_y, 600.0);

    // Poll tick while interpolating
    let changed = cursor.poll_tick();
    assert!(changed);
    assert!(cursor.current_x >= 350.0 && cursor.current_x <= 800.0);
    assert!(cursor.current_y >= 420.0 && cursor.current_y <= 600.0);

    // 3. Inactivity timer: simulate 2.5s passage
    cursor.last_activity = Instant::now() - Duration::from_millis(2650);
    let changed_fade = cursor.poll_tick();
    assert!(changed_fade);
    // After 2.65s (150ms into 300ms fade), opacity should be roughly 0.5
    assert!(cursor.fade_opacity < 0.9);
    assert!(cursor.fade_opacity > 0.0);

    // Simulate complete fade out (3.0s elapsed)
    cursor.last_activity = Instant::now() - Duration::from_millis(3000);
    cursor.poll_tick();
    assert!(cursor.fade_opacity <= 0.001);
    assert!(!cursor.is_visible());

    // 4. Any new interaction immediately restores opacity to 1.0
    cursor.touch_activity();
    assert_eq!(cursor.fade_opacity, 1.0);
    assert!(cursor.is_visible());
}

#[test]
fn test_mcp_cursor_theme_loading_and_fallbacks() {
    let theme = CursorTheme::load_active();
    assert!(!theme.name.is_empty());
    assert_eq!(theme.size, 24);

    let default_bmp = theme.get(PlatformCursorIcon::Default);
    assert_eq!(default_bmp.width, 24);
    assert_eq!(default_bmp.height, 24);
    assert_eq!(default_bmp.xhot, 4);
    assert_eq!(default_bmp.yhot, 4);

    let pointer_bmp = theme.get(PlatformCursorIcon::Pointer);
    assert_eq!(pointer_bmp.xhot, 8);
    assert_eq!(pointer_bmp.yhot, 5);

    let text_bmp = theme.get(PlatformCursorIcon::Text);
    assert_eq!(text_bmp.xhot, 11);
    assert_eq!(text_bmp.yhot, 12);
}

#[test]
fn test_app_runtime_mcp_bridge_commands_and_cursor_emulation() {
    let view = rsx! {
        <div class="flex flex-col p-4 w-[800px] h-[600px] bg-neutral-900 text-white">
            <button class="px-4 py-2 bg-blue-600 hover:bg-blue-500 rounded cursor-pointer" on_click={|_| {}}>
                "Click Target"
            </button>
            <input
                id="search_input"
                type="text"
                placeholder="Search..."
                class="mt-4 p-2 bg-neutral-800 text-white cursor-text"
            />
        </div>
    };

    let mut runtime = AppRuntime::new(view, 800.0, 600.0, 1.0);
    runtime.tick_frame();
    runtime.finalize_frame_telemetry("Dirty");

    // 1. Session info command reports real non-zero frame_count
    let (info_res, _) =
        strata::mcp_bridge::execute_mcp_command_core(&mut runtime, "session_info", json!({}));
    let info = info_res.expect("session_info must succeed");
    assert_eq!(info["viewport_width"], 800.0);
    assert_eq!(info["viewport_height"], 600.0);
    assert!(info["frame_count"].as_u64().unwrap() >= 1);

    // 2. Mouse move to hover button must NOT force needs_rebuild = true!
    let (move_res, move_outcome) = strata::mcp_bridge::execute_mcp_command_with_outcome(
        &mut runtime,
        "mouse_move",
        json!({ "x": 50.0, "y": 25.0, "smooth": true }),
    );
    let move_val = move_res.expect("mouse_move must succeed");
    assert_eq!(move_val["smooth"], true);
    assert!(
        !move_outcome.needs_rebuild,
        "paint-only hover mouse_move must NOT force a full DOM rebuild"
    );
    assert!(move_outcome.needs_redraw);
    assert_eq!(move_val["hover_fast_path_used"], true);
    assert_eq!(
        runtime.mcp_cursor.as_ref().unwrap().cursor_icon,
        PlatformCursorIcon::Pointer
    );

    // 3. Mouse move to input field (within bounds x: 16..111, y: 72..112)
    let (move_input, _) = strata::mcp_bridge::execute_mcp_command_core(
        &mut runtime,
        "mouse_move",
        json!({ "x": 50.0, "y": 85.0, "smooth": false }),
    );
    let _ = move_input.expect("mouse_move to input must succeed");
    assert_eq!(
        runtime.mcp_cursor.as_ref().unwrap().cursor_icon,
        PlatformCursorIcon::Text
    );

    // 4. Click input to focus
    let (click_res, _) = strata::mcp_bridge::execute_mcp_command_core(
        &mut runtime,
        "mouse_click",
        json!({ "x": 50.0, "y": 85.0 }),
    );
    assert!(click_res.is_ok());
    assert!(runtime.active_focus.is_some());

    // 5. Keyboard type
    let (type_res, _) = strata::mcp_bridge::execute_mcp_command_core(
        &mut runtime,
        "keyboard_type",
        json!({ "text": "Strata MCP" }),
    );
    assert!(type_res.is_ok());
    assert_eq!(
        runtime.active_focus.as_ref().unwrap().state.text.get(),
        "Strata MCP"
    );

    // 6. Keyboard key: Backspace & punctuation shortcuts via key_press
    let (key_res, _) = strata::mcp_bridge::execute_mcp_command_core(
        &mut runtime,
        "keyboard_key",
        json!({ "key": "backspace" }),
    );
    assert!(key_res.is_ok());
    assert_eq!(
        runtime.active_focus.as_ref().unwrap().state.text.get(),
        "Strata MC"
    );

    let (slash_res, _) = strata::mcp_bridge::execute_mcp_command_core(
        &mut runtime,
        "key_press",
        json!({ "key": "Slash", "modifiers": { "ctrl": true } }),
    );
    let slash_val = slash_res.expect("key_press Slash must succeed");
    assert_eq!(slash_val["resolved_key"], "Slash");

    let (wheel_res, _) = strata::mcp_bridge::execute_mcp_command_core(
        &mut runtime,
        "mouse_wheel",
        json!({ "delta_x": 0.0, "delta_y": -120.0, "x": 100.0, "y": 100.0 }),
    );
    assert!(wheel_res.is_ok());

    // 7. Safe execute_actions batch
    let (batch_res, _) = strata::mcp_bridge::execute_mcp_command_core(
        &mut runtime,
        "execute_actions",
        json!({
            "actions": [
                { "action": "move", "x": 200.0, "y": 150.0, "smooth": false },
                { "action": "click", "button": "left" },
                { "action": "type", "text": "!" }
            ]
        }),
    );
    let batch = batch_res.expect("execute_actions must succeed");
    assert_eq!(batch["executed_actions"], 3);

    // 8. Screenshot capture returns exact integer width & height (fixes (0x0) bug!)
    let (shot_res, _) =
        strata::mcp_bridge::execute_mcp_command_core(&mut runtime, "take_screenshot", json!({}));
    let shot = shot_res.expect("take_screenshot must succeed");
    let b64 = shot["png_base64"].as_str().unwrap();
    assert!(!b64.is_empty());
    assert_eq!(shot["width"].as_u64(), Some(800));
    assert_eq!(shot["height"].as_u64(), Some(600));

    // 9. Programmatic profiling endpoint returns real p50/p95/p99 and counters
    let (prof_res, _) =
        strata::mcp_bridge::execute_mcp_command_core(&mut runtime, "get_perf_profile", json!({}));
    let prof = prof_res.expect("get_perf_profile must succeed");
    assert!(prof["total_frames"].as_u64().unwrap() >= 1);
    assert!(prof["last_frame"]["dom_nodes"].as_u64().unwrap() > 0);
    assert!(prof["avg_tile_cache_hit_pct"].as_f64().is_some());
    assert!(prof["total_cpu"]["p95_us"].as_u64().is_some());
}

#[test]
fn test_strata_run_app_security_and_rce_mitigation() {
    let server = McpServer::default();

    // 1. Invalid execution mode rejected
    let invalid_mode_res = server.call_tool("strata_run_app", json!({ "mode": "malicious_mode" }));
    assert_eq!(invalid_mode_res.is_error, Some(true));
    if let McpContent::Text { text } = &invalid_mode_res.content[0] {
        assert!(text.contains("Invalid execution mode"));
    } else {
        panic!("Expected text response");
    }

    // 2. Path outside authorized workspace root rejected
    let outside_path = if cfg!(windows) {
        "C:\\Windows\\System32"
    } else {
        "/tmp"
    };
    let outside_res = server.call_tool(
        "strata_run_app",
        json!({ "mode": "debug", "project_dir": outside_path }),
    );
    assert_eq!(outside_res.is_error, Some(true));
    if let McpContent::Text { text } = &outside_res.content[0] {
        assert!(
            text.contains("outside the authorized workspace roots")
                || text.contains("does not contain a Cargo.toml")
        );
    } else {
        panic!("Expected text response");
    }

    // 3. Non-existent directory rejected
    let nonexistent_res = server.call_tool(
        "strata_run_app",
        json!({ "mode": "debug", "project_dir": "/path/to/nonexistent/strata/dir/12345" }),
    );
    assert_eq!(nonexistent_res.is_error, Some(true));
    if let McpContent::Text { text } = &nonexistent_res.content[0] {
        assert!(text.contains("does not exist") || text.contains("cannot be resolved"));
    } else {
        panic!("Expected text response");
    }
}

#[cfg(unix)]
static MCP_BRIDGE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(unix)]
#[test]
fn test_mcp_server_end_to_end_unix_socket_async_screenshot_and_new_tools() {
    let _bridge_guard = MCP_BRIDGE_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let sock_path = std::env::temp_dir().join(format!(
        "strata-mcp-e2e-{}-{}.sock",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&sock_path);
    strata::mcp_bridge::start_mcp_dev_listener(Some(sock_path.clone()))
        .expect("start_mcp_dev_listener");

    let stop = Arc::new(AtomicBool::new(false));
    let stop_ui = Arc::clone(&stop);

    // Run MCP server tool calls from a client thread while this test thread pumps the UI runtime
    let sock_client = sock_path.clone();
    let client_thread = std::thread::spawn(move || {
        let server = McpServer::new(strata_mcp::client::StrataDevClient::new(sock_client));

        // 1. strata_take_screenshot must report real (320x240), NOT (0x0), and encode PNG off-thread
        let shot = server.call_tool("strata_take_screenshot", json!({}));
        assert_eq!(shot.is_error, None);
        let has_dims = shot.content.iter().any(|c| match c {
            McpContent::Text { text } => text.contains("(320x240)"),
            _ => false,
        });
        assert!(
            has_dims,
            "strata_take_screenshot text payload must report exact (320x240), got {:?}",
            shot.content
        );
        let has_png_img = shot.content.iter().any(|c| match c {
            McpContent::Image { data, mime_type } => {
                mime_type == "image/png" && data.starts_with("iVBOR")
            }
            _ => false,
        });
        assert!(has_png_img, "screenshot must contain valid PNG base64 image");

        // 2. strata_key_press must succeed (no longer Unknown tool)
        let kp = server.call_tool(
            "strata_key_press",
            json!({ "key": "Escape", "modifiers": { "ctrl": false } }),
        );
        assert_eq!(kp.is_error, None);

        // 3. strata_mouse_wheel must succeed
        let mw = server.call_tool(
            "strata_mouse_wheel",
            json!({ "delta_y": -120.0, "x": 160.0, "y": 120.0 }),
        );
        assert_eq!(mw.is_error, None);

        // 4. strata_get_perf_profile must return valid rolling summary JSON
        let perf = server.call_tool("strata_get_perf_profile", json!({}));
        assert_eq!(perf.is_error, None);
        if let McpContent::Text { text } = &perf.content[0] {
            let v: serde_json::Value = serde_json::from_str(text).expect("valid perf JSON");
            assert!(v["total_frames"].as_u64().unwrap() >= 1);
            assert!(v["last_frame"]["dom_nodes"].as_u64().unwrap() > 0);
        } else {
            panic!("expected text perf payload");
        }

        stop_ui.store(true, Ordering::Release);
    });

    // Pump UI thread until client finishes
    let mut rt = AppRuntime::new(
        rsx! {
            <div class="w-[320px] h-[240px] bg-slate-900 text-white p-4">
                <button class="px-3 py-1 bg-blue-600 hover:bg-blue-500">"OK"</button>
            </div>
        },
        320.0,
        240.0,
        1.0,
    );
    rt.tick_frame();
    rt.finalize_frame_telemetry("Dirty");

    let deadline = Instant::now() + Duration::from_secs(10);
    while !stop.load(Ordering::Acquire) && Instant::now() < deadline {
        let _ = strata::mcp_bridge::drain_and_execute_mcp_requests_with_outcome(&mut rt);
        std::thread::sleep(Duration::from_millis(5));
    }
    client_thread.join().expect("client thread must succeed");
    let _ = std::fs::remove_file(&sock_path);
}

#[cfg(unix)]
#[test]
fn execute_actions_wait_is_nonblocking_and_routes_hotkeys_to_event_hook() {
    let _bridge_guard = MCP_BRIDGE_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use std::time::{Duration, Instant};
    use strata::{rsx, AppRuntime};
    use strata_platform::{KeyCode, PlatformEvent};

    let sock_path = std::env::temp_dir().join(format!(
        "strata-mcp-nonblock-wait-{}-{}.sock",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&sock_path);

    strata::mcp_bridge::start_mcp_dev_listener(Some(sock_path.clone()))
        .expect("start_mcp_dev_listener");

    let stop = Arc::new(AtomicBool::new(false));
    let stop_ui = Arc::clone(&stop);
    let sock_client = sock_path.clone();

    let client_thread = std::thread::spawn(move || {
        let server = McpServer::new(strata_mcp::client::StrataDevClient::new(sock_client));

        let t0 = Instant::now();
        let res = server.call_tool(
            "strata_execute_actions",
            json!({
                "actions": [
                    { "action": "key_press", "key": "F3" },
                    { "action": "wait", "ms": 220 },
                    { "action": "key_press", "key": "Escape" }
                ]
            }),
        );
        let elapsed = t0.elapsed();
        assert_eq!(res.is_error, None, "execute_actions must succeed: {res:?}");
        assert!(
            elapsed >= Duration::from_millis(190),
            "wait step must honor requested delay on caller side (elapsed={elapsed:?})"
        );
        if let McpContent::Text { text } = &res.content[0] {
            assert!(
                text.contains("\"executed_actions\": 3"),
                "expected 3 executed actions, got: {text}"
            );
        } else {
            panic!("expected text result");
        }
        stop_ui.store(true, Ordering::Release);
    });

    let mut rt = AppRuntime::new(
        rsx! {
            <div class="w-[320px] h-[240px] bg-slate-900 text-white p-4">"Wait Test"</div>
        },
        320.0,
        240.0,
        1.0,
    );
    rt.tick_frame();

    let mut hook_keys: Vec<KeyCode> = Vec::new();
    let mut max_drain_us: u128 = 0;
    let mut ui_ticks_during_wait: u32 = 0;

    let deadline = Instant::now() + Duration::from_secs(10);
    while !stop.load(Ordering::Acquire) && Instant::now() < deadline {
        let d0 = Instant::now();
        let _ = strata::mcp_bridge::drain_and_execute_mcp_requests_with_hook(
            &mut rt,
            |ev, _rt| {
                if let PlatformEvent::KeyDown { key, .. } = ev {
                    hook_keys.push(*key);
                    return true;
                }
                false
            },
        );
        max_drain_us = max_drain_us.max(d0.elapsed().as_micros());
        ui_ticks_during_wait += 1;
        std::thread::sleep(Duration::from_millis(5));
    }
    client_thread.join().expect("client thread must succeed");
    let _ = std::fs::remove_file(&sock_path);

    assert!(
        max_drain_us < 80_000,
        "UI main thread drain must never block on wait sleep (max_drain_us={max_drain_us})"
    );
    assert!(
        ui_ticks_during_wait >= 10,
        "UI main thread must stay responsive and keep ticking during 220ms wait (ticks={ui_ticks_during_wait})"
    );
    assert!(
        hook_keys.contains(&KeyCode::F3) && hook_keys.contains(&KeyCode::Escape),
        "both F3 and Escape must be delivered to host event hook, got: {hook_keys:?}"
    );
}

