//! CSS Selector parser, specificity calculator, and element matcher.

use crate::specificity::Specificity;

/// Interactive element pseudo-classes and environmental state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PseudoState {
    pub is_hovered: bool,
    pub is_focused: bool,
    pub is_active: bool,
    pub is_disabled: bool,
    pub is_dark: bool,
}

/// Atomic part of a CSS compound selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectorPart {
    Universal,
    Tag(String),
    Id(String),
    Class(String),
    PseudoClass(String),
}

/// Parsed CSS compound selector with pre-computed W3C specificity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    pub raw: String,
    pub parts: Vec<SelectorPart>,
    pub specificity: Specificity,
}

impl Selector {
    /// Parses a single compound selector (e.g. `button.btn-primary:hover`).
    pub fn parse(input: &str) -> Result<Self, String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err("Empty selector".to_string());
        }

        let mut parts = Vec::new();
        let mut spec = Specificity::ZERO;
        let mut chars = trimmed.chars().peekable();

        while let Some(&c) = chars.peek() {
            match c {
                '*' => {
                    chars.next();
                    parts.push(SelectorPart::Universal);
                    // Universal selector adds (0, 0, 0, 0)
                }
                '#' => {
                    chars.next();
                    let ident = parse_ident(&mut chars);
                    if ident.is_empty() {
                        return Err("Empty ID selector after '#' in CSS".to_string());
                    }
                    parts.push(SelectorPart::Id(ident));
                    spec += Specificity::ID;
                }
                '.' => {
                    chars.next();
                    let ident = parse_ident(&mut chars);
                    if ident.is_empty() {
                        return Err("Empty class selector after '.' in CSS".to_string());
                    }
                    parts.push(SelectorPart::Class(ident));
                    spec += Specificity::CLASS;
                }
                ':' => {
                    chars.next();
                    let ident = parse_ident(&mut chars);
                    if ident.is_empty() {
                        return Err("Empty pseudo-class after ':' in CSS".to_string());
                    }
                    parts.push(SelectorPart::PseudoClass(ident));
                    spec += Specificity::CLASS;
                }
                _ if c.is_alphabetic() || c == '_' || c == '-' => {
                    let ident = parse_ident(&mut chars);
                    parts.push(SelectorPart::Tag(ident));
                    spec += Specificity::TYPE;
                }
                _ => {
                    return Err(format!("Unexpected character '{}' in selector '{}'", c, trimmed));
                }
            }
        }

        Ok(Self {
            raw: trimmed.to_string(),
            parts,
            specificity: spec,
        })
    }

    /// Evaluates whether this selector matches an element's attributes and current pseudo-state.
    pub fn matches(&self, tag: &str, classes: &[&str], id: Option<&str>, state: &PseudoState) -> bool {
        for part in &self.parts {
            match part {
                SelectorPart::Universal => {}
                SelectorPart::Tag(expected_tag) => {
                    if !expected_tag.eq_ignore_ascii_case(tag) {
                        return false;
                    }
                }
                SelectorPart::Id(expected_id) => {
                    match id {
                        Some(actual_id) if actual_id == expected_id => {}
                        _ => return false,
                    }
                }
                SelectorPart::Class(expected_class) => {
                    if !classes.contains(&expected_class.as_str()) {
                        return false;
                    }
                }
                SelectorPart::PseudoClass(pseudo) => match pseudo.as_str() {
                    "hover" => {
                        if !state.is_hovered {
                            return false;
                        }
                    }
                    "focus" => {
                        if !state.is_focused {
                            return false;
                        }
                    }
                    "active" => {
                        if !state.is_active {
                            return false;
                        }
                    }
                    "disabled" => {
                        if !state.is_disabled {
                            return false;
                        }
                    }
                    "dark" => {
                        if !state.is_dark {
                            return false;
                        }
                    }
                    _ => return false,
                },
            }
        }
        true
    }
}

fn parse_ident<I: Iterator<Item = char>>(chars: &mut std::iter::Peekable<I>) -> String {
    let mut s = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_alphanumeric() || c == '-' || c == '_' {
            s.push(chars.next().unwrap());
        } else {
            break;
        }
    }
    s
}
