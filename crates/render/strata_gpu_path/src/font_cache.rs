//! Analytical Bézier glyph cache for infinite-resolution font rendering.

use std::collections::HashMap;
use crate::curve::{Point2D, QuadraticBezier, Rect2D};
use crate::path::Path;

/// Analytical vector glyph representation composed of quadratic Bézier contours.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalyticGlyph {
    pub char_code: char,
    pub advance_width: f32,
    pub bounds: Rect2D,
    pub curves: Vec<QuadraticBezier>,
}

/// In-memory cache of parsed vector glyph contours.
#[derive(Debug, Clone, Default)]
pub struct AnalyticGlyphCache {
    glyphs: HashMap<char, AnalyticGlyph>,
}

impl AnalyticGlyphCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, glyph: AnalyticGlyph) {
        self.glyphs.insert(glyph.char_code, glyph);
    }

    pub fn get(&self, char_code: char) -> Option<&AnalyticGlyph> {
        self.glyphs.get(&char_code)
    }

    /// Registers a vector glyph from a `Path`.
    pub fn register_path_glyph(&mut self, char_code: char, advance_width: f32, path: &Path, tolerance: f32) {
        let curves = path.to_quadratics(tolerance);
        let bounds = path.bounding_box().unwrap_or(Rect2D::new(0.0, 0.0, advance_width, 1.0));

        self.insert(AnalyticGlyph {
            char_code,
            advance_width,
            bounds,
            curves,
        });
    }

    /// Evaluates signed distance from a query pixel to the analytical glyph contours.
    pub fn evaluate_glyph_sdf(&self, char_code: char, point: Point2D) -> Option<f32> {
        let glyph = self.get(char_code)?;
        if glyph.curves.is_empty() {
            return None;
        }

        let mut min_dist = f32::INFINITY;
        for c in &glyph.curves {
            let d = crate::sdf::distance_to_quadratic(point, c);
            if d < min_dist {
                min_dist = d;
            }
        }

        let inside = crate::sdf::is_point_inside_closed_path(point, &glyph.curves);
        let signed_dist = if inside { -min_dist } else { min_dist };
        Some(signed_dist)
    }

    /// Returns an AnalyticGlyphCache pre-populated with analytical quadratic contours for ASCII.
    pub fn with_standard_ascii() -> Self {
        let mut cache = Self::new();
        for c in ' '..='~' {
            let (w, curves) = generate_ascii_glyph_curves(c);
            let bounds = if curves.is_empty() {
                Rect2D::new(0.0, 0.0, w, 1.0)
            } else {
                let mut min_x = f32::INFINITY;
                let mut min_y = f32::INFINITY;
                let mut max_x = f32::NEG_INFINITY;
                let mut max_y = f32::NEG_INFINITY;
                for q in &curves {
                    let b = q.bounding_box();
                    min_x = min_x.min(b.min_x);
                    min_y = min_y.min(b.min_y);
                    max_x = max_x.max(b.max_x);
                    max_y = max_y.max(b.max_y);
                }
                Rect2D::new(min_x, min_y, max_x, max_y)
            };
            cache.insert(AnalyticGlyph {
                char_code: c,
                advance_width: w,
                bounds,
                curves,
            });
        }
        cache
    }
}

fn line_segment(x0: f32, y0: f32, x1: f32, y1: f32) -> QuadraticBezier {
    QuadraticBezier::new(
        Point2D::new(x0, y0),
        Point2D::new((x0 + x1) * 0.5, (y0 + y1) * 0.5),
        Point2D::new(x1, y1),
    )
}

fn quad_arc(x0: f32, y0: f32, cx: f32, cy: f32, x1: f32, y1: f32) -> QuadraticBezier {
    QuadraticBezier::new(Point2D::new(x0, y0), Point2D::new(cx, cy), Point2D::new(x1, y1))
}

fn generate_ascii_glyph_curves(c: char) -> (f32, Vec<QuadraticBezier>) {
    match c {
        ' ' => (0.3, Vec::new()),
        '.' => {
            let curves = vec![
                line_segment(0.2, 0.0, 0.35, 0.0),
                line_segment(0.35, 0.0, 0.35, 0.15),
                line_segment(0.35, 0.15, 0.2, 0.15),
                line_segment(0.2, 0.15, 0.2, 0.0),
            ];
            (0.5, curves)
        }
        ',' => {
            let curves = vec![
                line_segment(0.2, -0.1, 0.35, 0.0),
                line_segment(0.35, 0.0, 0.35, 0.15),
                line_segment(0.35, 0.15, 0.2, 0.15),
                line_segment(0.2, 0.15, 0.2, -0.1),
            ];
            (0.5, curves)
        }
        '-' => {
            let curves = vec![
                line_segment(0.1, 0.4, 0.45, 0.4),
                line_segment(0.45, 0.4, 0.45, 0.5),
                line_segment(0.45, 0.5, 0.1, 0.5),
                line_segment(0.1, 0.5, 0.1, 0.4),
            ];
            (0.55, curves)
        }
        '_' => {
            let curves = vec![
                line_segment(0.0, -0.15, 0.55, -0.15),
                line_segment(0.55, -0.15, 0.55, -0.05),
                line_segment(0.55, -0.05, 0.0, -0.05),
                line_segment(0.0, -0.05, 0.0, -0.15),
            ];
            (0.55, curves)
        }
        'I' | 'l' | '1' | '|' => {
            let curves = vec![
                line_segment(0.1, 0.0, 0.25, 0.0),
                line_segment(0.25, 0.0, 0.25, 0.8),
                line_segment(0.25, 0.8, 0.1, 0.8),
                line_segment(0.1, 0.8, 0.1, 0.0),
            ];
            (0.35, curves)
        }
        'A' => {
            let curves = vec![
                line_segment(0.05, 0.0, 0.3, 0.8),
                line_segment(0.3, 0.8, 0.55, 0.0),
                line_segment(0.55, 0.0, 0.42, 0.0),
                line_segment(0.42, 0.0, 0.36, 0.22),
                line_segment(0.36, 0.22, 0.24, 0.22),
                line_segment(0.24, 0.22, 0.18, 0.0),
                line_segment(0.18, 0.0, 0.05, 0.0),
                line_segment(0.3, 0.62, 0.34, 0.35),
                line_segment(0.34, 0.35, 0.26, 0.35),
                line_segment(0.26, 0.35, 0.3, 0.62),
            ];
            (0.6, curves)
        }
        'O' | '0' => {
            let curves = vec![
                quad_arc(0.05, 0.4, 0.05, 0.8, 0.32, 0.8),
                quad_arc(0.32, 0.8, 0.6, 0.8, 0.6, 0.4),
                quad_arc(0.6, 0.4, 0.6, 0.0, 0.32, 0.0),
                quad_arc(0.32, 0.0, 0.05, 0.0, 0.05, 0.4),
                quad_arc(0.18, 0.4, 0.18, 0.68, 0.32, 0.68),
                quad_arc(0.32, 0.68, 0.47, 0.68, 0.47, 0.4),
                quad_arc(0.47, 0.4, 0.47, 0.12, 0.32, 0.12),
                quad_arc(0.32, 0.12, 0.18, 0.12, 0.18, 0.4),
            ];
            (0.65, curves)
        }
        'C' => {
            let curves = vec![
                line_segment(0.55, 0.65, 0.45, 0.75),
                quad_arc(0.45, 0.75, 0.05, 0.75, 0.05, 0.4),
                quad_arc(0.05, 0.4, 0.05, 0.05, 0.45, 0.05),
                line_segment(0.45, 0.05, 0.55, 0.15),
                line_segment(0.55, 0.15, 0.45, 0.2),
                quad_arc(0.45, 0.2, 0.18, 0.2, 0.18, 0.4),
                quad_arc(0.18, 0.4, 0.18, 0.6, 0.45, 0.6),
                line_segment(0.45, 0.6, 0.55, 0.65),
            ];
            (0.6, curves)
        }
        'H' => {
            let curves = vec![
                line_segment(0.08, 0.0, 0.2, 0.0),
                line_segment(0.2, 0.0, 0.2, 0.32),
                line_segment(0.2, 0.32, 0.42, 0.32),
                line_segment(0.42, 0.32, 0.42, 0.0),
                line_segment(0.42, 0.0, 0.54, 0.0),
                line_segment(0.54, 0.0, 0.54, 0.8),
                line_segment(0.54, 0.8, 0.42, 0.8),
                line_segment(0.42, 0.8, 0.42, 0.48),
                line_segment(0.42, 0.48, 0.2, 0.48),
                line_segment(0.2, 0.48, 0.2, 0.8),
                line_segment(0.2, 0.8, 0.08, 0.8),
                line_segment(0.08, 0.8, 0.08, 0.0),
            ];
            (0.62, curves)
        }
        'T' => {
            let curves = vec![
                line_segment(0.24, 0.0, 0.36, 0.0),
                line_segment(0.36, 0.0, 0.36, 0.68),
                line_segment(0.36, 0.68, 0.55, 0.68),
                line_segment(0.55, 0.68, 0.55, 0.8),
                line_segment(0.55, 0.8, 0.05, 0.8),
                line_segment(0.05, 0.8, 0.05, 0.68),
                line_segment(0.05, 0.68, 0.24, 0.68),
                line_segment(0.24, 0.68, 0.24, 0.0),
            ];
            (0.6, curves)
        }
        'L' => {
            let curves = vec![
                line_segment(0.08, 0.0, 0.5, 0.0),
                line_segment(0.5, 0.0, 0.5, 0.12),
                line_segment(0.5, 0.12, 0.2, 0.12),
                line_segment(0.2, 0.12, 0.2, 0.8),
                line_segment(0.2, 0.8, 0.08, 0.8),
                line_segment(0.08, 0.8, 0.08, 0.0),
            ];
            (0.55, curves)
        }
        'E' => {
            let curves = vec![
                line_segment(0.08, 0.0, 0.5, 0.0),
                line_segment(0.5, 0.0, 0.5, 0.12),
                line_segment(0.5, 0.12, 0.2, 0.12),
                line_segment(0.2, 0.12, 0.2, 0.34),
                line_segment(0.2, 0.34, 0.45, 0.34),
                line_segment(0.45, 0.34, 0.45, 0.46),
                line_segment(0.45, 0.46, 0.2, 0.46),
                line_segment(0.2, 0.46, 0.2, 0.68),
                line_segment(0.2, 0.68, 0.5, 0.68),
                line_segment(0.5, 0.68, 0.5, 0.8),
                line_segment(0.5, 0.8, 0.08, 0.8),
                line_segment(0.08, 0.8, 0.08, 0.0),
            ];
            (0.55, curves)
        }
        'F' => {
            let curves = vec![
                line_segment(0.08, 0.0, 0.2, 0.0),
                line_segment(0.2, 0.0, 0.2, 0.34),
                line_segment(0.2, 0.34, 0.45, 0.34),
                line_segment(0.45, 0.34, 0.45, 0.46),
                line_segment(0.45, 0.46, 0.2, 0.46),
                line_segment(0.2, 0.46, 0.2, 0.68),
                line_segment(0.2, 0.68, 0.5, 0.68),
                line_segment(0.5, 0.68, 0.5, 0.8),
                line_segment(0.5, 0.8, 0.08, 0.8),
                line_segment(0.08, 0.8, 0.08, 0.0),
            ];
            (0.55, curves)
        }
        _ => {
            let val = (c as u32 % 7) as f32 * 0.02;
            let curves = vec![
                line_segment(0.08, 0.0, 0.52 - val, 0.0),
                line_segment(0.52 - val, 0.0, 0.52 - val, 0.75),
                line_segment(0.52 - val, 0.75, 0.08, 0.75),
                line_segment(0.08, 0.75, 0.08, 0.0),
                line_segment(0.18, 0.12, 0.42 - val, 0.12),
                line_segment(0.42 - val, 0.12, 0.42 - val, 0.63),
                line_segment(0.42 - val, 0.63, 0.18, 0.63),
                line_segment(0.18, 0.63, 0.18, 0.12),
            ];
            (0.58 - val, curves)
        }
    }
}
