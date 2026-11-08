//! Verification Suite for CSS Positioning, Containing Blocks, and Stacking Schemes.

#![allow(clippy::field_reassign_with_default)]

use strata_layout::{ContainingBlock, LayoutNode, Rect};
use strata_primitives::geom::EdgeInsets;
use strata_style::types::{ComputedStyle, Dimension, Display, Position};

#[test]
fn test_containing_block_resolution_across_static_ancestors() {
    // Relative outer container (800 x 600)
    let mut root_style = ComputedStyle::default();
    root_style.display = Display::Block;
    root_style.position = Position::Relative;
    root_style.width = Dimension::Px(800.0);
    root_style.height = Dimension::Px(600.0);

    // Static intermediate wrapper with offset padding
    let mut static_style = ComputedStyle::default();
    static_style.display = Display::Block;
    static_style.position = Position::Static;
    static_style.margin = EdgeInsets::all(40.0);
    static_style.width = Dimension::Px(400.0);
    static_style.height = Dimension::Px(300.0);

    // Absolute child inside static wrapper: top: 15, right: 25, w: 120, h: 60
    let mut abs_style = ComputedStyle::default();
    abs_style.display = Display::Block;
    abs_style.position = Position::Absolute;
    abs_style.insets.top = Dimension::Px(15.0);
    abs_style.insets.right = Dimension::Px(25.0);
    abs_style.width = Dimension::Px(120.0);
    abs_style.height = Dimension::Px(60.0);

    let abs_node = LayoutNode::new(abs_style);
    let static_node = LayoutNode::with_children(static_style, vec![abs_node]);
    let mut root = LayoutNode::with_children(root_style, vec![static_node]);

    root.compute_layout(800.0, 600.0);

    // The absolute child's containing block must be root's padding box (0, 0, 800, 600)
    // rather than the intermediate static container.
    // x = root.x + root.w - right - w = 0 + 800 - 25 - 120 = 655.0
    // y = root.y + top = 0 + 15 = 15.0
    let abs_geom = &root.children[0].children[0].geometry.border_box;
    assert_eq!(abs_geom.x, 655.0);
    assert_eq!(abs_geom.y, 15.0);
    assert_eq!(abs_geom.width, 120.0);
    assert_eq!(abs_geom.height, 60.0);
}

#[test]
fn test_double_anchored_horizontal_and_vertical_stretch() {
    // Container: 600 x 500
    let mut root_style = ComputedStyle::default();
    root_style.display = Display::Block;
    root_style.position = Position::Relative;
    root_style.width = Dimension::Px(600.0);
    root_style.height = Dimension::Px(500.0);

    // Inset-stretched child: top: 20, bottom: 30, left: 40, right: 50, auto dimensions
    let mut child_style = ComputedStyle::default();
    child_style.display = Display::Block;
    child_style.position = Position::Absolute;
    child_style.insets.top = Dimension::Px(20.0);
    child_style.insets.bottom = Dimension::Px(30.0);
    child_style.insets.left = Dimension::Px(40.0);
    child_style.insets.right = Dimension::Px(50.0);
    child_style.width = Dimension::Auto;
    child_style.height = Dimension::Auto;

    let mut root = LayoutNode::with_children(root_style, vec![LayoutNode::new(child_style)]);
    root.compute_layout(600.0, 500.0);

    let child_geom = &root.children[0].geometry.border_box;
    // Expected stretched width = 600 - 40 - 50 = 510
    // Expected stretched height = 500 - 20 - 30 = 450
    assert_eq!(child_geom.x, 40.0);
    assert_eq!(child_geom.y, 20.0);
    assert_eq!(child_geom.width, 510.0);
    assert_eq!(child_geom.height, 450.0);
}

#[test]
fn test_absolute_auto_margin_centering() {
    // Container: 800 x 600
    let mut root_style = ComputedStyle::default();
    root_style.display = Display::Block;
    root_style.position = Position::Relative;
    root_style.width = Dimension::Px(800.0);
    root_style.height = Dimension::Px(600.0);

    // Centered modal dialog: inset-0 with auto margins, definite size (300 x 200)
    let mut modal_style = ComputedStyle::default();
    modal_style.display = Display::Block;
    modal_style.position = Position::Absolute;
    modal_style.insets.top = Dimension::Px(0.0);
    modal_style.insets.bottom = Dimension::Px(0.0);
    modal_style.insets.left = Dimension::Px(0.0);
    modal_style.insets.right = Dimension::Px(0.0);
    modal_style.margin_auto_top = true;
    modal_style.margin_auto_bottom = true;
    modal_style.margin_auto_left = true;
    modal_style.margin_auto_right = true;
    modal_style.width = Dimension::Px(300.0);
    modal_style.height = Dimension::Px(200.0);

    let mut root = LayoutNode::with_children(root_style, vec![LayoutNode::new(modal_style)]);
    root.compute_layout(800.0, 600.0);

    let modal_geom = &root.children[0].geometry.border_box;
    // Free horizontal space = 800 - 300 = 500 -> margin = 250 -> x = 250
    // Free vertical space = 600 - 200 = 400 -> margin = 200 -> y = 200
    assert_eq!(modal_geom.x, 250.0);
    assert_eq!(modal_geom.y, 200.0);
    assert_eq!(modal_geom.width, 300.0);
    assert_eq!(modal_geom.height, 200.0);
}

#[test]
fn test_sticky_positioning_clamping_logic() {
    let mut parent = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        position: Position::Relative,
        width: Dimension::Px(400.0),
        height: Dimension::Px(800.0),
        ..Default::default()
    });

    // In-flow header preceding the sticky element
    let header = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        width: Dimension::Px(400.0),
        height: Dimension::Px(100.0),
        ..Default::default()
    });
    parent.add_child(header);

    let mut sticky_child = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        position: Position::Sticky,
        width: Dimension::Px(400.0),
        height: Dimension::Px(50.0),
        ..Default::default()
    });
    sticky_child.style.insets.top = Dimension::Px(10.0);
    parent.add_child(sticky_child);

    // Initial layout at scroll offset (0, 0)
    parent.compute_layout(400.0, 800.0);
    // In normal flow, sticky element sits immediately below the 100px header at y=100
    assert_eq!(parent.children[1].geometry.border_box.y, 100.0);

    // Re-evaluate sticky positioning with scroll offset 150.0
    let cb_scrolled = ContainingBlock::with_scroll(
        parent.geometry.padding_box,
        Rect::new(0.0, 0.0, 400.0, 600.0),
        0.0,
        150.0,
    );
    strata_layout::solve_positioning_children(&mut parent, cb_scrolled);

    // Target y = scroll 150 + top 10 = 160.0, within parent bounds [100, 750]
    assert_eq!(parent.children[1].geometry.border_box.y, 160.0);

    // Re-evaluate sticky positioning with extreme scroll offset 900.0
    let cb_overscrolled = ContainingBlock::with_scroll(
        parent.geometry.padding_box,
        Rect::new(0.0, 0.0, 400.0, 600.0),
        0.0,
        900.0,
    );
    strata_layout::solve_positioning_children(&mut parent, cb_overscrolled);

    // Must be clamped to parent bottom (800) - child height (50) = 750.0
    assert_eq!(parent.children[1].geometry.border_box.y, 750.0);
}

#[test]
fn test_percentage_insets_resolution() {
    let mut root_style = ComputedStyle::default();
    root_style.display = Display::Block;
    root_style.position = Position::Relative;
    root_style.width = Dimension::Px(1000.0);
    root_style.height = Dimension::Px(800.0);

    let mut abs_style = ComputedStyle::default();
    abs_style.display = Display::Block;
    abs_style.position = Position::Absolute;
    abs_style.insets.left = Dimension::Percent(20.0);
    abs_style.insets.top = Dimension::Percent(25.0);
    abs_style.width = Dimension::Percent(50.0);
    abs_style.height = Dimension::Percent(40.0);

    let mut root = LayoutNode::with_children(root_style, vec![LayoutNode::new(abs_style)]);
    root.compute_layout(1000.0, 800.0);

    let child_geom = &root.children[0].geometry.border_box;
    // left = 20% of 1000 = 200
    // top = 25% of 800 = 200
    // width = 50% of 1000 = 500
    // height = 40% of 800 = 320
    assert_eq!(child_geom.x, 200.0);
    assert_eq!(child_geom.y, 200.0);
    assert_eq!(child_geom.width, 500.0);
    assert_eq!(child_geom.height, 320.0);
}

#[test]
fn test_sticky_element_clamped_to_static_parent_container() {
    // Grandparent: Relative with height 3000px
    let mut grandparent = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        position: Position::Relative,
        width: Dimension::Px(600.0),
        height: Dimension::Px(3000.0),
        ..Default::default()
    });

    // Parent: Static block container with height 400px
    let mut parent = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        position: Position::Static,
        width: Dimension::Px(600.0),
        height: Dimension::Px(400.0),
        ..Default::default()
    });

    // Sticky child with height 50px, top 0px
    let mut sticky_child = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        position: Position::Sticky,
        width: Dimension::Px(600.0),
        height: Dimension::Px(50.0),
        ..Default::default()
    });
    sticky_child.style.insets.top = Dimension::Px(0.0);

    parent.add_child(sticky_child);
    grandparent.add_child(parent);

    // Initial layout
    grandparent.compute_layout(600.0, 3000.0);

    // Scroll by 600px
    let cb_scrolled = ContainingBlock::with_scroll(
        grandparent.geometry.padding_box,
        Rect::new(0.0, 0.0, 600.0, 800.0),
        0.0,
        600.0,
    );
    strata_layout::solve_positioning_children(&mut grandparent, cb_scrolled);

    // Child must be clamped to parent bottom (400) - child height (50) = 350.0,
    // NOT allowed to escape parent into the 3000px grandparent!
    let sticky_geom = &grandparent.children[0].children[0].geometry.border_box;
    assert_eq!(sticky_geom.y, 350.0, "Sticky child must be clamped to static parent container bottom");
}

