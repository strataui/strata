//! Dynamic build plugin pipeline allowing custom Rust plugins to intercept and modify configuration and packaging.

use std::path::PathBuf;
use crate::config::ProjectConfig;

/// Build context passed to plugins before execution.
#[derive(Debug, Clone)]
pub struct BuildContext {
    pub target: String,
    pub environment: String,
    pub out_dir: PathBuf,
    pub release: bool,
}

/// Artifacts produced by the build pipeline.
#[derive(Debug, Clone, Default)]
pub struct BuildArtifacts {
    pub binary_path: Option<PathBuf>,
    pub output_files: Vec<PathBuf>,
    pub total_size_bytes: u64,
}

/// Package manifest passed to packaging hooks.
#[derive(Debug, Clone)]
pub struct PackageManifest {
    pub app_name: String,
    pub version: String,
    pub identifier: String,
    pub executable_name: String,
}

/// Core trait for build-time Rust plugins.
pub trait BuildPlugin: Send + Sync {
    /// Unique identifier for the plugin.
    fn name(&self) -> &str;

    /// Invoked immediately after configuration is loaded and before compilation begins.
    /// Plugins can mutate `config` dynamically (e.g., changing `app.name` or version).
    fn on_load_config(&self, _config: &mut ProjectConfig, _env: &str) -> Result<(), String> {
        Ok(())
    }

    /// Invoked right before the compiler/bundler starts.
    fn before_build(&self, _context: &BuildContext) -> Result<(), String> {
        Ok(())
    }

    /// Invoked immediately after compilation finishes.
    fn after_build(&self, _artifacts: &BuildArtifacts) -> Result<(), String> {
        Ok(())
    }

    /// Invoked before native packaging to adjust installer/distro metadata.
    fn modify_manifest(&self, _manifest: &mut PackageManifest) -> Result<(), String> {
        Ok(())
    }
}

/// Registry that orchestrates build plugins.
#[derive(Default)]
pub struct PluginRegistry {
    plugins: Vec<Box<dyn BuildPlugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self { plugins: Vec::new() }
    }

    pub fn register(&mut self, plugin: Box<dyn BuildPlugin>) {
        self.plugins.push(plugin);
    }

    pub fn execute_on_load_config(&self, config: &mut ProjectConfig, env: &str) -> Result<(), String> {
        for p in &self.plugins {
            p.on_load_config(config, env)?;
        }
        Ok(())
    }

    pub fn execute_before_build(&self, context: &BuildContext) -> Result<(), String> {
        for p in &self.plugins {
            p.before_build(context)?;
        }
        Ok(())
    }

    pub fn execute_after_build(&self, artifacts: &BuildArtifacts) -> Result<(), String> {
        for p in &self.plugins {
            p.after_build(artifacts)?;
        }
        Ok(())
    }

    pub fn execute_modify_manifest(&self, manifest: &mut PackageManifest) -> Result<(), String> {
        for p in &self.plugins {
            p.modify_manifest(manifest)?;
        }
        Ok(())
    }

    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }
}

/// Example built-in dynamic branding plugin.
/// Demonstrates changing the app name dynamically with a Rust plugin based on environment.
pub struct DynamicBrandingPlugin {
    pub prefix: String,
    pub suffix: String,
}

impl DynamicBrandingPlugin {
    pub fn new(prefix: &str, suffix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
            suffix: suffix.to_string(),
        }
    }
}

impl BuildPlugin for DynamicBrandingPlugin {
    fn name(&self) -> &str {
        "dynamic-branding"
    }

    fn on_load_config(&self, config: &mut ProjectConfig, env: &str) -> Result<(), String> {
        config.app.name = format!("{}{}{}", self.prefix, config.app.name, self.suffix);
        if env == "production" {
            config.window.title = format!("{} (PROD)", config.app.name);
        } else {
            config.window.title = format!("{} [{}]", config.app.name, env);
        }
        Ok(())
    }

    fn modify_manifest(&self, manifest: &mut PackageManifest) -> Result<(), String> {
        manifest.app_name = format!("{}{}{}", self.prefix, manifest.app_name, self.suffix);
        Ok(())
    }
}
