//! Strata Macro: First-principles JSX/HTML compiler, #[component], and dynamic! macro.

extern crate proc_macro;
use proc_macro::TokenStream;

mod ast;
mod codegen;
mod component;
mod cursor;
mod dynamic;
mod parser;

/// Declares a Strata functional component with fine-grained reactivity and JSX prop builder.
#[proc_macro_attribute]
pub fn component(attr: TokenStream, item: TokenStream) -> TokenStream {
    component::expand_component(attr, item)
}

/// Declarative JSX/HTML-in-Rust templating macro.
#[proc_macro]
pub fn rsx(input: TokenStream) -> TokenStream {
    let mut cursor = cursor::TokenCursor::new(input);
    match parser::parse_rsx(&mut cursor) {
        Ok(nodes) => {
            let code = codegen::generate_rsx(&nodes);
            code.parse().unwrap_or_else(|err| {
                panic!(
                    "Strata RSX codegen compilation error: {:?}\nGenerated code was:\n{}",
                    err, code
                );
            })
        }
        Err(err) => {
            panic!("Strata RSX syntax error: {}", err);
        }
    }
}

/// Dynamic JavaScript object/array constructor macro.
#[proc_macro]
pub fn dynamic(input: TokenStream) -> TokenStream {
    dynamic::expand_dynamic(input)
}
