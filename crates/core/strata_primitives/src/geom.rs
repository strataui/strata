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
    pub fn x(self) -> f32 { self.origin.x }
    #[inline(always)]
    pub fn y(self) -> f32 { self.origin.y }
    #[inline(always)]
    pub fn width(self) -> f32 { self.size.width }
    #[inline(always)]
    pub fn height(self) -> f32 { self.size.height }

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

impl<T> EdgeInsets<T> {
    #[inline(always)]
    pub const fn new(top: T, right: T, bottom: T, left: T) -> Self {
        Self { top, right, bottom, left }
    }
}

impl<T: Clone> EdgeInsets<T> {
    #[inline(always)]
    pub fn all(value: T) -> Self {
        Self {
            top: value.clone(),
            right: value.clone(),
            bottom: value.clone(),
            left: value,
        }
    }

    #[inline(always)]
    pub fn symmetric(vertical: T, horizontal: T) -> Self {
        Self {
            top: vertical.clone(),
            right: horizontal.clone(),
            bottom: vertical,
            left: horizontal,
        }
    }
}

impl EdgeInsets<f32> {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0, 0.0);

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

    /// Creates a 2D affine transform from standard W3C 6-parameter coefficients:
    /// `a` (scale x), `b` (shear y), `c` (shear x), `d` (scale y), `e` (translate x), `f` (translate y).
    #[inline(always)]
    pub const fn from_w3c(a: f32, b: f32, c: f32, d: f32, e: f32, f: f32) -> Self {
        Self { a, b, c, d, tx: e, ty: f }
    }

    #[inline(always)]
    pub fn transform_xy(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.tx,
            self.b * x + self.d * y + self.ty,
        )
    }

    #[inline(always)]
    pub fn translate(&mut self, dx: f32, dy: f32) {
        *self = self.multiply(Self::translation(dx, dy));
    }

    #[inline(always)]
    pub fn scale_mut(&mut self, sx: f32, sy: f32) {
        *self = self.multiply(Self::scale(sx, sy));
    }

    #[inline(always)]
    pub fn rotate_mut(&mut self, radians: f32) {
        *self = self.multiply(Self::rotation(radians));
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

/// 4x4 homogeneous transformation matrix stored in column-major order.
///
/// Matrix indices:
/// [ 0  4  8 12 ]
/// [ 1  5  9 13 ]
/// [ 2  6 10 14 ]
/// [ 3  7 11 15 ]
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matrix4x4 {
    pub m: [f32; 16],
}

impl Default for Matrix4x4 {
    #[inline(always)]
    fn default() -> Self {
        Self::identity()
    }
}

impl Matrix4x4 {
    pub const IDENTITY: Self = Self {
        m: [
            1.0, 0.0, 0.0, 0.0, // col 0
            0.0, 1.0, 0.0, 0.0, // col 1
            0.0, 0.0, 1.0, 0.0, // col 2
            0.0, 0.0, 0.0, 1.0, // col 3
        ],
    };

    #[inline(always)]
    pub const fn identity() -> Self {
        Self::IDENTITY
    }

    #[inline(always)]
    pub const fn from_array(m: [f32; 16]) -> Self {
        Self { m }
    }

    /// Constructs a 4x4 matrix from 2D affine matrix components:
    /// `matrix(a, b, c, d, e, f)` where `a, c, e` are row 0, `b, d, f` are row 1.
    #[inline(always)]
    pub const fn from_2d(a: f32, b: f32, c: f32, d: f32, e: f32, f: f32) -> Self {
        Self {
            m: [
                a, b, 0.0, 0.0,
                c, d, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                e, f, 0.0, 1.0,
            ],
        }
    }

    /// Creates a 3D translation matrix.
    #[inline(always)]
    pub fn translation(tx: f32, ty: f32, tz: f32) -> Self {
        Self {
            m: [
                1.0, 0.0, 0.0, 0.0,
                0.0, 1.0, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                tx, ty, tz, 1.0,
            ],
        }
    }

    /// Creates a 3D scaling matrix.
    #[inline(always)]
    pub fn scaling(sx: f32, sy: f32, sz: f32) -> Self {
        Self {
            m: [
                sx, 0.0, 0.0, 0.0,
                0.0, sy, 0.0, 0.0,
                0.0, 0.0, sz, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ],
        }
    }

    /// Creates a rotation matrix around the Z axis (standard 2D rotation).
    #[inline(always)]
    pub fn rotation_z(rad: f32) -> Self {
        let (sin, cos) = rad.sin_cos();
        Self {
            m: [
                cos, sin, 0.0, 0.0,
                -sin, cos, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ],
        }
    }

    /// Creates a rotation matrix around the X axis.
    #[inline(always)]
    pub fn rotation_x(rad: f32) -> Self {
        let (sin, cos) = rad.sin_cos();
        Self {
            m: [
                1.0, 0.0, 0.0, 0.0,
                0.0, cos, sin, 0.0,
                0.0, -sin, cos, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ],
        }
    }

    /// Creates a rotation matrix around the Y axis.
    #[inline(always)]
    pub fn rotation_y(rad: f32) -> Self {
        let (sin, cos) = rad.sin_cos();
        Self {
            m: [
                cos, 0.0, -sin, 0.0,
                0.0, 1.0, 0.0, 0.0,
                sin, 0.0, cos, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ],
        }
    }

    /// Creates a rotation matrix around an arbitrary 3D axis `[x, y, z]`.
    pub fn rotation_axis(x: f32, y: f32, z: f32, rad: f32) -> Self {
        let len = (x * x + y * y + z * z).sqrt();
        if len < 1e-6 {
            return Self::identity();
        }
        let (x, y, z) = (x / len, y / len, z / len);
        let (sin, cos) = rad.sin_cos();
        let one_minus_cos = 1.0 - cos;

        Self {
            m: [
                cos + x * x * one_minus_cos,
                y * x * one_minus_cos + z * sin,
                z * x * one_minus_cos - y * sin,
                0.0,

                x * y * one_minus_cos - z * sin,
                cos + y * y * one_minus_cos,
                z * y * one_minus_cos + x * sin,
                0.0,

                x * z * one_minus_cos + y * sin,
                y * z * one_minus_cos - x * sin,
                cos + z * z * one_minus_cos,
                0.0,

                0.0, 0.0, 0.0, 1.0,
            ],
        }
    }

    /// Creates a 2D skew matrix given horizontal and vertical angles in radians.
    #[inline(always)]
    pub fn skew(ax_rad: f32, ay_rad: f32) -> Self {
        Self {
            m: [
                1.0, ay_rad.tan(), 0.0, 0.0,
                ax_rad.tan(), 1.0, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ],
        }
    }

    /// Creates a perspective projection matrix where `d` is the distance to the projection plane.
    #[inline(always)]
    pub fn perspective(d: f32) -> Self {
        let p = if d > 0.0 { -1.0 / d } else { 0.0 };
        Self {
            m: [
                1.0, 0.0, 0.0, 0.0,
                0.0, 1.0, 0.0, 0.0,
                0.0, 0.0, 1.0, p,
                0.0, 0.0, 0.0, 1.0,
            ],
        }
    }

    /// Multiplies `self` by `rhs`: `Result = self * rhs`.
    pub fn multiply(&self, rhs: &Self) -> Self {
        let a = &self.m;
        let b = &rhs.m;
        let mut r = [0.0f32; 16];

        for col in 0..4 {
            for row in 0..4 {
                r[col * 4 + row] =
                    a[row] * b[col * 4]
                    + a[4 + row] * b[col * 4 + 1]
                    + a[8 + row] * b[col * 4 + 2]
                    + a[12 + row] * b[col * 4 + 3];
            }
        }

        Self { m: r }
    }

    /// Computes the determinant of the 4x4 matrix using Laplace expansion.
    pub fn determinant(&self) -> f32 {
        let m = &self.m;
        let s0 = m[0] * m[5] - m[1] * m[4];
        let s1 = m[0] * m[6] - m[2] * m[4];
        let s2 = m[0] * m[7] - m[3] * m[4];
        let s3 = m[1] * m[6] - m[2] * m[5];
        let s4 = m[1] * m[7] - m[3] * m[5];
        let s5 = m[2] * m[7] - m[3] * m[6];

        let c5 = m[10] * m[15] - m[11] * m[14];
        let c4 = m[9] * m[15] - m[11] * m[13];
        let c3 = m[9] * m[14] - m[10] * m[13];
        let c2 = m[8] * m[15] - m[11] * m[12];
        let c1 = m[8] * m[14] - m[10] * m[12];
        let c0 = m[8] * m[13] - m[9] * m[12];

        s0 * c5 - s1 * c4 + s2 * c3 + s3 * c2 - s4 * c1 + s5 * c0
    }

    /// Computes the analytical inverse of the 4x4 matrix.
    pub fn inverse(&self) -> Option<Self> {
        let m = &self.m;
        let s0 = m[0] * m[5] - m[1] * m[4];
        let s1 = m[0] * m[6] - m[2] * m[4];
        let s2 = m[0] * m[7] - m[3] * m[4];
        let s3 = m[1] * m[6] - m[2] * m[5];
        let s4 = m[1] * m[7] - m[3] * m[5];
        let s5 = m[2] * m[7] - m[3] * m[6];

        let c5 = m[10] * m[15] - m[11] * m[14];
        let c4 = m[9] * m[15] - m[11] * m[13];
        let c3 = m[9] * m[14] - m[10] * m[13];
        let c2 = m[8] * m[15] - m[11] * m[12];
        let c1 = m[8] * m[14] - m[10] * m[12];
        let c0 = m[8] * m[13] - m[9] * m[12];

        let det = s0 * c5 - s1 * c4 + s2 * c3 + s3 * c2 - s4 * c1 + s5 * c0;
        if det.abs() < 1e-7 {
            return None;
        }

        let inv_det = 1.0 / det;
        let mut inv = [0.0f32; 16];

        inv[0] = (m[5] * c5 - m[6] * c4 + m[7] * c3) * inv_det;
        inv[1] = (-m[1] * c5 + m[2] * c4 - m[3] * c3) * inv_det;
        inv[2] = (m[13] * s5 - m[14] * s4 + m[15] * s3) * inv_det;
        inv[3] = (-m[9] * s5 + m[10] * s4 - m[11] * s3) * inv_det;

        inv[4] = (-m[4] * c5 + m[6] * c2 - m[7] * c1) * inv_det;
        inv[5] = (m[0] * c5 - m[2] * c2 + m[3] * c1) * inv_det;
        inv[6] = (-m[12] * s5 + m[14] * s2 - m[15] * s1) * inv_det;
        inv[7] = (m[8] * s5 - m[10] * s2 + m[11] * s1) * inv_det;

        inv[8] = (m[4] * c4 - m[5] * c2 + m[7] * c0) * inv_det;
        inv[9] = (-m[0] * c4 + m[1] * c2 - m[3] * c0) * inv_det;
        inv[10] = (m[12] * s4 - m[13] * s2 + m[15] * s0) * inv_det;
        inv[11] = (-m[8] * s4 + m[9] * s2 - m[11] * s0) * inv_det;

        inv[12] = (-m[4] * c3 + m[5] * c1 - m[6] * c0) * inv_det;
        inv[13] = (m[0] * c3 - m[1] * c1 + m[2] * c0) * inv_det;
        inv[14] = (-m[12] * s3 + m[13] * s1 - m[14] * s0) * inv_det;
        inv[15] = (m[8] * s3 - m[9] * s1 + m[10] * s0) * inv_det;

        Some(Self { m: inv })
    }

    /// Transforms a 2D point using homogeneous coordinates with perspective divide.
    pub fn transform_point2d(&self, p: Point<f32>) -> Point<f32> {
        let m = &self.m;
        let x = m[0] * p.x + m[4] * p.y + m[12];
        let y = m[1] * p.x + m[5] * p.y + m[13];
        let w = m[3] * p.x + m[7] * p.y + m[15];

        if w.abs() > 1e-6 && (w - 1.0).abs() > 1e-6 {
            Point::new(x / w, y / w)
        } else {
            Point::new(x, y)
        }
    }

    /// Transforms 2D coordinates `(x, y)` using homogeneous coordinates with perspective divide.
    #[inline]
    pub fn transform_point(&self, x: f32, y: f32) -> (f32, f32) {
        let p = self.transform_point2d(Point::new(x, y));
        (p.x, p.y)
    }

    /// Transforms an axis-aligned rectangle, computing its new bounding box.
    pub fn transform_rect(&self, r: Rect<f32>) -> Rect<f32> {
        let p1 = self.transform_point2d(Point::new(r.origin.x, r.origin.y));
        let p2 = self.transform_point2d(Point::new(r.origin.x + r.size.width, r.origin.y));
        let p3 = self.transform_point2d(Point::new(r.origin.x, r.origin.y + r.size.height));
        let p4 = self.transform_point2d(Point::new(r.origin.x + r.size.width, r.origin.y + r.size.height));

        let min_x = p1.x.min(p2.x).min(p3.x).min(p4.x);
        let max_x = p1.x.max(p2.x).max(p3.x).max(p4.x);
        let min_y = p1.y.min(p2.y).min(p3.y).min(p4.y);
        let max_y = p1.y.max(p2.y).max(p3.y).max(p4.y);

        Rect::from_xywh(min_x, min_y, max_x - min_x, max_y - min_y)
    }

    /// Transforms an axis-aligned rectangle `(x, y, width, height)` returning the new bounding box `(min_x, min_y, w, h)`.
    #[inline]
    pub fn transform_bounds(&self, x: f32, y: f32, width: f32, height: f32) -> (f32, f32, f32, f32) {
        let r = self.transform_rect(Rect::from_xywh(x, y, width, height));
        (r.origin.x, r.origin.y, r.size.width, r.size.height)
    }

    /// Returns true if this matrix is approximately the identity matrix.
    pub fn is_identity(&self) -> bool {
        let id = Self::IDENTITY.m;
        for (i, &expected) in id.iter().enumerate() {
            if (self.m[i] - expected).abs() > 1e-5 {
                return false;
            }
        }
        true
    }

    /// Returns true if this matrix represents a pure 2D affine transformation (no Z or perspective).
    pub fn is_2d(&self) -> bool {
        let m = &self.m;
        m[2].abs() < 1e-5 && m[3].abs() < 1e-5
            && m[6].abs() < 1e-5 && m[7].abs() < 1e-5
            && m[8].abs() < 1e-5 && m[9].abs() < 1e-5 && (m[10] - 1.0).abs() < 1e-5 && m[11].abs() < 1e-5
            && m[14].abs() < 1e-5 && (m[15] - 1.0).abs() < 1e-5
    }
}
