//! Zero-dependency 2D geometry, subpixel snapping, and affine transformations.

use std::ops::{Add, Sub, Mul, Div, Neg};

/// 2D Cartesian Coordinate Point.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point<T = f32> {
    pub x: T,
    pub y: T,
}

impl<T> Point<T> {
    #[inline(always)]
    pub const fn new(x: T, y: T) -> Self {
        Self { x, y }
    }
}

impl Point<f32> {
    pub const ZERO: Self = Self::new(0.0, 0.0);

    #[inline(always)]
    pub fn distance_to(self, other: Self) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }

    #[inline(always)]
    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y
    }

    #[inline(always)]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self {
            x: self.x + (other.x - self.x) * t,
            y: self.y + (other.y - self.y) * t,
        }
    }
}

impl<T: Add<Output = T>> Add for Point<T> {
    type Output = Self;
    #[inline(always)]
    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl<T: Sub<Output = T>> Sub for Point<T> {
    type Output = Self;
    #[inline(always)]
    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl<T: Mul<Output = T> + Copy> Mul<T> for Point<T> {
    type Output = Self;
    #[inline(always)]
    fn mul(self, rhs: T) -> Self::Output {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

impl<T: Div<Output = T> + Copy> Div<T> for Point<T> {
    type Output = Self;
    #[inline(always)]
    fn div(self, rhs: T) -> Self::Output {
        Self::new(self.x / rhs, self.y / rhs)
    }
}

impl<T: Neg<Output = T>> Neg for Point<T> {
    type Output = Self;
    #[inline(always)]
    fn neg(self) -> Self::Output {
        Self::new(-self.x, -self.y)
    }
}

/// 2D Size Dimensions.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size<T = f32> {
    pub width: T,
    pub height: T,
}

impl<T> Size<T> {
    #[inline(always)]
    pub const fn new(width: T, height: T) -> Self {
        Self { width, height }
    }
}

impl Size<f32> {
    pub const ZERO: Self = Self::new(0.0, 0.0);

    #[inline(always)]
    pub fn is_empty(self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }

    #[inline(always)]
    pub fn aspect_ratio(self) -> f32 {
        if self.height == 0.0 { 0.0 } else { self.width / self.height }
    }

    #[inline(always)]
    pub fn clamp(self, min: Self, max: Self) -> Self {
        Self {
            width: self.width.clamp(min.width, max.width),
            height: self.height.clamp(min.height, max.height),
        }
    }
}

/// Axis-Aligned Bounding Box (AABB) Rectangle.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect<T = f32> {
    pub origin: Point<T>,
    pub size: Size<T>,
}

impl<T> Rect<T> {
    #[inline(always)]
    pub const fn new(origin: Point<T>, size: Size<T>) -> Self {
        Self { origin, size }
    }
}

impl Rect<f32> {
    pub const ZERO: Self = Self::new(Point::ZERO, Size::ZERO);

    #[inline(always)]
    pub const fn from_xywh(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            origin: Point::new(x, y),
            size: Size::new(width, height),
        }
    }

    #[inline(always)]
    pub fn min_x(self) -> f32 { self.origin.x }
    #[inline(always)]
    pub fn max_x(self) -> f32 { self.origin.x + self.size.width }
    #[inline(always)]
    pub fn min_y(self) -> f32 { self.origin.y }
    #[inline(always)]
    pub fn max_y(self) -> f32 { self.origin.y + self.size.height }

    #[inline(always)]
    pub fn contains_point(self, p: Point<f32>) -> bool {
        p.x >= self.min_x() && p.x <= self.max_x() && p.y >= self.min_y() && p.y <= self.max_y()
    }

    #[inline(always)]
    pub fn intersects(self, other: Self) -> bool {
        self.min_x() < other.max_x() && self.max_x() > other.min_x() &&
        self.min_y() < other.max_y() && self.max_y() > other.min_y()
    }

    /// Computes intersection rect with another rectangle.
    pub fn intersection(self, other: Self) -> Option<Self> {
        let x0 = self.min_x().max(other.min_x());
        let y0 = self.min_y().max(other.min_y());
        let x1 = self.max_x().min(other.max_x());
        let y1 = self.max_y().min(other.max_y());

        if x1 > x0 && y1 > y0 {
            Some(Self::from_xywh(x0, y0, x1 - x0, y1 - y0))
        } else {
            None
        }
    }

    /// Computes minimal bounding rect containing both rectangles.
    pub fn union(self, other: Self) -> Self {
        if self.size.is_empty() { return other; }
        if other.size.is_empty() { return self; }

        let x0 = self.min_x().min(other.min_x());
        let y0 = self.min_y().min(other.min_y());
        let x1 = self.max_x().max(other.max_x());
        let y1 = self.max_y().max(other.max_y());

        Self::from_xywh(x0, y0, x1 - x0, y1 - y0)
    }

    /// Deflates rectangle by insets (e.g. for padding/borders).
    #[inline(always)]
    pub fn deflate(self, insets: EdgeInsets<f32>) -> Self {
        Self::from_xywh(
            self.origin.x + insets.left,
            self.origin.y + insets.top,
            (self.size.width - insets.left - insets.right).max(0.0),
            (self.size.height - insets.top - insets.bottom).max(0.0),
        )
    }

    /// Inflates rectangle by insets (e.g. for margins/hit-testing).
    #[inline(always)]
    pub fn inflate(self, insets: EdgeInsets<f32>) -> Self {
        Self::from_xywh(
            self.origin.x - insets.left,
            self.origin.y - insets.top,
            self.size.width + insets.left + insets.right,
            self.size.height + insets.top + insets.bottom,
        )
    }
}

/// 4-Directional Insets (Padding, Margin, Border widths, Safe Areas).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EdgeInsets<T = f32> {
    pub top: T,
    pub right: T,
    pub bottom: T,
    pub left: T,
}

impl<T: Copy> EdgeInsets<T> {
    #[inline(always)]
    pub const fn new(top: T, right: T, bottom: T, left: T) -> Self {
        Self { top, right, bottom, left }
    }

    #[inline(always)]
    pub const fn all(value: T) -> Self {
        Self { top: value, right: value, bottom: value, left: value }
    }

    #[inline(always)]
    pub const fn symmetric(vertical: T, horizontal: T) -> Self {
        Self { top: vertical, right: horizontal, bottom: vertical, left: horizontal }
    }
}

impl EdgeInsets<f32> {
    pub const ZERO: Self = Self::all(0.0);

    #[inline(always)]
    pub fn horizontal(self) -> f32 { self.left + self.right }
    #[inline(always)]
    pub fn vertical(self) -> f32 { self.top + self.bottom }
}

/// 2x3 Affine Transformation Matrix.
///
/// | a  c  tx |
/// | b  d  ty |
/// | 0  0  1  |
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform2D {
    pub a: f32,  // scale x / cos
    pub b: f32,  // shear y / sin
    pub c: f32,  // shear x / -sin
    pub d: f32,  // scale y / cos
    pub tx: f32, // translate x
    pub ty: f32, // translate y
}

impl Default for Transform2D {
    #[inline(always)]
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform2D {
    pub const IDENTITY: Self = Self {
        a: 1.0, b: 0.0,
        c: 0.0, d: 1.0,
        tx: 0.0, ty: 0.0,
    };

    #[inline(always)]
    pub const fn translation(tx: f32, ty: f32) -> Self {
        Self {
            a: 1.0, b: 0.0,
            c: 0.0, d: 1.0,
            tx, ty,
        }
    }

    #[inline(always)]
    pub const fn scale(sx: f32, sy: f32) -> Self {
        Self {
            a: sx, b: 0.0,
            c: 0.0, d: sy,
            tx: 0.0, ty: 0.0,
        }
    }

    #[inline(always)]
    pub fn rotation(radians: f32) -> Self {
        let (sin, cos) = radians.sin_cos();
        Self {
            a: cos, b: sin,
            c: -sin, d: cos,
            tx: 0.0, ty: 0.0,
        }
    }

    #[inline(always)]
    pub fn transform_point(self, p: Point<f32>) -> Point<f32> {
        Point::new(
            self.a * p.x + self.c * p.y + self.tx,
            self.b * p.x + self.d * p.y + self.ty,
        )
    }

    /// Concatenates this transform with another (`self * other`).
    pub fn multiply(self, rhs: Self) -> Self {
        Self {
            a: self.a * rhs.a + self.c * rhs.b,
            b: self.b * rhs.a + self.d * rhs.b,
            c: self.a * rhs.c + self.c * rhs.d,
            d: self.b * rhs.c + self.d * rhs.d,
            tx: self.a * rhs.tx + self.c * rhs.ty + self.tx,
            ty: self.b * rhs.tx + self.d * rhs.ty + self.ty,
        }
    }

    /// Computes determinant of transformation.
    #[inline(always)]
    pub fn determinant(self) -> f32 {
        self.a * self.d - self.b * self.c
    }

    /// Inverts the affine transform if non-singular.
    pub fn inverse(self) -> Option<Self> {
        let det = self.determinant();
        if det.abs() < 1e-6 {
            return None;
        }
        let inv_det = 1.0 / det;
        Some(Self {
            a: self.d * inv_det,
            b: -self.b * inv_det,
            c: -self.c * inv_det,
            d: self.a * inv_det,
            tx: (self.c * self.ty - self.d * self.tx) * inv_det,
            ty: (self.b * self.tx - self.a * self.ty) * inv_det,
        })
    }
}

/// Fixed-point 1/64th pixel scalar coordinate (Chromium Blink LayoutUnit parity).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct LayoutUnit(pub i32);

impl LayoutUnit {
    pub const SCALE: f32 = 64.0;
    pub const ZERO: Self = Self(0);

    #[inline(always)]
    pub fn from_f32(val: f32) -> Self {
        Self((val * Self::SCALE).round() as i32)
    }

    #[inline(always)]
    pub fn to_f32(self) -> f32 {
        self.0 as f32 / Self::SCALE
    }

    /// Snaps coordinate to physical device grid based on Device Pixel Ratio (DPR).
    #[inline(always)]
    pub fn snap_to_device_grid(self, dpr: f32) -> f32 {
        let logical = self.to_f32();
        (logical * dpr).round() / dpr
    }

    /// Chromium border snapping formula: guarantees at least 1 physical pixel width.
    #[inline(always)]
    pub fn snap_border(self, dpr: f32) -> f32 {
        let physical = (self.to_f32() * dpr).round();
        if physical < 1.0 && self.0 > 0 {
            1.0 / dpr
        } else {
            physical / dpr
        }
    }
}

impl Add for LayoutUnit {
    type Output = Self;
    #[inline(always)]
    fn add(self, rhs: Self) -> Self::Output { Self(self.0 + rhs.0) }
}

impl Sub for LayoutUnit {
    type Output = Self;
    #[inline(always)]
    fn sub(self, rhs: Self) -> Self::Output { Self(self.0 - rhs.0) }
}

impl Mul<i32> for LayoutUnit {
    type Output = Self;
    #[inline(always)]
    fn mul(self, rhs: i32) -> Self::Output { Self(self.0 * rhs) }
}

impl Div<i32> for LayoutUnit {
    type Output = Self;
    #[inline(always)]
    fn div(self, rhs: i32) -> Self::Output { Self(self.0 / rhs) }
}

/// Cache-aligned 1/64th fixed-point integer layout rectangle.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct FixedRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl FixedRect {
    pub const ZERO: Self = Self { x: 0, y: 0, width: 0, height: 0 };

    #[inline(always)]
    pub fn from_f32(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x: (x * LayoutUnit::SCALE).round() as i32,
            y: (y * LayoutUnit::SCALE).round() as i32,
            width: (width * LayoutUnit::SCALE).round() as i32,
            height: (height * LayoutUnit::SCALE).round() as i32,
        }
    }

    #[inline(always)]
    pub fn to_rect_f32(self) -> Rect<f32> {
        Rect::from_xywh(
            self.x as f32 / LayoutUnit::SCALE,
            self.y as f32 / LayoutUnit::SCALE,
            self.width as f32 / LayoutUnit::SCALE,
            self.height as f32 / LayoutUnit::SCALE,
        )
    }

    #[inline(always)]
    pub fn snap_to_grid(self, dpr: f32) -> Rect<f32> {
        let r = self.to_rect_f32();
        Rect::from_xywh(
            (r.origin.x * dpr).round() / dpr,
            (r.origin.y * dpr).round() / dpr,
            (r.size.width * dpr).round() / dpr,
            (r.size.height * dpr).round() / dpr,
        )
    }
}
