use strata_layout::prelude::*;
use strata_layout::exclusions::{ExclusionKind, ExclusionSpace};
use strata_style::types::*;

#[test]
fn test_exclusion_space_opportunity_query() {
    let mut es = ExclusionSpace::new();

    // Left float: [0, 0, 100, 100]
    es.add_exclusion(
        Rect::new(0.0, 0.0, 100.0, 100.0),
        ExclusionKind::FloatLeft,
    );

    // Right float: [350, 0, 150, 150] in 500px container
    es.add_exclusion(
        Rect::new(350.0, 0.0, 150.0, 150.0),
        ExclusionKind::FloatRight,
    );

    let container_w = 500.0;

    // Line 1: y = 20..40 (Both floats active)
    let opp1 = es.line_opportunity(20.0, 20.0, container_w);
    assert_eq!(opp1.start_x, 100.0);
    assert_eq!(opp1.right_float_edge, 350.0);
    assert_eq!(opp1.available_width, 250.0);

    // Line 2: y = 110..130 (Left float ended, right float still active)
    let opp2 = es.line_opportunity(110.0, 20.0, container_w);
    assert_eq!(opp2.start_x, 0.0);
    assert_eq!(opp2.right_float_edge, 350.0);
    assert_eq!(opp2.available_width, 350.0);

    // Line 3: y = 180..200 (Both floats ended)
    let opp3 = es.line_opportunity(180.0, 20.0, container_w);
    assert_eq!(opp3.start_x, 0.0);
    assert_eq!(opp3.right_float_edge, 500.0);
    assert_eq!(opp3.available_width, 500.0);
}

#[test]
fn test_clearance_calculation() {
    let mut es = ExclusionSpace::new();

    // Left float height 100
    es.add_exclusion(
        Rect::new(0.0, 0.0, 120.0, 100.0),
        ExclusionKind::FloatLeft,
    );

    // Right float height 160
    es.add_exclusion(
        Rect::new(280.0, 0.0, 120.0, 160.0),
        ExclusionKind::FloatRight,
    );

    assert_eq!(es.clearance_offset(Clear::Left, 40.0), 100.0);
    assert_eq!(es.clearance_offset(Clear::Right, 40.0), 160.0);
    assert_eq!(es.clearance_offset(Clear::Both, 40.0), 160.0);
    assert_eq!(es.clearance_offset(Clear::None, 40.0), 40.0);
    assert_eq!(es.clearance_offset(Clear::Left, 120.0), 120.0);
}

#[test]
fn test_ifc_line_wrapping_around_float() {
    let mut es = ExclusionSpace::new();

    // Right float occupying [250, 0, 150, 45] in a 400px container
    es.add_exclusion(
        Rect::new(250.0, 0.0, 150.0, 45.0),
        ExclusionKind::FloatRight,
    );

    let ifc = InlineFormattingContext::new();
    let text = "The Rust programming language is blazingly fast and memory-efficient. With no runtime or garbage collector, it can power performance-critical services and run on embedded devices.";
    let style = InlineTextStyle::default();
    let items = vec![InlineItem::text(text, style)];

    let options = InlineLayoutOptions {
        font_size: 14.0,
        line_height: Some(20.0),
        max_width: Some(400.0),
        white_space: WhiteSpaceMode::Normal,
        vertical_align: VerticalAlignment::Baseline,
        is_monospace: false,
        exclusion_space: Some(&es),
        bfc_offset_y: 0.0,
    };

    let res = ifc.layout_inline_stream(&items, &options);
    assert!(res.lines.len() >= 4, "Expected at least 4 wrapped lines");

    // Lines 1 and 2 (within y = 0..45) must be narrowed to fit beside the 150px right float (max available = 250px)
    assert!(res.lines[0].bounds.width <= 250.0, "Line 0 width {} should be <= 250.0", res.lines[0].bounds.width);
    assert!(res.lines[1].bounds.width <= 250.0, "Line 1 width {} should be <= 250.0", res.lines[1].bounds.width);

    // Lines from y >= 45 should have available width up to 400px and can exceed 250px
    let later_line_expanded = res.lines[2..].iter().any(|l| l.bounds.width > 250.0);
    assert!(later_line_expanded, "Expected lines past the float bottom to expand beyond 250.0px");
}

#[test]
fn test_block_container_float_containment_clearfix() {
    let mut container = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        width: Dimension::Px(600.0),
        height: Dimension::Auto,
        ..Default::default()
    });

    // Floated right card (infobox simulation): 200px wide, 250px high
    let float_child = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        float: Float::Right,
        width: Dimension::Px(200.0),
        height: Dimension::Px(250.0),
        ..Default::default()
    });

    // Short intro paragraph: auto height, roughly 30px high
    let mut text_child = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        width: Dimension::Auto,
        height: Dimension::Auto,
        ..Default::default()
    });
    text_child.inline_items.push(InlineItem::text(
        "Introduction to systems architecture.",
        InlineTextStyle::default(),
    ));

    container.children.push(float_child);
    container.children.push(text_child);

    container.compute_layout(600.0, 1000.0);

    // Verify float is positioned at right edge (600 - 200 = 400)
    assert_eq!(container.children[0].geometry.border_box.x, 400.0);
    assert_eq!(container.children[0].geometry.border_box.y, 0.0);

    // Verify container height contains the 250px float
    assert!(
        container.geometry.border_box.height >= 250.0,
        "Container height {} must enclose 250px float",
        container.geometry.border_box.height
    );
}

#[test]
fn test_clear_element_drops_below_float() {
    let mut container = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        width: Dimension::Px(500.0),
        height: Dimension::Auto,
        ..Default::default()
    });

    // Floated left box: 150px wide, 120px high
    let float_box = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        float: Float::Left,
        width: Dimension::Px(150.0),
        height: Dimension::Px(120.0),
        ..Default::default()
    });

    // Block with clear: both
    let clear_box = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        clear: Clear::Both,
        width: Dimension::Px(500.0),
        height: Dimension::Px(40.0),
        ..Default::default()
    });

    container.children.push(float_box);
    container.children.push(clear_box);

    container.compute_layout(500.0, 800.0);

    // Floated box starts at y = 0
    assert_eq!(container.children[0].geometry.border_box.y, 0.0);

    // Cleared box must be placed at or below y = 120
    assert!(
        container.children[1].geometry.border_box.y >= 120.0,
        "Cleared element y {} must be >= 120.0",
        container.children[1].geometry.border_box.y
    );
}

#[test]
fn test_infobox_table_float_and_nested_rows() {
    let mut container = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        width: Dimension::Px(800.0),
        height: Dimension::Auto,
        ..Default::default()
    });

    let mut table = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        float: Float::Right,
        width: Dimension::Px(300.0),
        height: Dimension::Auto,
        ..Default::default()
    });

    let mut tbody = LayoutNode::new(ComputedStyle {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        width: Dimension::Percent(100.0),
        height: Dimension::Auto,
        ..Default::default()
    });

    let mut row1 = LayoutNode::new(ComputedStyle {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        width: Dimension::Percent(100.0),
        height: Dimension::Auto,
        ..Default::default()
    });
    let mut th1 = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        width: Dimension::Px(112.0),
        height: Dimension::Auto,
        font_size: 14.0,
        line_height: 1.4,
        ..Default::default()
    });
    th1.text = Some("Designed by".to_string());

    let mut td1 = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        flex_grow: 1.0,
        width: Dimension::Auto,
        height: Dimension::Auto,
        font_size: 14.0,
        line_height: 1.4,
        ..Default::default()
    });
    td1.text = Some("Graydon Hoare".to_string());
    row1.children.push(th1);
    row1.children.push(td1);

    let mut row2 = LayoutNode::new(ComputedStyle {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        width: Dimension::Percent(100.0),
        height: Dimension::Auto,
        ..Default::default()
    });
    let mut th2 = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        width: Dimension::Px(112.0),
        height: Dimension::Auto,
        font_size: 14.0,
        line_height: 1.4,
        ..Default::default()
    });
    th2.text = Some("First appeared".to_string());

    let mut td2 = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        flex_grow: 1.0,
        width: Dimension::Auto,
        height: Dimension::Auto,
        font_size: 14.0,
        line_height: 1.4,
        ..Default::default()
    });
    td2.text = Some("July 7, 2010".to_string());
    row2.children.push(th2);
    row2.children.push(td2);

    tbody.children.push(row1);
    tbody.children.push(row2);
    table.children.push(tbody);
    container.children.push(table);

    container.compute_layout(800.0, 1000.0);

    let t = &container.children[0];
    assert_eq!(t.geometry.border_box.x, 500.0);
    assert_eq!(t.geometry.border_box.y, 0.0);

    let tb = &t.children[0];
    assert_eq!(tb.geometry.border_box.x, 500.0);
    assert_eq!(tb.geometry.border_box.y, 0.0);

    let r1 = &tb.children[0];
    let r2 = &tb.children[1];
    println!("Row 1 border_box: {:?}", r1.geometry.border_box);
    println!("Row 2 border_box: {:?}", r2.geometry.border_box);
    assert_eq!(r1.geometry.border_box.y, 0.0);
    assert!(r2.geometry.border_box.y > 15.0, "Row 2 y {} must be > 15.0", r2.geometry.border_box.y);
}

#[test]
fn test_bfc_element_overflow_hidden_does_not_overlap_float() {
    // CSS 2.1 §9.5: The border box of an element establishing a new BFC
    // must not overlap the margin box of any floats in the same BFC.
    let mut container = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        width: Dimension::Px(800.0),
        height: Dimension::Auto,
        ..Default::default()
    });

    let float_box = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        float: Float::Left,
        width: Dimension::Px(200.0),
        height: Dimension::Px(250.0),
        ..Default::default()
    });

    let bfc_box = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        overflow_x: Overflow::Hidden,
        overflow_y: Overflow::Hidden,
        width: Dimension::Auto,
        height: Dimension::Px(100.0),
        ..Default::default()
    });

    container.children.push(float_box);
    container.children.push(bfc_box);

    container.compute_layout(800.0, 1000.0);

    // Float box is at (0, 0) with width 200
    assert_eq!(container.children[0].geometry.border_box.x, 0.0);
    assert_eq!(container.children[0].geometry.border_box.y, 0.0);
    assert_eq!(container.children[0].geometry.border_box.width, 200.0);

    // BFC box must be placed beside the float at x = 200, width = 600
    assert_eq!(container.children[1].geometry.border_box.x, 200.0);
    assert_eq!(container.children[1].geometry.border_box.y, 0.0);
    assert_eq!(container.children[1].geometry.border_box.width, 600.0);
}

#[test]
fn test_bfc_element_with_large_width_clears_below_float() {
    // CSS 2.1 §9.5: If the BFC element cannot fit beside the float,
    // it must clear by placing itself below preceding floats.
    let mut container = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        width: Dimension::Px(800.0),
        height: Dimension::Auto,
        ..Default::default()
    });

    let float_box = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        float: Float::Left,
        width: Dimension::Px(200.0),
        height: Dimension::Px(250.0),
        ..Default::default()
    });

    let bfc_box = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        overflow_x: Overflow::Hidden,
        overflow_y: Overflow::Hidden,
        width: Dimension::Px(700.0),
        height: Dimension::Px(100.0),
        ..Default::default()
    });

    container.children.push(float_box);
    container.children.push(bfc_box);

    container.compute_layout(800.0, 1000.0);

    // Float is at (0, 0), height 250
    assert_eq!(container.children[0].geometry.border_box.x, 0.0);
    assert_eq!(container.children[0].geometry.border_box.y, 0.0);

    // BFC box (width 700) cannot fit beside 200px float in 800px container; clears below float!
    assert_eq!(container.children[1].geometry.border_box.x, 0.0);
    assert_eq!(container.children[1].geometry.border_box.y, 250.0);
    assert_eq!(container.children[1].geometry.border_box.width, 700.0);
}

#[test]
fn test_bfc_internal_floats_contained_and_do_not_leak() {
    // CSS 2.1 §10.6.7 & §9.4.1: A BFC root's auto height encloses its internal floats,
    // and internal floats do not leak out into the parent's formatting context.
    let mut container = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        width: Dimension::Px(800.0),
        height: Dimension::Auto,
        ..Default::default()
    });

    let mut bfc_box = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        overflow_x: Overflow::Hidden,
        overflow_y: Overflow::Hidden,
        width: Dimension::Px(400.0),
        height: Dimension::Auto,
        ..Default::default()
    });

    let internal_float = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        float: Float::Left,
        width: Dimension::Px(150.0),
        height: Dimension::Px(120.0),
        ..Default::default()
    });
    bfc_box.children.push(internal_float);

    let subsequent_block = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        width: Dimension::Px(800.0),
        height: Dimension::Px(50.0),
        ..Default::default()
    });

    container.children.push(bfc_box);
    container.children.push(subsequent_block);

    container.compute_layout(800.0, 1000.0);

    // BFC box height must enclose its 120px internal float
    assert_eq!(container.children[0].geometry.border_box.height, 120.0);

    // Subsequent block starts after the BFC box at y = 120.0, and since internal float
    // did not leak out, subsequent block is placed at x = 0 (not shifted by float)
    assert_eq!(container.children[1].geometry.border_box.x, 0.0);
    assert_eq!(container.children[1].geometry.border_box.y, 120.0);
}

