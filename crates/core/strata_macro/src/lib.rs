//! Strata Macro: First-principles JSX/HTML compiler, #[component], and dynamic! macro.

extern crate proc_macro;
use proc_macro::TokenStream;

mod ast;
mod classes;
mod codegen;
mod component;
mod cursor;
mod dynamic;
mod parser;
mod store;

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
        Ok(nodes) => match codegen::generate_rsx(&nodes) {
            Ok(code) => code.parse().unwrap_or_else(|err| {
                panic!(
                    "Strata RSX codegen compilation error: {:?}\nGenerated code was:\n{}",
                    err, code
                );
            }),
            Err(errors) => {
                let message = errors.join("\n");
                let diagnostic = format!("compile_error!({:?})", message);
                diagnostic.parse().unwrap_or_else(|err| {
                    panic!(
                        "Strata RSX diagnostic emission failed: {:?}\nDiagnostic was:\n{}",
                        err, diagnostic
                    );
                })
            }
        },
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

/// Derives a companion `<StructName>Store` where each field is an independent atomic `Signal<T>`.
#[proc_macro_derive(Store)]
pub fn derive_store(input: TokenStream) -> TokenStream {
    store::expand_derive_store(input)
}

/// Alias for #[derive(Store)].
#[proc_macro_derive(ReactiveStore)]
pub fn derive_reactive_store(input: TokenStream) -> TokenStream {
    store::expand_derive_store(input)
}
