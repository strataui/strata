//! Recursive-descent RSX parser converting TokenCursor into an RSX AST.

use crate::ast::*;
use crate::cursor::TokenCursor;
use proc_macro::{Delimiter, TokenStream};

const VOID_TAGS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

pub fn parse_rsx(cursor: &mut TokenCursor) -> Result<Vec<RsxNode>, String> {
    let mut nodes = Vec::new();
    while !cursor.is_eof() {
        nodes.push(parse_node(cursor)?);
    }
    Ok(nodes)
}

fn parse_node(cursor: &mut TokenCursor) -> Result<RsxNode, String> {
    if cursor.peek_punct('<') {
        parse_tag_or_fragment(cursor)
    } else if cursor.peek_group(Delimiter::Brace).is_some() {
        let g = cursor.consume_group(Delimiter::Brace).unwrap();
        Ok(RsxNode::Expression(g.stream()))
    } else if let Some(lit) = cursor.consume_literal_string() {
        Ok(RsxNode::Text(lit))
    } else if let Some(id) = cursor.consume_ident() {
        Ok(RsxNode::Text(id))
    } else if let Some(tok) = cursor.next() {
        // Fallback for stray punctuation/tokens outside tags: treat as string
        Ok(RsxNode::Text(tok.to_string()))
    } else {
        Err("Unexpected end of input while parsing RSX node".to_string())
    }
}

fn parse_tag_or_fragment(cursor: &mut TokenCursor) -> Result<RsxNode, String> {
    cursor.consume_punct('<');

    // Case 1: Fragment opening `<>`
    if cursor.consume_punct('>') {
        return parse_fragment(cursor);
    }

    // Case 2: Unexpected closing tag `</...>`
    if cursor.peek_punct('/') {
        return Err("Unexpected closing tag '</' without matching opening tag".to_string());
    }

    // Case 3: Tag path (e.g. `div` or `Sidebar` or `Icons::Plus`)
    let first_ident = cursor
        .consume_ident()
        .ok_or_else(|| "Expected tag name or identifier after '<'".to_string())?;

    let mut path = vec![first_ident];
    while cursor.peek_punct(':') && cursor.peek_punct_at(1, ':') {
        cursor.consume_punct(':');
        cursor.consume_punct(':');
        let next_id = cursor
            .consume_ident()
            .ok_or_else(|| "Expected identifier after '::' in tag path".to_string())?;
        path.push(next_id);
    }

    let is_custom_component = path.len() > 1
        || path[0]
            .chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false);

    // Parse attributes
    let attributes = parse_attributes(cursor)?;

    // Check for self-closing `/>`
    if cursor.peek_punct('/') && cursor.peek_punct_at(1, '>') {
        cursor.consume_punct('/');
        cursor.consume_punct('>');
        if is_custom_component {
            return Ok(RsxNode::Component(RsxComponent {
                path,
                props: attributes,
                children: Vec::new(),
                self_closing: true,
            }));
        } else {
            return Ok(RsxNode::Element(RsxElement {
                tag: path[0].clone(),
                attributes,
                children: Vec::new(),
                self_closing: true,
            }));
        }
    }

    // Must be closing `>`
    if !cursor.consume_punct('>') {
        return Err(format!(
            "Expected '>' or '/>' to close opening tag for '<{}>'",
            path.join("::")
        ));
    }

    // Check void elements (only for standard HTML elements)
    if !is_custom_component {
        let tag_lower = path[0].to_ascii_lowercase();
        if VOID_TAGS.contains(&tag_lower.as_str()) {
            // Check if there is an explicit closing tag e.g. `</input>`
            if cursor.peek_punct('<')
                && cursor.peek_punct_at(1, '/')
                && cursor.peek_punct_at(3, '>')
            {
                let pos = cursor.current_pos();
                cursor.consume_punct('<');
                cursor.consume_punct('/');
                if let Some(close_id) = cursor.consume_ident() {
                    if close_id.eq_ignore_ascii_case(&tag_lower) {
                        cursor.consume_punct('>');
                        return Ok(RsxNode::Element(RsxElement {
                            tag: path[0].clone(),
                            attributes,
                            children: Vec::new(),
                            self_closing: true,
                        }));
                    }
                }
                cursor.set_pos(pos);
            }
            return Ok(RsxNode::Element(RsxElement {
                tag: path[0].clone(),
                attributes,
                children: Vec::new(),
                self_closing: true,
            }));
        }
    }

    // Parse children until matching closing tag `</tag>` is encountered
    let mut children = Vec::new();
    loop {
        if cursor.is_eof() {
            return Err(format!(
                "Unclosed tag '<{}>': reached end of input without finding '</{}>'",
                path.join("::"),
                path.join("::")
            ));
        }

        // Check if closing tag
        if cursor.peek_punct('<') && cursor.peek_punct_at(1, '/') {
            let saved_pos = cursor.current_pos();
            cursor.consume_punct('<');
            cursor.consume_punct('/');

            let mut close_path = Vec::new();
            if let Some(id) = cursor.consume_ident() {
                close_path.push(id);
                while cursor.peek_punct(':') && cursor.peek_punct_at(1, ':') {
                    cursor.consume_punct(':');
                    cursor.consume_punct(':');
                    if let Some(next_id) = cursor.consume_ident() {
                        close_path.push(next_id);
                    } else {
                        break;
                    }
                }
            }

            if cursor.consume_punct('>') {
                if close_path == path {
                    break;
                } else {
                    return Err(format!(
                        "Mismatched closing tag: expected '</{}>', found '</{}>'",
                        path.join("::"),
                        close_path.join("::")
                    ));
                }
            } else {
                cursor.set_pos(saved_pos);
            }
        }

        push_child_node(&mut children, parse_node(cursor)?);
    }

    if is_custom_component {
        Ok(RsxNode::Component(RsxComponent {
            path,
            props: attributes,
            children,
            self_closing: false,
        }))
    } else {
        Ok(RsxNode::Element(RsxElement {
            tag: path[0].clone(),
            attributes,
            children,
            self_closing: false,
        }))
    }
}

fn push_child_node(children: &mut Vec<RsxNode>, next_node: RsxNode) {
    if let RsxNode::Text(next_text) = next_node {
        if let Some(RsxNode::Text(prev_text)) = children.last_mut() {
            if matches!(next_text.as_str(), "." | "," | ":" | ";" | "!" | "?" | ")" | "]")
                || prev_text.ends_with('(')
                || prev_text.ends_with('[')
            {
                prev_text.push_str(&next_text);
            } else {
                prev_text.push(' ');
                prev_text.push_str(&next_text);
            }
        } else {
            children.push(RsxNode::Text(next_text));
        }
    } else {
        children.push(next_node);
    }
}

fn parse_fragment(cursor: &mut TokenCursor) -> Result<RsxNode, String> {
    let mut children = Vec::new();
    loop {
        if cursor.is_eof() {
            return Err("Unclosed fragment '<>': reached end of input without '</>'".to_string());
        }

        if cursor.peek_punct('<') && cursor.peek_punct_at(1, '/') && cursor.peek_punct_at(2, '>') {
            cursor.consume_punct('<');
            cursor.consume_punct('/');
            cursor.consume_punct('>');
            break;
        }

        push_child_node(&mut children, parse_node(cursor)?);
    }
    Ok(RsxNode::Fragment(children))
}

fn parse_attributes(cursor: &mut TokenCursor) -> Result<Vec<RsxAttribute>, String> {
    let mut attrs = Vec::new();

    while !cursor.is_eof() {
        if cursor.peek_punct('>') || (cursor.peek_punct('/') && cursor.peek_punct_at(1, '>')) {
            break;
        }

        // Attribute name (can contain hyphens like `stroke-width`, `data-state`)
        let mut name = cursor
            .consume_ident()
            .ok_or_else(|| "Expected attribute name".to_string())?;

        while cursor.peek_punct('-') {
            cursor.consume_punct('-');
            let part = cursor
                .consume_ident()
                .ok_or_else(|| "Expected identifier after '-' in attribute name".to_string())?;
            name.push('-');
            name.push_str(&part);
        }

        // Optional value
        let value = if cursor.consume_punct('=') {
            if cursor.peek_group(Delimiter::Brace).is_some() {
                let g = cursor.consume_group(Delimiter::Brace).unwrap();
                RsxAttrValue::Expression(g.stream())
            } else if let Some(lit) = cursor.consume_literal_string() {
                RsxAttrValue::Literal(lit)
            } else if let Some(id) = cursor.consume_ident() {
                let s: TokenStream = id.parse().map_err(|e| format!("{:?}", e))?;
                RsxAttrValue::Expression(s)
            } else {
                return Err(format!(
                    "Expected string literal or '{{expr}}' for attribute '{}'",
                    name
                ));
            }
        } else {
            RsxAttrValue::Flag
        };

        attrs.push(RsxAttribute { name, value });
    }

    Ok(attrs)
}
