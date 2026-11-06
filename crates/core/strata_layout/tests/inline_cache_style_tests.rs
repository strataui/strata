//! Regression coverage for the inline layout cache key.
//!
//! `InlineLayoutResult` carries cloned fragments with a full `InlineTextStyle`,
//! so a cache hit is only correct when every output-affecting style field is
//! part of the key. Paint-only differences (color, background color, text
//! decoration, link target) previously collided and reused the first run's
//! styling for the life of the thread.

use strata_layout::typography::{
    InlineFormattingContext, InlineItem, InlineLayoutOptions, InlineLayoutResult, InlineTextStyle,
};
use strata_style::types::{Color, TextDecoration};

fn options() -> InlineLayoutOptions<'static> {
    InlineLayoutOptions {
        font_size: 11.0,
        line_height: Some(16.0),
        max_width: Some(300.0),
        ..Default::default()
    }
}

fn style() -> InlineTextStyle {
    InlineTextStyle {
        font_size: 11.0,
        line_height: Some(16.0),
        ..Default::default()
    }
}

fn layout(text: &str, style: InlineTextStyle) -> InlineLayoutResult {
    let ifc = InlineFormattingContext::new();
    let items = vec![InlineItem::text(text, style)];
    ifc.layout_inline_stream(&items, &options())
}

fn fragment_colors(result: &InlineLayoutResult) -> Vec<Color> {
    result
        .lines
        .iter()
        .flat_map(|line| line.text_fragments.iter())
        .map(|fragment| fragment.style.color)
        .collect()
}

fn first_fragment_style(result: &InlineLayoutResult) -> &InlineTextStyle {
    &result.lines[0].text_fragments[0].style
}

/// Forces the thread-local cache past its eviction limit so the next lookup is
/// a guaranteed miss. Keeps the test independent of any clearing hook.
fn evict_cache() {
    let ifc = InlineFormattingContext::new();
    for i in 0..600 {
        let items = vec![InlineItem::text(format!("evict-{i}"), style())];
        let _ = ifc.layout_inline_stream(&items, &options());
    }
}

#[test]
fn distinct_colors_do_not_share_a_cache_entry() {
    let black = Color::new(0, 0, 0, 255);
    let grey = Color::new(0xA3, 0xA3, 0xA3, 255);

    let first = layout(
        "first",
        InlineTextStyle {
            color: black,
            ..style()
        },
    );
    let second = layout(
        "second",
        InlineTextStyle {
            color: grey,
            ..style()
        },
    );

    assert_eq!(fragment_colors(&first), vec![black]);
    assert_eq!(fragment_colors(&second), vec![grey]);

    // The defect is order dependent: re-running in the opposite order must
    // also preserve each run's own color.
    let third = layout(
        "second",
        InlineTextStyle {
            color: grey,
            ..style()
        },
    );
    let fourth = layout(
        "first",
        InlineTextStyle {
            color: black,
            ..style()
        },
    );

    assert_eq!(fragment_colors(&third), vec![grey]);
    assert_eq!(fragment_colors(&fourth), vec![black]);
}

#[test]
fn identical_text_with_different_paint_styles_keeps_each_style() {
    let base = style();

    let plain = layout("Diagnostics", base.clone());
    let background = layout(
        "Diagnostics",
        InlineTextStyle {
            background_color: Some(Color::new(0x1e, 0x29, 0x3b, 255)),
            ..base.clone()
        },
    );
    let decorated = layout(
        "Diagnostics",
        InlineTextStyle {
            text_decoration: TextDecoration::Underline,
            ..base.clone()
        },
    );
    let linked = layout(
        "Diagnostics",
        InlineTextStyle {
            href: Some("https://example.invalid/diagnostics".to_string()),
            ..base.clone()
        },
    );

    assert_eq!(first_fragment_style(&plain).background_color, None);
    assert_eq!(
        first_fragment_style(&background).background_color,
        Some(Color::new(0x1e, 0x29, 0x3b, 255))
    );
    assert_eq!(
        first_fragment_style(&decorated).text_decoration,
        TextDecoration::Underline
    );
    assert_eq!(
        first_fragment_style(&linked).href.as_deref(),
        Some("https://example.invalid/diagnostics")
    );

    // Geometry is identical, so only a key that includes the paint fields can
    // tell these results apart.
    assert_eq!(plain.total_width, background.total_width);
    assert_eq!(plain.total_width, decorated.total_width);
    assert_eq!(plain.total_width, linked.total_width);
    assert_ne!(plain, background);
    assert_ne!(plain, decorated);
    assert_ne!(plain, linked);
}

#[test]
fn cache_hit_matches_a_cleared_cache_miss() {
    let styled = InlineTextStyle {
        color: Color::new(0xF1, 0xF5, 0xF9, 255),
        background_color: Some(Color::new(0x11, 0x18, 0x27, 255)),
        text_decoration: TextDecoration::LineThrough,
        href: Some("https://example.invalid/tab".to_string()),
        ..style()
    };

    evict_cache();
    let miss = layout("Active tab", styled.clone());
    let hit = layout("Active tab", styled.clone());
    evict_cache();
    let cleared_miss = layout("Active tab", styled);

    assert_eq!(hit, miss, "cache hit must serialize like the original miss");
    assert_eq!(
        cleared_miss, miss,
        "a cleared cache must reproduce the cached result exactly"
    );
    assert_eq!(
        format!("{hit:?}"),
        format!("{miss:?}"),
        "fragment styles must survive the hit path"
    );
}
