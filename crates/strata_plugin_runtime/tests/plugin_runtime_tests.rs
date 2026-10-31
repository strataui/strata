use std::collections::HashMap;
use strata_plugin_runtime::{
    DynamicExtensionRegistry, PluginBinaryHeader, PluginCapability, PluginHost, PluginManifest,
    SecuritySandbox, STRATA_PLUGIN_ABI_VERSION, STRATA_PLUGIN_MAGIC,
};

#[test]
fn test_plugin_manifest_and_security_sandbox() {
    let manifest = PluginManifest::new("com.strata.analytics", "Analytics Engine", "1.0.0")
        .with_capability(PluginCapability::Network(vec!["api.strata.dev".to_string()]))
        .with_capability(PluginCapability::StorageRead);

    let sandbox = SecuritySandbox::new(manifest);

    // Network check
    assert!(sandbox.check_network_access("api.strata.dev").is_ok());
    assert!(sandbox.check_network_access("evil.com").is_err());

    // Storage permissions
    assert!(sandbox.check_storage_read().is_ok());
    assert!(sandbox.check_storage_write().is_err());

    // Render 3D denied
    assert!(sandbox.check_render_3d().is_err());
}

#[test]
fn test_dynamic_component_registration() {
    let mut registry = DynamicExtensionRegistry::new();

    registry.register_component("CustomMetricCard", |props, text| {
        let title = props.get("title").map(|s| s.as_str()).unwrap_or("Metric");
        let val = text.unwrap_or("0");
        format!("<div class=\"metric-card\"><h3>{}</h3><span>{}</span></div>", title, val)
    });

    assert!(registry.has_component("CustomMetricCard"));
    assert!(registry.has_component("custommetriccard")); // Case-insensitive

    let mut props = HashMap::new();
    props.insert("title".to_string(), "Active Telemetry".to_string());
    let rendered = registry.render_component("CustomMetricCard", &props, Some("4200 RPM")).unwrap();

    assert!(rendered.contains("<h3>Active Telemetry</h3>"));
    assert!(rendered.contains("<span>4200 RPM</span>"));
}

#[test]
fn test_tailwind_utility_extension_and_event_interceptor() {
    let mut registry = DynamicExtensionRegistry::new();

    // Register custom utility class
    registry.register_utility("glow-cyan", "box-shadow: 0 0 15px #06b6d4;");
    assert_eq!(
        registry.resolve_utility("glow-cyan"),
        Some("box-shadow: 0 0 15px #06b6d4;")
    );
    assert_eq!(registry.resolve_utility("unknown-class"), None);

    // Register synthetic event interceptor
    registry.register_event_interceptor(|event_type, target_id| {
        // Intercept right-clicks on protected canvas
        event_type == "contextmenu" && target_id == "telemetry-canvas"
    });

    assert!(registry.intercept_event("contextmenu", "telemetry-canvas"));
    assert!(!registry.intercept_event("click", "telemetry-canvas"));
    assert!(!registry.intercept_event("contextmenu", "other-element"));
}

#[test]
fn test_plugin_binary_header_validation() {
    let header = PluginBinaryHeader::new(64, 1024);
    assert!(header.is_valid());
    assert_eq!(header.magic, STRATA_PLUGIN_MAGIC);
    assert_eq!(header.abi_version, STRATA_PLUGIN_ABI_VERSION);

    // Corrupted magic
    let bad_header = PluginBinaryHeader {
        magic: [0, 0, 0, 0],
        abi_version: STRATA_PLUGIN_ABI_VERSION,
        manifest_offset: 64,
        manifest_length: 1024,
    };
    assert!(!bad_header.is_valid());
}

#[test]
fn test_plugin_host_lifecycle() {
    let mut host = PluginHost::new();
    let manifest = PluginManifest::new("ext.vector.tools", "Vector Tools", "0.2.0");

    assert_eq!(host.loaded_plugin_count(), 0);
    assert!(host.load_plugin(manifest.clone()).is_ok());
    assert_eq!(host.loaded_plugin_count(), 1);

    // Duplicate load fails
    assert!(host.load_plugin(manifest).is_err());

    // Unload
    assert!(host.unload_plugin("ext.vector.tools"));
    assert_eq!(host.loaded_plugin_count(), 0);
}
