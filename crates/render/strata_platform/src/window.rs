//! Window configuration, platform window traits, and mock implementations.

use crate::events::PlatformEvent;
use crate::surface::{FramePacer, RefreshRate, SurfaceConfig, SurfaceHandle};
use std::collections::VecDeque;
use std::time::Instant;

/// Operating mode of the platform window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowMode {
    #[default]
    Windowed,
    BorderlessFullscreen,
    ExclusiveFullscreen,
    Minimized,
    Maximized,
}

/// Window frame decoration style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowDecoration {
    #[default]
    NativeOs,
    ClientSideDecorations,
    FramelessTransparent,
}

/// Operating system color scheme / theme preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

/// Cursor icon style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CursorIcon {
    #[default]
    Default,
    Pointer,
    Text,
    Crosshair,
    Move,
    NotAllowed,
    Wait,
    Grab,
    Grabbing,
    ResizeN,
    ResizeS,
    ResizeE,
    ResizeW,
    ResizeNe,
    ResizeNw,
    ResizeSe,
    ResizeSw,
    ColResize,
    RowResize,
    Help,
    None,
}

/// Configuration used when creating or reconfiguring a platform window.
#[derive(Debug, Clone)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub min_width: Option<u32>,
    pub min_height: Option<u32>,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    pub resizable: bool,
    pub mode: WindowMode,
    pub decoration: WindowDecoration,
    pub theme: ThemeMode,
    pub transparent: bool,
    pub always_on_top: bool,
    pub single_instance_id: Option<String>,
    pub vibrancy: crate::vibrancy::VibrancyEffect,
}

impl WindowConfig {
    /// Configures the window to enforce a single-instance mutex and focus existing instance on launch.
    pub fn with_single_instance(mut self, app_id: impl Into<String>) -> Self {
        self.single_instance_id = Some(app_id.into());
        self
    }

    /// Configures window backdrop blur and vibrancy (Mica, Acrylic, NSVisualEffectView).
    pub fn with_vibrancy(mut self, vibrancy: crate::vibrancy::VibrancyEffect) -> Self {
        self.vibrancy = vibrancy;
        self
    }
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "Strata Application".to_string(),
            width: 1280,
            height: 800,
            min_width: Some(320),
            min_height: Some(240),
            max_width: None,
            max_height: None,
            resizable: true,
            mode: WindowMode::Windowed,
            decoration: WindowDecoration::NativeOs,
            theme: ThemeMode::System,
            transparent: false,
            always_on_top: false,
            single_instance_id: None,
            vibrancy: crate::vibrancy::VibrancyEffect::None,
        }
    }
}

/// Physical display monitor descriptor.
#[derive(Debug, Clone, PartialEq)]
pub struct MonitorInfo {
    pub id: u64,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
    pub refresh_rate: u32,
    pub is_primary: bool,
}

impl MonitorInfo {
    /// Creates a new monitor descriptor.
    pub fn new(
        id: u64,
        name: impl Into<String>,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        scale_factor: f32,
        refresh_rate: u32,
        is_primary: bool,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            x,
            y,
            width,
            height,
            scale_factor,
            refresh_rate,
            is_primary,
        }
    }

    /// Default standard 1080p 1.0x display.
    pub fn standard_1080p() -> Self {
        Self::new(1, "Primary Display", 0, 0, 1920, 1080, 1.0, 60, true)
    }

    /// High-DPI 4K 2.0x display.
    pub fn hidpi_4k() -> Self {
        Self::new(2, "4K UHD Display", 1920, 0, 3840, 2160, 2.0, 120, false)
    }
}

/// Abstract platform window trait defining common cross-platform window operations.
pub trait PlatformWindow {
    fn title(&self) -> &str;
    fn set_title(&mut self, title: &str);
    fn size(&self) -> (u32, u32);
    fn set_size(&mut self, width: u32, height: u32);
    fn scale_factor(&self) -> f32;
    fn set_scale_factor(&mut self, scale_factor: f32);
    fn mode(&self) -> WindowMode;
    fn set_mode(&mut self, mode: WindowMode);
    fn is_focused(&self) -> bool;
    fn set_focused(&mut self, focused: bool);
    fn cursor(&self) -> CursorIcon;
    fn set_cursor(&mut self, cursor: CursorIcon);
    fn request_redraw(&mut self);
    fn is_redraw_requested(&self) -> bool;
    fn poll_events(&mut self) -> Vec<PlatformEvent>;
    fn push_event(&mut self, event: PlatformEvent);
    /// Returns the monitor containing the center of this window, if known.
    fn current_monitor(&self) -> Option<MonitorInfo> {
        None
    }
    /// Returns the list of currently connected physical display monitors.
    fn available_monitors(&self) -> Vec<MonitorInfo> {
        Vec::new()
    }
    /// Returns the primary system monitor.
    fn primary_monitor(&self) -> Option<MonitorInfo> {
        None
    }
    /// Transmits an incremental AccessKit/OS accessibility tree update to native screen readers.
    fn on_accessibility_update(&mut self, _json_payload: &str) {}
    /// Sets candidate window positioning following active text caret coordinate.
    fn set_ime_cursor_location(&mut self, _x: i32, _y: i32, _width: i32, _height: i32) {}
    /// Updates surrounding text buffer for context-aware IME prediction.
    fn set_ime_surrounding_text(&mut self, _text: &str, _cursor_index: i32) {}
    /// Resets active IME composition session.
    fn reset_ime(&mut self) {}
    /// Initiates an interactive window drag operation with the native window manager.
    fn begin_window_drag(&mut self, _x: f32, _y: f32) {}
    /// Initiates an interactive window resize operation with the native window manager.
    fn begin_window_resize(&mut self, _edge: crate::csd::CsdHitZone, _x: f32, _y: f32) {}
    /// Toggles the window maximized state and returns true if the window is now maximized.
    fn toggle_maximize(&mut self) -> bool {
        let current = self.mode();
        let new_mode = if current == WindowMode::Maximized {
            WindowMode::Windowed
        } else {
            WindowMode::Maximized
        };
        self.set_mode(new_mode);
        new_mode == WindowMode::Maximized
    }
}

/// Contract for platform windows supporting direct software/swapchain pixel blitting.
pub trait NativeSurfaceWindow: PlatformWindow {
    /// Blits a 32-bit BGRA uncompressed buffer to the window client area.
    fn present_surface(&mut self, pixels: &[u8], width: u32, height: u32) -> Result<(), crate::surface::PresentError>;
}

/// Headless mock platform window for deterministic testing, CI automation, and headless rendering.
#[derive(Debug, Clone)]
pub struct MockPlatformWindow {
    pub config: WindowConfig,
    pub scale_factor: f32,
    pub is_focused: bool,
    pub cursor: CursorIcon,
    pub redraw_requested: bool,
    pub event_queue: VecDeque<PlatformEvent>,
    pub last_accessibility_update: Option<String>,
    pub ime_cursor_location: Option<(i32, i32, i32, i32)>,
    pub ime_surrounding: Option<(String, i32)>,
    pub monitors: Vec<MonitorInfo>,
    pub current_monitor_id: Option<u64>,
}

impl MockPlatformWindow {
    /// Creates a new mock window with the specified configuration.
    pub fn new(config: WindowConfig) -> Self {
        Self {
            config,
            scale_factor: 1.0,
            is_focused: true,
            cursor: CursorIcon::Default,
            redraw_requested: false,
            event_queue: VecDeque::new(),
            last_accessibility_update: None,
            ime_cursor_location: None,
            ime_surrounding: None,
            monitors: vec![MonitorInfo::standard_1080p()],
            current_monitor_id: Some(1),
        }
    }


    /// Creates a mock window with default configuration.
    pub fn new_mock() -> Self {
        Self::new(WindowConfig::default())
    }

    /// Injects an event into the window's event queue.
    pub fn inject_event(&mut self, event: PlatformEvent) {
        self.push_event(event);
    }

    /// Consumes and clears the redraw request flag.
    pub fn take_redraw_request(&mut self) -> bool {
        let requested = self.redraw_requested;
        self.redraw_requested = false;
        requested
    }

    /// Adds a monitor to the mock topology.
    pub fn add_monitor(&mut self, monitor: MonitorInfo) {
        self.monitors.push(monitor);
    }

    /// Moves the window to another monitor, automatically updating high-DPI scaling.
    pub fn move_to_monitor(&mut self, monitor_id: u64) {
        if let Some(monitor) = self.monitors.iter().find(|m| m.id == monitor_id).cloned() {
            self.current_monitor_id = Some(monitor_id);
            if (self.scale_factor - monitor.scale_factor).abs() > 0.001 {
                self.set_scale_factor(monitor.scale_factor);
            }
        }
    }
}

impl PlatformWindow for MockPlatformWindow {
    fn title(&self) -> &str {
        &self.config.title
    }

    fn set_title(&mut self, title: &str) {
        self.config.title = title.to_string();
    }

    fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    fn set_size(&mut self, width: u32, height: u32) {
        self.config.width = width;
        self.config.height = height;
        self.push_event(PlatformEvent::WindowResized { width, height });
    }

    fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    fn set_scale_factor(&mut self, scale_factor: f32) {
        self.scale_factor = scale_factor;
        self.push_event(PlatformEvent::ScaleFactorChanged { scale_factor });
    }

    fn mode(&self) -> WindowMode {
        self.config.mode
    }

    fn set_mode(&mut self, mode: WindowMode) {
        let old = self.config.mode;
        self.config.mode = mode;
        if old != mode {
            self.push_event(PlatformEvent::WindowMaximized(mode == WindowMode::Maximized));
        }
    }

    fn is_focused(&self) -> bool {
        self.is_focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.is_focused = focused;
        self.push_event(PlatformEvent::WindowFocused(focused));
    }

    fn cursor(&self) -> CursorIcon {
        self.cursor
    }

    fn set_cursor(&mut self, cursor: CursorIcon) {
        self.cursor = cursor;
    }

    fn request_redraw(&mut self) {
        self.redraw_requested = true;
    }

    fn is_redraw_requested(&self) -> bool {
        self.redraw_requested
    }

    fn poll_events(&mut self) -> Vec<PlatformEvent> {
        self.event_queue.drain(..).collect()
    }

    fn push_event(&mut self, event: PlatformEvent) {
        self.event_queue.push_back(event);
    }

    fn current_monitor(&self) -> Option<MonitorInfo> {
        self.current_monitor_id
            .and_then(|id| self.monitors.iter().find(|m| m.id == id).cloned())
    }

    fn available_monitors(&self) -> Vec<MonitorInfo> {
        self.monitors.clone()
    }

    fn primary_monitor(&self) -> Option<MonitorInfo> {
        self.monitors.iter().find(|m| m.is_primary).cloned()
    }

    fn on_accessibility_update(&mut self, json_payload: &str) {
        self.last_accessibility_update = Some(json_payload.to_string());
    }

    fn set_ime_cursor_location(&mut self, x: i32, y: i32, width: i32, height: i32) {
        self.ime_cursor_location = Some((x, y, width, height));
    }

    fn set_ime_surrounding_text(&mut self, text: &str, cursor_index: i32) {
        self.ime_surrounding = Some((text.to_string(), cursor_index));
    }

    fn reset_ime(&mut self) {
        self.ime_cursor_location = None;
        self.ime_surrounding = None;
    }
}

impl NativeSurfaceWindow for MockPlatformWindow {
    fn present_surface(&mut self, _pixels: &[u8], _width: u32, _height: u32) -> Result<(), crate::surface::PresentError> {
        self.redraw_requested = false;
        Ok(())
    }
}


/// Live interactive platform window with surface configuration, frame pacing, and event queuing.
#[derive(Debug, Clone)]
pub struct LivePlatformWindow {
    pub config: WindowConfig,
    pub surface_config: SurfaceConfig,
    pub surface_handle: SurfaceHandle,
    pub frame_pacer: FramePacer,
    pub scale_factor: f32,
    pub is_focused: bool,
    pub cursor: CursorIcon,
    pub redraw_requested: bool,
    pub event_queue: VecDeque<PlatformEvent>,
    pub monitors: Vec<MonitorInfo>,
    pub current_monitor_id: Option<u64>,
}

impl LivePlatformWindow {
    /// Creates a live platform window initialized with the given configuration.
    pub fn new(config: WindowConfig) -> Self {
        let scale_factor = 1.0;
        let surface_config = SurfaceConfig {
            width: config.width,
            height: config.height,
            scale_factor,
            ..Default::default()
        };
        let surface_handle = SurfaceHandle::new_headless(config.width, config.height, scale_factor);
        let frame_pacer = FramePacer::new(RefreshRate::Hz60);
        Self {
            config,
            surface_config,
            surface_handle,
            frame_pacer,
            scale_factor,
            is_focused: true,
            cursor: CursorIcon::Default,
            redraw_requested: true,
            event_queue: VecDeque::new(),
            monitors: vec![MonitorInfo::standard_1080p()],
            current_monitor_id: Some(1),
        }
    }

    /// Access surface configuration descriptor.
    pub fn surface_config(&self) -> &SurfaceConfig {
        &self.surface_config
    }

    /// Mutable access to frame pacer.
    pub fn frame_pacer_mut(&mut self) -> &mut FramePacer {
        &mut self.frame_pacer
    }

    /// Returns abstract surface handle.
    pub fn surface_handle(&self) -> SurfaceHandle {
        self.surface_handle
    }

    /// Consumes and clears redraw request flag.
    pub fn take_redraw_request(&mut self) -> bool {
        let requested = self.redraw_requested;
        self.redraw_requested = false;
        requested
    }
}

impl PlatformWindow for LivePlatformWindow {
    fn title(&self) -> &str {
        &self.config.title
    }

    fn set_title(&mut self, title: &str) {
        self.config.title = title.to_string();
    }

    fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    fn set_size(&mut self, width: u32, height: u32) {
        self.config.width = width;
        self.config.height = height;
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface_handle.width = width;
        self.surface_handle.height = height;
        self.request_redraw();
        self.push_event(PlatformEvent::WindowResized { width, height });
    }

    fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    fn set_scale_factor(&mut self, scale_factor: f32) {
        self.scale_factor = scale_factor;
        self.surface_config.scale_factor = scale_factor;
        self.surface_handle.scale_factor_snapped = (scale_factor * 64.0) as u32;
        self.request_redraw();
        self.push_event(PlatformEvent::ScaleFactorChanged { scale_factor });
    }

    fn mode(&self) -> WindowMode {
        self.config.mode
    }

    fn set_mode(&mut self, mode: WindowMode) {
        self.config.mode = mode;
    }

    fn is_focused(&self) -> bool {
        self.is_focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.is_focused = focused;
        self.push_event(PlatformEvent::WindowFocused(focused));
    }

    fn cursor(&self) -> CursorIcon {
        self.cursor
    }

    fn set_cursor(&mut self, cursor: CursorIcon) {
        self.cursor = cursor;
    }

    fn request_redraw(&mut self) {
        self.redraw_requested = true;
    }

    fn is_redraw_requested(&self) -> bool {
        self.redraw_requested
    }

    fn poll_events(&mut self) -> Vec<PlatformEvent> {
        self.event_queue.drain(..).collect()
    }

    fn push_event(&mut self, event: PlatformEvent) {
        self.event_queue.push_back(event);
    }

    fn current_monitor(&self) -> Option<MonitorInfo> {
        self.current_monitor_id
            .and_then(|id| self.monitors.iter().find(|m| m.id == id).cloned())
    }

    fn available_monitors(&self) -> Vec<MonitorInfo> {
        self.monitors.clone()
    }

    fn primary_monitor(&self) -> Option<MonitorInfo> {
        self.monitors.iter().find(|m| m.is_primary).cloned()
    }

    fn set_ime_cursor_location(&mut self, x: i32, y: i32, width: i32, height: i32) {
        #[cfg(all(target_os = "linux", feature = "gtk-compat"))]
        crate::gtk_window::gtk_set_ime_cursor_location(x, y, width, height);
        #[cfg(not(all(target_os = "linux", feature = "gtk-compat")))]
        let _ = (x, y, width, height);
    }

    fn set_ime_surrounding_text(&mut self, text: &str, cursor_index: i32) {
        #[cfg(all(target_os = "linux", feature = "gtk-compat"))]
        crate::gtk_window::gtk_set_ime_surrounding_text(text, cursor_index);
        #[cfg(not(all(target_os = "linux", feature = "gtk-compat")))]
        let _ = (text, cursor_index);
    }

    fn reset_ime(&mut self) {
        #[cfg(all(target_os = "linux", feature = "gtk-compat"))]
        crate::gtk_window::gtk_reset_ime();
    }
}

impl NativeSurfaceWindow for LivePlatformWindow {
    fn present_surface(&mut self, _pixels: &[u8], _width: u32, _height: u32) -> Result<(), crate::surface::PresentError> {
        self.redraw_requested = false;
        Ok(())
    }
}

/// Control flow returned by event loop handlers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EventLoopControl {
    #[default]
    Poll,
    Wait,
    WaitUntil(Instant),
    Exit,
}

/// Abstract platform event loop trait.
pub trait EventLoop {
    fn run<F>(&mut self, handler: F)
    where
        F: FnMut(PlatformEvent, &mut EventLoopControl);
}

/// Headless mock event loop for simulated frame dispatching and testing.
#[derive(Debug, Default)]
pub struct MockEventLoop {
    pending_events: VecDeque<PlatformEvent>,
    control: EventLoopControl,
}

impl MockEventLoop {
    pub fn new() -> Self {
        Self {
            pending_events: VecDeque::new(),
            control: EventLoopControl::Poll,
        }
    }

    pub fn queue_event(&mut self, event: PlatformEvent) {
        self.pending_events.push_back(event);
    }

    pub fn step<F>(&mut self, mut handler: F) -> EventLoopControl
    where
        F: FnMut(PlatformEvent, &mut EventLoopControl),
    {
        while let Some(event) = self.pending_events.pop_front() {
            handler(event, &mut self.control);
            if self.control == EventLoopControl::Exit {
                return EventLoopControl::Exit;
            }
        }
        self.control
    }
}

impl EventLoop for MockEventLoop {
    fn run<F>(&mut self, mut handler: F)
    where
        F: FnMut(PlatformEvent, &mut EventLoopControl),
    {
        while self.control != EventLoopControl::Exit {
            if self.pending_events.is_empty() {
                break;
            }
            if self.step(&mut handler) == EventLoopControl::Exit {
                break;
            }
        }
    }
}
