//! Strata Platform: Unified Native Hardware Abstraction Layer for Android, iOS, macOS, Windows, Linux, and Web.
//!
//! Provides zero-dependency platform windowing abstractions, client-side decorations (CSD),
//! desktop system tray integrations, and headless mock drivers for deterministic CI testing.

#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::field_reassign_with_default,
    clippy::chunks_exact_to_as_chunks
)]

pub mod csd;
pub mod cursor_theme;
pub mod dialog;
pub mod events;
#[cfg(target_os = "linux")]
pub mod gtk_window;
pub mod mcp_cursor;
pub mod menu;
pub mod raw_abi;
pub mod shortcuts;
pub mod single_instance;
pub mod surface;
pub mod tray;
pub mod vibrancy;
pub mod window;
pub mod window_manager;
#[cfg(target_os = "linux")]
pub mod x11_window;
#[cfg(target_os = "linux")]
pub mod wayland_window;
#[cfg(windows)]
pub mod win32_window;
#[cfg(target_os = "macos")]
pub mod macos_window;
#[cfg(target_os = "android")]
pub mod android_window;
#[cfg(target_os = "ios")]
pub mod ios_window;

#[cfg(target_os = "linux")]
pub use cairo;
pub use csd::{ControlPlacement, CsdGeometry, CsdHitZone};
pub use cursor_theme::{CursorBitmap, CursorTheme};
pub use dialog::{
    open_file, open_multiple_files, pick_folder, save_file, set_dialog_mock_response,
    show_message_box, DialogFilter, DialogMockResponse, MessageBoxButtons, MessageBoxResult,
    MessageBoxType, OpenFileOptions, PickFolderOptions, SaveFileOptions,
};
pub use events::{KeyCode, Modifiers, MouseButton, PlatformEvent, TouchPhase};
pub use mcp_cursor::McpCursorState;
#[cfg(target_os = "linux")]
pub use gtk;
#[cfg(target_os = "linux")]
pub use gtk_window::{
    cairo_rounded_rect, gtk_begin_window_drag, gtk_close, gtk_current_modifiers, gtk_focus_window,
    gtk_get_clipboard_text, gtk_is_closing, gtk_is_maximized, gtk_minimize, gtk_set_clipboard_text,
    gtk_set_cursor, gtk_toggle_maximize, init_pango_text_measurement, launch_gtk_window,
    pango_font_metrics, pango_measure_styled_text, pango_measure_styled_text_family,
    pango_measure_text, pango_measure_text_family, post_platform_event,
    render_display_list_to_cairo, render_to_png, render_to_png_bytes, resolved_font_family,
};
#[cfg(not(target_os = "linux"))]
pub fn render_to_png_bytes(
    _display_list: &strata_render::DisplayList,
    _width: i32,
    _height: i32,
    _cursor_overlay: Option<(&McpCursorState, &CursorTheme)>,
) -> Result<Vec<u8>, String> {
    Err("Headless PNG rendering currently requires Linux cairo backend".to_string())
}
pub use menu::{AcceleratorModifier, Menu, MenuAccelerator, MenuBar, MenuItem, MenuItemRole};
#[cfg(target_os = "linux")]
pub use pango;
pub use shortcuts::{ShortcutCallback, ShortcutManager, ShortcutTrigger};
pub use single_instance::{SingleInstanceLock, SingleInstanceStatus};
pub use surface::{
    FramePacer, PresentError, PresentMode, RefreshRate, SoftwareSurfacePresenter, SurfaceConfig,
    SurfaceHandle, SurfacePresenter, SwapchainCompositor, SwapchainFormat,
};
pub use tray::{NativeTrayHandle, TrayEventSink, TrayIcon, TrayMenu, TrayMenuItem};
pub use vibrancy::{
    apply_vibrancy, clear_vibrancy, VibrancyConfig, VibrancyEffect, VibrancyError,
};
pub use window::{
    CursorIcon, EventLoop, EventLoopControl, LivePlatformWindow, MockEventLoop, MockPlatformWindow,
    PlatformWindow, ThemeMode, WindowConfig, WindowDecoration, WindowMode,
};
pub use window_manager::{WindowId, WindowManager};
#[cfg(target_os = "linux")]
pub use x11_window::{X11EventLoop, X11Window};
#[cfg(target_os = "linux")]
pub use wayland_window::{WaylandDecoder, WaylandEncoder, WaylandEventLoop, WaylandWindow};
#[cfg(windows)]
pub use win32_window::{Win32EventLoop, Win32Window};
#[cfg(target_os = "macos")]
pub use macos_window::{MacOsEventLoop, MacOsWindow};
#[cfg(target_os = "android")]
pub use android_window::{AndroidEventLoop, AndroidWindow};
#[cfg(target_os = "ios")]
pub use ios_window::{IosEventLoop, IosWindow};

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
    pub use super::csd::{ControlPlacement, CsdGeometry, CsdHitZone};
    pub use super::events::{KeyCode, Modifiers, MouseButton, PlatformEvent, TouchPhase};
    pub use super::surface::{
        FramePacer, PresentMode, RefreshRate, SurfaceConfig, SurfaceHandle, SwapchainFormat,
    };
    pub use super::tray::{TrayIcon, TrayMenu, TrayMenuItem};
    pub use super::window::{
        CursorIcon, EventLoop, EventLoopControl, LivePlatformWindow, MockEventLoop,
        MockPlatformWindow, PlatformWindow, ThemeMode, WindowConfig, WindowDecoration, WindowMode,
    };
    #[cfg(target_os = "linux")]
    pub use super::x11_window::{X11EventLoop, X11Window};
    #[cfg(target_os = "linux")]
    pub use super::wayland_window::{WaylandEventLoop, WaylandWindow};
    #[cfg(windows)]
    pub use super::win32_window::{Win32EventLoop, Win32Window};
    #[cfg(target_os = "macos")]
    pub use super::macos_window::{MacOsEventLoop, MacOsWindow};
    #[cfg(target_os = "android")]
    pub use super::android_window::{AndroidEventLoop, AndroidWindow};
    #[cfg(target_os = "ios")]
    pub use super::ios_window::{IosEventLoop, IosWindow};
    pub use super::PlatformTarget;
}
