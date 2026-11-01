//! Multi-format clipboard bridge supporting Plain Text, Markdown, Semantic HTML, and RTF.

use crate::buffer::LineBuffer;
use crate::cursor::Selection;
use crate::markdown::to_markdown;
use crate::model::{BlockKind, Document, InlineMark, InlineSpan};

/// Multi-format clipboard payload for native OS clipboard exchange.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ClipboardPayload {
    /// Standard plain-text UTF-8 string.
    pub plain_text: String,
    /// GitHub Flavored Markdown representation.
    pub markdown: Option<String>,
    /// Semantic HTML markup representation.
    pub html: Option<String>,
    /// Rich Text Format (RTF) string for desktop native paste compatibility.
    pub rtf: Option<String>,
}

/// Clipboard serialization and parsing bridge.
pub struct ClipboardBridge;

impl ClipboardBridge {
    /// Extracts a multi-format clipboard payload from a line buffer selection.
    pub fn from_selection(buffer: &LineBuffer, sel: Selection) -> ClipboardPayload {
        let (start, end) = sel.ordered_range();

        if start == end {
            return ClipboardPayload::default();
        }

        let mut lines = Vec::new();
        if start.line == end.line {
            if let Some(l) = buffer.line(start.line) {
                let sc = start.col.min(l.len());
                let ec = end.col.min(l.len());
                lines.push(l[sc..ec].to_string());
            }
        } else {
            if let Some(first) = buffer.line(start.line) {
                let sc = start.col.min(first.len());
                lines.push(first[sc..].to_string());
            }
            for l in start.line + 1..end.line {
                if let Some(mid) = buffer.line(l) {
                    lines.push(mid.to_string());
                }
            }
            if let Some(last) = buffer.line(end.line) {
                let ec = end.col.min(last.len());
                lines.push(last[..ec].to_string());
            }
        }

        let plain_text = lines.join("\n");
        let html = format!("<pre><code>{}</code></pre>", plain_text);
        let rtf = format!(r"{{\rtf1\ansi\deff0 {{\fonttbl {{\f0 Courier;}}}} \f0 {} }}", plain_text);

        ClipboardPayload {
            plain_text: plain_text.clone(),
            markdown: Some(plain_text),
            html: Some(html),
            rtf: Some(rtf),
        }
    }

    /// Serializes a rich-text `Document` into all four clipboard representations.
    pub fn from_document(doc: &Document) -> ClipboardPayload {
        let markdown = to_markdown(doc);
        let html = Self::document_to_html(doc);
        let rtf = Self::document_to_rtf(doc);

        let mut plain_text = String::new();
        for b in &doc.blocks {
            plain_text.push_str(&b.plain_text());
            plain_text.push('\n');
        }

        ClipboardPayload {
            plain_text: plain_text.trim_end().to_string(),
            markdown: Some(markdown),
            html: Some(html),
            rtf: Some(rtf),
        }
    }

    /// Converts rich document blocks to clean semantic HTML.
    pub fn document_to_html(doc: &Document) -> String {
        let mut out = String::from("<div class=\"strata-document\">\n");

        for b in &doc.blocks {
            match b.kind {
                BlockKind::Paragraph => {
                    out.push_str("<p>");
                    Self::append_spans_html(&b.spans, &mut out);
                    out.push_str("</p>\n");
                }
                BlockKind::Heading(lvl) => {
                    out.push_str(&format!("<h{}>", lvl));
                    Self::append_spans_html(&b.spans, &mut out);
                    out.push_str(&format!("</h{}>\n", lvl));
                }
                BlockKind::Blockquote => {
                    out.push_str("<blockquote>");
                    Self::append_spans_html(&b.spans, &mut out);
                    out.push_str("</blockquote>\n");
                }
                BlockKind::CodeBlock(ref lang) => {
                    let lang_attr = lang.as_deref().map_or(String::new(), |l| format!(" class=\"language-{}\"", l));
                    out.push_str(&format!("<pre><code{}>", lang_attr));
                    Self::append_spans_html(&b.spans, &mut out);
                    out.push_str("</code></pre>\n");
                }
                BlockKind::BulletListItem => {
                    out.push_str("<li>");
                    Self::append_spans_html(&b.spans, &mut out);
                    out.push_str("</li>\n");
                }
                BlockKind::NumberedListItem(num) => {
                    out.push_str(&format!("<li value=\"{}\">", num));
                    Self::append_spans_html(&b.spans, &mut out);
                    out.push_str("</li>\n");
                }
                BlockKind::HorizontalRule => {
                    out.push_str("<hr />\n");
                }
            }
        }

        out.push_str("</div>");
        out
    }

    fn append_spans_html(spans: &[InlineSpan], out: &mut String) {
        for span in spans {
            let mut prefix = String::new();
            let mut suffix = String::new();

            for m in &span.marks {
                match m {
                    InlineMark::Bold => {
                        prefix.push_str("<strong>");
                        suffix.insert_str(0, "</strong>");
                    }
                    InlineMark::Italic => {
                        prefix.push_str("<em>");
                        suffix.insert_str(0, "</em>");
                    }
                    InlineMark::Underline => {
                        prefix.push_str("<u>");
                        suffix.insert_str(0, "</u>");
                    }
                    InlineMark::Strike => {
                        prefix.push_str("<s>");
                        suffix.insert_str(0, "</s>");
                    }
                    InlineMark::Code => {
                        prefix.push_str("<code>");
                        suffix.insert_str(0, "</code>");
                    }
                    InlineMark::Link(ref href) => {
                        prefix.push_str(&format!("<a href=\"{}\">", href));
                        suffix.insert_str(0, "</a>");
                    }
                }
            }

            out.push_str(&prefix);
            out.push_str(&span.text);
            out.push_str(&suffix);
        }
    }

    /// Converts document blocks to RTF format.
    pub fn document_to_rtf(doc: &Document) -> String {
        let mut rtf = String::from(r"{\rtf1\ansi\deff0 {\fonttbl {\f0 Arial;}} \f0 ");

        for b in &doc.blocks {
            match b.kind {
                BlockKind::Heading(_) => rtf.push_str(r"\b\fs32 "),
                BlockKind::Paragraph => rtf.push_str(r"\fs24 "),
                _ => rtf.push_str(r"\fs24 "),
            }

            for span in &b.spans {
                let is_bold = span.marks.contains(&InlineMark::Bold);
                let is_italic = span.marks.contains(&InlineMark::Italic);

                if is_bold { rtf.push_str(r"\b "); }
                if is_italic { rtf.push_str(r"\i "); }

                rtf.push_str(&span.text);

                if is_italic { rtf.push_str(r"\i0 "); }
                if is_bold { rtf.push_str(r"\b0 "); }
            }

            rtf.push_str(r"\par ");
        }

        rtf.push('}');
        rtf
    }
}
