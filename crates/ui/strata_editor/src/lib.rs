//! Strata Editor: Headless Document Model, Rich Text WYSIWYG & Code Editor Engine.
//!
//! Provides a zero-DOM, pure-Rust equivalent to TipTap, Lexical, Slate, and Monaco.
//! Supports hierarchical block nodes, inline marks (bold, italic, code, links), bi-directional
//! GitHub Flavored Markdown serialization, virtualized line buffers, multi-cursor editing,
//! incremental syntax highlighting, bracket matching, code folding, transactional undo/redo history,
//! multi-format clipboard exchange, and CJK IME composition.

pub mod buffer;
pub mod clipboard;
pub mod code_editor;
pub mod cursor;
pub mod editor;
pub mod history;
pub mod ime;
pub mod markdown;
pub mod model;
pub mod syntax;

pub use buffer::LineBuffer;
pub use clipboard::{ClipboardBridge, ClipboardPayload};
pub use code_editor::CodeEditor;
pub use cursor::{MultiCursor, Position, Selection};
pub use editor::{use_editor, EditorState};
pub use history::{EditOperation, EditorTransaction, HistoryStack};
pub use ime::{ImeComposition, ImeState};
pub use markdown::{from_markdown, to_markdown};
pub use model::{BlockKind, BlockNode, Document, InlineMark, InlineSpan};
pub use syntax::{FoldRange, Language, SyntaxHighlighter, SyntaxToken, TokenType};

pub mod prelude {
    pub use crate::buffer::LineBuffer;
    pub use crate::clipboard::{ClipboardBridge, ClipboardPayload};
    pub use crate::code_editor::CodeEditor;
    pub use crate::cursor::{MultiCursor, Position, Selection};
    pub use crate::editor::{use_editor, EditorState};
    pub use crate::history::{EditOperation, EditorTransaction, HistoryStack};
    pub use crate::ime::{ImeComposition, ImeState};
    pub use crate::markdown::{from_markdown, to_markdown};
    pub use crate::model::{BlockKind, BlockNode, Document, InlineMark, InlineSpan};
    pub use crate::syntax::{FoldRange, Language, SyntaxHighlighter, SyntaxToken, TokenType};
}
