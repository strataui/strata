//! Vector Path Representation and Mathematical Geometry for Strata Icons.

use strata_primitives::{Point, Rect, Size};

/// Atomic vector path drawing commands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathCommand {
    /// Move cursor to point without drawing.
    MoveTo(f32, f32),
    /// Draw straight line from current point to destination.
    LineTo(f32, f32),
    /// Draw quadratic Bézier curve with single control point.
    QuadCurveTo {
        cp: (f32, f32),
        to: (f32, f32),
    },
    /// Draw cubic Bézier curve with two control points.
    CubicCurveTo {
        cp1: (f32, f32),
        cp2: (f32, f32),
        to: (f32, f32),
    },
    /// Draw elliptical arc.
    ArcTo {
        rx: f32,
        ry: f32,
        x_axis_rotation: f32,
        large_arc: bool,
        sweep: bool,
        to: (f32, f32),
    },
    /// Close current sub-path with straight line to initial point.
    Close,
}

/// A structured 2D vector path composed of a sequence of commands.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VectorPath {
    pub commands: Vec<PathCommand>,
}

impl VectorPath {
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            commands: Vec::with_capacity(capacity),
        }
    }

    pub fn move_to(&mut self, x: f32, y: f32) -> &mut Self {
        self.commands.push(PathCommand::MoveTo(x, y));
        self
    }

    pub fn line_to(&mut self, x: f32, y: f32) -> &mut Self {
        self.commands.push(PathCommand::LineTo(x, y));
        self
    }

    pub fn quad_to(&mut self, cpx: f32, cpy: f32, x: f32, y: f32) -> &mut Self {
        self.commands.push(PathCommand::QuadCurveTo {
            cp: (cpx, cpy),
            to: (x, y),
        });
        self
    }

    pub fn cubic_to(
        &mut self,
        cp1x: f32,
        cp1y: f32,
        cp2x: f32,
        cp2y: f32,
        x: f32,
        y: f32,
    ) -> &mut Self {
        self.commands.push(PathCommand::CubicCurveTo {
            cp1: (cp1x, cp1y),
            cp2: (cp2x, cp2y),
            to: (x, y),
        });
        self
    }

    pub fn arc_to(
        &mut self,
        rx: f32,
        ry: f32,
        x_axis_rotation: f32,
        large_arc: bool,
        sweep: bool,
        x: f32,
        y: f32,
    ) -> &mut Self {
        self.commands.push(PathCommand::ArcTo {
            rx,
            ry,
            x_axis_rotation,
            large_arc,
            sweep,
            to: (x, y),
        });
        self
    }

    pub fn close(&mut self) -> &mut Self {
        self.commands.push(PathCommand::Close);
        self
    }

    /// Computes the axis-aligned bounding box enclosing all endpoints and control points.
    pub fn compute_bounds(&self) -> Rect {
        if self.commands.is_empty() {
            return Rect::ZERO;
        }

        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;

        let mut update = |x: f32, y: f32| {
            if x < min_x { min_x = x; }
            if x > max_x { max_x = x; }
            if y < min_y { min_y = y; }
            if y > max_y { max_y = y; }
        };

        for cmd in &self.commands {
            match cmd {
                PathCommand::MoveTo(x, y) | PathCommand::LineTo(x, y) => {
                    update(*x, *y);
                }
                PathCommand::QuadCurveTo { cp, to } => {
                    update(cp.0, cp.1);
                    update(to.0, to.1);
                }
                PathCommand::CubicCurveTo { cp1, cp2, to } => {
                    update(cp1.0, cp1.1);
                    update(cp2.0, cp2.1);
                    update(to.0, to.1);
                }
                PathCommand::ArcTo { to, .. } => {
                    update(to.0, to.1);
                }
                PathCommand::Close => {}
            }
        }

        if min_x.is_infinite() || min_y.is_infinite() {
            return Rect::ZERO;
        }

        Rect::new(
            Point::new(min_x, min_y),
            Size::new((max_x - min_x).max(0.0), (max_y - min_y).max(0.0)),
        )
    }

    /// Serializes commands into standard SVG `d` path syntax.
    pub fn to_svg_d(&self) -> String {
        let mut out = String::new();
        for (i, cmd) in self.commands.iter().enumerate() {
            if i > 0 {
                out.push(' ');
            }
            match cmd {
                PathCommand::MoveTo(x, y) => out.push_str(&format!("M {x} {y}")),
                PathCommand::LineTo(x, y) => out.push_str(&format!("L {x} {y}")),
                PathCommand::QuadCurveTo { cp, to } => {
                    out.push_str(&format!("Q {} {} {} {}", cp.0, cp.1, to.0, to.1))
                }
                PathCommand::CubicCurveTo { cp1, cp2, to } => {
                    out.push_str(&format!("C {} {} {} {} {} {}", cp1.0, cp1.1, cp2.0, cp2.1, to.0, to.1))
                }
                PathCommand::ArcTo { rx, ry, x_axis_rotation, large_arc, sweep, to } => {
                    let large = if *large_arc { 1 } else { 0 };
                    let swp = if *sweep { 1 } else { 0 };
                    out.push_str(&format!("A {rx} {ry} {x_axis_rotation} {large} {swp} {} {}", to.0, to.1))
                }
                PathCommand::Close => out.push('Z'),
            }
        }
        out
    }
}
