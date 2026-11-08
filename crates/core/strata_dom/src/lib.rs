//! Strata DOM: Fine-grained reactive signal graph, DOM representations, and node blueprints.

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

                for child in children {
                    let child_id = child.compile_to_ir(graph);
                    if !child_id.is_null() {
                        graph.append_child(node_id, child_id);
                    }
                }
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

