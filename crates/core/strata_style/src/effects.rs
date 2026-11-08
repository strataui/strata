//! Modern CSS visual effects: Backdrop filters and arbitrary vector clip-paths.

use strata_primitives::geom::Point;

/// Individual filter operation applied in backdrop-filter or filter pipelines.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FilterOp {
    /// Gaussian blur radius in pixels.
    Blur(f32),
    /// Linear multiplier for element brightness (1.0 = normal).
    Brightness(f32),
    /// Linear multiplier for element contrast (1.0 = normal).
    Contrast(f32),
    /// Grayscale conversion fraction (0.0 = full color, 1.0 = full grayscale).
    Grayscale(f32),
    /// Color inversion fraction (0.0 = normal, 1.0 = completely inverted).
    Invert(f32),
    /// Alpha transparency fraction (0.0 = fully transparent, 1.0 = fully opaque).
    Opacity(f32),
    /// Color saturation multiplier (1.0 = normal, 0.0 = desaturated).
    Saturate(f32),
    /// Sepia toning fraction (0.0 = normal, 1.0 = full sepia).
    Sepia(f32),
    /// Drop shadow filter operation with offsets, blur radius, and color.
    DropShadow {
        offset_x: f32,
        offset_y: f32,
        blur_radius: f32,
        color: crate::types::Color,
    },
    /// Hue rotation in degrees.
    HueRotate(f32),
}

/// Composite filter specification applied to an element or underneath it.
pub type FilterList = BackdropFilter;

/// Composite backdrop filter specification applied to pixels underneath the element.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BackdropFilter {
    pub operations: Vec<FilterOp>,
}

impl BackdropFilter {
    pub fn new(operations: Vec<FilterOp>) -> Self {
        Self { operations }
    }

    /// Convenience constructor for Gaussian blur backdrop.
    pub fn blur(radius: f32) -> Self {
        Self {
            operations: vec![FilterOp::Blur(radius)],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }
}

/// CSS clip-path geometric masking shape.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ClipPath {
    #[default]
    None,
    /// Inset rectangular clip mask with optional rounded corners.
    Inset {
        top: f32,
        right: f32,
        bottom: f32,
        left: f32,
        border_radius: f32,
    },
    /// Circular clip mask with radius and center coordinates.
    Circle {
        radius: f32,
        center_x: f32,
        center_y: f32,
    },
    /// Elliptical clip mask with x/y radii and center coordinates.
    Ellipse {
        radius_x: f32,
        radius_y: f32,
        center_x: f32,
        center_y: f32,
    },
    /// Polygon mask defined by a sequence of 2D boundary vertices.
    Polygon(Vec<Point<f32>>),
    /// Arbitrary SVG path string.
    Path(String),
}

impl ClipPath {
    pub fn inset(top: f32, right: f32, bottom: f32, left: f32, border_radius: f32) -> Self {
        Self::Inset {
            top,
            right,
            bottom,
            left,
            border_radius,
        }
    }

    pub fn circle(radius: f32, center_x: f32, center_y: f32) -> Self {
        Self::Circle {
            radius,
            center_x,
            center_y,
        }
    }

    pub fn ellipse(radius_x: f32, radius_y: f32, center_x: f32, center_y: f32) -> Self {
        Self::Ellipse {
            radius_x,
            radius_y,
            center_x,
            center_y,
        }
    }

    pub fn polygon(vertices: Vec<Point<f32>>) -> Self {
        Self::Polygon(vertices)
    }

    pub fn path(path_str: impl Into<String>) -> Self {
        Self::Path(path_str.into())
    }

    pub fn is_none(&self) -> bool {
        matches!(self, ClipPath::None)
    }

    /// Returns whether a test point (x, y) falls inside the clipping boundary.
    pub fn contains_point(&self, x: f32, y: f32, container_w: f32, container_h: f32) -> bool {
        match self {
            ClipPath::None => true,
            ClipPath::Inset {
                top,
                right,
                bottom,
                left,
                ..
            } => {
                x >= *left
                    && x <= (container_w - *right)
                    && y >= *top
                    && y <= (container_h - *bottom)
            }
            ClipPath::Circle {
                radius,
                center_x,
                center_y,
            } => {
                let dx = x - *center_x;
                let dy = y - *center_y;
                (dx * dx + dy * dy) <= (radius * radius)
            }
            ClipPath::Ellipse {
                radius_x,
                radius_y,
                center_x,
                center_y,
            } => {
                if *radius_x <= 0.0 || *radius_y <= 0.0 {
                    return false;
                }
                let norm_x = (x - *center_x) / radius_x;
                let norm_y = (y - *center_y) / radius_y;
                (norm_x * norm_x + norm_y * norm_y) <= 1.0
            }
            ClipPath::Polygon(vertices) => {
                if vertices.len() < 3 {
                    return true;
                }
                // Ray casting algorithm for point-in-polygon
                let mut inside = false;
                let mut j = vertices.len() - 1;
                for i in 0..vertices.len() {
                    let pi = vertices[i];
                    let pj = vertices[j];

                    if ((pi.y > y) != (pj.y > y))
                        && (x < (pj.x - pi.x) * (y - pi.y) / (pj.y - pi.y + 1e-6) + pi.x)
                    {
                        inside = !inside;
                    }
                    j = i;
                }
                inside
            }
            ClipPath::Path(_) => true, // Evaluated via GPU path mask
        }
    }
}

/// Continuously interpolates between two `ClipPath` shapes per W3C CSS Masking Module Level 1.
pub fn interpolate_clip_path(from: &ClipPath, to: &ClipPath, t: f32) -> ClipPath {
    let t = t.clamp(0.0, 1.0);
    match (from, to) {
        (
            ClipPath::Inset {
                top: t1,
                right: r1,
                bottom: b1,
                left: l1,
                border_radius: br1,
            },
            ClipPath::Inset {
                top: t2,
                right: r2,
                bottom: b2,
                left: l2,
                border_radius: br2,
            },
        ) => ClipPath::Inset {
            top: t1 + (t2 - t1) * t,
            right: r1 + (r2 - r1) * t,
            bottom: b1 + (b2 - b1) * t,
            left: l1 + (l2 - l1) * t,
            border_radius: br1 + (br2 - br1) * t,
        },
        (
            ClipPath::Circle {
                radius: r1,
                center_x: x1,
                center_y: y1,
            },
            ClipPath::Circle {
                radius: r2,
                center_x: x2,
                center_y: y2,
            },
        ) => ClipPath::Circle {
            radius: r1 + (r2 - r1) * t,
            center_x: x1 + (x2 - x1) * t,
            center_y: y1 + (y2 - y1) * t,
        },
        (
            ClipPath::Ellipse {
                radius_x: rx1,
                radius_y: ry1,
                center_x: cx1,
                center_y: cy1,
            },
            ClipPath::Ellipse {
                radius_x: rx2,
                radius_y: ry2,
                center_x: cx2,
                center_y: cy2,
            },
        ) => ClipPath::Ellipse {
            radius_x: rx1 + (rx2 - rx1) * t,
            radius_y: ry1 + (ry2 - ry1) * t,
            center_x: cx1 + (cx2 - cx1) * t,
            center_y: cy1 + (cy2 - cy1) * t,
        },
        (ClipPath::Polygon(v1), ClipPath::Polygon(v2)) => {
            if v1.len() == v2.len() {
                let mut out = Vec::with_capacity(v1.len());
                for (p1, p2) in v1.iter().zip(v2.iter()) {
                    out.push(Point::new(
                        p1.x + (p2.x - p1.x) * t,
                        p1.y + (p2.y - p1.y) * t,
                    ));
                }
                ClipPath::Polygon(out)
            } else if t < 0.5 {
                from.clone()
            } else {
                to.clone()
            }
        }
        _ => {
            if t < 0.5 {
                from.clone()
            } else {
                to.clone()
            }
        }
    }
}

