# Architecture Overview

Strata is architected as an interconnected collection of modular, single-responsibility Rust crates. Every stage of the frontend execution lifecycle—from macro expansion to GPU pixel presentation—is implemented in safe, idiomatic Rust without external browser runtimes.

---

## The Execution Pipeline

```text
┌─────────────────────────────────────────────────────────────┐
│                    Strata Application                       │
│             (Components, RSX, Tailwind Classes)             │
└──────────────────────────────┬──────────────────────────────┘
                               │
                ┌──────────────┴──────────────┐
                ▼                             ▼
   ┌─────────────────────────┐   ┌─────────────────────────┐
   │       strata_dom        │   │        strata_js        │
   │  (Reactive Signals DAG, │   │    (Dynamic Types,      │
   │    64-Byte UiIrNode)    │   │   Promises, Web APIs)   │
   └────────────┬────────────┘   └────────────┬────────────┘
                │                             │
                └──────────────┬──────────────┘
                               ▼
   ┌─────────────────────────────────────────────────────────┐
   │                      strata_style                       │
   │         (Tailwind JIT, 256-Bit SIMD Style Mask)         │
   └───────────────────────────┬─────────────────────────────┘
                               ▼
   ┌─────────────────────────────────────────────────────────┐
   │             strata_layout / strata_parallel             │
   │     (Chromium Parity Flexbox & Grid, Rayon Solver)      │
   └───────────────────────────┬─────────────────────────────┘
                               ▼
   ┌─────────────────────────────────────────────────────────┐
   │                      strata_render                      │
   │       (Display List, Stacking Context, Slug SDF)        │
   └───────────────────────────┬─────────────────────────────┘
                               │
    ┌──────────────┬───────────┼───────────┬──────────────┐
    ▼              ▼           ▼           ▼              ▼
┌────────┐    ┌────────┐  ┌────────┐  ┌────────┐     ┌────────┐
│ Desktop│    │ Mobile │  │  Web   │  │  Web   │     │Terminal│
│ macOS  │    │  iOS   │  │  WASM  │  │ Pure JS│     │  TUI   │
│ Win/Lin│    │ Android│  │ Canvas │  │  DOM   │     │  ANSI  │
└────────┘    └────────┘  └────────┘  └────────┘     └────────┘
```

---

## Crates Ecosystem

The workspace contains 25 specialized crates organized into cohesive layers:

### Core Framework & Declarative Syntax
- **`strata`**: Public umbrella crate providing prelude, runtime builder, and re-exports.
- **`strata_macro`**: Procedural macros (`rsx!`, `#[component]`, and compile-time utility validation).
- **`strata_dom`**: Reactive signals DAG, intermediate representation, and 64-byte `UiIrNode`.
- **`strata_style`**: CSS parser, cascade resolution, Tailwind JIT, and 256-bit SIMD style masks.
- **`strata_primitives`**: Mathematical primitives (vectors, matrices, rects, color spaces).

### Layout & Rendering Engine
- **`strata_layout`**: Spec-compliant Box Model, Flexbox (Level 1), and Grid (Level 2) solvers.
- **`strata_parallel`**: Rayon work-stealing parallel subtree solver based on containment axioms.
- **`strata_render`**: Hardware display lists, layer tree, Slug-tier GPU font rendering, and 3D `<Canvas3D>`.
- **`strata_gpu_path`**: Analytical GPU path engine for subpixel cubic Bezier curves without CPU tessellation.

### Platform Services & Runtime Abstraction
- **`strata_platform`**: Native windowing, frameless client-side decorations, Mica/Acrylic vibrancy, and display servers.
- **`strata_vfs`**: Universal virtual asynchronous filesystem (OPFS, XDG, app sandboxes).
- **`strata_storage`**: Pure-Rust Bitcask append-only write-ahead log (`StrataKV`) and biometric secure keystore.
- **`strata_device`**: Battery, thermals, safe area insets, memory pressure, and 120Hz ProMotion telemetry.
- **`strata_web_api`**: W3C Web Platform APIs (`fetch`, WebSockets, WebRTC Data Channels, WebAudio 3D).
- **`strata_audio`**: Low-latency 3D spatial audio graph and synthesis engine.

### Cross-Target Compilation & Tooling
- **`strata_tui`**: Terminal compilation target with double-buffered delta ANSI and Sixel/Kitty graphics.
- **`strata_codegen_js`**: Source-to-source transpiler emitting zero-WASM pure JavaScript ESM.
- **`strata_cli`**: CLI toolchain, project wizard, dev server, and packaging engine.
- **`strata_devtools`**: In-process HUD, profiler, and Chrome DevTools Protocol (CDP) bridge.
- **`strata_lsp`**: Full Language Server Protocol 3.17 server for editor integration.
- **`strata_sim`**: Dogfooded native mobile device simulator and virtual phone OS.
- **`strata_mobile_test`**: Headless automated mobile testing orchestrator.

### Advanced Innovations & Extensibility
- **`strata_a11y`**: AccessKit accessibility tree bridge for native screen-readers.
- **`strata_crdt`**: Conflict-free Replicated Data Type engine for real-time multiplayer signals.
- **`strata_agent_ui`**: Streaming generative UI protocol and agentic execution bridge.
- **`strata_build`**: Dynamic build-time plugin architecture.
- **`strata_plugin_runtime`**: Sandboxed WASM dynamic plugin host.
- **`strata_ffi`**: Zero-cost C ABI bindings for embedding Strata inside native C/C++/Swift applications.
- **`strata_benchmark`**: Standardized benchmarking suite for latency and memory audits.

---

## Memory Footprint & Cache Locality

Strata uses cache-aligned contiguous memory buffers rather than scattered heap allocations. All nodes in a component tree reside in cache-friendly arenas, ensuring CPU L1/L2 cache lines are saturated during layout passes. This architecture is the primary driver of Strata's sub-3MB resident memory footprint.
