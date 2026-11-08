//! Strata Render: Display List, Stacking Context Compositor, Analytical SDF GPU Rasterizer, and Unified 3D Engine.

#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::field_reassign_with_default,
    clippy::chunks_exact_to_as_chunks,
    clippy::should_implement_trait,
    clippy::collapsible_match
)]

pub mod math3d;
pub mod geometry3d;
pub mod compositor;
pub mod rasterizer;
pub mod rasterizer3d;
pub mod camera64;
pub mod overlay3d;
pub mod scene3d;
pub mod texture3d;
pub mod terrain3d;
pub mod swapchain;
pub mod tiling;
pub mod gpu_pipeline;
pub mod kinetic_scroll;
pub mod canvas2d;
pub mod orbit_controls;
pub mod shared_target;
pub mod custom_shader;
pub mod font;
pub mod shm_compositor;
pub mod gpu_context;
pub mod gpu3d;
pub mod texture_atlas;
pub mod blur;
pub mod multi_window_compositor;

// Re-exports
pub use font::{
    get_glyph, get_glyph_or_fallback, map_common_unicode, FontFallbackChain, GenericFontFamily,
    OsPlatform, SystemFontResolver, FONT_GLYPH_HEIGHT, FONT_GLYPH_WIDTH, TOFU_GLYPH,
};
pub use shm_compositor::{
    CompositorTelemetry, ShmFrameSlot, ShmRingBuffer, SlotState, ZeroCopyCompositor,
};
pub use canvas2d::{
    rasterize_canvas_commands, Canvas2DContext, CanvasCommand, CanvasPathVerb, Transform2D,
};
pub use compositor::{Compositor, DamageTracker, DisplayList, DrawCommand};
pub use strata_layout::{FillRule, StrokeLineCap, StrokeLineJoin, SvgItem};
pub use custom_shader::{
    CustomShaderPipeline, FaceCullMode, GpuRenderCallback, GpuRenderContext, PrimitiveTopology,
    WgslShaderSource, WGSL_COMPUTE_TELEMETRY_SHADER, WGSL_PBR_3D_SHADER,
};
pub use geometry3d::{
    generate_box, generate_cylinder, generate_plane, generate_sphere, generate_custom, MeshBuffers, Vertex3D,
};
pub use gpu_pipeline::{
    GpuBatch, GpuPathBatch, GpuPathPipeline, GpuPresentationPipeline, GpuQuadInstance, GpuUniforms,
    GpuVertex, QUAD_VERTICES, WGSL_ANALYTICAL_QUAD_SHADER,
};
pub use strata_gpu_path as gpu_path;
pub use kinetic_scroll::{
    KineticScrollController, ScrollAxis, DEFAULT_FRICTION, REST_VELOCITY_THRESHOLD,
    RUBBER_BAND_MAX_LIMIT, SPRING_DAMPING, SPRING_STIFFNESS,
};
pub use math3d::{Mat4, Ray3D, Vec3, Vec4};
pub use orbit_controls::{OrbitControls, OrbitInteractionMode};
pub use rasterizer::{sd_rounded_rect, sdf_coverage, FrameBuffer};
pub use scene3d::{Camera3D, Geometry3D, Light3D, Material3D, Mesh3D, Scene3D, Transform3D};
pub use shared_target::{render_hybrid_frame, SharedRenderTarget};
pub use swapchain::{
    PresentError, SoftwareSurfacePresenter, SurfacePresenter, SwapchainCompositor, SwapchainFormat,
};
pub use tiling::{DEFAULT_TILE_SIZE, OcclusionTracker, Tile, TileCoord, TilingSet};
pub use gpu_context::{
    GpuBackend, GpuBufferHandle, GpuBufferUsage, GpuCommandEncoder, GpuDevice,
    GpuRenderPassDescriptor, GpuTextureHandle,
};
pub use gpu3d::{Gpu3DPipeline, Gpu3DUniforms, Gpu3DVertex, WGSL_3D_STANDARD_SHADER};
pub use texture_atlas::{AtlasRegion, GpuTextureAtlas};
pub use blur::{BackdropFilterPipeline, WGSL_SEPARABLE_GAUSSIAN_BLUR_SHADER};
pub use multi_window_compositor::{MultiWindowCompositor, WindowSwapchainState};

pub mod prelude {
    pub use crate::canvas2d::Canvas2DContext;
    pub use crate::compositor::{Compositor, DamageTracker, DisplayList, DrawCommand};
    pub use crate::custom_shader::{CustomShaderPipeline, GpuRenderContext};
    pub use crate::geometry3d::{
        generate_box, generate_cylinder, generate_plane, generate_sphere, generate_custom, MeshBuffers, Vertex3D,
    };
    pub use crate::kinetic_scroll::KineticScrollController;
    pub use crate::math3d::{Mat4, Ray3D, Vec3, Vec4};
    pub use crate::orbit_controls::OrbitControls;
    pub use crate::rasterizer::FrameBuffer;
    pub use crate::scene3d::*;
    pub use crate::shared_target::SharedRenderTarget;
    pub use crate::shm_compositor::{CompositorTelemetry, ShmFrameSlot, ShmRingBuffer, ZeroCopyCompositor};
    pub use crate::swapchain::{
        PresentError, SoftwareSurfacePresenter, SurfacePresenter, SwapchainCompositor, SwapchainFormat,
    };
    pub use crate::gpu_context::{GpuBackend, GpuDevice};
    pub use crate::gpu3d::{Gpu3DPipeline, WGSL_3D_STANDARD_SHADER};
    pub use crate::texture_atlas::GpuTextureAtlas;
    pub use crate::blur::BackdropFilterPipeline;
    pub use crate::multi_window_compositor::MultiWindowCompositor;
}
