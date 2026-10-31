//! Secure action callback bus routing user interactions from agent UI back to host tools.

use std::collections::HashMap;

pub type ToolActionCallback = Box<dyn Fn(&str, &HashMap<String, String>) -> Result<String, String> + Send + Sync>;

/// Manages and routes action dispatches triggered by user gestures in agent-generated UIs.
#[derive(Default)]
pub struct AgentActionBus {
    handlers: HashMap<String, ToolActionCallback>,
}

impl AgentActionBus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a tool action handler (e.g. "deploy_swarm", "tune_pid").
    pub fn register<F>(&mut self, action_name: impl Into<String>, handler: F)
    where
        F: Fn(&str, &HashMap<String, String>) -> Result<String, String> + Send + Sync + 'static,
    {
        self.handlers.insert(action_name.into(), Box::new(handler));
    }

    /// Dispatches an action with parameters.
    pub fn dispatch(&self, action_name: &str, params: &HashMap<String, String>) -> Result<String, String> {
        if let Some(handler) = self.handlers.get(action_name) {
            handler(action_name, params)
        } else {
            Err(format!("No agent tool handler registered for action '{}'", action_name))
        }
    }

    pub fn has_action(&self, action_name: &str) -> bool {
        self.handlers.contains_key(action_name)
    }
}
