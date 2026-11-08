pub mod accesskit;
pub mod audit;
pub mod bridge;
pub mod builder;
pub mod node;
pub mod role;
pub mod update;

pub use accesskit::{
    AccessKitActionRequest, AccessKitAdapter, AccessKitNode, AccessKitTreeMeta,
    AccessKitTreeUpdate,
};
pub use audit::{
    audit_tree, calculate_contrast_ratio, evaluate_large_text_contrast,
    evaluate_normal_text_contrast, relative_luminance, AuditSeverity, AuditViolation,
    WcagCompliance,
};
pub use bridge::{A11yBridge, ActionCallback};
pub use builder::{
    bind_layout_tree_bounds, build_a11y_tree, build_a11y_tree_with_layout, extract_text_content,
    A11yTreeBuilder,
};
pub use node::{A11yAction, A11yLivePoliteness, A11yNode, A11yNodeId, A11yRect, A11yTree};

pub use role::A11yRole;
pub use update::A11yTreeUpdate;

