//! Plugin host coordinating active runtime plugins and capability enforcement.

use std::collections::HashMap;
use crate::manifest::PluginManifest;
use crate::registry::DynamicExtensionRegistry;
use crate::security::SecuritySandbox;

/// Manages active dynamic plugins, their sandboxes, and the runtime extension registry.
pub struct PluginHost {
    plugins: HashMap<String, SecuritySandbox>,
    pub registry: DynamicExtensionRegistry,
}

impl Default for PluginHost {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginHost {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            registry: DynamicExtensionRegistry::new(),
        }
    }

    /// Loads and registers a plugin into the host environment.
    pub fn load_plugin(&mut self, manifest: PluginManifest) -> Result<(), String> {
        let id = manifest.id.clone();
        if self.plugins.contains_key(&id) {
            return Err(format!("Plugin with id '{}' is already loaded", id));
        }

        let sandbox = SecuritySandbox::new(manifest);
        self.plugins.insert(id, sandbox);
        Ok(())
    }

    pub fn unload_plugin(&mut self, id: &str) -> bool {
        self.plugins.remove(id).is_some()
    }

    pub fn get_sandbox(&self, id: &str) -> Option<&SecuritySandbox> {
        self.plugins.get(id)
    }

    pub fn loaded_plugin_count(&self) -> usize {
        self.plugins.len()
    }
}
