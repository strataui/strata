//! Integration Tests: Multi-Pass Intrinsic Min/Max Sizing and Shrink-to-Fit Solver.

use strata_layout::prelude::*;
use strata_style::prelude::*;

#[test]
fn test_min_max_sizes_shrink_to_fit_formula() {
    let sizes = MinMaxSizes::new(100.0, 300.0);

    // Case 1: Available space smaller than min_content -> clamped to min_content
    assert_eq!(sizes.shrink_to_fit(50.0), 100.0);

    // Case 2: Available space between min and max -> adopts available
    assert_eq!(sizes.shrink_to_fit(220.0), 220.0);

    // Case 3: Available space larger than max_content -> clamped to max_content
    assert_eq!(sizes.shrink_to_fit(500.0), 300.0);
}

#[test]
fn test_compute_min_max_sizes_text_node() {
    let mut text_node = LayoutNode::new(ComputedStyle::default());
    text_node.text = Some("Distributed Cloud Architecture".to_string());
    text_node.style.font_size = 16.0;

    let sizes = compute_min_max_sizes(&text_node, None);

    // min_content: longest single word ("Architecture")
    assert!(sizes.min_content > 40.0 && sizes.min_content < 150.0);

    // max_content: full un-wrapped line ("Distributed Cloud Architecture")
    assert!(sizes.max_content > sizes.min_content);
    assert!(sizes.max_content > 180.0);
}

#[test]
fn test_compute_min_max_sizes_block_and_flex_containers() {
    // 1. Block container: sizes equal to widest child
    let mut parent = LayoutNode::new(ComputedStyle::default());
    parent.style.display = Display::Block;
    parent.style.padding.left = 10.0;
    parent.style.padding.right = 10.0;

    let mut child1 = LayoutNode::new(ComputedStyle::default());
    child1.style.width = Dimension::Px(150.0);

    let mut child2 = LayoutNode::new(ComputedStyle::default());
    child2.style.width = Dimension::Px(240.0);

    parent.add_child(child1);
    parent.add_child(child2);

    let block_sizes = compute_min_max_sizes(&parent, None);
    // Widest child (240.0) + padding (20.0) = 260.0
    assert_eq!(block_sizes.min_content, 260.0);
    assert_eq!(block_sizes.max_content, 260.0);

    // 2. Flex Row container: sizes sum along main axis
    let mut flex_parent = LayoutNode::new(ComputedStyle::default());
    flex_parent.style.display = Display::Flex;
    flex_parent.style.flex_direction = FlexDirection::Row;
    flex_parent.style.gap = 15.0;

    let mut f_child1 = LayoutNode::new(ComputedStyle::default());
    f_child1.style.width = Dimension::Px(100.0);

    let mut f_child2 = LayoutNode::new(ComputedStyle::default());
    f_child2.style.width = Dimension::Px(200.0);

    flex_parent.add_child(f_child1);
    flex_parent.add_child(f_child2);

    let flex_sizes = compute_min_max_sizes(&flex_parent, None);
    // 100.0 + 200.0 + 15.0 gap = 315.0
    assert_eq!(flex_sizes.min_content, 315.0);
    assert_eq!(flex_sizes.max_content, 315.0);
}

#[test]
fn test_compute_min_max_sizes_clamping() {
    let mut node = LayoutNode::new(ComputedStyle::default());
    node.text = Some("Hello".to_string());
    node.style.min_width = Dimension::Px(200.0);
    node.style.max_width = Dimension::Px(500.0);

    let sizes = compute_min_max_sizes(&node, None);
    assert!(sizes.min_content >= 200.0, "Must be clamped to min-width 200.0");
    assert!(sizes.max_content >= 200.0);
}

#[test]
fn test_compute_min_max_sizes_ignores_out_of_flow_absolute_children() {
    let mut parent = LayoutNode::new(ComputedStyle::default());
    parent.style.display = Display::Block;

    let mut in_flow_child = LayoutNode::new(ComputedStyle::default());
    in_flow_child.style.width = Dimension::Px(60.0);

    let mut out_of_flow_child = LayoutNode::new(ComputedStyle::default());
    out_of_flow_child.style.position = Position::Absolute;
    out_of_flow_child.style.width = Dimension::Px(240.0);

    parent.add_child(in_flow_child);
    parent.add_child(out_of_flow_child);

    let sizes = compute_min_max_sizes(&parent, None);
    assert_eq!(sizes.min_content, 60.0);
    assert_eq!(sizes.max_content, 60.0);
}

#[test]
fn test_compute_min_max_sizes_grid_container() {
    // 1. Grid with fixed px columns and gap: 200px + 300px + 20px gap = 520px
    let mut grid_parent = LayoutNode::new(ComputedStyle::default());
    grid_parent.style.display = Display::Grid;
    grid_parent.style.grid_template_columns = vec![GridTrack::Px(200.0), GridTrack::Px(300.0)];
    grid_parent.style.gap = 20.0;

    let mut item1 = LayoutNode::new(ComputedStyle::default());
    item1.style.width = Dimension::Px(100.0);

    let mut item2 = LayoutNode::new(ComputedStyle::default());
    item2.style.width = Dimension::Px(150.0);

    grid_parent.add_child(item1);
    grid_parent.add_child(item2);

    let sizes = compute_min_max_sizes(&grid_parent, None);
    assert_eq!(sizes.min_content, 520.0);
    assert_eq!(sizes.max_content, 520.0);

    // 2. Grid with auto and min-content tracks sizing to children
    let mut auto_grid = LayoutNode::new(ComputedStyle::default());
    auto_grid.style.display = Display::Grid;
    auto_grid.style.grid_template_columns = vec![GridTrack::MinContent, GridTrack::MaxContent];
    auto_grid.style.gap = 10.0;

    let mut text_item1 = LayoutNode::new(ComputedStyle::default());
    text_item1.text = Some("Distributed Systems".to_string());
    text_item1.style.font_size = 16.0;

    let mut text_item2 = LayoutNode::new(ComputedStyle::default());
    text_item2.text = Some("Cloud Computing Engine".to_string());
    text_item2.style.font_size = 16.0;

    auto_grid.add_child(text_item1);
    auto_grid.add_child(text_item2);

    let auto_sizes = compute_min_max_sizes(&auto_grid, None);
    // Track 0 (MinContent) takes child 0's min_content
    // Track 1 (MaxContent) takes child 1's max_content
    // Gap = 10.0
    assert!(auto_sizes.min_content > 50.0);
    assert!(auto_sizes.max_content > auto_sizes.min_content);
}

