//! Runtime data structures for active focus, scrollbars, hit targets, and editing.

/// Hit-test target result representing a clickable text input element in the blueprint tree.
#[derive(Clone)]
pub struct InputHitTarget {
    pub input_index: usize,
    pub input_id: Option<String>,
    pub entity_id: Option<strata_primitives::EntityId>,
    pub is_multiline: bool,
    pub current_value: String,
    pub font_size: f32,
    pub line_height: f32,
    pub is_mono: bool,
    pub font_family: strata_primitives::AtomId,
    pub content_box_x: f32,
    pub content_box_y: f32,
    pub content_box_width: f32,
    pub content_box_height: f32,
    pub on_input: Option<std::rc::Rc<dyn Fn(strata_dom::Event)>>,
    pub on_change: Option<std::rc::Rc<dyn Fn(strata_dom::Event)>>,
    pub on_submit: Option<std::rc::Rc<dyn Fn(strata_dom::Event)>>,
    pub on_focus: Option<std::rc::Rc<dyn Fn(strata_dom::Event)>>,
    pub on_blur: Option<std::rc::Rc<dyn Fn(strata_dom::Event)>>,
}

/// Active text input state tracked by the runtime while an input has keyboard focus.
#[derive(Clone)]
pub struct ActiveInput {
    pub input_index: usize,
    pub input_id: Option<String>,
    pub entity_id: Option<strata_primitives::EntityId>,
    pub is_multiline: bool,
    pub state: strata_dom::text_input::TextInputState,
    pub on_input: Option<std::rc::Rc<dyn Fn(strata_dom::Event)>>,
    pub on_change: Option<std::rc::Rc<dyn Fn(strata_dom::Event)>>,
    pub on_submit: Option<std::rc::Rc<dyn Fn(strata_dom::Event)>>,
    pub on_blur: Option<std::rc::Rc<dyn Fn(strata_dom::Event)>>,
}

impl std::fmt::Debug for ActiveInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActiveInput")
            .field("text", &self.state.text.get())
            .field("cursor", &self.state.cursor_index.get())
            .finish()
    }
}

/// Active scrollbar state for an overflow container.
#[derive(Debug, Clone)]
pub struct ActiveScrollbar {
    pub bounds: strata_layout::Rect,
    pub clip_bounds: strata_layout::Rect,
    pub border_radius: strata_style::types::BorderRadius,
    pub is_top_layer: bool,
    pub track_bounds: strata_layout::Rect,
    pub thumb_bounds: strata_layout::Rect,
    pub scroll_y: f32,
    pub max_scroll_y: f32,
    pub content_h: f32,
    pub is_id: bool,
    pub id: String,
    pub path: Vec<usize>,
    pub available_travel: f32,
}

/// Active drag operation on a scrollbar thumb.
#[derive(Debug, Clone)]
pub struct ScrollbarDrag {
    pub is_id: bool,
    pub id: String,
    pub path: Vec<usize>,
    pub start_pointer_y: f32,
    pub start_scroll_y: f32,
    pub available_travel: f32,
    pub max_scroll_y: f32,
}

/// Active kinetic smooth scroll animation tracking state.
#[derive(Debug, Clone)]
pub struct SmoothScrollTracker {
    pub container_path: Vec<usize>,
    pub container_id: String,
    pub is_id: bool,
    pub max_scroll_y: f32,
    pub available_travel: f32,
    pub bounds_y: f32,
    /// Current interpolated scroll offset in pixels.
    pub current_y: f32,
    /// Target scroll position in pixels.
    pub target_y: f32,
    /// Initial position at the start of the current impulse.
    pub start_y: f32,
    /// Initial velocity in px/s at the start of the current impulse.
    pub initial_velocity: f32,
    /// Timestamp when the current impulse started.
    pub start_time: std::time::Instant,
    /// Timestamp of the last incoming wheel event (for flick velocity calculation).
    pub last_event_time: std::time::Instant,
    /// Critically damped angular frequency (rad/s).
    pub omega: f32,
}

/// Internal descriptor of a scrollable target container under the cursor.
#[derive(Debug, Clone)]
pub struct ScrollTarget {
    pub path: Vec<usize>,
    pub id: Option<String>,
    pub bounds: strata_layout::Rect,
    pub max_scroll_x: f32,
    pub max_scroll_y: f32,
}
/// Computes a deterministic 64-bit ID from a hierarchical tree path.
pub fn path_to_node_id(path: &[usize]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for &idx in path {
        hash ^= idx as u64;
        hash = hash.wrapping_mul(0x100000001b3u64);
    }
    hash
}

/// Editing commands that can be applied to the focused text input from any UI surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditCommand {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
}

/// Action result from a pointer-down event dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerDownAction {
    /// Not handled by any element or active input.
    Ignored,
    /// Caret or selection inside the already-active focused input moved. Requires redraw only.
    CaretMoved,
    /// Focus transitioned to a new or different input. Requires layout/rebuild update.
    FocusChanged,
    /// DOM PointerDown or MouseDown event listener was invoked.
    DomListenerInvoked,
}

impl PointerDownAction {
    #[inline]
    pub fn is_handled(self) -> bool {
        self != PointerDownAction::Ignored
    }

    #[inline]
    pub fn needs_rebuild(self) -> bool {
        matches!(
            self,
            PointerDownAction::FocusChanged | PointerDownAction::DomListenerInvoked
        )
    }

    #[inline]
    pub fn needs_redraw(self) -> bool {
        self != PointerDownAction::Ignored
    }
}

/// Action result from a pointer-move event dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerMoveAction {
    /// Not handled.
    Ignored,
    /// Mouse drag text selection range updated in active input. Requires redraw only.
    SelectionDragged,
    /// DOM PointerMove or MouseMove event listener was invoked.
    DomListenerInvoked,
}

impl PointerMoveAction {
    #[inline]
    pub fn is_handled(self) -> bool {
        self != PointerMoveAction::Ignored
    }

    #[inline]
    pub fn needs_rebuild(self) -> bool {
        self == PointerMoveAction::DomListenerInvoked
    }

    #[inline]
    pub fn needs_redraw(self) -> bool {
        self != PointerMoveAction::Ignored
    }
}

/// Action result from a pointer-up event dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerUpAction {
    /// Not handled.
    Ignored,
    /// Mouse drag text selection finished. Requires redraw only.
    SelectionEnded,
    /// DOM PointerUp or MouseUp event listener was invoked.
    DomListenerInvoked,
}

impl PointerUpAction {
    #[inline]
    pub fn is_handled(self) -> bool {
        self != PointerUpAction::Ignored
    }

    #[inline]
    pub fn needs_rebuild(self) -> bool {
        self == PointerUpAction::DomListenerInvoked
    }

    #[inline]
    pub fn needs_redraw(self) -> bool {
        self != PointerUpAction::Ignored
    }
}

/// Outcome of dispatching a platform keyboard or text event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyDispatchResult {
    /// Key was not handled by any focused input or container scroll.
    Ignored,
    /// Caret moved, selection range updated, or page scrolled without mutating component state. Requires redraw only.
    RedrawOnly,
    /// Text buffer was modified or form submitted/blurred, requiring layout/signal recalculation.
    NeedsRebuild,
}

impl KeyDispatchResult {
    #[inline]
    pub fn is_handled(self) -> bool {
        self != KeyDispatchResult::Ignored
    }

    #[inline]
    pub fn needs_rebuild(self) -> bool {
        self == KeyDispatchResult::NeedsRebuild
    }

    #[inline]
    pub fn needs_redraw(self) -> bool {
        self != KeyDispatchResult::Ignored
    }
}

/// Binding between a dynamic text closure and its target location in the layout hierarchy.
#[derive(Clone)]
pub struct DynamicTextBinding {
    pub path: Vec<usize>,
    pub closure: std::rc::Rc<dyn Fn() -> String>,
    pub last_value: String,
}

impl std::fmt::Debug for DynamicTextBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DynamicTextBinding")
            .field("path", &self.path)
            .field("last_value", &self.last_value)
            .finish()
    }
}

/// Interaction context describing active pointer, keyboard focus, and theme interaction state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InteractionContext<'a> {
    pub hovered_path: Option<&'a [usize]>,
    pub focused_path: Option<&'a [usize]>,
    pub is_mouse_down: bool,
    pub is_dark_mode: bool,
}

/// Element selector metadata used when matching `<style>` rules across ancestors and siblings.
#[derive(Debug, Clone)]
pub(crate) struct CascadeElementMeta {
    pub tag: &'static str,
    pub classes: Vec<String>,
    pub id: Option<String>,
    pub attributes: Vec<(String, String)>,
    pub pseudo_state: strata_style::selector::PseudoState,
}

/// Active DOM drag-and-drop interaction state (`ondragstart`, `ondrag`, `ondragenter`, `ondragover`, `ondragleave`, `ondrop`, `ondragend`).
#[derive(Debug, Clone)]
pub struct ActiveDomDrag {
    pub source_path: Vec<usize>,
    pub start_pos: (f32, f32),
    pub started: bool,
    pub last_over_path: Option<Vec<usize>>,
    pub data_transfer: std::rc::Rc<std::cell::RefCell<std::collections::HashMap<String, String>>>,
}

/// Precomputed stylesheet selector dependency index using 64-bit Bloom filters and
/// exact tag/class/id sets so static `<style>` sheets do not disable incremental
/// layout reconciliation or fast paint-only hover restyling across unrelated subtrees.
#[derive(Debug, Clone, Default)]
pub(crate) struct StylesheetSelectorIndex {
    pub hover_bloom: u64,
    pub focus_bloom: u64,
    pub hover_tags: std::collections::HashSet<String>,
    pub hover_classes: std::collections::HashSet<String>,
    pub hover_ids: std::collections::HashSet<String>,
    pub focus_tags: std::collections::HashSet<String>,
    pub focus_classes: std::collections::HashSet<String>,
    pub focus_ids: std::collections::HashSet<String>,
    pub has_any_hover_rule: bool,
    pub has_any_focus_rule: bool,
    pub has_unbounded_hover_rule: bool,
    pub has_unbounded_focus_rule: bool,
    pub has_sibling_hover_rule: bool,
    pub has_sibling_combinator_or_positional_rules: bool,
    pub has_descendant_combinator_rules: bool,
}

impl StylesheetSelectorIndex {
    #[inline]
    fn bloom_bits(s: &str) -> u64 {
        let mut h1: u64 = 0xcbf29ce484222325;
        let mut h2: u64 = 0x84222325cbf29ce4;
        for b in s.bytes() {
            let lb = b.to_ascii_lowercase() as u64;
            h1 ^= lb;
            h1 = h1.wrapping_mul(0x100000001b3);
            h2 = h2.wrapping_mul(0x9e3779b97f4a7c15) ^ lb;
        }
        (1u64 << (h1 & 63)) | (1u64 << ((h2 >> 16) & 63))
    }

    fn compound_has_pseudo(comp: &strata_style::selector::CompoundSelector, target: &str) -> bool {
        comp.parts.iter().any(|p| match p {
            strata_style::selector::SelectorPart::PseudoClass(name) => {
                let n = name.trim().to_ascii_lowercase();
                n == target || n.contains(&format!(":{target}"))
            }
            _ => false,
        })
    }

    fn compound_has_positional_pseudo(comp: &strata_style::selector::CompoundSelector) -> bool {
        comp.parts.iter().any(|p| match p {
            strata_style::selector::SelectorPart::PseudoClass(name) => {
                let n = name.trim().to_ascii_lowercase();
                n.starts_with("nth-")
                    || n == "first-child"
                    || n == "last-child"
                    || n == "only-child"
                    || n == "first-of-type"
                    || n == "last-of-type"
                    || n == "only-of-type"
                    || n == "empty"
            }
            _ => false,
        })
    }

    fn index_compound_keys(
        comp: &strata_style::selector::CompoundSelector,
        bloom: &mut u64,
        tags: &mut std::collections::HashSet<String>,
        classes: &mut std::collections::HashSet<String>,
        ids: &mut std::collections::HashSet<String>,
        unbounded: &mut bool,
    ) {
        let mut has_key = false;
        for part in &comp.parts {
            match part {
                strata_style::selector::SelectorPart::Tag(t) => {
                    let k = t.to_ascii_lowercase();
                    *bloom |= Self::bloom_bits(&k);
                    tags.insert(k);
                    has_key = true;
                }
                strata_style::selector::SelectorPart::Class(c) => {
                    let k = c.to_ascii_lowercase();
                    *bloom |= Self::bloom_bits(&k);
                    classes.insert(k);
                    has_key = true;
                }
                strata_style::selector::SelectorPart::Id(id) => {
                    let k = id.to_ascii_lowercase();
                    *bloom |= Self::bloom_bits(&k);
                    ids.insert(k);
                    has_key = true;
                }
                _ => {}
            }
        }
        if !has_key {
            *unbounded = true;
        }
    }

    pub fn from_stylesheets(sheets: &[std::sync::Arc<strata_style::StyleSheet>]) -> Self {
        let mut idx = Self::default();
        for sheet in sheets {
            for rule in &sheet.rules {
                for sel in &rule.selectors {
                    if !sel.steps.is_empty() {
                        idx.has_descendant_combinator_rules = true;
                    }
                    let has_sibling_comb = sel.steps.iter().any(|(comb, _)| {
                        matches!(
                            comb,
                            strata_style::selector::Combinator::AdjacentSibling
                                | strata_style::selector::Combinator::SubsequentSibling
                        )
                    });
                    let has_positional = Self::compound_has_positional_pseudo(&sel.target)
                        || sel
                            .steps
                            .iter()
                            .any(|(_, c)| Self::compound_has_positional_pseudo(c));
                    if has_sibling_comb || has_positional {
                        idx.has_sibling_combinator_or_positional_rules = true;
                    }

                    if sel.has_any_hover_pseudo() {
                        idx.has_any_hover_rule = true;
                        if has_sibling_comb {
                            idx.has_sibling_hover_rule = true;
                        }
                        if Self::compound_has_pseudo(&sel.target, "hover") {
                            Self::index_compound_keys(
                                &sel.target,
                                &mut idx.hover_bloom,
                                &mut idx.hover_tags,
                                &mut idx.hover_classes,
                                &mut idx.hover_ids,
                                &mut idx.has_unbounded_hover_rule,
                            );
                        }
                        for (_, step_comp) in &sel.steps {
                            if Self::compound_has_pseudo(step_comp, "hover") {
                                Self::index_compound_keys(
                                    step_comp,
                                    &mut idx.hover_bloom,
                                    &mut idx.hover_tags,
                                    &mut idx.hover_classes,
                                    &mut idx.hover_ids,
                                    &mut idx.has_unbounded_hover_rule,
                                );
                            }
                        }
                    }

                    let target_focus = Self::compound_has_pseudo(&sel.target, "focus")
                        || Self::compound_has_pseudo(&sel.target, "focus-within")
                        || Self::compound_has_pseudo(&sel.target, "focus-visible");
                    let step_focus = sel.steps.iter().any(|(_, c)| {
                        Self::compound_has_pseudo(c, "focus")
                            || Self::compound_has_pseudo(c, "focus-within")
                            || Self::compound_has_pseudo(c, "focus-visible")
                    });
                    if target_focus || step_focus {
                        idx.has_any_focus_rule = true;
                        if target_focus {
                            Self::index_compound_keys(
                                &sel.target,
                                &mut idx.focus_bloom,
                                &mut idx.focus_tags,
                                &mut idx.focus_classes,
                                &mut idx.focus_ids,
                                &mut idx.has_unbounded_focus_rule,
                            );
                        }
                        for (_, step_comp) in &sel.steps {
                            if Self::compound_has_pseudo(step_comp, "focus")
                                || Self::compound_has_pseudo(step_comp, "focus-within")
                                || Self::compound_has_pseudo(step_comp, "focus-visible")
                            {
                                Self::index_compound_keys(
                                    step_comp,
                                    &mut idx.focus_bloom,
                                    &mut idx.focus_tags,
                                    &mut idx.focus_classes,
                                    &mut idx.focus_ids,
                                    &mut idx.has_unbounded_focus_rule,
                                );
                            }
                        }
                    }
                }
            }
        }
        idx
    }

    #[inline]
    pub fn node_may_match_hover(&self, tag: &str, id: Option<&str>, classes: &[String]) -> bool {
        if !self.has_any_hover_rule {
            return false;
        }
        if self.has_unbounded_hover_rule {
            return true;
        }
        let tb = Self::bloom_bits(tag);
        if (self.hover_bloom & tb) == tb && self.hover_tags.contains(&tag.to_ascii_lowercase()) {
            return true;
        }
        if let Some(id_str) = id {
            let ib = Self::bloom_bits(id_str);
            if (self.hover_bloom & ib) == ib && self.hover_ids.contains(&id_str.to_ascii_lowercase())
            {
                return true;
            }
        }
        for cls in classes {
            let cb = Self::bloom_bits(cls);
            if (self.hover_bloom & cb) == cb
                && self.hover_classes.contains(&cls.to_ascii_lowercase())
            {
                return true;
            }
        }
        false
    }

    #[inline]
    #[allow(dead_code)]
    pub fn node_may_match_focus(&self, tag: &str, id: Option<&str>, classes: &[String]) -> bool {
        if !self.has_any_focus_rule {
            return false;
        }
        if self.has_unbounded_focus_rule {
            return true;
        }
        let tb = Self::bloom_bits(tag);
        if (self.focus_bloom & tb) == tb && self.focus_tags.contains(&tag.to_ascii_lowercase()) {
            return true;
        }
        if let Some(id_str) = id {
            let ib = Self::bloom_bits(id_str);
            if (self.focus_bloom & ib) == ib && self.focus_ids.contains(&id_str.to_ascii_lowercase())
            {
                return true;
            }
        }
        for cls in classes {
            let cb = Self::bloom_bits(cls);
            if (self.focus_bloom & cb) == cb
                && self.focus_classes.contains(&cls.to_ascii_lowercase())
            {
                return true;
            }
        }
        false
    }
}

/// Per-update reconciliation statistics exposed for runtime profiling and observability.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReconcileStats {
    pub reused_subtrees: u32,
    pub rebuilt_nodes: u32,
}

/// Structured attribution explaining why a frame triggered work (`Dirty`, `RepaintCanvas3D`, or `Clean`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebuildReason {
    Clean,
    InitialMount,
    SignalWrite(String),
    PlatformEvent(String),
    HoverFastPath,
    HoverFastPathFailed(String),
    LayoutReconcileBailout(String),
    RepaintCanvas3D,
    AnimationTick,
    JsTimerOrMutation,
    FrameHookDirty,
    WindowResize,
    McpCommand(String),
}

impl std::fmt::Display for RebuildReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Clean => write!(f, "Clean"),
            Self::InitialMount => write!(f, "InitialMount"),
            Self::SignalWrite(sig) => write!(f, "SignalWrite({sig})"),
            Self::PlatformEvent(ev) => write!(f, "PlatformEvent({ev})"),
            Self::HoverFastPath => write!(f, "HoverFastPath"),
            Self::HoverFastPathFailed(r) => write!(f, "HoverFastPathFailed({r})"),
            Self::LayoutReconcileBailout(r) => write!(f, "LayoutReconcileBailout({r})"),
            Self::RepaintCanvas3D => write!(f, "RepaintCanvas3D"),
            Self::AnimationTick => write!(f, "AnimationTick"),
            Self::JsTimerOrMutation => write!(f, "JsTimerOrMutation"),
            Self::FrameHookDirty => write!(f, "FrameHookDirty"),
            Self::WindowResize => write!(f, "WindowResize"),
            Self::McpCommand(cmd) => write!(f, "McpCommand({cmd})"),
        }
    }
}
