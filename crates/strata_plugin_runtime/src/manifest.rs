//! Plugin metadata and capability permission manifests.

use std::collections::HashSet;

/// Specific hardware or subsystem capability requested by a plugin.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PluginCapability {
    Network(Vec<String>),
    StorageRead,
    StorageWrite,
    Render3D,
    AudioSynthesis,
    CustomComponents,
}

/// Metadata and permissions manifest for a Strata dynamic runtime plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub capabilities: HashSet<PluginCapability>,
    pub entry_point: String,
}

impl PluginManifest {
    pub fn new(id: impl Into<String>, name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            version: version.into(),
            author: String::new(),
            capabilities: HashSet::new(),
            entry_point: "main".to_string(),
        }
    }

    pub fn with_capability(mut self, cap: PluginCapability) -> Self {
        self.capabilities.insert(cap);
        self
    }

    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = author.into();
        self
    }

    pub fn has_capability(&self, cap: &PluginCapability) -> bool {
        self.capabilities.contains(cap)
    }
}
