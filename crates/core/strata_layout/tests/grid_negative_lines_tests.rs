//! W3C CSS Grid Level 1 §8.3 Negative Grid Line Resolution Parity Tests.

#![allow(clippy::field_reassign_with_default)]

use strata_layout::tree::LayoutNode;
use strata_style::prelude::*;
use strata_style::types::{ComputedStyle, Dimension, Display, GridTrack};

#[test]
fn test_negative_grid_column_span_full_explicit_grid() {
    let mut container_style = ComputedStyle::default();
    container_style.display = Display::Grid;
    container_style.width = Dimension::Px(400.0);
    container_style.height = Dimension::Px(300.0);
    container_style.grid_template_columns = vec![
        GridTrack::Px(100.0),
        GridTrack::Px(100.0),
        GridTrack::Px(100.0),
    ];

    let mut full_span_child = LayoutNode::new(ComputedStyle::default());
    full_span_child.style.grid_column_start = Some(1);
    full_span_child.style.grid_column_end = Some(-1);

    let mut container = LayoutNode::with_children(container_style, vec![full_span_child]);
    container.compute_layout(400.0, 300.0);

    let child_geom = &container.children[0].geometry.border_box;
    assert_eq!(
        child_geom.x, 0.0,
        "Item starting at line 1 must be positioned at x = 0"
    );
    assert_eq!(
        child_geom.width, 300.0,
        "Item spanning from line 1 to line -1 must span all 3 tracks (3 * 100px = 300px)"
    );
}

#[test]
fn test_negative_grid_column_last_cell() {
    let mut container_style = ComputedStyle::default();
    container_style.display = Display::Grid;
    container_style.width = Dimension::Px(400.0);
    container_style.height = Dimension::Px(200.0);
    container_style.grid_template_columns = vec![
        GridTrack::Px(80.0),
        GridTrack::Px(80.0),
        GridTrack::Px(80.0),
        GridTrack::Px(80.0),
    ];

    // -2 to -1 on 4 explicit columns should occupy the 4th column (track index 3)
    let mut child = LayoutNode::new(ComputedStyle::default());
    child.style.grid_column_start = Some(-2);
    child.style.grid_column_end = Some(-1);

    let mut container = LayoutNode::with_children(container_style, vec![child]);
    container.compute_layout(400.0, 200.0);

    let child_geom = &container.children[0].geometry.border_box;
    assert_eq!(
        child_geom.x, 240.0,
        "Item spanning -2 to -1 must start at track 3 (3 * 80px = 240px)"
    );
    assert_eq!(
        child_geom.width, 80.0,
        "Item spanning -2 to -1 must have width of 1 track (80px)"
    );
}

#[test]
fn test_negative_grid_column_subset_span() {
    let mut container_style = ComputedStyle::default();
    container_style.display = Display::Grid;
    container_style.width = Dimension::Px(500.0);
    container_style.height = Dimension::Px(200.0);
    container_style.grid_template_columns = vec![
        GridTrack::Px(50.0),
        GridTrack::Px(50.0),
        GridTrack::Px(50.0),
        GridTrack::Px(50.0),
        GridTrack::Px(50.0),
    ];

    // 2 to -2 on 5 explicit columns:
    // Line 2 = track 1.
    // Line -2 = line 5 (track 4 start, track 3 end).
    // Spans tracks 1, 2, 3 (3 tracks).
    let mut child = LayoutNode::new(ComputedStyle::default());
    child.style.grid_column_start = Some(2);
    child.style.grid_column_end = Some(-2);

    let mut container = LayoutNode::with_children(container_style, vec![child]);
    container.compute_layout(500.0, 200.0);

    let child_geom = &container.children[0].geometry.border_box;
    assert_eq!(
        child_geom.x, 50.0,
        "Item starting at line 2 must start at x = 50px"
    );
    assert_eq!(
        child_geom.width, 150.0,
        "Item spanning from line 2 to -2 must span 3 tracks (3 * 50px = 150px)"
    );
}

#[test]
fn test_negative_grid_row_span_full_explicit_grid() {
    let mut container_style = ComputedStyle::default();
    container_style.display = Display::Grid;
    container_style.width = Dimension::Px(200.0);
    container_style.height = Dimension::Px(300.0);
    container_style.grid_template_columns = vec![GridTrack::Px(100.0)];
    container_style.grid_template_rows = vec![
        GridTrack::Px(60.0),
        GridTrack::Px(60.0),
        GridTrack::Px(60.0),
    ];

    let mut child = LayoutNode::new(ComputedStyle::default());
    child.style.grid_row_start = Some(1);
    child.style.grid_row_end = Some(-1);

    let mut container = LayoutNode::with_children(container_style, vec![child]);
    container.compute_layout(200.0, 300.0);

    let child_geom = &container.children[0].geometry.border_box;
    assert_eq!(
        child_geom.y, 0.0,
        "Item starting at row line 1 must start at y = 0"
    );
    assert_eq!(
        child_geom.height, 180.0,
        "Item spanning from row line 1 to -1 must span all 3 rows (3 * 60px = 180px)"
    );
}

#[test]
fn test_css_cascade_negative_grid_line_parsing() {
    let css = r#"
        .hero {
            grid-column: 1 / -1;
            grid-row: -2 / -1;
        }
    "#;
    let sheet = StyleSheet::parse(css).expect("Failed to parse stylesheet");
    let state = PseudoState::default();

    let style = CascadeSolver::resolve("div", &["hero"], None, None, &[&sheet], None, &state);
    assert_eq!(style.grid_column_start, Some(1));
    assert_eq!(style.grid_column_end, Some(-1));
    assert_eq!(style.grid_row_start, Some(-2));
    assert_eq!(style.grid_row_end, Some(-1));
}
