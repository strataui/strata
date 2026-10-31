use strata_form::prelude::*;
use std::collections::HashMap;

#[test]
fn test_complex_registration_and_validation_schema() {
    let form = use_form();

    let email = form.register(
        "email",
        vec![
            ValidationRule::required("Email is required"),
            ValidationRule::email("Invalid email address"),
        ],
    );

    let password = form.register(
        "password",
        vec![
            ValidationRule::required("Password is required"),
            ValidationRule::min_length(8, "Must be at least 8 characters"),
        ],
    );

    let age = form.register(
        "age",
        vec![
            ValidationRule::min(18.0, "Must be at least 18"),
            ValidationRule::max(120.0, "Age exceeds reasonable limit"),
        ],
    );

    // Initial state
    assert!(!email.is_dirty.get());
    assert!(!password.is_dirty.get());

    // Enter invalid inputs
    email.set_value("not-an-email");
    password.set_value("short");
    age.set_value("16");

    let mut submission_ran = false;
    let submitted = form.handle_submit(|_| {
        submission_ran = true;
    });

    assert!(!submitted);
    assert!(!submission_ran);
    assert_eq!(email.error.get().unwrap(), "Invalid email address");
    assert_eq!(password.error.get().unwrap(), "Must be at least 8 characters");
    assert_eq!(age.error.get().unwrap(), "Must be at least 18");

    // Correct the inputs
    email.set_value("admin@larvance.com");
    password.set_value("super_secret_password");
    age.set_value("25");

    let mut captured_values = HashMap::new();
    let submitted2 = form.handle_submit(|vals| {
        captured_values = vals;
        submission_ran = true;
    });

    assert!(submitted2);
    assert!(submission_ran);
    assert_eq!(captured_values.get("email").unwrap(), "admin@larvance.com");
    assert_eq!(captured_values.get("password").unwrap(), "super_secret_password");
    assert_eq!(captured_values.get("age").unwrap(), "25");
}

#[test]
fn test_field_array_lifecycle() {
    let tasks = use_field_array(vec![
        "Task 1: Design".to_string(),
        "Task 2: Code".to_string(),
    ]);

    assert_eq!(tasks.len(), 2);

    tasks.append("Task 3: Test".to_string());
    assert_eq!(tasks.len(), 3);

    tasks.insert(1, "Task 1.5: Review".to_string());
    assert_eq!(tasks.len(), 4);
    assert_eq!(tasks.items().get()[1], "Task 1.5: Review");

    tasks.remove(1);
    assert_eq!(tasks.len(), 3);
    assert_eq!(tasks.items().get()[1], "Task 2: Code");
}
