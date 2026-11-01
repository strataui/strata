# strata-router

Universal, type-safe client-side routing, route pattern scoring, nested layouts, parameterized paths, and reactive navigation hooks for Project Strata.

---

## Architecture: Universal Pure-Rust Routing

The `strata-router` crate provides a pure-Rust routing engine designed for universal operation across desktop GPU windows, WebAssembly single-page applications, and Terminal UI (TUI) dashboards.

Unlike browser-bound routers that rely on JavaScript global objects (`window.history`, `popstate` events), `strata-router` runs on an abstract in-memory state engine connected directly to Strata's reactive signal graph.

```
       +-------------------------------------------------------+
       |               URL / Navigation Input                  |
       |  (Link click / nav.push() / nav.replace() / nav.back())|
       +---------------------------+---------------------------+
                                   |
                                   v
       +-------------------------------------------------------+
       |                    MemoryHistory                      |
       |        (entries: Vec<Location>, index: usize)         |
       +---------------------------+---------------------------+
                                   |
                      Dispatches New Location
                                   v
       +-------------------------------------------------------+
       |                    RouterContext                      |
       |     - location: Signal<Location>                      |
       |     - params: Signal<HashMap<String, String>>         |
       +---------------------------+---------------------------+
                                   |
                      Evaluates Registered Routes
                                   v
       +-------------------------------------------------------+
       |               match_path Scoring Engine               |
       |    - Literal Segment (Score: 100)                     |
       |    - Dynamic Parameter (:param) (Score: 50)           |
       |    - Wildcard Splat (*tail) (Score: 10)               |
       +---------------------------+---------------------------+
                                   |
                         Updates Params Signal
                                   v
       +-------------------------------------------------------+
       |                     render_routes                     |
       |     (Surgically Swaps Active View Hierarchy)          |
       +-------------------------------------------------------+
```

### Architectural Guarantees

1. **Target Independence**: Operates identically in native desktop GPU apps, headless server simulators, terminal consoles, and web browsers.
2. **Reactive Signal Integration**: Location and parameters are stored in reactive `Signal` cells, allowing components to extract params with fine-grained subscription boundaries.
3. **Deterministic Route Precedence**: Trie-less score-based matching ensures that `/clusters/us-east` always wins over `/clusters/:id` without reliance on registration order.
4. **Zero DOM Overhead**: Navigating between routes re-executes only the target view closure without tearing down top-level layout shells.

---

## Path Matching & Route Scoring

The `match_path` function resolves URLs against route patterns, extracts dynamic values, and calculates a deterministic specificity score.

### Segment Scoring Weights

To resolve route conflicts unambiguously, each segment in a pattern contributes to a total specificity score:

- **Literal Segment** (e.g. `clusters`, `telemetry`): **+100 points**
- **Dynamic Parameter** (e.g. `:cluster_id`, `:node_ip`): **+50 points**
- **Wildcard Splat** (e.g. `*filepath`, `*`): **+10 points**

```rust
use strata_router::prelude::*;

// Literal segment wins over dynamic parameter
let m1 = match_path("/clusters/settings", "/clusters/settings").unwrap();
let m2 = match_path("/clusters/:id", "/clusters/settings").unwrap();

assert_eq!(m1.score, 200); // 100 + 100
assert_eq!(m2.score, 150); // 100 + 50
assert!(m1.score > m2.score);
```

### Dynamic Parameters and Wildcard Splats

```rust
use strata_router::prelude::*;

// Extracting multiple dynamic parameters
let m = match_path(
    "/clusters/:cluster_id/pods/:pod_name/logs",
    "/clusters/production-omega/pods/auth-service-7b9d/logs"
).expect("Route must match");

assert_eq!(m.params.get("cluster_id").unwrap(), "production-omega");
assert_eq!(m.params.get("pod_name").unwrap(), "auth-service-7b9d");

// Wildcard tail matching for hierarchical telemetry paths
let wildcard = match_path(
    "/metrics/explore/*metric_path",
    "/metrics/explore/cluster/network/eth0/rx_bytes"
).expect("Wildcard route must match");

assert_eq!(
    wildcard.params.get("metric_path").unwrap(),
    "cluster/network/eth0/rx_bytes"
);
```

### Query String & Hash Parsing

The `Location::parse` method decomposes complete URL strings into normalized components:

```rust
use strata_router::prelude::*;

let loc = Location::parse("/observability/traces?service=billing&limit=50#flamegraph");

assert_eq!(loc.pathname, "/observability/traces");
assert_eq!(loc.search, "?service=billing&limit=50");
assert_eq!(loc.hash, "#flamegraph");
assert_eq!(loc.query.get("service").unwrap(), "billing");
assert_eq!(loc.query.get("limit").unwrap(), "50");
```

---

## Universal History Engine: `MemoryHistory`

The `MemoryHistory` structure manages an in-memory navigation stack with forward-truncation semantics:

```rust
use strata_router::history::{HistoryAction, MemoryHistory};

let mut history = MemoryHistory::new("/overview");
assert_eq!(history.current().pathname, "/overview");

// Push a new entry
let (loc, action) = history.push("/nodes");
assert_eq!(loc.pathname, "/nodes");
assert_eq!(action, HistoryAction::Push);

history.push("/nodes/worker-01");

// Navigate backward
let (prev, action) = history.back().unwrap();
assert_eq!(prev.pathname, "/nodes");
assert_eq!(action, HistoryAction::Pop);

// Push from middle truncates forward history (standard browser-spec behavior)
history.push("/alerts");
assert!(history.forward().is_none());
```

---

## Reactive Context and Hooks

The router coordinator is exposed through thread-local hooks defined in `context.rs`.

### Initializing the Router Context

At the root of the application, initialize the router with the default entry path:

```rust
use strata_router::prelude::*;

let router = init_router("/overview");
```

### Navigation Hooks

- **`use_location() -> Signal<Location>`**: Returns the reactive signal tracking current URL, pathname, query parameters, and hash.
- **`use_params() -> Signal<HashMap<String, String>>`**: Returns the reactive dictionary of route parameters extracted from the active route pattern.
- **`use_navigate() -> Navigator`**: Provides imperative programmatic navigation controls (`push`, `replace`, `back`, `forward`).
- **`use_router() -> RouterContext`**: Returns the underlying thread-local router coordinator.

```rust
use strata_router::prelude::*;

#[component]
pub fn NavigationBar() -> View {
    let loc = use_location();
    let nav = use_navigate();

    rsx! {
        <nav class="flex items-center gap-4 px-6 py-3 bg-neutral-900 border-b border-neutral-800">
            <span class="text-xs font-mono text-neutral-400">
                {"PATH: "} {loc.get().pathname}
            </span>

            <button
                class="px-3 py-1 bg-neutral-800 hover:bg-neutral-700 text-neutral-200 text-xs rounded"
                on_click={move |_| { nav.push("/clusters"); }}
            >
                {"Clusters"}
            </button>

            <button
                class="px-3 py-1 bg-neutral-800 hover:bg-neutral-700 text-neutral-200 text-xs rounded"
                on_click={move |_| { nav.back(); }}
            >
                {"Back"}
            </button>
        </nav>
    }
}
```

---

## Declarative Component Layer

The component module in `components.rs` provides declarative primitives for rendering views and triggering route transitions.

### `<Link>`

The `Link` component intercepts click events and initiates push navigation:

```rust
use strata_router::prelude::*;

rsx! {
    <Link
        to="/clusters/production"
        class="text-blue-400 hover:text-blue-300 font-medium text-sm underline"
    >
        {"View Production Cluster"}
    </Link>
}
```

### `<Navigate>`

The `Navigate` component triggers immediate redirection when rendered:

```rust
use strata_router::prelude::*;

// Perform replacement redirect on unauthorized access
rsx! {
    <Navigate to="/auth/login" replace={true} />
}
```

### Route Table: `render_routes`

The `render_routes` function evaluates a list of `RouteEntry` definitions against the current URL, automatically registering routes in the router coordinator:

```rust
use strata_router::prelude::*;
use strata_dom::prelude::*;
use strata_macro::rsx;

pub fn AppRouter() -> View {
    let routes = vec![
        RouteEntry::new("/", || rsx! { <DashboardOverview /> }),
        RouteEntry::new("/clusters", || rsx! { <ClusterList /> }),
        RouteEntry::new("/clusters/:cluster_id", || rsx! { <ClusterDetail /> }),
        RouteEntry::new("/clusters/:cluster_id/nodes/:node_id", || rsx! { <NodeDetail /> }),
        RouteEntry::new("/settings", || rsx! { <SystemSettings /> }),
    ];

    render_routes(&routes)
}
```

If no registered route matches, `render_routes` displays a built-in `404 Not Found` diagnostic view showing the unmatched path.

---

## Navigation Guards and Middleware Patterns

Authentication and Role-Based Access Control (RBAC) are implemented cleanly using functional route wrappers:

```rust
use strata_router::prelude::*;
use strata_dom::prelude::*;
use strata_macro::rsx;

pub fn require_admin<F: Fn() -> View + 'static>(is_admin: bool, protected_view: F) -> View {
    if is_admin {
        protected_view()
    } else {
        rsx! {
            <div class="p-8 text-center bg-red-950/20 border border-red-900 rounded-xl">
                <h3 class="text-red-400 font-bold text-lg">{"403 Forbidden"}</h3>
                <p class="text-neutral-400 text-sm mt-1">
                    {"Administrative privileges required to view cluster node configurations."}
                </p>
                <div class="mt-4">
                    <Link to="/" class="text-xs text-neutral-300 underline">
                        {"Return to Overview"}
                    </Link>
                </div>
            </div>
        }
    }
}
```

---

## Production Cloud Console Example

Below is a complete enterprise routing architecture for a multi-tenant cloud monitoring dashboard:

```rust
use strata_dom::prelude::*;
use strata_macro::{component, rsx};
use strata_router::prelude::*;

#[component]
pub fn CloudConsoleRoot() -> View {
    let _router = init_router("/clusters/us-east-prod/nodes/node-88");

    rsx! {
        <div class="flex h-screen w-screen bg-neutral-950 text-neutral-100 font-sans">
            <!-- Sidebar Layout -->
            <aside class="w-64 border-r border-neutral-800 p-4 flex flex-col gap-2">
                <div class="px-3 py-2 text-xs font-mono font-bold text-neutral-500 uppercase tracking-wider">
                    {"Cloud Infrastructure"}
                </div>
                <Link to="/overview" class="px-3 py-2 text-sm text-neutral-300 hover:bg-neutral-900 rounded-lg">
                    {"Overview"}
                </Link>
                <Link to="/clusters" class="px-3 py-2 text-sm text-neutral-300 hover:bg-neutral-900 rounded-lg">
                    {"Clusters"}
                </Link>
                <Link to="/metrics" class="px-3 py-2 text-sm text-neutral-300 hover:bg-neutral-900 rounded-lg">
                    {"Global Metrics"}
                </Link>
            </aside>

            <!-- Main Route Outlet -->
            <main class="flex-1 overflow-auto p-8">
                {AppRouter()}
            </main>
        </div>
    }
}

#[component]
pub fn NodeDetail() -> View {
    let params = use_params();
    let loc = use_location();

    let cluster_id = params.get().get("cluster_id").cloned().unwrap_or_default();
    let node_id = params.get().get("node_id").cloned().unwrap_or_default();

    rsx! {
        <div class="flex flex-col gap-6">
            <div>
                <span class="text-xs font-mono text-cyan-400 uppercase tracking-wider">
                    {"CLUSTER: "}{cluster_id}
                </span>
                <h1 class="text-2xl font-bold font-mono text-neutral-100 mt-1">
                    {"Node "}{node_id}
                </h1>
                <p class="text-xs font-mono text-neutral-500 mt-1">
                    {"Full URL: "}{loc.get().full_url()}
                </p>
            </div>

            <div class="grid grid-cols-2 gap-4">
                <div class="p-4 bg-neutral-900 border border-neutral-800 rounded-xl">
                    <span class="text-xs font-mono text-neutral-400">{"POD DENSITY"}</span>
                    <div class="text-xl font-bold text-neutral-200 mt-1">{"48 / 64 Pods"}</div>
                </div>
                <div class="p-4 bg-neutral-900 border border-neutral-800 rounded-xl">
                    <span class="text-xs font-mono text-neutral-400">{"NETWORK EGRESS"}</span>
                    <div class="text-xl font-bold text-emerald-400 mt-1">{"1.84 GB/s"}</div>
                </div>
            </div>
        </div>
    }
}
```
