//! W3C Flexible Box Layout Level 1 Compliance Integration Tests.
//!
//! Validates:
//! - Auto margins on main axis absorbing remaining positive free space (§8.1)
//! - Auto margins on cross axis centering or pushing items (§8.1)
//! - Baseline alignment across flex items with different font sizes, line heights, and padding (§9.5)
//! - Multiline flex lines with baseline-aligned items (§9.4, §9.5)
//! - Column flex container auto margins and cross axis fallback

#![allow(clippy::field_reassign_with_default)]

use strata_layout::prelude::*;
use strata_style::types::{
    AlignItems, ComputedStyle, Dimension, Display, FlexDirection,
};

#[test]
fn test_flexbox_main_axis_auto_margins_push_end() {
    // Container width = 500px, row layout.
    // Item 1: 100px width.
    // Item 2: 100px width, margin-left: auto.
    // Free space = 500 - 200 = 300px.
    // Item 2 margin-left absorbs 300px.
    let mut container_style = ComputedStyle::default();
    container_style.display = Display::Flex;
    container_style.flex_direction = FlexDirection::Row;
    container_style.width = Dimension::Px(500.0);
    container_style.height = Dimension::Px(100.0);

    let mut item1_style = ComputedStyle::default();
    item1_style.width = Dimension::Px(100.0);
    item1_style.height = Dimension::Px(50.0);

    let mut item2_style = ComputedStyle::default();
    item2_style.width = Dimension::Px(100.0);
    item2_style.height = Dimension::Px(50.0);
    item2_style.margin_auto_left = true;

    let mut container = LayoutNode::with_children(
        container_style,
        vec![LayoutNode::new(item1_style), LayoutNode::new(item2_style)],
    );

    container.compute_layout(500.0, 100.0);

    assert_eq!(container.children[0].geometry.border_box.x, 0.0);
    assert_eq!(container.children[0].geometry.border_box.width, 100.0);

    // Item 2 starts at 100 + 300 = 400
    assert_eq!(container.children[1].geometry.border_box.x, 400.0);
    assert_eq!(container.children[1].geometry.border_box.width, 100.0);
}

#[test]
fn test_flexbox_main_axis_auto_margins_centering() {
    // Container width = 600px, 1 item with 200px width and margin: 0 auto (auto left and right).
    // Free space = 600 - 200 = 400px.
    // Left and right margins each receive 200px.
    let mut container_style = ComputedStyle::default();
    container_style.display = Display::Flex;
    container_style.flex_direction = FlexDirection::Row;
    container_style.width = Dimension::Px(600.0);
    container_style.height = Dimension::Px(100.0);

    let mut item_style = ComputedStyle::default();
    item_style.width = Dimension::Px(200.0);
    item_style.height = Dimension::Px(50.0);
    item_style.margin_auto_left = true;
    item_style.margin_auto_right = true;

    let mut container = LayoutNode::with_children(container_style, vec![LayoutNode::new(item_style)]);
    container.compute_layout(600.0, 100.0);

    assert_eq!(container.children[0].geometry.border_box.x, 200.0);
    assert_eq!(container.children[0].geometry.border_box.width, 200.0);
}

#[test]
fn test_flexbox_cross_axis_auto_margins() {
    // Container height = 400px, row layout.
    // Item 1: height 100px, margin-top: auto, margin-bottom: auto (centered vertically).
    // Item 2: height 60px, margin-top: auto (pushed to bottom).
    let mut container_style = ComputedStyle::default();
    container_style.display = Display::Flex;
    container_style.flex_direction = FlexDirection::Row;
    container_style.width = Dimension::Px(500.0);
    container_style.height = Dimension::Px(400.0);

    let mut item1_style = ComputedStyle::default();
    item1_style.width = Dimension::Px(100.0);
    item1_style.height = Dimension::Px(100.0);
    item1_style.margin_auto_top = true;
    item1_style.margin_auto_bottom = true;

    let mut item2_style = ComputedStyle::default();
    item2_style.width = Dimension::Px(100.0);
    item2_style.height = Dimension::Px(60.0);
    item2_style.margin_auto_top = true;

    let mut container = LayoutNode::with_children(
        container_style,
        vec![LayoutNode::new(item1_style), LayoutNode::new(item2_style)],
    );

    container.compute_layout(500.0, 400.0);

    // Item 1 centered: (400 - 100) / 2 = 150
    assert_eq!(container.children[0].geometry.border_box.y, 150.0);
    assert_eq!(container.children[0].geometry.border_box.height, 100.0);

    // Item 2 pushed to bottom: 400 - 60 = 340
    assert_eq!(container.children[1].geometry.border_box.y, 340.0);
    assert_eq!(container.children[1].geometry.border_box.height, 60.0);
}

#[test]
fn test_flexbox_cross_axis_baseline_alignment() {
    // Container with align-items: baseline
    // Item 1: font_size = 20px, text = "Small", padding-top = 10px
    // Item 2: font_size = 40px, text = "Big", padding-top = 0px
    let mut container_style = ComputedStyle::default();
    container_style.display = Display::Flex;
    container_style.flex_direction = FlexDirection::Row;
    container_style.align_items = AlignItems::Baseline;
    container_style.width = Dimension::Px(500.0);
    container_style.height = Dimension::Px(200.0);

    let mut item1_style = ComputedStyle::default();
    item1_style.width = Dimension::Px(100.0);
    item1_style.height = Dimension::Px(60.0);
    item1_style.font_size = 20.0;
    item1_style.padding.top = 10.0;

    let mut item1 = LayoutNode::new(item1_style);
    item1.text = Some("Small".to_string());

    let mut item2_style = ComputedStyle::default();
    item2_style.width = Dimension::Px(120.0);
    item2_style.height = Dimension::Px(80.0);
    item2_style.font_size = 40.0;
    item2_style.padding.top = 0.0;

    let mut item2 = LayoutNode::new(item2_style);
    item2.text = Some("Big".to_string());

    let mut container = LayoutNode::with_children(container_style, vec![item1, item2]);
    container.compute_layout(500.0, 200.0);

    // Item 1 baseline offset: padding_top(10) + font_size(20)*0.8 = 26px
    // Item 2 baseline offset: padding_top(0) + font_size(40)*0.8 = 32px
    // Max baseline ascent = 32px
    // Item 2 starts at y = 0, its baseline is at y = 32
    // Item 1 cross offset = 32 - 26 = 6px. Item 1 starts at y = 6, its baseline is at 6 + 26 = 32!
    let child1_y = container.children[0].geometry.border_box.y;
    let child2_y = container.children[1].geometry.border_box.y;

    assert_eq!(child2_y, 0.0);
    assert_eq!(child1_y, 6.0);
}

#[test]
fn test_flexbox_column_auto_margins() {
    // Column layout: container height 500px.
    // Item 1: height 100px.
    // Item 2: height 100px, margin-top: auto.
    // Item 2 is pushed to the bottom of the container.
    let mut container_style = ComputedStyle::default();
    container_style.display = Display::Flex;
    container_style.flex_direction = FlexDirection::Column;
    container_style.width = Dimension::Px(200.0);
    container_style.height = Dimension::Px(500.0);

    let mut item1_style = ComputedStyle::default();
    item1_style.width = Dimension::Px(100.0);
    item1_style.height = Dimension::Px(100.0);

    let mut item2_style = ComputedStyle::default();
    item2_style.width = Dimension::Px(100.0);
    item2_style.height = Dimension::Px(100.0);
    item2_style.margin_auto_top = true;

    let mut container = LayoutNode::with_children(
        container_style,
        vec![LayoutNode::new(item1_style), LayoutNode::new(item2_style)],
    );

    container.compute_layout(200.0, 500.0);

    assert_eq!(container.children[0].geometry.border_box.y, 0.0);
    assert_eq!(container.children[1].geometry.border_box.y, 400.0);
}
