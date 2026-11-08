//! Bi-Directional Markdown Parser and Serializer.

use crate::model::{BlockKind, BlockNode, Document, InlineMark, InlineSpan};

/// Serializes a Document AST to clean GitHub Flavored Markdown.
pub fn to_markdown(doc: &Document) -> String {
    let mut out = String::new();

    for block in &doc.blocks {
        let text = serialize_spans(&block.spans);
        match &block.kind {
            BlockKind::Heading(lvl) => {
                let hashes = "#".repeat((*lvl as usize).clamp(1, 6));
                out.push_str(&format!("{} {}\n\n", hashes, text));
            }
            BlockKind::Paragraph => {
                out.push_str(&format!("{}\n\n", text));
            }
            BlockKind::Blockquote => {
                out.push_str(&format!("> {}\n\n", text));
            }
            BlockKind::BulletListItem => {
                out.push_str(&format!("- {}\n", text));
            }
            BlockKind::NumberedListItem(num) => {
                out.push_str(&format!("{}. {}\n", num, text));
            }
            BlockKind::CodeBlock(lang) => {
                let tag = lang.as_deref().unwrap_or("");
                out.push_str(&format!("```{}\n{}\n```\n\n", tag, text));
            }
            BlockKind::HorizontalRule => {
                out.push_str("---\n\n");
            }
        }
    }

    out.trim_end().to_string()
}

fn serialize_spans(spans: &[InlineSpan]) -> String {
    let mut res = String::new();
    for span in spans {
        let mut text = span.text.clone();
        for mark in &span.marks {
            match mark {
                InlineMark::Bold => text = format!("**{}**", text),
                InlineMark::Italic => text = format!("*{}*", text),
                InlineMark::Code => text = format!("`{}`", text),
                InlineMark::Link(href) => text = format!("[{}]({})", text, href),
                _ => {}
            }
        }
        res.push_str(&text);
    }
    res
}

/// Parses standard Markdown lines into a Document AST.
pub fn from_markdown(md: &str) -> Document {
    let mut doc = Document::new();
    let mut block_idx = 0;

    for line in md.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        block_idx += 1;
        let id = format!("block-{}", block_idx);

        if let Some(stripped) = trimmed.strip_prefix("# ") {
            doc.add_block(BlockNode::heading(id, 1, stripped));
        } else if let Some(stripped) = trimmed.strip_prefix("## ") {
            doc.add_block(BlockNode::heading(id, 2, stripped));
        } else if let Some(stripped) = trimmed.strip_prefix("### ") {
            doc.add_block(BlockNode::heading(id, 3, stripped));
        } else if let Some(stripped) = trimmed.strip_prefix("> ") {
            let mut b = BlockNode::new(id, BlockKind::Blockquote);
            b.spans.push(InlineSpan::plain(stripped));
            doc.add_block(b);
        } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            let mut b = BlockNode::new(id, BlockKind::BulletListItem);
            b.spans.push(InlineSpan::plain(&trimmed[2..]));
            doc.add_block(b);
        } else if trimmed == "---" {
            doc.add_block(BlockNode::new(id, BlockKind::HorizontalRule));
        } else {
            doc.add_block(BlockNode::paragraph(id, trimmed));
        }
    }

    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_roundtrip() {
        let md = "# Title\n\nIntro paragraph.\n\n> Important quote\n\n- Bullet item";
        let doc = from_markdown(md);
        assert_eq!(doc.blocks.len(), 4);

        assert_eq!(doc.blocks[0].kind, BlockKind::Heading(1));
        assert_eq!(doc.blocks[1].kind, BlockKind::Paragraph);
        assert_eq!(doc.blocks[2].kind, BlockKind::Blockquote);
        assert_eq!(doc.blocks[3].kind, BlockKind::BulletListItem);

        let serialized = to_markdown(&doc);
        assert_eq!(serialized, md);
    }
}
