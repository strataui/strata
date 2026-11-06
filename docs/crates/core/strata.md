# Crate `strata`: Root Facade & Application Runtime

The `strata` crate serves as the root facade and central integration layer of the Strata framework. It aggregates core modules, compilation passes, style resolvers, layout engines, and hardware presenters into a cohesive, high-performance universal UI runtime.

---

## Architectural Role & Subsystems

Rather than requiring downstream applications to directly manage cross-crate dependency graphs, `strata` unifies foundational subsystems through a consolidated facade and standard runtime orchestrator:

```text
┌────────────────────────────────────────────────────────────────────────┐
│                              Application                               │
│                (RSX Templates, Components, State Stores)               │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│                              strata                                    │
│                 (Root Facade, Prelude, Runtime Builder)                │
└───────┬──────────┬──────────┬──────────┬──────────┬──────────┬─────────┘
        │          │          │          │          │          │
        ▼          ▼          ▼          ▼          ▼          ▼
 ┌────────────┐┌───────────┐┌───────────┐┌───────────┐┌───────────┐┌───────────┐
 │strata_macro││strata_dom ││strata_style││strata_layout│strata_render│strata_plat- │
 │(rsx!, store││(Signals,  ││(Cascade,  ││(LayoutNG, ││(Display   ││   form    │
 │ components)││ UiIrGraph)││ Tailwind) ││ Flex/Grid)││  Lists)   ││ (Windows) │
 └────────────┘└───────────┘└───────────┘└───────────┘└───────────┘└───────────┘
```

### Core Mandatory Subsystems
- **`strata_dom`**: Fine-grained reactive signal graph, intermediate representation (`UiIrNode`), and synthetic event routing.
- **`strata_macro`**: Procedural macros including `rsx!`, `#[component]`, `dynamic!`, and `#[derive(Store)]`.
- **`strata_style`**: CSS stylesheet parser, selector specificity cascade solver, and Tailwind v3/v4 JIT compiler.
- **`strata_layout`**: Chromium LayoutNG-parity multi-pass Box Model, Flexbox (Level 1), and Grid (Level 2) solvers.
- **`strata_render`**: Retained display list compiler, compositing tree, and GPU presentation abstractions.
- **`strata_platform`**: Cross-platform window abstraction, event loop drivers, and hardware swapchain interfaces.
- **`strata_primitives`**: Mathematical vectors, 1/64th fixed-point coordinates (`LayoutUnit`), color science (`Oklch`, `Rgba`), and bump arenas.
- **`strata_js`**, **`strata_vfs`**, **`strata_storage`**, **`strata_web_api`**: Sandboxed scripting, virtual file systems, Bitcask storage, and standard Web platform APIs.

### Optional Feature Modules
- `ui`: Reusable headless component suite.
- `icons`: High-resolution vector icon library.
- `aria`: Headless WAI-ARIA accessibility state machines and floating collision solvers.
- `store`: Distributed reactive store derives and state synchronization.
- `query`: Asynchronous data fetching, query caching, and cache invalidation.
- `table`: High-performance virtualized telemetry table with column pinning and sorting.
- `charts`: Subpixel time-series metric charts and analytical visualizers.
- `parallel`: Rayon-driven parallel layout solver for massive DOM subtrees.

---

## Crate Prelude

Importing `strata::prelude::*` brings the entire declarative development environment into scope:

```rust
use strata::prelude::*;
```

### Key Re-Exports

| Namespace | Types & Functions | Description |
| :--- | :--- | :--- |
| **Reactive DOM** | `Signal`, `use_signal`, `use_state`, `use_memo`, `use_effect`, `View`, `NodeBlueprint`, `Tag`, `Attribute`, `Event`, `EventType` | Declarative reactive primitives and virtual DOM blueprints. |
| **Declarative Macros** | `rsx!`, `component`, `dynamic` | Template syntax and component definitions. |
| **Primitives** | `Point`, `Size`, `Rect`, `EdgeInsets`, `LayoutUnit`, `FixedRect`, `Rgba`, `Hsla`, `Oklch`, `BumpArena` | Fixed-point layout coordinates and color models. |
| **Styling** | `ComputedStyle` | Resolved CSS property container. |
| **Platform** | `WindowConfig`, `PlatformEvent`, `EventLoopControl` | Window lifecycle and system event handling. |
| **Application Runtime** | `app`, `launch`, `launch_headless`, `launch_window`, `StrataApp`, `StrataAppBuilder`, `AppRuntime`, `AppFrameInfo`, `AppLifecycleHooks` | Programmatic runtime drivers and frame pacer. |

---

## The Application Runtime (`AppRuntime`)

`AppRuntime` encapsulates the active runtime instance of a Strata application. It bridges the component graph, styling cascade, layout calculations, and rendering commands.

```rust
pub struct AppRuntime {
    pub root_blueprint: NodeBlueprint,
    pub layout_root: LayoutNode,
    pub display_list: DisplayList,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub dpr: f32,
    pub frame_count: u64,
}
```

### Frame Execution Cycle

When an event or state mutation invalidates the application, `AppRuntime` advances the frame via `tick_frame()`:

```text
┌────────────────────────────────────────────────────────┐
│                   State Invalidation                   │
│        (Signal Write / Platform Window Resize)         │
└───────────────────────────┬────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────┐
│              AppRuntime::tick_frame()                  │
│  1. Increment frame_count                              │
│  2. Recompute layout: compute_layout(width, height)    │
│  3. Compile display list: Compositor::build(...)       │
└───────────────────────────┬────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────┐
│             Swapchain Presentation                     │
│  Compositor blits draw commands to SurfacePresenter    │
└────────────────────────────────────────────────────────┘
```

### Layout Tree Synthesis
During initialization, `AppRuntime::build_layout_tree(blueprint)` recursively translates an RSX `NodeBlueprint` into a `LayoutNode` hierarchy:
1. Inherits cascading properties from parent `ComputedStyle`.
2. Tokenizes and evaluates Tailwind utility classes via `TailwindEngine::apply_class`.
3. Resolves intrinsic tag defaults (such as Flexbox columns for `<main>` or `<div>`).
4. Approximates intrinsic text dimensions when width is `Dimension::Auto`.

---

## Native Text Input and Selection

Text inputs are first-class runtime components, not application widget state. A press anywhere inside the padded container that wraps an `<input>` focuses the input; a `cursor-text` class, a `Cursor::Text` style, or a `<label>` ancestor registers the container as a fallback hit target while the input itself keeps priority. While focused, the runtime draws a blinking insertion caret (530 ms toggle, driven by `AnimationTick` through `poll_caret_blink`) and resets the blink cycle on every edit so the caret stays solid while typing.

Mouse semantics on a focused field:

- Click positions the caret at the nearest character boundary using subpixel text measurement.
- Dragging extends the selection from the press anchor.
- Shift+click extends the existing selection to the new point.
- Double-click selects the word under the pointer.
- Triple-click selects the line.
- Four or more clicks select the whole buffer.

Keyboard semantics on the focused field:

- `Left` and `Right` move the caret, or move by word with `Ctrl`/`Cmd`; `Home` and `End` jump to the buffer edges; `Shift` extends the selection.
- `Ctrl`/`Cmd` + `A` selects all, `C` copies, `V` pastes, `X` cuts.
- `Ctrl`/`Cmd` + `Z` undoes and `Y` (or `Shift+Z`) redoes.
- `Ctrl`/`Cmd` + `Backspace` and `Delete` remove the previous or next word.

`handle_key_event` performs the edits and routes clipboard work through `apply_edit_command`, which mirrors copies into an in-process fallback buffer so headless tests and processes without a display server keep working.

`sync_active_input_layout` pushes the focused input's live text, caret styling, and measured size into the layout tree after every rebuild and edit. This makes the runtime authoritative for the focused field, so typing works even when the application passes a stale `value` attribute or registers no `on_input` handler.

## Automatic Cursor Switching

`hit_test_cursor` resolves the pointer icon for a coordinate against the blueprint and computed layout. Children are tested front-to-back first, so the deepest interactive element wins. Explicit CSS `cursor` values map to their platform equivalent; otherwise a `<button>`, an `<a>`, a `cursor-pointer` class, or any registered `on_click` listener resolves to `Pointer`, and an `<input>` or `Cursor::Text` container resolves to `Text`. Everything else returns `Default`.

The native pump applies the result through `gtk_set_cursor` on pointer movement and resets it to `Default` on `CursorLeft`, so the icon does not stay stuck when the pointer leaves the window.

## User Event Hook

Every native window event passes through a single application hook before the runtime's built-in handling. `runtime::invoke_user_event_hook(event, runtime, hook)` calls the closure supplied to `launch_gtk_with_events` and returns a bool that asks the pump to rebuild the frame. Because the hook runs first, applications can intercept window, tray, and single-instance events:

- `PlatformEvent::TrayIconClicked`
- `PlatformEvent::TrayMenuItemClicked(id)`
- `PlatformEvent::SingleInstanceTriggered(args)`

Built-in handling runs regardless of the hook's return value; returning `true` additionally asks the pump to rebuild the frame.

## Scroll Chaining

`dispatch_smooth_scroll` accumulates wheel input into the kinetic momentum solver for the innermost scroll container under the pointer. It collects the containers whose bounds contain the point, then chooses the innermost one that can still absorb the delta in the requested direction. A container that has reached its top or bottom limit is skipped, so the same wheel event is applied to its parent instead of being trapped by the saturated inner scroller. When no container can move, the event is a no-op.

---

## Application Orchestration (`StrataApp` & `StrataAppBuilder`)

`StrataApp` provides low-level control over window management, swapchain presentation, lifecycle hooks, and manual frame stepping.

### Programmatic Configuration via `StrataAppBuilder`

```rust
use strata::prelude::*;
use strata::platform::WindowConfig;

let mut app = StrataApp::builder()
    .with_window(WindowConfig {
        title: "Telemetry Ingestion Node".into(),
        width: 1440,
        height: 900,
        resizable: true,
        decorations: true,
        transparent: false,
    })
    .with_dpr(2.0)
    .on_start(|runtime| {
        println!("Runtime initialized with viewport {}x{}", runtime.viewport_width, runtime.viewport_height);
    })
    .on_frame(|info, runtime| {
        if info.frame_index % 60 == 0 {
            println!("Frame {}: {} render commands generated", info.frame_index, info.command_count);
        }
    })
    .on_exit(|runtime| {
        println!("Shutting down runtime after {} frames", runtime.frame_count);
    })
    .build(TelemetryDashboard);
```

### Lifecycle Hook Interfaces

```rust
pub struct AppLifecycleHooks {
    pub on_start: Option<Box<dyn FnMut(&mut AppRuntime)>>,
    pub on_event: Option<Box<dyn FnMut(&PlatformEvent, &mut EventLoopControl, &mut AppRuntime)>>,
    pub on_frame: Option<Box<dyn FnMut(AppFrameInfo, &mut AppRuntime)>>,
    pub on_draw: Option<Box<dyn FnMut(&mut DisplayList)>>,
    pub on_exit: Option<Box<dyn FnMut(&mut AppRuntime)>>,
}
```

- `on_start`: Invoked once immediately before the initial presentation frame.
- `on_event`: Intercepts raw window and platform events before dispatching to the DOM.
- `on_frame`: Invoked after each layout resolution and display list compilation.
- `on_draw`: Allows low-level modification of raw rendering commands.
- `on_exit`: Executed during termination cleanup.

---

## Launch Modes

Strata provides three primary application initialization functions tailored for production, testing, and custom surface integration:

### 1. Interactive Desktop Launch (`launch_window`)
Spawns a native platform window backed by software or hardware rasterization:

```rust
let (runtime, window, presenter) = launch_window(
    TelemetryDashboard,
    WindowConfig {
        title: "Cluster Telemetry Monitor".into(),
        width: 1920,
        height: 1080,
        ..Default::default()
    },
);
```

### 2. Headless Layout & Simulation Engine (`launch_headless`)
Instantiates an `AppRuntime` without opening OS windows or allocating GPU swapchains. Ideal for integration testing, layout validation, server-side rendering, and headless CI verification:

```rust
let mut runtime = launch_headless(TelemetryDashboard, 1920.0, 1080.0);
assert_eq!(runtime.layout_root.geometry.border_box.width, 1920.0);
let commands = runtime.tick_frame();
assert!(!commands.commands.is_empty());
```

### 3. Default Managed Launch (`launch`)
Initializes standard viewport defaults (1280x800, DPR 1.0) and executes the initial compilation cycle:

```rust
fn main() {
    launch(TelemetryDashboard);
}
```

---

## Transition & Animation Runtime Orchestration

`AppRuntime` coordinates active CSS transitions, keyframes, scroll-driven animations, and view transitions across the DOM and layout trees:

### Transition & Animation Pipeline
1. **Tree Sync (`sync_tree_animations`)**: On view updates, compares current layout node styles against `previous_node_styles`. If an element is newly mounted or enters from `display: None`, initial transition states are seeded from `node.style.starting_style` (W3C `@starting-style`).
2. **Scroll-Driven Evaluation (`evaluate_scroll_animations`)**: Maps normalized container scroll offsets directly to keyframe animation progress with zero wall-clock timer overhead when stationary.
3. **View Transitions Manager (`ViewTransitionManager`)**:
   - Captures element snapshots (`capture_snapshot`) identified by `view-transition-name`.
   - On state transitions, morphs bounding boxes (`Rect::from_xywh`) and cross-fades opacity between previous and target states over an ease-in-out curve.
   - Cleans up active transitions on completion.
4. **Layout Tree Application (`apply_animations_to_layout_tree`)**: Recursively applies interpolated values onto `node.style`, with a final settling pass when animations finish to ensure exact target property values are restored.

---

## Production Cloud Telemetry Console Example

The following complete, runnable example constructs a cloud cluster observability panel with responsive layout, real-time node count tracking, and status metrics:

```rust
use strata::prelude::*;

#[component]
pub fn ClusterNodeCard(name: String, status: String, cpu_usage: f32) -> View {
    let status_color = if status == "healthy" {
        "text-emerald-400 bg-emerald-950/40 border-emerald-800"
    } else {
        "text-rose-400 bg-rose-950/40 border-rose-800"
    };

    rsx! {
        <div class="flex flex-col p-4 bg-neutral-900 border border-neutral-800 rounded-lg gap-2">
            <div class="flex items-center justify-between">
                <span class="font-mono text-sm text-neutral-200">{name}</span>
                <span class={format!("px-2 py-0.5 text-xs font-mono rounded border {}", status_color)}>
                    {status}
                </span>
            </div>
            <div class="flex items-center justify-between text-xs text-neutral-400">
                <span>CPU Load</span>
                <span class="font-mono">{format!("{:.1}%", cpu_usage)}</span>
            </div>
            <div class="w-full bg-neutral-800 h-1.5 rounded-full overflow-hidden">
                <div
                    class="bg-cyan-500 h-full"
                    style={format!("width: {}%;", cpu_usage.min(100.0))}
                />
            </div>
        </div>
    }
}

#[component]
pub fn TelemetryDashboard() -> View {
    let (node_count, set_node_count) = use_signal(|| 64);
    let (active_region, _set_active_region) = use_signal(|| "us-east-cluster-01".to_string());

    rsx! {
        <main class="flex flex-col w-full h-full bg-neutral-950 text-neutral-100 p-6 gap-6">
            <header class="flex items-center justify-between pb-4 border-b border-neutral-800">
                <div class="flex flex-col gap-1">
                    <h1 class="text-xl font-bold tracking-tight">Cloud Infrastructure Telemetry</h1>
                    <span class="text-xs font-mono text-neutral-400">Region: {active_region.get()}</span>
                </div>
                <div class="flex items-center gap-4">
                    <span class="text-sm font-mono text-neutral-300">Active Nodes: {node_count.get()}</span>
                    <button
                        class="px-3 py-1.5 bg-cyan-600 hover:bg-cyan-500 text-white text-xs font-medium rounded transition"
                        onclick={move || set_node_count.update(|c| *c += 1)}
                    >
                        Scale Up Node
                    </button>
                </div>
            </header>

            <section class="grid grid-cols-3 gap-4">
                <ClusterNodeCard name={"worker-pool-alpha-01".into()} status={"healthy".into()} cpu_usage={42.8} />
                <ClusterNodeCard name={"worker-pool-alpha-02".into()} status={"healthy".into()} cpu_usage={68.2} />
                <ClusterNodeCard name={"ingress-gateway-01".into()} status={"degraded".into()} cpu_usage={94.1} />
            </section>
        </main>
    }
}

fn main() {
    let mut app = strata::app()
        .with_window(strata::platform::WindowConfig {
            title: "Strata Infrastructure Telemetry".into(),
            width: 1280,
            height: 800,
            ..Default::default()
        })
        .build(TelemetryDashboard);

    app.run();
}
```
