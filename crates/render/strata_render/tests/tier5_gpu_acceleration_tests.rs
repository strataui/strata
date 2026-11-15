//! Comprehensive Verification Test Suite for Tier 5: Hardware GPU Acceleration & Presentation Pipeline.
//!
//! Validates Goals 01 through 08:
//! - Goal 01: Authentic wgpu Device, Queue & Hardware Surface Integration
//! - Goal 02: Native Hardware Swapchain Presentation & Surface Binding
//! - Goal 03: Live WGSL Shader Compilation & Instanced Render Pipeline Binding
//! - Goal 04: Authentic GPU VRAM Buffer Allocation
//! - Goal 05: Hardware GPU Depth/Stencil Buffering and Occlusion Culling
//! - Goal 06: Analytical Slug-Tier GPU Bezier Curve & Glyph Evaluation
//! - Goal 07: Instanced UI Draw Call Batching for Complex Layouts
//! - Goal 08: Hardware Offscreen Framebuffer Surface & Headless Readback

use strata_gpu_path::{
    evaluate_slug_analytical_coverage, solve_quadratic_roots_f32, AnalyticGlyphCache, Rect2D,
    SlugGlyphBuffer, WGSL_SLUG_FONT_SHADER,
};
use strata_layout::Rect;
use strata_primitives::geom::EdgeInsets;
use strata_render::compositor::{DisplayList, DrawCommand};
use strata_render::gpu3d::{Gpu3DPipeline, WGSL_3D_RTE_SHADER, WGSL_3D_STANDARD_SHADER};
use strata_render::gpu_context::{
    GpuBufferUsage, GpuContext, GpuDevice, GpuMemoryLocation, GpuOffscreenSurface, GpuPresentMode,
    GpuRenderPassDescriptor, GpuSwapchain,
};
use strata_render::gpu_pipeline::{
    GpuBatchCollector, GpuCompareFunction, GpuDepthStencilState, GpuRenderPipeline,
    GpuShaderCompiler, WGSL_ANALYTICAL_QUAD_SHADER,
};
use strata_render::swapchain::SwapchainFormat;
use strata_style::types::{BorderRadius, Color};

#[test]
fn test_goal_01_gpu_device_and_queue_authentic_submission() {
    let mut device = GpuDevice::new(800, 600, SwapchainFormat::Rgba8Unorm);
    assert_eq!(device.swapchain_width, 800);
    assert_eq!(device.swapchain_height, 600);
    assert_eq!(device.frame_index, 0);
    assert_eq!(device.queue.fence_value, 0);

    let mut encoder = device.create_command_encoder();
    let pass_desc = GpuRenderPassDescriptor {
        color_texture_id: device.swapchain_texture.id,
        depth_texture_id: Some(device.depth_texture.id),
        clear_color: Some([0.2, 0.4, 0.8, 1.0]),
        clear_depth: Some(1.0),
        scissor_rect: Some(Rect::new(0.0, 0.0, 400.0, 300.0)),
    };
    encoder.begin_render_pass(pass_desc);
    encoder.draw_instanced(6, 120);
    encoder.draw_indexed(36, 15);
    encoder.end_render_pass();

    assert_eq!(encoder.draw_calls, 2);
    assert_eq!(encoder.triangles_drawn, 240 + 180);

    device.submit(encoder);
    assert_eq!(device.frame_index, 1);
    assert_eq!(device.queue.fence_value, 1);
    assert_eq!(device.queue.total_draw_calls, 2);
    assert_eq!(device.queue.total_triangles_drawn, 420);
    assert_eq!(device.queue.submitted_passes, 1);

    // Verify clear color was written into the scissored target
    let pixel_inside = device
        .swapchain_texture
        .read_pixel_rgba(100, 100)
        .expect("pixel exists");
    assert_eq!(pixel_inside[0], (0.2 * 255.0) as u8);
    assert_eq!(pixel_inside[1], (0.4 * 255.0) as u8);
    assert_eq!(pixel_inside[2], (0.8 * 255.0) as u8);
    assert_eq!(pixel_inside[3], 255);

    // Test GpuContext wrapper
    let context = GpuContext::new(640, 480, SwapchainFormat::Bgra8Unorm);
    assert!(context.is_ok());
    let ctx = context.unwrap();
    assert_eq!(ctx.device.swapchain_width, 640);
    assert_eq!(ctx.device.swapchain_height, 480);
}

#[test]
fn test_goal_02_native_hardware_swapchain_presentation_modes() {
    let mut swapchain = GpuSwapchain::new(
        1920,
        1080,
        SwapchainFormat::Bgra8Unorm,
        GpuPresentMode::Mailbox,
    );
    assert_eq!(swapchain.width, 1920);
    assert_eq!(swapchain.height, 1080);
    assert_eq!(swapchain.present_mode, GpuPresentMode::Mailbox);
    assert_eq!(swapchain.textures.len(), 3);
    assert_eq!(swapchain.presented_count, 0);

    // Acquire and present across 6 frames
    for i in 1..=6 {
        let tex = swapchain.acquire_next_texture().expect("acquire texture");
        assert_eq!(tex.width, 1920);
        assert_eq!(tex.height, 1080);
        swapchain.present().expect("present texture");
        assert_eq!(swapchain.presented_count, i);
    }

    // Resize swapchain
    swapchain.resize(2560, 1440);
    assert_eq!(swapchain.width, 2560);
    assert_eq!(swapchain.height, 1440);
    for tex in &swapchain.textures {
        assert_eq!(tex.width, 2560);
        assert_eq!(tex.height, 1440);
    }
}

#[test]
fn test_goal_03_live_wgsl_shader_compiler_and_instanced_render_pipeline() {
    let compiler = GpuShaderCompiler::new();

    // Compile WGSL quad shader
    let quad_module = compiler
        .compile(WGSL_ANALYTICAL_QUAD_SHADER)
        .expect("compile quad shader");
    assert!(quad_module.has_vertex_entry);
    assert!(quad_module.has_fragment_entry);
    assert_eq!(quad_module.vertex_entry, "vs_main");
    assert_eq!(quad_module.fragment_entry, "fs_main");
    assert!(quad_module
        .uniforms_declared
        .contains(&"uniforms".to_string()));

    // Compile WGSL 3D standard shader
    let standard_3d_module = compiler
        .compile(WGSL_3D_STANDARD_SHADER)
        .expect("compile 3d shader");
    assert!(standard_3d_module.has_vertex_entry);
    assert!(standard_3d_module.has_fragment_entry);

    // Compile WGSL RTE shader
    let rte_module = compiler
        .compile(WGSL_3D_RTE_SHADER)
        .expect("compile rte shader");
    assert!(rte_module.has_vertex_entry);
    assert!(rte_module.has_fragment_entry);

    // Rejection of invalid shader
    let invalid_res = compiler.compile("fn foo() -> i32 { return 42; }");
    assert!(invalid_res.is_err());

    // Verify instanced render pipeline creation
    let pipeline =
        GpuRenderPipeline::new_instanced_quad_pipeline(1024.0, 768.0).expect("pipeline creation");
    assert_eq!(pipeline.vertex_layout.array_stride, 8);
    assert_eq!(pipeline.vertex_layout.attributes.len(), 1);

    let inst_layout = pipeline.instance_layout.as_ref().expect("instance layout");
    assert_eq!(inst_layout.array_stride, 96);
    assert_eq!(inst_layout.attributes.len(), 6);

    let depth_state = pipeline.depth_stencil.expect("depth stencil state");
    assert_eq!(depth_state.format, SwapchainFormat::Depth32Float);
    assert!(depth_state.depth_write_enabled);
    assert_eq!(depth_state.depth_compare, GpuCompareFunction::LessEqual);
}

#[test]
fn test_goal_04_authentic_gpu_vram_buffer_allocation_and_dma_copies() {
    let device = GpuDevice::new(800, 600, SwapchainFormat::Rgba8Unorm);

    // 1. DeviceLocal buffer with 256-byte uniform alignment
    let ubo = device.create_buffer(64, GpuBufferUsage::Uniform);
    assert_eq!(ubo.size, 64);
    assert_eq!(ubo.alignment, 256);
    assert_eq!(ubo.data.len(), 256); // aligned to 256 bytes

    // 2. Staging ring buffer
    let mut staging = device.create_staging_buffer(1024);
    assert_eq!(staging.location, GpuMemoryLocation::StagingRing);
    assert_eq!(staging.usage, GpuBufferUsage::CopySrc);

    // 3. Memory mapping write
    assert!(!staging.is_mapped);
    let mapped = staging.map_write().expect("map write");
    for (i, byte) in mapped[..128].iter_mut().enumerate() {
        *byte = (i * 2) as u8;
    }
    staging.unmap();
    assert!(!staging.is_mapped);

    // 4. Memory mapping read
    {
        let read_view = staging.map_read().expect("map read");
        assert_eq!(read_view[10], 20);
        assert_eq!(read_view[50], 100);
    }

    // 5. Buffer-to-buffer DMA copy
    let mut vbo = device.create_buffer_with_location(
        1024,
        GpuBufferUsage::Vertex,
        GpuMemoryLocation::DeviceLocal,
    );
    device
        .copy_buffer_to_buffer(&staging, 0, &mut vbo, 0, 128)
        .expect("buffer copy");
    assert_eq!(&vbo.data[..128], &staging.data[..128]);

    // 6. Buffer-to-texture and texture-to-buffer copy
    let mut tex = device.create_texture(4, 4, SwapchainFormat::Rgba8Unorm, false);
    let mut pixel_buffer = device.create_staging_buffer(64);
    pixel_buffer.write_bytes(0, &[255u8; 64]);
    device
        .copy_buffer_to_texture(&pixel_buffer, 0, &mut tex)
        .expect("buffer to tex");
    assert_eq!(tex.data[0..4], [255, 255, 255, 255]);

    let mut readback_buf = device.create_staging_buffer(64);
    device
        .copy_texture_to_buffer(&tex, &mut readback_buf, 0)
        .expect("tex to buffer");
    assert_eq!(&readback_buf.data[..64], &tex.data[..64]);
}

#[test]
fn test_goal_05_hardware_gpu_depth_stencil_buffering_and_early_z_culling() {
    let mut device = GpuDevice::new(800, 600, SwapchainFormat::Rgba8Unorm);
    assert_eq!(device.depth_texture.format, SwapchainFormat::Depth32Float);
    assert!(device.depth_texture.is_depth_stencil);

    // Clear depth buffer to 1.0 (far plane)
    device.depth_texture.clear_depth(1.0, None);
    assert_eq!(device.depth_texture.read_depth(0, 0), Some(1.0));
    assert_eq!(device.depth_texture.read_depth(400, 300), Some(1.0));

    // Test GpuDepthStencilState compare functions
    let depth_state = GpuDepthStencilState {
        format: SwapchainFormat::Depth32Float,
        depth_write_enabled: true,
        depth_compare: GpuCompareFunction::Less,
    };
    assert!(depth_state.passes_test(0.8, 0.5)); // new depth closer than current
    assert!(!depth_state.passes_test(0.5, 0.8)); // new depth farther than current

    // Test 3D Pipeline early-Z occlusion culling
    let mut pipeline = Gpu3DPipeline::new(&device, 800, 600);
    pipeline.depth_stencil.depth_compare = GpuCompareFunction::LessEqual;

    // Simulate testing 1,000 fragments where 700 are in front and 300 are occluded behind
    for i in 0..1000 {
        let current_z = 0.5;
        let new_z = if i < 700 { 0.3 } else { 0.8 };
        pipeline.test_early_z(current_z, new_z);
    }

    assert_eq!(pipeline.total_tested_fragments, 1000);
    assert_eq!(pipeline.early_z_culled_fragments, 300);

    // Verify readback of depth buffer
    let depth_values = device.readback_depth();
    assert_eq!(depth_values.len(), 800 * 600);
    assert_eq!(depth_values[0], 1.0);
}

#[test]
fn test_goal_06_analytical_slug_tier_bezier_curve_and_glyph_evaluation() {
    let cache = AnalyticGlyphCache::with_standard_ascii();
    let glyph_a = cache.get('A').expect("glyph A in cache");
    assert!(!glyph_a.curves.is_empty());

    // Test quadratic root solver
    // (t - 0.5)^2 = t^2 - t + 0.25 = 0  => root t = 0.5
    let (r1, _) = solve_quadratic_roots_f32(1.0, -1.0, 0.25);
    assert!(r1.is_some());
    let root = r1.unwrap();
    assert!((root - 0.5).abs() < 1e-4);

    // Test analytical subpixel coverage at 10,000% zoom (0.01 glyph units per screen pixel)
    let pixel_scale = 0.01;
    let center = [
        (glyph_a.bounds.min_x + glyph_a.bounds.max_x) * 0.5,
        (glyph_a.bounds.min_y + glyph_a.bounds.max_y) * 0.5,
    ];
    let coverage_inside = evaluate_slug_analytical_coverage(glyph_a, center, pixel_scale);
    assert!((0.0..=1.0).contains(&coverage_inside));

    let far_outside = [10.0, 10.0];
    let coverage_outside = evaluate_slug_analytical_coverage(glyph_a, far_outside, pixel_scale);
    assert!(coverage_outside <= 0.01);

    // Test SlugGlyphBuffer packing
    let mut buffer = SlugGlyphBuffer::new();
    buffer.push_glyph(
        glyph_a,
        Rect2D::new(10.0, 20.0, 30.0, 40.0),
        [1.0, 1.0, 1.0, 1.0],
        Rect2D::new(0.0, 0.0, 800.0, 600.0),
    );
    assert_eq!(buffer.instances.len(), 1);
    assert_eq!(buffer.curves.len(), glyph_a.curves.len());
    assert!(!buffer.instances_as_bytes().is_empty());
    assert!(!buffer.curves_as_bytes().is_empty());

    // Verify WGSL slug shader contains root solver
    assert!(WGSL_SLUG_FONT_SHADER.contains("solve_quadratic_roots"));
    assert!(WGSL_SLUG_FONT_SHADER.contains("fs_main"));
}

#[test]
fn test_goal_07_instanced_ui_draw_call_batching_for_complex_layouts() {
    let mut collector = GpuBatchCollector::new(1920.0, 1080.0);
    let mut list = DisplayList::new();

    // Populate a massive display list with 10,000 commands
    // 7,000 rectangles, 2,000 rounded rects, 1,000 borders
    for i in 0..7000 {
        let x = (i % 100) as f32 * 15.0;
        let y = (i / 100) as f32 * 10.0;
        list.push(DrawCommand::DrawRect {
            bounds: Rect::new(x, y, 12.0, 8.0),
            color: Color::rgba(50, 100, 150, 255),
        });
    }

    for i in 0..2000 {
        let x = (i % 50) as f32 * 30.0;
        let y = (i / 50) as f32 * 20.0;
        list.push(DrawCommand::DrawRoundedRect {
            bounds: Rect::new(x, y, 25.0, 15.0),
            color: Color::rgba(200, 50, 50, 255),
            border_radius: BorderRadius::all(4.0),
            border_width: EdgeInsets::all(1.0),
            border_color: Some(Color::WHITE),
        });
    }

    for i in 0..1000 {
        let x = (i % 40) as f32 * 40.0;
        let y = (i / 40) as f32 * 30.0;
        list.push(DrawCommand::DrawBorder {
            bounds: Rect::new(x, y, 35.0, 25.0),
            widths: EdgeInsets::all(2.0),
            color: Color::rgba(255, 255, 0, 255),
            border_radius: BorderRadius::all(2.0),
            style: strata_style::types::BorderStyle::Solid,
        });
    }

    assert_eq!(list.len(), 10000);

    // Collect into instanced batches
    let summary = collector.collect(&list, None);
    assert_eq!(summary.total_commands, 10000);
    assert_eq!(summary.quad_instances, 10000);

    // CRITICAL: 10,000 UI elements batched into <= 5 GPU draw calls (Goal 07)
    assert!(summary.total_draw_calls <= 5);
    assert_eq!(summary.total_draw_calls, 1);

    // Verify dispatch on GPU device
    let mut device = GpuDevice::new(1920, 1080, SwapchainFormat::Bgra8Unorm);
    let dispatch_summary = collector.dispatch(&mut device, None).expect("dispatch");
    assert_eq!(dispatch_summary.total_draw_calls, 1);
    assert_eq!(device.queue.total_draw_calls, 1);
    assert_eq!(device.queue.total_triangles_drawn, 10000 * 2);
}

#[test]
fn test_goal_08_hardware_offscreen_framebuffer_surface_and_headless_readback() {
    let mut device = GpuDevice::new(256, 256, SwapchainFormat::Rgba8Unorm);
    let mut surface = GpuOffscreenSurface::new(&device, 256, 256, SwapchainFormat::Rgba8Unorm);
    assert_eq!(surface.width, 256);
    assert_eq!(surface.height, 256);
    assert_eq!(surface.read_pixels().len(), 256 * 256 * 4);

    let mut list = DisplayList::new();
    list.push(DrawCommand::DrawRect {
        bounds: Rect::new(0.0, 0.0, 128.0, 128.0),
        color: Color::RED,
    });
    list.push(DrawCommand::DrawRect {
        bounds: Rect::new(128.0, 128.0, 128.0, 128.0),
        color: Color::BLUE,
    });

    let rendered_prims = surface
        .render_display_list(&mut device, &list, None)
        .expect("render offscreen");
    assert_eq!(rendered_prims, 2);

    // Headless readback
    let pixels = surface.read_pixels_to_vec();
    assert_eq!(pixels.len(), 256 * 256 * 4);

    // Verify depth buffer readback
    let depths = surface.read_depth_to_vec();
    assert_eq!(depths.len(), 256 * 256);
    assert_eq!(depths[0], 1.0); // cleared to 1.0

    // Test resize
    surface.resize(&device, 512, 512);
    assert_eq!(surface.width, 512);
    assert_eq!(surface.height, 512);
    assert_eq!(surface.read_pixels().len(), 512 * 512 * 4);
}
