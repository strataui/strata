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

/// Decodes numeric character references and common named HTML entities.
pub fn decode_html_entities(input: &str) -> String {
    if !input.contains('&') {
        return input.to_string();
    }

    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '&' {
            let mut entity = String::new();
            let mut found_semicolon = false;

            while let Some(&next_ch) = chars.peek() {
                if next_ch == ';' {
                    chars.next();
                    found_semicolon = true;
                    break;
                } else if next_ch.is_alphanumeric() || next_ch == '#' {
                    entity.push(chars.next().unwrap());
                    if entity.len() > 12 {
                        break;
                    }
                } else {
                    break;
                }
            }

            if !entity.is_empty() {
                if let Some(decoded) = resolve_entity(&entity) {
                    output.push_str(&decoded);
                    continue;
                }
            }

            output.push('&');
            output.push_str(&entity);
            if found_semicolon {
                output.push(';');
            }
        } else {
            output.push(ch);
        }
    }

    output
}

fn resolve_entity(entity: &str) -> Option<String> {
    if let Some(stripped) = entity.strip_prefix('#') {
        if let Some(hex) = stripped.strip_prefix('x').or_else(|| stripped.strip_prefix('X')) {
            u32::from_str_radix(hex, 16)
                .ok()
                .and_then(char::from_u32)
                .map(|c| c.to_string())
        } else {
            stripped
                .parse::<u32>()
                .ok()
                .and_then(char::from_u32)
                .map(|c| c.to_string())
        }
    } else {
        match entity.to_ascii_lowercase().as_str() {
            "tab" => Some("\t".to_string()),
            "newline" => Some("\n".to_string()),
            "amp" => Some("&".to_string()),
            "lt" => Some("<".to_string()),
            "gt" => Some(">".to_string()),
            "quot" => Some("\"".to_string()),
            "apos" => Some("'".to_string()),
            "colon" => Some(":".to_string()),
            _ => None,
        }
    }
}

/// Validates an attribute name and value against injection patterns and XSS evasion.
pub fn is_attribute_safe(name: &str, value: &str) -> bool {
    let lower_name = name.trim().to_ascii_lowercase();

    // Direct inline JS event handlers are forbidden in streaming agent UI;
    // agents must use declarative action identifiers like action="run_pipeline".
    if lower_name.starts_with("on") {
        return false;
    }

    // Decode HTML entities (run twice to prevent double-encoding evasion)
    let decoded = decode_html_entities(&decode_html_entities(value));

    // Clean URL value of inner whitespace, ASCII control characters, and zero-width characters
    let cleaned_val: String = decoded
        .chars()
        .filter(|&c| {
            !c.is_ascii_control()
                && !c.is_whitespace()
                && c != '\u{200B}'
                && c != '\u{200C}'
                && c != '\u{200D}'
                && c != '\u{FEFF}'
                && c != '\u{00AD}'
        })
        .collect::<String>()
        .to_ascii_lowercase();

    // Prevent script execution protocols
    if cleaned_val.starts_with("javascript:")
        || cleaned_val.starts_with("vbscript:")
        || cleaned_val.starts_with("blob:")
    {
        return false;
    }

    // Handle data: URIs - strictly permit only safe raster/static image MIME types
    if cleaned_val.starts_with("data:") {
        let is_safe_image = cleaned_val.starts_with("data:image/png")
            || cleaned_val.starts_with("data:image/jpeg")
            || cleaned_val.starts_with("data:image/jpg")
            || cleaned_val.starts_with("data:image/webp")
            || cleaned_val.starts_with("data:image/gif");

        if !is_safe_image {
            return false;
        }

        // Even with an allowed image MIME, ensure it doesn't contain embedded script payloads
        if cleaned_val.contains("javascript:") || cleaned_val.contains("<script") {
            return false;
        }
    }

    // Check style attribute for dangerous CSS expressions or script invocations
    if lower_name == "style"
        && (cleaned_val.contains("javascript:")
            || cleaned_val.contains("vbscript:")
            || cleaned_val.contains("expression(")
            || cleaned_val.contains("-moz-binding"))
    {
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
