# Reactive Signals DAG

Strata uses a **Fine-Grained Reactive Directed Acyclic Graph (DAG)** for state management instead of virtual DOM re-rendering. When state changes, only the exact layout attributes and text runs that read the signal are invalidated and updated.

---

## Architectural Paradigm: Eliminating the Virtual DOM

In traditional virtual DOM architectures (such as React, Vue, or Electron-wrapped frameworks):

1. Any state mutation triggers a recursive re-execution of component functions from the top down.
2. A duplicate virtual node tree is allocated in memory.
3. The engine recursively diffs the previous virtual tree against the next virtual tree.
4. Layout calculations and draw calls are batched and flushed to the rendering engine.

At 60 FPS or 120 FPS under high-frequency telemetry ingestion (e.g. 10,000 metric updates per second from cloud infrastructure clusters), this process creates heavy memory churn, triggers CPU cache misses, and incurs periodic garbage collection or reallocation pauses.

```
Traditional Virtual DOM Architecture (Recursive Tree Diffing):
[State Mutation] ---> [Re-run Component Tree] ---> [Allocate New VDOM Tree]
                                                               |
                                                               v
[Surgical DOM Flush] <--- [Reconciliation Diff Engine] <-------+
(High allocation pressure, tree-wide traversals, CPU cache churn)

---------------------------------------------------------------------------------

Strata Fine-Grained Reactive DAG (Direct Pointer Mutation):
[Component Mount] ---> [Executes ONCE] ---> [Constructs Reactive DAG Nodes]
                                                          |
                                                          v
[State Mutation] -----------------------------> [Direct Layout Node Mutation]
(Zero tree diffs, constant-time O(1) propagation, zero temporary allocations)
```

Strata eliminates the virtual DOM entirely:
- **Components Run Only Once**: A component function executes exactly once during its mount lifecycle to construct the layout hierarchy and register dependency edges in the reactive graph.
- **Direct Pointer Mutation**: Signals store direct reference handles to the underlying `UiIrNode`.
- **Sub-Nanosecond Micro-Updates**: Mutating an integer metric or status flag updates a single 64-byte node in memory with zero tree traversal.

---

## The Reactive Runtime Core: `ReactiveRuntime`

At the center of Strata's state engine is the `ReactiveRuntime`, a thread-local graph coordinator managing dependency tracking, batch queues, and topological notification flushes.

```
       +-------------------------------------------------------+
       |               ReactiveRuntime (Thread-Local)          |
       +-------------------------------------------------------+
       | - current_subscriber: Cell<Option<SubscriberId>>      |
       | - signal_subscribers: BTreeMap<SignalId, Set<SubId>>  |
       | - subscriber_signals: BTreeMap<SubId, Set<SignalId>>  |
       | - pending_subscribers: RefCell<BTreeSet<SubId>>       |
       | - batch_depth: Cell<usize>                            |
       +---------------------------+---------------------------+
                                   |
                +------------------+------------------+
                |                                     |
         Track Dependency                      Notify Subscribers
                |                                     |
                v                                     v
   +------------------------+             +------------------------+
   |   Signal::get() Read   |             |   Signal::set() Write  |
   | - Registers SubId into |             | - Queues SubIds into   |
   |   signal_subscribers   |             |   pending_subscribers  |
   +------------------------+             | - Flushes if depth == 0|
                                          +------------------------+
```

### Dependency Tracking Mechanics

When a signal is read via `.get()`, the runtime inspects `current_subscriber`. If an effect or memo is currently executing, a bidirectional edge is recorded:
1. `signal_subscribers`: Maps `SignalId -> Set<SubscriberId>` so the signal knows who to notify on write.
2. `subscriber_signals`: Maps `SubscriberId -> Set<SignalId>` so the subscriber can clear obsolete dependency edges before re-running.

### Dynamic Branch Pruning

If your computation contains conditional logic:

```rust
let active_view = use_memo(move || {
    if is_healthy.get() {
        latency_ms.get()
    } else {
        error_code.get()
    }
});
```

When `is_healthy` is `true`, `active_view` subscribes only to `is_healthy` and `latency_ms`. When `is_healthy` becomes `false`, the runtime calls `clear_subscriber_dependencies` prior to re-execution. `latency_ms` is pruned from the dependency set, ensuring subsequent updates to `latency_ms` do not cause spurious re-evaluations while unhealthy.

### Cycle Protection Guard

To prevent infinite loops caused by circular dependencies, `ReactiveRuntime::flush()` enforces a hard limit:

```rust
const MAX_ITERATIONS: usize = 1000;
```

If an effect updates a signal that circularly re-triggers itself more than 1,000 times within a single execution cycle, the engine aborts the flush loop and emits a diagnostic warning:
`"Strata Reactive Guard: Maximum reactive update iterations exceeded. Possible circular dependency."`

---

## Creating Signals: `use_signal`, `use_state`, and `create_signal`

Signals are reactive storage cells with atomic read and write operations.

### Decoupled Read and Write Capabilities

Using `create_signal` or `.split()`, you can separate read-only permissions from mutation capabilities:

```rust
use strata::prelude::*;

// Returns decoupled (ReadSignal<T>, WriteSignal<T>)
let (read_workers, set_workers) = create_signal(16);

// Consumers can only read:
let count = read_workers.get();

// Mutators can only update:
set_workers.set(32);
set_workers.update(|val| *val += 8);
```

### Direct Component Signals: `use_signal`

Inside component bodies, use `use_signal` for combined read-write handles:

```rust
use strata::prelude::*;

#[component]
pub fn ClusterNodeCounter() -> View {
    let node_count = use_signal(4);

    rsx! {
        <div class="flex items-center gap-4 p-4 bg-neutral-900 border border-neutral-800 rounded-xl">
            <span class="text-xs font-mono text-neutral-400">{"ACTIVE NODES:"}</span>
            <span class="text-xl font-bold font-mono text-cyan-400">{node_count.get().to_string()}</span>

            <button
                class="px-3 py-1 bg-cyan-600 hover:bg-cyan-500 text-white text-xs font-bold rounded"
                on_click={move |_| { node_count.update(|n| *n += 1); }}
            >
                {"Add Node"}
            </button>
        </div>
    }
}
```

### Reading Signals Without Subscribing: `untrack`

When reading state inside an effect or handler without registering a dependency edge, use `untrack` or `signal.untracked_get()`:

```rust
use strata_dom::reactive::untrack;

let threshold = use_signal(90.0);
let current_load = use_signal(94.2);

use_effect(move || {
    // We want this effect to trigger when current_load changes,
    // but NOT re-run if someone merely adjusts the alert threshold config:
    let load = current_load.get();
    let max = untrack(|| threshold.get());

    if load > max {
        eprintln!("CRITICAL ALERT: Cluster load {:.1}% exceeds threshold {:.1}%", load, max);
    }
    None
});
```

---

## Derived State: `use_memo`

The `Memo<T>` structure caches its output and only re-executes when its upstream dependencies mutate.

```rust
use strata::prelude::*;

#[component]
pub fn ClusterMetrics(nodes: Signal<Vec<f32>>) -> View {
    // Recalculates average CPU utilization only when the nodes vector changes
    let average_cpu = use_memo(move || {
        let current_nodes = nodes.get();
        if current_nodes.is_empty() {
            0.0
        } else {
            let sum: f32 = current_nodes.iter().sum();
            sum / current_nodes.len() as f32
        }
    });

    // High load indicator
    let is_critical = use_memo(move || {
        average_cpu.get() > 85.0
    });

    rsx! {
        <div class="p-4 bg-neutral-900 border border-neutral-800 rounded-xl">
            <span class="text-xs font-mono text-neutral-400">{"CLUSTER AVG CPU"}</span>
            <div class="text-2xl font-bold font-mono text-neutral-100">
                {format!("{:.1}%", average_cpu.get())}
            </div>

            <div class={if is_critical.get() { "text-red-400 text-xs mt-2" } else { "text-emerald-400 text-xs mt-2" }}>
                {if is_critical.get() { "WARNING: Capacity Warning" } else { "Optimal Load" }}
            </div>
        </div>
    }
}
```

### Diamond Dependency Resolution

Consider a diamond dependency graph where signals $B$ and $C$ depend on $A$, and memo $D$ depends on both $B$ and $C$:

```
        +-------+
        |   A   | (Source Signal)
        +---+---+
            |
      +-----+-----+
      |           |
      v           v
  +-------+   +-------+
  |   B   |   |   C   | (Derived Memos)
  +---+---+   +---+---+
      |           |
      +-----+-----+
            |
            v
        +-------+
        |   D   | (Final Consumer)
        +-------+
```

When $A$ updates, naive reactive libraries execute $D$ twice (once via $B$, then again via $C$). Strata's `ReactiveRuntime` queues pending subscribers in a topological order, ensuring $D$ executes **exactly once** after both $B$ and $C$ have finished settling.

---

## Side Effects: `use_effect`

The `Effect` primitive executes arbitrary side-effects (network polling, WebSocket streaming, timers, hardware polling) whenever tracked signals change.

### Teardown and Cleanup Closures

An effect closure can return an optional cleanup closure (`Option<Box<dyn FnOnce()>>`). The cleanup closure executes before the effect runs again, or when the effect is disposed on component unmount:

```rust
use strata::prelude::*;

#[component]
pub fn TelemetrySocketWatcher(cluster_endpoint: Signal<String>) -> View {
    use_effect(move || {
        let endpoint = cluster_endpoint.get();
        println!("Opening telemetry WebSocket to: {}", endpoint);
        
        let socket_handle = open_cluster_stream(&endpoint);

        // Cleanup closure executed before re-connecting or on unmount
        Some(Box::new(move || {
            println!("Closing telemetry WebSocket to: {}", endpoint);
            socket_handle.close();
        }))
    });

    rsx! {
        <div class="text-xs font-mono text-neutral-500">
            {"STREAM ACTIVE"}
        </div>
    }
}
```

---

## Atomic State Batching: `batch`

When modifying multiple signals in response to a single event (such as receiving an aggregated telemetry frame), wrapping modifications in `batch` suppresses intermediate notifications:

```rust
use strata_dom::reactive::batch;

let set_cpu = use_signal(24.5);
let set_memory = use_signal(1024);
let set_healthy = use_signal(true);

// Multiple signal writes:
batch(|| {
    set_cpu.set(88.4);
    set_memory.set(4096);
    set_healthy.set(false);
});
// Subscribers and downstream memos are notified ONCE with all three updates applied.
```

Batch operations can be nested arbitrarily; notifications are deferred until the outermost batch scope completes (`batch_depth == 0`).

---

## Struct-Level Reactive Stores: `#[derive(ReactiveStore)]`

For complex business state models, managing dozens of individual scalar signals introduces cognitive overhead. Strata provides `#[derive(ReactiveStore)]` in `strata_store`:

```rust
use strata_store::prelude::*;

#[derive(ReactiveStore, Clone, Debug, Default, PartialEq)]
pub struct ServiceMeshTopology {
    pub service_name: String,
    pub active_replicas: u32,
    pub p99_latency_ms: f32,
    pub egress_error_rate: f32,
}
```

The derive macro creates `ServiceMeshTopologyStore`, where each field is an independent `Signal<T>`.

```rust
let mesh = ServiceMeshTopology {
    service_name: "payment-gateway".to_string(),
    active_replicas: 8,
    p99_latency_ms: 12.4,
    egress_error_rate: 0.001,
}.into_store();

// Access individual atomic signals directly
let latency_sig = mesh.p99_latency_ms.clone();

// Batch update multiple struct fields atomically
mesh.batch(|s| {
    s.active_replicas.set(12);
    s.p99_latency_ms.set(18.2);
});

// Non-reactive snapshotting
let snap = mesh.snapshot();
assert_eq!(snap.active_replicas, 12);
```

---

## Production Blueprint: Cluster Observability Dashboard

Below is a complete, production-grade cloud infrastructure monitoring component demonstrating signals, derived memos, side-effect streaming, batch updates, and store derives:

```rust
use strata_dom::prelude::*;
use strata_macro::{component, rsx};
use strata_store::prelude::*;

#[derive(ReactiveStore, Clone, Debug, Default, PartialEq)]
pub struct ClusterNodeTelemetry {
    pub node_id: String,
    pub cpu_percent: f32,
    pub memory_gb_used: f32,
    pub network_in_mbps: f32,
    pub is_cordoned: bool,
}

#[component]
pub fn ProductionClusterMonitor() -> View {
    // 1. Initialize struct-level reactive store
    let node_store = ClusterNodeTelemetry {
        node_id: "node-us-east-1a-compute-09".to_string(),
        cpu_percent: 34.2,
        memory_gb_used: 18.5,
        network_in_mbps: 450.0,
        is_cordoned: false,
    }.into_store();

    // 2. Derived computed metrics (Memos)
    let cpu_sig = node_store.cpu_percent.clone();
    let is_cordoned_sig = node_store.is_cordoned.clone();

    let health_status = use_memo(move || {
        if is_cordoned_sig.get() {
            "CORDONED"
        } else if cpu_sig.get() > 80.0 {
            "HIGH LOAD"
        } else {
            "HEALTHY"
        }
    });

    let cpu_bar_width = use_memo(move || {
        format!("width: {:.1}%;", cpu_sig.get().min(100.0))
    });

    // 3. Side-effect simulating high-frequency telemetry ingestion
    let store_ref = node_store.clone();
    use_effect(move || {
        println!("Starting telemetry ingestion loop...");
        let active = std::rc::Rc::new(std::cell::Cell::new(true));
        let active_clone = active.clone();
        let store_handle = store_ref.clone();

        // In practice, this consumes from a WebSocket or QUIC channel
        let interval = set_interval(move || {
            if !active_clone.get() { return; }
            store_handle.batch(|s| {
                s.cpu_percent.set(78.5);
                s.memory_gb_used.set(22.1);
                s.network_in_mbps.set(620.0);
            });
        }, 1000);

        // Teardown closure
        Some(Box::new(move || {
            active.set(false);
            clear_interval(interval);
            println!("Telemetry ingestion loop halted.");
        }))
    });

    // 4. Reactive RSX View
    rsx! {
        <div class="p-6 bg-neutral-950 border border-neutral-800 rounded-2xl max-w-xl flex flex-col gap-6">
            <div class="flex items-center justify-between border-b border-neutral-800 pb-4">
                <div>
                    <h2 class="text-sm font-bold font-mono text-neutral-200">
                        {node_store.node_id.get()}
                    </h2>
                    <span class="text-xs font-mono text-neutral-500">
                        {"STATUS: "}{health_status.get()}
                    </span>
                </div>

                <button
                    class="px-3 py-1.5 text-xs font-mono bg-neutral-800 hover:bg-neutral-700 text-neutral-300 rounded border border-neutral-700"
                    on_click={move |_| {
                        let cur = node_store.is_cordoned.get();
                        node_store.is_cordoned.set(!cur);
                    }}
                >
                    {if node_store.is_cordoned.get() { "Uncordon" } else { "Cordon Node" }}
                </button>
            </div>

            <!-- CPU Utilization Bar -->
            <div>
                <div class="flex justify-between text-xs font-mono mb-2">
                    <span class="text-neutral-400">{"CPU UTILIZATION"}</span>
                    <span class="text-cyan-400 font-bold">{format!("{:.1}%", node_store.cpu_percent.get())}</span>
                </div>
                <div class="h-2 w-full bg-neutral-800 rounded-full overflow-hidden">
                    <div
                        class="h-full bg-cyan-500 transition-all duration-300"
                        style={cpu_bar_width.get()}
                    />
                </div>
            </div>

            <!-- Memory & Network Metrics Grid -->
            <div class="grid grid-cols-2 gap-4 font-mono text-xs">
                <div class="p-3 bg-neutral-900 border border-neutral-800 rounded-lg">
                    <span class="text-neutral-500 block mb-1">{"MEMORY USED"}</span>
                    <span class="text-base font-bold text-neutral-200">
                        {format!("{:.1} GB", node_store.memory_gb_used.get())}
                    </span>
                </div>
                <div class="p-3 bg-neutral-900 border border-neutral-800 rounded-lg">
                    <span class="text-neutral-500 block mb-1">{"NETWORK INGRESS"}</span>
                    <span class="text-base font-bold text-emerald-400">
                        {format!("{:.0} Mbps", node_store.network_in_mbps.get())}
                    </span>
                </div>
            </div>
        </div>
    }
}
```

---

## Best Practices and Diagnostic Guidelines

1. **Avoid Reading Signals in Write Handlers Without Intent**: If an effect reads a signal inside a condition, that signal becomes a permanent dependency until the effect re-runs. Use `untrack(|| signal.get())` for ambient reads.
2. **Batch High-Frequency WebSocket Telemetry**: When parsing binary telemetry packets, always wrap multi-signal updates in `batch(|| { ... })` to avoid intermediate layouts and paint calls.
3. **Keep Memo Functions Pure**: Memos must be deterministic calculations with no side-effects. Network calls, filesystem operations, and timer setups belong exclusively in `use_effect`.
4. **Use `#[derive(ReactiveStore)]` for Domain Models**: When domain models exceed 3-4 fields, prefer struct-level store derives to gain built-in snapshotting, batch transactions, and selector slices.
