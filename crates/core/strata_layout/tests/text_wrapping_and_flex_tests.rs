//! Regression tests for constrained text wrapping, box_model padding insets,
//! and W3C CSS Flexbox §9.7 iterative shrink/freeze redistribution.

use strata_layout::box_model::solve_block;
use strata_layout::flexbox::solve_flexbox;
use strata_layout::tree::LayoutNode;
use strata_style::types::{
    ComputedStyle, Dimension, Display, EdgeInsets, FlexDirection, Overflow, WhiteSpace,
};

#[test]
fn test_padded_and_max_width_text_block_wraps_inside_available_width() {
    let mut style = ComputedStyle::default();
    style.display = Display::Block;
    style.width = Dimension::Auto;
    style.height = Dimension::Auto;
    style.max_width = Dimension::Px(220.0);
    style.padding = EdgeInsets {
        left: 16.0,
        right: 16.0,
        top: 8.0,
        bottom: 8.0,
    };
    style.border_width = EdgeInsets::all(2.0);
    style.font_size = 14.0;
    style.line_height = 1.4;
    style.white_space = WhiteSpace::Normal;

    let mut node = LayoutNode::with_text(
        style,
        "Component descriptions with padding and borders must wrap cleanly without overflowing past the right border or scrollbar".to_string(),
    );
    solve_block(&mut node, 280.0, 400.0);

    assert!(
        node.geometry.border_box.width <= 220.0 + 0.5,
        "border_box width {} exceeded max_width 220 + padding/border!",
        node.geometry.border_box.width
    );
    assert!(
        node.geometry.content_box.width <= 220.0 - 36.0 + 0.5,
        "content_box width {} should reflect inset padding+border",
        node.geometry.content_box.width
    );
    assert!(
        node.geometry.border_box.height > 40.0,
        "wrapped multi-line description must grow vertically (got h={})",
        node.geometry.border_box.height
    );
}

#[test]
fn test_flex_row_redistributes_shrink_deficit_across_frozen_siblings_and_w_full_buttons() {
    // 1. Yellow banner / popover item: [fixed icon (24px), long description, fixed button (72px)] in 240px row
    let mut row_style = ComputedStyle::default();
    row_style.display = Display::Flex;
    row_style.flex_direction = FlexDirection::Row;
    row_style.width = Dimension::Px(240.0);
    row_style.height = Dimension::Auto;
    row_style.gap_col = 8.0;

    let mut icon_style = ComputedStyle::default();
    icon_style.display = Display::Block;
    icon_style.width = Dimension::Px(24.0);
    icon_style.height = Dimension::Px(24.0);
    icon_style.min_width = Dimension::Px(24.0);

    let mut desc_style = ComputedStyle::default();
    desc_style.display = Display::Block;
    desc_style.flex_shrink = 1.0;
    desc_style.font_size = 13.0;
    desc_style.line_height = 1.3;
    let desc = LayoutNode::with_text(
        desc_style,
        "Team collaboration sync warning banner must wrap inside available row space".to_string(),
    );

    let mut btn_style = ComputedStyle::default();
    btn_style.display = Display::Block;
    btn_style.width = Dimension::Px(72.0);
    btn_style.height = Dimension::Px(28.0);
    btn_style.min_width = Dimension::Px(72.0);

    let mut row = LayoutNode::with_children(
        row_style.clone(),
        vec![
            LayoutNode::new(icon_style),
            desc,
            LayoutNode::new(btn_style),
        ],
    );
    solve_flexbox(&mut row, 240.0, 400.0);

    let r_icon = row.children[0].geometry.border_box;
    let r_desc = row.children[1].geometry.border_box;
    let r_btn = row.children[2].geometry.border_box;
    assert_eq!(r_icon.width, 24.0);
    assert_eq!(r_btn.width, 72.0);
    assert!(
        r_btn.x + r_btn.width <= 240.0 + 0.5,
        "trailing button pushed outside 240px row: {:?}",
        r_btn
    );
    assert!(
        r_desc.width <= 128.0 + 0.5,
        "description should absorb entire deficit down to 128px, got {:?}",
        r_desc
    );

    // 2. w-full Button next to a sibling button inside a 220px card must NOT push sibling outside!
    let mut card_row_style = row_style;
    card_row_style.width = Dimension::Px(220.0);

    let mut full_btn_style = ComputedStyle::default();
    full_btn_style.display = Display::InlineFlex;
    full_btn_style.width = Dimension::Percent(100.0);
    full_btn_style.height = Dimension::Px(36.0);
    full_btn_style.padding = EdgeInsets::all(8.0);
    let mut full_btn_label = ComputedStyle::default();
    full_btn_label.display = Display::Inline;
    full_btn_label.font_size = 13.0;
    let full_btn = LayoutNode::with_children(
        full_btn_style,
        vec![LayoutNode::with_text(full_btn_label, "Connect".to_string())],
    );

    let mut sib_btn_style = ComputedStyle::default();
    sib_btn_style.display = Display::InlineFlex;
    sib_btn_style.width = Dimension::Px(80.0);
    sib_btn_style.height = Dimension::Px(36.0);
    sib_btn_style.overflow_x = Overflow::Visible;
    let mut sib_label = ComputedStyle::default();
    sib_label.display = Display::Inline;
    sib_label.font_size = 13.0;
    let sib_btn = LayoutNode::with_children(
        sib_btn_style,
        vec![LayoutNode::with_text(sib_label, "SetupConfig".to_string())],
    );

    let mut card_row = LayoutNode::with_children(card_row_style, vec![full_btn, sib_btn]);
    solve_flexbox(&mut card_row, 220.0, 200.0);

    let fb = card_row.children[0].geometry.border_box;
    let sb = card_row.children[1].geometry.border_box;
    assert!(
        sb.x + sb.width <= 220.0 + 0.5,
        "sibling button next to w-full button overflowed 220px card: fb={:?}, sb={:?}",
        fb,
        sb
    );
    assert!(
        fb.width <= 132.0 + 0.5 && (sb.width - 80.0).abs() < 0.5,
        "w-full button should absorb deficit and leave 80px sibling intact: fb={:?}, sb={:?}",
        fb,
        sb
    );
}
