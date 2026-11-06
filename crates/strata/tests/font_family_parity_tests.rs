//! Chromium-parity regression for CSS `font-family` propagation into layout.
//!
//! `AppRuntime::new` registers the Pango measurement hooks on Linux; this test
//! registers them directly so the layout engine measures with real host font
//! metrics rather than the synthetic shaper.

#![cfg(target_os = "linux")]

use strata::layout::{LayoutNode, LineBox};
use strata_primitives::AtomId;
use strata_style::types::{ComputedStyle, Dimension, Display, FontStyle, FontWeight};

const FONT_SIZE: f32 = 18.0;
const LINE_HEIGHT: f32 = 30.0;
const WIDTH: f32 = 900.0;

fn text_span(text: &str, family: AtomId, weight: FontWeight, style: FontStyle) -> LayoutNode {
    let mut node = LayoutNode::new(ComputedStyle {
        display: Display::Inline,
        font_family: family,
        font_size: FONT_SIZE,
        line_height: LINE_HEIGHT / FONT_SIZE,
        font_weight: weight,
        font_style: style,
        ..Default::default()
    });
    node.text = Some(text.to_string());
    node
}

fn second_line(block: &LayoutNode) -> &LineBox {
    block
        .line_boxes
        .get(1)
        .expect("the paragraph must wrap to at least two lines")
}

#[test]
fn dejavu_sans_paragraph_wraps_like_chromium() {
    strata_platform::init_pango_text_measurement();

    let dejavu = strata_primitives::atom::intern("DejaVu Sans");
    let mut block = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        width: Dimension::Px(WIDTH),
        height: Dimension::Auto,
        font_family: dejavu,
        font_size: FONT_SIZE,
        line_height: LINE_HEIGHT / FONT_SIZE,
        ..Default::default()
    });

    block.add_child(text_span(
        "A light run establishes the baseline.",
        dejavu,
        FontWeight::Light,
        FontStyle::Normal,
    ));
    block.add_child(text_span(
        " A medium run follows on the same line box.",
        dejavu,
        FontWeight::Medium,
        FontStyle::Normal,
    ));
    block.add_child(text_span(
        " A bold run adds emphasis.",
        dejavu,
        FontWeight::Bold,
        FontStyle::Normal,
    ));
    block.add_child(text_span(
        " An italic run changes posture.",
        dejavu,
        FontWeight::Normal,
        FontStyle::Italic,
    ));
    block.add_child(text_span(
        " The paragraph continues with normal weight text so wrapping and leading are exercised across several lines.",
        dejavu,
        FontWeight::Normal,
        FontStyle::Normal,
    ));

    block.compute_layout(WIDTH, 2000.0);

    assert!(
        block.line_boxes.len() >= 3,
        "the fixture paragraph must occupy several line boxes, got {}",
        block.line_boxes.len()
    );

    let line = second_line(&block);
    let last = line
        .text_fragments
        .last()
        .expect("the second line must contain text fragments");

    assert!(
        line.text_fragments
            .iter()
            .any(|fragment| fragment.text.contains("emphasis.")),
        "the bold run must wrap after 'adds' so 'emphasis.' starts line two"
    );
    assert!(
        last.text.ends_with("so "),
        "Chromium wraps the DejaVu Sans paragraph after 'so ', but the last fragment was {:?}",
        last.text
    );
    assert!(
        !line
            .text_fragments
            .iter()
            .any(|fragment| fragment.text.contains("wrapping")),
        "'wrapping' must move to the third line"
    );

    let (ascent, descent, _) = strata_platform::pango_font_metrics(dejavu, FONT_SIZE, false);
    let leading = LINE_HEIGHT - (ascent + descent);
    let expected_baseline = leading / 2.0 + ascent;
    let first = block.line_boxes.first().expect("the first line must exist");
    assert!(
        (first.baseline - expected_baseline).abs() <= 1.0,
        "first-line baseline {} must match Pango half-leading {}",
        first.baseline,
        expected_baseline
    );
}

/// The `inline_text` fixture line must end where a single Pango shaping pass of
/// the concatenated line text ends. Before the fragment assembly re-measured
/// merged runs, each fragment advanced by the sum of its isolated token widths
/// and the line drifted a few pixels short, compounding per fragment.
#[test]
fn continuous_fragment_advances_end_at_the_pango_line_width() {
    strata_platform::init_pango_text_measurement();

    let dejavu = strata_primitives::atom::intern("DejaVu Sans");
    let mut block = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        width: Dimension::Px(WIDTH),
        height: Dimension::Auto,
        font_family: dejavu,
        font_size: FONT_SIZE,
        line_height: LINE_HEIGHT / FONT_SIZE,
        ..Default::default()
    });

    // The opening fixture runs. A uniform weight and posture keep the line
    // directly comparable to one Pango measurement; the multiple child items
    // still force several independent fragments on the same line.
    for text in [
        "A light run establishes the baseline.",
        " A medium run follows on the same line box.",
        " A bold run adds emphasis.",
    ] {
        block.add_child(text_span(
            text,
            dejavu,
            FontWeight::Normal,
            FontStyle::Normal,
        ));
    }

    block.compute_layout(WIDTH, 2000.0);

    let line = block.line_boxes.first().expect("the first line must exist");
    assert!(
        line.text_fragments.len() >= 2,
        "the line must span several fragments, got {}",
        line.text_fragments.len()
    );

    let concatenated: String = line
        .text_fragments
        .iter()
        .map(|fragment| fragment.text.as_str())
        .collect();
    let last = line
        .text_fragments
        .last()
        .expect("the line must contain a text fragment");

    let (measured, _) = strata_platform::pango_measure_styled_text_family(
        &concatenated,
        FONT_SIZE,
        LINE_HEIGHT / FONT_SIZE,
        None,
        false,
        dejavu,
        FontWeight::Normal,
        FontStyle::Normal,
    );
    let expected_right = line.bounds.x + measured;

    assert!(
        (last.bounds.max_x() - expected_right).abs() <= 1.0,
        "line end {} must match the Pango measurement {} of the concatenated text {:?}",
        last.bounds.max_x(),
        expected_right,
        concatenated
    );
}

