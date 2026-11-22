//! Implementation of the `#[component]` attribute macro.
//!
//! Generates a companion builder struct in the type namespace to power named JSX props
//! with defaults, generics, `Option<String>` ergonomic setters, `{..props}` spread merging,
//! reconciliation keys, and children, while wrapping the component body in a
//! `strata_dom::ComponentScopeGuard` so React-style hooks persist per component instance.

use crate::cursor::TokenCursor;
use proc_macro::{Delimiter, TokenStream, TokenTree};

#[derive(Debug, Clone)]
enum ParamDefault {
    None,
    TraitDefault,
    Expr(String),
}

struct ParamInfo {
    name: String,
    ty: String,
    is_option: bool,
    is_option_string: bool,
    default: ParamDefault,
    /// Parameter tokens with `#[prop(...)]` stripped, ready to emit in the `fn` signature.
    clean_param_code: String,
}

#[derive(Debug, Clone)]
struct GenericParam {
    /// Exact parameter declaration as written (including bounds and any `= Default`).
    raw: String,
    /// Declaration with any `= Default` removed (valid inside `impl<...>`).
    impl_decl: String,
    /// Bare parameter identifier / lifetime name (valid in type-argument position).
    ident: String,
}

#[derive(Debug, Default)]
struct ParsedGenerics {
    lifetimes: Vec<GenericParam>,
    types: Vec<GenericParam>,
    consts: Vec<GenericParam>,
}

impl ParsedGenerics {
    fn is_empty(&self) -> bool {
        self.lifetimes.is_empty() && self.types.is_empty() && self.consts.is_empty()
    }

    fn all_ordered(&self) -> impl Iterator<Item = &GenericParam> {
        self.lifetimes
            .iter()
            .chain(self.types.iter())
            .chain(self.consts.iter())
    }

    /// `<'a, T: Clone = String, const N: usize = 4>` (or empty string if none).
    fn struct_generics(&self) -> String {
        if self.is_empty() {
            return String::new();
        }
        let items: Vec<&str> = self.all_ordered().map(|p| p.raw.as_str()).collect();
        format!("<{}>", items.join(", "))
    }

    /// `<'a, T: Clone, const N: usize>` (or empty string if none).
    fn impl_generics(&self) -> String {
        if self.is_empty() {
            return String::new();
        }
        let items: Vec<&str> = self.all_ordered().map(|p| p.impl_decl.as_str()).collect();
        format!("<{}>", items.join(", "))
    }

    /// `<'a, T, N>` (or empty string if none).
    fn ty_generics(&self) -> String {
        if self.is_empty() {
            return String::new();
        }
        let items: Vec<&str> = self.all_ordered().map(|p| p.ident.as_str()).collect();
        format!("<{}>", items.join(", "))
    }

    /// `::<'a, T, N>` (or empty string if none).
    fn turbofish(&self) -> String {
        if self.is_empty() {
            return String::new();
        }
        format!("::{}", self.ty_generics())
    }

    /// Builds an `impl<...>` header inserting an extra type parameter (e.g. `__F: Fn() + 'static`)
    /// after lifetimes and type parameters, and before any const parameters.
    fn impl_generics_with_extra_type(&self, extra_type_param: &str) -> String {
        let mut items: Vec<String> = Vec::new();
        for p in &self.lifetimes {
            items.push(p.impl_decl.clone());
        }
        for p in &self.types {
            items.push(p.impl_decl.clone());
        }
        items.push(extra_type_param.to_string());
        for p in &self.consts {
            items.push(p.impl_decl.clone());
        }
        format!("<{}>", items.join(", "))
    }

    /// Builds a trait parameter list `<'a, T: Bound, __Marker, const N: usize>` placing
    /// `__Marker` right before any const parameters so Rust's lifetime/type/const ordering holds.
    fn trait_generics_with_marker(&self) -> String {
        let mut items: Vec<String> = Vec::new();
        for p in &self.lifetimes {
            items.push(p.impl_decl.clone());
        }
        for p in &self.types {
            items.push(p.impl_decl.clone());
        }
        if self.consts.is_empty() {
            items.push("__Marker = ()".to_string());
        } else {
            items.push("__Marker".to_string());
        }
        for p in &self.consts {
            items.push(p.impl_decl.clone());
        }
        format!("<{}>", items.join(", "))
    }

    /// Builds trait type arguments `<'a, T, <marker_ty>, N>` matching `trait_generics_with_marker`.
    fn trait_args_with_marker(&self, marker_ty: &str) -> String {
        let mut items: Vec<String> = Vec::new();
        for p in &self.lifetimes {
            items.push(p.ident.clone());
        }
        for p in &self.types {
            items.push(p.ident.clone());
        }
        items.push(marker_ty.to_string());
        for p in &self.consts {
            items.push(p.ident.clone());
        }
        format!("<{}>", items.join(", "))
    }

    /// `PhantomData<(...)>` type consuming every declared generic parameter so `struct` items
    /// never trigger `E0392` and never require `T: Default` or `T: Sized`.
    fn phantom_type(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let mut elems = Vec::new();
        for p in &self.lifetimes {
            elems.push(format!("fn(&{} ())", p.ident));
        }
        for p in &self.types {
            elems.push(format!("fn(*const {})", p.ident));
        }
        for p in &self.consts {
            elems.push(format!("[(); {}]", p.ident));
        }
        Some(format!("std::marker::PhantomData<({},)>", elems.join(", ")))
    }
}

pub fn expand_component(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut cursor = TokenCursor::new(item.clone());

    // 1. Consume leading outer attributes (e.g. `#[allow(...)]`, `#[doc = "..."]`)
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
        return item;
    }
    cursor.consume_ident(); // consume `fn`

    // 4. Function name
    let fn_name = match cursor.consume_ident() {
        Some(name) => name,
        None => return item,
    };

    // 5. Consume optional generics e.g. `<'a, T: Clone + 'static, const N: usize>`
    let parsed_generics = parse_optional_fn_generics(&mut cursor);

    // 6. Parameter list group `(...)`
    let params_group = match cursor.consume_group(Delimiter::Parenthesis) {
        Some(g) => g,
        None => return item,
    };

    let params = parse_component_params(params_group.stream());

    // 7. Return type + optional `where` clause up to body `{ ... }`
    let mut pre_body_tokens: Vec<TokenTree> = Vec::new();
    while !cursor.is_eof() && cursor.peek_group(Delimiter::Brace).is_none() {
        if let Some(tok) = cursor.next() {
            pre_body_tokens.push(tok);
        }
    }
    let body_group = match cursor.consume_group(Delimiter::Brace) {
        Some(g) => g,
        None => return item,
    };

    let (ret_part, where_clause) = split_ret_and_where(&pre_body_tokens);

    // Reconstruct the function with `#[prop(...)]` stripped and `ComponentScopeGuard` injected
    // right after any leading inner `#![...]` attributes.
    let clean_params_joined: String = params
        .iter()
        .map(|p| p.clean_param_code.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let (inner_attrs, body_rest) = split_leading_inner_attrs(body_group.stream());
    let fn_rebuilt = format!(
        "{outer_attrs}\n{vis}fn {fn_name}{struct_generics}({clean_params}){ret_part}{where_pad}{where_clause} {{\n    {inner_attrs}\n    let __strata_component_scope_guard = strata_dom::ComponentScopeGuard::enter(concat!(module_path!(), \"::\", stringify!({fn_name})));\n    {body_rest}\n}}",
        outer_attrs = outer_attrs.join("\n"),
        vis = vis,
        fn_name = fn_name,
        struct_generics = parsed_generics.struct_generics(),
        clean_params = clean_params_joined,
        ret_part = if ret_part.is_empty() {
            String::new()
        } else {
            format!(" {}", ret_part)
        },
        where_pad = if where_clause.is_empty() { "" } else { "\n" },
        where_clause = where_clause,
        inner_attrs = inner_attrs,
        body_rest = body_rest,
    );

    // Generate companion struct and builder
    let builder_name = format!("{}Builder", fn_name);
    let struct_generics = parsed_generics.struct_generics();
    let impl_generics = parsed_generics.impl_generics();
    let ty_generics = parsed_generics.ty_generics();
    let turbofish = parsed_generics.turbofish();
    let phantom_type = parsed_generics.phantom_type();

    let mut builder_fields = Vec::new();
    let mut default_inits = Vec::new();
    let mut spread_merges = Vec::new();
    let mut builder_setters = Vec::new();
    let mut callback_traits = Vec::new();
    let mut build_args = Vec::new();
    let mut has_children_param = false;
    let mut has_key_param = false;

    for param in &params {
        if param.name == "children" {
            has_children_param = true;
        }
        if param.name == "key" {
            has_key_param = true;
        }

        default_inits.push(format!("{}: None,", param.name));
        spread_merges.push(format!(
            "if other.{0}.is_some() {{ self.{0} = other.{0}; }}",
            param.name
        ));

        if param.name == "onclick" {
            builder_fields.push("pub onclick: Option<std::rc::Rc<dyn Fn()>>,".to_string());
            builder_setters.push(format!(
                "#[allow(dead_code)]\n    {vis}fn onclick<Args>(mut self, handler: impl strata_dom::EventHandlerMarker<Args> + 'static) -> Self {{\n        self.onclick = Some(std::rc::Rc::new(move || handler.call_handler(strata_dom::Event::default())));\n        self\n    }}\n\n    #[allow(dead_code)]\n    {vis}fn onclick_fn(mut self, handler: impl Fn() + 'static) -> Self {{\n        self.onclick = Some(std::rc::Rc::new(handler));\n        self\n    }}",
                vis = vis,
            ));
            build_args.push(match &param.default {
                ParamDefault::Expr(expr) => format!("self.onclick.or_else(|| ({expr}).into())"),
                ParamDefault::TraitDefault | ParamDefault::None => "self.onclick".to_string(),
            });
        } else if let Some(fn_bound) = extract_rc_dyn_fn_bound(&param.ty, param.is_option) {
            let trait_name = format!("__{}_{}_IntoCallback", fn_name, param.name);
            let trait_decl_generics = parsed_generics.trait_generics_with_marker();
            let args_unit = parsed_generics.trait_args_with_marker("()");
            let args_fn = parsed_generics.trait_args_with_marker("((),)");
            let args_rc_fn = parsed_generics.trait_args_with_marker("(((),),)");
            let args_marker = parsed_generics.trait_args_with_marker("__Marker");
            let impl_f_generics =
                parsed_generics.impl_generics_with_extra_type(&format!("__F: {fn_bound} + 'static"));

            callback_traits.push(format!(
                r#"#[doc(hidden)]
#[allow(non_camel_case_types, dead_code)]
{vis}trait {trait_name}{trait_decl_generics} {where_clause} {{
    fn into_callback(self) -> Option<std::rc::Rc<dyn {fn_bound}>>;
}}

impl{impl_generics} {trait_name}{args_unit} for Option<std::rc::Rc<dyn {fn_bound}>> {where_clause} {{
    #[inline]
    fn into_callback(self) -> Option<std::rc::Rc<dyn {fn_bound}>> {{
        self
    }}
}}

impl{impl_generics} {trait_name}{args_unit} for std::rc::Rc<dyn {fn_bound}> {where_clause} {{
    #[inline]
    fn into_callback(self) -> Option<std::rc::Rc<dyn {fn_bound}>> {{
        Some(self)
    }}
}}

impl{impl_f_generics} {trait_name}{args_fn} for __F {where_clause} {{
    #[inline]
    fn into_callback(self) -> Option<std::rc::Rc<dyn {fn_bound}>> {{
        Some(std::rc::Rc::new(self))
    }}
}}

impl{impl_f_generics} {trait_name}{args_rc_fn} for std::rc::Rc<__F> {where_clause} {{
    #[inline]
    fn into_callback(self) -> Option<std::rc::Rc<dyn {fn_bound}>> {{
        let coerced: std::rc::Rc<dyn {fn_bound}> = self;
        Some(coerced)
    }}
}}"#,
                vis = vis,
                trait_name = trait_name,
                trait_decl_generics = trait_decl_generics,
                where_clause = where_clause,
                impl_generics = impl_generics,
                impl_f_generics = impl_f_generics,
                args_unit = args_unit,
                args_fn = args_fn,
                args_rc_fn = args_rc_fn,
                fn_bound = fn_bound,
            ));

            builder_fields.push(format!(
                "pub {}: Option<std::rc::Rc<dyn {}>>,",
                param.name, fn_bound
            ));
            builder_setters.push(format!(
                "#[allow(dead_code)]\n    {vis}fn {name}<__Marker>(mut self, val: impl {trait_name}{args_marker}) -> Self {{\n        self.{name} = val.into_callback();\n        self\n    }}\n\n    #[allow(dead_code)]\n    {vis}fn {name}_fn(mut self, val: impl {fn_bound} + 'static) -> Self {{\n        self.{name} = Some(std::rc::Rc::new(val));\n        self\n    }}",
                vis = vis,
                name = param.name,
                trait_name = trait_name,
                args_marker = args_marker,
                fn_bound = fn_bound,
            ));
            if param.is_option {
                build_args.push(match &param.default {
                    ParamDefault::Expr(expr) => {
                        format!("self.{}.or_else(|| ({expr}).into())", param.name)
                    }
                    ParamDefault::TraitDefault | ParamDefault::None => {
                        format!("self.{}", param.name)
                    }
                });
            } else {
                build_args.push(match &param.default {
                    ParamDefault::Expr(expr) => {
                        format!("self.{}.unwrap_or_else(|| ({expr}))", param.name)
                    }
                    ParamDefault::TraitDefault | ParamDefault::None => format!(
                        "self.{}.expect(\"Prop `{}` is required for <{}>\")",
                        param.name, param.name, fn_name
                    ),
                });
            }
        } else if param.is_option_string {
            builder_fields.push(format!("pub {}: {},", param.name, param.ty));
            builder_setters.push(format!(
                "#[allow(dead_code)]\n    {vis}fn {name}(mut self, val: impl strata_dom::IntoOptionalString) -> Self {{\n        self.{name} = val.into_optional_string();\n        self\n    }}",
                vis = vis,
                name = param.name,
            ));
            build_args.push(match &param.default {
                ParamDefault::Expr(expr) => format!(
                    "self.{}.or_else(|| strata_dom::IntoOptionalString::into_optional_string({expr}))",
                    param.name
                ),
                ParamDefault::TraitDefault | ParamDefault::None => format!("self.{}", param.name),
            });
        } else if param.is_option {
            builder_fields.push(format!("pub {}: {},", param.name, param.ty));
            builder_setters.push(format!(
                "#[allow(dead_code)]\n    {vis}fn {name}(mut self, val: impl Into<{ty}>) -> Self {{\n        self.{name} = val.into();\n        self\n    }}",
                vis = vis,
                name = param.name,
                ty = param.ty,
            ));
            build_args.push(match &param.default {
                ParamDefault::Expr(expr) => {
                    let trimmed = expr.trim();
                    if trimmed == "None" || trimmed.starts_with("Some(") || trimmed.starts_with("Some (") {
                        format!("self.{}.or_else(|| {expr})", param.name)
                    } else if is_numeric_literal(trimmed) {
                        format!("self.{}.or_else(|| Some({expr}))", param.name)
                    } else {
                        format!("self.{}.or_else(|| ({expr}).into())", param.name)
                    }
                }
                ParamDefault::TraitDefault | ParamDefault::None => format!("self.{}", param.name),
            });
        } else {
            builder_fields.push(format!("pub {}: Option<{}>,", param.name, param.ty));
            if is_numeric_or_bool_primitive(&param.ty) {
                builder_setters.push(format!(
                    "#[allow(dead_code)]\n    {vis}fn {name}(mut self, val: {ty}) -> Self {{\n        self.{name} = Some(val);\n        self\n    }}",
                    vis = vis,
                    name = param.name,
                    ty = param.ty,
                ));
            } else {
                builder_setters.push(format!(
                    "#[allow(dead_code)]\n    {vis}fn {name}(mut self, val: impl Into<{ty}>) -> Self {{\n        self.{name} = Some(val.into());\n        self\n    }}",
                    vis = vis,
                    name = param.name,
                    ty = param.ty,
                ));
            }
            build_args.push(match &param.default {
                ParamDefault::Expr(expr) => {
                    if is_plain_string(&param.ty) || is_class_list(&param.ty) {
                        format!("self.{}.unwrap_or_else(|| ({expr}).into())", param.name)
                    } else {
                        format!("self.{}.unwrap_or_else(|| {expr})", param.name)
                    }
                }
                ParamDefault::TraitDefault => {
                    format!("self.{}.unwrap_or_default()", param.name)
                }
                ParamDefault::None => {
                    if is_implicit_default_type(&param.ty) {
                        format!("self.{}.unwrap_or_default()", param.name)
                    } else {
                        format!(
                            "self.{}.expect(\"Prop `{}` is required for <{}>\")",
                            param.name, param.name, fn_name
                        )
                    }
                }
            });
        }
    }

    // If the function didn't explicitly have a `children` param, add slot to builder
    if !has_children_param {
        builder_fields.push("pub children: Option<Vec<strata_dom::NodeBlueprint>>,".to_string());
        default_inits.push("children: None,".to_string());
        spread_merges.push("if other.children.is_some() { self.children = other.children; }".to_string());
        builder_setters.push(format!(
            "#[allow(dead_code)]\n    {vis}fn children(mut self, children: Vec<strata_dom::NodeBlueprint>) -> Self {{\n        self.children = Some(children);\n        self\n    }}",
            vis = vis,
        ));
    }

    // If the function didn't declare its own `key` prop, provide reconciliation `.key(...)`
    if !has_key_param {
        builder_fields.push("pub __strata_key: Option<String>,".to_string());
        default_inits.push("__strata_key: None,".to_string());
        spread_merges.push(
            "if other.__strata_key.is_some() { self.__strata_key = other.__strata_key; }".to_string(),
        );
        builder_setters.push(format!(
            "#[allow(dead_code)]\n    {vis}fn key(mut self, key: impl std::fmt::Display) -> Self {{\n        self.__strata_key = Some(key.to_string());\n        self\n    }}",
            vis = vis,
        ));
    }

    let (struct_phantom_field, builder_phantom_field, builder_phantom_init) = match phantom_type {
        Some(ref pty) => (
            format!("_marker: {pty},"),
            format!("_marker: {pty},"),
            "_marker: std::marker::PhantomData,".to_string(),
        ),
        None => (String::new(), String::new(), String::new()),
    };

    let build_call = if !has_key_param {
        format!(
            "if let Some(__k) = self.__strata_key {{\n            strata_dom::with_component_key(__k, || {fn_name}{turbofish}(\n                {args}\n            ))\n        }} else {{\n            {fn_name}{turbofish}(\n                {args}\n            )\n        }}",
            fn_name = fn_name,
            turbofish = turbofish,
            args = build_args.join(",\n                "),
        )
    } else {
        format!(
            "{fn_name}{turbofish}(\n            {args}\n        )",
            fn_name = fn_name,
            turbofish = turbofish,
            args = build_args.join(",\n            "),
        )
    };

    let companion_code = format!(
        r#"
{callback_traits}

// Companion Type Namespace Item
#[allow(non_snake_case, dead_code)]
{vis}struct {fn_name}{struct_generics} {where_clause} {{
    {struct_phantom_field}
}}

impl{impl_generics} {fn_name}{ty_generics} {where_clause} {{
    #[allow(dead_code)]
    {vis}fn builder() -> {builder_name}{ty_generics} {{
        {builder_name}::default()
    }}
}}

// Companion Builder Struct
#[allow(non_snake_case, dead_code)]
{vis}struct {builder_name}{struct_generics} {where_clause} {{
    {fields}
    {builder_phantom_field}
}}

impl{impl_generics} Default for {builder_name}{ty_generics} {where_clause} {{
    fn default() -> Self {{
        Self {{
            {default_inits}
            {builder_phantom_init}
        }}
    }}
}}

impl{impl_generics} {builder_name}{ty_generics} {where_clause} {{
    {setters}

    #[doc(hidden)]
    #[allow(dead_code)]
    {vis}fn __spread(mut self, other: Self) -> Self {{
        {spread_merges}
        self
    }}

    #[allow(dead_code)]
    {vis}fn build(self) -> strata_dom::View {{
        {build_call}
    }}
}}
"#,
        callback_traits = callback_traits.join("\n\n"),
        vis = vis,
        fn_name = fn_name,
        struct_generics = struct_generics,
        impl_generics = impl_generics,
        ty_generics = ty_generics,
        where_clause = where_clause,
        struct_phantom_field = struct_phantom_field,
        builder_name = builder_name,
        fields = builder_fields.join("\n    "),
        builder_phantom_field = builder_phantom_field,
        default_inits = default_inits.join("\n            "),
        builder_phantom_init = builder_phantom_init,
        setters = builder_setters.join("\n\n    "),
        spread_merges = spread_merges.join("\n        "),
        build_call = build_call,
    );

    let full_expanded = format!("{}\n{}", fn_rebuilt, companion_code);

    match full_expanded.parse() {
        Ok(stream) => stream,
        Err(_) => item,
    }
}

fn parse_optional_fn_generics(cursor: &mut TokenCursor) -> ParsedGenerics {
    let mut parsed = ParsedGenerics::default();
    if !cursor.consume_punct('<') {
        return parsed;
    }

    let mut depth = 1usize;
    let mut current: Vec<TokenTree> = Vec::new();
    let mut raw_params: Vec<Vec<TokenTree>> = Vec::new();

    while !cursor.is_eof() && depth > 0 {
        if cursor.peek_punct('<') {
            depth += 1;
            if let Some(tok) = cursor.next() {
                current.push(tok);
            }
        } else if cursor.peek_punct('>') {
            depth -= 1;
            if depth == 0 {
                cursor.next();
                if !current.is_empty() {
                    raw_params.push(std::mem::take(&mut current));
                }
            } else if let Some(tok) = cursor.next() {
                current.push(tok);
            }
        } else if cursor.peek_punct(',') && depth == 1 {
            cursor.next();
            if !current.is_empty() {
                raw_params.push(std::mem::take(&mut current));
            }
        } else if let Some(tok) = cursor.next() {
            current.push(tok);
        }
    }

    for param_toks in raw_params {
        if param_toks.is_empty() {
            continue;
        }
        let raw = format_token_trees(&param_toks);
        let impl_decl = strip_generic_default(&param_toks);

        // Lifetime parameter: `'a` or `'a: 'b`
        if matches!(&param_toks[0], TokenTree::Punct(p) if p.as_char() == '\'') {
            let ident = if param_toks.len() >= 2 {
                format!("'{}", param_toks[1])
            } else {
                "'_".to_string()
            };
            parsed.lifetimes.push(GenericParam {
                raw,
                impl_decl,
                ident,
            });
            continue;
        }

        // Const parameter: `const N: usize` or `const N: usize = 4`
        if matches!(&param_toks[0], TokenTree::Ident(id) if id.to_string() == "const") {
            let ident = param_toks
                .get(1)
                .map(|t| t.to_string())
                .unwrap_or_default();
            parsed.consts.push(GenericParam {
                raw,
                impl_decl,
                ident,
            });
            continue;
        }

        // Type parameter: `T`, `T: Clone + 'static`, `T: Clone = String`
        let ident = param_toks[0].to_string();
        parsed.types.push(GenericParam {
            raw,
            impl_decl,
            ident,
        });
    }

    parsed
}

fn strip_generic_default(tokens: &[TokenTree]) -> String {
    let mut angle_depth = 0usize;
    let mut cut_idx = tokens.len();
    for (i, tok) in tokens.iter().enumerate() {
        if let TokenTree::Punct(p) = tok {
            match p.as_char() {
                '<' => angle_depth += 1,
                '>' => angle_depth = angle_depth.saturating_sub(1),
                '=' if angle_depth == 0 => {
                    cut_idx = i;
                    break;
                }
                _ => {}
            }
        }
    }
    format_token_trees(&tokens[..cut_idx])
}

fn format_token_trees(tokens: &[TokenTree]) -> String {
    let strs: Vec<String> = tokens.iter().map(|t| t.to_string()).collect();
    format_type_tokens(&strs)
}

fn split_ret_and_where(tokens: &[TokenTree]) -> (String, String) {
    let mut where_pos = None;
    for (i, tok) in tokens.iter().enumerate() {
        if let TokenTree::Ident(id) = tok {
            if id.to_string() == "where" {
                where_pos = Some(i);
                break;
            }
        }
    }
    match where_pos {
        Some(idx) => {
            let ret: TokenStream = tokens[..idx].iter().cloned().collect();
            let wh: TokenStream = tokens[idx..].iter().cloned().collect();
            (ret.to_string(), wh.to_string())
        }
        None => {
            let ret: TokenStream = tokens.iter().cloned().collect();
            (ret.to_string(), String::new())
        }
    }
}

fn split_leading_inner_attrs(stream: TokenStream) -> (String, String) {
    let mut cursor = TokenCursor::new(stream);
    let mut inner = Vec::new();
    while cursor.peek_punct('#') && cursor.peek_punct_at(1, '!') {
        let hash = cursor.next().unwrap();
        let bang = cursor.next().unwrap();
        inner.push(hash.to_string());
        inner.push(bang.to_string());
        if let Some(g) = cursor.consume_group(Delimiter::Bracket) {
            inner.push(format!("[{}]", g.stream()));
        }
    }
    let mut rest = TokenStream::new();
    while let Some(tok) = cursor.next() {
        rest.extend(std::iter::once(tok));
    }
    (inner.join(""), rest.to_string())
}

fn strip_single_generic_wrapper<'a>(ty: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    let trimmed = ty.trim();
    if !trimmed.ends_with('>') {
        return None;
    }
    for prefix in prefixes {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            let rest = rest.trim_start();
            if let Some(inner) = rest.strip_prefix('<') {
                return Some(inner[..inner.len() - 1].trim());
            }
        }
    }
    None
}

fn is_option_string_type(ty: &str) -> bool {
    let Some(inner) = strip_single_generic_wrapper(
        ty,
        &[
            "Option",
            "std::option::Option",
            "std :: option :: Option",
            "core::option::Option",
            "core :: option :: Option",
        ],
    ) else {
        return false;
    };
    let compact: String = inner.chars().filter(|c| !c.is_whitespace()).collect();
    matches!(
        compact.as_str(),
        "String" | "std::string::String" | "alloc::string::String"
    )
}

fn is_plain_string(ty: &str) -> bool {
    let compact: String = ty.chars().filter(|c| !c.is_whitespace()).collect();
    matches!(
        compact.as_str(),
        "String" | "std::string::String" | "alloc::string::String"
    )
}

fn is_class_list(ty: &str) -> bool {
    let compact: String = ty.chars().filter(|c| !c.is_whitespace()).collect();
    matches!(
        compact.as_str(),
        "ClassList" | "strata_dom::ClassList" | "strata::ClassList" | "strata::prelude::ClassList"
    )
}

fn is_static_str_ref(ty: &str) -> bool {
    let compact: String = ty.chars().filter(|c| !c.is_whitespace()).collect();
    matches!(compact.as_str(), "&'staticstr" | "&str")
}

fn is_vec_type(ty: &str) -> bool {
    let compact: String = ty.chars().filter(|c| !c.is_whitespace()).collect();
    compact.starts_with("Vec<")
        || compact.starts_with("std::vec::Vec<")
        || compact.starts_with("alloc::vec::Vec<")
}

fn is_implicit_default_type(ty: &str) -> bool {
    is_plain_string(ty)
        || is_numeric_or_bool_primitive(ty)
        || is_static_str_ref(ty)
        || is_vec_type(ty)
        || is_class_list(ty)
}

fn is_numeric_or_bool_primitive(ty: &str) -> bool {
    let compact: String = ty.chars().filter(|c| !c.is_whitespace()).collect();
    matches!(
        compact.as_str(),
        "usize"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "f32"
            | "f64"
            | "bool"
            | "char"
    )
}

fn is_numeric_literal(expr: &str) -> bool {
    let trimmed = expr.trim();
    let stripped = trimmed
        .strip_prefix('-')
        .or_else(|| trimmed.strip_prefix('+'))
        .unwrap_or(trimmed)
        .trim_start();
    stripped
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_digit())
}

fn extract_rc_dyn_fn_bound(ty: &str, is_option: bool) -> Option<String> {
    let maybe_rc = if is_option {
        strip_single_generic_wrapper(
            ty,
            &[
                "Option",
                "std::option::Option",
                "std :: option :: Option",
                "core::option::Option",
                "core :: option :: Option",
            ],
        )?
    } else {
        ty.trim()
    };

    let rc_inner = strip_single_generic_wrapper(
        maybe_rc,
        &[
            "Rc",
            "std::rc::Rc",
            "std :: rc :: Rc",
            "alloc::rc::Rc",
            "alloc :: rc :: Rc",
        ],
    )?;

    let fn_bound = rc_inner.strip_prefix("dyn ")?.trim();
    if fn_bound.starts_with("Fn")
        || fn_bound.starts_with("std::ops::Fn")
        || fn_bound.starts_with("core::ops::Fn")
    {
        Some(fn_bound.to_string())
    } else {
        None
    }
}

fn parse_prop_attr_default(group_stream: TokenStream) -> ParamDefault {
    let mut cursor = TokenCursor::new(group_stream);
    while !cursor.is_eof() {
        let Some(key) = cursor.consume_ident() else {
            cursor.next();
            continue;
        };
        if key == "default" {
            if cursor.consume_punct('=') {
                let mut expr_toks = TokenStream::new();
                let mut angle_depth = 0usize;
                while !cursor.is_eof() {
                    if cursor.peek_punct('<') {
                        angle_depth += 1;
                    } else if cursor.peek_punct('>') && angle_depth > 0 {
                        angle_depth -= 1;
                    } else if cursor.peek_punct(',') && angle_depth == 0 {
                        cursor.next();
                        break;
                    }
                    if let Some(tok) = cursor.next() {
                        expr_toks.extend(std::iter::once(tok));
                    }
                }
                let expr_str = expr_toks.to_string().trim().to_string();
                if !expr_str.is_empty() {
                    return ParamDefault::Expr(expr_str);
                }
                return ParamDefault::TraitDefault;
            }
            return ParamDefault::TraitDefault;
        } else if key == "optional" {
            return ParamDefault::TraitDefault;
        } else if key == "into" {
            let _ = cursor.consume_punct(',');
        } else {
            // Skip unknown `#[prop(...)]` item up to next comma
            while !cursor.is_eof() && !cursor.consume_punct(',') {
                cursor.next();
            }
        }
    }
    ParamDefault::None
}

fn parse_component_params(stream: TokenStream) -> Vec<ParamInfo> {
    let mut cursor = TokenCursor::new(stream);
    let mut params = Vec::new();

    while !cursor.is_eof() {
        // Consume leading parameter attributes (`#[prop(...)]` is extracted & stripped;
        // other attributes like `#[allow(...)]` are preserved on the emitted function param).
        let mut kept_attrs = Vec::new();
        let mut param_default = ParamDefault::None;

        while cursor.peek_punct('#') {
            cursor.consume_punct('#');
            if let Some(g) = cursor.consume_group(Delimiter::Bracket) {
                let mut attr_cursor = TokenCursor::new(g.stream());
                if attr_cursor.peek_ident().as_deref() == Some("prop") {
                    attr_cursor.consume_ident();
                    if let Some(inner) = attr_cursor.consume_group(Delimiter::Parenthesis) {
                        let parsed = parse_prop_attr_default(inner.stream());
                        if !matches!(parsed, ParamDefault::None) {
                            param_default = parsed;
                        }
                    }
                } else {
                    kept_attrs.push(format!("#[{}]", g.stream()));
                }
            }
        }

        // Optional leading `mut`
        let has_mut = if cursor.peek_ident().as_deref() == Some("mut") {
            cursor.consume_ident();
            true
        } else {
            false
        };

        let name = match cursor.consume_ident() {
            Some(n) => n,
            None => break,
        };

        if !cursor.consume_punct(':') {
            break;
        }

        // Collect type tokens until comma `,` at top level
        let mut ty_raw_stream = TokenStream::new();
        let mut ty_tokens = Vec::new();
        let mut angle_depth = 0;

        while !cursor.is_eof() {
            if cursor.peek_punct('<') {
                angle_depth += 1;
            } else if cursor.peek_punct('>') && angle_depth > 0 {
                angle_depth -= 1;
            } else if cursor.peek_punct(',') && angle_depth == 0 {
                cursor.consume_punct(',');
                break;
            }

            if let Some(tok) = cursor.next() {
                ty_tokens.push(tok.to_string());
                ty_raw_stream.extend(std::iter::once(tok));
            }
        }

        let ty_str = format_type_tokens(&ty_tokens);
        let ty_trimmed = ty_str.trim();
        let is_option = ty_trimmed.starts_with("Option <")
            || ty_trimmed.starts_with("Option<")
            || ty_trimmed.starts_with("std :: option :: Option")
            || ty_trimmed.starts_with("std::option::Option")
            || ty_trimmed.starts_with("core :: option :: Option")
            || ty_trimmed.starts_with("core::option::Option");
        let is_option_string = is_option && is_option_string_type(ty_trimmed);

        let mut clean = String::new();
        if !kept_attrs.is_empty() {
            clean.push_str(&kept_attrs.join(" "));
            clean.push(' ');
        }
        if has_mut {
            clean.push_str("mut ");
        }
        clean.push_str(&name);
        clean.push_str(": ");
        clean.push_str(&ty_raw_stream.to_string());

        params.push(ParamInfo {
            name,
            ty: ty_str,
            is_option,
            is_option_string,
            default: param_default,
            clean_param_code: clean,
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
