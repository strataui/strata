//! Zero-dependency pure-Rust YAML subset parser for Strata configuration files.

use std::collections::HashMap;

/// Representation of a parsed YAML value.
#[derive(Debug, Clone, PartialEq)]
pub enum YamlValue {
    Null,
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Array(Vec<YamlValue>),
    Map(HashMap<String, YamlValue>),
}

impl YamlValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            YamlValue::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            YamlValue::Integer(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            YamlValue::Float(f) => Some(*f),
            YamlValue::Integer(i) => Some(*i as f64),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            YamlValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_map(&self) -> Option<&HashMap<String, YamlValue>> {
        match self {
            YamlValue::Map(m) => Some(m),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<YamlValue>> {
        match self {
            YamlValue::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn get(&self, key: &str) -> Option<&YamlValue> {
        match self {
            YamlValue::Map(m) => m.get(key),
            _ => None,
        }
    }
}

/// Parses a YAML document into a YamlValue::Map.
pub fn parse_yaml(input: &str) -> Result<YamlValue, String> {
    let mut lines = Vec::new();
    for (line_num, raw_line) in input.lines().enumerate() {
        // Strip trailing comment if present
        let mut comment_split = raw_line.split('#');
        let code_part = comment_split.next().unwrap_or("");
        let trimmed = code_part.trim_end();
        if trimmed.trim().is_empty() {
            continue;
        }
        let indent = trimmed.chars().take_while(|c| *c == ' ').count();
        let content = trimmed.trim();
        lines.push((indent, content.to_string(), line_num + 1));
    }

    if lines.is_empty() {
        return Ok(YamlValue::Map(HashMap::new()));
    }

    let mut cursor = 0;
    parse_block(&lines, &mut cursor, 0)
}

fn parse_block(
    lines: &[(usize, String, usize)],
    cursor: &mut usize,
    base_indent: usize,
) -> Result<YamlValue, String> {
    if *cursor >= lines.len() {
        return Ok(YamlValue::Null);
    }

    let (_, first_content, _) = &lines[*cursor];
    if first_content.starts_with("- ") {
        // Sequence block
        let mut array = Vec::new();
        while *cursor < lines.len() {
            let (indent, content, _) = &lines[*cursor];
            if *indent < base_indent {
                break;
            }
            if *indent == base_indent && content.starts_with("- ") {
                let item_str = content[2..].trim();
                *cursor += 1;
                if item_str.is_empty() {
                    let sub_val = parse_block(lines, cursor, base_indent + 2)?;
                    array.push(sub_val);
                } else {
                    array.push(parse_scalar(item_str));
                }
            } else {
                break;
            }
        }
        Ok(YamlValue::Array(array))
    } else {
        // Mapping block
        let mut map = HashMap::new();
        while *cursor < lines.len() {
            let (indent, content, line_num) = &lines[*cursor];
            if *indent < base_indent {
                break;
            }
            if *indent > base_indent {
                return Err(format!(
                    "Unexpected indentation at line {}: expected {}",
                    line_num, base_indent
                ));
            }

            if let Some(colon_pos) = content.find(':') {
                let key = content[..colon_pos].trim().trim_matches('"').trim_matches('\'').to_string();
                let rest = content[colon_pos + 1..].trim();
                *cursor += 1;

                if rest.is_empty() {
                    // Nested block
                    if *cursor < lines.len() && lines[*cursor].0 > *indent {
                        let child_indent = lines[*cursor].0;
                        let child_val = parse_block(lines, cursor, child_indent)?;
                        map.insert(key, child_val);
                    } else {
                        map.insert(key, YamlValue::Null);
                    }
                } else if rest.starts_with('[') && rest.ends_with(']') {
                    // Inline list
                    let list_str = &rest[1..rest.len() - 1];
                    let items = list_str
                        .split(',')
                        .map(|s| parse_scalar(s.trim()))
                        .collect();
                    map.insert(key, YamlValue::Array(items));
                } else {
                    map.insert(key, parse_scalar(rest));
                }
            } else {
                return Err(format!("Malformed YAML key-value at line {}: {}", line_num, content));
            }
        }
        Ok(YamlValue::Map(map))
    }
}

fn parse_scalar(s: &str) -> YamlValue {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        let inner = &s[1..s.len() - 1];
        return YamlValue::String(inner.to_string());
    }

    if s.eq_ignore_ascii_case("true") || s.eq_ignore_ascii_case("yes") {
        return YamlValue::Bool(true);
    }
    if s.eq_ignore_ascii_case("false") || s.eq_ignore_ascii_case("no") {
        return YamlValue::Bool(false);
    }
    if s.eq_ignore_ascii_case("null") || s == "~" {
        return YamlValue::Null;
    }

    if let Ok(i) = s.parse::<i64>() {
        return YamlValue::Integer(i);
    }
    if let Ok(f) = s.parse::<f64>() {
        return YamlValue::Float(f);
    }

    YamlValue::String(s.to_string())
}
