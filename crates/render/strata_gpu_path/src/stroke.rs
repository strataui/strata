//! Analytical stroke offset expansion, unit normal evaluation, and cap/join generation.

use crate::curve::{Point2D, QuadraticBezier};

/// Line termination cap style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    Square,
}

/// Line segment junction style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

/// Stroke styling parameters for analytical hardware expansion.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokeOptions {
    pub width: f32,
    pub miter_limit: f32,
    pub cap: LineCap,
    pub join: LineJoin,
}

impl Default for StrokeOptions {
    fn default() -> Self {
        Self {
            width: 1.0,
            miter_limit: 4.0,
            cap: LineCap::Butt,
            join: LineJoin::Miter,
        }
    }
}

impl StrokeOptions {
    pub fn new(width: f32) -> Self {
        Self {
            width,
            ..Default::default()
        }
    }

    pub fn with_cap(mut self, cap: LineCap) -> Self {
        self.cap = cap;
        self
    }

    pub fn with_join(mut self, join: LineJoin) -> Self {
        self.join = join;
        self
    }

    pub fn with_miter_limit(mut self, limit: f32) -> Self {
        self.miter_limit = limit;
        self
    }
}

/// Computes the 2D unit normal vector (-dy, dx) for a given tangent vector.
pub fn unit_normal(tangent: Point2D) -> Point2D {
    let len_sq = tangent.x * tangent.x + tangent.y * tangent.y;
    if len_sq < 1e-8 {
        return Point2D::new(0.0, 1.0);
    }
    let inv_len = 1.0 / len_sq.sqrt();
    Point2D::new(-tangent.y * inv_len, tangent.x * inv_len)
}

/// Expands a quadratic curve into parallel offset curves at distance `+offset` and `-offset`.
pub fn offset_quadratic(curve: &QuadraticBezier, offset: f32) -> (QuadraticBezier, QuadraticBezier) {
    let n0 = unit_normal(curve.tangent(0.0));
    let n1 = unit_normal(curve.tangent(0.5));
    let n2 = unit_normal(curve.tangent(1.0));

    let p0_outer = Point2D::new(curve.p0.x + n0.x * offset, curve.p0.y + n0.y * offset);
    let p1_outer = Point2D::new(curve.p1.x + n1.x * offset, curve.p1.y + n1.y * offset);
    let p2_outer = Point2D::new(curve.p2.x + n2.x * offset, curve.p2.y + n2.y * offset);

    let p0_inner = Point2D::new(curve.p0.x - n0.x * offset, curve.p0.y - n0.y * offset);
    let p1_inner = Point2D::new(curve.p1.x - n1.x * offset, curve.p1.y - n1.y * offset);
    let p2_inner = Point2D::new(curve.p2.x - n2.x * offset, curve.p2.y - n2.y * offset);

    (
        QuadraticBezier::new(p0_outer, p1_outer, p2_outer),
        QuadraticBezier::new(p0_inner, p1_inner, p2_inner),
    )
}

/// Expands a sequence of connected quadratic curves into a closed outline representing the stroked envelope.
pub fn expand_stroke_quadratics(
    quads: &[QuadraticBezier],
    options: &StrokeOptions,
) -> Vec<QuadraticBezier> {
    if quads.is_empty() {
        return Vec::new();
    }

    let half_width = options.width * 0.5;
    let mut outer_curves = Vec::with_capacity(quads.len());
    let mut inner_curves = Vec::with_capacity(quads.len());

    for q in quads {
        let (outer, inner) = offset_quadratic(q, half_width);
        outer_curves.push(outer);
        inner_curves.push(inner);
    }

    let mut result = Vec::new();

    // 1. Forward outer curves with joins
    for i in 0..outer_curves.len() {
        result.push(outer_curves[i]);
        if i + 1 < outer_curves.len() {
            let cur_end = outer_curves[i].p2;
            let next_start = outer_curves[i + 1].p0;
            if cur_end.distance_squared(next_start) > 1e-4 {
                match options.join {
                    LineJoin::Bevel | LineJoin::Miter => {
                        let mid = cur_end.lerp(next_start, 0.5);
                        result.push(QuadraticBezier::new(cur_end, mid, next_start));
                    }
                    LineJoin::Round => {
                        let orig_junction = quads[i].p2;
                        result.push(QuadraticBezier::new(cur_end, orig_junction, next_start));
                    }
                }
            }
        }
    }

    // 2. End cap connecting outer to inner
    let last_outer = outer_curves.last().unwrap().p2;
    let last_inner = inner_curves.last().unwrap().p2;
    match options.cap {
        LineCap::Butt => {
            let mid = last_outer.lerp(last_inner, 0.5);
            result.push(QuadraticBezier::new(last_outer, mid, last_inner));
        }
        LineCap::Square => {
            let tangent = quads.last().unwrap().tangent(1.0);
            let len = (tangent.x * tangent.x + tangent.y * tangent.y).sqrt().max(1e-4);
            let forward = Point2D::new(tangent.x / len * half_width, tangent.y / len * half_width);

            let ext_outer = Point2D::new(last_outer.x + forward.x, last_outer.y + forward.y);
            let ext_inner = Point2D::new(last_inner.x + forward.x, last_inner.y + forward.y);

            let mid1 = last_outer.lerp(ext_outer, 0.5);
            result.push(QuadraticBezier::new(last_outer, mid1, ext_outer));
            let mid2 = ext_outer.lerp(ext_inner, 0.5);
            result.push(QuadraticBezier::new(ext_outer, mid2, ext_inner));
            let mid3 = ext_inner.lerp(last_inner, 0.5);
            result.push(QuadraticBezier::new(ext_inner, mid3, last_inner));
        }
        LineCap::Round => {
            let tangent = quads.last().unwrap().tangent(1.0);
            let len = (tangent.x * tangent.x + tangent.y * tangent.y).sqrt().max(1e-4);
            let forward = Point2D::new(tangent.x / len * half_width, tangent.y / len * half_width);
            let apex = Point2D::new(
                (last_outer.x + last_inner.x) * 0.5 + forward.x,
                (last_outer.y + last_inner.y) * 0.5 + forward.y,
            );
            result.push(QuadraticBezier::new(last_outer, apex, last_inner));
        }
    }

    // 3. Reversed inner curves
    for i in (0..inner_curves.len()).rev() {
        let q = inner_curves[i];
        let reversed = QuadraticBezier::new(q.p2, q.p1, q.p0);
        result.push(reversed);
    }

    // 4. Start cap connecting inner to outer
    let first_inner = inner_curves.first().unwrap().p0;
    let first_outer = outer_curves.first().unwrap().p0;
    match options.cap {
        LineCap::Butt => {
            let mid = first_inner.lerp(first_outer, 0.5);
            result.push(QuadraticBezier::new(first_inner, mid, first_outer));
        }
        LineCap::Square => {
            let tangent = quads.first().unwrap().tangent(0.0);
            let len = (tangent.x * tangent.x + tangent.y * tangent.y).sqrt().max(1e-4);
            let backward = Point2D::new(-tangent.x / len * half_width, -tangent.y / len * half_width);

            let ext_inner = Point2D::new(first_inner.x + backward.x, first_inner.y + backward.y);
            let ext_outer = Point2D::new(first_outer.x + backward.x, first_outer.y + backward.y);

            let mid1 = first_inner.lerp(ext_inner, 0.5);
            result.push(QuadraticBezier::new(first_inner, mid1, ext_inner));
            let mid2 = ext_inner.lerp(ext_outer, 0.5);
            result.push(QuadraticBezier::new(ext_inner, mid2, ext_outer));
            let mid3 = ext_outer.lerp(first_outer, 0.5);
            result.push(QuadraticBezier::new(ext_outer, mid3, first_outer));
        }
        LineCap::Round => {
            let tangent = quads.first().unwrap().tangent(0.0);
            let len = (tangent.x * tangent.x + tangent.y * tangent.y).sqrt().max(1e-4);
            let backward = Point2D::new(-tangent.x / len * half_width, -tangent.y / len * half_width);
            let apex = Point2D::new(
                (first_inner.x + first_outer.x) * 0.5 + backward.x,
                (first_inner.y + first_outer.y) * 0.5 + backward.y,
            );
            result.push(QuadraticBezier::new(first_inner, apex, first_outer));
        }
    }

    result
}
