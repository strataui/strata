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

impl Default for View {
    #[inline]
    fn default() -> Self {
        Self::new(NodeBlueprint::Empty)
    }
}

pub mod classes;
pub mod clipboard;
pub mod error_boundary;
pub mod events;
pub mod hooks;
pub mod ir;
pub mod portal;
pub mod reactive;
pub mod spring;
pub mod text_input;

pub use classes::{
    standard_class_index, ClassBitmask, ClassIter, ClassList, IntoClassList, STANDARD_CLASSES,
};
pub use clipboard::{
    clear_system_clipboard_bridge, get_clipboard_text, register_system_clipboard_bridge,
    set_clipboard_text,
};
pub use error_boundary::{ErrorBoundary, ErrorBoundaryBuilder, ErrorBoundaryInfo};
pub use events::{Event, EventDispatcher, EventPhase, EventType};
pub use hooks::*;
pub use ir::{
    ChildIter, DisplayChunkRange, FixedRect, IrNodeFlags, IrTag, MemoCache, MemoEntry, NodeId,
    StackGuard, UiIrGraph, UiIrNode,
};
pub use portal::{
    create_anchored_portal, create_dropdown_portal, create_portal, AnchorPlacement, Portal,
    PortalBuilder,
};
pub use reactive::{
    batch, clear_hmr_signals, clear_persistent_signals, create_context, create_signal,
    export_hmr_signals, has_dirty_signals, import_hmr_signals, mark_dirty, peek_dirty_signal_names,
    provide_context, reactive_telemetry_snapshot, reset_component_hooks, set_signal_name,
    signal_name, take_dirty_signal_names, take_dirty_signals, untrack, use_callback, use_context,
    use_effect, use_effect_with_deps, use_hmr_signal, use_id, use_memo, use_memo_with_deps,
    use_persistent_signal, use_reducer, use_reducer_init, use_ref, use_signal, use_state,
    with_component_key, with_root_component_scope, ComponentScopeGuard, Context, Effect,
    EffectGuard, HmrCodec, IntoEffectCleanup, IntoInitialState, Memo, ReactiveRuntime,
    ReactiveTelemetrySnapshot, ReadSignal, Ref, ScopeId, SetSignal, Signal, SignalId,
    SignalTelemetryEntry, SubscriberId, WriteSignal,
};
pub use spring::{SpringConfig, SpringSignal};
pub use strata_primitives::EntityId;
pub use text_input::{InputKey, InputKeyResult, InputModifiers, TextInputState};

/// Ergonomic conversion into `Option<String>` for `#[component]` builder setters.
///
/// Accepts string literals (`"hello"`), `String`, `&String`, `Cow<'_, str>`, `Option<String>`,
/// and bare `None` without `.to_string()` boilerplate or type-inference ambiguity.
pub trait IntoOptionalString {
    fn into_optional_string(self) -> Option<String>;
}

impl IntoOptionalString for String {
    #[inline]
    fn into_optional_string(self) -> Option<String> {
        Some(self)
    }
}

impl IntoOptionalString for &str {
    #[inline]
    fn into_optional_string(self) -> Option<String> {
        Some(self.to_string())
    }
}

impl IntoOptionalString for &String {
    #[inline]
    fn into_optional_string(self) -> Option<String> {
        Some(self.clone())
    }
}

impl IntoOptionalString for std::borrow::Cow<'_, str> {
    #[inline]
    fn into_optional_string(self) -> Option<String> {
        Some(self.into_owned())
    }
}

impl IntoOptionalString for Option<String> {
    #[inline]
    fn into_optional_string(self) -> Option<String> {
        self
    }
}

/// Converts RSX children lists (`Vec<NodeBlueprint>`), single `NodeBlueprint`, or `View` values
/// into the `children` parameter type declared on a `#[component]` (`View`, `NodeBlueprint`,
/// `Vec<NodeBlueprint>`, or their `Option<...>` forms).
pub trait IntoChildren<Target> {
    fn into_children(self) -> Target;
}

impl IntoChildren<Vec<NodeBlueprint>> for Vec<NodeBlueprint> {
    #[inline]
    fn into_children(self) -> Vec<NodeBlueprint> {
        self
    }
}

impl IntoChildren<Vec<NodeBlueprint>> for Option<Vec<NodeBlueprint>> {
    #[inline]
    fn into_children(self) -> Vec<NodeBlueprint> {
        self.unwrap_or_default()
    }
}

impl IntoChildren<Vec<NodeBlueprint>> for NodeBlueprint {
    #[inline]
    fn into_children(mut self) -> Vec<NodeBlueprint> {
        match &mut self {
            NodeBlueprint::Empty => Vec::new(),
            NodeBlueprint::Fragment(children) => std::mem::take(children),
            _ => vec![self],
        }
    }
}

impl IntoChildren<Vec<NodeBlueprint>> for View {
    #[inline]
    fn into_children(self) -> Vec<NodeBlueprint> {
        self.root.into_children()
    }
}

impl IntoChildren<View> for View {
    #[inline]
    fn into_children(self) -> View {
        self
    }
}

impl IntoChildren<View> for NodeBlueprint {
    #[inline]
    fn into_children(self) -> View {
        View::new(self)
    }
}

impl IntoChildren<View> for Vec<NodeBlueprint> {
    #[inline]
    fn into_children(mut self) -> View {
        match self.len() {
            0 => View::new(NodeBlueprint::Empty),
            1 => View::new(self.remove(0)),
            _ => View::new(NodeBlueprint::Fragment(self)),
        }
    }
}

impl IntoChildren<View> for Option<Vec<NodeBlueprint>> {
    #[inline]
    fn into_children(self) -> View {
        self.unwrap_or_default().into_children()
    }
}

impl IntoChildren<NodeBlueprint> for NodeBlueprint {
    #[inline]
    fn into_children(self) -> NodeBlueprint {
        self
    }
}

impl IntoChildren<NodeBlueprint> for View {
    #[inline]
    fn into_children(self) -> NodeBlueprint {
        self.root
    }
}

impl IntoChildren<NodeBlueprint> for Vec<NodeBlueprint> {
    #[inline]
    fn into_children(self) -> NodeBlueprint {
        IntoChildren::<View>::into_children(self).root
    }
}

impl IntoChildren<Option<Vec<NodeBlueprint>>> for Option<Vec<NodeBlueprint>> {
    #[inline]
    fn into_children(self) -> Option<Vec<NodeBlueprint>> {
        self
    }
}

impl IntoChildren<Option<Vec<NodeBlueprint>>> for Vec<NodeBlueprint> {
    #[inline]
    fn into_children(self) -> Option<Vec<NodeBlueprint>> {
        Some(self)
    }
}

impl IntoChildren<Option<Vec<NodeBlueprint>>> for View {
    #[inline]
    fn into_children(self) -> Option<Vec<NodeBlueprint>> {
        Some(self.into_children())
    }
}

impl IntoChildren<Option<Vec<NodeBlueprint>>> for NodeBlueprint {
    #[inline]
    fn into_children(self) -> Option<Vec<NodeBlueprint>> {
        Some(self.into_children())
    }
}

impl IntoChildren<Option<View>> for Option<View> {
    #[inline]
    fn into_children(self) -> Option<View> {
        self
    }
}

impl IntoChildren<Option<View>> for View {
    #[inline]
    fn into_children(self) -> Option<View> {
        Some(self)
    }
}

impl IntoChildren<Option<View>> for Vec<NodeBlueprint> {
    #[inline]
    fn into_children(self) -> Option<View> {
        Some(self.into_children())
    }
}

impl IntoChildren<Option<View>> for NodeBlueprint {
    #[inline]
    fn into_children(self) -> Option<View> {
        Some(View::new(self))
    }
}

impl IntoChildren<Option<NodeBlueprint>> for Option<NodeBlueprint> {
    #[inline]
    fn into_children(self) -> Option<NodeBlueprint> {
        self
    }
}

impl IntoChildren<Option<NodeBlueprint>> for NodeBlueprint {
    #[inline]
    fn into_children(self) -> Option<NodeBlueprint> {
        Some(self)
    }
}

impl IntoChildren<Option<NodeBlueprint>> for Vec<NodeBlueprint> {
    #[inline]
    fn into_children(self) -> Option<NodeBlueprint> {
        Some(self.into_children())
    }
}

impl IntoChildren<Option<NodeBlueprint>> for View {
    #[inline]
    fn into_children(self) -> Option<NodeBlueprint> {
        Some(self.root)
    }
}

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

impl<F: Fn(Event) + 'static> EventHandlerMarker<(Rc<()>, Event)> for Rc<F> {
    fn call_handler(&self, event: Event) {
        (self)(event);
    }
}

impl<F: Fn() + 'static> EventHandlerMarker<(Rc<()>,)> for Rc<F> {
    fn call_handler(&self, _event: Event) {
        (self)();
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

impl ValueHandlerMarker<(String,)> for Rc<dyn Fn(String)> {
    fn call_handler(&self, val: String) {
        (self)(val);
    }
}

impl ValueHandlerMarker<()> for Rc<dyn Fn()> {
    fn call_handler(&self, _val: String) {
        (self)();
    }
}

impl<F: Fn(String) + 'static> ValueHandlerMarker<(Rc<()>, String)> for Rc<F> {
    fn call_handler(&self, val: String) {
        (self)(val);
    }
}

impl<F: Fn() + 'static> ValueHandlerMarker<(Rc<()>,)> for Rc<F> {
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
    /// Thread-safe type-erased 3D scene handle (e.g. `Arc<strata_render::Scene3D>`) attached to a viewport element.
    ScenePayload(std::sync::Arc<dyn std::any::Any + Send + Sync>),
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

    pub fn scene_payload<T: Send + Sync + 'static>(payload: T) -> Self {
        Self::ScenePayload(std::sync::Arc::new(payload))
    }

    pub fn scene_payload_arc(payload: std::sync::Arc<dyn std::any::Any + Send + Sync>) -> Self {
        Self::ScenePayload(payload)
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
            Self::ScenePayload(_) => f.debug_tuple("ScenePayload").field(&"<scene_3d>").finish(),
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

impl Default for NodeBlueprint {
    #[inline]
    fn default() -> Self {
        Self::Empty
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
            if let Some(pos) = attributes
                .iter()
                .position(|a| matches!(a, Attribute::Entity(_)))
            {
                attributes[pos] = Attribute::Entity(entity_id);
            } else {
                attributes.push(Attribute::Entity(entity_id));
            }
        }
    }

    /// Iteratively ensures all Element nodes in this blueprint tree have a valid EntityId
    /// without consuming call-stack frames on deeply nested trees.
    pub fn ensure_entity_ids(&mut self) {
        let mut stack: Vec<&mut NodeBlueprint> = vec![self];
        while let Some(node) = stack.pop() {
            match node {
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
                    for child in children.iter_mut().rev() {
                        stack.push(child);
                    }
                }
                Self::Fragment(children) => {
                    for child in children.iter_mut().rev() {
                        stack.push(child);
                    }
                }
                _ => {}
            }
        }
    }

    /// Iteratively locates an element blueprint node by its EntityId (O(1) call stack).
    pub fn find_by_entity_id(&self, target_id: EntityId) -> Option<&NodeBlueprint> {
        if !target_id.is_valid() {
            return None;
        }
        let mut stack = vec![self];
        while let Some(node) = stack.pop() {
            if node.entity_id() == Some(target_id) {
                return Some(node);
            }
            match node {
                Self::Element { children, .. } | Self::Fragment(children) => {
                    for child in children.iter().rev() {
                        stack.push(child);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Iteratively locates a mutable element blueprint node by its EntityId (O(1) call stack).
    pub fn find_by_entity_id_mut(&mut self, target_id: EntityId) -> Option<&mut NodeBlueprint> {
        if !target_id.is_valid() {
            return None;
        }
        let mut stack = vec![self];
        while let Some(node) = stack.pop() {
            if node.entity_id() == Some(target_id) {
                return Some(node);
            }
            match node {
                Self::Element { children, .. } | Self::Fragment(children) => {
                    for child in children.iter_mut().rev() {
                        stack.push(child);
                    }
                }
                _ => {}
            }
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

    /// Iteratively locates an element blueprint node by its DOM node UID (O(1) call stack).
    pub fn find_by_dom_uid(&self, target_uid: u64) -> Option<&NodeBlueprint> {
        let mut stack = vec![self];
        while let Some(node) = stack.pop() {
            if node.dom_uid() == Some(target_uid) {
                return Some(node);
            }
            match node {
                Self::Element { children, .. } | Self::Fragment(children) => {
                    for child in children.iter().rev() {
                        stack.push(child);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Iteratively locates a mutable element blueprint node by its DOM node UID (O(1) call stack).
    pub fn find_by_dom_uid_mut(&mut self, target_uid: u64) -> Option<&mut NodeBlueprint> {
        let mut stack = vec![self];
        while let Some(node) = stack.pop() {
            if node.dom_uid() == Some(target_uid) {
                return Some(node);
            }
            match node {
                Self::Element { children, .. } | Self::Fragment(children) => {
                    for child in children.iter_mut().rev() {
                        stack.push(child);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Iteratively calculates the index path to the element blueprint node with matching DOM node UID (O(1) call stack).
    pub fn find_path_by_dom_uid(&self, target_uid: u64) -> Option<Vec<usize>> {
        if self.dom_uid() == Some(target_uid) {
            return Some(Vec::new());
        }
        let mut path = Vec::new();
        let mut stack: Vec<(&[NodeBlueprint], usize)> = Vec::new();
        match self {
            Self::Element { children, .. } | Self::Fragment(children) if !children.is_empty() => {
                stack.push((children.as_slice(), 0));
            }
            _ => return None,
        }
        while let Some((slice, idx)) = stack.last_mut() {
            if *idx >= slice.len() {
                stack.pop();
                path.pop();
                continue;
            }
            let current_idx = *idx;
            let child = &slice[current_idx];
            *idx += 1;
            path.push(current_idx);
            if child.dom_uid() == Some(target_uid) {
                return Some(path);
            }
            match child {
                Self::Element { children, .. } | Self::Fragment(children)
                    if !children.is_empty() =>
                {
                    stack.push((children.as_slice(), 0));
                }
                _ => {
                    path.pop();
                }
            }
        }
        None
    }

    /// Iteratively calculates the index path to the element blueprint node with matching EntityId (O(1) call stack).
    pub fn find_path_by_entity_id(&self, target_id: EntityId) -> Option<Vec<usize>> {
        if !target_id.is_valid() {
            return None;
        }
        if self.entity_id() == Some(target_id) {
            return Some(Vec::new());
        }
        let mut path = Vec::new();
        let mut stack: Vec<(&[NodeBlueprint], usize)> = Vec::new();
        match self {
            Self::Element { children, .. } | Self::Fragment(children) if !children.is_empty() => {
                stack.push((children.as_slice(), 0));
            }
            _ => return None,
        }
        while let Some((slice, idx)) = stack.last_mut() {
            if *idx >= slice.len() {
                stack.pop();
                path.pop();
                continue;
            }
            let current_idx = *idx;
            let child = &slice[current_idx];
            *idx += 1;
            path.push(current_idx);
            if child.entity_id() == Some(target_id) {
                return Some(path);
            }
            match child {
                Self::Element { children, .. } | Self::Fragment(children)
                    if !children.is_empty() =>
                {
                    stack.push((children.as_slice(), 0));
                }
                _ => {
                    path.pop();
                }
            }
        }
        None
    }

    /// Iteratively flattens nested Fragment nodes within Element children,
    /// splicing fragment children directly into their parent elements without deep recursion.
    pub fn flatten_fragments(mut self) -> Self {
        let mut stack: Vec<&mut NodeBlueprint> = vec![&mut self];
        while let Some(node) = stack.pop() {
            let children_opt = match node {
                Self::Element { children, .. } | Self::Fragment(children) => Some(children),
                _ => None,
            };
            if let Some(children) = children_opt {
                let needs_flatten = children
                    .iter()
                    .any(|c| matches!(c, Self::Fragment(_) | Self::Empty));
                if needs_flatten {
                    let old = std::mem::take(children);
                    let mut flat = Vec::with_capacity(old.len());
                    let mut splice_stack: Vec<NodeBlueprint> = old.into_iter().rev().collect();
                    while let Some(mut item) = splice_stack.pop() {
                        match &mut item {
                            Self::Fragment(sub) => {
                                for s in std::mem::take(sub).into_iter().rev() {
                                    splice_stack.push(s);
                                }
                            }
                            Self::Empty => {}
                            _ => flat.push(item),
                        }
                    }
                    *children = flat;
                }
                for child in children.iter_mut().rev() {
                    stack.push(child);
                }
            }
        }
        self
    }

    fn append_ir_children(
        graph: &mut UiIrGraph,
        parent_id: NodeId,
        children: &[NodeBlueprint],
        depth: usize,
    ) {
        if depth >= 256 {
            return;
        }
        for child in children {
            match child {
                NodeBlueprint::Fragment(subchildren) => {
                    Self::append_ir_children(graph, parent_id, subchildren, depth + 1);
                }
                NodeBlueprint::Empty => {}
                _ => {
                    let child_id = child.compile_to_ir_depth(graph, depth + 1);
                    if !child_id.is_null() {
                        graph.append_child(parent_id, child_id);
                    }
                }
            }
        }
    }

    /// Compiles this reactive blueprint tree into a contiguous 64-byte aligned `UiIrGraph`.
    pub fn compile_to_ir(&self, graph: &mut UiIrGraph) -> NodeId {
        self.compile_to_ir_depth(graph, 0)
    }

    fn compile_to_ir_depth(&self, graph: &mut UiIrGraph, depth: usize) -> NodeId {
        if depth >= 256 {
            return NodeId::NULL;
        }
        match self {
            Self::Element {
                tag,
                classes,
                attributes,
                children,
            } => {
                let mut ir_tag = tag.to_ir_tag();
                if classes
                    .iter()
                    .any(|c| c == "grid" || c.starts_with("grid-"))
                {
                    ir_tag = IrTag::GridContainer;
                } else if classes
                    .iter()
                    .any(|c| c == "flex" || c.starts_with("flex-"))
                {
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

                Self::append_ir_children(graph, node_id, children, depth);
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
                    let child_id = child.compile_to_ir_depth(graph, depth + 1);
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
    Audio,
    Video,
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
            | Tag::Hr
            | Tag::Audio => IrTag::Div,
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
            Tag::Img | Tag::Video => IrTag::Image,
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
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64
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
        let children: Vec<NodeBlueprint> = self
            .into_iter()
            .map(|item| item.into_node_blueprint())
            .collect();
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

impl Drop for NodeBlueprint {
    fn drop(&mut self) {
        let initial = match self {
            Self::Element { children, .. } | Self::Fragment(children) if !children.is_empty() => {
                std::mem::take(children)
            }
            _ => return,
        };
        drop_blueprint_children_iterative(initial);
    }
}

#[inline(never)]
fn drop_blueprint_children_iterative(mut stack: Vec<NodeBlueprint>) {
    while let Some(mut node) = stack.pop() {
        match &mut node {
            NodeBlueprint::Element { children, .. } | NodeBlueprint::Fragment(children)
                if !children.is_empty() =>
            {
                stack.append(children);
            }
            _ => {}
        }
        // `node` now has empty `children`, so its `Drop` exits immediately on the fast path.
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
            vec![NodeBlueprint::element_simple(
                Tag::P,
                "child",
                vec![NodeBlueprint::text("Hello")],
            )
            .with_entity_id(id2)],
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
            vec![NodeBlueprint::fragment(vec![
                NodeBlueprint::element_simple(Tag::Span, "first", vec![]),
                NodeBlueprint::element_simple(Tag::Span, "second", vec![]),
            ])],
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

    #[test]
    fn test_10000_deep_node_blueprint_lookup_and_drop_on_1mb_stack() {
        std::thread::Builder::new()
            .name("dom-1mb-stack".to_string())
            .stack_size(1024 * 1024)
            .spawn(|| {
                let target_id = EntityId::new(777_777);
                let mut cur = NodeBlueprint::element_simple(Tag::Span, "leaf", vec![])
                    .with_entity_id(target_id);
                for _ in 0..10_000 {
                    cur = NodeBlueprint::element_simple(Tag::Div, "wrap", vec![cur]);
                }
                assert!(cur.find_by_entity_id(target_id).is_some());
                let path = cur
                    .find_path_by_entity_id(target_id)
                    .expect("deep path lookup must succeed");
                assert_eq!(path.len(), 10_000);
                drop(cur);
            })
            .expect("spawn 1MB thread")
            .join()
            .expect("10,000-deep NodeBlueprint lookup & drop must not overflow 1 MiB stack");
    }

    #[test]
    fn test_dom_clipboard_fallback_roundtrip() {
        clear_system_clipboard_bridge();
        set_clipboard_text("strata clipboard payload");
        assert_eq!(
            get_clipboard_text().as_deref(),
            Some("strata clipboard payload")
        );
    }
}
