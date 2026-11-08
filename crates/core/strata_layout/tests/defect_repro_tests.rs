//! Focused reproductions for observed layout defects.

#![allow(clippy::field_reassign_with_default)]

use strata_layout::LayoutNode;
use strata_style::types::{
    AlignItems, ComputedStyle, Dimension, Display, FlexDirection, JustifyContent, Overflow,
    Position, TextOverflow, WhiteSpace,
};

fn text_style() -> ComputedStyle {
    let mut s = ComputedStyle::default();
    s.display = Display::Inline;
    s
}

#[test]
fn s1_auto_width_flex_container_fills_available_despite_zero_shrink() {
    // A block-level flex panel with auto width and flex-shrink: 0 must still
    // fill the available width. flex-shrink only describes how the panel
    // behaves as an item of its parent; it must not make the panel shrink to
    // fit its own content.
    let mut panel_style = ComputedStyle::default();
    panel_style.display = Display::Flex;
    panel_style.flex_direction = FlexDirection::Column;
    panel_style.width = Dimension::Auto;
    panel_style.flex_shrink = 0.0;
    panel_style.height = Dimension::Px(200.0);

    let mut child_style = ComputedStyle::default();
    child_style.display = Display::Block;
    child_style.width = Dimension::Auto;
    child_style.height = Dimension::Px(20.0);

    let mut panel = LayoutNode::with_children(panel_style, vec![LayoutNode::new(child_style)]);
    panel.compute_layout(600.0, 200.0);

    assert!(
        (panel.geometry.border_box.width - 600.0).abs() < 0.5,
        "auto-width flex panel collapsed to {}",
        panel.geometry.border_box.width
    );
    assert!(
        (panel.children[0].geometry.border_box.width - 600.0).abs() < 0.5,
        "panel child did not stretch: {}",
        panel.children[0].geometry.border_box.width
    );
}

#[test]
fn s1_auto_width_flex_panel_inside_column_fills_row() {
    let mut outer_style = ComputedStyle::default();
    outer_style.display = Display::Flex;
    outer_style.flex_direction = FlexDirection::Column;
    outer_style.width = Dimension::Px(600.0);
    outer_style.height = Dimension::Px(200.0);

    let mut panel_style = ComputedStyle::default();
    panel_style.display = Display::Flex;
    panel_style.flex_direction = FlexDirection::Column;
    panel_style.width = Dimension::Auto;
    panel_style.flex_shrink = 0.0;
    panel_style.height = Dimension::Px(40.0);

    let mut child_style = ComputedStyle::default();
    child_style.display = Display::Block;
    child_style.width = Dimension::Auto;
    child_style.height = Dimension::Px(20.0);

    let panel = LayoutNode::with_children(panel_style, vec![LayoutNode::new(child_style)]);
    let mut outer = LayoutNode::with_children(outer_style, vec![panel]);
    outer.compute_layout(600.0, 200.0);

    let panel = &outer.children[0];
    assert!((panel.geometry.border_box.width - 600.0).abs() < 0.5);
    assert!(
        (panel.children[0].geometry.border_box.width - 600.0).abs() < 0.5,
        "panel child did not stretch: {}",
        panel.children[0].geometry.border_box.width
    );
}

#[test]
fn s2_span_text_in_flex_row_has_nonzero_bounds() {
    let mut row_style = ComputedStyle::default();
    row_style.display = Display::Flex;
    row_style.flex_direction = FlexDirection::Row;
    row_style.align_items = AlignItems::Center;
    row_style.width = Dimension::Px(400.0);
    row_style.height = Dimension::Px(48.0);

    let mut span_style = ComputedStyle::default();
    span_style.display = Display::Inline;
    span_style.width = Dimension::Auto;

    let mut t1 = text_style();
    t1.width = Dimension::Auto;
    let mut t2 = text_style();
    t2.width = Dimension::Auto;
    let span = LayoutNode::with_children(
        span_style,
        vec![
            LayoutNode::with_text(t1, "Hello ".to_string()),
            LayoutNode::with_text(t2, "world".to_string()),
        ],
    );

    let mut row = LayoutNode::with_children(row_style, vec![span]);
    row.compute_layout(400.0, 48.0);

    let span = &row.children[0];
    assert!(span.geometry.border_box.width > 0.0);
    assert!(span.geometry.border_box.height > 0.0);
    for child in &span.children {
        assert!(
            child.geometry.border_box.width > 0.0 && child.geometry.border_box.height > 0.0,
            "text node reported zero bounds: {:?}",
            child.geometry.border_box
        );
    }
}

#[test]
fn s2_span_text_in_block_has_nonzero_bounds() {
    let mut block_style = ComputedStyle::default();
    block_style.display = Display::Block;
    block_style.width = Dimension::Px(400.0);
    block_style.height = Dimension::Auto;

    let mut span_style = ComputedStyle::default();
    span_style.display = Display::Inline;

    let mut t1 = text_style();
    t1.width = Dimension::Auto;
    let mut t2 = text_style();
    t2.width = Dimension::Auto;
    let span = LayoutNode::with_children(
        span_style,
        vec![
            LayoutNode::with_text(t1, "Hello ".to_string()),
            LayoutNode::with_text(t2, "world".to_string()),
        ],
    );

    let mut block = LayoutNode::with_children(block_style, vec![span]);
    block.compute_layout(400.0, 800.0);

    let span = &block.children[0];
    assert!(span.geometry.border_box.width > 0.0);
    for child in &span.children {
        assert!(
            child.geometry.border_box.width > 0.0 && child.geometry.border_box.height > 0.0,
            "text node reported zero bounds: {:?}",
            child.geometry.border_box
        );
    }
}

#[test]
fn s3a_absolute_inset_zero_fills_flex_sized_parent() {
    let mut outer_style = ComputedStyle::default();
    outer_style.display = Display::Flex;
    outer_style.flex_direction = FlexDirection::Column;
    outer_style.width = Dimension::Px(400.0);
    outer_style.height = Dimension::Px(300.0);

    let mut rel_style = ComputedStyle::default();
    rel_style.display = Display::Block;
    rel_style.position = Position::Relative;
    rel_style.flex_grow = 1.0;
    rel_style.flex_basis = Dimension::Percent(0.0);
    rel_style.width = Dimension::Auto;
    rel_style.height = Dimension::Auto;

    let mut abs_style = ComputedStyle::default();
    abs_style.position = Position::Absolute;
    abs_style.insets.top = Dimension::Px(0.0);
    abs_style.insets.right = Dimension::Px(0.0);
    abs_style.insets.bottom = Dimension::Px(0.0);
    abs_style.insets.left = Dimension::Px(0.0);
    abs_style.width = Dimension::Auto;
    abs_style.height = Dimension::Auto;

    let rel = LayoutNode::with_children(rel_style, vec![LayoutNode::new(abs_style)]);
    let mut outer = LayoutNode::with_children(outer_style, vec![rel]);
    outer.compute_layout(400.0, 300.0);

    let rel = &outer.children[0];
    let abs = &rel.children[0];
    assert!((rel.geometry.border_box.height - 300.0).abs() < 0.5);
    assert!(
        (abs.geometry.border_box.height - rel.geometry.border_box.height).abs() < 0.5,
        "absolute inset-0 height {} did not fill containing block {}",
        abs.geometry.border_box.height,
        rel.geometry.border_box.height
    );
}

#[test]
fn s3a_absolute_inset_zero_fills_row_flex_item() {
    let mut outer_style = ComputedStyle::default();
    outer_style.display = Display::Flex;
    outer_style.flex_direction = FlexDirection::Row;
    outer_style.width = Dimension::Px(800.0);
    outer_style.height = Dimension::Px(300.0);

    let mut rel_style = ComputedStyle::default();
    rel_style.display = Display::Flex;
    rel_style.flex_direction = FlexDirection::Column;
    rel_style.position = Position::Relative;
    rel_style.flex_grow = 1.0;
    rel_style.flex_basis = Dimension::Percent(0.0);
    rel_style.width = Dimension::Auto;
    rel_style.height = Dimension::Auto;

    let mut abs_style = ComputedStyle::default();
    abs_style.position = Position::Absolute;
    abs_style.insets.top = Dimension::Px(0.0);
    abs_style.insets.right = Dimension::Px(0.0);
    abs_style.insets.bottom = Dimension::Px(0.0);
    abs_style.insets.left = Dimension::Px(0.0);
    abs_style.width = Dimension::Auto;
    abs_style.height = Dimension::Auto;

    let rel = LayoutNode::with_children(rel_style, vec![LayoutNode::new(abs_style)]);
    let mut outer = LayoutNode::with_children(outer_style, vec![rel]);
    outer.compute_layout(800.0, 300.0);

    let rel = &outer.children[0];
    let abs = &rel.children[0];
    assert!((rel.geometry.border_box.height - 300.0).abs() < 0.5);
    assert!(
        (abs.geometry.border_box.height - rel.geometry.border_box.height).abs() < 0.5,
        "absolute inset-0 height {} did not fill containing block {}",
        abs.geometry.border_box.height,
        rel.geometry.border_box.height
    );
}

#[test]
fn s3b_absolute_static_position_respects_flex_align_items_center() {
    let mut flex_style = ComputedStyle::default();
    flex_style.display = Display::Flex;
    flex_style.flex_direction = FlexDirection::Row;
    flex_style.align_items = AlignItems::Center;
    flex_style.width = Dimension::Px(400.0);
    flex_style.height = Dimension::Px(100.0);

    let mut abs_style = ComputedStyle::default();
    abs_style.position = Position::Absolute;
    abs_style.width = Dimension::Px(50.0);
    abs_style.height = Dimension::Px(20.0);

    let mut flex = LayoutNode::with_children(flex_style, vec![LayoutNode::new(abs_style)]);
    flex.compute_layout(400.0, 100.0);

    let abs = &flex.children[0];
    assert!(
        (abs.geometry.border_box.y - 40.0).abs() < 0.5,
        "static position y {} ignored align-items center",
        abs.geometry.border_box.y
    );
}

#[test]
fn s4_truncate_direct_text_single_line_and_ellipsis() {
    let mut block_style = ComputedStyle::default();
    block_style.display = Display::Block;
    block_style.width = Dimension::Px(100.0);
    block_style.height = Dimension::Auto;
    block_style.overflow_x = Overflow::Hidden;
    block_style.overflow_y = Overflow::Hidden;
    block_style.white_space = WhiteSpace::NoWrap;
    block_style.text_overflow = TextOverflow::Ellipsis;
    block_style.font_size = 16.0;
    block_style.line_height = 1.5;

    let mut block = LayoutNode::with_text(
        block_style,
        "This is a very long string that must be truncated".to_string(),
    );
    block.compute_layout(100.0, 800.0);

    let line_height = 16.0 * 1.5;
    assert!(
        block.geometry.border_box.height <= line_height + 1.0,
        "truncate block height {} exceeds one line {}",
        block.geometry.border_box.height,
        line_height
    );
    assert_eq!(block.line_boxes.len(), 1);
    let frag_text: String = block.line_boxes[0]
        .text_fragments
        .iter()
        .map(|f| f.text.clone())
        .collect();
    assert!(
        frag_text.contains('\u{2026}'),
        "truncated fragment lacks ellipsis: {frag_text:?}"
    );
    let fragment_width: f32 = block.line_boxes[0]
        .text_fragments
        .iter()
        .map(|f| f.bounds.width)
        .sum();
    assert!(fragment_width <= block.geometry.content_box.width + 1.0);
}

#[test]
fn s4_truncate_single_line_and_ellipsis() {
    let mut block_style = ComputedStyle::default();
    block_style.display = Display::Block;
    block_style.width = Dimension::Px(100.0);
    block_style.height = Dimension::Auto;
    block_style.overflow_x = Overflow::Hidden;
    block_style.overflow_y = Overflow::Hidden;
    block_style.white_space = WhiteSpace::NoWrap;
    block_style.text_overflow = TextOverflow::Ellipsis;
    block_style.font_size = 16.0;
    block_style.line_height = 1.5;

    let mut t = text_style();
    t.width = Dimension::Auto;
    t.overflow_x = Overflow::Hidden;
    t.white_space = WhiteSpace::NoWrap;
    t.text_overflow = TextOverflow::Ellipsis;

    let text = LayoutNode::with_text(
        t,
        "This is a very long string that must be truncated".to_string(),
    );
    let mut block = LayoutNode::with_children(block_style, vec![text]);
    block.compute_layout(100.0, 800.0);

    let child = &block.children[0];
    let line_height = 16.0 * 1.5;
    assert!(
        block.geometry.border_box.height <= line_height + 1.0,
        "truncate block height {} exceeds one line {}",
        block.geometry.border_box.height,
        line_height
    );
    let lines = block.line_boxes.len().max(child.line_boxes.len());
    assert_eq!(lines, 1, "truncate produced {} line boxes", lines);
    let frag_text: String = block
        .line_boxes
        .iter()
        .chain(child.line_boxes.iter())
        .flat_map(|l| l.text_fragments.iter())
        .map(|f| f.text.clone())
        .collect();
    assert!(
        frag_text.contains('\u{2026}'),
        "truncated fragment lacks ellipsis: {frag_text:?}"
    );
}

#[test]
fn s3b_absolute_inset_zero_centers_inside_flex_grown_parent() {
    let mut outer_style = ComputedStyle::default();
    outer_style.display = Display::Flex;
    outer_style.flex_direction = FlexDirection::Column;
    outer_style.width = Dimension::Px(800.0);
    outer_style.height = Dimension::Px(600.0);

    let mut grown_style = ComputedStyle::default();
    grown_style.display = Display::Flex;
    grown_style.flex_direction = FlexDirection::Column;
    grown_style.position = Position::Relative;
    grown_style.flex_grow = 1.0;
    grown_style.flex_basis = Dimension::Percent(0.0);

    let mut overlay_style = ComputedStyle::default();
    overlay_style.display = Display::Flex;
    overlay_style.flex_direction = FlexDirection::Row;
    overlay_style.align_items = AlignItems::Center;
    overlay_style.justify_content = JustifyContent::Center;
    overlay_style.position = Position::Absolute;
    overlay_style.insets.top = Dimension::Px(0.0);
    overlay_style.insets.right = Dimension::Px(0.0);
    overlay_style.insets.bottom = Dimension::Px(0.0);
    overlay_style.insets.left = Dimension::Px(0.0);

    let mut child_style = ComputedStyle::default();
    child_style.display = Display::Block;
    child_style.width = Dimension::Px(520.0);
    child_style.height = Dimension::Px(320.0);

    let overlay = LayoutNode::with_children(overlay_style, vec![LayoutNode::new(child_style)]);
    let grown = LayoutNode::with_children(grown_style, vec![overlay]);
    let mut outer = LayoutNode::with_children(outer_style, vec![grown]);
    outer.compute_layout(800.0, 600.0);

    let grown = &outer.children[0];
    let overlay = &grown.children[0];
    assert!((grown.geometry.border_box.height - 600.0).abs() < 0.5);
    assert!((overlay.geometry.border_box.height - 600.0).abs() < 0.5);
    assert!(
        (overlay.children[0].geometry.border_box.y - 140.0).abs() < 0.5,
        "centered child y {} != 140",
        overlay.children[0].geometry.border_box.y
    );
}

#[test]
fn s5_flex_grow_respects_max_width_after_distribution() {
    let mut row_style = ComputedStyle::default();
    row_style.display = Display::Flex;
    row_style.flex_direction = FlexDirection::Row;
    row_style.justify_content = JustifyContent::SpaceBetween;
    row_style.width = Dimension::Px(900.0);

    let mut left_style = ComputedStyle::default();
    left_style.width = Dimension::Px(200.0);
    left_style.height = Dimension::Px(20.0);

    let mut search_style = ComputedStyle::default();
    search_style.flex_grow = 1.0;
    search_style.flex_shrink = 1.0;
    search_style.flex_basis = Dimension::Percent(0.0);
    search_style.max_width = Dimension::Px(448.0);
    search_style.height = Dimension::Px(20.0);

    let mut right_style = ComputedStyle::default();
    right_style.width = Dimension::Px(100.0);
    right_style.height = Dimension::Px(20.0);

    let mut row = LayoutNode::with_children(
        row_style,
        vec![
            LayoutNode::new(left_style),
            LayoutNode::new(search_style),
            LayoutNode::new(right_style),
        ],
    );
    row.compute_layout(1440.0, 900.0);

    assert!((row.children[1].geometry.border_box.width - 448.0).abs() < 0.5);
    assert!((row.children[2].geometry.border_box.x - 800.0).abs() < 0.5);
}

#[test]
fn s4_flex_text_item_truncates_with_ellipsis() {
    let mut container_style = ComputedStyle::default();
    container_style.display = Display::Flex;
    container_style.flex_direction = FlexDirection::Row;
    container_style.width = Dimension::Px(288.0);
    container_style.overflow_x = Overflow::Hidden;
    container_style.overflow_y = Overflow::Hidden;
    container_style.white_space = WhiteSpace::NoWrap;
    container_style.text_overflow = TextOverflow::Ellipsis;
    container_style.font_size = 12.0;
    container_style.line_height = 16.0 / 12.0;

    let mut text = text_style();
    text.width = Dimension::Auto;
    text.font_size = 12.0;
    text.line_height = 16.0 / 12.0;

    let text = LayoutNode::with_text(
        text,
        "a very long label that must not wrap onto several lines".to_string(),
    );
    let mut container = LayoutNode::with_children(container_style, vec![text]);
    container.compute_layout(1440.0, 900.0);

    let text = &container.children[0];
    assert_eq!(text.line_boxes.len(), 1);
    assert!((text.geometry.border_box.height - 16.0).abs() < 0.5);
    let fragment: String = text.line_boxes[0]
        .text_fragments
        .iter()
        .map(|f| f.text.clone())
        .collect();
    assert!(
        fragment.ends_with('\u{2026}'),
        "fragment does not end with ellipsis: {fragment:?}"
    );
    let width: f32 = text.line_boxes[0]
        .text_fragments
        .iter()
        .map(|f| f.bounds.width)
        .sum();
    assert!(width <= 288.0 + 0.5);
}

#[test]
fn s1_cross_stretch_relayouts_children_at_final_content_size() {
    // The panel keeps a stretched border box but its descendants must be
    // formatted for the same content width. Re-running layout at the final
    // width after a narrower pass must leave the inner row at the full width.
    let mut panel_style = ComputedStyle::default();
    panel_style.display = Display::Flex;
    panel_style.flex_direction = FlexDirection::Column;
    panel_style.width = Dimension::Auto;
    panel_style.flex_shrink = 0.0;

    let mut row_style = ComputedStyle::default();
    row_style.display = Display::Flex;
    row_style.flex_direction = FlexDirection::Row;
    row_style.width = Dimension::Auto;
    row_style.height = Dimension::Px(20.0);

    let mut panel = LayoutNode::with_children(panel_style, vec![LayoutNode::new(row_style)]);
    panel.compute_layout(320.0, 900.0);
    assert!((panel.geometry.border_box.width - 320.0).abs() < 0.5);
    assert!((panel.children[0].geometry.border_box.width - 320.0).abs() < 0.5);

    // Narrow pass followed by the final stretched pass.
    panel.compute_layout(126.6, 900.0);
    panel.compute_layout(320.0, 900.0);
    assert!(
        (panel.children[0].geometry.border_box.width - 320.0).abs() < 0.5,
        "inner row retained stale width {}",
        panel.children[0].geometry.border_box.width
    );
}
