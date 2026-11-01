//! Language Server Protocol (LSP 3.17) Data Types in Pure Rust.

/// Position in a text document expressed as zero-based line and character offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

impl Position {
    pub const fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

/// A range in a text document expressed as (start, end) positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

impl Range {
    pub const fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    pub const fn from_coords(start_line: u32, start_char: u32, end_line: u32, end_char: u32) -> Self {
        Self {
            start: Position::new(start_line, start_char),
            end: Position::new(end_line, end_char),
        }
    }
}

/// Represents a location inside a resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub uri: String,
    pub range: Range,
}

/// The severity level of a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DiagnosticSeverity {
    Error = 1,
    Warning = 2,
    Information = 3,
    Hint = 4,
}

/// A diagnostic message representing an error, warning, or hint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub range: Range,
    pub severity: DiagnosticSeverity,
    pub code: Option<String>,
    pub source: Option<String>,
    pub message: String,
}

impl Diagnostic {
    pub fn error(range: Range, message: impl Into<String>) -> Self {
        Self {
            range,
            severity: DiagnosticSeverity::Error,
            code: None,
            source: Some("strata-lsp".to_string()),
            message: message.into(),
        }
    }

    pub fn warning(range: Range, message: impl Into<String>) -> Self {
        Self {
            range,
            severity: DiagnosticSeverity::Warning,
            code: None,
            source: Some("strata-lsp".to_string()),
            message: message.into(),
        }
    }
}

/// Classification of a completion item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CompletionItemKind {
    Text = 1,
    Method = 2,
    Function = 3,
    Constructor = 4,
    Field = 5,
    Variable = 6,
    Class = 7,
    Interface = 8,
    Module = 9,
    Property = 10,
    Unit = 11,
    Value = 12,
    Enum = 13,
    Keyword = 14,
    Snippet = 15,
    Color = 16,
    File = 17,
    Reference = 18,
    Folder = 19,
    EnumMember = 20,
    Constant = 21,
    Struct = 22,
    Event = 23,
    Operator = 24,
    TypeParameter = 25,
}

/// An autocomplete item suggested to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionItemKind,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    pub insert_text: Option<String>,
}

impl CompletionItem {
    pub fn new(label: impl Into<String>, kind: CompletionItemKind) -> Self {
        Self {
            label: label.into(),
            kind,
            detail: None,
            documentation: None,
            insert_text: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn with_doc(mut self, doc: impl Into<String>) -> Self {
        self.documentation = Some(doc.into());
        self
    }

    pub fn with_insert(mut self, insert: impl Into<String>) -> Self {
        self.insert_text = Some(insert.into());
        self
    }
}

/// Content returned when hovering over a symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hover {
    pub contents: String,
    pub range: Option<Range>,
}

/// RGBA floating-point color representation (0.0 to 1.0).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorRgba {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub alpha: f32,
}

/// Gutter color representation returned by `textDocument/documentColor`.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorInformation {
    pub range: Range,
    pub color: ColorRgba,
}

/// Text edit to update a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub range: Range,
    pub new_text: String,
}

/// Formatted color replacement string returned by `textDocument/colorPresentation`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorPresentation {
    pub label: String,
    pub text_edit: Option<TextEdit>,
}

/// Server capabilities declared during LSP initialization handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerCapabilities {
    pub text_document_sync: u8, // 1 = Full, 2 = Incremental
    pub completion_provider: bool,
    pub hover_provider: bool,
    pub document_color_provider: bool,
}

impl Default for ServerCapabilities {
    fn default() -> Self {
        Self {
            text_document_sync: 1, // Full
            completion_provider: true,
            hover_provider: true,
            document_color_provider: true,
        }
    }
}
