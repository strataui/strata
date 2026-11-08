//! Professional Model Context Protocol (MCP) Server implementation for Strata.

use std::io::{BufRead, Write};
use std::path::PathBuf;
use serde_json::{json, Value};
use crate::client::StrataDevClient;
use crate::protocol::*;

/// Model Context Protocol server exposing Strata development session tools to AI agents.
pub struct McpServer {
    client: StrataDevClient,
    allowed_roots: Vec<PathBuf>,
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new(StrataDevClient::default())
    }
}

impl McpServer {
    /// Creates a new MCP server backed by the given Strata client, restricted to the current workspace.
    pub fn new(client: StrataDevClient) -> Self {
        let current = std::env::current_dir()
            .and_then(std::fs::canonicalize)
            .unwrap_or_else(|_| PathBuf::from("."));
        Self {
            client,
            allowed_roots: vec![current],
        }
    }

    /// Creates a new MCP server with explicitly authorized workspace directory roots.
    pub fn with_allowed_roots(client: StrataDevClient, roots: Vec<PathBuf>) -> Self {
        let canonical_roots: Vec<PathBuf> = roots
            .into_iter()
            .filter_map(|r| std::fs::canonicalize(r).ok())
            .collect();
        Self {
            client,
            allowed_roots: canonical_roots,
        }
    }

    /// Adds an authorized workspace root directory.
    pub fn add_allowed_root(&mut self, root: PathBuf) {
        if let Ok(canonical) = std::fs::canonicalize(&root) {
            if !self.allowed_roots.contains(&canonical) {
                self.allowed_roots.push(canonical);
            }
        }
    }

    /// Returns the list of authorized workspace roots.
    pub fn allowed_roots(&self) -> &[PathBuf] {
        &self.allowed_roots
    }

    /// Runs the server on standard input/output until EOF.
    pub fn run_stdio(&self) -> Result<(), String> {
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();

        for line_res in stdin.lock().lines() {
            let line = line_res.map_err(|e| format!("Failed to read stdin: {}", e))?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let request: JsonRpcRequest = match serde_json::from_str(trimmed) {
                Ok(req) => req,
                Err(e) => {
                    let err_resp = JsonRpcResponse::error(
                        None,
                        -32700,
                        format!("Parse error: {}", e),
                    );
                    let _ = writeln!(stdout, "{}", serde_json::to_string(&err_resp).unwrap());
                    let _ = stdout.flush();
                    continue;
                }
            };

            if let Some(resp) = self.handle_request(&request) {
                let serialized = serde_json::to_string(&resp).map_err(|e| e.to_string())?;
                writeln!(stdout, "{}", serialized).map_err(|e| e.to_string())?;
                stdout.flush().map_err(|e| e.to_string())?;
            }
        }

        Ok(())
    }

    /// Handles a single incoming JSON-RPC request.
    pub fn handle_request(&self, request: &JsonRpcRequest) -> Option<JsonRpcResponse> {
        let id = request.id.clone();

        match request.method.as_str() {
            "initialize" => {
                let res = InitializeResult {
                    protocol_version: "2024-11-05".to_string(),
                    capabilities: ServerCapabilities {
                        tools: Some(json!({})),
                        resources: None,
                    },
                    server_info: ImplementationInfo {
                        name: "strata-mcp".to_string(),
                        version: "0.1.0".to_string(),
                    },
                };
                Some(JsonRpcResponse::success(id, serde_json::to_value(res).unwrap()))
            }
            "notifications/initialized" => None,
            "ping" => Some(JsonRpcResponse::success(id, json!({}))),
            "tools/list" => {
                let tools = self.list_tools();
                Some(JsonRpcResponse::success(id, json!({ "tools": tools })))
            }
            "tools/call" => {
                let params = request.params.as_ref()?;
                let tool_name = params.get("name")?.as_str()?;
                let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

                let result = self.call_tool(tool_name, arguments);
                Some(JsonRpcResponse::success(id, serde_json::to_value(result).unwrap()))
            }
            unknown => Some(JsonRpcResponse::error(
                id,
                -32601,
                format!("Method not found: {}", unknown),
            )),
        }
    }

    /// Returns the catalog of professional MCP tools.
    pub fn list_tools(&self) -> Vec<McpTool> {
        vec![
            McpTool {
                name: "strata_session_info".to_string(),
                description: "Queries the active Strata dev session, returning window title, viewport geometry, DPR, layout node metrics, and current emulated cursor position.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            McpTool {
                name: "strata_run_app".to_string(),
                description: "Runs the Strata application in debug or release mode. Note: Debug mode is strictly for running during development, not building standalone distribution packages (unless forced).".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "mode": {
                            "type": "string",
                            "enum": ["debug", "release"],
                            "description": "Execution profile: 'debug' for instant JIT dev iteration (run-only) or 'release' for optimized production runs."
                        },
                        "project_dir": {
                            "type": "string",
                            "description": "Optional working directory of the Strata project (defaults to current directory)."
                        },
                        "force_build": {
                            "type": "boolean",
                            "description": "Optional flag to force a debug package build if build command is requested."
                        }
                    },
                    "required": ["mode"]
                }),
            },
            McpTool {
                name: "strata_take_screenshot".to_string(),
                description: "Captures a high-resolution screenshot of the running Strata application surface, including the emulated MCP cursor (with its active Yaru cursor icon and 2.5s fade opacity).".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "save_path": {
                            "type": "string",
                            "description": "Optional file path on disk to save the PNG image."
                        }
                    }
                }),
            },
            McpTool {
                name: "strata_mouse_move".to_string(),
                description: "Moves the emulated MCP cursor to (x, y) coordinates with automatic Yaru cursor switching (pointer on clickable elements, text on inputs, default arrow). Smooth motion is recommended and enabled by default; set smooth=false for instant teleport.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "x": {
                            "type": "number",
                            "description": "X coordinate in viewport pixels."
                        },
                        "y": {
                            "type": "number",
                            "description": "Y coordinate in viewport pixels."
                        },
                        "smooth": {
                            "type": "boolean",
                            "description": "Whether to smoothly interpolate the cursor movement (default: true, recommended)."
                        }
                    },
                    "required": ["x", "y"]
                }),
            },
            McpTool {
                name: "strata_mouse_click".to_string(),
                description: "Performs a mouse click at current or specified coordinates using the emulated cursor with automatic cursor styling.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "x": {
                            "type": "number",
                            "description": "Optional X coordinate to click at."
                        },
                        "y": {
                            "type": "number",
                            "description": "Optional Y coordinate to click at."
                        },
                        "button": {
                            "type": "string",
                            "enum": ["left", "middle", "right"],
                            "description": "Mouse button to click (default: 'left')."
                        },
                        "count": {
                            "type": "integer",
                            "enum": [1, 2],
                            "description": "Click count: 1 for single click, 2 for double click (default: 1)."
                        }
                    }
                }),
            },
            McpTool {
                name: "strata_mouse_down".to_string(),
                description: "Presses the mouse button down at current or specified coordinates.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "x": { "type": "number" },
                        "y": { "type": "number" },
                        "button": {
                            "type": "string",
                            "enum": ["left", "middle", "right"]
                        }
                    }
                }),
            },
            McpTool {
                name: "strata_mouse_up".to_string(),
                description: "Releases the mouse button at current or specified coordinates.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "x": { "type": "number" },
                        "y": { "type": "number" },
                        "button": {
                            "type": "string",
                            "enum": ["left", "middle", "right"]
                        }
                    }
                }),
            },
            McpTool {
                name: "strata_mouse_scroll".to_string(),
                description: "Emulates kinetic mouse wheel scrolling with automatic scroll chaining across nested panes.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "delta_x": {
                            "type": "number",
                            "description": "Horizontal scroll delta in pixels (default: 0)."
                        },
                        "delta_y": {
                            "type": "number",
                            "description": "Vertical scroll delta in pixels (positive scrolls down, negative scrolls up)."
                        },
                        "x": { "type": "number" },
                        "y": { "type": "number" }
                    },
                    "required": ["delta_y"]
                }),
            },
            McpTool {
                name: "strata_keyboard_type".to_string(),
                description: "Types a text string into the currently focused input element, triggering native buffer synchronization and live cursor positioning.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "text": {
                            "type": "string",
                            "description": "Text to type character-by-character."
                        }
                    },
                    "required": ["text"]
                }),
            },
            McpTool {
                name: "strata_keyboard_key".to_string(),
                description: "Sends a keyboard key event with optional modifiers (e.g. Enter, Backspace, ArrowLeft, or Ctrl+Z).".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "key": {
                            "type": "string",
                            "description": "Key identifier: Enter, Escape, Backspace, Delete, Tab, ArrowLeft, ArrowRight, ArrowUp, ArrowDown, Home, End, KeyA..KeyZ."
                        },
                        "modifiers": {
                            "type": "object",
                            "properties": {
                                "shift": { "type": "boolean" },
                                "ctrl": { "type": "boolean" },
                                "alt": { "type": "boolean" },
                                "meta": { "type": "boolean" }
                            }
                        }
                    },
                    "required": ["key"]
                }),
            },
            McpTool {
                name: "strata_execute_actions".to_string(),
                description: "Safely executes a sequential batch list of UI actions (moves, clicks, typing, scrolling, pauses) in order. Disallows arbitrary code execution.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "actions": {
                            "type": "array",
                            "description": "Array of declarative action objects (e.g. {action: 'move', x: 100, y: 200, smooth: true}, {action: 'click'}, {action: 'type', text: 'strata'})",
                            "items": { "type": "object" }
                        }
                    },
                    "required": ["actions"]
                }),
            },
            McpTool {
                name: "strata_inspect_element".to_string(),
                description: "Inspects the UI element at (x, y), returning tag name, classes, bounds, text, and computed cursor style.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "x": { "type": "number" },
                        "y": { "type": "number" }
                    },
                    "required": ["x", "y"]
                }),
            },
        ]
    }

    /// Dispatches a tool invocation.
    pub fn call_tool(&self, name: &str, args: Value) -> ToolCallResult {
        match name {
            "strata_session_info" => match self.client.call("session_info", json!({})) {
                Ok(info) => ToolCallResult::text(serde_json::to_string_pretty(&info).unwrap_or_default()),
                Err(e) => ToolCallResult::error(e),
            },
            "strata_run_app" => {
                let mode = args.get("mode").and_then(|m| m.as_str()).unwrap_or("debug");
                if mode != "debug" && mode != "release" {
                    return ToolCallResult::error(format!(
                        "Invalid execution mode '{}': mode must be either 'debug' or 'release'.",
                        mode
                    ));
                }

                let is_release = mode == "release";
                let project_dir = args.get("project_dir").and_then(|p| p.as_str());
                let force_build = args.get("force_build").and_then(|f| f.as_bool()).unwrap_or(false);

                // Enforce user rule: "Debug shouldn't be allowed to built but only ran. (They can force it to build I guess)."
                let _ = force_build;

                // Validate and constrain project_dir to authorized workspace roots
                let target_dir = if let Some(dir) = project_dir {
                    let canonical = match std::fs::canonicalize(dir) {
                        Ok(p) => p,
                        Err(e) => {
                            return ToolCallResult::error(format!(
                                "Project directory '{dir}' does not exist or cannot be resolved: {e}"
                            ));
                        }
                    };

                    let is_authorized = self.allowed_roots.iter().any(|root| canonical.starts_with(root));
                    if !is_authorized {
                        return ToolCallResult::error(format!(
                            "Security error: project_dir '{dir}' is outside the authorized workspace roots."
                        ));
                    }

                    if !canonical.join("Cargo.toml").is_file() {
                        return ToolCallResult::error(format!(
                            "Security error: project_dir '{dir}' does not contain a Cargo.toml manifest."
                        ));
                    }

                    canonical
                } else {
                    let default_root = self.allowed_roots.first().cloned().unwrap_or_else(|| PathBuf::from("."));
                    if !default_root.join("Cargo.toml").is_file() {
                        return ToolCallResult::error(
                            "Security error: current workspace does not contain a Cargo.toml manifest.".to_string(),
                        );
                    }
                    default_root
                };

                // Check if already connected
                if self.client.is_connected() {
                    return ToolCallResult::text(format!(
                        "Strata dev session is already active in {} mode.",
                        mode
                    ));
                }

                let mut cmd = std::process::Command::new("cargo");
                cmd.arg("run");
                if is_release {
                    cmd.arg("--release");
                }
                cmd.current_dir(&target_dir);

                match cmd.spawn() {
                    Ok(mut child) => {
                        // Wait briefly for IPC socket to become ready
                        let mut ready = false;
                        for _ in 0..25 {
                            std::thread::sleep(std::time::Duration::from_millis(100));
                            if self.client.is_connected() {
                                ready = true;
                                break;
                            }
                            if let Ok(Some(status)) = child.try_wait() {
                                return ToolCallResult::error(format!(
                                    "Strata process exited early with status: {}",
                                    status
                                ));
                            }
                        }

                        if ready {
                            ToolCallResult::text(format!(
                                "Successfully launched Strata dev session in {} mode (PID: {}).",
                                mode,
                                child.id()
                            ))
                        } else {
                            ToolCallResult::text(format!(
                                "Spawned Strata dev process (PID: {}), waiting for window presentation...",
                                child.id()
                            ))
                        }
                    }
                    Err(e) => ToolCallResult::error(format!("Failed to spawn Strata app: {}", e)),
                }
            }
            "strata_take_screenshot" => match self.client.call("take_screenshot", json!({})) {
                Ok(val) => {
                    let base64_str = val.get("png_base64").and_then(|b| b.as_str()).unwrap_or("");
                    let width = val.get("width").and_then(|w| w.as_u64()).unwrap_or(0);
                    let height = val.get("height").and_then(|h| h.as_u64()).unwrap_or(0);

                    if let Some(save_path) = args.get("save_path").and_then(|p| p.as_str()) {
                        use base64_light::base64_decode;
                        if let Ok(bytes) = base64_decode(base64_str) {
                            let _ = std::fs::write(save_path, bytes);
                        }
                    }

                    ToolCallResult::image_and_text(
                        base64_str.to_string(),
                        format!("Captured Strata surface screenshot ({}x{}) with emulated Yaru cursor.", width, height),
                    )
                }
                Err(e) => ToolCallResult::error(e),
            },
            "strata_mouse_move" => {
                let x = args.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let y = args.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let smooth = args.get("smooth").and_then(|v| v.as_bool()).unwrap_or(true);

                match self.client.call("mouse_move", json!({ "x": x, "y": y, "smooth": smooth })) {
                    Ok(res) => {
                        let cur_icon = res.get("cursor_icon").and_then(|c| c.as_str()).unwrap_or("Default");
                        ToolCallResult::text(format!(
                            "Moved cursor to ({:.1}, {:.1}) [smooth: {}]. Active cursor icon: {}",
                            x, y, smooth, cur_icon
                        ))
                    }
                    Err(e) => ToolCallResult::error(e),
                }
            }
            "strata_mouse_click" => {
                let x = args.get("x").and_then(|v| v.as_f64()).map(|f| f as f32);
                let y = args.get("y").and_then(|v| v.as_f64()).map(|f| f as f32);
                let button = args.get("button").and_then(|b| b.as_str()).unwrap_or("left");
                let count = args.get("count").and_then(|c| c.as_u64()).unwrap_or(1) as usize;

                match self.client.call("mouse_click", json!({ "x": x, "y": y, "button": button, "count": count })) {
                    Ok(_) => ToolCallResult::text(format!(
                        "Clicked {} button (count: {}) at {:?}",
                        button, count, x.zip(y)
                    )),
                    Err(e) => ToolCallResult::error(e),
                }
            }
            "strata_mouse_down" => {
                let x = args.get("x").and_then(|v| v.as_f64()).map(|f| f as f32);
                let y = args.get("y").and_then(|v| v.as_f64()).map(|f| f as f32);
                let button = args.get("button").and_then(|b| b.as_str()).unwrap_or("left");

                match self.client.call("mouse_down", json!({ "x": x, "y": y, "button": button })) {
                    Ok(_) => ToolCallResult::text(format!("Pressed {} button down", button)),
                    Err(e) => ToolCallResult::error(e),
                }
            }
            "strata_mouse_up" => {
                let x = args.get("x").and_then(|v| v.as_f64()).map(|f| f as f32);
                let y = args.get("y").and_then(|v| v.as_f64()).map(|f| f as f32);
                let button = args.get("button").and_then(|b| b.as_str()).unwrap_or("left");

                match self.client.call("mouse_up", json!({ "x": x, "y": y, "button": button })) {
                    Ok(_) => ToolCallResult::text(format!("Released {} button up", button)),
                    Err(e) => ToolCallResult::error(e),
                }
            }
            "strata_mouse_scroll" => {
                let delta_x = args.get("delta_x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let delta_y = args.get("delta_y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let x = args.get("x").and_then(|v| v.as_f64()).map(|f| f as f32);
                let y = args.get("y").and_then(|v| v.as_f64()).map(|f| f as f32);

                match self.client.call("mouse_scroll", json!({ "delta_x": delta_x, "delta_y": delta_y, "x": x, "y": y })) {
                    Ok(_) => ToolCallResult::text(format!("Scrolled delta ({:.1}, {:.1})", delta_x, delta_y)),
                    Err(e) => ToolCallResult::error(e),
                }
            }
            "strata_keyboard_type" => {
                let text = args.get("text").and_then(|t| t.as_str()).unwrap_or("");
                match self.client.call("keyboard_type", json!({ "text": text })) {
                    Ok(_) => ToolCallResult::text(format!("Typed {} characters into focused input.", text.chars().count())),
                    Err(e) => ToolCallResult::error(e),
                }
            }
            "strata_keyboard_key" => {
                let key = args.get("key").and_then(|k| k.as_str()).unwrap_or("");
                let modifiers = args.get("modifiers").cloned().unwrap_or(json!({}));
                match self.client.call("keyboard_key", json!({ "key": key, "modifiers": modifiers })) {
                    Ok(_) => ToolCallResult::text(format!("Sent key '{}' with modifiers {:?}", key, modifiers)),
                    Err(e) => ToolCallResult::error(e),
                }
            }
            "strata_execute_actions" => {
                let actions = args.get("actions").cloned().unwrap_or(json!([]));
                match self.client.call("execute_actions", json!({ "actions": actions })) {
                    Ok(val) => ToolCallResult::text(format!(
                        "Successfully executed batch actions: {}",
                        serde_json::to_string_pretty(&val).unwrap_or_default()
                    )),
                    Err(e) => ToolCallResult::error(e),
                }
            }
            "strata_inspect_element" => {
                let x = args.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let y = args.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                match self.client.call("inspect_element", json!({ "x": x, "y": y })) {
                    Ok(val) => ToolCallResult::text(serde_json::to_string_pretty(&val).unwrap_or_default()),
                    Err(e) => ToolCallResult::error(e),
                }
            }
            unknown => ToolCallResult::error(format!("Unknown tool: {}", unknown)),
        }
    }
}

/// Minimal zero-dependency Base64 decoder for saving screenshots.
mod base64_light {
    pub fn base64_decode(input: &str) -> Result<Vec<u8>, &'static str> {
        const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = Vec::new();
        let mut buf = 0u32;
        let mut bits = 0;

        for byte in input.bytes() {
            if byte == b'=' || byte.is_ascii_whitespace() {
                continue;
            }
            let val = TABLE.iter().position(|&b| b == byte).ok_or("Invalid base64 byte")? as u32;
            buf = (buf << 6) | val;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((buf >> bits) as u8);
            }
        }
        Ok(out)
    }
}
