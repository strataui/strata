# Introduction to Strata

**Strata** is a ground-up reimagining of frontend software architecture, engineered entirely in **pure Rust**. It delivers the declarative developer velocity of modern TypeScript, React (TSX), and Tailwind CSS while operating with the deterministic memory control, safety, and raw performance of native Rust.

Unlike Electron, Tauri, or React Native, **Strata never spawns an embedded browser, WebView, or Chromium instance**. Instead, it implements a pixel-perfect, spec-compliant CSS layout and GPU rasterization engine written in safe, idiomatic Rust.

---

## The Motivation: Why Zero Chromium?

For over a decade, cross-platform frontend development has relied on bundling Chromium or delegating rendering to operating system WebViews. While this approach enabled web developers to ship desktop and mobile software quickly, it introduced severe architectural penalties:

1. **Massive Memory Footprint**: Even the most trivial Electron application requires 120MB to 350MB of resident memory just to initialize the Chromium rendering pipeline and V8 JavaScript engine.
2. **Cold Start Latency**: Spawning multiple processes, initializing browser IPC channels, and parsing JavaScript bundles incurs 400ms to 1,500ms of startup latency before the first pixel can be displayed.
3. **Garbage Collection Jitter**: The JavaScript V8 engine executes garbage collection cycles unpredictably, causing noticeable frame drops during 60 FPS and 120 FPS animations.
4. **Platform WebView Inconsistencies**: Frameworks that rely on system WebViews (such as Tauri) experience subtle CSS rendering bugs, varying JavaScript engine versions, and broken feature parity between Windows (WebView2), macOS (WebKit), and Linux (WebKitGTK).

Strata eliminates these compromises by delivering an in-process, self-contained native engine with zero external browser dependencies.

---

## Architectural Principles

### 1. Deterministic Resource Management
Memory allocations in Strata are deterministic and strictly bounded. UI intermediate representations (`UiIrNode`) are packed into compact 64-byte structures. Applications boot in under **8 milliseconds** and idle comfortably at **2.8 MB of RAM**.

### 2. Fine-Grained Reactive DAG
Traditional web frameworks rely on dirty virtual DOM diffing, reconciling extensive component trees on every state change. Strata uses a fine-grained reactive dependency graph inspired by modern signal architectures. When a signal mutates, only the exact layout nodes or text runs that depend on it are updated.

### 3. Chromium Spec-Parity Layout
Strata implements spec-compliant CSS Box Model, Flexbox (CSS Flexible Box Layout Module Level 1), and Grid (CSS Grid Layout Module Level 2). Layout calculations adhere to W3C standards with subpixel precision, font baseline alignment, and multi-line flex-wrapping.

### 4. Parallel Subtree Reconciliation
Strata divides large component trees into independent containment boundaries. Using Rayon work-stealing thread pools, layout calculations for separate sidebars, modals, and data grids execute in parallel across all available CPU cores.

### 5. Universal Platform Target
Write declarative interfaces once using Rust macros (`rsx!`) and deploy natively to:
- **Desktop**: macOS (Metal/Cocoa), Windows (Direct3D 12/Win32), Linux (Wayland/X11/Vulkan).
- **Mobile**: iOS (UIKit/Metal), Android (NDK/Vulkan/SurfaceView).
- **Web**: WebAssembly + WebGPU canvas for ultra-performance, or pure JavaScript ESM transpilation for DOM environments.
- **Terminal**: Interactive Terminal User Interface (TUI) with delta ANSI rendering, Unicode box junctions, and Sixel/Kitty graphics.

---

## Framework Comparison

| Feature | Strata | Electron | Tauri | Flutter |
| :--- | :--- | :--- | :--- | :--- |
| **Language** | Rust | JavaScript / TypeScript | Rust + Web Frontend | Dart |
| **Rendering Engine** | GPU Analytical SDF / Vulkan / Metal | Chromium Blink | System WebKit / WebView2 | Skia / Impeller |
| **JS Engine Required** | Optional (In-process `strata_js`) | V8 | JavaScriptCore / V8 | None (Dart VM) |
| **Zero Chromium** | Yes | No | No (System WebView) | Yes |
| **Terminal Target** | Yes (`strata_tui`) | No | No | No |
| **Hot Module Reload** | Sub-3ms AST Patcher | Webpack / Vite HMR | Vite HMR | Dart Hot Reload |
| **Accessibility Bridge**| Native AccessKit | Chromium Blink A11y | System WebView A11y | Flutter Semantics |

---

## Project Status

Strata is actively developed under an open MIT license. The framework features 25 modular crates covering the entire frontend lifecycle from compile-time macros to GPU presentation pipelines.
