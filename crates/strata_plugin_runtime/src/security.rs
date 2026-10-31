//! Capability sandbox validator enforcing runtime security boundaries.

use crate::manifest::{PluginCapability, PluginManifest};

/// Enforces sandbox execution permissions for an active plugin instance.
pub struct SecuritySandbox {
    manifest: PluginManifest,
}

impl SecuritySandbox {
    pub fn new(manifest: PluginManifest) -> Self {
        Self { manifest }
    }

    pub fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    /// Verifies if network egress to `domain` is authorized.
    pub fn check_network_access(&self, domain: &str) -> Result<(), String> {
        for cap in &self.manifest.capabilities {
            if let PluginCapability::Network(allowed) = cap {
                if allowed.iter().any(|d| d == "*" || d == domain) {
                    return Ok(());
                }
            }
        }
        Err(format!(
            "Plugin '{}' denied unauthorized network egress to domain '{}'",
            self.manifest.id, domain
        ))
    }

    /// Verifies if storage read access is authorized.
    pub fn check_storage_read(&self) -> Result<(), String> {
        if self.manifest.has_capability(&PluginCapability::StorageRead) {
            Ok(())
        } else {
            Err(format!("Plugin '{}' denied storage read access", self.manifest.id))
        }
    }

    /// Verifies if storage write access is authorized.
    pub fn check_storage_write(&self) -> Result<(), String> {
        if self.manifest.has_capability(&PluginCapability::StorageWrite) {
            Ok(())
        } else {
            Err(format!("Plugin '{}' denied storage write access", self.manifest.id))
        }
    }

    /// Verifies if 3D hardware GPU rendering is authorized.
    pub fn check_render_3d(&self) -> Result<(), String> {
        if self.manifest.has_capability(&PluginCapability::Render3D) {
            Ok(())
        } else {
            Err(format!("Plugin '{}' denied 3D rendering access", self.manifest.id))
        }
    }
}
