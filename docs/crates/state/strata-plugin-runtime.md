# strata-plugin-runtime

Sandboxed dynamic plugin loader, WebAssembly capability security sandbox, and runtime extension host for Project Strata applications.

---

## Architecture: Sandboxed Extensibility

The `strata-plugin-runtime` crate provides a secure, sandboxed runtime environment for extending Strata applications with dynamic third-party extensions. In enterprise cloud telemetry, monitoring dashboards, and developer toolchains, extensions allow operators to inject proprietary visualizers, synthetic metrics calculators, custom Tailwind utility classes, and telemetry interceptors without recompiling the host application.

```
       +-------------------------------------------------------+
       |             Strata Host Application                   |
       +---------------------------+---------------------------+
                                   |
                                   v
       +-------------------------------------------------------+
       |                      PluginHost                       |
       |  - plugins: HashMap<PluginId, SecuritySandbox>        |
       |  - registry: DynamicExtensionRegistry                 |
       +---------------------------+---------------------------+
                                   |
                +------------------+------------------+
                |                                     |
                v                                     v
   +------------------------+             +------------------------+
   |    SecuritySandbox     |             | DynamicExtensionReg    |
   | - Network Egress Guard |             | - Custom RSX Tags      |
   | - Storage R/W Guard    |             | - Custom Utilities     |
   | - 3D Render / Audio    |             | - Event Interceptors   |
   +------------------------+             +------------------------+
                |                                     |
                +------------------+------------------+
                                   |
                                   v
       +-------------------------------------------------------+
       |         Dynamic Plugin Binary Bundle (.stpl)          |
       |     [PluginBinaryHeader: Magic 'STPL' + ABI v1]       |
       |     [Manifest Section: Permissions & Capabilities]    |
       |     [WASM Bytecode: Wasmer / Wasmtime Sandboxed Exec] |
       +-------------------------------------------------------+
```

### Security Posture & Sandbox Guarantees

1. **Principle of Least Privilege**: Plugins start with zero permissions. Network, storage, rendering, and synthetic DOM event access must be declared explicitly in the `PluginManifest` and approved by the host.
2. **Binary Header Verification**: Every compiled dynamic plugin bundle is prefixed with the standard `STPL` 16-byte binary header, verifying ABI version compatibility before invocation.
3. **Memory Isolation**: WASM execution runs in isolated linear memory pools, preventing arbitrary memory reads or corruptions of the host process.
4. **Non-Blocking Synthetic Extension**: Dynamic components and Tailwind utility classes resolve in sub-microsecond time with fallback defaults.

---

## Dynamic Plugin Application Binary Interface (ABI)

The binary layout is standardized in `abi.rs` with a 16-byte fixed-size header.

### ABI Constants

```rust
pub const STRATA_PLUGIN_ABI_VERSION: u32 = 1;
pub const STRATA_PLUGIN_MAGIC: [u8; 4] = [b'S', b'T', b'P', b'L'];
```

### Binary Header Structure

```rust
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginBinaryHeader {
    pub magic: [u8; 4],             // Must match [b'S', b'T', b'P', b'L']
    pub abi_version: u32,          // ABI revision (currently 1)
    pub manifest_offset: u32,      // Byte offset to JSON/BSON manifest in bundle
    pub manifest_length: u32,      // Length of manifest payload in bytes
}
```

The `is_valid()` method validates magic bytes and ABI version integrity:

```rust
use strata_plugin_runtime::{PluginBinaryHeader, STRATA_PLUGIN_MAGIC, STRATA_PLUGIN_ABI_VERSION};

let header = PluginBinaryHeader::new(64, 1024);
assert!(header.is_valid());
assert_eq!(header.magic, STRATA_PLUGIN_MAGIC);
assert_eq!(header.abi_version, STRATA_PLUGIN_ABI_VERSION);
```

---

## Capability Security Model: `PluginCapability` & `SecuritySandbox`

Permissions in `strata-plugin-runtime` are expressed through the `PluginCapability` enum:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PluginCapability {
    Network(Vec<String>),  // Authorized egress hostnames or wildcard "*"
    StorageRead,           // Read permission to sandboxed persistent storage
    StorageWrite,          // Write permission to sandboxed persistent storage
    Render3D,              // Access to offscreen GPU 3D rendering pipeline
    AudioSynthesis,        // Access to audio hardware output
    CustomComponents,      // Injects dynamic RSX component tags
}
```

### Declaring a Plugin Manifest

```rust
use strata_plugin_runtime::{PluginCapability, PluginManifest};

let manifest = PluginManifest::new(
    "com.strata.prometheus.exporter",
    "Prometheus Telemetry Exporter",
    "2.4.0",
)
.with_author("Infrastructure Reliability Engineering")
.with_capability(PluginCapability::Network(vec![
    "prometheus-us-east.internal.net".to_string(),
    "alertmanager.internal.net".to_string(),
]))
.with_capability(PluginCapability::StorageRead)
.with_capability(PluginCapability::CustomComponents);
```

### Runtime Enforcement: `SecuritySandbox`

The `SecuritySandbox` enforces capabilities before any privileged host subsystem is accessed:

```rust
use strata_plugin_runtime::SecuritySandbox;

let sandbox = SecuritySandbox::new(manifest);

// Network egress verification
assert!(sandbox.check_network_access("prometheus-us-east.internal.net").is_ok());
assert!(sandbox.check_network_access("unauthorized-egress.external.org").is_err());

// Storage read vs write verification
assert!(sandbox.check_storage_read().is_ok());
assert!(sandbox.check_storage_write().is_err()); // Denied: StorageWrite not granted

// Render 3D verification
assert!(sandbox.check_render_3d().is_err());     // Denied: Render3D not granted
```

---

## Dynamic Extension Registry: `DynamicExtensionRegistry`

The `DynamicExtensionRegistry` provides three extension vectors:

1. **Custom RSX Component Tags**: Dynamic rendering closures injected by plugins.
2. **Tailwind CSS Utility Classes**: Custom JIT utility class declarations.
3. **Synthetic Event Interceptors**: Global event filters capable of consuming or logging UI events.

### 1. Dynamic Component Registration

Plugins can register custom component renderers identified by case-insensitive tag names:

```rust
use std::collections::HashMap;
use strata_plugin_runtime::DynamicExtensionRegistry;

let mut registry = DynamicExtensionRegistry::new();

registry.register_component("PrometheusGauge", |props, children| {
    let metric = props.get("metric").map(|s| s.as_str()).unwrap_or("unknown");
    let val = children.unwrap_or("0.0");
    format!(
        "<div class=\"telemetry-gauge\"><label>{}</label><span>{}</span></div>",
        metric, val
    )
});

assert!(registry.has_component("PrometheusGauge"));
assert!(registry.has_component("prometheusgauge")); // Case-insensitive lookup

let mut props = HashMap::new();
props.insert("metric".to_string(), "cpu_system_seconds_total".to_string());
let rendered = registry.render_component("PrometheusGauge", &props, Some("4821.5")).unwrap();
assert!(rendered.contains("cpu_system_seconds_total"));
```

### 2. Custom Tailwind / CSS Utility Classes

Plugins can extend the styling engine with custom utility classes:

```rust
// Register glowing status indicator utility classes
registry.register_utility("glow-healthy", "box-shadow: 0 0 12px #10b981; border-color: #10b981;");
registry.register_utility("glow-critical", "box-shadow: 0 0 16px #ef4444; border-color: #ef4444;");

assert_eq!(
    registry.resolve_utility("glow-healthy"),
    Some("box-shadow: 0 0 12px #10b981; border-color: #10b981;")
);
```

### 3. Synthetic Event Interception

Plugins can install event interceptors to audit actions or enforce operational safety guards:

```rust
// Intercept right-clicks or critical actions on production deployment buttons
registry.register_event_interceptor(|event_type, target_id| {
    if event_type == "click" && target_id == "btn-destroy-cluster" {
        eprintln!("AUDIT ALERT: Cluster destruction event intercepted by policy guard.");
        true // Returning true cancels / consumes the event
    } else {
        false // Allow normal event dispatch
    }
});

// Event is blocked:
assert!(registry.intercept_event("click", "btn-destroy-cluster"));

// Normal event proceeds:
assert!(!registry.intercept_event("click", "btn-refresh-telemetry"));
```

---

## Plugin Host Lifecycle: `PluginHost`

The `PluginHost` acts as the central coordinator, holding active sandboxes and managing plugin lifecycles:

```rust
use strata_plugin_runtime::{PluginHost, PluginManifest, PluginCapability};

let mut host = PluginHost::new();
assert_eq!(host.loaded_plugin_count(), 0);

let manifest = PluginManifest::new(
    "com.strata.network.tracer",
    "Distributed Network Tracer",
    "1.1.0",
)
.with_capability(PluginCapability::Network(vec!["*".to_string()]));

// Load and register plugin
assert!(host.load_plugin(manifest.clone()).is_ok());
assert_eq!(host.loaded_plugin_count(), 1);

// Prevent duplicate plugin registration
assert!(host.load_plugin(manifest).is_err());

// Inspect active sandbox
let sandbox = host.get_sandbox("com.strata.network.tracer").unwrap();
assert_eq!(sandbox.manifest().name, "Distributed Network Tracer");

// Unload plugin cleanly
assert!(host.unload_plugin("com.strata.network.tracer"));
assert_eq!(host.loaded_plugin_count(), 0);
```

---

## Enterprise Telemetry Plugin Implementation

Below is a complete implementation of a third-party observability extension that registers custom visualizers and intercepts unauthorized outbound traffic:

```rust
use std::collections::HashMap;
use strata_plugin_runtime::{
    DynamicExtensionRegistry, PluginCapability, PluginHost, PluginManifest,
};

pub struct CloudObservabilityExtension;

impl CloudObservabilityExtension {
    pub fn install(host: &mut PluginHost) -> Result<(), String> {
        let manifest = PluginManifest::new(
            "io.strata.observability.latency",
            "Latency Distribution Visualizer",
            "1.0.0",
        )
        .with_author("Telemetry Platform Team")
        .with_capability(PluginCapability::Network(vec![
            "telemetry.mesh.internal".to_string(),
        ]))
        .with_capability(PluginCapability::CustomComponents);

        host.load_plugin(manifest)?;

        // Register custom RSX LatencyP99 card
        host.registry.register_component("LatencyP99Card", |props, text| {
            let service = props.get("service").map(|s| s.as_str()).unwrap_or("default");
            let p99_ms = text.unwrap_or("0.0");
            format!(
                "<div class=\"p-4 rounded-xl bg-neutral-900 border border-neutral-800\">\
                    <span class=\"text-xs text-neutral-500 font-mono\">{}</span>\
                    <div class=\"text-xl font-bold font-mono text-cyan-400 mt-1\">{} ms</div>\
                </div>",
                service, p99_ms
            )
        });

        // Register Tailwind metric utility class
        host.registry.register_utility(
            "bg-telemetry-surface",
            "background-color: #0c0d0e; border: 1px solid #1f2428;",
        );

        // Register audit interceptor
        host.registry.register_event_interceptor(|event_type, target_id| {
            if event_type == "mutation" && target_id.starts_with("telemetry-config-") {
                println!("Audit: telemetry configuration modified on {}", target_id);
            }
            false
        });

        Ok(())
    }
}
```

---

## Security Threat Mitigation Matrix

| Threat Vector | Mitigation Strategy in `strata-plugin-runtime` |
| :--- | :--- |
| **Data Exfiltration** | Domain-whitelisted egress filtering via `check_network_access()`. Unauthorized network operations are blocked. |
| **Filesystem Tampering** | Storage operations segregated into `StorageRead` and `StorageWrite`. Local filesystem paths are sandboxed. |
| **GPU / Resource Exhaustion** | 3D rendering pipeline calls gated by `Render3D` capability checks. |
| **Event Hijacking** | Event interceptors execute synchronously within bounded closures; cannot synthesize arbitrary OS-level inputs. |
| **ABI Version Drift** | `PluginBinaryHeader` rejects dynamic bundles compiled against incompatible ABI versions during initialization. |
