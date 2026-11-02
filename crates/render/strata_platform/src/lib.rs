//! Strata Platform: Unified Native Hardware Abstraction Layer for Android, iOS, macOS, Windows, Linux, and Web.
//!
//! Provides zero-dependency platform windowing abstractions, client-side decorations (CSD),
//! desktop system tray integrations, and headless mock drivers for deterministic CI testing.

pub mod csd;
pub mod events;
pub mod raw_abi;
pub mod surface;
pub mod menu;
pub mod tray;
pub mod window;
pub mod window_manager;
#[cfg(target_os = "linux")]
pub mod x11_window;
#[cfg(target_os = "linux")]
pub mod gtk_window;

pub use csd::{ControlPlacement, CsdGeometry, CsdHitZone};
pub use events::{KeyCode, Modifiers, MouseButton, PlatformEvent, TouchPhase};
pub use menu::{AcceleratorModifier, Menu, MenuAccelerator, MenuBar, MenuItem, MenuItemRole};
pub use surface::{
    FramePacer, PresentError, PresentMode, RefreshRate, SoftwareSurfacePresenter, SurfaceConfig,
    SurfaceHandle, SurfacePresenter, SwapchainCompositor, SwapchainFormat,
};
pub use tray::{TrayIcon, TrayMenu, TrayMenuItem};
pub use window::{
    CursorIcon, EventLoop, EventLoopControl, LivePlatformWindow, MockEventLoop, MockPlatformWindow,
    PlatformWindow, ThemeMode, WindowConfig, WindowDecoration, WindowMode,
};
#[cfg(target_os = "linux")]
pub use x11_window::{X11EventLoop, X11Window};
#[cfg(target_os = "linux")]
pub use gtk_window::{cairo_rounded_rect, launch_gtk_window, render_display_list_to_cairo, render_to_png};
#[cfg(target_os = "linux")]
pub use cairo;
pub use window_manager::{WindowId, WindowManager};

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
    pub use super::tray::{TrayIcon, TrayMenu, TrayMenuItem};
    pub use super::surface::{FramePacer, PresentMode, RefreshRate, SurfaceConfig, SurfaceHandle, SwapchainFormat};
    pub use super::window::{
        CursorIcon, EventLoop, EventLoopControl, LivePlatformWindow, MockEventLoop, MockPlatformWindow,
        PlatformWindow, ThemeMode, WindowConfig, WindowDecoration, WindowMode,
    };
    #[cfg(target_os = "linux")]
    pub use super::x11_window::{X11EventLoop, X11Window};
    pub use super::PlatformTarget;
}
