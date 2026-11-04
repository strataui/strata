//! HTML5 document tree builder with error tolerance and semantic sanitization.

use crate::ast::SemanticNode;
use crate::tokenizer::{HtmlToken, HtmlTokenizer};

/// Set of standard HTML void elements that must not have children or closing tags.
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input",
    "link", "meta", "param", "source", "track", "wbr",
];

/// Tags whose text/markup is completely ignored from output.
const STRIPPED_ELEMENTS: &[&str] = &[
    "script", "noscript",
];

/// Stack frame representing an unclosed element during tree construction.
struct ElementFrame {
    tag: String,
    attributes: Vec<(String, String)>,
    children: Vec<SemanticNode>,
}

/// Headless HTML parser that constructs a clean `SemanticNode` tree.
pub struct HtmlParser;

impl HtmlParser {
    /// Parses raw HTML text into a list of top-level semantic nodes.
    pub fn parse(html: &str) -> Vec<SemanticNode> {
        let tokens = HtmlTokenizer::new(html).tokenize();
        Self::build_tree(tokens)
    }

    /// Parses raw HTML text into a single root `<div class="strata-article-root">` element.
    pub fn parse_document(html: &str) -> SemanticNode {
        let nodes = Self::parse(html);
        SemanticNode::element(
            "div",
            vec![("class".to_string(), "strata-article-root".to_string())],
            nodes,
        )
    }

    fn build_tree(tokens: Vec<HtmlToken>) -> Vec<SemanticNode> {
        let mut root_nodes = Vec::new();
        let mut stack: Vec<ElementFrame> = Vec::new();
        let mut skipping_tag: Option<String> = None;

        for token in tokens {
            if let Some(ref skip_name) = skipping_tag {
                if let HtmlToken::EndTag { name } = token {
                    if name.eq_ignore_ascii_case(skip_name) {
                        skipping_tag = None;
                    }
                }
                continue;
            }

            match token {
                HtmlToken::DocType(_) | HtmlToken::Comment(_) => {
                    // Omit DOCTYPE and HTML comments from semantic view output
                }
                HtmlToken::Text(text) => {
                    let should_keep = if let Some(top) = stack.last() {
                        if matches!(
                            top.tag.as_str(),
                            "table" | "tbody" | "thead" | "tfoot" | "tr" | "colgroup" | "ul" | "ol" | "dl" | "select" | "html" | "head"
                        ) {
                            !text.trim_matches(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0C')).is_empty()
                        } else {
                            !text.is_empty()
                        }
                    } else {
                        !text.trim_matches(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0C')).is_empty()
                    };
                    if should_keep {
                        let node = SemanticNode::Text(text);
                        if let Some(top) = stack.last_mut() {
                            top.children.push(node);
                        } else {
                            root_nodes.push(node);
                        }
                    }
                }
                HtmlToken::StartTag {
                    name,
                    attributes,
                    self_closing,
                } => {
                    let name_lower = name.to_lowercase();

                    if STRIPPED_ELEMENTS.contains(&name_lower.as_str()) {
                        if !self_closing {
                            skipping_tag = Some(name_lower);
                        }
                        continue;
                    }

                    let is_void = self_closing || VOID_ELEMENTS.contains(&name_lower.as_str());

                    if is_void {
                        let node = SemanticNode::Element {
                            tag: name_lower,
                            attributes,
                            children: Vec::new(),
                        };
                        if let Some(top) = stack.last_mut() {
                            top.children.push(node);
                        } else {
                            root_nodes.push(node);
                        }
                    } else {
                        stack.push(ElementFrame {
                            tag: name_lower,
                            attributes,
                            children: Vec::new(),
                        });
                    }
                }
                HtmlToken::EndTag { name } => {
                    let name_lower = name.to_lowercase();

                    // Search stack from top to bottom for matching tag
                    if let Some(pos) = stack.iter().rposition(|f| f.tag == name_lower) {
                        // Pop elements down to pos
                        while stack.len() > pos + 1 {
                            let unclosed = stack.pop().unwrap();
                            let node = SemanticNode::Element {
                                tag: unclosed.tag,
                                attributes: unclosed.attributes,
                                children: unclosed.children,
                            };
                            if let Some(top) = stack.last_mut() {
                                top.children.push(node);
                            } else {
                                root_nodes.push(node);
                            }
                        }

                        let finished = stack.pop().unwrap();
                        let node = SemanticNode::Element {
                            tag: finished.tag,
                            attributes: finished.attributes,
                            children: finished.children,
                        };

                        if let Some(top) = stack.last_mut() {
                            top.children.push(node);
                        } else {
                            root_nodes.push(node);
                        }
                    }
                }
            }
        }

        // Drain any remaining unclosed elements on the stack
        while let Some(frame) = stack.pop() {
            let node = SemanticNode::Element {
                tag: frame.tag,
                attributes: frame.attributes,
                children: frame.children,
            };
            if let Some(top) = stack.last_mut() {
                top.children.push(node);
            } else {
                root_nodes.push(node);
            }
        }

        root_nodes
    }
}
