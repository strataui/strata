//! Model Context Protocol (MCP) Dev Bridge and In-Process IPC Dispatcher.
//!
//! Provides the bidirectional communication pipeline connecting external AI agents
//! to active Strata runtime windows, emulating mouse movements with authentic Yaru
//! cursor overlays, taking high-resolution screenshots, and safely batching UI actions.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;
use std::time::Duration;
use serde_json::{json, Value};

use crate::runtime::AppRuntime;
use strata_platform::{KeyCode, Modifiers, MouseButton};

type PendingItem = (String, Sender<String>);

fn get_pending_queue() -> &'static Mutex<Vec<PendingItem>> {
    static QUEUE: std::sync::OnceLock<Mutex<Vec<PendingItem>>> = std::sync::OnceLock::new();
    QUEUE.get_or_init(|| Mutex::new(Vec::new()))
}

#[cfg(unix)]
fn get_current_uid() -> u32 {
    extern "C" {
        fn getuid() -> u32;
    }
    unsafe { getuid() }
}

#[cfg(target_os = "linux")]
fn verify_peer_credentials(stream: &UnixStream) -> Result<(), std::io::Error> {
    use std::os::unix::io::AsRawFd;

    #[repr(C)]
    struct UCred {
        pid: i32,
        uid: u32,
        gid: u32,
    }

    extern "C" {
        fn getsockopt(
            sockfd: std::os::raw::c_int,
            level: std::os::raw::c_int,
            optname: std::os::raw::c_int,
            optval: *mut std::os::raw::c_void,
            optlen: *mut u32,
        ) -> std::os::raw::c_int;
    }

    let fd = stream.as_raw_fd();
    let mut ucred = UCred {
        pid: 0,
        uid: u32::MAX,
        gid: u32::MAX,
    };
    let mut len = std::mem::size_of::<UCred>() as u32;

    const SOL_SOCKET: std::os::raw::c_int = 1;
    const SO_PEERCRED: std::os::raw::c_int = 17;

    let res = unsafe {
        getsockopt(
            fd,
            SOL_SOCKET,
            SO_PEERCRED,
            &mut ucred as *mut _ as *mut std::os::raw::c_void,
            &mut len as *mut u32,
        )
    };

    if res != 0 {
        return Err(std::io::Error::last_os_error());
    }

    let my_uid = get_current_uid();
    if ucred.uid != my_uid {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("Peer UID {} does not match process UID {}", ucred.uid, my_uid),
        ));
    }

    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn verify_peer_credentials(stream: &UnixStream) -> Result<(), std::io::Error> {
    use std::os::unix::io::AsRawFd;

    extern "C" {
        fn getpeereid(s: std::os::raw::c_int, euid: *mut u32, egid: *mut u32) -> std::os::raw::c_int;
    }

    let fd = stream.as_raw_fd();
    let mut euid = u32::MAX;
    let mut egid = u32::MAX;
    let res = unsafe { getpeereid(fd, &mut euid, &mut egid) };
    if res != 0 {
        return Err(std::io::Error::last_os_error());
    }

    let my_uid = get_current_uid();
    if euid != my_uid {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("Peer UID {} does not match process UID {}", euid, my_uid),
        ));
    }

    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "ios")))]
fn verify_peer_credentials(_stream: &UnixStream) -> Result<(), std::io::Error> {
    Ok(())
}

/// Spawns a background thread listening for MCP connections on a Unix domain socket.
pub fn start_mcp_dev_listener(socket_path: Option<PathBuf>) -> Result<(), std::io::Error> {
    let path = socket_path.unwrap_or_else(default_dev_socket_path);

    // Clean up stale socket if present
    if path.exists() {
        let _ = std::fs::remove_file(&path);
    }

    if let Some(parent) = path.parent() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = std::fs::DirBuilder::new();
            builder.recursive(true);
            builder.mode(0o700);
            let _ = builder.create(parent);
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
        }
        #[cfg(not(unix))]
        {
            let _ = std::fs::create_dir_all(parent);
        }
    }

    let listener = UnixListener::bind(&path)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }

    std::thread::Builder::new()
        .name("strata-mcp-ipc-listener".to_string())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                if verify_peer_credentials(&stream).is_ok() {
                    handle_client_connection(stream);
                }
            }
        })?;

    Ok(())
}

fn handle_client_connection(mut stream: UnixStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));

    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    });

    let mut line = String::new();
    while let Ok(n) = reader.read_line(&mut line) {
        if n == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            line.clear();
            continue;
        }

        let (tx, rx) = channel();
        if let Ok(mut q) = get_pending_queue().lock() {
            q.push((trimmed.to_string(), tx));
        }

        // Signal GTK main event loop to wake up and process the queue
        #[cfg(target_os = "linux")]
        strata_platform::post_platform_event(strata_platform::PlatformEvent::RedrawRequested);

        let response = match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(res) => res,
            Err(_) => json!({
                "jsonrpc": "2.0",
                "error": { "code": -32000, "message": "Request timed out waiting for UI thread" }
            })
            .to_string(),
        };

        let mut out_bytes = response.into_bytes();
        out_bytes.push(b'\n');
        if stream.write_all(&out_bytes).is_err() {
            break;
        }
        let _ = stream.flush();

        line.clear();
    }
}

/// Drains and processes all pending MCP requests on the active main UI thread.
///
/// Returns true if any executed command requires a view rebuild or display list redraw.
pub fn drain_and_execute_mcp_requests(runtime: &mut AppRuntime) -> bool {
    let mut items = Vec::new();
    {
        if let Ok(mut lock) = get_pending_queue().lock() {
            if lock.is_empty() {
                return false;
            }
            items = std::mem::take(&mut *lock);
        }
    }

    let mut any_dirty = false;
    for (req_str, tx) in items {
        let (resp_str, dirty) = process_single_mcp_request(runtime, &req_str);
        if dirty {
            any_dirty = true;
        }
        let _ = tx.send(resp_str);
    }

    any_dirty
}

fn process_single_mcp_request(runtime: &mut AppRuntime, req_str: &str) -> (String, bool) {
    let parsed: Value = match serde_json::from_str(req_str) {
        Ok(v) => v,
        Err(e) => {
            let err = json!({
                "jsonrpc": "2.0",
                "error": { "code": -32700, "message": format!("Parse error: {}", e) }
            });
            return (err.to_string(), false);
        }
    };

    let id = parsed.get("id").cloned();
    let method = parsed.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let params = parsed.get("params").cloned().unwrap_or(json!({}));

    let (result, dirty) = execute_mcp_command_core(runtime, method, params);

    let resp = match result {
        Ok(val) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": val,
        }),
        Err(err_msg) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32603, "message": err_msg }
        }),
    };

    (resp.to_string(), dirty)
}

/// Core execution routine for MCP commands directly modifying `AppRuntime`.
pub fn execute_mcp_command_core(
    runtime: &mut AppRuntime,
    method: &str,
    params: Value,
) -> (Result<Value, String>, bool) {
    match method {
        "session_info" => {
            let cursor_info = runtime.mcp_cursor.as_ref().map(|c| {
                json!({
                    "x": c.current_x,
                    "y": c.current_y,
                    "target_x": c.target_x,
                    "target_y": c.target_y,
                    "is_moving": c.is_moving,
                    "fade_opacity": c.fade_opacity,
                    "cursor_icon": format!("{:?}", c.cursor_icon),
                    "is_down": c.is_down,
                })
            });

            let focus_info = runtime.active_focus.as_ref().map(|a| {
                json!({
                    "input_index": a.input_index,
                    "input_id": a.input_id,
                    "text": a.state.text.get(),
                    "cursor": a.state.cursor_index.get(),
                })
            });

            let info = json!({
                "viewport_width": runtime.viewport_width,
                "viewport_height": runtime.viewport_height,
                "dpr": runtime.dpr,
                "frame_count": runtime.frame_count,
                "cursor": cursor_info,
                "active_focus": focus_info,
                "cursor_theme": runtime.cursor_theme.name,
            });

            (Ok(info), false)
        }
        "mouse_move" => {
            let x = params.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            let y = params.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            let smooth = params.get("smooth").and_then(|v| v.as_bool()).unwrap_or(true);

            let dirty = runtime.mcp_move_pointer(x, y, smooth);
            let cur_icon = runtime
                .mcp_cursor
                .as_ref()
                .map(|c| format!("{:?}", c.cursor_icon))
                .unwrap_or_else(|| "Default".to_string());

            (
                Ok(json!({
                    "x": x,
                    "y": y,
                    "smooth": smooth,
                    "cursor_icon": cur_icon
                })),
                dirty || true,
            )
        }
        "mouse_click" => {
            let x = params.get("x").and_then(|v| v.as_f64()).map(|f| f as f32);
            let y = params.get("y").and_then(|v| v.as_f64()).map(|f| f as f32);
            let btn_str = params.get("button").and_then(|b| b.as_str()).unwrap_or("left");
            let count = params.get("count").and_then(|c| c.as_u64()).unwrap_or(1) as usize;
            let button = parse_mouse_button(btn_str);

            let dirty = runtime.mcp_mouse_click(button, x, y, count);
            (Ok(json!({ "clicked": true })), dirty || true)
        }
        "mouse_down" => {
            let x = params.get("x").and_then(|v| v.as_f64()).map(|f| f as f32);
            let y = params.get("y").and_then(|v| v.as_f64()).map(|f| f as f32);
            let btn_str = params.get("button").and_then(|b| b.as_str()).unwrap_or("left");
            let button = parse_mouse_button(btn_str);

            let dirty = runtime.mcp_mouse_down(button, x, y);
            (Ok(json!({ "down": true })), dirty || true)
        }
        "mouse_up" => {
            let x = params.get("x").and_then(|v| v.as_f64()).map(|f| f as f32);
            let y = params.get("y").and_then(|v| v.as_f64()).map(|f| f as f32);
            let btn_str = params.get("button").and_then(|b| b.as_str()).unwrap_or("left");
            let button = parse_mouse_button(btn_str);

            let dirty = runtime.mcp_mouse_up(button, x, y);
            (Ok(json!({ "up": true })), dirty || true)
        }
        "mouse_scroll" => {
            let delta_x = params.get("delta_x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            let delta_y = params.get("delta_y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            let x = params.get("x").and_then(|v| v.as_f64()).map(|f| f as f32);
            let y = params.get("y").and_then(|v| v.as_f64()).map(|f| f as f32);

            let dirty = runtime.mcp_mouse_scroll(delta_x, delta_y, x, y);
            (Ok(json!({ "scrolled": true })), dirty || true)
        }
        "keyboard_type" => {
            let text = params.get("text").and_then(|t| t.as_str()).unwrap_or("");
            let dirty = runtime.mcp_keyboard_type(text);
            (
                Ok(json!({ "typed_chars": text.chars().count() })),
                dirty || true,
            )
        }
        "keyboard_key" => {
            let key_str = params.get("key").and_then(|k| k.as_str()).unwrap_or("");
            let key = parse_key_code(key_str);
            let modifiers = parse_modifiers(params.get("modifiers"));

            let dirty = runtime.mcp_keyboard_key(key, modifiers);
            (Ok(json!({ "key_sent": key_str })), dirty || true)
        }
        "take_screenshot" => match runtime.mcp_capture_screenshot_png() {
            Ok(bytes) => {
                let base64_str = base64_encode(&bytes);
                (
                    Ok(json!({
                        "png_base64": base64_str,
                        "width": runtime.viewport_width,
                        "height": runtime.viewport_height,
                    })),
                    false,
                )
            }
            Err(e) => (Err(e), false),
        },
        "inspect_element" => {
            let x = params.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            let y = params.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            let (tag, classes, bounds, text, cursor_icon) = runtime.mcp_inspect_element(x, y);

            (
                Ok(json!({
                    "tag": tag,
                    "classes": classes,
                    "bounds": {
                        "x": bounds.x,
                        "y": bounds.y,
                        "width": bounds.width,
                        "height": bounds.height,
                    },
                    "text": text,
                    "cursor_icon": format!("{:?}", cursor_icon),
                })),
                false,
            )
        }
        "execute_actions" => {
            let actions_val = params.get("actions").cloned().unwrap_or(json!([]));
            let actions: Vec<Value> = actions_val.as_array().cloned().unwrap_or_default();
            let mut executed = 0;

            for act in actions {
                let act_type = act.get("action").and_then(|a| a.as_str()).unwrap_or("");
                match act_type {
                    "move" => {
                        let x = act.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                        let y = act.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                        let smooth = act.get("smooth").and_then(|v| v.as_bool()).unwrap_or(true);
                        runtime.mcp_move_pointer(x, y, smooth);
                        executed += 1;
                    }
                    "down" => {
                        let btn = parse_mouse_button(act.get("button").and_then(|b| b.as_str()).unwrap_or("left"));
                        runtime.mcp_mouse_down(btn, None, None);
                        executed += 1;
                    }
                    "up" => {
                        let btn = parse_mouse_button(act.get("button").and_then(|b| b.as_str()).unwrap_or("left"));
                        runtime.mcp_mouse_up(btn, None, None);
                        executed += 1;
                    }
                    "click" => {
                        let x = act.get("x").and_then(|v| v.as_f64()).map(|f| f as f32);
                        let y = act.get("y").and_then(|v| v.as_f64()).map(|f| f as f32);
                        let btn = parse_mouse_button(act.get("button").and_then(|b| b.as_str()).unwrap_or("left"));
                        let count = act.get("count").and_then(|c| c.as_u64()).unwrap_or(1) as usize;
                        runtime.mcp_mouse_click(btn, x, y, count);
                        executed += 1;
                    }
                    "type" => {
                        let text = act.get("text").and_then(|t| t.as_str()).unwrap_or("");
                        runtime.mcp_keyboard_type(text);
                        executed += 1;
                    }
                    "key" => {
                        let key = parse_key_code(act.get("key").and_then(|k| k.as_str()).unwrap_or(""));
                        let mods = parse_modifiers(act.get("modifiers"));
                        runtime.mcp_keyboard_key(key, mods);
                        executed += 1;
                    }
                    "scroll" => {
                        let dx = act.get("delta_x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                        let dy = act.get("delta_y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                        let x = act.get("x").and_then(|v| v.as_f64()).map(|f| f as f32);
                        let y = act.get("y").and_then(|v| v.as_f64()).map(|f| f as f32);
                        runtime.mcp_mouse_scroll(dx, dy, x, y);
                        executed += 1;
                    }
                    "wait" => {
                        let ms = act.get("ms").and_then(|m| m.as_u64()).unwrap_or(50).min(5000);
                        std::thread::sleep(Duration::from_millis(ms));
                        executed += 1;
                    }
                    _ => {}
                }
            }

            (Ok(json!({ "executed_actions": executed })), true)
        }
        unknown => (Err(format!("Unknown MCP method: {}", unknown)), false),
    }
}

fn parse_mouse_button(s: &str) -> MouseButton {
    match s.to_ascii_lowercase().as_str() {
        "middle" | "wheel" => MouseButton::Middle,
        "right" => MouseButton::Right,
        _ => MouseButton::Left,
    }
}

fn parse_key_code(s: &str) -> KeyCode {
    match s.to_ascii_lowercase().as_str() {
        "enter" | "return" => KeyCode::Enter,
        "escape" | "esc" => KeyCode::Escape,
        "backspace" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "tab" => KeyCode::Tab,
        "space" => KeyCode::Space,
        "arrowleft" | "left" => KeyCode::ArrowLeft,
        "arrowright" | "right" => KeyCode::ArrowRight,
        "arrowup" | "up" => KeyCode::ArrowUp,
        "arrowdown" | "down" => KeyCode::ArrowDown,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "a" | "keya" => KeyCode::KeyA,
        "c" | "keyc" => KeyCode::KeyC,
        "v" | "keyv" => KeyCode::KeyV,
        "x" | "keyx" => KeyCode::KeyX,
        "z" | "keyz" => KeyCode::KeyZ,
        "y" | "keyy" => KeyCode::KeyY,
        _ => KeyCode::Space,
    }
}

fn parse_modifiers(val: Option<&Value>) -> Modifiers {
    let mut mods = Modifiers::default();
    if let Some(v) = val {
        mods.shift = v.get("shift").and_then(|b| b.as_bool()).unwrap_or(false);
        mods.ctrl = v.get("ctrl").and_then(|b| b.as_bool()).unwrap_or(false);
        mods.alt = v.get("alt").and_then(|b| b.as_bool()).unwrap_or(false);
        mods.meta = v.get("meta").and_then(|b| b.as_bool()).unwrap_or(false);
    }
    mods
}

pub fn default_dev_socket_path() -> PathBuf {
    if let Ok(path) = std::env::var("STRATA_DEV_SOCKET") {
        PathBuf::from(path)
    } else if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime_dir).join("strata-dev.sock")
    } else {
        #[cfg(unix)]
        {
            let uid = get_current_uid();
            let runtime_dir = PathBuf::from(format!("/tmp/strata-runtime-{}", uid));
            runtime_dir.join("strata-dev.sock")
        }
        #[cfg(not(unix))]
        {
            PathBuf::from("/tmp/strata-dev.sock")
        }
    }
}

fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };

        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);

        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}
