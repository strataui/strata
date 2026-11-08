//! Action router and bridge connecting OS screen-reader events with Strata reactive DOM.

use crate::node::{A11yAction, A11yNodeId, A11yTree};
use crate::update::A11yTreeUpdate;

pub type ActionCallback = Box<dyn Fn(A11yNodeId, A11yAction) + Send + Sync>;

/// Bridges Strata's accessibility tree with OS assistive technologies.
pub struct A11yBridge {
    current_tree: Option<A11yTree>,
    action_handler: Option<ActionCallback>,
}

impl std::fmt::Debug for A11yBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("A11yBridge")
            .field("current_tree", &self.current_tree)
            .field("has_action_handler", &self.action_handler.is_some())
            .finish()
    }
}

impl Default for A11yBridge {

    fn default() -> Self {
        Self::new()
    }
}

impl A11yBridge {
    pub fn new() -> Self {
        Self {
            current_tree: None,
            action_handler: None,
        }
    }

    /// Registers a callback to receive action dispatches from the OS screen reader.
    pub fn set_action_handler<F>(&mut self, handler: F)
    where
        F: Fn(A11yNodeId, A11yAction) + Send + Sync + 'static,
    {
        self.action_handler = Some(Box::new(handler));
    }

    /// Processes a new accessibility tree, returning the incremental update to transmit to the OS.
    pub fn update(&mut self, new_tree: A11yTree) -> A11yTreeUpdate {
        let diff = A11yTreeUpdate::diff(self.current_tree.as_ref(), &new_tree);
        self.current_tree = Some(new_tree);
        diff
    }

    /// Dispatches an action from an assistive device (e.g. user pressed Space on a screen-reader focused button).
    pub fn dispatch_action(&self, target_node: A11yNodeId, action: A11yAction) {
        if let Some(handler) = &self.action_handler {
            handler(target_node, action);
        }
    }

    pub fn current_tree(&self) -> Option<&A11yTree> {
        self.current_tree.as_ref()
    }
}
