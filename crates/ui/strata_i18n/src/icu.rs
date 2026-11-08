//! Pure-Rust ICU MessageFormat Parser and Evaluator.

use std::collections::HashMap;

/// Plural categories according to Unicode CLDR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluralCategory {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

impl PluralCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Zero => "zero",
            Self::One => "one",
            Self::Two => "two",
            Self::Few => "few",
            Self::Many => "many",
            Self::Other => "other",
        }
    }
}

/// Abstract Syntax Tree node for ICU MessageFormat.
#[derive(Debug, Clone, PartialEq)]
pub enum IcuNode {
    Text(String),
    Variable(String),
    Hash(String), // The '#' token in plurals, referencing parent count variable
    Plural {
        variable: String,
        offset: usize,
        arms: Vec<(String, Vec<IcuNode>)>,
    },
    Select {
        variable: String,
        arms: Vec<(String, Vec<IcuNode>)>,
    },
}

/// ICU MessageFormat AST container.
#[derive(Debug, Clone, PartialEq)]
pub struct IcuMessage {
    nodes: Vec<IcuNode>,
}

impl IcuMessage {
    /// Parse an ICU MessageFormat string into an executable AST.
    pub fn parse(input: &str) -> Result<Self, String> {
        let mut chars = input.chars().peekable();
        let nodes = parse_message(&mut chars, 0, None)?;
        Ok(Self { nodes })
    }

    /// Format the message with provided string and numeric arguments.
    pub fn format(&self, args: &HashMap<String, IcuValue>, language: &str) -> String {
        let mut out = String::new();
        format_nodes(&self.nodes, args, language, None, &mut out);
        out
    }
}

/// Value passed to ICU message formatters.
#[derive(Debug, Clone, PartialEq)]
pub enum IcuValue {
    String(String),
    Number(f64),
    Int(i64),
}

impl From<&str> for IcuValue {
    fn from(s: &str) -> Self {
        Self::String(s.to_string())
    }
}

impl From<String> for IcuValue {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<i64> for IcuValue {
    fn from(n: i64) -> Self {
        Self::Int(n)
    }
}

impl From<usize> for IcuValue {
    fn from(n: usize) -> Self {
        Self::Int(n as i64)
    }
}

impl From<i32> for IcuValue {
    fn from(n: i32) -> Self {
        Self::Int(n as i64)
    }
}

impl From<u32> for IcuValue {
    fn from(n: u32) -> Self {
        Self::Int(n as i64)
    }
}

impl From<u64> for IcuValue {
    fn from(n: u64) -> Self {
        Self::Int(n as i64)
    }
}

impl From<isize> for IcuValue {
    fn from(n: isize) -> Self {
        Self::Int(n as i64)
    }
}

impl From<f64> for IcuValue {
    fn from(n: f64) -> Self {
        Self::Number(n)
    }
}

impl IcuValue {
    pub fn as_f64(&self) -> f64 {
        match self {
            Self::Number(n) => *n,
            Self::Int(n) => *n as f64,
            Self::String(s) => s.parse().unwrap_or(0.0),
        }
    }

    pub fn to_formatted_string(&self) -> String {
        match self {
            Self::String(s) => s.clone(),
            Self::Int(n) => n.to_string(),
            Self::Number(n) => {
                if n.fract() == 0.0 {
                    format!("{:.0}", n)
                } else {
                    format!("{n}")
                }
            }
        }
    }
}

fn parse_message<I>(
    chars: &mut std::iter::Peekable<I>,
    depth: usize,
    parent_var: Option<&str>,
) -> Result<Vec<IcuNode>, String>
where
    I: Iterator<Item = char>,
{
    let mut nodes = Vec::new();
    let mut text_buf = String::new();

    while let Some(&c) = chars.peek() {
        if c == '}' && depth > 0 {
            break;
        }

        if c == '{' {
            chars.next(); // consume '{'
            if !text_buf.is_empty() {
                nodes.push(IcuNode::Text(std::mem::take(&mut text_buf)));
            }

            let element = parse_element(chars, depth + 1, parent_var)?;
            nodes.push(element);
        } else if c == '#' && parent_var.is_some() {
            chars.next(); // consume '#'
            if !text_buf.is_empty() {
                nodes.push(IcuNode::Text(std::mem::take(&mut text_buf)));
            }
            nodes.push(IcuNode::Hash(parent_var.unwrap().to_string()));
        } else {
            text_buf.push(chars.next().unwrap());
        }
    }

    if !text_buf.is_empty() {
        nodes.push(IcuNode::Text(text_buf));
    }

    Ok(nodes)
}

fn parse_element<I>(
    chars: &mut std::iter::Peekable<I>,
    depth: usize,
    parent_var: Option<&str>,
) -> Result<IcuNode, String>
where
    I: Iterator<Item = char>,
{
    let mut ident = String::new();
    while let Some(&c) = chars.peek() {
        if c == ',' || c == '}' {
            break;
        }
        ident.push(chars.next().unwrap());
    }

    let variable = ident.trim().to_string();
    if variable.is_empty() {
        return Err("Empty element identifier in ICU message".into());
    }

    match chars.peek() {
        Some(&'}') => {
            chars.next(); // consume '}'
            Ok(IcuNode::Variable(variable))
        }
        Some(&',') => {
            chars.next(); // consume ','
            let mut element_type = String::new();
            while let Some(&c) = chars.peek() {
                if c == ',' || c == '{' || c == '}' {
                    break;
                }
                element_type.push(chars.next().unwrap());
            }

            let element_type = element_type.trim().to_ascii_lowercase();

            match element_type.as_str() {
                "plural" => {
                    let mut offset = 0;
                    if chars.peek() == Some(&',') {
                        chars.next(); // consume ','
                    }

                    let arms = parse_arms(chars, depth, Some(&variable), &mut offset)?;
                    if chars.peek() == Some(&'}') {
                        chars.next();
                    }
                    Ok(IcuNode::Plural {
                        variable,
                        offset,
                        arms,
                    })
                }
                "select" => {
                    if chars.peek() == Some(&',') {
                        chars.next(); // consume ','
                    }
                    let mut dummy_offset = 0;
                    let arms = parse_arms(chars, depth, parent_var, &mut dummy_offset)?;
                    if chars.peek() == Some(&'}') {
                        chars.next();
                    }
                    Ok(IcuNode::Select { variable, arms })
                }
                other => Err(format!("Unsupported ICU element type: '{other}'")),
            }
        }
        _ => Err("Unexpected end of ICU element".into()),
    }
}

fn parse_arms<I>(
    chars: &mut std::iter::Peekable<I>,
    depth: usize,
    parent_var: Option<&str>,
    offset: &mut usize,
) -> Result<Vec<(String, Vec<IcuNode>)>, String>
where
    I: Iterator<Item = char>,
{
    let mut arms = Vec::new();

    loop {
        skip_whitespace(chars);
        match chars.peek() {
            None | Some(&'}') => break,
            _ => {}
        }

        let mut selector = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() || c == '{' {
                break;
            }
            selector.push(chars.next().unwrap());
        }

        let selector = selector.trim().to_string();
        if selector.is_empty() {
            break;
        }

        // Check for offset:N
        if let Some(rest) = selector.strip_prefix("offset:") {
            if let Ok(n) = rest.trim().parse::<usize>() {
                *offset = n;
            }
            continue;
        }

        skip_whitespace(chars);
        if chars.peek() != Some(&'{') {
            return Err(format!("Expected '{{' after arm selector '{selector}'"));
        }
        chars.next(); // consume '{'

        let arm_nodes = parse_message(chars, depth + 1, parent_var)?;
        if chars.peek() != Some(&'}') {
            return Err(format!("Expected '}}' closing arm '{selector}'"));
        }
        chars.next(); // consume '}'

        arms.push((selector, arm_nodes));
    }

    Ok(arms)
}

fn skip_whitespace<I>(chars: &mut std::iter::Peekable<I>)
where
    I: Iterator<Item = char>,
{
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else {
            break;
        }
    }
}

fn format_nodes(
    nodes: &[IcuNode],
    args: &HashMap<String, IcuValue>,
    language: &str,
    parent_count: Option<f64>,
    out: &mut String,
) {
    for node in nodes {
        match node {
            IcuNode::Text(t) => out.push_str(t),
            IcuNode::Variable(var) => {
                if let Some(val) = args.get(var) {
                    out.push_str(&val.to_formatted_string());
                } else {
                    out.push_str(&format!("{{{var}}}"));
                }
            }
            IcuNode::Hash(var) => {
                let count = parent_count
                    .or_else(|| args.get(var).map(|v| v.as_f64()))
                    .unwrap_or(0.0);
                if count.fract() == 0.0 {
                    out.push_str(&format!("{:.0}", count));
                } else {
                    out.push_str(&format!("{count}"));
                }
            }
            IcuNode::Select { variable, arms } => {
                let val_str = args
                    .get(variable)
                    .map(|v| v.to_formatted_string())
                    .unwrap_or_default();

                // Find matching selector or fallback to "other"
                let matched = arms
                    .iter()
                    .find(|(sel, _)| sel.eq_ignore_ascii_case(&val_str))
                    .or_else(|| arms.iter().find(|(sel, _)| sel == "other"))
                    .or_else(|| arms.first());

                if let Some((_, sub_nodes)) = matched {
                    format_nodes(sub_nodes, args, language, parent_count, out);
                }
            }
            IcuNode::Plural {
                variable,
                offset,
                arms,
            } => {
                let raw_count = args.get(variable).map(|v| v.as_f64()).unwrap_or(0.0);
                let effective_count = (raw_count - *offset as f64).max(0.0);

                let exact_selector = format!("={:.0}", raw_count);
                let cat = resolve_cldr_plural(language, effective_count);

                let matched = arms
                    .iter()
                    .find(|(sel, _)| sel == &exact_selector)
                    .or_else(|| arms.iter().find(|(sel, _)| sel.eq_ignore_ascii_case(cat.as_str())))
                    .or_else(|| arms.iter().find(|(sel, _)| sel == "other"))
                    .or_else(|| arms.first());

                if let Some((_, sub_nodes)) = matched {
                    format_nodes(sub_nodes, args, language, Some(effective_count), out);
                }
            }
        }
    }
}

/// Simple CLDR Plural Category Rule Resolver
fn resolve_cldr_plural(language: &str, count: f64) -> PluralCategory {
    let int_count = count as i64;
    let is_int = count.fract() == 0.0;

    match language {
        "fr" | "pt" => {
            if is_int && (int_count == 0 || int_count == 1) {
                PluralCategory::One
            } else {
                PluralCategory::Other
            }
        }
        "ar" => {
            if is_int {
                if int_count == 0 {
                    PluralCategory::Zero
                } else if int_count == 1 {
                    PluralCategory::One
                } else if int_count == 2 {
                    PluralCategory::Two
                } else if (3..=10).contains(&(int_count % 100)) {
                    PluralCategory::Few
                } else if (11..=99).contains(&(int_count % 100)) {
                    PluralCategory::Many
                } else {
                    PluralCategory::Other
                }
            } else {
                PluralCategory::Other
            }
        }
        "ru" | "uk" => {
            if is_int {
                let rem10 = int_count % 10;
                let rem100 = int_count % 100;
                if rem10 == 1 && rem100 != 11 {
                    PluralCategory::One
                } else if (2..=4).contains(&rem10) && !(12..=14).contains(&rem100) {
                    PluralCategory::Few
                } else {
                    PluralCategory::Many
                }
            } else {
                PluralCategory::Other
            }
        }
        _ => {
            // English, German, Spanish, etc.
            if is_int && int_count == 1 {
                PluralCategory::One
            } else {
                PluralCategory::Other
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variable_interpolation() {
        let msg = IcuMessage::parse("Hello, {name}!").unwrap();
        let mut args = HashMap::new();
        args.insert("name".to_string(), "Alex".into());

        assert_eq!(msg.format(&args, "en"), "Hello, Alex!");
    }

    #[test]
    fn test_select_gender() {
        let template = "{gender, select, male {He liked your post.} female {She liked your post.} other {They liked your post.}}";
        let msg = IcuMessage::parse(template).unwrap();

        let mut args = HashMap::new();
        args.insert("gender".to_string(), "female".into());
        assert_eq!(msg.format(&args, "en"), "She liked your post.");

        args.insert("gender".to_string(), "unknown".into());
        assert_eq!(msg.format(&args, "en"), "They liked your post.");
    }

    #[test]
    fn test_plural_with_exact_and_hash() {
        let template = "{count, plural, =0 {No items} one {# item} other {# items}}";
        let msg = IcuMessage::parse(template).unwrap();

        let mut args = HashMap::new();
        args.insert("count".to_string(), 0.into());
        assert_eq!(msg.format(&args, "en"), "No items");

        args.insert("count".to_string(), 1.into());
        assert_eq!(msg.format(&args, "en"), "1 item");

        args.insert("count".to_string(), 42.into());
        assert_eq!(msg.format(&args, "en"), "42 items");
    }

    #[test]
    fn test_nested_select_and_plural() {
        let template = "{gender, select, female {She has {count, plural, one {# message} other {# messages}}.} other {They have {count, plural, one {# message} other {# messages}}.}}";
        let msg = IcuMessage::parse(template).unwrap();

        let mut args = HashMap::new();
        args.insert("gender".to_string(), "female".into());
        args.insert("count".to_string(), 3.into());

        assert_eq!(msg.format(&args, "en"), "She has 3 messages.");
    }
}
