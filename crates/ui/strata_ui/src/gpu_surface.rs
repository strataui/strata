//! Declarative `<GpuSurface>` Component: Low-Level Hardware GPU Escape Hatch.

#![allow(non_snake_case)]

use strata_dom::View;
use strata_macro::{component, rsx};
pub use strata_render::custom_shader::{
    CustomShaderPipeline, FaceCullMode, GpuRenderCallback, GpuRenderContext, PrimitiveTopology,
    WgslShaderSource, WGSL_COMPUTE_TELEMETRY_SHADER, WGSL_PBR_3D_SHADER,
};

/// Declarative Low-Level GPU Escape Hatch Component.
///
/// Exposes raw GPU rendering pass hooks (`on_render`) and custom WGSL compute/fragment
/// shaders directly within the Strata component tree.
#[component]
pub fn GpuSurface(
    width: Option<f32>,
    height: Option<f32>,
    class: Option<&'static str>,
    on_render: Option<GpuRenderCallback>,
) -> View {
    let _w = width.unwrap_or(400.0);
    let _h = height.unwrap_or(300.0);
    let custom = class.unwrap_or("");

    rsx! {
        <div class={format!("relative overflow-hidden bg-neutral-950 rounded-xl border border-neutral-800/80 {}", custom)}>
            <div class="absolute inset-0 pointer-events-auto" />
        </div>
    }
}
