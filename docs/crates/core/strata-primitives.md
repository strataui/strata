# Crate `strata_primitives`: Foundation Primitives & Memory Architecture

The `strata_primitives` crate provides zero-dependency, cache-optimized mathematical, geometric, colorimetric, string interning, and memory allocation primitives for Project Strata.

Designed for high-throughput UI layout and rendering pipelines, `strata_primitives` eliminates runtime allocations from the hot path through contiguous bump arenas and fixed-point coordinates.

---

## Architecture & Subsystems

```text
┌────────────────────────────────────────────────────────────────────────┐
│                          strata_primitives                             │
└───────┬──────────────────┬───────────────────┬──────────────────┬──────┘
        │                  │                   │                  │
        ▼                  ▼                   ▼                  ▼
┌──────────────┐   ┌───────────────┐   ┌───────────────┐   ┌──────────────┐
│  arena.rs    │   │   atom.rs     │   │   color.rs    │   │   geom.rs    │
│  BumpArena   │   │  AtomTable    │   │  Rgba, Hsla   │   │ Point, Size  │
│  FrameManager│   │  AtomId       │   │  Oklch        │   │ Rect, Insets │
│  ThreadLocal │   │  Static Attr  │   │  WCAG Engine  │   │ LayoutUnit   │
└──────────────┘   └───────────────┘   └───────────────┘   └──────────────┘
```

---

## High-Performance Memory Allocators (`arena`)

Standard dynamic allocators (`malloc`, `jemalloc`) introduce pointer chasing, heap fragmentation, and synchronization overhead. `strata_primitives::arena` implements linear bump allocation with bulk single-cycle resets.

### 1. `BumpArena`
A contiguous, non-fragmenting block of virtual memory with SIMD sentry buffers:

```rust
pub const DEFAULT_ARENA_CAPACITY: usize = 1024 * 1024; // 1 MB
pub const SIMD_SENTRY_BYTES: usize = 32;               // AVX2 / NEON boundary safety

pub struct BumpArena {
    memory: NonNull<u8>,
    capacity: usize,
    offset: UnsafeCell<usize>,
}
```

#### Key Mechanics:
- **SIMD Sentry Padding**: Allocates an extra 32 trailing zero-bytes beyond the nominal capacity. When AVX2 (256-bit) or NEON (128-bit) vectorized loops scan memory, this sentry buffer prevents reading past allocated virtual memory pages.
- **Cache-Line Alignment**: The underlying block is aligned to 64 bytes (`Layout::from_size_align(total, 64)`).
- **Single-Cycle Reset**: Invoking `reset()` instantly returns `offset` to 0 without executing individual element drop destructors, enabling complete UI reconciliation with $O(1)$ overhead.

```rust
use strata_primitives::arena::BumpArena;

let arena = BumpArena::new(1024 * 1024);

// Sequential allocation of UI nodes
let buffer = arena.alloc_slice(&[10u32, 20, 30, 40]);
let text = arena.alloc_str("cluster-worker-pod-99");

assert_eq!(buffer.len(), 4);
assert_eq!(text, "cluster-worker-pod-99");

// Reset entire arena in 1 instruction
arena.reset();
assert_eq!(arena.allocated_bytes(), 0);
```

### 2. Double-Buffered `FrameArenaManager`
For retained UI rendering across successive frames, `FrameArenaManager` coordinates two `BumpArena` buffers in ping-pong succession:

```rust
pub struct FrameArenaManager {
    arenas: [BumpArena; 2],
    current_index: usize,
    frame_epoch: u64,
}
```

```text
 Frame N:
 ┌──────────────────────┐   ┌──────────────────────┐
 │  Active Arena (0)    │   │ Previous Arena (1)   │
 │  (Frame allocations) │   │ (Retained display)   │
 └──────────────────────┘   └──────────────────────┘
                  │
                  ▼ swap_and_reset()
 Frame N + 1:
 ┌──────────────────────┐   ┌──────────────────────┐
 │ Previous Arena (0)   │   │  Active Arena (1)    │
 │ (Retained display)   │   │  (Reset to offset 0) │
 └──────────────────────┘   └──────────────────────┘
```

- **`active_arena()`**: Points to the buffer currently receiving UI element allocations.
- **`previous_arena()`**: Holds references to display list commands submitted for GPU rendering during the previous frame.
- **`swap_and_reset()`**: Swaps indices, increments `frame_epoch`, and resets the newly active buffer to zero offset in $O(1)$ time.

### 3. `ThreadLocalArena`
A dedicated 64-byte aligned arena designed for multi-threaded worker pools:

```rust
#[repr(align(64))]
pub struct ThreadLocalArena {
    start: *mut u8,
    ptr: *mut u8,
    end: *mut u8,
    capacity: usize,
}
```

---

## Lock-Free String Interning (`atom`)

During HTML and CSS parsing, string duplication of tag names (`div`, `span`) and property keys (`display`, `margin`, `color`) severely degrades throughput. `AtomTable` implements string interning mapping strings to 32-bit identifiers.

### `AtomId` Representation
```rust
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AtomId(pub u32);
```

### Pre-Baked Static Partitioning
To guarantee zero-lock read overhead for common HTML and CSS keywords, the `AtomId` space is partitioned:
- **Indices `1 ..= 1023`**: Pre-baked static atoms compiled directly into the binary.
  - HTML Tags (`AtomId::DIV = 1`, `AtomId::SPAN = 2`, `AtomId::BUTTON = 6`, `AtomId::MAIN = 29`, etc.).
  - CSS Properties (`AtomId::DISPLAY = 101`, `AtomId::POSITION = 102`, `AtomId::WIDTH = 103`, `AtomId::MARGIN = 109`, etc.).
- **Indices `1024+`**: Dynamically interned runtime identifiers.

```rust
use strata_primitives::atom::{AtomId, AtomTable};

let table = AtomTable::new();

// Static lookup requires no dynamic hashing or write locking
assert!(AtomId::DIV.is_static());
assert_eq!(table.resolve(AtomId::DIV), Some("div".to_string()));

// Dynamic strings are assigned sequential IDs >= 1024
let dynamic_id = table.intern("custom-telemetry-gauge");
assert!(!dynamic_id.is_static());
assert_eq!(table.resolve(dynamic_id), Some("custom-telemetry-gauge".to_string()));
```

---

## Color Science & Contrast Engine (`color`)

The color subsystem implements exact conversion pipelines across sRGB, HSLA, and perceptually uniform OKLCH spaces, along with WCAG 2.1 contrast calculations.

### 1. `Rgba`
A compact 4-byte color representation:

```rust
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}
```

#### Parsing Hex Strings (Zero Heap Allocation):
Parses `#RGB`, `#RGBA`, `#RRGGBB`, and `#RRGGBBAA` formats without allocating a `String`:

```rust
let red = Rgba::parse_hex("#ff0000").unwrap();
let semi_blue = Rgba::parse_hex("#0000ff80").unwrap();
assert_eq!(red, Rgba::new(255, 0, 0, 255));
```

#### Porter-Duff Source-Over Alpha Composite:
Computes true mathematical compositing over background surfaces:

$$\alpha_{\text{out}} = \alpha_{\text{src}} + \alpha_{\text{dst}} \cdot (1 - \alpha_{\text{src}})$$
$$C_{\text{out}} = \frac{C_{\text{src}} \alpha_{\text{src}} + C_{\text{dst}} \alpha_{\text{dst}} (1 - \alpha_{\text{src}})}{\alpha_{\text{out}}}$$

```rust
let foreground = Rgba::new(255, 255, 255, 128); // 50% white
let background = Rgba::new(0, 0, 0, 255);       // Solid black
let composite = foreground.blend_over(background);
assert_eq!(composite.r, 128);
assert_eq!(composite.a, 255);
```

#### WCAG 2.1 Contrast Calculations:
Evaluates relative luminance via standard sRGB gamma linearization:

$$L = 0.2126 \cdot R_{\text{linear}} + 0.7152 \cdot G_{\text{linear}} + 0.0722 \cdot B_{\text{linear}}$$
$$\text{Contrast Ratio} = \frac{L_{\text{lighter}} + 0.05}{L_{\text{darker}} + 0.05}$$

```rust
let white = Rgba::WHITE;
let dark_bg = Rgba::new(24, 24, 27, 255); // neutral-900
let ratio = white.contrast_ratio(dark_bg);
assert!(ratio >= 12.0); // Conforms to AAA accessibility standards (> 7:1)
```

### 2. `Oklch` Perceptually Uniform Model
Implements CSS Color Module Level 4 `oklch(L C H / A)`:
- `l`: Perceptual Lightness ($0.0 \dots 1.0$)
- `c`: Chroma ($0.0 \dots \approx 0.4$)
- `h`: Hue angle in degrees ($0.0 \dots 360.0^\circ$)
- `a`: Alpha channel ($0.0 \dots 1.0$)

```rust
use strata_primitives::color::Oklch;

let teal = Oklch::new(0.75, 0.14, 180.0, 1.0);
let rgba = teal.to_rgba();
```

---

## Subpixel Geometry & Blink `LayoutUnit` (`geom`)

To achieve layout parity with Chromium Blink, `strata_primitives` models physical and logical layout bounds with subpixel fixed-point arithmetic.

### 1. `LayoutUnit` (1/64th Pixel Fixed-Point Scalar)
Floating-point coordinates suffer from non-associative rounding errors across deep layout hierarchies. Following Chromium Blink's `LayoutUnit` specification, Strata represents coordinates as 32-bit signed integers scaled by 64 ($2^6$):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct LayoutUnit(pub i32);

impl LayoutUnit {
    pub const SCALE: f32 = 64.0;
    
    pub fn from_f32(val: f32) -> Self {
        Self((val * Self::SCALE).round() as i32)
    }

    pub fn to_f32(self) -> f32 {
        self.0 as f32 / Self::SCALE
    }
}
```

#### Subpixel Snapping Rules:
- **Device Grid Snapping (`snap_to_device_grid`)**:
  Snaps a logical layout unit to the nearest physical device pixel boundary given a Device Pixel Ratio (DPR):
  $$\text{snapped} = \frac{\text{round}(\text{logical} \cdot \text{DPR})}{\text{DPR}}$$
- **Hairline Border Snapping (`snap_border`)**:
  Guarantees that thin structural borders never snap to 0 physical pixels on high-DPI displays:
  ```rust
  let hairline = LayoutUnit::from_f32(0.2);
  assert_eq!(hairline.snap_border(1.0), 1.0); // 1.0 physical pixel on 1x DPR
  assert_eq!(hairline.snap_border(2.0), 0.5); // 0.5 logical pixels (1.0 physical) on 2x DPR
  ```

### 2. Geometry Structs: `Point`, `Size`, `Rect`, `EdgeInsets`
Generic over scalar coordinates (`f32`, `LayoutUnit`):

```rust
use strata_primitives::geom::{Point, Rect, EdgeInsets};

let r1 = Rect::from_xywh(0.0, 0.0, 100.0, 100.0);
let r2 = Rect::from_xywh(50.0, 50.0, 100.0, 100.0);

assert!(r1.intersects(r2));
let intersection = r1.intersection(r2).unwrap();
assert_eq!(intersection, Rect::from_xywh(50.0, 50.0, 50.0, 50.0));

let deflated = r1.deflate(EdgeInsets::all(10.0));
assert_eq!(deflated, Rect::from_xywh(10.0, 10.0, 80.0, 80.0));
```

### 3. Affine Transforms: `Transform2D`
Implements standard W3C 2D affine transformation matrices:

$$\begin{bmatrix} x' \\ y' \\ 1 \end{bmatrix} = \begin{bmatrix} a & c & tx \\ b & d & ty \\ 0 & 0 & 1 \end{bmatrix} \begin{bmatrix} x \\ y \\ 1 \end{bmatrix}$$

Includes matrix concatenation (`multiply`), determinant calculation (`determinant`), and non-singular matrix inversion (`inverse`).

---

## Production Cloud Telemetry Monitor Example

This example demonstrates zero-allocation parsing of cloud metric telemetry streams using `BumpArena`, static atom matching, and fixed-point subpixel snapping:

```rust
use strata_primitives::prelude::*;

pub struct TelemetryDataPoint<'a> {
    pub metric_name: &'a str,
    pub atom: AtomId,
    pub timestamp_ms: u64,
    pub value_snapped: LayoutUnit,
    pub status_color: Rgba,
}

pub fn ingest_telemetry_batch<'a>(
    arena: &'a BumpArena,
    atom_table: &AtomTable,
    raw_payloads: &[(&str, f32)],
    dpr: f32,
) -> &'a [TelemetryDataPoint<'a>] {
    let mut points = Vec::with_capacity(raw_payloads.len());

    for (name, val) in raw_payloads {
        // Linear bump allocation of metric label string
        let interned_name = arena.alloc_str(name);
        let atom = atom_table.intern(interned_name);

        // Fixed-point subpixel rounding according to hardware DPR
        let raw_unit = LayoutUnit::from_f32(*val);
        let snapped_f32 = raw_unit.snap_to_device_grid(dpr);
        let value_snapped = LayoutUnit::from_f32(snapped_f32);

        // Perceptually uniform alert coloring
        let status_color = if *val > 90.0 {
            Oklch::new(0.65, 0.22, 25.0, 1.0).to_rgba() // Critical alert red
        } else if *val > 70.0 {
            Oklch::new(0.80, 0.16, 85.0, 1.0).to_rgba() // Warning yellow
        } else {
            Oklch::new(0.75, 0.18, 145.0, 1.0).to_rgba() // Nominal green
        };

        points.push(TelemetryDataPoint {
            metric_name: interned_name,
            atom,
            timestamp_ms: 1730000000000,
            value_snapped,
            status_color,
        });
    }

    // Allocate continuous contiguous slice in bump arena
    arena.alloc_slice(&points)
}
```
