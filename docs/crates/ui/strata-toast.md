# strata_toast: Sonner-Tier Stacked Notification Engine

The `strata_toast` crate provides a reactive and imperative toast notification pipeline for the Strata engine. Modeled after the physical interaction paradigms of Sonner, `strata_toast` features 3D stacked card depth layouts, asynchronous promise-driven toast state transitions (Loading -> Success / Error), multi-anchor viewport positioning, interactive action buttons, and automatic dismiss timers.

---

## Architectural Principles

The toast notification system separates notification queuing from physical layout positioning:

```text
┌─────────────────────────────────────────────────────────────┐
│                 Global Toast Dispatch Stream                │
│       (toast_success, toast_error, toast_warning, ...)      │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                      ToastManager State                     │
│    (toasts: Signal<Vec<ToastItem>>, ID sequencing, timers)  │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                 Spatial 3D Stacking Engine                  │
│       compute_stack_layout(toasts, position, max_visible)   │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│               Stacked Visual Transformations                │
│     Depth 0 (Front): scale: 1.00 • offset_y:   0px • z: 50  │
│     Depth 1 (Mid):   scale: 0.94 • offset_y: -14px • z: 49  │
│     Depth 2 (Back):  scale: 0.88 • offset_y: -28px • z: 48  │
└─────────────────────────────────────────────────────────────┘
```

1. **Card Depth Stacking**: Rather than pushing old toasts down and cluttering vertical screen space, newest notifications rest in the foreground while prior toasts slide back along a scaled Z-depth stack.
2. **Asynchronous Resolution**: Long-running background operations (such as container deployments or database migrations) display a persistent loading card that mutates in-place upon completion.
3. **Thread-Safe Imperative & Declarative API**: Dispatch notifications from asynchronous Tokio tasks, gRPC background streams, or declarative event handlers.

---

## Core Types and Data Models

### Notification Kinds: `ToastKind`

Categorizes notifications and drives default icons and styling tokens:

```rust
use strata_toast::prelude::ToastKind;

let kind_default = ToastKind::Default; // Standard neutral status
let kind_success = ToastKind::Success; // Emerald confirmation badge
let kind_error = ToastKind::Error;     // Rose error indicator
let kind_warning = ToastKind::Warning; // Amber caution banner
let kind_info = ToastKind::Info;       // Sky informational note
let kind_loading = ToastKind::Loading; // Indeterminate spinner (indefinite duration)
```

---

### Viewport Screen Anchors: `ToastPosition`

Configures the corner or edge where toasts stack:

```rust
use strata_toast::prelude::ToastPosition;

// Bottom alignments expand upwards
let br = ToastPosition::BottomRight; // Standard desktop telemetry position
let bl = ToastPosition::BottomLeft;
let bc = ToastPosition::BottomCenter;

// Top alignments expand downwards
let tr = ToastPosition::TopRight;
let tl = ToastPosition::TopLeft;
let tc = ToastPosition::TopCenter;
```

---

### Toast Notification Item: `ToastItem`

Defines notification payload, duration, and optional interactive callback:

```rust
use strata_toast::prelude::{ToastAction, ToastItem, ToastKind};

let item = ToastItem::new(101, "Cluster Auto-Scale Event", ToastKind::Info)
    .with_description("Added 8 compute replicas to us-east-01 in response to QPS surge.")
    .with_duration(6000) // 6 seconds auto-dismiss
    .with_action("View Topology", || {
        println!("Navigating to cluster visualizer...");
    });
```

#### API Specification: `ToastItem`

| Method / Field | Type | Description |
| :--- | :--- | :--- |
| `id` | `u64` | Monotonically increasing unique notification identifier. |
| `title` | `String` | Primary headline message. |
| `description` | `Option<String>` | Supplementary multiline context text. |
| `kind` | `ToastKind` | Semantic status categorization. |
| `duration_ms` | `u64` | Milliseconds before auto-dismissal (`0` denotes persistent until manually dismissed or updated). |
| `dismissible` | `bool` | Displays close icon trigger when `true`. |
| `with_description(desc)` | `fn(impl Into<String>) -> Self` | Builder pattern method attaching secondary text. |
| `with_duration(ms)` | `fn(u64) -> Self` | Builder pattern method overriding auto-dismiss timeout. |
| `with_action(label, on_click)` | `fn(impl Into<String>, impl Fn() + 'static) -> Self` | Builder pattern method embedding an interactive button. |

---

## The Toast Manager: `ToastManager`

Manages the active notification queue, sequencing, and stack projections:

```rust
use strata_toast::prelude::{ToastKind, ToastManager, ToastPosition};

let manager = ToastManager::new();

// Immediate helper dispatchers
let id1 = manager.success("Deployment rolled out successfully!");
let id2 = manager.error("TLS Certificate validation failure on edge-02");
let id3 = manager.warning("High memory pressure detected on clickhouse-01");

// Manual dismissal
manager.dismiss(id1);

// Clear entire stack
manager.clear();
```

---

## Asynchronous Promise / Loading Toast Pipelines

For multi-stage async cloud operations, start an indefinite `ToastKind::Loading` notification and resolve it atomically when the future completes:

```rust
use strata_toast::prelude::{ToastKind, ToastManager};

let manager = ToastManager::new();

// Step 1: Initiate long-running async migration task
let migration_toast_id = manager.loading("Migrating Redis cluster to partition v2...");

// Verify loading toast has zero auto-dismiss timeout
let active = manager.toasts.get();
assert_eq!(active[0].kind, ToastKind::Loading);
assert_eq!(active[0].duration_ms, 0);

// Step 2: Background task resolves successfully
let success = manager.update(
    migration_toast_id,
    ToastKind::Success,
    "Redis partition v2 migration finished without downtime!"
);
assert!(success);

// Toast is now marked as Success with standard 4000ms dismiss timer
let updated = manager.toasts.get();
assert_eq!(updated[0].kind, ToastKind::Success);
assert_eq!(updated[0].duration_ms, 4000);
```

---

## 3D Spatial Stacking Engine: `compute_stack_layout`

Calculates exact translation offsets, 3D scale factors, and opacity layers:

```rust
use strata_toast::prelude::{compute_stack_layout, ToastItem, ToastKind, ToastPosition};

let active_toasts = vec![
    ToastItem::new(1, "Foreground Notification", ToastKind::Success),
    ToastItem::new(2, "Middle Layer Notification", ToastKind::Info),
    ToastItem::new(3, "Background Layer Notification", ToastKind::Warning),
];

// Compute visual stack layout for BottomRight anchor, maximum 3 visible
let visuals = compute_stack_layout(&active_toasts, ToastPosition::BottomRight, 3);

// Card 0 (Frontmost)
assert_eq!(visuals[0].offset_y, 0.0);
assert_eq!(visuals[0].scale, 1.0);
assert_eq!(visuals[0].z_index, 50);

// Card 1 (Mid Layer: scaled down 6%, translated up 14px)
assert_eq!(visuals[1].offset_y, -14.0);
assert_eq!(visuals[1].scale, 0.94);
assert_eq!(visuals[1].z_index, 49);

// Card 2 (Back Layer: scaled down 12%, translated up 28px)
assert_eq!(visuals[2].offset_y, -28.0);
assert_eq!(visuals[2].scale, 0.88);
assert_eq!(visuals[2].z_index, 48);
```

---

## Global Standalone Functions

For quick notifications without explicitly passing around manager instances:

```rust
use strata_toast::prelude::{toast_dismiss, toast_error, toast_info, toast_success, toast_warning};

let id = toast_success("Configuration saved.");
toast_info("Background sync initiated.");
toast_warning("API rate limit at 80%.");
toast_error("Gateway connection timed out.");

toast_dismiss(id);
```

---

## Production Component Example: Sonner-Style Toast Stack Renderer

```rust
use strata_dom::prelude::*;
use strata_icons::prelude::IconName;
use strata_toast::prelude::*;
use strata_ui::prelude::Icon;

#[component]
pub fn ToastHost(manager: ToastManager) -> View {
    let visuals = manager.stacked_toasts();

    rsx! {
        <div class="fixed bottom-6 right-6 z-50 flex flex-col pointer-events-none select-none font-sans">
            <div class="relative w-96 h-20">
                {visuals.into_iter().map(|visual| {
                    let id = visual.item.id;
                    let (icon, border_color, text_color) = match visual.item.kind {
                        ToastKind::Success => (IconName::CheckCircle, "border-emerald-800/80", "text-emerald-400"),
                        ToastKind::Error => (IconName::AlertCircle, "border-rose-800/80", "text-rose-400"),
                        ToastKind::Warning => (IconName::AlertTriangle, "border-amber-800/80", "text-amber-400"),
                        ToastKind::Info => (IconName::Info, "border-sky-800/80", "text-sky-400"),
                        ToastKind::Loading => (IconName::RefreshCw, "border-indigo-800/80", "text-indigo-400 animate-spin"),
                        ToastKind::Default => (IconName::Bell, "border-neutral-800", "text-neutral-400"),
                    };

                    let manager_clone = manager.clone();

                    rsx! {
                        <div
                            class="absolute bottom-0 right-0 w-full p-4 bg-neutral-900/95 backdrop-blur-xl border rounded-xl shadow-2xl pointer-events-auto transition-all duration-200 {border_color}"
                            style="transform: translateY({visual.offset_y}px) scale({visual.scale}); z-index: {visual.z_index}; opacity: {visual.opacity};"
                        >
                            <div class="flex items-start gap-3">
                                <div class="mt-0.5 {text_color}">
                                    {Icon(icon, Some("w-5 h-5"))}
                                </div>
                                <div class="flex-1 min-w-0">
                                    <h4 class="text-xs font-semibold text-neutral-200 truncate">
                                        {visual.item.title}
                                    </h4>
                                    {if let Some(desc) = visual.item.description {
                                        rsx! {
                                            <p class="text-[11px] text-neutral-400 mt-0.5 line-clamp-2">
                                                {desc}
                                            </p>
                                        }
                                    } else {
                                        rsx! { <span class="hidden" /> }
                                    }}
                                </div>
                                {if visual.item.dismissible {
                                    rsx! {
                                        <button
                                            class="text-neutral-500 hover:text-white p-1 transition-colors"
                                            onclick={move |_| manager_clone.dismiss(id)}
                                        >
                                            {Icon(IconName::X, Some("w-3.5 h-3.5"))}
                                        </button>
                                    }
                                } else {
                                    rsx! { <span class="hidden" /> }
                                }}
                            </div>
                        </div>
                    }
                })}
            </div>
        </div>
    }
}
```
