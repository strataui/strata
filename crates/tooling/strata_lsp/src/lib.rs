//! Strata Language Server Protocol (LSP 3.17) implementation in pure Rust.

pub mod types;
pub mod jsonrpc;
pub mod parser;
pub mod diagnostics;
pub mod completion;
pub mod hover;
pub mod color;
pub mod server;

pub use types::*;
pub use jsonrpc::{encode_framed_message, parse_content_length, JsonRpcMessage};
pub use parser::{ParseError, RsxAttribute, RsxDocument, RsxNode, SourceMap, Span};
pub use diagnostics::validate_document;
pub use completion::compute_completions;
pub use hover::resolve_hover;
pub use color::{find_document_colors, format_color_presentation};
pub use server::LspServer;
