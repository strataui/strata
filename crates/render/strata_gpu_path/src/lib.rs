//! Slug and Vello-tier analytical GPU Bézier curve and vector path engine for Project Strata.

pub mod buffer;
pub mod compute;
pub mod curve;
pub mod font_cache;
pub mod font_shader;
pub mod path;
pub mod sdf;
pub mod shaders;
pub mod stroke;

pub use buffer::{GpuCurveInstance, PathCommandBuffer};
pub use compute::{
    stroke_style_flags, CurveComputeDispatcher, GpuPathSegment, GpuPathUniforms, RawCurveInput,
    FLAG_CAP_ROUND, FLAG_CAP_SQUARE, FLAG_CLOSED, FLAG_END_OPEN, FLAG_EVEN_ODD, FLAG_FILL,
    FLAG_JOIN_BEVEL, FLAG_JOIN_ROUND, FLAG_START_OPEN, FLAG_STROKE, FLAG_TRIANGLE,
};
pub use curve::{CubicBezier, Point2D, QuadraticBezier, Rect2D};
pub use font_cache::{AnalyticGlyph, AnalyticGlyphCache};
pub use font_shader::{
    evaluate_slug_analytical_coverage, solve_quadratic_roots_f32, GpuGlyphCurve, GpuGlyphInstance,
    SlugGlyphBuffer, MAX_GLYPH_CURVES, MAX_GLYPH_INSTANCES, WGSL_SLUG_FONT_SHADER,
};

pub use path::{FillRule, Path, PathBuilder, PathVerb, StrokeStyle};
pub use sdf::{
    distance_to_quadratic, is_point_inside_closed_path, is_point_inside_closed_path_rule,
    solve_cubic_roots,
};
pub use shaders::{
    WGSL_ANALYTICAL_PATH_RASTER_SHADER, WGSL_CONCENTRIC_FILLET_CORNER_SHADER,
    WGSL_PATH_COMPUTE_FLATTEN_SHADER,
};
pub use stroke::{
    dash_quadratics, dash_quadratics_with_offset, expand_stroke_quadratics, offset_quadratic,
    quadratic_subcurve, unit_normal, LineCap, LineJoin, StrokeOptions,
};
