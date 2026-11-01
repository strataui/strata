# Crate `strata_macro`: Declarative Macro Compiler & Syntax Expansion

The `strata_macro` crate provides procedural macros for Project Strata. It includes a zero-dependency JSX/HTML-in-Rust compiler (`rsx!`), a component builder generator (`#[component]`), and reactive state derive macros (`#[derive(Store)]`, `#[derive(ReactiveStore)]`).

Unlike typical procedural macro crates that depend heavily on third-party parsing crates like `syn` or `quote`, `strata_macro` features a custom, hand-written recursive-descent token parser operating directly on `proc_macro::TokenStream`. This eliminates dependency overhead and optimizes compile times.

---

## Architectural Pipeline

```text
┌────────────────────────────────────────────────────────┐
│                   Macro Invocation                     │
│    rsx! { <div class="flex"><span>CPU</span></div> }   │
└───────────────────────────┬────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────┐
│             TokenCursor (cursor.rs)                    │
│    TokenTree stream traversal, lookaheads, delimiters  │
└───────────────────────────┬────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────┐
│             Recursive-Descent Parser (parser.rs)       │
│    Emits RsxNode AST: Elements, Components, Text, Expr │
└───────────────────────────┬────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────┐
│             Codegen Engine (codegen.rs)                │
│    Lowers AST into strata_dom::NodeBlueprint & View    │
└────────────────────────────────────────────────────────┘
```

---

## The `rsx!` Macro

The `rsx!` procedural macro compiles JSX/HTML syntax directly into strongly typed `strata_dom::NodeBlueprint` structures.

### Supported Syntax Constructs

1. **HTML Elements**: Standard lowercase tags (`<div>`, `<span>`, `<button>`, `<input>`, `<main>`, `<svg>`, `<canvas>`, etc.).
2. **Custom Components**: Capitalized identifiers (`<ClusterMetricCard />`, `<PodLogViewer />`) and namespaced paths (`<Icons::Server />`).
3. **Fragments**: Empty tags `<> ... </>` compiling to `NodeBlueprint::fragment(...)`.
4. **Self-Closing Tags**: `<input />`, `<hr />`, `<img />`.
5. **Dynamic Rust Expressions**: Wrapped inside braces `{ ... }`.
6. **Attributes & Event Listeners**:
   - String literals: `id="telemetry-panel"`.
   - Dynamic expressions: `class={format!("status-{}", status)}`.
   - Event callbacks: `onclick={move || set_count.update(|c| *c += 1)}`.
   - Two-way input bindings: `oninput={move |val: String| set_name.set(val)}`.

### Internal AST Representation (`ast.rs`)

```rust
pub enum RsxNode {
    Element(RsxElement),
    Component(RsxComponent),
    Text(String),
    Expression(TokenStream),
    Fragment(Vec<RsxNode>),
}

pub struct RsxElement {
    pub tag: String,
    pub attributes: Vec<RsxAttribute>,
    pub children: Vec<RsxNode>,
    pub self_closing: bool,
}

pub struct RsxComponent {
    pub path: Vec<String>,
    pub props: Vec<RsxAttribute>,
    pub children: Vec<RsxNode>,
    pub self_closing: bool,
}
```

### Compile-Time Tailwind Tokenization
When an element defines `class="..."` or `className="..."`, the codegen lowering pass splits the literal string into individual class tokens at compile time:

```rust
// Given:
rsx! { <div class="flex items-center gap-4 bg-neutral-900" /> }

// Codegen lowers to:
strata_dom::NodeBlueprint::element(
    strata_dom::Tag::Div,
    vec![
        "flex".to_string(),
        "items-center".to_string(),
        "gap-4".to_string(),
        "bg-neutral-900".to_string(),
    ],
    vec![],
    vec![],
)
```

Dynamic expressions within class attributes (`class={computed_classes}`) are lowered into an immediate whitespace splitting iterator:

```rust
({ let _s = (computed_classes).to_string(); _s.split_whitespace().map(|c| c.to_string()).collect::<Vec<String>>() })
```

---

## The `#[component]` Attribute Macro

The `#[component]` macro transforms a Rust function returning a `View` into a full-featured Strata component supporting JSX-style named prop invocation.

### Dual Value & Type Namespace Pattern

Rust maintains separate namespaces for values (functions) and types (structs). `#[component]` takes advantage of this by generating:
1. **In the Value Namespace**: The original function `pub fn ClusterNode(props...) -> View`.
2. **In the Type Namespace**: A companion builder struct `pub struct ClusterNodeBuilder`.

```text
                  #[component]
         pub fn ServerRack(id: String, active: bool) -> View
                               │
               ┌───────────────┴───────────────┐
               ▼                               ▼
    [Value Namespace]                  [Type Namespace]
  pub fn ServerRack(...)             pub struct ServerRackBuilder {
  (Preserved for direct calls)           pub id: Option<String>,
                                         pub active: Option<bool>,
                                     }
                                     impl ServerRackBuilder {
                                         pub fn id(mut self, ...) -> Self;
                                         pub fn active(mut self, ...) -> Self;
                                         pub fn build(self) -> View;
                                     }
```

This pattern allows the `rsx!` macro to expand `<ServerRack id="rack-01" active=true />` into:

```rust
ServerRackBuilder::new()
    .id("rack-01".into())
    .active(true)
    .build()
```

### Component Parameters & Children Handling

Components can accept named properties, optional parameters, children, and event callbacks:

```rust
use strata::prelude::*;

#[component]
pub fn MetricCard(
    title: String,
    value: f64,
    unit: Option<String>,
    on_refresh: Option<std::rc::Rc<dyn Fn()>>,
    children: Option<Vec<NodeBlueprint>>,
) -> View {
    let unit_str = unit.unwrap_or_else(|| "ms".to_string());

    rsx! {
        <div class="flex flex-col p-4 bg-neutral-900 border border-neutral-800 rounded">
            <div class="flex items-center justify-between">
                <span class="text-xs text-neutral-400 font-mono">{title}</span>
                {if let Some(refresh) = on_refresh {
                    rsx! {
                        <button class="text-xs text-cyan-400" onclick={move || refresh()}>
                            Refresh
                        </button>
                    }
                } else {
                    rsx! { <></> }
                }}
            </div>
            <div class="flex items-baseline gap-1 my-2">
                <span class="text-2xl font-bold font-mono text-white">{format!("{:.2}", value)}</span>
                <span class="text-xs text-neutral-500 font-mono">{unit_str}</span>
            </div>
            {if let Some(kids) = children {
                rsx! { <div class="mt-2 pt-2 border-t border-neutral-800">{kids}</div> }
            } else {
                rsx! { <></> }
            }}
        </div>
    }
}
```

---

## Reactive Store Derivation (`#[derive(Store)]`)

The `#[derive(Store)]` (or alias `#[derive(ReactiveStore)]`) macro provides fine-grained, atomized reactive state models for complex data structures.

### Generated Companion Structure

Applying `#[derive(Store)]` to a plain data struct:

```rust
#[derive(Clone, Default, Store)]
pub struct ClusterNodeTelemetry {
    pub node_id: String,
    pub cpu_cores: u32,
    pub memory_allocated_bytes: u64,
    pub is_cordoned: bool,
}
```

Automatically synthesizes a companion `ClusterNodeTelemetryStore` struct where every field becomes an independent `Signal<T>`:

```rust
pub struct ClusterNodeTelemetryStore {
    pub node_id: strata_dom::Signal<String>,
    pub cpu_cores: strata_dom::Signal<u32>,
    pub memory_allocated_bytes: strata_dom::Signal<u64>,
    pub is_cordoned: strata_dom::Signal<bool>,
}
```

### Store Capabilities

```rust
impl ClusterNodeTelemetryStore {
    /// Initializes atomic signals from an existing instance.
    pub fn new(initial: ClusterNodeTelemetry) -> Self;

    /// Evaluates all field signals, returning a full snapshot.
    pub fn get(&self) -> ClusterNodeTelemetry;

    /// Atomically updates all fields inside a batch block.
    pub fn set(&self, next: ClusterNodeTelemetry);

    /// Executes multiple field mutations within a single reactive batch.
    pub fn batch<R>(&self, f: impl FnOnce(&Self) -> R) -> R;

    /// Creates an untracked snapshot without registering reactive dependencies.
    pub fn snapshot(&self) -> ClusterNodeTelemetry;

    /// Restores store state from a snapshot within a single reactive batch.
    pub fn restore(&self, snapshot: &ClusterNodeTelemetry);
}
```

### Fine-Grained Reactive Isolation
When a component accesses `store.cpu_cores.get()`, only that individual field registers a reactive dependency. Updating `memory_allocated_bytes` will not re-evaluate or dirty the component tracking CPU cores.

---

## The `dynamic!` Macro

The `dynamic!` macro provides dynamic JavaScript-compatible object and array construction for interacting with the `strata_js` engine:

```rust
use strata::prelude::*;

let payload = dynamic!({
    "service": "telemetry-gateway",
    "metrics": [
        { "name": "request_latency", "p99": 14.2 },
        { "name": "error_rate", "value": 0.0001 }
    ],
    "region": "us-west-2"
});
```

---

## Production Cloud Infrastructure Example: Tracing Span Explorer

The following example implements a distributed tracing span explorer leveraging `#[component]`, `rsx!`, and `#[derive(Store)]`:

```rust
use strata::prelude::*;

#[derive(Clone, Debug, Default, Store)]
pub struct TracingFilterState {
    pub service_name: String,
    pub min_latency_ms: f64,
    pub show_errors_only: bool,
}

#[component]
pub fn SpanRow(
    span_id: String,
    service: String,
    duration_ms: f64,
    is_error: bool,
) -> View {
    let status_badge = if is_error {
        "text-rose-400 bg-rose-950/50 border-rose-800"
    } else {
        "text-emerald-400 bg-emerald-950/50 border-emerald-800"
    };

    rsx! {
        <div class="flex items-center justify-between px-3 py-2 text-xs font-mono border-b border-neutral-800 hover:bg-neutral-900/60 transition">
            <div class="flex items-center gap-3">
                <span class={format!("px-1.5 py-0.5 rounded border text-[10px] {}", status_badge)}>
                    {if is_error { "ERR" } else { "OK" }}
                </span>
                <span class="text-neutral-400">{span_id}</span>
                <span class="text-neutral-200 font-semibold">{service}</span>
            </div>
            <div class="flex items-center gap-2">
                <span class="text-neutral-300">{format!("{:.2}ms", duration_ms)}</span>
            </div>
        </div>
    }
}

#[component]
pub fn DistributedTraceViewer() -> View {
    let filter_store = TracingFilterState {
        service_name: "auth-gateway".into(),
        min_latency_ms: 10.0,
        show_errors_only: false,
    }.into_store();

    rsx! {
        <main class="flex flex-col w-full h-full bg-neutral-950 text-neutral-100 p-6 gap-4">
            <header class="flex items-center justify-between pb-3 border-b border-neutral-800">
                <h1 class="text-lg font-bold">Distributed Trace Telemetry</h1>
                <div class="flex items-center gap-4">
                    <label class="flex items-center gap-2 text-xs text-neutral-400">
                        <span>Service:</span>
                        <input
                            class="px-2 py-1 bg-neutral-900 border border-neutral-700 rounded text-neutral-200 font-mono text-xs focus:outline-none focus:border-cyan-500"
                            value={filter_store.service_name.get()}
                            oninput={move |val: String| filter_store.service_name.set(val)}
                        />
                    </label>
                    <label class="flex items-center gap-2 text-xs text-neutral-400">
                        <span>Errors Only:</span>
                        <input
                            type="checkbox"
                            checked={filter_store.show_errors_only.get()}
                            onclick={move || filter_store.show_errors_only.update(|e| *e = !*e)}
                        />
                    </label>
                </div>
            </header>

            <section class="flex flex-col bg-neutral-900/40 border border-neutral-800 rounded-lg overflow-hidden">
                <div class="flex items-center justify-between px-3 py-2 bg-neutral-900 border-b border-neutral-800 text-[11px] font-mono text-neutral-400 uppercase tracking-wider">
                    <span>Trace Identity & Service</span>
                    <span>Duration</span>
                </div>
                <SpanRow span_id={"span-9481a".into()} service={"auth-gateway".into()} duration_ms={14.8} is_error=false />
                <SpanRow span_id={"span-9481b".into()} service={"billing-worker".into()} duration_ms={142.3} is_error=true />
                <SpanRow span_id={"span-9481c".into()} service={"cache-cluster".into()} duration_ms={2.1} is_error=false />
            </section>
        </main>
    }
}
```
