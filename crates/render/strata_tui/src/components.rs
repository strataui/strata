//! Conditional TUI RSX Components (<TuiFallback>, <NoTui>, <TuiOnly>).

use strata_dom::{NodeBlueprint, View};

pub use strata_primitives::target::{is_tui_target, set_tui_target_active};


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
