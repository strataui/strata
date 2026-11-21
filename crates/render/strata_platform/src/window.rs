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

/// Monitor workarea placement and sticky docking anchor for a platform window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WindowAnchor {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
}

impl WindowAnchor {
    /// Returns true if this anchor pins ("sticks") the window to a monitor workarea corner.
    pub const fn is_sticky_dock(self) -> bool {
        matches!(
            self,
            Self::TopLeft | Self::TopRight | Self::BottomLeft | Self::BottomRight
        )
    }

    /// Computes target `(x, y)` coordinates within a monitor workarea for a window of `(win_w, win_h)`.
    pub const fn compute_position(
        self,
        workarea_x: i32,
        workarea_y: i32,
        workarea_w: i32,
        workarea_h: i32,
        win_w: i32,
        win_h: i32,
    ) -> (i32, i32) {
        match self {
            Self::TopLeft => (workarea_x, workarea_y),
            Self::TopRight => (workarea_x + workarea_w - win_w, workarea_y),
            Self::BottomLeft => (workarea_x, workarea_y + workarea_h - win_h),
            Self::BottomRight => (
                workarea_x + workarea_w - win_w,
                workarea_y + workarea_h - win_h,
            ),
            Self::Center => (
                workarea_x + (workarea_w - win_w) / 2,
                workarea_y + (workarea_h - win_h) / 2,
            ),
        }
    }
}

/// Visual styling and positioning configuration for native hover tooltips
/// (`title` and button/image `alt` attributes).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HoverTooltipStyle {
    /// Background fill color of the tooltip surface.
    pub background_color: strata_style::types::Color,
    /// Border stroke color of the tooltip surface.
    pub border_color: strata_style::types::Color,
    /// Foreground color of the tooltip text.
    pub text_color: strata_style::types::Color,
    /// Drop shadow color beneath the tooltip surface (alpha `0` disables the shadow).
    pub shadow_color: strata_style::types::Color,
    /// Font size in logical pixels.
    pub font_size: f32,
    /// Line-height multiplier relative to `font_size`.
    pub line_height: f32,
    /// Horizontal inner padding in logical pixels.
    pub padding_x: f32,
    /// Vertical inner padding in logical pixels.
    pub padding_y: f32,
    /// Corner border radius in logical pixels.
    pub corner_radius: f32,
    /// Border thickness in logical pixels (`0.0` disables the border).
    pub border_width: f32,
    /// Maximum text wrapping width in logical pixels.
    pub max_width: f32,
    /// Horizontal offset from the pointer anchor in logical pixels.
    pub cursor_offset_x: f32,
    /// Vertical offset from the pointer anchor in logical pixels.
    pub cursor_offset_y: f32,
}

impl Default for HoverTooltipStyle {
    fn default() -> Self {
        Self {
            background_color: strata_style::types::Color::new(18, 22, 29, 242),
            border_color: strata_style::types::Color::new(255, 255, 255, 34),
            text_color: strata_style::types::Color::new(241, 245, 249, 255),
            shadow_color: strata_style::types::Color::new(0, 0, 0, 110),
            font_size: 12.0,
            line_height: 1.35,
            padding_x: 9.0,
            padding_y: 5.0,
            corner_radius: 6.0,
            border_width: 1.0,
            max_width: 340.0,
            cursor_offset_x: 12.0,
            cursor_offset_y: 18.0,
        }
    }
}

impl HoverTooltipStyle {
    /// Overrides the tooltip background fill color.
    pub const fn with_background_color(mut self, color: strata_style::types::Color) -> Self {
        self.background_color = color;
        self
    }

    /// Overrides the tooltip border stroke color.
    pub const fn with_border_color(mut self, color: strata_style::types::Color) -> Self {
        self.border_color = color;
        self
    }

    /// Overrides the tooltip text foreground color.
    pub const fn with_text_color(mut self, color: strata_style::types::Color) -> Self {
        self.text_color = color;
        self
    }

    /// Overrides the tooltip drop-shadow color (use alpha `0` to disable shadow).
    pub const fn with_shadow_color(mut self, color: strata_style::types::Color) -> Self {
        self.shadow_color = color;
        self
    }

    /// Overrides the tooltip font size in logical pixels.
    pub const fn with_font_size(mut self, font_size: f32) -> Self {
        self.font_size = font_size;
        self
    }

    /// Overrides the horizontal and vertical inner padding in logical pixels.
    pub const fn with_padding(mut self, padding_x: f32, padding_y: f32) -> Self {
        self.padding_x = padding_x;
        self.padding_y = padding_y;
        self
    }

    /// Overrides the tooltip corner radius in logical pixels.
    pub const fn with_corner_radius(mut self, corner_radius: f32) -> Self {
        self.corner_radius = corner_radius;
        self
    }

    /// Overrides the tooltip border width in logical pixels (`0.0` disables border).
    pub const fn with_border_width(mut self, border_width: f32) -> Self {
        self.border_width = border_width;
        self
    }

    /// Overrides the maximum wrapping width in logical pixels.
    pub const fn with_max_width(mut self, max_width: f32) -> Self {
        self.max_width = max_width;
        self
    }

    /// Overrides the cursor anchor offset `(x, y)` in logical pixels.
    pub const fn with_cursor_offset(mut self, x: f32, y: f32) -> Self {
        self.cursor_offset_x = x;
        self.cursor_offset_y = y;
        self
    }
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
    pub anchor: Option<WindowAnchor>,
    pub dock_bottom_right: bool,
    pub single_instance_id: Option<String>,
    pub vibrancy: crate::vibrancy::VibrancyEffect,
    /// Swapchain presentation mode (`Immediate` / `Mailbox` for uncapped 3D viewports, `Fifo` default).
    pub present_mode: crate::surface::PresentMode,
    /// Pointer dwell delay before native hover tooltips (`title` / `alt`) become visible.
    pub tooltip_delay: std::time::Duration,
    /// Visual styling and offset configuration for native hover tooltips.
    pub tooltip_style: HoverTooltipStyle,
}

impl WindowConfig {
    /// Sets swapchain present mode (e.g. `PresentMode::Immediate` for uncapped framerate).
    pub fn with_present_mode(mut self, present_mode: crate::surface::PresentMode) -> Self {
        self.present_mode = present_mode;
        self
    }

    /// Configures the dwell delay before native hover tooltips become visible.
    pub fn with_tooltip_delay(mut self, delay: std::time::Duration) -> Self {
        self.tooltip_delay = delay;
        self
    }

    /// Configures the dwell delay in milliseconds before native hover tooltips become visible.
    pub fn with_tooltip_delay_ms(mut self, delay_ms: u64) -> Self {
        self.tooltip_delay = std::time::Duration::from_millis(delay_ms);
        self
    }

    /// Configures visual styling and offset for native hover tooltips.
    pub fn with_tooltip_style(mut self, style: HoverTooltipStyle) -> Self {
        self.tooltip_style = style;
        self
    }

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

    /// Configures monitor workarea placement or corner docking for this window.
    pub fn with_anchor(mut self, anchor: WindowAnchor) -> Self {
        self.anchor = Some(anchor);
        if anchor == WindowAnchor::BottomRight {
            self.dock_bottom_right = true;
        }
        self
    }

    /// Pins the window to the bottom-right corner of the primary monitor's workarea ("stuck" mode).
    pub fn with_dock_bottom_right(mut self, dock_bottom_right: bool) -> Self {
        self.dock_bottom_right = dock_bottom_right;
        if dock_bottom_right && self.anchor.is_none() {
            self.anchor = Some(WindowAnchor::BottomRight);
        } else if !dock_bottom_right && self.anchor == Some(WindowAnchor::BottomRight) {
            self.anchor = None;
        }
        self
    }

    /// Resolves the effective window anchor, honoring both `anchor` and `dock_bottom_right`.
    pub const fn effective_anchor(&self) -> Option<WindowAnchor> {
        if let Some(anchor) = self.anchor {
            Some(anchor)
        } else if self.dock_bottom_right {
            Some(WindowAnchor::BottomRight)
        } else {
            None
        }
    }

    /// Returns true when the window is docked ("stuck") to a monitor workarea corner.
    pub const fn is_docked(&self) -> bool {
        match self.effective_anchor() {
            Some(anchor) => anchor.is_sticky_dock(),
            None => false,
        }
    }

    /// Computes target `(x, y)` coordinates in the given monitor workarea if an anchor is configured.
    pub const fn compute_anchored_position(
        &self,
        workarea_x: i32,
        workarea_y: i32,
        workarea_w: i32,
        workarea_h: i32,
    ) -> Option<(i32, i32)> {
        match self.effective_anchor() {
            Some(anchor) => Some(anchor.compute_position(
                workarea_x,
                workarea_y,
                workarea_w,
                workarea_h,
                self.width as i32,
                self.height as i32,
            )),
            None => None,
        }
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
            anchor: None,
            dock_bottom_right: false,
            single_instance_id: None,
            vibrancy: crate::vibrancy::VibrancyEffect::None,
            present_mode: crate::surface::PresentMode::Fifo,
            tooltip_delay: std::time::Duration::from_millis(450),
            tooltip_style: HoverTooltipStyle::default(),
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
    fn present_surface(
        &mut self,
        pixels: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(), crate::surface::PresentError>;
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
            self.push_event(PlatformEvent::WindowMaximized(
                mode == WindowMode::Maximized,
            ));
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
    fn present_surface(
        &mut self,
        _pixels: &[u8],
        _width: u32,
        _height: u32,
    ) -> Result<(), crate::surface::PresentError> {
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
    pub display_buffer: Vec<u8>,
    pub is_native_presented: bool,
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
        let surface_handle = {
            #[cfg(target_os = "linux")]
            {
                if std::env::var_os("WAYLAND_DISPLAY").is_some() {
                    SurfaceHandle {
                        window_id: 1001,
                        raw_window_ptr: 0x574C4E44,  // "WLND"
                        raw_display_ptr: 0x574C4453, // "WLDS"
                        width: config.width,
                        height: config.height,
                        scale_factor_snapped: (scale_factor * 64.0) as u32,
                    }
                } else if std::env::var_os("DISPLAY").is_some() {
                    SurfaceHandle {
                        window_id: 1002,
                        raw_window_ptr: 0x58313157,  // "X11W"
                        raw_display_ptr: 0x58313144, // "X11D"
                        width: config.width,
                        height: config.height,
                        scale_factor_snapped: (scale_factor * 64.0) as u32,
                    }
                } else {
                    SurfaceHandle::new_headless(config.width, config.height, scale_factor)
                }
            }
            #[cfg(target_os = "windows")]
            {
                SurfaceHandle {
                    window_id: 2001,
                    raw_window_ptr: 0x57494E32,  // "WIN2"
                    raw_display_ptr: 0x47444931, // "GDI1"
                    width: config.width,
                    height: config.height,
                    scale_factor_snapped: (scale_factor * 64.0) as u32,
                }
            }
            #[cfg(target_os = "macos")]
            {
                SurfaceHandle {
                    window_id: 3001,
                    raw_window_ptr: 0x434F4341,  // "COCA"
                    raw_display_ptr: 0x4150504C, // "APPL"
                    width: config.width,
                    height: config.height,
                    scale_factor_snapped: (scale_factor * 64.0) as u32,
                }
            }
            #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
            {
                SurfaceHandle::new_headless(config.width, config.height, scale_factor)
            }
        };
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
            display_buffer: Vec::new(),
            is_native_presented: false,
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
    fn present_surface(
        &mut self,
        pixels: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(), crate::surface::PresentError> {
        self.redraw_requested = false;
        let expected_bytes = (width as usize) * (height as usize) * 4;
        if pixels.len() < expected_bytes {
            return Err(crate::surface::PresentError::BufferTooSmall);
        }
        self.display_buffer.resize(expected_bytes, 0);
        self.display_buffer
            .copy_from_slice(&pixels[..expected_bytes]);
        self.is_native_presented = true;
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
