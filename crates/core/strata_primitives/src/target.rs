//! Runtime Target State (e.g. GUI vs TUI).

use std::sync::atomic::{AtomicBool, Ordering};

static TUI_TARGET_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Checks if the current runtime target is TUI.
pub fn is_tui_target() -> bool {
    TUI_TARGET_ACTIVE.load(Ordering::SeqCst)
}

/// Sets whether the TUI target is active (useful for testing and dual-mode execution).
pub fn set_tui_target_active(active: bool) {
    TUI_TARGET_ACTIVE.store(active, Ordering::SeqCst);
}
