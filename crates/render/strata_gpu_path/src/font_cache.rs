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
#[derive(Debug, Default)]
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
}
