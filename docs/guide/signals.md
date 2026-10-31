# Reactive Signals DAG

Strata uses a **Fine-Grained Reactive Directed Acyclic Graph (DAG)** for state management instead of virtual DOM re-rendering. When state changes, only the exact layout attributes and text runs that read the signal are invalidated and updated.

---

## Why Signals Outperform Virtual DOM

In traditional React-style virtual DOM architectures:
1. A state change triggers a re-execution of component functions from the top down.
2. A new virtual DOM tree is constructed in memory.
3. The engine recursively diffs the old virtual tree against the new tree.
4. Changes are batched and flushed to the rendering engine.

At 60 FPS or 120 FPS, this process allocates thousands of temporary objects per frame, triggering frequent garbage collection spikes.

Strata eliminates the virtual DOM entirely:
- **Zero VDOM Diffs**: Components execute **only once** to establish the reactive dependency graph.
- **Direct Pointer Mutation**: Signals store direct reference handles to the underlying `UiIrNode`.
- **Micro-Updates**: Mutating a numeric counter or hover state updates a single 64-byte node in memory with zero tree traversal.

---

## Creating Signals: `use_signal` and `use_state`

Use `use_signal` to define reactive variables:

```rust
use strata::prelude::*;

#[component]
pub fn Counter() -> View {
    let (count, set_count) = use_signal(|| 0);

    rsx! {
        <div class="flex items-center gap-4 p-4 bg-neutral-900 rounded-xl border border-neutral-800">
            <span class="text-white font-mono text-xl">{count()}</span>

            <Button
                variant="primary"
                on_click={move |_| set_count.update(|val| *val += 1)}
            >
                Increment
            </Button>
        </div>
    }
}
```

- **Reading**: `count()` registers the current reactive context as a subscriber.
- **Writing**: `set_count.set(newValue)` or `set_count.update(|val| ...)` notifies all registered subscribers immediately.

---

## Derived State: `use_memo`

`use_memo` creates a read-only computed signal that only recalculates when its upstream dependencies change:

```rust
#[component]
pub fn TransactionSummary(transactions: Signal<Vec<Transaction>>) -> View {
    let total_volume = use_memo(move || {
        transactions().iter().map(|tx| tx.amount).sum::<f64>()
    });

    let high_value_count = use_memo(move || {
        transactions().iter().filter(|tx| tx.amount > 10_000.0).count()
    });

    rsx! {
        <div class="grid grid-cols-2 gap-4">
            <MetricCard title="Total Volume" value={format!("${:.2}", total_volume())} />
            <MetricCard title="High Value" value={format!("{}", high_value_count())} />
        </div>
    }
}
```

---

## Side Effects: `use_effect`

`use_effect` runs side effects in response to signal updates, such as timers, network requests, hardware sensor listening, or persistent storage sync:

```rust
#[component]
pub fn AutoSync() -> View {
    let (status, set_status) = use_signal(|| "idle");

    use_effect(move || {
        let timer = set_interval(move || {
            set_status.set("syncing");
            // Perform async synchronization
            spawn(async move {
                let _res = fetch("/api/v1/sync").await;
                set_status.set("synced");
            });
        }, 5000);

        // Optional teardown closure invoked on unmount or before rerun
        move || {
            clear_interval(timer);
        }
    });

    rsx! {
        <div class="text-xs font-mono text-neutral-400">
            SYNC STATUS: {status()}
        </div>
    }
}
```

---

## Atomic State Batching

When modifying multiple signals simultaneously, use `batch` to prevent intermediate renders:

```rust
use strata::reactive::batch;

batch(|| {
    set_user_id.set(42);
    set_auth_token.set(Some("bearer_token_xyz".into()));
    set_logged_in.set(true);
});
// Downstream consumers are notified exactly once with all updates applied.
```
