//! Strata Web Reader: Semantic Web Transpiler & Offline Wikipedia Reader Engine.
//!
//! Enables native, ultra-lightweight rendering of HTML5 articles and Wikipedia content
//! without requiring JavaScript execution, Chromium, or web engine overhead.

#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::field_reassign_with_default,
    clippy::collapsible_match
)]

pub mod ast;
pub mod cache;
pub mod csp;
pub mod parser;
pub mod reader_theme;
pub mod tokenizer;
pub mod transpiler;
pub mod wikipedia;

pub use ast::SemanticNode;
pub use cache::ArticleCache;
pub use csp::{extract_meta_csp, ContentSecurityPolicy, CspPolicy, DirectiveKind, SourceExpr};
pub use parser::{
    document_to_semantic_nodes, extract_script_descriptors, extract_scripts,
    semantic_nodes_to_document, HtmlParser, ScriptDescriptor, MAX_TREE_DEPTH,
};
pub use reader_theme::ReaderTheme;
pub use strata_dom::NodeBlueprint;
pub use strata_style::MediaContext;
pub use tokenizer::{decode_html_entities, HtmlToken, HtmlTokenizer, StreamTokenizer};
pub use transpiler::{ArticleMetadata, RenderMode, WebTranspiler};
pub use wikipedia::{
    InfoboxRow, WikipediaArticle, WikipediaEngine, WikipediaInfobox, WikipediaSection,
    WikipediaTocItem,
};

pub mod prelude {
    pub use crate::ast::SemanticNode;
    pub use crate::cache::ArticleCache;
    pub use crate::csp::{
        extract_meta_csp, ContentSecurityPolicy, CspPolicy, DirectiveKind, SourceExpr,
    };
    pub use crate::parser::{
        extract_script_descriptors, extract_scripts, HtmlParser, ScriptDescriptor, MAX_TREE_DEPTH,
    };
    pub use crate::reader_theme::ReaderTheme;
    pub use crate::tokenizer::{decode_html_entities, HtmlToken, HtmlTokenizer, StreamTokenizer};
    pub use crate::transpiler::{ArticleMetadata, RenderMode, WebTranspiler};
    pub use crate::wikipedia::{
        InfoboxRow, WikipediaArticle, WikipediaEngine, WikipediaInfobox, WikipediaSection,
        WikipediaTocItem,
    };
    pub use strata_dom::NodeBlueprint;
    pub use strata_style::MediaContext;
}
