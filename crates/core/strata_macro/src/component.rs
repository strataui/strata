//! Implementation of the #[component] attribute macro.
//! Generates a companion builder struct in the type namespace to power named JSX props
//! with defaults and children while preserving the callable function in the value namespace.

use crate::cursor::TokenCursor;
use proc_macro::{Delimiter, TokenStream, TokenTree};

struct ParamInfo {
    name: String,
    ty: String,
    is_option: bool,
}

pub fn expand_component(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut cursor = TokenCursor::new(item.clone());

    // 1. Consume leading outer attributes (e.g. `#[allow(...)]`)
    let mut outer_attrs = Vec::new();
    while cursor.peek_punct('#') {
        let hash = cursor.next().unwrap();
        outer_attrs.push(hash.to_string());
        if let Some(g) = cursor.consume_group(Delimiter::Bracket) {
            outer_attrs.push(format!("[{}]", g.stream()));
        }
    }

    // 2. Consume visibility (e.g. `pub` or `pub(crate)`)
    let mut vis = String::new();
    if cursor.peek_ident().as_deref() == Some("pub") {
        vis.push_str("pub");
        cursor.consume_ident();
        if cursor.peek_group(Delimiter::Parenthesis).is_some() {
            let g = cursor.consume_group(Delimiter::Parenthesis).unwrap();
            vis.push_str(&format!("({})", g.stream()));
        }
        vis.push(' ');
    }

    // 3. Must see `fn`
    if cursor.peek_ident().as_deref() != Some("fn") {
        // If not a function, return item unchanged
        return item;
    }
    cursor.consume_ident(); // consume `fn`

    // 4. Function name
    let fn_name = match cursor.consume_ident() {
        Some(name) => name,
        None => return item,
    };

    // 5. Consume optional generics e.g. `<T>`
    let mut generics = String::new();
    if cursor.consume_punct('<') {
        let mut depth = 1;
        generics.push('<');
        while !cursor.is_eof() && depth > 0 {
            if cursor.peek_punct('<') {
                depth += 1;
            } else if cursor.peek_punct('>') {
                depth -= 1;
            }
            if let Some(tok) = cursor.next() {
                generics.push_str(&tok.to_string());
                generics.push(' ');
            }
        }
    }

    // 6. Parameter list group
    let params_group = match cursor.consume_group(Delimiter::Parenthesis) {
        Some(g) => g,
        None => return item,
    };

    let params = parse_component_params(params_group.stream());

    // Generate companion struct and builder
    let builder_name = format!("{}Builder", fn_name);

    let mut builder_fields = Vec::new();
    let mut builder_setters = Vec::new();
    let mut build_args = Vec::new();
    let mut has_children_param = false;

    for param in &params {
        if param.name == "children" {
            has_children_param = true;
        }

        if param.name == "onclick" {
            builder_fields.push("pub onclick: Option<std::rc::Rc<dyn Fn()>>,".to_string());
            builder_setters.push(format!(
                "#[allow(dead_code)]\n{} fn onclick<Args>(mut self, handler: impl strata_dom::EventHandlerMarker<Args> + 'static) -> Self {{\n    self.onclick = Some(std::rc::Rc::new(move || handler.call_handler(strata_dom::Event::default())));\n    self\n}}",
                vis
            ));
            build_args.push("self.onclick".to_string());
        } else if param.is_option {
            builder_fields.push(format!("pub {}: {},", param.name, param.ty));
            builder_setters.push(format!(
                "#[allow(dead_code)]\n{} fn {}(mut self, val: impl Into<{}>) -> Self {{\n    self.{} = val.into();\n    self\n}}",
                vis, param.name, param.ty, param.name
            ));
            build_args.push(format!("self.{}", param.name));
        } else {
            builder_fields.push(format!("pub {}: Option<{}>,", param.name, param.ty));
            builder_setters.push(format!(
                "#[allow(dead_code)]\n{} fn {}(mut self, val: impl Into<{}>) -> Self {{\n    self.{} = Some(val.into());\n    self\n}}",
                vis, param.name, param.ty, param.name
            ));
            build_args.push(format!(
                "self.{}.expect(\"Prop `{}` is required for <{}>\")",
                param.name, param.name, fn_name
            ));
        }
    }

    // If the function didn't explicitly have a `children` param, add slot to builder
    let extra_children_field = if !has_children_param {
        "pub children: Option<Vec<strata_dom::NodeBlueprint>>,"
    } else {
        ""
    };

    let extra_children_setter = if !has_children_param {
        format!(
            "#[allow(dead_code)]\n{} fn children(mut self, children: Vec<strata_dom::NodeBlueprint>) -> Self {{\n    self.children = Some(children);\n    self\n}}",
            vis
        )
    } else {
        String::new()
    };

    let companion_code = format!(
        r#"
// Companion Type Namespace Item
#[allow(non_snake_case)]
{vis}struct {fn_name} {{}}

impl {fn_name} {{
    #[allow(dead_code)]
    {vis}fn builder() -> {builder_name} {{
        {builder_name}::default()
    }}
}}

// Companion Builder Struct
#[derive(Default)]
#[allow(non_snake_case, dead_code)]
{vis}struct {builder_name} {{
    {fields}
    {extra_children_field}
}}

impl {builder_name} {{
    {setters}
    {extra_children_setter}

    #[allow(dead_code)]
    {vis}fn build(self) -> strata_dom::View {{
        {fn_name}(
            {args}
        )
    }}
}}
"#,
        vis = vis,
        fn_name = fn_name,
        builder_name = builder_name,
        fields = builder_fields.join("\n    "),
        extra_children_field = extra_children_field,
        setters = builder_setters.join("\n\n    "),
        extra_children_setter = extra_children_setter,
        args = build_args.join(",\n            ")
    );

    // Combine original function and companion builder
    let full_expanded = format!("{}\n{}", item, companion_code);

    match full_expanded.parse() {
        Ok(stream) => stream,
        Err(_) => item, // Fallback to original item if parse error
    }
}

fn parse_component_params(stream: TokenStream) -> Vec<ParamInfo> {
    let mut cursor = TokenCursor::new(stream);
    let mut params = Vec::new();

    while !cursor.is_eof() {
        // Skip leading `mut`
        if cursor.peek_ident().as_deref() == Some("mut") {
            cursor.consume_ident();
        }

        let name = match cursor.consume_ident() {
            Some(n) => n,
            None => break,
        };

        if !cursor.consume_punct(':') {
            break;
        }

        // Collect type tokens until comma `,` at top level
        let mut ty_tokens = Vec::new();
        let mut angle_depth = 0;
        let mut paren_depth = 0;

        while !cursor.is_eof() {
            if cursor.peek_punct('<') {
                angle_depth += 1;
            } else if cursor.peek_punct('>') && angle_depth > 0 {
                angle_depth -= 1;
            } else if cursor.peek_punct(',') && angle_depth == 0 && paren_depth == 0 {
                cursor.consume_punct(',');
                break;
            }

            if let Some(tok) = cursor.next() {
                match &tok {
                    TokenTree::Group(_g) => {
                        paren_depth += 1;
                        ty_tokens.push(tok.to_string());
                        paren_depth -= 1;
                    }
                    _ => {
                        ty_tokens.push(tok.to_string());
                    }
                }
            }
        }

        let ty_str = format_type_tokens(&ty_tokens);
        let ty_trimmed = ty_str.trim();
        let is_option = ty_trimmed.starts_with("Option <")
            || ty_trimmed.starts_with("Option<")
            || ty_trimmed.starts_with("std :: option :: Option");

        params.push(ParamInfo {
            name,
            ty: ty_str,
            is_option,
        });
    }

    params
}

fn format_type_tokens(tokens: &[String]) -> String {
    let mut out = String::new();
    let mut prev_was_quote = false;

    for tok in tokens {
        if prev_was_quote {
            out.push_str(tok);
            prev_was_quote = false;
        } else if tok == "'" {
            if !out.is_empty() && !out.ends_with('&') && !out.ends_with('<') {
                out.push(' ');
            }
            out.push('\'');
            prev_was_quote = true;
        } else if tok == ":" {
            out.push(':');
        } else {
            if !out.is_empty()
                && !out.ends_with('<')
                && !out.ends_with('&')
                && !out.ends_with(':')
                && tok != ">"
                && tok != ","
            {
                out.push(' ');
            }
            out.push_str(tok);
        }
    }

    out
}
