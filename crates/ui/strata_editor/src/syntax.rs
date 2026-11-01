//! Real-time incremental syntax tokenization, bracket pair matching, and code folding.

use crate::buffer::LineBuffer;
use crate::cursor::Position;

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

/// Syntax analysis engine providing real-time tokenization and structural navigation.
#[derive(Debug, Clone, Default)]
pub struct SyntaxHighlighter;

impl SyntaxHighlighter {
    pub fn new() -> Self {
        Self
    }

    /// Tokenizes a single line for high-speed incremental viewport rendering.
    pub fn highlight_line(&self, line: &str, lang: Language) -> Vec<SyntaxToken> {
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
                word.chars().next().map_or(false, |c| c.is_uppercase())
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
            ) || word.chars().next().map_or(false, |c| c.is_uppercase()),
            _ => false,
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

    /// Scans buffer to locate multi-line code folding regions (based on `{` and `}`).
    pub fn compute_fold_ranges(&self, buffer: &LineBuffer) -> Vec<FoldRange> {
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

        if trimmed.ends_with('{') || trimmed.ends_with(':') || trimmed.ends_with('(') || trimmed.ends_with('[') {
            current_indent + 4
        } else {
            current_indent
        }
    }
}
