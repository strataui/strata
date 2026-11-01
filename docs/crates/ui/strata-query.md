# strata_query: Asynchronous Server-State & Mutation Engine

The `strata_query` crate provides an asynchronous query caching and mutation lifecycle engine for the Strata framework. Designed as a pure-Rust equivalent to TanStack Query (React Query) and SWR, `strata_query` automates in-flight request deduplication, Stale-While-Revalidate background caching, retry backoff algorithms, declarative query invalidation, and transactional mutation pipelines without external JavaScript runtimes.

---

## Architectural Principles

Server state in Strata is treated as fundamentally distinct from local client UI state:

```text
┌─────────────────────────────────────────────────────────────┐
│                    Component Render Tree                    │
│      use_query(&client, "metrics:qps", fetcher, opts)       │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                     QueryClient Cache                       │
│  (Keyed Entries • Timestamp • Expiration • In-Flight Dedupe)│
└──────────────────────────────┬──────────────────────────────┘
                               │
            ┌──────────────────┴──────────────────┐
     (Fresh Cache Hit)                     (Stale / Missing)
            ▼                                     ▼
┌─────────────────────────────┐       ┌─────────────────────────────┐
│   Instant Return from RAM   │       │   Execute Async Fetcher     │
│   (Zero network latency)    │       │   (Update cache & signals)  │
└─────────────────────────────┘       └─────────────────────────────┘
                                                      │
                                                      ▼
                                      ┌─────────────────────────────┐
                                      │   Notify Dependent Nodes    │
                                      │   (data, status, is_loading)│
                                      └─────────────────────────────┘
```

1. **Request Deduplication**: If five distinct components subscribe to `"cluster:health:us-east-01"` simultaneously during mount, only one network request is dispatched. All five components share the resolved cache promise.
2. **Stale-While-Revalidate**: Cached data is presented instantly upon view navigation, while a non-blocking background fetch verifies freshness against the remote telemetry API.
3. **Optimistic Updates & Cache Invalidation**: Mutations trigger immediate local signal adjustments, automatically invalidating stale query keys upon remote acknowledgment.

---

## Core Types and Data Models

### The Cache Coordinator: `QueryClient`

Maintains the thread-safe in-memory cache and coordinates query invalidations:

```rust
use strata_query::prelude::QueryClient;

// Instantiate global or scoped query client
let client = QueryClient::new();

// Invalidate specific cache key or prefix
client.invalidate_queries("cluster:nodes");

// Evict all active query cache entries
client.clear();
```

---

### Query Configuration: `QueryOptions`

Tunes caching lifetimes and retry tolerance:

```rust
use std::time::Duration;
use strata_query::prelude::QueryOptions;

let options = QueryOptions {
    stale_time: Duration::from_secs(60), // Data considered fresh for 60 seconds
    retry_count: 3,                      // Attempt 3 retries with backoff on failure
};
```

| Field | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `stale_time` | `Duration` | `Duration::from_secs(30)` | Duration during which cached results are returned without background revalidation. |
| `retry_count` | `usize` | `2` | Number of automated retry attempts before emitting an error signal. |

---

### Query Lifecycle Status: `QueryStatus`

| Variant | Description |
| :--- | :--- |
| `QueryStatus::Pending` | Initial state or active network request in flight with no existing cached data. |
| `QueryStatus::Success` | Fetcher successfully resolved and returned valid data. |
| `QueryStatus::Error` | Fetcher returned an error or exceeded retry tolerance. |

---

## The `use_query` Declarative Hook

Binds an asynchronous remote fetch operation to fine-grained reactive signals:

```rust
use std::time::Duration;
use strata_query::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub struct ClusterMetrics {
    pub current_qps: u64,
    pub p99_latency_ms: f32,
    pub active_nodes: usize,
}

let client = QueryClient::new();

let query = use_query::<ClusterMetrics, String, _>(
    &client,
    "telemetry:cluster_metrics",
    || {
        // Asynchronous or simulated remote gRPC/HTTP fetch
        Ok(ClusterMetrics {
            current_qps: 18450,
            p99_latency_ms: 3.82,
            active_nodes: 48,
        })
    },
    QueryOptions {
        stale_time: Duration::from_secs(15),
        retry_count: 2,
    },
);

// Inspect reactive state
assert_eq!(query.status.get(), QueryStatus::Success);
assert_eq!(query.data.get().unwrap().current_qps, 18450);
assert!(!query.is_loading.get());

// Trigger an explicit refetch
query.refetch();
```

#### API Specification: `QueryResult<T, E>`

| Member | Type | Description |
| :--- | :--- | :--- |
| `data` | `Signal<Option<T>>` | Resolved query payload, or `None` if pending/error. |
| `error` | `Signal<Option<E>>` | Error payload emitted by fetcher upon failure. |
| `status` | `Signal<QueryStatus>` | High-level lifecycle enum (`Pending`, `Success`, `Error`). |
| `is_loading` | `Signal<bool>` | `true` if query is in its initial loading state with no cached data. |
| `is_fetching` | `Signal<bool>` | `true` whenever a network request is actively executing (including background revalidation). |
| `refetch()` | `fn()` | Forces immediate re-execution of the fetcher, ignoring `stale_time`. |

---

## Mutation Pipelines: `use_mutation`

Manages asynchronous state modifications (such as scaling clusters, restarting pods, or mutating firewall rules) with loading and error boundaries:

```rust
use strata_query::prelude::*;

// Define mutation accepting node count variable
let scale_cluster_mutation = use_mutation(|target_nodes: usize| {
    if target_nodes == 0 {
        Err("Cannot scale cluster down to 0 nodes")
    } else {
        Ok(format!("Successfully scaled to {} nodes", target_nodes))
    }
});

assert!(!scale_cluster_mutation.is_pending.get());
assert!(!scale_cluster_mutation.is_success.get());

// Execute mutation with variable payload
scale_cluster_mutation.mutate(64);

// Verify success state
assert!(scale_cluster_mutation.is_success.get());
assert_eq!(
    scale_cluster_mutation.data.get(),
    Some("Successfully scaled to 64 nodes".to_string())
);
assert_eq!(scale_cluster_mutation.error.get(), None);

// Execute invalid mutation
scale_cluster_mutation.mutate(0);
assert!(!scale_cluster_mutation.is_success.get());
assert_eq!(
    scale_cluster_mutation.error.get(),
    Some("Cannot scale cluster down to 0 nodes")
);
```

#### API Specification: `MutationResult<TData, TError, TVariables>`

| Member | Type | Description |
| :--- | :--- | :--- |
| `data` | `Signal<Option<TData>>` | Result returned by successful mutation execution. |
| `error` | `Signal<Option<TError>>` | Error returned by failed mutation execution. |
| `is_pending` | `Signal<bool>` | `true` while mutation closure is actively executing. |
| `is_success` | `Signal<bool>` | `true` once mutation completes successfully. |
| `mutate(variables)` | `fn(TVariables)` | Dispatches mutation execution with input arguments. |

---

## Production Component Example: Observable Cluster Health Widget

```rust
use std::time::Duration;
use strata_dom::prelude::*;
use strata_icons::prelude::IconName;
use strata_query::prelude::*;
use strata_ui::prelude::Icon;

#[component]
pub fn ClusterTelemetryPanel(client: QueryClient) -> View {
    let query = use_query::<u64, String, _>(
        &client,
        "metrics:active_qps",
        || Ok(24_890), // Simulated gRPC telemetry poll
        QueryOptions {
            stale_time: Duration::from_secs(5),
            retry_count: 3,
        },
    );

    let restart_mutation = use_mutation(|node_id: &'static str| {
        // Simulated asynchronous cluster restart command
        Ok(format!("Ingress node {} restarted", node_id))
    });

    let is_restarting = restart_mutation.is_pending.get();

    rsx! {
        <div class="flex flex-col gap-4 p-5 bg-neutral-950 border border-neutral-800 rounded-xl shadow-2xl font-mono text-xs">
            <header class="flex items-center justify-between pb-3 border-b border-neutral-800">
                <span class="font-bold text-neutral-300 uppercase tracking-wide">
                    "Live Ingress Telemetry"
                </span>
                <button
                    class="p-1.5 bg-neutral-900 border border-neutral-800 rounded hover:bg-neutral-800 text-neutral-400 hover:text-white transition-colors"
                    onclick={move |_| query.refetch()}
                    title="Force Cache Refetch"
                >
                    {Icon(IconName::RefreshCw, Some("w-3.5 h-3.5"))}
                </button>
            </header>

            // Query state handling
            {match query.status.get() {
                QueryStatus::Pending => rsx! {
                    <div class="py-8 text-center text-neutral-500">
                        "Connecting to telemetry stream..."
                    </div>
                },
                QueryStatus::Error => rsx! {
                    <div class="py-4 text-rose-400 bg-rose-950/20 border border-rose-800 rounded p-3">
                        "Failed to synchronize metrics."
                    </div>
                },
                QueryStatus::Success => rsx! {
                    <div class="flex items-baseline gap-2">
                        <span class="text-3xl font-extrabold text-sky-400">
                            {format!("{} req/s", query.data.get().unwrap_or(0))}
                        </span>
                        <span class="text-emerald-400 font-semibold">
                            "• Ingress Nominal"
                        </span>
                    </div>
                },
            }}

            // Trigger mutation
            <div class="pt-2 border-t border-neutral-800 flex items-center justify-between">
                <span class="text-neutral-500">"Control Plane:"</span>
                <button
                    class="px-3 py-1.5 bg-neutral-900 hover:bg-neutral-800 border border-neutral-700 rounded text-neutral-300 font-sans font-semibold disabled:opacity-40"
                    disabled={is_restarting}
                    onclick={move |_| restart_mutation.mutate("ingress-proxy-01")}
                >
                    {if is_restarting { "Restarting..." } else { "Restart Ingress Proxy" }}
                </button>
            </div>
        </div>
    }
}
```
