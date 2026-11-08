//! WCAG 2.1 AAA color contrast calculations and automated compliance auditing.

use crate::node::{A11yNode, A11yTree};
use crate::role::A11yRole;

/// WCAG compliance rating for a given contrast ratio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WcagCompliance {
    Fail,
    AaLargeOnly,
    Aa,
    Aaa,
}

/// Computes the relative luminance of an sRGB color according to W3C WCAG 2.1.
/// Input values `r`, `g`, `b` in `[0, 255]`.
pub fn relative_luminance(r: u8, g: u8, b: u8) -> f64 {
    fn channel_luminance(val: u8) -> f64 {
        let v = val as f64 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    }

    let r_lin = channel_luminance(r);
    let g_lin = channel_luminance(g);
    let b_lin = channel_luminance(b);

    0.2126 * r_lin + 0.7152 * g_lin + 0.0722 * b_lin
}

/// Computes the contrast ratio between two sRGB colors according to WCAG 2.1.
/// Returns a ratio in the range `1.0` to `21.0`.
pub fn calculate_contrast_ratio(fg: (u8, u8, u8), bg: (u8, u8, u8)) -> f64 {
    let l1 = relative_luminance(fg.0, fg.1, fg.2);
    let l2 = relative_luminance(bg.0, bg.1, bg.2);

    let lighter = l1.max(l2);
    let darker = l1.min(l2);

    (lighter + 0.05) / (darker + 0.05)
}

/// Evaluates WCAG compliance for standard body text (size < 18pt or < 14pt bold).
pub fn evaluate_normal_text_contrast(ratio: f64) -> WcagCompliance {
    if ratio >= 7.0 {
        WcagCompliance::Aaa
    } else if ratio >= 4.5 {
        WcagCompliance::Aa
    } else if ratio >= 3.0 {
        WcagCompliance::AaLargeOnly
    } else {
        WcagCompliance::Fail
    }
}

/// Evaluates WCAG compliance for large text (size >= 18pt or >= 14pt bold).
pub fn evaluate_large_text_contrast(ratio: f64) -> WcagCompliance {
    if ratio >= 4.5 {
        WcagCompliance::Aaa
    } else if ratio >= 3.0 {
        WcagCompliance::Aa
    } else {
        WcagCompliance::Fail
    }
}

/// Severity of an accessibility audit violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditSeverity {
    Warning,
    Error,
}

/// A violation identified during an accessibility audit.
#[derive(Debug, Clone, PartialEq)]
pub struct AuditViolation {
    pub rule: &'static str,
    pub severity: AuditSeverity,
    pub node_id: u64,
    pub message: String,
}

/// Audits an `A11yTree` for accessibility compliance.
pub fn audit_tree(tree: &A11yTree) -> Vec<AuditViolation> {
    let mut violations = Vec::new();

    for node in tree.nodes.values() {
        audit_node(node, &mut violations);
    }

    violations
}

fn audit_node(node: &A11yNode, violations: &mut Vec<AuditViolation>) {
    // Rule 1: Interactive elements must have an accessible name (label)
    if node.role.is_interactive() {
        let has_name = node.name.as_ref().map(|s| !s.trim().is_empty()).unwrap_or(false);
        if !has_name {
            violations.push(AuditViolation {
                rule: "A11Y_RULE_001_MISSING_ACCESSIBLE_NAME",
                severity: AuditSeverity::Error,
                node_id: node.id.0,
                message: format!(
                    "Interactive element with role '{:?}' has no accessible name or aria-label.",
                    node.role
                ),
            });
        }
    }

    // Rule 2: Images must have an accessible name or explicit presentation role
    if node.role == A11yRole::Image {
        let has_alt = node.name.as_ref().map(|s| !s.trim().is_empty()).unwrap_or(false);
        if !has_alt {
            violations.push(AuditViolation {
                rule: "A11Y_RULE_002_IMAGE_MISSING_ALT",
                severity: AuditSeverity::Error,
                node_id: node.id.0,
                message: "Image element must specify alt text or an accessible name.".to_string(),
            });
        }
    }

    // Rule 3: Form inputs must have an associated label or description
    if node.role == A11yRole::TextInput {
        let has_label = node.name.is_some() || node.description.is_some();
        if !has_label {
            violations.push(AuditViolation {
                rule: "A11Y_RULE_003_INPUT_MISSING_LABEL",
                severity: AuditSeverity::Warning,
                node_id: node.id.0,
                message: "TextInput element should have an associated label or description.".to_string(),
            });
        }
    }

    // Rule 4: Sliders must have min, max, and current numeric value defined
    if node.role == A11yRole::Slider
        && (node.numeric_value.is_none() || node.min_numeric_value.is_none() || node.max_numeric_value.is_none()) {
            violations.push(AuditViolation {
                rule: "A11Y_RULE_004_SLIDER_MISSING_BOUNDS",
                severity: AuditSeverity::Error,
                node_id: node.id.0,
                message: "Slider element must provide numeric_value, min_numeric_value, and max_numeric_value.".to_string(),
            });
        }
}
