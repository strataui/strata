//! Geometry structures for CSS Box Model and subpixel layout snapping.

use strata_primitives::geom::{
    EdgeInsets, Point as PrimPoint, Rect as PrimRect, Size as PrimSize,
};
use strata_primitives::LayoutUnit;

/// 2D Cartesian Coordinate Point.
pub type Point = PrimPoint<f32>;

/// 2D Size Dimensions.
pub type Size = PrimSize<f32>;

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
        Self {
            x,
            y,
            width,
            height,
        }
    }

    #[inline(always)]
    pub const fn from_xywh(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    #[inline(always)]
    pub const fn zero() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        }
    }

    #[inline(always)]
    pub fn min_x(self) -> f32 {
        self.x
    }
    #[inline(always)]
    pub fn max_x(self) -> f32 {
        self.x + self.width
    }
    #[inline(always)]
    pub fn min_y(self) -> f32 {
        self.y
    }
    #[inline(always)]
    pub fn max_y(self) -> f32 {
        self.y + self.height
    }

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

    /// Returns true if this rectangle contains the given point (px, py).
    #[inline]
    pub fn contains_point(self, px: f32, py: f32) -> bool {
        px >= self.min_x() && px <= self.max_x() && py >= self.min_y() && py <= self.max_y()
    }

    /// Returns true if this rectangle completely encloses `other`.
    #[inline]
    pub fn contains_rect(self, other: Self) -> bool {
        if other.width <= 0.0 || other.height <= 0.0 {
            return false;
        }
        other.min_x() >= self.min_x()
            && other.max_x() <= self.max_x()
            && other.min_y() >= self.min_y()
            && other.max_y() <= self.max_y()
    }

    /// Chromium subpixel layout boundary snapping via `LayoutUnit` (1/64th px fixed point).
    ///
    /// Snaps bounding edges against global screen space, deriving width and height
    /// as `snapped_max - snapped_min`. This guarantees adjacent sibling boxes and
    /// deeply nested descendants snap seamlessly with zero hairline cracks or rounding drift.
    pub fn snap_to_pixel(self) -> Self {
        let x = if self.x.is_finite() { self.x.clamp(-1e7, 1e7) } else { 0.0 };
        let y = if self.y.is_finite() { self.y.clamp(-1e7, 1e7) } else { 0.0 };
        let w = if self.width.is_finite() { self.width.clamp(0.0, 1e7) } else { 0.0 };
        let h = if self.height.is_finite() { self.height.clamp(0.0, 1e7) } else { 0.0 };

        let snapped_x = LayoutUnit::from_f32(x).to_f32();
        let snapped_y = LayoutUnit::from_f32(y).to_f32();
        let snapped_max_x = LayoutUnit::from_f32(x + w).to_f32();
        let snapped_max_y = LayoutUnit::from_f32(y + h).to_f32();

        Self {
            x: snapped_x,
            y: snapped_y,
            width: (snapped_max_x - snapped_x).max(0.0),
            height: (snapped_max_y - snapped_y).max(0.0),
        }
    }

    /// Snaps bounding edges against a device scale factor (e.g. 1.0 for 1x, 2.0 for Retina).
    pub fn snap_to_scale(self, scale: f32) -> Self {
        let s = if scale > 0.0 && scale.is_finite() { scale } else { 1.0 };
        let x = if self.x.is_finite() { self.x.clamp(-1e7, 1e7) } else { 0.0 };
        let y = if self.y.is_finite() { self.y.clamp(-1e7, 1e7) } else { 0.0 };
        let w = if self.width.is_finite() { self.width.clamp(0.0, 1e7) } else { 0.0 };
        let h = if self.height.is_finite() { self.height.clamp(0.0, 1e7) } else { 0.0 };

        let snapped_x = (x * s).round() / s;
        let snapped_y = (y * s).round() / s;
        let snapped_max_x = ((x + w) * s).round() / s;
        let snapped_max_y = ((y + h) * s).round() / s;

        Self {
            x: snapped_x,
            y: snapped_y,
            width: (snapped_max_x - snapped_x).max(0.0),
            height: (snapped_max_y - snapped_y).max(0.0),
        }
    }

    /// Returns true if all components of the rectangle are finite numbers.
    #[inline(always)]
    pub fn is_finite(&self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.width.is_finite() && self.height.is_finite()
    }

    /// Sanitizes all coordinates ensuring they are finite and clamped to valid layout limits.
    pub fn sanitize(&mut self) {
        if !self.x.is_finite() {
            self.x = 0.0;
        } else {
            self.x = self.x.clamp(-1e7, 1e7);
        }
        if !self.y.is_finite() {
            self.y = 0.0;
        } else {
            self.y = self.y.clamp(-1e7, 1e7);
        }
        if !self.width.is_finite() {
            self.width = 0.0;
        } else {
            self.width = self.width.clamp(0.0, 1e7);
        }
        if !self.height.is_finite() {
            self.height = 0.0;
        } else {
            self.height = self.height.clamp(0.0, 1e7);
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

/// The concentric boxes and scroll metrics forming the CSS Box Model for an element.
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
    /// Total scrollable content extent (width, height) enclosing all overflowing children.
    pub scroll_bounds: Size,
    /// Current scroll offset (scroll_x, scroll_y).
    pub scroll_offset: Point,
    /// Whether element has scrollable overflow along (horizontal, vertical) axes.
    pub has_scroll: (bool, bool),
}

impl LayoutGeometry {
    /// Returns true if all concentric boxes contain strictly finite coordinates.
    #[inline(always)]
    pub fn is_finite(&self) -> bool {
        self.content_box.is_finite()
            && self.padding_box.is_finite()
            && self.border_box.is_finite()
            && self.margin_box.is_finite()
    }

    /// Sanitizes all concentric rectangles to strictly finite values.
    pub fn sanitize(&mut self) {
        self.content_box.sanitize();
        self.padding_box.sanitize();
        self.border_box.sanitize();
        self.margin_box.sanitize();
    }

    /// Returns the maximum allowable scroll offset `(max_x, max_y)`.
    #[inline(always)]
    pub fn max_scroll(&self) -> Point {
        Point::new(
            (self.scroll_bounds.width - self.padding_box.width).max(0.0),
            (self.scroll_bounds.height - self.padding_box.height).max(0.0),
        )
    }

    /// Returns true if this element is an active scroll container.
    #[inline(always)]
    pub fn is_scroll_container(&self) -> bool {
        self.has_scroll.0 || self.has_scroll.1
    }

    /// Clamps a candidate scroll offset to valid bounds `[0, max_scroll]`.
    #[inline(always)]
    pub fn clamp_scroll_offset(&self, offset: Point) -> Point {
        let max = self.max_scroll();
        Point::new(offset.x.clamp(0.0, max.x), offset.y.clamp(0.0, max.y))
    }

    /// Computes concentric boxes from a given border_box and padding/border/margin insets.
    pub fn from_border_box(
        border_box: Rect,
        padding: EdgeInsets<f32>,
        border: EdgeInsets<f32>,
        margin: EdgeInsets<f32>,
    ) -> Self {
        let sanitize_edge = |e: EdgeInsets<f32>| -> EdgeInsets<f32> {
            EdgeInsets {
                top: if e.top.is_finite() { e.top.clamp(-1e7, 1e7) } else { 0.0 },
                right: if e.right.is_finite() { e.right.clamp(-1e7, 1e7) } else { 0.0 },
                bottom: if e.bottom.is_finite() { e.bottom.clamp(-1e7, 1e7) } else { 0.0 },
                left: if e.left.is_finite() { e.left.clamp(-1e7, 1e7) } else { 0.0 },
            }
        };

        let padding = sanitize_edge(padding);
        let border = sanitize_edge(border);
        let margin = sanitize_edge(margin);

        let bx = if border_box.x.is_finite() { border_box.x.clamp(-1e7, 1e7) } else { 0.0 };
        let by = if border_box.y.is_finite() { border_box.y.clamp(-1e7, 1e7) } else { 0.0 };
        let bw = if border_box.width.is_finite() { border_box.width.clamp(0.0, 1e7) } else { 0.0 };
        let bh = if border_box.height.is_finite() { border_box.height.clamp(0.0, 1e7) } else { 0.0 };
        let border_box = Rect::new(bx, by, bw, bh);

        let content_box = Rect::new(
            border_box.x + border.left + padding.left,
            border_box.y + border.top + padding.top,
            (border_box.width - border.left - border.right - padding.left - padding.right)
                .clamp(0.0, 1e7),
            (border_box.height - border.top - border.bottom - padding.top - padding.bottom)
                .clamp(0.0, 1e7),
        );

        let padding_box = Rect::new(
            border_box.x + border.left,
            border_box.y + border.top,
            (border_box.width - border.left - border.right).clamp(0.0, 1e7),
            (border_box.height - border.top - border.bottom).clamp(0.0, 1e7),
        );

        let margin_box = Rect::new(
            border_box.x - margin.left,
            border_box.y - margin.top,
            (border_box.width + margin.left + margin.right).clamp(0.0, 1e7),
            (border_box.height + margin.top + margin.bottom).clamp(0.0, 1e7),
        );

        let scroll_bounds = Size::new(content_box.width, content_box.height);
        let scroll_offset = Point::ZERO;
        let has_scroll = (false, false);

        Self {
            content_box,
            padding_box,
            border_box,
            margin_box,
            scroll_bounds,
            scroll_offset,
            has_scroll,
        }
    }

    /// Computes concentric boxes while preserving the specified scroll offset.
    pub fn from_border_box_with_scroll(
        border_box: Rect,
        padding: EdgeInsets<f32>,
        border: EdgeInsets<f32>,
        margin: EdgeInsets<f32>,
        scroll_offset: Point,
    ) -> Self {
        let mut geom = Self::from_border_box(border_box, padding, border, margin);
        geom.scroll_offset = scroll_offset;
        geom
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
        let _prof = crate::prof::Scope::new(crate::prof::Slot::Geometry);
        self.content_box = self.content_box.snap_to_pixel();
        self.padding_box = self.padding_box.snap_to_pixel();
        self.border_box = self.border_box.snap_to_pixel();
        self.margin_box = self.margin_box.snap_to_pixel();
    }

    /// Snaps all concentric rectangles against a device scale factor.
    pub fn snap_to_scale(&mut self, scale: f32) {
        let _prof = crate::prof::Scope::new(crate::prof::Slot::Geometry);
        self.content_box = self.content_box.snap_to_scale(scale);
        self.padding_box = self.padding_box.snap_to_scale(scale);
        self.border_box = self.border_box.snap_to_scale(scale);
        self.margin_box = self.margin_box.snap_to_scale(scale);
    }
}
