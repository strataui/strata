//! W3C-compliant synthetic event pipeline with 3-phase capturing and bubbling dispatch.

use std::cell::Cell;
use std::ops::Deref;

/// Standard synthetic event types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EventType {
    Click,
    DblClick,
    /// W3C `contextmenu`: fired for a secondary-button press on the nearest listening element.
    ContextMenu,
    AuxClick,
    MouseDown,
    MouseUp,
    MouseMove,
    MouseEnter,
    MouseLeave,
    MouseOver,
    MouseOut,
    PointerDown,
    PointerUp,
    PointerMove,
    KeyDown,
    KeyUp,
    Input,
    Change,
    Submit,
    Focus,
    Blur,
    Wheel,
    TransitionRun,
    TransitionStart,
    TransitionEnd,
    TransitionCancel,
    AnimationStart,
    AnimationIteration,
    AnimationEnd,
    AnimationCancel,
    Custom(&'static str),
}

/// The three dispatch phases of the W3C DOM Event Model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventPhase {
    None,
    Capturing,
    AtTarget,
    Bubbling,
}

/// Synthetic DOM Event passed to event handlers, fully modeled after W3C DOM Level 3 / UI Events.
#[derive(Clone, Debug)]
pub struct Event {
    pub event_type: EventType,
    pub phase: Cell<EventPhase>,
    pub target_id: Option<u32>,
    pub current_target_id: Cell<Option<u32>>,
    pub propagation_stopped: Cell<bool>,
    pub default_prevented: Cell<bool>,

    // Text & Form Value Properties
    pub value: String,
    pub data: Option<String>,
    pub input_type: String,

    // Mouse & Pointer Coordinate Properties (W3C MouseEvent / PointerEvent)
    pub client_x: f64,
    pub client_y: f64,
    pub screen_x: f64,
    pub screen_y: f64,
    pub page_x: f64,
    pub page_y: f64,
    pub offset_x: f64,
    pub offset_y: f64,
    pub movement_x: f64,
    pub movement_y: f64,
    pub button: i16,
    pub buttons: u16,
    pub detail: i32,

    // Keyboard Properties (W3C KeyboardEvent)
    pub key: String,
    pub code: String,
    pub repeat: bool,
    pub location: u32,

    // Modifier Keys
    pub ctrl_key: bool,
    pub shift_key: bool,
    pub alt_key: bool,
    pub meta_key: bool,

    // Wheel Properties (W3C WheelEvent)
    pub delta_x: f64,
    pub delta_y: f64,
    pub delta_z: f64,
    pub delta_mode: u32,

    // Event Timestamp in Milliseconds
    pub timestamp: f64,

    // Transition & Animation Event Properties (W3C CSS Transitions & Animations)
    pub property_name: String,
    pub animation_name: String,
    pub elapsed_time: f32,
}

impl Default for Event {
    fn default() -> Self {
        Self {
            event_type: EventType::Click,
            phase: Cell::new(EventPhase::None),
            target_id: None,
            current_target_id: Cell::new(None),
            propagation_stopped: Cell::new(false),
            default_prevented: Cell::new(false),
            value: String::new(),
            data: None,
            input_type: String::new(),
            client_x: 0.0,
            client_y: 0.0,
            screen_x: 0.0,
            screen_y: 0.0,
            page_x: 0.0,
            page_y: 0.0,
            offset_x: 0.0,
            offset_y: 0.0,
            movement_x: 0.0,
            movement_y: 0.0,
            button: 0,
            buttons: 0,
            detail: 0,
            key: String::new(),
            code: String::new(),
            repeat: false,
            location: 0,
            ctrl_key: false,
            shift_key: false,
            alt_key: false,
            meta_key: false,
            delta_x: 0.0,
            delta_y: 0.0,
            delta_z: 0.0,
            delta_mode: 0,
            timestamp: 0.0,
            property_name: String::new(),
            animation_name: String::new(),
            elapsed_time: 0.0,
        }
    }
}

impl Event {
    pub fn new(event_type: EventType) -> Self {
        Self {
            event_type,
            ..Default::default()
        }
    }

    pub fn with_value(event_type: EventType, val: impl Into<String>) -> Self {
        Self {
            event_type,
            value: val.into(),
            ..Default::default()
        }
    }

    pub fn transition(event_type: EventType, property: impl Into<String>, elapsed_time: f32) -> Self {
        Self {
            event_type,
            property_name: property.into(),
            elapsed_time,
            ..Default::default()
        }
    }

    pub fn animation(event_type: EventType, animation_name: impl Into<String>, elapsed_time: f32) -> Self {
        Self {
            event_type,
            animation_name: animation_name.into(),
            elapsed_time,
            ..Default::default()
        }
    }

    pub fn stop_propagation(&self) {
        self.propagation_stopped.set(true);
    }

    pub fn prevent_default(&self) {
        self.default_prevented.set(true);
    }

    pub fn is_propagation_stopped(&self) -> bool {
        self.propagation_stopped.get()
    }

    pub fn is_default_prevented(&self) -> bool {
        self.default_prevented.get()
    }

    pub fn phase(&self) -> EventPhase {
        self.phase.get()
    }

    pub fn current_target(&self) -> Option<u32> {
        self.current_target_id.get()
    }

    /// Returns true if this event was initiated by the primary (usually left) mouse button.
    pub fn is_primary(&self) -> bool {
        self.button == 0
    }

    /// Returns true if this event was initiated by the auxiliary (usually middle) mouse button.
    pub fn is_auxiliary(&self) -> bool {
        self.button == 1
    }

    /// Returns true if this event was initiated by the secondary (usually right) mouse button.
    pub fn is_secondary(&self) -> bool {
        self.button == 2
    }

    /// Returns true if any keyboard modifier key (Ctrl, Shift, Alt, or Meta) was held.
    pub fn has_modifier(&self) -> bool {
        self.ctrl_key || self.shift_key || self.alt_key || self.meta_key
    }
}

impl Deref for Event {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

/// Dispatches a synthetic event across a node tree ancestry path.
pub struct EventDispatcher;

impl EventDispatcher {
    /// Dispatches an event through Capturing, AtTarget, and Bubbling phases.
    /// `ancestry_path` must be ordered from Root (index 0) to Target element (last index).
    pub fn dispatch<F>(ancestry_path: &[u32], event: &Event, mut listener_invoker: F)
    where
        F: FnMut(u32, EventPhase, &Event),
    {
        if ancestry_path.is_empty() {
            return;
        }

        let target_node = *ancestry_path.last().unwrap();

        // 1. Capturing Phase: Root down to target's parent
        event.phase.set(EventPhase::Capturing);
        if ancestry_path.len() > 1 {
            for &node_id in &ancestry_path[..ancestry_path.len() - 1] {
                if event.is_propagation_stopped() {
                    return;
                }
                event.current_target_id.set(Some(node_id));
                listener_invoker(node_id, EventPhase::Capturing, event);
            }
        }

        // 2. At Target Phase
        if event.is_propagation_stopped() {
            return;
        }
        event.phase.set(EventPhase::AtTarget);
        event.current_target_id.set(Some(target_node));
        listener_invoker(target_node, EventPhase::AtTarget, event);

        // 3. Bubbling Phase: Target's parent up to Root
        if event.is_propagation_stopped() {
            return;
        }
        event.phase.set(EventPhase::Bubbling);
        if ancestry_path.len() > 1 {
            for &node_id in ancestry_path[..ancestry_path.len() - 1].iter().rev() {
                if event.is_propagation_stopped() {
                    return;
                }
                event.current_target_id.set(Some(node_id));
                listener_invoker(node_id, EventPhase::Bubbling, event);
            }
        }
    }
}
