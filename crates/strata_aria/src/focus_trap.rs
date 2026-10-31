//! Focus Trap Primitives for Accessible Modals, Dialogs, and Drawers.

use strata_dom::Signal;

/// Traps keyboard tab navigation within an isolated DOM tree or focusable node list.
#[derive(Clone)]
pub struct FocusTrap {
    pub active: Signal<bool>,
    elements: Vec<String>,
    current_index: Signal<usize>,
    previous_focused_id: Option<String>,
}

impl FocusTrap {
    /// Creates a new focus trap initialized with a list of focusable element IDs.
    pub fn new(elements: Vec<String>) -> Self {
        Self {
            active: Signal::new(false),
            elements,
            current_index: Signal::new(0),
            previous_focused_id: None,
        }
    }

    /// Activates the trap, recording the previously focused element ID for restoration on exit.
    pub fn activate(&mut self, previously_focused: Option<String>) {
        self.previous_focused_id = previously_focused;
        self.current_index.set(0);
        self.active.set(true);
    }

    /// Deactivates the trap, returning the previously focused element ID to restore focus.
    pub fn deactivate(&mut self) -> Option<String> {
        self.active.set(false);
        self.previous_focused_id.take()
    }

    /// Updates the list of focusable elements within the trap.
    pub fn set_elements(&mut self, elements: Vec<String>) {
        self.elements = elements;
        if self.current_index.get() >= self.elements.len() && !self.elements.is_empty() {
            self.current_index.set(0);
        }
    }

    /// Gets the ID of the currently focused element inside the trap.
    pub fn current_target(&self) -> Option<String> {
        if !self.active.get() || self.elements.is_empty() {
            return None;
        }
        self.elements.get(self.current_index.get()).cloned()
    }

    /// Handles keyboard events (Tab / Shift+Tab) to keep focus trapped.
    /// Returns the target element ID that should receive focus next.
    pub fn handle_key(&self, key: &str, shift: bool) -> Option<String> {
        if !self.active.get() || self.elements.is_empty() {
            return None;
        }

        let total = self.elements.len();
        let cur = self.current_index.get();

        if key == "Tab" {
            let next_idx = if shift {
                if cur == 0 {
                    total - 1
                } else {
                    cur - 1
                }
            } else {
                (cur + 1) % total
            };

            self.current_index.set(next_idx);
            return self.elements.get(next_idx).cloned();
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_focus_trap_cycle() {
        let items = vec!["input-name".to_string(), "input-email".to_string(), "btn-submit".to_string()];
        let mut trap = FocusTrap::new(items);

        assert!(!trap.active.get());
        assert_eq!(trap.current_target(), None);

        // Activate
        trap.activate(Some("btn-open-modal".to_string()));
        assert!(trap.active.get());
        assert_eq!(trap.current_target(), Some("input-name".to_string()));

        // Tab -> input-email
        let next = trap.handle_key("Tab", false);
        assert_eq!(next, Some("input-email".to_string()));
        assert_eq!(trap.current_target(), Some("input-email".to_string()));

        // Tab -> btn-submit
        let next = trap.handle_key("Tab", false);
        assert_eq!(next, Some("btn-submit".to_string()));

        // Tab -> wraps back to input-name
        let next = trap.handle_key("Tab", false);
        assert_eq!(next, Some("input-name".to_string()));

        // Shift+Tab -> wraps backward to btn-submit
        let prev = trap.handle_key("Tab", true);
        assert_eq!(prev, Some("btn-submit".to_string()));

        // Deactivate restores previous focus
        let restored = trap.deactivate();
        assert_eq!(restored, Some("btn-open-modal".to_string()));
        assert!(!trap.active.get());
    }
}
