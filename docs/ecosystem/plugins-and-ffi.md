# Plugins & C-FFI Embedding

Strata provides an extensible architecture through dynamic build plugins (`strata_build`), sandboxed WASM plugin runtimes (`strata_plugin_runtime`), and zero-cost C ABI bindings (`strata_ffi`).

---

## Dynamic Build Plugins (`strata_build`)

Build plugins allow developers to hook into the compilation and asset packaging pipeline:
- Asset optimization (SVG path minification, texture compression).
- Code generation (GraphQL / Protobuf to RSX types).
- Custom shader compilation (GLSL/HLSL to SPIR-V).

### Defining a Plugin

```rust
use strata_build::{Plugin, BuildContext, Result};

pub struct AssetOptimizationPlugin;

impl Plugin for AssetOptimizationPlugin {
    fn name(&self) -> &'static str {
        "asset-optimizer"
    }

    fn on_pre_build(&self, ctx: &mut BuildContext) -> Result<()> {
        println!("Optimizing vector icons and textures...");
        // Custom asset pipeline execution
        Ok(())
    }
}
```

Configuring plugins in `strata.toml`:

```toml
[build.plugins]
enabled = ["asset-optimizer"]
```

---

## Sandboxed WASM Plugin Runtime (`strata_plugin_runtime`)

For applications that need user-authored or third-party extensions (like Figma, VS Code, or Obsidian plugins), Strata provides a sandboxed WebAssembly host:

- **Isolated Execution**: Third-party plugins execute in memory-isolated WASM instances.
- **Controlled Capabilities**: Host applications declare fine-grained permissions (file access, network sockets, UI injection).
- **Near-Native Speed**: JIT-compiled through pure-Rust WASM runtimes.

```rust
use strata_plugin_runtime::{PluginHost, Capability};

fn load_extension() -> Result<(), PluginError> {
    let mut host = PluginHost::new();
    host.grant_capability(Capability::NetworkFetch("https://api.example.com/*"));

    let extension = host.load_wasm("plugins/chart_exporter.wasm")?;
    extension.invoke_hook("on_render", &render_context)?;

    Ok(())
}
```

---

## C-FFI Foreign Function Interface (`strata_ffi`)

`strata_ffi` exposes stable C-compatible ABI symbols, allowing you to embed Strata UI views inside existing native desktop applications written in C++, Swift (macOS/iOS), Objective-C, or C# (.NET):

### Exported C Headers (`strata.h`)

```c
#ifndef STRATA_FFI_H
#define STRATA_FFI_H

#include <stdint.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct StrataAppContext StrataAppContext;

// Create and initialize a Strata rendering instance
StrataAppContext* strata_app_create(uint32_t width, uint32_t height);

// Resize the viewport
void strata_app_resize(StrataAppContext* ctx, uint32_t width, uint32_t height, float dpi_scale);

// Dispatch input events
void strata_app_dispatch_mouse_move(StrataAppContext* ctx, float x, float y);
void strata_app_dispatch_mouse_button(StrataAppContext* ctx, uint32_t button, bool pressed);

// Render directly into an existing native window handle or GPU surface
void strata_app_render_frame(StrataAppContext* ctx, void* native_render_target);

// Destroy the context and release resources
void strata_app_destroy(StrataAppContext* ctx);

#ifdef __cplusplus
}
#endif

#endif // STRATA_FFI_H
```

### Swift Integration Example (macOS / iOS)

```swift
import Cocoa
import StrataEngine

class StrataMetalView: NSView {
    private var strataContext: OpaquePointer?

    override init(frame: NSRect) {
        super.init(frame: frame)
        strataContext = strata_app_create(UInt32(frame.width), UInt32(frame.height))
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let ctx = strataContext else { return }
        strata_app_render_frame(ctx, self.layer)
    }

    deinit {
        if let ctx = strataContext {
            strata_app_destroy(ctx)
        }
    }
}
```
