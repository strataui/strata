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
