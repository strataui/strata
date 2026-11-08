//! High-precision OpenType typography, text shaping, BiDi UAX#9, and Inline Formatting Context.

pub mod bidi;
pub mod font_metrics;
pub mod inline_context;
pub mod line_breaker;
pub mod opentype;
pub mod shaper;

pub use bidi::{bidi_mirror, BidiClass, BidiEngine, BidiRun};
pub use font_metrics::{compute_half_leading, CssStrut, FontHeight, FontMetrics};
pub use inline_context::{
    AtomicFragment, InlineFormattingContext, InlineItem, InlineLayoutOptions, InlineLayoutResult,
    InlineTextStyle, LineBox, TextFragment, VerticalAlignment,
};
pub use line_breaker::{
    can_break_between_chars, classify_line_break, is_prohibited_end, is_prohibited_start,
    LineBreakClass, LineBreaker, TextLine, WhiteSpaceMode,
};
pub use opentype::{
    font_database, lookup_font_for_style, FontDatabase, OpenTypeFont, OpenTypeParseError,
};
pub use shaper::{apply_small_caps, OpenTypeFeatures, ShapedGlyph, TextShaper};
