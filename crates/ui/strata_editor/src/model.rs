//! Headless Block Document Tree and Inline Mark AST.

use serde::{Deserialize, Serialize};

/// Text formatting marks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InlineMark {
    Bold,
    Italic,
    Code,
    Underline,
    Strike,
    Link(String),
}

/// A formatted span of text within a block node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InlineSpan {
    pub text: String,
    pub marks: Vec<InlineMark>,
}

impl InlineSpan {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            marks: Vec::new(),
        }
    }

    pub fn with_mark(mut self, mark: InlineMark) -> Self {
        self.marks.push(mark);
        self
    }
}

/// Structural block kinds supported by the document editor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockKind {
    Paragraph,
    Heading(u8),
    Blockquote,
    CodeBlock(Option<String>),
    BulletListItem,
    NumberedListItem(usize),
    HorizontalRule,
}

/// A single block node containing child inline spans.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockNode {
    pub id: String,
    pub kind: BlockKind,
    pub spans: Vec<InlineSpan>,
}

impl BlockNode {
    pub fn new(id: impl Into<String>, kind: BlockKind) -> Self {
        Self {
            id: id.into(),
            kind,
            spans: Vec::new(),
        }
    }

    pub fn paragraph(id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind: BlockKind::Paragraph,
            spans: vec![InlineSpan::plain(text)],
        }
    }

    pub fn heading(id: impl Into<String>, level: u8, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind: BlockKind::Heading(level),
            spans: vec![InlineSpan::plain(text)],
        }
    }

    pub fn plain_text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }
}

/// The root headless document containing top-level blocks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Document {
    pub blocks: Vec<BlockNode>,
}

impl Document {
    pub fn new() -> Self {
        Self { blocks: Vec::new() }
    }

    pub fn add_block(&mut self, block: BlockNode) {
        self.blocks.push(block);
    }

    pub fn push(&mut self, block: BlockNode) {
        self.blocks.push(block);
    }
}
