//! Strata Codegen JS: Pure JavaScript ESM code generator, AST transpiler, and Web-WASM harness.
//!
//! Provides dual-mode web compilation:
//! - **Mode A (Web-WASM):** HTML5 canvas surface harness with HiDPI scaling and event pumps.
//! - **Mode B (Web-JS):** Lightweight vanilla ESM JS transpilation with micro reactive runtime and CSS extraction.

pub mod css_extractor;
pub mod runtime;
pub mod target;
pub mod transpiler;
pub mod wasm_harness;

pub use css_extractor::CssExtractor;
pub use runtime::JsRuntimeEmitter;
pub use target::{WebBuildArtifacts, WebBuildConfig, WebTargetBuilder, WebTargetMode};
pub use transpiler::JsTranspiler;
pub use wasm_harness::WasmHarnessGenerator;

/// Web codegen prelude module.
pub mod prelude {
    pub use super::css_extractor::CssExtractor;
    pub use super::runtime::JsRuntimeEmitter;
    pub use super::target::{WebBuildArtifacts, WebBuildConfig, WebTargetBuilder, WebTargetMode};
    pub use super::transpiler::JsTranspiler;
    pub use super::wasm_harness::WasmHarnessGenerator;
}
