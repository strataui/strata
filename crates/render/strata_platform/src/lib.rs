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

#[cfg(all(target_os = "linux", feature = "gtk-compat"))]
pub use cairo;
pub use csd::{ControlPlacement, CsdController, CsdGeometry, CsdHitZone};
pub use cursor_theme::{CursorBitmap, CursorTheme};
#[cfg(any(test, feature = "test-internals"))]
pub use dialog::set_dialog_mock_response;
pub use dialog::{
    open_file, open_multiple_files, pick_folder, save_file, show_message_box, DialogFilter,
    DialogMockResponse, MessageBoxButtons, MessageBoxResult, MessageBoxType, OpenFileOptions,
    PickFolderOptions, SaveFileOptions,
};
pub use events::{KeyCode, Modifiers, MouseButton, PlatformEvent, TouchPhase};
#[cfg(all(target_os = "linux", feature = "gtk-compat"))]
pub use gtk;
#[cfg(all(target_os = "linux", feature = "gtk-compat"))]
pub use gtk_window::{
    cairo_rounded_rect, cairo_rounded_rect_radii, gtk_begin_window_drag, gtk_close,
    gtk_current_modifiers, gtk_focus_window, gtk_get_clipboard_text, gtk_is_closing,
    gtk_is_maximized, gtk_minimize, gtk_reset_ime, gtk_set_clipboard_text, gtk_set_cursor,
    gtk_set_ime_cursor_location, gtk_set_ime_surrounding_text, gtk_toggle_maximize,
    gtk_warp_cursor, init_pango_text_measurement, launch_gtk_window, launch_gtk_window_with_config,
    launch_gtk_window_with_scheduler, pango_font_metrics, pango_measure_styled_text,
    pango_measure_styled_text_family, pango_measure_text, pango_measure_text_family,
    post_platform_event, render_display_list_to_cairo, render_to_png, render_to_png_bytes,
    resolved_font_family, set_cursor_position, warp_cursor,
};
pub use mcp_cursor::McpCursorState;
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

    fb.to_png_bytes()
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
pub use tray::{
    clear_delivered_notifications, get_delivered_notifications, DbusStatusNotifier,
    DesktopNotification, NativeTrayHandle, TrayEvent, TrayEventChannel, TrayEventReceiver,
    TrayEventSink, TrayIcon, TrayMenu, TrayMenuItem,
};
pub use vibrancy::{apply_vibrancy, clear_vibrancy, VibrancyConfig, VibrancyEffect, VibrancyError};
#[cfg(target_os = "linux")]
pub use wayland_window::{WaylandDecoder, WaylandEncoder, WaylandEventLoop, WaylandWindow};
#[cfg(windows)]
pub use win32_window::{Win32EventLoop, Win32Window};
pub use window::{
    CursorIcon, EventLoop, EventLoopControl, LivePlatformWindow, MockEventLoop, MockPlatformWindow,
    MonitorInfo, NativeSurfaceWindow, PlatformWindow, ThemeMode, WindowAnchor, WindowConfig,
    WindowDecoration, WindowMode,
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
        CursorIcon, EventLoop, EventLoopControl, LivePlatformWindow, MockEventLoop,
        MockPlatformWindow, MonitorInfo, NativeSurfaceWindow, PlatformWindow, ThemeMode,
        WindowAnchor, WindowConfig, WindowDecoration, WindowMode,
    };
    #[cfg(target_os = "linux")]
    pub use super::x11_window::{PureX11Window, X11EventLoop, X11Window};
    pub use super::PlatformTarget;
}
