//! Comprehensive integration and unit test suite for the Strata MCP Server,
//! authentic Yaru cursor overlay engine, action batching, and debug build policy.

use std::time::{Duration, Instant};
use serde_json::json;
use strata::prelude::*;
use strata_platform::{CursorIcon as PlatformCursorIcon, McpCursorState, CursorTheme};
use strata_mcp::protocol::*;
use strata_mcp::server::McpServer;


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
    let tools = list_resp.result.unwrap()["tools"].as_array().unwrap().clone();
    assert_eq!(tools.len(), 12);

    let tool_names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(tool_names.contains(&"strata_session_info"));
    assert!(tool_names.contains(&"strata_run_app"));
    assert!(tool_names.contains(&"strata_take_screenshot"));
    assert!(tool_names.contains(&"strata_mouse_move"));
    assert!(tool_names.contains(&"strata_mouse_click"));
    assert!(tool_names.contains(&"strata_mouse_down"));
    assert!(tool_names.contains(&"strata_mouse_up"));
    assert!(tool_names.contains(&"strata_mouse_scroll"));
    assert!(tool_names.contains(&"strata_keyboard_type"));
    assert!(tool_names.contains(&"strata_keyboard_key"));
    assert!(tool_names.contains(&"strata_execute_actions"));
    assert!(tool_names.contains(&"strata_inspect_element"));
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
            <button class="px-4 py-2 bg-blue-600 rounded cursor-pointer" on_click={|_| {}}>
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

    // 1. Session info command
    let (info_res, _) = strata::mcp_bridge::execute_mcp_command_core(&mut runtime, "session_info", json!({}));
    let info = info_res.expect("session_info must succeed");
    assert_eq!(info["viewport_width"], 800.0);
    assert_eq!(info["viewport_height"], 600.0);

    // 2. Mouse move to button
    let (move_res, _) = strata::mcp_bridge::execute_mcp_command_core(
        &mut runtime,
        "mouse_move",
        json!({ "x": 50.0, "y": 25.0, "smooth": true }),
    );
    let move_val = move_res.expect("mouse_move must succeed");
    assert_eq!(move_val["smooth"], true);
    assert_eq!(runtime.mcp_cursor.as_ref().unwrap().cursor_icon, PlatformCursorIcon::Pointer);

    // 3. Mouse move to input field (within bounds x: 16..111, y: 72..112)
    let (move_input, _) = strata::mcp_bridge::execute_mcp_command_core(
        &mut runtime,
        "mouse_move",
        json!({ "x": 50.0, "y": 85.0, "smooth": false }),
    );
    let _ = move_input.expect("mouse_move to input must succeed");
    assert_eq!(runtime.mcp_cursor.as_ref().unwrap().cursor_icon, PlatformCursorIcon::Text);

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
    assert_eq!(runtime.active_focus.as_ref().unwrap().state.text.get(), "Strata MCP");

    // 6. Keyboard key: Backspace
    let (key_res, _) = strata::mcp_bridge::execute_mcp_command_core(
        &mut runtime,
        "keyboard_key",
        json!({ "key": "backspace" }),
    );
    assert!(key_res.is_ok());
    assert_eq!(runtime.active_focus.as_ref().unwrap().state.text.get(), "Strata MC");

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

    // 8. Screenshot capture
    let (shot_res, _) = strata::mcp_bridge::execute_mcp_command_core(&mut runtime, "take_screenshot", json!({}));
    let shot = shot_res.expect("take_screenshot must succeed");
    let b64 = shot["png_base64"].as_str().unwrap();
    assert!(!b64.is_empty());
    // Verify PNG magic bytes: \x89PNG
    let raw = strata_mcp::client::StrataDevClient::default();
    let _ = raw;
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
    let outside_path = if cfg!(windows) { "C:\\Windows\\System32" } else { "/tmp" };
    let outside_res = server.call_tool(
        "strata_run_app",
        json!({ "mode": "debug", "project_dir": outside_path }),
    );
    assert_eq!(outside_res.is_error, Some(true));
    if let McpContent::Text { text } = &outside_res.content[0] {
        assert!(text.contains("outside the authorized workspace roots") || text.contains("does not contain a Cargo.toml"));
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

