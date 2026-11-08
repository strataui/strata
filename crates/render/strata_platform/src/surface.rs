//! Abstract hardware presentation surfaces, swapchain configuration, and frame pacing.

use std::time::{Duration, Instant};

pub use strata_render::swapchain::{
    PresentError, SoftwareSurfacePresenter, SurfacePresenter, SwapchainCompositor, SwapchainFormat,
};

/// Presentation mode defining how swapchain images are presented to the display server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PresentMode {
    /// Presentation happens immediately without waiting for vertical blanking (may tear).
    Immediate,
    /// Triple-buffering with lowest input latency: unpresented frames are replaced by newer frames.
    Mailbox,
    /// Standard VSync FIFO presentation queue (zero tearing, 60/120 FPS capped).
    #[default]
    Fifo,
    /// VSync FIFO with adaptive tearing if frame deadline is missed.
    FifoRelaxed,
}

/// Target display refresh rate for frame pacing and VSync synchronization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[derive(Default)]
pub enum RefreshRate {
    #[default]
    Hz60,
    Hz90,
    Hz120,
    Hz144,
    Custom(u32),
}


impl RefreshRate {
    /// Returns integer frequency in Hertz.
    pub const fn as_hz(&self) -> u32 {
        match self {
            Self::Hz60 => 60,
            Self::Hz90 => 90,
            Self::Hz120 => 120,
            Self::Hz144 => 144,
            Self::Custom(hz) => *hz,
        }
    }

    /// Frame duration interval for this refresh rate.
    pub fn frame_interval(&self) -> Duration {
        let hz = self.as_hz().max(1);
        Duration::from_nanos(1_000_000_000 / hz as u64)
    }
}

/// Configuration descriptor for a presentation surface and its swapchain.
#[derive(Debug, Clone)]
pub struct SurfaceConfig {
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
    pub format: SwapchainFormat,
    pub present_mode: PresentMode,
    pub refresh_rate: RefreshRate,
}

impl Default for SurfaceConfig {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 800,
            scale_factor: 1.0,
            format: SwapchainFormat::Bgra8Unorm,
            present_mode: PresentMode::Fifo,
            refresh_rate: RefreshRate::Hz60,
        }
    }
}

impl SurfaceConfig {
    /// Physical width in backing store pixels: ⌈logical_width * scale_factor⌉.
    pub fn physical_width(&self) -> u32 {
        ((self.width as f32 * self.scale_factor) + 0.5) as u32
    }

    /// Physical height in backing store pixels: ⌈logical_height * scale_factor⌉.
    pub fn physical_height(&self) -> u32 {
        ((self.height as f32 * self.scale_factor) + 0.5) as u32
    }

    /// Physical dimensions as (width, height) tuple.
    pub fn physical_size(&self) -> (u32, u32) {
        (self.physical_width(), self.physical_height())
    }

    /// Total byte footprint for a single physical frame buffer at this configuration.
    pub fn buffer_byte_size(&self) -> usize {
        let (pw, ph) = self.physical_size();
        (pw as usize) * (ph as usize) * self.format.bytes_per_pixel()
    }
}

/// Frame pacing coordinator that stabilizes frame presentation deadlines and calculates sleep intervals.
#[derive(Debug, Clone)]
pub struct FramePacer {
    pub target_refresh: RefreshRate,
    pub frame_interval: Duration,
    pub last_frame_instant: Option<Instant>,
    pub average_frame_duration: Duration,
    pub frame_count: u64,
}

impl FramePacer {
    /// Creates a new frame pacer locked to the given target refresh rate.
    pub fn new(target_refresh: RefreshRate) -> Self {
        let frame_interval = target_refresh.frame_interval();
        Self {
            target_refresh,
            frame_interval,
            last_frame_instant: None,
            average_frame_duration: Duration::from_micros(500),
            frame_count: 0,
        }
    }

    /// Marks the beginning of a frame tick, returning the current timestamp.
    pub fn begin_frame(&mut self) -> Instant {
        Instant::now()
    }

    /// Marks the completion of a frame, updating exponential moving average of frame duration.
    pub fn end_frame(&mut self, frame_start: Instant) -> Duration {
        let duration = frame_start.elapsed();
        self.frame_count += 1;
        self.last_frame_instant = Some(Instant::now());

        // EWMA update with alpha = 0.2
        let alpha = 0.2;
        let prev_micros = self.average_frame_duration.as_micros() as f64;
        let curr_micros = duration.as_micros() as f64;
        let new_micros = (alpha * curr_micros + (1.0 - alpha) * prev_micros).max(10.0);
        self.average_frame_duration = Duration::from_micros(new_micros as u64);

        duration
    }

    /// Computes duration until next VSync deadline to avoid busy spinning.
    pub fn duration_until_next_deadline(&self) -> Duration {
        if let Some(last) = self.last_frame_instant {
            let elapsed = last.elapsed();
            if elapsed < self.frame_interval {
                self.frame_interval - elapsed
            } else {
                Duration::ZERO
            }
        } else {
            Duration::ZERO
        }
    }

    /// Synchronizes execution with the hardware VSync signal or refresh rate deadline.
    pub fn wait_vsync(&mut self) {
        #[cfg(windows)]
        {
            unsafe {
                extern "system" {
                    fn LoadLibraryA(lpLibFileName: *const u8) -> *mut std::ffi::c_void;
                    fn GetProcAddress(
                        hModule: *mut std::ffi::c_void,
                        lpProcName: *const u8,
                    ) -> *mut std::ffi::c_void;
                    fn FreeLibrary(hLibModule: *mut std::ffi::c_void) -> i32;
                }
                let dwmapi = LoadLibraryA(b"dwmapi.dll\0".as_ptr());
                if !dwmapi.is_null() {
                    let proc = GetProcAddress(dwmapi, b"DwmFlush\0".as_ptr());
                    if !proc.is_null() {
                        let dwm_flush: unsafe extern "system" fn() -> i32 =
                            std::mem::transmute(proc);
                        let _ = dwm_flush();
                    }
                    FreeLibrary(dwmapi);
                }
            }
        }

        // High-precision deadline sleep for remaining duration until next frame
        let remaining = self.duration_until_next_deadline();
        if remaining > Duration::from_micros(500) {
            std::thread::sleep(remaining - Duration::from_micros(250));
        }
        // Sub-millisecond spin loop to nail the deadline
        while self.duration_until_next_deadline() > Duration::ZERO {
            std::hint::spin_loop();
        }
        self.last_frame_instant = Some(Instant::now());
    }
}

/// Abstract opaque handle to a native window surface for presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceHandle {
    pub window_id: u64,
    pub raw_window_ptr: usize,
    pub raw_display_ptr: usize,
    pub width: u32,
    pub height: u32,
    pub scale_factor_snapped: u32, // Fixed-point 1/64th scale factor
}

impl SurfaceHandle {
    /// Creates a headless surface handle for testing, CI automation, and offscreen rendering.
    pub fn new_headless(width: u32, height: u32, scale_factor: f32) -> Self {
        Self {
            window_id: 1,
            raw_window_ptr: 0,
            raw_display_ptr: 0,
            width,
            height,
            scale_factor_snapped: (scale_factor * 64.0) as u32,
        }
    }

    /// Scale factor as a floating-point multiplier.
    pub fn scale_factor(&self) -> f32 {
        self.scale_factor_snapped as f32 / 64.0
    }
}
