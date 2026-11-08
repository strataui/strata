//! Runtime data structures for active focus, scrollbars, hit targets, and editing.

/// Hit-test target result representing a clickable text input element in the blueprint tree.
#[derive(Clone)]
pub struct InputHitTarget {
    pub input_index: usize,
    pub input_id: Option<String>,
    pub current_value: String,
    pub font_size: f32,
    pub line_height: f32,
    pub is_mono: bool,
    pub font_family: strata_primitives::AtomId,
    pub content_box_x: f32,
    pub content_box_y: f32,
    pub content_box_width: f32,
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
        matches!(self, PointerDownAction::FocusChanged | PointerDownAction::DomListenerInvoked)
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


