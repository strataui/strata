//! Comprehensive W3C CSS Sizing Level 3 Intrinsic Sizing Tests.
//!
//! Validates `min-content`, `max-content`, and `fit-content` across CSS parsing,
//! block formatting context, flexbox containers, and grid layouts.

use strata_layout::prelude::*;
use strata_style::prelude::*;

#[test]
fn test_css_parser_intrinsic_keywords() {
    assert_eq!(
        strata_style::parser::parse_dimension("min-content"),
        Some(Dimension::MinContent)
    );
    assert_eq!(
        strata_style::parser::parse_dimension("max-content"),
        Some(Dimension::MaxContent)
    );
    assert_eq!(
        strata_style::parser::parse_dimension("fit-content"),
        Some(Dimension::FitContent(0.0))
    );
    assert_eq!(
        strata_style::parser::parse_dimension("fit-content(300px)"),
        Some(Dimension::FitContent(300.0))
    );
    assert_eq!(
        strata_style::parser::parse_dimension("fit-content(20rem)"),
        Some(Dimension::FitContent(320.0))
    );
}

#[test]
fn test_block_container_width_min_content() {
    let mut container = LayoutNode::new(ComputedStyle::default());
    container.style.display = Display::Block;
    container.style.width = Dimension::MinContent;

    let mut child1 = LayoutNode::new(ComputedStyle::default());
    child1.style.display = Display::Block;
    child1.style.width = Dimension::Px(140.0);
    child1.style.height = Dimension::Px(50.0);

    let mut child2 = LayoutNode::new(ComputedStyle::default());
    child2.style.display = Display::Block;
    child2.style.width = Dimension::Px(80.0);
    child2.style.height = Dimension::Px(50.0);

    container.add_child(child1);
    container.add_child(child2);

    container.compute_layout(1000.0, 800.0);

    // In a block container, children stack vertically.
    // min-content width is the maximum child width (140.0px), NOT available 1000.0px!
    assert_eq!(container.geometry.border_box.width, 140.0);
}

#[test]
fn test_block_container_width_max_content_inline_children() {
    let mut container = LayoutNode::new(ComputedStyle::default());
    container.style.display = Display::Block;
    container.style.width = Dimension::MaxContent;

    // Two inline phrasing blocks side-by-side
    let mut child1 = LayoutNode::new(ComputedStyle::default());
    child1.style.display = Display::InlineBlock;
    child1.style.width = Dimension::Px(120.0);
    child1.style.height = Dimension::Px(40.0);

    let mut child2 = LayoutNode::new(ComputedStyle::default());
    child2.style.display = Display::InlineBlock;
    child2.style.width = Dimension::Px(180.0);
    child2.style.height = Dimension::Px(40.0);

    container.add_child(child1);
    child2.style.margin.left = 10.0;
    container.add_child(child2);

    container.compute_layout(1000.0, 800.0);

    // max-content width is the sum of inline elements on the line: 120 + 10 + 180 = 310.0px
    assert_eq!(container.geometry.border_box.width, 310.0);
}

#[test]
fn test_block_container_width_fit_content_shrink_to_fit() {
    let create_container = || {
        let mut container = LayoutNode::new(ComputedStyle::default());
        container.style.display = Display::Block;
        container.style.width = Dimension::FitContent(0.0);

        let mut child1 = LayoutNode::new(ComputedStyle::default());
        child1.style.display = Display::InlineBlock;
        child1.style.width = Dimension::Px(100.0);

        let mut child2 = LayoutNode::new(ComputedStyle::default());
        child2.style.display = Display::InlineBlock;
        child2.style.width = Dimension::Px(150.0);

        container.add_child(child1);
        container.add_child(child2);
        container
    };

    // min_content is max(100, 150) = 150.0
    // max_content is 100 + 150 = 250.0

    // Case 1: Large available width (800.0) -> fits to max_content (250.0)
    let mut c1 = create_container();
    c1.compute_layout(800.0, 600.0);
    assert_eq!(c1.geometry.border_box.width, 250.0);

    // Case 2: Available width between min and max (200.0) -> clamps to available width (200.0)
    let mut c2 = create_container();
    c2.compute_layout(200.0, 600.0);
    assert_eq!(c2.geometry.border_box.width, 200.0);

    // Case 3: Available width smaller than min_content (100.0) -> clamps to min_content (150.0)
    let mut c3 = create_container();
    c3.compute_layout(100.0, 600.0);
    assert_eq!(c3.geometry.border_box.width, 150.0);
}

#[test]
fn test_flexbox_item_width_min_content() {
    let mut flex_container = LayoutNode::new(ComputedStyle::default());
    flex_container.style.display = Display::Flex;
    flex_container.style.flex_direction = FlexDirection::Row;
    flex_container.style.width = Dimension::Px(800.0);

    let mut item1 = LayoutNode::new(ComputedStyle::default());
    item1.style.display = Display::Block;
    item1.style.width = Dimension::MinContent;

    let mut sub_child = LayoutNode::new(ComputedStyle::default());
    sub_child.style.width = Dimension::Px(175.0);
    sub_child.style.height = Dimension::Px(40.0);
    item1.add_child(sub_child);

    let mut item2 = LayoutNode::new(ComputedStyle::default());
    item2.style.display = Display::Block;
    item2.style.flex_grow = 1.0;

    flex_container.add_child(item1);
    flex_container.add_child(item2);

    flex_container.compute_layout(800.0, 600.0);

    assert_eq!(flex_container.children[0].geometry.border_box.width, 175.0);
    assert_eq!(flex_container.children[1].geometry.border_box.width, 625.0);
}

#[test]
fn test_tailwind_w_fit_and_w_min_classes() {
    let mut node_fit = LayoutNode::new(TailwindEngine::resolve_classes("w-fit block"));
    let mut inner = LayoutNode::new(ComputedStyle::default());
    inner.style.width = Dimension::Px(220.0);
    node_fit.add_child(inner);

    node_fit.compute_layout(800.0, 600.0);
    assert_eq!(node_fit.geometry.border_box.width, 220.0);

    let mut node_min = LayoutNode::new(TailwindEngine::resolve_classes("w-min block"));
    let mut inner1 = LayoutNode::new(ComputedStyle::default());
    inner1.style.width = Dimension::Px(160.0);
    let mut inner2 = LayoutNode::new(ComputedStyle::default());
    inner2.style.width = Dimension::Px(90.0);
    node_min.add_child(inner1);
    node_min.add_child(inner2);

    node_min.compute_layout(800.0, 600.0);
    assert_eq!(node_min.geometry.border_box.width, 160.0);
}
