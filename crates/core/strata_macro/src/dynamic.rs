//! The dynamic! macro for constructing dynamic JavaScript JsValue hierarchies.

use crate::cursor::TokenCursor;
use proc_macro::{Delimiter, TokenStream, TokenTree};

pub fn expand_dynamic(input: TokenStream) -> TokenStream {
    let mut cursor = TokenCursor::new(input);

    if cursor.is_eof() {
        return "strata_js::JsValue::Undefined".parse().unwrap();
    }

    let tokens = collect_until_comma(&mut cursor);
    let code = lower_tokens(tokens);
    code.parse()
        .unwrap_or_else(|_| "strata_js::JsValue::Null".parse().unwrap())
}

fn collect_until_comma(cursor: &mut TokenCursor) -> Vec<TokenTree> {
    let mut tokens = Vec::new();
    while !cursor.is_eof() {
        if cursor.peek_punct(',') {
            break;
        }
        if let Some(tok) = cursor.next() {
            tokens.push(tok);
        }
    }
    tokens
}

fn lower_tokens(tokens: Vec<TokenTree>) -> String {
    if tokens.is_empty() {
        return "strata_js::JsValue::Undefined".to_string();
    }

    if tokens.len() == 1 {
        match &tokens[0] {
            TokenTree::Group(g) if g.delimiter() == Delimiter::Brace => {
                return parse_dynamic_object(g.stream());
            }
            TokenTree::Group(g) if g.delimiter() == Delimiter::Bracket => {
                return parse_dynamic_array(g.stream());
            }
            TokenTree::Ident(id) => match id.to_string().as_str() {
                "null" => return "strata_js::JsValue::Null".to_string(),
                "undefined" => return "strata_js::JsValue::Undefined".to_string(),
                "true" => return "strata_js::JsValue::Bool(true)".to_string(),
                "false" => return "strata_js::JsValue::Bool(false)".to_string(),
                _ => {}
            },
            _ => {}
        }
    }

    let stream: TokenStream = tokens.into_iter().collect();
    format!("strata_js::JsValue::from({{ {} }})", stream)
}

fn parse_dynamic_object(stream: TokenStream) -> String {
    let mut cursor = TokenCursor::new(stream);
    let mut entries = Vec::new();

    while !cursor.is_eof() {
        // Key can be string literal or identifier
        let key = if let Some(k) = cursor.consume_literal_string() {
            k
        } else if let Some(id) = cursor.consume_ident() {
            id
        } else {
            break;
        };

        if !cursor.consume_punct(':') {
            break;
        }

        // Value until comma
        let val_tokens = collect_until_comma(&mut cursor);
        let val_code = lower_tokens(val_tokens);
        entries.push(format!("({:?}.to_string(), {})", key, val_code));

        cursor.consume_punct(',');
    }

    format!(
        "strata_js::JsValue::Object(std::collections::BTreeMap::from([{}]))",
        entries.join(", ")
    )
}

fn parse_dynamic_array(stream: TokenStream) -> String {
    let mut cursor = TokenCursor::new(stream);
    let mut items = Vec::new();

    while !cursor.is_eof() {
        let item_tokens = collect_until_comma(&mut cursor);
        if item_tokens.is_empty() {
            break;
        }
        let item_code = lower_tokens(item_tokens);
        items.push(item_code);
        cursor.consume_punct(',');
    }

    format!("strata_js::JsValue::Array(vec![{}])", items.join(", "))
}
