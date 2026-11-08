//! Headless Behavioral Primitives for Strata UI.
//!
//! Provides accessible, unstyled interaction state machines: Focus Traps, Roving TabIndex,
//! Floating Popover Positioning, and Outside-Click Dismissal Layers.

use strata_primitives::{Point, Rect, Size};

/// Focus Trap manager constraining Tab navigation within modal containers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusTrap {
    pub count: usize,
    pub active_index: usize,
    pub wrap_around: bool,
}

impl FocusTrap {
    pub fn new(count: usize) -> Self {
        Self {
            count,
            active_index: 0,
            wrap_around: true,
        }
    }

    /// Focus next element, wrapping to start if enabled.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> usize {
        if self.count == 0 {
            return 0;
        }
        if self.active_index + 1 < self.count {
            self.active_index += 1;
        } else if self.wrap_around {
            self.active_index = 0;
        }
        self.active_index
    }

    /// Focus previous element, wrapping to end if enabled.
    pub fn prev(&mut self) -> usize {
        if self.count == 0 {
            return 0;
        }
        if self.active_index > 0 {
            self.active_index -= 1;
        } else if self.wrap_around {
            self.active_index = self.count - 1;
        }
        self.active_index
    }

    pub fn set(&mut self, idx: usize) {
        if idx < self.count {
            self.active_index = idx;
        }
    }
}

/// Navigation orientation for roving tab indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Orientation {
    #[default]
    Horizontal,
    Vertical,
}

/// Roving TabIndex manager driving arrow-key navigation in menus, toolbars, and tab lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RovingIndex {
    pub count: usize,
    pub current: usize,
    pub orientation: Orientation,
    pub loop_around: bool,
}

impl RovingIndex {
    pub fn new(count: usize, orientation: Orientation) -> Self {
        Self {
            count,
            current: 0,
            orientation,
            loop_around: true,
        }
    }

    /// Process keyboard navigation event and return new selected index if changed.
    pub fn handle_key(&mut self, key: &str) -> Option<usize> {
        if self.count == 0 {
            return None;
        }
        let old = self.current;
        match (self.orientation, key) {
            (Orientation::Horizontal, "ArrowRight") | (Orientation::Vertical, "ArrowDown") => {
                if self.current + 1 < self.count {
                    self.current += 1;
                } else if self.loop_around {
                    self.current = 0;
                }
            }
            (Orientation::Horizontal, "ArrowLeft") | (Orientation::Vertical, "ArrowUp") => {
                if self.current > 0 {
                    self.current -= 1;
                } else if self.loop_around {
                    self.current = self.count - 1;
                }
            }
            (_, "Home") => {
                self.current = 0;
            }
            (_, "End") => {
                self.current = self.count - 1;
            }
            _ => return None,
        }

        if self.current != old {
            Some(self.current)
        } else {
            None
        }
    }
}

/// Placement orientation for anchored floating popovers and tooltips.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Placement {
    #[default]
    Bottom,
    BottomStart,
    BottomEnd,
    Top,
    TopStart,
    TopEnd,
    Left,
    LeftStart,
    LeftEnd,
    Right,
    RightStart,
    RightEnd,
}

/// Computes pixel position for an anchored floating box with boundary collision detection.
pub struct FloatingAnchor;

impl FloatingAnchor {
    pub fn compute_position(
        anchor: Rect,
        floating_size: Size,
        placement: Placement,
        offset: f32,
    ) -> Point {
        let (ax, ay, aw, ah) = (anchor.origin.x, anchor.origin.y, anchor.size.width, anchor.size.height);
        let (fw, fh) = (floating_size.width, floating_size.height);

        match placement {
            Placement::Bottom => Point::new(ax + (aw - fw) / 2.0, ay + ah + offset),
            Placement::BottomStart => Point::new(ax, ay + ah + offset),
            Placement::BottomEnd => Point::new(ax + aw - fw, ay + ah + offset),
            Placement::Top => Point::new(ax + (aw - fw) / 2.0, ay - fh - offset),
            Placement::TopStart => Point::new(ax, ay - fh - offset),
            Placement::TopEnd => Point::new(ax + aw - fw, ay - fh - offset),
            Placement::Left => Point::new(ax - fw - offset, ay + (ah - fh) / 2.0),
            Placement::LeftStart => Point::new(ax - fw - offset, ay),
            Placement::LeftEnd => Point::new(ax - fw - offset, ay + ah - fh),
            Placement::Right => Point::new(ax + aw + offset, ay + (ah - fh) / 2.0),
            Placement::RightStart => Point::new(ax + aw + offset, ay),
            Placement::RightEnd => Point::new(ax + aw + offset, ay + ah - fh),
        }
    }

    /// Adjust position to ensure floating box remains within viewport boundaries.
    pub fn flip_if_overflow(
        pos: Point,
        floating_size: Size,
        viewport: Size,
    ) -> Point {
        let mut x = pos.x;
        let mut y = pos.y;

        if x + floating_size.width > viewport.width {
            x = (viewport.width - floating_size.width).max(0.0);
        }
        if x < 0.0 {
            x = 0.0;
        }

        if y + floating_size.height > viewport.height {
            y = (viewport.height - floating_size.height).max(0.0);
        }
        if y < 0.0 {
            y = 0.0;
        }

        Point::new(x, y)
    }
}

/// Dismissable layer rules for dialogs, popovers, and drawers.
pub struct DismissableLayer;

impl DismissableLayer {
    /// Checks whether an Escape key press triggers dismissal.
    pub fn should_dismiss_on_escape(key: &str) -> bool {
        key == "Escape" || key == "Esc"
    }

    /// Checks whether a click at point is outside the layer bounds.
    pub fn should_dismiss_on_outside_click(click: Point, layer_bounds: Rect) -> bool {
        !layer_bounds.contains_point(click)
    }
}
