---
layout: home

hero:
  name: "Strata"
  text: "Zero Chromium. Pure Rust. Native 120 FPS."
  tagline: "The high-performance universal frontend engine and UI framework. Compiles declarative RSX to native Desktop, Mobile, Web, and Terminal with sub-3MB memory footprint."
  image:
    src: /logo.svg
    alt: Strata Engine
  actions:
    - theme: brand
      text: Get Started
      link: /guide/quick-start
    - theme: alt
      text: Architecture
      link: /architecture/overview
    - theme: alt
      text: GitHub
      link: https://github.com/larvance/strata

features:
  - title: Zero Chromium Architecture
    details: Eliminates heavy WebViews, CEF, and V8 runtimes. Starts up in under 8ms with an idle memory footprint below 3MB.
    icon:
      src: /logo.svg
  - title: Reactive Signals DAG
    details: Fine-grained reactivity via use_signal and use_effect. Direct node mutation without virtual DOM dirty reconciliation overhead.
    icon:
      src: /logo.svg
  - title: Pixel-Perfect Layout Parity
    details: Spec-compliant CSS Box Model, Flexbox Level 1, and Grid Level 2 solvers with subpixel text shaping and HarfBuzz fonts.
    icon:
      src: /logo.svg
  - title: Multi-Threaded Subtrees
    details: Parallel subtree reconciliation across Rayon work-stealing threads based on the 5 Axioms of Layout Containment.
    icon:
      src: /logo.svg
  - title: Analytical GPU & SDF Pipeline
    details: Direct Vulkan, Metal, Direct3D 12, and WebGPU backends with Slug-tier cubic Bezier font and curve rasterization.
    icon:
      src: /logo.svg
  - title: Universal Multi-Target Engine
    details: Write once in RSX and compile to native Desktop (macOS/Win/Linux), Mobile (iOS/Android), Web (WASM/JS), and Terminal (TUI).
    icon:
      src: /logo.svg
---

<div class="strata-scorecard">
  <div class="strata-card">
    <div class="card-label">Cold Start Latency</div>
    <div class="card-value">&lt; 8 ms</div>
    <div class="card-desc">Instantaneous native binary launch with pre-compiled layout state.</div>
  </div>
  <div class="strata-card">
    <div class="card-label">Idle Memory Footprint</div>
    <div class="card-value">2.8 MB</div>
    <div class="card-desc">Zero embedded browser or runtime VM overhead.</div>
  </div>
  <div class="strata-card">
    <div class="card-label">Layout Throughput</div>
    <div class="card-value">100k nodes / 2.95ms</div>
    <div class="card-desc">Rayon multi-threaded work-stealing layout engine.</div>
  </div>
  <div class="strata-card">
    <div class="card-label">Rendering Target</div>
    <div class="card-value">120 FPS</div>
    <div class="card-desc">Direct GPU presentation pipeline with zero frame tearing.</div>
  </div>
</div>

## Declarative RSX Meets Pure Rust Performance

Strata combines the developer velocity of modern JSX/TSX with the safety and memory efficiency of idiomatic Rust. Component trees are verified at compile time, eliminating runtime type mismatches and missing CSS utility classes.

```rust
use strata::prelude::*;

#[component]
pub fn TelemetryDashboard() -> View {
    let (active_tab, set_active_tab) = use_state(|| "overview");
    let (metrics, set_metrics) = use_signal(|| SystemMetrics::default());

    use_effect(move || {
        let interval = set_interval(move || {
            set_metrics.update(|m| m.refresh());
        }, 100);
        move || clear_interval(interval)
    });

    rsx! {
        <div class="flex h-screen w-screen bg-neutral-950 text-neutral-100 font-sans">
            <!-- Sidebar Navigation -->
            <aside class="w-64 border-r border-neutral-800 p-6 flex flex-col justify-between">
                <div class="space-y-6">
                    <div class="flex items-center gap-3">
                        <div class="w-3 h-3 rounded-full bg-indigo-500 animate-pulse" />
                        <span class="font-mono text-sm font-semibold tracking-wider text-neutral-200">
                            STRATA // NODE-01
                        </span>
                    </div>

                    <nav class="space-y-1">
                        <TabButton label="Overview" active={active_tab() == "overview"} on_click={move |_| set_active_tab.set("overview")} />
                        <TabButton label="Workloads" active={active_tab() == "workloads"} on_click={move |_| set_active_tab.set("workloads")} />
                        <TabButton label="Storage" active={active_tab() == "storage"} on_click={move |_| set_active_tab.set("storage")} />
                    </nav>
                </div>

                <div class="text-xs text-neutral-500 font-mono">
                    UPTIME: 99.998% // 120 FPS
                </div>
            </aside>

            <!-- Main Workstation Viewport -->
            <main class="flex-1 p-8 overflow-y-auto">
                <header class="flex items-center justify-between pb-6 border-b border-neutral-800">
                    <div>
                        <h1 class="text-2xl font-semibold tracking-tight text-white">Cluster Telemetry</h1>
                        <p class="text-sm text-neutral-400 mt-1">Real-time distributed execution nodes</p>
                    </div>
                    <Badge variant="emerald">ALL SYSTEMS OPERATIONAL</Badge>
                </header>

                <div class="grid grid-cols-3 gap-6 mt-6">
                    <MetricCard title="Frame Budget" value={format!("{:.2} ms", metrics().frame_time)} status="nominal" />
                    <MetricCard title="Resident Memory" value={format!("{:.1} MB", metrics().resident_mem)} status="nominal" />
                    <MetricCard title="Compute Nodes" value={format!("{}", metrics().node_count)} status="nominal" />
                </div>
            </main>
        </div>
    }
}
```

---

## Architectural Comparison

| Dimension | Strata | Electron | Tauri | Flutter |
| :--- | :--- | :--- | :--- | :--- |
| **Runtime Engine** | Pure Rust Native Kernel | Chromium + Node.js | System WebView (WebKit/WebView2) | Dart VM + Skia/Impeller |
| **Cold Start Latency** | **&lt; 8 ms** | 450 - 1,200 ms | 150 - 400 ms | 80 - 250 ms |
| **Idle Memory Footprint** | **2.8 MB** | 120 - 350 MB | 45 - 90 MB | 35 - 75 MB |
| **Binary Size (Stripped)** | **4.8 MB** | 85 - 160 MB | 12 - 25 MB | 20 - 45 MB |
| **Layout Solver** | Multi-threaded Rayon Flex/Grid | Chromium Blink (Single Thread) | Blink / WebKit | Single-threaded Dart RenderObject |
| **Tailwind CSS** | Built-in JIT SIMD Engine | PostCSS / Webpack | PostCSS / Vite | Unsupported (Custom Widgets) |
| **Terminal (TUI) Target** | **Native Delta ANSI Engine** | Unsupported | Unsupported | Unsupported |
| **Dual Web Compilation** | **WASM + Pure JS ESM** | Web Only (Via DOM) | Web Only (Via DOM) | WASM CanvasKit Only |

---

## Quick Installation

Deploy the Strata toolchain in seconds using the standalone bootstrap installer:

```bash
curl -fsSL https://strata.larvance.com/install.sh | sh
```

Or install via Cargo from source:

```bash
cargo install --path crates/strata_cli --locked
```

Initialize your first production application:

```bash
# Initialize a new high-performance project
strata new my-app

# Navigate to project directory
cd my-app

# Launch instantaneous HMR dev environment
strata dev
```
