//! Strata Form: Signal-Driven Reactive Form State and Schema Validation Engine.
//!
//! Provides a pure-Rust, zero-re-render equivalent to `react-hook-form` and `zod`
//! with declarative validation rules, touched/dirty tracking, dynamic field arrays, and atomic submission handling.

pub mod field;
pub mod field_array;
pub mod form;
pub mod validation;

pub use field::FormField;
pub use field_array::{use_field_array, FieldArray};
pub use form::{use_form, Form};
pub use validation::ValidationRule;

pub mod prelude {
    pub use crate::field::FormField;
    pub use crate::field_array::{use_field_array, FieldArray};
    pub use crate::form::{use_form, Form};
    pub use crate::validation::ValidationRule;
}
