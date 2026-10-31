//! Action registry for environment-aware custom build tasks and lifecycle actions.

use std::collections::HashMap;
use std::sync::Arc;
use crate::config::ProjectConfig;

/// Context provided to a dispatched action.
#[derive(Debug, Clone)]
pub struct ActionContext {
    pub name: String,
    pub environment: String,
    pub arguments: Vec<String>,
    pub config: ProjectConfig,
}

pub type ActionHandler = Arc<dyn Fn(&ActionContext) -> Result<String, String> + Send + Sync>;

/// Registry managing extensible lifecycle and custom developer actions.
#[derive(Default, Clone)]
pub struct ActionRegistry {
    actions: HashMap<String, ActionHandler>,
}

impl ActionRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            actions: HashMap::new(),
        };
        registry.register_defaults();
        registry
    }

    /// Registers a new named action.
    pub fn register<F>(&mut self, name: &str, handler: F)
    where
        F: Fn(&ActionContext) -> Result<String, String> + Send + Sync + 'static,
    {
        self.actions.insert(name.to_string(), Arc::new(handler));
    }

    /// Dispatches an action by name with the given context.
    pub fn dispatch(&self, ctx: &ActionContext) -> Result<String, String> {
        if let Some(handler) = self.actions.get(&ctx.name) {
            handler(ctx)
        } else {
            Err(format!("Action '{}' is not registered in ActionRegistry", ctx.name))
        }
    }

    /// Returns whether an action with the given name exists.
    pub fn has_action(&self, name: &str) -> bool {
        self.actions.contains_key(name)
    }

    /// Registers standard built-in actions.
    fn register_defaults(&mut self) {
        // Built-in info action
        self.register("info", |ctx| {
            Ok(format!(
                "Strata App '{}' v{} [Environment: {}]",
                ctx.config.app.name, ctx.config.app.version, ctx.environment
            ))
        });

        // Built-in validate action
        self.register("validate", |ctx| {
            if ctx.config.app.name.is_empty() {
                return Err("Validation failure: app.name cannot be empty".to_string());
            }
            if ctx.config.window.width == 0 || ctx.config.window.height == 0 {
                return Err("Validation failure: window dimensions must be greater than zero".to_string());
            }
            Ok("Project configuration is valid".to_string())
        });
    }
}
