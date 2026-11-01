# strata_form: Reactive Form State & Schema Validation Engine

The `strata_form` crate provides a signal-driven, zero-re-render form state and validation engine for the Strata framework. Designed as a pure-Rust equivalent to `react-hook-form` and `zod`, `strata_form` delivers fine-grained field subscription, touched/dirty mutation tracking, declarative validation rule pipelines, dynamic field arrays, and atomic submission orchestration without causing global DOM or component tree re-evaluations.

---

## Architectural Principles

Form state in Strata operates through fine-grained reactive signals:

```text
┌─────────────────────────────────────────────────────────────┐
│                         Form Root                           │
│  (Orchestrates register, validate_all, handle_submit)       │
└──────────────────────────────┬──────────────────────────────┘
                               │
                ┌──────────────┴──────────────┐
                ▼                             ▼
┌─────────────────────────────┐ ┌─────────────────────────────┐
│      FormField "email"      │ │    FormField "password"     │
│ • value: Signal<String>     │ │ • value: Signal<String>     │
│ • error: Signal<Option<str>>│ │ • error: Signal<Option<str>>│
│ • is_dirty: Signal<bool>    │ │ • is_dirty: Signal<bool>    │
│ • is_touched: Signal<bool>  │ │ • is_touched: Signal<bool>  │
│ • rules: Vec<ValidationRule>│ │ • rules: Vec<ValidationRule>│
└─────────────────────────────┘ └─────────────────────────────┘
                │                             │
                ▼                             ▼
┌─────────────────────────────────────────────────────────────┐
│                     Dynamic Field Array                     │
│    (append, prepend, insert, remove, swap, move_item)       │
└─────────────────────────────────────────────────────────────┘
```

1. **Zero Global Re-renders**: Keystroke mutations mutate only the targeted field's `value` signal. Sibling fields and parent containers experience zero re-render passes.
2. **Declarative Validation Schemas**: Validation rules evaluate immediately on blur (`touch`), on input (if already dirty), or in batch during form submission.
3. **Atomic Submission Safety**: Form submissions validate all registered fields synchronously; if any rule fails, submission is aborted and error signals are illuminated simultaneously.

---

## Form State Controller: `Form` and `use_form`

The `Form` state container coordinates registered fields and submission handlers:

```rust
use strata_form::prelude::*;

let form = use_form();

// Register cluster credentials and provisioning fields
let cluster_name = form.register(
    "cluster_name",
    vec![
        ValidationRule::required("Cluster name is required"),
        ValidationRule::min_length(4, "Must be at least 4 characters"),
    ],
);

let admin_email = form.register(
    "admin_email",
    vec![
        ValidationRule::required("Admin contact email is required"),
        ValidationRule::email("Please provide a valid corporate email"),
    ],
);

let replica_count = form.register(
    "replica_count",
    vec![
        ValidationRule::min(1.0, "Minimum replica count is 1"),
        ValidationRule::max(64.0, "Maximum replica count is 64"),
    ],
);
```

#### API Specification: `Form`

| Method | Parameters | Return Type | Description |
| :--- | :--- | :--- | :--- |
| `use_form()` | None | `Form` | Instantiates a new reactive form state controller. |
| `register(name, rules)` | `(&str, Vec<ValidationRule<String>>)` | `FormField` | Registers a field by name with associated validation constraints. |
| `get_field(name)` | `&str` | `Option<FormField>` | Retrieves an existing registered field handle. |
| `validate_all()` | None | `bool` | Executes validation rules across every registered field. Returns `true` if all pass. |
| `handle_submit(callback)` | `impl FnOnce(HashMap<String, String>)` | `bool` | Triggers `validate_all()`; executes callback with field values if valid. |
| `reset()` | None | `()` | Resets all fields back to initial values, clearing errors, dirty, and touched states. |

---

## Field State Tracking: `FormField`

Each individual field maintains independent reactive telemetry:

```rust
use strata_form::prelude::*;

let field = form.register("endpoint", vec![ValidationRule::required("Endpoint required")]);

// Initial state
assert!(!field.is_dirty.get());
assert!(!field.is_touched.get());
assert_eq!(field.error.get(), None);

// Simulate user typing
field.set_value("https://k8s.us-east-01.strata.internal:6443");
assert!(field.is_dirty.get());

// User unfocuses field (blur)
field.touch();
assert!(field.is_touched.get());

// Validate field explicitly
assert!(field.validate());
assert_eq!(field.error.get(), None);
```

#### Member Signals: `FormField`

| Signal | Type | Description |
| :--- | :--- | :--- |
| `value` | `Signal<String>` | Current string value entered in the field. |
| `error` | `Signal<Option<String>>` | Active validation error message, or `None` if field is valid. |
| `is_dirty` | `Signal<bool>` | `true` if current value differs from its initial registration value. |
| `is_touched` | `Signal<bool>` | `true` once the field has lost focus at least once (`blur`). |

---

## Validation Schema Rules: `ValidationRule`

`ValidationRule` provides standard validators as well as custom validation closures:

```rust
use strata_form::prelude::ValidationRule;

// Standard constraint rules
let req = ValidationRule::required("Field cannot be empty");
let min_len = ValidationRule::min_length(8, "Must contain at least 8 characters");
let max_len = ValidationRule::max_length(64, "Exceeds maximum length of 64 characters");
let email = ValidationRule::email("Invalid email format");
let min_val = ValidationRule::min(10.0, "Must be greater than or equal to 10");
let max_val = ValidationRule::max(100.0, "Must be less than or equal to 100");

// Custom procedural validation rule (e.g. Subnet CIDR block validation)
let cidr_rule = ValidationRule::custom(|val: &String| {
    if val.starts_with("10.") && val.contains("/16") {
        Ok(())
    } else {
        Err("Subnet must be an internal 10.x.x.x/16 CIDR block".to_string())
    }
});
```

#### Validation Rules Matrix

| Rule Constructor | Parameters | Validation Logic |
| :--- | :--- | :--- |
| `ValidationRule::required(msg)` | `impl Into<String>` | `!val.trim().is_empty()` |
| `ValidationRule::min_length(min, msg)` | `(usize, impl Into<String>)` | `val.len() >= min` |
| `ValidationRule::max_length(max, msg)` | `(usize, impl Into<String>)` | `val.len() <= max` |
| `ValidationRule::email(msg)` | `impl Into<String>` | Matches `^[^@\s]+@[^@\s]+\.[^@\s]+$` |
| `ValidationRule::min(min, msg)` | `(f64, impl Into<String>)` | Parsed numeric value `n >= min` |
| `ValidationRule::max(max, msg)` | `(f64, impl Into<String>)` | Parsed numeric value `n <= max` |
| `ValidationRule::custom(closure)` | `impl Fn(&String) -> Result<(), String> + 'static` | User-defined predicate |

---

## Dynamic Field Arrays: `use_field_array`

For dynamic repeating items (such as firewall egress rules, cluster nodes, or environment variables):

```rust
use strata_form::prelude::use_field_array;

let mut allowed_ips = use_field_array(vec![
    "10.0.1.0/24".to_string(),
    "10.0.2.0/24".to_string(),
]);
assert_eq!(allowed_ips.len(), 2);

// Append new entry
allowed_ips.append("192.168.1.0/24".to_string());
assert_eq!(allowed_ips.len(), 3);

// Insert at index
allowed_ips.insert(1, "172.16.0.0/16".to_string());
assert_eq!(allowed_ips.items().get()[1], "172.16.0.0/16");

// Remove item
allowed_ips.remove(1);
assert_eq!(allowed_ips.len(), 3);

// Reorder items
allowed_ips.swap(0, 2);
```

#### API Specification: `FieldArray`

| Method | Parameters | Return Type | Description |
| :--- | :--- | :--- | :--- |
| `use_field_array(init)` | `Vec<String>` | `FieldArray` | Initializes a reactive dynamic array. |
| `items()` | None | `Signal<Vec<String>>` | Reactive signal tracking list items. |
| `len()` | None | `usize` | Item count in the array. |
| `is_empty()` | None | `bool` | True if array contains zero items. |
| `append(item)` | `String` | `()` | Appends item to the end of the array. |
| `prepend(item)` | `String` | `()` | Prepends item to the beginning of the array. |
| `insert(index, item)` | `(usize, String)` | `()` | Inserts item at index position. |
| `remove(index)` | `usize` | `Option<String>` | Removes and returns item at index. |
| `swap(i, j)` | `(usize, usize)` | `()` | Swaps positions of two items. |
| `move_item(from, to)` | `(usize, usize)` | `()` | Relocates an item from source to destination index. |

---

## Production Component Example: Production Cluster Provisioning Form

```rust
use std::collections::HashMap;
use strata_dom::prelude::*;
use strata_form::prelude::*;
use strata_ui::prelude::*;

#[component]
pub fn ClusterProvisioningForm() -> View {
    let form = use_form();

    let name_field = form.register(
        "cluster_name",
        vec![
            ValidationRule::required("Cluster name cannot be blank"),
            ValidationRule::min_length(4, "Must be at least 4 characters"),
        ],
    );

    let replicas_field = form.register(
        "replicas",
        vec![
            ValidationRule::required("Replica target is mandatory"),
            ValidationRule::min(1.0, "Must provision at least 1 node"),
            ValidationRule::max(128.0, "Cluster maximum limit is 128 nodes"),
        ],
    );

    let submit_status = Signal::new(Option::<String>::None);

    let on_submit = {
        let form = form.clone();
        let submit_status = submit_status.clone();
        move |_| {
            let form_clone = form.clone();
            let submit_status = submit_status.clone();
            let success = form_clone.handle_submit(move |vals: HashMap<String, String>| {
                let name = vals.get("cluster_name").unwrap();
                let nodes = vals.get("replicas").unwrap();
                submit_status.set(Some(format!("Provisioning cluster '{}' with {} replicas...", name, nodes)));
            });
            if !success {
                submit_status.set(Some("Form submission failed validation.".into()));
            }
        }
    };

    rsx! {
        <form class="flex flex-col gap-5 max-w-lg bg-neutral-950 p-6 border border-neutral-800 rounded-xl shadow-2xl font-mono text-xs">
            <header class="border-b border-neutral-800 pb-3">
                <h3 class="text-sm font-bold text-neutral-200 uppercase tracking-wide">
                    "Provision High-Throughput Cluster"
                </h3>
            </header>

            // Cluster Name Field
            <div class="flex flex-col gap-1.5">
                <label class="text-neutral-400 font-semibold">"Cluster Identifier"</label>
                <input
                    type="text"
                    placeholder="e.g. prod-telemetry-01"
                    class="bg-neutral-900 border border-neutral-800 rounded-lg px-3 py-2 text-neutral-200 focus:outline-none focus:border-indigo-500"
                    oninput={move |e| name_field.set_value(e.value)}
                    onblur={move |_| name_field.touch()}
                />
                {if let Some(err) = name_field.error.get() {
                    rsx! { <span class="text-rose-400 text-[11px] font-sans">{err}</span> }
                } else {
                    rsx! { <span class="text-neutral-500 text-[11px]">"Unique DNS-compliant cluster subdomain"</span> }
                }}
            </div>

            // Replica Nodes Field
            <div class="flex flex-col gap-1.5">
                <label class="text-neutral-400 font-semibold">"Initial Replica Pool Size"</label>
                <input
                    type="number"
                    placeholder="16"
                    class="bg-neutral-900 border border-neutral-800 rounded-lg px-3 py-2 text-neutral-200 focus:outline-none focus:border-indigo-500"
                    oninput={move |e| replicas_field.set_value(e.value)}
                    onblur={move |_| replicas_field.touch()}
                />
                {if let Some(err) = replicas_field.error.get() {
                    rsx! { <span class="text-rose-400 text-[11px] font-sans">{err}</span> }
                } else {
                    rsx! { <span class="text-neutral-500 text-[11px]">"Initial compute nodes provisioned across zones"</span> }
                }}
            </div>

            // Submit Button
            <button
                type="button"
                class="mt-2 bg-indigo-600 hover:bg-indigo-500 text-white font-sans font-semibold py-2.5 rounded-lg transition-colors cursor-pointer"
                onclick={on_submit}
            >
                "Initialize Provisioning Pipeline"
            </button>

            // Status message
            {if let Some(msg) = submit_status.get() {
                rsx! {
                    <div class="p-3 bg-neutral-900 border border-neutral-800 rounded text-neutral-300">
                        {msg}
                    </div>
                }
            } else {
                rsx! { <span class="hidden" /> }
            }}
        </form>
    }
}
```
