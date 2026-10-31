//! # Strata Build Engine
//!
//! Multi-format configuration loading (`strata.toml`, `strata.yaml`),
//! environment-dependent cascading overrides, dynamic build plugin lifecycle hooks,
//! and extensible action execution for Strata applications.

pub mod action;
pub mod config;
pub mod plugin;
pub mod yaml_parser;

pub use action::{ActionContext, ActionHandler, ActionRegistry};
pub use config::{AppConfig, BuildConfig, ProjectConfig, WebConfig, WindowConfig};
pub use plugin::{
    BuildArtifacts, BuildContext, BuildPlugin, DynamicBrandingPlugin, PackageManifest,
    PluginRegistry,
};
pub use yaml_parser::{parse_yaml, YamlValue};
