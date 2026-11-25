//! Pure-Rust fast video pixel format converters (YUYV, NV12, I420, BGRA, RGB24 -> RGBA8)
//! and zero-copy `strata-render` `FrameBuffer` bridge.

use strata_render::FrameBuffer;

/// Pixel encoding format reported by camera, screen-capture, or video decoder backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PixelFormat {
    Rgba8,
    Bgra8,
    Rgb24,
    /// Packed YUV 4:2:2 (`Y0 U0 Y1 V0` — standard V4L2 webcam format).
    Yuyv,
    /// Semi-planar YUV 4:2:0 (full Y plane + interleaved UV plane — AVFoundation / MediaFoundation / VAAPI).
    Nv12,
    /// Planar YUV 4:2:0 (Y plane, U plane, V plane — WebRTC / software decoders).
    I420,
    /// Motion JPEG frame payload.
    Mjpeg,
}

/// Decoded RGBA8 video frame ready for direct `strata-render` GPU or software presentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    /// Presentation timestamp in microseconds.
    pub pts_us: u64,
    /// Monotonic frame sequence number.
    pub sequence: u64,
    /// Tightly packed RGBA8 pixel bytes (`width * height * 4`).
    pub rgba: Vec<u8>,
}

impl VideoFrame {
    /// Creates a blank black opaque RGBA8 frame of dimensions `width x height`.
    pub fn blank(width: u32, height: u32) -> Self {
        let len = (width as usize).saturating_mul(height as usize).saturating_mul(4);
        let mut rgba = vec![0u8; len];
        for px in rgba.as_chunks_mut::<4>().0 {
            px[3] = 255;
        }
        Self {
            width,
            height,
            pts_us: 0,
            sequence: 0,
            rgba,
        }
    }

    /// Returns `true` if `rgba.len() == width * height * 4`.
    pub fn is_valid(&self) -> bool {
        self.width > 0
            && self.height > 0
            && self.rgba.len() == (self.width as usize) * (self.height as usize) * 4
    }

    /// Converts this video frame directly into a [`strata_render::FrameBuffer`] for rendering.
    pub fn to_framebuffer(&self) -> FrameBuffer {
        let mut fb = FrameBuffer::new(self.width as usize, self.height as usize);
        let copy_len = fb.pixels.len().min(self.rgba.len());
        fb.pixels[..copy_len].copy_from_slice(&self.rgba[..copy_len]);
        fb
    }
}

/// Fast integer fixed-point BT.601 YUV -> RGB conversion (studio swing `[16..235]`, `[16..240]`).
#[inline]
pub fn yuv_to_rgb_bt601(y: u8, u: u8, v: u8) -> (u8, u8, u8) {
    let c = (y as i32 - 16).max(0);
    let d = u as i32 - 128;
    let e = v as i32 - 128;
    let r = (298 * c + 409 * e + 128) >> 8;
    let g = (298 * c - 100 * d - 208 * e + 128) >> 8;
    let b = (298 * c + 516 * d + 128) >> 8;
    (
        r.clamp(0, 255) as u8,
        g.clamp(0, 255) as u8,
        b.clamp(0, 255) as u8,
    )
}

/// Converts packed YUYV 4:2:2 (`width * height * 2` bytes) into tightly packed RGBA8.
pub fn yuyv_to_rgba(src: &[u8], width: u32, height: u32, dst: &mut Vec<u8>) {
    let pixels = (width as usize).saturating_mul(height as usize);
    dst.resize(pixels * 4, 255);
    let pairs = (pixels / 2).min(src.len() / 4);
    for i in 0..pairs {
        let s = i * 4;
        let y0 = src[s];
        let u = src[s + 1];
        let y1 = src[s + 2];
        let v = src[s + 3];

        let (r0, g0, b0) = yuv_to_rgb_bt601(y0, u, v);
        let (r1, g1, b1) = yuv_to_rgb_bt601(y1, u, v);

        let d = i * 8;
        dst[d] = r0;
        dst[d + 1] = g0;
        dst[d + 2] = b0;
        dst[d + 3] = 255;
        dst[d + 4] = r1;
        dst[d + 5] = g1;
        dst[d + 6] = b1;
        dst[d + 7] = 255;
    }
}

/// Converts semi-planar NV12 (YUV 4:2:0) into tightly packed RGBA8.
pub fn nv12_to_rgba(y_plane: &[u8], uv_plane: &[u8], width: u32, height: u32, dst: &mut Vec<u8>) {
    let w = width as usize;
    let h = height as usize;
    dst.resize(w * h * 4, 255);
    let uv_stride = w.div_ceil(2) * 2;
    for row in 0..h {
        let uv_row = (row / 2) * uv_stride;
        for col in 0..w {
            let y_idx = row * w + col;
            let uv_idx = uv_row + (col / 2) * 2;
            let y = y_plane.get(y_idx).copied().unwrap_or(16);
            let u = uv_plane.get(uv_idx).copied().unwrap_or(128);
            let v = uv_plane.get(uv_idx + 1).copied().unwrap_or(128);
            let (r, g, b) = yuv_to_rgb_bt601(y, u, v);
            let d = y_idx * 4;
            dst[d] = r;
            dst[d + 1] = g;
            dst[d + 2] = b;
            dst[d + 3] = 255;
        }
    }
}

/// Converts planar I420 (YUV 4:2:0) into tightly packed RGBA8.
pub fn i420_to_rgba(
    y_plane: &[u8],
    u_plane: &[u8],
    v_plane: &[u8],
    width: u32,
    height: u32,
    dst: &mut Vec<u8>,
) {
    let w = width as usize;
    let h = height as usize;
    let cw = w.div_ceil(2);
    dst.resize(w * h * 4, 255);
    for row in 0..h {
        let c_row = (row / 2) * cw;
        for col in 0..w {
            let y_idx = row * w + col;
            let c_idx = c_row + (col / 2);
            let y = y_plane.get(y_idx).copied().unwrap_or(16);
            let u = u_plane.get(c_idx).copied().unwrap_or(128);
            let v = v_plane.get(c_idx).copied().unwrap_or(128);
            let (r, g, b) = yuv_to_rgb_bt601(y, u, v);
            let d = y_idx * 4;
            dst[d] = r;
            dst[d + 1] = g;
            dst[d + 2] = b;
            dst[d + 3] = 255;
        }
    }
}

/// Converts BGRA8 into RGBA8 (zero-alloc if `dst` already has sufficient capacity).
pub fn bgra_to_rgba(src: &[u8], dst: &mut Vec<u8>) {
    let px_count = src.len() / 4;
    dst.resize(px_count * 4, 255);
    for i in 0..px_count {
        let base = i * 4;
        dst[base] = src[base + 2];
        dst[base + 1] = src[base + 1];
        dst[base + 2] = src[base];
        dst[base + 3] = src[base + 3];
    }
}

/// Converts RGB24 into opaque RGBA8.
pub fn rgb24_to_rgba(src: &[u8], dst: &mut Vec<u8>) {
    let px_count = src.len() / 3;
    dst.resize(px_count * 4, 255);
    for i in 0..px_count {
        let s = i * 3;
        let d = i * 4;
        dst[d] = src[s];
        dst[d + 1] = src[s + 1];
        dst[d + 2] = src[s + 2];
        dst[d + 3] = 255;
    }
}
