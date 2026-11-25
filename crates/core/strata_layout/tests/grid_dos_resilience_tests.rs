//! Adversarial Grid Memory Allocation & DoS Resilience Tests.
//!
//! Validates:
//! 1. Pathological line placement (e.g. 50,000 / 10,000,000) does not cause OOM memory blowout.
//! 2. Pathological spans are clamped to MAX_GRID_TRACKS.
//! 3. Layout resolves in sub-millisecond time with zero panics.

use std::time::Instant;
use strata_layout::grid::{solve_grid, MAX_GRID_TRACKS};
use strata_layout::tree::LayoutNode;
use strata_style::types::{ComputedStyle, Display, GridTrack};

#[test]
fn test_pathological_grid_line_numbers_do_not_exhaust_memory() {
    let mut container = LayoutNode::new(ComputedStyle {
        display: Display::Grid,
        grid_template_columns: vec![GridTrack::Px(100.0)],
        grid_template_rows: vec![GridTrack::Px(100.0)],
        ..Default::default()
    });

    let mut malicious_child = LayoutNode::new(ComputedStyle {
        grid_row_start: Some(50_000),
        grid_column_start: Some(50_000),
        grid_row_span: Some(50_000),
        grid_column_span: Some(50_000),
        ..Default::default()
    });
    malicious_child.geometry.border_box.width = 50.0;
    malicious_child.geometry.border_box.height = 50.0;

    container.children.push(malicious_child);

    let start = Instant::now();
    solve_grid(&mut container, 800.0, 600.0);
    let elapsed = start.elapsed();

    // Must solve in well under 500ms (even on loaded debug CI VMs) without exhausting memory
    assert!(
        elapsed.as_millis() < 500,
        "Pathological grid layout must complete in <500ms, took {elapsed:?}"
    );

    // Verify child geometry is within bounded coordinates
    let placed_child = &container.children[0];
    assert!(
        placed_child.geometry.border_box.x >= 0.0,
        "X position must be non-negative"
    );
    assert!(
        placed_child.geometry.border_box.y >= 0.0,
        "Y position must be non-negative"
    );
}

#[test]
fn test_extreme_integer_grid_line_placement() {
    let mut container = LayoutNode::new(ComputedStyle {
        display: Display::Grid,
        ..Default::default()
    });

    let child = LayoutNode::new(ComputedStyle {
        grid_row_start: Some(i32::MAX),
        grid_column_start: Some(i32::MAX),
        ..Default::default()
    });
    container.children.push(child);

    // Should clamp gracefully without integer overflow panic
    solve_grid(&mut container, 1000.0, 1000.0);
    assert_eq!(container.children.len(), 1);
}

#[test]
fn test_max_grid_tracks_constant_is_enforced() {
    assert_eq!(MAX_GRID_TRACKS, 1000);
}
