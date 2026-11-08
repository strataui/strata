//! IPC Client communicating with active Strata development sessions over Unix domain sockets.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;
use serde_json::Value;

/// Client connection handle to a running Strata application dev instance.
#[derive(Debug, Clone)]
pub struct StrataDevClient {
    socket_path: PathBuf,
}

impl Default for StrataDevClient {
    fn default() -> Self {
        Self::new(default_socket_path())
    }
}

impl StrataDevClient {
    /// Creates a client targeting the given Unix domain socket path.
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }

    /// Checks if a running Strata dev instance is reachable at the socket path.
    pub fn is_connected(&self) -> bool {
        if !self.socket_path.exists() {
            return false;
        }
        match UnixStream::connect(&self.socket_path) {
            Ok(stream) => {
                let _ = stream.set_read_timeout(Some(Duration::from_millis(300)));
                true
            }
            Err(_) => false,
        }
    }

    /// Sends a JSON-RPC command to the running Strata application and waits for the response.
    pub fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        if !self.socket_path.exists() {
            return Err(format!(
                "No active Strata dev session found at '{}'. Run 'strata run' or call 'strata_run_app' first.",
                self.socket_path.display()
            ));
        }

        let mut stream = UnixStream::connect(&self.socket_path).map_err(|e| {
            format!(
                "Failed to connect to Strata dev socket '{}': {}",
                self.socket_path.display(),
                e
            )
        })?;

        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| format!("Failed to set read timeout: {}", e))?;
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| format!("Failed to set write timeout: {}", e))?;

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });

        let mut payload = serde_json::to_string(&request).map_err(|e| e.to_string())?;
        payload.push('\n');

        stream
            .write_all(payload.as_bytes())
            .map_err(|e| format!("Failed to send command to Strata dev instance: {}", e))?;
        stream
            .flush()
            .map_err(|e| format!("Failed to flush socket: {}", e))?;

        let mut reader = BufReader::new(stream);
        let mut response_line = String::new();
        reader
            .read_line(&mut response_line)
            .map_err(|e| format!("Failed to read response from Strata dev instance: {}", e))?;

        if response_line.trim().is_empty() {
            return Err("Empty response received from Strata dev instance".to_string());
        }

        let parsed: Value = serde_json::from_str(&response_line).map_err(|e| {
            format!(
                "Failed to parse JSON response '{}': {}",
                response_line.trim(),
                e
            )
        })?;

        if let Some(err) = parsed.get("error") {
            let msg = err
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error from Strata dev session");
            return Err(msg.to_string());
        }

        if let Some(result) = parsed.get("result") {
            Ok(result.clone())
        } else {
            Ok(Value::Null)
        }
    }

    /// Fetches live session details including viewport dimensions, DPR, focus, and MCP cursor state.
    pub fn session_info(&self) -> Result<Value, String> {
        self.call("session_info", serde_json::json!({}))
    }

    /// Captures a PNG screenshot from the active window with the emulated MCP cursor overlay.
    pub fn take_screenshot(&self) -> Result<Vec<u8>, String> {
        let resp = self.call("take_screenshot", serde_json::json!({}))?;
        let base64_str = resp
            .get("png_base64")
            .and_then(|b| b.as_str())
            .ok_or_else(|| "Missing png_base64 in screenshot response".to_string())?;
        base64_decode(base64_str)
    }

    /// Moves the emulated MCP cursor to (x, y).
    pub fn mouse_move(&self, x: f32, y: f32, smooth: bool) -> Result<Value, String> {
        self.call(
            "mouse_move",
            serde_json::json!({ "x": x, "y": y, "smooth": smooth }),
        )
    }

    /// Clicks at the current or specified coordinates.
    pub fn mouse_click(
        &self,
        button: &str,
        x: Option<f32>,
        y: Option<f32>,
        count: usize,
    ) -> Result<Value, String> {
        self.call(
            "mouse_click",
            serde_json::json!({
                "button": button,
                "x": x,
                "y": y,
                "count": count,
            }),
        )
    }

    /// Emulates mouse button down.
    pub fn mouse_down(&self, button: &str, x: Option<f32>, y: Option<f32>) -> Result<Value, String> {
        self.call(
            "mouse_down",
            serde_json::json!({ "button": button, "x": x, "y": y }),
        )
    }

    /// Emulates mouse button release up.
    pub fn mouse_up(&self, button: &str, x: Option<f32>, y: Option<f32>) -> Result<Value, String> {
        self.call(
            "mouse_up",
            serde_json::json!({ "button": button, "x": x, "y": y }),
        )
    }

    /// Emulates mouse scroll wheels.
    pub fn mouse_scroll(
        &self,
        delta_x: f32,
        delta_y: f32,
        x: Option<f32>,
        y: Option<f32>,
    ) -> Result<Value, String> {
        self.call(
            "mouse_scroll",
            serde_json::json!({
                "delta_x": delta_x,
                "delta_y": delta_y,
                "x": x,
                "y": y,
            }),
        )
    }

    /// Types text into the focused element.
    pub fn keyboard_type(&self, text: &str) -> Result<Value, String> {
        self.call("keyboard_type", serde_json::json!({ "text": text }))
    }

    /// Sends a keyboard key code with optional modifier flags.
    pub fn keyboard_key(&self, key: &str, modifiers: Value) -> Result<Value, String> {
        self.call(
            "keyboard_key",
            serde_json::json!({ "key": key, "modifiers": modifiers }),
        )
    }

    /// Inspects an element at coordinates (x, y).
    pub fn inspect_element(&self, x: f32, y: f32) -> Result<Value, String> {
        self.call("inspect_element", serde_json::json!({ "x": x, "y": y }))
    }

    /// Executes a sequential batch of UI actions.
    pub fn execute_actions(&self, actions: Vec<Value>) -> Result<Value, String> {
        self.call("execute_actions", serde_json::json!({ "actions": actions }))
    }
}

fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0;

    for &b in input.as_bytes() {
        if b.is_ascii_whitespace() || b == b'=' {
            continue;
        }
        let val = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err(format!("Invalid base64 byte: {}", b as char)),
        };
        buf = (buf << 6) | (val as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Ok(out)
}

/// Computes the default Unix domain socket path for Strata dev sessions.
pub fn default_socket_path() -> PathBuf {
    if let Ok(path) = std::env::var("STRATA_DEV_SOCKET") {
        PathBuf::from(path)
    } else if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime_dir).join("strata-dev.sock")
    } else {
        #[cfg(unix)]
        {
            extern "C" {
                fn getuid() -> u32;
            }
            let uid = unsafe { getuid() };
            PathBuf::from(format!("/tmp/strata-runtime-{}", uid)).join("strata-dev.sock")
        }
        #[cfg(not(unix))]
        {
            PathBuf::from("/tmp/strata-dev.sock")
        }
    }
}
