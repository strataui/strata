//! Platform Event Model: Mouse, Keyboard, Touch, Window State, and File Drag-and-Drop.

use std::path::PathBuf;
use crate::window::ThemeMode;

/// Mouse button identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Other(u16),
}

/// Keyboard key codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ,
    KeyK, KeyL, KeyM, KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT,
    KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
    Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
    Escape, Enter, Tab, Space, Backspace, Delete,
    ArrowUp, ArrowDown, ArrowLeft, ArrowRight,
    Home, End, PageUp, PageDown,
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    Unknown,
}

/// Keyboard modifier keys active during an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
    WindowResized { width: u32, height: u32 },
    WindowMoved { x: i32, y: i32 },
    WindowCloseRequested,
    WindowFocused(bool),
    ScaleFactorChanged { scale_factor: f32 },
    ThemeChanged(ThemeMode),
    RedrawRequested,

    // Cursor / Mouse
    MouseMove { x: f32, y: f32 },
    MouseDown { button: MouseButton, x: f32, y: f32 },
    MouseUp { button: MouseButton, x: f32, y: f32 },
    Scroll { delta_x: f32, delta_y: f32, is_momentum: bool },
    CursorEntered,
    CursorLeft,

    // Keyboard
    KeyDown { key: KeyCode, modifiers: Modifiers },
    KeyUp { key: KeyCode, modifiers: Modifiers },
    ReceivedCharacter(char),

    // Multi-touch
    Touch { touch_id: u64, phase: TouchPhase, x: f32, y: f32 },

    // File Drag-and-Drop
    FileDrop(Vec<PathBuf>),
}
