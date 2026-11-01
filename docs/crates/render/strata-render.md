# strata_render

`strata_render` is the core graphics rasterization, stacking context compositor, and hardware presentation engine of the Strata framework. It transforms high-level layout trees into low-level display lists, evaluates analytical Signed Distance Fields (SDF) directly on the GPU, enforces strict CSS 2.1 Appendix E stacking contexts, and integrates a unified Three.js-style 3D viewport into declarative RSX applications.

---

## Architectural Mission

Traditional cross-platform frameworks rely on heavyweight third-party 2D rasterizers such as Skia, Cairo, or Chromium Blink/WebRender. These rasterizers introduce significant binary bloat, dynamic memory allocations, and CPU-intensive tessellation overhead.

`strata_render` implements a modern, GPU-native graphics pipeline engineered in pure Rust:
- **Zero CPU Tessellation**: Rounded rectangles, borders, and drop shadows are evaluated analytically in fragment shaders via signed distance mathematics.
- **Spec-Compliant Compositing**: Implements the complete 7-layer stacking context tree defined by CSS 2.1 Appendix E.
- **Subpixel Damage Tracking**: Eliminates redundant frame presentation through dirty-rect damage calculation.
- **Unified 3D Viewport**: Seamlessly embeds hardware-accelerated 3D scenes inside standard CSS Box Model hierarchies.

---

## Subsystem Architecture

```text
┌─────────────────────────────────────────────────────────────────┐
│               strata_layout (LayoutNode Tree)                   │
└────────────────────────────────┬────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Compositor Pipeline                        │
│   - CSS 2.1 Appendix E 7-Layer Hierarchy Sorting                │
│   - Stacking Context Boundary Resolution                        │
│   - Clip Rectangle & Border Radius Inheritance                  │
└────────────────────────────────┬────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────┐
│                    DisplayList & Commands                       │
│   - DrawRect, DrawRoundedRect, DrawShadow, DrawText             │
│   - DrawViewport3D, DrawCanvas2D, PushClip, PopClip             │
└────────────────┬───────────────────────────────┬────────────────┘
                 │                               │
                 ▼                               ▼
┌─────────────────────────────────┐   ┌───────────────────────────┐
│     GpuPresentationPipeline     │   │   FrameBuffer (Software)  │
│   - 96-Byte GpuQuadInstance     │   │   - Analytical SDF 2D     │
│   - WGSL Analytical Quad Shader │   │   - Porter-Duff Over      │
│   - Batch Upload & Scissoring   │   │   - Headless CI / Tests   │
└────────────────┬────────────────┘   └─────────────┬─────────────┘
                 │                                  │
                 └────────────────┬─────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────┐
│                     Hardware Presentation                       │
│   - SwapchainCompositor & SurfacePresenter                      │
│   - Subpixel Damage Rect Scissoring                             │
│   - Backing Formats: Bgra8Unorm / Rgba8Unorm / Rgba16Float      │
└─────────────────────────────────────────────────────────────────┘
```

---

## CSS 2.1 Appendix E 7-Layer Stacking Contexts

In CSS rendering, z-index does not simply order siblings; it creates isolated stacking contexts where descendant elements are painted according to a rigorous 7-layer specification.

### Stacking Context Creation Rules

An element establishes a new Stacking Context in `strata_render` if any of the following conditions are met:
1. The element is the root layout node (`is_root == true`).
2. The element is positioned (`position: relative`, `absolute`, or `fixed`) and has an explicit `z-index` value.
3. The element has an opacity strictly less than `1.0` (`opacity < 0.999`).

### The 7-Layer Painting Order

The `Compositor::paint_stacking_context` method recursively processes nodes into seven distinct layers:

| Layer | Classification | Processing Rules |
| :--- | :--- | :--- |
| **Layer 1** | Backgrounds & Borders | Renders box shadows, background fill (`DrawRect` / `DrawRoundedRect`), and element borders. |
| **Layer 2** | Negative `z-index` | Child stacking contexts with explicit negative `z-index`, sorted ascending (lowest value first). |
| **Layer 3** | In-Flow Block Descendants | Non-inline, non-positioned in-flow block elements (`Position::Static`), rendered depth-first. |
| **Layer 4** | Non-Positioned Floats | Floating content (bypassed in modern flexbox/grid layout trees). |
| **Layer 5** | In-Flow Inline Descendants | In-flow inline-level boxes (`Display::Inline`, `Display::InlineBlock`, text nodes). |
| **Layer 6** | Positioned `z-index: 0 / auto` | Positioned descendants where `z-index` is `auto` or explicitly `0`. |
| **Layer 7** | Positive `z-index` | Child stacking contexts with positive `z-index`, sorted ascending (e.g., `1`, then `10`). |

### DisplayList and Draw Commands

When the layout tree is compiled, `Compositor::build_display_list` outputs a flat, ordered `DisplayList` containing explicit GPU primitives:

```rust
use strata_layout::Rect;
use strata_primitives::geom::EdgeInsets;
use strata_style::types::{BorderRadius, BoxShadow, Color, FontWeight};
use strata_render::scene3d::Scene3D;
use strata_render::canvas2d::CanvasCommand;

#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    DrawRect {
        bounds: Rect,
        color: Color,
    },
    DrawRoundedRect {
        bounds: Rect,
        color: Color,
        border_radius: BorderRadius,
        border_width: EdgeInsets<f32>,
        border_color: Option<Color>,
    },
    DrawShadow {
        bounds: Rect,
        shadow: BoxShadow,
    },
    DrawBorder {
        bounds: Rect,
        widths: EdgeInsets<f32>,
        color: Color,
        border_radius: BorderRadius,
    },
    DrawText {
        origin: (f32, f32),
        text: String,
        font_size: f32,
        line_height: f32,
        font_weight: FontWeight,
        color: Color,
    },
    DrawViewport3D {
        bounds: Rect,
        scene: Scene3D,
    },
    DrawCanvas2D {
        bounds: Rect,
        commands: Vec<CanvasCommand>,
    },
    PushClip {
        bounds: Rect,
        radius: BorderRadius,
    },
    PopClip,
}
```

---

## Dirty-Rect Damage Tracking

Repainting an entire $3840 \times 2160$ screen when only a small telemetry badge updates wastes memory bandwidth and battery power. Strata enforces subpixel damage tracking across successive frames.

### Damage Accumulation

The `DamageTracker` accumulates axis-aligned bounding boxes of all modified components:

```rust
use strata_render::prelude::*;
use strata_layout::Rect;

let mut tracker = DamageTracker::new();

// Mark mutated telemetry widget bounds
tracker.add_damage(Rect::new(24.0, 48.0, 320.0, 180.0));
tracker.add_damage(Rect::new(280.0, 150.0, 120.0, 40.0));

// Compute the minimal bounding rectangle encompassing all damaged regions
if let Some(damage_box) = tracker.bounding_damage() {
    assert_eq!(damage_box.x, 24.0);
    assert_eq!(damage_box.y, 48.0);
    assert_eq!(damage_box.width, 376.0); // 400.0 - 24.0
    assert_eq!(damage_box.height, 142.0); // 190.0 - 48.0
}
```

### Damaged Execution

When rendering a frame, `FrameBuffer::execute_display_list_damaged` or `GpuBatch::from_display_list_damaged` compares each command's bounding box against the damage rectangle. Commands outside the damage envelope are culled in $O(1)$:

```rust
// If a command bounds does not intersect the damage bounding box, it is skipped entirely:
if let Some(cmd_bounds) = cmd.bounds() {
    if cmd_bounds.intersect(effective_clip).is_none() {
        continue; // Culled without CPU or GPU rasterization cost
    }
}
```

---

## Analytical Signed Distance Field (SDF) Rasterization

Rather than computing hundreds of micro-triangles to produce curved borders or anti-aliased corners, `strata_render` calculates the exact signed distance from each pixel center to the geometric shape boundary.

### The Rounded Rectangle SDF Formula

Given query pixel $P = (p_x, p_y)$ relative to the box center, box half-extents $B = (w/2, h/2)$, and corner radius $r$:

$$\mathbf{q} = |P| - B + (r, r)$$

$$d(P) = \|\max(\mathbf{q}, \mathbf{0})\|_2 + \min(\max(q_x, q_y), 0) - r$$

```rust
#[inline(always)]
pub fn sd_rounded_rect(px: f32, py: f32, half_w: f32, half_h: f32, r: f32) -> f32 {
    let qx = px.abs() - half_w + r;
    let qy = py.abs() - half_h + r;

    let len = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    let inside = qx.max(qy).min(0.0);
    len + inside - r
}

#[inline(always)]
pub fn sdf_coverage(distance: f32) -> f32 {
    (0.5 - distance).clamp(0.0, 1.0)
}
```

When evaluated, `sdf_coverage(d)` returns continuous subpixel alpha in $[0.0, 1.0]$, delivering mathematically smooth anti-aliased edges without multi-sample anti-aliasing (MSAA) memory overhead.

---

## GPU Presentation Pipeline

The GPU presentation pipeline compiles high-level `DisplayList` instructions into instanced GPU vertex buffers.

### Memory Representation: `GpuQuadInstance`

Each 2D rectangle, rounded card, or border emitted to the GPU occupies exactly **96 contiguous bytes** aligned for hardware consumption:

```rust
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpuQuadInstance {
    pub bounds: [f32; 4],        // [x, y, width, height]
    pub color: [f32; 4],         // [r, g, b, a]
    pub border_radius: [f32; 4], // [top_left, top_right, bottom_right, bottom_left]
    pub border_width: [f32; 4],  // [top, right, bottom, left]
    pub border_color: [f32; 4],  // [r, g, b, a]
    pub clip_rect: [f32; 4],     // [clip_x, clip_y, clip_w, clip_h]
}
```

### Analytical Quad Shader (WGSL)

The embedded WebGPU Shading Language (WGSL) shader executes instanced quad rasterization with zero CPU geometry processing:

```wgsl
struct VertexInput {
    @location(0) position: vec2<f32>,
};

struct InstanceInput {
    @location(1) bounds: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) border_radius: vec4<f32>,
    @location(4) border_width: vec4<f32>,
    @location(5) border_color: vec4<f32>,
    @location(6) clip_rect: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) pixel_pos: vec2<f32>,
    @location(1) bounds: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) border_radius: vec4<f32>,
    @location(4) border_width: vec4<f32>,
    @location(5) border_color: vec4<f32>,
    @location(6) clip_rect: vec4<f32>,
};

struct Uniforms {
    viewport_size: vec2<f32>,
    _padding: vec2<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(model: VertexInput, instance: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    let pixel_coords = instance.bounds.xy + model.position * instance.bounds.zw;
    
    let ndc_x = (pixel_coords.x / uniforms.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (pixel_coords.y / uniforms.viewport_size.y) * 2.0;
    
    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.pixel_pos = pixel_coords;
    out.bounds = instance.bounds;
    out.color = instance.color;
    out.border_radius = instance.border_radius;
    out.border_width = instance.border_width;
    out.border_color = instance.border_color;
    out.clip_rect = instance.clip_rect;
    return out;
}

fn sd_rounded_rect(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + vec2<f32>(r, r);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Hardware Scissor Clipping
    if (in.pixel_pos.x < in.clip_rect.x || in.pixel_pos.x > in.clip_rect.x + in.clip_rect.z ||
        in.pixel_pos.y < in.clip_rect.y || in.pixel_pos.y > in.clip_rect.y + in.clip_rect.w) {
        discard;
    }

    let center = in.bounds.xy + in.bounds.zw * 0.5;
    let half_extents = in.bounds.zw * 0.5;
    let p = in.pixel_pos - center;

    // Corner radius selection per quadrant
    var r: f32 = in.border_radius.x;
    if (p.x >= 0.0 && p.y < 0.0) {
        r = in.border_radius.y;
    } else if (p.x >= 0.0 && p.y >= 0.0) {
        r = in.border_radius.z;
    } else if (p.x < 0.0 && p.y >= 0.0) {
        r = in.border_radius.w;
    }
    r = min(r, min(half_extents.x, half_extents.y));

    let dist = sd_rounded_rect(p, half_extents, r);
    let coverage = clamp(0.5 - dist, 0.0, 1.0);

    if (coverage <= 0.0) {
        discard;
    }

    let bw = in.border_width.x;
    if (bw > 0.0 && in.border_color.a > 0.0) {
        let inner_dist = dist + bw;
        let inner_cov = clamp(0.5 - inner_dist, 0.0, 1.0);
        
        let border_alpha = in.border_color.a * coverage * (1.0 - inner_cov);
        let fill_alpha = in.color.a * inner_cov;
        
        let out_rgb = in.border_color.rgb * border_alpha + in.color.rgb * fill_alpha;
        let out_a = border_alpha + fill_alpha;
        return vec4<f32>(out_rgb, out_a);
    }

    return vec4<f32>(in.color.rgb, in.color.a * coverage);
}
```

---

## Unified 3D Viewport (`<Canvas3D>`)

Strata enables developers to embed fully hardware-accelerated 3D viewports directly inside their 2D RSX layouts. The 3D viewport participates directly in CSS box-sizing, rounded corner clipping, and stacking order.

### Declarative RSX Components

The 3D engine mirrors the declarative semantics of Three.js and React Three Fiber:

- `<Canvas3D>`: Root canvas container participating in flexbox or grid layout.
- `<PerspectiveCamera>`: Perspective projection camera with field-of-view, near/far clipping planes, position, and target vectors.
- `<OrthographicCamera>`: Orthographic projection camera for isometric architectural telemetry views.
- `<AmbientLight>`, `<DirectionalLight>`, `<PointLight>`: Lighting sources supporting shadow maps and physical attenuation.
- `<Mesh>`: Scene node binding geometry and material with spatial transformations (`position`, `rotation`, `scale`).
- `<BoxGeometry>`, `<SphereGeometry>`, `<PlaneGeometry>`, `<CylinderGeometry>`: Parametric geometric primitives.
- `<StandardMaterial>`: Physically-Based Rendering (PBR) metallic-roughness material.
- `<ShaderMaterial>`: Low-level custom GLSL/WGSL fragment shader material with reactive uniform bindings.

### Declarative Example: Cloud Infrastructure Topology

```rust
use strata::prelude::*;
use strata_render::prelude::*;

#[component]
pub fn ClusterNodeViewer(node_id: &'static str, load_metric: f32) -> View {
    rsx! {
        <div class="flex flex-col p-6 bg-slate-900 border border-slate-800 rounded-2xl w-full max-w-4xl">
            <div class="flex items-center justify-between mb-4">
                <div>
                    <h3 class="text-white font-semibold text-lg">Cluster Node Telemetry</h3>
                    <p class="text-xs text-slate-400 font-mono">ID: {node_id}</p>
                </div>
                <div class="px-3 py-1 bg-emerald-500/10 border border-emerald-500/20 text-emerald-400 text-xs font-mono rounded-full">
                    ACTIVE
                </div>
            </div>

            <!-- 3D Canvas participating in CSS Flexbox layout -->
            <Canvas3D class="w-full h-80 rounded-xl overflow-hidden border border-slate-800/80 bg-slate-950">
                <PerspectiveCamera
                    fov=50.0
                    aspect=1.777
                    near=0.1
                    far=100.0
                    position={[0.0, 3.0, 6.0]}
                    target={[0.0, 0.0, 0.0]}
                />
                <AmbientLight color="#1e293b" intensity=0.6 />
                <DirectionalLight
                    position={[4.0, 8.0, 4.0]}
                    color="#ffffff"
                    intensity=1.2
                    cast_shadow=true
                />

                <!-- Server Blade Chassis Representation -->
                <Mesh position={[0.0, 0.0, 0.0]} rotation={[0.2, 0.4, 0.0]} scale={[1.0, 1.0, 1.0]}>
                    <BoxGeometry width=2.4 height=0.6 depth=3.2 />
                    <StandardMaterial color="#334155" roughness=0.3 metalness=0.85 />
                </Mesh>

                <!-- Status Indicator Heat Emitting Component -->
                <Mesh position={[0.0, 0.4, 0.0]} scale={[0.8, 0.1, 0.8]}>
                    <CylinderGeometry radius_top=0.6 radius_bottom=0.6 height=0.2 radial_segments=32 />
                    <StandardMaterial color="#3b82f6" roughness=0.1 metalness=0.9 />
                </Mesh>
            </Canvas3D>

            <div class="mt-4 flex items-center justify-between text-xs text-slate-400 font-mono">
                <span>GPU Compute Utilization: 74.2%</span>
                <span>Thermal Dissipation: 41 C</span>
            </div>
        </div>
    }
}
```

---

## Low-Level Custom Hardware Shader Pipeline

For specialized cloud telemetry applications requiring custom WGSL compute passes or custom geometry pipelines, `strata_render` provides the `CustomShaderPipeline` escape hatch.

```rust
use strata_render::custom_shader::{
    CustomShaderPipeline, FaceCullMode, PrimitiveTopology, WgslShaderSource,
};
use strata_render::math3d::Mat4;

// Load standard embedded PBR shader or custom WGSL source
let shader = WgslShaderSource::pbr_3d();
let mut pipeline = CustomShaderPipeline::new(shader);

// Configure pipeline state
pipeline.topology = PrimitiveTopology::TriangleList;
pipeline.cull_mode = FaceCullMode::Back;
pipeline.depth_write_enabled = true;

// Bind uniform values
pipeline.set_uniform_f32("roughness", 0.15);
pipeline.set_uniform_vec4("base_color", [0.12, 0.58, 0.95, 1.0]);
pipeline.set_uniform_mat4("model_view", &Mat4::IDENTITY);

// Validate entry points and shader syntax integrity
assert!(pipeline.validate().is_ok());
```

---

## Hybrid 2D/3D Compositing and Shared Render Targets

When compositing complex 3D viewports alongside high-frequency 2D telemetry heads-up displays (HUDs), `SharedRenderTarget` manages offscreen color and depth buffers. It avoids redundant 3D scene re-draws when only 2D UI elements have mutated:

```rust
use strata_layout::Rect;
use strata_render::shared_target::SharedRenderTarget;
use strata_render::scene3d::Scene3D;
use strata_render::rasterizer::FrameBuffer;

let bounds = Rect::new(0.0, 0.0, 1280.0, 720.0);
let mut shared_target = SharedRenderTarget::new(bounds);
let scene = Scene3D::default();

// Only re-renders 3D geometry when the scene model or camera changes
shared_target.render_if_dirty(&scene);

// Blits 3D offscreen buffer into the primary swapchain framebuffer
let mut surface_framebuffer = FrameBuffer::new(1280, 720);
shared_target.composite_to_surface(&mut surface_framebuffer, None);
```

---

## Complete Working Rust Example: High-Performance Presentation Loop

The following complete Rust example demonstrates creating a swapchain compositor, compiling a layout-driven display list, executing damage-scissored rendering, and presenting to a display surface:

```rust
use strata_layout::Rect;
use strata_primitives::geom::EdgeInsets;
use strata_style::types::{BorderRadius, BoxShadow, Color};
use strata_render::compositor::{DisplayList, DrawCommand};
use strata_render::swapchain::{
    SoftwareSurfacePresenter, SurfacePresenter, SwapchainCompositor, SwapchainFormat,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    const WIDTH: u32 = 1920;
    const HEIGHT: u32 = 1080;
    const SCALE_FACTOR: f32 = 1.0;

    // 1. Initialize Swapchain Compositor and Backing Surface Presenter
    let mut compositor = SwapchainCompositor::new(WIDTH, HEIGHT, SCALE_FACTOR);
    let mut presenter = SoftwareSurfacePresenter::new(WIDTH, HEIGHT, SwapchainFormat::Bgra8Unorm);

    // 2. Build a DisplayList representing a Cloud Telemetry Dashboard
    let mut display_list = DisplayList::new();

    // Dark background chassis
    display_list.push(DrawCommand::DrawRect {
        bounds: Rect::new(0.0, 0.0, WIDTH as f32, HEIGHT as f32),
        color: Color::rgb(15, 23, 42),
    });

    // Telemetry Card 1 (Server Rack 1 Status)
    let card1_bounds = Rect::new(40.0, 40.0, 420.0, 240.0);
    display_list.push(DrawCommand::DrawShadow {
        bounds: card1_bounds,
        shadow: BoxShadow {
            offset_x: 0.0,
            offset_y: 8.0,
            blur_radius: 24.0,
            spread_radius: 0.0,
            color: Color::rgba(0, 0, 0, 120),
        },
    });
    display_list.push(DrawCommand::DrawRoundedRect {
        bounds: card1_bounds,
        color: Color::rgb(30, 41, 59),
        border_radius: BorderRadius::all(16.0),
        border_width: EdgeInsets::all(1.0),
        border_color: Some(Color::rgb(51, 65, 85)),
    });

    // Telemetry Card 2 (Network Packet Throughput)
    let card2_bounds = Rect::new(500.0, 40.0, 420.0, 240.0);
    display_list.push(DrawCommand::DrawRoundedRect {
        bounds: card2_bounds,
        color: Color::rgb(30, 41, 59),
        border_radius: BorderRadius::all(16.0),
        border_width: EdgeInsets::all(1.0),
        border_color: Some(Color::rgb(51, 65, 85)),
    });

    // 3. Initial Frame: Full screen presentation
    let presented = compositor.render_and_present(&display_list, &mut presenter, true)?;
    assert!(presented);
    assert_eq!(presenter.presented_frames, 1);

    // 4. Subsequent Frame: Mutate only Card 1 (dirty rect damage tracking)
    compositor.damage_tracker.add_damage(card1_bounds);

    // Present with damage scissoring: Card 2 and outer background are untouched!
    let presented_subsequent = compositor.render_and_present(&display_list, &mut presenter, false)?;
    assert!(presented_subsequent);
    assert_eq!(presenter.presented_frames, 2);
    assert_eq!(presenter.damage_clipped_frames, 1);

    println!(
        "Successfully presented frame {} with subpixel damage tracking.",
        compositor.frame_count
    );

    Ok(())
}
```
