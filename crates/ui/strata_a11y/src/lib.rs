//! Native screen-reader accessibility bridge and WCAG 2.1 AAA auditor for Project Strata.

pub mod audit;
pub mod bridge;
pub mod node;
pub mod role;
pub mod update;

pub use audit::{
    audit_tree, calculate_contrast_ratio, evaluate_large_text_contrast,
    evaluate_normal_text_contrast, relative_luminance, AuditSeverity, AuditViolation,
    WcagCompliance,
};
pub use bridge::{A11yBridge, ActionCallback};
pub use node::{A11yAction, A11yNode, A11yNodeId, A11yRect, A11yTree};
pub use role::A11yRole;
pub use update::A11yTreeUpdate;
