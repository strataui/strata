//! Platform Event Model: Mouse, Keyboard, Touch, Window State, and File Drag-and-Drop.

use crate::window::ThemeMode;
use std::path::PathBuf;

/// Mouse button identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Other(u16),
}

impl MouseButton {
    /// Returns the standard W3C DOM button index: 0 for Left, 1 for Middle, 2 for Right, or custom.
    pub fn to_button_index(&self) -> i16 {
        match self {
            MouseButton::Left => 0,
            MouseButton::Middle => 1,
            MouseButton::Right => 2,
            MouseButton::Other(b) => *b as i16,
        }
    }
}

/// Keyboard key codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    KeyA,
    KeyB,
    KeyC,
    KeyD,
    KeyE,
    KeyF,
    KeyG,
    KeyH,
    KeyI,
    KeyJ,
    KeyK,
    KeyL,
    KeyM,
    KeyN,
    KeyO,
    KeyP,
    KeyQ,
    KeyR,
    KeyS,
    KeyT,
    KeyU,
    KeyV,
    KeyW,
    KeyX,
    KeyY,
    KeyZ,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    Escape,
    Enter,
    Tab,
    Space,
    Backspace,
    Delete,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    Slash,
    Comma,
    Period,
    Minus,
    Equals,
    BracketLeft,
    BracketRight,
    Semicolon,
    Backquote,
    Unknown,
}

/// Keyboard modifier keys active during an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool, // Command on macOS, Windows key on Windows
}

/// Touch phase for mobile and touchscreens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchPhase {
    Started,
    Moved,
    Ended,
    Cancelled,
}

/// Native platform event emitted to the Strata event loop.
#[derive(Debug, Clone, PartialEq)]
pub enum PlatformEvent {
    // Window lifecycle & geometry
    WindowResized {
        width: u32,
        height: u32,
    },
    WindowMoved {
        x: i32,
        y: i32,
    },
    WindowCloseRequested,
    WindowFocused(bool),
    /// Emitted when the window enters or leaves the maximized state.
    WindowMaximized(bool),
    ScaleFactorChanged {
        scale_factor: f32,
    },
    ThemeChanged(ThemeMode),
    RedrawRequested,
    /// Periodic VSync display frame animation tick for physics and inertial kinetic scrolling.
    AnimationTick,

    // Cursor / Mouse
    MouseMove {
        x: f32,
        y: f32,
    },
    MouseDown {
        button: MouseButton,
        x: f32,
        y: f32,
    },
    MouseUp {
        button: MouseButton,
        x: f32,
        y: f32,
    },
    DoubleClick {
        button: MouseButton,
        x: f32,
        y: f32,
    },
    Scroll {
        delta_x: f32,
        delta_y: f32,
        is_momentum: bool,
    },
    CursorEntered,
    CursorLeft,

    // Keyboard
    KeyDown {
        key: KeyCode,
        modifiers: Modifiers,
    },
    KeyUp {
        key: KeyCode,
        modifiers: Modifiers,
    },
    ReceivedCharacter(char),

    // Input Method Editor (IME) for internationalized / CJK text composition
    ImePreedit {
        text: String,
        cursor: Option<(usize, usize)>,
    },
    ImeCommit(String),
    ImeDeleteSurrounding {
        offset: i32,
        n_chars: u32,
    },

    // Multi-touch
    Touch {
        touch_id: u64,
        phase: TouchPhase,
        x: f32,
        y: f32,
    },

    // Stylus / Pen Tablet (W3C Pointer Events Level 3 / Wayland tablet-v2 / GTK / NSEvent / WM_POINTER)
    Stylus {
        phase: StylusPhase,
        pointer_id: u64,
        tool: PointerKind,
        x: f32,
        y: f32,
        /// Normalized tip pressure in `[0.0, 1.0]`.
        pressure: f32,
        /// Pen tilt along the X axis in degrees (`-90.0..=90.0`).
        tilt_x: f32,
        /// Pen tilt along the Y axis in degrees (`-90.0..=90.0`).
        tilt_y: f32,
        /// Clockwise barrel rotation in degrees (`0.0..=359.0`).
        twist: f32,
        /// Barrel tangential pressure / airbrush wheel in `[-1.0, 1.0]`.
        tangential_pressure: f32,
        /// `true` when the stylus is hovering in proximity without touching the surface.
        hovering: bool,
        /// Bitmask of pressed barrel / tip buttons.
        buttons: u16,
    },

    // File Drag-and-Drop
    FileDrop(Vec<PathBuf>),

    // Desktop System Tray
    TrayIconClicked,
    TrayMenuItemClicked(u64),

    // Single-Instance IPC
    SingleInstanceTriggered(Vec<String>),
}

/// Type of pointing device / stylus tool emitting pointer events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PointerKind {
    Mouse,
    Touch,
    Pen,
    Eraser,
    Brush,
    Airbrush,
}

impl PointerKind {
    /// Returns the W3C PointerEvent `pointerType` string (`"mouse"`, `"touch"`, or `"pen"`).
    pub const fn as_w3c_str(self) -> &'static str {
        match self {
            Self::Mouse => "mouse",
            Self::Touch => "touch",
            Self::Pen | Self::Eraser | Self::Brush | Self::Airbrush => "pen",
        }
    }
}

/// Lifecycle phase of a stylus / pen tablet interaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StylusPhase {
    Down,
    Move,
    Up,
    Hover,
    Cancel,
}

/// Decodes a `file://` URI (or raw local filesystem path) with RFC 3986 percent-encoding into a `PathBuf`.
pub fn decode_file_uri_to_path(uri: &str) -> Option<PathBuf> {
    let trimmed = uri.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let raw_path = if let Some(rest) = trimmed.strip_prefix("file://localhost") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("file://") {
        rest
    } else {
        trimmed
    };
    if raw_path.is_empty() {
        return None;
    }
    let bytes = raw_path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let h1 = (bytes[i + 1] as char).to_digit(16);
            let h2 = (bytes[i + 2] as char).to_digit(16);
            if let (Some(d1), Some(d2)) = (h1, h2) {
                decoded.push(((d1 << 4) | d2) as u8);
                i += 3;
                continue;
            }
        }
        decoded.push(bytes[i]);
        i += 1;
    }
    let s = String::from_utf8_lossy(&decoded).into_owned();
    Some(PathBuf::from(s))
}

/// Parses an RFC 2483 `text/uri-list` payload into a list of local `PathBuf`s.
pub fn parse_dropped_uri_list_to_paths(payload: &str) -> Vec<PathBuf> {
    payload
        .lines()
        .filter_map(decode_file_uri_to_path)
        .collect()
}
