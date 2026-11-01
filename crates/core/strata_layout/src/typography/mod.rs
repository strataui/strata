//! High-precision OpenType typography, text shaping, BiDi UAX#9, and Inline Formatting Context.

pub mod bidi;
pub mod font_metrics;
pub mod inline_context;
pub mod line_breaker;
pub mod shaper;

pub use bidi::{BidiClass, BidiEngine, BidiRun};
pub use font_metrics::{compute_half_leading, CssStrut, FontHeight, FontMetrics};
pub use inline_context::{
    InlineFormattingContext, InlineLayoutOptions, InlineLayoutResult, LineBox, VerticalAlignment,
};
pub use line_breaker::{LineBreaker, TextLine, WhiteSpaceMode};
pub use shaper::{ShapedGlyph, TextShaper};
