//! Strata Editor: Headless Document Model, Rich Text WYSIWYG & Monaco-Class Code Editor Engine.
//!
//! Provides a zero-DOM, pure-Rust equivalent to TipTap, Lexical, Slate, and Monaco.
//! Supports hierarchical block nodes, inline marks (bold, italic, code, links), bi-directional
//! GitHub Flavored Markdown serialization, B-Tree rope virtualized line buffers, multi-cursor
//! editing, true incremental `tree.edit(&InputEdit)` AST parsing, 10-language incremental syntax
//! highlighting, bracket matching, fold-aware virtualization, Find & Replace, Canvas2D minimap,
//! LSP-compatible diagnostics, CSS-variable theming (`--strata-editor-*`), transactional
//! undo/redo history, multi-format clipboard exchange, and CJK IME composition.

pub mod buffer;
pub mod clipboard;
pub mod code_editor;
pub mod cursor;
pub mod diagnostics;
pub mod editor;
pub mod history;
pub mod ime;
pub mod markdown;
pub mod minimap;
pub mod model;
pub mod search;
pub mod soft_wrap;
pub mod syntax;
pub mod theme;
pub mod widget;

pub use buffer::{LineBuffer, TextRope};
pub use clipboard::{ClipboardBridge, ClipboardPayload};
pub use code_editor::CodeEditor;
pub use cursor::{MultiCursor, Position, Selection};
pub use diagnostics::{DiagnosticCollection, DiagnosticCounts, DiagnosticSeverity, EditorDiagnostic};
pub use editor::{use_editor, EditorState};
pub use history::{EditOperation, EditorTransaction, HistoryStack};
pub use ime::{ImeComposition, ImeState};
pub use markdown::{from_markdown, to_markdown};
pub use minimap::{
    compute_minimap_metrics, minimap_y_to_scroll_top, render_minimap_canvas_commands,
    MinimapConfig, MinimapMetrics,
};
pub use model::{BlockKind, BlockNode, Document, InlineMark, InlineSpan};
pub use search::{FindReplaceState, SearchMatch};
pub use soft_wrap::{logical_to_visual, visual_to_logical, wrap_logical_line, VisualLine};
pub use syntax::{
    FoldRange, Language, LineLexState, SyntaxDiagnostic, SyntaxHighlighter, SyntaxToken, TokenType,
};
pub use theme::EditorTheme;
pub use widget::{
    render_interactive_editor, use_code_editor, CodeEditorHandle, CodeEditorOptions,
    CodeEditorView, CodeEditorWidget,
};

pub mod prelude {
    pub use crate::buffer::{LineBuffer, TextRope};
    pub use crate::clipboard::{ClipboardBridge, ClipboardPayload};
    pub use crate::code_editor::CodeEditor;
    pub use crate::cursor::{MultiCursor, Position, Selection};
    pub use crate::diagnostics::{
        DiagnosticCollection, DiagnosticCounts, DiagnosticSeverity, EditorDiagnostic,
    };
    pub use crate::editor::{use_editor, EditorState};
    pub use crate::history::{EditOperation, EditorTransaction, HistoryStack};
    pub use crate::ime::{ImeComposition, ImeState};
    pub use crate::markdown::{from_markdown, to_markdown};
    pub use crate::minimap::{
        compute_minimap_metrics, minimap_y_to_scroll_top, render_minimap_canvas_commands,
        MinimapConfig, MinimapMetrics,
    };
    pub use crate::model::{BlockKind, BlockNode, Document, InlineMark, InlineSpan};
    pub use crate::search::{FindReplaceState, SearchMatch};
    pub use crate::soft_wrap::{
        logical_to_visual, visual_to_logical, wrap_logical_line, VisualLine,
    };
    pub use crate::syntax::{
        FoldRange, Language, LineLexState, SyntaxDiagnostic, SyntaxHighlighter, SyntaxToken,
        TokenType,
    };
    pub use crate::theme::EditorTheme;
    pub use crate::widget::{
        render_interactive_editor, use_code_editor, CodeEditorHandle, CodeEditorOptions,
        CodeEditorView, CodeEditorWidget,
    };
}
