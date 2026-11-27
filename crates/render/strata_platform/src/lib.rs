//! Strata Platform: Unified Native Hardware Abstraction Layer for Android, iOS, macOS, Windows, Linux, and Web.
//!
//! Provides zero-dependency platform windowing abstractions, client-side decorations (CSD),
//! desktop system tray integrations, and headless mock drivers for deterministic CI testing.
//!
//! # Examples
//!
//! ```rust
//! use strata_platform::WindowConfig;
//!
//! let mut config = WindowConfig::default();
//! config.title = "My Strata App".into();
//! assert_eq!(config.title, "My Strata App");
//! ```

#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::field_reassign_with_default,
    clippy::chunks_exact_to_as_chunks
)]

#[cfg(target_os = "android")]
pub mod android_window;
pub mod biometrics;
pub mod csd;
pub mod cursor_theme;
#[cfg(target_os = "linux")]
pub mod dbus;
pub mod dialog;
pub mod events;
#[cfg(all(target_os = "linux", feature = "gtk-compat"))]
pub mod gtk_window;
#[cfg(target_os = "ios")]
pub mod ios_window;
#[cfg(target_os = "macos")]
pub mod macos_window;
pub mod mcp_cursor;
pub mod menu;
pub mod notifications;
pub mod print;
pub mod raw_abi;
pub mod shortcuts;
pub mod single_instance;
pub mod surface;
pub mod tray;
pub mod vibrancy;
#[cfg(target_os = "linux")]
pub mod wayland_window;
#[cfg(feature = "wgpu-backend")]
pub mod wgpu_surface;
#[cfg(windows)]
pub mod win32_window;
pub mod window;
pub mod window_manager;
#[cfg(target_os = "linux")]
pub mod x11_window;

pub use biometrics::{
    authenticate_biometric, probe_biometric_capability, set_mock_biometric_result,
    BiometricAuthResult, BiometricCapability, BiometricKind,
};
#[cfg(all(target_os = "linux", feature = "gtk-compat"))]
pub use cairo;
pub use csd::{ControlPlacement, CsdController, CsdGeometry, CsdHitZone};
pub use cursor_theme::{CursorBitmap, CursorTheme};
#[cfg(any(test, feature = "test-internals"))]
pub use dialog::set_dialog_mock_response;
pub use dialog::{
    open_file, open_file_async, open_multiple_files, open_multiple_files_async, pick_folder,
    pick_folder_async, save_file, save_file_async, show_message_box, DialogFilter,
    DialogMockResponse, MessageBoxButtons, MessageBoxResult, MessageBoxType, OpenFileOptions,
    PickFolderOptions, SaveFileOptions,
};
pub use events::{
    decode_file_uri_to_path, parse_dropped_uri_list_to_paths, KeyCode, Modifiers, MouseButton,
    PlatformEvent, PointerKind, StylusPhase, TouchPhase,
};
pub use notifications::{
    close_notification, enable_notification_recorder, take_closed_notification_ids,
    take_recorded_notifications, NotificationAction, NotificationBuilder, NotificationRecord,
    NotificationUrgency,
};
pub use print::{
    list_printers, print_pdf_bytes, set_mock_print_spooler, take_mock_print_jobs, PrintDuplex,
    PrintJobOptions, PrinterInfo, RecordedPrintJob,
};
#[cfg(all(target_os = "linux", feature = "gtk-compat"))]
pub use gtk;
/// Snapshot of hardware GPU pass timings, Scene3D offscreen cache statistics, and VRAM allocation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlatformGpuTelemetry {
    pub gpu_timestamps_supported: bool,
    pub gpu_shadow_pass_us: u32,
    pub gpu_3d_pass_us: u32,
    pub gpu_2d_overlay_pass_us: u32,
    pub scene3d_cache_hits: u64,
    pub scene3d_cache_misses: u64,
    pub gpu_memory_bytes: u64,
}

/// Structured wall-clock breakdown returned by hybrid GTK + wgpu swapchain presentation so
/// [`FrameProfileRecord`] captures real 2D raster/upload, 3D GPU encode/submit, and swapchain
/// present durations instead of attributing everything to a single bucket.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GpuPresentTimings {
    /// True when the native wgpu swapchain rendered and presented the frame.
    pub presented: bool,
    /// Time spent rasterizing/uploading the 2D UI overlay (Cairo tiles + texture upload,
    /// or GPU 2D batch collection when `STRATA_GPU_2D=1`).
    pub raster_2d: std::time::Duration,
    /// Time spent acquiring the swapchain surface, encoding 3D/2D passes, and submitting to wgpu.
    pub wgpu_3d_submit: std::time::Duration,
    /// Time spent in `wgpu::SurfaceTexture::present()` (swapchain handoff / vsync).
    pub swapchain_present: std::time::Duration,
}

/// Owned surface pixel buffer captured on the UI thread so PNG compression & base64 encoding
/// can execute on a background worker thread without stalling rendering.
#[derive(Debug, Clone)]
pub struct CapturedSurfaceBuffer {
    pub width: u32,
    pub height: u32,
    pub stride: i32,
    pub pixels: Vec<u8>,
    pub is_cairo_argb32: bool,
}

impl CapturedSurfaceBuffer {
    /// Encodes the captured pixel buffer into PNG bytes. Safe to call from any background thread.
    pub fn encode_png_bytes(self) -> Result<Vec<u8>, String> {
        if self.width == 0 || self.height == 0 {
            return Err("Captured surface dimensions must be non-zero".to_string());
        }
        #[cfg(all(target_os = "linux", feature = "gtk-compat"))]
        if self.is_cairo_argb32 {
            let surface = cairo::ImageSurface::create_for_data(
                self.pixels,
                cairo::Format::ARgb32,
                self.width as i32,
                self.height as i32,
                self.stride,
            )
            .map_err(|e| format!("Failed to wrap captured surface for PNG encoding: {:?}", e))?;
            let mut buf = Vec::new();
            {
                let mut cursor = std::io::Cursor::new(&mut buf);
                surface
                    .write_to_png(&mut cursor)
                    .map_err(|e| format!("Failed to write PNG: {:?}", e))?;
            }
            return Ok(buf);
        }

        let fb = strata_render::FrameBuffer {
            width: self.width as usize,
            height: self.height as usize,
            pixels: self.pixels,
            clip_mask_stack: Vec::new(),
        };
        fb.to_png_bytes()
    }
}

#[cfg(all(target_os = "linux", feature = "gtk-compat"))]
pub use gtk_window::{
    cairo_rounded_rect, cairo_rounded_rect_radii, gtk_active_present_mode_is_vsync,
    gtk_begin_window_drag, gtk_close, gtk_current_modifiers, gtk_destroy_gpu_state,
    gtk_focus_window, gtk_get_clipboard_text, gtk_gpu_2d_enabled, gtk_gpu_surface_is_active,
    gtk_is_closing, gtk_is_maximized, gtk_last_gpu_telemetry, gtk_last_tile_cache_stats,
    gtk_minimize, gtk_present_active_gpu_surface_now, gtk_present_active_gpu_surface_now_timed,
    gtk_primary_refresh_interval, gtk_reset_ime, gtk_set_clipboard_text, gtk_set_cursor,
    gtk_set_gpu_2d_enabled, gtk_set_ime_cursor_location, gtk_set_ime_surrounding_text,
    gtk_take_scroll_pointer_pos, gtk_toggle_maximize, gtk_warp_cursor,
    init_pango_text_measurement, launch_gtk_window, launch_gtk_window_with_config,
    launch_gtk_window_with_scheduler, pango_font_metrics, pango_measure_styled_text,
    pango_measure_styled_text_family, pango_measure_text, pango_measure_text_family,
    post_platform_event, render_display_list_to_cairo, render_to_captured_buffer, render_to_png,
    render_to_png_bytes, resolved_font_family, set_cursor_position, try_present_display_list_gpu,
    try_present_display_list_gpu_timed, warp_cursor,
};
#[cfg(feature = "wgpu-backend")]
pub use wgpu_surface::{
    clear_thread_wgpu_renderer, render_scene_3d_wgpu_bgra, WgpuSwapchainPresenter,
    WgpuWindowPresenter,
};
pub use mcp_cursor::McpCursorState;
/// Pure-Rust headless software rasterization into an owned RGBA pixel buffer.
pub fn render_to_captured_buffer_pure(
    display_list: &strata_render::DisplayList,
    width: i32,
    height: i32,
    cursor_overlay: Option<(&McpCursorState, &CursorTheme)>,
) -> Result<CapturedSurfaceBuffer, String> {
    if width <= 0 || height <= 0 {
        return Err("Dimensions must be positive to render to PNG".to_string());
    }

    let mut fb = strata_render::FrameBuffer::new(width as usize, height as usize);
    fb.execute_display_list(display_list);

    if let Some((mcp_cursor, theme)) = cursor_overlay {
        if mcp_cursor.fade_opacity > 0.001 {
            let bitmap = theme.get(mcp_cursor.cursor_icon);
            let start_x = (mcp_cursor.current_x.round() as i32) - (bitmap.xhot as i32);
            let start_y = (mcp_cursor.current_y.round() as i32) - (bitmap.yhot as i32);

            for cy in 0..bitmap.height as i32 {
                let py = start_y + cy;
                if py < 0 || py >= height {
                    continue;
                }
                for cx in 0..bitmap.width as i32 {
                    let px = start_x + cx;
                    if px < 0 || px >= width {
                        continue;
                    }
                    let idx = ((cy as usize) * (bitmap.width as usize) + (cx as usize)) * 4;
                    let b = bitmap.pixels[idx] as f32 / 255.0;
                    let g = bitmap.pixels[idx + 1] as f32 / 255.0;
                    let r = bitmap.pixels[idx + 2] as f32 / 255.0;
                    let a = (bitmap.pixels[idx + 3] as f32 / 255.0) * mcp_cursor.fade_opacity;
                    if a > 0.001 {
                        fb.set_pixel(
                            px as usize,
                            py as usize,
                            strata_style::types::Color::from_f32(r, g, b, a),
                        );
                    }
                }
            }
        }
    }

    Ok(CapturedSurfaceBuffer {
        width: width as u32,
        height: height as u32,
        stride: width * 4,
        pixels: fb.pixels,
        is_cairo_argb32: false,
    })
}

/// Pure-Rust headless software rasterization and PNG byte export.
///
/// Completely decoupled from Cairo, GTK3, Pango, or X11/Wayland display servers.
/// Operates deterministically across Linux, macOS, Windows, and CI runners.
pub fn render_to_png_bytes_pure(
    display_list: &strata_render::DisplayList,
    width: i32,
    height: i32,
    cursor_overlay: Option<(&McpCursorState, &CursorTheme)>,
) -> Result<Vec<u8>, String> {
    render_to_captured_buffer_pure(display_list, width, height, cursor_overlay)?.encode_png_bytes()
}

#[cfg(not(all(target_os = "linux", feature = "gtk-compat")))]
pub fn render_to_captured_buffer(
    display_list: &strata_render::DisplayList,
    width: i32,
    height: i32,
    cursor_overlay: Option<(&McpCursorState, &CursorTheme)>,
) -> Result<CapturedSurfaceBuffer, String> {
    render_to_captured_buffer_pure(display_list, width, height, cursor_overlay)
}

#[cfg(not(all(target_os = "linux", feature = "gtk-compat")))]
pub fn render_to_png_bytes(
    display_list: &strata_render::DisplayList,
    width: i32,
    height: i32,
    cursor_overlay: Option<(&McpCursorState, &CursorTheme)>,
) -> Result<Vec<u8>, String> {
    render_to_png_bytes_pure(display_list, width, height, cursor_overlay)
}
#[cfg(not(all(target_os = "linux", feature = "gtk-compat")))]
pub fn init_pango_text_measurement() {
    // Graceful no-op fallback when built without GTK/Pango
}
#[cfg(not(all(target_os = "linux", feature = "gtk-compat")))]
pub fn gtk_begin_window_drag(_x: f32, _y: f32) {}
#[cfg(not(all(target_os = "linux", feature = "gtk-compat")))]
pub fn gtk_is_maximized() -> bool {
    false
}
#[cfg(not(all(target_os = "linux", feature = "gtk-compat")))]
pub fn gtk_take_scroll_pointer_pos() -> Option<(f32, f32)> {
    None
}
#[cfg(not(all(target_os = "linux", feature = "gtk-compat")))]
pub fn gtk_set_gpu_2d_enabled(_enabled: bool) {}
#[cfg(not(all(target_os = "linux", feature = "gtk-compat")))]
pub fn gtk_gpu_2d_enabled() -> bool {
    false
}
#[cfg(not(all(target_os = "linux", feature = "gtk-compat")))]
pub fn gtk_last_tile_cache_stats() -> (u32, u32) {
    (0, 0)
}
#[cfg(not(all(target_os = "linux", feature = "gtk-compat")))]
pub fn gtk_last_gpu_telemetry() -> PlatformGpuTelemetry {
    PlatformGpuTelemetry::default()
}
#[cfg(not(all(target_os = "linux", feature = "gtk-compat")))]
pub fn try_present_display_list_gpu_timed(
    _list: &strata_render::DisplayList,
    _width: f32,
    _height: f32,
    _overlay_dirty: bool,
    _transparent_bg: bool,
    _cursor_overlay: Option<(&McpCursorState, &CursorTheme)>,
) -> GpuPresentTimings {
    GpuPresentTimings::default()
}
#[cfg(not(all(target_os = "linux", feature = "gtk-compat")))]
pub fn gtk_present_active_gpu_surface_now_timed() -> GpuPresentTimings {
    GpuPresentTimings::default()
}
#[cfg(target_os = "android")]
pub use android_window::{AndroidEventLoop, AndroidWindow};
#[cfg(target_os = "linux")]
pub use dbus::{show_desktop_notification, DbusClient, DbusMessageDecoder, DbusMessageEncoder};
#[cfg(target_os = "ios")]
pub use ios_window::{IosEventLoop, IosWindow};
#[cfg(target_os = "macos")]
pub use macos_window::{MacOsEventLoop, MacOsWindow};
pub use menu::{AcceleratorModifier, Menu, MenuAccelerator, MenuBar, MenuItem, MenuItemRole};
#[cfg(all(target_os = "linux", feature = "gtk-compat"))]
pub use pango;
pub use shortcuts::{ShortcutCallback, ShortcutManager, ShortcutTrigger};
pub use single_instance::{decode_args, encode_args, SingleInstanceLock, SingleInstanceStatus};
pub use surface::{
    FramePacer, PresentError, PresentMode, RefreshRate, SoftwareSurfacePresenter, SurfaceConfig,
    SurfaceHandle, SurfacePresenter, SwapchainCompositor, SwapchainFormat,
};
#[cfg(target_os = "linux")]
pub use tray::DbusStatusNotifier;
pub use tray::{
    clear_delivered_notifications, get_delivered_notifications, DesktopNotification,
    NativeTrayHandle, TrayEvent, TrayEventChannel, TrayEventReceiver, TrayEventSink, TrayIcon,
    TrayMenu, TrayMenuItem,
};
pub use vibrancy::{apply_vibrancy, clear_vibrancy, VibrancyConfig, VibrancyEffect, VibrancyError};
#[cfg(target_os = "linux")]
pub use wayland_window::{WaylandDecoder, WaylandEncoder, WaylandEventLoop, WaylandWindow};
#[cfg(windows)]
pub use win32_window::{Win32EventLoop, Win32Window};
pub use window::{
    CursorIcon, EventLoop, EventLoopControl, HoverTooltipStyle, LivePlatformWindow, MockEventLoop,
    MockPlatformWindow, MonitorInfo, NativeSurfaceWindow, PlatformWindow, ThemeMode, WindowAnchor,
    WindowConfig, WindowDecoration, WindowMode,
};
pub use window_manager::{WindowId, WindowManager};
#[cfg(target_os = "linux")]
pub use x11_window::{
    PureX11Client, PureX11Window, X11EventLoop, X11ShmSegment, X11Window, X11WireDecoder,
    X11WireEncoder,
};

/// Target operating system or runtime platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlatformTarget {
    Android,
    Ios,
    MacOs,
    Windows,
    Linux,
    WebWasm,
    WebJs,
}

impl PlatformTarget {
    /// Detects current runtime platform target from compile-time flags.
    pub const fn current() -> Self {
        #[cfg(target_os = "android")]
        return PlatformTarget::Android;
        #[cfg(target_os = "ios")]
        return PlatformTarget::Ios;
        #[cfg(target_os = "macos")]
        return PlatformTarget::MacOs;
        #[cfg(target_os = "windows")]
        return PlatformTarget::Windows;
        #[cfg(target_os = "linux")]
        return PlatformTarget::Linux;
        #[cfg(all(target_arch = "wasm32", not(target_os = "unknown")))]
        return PlatformTarget::WebWasm;
        #[cfg(not(any(
            target_os = "android",
            target_os = "ios",
            target_os = "macos",
            target_os = "windows",
            target_os = "linux"
        )))]
        return PlatformTarget::WebJs;
    }

    /// Returns true if this target is a mobile operating system.
    pub const fn is_mobile(&self) -> bool {
        matches!(self, PlatformTarget::Android | PlatformTarget::Ios)
    }

    /// Returns true if this target is a desktop operating system.
    pub const fn is_desktop(&self) -> bool {
        matches!(
            self,
            PlatformTarget::MacOs | PlatformTarget::Windows | PlatformTarget::Linux
        )
    }

    /// Returns true if this target runs inside a web browser.
    pub const fn is_web(&self) -> bool {
        matches!(self, PlatformTarget::WebWasm | PlatformTarget::WebJs)
    }
}

/// Platform prelude module.
pub mod prelude {
    #[cfg(target_os = "android")]
    pub use super::android_window::{AndroidEventLoop, AndroidWindow};
    pub use super::csd::{ControlPlacement, CsdController, CsdGeometry, CsdHitZone};
    pub use super::events::{KeyCode, Modifiers, MouseButton, PlatformEvent, TouchPhase};
    #[cfg(target_os = "ios")]
    pub use super::ios_window::{IosEventLoop, IosWindow};
    #[cfg(target_os = "macos")]
    pub use super::macos_window::{MacOsEventLoop, MacOsWindow};
    pub use super::surface::{
        FramePacer, PresentMode, RefreshRate, SurfaceConfig, SurfaceHandle, SwapchainFormat,
    };
    pub use super::tray::{DesktopNotification, TrayIcon, TrayMenu, TrayMenuItem};
    #[cfg(target_os = "linux")]
    pub use super::wayland_window::{WaylandEventLoop, WaylandWindow};
    #[cfg(windows)]
    pub use super::win32_window::{Win32EventLoop, Win32Window};
    pub use super::window::{
        CursorIcon, EventLoop, EventLoopControl, HoverTooltipStyle, LivePlatformWindow,
        MockEventLoop, MockPlatformWindow, MonitorInfo, NativeSurfaceWindow, PlatformWindow,
        ThemeMode, WindowAnchor, WindowConfig, WindowDecoration, WindowMode,
    };
    #[cfg(target_os = "linux")]
    pub use super::x11_window::{PureX11Window, X11EventLoop, X11Window};
    pub use super::PlatformTarget;
}
