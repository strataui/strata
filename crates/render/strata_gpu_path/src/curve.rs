//! Parametric Quadratic and Cubic Bézier mathematical primitives.

/// 2D point representation for vector curves.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point2D {
    pub x: f32,
    pub y: f32,
}

impl Point2D {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn distance_squared(&self, other: Point2D) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        dx * dx + dy * dy
    }

    pub fn distance(&self, other: Point2D) -> f32 {
        self.distance_squared(other).sqrt()
    }

    pub fn lerp(&self, other: Point2D, t: f32) -> Point2D {
        Point2D::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
        )
    }
}

/// Axis-aligned 2D bounding box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect2D {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

impl Rect2D {
    pub fn new(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Self {
        Self { min_x, min_y, max_x, max_y }
    }

    pub fn contains(&self, p: Point2D) -> bool {
        p.x >= self.min_x && p.x <= self.max_x && p.y >= self.min_y && p.y <= self.max_y
    }

    pub fn width(&self) -> f32 {
        self.max_x - self.min_x
    }

    pub fn height(&self) -> f32 {
        self.max_y - self.min_y
    }
}

impl From<strata_primitives::geom::Point<f32>> for Point2D {
    fn from(p: strata_primitives::geom::Point<f32>) -> Self {
        Self::new(p.x, p.y)
    }
}

impl From<Point2D> for strata_primitives::geom::Point<f32> {
    fn from(p: Point2D) -> Self {
        strata_primitives::geom::Point::new(p.x, p.y)
    }
}

impl From<strata_primitives::geom::Rect<f32>> for Rect2D {
    fn from(r: strata_primitives::geom::Rect<f32>) -> Self {
        Self::new(r.min_x(), r.min_y(), r.max_x(), r.max_y())
    }
}

impl From<Rect2D> for strata_primitives::geom::Rect<f32> {
    fn from(r: Rect2D) -> Self {
        strata_primitives::geom::Rect::from_xywh(r.min_x, r.min_y, r.width(), r.height())
    }
}

/// Quadratic Bézier curve defined by three control points: P0 (start), P1 (control), P2 (end).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuadraticBezier {
    pub p0: Point2D,
    pub p1: Point2D,
    pub p2: Point2D,
}

impl QuadraticBezier {
    pub const fn new(p0: Point2D, p1: Point2D, p2: Point2D) -> Self {
        Self { p0, p1, p2 }
    }

    /// Evaluates curve at parameter t in [0.0, 1.0].
    /// B(t) = (1-t)^2 * P0 + 2t(1-t) * P1 + t^2 * P2
    pub fn evaluate(&self, t: f32) -> Point2D {
        let u = 1.0 - t;
        let x = u * u * self.p0.x + 2.0 * u * t * self.p1.x + t * t * self.p2.x;
        let y = u * u * self.p0.y + 2.0 * u * t * self.p1.y + t * t * self.p2.y;
        Point2D::new(x, y)
    }

    /// First derivative tangent vector B'(t) = 2(1-t)(P1 - P0) + 2t(P2 - P1).
    pub fn tangent(&self, t: f32) -> Point2D {
        let u = 1.0 - t;
        let dx = 2.0 * u * (self.p1.x - self.p0.x) + 2.0 * t * (self.p2.x - self.p1.x);
        let dy = 2.0 * u * (self.p1.y - self.p0.y) + 2.0 * t * (self.p2.y - self.p1.y);
        Point2D::new(dx, dy)
    }

    /// Subdivides the quadratic curve at parameter t using De Casteljau's algorithm.
    pub fn split(&self, t: f32) -> (QuadraticBezier, QuadraticBezier) {
        let p01 = self.p0.lerp(self.p1, t);
        let p12 = self.p1.lerp(self.p2, t);
        let p012 = p01.lerp(p12, t);

        (
            QuadraticBezier::new(self.p0, p01, p012),
            QuadraticBezier::new(p012, p12, self.p2),
        )
    }

    /// Computes tight axis-aligned bounding box including curve extrema.
    pub fn bounding_box(&self) -> Rect2D {
        let mut min_x = self.p0.x.min(self.p2.x);
        let mut max_x = self.p0.x.max(self.p2.x);
        let mut min_y = self.p0.y.min(self.p2.y);
        let mut max_y = self.p0.y.max(self.p2.y);

        // Check horizontal extrema: d/dt X(t) = 0
        let denom_x = self.p0.x - 2.0 * self.p1.x + self.p2.x;
        if denom_x.abs() > 1e-6 {
            let tx = (self.p0.x - self.p1.x) / denom_x;
            if (0.0..=1.0).contains(&tx) {
                let x = self.evaluate(tx).x;
                min_x = min_x.min(x);
                max_x = max_x.max(x);
            }
        }

        // Check vertical extrema: d/dt Y(t) = 0
        let denom_y = self.p0.y - 2.0 * self.p1.y + self.p2.y;
        if denom_y.abs() > 1e-6 {
            let ty = (self.p0.y - self.p1.y) / denom_y;
            if (0.0..=1.0).contains(&ty) {
                let y = self.evaluate(ty).y;
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }

        Rect2D::new(min_x, min_y, max_x, max_y)
    }
}

/// Cubic Bézier curve defined by four control points: P0, P1, P2, P3.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CubicBezier {
    pub p0: Point2D,
    pub p1: Point2D,
    pub p2: Point2D,
    pub p3: Point2D,
}

impl CubicBezier {
    pub const fn new(p0: Point2D, p1: Point2D, p2: Point2D, p3: Point2D) -> Self {
        Self { p0, p1, p2, p3 }
    }

    /// Evaluates curve at parameter t in [0.0, 1.0].
    pub fn evaluate(&self, t: f32) -> Point2D {
        let u = 1.0 - t;
        let u2 = u * u;
        let u3 = u2 * u;
        let t2 = t * t;
        let t3 = t2 * t;

        let x = u3 * self.p0.x + 3.0 * u2 * t * self.p1.x + 3.0 * u * t2 * self.p2.x + t3 * self.p3.x;
        let y = u3 * self.p0.y + 3.0 * u2 * t * self.p1.y + 3.0 * u * t2 * self.p2.y + t3 * self.p3.y;
        Point2D::new(x, y)
    }

    /// Approximates a cubic Bézier curve with an adaptive sequence of quadratic Béziers for direct GPU evaluation.
    pub fn to_quadratics(&self, tolerance: f32) -> Vec<QuadraticBezier> {
        let mut result = Vec::new();
        self.subdivide_to_quadratics(tolerance, &mut result);
        result
    }

    fn subdivide_to_quadratics(&self, tolerance: f32, output: &mut Vec<QuadraticBezier>) {
        // Compute midpoint quadratic control point approximation: C = (3 P1 - P0 + 3 P2 - P3) / 4
        let cx = (3.0 * self.p1.x - self.p0.x + 3.0 * self.p2.x - self.p3.x) / 4.0;
        let cy = (3.0 * self.p1.y - self.p0.y + 3.0 * self.p2.y - self.p3.y) / 4.0;
        let quad = QuadraticBezier::new(self.p0, Point2D::new(cx, cy), self.p3);

        // Test error at t = 0.5
        let cubic_mid = self.evaluate(0.5);
        let quad_mid = quad.evaluate(0.5);
        let err = cubic_mid.distance(quad_mid);

        if err <= tolerance {
            output.push(quad);
        } else {
            // Split cubic in half using De Casteljau
            let p01 = self.p0.lerp(self.p1, 0.5);
            let p12 = self.p1.lerp(self.p2, 0.5);
            let p23 = self.p2.lerp(self.p3, 0.5);
            let p012 = p01.lerp(p12, 0.5);
            let p123 = p12.lerp(p23, 0.5);
            let p0123 = p012.lerp(p123, 0.5);

            let left = CubicBezier::new(self.p0, p01, p012, p0123);
            let right = CubicBezier::new(p0123, p123, p23, self.p3);

            left.subdivide_to_quadratics(tolerance, output);
            right.subdivide_to_quadratics(tolerance, output);
        }
    }
}
