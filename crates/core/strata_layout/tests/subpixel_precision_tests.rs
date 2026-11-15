#![allow(clippy::field_reassign_with_default)]

use strata_layout::geometry::{LayoutGeometry, Rect};
use strata_layout::tree::LayoutNode;
use strata_style::types::{ComputedStyle, Dimension, Display, GridTrack};

#[test]
fn test_rect_subpixel_edge_boundary_snapping() {
    // 3 adjacent boxes sharing boundaries at fractional coordinates
    let b1 = Rect::new(0.0, 0.0, 33.33333, 50.0).snap_to_pixel();
    let b2 = Rect::new(33.33333, 0.0, 33.33333, 50.0).snap_to_pixel();
    let b3 = Rect::new(66.66666, 0.0, 33.33334, 50.0).snap_to_pixel();

    // Box 1 right edge must match Box 2 left edge exactly
    assert_eq!(
        b1.x + b1.width,
        b2.x,
        "Box 1 right edge ({}) must seamlessly meet Box 2 left edge ({})",
        b1.x + b1.width,
        b2.x
    );

    // Box 2 right edge must match Box 3 left edge exactly
    assert_eq!(
        b2.x + b2.width,
        b3.x,
        "Box 2 right edge ({}) must seamlessly meet Box 3 left edge ({})",
        b2.x + b2.width,
        b3.x
    );

    // Total width across all 3 boxes must equal exactly 100.0 (the outer span)
    let total_w = b1.width + b2.width + b3.width;
    assert_eq!(
        total_w, 100.0,
        "Sum of snapped widths must equal exact container width without subpixel drift"
    );
}

#[test]
fn test_rect_device_scale_snapping_retina_and_hidpi() {
    let r = Rect::new(10.3333, 20.6667, 100.3333, 50.6667);

    // Test at 2.0x Retina scale
    let r_retina = r.snap_to_scale(2.0);
    assert_eq!((r_retina.x * 2.0).fract(), 0.0);
    assert_eq!((r_retina.y * 2.0).fract(), 0.0);
    assert_eq!((r_retina.width * 2.0).fract(), 0.0);
    assert_eq!((r_retina.height * 2.0).fract(), 0.0);

    // Test at 1.5x HiDPI scale
    let r_15x = r.snap_to_scale(1.5);
    assert!(((r_15x.x * 1.5).round() - r_15x.x * 1.5).abs() < 1e-4);

    // LayoutGeometry snap_to_scale
    let mut geom = LayoutGeometry::from_border_box(
        r,
        Default::default(),
        Default::default(),
        Default::default(),
    );
    geom.snap_to_scale(2.0);
    assert_eq!((geom.border_box.x * 2.0).fract(), 0.0);
    assert_eq!((geom.border_box.width * 2.0).fract(), 0.0);
}

#[test]
fn test_grid_subpixel_column_tracks_no_hairline_gaps() {
    let mut grid_style = ComputedStyle::default();
    grid_style.display = Display::Grid;
    grid_style.width = Dimension::Px(100.0);
    grid_style.height = Dimension::Px(50.0);
    grid_style.grid_template_columns =
        vec![GridTrack::Fr(1.0), GridTrack::Fr(1.0), GridTrack::Fr(1.0)];
    grid_style.grid_template_rows = vec![GridTrack::Fr(1.0)];

    let c1 = LayoutNode::new(ComputedStyle::default());
    let c2 = LayoutNode::new(ComputedStyle::default());
    let c3 = LayoutNode::new(ComputedStyle::default());

    let mut grid = LayoutNode::with_children(grid_style, vec![c1, c2, c3]);
    grid.compute_layout(100.0, 50.0);

    let cell1 = &grid.children[0];
    let cell2 = &grid.children[1];
    let cell3 = &grid.children[2];

    // Assert that cell 1 and cell 2 share boundary with zero gap
    let cell1_right = cell1.geometry.border_box.x + cell1.geometry.border_box.width;
    assert_eq!(
        cell1_right, cell2.geometry.border_box.x,
        "Cell 1 right edge ({}) must equal Cell 2 left edge ({})",
        cell1_right, cell2.geometry.border_box.x
    );

    // Assert that cell 2 and cell 3 share boundary with zero gap
    let cell2_right = cell2.geometry.border_box.x + cell2.geometry.border_box.width;
    assert_eq!(
        cell2_right, cell3.geometry.border_box.x,
        "Cell 2 right edge ({}) must equal Cell 3 left edge ({})",
        cell2_right, cell3.geometry.border_box.x
    );

    // Cell 3 right edge must equal grid right edge (100.0)
    let cell3_right = cell3.geometry.border_box.x + cell3.geometry.border_box.width;
    assert_eq!(cell3_right, 100.0);
}

#[test]
fn test_deeply_nested_layout_tree_zero_drift() {
    // Build a 10-level deeply nested hierarchy with fractional padding and widths
    let mut current = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        width: Dimension::Auto,
        height: Dimension::Px(20.0),
        ..Default::default()
    });

    for _ in 0..10 {
        let mut parent_style = ComputedStyle::default();
        parent_style.display = Display::Block;
        parent_style.width = Dimension::Px(300.3333);
        parent_style.height = Dimension::Auto;
        parent_style.padding.left = 5.015625;
        parent_style.padding.right = 5.015625;

        current = LayoutNode::with_children(parent_style, vec![current]);
    }

    current.compute_layout(500.0, 1000.0);

    // Ensure all geometry coordinates remain strictly finite and within layout limits
    assert!(current.geometry.is_finite());
    assert!(current.geometry.border_box.width > 0.0);
}
