//! Strata Web Reader: Semantic Web Transpiler & Offline Wikipedia Reader Engine.
//!
//! Enables native, ultra-lightweight rendering of HTML5 articles and Wikipedia content
//! without requiring JavaScript execution, Chromium, or web engine overhead.

pub mod ast;
pub mod tokenizer;
pub mod parser;
pub mod transpiler;
pub mod reader_theme;
pub mod wikipedia;
pub mod cache;

pub use ast::SemanticNode;
pub use tokenizer::{decode_html_entities, HtmlToken, HtmlTokenizer};
pub use parser::HtmlParser;
pub use transpiler::WebTranspiler;
pub use reader_theme::ReaderTheme;
pub use wikipedia::{
    InfoboxRow, WikipediaArticle, WikipediaEngine, WikipediaInfobox, WikipediaSection,
    WikipediaTocItem,
};
pub use cache::ArticleCache;

pub mod prelude {
    pub use crate::ast::SemanticNode;
    pub use crate::tokenizer::{decode_html_entities, HtmlToken, HtmlTokenizer};
    pub use crate::parser::HtmlParser;
    pub use crate::transpiler::WebTranspiler;
    pub use crate::reader_theme::ReaderTheme;
    pub use crate::wikipedia::{
        InfoboxRow, WikipediaArticle, WikipediaEngine, WikipediaInfobox, WikipediaSection,
        WikipediaTocItem,
    };
    pub use crate::cache::ArticleCache;
}
