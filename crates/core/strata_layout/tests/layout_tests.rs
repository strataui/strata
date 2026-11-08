//! Integration Tests: Chromium-grade Box Model, Flexbox, CSS Grid, Positioning, and Subpixel Snapping.

#![allow(clippy::field_reassign_with_default)]

use strata_layout::prelude::*;
use strata_layout::box_model::collapse_margins;
use strata_primitives::geom::EdgeInsets;
use strata_style::types::{
    AlignItems, ComputedStyle, Dimension, Display, FlexDirection, FlexWrap,
    GridTrack, JustifyContent, Position,
};

#[test]
fn test_css_margin_collapsing_rules() {
    // 1. Both positive: max(m1, m2)
    assert_eq!(collapse_margins(20.0, 30.0), 30.0);
    assert_eq!(collapse_margins(50.0, 10.0), 50.0);

    // 2. Both negative: -max(|m1|, |m2|)
    assert_eq!(collapse_margins(-15.0, -25.0), -25.0);
    assert_eq!(collapse_margins(-40.0, -10.0), -40.0);

    // 3. Mixed signs: max(m1, m2, 0) + min(m1, m2, 0)
    assert_eq!(collapse_margins(30.0, -10.0), 20.0);
    assert_eq!(collapse_margins(10.0, -30.0), -20.0);
    assert_eq!(collapse_margins(0.0, -15.0), -15.0);
    assert_eq!(collapse_margins(25.0, 0.0), 25.0);
}

#[test]
fn test_block_layout_sibling_margin_collapsing() {
    // Container with 2 adjacent block children
    let mut root_style = ComputedStyle::default();
    root_style.display = Display::Block;
    root_style.width = Dimension::Px(400.0);
    root_style.height = Dimension::Auto;

    let mut child1_style = ComputedStyle::default();
    child1_style.display = Display::Block;
    child1_style.width = Dimension::Px(200.0);
    child1_style.height = Dimension::Px(100.0);
    child1_style.margin.bottom = 30.0;

    let mut child2_style = ComputedStyle::default();
    child2_style.display = Display::Block;
    child2_style.width = Dimension::Px(200.0);
    child2_style.height = Dimension::Px(80.0);
    child2_style.margin.top = 20.0; // Should collapse with child1 bottom: max(30, 20) = 30

    let child1 = LayoutNode::new(child1_style);
    let child2 = LayoutNode::new(child2_style);
    let mut root = LayoutNode::with_children(root_style, vec![child1, child2]);

    root.compute_layout(400.0, 800.0);

    // Child 1 starts at y = 0, height = 100
    assert_eq!(root.children[0].geometry.border_box.y, 0.0);
    assert_eq!(root.children[0].geometry.border_box.height, 100.0);

    // Child 2 starts at y = 100 + 30 (collapsed margin) = 130 (not 100 + 30 + 20 = 150)
    assert_eq!(root.children[1].geometry.border_box.y, 130.0);
    assert_eq!(root.children[1].geometry.border_box.height, 80.0);
}

#[test]
fn test_flexbox_grow_expansion() {
    // Container width = 300px
    // Item 1: flex-grow = 1, width = 0
    // Item 2: flex-grow = 2, width = 0
    let mut container_style = ComputedStyle::default();
    container_style.display = Display::Flex;
    container_style.flex_direction = FlexDirection::Row;
    container_style.width = Dimension::Px(300.0);
    container_style.height = Dimension::Px(100.0);

    let mut item1_style = ComputedStyle::default();
    item1_style.width = Dimension::Px(0.0);
    item1_style.flex_grow = 1.0;

    let mut item2_style = ComputedStyle::default();
    item2_style.width = Dimension::Px(0.0);
    item2_style.flex_grow = 2.0;

    let mut container = LayoutNode::with_children(
        container_style,
        vec![LayoutNode::new(item1_style), LayoutNode::new(item2_style)],
    );

    container.compute_layout(300.0, 100.0);

    // Item 1: 1/3 of 300 = 100
    // Item 2: 2/3 of 300 = 200
    assert_eq!(container.children[0].geometry.border_box.width, 100.0);
    assert_eq!(container.children[1].geometry.border_box.width, 200.0);
    assert_eq!(container.children[0].geometry.border_box.x, 0.0);
    assert_eq!(container.children[1].geometry.border_box.x, 100.0);
}

#[test]
fn test_flexbox_weighted_shrink_contraction() {
    // Container width = 500px
    // Item 1: width = 400px, flex-shrink = 1.0
    // Item 2: width = 200px, flex-shrink = 2.0
    // Total hypothetical width = 600px, free space = -100px
    // Scaled shrink: item1 = 1.0 * 400 = 400, item2 = 2.0 * 200 = 400, total = 800
    // Item 1 reduction: (400 / 800) * 100 = 50 -> resolved width = 350px
    // Item 2 reduction: (400 / 800) * 100 = 50 -> resolved width = 150px
    let mut container_style = ComputedStyle::default();
    container_style.display = Display::Flex;
    container_style.flex_direction = FlexDirection::Row;
    container_style.width = Dimension::Px(500.0);
    container_style.height = Dimension::Px(100.0);

    let mut item1_style = ComputedStyle::default();
    item1_style.width = Dimension::Px(400.0);
    item1_style.flex_shrink = 1.0;

    let mut item2_style = ComputedStyle::default();
    item2_style.width = Dimension::Px(200.0);
    item2_style.flex_shrink = 2.0;

    let mut container = LayoutNode::with_children(
        container_style,
        vec![LayoutNode::new(item1_style), LayoutNode::new(item2_style)],
    );

    container.compute_layout(500.0, 100.0);

    assert_eq!(container.children[0].geometry.border_box.width, 350.0);
    assert_eq!(container.children[1].geometry.border_box.width, 150.0);
    assert_eq!(
        container.children[0].geometry.border_box.width + container.children[1].geometry.border_box.width,
        500.0
    );
}

#[test]
fn test_flexbox_multi_line_wrapping() {
    // Container width = 200px, flex_wrap = Wrap, gap = 10px
    // Item 1: width = 90px, height = 40px
    // Item 2: width = 90px, height = 40px
    // (Line 1: 90 + 10 + 90 = 190 <= 200)
    // Item 3: width = 90px, height = 40px (Wraps to Line 2)
    let mut container_style = ComputedStyle::default();
    container_style.display = Display::Flex;
    container_style.flex_direction = FlexDirection::Row;
    container_style.flex_wrap = FlexWrap::Wrap;
    container_style.width = Dimension::Px(200.0);
    container_style.gap = 10.0;

    let mut item1 = ComputedStyle::default();
    item1.width = Dimension::Px(90.0);
    item1.height = Dimension::Px(40.0);

    let mut item2 = ComputedStyle::default();
    item2.width = Dimension::Px(90.0);
    item2.height = Dimension::Px(40.0);

    let mut item3 = ComputedStyle::default();
    item3.width = Dimension::Px(90.0);
    item3.height = Dimension::Px(40.0);

    let mut container = LayoutNode::with_children(
        container_style,
        vec![LayoutNode::new(item1), LayoutNode::new(item2), LayoutNode::new(item3)],
    );

    container.compute_layout(200.0, 500.0);

    // Line 1 items: y = 0
    assert_eq!(container.children[0].geometry.border_box.x, 0.0);
    assert_eq!(container.children[0].geometry.border_box.y, 0.0);
    assert_eq!(container.children[1].geometry.border_box.x, 100.0); // 90 + 10 gap
    assert_eq!(container.children[1].geometry.border_box.y, 0.0);

    // Line 2 item: wraps, y = 40 (line 1 height) + 10 (gap) = 50.0
    assert_eq!(container.children[2].geometry.border_box.x, 0.0);
    assert_eq!(container.children[2].geometry.border_box.y, 50.0);
}

#[test]
fn test_flexbox_justification_and_alignment() {
    // Container width = 400px, height = 200px
    // JustifyContent: Center, AlignItems: Center
    let mut container_style = ComputedStyle::default();
    container_style.display = Display::Flex;
    container_style.flex_direction = FlexDirection::Row;
    container_style.justify_content = JustifyContent::Center;
    container_style.align_items = AlignItems::Center;
    container_style.width = Dimension::Px(400.0);
    container_style.height = Dimension::Px(200.0);

    let mut item_style = ComputedStyle::default();
    item_style.width = Dimension::Px(100.0);
    item_style.height = Dimension::Px(50.0);

    let mut container = LayoutNode::with_children(
        container_style,
        vec![LayoutNode::new(item_style)],
    );

    container.compute_layout(400.0, 200.0);

    // Centered horizontally: (400 - 100) / 2 = 150
    assert_eq!(container.children[0].geometry.border_box.x, 150.0);
    // Centered vertically: (200 - 50) / 2 = 75
    assert_eq!(container.children[0].geometry.border_box.y, 75.0);
}

#[test]
fn test_css_grid_fractional_tracks_and_span() {
    // Grid: columns = [100px, 1fr, 2fr], gap = 10px, container width = 400px
    // Total gap = 2 * 10 = 20px
    // Remaining for fr = 400 - 100 - 20 = 280px
    // 1fr = 280 / 3 = 93.333...px
    // 2fr = 280 * 2 / 3 = 186.666...px
    let mut grid_style = ComputedStyle::default();
    grid_style.display = Display::Grid;
    grid_style.width = Dimension::Px(400.0);
    grid_style.height = Dimension::Px(300.0);
    grid_style.gap = 10.0;
    grid_style.grid_template_columns = vec![
        GridTrack::Px(100.0),
        GridTrack::Fr(1.0),
        GridTrack::Fr(2.0),
    ];

    let mut item1 = ComputedStyle::default();
    item1.height = Dimension::Px(50.0);

    let mut item2 = ComputedStyle::default();
    item2.height = Dimension::Px(50.0);

    let mut item3 = ComputedStyle::default();
    item3.height = Dimension::Px(50.0);

    let mut container = LayoutNode::with_children(
        grid_style,
        vec![LayoutNode::new(item1), LayoutNode::new(item2), LayoutNode::new(item3)],
    );

    container.compute_layout(400.0, 300.0);

    // Track 0: fixed 100px
    assert_eq!(container.children[0].geometry.border_box.x, 0.0);
    assert_eq!(container.children[0].geometry.border_box.width, 100.0);

    // Track 1: starts at 100 + 10 = 110px
    assert_eq!(container.children[1].geometry.border_box.x, 110.0);
    let track1_width = container.children[1].geometry.border_box.width;
    let track2_width = container.children[2].geometry.border_box.width;

    // Verify 1fr vs 2fr proportionality within subpixel tolerance
    assert!((track2_width - (track1_width * 2.0)).abs() < 0.2);
}

#[test]
fn test_css_grid_auto_placement_matrix() {
    // 2-column grid.
    // Item 1: span 1
    // Item 2: span 2 (cannot fit in row 0 col 1, so must wrap to row 1 col 0)
    // Item 3: span 1 (placed in row 2 col 0)
    let mut grid_style = ComputedStyle::default();
    grid_style.display = Display::Grid;
    grid_style.width = Dimension::Px(200.0);
    grid_style.grid_template_columns = vec![GridTrack::Px(100.0), GridTrack::Px(100.0)];

    let mut item1 = ComputedStyle::default();
    item1.height = Dimension::Px(40.0);
    item1.grid_column_span = Some(1);

    let mut item2 = ComputedStyle::default();
    item2.height = Dimension::Px(40.0);
    item2.grid_column_span = Some(2);

    let mut item3 = ComputedStyle::default();
    item3.height = Dimension::Px(40.0);
    item3.grid_column_span = Some(1);

    let mut container = LayoutNode::with_children(
        grid_style,
        vec![LayoutNode::new(item1), LayoutNode::new(item2), LayoutNode::new(item3)],
    );

    container.compute_layout(200.0, 400.0);

    // Item 1 at row 0, col 0
    assert_eq!(container.children[0].geometry.border_box.x, 0.0);
    assert_eq!(container.children[0].geometry.border_box.y, 0.0);

    // Item 2 wrapped to row 1, col 0, spanning 200px
    assert_eq!(container.children[1].geometry.border_box.x, 0.0);
    assert_eq!(container.children[1].geometry.border_box.y, 40.0);
    assert_eq!(container.children[1].geometry.border_box.width, 200.0);

    // Item 3 placed at row 2, col 0
    assert_eq!(container.children[2].geometry.border_box.x, 0.0);
    assert_eq!(container.children[2].geometry.border_box.y, 80.0);
}

#[test]
fn test_relative_and_absolute_positioning() {
    let mut container_style = ComputedStyle::default();
    container_style.display = Display::Block;
    container_style.width = Dimension::Px(500.0);
    container_style.height = Dimension::Px(400.0);
    container_style.padding = EdgeInsets::all(20.0);

    // 1. Relative child: in-flow position offset by left: 15, top: 10
    let mut rel_style = ComputedStyle::default();
    rel_style.display = Display::Block;
    rel_style.position = Position::Relative;
    rel_style.width = Dimension::Px(100.0);
    rel_style.height = Dimension::Px(50.0);
    rel_style.insets.left = Dimension::Px(15.0);
    rel_style.insets.top = Dimension::Px(10.0);

    // 2. Absolute child: out of flow, top: 5, right: 10
    let mut abs_style = ComputedStyle::default();
    abs_style.display = Display::Block;
    abs_style.position = Position::Absolute;
    abs_style.width = Dimension::Px(80.0);
    abs_style.height = Dimension::Px(40.0);
    abs_style.insets.top = Dimension::Px(5.0);
    abs_style.insets.right = Dimension::Px(10.0);

    let mut container = LayoutNode::with_children(
        container_style,
        vec![LayoutNode::new(rel_style), LayoutNode::new(abs_style)],
    );

    container.compute_layout(500.0, 400.0);

    // Relative element: normal flow starts at padding x=20, y=20, offset by dx=15, dy=10 => (35, 30)
    assert_eq!(container.children[0].geometry.border_box.x, 35.0);
    assert_eq!(container.children[0].geometry.border_box.y, 30.0);

    // Absolute element: container padding box is x=0, y=0, width=500, height=400
    // Right = 10, width = 80 => x = 0 + 500 - 10 - 80 = 410.0
    // Top = 5 => y = 0 + 5 = 5.0
    assert_eq!(container.children[1].geometry.border_box.x, 410.0);
    assert_eq!(container.children[1].geometry.border_box.y, 5.0);
}

#[test]
fn test_fixed_positioning_relative_to_viewport() {
    let mut root_style = ComputedStyle::default();
    root_style.display = Display::Block;
    root_style.width = Dimension::Px(800.0);
    root_style.height = Dimension::Px(600.0);

    let mut fixed_style = ComputedStyle::default();
    fixed_style.position = Position::Fixed;
    fixed_style.width = Dimension::Px(120.0);
    fixed_style.height = Dimension::Px(60.0);
    fixed_style.insets.bottom = Dimension::Px(20.0);
    fixed_style.insets.right = Dimension::Px(30.0);

    let mut root = LayoutNode::with_children(root_style, vec![LayoutNode::new(fixed_style)]);

    // Viewport: 800 x 600
    root.compute_layout(800.0, 600.0);

    // bottom = 20, right = 30
    // x = 800 - 30 - 120 = 650
    // y = 600 - 20 - 60 = 520
    assert_eq!(root.children[0].geometry.border_box.x, 650.0);
    assert_eq!(root.children[0].geometry.border_box.y, 520.0);
}

#[test]
fn test_text_intrinsic_measurement_and_wrapping() {
    let text = "Hello Strata Zero Dependency Frontend Engine";
    let font_size = 16.0;
    let line_height_mul = 1.2;

    // Unbroken single line
    let (w1, h1) = measure_text_bounds(text, font_size, line_height_mul, None);
    assert!(w1 > 100.0);
    assert_eq!(h1, 16.0 * 1.2);

    // Constrained width forcing line wrap into multiple lines
    let (w2, h2) = measure_text_bounds(text, font_size, line_height_mul, Some(80.0));
    assert!(w2 <= 80.0 || w2 < w1);
    assert!(h2 > h1); // Has wrapped into multiple lines
}

#[test]
fn test_subpixel_layout_snapping() {
    let rect = Rect::new(10.1234, 20.5678, 100.3333, 50.6666);
    let snapped = rect.snap_to_pixel();

    // Snap to 1/64th fixed point (multiples of 0.015625)
    let unit_x = (snapped.x * 64.0).round();
    let reconstructed_x = unit_x / 64.0;
    assert_eq!(snapped.x, reconstructed_x);

    let unit_y = (snapped.y * 64.0).round();
    let reconstructed_y = unit_y / 64.0;
    assert_eq!(snapped.y, reconstructed_y);
}

#[test]
fn test_wpt_suite_execution_and_compliance() {
    let results = WptSuite::run_all();
    assert_eq!(results.len(), 12);

    for vector in &results {
        assert!(
            vector.passed,
            "WPT Test vector '{}' ({}) failed with deviation: {:.4}px",
            vector.id, vector.title, vector.deviation_px
        );
        assert!(vector.deviation_px <= 0.016);
    }

    let compliance = WptSuite::compliance_percentage();
    assert_eq!(compliance, 100.0);
}

#[test]
fn test_subtree_layout_cache_invalidation_and_hit() {
    let mut root_style = ComputedStyle::default();
    root_style.display = Display::Flex;
    root_style.width = Dimension::Px(500.0);
    root_style.height = Dimension::Px(300.0);

    let mut child_style = ComputedStyle::default();
    child_style.width = Dimension::Px(200.0);
    child_style.height = Dimension::Px(100.0);

    let mut root = LayoutNode::with_children(root_style, vec![LayoutNode::new(child_style)]);

    assert!(!root.layout_cache.is_valid);
    assert!(!root.children[0].layout_cache.is_valid);

    // Initial layout computes and populates cache
    root.compute_layout(500.0, 300.0);

    assert!(root.layout_cache.is_valid);
    assert_eq!(root.layout_cache.available_w, 500.0);
    assert_eq!(root.layout_cache.available_h, 300.0);
    assert_eq!(root.layout_cache.result_w, 500.0);
    assert_eq!(root.layout_cache.result_h, 300.0);

    // Repeated layout with identical bounds hits the cache
    root.compute_layout(500.0, 300.0);
    assert!(root.layout_cache.is_valid);

    // Explicit cache invalidation
    root.invalidate_cache();
    assert!(!root.layout_cache.is_valid);
    assert!(!root.children[0].layout_cache.is_valid);
}

#[test]
fn test_layout_boundary_containment_classification() {
    use strata_style::types::{Containment, Dimension, Overflow, Position};

    // 1. contain: strict is always a layout boundary
    let mut strict_style = ComputedStyle::default();
    strict_style.contain = Containment::STRICT;
    let strict_node = LayoutNode::new(strict_style);
    assert!(strict_node.is_layout_boundary(), "contain: strict must be a layout boundary");

    // 2. contain: content is a layout boundary
    let mut content_style = ComputedStyle::default();
    content_style.contain = Containment::CONTENT;
    let content_node = LayoutNode::new(content_style);
    assert!(content_node.is_layout_boundary(), "contain: content must be a layout boundary");

    // 3. contain: layout + contain: size is a layout boundary
    let mut layout_size_style = ComputedStyle::default();
    layout_size_style.contain.layout = true;
    layout_size_style.contain.size = true;
    let layout_size_node = LayoutNode::new(layout_size_style);
    assert!(layout_size_node.is_layout_boundary(), "contain: layout + size must be a layout boundary");

    // 4. contain: layout with definite width and height is a layout boundary
    let mut layout_definite_style = ComputedStyle::default();
    layout_definite_style.contain.layout = true;
    layout_definite_style.width = Dimension::Px(300.0);
    layout_definite_style.height = Dimension::Px(200.0);
    let layout_definite_node = LayoutNode::new(layout_definite_style);
    assert!(layout_definite_node.is_layout_boundary(), "contain: layout with definite dimensions must be a layout boundary");

    // 5. contain: layout with auto height is NOT a layout boundary (height depends on descendants)
    let mut layout_auto_style = ComputedStyle::default();
    layout_auto_style.contain.layout = true;
    layout_auto_style.width = Dimension::Px(300.0);
    layout_auto_style.height = Dimension::Auto;
    let layout_auto_node = LayoutNode::new(layout_auto_style);
    assert!(!layout_auto_node.is_layout_boundary(), "contain: layout with auto height must not be a layout boundary");

    // 6. Absolute positioned box with clipping and definite dimensions is a layout boundary
    let mut abs_style = ComputedStyle::default();
    abs_style.position = Position::Absolute;
    abs_style.overflow_x = Overflow::Hidden;
    abs_style.overflow_y = Overflow::Hidden;
    abs_style.width = Dimension::Px(150.0);
    abs_style.height = Dimension::Px(100.0);
    let abs_node = LayoutNode::new(abs_style);
    assert!(abs_node.is_layout_boundary(), "clipped absolute box with definite dimensions must be a layout boundary");
}

#[test]
fn test_granular_subtree_layout_invalidation_and_preservation() {
    use strata_style::types::{Containment, Dimension, Display};

    let mut root_style = ComputedStyle::default();
    root_style.display = Display::Block;
    root_style.width = Dimension::Px(600.0);
    root_style.height = Dimension::Px(400.0);

    // Sibling A: Layout boundary (contain: strict)
    let mut a_style = ComputedStyle::default();
    a_style.display = Display::Block;
    a_style.contain = Containment::STRICT;
    a_style.width = Dimension::Px(250.0);
    a_style.height = Dimension::Px(200.0);

    let mut a_child_style = ComputedStyle::default();
    a_child_style.width = Dimension::Px(100.0);
    a_child_style.height = Dimension::Px(50.0);

    let sib_a = LayoutNode::with_children(
        a_style,
        vec![LayoutNode::new(a_child_style.clone()), LayoutNode::new(a_child_style)],
    );

    // Sibling B: Independent subtree
    let mut b_style = ComputedStyle::default();
    b_style.display = Display::Block;
    b_style.width = Dimension::Px(250.0);
    b_style.height = Dimension::Px(200.0);

    let mut b_child_style = ComputedStyle::default();
    b_child_style.width = Dimension::Px(120.0);
    b_child_style.height = Dimension::Px(60.0);

    let sib_b = LayoutNode::with_children(
        b_style,
        vec![LayoutNode::new(b_child_style.clone()), LayoutNode::new(b_child_style)],
    );

    let mut root = LayoutNode::with_children(root_style, vec![sib_a, sib_b]);

    // Initial full layout
    root.compute_layout(600.0, 400.0);
    assert!(root.layout_cache.is_valid);
    assert!(root.children[0].layout_cache.is_valid);
    assert!(root.children[1].layout_cache.is_valid);

    // Invalidate layout ONLY within Sibling A (index path [0, 0] under boundary [0])
    let invalidated = root.invalidate_layout_boundary(&[0, 0]);
    assert!(invalidated, "Must locate boundary along path");

    // Sibling A was invalidated
    assert!(!root.children[0].layout_cache.is_valid);

    // Sibling B was NOT invalidated: its layout cache remains intact
    assert!(
        root.children[1].layout_cache.is_valid,
        "Sibling B layout cache must be preserved across isolated boundary invalidation"
    );

    // Recompute localized layout within Sibling A boundary
    let localized = root.compute_subgrid_or_boundary_layout(&[0, 0]);
    assert!(localized, "Must recompute strictly within layout boundary");

    // Sibling A's children are now validly formatted again
    assert!(root.children[0].geometry.border_box.width > 0.0);
    // Sibling B is still valid and uncorrupted
    assert!(root.children[1].layout_cache.is_valid);
    assert_eq!(root.children[1].geometry.border_box.width, 250.0);
}


