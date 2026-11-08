//! Central Reactive Form Orchestrator.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use strata_dom::reactive::Signal;

use crate::field::FormField;
use crate::validation::ValidationRule;

/// Reactive Form state container.
#[derive(Clone)]
pub struct Form {
    fields: Rc<RefCell<HashMap<String, FormField>>>,
    pub is_submitting: Signal<bool>,
    pub is_valid: Signal<bool>,
    pub submit_count: Signal<usize>,
}

impl Default for Form {
    fn default() -> Self {
        Self::new()
    }
}

impl Form {
    pub fn new() -> Self {
        Self {
            fields: Rc::new(RefCell::new(HashMap::new())),
            is_submitting: Signal::new(false),
            is_valid: Signal::new(true),
            submit_count: Signal::new(0),
        }
    }

    /// Register a field with the form, configuring validation rules.
    pub fn register(&self, name: &str, rules: Vec<ValidationRule<String>>) -> FormField {
        let mut fields = self.fields.borrow_mut();
        if let Some(f) = fields.get(name) {
            f.clone()
        } else {
            let field = FormField::new(name, "", rules);
            fields.insert(name.to_string(), field.clone());
            field
        }
    }

    /// Register a field with an explicit initial value.
    pub fn register_with_default(&self, name: &str, default_val: &str, rules: Vec<ValidationRule<String>>) -> FormField {
        let mut fields = self.fields.borrow_mut();
        if let Some(f) = fields.get(name) {
            f.clone()
        } else {
            let field = FormField::new(name, default_val, rules);
            fields.insert(name.to_string(), field.clone());
            field
        }
    }

    /// Get a registered field by name.
    pub fn get_field(&self, name: &str) -> Option<FormField> {
        self.fields.borrow().get(name).cloned()
    }

    /// Snapshot all current field values as a string map.
    pub fn get_values(&self) -> HashMap<String, String> {
        let fields = self.fields.borrow();
        let mut values = HashMap::new();
        for (name, field) in fields.iter() {
            values.insert(name.clone(), field.value.get());
        }
        values
    }

    /// Set value of a specific field.
    pub fn set_value(&self, name: &str, val: impl Into<String>) {
        if let Some(f) = self.get_field(name) {
            f.set_value(val);
        }
    }

    /// Validate all registered fields. Returns true if all fields are valid.
    pub fn validate_all(&self) -> bool {
        let fields = self.fields.borrow();
        let mut valid = true;
        for field in fields.values() {
            field.is_touched.set(true);
            if !field.validate() {
                valid = false;
            }
        }
        self.is_valid.set(valid);
        valid
    }

    /// Handle form submission, validating all fields and executing callback on success.
    pub fn handle_submit<F: FnOnce(HashMap<String, String>)>(&self, on_valid: F) -> bool {
        self.submit_count.update(|c| *c += 1);
        self.is_submitting.set(true);

        let valid = self.validate_all();
        if valid {
            let values = self.get_values();
            on_valid(values);
        }

        self.is_submitting.set(false);
        valid
    }

    /// Reset all fields to their initial values.
    pub fn reset(&self) {
        let fields = self.fields.borrow();
        for field in fields.values() {
            field.reset();
        }
        self.is_valid.set(true);
        self.is_submitting.set(false);
    }
}

/// Hook to initialize a reactive Form instance.
pub fn use_form() -> Form {
    Form::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_form_lifecycle_and_submission() {
        let form = use_form();
        let email_field = form.register("email", vec![ValidationRule::email("Invalid email")]);
        let pass_field = form.register("password", vec![ValidationRule::min_length(6, "Password too short")]);

        // Attempt submit while empty
        let mut submitted = false;
        let success = form.handle_submit(|_| {
            submitted = true;
        });
        assert!(!success);
        assert!(!submitted);
        assert_eq!(form.submit_count.get(), 1);
        assert!(email_field.error.get().is_some());
        assert!(pass_field.error.get().is_some());

        // Fill in valid data
        email_field.set_value("developer@strata.dev");
        pass_field.set_value("secure_pass_123");

        let mut received_values = HashMap::new();
        let success2 = form.handle_submit(|values| {
            received_values = values;
        });
        assert!(success2);
        assert_eq!(received_values.get("email").unwrap(), "developer@strata.dev");
        assert_eq!(received_values.get("password").unwrap(), "secure_pass_123");
    }
}
