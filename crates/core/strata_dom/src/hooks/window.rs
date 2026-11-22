//! Window, cursor, keyboard shortcut, and clipboard control hooks.

use crate::reactive::Signal;
use std::cell::RefCell;
use std::rc::Rc;

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

type ShortcutEntry = (KeyChord, Rc<dyn Fn()>);
type CursorEntry = (CursorIcon, Signal<bool>);

thread_local! {
    static MANUAL_CURSOR: RefCell<Signal<Option<CursorIcon>>> = RefCell::new(Signal::new(None));
    static CURSOR_REQUESTS: RefCell<Vec<(u32, CursorEntry)>> = const { RefCell::new(Vec::new()) };
    static REGISTERED_SHORTCUTS: RefCell<Vec<(u32, ShortcutEntry)>> = RefCell::new(Vec::new());
    static NEXT_SHORTCUT_ID: RefCell<u32> = const { RefCell::new(1) };
    static CLIPBOARD_CONTENT: RefCell<Signal<String>> = RefCell::new(Signal::new(String::new()));
}

/// Returns the currently active pointer cursor icon, taking into account manual overrides and reactive hooks.
pub fn current_cursor() -> CursorIcon {
    if let Some(manual) = MANUAL_CURSOR.with(|c| c.borrow().get()) {
        return manual;
    }

    CURSOR_REQUESTS.with(|reqs| {
        for (_, (icon, active)) in reqs.borrow().iter().rev() {
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
///
/// Inside a `#[component]` render body, the cursor request is persisted in a component hook slot,
/// updated in place on re-renders (preventing duplicate `CURSOR_REQUESTS` entries), and automatically
/// removed when the component unmounts. Outside a component scope or inside an effect, registers an
/// unscoped reactive cursor entry.
pub fn use_cursor(icon: CursorIcon, active: Signal<bool>) {
    let entry: CursorEntry = (icon, active);
    if crate::reactive::in_component_hook_context() {
        let _ = upsert_component_entry(&CURSOR_REQUESTS, entry);
    } else {
        let id = alloc_registry_id();
        CURSOR_REQUESTS.with(|reqs| {
            reqs.borrow_mut().push((id, entry));
        });
    }
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
    fn normalize_key_token(raw: &str) -> String {
        if raw == " " {
            return "space".to_string();
        }
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "space" | "spacebar" if raw == " " || raw.eq_ignore_ascii_case("space") || raw.eq_ignore_ascii_case("spacebar") => {
                "space".to_string()
            }
            "esc" | "escape" => "escape".to_string(),
            "return" | "enter" => "enter".to_string(),
            other => other.to_string(),
        }
    }

    /// Parses a string like "Ctrl+Shift+P", "Cmd+K", or "Escape".
    pub fn parse(s: &str) -> Self {
        let mut ctrl = false;
        let mut shift = false;
        let mut alt = false;
        let mut meta = false;
        let mut key = String::new();

        for part in s.split('+') {
            if part == " " {
                key = "space".to_string();
                continue;
            }
            let p = part.trim();
            match p.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => ctrl = true,
                "shift" => shift = true,
                "alt" | "option" => alt = true,
                "cmd" | "command" | "meta" | "super" => meta = true,
                other => key = Self::normalize_key_token(other),
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
        let incoming = Self::normalize_key_token(key);
        self.key.eq_ignore_ascii_case(&incoming)
            && self.ctrl == ctrl
            && self.shift == shift
            && self.alt == alt
            && self.meta == meta
    }
}

struct ScopedEntryGuard<T: 'static> {
    id: u32,
    registry: &'static std::thread::LocalKey<RefCell<Vec<(u32, T)>>>,
}

impl<T: 'static> Drop for ScopedEntryGuard<T> {
    fn drop(&mut self) {
        let _ = self.registry.try_with(|s| {
            if let Ok(mut list) = s.try_borrow_mut() {
                list.retain(|(id, _)| *id != self.id);
            }
        });
    }
}

struct ComponentEntrySlot<T: 'static> {
    id: u32,
    guard: Rc<ScopedEntryGuard<T>>,
}

impl<T: 'static> Clone for ComponentEntrySlot<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            guard: Rc::clone(&self.guard),
        }
    }
}

fn alloc_registry_id() -> u32 {
    NEXT_SHORTCUT_ID.with(|n| {
        let mut val = n.borrow_mut();
        let cur = *val;
        *val += 1;
        cur
    })
}

fn push_unscoped_entry<T: 'static>(
    registry: &'static std::thread::LocalKey<RefCell<Vec<(u32, T)>>>,
    value: T,
) -> (u32, ScopedEntryGuard<T>) {
    let id = alloc_registry_id();
    registry.with(|s| {
        s.borrow_mut().push((id, value));
    });
    (id, ScopedEntryGuard { id, registry })
}

fn upsert_component_entry<T: 'static>(
    registry: &'static std::thread::LocalKey<RefCell<Vec<(u32, T)>>>,
    value: T,
) -> Rc<ScopedEntryGuard<T>> {
    let mut init_val = Some(value);
    let slot = crate::reactive::use_hook_slot_cloned(|| {
        let initial = init_val.take().unwrap();
        let (id, guard) = push_unscoped_entry(registry, initial);
        ComponentEntrySlot {
            id,
            guard: Rc::new(guard),
        }
    })
    .expect("component hook context active");

    if let Some(latest) = init_val.take() {
        registry.with(|s| {
            let mut list = s.borrow_mut();
            if let Some(entry) = list.iter_mut().find(|(id, _)| *id == slot.id) {
                entry.1 = latest;
            } else {
                list.push((slot.id, latest));
            }
        });
    }

    Rc::clone(&slot.guard)
}

/// Handle returned by [`use_keyboard_shortcut`].
///
/// Outside a `#[component]` scope, dropping the handle unregisters the shortcut immediately.
/// Inside a `#[component]` body, lifetime is bound to the component hook slot so dropping the
/// handle at the end of `render` keeps the shortcut active until component unmount.
pub struct ShortcutHandle {
    _guard: Rc<ScopedEntryGuard<ShortcutEntry>>,
}

/// Registers a global keyboard shortcut with an invoked callback.
///
/// Inside a `#[component]` render body, the shortcut is persisted in the component's hook slot,
/// updated in place on re-renders (preventing duplicate registrations), and unregistered
/// automatically when the component unmounts. Outside a component scope or inside an effect,
/// returns an RAII [`ShortcutHandle`] whose `Drop` unregisters immediately.
pub fn use_keyboard_shortcut<F: Fn() + 'static>(shortcut: &str, handler: F) -> ShortcutHandle {
    let entry: ShortcutEntry = (KeyChord::parse(shortcut), Rc::new(handler));
    let guard = if crate::reactive::in_component_hook_context() {
        upsert_component_entry(&REGISTERED_SHORTCUTS, entry)
    } else {
        Rc::new(push_unscoped_entry(&REGISTERED_SHORTCUTS, entry).1)
    };
    ShortcutHandle { _guard: guard }
}

/// Component-ergonomic alias for [`use_keyboard_shortcut`] discarding the [`ShortcutHandle`].
///
/// Registers the shortcut on mount, refreshes the closure in place on re-render, and cleans up
/// on component unmount.
pub fn use_shortcut<F: Fn() + 'static>(shortcut: &str, handler: F) {
    let _ = use_keyboard_shortcut(shortcut, handler);
}

/// Dispatches a keydown event through registered shortcuts.
/// Returns true if any registered shortcut consumed the key event.
pub fn dispatch_global_keydown(key: &str, ctrl: bool, shift: bool, alt: bool, meta: bool) -> bool {
    let mut handled = false;
    REGISTERED_SHORTCUTS.with(|s| {
        let list = s.borrow().clone();
        for (_, (chord, callback)) in list {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reactive::{
        reset_component_hooks, with_root_component_scope, ComponentScopeGuard,
    };

    #[test]
    fn test_component_use_cursor_does_not_leak_cursor_requests_across_rerenders_or_unmount() {
        reset_component_hooks();
        CURSOR_REQUESTS.with(|reqs| reqs.borrow_mut().clear());
        clear_cursor_override();

        let sig = Signal::new(true);
        let render = |icon: CursorIcon| {
            let sig_c = sig.clone();
            with_root_component_scope(|| {
                let _comp = ComponentScopeGuard::enter("CursorOwner");
                use_cursor(icon, sig_c);
            });
        };

        for _ in 0..25 {
            render(CursorIcon::Grab);
        }
        assert_eq!(CURSOR_REQUESTS.with(|r| r.borrow().len()), 1);
        assert_eq!(current_cursor(), CursorIcon::Grab);

        // Updating icon updates the single slot in place without pushing a second entry
        render(CursorIcon::ColResize);
        assert_eq!(CURSOR_REQUESTS.with(|r| r.borrow().len()), 1);
        assert_eq!(current_cursor(), CursorIcon::ColResize);

        // Unmounting clears the entry completely
        reset_component_hooks();
        assert_eq!(CURSOR_REQUESTS.with(|r| r.borrow().len()), 0);
        assert_eq!(current_cursor(), CursorIcon::Default);
    }
}

