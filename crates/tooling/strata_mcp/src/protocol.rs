//! Model Context Protocol (MCP) JSON-RPC 2.0 schema types and safe action definitions.

use serde::{Deserialize, Serialize};

/// Standard JSON-RPC 2.0 Request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    #[serde(default = "default_jsonrpc")]
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    pub fn new(id: Option<serde_json::Value>, method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.into(),
            params,
        }
    }
}

fn default_jsonrpc() -> String {
    "2.0".to_string()
}

/// Standard JSON-RPC 2.0 Response wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    pub fn is_success(&self) -> bool {
        self.error.is_none() && self.result.is_some()
    }

    pub fn success(id: Option<serde_json::Value>, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Option<serde_json::Value>, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

/// Standard JSON-RPC 2.0 Error payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Information identifying an MCP implementation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplementationInfo {
    pub name: String,
    pub version: String,
}

/// Server capabilities declared during MCP handshake.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<serde_json::Value>,
}

/// MCP initialization handshake response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    pub server_info: ImplementationInfo,
}

/// MCP Tool metadata definition exposed in `tools/list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Result of calling an MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub content: Vec<McpContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// Content items returned from tool executions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpContent {
    Text {
        text: String,
    },
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
}

impl ToolCallResult {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![McpContent::Text {
                text: text.into(),
            }],
            is_error: None,
        }
    }

    pub fn image_and_text(png_base64: String, description: impl Into<String>) -> Self {
        Self {
            content: vec![
                McpContent::Image {
                    data: png_base64,
                    mime_type: "image/png".to_string(),
                },
                McpContent::Text {
                    text: description.into(),
                },
            ],
            is_error: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content: vec![McpContent::Text {
                text: message.into(),
            }],
            is_error: Some(true),
        }
    }
}

/// Keyboard modifiers for key input commands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpModifiers {
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub meta: bool,
}

/// Safe, declarative batch actions that can be executed sequentially in a Strata dev session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum McpAction {
    /// Moves the emulated cursor to `(x, y)`.
    Move {
        x: f32,
        y: f32,
        #[serde(default = "default_true")]
        smooth: bool,
    },
    /// Presses the mouse button down.
    Down {
        #[serde(default = "default_left")]
        button: String,
    },
    /// Releases the mouse button up.
    Up {
        #[serde(default = "default_left")]
        button: String,
    },
    /// Performs a full click at the current or specified coordinates.
    Click {
        x: Option<f32>,
        y: Option<f32>,
        #[serde(default = "default_left")]
        button: String,
        #[serde(default = "default_one")]
        count: usize,
    },
    /// Scrolls the viewport horizontally or vertically.
    Scroll {
        #[serde(default)]
        delta_x: f32,
        delta_y: f32,
        x: Option<f32>,
        y: Option<f32>,
    },
    /// Types a text string into the active focused input.
    Type {
        text: String,
    },
    /// Sends a discrete keyboard press and release with modifiers.
    Key {
        key: String,
        #[serde(default)]
        modifiers: McpModifiers,
    },
    /// Pauses execution for a safe duration in milliseconds (clamped to max 5000ms).
    Wait {
        ms: u64,
    },
}

fn default_true() -> bool {
    true
}

fn default_left() -> String {
    "left".to_string()
}

fn default_one() -> usize {
    1
}
