//! Cross-platform camera capture (zero-C-dep Linux V4L2 `mmap` ioctls + deterministic
//! `MockCameraSource` SMPTE color bar generator + reactive `use_camera` / `use_cameras` hooks).

use crate::pixel_format::{PixelFormat, VideoFrame};
#[cfg(target_os = "linux")]
use crate::pixel_format::yuyv_to_rgba;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use strata_dom::Signal;

/// Metadata describing a discovered physical or virtual camera endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraDeviceInfo {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub bus_info: String,
    pub default_width: u32,
    pub default_height: u32,
    pub default_fps: u32,
    pub formats: Vec<PixelFormat>,
}

/// Requested stream parameters when opening a camera.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraConfig {
    pub device_id: Option<String>,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub pixel_format: PixelFormat,
}

impl Default for CameraConfig {
    fn default() -> Self {
        Self {
            device_id: None,
            width: 640,
            height: 480,
            fps: 30,
            pixel_format: PixelFormat::Yuyv,
        }
    }
}

/// Errors arising from camera discovery or frame capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CameraError {
    DeviceNotFound(String),
    PermissionDenied(String),
    UnsupportedFormat(String),
    IoError(String),
}

impl std::fmt::Display for CameraError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DeviceNotFound(s) => write!(f, "Camera device not found: {s}"),
            Self::PermissionDenied(s) => write!(f, "Camera permission denied: {s}"),
            Self::UnsupportedFormat(s) => write!(f, "Unsupported camera format: {s}"),
            Self::IoError(s) => write!(f, "Camera I/O error: {s}"),
        }
    }
}

impl std::error::Error for CameraError {}

// ---------------------------------------------------------------------------
// Linux V4L2 Zero-C-Dependency Kernel IOCTL Definitions
// ---------------------------------------------------------------------------
#[cfg(target_os = "linux")]
mod v4l2 {
    pub const VIDIOC_QUERYCAP: libc::c_ulong = 0x8068_5600;
    pub const VIDIOC_S_FMT: libc::c_ulong = 0xC0CC_5605;
    pub const VIDIOC_REQBUFS: libc::c_ulong = 0xC014_5608;
    pub const VIDIOC_QUERYBUF: libc::c_ulong = 0xC058_5609;
    pub const VIDIOC_QBUF: libc::c_ulong = 0xC058_560F;
    pub const VIDIOC_DQBUF: libc::c_ulong = 0xC058_5611;
    pub const VIDIOC_STREAMON: libc::c_ulong = 0x4004_5612;
    pub const VIDIOC_STREAMOFF: libc::c_ulong = 0x4004_5613;

    pub const V4L2_BUF_TYPE_VIDEO_CAPTURE: u32 = 1;
    pub const V4L2_MEMORY_MMAP: u32 = 1;
    pub const V4L2_CAP_VIDEO_CAPTURE: u32 = 0x0000_0001;
    pub const V4L2_CAP_DEVICE_CAPS: u32 = 0x8000_0000;
    pub const V4L2_PIX_FMT_YUYV: u32 = u32::from_le_bytes(*b"YUYV");

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct V4l2Capability {
        pub driver: [u8; 16],
        pub card: [u8; 32],
        pub bus_info: [u8; 32],
        pub version: u32,
        pub capabilities: u32,
        pub device_caps: u32,
        pub reserved: [u32; 3],
    }

    impl Default for V4l2Capability {
        fn default() -> Self {
            unsafe { std::mem::zeroed() }
        }
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct V4l2PixFormat {
        pub width: u32,
        pub height: u32,
        pub pixelformat: u32,
        pub field: u32,
        pub bytesperline: u32,
        pub sizeimage: u32,
        pub colorspace: u32,
        pub priv_field: u32,
        pub flags: u32,
        pub ycbcr_enc: u32,
        pub quantization: u32,
        pub xfer_func: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct V4l2Format {
        pub buf_type: u32,
        pub _pad: u32,
        pub pix: V4l2PixFormat,
        pub _raw_tail: [u8; 200 - std::mem::size_of::<V4l2PixFormat>()],
    }

    impl Default for V4l2Format {
        fn default() -> Self {
            unsafe { std::mem::zeroed() }
        }
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct V4l2RequestBuffers {
        pub count: u32,
        pub buf_type: u32,
        pub memory: u32,
        pub capabilities: u32,
        pub reserved: [u32; 1],
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct V4l2Timeval {
        pub tv_sec: libc::c_long,
        pub tv_usec: libc::c_long,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct V4l2Timecode {
        pub kind: u32,
        pub flags: u32,
        pub frames: u8,
        pub seconds: u8,
        pub minutes: u8,
        pub hours: u8,
        pub userbits: [u8; 4],
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct V4l2Buffer {
        pub index: u32,
        pub buf_type: u32,
        pub bytesused: u32,
        pub flags: u32,
        pub field: u32,
        pub timestamp: V4l2Timeval,
        pub timecode: V4l2Timecode,
        pub sequence: u32,
        pub memory: u32,
        pub m_offset: u64,
        pub length: u32,
        pub reserved2: u32,
        pub request_fd: i32,
    }

    impl Default for V4l2Buffer {
        fn default() -> Self {
            unsafe { std::mem::zeroed() }
        }
    }

    pub fn cstr_to_string(buf: &[u8]) -> String {
        let nul = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        String::from_utf8_lossy(&buf[..nul]).trim().to_string()
    }
}

/// Discovers physical camera devices attached to the host OS.
pub fn list_cameras() -> Vec<CameraDeviceInfo> {
    #[allow(unused_mut)]
    let mut devices = Vec::new();
    #[cfg(target_os = "linux")]
    {
        use std::ffi::CString;
        for idx in 0..64 {
            let path = format!("/dev/video{idx}");
            if !std::path::Path::new(&path).exists() {
                continue;
            }
            let Ok(c_path) = CString::new(path.as_str()) else {
                continue;
            };
            let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_RDWR | libc::O_NONBLOCK) };
            if fd < 0 {
                continue;
            }
            let mut cap = v4l2::V4l2Capability::default();
            let rc = unsafe { libc::ioctl(fd, v4l2::VIDIOC_QUERYCAP as _, &mut cap) };
            unsafe {
                libc::close(fd);
            }
            if rc == 0 {
                let effective_caps = if cap.capabilities & v4l2::V4L2_CAP_DEVICE_CAPS != 0 {
                    cap.device_caps
                } else {
                    cap.capabilities
                };
                if effective_caps & v4l2::V4L2_CAP_VIDEO_CAPTURE != 0 {
                    let name = v4l2::cstr_to_string(&cap.card);
                    devices.push(CameraDeviceInfo {
                        id: path.clone(),
                        name: if name.is_empty() { path } else { name },
                        driver: v4l2::cstr_to_string(&cap.driver),
                        bus_info: v4l2::cstr_to_string(&cap.bus_info),
                        default_width: 640,
                        default_height: 480,
                        default_fps: 30,
                        formats: vec![PixelFormat::Yuyv, PixelFormat::Mjpeg],
                    });
                }
            }
        }
    }
    devices
}

/// Active live hardware camera capture stream (V4L2 `mmap` ring on Linux).
pub struct CameraStream {
    pub device_id: String,
    pub width: u32,
    pub height: u32,
    running: Arc<AtomicBool>,
    frame_count: Arc<AtomicU64>,
    latest_frame: Arc<Mutex<Option<Arc<VideoFrame>>>>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl CameraStream {
    /// Opens a physical camera device and starts background RGBA frame streaming.
    pub fn open(config: CameraConfig) -> Result<Self, CameraError> {
        #[cfg(target_os = "linux")]
        {
            Self::open_linux_v4l2(config)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = config;
            Err(CameraError::DeviceNotFound(
                "Hardware camera backend not active on this target; use MockCameraSource".into(),
            ))
        }
    }

    #[cfg(target_os = "linux")]
    fn open_linux_v4l2(config: CameraConfig) -> Result<Self, CameraError> {
        use std::ffi::CString;

        let device_id = match config.device_id {
            Some(id) => id,
            None => list_cameras()
                .into_iter()
                .next()
                .map(|c| c.id)
                .ok_or_else(|| CameraError::DeviceNotFound("No V4L2 capture device found".into()))?,
        };

        let c_path = CString::new(device_id.as_str())
            .map_err(|_| CameraError::DeviceNotFound(device_id.clone()))?;
        let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_RDWR | libc::O_NONBLOCK) };
        if fd < 0 {
            let err = std::io::Error::last_os_error();
            return Err(if err.kind() == std::io::ErrorKind::PermissionDenied {
                CameraError::PermissionDenied(format!("{device_id}: {err}"))
            } else {
                CameraError::DeviceNotFound(format!("{device_id}: {err}"))
            });
        }

        // Negotiate YUYV pixel format
        let mut fmt = v4l2::V4l2Format {
            buf_type: v4l2::V4L2_BUF_TYPE_VIDEO_CAPTURE,
            ..Default::default()
        };
        fmt.pix.width = config.width.max(160);
        fmt.pix.height = config.height.max(120);
        fmt.pix.pixelformat = v4l2::V4L2_PIX_FMT_YUYV;
        if unsafe { libc::ioctl(fd, v4l2::VIDIOC_S_FMT as _, &mut fmt) } < 0 {
            let err = std::io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(CameraError::UnsupportedFormat(format!(
                "VIDIOC_S_FMT failed: {err}"
            )));
        }

        let width = fmt.pix.width;
        let height = fmt.pix.height;

        // Request 4 kernel mmap buffers
        let mut req = v4l2::V4l2RequestBuffers {
            count: 4,
            buf_type: v4l2::V4L2_BUF_TYPE_VIDEO_CAPTURE,
            memory: v4l2::V4L2_MEMORY_MMAP,
            ..Default::default()
        };
        if unsafe { libc::ioctl(fd, v4l2::VIDIOC_REQBUFS as _, &mut req) } < 0 || req.count == 0 {
            let err = std::io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(CameraError::IoError(format!("VIDIOC_REQBUFS failed: {err}")));
        }

        let mut mapped: Vec<(*mut libc::c_void, usize)> = Vec::new();
        for idx in 0..req.count {
            let mut buf = v4l2::V4l2Buffer {
                index: idx,
                buf_type: v4l2::V4L2_BUF_TYPE_VIDEO_CAPTURE,
                memory: v4l2::V4L2_MEMORY_MMAP,
                ..Default::default()
            };
            if unsafe { libc::ioctl(fd, v4l2::VIDIOC_QUERYBUF as _, &mut buf) } < 0 {
                for (ptr, len) in mapped {
                    unsafe { libc::munmap(ptr, len) };
                }
                unsafe { libc::close(fd) };
                return Err(CameraError::IoError("VIDIOC_QUERYBUF failed".into()));
            }
            let ptr = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    buf.length as usize,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_SHARED,
                    fd,
                    buf.m_offset as libc::off_t,
                )
            };
            if ptr == libc::MAP_FAILED {
                for (p, len) in mapped {
                    unsafe { libc::munmap(p, len) };
                }
                unsafe { libc::close(fd) };
                return Err(CameraError::IoError("V4L2 mmap failed".into()));
            }
            mapped.push((ptr, buf.length as usize));
            unsafe {
                let _ = libc::ioctl(fd, v4l2::VIDIOC_QBUF as _, &mut buf);
            }
        }

        let mut buf_type = v4l2::V4L2_BUF_TYPE_VIDEO_CAPTURE;
        if unsafe { libc::ioctl(fd, v4l2::VIDIOC_STREAMON as _, &mut buf_type) } < 0 {
            for (ptr, len) in mapped {
                unsafe { libc::munmap(ptr, len) };
            }
            unsafe { libc::close(fd) };
            return Err(CameraError::IoError("VIDIOC_STREAMON failed".into()));
        }

        // Wrap raw pointers in usize so they cross thread boundary cleanly (owned solely by worker).
        let mapped_addrs: Vec<(usize, usize)> =
            mapped.into_iter().map(|(p, l)| (p as usize, l)).collect();

        let running = Arc::new(AtomicBool::new(true));
        let frame_count = Arc::new(AtomicU64::new(0));
        let latest_frame = Arc::new(Mutex::new(None));

        let running_bg = running.clone();
        let count_bg = frame_count.clone();
        let latest_bg = latest_frame.clone();

        let worker = std::thread::Builder::new()
            .name("strata-v4l2-camera".into())
            .spawn(move || {
                let mut rgba_scratch = Vec::with_capacity((width * height * 4) as usize);
                while running_bg.load(Ordering::Relaxed) {
                    let mut pfd = libc::pollfd {
                        fd,
                        events: libc::POLLIN,
                        revents: 0,
                    };
                    let ready = unsafe { libc::poll(&mut pfd, 1, 50) };
                    if ready <= 0 {
                        continue;
                    }
                    let mut vbuf = v4l2::V4l2Buffer {
                        buf_type: v4l2::V4L2_BUF_TYPE_VIDEO_CAPTURE,
                        memory: v4l2::V4L2_MEMORY_MMAP,
                        ..Default::default()
                    };
                    if unsafe { libc::ioctl(fd, v4l2::VIDIOC_DQBUF as _, &mut vbuf) } == 0 {
                        if let Some(&(addr, len)) = mapped_addrs.get(vbuf.index as usize) {
                            let used = (vbuf.bytesused as usize).min(len);
                            let raw_slice =
                                unsafe { std::slice::from_raw_parts(addr as *const u8, used) };
                            yuyv_to_rgba(raw_slice, width, height, &mut rgba_scratch);
                            let seq = count_bg.fetch_add(1, Ordering::Relaxed) + 1;
                            let pts_us = (vbuf.timestamp.tv_sec as u64)
                                .saturating_mul(1_000_000)
                                .saturating_add(vbuf.timestamp.tv_usec as u64);
                            let frame = Arc::new(VideoFrame {
                                width,
                                height,
                                pts_us,
                                sequence: seq,
                                rgba: rgba_scratch.clone(),
                            });
                            if let Ok(mut guard) = latest_bg.lock() {
                                *guard = Some(frame);
                            }
                        }
                        unsafe {
                            let _ = libc::ioctl(fd, v4l2::VIDIOC_QBUF as _, &mut vbuf);
                        }
                    }
                }
                let mut bt = v4l2::V4L2_BUF_TYPE_VIDEO_CAPTURE;
                unsafe {
                    let _ = libc::ioctl(fd, v4l2::VIDIOC_STREAMOFF as _, &mut bt);
                    for (addr, len) in mapped_addrs {
                        libc::munmap(addr as *mut libc::c_void, len);
                    }
                    libc::close(fd);
                }
            })
            .map_err(|e| CameraError::IoError(format!("Failed to spawn camera thread: {e}")))?;

        Ok(Self {
            device_id,
            width,
            height,
            running,
            frame_count,
            latest_frame,
            worker: Some(worker),
        })
    }

    /// Returns the most recently captured RGBA [`VideoFrame`], if available.
    pub fn latest_frame(&self) -> Option<Arc<VideoFrame>> {
        self.latest_frame.lock().ok().and_then(|g| g.clone())
    }

    /// Returns total captured frames since stream start.
    pub fn frame_count(&self) -> u64 {
        self.frame_count.load(Ordering::Relaxed)
    }

    /// Stops capture cleanly.
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for CameraStream {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Deterministic synthetic camera producing SMPTE color bars + animated scanline for CI & previews.
pub struct MockCameraSource {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    sequence: AtomicU64,
    latest: Mutex<Option<Arc<VideoFrame>>>,
}

impl MockCameraSource {
    pub fn new(config: CameraConfig) -> Self {
        Self {
            width: config.width.max(16),
            height: config.height.max(16),
            fps: config.fps.max(1),
            sequence: AtomicU64::new(0),
            latest: Mutex::new(None),
        }
    }

    /// Synthesizes and publishes the next animated SMPTE RGBA8 frame.
    pub fn capture_next_frame(&self) -> Arc<VideoFrame> {
        let seq = self.sequence.fetch_add(1, Ordering::Relaxed) + 1;
        let w = self.width as usize;
        let h = self.height as usize;
        let mut rgba = vec![0u8; w * h * 4];

        // 7 classic SMPTE color bars
        const BARS: [(u8, u8, u8); 7] = [
            (192, 192, 192), // White/Gray
            (192, 192, 0),   // Yellow
            (0, 192, 192),   // Cyan
            (0, 192, 0),     // Green
            (192, 0, 192),   // Magenta
            (192, 0, 0),     // Red
            (0, 0, 192),     // Blue
        ];
        let scan_y = (seq as usize * 3) % h.max(1);

        for y in 0..h {
            let is_scan = y == scan_y || y + 1 == scan_y;
            for x in 0..w {
                let bar_idx = (x * BARS.len()) / w.max(1);
                let (r, g, b) = if is_scan {
                    (255, 255, 255)
                } else {
                    BARS[bar_idx.min(BARS.len() - 1)]
                };
                let d = (y * w + x) * 4;
                rgba[d] = r;
                rgba[d + 1] = g;
                rgba[d + 2] = b;
                rgba[d + 3] = 255;
            }
        }

        let pts_us = (seq * 1_000_000) / self.fps as u64;
        let frame = Arc::new(VideoFrame {
            width: self.width,
            height: self.height,
            pts_us,
            sequence: seq,
            rgba,
        });
        if let Ok(mut guard) = self.latest.lock() {
            *guard = Some(frame.clone());
        }
        frame
    }

    pub fn latest_frame(&self) -> Option<Arc<VideoFrame>> {
        self.latest.lock().ok().and_then(|g| g.clone())
    }

    pub fn frame_count(&self) -> u64 {
        self.sequence.load(Ordering::Relaxed)
    }
}

enum CameraBackend {
    Hardware(CameraStream),
    Mock(Arc<MockCameraSource>),
    Stopped,
}

/// Reactive Strata camera controller returned by [`use_camera`].
pub struct CameraHandle {
    config: CameraConfig,
    backend: RefCell<CameraBackend>,
    is_active: Signal<bool>,
    frame_count: Signal<u64>,
    latest_frame: Signal<Option<Arc<VideoFrame>>>,
}

impl CameraHandle {
    pub fn new(config: CameraConfig) -> Self {
        Self {
            config,
            backend: RefCell::new(CameraBackend::Stopped),
            is_active: Signal::new(false),
            frame_count: Signal::new(0),
            latest_frame: Signal::new(None),
        }
    }

    /// Starts live hardware camera streaming.
    pub fn start(&self) -> Result<(), CameraError> {
        let stream = CameraStream::open(self.config.clone())?;
        *self.backend.borrow_mut() = CameraBackend::Hardware(stream);
        self.is_active.set(true);
        Ok(())
    }

    /// Starts deterministic SMPTE mock camera streaming.
    pub fn start_mock(&self) -> Arc<MockCameraSource> {
        let mock = Arc::new(MockCameraSource::new(self.config.clone()));
        *self.backend.borrow_mut() = CameraBackend::Mock(mock.clone());
        self.is_active.set(true);
        mock
    }

    /// Stops capture and releases camera hardware.
    pub fn stop(&self) {
        *self.backend.borrow_mut() = CameraBackend::Stopped;
        self.is_active.set(false);
    }

    /// Pulls the latest frame into the reactive [`Self::latest_frame`] & [`Self::frame_count`] signals.
    pub fn poll_frame(&self) -> Option<Arc<VideoFrame>> {
        let (frame, count) = match &*self.backend.borrow() {
            CameraBackend::Hardware(s) => (s.latest_frame(), s.frame_count()),
            CameraBackend::Mock(m) => {
                let f = m.capture_next_frame();
                (Some(f), m.frame_count())
            }
            CameraBackend::Stopped => (None, 0),
        };
        if let Some(ref f) = frame {
            self.latest_frame.set(Some(f.clone()));
            self.frame_count.set(count);
        }
        frame
    }

    pub fn is_active(&self) -> Signal<bool> {
        self.is_active.clone()
    }

    pub fn frame_count(&self) -> Signal<u64> {
        self.frame_count.clone()
    }

    pub fn latest_frame(&self) -> Signal<Option<Arc<VideoFrame>>> {
        self.latest_frame.clone()
    }
}

/// Reactive hook enumerating all physical camera devices on the host.
pub fn use_cameras() -> Signal<Vec<CameraDeviceInfo>> {
    Signal::new(list_cameras())
}

/// Reactive hook returning a managed [`CameraHandle`] for live video capture.
pub fn use_camera(config: CameraConfig) -> CameraHandle {
    CameraHandle::new(config)
}
