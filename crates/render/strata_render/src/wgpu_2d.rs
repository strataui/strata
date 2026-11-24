//! Hardware `wgpu` 2D, Stencil Clip, Analytical Vector Path, and Slug Text Presenter Pipelines.
//!
//! Executes [`crate::gpu_pipeline::Hardware2DFrame`] batches compiled by
//! [`crate::gpu_pipeline::UnifiedGpuCollector`] directly on real `wgpu` hardware surfaces and
//! offscreen render targets, wiring [`WGSL_ANALYTICAL_QUAD_SHADER`],
//! [`WGSL_STENCIL_CLIP_SHADER`], [`strata_gpu_path::WGSL_ANALYTICAL_PATH_RASTER_SHADER`], and
//! [`strata_gpu_path::font_shader::WGSL_SLUG_FONT_SHADER`].

use crate::gpu_pipeline::{
    GpuClipVertex, GpuQuadInstance, GpuUniforms, GpuVertex, Hardware2DFrame, HardwareDrawOpKind,
    HardwareRenderItem, QUAD_VERTICES, SIGNED_QUAD_VERTICES, WGSL_ANALYTICAL_QUAD_SHADER,
    WGSL_STENCIL_CLIP_SHADER,
};
use std::ops::Range;
use std::sync::Arc;
use strata_layout::Rect;

const MAX_STENCIL_LAYERS: usize = 8;

/// Uniform layout shared across `WGSL_ANALYTICAL_QUAD_SHADER`, `WGSL_STENCIL_CLIP_SHADER`,
/// `WGSL_ANALYTICAL_PATH_RASTER_SHADER`, and `WGSL_SLUG_FONT_SHADER` (first vec2 is `viewport_size`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct Gpu2DUniformData {
    viewport_size: [f32; 2],
    dpr: f32,
    _pad: f32,
}

impl Gpu2DUniformData {
    #[inline]
    fn as_bytes(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                (self as *const Self) as *const u8,
                std::mem::size_of::<Self>(),
            )
        }
    }
}

/// Hardware `wgpu` polygonal/even-odd stencil clip pipeline built from [`WGSL_STENCIL_CLIP_SHADER`].
///
/// Pre-builds `StencilOperation::Invert` write pipelines across bits `0..8` of the
/// `Depth24PlusStencil8` attachment so nested arbitrary clip layers compose via exact
/// hardware even-odd intersection.
pub struct GpuStencilClipPipeline {
    uniform_bgl: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    shader: wgpu::ShaderModule,
    color_format: wgpu::TextureFormat,
    invert_bit_pipelines: [wgpu::RenderPipeline; MAX_STENCIL_LAYERS],
}

impl GpuStencilClipPipeline {
    /// Constructs a hardware stencil clip pipeline on `device` matching `color_format`.
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
        let uniform_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("strata_2d_uniform_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("strata_2d_stencil_clip_pl"),
            bind_group_layouts: &[&uniform_bgl],
            push_constant_ranges: &[],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("strata_2d_stencil_clip_wgsl"),
            source: wgpu::ShaderSource::Wgsl(WGSL_STENCIL_CLIP_SHADER.into()),
        });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: GpuClipVertex::BYTE_SIZE as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            }],
        };

        let invert_bit_pipelines = std::array::from_fn(|bit_idx| {
            let bit_mask = 1u32 << bit_idx;
            let invert_face = wgpu::StencilFaceState {
                compare: wgpu::CompareFunction::Always,
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op: wgpu::StencilOperation::Invert,
            };
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("strata_2d_stencil_clip_bit_pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: std::slice::from_ref(&vertex_layout),
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: color_format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::empty(),
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth24PlusStencil8,
                    depth_write_enabled: false,
                    depth_compare: wgpu::CompareFunction::Always,
                    stencil: wgpu::StencilState {
                        front: invert_face,
                        back: invert_face,
                        read_mask: bit_mask,
                        write_mask: bit_mask,
                    },
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            })
        });

        Self {
            uniform_bgl,
            pipeline_layout,
            shader,
            color_format,
            invert_bit_pipelines,
        }
    }

    #[inline]
    pub fn uniform_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.uniform_bgl
    }

    #[inline]
    pub fn uniform_pipeline_layout(&self) -> &wgpu::PipelineLayout {
        &self.pipeline_layout
    }

    #[inline]
    pub fn color_format(&self) -> wgpu::TextureFormat {
        self.color_format
    }

    #[inline]
    pub fn shader(&self) -> &wgpu::ShaderModule {
        &self.shader
    }

    #[inline]
    pub fn layer_pipeline(&self, layer_bit: usize) -> &wgpu::RenderPipeline {
        &self.invert_bit_pipelines[layer_bit.min(MAX_STENCIL_LAYERS - 1)]
    }

    /// Writes all active even-odd stencil layers `(vertex_byte_range, vertex_count)` into the
    /// stencil attachment and returns the packed reference bitmask `(1 << count) - 1`.
    fn record_mask<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        uniform_bg: &'pass wgpu::BindGroup,
        stencil_vbuf: &'pass wgpu::Buffer,
        layers: &[(Range<u64>, u32)],
    ) -> usize {
        let mut active_layers = 0usize;
        for (byte_range, vert_count) in layers.iter().take(MAX_STENCIL_LAYERS) {
            if *vert_count == 0 {
                continue;
            }
            pass.set_pipeline(&self.invert_bit_pipelines[active_layers]);
            pass.set_bind_group(0, uniform_bg, &[]);
            pass.set_vertex_buffer(0, stencil_vbuf.slice(byte_range.clone()));
            pass.set_stencil_reference(0);
            pass.draw(0..*vert_count, 0..1);
            active_layers += 1;
        }
        active_layers
    }
}

struct StencilDepthTarget {
    width: u32,
    height: u32,
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
}

struct Offscreen2DTarget {
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    color: wgpu::Texture,
    view: wgpu::TextureView,
    readback: wgpu::Buffer,
    padded_bytes_per_row: u32,
}

/// Hardware `wgpu` 2D Batch, Stencil Clip, Vector Path, and Slug Text Renderer.
///
/// Consumes [`Hardware2DFrame`] batches compiled by [`crate::gpu_pipeline::UnifiedGpuCollector`]
/// and renders analytical quads, dashed strokes, vector path caps/joins, Slug glyphs, and
/// non-rectangular stencil clip masks directly into swapchain or offscreen `wgpu::TextureView`s.
pub struct Wgpu2DRenderer {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    color_format: wgpu::TextureFormat,

    stencil_clip_pipeline: GpuStencilClipPipeline,

    quad_shader: wgpu::ShaderModule,
    quad_pipeline: wgpu::RenderPipeline,
    quad_stenciled: [Option<wgpu::RenderPipeline>; MAX_STENCIL_LAYERS],

    path_shader: wgpu::ShaderModule,
    path_pipeline: wgpu::RenderPipeline,
    path_stenciled: [Option<wgpu::RenderPipeline>; MAX_STENCIL_LAYERS],

    slug_shader: wgpu::ShaderModule,
    slug_bgl: wgpu::BindGroupLayout,
    slug_pl: wgpu::PipelineLayout,
    slug_pipeline: wgpu::RenderPipeline,
    slug_stenciled: [Option<wgpu::RenderPipeline>; MAX_STENCIL_LAYERS],
    slug_bind_group: wgpu::BindGroup,

    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,

    unit_quad_vbuf: wgpu::Buffer,
    signed_quad_vbuf: wgpu::Buffer,
    quad_ibuf: wgpu::Buffer,
    quad_ibuf_cap_bytes: u64,
    path_seg_ibuf: wgpu::Buffer,
    path_seg_ibuf_cap_bytes: u64,
    slug_inst_ibuf: wgpu::Buffer,
    slug_inst_ibuf_cap_bytes: u64,
    slug_curve_sbuf: wgpu::Buffer,
    slug_curve_sbuf_cap_bytes: u64,
    stencil_vbuf: wgpu::Buffer,
    stencil_vbuf_cap_bytes: u64,

    stencil_target: Option<StencilDepthTarget>,
    offscreen: Option<Offscreen2DTarget>,
    last_stencil_ranges: Vec<Vec<(Range<u64>, u32)>>,
}

#[inline]
fn create_growable_buffer(
    device: &wgpu::Device,
    label: &str,
    byte_size: u64,
    usage: wgpu::BufferUsages,
) -> (wgpu::Buffer, u64) {
    let cap = byte_size.max(64).next_power_of_two();
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: cap,
        usage,
        mapped_at_creation: false,
    });
    (buf, cap)
}

#[inline]
fn upload_growable(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    usage: wgpu::BufferUsages,
    buf: &mut wgpu::Buffer,
    cap_bytes: &mut u64,
    bytes: &[u8],
) -> bool {
    if bytes.is_empty() {
        return false;
    }
    let needed = bytes.len() as u64;
    let mut reallocated = false;
    if needed > *cap_bytes {
        let (next_buf, next_cap) = create_growable_buffer(device, label, needed, usage);
        *buf = next_buf;
        *cap_bytes = next_cap;
        reallocated = true;
    }
    queue.write_buffer(buf, 0, bytes);
    reallocated
}

#[inline]
fn quad_vertex_bytes(verts: &[GpuVertex; 6]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(verts.as_ptr() as *const u8, std::mem::size_of_val(verts))
    }
}

#[inline]
fn make_unit_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<GpuVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
        }],
    }
}

#[inline]
fn make_quad_instance_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRS: [wgpu::VertexAttribute; 6] = [
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 0,
            shader_location: 1,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 16,
            shader_location: 2,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 32,
            shader_location: 3,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 48,
            shader_location: 4,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 64,
            shader_location: 5,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 80,
            shader_location: 6,
        },
    ];
    wgpu::VertexBufferLayout {
        array_stride: GpuQuadInstance::BYTE_SIZE as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &ATTRS,
    }
}

#[inline]
fn make_path_instance_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRS: [wgpu::VertexAttribute; 7] = [
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: 0,
            shader_location: 1,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: 8,
            shader_location: 2,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: 16,
            shader_location: 3,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32,
            offset: 24,
            shader_location: 4,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint32,
            offset: 28,
            shader_location: 5,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 32,
            shader_location: 6,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32,
            offset: 48,
            shader_location: 7,
        },
    ];
    wgpu::VertexBufferLayout {
        array_stride: strata_gpu_path::GpuPathSegment::BYTE_SIZE as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &ATTRS,
    }
}

#[inline]
fn make_slug_instance_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRS: [wgpu::VertexAttribute; 5] = [
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 0,
            shader_location: 1,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 16,
            shader_location: 2,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint32x2,
            offset: 32,
            shader_location: 3,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 40,
            shader_location: 4,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 56,
            shader_location: 5,
        },
    ];
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<strata_gpu_path::GpuGlyphInstance>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &ATTRS,
    }
}

#[inline]
fn build_2d_pipeline(
    device: &wgpu::Device,
    label: &str,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    buffers: &[wgpu::VertexBufferLayout<'_>],
    color_format: wgpu::TextureFormat,
    blend: wgpu::BlendState,
    stencil_layer_count: Option<usize>,
) -> wgpu::RenderPipeline {
    let stencil = match stencil_layer_count {
        None => wgpu::StencilState {
            front: wgpu::StencilFaceState::IGNORE,
            back: wgpu::StencilFaceState::IGNORE,
            read_mask: 0,
            write_mask: 0,
        },
        Some(n) => {
            let layers = n.clamp(1, MAX_STENCIL_LAYERS);
            let mask_bits = ((1u32 << layers) - 1) & 0xFF;
            let eq_face = wgpu::StencilFaceState {
                compare: wgpu::CompareFunction::Equal,
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op: wgpu::StencilOperation::Keep,
            };
            wgpu::StencilState {
                front: eq_face,
                back: eq_face,
                read_mask: mask_bits,
                write_mask: 0,
            }
        }
    };

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers,
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: Some(blend),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth24PlusStencil8,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::Always,
            stencil,
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

fn create_static_vertex_buffer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    bytes: &[u8],
) -> wgpu::Buffer {
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: bytes.len() as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&buf, 0, bytes);
    buf
}

fn compute_scissor_pixels(
    op_scissor: Option<Rect>,
    damage: Option<Rect>,
    target_width: u32,
    target_height: u32,
    logical_w: f32,
    logical_h: f32,
) -> Option<[u32; 4]> {
    let mut r = Rect::new(0.0, 0.0, logical_w.max(1.0), logical_h.max(1.0));
    if let Some(s) = op_scissor {
        r = r.intersect(s)?;
    }
    if let Some(d) = damage {
        r = r.intersect(d)?;
    }
    let sx = target_width as f32 / logical_w.max(1.0);
    let sy = target_height as f32 / logical_h.max(1.0);
    let x0 = (r.x * sx).floor().max(0.0) as u32;
    let y0 = (r.y * sy).floor().max(0.0) as u32;
    let x1 = ((r.x + r.width) * sx).ceil().min(target_width as f32) as u32;
    let y1 = ((r.y + r.height) * sy).ceil().min(target_height as f32) as u32;
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some([x0, y0, x1 - x0, y1 - y0])
}

impl Wgpu2DRenderer {
    /// Creates a hardware 2D renderer sharing an existing `wgpu::Device` & `wgpu::Queue`.
    pub fn from_device(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        color_format: wgpu::TextureFormat,
    ) -> Self {
        let stencil_clip_pipeline = GpuStencilClipPipeline::new(&device, color_format);

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("strata_2d_uniforms"),
            size: GpuUniforms::BYTE_SIZE as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("strata_2d_uniform_bg"),
            layout: stencil_clip_pipeline.uniform_bind_group_layout(),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let unit_quad_vbuf = create_static_vertex_buffer(
            &device,
            &queue,
            "strata_2d_unit_quad_vbuf",
            quad_vertex_bytes(&QUAD_VERTICES),
        );
        let signed_quad_vbuf = create_static_vertex_buffer(
            &device,
            &queue,
            "strata_2d_signed_quad_vbuf",
            quad_vertex_bytes(&SIGNED_QUAD_VERTICES),
        );

        let unit_layout = make_unit_vertex_layout();
        let quad_inst_layout = make_quad_instance_layout();
        let path_inst_layout = make_path_instance_layout();
        let slug_inst_layout = make_slug_instance_layout();

        let quad_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("strata_2d_analytical_quad_wgsl"),
            source: wgpu::ShaderSource::Wgsl(WGSL_ANALYTICAL_QUAD_SHADER.into()),
        });
        let quad_pipeline = build_2d_pipeline(
            &device,
            "strata_2d_quad_pipeline",
            stencil_clip_pipeline.uniform_pipeline_layout(),
            &quad_shader,
            &[unit_layout.clone(), quad_inst_layout.clone()],
            color_format,
            wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING,
            None,
        );
        let quad_l1 = build_2d_pipeline(
            &device,
            "strata_2d_quad_stenciled_pipeline",
            stencil_clip_pipeline.uniform_pipeline_layout(),
            &quad_shader,
            &[unit_layout.clone(), quad_inst_layout],
            color_format,
            wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING,
            Some(1),
        );
        let quad_stenciled = [Some(quad_l1), None, None, None, None, None, None, None];

        let path_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("strata_2d_analytical_path_wgsl"),
            source: wgpu::ShaderSource::Wgsl(
                strata_gpu_path::WGSL_ANALYTICAL_PATH_RASTER_SHADER.into(),
            ),
        });
        let path_pipeline = build_2d_pipeline(
            &device,
            "strata_2d_path_pipeline",
            stencil_clip_pipeline.uniform_pipeline_layout(),
            &path_shader,
            &[unit_layout.clone(), path_inst_layout.clone()],
            color_format,
            wgpu::BlendState::ALPHA_BLENDING,
            None,
        );
        let path_l1 = build_2d_pipeline(
            &device,
            "strata_2d_path_stenciled_pipeline",
            stencil_clip_pipeline.uniform_pipeline_layout(),
            &path_shader,
            &[unit_layout.clone(), path_inst_layout],
            color_format,
            wgpu::BlendState::ALPHA_BLENDING,
            Some(1),
        );
        let path_stenciled = [Some(path_l1), None, None, None, None, None, None, None];

        let slug_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("strata_2d_slug_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let slug_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("strata_2d_slug_pl"),
            bind_group_layouts: &[&slug_bgl],
            push_constant_ranges: &[],
        });
        let slug_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("strata_2d_slug_wgsl"),
            source: wgpu::ShaderSource::Wgsl(strata_gpu_path::WGSL_SLUG_FONT_SHADER.into()),
        });
        let slug_pipeline = build_2d_pipeline(
            &device,
            "strata_2d_slug_pipeline",
            &slug_pl,
            &slug_shader,
            &[unit_layout.clone(), slug_inst_layout.clone()],
            color_format,
            wgpu::BlendState::ALPHA_BLENDING,
            None,
        );
        let slug_l1 = build_2d_pipeline(
            &device,
            "strata_2d_slug_stenciled_pipeline",
            &slug_pl,
            &slug_shader,
            &[unit_layout, slug_inst_layout],
            color_format,
            wgpu::BlendState::ALPHA_BLENDING,
            Some(1),
        );
        let slug_stenciled = [Some(slug_l1), None, None, None, None, None, None, None];

        let (quad_ibuf, quad_ibuf_cap_bytes) = create_growable_buffer(
            &device,
            "strata_2d_quad_instances",
            4096,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let (path_seg_ibuf, path_seg_ibuf_cap_bytes) = create_growable_buffer(
            &device,
            "strata_2d_path_segments",
            4096,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let (slug_inst_ibuf, slug_inst_ibuf_cap_bytes) = create_growable_buffer(
            &device,
            "strata_2d_slug_instances",
            4096,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let (slug_curve_sbuf, slug_curve_sbuf_cap_bytes) = create_growable_buffer(
            &device,
            "strata_2d_slug_curves",
            4096,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );
        let (stencil_vbuf, stencil_vbuf_cap_bytes) = create_growable_buffer(
            &device,
            "strata_2d_stencil_vertices",
            4096,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );

        let slug_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("strata_2d_slug_bg"),
            layout: &slug_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: slug_curve_sbuf.as_entire_binding(),
                },
            ],
        });

        Self {
            device,
            queue,
            color_format,
            stencil_clip_pipeline,
            quad_shader,
            quad_pipeline,
            quad_stenciled,
            path_shader,
            path_pipeline,
            path_stenciled,
            slug_shader,
            slug_bgl,
            slug_pl,
            slug_pipeline,
            slug_stenciled,
            slug_bind_group,
            uniform_buffer,
            uniform_bind_group,
            unit_quad_vbuf,
            signed_quad_vbuf,
            quad_ibuf,
            quad_ibuf_cap_bytes,
            path_seg_ibuf,
            path_seg_ibuf_cap_bytes,
            slug_inst_ibuf,
            slug_inst_ibuf_cap_bytes,
            slug_curve_sbuf,
            slug_curve_sbuf_cap_bytes,
            stencil_vbuf,
            stencil_vbuf_cap_bytes,
            stencil_target: None,
            offscreen: None,
            last_stencil_ranges: Vec::new(),
        }
    }

    #[inline]
    pub fn device(&self) -> &Arc<wgpu::Device> {
        &self.device
    }

    #[inline]
    pub fn queue(&self) -> &Arc<wgpu::Queue> {
        &self.queue
    }

    #[inline]
    pub fn color_format(&self) -> wgpu::TextureFormat {
        self.color_format
    }

    /// Returns the underlying hardware stencil clip pipeline bound to [`WGSL_STENCIL_CLIP_SHADER`].
    #[inline]
    pub fn stencil_clip_pipeline(&self) -> &GpuStencilClipPipeline {
        &self.stencil_clip_pipeline
    }

    /// Total GPU buffer & render-target memory allocated by this 2D renderer in bytes.
    pub fn gpu_memory_bytes(&self) -> u64 {
        let mut bytes = self.uniform_buffer.size()
            + self.unit_quad_vbuf.size()
            + self.signed_quad_vbuf.size()
            + self.quad_ibuf_cap_bytes
            + self.path_seg_ibuf_cap_bytes
            + self.slug_inst_ibuf_cap_bytes
            + self.slug_curve_sbuf_cap_bytes
            + self.stencil_vbuf_cap_bytes;
        if let Some(ref st) = self.stencil_target {
            bytes += (st.width as u64) * (st.height as u64) * 4;
        }
        if let Some(ref off) = self.offscreen {
            bytes += (off.width as u64) * (off.height as u64) * 4
                + (off.padded_bytes_per_row as u64) * (off.height as u64);
        }
        bytes
    }

    fn ensure_stencil_target(&mut self, width: u32, height: u32) {
        let w = width.max(1);
        let h = height.max(1);
        let valid = self
            .stencil_target
            .as_ref()
            .is_some_and(|t| t.width == w && t.height == h);
        if valid {
            return;
        }
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("strata_2d_depth24_stencil8"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24PlusStencil8,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        self.stencil_target = Some(StencilDepthTarget {
            width: w,
            height: h,
            _texture: tex,
            view,
        });
    }

    fn ensure_stenciled_pipelines(&mut self, layer_count: usize) {
        let slot = layer_count.clamp(1, MAX_STENCIL_LAYERS) - 1;
        if self.quad_stenciled[slot].is_none() {
            self.quad_stenciled[slot] = Some(build_2d_pipeline(
                &self.device,
                "strata_2d_quad_stenciled_pipeline",
                self.stencil_clip_pipeline.uniform_pipeline_layout(),
                &self.quad_shader,
                &[make_unit_vertex_layout(), make_quad_instance_layout()],
                self.color_format,
                wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING,
                Some(slot + 1),
            ));
        }
        if self.path_stenciled[slot].is_none() {
            self.path_stenciled[slot] = Some(build_2d_pipeline(
                &self.device,
                "strata_2d_path_stenciled_pipeline",
                self.stencil_clip_pipeline.uniform_pipeline_layout(),
                &self.path_shader,
                &[make_unit_vertex_layout(), make_path_instance_layout()],
                self.color_format,
                wgpu::BlendState::ALPHA_BLENDING,
                Some(slot + 1),
            ));
        }
        if self.slug_stenciled[slot].is_none() {
            self.slug_stenciled[slot] = Some(build_2d_pipeline(
                &self.device,
                "strata_2d_slug_stenciled_pipeline",
                &self.slug_pl,
                &self.slug_shader,
                &[make_unit_vertex_layout(), make_slug_instance_layout()],
                self.color_format,
                wgpu::BlendState::ALPHA_BLENDING,
                Some(slot + 1),
            ));
        }
    }

    /// Uploads frame uniform, quad, path, Slug glyph, and triangulated stencil clip buffers.
    pub fn prepare_frame(&mut self, frame: &Hardware2DFrame, logical_w: f32, logical_h: f32, dpr: f32) {
        let uni = Gpu2DUniformData {
            viewport_size: [logical_w.max(1.0), logical_h.max(1.0)],
            dpr: dpr.max(0.1),
            _pad: 0.0,
        };
        self.queue.write_buffer(&self.uniform_buffer, 0, uni.as_bytes());

        upload_growable(
            &self.device,
            &self.queue,
            "strata_2d_quad_instances",
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            &mut self.quad_ibuf,
            &mut self.quad_ibuf_cap_bytes,
            frame.quads.as_bytes(),
        );
        upload_growable(
            &self.device,
            &self.queue,
            "strata_2d_path_segments",
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            &mut self.path_seg_ibuf,
            &mut self.path_seg_ibuf_cap_bytes,
            frame.paths.as_bytes(),
        );
        upload_growable(
            &self.device,
            &self.queue,
            "strata_2d_slug_instances",
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            &mut self.slug_inst_ibuf,
            &mut self.slug_inst_ibuf_cap_bytes,
            frame.text.instance_bytes(),
        );
        if upload_growable(
            &self.device,
            &self.queue,
            "strata_2d_slug_curves",
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            &mut self.slug_curve_sbuf,
            &mut self.slug_curve_sbuf_cap_bytes,
            frame.text.curve_bytes(),
        ) {
            self.slug_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("strata_2d_slug_bg"),
                layout: &self.slug_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.slug_curve_sbuf.as_entire_binding(),
                    },
                ],
            });
        }

        // Pack all clip masks' triangulated vertex streams into a single vertex buffer.
        self.last_stencil_ranges.clear();
        let total_verts: usize = frame
            .clip_masks
            .iter()
            .map(|m| m.total_stencil_vertices())
            .sum();
        let mut max_layer_count = 1usize;
        if total_verts > 0 {
            let mut packed = Vec::<GpuClipVertex>::with_capacity(total_verts);
            for mask in &frame.clip_masks {
                let mut layer_ranges = Vec::with_capacity(mask.stencil_layers.len());
                let mut non_empty = 0usize;
                for layer in &mask.stencil_layers {
                    if layer.is_empty() {
                        continue;
                    }
                    let start_byte = (packed.len() * GpuClipVertex::BYTE_SIZE) as u64;
                    packed.extend_from_slice(layer);
                    let end_byte = (packed.len() * GpuClipVertex::BYTE_SIZE) as u64;
                    layer_ranges.push((start_byte..end_byte, layer.len() as u32));
                    non_empty += 1;
                }
                max_layer_count = max_layer_count.max(non_empty.min(MAX_STENCIL_LAYERS));
                self.last_stencil_ranges.push(layer_ranges);
            }
            upload_growable(
                &self.device,
                &self.queue,
                "strata_2d_stencil_vertices",
                wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                &mut self.stencil_vbuf,
                &mut self.stencil_vbuf_cap_bytes,
                GpuClipVertex::slice_to_bytes(&packed),
            );
        }
        for n in 1..=max_layer_count {
            self.ensure_stenciled_pipelines(n);
        }
    }

    /// Renders a contiguous range of `frame.items` (`Batch` items only) into `target_view`,
    /// preserving any `Viewport3D` split boundaries handled by a hybrid compositor caller.
    #[allow(clippy::too_many_arguments)]
    pub fn render_batch_slice_to_view(
        &mut self,
        frame: &Hardware2DFrame,
        item_range: Range<usize>,
        target_view: &wgpu::TextureView,
        target_width: u32,
        target_height: u32,
        logical_w: f32,
        logical_h: f32,
        clear_color: Option<[f32; 4]>,
        damage: Option<Rect>,
    ) {
        let w = target_width.max(1);
        let h = target_height.max(1);
        self.ensure_stencil_target(w, h);
        let stencil_view = self.stencil_target.as_ref().unwrap().view.clone();

        let has_batches = frame.items[item_range.clone()]
            .iter()
            .any(|it| matches!(it, HardwareRenderItem::Batch(_)));
        if !has_batches && clear_color.is_none() {
            return;
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("strata_2d_encoder"),
            });

        if !has_batches {
            if let Some(c) = clear_color {
                {
                    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("strata_2d_clear_pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: target_view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color {
                                    r: c[0] as f64,
                                    g: c[1] as f64,
                                    b: c[2] as f64,
                                    a: c[3] as f64,
                                }),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                }
                self.queue.submit(Some(encoder.finish()));
            }
            return;
        }

        // Partition ops into sub-runs that require clearing the hardware stencil buffer
        // whenever switching between distinct non-rectangular clip masks.
        let mut pass_slices: Vec<(usize, usize)> = Vec::new();
        let mut run_start = item_range.start;
        let mut active_stencil_mask: Option<usize> = None;
        for idx in item_range.clone() {
            if let HardwareRenderItem::Batch(op) = frame.items[idx] {
                if let Some(mask_id) = op.clip_mask {
                    if let Some(prev) = active_stencil_mask {
                        if prev != mask_id {
                            pass_slices.push((run_start, idx));
                            run_start = idx;
                            active_stencil_mask = Some(mask_id);
                        }
                    } else {
                        active_stencil_mask = Some(mask_id);
                    }
                }
            }
        }
        pass_slices.push((run_start, item_range.end));

        let quad_bytes = frame.quads.as_bytes();
        let path_bytes = frame.paths.as_bytes();
        let slug_inst_bytes = frame.text.instance_bytes();

        for (pass_idx, (s, e)) in pass_slices.into_iter().enumerate() {
            let color_load = if pass_idx == 0 {
                match clear_color {
                    Some(c) => wgpu::LoadOp::Clear(wgpu::Color {
                        r: c[0] as f64,
                        g: c[1] as f64,
                        b: c[2] as f64,
                        a: c[3] as f64,
                    }),
                    None => wgpu::LoadOp::Load,
                }
            } else {
                wgpu::LoadOp::Load
            };

            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("strata_2d_batch_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: color_load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &stencil_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0),
                        store: wgpu::StoreOp::Discard,
                    }),
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            let mut loaded_mask: Option<(usize, usize)> = None;
            for item in &frame.items[s..e] {
                let HardwareRenderItem::Batch(op) = item else {
                    continue;
                };
                if op.end <= op.start {
                    continue;
                }

                let mut active_layers = 0usize;
                if let Some(mask_id) = op.clip_mask {
                    if let Some((prev_id, prev_layers)) = loaded_mask {
                        if prev_id == mask_id {
                            active_layers = prev_layers;
                        }
                    } else if let Some(layers) = self.last_stencil_ranges.get(mask_id) {
                        if let Some([sx, sy, sw, sh]) = compute_scissor_pixels(
                            None, damage, w, h, logical_w, logical_h,
                        ) {
                            pass.set_scissor_rect(sx, sy, sw, sh);
                            active_layers = self.stencil_clip_pipeline.record_mask(
                                &mut pass,
                                &self.uniform_bind_group,
                                &self.stencil_vbuf,
                                layers,
                            );
                            loaded_mask = Some((mask_id, active_layers));
                        }
                    }
                }

                let Some([sx, sy, sw, sh]) = compute_scissor_pixels(
                    op.scissor_rect,
                    damage,
                    w,
                    h,
                    logical_w,
                    logical_h,
                ) else {
                    continue;
                };
                pass.set_scissor_rect(sx, sy, sw, sh);

                let inst_count = (op.end - op.start) as u32;
                let stencil_ref = if active_layers > 0 {
                    ((1u32 << active_layers.min(MAX_STENCIL_LAYERS)) - 1) & 0xFF
                } else {
                    0
                };

                match op.kind {
                    HardwareDrawOpKind::Quads => {
                        let start_byte = (op.start * GpuQuadInstance::BYTE_SIZE) as u64;
                        let end_byte = (op.end * GpuQuadInstance::BYTE_SIZE) as u64;
                        if end_byte as usize > quad_bytes.len() {
                            continue;
                        }
                        let pipe = if active_layers > 0 {
                            self.quad_stenciled[active_layers - 1]
                                .as_ref()
                                .unwrap_or(&self.quad_pipeline)
                        } else {
                            &self.quad_pipeline
                        };
                        pass.set_pipeline(pipe);
                        pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        pass.set_vertex_buffer(0, self.unit_quad_vbuf.slice(..));
                        pass.set_vertex_buffer(1, self.quad_ibuf.slice(start_byte..end_byte));
                        pass.set_stencil_reference(stencil_ref);
                        pass.draw(0..6, 0..inst_count);
                    }
                    HardwareDrawOpKind::Paths => {
                        let stride = strata_gpu_path::GpuPathSegment::BYTE_SIZE;
                        let start_byte = (op.start * stride) as u64;
                        let end_byte = (op.end * stride) as u64;
                        if end_byte as usize > path_bytes.len() {
                            continue;
                        }
                        let pipe = if active_layers > 0 {
                            self.path_stenciled[active_layers - 1]
                                .as_ref()
                                .unwrap_or(&self.path_pipeline)
                        } else {
                            &self.path_pipeline
                        };
                        pass.set_pipeline(pipe);
                        pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                        pass.set_vertex_buffer(0, self.signed_quad_vbuf.slice(..));
                        pass.set_vertex_buffer(1, self.path_seg_ibuf.slice(start_byte..end_byte));
                        pass.set_stencil_reference(stencil_ref);
                        pass.draw(0..6, 0..inst_count);
                    }
                    HardwareDrawOpKind::Text => {
                        let stride = std::mem::size_of::<strata_gpu_path::GpuGlyphInstance>();
                        let start_byte = (op.start * stride) as u64;
                        let end_byte = (op.end * stride) as u64;
                        if end_byte as usize > slug_inst_bytes.len() {
                            continue;
                        }
                        let pipe = if active_layers > 0 {
                            self.slug_stenciled[active_layers - 1]
                                .as_ref()
                                .unwrap_or(&self.slug_pipeline)
                        } else {
                            &self.slug_pipeline
                        };
                        pass.set_pipeline(pipe);
                        pass.set_bind_group(0, &self.slug_bind_group, &[]);
                        pass.set_vertex_buffer(0, self.unit_quad_vbuf.slice(..));
                        pass.set_vertex_buffer(1, self.slug_inst_ibuf.slice(start_byte..end_byte));
                        pass.set_stencil_reference(stencil_ref);
                        pass.draw(0..6, 0..inst_count);
                    }
                }
            }
        }

        self.queue.submit(Some(encoder.finish()));
    }

    /// Compiles `list` with `collector` and renders all 2D GPU batches (`GpuBatch`, `GpuPathBatch`,
    /// `GpuSlugTextPipeline`, and [`GpuStencilClipPipeline`] clip masks) directly into `target_view`.
    #[allow(clippy::too_many_arguments)]
    pub fn render_display_list_to_view(
        &mut self,
        collector: &mut crate::gpu_pipeline::UnifiedGpuCollector,
        list: &crate::compositor::DisplayList,
        target_view: &wgpu::TextureView,
        target_width: u32,
        target_height: u32,
        dpr: f32,
        clear_color: Option<[f32; 4]>,
        damage: Option<Rect>,
    ) -> crate::gpu_pipeline::GpuBatchSummary {
        let frame = collector.collect_hardware_frame(list, damage);
        self.prepare_frame(&frame, collector.viewport_width, collector.viewport_height, dpr);
        self.render_batch_slice_to_view(
            &frame,
            0..frame.items.len(),
            target_view,
            target_width,
            target_height,
            collector.viewport_width,
            collector.viewport_height,
            clear_color,
            damage,
        );
        frame.summary
    }

    /// Renders `list` into a reusable hardware offscreen texture and reads back RGBA/BGRA bytes
    /// (matching [`Self::color_format`]) when `readback` is `true`.
    #[allow(clippy::too_many_arguments)]
    pub fn render_display_list_offscreen(
        &mut self,
        collector: &mut crate::gpu_pipeline::UnifiedGpuCollector,
        list: &crate::compositor::DisplayList,
        width: u32,
        height: u32,
        dpr: f32,
        clear_color: [f32; 4],
        damage: Option<Rect>,
        readback: bool,
    ) -> Result<(crate::gpu_pipeline::GpuBatchSummary, Vec<u8>), String> {
        let w = width.max(1);
        let h = height.max(1);
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let unpadded = w * 4;
        let padded = unpadded.div_ceil(align) * align;

        let recreate = self
            .offscreen
            .as_ref()
            .is_none_or(|o| o.width != w || o.height != h || o.format != self.color_format);
        if recreate {
            let color = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("strata_2d_offscreen_color"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.color_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = color.create_view(&Default::default());
            let rb = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("strata_2d_offscreen_readback"),
                size: (padded * h) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            self.offscreen = Some(Offscreen2DTarget {
                width: w,
                height: h,
                format: self.color_format,
                color,
                view,
                readback: rb,
                padded_bytes_per_row: padded,
            });
        }

        let target_view = self.offscreen.as_ref().unwrap().view.clone();
        let summary = self.render_display_list_to_view(
            collector,
            list,
            &target_view,
            w,
            h,
            dpr,
            Some(clear_color),
            damage,
        );

        if !readback {
            self.device.poll(wgpu::Maintain::Wait);
            return Ok((summary, Vec::new()));
        }

        let off = self.offscreen.as_ref().unwrap();
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("strata_2d_readback_encoder"),
            });
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &off.color,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &off.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(off.padded_bytes_per_row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(enc.finish()));

        let slice = off.readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx.send(res);
        });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .map_err(|e| format!("2d readback recv error: {e}"))?
            .map_err(|e| format!("2d readback map error: {e:?}"))?;

        let data = slice.get_mapped_range();
        let mut out = vec![0u8; (w * h * 4) as usize];
        let row_len = w as usize * 4;
        for row in 0..h as usize {
            let src_start = row * off.padded_bytes_per_row as usize;
            let dst_start = row * row_len;
            out[dst_start..dst_start + row_len]
                .copy_from_slice(&data[src_start..src_start + row_len]);
        }
        drop(data);
        off.readback.unmap();
        Ok((summary, out))
    }
}
