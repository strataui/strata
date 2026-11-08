//! Slug and Vello-tier analytical GPU Bézier curve and vector path engine for Project Strata.

pub mod buffer;
pub mod compute;
pub mod curve;
pub mod font_cache;
pub mod path;
pub mod sdf;
pub mod shaders;
pub mod stroke;
pub mod font_shader;

pub use buffer::{GpuCurveInstance, PathCommandBuffer};
pub use compute::{
    CurveComputeDispatcher, GpuPathSegment, GpuPathUniforms, RawCurveInput, FLAG_CLOSED,
    FLAG_EVEN_ODD, FLAG_FILL, FLAG_STROKE,
};
pub use curve::{CubicBezier, Point2D, QuadraticBezier, Rect2D};
pub use font_cache::{AnalyticGlyph, AnalyticGlyphCache};
pub use font_shader::{
    GpuGlyphCurve, GpuGlyphInstance, SlugGlyphBuffer, WGSL_SLUG_FONT_SHADER,
};
pub use path::{FillRule, Path, PathBuilder, PathVerb, StrokeStyle};
pub use sdf::{distance_to_quadratic, is_point_inside_closed_path, solve_cubic_roots};
pub use shaders::{
    WGSL_ANALYTICAL_PATH_RASTER_SHADER, WGSL_CONCENTRIC_FILLET_CORNER_SHADER,
    WGSL_PATH_COMPUTE_FLATTEN_SHADER,
};
pub use stroke::{
    expand_stroke_quadratics, offset_quadratic, unit_normal, LineCap, LineJoin, StrokeOptions,
};

