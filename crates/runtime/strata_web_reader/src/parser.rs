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

    /// Parses raw HTML text into a list of semantic nodes, executing inline `<script>` tags via `strata_js`.
    pub fn parse_with_scripts(html: &str) -> Vec<SemanticNode> {
        let base_nodes = Self::parse(html);
        let scripts = extract_scripts(html);
        if scripts.is_empty() {
            return base_nodes;
        }

        let doc = semantic_nodes_to_document(&base_nodes);
        let mut engine = strata_js::JsEngine::with_document(doc);
        for script in &scripts {
            let _ = engine.eval(script);
        }

        if engine.document.has_mutated() {
            let nodes = document_to_semantic_nodes(&engine.document);
            if !nodes.is_empty() {
                return nodes;
            }
        }

        base_nodes
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
                            if let Some(unclosed) = stack.pop() {
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
                        }

                        if let Some(finished) = stack.pop() {
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

/// Extracts the raw JavaScript code contents from `<script>` tags in the HTML.
pub fn extract_scripts(html: &str) -> Vec<String> {
    let mut scripts = Vec::new();
    let lower = html.to_ascii_lowercase();
    let mut pos = 0;
    while let Some(start_idx) = lower[pos..].find("<script") {
        let abs_start = pos + start_idx;
        let after_script = abs_start + 7;
        if after_script < html.len() {
            let ch = lower.as_bytes()[after_script];
            if ch == b'>' || ch == b' ' || ch == b'\t' || ch == b'\n' || ch == b'\r' {
                if let Some(tag_end) = lower[after_script..].find('>') {
                    let code_start = after_script + tag_end + 1;
                    if let Some(close_idx) = lower[code_start..].find("</script>") {
                        let code = &html[code_start..code_start + close_idx];
                        scripts.push(code.trim().to_string());
                        pos = code_start + close_idx + 9;
                        continue;
                    }
                }
            }
        }
        pos = abs_start + 7;
    }
    scripts
}

/// Converts a `strata_js::DomNode` into a `strata_web_reader::SemanticNode`.
pub fn from_dom_node(node: &strata_js::DomNode) -> SemanticNode {
    let inner = node.inner.borrow();
    match &inner.node_type {
        strata_js::DomNodeType::Element { tag, attributes, .. } => {
            let attrs: Vec<(String, String)> = attributes
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            let children: Vec<SemanticNode> = inner
                .children
                .iter()
                .map(from_dom_node)
                .collect();
            SemanticNode::element(tag.clone(), attrs, children)
        }
        strata_js::DomNodeType::Text(text) => SemanticNode::text(text.clone()),
        strata_js::DomNodeType::Comment(comment) => SemanticNode::Comment(comment.clone()),
    }
}

/// Converts a `strata_js::Document` into a list of top-level semantic nodes.
pub fn document_to_semantic_nodes(doc: &strata_js::Document) -> Vec<SemanticNode> {
    doc.body
        .children()
        .iter()
        .map(from_dom_node)
        .collect()
}

/// Converts a `SemanticNode` into a `strata_js::DomNode`.
pub fn semantic_node_to_dom_node(node: &SemanticNode) -> strata_js::DomNode {
    match node {
        SemanticNode::Element { tag, attributes, children } => {
            let mut map = std::collections::HashMap::new();
            for (k, v) in attributes {
                map.insert(k.to_ascii_lowercase(), v.clone());
            }
            let elem = strata_js::DomNode::element(tag, map);
            for child in children {
                elem.append_child(semantic_node_to_dom_node(child));
            }
            elem
        }
        SemanticNode::Text(text) => strata_js::DomNode::text(text),
        SemanticNode::Comment(comment) => strata_js::DomNode::comment(comment),
    }
}

/// Converts a list of semantic nodes into a `strata_js::Document`.
pub fn semantic_nodes_to_document(nodes: &[SemanticNode]) -> strata_js::Document {
    let doc = strata_js::Document::new();
    for node in nodes {
        match node {
            SemanticNode::Element { tag, children, .. } => {
                if tag.eq_ignore_ascii_case("html") {
                    for c in children {
                        if let SemanticNode::Element { tag: c_tag, children: c_children, .. } = c {
                            if c_tag.eq_ignore_ascii_case("head") {
                                for h_child in c_children {
                                    doc.head.append_child(semantic_node_to_dom_node(h_child));
                                }
                            } else if c_tag.eq_ignore_ascii_case("body") {
                                for b_child in c_children {
                                    doc.body.append_child(semantic_node_to_dom_node(b_child));
                                }
                            } else {
                                doc.body.append_child(semantic_node_to_dom_node(c));
                            }
                        } else {
                            doc.body.append_child(semantic_node_to_dom_node(c));
                        }
                    }
                } else if tag.eq_ignore_ascii_case("head") {
                    for h_child in children {
                        doc.head.append_child(semantic_node_to_dom_node(h_child));
                    }
                } else if tag.eq_ignore_ascii_case("body") {
                    for b_child in children {
                        doc.body.append_child(semantic_node_to_dom_node(b_child));
                    }
                } else {
                    doc.body.append_child(semantic_node_to_dom_node(node));
                }
            }
            _ => {
                doc.body.append_child(semantic_node_to_dom_node(node));
            }
        }
    }
    doc
}

