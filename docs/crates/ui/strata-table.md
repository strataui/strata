# strata_table: Headless Data Grid & Table Engine

The `strata_table` crate delivers a high-throughput, headless data grid state machine for the Strata engine. Serving as a zero-DOM, pure-Rust equivalent to TanStack Table (React Table) and AG Grid, `strata_table` manages multi-column sorting, case-insensitive substring filtering, dynamic pagination windowing, horizontal column pinning, and row selection states through fine-grained reactive signals.

---

## Architectural Principles

The data table engine decouples state management from layout presentation:

```text
┌─────────────────────────────────────────────────────────────┐
│                      Table<T> State                         │
│  (Data Signal, Sorting Signal, Filter Signal, Select Signal)│
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                    Filtering Pipeline                       │
│    (Case-insensitive substring search across column accessors)│
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                     Sorting Pipeline                        │
│   (Tri-state Asc -> Desc -> None ordering via accessors)    │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                   Pagination Windowing                      │
│     (page_index * page_size slice extraction: O(1) subset)  │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│              Render Blueprint / Virtualizer                 │
│          (<Table>, <TableRow>, <TableCell>, etc.)           │
└─────────────────────────────────────────────────────────────┘
```

1. **Signal-Driven Reactivity**: Modifying a filter, sort criteria, or page index recomputes only the derived visible row slice without invalidating the broader component tree.
2. **Zero In-Memory Duplication**: Rows are accessed via immutable borrowed closures `Rc<dyn Fn(&T) -> String>`, eliminating unnecessary clone operations during filter and sort stages.
3. **Composable Virtualization**: Seamlessly pipes paginated or unpaginated row slices into `strata_virtual` for smooth 120 FPS rendering of 1,000,000+ cluster telemetry events.

---

## Core Types and Data Models

### Column Definition: `ColumnDef<T>`

Configures column metadata, data extraction accessors, sorting rules, and horizontal pinning:

```rust
use strata_table::prelude::{ColumnDef, ColumnPin};

#[derive(Clone, Debug)]
pub struct MicroserviceInstance {
    pub id: &'static str,
    pub service_name: &'static str,
    pub cluster_region: &'static str,
    pub p99_latency_ms: f32,
    pub error_rate_percent: f32,
    pub healthy: bool,
}

// Construct typed columns
let columns = vec![
    ColumnDef::new("id", "Instance ID", |m: &MicroserviceInstance| m.id.to_string())
        .with_pin(ColumnPin::Left), // Sticky left column
    ColumnDef::new("service", "Service Name", |m: &MicroserviceInstance| m.service_name.to_string()),
    ColumnDef::new("region", "Region", |m: &MicroserviceInstance| m.cluster_region.to_string()),
    ColumnDef::new("latency", "P99 Latency", |m: &MicroserviceInstance| format!("{:.2} ms", m.p99_latency_ms)),
    ColumnDef::new("errors", "Error Rate", |m: &MicroserviceInstance| format!("{:.2}%", m.error_rate_percent)),
    ColumnDef::new("status", "Health", |m: &MicroserviceInstance| if m.healthy { "Healthy".into() } else { "Unhealthy".into() }),
];
```

#### API Specification: `ColumnDef<T>`

| Method | Parameters | Return Type | Description |
| :--- | :--- | :--- | :--- |
| `new(id, header, accessor)` | `(impl Into<String>, impl Into<String>, impl Fn(&T) -> String + 'static)` | `Self` | Instantiates a column definition with default sortable and filterable flags enabled. |
| `with_pin(pin)` | `ColumnPin` | `Self` | Sets horizontal pinning mode (`ColumnPin::Left`, `ColumnPin::Right`, `ColumnPin::None`). |
| `sortable(enable)` | `bool` | `Self` | Enables or disables sorting interactions on this column. |
| `filterable(enable)` | `bool` | `Self` | Enables or disables string query filtering on this column. |

---

### Sorting Mechanics: `SortDirection`

Sorting operates through a deterministic 3-state transition cycle:

$$\text{None} \longrightarrow \text{Asc} \longrightarrow \text{Desc} \longrightarrow \text{None}$$

```rust
use strata_table::prelude::SortDirection;

let dir = SortDirection::None;
let next_dir = dir.next();
assert_eq!(next_dir, SortDirection::Asc);

let next_dir = next_dir.next();
assert_eq!(next_dir, SortDirection::Desc);

let reset_dir = next_dir.next();
assert_eq!(reset_dir, SortDirection::None);
```

---

## State Controller: `Table<T>`

The `Table<T>` struct exposes direct reactive signal accessors and mutation commands:

```rust
use strata_table::prelude::{use_table, ColumnDef};

let instances = vec![
    MicroserviceInstance { id: "srv-01", service_name: "auth-gateway", cluster_region: "us-east-1", p99_latency_ms: 3.2, error_rate_percent: 0.01, healthy: true },
    MicroserviceInstance { id: "srv-02", service_name: "telemetry-ingest", cluster_region: "us-west-2", p99_latency_ms: 12.8, error_rate_percent: 0.15, healthy: true },
    MicroserviceInstance { id: "srv-03", service_name: "billing-ledger", cluster_region: "eu-west-1", p99_latency_ms: 45.1, error_rate_percent: 2.40, healthy: false },
];

let table = use_table(instances, columns);

// 1. Pagination controls
table.page_size.set(10);
table.page_index.set(0);
assert_eq!(table.total_pages(), 1);

// 2. Sorting by column
table.toggle_sort("latency"); // Ascending
table.toggle_sort("latency"); // Descending

// 3. Filtering
table.set_filter("service", "telemetry");
assert_eq!(table.rows().len(), 1);
table.set_filter("service", ""); // Clears filter

// 4. Row Selection
table.toggle_row(0);
assert!(table.is_selected(0));
table.select_all();
assert_eq!(table.selected_indices.get().len(), 3);
table.deselect_all();
```

#### API Specification: `Table<T>`

| Member | Type | Description |
| :--- | :--- | :--- |
| `data` | `Signal<Vec<T>>` | Underlying data source vector. |
| `columns` | `Vec<ColumnDef<T>>` | Registered column configuration metadata. |
| `sorting` | `Signal<Option<(String, SortDirection)>>` | Active sorted column ID and sort direction. |
| `filters` | `Signal<Vec<(String, String)>>` | Active list of column filter queries `(column_id, query)`. |
| `page_index` | `Signal<usize>` | Zero-indexed current page. |
| `page_size` | `Signal<usize>` | Items displayed per page slice. |
| `selected_indices` | `Signal<Vec<usize>>` | List of selected row indices. |
| `toggle_sort(col_id)` | `fn(&str)` | Cycles sorting direction for specified column. |
| `set_filter(col_id, query)` | `fn(&str, impl Into<String>)` | Applies string filter and resets `page_index` to 0. |
| `toggle_row(index)` | `fn(usize)` | Toggles selection state of a specific row index. |
| `is_selected(index)` | `fn(usize) -> bool` | Checks if a row index is in the selection set. |
| `select_all()` | `fn()` | Adds all rows to the selection set. |
| `deselect_all()` | `fn()` | Clears the selection set. |
| `next_page()` / `prev_page()` | `fn()` | Bounds-checked pagination navigation. |
| `total_pages()` | `fn() -> usize` | Computes total page count based on filtered rows. |
| `rows()` | `fn() -> Vec<(usize, T)>` | Returns currently visible rows with original indices. |

---

## Presentation Example: Full Observable Telemetry Table

```rust
use strata_dom::prelude::*;
use strata_icons::prelude::IconName;
use strata_table::prelude::*;
use strata_ui::prelude::*;

#[component]
pub fn MicroserviceTelemetryGrid(instances: Vec<MicroserviceInstance>) -> View {
    let columns = vec![
        ColumnDef::new("id", "Instance ID", |m: &MicroserviceInstance| m.id.to_string()),
        ColumnDef::new("service", "Service", |m: &MicroserviceInstance| m.service_name.to_string()),
        ColumnDef::new("region", "Region", |m: &MicroserviceInstance| m.cluster_region.to_string()),
        ColumnDef::new("latency", "P99 Latency", |m: &MicroserviceInstance| format!("{:.1} ms", m.p99_latency_ms)),
        ColumnDef::new("errors", "Error Rate", |m: &MicroserviceInstance| format!("{:.2}%", m.error_rate_percent)),
    ];

    let table = use_table(instances, columns);
    table.page_size.set(25);

    let rows = table.rows();

    rsx! {
        <div class="flex flex-col gap-4 bg-neutral-950 p-6 border border-neutral-800 rounded-xl shadow-2xl">
            // Filter Bar
            <div class="flex items-center justify-between gap-4">
                <input
                    type="text"
                    placeholder="Filter services..."
                    class="bg-neutral-900 border border-neutral-800 rounded-lg px-3 py-1.5 text-xs text-neutral-200 placeholder-neutral-500 font-mono w-64 focus:outline-none focus:border-indigo-500"
                    oninput={move |e| table.set_filter("service", e.value)}
                />

                <div class="flex items-center gap-2">
                    <span class="text-xs font-mono text-neutral-400">
                        {format!("Page {} of {}", table.page_index.get() + 1, table.total_pages())}
                    </span>
                    <button
                        class="px-2 py-1 bg-neutral-900 border border-neutral-800 rounded text-xs text-neutral-300 hover:bg-neutral-800 disabled:opacity-40"
                        onclick={move |_| table.prev_page()}
                    >
                        "Previous"
                    </button>
                    <button
                        class="px-2 py-1 bg-neutral-900 border border-neutral-800 rounded text-xs text-neutral-300 hover:bg-neutral-800 disabled:opacity-40"
                        onclick={move |_| table.next_page()}
                    >
                        "Next"
                    </button>
                </div>
            </div>

            // Data Table Surface
            <div class="border border-neutral-800 rounded-lg overflow-hidden">
                <table class="w-full text-left border-collapse font-mono text-xs">
                    <thead>
                        <tr class="bg-neutral-900/90 border-b border-neutral-800 text-neutral-400">
                            {table.columns.iter().map(|col| {
                                let col_id = col.id.clone();
                                rsx! {
                                    <th
                                        class="px-4 py-3 font-semibold cursor-pointer hover:text-white select-none transition-colors"
                                        onclick={move |_| table.toggle_sort(&col_id)}
                                    >
                                        <div class="flex items-center gap-1.5">
                                            <span>{&col.header}</span>
                                            {if let Some((ref sorted_id, dir)) = table.sorting.get() {
                                                if sorted_id == &col.id {
                                                    match dir {
                                                        SortDirection::Asc => rsx! { <span>"↑"</span> },
                                                        SortDirection::Desc => rsx! { <span>"↓"</span> },
                                                        SortDirection::None => rsx! { <span class="opacity-0">"·"</span> },
                                                    }
                                                } else {
                                                    rsx! { <span class="opacity-0">"·"</span> }
                                                }
                                            } else {
                                                rsx! { <span class="opacity-0">"·"</span> }
                                            }}
                                        </div>
                                    </th>
                                }
                            })}
                        </tr>
                    </thead>
                    <tbody class="divide-y divide-neutral-800/60 bg-neutral-950">
                        {rows.into_iter().map(|(idx, instance)| {
                            let is_sel = table.is_selected(idx);
                            rsx! {
                                <tr
                                    class={if is_sel { "bg-indigo-950/20" } else { "hover:bg-neutral-900/40" }}
                                    onclick={move |_| table.toggle_row(idx)}
                                >
                                    <td class="px-4 py-2.5 text-neutral-300">{instance.id}</td>
                                    <td class="px-4 py-2.5 text-neutral-200 font-bold">{instance.service_name}</td>
                                    <td class="px-4 py-2.5 text-neutral-400">{instance.cluster_region}</td>
                                    <td class="px-4 py-2.5 text-sky-400">{format!("{:.1} ms", instance.p99_latency_ms)}</td>
                                    <td class="px-4 py-2.5" class={if instance.error_rate_percent > 1.0 { "text-rose-400 font-bold" } else { "text-emerald-400" }}>
                                        {format!("{:.2}%", instance.error_rate_percent)}
                                    </td>
                                </tr>
                            }
                        })}
                    </tbody>
                </table>
            </div>
        </div>
    }
}
```
