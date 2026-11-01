//! Bidirectional IPC protocol definition for communicating with isolated Chromium workers.

use strata_primitives::geom::Rect;

/// Mouse interaction type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseAction {
    Move,
    Down,
    Up,
    Wheel,
}

/// Mouse button identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    None,
    Left,
    Right,
    Middle,
}

/// Keyboard interaction type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Down,
    Up,
    Char,
}

/// Keyboard modifier flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

/// Browser navigation lifecycle state.
#[derive(Debug, Clone, PartialEq)]
pub enum NavigationState {
    Idle,
    Loading { url: String, progress: f32 },
    Completed { url: String, title: String },
    Failed { url: String, error: String },
}

/// Host-to-Chromium command.
#[derive(Debug, Clone, PartialEq)]
pub enum ChromiumCommand {
    /// Navigate to a target URL.
    Navigate { url: String },
    /// Resize the browser viewport.
    Resize {
        width: u32,
        height: u32,
        scale_factor: f32,
    },
    /// Forward mouse interaction.
    MouseEvent {
        action: MouseAction,
        x: f32,
        y: f32,
        button: MouseButton,
        modifiers: Modifiers,
        delta_x: f32,
        delta_y: f32,
    },
    /// Forward keyboard interaction.
    KeyEvent {
        action: KeyAction,
        key_code: u32,
        text: Option<String>,
        modifiers: Modifiers,
    },
    /// Evaluate arbitrary JavaScript in page context.
    ExecuteJs { request_id: u64, script: String },
    /// Inject a cookie into the browser session.
    SetCookie {
        name: String,
        value: String,
        domain: String,
        path: String,
    },
    /// Request browser reload.
    Reload,
    /// Request graceful subprocess termination.
    Terminate,
}

/// Chromium-to-Host event.
#[derive(Debug, Clone, PartialEq)]
pub enum ChromiumEvent {
    /// New raster frame available in shared memory.
    FrameReady {
        buffer_id: u32,
        width: u32,
        height: u32,
        stride: u32,
        damage_rects: Vec<Rect>,
    },
    /// Page title was updated.
    TitleChanged { title: String },
    /// Active URL changed (e.g. following redirects or pushState).
    UrlChanged { url: String },
    /// Page load progress updated (0.0 to 1.0).
    LoadingProgress { progress: f32 },
    /// Intercepted or captured cookie (e.g. OAuth tokens).
    CookieCaptured {
        name: String,
        value: String,
        domain: String,
    },
    /// Result of an asynchronous JavaScript evaluation.
    JsResult {
        request_id: u64,
        result: Result<String, String>,
    },
    /// Subprocess exited.
    ProcessTerminated { exit_code: i32 },
}

impl ChromiumCommand {
    /// Serializes a command into a line-delimited message string.
    pub fn serialize(&self) -> String {
        match self {
            Self::Navigate { url } => format!("NAVIGATE|{}", url),
            Self::Resize { width, height, scale_factor } => {
                format!("RESIZE|{}|{}|{}", width, height, scale_factor)
            }
            Self::MouseEvent { action, x, y, button, modifiers, delta_x, delta_y } => {
                format!(
                    "MOUSE|{:?}|{}|{}|{:?}|{}|{}|{}|{}|{}|{}",
                    action, x, y, button,
                    modifiers.shift as u8, modifiers.ctrl as u8, modifiers.alt as u8, modifiers.meta as u8,
                    delta_x, delta_y
                )
            }
            Self::KeyEvent { action, key_code, text, modifiers } => {
                format!(
                    "KEY|{:?}|{}|{}|{}|{}|{}|{}",
                    action, key_code, text.as_deref().unwrap_or(""),
                    modifiers.shift as u8, modifiers.ctrl as u8, modifiers.alt as u8, modifiers.meta as u8
                )
            }
            Self::ExecuteJs { request_id, script } => {
                format!("EXEC_JS|{}|{}", request_id, script)
            }
            Self::SetCookie { name, value, domain, path } => {
                format!("SET_COOKIE|{}|{}|{}|{}", name, value, domain, path)
            }
            Self::Reload => "RELOAD".to_string(),
            Self::Terminate => "TERMINATE".to_string(),
        }
    }

    /// Deserializes a command from a line-delimited message string.
    pub fn deserialize(line: &str) -> Option<Self> {
        let parts: Vec<&str> = line.trim().split('|').collect();
        if parts.is_empty() {
            return None;
        }

        match parts[0] {
            "NAVIGATE" => Some(Self::Navigate {
                url: parts.get(1)?.to_string(),
            }),
            "RESIZE" => Some(Self::Resize {
                width: parts.get(1)?.parse().ok()?,
                height: parts.get(2)?.parse().ok()?,
                scale_factor: parts.get(3)?.parse().ok()?,
            }),
            "RELOAD" => Some(Self::Reload),
            "TERMINATE" => Some(Self::Terminate),
            _ => None,
        }
    }
}
