//! Source-Mapped RSX AST Parser with Line/Column Coordinate Resolution.

use crate::types::{Diagnostic, DiagnosticSeverity, Position, Range};

/// A byte-offset span within the document source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Converts this byte span into an LSP Range using the provided SourceMap.
    pub fn to_range(&self, map: &SourceMap) -> Range {
        Range::new(map.offset_to_position(self.start), map.offset_to_position(self.end))
    }
}

/// Source coordinate map indexing newline offsets for O(log N) coordinate lookups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceMap {
    line_starts: Vec<usize>,
    total_len: usize,
}

impl SourceMap {
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        for (i, b) in source.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Self {
            line_starts,
            total_len: source.len(),
        }
    }

    /// Converts a 0-indexed byte offset into an LSP (line, character) position.
    pub fn offset_to_position(&self, offset: usize) -> Position {
        let clamped = offset.min(self.total_len);
        let line_idx = match self.line_starts.binary_search(&clamped) {
            Ok(exact) => exact,
            Err(insertion) => insertion.saturating_sub(1),
        };

        let start_of_line = self.line_starts[line_idx];
        let char_offset = clamped.saturating_sub(start_of_line);

        Position::new(line_idx as u32, char_offset as u32)
    }

    /// Converts an LSP (line, character) position into a 0-indexed byte offset.
    pub fn position_to_offset(&self, pos: Position) -> usize {
        let line_idx = (pos.line as usize).min(self.line_starts.len().saturating_sub(1));
        let line_start = self.line_starts[line_idx];
        (line_start + pos.character as usize).min(self.total_len)
    }
}

/// An attribute on an RSX element (e.g. `class="flex p-4"`, `onclick={...}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RsxAttribute {
    pub name: String,
    pub name_span: Span,
    pub value: Option<String>,
    pub value_span: Option<Span>,
    pub is_expression: bool,
}

/// A node within the RSX syntax tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RsxNode {
    Element {
        name: String,
        name_span: Span,
        is_self_closing: bool,
        open_span: Span,
        close_span: Option<Span>,
        attributes: Vec<RsxAttribute>,
        children: Vec<RsxNode>,
    },
    Text {
        content: String,
        span: Span,
    },
    Block {
        expression: String,
        span: Span,
    },
}

/// A parse error detected during AST scanning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
    pub severity: DiagnosticSeverity,
}

impl ParseError {
    pub fn to_diagnostic(&self, map: &SourceMap) -> Diagnostic {
        Diagnostic {
            range: self.span.to_range(map),
            severity: self.severity,
            code: None,
            source: Some("strata-lsp".to_string()),
            message: self.message.clone(),
        }
    }
}

/// High-level parsed RSX document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RsxDocument {
    pub source: String,
    pub source_map: SourceMap,
    pub nodes: Vec<RsxNode>,
    pub errors: Vec<ParseError>,
}

impl RsxDocument {
    /// Parses an RSX text string into a document representation.
    pub fn parse(source: &str) -> Self {
        let source_map = SourceMap::new(source);
        let mut parser = ParserState::new(source);
        let nodes = parser.parse_nodes();

        Self {
            source: source.to_string(),
            source_map,
            nodes,
            errors: parser.errors,
        }
    }
}

// ---------------------------------------------------------------------------
// Recursive-Descent Lexer & Parser Implementation
// ---------------------------------------------------------------------------

struct ParserState<'a> {
    source: &'a str,
    bytes: &'a [u8],
    cursor: usize,
    errors: Vec<ParseError>,
}

impl<'a> ParserState<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            bytes: source.as_bytes(),
            cursor: 0,
            errors: Vec::new(),
        }
    }

    fn is_eof(&self) -> bool {
        self.cursor >= self.bytes.len()
    }

    fn skip_whitespace(&mut self) {
        while !self.is_eof() && self.bytes[self.cursor].is_ascii_whitespace() {
            self.cursor += 1;
        }
    }

    fn parse_nodes(&mut self) -> Vec<RsxNode> {
        let mut nodes = Vec::new();
        while !self.is_eof() {
            self.skip_whitespace();
            if self.is_eof() {
                break;
            }

            if self.bytes[self.cursor] == b'<' {
                if self.cursor + 1 < self.bytes.len() && self.bytes[self.cursor + 1] == b'/' {
                    // Stray closing tag
                    break;
                }
                if let Some(elem) = self.parse_element() {
                    nodes.push(elem);
                } else {
                    self.cursor += 1;
                }
            } else if self.bytes[self.cursor] == b'{' {
                if let Some(block) = self.parse_block() {
                    nodes.push(block);
                } else {
                    self.cursor += 1;
                }
            } else {
                let text = self.parse_text();
                if !text.content.trim().is_empty() {
                    nodes.push(RsxNode::Text {
                        content: text.content,
                        span: text.span,
                    });
                }
            }
        }
        nodes
    }

    fn parse_element(&mut self) -> Option<RsxNode> {
        let open_start = self.cursor;
        self.cursor += 1; // skip '<'
        self.skip_whitespace();

        // Tag Name
        let name_start = self.cursor;
        while !self.is_eof()
            && (self.bytes[self.cursor].is_ascii_alphanumeric()
                || self.bytes[self.cursor] == b'_'
                || self.bytes[self.cursor] == b'-'
                || self.bytes[self.cursor] == b':')
        {
            self.cursor += 1;
        }
        let name_end = self.cursor;
        if name_start == name_end {
            self.errors.push(ParseError {
                message: "Expected tag name after '<'".to_string(),
                span: Span::new(open_start, self.cursor),
                severity: DiagnosticSeverity::Error,
            });
            return None;
        }
        let tag_name = self.source[name_start..name_end].to_string();
        let name_span = Span::new(name_start, name_end);

        // Attributes
        let mut attributes = Vec::new();
        let mut is_self_closing = false;

        loop {
            self.skip_whitespace();
            if self.is_eof() {
                self.errors.push(ParseError {
                    message: format!("Unclosed tag '<{}>'", tag_name),
                    span: Span::new(open_start, self.cursor),
                    severity: DiagnosticSeverity::Error,
                });
                return None;
            }

            if self.bytes[self.cursor] == b'/' && self.cursor + 1 < self.bytes.len() && self.bytes[self.cursor + 1] == b'>' {
                is_self_closing = true;
                self.cursor += 2;
                break;
            } else if self.bytes[self.cursor] == b'>' {
                self.cursor += 1;
                break;
            } else {
                if let Some(attr) = self.parse_attribute() {
                    attributes.push(attr);
                } else {
                    self.cursor += 1;
                }
            }
        }

        let open_span = Span::new(open_start, self.cursor);

        // Void elements in HTML that don't need closing
        let is_void_element = matches!(
            tag_name.to_ascii_lowercase().as_str(),
            "area" | "base" | "br" | "col" | "embed" | "hr" | "img" | "input" | "link" | "meta" | "param" | "source" | "track" | "wbr"
        );

        if is_self_closing || is_void_element {
            return Some(RsxNode::Element {
                name: tag_name,
                name_span,
                is_self_closing: true,
                open_span,
                close_span: None,
                attributes,
                children: Vec::new(),
            });
        }

        // Parse Children
        let children = self.parse_nodes();

        // Closing Tag
        self.skip_whitespace();
        let mut close_span = None;

        if !self.is_eof() && self.bytes[self.cursor] == b'<' && self.cursor + 1 < self.bytes.len() && self.bytes[self.cursor + 1] == b'/' {
            let close_start = self.cursor;
            self.cursor += 2; // skip '</'
            self.skip_whitespace();

            let cname_start = self.cursor;
            while !self.is_eof()
                && (self.bytes[self.cursor].is_ascii_alphanumeric()
                    || self.bytes[self.cursor] == b'_'
                    || self.bytes[self.cursor] == b'-'
                    || self.bytes[self.cursor] == b':')
            {
                self.cursor += 1;
            }
            let cname = &self.source[cname_start..self.cursor];

            self.skip_whitespace();
            if !self.is_eof() && self.bytes[self.cursor] == b'>' {
                self.cursor += 1;
            }

            let close_end = self.cursor;
            close_span = Some(Span::new(close_start, close_end));

            if cname != tag_name {
                self.errors.push(ParseError {
                    message: format!("Mismatched closing tag: expected '</{}>', found '</{}>'", tag_name, cname),
                    span: Span::new(close_start, close_end),
                    severity: DiagnosticSeverity::Error,
                });
            }
        } else {
            self.errors.push(ParseError {
                message: format!("Unclosed tag '<{}>': missing closing '</{}>'", tag_name, tag_name),
                span: open_span,
                severity: DiagnosticSeverity::Error,
            });
        }

        Some(RsxNode::Element {
            name: tag_name,
            name_span,
            is_self_closing: false,
            open_span,
            close_span,
            attributes,
            children,
        })
    }

    fn parse_attribute(&mut self) -> Option<RsxAttribute> {
        let name_start = self.cursor;
        while !self.is_eof()
            && (self.bytes[self.cursor].is_ascii_alphanumeric()
                || self.bytes[self.cursor] == b'_'
                || self.bytes[self.cursor] == b'-'
                || self.bytes[self.cursor] == b':')
        {
            self.cursor += 1;
        }
        let name_end = self.cursor;
        if name_start == name_end {
            return None;
        }
        let name = self.source[name_start..name_end].to_string();
        let name_span = Span::new(name_start, name_end);

        self.skip_whitespace();
        if self.is_eof() || self.bytes[self.cursor] != b'=' {
            // Boolean attribute without value
            return Some(RsxAttribute {
                name,
                name_span,
                value: None,
                value_span: None,
                is_expression: false,
            });
        }

        self.cursor += 1; // skip '='
        self.skip_whitespace();

        if self.is_eof() {
            return None;
        }

        if self.bytes[self.cursor] == b'"' || self.bytes[self.cursor] == b'\'' {
            let quote = self.bytes[self.cursor];
            self.cursor += 1;
            let val_start = self.cursor;
            while !self.is_eof() && self.bytes[self.cursor] != quote {
                self.cursor += 1;
            }
            let val_end = self.cursor;
            if !self.is_eof() {
                self.cursor += 1; // skip closing quote
            }
            let value = self.source[val_start..val_end].to_string();
            let value_span = Some(Span::new(val_start, val_end));

            Some(RsxAttribute {
                name,
                name_span,
                value: Some(value),
                value_span,
                is_expression: false,
            })
        } else if self.bytes[self.cursor] == b'{' {
            self.cursor += 1;
            let val_start = self.cursor;
            let mut depth = 1;
            while !self.is_eof() && depth > 0 {
                if self.bytes[self.cursor] == b'{' {
                    depth += 1;
                } else if self.bytes[self.cursor] == b'}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                self.cursor += 1;
            }
            let val_end = self.cursor;
            if !self.is_eof() {
                self.cursor += 1; // skip '}'
            }
            let value = self.source[val_start..val_end].to_string();
            let value_span = Some(Span::new(val_start, val_end));

            Some(RsxAttribute {
                name,
                name_span,
                value: Some(value),
                value_span,
                is_expression: true,
            })
        } else {
            None
        }
    }

    fn parse_block(&mut self) -> Option<RsxNode> {
        let start = self.cursor;
        self.cursor += 1; // skip '{'
        let mut depth = 1;
        let expr_start = self.cursor;

        while !self.is_eof() && depth > 0 {
            if self.bytes[self.cursor] == b'{' {
                depth += 1;
            } else if self.bytes[self.cursor] == b'}' {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            self.cursor += 1;
        }

        let expr_end = self.cursor;
        if !self.is_eof() {
            self.cursor += 1; // skip '}'
        }
        let end = self.cursor;

        let expression = self.source[expr_start..expr_end].to_string();
        Some(RsxNode::Block {
            expression,
            span: Span::new(start, end),
        })
    }

    fn parse_text(&mut self) -> ParsedText {
        let start = self.cursor;
        while !self.is_eof() && self.bytes[self.cursor] != b'<' && self.bytes[self.cursor] != b'{' {
            self.cursor += 1;
        }
        let end = self.cursor;
        ParsedText {
            content: self.source[start..end].to_string(),
            span: Span::new(start, end),
        }
    }
}

struct ParsedText {
    content: String,
    span: Span,
}
