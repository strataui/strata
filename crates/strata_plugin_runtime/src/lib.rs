//! Sandboxed dynamic plugin loader and runtime extension host for Project Strata.

pub mod abi;
pub mod host;
pub mod manifest;
pub mod registry;
pub mod security;

pub use abi::{PluginBinaryHeader, STRATA_PLUGIN_ABI_VERSION, STRATA_PLUGIN_MAGIC};
pub use host::PluginHost;
pub use manifest::{PluginCapability, PluginManifest};
pub use registry::{ComponentRenderer, DynamicExtensionRegistry, EventInterceptor};
pub use security::SecuritySandbox;
