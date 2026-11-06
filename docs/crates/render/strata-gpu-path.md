# strata_gpu_path

`strata_gpu_path` is the high-performance analytical vector path and GPU curve rasterization engine of the Strata framework. Engineered to meet and exceed the mathematical precision of Slug and Vello, it evaluates quadratic and cubic Bézier curves directly in GPU fragment shaders using analytical polynomial root-finding, completely bypassing CPU tessellation and font texture atlases.

---

## Architectural Mission

Traditional 2D vector graphics engines (such as Skia, Direct2D, and Cairo) suffer from two fundamental bottlenecks:

1. **CPU Polygon Tessellation**: Paths are flattened into thousands of tiny triangles on the CPU. At high zoom factors or during high-frequency telemetry updates, tessellation overwhelms CPU compute cores, limits framerates, and consumes megabytes of vertex buffer memory.
2. **Pre-Rasterized Texture Atlases**: Text glyphs and icons are rasterized into bitmap texture sheets. Zooming causes visible blurriness or requires expensive atlas rebuilds, producing perceptible frame drops.

`strata_gpu_path` solves both challenges via **Analytical GPU Curve Evaluation**:
- **Compact Curve Representation**: Vector paths are uploaded to GPU storage buffers as compact mathematical polynomials (32 bytes per curve segment).
- **Exact Polynomial Root-Finding**: Distance to curve boundaries is evaluated analytically inside GPU fragment shaders using closed-form cubic roots.
- **Infinite Resolution**: Glyphs and telemetry curves retain pixel-perfect, mathematically sharp edges at $1\times$, $10\times$, or $1000\times$ zoom levels with zero memory overhead.

---

## Subsystem Architecture

```text
┌─────────────────────────────────────────────────────────────────┐
│                    Path & Geometry Definition                   │
│         (PathBuilder, PathVerb: MoveTo, LineTo, QuadTo, CubicTo)│
└────────────────────────────────┬────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────┐
│              Cubic-to-Quadratic Subdivider                      │
│   - Midpoint Error Estimation (tolerance epsilon)               │
│   - Recursive De Casteljau Subdivision                          │
└────────────────────────────────┬────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────┐
│              PathCommandBuffer & Instance Packing               │
│   - 32-Byte GpuCurveInstance (P0, P1, P2, Flags, Z-Index)       │
│   - Winding Number & Fill Rule Encoding (NonZero, EvenOdd)      │
└────────────────┬───────────────────────────────┬────────────────┘
                 │                               │
                 ▼                               ▼
┌─────────────────────────────────┐   ┌───────────────────────────┐
│       AnalyticGlyphCache        │   │    Analytical SDF Shader  │
│   - Vector Glyph Contours       │   │   - Exact Distance to Quad│
│   - Advance Widths & Extents    │   │   - Depressed Cubic Roots │
│   - Zero Texture Atlasing       │   │   - Subpixel Anti-Aliasing│
└─────────────────────────────────┘   └───────────────────────────┘
```

---

## Parametric Curve Primitives

### Quadratic Bézier Curves

A quadratic Bézier curve is defined by three points: start point $P_0$, control point $P_1$, and end point $P_2$:

$$B(t) = (1 - t)^2 P_0 + 2t(1 - t) P_1 + t^2 P_2, \quad t \in [0, 1]$$

In polynomial coefficient form:

$$B(t) = \mathbf{a} t^2 + \mathbf{b} t + \mathbf{c}$$

$$\mathbf{a} = P_0 - 2P_1 + P_2$$

$$\mathbf{b} = 2(P_1 - P_0)$$

$$\mathbf{c} = P_0$$

### Cubic Bézier Curves and Adaptive Subdivision

While quadratic curves are ideal for hardware evaluation, graphic designers and font designers author curves using cubic Béziers (four control points: $P_0, P_1, P_2, P_3$).

To achieve optimal GPU fragment shader performance, `strata_gpu_path` adaptively decomposes cubic Béziers into a sequence of quadratic curves using recursive De Casteljau subdivision:

```rust
use strata_gpu_path::curve::{CubicBezier, Point2D, QuadraticBezier};

let cubic = CubicBezier::new(
    Point2D::new(0.0, 0.0),
    Point2D::new(30.0, 90.0),
    Point2D::new(70.0, 90.0),
    Point2D::new(100.0, 0.0),
);

// Subdivide into quadratics within a 0.25px geometric tolerance
let tolerance = 0.25;
let quadratics: Vec<QuadraticBezier> = cubic.to_quadratics(tolerance);

println!("Decomposed cubic into {} GPU-evaluable quadratic segments.", quadratics.len());
```

The algorithm approximates the cubic midpoint with a candidate quadratic control point:

$$C = \frac{3 P_1 - P_0 + 3 P_2 - P_3}{4}$$

If the distance between $B_{\text{cubic}}(0.5)$ and $B_{\text{quad}}(0.5)$ exceeds the tolerance threshold, the curve is split in half and evaluated recursively.

---

## Exact Polynomial Root-Finding and SDF Mathematics

To compute the shortest distance from an arbitrary fragment coordinate $P = (p_x, p_y)$ to a quadratic Bézier curve $B(t)$, we minimize the squared distance:

$$D(t) = \| B(t) - P \|^2 = (a_x t^2 + b_x t + c_x)^2 + (a_y t^2 + b_y t + c_y)^2$$

Setting the derivative $\frac{d}{dt} D(t) = 0$ yields a **cubic polynomial** in $t$:

$$k_3 t^3 + k_2 t^2 + k_1 t + k_0 = 0$$

Where:
- $k_3 = 2 (\mathbf{a} \cdot \mathbf{a})$
- $k_2 = 3 (\mathbf{a} \cdot \mathbf{b})$
- $k_1 = 2 (\mathbf{a} \cdot \mathbf{c}) + (\mathbf{b} \cdot \mathbf{b})$
- $k_0 = \mathbf{b} \cdot \mathbf{c}$
- $\mathbf{c} = P_0 - P$

### Depressed Cubic Solver (`solve_cubic_roots`)

`strata_gpu_path::sdf::solve_cubic_roots` implements the Cardano-Viète algebraic method. By substituting $t = x - \frac{b}{3a}$, the equation transforms into a depressed cubic:

$$x^3 + px + q = 0$$

$$p = \frac{3ac - b^2}{3a^2}, \quad q = \frac{2b^3 - 9abc + 27a^2d}{27a^3}$$

The discriminant $\Delta = \frac{q^2}{4} + \frac{p^3}{27}$ governs root behavior:
1. **$\Delta > 0$**: Exactly one real root. Solved via Cardano's hyperbolic formula.
2. **$\Delta < 0$**: Three distinct real roots (*Casus irreducibilis*). Solved using Viète's trigonometric formulation:

$$r = \sqrt{-\frac{p^3}{27}}, \quad \phi = \arccos\left(-\frac{q}{2r}\right)$$

$$x_k = 2 \sqrt{-\frac{p}{3}} \cos\left(\frac{\phi + 2k\pi}{3}\right), \quad k \in \{0, 1, 2\}$$

Roots falling within $t \in [0.0, 1.0]$ are evaluated alongside boundary endpoints $t = 0.0$ and $t = 1.0$ to determine the true Euclidean distance:

```rust
use strata_gpu_path::curve::{Point2D, QuadraticBezier};
use strata_gpu_path::sdf::distance_to_quadratic;

let curve = QuadraticBezier::new(
    Point2D::new(0.0, 0.0),
    Point2D::new(50.0, 100.0),
    Point2D::new(100.0, 0.0),
);

let query_point = Point2D::new(50.0, 20.0);
let dist = distance_to_quadratic(query_point, &curve);

println!("Exact distance from query point to quadratic curve: {:.4} px", dist);
```

---

## Closed Path Winding Numbers and Path Booleans

For filled vector paths, determining whether a pixel lies inside or outside the shape boundary is accomplished by computing the **winding number**.

`strata_gpu_path::sdf::is_point_inside_closed_path` casts an infinite horizontal ray from query point $P$ towards $+X$, counting signed intersections with each quadratic segment:

```rust
use strata_gpu_path::curve::{Point2D, QuadraticBezier};
use strata_gpu_path::sdf::is_point_inside_closed_path;

let triangle_curves = vec![
    QuadraticBezier::new(Point2D::new(0.0, 0.0), Point2D::new(25.0, 0.0), Point2D::new(50.0, 0.0)),
    QuadraticBezier::new(Point2D::new(50.0, 0.0), Point2D::new(50.0, 25.0), Point2D::new(25.0, 50.0)),
    QuadraticBezier::new(Point2D::new(25.0, 50.0), Point2D::new(0.0, 25.0), Point2D::new(0.0, 0.0)),
];

assert!(is_point_inside_closed_path(Point2D::new(25.0, 20.0), &triangle_curves));
assert!(!is_point_inside_closed_path(Point2D::new(80.0, 80.0), &triangle_curves));
```

Supported fill rules:
- **`FillRule::NonZero`**: A point is inside if the net winding count $\neq 0$.
- **`FillRule::EvenOdd`**: A point is inside if the total intersection count is odd.

---

## GPU Instance Packing: `GpuCurveInstance`

Curves emitted to the GPU are packed into a **32-byte** aligned structure:

```rust
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpuCurveInstance {
    pub p0_x: f32,       // Start point X (4 bytes)
    pub p0_y: f32,       // Start point Y (4 bytes)
    pub p1_x: f32,       // Control point X (4 bytes)
    pub p1_y: f32,       // Control point Y (4 bytes)
    pub p2_x: f32,       // End point X (4 bytes)
    pub p2_y: f32,       // End point Y (4 bytes)
    pub flags: u32,      // Bitflags (FLAG_FILL = 1, FLAG_STROKE = 2) (4 bytes)
    pub z_index: f32,    // Stacking context depth (4 bytes)
}
```

The `PathCommandBuffer` aggregates paths into unified instance buffers:

```rust
use strata_gpu_path::buffer::PathCommandBuffer;
use strata_gpu_path::path::{PathBuilder, FillRule};

let mut buffer = PathCommandBuffer::new();

let path = PathBuilder::new()
    .move_to(10.0, 10.0)
    .line_to(120.0, 40.0)
    .quad_to(180.0, 120.0, 240.0, 80.0)
    .close()
    .fill(FillRule::NonZero)
    .build();

buffer.append_path(&path, 0.2, 1.0);

assert!(buffer.total_curves() > 0);
assert_eq!(buffer.byte_size(), buffer.total_curves() * 32);
```

---

## Analytical Vector Glyph Cache

Instead of pre-rendering fonts to 2048x2048 texture sheets, `AnalyticGlyphCache` stores font outlines as analytical Bézier contour lists:

```rust
use strata_gpu_path::font_cache::AnalyticGlyphCache;
use strata_gpu_path::path::PathBuilder;
use strata_gpu_path::curve::Point2D;

let mut cache = AnalyticGlyphCache::new();

// Register vector outline for glyph 'A'
let glyph_a_path = PathBuilder::new()
    .move_to(0.0, 0.0)
    .line_to(25.0, 80.0)
    .line_to(50.0, 0.0)
    .close()
    .build();

cache.register_path_glyph('A', 55.0, &glyph_a_path, 0.1);

// Query signed distance at any scale factor: negative inside, positive outside
let query_pt = Point2D::new(25.0, 30.0);
if let Some(sdf) = cache.evaluate_glyph_sdf('A', query_pt) {
    println!("Signed distance at {:?}: {:.2} px", query_pt, sdf);
    assert!(sdf < 0.0); // Inside glyph contour
}
```

---

## Complete Working Rust Example: Cloud Infrastructure Spline Graph

The following complete Rust example demonstrates generating smooth, cubic Bézier time-series telemetry splines for cloud server request throughput, decomposing them into GPU curve instances, and calculating signed distances:

```rust
use strata_gpu_path::buffer::PathCommandBuffer;
use strata_gpu_path::curve::Point2D;
use strata_gpu_path::path::{Path, PathBuilder, StrokeStyle};
use strata_gpu_path::sdf::distance_to_quadratic;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Telemetry datapoints: (Time in seconds, Requests per second)
    let telemetry_samples = [
        (0.0, 1200.0),
        (10.0, 1850.0),
        (20.0, 3400.0),
        (30.0, 2900.0),
        (40.0, 4800.0),
        (50.0, 4100.0),
    ];

    // Normalize datapoints to screen coordinates (600x300 viewport)
    let map_pt = |(t, req): (f32, f32)| -> Point2D {
        let x = (t / 50.0) * 560.0 + 20.0;
        let y = 280.0 - (req / 5000.0) * 240.0;
        Point2D::new(x, y)
    };

    // 2. Build continuous cubic Bézier telemetry curve
    let mut builder = PathBuilder::new();
    let p0 = map_pt(telemetry_samples[0]);
    builder = builder.move_to(p0.x, p0.y);

    for window in telemetry_samples.windows(2) {
        let start = map_pt(window[0]);
        let end = map_pt(window[1]);

        // Compute smooth cubic control handles
        let dx = (end.x - start.x) * 0.5;
        let c1 = Point2D::new(start.x + dx, start.y);
        let c2 = Point2D::new(end.x - dx, end.y);

        builder = builder.cubic_to(c1.x, c1.y, c2.x, c2.y, end.x, end.y);
    }

    let path = builder
        .stroke(StrokeStyle { width: 2.5, miter_limit: 4.0 })
        .build();

    // 3. Decompose cubic path into GPU-ready quadratic segments
    let tolerance = 0.2; // 0.2px subpixel accuracy
    let gpu_quadratics = path.to_quadratics(tolerance);
    println!("Generated {} quadratic Bézier GPU curves for telemetry spline.", gpu_quadratics.len());

    // 4. Batch curves into GPU command buffer
    let mut command_buffer = PathCommandBuffer::new();
    command_buffer.append_path(&path, tolerance, 1.0);

    println!(
        "Encoded {} bytes into GPU buffer across {} draw calls.",
        command_buffer.byte_size(),
        command_buffer.total_draw_calls
    );

    // 5. Test mathematical SDF accuracy on arbitrary cursor inspection point
    let cursor_pos = Point2D::new(300.0, 150.0);
    let mut min_distance = f32::INFINITY;

    for quad in &gpu_quadratics {
        let d = distance_to_quadratic(cursor_pos, quad);
        if d < min_distance {
            min_distance = d;
        }
    }

    println!(
        "Distance from inspector cursor ({:.1}, {:.1}) to telemetry curve: {:.3} px",
        cursor_pos.x, cursor_pos.y, min_distance
    );

    Ok(())
}
```

---

## Embedded WebGPU/WGSL Compute Shaders & Direct Hardware Path Pipeline

`strata_gpu_path` embeds zero-CPU WebGPU Shading Language (WGSL) compute and analytical fragment shaders for direct GPU curve decomposition, stroke expansion, and subpixel distance field rasterization:

### 1. Parallel Curve Decomposition (`WGSL_PATH_COMPUTE_FLATTEN_SHADER`)
Accepts batches of 64-byte `RawCurveInput` structs in a read-only storage buffer and computes degree reduction / quadratic approximations directly on the GPU in `@compute @workgroup_size(64)` workgroups.

### 2. Analytical Signed Distance Field Rasterization (`WGSL_ANALYTICAL_PATH_RASTER_SHADER`)
Evaluates continuous distance to quadratic Bézier segments $B(t)$ in fragment shaders, applying subpixel anti-aliasing via distance field thresholds without MSAA or polygon triangulation.

### 3. Analytical Stroke Expansion Engine
Computes unit normal offset envelopes $\mathbf{n}(t) = \frac{(-y'(t), x'(t))}{\sqrt{x'(t)^2 + y'(t)^2}}$ for thick strokes, generating closed outline curves with support for `LineCap` (`Butt`, `Round`, `Square`) and `LineJoin` (`Miter`, `Round`, `Bevel`).

### 4. Direct Hardware Integration with `GpuPathPipeline`
Within `strata_render` and `strata::runtime::AppRuntime`, `GpuPathPipeline` batches both `DrawCommand::DrawGpuPath` and declarative SVG icon paths into zero-copy `GpuPathSegment` storage buffers, complete with spatial damage scissoring and viewport projection.

