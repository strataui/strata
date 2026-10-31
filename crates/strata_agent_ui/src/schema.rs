//! Whitelisted component schema and security validation rules for Agent Generative UI.

use std::collections::HashSet;

/// Approved RSX/HTML component tags safe for AI agent generation.
pub const ALLOWED_TAGS: &[&str] = &[
    "div", "span", "p", "h1", "h2", "h3", "h4", "h5", "h6",
    "button", "input", "textarea", "label", "form",
    "table", "tr", "th", "td",
    "ul", "ol", "li",
    "card", "badge", "slider", "switch", "modal", "drawer", "datagrid",
    "canvas3d", "mesh", "boxgeometry", "spheregeometry",
];

/// Prohibited tags that present security risks (code execution, unvetted resources).
pub const PROHIBITED_TAGS: &[&str] = &[
    "script", "iframe", "object", "embed", "applet", "meta", "link", "base",
];

/// Validates whether a tag name is permitted in the Agent UI sandbox.
pub fn is_tag_allowed(tag: &str) -> bool {
    let lower = tag.to_ascii_lowercase();
    if PROHIBITED_TAGS.contains(&lower.as_str()) {
        return false;
    }
    ALLOWED_TAGS.contains(&lower.as_str())
}

/// Validates an attribute name and value against injection patterns.
pub fn is_attribute_safe(name: &str, value: &str) -> bool {
    let lower_name = name.to_ascii_lowercase();
    let lower_val = value.to_ascii_lowercase();

    // Prevent inline raw javascript: URLs
    if lower_val.starts_with("javascript:") || lower_val.starts_with("data:text/html") {
        return false;
    }

    // Direct inline JS event handlers are forbidden in streaming agent UI;
    // agents must use declarative action identifiers like action="run_pipeline".
    if lower_name.starts_with("on") && !matches!(lower_name.as_str(), "onclick" | "oninput" | "onchange") {
        return false;
    }

    true
}

/// Schema validator holding whitelist configurations.
#[derive(Debug, Clone)]
pub struct AgentSchemaValidator {
    custom_allowed_tags: HashSet<String>,
}

impl Default for AgentSchemaValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentSchemaValidator {
    pub fn new() -> Self {
        let mut custom = HashSet::new();
        for &t in ALLOWED_TAGS {
            custom.insert(t.to_string());
        }
        Self { custom_allowed_tags: custom }
    }

    pub fn allow_custom_component(&mut self, tag: impl Into<String>) {
        self.custom_allowed_tags.insert(tag.into().to_ascii_lowercase());
    }

    pub fn validate_tag(&self, tag: &str) -> Result<(), String> {
        let lower = tag.to_ascii_lowercase();
        if PROHIBITED_TAGS.contains(&lower.as_str()) {
            return Err(format!("Prohibited dangerous tag '<{}>' rejected", tag));
        }
        if !self.custom_allowed_tags.contains(&lower) {
            return Err(format!("Unrecognized/unauthorized component tag '<{}>'", tag));
        }
        Ok(())
    }

    pub fn validate_attribute(&self, name: &str, value: &str) -> Result<(), String> {
        if !is_attribute_safe(name, value) {
            return Err(format!("Unsafe attribute '{}=\"{}\"' rejected", name, value));
        }
        Ok(())
    }
}
