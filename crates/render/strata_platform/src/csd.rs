//! Client-Side Decorations (CSD): Titlebar geometry, control buttons, and resize border hit-testing.

use crate::window::CursorIcon;

/// The region of a Client-Side Decorated window under a given coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CsdHitZone {
    None,
    Titlebar,
    MinimizeButton,
    MaximizeButton,
    CloseButton,
    ResizeBorderTop,
    ResizeBorderBottom,
    ResizeBorderLeft,
    ResizeBorderRight,
    ResizeCornerTopLeft,
    ResizeCornerTopRight,
    ResizeCornerBottomLeft,
    ResizeCornerBottomRight,
}

impl CsdHitZone {
    /// Returns true if this hit zone represents an interactive window control button.
    pub fn is_button(&self) -> bool {
        matches!(
            self,
            CsdHitZone::MinimizeButton | CsdHitZone::MaximizeButton | CsdHitZone::CloseButton
        )
    }

    /// Returns true if this hit zone is the window titlebar.
    pub fn is_titlebar(&self) -> bool {
        matches!(self, CsdHitZone::Titlebar)
    }

    /// Returns true if this hit zone triggers window resizing.
    pub fn is_resize(&self) -> bool {
        matches!(
            self,
            CsdHitZone::ResizeBorderTop
                | CsdHitZone::ResizeBorderBottom
                | CsdHitZone::ResizeBorderLeft
                | CsdHitZone::ResizeBorderRight
                | CsdHitZone::ResizeCornerTopLeft
                | CsdHitZone::ResizeCornerTopRight
                | CsdHitZone::ResizeCornerBottomLeft
                | CsdHitZone::ResizeCornerBottomRight
        )
    }

    /// Converts the zone to an X11 `_NET_WM_MOVERESIZE` direction index.
    pub fn to_x11_moveresize_direction(&self) -> Option<u32> {
        match self {
            CsdHitZone::ResizeCornerTopLeft => Some(0),
            CsdHitZone::ResizeBorderTop => Some(1),
            CsdHitZone::ResizeCornerTopRight => Some(2),
            CsdHitZone::ResizeBorderRight => Some(3),
            CsdHitZone::ResizeCornerBottomRight => Some(4),
            CsdHitZone::ResizeBorderBottom => Some(5),
            CsdHitZone::ResizeCornerBottomLeft => Some(6),
            CsdHitZone::ResizeBorderLeft => Some(7),
            CsdHitZone::Titlebar => Some(8), // _NET_WM_MOVERESIZE_MOVE
            _ => None,
        }
    }

    /// Converts the zone to a Wayland `xdg_toplevel` resize edge bitmask.
    pub fn to_wayland_resize_edge(&self) -> Option<u32> {
        match self {
            CsdHitZone::ResizeBorderTop => Some(1),         // TOP
            CsdHitZone::ResizeBorderBottom => Some(2),      // BOTTOM
            CsdHitZone::ResizeBorderLeft => Some(4),        // LEFT
            CsdHitZone::ResizeCornerTopLeft => Some(5),     // TOP | LEFT
            CsdHitZone::ResizeCornerBottomLeft => Some(6),  // BOTTOM | LEFT
            CsdHitZone::ResizeBorderRight => Some(8),       // RIGHT
            CsdHitZone::ResizeCornerTopRight => Some(9),    // TOP | RIGHT
            CsdHitZone::ResizeCornerBottomRight => Some(10), // BOTTOM | RIGHT
            _ => None,
        }
    }

    /// Converts the zone to a Win32 `WM_NCHITTEST` result code.
    pub fn to_win32_nchittest(&self) -> isize {
        match self {
            CsdHitZone::None => 1,             // HTCLIENT
            CsdHitZone::Titlebar => 2,         // HTCAPTION
            CsdHitZone::MinimizeButton => 8,   // HTMINBUTTON
            CsdHitZone::MaximizeButton => 9,   // HTMAXBUTTON
            CsdHitZone::CloseButton => 20,     // HTCLOSE
            CsdHitZone::ResizeBorderLeft => 10,   // HTLEFT
            CsdHitZone::ResizeBorderRight => 11,  // HTRIGHT
            CsdHitZone::ResizeBorderTop => 12,    // HTTOP
            CsdHitZone::ResizeCornerTopLeft => 13, // HTTOPLEFT
            CsdHitZone::ResizeCornerTopRight => 14, // HTTOPRIGHT
            CsdHitZone::ResizeBorderBottom => 15,  // HTBOTTOM
            CsdHitZone::ResizeCornerBottomLeft => 16, // HTBOTTOMLEFT
            CsdHitZone::ResizeCornerBottomRight => 17, // HTBOTTOMRIGHT
        }
    }

    /// Determines the appropriate cursor icon for this hit zone.
    pub fn cursor_icon(&self) -> CursorIcon {
        match self {
            CsdHitZone::None => CursorIcon::Default,
            CsdHitZone::Titlebar => CursorIcon::Default,
            CsdHitZone::MinimizeButton | CsdHitZone::MaximizeButton | CsdHitZone::CloseButton => {
                CursorIcon::Pointer
            }
            CsdHitZone::ResizeBorderTop | CsdHitZone::ResizeBorderBottom => CursorIcon::ResizeN,
            CsdHitZone::ResizeBorderLeft | CsdHitZone::ResizeBorderRight => CursorIcon::ResizeE,
            CsdHitZone::ResizeCornerTopLeft | CsdHitZone::ResizeCornerBottomRight => {
                CursorIcon::ResizeNw
            }
            CsdHitZone::ResizeCornerTopRight | CsdHitZone::ResizeCornerBottomLeft => {
                CursorIcon::ResizeNe
            }
        }
    }
}

/// Placement orientation for titlebar control buttons (close, minimize, maximize).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ControlPlacement {
    #[default]
    Right, // Windows / Linux default
    Left, // macOS default
}

/// Geometry parameters defining CSD titlebar and resize borders.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CsdGeometry {
    pub titlebar_height: f32,
    pub button_width: f32,
    pub button_height: f32,
    pub resize_border_thickness: f32,
    pub placement: ControlPlacement,
}

impl Default for CsdGeometry {
    fn default() -> Self {
        Self {
            titlebar_height: 32.0,
            button_width: 46.0,
            button_height: 32.0,
            resize_border_thickness: 6.0,
            placement: ControlPlacement::Right,
        }
    }
}

impl CsdGeometry {
    /// Creates custom CSD geometry parameters.
    pub fn new(
        titlebar_height: f32,
        button_width: f32,
        button_height: f32,
        resize_border_thickness: f32,
        placement: ControlPlacement,
    ) -> Self {
        Self {
            titlebar_height,
            button_width,
            button_height,
            resize_border_thickness,
            placement,
        }
    }

    /// Creates a macOS-styled CSD geometry layout with left-aligned buttons.
    pub fn macos_style() -> Self {
        Self {
            titlebar_height: 28.0,
            button_width: 24.0,
            button_height: 28.0,
            resize_border_thickness: 5.0,
            placement: ControlPlacement::Left,
        }
    }

    /// Resolves the CsdHitZone for a coordinate `(x, y)` given the window dimensions.
    pub fn hit_test(&self, window_width: f32, window_height: f32, x: f32, y: f32) -> CsdHitZone {
        // Outside the window bounds
        if x < 0.0 || y < 0.0 || x > window_width || y > window_height {
            return CsdHitZone::None;
        }

        let t = self.resize_border_thickness;

        // 1. Resize Corners
        if x < t && y < t {
            return CsdHitZone::ResizeCornerTopLeft;
        }
        if x > window_width - t && y < t {
            return CsdHitZone::ResizeCornerTopRight;
        }
        if x < t && y > window_height - t {
            return CsdHitZone::ResizeCornerBottomLeft;
        }
        if x > window_width - t && y > window_height - t {
            return CsdHitZone::ResizeCornerBottomRight;
        }

        // 2. Resize Borders
        if y < t {
            return CsdHitZone::ResizeBorderTop;
        }
        if y > window_height - t {
            return CsdHitZone::ResizeBorderBottom;
        }
        if x < t {
            return CsdHitZone::ResizeBorderLeft;
        }
        if x > window_width - t {
            return CsdHitZone::ResizeBorderRight;
        }

        // 3. Titlebar & Control Buttons
        if y <= self.titlebar_height {
            match self.placement {
                ControlPlacement::Right => {
                    let bw = self.button_width;
                    if x >= window_width - bw {
                        return CsdHitZone::CloseButton;
                    }
                    if x >= window_width - 2.0 * bw {
                        return CsdHitZone::MaximizeButton;
                    }
                    if x >= window_width - 3.0 * bw {
                        return CsdHitZone::MinimizeButton;
                    }
                    CsdHitZone::Titlebar
                }
                ControlPlacement::Left => {
                    let bw = self.button_width;
                    if x <= bw {
                        return CsdHitZone::CloseButton;
                    }
                    if x <= 2.0 * bw {
                        return CsdHitZone::MinimizeButton;
                    }
                    if x <= 3.0 * bw {
                        return CsdHitZone::MaximizeButton;
                    }
                    CsdHitZone::Titlebar
                }
            }
        } else {
            CsdHitZone::None
        }
    }
}

/// High-level controller managing CSD geometry, hit-testing, and native drag tracking.
#[derive(Debug, Clone)]
pub struct CsdController {
    pub geometry: CsdGeometry,
    pub is_dragging: bool,
    pub active_zone: CsdHitZone,
    pub drag_start_pos: Option<(f32, f32)>,
}

impl Default for CsdController {
    fn default() -> Self {
        Self {
            geometry: CsdGeometry::default(),
            is_dragging: false,
            active_zone: CsdHitZone::None,
            drag_start_pos: None,
        }
    }
}

impl CsdController {
    /// Creates a new CsdController with the given geometry.
    pub fn new(geometry: CsdGeometry) -> Self {
        Self {
            geometry,
            is_dragging: false,
            active_zone: CsdHitZone::None,
            drag_start_pos: None,
        }
    }

    /// Performs hit-testing against the managed geometry.
    pub fn hit_test(&self, window_width: f32, window_height: f32, x: f32, y: f32) -> CsdHitZone {
        self.geometry.hit_test(window_width, window_height, x, y)
    }

    /// Processes a pointer down event and records drag start candidate coordinates.
    pub fn on_pointer_down(
        &mut self,
        window_width: f32,
        window_height: f32,
        x: f32,
        y: f32,
    ) -> CsdHitZone {
        let zone = self.hit_test(window_width, window_height, x, y);
        self.active_zone = zone;
        if zone == CsdHitZone::Titlebar || zone.is_resize() {
            self.drag_start_pos = Some((x, y));
        } else {
            self.drag_start_pos = None;
        }
        zone
    }

    /// Resets drag tracking state on pointer up.
    pub fn on_pointer_up(&mut self) {
        self.is_dragging = false;
        self.active_zone = CsdHitZone::None;
        self.drag_start_pos = None;
    }

    /// Returns true if the pointer has travelled past the minimum drag threshold.
    pub fn should_start_drag(&mut self, x: f32, y: f32) -> bool {
        if let Some((sx, sy)) = self.drag_start_pos {
            if (x - sx).abs() > 4.0 || (y - sy).abs() > 4.0 {
                self.is_dragging = true;
                return true;
            }
        }
        false
    }
}

