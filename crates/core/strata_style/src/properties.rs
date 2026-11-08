//! CSS Properties and Values API Level 1 (Houdini `@property`) registration and syntax engine.

use crate::types::Dimension;
use std::collections::HashMap;

/// Syntax descriptor for a registered custom CSS property.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertySyntax {
    /// Any valid token stream (`*`).
    Any,
    /// Color values (`<color>`).
    Color,
    /// Length dimensions (`<length>`).
    Length,
    /// Percentage dimensions (`<percentage>`).
    Percentage,
    /// Length or percentage (`<length-percentage>`).
    LengthPercentage,
    /// Floating-point numbers (`<number>`).
    Number,
    /// Integer numbers (`<integer>`).
    Integer,
    /// Angles (`<angle>`).
    Angle,
    /// Time durations (`<time>`).
    Time,
    /// Custom identifier (`<custom-ident>`).
    CustomIdent,
}

impl PropertySyntax {
    /// Parses a CSS syntax descriptor string (e.g. `'<color>'` or `"<length>"`).
    pub fn parse(s: &str) -> Self {
        let cleaned = s.trim().trim_matches('\'').trim_matches('"').trim();
        match cleaned.to_ascii_lowercase().as_str() {
            "*" => Self::Any,
            "<color>" => Self::Color,
            "<length>" => Self::Length,
            "<percentage>" => Self::Percentage,
            "<length-percentage>" => Self::LengthPercentage,
            "<number>" => Self::Number,
            "<integer>" => Self::Integer,
            "<angle>" => Self::Angle,
            "<time>" => Self::Time,
            "<custom-ident>" => Self::CustomIdent,
            _ => Self::Any,
        }
    }
}

/// A registered custom CSS property defined via CSS `@property` or programmatic API.
#[derive(Debug, Clone, PartialEq)]
pub struct RegisteredProperty {
    pub name: String,
    pub syntax: PropertySyntax,
    pub inherits: bool,
    pub initial_value: Option<String>,
}

/// In-memory registry of custom CSS properties enabling typed interpolation and inheritance control.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PropertyRegistry {
    properties: HashMap<String, RegisteredProperty>,
}

impl PropertyRegistry {
    /// Creates an empty `PropertyRegistry`.
    pub fn new() -> Self {
        Self {
            properties: HashMap::new(),
        }
    }

    /// Registers a new custom property definition.
    pub fn register(&mut self, prop: RegisteredProperty) {
        self.properties.insert(prop.name.clone(), prop);
    }

    /// Retrieves a registered property definition by name.
    pub fn get(&self, name: &str) -> Option<&RegisteredProperty> {
        self.properties.get(name)
    }

    /// Returns true if the custom property is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.properties.contains_key(name)
    }

    /// Evaluates if a custom property inherits down the cascade (defaults to true if unregistered).
    pub fn is_inheritable(&self, name: &str) -> bool {
        self.properties.get(name).is_none_or(|p| p.inherits)
    }

    /// Linearly interpolates two custom property string values based on the registered syntax.
    pub fn interpolate(&self, name: &str, from: &str, to: &str, t: f32) -> Option<String> {
        let prop = self.properties.get(name)?;
        let t = t.clamp(0.0, 1.0);
        match prop.syntax {
            PropertySyntax::Color => {
                let c1 = crate::parser::parse_color(from)?;
                let c2 = crate::parser::parse_color(to)?;
                let res = crate::transition::interpolate_color(c1, c2, t);
                Some(format!(
                    "rgba({}, {}, {}, {:.3})",
                    res.r,
                    res.g,
                    res.b,
                    res.a as f32 / 255.0
                ))
            }
            PropertySyntax::Number | PropertySyntax::Angle | PropertySyntax::Time => {
                let n1 = from.trim().parse::<f32>().ok()?;
                let n2 = to.trim().parse::<f32>().ok()?;
                let val = n1 + (n2 - n1) * t;
                Some(format!("{:.4}", val))
            }
            PropertySyntax::Integer => {
                let n1 = from.trim().parse::<f32>().ok()?;
                let n2 = to.trim().parse::<f32>().ok()?;
                let val = (n1 + (n2 - n1) * t).round() as i32;
                Some(val.to_string())
            }
            PropertySyntax::Length => {
                let d1 = crate::parser::parse_dimension(from)?;
                let d2 = crate::parser::parse_dimension(to)?;
                let interpolated = crate::transition::interpolate_dimension(&d1, &d2, t);
                match interpolated {
                    Dimension::Px(px) => Some(format!("{}px", px)),
                    Dimension::Rem(rem) => Some(format!("{}rem", rem)),
                    Dimension::Em(em) => Some(format!("{}em", em)),
                    _ => Some(to.to_string()),
                }
            }
            PropertySyntax::Percentage => {
                let p1 = from.trim().strip_suffix('%')?.trim().parse::<f32>().ok()?;
                let p2 = to.trim().strip_suffix('%')?.trim().parse::<f32>().ok()?;
                let val = p1 + (p2 - p1) * t;
                Some(format!("{:.2}%", val))
            }
            _ => {
                if t < 0.5 {
                    Some(from.to_string())
                } else {
                    Some(to.to_string())
                }
            }
        }
    }
}

/// Parses the declaration body of a CSS `@property` rule.
pub fn parse_property_rule_body(name: &str, body: &str) -> RegisteredProperty {
    let mut syntax = PropertySyntax::Any;
    let mut inherits = true;
    let mut initial_value = None;

    for stmt in body.split(';') {
        let trimmed = stmt.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some((k, v)) = trimmed.split_once(':') {
            let key = k.trim().to_ascii_lowercase();
            let val = v.trim();

            match key.as_str() {
                "syntax" => {
                    syntax = PropertySyntax::parse(val);
                }
                "inherits" => {
                    inherits = val.eq_ignore_ascii_case("true");
                }
                "initial-value" => {
                    initial_value = Some(val.to_string());
                }
                _ => {}
            }
        }
    }

    RegisteredProperty {
        name: name.to_string(),
        syntax,
        inherits,
        initial_value,
    }
}
