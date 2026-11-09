//! Strata DOM: Fine-grained reactive signal graph, DOM representations, and node blueprints.
//!
//! # Examples
//!
//! ```rust
//! use strata_dom::reactive::Signal;
//!
//! let count = Signal::new(100);
//! assert_eq!(count.get(), 100);
//! count.set(200);
//! assert_eq!(count.get(), 200);
//! ```

use std::rc::Rc;

/// Represents an opaque handle to a rendered UI subtree.
#[derive(Debug, Clone)]
pub struct View {
    pub root: NodeBlueprint,
}

impl View {
    pub fn new(root: NodeBlueprint) -> Self {
        Self { root }
    }

    /// Recursively flattens nested Fragment nodes within Element children.
    pub fn flatten_fragments(self) -> Self {
        Self {
            root: self.root.flatten_fragments(),
        }
    }
}

pub mod classes;
pub mod events;
pub mod hooks;
pub mod ir;
pub mod reactive;
pub mod spring;
pub mod text_input;

pub use classes::{ClassBitmask, ClassIter, ClassList, STANDARD_CLASSES, standard_class_index};
pub use events::{Event, EventDispatcher, EventPhase, EventType};
pub use hooks::*;
pub use ir::{
    ChildIter, DisplayChunkRange, FixedRect, IrNodeFlags, IrTag, MemoCache, MemoEntry, NodeId,
    StackGuard, UiIrGraph, UiIrNode,
};
pub use reactive::{
    batch, create_signal, has_dirty_signals, mark_dirty, take_dirty_signals, untrack, use_effect,
    use_memo, use_signal, use_state, Effect, Memo, ReactiveRuntime, ReadSignal, ScopeId, SetSignal,
    Signal, SignalId, SubscriberId, WriteSignal,
};
pub use spring::{SpringConfig, SpringSignal};
pub use text_input::{InputKey, InputKeyResult, InputModifiers, TextInputState};
pub use strata_primitives::EntityId;

/// Trait abstracting event handlers accepting `()` or `(Event)`.
pub trait EventHandlerMarker<Args> {
    fn call_handler(&self, event: Event);
}

impl<F: Fn(Event) + 'static> EventHandlerMarker<(Event,)> for F {
    fn call_handler(&self, event: Event) {
        (self)(event);
    }
}

impl<F: Fn() + 'static> EventHandlerMarker<()> for F {
    fn call_handler(&self, _event: Event) {
        (self)();
    }
}

impl EventHandlerMarker<()> for Rc<dyn Fn()> {
    fn call_handler(&self, _event: Event) {
        (self)();
    }
}

impl EventHandlerMarker<(Event,)> for Rc<dyn Fn(Event)> {
    fn call_handler(&self, event: Event) {
        (self)(event);
    }
}

impl<Args, F: EventHandlerMarker<Args>> EventHandlerMarker<Args> for Option<F> {
    fn call_handler(&self, event: Event) {
        if let Some(ref handler) = self {
            handler.call_handler(event);
        }
    }
}

/// Trait abstracting input/change value handlers accepting `()` or `(String)`.
pub trait ValueHandlerMarker<Args> {
    fn call_handler(&self, val: String);
}

impl<F: Fn(String) + 'static> ValueHandlerMarker<(String,)> for F {
    fn call_handler(&self, val: String) {
        (self)(val);
    }
}

impl<F: Fn() + 'static> ValueHandlerMarker<()> for F {
    fn call_handler(&self, _val: String) {
        (self)();
    }
}

impl<Args, F: ValueHandlerMarker<Args>> ValueHandlerMarker<Args> for Option<F> {
    fn call_handler(&self, val: String) {
        if let Some(ref handler) = self {
            handler.call_handler(val);
        }
    }
}

/// DOM element attribute or event listener.
#[derive(Clone)]
pub enum Attribute {
    Property {
        key: String,
        value: String,
    },
    EventListener {
        event: EventType,
        handler: Rc<dyn Fn(Event)>,
    },
    DirectStyle(Rc<dyn std::any::Any>),
    Entity(EntityId),
}

impl Attribute {
    pub fn property(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self::Property {
            key: key.into(),
            value: value.into(),
        }
    }

    pub fn direct_style<T: 'static>(style: T) -> Self {
        Self::DirectStyle(Rc::new(style))
    }

    pub fn entity(id: EntityId) -> Self {
        Self::Entity(id)
    }

    pub fn event_listener<Args, F: EventHandlerMarker<Args> + 'static>(
        event: EventType,
        handler: F,
    ) -> Self {
        Self::EventListener {
            event,
            handler: Rc::new(move |e| handler.call_handler(e)),
        }
    }

    pub fn value_listener<Args, F: ValueHandlerMarker<Args> + 'static>(
        event: EventType,
        handler: F,
    ) -> Self {
        Self::EventListener {
            event,
            handler: Rc::new(move |e| handler.call_handler(e.value.clone())),
        }
    }
}

impl std::fmt::Debug for Attribute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Property { key, value } => f
                .debug_struct("Property")
                .field("key", key)
                .field("value", value)
                .finish(),
            Self::EventListener { event, .. } => f
                .debug_struct("EventListener")
                .field("event", event)
                .field("handler", &"<callback>")
                .finish(),
            Self::DirectStyle(_) => f
                .debug_tuple("DirectStyle")
                .field(&"<typed_style>")
                .finish(),
            Self::Entity(id) => f.debug_tuple("Entity").field(id).finish(),
        }
    }
}

/// Core reactive blueprint tree representation.
#[derive(Clone)]
pub enum NodeBlueprint {
    Element {
        tag: Tag,
        classes: ClassList,
        attributes: Vec<Attribute>,
        children: Vec<NodeBlueprint>,
    },
    Text(String),
    DynamicText(Rc<dyn Fn() -> String>),
    Fragment(Vec<NodeBlueprint>),
    Empty,
}

impl std::fmt::Debug for NodeBlueprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Element {
                tag,
                classes,
                attributes,
                children,
            } => f
                .debug_struct("Element")
                .field("tag", tag)
                .field("classes", classes)
                .field("attributes", attributes)
                .field("children", children)
                .finish(),
            Self::Text(t) => f.debug_tuple("Text").field(t).finish(),
            Self::DynamicText(_) => f.debug_tuple("DynamicText").field(&"<dynamic>").finish(),
            Self::Fragment(c) => f.debug_tuple("Fragment").field(c).finish(),
            Self::Empty => write!(f, "Empty"),
        }
    }
}

impl NodeBlueprint {
    pub fn element(
        tag: Tag,
        classes: impl Into<ClassList>,
        attributes: Vec<Attribute>,
        children: Vec<NodeBlueprint>,
    ) -> Self {
        Self::Element {
            tag,
            classes: classes.into(),
            attributes,
            children,
        }
    }

    pub fn element_simple(
        tag: Tag,
        classes: impl Into<ClassList>,
        children: Vec<NodeBlueprint>,
    ) -> Self {
        Self::Element {
            tag,
            classes: classes.into(),
            attributes: Vec::new(),
            children,
        }
    }

    pub fn text(content: impl Into<String>) -> Self {
        Self::Text(content.into())
    }

    pub fn dynamic_text(f: impl Fn() -> String + 'static) -> Self {
        Self::DynamicText(Rc::new(f))
    }

    pub fn fragment(children: Vec<NodeBlueprint>) -> Self {
        Self::Fragment(children)
    }

    pub fn empty() -> Self {
        Self::Empty
    }

    /// Returns the EntityId assigned to this blueprint element if present.
    pub fn entity_id(&self) -> Option<EntityId> {
        match self {
            Self::Element { attributes, .. } => {
                for attr in attributes {
                    if let Attribute::Entity(id) = attr {
                        if id.is_valid() {
                            return Some(*id);
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Assigns or overrides the EntityId for an Element node blueprint.
    pub fn with_entity_id(mut self, entity_id: EntityId) -> Self {
        self.set_entity_id(entity_id);
        self
    }

    /// Mutably assigns or overrides the EntityId for an Element node blueprint.
    pub fn set_entity_id(&mut self, entity_id: EntityId) {
        if let Self::Element { attributes, .. } = self {
            if let Some(pos) = attributes.iter().position(|a| matches!(a, Attribute::Entity(_))) {
                attributes[pos] = Attribute::Entity(entity_id);
            } else {
                attributes.push(Attribute::Entity(entity_id));
            }
        }
    }

    /// Recursively ensures all Element nodes in this blueprint tree have a valid EntityId.
    pub fn ensure_entity_ids(&mut self) {
        match self {
            Self::Element {
                attributes,
                children,
                ..
            } => {
                let has_id = attributes
                    .iter()
                    .any(|a| matches!(a, Attribute::Entity(id) if id.is_valid()));
                if !has_id {
                    attributes.push(Attribute::Entity(EntityId::next()));
                }
                for child in children {
                    child.ensure_entity_ids();
                }
            }
            Self::Fragment(children) => {
                for child in children {
                    child.ensure_entity_ids();
                }
            }
            _ => {}
        }
    }

    /// Recursively locates an element blueprint node by its EntityId.
    pub fn find_by_entity_id(&self, target_id: EntityId) -> Option<&NodeBlueprint> {
        if !target_id.is_valid() {
            return None;
        }
        if self.entity_id() == Some(target_id) {
            return Some(self);
        }
        match self {
            Self::Element { children, .. } | Self::Fragment(children) => {
                for child in children {
                    if let Some(found) = child.find_by_entity_id(target_id) {
                        return Some(found);
                    }
                }
            }
            _ => {}
        }
        None
    }

    /// Recursively locates a mutable element blueprint node by its EntityId.
    pub fn find_by_entity_id_mut(&mut self, target_id: EntityId) -> Option<&mut NodeBlueprint> {
        if !target_id.is_valid() {
            return None;
        }
        if self.entity_id() == Some(target_id) {
            return Some(self);
        }
        match self {
            Self::Element { children, .. } | Self::Fragment(children) => {
                for child in children {
                    if let Some(found) = child.find_by_entity_id_mut(target_id) {
                        return Some(found);
                    }
                }
            }
            _ => {}
        }
        None
    }

    /// Returns the DOM node UID from a `data-strata-node-uid` attribute if present.
    pub fn dom_uid(&self) -> Option<u64> {
        match self {
            Self::Element { attributes, .. } => {
                for attr in attributes {
                    if let Attribute::Property { key, value } = attr {
                        if key == "data-strata-node-uid" {
                            if let Ok(uid) = value.parse::<u64>() {
                                return Some(uid);
                            }
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Recursively locates an element blueprint node by its DOM node UID.
    pub fn find_by_dom_uid(&self, target_uid: u64) -> Option<&NodeBlueprint> {
        if self.dom_uid() == Some(target_uid) {
            return Some(self);
        }
        match self {
            Self::Element { children, .. } | Self::Fragment(children) => {
                for child in children {
                    if let Some(found) = child.find_by_dom_uid(target_uid) {
                        return Some(found);
                    }
                }
            }
            _ => {}
        }
        None
    }

    /// Recursively locates a mutable element blueprint node by its DOM node UID.
    pub fn find_by_dom_uid_mut(&mut self, target_uid: u64) -> Option<&mut NodeBlueprint> {
        if self.dom_uid() == Some(target_uid) {
            return Some(self);
        }
        match self {
            Self::Element { children, .. } | Self::Fragment(children) => {
                for child in children {
                    if let Some(found) = child.find_by_dom_uid_mut(target_uid) {
                        return Some(found);
                    }
                }
            }
            _ => {}
        }
        None
    }

    /// Recursively calculates the index path to the element blueprint node with matching DOM node UID.
    pub fn find_path_by_dom_uid(&self, target_uid: u64) -> Option<Vec<usize>> {
        let mut path = Vec::new();
        if self.find_path_by_dom_uid_internal(target_uid, &mut path) {
            Some(path)
        } else {
            None
        }
    }

    fn find_path_by_dom_uid_internal(&self, target_uid: u64, path: &mut Vec<usize>) -> bool {
        if self.dom_uid() == Some(target_uid) {
            return true;
        }
        match self {
            Self::Element { children, .. } | Self::Fragment(children) => {
                for (i, child) in children.iter().enumerate() {
                    path.push(i);
                    if child.find_path_by_dom_uid_internal(target_uid, path) {
                        return true;
                    }
                    path.pop();
                }
            }
            _ => {}
        }
        false
    }

    /// Recursively calculates the index path to the element blueprint node with matching EntityId.
    pub fn find_path_by_entity_id(&self, target_id: EntityId) -> Option<Vec<usize>> {
        if !target_id.is_valid() {
            return None;
        }
        let mut path = Vec::new();
        if self.find_path_by_entity_id_internal(target_id, &mut path) {
            Some(path)
        } else {
            None
        }
    }

    fn find_path_by_entity_id_internal(&self, target_id: EntityId, path: &mut Vec<usize>) -> bool {
        if self.entity_id() == Some(target_id) {
            return true;
        }
        match self {
            Self::Element { children, .. } | Self::Fragment(children) => {
                for (i, child) in children.iter().enumerate() {
                    path.push(i);
                    if child.find_path_by_entity_id_internal(target_id, path) {
                        return true;
                    }
                    path.pop();
                }
            }
            _ => {}
        }
        false
    }

    /// Recursively flattens nested Fragment nodes within Element children,
    /// splicing fragment children directly into their parent elements.
    pub fn flatten_fragments(self) -> Self {
        match self {
            Self::Element {
                tag,
                classes,
                attributes,
                children,
            } => {
                let mut flat_children = Vec::with_capacity(children.len());
                for child in children {
                    let flat_child = child.flatten_fragments();
                    Self::splice_fragment_into(flat_child, &mut flat_children);
                }
                Self::Element {
                    tag,
                    classes,
                    attributes,
                    children: flat_children,
                }
            }
            Self::Fragment(children) => {
                let mut flat_children = Vec::with_capacity(children.len());
                for child in children {
                    let flat_child = child.flatten_fragments();
                    Self::splice_fragment_into(flat_child, &mut flat_children);
                }
                Self::Fragment(flat_children)
            }
            other => other,
        }
    }

    fn splice_fragment_into(node: Self, out: &mut Vec<Self>) {
        match node {
            Self::Fragment(children) => {
                for child in children {
                    Self::splice_fragment_into(child, out);
                }
            }
            Self::Empty => {}
            other => out.push(other),
        }
    }

    fn append_ir_children(graph: &mut UiIrGraph, parent_id: NodeId, children: &[NodeBlueprint]) {
        for child in children {
            match child {
                NodeBlueprint::Fragment(subchildren) => {
                    Self::append_ir_children(graph, parent_id, subchildren);
                }
                NodeBlueprint::Empty => {}
                _ => {
                    let child_id = child.compile_to_ir(graph);
                    if !child_id.is_null() {
                        graph.append_child(parent_id, child_id);
                    }
                }
            }
        }
    }

    /// Compiles this reactive blueprint tree into a contiguous 64-byte aligned `UiIrGraph`.
    pub fn compile_to_ir(&self, graph: &mut UiIrGraph) -> NodeId {
        match self {
            Self::Element {
                tag,
                classes,
                attributes,
                children,
            } => {
                let mut ir_tag = tag.to_ir_tag();
                if classes.iter().any(|c| c == "grid" || c.starts_with("grid-")) {
                    ir_tag = IrTag::GridContainer;
                } else if classes.iter().any(|c| c == "flex" || c.starts_with("flex-")) {
                    ir_tag = IrTag::FlexContainer;
                } else if classes.iter().any(|c| {
                    c == "overflow-auto"
                        || c == "overflow-scroll"
                        || c == "overflow-y-auto"
                        || c == "overflow-y-scroll"
                }) {
                    ir_tag = IrTag::ScrollView;
                }

                let mut flags = IrNodeFlags::IS_VISIBLE;
                for attr in attributes {
                    if matches!(attr, Attribute::EventListener { .. }) {
                        flags |= IrNodeFlags::HAS_EVENT_LISTENERS;
                    }
                }
                if classes.iter().any(|c| {
                    c == "contain-strict" || c == "contain-paint" || c == "overflow-hidden"
                }) {
                    flags |= IrNodeFlags::IS_CONTAINMENT_BOUNDARY;
                }

                let node_id = graph.alloc_node(ir_tag);
                let memo_hash = Self::compute_hash(ir_tag as u8, classes);
                if let Some(node) = graph.get_mut(node_id) {
                    node.flags = flags;
                    node.memo_hash = memo_hash;
                }

                Self::append_ir_children(graph, node_id, children);
                node_id
            }
            Self::Text(text) => {
                let node_id = graph.alloc_node(IrTag::Text);
                let mut h = 0xcbf29ce484222325u64;
                for &b in text.as_bytes() {
                    h ^= b as u64;
                    h = h.wrapping_mul(0x100000001b3u64);
                }
                if let Some(node) = graph.get_mut(node_id) {
                    node.flags = IrNodeFlags::IS_VISIBLE | IrNodeFlags::IS_TEXT_LEAF;
                    node.memo_hash = h;
                }
                node_id
            }
            Self::DynamicText(_) => {
                let node_id = graph.alloc_node(IrTag::Text);
                if let Some(node) = graph.get_mut(node_id) {
                    node.flags = IrNodeFlags::IS_VISIBLE | IrNodeFlags::IS_TEXT_LEAF;
                }
                node_id
            }
            Self::Fragment(children) => {
                if children.is_empty() {
                    return NodeId::NULL;
                }
                let node_id = graph.alloc_node(IrTag::ComponentRoot);
                if let Some(node) = graph.get_mut(node_id) {
                    node.flags = IrNodeFlags::IS_VISIBLE;
                }
                for child in children {
                    let child_id = child.compile_to_ir(graph);
                    if !child_id.is_null() {
                        graph.append_child(node_id, child_id);
                    }
                }
                node_id
            }
            Self::Empty => NodeId::NULL,
        }
    }

    fn compute_hash(tag_u8: u8, classes: &ClassList) -> u64 {
        let mut h = 0xcbf29ce484222325u64;
        h ^= tag_u8 as u64;
        h = h.wrapping_mul(0x100000001b3u64);
        for c in classes.iter() {
            for &b in c.as_bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x100000001b3u64);
            }
        }
        h
    }
}

/// Supported HTML tag variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tag {
    Div,
    Span,
    Button,
    Input,
    Select,
    Option,
    Textarea,
    Label,
    Form,
    Header,
    Nav,
    Main,
    Section,
    Aside,
    Footer,
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
    P,
    Ul,
    Ol,
    Li,
    A,
    Img,
    Svg,
    Path,
    Canvas,
    Table,
    Thead,
    Tbody,
    Tr,
    Th,
    Td,
    Hr,
    Br,
    Custom(&'static str),
}

impl Tag {
    /// Maps this DOM tag to its corresponding 1-byte IR tag variant.
    pub fn to_ir_tag(self) -> IrTag {
        match self {
            Tag::Div
            | Tag::Main
            | Tag::Section
            | Tag::Header
            | Tag::Footer
            | Tag::Nav
            | Tag::Aside
            | Tag::Form
            | Tag::Table
            | Tag::Thead
            | Tag::Tbody
            | Tag::Tr
            | Tag::Th
            | Tag::Td
            | Tag::Ul
            | Tag::Ol
            | Tag::Li
            | Tag::Hr => IrTag::Div,
            Tag::Span
            | Tag::Label
            | Tag::P
            | Tag::H1
            | Tag::H2
            | Tag::H3
            | Tag::H4
            | Tag::H5
            | Tag::H6
            | Tag::A
            | Tag::Br => IrTag::Span,
            Tag::Button => IrTag::Button,
            Tag::Input | Tag::Textarea | Tag::Select | Tag::Option => IrTag::Input,
            Tag::Img => IrTag::Image,
            Tag::Svg | Tag::Path => IrTag::Svg,
            Tag::Canvas => IrTag::Canvas3D,
            Tag::Custom(_) => IrTag::Div,
        }
    }
}

/// Universal trait converting Rust expressions into `NodeBlueprint` in RSX children.
pub trait IntoNodeBlueprint {
    fn into_node_blueprint(self) -> NodeBlueprint;
}

impl IntoNodeBlueprint for NodeBlueprint {
    #[inline]
    fn into_node_blueprint(self) -> NodeBlueprint {
        self
    }
}

impl IntoNodeBlueprint for View {
    #[inline]
    fn into_node_blueprint(self) -> NodeBlueprint {
        self.root
    }
}

impl IntoNodeBlueprint for () {
    #[inline]
    fn into_node_blueprint(self) -> NodeBlueprint {
        NodeBlueprint::Empty
    }
}

impl IntoNodeBlueprint for &str {
    #[inline]
    fn into_node_blueprint(self) -> NodeBlueprint {
        NodeBlueprint::Text(self.to_string())
    }
}

impl IntoNodeBlueprint for String {
    #[inline]
    fn into_node_blueprint(self) -> NodeBlueprint {
        NodeBlueprint::Text(self)
    }
}

impl IntoNodeBlueprint for &String {
    #[inline]
    fn into_node_blueprint(self) -> NodeBlueprint {
        NodeBlueprint::Text(self.clone())
    }
}

impl IntoNodeBlueprint for bool {
    #[inline]
    fn into_node_blueprint(self) -> NodeBlueprint {
        NodeBlueprint::Text(self.to_string())
    }
}

macro_rules! impl_into_node_blueprint_for_num {
    ($($t:ty),*) => {
        $(
            impl IntoNodeBlueprint for $t {
                #[inline]
                fn into_node_blueprint(self) -> NodeBlueprint {
                    NodeBlueprint::Text(self.to_string())
                }
            }
        )*
    };
}

impl_into_node_blueprint_for_num!(
    i8, i16, i32, i64, i128, isize,
    u8, u16, u32, u64, u128, usize,
    f32, f64
);

impl<T: IntoNodeBlueprint> IntoNodeBlueprint for Option<T> {
    #[inline]
    fn into_node_blueprint(self) -> NodeBlueprint {
        match self {
            Some(val) => val.into_node_blueprint(),
            None => NodeBlueprint::Empty,
        }
    }
}

impl<T: IntoNodeBlueprint> IntoNodeBlueprint for Vec<T> {
    #[inline]
    fn into_node_blueprint(self) -> NodeBlueprint {
        let children: Vec<NodeBlueprint> = self.into_iter().map(|item| item.into_node_blueprint()).collect();
        NodeBlueprint::Fragment(children)
    }
}

impl<I, F, T> IntoNodeBlueprint for std::iter::Map<I, F>
where
    I: Iterator,
    F: FnMut(I::Item) -> T,
    T: IntoNodeBlueprint,
{
    #[inline]
    fn into_node_blueprint(self) -> NodeBlueprint {
        let children: Vec<NodeBlueprint> = self.map(|item| item.into_node_blueprint()).collect();
        NodeBlueprint::Fragment(children)
    }
}

impl IntoNodeBlueprint for Rc<dyn Fn() -> String> {
    #[inline]
    fn into_node_blueprint(self) -> NodeBlueprint {
        NodeBlueprint::DynamicText(self)
    }
}

impl<T: std::fmt::Display + Clone + 'static> IntoNodeBlueprint for reactive::Signal<T> {
    #[inline]
    fn into_node_blueprint(self) -> NodeBlueprint {
        NodeBlueprint::DynamicText(Rc::new(move || self.get().to_string()))
    }
}

impl<T: std::fmt::Display + Clone + 'static> IntoNodeBlueprint for &reactive::Signal<T> {
    #[inline]
    fn into_node_blueprint(self) -> NodeBlueprint {
        let sig = self.clone();
        NodeBlueprint::DynamicText(Rc::new(move || sig.get().to_string()))
    }
}

impl<T: std::fmt::Display + Clone + 'static> IntoNodeBlueprint for reactive::ReadSignal<T> {
    #[inline]
    fn into_node_blueprint(self) -> NodeBlueprint {
        NodeBlueprint::DynamicText(Rc::new(move || self.get().to_string()))
    }
}

impl<T: std::fmt::Display + Clone + 'static> IntoNodeBlueprint for &reactive::ReadSignal<T> {
    #[inline]
    fn into_node_blueprint(self) -> NodeBlueprint {
        let sig = self.clone();
        NodeBlueprint::DynamicText(Rc::new(move || sig.get().to_string()))
    }
}

#[cfg(test)]
mod entity_tests {
    use super::*;

    #[test]
    fn test_node_blueprint_entity_id_assignment_and_lookup() {
        let id1 = EntityId::new(101);
        let id2 = EntityId::new(102);

        let bp = NodeBlueprint::element_simple(
            Tag::Div,
            "container",
            vec![
                NodeBlueprint::element_simple(Tag::P, "child", vec![NodeBlueprint::text("Hello")])
                    .with_entity_id(id2),
            ],
        )
        .with_entity_id(id1);

        assert_eq!(bp.entity_id(), Some(id1));
        let found1 = bp.find_by_entity_id(id1);
        assert!(found1.is_some());
        assert_eq!(found1.unwrap().entity_id(), Some(id1));

        let found2 = bp.find_by_entity_id(id2);
        assert!(found2.is_some());
        assert_eq!(found2.unwrap().entity_id(), Some(id2));

        assert!(bp.find_by_entity_id(EntityId::new(999)).is_none());
    }

    #[test]
    fn test_ensure_entity_ids_preserves_flattening() {
        let mut tree = NodeBlueprint::element_simple(
            Tag::Div,
            "root",
            vec![
                NodeBlueprint::fragment(vec![
                    NodeBlueprint::element_simple(Tag::Span, "first", vec![]),
                    NodeBlueprint::element_simple(Tag::Span, "second", vec![]),
                ]),
            ],
        );

        tree.ensure_entity_ids();
        let root_id = tree.entity_id().unwrap();
        assert!(root_id.is_valid());

        let flat = tree.flatten_fragments();
        assert_eq!(flat.entity_id(), Some(root_id));

        if let NodeBlueprint::Element { children, .. } = &flat {
            assert_eq!(children.len(), 2);
            assert!(children[0].entity_id().is_some());
            assert!(children[1].entity_id().is_some());
            assert_ne!(children[0].entity_id(), children[1].entity_id());
        } else {
            panic!("Expected Element node");
        }
    }
}


