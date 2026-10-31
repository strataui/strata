//! SVG path construction, verb sequence, and stroke/fill styling.

use crate::curve::{CubicBezier, Point2D, QuadraticBezier, Rect2D};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathVerb {
    MoveTo(Point2D),
    LineTo(Point2D),
    QuadTo(Point2D, Point2D),
    CubicTo(Point2D, Point2D, Point2D),
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillRule {
    NonZero,
    EvenOdd,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokeStyle {
    pub width: f32,
    pub miter_limit: f32,
}

impl Default for StrokeStyle {
    fn default() -> Self {
        Self {
            width: 1.0,
            miter_limit: 4.0,
        }
    }
}

/// A 2D vector path composed of a sequence of path verbs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Path {
    pub verbs: Vec<PathVerb>,
    pub fill_rule: Option<FillRule>,
    pub stroke: Option<StrokeStyle>,
}

impl Path {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_fill(mut self, rule: FillRule) -> Self {
        self.fill_rule = Some(rule);
        self
    }

    pub fn with_stroke(mut self, stroke: StrokeStyle) -> Self {
        self.stroke = Some(stroke);
        self
    }

    /// Decomposes all path verbs into a flat sequence of Quadratic Bézier curves.
    pub fn to_quadratics(&self, tolerance: f32) -> Vec<QuadraticBezier> {
        let mut quadratics = Vec::new();
        let mut current_pt = Point2D::new(0.0, 0.0);
        let mut start_subpath = Point2D::new(0.0, 0.0);

        for verb in &self.verbs {
            match *verb {
                PathVerb::MoveTo(pt) => {
                    current_pt = pt;
                    start_subpath = pt;
                }
                PathVerb::LineTo(pt) => {
                    // Represent line segment as degenerate quadratic with control point at midpoint
                    let mid = current_pt.lerp(pt, 0.5);
                    quadratics.push(QuadraticBezier::new(current_pt, mid, pt));
                    current_pt = pt;
                }
                PathVerb::QuadTo(p1, p2) => {
                    quadratics.push(QuadraticBezier::new(current_pt, p1, p2));
                    current_pt = p2;
                }
                PathVerb::CubicTo(p1, p2, p3) => {
                    let cubic = CubicBezier::new(current_pt, p1, p2, p3);
                    quadratics.extend(cubic.to_quadratics(tolerance));
                    current_pt = p3;
                }
                PathVerb::Close => {
                    if current_pt != start_subpath {
                        let mid = current_pt.lerp(start_subpath, 0.5);
                        quadratics.push(QuadraticBezier::new(current_pt, mid, start_subpath));
                        current_pt = start_subpath;
                    }
                }
            }
        }

        quadratics
    }

    /// Computes the bounding box for the entire path.
    pub fn bounding_box(&self) -> Option<Rect2D> {
        let quads = self.to_quadratics(0.1);
        if quads.is_empty() {
            return None;
        }

        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;

        for q in quads {
            let bb = q.bounding_box();
            min_x = min_x.min(bb.min_x);
            min_y = min_y.min(bb.min_y);
            max_x = max_x.max(bb.max_x);
            max_y = max_y.max(bb.max_y);
        }

        Some(Rect2D::new(min_x, min_y, max_x, max_y))
    }
}

/// Fluent builder for constructing vector paths, including SVG path string parsing.
pub struct PathBuilder {
    path: Path,
}

impl Default for PathBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PathBuilder {
    pub fn new() -> Self {
        Self { path: Path::new() }
    }

    pub fn move_to(mut self, x: f32, y: f32) -> Self {
        self.path.verbs.push(PathVerb::MoveTo(Point2D::new(x, y)));
        self
    }

    pub fn line_to(mut self, x: f32, y: f32) -> Self {
        self.path.verbs.push(PathVerb::LineTo(Point2D::new(x, y)));
        self
    }

    pub fn quad_to(mut self, cx: f32, cy: f32, x: f32, y: f32) -> Self {
        self.path.verbs.push(PathVerb::QuadTo(Point2D::new(cx, cy), Point2D::new(x, y)));
        self
    }

    pub fn cubic_to(mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) -> Self {
        self.path.verbs.push(PathVerb::CubicTo(
            Point2D::new(c1x, c1y),
            Point2D::new(c2x, c2y),
            Point2D::new(x, y),
        ));
        self
    }

    pub fn close(mut self) -> Self {
        self.path.verbs.push(PathVerb::Close);
        self
    }

    pub fn fill(mut self, rule: FillRule) -> Self {
        self.path.fill_rule = Some(rule);
        self
    }

    pub fn stroke(mut self, style: StrokeStyle) -> Self {
        self.path.stroke = Some(style);
        self
    }

    pub fn build(self) -> Path {
        self.path
    }

    /// Parses an SVG path data string (e.g. "M 10 20 L 50 60 Q 80 90 100 120 Z").
    pub fn from_svg(svg_data: &str) -> Path {
        let mut builder = PathBuilder::new();
        let tokens: Vec<&str> = svg_data.split_whitespace().collect();
        let mut idx = 0;

        while idx < tokens.len() {
            let cmd = tokens[idx];
            idx += 1;

            match cmd {
                "M" | "m" if idx + 1 < tokens.len() => {
                    let x: f32 = tokens[idx].parse().unwrap_or(0.0);
                    let y: f32 = tokens[idx + 1].parse().unwrap_or(0.0);
                    idx += 2;
                    builder = builder.move_to(x, y);
                }
                "L" | "l" if idx + 1 < tokens.len() => {
                    let x: f32 = tokens[idx].parse().unwrap_or(0.0);
                    let y: f32 = tokens[idx + 1].parse().unwrap_or(0.0);
                    idx += 2;
                    builder = builder.line_to(x, y);
                }
                "Q" | "q" if idx + 3 < tokens.len() => {
                    let cx: f32 = tokens[idx].parse().unwrap_or(0.0);
                    let cy: f32 = tokens[idx + 1].parse().unwrap_or(0.0);
                    let x: f32 = tokens[idx + 2].parse().unwrap_or(0.0);
                    let y: f32 = tokens[idx + 3].parse().unwrap_or(0.0);
                    idx += 4;
                    builder = builder.quad_to(cx, cy, x, y);
                }
                "C" | "c" if idx + 5 < tokens.len() => {
                    let c1x: f32 = tokens[idx].parse().unwrap_or(0.0);
                    let c1y: f32 = tokens[idx + 1].parse().unwrap_or(0.0);
                    let c2x: f32 = tokens[idx + 2].parse().unwrap_or(0.0);
                    let c2y: f32 = tokens[idx + 3].parse().unwrap_or(0.0);
                    let x: f32 = tokens[idx + 4].parse().unwrap_or(0.0);
                    let y: f32 = tokens[idx + 5].parse().unwrap_or(0.0);
                    idx += 6;
                    builder = builder.cubic_to(c1x, c1y, c2x, c2y, x, y);
                }
                "Z" | "z" => {
                    builder = builder.close();
                }
                _ => {}
            }
        }

        builder.build()
    }
}
