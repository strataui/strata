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

pub mod events;
pub mod ir;
pub mod reactive;
pub mod spring;

pub use events::{Event, EventDispatcher, EventPhase, EventType};
pub use ir::{
    DisplayChunkRange, FixedRect, IrNodeFlags, IrTag, MemoCache, MemoEntry, NodeId, StackGuard,
    UiIrGraph, UiIrNode,
};
pub use reactive::{
    batch, create_signal, untrack, use_effect, use_memo, use_signal, use_state, Effect, Memo,
    ReactiveRuntime, ReadSignal, ScopeId, SetSignal, Signal, SignalId, SubscriberId, WriteSignal,
};
pub use spring::{SpringConfig, SpringSignal};

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
}

impl Attribute {
    pub fn property(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self::Property {
            key: key.into(),
            value: value.into(),
        }
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
        }
    }
}

/// Core reactive blueprint tree representation.
#[derive(Clone)]
pub enum NodeBlueprint {
    Element {
        tag: Tag,
        classes: Vec<String>,
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
        classes: Vec<String>,
        attributes: Vec<Attribute>,
        children: Vec<NodeBlueprint>,
    ) -> Self {
        Self::Element {
            tag,
            classes,
            attributes,
            children,
        }
    }

    pub fn element_simple(
        tag: Tag,
        classes: Vec<String>,
        children: Vec<NodeBlueprint>,
    ) -> Self {
        Self::Element {
            tag,
            classes,
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

impl IntoNodeBlueprint for &'static str {
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

