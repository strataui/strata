//! Shared memory frame buffer and pixel presentation pipeline.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use strata_primitives::geom::Rect;

/// Pixel color format for the rendered frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PixelFormat {
    #[default]
    Bgra8,
    Rgba8,
}

/// Thread-safe shared memory pixel surface received from Chromium OSR.
#[derive(Clone)]
pub struct ShmSurface {
    width: Arc<RwLock<u32>>,
    height: Arc<RwLock<u32>>,
    stride: Arc<RwLock<u32>>,
    format: PixelFormat,
    pixels: Arc<RwLock<Vec<u8>>>,
    frame_index: Arc<AtomicU64>,
    damage_rects: Arc<RwLock<Vec<Rect>>>,
}

impl ShmSurface {
    /// Creates a new shared memory surface initialized to blank background.
    pub fn new(width: u32, height: u32, format: PixelFormat) -> Self {
        let stride = width * 4;
        let byte_len = (stride * height) as usize;
        let pixels = vec![255u8; byte_len]; // Default white background

        Self {
            width: Arc::new(RwLock::new(width)),
            height: Arc::new(RwLock::new(height)),
            stride: Arc::new(RwLock::new(stride)),
            format,
            pixels: Arc::new(RwLock::new(pixels)),
            frame_index: Arc::new(AtomicU64::new(0)),
            damage_rects: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Viewport width in pixels.
    pub fn width(&self) -> u32 {
        *self.width.read().unwrap()
    }

    /// Viewport height in pixels.
    pub fn height(&self) -> u32 {
        *self.height.read().unwrap()
    }

    /// Byte stride per horizontal scanline.
    pub fn stride(&self) -> u32 {
        *self.stride.read().unwrap()
    }

    /// Pixel format.
    pub fn format(&self) -> PixelFormat {
        self.format
    }

    /// Current frame monotonic revision index.
    pub fn frame_index(&self) -> u64 {
        self.frame_index.load(Ordering::Relaxed)
    }

    /// Resizes the shared memory backing store to accommodate new viewport dimensions.
    pub fn resize(&self, new_width: u32, new_height: u32) {
        let new_stride = new_width * 4;
        let new_byte_len = (new_stride * new_height) as usize;

        let mut w = self.width.write().unwrap();
        let mut h = self.height.write().unwrap();
        let mut s = self.stride.write().unwrap();
        let mut px = self.pixels.write().unwrap();

        *w = new_width;
        *h = new_height;
        *s = new_stride;

        px.resize(new_byte_len, 255u8);
        self.frame_index.fetch_add(1, Ordering::Relaxed);
    }

    /// Blits new raster data into the shared surface and updates dirty damage rects.
    pub fn update_pixels(&self, src: &[u8], damage: Vec<Rect>) {
        let mut px = self.pixels.write().unwrap();
        let copy_len = px.len().min(src.len());
        px[..copy_len].copy_from_slice(&src[..copy_len]);

        let mut d = self.damage_rects.write().unwrap();
        *d = damage;

        self.frame_index.fetch_add(1, Ordering::Relaxed);
    }

    /// Reads back a copy of the raw pixel bytes for software rendering or GPU upload.
    pub fn clone_pixels(&self) -> Vec<u8> {
        self.pixels.read().unwrap().clone()
    }

    /// Samples a single pixel color `[r, g, b, a]` at coordinates `(x, y)`.
    pub fn sample_pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        let w = self.width();
        let h = self.height();
        let stride = self.stride() as usize;

        if x >= w || y >= h {
            return None;
        }

        let px = self.pixels.read().unwrap();
        let offset = (y as usize) * stride + (x as usize) * 4;

        if offset + 4 <= px.len() {
            match self.format {
                PixelFormat::Bgra8 => {
                    let b = px[offset];
                    let g = px[offset + 1];
                    let r = px[offset + 2];
                    let a = px[offset + 3];
                    Some([r, g, b, a])
                }
                PixelFormat::Rgba8 => {
                    let r = px[offset];
                    let g = px[offset + 1];
                    let b = px[offset + 2];
                    let a = px[offset + 3];
                    Some([r, g, b, a])
                }
            }
        } else {
            None
        }
    }

    /// Fills a region in the surface with a solid color (used by mock renderers).
    pub fn fill_rect(&self, rect: Rect, color: [u8; 4]) {
        let w = self.width();
        let h = self.height();
        let stride = self.stride() as usize;

        let x0 = (rect.min_x() as u32).min(w);
        let x1 = (rect.max_x() as u32).min(w);
        let y0 = (rect.min_y() as u32).min(h);
        let y1 = (rect.max_y() as u32).min(h);

        let mut px = self.pixels.write().unwrap();

        let (b0, b1, b2, b3) = match self.format {
            PixelFormat::Bgra8 => (color[2], color[1], color[0], color[3]),
            PixelFormat::Rgba8 => (color[0], color[1], color[2], color[3]),
        };

        for y in y0..y1 {
            let row_offset = (y as usize) * stride;
            for x in x0..x1 {
                let pixel_offset = row_offset + (x as usize) * 4;
                if pixel_offset + 4 <= px.len() {
                    px[pixel_offset] = b0;
                    px[pixel_offset + 1] = b1;
                    px[pixel_offset + 2] = b2;
                    px[pixel_offset + 3] = b3;
                }
            }
        }

        self.frame_index.fetch_add(1, Ordering::Relaxed);
    }

    /// Returns the currently active damage rects.
    pub fn damage_rects(&self) -> Vec<Rect> {
        self.damage_rects.read().unwrap().clone()
    }

    /// Serializes this surface into a binary shared memory buffer (header + raw pixel payload).
    pub fn serialize_shm_buffer(&self) -> Vec<u8> {
        let w = self.width();
        let h = self.height();
        let stride = self.stride();
        let format_id = match self.format {
            PixelFormat::Bgra8 => 0,
            PixelFormat::Rgba8 => 1,
        };
        let rev = self.frame_index();
        let damage = self.damage_rects();

        let mut damage_array = [[0.0f32; 4]; 16];
        let count = damage.len().min(16);
        for (i, r) in damage.iter().take(count).enumerate() {
            damage_array[i] = [r.min_x(), r.min_y(), r.width(), r.height()];
        }

        let header = ShmBufferHeader {
            magic: SHM_MAGIC,
            version: 1,
            width: w,
            height: h,
            stride,
            format: format_id,
            revision: rev,
            damage_count: count as u32,
            damage_rects: damage_array,
        };

        let px = self.pixels.read().unwrap();
        let mut buffer = Vec::with_capacity(SHM_HEADER_SIZE + px.len());
        // Safety: ShmBufferHeader is POD types with #[repr(C)]
        let header_slice: &[u8] = unsafe {
            std::slice::from_raw_parts(
                &header as *const ShmBufferHeader as *const u8,
                SHM_HEADER_SIZE,
            )
        };
        buffer.extend_from_slice(header_slice);
        buffer.extend_from_slice(&px);
        buffer
    }

    /// Deserializes a binary shared memory buffer, updating the surface dimensions and pixels.
    pub fn deserialize_shm_buffer(&self, buffer: &[u8]) -> Result<(), ShmError> {
        if buffer.len() < SHM_HEADER_SIZE {
            return Err(ShmError::BufferTooSmall {
                required: SHM_HEADER_SIZE,
                provided: buffer.len(),
            });
        }

        let header = unsafe { std::ptr::read_unaligned(buffer.as_ptr() as *const ShmBufferHeader) };
        if header.magic != SHM_MAGIC {
            return Err(ShmError::InvalidMagic(header.magic));
        }
        if header.version != 1 {
            return Err(ShmError::UnsupportedVersion(header.version));
        }

        let expected_bytes = (header.stride * header.height) as usize;
        let total_required = SHM_HEADER_SIZE + expected_bytes;
        if buffer.len() < total_required {
            return Err(ShmError::BufferTooSmall {
                required: total_required,
                provided: buffer.len(),
            });
        }

        let pixel_slice = &buffer[SHM_HEADER_SIZE..total_required];
        let mut rects = Vec::with_capacity(header.damage_count as usize);
        for i in 0..(header.damage_count as usize).min(16) {
            let [x, y, w, h] = header.damage_rects[i];
            rects.push(Rect::from_xywh(x, y, w, h));
        }

        self.resize(header.width, header.height);
        self.update_pixels(pixel_slice, rects);
        self.frame_index.store(header.revision, Ordering::Relaxed);
        Ok(())
    }
}

/// Binary protocol header for zero-copy shared memory frame exchange with external CEF/Chromium processes.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShmBufferHeader {
    /// Magic identifier: 0x53545241 ('STRA')
    pub magic: u32,
    /// Protocol version: 1
    pub version: u32,
    /// Viewport width in pixels
    pub width: u32,
    /// Viewport height in pixels
    pub height: u32,
    /// Byte stride per row
    pub stride: u32,
    /// Pixel format: 0 for Bgra8, 1 for Rgba8
    pub format: u32,
    /// Monotonic frame revision number
    pub revision: u64,
    /// Number of dirty damage rectangles (max 16)
    pub damage_count: u32,
    /// Damage rectangles: [x, y, w, h] in f32
    pub damage_rects: [[f32; 4]; 16],
}

pub const SHM_MAGIC: u32 = 0x53545241; // 'STRA'
pub const SHM_HEADER_SIZE: usize = std::mem::size_of::<ShmBufferHeader>();

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShmError {
    BufferTooSmall { required: usize, provided: usize },
    InvalidMagic(u32),
    UnsupportedVersion(u32),
    DimensionMismatch { expected: (u32, u32), received: (u32, u32) },
}

impl std::fmt::Display for ShmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BufferTooSmall { required, provided } => write!(f, "SHM buffer too small: required {}, got {}", required, provided),
            Self::InvalidMagic(m) => write!(f, "Invalid SHM magic: 0x{:08X}", m),
            Self::UnsupportedVersion(v) => write!(f, "Unsupported SHM protocol version: {}", v),
            Self::DimensionMismatch { expected, received } => write!(f, "SHM dimension mismatch: expected {:?}, got {:?}", expected, received),
        }
    }
}

impl std::error::Error for ShmError {}
