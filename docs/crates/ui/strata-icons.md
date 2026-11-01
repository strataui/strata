# strata_icons: High-Performance GPU Vector Icon Suite

The `strata_icons` crate delivers an integrated, zero-runtime-overhead vector icon library engineered specifically for the Strata engine. Providing feature and visual parity with modern industry icon sets such as Lucide and Heroicons, `strata_icons` features over 1,000 vector primitives optimized for direct GPU vector rasterization, subpixel anti-aliasing, and custom stroke formatting.

---

## Architecture and Vector Pipeline

Unlike traditional web applications that embed heavyweight SVGs into DOM trees or render font glyphs with irregular hinting, Strata processes vector icons through an analytical GPU path engine:

```text
┌─────────────────────────┐
│       IconName          │ (Compile-time enum discriminant)
└────────────┬────────────┘
             │
             ▼
┌─────────────────────────┐
│     Registry Lookup     │ (O(1) static array indexing, zero allocations)
└────────────┬────────────┘
             │
             ▼
┌─────────────────────────┐
│  VectorPath Data (d)    │ (Parsed path commands: M, L, Q, C, Z)
└────────────┬────────────┘
             │
             ▼
┌─────────────────────────┐
│ Strata GPU Path Engine  │ (Hardware Slug/SDF shader curve rasterization)
└─────────────────────────┘
```

1. **Zero Runtime Deserialization**: All vector geometries are embedded as static binary path coordinates within the binary, eliminating XML/SVG parsing overhead.
2. **Infinite Scaling & Crisp Edges**: Vectors render at any resolution—from 12px micro-indicators in dense cluster matrices to high-DPI 4K displays—without rasterization artifacts or blur.
3. **Compile-Time Typings**: Missing or mistyped icon names are rejected at compile time via the strongly typed `IconName` enum.

---

## Core Types and Data Models

### IconName Enumeration

Icons are categorized across distinct functional domains:

```rust
use strata_icons::prelude::IconName;

// Infrastructure & Cluster Monitoring
let server = IconName::Server;
let cpu = IconName::Cpu;
let db = IconName::Database;
let net = IconName::Network;
let trace = IconName::Activity;
let shell = IconName::Terminal;

// Security & Compliance
let firewall = IconName::Shield;
let verified = IconName::ShieldCheck;
let secure_key = IconName::Key;
let lock = IconName::Lock;

// Developer Lifecycle & Git Ops
let git_branch = IconName::GitBranch;
let git_commit = IconName::GitCommit;
let git_pr = IconName::GitPullRequest;
let code = IconName::Code;

// Telemetry Status & Indicators
let warn = IconName::AlertTriangle;
let err = IconName::AlertCircle;
let ok = IconName::CheckCircle;
let pulse = IconName::Zap;
```

---

### VectorPath and PathCommand

For custom geometry synthesis or runtime path inspection, `VectorPath` models vector segments:

```rust
use strata_icons::prelude::{PathCommand, VectorPath};

let mut path = VectorPath::new();
path.move_to(0.0, 0.0)
    .line_to(24.0, 0.0)
    .line_to(24.0, 24.0)
    .line_to(0.0, 24.0)
    .close();

let bounds = path.compute_bounds();
assert_eq!(bounds.size.width, 24.0);
assert_eq!(bounds.size.height, 24.0);

let svg_d = path.to_svg_d();
assert!(svg_d.contains("M 0 0"));
```

#### Supported Path Commands

| Command | Description | Mathematical Expression |
| :--- | :--- | :--- |
| `PathCommand::MoveTo(x, y)` | Lifts the pen and relocates to coordinate `(x, y)`. | $P_0 = (x, y)$ |
| `PathCommand::LineTo(x, y)` | Draws a straight line to `(x, y)`. | $P(t) = (1-t)P_0 + tP_1$ |
| `PathCommand::QuadTo(cx, cy, x, y)` | Draws a quadratic Bezier curve with control point `(cx, cy)`. | $B(t) = (1-t)^2 P_0 + 2(1-t)t P_c + t^2 P_1$ |
| `PathCommand::CubicTo(c1x, c1y, c2x, c2y, x, y)` | Draws a cubic Bezier curve with dual control points. | $B(t) = (1-t)^3 P_0 + 3(1-t)^2 t P_{c1} + 3(1-t)t^2 P_{c2} + t^3 P_1$ |
| `PathCommand::Close` | Closes current subpath back to origin. | Subpath terminator |

---

## Component Interfaces and Usage

### The `Icon` Component

The primary declarative component accepts an `IconName` and optional Tailwind utility classes for color, size, and animation:

```rust
use strata_dom::prelude::*;
use strata_icons::prelude::*;

#[component]
pub fn ClusterNodeStatus(is_healthy: bool) -> View {
    rsx! {
        <div class="flex items-center gap-2 p-2 rounded bg-neutral-900 border border-neutral-800">
            {if is_healthy {
                Icon(IconName::CheckCircle, Some("w-4 h-4 text-emerald-400"))
            } else {
                Icon(IconName::AlertTriangle, Some("w-4 h-4 text-rose-500 animate-bounce"))
            }}
            <span class="text-xs font-mono text-neutral-300">
                {if is_healthy { "us-east-cluster: Nominal" } else { "us-east-cluster: Degraded" }}
            </span>
        </div>
    }
}
```

---

### The `IconConfig` Component

When precise numerical dimensions or variable stroke weights are required, `IconConfig` allows explicit parameterization:

```rust
use strata_icons::prelude::{IconConfig, IconName};

// Renders a high-prominence 32px server node icon with custom 1.5px stroke weight
let server_icon = IconConfig(
    IconName::Server,
    Some(32.0),                // size_px (sets width and height)
    Some(1.5),                 // stroke_width
    Some("text-sky-400 stroke-current"), // Tailwind color token
);
```

#### Properties Specification

| Parameter | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `name` | `IconName` | Required | Typed icon identifier. |
| `size_px` | `Option<f32>` | `24.0` | Exact viewport dimensions in logical pixels. |
| `stroke_width` | `Option<f32>` | `2.0` | SVG stroke width thickness in pixels. |
| `class` | `Option<&'static str>` | `None` | CSS and Tailwind style classes (e.g. `text-indigo-400`, `rotate-90`). |

---

### Typed Static Constructors: `Icons::*`

To eliminate string and option verbosity in ergonomic RSX trees, `Icons` provides static constructors for every standard icon:

```rust
use strata_dom::prelude::*;
use strata_icons::prelude::Icons;

#[component]
pub fn TelemetryQuickActions() -> View {
    rsx! {
        <div class="flex gap-1 bg-neutral-950 p-1 border border-neutral-800 rounded-lg">
            <button class="p-1.5 hover:bg-neutral-800 rounded transition-colors text-neutral-400 hover:text-white" title="Refresh">
                {Icons::RefreshCw(Some("w-4 h-4"))}
            </button>
            <button class="p-1.5 hover:bg-neutral-800 rounded transition-colors text-neutral-400 hover:text-white" title="Filter Routes">
                {Icons::Filter(Some("w-4 h-4"))}
            </button>
            <button class="p-1.5 hover:bg-neutral-800 rounded transition-colors text-neutral-400 hover:text-white" title="Cluster Settings">
                {Icons::Settings(Some("w-4 h-4"))}
            </button>
        </div>
    }
}
```

---

## Icon Domain Catalog

The library includes over 90 core production icons organized into logical sets:

### 1. Cloud Infrastructure & DevOps
- `Server`: Rack server chassis.
- `Cpu`: Central processor package with external pins.
- `HardDrive`: Storage volume disk.
- `Database`: Multi-platter database cylinder.
- `Network`: Distributed mesh topology nodes.
- `Activity`: Real-time oscillograph / heartbeat pulse.
- `Terminal`: Command-line shell prompt (`>_`).
- `Code`: Open/close angle brackets (`</>`).
- `GitBranch`: Forked commit lineage.
- `GitCommit`: Single lineage node.
- `GitPullRequest`: Branch merge target.
- `Shield`: Security perimeter crest.
- `ShieldCheck`: Validated cryptographic certificate / compliance badge.
- `Zap`: Electrical lightning bolt for fast-path routing.
- `Cloud`: Remote cloud provider infrastructure.
- `Layers`: Multi-tier software architecture stack.
- `Box`: Containerized deployment unit.

### 2. Actions & Navigation
- `Search`: Optical magnifying glass.
- `RefreshCw`: Clockwise looping circular update arrows.
- `Sliders`: Equalizer adjustment sliders for config parameters.
- `Filter`: Funnel data query selector.
- `Download` / `Upload`: Network payload transfer arrows.
- `Maximize` / `Minimize`: Fullscreen view controllers.
- `ArrowLeft`, `ArrowRight`, `ArrowUp`, `ArrowDown`: Directional indicators.
- `ChevronLeft`, `ChevronRight`, `ChevronUp`, `ChevronDown`: Hierarchy fold indicators.

### 3. Monitoring & Status
- `AlertTriangle`: Warning condition indicator (ambient amber).
- `AlertCircle`: Critical error indicator (ambient rose).
- `CheckCircle`: Verification success badge (ambient emerald).
- `Info`: Information banner anchor.
- `HelpCircle`: Interactive documentation tooltip trigger.
- `Sparkles`: Generative AI and automated analysis trigger.

---

## Performance Characteristics

| Metric | Strata Icons | Standard React SVG Icon (e.g. lucide-react) |
| :--- | :--- | :--- |
| **Parsing Cost** | **0.00 μs** (Static Rust slices) | 0.8–2.4 ms per DOM parse |
| **GPU Texture Memory** | **0 KB** (Direct SDF curve shader) | Varies with bitmap cache |
| **Layout Pass Impact** | **O(1) cached rect** | Triggers browser DOM reflow |
| **Binary Overhead** | **~200 bytes per icon** | 1.5–4.0 KB per component JS bundle |
