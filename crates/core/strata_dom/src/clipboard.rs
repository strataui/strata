//! Cycle-free cross-crate clipboard bridge with thread-local fallback store.
//!
//! Allows higher-level UI widgets (such as `strata-editor`) and core `<input>`/`<textarea>`
//! editing commands to share a unified OS clipboard without depending directly on
//! `strata-ui-platform`. Native runtimes register an OS clipboard provider via
//! [`register_system_clipboard_bridge`]; headless tests and fallback environments
//! seamlessly use the built-in thread-local clipboard store.

use std::cell::RefCell;
use std::sync::{OnceLock, RwLock};

type ClipboardReadFn = fn() -> Option<String>;
type ClipboardWriteFn = fn(&str);

struct SystemClipboardBridge {
    read: ClipboardReadFn,
    write: ClipboardWriteFn,
}

static SYSTEM_BRIDGE: OnceLock<RwLock<Option<SystemClipboardBridge>>> = OnceLock::new();

thread_local! {
    static FALLBACK_CLIPBOARD: RefCell<String> = const { RefCell::new(String::new()) };
}

fn bridge_lock() -> &'static RwLock<Option<SystemClipboardBridge>> {
    SYSTEM_BRIDGE.get_or_init(|| RwLock::new(None))
}

/// Registers process-wide OS clipboard read/write callbacks (invoked by `strata-ui`).
pub fn register_system_clipboard_bridge(read: ClipboardReadFn, write: ClipboardWriteFn) {
    if let Ok(mut guard) = bridge_lock().write() {
        *guard = Some(SystemClipboardBridge { read, write });
    }
}

/// Clears any registered OS clipboard provider (useful in isolated tests).
pub fn clear_system_clipboard_bridge() {
    if let Ok(mut guard) = bridge_lock().write() {
        *guard = None;
    }
}

/// Writes UTF-8 text to the OS clipboard (if bridged) and always mirrors it to the
/// thread-local fallback store.
pub fn set_clipboard_text(text: &str) {
    FALLBACK_CLIPBOARD.with(|cb| {
        let mut slot = cb.borrow_mut();
        slot.clear();
        slot.push_str(text);
    });
    let write_fn = bridge_lock()
        .read()
        .ok()
        .and_then(|g| g.as_ref().map(|b| b.write));
    if let Some(write) = write_fn {
        write(text);
    }
}

/// Reads UTF-8 text from the OS clipboard (if bridged), falling back to the
/// thread-local clipboard store when headless or empty.
pub fn get_clipboard_text() -> Option<String> {
    let read_fn = bridge_lock()
        .read()
        .ok()
        .and_then(|g| g.as_ref().map(|b| b.read));
    if let Some(read) = read_fn {
        if let Some(os_text) = read() {
            if !os_text.is_empty() {
                return Some(os_text);
            }
        }
    }
    FALLBACK_CLIPBOARD.with(|cb| {
        let s = cb.borrow();
        if s.is_empty() {
            None
        } else {
            Some(s.clone())
        }
    })
}
