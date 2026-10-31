# Strata Example: Creative Studio

A high-performance vector design studio and canvas editor written in **pure Rust** utilizing Strata's future syntax, reactive signals, and Tailwind styling engine.

## Key Architectural Features
- **Zero Chromium:** Hardware-accelerated 120 FPS infinite canvas running directly on Metal / Vulkan GPU pipelines.
- **Floating Tool Dock:** Keyboard shortcuts (V, F, R, O, T, H) with instant reactive tool switching.
- **Layers Hierarchy:** Tree view showing visibility toggles, lock states, and active selection states.
- **Real-Time Property Inspector:** Live X/Y coordinates, dimensions, corner radius slider, fill colors, and opacity adjustments with immediate visual canvas updates.
- **Tailwind Utility Classes:** Complex multi-panel design studio layout built completely with Tailwind utility classes (`bg-neutral-950`, `border-neutral-800`, `select-none`, `backdrop-blur-md`).

## Running the Example
```bash
#Run natively on your desktop (macOS / Windows / Linux):
cargo run -p creative_studio

#Or build for the web (WASM):
cargo build -p creative_studio --target wasm32-unknown-unknown
```
