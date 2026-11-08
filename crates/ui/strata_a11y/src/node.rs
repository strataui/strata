//! Accessibility node and tree representation.

use std::collections::HashMap;
use crate::role::A11yRole;

/// Unique identifier for an accessibility tree node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct A11yNodeId(pub u64);

/// Rectangular spatial bounding box for accessibility hit-testing and screen-reader visual focus rings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct A11yRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl A11yRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }
}

/// Actions an assistive technology (e.g. screen reader) can request on a node.
#[derive(Debug, Clone, PartialEq)]
pub enum A11yAction {
    Click,
    Focus,
    Blur,
    SetValue(String),
    Increment,
    Decrement,
    ScrollUp,
    ScrollDown,
}

/// Politeness level of a live region for screen-reader speech interruption.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum A11yLivePoliteness {
    #[default]
    Off,
    Polite,
    Assertive,
}

impl A11yLivePoliteness {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Polite => "polite",
            Self::Assertive => "assertive",
        }
    }

    pub fn from_str_name(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "polite" => Self::Polite,
            "assertive" => Self::Assertive,
            _ => Self::Off,
        }
    }
}

/// A single node in the accessibility tree.
#[derive(Debug, Clone, PartialEq)]
pub struct A11yNode {
    pub id: A11yNodeId,
    pub role: A11yRole,
    pub bounds: Option<A11yRect>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub value: Option<String>,
    pub numeric_value: Option<f64>,
    pub min_numeric_value: Option<f64>,
    pub max_numeric_value: Option<f64>,
    pub numeric_step: Option<f64>,
    pub children: Vec<A11yNodeId>,
    pub disabled: bool,
    pub selected: bool,
    pub checked: Option<bool>,
    pub expanded: Option<bool>,
    pub focusable: bool,
    pub focused: bool,
    pub supported_actions: Vec<A11yAction>,
    pub live: Option<A11yLivePoliteness>,
    pub live_atomic: bool,
    pub live_busy: bool,
}

impl A11yNode {
    pub fn new(id: A11yNodeId, role: A11yRole) -> Self {
        let focusable = role.is_interactive();
        Self {
            id,
            role,
            bounds: None,
            name: None,
            description: None,
            value: None,
            numeric_value: None,
            min_numeric_value: None,
            max_numeric_value: None,
            numeric_step: None,
            children: Vec::new(),
            disabled: false,
            selected: false,
            checked: None,
            expanded: None,
            focusable,
            focused: false,
            supported_actions: Vec::new(),
            live: None,
            live_atomic: false,
            live_busy: false,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn with_bounds(mut self, bounds: A11yRect) -> Self {
        self.bounds = Some(bounds);
        self
    }

    pub fn with_value(mut self, val: impl Into<String>) -> Self {
        self.value = Some(val.into());
        self
    }

    pub fn with_numeric_range(mut self, val: f64, min: f64, max: f64, step: f64) -> Self {
        self.numeric_value = Some(val);
        self.min_numeric_value = Some(min);
        self.max_numeric_value = Some(max);
        self.numeric_step = Some(step);
        self
    }

    pub fn with_live(mut self, politeness: A11yLivePoliteness, atomic: bool) -> Self {
        self.live = Some(politeness);
        self.live_atomic = atomic;
        self
    }

    pub fn with_busy(mut self, busy: bool) -> Self {
        self.live_busy = busy;
        self
    }

    pub fn with_actions(mut self, actions: Vec<A11yAction>) -> Self {
        self.supported_actions = actions;
        self
    }

    pub fn with_disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn with_focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn with_selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn with_checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    pub fn with_expanded(mut self, expanded: bool) -> Self {
        self.expanded = Some(expanded);
        self
    }

    pub fn add_child(&mut self, child_id: A11yNodeId) {
        self.children.push(child_id);
    }
}

/// An accessible tree representing the current UI hierarchy for screen readers.
#[derive(Debug, Clone, PartialEq)]
pub struct A11yTree {
    pub root_id: A11yNodeId,
    pub nodes: HashMap<A11yNodeId, A11yNode>,
}

impl A11yTree {
    pub fn new(root_id: A11yNodeId, root_node: A11yNode) -> Self {
        let mut nodes = HashMap::new();
        nodes.insert(root_id, root_node);
        Self { root_id, nodes }
    }

    pub fn insert_node(&mut self, node: A11yNode) {
        self.nodes.insert(node.id, node);
    }

    pub fn get_node(&self, id: A11yNodeId) -> Option<&A11yNode> {
        self.nodes.get(&id)
    }

    pub fn get_node_mut(&mut self, id: A11yNodeId) -> Option<&mut A11yNode> {
        self.nodes.get_mut(&id)
    }

    /// Recursively counts all nodes in the tree.
    pub fn total_nodes(&self) -> usize {
        self.nodes.len()
    }
}
