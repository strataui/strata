//! End-to-end regression tests for consumer-reported layout defects.
//!
//! Each test uses the exact reported markup and runs it through the production
//! RSX blueprint -> `AppRuntime::build_layout_tree` -> `compute_layout` path at
//! a 1440x900 viewport. No `LayoutNode` tree is hand assembled here.
//!
//! Note on the HTML transpiler path: `WebTranspiler::transpile_full_document`
//! emits a fully cascaded `ComputedStyle` into `Attribute::DirectStyle`, which
//! replaces the class-derived style in `build_layout_tree`. Importing
//! `strata_web_reader` from `strata_layout` would create a package cycle, and
//! class utilities therefore cannot be exercised through that path without
//! changing the browser's cascade. These tests use the same runtime and layout
//! pipeline with class-based markup, which is the path native Strata apps use.

use strata_layout::LayoutNode;
use strata_style::types::{Dimension, Position};

fn find_text<'a>(node: &'a LayoutNode, needle: &str) -> Option<&'a LayoutNode> {
    if let Some(t) = &node.text {
        if t.contains(needle) {
            return Some(node);
        }
    }
    for child in &node.children {
        if let Some(found) = find_text(child, needle) {
            return Some(found);
        }
    }
    None
}

fn find_positioned(node: &LayoutNode, position: Position) -> Option<&LayoutNode> {
    if node.style.position == position {
        return Some(node);
    }
    for child in &node.children {
        if let Some(found) = find_positioned(child, position) {
            return Some(found);
        }
    }
    None
}

fn find_width(node: &LayoutNode, width: f32) -> Option<&LayoutNode> {
    if (node.geometry.border_box.width - width).abs() < 0.5 {
        return Some(node);
    }
    for child in &node.children {
        if let Some(found) = find_width(child, width) {
            return Some(found);
        }
    }
    None
}

fn find_explicit_width(node: &LayoutNode, width: f32) -> Option<&LayoutNode> {
    if node.style.width == Dimension::Px(width) {
        return Some(node);
    }
    for child in &node.children {
        if let Some(found) = find_explicit_width(child, width) {
            return Some(found);
        }
    }
    None
}

macro_rules! rsx_root {
    ({ $($markup:tt)* }) => {{
        use strata::prelude::*;
        let view = rsx! { $($markup)* };
        #[allow(unused_mut)]
        let mut root = AppRuntime::build_layout_tree(&view.root);
        root.compute_layout(1440.0, 900.0);
        root
    }};
    ($($markup:tt)*) => {{
        use strata::prelude::*;
        let view = rsx! { $($markup)* };
        #[allow(unused_mut)]
        let mut root = AppRuntime::build_layout_tree(&view.root);
        root.compute_layout(1440.0, 900.0);
        root
    }};
}

#[test]
fn s1_flex_shrink_zero_matches_default_cross_stretch() {
    let shrinking = rsx_root!(
        {
            <div style="width: 320px;" class="flex flex-col">
                <div class="p-3 flex flex-col flex-shrink-0">
                    <div class="flex items-center">
                        <div class="text-xs">"Diagnostics"</div>
                        <div class="flex-1" />
                        <div class="text-xs">"engine 5.4"</div>
                    </div>
                </div>
            </div>
        }
    );
    let stretching = rsx_root!(
        {
            <div style="width: 320px;" class="flex flex-col">
                <div class="p-3 flex flex-col">
                    <div class="flex items-center">
                        <div class="text-xs">"Diagnostics"</div>
                        <div class="flex-1" />
                        <div class="text-xs">"engine 5.4"</div>
                    </div>
                </div>
            </div>
        }
    );

    let left_a = find_text(&shrinking, "Diagnostics").expect("left label");
    let right_a = find_text(&shrinking, "engine 5.4").expect("right label");
    let left_b = find_text(&stretching, "Diagnostics").expect("left label");
    let right_b = find_text(&stretching, "engine 5.4").expect("right label");

    // The inner row is the parent of the two labels. Both variants must give
    // it the full 320px minus the panel's 12px padding on each side.
    let row_a = find_common_parent(&shrinking, "Diagnostics", "engine 5.4").expect("inner row");
    assert!(
        (row_a.geometry.border_box.width - 296.0).abs() < 0.5,
        "inner row width {} != 296",
        row_a.geometry.border_box.width
    );

    // The right label must sit at the row's right content edge, proving the
    // spacer received the free space and the descendants saw the stretch.
    let row_right = row_a.geometry.content_box.x + row_a.geometry.content_box.width;
    let right_edge = right_a.geometry.border_box.x + right_a.geometry.border_box.width;
    assert!(
        (right_edge - row_right).abs() < 1.0,
        "right label edge {right_edge} != row right edge {row_right}"
    );
    assert!(
        left_a.geometry.border_box.width > 0.0 && right_a.geometry.border_box.height > 0.0,
        "labels were not measured"
    );

    // shrink-0 is a flex-item property of the parent, not a content sizing
    // hint. Both trees must be geometrically identical.
    assert!((left_a.geometry.border_box.x - left_b.geometry.border_box.x).abs() < 0.01);
    assert!((right_a.geometry.border_box.x - right_b.geometry.border_box.x).abs() < 0.01);
    assert!((right_a.geometry.border_box.y - right_b.geometry.border_box.y).abs() < 0.01);
    assert!((right_a.geometry.border_box.width - right_b.geometry.border_box.width).abs() < 0.01);
}

fn find_common_parent<'a>(
    node: &'a LayoutNode,
    first: &str,
    second: &str,
) -> Option<&'a LayoutNode> {
    let has_first = find_text(node, first).is_some();
    let has_second = find_text(node, second).is_some();
    if has_first && has_second {
        for child in &node.children {
            if let Some(found) = find_common_parent(child, first, second) {
                return Some(found);
            }
        }
        return Some(node);
    }
    None
}

#[test]
fn s2_span_text_is_measured_in_flex_rows_and_blocks() {
    let root = rsx_root!(
        {
            <div>
                <div class="flex items-center w-[320px]">
                    <div class="text-xs">"Kernel"</div>
                    <div class="flex-1" />
                    <span class="text-xs">"kernel rows value"</span>
                </div>
                <div><span class="text-xs">"plain span text"</span></div>
                <div>
                    <span class="text-xs">"outer "
                        <span>"inner text"</span>
                        " tail"
                    </span>
                </div>
            </div>
        }
    );

    for needle in [
        "kernel rows value",
        "plain span text",
        "outer ",
        "inner text",
        " tail",
    ] {
        let text = find_text(&root, needle).unwrap_or_else(|| panic!("missing {needle:?}"));
        assert!(
            text.geometry.border_box.width > 0.0 && text.geometry.border_box.height > 0.0,
            "text {needle:?} has empty bounds {:?}",
            text.geometry.border_box
        );
    }

    // The span that is a flex item must also have non-zero bounds.
    let flex_span = find_parent_of_text(&root, "kernel rows value").expect("flex span");
    assert!(flex_span.geometry.border_box.width > 0.0);
    assert!(flex_span.geometry.border_box.height > 0.0);
}

fn find_parent_of_text<'a>(node: &'a LayoutNode, needle: &str) -> Option<&'a LayoutNode> {
    for child in &node.children {
        if child.text.as_deref().is_some_and(|t| t.contains(needle)) {
            return Some(node);
        }
        if let Some(found) = find_parent_of_text(child, needle) {
            return Some(found);
        }
    }
    None
}

#[test]
fn s3_absolute_inset_zero_centers_in_definite_height() {
    let root = rsx_root!(
        {
            <div style="width:1440px;height:900px" class="w-full h-full relative">
                <div class="absolute inset-0 flex items-center justify-center p-4">
                    <div class="w-[520px] h-[320px]" />
                </div>
            </div>
        }
    );

    let overlay = find_positioned(&root, Position::Absolute).expect("overlay");
    assert!((overlay.geometry.border_box.x - 0.0).abs() < 0.5);
    assert!((overlay.geometry.border_box.y - 0.0).abs() < 0.5);
    assert!((overlay.geometry.border_box.width - 1440.0).abs() < 0.5);
    assert!((overlay.geometry.border_box.height - 900.0).abs() < 0.5);

    let child = find_explicit_width(&root, 520.0).expect("centered child");
    assert!(
        (child.geometry.border_box.x - 460.0).abs() < 0.5,
        "centered child x {} != 460",
        child.geometry.border_box.x
    );
    assert!(
        (child.geometry.border_box.y - 290.0).abs() < 0.5,
        "centered child y {} != 290",
        child.geometry.border_box.y
    );
    assert!((child.geometry.border_box.width - 520.0).abs() < 0.5);
    assert!((child.geometry.border_box.height - 320.0).abs() < 0.5);
}

#[test]
fn s3_absolute_inset_zero_tracks_flex_grown_parent_height() {
    let root = rsx_root!(
        {
            <div class="flex flex-col w-[800px] h-[600px]">
                <div class="flex-1 relative">
                    <div class="absolute inset-0 flex items-center justify-center p-4">
                        <div class="w-[520px] h-[320px]" />
                    </div>
                </div>
            </div>
        }
    );

    let overlay = find_positioned(&root, Position::Absolute).expect("overlay");
    assert!((overlay.geometry.border_box.width - 800.0).abs() < 0.5);
    assert!(
        (overlay.geometry.border_box.height - 600.0).abs() < 0.5,
        "overlay height {} did not follow flex-grown parent",
        overlay.geometry.border_box.height
    );

    let child = find_explicit_width(&root, 520.0).expect("centered child");
    assert!(
        (child.geometry.border_box.x - 140.0).abs() < 0.5,
        "child x {} != 140",
        child.geometry.border_box.x
    );
    assert!(
        (child.geometry.border_box.y - 140.0).abs() < 0.5,
        "child y {} != 140",
        child.geometry.border_box.y
    );
}

#[test]
fn s4_truncate_renders_one_clipped_ellipsis_line() {
    let root = rsx_root!(
        {
            <div class="truncate" style="width:288px;font-size:12px;line-height:16px;">"a very long label that must not wrap onto several lines"</div>
        }
    );

    let text = find_text(&root, "a very long label").expect("text node");
    assert!(
        text.geometry.border_box.height <= 16.5,
        "truncate text height {} exceeds one line",
        text.geometry.border_box.height
    );
    assert_eq!(
        text.line_boxes.len(),
        1,
        "truncate must produce one line box"
    );

    let frag_text: String = text.line_boxes[0]
        .text_fragments
        .iter()
        .map(|f| f.text.clone())
        .collect();
    assert!(
        frag_text.ends_with('\u{2026}'),
        "painted fragment does not end with an ellipsis: {frag_text:?}"
    );
    let fragment_width: f32 = text.line_boxes[0]
        .text_fragments
        .iter()
        .map(|f| f.bounds.width)
        .sum();
    assert!(
        fragment_width <= 288.0 + 0.5,
        "truncated fragment width {fragment_width} exceeds 288"
    );
}

#[test]
fn s5_flex_one_respects_max_width_and_keeps_right_cluster_anchored() {
    let root = rsx_root!(
        {
            <div class="flex justify-between w-[900px]">
                <div class="w-[200px]">"Left"</div>
                <div class="flex-1 max-w-md">"Search"</div>
                <div class="w-[100px]">"Right"</div>
            </div>
        }
    );

    let search = find_explicit_width(&root, 448.0)
        .or_else(|| find_width(&root, 448.0))
        .expect("search field clamped to max-w-md");
    assert!(
        (search.geometry.border_box.width - 448.0).abs() < 0.5,
        "search width {} != 448",
        search.geometry.border_box.width
    );

    let right = find_explicit_width(&root, 100.0).expect("right cluster");
    assert!(
        (right.geometry.border_box.x - 800.0).abs() < 0.5,
        "right cluster x {} != 800",
        right.geometry.border_box.x
    );
    let right_edge = right.geometry.border_box.x + right.geometry.border_box.width;
    assert!(
        (right_edge - 900.0).abs() < 0.5,
        "right cluster is not anchored to the row's right edge"
    );
}
