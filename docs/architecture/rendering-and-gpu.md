# GPU & Analytical SDF Rendering

Strata bypasses traditional 2D rasterizers like Skia or Cairo in favor of a modern, GPU-native rendering pipeline (`strata_render` and `strata_gpu_path`).

---

## The Rendering Pipeline

```text
┌─────────────────────────┐
│     strata_layout       │ Layout Geometry & Box Models
└────────────┬────────────┘
             │
             ▼
┌─────────────────────────┐
│      Display List       │ Clip Rects, Opacity Layers, Draw Primitives
└────────────┬────────────┘
             │
             ▼
┌─────────────────────────┐
│   strata_gpu_path / SDF │ Analytical GPU Cubic Beziers & Glyph Curves
└────────────┬────────────┘
             │
             ▼
┌─────────────────────────┐
│   Hardware Swapchain    │ Vulkan / Metal / Direct3D 12 / WebGPU
└─────────────────────────┘
```

---

## Analytical GPU Paths & Slug-Tier SDF

Traditional 2D graphics libraries render text and vector paths using one of two inefficient methods:
1. **CPU Tessellation**: Paths are broken down into hundreds of tiny triangles on the CPU, creating massive vertex buffers and starving the CPU of compute cycles.
2. **Glyph Texture Atlases**: Fonts are pre-rasterized into GPU texture sheets. When scaling or zooming, atlases must be recomputed, causing frame stutter, or text appears blurry.

Strata implements **Analytical GPU Path Evaluation**:
- **Cubic Bezier Shaders**: Bezier curves are evaluated directly inside GPU fragment shaders using quadratic and cubic root-finding mathematics.
- **Zero Texture Atlases**: Font outlines and vector icons are passed as compact mathematical polynomials.
- **Infinite Zoom**: Glyphs and vector shapes remain mathematically sharp at 1x, 4x, or 100x zoom levels with zero memory overhead.

---

## Stacking Contexts & 3D Transforms

The render tree computes visual layers based on the CSS Stacking Context specification:
- **`z-index` Ordering**: Hierarchical sorting of positioned elements.
- **Compositing Layers**: Elements with `opacity < 1.0`, CSS filters (`backdrop-blur`), or 3D transforms (`transform: rotateY(...)`) are promoted to isolated GPU render targets.
- **Hardware Clipping**: Rounded corners (`border-radius`) and overflow clipping (`overflow: hidden`) are evaluated analytically in pixel shaders using signed distance functions, eliminating jagged edges.

---

## Unified 3D Viewport (`<Canvas3D>`)

Strata enables developers to embed fully hardware-accelerated 3D viewports directly inside their 2D RSX layouts:

```rust
use strata::prelude::*;
use strata_render::three_d::*;

#[component]
pub fn PbrModelViewer() -> View {
    rsx! {
        <div class="p-6 bg-neutral-900 rounded-2xl border border-neutral-800">
            <h2 class="text-white font-semibold text-lg mb-4">Hardware Component CAD</h2>

            <!-- 3D Canvas participating in CSS Flexbox layout -->
            <Canvas3D
                class="w-full h-96 rounded-xl overflow-hidden border border-neutral-800"
                scene={|scene| {
                    scene.add_mesh(Mesh::new(
                        BoxGeometry::new(1.0, 1.0, 1.0),
                        StandardMaterial::pbr()
                            .albedo(Color::rgb(0.36, 0.41, 0.82))
                            .roughness(0.2)
                            .metallic(0.8)
                    ));
                    scene.add_directional_light(Vec3::new(2.0, 4.0, 3.0), Color::WHITE);
                }}
                camera={PerspectiveCamera::new(45.0, 1.0, 0.1, 100.0)}
            />
        </div>
    }
}
```

The 3D canvas participates directly in CSS box-sizing, rounded corner clipping, and stacking order.
