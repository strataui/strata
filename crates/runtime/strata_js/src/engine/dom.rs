//! W3C DOM Tree representation, client-side mutations, and HTML5 fragment serialization.

use super::JsEngine;
use crate::JsValue;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DOM_UID: AtomicU64 = AtomicU64::new(1);

/// Types of live client-side DOM mutations emitted during script evaluation or native DOM manipulation.
#[derive(Debug, Clone, PartialEq)]
pub enum DomMutationType {
    ChildAppended { parent_uid: u64, child_uid: u64 },
    ChildPrepended { parent_uid: u64, child_uid: u64 },
    ChildInserted { parent_uid: u64, child_uid: u64, before_uid: Option<u64> },
    ChildRemoved { parent_uid: u64, child_uid: u64 },
    ChildReplaced { parent_uid: u64, new_child_uid: u64, old_child_uid: u64 },
    AttributeSet { node_uid: u64, name: String, value: String },
    AttributeRemoved { node_uid: u64, name: String },
    TextContentChanged { node_uid: u64, new_text: String },
    InnerHtmlChanged { node_uid: u64, html: String },
    StyleChanged { node_uid: u64, property: String, value: String },
}

/// A fine-grained record describing an individual DOM mutation.
#[derive(Debug, Clone, PartialEq)]
pub struct DomMutationRecord {
    pub target_uid: u64,
    pub mutation: DomMutationType,
}

pub type MutationListener = Rc<RefCell<dyn FnMut(&DomMutationRecord)>>;

/// Notification sink shared across DOM nodes and the containing Document.
pub struct MutationSink {
    pub queue: Vec<DomMutationRecord>,
    pub listeners: Vec<MutationListener>,
    pub sender: Option<std::sync::mpsc::Sender<DomMutationRecord>>,
    pub mutated: bool,
}

impl Default for MutationSink {
    fn default() -> Self {
        Self::new()
    }
}

impl MutationSink {
    pub fn new() -> Self {
        Self {
            queue: Vec::new(),
            listeners: Vec::new(),
            sender: None,
            mutated: false,
        }
    }

    pub fn emit(&mut self, record: DomMutationRecord) {
        self.mutated = true;
        self.queue.push(record.clone());
        for listener in &self.listeners {
            if let Ok(mut l) = listener.try_borrow_mut() {
                (l)(&record);
            }
        }
        if let Some(ref sender) = self.sender {
            let _ = sender.send(record);
        }
    }
}

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
    pub mutation_sink: Option<Rc<RefCell<MutationSink>>>,
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
                mutation_sink: None,
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
                mutation_sink: None,
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
                mutation_sink: None,
            })),
        }
    }

    pub(crate) fn emit_mutation(&self, mutation: DomMutationType, target_uid: u64) {
        let inner = self.inner.borrow();
        if let Some(ref sink) = inner.mutation_sink {
            sink.borrow_mut().emit(DomMutationRecord {
                target_uid,
                mutation,
            });
        }
    }

    pub(crate) fn set_mutation_sink_recursive(&self, sink: &Rc<RefCell<MutationSink>>) {
        let mut inner = self.inner.borrow_mut();
        inner.mutation_sink = Some(sink.clone());
        for child in &inner.children {
            child.set_mutation_sink_recursive(sink);
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
        let uid = inner.uid;
        if let DomNodeType::Element { ref mut attributes, .. } = inner.node_type {
            attributes.insert(name.to_ascii_lowercase(), val.to_string());
        }
        drop(inner);
        self.emit_mutation(
            DomMutationType::AttributeSet {
                node_uid: uid,
                name: name.to_ascii_lowercase(),
                value: val.to_string(),
            },
            uid,
        );
    }

    pub fn remove_attribute(&self, name: &str) {
        let mut inner = self.inner.borrow_mut();
        let uid = inner.uid;
        if let DomNodeType::Element { ref mut attributes, .. } = inner.node_type {
            attributes.remove(&name.to_ascii_lowercase());
        }
        drop(inner);
        self.emit_mutation(
            DomMutationType::AttributeRemoved {
                node_uid: uid,
                name: name.to_ascii_lowercase(),
            },
            uid,
        );
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
        let uid = inner.uid;
        if let DomNodeType::Element { ref mut style, .. } = inner.node_type {
            style.insert(prop.to_ascii_lowercase(), val.to_string());
        }
        drop(inner);
        self.emit_mutation(
            DomMutationType::StyleChanged {
                node_uid: uid,
                property: prop.to_ascii_lowercase(),
                value: val.to_string(),
            },
            uid,
        );
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
        let sink = self.inner.borrow().mutation_sink.clone();
        if let Some(ref s) = sink {
            child.set_mutation_sink_recursive(s);
        }
        child.inner.borrow_mut().parent = Some(Rc::downgrade(&self.inner));
        let mut inner = self.inner.borrow_mut();
        let uid = inner.uid;
        inner.children.clear();
        inner.children.push(child);
        drop(inner);
        self.emit_mutation(
            DomMutationType::TextContentChanged {
                node_uid: uid,
                new_text: text.to_string(),
            },
            uid,
        );
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
        let sink = self.inner.borrow().mutation_sink.clone();
        for child in &parsed {
            if let Some(ref s) = sink {
                child.set_mutation_sink_recursive(s);
            }
            child.inner.borrow_mut().parent = Some(Rc::downgrade(&self.inner));
        }
        let mut inner = self.inner.borrow_mut();
        let uid = inner.uid;
        inner.children = parsed;
        drop(inner);
        self.emit_mutation(
            DomMutationType::InnerHtmlChanged {
                node_uid: uid,
                html: html.to_string(),
            },
            uid,
        );
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
        let sink = self.inner.borrow().mutation_sink.clone();
        if let Some(ref s) = sink {
            child.set_mutation_sink_recursive(s);
        }
        child.inner.borrow_mut().parent = Some(Rc::downgrade(&self.inner));
        self.inner.borrow_mut().children.push(child.clone());
        let parent_uid = self.uid();
        let child_uid = child.uid();
        self.emit_mutation(
            DomMutationType::ChildAppended {
                parent_uid,
                child_uid,
            },
            parent_uid,
        );
        child
    }

    pub fn prepend_child(&self, child: DomNode) -> DomNode {
        child.remove();
        let sink = self.inner.borrow().mutation_sink.clone();
        if let Some(ref s) = sink {
            child.set_mutation_sink_recursive(s);
        }
        child.inner.borrow_mut().parent = Some(Rc::downgrade(&self.inner));
        self.inner.borrow_mut().children.insert(0, child.clone());
        let parent_uid = self.uid();
        let child_uid = child.uid();
        self.emit_mutation(
            DomMutationType::ChildPrepended {
                parent_uid,
                child_uid,
            },
            parent_uid,
        );
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
                let before_uid = Some(ref_node.uid());
                let sink = inner.mutation_sink.clone();
                drop(inner);
                new_child.remove();
                if let Some(ref s) = sink {
                    new_child.set_mutation_sink_recursive(s);
                }
                let mut inner = self.inner.borrow_mut();
                new_child.inner.borrow_mut().parent = Some(Rc::downgrade(&self.inner));
                inner.children.insert(idx, new_child.clone());
                let parent_uid = inner.uid;
                let child_uid = new_child.uid();
                drop(inner);
                self.emit_mutation(
                    DomMutationType::ChildInserted {
                        parent_uid,
                        child_uid,
                        before_uid,
                    },
                    parent_uid,
                );
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
        let sink = inner.mutation_sink.clone();
        drop(inner);
        new_child.remove();
        if let Some(ref s) = sink {
            new_child.set_mutation_sink_recursive(s);
        }
        let mut inner = self.inner.borrow_mut();
        old_child.inner.borrow_mut().parent = None;
        new_child.inner.borrow_mut().parent = Some(Rc::downgrade(&self.inner));
        inner.children[idx] = new_child.clone();
        let parent_uid = inner.uid;
        let new_child_uid = new_child.uid();
        let old_child_uid = old_child.uid();
        drop(inner);
        self.emit_mutation(
            DomMutationType::ChildReplaced {
                parent_uid,
                new_child_uid,
                old_child_uid,
            },
            parent_uid,
        );
        Ok(old_child.clone())
    }

    pub fn remove_child(&self, child: &DomNode) -> bool {
        let parent_uid = self.uid();
        let child_uid = child.uid();
        let mut inner = self.inner.borrow_mut();
        let before_len = inner.children.len();
        if let Some(pos) = inner.children.iter().position(|c| Rc::ptr_eq(&c.inner, &child.inner)) {
            inner.children.remove(pos);
            child.inner.borrow_mut().parent = None;
        }
        let removed = inner.children.len() < before_len;
        drop(inner);
        if removed {
            self.emit_mutation(
                DomMutationType::ChildRemoved {
                    parent_uid,
                    child_uid,
                },
                parent_uid,
            );
        }
        removed
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
            mutation_sink: None,
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
        self.dispatch_event_with_args(event, vec![], engine);
    }

    pub fn dispatch_event_with_args(&self, event: &str, args: Vec<JsValue>, engine: &mut JsEngine) {
        let attr_name = format!("on{}", event.to_ascii_lowercase());
        let inline_code = {
            let inner = self.inner.borrow();
            match &inner.node_type {
                DomNodeType::Element { attributes, .. } => attributes.get(&attr_name).cloned(),
                _ => None,
            }
        };

        if let Some(code) = inline_code {
            let this_val = engine.dom_node_to_js(self);
            engine.current_scope.borrow_mut().declare("this", this_val);
            if let Some(ev_val) = args.first() {
                engine.current_scope.borrow_mut().declare("event", ev_val.clone());
            }
            let _ = engine.eval(&code);
        }

        let listeners = {
            let inner = self.inner.borrow();
            inner
                .event_listeners
                .get(&event.to_ascii_lowercase())
                .cloned()
                .unwrap_or_default()
        };

        for listener in listeners {
            let _ = engine.call_value(&listener, args.clone());
        }
    }

    pub fn dispatch_event_with_bubbling(&self, event: &str, event_obj: JsValue, engine: &mut JsEngine) {
        let mut current = Some(self.clone());
        while let Some(node) = current {
            node.dispatch_event_with_args(event, vec![event_obj.clone()], engine);

            let propagation_stopped = event_obj.get_prop("__propagation_stopped__") == Some(JsValue::Bool(true));
            if propagation_stopped {
                break;
            }

            current = node.parent_element();
        }
    }
}

fn is_void_element(tag: &str) -> bool {
    matches!(
        tag.to_ascii_lowercase().as_str(),
        "area" | "base" | "br" | "col" | "embed" | "hr" | "img" | "input" | "link" | "meta" | "param" | "source" | "track" | "wbr"
    )
}

fn find_matching_close_pos(html: &str, tag_name: &str) -> Option<usize> {
    let open_prefix = format!("<{}", tag_name);
    let close_tag = format!("</{}>", tag_name);
    let bytes = html.as_bytes();
    let mut depth = 1usize;
    let mut pos = 0;

    while pos < bytes.len() {
        if bytes[pos] == b'<' {
            let rem = &html[pos..];
            if rem.to_ascii_lowercase().starts_with(&close_tag) {
                depth -= 1;
                if depth == 0 {
                    return Some(pos);
                }
                pos += close_tag.len();
                continue;
            } else if rem.to_ascii_lowercase().starts_with(&open_prefix) {
                let next_char = rem[open_prefix.len()..].chars().next();
                if matches!(next_char, Some(' ') | Some('\t') | Some('\n') | Some('\r') | Some('>') | Some('/')) {
                    if let Some(end_idx) = rem.find('>') {
                        let tag_str = &rem[..end_idx + 1];
                        if !tag_str.ends_with("/>") && !is_void_element(tag_name) {
                            depth += 1;
                        }
                        pos += end_idx + 1;
                        continue;
                    }
                }
            }
        }
        pos += 1;
    }
    None
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
                // Find matching closing tag `</tag_name>`
                let close_tag = format!("</{}>", tag_name);
                let remaining = &html[i..];
                if let Some(close_pos) = find_matching_close_pos(remaining, &tag_name) {
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
    pub mutation_sink: Rc<RefCell<MutationSink>>,
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

impl Document {
    pub fn new() -> Self {
        let sink = Rc::new(RefCell::new(MutationSink::new()));
        let html = DomNode::element("html", HashMap::new());
        let head = DomNode::element("head", HashMap::new());
        let body = DomNode::element("body", HashMap::new());

        html.set_mutation_sink_recursive(&sink);
        head.set_mutation_sink_recursive(&sink);
        body.set_mutation_sink_recursive(&sink);

        html.append_child(head.clone());
        html.append_child(body.clone());

        // Clear initial tree creation events from queue so initial document tree starts clean
        sink.borrow_mut().queue.clear();
        sink.borrow_mut().mutated = false;

        Self {
            html,
            head,
            body,
            title: Rc::new(RefCell::new("Strata Document".to_string())),
            mutated: Rc::new(RefCell::new(false)),
            mutation_sink: sink,
        }
    }

    pub fn mark_mutated(&self) {
        *self.mutated.borrow_mut() = true;
        self.mutation_sink.borrow_mut().mutated = true;
    }

    pub fn has_mutated(&self) -> bool {
        *self.mutated.borrow() || self.mutation_sink.borrow().mutated
    }

    pub fn clear_mutated(&self) {
        *self.mutated.borrow_mut() = false;
        let mut sink = self.mutation_sink.borrow_mut();
        sink.mutated = false;
        sink.queue.clear();
    }

    pub fn add_mutation_listener<F>(&self, listener: F)
    where
        F: FnMut(&DomMutationRecord) + 'static,
    {
        self.mutation_sink
            .borrow_mut()
            .listeners
            .push(Rc::new(RefCell::new(listener)));
    }

    pub fn set_mutation_channel(&self, sender: std::sync::mpsc::Sender<DomMutationRecord>) {
        self.mutation_sink.borrow_mut().sender = Some(sender);
    }

    pub fn drain_mutations(&self) -> Vec<DomMutationRecord> {
        let mut sink = self.mutation_sink.borrow_mut();
        sink.mutated = false;
        *self.mutated.borrow_mut() = false;
        std::mem::take(&mut sink.queue)
    }

    pub fn pending_mutations(&self) -> Vec<DomMutationRecord> {
        self.mutation_sink.borrow().queue.clone()
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

        doc.clear_mutated();
        doc
    }

    pub fn create_element(&self, tag: &str) -> DomNode {
        let node = DomNode::element(tag, HashMap::new());
        node.set_mutation_sink_recursive(&self.mutation_sink);
        node
    }

    pub fn create_text_node(&self, text: &str) -> DomNode {
        let node = DomNode::text(text);
        node.set_mutation_sink_recursive(&self.mutation_sink);
        node
    }

    pub fn create_comment(&self, comment: &str) -> DomNode {
        let node = DomNode::comment(comment);
        node.set_mutation_sink_recursive(&self.mutation_sink);
        node
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

