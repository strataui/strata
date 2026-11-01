//! Conditional TUI RSX Components (<TuiFallback>, <NoTui>, <TuiOnly>).

use std::sync::atomic::{AtomicBool, Ordering};
use strata_dom::{NodeBlueprint, View};

static TUI_TARGET_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Checks if the current runtime target is TUI.
pub fn is_tui_target() -> bool {
    TUI_TARGET_ACTIVE.load(Ordering::SeqCst)
}

/// Sets whether the TUI target is active (useful for testing and dual-mode execution).
pub fn set_tui_target_active(active: bool) {
    TUI_TARGET_ACTIVE.store(active, Ordering::SeqCst);
}

/// A component providing declarative fallback between rich GUI surfaces and terminal representations.
pub fn tui_fallback(gui: View, tui: View) -> View {
    if is_tui_target() {
        tui
    } else {
        gui
    }
}

/// Renders children only when running on a non-TUI target (omitted on text terminals).
pub fn no_tui(children: Vec<NodeBlueprint>) -> View {
    if is_tui_target() {
        View::new(NodeBlueprint::Empty)
    } else {
        View::new(NodeBlueprint::Fragment(children))
    }
}

/// Renders children strictly on the TUI target.
pub fn tui_only(children: Vec<NodeBlueprint>) -> View {
    if is_tui_target() {
        View::new(NodeBlueprint::Fragment(children))
    } else {
        View::new(NodeBlueprint::Empty)
    }
}
