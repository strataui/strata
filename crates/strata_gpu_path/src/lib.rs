//! Slug and Vello-tier analytical GPU Bézier curve and vector path engine for Project Strata.

pub mod buffer;
pub mod curve;
pub mod font_cache;
pub mod path;
pub mod sdf;

pub use buffer::{GpuCurveInstance, PathCommandBuffer};
pub use curve::{CubicBezier, Point2D, QuadraticBezier, Rect2D};
pub use font_cache::{AnalyticGlyph, AnalyticGlyphCache};
pub use path::{FillRule, Path, PathBuilder, PathVerb, StrokeStyle};
pub use sdf::{distance_to_quadratic, is_point_inside_closed_path, solve_cubic_roots};
