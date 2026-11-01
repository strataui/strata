# strata_motion: Numerical Spring Physics & Animation Engine

The `strata_motion` crate delivers a pure-Rust, hardware-framerate animation pipeline for the Strata engine. Serving as a zero-dependency equivalent to Framer Motion and React Spring, `strata_motion` powers analytical harmonic spring oscillators, cubic Bezier tweening, reactive motion signals, and exit animation coordination (`AnimatePresence`) directly within Strata's retained-mode scene graph.

---

## Architectural Principles

Rather than relying on browser CSS transitions or timer thread polling, `strata_motion` integrates with Strata's 120 FPS frame render loop:

```text
┌─────────────────────────────────────────────────────────────┐
│                 Display VSync Frame Signal                  │
│                    (e.g., dt = 1/120s)                      │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                 MotionValue State Machine                   │
│        (Current Value, Target Value, Instantaneous Velocity)│
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│             Symplectic Euler Numerical Solver               │
│       F_spring = -k * x • F_damping = -c * v • a = F / m    │
│           v += a * dt • x += v * dt                         │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│             Direct GPU Property Transformation              │
│       (Opacity, Scale, TranslateX, TranslateY, Rotation)    │
└─────────────────────────────────────────────────────────────┘
```

1. **Analytical Physical Realism**: Animations are driven by physical parameters (mass, stiffness, and damping) instead of fixed artificial durations, ensuring fluid momentum conservation during sudden user interaction reversals.
2. **Symplectic Integration**: Differential equations are solved via semi-implicit (symplectic) Euler integration, preserving energy stability without high-order mathematical drift.
3. **Exit Animation Retention**: Unmounting nodes are retained in the layout tree until their exit physics simulations settle to rest.

---

## Harmonic Spring Dynamics: `SpringConfig` and `SpringState`

### The Physical Model

The oscillator solves the damped harmonic equation:

$$m \frac{d^2 x}{dt^2} + c \frac{dx}{dt} + k (x - x_{\text{target}}) = 0$$

Where:
- $m$: Mass of the moving element (`mass`)
- $k$: Spring stiffness coefficient (`stiffness`)
- $c$: Velocity damping friction coefficient (`damping`)
- $x - x_{\text{target}}$: Displacement error from rest target

```rust
use strata_motion::prelude::{SpringConfig, SpringState};

// Default balanced configuration
let config = SpringConfig::default();
assert_eq!(config.stiffness, 170.0);
assert_eq!(config.damping, 26.0);
assert_eq!(config.mass, 1.0);

// Instantiate oscillator at initial coordinate 0.0
let mut state = SpringState::new(0.0);
state.target = 100.0; // Animate towards 100.0px

// Advance simulation by 16.6ms (60 FPS step)
let settled = state.step(1.0 / 60.0, &config);
assert!(!settled);
assert!(state.current > 0.0);
assert!(state.velocity > 0.0);
```

#### Spring Presets

| Preset | Stiffness ($k$) | Damping ($c$) | Mass ($m$) | Dynamic Character |
| :--- | :--- | :--- | :--- | :--- |
| `SpringConfig::default()` | `170.0` | `26.0` | `1.0` | Balanced UI transition with subtle organic overshoot. |
| `SpringConfig::gentle()` | `120.0` | `14.0` | `1.0` | Soft, cushioned movement with natural deceleration. |
| `SpringConfig::wobbly()` | `180.0` | `12.0` | `1.0` | High-energy bouncing oscillation for alerts and counters. |
| `SpringConfig::stiff()` | `210.0` | `20.0` | `1.0` | Snappy, precise response for tabs, buttons, and switches. |
| `SpringConfig::slow()` | `280.0` | `60.0` | `1.0` | Critically damped, zero-overshoot slide for drawer sheets. |

---

## Reactive Motion Values: `MotionValue`

`MotionValue` wraps spring state in Strata reactive signals, automatically notifying dependent layout nodes during frame ticks:

```rust
use strata_motion::prelude::use_motion_value;

let drawer_x = use_motion_value(-320.0); // Hidden off-screen left

// Trigger animation towards target
drawer_x.animate_to(0.0);
assert!(drawer_x.is_animating.get());
assert_eq!(drawer_x.target.get(), 0.0);

// Tick loop (executed in Strata VSync pump)
let dt = 1.0 / 120.0;
while !drawer_x.tick(dt) {
    // drawer_x.value.get() updates continuously
}

assert_eq!(drawer_x.value.get(), 0.0);
assert!(!drawer_x.is_animating.get());

// Immediate jump without animation
drawer_x.jump_to(-320.0);
assert_eq!(drawer_x.value.get(), -320.0);
```

#### API Specification: `MotionValue`

| Member | Type | Description |
| :--- | :--- | :--- |
| `value` | `Signal<f32>` | Current instantaneous value updated every frame tick. |
| `velocity` | `Signal<f32>` | Instantaneous velocity vector in pixels per second. |
| `target` | `Signal<f32>` | Destination coordinate or value. |
| `is_animating` | `Signal<bool>` | Flag indicating if oscillator has yet to settle at rest target. |
| `spring_config` | `SpringConfig` | Mass, stiffness, and damping tuning parameters. |
| `animate_to(target)` | `fn(f32)` | Updates target and engages the frame oscillator loop. |
| `jump_to(value)` | `fn(f32)` | Instantly snaps position and zeroes velocity without animating. |
| `tick(dt)` | `fn(f32) -> bool` | Advances physics by delta time `dt`; returns `true` upon settling. |

---

## Keyframe Easing and Tween Interpolation

For deterministic, time-bounded transitions that do not require physical spring momentum, `Easing` provides parametric easing:

```rust
use strata_motion::prelude::{interpolate, Easing};

// Evaluate normalized progress [0.0, 1.0]
let progress = 0.5;
let eased = Easing::EaseInOut.evaluate(progress);

// Interpolate opacity or coordinates
let opacity = interpolate(0.0, 1.0, 0.5, Easing::EaseOut);
assert_eq!(opacity, 0.875);

// Custom cubic Bezier curve (equivalent to CSS cubic-bezier(0.16, 1.0, 0.3, 1.0))
let fluid_curve = Easing::CubicBezier(0.16, 1.0, 0.3, 1.0);
let custom_pos = interpolate(100.0, 500.0, 0.25, fluid_curve);
```

#### Supported Easing Modes

- `Easing::Linear`: Uniform constant velocity ($f(t) = t$).
- `Easing::EaseIn`: Cubic acceleration ($f(t) = t^3$).
- `Easing::EaseOut`: Cubic deceleration ($f(t) = 1 - (1-t)^3$).
- `Easing::EaseInOut`: Symmetric cubic s-curve acceleration and deceleration.
- `Easing::CubicBezier(x1, y1, x2, y2)`: Arbitrary four-point Bezier polynomial solver.

---

## Exit Lifecycle Coordination: `AnimatePresenceState`

In declarative UI architectures, removing an element from a dataset abruptly deletes it from the DOM, making exit transitions impossible. `AnimatePresenceState` intercepts removals, keeping exiting nodes in the render tree until exit animations conclude:

```rust
use strata_motion::prelude::AnimatePresenceState;

let presence = AnimatePresenceState::new();

// Initial state: 2 active telemetry cards
presence.sync_keys(&["card-node-01".to_string(), "card-node-02".to_string()]);
assert_eq!(presence.items.get().len(), 2);
assert!(!presence.is_exiting("card-node-02"));

// Remove card-node-02: Retained in tree but marked as exiting
presence.sync_keys(&["card-node-01".to_string()]);
assert_eq!(presence.items.get().len(), 2);
assert!(presence.is_exiting("card-node-02")); // Exit transition is playing!

// Frame callback when exit animation finishes
presence.on_exit_complete("card-node-02");

// Now safely pruned from tree
assert_eq!(presence.items.get().len(), 1);
assert_eq!(presence.items.get()[0].key, "card-node-01");
```

---

## Production Component Example: Spring-Animated Telemetry Drawer

```rust
use strata_dom::prelude::*;
use strata_motion::prelude::*;

#[component]
pub fn AnimatedTelemetryDrawer(is_open: bool, children: Children) -> View {
    // 320px wide drawer starting off-screen
    let offset_x = use_motion_value(-320.0);

    // Reactively animate when is_open toggles
    if is_open {
        offset_x.animate_to(0.0);
    } else {
        offset_x.animate_to(-320.0);
    }

    let current_x = offset_x.value.get();

    rsx! {
        <div class="fixed inset-0 pointer-events-none z-50">
            // Backdrop with spring opacity
            <div
                class="absolute inset-0 bg-black/60 backdrop-blur-sm pointer-events-auto transition-opacity"
                class={if is_open { "opacity-100" } else { "opacity-0 pointer-events-none" }}
            />

            // Physical Drawer Sheet
            <aside
                class="absolute top-0 bottom-0 left-0 w-80 bg-neutral-900 border-r border-neutral-800 p-6 pointer-events-auto shadow-2xl font-mono"
                style="transform: translateX({current_x}px);"
            >
                <header class="flex items-center justify-between pb-4 border-b border-neutral-800 text-sm font-bold text-neutral-200">
                    <span>"Node Deep Inspection"</span>
                </header>
                <div class="mt-4 text-xs text-neutral-400">
                    {children}
                </div>
            </aside>
        </div>
    }
}
```
