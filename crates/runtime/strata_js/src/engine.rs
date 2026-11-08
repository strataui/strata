//! Pure-Rust JavaScript Execution Engine, AST Parser, and W3C DOM Bindings.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU64, Ordering};
use crate::json::{parse_json, stringify_json};
use crate::promise::{drain_microtasks, JsPromise, PromiseState, RejectFn, ResolveFn};
use crate::JsValue;

// ============================================================================
// 1. DOM Tree Representation
// ============================================================================

static NEXT_DOM_UID: AtomicU64 = AtomicU64::new(1);

/// Types of DOM nodes supported by the engine.
#[derive(Debug, Clone, PartialEq)]
pub enum DomNodeType {
    Element {
        tag: String,
        attributes: HashMap<String, String>,
        style: HashMap<String, String>,
    },
    Text(String),
    Comment(String),
}

/// A node in the Document Object Model tree.
#[derive(Clone)]
pub struct DomNode {
    pub inner: Rc<RefCell<DomNodeInner>>,
}

pub struct DomNodeInner {
    pub uid: u64,
    pub node_type: DomNodeType,
    pub parent: Option<Weak<RefCell<DomNodeInner>>>,
    pub children: Vec<DomNode>,
    pub event_listeners: HashMap<String, Vec<JsValue>>,
}

impl DomNode {
    pub fn element(tag: &str, attributes: HashMap<String, String>) -> Self {
        Self {
            inner: Rc::new(RefCell::new(DomNodeInner {
                uid: NEXT_DOM_UID.fetch_add(1, Ordering::Relaxed),
                node_type: DomNodeType::Element {
                    tag: tag.to_ascii_lowercase(),
                    attributes,
                    style: HashMap::new(),
                },
                parent: None,
                children: Vec::new(),
                event_listeners: HashMap::new(),
            })),
        }
    }

    pub fn text(content: &str) -> Self {
        Self {
            inner: Rc::new(RefCell::new(DomNodeInner {
                uid: NEXT_DOM_UID.fetch_add(1, Ordering::Relaxed),
                node_type: DomNodeType::Text(content.to_string()),
                parent: None,
                children: Vec::new(),
                event_listeners: HashMap::new(),
            })),
        }
    }

    pub fn comment(content: &str) -> Self {
        Self {
            inner: Rc::new(RefCell::new(DomNodeInner {
                uid: NEXT_DOM_UID.fetch_add(1, Ordering::Relaxed),
                node_type: DomNodeType::Comment(content.to_string()),
                parent: None,
                children: Vec::new(),
                event_listeners: HashMap::new(),
            })),
        }
    }

    pub fn tag_name(&self) -> Option<String> {
        let inner = self.inner.borrow();
        match &inner.node_type {
            DomNodeType::Element { tag, .. } => Some(tag.to_ascii_uppercase()),
            _ => None,
        }
    }

    pub fn id(&self) -> Option<String> {
        self.get_attribute("id")
    }

    pub fn class_name(&self) -> String {
        self.get_attribute("class").unwrap_or_default()
    }

    pub fn set_class_name(&self, val: &str) {
        self.set_attribute("class", val);
    }

    pub fn get_attribute(&self, name: &str) -> Option<String> {
        let inner = self.inner.borrow();
        match &inner.node_type {
            DomNodeType::Element { attributes, .. } => {
                let name_lower = name.to_ascii_lowercase();
                attributes.get(&name_lower).cloned()
            }
            _ => None,
        }
    }

    pub fn set_attribute(&self, name: &str, val: &str) {
        let mut inner = self.inner.borrow_mut();
        if let DomNodeType::Element { ref mut attributes, .. } = inner.node_type {
            attributes.insert(name.to_ascii_lowercase(), val.to_string());
        }
    }

    pub fn remove_attribute(&self, name: &str) {
        let mut inner = self.inner.borrow_mut();
        if let DomNodeType::Element { ref mut attributes, .. } = inner.node_type {
            attributes.remove(&name.to_ascii_lowercase());
        }
    }

    pub fn has_attribute(&self, name: &str) -> bool {
        self.get_attribute(name).is_some()
    }

    pub fn get_style(&self, prop: &str) -> Option<String> {
        let inner = self.inner.borrow();
        match &inner.node_type {
            DomNodeType::Element { style, .. } => style.get(&prop.to_ascii_lowercase()).cloned(),
            _ => None,
        }
    }

    pub fn set_style(&self, prop: &str, val: &str) {
        let mut inner = self.inner.borrow_mut();
        if let DomNodeType::Element { ref mut style, .. } = inner.node_type {
            style.insert(prop.to_ascii_lowercase(), val.to_string());
        }
    }

    pub fn text_content(&self) -> String {
        let inner = self.inner.borrow();
        match &inner.node_type {
            DomNodeType::Text(s) => s.clone(),
            DomNodeType::Comment(_) => String::new(),
            DomNodeType::Element { .. } => {
                let mut out = String::new();
                for child in &inner.children {
                    out.push_str(&child.text_content());
                }
                out
            }
        }
    }

    pub fn uid(&self) -> u64 {
        self.inner.borrow().uid
    }

    pub fn set_text_content(&self, text: &str) {
        let child = DomNode::text(text);
        child.inner.borrow_mut().parent = Some(Rc::downgrade(&self.inner));
        let mut inner = self.inner.borrow_mut();
        inner.children.clear();
        inner.children.push(child);
    }

    pub fn inner_html(&self) -> String {
        let inner = self.inner.borrow();
        let mut out = String::new();
        for child in &inner.children {
            out.push_str(&child.outer_html());
        }
        out
    }

    pub fn set_inner_html(&self, html: &str) {
        let parsed = parse_html_fragment(html);
        for child in &parsed {
            child.inner.borrow_mut().parent = Some(Rc::downgrade(&self.inner));
        }
        let mut inner = self.inner.borrow_mut();
        inner.children = parsed;
    }

    pub fn outer_html(&self) -> String {
        let inner = self.inner.borrow();
        match &inner.node_type {
            DomNodeType::Text(s) => s.clone(),
            DomNodeType::Comment(c) => format!("<!--{c}-->"),
            DomNodeType::Element { tag, attributes, .. } => {
                let mut out = format!("<{tag}");
                for (k, v) in attributes {
                    out.push_str(&format!(" {k}=\"{v}\""));
                }
                if is_void_element(tag) && inner.children.is_empty() {
                    out.push_str(" />");
                } else {
                    out.push('>');
                    for child in &inner.children {
                        out.push_str(&child.outer_html());
                    }
                    out.push_str(&format!("</{tag}>"));
                }
                out
            }
        }
    }

    pub fn parent_node(&self) -> Option<DomNode> {
        self.inner
            .borrow()
            .parent
            .as_ref()
            .and_then(|w| w.upgrade())
            .map(|inner| DomNode { inner })
    }

    pub fn parent_element(&self) -> Option<DomNode> {
        let parent = self.parent_node()?;
        if matches!(parent.inner.borrow().node_type, DomNodeType::Element { .. }) {
            Some(parent)
        } else {
            None
        }
    }

    pub fn first_child(&self) -> Option<DomNode> {
        self.inner.borrow().children.first().cloned()
    }

    pub fn last_child(&self) -> Option<DomNode> {
        self.inner.borrow().children.last().cloned()
    }

    pub fn first_element_child(&self) -> Option<DomNode> {
        let inner = self.inner.borrow();
        inner
            .children
            .iter()
            .find(|c| matches!(c.inner.borrow().node_type, DomNodeType::Element { .. }))
            .cloned()
    }

    pub fn last_element_child(&self) -> Option<DomNode> {
        let inner = self.inner.borrow();
        inner
            .children
            .iter()
            .rfind(|c| matches!(c.inner.borrow().node_type, DomNodeType::Element { .. }))
            .cloned()
    }

    pub fn previous_sibling(&self) -> Option<DomNode> {
        let parent = self.parent_node()?;
        let siblings = parent.children();
        let idx = siblings.iter().position(|c| Rc::ptr_eq(&c.inner, &self.inner))?;
        if idx > 0 {
            Some(siblings[idx - 1].clone())
        } else {
            None
        }
    }

    pub fn next_sibling(&self) -> Option<DomNode> {
        let parent = self.parent_node()?;
        let siblings = parent.children();
        let idx = siblings.iter().position(|c| Rc::ptr_eq(&c.inner, &self.inner))?;
        if idx + 1 < siblings.len() {
            Some(siblings[idx + 1].clone())
        } else {
            None
        }
    }

    pub fn previous_element_sibling(&self) -> Option<DomNode> {
        let mut curr = self.previous_sibling();
        while let Some(node) = curr {
            if matches!(node.inner.borrow().node_type, DomNodeType::Element { .. }) {
                return Some(node);
            }
            curr = node.previous_sibling();
        }
        None
    }

    pub fn next_element_sibling(&self) -> Option<DomNode> {
        let mut curr = self.next_sibling();
        while let Some(node) = curr {
            if matches!(node.inner.borrow().node_type, DomNodeType::Element { .. }) {
                return Some(node);
            }
            curr = node.next_sibling();
        }
        None
    }

    pub fn child_element_count(&self) -> usize {
        let inner = self.inner.borrow();
        inner
            .children
            .iter()
            .filter(|c| matches!(c.inner.borrow().node_type, DomNodeType::Element { .. }))
            .count()
    }

    pub fn child_nodes(&self) -> Vec<DomNode> {
        self.children()
    }

    pub fn append_child(&self, child: DomNode) -> DomNode {
        child.remove();
        child.inner.borrow_mut().parent = Some(Rc::downgrade(&self.inner));
        self.inner.borrow_mut().children.push(child.clone());
        child
    }

    pub fn prepend_child(&self, child: DomNode) -> DomNode {
        child.remove();
        child.inner.borrow_mut().parent = Some(Rc::downgrade(&self.inner));
        self.inner.borrow_mut().children.insert(0, child.clone());
        child
    }

    pub fn insert_before(
        &self,
        new_child: DomNode,
        reference_child: Option<&DomNode>,
    ) -> Result<DomNode, String> {
        match reference_child {
            None => Ok(self.append_child(new_child)),
            Some(ref_node) => {
                let inner = self.inner.borrow();
                let idx = inner
                    .children
                    .iter()
                    .position(|c| Rc::ptr_eq(&c.inner, &ref_node.inner))
                    .ok_or_else(|| {
                        "NotFoundError: reference node is not a child of this node".to_string()
                    })?;
                drop(inner);
                new_child.remove();
                let mut inner = self.inner.borrow_mut();
                new_child.inner.borrow_mut().parent = Some(Rc::downgrade(&self.inner));
                inner.children.insert(idx, new_child.clone());
                Ok(new_child)
            }
        }
    }

    pub fn replace_child(
        &self,
        new_child: DomNode,
        old_child: &DomNode,
    ) -> Result<DomNode, String> {
        let inner = self.inner.borrow();
        let idx = inner
            .children
            .iter()
            .position(|c| Rc::ptr_eq(&c.inner, &old_child.inner))
            .ok_or_else(|| {
                "NotFoundError: node to be replaced is not a child of this node".to_string()
            })?;
        drop(inner);
        new_child.remove();
        let mut inner = self.inner.borrow_mut();
        old_child.inner.borrow_mut().parent = None;
        new_child.inner.borrow_mut().parent = Some(Rc::downgrade(&self.inner));
        inner.children[idx] = new_child;
        Ok(old_child.clone())
    }

    pub fn remove_child(&self, child: &DomNode) -> bool {
        let mut inner = self.inner.borrow_mut();
        let before_len = inner.children.len();
        if let Some(pos) = inner.children.iter().position(|c| Rc::ptr_eq(&c.inner, &child.inner)) {
            inner.children.remove(pos);
            child.inner.borrow_mut().parent = None;
        }
        inner.children.len() < before_len
    }

    pub fn remove(&self) -> bool {
        if let Some(parent) = self.parent_node() {
            parent.remove_child(self)
        } else {
            false
        }
    }

    pub fn replace_with(&self, new_node: DomNode) -> bool {
        if let Some(parent) = self.parent_node() {
            parent.replace_child(new_node, self).is_ok()
        } else {
            false
        }
    }

    pub fn contains(&self, other: &DomNode) -> bool {
        if Rc::ptr_eq(&self.inner, &other.inner) {
            return true;
        }
        for child in self.children() {
            if child.contains(other) {
                return true;
            }
        }
        false
    }

    pub fn clone_node(&self, deep: bool) -> DomNode {
        let inner = self.inner.borrow();
        let cloned_inner = DomNodeInner {
            uid: NEXT_DOM_UID.fetch_add(1, Ordering::Relaxed),
            node_type: inner.node_type.clone(),
            parent: None,
            children: if deep {
                inner.children.iter().map(|c| c.clone_node(true)).collect()
            } else {
                Vec::new()
            },
            event_listeners: HashMap::new(),
        };
        let node = DomNode {
            inner: Rc::new(RefCell::new(cloned_inner)),
        };
        if deep {
            for child in &node.inner.borrow().children {
                child.inner.borrow_mut().parent = Some(Rc::downgrade(&node.inner));
            }
        }
        node
    }

    pub fn children(&self) -> Vec<DomNode> {
        self.inner.borrow().children.clone()
    }

    pub fn get_element_by_id(&self, id: &str) -> Option<DomNode> {
        let id_lower = id.to_ascii_lowercase();
        if let Some(this_id) = self.id() {
            if this_id.to_ascii_lowercase() == id_lower {
                return Some(self.clone());
            }
        }
        let children = self.children();
        for child in children {
            if let Some(found) = child.get_element_by_id(id) {
                return Some(found);
            }
        }
        None
    }

    pub fn query_selector(&self, selector: &str) -> Option<DomNode> {
        let all = self.query_selector_all(selector);
        all.into_iter().next()
    }

    pub fn query_selector_all(&self, selector: &str) -> Vec<DomNode> {
        let mut results = Vec::new();
        self.collect_matching(selector.trim(), &mut results);
        results
    }

    fn collect_matching(&self, selector: &str, results: &mut Vec<DomNode>) {
        if self.matches_simple_selector(selector) {
            results.push(self.clone());
        }
        for child in self.children() {
            child.collect_matching(selector, results);
        }
    }

    fn matches_simple_selector(&self, selector: &str) -> bool {
        if selector.is_empty() {
            return false;
        }
        if let Some(id) = selector.strip_prefix('#') {
            return self.id().map(|v| v == id).unwrap_or(false);
        }
        if let Some(class) = selector.strip_prefix('.') {
            return self
                .class_name()
                .split_whitespace()
                .any(|c| c == class);
        }
        if let Some(tag) = self.tag_name() {
            if tag.eq_ignore_ascii_case(selector) {
                return true;
            }
        }
        false
    }

    pub fn add_event_listener(&self, event: &str, callback: JsValue) {
        let mut inner = self.inner.borrow_mut();
        inner
            .event_listeners
            .entry(event.to_ascii_lowercase())
            .or_default()
            .push(callback);
    }

    pub fn dispatch_event(&self, event: &str, engine: &mut JsEngine) {
        let listeners = {
            let inner = self.inner.borrow();
            inner
                .event_listeners
                .get(&event.to_ascii_lowercase())
                .cloned()
                .unwrap_or_default()
        };

        for listener in listeners {
            if let JsValue::Object(map) = &listener {
                if let Some(JsValue::String(fn_name)) = map.get("__fn__") {
                    let _ = engine.call_function(fn_name, vec![]);
                }
            }
        }
    }
}

fn is_void_element(tag: &str) -> bool {
    matches!(
        tag.to_ascii_lowercase().as_str(),
        "area" | "base" | "br" | "col" | "embed" | "hr" | "img" | "input" | "link" | "meta" | "param" | "source" | "track" | "wbr"
    )
}

/// Parses a basic HTML snippet into a vector of DOM nodes.
pub fn parse_html_fragment(html: &str) -> Vec<DomNode> {
    let mut nodes = Vec::new();
    let bytes = html.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'<' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                // Unexpected closing tag, advance past '>'
                while i < bytes.len() && bytes[i] != b'>' {
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1;
                }
                continue;
            }

            // Find tag end
            let start_tag = i + 1;
            while i < bytes.len() && bytes[i] != b'>' {
                i += 1;
            }
            if i >= bytes.len() {
                break;
            }
            let tag_content = std::str::from_utf8(&bytes[start_tag..i]).unwrap_or("");
            i += 1; // skip '>'

            let is_self_closing = tag_content.ends_with('/');
            let tag_str = tag_content.trim_end_matches('/').trim();

            let mut parts = tag_str.split_whitespace();
            let tag_name = parts.next().unwrap_or("div").to_ascii_lowercase();

            let mut attributes = HashMap::new();
            for part in parts {
                if let Some((k, v)) = part.split_once('=') {
                    let val_clean = v.trim_matches('"').trim_matches('\'');
                    attributes.insert(k.to_ascii_lowercase(), val_clean.to_string());
                } else if !part.is_empty() {
                    attributes.insert(part.to_ascii_lowercase(), String::new());
                }
            }

            let elem = DomNode::element(&tag_name, attributes);

            if !is_self_closing && !is_void_element(&tag_name) {
                // Find closing tag `</tag_name>`
                let close_tag = format!("</{}>", tag_name);
                let remaining = &html[i..];
                if let Some(close_pos) = remaining.to_ascii_lowercase().find(&close_tag) {
                    let inner_str = &remaining[..close_pos];
                    let children = parse_html_fragment(inner_str);
                    for child in &children {
                        child.inner.borrow_mut().parent = Some(Rc::downgrade(&elem.inner));
                    }
                    elem.inner.borrow_mut().children = children;
                    i += close_pos + close_tag.len();
                }
            }

            nodes.push(elem);
        } else {
            // Text node
            let text_start = i;
            while i < bytes.len() && bytes[i] != b'<' {
                i += 1;
            }
            let text_str = std::str::from_utf8(&bytes[text_start..i]).unwrap_or("");
            if !text_str.is_empty() {
                nodes.push(DomNode::text(text_str));
            }
        }
    }

    nodes
}

// ============================================================================
// 2. Document Model
// ============================================================================

/// W3C-compliant document node container holding root, head, body, and DOM elements.
#[derive(Clone)]
pub struct Document {
    pub html: DomNode,
    pub head: DomNode,
    pub body: DomNode,
    pub title: Rc<RefCell<String>>,
    pub mutated: Rc<RefCell<bool>>,
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

impl Document {
    pub fn new() -> Self {
        let html = DomNode::element("html", HashMap::new());
        let head = DomNode::element("head", HashMap::new());
        let body = DomNode::element("body", HashMap::new());

        html.append_child(head.clone());
        html.append_child(body.clone());

        Self {
            html,
            head,
            body,
            title: Rc::new(RefCell::new("Strata Document".to_string())),
            mutated: Rc::new(RefCell::new(false)),
        }
    }

    pub fn mark_mutated(&self) {
        *self.mutated.borrow_mut() = true;
    }

    pub fn has_mutated(&self) -> bool {
        *self.mutated.borrow()
    }

    pub fn from_html(html_str: &str) -> Self {
        let doc = Self::new();
        let parsed = parse_html_fragment(html_str);

        // Check if root has <html> or <body>
        for node in parsed {
            if let Some(tag) = node.tag_name() {
                match tag.as_str() {
                    "HTML" => {
                        for child in node.children() {
                            if let Some(c_tag) = child.tag_name() {
                                if c_tag == "HEAD" {
                                    for c in child.children() {
                                        doc.head.append_child(c);
                                    }
                                } else if c_tag == "BODY" {
                                    for c in child.children() {
                                        doc.body.append_child(c);
                                    }
                                }
                            }
                        }
                    }
                    "HEAD" => {
                        for c in node.children() {
                            doc.head.append_child(c);
                        }
                    }
                    "BODY" => {
                        for c in node.children() {
                            doc.body.append_child(c);
                        }
                    }
                    _ => {
                        doc.body.append_child(node);
                    }
                }
            } else {
                doc.body.append_child(node);
            }
        }

        doc
    }

    pub fn create_element(&self, tag: &str) -> DomNode {
        DomNode::element(tag, HashMap::new())
    }

    pub fn create_text_node(&self, text: &str) -> DomNode {
        DomNode::text(text)
    }

    pub fn create_comment(&self, comment: &str) -> DomNode {
        DomNode::comment(comment)
    }

    pub fn get_element_by_id(&self, id: &str) -> Option<DomNode> {
        self.html.get_element_by_id(id)
    }

    pub fn get_element_by_uid(&self, uid: u64) -> Option<DomNode> {
        self.find_by_uid(&self.html, uid)
    }

    fn find_by_uid(&self, current: &DomNode, target_uid: u64) -> Option<DomNode> {
        if current.uid() == target_uid {
            return Some(current.clone());
        }
        for child in current.children() {
            if let Some(found) = self.find_by_uid(&child, target_uid) {
                return Some(found);
            }
        }
        None
    }

    pub fn query_selector(&self, selector: &str) -> Option<DomNode> {
        self.html.query_selector(selector)
    }

    pub fn query_selector_all(&self, selector: &str) -> Vec<DomNode> {
        self.html.query_selector_all(selector)
    }

    pub fn to_html(&self) -> String {
        self.html.outer_html()
    }
}

// ============================================================================
// 3. JavaScript AST & Statements
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    StrictEq,
    NotEq,
    StrictNotEq,
    Lt,
    Lte,
    Gt,
    Gte,
    And,
    Or,
    InstanceOf,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Not,
    Neg,
    Typeof,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TemplatePart {
    Literal(String),
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ArrowBody {
    Expr(Box<Expr>),
    Block(Vec<Stmt>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(JsValue),
    Identifier(String),
    Assign {
        target: Box<Expr>,
        value: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Member {
        object: Box<Expr>,
        property: String,
        computed: Option<Box<Expr>>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    New {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Array(Vec<Expr>),
    Object(Vec<(String, Expr)>),
    TemplateLiteral(Vec<TemplatePart>),
    ArrowFunction {
        params: Vec<String>,
        body: ArrowBody,
        is_async: bool,
    },
    Await(Box<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum StatementFlow {
    Normal(JsValue),
    Return(JsValue),
    Throw(JsValue),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassDef {
    pub name: String,
    pub super_class: Option<String>,
    pub constructor: Option<(Vec<String>, Vec<Stmt>)>,
    pub methods: HashMap<String, (Vec<String>, Vec<Stmt>)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    VarDecl {
        name: String,
        init: Option<Expr>,
    },
    Assign {
        target: Expr,
        value: Expr,
    },
    Expr(Expr),
    Block(Vec<Stmt>),
    If {
        cond: Expr,
        then_branch: Box<Stmt>,
        else_branch: Option<Box<Stmt>>,
    },
    While {
        cond: Expr,
        body: Box<Stmt>,
    },
    For {
        init: Option<Box<Stmt>>,
        cond: Option<Expr>,
        step: Option<Expr>,
        body: Box<Stmt>,
    },
    FunctionDecl {
        name: String,
        params: Vec<String>,
        body: Vec<Stmt>,
        is_async: bool,
    },
    ClassDecl {
        name: String,
        super_class: Option<String>,
        constructor: Option<(Vec<String>, Vec<Stmt>)>,
        methods: Vec<(String, Vec<String>, Vec<Stmt>)>,
    },
    Return(Option<Expr>),
    TryCatch {
        try_block: Vec<Stmt>,
        catch_param: Option<String>,
        catch_block: Option<Vec<Stmt>>,
        finally_block: Option<Vec<Stmt>>,
    },
    Throw(Expr),
}

// ============================================================================
// 4. JavaScript Parser (Recursive Descent)
// ============================================================================

pub struct JsParser {
    chars: Vec<char>,
    pos: usize,
}

impl JsParser {
    pub fn new(src: &str) -> Self {
        Self {
            chars: src.chars().collect(),
            pos: 0,
        }
    }

    pub fn parse_program(&mut self) -> Result<Vec<Stmt>, String> {
        let mut stmts = Vec::new();
        self.skip_whitespace_and_comments();
        while !self.is_eof() {
            let stmt = self.parse_statement()?;
            stmts.push(stmt);
            self.skip_whitespace_and_comments();
        }
        Ok(stmts)
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.chars.len()
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.peek();
        if ch.is_some() {
            self.pos += 1;
        }
        ch
    }

    fn skip_whitespace_and_comments(&mut self) {
        while !self.is_eof() {
            let ch = self.peek().unwrap();
            if ch.is_whitespace() {
                self.advance();
            } else if ch == '/' && self.pos + 1 < self.chars.len() {
                let next = self.chars[self.pos + 1];
                if next == '/' {
                    // Line comment
                    self.pos += 2;
                    while !self.is_eof() && self.peek() != Some('\n') {
                        self.advance();
                    }
                } else if next == '*' {
                    // Block comment
                    self.pos += 2;
                    while self.pos + 1 < self.chars.len() {
                        if self.chars[self.pos] == '*' && self.chars[self.pos + 1] == '/' {
                            self.pos += 2;
                            break;
                        }
                        self.pos += 1;
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }

    fn consume_semicolon(&mut self) {
        self.skip_whitespace_and_comments();
        if self.peek() == Some(';') {
            self.advance();
        }
    }

    fn parse_statement(&mut self) -> Result<Stmt, String> {
        self.skip_whitespace_and_comments();
        if self.is_eof() {
            return Err("Unexpected EOF while parsing statement".to_string());
        }

        if self.peek() == Some('{') {
            return self.parse_block();
        }

        let ident = self.peek_identifier();
        if let Some(ref kw) = ident {
            match kw.as_str() {
                "var" | "let" | "const" => return self.parse_var_decl(),
                "if" => return self.parse_if(),
                "while" => return self.parse_while(),
                "for" => return self.parse_for(),
                "function" => return self.parse_function_decl(false),
                "async" => {
                    let saved_pos = self.pos;
                    let _ = self.read_identifier();
                    self.skip_whitespace_and_comments();
                    if self.peek_identifier().as_deref() == Some("function") {
                        return self.parse_function_decl(true);
                    }
                    self.pos = saved_pos;
                }
                "class" => return self.parse_class_decl(),
                "return" => return self.parse_return(),
                "try" => return self.parse_try_catch(),
                "throw" => return self.parse_throw(),
                _ => {}
            }
        }

        // Expression statement (including assignment expressions)
        let expr = self.parse_expression()?;
        self.consume_semicolon();
        Ok(Stmt::Expr(expr))
    }

    fn parse_block_stmts(&mut self) -> Result<Vec<Stmt>, String> {
        self.skip_whitespace_and_comments();
        self.expect_char('{')?;
        let mut stmts = Vec::new();
        self.skip_whitespace_and_comments();
        while !self.is_eof() && self.peek() != Some('}') {
            let stmt = self.parse_statement()?;
            stmts.push(stmt);
            self.skip_whitespace_and_comments();
        }
        self.expect_char('}')?;
        Ok(stmts)
    }

    fn parse_block(&mut self) -> Result<Stmt, String> {
        let stmts = self.parse_block_stmts()?;
        Ok(Stmt::Block(stmts))
    }

    fn parse_throw(&mut self) -> Result<Stmt, String> {
        self.read_identifier()?; // "throw"
        self.skip_whitespace_and_comments();
        let expr = self.parse_expression()?;
        self.consume_semicolon();
        Ok(Stmt::Throw(expr))
    }

    fn parse_try_catch(&mut self) -> Result<Stmt, String> {
        self.read_identifier()?; // "try"
        let try_block = self.parse_block_stmts()?;
        self.skip_whitespace_and_comments();

        let mut catch_param = None;
        let mut catch_block = None;
        if self.peek_identifier().as_deref() == Some("catch") {
            self.read_identifier()?; // "catch"
            self.skip_whitespace_and_comments();
            if self.peek() == Some('(') {
                self.advance();
                self.skip_whitespace_and_comments();
                let param = self.read_identifier()?;
                self.skip_whitespace_and_comments();
                self.expect_char(')')?;
                catch_param = Some(param);
            }
            catch_block = Some(self.parse_block_stmts()?);
        }

        self.skip_whitespace_and_comments();
        let mut finally_block = None;
        if self.peek_identifier().as_deref() == Some("finally") {
            self.read_identifier()?; // "finally"
            finally_block = Some(self.parse_block_stmts()?);
        }

        if catch_block.is_none() && finally_block.is_none() {
            return Err("SyntaxError: Missing catch or finally clause after try".to_string());
        }

        Ok(Stmt::TryCatch {
            try_block,
            catch_param,
            catch_block,
            finally_block,
        })
    }

    fn parse_var_decl(&mut self) -> Result<Stmt, String> {
        self.read_identifier()?; // var/let/const
        self.skip_whitespace_and_comments();
        let name = self.read_identifier()?;
        self.skip_whitespace_and_comments();

        let mut init = None;
        if self.peek() == Some('=') {
            self.advance();
            init = Some(self.parse_expression()?);
        }
        self.consume_semicolon();
        Ok(Stmt::VarDecl { name, init })
    }

    fn parse_if(&mut self) -> Result<Stmt, String> {
        self.read_identifier()?; // if
        self.skip_whitespace_and_comments();
        self.expect_char('(')?;
        let cond = self.parse_expression()?;
        self.expect_char(')')?;
        let then_branch = Box::new(self.parse_statement()?);

        self.skip_whitespace_and_comments();
        let mut else_branch = None;
        if self.peek_identifier().as_deref() == Some("else") {
            self.read_identifier()?; // else
            else_branch = Some(Box::new(self.parse_statement()?));
        }

        Ok(Stmt::If {
            cond,
            then_branch,
            else_branch,
        })
    }

    fn parse_while(&mut self) -> Result<Stmt, String> {
        self.read_identifier()?; // while
        self.skip_whitespace_and_comments();
        self.expect_char('(')?;
        let cond = self.parse_expression()?;
        self.expect_char(')')?;
        let body = Box::new(self.parse_statement()?);
        Ok(Stmt::While { cond, body })
    }

    fn parse_for(&mut self) -> Result<Stmt, String> {
        self.read_identifier()?; // for
        self.skip_whitespace_and_comments();
        self.expect_char('(')?;

        let init = if self.peek() == Some(';') {
            self.advance();
            None
        } else {
            Some(Box::new(self.parse_statement()?))
        };

        self.skip_whitespace_and_comments();
        let cond = if self.peek() == Some(';') {
            None
        } else {
            Some(self.parse_expression()?)
        };
        self.consume_semicolon();

        self.skip_whitespace_and_comments();
        let step = if self.peek() == Some(')') {
            None
        } else {
            Some(self.parse_expression()?)
        };
        self.expect_char(')')?;

        let body = Box::new(self.parse_statement()?);
        Ok(Stmt::For {
            init,
            cond,
            step,
            body,
        })
    }

    fn parse_function_decl(&mut self, is_async: bool) -> Result<Stmt, String> {
        self.read_identifier()?; // function
        self.skip_whitespace_and_comments();
        let name = self.read_identifier()?;
        self.skip_whitespace_and_comments();
        self.expect_char('(')?;

        let mut params = Vec::new();
        self.skip_whitespace_and_comments();
        while !self.is_eof() && self.peek() != Some(')') {
            let param = self.read_identifier()?;
            params.push(param);
            self.skip_whitespace_and_comments();
            if self.peek() == Some(',') {
                self.advance();
                self.skip_whitespace_and_comments();
            }
        }
        self.expect_char(')')?;

        let block = self.parse_block()?;
        let body = match block {
            Stmt::Block(stmts) => stmts,
            other => vec![other],
        };

        Ok(Stmt::FunctionDecl {
            name,
            params,
            body,
            is_async,
        })
    }

    fn parse_class_decl(&mut self) -> Result<Stmt, String> {
        self.read_identifier()?; // class
        self.skip_whitespace_and_comments();
        let name = self.read_identifier()?;
        self.skip_whitespace_and_comments();
        let mut super_class = None;
        if self.peek_identifier().as_deref() == Some("extends") {
            self.read_identifier()?; // extends
            self.skip_whitespace_and_comments();
            super_class = Some(self.read_identifier()?);
            self.skip_whitespace_and_comments();
        }
        self.expect_char('{')?;
        self.skip_whitespace_and_comments();

        let mut constructor = None;
        let mut methods = Vec::new();

        while !self.is_eof() && self.peek() != Some('}') {
            self.skip_whitespace_and_comments();
            if self.peek() == Some(';') {
                self.advance();
                continue;
            }
            if self.peek() == Some('}') {
                break;
            }

            let method_name = self.read_identifier()?;
            self.skip_whitespace_and_comments();
            self.expect_char('(')?;
            let mut params = Vec::new();
            self.skip_whitespace_and_comments();
            while !self.is_eof() && self.peek() != Some(')') {
                let p = self.read_identifier()?;
                params.push(p);
                self.skip_whitespace_and_comments();
                if self.peek() == Some(',') {
                    self.advance();
                    self.skip_whitespace_and_comments();
                }
            }
            self.expect_char(')')?;

            let body_block = self.parse_block()?;
            let body = match body_block {
                Stmt::Block(stmts) => stmts,
                other => vec![other],
            };

            if method_name == "constructor" {
                constructor = Some((params, body));
            } else {
                methods.push((method_name, params, body));
            }

            self.skip_whitespace_and_comments();
            if self.peek() == Some(';') {
                self.advance();
            }
            self.skip_whitespace_and_comments();
        }

        self.expect_char('}')?;
        self.consume_semicolon();

        Ok(Stmt::ClassDecl {
            name,
            super_class,
            constructor,
            methods,
        })
    }

    fn parse_return(&mut self) -> Result<Stmt, String> {
        self.read_identifier()?; // return
        self.skip_whitespace_and_comments();
        let mut expr = None;
        if self.peek() != Some(';') && self.peek() != Some('}') {
            expr = Some(self.parse_expression()?);
        }
        self.consume_semicolon();
        Ok(Stmt::Return(expr))
    }

    // Expressions
    pub fn parse_expression(&mut self) -> Result<Expr, String> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<Expr, String> {
        let left = self.parse_logical_or()?;
        self.skip_whitespace_and_comments();
        if self.peek() == Some('=') && self.pos + 1 < self.chars.len() && self.chars[self.pos + 1] != '=' {
            self.advance(); // consume '='
            let right = self.parse_assignment()?;
            return Ok(Expr::Assign {
                target: Box::new(left),
                value: Box::new(right),
            });
        }
        Ok(left)
    }

    fn parse_logical_or(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_logical_and()?;
        self.skip_whitespace_and_comments();
        while self.pos + 1 < self.chars.len()
            && self.chars[self.pos] == '|'
            && self.chars[self.pos + 1] == '|'
        {
            self.pos += 2;
            let right = self.parse_logical_and()?;
            left = Expr::Binary {
                op: BinaryOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            };
            self.skip_whitespace_and_comments();
        }
        Ok(left)
    }

    fn parse_logical_and(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_equality()?;
        self.skip_whitespace_and_comments();
        while self.pos + 1 < self.chars.len()
            && self.chars[self.pos] == '&'
            && self.chars[self.pos + 1] == '&'
        {
            self.pos += 2;
            let right = self.parse_equality()?;
            left = Expr::Binary {
                op: BinaryOp::And,
                left: Box::new(left),
                right: Box::new(right),
            };
            self.skip_whitespace_and_comments();
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_relational()?;
        self.skip_whitespace_and_comments();

        while !self.is_eof() {
            if self.pos + 2 < self.chars.len()
                && self.chars[self.pos] == '='
                && self.chars[self.pos + 1] == '='
                && self.chars[self.pos + 2] == '='
            {
                self.pos += 3;
                let right = self.parse_relational()?;
                left = Expr::Binary {
                    op: BinaryOp::StrictEq,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else if self.pos + 2 < self.chars.len()
                && self.chars[self.pos] == '!'
                && self.chars[self.pos + 1] == '='
                && self.chars[self.pos + 2] == '='
            {
                self.pos += 3;
                let right = self.parse_relational()?;
                left = Expr::Binary {
                    op: BinaryOp::StrictNotEq,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else if self.pos + 1 < self.chars.len()
                && self.chars[self.pos] == '='
                && self.chars[self.pos + 1] == '='
            {
                self.pos += 2;
                let right = self.parse_relational()?;
                left = Expr::Binary {
                    op: BinaryOp::Eq,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else if self.pos + 1 < self.chars.len()
                && self.chars[self.pos] == '!'
                && self.chars[self.pos + 1] == '='
            {
                self.pos += 2;
                let right = self.parse_relational()?;
                left = Expr::Binary {
                    op: BinaryOp::NotEq,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
            self.skip_whitespace_and_comments();
        }
        Ok(left)
    }

    fn parse_relational(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_additive()?;
        self.skip_whitespace_and_comments();

        while !self.is_eof() {
            if self.pos + 1 < self.chars.len()
                && self.chars[self.pos] == '<'
                && self.chars[self.pos + 1] == '='
            {
                self.pos += 2;
                let right = self.parse_additive()?;
                left = Expr::Binary {
                    op: BinaryOp::Lte,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else if self.pos + 1 < self.chars.len()
                && self.chars[self.pos] == '>'
                && self.chars[self.pos + 1] == '='
            {
                self.pos += 2;
                let right = self.parse_additive()?;
                left = Expr::Binary {
                    op: BinaryOp::Gte,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else if self.peek() == Some('<') {
                self.advance();
                let right = self.parse_additive()?;
                left = Expr::Binary {
                    op: BinaryOp::Lt,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else if self.peek() == Some('>') {
                self.advance();
                let right = self.parse_additive()?;
                left = Expr::Binary {
                    op: BinaryOp::Gt,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else if self.peek_identifier().as_deref() == Some("instanceof") {
                self.read_identifier()?;
                let right = self.parse_additive()?;
                left = Expr::Binary {
                    op: BinaryOp::InstanceOf,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
            self.skip_whitespace_and_comments();
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_multiplicative()?;
        self.skip_whitespace_and_comments();

        while !self.is_eof() {
            if self.peek() == Some('+') {
                self.advance();
                let right = self.parse_multiplicative()?;
                left = Expr::Binary {
                    op: BinaryOp::Add,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else if self.peek() == Some('-') {
                self.advance();
                let right = self.parse_multiplicative()?;
                left = Expr::Binary {
                    op: BinaryOp::Sub,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
            self.skip_whitespace_and_comments();
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_unary()?;
        self.skip_whitespace_and_comments();

        while !self.is_eof() {
            if self.peek() == Some('*') {
                self.advance();
                let right = self.parse_unary()?;
                left = Expr::Binary {
                    op: BinaryOp::Mul,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else if self.peek() == Some('/') {
                self.advance();
                let right = self.parse_unary()?;
                left = Expr::Binary {
                    op: BinaryOp::Div,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else if self.peek() == Some('%') {
                self.advance();
                let right = self.parse_unary()?;
                left = Expr::Binary {
                    op: BinaryOp::Mod,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
            self.skip_whitespace_and_comments();
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        self.skip_whitespace_and_comments();
        if self.peek() == Some('!') {
            self.advance();
            let expr = self.parse_unary()?;
            return Ok(Expr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(expr),
            });
        }
        if self.peek() == Some('-') {
            self.advance();
            let expr = self.parse_unary()?;
            return Ok(Expr::Unary {
                op: UnaryOp::Neg,
                expr: Box::new(expr),
            });
        }
        if self.peek_identifier().as_deref() == Some("typeof") {
            self.read_identifier()?;
            let expr = self.parse_unary()?;
            return Ok(Expr::Unary {
                op: UnaryOp::Typeof,
                expr: Box::new(expr),
            });
        }
        if self.peek_identifier().as_deref() == Some("await") {
            self.read_identifier()?;
            let expr = self.parse_unary()?;
            return Ok(Expr::Await(Box::new(expr)));
        }

        self.parse_call_and_member()
    }

    fn parse_call_and_member(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_primary()?;

        loop {
            self.skip_whitespace_and_comments();
            if self.peek() == Some('.') {
                self.advance(); // '.'
                self.skip_whitespace_and_comments();
                let prop = self.read_identifier()?;
                expr = Expr::Member {
                    object: Box::new(expr),
                    property: prop,
                    computed: None,
                };
            } else if self.peek() == Some('[') {
                self.advance(); // '['
                let prop_expr = self.parse_expression()?;
                self.expect_char(']')?;
                if let Expr::Literal(JsValue::String(s)) = prop_expr {
                    expr = Expr::Member {
                        object: Box::new(expr),
                        property: s,
                        computed: None,
                    };
                } else {
                    expr = Expr::Member {
                        object: Box::new(expr),
                        property: String::new(),
                        computed: Some(Box::new(prop_expr)),
                    };
                }
            } else if self.peek() == Some('(') {
                self.advance(); // '('
                let mut args = Vec::new();
                self.skip_whitespace_and_comments();
                while !self.is_eof() && self.peek() != Some(')') {
                    let arg = self.parse_expression()?;
                    args.push(arg);
                    self.skip_whitespace_and_comments();
                    if self.peek() == Some(',') {
                        self.advance();
                        self.skip_whitespace_and_comments();
                    }
                }
                self.expect_char(')')?;
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
                };
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_arrow_body(&mut self, params: Vec<String>, is_async: bool) -> Result<Expr, String> {
        self.skip_whitespace_and_comments();
        if self.peek() == Some('{') {
            let body = self.parse_block_stmts()?;
            Ok(Expr::ArrowFunction {
                params,
                body: ArrowBody::Block(body),
                is_async,
            })
        } else {
            let expr = self.parse_assignment()?;
            Ok(Expr::ArrowFunction {
                params,
                body: ArrowBody::Expr(Box::new(expr)),
                is_async,
            })
        }
    }

    fn parse_template_literal(&mut self) -> Result<Expr, String> {
        self.advance(); // consume opening '`'
        let mut parts = Vec::new();
        let mut current_lit = String::new();
        while !self.is_eof() {
            let c = self.advance().unwrap();
            if c == '`' {
                if !current_lit.is_empty() {
                    parts.push(TemplatePart::Literal(current_lit));
                }
                return Ok(Expr::TemplateLiteral(parts));
            }
            if c == '$' && self.peek() == Some('{') {
                self.advance(); // consume '{'
                if !current_lit.is_empty() {
                    parts.push(TemplatePart::Literal(current_lit));
                    current_lit = String::new();
                }
                self.skip_whitespace_and_comments();
                let expr = self.parse_expression()?;
                self.skip_whitespace_and_comments();
                self.expect_char('}')?;
                parts.push(TemplatePart::Expr(expr));
            } else if c == '\\' && !self.is_eof() {
                let escaped = self.advance().unwrap();
                match escaped {
                    'n' => current_lit.push('\n'),
                    't' => current_lit.push('\t'),
                    'r' => current_lit.push('\r'),
                    '`' => current_lit.push('`'),
                    '$' => current_lit.push('$'),
                    '\\' => current_lit.push('\\'),
                    other => {
                        current_lit.push('\\');
                        current_lit.push(other);
                    }
                }
            } else {
                current_lit.push(c);
            }
        }
        Err("SyntaxError: Unterminated template literal".to_string())
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        self.skip_whitespace_and_comments();
        if self.is_eof() {
            return Err("Unexpected EOF in primary expression".to_string());
        }

        let ch = self.peek().unwrap();

        // Template literal
        if ch == '`' {
            return self.parse_template_literal();
        }

        // Grouping `(expr)` or Arrow function `(a, b) => ...` / `() => ...`
        if ch == '(' {
            let saved_pos = self.pos;
            self.advance(); // '('
            self.skip_whitespace_and_comments();
            if self.peek() == Some(')') {
                self.advance(); // ')'
                self.skip_whitespace_and_comments();
                if self.peek() == Some('=') && self.pos + 1 < self.chars.len() && self.chars[self.pos + 1] == '>' {
                    self.pos += 2; // consume '=>'
                    return self.parse_arrow_body(Vec::new(), false);
                }
            } else {
                let mut params = Vec::new();
                let mut is_params = true;
                while !self.is_eof() && self.peek() != Some(')') {
                    if let Ok(param_name) = self.read_identifier() {
                        params.push(param_name);
                        self.skip_whitespace_and_comments();
                        if self.peek() == Some(',') {
                            self.advance();
                            self.skip_whitespace_and_comments();
                        } else if self.peek() != Some(')') {
                            is_params = false;
                            break;
                        }
                    } else {
                        is_params = false;
                        break;
                    }
                }
                if is_params && self.peek() == Some(')') {
                    self.advance(); // ')'
                    self.skip_whitespace_and_comments();
                    if self.peek() == Some('=') && self.pos + 1 < self.chars.len() && self.chars[self.pos + 1] == '>' {
                        self.pos += 2; // consume '=>'
                        return self.parse_arrow_body(params, false);
                    }
                }
            }
            // Not an arrow function, rewind and parse standard grouped expression
            self.pos = saved_pos;
            self.advance(); // '('
            let expr = self.parse_expression()?;
            self.expect_char(')')?;
            return Ok(expr);
        }

        // Array literal `[...]`
        if ch == '[' {
            self.advance();
            let mut items = Vec::new();
            self.skip_whitespace_and_comments();
            while !self.is_eof() && self.peek() != Some(']') {
                let item = self.parse_expression()?;
                items.push(item);
                self.skip_whitespace_and_comments();
                if self.peek() == Some(',') {
                    self.advance();
                    self.skip_whitespace_and_comments();
                }
            }
            self.expect_char(']')?;
            return Ok(Expr::Array(items));
        }

        // Object literal `{...}`
        if ch == '{' {
            self.advance();
            let mut pairs = Vec::new();
            self.skip_whitespace_and_comments();
            while !self.is_eof() && self.peek() != Some('}') {
                let key = if self.peek() == Some('"') || self.peek() == Some('\'') {
                    self.read_string()?
                } else {
                    self.read_identifier()?
                };
                self.skip_whitespace_and_comments();
                self.expect_char(':')?;
                let val = self.parse_expression()?;
                pairs.push((key, val));
                self.skip_whitespace_and_comments();
                if self.peek() == Some(',') {
                    self.advance();
                    self.skip_whitespace_and_comments();
                }
            }
            self.expect_char('}')?;
            return Ok(Expr::Object(pairs));
        }

        // String literals
        if ch == '"' || ch == '\'' {
            let s = self.read_string()?;
            return Ok(Expr::Literal(JsValue::String(s)));
        }

        // Number literals
        if ch.is_ascii_digit() {
            let n = self.read_number()?;
            return Ok(Expr::Literal(JsValue::Number(n)));
        }

        // Identifiers and Keywords
        let ident = self.read_identifier()?;
        if ident == "async" {
            self.skip_whitespace_and_comments();
            if self.peek_identifier().as_deref() == Some("function") {
                self.read_identifier()?; // function
                self.skip_whitespace_and_comments();
                let _name = if self.peek() != Some('(') {
                    self.read_identifier().ok()
                } else {
                    None
                };
                self.skip_whitespace_and_comments();
                self.expect_char('(')?;
                let mut params = Vec::new();
                self.skip_whitespace_and_comments();
                while !self.is_eof() && self.peek() != Some(')') {
                    let p = self.read_identifier()?;
                    params.push(p);
                    self.skip_whitespace_and_comments();
                    if self.peek() == Some(',') {
                        self.advance();
                        self.skip_whitespace_and_comments();
                    }
                }
                self.expect_char(')')?;
                let body = self.parse_block_stmts()?;
                return Ok(Expr::ArrowFunction {
                    params,
                    body: ArrowBody::Block(body),
                    is_async: true,
                });
            } else if self.peek() == Some('(') {
                let saved_pos = self.pos;
                self.advance(); // '('
                self.skip_whitespace_and_comments();
                if self.peek() == Some(')') {
                    self.advance(); // ')'
                    self.skip_whitespace_and_comments();
                    if self.peek() == Some('=') && self.pos + 1 < self.chars.len() && self.chars[self.pos + 1] == '>' {
                        self.pos += 2; // consume '=>'
                        return self.parse_arrow_body(Vec::new(), true);
                    }
                } else {
                    let mut params = Vec::new();
                    let mut is_params = true;
                    while !self.is_eof() && self.peek() != Some(')') {
                        if let Ok(param_name) = self.read_identifier() {
                            params.push(param_name);
                            self.skip_whitespace_and_comments();
                            if self.peek() == Some(',') {
                                self.advance();
                                self.skip_whitespace_and_comments();
                            } else if self.peek() != Some(')') {
                                is_params = false;
                                break;
                            }
                        } else {
                            is_params = false;
                            break;
                        }
                    }
                    if is_params && self.peek() == Some(')') {
                        self.advance(); // ')'
                        self.skip_whitespace_and_comments();
                        if self.peek() == Some('=') && self.pos + 1 < self.chars.len() && self.chars[self.pos + 1] == '>' {
                            self.pos += 2; // consume '=>'
                            return self.parse_arrow_body(params, true);
                        }
                    }
                }
                self.pos = saved_pos;
            } else if let Some(param) = self.peek_identifier() {
                let saved_pos = self.pos;
                let _ = self.read_identifier();
                self.skip_whitespace_and_comments();
                if self.peek() == Some('=') && self.pos + 1 < self.chars.len() && self.chars[self.pos + 1] == '>' {
                    self.pos += 2; // consume '=>'
                    return self.parse_arrow_body(vec![param], true);
                }
                self.pos = saved_pos;
            }
        }

        if ident == "function" {
            self.skip_whitespace_and_comments();
            let _name = if self.peek() != Some('(') {
                self.read_identifier().ok()
            } else {
                None
            };
            self.skip_whitespace_and_comments();
            self.expect_char('(')?;
            let mut params = Vec::new();
            self.skip_whitespace_and_comments();
            while !self.is_eof() && self.peek() != Some(')') {
                let p = self.read_identifier()?;
                params.push(p);
                self.skip_whitespace_and_comments();
                if self.peek() == Some(',') {
                    self.advance();
                    self.skip_whitespace_and_comments();
                }
            }
            self.expect_char(')')?;
            let body = self.parse_block_stmts()?;
            return Ok(Expr::ArrowFunction {
                params,
                body: ArrowBody::Block(body),
                is_async: false,
            });
        }

        if ident == "new" {
            self.skip_whitespace_and_comments();
            let mut callee = self.parse_primary()?;
            loop {
                self.skip_whitespace_and_comments();
                if self.peek() == Some('.') {
                    self.advance();
                    self.skip_whitespace_and_comments();
                    let prop = self.read_identifier()?;
                    callee = Expr::Member {
                        object: Box::new(callee),
                        property: prop,
                        computed: None,
                    };
                } else if self.peek() == Some('[') {
                    self.advance();
                    let prop_expr = self.parse_expression()?;
                    self.expect_char(']')?;
                    callee = Expr::Member {
                        object: Box::new(callee),
                        property: String::new(),
                        computed: Some(Box::new(prop_expr)),
                    };
                } else {
                    break;
                }
            }
            self.skip_whitespace_and_comments();
            let mut args = Vec::new();
            if self.peek() == Some('(') {
                self.advance();
                self.skip_whitespace_and_comments();
                while !self.is_eof() && self.peek() != Some(')') {
                    let arg = self.parse_expression()?;
                    args.push(arg);
                    self.skip_whitespace_and_comments();
                    if self.peek() == Some(',') {
                        self.advance();
                        self.skip_whitespace_and_comments();
                    }
                }
                self.expect_char(')')?;
            }
            return Ok(Expr::New {
                callee: Box::new(callee),
                args,
            });
        }

        self.skip_whitespace_and_comments();
        if self.peek() == Some('=') && self.pos + 1 < self.chars.len() && self.chars[self.pos + 1] == '>' {
            self.pos += 2; // consume '=>'
            return self.parse_arrow_body(vec![ident], false);
        }

        match ident.as_str() {
            "true" => Ok(Expr::Literal(JsValue::Bool(true))),
            "false" => Ok(Expr::Literal(JsValue::Bool(false))),
            "null" => Ok(Expr::Literal(JsValue::Null)),
            "undefined" => Ok(Expr::Literal(JsValue::Undefined)),
            _ => Ok(Expr::Identifier(ident)),
        }
    }

    fn read_string(&mut self) -> Result<String, String> {
        let quote = self.advance().unwrap();
        let mut out = String::new();
        while !self.is_eof() {
            let ch = self.advance().unwrap();
            if ch == quote {
                return Ok(out);
            }
            if ch == '\\' && !self.is_eof() {
                let escaped = self.advance().unwrap();
                match escaped {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    '\\' => out.push('\\'),
                    '\'' => out.push('\''),
                    '"' => out.push('"'),
                    _ => out.push(escaped),
                }
            } else {
                out.push(ch);
            }
        }
        Err("Unterminated string literal".to_string())
    }

    fn read_number(&mut self) -> Result<f64, String> {
        let start = self.pos;
        while !self.is_eof() {
            let ch = self.peek().unwrap();
            if ch.is_ascii_digit() || ch == '.' {
                self.advance();
            } else {
                break;
            }
        }
        let num_str: String = self.chars[start..self.pos].iter().collect();
        num_str
            .parse::<f64>()
            .map_err(|e| format!("Invalid number: {e}"))
    }

    fn read_identifier(&mut self) -> Result<String, String> {
        self.skip_whitespace_and_comments();
        let start = self.pos;
        if self.is_eof() {
            return Err("Unexpected EOF while reading identifier".to_string());
        }
        let first = self.peek().unwrap();
        if !first.is_alphabetic() && first != '_' && first != '$' {
            return Err(format!("Expected identifier, found '{first}'"));
        }
        self.advance();

        while !self.is_eof() {
            let ch = self.peek().unwrap();
            if ch.is_alphanumeric() || ch == '_' || ch == '$' {
                self.advance();
            } else {
                break;
            }
        }
        Ok(self.chars[start..self.pos].iter().collect())
    }

    fn peek_identifier(&self) -> Option<String> {
        let mut i = self.pos;
        while i < self.chars.len() && self.chars[i].is_whitespace() {
            i += 1;
        }
        if i >= self.chars.len() {
            return None;
        }
        let first = self.chars[i];
        if !first.is_alphabetic() && first != '_' && first != '$' {
            return None;
        }
        let start = i;
        i += 1;
        while i < self.chars.len() && (self.chars[i].is_alphanumeric() || self.chars[i] == '_' || self.chars[i] == '$') {
            i += 1;
        }
        Some(self.chars[start..i].iter().collect())
    }

    fn expect_char(&mut self, expected: char) -> Result<(), String> {
        self.skip_whitespace_and_comments();
        if self.peek() == Some(expected) {
            self.advance();
            Ok(())
        } else {
            Err(format!(
                "Expected '{}', found '{:?}'",
                expected,
                self.peek()
            ))
        }
    }
}

// ============================================================================
// 5. JavaScript Scope Chain, Closures & Runtime Evaluator
// ============================================================================

/// Lexical scope environment frame supporting lexical nesting and closures.
#[derive(Debug, Clone, PartialEq)]
pub struct Scope {
    pub parent: Option<Rc<RefCell<Scope>>>,
    pub bindings: HashMap<String, JsValue>,
}

impl Scope {
    pub fn new_root() -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            parent: None,
            bindings: HashMap::new(),
        }))
    }

    pub fn new_child(parent: Rc<RefCell<Self>>) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            parent: Some(parent),
            bindings: HashMap::new(),
        }))
    }

    pub fn get(&self, name: &str) -> Option<JsValue> {
        if let Some(val) = self.bindings.get(name) {
            return Some(val.clone());
        }
        if let Some(ref parent) = self.parent {
            return parent.borrow().get(name);
        }
        None
    }

    pub fn set(&mut self, name: &str, val: JsValue) -> bool {
        if self.bindings.contains_key(name) {
            self.bindings.insert(name.to_string(), val);
            return true;
        }
        if let Some(ref parent) = self.parent {
            return parent.borrow_mut().set(name, val);
        }
        false
    }

    pub fn declare(&mut self, name: &str, val: JsValue) {
        self.bindings.insert(name.to_string(), val);
    }
}

/// Represents an executable JavaScript function closure capturing its lexical environment.
#[derive(Clone)]
pub struct JsClosure {
    pub params: Vec<String>,
    pub body: Vec<Stmt>,
    pub env: Rc<RefCell<Scope>>,
    pub is_async: bool,
}

pub struct JsEngine {
    pub globals: BTreeMap<String, JsValue>,
    pub root_scope: Rc<RefCell<Scope>>,
    pub current_scope: Rc<RefCell<Scope>>,
    pub document: Document,
    pub console_logs: Vec<String>,
    pub user_functions: HashMap<String, JsClosure>,
    pub live_nodes: RefCell<HashMap<u64, DomNode>>,
    pub anon_counter: usize,
    pub classes: HashMap<String, ClassDef>,
    pub promises: HashMap<u64, JsPromise<JsValue>>,
    pub promise_controllers: HashMap<u64, (ResolveFn<JsValue>, RejectFn)>,
    pub next_promise_id: u64,
    pub pending_throw: Option<JsValue>,
}

impl Default for JsEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl JsEngine {
    pub fn new() -> Self {
        Self::with_document(Document::new())
    }

    pub fn with_document(doc: Document) -> Self {
        let mut globals = BTreeMap::new();
        globals.insert("window".to_string(), JsValue::Object(BTreeMap::new()));
        globals.insert("document".to_string(), JsValue::Object(BTreeMap::new()));
        globals.insert("console".to_string(), JsValue::Object(BTreeMap::new()));
        let mut math_obj = BTreeMap::new();
        math_obj.insert("PI".to_string(), JsValue::Number(std::f64::consts::PI));
        math_obj.insert("E".to_string(), JsValue::Number(std::f64::consts::E));
        globals.insert("Math".to_string(), JsValue::Object(math_obj));
        globals.insert("JSON".to_string(), JsValue::Object(BTreeMap::new()));

        let mut promise_obj = BTreeMap::new();
        promise_obj.insert("__is_class__".to_string(), JsValue::Bool(true));
        promise_obj.insert("__class_name__".to_string(), JsValue::String("Promise".to_string()));
        globals.insert("Promise".to_string(), JsValue::Object(promise_obj));

        let root_scope = Scope::new_root();
        for (k, v) in &globals {
            root_scope.borrow_mut().declare(k, v.clone());
        }

        Self {
            globals,
            root_scope: root_scope.clone(),
            current_scope: root_scope,
            document: doc,
            console_logs: Vec::new(),
            user_functions: HashMap::new(),
            live_nodes: RefCell::new(HashMap::new()),
            anon_counter: 0,
            classes: HashMap::new(),
            promises: HashMap::new(),
            promise_controllers: HashMap::new(),
            next_promise_id: 1,
            pending_throw: None,
        }
    }

    pub fn create_promise_value(&mut self, promise: JsPromise<JsValue>) -> JsValue {
        let pid = self.next_promise_id;
        self.next_promise_id += 1;
        self.promises.insert(pid, promise);
        let mut map = BTreeMap::new();
        map.insert("__is_promise__".to_string(), JsValue::Bool(true));
        map.insert("__promise_id__".to_string(), JsValue::Number(pid as f64));
        JsValue::Object(map)
    }

    pub fn resolve_constructor(&self, class_name: &str) -> Option<(Vec<String>, Vec<Stmt>, Option<String>)> {
        let cls = self.classes.get(class_name)?;
        if let Some(ref ctor) = cls.constructor {
            Some((ctor.0.clone(), ctor.1.clone(), cls.super_class.clone()))
        } else if let Some(ref super_name) = cls.super_class {
            self.resolve_constructor(super_name)
        } else {
            None
        }
    }

    pub fn resolve_method(&self, class_name: &str, method_name: &str) -> Option<(Vec<String>, Vec<Stmt>, Option<String>)> {
        let mut curr_class = Some(class_name.to_string());
        while let Some(cls_name) = curr_class {
            if let Some(cls) = self.classes.get(&cls_name) {
                if let Some(m) = cls.methods.get(method_name) {
                    return Some((m.0.clone(), m.1.clone(), cls.super_class.clone()));
                }
                curr_class = cls.super_class.clone();
            } else {
                break;
            }
        }
        None
    }

    pub fn is_instance_of(&self, instance: &JsValue, class_name: &str) -> bool {
        if class_name == "Object" {
            return matches!(instance, JsValue::Object(_) | JsValue::Array(_));
        }
        if class_name == "Array" {
            return matches!(instance, JsValue::Array(_));
        }
        if let JsValue::Object(map) = instance {
            if let Some(JsValue::String(inst_class)) = map.get("__class__") {
                let mut curr = Some(inst_class.clone());
                while let Some(c) = curr {
                    if c == class_name {
                        return true;
                    }
                    curr = self.classes.get(&c).and_then(|cd| cd.super_class.clone());
                }
            }
        }
        false
    }

    pub fn instantiate_class(&mut self, class_name: &str, args: Vec<JsValue>) -> Result<JsValue, String> {
        let _cls = self.classes.get(class_name).cloned().ok_or_else(|| {
            format!("ReferenceError: {class_name} is not defined")
        })?;

        let mut instance_map = BTreeMap::new();
        instance_map.insert("__class__".to_string(), JsValue::String(class_name.to_string()));
        let instance_obj = JsValue::Object(instance_map);

        if let Some((params, body, super_opt)) = self.resolve_constructor(class_name) {
            let ctor_scope = Scope::new_child(self.current_scope.clone());
            ctor_scope.borrow_mut().declare("this", instance_obj);
            if let Some(super_name) = super_opt {
                ctor_scope.borrow_mut().declare("__super_class__", JsValue::String(super_name));
            }
            ctor_scope.borrow_mut().declare("__current_class__", JsValue::String(class_name.to_string()));

            for (p, a) in params.iter().zip(args) {
                ctor_scope.borrow_mut().declare(p, a);
            }

            let saved_scope = self.current_scope.clone();
            self.current_scope = ctor_scope.clone();

            let mut ret_val = None;
            let mut err = None;
            for stmt in &body {
                match self.execute_stmt_flow(stmt) {
                    Ok(StatementFlow::Normal(_)) => {}
                    Ok(StatementFlow::Return(v)) => {
                        if let JsValue::Object(_) = v {
                            ret_val = Some(v);
                        }
                        break;
                    }
                    Ok(StatementFlow::Throw(v)) => {
                        err = Some(format!("Uncaught exception: {v}"));
                        break;
                    }
                    Err(e) => {
                        err = Some(e);
                        break;
                    }
                }
            }

            self.current_scope = saved_scope;
            if let Some(e) = err {
                return Err(e);
            }

            if let Some(ret) = ret_val {
                return Ok(ret);
            }

            let final_this = ctor_scope.borrow().get("this").unwrap_or(JsValue::Undefined);
            Ok(final_this)
        } else {
            Ok(instance_obj)
        }
    }

    pub fn eval(&mut self, script: &str) -> Result<JsValue, String> {
        let mut parser = JsParser::new(script);
        let stmts = parser.parse_program()?;

        for (k, v) in &self.globals {
            self.root_scope.borrow_mut().declare(k, v.clone());
        }
        self.current_scope = self.root_scope.clone();

        let mut result = JsValue::Undefined;
        for stmt in &stmts {
            match self.execute_stmt_flow(stmt)? {
                StatementFlow::Normal(v) => result = v,
                StatementFlow::Return(v) => {
                    result = v;
                    break;
                }
                StatementFlow::Throw(err) => {
                    return Err(format!("Uncaught exception: {err}"));
                }
            }
        }

        for (k, v) in &self.root_scope.borrow().bindings {
            self.globals.insert(k.clone(), v.clone());
        }

        Ok(result)
    }

    pub fn execute_statement(&mut self, stmt: &Stmt) -> Result<JsValue, String> {
        match self.execute_stmt_flow(stmt)? {
            StatementFlow::Normal(v) | StatementFlow::Return(v) => Ok(v),
            StatementFlow::Throw(err) => Err(format!("Uncaught exception: {err}")),
        }
    }

    fn execute_block_flow(&mut self, stmts: &[Stmt]) -> Result<StatementFlow, String> {
        let mut res = JsValue::Undefined;
        for s in stmts {
            match self.execute_stmt_flow(s)? {
                StatementFlow::Normal(v) => res = v,
                early => return Ok(early),
            }
        }
        Ok(StatementFlow::Normal(res))
    }

    fn execute_stmt_flow(&mut self, stmt: &Stmt) -> Result<StatementFlow, String> {
        match stmt {
            Stmt::VarDecl { name, init } => {
                let val = match init {
                    Some(expr) => self.eval_expr(expr)?,
                    None => JsValue::Undefined,
                };
                if let Some(err) = self.pending_throw.take() {
                    return Ok(StatementFlow::Throw(err));
                }
                self.current_scope.borrow_mut().declare(name, val.clone());
                if self.current_scope.borrow().parent.is_none() {
                    self.globals.insert(name.clone(), val);
                }
                Ok(StatementFlow::Normal(JsValue::Undefined))
            }
            Stmt::Assign { target, value } => {
                let val = self.eval_expr(value)?;
                if let Some(err) = self.pending_throw.take() {
                    return Ok(StatementFlow::Throw(err));
                }
                self.assign_target(target, val.clone())?;
                Ok(StatementFlow::Normal(val))
            }
            Stmt::Expr(expr) => {
                let val = self.eval_expr(expr)?;
                if let Some(err) = self.pending_throw.take() {
                    return Ok(StatementFlow::Throw(err));
                }
                Ok(StatementFlow::Normal(val))
            }
            Stmt::Block(stmts) => self.execute_block_flow(stmts),
            Stmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let c = self.eval_expr(cond)?;
                if c.is_truthy() {
                    self.execute_stmt_flow(then_branch)
                } else if let Some(e) = else_branch {
                    self.execute_stmt_flow(e)
                } else {
                    Ok(StatementFlow::Normal(JsValue::Undefined))
                }
            }
            Stmt::While { cond, body } => {
                let mut res = JsValue::Undefined;
                while self.eval_expr(cond)?.is_truthy() {
                    match self.execute_stmt_flow(body)? {
                        StatementFlow::Normal(v) => res = v,
                        early => return Ok(early),
                    }
                }
                Ok(StatementFlow::Normal(res))
            }
            Stmt::For {
                init,
                cond,
                step,
                body,
            } => {
                if let Some(i) = init {
                    match self.execute_stmt_flow(i)? {
                        StatementFlow::Normal(_) => {}
                        early => return Ok(early),
                    }
                }
                let mut res = JsValue::Undefined;
                loop {
                    if let Some(c) = cond {
                        if !self.eval_expr(c)?.is_truthy() {
                            break;
                        }
                    }
                    match self.execute_stmt_flow(body)? {
                        StatementFlow::Normal(v) => res = v,
                        early => return Ok(early),
                    }
                    if let Some(s) = step {
                        self.eval_expr(s)?;
                    }
                }
                Ok(StatementFlow::Normal(res))
            }
            Stmt::FunctionDecl {
                name,
                params,
                body,
                is_async,
            } => {
                let closure = JsClosure {
                    params: params.clone(),
                    body: body.clone(),
                    env: self.current_scope.clone(),
                    is_async: *is_async,
                };
                self.user_functions.insert(name.clone(), closure);
                let mut obj = BTreeMap::new();
                obj.insert("__fn__".to_string(), JsValue::String(name.clone()));
                let fn_val = JsValue::Object(obj);
                self.current_scope.borrow_mut().declare(name, fn_val.clone());
                if self.current_scope.borrow().parent.is_none() {
                    self.globals.insert(name.clone(), fn_val);
                }
                Ok(StatementFlow::Normal(JsValue::Undefined))
            }
            Stmt::ClassDecl {
                name,
                super_class,
                constructor,
                methods,
            } => {
                let mut methods_map = HashMap::new();
                for (m_name, params, body) in methods {
                    methods_map.insert(m_name.clone(), (params.clone(), body.clone()));
                }
                self.classes.insert(name.clone(), ClassDef {
                    name: name.clone(),
                    super_class: super_class.clone(),
                    constructor: constructor.clone(),
                    methods: methods_map,
                });
                let mut class_meta = BTreeMap::new();
                class_meta.insert("__class_name__".to_string(), JsValue::String(name.clone()));
                class_meta.insert("__is_class__".to_string(), JsValue::Bool(true));
                let class_val = JsValue::Object(class_meta);
                self.current_scope.borrow_mut().declare(name, class_val.clone());
                if self.current_scope.borrow().parent.is_none() {
                    self.globals.insert(name.clone(), class_val);
                }
                Ok(StatementFlow::Normal(JsValue::Undefined))
            }
            Stmt::Return(expr_opt) => {
                let val = match expr_opt {
                    Some(e) => self.eval_expr(e)?,
                    None => JsValue::Undefined,
                };
                if let Some(err) = self.pending_throw.take() {
                    return Ok(StatementFlow::Throw(err));
                }
                Ok(StatementFlow::Return(val))
            }
            Stmt::Throw(expr) => {
                let val = self.eval_expr(expr)?;
                Ok(StatementFlow::Throw(val))
            }
            Stmt::TryCatch {
                try_block,
                catch_param,
                catch_block,
                finally_block,
            } => {
                let mut flow = self.execute_block_flow(try_block)?;
                if let StatementFlow::Throw(ref thrown_val) = flow {
                    let thrown_val = thrown_val.clone();
                    if let Some(catch_stmts) = catch_block {
                        let catch_scope = Scope::new_child(self.current_scope.clone());
                        if let Some(ref param_name) = catch_param {
                            catch_scope.borrow_mut().declare(param_name, thrown_val);
                        }
                        let saved_scope = self.current_scope.clone();
                        self.current_scope = catch_scope;
                        let catch_flow = self.execute_block_flow(catch_stmts);
                        self.current_scope = saved_scope;
                        flow = catch_flow?;
                    }
                }
                if let Some(finally_stmts) = finally_block {
                    let finally_flow = self.execute_block_flow(finally_stmts)?;
                    if let StatementFlow::Return(_) | StatementFlow::Throw(_) = finally_flow {
                        flow = finally_flow;
                    }
                }
                Ok(flow)
            }
        }
    }

    fn assign_target(&mut self, target: &Expr, val: JsValue) -> Result<(), String> {
        match target {
            Expr::Identifier(name) => {
                if !self.current_scope.borrow_mut().set(name, val.clone()) {
                    self.current_scope.borrow_mut().declare(name, val.clone());
                }
                if self.current_scope.borrow().parent.is_none() {
                    self.globals.insert(name.clone(), val);
                }
                Ok(())
            }
            Expr::Member {
                object,
                property,
                computed,
            } => {
                if let Some(comp_expr) = computed {
                    let prop_val = self.eval_expr(comp_expr)?;
                    if let Expr::Identifier(ref id) = &**object {
                        let existing = self.current_scope.borrow().get(id);
                        if let Some(JsValue::Array(mut arr)) = existing {
                            if let Some(idx) = prop_val.as_f64() {
                                let u_idx = idx as usize;
                                if u_idx >= arr.len() {
                                    arr.resize(u_idx + 1, JsValue::Undefined);
                                }
                                arr[u_idx] = val.clone();
                                self.current_scope.borrow_mut().set(id, JsValue::Array(arr.clone()));
                                if self.current_scope.borrow().parent.is_none() {
                                    self.globals.insert(id.clone(), JsValue::Array(arr));
                                }
                                return Ok(());
                            }
                        } else if let Some(JsValue::Object(mut map)) = existing {
                            let key = match prop_val {
                                JsValue::String(s) => s,
                                JsValue::Number(n) => format!("{n}"),
                                other => other.to_string(),
                            };
                            map.insert(key, val.clone());
                            self.current_scope.borrow_mut().set(id, JsValue::Object(map.clone()));
                            if self.current_scope.borrow().parent.is_none() {
                                self.globals.insert(id.clone(), JsValue::Object(map));
                            }
                            return Ok(());
                        }
                    }
                    return Ok(());
                }

                // Check if target is document or DOM element
                if let Expr::Member {
                    object: parent_obj,
                    property: parent_prop,
                    ..
                } = &**object
                {
                    if let Expr::Identifier(id) = &**parent_obj {
                        if id == "document" && parent_prop == "body" {
                            if property == "innerHTML" {
                                if let Some(s) = val.as_str() {
                                    self.document.body.set_inner_html(s);
                                    self.document.mark_mutated();
                                }
                                return Ok(());
                            } else if property == "textContent" {
                                if let Some(s) = val.as_str() {
                                    self.document.body.set_text_content(s);
                                    self.document.mark_mutated();
                                }
                                return Ok(());
                            }
                        }
                    }
                }

                if let Expr::Identifier(id) = &**object {
                    if id == "document" && property == "title" {
                        if let Some(s) = val.as_str() {
                            *self.document.title.borrow_mut() = s.to_string();
                            self.document.mark_mutated();
                        }
                        return Ok(());
                    }
                }

                let obj = self.eval_expr(object)?;
                if let JsValue::Object(ref map) = obj {
                    if let Some(node) = self.resolve_dom_node(map) {
                        if property == "innerHTML" {
                            if let Some(s) = val.as_str() {
                                node.set_inner_html(s);
                                self.document.mark_mutated();
                            }
                            return Ok(());
                        } else if property == "textContent" {
                            if let Some(s) = val.as_str() {
                                node.set_text_content(s);
                                self.document.mark_mutated();
                            }
                            return Ok(());
                        } else if property == "className" {
                            if let Some(s) = val.as_str() {
                                node.set_class_name(s);
                                self.document.mark_mutated();
                            }
                            return Ok(());
                        } else if property == "id" {
                            if let Some(s) = val.as_str() {
                                node.set_attribute("id", s);
                                self.document.mark_mutated();
                            }
                            return Ok(());
                        }
                    }
                }

                if let Expr::Identifier(ref id) = &**object {
                    let existing = self.current_scope.borrow().get(id);
                    if let Some(JsValue::Object(mut map)) = existing {
                        map.insert(property.clone(), val.clone());
                        self.current_scope.borrow_mut().set(id, JsValue::Object(map.clone()));
                        if self.current_scope.borrow().parent.is_none() {
                            self.globals.insert(id.clone(), JsValue::Object(map));
                        }
                        return Ok(());
                    }
                }

                if let JsValue::Object(mut map) = obj {
                    map.insert(property.clone(), val);
                }
                Ok(())
            }
            _ => Err("Invalid assignment target".to_string()),
        }
    }

    pub fn call_value(&mut self, callee: &JsValue, args: Vec<JsValue>) -> Result<JsValue, String> {
        match callee {
            JsValue::String(name) => self.call_function(name, args),
            JsValue::Object(map) => {
                if let Some(JsValue::String(name)) = map.get("__fn__") {
                    self.call_function(name, args)
                } else {
                    Ok(JsValue::Undefined)
                }
            }
            _ => Ok(JsValue::Undefined),
        }
    }

    pub fn eval_expr(&mut self, expr: &Expr) -> Result<JsValue, String> {
        match expr {
            Expr::Literal(val) => Ok(val.clone()),
            Expr::Assign { target, value } => {
                let val = self.eval_expr(value)?;
                self.assign_target(target, val.clone())?;
                Ok(val)
            }
            Expr::Identifier(name) => {
                if let Some(val) = self.current_scope.borrow().get(name) {
                    return Ok(val);
                }
                if let Some(val) = self.globals.get(name) {
                    return Ok(val.clone());
                }
                match name.as_str() {
                    "undefined" => Ok(JsValue::Undefined),
                    "null" => Ok(JsValue::Null),
                    "document" => Ok(JsValue::Object(BTreeMap::new())),
                    "window" => Ok(JsValue::Object(BTreeMap::new())),
                    _ => Ok(JsValue::Undefined),
                }
            }
            Expr::Binary { op, left, right } => {
                if let BinaryOp::InstanceOf = op {
                    let l_val = self.eval_expr(left)?;
                    let class_name = match &**right {
                        Expr::Identifier(ref id) => id.clone(),
                        Expr::Literal(JsValue::String(ref s)) => s.clone(),
                        other => {
                            let rv = self.eval_expr(other)?;
                            if let JsValue::Object(map) = rv {
                                map.get("__class_name__")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string()
                            } else {
                                String::new()
                            }
                        }
                    };
                    return Ok(JsValue::Bool(self.is_instance_of(&l_val, &class_name)));
                }
                let l_val = self.eval_expr(left)?;
                let r_val = self.eval_expr(right)?;
                self.eval_binary(op, &l_val, &r_val)
            }
            Expr::Unary { op, expr } => {
                let val = self.eval_expr(expr)?;
                match op {
                    UnaryOp::Not => Ok(JsValue::Bool(!val.is_truthy())),
                    UnaryOp::Neg => {
                        let n = val.as_f64().unwrap_or(0.0);
                        Ok(JsValue::Number(-n))
                    }
                    UnaryOp::Typeof => Ok(JsValue::String(val.js_typeof().to_string())),
                }
            }
            Expr::TemplateLiteral(parts) => {
                let mut out = String::new();
                for part in parts {
                    match part {
                        TemplatePart::Literal(s) => out.push_str(s),
                        TemplatePart::Expr(e) => {
                            let v = self.eval_expr(e)?;
                            match v {
                                JsValue::String(s) => out.push_str(&s),
                                JsValue::Number(n) => out.push_str(&format!("{n}")),
                                JsValue::Bool(b) => out.push_str(&format!("{b}")),
                                JsValue::Null => out.push_str("null"),
                                JsValue::Undefined => out.push_str("undefined"),
                                other => out.push_str(&stringify_json(&other)),
                            }
                        }
                    }
                }
                Ok(JsValue::String(out))
            }
            Expr::ArrowFunction {
                params,
                body,
                is_async,
            } => {
                let fn_name = format!("__anon_fn_{}", self.anon_counter);
                self.anon_counter += 1;
                let stmts = match body {
                    ArrowBody::Expr(e) => vec![Stmt::Return(Some((**e).clone()))],
                    ArrowBody::Block(s) => s.clone(),
                };
                let closure = JsClosure {
                    params: params.clone(),
                    body: stmts,
                    env: self.current_scope.clone(),
                    is_async: *is_async,
                };
                self.user_functions.insert(fn_name.clone(), closure);
                let mut obj = BTreeMap::new();
                obj.insert("__fn__".to_string(), JsValue::String(fn_name));
                Ok(JsValue::Object(obj))
            }
            Expr::Await(inner) => {
                let val = self.eval_expr(inner)?;
                drain_microtasks();
                if let JsValue::Object(ref map) = val {
                    if map.get("__is_promise__") == Some(&JsValue::Bool(true)) {
                        if let Some(JsValue::Number(pid_num)) = map.get("__promise_id__") {
                            let pid = *pid_num as u64;
                            if let Some(promise) = self.promises.get(&pid).cloned() {
                                match promise.state() {
                                    PromiseState::Fulfilled(res) => {
                                        return Ok(res);
                                    }
                                    PromiseState::Rejected(err) => {
                                        self.pending_throw = Some(err);
                                        return Ok(JsValue::Undefined);
                                    }
                                    PromiseState::Pending => {
                                        drain_microtasks();
                                        match promise.state() {
                                            PromiseState::Fulfilled(res) => return Ok(res),
                                            PromiseState::Rejected(err) => {
                                                self.pending_throw = Some(err);
                                                return Ok(JsValue::Undefined);
                                            }
                                            PromiseState::Pending => return Ok(val),
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(val)
            }
            Expr::Member {
                object,
                property,
                computed,
            } => {
                let obj_val = self.eval_expr(object)?;
                if let Some(comp_expr) = computed {
                    let prop_val = self.eval_expr(comp_expr)?;
                    if let JsValue::Array(ref arr) = obj_val {
                        if let Some(idx) = prop_val.as_f64() {
                            return Ok(arr.get(idx as usize).cloned().unwrap_or(JsValue::Undefined));
                        }
                    }
                    if let JsValue::Object(ref map) = obj_val {
                        let key = match prop_val {
                            JsValue::String(s) => s,
                            JsValue::Number(n) => format!("{n}"),
                            other => other.to_string(),
                        };
                        return Ok(map.get(&key).cloned().unwrap_or(JsValue::Undefined));
                    }
                    if let JsValue::String(ref s) = obj_val {
                        if let Some(idx) = prop_val.as_f64() {
                            return Ok(s.chars().nth(idx as usize).map(|c| JsValue::String(c.to_string())).unwrap_or(JsValue::Undefined));
                        }
                    }
                    return Ok(JsValue::Undefined);
                }

                if let Expr::Identifier(id) = &**object {
                    if id == "document" {
                        match property.as_str() {
                            "body" => {
                                let mut map = BTreeMap::new();
                                map.insert("__is_body__".to_string(), JsValue::Bool(true));
                                map.insert(
                                    "innerHTML".to_string(),
                                    JsValue::String(self.document.body.inner_html()),
                                );
                                map.insert(
                                    "textContent".to_string(),
                                    JsValue::String(self.document.body.text_content()),
                                );
                                return Ok(JsValue::Object(map));
                            }
                            "title" => {
                                return Ok(JsValue::String(self.document.title.borrow().clone()));
                            }
                            _ => {}
                        }
                    }
                }

                if property == "length" {
                    match &obj_val {
                        JsValue::Array(arr) => return Ok(JsValue::Number(arr.len() as f64)),
                        JsValue::String(s) => return Ok(JsValue::Number(s.len() as f64)),
                        _ => {}
                    }
                }

                if let JsValue::Object(ref map) = obj_val {
                    if let Some(node) = self.resolve_dom_node(map) {
                        match property.as_str() {
                            "parentNode" => {
                                return Ok(node.parent_node().map(|p| self.dom_node_to_js(&p)).unwrap_or(JsValue::Null));
                            }
                            "parentElement" => {
                                return Ok(node.parent_element().map(|p| self.dom_node_to_js(&p)).unwrap_or(JsValue::Null));
                            }
                            "firstChild" => {
                                return Ok(node.first_child().map(|c| self.dom_node_to_js(&c)).unwrap_or(JsValue::Null));
                            }
                            "lastChild" => {
                                return Ok(node.last_child().map(|c| self.dom_node_to_js(&c)).unwrap_or(JsValue::Null));
                            }
                            "firstElementChild" => {
                                return Ok(node.first_element_child().map(|c| self.dom_node_to_js(&c)).unwrap_or(JsValue::Null));
                            }
                            "lastElementChild" => {
                                return Ok(node.last_element_child().map(|c| self.dom_node_to_js(&c)).unwrap_or(JsValue::Null));
                            }
                            "previousSibling" => {
                                return Ok(node.previous_sibling().map(|s| self.dom_node_to_js(&s)).unwrap_or(JsValue::Null));
                            }
                            "nextSibling" => {
                                return Ok(node.next_sibling().map(|s| self.dom_node_to_js(&s)).unwrap_or(JsValue::Null));
                            }
                            "previousElementSibling" => {
                                return Ok(node.previous_element_sibling().map(|s| self.dom_node_to_js(&s)).unwrap_or(JsValue::Null));
                            }
                            "nextElementSibling" => {
                                return Ok(node.next_element_sibling().map(|s| self.dom_node_to_js(&s)).unwrap_or(JsValue::Null));
                            }
                            "childNodes" => {
                                let children_js = node.children().iter().map(|c| self.dom_node_to_js(c)).collect();
                                return Ok(JsValue::Array(children_js));
                            }
                            "children" => {
                                let children_js = node
                                    .children()
                                    .iter()
                                    .filter(|c| matches!(c.inner.borrow().node_type, DomNodeType::Element { .. }))
                                    .map(|c| self.dom_node_to_js(c))
                                    .collect();
                                return Ok(JsValue::Array(children_js));
                            }
                            "childElementCount" => {
                                return Ok(JsValue::Number(node.child_element_count() as f64));
                            }
                            "innerHTML" => {
                                return Ok(JsValue::String(node.inner_html()));
                            }
                            "outerHTML" => {
                                return Ok(JsValue::String(node.outer_html()));
                            }
                            "textContent" => {
                                return Ok(JsValue::String(node.text_content()));
                            }
                            "className" => {
                                return Ok(JsValue::String(node.class_name()));
                            }
                            "tagName" => {
                                return Ok(JsValue::String(node.tag_name().unwrap_or_default()));
                            }
                            "id" => {
                                return Ok(JsValue::String(node.id().unwrap_or_default()));
                            }
                            _ => {}
                        }
                    }
                    return Ok(map.get(property).cloned().unwrap_or(JsValue::Undefined));
                }
                Ok(JsValue::Undefined)
            }
            Expr::Call { callee, args } => {
                let mut evaluated_args = Vec::new();
                for a in args {
                    evaluated_args.push(self.eval_expr(a)?);
                }

                if let Expr::Identifier(id) = &**callee {
                    if id == "super" {
                        let super_class_opt = self.current_scope.borrow().get("__super_class__");
                        if let Some(JsValue::String(super_name)) = super_class_opt {
                            let super_ctor = self.resolve_constructor(&super_name);
                            if let Some((params, body, next_super)) = super_ctor {
                                let current_this = self.current_scope.borrow().get("this").unwrap_or(JsValue::Undefined);
                                let super_scope = Scope::new_child(self.current_scope.clone());
                                super_scope.borrow_mut().declare("this", current_this);
                                if let Some(ns) = next_super {
                                    super_scope.borrow_mut().declare("__super_class__", JsValue::String(ns));
                                }
                                super_scope.borrow_mut().declare("__current_class__", JsValue::String(super_name));

                                for (p, a) in params.iter().zip(evaluated_args) {
                                    super_scope.borrow_mut().declare(p, a);
                                }

                                let saved_scope = self.current_scope.clone();
                                self.current_scope = super_scope.clone();

                                let mut err = None;
                                for stmt in &body {
                                    match self.execute_stmt_flow(stmt) {
                                        Ok(StatementFlow::Normal(_)) => {}
                                        Ok(StatementFlow::Return(_)) => break,
                                        Ok(StatementFlow::Throw(v)) => {
                                            err = Some(format!("Uncaught exception: {v}"));
                                            break;
                                        }
                                        Err(e) => {
                                            err = Some(e);
                                            break;
                                        }
                                    }
                                }

                                self.current_scope = saved_scope;
                                if let Some(e) = err {
                                    return Err(e);
                                }

                                if let Some(updated_this) = super_scope.borrow().get("this") {
                                    self.current_scope.borrow_mut().set("this", updated_this);
                                }
                                return Ok(JsValue::Undefined);
                            }
                        }
                    }
                }

                if let Expr::Member { object, property, computed } = &**callee {
                    if computed.is_none() {
                        if let Expr::Identifier(id) = &**object {
                            if id == "super" {
                                let current_class_opt = self.current_scope.borrow().get("__current_class__");
                                if let Some(JsValue::String(curr_cls)) = current_class_opt {
                                    let super_class_opt = self.classes.get(&curr_cls).and_then(|c| c.super_class.clone());
                                    if let Some(super_name) = super_class_opt {
                                        if let Some((params, body, _)) = self.resolve_method(&super_name, property) {
                                            let current_this = self.current_scope.borrow().get("this").unwrap_or(JsValue::Undefined);
                                            let super_scope = Scope::new_child(self.current_scope.clone());
                                            super_scope.borrow_mut().declare("this", current_this);
                                            super_scope.borrow_mut().declare("__current_class__", JsValue::String(super_name));

                                            for (p, a) in params.iter().zip(evaluated_args) {
                                                super_scope.borrow_mut().declare(p, a);
                                            }

                                            let saved_scope = self.current_scope.clone();
                                            self.current_scope = super_scope.clone();

                                            let mut ret = JsValue::Undefined;
                                            let mut err = None;
                                            for stmt in &body {
                                                match self.execute_stmt_flow(stmt) {
                                                    Ok(StatementFlow::Normal(v)) => ret = v,
                                                    Ok(StatementFlow::Return(v)) => {
                                                        ret = v;
                                                        break;
                                                    }
                                                    Ok(StatementFlow::Throw(v)) => {
                                                        err = Some(format!("Uncaught exception: {v}"));
                                                        break;
                                                    }
                                                    Err(e) => {
                                                        err = Some(e);
                                                        break;
                                                    }
                                                }
                                            }

                                            self.current_scope = saved_scope;
                                            if let Some(e) = err {
                                                return Err(e);
                                            }

                                            if let Some(updated_this) = super_scope.borrow().get("this") {
                                                self.current_scope.borrow_mut().set("this", updated_this);
                                            }
                                            return Ok(ret);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if let Expr::Identifier(id) = &**object {
                        if id == "document" {
                            match property.as_str() {
                                "getElementById" => {
                                    if let Some(target_id) = evaluated_args.first().and_then(|v| v.as_str()) {
                                        if let Some(elem) = self.document.get_element_by_id(target_id) {
                                            return Ok(self.dom_node_to_js(&elem));
                                        }
                                    }
                                    return Ok(JsValue::Null);
                                }
                                "querySelector" => {
                                    if let Some(sel) = evaluated_args.first().and_then(|v| v.as_str()) {
                                        if let Some(elem) = self.document.query_selector(sel) {
                                            return Ok(self.dom_node_to_js(&elem));
                                        }
                                    }
                                    return Ok(JsValue::Null);
                                }
                                "querySelectorAll" => {
                                    if let Some(sel) = evaluated_args.first().and_then(|v| v.as_str()) {
                                        let all = self.document.query_selector_all(sel);
                                        let list: Vec<JsValue> = all.iter().map(|n| self.dom_node_to_js(n)).collect();
                                        return Ok(JsValue::Array(list));
                                    }
                                    return Ok(JsValue::Array(Vec::new()));
                                }
                                "createElement" => {
                                    if let Some(tag) = evaluated_args.first().and_then(|v| v.as_str()) {
                                        let elem = self.document.create_element(tag);
                                        return Ok(self.dom_node_to_js(&elem));
                                    }
                                    return Ok(JsValue::Null);
                                }
                                "createTextNode" => {
                                    if let Some(text) = evaluated_args.first().and_then(|v| v.as_str()) {
                                        let elem = self.document.create_text_node(text);
                                        return Ok(self.dom_node_to_js(&elem));
                                    }
                                    return Ok(JsValue::Null);
                                }
                                "createComment" => {
                                    if let Some(text) = evaluated_args.first().and_then(|v| v.as_str()) {
                                        let elem = self.document.create_comment(text);
                                        return Ok(self.dom_node_to_js(&elem));
                                    }
                                    return Ok(JsValue::Null);
                                }
                                _ => {}
                            }
                        } else if id == "console" {
                            let msg = evaluated_args
                                .iter()
                                .map(|v| match v {
                                    JsValue::String(s) => s.clone(),
                                    JsValue::Number(n) => n.to_string(),
                                    JsValue::Bool(b) => b.to_string(),
                                    JsValue::Null => "null".to_string(),
                                    JsValue::Undefined => "undefined".to_string(),
                                    other => stringify_json(other),
                                })
                                .collect::<Vec<_>>()
                                .join(" ");
                            self.console_logs.push(msg);
                            return Ok(JsValue::Undefined);
                        } else if id == "JSON" {
                            if property == "stringify" {
                                if let Some(first) = evaluated_args.first() {
                                    return Ok(JsValue::String(stringify_json(first)));
                                }
                            } else if property == "parse" {
                                if let Some(first) = evaluated_args.first().and_then(|v| v.as_str()) {
                                    return parse_json(first);
                                }
                            }
                        } else if id == "Math" {
                            match property.as_str() {
                                "floor" => {
                                    let n = evaluated_args.first().and_then(|v| v.as_f64()).unwrap_or(0.0);
                                    return Ok(JsValue::Number(n.floor()));
                                }
                                "ceil" => {
                                    let n = evaluated_args.first().and_then(|v| v.as_f64()).unwrap_or(0.0);
                                    return Ok(JsValue::Number(n.ceil()));
                                }
                                "round" => {
                                    let n = evaluated_args.first().and_then(|v| v.as_f64()).unwrap_or(0.0);
                                    return Ok(JsValue::Number(n.round()));
                                }
                                "abs" => {
                                    let n = evaluated_args.first().and_then(|v| v.as_f64()).unwrap_or(0.0);
                                    return Ok(JsValue::Number(n.abs()));
                                }
                                "min" => {
                                    let mut m = f64::INFINITY;
                                    for a in &evaluated_args {
                                        if let Some(n) = a.as_f64() {
                                            if n < m { m = n; }
                                        }
                                    }
                                    return Ok(JsValue::Number(m));
                                }
                                "max" => {
                                    let mut m = f64::NEG_INFINITY;
                                    for a in &evaluated_args {
                                        if let Some(n) = a.as_f64() {
                                            if n > m { m = n; }
                                        }
                                    }
                                    return Ok(JsValue::Number(m));
                                }
                                "sqrt" => {
                                    let n = evaluated_args.first().and_then(|v| v.as_f64()).unwrap_or(0.0);
                                    return Ok(JsValue::Number(n.sqrt()));
                                }
                                _ => {}
                            }
                        } else if id == "Promise" {
                            match property.as_str() {
                                "resolve" => {
                                    let v = evaluated_args.first().cloned().unwrap_or(JsValue::Undefined);
                                    let p = JsPromise::resolve(v);
                                    return Ok(self.create_promise_value(p));
                                }
                                "reject" => {
                                    let v = evaluated_args.first().cloned().unwrap_or(JsValue::Undefined);
                                    let p = JsPromise::reject(v);
                                    return Ok(self.create_promise_value(p));
                                }
                                "all" => {
                                    let first = evaluated_args.first().cloned().unwrap_or(JsValue::Array(Vec::new()));
                                    if let JsValue::Array(items) = first {
                                        let mut promise_list = Vec::new();
                                        for item in items {
                                            if let JsValue::Object(ref map) = item {
                                                if map.get("__is_promise__") == Some(&JsValue::Bool(true)) {
                                                    if let Some(JsValue::Number(pid_num)) = map.get("__promise_id__") {
                                                        if let Some(p) = self.promises.get(&(*pid_num as u64)).cloned() {
                                                            promise_list.push(p);
                                                            continue;
                                                        }
                                                    }
                                                }
                                            }
                                            promise_list.push(JsPromise::resolve(item));
                                        }
                                        let all_p = JsPromise::all(promise_list);
                                        drain_microtasks();
                                        let mapped_p = all_p.then(JsValue::Array);
                                        drain_microtasks();
                                        return Ok(self.create_promise_value(mapped_p));
                                    }
                                    let p = JsPromise::resolve(JsValue::Array(Vec::new()));
                                    return Ok(self.create_promise_value(p));
                                }
                                _ => {}
                            }
                        }
                    }

                    // Array prototype method calls: arr.push, arr.map, arr.filter, arr.join, etc.
                    let obj_val = self.eval_expr(object)?;
                    if let JsValue::Array(ref arr) = obj_val {
                        let obj_ident = if let Expr::Identifier(ref id) = &**object {
                            Some(id.clone())
                        } else {
                            None
                        };

                        match property.as_str() {
                            "push" => {
                                let mut new_len = arr.len();
                                if let Some(ref id) = obj_ident {
                                    let existing = self.current_scope.borrow().get(id);
                                    if let Some(JsValue::Array(mut target_arr)) = existing {
                                        for a in evaluated_args {
                                            target_arr.push(a);
                                        }
                                        new_len = target_arr.len();
                                        self.current_scope.borrow_mut().set(id, JsValue::Array(target_arr.clone()));
                                        if self.current_scope.borrow().parent.is_none() {
                                            self.globals.insert(id.clone(), JsValue::Array(target_arr));
                                        }
                                    } else if let Some(JsValue::Array(ref mut target_arr)) = self.globals.get_mut(id) {
                                        for a in evaluated_args {
                                            target_arr.push(a);
                                        }
                                        new_len = target_arr.len();
                                    }
                                }
                                return Ok(JsValue::Number(new_len as f64));
                            }
                            "pop" => {
                                let mut popped = JsValue::Undefined;
                                if let Some(ref id) = obj_ident {
                                    let existing = self.current_scope.borrow().get(id);
                                    if let Some(JsValue::Array(mut target_arr)) = existing {
                                        popped = target_arr.pop().unwrap_or(JsValue::Undefined);
                                        self.current_scope.borrow_mut().set(id, JsValue::Array(target_arr.clone()));
                                        if self.current_scope.borrow().parent.is_none() {
                                            self.globals.insert(id.clone(), JsValue::Array(target_arr));
                                        }
                                    } else if let Some(JsValue::Array(ref mut target_arr)) = self.globals.get_mut(id) {
                                        popped = target_arr.pop().unwrap_or(JsValue::Undefined);
                                    }
                                }
                                return Ok(popped);
                            }
                            "map" => {
                                if let Some(cb) = evaluated_args.first().cloned() {
                                    let mut out = Vec::new();
                                    for (i, item) in arr.iter().enumerate() {
                                        let res = self.call_value(&cb, vec![item.clone(), JsValue::Number(i as f64), obj_val.clone()])?;
                                        out.push(res);
                                    }
                                    return Ok(JsValue::Array(out));
                                }
                                return Ok(JsValue::Array(Vec::new()));
                            }
                            "filter" => {
                                if let Some(cb) = evaluated_args.first().cloned() {
                                    let mut out = Vec::new();
                                    for (i, item) in arr.iter().enumerate() {
                                        let res = self.call_value(&cb, vec![item.clone(), JsValue::Number(i as f64), obj_val.clone()])?;
                                        if res.is_truthy() {
                                            out.push(item.clone());
                                        }
                                    }
                                    return Ok(JsValue::Array(out));
                                }
                                return Ok(JsValue::Array(Vec::new()));
                            }
                            "find" => {
                                if let Some(cb) = evaluated_args.first().cloned() {
                                    for (i, item) in arr.iter().enumerate() {
                                        let res = self.call_value(&cb, vec![item.clone(), JsValue::Number(i as f64), obj_val.clone()])?;
                                        if res.is_truthy() {
                                            return Ok(item.clone());
                                        }
                                    }
                                }
                                return Ok(JsValue::Undefined);
                            }
                            "findIndex" => {
                                if let Some(cb) = evaluated_args.first().cloned() {
                                    for (i, item) in arr.iter().enumerate() {
                                        let res = self.call_value(&cb, vec![item.clone(), JsValue::Number(i as f64), obj_val.clone()])?;
                                        if res.is_truthy() {
                                            return Ok(JsValue::Number(i as f64));
                                        }
                                    }
                                }
                                return Ok(JsValue::Number(-1.0));
                            }
                            "some" => {
                                if let Some(cb) = evaluated_args.first().cloned() {
                                    for (i, item) in arr.iter().enumerate() {
                                        let res = self.call_value(&cb, vec![item.clone(), JsValue::Number(i as f64), obj_val.clone()])?;
                                        if res.is_truthy() {
                                            return Ok(JsValue::Bool(true));
                                        }
                                    }
                                }
                                return Ok(JsValue::Bool(false));
                            }
                            "every" => {
                                if let Some(cb) = evaluated_args.first().cloned() {
                                    for (i, item) in arr.iter().enumerate() {
                                        let res = self.call_value(&cb, vec![item.clone(), JsValue::Number(i as f64), obj_val.clone()])?;
                                        if !res.is_truthy() {
                                            return Ok(JsValue::Bool(false));
                                        }
                                    }
                                    return Ok(JsValue::Bool(true));
                                }
                                return Ok(JsValue::Bool(true));
                            }
                            "reduce" => {
                                if let Some(cb) = evaluated_args.first().cloned() {
                                    let mut iter = arr.iter();
                                    let mut accum = if evaluated_args.len() >= 2 {
                                        evaluated_args[1].clone()
                                    } else if let Some(first) = iter.next() {
                                        first.clone()
                                    } else {
                                        return Err("TypeError: Reduce of empty array with no initial value".to_string());
                                    };
                                    for (i, item) in iter.enumerate() {
                                        accum = self.call_value(&cb, vec![accum, item.clone(), JsValue::Number(i as f64), obj_val.clone()])?;
                                    }
                                    return Ok(accum);
                                }
                                return Ok(JsValue::Undefined);
                            }
                            "join" => {
                                let sep = evaluated_args.first().and_then(|v| v.as_str()).unwrap_or(",");
                                return Ok(JsValue::String(obj_val.join(sep)));
                            }
                            "slice" => {
                                let start = evaluated_args.first().and_then(|v| v.as_f64()).unwrap_or(0.0) as isize;
                                let end = evaluated_args.get(1).and_then(|v| v.as_f64()).map(|n| n as isize);
                                return Ok(obj_val.slice(start, end));
                            }
                            "includes" => {
                                if let Some(item) = evaluated_args.first() {
                                    return Ok(JsValue::Bool(obj_val.includes(item)));
                                }
                                return Ok(JsValue::Bool(false));
                            }
                            "indexOf" => {
                                if let Some(target) = evaluated_args.first() {
                                    for (i, item) in arr.iter().enumerate() {
                                        if item.strict_eq(target) {
                                            return Ok(JsValue::Number(i as f64));
                                        }
                                    }
                                }
                                return Ok(JsValue::Number(-1.0));
                            }
                            "reverse" => {
                                let mut reversed = arr.clone();
                                reversed.reverse();
                                if let Some(ref id) = obj_ident {
                                    self.current_scope.borrow_mut().set(id, JsValue::Array(reversed.clone()));
                                    if self.current_scope.borrow().parent.is_none() {
                                        self.globals.insert(id.clone(), JsValue::Array(reversed.clone()));
                                    }
                                }
                                return Ok(JsValue::Array(reversed));
                            }
                            "flat" => {
                                let depth = evaluated_args.first().and_then(|v| v.as_f64()).unwrap_or(1.0) as usize;
                                return Ok(obj_val.flat(depth));
                            }
                            "sort" => {
                                let mut sorted = arr.clone();
                                if let Some(cb) = evaluated_args.first().cloned() {
                                    let mut sort_err = None;
                                    sorted.sort_by(|a, b| {
                                        if sort_err.is_some() {
                                            return std::cmp::Ordering::Equal;
                                        }
                                        match self.call_value(&cb, vec![a.clone(), b.clone()]) {
                                            Ok(JsValue::Number(n)) => {
                                                if n < 0.0 {
                                                    std::cmp::Ordering::Less
                                                } else if n > 0.0 {
                                                    std::cmp::Ordering::Greater
                                                } else {
                                                    std::cmp::Ordering::Equal
                                                }
                                            }
                                            Ok(_) => std::cmp::Ordering::Equal,
                                            Err(e) => {
                                                sort_err = Some(e);
                                                std::cmp::Ordering::Equal
                                            }
                                        }
                                    });
                                    if let Some(err) = sort_err {
                                        return Err(err);
                                    }
                                } else {
                                    sorted.sort_by_key(|a| a.to_string());
                                }
                                if let Some(ref id) = obj_ident {
                                    self.current_scope.borrow_mut().set(id, JsValue::Array(sorted.clone()));
                                    if self.current_scope.borrow().parent.is_none() {
                                        self.globals.insert(id.clone(), JsValue::Array(sorted.clone()));
                                    }
                                }
                                return Ok(JsValue::Array(sorted));
                            }
                            _ => {}
                        }
                    }

                    // DOM Node method calls: elem.appendChild, elem.insertBefore, elem.removeChild, etc.
                    if let JsValue::Object(ref map) = obj_val {
                        if let Some(node) = self.resolve_dom_node(map) {
                            match property.as_str() {
                                "setAttribute" => {
                                    if evaluated_args.len() >= 2 {
                                        if let (Some(k), Some(v)) = (
                                            evaluated_args[0].as_str(),
                                            evaluated_args[1].as_str(),
                                        ) {
                                            node.set_attribute(k, v);
                                            self.document.mark_mutated();
                                        }
                                    }
                                    return Ok(JsValue::Undefined);
                                }
                                "getAttribute" => {
                                    if let Some(k) = evaluated_args.first().and_then(|v| v.as_str()) {
                                        if let Some(v) = node.get_attribute(k) {
                                            return Ok(JsValue::String(v));
                                        }
                                    }
                                    return Ok(JsValue::Null);
                                }
                                "removeAttribute" => {
                                    if let Some(k) = evaluated_args.first().and_then(|v| v.as_str()) {
                                        node.remove_attribute(k);
                                        self.document.mark_mutated();
                                    }
                                    return Ok(JsValue::Undefined);
                                }
                                "hasAttribute" => {
                                    if let Some(k) = evaluated_args.first().and_then(|v| v.as_str()) {
                                        return Ok(JsValue::Bool(node.has_attribute(k)));
                                    }
                                    return Ok(JsValue::Bool(false));
                                }
                                "appendChild" => {
                                    if let Some(JsValue::Object(child_map)) = evaluated_args.first() {
                                        if let Some(child_node) = self.resolve_dom_node(child_map) {
                                            node.append_child(child_node.clone());
                                            self.document.mark_mutated();
                                            return Ok(self.dom_node_to_js(&child_node));
                                        }
                                    }
                                    return Ok(JsValue::Null);
                                }
                                "prepend" => {
                                    if let Some(JsValue::Object(child_map)) = evaluated_args.first() {
                                        if let Some(child_node) = self.resolve_dom_node(child_map) {
                                            node.prepend_child(child_node.clone());
                                            self.document.mark_mutated();
                                            return Ok(self.dom_node_to_js(&child_node));
                                        }
                                    }
                                    return Ok(JsValue::Null);
                                }
                                "insertBefore" => {
                                    if let Some(JsValue::Object(new_map)) = evaluated_args.first() {
                                        if let Some(new_node) = self.resolve_dom_node(new_map) {
                                            let ref_node = if evaluated_args.len() >= 2 {
                                                if let Some(JsValue::Object(ref_map)) = evaluated_args.get(1) {
                                                    self.resolve_dom_node(ref_map)
                                                } else {
                                                    None
                                                }
                                            } else {
                                                None
                                            };
                                            let inserted = node.insert_before(new_node.clone(), ref_node.as_ref())?;
                                            self.document.mark_mutated();
                                            return Ok(self.dom_node_to_js(&inserted));
                                        }
                                    }
                                    return Ok(JsValue::Null);
                                }
                                "replaceChild" => {
                                    if evaluated_args.len() >= 2 {
                                        if let (Some(JsValue::Object(new_map)), Some(JsValue::Object(old_map))) = (
                                            evaluated_args.first(),
                                            evaluated_args.get(1),
                                        ) {
                                            if let (Some(new_node), Some(old_node)) = (
                                                self.resolve_dom_node(new_map),
                                                self.resolve_dom_node(old_map),
                                            ) {
                                                let replaced = node.replace_child(new_node, &old_node)?;
                                                self.document.mark_mutated();
                                                return Ok(self.dom_node_to_js(&replaced));
                                            }
                                        }
                                    }
                                    return Ok(JsValue::Null);
                                }
                                "removeChild" => {
                                    if let Some(JsValue::Object(child_map)) = evaluated_args.first() {
                                        if let Some(child_node) = self.resolve_dom_node(child_map) {
                                            let removed = node.remove_child(&child_node);
                                            if removed {
                                                self.document.mark_mutated();
                                                return Ok(self.dom_node_to_js(&child_node));
                                            }
                                        }
                                    }
                                    return Ok(JsValue::Null);
                                }
                                "remove" => {
                                    node.remove();
                                    self.document.mark_mutated();
                                    return Ok(JsValue::Undefined);
                                }
                                "cloneNode" => {
                                    let deep = evaluated_args.first().map(|v| v.is_truthy()).unwrap_or(false);
                                    let cloned = node.clone_node(deep);
                                    return Ok(self.dom_node_to_js(&cloned));
                                }
                                "contains" => {
                                    if let Some(JsValue::Object(other_map)) = evaluated_args.first() {
                                        if let Some(other_node) = self.resolve_dom_node(other_map) {
                                            return Ok(JsValue::Bool(node.contains(&other_node)));
                                        }
                                    }
                                    return Ok(JsValue::Bool(false));
                                }
                                "querySelector" => {
                                    if let Some(sel) = evaluated_args.first().and_then(|v| v.as_str()) {
                                        if let Some(found) = node.query_selector(sel) {
                                            return Ok(self.dom_node_to_js(&found));
                                        }
                                    }
                                    return Ok(JsValue::Null);
                                }
                                "querySelectorAll" => {
                                    if let Some(sel) = evaluated_args.first().and_then(|v| v.as_str()) {
                                        let found = node.query_selector_all(sel);
                                        let list: Vec<JsValue> = found.iter().map(|n| self.dom_node_to_js(n)).collect();
                                        return Ok(JsValue::Array(list));
                                    }
                                    return Ok(JsValue::Array(Vec::new()));
                                }
                                "addEventListener" => {
                                    if evaluated_args.len() >= 2 {
                                        if let Some(event) = evaluated_args[0].as_str() {
                                            node.add_event_listener(event, evaluated_args[1].clone());
                                        }
                                    }
                                    return Ok(JsValue::Undefined);
                                }
                                _ => {}
                            }
                        }
                    }
                }

                if let Expr::Member { object, property, computed: None } = &**callee {
                    let obj_val = self.eval_expr(object)?;
                    if let JsValue::Object(ref map) = obj_val {
                        if map.get("__is_promise__") == Some(&JsValue::Bool(true)) {
                            if let Some(JsValue::Number(pid_num)) = map.get("__promise_id__") {
                                let pid = *pid_num as u64;
                                if let Some(promise) = self.promises.get(&pid).cloned() {
                                    match property.as_str() {
                                        "then" => {
                                            let cb = evaluated_args.first().cloned();
                                            let (next_promise, resolve, reject) = JsPromise::<JsValue>::new_pending();
                                            let mut next_val = JsValue::Undefined;
                                            let mut had_err = None;

                                            match promise.state() {
                                                PromiseState::Fulfilled(v) => {
                                                    if let Some(ref callback) = cb {
                                                        match self.call_value(callback, vec![v]) {
                                                            Ok(res) => next_val = res,
                                                            Err(e) => had_err = Some(e),
                                                        }
                                                    } else {
                                                        next_val = v;
                                                    }
                                                }
                                                PromiseState::Rejected(err) => {
                                                    had_err = Some(err.to_string());
                                                }
                                                PromiseState::Pending => {}
                                            }

                                            if let Some(e) = had_err {
                                                reject(JsValue::String(e));
                                            } else {
                                                if let JsValue::Object(ref m) = next_val {
                                                    if m.get("__is_promise__") == Some(&JsValue::Bool(true)) {
                                                        if let Some(JsValue::Number(sub_pid)) = m.get("__promise_id__") {
                                                            if let Some(sub_p) = self.promises.get(&(*sub_pid as u64)).cloned() {
                                                                match sub_p.state() {
                                                                    PromiseState::Fulfilled(sv) => resolve(sv),
                                                                    PromiseState::Rejected(se) => reject(se),
                                                                    PromiseState::Pending => resolve(next_val),
                                                                }
                                                                drain_microtasks();
                                                                return Ok(self.create_promise_value(next_promise));
                                                            }
                                                        }
                                                    }
                                                }
                                                resolve(next_val);
                                            }
                                            drain_microtasks();
                                            return Ok(self.create_promise_value(next_promise));
                                        }
                                        "catch" => {
                                            let cb = evaluated_args.first().cloned();
                                            let (next_promise, resolve, reject) = JsPromise::<JsValue>::new_pending();

                                            match promise.state() {
                                                PromiseState::Rejected(err) => {
                                                    if let Some(ref callback) = cb {
                                                        match self.call_value(callback, vec![err]) {
                                                            Ok(recovered) => resolve(recovered),
                                                            Err(e) => reject(JsValue::String(e)),
                                                        }
                                                    } else {
                                                        reject(err);
                                                    }
                                                }
                                                PromiseState::Fulfilled(v) => {
                                                    resolve(v);
                                                }
                                                PromiseState::Pending => {}
                                            }
                                            drain_microtasks();
                                            return Ok(self.create_promise_value(next_promise));
                                        }
                                        "finally" => {
                                            let cb = evaluated_args.first().cloned();
                                            if let Some(ref callback) = cb {
                                                let _ = self.call_value(callback, Vec::new());
                                            }
                                            drain_microtasks();
                                            return Ok(obj_val.clone());
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }

                        if let Some(JsValue::String(cls_name)) = map.get("__class__") {
                            if let Some((params, body, super_class_opt)) = self.resolve_method(cls_name, property) {
                                let method_scope = Scope::new_child(self.current_scope.clone());
                                method_scope.borrow_mut().declare("this", obj_val.clone());
                                method_scope.borrow_mut().declare("__current_class__", JsValue::String(cls_name.clone()));
                                if let Some(s_name) = super_class_opt {
                                    method_scope.borrow_mut().declare("__super_class__", JsValue::String(s_name));
                                }

                                for (p, a) in params.iter().zip(evaluated_args) {
                                    method_scope.borrow_mut().declare(p, a);
                                }

                                let saved_scope = self.current_scope.clone();
                                self.current_scope = method_scope.clone();

                                let mut ret = JsValue::Undefined;
                                let mut err = None;
                                for stmt in &body {
                                    match self.execute_stmt_flow(stmt) {
                                        Ok(StatementFlow::Normal(v)) => ret = v,
                                        Ok(StatementFlow::Return(v)) => {
                                            ret = v;
                                            break;
                                        }
                                        Ok(StatementFlow::Throw(v)) => {
                                            err = Some(format!("Uncaught exception: {v}"));
                                            break;
                                        }
                                        Err(e) => {
                                            err = Some(e);
                                            break;
                                        }
                                    }
                                }

                                self.current_scope = saved_scope;
                                if let Some(e) = err {
                                    return Err(e);
                                }

                                if let Expr::Identifier(ref id) = &**object {
                                    if let Some(updated_this) = method_scope.borrow().get("this") {
                                        self.current_scope.borrow_mut().set(id, updated_this.clone());
                                        if self.current_scope.borrow().parent.is_none() {
                                            self.globals.insert(id.clone(), updated_this);
                                        }
                                    }
                                }

                                return Ok(ret);
                            }
                        }
                    }
                }

                // Global function calls or indirect calls
                if let Expr::Identifier(fn_name) = &**callee {
                    if self.user_functions.contains_key(fn_name) {
                        return self.call_function(fn_name, evaluated_args);
                    }
                    if let Some(val) = self.globals.get(fn_name).cloned() {
                        return self.call_value(&val, evaluated_args);
                    }
                }

                let callee_val = self.eval_expr(callee)?;
                self.call_value(&callee_val, evaluated_args)
            }
            Expr::New { callee, args } => {
                let mut evaluated_args = Vec::new();
                for a in args {
                    evaluated_args.push(self.eval_expr(a)?);
                }
                match &**callee {
                    Expr::Identifier(name) => {
                        if name == "Promise" {
                            let pid = self.next_promise_id;
                            self.next_promise_id += 1;
                            let (promise, resolve_fn, reject_fn) = JsPromise::<JsValue>::new_pending();
                            self.promises.insert(pid, promise);
                            self.promise_controllers.insert(pid, (resolve_fn, reject_fn));

                            if let Some(executor) = evaluated_args.first().cloned() {
                                let mut res_map = BTreeMap::new();
                                res_map.insert("__fn__".to_string(), JsValue::String(format!("__promise_res_{pid}")));
                                let res_val = JsValue::Object(res_map);

                                let mut rej_map = BTreeMap::new();
                                rej_map.insert("__fn__".to_string(), JsValue::String(format!("__promise_rej_{pid}")));
                                let rej_val = JsValue::Object(rej_map);

                                self.call_value(&executor, vec![res_val, rej_val])?;
                            }
                            drain_microtasks();

                            let mut map = BTreeMap::new();
                            map.insert("__is_promise__".to_string(), JsValue::Bool(true));
                            map.insert("__promise_id__".to_string(), JsValue::Number(pid as f64));
                            Ok(JsValue::Object(map))
                        } else if name == "Error" {
                            let msg = evaluated_args.first().map(|v| v.to_string()).unwrap_or_default();
                            let mut map = BTreeMap::new();
                            map.insert("name".to_string(), JsValue::String("Error".to_string()));
                            map.insert("message".to_string(), JsValue::String(msg));
                            Ok(JsValue::Object(map))
                        } else if name == "Array" {
                            Ok(JsValue::Array(evaluated_args))
                        } else if name == "Object" {
                            Ok(JsValue::Object(BTreeMap::new()))
                        } else if self.classes.contains_key(name) {
                            self.instantiate_class(name, evaluated_args)
                        } else {
                            Err(format!("TypeError: {name} is not a constructor"))
                        }
                    }
                    other => {
                        let callee_val = self.eval_expr(other)?;
                        if let JsValue::Object(map) = callee_val {
                            if let Some(JsValue::String(ref cls_name)) = map.get("__class_name__") {
                                if cls_name == "Promise" {
                                    let pid = self.next_promise_id;
                                    self.next_promise_id += 1;
                                    let (promise, resolve_fn, reject_fn) = JsPromise::<JsValue>::new_pending();
                                    self.promises.insert(pid, promise);
                                    self.promise_controllers.insert(pid, (resolve_fn, reject_fn));

                                    if let Some(executor) = evaluated_args.first().cloned() {
                                        let mut res_map = BTreeMap::new();
                                        res_map.insert("__fn__".to_string(), JsValue::String(format!("__promise_res_{pid}")));
                                        let res_val = JsValue::Object(res_map);

                                        let mut rej_map = BTreeMap::new();
                                        rej_map.insert("__fn__".to_string(), JsValue::String(format!("__promise_rej_{pid}")));
                                        let rej_val = JsValue::Object(rej_map);

                                        self.call_value(&executor, vec![res_val, rej_val])?;
                                    }
                                    drain_microtasks();

                                    let mut map = BTreeMap::new();
                                    map.insert("__is_promise__".to_string(), JsValue::Bool(true));
                                    map.insert("__promise_id__".to_string(), JsValue::Number(pid as f64));
                                    return Ok(JsValue::Object(map));
                                }
                                if self.classes.contains_key(cls_name) {
                                    return self.instantiate_class(cls_name, evaluated_args);
                                }
                            }
                        }
                        Err("TypeError: Value is not a constructor".to_string())
                    }
                }
            }
            Expr::Array(items) => {
                let mut vals = Vec::new();
                for item in items {
                    vals.push(self.eval_expr(item)?);
                }
                Ok(JsValue::Array(vals))
            }
            Expr::Object(pairs) => {
                let mut map = BTreeMap::new();
                for (k, v) in pairs {
                    map.insert(k.clone(), self.eval_expr(v)?);
                }
                Ok(JsValue::Object(map))
            }
        }
    }

    fn resolve_dom_node(&self, map: &BTreeMap<String, JsValue>) -> Option<DomNode> {
        if let Some(JsValue::Number(uid)) = map.get("__uid__") {
            let u = *uid as u64;
            if let Some(node) = self.live_nodes.borrow().get(&u).cloned() {
                return Some(node);
            }
            if let Some(found) = self.document.get_element_by_uid(u) {
                self.live_nodes.borrow_mut().insert(u, found.clone());
                return Some(found);
            }
        }
        if let Some(JsValue::String(id)) = map.get("__dom_id__") {
            if let Some(found) = self.document.get_element_by_id(id) {
                self.live_nodes.borrow_mut().insert(found.uid(), found.clone());
                return Some(found);
            }
        }
        None
    }

    fn dom_node_to_js(&self, node: &DomNode) -> JsValue {
        self.live_nodes.borrow_mut().insert(node.uid(), node.clone());
        let mut map = BTreeMap::new();
        map.insert("__uid__".to_string(), JsValue::Number(node.uid() as f64));
        let id_val = node.id().unwrap_or_else(|| format!("dom_node_{}", node.uid()));
        map.insert("__dom_id__".to_string(), JsValue::String(id_val));
        map.insert(
            "id".to_string(),
            JsValue::String(node.id().unwrap_or_default()),
        );
        map.insert(
            "tagName".to_string(),
            JsValue::String(node.tag_name().unwrap_or_default()),
        );
        map.insert(
            "className".to_string(),
            JsValue::String(node.class_name()),
        );
        map.insert(
            "innerHTML".to_string(),
            JsValue::String(node.inner_html()),
        );
        map.insert(
            "textContent".to_string(),
            JsValue::String(node.text_content()),
        );
        JsValue::Object(map)
    }

    pub fn call_closure(&mut self, closure: &JsClosure, args: Vec<JsValue>) -> Result<JsValue, String> {
        let call_scope = Scope::new_child(closure.env.clone());
        for (param, arg) in closure.params.iter().zip(args) {
            call_scope.borrow_mut().declare(param, arg);
        }

        let saved_scope = self.current_scope.clone();
        self.current_scope = call_scope;

        let mut ret = JsValue::Undefined;
        let mut thrown = None;
        let mut sys_err = None;
        for stmt in &closure.body {
            match self.execute_stmt_flow(stmt) {
                Ok(StatementFlow::Normal(v)) => ret = v,
                Ok(StatementFlow::Return(v)) => {
                    ret = v;
                    break;
                }
                Ok(StatementFlow::Throw(v)) => {
                    thrown = Some(v);
                    break;
                }
                Err(e) => {
                    sys_err = Some(e);
                    break;
                }
            }
        }

        self.current_scope = saved_scope;

        if closure.is_async {
            if let Some(err_val) = thrown {
                let p = JsPromise::reject(err_val);
                return Ok(self.create_promise_value(p));
            }
            if let Some(e) = sys_err {
                let p = JsPromise::reject(JsValue::String(e));
                return Ok(self.create_promise_value(p));
            }
            if let JsValue::Object(ref map) = ret {
                if map.get("__is_promise__") == Some(&JsValue::Bool(true)) {
                    return Ok(ret);
                }
            }
            let p = JsPromise::resolve(ret);
            return Ok(self.create_promise_value(p));
        }

        if let Some(v) = thrown {
            self.pending_throw = Some(v);
            return Ok(JsValue::Undefined);
        }

        if let Some(err_res) = sys_err {
            return Err(err_res);
        }

        Ok(ret)
    }

    pub fn call_function(&mut self, name: &str, args: Vec<JsValue>) -> Result<JsValue, String> {
        if let Some(pid_str) = name.strip_prefix("__promise_res_") {
            if let Ok(pid) = pid_str.parse::<u64>() {
                if let Some((res_fn, _)) = self.promise_controllers.get(&pid).cloned() {
                    let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                    res_fn(val);
                    drain_microtasks();
                    return Ok(JsValue::Undefined);
                }
            }
        }
        if let Some(pid_str) = name.strip_prefix("__promise_rej_") {
            if let Ok(pid) = pid_str.parse::<u64>() {
                if let Some((_, rej_fn)) = self.promise_controllers.get(&pid).cloned() {
                    let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                    rej_fn(val);
                    drain_microtasks();
                    return Ok(JsValue::Undefined);
                }
            }
        }

        if let Some(closure) = self.user_functions.get(name).cloned() {
            return self.call_closure(&closure, args);
        }

        Err(format!("Undefined function: '{name}'"))
    }

    fn eval_binary(&self, op: &BinaryOp, left: &JsValue, right: &JsValue) -> Result<JsValue, String> {
        match op {
            BinaryOp::Add => match (left, right) {
                (JsValue::String(a), JsValue::String(b)) => Ok(JsValue::String(format!("{a}{b}"))),
                (JsValue::String(a), b) => Ok(JsValue::String(format!("{a}{}", self.to_display_string(b)))),
                (a, JsValue::String(b)) => Ok(JsValue::String(format!("{}{b}", self.to_display_string(a)))),
                (JsValue::Number(a), JsValue::Number(b)) => Ok(JsValue::Number(a + b)),
                _ => Ok(JsValue::Number(0.0)),
            },
            BinaryOp::Sub => {
                let a = left.as_f64().unwrap_or(0.0);
                let b = right.as_f64().unwrap_or(0.0);
                Ok(JsValue::Number(a - b))
            }
            BinaryOp::Mul => {
                let a = left.as_f64().unwrap_or(0.0);
                let b = right.as_f64().unwrap_or(0.0);
                Ok(JsValue::Number(a * b))
            }
            BinaryOp::Div => {
                let a = left.as_f64().unwrap_or(0.0);
                let b = right.as_f64().unwrap_or(0.0);
                Ok(JsValue::Number(a / b))
            }
            BinaryOp::Mod => {
                let a = left.as_f64().unwrap_or(0.0);
                let b = right.as_f64().unwrap_or(0.0);
                Ok(JsValue::Number(a % b))
            }
            BinaryOp::StrictEq => Ok(JsValue::Bool(left.strict_eq(right))),
            BinaryOp::StrictNotEq => Ok(JsValue::Bool(!left.strict_eq(right))),
            BinaryOp::Eq => Ok(JsValue::Bool(left.loose_eq(right))),
            BinaryOp::NotEq => Ok(JsValue::Bool(!left.loose_eq(right))),
            BinaryOp::Lt => {
                let a = left.as_f64().unwrap_or(0.0);
                let b = right.as_f64().unwrap_or(0.0);
                Ok(JsValue::Bool(a < b))
            }
            BinaryOp::Lte => {
                let a = left.as_f64().unwrap_or(0.0);
                let b = right.as_f64().unwrap_or(0.0);
                Ok(JsValue::Bool(a <= b))
            }
            BinaryOp::Gt => {
                let a = left.as_f64().unwrap_or(0.0);
                let b = right.as_f64().unwrap_or(0.0);
                Ok(JsValue::Bool(a > b))
            }
            BinaryOp::Gte => {
                let a = left.as_f64().unwrap_or(0.0);
                let b = right.as_f64().unwrap_or(0.0);
                Ok(JsValue::Bool(a >= b))
            }
            BinaryOp::And => {
                if !left.is_truthy() {
                    Ok(left.clone())
                } else {
                    Ok(right.clone())
                }
            }
            BinaryOp::Or => {
                if left.is_truthy() {
                    Ok(left.clone())
                } else {
                    Ok(right.clone())
                }
            }
            BinaryOp::InstanceOf => {
                let class_name = match right {
                    JsValue::String(s) => s.clone(),
                    JsValue::Object(map) => {
                        map.get("__class_name__")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string()
                    }
                    _ => String::new(),
                };
                Ok(JsValue::Bool(self.is_instance_of(left, &class_name)))
            }
        }
    }

    fn to_display_string(&self, val: &JsValue) -> String {
        match val {
            JsValue::String(s) => s.clone(),
            JsValue::Number(n) => n.to_string(),
            JsValue::Bool(b) => b.to_string(),
            JsValue::Null => "null".to_string(),
            JsValue::Undefined => "undefined".to_string(),
            other => stringify_json(other),
        }
    }
}
