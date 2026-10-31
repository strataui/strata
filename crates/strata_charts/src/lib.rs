//! Strata Charts: Declarative SVG & GPU Data Visualization Engine.
//!
//! Provides a pure-Rust, zero-DOM equivalent to Recharts and Visx.
//! Features continuous linear and categorical band scales, SVG line/area/bar generators,
//! and responsive grid coordinate mapping.

pub mod path;
pub mod scale;

pub use path::{area_path, bar_rects, line_path};
pub use scale::{ScaleBand, ScaleLinear};

pub mod prelude {
    pub use crate::path::{area_path, bar_rects, line_path};
    pub use crate::scale::{ScaleBand, ScaleLinear};
}
