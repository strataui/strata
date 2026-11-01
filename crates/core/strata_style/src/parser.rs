//! Zero-dependency CSS stylesheet lexer, parser, and property value decoders.

use crate::selector::Selector;
use crate::types::{Color, Dimension, EdgeInsets};

/// Single CSS property declaration (e.g. `background-color: #000 !important;`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub property: String,
    pub value: String,
    pub important: bool,
}

/// CSS style rule associating a list of selectors with a declaration block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleRule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<Declaration>,
}

/// Parsed CSS stylesheet containing ordered style rules.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StyleSheet {
    pub rules: Vec<StyleRule>,
}

impl StyleSheet {
    /// Parses a complete CSS stylesheet string.
    pub fn parse(css: &str) -> Result<Self, String> {
        let stripped = strip_css_comments(css);
        let mut rules = Vec::new();
        let mut chars = stripped.chars().peekable();

        loop {
            skip_whitespace(&mut chars);
            if chars.peek().is_none() {
                break;
            }

            // Read selectors up to '{'
            let mut selector_buf = String::new();
            let mut found_brace = false;

            while let Some(c) = chars.next() {
                if c == '{' {
                    found_brace = true;
                    break;
                }
                selector_buf.push(c);
            }

            if !found_brace {
                break;
            }

            // Read declaration block up to '}'
            let mut decl_buf = String::new();
            let mut depth = 1;

            while let Some(c) = chars.next() {
                if c == '{' {
                    depth += 1;
                    decl_buf.push(c);
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    decl_buf.push(c);
                } else {
                    decl_buf.push(c);
                }
            }

            // Parse comma-separated selectors
            let mut selectors = Vec::new();
            for sel_str in selector_buf.split(',') {
                let trimmed = sel_str.trim();
                if !trimmed.is_empty() {
                    if let Ok(sel) = Selector::parse(trimmed) {
                        selectors.push(sel);
                    }
                }
            }

            if selectors.is_empty() {
                continue;
            }

            // Parse declaration block
            let declarations = parse_declarations(&decl_buf);
            rules.push(StyleRule {
                selectors,
                declarations,
            });
        }

        Ok(Self { rules })
    }
}

/// Parses a declaration block body into a list of `Declaration`s.
pub fn parse_declarations(block: &str) -> Vec<Declaration> {
    let mut declarations = Vec::new();

    for stmt in block.split(';') {
        let trimmed = stmt.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some((prop, val)) = trimmed.split_once(':') {
            let property = prop.trim().to_ascii_lowercase();
            let mut val_str = val.trim();
            let mut important = false;

            if let Some(stripped) = val_str.strip_suffix("!important") {
                important = true;
                val_str = stripped.trim();
            } else if val_str.to_ascii_lowercase().ends_with("!important") {
                important = true;
                let len = val_str.len();
                val_str = val_str[..len - 10].trim();
            }

            declarations.push(Declaration {
                property,
                value: val_str.to_string(),
                important,
            });
        }
    }

    declarations
}

/// Strips all `/* ... */` comment blocks from a CSS string.
fn strip_css_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut chars = css.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '/' && chars.peek() == Some(&'*') {
            chars.next(); // consume '*'
            while let Some(inner) = chars.next() {
                if inner == '*' && chars.peek() == Some(&'/') {
                    chars.next(); // consume '/'
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }

    out
}

fn skip_whitespace<I: Iterator<Item = char>>(chars: &mut std::iter::Peekable<I>) {
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else {
            break;
        }
    }
}

/// Decodes CSS dimension values into `Dimension`.
pub fn parse_dimension(val: &str) -> Option<Dimension> {
    let s = val.trim().to_ascii_lowercase();
    if s == "auto" {
        return Some(Dimension::Auto);
    }
    if let Some(px) = s.strip_suffix("px") {
        if let Ok(n) = px.trim().parse::<f32>() {
            return Some(Dimension::Px(n));
        }
    }
    if let Some(pct) = s.strip_suffix('%') {
        if let Ok(n) = pct.trim().parse::<f32>() {
            return Some(Dimension::Percent(n));
        }
    }
    if let Some(rem) = s.strip_suffix("rem") {
        if let Ok(n) = rem.trim().parse::<f32>() {
            return Some(Dimension::Rem(n));
        }
    }
    if let Some(em) = s.strip_suffix("em") {
        if let Ok(n) = em.trim().parse::<f32>() {
            return Some(Dimension::Em(n));
        }
    }
    if let Some(vw) = s.strip_suffix("vw") {
        if let Ok(n) = vw.trim().parse::<f32>() {
            return Some(Dimension::Vw(n));
        }
    }
    if let Some(vh) = s.strip_suffix("vh") {
        if let Ok(n) = vh.trim().parse::<f32>() {
            return Some(Dimension::Vh(n));
        }
    }
    if let Ok(n) = s.parse::<f32>() {
        // Raw 0 or unitless zero is 0px
        if n == 0.0 {
            return Some(Dimension::Px(0.0));
        }
    }
    None
}

/// Decodes CSS colors: `#hex`, `rgb(...)`, `rgba(...)`, `hsl(...)`, `hsla(...)`, or named colors.
pub fn parse_color(val: &str) -> Option<Color> {
    let s = val.trim();
    if s.starts_with('#') {
        return Color::parse_hex(s);
    }

    let lower = s.to_ascii_lowercase();

    if lower.starts_with("rgb(") && lower.ends_with(')') {
        let inner = &lower[4..lower.len() - 1];
        let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
        if parts.len() == 3 {
            let r = parts[0].parse::<u8>().ok()?;
            let g = parts[1].parse::<u8>().ok()?;
            let b = parts[2].parse::<u8>().ok()?;
            return Some(Color::new(r, g, b, 255));
        }
    }

    if lower.starts_with("rgba(") && lower.ends_with(')') {
        let inner = &lower[5..lower.len() - 1];
        let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
        if parts.len() == 4 {
            let r = parts[0].parse::<u8>().ok()?;
            let g = parts[1].parse::<u8>().ok()?;
            let b = parts[2].parse::<u8>().ok()?;
            let a = (parts[3].parse::<f32>().ok()?.clamp(0.0, 1.0) * 255.0).round() as u8;
            return Some(Color::new(r, g, b, a));
        }
    }

    // Common CSS color keywords
    match lower.as_str() {
        "transparent" => Some(Color::TRANSPARENT),
        "black" => Some(Color::BLACK),
        "white" => Some(Color::WHITE),
        "red" => Some(Color::RED),
        "green" => Some(Color::GREEN),
        "blue" => Some(Color::BLUE),
        _ => None,
    }
}

/// Decodes CSS 4-directional insets shorthand (e.g. `10px`, `10px 20px`, `10px 20px 30px 40px`).
pub fn parse_edge_insets_px(val: &str) -> Option<EdgeInsets<f32>> {
    let parts: Vec<f32> = val
        .split_whitespace()
        .filter_map(|p| {
            if let Some(Dimension::Px(px)) = parse_dimension(p) {
                Some(px)
            } else {
                None
            }
        })
        .collect();

    match parts.len() {
        1 => Some(EdgeInsets::all(parts[0])),
        2 => Some(EdgeInsets::symmetric(parts[0], parts[1])),
        3 => Some(EdgeInsets::new(parts[0], parts[1], parts[2], parts[1])),
        4 => Some(EdgeInsets::new(parts[0], parts[1], parts[2], parts[3])),
        _ => None,
    }
}
