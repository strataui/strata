//! Strata Layout: W3C Spec-compliant, Chromium-grade Box Model, Flexbox, and Grid solvers.
//!
//! # Examples
//!
//! ```rust
//! use strata_layout::tree::LayoutNode;
//! use strata_style::types::ComputedStyle;
//!
//! let mut root = LayoutNode::new(ComputedStyle::default());
//! root.compute_layout(1024.0, 768.0);
//! assert_eq!(root.geometry.border_box.width, 1024.0);
//! ```

#![forbid(unsafe_code)]

pub mod box_model;
pub mod exclusions;
pub mod flexbox;
pub mod geometry;
pub mod grid;
pub mod intrinsic;
pub mod positioning;
pub mod prof;
pub mod table;
pub mod text;
pub mod tree;
pub mod typography;
pub mod wpt;

pub use box_model::{
    clamp_dimension, clamp_dimension_with_node, collapse_margins, resolve_dimension,
    resolve_dimension_with_node,
};
pub use exclusions::{ExclusionKind, ExclusionRect, ExclusionSpace, LineOpportunity};
pub use geometry::{LayoutGeometry, Point, Rect, Size};
pub use intrinsic::{compute_min_max_sizes, MinMaxSizes};
pub use positioning::{
    solve_out_of_flow_box, solve_positioning, solve_positioning_children, ContainingBlock,
};
pub use table::{
    border_style_priority, resolve_collapsed_border, solve_table, CollapsedBorderEdge,
    ElementSource, TableBorderGrid,
};
pub use text::{
    is_monospace_family, layout_inline_text, measure_text_bounds, measure_text_bounds_ext,
    measure_text_bounds_styled_with_family, measure_text_bounds_with_family,
    measure_text_bounds_with_white_space,
};
pub use tree::{
    clear_layout_overflow, last_layout_overflow, record_layout_overflow, set_on_layout_overflow,
    solve_anchored_position, AnchorPlacement, FillRule, ImageResource, LayoutDepthGuard,
    LayoutNode, PortalAnchorSpec, StrokeLineCap, StrokeLineJoin, SvgItem, ViewportSceneHandle,
    MAX_LAYOUT_DEPTH,
};
pub use typography::{
    apply_small_caps, bidi_mirror, can_break_between_chars, classify_line_break, is_prohibited_end,
    is_prohibited_start, AtomicFragment, BidiClass, BidiEngine, BidiRun, CssStrut, FontDatabase,
    FontHeight, FontMetrics, InlineFormattingContext, InlineItem, InlineLayoutOptions,
    InlineLayoutResult, InlineTextStyle, LineBox, LineBreakClass, LineBreaker, OpenTypeFeatures,
    OpenTypeFont, OpenTypeParseError, ShapedGlyph, TextFragment, TextLine, TextShaper,
    VerticalAlignment, WhiteSpaceMode,
};
pub use wpt::{
    InternalLayoutRegressionSuite, SpecificationConformanceVector, WptSuite, WptTestVector,
};

pub mod prelude {
    pub use crate::box_model::collapse_margins;
    pub use crate::geometry::{LayoutGeometry, Point, Rect, Size};
    pub use crate::intrinsic::{compute_min_max_sizes, MinMaxSizes};
    pub use crate::text::{
        is_monospace_family, layout_inline_text, measure_text_bounds, measure_text_bounds_ext,
        measure_text_bounds_styled_with_family, measure_text_bounds_with_family,
    };
    pub use crate::tree::{solve_anchored_position, AnchorPlacement, LayoutNode, PortalAnchorSpec};
    pub use crate::typography::*;
    pub use crate::wpt::{
        InternalLayoutRegressionSuite, SpecificationConformanceVector, WptSuite, WptTestVector,
    };
}
