//! Integration tests for explicit CSS Grid tracks, gaps, explicit placement, and spans.

#![allow(clippy::field_reassign_with_default)]

use strata_layout::prelude::*;
use strata_style::types::{ComputedStyle, Dimension, Display, GridTrack};

fn card_style(column_span: Option<u32>) -> ComputedStyle {
    let mut style = ComputedStyle::default();
    style.display = Display::Block;
    style.grid_column_span = column_span;
    style
}

#[test]
fn test_explicit_tracks_match_parity_fixture() {
    // Mirrors the `grid_tracks` parity fixture:
    // width 1000px, columns 260px 420px 260px, rows 150px 150px, gap 16px.
    let mut grid_style = ComputedStyle::default();
    grid_style.display = Display::Grid;
    grid_style.width = Dimension::Px(1000.0);
    grid_style.gap = 16.0;
    grid_style.grid_template_columns = vec![
        GridTrack::Px(260.0),
        GridTrack::Px(420.0),
        GridTrack::Px(260.0),
    ];
    grid_style.grid_template_rows = vec![GridTrack::Px(150.0), GridTrack::Px(150.0)];

    let mut container = LayoutNode::with_children(
        grid_style,
        vec![
            LayoutNode::new(card_style(Some(2))),
            LayoutNode::new(card_style(None)),
            LayoutNode::new(card_style(None)),
            LayoutNode::new(card_style(Some(2))),
        ],
    );

    container.compute_layout(1000.0, 720.0);

    let boxes: Vec<(f32, f32, f32, f32)> = container
        .children
        .iter()
        .map(|child| {
            let border_box = child.geometry.border_box;
            (
                border_box.x,
                border_box.y,
                border_box.width,
                border_box.height,
            )
        })
        .collect();

    // Chromium reference geometry.
    assert_eq!(boxes[0], (0.0, 0.0, 696.0, 150.0));
    assert_eq!(boxes[1], (712.0, 0.0, 260.0, 150.0));
    assert_eq!(boxes[2], (0.0, 166.0, 260.0, 150.0));
    assert_eq!(boxes[3], (276.0, 166.0, 696.0, 150.0));
}

#[test]
fn test_explicit_placement_overrides_auto_cursor() {
    let mut grid_style = ComputedStyle::default();
    grid_style.display = Display::Grid;
    grid_style.width = Dimension::Px(300.0);
    grid_style.grid_template_columns = vec![
        GridTrack::Px(100.0),
        GridTrack::Px(100.0),
        GridTrack::Px(100.0),
    ];
    grid_style.grid_template_rows = vec![GridTrack::Px(50.0), GridTrack::Px(50.0)];

    // Placed on the second column and second row, both 1-based CSS lines.
    let mut placed_style = card_style(None);
    placed_style.grid_column_start = Some(2);
    placed_style.grid_row_start = Some(2);

    let auto_style = card_style(None);

    let mut container = LayoutNode::with_children(
        grid_style,
        vec![LayoutNode::new(placed_style), LayoutNode::new(auto_style)],
    );

    container.compute_layout(300.0, 200.0);

    let placed = container.children[0].geometry.border_box;
    assert_eq!(placed.x, 100.0);
    assert_eq!(placed.y, 50.0);

    // The auto cursor continues after the explicitly placed item.
    let auto = container.children[1].geometry.border_box;
    assert_eq!(auto.x, 200.0);
    assert_eq!(auto.y, 50.0);
}
