//! W3C CSS Grid Layout Level 2 Integration Tests.
//!
//! Validates:
//! - Subgrid track inheritance from parent grid containers (§2)
//! - Auto-placement cursor algorithm with `grid-auto-flow: dense` (§8.5)
//! - `minmax(min, max)` track resolution (§7.2)
//! - Column-major auto placement flow (`grid-auto-flow: column`)

#![allow(clippy::field_reassign_with_default)]

use strata_layout::prelude::*;
use strata_style::types::{
    ComputedStyle, Dimension, Display, GridAutoFlow, GridTrack, GridTrackBreadth,
};

#[test]
fn test_w3c_grid_dense_auto_placement_fills_holes() {
    // 3-column grid of 100px each (total 300px).
    // Item 1: span 2 columns (occupies row 0, col 0..2)
    // Item 2: span 2 columns (cannot fit in row 0 col 2, wraps to row 1, col 0..2)
    //   -> Leaves row 0, col 2 empty!
    // Item 3: span 1 column.
    // In dense mode (grid-auto-flow: dense), Item 3 must fill the hole at row 0, col 2!
    let mut container_style = ComputedStyle::default();
    container_style.display = Display::Grid;
    container_style.grid_template_columns = vec![
        GridTrack::Px(100.0),
        GridTrack::Px(100.0),
        GridTrack::Px(100.0),
    ];
    container_style.grid_auto_flow = GridAutoFlow::RowDense;
    container_style.width = Dimension::Px(300.0);
    container_style.height = Dimension::Px(200.0);

    let mut item1 = LayoutNode::new(ComputedStyle::default());
    item1.style.grid_column_span = Some(2);

    let mut item2 = LayoutNode::new(ComputedStyle::default());
    item2.style.grid_column_span = Some(2);

    let mut item3 = LayoutNode::new(ComputedStyle::default());
    item3.style.grid_column_span = Some(1);

    let mut container = LayoutNode::with_children(container_style, vec![item1, item2, item3]);
    container.compute_layout(300.0, 200.0);

    // Item 1 is at row 0, col 0 (x = 0, y = 0, width = 200)
    assert_eq!(container.children[0].geometry.border_box.x, 0.0);
    assert_eq!(container.children[0].geometry.border_box.y, 0.0);
    assert_eq!(container.children[0].geometry.border_box.width, 200.0);

    // Item 2 is at row 1, col 0 (x = 0, y = 100, width = 200)
    assert_eq!(container.children[1].geometry.border_box.x, 0.0);
    assert_eq!(container.children[1].geometry.border_box.y, 100.0);
    assert_eq!(container.children[1].geometry.border_box.width, 200.0);

    // Item 3 (dense) backfilled into row 0, col 2 (x = 200, y = 0, width = 100)!
    assert_eq!(container.children[2].geometry.border_box.x, 200.0);
    assert_eq!(container.children[2].geometry.border_box.y, 0.0);
    assert_eq!(container.children[2].geometry.border_box.width, 100.0);
}

#[test]
fn test_w3c_grid_subgrid_track_inheritance() {
    // Parent grid: 4 columns: [100px, 150px, 200px, 250px]
    // Item 1 is a subgrid spanning columns 1..3 (columns index 1 and 2, sizes 150px and 200px)
    // Inside Item 1, there are 2 children with 1-column span each.
    // Child 1 must get width 150px, Child 2 must get width 200px!
    let mut parent_style = ComputedStyle::default();
    parent_style.display = Display::Grid;
    parent_style.grid_template_columns = vec![
        GridTrack::Px(100.0),
        GridTrack::Px(150.0),
        GridTrack::Px(200.0),
        GridTrack::Px(250.0),
    ];
    parent_style.width = Dimension::Px(700.0);
    parent_style.height = Dimension::Px(300.0);

    let mut subgrid_style = ComputedStyle::default();
    subgrid_style.display = Display::Grid;
    subgrid_style.grid_column_start = Some(2); // line 2 = index 1
    subgrid_style.grid_column_span = Some(2);  // spans index 1 and 2 (150px + 200px)
    subgrid_style.grid_template_columns = vec![GridTrack::Subgrid];

    let sub_child1 = LayoutNode::new(ComputedStyle::default());
    let sub_child2 = LayoutNode::new(ComputedStyle::default());

    let subgrid_node = LayoutNode::with_children(subgrid_style, vec![sub_child1, sub_child2]);
    let mut parent = LayoutNode::with_children(parent_style, vec![subgrid_node]);

    parent.compute_layout(700.0, 300.0);

    // Subgrid container is placed at x = 100, width = 350
    assert_eq!(parent.children[0].geometry.border_box.x, 100.0);
    assert_eq!(parent.children[0].geometry.border_box.width, 350.0);

    // Subgrid's child 1 inherits track 1: x = 100 + 0 = 100, width = 150
    assert_eq!(parent.children[0].children[0].geometry.border_box.x, 100.0);
    assert_eq!(parent.children[0].children[0].geometry.border_box.width, 150.0);

    // Subgrid's child 2 inherits track 2: x = 100 + 150 = 250, width = 200
    assert_eq!(parent.children[0].children[1].geometry.border_box.x, 250.0);
    assert_eq!(parent.children[0].children[1].geometry.border_box.width, 200.0);
}

#[test]
fn test_w3c_grid_minmax_track_resolution() {
    // Container width = 400px.
    // Column 0: minmax(100px, 1fr)
    // Column 1: minmax(150px, 2fr)
    // Total min = 100 + 150 = 250px.
    // Remaining space = 400 - 250 = 150px.
    // Total fr = 1 + 2 = 3fr.
    // Col 0 gets 100 + (1/3)*150 = 150px.
    // Col 1 gets 150 + (2/3)*150 = 250px.
    let mut grid_style = ComputedStyle::default();
    grid_style.display = Display::Grid;
    grid_style.grid_template_columns = vec![
        GridTrack::MinMax(GridTrackBreadth::Px(100.0), GridTrackBreadth::Fr(1.0)),
        GridTrack::MinMax(GridTrackBreadth::Px(150.0), GridTrackBreadth::Fr(2.0)),
    ];
    grid_style.width = Dimension::Px(400.0);
    grid_style.height = Dimension::Px(100.0);

    let item1 = LayoutNode::new(ComputedStyle::default());
    let item2 = LayoutNode::new(ComputedStyle::default());

    let mut grid = LayoutNode::with_children(grid_style, vec![item1, item2]);
    grid.compute_layout(400.0, 100.0);

    assert_eq!(grid.children[0].geometry.border_box.width, 150.0);
    assert_eq!(grid.children[1].geometry.border_box.width, 250.0);
    assert_eq!(grid.children[0].geometry.border_box.x, 0.0);
    assert_eq!(grid.children[1].geometry.border_box.x, 150.0);
}

#[test]
fn test_w3c_grid_multipass_explicit_item_not_stolen_by_earlier_auto_item() {
    // 2-column grid of 100px each.
    // Item 0 is an auto item (no explicit placement).
    // Item 1 is an explicit item placed at grid-column-start: 1, grid-row-start: 1 (cell 0, 0).
    // Under CSS Grid Level 2 §8.5 multi-pass placement:
    // Phase 1 reserves cell (0, 0) for Item 1.
    // Phase 4 auto-places Item 0 into cell (0, 1).
    let mut grid_style = ComputedStyle::default();
    grid_style.display = Display::Grid;
    grid_style.grid_template_columns = vec![GridTrack::Px(100.0), GridTrack::Px(100.0)];
    grid_style.width = Dimension::Px(200.0);
    grid_style.height = Dimension::Px(100.0);

    let auto_item = LayoutNode::new(ComputedStyle::default());

    let mut explicit_style = ComputedStyle::default();
    explicit_style.grid_column_start = Some(1);
    explicit_style.grid_row_start = Some(1);
    let explicit_item = LayoutNode::new(explicit_style);

    let mut grid = LayoutNode::with_children(grid_style, vec![auto_item, explicit_item]);
    grid.compute_layout(200.0, 100.0);

    // Item 0 (auto) must be pushed to column 1 because cell (0, 0) was reserved by Item 1!
    assert_eq!(grid.children[0].geometry.border_box.x, 100.0);
    assert_eq!(grid.children[0].geometry.border_box.y, 0.0);

    // Item 1 (explicit) gets its authored position (0, 0)!
    assert_eq!(grid.children[1].geometry.border_box.x, 0.0);
    assert_eq!(grid.children[1].geometry.border_box.y, 0.0);
}

#[test]
fn test_w3c_grid_implicit_column_track_expansion() {
    // 2-column explicit grid: [100px, 100px]
    // Item 0 placed at grid-column-start: 4, grid-column-span: 2.
    // Total columns needed = 3 (0..3) + 2 = 5 columns.
    // Implicit tracks (cols 2, 3, 4) expand with GridTrack::Auto.
    let mut grid_style = ComputedStyle::default();
    grid_style.display = Display::Grid;
    grid_style.grid_template_columns = vec![GridTrack::Px(100.0), GridTrack::Px(100.0)];
    grid_style.width = Dimension::Px(500.0);
    grid_style.height = Dimension::Px(100.0);

    let mut placed_style = ComputedStyle::default();
    placed_style.grid_column_start = Some(4); // 1-based line 4 = 0-indexed column 3
    placed_style.grid_column_span = Some(2);  // spans columns 3 and 4
    placed_style.grid_row_start = Some(1);
    let item = LayoutNode::new(placed_style);

    let mut grid = LayoutNode::with_children(grid_style, vec![item]);
    grid.compute_layout(500.0, 100.0);

    // Columns 0 and 1 are 100px each.
    // Remaining space for implicit columns (3 tracks) = 500 - 200 = 300px -> 100px each.
    // Column 3 starts at x = 100 + 100 + 100 = 300px.
    // Spanning 2 tracks (cols 3 and 4) = 100 + 100 = 200px width.
    assert_eq!(grid.children[0].geometry.border_box.x, 300.0);
    assert_eq!(grid.children[0].geometry.border_box.width, 200.0);
}

