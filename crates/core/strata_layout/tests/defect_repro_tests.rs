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

#[test]
fn bug1_nested_percentage_flex_grid_and_table_do_not_square_percentages() {
    use strata_style::types::GridTrack;

    // 1440px row: [24% left pane] + [4px resizer] + [76% right split: 70% + 4px + 30%]
    let mut root_style = ComputedStyle::default();
    root_style.display = Display::Flex;
    root_style.flex_direction = FlexDirection::Row;
    root_style.width = Dimension::Px(1440.0);
    root_style.height = Dimension::Px(900.0);

    let mut left_pane_style = ComputedStyle::default();
    left_pane_style.display = Display::Flex;
    left_pane_style.flex_direction = FlexDirection::Column;
    left_pane_style.width = Dimension::Percent(24.0);
    left_pane_style.height = Dimension::Percent(100.0);

    let mut full_child_style = ComputedStyle::default();
    full_child_style.display = Display::Block;
    full_child_style.width = Dimension::Percent(100.0);
    full_child_style.height = Dimension::Px(40.0);

    let mut resizer_style = ComputedStyle::default();
    resizer_style.width = Dimension::Px(4.0);
    resizer_style.flex_shrink = 0.0;
    resizer_style.height = Dimension::Percent(100.0);

    let mut right_split_style = ComputedStyle::default();
    right_split_style.display = Display::Flex;
    right_split_style.flex_direction = FlexDirection::Row;
    right_split_style.width = Dimension::Percent(76.0);
    right_split_style.height = Dimension::Percent(100.0);

    let mut center_pane_style = ComputedStyle::default();
    center_pane_style.display = Display::Grid;
    center_pane_style.grid_template_columns = vec![GridTrack::Fr(1.0)];
    center_pane_style.width = Dimension::Percent(70.0);
    center_pane_style.height = Dimension::Percent(100.0);

    let mut inspector_pane_style = ComputedStyle::default();
    inspector_pane_style.display = Display::Flex;
    inspector_pane_style.flex_direction = FlexDirection::Column;
    inspector_pane_style.width = Dimension::Percent(30.0);
    inspector_pane_style.height = Dimension::Percent(100.0);

    let left_pane = LayoutNode::with_children(left_pane_style, vec![LayoutNode::new(full_child_style.clone())]);
    let center_pane = LayoutNode::with_children(center_pane_style, vec![LayoutNode::new(full_child_style.clone())]);
    let inspector_pane = LayoutNode::with_children(inspector_pane_style, vec![LayoutNode::new(full_child_style)]);

    let right_split = LayoutNode::with_children(
        right_split_style,
        vec![
            center_pane,
            LayoutNode::new(resizer_style.clone()),
            inspector_pane,
        ],
    );

    let mut root = LayoutNode::with_children(
        root_style,
        vec![left_pane, LayoutNode::new(resizer_style), right_split],
    );
    root.compute_layout(1440.0, 900.0);

    let left = &root.children[0];
    let right = &root.children[2];
    let center = &right.children[0];
    let inspector = &right.children[2];

    // Left pane is ~344.6px (rounded to 345px); its 100%-width child must match left pane width (not 82.7px)
    assert!((left.geometry.border_box.width - 345.0).abs() <= 2.0);
    assert!(
        (left.children[0].geometry.border_box.width - left.geometry.border_box.width).abs() <= 1.0,
        "left child width {} != left pane width {}",
        left.children[0].geometry.border_box.width,
        left.geometry.border_box.width
    );
    assert_eq!(left.style.width, Dimension::Percent(24.0));

    // Nested 76% -> 70% Grid pane (~764px) and 76% -> 30% Flex pane (~327px) must lay out children at full pane width
    assert!((center.geometry.border_box.width - 764.0).abs() <= 3.0);
    assert!(
        (center.children[0].geometry.border_box.width - center.geometry.border_box.width).abs() <= 1.0,
        "center grid child width {} != center pane width {}",
        center.children[0].geometry.border_box.width,
        center.geometry.border_box.width
    );
    assert!((inspector.geometry.border_box.width - 327.0).abs() <= 3.0);
    assert!(
        (inspector.children[0].geometry.border_box.width - inspector.geometry.border_box.width).abs() <= 1.0,
        "inspector child width {} != inspector pane width {}",
        inspector.children[0].geometry.border_box.width,
        inspector.geometry.border_box.width
    );
}

#[test]
fn bug2_column_flex1_propagates_grown_height_and_invalidates_zero_height_cache() {
    let mut outer_style = ComputedStyle::default();
    outer_style.display = Display::Flex;
    outer_style.flex_direction = FlexDirection::Column;
    outer_style.width = Dimension::Px(1440.0);
    outer_style.height = Dimension::Percent(100.0);

    let mut header_style = ComputedStyle::default();
    header_style.width = Dimension::Percent(100.0);
    header_style.height = Dimension::Px(30.0);
    header_style.flex_shrink = 0.0;

    // Middle container uses flex-1 (height: Auto, flex_grow: 1, flex_basis: 0%) WITHOUT h-full
    let mut middle_style = ComputedStyle::default();
    middle_style.display = Display::Flex;
    middle_style.flex_direction = FlexDirection::Column;
    middle_style.width = Dimension::Percent(100.0);
    middle_style.height = Dimension::Auto;
    middle_style.flex_grow = 1.0;
    middle_style.flex_shrink = 1.0;
    middle_style.flex_basis = Dimension::Percent(0.0);

    let mut inner_flex1_style = ComputedStyle::default();
    inner_flex1_style.display = Display::Block;
    inner_flex1_style.width = Dimension::Percent(100.0);
    inner_flex1_style.height = Dimension::Auto;
    inner_flex1_style.flex_grow = 1.0;
    inner_flex1_style.flex_shrink = 1.0;
    inner_flex1_style.flex_basis = Dimension::Percent(0.0);

    let middle = LayoutNode::with_children(middle_style, vec![LayoutNode::new(inner_flex1_style)]);
    let mut outer = LayoutNode::with_children(
        outer_style,
        vec![LayoutNode::new(header_style), middle],
    );

    outer.compute_layout(1440.0, 900.0);
    assert!((outer.children[1].geometry.border_box.height - 870.0).abs() <= 1.0);
    assert!(
        (outer.children[1].children[0].geometry.border_box.height - 870.0).abs() <= 1.0,
        "inner flex-1 child height {} did not expand to 870px",
        outer.children[1].children[0].geometry.border_box.height
    );

    // Resize vertically from 900 -> 700: cache must not serve stale 870px height
    outer.compute_layout(1440.0, 700.0);
    assert!((outer.children[1].geometry.border_box.height - 670.0).abs() <= 1.0);
    assert!(
        (outer.children[1].children[0].geometry.border_box.height - 670.0).abs() <= 1.0,
        "inner flex-1 child height {} did not update to 670px on resize",
        outer.children[1].children[0].geometry.border_box.height
    );
}

#[test]
fn bug4_flex_children_never_permanently_mutate_to_inline_flex() {
    // Card inside a non-stretching flex-col (items-start) has a 400px header and an auto-width
    // `display: flex; justify-content: space-between` footer row without `w-full`.
    let mut outer_col = ComputedStyle::default();
    outer_col.display = Display::Flex;
    outer_col.flex_direction = FlexDirection::Column;
    outer_col.align_items = AlignItems::FlexStart;
    outer_col.width = Dimension::Px(800.0);
    outer_col.height = Dimension::Px(400.0);

    let mut card_style = ComputedStyle::default();
    card_style.display = Display::Flex;
    card_style.flex_direction = FlexDirection::Column;
    card_style.width = Dimension::Auto;

    let mut header_style = ComputedStyle::default();
    header_style.width = Dimension::Px(400.0);
    header_style.height = Dimension::Px(32.0);

    let mut footer_row_style = ComputedStyle::default();
    footer_row_style.display = Display::Flex;
    footer_row_style.flex_direction = FlexDirection::Row;
    footer_row_style.justify_content = JustifyContent::SpaceBetween;
    footer_row_style.width = Dimension::Auto;
    footer_row_style.height = Dimension::Px(28.0);

    let mut pill = ComputedStyle::default();
    pill.width = Dimension::Px(60.0);
    pill.height = Dimension::Px(20.0);

    let footer_row = LayoutNode::with_children(
        footer_row_style,
        vec![LayoutNode::new(pill.clone()), LayoutNode::new(pill)],
    );
    let card = LayoutNode::with_children(card_style, vec![LayoutNode::new(header_style), footer_row]);
    let mut root = LayoutNode::with_children(outer_col, vec![card]);

    root.compute_layout(800.0, 400.0);

    let card_node = &root.children[0];
    let footer_node = &card_node.children[1];
    assert_eq!(card_node.style.display, Display::Flex);
    assert_eq!(footer_node.style.display, Display::Flex);
    assert!((card_node.geometry.border_box.width - 400.0).abs() <= 1.0);
    assert!((footer_node.geometry.border_box.width - 400.0).abs() <= 1.0);
    // Right pill in justify-between footer must sit at 400 - 60 = 340px (not collapsed to 60px)
    assert!(
        (footer_node.children[1].geometry.border_box.x - 340.0).abs() <= 1.0,
        "justify-between right child x {} collapsed instead of reaching 340",
        footer_node.children[1].geometry.border_box.x
    );
}

