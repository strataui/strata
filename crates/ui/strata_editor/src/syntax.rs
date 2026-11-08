//! Real-time incremental syntax tokenization, bracket pair matching, and code folding.

use crate::buffer::LineBuffer;
use crate::cursor::Position;

use tree_sitter::{InputEdit, Node, Parser, Tree};

/// Supported language syntaxes for the code editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Language {
    #[default]
    Rust,
    TypeScript,
    JavaScript,
    Python,
    Json,
    Markdown,
    Html,
    PlainText,
}

/// Token classification for semantic syntax highlighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenType {
    Keyword,
    Type,
    Function,
    Identifier,
    String,
    Number,
    Comment,
    Operator,
    Punctuation,
    Tag,
    Attribute,
}

/// A highlighted syntax token with column span offsets.
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

fn create_parser_for_lang(lang: Language) -> Option<Parser> {
    let mut parser = Parser::new();
    match lang {
        Language::Rust => {
            parser.set_language(&tree_sitter_rust::LANGUAGE.into()).ok()?;
            Some(parser)
        }
        Language::JavaScript | Language::TypeScript => {
            parser.set_language(&tree_sitter_javascript::LANGUAGE.into()).ok()?;
            Some(parser)
        }
        _ => None,
    }
}

fn classify_tree_sitter_kind(kind: &str, parent_kind: &str) -> TokenType {
    match kind {
        "line_comment" | "block_comment" | "comment" => TokenType::Comment,

        "string_literal" | "raw_string_literal" | "char_literal" | "string"
        | "template_string" | "string_content" | "escape_sequence" => TokenType::String,

        "integer_literal" | "float_literal" | "number" => TokenType::Number,

        "primitive_type" | "type_identifier" => TokenType::Type,

        "identifier"
            if parent_kind == "call_expression"
                || parent_kind == "function_item"
                || parent_kind == "function_declaration"
                || parent_kind == "function_declarator"
                || parent_kind == "method_definition" =>
        {
            TokenType::Function
        }

        "fn" | "let" | "mut" | "pub" | "struct" | "enum" | "impl" | "trait" | "match"
        | "if" | "else" | "return" | "for" | "in" | "while" | "loop" | "async" | "await"
        | "use" | "mod" | "crate" | "const" | "static" | "type" | "where" | "as" | "ref"
        | "move" | "unsafe" | "dyn" | "super" | "self" | "Self"
        | "function" | "var" | "import" | "export" | "default" | "from" | "class"
        | "interface" | "extends" | "new" | "this" | "try" | "catch" | "finally"
        | "switch" | "case" | "break" | "continue" | "throw" | "typeof" | "instanceof"
        | "yield" | "debugger" => TokenType::Keyword,

        "+" | "-" | "*" | "/" | "%" | "=" | "==" | "!=" | "<" | ">" | "<=" | ">="
        | "&&" | "||" | "!" | "&" | "|" | "^" | "<<" | ">>" | "+=" | "-=" | "*="
        | "/=" | "%=" | "&=" | "|=" | "^=" | "=>" | "->" => TokenType::Operator,

        "(" | ")" | "[" | "]" | "{" | "}" | ";" | ":" | "::" | "," | "." => TokenType::Punctuation,

        "tag_name" | "jsx_opening_element" | "jsx_closing_element" => TokenType::Tag,
        "property_identifier" | "field_identifier" | "shorthand_property_identifier" => {
            TokenType::Attribute
        }

        _ => TokenType::Identifier,
    }
}

fn collect_leaf_tokens(node: Node, line_idx: usize, out: &mut Vec<SyntaxToken>, line_text: &str) {
    if node.start_position().row > line_idx || node.end_position().row < line_idx {
        return;
    }

    if node.child_count() == 0 {
        let kind = node.kind();
        let start_pt = node.start_position();
        let end_pt = node.end_position();

        let start_col = if start_pt.row == line_idx { start_pt.column } else { 0 };
        let end_col = if end_pt.row == line_idx { end_pt.column } else { line_text.len() };

        if start_col < end_col && start_col < line_text.len() {
            let clamped_end = end_col.min(line_text.len());
            let text = line_text[start_col..clamped_end].to_string();
            if !text.trim().is_empty() {
                let parent_kind = node.parent().map(|p| p.kind()).unwrap_or("");
                let token_type = classify_tree_sitter_kind(kind, parent_kind);
                out.push(SyntaxToken {
                    token_type,
                    start_col,
                    end_col: clamped_end,
                    text,
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
                | "function_declaration"
                | "class_declaration"
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

/// Syntax analysis engine providing real-time Tree-Sitter AST tokenization, incremental parsing, and structural navigation.
pub struct SyntaxHighlighter {
    parser: Option<Parser>,
    tree: Option<Tree>,
    language: Language,
}

impl std::fmt::Debug for SyntaxHighlighter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyntaxHighlighter")
            .field("has_parser", &self.parser.is_some())
            .field("has_tree", &self.tree.is_some())
            .field("language", &self.language)
            .finish()
    }
}

impl Clone for SyntaxHighlighter {
    fn clone(&self) -> Self {
        let mut cloned = Self::with_language(self.language);
        cloned.tree = self.tree.clone();
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
            tree: None,
            language: lang,
        }
    }

    pub fn set_language(&mut self, lang: Language) {
        if self.language != lang {
            self.language = lang;
            self.parser = create_parser_for_lang(lang);
            self.tree = None;
        }
    }

    pub fn language(&self) -> Language {
        self.language
    }

    /// Accesses the active Tree-Sitter AST tree, if parsed.
    pub fn tree(&self) -> Option<&Tree> {
        self.tree.as_ref()
    }

    /// Parses a complete source text into a Tree-Sitter AST.
    pub fn parse(&mut self, source: &str) {
        if let Some(ref mut parser) = self.parser {
            self.tree = parser.parse(source, None);
        }
    }

    /// Informs the Tree-Sitter AST of an edit for subsequent incremental parsing.
    pub fn edit(&mut self, edit: &InputEdit) {
        if let Some(ref mut tree) = self.tree {
            tree.edit(edit);
        }
    }

    /// Re-parses the document incrementally using the edited previous AST.
    pub fn parse_incremental(&mut self, source: &str) {
        if let Some(ref mut parser) = self.parser {
            self.tree = parser.parse(source, self.tree.as_ref());
        }
    }

    /// Tokenizes a single line for high-speed incremental viewport rendering.
    pub fn highlight_line(&self, line: &str, lang: Language) -> Vec<SyntaxToken> {
        if line.is_empty() {
            return Vec::new();
        }

        Self::tokenize_line_lexical(line, lang)
    }

    /// Extracts syntax tokens for a line directly from the active Tree-Sitter AST.
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

        Self::tokenize_line_lexical(line, self.language)
    }

    /// Fast lexical tokenizer fallback for languages without a Tree-Sitter grammar.
    pub fn tokenize_line_lexical(line: &str, lang: Language) -> Vec<SyntaxToken> {
        if line.is_empty() {
            return Vec::new();
        }

        let mut tokens = Vec::new();
        let chars: Vec<char> = line.chars().collect();
        let mut idx = 0;

        while idx < chars.len() {
            let ch = chars[idx];

            // 1. Whitespace
            if ch.is_whitespace() {
                idx += 1;
                continue;
            }

            // 2. Line comment (// or #)
            if (ch == '/' && idx + 1 < chars.len() && chars[idx + 1] == '/')
                || (ch == '#' && (lang == Language::Python || lang == Language::Markdown))
            {
                let start = idx;
                let text: String = chars[start..].iter().collect();
                tokens.push(SyntaxToken {
                    token_type: TokenType::Comment,
                    start_col: start,
                    end_col: chars.len(),
                    text,
                });
                break;
            }

            // 3. String literals ("..." or '...')
            if ch == '"' || ch == '\'' || ch == '`' {
                let quote = ch;
                let start = idx;
                idx += 1;
                while idx < chars.len() {
                    if chars[idx] == '\\' && idx + 1 < chars.len() {
                        idx += 2;
                    } else if chars[idx] == quote {
                        idx += 1;
                        break;
                    } else {
                        idx += 1;
                    }
                }
                let text: String = chars[start..idx].iter().collect();
                tokens.push(SyntaxToken {
                    token_type: TokenType::String,
                    start_col: start,
                    end_col: idx,
                    text,
                });
                continue;
            }

            // 4. Numeric literals
            if ch.is_ascii_digit() {
                let start = idx;
                while idx < chars.len() && (chars[idx].is_ascii_digit() || chars[idx] == '.' || chars[idx] == '_') {
                    idx += 1;
                }
                let text: String = chars[start..idx].iter().collect();
                tokens.push(SyntaxToken {
                    token_type: TokenType::Number,
                    start_col: start,
                    end_col: idx,
                    text,
                });
                continue;
            }

            // 5. Identifiers and Keywords
            if ch.is_alphabetic() || ch == '_' {
                let start = idx;
                while idx < chars.len() && (chars[idx].is_alphanumeric() || chars[idx] == '_') {
                    idx += 1;
                }
                let text: String = chars[start..idx].iter().collect();

                let token_type = if Self::is_keyword(&text, lang) {
                    TokenType::Keyword
                } else if Self::is_type(&text, lang) {
                    TokenType::Type
                } else if idx < chars.len() && chars[idx] == '(' {
                    TokenType::Function
                } else {
                    TokenType::Identifier
                };

                tokens.push(SyntaxToken {
                    token_type,
                    start_col: start,
                    end_col: idx,
                    text,
                });
                continue;
            }

            // 6. Operators and Punctuation
            let start = idx;
            idx += 1;
            let token_type = match ch {
                '+' | '-' | '*' | '/' | '%' | '=' | '<' | '>' | '!' | '&' | '|' | '^' => {
                    TokenType::Operator
                }
                _ => TokenType::Punctuation,
            };

            tokens.push(SyntaxToken {
                token_type,
                start_col: start,
                end_col: idx,
                text: ch.to_string(),
            });
        }

        tokens
    }

    fn is_keyword(word: &str, lang: Language) -> bool {
        match lang {
            Language::Rust => matches!(
                word,
                "fn" | "let" | "mut" | "pub" | "struct" | "enum" | "impl" | "trait" | "match"
                    | "if" | "else" | "return" | "for" | "in" | "while" | "loop" | "async"
                    | "await" | "use" | "mod" | "crate" | "const" | "static" | "type"
            ),
            Language::TypeScript | Language::JavaScript => matches!(
                word,
                "function" | "const" | "let" | "var" | "return" | "if" | "else" | "import"
                    | "export" | "default" | "from" | "class" | "interface" | "type" | "new"
                    | "this" | "async" | "await" | "try" | "catch" | "switch" | "case"
            ),
            Language::Python => matches!(
                word,
                "def" | "class" | "return" | "if" | "elif" | "else" | "for" | "while"
                    | "import" | "from" | "as" | "try" | "except" | "finally" | "with"
                    | "lambda" | "pass" | "None" | "True" | "False"
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
                        "u8" | "u16" | "u32" | "u64" | "u128" | "usize"
                            | "i8" | "i16" | "i32" | "i64" | "i128" | "isize"
                            | "f32" | "f64" | "bool" | "char" | "str" | "String" | "Vec" | "Option" | "Result"
                    )
            }
            Language::TypeScript => matches!(
                word,
                "string" | "number" | "boolean" | "any" | "void" | "never" | "unknown" | "Record" | "Array"
            ) || word.chars().next().is_some_and(|c| c.is_uppercase()),
            _ => false,
        }
    }

    /// Returns the root node of the active Tree-Sitter syntax tree, if available.
    pub fn root_node(&self) -> Option<Node<'_>> {
        self.tree.as_ref().map(|t| t.root_node())
    }

    /// Finds the finest AST node at the given 0-indexed row and column.
    pub fn node_at_position(&self, line: usize, col: usize) -> Option<Node<'_>> {
        let tree = self.tree.as_ref()?;
        let pt = tree_sitter::Point {
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
            ')' => ('(', false),
            ']' => ('[', false),
            '}' => ('{', false),
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
                    let start_c = if l == pos.line { pos.col } else { chars.len().saturating_sub(1) };
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

    /// Scans buffer or Tree-Sitter AST to locate multi-line code folding regions.
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

        let mut ranges = Vec::new();
        let mut stack = Vec::new();

        for l in 0..buffer.line_count() {
            if let Some(text) = buffer.line(l) {
                for ch in text.chars() {
                    if ch == '{' {
                        stack.push(l);
                    } else if ch == '}' {
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
        }

        ranges
    }

    /// Computes indentation column spaces for a newly created newline following line `line_idx`.
    pub fn compute_auto_indent(&self, buffer: &LineBuffer, line_idx: usize) -> usize {
        let prev_line = match buffer.line(line_idx) {
            Some(l) => l,
            None => return 0,
        };

        let current_indent = prev_line.chars().take_while(|c| *c == ' ').count();
        let trimmed = prev_line.trim_end();

        if let Some(ref tree) = self.tree {
            let pt = tree_sitter::Point {
                row: line_idx,
                column: prev_line.len().saturating_sub(1),
            };
            if let Some(node) = tree.root_node().descendant_for_point_range(pt, pt) {
                let kind = node.kind();
                if kind == "{" || kind == ":" || kind == "(" || kind == "[" {
                    return current_indent + 4;
                }
            }
        }

        if trimmed.ends_with('{') || trimmed.ends_with(':') || trimmed.ends_with('(') || trimmed.ends_with('[') {
            current_indent + 4
        } else {
            current_indent
        }
    }
}
