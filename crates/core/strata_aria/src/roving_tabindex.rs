//! WAI-ARIA Roving Tabindex Engine for Menus, Toolbars, Tabs, and Radios.

use strata_dom::Signal;

/// Navigation axis orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Horizontal,
    Vertical,
    Both,
}

/// Roving Tabindex manager keeping a single active element with tabindex="0"
/// while all sibling elements hold tabindex="-1".
#[derive(Clone)]
pub struct RovingTabIndex {
    pub orientation: Orientation,
    pub loop_navigation: bool,
    items: Vec<String>,
    disabled_indices: Vec<usize>,
    selected_index: Signal<usize>,
}

impl RovingTabIndex {
    /// Creates a new roving tabindex controller with given items and orientation.
    pub fn new(items: Vec<String>, orientation: Orientation) -> Self {
        Self {
            orientation,
            loop_navigation: true,
            items,
            disabled_indices: Vec::new(),
            selected_index: Signal::new(0),
        }
    }

    /// Mark an index as disabled so keyboard navigation skips it.
    pub fn set_disabled(&mut self, disabled: Vec<usize>) {
        self.disabled_indices = disabled;
    }

    /// Returns whether an index is disabled.
    pub fn is_disabled(&self, index: usize) -> bool {
        self.disabled_indices.contains(&index)
    }

    /// Gets the current selected index.
    pub fn current_index(&self) -> usize {
        self.selected_index.get()
    }

    /// Selects an item by index if not disabled.
    pub fn select_index(&self, index: usize) {
        if index < self.items.len() && !self.is_disabled(index) {
            self.selected_index.set(index);
        }
    }

    /// Gets the ID of the currently selected element.
    pub fn selected_id(&self) -> Option<String> {
        self.items.get(self.selected_index.get()).cloned()
    }

    /// Computes HTML `tabindex` for a child item (0 for active, -1 for inactive).
    pub fn tab_index_for(&self, index: usize) -> i32 {
        if index == self.selected_index.get() {
            0
        } else {
            -1
        }
    }

    /// Computes WAI-ARIA `aria-selected` boolean for an item.
    pub fn aria_selected_for(&self, index: usize) -> bool {
        index == self.selected_index.get()
    }

    /// Handles arrow and home/end keys according to roving tabindex conventions.
    pub fn handle_key(&self, key: &str) -> Option<usize> {
        let total = self.items.len();
        if total == 0 {
            return None;
        }

        let is_forward = matches!(
            (self.orientation, key),
            (Orientation::Horizontal | Orientation::Both, "ArrowRight")
                | (Orientation::Vertical | Orientation::Both, "ArrowDown")
        );

        let is_backward = matches!(
            (self.orientation, key),
            (Orientation::Horizontal | Orientation::Both, "ArrowLeft")
                | (Orientation::Vertical | Orientation::Both, "ArrowUp")
        );

        if key == "Home" {
            for i in 0..total {
                if !self.is_disabled(i) {
                    self.selected_index.set(i);
                    return Some(i);
                }
            }
        } else if key == "End" {
            for i in (0..total).rev() {
                if !self.is_disabled(i) {
                    self.selected_index.set(i);
                    return Some(i);
                }
            }
        } else if is_forward {
            let mut next = self.selected_index.get();
            for _ in 0..total {
                next += 1;
                if next >= total {
                    if self.loop_navigation {
                        next = 0;
                    } else {
                        break;
                    }
                }
                if !self.is_disabled(next) {
                    self.selected_index.set(next);
                    return Some(next);
                }
            }
        } else if is_backward {
            let mut prev = self.selected_index.get();
            for _ in 0..total {
                if prev == 0 {
                    if self.loop_navigation {
                        prev = total - 1;
                    } else {
                        break;
                    }
                } else {
                    prev -= 1;
                }
                if !self.is_disabled(prev) {
                    self.selected_index.set(prev);
                    return Some(prev);
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roving_tabindex_horizontal() {
        let items = vec!["tab-0".to_string(), "tab-1".to_string(), "tab-2".to_string()];
        let mut roving = RovingTabIndex::new(items, Orientation::Horizontal);
        roving.set_disabled(vec![1]); // Disable middle tab

        assert_eq!(roving.tab_index_for(0), 0);
        assert_eq!(roving.tab_index_for(1), -1);
        assert_eq!(roving.tab_index_for(2), -1);

        // ArrowRight skips disabled tab-1 and lands on tab-2
        let next = roving.handle_key("ArrowRight");
        assert_eq!(next, Some(2));
        assert_eq!(roving.tab_index_for(2), 0);
        assert_eq!(roving.tab_index_for(0), -1);

        // ArrowRight wraps to tab-0
        let next = roving.handle_key("ArrowRight");
        assert_eq!(next, Some(0));

        // Home / End
        roving.handle_key("End");
        assert_eq!(roving.current_index(), 2);
        roving.handle_key("Home");
        assert_eq!(roving.current_index(), 0);
    }
}
