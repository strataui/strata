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
    assert_eq!(
        sticky_geom.y, 350.0,
        "Sticky child must be clamped to static parent container bottom"
    );
}

#[test]
fn test_absolute_auto_width_and_height_with_right_inset_in_block_and_flex_parents() {
    use strata_style::types::FlexDirection;

    for is_flex in [false, true] {
        let mut parent_style = ComputedStyle {
            display: if is_flex {
                Display::Flex
            } else {
                Display::Block
            },
            flex_direction: FlexDirection::Row,
            position: Position::Relative,
            width: Dimension::Px(360.0),
            height: Dimension::Px(80.0),
            ..Default::default()
        };
        if is_flex {
            parent_style.justify_content = strata_style::types::JustifyContent::SpaceBetween;
        }

        // Absolute dropdown wrapper: width: auto, height: auto, right: 8px, top: 36px
        let mut dropdown_wrap_style = ComputedStyle {
            display: Display::Block,
            position: Position::Absolute,
            width: Dimension::Auto,
            height: Dimension::Auto,
            ..Default::default()
        };
        dropdown_wrap_style.insets.right = Dimension::Px(8.0);
        dropdown_wrap_style.insets.top = Dimension::Px(36.0);

        // Fixed-width w-48 (192px) menu panel inside the auto-sized absolute wrapper
        let menu_panel = LayoutNode::new(ComputedStyle {
            display: Display::Block,
            width: Dimension::Px(192.0),
            height: Dimension::Px(112.0),
            ..Default::default()
        });

        let dropdown_wrap = LayoutNode::with_children(dropdown_wrap_style, vec![menu_panel]);
        let mut parent = LayoutNode::with_children(parent_style, vec![dropdown_wrap]);
        parent.compute_layout(360.0, 80.0);

        let wrap_box = parent.children[0].geometry.border_box;
        let panel_box = parent.children[0].children[0].geometry.border_box;

        // Expected x = 360 - 8 - 192 = 160.0 (NOT 360 - 8 - 0 = 352.0)
        assert_eq!(
            wrap_box.width, 192.0,
            "is_flex={is_flex}: auto width must shrink-to-fit w-48 child"
        );
        assert_eq!(
            wrap_box.height, 112.0,
            "is_flex={is_flex}: auto height must wrap laid-out child"
        );
        assert_eq!(
            wrap_box.x, 160.0,
            "is_flex={is_flex}: right inset must subtract intrinsic width"
        );
        assert_eq!(
            wrap_box.y, 36.0,
            "is_flex={is_flex}: top inset must anchor to 36px"
        );
        assert_eq!(
            panel_box.x, 160.0,
            "is_flex={is_flex}: child subtree must translate to item_x"
        );
        assert_eq!(
            panel_box.y, 36.0,
            "is_flex={is_flex}: child subtree must translate to item_y"
        );
        assert!(
            wrap_box.contains_point(220.0, 90.0),
            "border_box hit-test must cover dropdown"
        );
    }
}

#[test]
fn test_absolute_auto_height_with_bottom_and_right_insets() {
    let mut root_style = ComputedStyle {
        display: Display::Flex,
        flex_direction: strata_style::types::FlexDirection::Column,
        position: Position::Relative,
        width: Dimension::Px(400.0),
        height: Dimension::Px(300.0),
        ..Default::default()
    };
    root_style.align_items = strata_style::types::AlignItems::Center;

    let mut popover_style = ComputedStyle {
        display: Display::Flex,
        flex_direction: strata_style::types::FlexDirection::Column,
        position: Position::Absolute,
        width: Dimension::Auto,
        height: Dimension::Auto,
        ..Default::default()
    };
    popover_style.insets.right = Dimension::Px(12.0);
    popover_style.insets.bottom = Dimension::Px(16.0);

    let item_a = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        width: Dimension::Px(180.0),
        height: Dimension::Px(36.0),
        ..Default::default()
    });
    let item_b = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        width: Dimension::Px(180.0),
        height: Dimension::Px(44.0),
        ..Default::default()
    });

    let popover = LayoutNode::with_children(popover_style, vec![item_a, item_b]);
    let mut root = LayoutNode::with_children(root_style, vec![popover]);
    root.compute_layout(400.0, 300.0);

    let pop_box = root.children[0].geometry.border_box;
    // Total width = 180, total height = 36 + 44 = 80
    // x = 400 - 12 - 180 = 208
    // y = 300 - 16 - 80 = 204
    assert_eq!(pop_box.width, 180.0);
    assert_eq!(pop_box.height, 80.0);
    assert_eq!(pop_box.x, 208.0);
    assert_eq!(pop_box.y, 204.0);
    assert_eq!(root.children[0].children[0].geometry.border_box.y, 204.0);
    assert_eq!(root.children[0].children[1].geometry.border_box.y, 240.0);
}

#[test]
fn test_nested_relative_insets_do_not_compound_across_positioned_ancestors() {
    // 3 nested Position::Relative flex containers wrapping a Position::Relative inner slice
    // with `top: -50px` — previously compounded -50px 4x to -200px!
    let row0 = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        width: Dimension::Px(400.0),
        height: Dimension::Px(22.0),
        ..Default::default()
    });
    let row1 = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        width: Dimension::Px(400.0),
        height: Dimension::Px(22.0),
        ..Default::default()
    });

    let mut slice_style = ComputedStyle {
        display: Display::Flex,
        flex_direction: strata_style::types::FlexDirection::Column,
        position: Position::Relative,
        width: Dimension::Px(400.0),
        height: Dimension::Auto,
        ..Default::default()
    };
    slice_style.insets.top = Dimension::Px(-50.0);
    slice_style.insets.left = Dimension::Px(12.0);
    let slice = LayoutNode::with_children(slice_style, vec![row0, row1]);

    let viewport = LayoutNode::with_children(
        ComputedStyle {
            display: Display::Block,
            position: Position::Relative,
            width: Dimension::Px(400.0),
            height: Dimension::Px(240.0),
            ..Default::default()
        },
        vec![slice],
    );
    let body = LayoutNode::with_children(
        ComputedStyle {
            display: Display::Flex,
            flex_direction: strata_style::types::FlexDirection::Row,
            position: Position::Relative,
            width: Dimension::Px(400.0),
            height: Dimension::Px(240.0),
            ..Default::default()
        },
        vec![viewport],
    );
    let mut root = LayoutNode::with_children(
        ComputedStyle {
            display: Display::Flex,
            flex_direction: strata_style::types::FlexDirection::Column,
            position: Position::Relative,
            width: Dimension::Px(400.0),
            height: Dimension::Px(240.0),
            ..Default::default()
        },
        vec![body],
    );

    root.compute_layout(400.0, 240.0);
    let slice_box = root.children[0].children[0].children[0].geometry.border_box;
    let row0_box = root.children[0].children[0].children[0].children[0]
        .geometry
        .border_box;
    let row1_box = root.children[0].children[0].children[0].children[1]
        .geometry
        .border_box;

    assert_eq!(slice_box.x, 12.0, "relative left must apply exactly once");
    assert_eq!(slice_box.y, -50.0, "relative top must apply exactly once");
    assert_eq!(row0_box.y, -50.0);
    assert_eq!(row1_box.y, -28.0);

    // Calling compute_layout a second time (cache hit path) must remain identically idempotent!
    root.compute_layout(400.0, 240.0);
    let slice_box_2 = root.children[0].children[0].children[0].geometry.border_box;
    assert_eq!(slice_box_2.x, 12.0);
    assert_eq!(slice_box_2.y, -50.0);
}

#[test]
fn test_absolute_overflow_y_auto_flex_column_with_vh_max_height_in_short_relative_parent() {
    use strata_style::types::{FlexDirection, Overflow};

    let make_row = |h: f32| {
        LayoutNode::new(ComputedStyle {
            display: Display::Block,
            width: Dimension::Percent(100.0),
            height: Dimension::Px(h),
            flex_shrink: 0.0,
            ..Default::default()
        })
    };

    let build_tree = |row_count: usize| {
        let rows: Vec<LayoutNode> = (0..row_count).map(|_| make_row(60.0)).collect();
        let mut dd_style = ComputedStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            position: Position::Absolute,
            width: Dimension::Px(280.0),
            height: Dimension::Auto,
            max_height: Dimension::Vh(76.0),
            overflow_y: Overflow::Auto,
            ..Default::default()
        };
        dd_style.insets.top = Dimension::Percent(100.0);
        dd_style.insets.right = Dimension::Px(0.0);
        let dropdown = LayoutNode::with_children(dd_style, rows);
        // Short relative anchor container (40px tall) inside a 1280x900 viewport.
        let anchor = LayoutNode::with_children(
            ComputedStyle {
                display: Display::Flex,
                position: Position::Relative,
                width: Dimension::Px(280.0),
                height: Dimension::Px(40.0),
                ..Default::default()
            },
            vec![dropdown],
        );
        LayoutNode::with_children(
            ComputedStyle {
                display: Display::Block,
                width: Dimension::Percent(100.0),
                height: Dimension::Percent(100.0),
                ..Default::default()
            },
            vec![anchor],
        )
    };

    // 1. Intrinsic content shorter than 76vh (3 * 60 = 180px < 684px): must preserve 180px.
    let mut short_root = build_tree(3);
    short_root.compute_layout(1280.0, 900.0);
    let short_dd = &short_root.children[0].children[0];
    assert_eq!(
        short_dd.geometry.border_box.height, 180.0,
        "absolute flex-col overflow-y-auto max-h-[76vh] must preserve intrinsic content height"
    );
    assert_eq!(
        short_dd.geometry.border_box.y, 40.0,
        "top-full must still offset by 40px anchor"
    );
    assert_eq!(short_dd.geometry.max_scroll().y, 0.0);

    // 2. Intrinsic content taller than 76vh (15 * 60 = 900px > 684px): clamps to 684px and scrolls.
    let mut tall_root = build_tree(15);
    tall_root.compute_layout(1280.0, 900.0);
    let tall_dd = &tall_root.children[0].children[0];
    assert!(
        (tall_dd.geometry.border_box.height - 684.0).abs() <= 1.0,
        "expected clamp to 76vh (684px), got {}",
        tall_dd.geometry.border_box.height
    );
    assert!(
        (tall_dd.geometry.scroll_bounds.height - 900.0).abs() <= 1.0,
        "scroll_height should retain full 900px content, got {}",
        tall_dd.geometry.scroll_bounds.height
    );
    assert!(
        (tall_dd.geometry.max_scroll().y - 216.0).abs() <= 1.0,
        "max_scroll_y should be 216px, got {}",
        tall_dd.geometry.max_scroll().y
    );

    // 3. Resizing viewport height to 500px must invalidate cached vh clamp -> 76% of 500 = 380px.
    tall_root.compute_layout(1280.0, 500.0);
    let resized_dd = &tall_root.children[0].children[0];
    assert!(
        (resized_dd.geometry.border_box.height - 380.0).abs() <= 1.0,
        "expected resized 76vh clamp (380px), got {}",
        resized_dd.geometry.border_box.height
    );
}

#[test]
fn test_percentage_max_height_on_overflow_y_auto_preserves_intrinsic_and_clamps_definite_cb() {
    use strata_style::types::{FlexDirection, Overflow};

    let make_items = |count: usize| -> Vec<LayoutNode> {
        (0..count)
            .map(|_| {
                LayoutNode::new(ComputedStyle {
                    display: Display::Block,
                    width: Dimension::Percent(100.0),
                    height: Dimension::Px(50.0),
                    flex_shrink: 0.0,
                    ..Default::default()
                })
            })
            .collect()
    };

    // Case A: Definite 300px relative CB with absolute max-h-[80%] (240px clamp).
    let dd_short = LayoutNode::with_children(
        ComputedStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            position: Position::Absolute,
            width: Dimension::Px(220.0),
            height: Dimension::Auto,
            max_height: Dimension::Percent(80.0),
            overflow_y: Overflow::Auto,
            ..Default::default()
        },
        make_items(2), // 100px < 240px
    );
    let dd_tall = LayoutNode::with_children(
        ComputedStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            position: Position::Absolute,
            width: Dimension::Px(220.0),
            height: Dimension::Auto,
            max_height: Dimension::Percent(80.0),
            overflow_y: Overflow::Auto,
            ..Default::default()
        },
        make_items(10), // 500px > 240px
    );
    let mut definite_parent = LayoutNode::with_children(
        ComputedStyle {
            display: Display::Block,
            position: Position::Relative,
            width: Dimension::Px(400.0),
            height: Dimension::Px(300.0),
            ..Default::default()
        },
        vec![dd_short, dd_tall],
    );
    definite_parent.compute_layout(800.0, 600.0);
    assert_eq!(
        definite_parent.children[0].geometry.border_box.height, 100.0,
        "short content inside definite CB must keep 100px intrinsic height"
    );
    assert_eq!(
        definite_parent.children[1].geometry.border_box.height, 240.0,
        "tall content inside 300px CB with max-h-[80%] must clamp to 240px, not collapse to 0"
    );
    assert_eq!(definite_parent.children[1].geometry.scroll_bounds.height, 500.0);
    assert_eq!(definite_parent.children[1].geometry.max_scroll().y, 260.0);

    // Case B: Indefinite / zero-height relative wrapper (only absolute child) with max-h-[80%]:
    // per CSS 2.1 §10.7, percentage max-height against indefinite CB behaves as none rather than 0.
    let dd_indef = LayoutNode::with_children(
        ComputedStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            position: Position::Absolute,
            width: Dimension::Px(220.0),
            height: Dimension::Auto,
            max_height: Dimension::Percent(80.0),
            overflow_y: Overflow::Auto,
            ..Default::default()
        },
        make_items(4), // 200px
    );
    let mut indef_parent = LayoutNode::with_children(
        ComputedStyle {
            display: Display::Block,
            position: Position::Relative,
            width: Dimension::Px(220.0),
            height: Dimension::Auto,
            ..Default::default()
        },
        vec![dd_indef],
    );
    indef_parent.compute_layout(800.0, 600.0);
    assert_eq!(
        indef_parent.children[0].geometry.border_box.height, 200.0,
        "indefinite percentage max-height must preserve intrinsic 200px height instead of collapsing to 0"
    );
}

#[test]
fn test_absolute_flex_column_wrapper_with_vh_max_height_and_nested_scroll_body() {
    use strata_style::types::{FlexDirection, Overflow};

    let rows: Vec<LayoutNode> = (0..20)
        .map(|_| {
            LayoutNode::new(ComputedStyle {
                display: Display::Block,
                width: Dimension::Percent(100.0),
                height: Dimension::Px(40.0),
                flex_shrink: 0.0,
                ..Default::default()
            })
        })
        .collect();

    // Pattern: absolute outer panel `flex flex-col max-h-[50vh] overflow-hidden` containing
    // a fixed header + `flex-1 min-h-0 overflow-y-auto` scrollable list.
    let header = LayoutNode::new(ComputedStyle {
        display: Display::Block,
        width: Dimension::Percent(100.0),
        height: Dimension::Px(32.0),
        flex_shrink: 0.0,
        ..Default::default()
    });
    let scroll_body = LayoutNode::with_children(
        ComputedStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            width: Dimension::Percent(100.0),
            flex_grow: 1.0,
            flex_shrink: 1.0,
            min_height: Dimension::Px(0.0),
            overflow_y: Overflow::Auto,
            ..Default::default()
        },
        rows,
    );
    let mut panel_style = ComputedStyle {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        position: Position::Absolute,
        width: Dimension::Px(260.0),
        height: Dimension::Auto,
        max_height: Dimension::Vh(50.0),
        overflow_y: Overflow::Hidden,
        ..Default::default()
    };
    panel_style.insets.top = Dimension::Percent(100.0);
    let panel = LayoutNode::with_children(panel_style, vec![header, scroll_body]);
    let anchor = LayoutNode::with_children(
        ComputedStyle {
            display: Display::Block,
            position: Position::Relative,
            width: Dimension::Px(260.0),
            height: Dimension::Px(36.0),
            ..Default::default()
        },
        vec![panel],
    );
    let mut root = LayoutNode::with_children(
        ComputedStyle {
            display: Display::Block,
            width: Dimension::Percent(100.0),
            height: Dimension::Percent(100.0),
            ..Default::default()
        },
        vec![anchor],
    );

    root.compute_layout(1200.0, 800.0); // 50vh = 400px
    let panel_node = &root.children[0].children[0];
    let header_node = &panel_node.children[0];
    let body_node = &panel_node.children[1];

    assert_eq!(panel_node.geometry.border_box.height, 400.0);
    assert_eq!(header_node.geometry.border_box.height, 32.0);
    assert_eq!(body_node.geometry.border_box.height, 368.0);
    assert_eq!(body_node.geometry.scroll_bounds.height, 800.0);
    assert_eq!(body_node.geometry.max_scroll().y, 432.0);
}

#[test]
fn test_isolated_fixed_overlay_relayout_preserves_root_and_sibling_caches() {
    let make_hud_tree = |label_text: &str| {
        let mut sibling_panel = LayoutNode::new(ComputedStyle {
            display: Display::Block,
            width: Dimension::Px(320.0),
            height: Dimension::Px(600.0),
            ..Default::default()
        });
        sibling_panel.children.push(LayoutNode::new(ComputedStyle {
            display: Display::Block,
            width: Dimension::Px(280.0),
            height: Dimension::Px(40.0),
            ..Default::default()
        }));

        let mut hud_style = ComputedStyle {
            display: Display::Flex,
            position: Position::Fixed,
            width: Dimension::Auto,
            height: Dimension::Auto,
            ..Default::default()
        };
        hud_style.insets.right = Dimension::Px(16.0);
        hud_style.insets.bottom = Dimension::Px(12.0);
        hud_style.padding = strata_style::types::EdgeInsets::all(8.0);

        let mut text_node = LayoutNode::new(ComputedStyle {
            display: Display::Inline,
            font_size: 13.0,
            line_height: 1.4,
            ..Default::default()
        });
        text_node.text = Some(label_text.to_string());

        let hud = LayoutNode::with_children(hud_style, vec![text_node]);
        LayoutNode::with_children(
            ComputedStyle {
                display: Display::Block,
                width: Dimension::Px(1280.0),
                height: Dimension::Px(720.0),
                ..Default::default()
            },
            vec![sibling_panel, hud],
        )
    };

    let mut root = make_hud_tree("ALT 100m");
    root.compute_layout(1280.0, 720.0);

    assert!(root.layout_cache.is_valid);
    assert!(root.children[0].layout_cache.is_valid);
    assert!(root.children[1].is_isolated_relayout_root());
    assert_eq!(
        root.find_isolated_relayout_boundary_path(&[1, 0]),
        Some(&[1][..])
    );

    let initial_hud_box = root.children[1].geometry.border_box;
    let initial_sibling_box = root.children[0].geometry.border_box;

    // Mutate only the text inside the position: fixed HUD overlay and relayout in isolation.
    let updated_label = "ALT 12450.75m | HDG 274.2 DEG";
    root.children[1].children[0].text = Some(updated_label.to_string());
    assert!(root.invalidate_ancestors_within(&[1], &[1, 0]));
    assert!(
        root.layout_cache.is_valid,
        "Root layout cache must stay valid when invalidating inside an isolated fixed overlay"
    );
    assert!(
        root.children[0].layout_cache.is_valid,
        "Sibling layout cache must stay valid when invalidating inside an isolated fixed overlay"
    );

    assert!(root.recompute_isolated_boundary_at_path(&[1], 1280.0, 720.0));
    assert!(root.layout_cache.is_valid);
    assert!(root.children[0].layout_cache.is_valid);
    assert_eq!(root.children[0].geometry.border_box, initial_sibling_box);

    let isolated_hud_box = root.children[1].geometry.border_box;
    assert!(
        isolated_hud_box.width > initial_hud_box.width,
        "Longer HUD text must widen the shrink-to-fit fixed overlay: {:?} vs {:?}",
        isolated_hud_box,
        initial_hud_box
    );

    // Compare against a fresh full root layout pass for exact geometric parity.
    let mut reference = make_hud_tree(updated_label);
    reference.compute_layout(1280.0, 720.0);
    let ref_hud_box = reference.children[1].geometry.border_box;
    assert!(
        (isolated_hud_box.x - ref_hud_box.x).abs() < 0.01
            && (isolated_hud_box.y - ref_hud_box.y).abs() < 0.01
            && (isolated_hud_box.width - ref_hud_box.width).abs() < 0.01
            && (isolated_hud_box.height - ref_hud_box.height).abs() < 0.01,
        "Isolated fixed overlay relayout {:?} must match full compute_layout {:?}",
        isolated_hud_box,
        ref_hud_box
    );
}
