# Flagship Examples & Showcase

The Strata repository includes production-grade reference applications in the `examples/` directory, illustrating real-world engineering patterns across enterprise dashboards, CAD tools, and vector design studios.

---

## 1. SaaS Telemetry Dashboard (`examples/saas_dashboard`)

A complete enterprise cloud infrastructure monitoring portal with real-time reactive telemetry metrics, dynamic data grids, modal dialogues, and Tailwind styling.

```bash
cargo run --release --example saas_dashboard
```

### Architectural Features
- **Real-Time Reactive Metrics**: Simulates 100 Hz server cluster telemetry updating through fine-grained signals.
- **Dynamic Data Grids**: Virtualized table displaying active microservices with sortable columns and filtering.
- **Modal Dialogues & Drawers**: Focus-trapped accessible dialogs with backdrop blur transitions.
- **Responsive Layout**: Collapsible sidebar with desktop and compact tablet breakpoints.

---

## 2. Creative Studio (`examples/creative_studio`)

A professional vector design and node composition studio inspired by modern desktop CAD and design tools.

```bash
cargo run --release --example creative_studio
```

### Architectural Features
- **Infinite Vector Canvas**: Subpixel pan and zoom canvas evaluating analytical Bezier curves at 120 FPS.
- **Unified 3D PBR Viewport**: Embedded `<Canvas3D>` rendering lighting, materials, and 3D mesh nodes.
- **Dockable Property Inspectors**: Dynamic attribute controls (transforms, fill, stroke, blend modes, shadows).
- **History & Undo/Redo**: Fast immutable state snapshots powered by generational node arenas.

---

## 3. Electronic Design Automation (EDA) Studio (`examples/eda_studio`)

A high-performance EDA platform featuring digital logic waveform analysis, schematic drafting, and SPICE circuit simulation.

```bash
cargo run --release --example eda_studio
```

### Architectural Features
- **Nanosecond Time Ruler**: Zoomable digital logic analyzer rendering transitions with zero visual aliasing.
- **Hexagonal Bus Envelopes**: Multi-bit digital bus visualization (`8'hFF`, `16'hA5A5`).
- **Live Signal Inspection**: Interactive cursors displaying instantaneous circuit voltages and delta timing measurements ($\Delta t$).
- **Schematic Canvas**: Pixel-precise grid snapping for electrical components (resistors, capacitors, transistors, logic gates).

---

## 4. E-Commerce Storefront (`examples/ecommerce_storefront`)

A consumer-facing retail marketplace demonstrating client-side reactive filtering, image caching, and shopping cart drawer states.

```bash
cargo run --release --example ecommerce_storefront
```

### Architectural Features
- **Instantaneous Multi-Facet Filtering**: Filters thousands of products by price, category, and availability without reflow lag.
- **Sliding Shopping Cart Drawer**: Animated spring drawer with order total calculations.
- **Dual Web Compilation**: Runs as a desktop binary or compiles to a 14 KB pure JavaScript ESM bundle.

---

## Running Examples Locally

You can run any example directly from the cloned repository:

```bash
# Clone the repository
git clone https://github.com/larvance/strata.git
cd strata

# Run any example target
cargo run --release --example eda_studio
cargo run --release --example creative_studio
cargo run --release --example saas_dashboard
cargo run --release --example ecommerce_storefront
```
