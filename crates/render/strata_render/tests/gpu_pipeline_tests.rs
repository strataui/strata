//! Integration tests for Strata Render Hardware GPU pipelines:
//! GPU Context, 3D Pipeline, Texture Atlas, Backdrop Filter Blur, and Multi-Window Swapchains.

use strata_gpu_path::curve::Point2D;
use strata_layout::typography::inline_context::{InlineTextStyle, TextFragment};
use strata_layout::Rect;
use strata_render::blur::BackdropFilterPipeline;
use strata_render::compositor::{DisplayList, DrawCommand};
use strata_render::gpu3d::{Gpu3DPipeline, Gpu3DUniforms, WGSL_3D_STANDARD_SHADER};
use strata_render::gpu_context::{
    GpuAdapter, GpuBufferUsage, GpuDevice, GpuPresentMode, GpuRenderPassDescriptor,
    GpuSurfacePresenter, GpuSwapchain,
};
use strata_render::gpu_pipeline::GpuSlugTextPipeline;
use strata_render::multi_window_compositor::MultiWindowCompositor;
use strata_render::rasterizer::FrameBuffer;
use strata_render::scene3d::{
    Camera3D, Geometry3D, Light3D, Material3D, Mesh3D, Scene3D, Transform3D,
};
use strata_render::swapchain::{SurfacePresenter, SwapchainFormat};
use strata_render::texture_atlas::GpuTextureAtlas;
use strata_style::types::Color;

#[test]
fn test_gpu_device_and_context_resource_allocation() {
    let mut device = GpuDevice::new(1024, 768, SwapchainFormat::Bgra8Unorm);
    assert_eq!(device.swapchain_width, 1024);
    assert_eq!(device.swapchain_height, 768);
    assert_eq!(device.swapchain_format, SwapchainFormat::Bgra8Unorm);
    assert!(!device.adapter_name.is_empty());
    assert_eq!(device.frame_index, 0);

    // Verify swapchain and depth texture handles
    assert_eq!(device.swapchain_texture.width, 1024);
    assert_eq!(device.swapchain_texture.height, 768);
    assert!(!device.swapchain_texture.is_depth_stencil);

    assert_eq!(device.depth_texture.width, 1024);
    assert_eq!(device.depth_texture.height, 768);
    assert!(device.depth_texture.is_depth_stencil);

    // Buffer creation and write_bytes
    let mut vbo = device.create_buffer(1024, GpuBufferUsage::Vertex);
    assert_eq!(vbo.size, 1024);
    assert_eq!(vbo.usage, GpuBufferUsage::Vertex);
    let sample_bytes = [1u8, 2, 3, 4, 5, 6, 7, 8];
    vbo.write_bytes(16, &sample_bytes);
    assert_eq!(&vbo.data[16..24], &sample_bytes);

    // Texture creation and write_pixels
    let mut tex = device.create_texture(64, 64, SwapchainFormat::Rgba8Unorm, false);
    assert_eq!(tex.width, 64);
    assert_eq!(tex.height, 64);
    assert_eq!(tex.data.len(), 64 * 64 * 4);
    let red_pixel = [255u8, 0, 0, 255];
    tex.write_pixels(&red_pixel);
    assert_eq!(&tex.data[..4], &red_pixel);

    // Command encoding and submission
    let mut encoder = device.create_command_encoder();
    let pass_desc = GpuRenderPassDescriptor {
        color_texture_id: device.swapchain_texture.id,
        depth_texture_id: Some(device.depth_texture.id),
        clear_color: Some([0.1, 0.2, 0.3, 1.0]),
        clear_depth: Some(1.0),
        scissor_rect: Some(Rect::new(0.0, 0.0, 512.0, 512.0)),
    };
    encoder.begin_render_pass(pass_desc);
    encoder.draw_instanced(6, 10);
    encoder.draw_indexed(36, 2);
    encoder.end_render_pass();

    assert_eq!(encoder.draw_calls, 2);
    assert_eq!(encoder.triangles_drawn, 20 + 24);

    device.submit(encoder);
    assert_eq!(device.frame_index, 1);

    // Resize device
    device.resize(1920, 1080).expect("Resize must succeed");
    assert_eq!(device.swapchain_width, 1920);
    assert_eq!(device.swapchain_height, 1080);
}

#[test]
fn test_gpu_3d_pipeline_and_uniforms() {
    let mut device = GpuDevice::new(800, 600, SwapchainFormat::Bgra8Unorm);
    let mut pipeline = Gpu3DPipeline::new(&device, 800, 600);

    assert!(WGSL_3D_STANDARD_SHADER.contains("vs_main"));
    assert!(WGSL_3D_STANDARD_SHADER.contains("fs_main"));
    assert!(WGSL_3D_STANDARD_SHADER.contains("struct Uniforms"));

    // Verify uniforms byte representation
    let uniforms = Gpu3DUniforms::default();
    let bytes = uniforms.as_bytes();
    assert_eq!(bytes.len(), std::mem::size_of::<Gpu3DUniforms>());

    // Build a 3D scene
    let mut scene = Scene3D::new();
    scene.camera = Camera3D::Perspective {
        fov_horizontal_deg: 75.0,
        near: 0.1,
        far: 100.0,
        position: [0.0, 2.0, 5.0],
        target: [0.0, 0.0, 0.0],
    };
    scene.add_light(Light3D::Ambient {
        color: Color::rgb(50, 50, 60),
        intensity: 0.3,
    });
    scene.add_light(Light3D::Directional {
        position: [1.0, 2.0, 1.0],
        color: Color::WHITE,
        intensity: 0.8,
        cast_shadow: true,
    });

    let mesh = Mesh3D::new(
        Geometry3D::Box {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        },
        Material3D::Standard {
            color: Color::rgb(200, 40, 40),
            roughness: 0.3,
            metalness: 0.7,
            emissive: None,
        },
    )
    .with_transform(Transform3D {
        position: [0.0, 0.5, 0.0],
        rotation: [0.0, 0.785, 0.0],
        scale: [1.0, 1.0, 1.0],
    });

    scene.add_mesh(mesh);

    // Render scene with GPU pipeline
    pipeline.render_scene(&mut device, &scene, Rect::new(0.0, 0.0, 800.0, 600.0), None);

    assert_eq!(device.frame_index, 1);
}

#[test]
fn test_gpu_texture_atlas_shelf_packing_and_lru_eviction() {
    let device = GpuDevice::new(256, 256, SwapchainFormat::Rgba8Unorm);
    let mut atlas = GpuTextureAtlas::new(&device, 256, 256, SwapchainFormat::Rgba8Unorm);
    assert_eq!(atlas.width, 256);
    assert_eq!(atlas.height, 256);
    assert_eq!(atlas.allocated_region_count(), 0);

    // Allocate 3 glyph regions
    let r1 = atlas
        .allocate(32, 32, 1)
        .expect("Allocation 1 must succeed");
    assert_eq!(r1.width, 32);
    assert_eq!(r1.height, 32);
    assert_eq!(r1.x, 0);
    assert_eq!(r1.y, 0);
    assert_eq!(r1.uv_bounds, [0.0, 0.0, 32.0 / 256.0, 32.0 / 256.0]);

    let r2 = atlas
        .allocate(64, 32, 1)
        .expect("Allocation 2 must succeed");
    assert_eq!(r2.x, 32);
    assert_eq!(r2.y, 0);

    let _r3 = atlas
        .allocate(128, 64, 1)
        .expect("Allocation 3 must succeed");
    assert_eq!(atlas.allocated_region_count(), 3);

    // Touch r1 and r2 on frame 10
    atlas.touch(r1.id, 10);
    atlas.touch(r2.id, 10);

    // Evict regions older than frame 5
    atlas.evict_lru(10);
    // r3 was allocated at frame 1 and never touched, so it should be evicted
    let (_, _, evictions) = atlas.stats();
    assert_eq!(evictions, 1);
    assert_eq!(atlas.allocated_region_count(), 2);

    // Deallocate r1
    atlas.deallocate(r1.id);
    assert_eq!(atlas.allocated_region_count(), 1);
}

#[test]
fn test_backdrop_filter_separable_gaussian_blur() {
    let mut device = GpuDevice::new(64, 64, SwapchainFormat::Bgra8Unorm);
    let mut pipeline = BackdropFilterPipeline::new(&device, 64, 64);
    assert_eq!(pipeline.width, 64);
    assert_eq!(pipeline.height, 64);

    // Test GPU blur command recording
    let swapchain_tex_id = device.swapchain_texture.id;
    pipeline.execute_blur(
        &mut device,
        swapchain_tex_id,
        4.0,
        Some(Rect::new(16.0, 16.0, 32.0, 32.0)),
    );
    assert_eq!(device.frame_index, 1);

    let mut fb = FrameBuffer::new(64, 64);
    // Fill half of the framebuffer with red, half with green
    for y in 0..64 {
        for x in 0..64 {
            let color = if x < 32 {
                Color::rgb(255, 0, 0)
            } else {
                Color::rgb(0, 255, 0)
            };
            fb.set_pixel(x, y, color);
        }
    }

    // Apply software Gaussian blur backdrop filter
    pipeline.apply_software_blur(&mut fb, Rect::new(16.0, 16.0, 32.0, 32.0), 4.0);

    // Center pixel should be a blended mixture between red and green
    let center_pixel = fb.get_pixel(32, 32);
    assert!(
        center_pixel.r > 50,
        "Red component should be blurred into center"
    );
    assert!(
        center_pixel.g > 50,
        "Green component should be blurred into center"
    );
}

#[test]
fn test_multi_window_compositor_damage_and_swapchains() {
    let mut compositor = MultiWindowCompositor::new();
    assert_eq!(compositor.window_count(), 0);

    // Register two windows
    compositor.register_window(1, 400, 300, 1.0);
    compositor.register_window(2, 800, 600, 2.0);

    assert_eq!(compositor.window_count(), 2);
    assert!(compositor.contains_window(1));
    assert!(compositor.contains_window(2));
    assert!(!compositor.contains_window(3));

    // Mark damage rect on window 1
    compositor.mark_damage(1, Rect::new(10.0, 10.0, 100.0, 50.0));

    // Create a display list
    let mut dl = DisplayList::new();
    dl.push(DrawCommand::DrawRect {
        bounds: Rect::new(10.0, 10.0, 100.0, 50.0),
        color: Color::rgb(0, 120, 215),
    });

    // Render and present window 1
    let buf1 = compositor
        .render_and_present(1, &dl, false)
        .expect("Presentation for window 1 must succeed");
    assert_eq!(buf1.len(), 400 * 300 * 4);

    // Render and present window 2 with forced full redraw
    let buf2 = compositor
        .render_and_present(2, &dl, true)
        .expect("Presentation for window 2 must succeed");
    assert_eq!(buf2.len(), 800 * 600 * 4);

    // Resize window 1
    compositor
        .resize_window(1, 500, 400)
        .expect("Resize window 1 must succeed");

    // Unregister window 2
    compositor.unregister_window(2);
    assert_eq!(compositor.window_count(), 1);
    assert!(!compositor.contains_window(2));
}

#[test]
fn test_gpu_adapter_enumeration_and_device_request() {
    let adapters = GpuAdapter::enumerate_adapters();
    assert!(
        !adapters.is_empty(),
        "Must enumerate at least one GPU adapter or software fallback"
    );

    let primary = &adapters[0];
    assert!(!primary.name.is_empty());
    assert!(primary.limits.max_texture_dimension_2d >= 4096);
    assert!(primary.limits.max_storage_buffer_binding_size > 0);
    assert!(primary.features.sdf_instanced_quads);
    assert!(primary.features.slug_analytical_fonts);

    // Request a device from the primary adapter
    let device = primary
        .request_device(1280, 720, SwapchainFormat::Bgra8Unorm)
        .expect("Request device must succeed");
    assert_eq!(device.swapchain_width, 1280);
    assert_eq!(device.swapchain_height, 720);
    assert_eq!(device.swapchain_format, SwapchainFormat::Bgra8Unorm);
    assert_eq!(device.backend, primary.backend);
    assert_eq!(device.adapter_name, primary.name);
}

#[test]
fn test_gpu_swapchain_modes_and_presentation_lifecycle() {
    let mut swapchain = GpuSwapchain::new(
        1920,
        1080,
        SwapchainFormat::Bgra8Unorm,
        GpuPresentMode::Fifo,
    );
    assert_eq!(swapchain.width, 1920);
    assert_eq!(swapchain.height, 1080);
    assert_eq!(swapchain.present_mode, GpuPresentMode::Fifo);
    assert_eq!(swapchain.textures.len(), 3);
    assert_eq!(swapchain.presented_count, 0);

    // Cycle through textures
    let t0_idx = swapchain.current_index;
    let t1 = swapchain.acquire_next_texture().expect("Texture 1 acquire");
    assert_eq!(t1.width, 1920);
    assert_ne!(swapchain.current_index, t0_idx);

    let _t2 = swapchain.acquire_next_texture().expect("Texture 2 acquire");
    let _t3 = swapchain.acquire_next_texture().expect("Texture 3 acquire");
    assert_eq!(swapchain.current_index, t0_idx, "Must cycle back to 0");

    // Present frames
    swapchain.present().expect("Present 1 must succeed");
    swapchain.present().expect("Present 2 must succeed");
    assert_eq!(swapchain.presented_count, 2);

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
fn test_gpu_surface_presenter_display_list_and_damage_clipping() {
    let mut presenter = GpuSurfacePresenter::new(800, 600, SwapchainFormat::Bgra8Unorm);
    assert_eq!(presenter.surface_size(), (800, 600));
    assert_eq!(presenter.presented_frames, 0);
    assert_eq!(presenter.damage_clipped_frames, 0);

    // DisplayList presentation
    let mut dl = DisplayList::new();
    dl.push(DrawCommand::DrawRect {
        bounds: Rect::new(0.0, 0.0, 800.0, 600.0),
        color: Color::rgb(20, 24, 30),
    });
    dl.push(DrawCommand::DrawText {
        origin: (50.0, 100.0),
        bounds_width: None,
        bounds_height: None,
        text_align: strata_style::TextAlign::Left,
        text: "Strata GPU Presenter".to_string(),
        font_size: 24.0,
        line_height: 28.0,
        font_weight: strata_style::FontWeight::Normal,
        color: Color::WHITE,
        is_monospace: false,
        font_family: strata_primitives::AtomId::FONT_SANS,
    });

    // Present full display list
    presenter
        .present_display_list(&dl, None)
        .expect("Full DL present must succeed");
    assert_eq!(presenter.presented_frames, 1);
    assert_eq!(presenter.damage_clipped_frames, 0);

    // Present damaged display list
    let damage = Rect::new(50.0, 50.0, 200.0, 100.0);
    presenter
        .present_display_list(&dl, Some(damage))
        .expect("Damaged DL present must succeed");
    assert_eq!(presenter.presented_frames, 2);
    assert_eq!(presenter.damage_clipped_frames, 1);

    // Software framebuffer presentation through SurfacePresenter trait
    let mut fb = FrameBuffer::new(800, 600);
    fb.set_pixel(100, 100, Color::rgb(255, 0, 0));
    fb.set_pixel(200, 200, Color::rgb(0, 255, 0));

    SurfacePresenter::present(&mut presenter, &fb, None).expect("FB present must succeed");
    assert_eq!(presenter.presented_frames, 3);

    // Verify BGRA channel conversion in backing buffer:
    // Pixel (100, 100) -> Red: BGRA format should have B=0, G=0, R=255, A=255
    let pixel_idx = (100 * 800 + 100) * 4;
    assert_eq!(presenter.backing_buffer[pixel_idx], 0, "Blue channel");
    assert_eq!(presenter.backing_buffer[pixel_idx + 1], 0, "Green channel");
    assert_eq!(presenter.backing_buffer[pixel_idx + 2], 255, "Red channel");
    assert_eq!(
        presenter.backing_buffer[pixel_idx + 3],
        255,
        "Alpha channel"
    );

    // Present with damage
    SurfacePresenter::present(&mut presenter, &fb, Some(damage))
        .expect("FB damaged present must succeed");
    assert_eq!(presenter.presented_frames, 4);
    assert_eq!(presenter.damage_clipped_frames, 2);

    // Resize
    SurfacePresenter::resize(&mut presenter, 1024, 768).expect("Resize must succeed");
    assert_eq!(presenter.surface_size(), (1024, 768));
    assert_eq!(presenter.backing_buffer.len(), 1024 * 768 * 4);
}

#[test]
fn test_gpu_slug_text_pipeline_compilation_and_bezier_evaluation() {
    let mut text_pipeline = GpuSlugTextPipeline::new();
    assert_eq!(text_pipeline.total_glyphs, 0);
    assert_eq!(text_pipeline.total_curves, 0);

    // Check analytical glyph cache
    let glyph_a = text_pipeline.glyph_cache.get('A');
    assert!(glyph_a.is_some(), "Glyph cache must contain ASCII 'A'");
    let glyph_a = glyph_a.unwrap();
    assert!(glyph_a.advance_width > 0.0);
    assert!(
        !glyph_a.curves.is_empty(),
        "Glyph 'A' must have analytical curves"
    );

    // Evaluate signed distance field on glyph contour
    let sdf_dist = text_pipeline
        .glyph_cache
        .evaluate_glyph_sdf('A', Point2D::new(0.5, 0.5));
    assert!(sdf_dist.is_some(), "Must evaluate SDF distance to contour");

    // Compile DisplayList with DrawText
    let mut dl = DisplayList::new();
    dl.push(DrawCommand::DrawText {
        origin: (10.0, 20.0),
        bounds_width: None,
        bounds_height: None,
        text_align: strata_style::TextAlign::Left,
        text: "Strata Analytical Slug".to_string(),
        font_size: 16.0,
        line_height: 20.0,
        font_weight: strata_style::FontWeight::Normal,
        color: Color::rgb(240, 240, 245),
        is_monospace: false,
        font_family: strata_primitives::AtomId::FONT_SANS,
    });

    let glyph_count = text_pipeline.compile_display_list(&dl, 1920.0, 1080.0, None);
    assert_eq!(glyph_count, 22); // "Strata Analytical Slug" has 22 chars
    assert!(text_pipeline.total_curves > 0);
    assert_eq!(text_pipeline.buffer.instances.len(), 22);
    assert_eq!(
        text_pipeline.buffer.curves.len(),
        text_pipeline.total_curves
    );

    // Damage scissoring: Damage rect outside the text bounds
    let outside_damage = Rect::new(500.0, 500.0, 100.0, 100.0);
    let culled_count =
        text_pipeline.compile_display_list(&dl, 1920.0, 1080.0, Some(outside_damage));
    assert_eq!(culled_count, 0, "Text outside damage rect must be culled");

    // Compile DrawTextFragments
    let mut dl_frag = DisplayList::new();
    let frag = TextFragment {
        bounds: Rect::new(10.0, 50.0, 100.0, 20.0),
        text: "Fragment".to_string(),
        baseline_y: 16.0,
        style: InlineTextStyle {
            color: Color::rgb(100, 200, 255),
            font_size: 14.0,
            ..Default::default()
        },
        font_family: strata_primitives::AtomId::FONT_SANS,
        item_index: 0,
    };
    dl_frag.push(DrawCommand::DrawTextFragments {
        fragments: vec![frag],
    });

    let frag_glyph_count = text_pipeline.compile_display_list(&dl_frag, 1920.0, 1080.0, None);
    assert_eq!(frag_glyph_count, 8); // "Fragment" has 8 chars
    assert!(text_pipeline.total_curves > 0);
}
