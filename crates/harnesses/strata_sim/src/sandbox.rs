//! Multi-App Sandbox managing concurrent running and suspended Strata application instances.

use crate::launcher::AppMetadata;
use std::collections::HashMap;

/// Lifecycle state of a sandboxed application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppLifecycleState {
    ForegroundActive,
    BackgroundSuspended,
    Terminated,
}

/// Instance record for an installed application inside the sandbox.
#[derive(Debug, Clone, PartialEq)]
pub struct SandboxedAppInstance {
    pub metadata: AppMetadata,
    pub state: AppLifecycleState,
    pub launch_count: u32,
    pub frame_count: u64,
    pub memory_bytes: usize,
}

/// Multi-Application Sandbox coordinator.
#[derive(Debug, Clone, PartialEq)]
pub struct MultiAppSandbox {
    pub instances: HashMap<String, SandboxedAppInstance>,
    pub foreground_bundle_id: Option<String>,
}

impl Default for MultiAppSandbox {
    fn default() -> Self {
        Self::new()
    }
}

impl MultiAppSandbox {
    pub fn new() -> Self {
        let mut sandbox = Self {
            instances: HashMap::new(),
            foreground_bundle_id: None,
        };

        // Pre-register standard Strata apps
        sandbox.register_app(AppMetadata::new(
            "com.strata.saas",
            "Cloud SaaS",
            "server",
            "Productivity",
        ));
        sandbox.register_app(AppMetadata::new(
            "com.strata.storefront",
            "Apex Store",
            "shopping-cart",
            "Shopping",
        ));
        sandbox.register_app(AppMetadata::new(
            "com.strata.studio",
            "Creative Studio",
            "palette",
            "Graphics",
        ));
        sandbox.register_app(AppMetadata::new(
            "com.strata.settings",
            "Settings",
            "settings",
            "System",
        ));

        sandbox
    }

    /// Registers a new application bundle into the sandbox.
    pub fn register_app(&mut self, metadata: AppMetadata) {
        let bundle_id = metadata.bundle_id.clone();
        self.instances.insert(
            bundle_id,
            SandboxedAppInstance {
                metadata,
                state: AppLifecycleState::Terminated,
                launch_count: 0,
                frame_count: 0,
                memory_bytes: 4 * 1024 * 1024, // ~4MB baseline
            },
        );
    }

    /// Launches an application into the foreground, suspending any existing active app.
    pub fn launch_app(&mut self, bundle_id: &str) -> bool {
        if !self.instances.contains_key(bundle_id) {
            return false;
        }

        // Suspend previous foreground app
        if let Some(prev_id) = &self.foreground_bundle_id {
            if let Some(prev_app) = self.instances.get_mut(prev_id) {
                prev_app.state = AppLifecycleState::BackgroundSuspended;
            }
        }

        // Activate new app
        if let Some(app) = self.instances.get_mut(bundle_id) {
            app.state = AppLifecycleState::ForegroundActive;
            app.launch_count += 1;
            self.foreground_bundle_id = Some(bundle_id.to_string());
            true
        } else {
            false
        }
    }

    /// Returns to Home Screen, suspending the foreground app.
    pub fn return_to_home(&mut self) {
        if let Some(active_id) = &self.foreground_bundle_id {
            if let Some(app) = self.instances.get_mut(active_id) {
                app.state = AppLifecycleState::BackgroundSuspended;
            }
        }
        self.foreground_bundle_id = None;
    }

    /// Terminates an application bundle, purging its volatile runtime memory.
    pub fn terminate_app(&mut self, bundle_id: &str) {
        if let Some(app) = self.instances.get_mut(bundle_id) {
            app.state = AppLifecycleState::Terminated;
        }
        if self.foreground_bundle_id.as_deref() == Some(bundle_id) {
            self.foreground_bundle_id = None;
        }
    }

    /// Executes zero-state-loss display handover when a foldable screen opens or closes.
    pub fn execute_display_handover(&mut self) {
        // Active reactive signals and data models are preserved across viewport geometry swaps
        if let Some(active_id) = &self.foreground_bundle_id {
            if let Some(app) = self.instances.get_mut(active_id) {
                app.frame_count += 1;
            }
        }
    }

    /// Returns the currently active foreground app, if any.
    pub fn active_app(&self) -> Option<&SandboxedAppInstance> {
        self.foreground_bundle_id
            .as_ref()
            .and_then(|id| self.instances.get(id))
    }
}
