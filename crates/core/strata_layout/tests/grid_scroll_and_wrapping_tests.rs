use strata_layout::prelude::*;
use strata_style::types::{
    ComputedStyle, Dimension, Display, FlexDirection, GridTrack, Overflow,
};

#[test]
fn test_grid_definite_height_overflow_auto_sizes_rows_by_content_and_scrolls() {
    // Reproduces GraphsPanel: Display::Flex column with a flex-1 Display::Grid
    // overflow-y-auto containing more implicit rows than fit vertically.
    let root_style = ComputedStyle {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        width: Dimension::Px(400.0),
        height: Dimension::Px(300.0),
        ..ComputedStyle::default()
    };

    let grid_style = ComputedStyle {
        display: Display::Grid,
        flex_grow: 1.0,
        grid_template_columns: vec![GridTrack::Fr(1.0), GridTrack::Fr(1.0)],
        gap: 10.0,
        overflow_y: Overflow::Auto,
        ..ComputedStyle::default()
    };

    let mut items = Vec::new();
    for _ in 0..8 {
        let s = ComputedStyle {
            display: Display::Block,
            height: Dimension::Px(120.0),
            ..ComputedStyle::default()
        };
        items.push(LayoutNode::new(s));
    }

    let grid = LayoutNode::with_children(grid_style, items);
    let mut root = LayoutNode::with_children(root_style, vec![grid]);
    root.compute_layout(400.0, 300.0);

    let g = &root.children[0];
    assert!(
        (g.geometry.border_box.height - 300.0).abs() < 1.0,
        "flex-1 grid container must stay constrained to 300px, got {}",
        g.geometry.border_box.height
    );
    // 4 rows * 120px + 3 * 10px gaps = 510px > 300px => max_scroll_y ~= 210px
    for (i, child) in g.children.iter().enumerate() {
        assert!(
            (child.geometry.border_box.height - 120.0).abs() < 0.5,
            "grid item {i} was squashed to {} instead of preserving 120px content height",
            child.geometry.border_box.height
        );
    }
    assert!(
        g.geometry.max_scroll().y >= 209.0,
        "scrollable grid must report positive max_scroll().y (~210px), got {}",
        g.geometry.max_scroll().y
    );
}

#[test]
fn test_grid_auto_height_respects_max_height_and_scrolls() {
    // Reproduces StudioHubModal: Auto-height Display::Grid with max-h-[240px] overflow-y-auto inside a Flex parent
    let modal_style = ComputedStyle {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        width: Dimension::Px(420.0),
        ..ComputedStyle::default()
    };

    let grid_style = ComputedStyle {
        display: Display::Grid,
        grid_template_columns: vec![GridTrack::Fr(1.0), GridTrack::Fr(1.0)],
        gap: 8.0,
        max_height: Dimension::Px(240.0),
        overflow_y: Overflow::Auto,
        ..ComputedStyle::default()
    };

    let mut cards = Vec::new();
    for _ in 0..6 {
        let s = ComputedStyle {
            display: Display::Block,
            height: Dimension::Px(130.0),
            ..ComputedStyle::default()
        };
        cards.push(LayoutNode::new(s));
    }

    let grid = LayoutNode::with_children(grid_style, cards);
    let mut modal = LayoutNode::with_children(modal_style, vec![grid]);
    modal.compute_layout(420.0, 800.0);

    let g = &modal.children[0];
    assert!(
        (g.geometry.border_box.height - 240.0).abs() < 1.0,
        "auto-height grid must clamp to max-height 240px, got {}",
        g.geometry.border_box.height
    );
    // 3 rows * 130px + 2 * 8px gaps = 406px -> max_scroll().y = 166px
    assert!(
        g.geometry.max_scroll().y >= 165.0,
        "clamped max-height grid must allow vertical scrolling (~166px), got {}",
        g.geometry.max_scroll().y
    );
}

#[test]
fn test_grid_children_wrap_text_within_column_width_and_translate_line_boxes() {
    let grid_style = ComputedStyle {
        display: Display::Grid,
        width: Dimension::Px(320.0),
        height: Dimension::Px(200.0),
        grid_template_columns: vec![GridTrack::Fr(1.0), GridTrack::Fr(1.0)],
        gap: 20.0,
        ..ComputedStyle::default()
    };

    let cell1_style = ComputedStyle {
        display: Display::Block,
        font_size: 14.0,
        ..ComputedStyle::default()
    };
    let mut cell1 = LayoutNode::new(cell1_style.clone());
    cell1.inline_items = vec![strata_layout::typography::InlineItem::text(
        "Long descriptive label inside first column that must wrap instead of bleeding across",
        strata_layout::typography::InlineTextStyle::default(),
    )];

    let mut cell2 = LayoutNode::new(cell1_style.clone());
    cell2.inline_items = vec![strata_layout::typography::InlineItem::text(
        "Second column text label that must start cleanly at column 2 x offset",
        strata_layout::typography::InlineTextStyle::default(),
    )];

    let cell3 = LayoutNode::with_text(
        cell1_style,
        "Third cell leaf text that wraps vertically to fit inside its grid track".to_string(),
    );

    let mut grid = LayoutNode::with_children(grid_style, vec![cell1, cell2, cell3]);
    grid.compute_layout(320.0, 200.0);

    // Each column track is (320 - 20) / 2 = 150px wide; col 2 starts at x = 170px.
    let c0 = &grid.children[0];
    let c1 = &grid.children[1];
    let c2 = &grid.children[2];
    assert!(c0.geometry.border_box.width <= 150.5);
    assert!((c1.geometry.border_box.x - 170.0).abs() < 1.0);
    assert!(
        c0.line_boxes.len() > 1,
        "cell 0 inline text must wrap into multiple line boxes within 150px column width, got {} lines",
        c0.line_boxes.len()
    );
    for lb in &c0.line_boxes {
        assert!(
            lb.bounds.x >= 0.0 && lb.bounds.x + lb.bounds.width <= 151.0,
            "column 0 line box {:?} bled past 150px track boundary",
            lb.bounds
        );
    }
    assert!(c1.line_boxes.len() > 1);
    for lb in &c1.line_boxes {
        assert!(
            lb.bounds.x >= 169.5 && lb.bounds.x + lb.bounds.width <= 321.0,
            "column 1 line box {:?} was not translated or overflowed column 1 bounds [170, 320]",
            lb.bounds
        );
    }
    assert!(
        c2.geometry.border_box.width <= 150.5 && c2.geometry.border_box.height > 20.0,
        "cell 2 leaf text must constrain width <= 150 and wrap vertically, got {:?}",
        c2.geometry.border_box
    );
}
