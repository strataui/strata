# Strata Example: Apex Athletics Commerce Storefront

A high-performance technical apparel e-commerce experience written in **pure Rust** utilizing Strata's future syntax, reactive signals, and Tailwind styling engine.

## Key Architectural Features
- **Zero Chromium:** Renders at native 120 FPS directly onto GPU surfaces with $<15$MB memory consumption.
- **Dynamic Array & Variant Methods:** Utilizes `strata_js`-style dynamic operations for filtering catalog arrays, calculating cart summaries, and handling coupon codes.
- **Derived Memos & Reactive Cart:** Real-time shopping bag counter, quantity steppers, subtotal, and tax derivation without virtual DOM diffing overhead.
- **Tailwind Utility Classes:** Clean, modern responsive e-commerce layout built entirely using Tailwind classes (`bg-neutral-950`, `border-neutral-800`, `grid-cols-3`, `hover:scale-110`).
- **Interactive Slide-over Drawer:** Cart drawer with promo code validation and checkout trigger.

## Running the Example
```bash
#Run natively on your desktop (macOS / Windows / Linux):
cargo run -p ecommerce_storefront

#Or build for the web (WASM):
cargo build -p ecommerce_storefront --target wasm32-unknown-unknown
```
