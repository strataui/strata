//! ANSI / VT Terminal Input Event Parser, Mouse SGR 1006, and Bracketed Paste.

use std::io::{self, Read};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::raw_mode::get_terminal_size;

/// Modifier key flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
}

impl Modifiers {
    pub const NONE: Self = Self {
        shift: false,
        alt: false,
        ctrl: false,
    };
    pub const SHIFT: Self = Self {
        shift: true,
        alt: false,
        ctrl: false,
    };
    pub const ALT: Self = Self {
        shift: false,
        alt: true,
        ctrl: false,
    };
    pub const CTRL: Self = Self {
        shift: false,
        alt: false,
        ctrl: true,
    };
}

/// Keyboard key codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyCode {
    Char(char),
    Enter,
    Backspace,
    Tab,
    BackTab,
    Esc,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    F(u8),
    Null,
}

/// Key event details.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    pub code: KeyCode,
    pub modifiers: Modifiers,
}

impl KeyEvent {
    pub fn new(code: KeyCode, modifiers: Modifiers) -> Self {
        Self { code, modifiers }
    }

    pub fn char(ch: char) -> Self {
        Self {
            code: KeyCode::Char(ch),
            modifiers: Modifiers::NONE,
        }
    }
}

/// Mouse event action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseAction {
    Down,
    Up,
    Drag,
    Move,
    ScrollUp,
    ScrollDown,
    ScrollLeft,
    ScrollRight,
}

/// Mouse button identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    None,
}

/// Mouse event details with 0-based cell coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MouseEvent {
    pub button: MouseButton,
    pub action: MouseAction,
    pub col: u16,
    pub row: u16,
    pub modifiers: Modifiers,
}

/// Terminal input events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuiEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize(u16, u16),
    Paste(String),
    FocusGained,
    FocusLost,
}

impl TuiEvent {
    /// Translates terminal character cell coordinates and actions to a `strata_platform::PlatformEvent`.
    pub fn to_platform_event(
        &self,
        cell_width_px: f32,
        cell_height_px: f32,
    ) -> Option<strata_platform::PlatformEvent> {
        match self {
            Self::Mouse(m) => {
                let x = m.col as f32 * cell_width_px;
                let y = m.row as f32 * cell_height_px;
                let button = match m.button {
                    MouseButton::Left => strata_platform::MouseButton::Left,
                    MouseButton::Middle => strata_platform::MouseButton::Middle,
                    MouseButton::Right => strata_platform::MouseButton::Right,
                    MouseButton::None => strata_platform::MouseButton::Left,
                };

                match m.action {
                    MouseAction::Down => {
                        Some(strata_platform::PlatformEvent::MouseDown { button, x, y })
                    }
                    MouseAction::Up => {
                        Some(strata_platform::PlatformEvent::MouseUp { button, x, y })
                    }
                    MouseAction::Drag | MouseAction::Move => {
                        Some(strata_platform::PlatformEvent::MouseMove { x, y })
                    }
                    MouseAction::ScrollUp => Some(strata_platform::PlatformEvent::Scroll {
                        delta_x: 0.0,
                        delta_y: -1.0,
                        is_momentum: false,
                    }),
                    MouseAction::ScrollDown => Some(strata_platform::PlatformEvent::Scroll {
                        delta_x: 0.0,
                        delta_y: 1.0,
                        is_momentum: false,
                    }),
                    MouseAction::ScrollLeft => Some(strata_platform::PlatformEvent::Scroll {
                        delta_x: -1.0,
                        delta_y: 0.0,
                        is_momentum: false,
                    }),
                    MouseAction::ScrollRight => Some(strata_platform::PlatformEvent::Scroll {
                        delta_x: 1.0,
                        delta_y: 0.0,
                        is_momentum: false,
                    }),
                }
            }
            Self::Resize(cols, rows) => {
                Some(strata_platform::PlatformEvent::WindowResized {
                    width: *cols as u32,
                    height: *rows as u32,
                })
            }
            Self::FocusGained => Some(strata_platform::PlatformEvent::WindowFocused(true)),
            Self::FocusLost => Some(strata_platform::PlatformEvent::WindowFocused(false)),
            _ => None,
        }
    }
}

/// Asynchronous terminal event reader listening on standard input.
pub struct TuiEventReader {
    receiver: Receiver<TuiEvent>,
    _thread: Option<JoinHandle<()>>,
    last_size: (u16, u16),
}

impl TuiEventReader {
    /// Creates and spawns a background event reader reading from stdin.
    pub fn new() -> Self {
        let (sender, receiver) = channel();
        let initial_size = get_terminal_size();

        let thread_handle = thread::Builder::new()
            .name("strata-tui-input".to_string())
            .spawn(move || {
                run_input_reader_loop(sender);
            })
            .ok();

        Self {
            receiver,
            _thread: thread_handle,
            last_size: initial_size,
        }
    }

    /// Checks if a resize event occurred since last check and returns it if so.
    pub fn check_resize(&mut self) -> Option<TuiEvent> {
        let current_size = get_terminal_size();
        if current_size != self.last_size {
            self.last_size = current_size;
            Some(TuiEvent::Resize(current_size.0, current_size.1))
        } else {
            None
        }
    }

    /// Polls for an event with a timeout.
    pub fn poll_event(&mut self, timeout: Duration) -> Option<TuiEvent> {
        if let Some(resize) = self.check_resize() {
            return Some(resize);
        }

        match self.receiver.recv_timeout(timeout) {
            Ok(event) => Some(event),
            Err(_) => self.check_resize(),
        }
    }

    /// Tries to read an event without blocking.
    pub fn try_read_event(&mut self) -> Option<TuiEvent> {
        if let Some(resize) = self.check_resize() {
            return Some(resize);
        }

        self.receiver.try_recv().ok()
    }
}

impl Default for TuiEventReader {
    fn default() -> Self {
        Self::new()
    }
}

/// Background loop reading raw bytes from stdin and forwarding parsed `TuiEvent`s.
fn run_input_reader_loop(sender: Sender<TuiEvent>) {
    let mut stdin = io::stdin();
    let mut buffer = [0u8; 1024];
    let mut pending = Vec::new();

    loop {
        match stdin.read(&mut buffer) {
            Ok(0) => break, // EOF
            Ok(n) => {
                pending.extend_from_slice(&buffer[..n]);
                while !pending.is_empty() {
                    match parse_next_event(&pending) {
                        Some((event, consumed)) => {
                            pending.drain(..consumed);
                            if sender.send(event).is_err() {
                                return;
                            }
                        }
                        None => {
                            // Incomplete escape sequence - wait for more bytes,
                            // unless sequence is invalid or buffer is too long
                            if pending.len() > 64 || (!pending.is_empty() && pending[0] != 0x1b) {
                                // If not escape or malformed, drop first byte as individual char
                                let b = pending.remove(0);
                                if let Some(event) = parse_single_byte(b) {
                                    if sender.send(event).is_err() {
                                        return;
                                    }
                                }
                            } else {
                                break;
                            }
                        }
                    }
                }
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
}

/// Parses a single non-escape ASCII byte.
fn parse_single_byte(b: u8) -> Option<TuiEvent> {
    match b {
        0 => Some(TuiEvent::Key(KeyEvent::new(KeyCode::Null, Modifiers::NONE))),
        b'\t' => Some(TuiEvent::Key(KeyEvent::new(KeyCode::Tab, Modifiers::NONE))),
        b'\n' | b'\r' => Some(TuiEvent::Key(KeyEvent::new(KeyCode::Enter, Modifiers::NONE))),
        8 | 127 => Some(TuiEvent::Key(KeyEvent::new(KeyCode::Backspace, Modifiers::NONE))),
        27 => Some(TuiEvent::Key(KeyEvent::new(KeyCode::Esc, Modifiers::NONE))),
        1..=26 => {
            // Ctrl+A through Ctrl+Z (except Tab, LF, CR, BS handled above)
            let ch = (b'a' + (b - 1)) as char;
            Some(TuiEvent::Key(KeyEvent::new(KeyCode::Char(ch), Modifiers::CTRL)))
        }
        32..=126 => Some(TuiEvent::Key(KeyEvent::char(b as char))),
        _ => None,
    }
}

/// Parses the next complete `TuiEvent` from a byte slice.
/// Returns `Some((event, bytes_consumed))` or `None` if more bytes are needed.
pub fn parse_next_event(buf: &[u8]) -> Option<(TuiEvent, usize)> {
    if buf.is_empty() {
        return None;
    }

    if buf[0] != 0x1b {
        // Plain UTF-8 character or single key
        if (buf[0] & 0x80) == 0 {
            // ASCII
            return parse_single_byte(buf[0]).map(|ev| (ev, 1));
        }

        // Multi-byte UTF-8
        let utf8_len = match buf[0] {
            0xC0..=0xDF => 2,
            0xE0..=0xEF => 3,
            0xF0..=0xF7 => 4,
            _ => 1,
        };

        if buf.len() < utf8_len {
            return None; // Wait for full UTF-8 codepoint
        }

        if let Ok(s) = std::str::from_utf8(&buf[..utf8_len]) {
            if let Some(ch) = s.chars().next() {
                return Some((TuiEvent::Key(KeyEvent::char(ch)), utf8_len));
            }
        }
        return Some((TuiEvent::Key(KeyEvent::char('\u{FFFD}')), 1));
    }

    // Escape sequence (buf[0] == 0x1b)
    if buf.len() == 1 {
        // Just ESC
        return Some((TuiEvent::Key(KeyEvent::new(KeyCode::Esc, Modifiers::NONE)), 1));
    }

    match buf[1] {
        b'[' => parse_csi_sequence(&buf[2..]).map(|(ev, len)| (ev, len + 2)),
        b'O' => parse_ss3_sequence(&buf[2..]).map(|(ev, len)| (ev, len + 2)),
        _ => {
            // Alt + Key
            if let Some((TuiEvent::Key(mut k), len)) = parse_next_event(&buf[1..]) {
                k.modifiers.alt = true;
                Some((TuiEvent::Key(k), len + 1))
            } else {
                Some((TuiEvent::Key(KeyEvent::new(KeyCode::Esc, Modifiers::NONE)), 1))
            }
        }
    }
}

/// Parses SS3 sequences (e.g. `\x1bOP` for F1-F4).
fn parse_ss3_sequence(buf: &[u8]) -> Option<(TuiEvent, usize)> {
    if buf.is_empty() {
        return None;
    }
    match buf[0] {
        b'P' => Some((TuiEvent::Key(KeyEvent::new(KeyCode::F(1), Modifiers::NONE)), 1)),
        b'Q' => Some((TuiEvent::Key(KeyEvent::new(KeyCode::F(2), Modifiers::NONE)), 1)),
        b'R' => Some((TuiEvent::Key(KeyEvent::new(KeyCode::F(3), Modifiers::NONE)), 1)),
        b'S' => Some((TuiEvent::Key(KeyEvent::new(KeyCode::F(4), Modifiers::NONE)), 1)),
        _ => None,
    }
}

/// Parses Control Sequence Introducer (CSI) sequences (`\x1b[`).
fn parse_csi_sequence(buf: &[u8]) -> Option<(TuiEvent, usize)> {
    if buf.is_empty() {
        return None;
    }

    // Bracketed Paste: \x1b[200~<content>\x1b[201~
    if buf.starts_with(b"200~") {
        let content_start = 4;
        let paste_end_marker = b"\x1b[201~";
        if let Some(pos) = buf[content_start..]
            .windows(paste_end_marker.len())
            .position(|w| w == paste_end_marker)
        {
            let payload_bytes = &buf[content_start..content_start + pos];
            let payload = String::from_utf8_lossy(payload_bytes).into_owned();
            let total_len = content_start + pos + paste_end_marker.len();
            return Some((TuiEvent::Paste(payload), total_len));
        }
        return None; // Wait for end marker
    }

    // SGR 1006 Extended Mouse: \x1b[<Cb;Cx;CyM or \x1b[<Cb;Cx;Cym
    if buf.starts_with(b"<") {
        return parse_sgr_mouse(&buf[1..]).map(|(ev, len)| (ev, len + 1));
    }

    // Focus Tracking: \x1b[I (Focus In), \x1b[O (Focus Out)
    if buf[0] == b'I' {
        return Some((TuiEvent::FocusGained, 1));
    }
    if buf[0] == b'O' {
        return Some((TuiEvent::FocusLost, 1));
    }

    // Normal Mouse (X10/X11): \x1b[MCbCxCy
    if buf[0] == b'M' {
        if buf.len() < 4 {
            return None;
        }
        let cb = buf[1].saturating_sub(32);
        let cx = buf[2].saturating_sub(33) as u16;
        let cy = buf[3].saturating_sub(33) as u16;

        let (button, action) = match cb & 0b11 {
            0 => (MouseButton::Left, MouseAction::Down),
            1 => (MouseButton::Middle, MouseAction::Down),
            2 => (MouseButton::Right, MouseAction::Down),
            _ => (MouseButton::None, MouseAction::Up),
        };

        return Some((
            TuiEvent::Mouse(MouseEvent {
                button,
                action,
                col: cx,
                row: cy,
                modifiers: Modifiers::NONE,
            }),
            4,
        ));
    }

    // Arrows and Standard Navigation Keys
    match buf[0] {
        b'A' => return Some((TuiEvent::Key(KeyEvent::new(KeyCode::Up, Modifiers::NONE)), 1)),
        b'B' => return Some((TuiEvent::Key(KeyEvent::new(KeyCode::Down, Modifiers::NONE)), 1)),
        b'C' => return Some((TuiEvent::Key(KeyEvent::new(KeyCode::Right, Modifiers::NONE)), 1)),
        b'D' => return Some((TuiEvent::Key(KeyEvent::new(KeyCode::Left, Modifiers::NONE)), 1)),
        b'H' => return Some((TuiEvent::Key(KeyEvent::new(KeyCode::Home, Modifiers::NONE)), 1)),
        b'F' => return Some((TuiEvent::Key(KeyEvent::new(KeyCode::End, Modifiers::NONE)), 1)),
        b'Z' => return Some((TuiEvent::Key(KeyEvent::new(KeyCode::BackTab, Modifiers::SHIFT)), 1)),
        _ => {}
    }

    // Parametric sequences: digits, semicolons, terminated by letter or ~
    let term_idx = buf.iter().position(|&b| (0x40..=0x7E).contains(&b))?;
    let terminator = buf[term_idx];
    let params_str = std::str::from_utf8(&buf[..term_idx]).ok()?;
    let total_len = term_idx + 1;

    let parts: Vec<&str> = params_str.split(';').collect();

    // Check modifier code (e.g. \x1b[1;5A -> Ctrl+Up)
    let modifiers = if parts.len() > 1 {
        parse_modifier_code(parts[1].parse::<u8>().unwrap_or(1))
    } else {
        Modifiers::NONE
    };

    match terminator {
        b'A' => Some((TuiEvent::Key(KeyEvent::new(KeyCode::Up, modifiers)), total_len)),
        b'B' => Some((TuiEvent::Key(KeyEvent::new(KeyCode::Down, modifiers)), total_len)),
        b'C' => Some((TuiEvent::Key(KeyEvent::new(KeyCode::Right, modifiers)), total_len)),
        b'D' => Some((TuiEvent::Key(KeyEvent::new(KeyCode::Left, modifiers)), total_len)),
        b'H' => Some((TuiEvent::Key(KeyEvent::new(KeyCode::Home, modifiers)), total_len)),
        b'F' => Some((TuiEvent::Key(KeyEvent::new(KeyCode::End, modifiers)), total_len)),
        b'~' => {
            let num = parts[0].parse::<u8>().unwrap_or(0);
            let code = match num {
                1 | 7 => KeyCode::Home,
                2 => KeyCode::Insert,
                3 => KeyCode::Delete,
                4 | 8 => KeyCode::End,
                5 => KeyCode::PageUp,
                6 => KeyCode::PageDown,
                11..=15 => KeyCode::F(num - 10),
                17..=21 => KeyCode::F(num - 11),
                23..=24 => KeyCode::F(num - 12),
                _ => return None,
            };
            Some((TuiEvent::Key(KeyEvent::new(code, modifiers)), total_len))
        }
        _ => None,
    }
}

/// Parses modifiers from standard CSI modifier parameter (1 = none, 2 = shift, 3 = alt, 5 = ctrl, etc.).
fn parse_modifier_code(code: u8) -> Modifiers {
    let val = code.saturating_sub(1);
    Modifiers {
        shift: (val & 1) != 0,
        alt: (val & 2) != 0,
        ctrl: (val & 4) != 0,
    }
}

/// Parses SGR 1006 extended mouse format: `<Cb;Cx;CyM` (press/drag) or `<Cb;Cx;Cym` (release).
fn parse_sgr_mouse(buf: &[u8]) -> Option<(TuiEvent, usize)> {
    let term_idx = buf.iter().position(|&b| b == b'M' || b == b'm')?;
    let is_release = buf[term_idx] == b'm';
    let s = std::str::from_utf8(&buf[..term_idx]).ok()?;
    let parts: Vec<&str> = s.split(';').collect();

    if parts.len() != 3 {
        return None;
    }

    let cb = parts[0].parse::<u16>().ok()?;
    let cx = parts[1].parse::<u16>().ok()?.saturating_sub(1); // 1-based to 0-based
    let cy = parts[2].parse::<u16>().ok()?.saturating_sub(1);

    let modifiers = Modifiers {
        shift: (cb & 4) != 0,
        alt: (cb & 8) != 0,
        ctrl: (cb & 16) != 0,
    };

    let is_motion = (cb & 32) != 0;
    let is_wheel = (cb & 64) != 0;

    let (button, action) = if is_wheel {
        let action = match cb & 0b11 {
            0 => MouseAction::ScrollUp,
            1 => MouseAction::ScrollDown,
            2 => MouseAction::ScrollLeft,
            _ => MouseAction::ScrollRight,
        };
        (MouseButton::None, action)
    } else if is_release {
        (MouseButton::None, MouseAction::Up)
    } else if is_motion {
        let button = match cb & 0b11 {
            0 => MouseButton::Left,
            1 => MouseButton::Middle,
            2 => MouseButton::Right,
            _ => MouseButton::None,
        };
        let action = if button == MouseButton::None {
            MouseAction::Move
        } else {
            MouseAction::Drag
        };
        (button, action)
    } else {
        let button = match cb & 0b11 {
            0 => MouseButton::Left,
            1 => MouseButton::Middle,
            2 => MouseButton::Right,
            _ => MouseButton::None,
        };
        (button, MouseAction::Down)
    };

    Some((
        TuiEvent::Mouse(MouseEvent {
            button,
            action,
            col: cx,
            row: cy,
            modifiers,
        }),
        term_idx + 1,
    ))
}
