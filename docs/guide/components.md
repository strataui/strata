# Components & RSX Syntax

Strata applications are assembled from modular, declarative components using the `rsx!` macro. The syntax mirrors standard JSX/TSX while leveraging the strict type-safety, ownership model, and compile-time guarantees of Rust.

---

## Declaring a Component

Components in Strata are annotated with the `#[component]` procedural macro. A component function accepts typed arguments (props) and returns a `View`:

```rust
use strata::prelude::*;

#[component]
pub fn StatusBadge(label: &'static str, active: bool) -> View {
    rsx! {
        <div class="inline-flex items-center gap-2 px-3 py-1 rounded-full text-xs font-mono font-medium border"
             class={if active {
                 "bg-emerald-950/40 text-emerald-400 border-emerald-800"
             } else {
                 "bg-neutral-900 text-neutral-400 border-neutral-800"
             }}>
            <span class="w-1.5 h-1.5 rounded-full"
                  class={if active { "bg-emerald-400 animate-pulse" } else { "bg-neutral-600" }} />
            <span>{label}</span>
        </div>
    }
}
```

---

## Props and Optional Attributes

Props are defined directly as typed function parameters. Optional properties use Rust's standard `Option<T>`:

```rust
#[component]
pub fn ActionButton(
    title: String,
    variant: Option<&'static str>,
    disabled: Option<bool>,
    on_click: EventHandler<MouseEvent>,
) -> View {
    let variant = variant.unwrap_or("primary");
    let disabled = disabled.unwrap_or(false);

    let base_style = "px-4 py-2 rounded-lg font-medium text-sm transition-all duration-150 cursor-pointer";
    let variant_style = match variant {
        "primary" => "bg-indigo-600 hover:bg-indigo-500 text-white shadow-lg shadow-indigo-500/20",
        "secondary" => "bg-neutral-800 hover:bg-neutral-700 text-neutral-200 border border-neutral-700",
        _ => "bg-transparent text-neutral-400 hover:text-white",
    };

    rsx! {
        <button
            class="{base_style} {variant_style}"
            disabled={disabled}
            onclick={move |e| on_click.call(e)}
        >
            {title}
        </button>
    }
}
```

---

## Children and Content Projection

To project nested child nodes into a component, accept a `children: Children` parameter:

```rust
#[component]
pub fn GlassCard(title: &'static str, children: Children) -> View {
    rsx! {
        <div class="bg-neutral-900/60 backdrop-blur-xl border border-neutral-800 rounded-xl p-6 shadow-2xl">
            <header class="flex items-center justify-between pb-4 mb-4 border-b border-neutral-800/80">
                <h3 class="text-sm font-semibold tracking-wide text-neutral-300 font-mono uppercase">
                    {title}
                </h3>
            </header>
            <div class="card-content">
                {children}
            </div>
        </div>
    }
}
```

Consuming the card with nested elements:

```rust
rsx! {
    <GlassCard title="Node Telemetry">
        <p class="text-neutral-400 text-sm">Cluster load is operating within nominal parameters.</p>
        <div class="mt-4 flex gap-2">
            <Badge label="CPU: 12%" />
            <Badge label="RAM: 4.2 GB" />
        </div>
    </GlassCard>
}
```

---

## Conditional Rendering

Render elements conditionally using idiomatic Rust expressions:

```rust
#[component]
pub fn NotificationBanner(error: Option<String>) -> View {
    rsx! {
        <div class="w-full">
            {if let Some(err_msg) = error {
                rsx! {
                    <div class="p-4 bg-rose-950/60 border border-rose-800 rounded-lg text-rose-300 text-sm flex items-center gap-3">
                        <span class="font-bold font-mono">[!]</span>
                        <span>{err_msg}</span>
                    </div>
                }
            } else {
                rsx! {
                    <div class="p-4 bg-emerald-950/30 border border-emerald-800/60 rounded-lg text-emerald-400 text-sm">
                        All operations completed successfully.
                    </div>
                }
            }}
        </div>
    }
}
```

---

## Iterating Over Collections

Render lists of items using Rust's `.iter().map(...)`:

```rust
#[component]
pub fn ProcessTable(processes: Vec<ProcessInfo>) -> View {
    rsx! {
        <div class="w-full border border-neutral-800 rounded-lg overflow-hidden bg-neutral-900/40">
            <table class="w-full text-left text-sm font-mono">
                <thead class="bg-neutral-800/60 text-neutral-400 text-xs">
                    <tr>
                        <th class="p-3">PID</th>
                        <th class="p-3">Process</th>
                        <th class="p-3">Memory</th>
                        <th class="p-3">Status</th>
                    </tr>
                </thead>
                <tbody class="divide-y divide-neutral-800/50">
                    {processes.into_iter().map(|proc| rsx! {
                        <tr key={proc.pid} class="hover:bg-neutral-800/30 transition-colors">
                            <td class="p-3 text-neutral-500">{proc.pid}</td>
                            <td class="p-3 text-white font-medium">{proc.name}</td>
                            <td class="p-3 text-neutral-300">{proc.memory_mb} MB</td>
                            <td class="p-3 text-emerald-400">{proc.status}</td>
                        </tr>
                    })}
                </tbody>
            </table>
        </div>
    }
}
```

---

## Compile-Time Safety

Because Strata components compile down directly to Rust abstract syntax trees:
- Typos in HTML attributes trigger compiler errors.
- Unused variables or dangling signal references are caught by `rustc`.
- Types are strictly checked without runtime overhead or hydration mismatches.
