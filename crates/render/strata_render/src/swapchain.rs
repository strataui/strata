//! Hardware Swapchain Presentation Pipeline, Surface Presenter Trait, and Damage Scissoring.

use std::time::{Duration, Instant};
use strata_layout::Rect;
use crate::compositor::{DamageTracker, DisplayList};
use crate::rasterizer::FrameBuffer;

/// Color format for pixel data stored in the swapchain surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SwapchainFormat {
    /// 32-bit BGRA unsigned normalized (standard Windows DXGI and macOS Metal format).
    #[default]
    Bgra8Unorm,
    /// 32-bit RGBA unsigned normalized (standard Wayland / Vulkan format).
    Rgba8Unorm,
    /// 64-bit HDR half-precision floating point.
    Rgba16Float,
}

impl SwapchainFormat {
    /// Bytes per pixel for this format.
    pub const fn bytes_per_pixel(&self) -> usize {
        match self {
            Self::Bgra8Unorm | Self::Rgba8Unorm => 4,
            Self::Rgba16Float => 8,
        }
    }
}

/// Potential presentation and swapchain errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresentError {
    SurfaceLost,
    Outdated,
    Timeout,
    BufferTooSmall,
    Custom(String),
}

/// Abstract hardware presentation surface capable of receiving rendered frames.
pub trait SurfacePresenter {
    /// Presents the contents of the given framebuffer to the display surface, optionally clipped to damage.
    fn present(&mut self, buffer: &FrameBuffer, damage: Option<Rect>) -> Result<(), PresentError>;

    /// Notifies the presentation surface of a dimension resize.
    fn resize(&mut self, width: u32, height: u32) -> Result<(), PresentError>;

    /// Returns physical surface dimensions as (width, height).
    fn surface_size(&self) -> (u32, u32);
}

/// Zero-dependency software presentation surface holding a target backing pixel store.
#[derive(Debug, Clone)]
pub struct SoftwareSurfacePresenter {
    pub width: u32,
    pub height: u32,
    pub format: SwapchainFormat,
    pub backing_buffer: Vec<u8>,
    pub presented_frames: u64,
    pub damage_clipped_frames: u64,
}

impl SoftwareSurfacePresenter {
    /// Creates a new software presentation surface initialized with the specified size and format.
    pub fn new(width: u32, height: u32, format: SwapchainFormat) -> Self {
        let byte_size = (width as usize) * (height as usize) * format.bytes_per_pixel();
        Self {
            width,
            height,
            format,
            backing_buffer: vec![0; byte_size],
            presented_frames: 0,
            damage_clipped_frames: 0,
        }
    }
}

impl SurfacePresenter for SoftwareSurfacePresenter {
    fn present(&mut self, buffer: &FrameBuffer, damage: Option<Rect>) -> Result<(), PresentError> {
        let expected_size = (self.width as usize) * (self.height as usize) * self.format.bytes_per_pixel();
        if self.backing_buffer.len() < expected_size {
            return Err(PresentError::BufferTooSmall);
        }

        if let Some(damage_rect) = damage {
            self.damage_clipped_frames += 1;
            let min_x = (damage_rect.x.floor() as isize).max(0).min(self.width as isize) as usize;
            let max_x = (damage_rect.max_x().ceil() as isize).max(0).min(self.width as isize) as usize;
            let min_y = (damage_rect.y.floor() as isize).max(0).min(self.height as isize) as usize;
            let max_y = (damage_rect.max_y().ceil() as isize).max(0).min(self.height as isize) as usize;

            let bpp = self.format.bytes_per_pixel();
            let src_stride = buffer.width * 4;
            let dst_stride = self.width as usize * bpp;

            for y in min_y..max_y {
                let src_row = y * src_stride;
                let dst_row = y * dst_stride;

                for x in min_x..max_x {
                    let src_idx = src_row + x * 4;
                    let dst_idx = dst_row + x * bpp;

                    if src_idx + 3 < buffer.pixels.len() && dst_idx + 3 < self.backing_buffer.len() {
                        let r = buffer.pixels[src_idx];
                        let g = buffer.pixels[src_idx + 1];
                        let b = buffer.pixels[src_idx + 2];
                        let a = buffer.pixels[src_idx + 3];

                        match self.format {
                            SwapchainFormat::Bgra8Unorm => {
                                self.backing_buffer[dst_idx] = b;
                                self.backing_buffer[dst_idx + 1] = g;
                                self.backing_buffer[dst_idx + 2] = r;
                                self.backing_buffer[dst_idx + 3] = a;
                            }
                            SwapchainFormat::Rgba8Unorm => {
                                self.backing_buffer[dst_idx] = r;
                                self.backing_buffer[dst_idx + 1] = g;
                                self.backing_buffer[dst_idx + 2] = b;
                                self.backing_buffer[dst_idx + 3] = a;
                            }
                            SwapchainFormat::Rgba16Float => {
                                // Simple 8-bit to half-float mapping for fallback
                                self.backing_buffer[dst_idx] = r;
                                self.backing_buffer[dst_idx + 1] = g;
                                self.backing_buffer[dst_idx + 2] = b;
                                self.backing_buffer[dst_idx + 3] = a;
                            }
                        }
                    }
                }
            }
        } else {
            // Full screen copy
            if self.format == SwapchainFormat::Rgba8Unorm {
                let copy_len = self.backing_buffer.len().min(buffer.pixels.len());
                self.backing_buffer[..copy_len].copy_from_slice(&buffer.pixels[..copy_len]);
            } else if self.format == SwapchainFormat::Bgra8Unorm {
                for (dst, src) in self.backing_buffer.chunks_exact_mut(4).zip(buffer.pixels.chunks_exact(4)) {
                    dst[0] = src[2]; // B
                    dst[1] = src[1]; // G
                    dst[2] = src[0]; // R
                    dst[3] = src[3]; // A
                }
            }
        }

        self.presented_frames += 1;
        Ok(())
    }

    fn resize(&mut self, width: u32, height: u32) -> Result<(), PresentError> {
        self.width = width;
        self.height = height;
        let byte_size = (width as usize) * (height as usize) * self.format.bytes_per_pixel();
        self.backing_buffer.resize(byte_size, 0);
        Ok(())
    }

    fn surface_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

/// Orchestrates display list compilation, damage scissoring, and swapchain presentation.
#[derive(Debug)]
pub struct SwapchainCompositor {
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
    pub framebuffer: FrameBuffer,
    pub damage_tracker: DamageTracker,
    pub frame_count: u64,
    pub total_render_duration: Duration,
}

impl SwapchainCompositor {
    /// Creates a swapchain compositor with backing framebuffer sized to physical pixels.
    pub fn new(width: u32, height: u32, scale_factor: f32) -> Self {
        let pw = ((width as f32 * scale_factor) + 0.5) as usize;
        let ph = ((height as f32 * scale_factor) + 0.5) as usize;
        let mut damage_tracker = DamageTracker::new();
        damage_tracker.add_damage(Rect::new(0.0, 0.0, width as f32, height as f32));

        Self {
            width,
            height,
            scale_factor,
            framebuffer: FrameBuffer::new(pw.max(1), ph.max(1)),
            damage_tracker,
            frame_count: 0,
            total_render_duration: Duration::ZERO,
        }
    }

    /// Resizes swapchain dimensions, updating internal framebuffer and registering full dirty damage.
    pub fn resize(&mut self, width: u32, height: u32, scale_factor: f32) {
        self.width = width;
        self.height = height;
        self.scale_factor = scale_factor;
        let pw = ((width as f32 * scale_factor) + 0.5) as usize;
        let ph = ((height as f32 * scale_factor) + 0.5) as usize;
        self.framebuffer = FrameBuffer::new(pw.max(1), ph.max(1));
        self.damage_tracker.add_damage(Rect::new(0.0, 0.0, width as f32, height as f32));
    }

    /// Renders display list and presents to target surface. Skips frame if clean and force_redraw is false.
    pub fn render_and_present<P: SurfacePresenter>(
        &mut self,
        display_list: &DisplayList,
        presenter: &mut P,
        force_redraw: bool,
    ) -> Result<bool, PresentError> {
        let damage = if force_redraw {
            Some(Rect::new(0.0, 0.0, self.width as f32, self.height as f32))
        } else {
            self.damage_tracker.bounding_damage()
        };

        if damage.is_none() && !force_redraw {
            // Clean frame: 0 CPU/GPU wasted!
            return Ok(false);
        }

        let start = Instant::now();
        self.framebuffer.execute_display_list(display_list);
        presenter.present(&self.framebuffer, damage)?;

        self.damage_tracker.reset();
        self.frame_count += 1;
        self.total_render_duration += start.elapsed();

        Ok(true)
    }
}
