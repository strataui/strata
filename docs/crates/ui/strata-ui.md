# strata_ui: Headless & Composite Component Primitives

The `strata_ui` crate provides an exhaustive suite of accessible, high-performance UI primitives and composite components for the Strata engine. Designed with the architectural rigor of Radix UI and styled with Tailwind JIT utility classes, each component operates without browser DOM dependencies, compiling directly into Strata's retained-mode `UiIrNode` scene graph and AccessKit accessibility tree.

---

## Architecture and Design Principles

The component architecture is partitioned into four functional layers:

```text
┌────────────────────────────────────────────────────────────────────────┐
│                        Composite Components                            │
│           (DataTable, FormField, MediaDropzone, CommandPalette)        │
├────────────────────────────────────────────────────────────────────────┤
│                         Surface & Visual                               │
│           (Canvas2D, ScrollView, SceneViewport, GpuSurface)            │
├────────────────────────────────────────────────────────────────────────┤
│                      Styled Atomic Primitives                          │
│           (Button, Input, Modal, Tabs, Accordion, Badge, Stat)         │
├────────────────────────────────────────────────────────────────────────┤
│                      Headless State Controllers                        │
│           (FocusTrap, RovingIndex, FloatingAnchor, DismissableLayer)   │
└────────────────────────────────────────────────────────────────────────┘
```

1. **Headless State Mechanics**: Core interactive behaviors (focus management, keyboard roving indices, popup placement, and outside click dismissal) are decoupled from rendering.
2. **Zero-Allocation Layout Descriptors**: Components produce `NodeBlueprint` structures containing typed tags, utility style tokens, and reactive signal bindings.
3. **Accessibility First (WCAG 2.1 AAA)**: Every primitive integrates ARIA roles, states (`aria-expanded`, `aria-selected`, `aria-disabled`), and keyboard navigation hooks.
4. **Cloud Infrastructure Domain**: Native visual idioms tailored for mission-critical telemetry, observability consoles, cluster orchestration dashboards, and distributed systems monitoring.

---

## Headless Interactive Primitives

Headless primitives encapsulate complex interaction algorithms without dictating visual appearance.

### FocusTrap

Manages cyclic keyboard focus containment for modal dialogs, drawers, and overlay panels.

```rust
use strata_ui::prelude::FocusTrap;

let mut trap = FocusTrap::new(4);
assert_eq!(trap.active_index, 0);

// Tab key forward navigation wraps around
assert_eq!(trap.next(), 1);
assert_eq!(trap.next(), 2);
assert_eq!(trap.next(), 3);
assert_eq!(trap.next(), 0);

// Shift+Tab backward navigation
assert_eq!(trap.prev(), 3);
```

#### API Specification

| Field / Method | Type | Description |
| :--- | :--- | :--- |
| `active_index` | `usize` | Currently focused element index within the trap boundary. |
| `new(count: usize)` | `Self` | Instantiates a focus trap managing `count` interactive targets. |
| `next(&mut self)` | `usize` | Advances focus forward with modulo wraparound. |
| `prev(&mut self)` | `usize` | Steps focus backward with reverse wraparound. |
| `set(&mut self, idx: usize)` | `()` | Explicitly targets an element index. |

---

### RovingIndex

Controls single-tab-stop roving keyboard navigation across lists, toolbars, and tab lists.

```rust
use strata_ui::prelude::{Orientation, RovingIndex};

let mut roving = RovingIndex::new(3, Orientation::Horizontal);

assert_eq!(roving.handle_key("ArrowRight"), Some(1));
assert_eq!(roving.handle_key("ArrowRight"), Some(2));
assert_eq!(roving.handle_key("ArrowRight"), Some(0)); // Wraparound
assert_eq!(roving.handle_key("Home"), Some(0));
assert_eq!(roving.handle_key("End"), Some(2));
```

#### Keyboard Mapping

| Key Event | Orientation::Horizontal | Orientation::Vertical |
| :--- | :--- | :--- |
| `ArrowRight` | Increments index | No action |
| `ArrowLeft` | Decrements index | No action |
| `ArrowDown` | No action | Increments index |
| `ArrowUp` | No action | Decrements index |
| `Home` | Jumps to index 0 | Jumps to index 0 |
| `End` | Jumps to index `count - 1` | Jumps to index `count - 1` |

---

### FloatingAnchor

Calculates exact absolute positions for dropdowns, tooltips, popovers, and context menus relative to reference anchors.

```rust
use strata_primitives::{Point, Rect, Size};
use strata_ui::prelude::{FloatingAnchor, Placement};

let anchor = Rect::from_xywh(100.0, 100.0, 80.0, 40.0);
let floating_size = Size::new(120.0, 60.0);

let position = FloatingAnchor::compute_position(
    anchor,
    floating_size,
    Placement::Bottom,
    8.0, // Gap offset in pixels
);

assert_eq!(position.y, 148.0); // 100.0 + 40.0 + 8.0
assert_eq!(position.x, 80.0);  // 100.0 + (80.0 - 120.0) / 2.0
```

#### Collision Detection & Flipping

```rust
let clamped = FloatingAnchor::flip_if_overflow(
    Point::new(950.0, 500.0),
    Size::new(100.0, 50.0),
    Size::new(1000.0, 800.0), // Viewport bounds
);
assert_eq!(clamped.x, 900.0); // Restrained inside 1000px boundary
```

---

### DismissableLayer

Provides universal escape key detection and outside pointer collision algorithms.

```rust
use strata_primitives::{Point, Rect};
use strata_ui::prelude::DismissableLayer;

// Keyboard escape detection
assert!(DismissableLayer::should_dismiss_on_escape("Escape"));
assert!(DismissableLayer::should_dismiss_on_escape("Esc"));

// Outside hit testing
let layer_rect = Rect::from_xywh(50.0, 50.0, 200.0, 200.0);
assert!(DismissableLayer::should_dismiss_on_outside_click(Point::new(10.0, 10.0), layer_rect));
assert!(!DismissableLayer::should_dismiss_on_outside_click(Point::new(100.0, 100.0), layer_rect));
```

---

## Action and Button Controls

### Button

General-purpose action trigger supporting multiple visual hierarchies, loading spinners, icon slotting, and accessible states.

```rust
use strata_dom::prelude::*;
use strata_icons::prelude::IconName;
use strata_ui::prelude::*;

#[component]
pub fn RestartClusterButton(on_restart: EventHandler<()>) -> View {
    let is_restarting = Signal::new(false);

    rsx! {
        <Button
            variant={Some(ButtonVariant::Destructive)}
            size={Some(ButtonSize::Md)}
            is_loading={Some(is_restarting.get())}
            prefix_icon={Some(IconName::RefreshCw)}
            on_click={Some(Callback::new(move |_| {
                is_restarting.set(true);
                on_restart.call(());
            }))}
        >
            "Restart Ingress Controller"
        </Button>
    }
}
```

#### Component Properties

| Property | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `variant` | `Option<ButtonVariant>` | `ButtonVariant::Primary` | Visual styling variant (`Primary`, `Secondary`, `Destructive`, `Outline`, `Ghost`, `Link`). |
| `size` | `Option<ButtonSize>` | `ButtonSize::Md` | Sizing preset (`Sm`, `Md`, `Lg`, `Icon`). |
| `is_loading` | `Option<bool>` | `false` | When true, renders a spinner and sets `aria-disabled="true"`. |
| `disabled` | `Option<bool>` | `false` | Suppresses interaction and applies disabled opacity tokens. |
| `prefix_icon` | `Option<IconName>` | `None` | Prepended vector icon. |
| `suffix_icon` | `Option<IconName>` | `None` | Appended vector icon. |
| `class` | `Option<&'static str>` | `None` | Supplementary Tailwind utility overrides. |
| `on_click` | `Option<Callback<()>>` | `None` | Event dispatch triggered on click or keyboard `Enter`/`Space`. |

---

### IconButton and ButtonGroup

```rust
// Compact circular action button
let inspect_btn = IconButton(
    IconName::Activity,
    Some(ButtonVariant::Ghost),
    Some(ButtonSize::Sm),
    Some(true), // is_round
    Some("Inspect Node Telemetry"), // aria_label
    None, // disabled
    None, // class
    None, // on_click
);

// Segmented group container
let view_toggle = ButtonGroup(
    Some(vec![
        Button(Some(ButtonVariant::Secondary), Some(ButtonSize::Sm), None, None, None, None, None, None, Some(vec!["Metrics".into()])),
        Button(Some(ButtonVariant::Ghost), Some(ButtonSize::Sm), None, None, None, None, None, None, Some(vec!["Logs".into()])),
        Button(Some(ButtonVariant::Ghost), Some(ButtonSize::Sm), None, None, None, None, None, None, Some(vec!["Traces".into()])),
    ]),
    Some("border border-neutral-800 rounded-lg p-0.5 bg-neutral-900"),
);
```

---

## Form Input Primitives

### Input and PasswordInput

Accessible single-line text fields with integrated label bindings, icon decorators, and validation states.

```rust
use strata_dom::Signal;
use strata_icons::prelude::IconName;
use strata_ui::prelude::Input;

let endpoint_sig = Signal::new(String::from("https://grpc.prod.strata.internal:8443"));

let input_field = Input(
    Some("Cluster Endpoint"),
    Some("Enter fully qualified gRPC address"),
    Some(endpoint_sig),
    Some(IconName::Server),
    None, // suffix_icon
    Some("Internal TLS termination endpoint"),
    Some(false), // error state
    Some(false), // disabled
    Some("w-full max-w-lg font-mono"),
    None, // on_input callback
);
```

### NumberInput, Switch, and Slider

Continuous and discrete parameter adjustments for telemetry alert thresholds and compute allocation.

```rust
use strata_dom::Signal;
use strata_ui::prelude::{NumberInput, Slider, Switch};

// Discrete CPU Core Allocation
let core_count = Signal::new(16);
let cpu_input = NumberInput(
    Some("Provisioned Cores"),
    core_count,
    Some(2),   // min
    Some(128), // max
    Some(2),   // step
    None,      // disabled
    None,      // class
);

// Boolean Toggle for Mutual TLS
let mtls_enabled = Signal::new(true);
let mtls_toggle = Switch(
    Some("Enforce Mutual TLS"),
    Some("Require cryptographic client verification on all edge routes"),
    mtls_enabled,
    None, // disabled
);

// Continuous Threshold Slider
let memory_alert_limit = Signal::new(85.0);
let threshold_slider = Slider(
    0.0,
    100.0,
    memory_alert_limit,
    Some(1.0), // step
    Some("w-full"),
);
```

---

## Data Display and Observability

### Card Layout Architecture

`Card`, `CardHeader`, `CardContent`, and `CardFooter` assemble structured visual surfaces with dark-mode styling:

```rust
use strata_ui::prelude::*;

let cluster_card = Card(
    Some(vec![
        CardHeader(
            Some("Cluster Health Status"),
            Some("Active nodes across us-east-01 and eu-west-01"),
            Some(Badge(Some(BadgeVariant::Success), Some("Healthy"), None).root),
            None,
        ).root,
        CardContent(
            Some(vec![
                Stat("Cluster CPU Load", "38.4%", Some(2.1), Some("Past 1h"), None).root,
                Stat("Network Ingestion", "1.42 GB/s", Some(-0.4), Some("Nominal"), None).root,
            ]),
            Some("grid grid-cols-2 gap-4"),
        ).root,
        CardFooter(
            Some(vec![
                Button(Some(ButtonVariant::Outline), Some(ButtonSize::Sm), None, None, None, None, None, None, Some(vec!["View Raw Metrics".into()])).root,
            ]),
            Some("justify-end border-t border-neutral-800/80 pt-3"),
        ).root,
    ]),
    Some("bg-neutral-900/80 border border-neutral-800 rounded-xl p-5 shadow-2xl"),
);
```

### Stat Component

Displays high-prominence numerical KPIs with percentage trend indicators:

| Parameter | Type | Description |
| :--- | :--- | :--- |
| `label` | `&'static str` | Descriptive metric title (e.g., `"P99 Latency"`). |
| `value` | `&'static str` | Formatted value string (e.g., `"4.12 ms"`). |
| `change_percent` | `Option<f32>` | Delta percentage; automatically colorizes emerald (positive) or rose (negative). |
| `period` | `Option<&'static str>` | Relative timeframe label (e.g., `"vs prior 24h"`). |
| `class` | `Option<&'static str>` | Custom styling overrides. |

### Tabs and AccordionItem

```rust
use strata_dom::Signal;
use strata_ui::prelude::{AccordionItem, Tabs};

// Navigation Tabs
let active_tab = Signal::new(0);
let metrics_tabs = Tabs(
    vec!["Throughput", "Latency", "Error Budget", "Audit Log"],
    active_tab,
    Some("border-b border-neutral-800"),
);

// Collapsible Details
let is_expanded = Signal::new(false);
let tls_accordion = AccordionItem(
    "Cryptographic Cipher Suites & Handshake Protocol",
    is_expanded,
    None, // Content children
    Some("border-b border-neutral-800 py-3"),
);
```

---

## Feedback and Overlays

### Modal and Drawer

Overlay dialogs with built-in backdrop blurring, keyboard escape interception, and focus traps.

```rust
use strata_dom::Signal;
use strata_ui::prelude::{Drawer, Modal, Placement};

let is_modal_open = Signal::new(true);

let confirmation_dialog = Modal(
    is_modal_open,
    "Drain Node Traffic",
    Some("Draining node-04 will evict 48 active pods and reroute traffic to standby instances."),
    None, // Content children
    Some("max-w-md bg-neutral-950 border border-neutral-800 p-6 rounded-2xl shadow-2xl"),
);

let is_drawer_open = Signal::new(false);
let telemetry_drawer = Drawer(
    is_drawer_open,
    Placement::Right,
    "Pod Deep Inspection",
    None, // Content children
    Some("w-96 bg-neutral-900 border-l border-neutral-800"),
);
```

### Toast Notification Queue

Global, thread-safe notification dispatch system:

```rust
use strata_ui::prelude::Toast;

// Imperative dispatch from any subsystem or async task
let id1 = Toast::success("Deployment rolled out to 100% of canary instances.");
let id2 = Toast::error("Connection timed out to telemetry-collector-02.");
let id3 = Toast::warning("Node memory usage exceeded 85% threshold.");

// Targeted dismissal
Toast::dismiss(id1);

// Bulk reset
Toast::clear();
```

---

## Navigation Architecture

```rust
use strata_icons::prelude::IconName;
use strata_ui::prelude::{Breadcrumb, BreadcrumbItemData, Sidebar, SidebarItemData};

// Sidebar Configuration
let nav_items = vec![
    SidebarItemData {
        label: "Cluster Overview",
        icon: IconName::Server,
        href: Some("/clusters"),
        badge: None,
        is_active: true,
    },
    SidebarItemData {
        label: "Security & IAM",
        icon: IconName::Shield,
        href: Some("/security"),
        badge: Some("3 Alerts"),
        is_active: false,
    },
    SidebarItemData {
        label: "Network Topologies",
        icon: IconName::Network,
        href: Some("/networks"),
        badge: None,
        is_active: false,
    },
];

let primary_sidebar = Sidebar(nav_items, None, None, None, None);

// Hierarchical Breadcrumbs
let trail = vec![
    BreadcrumbItemData { label: "Clusters", href: Some("/clusters"), is_current: false },
    BreadcrumbItemData { label: "us-east-prod", href: Some("/clusters/us-east-prod"), is_current: false },
    BreadcrumbItemData { label: "ingress-gateway-01", href: None, is_current: true },
];
let breadcrumbs = Breadcrumb(trail, None);
```

---

## Scroll Containers and Frameless Window Chrome

### ScrollArea

`strata_ui::scroll_area::ScrollArea` is the declarative scroll container. It renders a `div` with `overflow-y-auto` and a stable `id`; the runtime uses that id to keep the scroll offset across component re-renders.

```rust
use strata_ui::prelude::ScrollArea;

let feed = ScrollArea(
    Some("telemetry-feed"),   // stable id for scroll-position persistence
    Some("h-96 w-full px-4"), // Tailwind overrides
    Some(rows),               // children
    Some(true),               // show_scrollbar
);
```

```rust
rsx! {
    <ScrollArea id={Some("telemetry-feed")} class={Some("h-96 w-full px-4")} show_scrollbar={Some(true)}>
        {rows}
    </ScrollArea>
}
```

| Parameter | Type | Description |
| :--- | :--- | :--- |
| `id` | `Option<&'static str>` | Stable container identity used for scroll-position persistence. |
| `class` | `Option<&'static str>` | Additional Tailwind utilities appended to the container. |
| `children` | `Option<Vec<NodeBlueprint>>` | Scrollable content. |
| `show_scrollbar` | `Option<bool>` | Optional scrollbar flag; defaults to `true`. |

For containers that overflow, the runtime supplies the interaction behavior: wheel input feeds the kinetic momentum solver, the floating thumb expands and becomes more opaque on hover without shifting layout, the thumb can be dragged, and a saturated inner container forwards the remaining delta to its parent.

### TitleBar and WindowDragHandle

`strata_ui::title_bar::TitleBar` emits frameless window chrome. It is a `<header>` whose body carries `data-app-region="drag"` when `drag_region` is true, or `data-app-region="no-drag"` when false.

```rust
use strata_ui::prelude::{TitleBar, WindowDragHandle};

rsx! {
    <TitleBar
        title={Some("Strata Telemetry".to_string())}
        drag_region={Some(true)}
        show_controls={Some(true)}
        on_minimize={Some(std::rc::Rc::new(|_: strata_dom::Event| { /* minimize */ }))}
        on_maximize={Some(std::rc::Rc::new(|_: strata_dom::Event| { /* toggle maximize */ }))}
        on_close={Some(std::rc::Rc::new(|_: strata_dom::Event| { /* close */ }))}
    />
}

rsx! {
    <WindowDragHandle class={Some("h-8 w-full")}>
        <span class="text-xs text-neutral-400">{"Custom chrome drag strip"}</span>
    </WindowDragHandle>
}
```

| Parameter | Type | Description |
| :--- | :--- | :--- |
| `title` | `Option<String>` | Title text rendered on the left. |
| `drag_region` | `Option<bool>` | Emits `data-app-region="drag"` when true; defaults to `true`. |
| `show_controls` | `Option<bool>` | Renders the built-in minimize, maximize, and close buttons; defaults to `true`. |
| `on_minimize` | `Option<Rc<dyn Fn(Event)>>` | Invoked by the minimize button. |
| `on_maximize` | `Option<Rc<dyn Fn(Event)>>` | Invoked by the maximize button. |
| `on_close` | `Option<Rc<dyn Fn(Event)>>` | Invoked by the close button. |
| `class` | `Option<&'static str>` | Additional Tailwind utilities. |
| `children` | `Option<Vec<NodeBlueprint>>` | Extra controls placed before the built-in buttons. |

The built-in buttons set `data-window-action="minimize"`, `"maximize"`, and `"close"`, and each also opts out of dragging with `data-app-region="no-drag"`. `WindowDragHandle` is the bare primitive: a `div` with `data-app-region="drag"` for custom title strips.

`strata_ui` deliberately does not depend on `strata_platform`. Components only emit `data-*` markers and optional callbacks; the application or runtime translates them into native calls. The runtime treats a primary press on a drag region as a window-drag candidate once the pointer moves past a small threshold, and a double-click on a drag region toggles maximize.

---

## High-Throughput Composite DataTable

The `DataTable` component combines typed column accessors with structured headers, sorting, and tabular styling:

```rust
use strata_ui::prelude::{DataTable, TableColumnDef};

struct ComputeNode {
    hostname: &'static str,
    ip_address: &'static str,
    cpu_cores: u32,
    memory_gb: u32,
    status: &'static str,
}

let nodes = vec![
    ComputeNode { hostname: "node-us-east-01", ip_address: "10.0.1.12", cpu_cores: 64, memory_gb: 256, status: "Ready" },
    ComputeNode { hostname: "node-us-east-02", ip_address: "10.0.1.13", cpu_cores: 64, memory_gb: 256, status: "Ready" },
    ComputeNode { hostname: "node-us-east-03", ip_address: "10.0.1.14", cpu_cores: 128, memory_gb: 512, status: "Draining" },
];

let columns = vec![
    TableColumnDef::new("host", "Hostname", |n: &ComputeNode| n.hostname.to_string()),
    TableColumnDef::new("ip", "Private IP", |n: &ComputeNode| n.ip_address.to_string()),
    TableColumnDef::new("cores", "Cores", |n: &ComputeNode| format!("{} vCPU", n.cpu_cores)),
    TableColumnDef::new("ram", "Memory", |n: &ComputeNode| format!("{} GB", n.memory_gb)),
    TableColumnDef::new("status", "Status", |n: &ComputeNode| n.status.to_string()),
];

let table_view = DataTable(columns, &nodes, Some("border border-neutral-800 rounded-lg overflow-hidden"));
```

---

## GPU Surfaces and 2D/3D Hybrid Graphics

For rich telemetry dashboards, `strata_ui` exposes hardware-accelerated drawing surfaces:

```rust
use std::rc::Rc;
use strata_style::types::Color;
use strata_ui::prelude::{Canvas2D, ScrollView};

// 2D Canvas Drawing Pipeline
let telemetry_canvas = Canvas2D(
    Some(600.0),
    Some(240.0),
    Some("rounded-lg border border-neutral-800 bg-neutral-950"),
    Some(Rc::new(|ctx| {
        ctx.set_fill_style(Color::rgb(14, 165, 233));
        ctx.fill_rect(20.0, 40.0, 80.0, 160.0);
    })),
);

// Virtualized Viewport Container
let scrollable_feed = ScrollView(
    Some(vec![telemetry_canvas.root]),
    Some("h-96 w-full overflow-y-auto"),
    Some(true), // show_scrollbar
);
```
