//! Universal cross-platform keyboard shortcut and keybinding manager.
//!
//! Provides declarative shortcut registration (e.g., "Ctrl+Shift+I", "F12", "Alt+Left"),
//! modifier bitmask matching, and priority event dispatching across desktop platforms.

use crate::events::{KeyCode, Modifiers, PlatformEvent};

/// Normalized keyboard shortcut trigger combining a key code and required modifier states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShortcutTrigger {
    pub key: KeyCode,
    pub modifiers: Modifiers,
}

impl ShortcutTrigger {
    /// Creates a new trigger with explicit key and modifier flags.
    pub const fn new(key: KeyCode, modifiers: Modifiers) -> Self {
        Self { key, modifiers }
    }

    /// Creates a trigger with no modifier keys active.
    pub const fn plain(key: KeyCode) -> Self {
        Self {
            key,
            modifiers: Modifiers {
                shift: false,
                ctrl: false,
                alt: false,
                meta: false,
            },
        }
    }

    /// Parses a human-readable shortcut string (e.g., "Ctrl+Shift+I", "F12", "Alt+Left").
    pub fn parse(spec: &str) -> Option<Self> {
        let mut ctrl = false;
        let mut shift = false;
        let mut alt = false;
        let mut meta = false;
        let mut target_key = None;

        for part in spec.split('+') {
            let trimmed = part.trim();
            match trimmed.to_lowercase().as_str() {
                "ctrl" | "control" => ctrl = true,
                "shift" => shift = true,
                "alt" | "opt" | "option" => alt = true,
                "meta" | "cmd" | "command" | "super" | "win" => meta = true,
                _ => {
                    target_key = parse_key_code(trimmed);
                }
            }
        }

        target_key.map(|key| Self {
            key,
            modifiers: Modifiers {
                shift,
                ctrl,
                alt,
                meta,
            },
        })
    }

    /// Tests if this trigger matches a given platform key and modifier state.
    pub fn matches(&self, key: KeyCode, modifiers: Modifiers) -> bool {
        self.key == key
            && self.modifiers.ctrl == modifiers.ctrl
            && self.modifiers.shift == modifiers.shift
            && self.modifiers.alt == modifiers.alt
            && self.modifiers.meta == modifiers.meta
    }
}

/// Parses a string representation of a key name into a `KeyCode`.
fn parse_key_code(name: &str) -> Option<KeyCode> {
    match name.to_uppercase().as_str() {
        "A" => Some(KeyCode::KeyA),
        "B" => Some(KeyCode::KeyB),
        "C" => Some(KeyCode::KeyC),
        "D" => Some(KeyCode::KeyD),
        "E" => Some(KeyCode::KeyE),
        "F" => Some(KeyCode::KeyF),
        "G" => Some(KeyCode::KeyG),
        "H" => Some(KeyCode::KeyH),
        "I" => Some(KeyCode::KeyI),
        "J" => Some(KeyCode::KeyJ),
        "K" => Some(KeyCode::KeyK),
        "L" => Some(KeyCode::KeyL),
        "M" => Some(KeyCode::KeyM),
        "N" => Some(KeyCode::KeyN),
        "O" => Some(KeyCode::KeyO),
        "P" => Some(KeyCode::KeyP),
        "Q" => Some(KeyCode::KeyQ),
        "R" => Some(KeyCode::KeyR),
        "S" => Some(KeyCode::KeyS),
        "T" => Some(KeyCode::KeyT),
        "U" => Some(KeyCode::KeyU),
        "V" => Some(KeyCode::KeyV),
        "W" => Some(KeyCode::KeyW),
        "X" => Some(KeyCode::KeyX),
        "Y" => Some(KeyCode::KeyY),
        "Z" => Some(KeyCode::KeyZ),
        "0" => Some(KeyCode::Digit0),
        "1" => Some(KeyCode::Digit1),
        "2" => Some(KeyCode::Digit2),
        "3" => Some(KeyCode::Digit3),
        "4" => Some(KeyCode::Digit4),
        "5" => Some(KeyCode::Digit5),
        "6" => Some(KeyCode::Digit6),
        "7" => Some(KeyCode::Digit7),
        "8" => Some(KeyCode::Digit8),
        "9" => Some(KeyCode::Digit9),
        "F1" => Some(KeyCode::F1),
        "F2" => Some(KeyCode::F2),
        "F3" => Some(KeyCode::F3),
        "F4" => Some(KeyCode::F4),
        "F5" => Some(KeyCode::F5),
        "F6" => Some(KeyCode::F6),
        "F7" => Some(KeyCode::F7),
        "F8" => Some(KeyCode::F8),
        "F9" => Some(KeyCode::F9),
        "F10" => Some(KeyCode::F10),
        "F11" => Some(KeyCode::F11),
        "F12" => Some(KeyCode::F12),
        "ESC" | "ESCAPE" => Some(KeyCode::Escape),
        "ENTER" | "RETURN" => Some(KeyCode::Enter),
        "TAB" => Some(KeyCode::Tab),
        "SPACE" => Some(KeyCode::Space),
        "BACKSPACE" => Some(KeyCode::Backspace),
        "DELETE" | "DEL" => Some(KeyCode::Delete),
        "LEFT" | "ARROWLEFT" => Some(KeyCode::ArrowLeft),
        "RIGHT" | "ARROWRIGHT" => Some(KeyCode::ArrowRight),
        "UP" | "ARROWUP" => Some(KeyCode::ArrowUp),
        "DOWN" | "ARROWDOWN" => Some(KeyCode::ArrowDown),
        "HOME" => Some(KeyCode::Home),
        "END" => Some(KeyCode::End),
        "PAGEUP" | "PGUP" => Some(KeyCode::PageUp),
        "PAGEDOWN" | "PGDN" => Some(KeyCode::PageDown),
        _ => None,
    }
}

/// Action closure executed when a registered shortcut triggers.
/// Returns `true` if the shortcut was consumed and requires a UI repaint.
pub type ShortcutCallback = Box<dyn Fn() -> bool>;

/// Registry managing active keyboard shortcuts and dispatching incoming key events.
#[derive(Default)]
pub struct ShortcutManager {
    bindings: Vec<(ShortcutTrigger, ShortcutCallback)>,
}

impl ShortcutManager {
    /// Creates an empty shortcut manager.
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }

    /// Registers a shortcut by spec string (e.g. "Ctrl+Shift+I", "F12").
    /// Returns `true` if the shortcut string was parsed and successfully registered.
    pub fn register(&mut self, spec: &str, action: impl Fn() -> bool + 'static) -> bool {
        if let Some(trigger) = ShortcutTrigger::parse(spec) {
            self.bindings.push((trigger, Box::new(action)));
            true
        } else {
            false
        }
    }

    /// Registers a shortcut with a pre-constructed `ShortcutTrigger`.
    pub fn register_trigger(&mut self, trigger: ShortcutTrigger, action: impl Fn() -> bool + 'static) {
        self.bindings.push((trigger, Box::new(action)));
    }

    /// Dispatches an incoming `PlatformEvent`. If it matches a registered shortcut,
    /// invokes the associated action and returns its repaint status.
    pub fn dispatch(&self, event: &PlatformEvent) -> bool {
        if let PlatformEvent::KeyDown { key, modifiers } = event {
            for (trigger, action) in &self.bindings {
                if trigger.matches(*key, *modifiers) {
                    return action();
                }
            }
        }
        false
    }

    /// Returns the number of registered shortcut bindings.
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// Checks if the manager contains any registered shortcuts.
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }
}
