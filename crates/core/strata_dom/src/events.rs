//! W3C-compliant synthetic event pipeline with 3-phase capturing and bubbling dispatch.

use std::cell::Cell;
use std::ops::Deref;

/// Standard synthetic event types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EventType {
    Click,
    Input,
    Change,
    Submit,
    KeyDown,
    KeyUp,
    PointerDown,
    PointerUp,
    PointerMove,
    Focus,
    Blur,
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

/// Synthetic DOM Event passed to event handlers.
#[derive(Clone, Debug)]
pub struct Event {
    pub event_type: EventType,
    pub phase: Cell<EventPhase>,
    pub target_id: Option<u32>,
    pub current_target_id: Cell<Option<u32>>,
    pub propagation_stopped: Cell<bool>,
    pub default_prevented: Cell<bool>,
    pub value: String,
    pub client_x: f64,
    pub client_y: f64,
    pub button: u8,
    pub key: String,
    pub code: String,
    pub ctrl_key: bool,
    pub shift_key: bool,
    pub alt_key: bool,
    pub meta_key: bool,
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
            client_x: 0.0,
            client_y: 0.0,
            button: 0,
            key: String::new(),
            code: String::new(),
            ctrl_key: false,
            shift_key: false,
            alt_key: false,
            meta_key: false,
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
