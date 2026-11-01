# Crate `strata_dom`: Reactive Signal Graph & Intermediate Representation

The `strata_dom` crate implements the reactive foundation and intermediate presentation tree for Project Strata. It couples a SolidJS-grade, fine-grained reactive dependency graph with a 64-byte cache-aligned Intermediate Representation (`UiIrNode`).

By discarding traditional Virtual DOM reconcilers and recursive tree diffing, `strata_dom` achieves direct $O(1)$ state-to-layout propagation with sub-millisecond dispatch cycles.

---

## Architectural Systems

```text
┌────────────────────────────────────────────────────────┐
│                   Application Code                     │
│               (Signals, Memos, Effects)                │
└───────────────────────────┬────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────┐
│            Reactive Runtime DAG (reactive.rs)          │
│    Signal<T>, SpringSignal, Dynamic Subscriber Sets    │
└───────────────────────────┬────────────────────────────┘
                            │ (Direct Invalidation)
                            ▼
┌────────────────────────────────────────────────────────┐
│             UI IR Graph Engine (ir.rs)                 │
│      Flat Array of 64-Byte Cache-Aligned UiIrNodes     │
└───────────────────────────┬────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────┐
│            Synthetic Event Pipeline (events.rs)        │
│       Capturing, Target, and Bubbling Propagation      │
└────────────────────────────────────────────────────────┘
```

---

## The Reactive Dependency Graph (`reactive`)

Strata uses automatic runtime dependency tracking based on a thread-local Directed Acyclic Graph (DAG). Components execute once to build the initial layout tree and register signal listeners. Subsequent mutations bypass component re-execution entirely.

### Central Coordinator: `ReactiveRuntime`

The thread-local `ReactiveRuntime` coordinates dependency registration, batching, and topological propagation:

```rust
pub struct ReactiveRuntime {
    current_subscriber: Cell<Option<SubscriberId>>,
    signal_subscribers: RefCell<BTreeMap<SignalId, BTreeSet<SubscriberId>>>,
    subscriber_signals: RefCell<BTreeMap<SubscriberId, BTreeSet<SignalId>>>,
    subscribers: RefCell<BTreeMap<SubscriberId, Rc<RefCell<dyn FnMut()>>>>,
    pending_subscribers: RefCell<BTreeSet<SubscriberId>>,
    batch_depth: Cell<usize>,
    next_signal_id: Cell<SignalId>,
    next_subscriber_id: Cell<SubscriberId>,
    next_scope_id: Cell<ScopeId>,
}
```

### Dependency Tracking Mechanics

1. **Automatic Tracking on Read (`track`)**:
   When `signal.get()` or `signal.read()` is invoked, the signal queries `current_subscriber`. If an active subscriber (e.g. an effect or memo) is executing, the signal's `SignalId` and the subscriber's `SubscriberId` are bidirectionally recorded in the dependency sets.

2. **Clean Dependency Pruning**:
   Before an effect or memo re-runs, `clear_subscriber_dependencies(sub_id)` removes stale edges from the DAG. This guarantees that conditional branching (`if condition { a.get() } else { b.get() }`) never retains zombie subscriptions to inactive branches.

3. **Batched Propagation (`batch`)**:
   Multiple signal mutations can be batched together to coalesce subscriber notifications:

   ```rust
   strata_dom::batch(|| {
       cpu_usage.set(88.4);
       memory_pressure.set(true);
       active_workers.update(|w| *w += 4);
   }); // All dependents notified once upon scope exit
   ```

4. **Cycle Protection Guard**:
   To prevent unbounded circular cascades, `ReactiveRuntime::flush()` enforces a hard limit of 1,000 iterations per flush cycle, triggering an error if an infinite reactive feedback loop is detected.

5. **Untracked Reads (`untrack`)**:
   Allows inspecting signal values inside reactive scopes without creating dependency subscriptions:

   ```rust
   let current_baseline = strata_dom::untrack(|| baseline_signal.get());
   ```

### Core Reactive Primitives

- `Signal<T>`: Read-write fine-grained scalar container (`get`, `set`, `update`, `with`, `split`).
- `ReadSignal<T>` / `WriteSignal<T>`: Decoupled single-capability handles.
- `use_memo(f)`: Lazily evaluated, memoized derived signal that only invalidates downstream consumers when its computed output changes.
- `use_effect(f)`: Reactive computation running side effects upon dependency invalidation.

---

## Spring Physics Dynamics (`SpringSignal`)

For fluid UI telemetry meters, scale factors, and visual indicators, `strata_dom::spring` provides `SpringSignal`, an analytically integrated mass-spring-damper oscillator:

$$m \frac{d^2 x}{dt^2} + c \frac{dx}{dt} + k (x - x_{\text{target}}) = 0$$

Where:
- $m$: Mass (`config.mass`)
- $c$: Damping coefficient (`config.damping`)
- $k$: Stiffness tension (`config.stiffness`)

```rust
use strata_dom::spring::{SpringConfig, SpringSignal};

// Crisp, snappy spring preset for telemetry meters
let spring = SpringSignal::with_config(0.0, SpringConfig::stiff());

// Set target value
spring.set_target(100.0);

// Advance physics simulation by elapsed time step (seconds)
spring.advance(0.016); // 16ms frame step

let current_position = spring.get();
assert!(current_position > 0.0);
```

### Pre-Configured Physical Presets
- `SpringConfig::stiff()`: Tension 210, Damping 20, Mass 1 (fast settling, minimal overshoot).
- `SpringConfig::gentle()`: Tension 120, Damping 14, Mass 1 (smooth deceleration).
- `SpringConfig::wobbly()`: Tension 180, Damping 12, Mass 1 (expressive oscillation).
- `SpringConfig::bouncy()`: Tension 240, Damping 10, Mass 1 (high energy).

---

## The 64-Byte Cache-Aligned `UiIrNode` (`ir`)

Conventional web browsers maintain DOM elements as multi-kilobyte heap structures populated with pointer graphs. In contrast, Strata stores layout nodes in a contiguous arena as 64-byte structs (`UiIrNode`) matching the exact cache-line size of modern x86-64 and ARM64 CPUs.

```rust
#[repr(C, align(64))]
#[derive(Debug, Clone, Copy)]
pub struct UiIrNode {
    // 8-byte aligned fields (16 bytes)
    pub memo_hash: u64,                   // Offset  0..8
    pub display_chunk: DisplayChunkRange, // Offset  8..16

    // 4-byte aligned fields (44 bytes)
    pub layout_rect: FixedRect,           // Offset 16..32 (16 bytes: x, y, w, h in 1/64th px)
    pub id: NodeId,                       // Offset 32..36
    pub parent: NodeId,                   // Offset 36..40
    pub first_child: NodeId,              // Offset 40..44
    pub next_sibling: NodeId,             // Offset 44..48
    pub child_count: u32,                 // Offset 48..52
    pub style_id: u32,                    // Offset 52..56
    pub subtree_span: u32,                // Offset 56..60

    // 2-byte aligned fields (2 bytes)
    pub flags: u16,                       // Offset 60..62

    // 1-byte aligned fields (2 bytes)
    pub tag: IrTag,                       // Offset 62..63
    pub _reserved0: u8,                   // Offset 63..64
}
```

```text
 0      8      16                32   36   40   44   48   52   56   60  62 63 64
┌──────┬──────┬─────────────────┬────┬────┬────┬────┬────┬────┬────┬───┬──┬──┐
│memo  │chunk │layout_rect      │id  │prnt│chld│sibl│cnt │styl│span│flg│tg│r │
│hash  │range │(16B FixedRect)  │    │    │    │    │    │    │    │   │  │  │
└──────┴──────┴─────────────────┴────┴────┴────┴────┴────┴────┴────┴───┴──┴──┘
 [---- 16B ---][----- 16B ------][---------- 28B ----------][ 2B][1B][1B]
 Total = Exactly 64 Bytes (Single CPU L1 Cache Line)
```

### Memory Footprint & Hardware Benefits
- **Zero Cache Miss Spans**: Accessing a `UiIrNode` loads its identity, hierarchy pointers, flags, and layout box into L1 cache in a single memory transaction.
- **Cache Saturation**: A tree of 16,384 nodes occupies exactly 1 Megabyte of contiguous memory, fitting entirely within modern L2/L3 CPU caches.
- **Index Handles**: References use 32-bit `NodeId` integers rather than 64-bit pointers, halving memory consumption and preventing dangling pointers.

### Node Flags (`IrNodeFlags`)
Bitflags encode element state and optimization metadata:
- `HAS_DIRTY_LAYOUT` (`1 << 0`): Queues node for relayout.
- `HAS_DIRTY_STYLE` (`1 << 1`): Queues node for style cascade re-evaluation.
- `IS_CONTAINMENT_BOUNDARY` (`1 << 2`): Declares layout containment; child mutations cannot affect external ancestors.
- `IS_TEXT_LEAF` (`1 << 3`): Text content node.
- `IS_VISIBLE` (`1 << 4`): Rendered visibility state.
- `HAS_EVENT_LISTENERS` (`1 << 5`): Accelerates event target hit-testing.
- `HAS_MEMO_CACHE` (`1 << 6`): Memoized presentation chunk.

---

## Synthetic Event Dispatching Pipeline (`events`)

`strata_dom::events` implements synthetic event routing mirroring W3C DOM specifications across three distinct propagation stages:

```text
                     1. Capturing Stage
                 Window ─────────────► Root
                                        │
                                        ▼
                                   Parent Node
                                        │
                                        ▼
                     2. Target Stage   Target Element
                                        │
                                        ▼
                                   Parent Node
                                        │
                                        ▼
                     3. Bubbling Stage Root ─────────────► Window
```

### Event Data Model

```rust
pub struct Event {
    pub event_type: EventType,
    pub target_id: Option<u32>,
    pub current_target_id: Cell<Option<u32>>,
    pub propagation_stopped: Cell<bool>,
    pub default_prevented: Cell<bool>,
    pub value: String,
    pub client_x: f64,
    pub client_y: f64,
    pub button: u8,
    pub key: String,
    pub code: String,
    pub ctrl_key: bool,
    pub shift_key: bool,
    pub alt_key: bool,
    pub meta_key: bool,
}
```

### Event Control Flow
- `event.stop_propagation()`: Halts further traversal across ancestor or descendant listeners.
- `event.prevent_default()`: Signals the runtime to suppress default platform interactions.

---

## Production Cloud Infrastructure Example: Kubernetes Autoscaler Monitor

The following production-grade component tracks real-time cluster autoscaling metrics with animated spring-physics indicators and fine-grained signal subscriptions:

```rust
use strata_dom::prelude::*;
use strata_dom::spring::{SpringConfig, SpringSignal};

pub struct ClusterAutoscalerState {
    pub desired_replicas: Signal<u32>,
    pub active_replicas: Signal<u32>,
    pub cpu_utilization_pct: Signal<f32>,
    pub visual_gauge_spring: SpringSignal,
}

impl ClusterAutoscalerState {
    pub fn new(initial_replicas: u32) -> Self {
        Self {
            desired_replicas: Signal::new(initial_replicas),
            active_replicas: Signal::new(initial_replicas),
            cpu_utilization_pct: Signal::new(45.0),
            visual_gauge_spring: SpringSignal::with_config(45.0, SpringConfig::stiff()),
        }
    }

    pub fn set_metric(&self, cpu_pct: f32) {
        self.cpu_utilization_pct.set(cpu_pct);
        self.visual_gauge_spring.set_target(cpu_pct);

        // Automatic scale calculation
        if cpu_pct > 80.0 {
            let next = (self.active_replicas.get() + 2).min(32);
            self.desired_replicas.set(next);
        } else if cpu_pct < 30.0 {
            let next = self.active_replicas.get().saturating_sub(1).max(2);
            self.desired_replicas.set(next);
        }
    }

    pub fn tick_physics(&self, delta_seconds: f32) {
        self.visual_gauge_spring.advance(delta_seconds);
    }
}

pub fn render_autoscaler_status(state: &ClusterAutoscalerState) -> View {
    let desired = state.desired_replicas.clone();
    let active = state.active_replicas.clone();
    let gauge = state.visual_gauge_spring.clone();

    // Derived memo for scale direction indicator
    let scale_status = use_memo({
        let d = desired.clone();
        let a = active.clone();
        move || {
            let desired_val = d.get();
            let active_val = a.get();
            if desired_val > active_val {
                "Scaling Up (Provisioning Pods)"
            } else if desired_val < active_val {
                "Scaling Down (Terminating Pods)"
            } else {
                "Optimal (Steady State)"
            }
        }
    });

    View::new(NodeBlueprint::element(
        Tag::Section,
        vec!["flex".into(), "flex-col".into(), "p-4".into(), "bg-neutral-900".into(), "rounded-lg".into(), "gap-3".into()],
        vec![],
        vec![
            NodeBlueprint::element(
                Tag::Header,
                vec!["flex".into(), "justify-between".into(), "items-center".into()],
                vec![],
                vec![
                    NodeBlueprint::text("Autoscaler Controller: worker-v2"),
                    NodeBlueprint::text(scale_status.get()),
                ],
            ),
            NodeBlueprint::element(
                Tag::Div,
                vec!["w-full".into(), "bg-neutral-800".into(), "h-2".into(), "rounded-full".into(), "overflow-hidden".into()],
                vec![],
                vec![
                    NodeBlueprint::element(
                        Tag::Div,
                        vec!["bg-cyan-500".into(), "h-full".into()],
                        vec![Attribute::property("style", format!("width: {:.1}%;", gauge.get().clamp(0.0, 100.0)))],
                        vec![],
                    )
                ],
            ),
            NodeBlueprint::element(
                Tag::Footer,
                vec!["flex".into(), "gap-4".into(), "text-xs".into(), "font-mono".into(), "text-neutral-400".into()],
                vec![],
                vec![
                    NodeBlueprint::text(format!("Active: {}", active.get())),
                    NodeBlueprint::text(format!("Desired: {}", desired.get())),
                    NodeBlueprint::text(format!("Observed CPU: {:.1}%", gauge.get())),
                ],
            ),
        ],
    ))
}
```
