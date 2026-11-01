//! Intrinsic text dimension measurement, OpenType shaping, and line breaking facade.

use crate::typography::{
    InlineFormattingContext, InlineLayoutOptions, InlineLayoutResult, WhiteSpaceMode,
    VerticalAlignment,
};

/// Measures intrinsic text bounds (width, height) using subpixel typography and line breaking.
pub fn measure_text_bounds(
    text: &str,
    font_size: f32,
    line_height_multiplier: f32,
    max_width: Option<f32>,
) -> (f32, f32) {
    if text.is_empty() {
        return (0.0, font_size * line_height_multiplier);
    }

    let ifc = InlineFormattingContext::new();
    let explicit_lh = font_size * line_height_multiplier;
    let options = InlineLayoutOptions {
        font_size,
        line_height: Some(explicit_lh),
        max_width,
        white_space: WhiteSpaceMode::Normal,
        vertical_align: VerticalAlignment::Baseline,
        is_monospace: false,
    };

    let result = ifc.layout_text(text, &options);
    let single_line_height = font_size * line_height_multiplier;
    let total_height = result.line_count.max(1) as f32 * single_line_height;
    (result.total_width, total_height)
}

/// Advanced text layout returning detailed line boxes, glyph runs, baselines, and geometry.
pub fn layout_inline_text(text: &str, options: &InlineLayoutOptions) -> InlineLayoutResult {
    let ifc = InlineFormattingContext::new();
    ifc.layout_text(text, options)
}
