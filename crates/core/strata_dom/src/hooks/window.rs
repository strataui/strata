//! Window, cursor, keyboard shortcut, and clipboard control hooks.

use std::cell::RefCell;
use std::rc::Rc;
use crate::reactive::Signal;

/// Standard pointer cursor icons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorIcon {
    #[default]
    Default,
    Pointer,
    Text,
    Move,
    NotAllowed,
    Grab,
    Grabbing,
    Crosshair,
    ColResize,
    RowResize,
}

type ShortcutEntry = (u32, KeyChord, Rc<dyn Fn()>);

thread_local! {
    static MANUAL_CURSOR: RefCell<Signal<Option<CursorIcon>>> = RefCell::new(Signal::new(None));
    static CURSOR_REQUESTS: RefCell<Vec<(CursorIcon, Signal<bool>)>> = const { RefCell::new(Vec::new()) };
    static REGISTERED_SHORTCUTS: RefCell<Vec<ShortcutEntry>> = RefCell::new(Vec::new());
    static NEXT_SHORTCUT_ID: RefCell<u32> = const { RefCell::new(1) };
    static CLIPBOARD_CONTENT: RefCell<Signal<String>> = RefCell::new(Signal::new(String::new()));
}

/// Returns the currently active pointer cursor icon, taking into account manual overrides and reactive hooks.
pub fn current_cursor() -> CursorIcon {
    if let Some(manual) = MANUAL_CURSOR.with(|c| c.borrow().get()) {
        return manual;
    }

    CURSOR_REQUESTS.with(|reqs| {
        for (icon, active) in reqs.borrow().iter().rev() {
            if active.get() {
                return *icon;
            }
        }
        CursorIcon::Default
    })
}

/// Sets the current pointer cursor icon directly as an explicit override.
pub fn set_current_cursor(icon: CursorIcon) {
    MANUAL_CURSOR.with(|c| c.borrow().set(Some(icon)));
}

/// Clears any manual pointer cursor override, returning control to reactive hooks.
pub fn clear_cursor_override() {
    MANUAL_CURSOR.with(|c| c.borrow().set(None));
}

/// Reactive hook that activates the specified pointer cursor whenever `active` signal evaluates to true.
pub fn use_cursor(icon: CursorIcon, active: Signal<bool>) {
    CURSOR_REQUESTS.with(|reqs| {
        reqs.borrow_mut().push((icon, active));
    });
}

/// Representation of a parsed keyboard key chord.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyChord {
    pub key: String,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
}

impl KeyChord {
    /// Parses a string like "Ctrl+Shift+P", "Cmd+K", or "Escape".
    pub fn parse(s: &str) -> Self {
        let mut ctrl = false;
        let mut shift = false;
        let mut alt = false;
        let mut meta = false;
        let mut key = String::new();

        for part in s.split('+') {
            let p = part.trim();
            match p.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => ctrl = true,
                "shift" => shift = true,
                "alt" | "option" => alt = true,
                "cmd" | "command" | "meta" | "super" => meta = true,
                other => key = other.to_ascii_lowercase(),
            }
        }

        Self {
            key,
            ctrl,
            shift,
            alt,
            meta,
        }
    }

    /// Evaluates if an incoming keystroke matches this key chord.
    pub fn matches(&self, key: &str, ctrl: bool, shift: bool, alt: bool, meta: bool) -> bool {
        self.key.eq_ignore_ascii_case(key)
            && self.ctrl == ctrl
            && self.shift == shift
            && self.alt == alt
            && self.meta == meta
    }
}

/// Handle returned by `use_keyboard_shortcut` that unregisters the shortcut on drop.
pub struct ShortcutHandle {
    id: u32,
}

impl Drop for ShortcutHandle {
    fn drop(&mut self) {
        REGISTERED_SHORTCUTS.with(|s| {
            s.borrow_mut().retain(|(id, _, _)| *id != self.id);
        });
    }
}

/// Registers a global keyboard shortcut with an invoked callback.
pub fn use_keyboard_shortcut<F: Fn() + 'static>(shortcut: &str, handler: F) -> ShortcutHandle {
    let chord = KeyChord::parse(shortcut);
    let id = NEXT_SHORTCUT_ID.with(|n| {
        let mut val = n.borrow_mut();
        let cur = *val;
        *val += 1;
        cur
    });

    let callback = Rc::new(handler);
    REGISTERED_SHORTCUTS.with(|s| {
        s.borrow_mut().push((id, chord, callback));
    });

    ShortcutHandle { id }
}

/// Dispatches a keydown event through registered shortcuts.
/// Returns true if any registered shortcut consumed the key event.
pub fn dispatch_global_keydown(key: &str, ctrl: bool, shift: bool, alt: bool, meta: bool) -> bool {
    let mut handled = false;
    REGISTERED_SHORTCUTS.with(|s| {
        let list = s.borrow().clone();
        for (_, chord, callback) in list {
            if chord.matches(key, ctrl, shift, alt, meta) {
                callback();
                handled = true;
                break;
            }
        }
    });
    handled
}

/// Handle returned by `use_clipboard()` providing reactive clipboard operations.
#[derive(Clone)]
pub struct ClipboardHandle {
    pub text: Signal<String>,
    pub copy: Rc<dyn Fn(&str)>,
    pub read_text: Rc<dyn Fn() -> String>,
}

/// Headless hook providing reactive clipboard reading and writing.
pub fn use_clipboard() -> ClipboardHandle {
    let text_sig = CLIPBOARD_CONTENT.with(|c| c.borrow().clone());

    let copy = {
        let sig = text_sig.clone();
        Rc::new(move |val: &str| {
            sig.set(val.to_string());
        })
    };

    let read_text = {
        let sig = text_sig.clone();
        Rc::new(move || sig.get())
    };

    ClipboardHandle {
        text: text_sig,
        copy,
        read_text,
    }
}
