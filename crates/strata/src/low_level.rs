//! Low-level direct engine driver and hardware surface APIs.
//!
//! Provides direct access to raw swapchains, offscreen framebuffers,
//! and unabstracted command encoders without RSX virtual DOM tree overhead.

/// Raw windowing surface representation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RawSurfaceHandle {
    pub surface_ptr: usize,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
}

impl RawSurfaceHandle {
    pub fn new(surface_ptr: usize, width: u32, height: u32, scale_factor: f64) -> Self {
        Self {
            surface_ptr,
            width,
            height,
            scale_factor,
        }
    }

    pub fn physical_width(&self) -> u32 {
        (self.width as f64 * self.scale_factor).round() as u32
    }

    pub fn physical_height(&self) -> u32 {
        (self.height as f64 * self.scale_factor).round() as u32
    }
}

/// Headless offscreen pixel buffer.
#[derive(Debug, Clone)]
pub struct OffscreenBuffer {
    pub width: u32,
    pub height: u32,
    pixels: Vec<u8>, // RGBA8
}

impl OffscreenBuffer {
    pub fn new(width: u32, height: u32) -> Self {
        let size = (width as usize) * (height as usize) * 4;
        Self {
            width,
            height,
            pixels: vec![0; size],
        }
    }

    pub fn clear(&mut self, color: [u8; 4]) {
        for chunk in self.pixels.as_chunks_mut::<4>().0 {
            chunk.copy_from_slice(&color);
        }
    }

    pub fn set_pixel(&mut self, x: u32, y: u32, color: [u8; 4]) {
        if x < self.width && y < self.height {
            let offset = ((y * self.width + x) * 4) as usize;
            self.pixels[offset..offset + 4].copy_from_slice(&color);
        }
    }

    pub fn get_pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x < self.width && y < self.height {
            let offset = ((y * self.width + x) * 4) as usize;
            Some([
                self.pixels[offset],
                self.pixels[offset + 1],
                self.pixels[offset + 2],
                self.pixels[offset + 3],
            ])
        } else {
            None
        }
    }

    pub fn to_raw_rgba(&self) -> &[u8] {
        &self.pixels
    }

    pub fn byte_len(&self) -> usize {
        self.pixels.len()
    }
}

/// Direct low-level render pass encoder for submitting raw primitives directly to hardware.
#[derive(Debug, Clone)]
pub struct LowLevelRenderer {
    pub width: u32,
    pub height: u32,
    quad_count: usize,
    damaged_rects: Vec<(u32, u32, u32, u32)>,
    buffer: OffscreenBuffer,
}

impl LowLevelRenderer {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            quad_count: 0,
            damaged_rects: Vec::new(),
            buffer: OffscreenBuffer::new(width, height),
        }
    }

    /// Submits a colored quad without going through declarative RSX layout or DOM tree.
    pub fn submit_quad(&mut self, x: u32, y: u32, w: u32, h: u32, color: [u8; 4]) {
        self.quad_count += 1;
        let x_end = (x + w).min(self.width);
        let y_end = (y + h).min(self.height);

        for py in y..y_end {
            for px in x..x_end {
                self.buffer.set_pixel(px, py, color);
            }
        }
    }

    /// Records a damaged dirty rectangle for partial swapchain blitting.
    pub fn damaged_blit(&mut self, x: u32, y: u32, w: u32, h: u32) {
        self.damaged_rects.push((x, y, w, h));
    }

    pub fn rendered_quad_count(&self) -> usize {
        self.quad_count
    }

    pub fn damaged_rect_count(&self) -> usize {
        self.damaged_rects.len()
    }

    pub fn buffer(&self) -> &OffscreenBuffer {
        &self.buffer
    }
}
