//! Integration Tests: Native Scroll Containers & Overflow Management.
//!
//! Validates `scroll_bounds`, `has_scroll`, `max_scroll`, coordinate clamping,
//! and scroll translation across block and flex containers.

use strata_layout::prelude::*;
use strata_style::prelude::*;

#[test]
fn test_block_scroll_container_vertical_overflow() {
    let mut container = LayoutNode::new(ComputedStyle::default());
    container.style.display = Display::Block;
    container.style.width = Dimension::Px(400.0);
    container.style.height = Dimension::Px(300.0);
    container.style.overflow_y = Overflow::Auto;

    // 4 items of 100px height each -> total content height 400px
    for _ in 0..4 {
        let mut item = LayoutNode::new(ComputedStyle::default());
        item.style.display = Display::Block;
        item.style.width = Dimension::Px(400.0);
        item.style.height = Dimension::Px(100.0);
        container.add_child(item);
    }

    container.compute_layout(800.0, 600.0);

    // Container border box is 400x300
    assert_eq!(container.geometry.border_box.width, 400.0);
    assert_eq!(container.geometry.border_box.height, 300.0);

    // Scroll bounds extend to cover all 4 children (400.0px)
    assert!(container.geometry.scroll_bounds.height >= 400.0);
    assert_eq!(container.geometry.has_scroll, (false, true));

    // Maximum vertical scroll is 400 - 300 = 100px
    let max = container.geometry.max_scroll();
    assert_eq!(max.x, 0.0);
    assert!(max.y >= 100.0);
}

#[test]
fn test_flex_scroll_container_horizontal_overflow() {
    let mut container = LayoutNode::new(ComputedStyle::default());
    container.style.display = Display::Flex;
    container.style.flex_direction = FlexDirection::Row;
    container.style.width = Dimension::Px(500.0);
    container.style.height = Dimension::Px(150.0);
    container.style.overflow_x = Overflow::Scroll;

    // 5 children of 150px width each -> total 750px
    for _ in 0..5 {
        let mut item = LayoutNode::new(ComputedStyle::default());
        item.style.width = Dimension::Px(150.0);
        item.style.height = Dimension::Px(100.0);
        item.style.flex_shrink = 0.0;
        container.add_child(item);
    }

    container.compute_layout(1000.0, 800.0);

    assert_eq!(container.geometry.border_box.width, 500.0);
    assert!(container.geometry.scroll_bounds.width >= 750.0);
    assert!(container.geometry.has_scroll.0);

    let max = container.geometry.max_scroll();
    assert!(max.x >= 250.0);
}

#[test]
fn test_overflow_visible_does_not_activate_scroll() {
    let mut container = LayoutNode::new(ComputedStyle::default());
    container.style.display = Display::Block;
    container.style.width = Dimension::Px(200.0);
    container.style.height = Dimension::Px(200.0);
    container.style.overflow_x = Overflow::Visible;
    container.style.overflow_y = Overflow::Visible;

    let mut item = LayoutNode::new(ComputedStyle::default());
    item.style.width = Dimension::Px(500.0);
    item.style.height = Dimension::Px(500.0);
    container.add_child(item);

    container.compute_layout(800.0, 600.0);

    assert_eq!(container.geometry.has_scroll, (false, false));
    assert!(!container.geometry.is_scroll_container());
}

#[test]
fn test_scroll_offset_clamping() {
    let mut container = LayoutNode::new(ComputedStyle::default());
    container.style.display = Display::Block;
    container.style.width = Dimension::Px(300.0);
    container.style.height = Dimension::Px(300.0);
    container.style.overflow_y = Overflow::Scroll;

    let mut item = LayoutNode::new(ComputedStyle::default());
    item.style.width = Dimension::Px(300.0);
    item.style.height = Dimension::Px(500.0);
    container.add_child(item);

    container.compute_layout(800.0, 600.0);

    let max = container.geometry.max_scroll();
    assert_eq!(max.y, 200.0);

    // Negative offset clamps to 0
    let clamped_neg = container.geometry.clamp_scroll_offset(Point::new(-20.0, -50.0));
    assert_eq!(clamped_neg, Point::new(0.0, 0.0));

    // Excessive offset clamps to max_scroll
    let clamped_excess = container.geometry.clamp_scroll_offset(Point::new(100.0, 999.0));
    assert_eq!(clamped_excess, Point::new(0.0, 200.0));

    // In-range offset preserves value
    let clamped_valid = container.geometry.clamp_scroll_offset(Point::new(0.0, 120.0));
    assert_eq!(clamped_valid, Point::new(0.0, 120.0));
}
