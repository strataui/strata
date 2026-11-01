# strata_sim: Standalone Virtual Smartphone OS Simulator

The `strata_sim` crate provides a standalone virtual smartphone operating system and interactive mobile hardware simulator written entirely in safe, idiomatic Rust. It dogfoods Strata's own layout, rendering, reactive signals, and 3D GPU pipelines to simulate an entire modern mobile environment directly on the developer desktop.

Developers can test mobile user experiences, edge gestures, dynamic cutouts, safe-area insets, foldable postures, and low-level hardware sensor changes with sub-millisecond hot reloading and zero emulator boot overhead.

---

## Architectural Topology

```text
┌────────────────────────────────────────────────────────────────────────┐
│                      MobileSimulatorWorkspace                          │
├────────────────────────────────────────────────────────────────────────┤
│                                                                        │
│   ┌────────────────────┐   ┌───────────────────┐   ┌───────────────┐   │
│   │   ChassisRenderer  │   │  VirtualHalBus    │   │ Profile Engine│   │
│   │ (3D Bezel & Glass) │   │ (Sensors & Radios)│   │ (Hardware Dims│   │
│   └─────────┬──────────┘   └─────────┬─────────┘   └───────┬───────┘   │
│             │                        │                     │           │
│             ▼                        ▼                     ▼           │
│   ┌────────────────────────────────────────────────────────────────┐   │
│   │                    Simulated Display Panel                     │   │
│   │                                                                │   │
│   │   ┌────────────────────────────────────────────────────────┐   │   │
│   │   │                      Status Bar                        │   │   │
│   │   │  [Time]       [Dynamic Island / Cutout]     [Battery]  │   │   │
│   │   ├────────────────────────────────────────────────────────┤   │   │
│   │   │                                                        │   │   │
│   │   │                   MultiAppSandbox                      │   │   │
│   │   │       (Active Application View / Recents Deck)         │   │   │
│   │   │                                                        │   │   │
│   │   ├────────────────────────────────────────────────────────┤   │   │
│   │   │                Navigation Interaction Subsystem        │   │   │
│   │   │         [Gesture Pill Bar / 3-Button Nav Deck]         │   │   │
│   │   └────────────────────────────────────────────────────────┘   │   │
│   │                                                                │   │
│   └────────────────────────────────────────────────────────────────┘   │
│                                                                        │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 1. Device Profile Engine (`profile.rs`)

The simulator accurately replicates the physical dimensions, bezel curvature, material aesthetics, and subpixel screen panels of commercial mobile hardware.

### Hardware Chassis Specs (`ChassisProfile`)

Models the mechanical outer casing surrounding the display:

```rust
pub struct ChassisProfile {
    pub outer_width: f32,
    pub outer_height: f32,
    pub outer_depth: f32,
    pub corner_radius: f32,
    pub bezel_width: f32,
    pub material: ChassisMaterial,
    pub physical_buttons: Vec<PhysicalButton>,
}
```

Supported enclosure materials include `TitaniumNatural`, `AluminumMidnight`, `CeramicShieldWhite`, and `MatteGlassObsidian`, each modulating specular highlights and ambient reflections in the 3D viewport.

### Screen Metrics & Subpixel Panel Specifications (`ScreenProfile`)

Models display geometry and hardware characteristics:

- **Logical Viewport**: Dimensions in density-independent pixels (e.g., $440 \times 956$ pt)
- **Physical Resolution**: Hardware raster grid (e.g., $1320 \times 2868$ px)
- **Device Pixel Ratio (DPR)**: Rendering scale multiplier (typically $2.0\times$ or $3.0\times$)
- **Peak Luminance**: Calibrated in cd/m^2 (nits) for HDR content simulation
- **Display Refresh Rate**: Dynamically toggleable between 60 Hz and 120 Hz ProMotion variable refresh rates

### Hardware Occlusion Profiles (`CutoutProfile`)

Accurately projects hardware camera cutouts into the display viewport:

```rust
pub enum CutoutProfile {
    None,
    DynamicIsland { resting_width: f32, resting_height: f32, corner_radius: f32 },
    PunchHoleCenter { diameter: f32, top_offset: f32 },
    PunchHoleLeft { diameter: f32, top_offset: f32, left_offset: f32 },
    WaterdropNotch { width: f32, height: f32 },
    UnderDisplayCamera,
    RectangularNotch { width: f32, height: f32, corner_radius: f32 },
}
```

### Factory Device Presets

Out of the box, `DeviceProfile` includes factory-calibrated specifications:
- `DeviceProfile::iphone_16_pro_max()`: Titanium Natural, Dynamic Island, 120Hz ProMotion, $440 \times 956$ pt.
- `DeviceProfile::pixel_9_pro_fold()`: Book-style folding hinge, dual screens ($412 \times 924$ cover, $840 \times 840$ inner).
- `DeviceProfile::galaxy_z_flip_6()`: Clamshell folding hinge, flex mode posture support.
- `DeviceProfile::ipad_pro_13_m4()`: Ultra-thin aluminum chassis, tandem OLED panel.

---

## 2. Virtual Hardware Abstraction Layer (`VirtualHalBus`)

The `VirtualHalBus` simulates device internal electronics, radio interfaces, and physical environmental telemetry without hardware dependencies.

### Power & Thermal Telemetry

Simulates battery discharging dynamics, USB-PD fast charging (up to 120W), Qi2 wireless inductive coupling, and OS thermal mitigation states:

```rust
pub enum ThermalState {
    Nominal,    // Unthrottled operation
    Fair,       // Slight skin warming, unthrottled CPU
    Serious,    // Frame rate throttled to 60Hz, display dimmed 15%
    Critical,   // Heavy CPU throttling, background tasks suspended
    Emergency,  // Emergency cooling mode, radios disabled
}
```

### Sensors and Geolocation

Simulates hardware sensor buses updating at up to 100 Hz:
- **3-Axis Accelerometer & Gyroscope**: Emulates gravitational vectors ($\vec{g} = [g_x, g_y, g_z]$) and rotational angular velocity.
- **Barometer & Ambient Light Sensor**: Atmospheric pressure in hPa and illumination in lux for automatic dark-mode transitions.
- **GPS Coordinates**: WGS-84 latitude, longitude, altitude, and heading angle vectors.

### Radios & Network Simulation

Modulates network connectivity profiles to test application behavior under adverse cellular conditions:
- **Modem Modes**: `NR5G_SA` (Standalone 5G), `NR5G_NSA`, `LTE_Advanced`, `HSPA`, `Offline`.
- **Packet Latency & Loss**: Injects configurable artificial packet jitter ($0\text{--}500\text{ ms}$) and dropped frames.

---

## 3. Dynamic Island Spring-Damper Oscillator (`island.rs`)

The Dynamic Island simulation avoids rigid CSS transitions in favor of physical harmonic oscillator differential equations:

$$m \ddot{x} + c \dot{x} + k (x - x_{\text{target}}) = 0$$

Where:
- $m$: Simulated mass of the fluid pill element
- $k$: Spring stiffness coefficient
- $c$: Critical damping factor ($c = 2 \sqrt{m k}$) preventing unnatural oscillating ringing

```rust
pub struct DynamicIslandOscillator {
    pub current_width: f32,
    pub target_width: f32,
    pub velocity_width: f32,
    pub current_height: f32,
    pub target_height: f32,
    pub velocity_height: f32,
    pub state: DynamicIslandState,
}
```

### Dynamic Notification Expansion

Transitions seamlessly between operational states:
1. **Resting**: Compact pill enclosing front-facing sensors ($125 \times 36$ pt)
2. **CompactLeadingTrailing**: Split icon on left, audio waveform or timer on right
3. **ExpandedInteractive**: Large card ($360 \times 160$ pt) presenting interactive audio controls, incoming call actions, or cloud deployment status alerts.

---

## 4. Navigation Subsystems (`navigation.rs`)

The simulator incorporates the dominant mobile navigation paradigms:

### Three-Button Navigation Deck

Simulates legacy and enterprise Android navigation:
- `Back`: Triggers view pop and navigation stack unwinding
- `Home`: Returns immediately to the simulated home launcher
- `Recents`: Activates the multi-tasking card stack carousel

### Predictive Back Gesture Engine (`GestureNavEngine`)

Implements modern edge gesture detection conforming to Android 14+ and iOS specifications:
- Tracks subpixel touch trajectories beginning within the $24\text{ pt}$ edge activation zone.
- Computes progress ratio $p \in [0.0, 1.0]$ based on displacement vector $\Delta x$.
- Scales and curves the underlying view with cubic bezier physics before confirming transition or snapping back.

---

## 5. Application Sandboxing & Lifecycle (`sandbox.rs`)

The `MultiAppSandbox` manages concurrent sandboxed applications within the simulated mobile OS:

```rust
pub enum AppLifecycleState {
    Unloaded,
    Prewarming,
    ForegroundActive,
    BackgroundSuspended,
    Terminated,
}
```

- **Process Memory Limits**: Enforces mobile memory budgets (e.g. 512 MB per app), signaling memory pressure warnings when thresholds are approached.
- **Multitasking Carousel (`RecentsCarouselState`)**: Generates 3D perspective cards of backgrounded applications, supporting swipe-up termination and instant task switching.

---

## 6. Real-Time 3D Chassis Renderer (`chassis.rs`)

The device chassis is rendered as a 3D perspective mesh using Strata's `<Canvas3D>`:

```rust
pub struct Chassis3DTransform {
    pub pitch_deg: f32, // X-axis tilt
    pub yaw_deg: f32,   // Y-axis tilt
    pub roll_deg: f32,  // Z-axis tilt
    pub zoom: f32,
}
```

Interactive mouse hover over the desktop window gently tilts the chassis in 3D space, casting dynamic shadows and specular glints across the metal edges to inspect bezel clearances and glass reflections.

---

## Standalone Usage

Launch the interactive mobile simulator directly from the workspace:

```bash
# Launch the simulator workspace
cargo run --release -p strata_sim

# Execute simulator automated unit and interaction tests
cargo test -p strata_sim
```
