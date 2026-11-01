//! Reactive Form Field State and Mutation.

use strata_dom::reactive::Signal;
use crate::validation::ValidationRule;

/// A single reactive field within a form.
#[derive(Clone)]
pub struct FormField {
    pub name: String,
    pub value: Signal<String>,
    pub error: Signal<Option<String>>,
    pub is_dirty: Signal<bool>,
    pub is_touched: Signal<bool>,
    initial_value: String,
    rules: Vec<ValidationRule<String>>,
}

impl FormField {
    pub fn new(name: impl Into<String>, initial: impl Into<String>, rules: Vec<ValidationRule<String>>) -> Self {
        let initial_str = initial.into();
        Self {
            name: name.into(),
            value: Signal::new(initial_str.clone()),
            error: Signal::new(None),
            is_dirty: Signal::new(false),
            is_touched: Signal::new(false),
            initial_value: initial_str,
            rules,
        }
    }

    /// Update field value and revalidate if field was already touched.
    pub fn set_value(&self, new_val: impl Into<String>) {
        let val_str = new_val.into();
        let dirty = val_str != self.initial_value;
        self.is_dirty.set(dirty);
        self.value.set(val_str.clone());

        if self.is_touched.get() {
            self.validate();
        }
    }

    /// Mark the field as touched/blurred and trigger validation.
    pub fn touch(&self) {
        self.is_touched.set(true);
        self.validate();
    }

    /// Validate the current field value against all registered rules.
    pub fn validate(&self) -> bool {
        let current_val = self.value.get();
        for rule in &self.rules {
            if let Err(err_msg) = rule.validate(&current_val) {
                self.error.set(Some(err_msg));
                return false;
            }
        }
        self.error.set(None);
        true
    }

    /// Reset field to initial state.
    pub fn reset(&self) {
        self.value.set(self.initial_value.clone());
        self.error.set(None);
        self.is_dirty.set(false);
        self.is_touched.set(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_reactivity_and_validation() {
        let field = FormField::new(
            "username",
            "",
            vec![
                ValidationRule::required("Username is required"),
                ValidationRule::min_length(3, "Min 3 chars"),
            ],
        );

        assert!(!field.is_dirty.get());
        assert!(!field.is_touched.get());
        assert!(field.error.get().is_none());

        field.set_value("jo");
        assert!(field.is_dirty.get());

        // Validate
        assert!(!field.validate());
        assert_eq!(field.error.get().unwrap(), "Min 3 chars");

        field.set_value("john_doe");
        assert!(field.validate());
        assert!(field.error.get().is_none());

        field.reset();
        assert!(!field.is_dirty.get());
        assert_eq!(field.value.get(), "");
    }
}
