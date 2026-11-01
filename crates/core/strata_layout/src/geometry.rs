//! Geometry structures for CSS Box Model and subpixel layout snapping.

use strata_primitives::geom::{EdgeInsets, Rect as PrimRect};
use strata_primitives::LayoutUnit;

/// 2D Rectangle in floating point pixels.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    #[inline(always)]
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    #[inline(always)]
    pub const fn from_xywh(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    #[inline(always)]
    pub const fn zero() -> Self {
        Self { x: 0.0, y: 0.0, width: 0.0, height: 0.0 }
    }

    #[inline(always)]
    pub fn min_x(self) -> f32 { self.x }
    #[inline(always)]
    pub fn max_x(self) -> f32 { self.x + self.width }
    #[inline(always)]
    pub fn min_y(self) -> f32 { self.y }
    #[inline(always)]
    pub fn max_y(self) -> f32 { self.y + self.height }

    /// Computes intersection with another rectangle if overlapping.
    #[inline]
    pub fn intersect(self, other: Self) -> Option<Self> {
        let x0 = self.min_x().max(other.min_x());
        let y0 = self.min_y().max(other.min_y());
        let x1 = self.max_x().min(other.max_x());
        let y1 = self.max_y().min(other.max_y());

        if x1 > x0 && y1 > y0 {
            Some(Self::new(x0, y0, x1 - x0, y1 - y0))
        } else {
            None
        }
    }

    /// Chromium subpixel pixel snapping via `LayoutUnit` (1/64th px fixed point).
    pub fn snap_to_pixel(self) -> Self {
        Self {
            x: LayoutUnit::from_f32(self.x).to_f32(),
            y: LayoutUnit::from_f32(self.y).to_f32(),
            width: LayoutUnit::from_f32(self.width).to_f32(),
            height: LayoutUnit::from_f32(self.height).to_f32(),
        }
    }
}

impl From<PrimRect<f32>> for Rect {
    fn from(r: PrimRect<f32>) -> Self {
        Self {
            x: r.origin.x,
            y: r.origin.y,
            width: r.size.width,
            height: r.size.height,
        }
    }
}

impl From<Rect> for PrimRect<f32> {
    fn from(r: Rect) -> Self {
        PrimRect::from_xywh(r.x, r.y, r.width, r.height)
    }
}

/// The 4 concentric boxes forming the CSS Box Model for an element.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LayoutGeometry {
    /// Content box (where inner text and child elements reside).
    pub content_box: Rect,
    /// Padding box (content box expanded by padding insets).
    pub padding_box: Rect,
    /// Border box (padding box expanded by border widths).
    /// This is the primary rectangle used for element rendering and layout bounds.
    pub border_box: Rect,
    /// Margin box (border box expanded by margin insets).
    pub margin_box: Rect,
}

impl LayoutGeometry {
    /// Computes concentric boxes from a given border_box and padding/border/margin insets.
    pub fn from_border_box(
        border_box: Rect,
        padding: EdgeInsets<f32>,
        border: EdgeInsets<f32>,
        margin: EdgeInsets<f32>,
    ) -> Self {
        let content_box = Rect::new(
            border_box.x + border.left + padding.left,
            border_box.y + border.top + padding.top,
            (border_box.width - border.left - border.right - padding.left - padding.right).max(0.0),
            (border_box.height - border.top - border.bottom - padding.top - padding.bottom).max(0.0),
        );

        let padding_box = Rect::new(
            border_box.x + border.left,
            border_box.y + border.top,
            (border_box.width - border.left - border.right).max(0.0),
            (border_box.height - border.top - border.bottom).max(0.0),
        );

        let margin_box = Rect::new(
            border_box.x - margin.left,
            border_box.y - margin.top,
            border_box.width + margin.left + margin.right,
            border_box.height + margin.top + margin.bottom,
        );

        Self {
            content_box,
            padding_box,
            border_box,
            margin_box,
        }
    }

    /// Translates all concentric rectangles by (dx, dy).
    pub fn translate(&mut self, dx: f32, dy: f32) {
        self.content_box.x += dx;
        self.content_box.y += dy;
        self.padding_box.x += dx;
        self.padding_box.y += dy;
        self.border_box.x += dx;
        self.border_box.y += dy;
        self.margin_box.x += dx;
        self.margin_box.y += dy;
    }

    /// Snaps all concentric rectangles to Chromium subpixel coordinates.
    pub fn snap_to_pixel(&mut self) {
        self.content_box = self.content_box.snap_to_pixel();
        self.padding_box = self.padding_box.snap_to_pixel();
        self.border_box = self.border_box.snap_to_pixel();
        self.margin_box = self.margin_box.snap_to_pixel();
    }
}
