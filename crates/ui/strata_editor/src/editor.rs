//! Headless Editor State Machine and Text Mutation Pipeline.

use strata_dom::Signal;
use crate::markdown::{from_markdown, to_markdown};
use crate::model::{BlockKind, BlockNode, Document, InlineSpan};

/// Headless editor state controller managing document AST and active selection cursor.
#[derive(Clone)]
pub struct EditorState {
    pub document: Signal<Document>,
    pub cursor: Signal<Option<(usize, usize)>>,
}

impl Default for EditorState {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorState {
    /// Creates a new editor with an empty paragraph block.
    pub fn new() -> Self {
        let mut doc = Document::new();
        doc.add_block(BlockNode::paragraph("block-1", ""));
        Self {
            document: Signal::new(doc),
            cursor: Signal::new(Some((0, 0))),
        }
    }

    /// Read the serialized markdown content of the editor.
    pub fn to_markdown(&self) -> String {
        to_markdown(&self.document.get())
    }

    /// Load document from a markdown string.
    pub fn load_markdown(&self, md: &str) {
        let doc = from_markdown(md);
        self.document.set(doc);
        self.cursor.set(Some((0, 0)));
    }

    /// Appends text to the currently active block.
    pub fn insert_text(&self, text: &str) {
        let mut doc = self.document.get();
        if let Some((b_idx, _)) = self.cursor.get() {
            if let Some(block) = doc.blocks.get_mut(b_idx) {
                if let Some(span) = block.spans.last_mut() {
                    span.text.push_str(text);
                } else {
                    block.spans.push(InlineSpan::plain(text));
                }
            }
        }
        self.document.set(doc);
    }

    /// Inserts a new block below the current block.
    pub fn insert_block(&self, kind: BlockKind) {
        let mut doc = self.document.get();
        let next_id = format!("block-{}", doc.blocks.len() + 1);
        let new_block = BlockNode::new(next_id, kind);

        let insert_idx = if let Some((b_idx, _)) = self.cursor.get() {
            (b_idx + 1).min(doc.blocks.len())
        } else {
            doc.blocks.len()
        };

        doc.blocks.insert(insert_idx, new_block);
        self.document.set(doc);
        self.cursor.set(Some((insert_idx, 0)));
    }
}

/// Convenience hook to create an editor state instance.
pub fn use_editor(initial_markdown: Option<&str>) -> EditorState {
    let editor = EditorState::new();
    if let Some(md) = initial_markdown {
        editor.load_markdown(md);
    }
    editor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_editor_operations() {
        let editor = use_editor(Some("# Initial Header\n\nInitial paragraph"));
        assert_eq!(editor.document.get().blocks.len(), 2);

        editor.insert_block(BlockKind::Paragraph);
        editor.insert_text("Added line via editor");

        let md = editor.to_markdown();
        assert!(md.contains("Added line via editor"));
    }
}
