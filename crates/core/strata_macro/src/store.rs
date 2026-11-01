//! Hand-written zero-dependency implementation of the #[derive(Store)] macro.
//!
//! Generates a companion `<StructName>Store` struct where each field is an independent
//! atomic `Signal<FieldType>`, with batch transaction and rollback support.

use proc_macro::{Delimiter, TokenStream};
use crate::cursor::TokenCursor;

struct FieldInfo {
    vis: String,
    name: String,
    ty: String,
}

pub fn expand_derive_store(input: TokenStream) -> TokenStream {
    let mut cursor = TokenCursor::new(input.clone());

    // 1. Skip outer attributes (e.g. `#[derive(...)]`, `#[doc = "..."]`)
    while cursor.peek_punct('#') {
        cursor.next(); // consume '#'
        if cursor.peek_group(Delimiter::Bracket).is_some() {
            cursor.consume_group(Delimiter::Bracket);
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

    // 3. Must be `struct`
    if cursor.peek_ident().as_deref() != Some("struct") {
        panic!("#[derive(Store)] can only be applied to named structs");
    }
    cursor.consume_ident(); // consume `struct`

    // 4. Struct Name
    let struct_name = match cursor.consume_ident() {
        Some(name) => name,
        None => panic!("Expected struct name in #[derive(Store)]"),
    };

    // 5. Consume optional generics
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
            if let Some(t) = cursor.next() {
                generics.push_str(&t.to_string());
            }
        }
    }

    // 6. Look for fields enclosed in `{ ... }`
    let body_group = match cursor.consume_group(Delimiter::Brace) {
        Some(g) => g,
        None => panic!("#[derive(Store)] only supports structs with named fields {{ ... }}"),
    };

    // 7. Parse fields from body
    let mut field_cursor = TokenCursor::new(body_group.stream());
    let mut fields = Vec::new();

    while !field_cursor.is_eof() {
        // Skip field attributes
        while field_cursor.peek_punct('#') {
            field_cursor.next();
            if field_cursor.peek_group(Delimiter::Bracket).is_some() {
                field_cursor.consume_group(Delimiter::Bracket);
            }
        }

        if field_cursor.is_eof() {
            break;
        }

        // Field visibility
        let mut f_vis = String::new();
        if field_cursor.peek_ident().as_deref() == Some("pub") {
            f_vis.push_str("pub");
            field_cursor.consume_ident();
            if field_cursor.peek_group(Delimiter::Parenthesis).is_some() {
                let g = field_cursor.consume_group(Delimiter::Parenthesis).unwrap();
                f_vis.push_str(&format!("({})", g.stream()));
            }
            f_vis.push(' ');
        }

        // Field name
        let f_name = match field_cursor.consume_ident() {
            Some(name) => name,
            None => break,
        };

        // Expect `:`
        if !field_cursor.consume_punct(':') {
            panic!("Expected ':' after field name '{}' in struct '{}'", f_name, struct_name);
        }

        // Field type tokens until `,` or EOF
        let mut ty_tokens = String::new();
        let mut depth = 0;
        while !field_cursor.is_eof() {
            if depth == 0 && field_cursor.peek_punct(',') {
                field_cursor.consume_punct(',');
                break;
            }

            if field_cursor.peek_punct('<') {
                depth += 1;
            } else if field_cursor.peek_punct('>') && depth > 0 {
                depth -= 1;
            }

            if let Some(tt) = field_cursor.next() {
                ty_tokens.push_str(&tt.to_string());
                ty_tokens.push(' ');
            }
        }

        let f_ty = ty_tokens.trim().to_string();
        if !f_name.is_empty() && !f_ty.is_empty() {
            fields.push(FieldInfo {
                vis: f_vis,
                name: f_name,
                ty: f_ty,
            });
        }
    }

    // 8. Generate companion `<StructName>Store`
    let store_name = format!("{}Store", struct_name);

    let mut store_fields = String::new();
    let mut clone_fields = String::new();
    let mut new_fields = String::new();
    let mut get_fields = String::new();
    let mut set_fields = String::new();
    let mut snapshot_fields = String::new();
    let mut restore_fields = String::new();

    for f in &fields {
        let field_vis = if f.vis.is_empty() { "pub ".to_string() } else { f.vis.clone() };
        store_fields.push_str(&format!("    {}{}: strata_dom::Signal<{}>,\n", field_vis, f.name, f.ty));
        clone_fields.push_str(&format!("            {}: self.{}.clone(),\n", f.name, f.name));
        new_fields.push_str(&format!("            {}: strata_dom::Signal::new(initial.{}),\n", f.name, f.name));
        get_fields.push_str(&format!("            {}: self.{}.get(),\n", f.name, f.name));
        set_fields.push_str(&format!("            self.{}.set(next.{});\n", f.name, f.name));
        snapshot_fields.push_str(&format!("            {}: self.{}.untracked_get(),\n", f.name, f.name));
        restore_fields.push_str(&format!("            self.{}.set(snapshot.{}.clone());\n", f.name, f.name));
    }

    let output = format!(
        r#"
{vis}struct {store_name}{generics} {{
{store_fields}
}}

impl{generics} Clone for {store_name}{generics} {{
    fn clone(&self) -> Self {{
        Self {{
{clone_fields}
        }}
    }}
}}

impl{generics} {store_name}{generics} {{
    pub fn new(initial: {struct_name}{generics}) -> Self {{
        Self {{
{new_fields}
        }}
    }}

    pub fn get(&self) -> {struct_name}{generics} {{
        {struct_name} {{
{get_fields}
        }}
    }}

    pub fn set(&self, next: {struct_name}{generics}) {{
        strata_dom::batch(|| {{
{set_fields}
        }});
    }}

    pub fn batch<R>(&self, f: impl FnOnce(&Self) -> R) -> R {{
        strata_dom::batch(|| f(self))
    }}

    pub fn snapshot(&self) -> {struct_name}{generics} {{
        {struct_name} {{
{snapshot_fields}
        }}
    }}

    pub fn restore(&self, snapshot: &{struct_name}{generics}) {{
        strata_dom::batch(|| {{
{restore_fields}
        }});
    }}
}}

impl{generics} Default for {store_name}{generics}
where
    {struct_name}{generics}: Default,
{{
    fn default() -> Self {{
        Self::new({struct_name}::default())
    }}
}}

impl{generics} {struct_name}{generics} {{
    pub fn into_store(self) -> {store_name}{generics} {{
        {store_name}::new(self)
    }}
}}
"#
    );

    output.parse().unwrap_or_else(|err| {
        panic!("Failed to parse generated Store tokens: {:?}\nGenerated:\n{}", err, output)
    })
}
