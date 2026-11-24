//! Model Context Protocol (MCP) Dev Bridge and In-Process IPC Dispatcher.
//!
//! Provides the bidirectional communication pipeline connecting external AI agents
//! to active Strata runtime windows, emulating mouse movements with authentic Yaru
//! cursor overlays, taking high-resolution screenshots, and safely batching UI actions.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;
use std::time::Duration;

use crate::runtime::AppRuntime;
use strata_platform::{KeyCode, Modifiers, MouseButton, PlatformEvent};

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
            format!(
                "Peer UID {} does not match process UID {}",
                ucred.uid, my_uid
            ),
        ));
    }

    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn verify_peer_credentials(stream: &UnixStream) -> Result<(), std::io::Error> {
    use std::os::unix::io::AsRawFd;

    extern "C" {
        fn getpeereid(
            s: std::os::raw::c_int,
            euid: *mut u32,
            egid: *mut u32,
        ) -> std::os::raw::c_int;
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

fn dispatch_to_ui_thread(req_json: String) -> String {
    let (tx, rx) = channel();
    if let Ok(mut q) = get_pending_queue().lock() {
        q.push((req_json, tx));
    }

    // Signal GTK main event loop to wake up and process the queue
    #[cfg(target_os = "linux")]
    strata_platform::post_platform_event(strata_platform::PlatformEvent::RedrawRequested);

    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(res) => res,
        Err(_) => json!({
            "jsonrpc": "2.0",
            "error": { "code": -32000, "message": "Request timed out waiting for UI thread" }
        })
        .to_string(),
    }
}

/// Splits an `execute_actions` request on `"wait"` steps so every delay sleeps on the background
/// IPC thread instead of blocking the UI main thread or polluting `input_dispatch_us`.
fn execute_actions_nonblocking_on_ipc_thread(id: Value, actions: &[Value]) -> String {
    let mut total_executed: u64 = 0;
    let mut pending_batch: Vec<Value> = Vec::new();

    for act in actions {
        if act.get("action").and_then(|v| v.as_str()) == Some("wait") {
            if !pending_batch.is_empty() {
                let batch = std::mem::take(&mut pending_batch);
                let sub_req = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "method": "execute_actions",
                    "params": { "actions": batch },
                });
                let resp_str = dispatch_to_ui_thread(sub_req.to_string());
                if let Ok(parsed) = serde_json::from_str::<Value>(&resp_str) {
                    if parsed.get("error").is_some() {
                        return resp_str;
                    }
                    total_executed += parsed
                        .get("result")
                        .and_then(|r| r.get("executed_actions"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                } else {
                    return resp_str;
                }
            }
            let ms = act
                .get("ms")
                .and_then(|m| m.as_u64())
                .unwrap_or(50)
                .min(5000);
            if ms > 0 {
                std::thread::sleep(Duration::from_millis(ms));
            }
            total_executed += 1;
        } else {
            pending_batch.push(act.clone());
        }
    }

    if !pending_batch.is_empty() {
        let sub_req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "execute_actions",
            "params": { "actions": pending_batch },
        });
        let resp_str = dispatch_to_ui_thread(sub_req.to_string());
        if let Ok(parsed) = serde_json::from_str::<Value>(&resp_str) {
            if parsed.get("error").is_some() {
                return resp_str;
            }
            total_executed += parsed
                .get("result")
                .and_then(|r| r.get("executed_actions"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
        } else {
            return resp_str;
        }
    }

    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": { "executed_actions": total_executed },
    })
    .to_string()
}

fn handle_client_connection(mut stream: UnixStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));

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

        // Handle `execute_actions` batches containing `"wait"` on this background IPC thread so
        // waiting never freezes the UI main thread or spikes `input_dispatch_us`.
        let response = if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
            let is_exec_with_wait = parsed.get("method").and_then(|m| m.as_str())
                == Some("execute_actions")
                && parsed
                    .get("params")
                    .and_then(|p| p.get("actions"))
                    .and_then(|a| a.as_array())
                    .is_some_and(|arr| {
                        arr.iter()
                            .any(|a| a.get("action").and_then(|v| v.as_str()) == Some("wait"))
                    });
            if is_exec_with_wait {
                let id = parsed.get("id").cloned().unwrap_or(Value::Null);
                let actions = parsed
                    .get("params")
                    .and_then(|p| p.get("actions"))
                    .and_then(|a| a.as_array())
                    .cloned()
                    .unwrap_or_default();
                execute_actions_nonblocking_on_ipc_thread(id, &actions)
            } else {
                dispatch_to_ui_thread(trimmed.to_string())
            }
        } else {
            dispatch_to_ui_thread(trimmed.to_string())
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

/// Structured outcome returned when draining or executing MCP commands, separating full view
/// rebuilds from paint-only redraws (e.g. fast-path hover restyling or cursor-only moves).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct McpDrainOutcome {
    pub needs_rebuild: bool,
    pub needs_redraw: bool,
}

impl McpDrainOutcome {
    pub fn merge(&mut self, other: Self) {
        self.needs_rebuild |= other.needs_rebuild;
        self.needs_redraw |= other.needs_redraw;
    }

    pub fn any_dirty(self) -> bool {
        self.needs_rebuild || self.needs_redraw
    }
}

/// Dispatches a synthetic MCP [`PlatformEvent`] through DevTools (when enabled) and the host
/// application's event hook (`launch_with_events` / `launch_with_frame`), returning
/// `(intercepted_by_devtools, needs_rebuild, needs_redraw)`.
fn dispatch_synthetic_event_to_hook<F>(
    runtime: &mut AppRuntime,
    event: &PlatformEvent,
    user_event_hook: &mut F,
) -> (bool, bool, bool)
where
    F: FnMut(&PlatformEvent, &mut AppRuntime) -> bool,
{
    #[cfg(feature = "devtools")]
    if let Some(ref mut dt) = runtime.devtools {
        if strata_devtools::DevToolsEventInterceptor::intercept(&mut dt.hud, event) {
            runtime.mark_paint_dirty();
            return (true, true, true);
        }
    }
    let hook_signals_before = strata_dom::reactive::has_dirty_signals();
    if user_event_hook(event, runtime)
        || (!hook_signals_before && strata_dom::reactive::has_dirty_signals())
    {
        runtime.mark_paint_dirty();
        (false, true, true)
    } else {
        (false, false, false)
    }
}

fn current_mcp_pointer_pos(runtime: &AppRuntime) -> (f32, f32) {
    runtime
        .mcp_cursor
        .as_ref()
        .map(|c| (c.current_x, c.current_y))
        .unwrap_or(runtime.last_mouse_pos)
}

fn exec_mcp_move<F>(
    runtime: &mut AppRuntime,
    x: f32,
    y: f32,
    smooth: bool,
    user_event_hook: &mut F,
) -> McpDrainOutcome
where
    F: FnMut(&PlatformEvent, &mut AppRuntime) -> bool,
{
    runtime.last_mouse_pos = (x, y);
    let ev = PlatformEvent::MouseMove { x, y };
    let (intercepted, h_rb, h_rd) = dispatch_synthetic_event_to_hook(runtime, &ev, user_event_hook);
    if intercepted {
        return McpDrainOutcome {
            needs_rebuild: h_rb,
            needs_redraw: h_rd,
        };
    }
    let (rb, rd) = runtime.mcp_move_pointer_outcome(x, y, smooth);
    McpDrainOutcome {
        needs_rebuild: h_rb || rb,
        needs_redraw: h_rd || rd,
    }
}

fn exec_mcp_down<F>(
    runtime: &mut AppRuntime,
    button: MouseButton,
    x: Option<f32>,
    y: Option<f32>,
    user_event_hook: &mut F,
) -> McpDrainOutcome
where
    F: FnMut(&PlatformEvent, &mut AppRuntime) -> bool,
{
    let mut out = McpDrainOutcome::default();
    let (px, py) = match (x, y) {
        (Some(px), Some(py)) => {
            out.merge(exec_mcp_move(runtime, px, py, false, user_event_hook));
            (px, py)
        }
        _ => current_mcp_pointer_pos(runtime),
    };
    let ev = PlatformEvent::MouseDown {
        button,
        x: px,
        y: py,
    };
    let (intercepted, h_rb, h_rd) = dispatch_synthetic_event_to_hook(runtime, &ev, user_event_hook);
    out.merge(McpDrainOutcome {
        needs_rebuild: h_rb,
        needs_redraw: h_rd,
    });
    if !intercepted {
        let (rb, rd) = runtime.mcp_mouse_down_outcome(button, None, None);
        out.merge(McpDrainOutcome {
            needs_rebuild: rb,
            needs_redraw: rd,
        });
    }
    out
}

fn exec_mcp_up<F>(
    runtime: &mut AppRuntime,
    button: MouseButton,
    x: Option<f32>,
    y: Option<f32>,
    user_event_hook: &mut F,
) -> McpDrainOutcome
where
    F: FnMut(&PlatformEvent, &mut AppRuntime) -> bool,
{
    let mut out = McpDrainOutcome::default();
    let (px, py) = match (x, y) {
        (Some(px), Some(py)) => {
            out.merge(exec_mcp_move(runtime, px, py, false, user_event_hook));
            (px, py)
        }
        _ => current_mcp_pointer_pos(runtime),
    };
    let ev = PlatformEvent::MouseUp {
        button,
        x: px,
        y: py,
    };
    let (intercepted, h_rb, h_rd) = dispatch_synthetic_event_to_hook(runtime, &ev, user_event_hook);
    out.merge(McpDrainOutcome {
        needs_rebuild: h_rb,
        needs_redraw: h_rd,
    });
    if !intercepted {
        let (rb, rd) = runtime.mcp_mouse_up_outcome(button, None, None);
        out.merge(McpDrainOutcome {
            needs_rebuild: rb,
            needs_redraw: rd,
        });
    }
    out
}

fn exec_mcp_click<F>(
    runtime: &mut AppRuntime,
    button: MouseButton,
    x: Option<f32>,
    y: Option<f32>,
    count: usize,
    user_event_hook: &mut F,
) -> McpDrainOutcome
where
    F: FnMut(&PlatformEvent, &mut AppRuntime) -> bool,
{
    let mut out = McpDrainOutcome::default();
    if let (Some(px), Some(py)) = (x, y) {
        out.merge(exec_mcp_move(runtime, px, py, false, user_event_hook));
    }
    for _ in 0..count.max(1) {
        out.merge(exec_mcp_down(runtime, button, None, None, user_event_hook));
        out.merge(exec_mcp_up(runtime, button, None, None, user_event_hook));
    }
    out
}

fn exec_mcp_scroll<F>(
    runtime: &mut AppRuntime,
    delta_x: f32,
    delta_y: f32,
    x: Option<f32>,
    y: Option<f32>,
    user_event_hook: &mut F,
) -> McpDrainOutcome
where
    F: FnMut(&PlatformEvent, &mut AppRuntime) -> bool,
{
    let mut out = McpDrainOutcome::default();
    if let (Some(px), Some(py)) = (x, y) {
        out.merge(exec_mcp_move(runtime, px, py, false, user_event_hook));
    }
    let ev = PlatformEvent::Scroll {
        delta_x,
        delta_y,
        is_momentum: false,
    };
    let (intercepted, h_rb, h_rd) = dispatch_synthetic_event_to_hook(runtime, &ev, user_event_hook);
    out.merge(McpDrainOutcome {
        needs_rebuild: h_rb,
        needs_redraw: h_rd,
    });
    if !intercepted {
        let (rb, rd) = runtime.mcp_mouse_scroll_outcome(delta_x, delta_y, None, None);
        out.merge(McpDrainOutcome {
            needs_rebuild: rb,
            needs_redraw: rd,
        });
    }
    out
}

fn exec_mcp_type<F>(
    runtime: &mut AppRuntime,
    text: &str,
    user_event_hook: &mut F,
) -> McpDrainOutcome
where
    F: FnMut(&PlatformEvent, &mut AppRuntime) -> bool,
{
    let mut out = McpDrainOutcome::default();
    let mut char_buf = [0u8; 4];
    for ch in text.chars() {
        let ev = PlatformEvent::ReceivedCharacter(ch);
        let (intercepted, h_rb, h_rd) =
            dispatch_synthetic_event_to_hook(runtime, &ev, user_event_hook);
        out.merge(McpDrainOutcome {
            needs_rebuild: h_rb,
            needs_redraw: h_rd,
        });
        if !intercepted {
            let s = ch.encode_utf8(&mut char_buf);
            let (rb, rd) = runtime.mcp_keyboard_type_outcome(s);
            out.merge(McpDrainOutcome {
                needs_rebuild: rb,
                needs_redraw: rd,
            });
        }
    }
    out
}

fn exec_mcp_key<F>(
    runtime: &mut AppRuntime,
    key: KeyCode,
    modifiers: Modifiers,
    user_event_hook: &mut F,
) -> McpDrainOutcome
where
    F: FnMut(&PlatformEvent, &mut AppRuntime) -> bool,
{
    let mut out = McpDrainOutcome::default();
    if let Some(ref mut c) = runtime.mcp_cursor {
        c.touch_activity();
    }
    let ev_down = PlatformEvent::KeyDown { key, modifiers };
    let (intercepted_down, h_rb_down, h_rd_down) =
        dispatch_synthetic_event_to_hook(runtime, &ev_down, user_event_hook);
    out.merge(McpDrainOutcome {
        needs_rebuild: h_rb_down,
        needs_redraw: h_rd_down,
    });
    if !intercepted_down {
        let res_down = runtime.dispatch_key_event(&ev_down);
        out.merge(McpDrainOutcome {
            needs_rebuild: res_down.needs_rebuild(),
            needs_redraw: res_down.needs_redraw(),
        });
    }

    let ev_up = PlatformEvent::KeyUp { key, modifiers };
    let (intercepted_up, h_rb_up, h_rd_up) =
        dispatch_synthetic_event_to_hook(runtime, &ev_up, user_event_hook);
    out.merge(McpDrainOutcome {
        needs_rebuild: h_rb_up,
        needs_redraw: h_rd_up,
    });
    if !intercepted_up {
        let res_up = runtime.dispatch_key_event(&ev_up);
        out.merge(McpDrainOutcome {
            needs_rebuild: res_up.needs_rebuild(),
            needs_redraw: res_up.needs_redraw(),
        });
    }
    if out.needs_redraw {
        runtime.mark_paint_dirty();
    }
    out
}

/// Drains and processes all pending MCP requests on the active main UI thread, forwarding all
/// synthetic input events (`KeyDown`/`KeyUp`/`ReceivedCharacter`/`Scroll`/`MouseMove`/`MouseDown`/`MouseUp`)
/// through `user_event_hook` just like native OS events.
///
/// For `take_screenshot`, captures the pixel buffer on the UI thread (~1–2 ms) and offloads
/// PNG compression + base64 encoding onto a background worker thread so rendering never stalls.
pub fn drain_and_execute_mcp_requests_with_hook<F>(
    runtime: &mut AppRuntime,
    mut user_event_hook: F,
) -> McpDrainOutcome
where
    F: FnMut(&PlatformEvent, &mut AppRuntime) -> bool,
{
    let mut items = Vec::new();
    {
        if let Ok(mut lock) = get_pending_queue().lock() {
            if lock.is_empty() {
                return McpDrainOutcome::default();
            }
            items = std::mem::take(&mut *lock);
        }
    }

    let mut total = McpDrainOutcome::default();
    for (req_str, tx) in items {
        let parsed: Value = match serde_json::from_str(&req_str) {
            Ok(v) => v,
            Err(e) => {
                let err = json!({
                    "jsonrpc": "2.0",
                    "error": { "code": -32700, "message": format!("Parse error: {}", e) }
                });
                let _ = tx.send(err.to_string());
                continue;
            }
        };

        let id = parsed.get("id").cloned();
        let method = parsed.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = parsed.get("params").cloned().unwrap_or(json!({}));

        if method == "take_screenshot" {
            match runtime.mcp_capture_screenshot_buffer() {
                Ok(buf) => {
                    let width = buf.width;
                    let height = buf.height;
                    let tx_worker = tx.clone();
                    let id_worker = id.clone();
                    let spawn_res = std::thread::Builder::new()
                        .name("strata-mcp-png-encode".to_string())
                        .spawn(move || {
                            let resp = match buf.encode_png_bytes() {
                                Ok(bytes) => json!({
                                    "jsonrpc": "2.0",
                                    "id": id_worker,
                                    "result": {
                                        "png_base64": base64_encode(&bytes),
                                        "width": width,
                                        "height": height,
                                    }
                                }),
                                Err(err_msg) => json!({
                                    "jsonrpc": "2.0",
                                    "id": id_worker,
                                    "error": { "code": -32603, "message": err_msg }
                                }),
                            };
                            let _ = tx_worker.send(resp.to_string());
                        });
                    if let Err(e) = spawn_res {
                        let resp = json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": { "code": -32603, "message": format!("Failed to spawn PNG encoder thread: {e}") }
                        });
                        let _ = tx.send(resp.to_string());
                    }
                }
                Err(err_msg) => {
                    let resp = json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32603, "message": err_msg }
                    });
                    let _ = tx.send(resp.to_string());
                }
            }
            continue;
        }

        let (result, outcome) =
            execute_mcp_command_with_hook(runtime, method, params, &mut user_event_hook);
        total.merge(outcome);

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
        let _ = tx.send(resp.to_string());
    }

    total
}

/// Drains and processes all pending MCP requests on the active main UI thread, returning
/// separate `needs_rebuild` and `needs_redraw` flags.
pub fn drain_and_execute_mcp_requests_with_outcome(runtime: &mut AppRuntime) -> McpDrainOutcome {
    drain_and_execute_mcp_requests_with_hook(runtime, |_, _| false)
}

/// Drains and processes all pending MCP requests on the active main UI thread.
///
/// Returns true if any executed command requires a view rebuild or display list redraw.
pub fn drain_and_execute_mcp_requests(runtime: &mut AppRuntime) -> bool {
    drain_and_execute_mcp_requests_with_outcome(runtime).any_dirty()
}

/// Core execution routine for MCP commands directly modifying `AppRuntime` (returning a combined dirty bool).
pub fn execute_mcp_command_core(
    runtime: &mut AppRuntime,
    method: &str,
    params: Value,
) -> (Result<Value, String>, bool) {
    let (res, outcome) = execute_mcp_command_with_outcome(runtime, method, params);
    (res, outcome.any_dirty())
}

/// Core execution routine for MCP commands returning structured `(needs_rebuild, needs_redraw)`.
pub fn execute_mcp_command_with_outcome(
    runtime: &mut AppRuntime,
    method: &str,
    params: Value,
) -> (Result<Value, String>, McpDrainOutcome) {
    execute_mcp_command_with_hook(runtime, method, params, &mut |_, _| false)
}

/// Core execution routine for MCP commands that also forwards synthetic input events through the
/// host application's event hook (`launch_with_events` / `launch_with_frame`).
pub fn execute_mcp_command_with_hook<F>(
    runtime: &mut AppRuntime,
    method: &str,
    params: Value,
    user_event_hook: &mut F,
) -> (Result<Value, String>, McpDrainOutcome)
where
    F: FnMut(&PlatformEvent, &mut AppRuntime) -> bool,
{
    let clean = McpDrainOutcome::default();
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
                let text = if runtime.active_input_is_password(a) {
                    AppRuntime::mask_password_text(&a.state.text.get())
                } else {
                    a.state.text.get()
                };
                json!({
                    "input_index": a.input_index,
                    "input_id": a.input_id,
                    "text": text,
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
                "cursor_theme": runtime.cursor_theme().name,
            });

            (Ok(info), clean)
        }
        "get_perf_profile" => {
            let summary = runtime.perf_profile_summary();
            match serde_json::to_value(&summary) {
                Ok(val) => (Ok(val), clean),
                Err(e) => (Err(format!("Failed to serialize perf profile: {e}")), clean),
            }
        }
        "mouse_move" => {
            let x = params.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            let y = params.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            let smooth = params
                .get("smooth")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            let out = exec_mcp_move(runtime, x, y, smooth, user_event_hook);
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
                    "cursor_icon": cur_icon,
                    "needs_rebuild": out.needs_rebuild,
                    "needs_redraw": out.needs_redraw,
                    "hover_fast_path_used": runtime.last_hover_fast_path_used(),
                })),
                out,
            )
        }
        "mouse_click" => {
            let x = params.get("x").and_then(|v| v.as_f64()).map(|f| f as f32);
            let y = params.get("y").and_then(|v| v.as_f64()).map(|f| f as f32);
            let btn_str = params
                .get("button")
                .and_then(|b| b.as_str())
                .unwrap_or("left");
            let count = params.get("count").and_then(|c| c.as_u64()).unwrap_or(1) as usize;
            let button = parse_mouse_button(btn_str);

            let out = exec_mcp_click(runtime, button, x, y, count, user_event_hook);
            (
                Ok(json!({
                    "clicked": true,
                    "needs_rebuild": out.needs_rebuild,
                    "needs_redraw": out.needs_redraw,
                })),
                out,
            )
        }
        "mouse_down" => {
            let x = params.get("x").and_then(|v| v.as_f64()).map(|f| f as f32);
            let y = params.get("y").and_then(|v| v.as_f64()).map(|f| f as f32);
            let btn_str = params
                .get("button")
                .and_then(|b| b.as_str())
                .unwrap_or("left");
            let button = parse_mouse_button(btn_str);

            let out = exec_mcp_down(runtime, button, x, y, user_event_hook);
            (Ok(json!({ "down": true })), out)
        }
        "mouse_up" => {
            let x = params.get("x").and_then(|v| v.as_f64()).map(|f| f as f32);
            let y = params.get("y").and_then(|v| v.as_f64()).map(|f| f as f32);
            let btn_str = params
                .get("button")
                .and_then(|b| b.as_str())
                .unwrap_or("left");
            let button = parse_mouse_button(btn_str);

            let out = exec_mcp_up(runtime, button, x, y, user_event_hook);
            (Ok(json!({ "up": true })), out)
        }
        "mouse_scroll" | "mouse_wheel" => {
            let delta_x = params
                .get("delta_x")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as f32;
            let delta_y = params
                .get("delta_y")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as f32;
            let x = params.get("x").and_then(|v| v.as_f64()).map(|f| f as f32);
            let y = params.get("y").and_then(|v| v.as_f64()).map(|f| f as f32);

            let out = exec_mcp_scroll(runtime, delta_x, delta_y, x, y, user_event_hook);
            (
                Ok(json!({
                    "scrolled": true,
                    "delta_x": delta_x,
                    "delta_y": delta_y,
                    "needs_rebuild": out.needs_rebuild,
                    "needs_redraw": out.needs_redraw,
                })),
                out,
            )
        }
        "keyboard_type" => {
            let text = params.get("text").and_then(|t| t.as_str()).unwrap_or("");
            let out = exec_mcp_type(runtime, text, user_event_hook);
            (Ok(json!({ "typed_chars": text.chars().count() })), out)
        }
        "keyboard_key" | "key_press" => {
            let key_str = params.get("key").and_then(|k| k.as_str()).unwrap_or("");
            let key = parse_key_code(key_str);
            let modifiers = parse_modifiers(params.get("modifiers"));

            let out = exec_mcp_key(runtime, key, modifiers, user_event_hook);
            (
                Ok(json!({
                    "key_sent": key_str,
                    "resolved_key": format!("{:?}", key),
                    "needs_rebuild": out.needs_rebuild,
                    "needs_redraw": out.needs_redraw,
                })),
                out,
            )
        }
        "take_screenshot" => match runtime.mcp_capture_screenshot_buffer() {
            Ok(buf) => {
                let width = buf.width;
                let height = buf.height;
                match buf.encode_png_bytes() {
                    Ok(bytes) => {
                        let base64_str = base64_encode(&bytes);
                        (
                            Ok(json!({
                                "png_base64": base64_str,
                                "width": width,
                                "height": height,
                            })),
                            clean,
                        )
                    }
                    Err(e) => (Err(e), clean),
                }
            }
            Err(e) => (Err(e), clean),
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
                clean,
            )
        }
        "execute_actions" => {
            let actions_val = params.get("actions").cloned().unwrap_or(json!([]));
            let actions: Vec<Value> = actions_val.as_array().cloned().unwrap_or_default();
            let mut executed = 0;
            let mut batch_outcome = McpDrainOutcome::default();

            for act in actions {
                let act_type = act.get("action").and_then(|a| a.as_str()).unwrap_or("");
                match act_type {
                    "move" => {
                        let x = act.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                        let y = act.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                        let smooth = act.get("smooth").and_then(|v| v.as_bool()).unwrap_or(true);
                        batch_outcome.merge(exec_mcp_move(
                            runtime,
                            x,
                            y,
                            smooth,
                            user_event_hook,
                        ));
                        executed += 1;
                    }
                    "down" => {
                        let btn = parse_mouse_button(
                            act.get("button").and_then(|b| b.as_str()).unwrap_or("left"),
                        );
                        let x = act.get("x").and_then(|v| v.as_f64()).map(|f| f as f32);
                        let y = act.get("y").and_then(|v| v.as_f64()).map(|f| f as f32);
                        batch_outcome.merge(exec_mcp_down(runtime, btn, x, y, user_event_hook));
                        executed += 1;
                    }
                    "up" => {
                        let btn = parse_mouse_button(
                            act.get("button").and_then(|b| b.as_str()).unwrap_or("left"),
                        );
                        let x = act.get("x").and_then(|v| v.as_f64()).map(|f| f as f32);
                        let y = act.get("y").and_then(|v| v.as_f64()).map(|f| f as f32);
                        batch_outcome.merge(exec_mcp_up(runtime, btn, x, y, user_event_hook));
                        executed += 1;
                    }
                    "click" => {
                        let x = act.get("x").and_then(|v| v.as_f64()).map(|f| f as f32);
                        let y = act.get("y").and_then(|v| v.as_f64()).map(|f| f as f32);
                        let btn = parse_mouse_button(
                            act.get("button").and_then(|b| b.as_str()).unwrap_or("left"),
                        );
                        let count = act.get("count").and_then(|c| c.as_u64()).unwrap_or(1) as usize;
                        batch_outcome.merge(exec_mcp_click(
                            runtime,
                            btn,
                            x,
                            y,
                            count,
                            user_event_hook,
                        ));
                        executed += 1;
                    }
                    "type" => {
                        let text = act.get("text").and_then(|t| t.as_str()).unwrap_or("");
                        batch_outcome.merge(exec_mcp_type(runtime, text, user_event_hook));
                        executed += 1;
                    }
                    "key" | "key_press" => {
                        let key =
                            parse_key_code(act.get("key").and_then(|k| k.as_str()).unwrap_or(""));
                        let mods = parse_modifiers(act.get("modifiers"));
                        batch_outcome.merge(exec_mcp_key(runtime, key, mods, user_event_hook));
                        executed += 1;
                    }
                    "scroll" | "wheel" => {
                        let dx = act.get("delta_x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                        let dy = act.get("delta_y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                        let x = act.get("x").and_then(|v| v.as_f64()).map(|f| f as f32);
                        let y = act.get("y").and_then(|v| v.as_f64()).map(|f| f as f32);
                        batch_outcome.merge(exec_mcp_scroll(
                            runtime,
                            dx,
                            dy,
                            x,
                            y,
                            user_event_hook,
                        ));
                        executed += 1;
                    }
                    "wait" => {
                        // Waits are scheduled off the UI main thread by the MCP server and the
                        // background IPC listener (`execute_actions_nonblocking_on_ipc_thread`).
                        // Never block the UI thread via `std::thread::sleep`.
                        executed += 1;
                    }
                    _ => {}
                }
            }

            (
                Ok(json!({ "executed_actions": executed })),
                batch_outcome,
            )
        }
        unknown => (Err(format!("Unknown MCP method: {}", unknown)), clean),
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
    match s.trim().to_ascii_lowercase().as_str() {
        "enter" | "return" => KeyCode::Enter,
        "escape" | "esc" => KeyCode::Escape,
        "backspace" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "tab" => KeyCode::Tab,
        "space" | " " => KeyCode::Space,
        "arrowleft" | "left" => KeyCode::ArrowLeft,
        "arrowright" | "right" => KeyCode::ArrowRight,
        "arrowup" | "up" => KeyCode::ArrowUp,
        "arrowdown" | "down" => KeyCode::ArrowDown,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "a" | "keya" => KeyCode::KeyA,
        "b" | "keyb" => KeyCode::KeyB,
        "c" | "keyc" => KeyCode::KeyC,
        "d" | "keyd" => KeyCode::KeyD,
        "e" | "keye" => KeyCode::KeyE,
        "f" | "keyf" => KeyCode::KeyF,
        "g" | "keyg" => KeyCode::KeyG,
        "h" | "keyh" => KeyCode::KeyH,
        "i" | "keyi" => KeyCode::KeyI,
        "j" | "keyj" => KeyCode::KeyJ,
        "k" | "keyk" => KeyCode::KeyK,
        "l" | "keyl" => KeyCode::KeyL,
        "m" | "keym" => KeyCode::KeyM,
        "n" | "keyn" => KeyCode::KeyN,
        "o" | "keyo" => KeyCode::KeyO,
        "p" | "keyp" => KeyCode::KeyP,
        "q" | "keyq" => KeyCode::KeyQ,
        "r" | "keyr" => KeyCode::KeyR,
        "s" | "keys" => KeyCode::KeyS,
        "t" | "keyt" => KeyCode::KeyT,
        "u" | "keyu" => KeyCode::KeyU,
        "v" | "keyv" => KeyCode::KeyV,
        "w" | "keyw" => KeyCode::KeyW,
        "x" | "keyx" => KeyCode::KeyX,
        "y" | "keyy" => KeyCode::KeyY,
        "z" | "keyz" => KeyCode::KeyZ,
        "0" | "digit0" => KeyCode::Digit0,
        "1" | "digit1" => KeyCode::Digit1,
        "2" | "digit2" => KeyCode::Digit2,
        "3" | "digit3" => KeyCode::Digit3,
        "4" | "digit4" => KeyCode::Digit4,
        "5" | "digit5" => KeyCode::Digit5,
        "6" | "digit6" => KeyCode::Digit6,
        "7" | "digit7" => KeyCode::Digit7,
        "8" | "digit8" => KeyCode::Digit8,
        "9" | "digit9" => KeyCode::Digit9,
        "f1" => KeyCode::F1,
        "f2" => KeyCode::F2,
        "f3" => KeyCode::F3,
        "f4" => KeyCode::F4,
        "f5" => KeyCode::F5,
        "f6" => KeyCode::F6,
        "f7" => KeyCode::F7,
        "f8" => KeyCode::F8,
        "f9" => KeyCode::F9,
        "f10" => KeyCode::F10,
        "f11" => KeyCode::F11,
        "f12" => KeyCode::F12,
        "/" | "slash" => KeyCode::Slash,
        "," | "comma" => KeyCode::Comma,
        "." | "period" => KeyCode::Period,
        "-" | "minus" => KeyCode::Minus,
        "=" | "+" | "equal" | "equals" | "plus" => KeyCode::Equals,
        "[" | "bracketleft" => KeyCode::BracketLeft,
        "]" | "bracketright" => KeyCode::BracketRight,
        ";" | "semicolon" => KeyCode::Semicolon,
        "`" | "backquote" | "grave" => KeyCode::Backquote,
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
