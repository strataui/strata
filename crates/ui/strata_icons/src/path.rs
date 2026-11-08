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

    #[allow(clippy::too_many_arguments)]
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

    /// Compresses path commands into a compact binary byte representation.
    pub fn to_binary(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.commands.len() * 9 + 4);
        out.extend_from_slice(&(self.commands.len() as u32).to_le_bytes());
        for cmd in &self.commands {
            match cmd {
                PathCommand::MoveTo(x, y) => {
                    out.push(1);
                    out.extend_from_slice(&x.to_le_bytes());
                    out.extend_from_slice(&y.to_le_bytes());
                }
                PathCommand::LineTo(x, y) => {
                    out.push(2);
                    out.extend_from_slice(&x.to_le_bytes());
                    out.extend_from_slice(&y.to_le_bytes());
                }
                PathCommand::QuadCurveTo { cp, to } => {
                    out.push(3);
                    out.extend_from_slice(&cp.0.to_le_bytes());
                    out.extend_from_slice(&cp.1.to_le_bytes());
                    out.extend_from_slice(&to.0.to_le_bytes());
                    out.extend_from_slice(&to.1.to_le_bytes());
                }
                PathCommand::CubicCurveTo { cp1, cp2, to } => {
                    out.push(4);
                    out.extend_from_slice(&cp1.0.to_le_bytes());
                    out.extend_from_slice(&cp1.1.to_le_bytes());
                    out.extend_from_slice(&cp2.0.to_le_bytes());
                    out.extend_from_slice(&cp2.1.to_le_bytes());
                    out.extend_from_slice(&to.0.to_le_bytes());
                    out.extend_from_slice(&to.1.to_le_bytes());
                }
                PathCommand::ArcTo {
                    rx,
                    ry,
                    x_axis_rotation,
                    large_arc,
                    sweep,
                    to,
                } => {
                    out.push(5);
                    out.extend_from_slice(&rx.to_le_bytes());
                    out.extend_from_slice(&ry.to_le_bytes());
                    out.extend_from_slice(&x_axis_rotation.to_le_bytes());
                    let flags = (if *large_arc { 1 } else { 0 }) | (if *sweep { 2 } else { 0 });
                    out.push(flags);
                    out.extend_from_slice(&to.0.to_le_bytes());
                    out.extend_from_slice(&to.1.to_le_bytes());
                }
                PathCommand::Close => {
                    out.push(6);
                }
            }
        }
        out
    }

    /// Decompresses path commands from binary byte representation.
    pub fn from_binary(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() < 4 {
            return Err("Binary payload too short");
        }
        let count = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        let mut idx = 4;
        let mut commands = Vec::with_capacity(count);

        for _ in 0..count {
            if idx >= bytes.len() {
                return Err("Truncated binary command");
            }
            let tag = bytes[idx];
            idx += 1;
            match tag {
                1 => {
                    if idx + 8 > bytes.len() {
                        return Err("Truncated MoveTo");
                    }
                    let x = f32::from_le_bytes([bytes[idx], bytes[idx + 1], bytes[idx + 2], bytes[idx + 3]]);
                    let y = f32::from_le_bytes([bytes[idx + 4], bytes[idx + 5], bytes[idx + 6], bytes[idx + 7]]);
                    idx += 8;
                    commands.push(PathCommand::MoveTo(x, y));
                }
                2 => {
                    if idx + 8 > bytes.len() {
                        return Err("Truncated LineTo");
                    }
                    let x = f32::from_le_bytes([bytes[idx], bytes[idx + 1], bytes[idx + 2], bytes[idx + 3]]);
                    let y = f32::from_le_bytes([bytes[idx + 4], bytes[idx + 5], bytes[idx + 6], bytes[idx + 7]]);
                    idx += 8;
                    commands.push(PathCommand::LineTo(x, y));
                }
                3 => {
                    if idx + 16 > bytes.len() {
                        return Err("Truncated QuadCurveTo");
                    }
                    let cpx = f32::from_le_bytes([bytes[idx], bytes[idx + 1], bytes[idx + 2], bytes[idx + 3]]);
                    let cpy = f32::from_le_bytes([bytes[idx + 4], bytes[idx + 5], bytes[idx + 6], bytes[idx + 7]]);
                    let tox = f32::from_le_bytes([bytes[idx + 8], bytes[idx + 9], bytes[idx + 10], bytes[idx + 11]]);
                    let toy = f32::from_le_bytes([bytes[idx + 12], bytes[idx + 13], bytes[idx + 14], bytes[idx + 15]]);
                    idx += 16;
                    commands.push(PathCommand::QuadCurveTo {
                        cp: (cpx, cpy),
                        to: (tox, toy),
                    });
                }
                4 => {
                    if idx + 24 > bytes.len() {
                        return Err("Truncated CubicCurveTo");
                    }
                    let cp1x = f32::from_le_bytes([bytes[idx], bytes[idx + 1], bytes[idx + 2], bytes[idx + 3]]);
                    let cp1y = f32::from_le_bytes([bytes[idx + 4], bytes[idx + 5], bytes[idx + 6], bytes[idx + 7]]);
                    let cp2x = f32::from_le_bytes([bytes[idx + 8], bytes[idx + 9], bytes[idx + 10], bytes[idx + 11]]);
                    let cp2y = f32::from_le_bytes([bytes[idx + 12], bytes[idx + 13], bytes[idx + 14], bytes[idx + 15]]);
                    let tox = f32::from_le_bytes([bytes[idx + 16], bytes[idx + 17], bytes[idx + 18], bytes[idx + 19]]);
                    let toy = f32::from_le_bytes([bytes[idx + 20], bytes[idx + 21], bytes[idx + 22], bytes[idx + 23]]);
                    idx += 24;
                    commands.push(PathCommand::CubicCurveTo {
                        cp1: (cp1x, cp1y),
                        cp2: (cp2x, cp2y),
                        to: (tox, toy),
                    });
                }
                5 => {
                    if idx + 21 > bytes.len() {
                        return Err("Truncated ArcTo");
                    }
                    let rx = f32::from_le_bytes([bytes[idx], bytes[idx + 1], bytes[idx + 2], bytes[idx + 3]]);
                    let ry = f32::from_le_bytes([bytes[idx + 4], bytes[idx + 5], bytes[idx + 6], bytes[idx + 7]]);
                    let rot = f32::from_le_bytes([bytes[idx + 8], bytes[idx + 9], bytes[idx + 10], bytes[idx + 11]]);
                    let flags = bytes[idx + 12];
                    let tox = f32::from_le_bytes([bytes[idx + 13], bytes[idx + 14], bytes[idx + 15], bytes[idx + 16]]);
                    let toy = f32::from_le_bytes([bytes[idx + 17], bytes[idx + 18], bytes[idx + 19], bytes[idx + 20]]);
                    idx += 21;
                    commands.push(PathCommand::ArcTo {
                        rx,
                        ry,
                        x_axis_rotation: rot,
                        large_arc: (flags & 1) != 0,
                        sweep: (flags & 2) != 0,
                        to: (tox, toy),
                    });
                }
                6 => {
                    commands.push(PathCommand::Close);
                }
                _ => return Err("Unknown binary command tag"),
            }
        }

        Ok(Self { commands })
    }
}
