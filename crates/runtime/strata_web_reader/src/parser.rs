//! HTML5 document tree builder with error tolerance and semantic sanitization.

use crate::ast::SemanticNode;
use crate::tokenizer::{HtmlToken, HtmlTokenizer};

/// Set of standard HTML void elements that must not have children or closing tags.
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Tags whose text/markup is completely ignored from output.
const STRIPPED_ELEMENTS: &[&str] = &["script", "noscript"];

/// Block-level HTML elements that implicitly close an open `<p>` element in button scope.
const BLOCK_TAGS_CLOSING_P: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "center",
    "details",
    "dialog",
    "dir",
    "div",
    "dl",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "header",
    "hgroup",
    "main",
    "menu",
    "nav",
    "ol",
    "p",
    "section",
    "summary",
    "ul",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "pre",
    "listing",
    "form",
    "hr",
    "table",
];

/// SVG shape and graphic leaf tags treated as self-closing void elements.
const SVG_LEAF_TAGS: &[&str] = &[
    "path", "circle", "rect", "line", "polygon", "polyline", "ellipse", "stop", "use",
];

/// Standard WHATWG HTML5 tree construction insertion modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertionMode {
    Initial,
    BeforeHtml,
    BeforeHead,
    InHead,
    AfterHead,
    InBody,
    InTable,
    InTableBody,
    InRow,
    InCell,
    AfterBody,
    AfterAfterBody,
}

/// Stack frame representing an unclosed element during tree construction.
struct ElementFrame {
    tag: String,
    attributes: Vec<(String, String)>,
    children: Vec<SemanticNode>,
}

/// State machine executing WHATWG HTML5 tree construction rules.
pub struct TreeBuilder {
    pub mode: InsertionMode,
    stack: Vec<ElementFrame>,
    pub root_nodes: Vec<SemanticNode>,
    skipping_tag: Option<String>,
}

/// Maximum depth of nested element frames in the WHATWG parser tree builder to prevent stack exhaustion / OOM (CWE-400).
pub const MAX_TREE_DEPTH: usize = 512;

impl TreeBuilder {
    pub fn new(is_full_document: bool) -> Self {
        Self {
            mode: if is_full_document {
                InsertionMode::Initial
            } else {
                InsertionMode::InBody
            },
            stack: Vec::new(),
            root_nodes: Vec::new(),
            skipping_tag: None,
        }
    }

    fn push_element(&mut self, tag: String, attributes: Vec<(String, String)>) {
        if self.stack.len() >= MAX_TREE_DEPTH {
            let node = SemanticNode::Element {
                tag,
                attributes,
                children: Vec::new(),
            };
            if let Some(top) = self.stack.last_mut() {
                top.children.push(node);
            } else {
                self.root_nodes.push(node);
            }
            return;
        }
        self.stack.push(ElementFrame {
            tag,
            attributes,
            children: Vec::new(),
        });
    }

    pub fn pop_element(&mut self) -> Option<SemanticNode> {
        if let Some(frame) = self.stack.pop() {
            let node = SemanticNode::Element {
                tag: frame.tag,
                attributes: frame.attributes,
                children: frame.children,
            };
            if let Some(top) = self.stack.last_mut() {
                top.children.push(node.clone());
            } else {
                self.root_nodes.push(node.clone());
            }
            Some(node)
        } else {
            None
        }
    }

    fn append_node_to_current(&mut self, node: SemanticNode) {
        if let Some(top) = self.stack.last_mut() {
            top.children.push(node);
        } else {
            self.root_nodes.push(node);
        }
    }

    fn has_in_scope(&self, target_tag: &str, limiters: &[&str]) -> bool {
        for frame in self.stack.iter().rev() {
            if frame.tag == target_tag {
                return true;
            }
            if limiters.contains(&frame.tag.as_str()) {
                return false;
            }
        }
        false
    }

    fn has_p_in_button_scope(&self) -> bool {
        self.has_in_scope(
            "p",
            &[
                "applet", "caption", "html", "table", "td", "th", "marquee", "object", "template",
            ],
        )
    }

    fn close_p_element(&mut self) {
        if self.has_p_in_button_scope() {
            while let Some(top) = self.stack.last() {
                let is_p = top.tag == "p";
                self.pop_element();
                if is_p {
                    break;
                }
            }
        }
    }

    fn has_in_list_scope(&self, target_tag: &str) -> bool {
        self.has_in_scope(target_tag, &["ul", "ol", "html", "table"])
    }

    fn close_element_named(&mut self, name: &str) {
        if let Some(pos) = self.stack.iter().rposition(|f| f.tag == name) {
            while self.stack.len() > pos {
                self.pop_element();
            }
        }
    }

    fn is_in_table(&self) -> bool {
        self.stack.iter().any(|f| f.tag == "table")
    }

    fn is_in_cell(&self) -> bool {
        self.stack
            .iter()
            .rev()
            .any(|f| f.tag == "td" || f.tag == "th")
    }

    fn foster_parent_node(&mut self, node: SemanticNode) {
        if let Some(table_pos) = self.stack.iter().rposition(|f| f.tag == "table") {
            if table_pos > 0 {
                self.stack[table_pos - 1].children.push(node);
            } else {
                self.root_nodes.push(node);
            }
        } else {
            self.append_node_to_current(node);
        }
    }

    pub fn process_token(&mut self, token: HtmlToken) {
        if let Some(ref skip_name) = self.skipping_tag {
            if let HtmlToken::EndTag { name } = token {
                if name.eq_ignore_ascii_case(skip_name) {
                    self.skipping_tag = None;
                }
            }
            return;
        }

        match token {
            HtmlToken::DocType(_) => {
                if self.mode == InsertionMode::Initial {
                    self.mode = InsertionMode::BeforeHtml;
                }
            }
            HtmlToken::Comment(_) => {}
            HtmlToken::Text(text) => {
                let is_ws = text
                    .trim_matches(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0C'))
                    .is_empty();

                if matches!(
                    self.mode,
                    InsertionMode::Initial
                        | InsertionMode::BeforeHtml
                        | InsertionMode::BeforeHead
                        | InsertionMode::AfterHead
                ) {
                    if is_ws {
                        return;
                    }
                    self.mode = InsertionMode::InBody;
                }

                let in_table_structure = self.stack.last().is_some_and(|top| {
                    matches!(
                        top.tag.as_str(),
                        "table" | "tbody" | "thead" | "tfoot" | "tr"
                    )
                });

                if in_table_structure {
                    if is_ws {
                        return;
                    }
                    // Foster parenting: misplaced non-whitespace text inside table structure
                    self.foster_parent_node(SemanticNode::Text(text));
                    return;
                }

                let should_keep = if let Some(top) = self.stack.last() {
                    if matches!(
                        top.tag.as_str(),
                        "table"
                            | "tbody"
                            | "thead"
                            | "tfoot"
                            | "tr"
                            | "colgroup"
                            | "select"
                            | "html"
                            | "head"
                            | "svg"
                            | "g"
                            | "defs"
                    ) {
                        !is_ws
                    } else {
                        !text.is_empty()
                    }
                } else {
                    !is_ws
                };

                if should_keep {
                    self.append_node_to_current(SemanticNode::Text(text));
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
                        self.skipping_tag = Some(name_lower);
                    }
                    return;
                }

                // SVG foreign elements
                if self.stack.iter().any(|f| f.tag == "svg")
                    && (self_closing || SVG_LEAF_TAGS.contains(&name_lower.as_str()))
                {
                    self.append_node_to_current(SemanticNode::Element {
                        tag: name_lower,
                        attributes,
                        children: Vec::new(),
                    });
                    return;
                }

                let is_void = self_closing || VOID_ELEMENTS.contains(&name_lower.as_str());
                if is_void {
                    if name_lower == "hr" {
                        self.close_p_element();
                    }
                    let node = SemanticNode::Element {
                        tag: name_lower,
                        attributes,
                        children: Vec::new(),
                    };
                    if self.is_in_table() && !self.is_in_cell() {
                        self.foster_parent_node(node);
                    } else {
                        self.append_node_to_current(node);
                    }
                    return;
                }

                // Mode transitions for document structure
                if name_lower == "html" {
                    if matches!(
                        self.mode,
                        InsertionMode::Initial | InsertionMode::BeforeHtml
                    ) {
                        self.push_element(name_lower, attributes);
                        self.mode = InsertionMode::BeforeHead;
                        return;
                    }
                } else if name_lower == "head" {
                    if self.mode == InsertionMode::BeforeHead {
                        self.push_element(name_lower, attributes);
                        self.mode = InsertionMode::InHead;
                        return;
                    }
                } else if name_lower == "body" {
                    if self.mode == InsertionMode::InHead {
                        self.close_element_named("head");
                        self.mode = InsertionMode::AfterHead;
                    }
                    if matches!(
                        self.mode,
                        InsertionMode::AfterHead
                            | InsertionMode::BeforeHead
                            | InsertionMode::BeforeHtml
                    ) {
                        self.push_element(name_lower, attributes);
                        self.mode = InsertionMode::InBody;
                        return;
                    }
                }

                if matches!(
                    self.mode,
                    InsertionMode::Initial
                        | InsertionMode::BeforeHtml
                        | InsertionMode::BeforeHead
                        | InsertionMode::AfterHead
                ) {
                    self.mode = InsertionMode::InBody;
                }

                // Paragraph auto-closing
                if name_lower == "p" {
                    self.close_p_element();
                    self.push_element(name_lower, attributes);
                    return;
                } else if BLOCK_TAGS_CLOSING_P.contains(&name_lower.as_str()) {
                    self.close_p_element();
                }

                // List item auto-closing
                if name_lower == "li" {
                    if self.has_in_list_scope("li") {
                        self.close_element_named("li");
                    }
                    self.push_element(name_lower, attributes);
                    return;
                } else if name_lower == "dt" || name_lower == "dd" {
                    if self.has_in_scope("dt", &["dl", "html", "table"]) {
                        self.close_element_named("dt");
                    } else if self.has_in_scope("dd", &["dl", "html", "table"]) {
                        self.close_element_named("dd");
                    }
                    self.push_element(name_lower, attributes);
                    return;
                }

                // Select option auto-closing
                if name_lower == "option" {
                    if self.stack.last().is_some_and(|f| f.tag == "option") {
                        self.pop_element();
                    }
                    self.push_element(name_lower, attributes);
                    return;
                } else if name_lower == "optgroup" {
                    if self.stack.last().is_some_and(|f| f.tag == "option") {
                        self.pop_element();
                    }
                    if self.stack.last().is_some_and(|f| f.tag == "optgroup") {
                        self.pop_element();
                    }
                    self.push_element(name_lower, attributes);
                    return;
                }

                // Table structure handling
                if name_lower == "table" {
                    self.close_p_element();
                    self.push_element(name_lower, attributes);
                    self.mode = InsertionMode::InTable;
                    return;
                } else if matches!(name_lower.as_str(), "tbody" | "thead" | "tfoot") {
                    if self.stack.last().is_some_and(|f| f.tag == "caption") {
                        self.close_element_named("caption");
                    }
                    if let Some(pos) = self
                        .stack
                        .iter()
                        .rposition(|f| matches!(f.tag.as_str(), "tbody" | "thead" | "tfoot"))
                    {
                        while self.stack.len() > pos {
                            self.pop_element();
                        }
                    }
                    self.push_element(name_lower, attributes);
                    self.mode = InsertionMode::InTableBody;
                    return;
                } else if name_lower == "tr" {
                    if self.stack.last().is_some_and(|f| f.tag == "caption") {
                        self.close_element_named("caption");
                    }
                    if self.is_in_cell() {
                        self.close_element_named("td");
                        self.close_element_named("th");
                    }
                    if self.stack.last().is_some_and(|f| f.tag == "tr") {
                        self.pop_element();
                    }
                    if self.stack.last().is_some_and(|f| f.tag == "table") {
                        self.push_element("tbody".to_string(), Vec::new());
                    }
                    self.push_element(name_lower, attributes);
                    self.mode = InsertionMode::InRow;
                    return;
                } else if name_lower == "td" || name_lower == "th" {
                    if self.is_in_cell() {
                        self.close_element_named("td");
                        self.close_element_named("th");
                    }
                    if !self.stack.last().is_some_and(|f| f.tag == "tr") {
                        if self.stack.last().is_some_and(|f| f.tag == "table") {
                            self.push_element("tbody".to_string(), Vec::new());
                        }
                        self.push_element("tr".to_string(), Vec::new());
                    }
                    self.push_element(name_lower, attributes);
                    self.mode = InsertionMode::InCell;
                    return;
                }

                // Foster parenting for non-table elements inside table
                let in_table_structure = self.stack.last().is_some_and(|top| {
                    matches!(
                        top.tag.as_str(),
                        "table" | "tbody" | "thead" | "tfoot" | "tr"
                    )
                });

                if in_table_structure
                    && !matches!(
                        name_lower.as_str(),
                        "caption"
                            | "colgroup"
                            | "col"
                            | "thead"
                            | "tbody"
                            | "tfoot"
                            | "tr"
                            | "td"
                            | "th"
                            | "style"
                            | "script"
                    )
                {
                    let node = SemanticNode::Element {
                        tag: name_lower,
                        attributes,
                        children: Vec::new(),
                    };
                    self.foster_parent_node(node);
                    return;
                }

                self.push_element(name_lower, attributes);
            }
            HtmlToken::EndTag { name } => {
                let name_lower = name.to_lowercase();

                if name_lower == "p" {
                    self.close_p_element();
                    return;
                } else if name_lower == "table" {
                    if self.is_in_cell() {
                        self.close_element_named("td");
                        self.close_element_named("th");
                    }
                    if self.stack.last().is_some_and(|f| f.tag == "tr") {
                        self.pop_element();
                    }
                    if self
                        .stack
                        .last()
                        .is_some_and(|f| matches!(f.tag.as_str(), "tbody" | "thead" | "tfoot"))
                    {
                        self.pop_element();
                    }
                    self.close_element_named("table");
                    self.mode = InsertionMode::InBody;
                    return;
                } else if name_lower == "tr" {
                    if self.is_in_cell() {
                        self.close_element_named("td");
                        self.close_element_named("th");
                    }
                    self.close_element_named("tr");
                    self.mode = InsertionMode::InTableBody;
                    return;
                } else if matches!(name_lower.as_str(), "td" | "th") {
                    self.close_element_named(&name_lower);
                    self.mode = InsertionMode::InRow;
                    return;
                } else if matches!(name_lower.as_str(), "tbody" | "thead" | "tfoot") {
                    if self.is_in_cell() {
                        self.close_element_named("td");
                        self.close_element_named("th");
                    }
                    if self.stack.last().is_some_and(|f| f.tag == "tr") {
                        self.pop_element();
                    }
                    self.close_element_named(&name_lower);
                    self.mode = InsertionMode::InTable;
                    return;
                } else if name_lower == "head" {
                    self.close_element_named("head");
                    self.mode = InsertionMode::AfterHead;
                    return;
                } else if name_lower == "body" {
                    self.close_element_named("body");
                    self.mode = InsertionMode::AfterBody;
                    return;
                } else if name_lower == "html" {
                    self.close_element_named("html");
                    self.mode = InsertionMode::AfterAfterBody;
                    return;
                }

                self.close_element_named(&name_lower);
            }
        }
    }
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
        let is_full_document = tokens.iter().any(|t| match t {
            HtmlToken::DocType(_) => true,
            HtmlToken::StartTag { name, .. } => name.eq_ignore_ascii_case("html"),
            _ => false,
        });

        let mut builder = TreeBuilder::new(is_full_document);
        for token in tokens {
            builder.process_token(token);
        }

        // Drain any remaining unclosed elements
        while builder.pop_element().is_some() {}

        builder.root_nodes
    }
}

/// Describes an executable script extracted from an HTML document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptDescriptor {
    /// Inline script block `<script>...</script>`.
    Inline {
        code: String,
        nonce: Option<String>,
        integrity: Option<String>,
    },
    /// External script reference `<script src="...">`.
    External {
        src: String,
        nonce: Option<String>,
        integrity: Option<String>,
        is_async: bool,
        is_defer: bool,
    },
}

/// Helper to parse attribute key-values from `<script ...>` tag text.
fn parse_script_attributes(
    attrs_str: &str,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    bool,
    bool,
) {
    let mut src = None;
    let mut nonce = None;
    let mut integrity = None;
    let mut script_type = None;
    let mut is_async = false;
    let mut is_defer = false;

    let bytes = attrs_str.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }

        let key_start = i;
        while i < bytes.len()
            && !bytes[i].is_ascii_whitespace()
            && bytes[i] != b'='
            && bytes[i] != b'>'
            && bytes[i] != b'/'
        {
            i += 1;
        }
        let key = &attrs_str[key_start..i].to_ascii_lowercase();

        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }

        let mut val = String::new();
        if i < bytes.len() && bytes[i] == b'=' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < bytes.len() {
                if bytes[i] == b'"' || bytes[i] == b'\'' {
                    let quote = bytes[i];
                    i += 1;
                    let val_start = i;
                    while i < bytes.len() && bytes[i] != quote {
                        i += 1;
                    }
                    val = attrs_str[val_start..i].to_string();
                    if i < bytes.len() && bytes[i] == quote {
                        i += 1;
                    }
                } else {
                    let val_start = i;
                    while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' {
                        i += 1;
                    }
                    val = attrs_str[val_start..i].to_string();
                }
            }
        }

        match key.as_str() {
            "src" => src = Some(val),
            "nonce" => nonce = Some(val),
            "integrity" => integrity = Some(val),
            "type" => script_type = Some(val),
            "async" => is_async = true,
            "defer" => is_defer = true,
            _ => {}
        }
    }

    (src, nonce, integrity, script_type, is_async, is_defer)
}

/// Extracts all `<script>` descriptors (both inline code and external `src` bundles) from the HTML.
pub fn extract_script_descriptors(html: &str) -> Vec<ScriptDescriptor> {
    let mut descriptors = Vec::new();
    let lower = html.to_ascii_lowercase();
    let mut pos = 0;

    while let Some(start_idx) = lower[pos..].find("<script") {
        let abs_start = pos + start_idx;
        let after_script = abs_start + 7;
        if after_script >= html.len() {
            break;
        }

        let ch = lower.as_bytes()[after_script];
        if ch != b'>' && ch != b'/' && !ch.is_ascii_whitespace() {
            pos = abs_start + 7;
            continue;
        }

        let tag_end = match lower[after_script..].find('>') {
            Some(end) => after_script + end,
            None => break,
        };

        let attrs_str = &html[after_script..tag_end];
        let (src, nonce, integrity, script_type, is_async, is_defer) =
            parse_script_attributes(attrs_str);

        // Filter out non-JavaScript script blocks (e.g. application/ld+json, text/template)
        let is_executable_js = match script_type.as_deref() {
            None => true,
            Some(t) => {
                let t_lower = t.to_ascii_lowercase();
                let clean = t_lower.trim();
                clean.is_empty()
                    || clean == "text/javascript"
                    || clean == "application/javascript"
                    || clean == "text/ecmascript"
                    || clean == "application/ecmascript"
                    || clean == "module"
            }
        };

        if !is_executable_js {
            pos = tag_end + 1;
            continue;
        }

        let code_start = tag_end + 1;
        let close_tag = "</script>";
        let (code, next_pos) = if let Some(close_idx) = lower[code_start..].find(close_tag) {
            let inner = &html[code_start..code_start + close_idx];
            (inner, code_start + close_idx + close_tag.len())
        } else {
            (&html[code_start..], html.len())
        };

        if let Some(src_url) = src {
            if !src_url.trim().is_empty() {
                descriptors.push(ScriptDescriptor::External {
                    src: src_url.trim().to_string(),
                    nonce,
                    integrity,
                    is_async,
                    is_defer,
                });
            }
        } else {
            let trimmed = code.trim().to_string();
            if !trimmed.is_empty() {
                descriptors.push(ScriptDescriptor::Inline {
                    code: trimmed,
                    nonce,
                    integrity,
                });
            }
        }

        pos = next_pos;
    }

    descriptors
}

/// Extracts the raw JavaScript code contents from `<script>` tags in the HTML.
pub fn extract_scripts(html: &str) -> Vec<String> {
    extract_script_descriptors(html)
        .into_iter()
        .filter_map(|d| match d {
            ScriptDescriptor::Inline { code, .. } => Some(code),
            _ => None,
        })
        .collect()
}

/// Converts a `strata_js::DomNode` into a `strata_web_reader::SemanticNode`.
pub fn from_dom_node(node: &strata_js::DomNode) -> SemanticNode {
    let inner = node.inner.borrow();
    match &inner.node_type {
        strata_js::DomNodeType::Element {
            tag, attributes, ..
        } => {
            let mut attrs: Vec<(String, String)> = attributes
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            let uid = node.uid();
            if !attrs.iter().any(|(k, _)| k == "data-strata-node-uid") {
                attrs.push(("data-strata-node-uid".to_string(), uid.to_string()));
            }
            let children: Vec<SemanticNode> = inner.children.iter().map(from_dom_node).collect();
            SemanticNode::element(tag.clone(), attrs, children)
        }
        strata_js::DomNodeType::Text(text) => SemanticNode::text(text.clone()),
        strata_js::DomNodeType::Comment(comment) => SemanticNode::Comment(comment.clone()),
    }
}

/// Converts a `strata_js::Document` into a list of top-level semantic nodes.
pub fn document_to_semantic_nodes(doc: &strata_js::Document) -> Vec<SemanticNode> {
    if !doc.head.children().is_empty() {
        vec![from_dom_node(&doc.html)]
    } else {
        doc.body.children().iter().map(from_dom_node).collect()
    }
}

/// Converts a `SemanticNode` into a `strata_js::DomNode`.
pub fn semantic_node_to_dom_node(node: &SemanticNode) -> strata_js::DomNode {
    match node {
        SemanticNode::Element {
            tag,
            attributes,
            children,
        } => {
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
                        if let SemanticNode::Element {
                            tag: c_tag,
                            children: c_children,
                            ..
                        } = c
                        {
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
    doc.clear_mutated();
    doc
}
