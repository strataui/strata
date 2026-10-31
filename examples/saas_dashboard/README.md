# Strata Example: Enterprise AI Cloud Dashboard

A production-grade infrastructure analytics dashboard written in **pure Rust** utilizing Strata's future syntax, reactive signals, and Tailwind styling engine.

## Key Architectural Features
- **Zero Chromium:** Renders at native 120 FPS directly onto GPU surfaces with $<15$MB memory consumption.
- **Declarative RSX:** Modern JSX/HTML syntax embedded natively in Rust.
- **Fine-Grained Signals:** Real-time updates to throughput, GPU load, and P99 latency bypass full virtual-DOM diffing passes.
- **Tailwind Utility Classes:** High-density enterprise layout built entirely using Tailwind classes (`bg-neutral-950`, `border-neutral-800`, `grid-cols-4`, `backdrop-blur-md`).
- **Interactive Modals:** Reactive slide-over modal for provisioning GPU model clusters with dynamic cost recalculation.

## Running the Example
```bash
#Run natively on your desktop (macOS / Windows / Linux):
cargo run -p saas_dashboard

#Or build for the web (WASM):
cargo build -p saas_dashboard --target wasm32-unknown-unknown
```
