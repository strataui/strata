# Dual Web Compilation Strategy

Strata provides two distinct compilation pathways for the web, allowing developers to balance raw compute performance against bundle size and startup latency:

1. **High-Performance WebAssembly (WASM) Target**
2. **Zero-WASM Pure JavaScript ESM Transpilation Target**

---

## Strategy 1: High-Performance WebAssembly (`--target wasm`)

In WebAssembly mode, the complete Strata native engine—including the analytical SDF rasterizer, layout solver, and reactive signals DAG—is compiled directly to a compact `.wasm` binary targeting `wasm32-unknown-unknown`.

### Architectural Characteristics
- **Rendering Surface**: High-performance `<canvas>` element rendered via **WebGPU** (with automatic WebGL2 fallback).
- **Execution Speed**: Near-native computation throughput for complex layouts, math-heavy simulations, and 120 FPS animations.
- **Unified Engine**: Pixel-for-pixel visual parity with native desktop and mobile binaries.
- **Use Cases**: Creative studios, CAD modeling, telemetry dashboards, and high-frequency data grids.

```bash
strata build --target wasm --release
```

Generated Output:
```text
dist/wasm/
├── index.html          # Lightweight HTML container
├── strata_app.js       # WASM glue & WebGL/WebGPU surface initialization
└── strata_app_bg.wasm  # Stripped, LTO-optimized WebAssembly binary (1.8 MB)
```

---

## Strategy 2: Zero-WASM JavaScript Transpiler (`--target js`)

Not all web environments can afford the download or initialization cost of a WebAssembly module. For content-focused web portals, marketing pages, and strict zero-WASM corporate environments, Strata includes **`strata_codegen_js`**.

`strata_codegen_js` is an in-process source-to-source compiler written in Rust that transforms Strata RSX components, signals, and Tailwind styles into pure modern JavaScript (ESM) and native browser DOM calls:

### Architectural Characteristics
- **Zero WASM Overhead**: Emits lightweight, human-readable JavaScript ESM modules.
- **Sub-50ms First Contentful Paint (FCP)**: The browser parses and renders HTML DOM directly without waiting for WebAssembly instantiation.
- **DOM Accessibility**: Elements are standard HTML5 elements (`<div>`, `<button>`, `<input>`), seamlessly integrated with browser extensions and assistive technology.
- **CSS Extraction**: Tailwind classes are compiled into a standalone, minified `.css` file.

```bash
strata build --target js --release
```

Generated Output:
```text
dist/js/
├── index.html          # Pre-rendered static HTML shell
├── app.esm.js          # Zero-dependency ESM bundle (14 KB gzipped)
└── styles.css          # Pruned atomic CSS stylesheet (6 KB gzipped)
```

---

## Technical Comparison Matrix

| Metric | WebAssembly Mode (`--target wasm`) | JavaScript Transpilation (`--target js`) |
| :--- | :--- | :--- |
| **Output Format** | `.wasm` + WebGL/WebGPU Canvas | `.js` ESM + Native HTML5 DOM |
| **Bundle Size** | 1.8 MB (uncompressed WASM) | **14 KB** (minified ESM bundle) |
| **First Contentful Paint (FCP)** | 120 ms - 250 ms | **&lt; 50 ms** |
| **Frame Budget Stability** | Deterministic 60-120 FPS | Dependent on browser DOM reflows |
| **Canvas Integration** | Direct GPU swapchain | `<canvas>` container elements |
| **Search Engine Optimization** | Client-side Canvas | Fully indexable semantic HTML DOM |
| **Memory Footprint** | 8 MB - 16 MB in browser VM | 3 MB - 6 MB |
