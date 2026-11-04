//! W3C CSS Floats & Exclusion Space engine (Blink LayoutNG parity).
//!
//! Tracks floating boxes in a Block Formatting Context (BFC) and solves
//! line opportunities for Inline Formatting Context (IFC) line boxes.

use crate::geometry::Rect;
use strata_style::types::Clear;

/// Directional orientation of a spatial layout exclusion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExclusionKind {
    FloatLeft,
    FloatRight,
}

/// An individual rectangular obstacle registered in the formatting context.
#[derive(Debug, Clone, PartialEq)]
pub struct ExclusionRect {
    pub bounds: Rect,
    pub kind: ExclusionKind,
}

/// Usable inline opportunity for a line box at a given vertical position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineOpportunity {
    /// Horizontal start offset relative to content container origin.
    pub start_x: f32,
    /// Width available for inline phrasing content without colliding with floats.
    pub available_width: f32,
    /// Absolute right edge of the leftmost float obstruction (or 0.0).
    pub left_float_edge: f32,
    /// Absolute left edge of the rightmost float obstruction (or container width).
    pub right_float_edge: f32,
}

/// Space manager tracking active float exclusions within a Block Formatting Context.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExclusionSpace {
    exclusions: Vec<ExclusionRect>,
}

impl ExclusionSpace {
    /// Constructs an empty exclusion space.
    pub fn new() -> Self {
        Self {
            exclusions: Vec::new(),
        }
    }

    /// Adds a float exclusion rectangle in container coordinates.
    pub fn add_exclusion(&mut self, bounds: Rect, kind: ExclusionKind) {
        self.exclusions.push(ExclusionRect { bounds, kind });
    }

    /// Returns whether any exclusions are active.
    pub fn is_empty(&self) -> bool {
        self.exclusions.is_empty()
    }

    /// Returns a slice of all active exclusions.
    pub fn exclusions(&self) -> &[ExclusionRect] {
        &self.exclusions
    }

    /// Computes the usable horizontal line opportunity for a line box spanning `[y, y + height]`.
    pub fn line_opportunity(&self, y: f32, height: f32, container_width: f32) -> LineOpportunity {
        if self.exclusions.is_empty() {
            return LineOpportunity {
                start_x: 0.0,
                available_width: container_width.max(0.0),
                left_float_edge: 0.0,
                right_float_edge: container_width,
            };
        }

        let line_top = y;
        let line_bottom = y + height.max(0.001);

        let mut left_edge: f32 = 0.0;
        let mut right_edge: f32 = container_width;

        for e in &self.exclusions {
            let float_top = e.bounds.y;
            let float_bottom = e.bounds.y + e.bounds.height;

            // Check if vertical intervals intersect
            if float_top < line_bottom && float_bottom > line_top {
                match e.kind {
                    ExclusionKind::FloatLeft => {
                        let r = e.bounds.x + e.bounds.width;
                        if r > left_edge {
                            left_edge = r;
                        }
                    }
                    ExclusionKind::FloatRight => {
                        let l = e.bounds.x;
                        if l < right_edge {
                            right_edge = l;
                        }
                    }
                }
            }
        }

        let avail = if right_edge > left_edge {
            right_edge - left_edge
        } else {
            0.0
        };

        LineOpportunity {
            start_x: left_edge,
            available_width: avail,
            left_float_edge: left_edge,
            right_float_edge: right_edge,
        }
    }

    /// Calculates clearance offset advancing past the bottom edge of relevant floats.
    pub fn clearance_offset(&self, clear: Clear, current_y: f32) -> f32 {
        if self.exclusions.is_empty() || clear == Clear::None {
            return current_y;
        }

        let mut cleared_y = current_y;

        for e in &self.exclusions {
            let matches = match clear {
                Clear::None => false,
                Clear::Left | Clear::InlineStart => e.kind == ExclusionKind::FloatLeft,
                Clear::Right | Clear::InlineEnd => e.kind == ExclusionKind::FloatRight,
                Clear::Both => true,
            };

            if matches {
                let bottom = e.bounds.y + e.bounds.height;
                if bottom > cleared_y {
                    cleared_y = bottom;
                }
            }
        }

        cleared_y
    }

    /// Returns the maximum bottom coordinate among all registered floats.
    pub fn max_float_bottom(&self) -> f32 {
        self.exclusions
            .iter()
            .map(|e| e.bounds.y + e.bounds.height)
            .fold(0.0, f32::max)
    }

    /// Finds the nearest bottom edge of an active float that occurs strictly after `y`.
    pub fn next_float_bottom_after(&self, y: f32) -> Option<f32> {
        let mut best: Option<f32> = None;
        for e in &self.exclusions {
            let bottom = e.bounds.y + e.bounds.height;
            if bottom > y + 0.01 {
                match best {
                    Some(b) => {
                        if bottom < b {
                            best = Some(bottom);
                        }
                    }
                    None => best = Some(bottom),
                }
            }
        }
        best
    }

    /// Translates all exclusion boundaries by a spatial delta `(dx, dy)`.
    pub fn translate(&mut self, dx: f32, dy: f32) {
        for e in &mut self.exclusions {
            e.bounds.x += dx;
            e.bounds.y += dy;
        }
    }
}
