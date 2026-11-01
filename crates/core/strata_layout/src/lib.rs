//! Strata Layout: W3C Spec-compliant, Chromium-grade Box Model, Flexbox, and Grid solvers.

pub mod geometry;
pub mod box_model;
pub mod flexbox;
pub mod grid;
pub mod positioning;
pub mod text;
pub mod tree;
pub mod typography;
pub mod intrinsic;
pub mod wpt;

pub use geometry::{LayoutGeometry, Rect};
pub use box_model::{clamp_dimension, collapse_margins, resolve_dimension};
pub use tree::LayoutNode;
pub use text::{layout_inline_text, measure_text_bounds};
pub use intrinsic::{compute_min_max_sizes, MinMaxSizes};
pub use typography::{
    BidiClass, BidiEngine, BidiRun, CssStrut, FontHeight, FontMetrics,
    InlineFormattingContext, InlineLayoutOptions, InlineLayoutResult, LineBox, LineBreaker,
    ShapedGlyph, TextLine, TextShaper, VerticalAlignment, WhiteSpaceMode,
};
pub use wpt::{WptSuite, WptTestVector};

pub mod prelude {
    pub use crate::geometry::{LayoutGeometry, Rect};
    pub use crate::box_model::collapse_margins;
    pub use crate::tree::LayoutNode;
    pub use crate::text::{layout_inline_text, measure_text_bounds};
    pub use crate::intrinsic::{compute_min_max_sizes, MinMaxSizes};
    pub use crate::typography::*;
    pub use crate::wpt::{WptSuite, WptTestVector};
}

