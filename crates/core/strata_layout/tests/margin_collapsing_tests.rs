#![allow(clippy::field_reassign_with_default)]

use strata_layout::box_model::collapse_margins;
use strata_layout::prelude::*;
use strata_style::types::{ComputedStyle, Dimension, Display, Overflow};

#[test]
fn test_css_margin_collapsing_basic_math() {
    // Both positive: max(m1, m2)
    assert_eq!(collapse_margins(20.0, 30.0), 30.0);
    assert_eq!(collapse_margins(50.0, 10.0), 50.0);

    // Both negative: most negative (-max(|m1|, |m2|))
    assert_eq!(collapse_margins(-15.0, -25.0), -25.0);
    assert_eq!(collapse_margins(-40.0, -10.0), -40.0);

    // One positive, one negative: max_pos + min_neg
    assert_eq!(collapse_margins(30.0, -10.0), 20.0);
    assert_eq!(collapse_margins(10.0, -30.0), -20.0);
    assert_eq!(collapse_margins(0.0, -15.0), -15.0);
    assert_eq!(collapse_margins(25.0, 0.0), 25.0);
}

#[test]
fn test_empty_block_margin_collapsing_through() {
    let mut root_style = ComputedStyle::default();
    root_style.display = Display::Block;
    root_style.width = Dimension::Px(300.0);
    root_style.height = Dimension::Auto;

    // Child 1: height 50, margin-bottom 20
    let mut s1 = ComputedStyle::default();
    s1.display = Display::Block;
    s1.width = Dimension::Px(200.0);
    s1.height = Dimension::Px(50.0);
    s1.margin.bottom = 20.0;

    // Child 2: Empty block (height 0, margin-top 30, margin-bottom 40)
    let mut s2 = ComputedStyle::default();
    s2.display = Display::Block;
    s2.width = Dimension::Px(200.0);
    s2.height = Dimension::Px(0.0);
    s2.margin.top = 30.0;
    s2.margin.bottom = 40.0;

    // Child 3: height 50, margin-top 10
    let mut s3 = ComputedStyle::default();
    s3.display = Display::Block;
    s3.width = Dimension::Px(200.0);
    s3.height = Dimension::Px(50.0);
    s3.margin.top = 10.0;

    let node1 = LayoutNode::new(s1);
    let node2 = LayoutNode::new(s2);
    let node3 = LayoutNode::new(s3);

    let mut root = LayoutNode::with_children(root_style, vec![node1, node2, node3]);
    root.compute_layout(300.0, 600.0);

    // Child 1: top at 0, bottom at 50
    assert_eq!(root.children[0].geometry.border_box.y, 0.0);
    assert_eq!(root.children[0].geometry.border_box.height, 50.0);

    // Adjoining margins across Child 1 (mb=20), Empty Child 2 (mt=30, mb=40), and Child 3 (mt=10)
    // collapse together into a single margin: max(20, 30, 40, 10) = 40.0.
    // Child 3 must start at y = 50 + 40 = 90.0 (NOT 50 + max(20,30) + max(40,10) = 50 + 30 + 40 = 120.0).
    assert_eq!(
        root.children[2].geometry.border_box.y,
        90.0,
        "Child 3 must be positioned at 90.0 after single collapsed margin through empty block"
    );
    assert_eq!(root.children[2].geometry.border_box.height, 50.0);
}

#[test]
fn test_multiple_consecutive_empty_blocks_collapsing_through() {
    let mut root_style = ComputedStyle::default();
    root_style.display = Display::Block;
    root_style.width = Dimension::Px(300.0);
    root_style.height = Dimension::Auto;

    // Sibling 1: h=40, mb=15
    let mut s1 = ComputedStyle::default();
    s1.display = Display::Block;
    s1.height = Dimension::Px(40.0);
    s1.margin.bottom = 15.0;

    // Empty Sibling 2: mt=25, mb=10
    let mut s2 = ComputedStyle::default();
    s2.display = Display::Block;
    s2.height = Dimension::Px(0.0);
    s2.margin.top = 25.0;
    s2.margin.bottom = 10.0;

    // Empty Sibling 3: mt=35, mb=5
    let mut s3 = ComputedStyle::default();
    s3.display = Display::Block;
    s3.height = Dimension::Px(0.0);
    s3.margin.top = 35.0;
    s3.margin.bottom = 5.0;

    // Sibling 4: h=40, mt=20
    let mut s4 = ComputedStyle::default();
    s4.display = Display::Block;
    s4.height = Dimension::Px(40.0);
    s4.margin.top = 20.0;

    let mut root = LayoutNode::with_children(
        root_style,
        vec![
            LayoutNode::new(s1),
            LayoutNode::new(s2),
            LayoutNode::new(s3),
            LayoutNode::new(s4),
        ],
    );
    root.compute_layout(300.0, 600.0);

    // Collapsed margin is max(15, 25, 10, 35, 5, 20) = 35.0
    // Sibling 4 starts at 40 + 35 = 75.0
    assert_eq!(root.children[0].geometry.border_box.y, 0.0);
    assert_eq!(root.children[0].geometry.border_box.height, 40.0);
    assert_eq!(
        root.children[3].geometry.border_box.y,
        75.0,
        "Sibling 4 must be positioned at 75.0 across multiple empty blocks"
    );
}

#[test]
fn test_parent_first_child_top_margin_collapsing() {
    let mut container_style = ComputedStyle::default();
    container_style.display = Display::Block;
    container_style.width = Dimension::Px(300.0);
    container_style.height = Dimension::Auto;
    container_style.margin.top = 10.0;
    // border = 0, padding = 0

    let mut child_style = ComputedStyle::default();
    child_style.display = Display::Block;
    child_style.width = Dimension::Px(200.0);
    child_style.height = Dimension::Px(50.0);
    child_style.margin.top = 30.0;

    let mut container = LayoutNode::with_children(container_style, vec![LayoutNode::new(child_style)]);
    container.compute_layout(300.0, 600.0);

    // Child top margin (30) collapses with parent top margin (10) -> parent effective margin.top = 30.
    // Child border box sits at y = 0 relative to parent!
    assert_eq!(
        container.children[0].geometry.border_box.y,
        0.0,
        "First child must sit at y = 0 relative to parent when top margins collapse"
    );
    assert_eq!(
        container.style.margin.top,
        30.0,
        "Parent effective top margin must be collapsed to max(10, 30) = 30"
    );
}

#[test]
fn test_parent_last_child_bottom_margin_collapsing() {
    let mut container_style = ComputedStyle::default();
    container_style.display = Display::Block;
    container_style.width = Dimension::Px(300.0);
    container_style.height = Dimension::Auto;
    container_style.margin.bottom = 15.0;
    // border = 0, padding = 0

    let mut child_style = ComputedStyle::default();
    child_style.display = Display::Block;
    child_style.width = Dimension::Px(200.0);
    child_style.height = Dimension::Px(60.0);
    child_style.margin.bottom = 35.0;

    let mut container = LayoutNode::with_children(container_style, vec![LayoutNode::new(child_style)]);
    container.compute_layout(300.0, 600.0);

    // Parent height wraps child tightly without expanding for the collapsed bottom margin
    assert_eq!(
        container.geometry.border_box.height,
        60.0,
        "Parent auto height must not include the bottom margin that collapsed with parent"
    );
    assert_eq!(
        container.style.margin.bottom,
        35.0,
        "Parent effective bottom margin must be collapsed to max(15, 35) = 35"
    );
}

#[test]
fn test_bfc_boundary_prevents_parent_child_collapsing() {
    let mut bfc_style = ComputedStyle::default();
    bfc_style.display = Display::Block;
    bfc_style.width = Dimension::Px(300.0);
    bfc_style.height = Dimension::Auto;
    bfc_style.overflow_x = Overflow::Hidden;
    bfc_style.overflow_y = Overflow::Hidden; // Establishes BFC!
    bfc_style.margin.top = 10.0;

    let mut child_style = ComputedStyle::default();
    child_style.display = Display::Block;
    child_style.width = Dimension::Px(200.0);
    child_style.height = Dimension::Px(50.0);
    child_style.margin.top = 30.0;

    let mut bfc_container = LayoutNode::with_children(bfc_style, vec![LayoutNode::new(child_style)]);
    bfc_container.compute_layout(300.0, 600.0);

    // BFC root prevents parent-child margin collapsing:
    // Child is offset by its own margin-top (30) inside the container
    assert_eq!(
        bfc_container.children[0].geometry.border_box.y,
        30.0,
        "BFC root container must prevent child margin from collapsing with parent"
    );
    assert_eq!(
        bfc_container.style.margin.top,
        10.0,
        "BFC container top margin remains 10"
    );
}
