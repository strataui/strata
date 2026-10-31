//! Accessible Accordion State Machine supporting Single and Multiple Disclosure.

use strata_dom::Signal;

/// Mode of accordion expansion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccordionType {
    Single,
    Multiple,
}

/// Headless accordion state controller.
#[derive(Clone)]
pub struct AccordionState {
    pub accordion_type: AccordionType,
    pub collapsible: bool,
    pub value: Signal<Vec<String>>,
}

impl AccordionState {
    /// Creates a new accordion state.
    pub fn new(accordion_type: AccordionType, collapsible: bool, default_value: Vec<String>) -> Self {
        Self {
            accordion_type,
            collapsible,
            value: Signal::new(default_value),
        }
    }

    /// Checks if a particular panel item is currently expanded.
    pub fn is_expanded(&self, item: &str) -> bool {
        self.value.get().iter().any(|v| v == item)
    }

    /// Toggles the disclosure of an item according to Single/Multiple rules.
    pub fn toggle(&self, item: &str) {
        let current = self.value.get();
        let is_open = current.iter().any(|v| v == item);

        match self.accordion_type {
            AccordionType::Single => {
                if is_open {
                    if self.collapsible {
                        self.value.set(Vec::new());
                    }
                } else {
                    self.value.set(vec![item.to_string()]);
                }
            }
            AccordionType::Multiple => {
                let mut updated = current;
                if is_open {
                    updated.retain(|v| v != item);
                } else {
                    updated.push(item.to_string());
                }
                self.value.set(updated);
            }
        }
    }

    /// Returns ARIA attributes for the header trigger button.
    pub fn trigger_attrs(&self, item: &str, panel_id: &str) -> [(&'static str, String); 3] {
        [
            ("aria-expanded", self.is_expanded(item).to_string()),
            ("aria-controls", panel_id.to_string()),
            ("role", "button".to_string()),
        ]
    }

    /// Returns ARIA attributes for the expandable content panel.
    pub fn panel_attrs(&self, item: &str, trigger_id: &str) -> [(&'static str, String); 3] {
        [
            ("role", "region".to_string()),
            ("aria-labelledby", trigger_id.to_string()),
            ("aria-hidden", (!self.is_expanded(item)).to_string()),
        ]
    }
}

/// Convenience hook to create an accordion state controller.
pub fn use_accordion(accordion_type: AccordionType, collapsible: bool, default_value: Vec<String>) -> AccordionState {
    AccordionState::new(accordion_type, collapsible, default_value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accordion_single_collapsible() {
        let acc = use_accordion(AccordionType::Single, true, vec!["item-1".to_string()]);

        assert!(acc.is_expanded("item-1"));
        assert!(!acc.is_expanded("item-2"));

        // Toggle item-2: closes item-1 and opens item-2
        acc.toggle("item-2");
        assert!(!acc.is_expanded("item-1"));
        assert!(acc.is_expanded("item-2"));

        // Collapse item-2
        acc.toggle("item-2");
        assert!(!acc.is_expanded("item-2"));
        assert!(acc.value.get().is_empty());
    }

    #[test]
    fn test_accordion_multiple() {
        let acc = use_accordion(AccordionType::Multiple, true, Vec::new());

        acc.toggle("item-1");
        acc.toggle("item-2");
        assert!(acc.is_expanded("item-1"));
        assert!(acc.is_expanded("item-2"));

        acc.toggle("item-1");
        assert!(!acc.is_expanded("item-1"));
        assert!(acc.is_expanded("item-2"));
    }
}
