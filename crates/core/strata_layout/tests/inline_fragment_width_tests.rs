//! Verifies that merged inline fragments advance by the continuously measured
//! width of the merged run rather than the sum of isolated token widths.
//!
#![allow(clippy::field_reassign_with_default, clippy::too_many_arguments)]
//! Pango shapes a merged fragment as one string, so its painted advance is
//! slightly wider than the sum of the per-token measurements. The fragment
//! assembly pass must therefore re-measure the merged text and use that width
//! for both the fragment bounds and the cursor advance of the next fragment.

use strata_layout::typography::{
    InlineFormattingContext, InlineItem, InlineLayoutOptions, InlineTextStyle,
};
use strata_style::types::{FontStyle, FontWeight};

/// Deterministic measurer that models continuous shaping: any run of eight or
/// more characters carries a fixed shaping bonus that no individual token
/// reaches, so the merged advance is strictly wider than the token sum.
fn continuous_probe_measurer(
    text: &str,
    _font_size: f32,
    _line_height_multiplier: f32,
    _max_width: Option<f32>,
    _is_monospace: bool,
    _font_family: strata_primitives::AtomId,
    _font_weight: FontWeight,
    _font_style: FontStyle,
) -> (f32, f32) {
    let chars = text.chars().count() as f32;
    let shaping_bonus = if chars >= 8.0 { 13.0 } else { 0.0 };
    (chars * 10.0 + shaping_bonus, 21.0)
}

#[test]
fn merged_fragment_advances_by_continuous_width() {
    strata_layout::text::register_family_styled_text_measurer(continuous_probe_measurer);

    let style = InlineTextStyle {
        font_size: 18.0,
        ..Default::default()
    };

    // Two items with distinct indices so they form two separate fragments on
    // one line. Hyphens give the tokenizer break opportunities without spaces.
    let items = vec![
        InlineItem::text("alpha-beta-gamma", style.clone()),
        InlineItem::text("delta-epsilon", style),
    ];
    let options = InlineLayoutOptions {
        font_size: 18.0,
        line_height: Some(21.0),
        max_width: None,
        ..Default::default()
    };

    let ifc = InlineFormattingContext::new();
    let result = ifc.layout_inline_stream(&items, &options);

    assert_eq!(result.lines.len(), 1, "the probe stream must fit one line");
    let line = &result.lines[0];
    assert_eq!(
        line.text_fragments.len(),
        2,
        "each item must produce its own fragment"
    );

    let first = &line.text_fragments[0];
    let second = &line.text_fragments[1];

    assert_eq!(first.text, "alpha-beta-gamma");
    assert_eq!(second.text, "delta-epsilon");

    // Token sum for "alpha-beta-gamma" is 60 + 50 + 50 = 160; continuous
    // measurement is 173.
    assert!(
        (first.bounds.width - 173.0).abs() < 0.01,
        "first fragment must use the continuous width, got {}",
        first.bounds.width
    );
    assert!(
        (second.bounds.x - 173.0).abs() < 0.01,
        "second fragment must start after the continuous width, got {}",
        second.bounds.x
    );

    // Token sum for "delta-epsilon" is 60 + 70 = 130; continuous measurement
    // is 143.
    assert!(
        (second.bounds.width - 143.0).abs() < 0.01,
        "second fragment must use the continuous width, got {}",
        second.bounds.width
    );

    // 173 + 143 = 316, not the 290 token-sum total.
    assert!(
        (result.total_width - 316.0).abs() < 0.01,
        "line width must accumulate continuous advances, got {}",
        result.total_width
    );
}
