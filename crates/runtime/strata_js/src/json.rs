//! Zero-dependency, pure-Rust JSON parser and serializer for JsValue.

use std::collections::BTreeMap;
use crate::JsValue;

/// Parses a JSON string into a dynamic `JsValue`.
pub fn parse_json(input: &str) -> Result<JsValue, String> {
    let mut chars = input.chars().peekable();
    skip_whitespace(&mut chars);
    let val = parse_value(&mut chars)?;
    skip_whitespace(&mut chars);
    if chars.peek().is_some() {
        return Err("Unexpected trailing characters after JSON value".to_string());
    }
    Ok(val)
}

/// Serializes a `JsValue` into a standard JSON string.
pub fn stringify_json(val: &JsValue) -> String {
    let mut out = String::new();
    serialize_value(val, &mut out);
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

fn parse_value<I: Iterator<Item = char>>(
    chars: &mut std::iter::Peekable<I>,
) -> Result<JsValue, String> {
    skip_whitespace(chars);
    match chars.peek() {
        Some('{') => parse_object(chars),
        Some('[') => parse_array(chars),
        Some('"') => parse_string(chars).map(JsValue::String),
        Some('t') | Some('f') => parse_bool(chars),
        Some('n') => parse_null(chars),
        Some('u') => parse_undefined(chars),
        Some(&c) if c == '-' || c.is_ascii_digit() => parse_number(chars),
        Some(&c) => Err(format!("Unexpected character '{}' while parsing JSON", c)),
        None => Err("Unexpected end of input while parsing JSON".to_string()),
    }
}

fn parse_object<I: Iterator<Item = char>>(
    chars: &mut std::iter::Peekable<I>,
) -> Result<JsValue, String> {
    chars.next(); // consume '{'
    skip_whitespace(chars);

    let mut map = BTreeMap::new();

    if let Some(&'}') = chars.peek() {
        chars.next();
        return Ok(JsValue::Object(map));
    }

    loop {
        skip_whitespace(chars);
        let key = match chars.peek() {
            Some('"') => parse_string(chars)?,
            Some(&c) => return Err(format!("Expected string key in object, found '{}'", c)),
            None => return Err("Unexpected end of object".to_string()),
        };

        skip_whitespace(chars);
        if chars.next() != Some(':') {
            return Err("Expected ':' after object key".to_string());
        }

        let val = parse_value(chars)?;
        map.insert(key, val);

        skip_whitespace(chars);
        match chars.next() {
            Some(',') => {
                skip_whitespace(chars);
                // Allow trailing comma gracefully
                if let Some(&'}') = chars.peek() {
                    chars.next();
                    break;
                }
            }
            Some('}') => break,
            Some(c) => return Err(format!("Expected ',' or '}}' in object, found '{}'", c)),
            None => return Err("Unterminated object: missing '}'".to_string()),
        }
    }

    Ok(JsValue::Object(map))
}

fn parse_array<I: Iterator<Item = char>>(
    chars: &mut std::iter::Peekable<I>,
) -> Result<JsValue, String> {
    chars.next(); // consume '['
    skip_whitespace(chars);

    let mut items = Vec::new();

    if let Some(&']') = chars.peek() {
        chars.next();
        return Ok(JsValue::Array(items));
    }

    loop {
        let val = parse_value(chars)?;
        items.push(val);

        skip_whitespace(chars);
        match chars.next() {
            Some(',') => {
                skip_whitespace(chars);
                // Allow trailing comma gracefully
                if let Some(&']') = chars.peek() {
                    chars.next();
                    break;
                }
            }
            Some(']') => break,
            Some(c) => return Err(format!("Expected ',' or ']' in array, found '{}'", c)),
            None => return Err("Unterminated array: missing ']'".to_string()),
        }
    }

    Ok(JsValue::Array(items))
}

fn parse_string<I: Iterator<Item = char>>(
    chars: &mut std::iter::Peekable<I>,
) -> Result<String, String> {
    chars.next(); // consume opening '"'
    let mut s = String::new();

    while let Some(c) = chars.next() {
        match c {
            '"' => return Ok(s),
            '\\' => match chars.next() {
                Some('"') => s.push('"'),
                Some('\\') => s.push('\\'),
                Some('/') => s.push('/'),
                Some('b') => s.push('\x08'),
                Some('f') => s.push('\x0C'),
                Some('n') => s.push('\n'),
                Some('r') => s.push('\r'),
                Some('t') => s.push('\t'),
                Some('u') => {
                    let mut hex = String::with_capacity(4);
                    for _ in 0..4 {
                        if let Some(h) = chars.next() {
                            hex.push(h);
                        } else {
                            return Err("Unterminated unicode escape in string".to_string());
                        }
                    }
                    if let Ok(code) = u32::from_str_radix(&hex, 16) {
                        if let Some(ch) = char::from_u32(code) {
                            s.push(ch);
                        } else {
                            return Err(format!("Invalid unicode codepoint: \\u{}", hex));
                        }
                    } else {
                        return Err(format!("Invalid hex digits in \\u{}", hex));
                    }
                }
                Some(other) => return Err(format!("Invalid escape character: \\{}", other)),
                None => return Err("Unterminated escape sequence in string".to_string()),
            },
            other => s.push(other),
        }
    }

    Err("Unterminated string: missing closing quote".to_string())
}

fn parse_number<I: Iterator<Item = char>>(
    chars: &mut std::iter::Peekable<I>,
) -> Result<JsValue, String> {
    let mut num_str = String::new();

    if let Some(&'-') = chars.peek() {
        num_str.push(chars.next().unwrap());
    }

    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E' || c == '+' || c == '-' {
            num_str.push(chars.next().unwrap());
        } else {
            break;
        }
    }

    match num_str.parse::<f64>() {
        Ok(n) => Ok(JsValue::Number(n)),
        Err(_) => Err(format!("Invalid numeric literal: '{}'", num_str)),
    }
}

fn parse_bool<I: Iterator<Item = char>>(
    chars: &mut std::iter::Peekable<I>,
) -> Result<JsValue, String> {
    let mut ident = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_ascii_alphabetic() {
            ident.push(chars.next().unwrap());
        } else {
            break;
        }
    }

    match ident.as_str() {
        "true" => Ok(JsValue::Bool(true)),
        "false" => Ok(JsValue::Bool(false)),
        _ => Err(format!("Unexpected identifier: '{}'", ident)),
    }
}

fn parse_null<I: Iterator<Item = char>>(
    chars: &mut std::iter::Peekable<I>,
) -> Result<JsValue, String> {
    let mut ident = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_ascii_alphabetic() {
            ident.push(chars.next().unwrap());
        } else {
            break;
        }
    }

    if ident == "null" {
        Ok(JsValue::Null)
    } else {
        Err(format!("Unexpected identifier: '{}'", ident))
    }
}

fn parse_undefined<I: Iterator<Item = char>>(
    chars: &mut std::iter::Peekable<I>,
) -> Result<JsValue, String> {
    let mut ident = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_ascii_alphabetic() {
            ident.push(chars.next().unwrap());
        } else {
            break;
        }
    }

    if ident == "undefined" {
        Ok(JsValue::Undefined)
    } else {
        Err(format!("Unexpected identifier: '{}'", ident))
    }
}

fn serialize_value(val: &JsValue, out: &mut String) {
    match val {
        JsValue::Undefined | JsValue::Null => out.push_str("null"),
        JsValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        JsValue::Number(n) => {
            if n.is_nan() || n.is_infinite() {
                out.push_str("null");
            } else if n.fract() == 0.0 && *n >= (i64::MIN as f64) && *n <= (i64::MAX as f64) {
                out.push_str(&format!("{}", *n as i64));
            } else {
                out.push_str(&format!("{}", n));
            }
        }
        JsValue::String(s) => {
            out.push('"');
            for c in s.chars() {
                match c {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    '\x08' => out.push_str("\\b"),
                    '\x0C' => out.push_str("\\f"),
                    other if other.is_control() => {
                        out.push_str(&format!("\\u{:04x}", other as u32));
                    }
                    other => out.push(other),
                }
            }
            out.push('"');
        }
        JsValue::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                serialize_value(item, out);
            }
            out.push(']');
        }
        JsValue::Object(map) => {
            out.push('{');
            for (i, (k, v)) in map.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('"');
                for c in k.chars() {
                    if c == '"' {
                        out.push_str("\\\"");
                    } else if c == '\\' {
                        out.push_str("\\\\");
                    } else {
                        out.push(c);
                    }
                }
                out.push_str("\":");
                serialize_value(v, out);
            }
            out.push('}');
        }
    }
}
