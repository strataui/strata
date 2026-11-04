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
pub mod exclusions;
pub mod table;
pub mod wpt;

pub use geometry::{LayoutGeometry, Rect};
pub use box_model::{clamp_dimension, collapse_margins, resolve_dimension};
pub use positioning::{ContainingBlock, solve_positioning, solve_positioning_children, solve_out_of_flow_box};
pub use table::solve_table;
pub use tree::{FillRule, LayoutNode, StrokeLineCap, StrokeLineJoin, SvgItem};
pub use text::{is_monospace_family, layout_inline_text, measure_text_bounds, measure_text_bounds_ext};
pub use intrinsic::{compute_min_max_sizes, MinMaxSizes};
pub use exclusions::{ExclusionKind, ExclusionRect, ExclusionSpace, LineOpportunity};
pub use typography::{
    AtomicFragment, BidiClass, BidiEngine, BidiRun, CssStrut, FontHeight, FontMetrics,
    InlineFormattingContext, InlineItem, InlineLayoutOptions, InlineLayoutResult, InlineTextStyle,
    LineBox, LineBreaker, ShapedGlyph, TextFragment, TextLine, TextShaper, VerticalAlignment,
    WhiteSpaceMode,
};
pub use wpt::{WptSuite, WptTestVector};

pub mod prelude {
    pub use crate::geometry::{LayoutGeometry, Rect};
    pub use crate::box_model::collapse_margins;
    pub use crate::tree::LayoutNode;
    pub use crate::text::{is_monospace_family, layout_inline_text, measure_text_bounds, measure_text_bounds_ext};
    pub use crate::intrinsic::{compute_min_max_sizes, MinMaxSizes};
    pub use crate::typography::*;
    pub use crate::wpt::{WptSuite, WptTestVector};
}

