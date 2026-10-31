//! Strata Editor: Headless Document Model & Markdown Engine.
//!
//! Provides a zero-DOM, pure-Rust equivalent to TipTap, Lexical, and Slate.
//! Supports hierarchical block nodes, inline marks (bold, italic, code, links), bi-directional
//! GitHub Flavored Markdown serialization, and cursor text manipulation.

pub mod editor;
pub mod markdown;
pub mod model;

pub use editor::{use_editor, EditorState};
pub use markdown::{from_markdown, to_markdown};
pub use model::{BlockKind, BlockNode, Document, InlineMark, InlineSpan};

pub mod prelude {
    pub use crate::editor::{use_editor, EditorState};
    pub use crate::markdown::{from_markdown, to_markdown};
    pub use crate::model::{BlockKind, BlockNode, Document, InlineMark, InlineSpan};
}
