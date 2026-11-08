//! Form Validation Rules and Schema Validation.

use std::rc::Rc;

/// Type alias for custom field validator callbacks.
pub type ValidatorFn<T> = Rc<dyn Fn(&T) -> Result<(), String>>;

/// Predefined validation rules for form fields.
#[derive(Clone)]
pub enum ValidationRule<T> {
    Required { message: String },
    MinLength { min: usize, message: String },
    MaxLength { max: usize, message: String },
    Min { min: f64, message: String },
    Max { max: f64, message: String },
    Email { message: String },
    Custom {
        validator: ValidatorFn<T>,
    },
}

impl<T> std::fmt::Debug for ValidationRule<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Required { message } => write!(f, "Required({message})"),
            Self::MinLength { min, message } => write!(f, "MinLength({min}, {message})"),
            Self::MaxLength { max, message } => write!(f, "MaxLength({max}, {message})"),
            Self::Min { min, message } => write!(f, "Min({min}, {message})"),
            Self::Max { max, message } => write!(f, "Max({max}, {message})"),
            Self::Email { message } => write!(f, "Email({message})"),
            Self::Custom { .. } => write!(f, "Custom(<validator>)"),
        }
    }
}

impl ValidationRule<String> {
    pub fn required(msg: impl Into<String>) -> Self {
        Self::Required { message: msg.into() }
    }

    pub fn min_length(min: usize, msg: impl Into<String>) -> Self {
        Self::MinLength { min, message: msg.into() }
    }

    pub fn max_length(max: usize, msg: impl Into<String>) -> Self {
        Self::MaxLength { max, message: msg.into() }
    }

    pub fn email(msg: impl Into<String>) -> Self {
        Self::Email { message: msg.into() }
    }

    pub fn min(min: f64, msg: impl Into<String>) -> Self {
        Self::Min { min, message: msg.into() }
    }

    pub fn max(max: f64, msg: impl Into<String>) -> Self {
        Self::Max { max, message: msg.into() }
    }

    pub fn custom(validator: impl Fn(&String) -> Result<(), String> + 'static) -> Self {
        Self::Custom {
            validator: Rc::new(validator),
        }
    }

    /// Validate a string value against this rule.
    pub fn validate(&self, val: &str) -> Result<(), String> {
        match self {
            Self::Required { message } => {
                if val.trim().is_empty() {
                    Err(message.clone())
                } else {
                    Ok(())
                }
            }
            Self::MinLength { min, message } => {
                if val.len() < *min {
                    Err(message.clone())
                } else {
                    Ok(())
                }
            }
            Self::MaxLength { max, message } => {
                if val.len() > *max {
                    Err(message.clone())
                } else {
                    Ok(())
                }
            }
            Self::Min { min, message } => {
                if let Ok(n) = val.parse::<f64>() {
                    if n < *min {
                        return Err(message.clone());
                    }
                }
                Ok(())
            }
            Self::Max { max, message } => {
                if let Ok(n) = val.parse::<f64>() {
                    if n > *max {
                        return Err(message.clone());
                    }
                }
                Ok(())
            }
            Self::Email { message } => {
                let clean = val.trim();
                if !clean.contains('@') || !clean.contains('.') || clean.starts_with('@') || clean.ends_with('@') {
                    Err(message.clone())
                } else {
                    Ok(())
                }
            }
            Self::Custom { validator } => {
                let s = val.to_string();
                validator(&s)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_rules() {
        let req = ValidationRule::required("This field is required");
        assert!(req.validate("Hello").is_ok());
        assert!(req.validate("").is_err());
        assert!(req.validate("   ").is_err());

        let min = ValidationRule::min_length(4, "Must be at least 4 chars");
        assert!(min.validate("Rust").is_ok());
        assert!(min.validate("Go").is_err());

        let email = ValidationRule::email("Invalid email address");
        assert!(email.validate("user@example.com").is_ok());
        assert!(email.validate("invalid-email").is_err());
    }
}
