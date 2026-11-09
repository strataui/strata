//! Specification conformance and adversarial stress tests for deep tree recursion guards.
//!
//! Validates that Project Strata gracefully handles extreme DOM tree depths (up to 10,000+ levels)
//! across layout computation, geometry translation, cache invalidation, intrinsic sizing,
//! display list composition, and iterative deallocation without thread call-stack exhaustion.

use strata_layout::intrinsic::compute_min_max_sizes;
use strata_layout::tree::{LayoutNode, MAX_LAYOUT_DEPTH};
use strata_style::types::{ComputedStyle, Dimension, Display};

#[test]
fn test_10000_nested_blocks_layout_and_deallocation() {
    // Construct 10,000 singly-nested Block layout nodes
    let mut current = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        width: Dimension::Px(120.0),
        height: Dimension::Px(40.0),
        ..ComputedStyle::default()
    });

    for _ in 0..10_000 {
        let parent = LayoutNode::with_children(
            ComputedStyle {
                display: Display::Block,
                width: Dimension::Auto,
                height: Dimension::Auto,
                ..ComputedStyle::default()
            },
            vec![current],
        );
        current = parent;
    }

    // 1. Layout computation under depth guard
    current.compute_layout(1024.0, 768.0);
    assert!(
        current.geometry.border_box.width.is_finite(),
        "Root width must be finite"
    );
    assert!(
        current.geometry.border_box.height.is_finite(),
        "Root height must be finite"
    );
    assert!(
        current.geometry.border_box.width > 0.0,
        "Root width must be positive"
    );

    // 2. Iterative translation on 10,000-deep tree
    let initial_x = current.geometry.border_box.x;
    let initial_y = current.geometry.border_box.y;
    current.translate(25.0, 50.0);
    assert_eq!(current.geometry.border_box.x, initial_x + 25.0);
    assert_eq!(current.geometry.border_box.y, initial_y + 50.0);

    // 3. Iterative cache invalidation on 10,000-deep tree
    current.invalidate_cache();
    assert!(
        !current.layout_cache.is_valid,
        "Cache must be marked invalid"
    );

    // 4. Clean deallocation via iterative Drop implementation
    drop(current);
}

#[test]
fn test_5000_nested_inline_containers_phrasing_flattening() {
    let text_leaf = LayoutNode::with_text(
        ComputedStyle {
            display: Display::Inline,
            font_size: 16.0,
            line_height: 1.25,
            ..ComputedStyle::default()
        },
        "Deeply nested inline payload".to_string(),
    );

    let mut current = text_leaf;
    for _ in 0..5_000 {
        let parent = LayoutNode::with_children(
            ComputedStyle {
                display: Display::Inline,
                font_size: 16.0,
                line_height: 1.25,
                ..ComputedStyle::default()
            },
            vec![current],
        );
        current = parent;
    }

    // Wrap in an outer block container
    let mut root = LayoutNode::with_children(
        ComputedStyle {
            display: Display::Block,
            width: Dimension::Px(400.0),
            height: Dimension::Auto,
            ..ComputedStyle::default()
        },
        vec![current],
    );

    root.compute_layout(400.0, 600.0);
    assert!(root.geometry.border_box.width.is_finite());
    assert!(root.geometry.border_box.height.is_finite());

    strata_layout::box_model::clear_inline_descendant_line_boxes(&mut root);
    drop(root);
}

#[test]
fn test_5000_nested_blocks_intrinsic_min_max_sizes() {
    let mut current = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        width: Dimension::Px(250.0),
        height: Dimension::Px(60.0),
        ..ComputedStyle::default()
    });

    for _ in 0..5_000 {
        let parent = LayoutNode::with_children(
            ComputedStyle {
                display: Display::Block,
                width: Dimension::Auto,
                height: Dimension::Auto,
                ..ComputedStyle::default()
            },
            vec![current],
        );
        current = parent;
    }

    let sizes = compute_min_max_sizes(&current, Some(800.0));
    assert!(
        sizes.min_content.is_finite(),
        "Intrinsic min_content must be finite"
    );
    assert!(
        sizes.max_content.is_finite(),
        "Intrinsic max_content must be finite"
    );

    drop(current);
}

#[test]
fn test_custom_depth_guard_exhaustion() {
    use strata_layout::tree::LayoutDepthGuard;

    assert_eq!(MAX_LAYOUT_DEPTH, 512);
    let root_guard = LayoutDepthGuard::root();
    assert_eq!(root_guard.current_depth(), 0);
    assert!(!root_guard.is_exhausted());

    let guard = LayoutDepthGuard::with_max_depth(3);
    assert!(!guard.is_exhausted());
    assert_eq!(guard.current_depth(), 0);

    let g1 = guard.enter().expect("depth 1");
    assert_eq!(g1.current_depth(), 1);
    assert!(!g1.is_exhausted());

    let g2 = g1.enter().expect("depth 2");
    assert_eq!(g2.current_depth(), 2);
    assert!(!g2.is_exhausted());

    let g3 = g2.enter().expect("depth 3");
    assert_eq!(g3.current_depth(), 3);
    assert!(g3.is_exhausted());

    assert!(g3.enter().is_none(), "Depth 3 must exhaust guard with max_depth 3");
}
