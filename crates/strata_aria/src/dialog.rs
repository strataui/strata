//! Accessible Dialog / Modal State Machine with ARIA Semantic Attributes.

use strata_dom::Signal;

/// Configuration for an accessible dialog.
#[derive(Debug, Clone)]
pub struct DialogConfig {
    pub id: String,
    pub close_on_escape: bool,
    pub close_on_outside_click: bool,
}

impl Default for DialogConfig {
    fn default() -> Self {
        Self {
            id: "strata-dialog".to_string(),
            close_on_escape: true,
            close_on_outside_click: true,
        }
    }
}

/// Headless dialog state manager.
#[derive(Clone)]
pub struct DialogState {
    pub open: Signal<bool>,
    pub id: String,
    pub title_id: String,
    pub description_id: String,
    pub close_on_escape: bool,
    pub close_on_outside_click: bool,
}

impl DialogState {
    /// Creates a new dialog state with the provided configuration.
    pub fn new(config: DialogConfig) -> Self {
        let title_id = format!("{}-title", config.id);
        let description_id = format!("{}-desc", config.id);
        Self {
            open: Signal::new(false),
            id: config.id,
            title_id,
            description_id,
            close_on_escape: config.close_on_escape,
            close_on_outside_click: config.close_on_outside_click,
        }
    }

    /// Open the dialog.
    pub fn open_dialog(&self) {
        self.open.set(true);
    }

    /// Close the dialog.
    pub fn close_dialog(&self) {
        self.open.set(false);
    }

    /// Toggle open state.
    pub fn toggle(&self) {
        self.open.set(!self.open.get());
    }

    /// Handle Escape key press. Returns true if dismiss was triggered.
    pub fn handle_key(&self, key: &str) -> bool {
        if self.open.get() && self.close_on_escape && key == "Escape" {
            self.close_dialog();
            return true;
        }
        false
    }

    /// Handle click outside dialog bounds. Returns true if dismiss was triggered.
    pub fn handle_outside_click(&self) -> bool {
        if self.open.get() && self.close_on_outside_click {
            self.close_dialog();
            return true;
        }
        false
    }

    /// Generates ARIA attributes for the dialog container element.
    pub fn container_attrs(&self) -> [(&'static str, String); 4] {
        [
            ("role", "dialog".to_string()),
            ("aria-modal", "true".to_string()),
            ("aria-labelledby", self.title_id.clone()),
            ("aria-describedby", self.description_id.clone()),
        ]
    }

    /// Generates ARIA attributes for the trigger button that toggles this dialog.
    pub fn trigger_attrs(&self) -> [(&'static str, String); 3] {
        [
            ("aria-haspopup", "dialog".to_string()),
            ("aria-expanded", self.open.get().to_string()),
            ("aria-controls", self.id.clone()),
        ]
    }
}

/// Convenience hook to initialize a headless dialog state.
pub fn use_dialog(config: DialogConfig) -> DialogState {
    DialogState::new(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dialog_lifecycle() {
        let dialog = use_dialog(DialogConfig {
            id: "confirm-modal".to_string(),
            close_on_escape: true,
            close_on_outside_click: true,
        });

        assert!(!dialog.open.get());
        dialog.open_dialog();
        assert!(dialog.open.get());

        // Escape key dismisses
        assert!(dialog.handle_key("Escape"));
        assert!(!dialog.open.get());

        // Outside click dismisses
        dialog.open_dialog();
        assert!(dialog.handle_outside_click());
        assert!(!dialog.open.get());
    }
}
