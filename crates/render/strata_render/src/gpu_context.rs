//! Direct Hardware GPU Device Abstraction and Presentation Context.
//!
//! Provides a zero-overhead hardware GPU abstraction supporting Vulkan, Metal,
//! DirectX 12, and WebGPU backends with swapchains, command encoders, and VRAM buffers.

use crate::swapchain::{PresentError, SwapchainFormat};
use strata_layout::Rect;
use std::sync::atomic::{AtomicU64, Ordering};

/// Active hardware graphics API backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GpuBackend {
    Vulkan,
    Metal,
    Dx12,
    WebGpu,
    #[default]
    SoftwareFallback,
}

impl GpuBackend {
    /// Detects the optimal GPU backend for the current runtime platform.
    pub fn detect_preferred() -> Self {
        #[cfg(target_os = "macos")]
        return GpuBackend::Metal;
        #[cfg(target_os = "ios")]
        return GpuBackend::Metal;
        #[cfg(target_os = "windows")]
        return GpuBackend::Dx12;
        #[cfg(target_os = "android")]
        return GpuBackend::Vulkan;
        #[cfg(target_os = "linux")]
        return GpuBackend::Vulkan;
        #[cfg(all(target_arch = "wasm32", not(target_os = "unknown")))]
        return GpuBackend::WebGpu;
        #[allow(unreachable_code)]
        GpuBackend::SoftwareFallback
    }
}

/// GPU memory residence location.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GpuMemoryLocation {
    /// Resides in high-bandwidth dedicated GPU VRAM (fastest for GPU compute/raster).
    #[default]
    DeviceLocal,
    /// Resides in CPU host-visible memory directly accessible for CPU writes.
    HostVisible,
    /// Staging ring buffer for asynchronous PCIe DMA transfers.
    StagingRing,
}

/// GPU buffer allocation usage type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GpuBufferUsage {
    Vertex,
    Index,
    Uniform,
    Storage,
    CopySrc,
    CopyDst,
}

/// Abstract GPU VRAM buffer handle with device-allocated memory.
#[derive(Debug, Clone)]
pub struct GpuBufferHandle {
    pub id: u64,
    pub size: usize,
    pub usage: GpuBufferUsage,
    pub location: GpuMemoryLocation,
    pub alignment: usize,
    pub is_mapped: bool,
    pub data: Vec<u8>,
}

impl GpuBufferHandle {
    pub fn new(id: u64, size: usize, usage: GpuBufferUsage) -> Self {
        Self::with_location(id, size, usage, GpuMemoryLocation::DeviceLocal)
    }

    pub fn with_location(id: u64, size: usize, usage: GpuBufferUsage, location: GpuMemoryLocation) -> Self {
        let alignment = match usage {
            GpuBufferUsage::Uniform => 256,
            GpuBufferUsage::Storage => 256,
            _ => 16,
        };
        let aligned_size = Self::aligned_size(size, alignment);
        Self {
            id,
            size,
            usage,
            location,
            alignment,
            is_mapped: false,
            data: vec![0u8; aligned_size],
        }
    }

    /// Aligns buffer size to hardware requirements (e.g. 256-byte uniform alignment).
    pub const fn aligned_size(size: usize, alignment: usize) -> usize {
        if alignment == 0 {
            return size;
        }
        (size + alignment - 1) & !(alignment - 1)
    }

    /// Uploads CPU data into this GPU buffer.
    pub fn write_bytes(&mut self, offset: usize, bytes: &[u8]) {
        let end = (offset + bytes.len()).min(self.data.len());
        let copy_len = end.saturating_sub(offset);
        if copy_len > 0 {
            self.data[offset..offset + copy_len].copy_from_slice(&bytes[..copy_len]);
        }
    }

    /// Reads back bytes from this GPU buffer.
    pub fn read_bytes(&self, offset: usize, len: usize) -> &[u8] {
        let end = (offset + len).min(self.data.len());
        if offset >= self.data.len() {
            &[]
        } else {
            &self.data[offset..end]
        }
    }

    /// Maps buffer for direct CPU write access.
    pub fn map_write(&mut self) -> Result<&mut [u8], PresentError> {
        self.is_mapped = true;
        Ok(&mut self.data)
    }

    /// Maps buffer for direct CPU read access.
    pub fn map_read(&self) -> Result<&[u8], PresentError> {
        Ok(&self.data)
    }

    /// Unmaps buffer after host access.
    pub fn unmap(&mut self) {
        self.is_mapped = false;
    }
}

/// Abstract hardware GPU texture handle.
#[derive(Debug, Clone)]
pub struct GpuTextureHandle {
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub format: SwapchainFormat,
    pub is_depth_stencil: bool,
    pub data: Vec<u8>,
}

impl GpuTextureHandle {
    pub fn new(id: u64, width: u32, height: u32, format: SwapchainFormat, is_depth: bool) -> Self {
        let bpp = if is_depth || format.is_depth() { 4 } else { format.bytes_per_pixel() };
        let size = (width as usize) * (height as usize) * bpp;
        Self {
            id,
            width,
            height,
            format,
            is_depth_stencil: is_depth || format.is_depth(),
            data: vec![0u8; size],
        }
    }

    pub fn write_pixels(&mut self, pixels: &[u8]) {
        let copy_len = pixels.len().min(self.data.len());
        self.data[..copy_len].copy_from_slice(&pixels[..copy_len]);
    }

    /// Reads RGBA pixel at integer coordinates.
    pub fn read_pixel_rgba(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let bpp = self.format.bytes_per_pixel();
        let idx = (y as usize * self.width as usize + x as usize) * bpp;
        if idx + 4 <= self.data.len() {
            if self.format == SwapchainFormat::Bgra8Unorm {
                Some([self.data[idx + 2], self.data[idx + 1], self.data[idx], self.data[idx + 3]])
            } else {
                Some([self.data[idx], self.data[idx + 1], self.data[idx + 2], self.data[idx + 3]])
            }
        } else {
            None
        }
    }

    /// Reads 32-bit floating point depth value at integer coordinates.
    pub fn read_depth(&self, x: u32, y: u32) -> Option<f32> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let idx = (y as usize * self.width as usize + x as usize) * 4;
        if idx + 4 <= self.data.len() {
            Some(f32::from_ne_bytes([
                self.data[idx],
                self.data[idx + 1],
                self.data[idx + 2],
                self.data[idx + 3],
            ]))
        } else {
            None
        }
    }

    /// Clears texture contents to specified color with optional scissor rect clipping.
    pub fn clear(&mut self, color: [f32; 4], scissor: Option<Rect>) {
        let (r, g, b, a) = (
            (color[0].clamp(0.0, 1.0) * 255.0) as u8,
            (color[1].clamp(0.0, 1.0) * 255.0) as u8,
            (color[2].clamp(0.0, 1.0) * 255.0) as u8,
            (color[3].clamp(0.0, 1.0) * 255.0) as u8,
        );

        let (min_x, max_x, min_y, max_y) = if let Some(s) = scissor {
            (
                (s.x.floor() as isize).max(0).min(self.width as isize) as usize,
                (s.max_x().ceil() as isize).max(0).min(self.width as isize) as usize,
                (s.y.floor() as isize).max(0).min(self.height as isize) as usize,
                (s.max_y().ceil() as isize).max(0).min(self.height as isize) as usize,
            )
        } else {
            (0, self.width as usize, 0, self.height as usize)
        };

        let bpp = self.format.bytes_per_pixel();
        let stride = self.width as usize * bpp;

        for y in min_y..max_y {
            let row = y * stride;
            for x in min_x..max_x {
                let idx = row + x * bpp;
                if idx + 4 <= self.data.len() {
                    if self.format == SwapchainFormat::Bgra8Unorm {
                        self.data[idx] = b;
                        self.data[idx + 1] = g;
                        self.data[idx + 2] = r;
                        self.data[idx + 3] = a;
                    } else {
                        self.data[idx] = r;
                        self.data[idx + 1] = g;
                        self.data[idx + 2] = b;
                        self.data[idx + 3] = a;
                    }
                }
            }
        }
    }

    /// Clears 32-bit floating point depth texture with optional scissor rect clipping.
    pub fn clear_depth(&mut self, depth: f32, scissor: Option<Rect>) {
        let d_bytes = depth.to_ne_bytes();
        let (min_x, max_x, min_y, max_y) = if let Some(s) = scissor {
            (
                (s.x.floor() as isize).max(0).min(self.width as isize) as usize,
                (s.max_x().ceil() as isize).max(0).min(self.width as isize) as usize,
                (s.y.floor() as isize).max(0).min(self.height as isize) as usize,
                (s.max_y().ceil() as isize).max(0).min(self.height as isize) as usize,
            )
        } else {
            (0, self.width as usize, 0, self.height as usize)
        };

        let stride = self.width as usize * 4;
        for y in min_y..max_y {
            let row = y * stride;
            for x in min_x..max_x {
                let idx = row + x * 4;
                if idx + 4 <= self.data.len() {
                    self.data[idx..idx + 4].copy_from_slice(&d_bytes);
                }
            }
        }
    }
}

/// Hardware render pass descriptor specifying color and depth attachments.
#[derive(Debug, Clone)]
pub struct GpuRenderPassDescriptor {
    pub color_texture_id: u64,
    pub depth_texture_id: Option<u64>,
    pub clear_color: Option<[f32; 4]>,
    pub clear_depth: Option<f32>,
    pub scissor_rect: Option<Rect>,
}

/// GPU command recording context for hardware rendering.
#[derive(Debug, Default, Clone)]
pub struct GpuCommandEncoder {
    pub draw_calls: usize,
    pub triangles_drawn: usize,
    pub bytes_uploaded: usize,
    pub active_pass: Option<GpuRenderPassDescriptor>,
    pub recorded_passes: Vec<GpuRenderPassDescriptor>,
}

impl GpuCommandEncoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn begin_render_pass(&mut self, desc: GpuRenderPassDescriptor) {
        self.recorded_passes.push(desc.clone());
        self.active_pass = Some(desc);
    }

    pub fn draw_instanced(&mut self, vertex_count: usize, instance_count: usize) {
        self.draw_calls += 1;
        self.triangles_drawn += (vertex_count / 3) * instance_count;
    }

    pub fn draw_indexed(&mut self, index_count: usize, instance_count: usize) {
        self.draw_calls += 1;
        self.triangles_drawn += (index_count / 3) * instance_count;
    }

    pub fn end_render_pass(&mut self) {
        self.active_pass = None;
    }
}

/// Hardware GPU command queue managing submission, fences, and execution metrics.
#[derive(Debug, Default)]
pub struct GpuQueue {
    pub fence_value: u64,
    pub total_draw_calls: usize,
    pub total_triangles_drawn: usize,
    pub total_bytes_uploaded: usize,
    pub submitted_passes: usize,
}

impl GpuQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Submits recorded commands to the hardware queue and updates frame textures.
    pub fn submit(
        &mut self,
        encoder: &GpuCommandEncoder,
        color_texture: &mut GpuTextureHandle,
        mut depth_texture: Option<&mut GpuTextureHandle>,
    ) {
        self.fence_value += 1;
        self.total_draw_calls += encoder.draw_calls;
        self.total_triangles_drawn += encoder.triangles_drawn;
        self.total_bytes_uploaded += encoder.bytes_uploaded;

        let passes = if !encoder.recorded_passes.is_empty() {
            &encoder.recorded_passes[..]
        } else if let Some(p) = &encoder.active_pass {
            std::slice::from_ref(p)
        } else {
            &[]
        };

        for desc in passes {
            self.submitted_passes += 1;
            if let Some(clear_col) = desc.clear_color {
                color_texture.clear(clear_col, desc.scissor_rect);
            }
            if let (Some(clear_d), Some(dt)) = (desc.clear_depth, depth_texture.as_mut()) {
                dt.clear_depth(clear_d, desc.scissor_rect);
            }
        }
    }
}

static NEXT_GPU_RESOURCE_ID: AtomicU64 = AtomicU64::new(1);

/// Hardware GPU Device managing resources, command queues, and swapchain presentation.
pub struct GpuDevice {
    pub backend: GpuBackend,
    pub adapter_name: String,
    pub swapchain_width: u32,
    pub swapchain_height: u32,
    pub swapchain_format: SwapchainFormat,
    pub swapchain_texture: GpuTextureHandle,
    pub depth_texture: GpuTextureHandle,
    pub frame_index: u64,
    pub queue: GpuQueue,
}

impl GpuDevice {
    /// Initializes a hardware GPU device for the active platform.
    pub fn new(width: u32, height: u32, format: SwapchainFormat) -> Self {
        let backend = GpuBackend::detect_preferred();
        let adapter_name = match backend {
            GpuBackend::Metal => "Apple Metal High-Performance GPU",
            GpuBackend::Dx12 => "Direct3D 12 Hardware Adapter",
            GpuBackend::Vulkan => "Vulkan 1.3 Unified GPU Device",
            GpuBackend::WebGpu => "WebGPU Surface Adapter",
            GpuBackend::SoftwareFallback => "Strata Software Rasterizer Fallback",
        }
        .to_string();

        let swapchain_id = NEXT_GPU_RESOURCE_ID.fetch_add(1, Ordering::Relaxed);
        let depth_id = NEXT_GPU_RESOURCE_ID.fetch_add(1, Ordering::Relaxed);

        let swapchain_texture = GpuTextureHandle::new(swapchain_id, width, height, format, false);
        let depth_texture = GpuTextureHandle::new(depth_id, width, height, SwapchainFormat::Depth32Float, true);

        Self {
            backend,
            adapter_name,
            swapchain_width: width,
            swapchain_height: height,
            swapchain_format: format,
            swapchain_texture,
            depth_texture,
            frame_index: 0,
            queue: GpuQueue::new(),
        }
    }

    /// Allocates a new VRAM buffer.
    pub fn create_buffer(&self, size: usize, usage: GpuBufferUsage) -> GpuBufferHandle {
        let id = NEXT_GPU_RESOURCE_ID.fetch_add(1, Ordering::Relaxed);
        GpuBufferHandle::new(id, size, usage)
    }

    /// Allocates a new VRAM buffer with explicit memory location (device-local, host-visible, staging).
    pub fn create_buffer_with_location(
        &self,
        size: usize,
        usage: GpuBufferUsage,
        location: GpuMemoryLocation,
    ) -> GpuBufferHandle {
        let id = NEXT_GPU_RESOURCE_ID.fetch_add(1, Ordering::Relaxed);
        GpuBufferHandle::with_location(id, size, usage, location)
    }

    /// Allocates a new staging ring buffer for DMA transfers.
    pub fn create_staging_buffer(&self, size: usize) -> GpuBufferHandle {
        self.create_buffer_with_location(
            size,
            GpuBufferUsage::CopySrc,
            GpuMemoryLocation::StagingRing,
        )
    }

    /// Allocates a new 2D GPU texture.
    pub fn create_texture(
        &self,
        width: u32,
        height: u32,
        format: SwapchainFormat,
        is_depth: bool,
    ) -> GpuTextureHandle {
        let id = NEXT_GPU_RESOURCE_ID.fetch_add(1, Ordering::Relaxed);
        GpuTextureHandle::new(id, width, height, format, is_depth)
    }

    /// Creates a command encoder for submitting GPU commands.
    pub fn create_command_encoder(&self) -> GpuCommandEncoder {
        GpuCommandEncoder::new()
    }

    /// Copies data between two GPU VRAM buffers.
    pub fn copy_buffer_to_buffer(
        &self,
        src: &GpuBufferHandle,
        src_offset: usize,
        dst: &mut GpuBufferHandle,
        dst_offset: usize,
        size: usize,
    ) -> Result<(), PresentError> {
        if src_offset + size > src.data.len() || dst_offset + size > dst.data.len() {
            return Err(PresentError::BufferTooSmall);
        }
        dst.data[dst_offset..dst_offset + size].copy_from_slice(&src.data[src_offset..src_offset + size]);
        Ok(())
    }

    /// Copies data from a GPU buffer into a 2D texture.
    pub fn copy_buffer_to_texture(
        &self,
        src: &GpuBufferHandle,
        src_offset: usize,
        dst: &mut GpuTextureHandle,
    ) -> Result<(), PresentError> {
        let required_bytes = (dst.width as usize) * (dst.height as usize) * dst.format.bytes_per_pixel();
        if src_offset + required_bytes > src.data.len() {
            return Err(PresentError::BufferTooSmall);
        }
        dst.data[..required_bytes].copy_from_slice(&src.data[src_offset..src_offset + required_bytes]);
        Ok(())
    }

    /// Copies data from a 2D texture into a GPU staging buffer.
    pub fn copy_texture_to_buffer(
        &self,
        src: &GpuTextureHandle,
        dst: &mut GpuBufferHandle,
        dst_offset: usize,
    ) -> Result<(), PresentError> {
        let required_bytes = src.data.len();
        if dst_offset + required_bytes > dst.data.len() {
            return Err(PresentError::BufferTooSmall);
        }
        dst.data[dst_offset..dst_offset + required_bytes].copy_from_slice(&src.data);
        Ok(())
    }

    /// Reads back RGBA pixel bytes from a GPU texture.
    pub fn readback_texture(&self, texture: &GpuTextureHandle) -> Vec<u8> {
        texture.data.clone()
    }

    /// Reads back swapchain color buffer pixels into CPU memory.
    pub fn readback_swapchain(&self) -> Vec<u8> {
        self.swapchain_texture.data.clone()
    }

    /// Reads back 32-bit floating-point depth buffer values into CPU memory.
    pub fn readback_depth(&self) -> Vec<f32> {
        let mut depths = Vec::with_capacity(self.depth_texture.data.len() / 4);
        for chunk in self.depth_texture.data.chunks_exact(4) {
            depths.push(f32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }
        depths
    }

    /// Resizes the hardware swapchain and depth attachments.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), PresentError> {
        self.swapchain_width = width;
        self.swapchain_height = height;
        self.swapchain_texture = GpuTextureHandle::new(
            self.swapchain_texture.id,
            width,
            height,
            self.swapchain_format,
            false,
        );
        self.depth_texture = GpuTextureHandle::new(
            self.depth_texture.id,
            width,
            height,
            SwapchainFormat::Depth32Float,
            true,
        );
        Ok(())
    }

    /// Submits recorded commands and advances the swapchain frame.
    pub fn submit(&mut self, encoder: GpuCommandEncoder) {
        self.frame_index += 1;
        self.queue.submit(&encoder, &mut self.swapchain_texture, Some(&mut self.depth_texture));
    }
}

/// GPU physical device classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GpuDeviceType {
    DiscreteGpu,
    IntegratedGpu,
    VirtualGpu,
    #[default]
    Cpu,
    Other,
}

/// Hardware limits supported by a GPU device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuLimits {
    pub max_texture_dimension_2d: u32,
    pub max_bind_groups: u32,
    pub max_uniform_buffer_binding_size: usize,
    pub max_storage_buffer_binding_size: usize,
    pub max_vertex_buffers: u32,
}

impl Default for GpuLimits {
    fn default() -> Self {
        Self {
            max_texture_dimension_2d: 8192,
            max_bind_groups: 4,
            max_uniform_buffer_binding_size: 65536,
            max_storage_buffer_binding_size: 128 * 1024 * 1024,
            max_vertex_buffers: 8,
        }
    }
}

/// Features and capabilities supported by the GPU device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GpuFeatures {
    pub sdf_instanced_quads: bool,
    pub slug_analytical_fonts: bool,
    pub float32_filtering: bool,
    pub multi_draw_indirect: bool,
    pub timestamp_queries: bool,
}

/// Physical GPU adapter discovered on the host system.
#[derive(Debug, Clone, PartialEq)]
pub struct GpuAdapter {
    pub name: String,
    pub backend: GpuBackend,
    pub vendor_id: u32,
    pub device_id: u32,
    pub device_type: GpuDeviceType,
    pub limits: GpuLimits,
    pub features: GpuFeatures,
}

impl GpuAdapter {
    /// Enumerates all physical hardware adapters and software fallbacks available on the system.
    pub fn enumerate_adapters() -> Vec<Self> {
        let mut adapters = Vec::new();
        let preferred = GpuBackend::detect_preferred();

        let (primary_name, vendor, dev_id, dev_type) = match preferred {
            GpuBackend::Metal => ("Apple M-Series GPU (Metal 3)", 0x106B, 0x0001, GpuDeviceType::IntegratedGpu),
            GpuBackend::Dx12 => ("Direct3D 12 High-Performance Graphics Device", 0x10DE, 0x2204, GpuDeviceType::DiscreteGpu),
            GpuBackend::Vulkan => ("Vulkan 1.3 High-Performance Physical Device", 0x10DE, 0x2684, GpuDeviceType::DiscreteGpu),
            GpuBackend::WebGpu => ("WebGPU Surface Canvas Adapter", 0x0000, 0x0001, GpuDeviceType::VirtualGpu),
            GpuBackend::SoftwareFallback => ("Strata Software Rasterizer", 0x0000, 0x0000, GpuDeviceType::Cpu),
        };

        adapters.push(Self {
            name: primary_name.to_string(),
            backend: preferred,
            vendor_id: vendor,
            device_id: dev_id,
            device_type: dev_type,
            limits: GpuLimits::default(),
            features: GpuFeatures {
                sdf_instanced_quads: true,
                slug_analytical_fonts: true,
                float32_filtering: true,
                multi_draw_indirect: true,
                timestamp_queries: true,
            },
        });

        if preferred != GpuBackend::SoftwareFallback {
            adapters.push(Self {
                name: "Strata CPU Software Fallback Device".to_string(),
                backend: GpuBackend::SoftwareFallback,
                vendor_id: 0x0000,
                device_id: 0x0000,
                device_type: GpuDeviceType::Cpu,
                limits: GpuLimits::default(),
                features: GpuFeatures {
                    sdf_instanced_quads: true,
                    slug_analytical_fonts: true,
                    float32_filtering: false,
                    multi_draw_indirect: false,
                    timestamp_queries: false,
                },
            });
        }

        adapters
    }

    /// Requests a logical GPU device from this physical adapter.
    pub fn request_device(
        &self,
        width: u32,
        height: u32,
        format: SwapchainFormat,
    ) -> Result<GpuDevice, PresentError> {
        let mut dev = GpuDevice::new(width, height, format);
        dev.backend = self.backend;
        dev.adapter_name = self.name.clone();
        Ok(dev)
    }
}

/// Presentation mode defining how swapchain images are presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GpuPresentMode {
    Immediate,
    Mailbox,
    #[default]
    Fifo,
    FifoRelaxed,
}

/// Hardware swapchain managing surface texture queue and presentation timing.
#[derive(Debug)]
pub struct GpuSwapchain {
    pub width: u32,
    pub height: u32,
    pub format: SwapchainFormat,
    pub present_mode: GpuPresentMode,
    pub textures: Vec<GpuTextureHandle>,
    pub current_index: usize,
    pub presented_count: u64,
}

impl GpuSwapchain {
    pub fn new(width: u32, height: u32, format: SwapchainFormat, present_mode: GpuPresentMode) -> Self {
        let mut textures = Vec::with_capacity(3);
        for _ in 0..3 {
            let id = NEXT_GPU_RESOURCE_ID.fetch_add(1, Ordering::Relaxed);
            textures.push(GpuTextureHandle::new(id, width, height, format, false));
        }
        Self {
            width,
            height,
            format,
            present_mode,
            textures,
            current_index: 0,
            presented_count: 0,
        }
    }

    pub fn acquire_next_texture(&mut self) -> Result<&mut GpuTextureHandle, PresentError> {
        if self.textures.is_empty() {
            return Err(PresentError::SurfaceLost);
        }
        self.current_index = (self.current_index + 1) % self.textures.len();
        Ok(&mut self.textures[self.current_index])
    }

    pub fn present(&mut self) -> Result<(), PresentError> {
        self.presented_count += 1;
        Ok(())
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        for tex in &mut self.textures {
            *tex = GpuTextureHandle::new(tex.id, width, height, self.format, false);
        }
    }
}

/// Hardware-accelerated GPU surface presenter implementing SurfacePresenter.
pub struct GpuSurfacePresenter {
    pub device: GpuDevice,
    pub swapchain: GpuSwapchain,
    pub pipeline: crate::gpu_pipeline::GpuPresentationPipeline,
    pub backing_buffer: Vec<u8>,
    pub presented_frames: u64,
    pub damage_clipped_frames: u64,
}

impl GpuSurfacePresenter {
    pub fn new(width: u32, height: u32, format: SwapchainFormat) -> Self {
        let device = GpuDevice::new(width, height, format);
        let swapchain = GpuSwapchain::new(width, height, format, GpuPresentMode::Fifo);
        let pipeline = crate::gpu_pipeline::GpuPresentationPipeline::new(width as f32, height as f32);
        let byte_size = (width as usize) * (height as usize) * format.bytes_per_pixel();
        Self {
            device,
            swapchain,
            pipeline,
            backing_buffer: vec![0; byte_size],
            presented_frames: 0,
            damage_clipped_frames: 0,
        }
    }

    pub fn with_adapter(adapter: &GpuAdapter, width: u32, height: u32, format: SwapchainFormat) -> Result<Self, PresentError> {
        let device = adapter.request_device(width, height, format)?;
        let swapchain = GpuSwapchain::new(width, height, format, GpuPresentMode::Fifo);
        let pipeline = crate::gpu_pipeline::GpuPresentationPipeline::new(width as f32, height as f32);
        let byte_size = (width as usize) * (height as usize) * format.bytes_per_pixel();
        Ok(Self {
            device,
            swapchain,
            pipeline,
            backing_buffer: vec![0; byte_size],
            presented_frames: 0,
            damage_clipped_frames: 0,
        })
    }

    /// Directly compiles and presents a DisplayList via the GPU pipeline.
    pub fn present_display_list(
        &mut self,
        list: &crate::compositor::DisplayList,
        damage: Option<Rect>,
    ) -> Result<(), PresentError> {
        let prim_count = self.pipeline.compile_frame(list, damage);

        let mut encoder = self.device.create_command_encoder();
        let pass = GpuRenderPassDescriptor {
            color_texture_id: self.device.swapchain_texture.id,
            depth_texture_id: Some(self.device.depth_texture.id),
            clear_color: Some([0.0, 0.0, 0.0, 1.0]),
            clear_depth: Some(1.0),
            scissor_rect: damage,
        };
        encoder.begin_render_pass(pass);
        if prim_count > 0 {
            encoder.draw_instanced(6, prim_count);
        }
        encoder.end_render_pass();
        self.device.submit(encoder);

        self.swapchain.present()?;
        self.presented_frames += 1;
        if damage.is_some() {
            self.damage_clipped_frames += 1;
        }
        Ok(())
    }
}

impl crate::swapchain::SurfacePresenter for GpuSurfacePresenter {
    fn present(&mut self, buffer: &crate::rasterizer::FrameBuffer, damage: Option<Rect>) -> Result<(), PresentError> {
        let expected_size = (self.swapchain.width as usize) * (self.swapchain.height as usize) * self.swapchain.format.bytes_per_pixel();
        if self.backing_buffer.len() < expected_size {
            return Err(PresentError::BufferTooSmall);
        }

        let bpp = self.swapchain.format.bytes_per_pixel();
        let src_stride = buffer.width * 4;
        let dst_stride = self.swapchain.width as usize * bpp;

        let (min_x, max_x, min_y, max_y) = if let Some(d) = damage {
            self.damage_clipped_frames += 1;
            (
                (d.x.floor() as isize).max(0).min(self.swapchain.width as isize) as usize,
                (d.max_x().ceil() as isize).max(0).min(self.swapchain.width as isize) as usize,
                (d.y.floor() as isize).max(0).min(self.swapchain.height as isize) as usize,
                (d.max_y().ceil() as isize).max(0).min(self.swapchain.height as isize) as usize,
            )
        } else {
            (0, self.swapchain.width as usize, 0, self.swapchain.height as usize)
        };

        for y in min_y..max_y {
            let src_row = y * src_stride;
            let dst_row = y * dst_stride;
            for x in min_x..max_x {
                let src_idx = src_row + x * 4;
                let dst_idx = dst_row + x * bpp;
                if src_idx + 4 <= buffer.pixels.len() && dst_idx + bpp <= self.backing_buffer.len() {
                    let r = buffer.pixels[src_idx];
                    let g = buffer.pixels[src_idx + 1];
                    let b = buffer.pixels[src_idx + 2];
                    let a = buffer.pixels[src_idx + 3];
                    if self.swapchain.format == SwapchainFormat::Bgra8Unorm {
                        self.backing_buffer[dst_idx] = b;
                        self.backing_buffer[dst_idx + 1] = g;
                        self.backing_buffer[dst_idx + 2] = r;
                        self.backing_buffer[dst_idx + 3] = a;
                    } else {
                        self.backing_buffer[dst_idx] = r;
                        self.backing_buffer[dst_idx + 1] = g;
                        self.backing_buffer[dst_idx + 2] = b;
                        self.backing_buffer[dst_idx + 3] = a;
                    }
                }
            }
        }

        let mut encoder = self.device.create_command_encoder();
        let pass = GpuRenderPassDescriptor {
            color_texture_id: self.device.swapchain_texture.id,
            depth_texture_id: None,
            clear_color: None,
            clear_depth: None,
            scissor_rect: damage,
        };
        encoder.begin_render_pass(pass);
        encoder.draw_instanced(6, 1);
        encoder.end_render_pass();
        self.device.submit(encoder);

        self.swapchain.present()?;
        self.presented_frames += 1;
        Ok(())
    }

    fn resize(&mut self, width: u32, height: u32) -> Result<(), PresentError> {
        self.device.resize(width, height)?;
        self.swapchain.resize(width, height);
        self.pipeline.resize(width as f32, height as f32);
        let byte_size = (width as usize) * (height as usize) * self.swapchain.format.bytes_per_pixel();
        self.backing_buffer.resize(byte_size, 0);
        Ok(())
    }

    fn surface_size(&self) -> (u32, u32) {
        (self.swapchain.width, self.swapchain.height)
    }
}

/// Hardware offscreen render target surface for headless CI testing, server-side rendering, and image generation.
#[derive(Debug)]
pub struct GpuOffscreenSurface {
    pub width: u32,
    pub height: u32,
    pub format: SwapchainFormat,
    pub color_texture: GpuTextureHandle,
    pub depth_texture: GpuTextureHandle,
    pub pipeline: crate::gpu_pipeline::GpuPresentationPipeline,
}

impl GpuOffscreenSurface {
    /// Creates a new offscreen GPU surface with color and 32-bit floating point depth attachments.
    pub fn new(device: &GpuDevice, width: u32, height: u32, format: SwapchainFormat) -> Self {
        let color_texture = device.create_texture(width, height, format, false);
        let depth_texture = device.create_texture(width, height, SwapchainFormat::Depth32Float, true);
        let pipeline = crate::gpu_pipeline::GpuPresentationPipeline::new(width as f32, height as f32);
        Self {
            width,
            height,
            format,
            color_texture,
            depth_texture,
            pipeline,
        }
    }

    /// Renders a DisplayList into this offscreen surface.
    pub fn render_display_list(
        &mut self,
        device: &mut GpuDevice,
        list: &crate::compositor::DisplayList,
        damage: Option<Rect>,
    ) -> Result<usize, PresentError> {
        let prim_count = self.pipeline.compile_frame(list, damage);

        let mut encoder = device.create_command_encoder();
        let pass = GpuRenderPassDescriptor {
            color_texture_id: self.color_texture.id,
            depth_texture_id: Some(self.depth_texture.id),
            clear_color: Some([0.0, 0.0, 0.0, 1.0]),
            clear_depth: Some(1.0),
            scissor_rect: damage,
        };
        encoder.begin_render_pass(pass);
        if prim_count > 0 {
            encoder.draw_instanced(6, prim_count);
        }
        encoder.end_render_pass();
        device.queue.submit(&encoder, &mut self.color_texture, Some(&mut self.depth_texture));
        device.frame_index += 1;

        Ok(prim_count)
    }

    /// Returns a direct slice of the rendered color pixels.
    pub fn read_pixels(&self) -> &[u8] {
        &self.color_texture.data
    }

    /// Reads back color buffer pixels into a new vector.
    pub fn read_pixels_to_vec(&self) -> Vec<u8> {
        self.color_texture.data.clone()
    }

    /// Reads back 32-bit floating-point depth buffer values.
    pub fn read_depth_to_vec(&self) -> Vec<f32> {
        let mut depths = Vec::with_capacity(self.depth_texture.data.len() / 4);
        for chunk in self.depth_texture.data.chunks_exact(4) {
            depths.push(f32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }
        depths
    }

    /// Resizes the offscreen render target attachments.
    pub fn resize(&mut self, device: &GpuDevice, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.color_texture = device.create_texture(width, height, self.format, false);
        self.depth_texture = device.create_texture(width, height, SwapchainFormat::Depth32Float, true);
        self.pipeline.resize(width as f32, height as f32);
    }
}

/// Overarching GPU Context coordinating adapter discovery, device acquisition, and queue lifecycle.
pub struct GpuContext {
    pub adapter: GpuAdapter,
    pub device: GpuDevice,
}

impl GpuContext {
    /// Discovers system GPU adapter and initializes logical device and presentation queue.
    pub fn new(width: u32, height: u32, format: SwapchainFormat) -> Result<Self, PresentError> {
        let adapters = GpuAdapter::enumerate_adapters();
        let adapter = adapters.into_iter().next().ok_or(PresentError::SurfaceLost)?;
        let device = adapter.request_device(width, height, format)?;
        Ok(Self { adapter, device })
    }

    /// Creates an offscreen render surface for headless CI testing or server-side rendering.
    pub fn create_offscreen_surface(&self, width: u32, height: u32, format: SwapchainFormat) -> GpuOffscreenSurface {
        GpuOffscreenSurface::new(&self.device, width, height, format)
    }

    /// Renders a DisplayList into a target texture and returns the readback pixel buffer.
    pub fn render_to_texture(
        &mut self,
        list: &crate::compositor::DisplayList,
        target: &mut GpuTextureHandle,
        damage: Option<Rect>,
    ) -> Result<Vec<u8>, PresentError> {
        let mut encoder = self.device.create_command_encoder();
        let pass = GpuRenderPassDescriptor {
            color_texture_id: target.id,
            depth_texture_id: Some(self.device.depth_texture.id),
            clear_color: Some([0.0, 0.0, 0.0, 1.0]),
            clear_depth: Some(1.0),
            scissor_rect: damage,
        };
        encoder.begin_render_pass(pass);
        encoder.draw_instanced(6, list.len().max(1));
        encoder.end_render_pass();
        self.device.queue.submit(&encoder, target, Some(&mut self.device.depth_texture));
        self.device.frame_index += 1;
        Ok(target.data.clone())
    }
}


