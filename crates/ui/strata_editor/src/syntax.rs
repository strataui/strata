//! Real-time incremental syntax tokenization, Tree-Sitter `InputEdit` AST updates,
//! multi-line incremental grammar state machines for 10 languages, bracket pair matching,
//! structural code folding, and syntax diagnostic extraction.

use std::ops::Range;

use serde::{Deserialize, Serialize};
use tree_sitter::{InputEdit, Node, Parser, Point, Tree};

use crate::buffer::LineBuffer;
use crate::cursor::Position;

/// Supported language syntaxes for the Monaco-class code editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Language {
    #[default]
    Rust,
    Python,
    Json,
    Yaml,
    Toml,
    XmlSdf,
    Cpp,
    JavaScript,
    TypeScript,
    Markdown,
    Shell,
    Html,
    PlainText,
}

impl Language {
    /// Returns all 10 primary code/config languages supported by `strata-editor`.
    pub const ALL_LANGUAGES: &'static [Language] = &[
        Language::Rust,
        Language::Python,
        Language::Json,
        Language::Yaml,
        Language::Toml,
        Language::XmlSdf,
        Language::Cpp,
        Language::JavaScript,
        Language::TypeScript,
        Language::Markdown,
        Language::Shell,
    ];

    /// Infers the [`Language`] from a file extension or Markdown fence info string
    /// (case-insensitive, with or without leading dot).
    pub fn from_extension(ext: &str) -> Self {
        let cleaned = ext.trim().trim_start_matches('.').to_ascii_lowercase();
        match cleaned.as_str() {
            "rs" | "rust" => Self::Rust,
            "py" | "pyi" | "pyw" | "python" | "python3" => Self::Python,
            "json" | "jsonc" | "geojson" => Self::Json,
            "yml" | "yaml" => Self::Yaml,
            "toml" => Self::Toml,
            "xml" | "sdf" | "urdf" | "xacro" | "world" | "launch" | "svg" => Self::XmlSdf,
            "c" | "h" | "cc" | "cpp" | "c++" | "cxx" | "hh" | "hpp" | "hxx" | "ino" => Self::Cpp,
            "js" | "mjs" | "cjs" | "jsx" | "javascript" => Self::JavaScript,
            "ts" | "mts" | "cts" | "tsx" | "typescript" => Self::TypeScript,
            "md" | "markdown" | "mdx" => Self::Markdown,
            "sh" | "bash" | "zsh" | "ksh" | "env" | "shell" | "console" => Self::Shell,
            "html" | "htm" => Self::Html,
            _ => Self::PlainText,
        }
    }

    /// Infers the [`Language`] from a filename or path.
    pub fn from_filename(path: &str) -> Self {
        let base = path.rsplit(['/', '\\']).next().unwrap_or(path);
        match base {
            "Cargo.lock" | "Cargo.toml" => return Self::Toml,
            "Makefile" | "Dockerfile" | ".bashrc" | ".zshrc" | ".profile" => return Self::Shell,
            _ => {}
        }
        if let Some((_, ext)) = base.rsplit_once('.') {
            Self::from_extension(ext)
        } else {
            Self::PlainText
        }
    }

    /// Human-readable display label for status bars and language pickers.
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Python => "Python",
            Self::Json => "JSON",
            Self::Yaml => "YAML",
            Self::Toml => "TOML",
            Self::XmlSdf => "XML / SDF",
            Self::Cpp => "C / C++",
            Self::JavaScript => "JavaScript",
            Self::TypeScript => "TypeScript",
            Self::Markdown => "Markdown",
            Self::Shell => "Shell",
            Self::Html => "HTML",
            Self::PlainText => "Plain Text",
        }
    }

    /// Default single-line comment prefix for `Ctrl+/` toggle-comment actions.
    pub const fn line_comment_prefix(self) -> Option<&'static str> {
        match self {
            Self::Rust | Self::Cpp | Self::JavaScript | Self::TypeScript => Some("//"),
            Self::Python | Self::Yaml | Self::Toml | Self::Shell => Some("#"),
            Self::XmlSdf | Self::Html | Self::Markdown | Self::Json | Self::PlainText => None,
        }
    }

    /// Whether this language uses a native Tree-Sitter grammar by default.
    pub const fn has_builtin_tree_sitter(self) -> bool {
        matches!(self, Self::Rust | Self::JavaScript)
    }
}

/// Token classification for semantic syntax highlighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TokenType {
    Keyword,
    ControlFlow,
    Type,
    Function,
    Macro,
    Identifier,
    Parameter,
    Property,
    String,
    StringEscape,
    Number,
    Boolean,
    Comment,
    DocComment,
    Operator,
    Punctuation,
    Tag,
    Attribute,
    Namespace,
    Heading,
    Link,
    Variable,
}

/// A highlighted syntax token with UTF-8 byte column span offsets within its line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxToken {
    pub token_type: TokenType,
    pub start_col: usize,
    pub end_col: usize,
    pub text: String,
}

/// A collapsible block of code in the editor gutter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoldRange {
    pub start_line: usize,
    pub end_line: usize,
    pub collapsed: bool,
}

/// Lightweight structural syntax diagnostic emitted directly by the incremental syntax engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxDiagnostic {
    pub start: Position,
    pub end: Position,
    pub message: String,
    pub is_warning: bool,
}

/// Persistent cross-line lexer state stored per line for O(delta) incremental re-tokenization.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LineLexState {
    #[default]
    Normal,
    /// Inside `/* ... */` block comment (Rust, C/C++, JS/TS) with nesting depth for Rust.
    BlockComment { depth: u16, doc: bool },
    /// Inside `<!-- ... -->` XML/SDF/HTML/Markdown comment.
    XmlComment,
    /// Inside `<![CDATA[ ... ]]>` XML/SDF section.
    XmlCdata,
    /// Inside Python/TOML triple-quoted string (`"""` or `'''`).
    TripleString { quote: u8 },
    /// Inside Rust raw string `r###"..."###`.
    RustRawString { hashes: u8 },
    /// Inside Markdown fenced code block with optional embedded language.
    MarkdownFence {
        fence_char: u8,
        fence_len: u8,
        embedded: Language,
    },
    /// Inside Shell heredoc `<<EOF ... EOF`.
    ShellHeredoc { delimiter: String },
    /// Inside YAML block scalar (`|` or `>`) indented deeper than `base_indent`.
    YamlBlockScalar { base_indent: usize },
}

fn create_parser_for_lang(lang: Language) -> Option<Parser> {
    let mut parser = Parser::new();
    match lang {
        Language::Rust => {
            parser
                .set_language(&tree_sitter_rust::LANGUAGE.into())
                .ok()?;
            Some(parser)
        }
        Language::JavaScript => {
            parser
                .set_language(&tree_sitter_javascript::LANGUAGE.into())
                .ok()?;
            Some(parser)
        }
        _ => None,
    }
}

fn classify_tree_sitter_kind(kind: &str, parent_kind: &str, text: &str) -> TokenType {
    match kind {
        "line_comment" | "block_comment" | "comment" => {
            if text.starts_with("///") || text.starts_with("//!") || text.starts_with("/**") {
                TokenType::DocComment
            } else {
                TokenType::Comment
            }
        }

        "string_literal"
        | "raw_string_literal"
        | "char_literal"
        | "string"
        | "template_string"
        | "string_content"
        | "string_fragment" => TokenType::String,

        "escape_sequence" => TokenType::StringEscape,

        "integer_literal" | "float_literal" | "number" => TokenType::Number,

        "true" | "false" | "boolean_literal" => TokenType::Boolean,

        "primitive_type" | "type_identifier" | "predefined_type" => TokenType::Type,

        "lifetime" | "label" => TokenType::Namespace,

        "attribute_item" | "inner_attribute_item" | "meta_item" => TokenType::Attribute,

        "identifier"
            if parent_kind == "macro_invocation" || parent_kind == "macro_definition" =>
        {
            TokenType::Macro
        }

        "identifier"
            if parent_kind == "call_expression"
                || parent_kind == "function_item"
                || parent_kind == "function_declaration"
                || parent_kind == "function_declarator"
                || parent_kind == "method_definition" =>
        {
            TokenType::Function
        }

        "identifier" if parent_kind == "parameter" || parent_kind == "formal_parameters" => {
            TokenType::Parameter
        }

        "if" | "else" | "match" | "return" | "for" | "in" | "while" | "loop" | "break"
        | "continue" | "try" | "catch" | "finally" | "switch" | "case" | "throw" | "yield"
        | "await" => TokenType::ControlFlow,

        "fn" | "let" | "mut" | "pub" | "struct" | "enum" | "impl" | "trait" | "async" | "use"
        | "mod" | "crate" | "const" | "static" | "type" | "where" | "as" | "ref" | "move"
        | "unsafe" | "dyn" | "super" | "self" | "Self" | "function" | "var" | "import"
        | "export" | "default" | "from" | "class" | "interface" | "extends" | "new" | "this"
        | "typeof" | "instanceof" | "debugger" | "null" | "undefined" => TokenType::Keyword,

        "+" | "-" | "*" | "/" | "%" | "=" | "==" | "!=" | "<" | ">" | "<=" | ">=" | "&&" | "||"
        | "!" | "&" | "|" | "^" | "<<" | ">>" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|="
        | "^=" | "=>" | "->" | "?" | ".." | "..=" => TokenType::Operator,

        "(" | ")" | "[" | "]" | "{" | "}" | ";" | ":" | "::" | "," | "." => TokenType::Punctuation,

        "tag_name" | "jsx_opening_element" | "jsx_closing_element" => TokenType::Tag,
        "property_identifier" | "field_identifier" | "shorthand_property_identifier" => {
            TokenType::Property
        }

        _ => TokenType::Identifier,
    }
}

fn snap_char_boundary(s: &str, mut byte_idx: usize) -> usize {
    byte_idx = byte_idx.min(s.len());
    while byte_idx > 0 && !s.is_char_boundary(byte_idx) {
        byte_idx -= 1;
    }
    byte_idx
}

fn collect_leaf_tokens(node: Node, line_idx: usize, out: &mut Vec<SyntaxToken>, line_text: &str) {
    if node.start_position().row > line_idx || node.end_position().row < line_idx {
        return;
    }

    // Keep string / comment literals cohesive rather than splitting quotes from content if no escapes.
    let kind = node.kind();
    let is_atomic = matches!(
        kind,
        "line_comment" | "block_comment" | "comment" | "raw_string_literal" | "char_literal"
    );

    if node.child_count() == 0 || is_atomic {
        let start_pt = node.start_position();
        let end_pt = node.end_position();

        let raw_start = if start_pt.row == line_idx {
            start_pt.column
        } else {
            0
        };
        let raw_end = if end_pt.row == line_idx {
            end_pt.column
        } else {
            line_text.len()
        };

        let start_col = snap_char_boundary(line_text, raw_start);
        let end_col = snap_char_boundary(line_text, raw_end);

        if start_col < end_col && start_col < line_text.len() {
            let text = &line_text[start_col..end_col];
            if !text.trim().is_empty() {
                let parent_kind = node.parent().map(|p| p.kind()).unwrap_or("");
                let token_type = classify_tree_sitter_kind(kind, parent_kind, text);
                out.push(SyntaxToken {
                    token_type,
                    start_col,
                    end_col,
                    text: text.to_string(),
                });
            }
        }
        return;
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_leaf_tokens(child, line_idx, out, line_text);
        }
    }
}

fn collect_ast_fold_ranges(node: Node, out: &mut Vec<FoldRange>) {
    let start_row = node.start_position().row;
    let end_row = node.end_position().row;

    if end_row > start_row {
        let is_foldable = matches!(
            node.kind(),
            "block"
                | "declaration_list"
                | "match_block"
                | "statement_block"
                | "field_declaration_list"
                | "enum_variant_list"
                | "object"
                | "array"
                | "function_item"
                | "impl_item"
                | "struct_item"
                | "enum_item"
                | "trait_item"
                | "mod_item"
                | "function_declaration"
                | "class_declaration"
                | "class_body"
                | "switch_body"
                | "block_comment"
        );

        if is_foldable {
            out.push(FoldRange {
                start_line: start_row,
                end_line: end_row,
                collapsed: false,
            });
        }
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_ast_fold_ranges(child, out);
        }
    }
}

fn collect_tree_sitter_errors(node: Node, out: &mut Vec<SyntaxDiagnostic>) {
    if node.is_error() || node.is_missing() {
        let s = node.start_position();
        let e = node.end_position();
        let msg = if node.is_missing() {
            format!("Missing expected syntax token `{}`", node.kind())
        } else {
            "Syntax error".to_string()
        };
        out.push(SyntaxDiagnostic {
            start: Position::new(s.row, s.column),
            end: Position::new(e.row, e.column.max(s.column + 1)),
            message: msg,
            is_warning: false,
        });
        return;
    }
    if !node.has_error() {
        return;
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_tree_sitter_errors(child, out);
        }
    }
}

/// Syntax analysis engine providing real-time Tree-Sitter `InputEdit` incremental parsing,
/// 10-language incremental line-state tokenization, structural code folding, and diagnostics.
pub struct SyntaxHighlighter {
    parser: Option<Parser>,
    custom_ts_language: Option<tree_sitter::Language>,
    tree: Option<Tree>,
    language: Language,
    /// Multi-line lexer state entering line `i` (length == `line_count`).
    line_entry_states: Vec<LineLexState>,
    /// Cached syntax tokens per line for O(1) 120 FPS viewport rendering.
    line_token_cache: Vec<Option<Vec<SyntaxToken>>>,
    /// Count of incremental `tree.edit(&InputEdit)` + reparse cycles executed.
    incremental_edit_count: u64,
    /// Last changed line ranges reported by Tree-Sitter or incremental state propagation.
    last_changed_lines: Vec<Range<usize>>,
}

impl std::fmt::Debug for SyntaxHighlighter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyntaxHighlighter")
            .field("has_parser", &self.parser.is_some())
            .field("has_tree", &self.tree.is_some())
            .field("language", &self.language)
            .field("cached_lines", &self.line_token_cache.len())
            .field("incremental_edit_count", &self.incremental_edit_count)
            .finish()
    }
}

impl Clone for SyntaxHighlighter {
    fn clone(&self) -> Self {
        let mut cloned = Self::with_language(self.language);
        if let Some(ref custom) = self.custom_ts_language {
            cloned.register_tree_sitter_language(custom.clone());
        }
        cloned.tree = self.tree.clone();
        cloned.line_entry_states = self.line_entry_states.clone();
        cloned.line_token_cache = self.line_token_cache.clone();
        cloned.incremental_edit_count = self.incremental_edit_count;
        cloned.last_changed_lines = self.last_changed_lines.clone();
        cloned
    }
}

impl Default for SyntaxHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl SyntaxHighlighter {
    pub fn new() -> Self {
        Self::with_language(Language::Rust)
    }

    pub fn with_language(lang: Language) -> Self {
        let parser = create_parser_for_lang(lang);
        Self {
            parser,
            custom_ts_language: None,
            tree: None,
            language: lang,
            line_entry_states: Vec::new(),
            line_token_cache: Vec::new(),
            incremental_edit_count: 0,
            last_changed_lines: Vec::new(),
        }
    }

    /// Registers an external [`tree_sitter::Language`] grammar on this highlighter instance
    /// (e.g. if an application brings its own `tree-sitter-python` or `tree-sitter-cpp` crate).
    pub fn register_tree_sitter_language(&mut self, ts_lang: tree_sitter::Language) -> bool {
        let mut parser = Parser::new();
        if parser.set_language(&ts_lang).is_ok() {
            self.parser = Some(parser);
            self.custom_ts_language = Some(ts_lang);
            self.tree = None;
            self.line_token_cache.clear();
            true
        } else {
            false
        }
    }

    pub fn set_language(&mut self, lang: Language) {
        if self.language != lang {
            self.language = lang;
            self.custom_ts_language = None;
            self.parser = create_parser_for_lang(lang);
            self.tree = None;
            self.line_entry_states.clear();
            self.line_token_cache.clear();
            self.last_changed_lines.clear();
        }
    }

    pub fn language(&self) -> Language {
        self.language
    }

    /// Number of incremental edits processed via [`Self::edit`] / [`Self::apply_input_edit_and_reparse`].
    pub fn incremental_edit_count(&self) -> u64 {
        self.incremental_edit_count
    }

    /// Returns the line ranges invalidated during the most recent incremental reparse.
    pub fn last_changed_lines(&self) -> &[Range<usize>] {
        &self.last_changed_lines
    }

    /// Accesses the active Tree-Sitter AST tree, if parsed.
    pub fn tree(&self) -> Option<&Tree> {
        self.tree.as_ref()
    }

    /// Parses a complete source text into a Tree-Sitter AST and initializes line lexer states.
    pub fn parse(&mut self, source: &str) {
        if let Some(ref mut parser) = self.parser {
            self.tree = parser.parse(source, None);
        } else {
            self.tree = None;
        }
        let buf = LineBuffer::from_str(source);
        self.rebuild_line_states(&buf);
    }

    /// Parses a [`LineBuffer`] directly via zero-copy rope slices (no full-document `String` copy).
    pub fn parse_buffer(&mut self, buffer: &LineBuffer) {
        self.tree = self.parse_rope_tree(buffer, None);
        self.rebuild_line_states(buffer);
    }

    /// Internal zero-copy Tree-Sitter rope callback parser using `Parser::parse_with_options`.
    fn parse_rope_tree(&mut self, buffer: &LineBuffer, old_tree: Option<&Tree>) -> Option<Tree> {
        let parser = self.parser.as_mut()?;
        let line_count = buffer.line_count();
        let mut read_cb = |_byte_offset: usize, position: Point| -> &[u8] {
            let row = position.row;
            let col = position.column;
            if row >= line_count {
                return b"";
            }
            if let Some(line) = buffer.line(row) {
                if col < line.len() {
                    return &line.as_bytes()[col..];
                } else if col == line.len() && row + 1 < line_count {
                    return b"\n";
                }
            }
            b""
        };
        parser.parse_with_options(&mut read_cb, old_tree, None)
    }

    /// Recomputes `line_entry_states` across the entire buffer and resets the per-line token cache.
    fn rebuild_line_states(&mut self, buffer: &LineBuffer) {
        let n = buffer.line_count();
        self.line_entry_states.clear();
        self.line_entry_states.reserve(n);
        self.line_token_cache.clear();
        self.line_token_cache.resize(n, None);

        let mut cur_state = LineLexState::Normal;
        for row in 0..n {
            self.line_entry_states.push(cur_state.clone());
            if let Some(line) = buffer.line(row) {
                let (_, next_state) = Self::scan_line_with_state(line, self.language, &cur_state);
                cur_state = next_state;
            }
        }
        self.last_changed_lines = vec![0..n];
    }

    /// Informs the Tree-Sitter AST and incremental line-state cache of an edit before re-parsing.
    pub fn edit(&mut self, edit: &InputEdit) {
        if let Some(ref mut tree) = self.tree {
            tree.edit(edit);
        }
        self.apply_line_cache_splice(edit);
        self.incremental_edit_count = self.incremental_edit_count.saturating_add(1);
    }

    fn apply_line_cache_splice(&mut self, edit: &InputEdit) {
        if self.line_entry_states.is_empty() {
            return;
        }
        let start_row = edit.start_position.row;
        let old_end_row = edit.old_end_position.row;
        let new_end_row = edit.new_end_position.row;

        if old_end_row > new_end_row {
            let remove_count = old_end_row - new_end_row;
            let del_start = (new_end_row + 1).min(self.line_entry_states.len());
            let del_end = (del_start + remove_count).min(self.line_entry_states.len());
            if del_start < del_end {
                self.line_entry_states.drain(del_start..del_end);
                self.line_token_cache.drain(del_start..del_end);
            }
        } else if new_end_row > old_end_row {
            let insert_count = new_end_row - old_end_row;
            let ins_at = (old_end_row + 1).min(self.line_entry_states.len());
            let fill_state = self
                .line_entry_states
                .get(start_row)
                .cloned()
                .unwrap_or_default();
            self.line_entry_states
                .splice(ins_at..ins_at, std::iter::repeat_n(fill_state, insert_count));
            self.line_token_cache
                .splice(ins_at..ins_at, std::iter::repeat_n(None, insert_count));
        }

        // Invalidate token cache across the directly edited line span.
        let inv_start = start_row.min(self.line_token_cache.len());
        let inv_end = (new_end_row + 1).min(self.line_token_cache.len());
        for slot in &mut self.line_token_cache[inv_start..inv_end] {
            *slot = None;
        }
    }

    /// Re-parses the document incrementally from a full `&str` using the edited previous AST.
    pub fn parse_incremental(&mut self, source: &str) {
        let old_tree = self.tree.take();
        if let Some(ref mut parser) = self.parser {
            let new_tree = parser.parse(source, old_tree.as_ref());
            if let (Some(ref old_t), Some(ref new_t)) = (&old_tree, &new_tree) {
                self.last_changed_lines = old_t
                    .changed_ranges(new_t)
                    .map(|r| r.start_point.row..r.end_point.row.saturating_add(1))
                    .collect();
            }
            self.tree = new_tree;
        }
        let buf = LineBuffer::from_str(source);
        self.propagate_states_from(0, buf.line_count(), &buf);
    }

    /// Re-parses the [`LineBuffer`] incrementally via zero-copy rope slices after one or more
    /// [`Self::edit`] calls, invalidating ONLY the lines whose AST or multi-line lexer state changed.
    pub fn reparse_incremental_buffer(
        &mut self,
        buffer: &LineBuffer,
        dirty_start_row: usize,
        dirty_end_row: usize,
    ) -> &[Range<usize>] {
        let old_tree = self.tree.take();
        let new_tree = self.parse_rope_tree(buffer, old_tree.as_ref());
        let mut changed_ranges: Vec<Range<usize>> = Vec::new();
        let total_lines = buffer.line_count();

        if let (Some(ref old_t), Some(ref new_t)) = (&old_tree, &new_tree) {
            for r in old_t.changed_ranges(new_t) {
                let s = r.start_point.row.min(total_lines);
                let e = (r.end_point.row + 1).min(total_lines);
                if s < e {
                    changed_ranges.push(s..e);
                }
            }
        }
        self.tree = new_tree;

        if self.line_entry_states.len() != total_lines {
            self.line_entry_states.resize(total_lines, LineLexState::Normal);
            self.line_token_cache.resize(total_lines, None);
        }

        let prop_end = self.propagate_states_from(dirty_start_row, dirty_end_row, buffer);
        let base_s = dirty_start_row.min(total_lines);
        let base_e = prop_end.min(total_lines);
        if base_s < base_e {
            changed_ranges.push(base_s..base_e);
        }

        // Invalidate token cache for every changed range.
        for r in &changed_ranges {
            let s = r.start.min(self.line_token_cache.len());
            let e = r.end.min(self.line_token_cache.len());
            for slot in &mut self.line_token_cache[s..e] {
                *slot = None;
            }
        }

        self.last_changed_lines = changed_ranges;
        &self.last_changed_lines
    }

    /// Convenience single-edit incremental update: calls `tree.edit(edit)` and reparses via
    /// zero-copy rope slices in a single call.
    pub fn apply_input_edit_and_reparse(
        &mut self,
        edit: &InputEdit,
        buffer: &LineBuffer,
    ) -> &[Range<usize>] {
        self.edit(edit);
        self.reparse_incremental_buffer(
            buffer,
            edit.start_position.row,
            edit.new_end_position.row.saturating_add(1),
        )
    }

    /// Propagates multi-line lexer state forward from `start_row`, stopping early as soon as
    /// `row >= min_end_row` and the entry state of `row` matches the previously cached state.
    fn propagate_states_from(
        &mut self,
        start_row: usize,
        min_end_row: usize,
        buffer: &LineBuffer,
    ) -> usize {
        let total = buffer.line_count();
        if total == 0 {
            self.line_entry_states.clear();
            self.line_token_cache.clear();
            return 0;
        }
        if self.line_entry_states.len() != total {
            self.rebuild_line_states(buffer);
            return total;
        }

        let mut row = start_row.min(total - 1);
        let mut cur_state = self.line_entry_states[row].clone();
        let stop_floor = min_end_row.min(total);

        while row < total {
            self.line_entry_states[row] = cur_state.clone();
            self.line_token_cache[row] = None;
            let next_state = if let Some(line) = buffer.line(row) {
                Self::scan_line_with_state(line, self.language, &cur_state).1
            } else {
                LineLexState::Normal
            };
            row += 1;
            if row < total {
                if row >= stop_floor && self.line_entry_states[row] == next_state {
                    break;
                }
                cur_state = next_state;
            }
        }
        row
    }

    /// Returns cached syntax tokens for `line_idx`, computing and memoizing them lazily on cache miss.
    pub fn tokens_for_line_cached(&mut self, line_idx: usize, buffer: &LineBuffer) -> &[SyntaxToken] {
        let total = buffer.line_count();
        if line_idx >= total {
            return &[];
        }
        if self.line_token_cache.len() != total {
            self.rebuild_line_states(buffer);
        }
        if self.line_token_cache[line_idx].is_none() {
            let line = buffer.line(line_idx).unwrap_or("");
            let tokens = self.highlight_line_ast(line_idx, line);
            self.line_token_cache[line_idx] = Some(tokens);
        }
        self.line_token_cache[line_idx].as_deref().unwrap_or(&[])
    }

    /// Tokenizes a single line statelessly (backwards-compatible helper).
    pub fn highlight_line(&self, line: &str, lang: Language) -> Vec<SyntaxToken> {
        if line.is_empty() {
            return Vec::new();
        }
        Self::tokenize_line_lexical(line, lang)
    }

    /// Extracts syntax tokens for a line using the active Tree-Sitter AST when present,
    /// falling back to the incremental multi-line state tokenizer for non-Tree-Sitter languages.
    pub fn highlight_line_ast(&self, line_idx: usize, line: &str) -> Vec<SyntaxToken> {
        if line.is_empty() {
            return Vec::new();
        }

        if let Some(ref tree) = self.tree {
            let mut tokens = Vec::new();
            collect_leaf_tokens(tree.root_node(), line_idx, &mut tokens, line);
            if !tokens.is_empty() {
                tokens.sort_by_key(|t| t.start_col);
                return tokens;
            }
        }

        let entry_state = self
            .line_entry_states
            .get(line_idx)
            .cloned()
            .unwrap_or(LineLexState::Normal);
        Self::scan_line_with_state(line, self.language, &entry_state).0
    }

    /// Stateless single-line tokenizer returning tokens with UTF-8 byte offsets.
    pub fn tokenize_line_lexical(line: &str, lang: Language) -> Vec<SyntaxToken> {
        if line.is_empty() {
            return Vec::new();
        }
        Self::scan_line_with_state(line, lang, &LineLexState::Normal).0
    }

    /// Core 10-language incremental line scanner returning `(tokens_with_byte_offsets, exit_state)`.
    pub fn scan_line_with_state(
        line: &str,
        lang: Language,
        entry_state: &LineLexState,
    ) -> (Vec<SyntaxToken>, LineLexState) {
        match lang {
            Language::Json => (Self::scan_json_line(line), LineLexState::Normal),
            Language::Yaml => Self::scan_yaml_line(line, entry_state),
            Language::Toml => Self::scan_toml_line(line, entry_state),
            Language::XmlSdf | Language::Html => Self::scan_xml_line(line, entry_state),
            Language::Markdown => Self::scan_markdown_line(line, entry_state),
            Language::Shell => Self::scan_shell_line(line, entry_state),
            Language::Python => Self::scan_python_line(line, entry_state),
            Language::Cpp => Self::scan_c_like_line(line, lang, entry_state),
            Language::Rust | Language::JavaScript | Language::TypeScript => {
                Self::scan_c_like_line(line, lang, entry_state)
            }
            Language::PlainText => (Vec::new(), LineLexState::Normal),
        }
    }

    fn scan_json_line(line: &str) -> Vec<SyntaxToken> {
        let mut tokens = Vec::new();
        let bytes = line.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        while i < len {
            let b = bytes[i];
            if b.is_ascii_whitespace() {
                i += 1;
                continue;
            }
            // JSONC comments
            if b == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
                tokens.push(SyntaxToken {
                    token_type: TokenType::Comment,
                    start_col: i,
                    end_col: len,
                    text: line[i..len].to_string(),
                });
                break;
            }
            // Strings (distinguish object property keys followed by `:`)
            if b == b'"' {
                let start = i;
                i += 1;
                while i < len {
                    if bytes[i] == b'\\' && i + 1 < len {
                        i += 2;
                    } else if bytes[i] == b'"' {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
                let mut look = i;
                while look < len && bytes[look].is_ascii_whitespace() {
                    look += 1;
                }
                let tt = if look < len && bytes[look] == b':' {
                    TokenType::Property
                } else {
                    TokenType::String
                };
                tokens.push(SyntaxToken {
                    token_type: tt,
                    start_col: start,
                    end_col: i,
                    text: line[start..i].to_string(),
                });
                continue;
            }
            // Numbers (including negative and scientific notation)
            if b.is_ascii_digit() || (b == b'-' && i + 1 < len && bytes[i + 1].is_ascii_digit()) {
                let start = i;
                i += 1;
                while i < len
                    && (bytes[i].is_ascii_digit()
                        || matches!(bytes[i], b'.' | b'e' | b'E' | b'+' | b'-'))
                {
                    i += 1;
                }
                tokens.push(SyntaxToken {
                    token_type: TokenType::Number,
                    start_col: start,
                    end_col: i,
                    text: line[start..i].to_string(),
                });
                continue;
            }
            // Booleans & null
            if b.is_ascii_alphabetic() {
                let start = i;
                while i < len && bytes[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                let word = &line[start..i];
                let tt = match word {
                    "true" | "false" => TokenType::Boolean,
                    "null" => TokenType::Keyword,
                    _ => TokenType::Identifier,
                };
                tokens.push(SyntaxToken {
                    token_type: tt,
                    start_col: start,
                    end_col: i,
                    text: word.to_string(),
                });
                continue;
            }
            let ch_len = line[i..].chars().next().map_or(1, char::len_utf8);
            tokens.push(SyntaxToken {
                token_type: TokenType::Punctuation,
                start_col: i,
                end_col: i + ch_len,
                text: line[i..i + ch_len].to_string(),
            });
            i += ch_len;
        }
        tokens
    }

    fn scan_yaml_line(line: &str, entry_state: &LineLexState) -> (Vec<SyntaxToken>, LineLexState) {
        let indent = line.bytes().take_while(|&b| b == b' ').count();
        let trimmed = line.trim();

        if let LineLexState::YamlBlockScalar { base_indent } = *entry_state {
            if trimmed.is_empty() || indent > base_indent {
                let mut toks = Vec::new();
                if !trimmed.is_empty() {
                    toks.push(SyntaxToken {
                        token_type: TokenType::String,
                        start_col: indent,
                        end_col: line.len(),
                        text: line[indent..].to_string(),
                    });
                }
                return (toks, LineLexState::YamlBlockScalar { base_indent });
            }
        }

        let mut tokens = Vec::new();
        if trimmed.is_empty() {
            return (tokens, LineLexState::Normal);
        }
        if trimmed == "---" || trimmed == "..." {
            tokens.push(SyntaxToken {
                token_type: TokenType::Keyword,
                start_col: indent,
                end_col: indent + trimmed.len(),
                text: trimmed.to_string(),
            });
            return (tokens, LineLexState::Normal);
        }
        if trimmed.starts_with('#') {
            tokens.push(SyntaxToken {
                token_type: TokenType::Comment,
                start_col: indent,
                end_col: line.len(),
                text: line[indent..].to_string(),
            });
            return (tokens, LineLexState::Normal);
        }

        let mut pos = indent;
        let bytes = line.as_bytes();
        // Leading list dash `- `
        if bytes.get(pos) == Some(&b'-')
            && bytes.get(pos + 1).is_none_or(|b| b.is_ascii_whitespace())
        {
            tokens.push(SyntaxToken {
                token_type: TokenType::Punctuation,
                start_col: pos,
                end_col: pos + 1,
                text: "-".to_string(),
            });
            pos += 1;
            while pos < line.len() && bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }
        }

        // Check for `key:` pair
        let mut exit_state = LineLexState::Normal;
        if let Some(colon_rel) = find_yaml_mapping_colon(&line[pos..]) {
            let colon_abs = pos + colon_rel;
            let key_str = line[pos..colon_abs].trim_end();
            if !key_str.is_empty() {
                tokens.push(SyntaxToken {
                    token_type: TokenType::Property,
                    start_col: pos,
                    end_col: pos + key_str.len(),
                    text: key_str.to_string(),
                });
            }
            tokens.push(SyntaxToken {
                token_type: TokenType::Punctuation,
                start_col: colon_abs,
                end_col: colon_abs + 1,
                text: ":".to_string(),
            });
            pos = colon_abs + 1;
        }

        // Scan YAML value portion
        let mut i = pos;
        while i < line.len() {
            let b = bytes[i];
            if b.is_ascii_whitespace() {
                i += 1;
                continue;
            }
            if b == b'#' {
                tokens.push(SyntaxToken {
                    token_type: TokenType::Comment,
                    start_col: i,
                    end_col: line.len(),
                    text: line[i..].to_string(),
                });
                break;
            }
            if b == b'|' || b == b'>' {
                let start = i;
                i += 1;
                while i < line.len() && matches!(bytes[i], b'-' | b'+' | b'0'..=b'9') {
                    i += 1;
                }
                tokens.push(SyntaxToken {
                    token_type: TokenType::Operator,
                    start_col: start,
                    end_col: i,
                    text: line[start..i].to_string(),
                });
                exit_state = LineLexState::YamlBlockScalar {
                    base_indent: indent,
                };
                continue;
            }
            if b == b'&' || b == b'*' {
                let start = i;
                i += 1;
                while i < line.len() && !bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                tokens.push(SyntaxToken {
                    token_type: TokenType::Namespace,
                    start_col: start,
                    end_col: i,
                    text: line[start..i].to_string(),
                });
                continue;
            }
            if b == b'!' {
                let start = i;
                while i < line.len() && !bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                tokens.push(SyntaxToken {
                    token_type: TokenType::Type,
                    start_col: start,
                    end_col: i,
                    text: line[start..i].to_string(),
                });
                continue;
            }
            if b == b'"' || b == b'\'' {
                let quote = b;
                let start = i;
                i += 1;
                while i < line.len() {
                    if quote == b'"' && bytes[i] == b'\\' && i + 1 < line.len() {
                        i += 2;
                    } else if bytes[i] == quote {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
                tokens.push(SyntaxToken {
                    token_type: TokenType::String,
                    start_col: start,
                    end_col: i,
                    text: line[start..i].to_string(),
                });
                continue;
            }
            if matches!(b, b'[' | b']' | b'{' | b'}' | b',') {
                tokens.push(SyntaxToken {
                    token_type: TokenType::Punctuation,
                    start_col: i,
                    end_col: i + 1,
                    text: line[i..i + 1].to_string(),
                });
                i += 1;
                continue;
            }
            // Scalar word / number / bool
            let start = i;
            while i < line.len()
                && !matches!(bytes[i], b',' | b']' | b'}' | b'#')
                && !bytes[i].is_ascii_whitespace()
            {
                i += 1;
            }
            let word = &line[start..i];
            let tt = match word {
                "true" | "false" | "yes" | "no" | "on" | "off" => TokenType::Boolean,
                "null" | "~" => TokenType::Keyword,
                _ if is_numeric_literal(word) => TokenType::Number,
                _ => TokenType::String,
            };
            tokens.push(SyntaxToken {
                token_type: tt,
                start_col: start,
                end_col: i,
                text: word.to_string(),
            });
        }

        (tokens, exit_state)
    }

    fn scan_toml_line(line: &str, entry_state: &LineLexState) -> (Vec<SyntaxToken>, LineLexState) {
        let mut tokens = Vec::new();
        let bytes = line.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        if let LineLexState::TripleString { quote } = *entry_state {
            let delim = [quote, quote, quote];
            if let Some(pos) = find_bytes(&bytes[i..], &delim) {
                let end = i + pos + 3;
                tokens.push(SyntaxToken {
                    token_type: TokenType::String,
                    start_col: 0,
                    end_col: end,
                    text: line[0..end].to_string(),
                });
                i = end;
            } else {
                if !line.is_empty() {
                    tokens.push(SyntaxToken {
                        token_type: TokenType::String,
                        start_col: 0,
                        end_col: len,
                        text: line.to_string(),
                    });
                }
                return (tokens, LineLexState::TripleString { quote });
            }
        }

        let trimmed_start = line.bytes().take_while(|b| b.is_ascii_whitespace()).count();
        if i == 0 && bytes.get(trimmed_start) == Some(&b'[') {
            // Section header `[table]` or `[[array]]`
            let mut end = len;
            if let Some(comment_pos) = line[trimmed_start..].find('#') {
                end = trimmed_start + comment_pos;
            }
            let header = line[trimmed_start..end].trim_end();
            tokens.push(SyntaxToken {
                token_type: TokenType::Heading,
                start_col: trimmed_start,
                end_col: trimmed_start + header.len(),
                text: header.to_string(),
            });
            if end < len {
                tokens.push(SyntaxToken {
                    token_type: TokenType::Comment,
                    start_col: end,
                    end_col: len,
                    text: line[end..].to_string(),
                });
            }
            return (tokens, LineLexState::Normal);
        }

        let mut seen_eq = false;
        while i < len {
            let b = bytes[i];
            if b.is_ascii_whitespace() {
                i += 1;
                continue;
            }
            if b == b'#' {
                tokens.push(SyntaxToken {
                    token_type: TokenType::Comment,
                    start_col: i,
                    end_col: len,
                    text: line[i..].to_string(),
                });
                break;
            }
            // Triple-quoted strings
            if (b == b'"' || b == b'\'')
                && i + 2 < len
                && bytes[i + 1] == b
                && bytes[i + 2] == b
            {
                let start = i;
                let delim = [b, b, b];
                i += 3;
                if let Some(rel) = find_bytes(&bytes[i..], &delim) {
                    i += rel + 3;
                    tokens.push(SyntaxToken {
                        token_type: TokenType::String,
                        start_col: start,
                        end_col: i,
                        text: line[start..i].to_string(),
                    });
                    continue;
                } else {
                    tokens.push(SyntaxToken {
                        token_type: TokenType::String,
                        start_col: start,
                        end_col: len,
                        text: line[start..len].to_string(),
                    });
                    return (tokens, LineLexState::TripleString { quote: b });
                }
            }
            if b == b'"' || b == b'\'' {
                let start = i;
                let q = b;
                i += 1;
                while i < len {
                    if q == b'"' && bytes[i] == b'\\' && i + 1 < len {
                        i += 2;
                    } else if bytes[i] == q {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
                tokens.push(SyntaxToken {
                    token_type: if !seen_eq {
                        TokenType::Property
                    } else {
                        TokenType::String
                    },
                    start_col: start,
                    end_col: i,
                    text: line[start..i].to_string(),
                });
                continue;
            }
            if b == b'=' {
                seen_eq = true;
                tokens.push(SyntaxToken {
                    token_type: TokenType::Operator,
                    start_col: i,
                    end_col: i + 1,
                    text: "=".to_string(),
                });
                i += 1;
                continue;
            }
            if b.is_ascii_digit() || (b == b'-' && seen_eq && i + 1 < len && bytes[i + 1].is_ascii_digit()) {
                let start = i;
                i += 1;
                while i < len
                    && (bytes[i].is_ascii_alphanumeric()
                        || matches!(bytes[i], b'.' | b'_' | b'-' | b':' | b'+' | b'T' | b'Z'))
                {
                    i += 1;
                }
                tokens.push(SyntaxToken {
                    token_type: TokenType::Number,
                    start_col: start,
                    end_col: i,
                    text: line[start..i].to_string(),
                });
                continue;
            }
            if b.is_ascii_alphabetic() || b == b'_' {
                let start = i;
                while i < len && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'_' | b'-')) {
                    i += 1;
                }
                let word = &line[start..i];
                let tt = if !seen_eq {
                    TokenType::Property
                } else if word == "true" || word == "false" {
                    TokenType::Boolean
                } else {
                    TokenType::Identifier
                };
                tokens.push(SyntaxToken {
                    token_type: tt,
                    start_col: start,
                    end_col: i,
                    text: word.to_string(),
                });
                continue;
            }
            let ch_len = line[i..].chars().next().map_or(1, char::len_utf8);
            tokens.push(SyntaxToken {
                token_type: TokenType::Punctuation,
                start_col: i,
                end_col: i + ch_len,
                text: line[i..i + ch_len].to_string(),
            });
            i += ch_len;
        }
        (tokens, LineLexState::Normal)
    }

    fn scan_xml_line(line: &str, entry_state: &LineLexState) -> (Vec<SyntaxToken>, LineLexState) {
        let mut tokens = Vec::new();
        let bytes = line.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        match entry_state {
            LineLexState::XmlComment => {
                if let Some(rel) = find_bytes(&bytes[i..], b"-->") {
                    let end = i + rel + 3;
                    tokens.push(SyntaxToken {
                        token_type: TokenType::Comment,
                        start_col: 0,
                        end_col: end,
                        text: line[0..end].to_string(),
                    });
                    i = end;
                } else {
                    if !line.is_empty() {
                        tokens.push(SyntaxToken {
                            token_type: TokenType::Comment,
                            start_col: 0,
                            end_col: len,
                            text: line.to_string(),
                        });
                    }
                    return (tokens, LineLexState::XmlComment);
                }
            }
            LineLexState::XmlCdata => {
                if let Some(rel) = find_bytes(&bytes[i..], b"]]>") {
                    let end = i + rel + 3;
                    tokens.push(SyntaxToken {
                        token_type: TokenType::String,
                        start_col: 0,
                        end_col: end,
                        text: line[0..end].to_string(),
                    });
                    i = end;
                } else {
                    if !line.is_empty() {
                        tokens.push(SyntaxToken {
                            token_type: TokenType::String,
                            start_col: 0,
                            end_col: len,
                            text: line.to_string(),
                        });
                    }
                    return (tokens, LineLexState::XmlCdata);
                }
            }
            _ => {}
        }

        while i < len {
            let b = bytes[i];
            if b.is_ascii_whitespace() {
                i += 1;
                continue;
            }
            if bytes[i..].starts_with(b"<!--") {
                let start = i;
                if let Some(rel) = find_bytes(&bytes[i + 4..], b"-->") {
                    i = i + 4 + rel + 3;
                    tokens.push(SyntaxToken {
                        token_type: TokenType::Comment,
                        start_col: start,
                        end_col: i,
                        text: line[start..i].to_string(),
                    });
                    continue;
                } else {
                    tokens.push(SyntaxToken {
                        token_type: TokenType::Comment,
                        start_col: start,
                        end_col: len,
                        text: line[start..len].to_string(),
                    });
                    return (tokens, LineLexState::XmlComment);
                }
            }
            if bytes[i..].starts_with(b"<![CDATA[") {
                let start = i;
                if let Some(rel) = find_bytes(&bytes[i + 9..], b"]]>") {
                    i = i + 9 + rel + 3;
                    tokens.push(SyntaxToken {
                        token_type: TokenType::String,
                        start_col: start,
                        end_col: i,
                        text: line[start..i].to_string(),
                    });
                    continue;
                } else {
                    tokens.push(SyntaxToken {
                        token_type: TokenType::String,
                        start_col: start,
                        end_col: len,
                        text: line[start..len].to_string(),
                    });
                    return (tokens, LineLexState::XmlCdata);
                }
            }
            if b == b'<' {
                let p_start = i;
                i += 1;
                if i < len && (bytes[i] == b'/' || bytes[i] == b'?' || bytes[i] == b'!') {
                    i += 1;
                }
                tokens.push(SyntaxToken {
                    token_type: TokenType::Punctuation,
                    start_col: p_start,
                    end_col: i,
                    text: line[p_start..i].to_string(),
                });
                // Tag name
                let tag_start = i;
                while i < len
                    && (bytes[i].is_ascii_alphanumeric()
                        || matches!(bytes[i], b'_' | b'-' | b':' | b'.'))
                {
                    i += 1;
                }
                if i > tag_start {
                    tokens.push(SyntaxToken {
                        token_type: TokenType::Tag,
                        start_col: tag_start,
                        end_col: i,
                        text: line[tag_start..i].to_string(),
                    });
                }
                // Attributes until `>`
                while i < len {
                    if bytes[i].is_ascii_whitespace() {
                        i += 1;
                        continue;
                    }
                    if bytes[i] == b'>' {
                        tokens.push(SyntaxToken {
                            token_type: TokenType::Punctuation,
                            start_col: i,
                            end_col: i + 1,
                            text: ">".to_string(),
                        });
                        i += 1;
                        break;
                    }
                    if (bytes[i] == b'/' || bytes[i] == b'?') && i + 1 < len && bytes[i + 1] == b'>' {
                        tokens.push(SyntaxToken {
                            token_type: TokenType::Punctuation,
                            start_col: i,
                            end_col: i + 2,
                            text: line[i..i + 2].to_string(),
                        });
                        i += 2;
                        break;
                    }
                    if bytes[i] == b'=' {
                        tokens.push(SyntaxToken {
                            token_type: TokenType::Operator,
                            start_col: i,
                            end_col: i + 1,
                            text: "=".to_string(),
                        });
                        i += 1;
                        continue;
                    }
                    if bytes[i] == b'"' || bytes[i] == b'\'' {
                        let q = bytes[i];
                        let s_start = i;
                        i += 1;
                        while i < len && bytes[i] != q {
                            i += 1;
                        }
                        if i < len {
                            i += 1;
                        }
                        tokens.push(SyntaxToken {
                            token_type: TokenType::String,
                            start_col: s_start,
                            end_col: i,
                            text: line[s_start..i].to_string(),
                        });
                        continue;
                    }
                    let a_start = i;
                    while i < len
                        && !bytes[i].is_ascii_whitespace()
                        && !matches!(bytes[i], b'=' | b'>' | b'/' | b'?')
                    {
                        i += 1;
                    }
                    if i > a_start {
                        tokens.push(SyntaxToken {
                            token_type: TokenType::Attribute,
                            start_col: a_start,
                            end_col: i,
                            text: line[a_start..i].to_string(),
                        });
                    } else {
                        i += 1;
                    }
                }
                continue;
            }
            // Text / numeric payload between XML/SDF tags (e.g. `<pose>0 0 1.2 0 0 0</pose>`)
            let start = i;
            while i < len && bytes[i] != b'<' && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            let word = &line[start..i];
            let tt = if is_numeric_literal(word) {
                TokenType::Number
            } else if word == "true" || word == "false" {
                TokenType::Boolean
            } else {
                TokenType::Identifier
            };
            tokens.push(SyntaxToken {
                token_type: tt,
                start_col: start,
                end_col: i,
                text: word.to_string(),
            });
        }
        (tokens, LineLexState::Normal)
    }

    fn scan_markdown_line(
        line: &str,
        entry_state: &LineLexState,
    ) -> (Vec<SyntaxToken>, LineLexState) {
        let indent = line.bytes().take_while(|&b| b == b' ').count();
        let trimmed = &line[indent..];

        if let LineLexState::MarkdownFence {
            fence_char,
            fence_len,
            embedded,
        } = *entry_state
        {
            let run = trimmed.bytes().take_while(|&b| b == fence_char).count();
            if run >= fence_len as usize && trimmed[run..].trim().is_empty() {
                return (
                    vec![SyntaxToken {
                        token_type: TokenType::Punctuation,
                        start_col: indent,
                        end_col: line.len(),
                        text: trimmed.to_string(),
                    }],
                    LineLexState::Normal,
                );
            }
            let (toks, _) = Self::scan_line_with_state(line, embedded, &LineLexState::Normal);
            return (
                toks,
                LineLexState::MarkdownFence {
                    fence_char,
                    fence_len,
                    embedded,
                },
            );
        }

        // Opening fenced code block ```lang or ~~~lang
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let fc = trimmed.as_bytes()[0];
            let fl = trimmed.bytes().take_while(|&b| b == fc).count().min(255) as u8;
            let info = trimmed[fl as usize..].trim();
            let embedded = if info.is_empty() {
                Language::PlainText
            } else {
                Language::from_extension(info.split_whitespace().next().unwrap_or(""))
            };
            let mut toks = vec![SyntaxToken {
                token_type: TokenType::Punctuation,
                start_col: indent,
                end_col: indent + fl as usize,
                text: line[indent..indent + fl as usize].to_string(),
            }];
            if !info.is_empty() {
                let info_start = indent + fl as usize;
                toks.push(SyntaxToken {
                    token_type: TokenType::Type,
                    start_col: info_start,
                    end_col: line.len(),
                    text: line[info_start..].to_string(),
                });
            }
            return (
                toks,
                LineLexState::MarkdownFence {
                    fence_char: fc,
                    fence_len: fl,
                    embedded,
                },
            );
        }

        // ATX Headings `# ...`
        if trimmed.starts_with('#') {
            let hashes = trimmed.bytes().take_while(|&b| b == b'#').count();
            if (1..=6).contains(&hashes)
                && trimmed.as_bytes().get(hashes).is_none_or(|b| *b == b' ')
            {
                return (
                    vec![SyntaxToken {
                        token_type: TokenType::Heading,
                        start_col: indent,
                        end_col: line.len(),
                        text: trimmed.to_string(),
                    }],
                    LineLexState::Normal,
                );
            }
        }

        // Blockquote `> ...`
        if trimmed.starts_with('>') {
            return (
                vec![SyntaxToken {
                    token_type: TokenType::Comment,
                    start_col: indent,
                    end_col: line.len(),
                    text: trimmed.to_string(),
                }],
                LineLexState::Normal,
            );
        }

        let mut tokens = Vec::new();
        let bytes = line.as_bytes();
        let len = bytes.len();
        let mut i = indent;
        while i < len {
            if bytes[i] == b'`' {
                let start = i;
                i += 1;
                while i < len && bytes[i] != b'`' {
                    i += 1;
                }
                if i < len {
                    i += 1;
                }
                tokens.push(SyntaxToken {
                    token_type: TokenType::String,
                    start_col: start,
                    end_col: i,
                    text: line[start..i].to_string(),
                });
                continue;
            }
            if bytes[i] == b'[' {
                if let Some(close_br) = line[i..].find("](") {
                    let link_start = i;
                    let after_mid = i + close_br + 2;
                    if let Some(close_par) = line[after_mid..].find(')') {
                        let end = after_mid + close_par + 1;
                        tokens.push(SyntaxToken {
                            token_type: TokenType::Link,
                            start_col: link_start,
                            end_col: end,
                            text: line[link_start..end].to_string(),
                        });
                        i = end;
                        continue;
                    }
                }
            }
            if bytes[i..].starts_with(b"**") {
                if let Some(close) = line[i + 2..].find("**") {
                    let end = i + 2 + close + 2;
                    tokens.push(SyntaxToken {
                        token_type: TokenType::Keyword,
                        start_col: i,
                        end_col: end,
                        text: line[i..end].to_string(),
                    });
                    i = end;
                    continue;
                }
            }
            i += line[i..].chars().next().map_or(1, char::len_utf8);
        }
        (tokens, LineLexState::Normal)
    }

    fn scan_shell_line(line: &str, entry_state: &LineLexState) -> (Vec<SyntaxToken>, LineLexState) {
        if let LineLexState::ShellHeredoc { ref delimiter } = *entry_state {
            if line.trim() == delimiter.as_str() {
                return (
                    vec![SyntaxToken {
                        token_type: TokenType::Keyword,
                        start_col: 0,
                        end_col: line.len(),
                        text: line.to_string(),
                    }],
                    LineLexState::Normal,
                );
            }
            let toks = if line.is_empty() {
                Vec::new()
            } else {
                vec![SyntaxToken {
                    token_type: TokenType::String,
                    start_col: 0,
                    end_col: line.len(),
                    text: line.to_string(),
                }]
            };
            return (toks, entry_state.clone());
        }

        let mut tokens = Vec::new();
        let bytes = line.as_bytes();
        let len = bytes.len();
        let mut i = 0;
        let mut exit_state = LineLexState::Normal;

        // Shebang
        if line.starts_with("#!") {
            return (
                vec![SyntaxToken {
                    token_type: TokenType::DocComment,
                    start_col: 0,
                    end_col: len,
                    text: line.to_string(),
                }],
                LineLexState::Normal,
            );
        }

        while i < len {
            let b = bytes[i];
            if b.is_ascii_whitespace() {
                i += 1;
                continue;
            }
            if b == b'#' {
                tokens.push(SyntaxToken {
                    token_type: TokenType::Comment,
                    start_col: i,
                    end_col: len,
                    text: line[i..].to_string(),
                });
                break;
            }
            // Heredoc `<<EOF` / `<<-'EOF'`
            if b == b'<' && i + 1 < len && bytes[i + 1] == b'<' && bytes.get(i + 2) != Some(&b'<')
            {
                let start = i;
                i += 2;
                if i < len && bytes[i] == b'-' {
                    i += 1;
                }
                while i < len && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                let delim_start = i;
                while i < len && !bytes[i].is_ascii_whitespace() && bytes[i] != b';' {
                    i += 1;
                }
                let raw_delim = line[delim_start..i].trim_matches(|c| c == '\'' || c == '"');
                tokens.push(SyntaxToken {
                    token_type: TokenType::Operator,
                    start_col: start,
                    end_col: i,
                    text: line[start..i].to_string(),
                });
                if !raw_delim.is_empty() {
                    exit_state = LineLexState::ShellHeredoc {
                        delimiter: raw_delim.to_string(),
                    };
                }
                continue;
            }
            if b == b'$' {
                let start = i;
                i += 1;
                if i < len && bytes[i] == b'{' {
                    i += 1;
                    while i < len && bytes[i] != b'}' {
                        i += 1;
                    }
                    if i < len {
                        i += 1;
                    }
                } else {
                    while i < len
                        && (bytes[i].is_ascii_alphanumeric()
                            || matches!(bytes[i], b'_' | b'?' | b'#' | b'@' | b'*' | b'$'))
                    {
                        i += 1;
                    }
                }
                tokens.push(SyntaxToken {
                    token_type: TokenType::Variable,
                    start_col: start,
                    end_col: i,
                    text: line[start..i].to_string(),
                });
                continue;
            }
            if b == b'"' || b == b'\'' {
                let q = b;
                let start = i;
                i += 1;
                while i < len {
                    if q == b'"' && bytes[i] == b'\\' && i + 1 < len {
                        i += 2;
                    } else if bytes[i] == q {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
                tokens.push(SyntaxToken {
                    token_type: TokenType::String,
                    start_col: start,
                    end_col: i,
                    text: line[start..i].to_string(),
                });
                continue;
            }
            if b == b'-' && i + 1 < len && (bytes[i + 1].is_ascii_alphabetic() || bytes[i + 1] == b'-')
            {
                let start = i;
                while i < len
                    && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'-' | b'_'))
                {
                    i += 1;
                }
                tokens.push(SyntaxToken {
                    token_type: TokenType::Attribute,
                    start_col: start,
                    end_col: i,
                    text: line[start..i].to_string(),
                });
                continue;
            }
            if b.is_ascii_digit() {
                let start = i;
                while i < len && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                    i += 1;
                }
                tokens.push(SyntaxToken {
                    token_type: TokenType::Number,
                    start_col: start,
                    end_col: i,
                    text: line[start..i].to_string(),
                });
                continue;
            }
            if b.is_ascii_alphabetic() || b == b'_' {
                let start = i;
                while i < len
                    && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'_' | b'-'))
                {
                    i += 1;
                }
                let word = &line[start..i];
                let tt = if matches!(
                    word,
                    "if" | "then"
                        | "else"
                        | "elif"
                        | "fi"
                        | "for"
                        | "while"
                        | "until"
                        | "do"
                        | "done"
                        | "case"
                        | "esac"
                        | "in"
                        | "return"
                        | "break"
                        | "continue"
                        | "exit"
                ) {
                    TokenType::ControlFlow
                } else if matches!(
                    word,
                    "function"
                        | "local"
                        | "export"
                        | "readonly"
                        | "declare"
                        | "unset"
                        | "set"
                        | "source"
                        | "eval"
                        | "exec"
                ) {
                    TokenType::Keyword
                } else if matches!(
                    word,
                    "echo" | "printf" | "cd" | "pwd" | "test" | "grep" | "sed" | "awk" | "cargo"
                        | "git"
                ) {
                    TokenType::Function
                } else if bytes.get(i) == Some(&b'=') {
                    TokenType::Variable
                } else {
                    TokenType::Identifier
                };
                tokens.push(SyntaxToken {
                    token_type: tt,
                    start_col: start,
                    end_col: i,
                    text: word.to_string(),
                });
                continue;
            }
            let ch_len = line[i..].chars().next().map_or(1, char::len_utf8);
            let tt = if matches!(b, b'|' | b'&' | b'>' | b'<' | b'=' | b'!') {
                TokenType::Operator
            } else {
                TokenType::Punctuation
            };
            tokens.push(SyntaxToken {
                token_type: tt,
                start_col: i,
                end_col: i + ch_len,
                text: line[i..i + ch_len].to_string(),
            });
            i += ch_len;
        }
        (tokens, exit_state)
    }

    fn scan_python_line(
        line: &str,
        entry_state: &LineLexState,
    ) -> (Vec<SyntaxToken>, LineLexState) {
        let mut tokens = Vec::new();
        let bytes = line.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        if let LineLexState::TripleString { quote } = *entry_state {
            let delim = [quote, quote, quote];
            if let Some(rel) = find_bytes(&bytes[i..], &delim) {
                let end = i + rel + 3;
                tokens.push(SyntaxToken {
                    token_type: TokenType::String,
                    start_col: 0,
                    end_col: end,
                    text: line[0..end].to_string(),
                });
                i = end;
            } else {
                if !line.is_empty() {
                    tokens.push(SyntaxToken {
                        token_type: TokenType::String,
                        start_col: 0,
                        end_col: len,
                        text: line.to_string(),
                    });
                }
                return (tokens, LineLexState::TripleString { quote });
            }
        }

        while i < len {
            let b = bytes[i];
            if b.is_ascii_whitespace() {
                i += 1;
                continue;
            }
            if b == b'#' {
                tokens.push(SyntaxToken {
                    token_type: TokenType::Comment,
                    start_col: i,
                    end_col: len,
                    text: line[i..].to_string(),
                });
                break;
            }
            // Decorators `@dataclass`
            if b == b'@' {
                let start = i;
                i += 1;
                while i < len
                    && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'_' | b'.'))
                {
                    i += 1;
                }
                tokens.push(SyntaxToken {
                    token_type: TokenType::Attribute,
                    start_col: start,
                    end_col: i,
                    text: line[start..i].to_string(),
                });
                continue;
            }
            // Prefixed strings (`f"..."`, `r"..."`, `b"..."`, `"""..."""`)
            let str_prefix_len = if matches!(b, b'f' | b'r' | b'b' | b'u' | b'F' | b'R' | b'B' | b'U')
                && bytes.get(i + 1).is_some_and(|&nb| nb == b'"' || nb == b'\'')
            {
                1
            } else if i + 2 < len
                && matches!(b, b'f' | b'r' | b'b' | b'F' | b'R' | b'B')
                && matches!(bytes[i + 1], b'f' | b'r' | b'b' | b'F' | b'R' | b'B')
                && matches!(bytes[i + 2], b'"' | b'\'')
            {
                2
            } else {
                0
            };
            if str_prefix_len > 0 || b == b'"' || b == b'\'' {
                let start = i;
                let q_pos = i + str_prefix_len;
                let q = bytes[q_pos];
                if q_pos + 2 < len && bytes[q_pos + 1] == q && bytes[q_pos + 2] == q {
                    let delim = [q, q, q];
                    let body_start = q_pos + 3;
                    if let Some(rel) = find_bytes(&bytes[body_start..], &delim) {
                        i = body_start + rel + 3;
                        tokens.push(SyntaxToken {
                            token_type: TokenType::String,
                            start_col: start,
                            end_col: i,
                            text: line[start..i].to_string(),
                        });
                        continue;
                    } else {
                        tokens.push(SyntaxToken {
                            token_type: TokenType::String,
                            start_col: start,
                            end_col: len,
                            text: line[start..len].to_string(),
                        });
                        return (tokens, LineLexState::TripleString { quote: q });
                    }
                }
                i = q_pos + 1;
                while i < len {
                    if bytes[i] == b'\\' && i + 1 < len {
                        i += 2;
                    } else if bytes[i] == q {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
                tokens.push(SyntaxToken {
                    token_type: TokenType::String,
                    start_col: start,
                    end_col: i,
                    text: line[start..i].to_string(),
                });
                continue;
            }
            if b.is_ascii_digit() {
                let start = i;
                while i < len
                    && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'.' | b'_'))
                {
                    i += 1;
                }
                tokens.push(SyntaxToken {
                    token_type: TokenType::Number,
                    start_col: start,
                    end_col: i,
                    text: line[start..i].to_string(),
                });
                continue;
            }
            let ch = line[i..].chars().next().unwrap();
            if ch.is_alphabetic() || ch == '_' {
                let start = i;
                while i < len {
                    let c = line[i..].chars().next().unwrap();
                    if c.is_alphanumeric() || c == '_' {
                        i += c.len_utf8();
                    } else {
                        break;
                    }
                }
                let word = &line[start..i];
                let tt = if matches!(
                    word,
                    "True" | "False"
                ) {
                    TokenType::Boolean
                } else if Self::is_keyword(word, Language::Python) {
                    TokenType::Keyword
                } else if Self::is_type(word, Language::Python) {
                    TokenType::Type
                } else if bytes.get(i) == Some(&b'(') {
                    TokenType::Function
                } else {
                    TokenType::Identifier
                };
                tokens.push(SyntaxToken {
                    token_type: tt,
                    start_col: start,
                    end_col: i,
                    text: word.to_string(),
                });
                continue;
            }
            let ch_len = ch.len_utf8();
            let tt = if matches!(
                b,
                b'+' | b'-' | b'*' | b'/' | b'%' | b'=' | b'<' | b'>' | b'!' | b'&' | b'|' | b'^'
            ) {
                TokenType::Operator
            } else {
                TokenType::Punctuation
            };
            tokens.push(SyntaxToken {
                token_type: tt,
                start_col: i,
                end_col: i + ch_len,
                text: line[i..i + ch_len].to_string(),
            });
            i += ch_len;
        }
        (tokens, LineLexState::Normal)
    }

    fn scan_c_like_line(
        line: &str,
        lang: Language,
        entry_state: &LineLexState,
    ) -> (Vec<SyntaxToken>, LineLexState) {
        let mut tokens = Vec::new();
        let bytes = line.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        match *entry_state {
            LineLexState::BlockComment { mut depth, doc } => {
                let start = 0;
                while i + 1 < len {
                    if lang == Language::Rust && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                        depth += 1;
                        i += 2;
                    } else if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        depth = depth.saturating_sub(1);
                        i += 2;
                        if depth == 0 {
                            tokens.push(SyntaxToken {
                                token_type: if doc {
                                    TokenType::DocComment
                                } else {
                                    TokenType::Comment
                                },
                                start_col: start,
                                end_col: i,
                                text: line[start..i].to_string(),
                            });
                            break;
                        }
                    } else {
                        i += 1;
                    }
                }
                if depth > 0 {
                    if !line.is_empty() {
                        tokens.push(SyntaxToken {
                            token_type: if doc {
                                TokenType::DocComment
                            } else {
                                TokenType::Comment
                            },
                            start_col: 0,
                            end_col: len,
                            text: line.to_string(),
                        });
                    }
                    return (tokens, LineLexState::BlockComment { depth, doc });
                }
            }
            LineLexState::RustRawString { hashes } => {
                if let Some(end) = find_rust_raw_string_end(bytes, 0, hashes) {
                    tokens.push(SyntaxToken {
                        token_type: TokenType::String,
                        start_col: 0,
                        end_col: end,
                        text: line[0..end].to_string(),
                    });
                    i = end;
                } else {
                    if !line.is_empty() {
                        tokens.push(SyntaxToken {
                            token_type: TokenType::String,
                            start_col: 0,
                            end_col: len,
                            text: line.to_string(),
                        });
                    }
                    return (tokens, LineLexState::RustRawString { hashes });
                }
            }
            _ => {}
        }

        // C/C++ preprocessor directive lines (`#include`, `#define`, `#ifdef`, `#endif`)
        if lang == Language::Cpp && i == 0 {
            let ws = line.bytes().take_while(|b| b.is_ascii_whitespace()).count();
            if bytes.get(ws) == Some(&b'#') {
                let mut d_end = ws + 1;
                while d_end < len && bytes[d_end].is_ascii_whitespace() {
                    d_end += 1;
                }
                while d_end < len && (bytes[d_end].is_ascii_alphanumeric() || bytes[d_end] == b'_')
                {
                    d_end += 1;
                }
                tokens.push(SyntaxToken {
                    token_type: TokenType::Macro,
                    start_col: ws,
                    end_col: d_end,
                    text: line[ws..d_end].to_string(),
                });
                i = d_end;
                // Highlight `<header.h>` right after `#include`
                if line[ws..d_end].contains("include") {
                    while i < len && bytes[i].is_ascii_whitespace() {
                        i += 1;
                    }
                    if bytes.get(i) == Some(&b'<') {
                        let s = i;
                        if let Some(rel) = line[i..].find('>') {
                            i += rel + 1;
                            tokens.push(SyntaxToken {
                                token_type: TokenType::String,
                                start_col: s,
                                end_col: i,
                                text: line[s..i].to_string(),
                            });
                        }
                    }
                }
            }
        }

        while i < len {
            let b = bytes[i];
            if b.is_ascii_whitespace() {
                i += 1;
                continue;
            }
            // Line comments
            if b == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
                let is_doc = bytes.get(i + 2).is_some_and(|&c| c == b'/' || c == b'!');
                tokens.push(SyntaxToken {
                    token_type: if is_doc {
                        TokenType::DocComment
                    } else {
                        TokenType::Comment
                    },
                    start_col: i,
                    end_col: len,
                    text: line[i..len].to_string(),
                });
                break;
            }
            // Block comments `/* ... */`
            if b == b'/' && i + 1 < len && bytes[i + 1] == b'*' {
                let start = i;
                let doc = bytes.get(i + 2) == Some(&b'*') && bytes.get(i + 3) != Some(&b'/');
                let mut depth = 1u16;
                i += 2;
                while i + 1 < len {
                    if lang == Language::Rust && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                        depth += 1;
                        i += 2;
                    } else if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        depth -= 1;
                        i += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        i += 1;
                    }
                }
                if depth > 0 {
                    tokens.push(SyntaxToken {
                        token_type: if doc {
                            TokenType::DocComment
                        } else {
                            TokenType::Comment
                        },
                        start_col: start,
                        end_col: len,
                        text: line[start..len].to_string(),
                    });
                    return (tokens, LineLexState::BlockComment { depth, doc });
                }
                tokens.push(SyntaxToken {
                    token_type: if doc {
                        TokenType::DocComment
                    } else {
                        TokenType::Comment
                    },
                    start_col: start,
                    end_col: i,
                    text: line[start..i].to_string(),
                });
                continue;
            }
            // Rust raw strings `r#"..."#`
            if lang == Language::Rust && b == b'r' {
                let mut probe = i + 1;
                let mut hashes = 0u8;
                while probe < len && bytes[probe] == b'#' && hashes < 250 {
                    hashes += 1;
                    probe += 1;
                }
                if probe < len && bytes[probe] == b'"' {
                    let start = i;
                    if let Some(end) = find_rust_raw_string_end(bytes, probe + 1, hashes) {
                        tokens.push(SyntaxToken {
                            token_type: TokenType::String,
                            start_col: start,
                            end_col: end,
                            text: line[start..end].to_string(),
                        });
                        i = end;
                        continue;
                    } else {
                        tokens.push(SyntaxToken {
                            token_type: TokenType::String,
                            start_col: start,
                            end_col: len,
                            text: line[start..len].to_string(),
                        });
                        return (tokens, LineLexState::RustRawString { hashes });
                    }
                }
            }
            // Rust attributes `#[...]` / `#![...]`
            if lang == Language::Rust
                && b == b'#'
                && (bytes.get(i + 1) == Some(&b'[')
                    || (bytes.get(i + 1) == Some(&b'!') && bytes.get(i + 2) == Some(&b'[')))
            {
                let start = i;
                while i < len && bytes[i] != b']' {
                    i += 1;
                }
                if i < len {
                    i += 1;
                }
                tokens.push(SyntaxToken {
                    token_type: TokenType::Attribute,
                    start_col: start,
                    end_col: i,
                    text: line[start..i].to_string(),
                });
                continue;
            }
            // Strings (`"..."`, `'...'`, `` `...` ``)
            if b == b'"' || b == b'`' || (b == b'\'' && lang != Language::Rust) {
                let quote = b;
                let start = i;
                i += 1;
                while i < len {
                    if bytes[i] == b'\\' && i + 1 < len {
                        i += 2;
                    } else if bytes[i] == quote {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
                tokens.push(SyntaxToken {
                    token_type: TokenType::String,
                    start_col: start,
                    end_col: i,
                    text: line[start..i].to_string(),
                });
                continue;
            }
            // Rust char literal vs lifetime `'a`
            if b == b'\'' && lang == Language::Rust {
                let start = i;
                if i + 2 < len && bytes[i + 1] == b'\\' {
                    i += 3;
                    while i < len && bytes[i] != b'\'' {
                        i += 1;
                    }
                    if i < len {
                        i += 1;
                    }
                    tokens.push(SyntaxToken {
                        token_type: TokenType::String,
                        start_col: start,
                        end_col: i,
                        text: line[start..i].to_string(),
                    });
                    continue;
                }
                if let Some(ch) = line[i + 1..].chars().next() {
                    let after = i + 1 + ch.len_utf8();
                    if bytes.get(after) == Some(&b'\'') {
                        i = after + 1;
                        tokens.push(SyntaxToken {
                            token_type: TokenType::String,
                            start_col: start,
                            end_col: i,
                            text: line[start..i].to_string(),
                        });
                        continue;
                    }
                }
                // Lifetime `'a`, `'static`
                i += 1;
                while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                tokens.push(SyntaxToken {
                    token_type: TokenType::Namespace,
                    start_col: start,
                    end_col: i,
                    text: line[start..i].to_string(),
                });
                continue;
            }
            // Numeric literals (hex, binary, float, suffixes)
            if b.is_ascii_digit() {
                let start = i;
                while i < len
                    && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'.' | b'_'))
                {
                    i += 1;
                }
                tokens.push(SyntaxToken {
                    token_type: TokenType::Number,
                    start_col: start,
                    end_col: i,
                    text: line[start..i].to_string(),
                });
                continue;
            }
            // Identifiers, keywords, macros, types, functions
            let ch = line[i..].chars().next().unwrap();
            if ch.is_alphabetic() || ch == '_' {
                let start = i;
                while i < len {
                    let c = line[i..].chars().next().unwrap();
                    if c.is_alphanumeric() || c == '_' {
                        i += c.len_utf8();
                    } else {
                        break;
                    }
                }
                let word = &line[start..i];
                let tt = if lang == Language::Rust && bytes.get(i) == Some(&b'!') {
                    i += 1;
                    TokenType::Macro
                } else if word == "true" || word == "false" {
                    TokenType::Boolean
                } else if Self::is_keyword(word, lang) {
                    TokenType::Keyword
                } else if Self::is_type(word, lang) {
                    TokenType::Type
                } else if bytes.get(i) == Some(&b'(') {
                    TokenType::Function
                } else if bytes.get(i) == Some(&b':') && bytes.get(i + 1) == Some(&b':') {
                    TokenType::Namespace
                } else {
                    TokenType::Identifier
                };
                tokens.push(SyntaxToken {
                    token_type: tt,
                    start_col: start,
                    end_col: i,
                    text: line[start..i].to_string(),
                });
                continue;
            }
            // Operators and punctuation
            let ch_len = ch.len_utf8();
            let tt = if matches!(
                b,
                b'+' | b'-' | b'*' | b'/' | b'%' | b'=' | b'<' | b'>' | b'!' | b'&' | b'|' | b'^'
            ) {
                TokenType::Operator
            } else {
                TokenType::Punctuation
            };
            tokens.push(SyntaxToken {
                token_type: tt,
                start_col: i,
                end_col: i + ch_len,
                text: line[i..i + ch_len].to_string(),
            });
            i += ch_len;
        }

        (tokens, LineLexState::Normal)
    }

    fn is_keyword(word: &str, lang: Language) -> bool {
        match lang {
            Language::Rust => matches!(
                word,
                "fn" | "let"
                    | "mut"
                    | "pub"
                    | "struct"
                    | "enum"
                    | "impl"
                    | "trait"
                    | "match"
                    | "if"
                    | "else"
                    | "return"
                    | "for"
                    | "in"
                    | "while"
                    | "loop"
                    | "async"
                    | "await"
                    | "use"
                    | "mod"
                    | "crate"
                    | "const"
                    | "static"
                    | "type"
                    | "where"
                    | "as"
                    | "ref"
                    | "move"
                    | "unsafe"
                    | "dyn"
                    | "break"
                    | "continue"
                    | "super"
                    | "self"
                    | "Self"
            ),
            Language::TypeScript | Language::JavaScript => matches!(
                word,
                "function"
                    | "const"
                    | "let"
                    | "var"
                    | "return"
                    | "if"
                    | "else"
                    | "import"
                    | "export"
                    | "default"
                    | "from"
                    | "class"
                    | "interface"
                    | "type"
                    | "new"
                    | "this"
                    | "async"
                    | "await"
                    | "try"
                    | "catch"
                    | "finally"
                    | "switch"
                    | "case"
                    | "break"
                    | "continue"
                    | "throw"
                    | "extends"
                    | "implements"
                    | "null"
                    | "undefined"
                    | "readonly"
                    | "enum"
            ),
            Language::Python => matches!(
                word,
                "def"
                    | "class"
                    | "return"
                    | "if"
                    | "elif"
                    | "else"
                    | "for"
                    | "in"
                    | "while"
                    | "import"
                    | "from"
                    | "as"
                    | "try"
                    | "except"
                    | "finally"
                    | "with"
                    | "lambda"
                    | "pass"
                    | "break"
                    | "continue"
                    | "yield"
                    | "async"
                    | "await"
                    | "raise"
                    | "assert"
                    | "global"
                    | "nonlocal"
                    | "del"
                    | "not"
                    | "and"
                    | "or"
                    | "is"
                    | "match"
                    | "case"
                    | "None"
                    | "True"
                    | "False"
            ),
            Language::Cpp => matches!(
                word,
                "auto"
                    | "break"
                    | "case"
                    | "catch"
                    | "class"
                    | "concept"
                    | "const"
                    | "consteval"
                    | "constexpr"
                    | "constinit"
                    | "continue"
                    | "co_await"
                    | "co_return"
                    | "co_yield"
                    | "default"
                    | "delete"
                    | "do"
                    | "else"
                    | "enum"
                    | "explicit"
                    | "extern"
                    | "final"
                    | "for"
                    | "friend"
                    | "goto"
                    | "if"
                    | "inline"
                    | "mutable"
                    | "namespace"
                    | "new"
                    | "noexcept"
                    | "nullptr"
                    | "operator"
                    | "override"
                    | "private"
                    | "protected"
                    | "public"
                    | "requires"
                    | "return"
                    | "sizeof"
                    | "static"
                    | "static_assert"
                    | "struct"
                    | "switch"
                    | "template"
                    | "this"
                    | "throw"
                    | "try"
                    | "typedef"
                    | "typename"
                    | "union"
                    | "using"
                    | "virtual"
                    | "volatile"
                    | "while"
            ),
            Language::Json => matches!(word, "true" | "false" | "null"),
            _ => false,
        }
    }

    fn is_type(word: &str, lang: Language) -> bool {
        match lang {
            Language::Rust => {
                word.chars().next().is_some_and(|c| c.is_uppercase())
                    || matches!(
                        word,
                        "u8" | "u16"
                            | "u32"
                            | "u64"
                            | "u128"
                            | "usize"
                            | "i8"
                            | "i16"
                            | "i32"
                            | "i64"
                            | "i128"
                            | "isize"
                            | "f32"
                            | "f64"
                            | "bool"
                            | "char"
                            | "str"
                            | "String"
                            | "Vec"
                            | "Option"
                            | "Result"
                    )
            }
            Language::TypeScript => {
                matches!(
                    word,
                    "string"
                        | "number"
                        | "boolean"
                        | "any"
                        | "void"
                        | "never"
                        | "unknown"
                        | "Record"
                        | "Array"
                        | "Promise"
                ) || word.chars().next().is_some_and(|c| c.is_uppercase())
            }
            Language::Cpp => {
                matches!(
                    word,
                    "void"
                        | "bool"
                        | "char"
                        | "short"
                        | "int"
                        | "long"
                        | "float"
                        | "double"
                        | "signed"
                        | "unsigned"
                        | "size_t"
                        | "int8_t"
                        | "int16_t"
                        | "int32_t"
                        | "int64_t"
                        | "uint8_t"
                        | "uint16_t"
                        | "uint32_t"
                        | "uint64_t"
                        | "string"
                        | "vector"
                        | "unique_ptr"
                        | "shared_ptr"
                        | "optional"
                        | "span"
                )
            }
            Language::Python => {
                matches!(
                    word,
                    "int" | "float" | "str" | "bool" | "bytes" | "list" | "dict" | "set" | "tuple"
                        | "Any" | "Optional" | "Union" | "Callable" | "Sequence"
                ) || word.chars().next().is_some_and(|c| c.is_uppercase())
            }
            _ => false,
        }
    }

    /// Returns the root node of the active Tree-Sitter syntax tree, if available.
    pub fn root_node(&self) -> Option<Node<'_>> {
        self.tree.as_ref().map(|t| t.root_node())
    }

    /// Finds the finest AST node at the given 0-indexed row and byte column.
    pub fn node_at_position(&self, line: usize, col: usize) -> Option<Node<'_>> {
        let tree = self.tree.as_ref()?;
        let pt = Point {
            row: line,
            column: col,
        };
        tree.root_node().descendant_for_point_range(pt, pt)
    }

    /// Finds the enclosing scope or block range (start_line, end_line) containing the given position.
    pub fn enclosing_scope_range(&self, line: usize, col: usize) -> Option<(usize, usize)> {
        let mut node = self.node_at_position(line, col)?;
        loop {
            let kind = node.kind();
            if matches!(
                kind,
                "function_item"
                    | "impl_item"
                    | "struct_item"
                    | "enum_item"
                    | "trait_item"
                    | "block"
                    | "statement_block"
                    | "class_declaration"
                    | "function_declaration"
            ) {
                return Some((node.start_position().row, node.end_position().row));
            }
            node = node.parent()?;
        }
    }

    /// Finds matching open or close bracket for delimiter at position `pos`.
    pub fn find_matching_bracket(&self, buffer: &LineBuffer, pos: Position) -> Option<Position> {
        let line = buffer.line(pos.line)?;
        let ch = line.chars().nth(pos.col)?;

        let (target, is_forward) = match ch {
            '(' => (')', true),
            '[' => (']', true),
            '{' => ('}', true),
            '<' if matches!(self.language, Language::XmlSdf | Language::Html) => ('>', true),
            ')' => ('(', false),
            ']' => ('[', false),
            '}' => ('{', false),
            '>' if matches!(self.language, Language::XmlSdf | Language::Html) => ('<', false),
            _ => return None,
        };

        let mut depth = 0;

        if is_forward {
            for l in pos.line..buffer.line_count() {
                if let Some(text) = buffer.line(l) {
                    let start_c = if l == pos.line { pos.col } else { 0 };
                    for (c, cur) in text.chars().enumerate().skip(start_c) {
                        if cur == ch {
                            depth += 1;
                        } else if cur == target {
                            depth -= 1;
                            if depth == 0 {
                                return Some(Position::new(l, c));
                            }
                        }
                    }
                }
            }
        } else {
            for l in (0..=pos.line).rev() {
                if let Some(text) = buffer.line(l) {
                    let chars: Vec<char> = text.chars().collect();
                    let start_c = if l == pos.line {
                        pos.col
                    } else {
                        chars.len().saturating_sub(1)
                    };
                    for c in (0..=start_c).rev() {
                        let cur = chars[c];
                        if cur == ch {
                            depth += 1;
                        } else if cur == target {
                            depth -= 1;
                            if depth == 0 {
                                return Some(Position::new(l, c));
                            }
                        }
                    }
                }
            }
        }

        None
    }

    /// Extracts structural multi-line code folding regions for all 10 supported languages.
    pub fn compute_fold_ranges(&self, buffer: &LineBuffer) -> Vec<FoldRange> {
        if let Some(ref tree) = self.tree {
            let mut ranges = Vec::new();
            collect_ast_fold_ranges(tree.root_node(), &mut ranges);
            if !ranges.is_empty() {
                ranges.sort_by_key(|r| (r.start_line, r.end_line));
                ranges.dedup_by_key(|r| (r.start_line, r.end_line));
                return ranges;
            }
        }

        let mut ranges = match self.language {
            Language::Python | Language::Yaml => compute_indent_fold_ranges(buffer),
            Language::Toml => compute_toml_fold_ranges(buffer),
            Language::XmlSdf | Language::Html => compute_xml_fold_ranges(buffer),
            Language::Markdown => compute_markdown_fold_ranges(buffer),
            Language::Shell => compute_shell_fold_ranges(buffer),
            Language::Cpp => {
                let mut r = compute_brace_fold_ranges(buffer, true);
                r.extend(compute_cpp_preproc_fold_ranges(buffer));
                r
            }
            Language::Json => compute_brace_fold_ranges(buffer, true),
            _ => compute_brace_fold_ranges(buffer, false),
        };

        ranges.sort_by_key(|r| (r.start_line, r.end_line));
        ranges.dedup_by_key(|r| (r.start_line, r.end_line));
        ranges
    }

    /// Collects real-time syntax diagnostics from the Tree-Sitter AST or structural validators
    /// (JSON, XML/SDF, TOML, unclosed brackets).
    pub fn collect_syntax_diagnostics(&self, buffer: &LineBuffer) -> Vec<SyntaxDiagnostic> {
        let mut out = Vec::new();
        if let Some(ref tree) = self.tree {
            collect_tree_sitter_errors(tree.root_node(), &mut out);
            return out;
        }
        match self.language {
            Language::Json => {
                let text = buffer.to_text();
                if !text.trim().is_empty() {
                    if let Err(err) = serde_json::from_str::<serde_json::Value>(&text) {
                        let line = err.line().saturating_sub(1);
                        let col = err.column().saturating_sub(1);
                        out.push(SyntaxDiagnostic {
                            start: Position::new(line, col),
                            end: Position::new(line, col + 1),
                            message: err.to_string(),
                            is_warning: false,
                        });
                    }
                }
            }
            Language::XmlSdf | Language::Html => {
                validate_xml_tags(buffer, &mut out);
            }
            _ => {}
        }
        out
    }

    /// Computes indentation column spaces for a newly created newline following line `line_idx`.
    pub fn compute_auto_indent(&self, buffer: &LineBuffer, line_idx: usize) -> usize {
        let prev_line = match buffer.line(line_idx) {
            Some(l) => l,
            None => return 0,
        };

        let current_indent = prev_line.chars().take_while(|c| *c == ' ').count();
        let trimmed = prev_line.trim_end();
        let indent_step = match self.language {
            Language::Yaml | Language::Json | Language::XmlSdf | Language::Html => 2,
            _ => 4,
        };

        if let Some(ref tree) = self.tree {
            let pt = Point {
                row: line_idx,
                column: prev_line.len().saturating_sub(1),
            };
            if let Some(node) = tree.root_node().descendant_for_point_range(pt, pt) {
                let kind = node.kind();
                if kind == "{" || kind == ":" || kind == "(" || kind == "[" {
                    return current_indent + indent_step;
                }
            }
        }

        if trimmed.ends_with('{')
            || trimmed.ends_with(':')
            || trimmed.ends_with('(')
            || trimmed.ends_with('[')
            || (matches!(self.language, Language::XmlSdf | Language::Html)
                && trimmed.ends_with('>')
                && !trimmed.ends_with("/>")
                && !trimmed.starts_with("</")
                && !trimmed.starts_with("<?")
                && !trimmed.starts_with("<!--"))
            || (self.language == Language::Shell
                && (trimmed.ends_with("then")
                    || trimmed.ends_with("do")
                    || trimmed.ends_with("else")))
        {
            current_indent + indent_step
        } else {
            current_indent
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers & Language-Specific Fold / Validator Implementations
// ---------------------------------------------------------------------------

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn find_rust_raw_string_end(bytes: &[u8], mut i: usize, hashes: u8) -> Option<usize> {
    while i < bytes.len() {
        if bytes[i] == b'"' {
            let mut h = 0u8;
            while h < hashes && i + 1 + (h as usize) < bytes.len() && bytes[i + 1 + h as usize] == b'#' {
                h += 1;
            }
            if h == hashes {
                return Some(i + 1 + hashes as usize);
            }
        }
        i += 1;
    }
    None
}

fn find_yaml_mapping_colon(slice: &str) -> Option<usize> {
    let bytes = slice.as_bytes();
    let mut in_single = false;
    let mut in_double = false;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\'' && !in_double {
            in_single = !in_single;
        } else if b == b'"' && !in_single {
            in_double = !in_double;
        } else if !in_single && !in_double {
            if b == b'#' {
                return None;
            }
            if b == b':' && bytes.get(i + 1).is_none_or(|nb| nb.is_ascii_whitespace()) {
                return Some(i);
            }
        }
    }
    None
}

fn is_numeric_literal(word: &str) -> bool {
    if word.is_empty() {
        return false;
    }
    let s = word.strip_prefix('-').or_else(|| word.strip_prefix('+')).unwrap_or(word);
    if s.is_empty() {
        return false;
    }
    let mut has_digit = false;
    for b in s.bytes() {
        if b.is_ascii_digit() {
            has_digit = true;
        } else if !matches!(b, b'.' | b'_' | b'e' | b'E' | b'+' | b'-' | b'x' | b'X' | b'a'..=b'f' | b'A'..=b'F') {
            return false;
        }
    }
    has_digit
}

fn compute_brace_fold_ranges(buffer: &LineBuffer, include_brackets: bool) -> Vec<FoldRange> {
    let mut ranges = Vec::new();
    let mut brace_stack = Vec::new();
    let mut bracket_stack = Vec::new();

    for l in 0..buffer.line_count() {
        if let Some(text) = buffer.line(l) {
            let mut in_str: Option<char> = None;
            let mut prev_esc = false;
            for ch in text.chars() {
                if let Some(q) = in_str {
                    if prev_esc {
                        prev_esc = false;
                    } else if ch == '\\' {
                        prev_esc = true;
                    } else if ch == q {
                        in_str = None;
                    }
                    continue;
                }
                match ch {
                    '"' | '\'' => in_str = Some(ch),
                    '{' => brace_stack.push(l),
                    '}' => {
                        if let Some(start_l) = brace_stack.pop() {
                            if l > start_l {
                                ranges.push(FoldRange {
                                    start_line: start_l,
                                    end_line: l,
                                    collapsed: false,
                                });
                            }
                        }
                    }
                    '[' if include_brackets => bracket_stack.push(l),
                    ']' if include_brackets => {
                        if let Some(start_l) = bracket_stack.pop() {
                            if l > start_l {
                                ranges.push(FoldRange {
                                    start_line: start_l,
                                    end_line: l,
                                    collapsed: false,
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    ranges
}

fn compute_cpp_preproc_fold_ranges(buffer: &LineBuffer) -> Vec<FoldRange> {
    let mut ranges = Vec::new();
    let mut stack = Vec::new();
    for l in 0..buffer.line_count() {
        if let Some(text) = buffer.line(l) {
            let trimmed = text.trim_start();
            if trimmed.starts_with("#if") {
                stack.push(l);
            } else if trimmed.starts_with("#endif") {
                if let Some(start_l) = stack.pop() {
                    if l > start_l {
                        ranges.push(FoldRange {
                            start_line: start_l,
                            end_line: l,
                            collapsed: false,
                        });
                    }
                }
            }
        }
    }
    ranges
}

fn compute_indent_fold_ranges(buffer: &LineBuffer) -> Vec<FoldRange> {
    let n = buffer.line_count();
    let mut ranges = Vec::new();
    let mut non_empty: Vec<(usize, usize)> = Vec::new();

    for l in 0..n {
        if let Some(text) = buffer.line(l) {
            let trimmed = text.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let indent = text.bytes().take_while(|&b| b == b' ' || b == b'\t').count();
            non_empty.push((l, indent));
        }
    }

    for idx in 0..non_empty.len() {
        let (start_line, base_indent) = non_empty[idx];
        let mut last_child = start_line;
        for &(cand_line, cand_indent) in &non_empty[idx + 1..] {
            if cand_indent > base_indent {
                last_child = cand_line;
            } else {
                break;
            }
        }
        if last_child > start_line {
            ranges.push(FoldRange {
                start_line,
                end_line: last_child,
                collapsed: false,
            });
        }
    }
    ranges
}

fn compute_toml_fold_ranges(buffer: &LineBuffer) -> Vec<FoldRange> {
    let mut ranges = Vec::new();
    let mut headers = Vec::new();
    let n = buffer.line_count();
    for l in 0..n {
        if let Some(text) = buffer.line(l) {
            let trimmed = text.trim();
            if trimmed.starts_with('[') && trimmed.contains(']') {
                headers.push(l);
            }
        }
    }
    for (idx, &start_l) in headers.iter().enumerate() {
        let next_start = headers.get(idx + 1).copied().unwrap_or(n);
        let mut end_l = next_start.saturating_sub(1);
        while end_l > start_l && buffer.line(end_l).is_some_and(|s| s.trim().is_empty()) {
            end_l -= 1;
        }
        if end_l > start_l {
            ranges.push(FoldRange {
                start_line: start_l,
                end_line: end_l,
                collapsed: false,
            });
        }
    }
    ranges
}

fn compute_xml_fold_ranges(buffer: &LineBuffer) -> Vec<FoldRange> {
    let mut ranges = Vec::new();
    let mut stack: Vec<(String, usize)> = Vec::new();
    for l in 0..buffer.line_count() {
        if let Some(text) = buffer.line(l) {
            let mut i = 0;
            let bytes = text.as_bytes();
            while i < bytes.len() {
                if bytes[i] == b'<' {
                    if bytes[i..].starts_with(b"<!--") {
                        if let Some(rel) = find_bytes(&bytes[i + 4..], b"-->") {
                            i += 4 + rel + 3;
                            continue;
                        } else {
                            break;
                        }
                    }
                    if bytes.get(i + 1) == Some(&b'/') {
                        let name_start = i + 2;
                        let mut name_end = name_start;
                        while name_end < bytes.len()
                            && !bytes[name_end].is_ascii_whitespace()
                            && bytes[name_end] != b'>'
                        {
                            name_end += 1;
                        }
                        let close_name = &text[name_start..name_end];
                        if let Some(pos) = stack.iter().rposition(|(n, _)| n == close_name) {
                            let (_, start_l) = stack.remove(pos);
                            if l > start_l {
                                ranges.push(FoldRange {
                                    start_line: start_l,
                                    end_line: l,
                                    collapsed: false,
                                });
                            }
                        }
                        i = name_end;
                        continue;
                    } else if bytes.get(i + 1).is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_')
                    {
                        let name_start = i + 1;
                        let mut name_end = name_start;
                        while name_end < bytes.len()
                            && (bytes[name_end].is_ascii_alphanumeric()
                                || matches!(bytes[name_end], b'_' | b'-' | b':' | b'.'))
                        {
                            name_end += 1;
                        }
                        let tag_name = text[name_start..name_end].to_string();
                        if let Some(gt_rel) = text[name_end..].find('>') {
                            let gt_abs = name_end + gt_rel;
                            let self_closing = gt_abs > 0 && bytes[gt_abs - 1] == b'/';
                            if !self_closing {
                                stack.push((tag_name, l));
                            }
                            i = gt_abs + 1;
                            continue;
                        } else {
                            stack.push((tag_name, l));
                        }
                    }
                }
                i += 1;
            }
        }
    }
    ranges
}

fn validate_xml_tags(buffer: &LineBuffer, out: &mut Vec<SyntaxDiagnostic>) {
    let mut stack: Vec<(String, usize, usize)> = Vec::new();
    let mut in_comment = false;
    for l in 0..buffer.line_count() {
        let Some(text) = buffer.line(l) else {
            continue;
        };
        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if in_comment {
                if let Some(rel) = find_bytes(&bytes[i..], b"-->") {
                    i += rel + 3;
                    in_comment = false;
                } else {
                    break;
                }
                continue;
            }
            if bytes[i..].starts_with(b"<!--") {
                if let Some(rel) = find_bytes(&bytes[i + 4..], b"-->") {
                    i += 4 + rel + 3;
                } else {
                    in_comment = true;
                    break;
                }
                continue;
            }
            if bytes[i] == b'<' {
                if bytes.get(i + 1) == Some(&b'/') {
                    let ns = i + 2;
                    let mut ne = ns;
                    while ne < bytes.len() && !bytes[ne].is_ascii_whitespace() && bytes[ne] != b'>' {
                        ne += 1;
                    }
                    let close_name = &text[ns..ne];
                    if let Some((open_name, _, _)) = stack.last() {
                        if open_name == close_name {
                            stack.pop();
                        } else {
                            out.push(SyntaxDiagnostic {
                                start: Position::new(l, i),
                                end: Position::new(l, (ne + 1).min(bytes.len())),
                                message: format!(
                                    "Mismatched closing tag `</{close_name}>`, expected `</{open_name}>`"
                                ),
                                is_warning: false,
                            });
                            if let Some(pos) = stack.iter().rposition(|(n, _, _)| n == close_name) {
                                stack.truncate(pos);
                            }
                        }
                    } else {
                        out.push(SyntaxDiagnostic {
                            start: Position::new(l, i),
                            end: Position::new(l, (ne + 1).min(bytes.len())),
                            message: format!("Unexpected closing tag `</{close_name}>`"),
                            is_warning: false,
                        });
                    }
                    i = ne;
                    continue;
                } else if bytes.get(i + 1).is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_') {
                    let ns = i + 1;
                    let mut ne = ns;
                    while ne < bytes.len()
                        && (bytes[ne].is_ascii_alphanumeric()
                            || matches!(bytes[ne], b'_' | b'-' | b':' | b'.'))
                    {
                        ne += 1;
                    }
                    let tag_name = text[ns..ne].to_string();
                    if let Some(gt_rel) = text[ne..].find('>') {
                        let gt_abs = ne + gt_rel;
                        if gt_abs == 0 || bytes[gt_abs - 1] != b'/' {
                            stack.push((tag_name, l, i));
                        }
                        i = gt_abs + 1;
                        continue;
                    }
                }
            }
            i += 1;
        }
    }
    for (name, line, col) in stack {
        out.push(SyntaxDiagnostic {
            start: Position::new(line, col),
            end: Position::new(line, col + name.len() + 1),
            message: format!("Unclosed XML/SDF tag `<{name}>`"),
            is_warning: false,
        });
    }
}

fn compute_markdown_fold_ranges(buffer: &LineBuffer) -> Vec<FoldRange> {
    let n = buffer.line_count();
    let mut ranges = Vec::new();
    let mut heading_stack: Vec<(usize, usize)> = Vec::new(); // (level, line)
    let mut fence_open: Option<(u8, usize, usize)> = None; // (char, len, start_line)

    for l in 0..n {
        let Some(text) = buffer.line(l) else {
            continue;
        };
        let trimmed = text.trim_start();
        if let Some((fc, fl, start_l)) = fence_open {
            let run = trimmed.bytes().take_while(|&b| b == fc).count();
            if run >= fl && trimmed[run..].trim().is_empty() {
                if l > start_l {
                    ranges.push(FoldRange {
                        start_line: start_l,
                        end_line: l,
                        collapsed: false,
                    });
                }
                fence_open = None;
            }
            continue;
        }
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let fc = trimmed.as_bytes()[0];
            let fl = trimmed.bytes().take_while(|&b| b == fc).count();
            fence_open = Some((fc, fl, l));
            continue;
        }
        if trimmed.starts_with('#') {
            let lvl = trimmed.bytes().take_while(|&b| b == b'#').count();
            if (1..=6).contains(&lvl) && trimmed.as_bytes().get(lvl).is_none_or(|&b| b == b' ') {
                while let Some(&(prev_lvl, start_l)) = heading_stack.last() {
                    if prev_lvl >= lvl {
                        heading_stack.pop();
                        let end_l = l.saturating_sub(1);
                        if end_l > start_l {
                            ranges.push(FoldRange {
                                start_line: start_l,
                                end_line: end_l,
                                collapsed: false,
                            });
                        }
                    } else {
                        break;
                    }
                }
                heading_stack.push((lvl, l));
            }
        }
    }
    for (_, start_l) in heading_stack {
        let end_l = n.saturating_sub(1);
        if end_l > start_l {
            ranges.push(FoldRange {
                start_line: start_l,
                end_line: end_l,
                collapsed: false,
            });
        }
    }
    ranges
}

fn compute_shell_fold_ranges(buffer: &LineBuffer) -> Vec<FoldRange> {
    let mut ranges = compute_brace_fold_ranges(buffer, false);
    let mut if_stack = Vec::new();
    let mut loop_stack = Vec::new();
    let mut case_stack = Vec::new();

    for l in 0..buffer.line_count() {
        let Some(text) = buffer.line(l) else {
            continue;
        };
        let trimmed = text.trim();
        let first = trimmed.split_whitespace().next().unwrap_or("");
        match first {
            "if" => if_stack.push(l),
            "fi" => {
                if let Some(s) = if_stack.pop() {
                    if l > s {
                        ranges.push(FoldRange {
                            start_line: s,
                            end_line: l,
                            collapsed: false,
                        });
                    }
                }
            }
            "for" | "while" | "until" => loop_stack.push(l),
            "done" => {
                if let Some(s) = loop_stack.pop() {
                    if l > s {
                        ranges.push(FoldRange {
                            start_line: s,
                            end_line: l,
                            collapsed: false,
                        });
                    }
                }
            }
            "case" => case_stack.push(l),
            "esac" => {
                if let Some(s) = case_stack.pop() {
                    if l > s {
                        ranges.push(FoldRange {
                            start_line: s,
                            end_line: l,
                            collapsed: false,
                        });
                    }
                }
            }
            _ => {}
        }
    }
    ranges
}
