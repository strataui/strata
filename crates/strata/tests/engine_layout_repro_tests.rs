//! End-to-end regression tests for consumer-reported HTML markup.
//!
//! Every test feeds the exact reported HTML through the production
//! `HtmlParser::parse` -> `WebTranspiler::transpile_full_document` ->
//! `AppRuntime::build_layout_tree` -> `compute_layout` pipeline at 1440x900.
//! The `rsx!` macro is intentionally not used here: the reported markup must
//! survive the HTML parser and transpiler unchanged.

use strata::prelude::*;
use strata_layout::LayoutNode;
use strata_style::types::{Dimension, Position};
use strata_web_reader::prelude::*;

const VIEWPORT_WIDTH: f32 = 1440.0;
const VIEWPORT_HEIGHT: f32 = 900.0;

fn html_root(html: &str) -> LayoutNode {
    let nodes = HtmlParser::parse(html);
    let theme = ReaderTheme::dark();
    let transpiler = WebTranspiler::new(&theme);
    let media = MediaContext::new(VIEWPORT_WIDTH, VIEWPORT_HEIGHT, true);
    let document = transpiler.transpile_full_document(&nodes, &media);
    let mut root = AppRuntime::build_layout_tree(&document);
    root.compute_layout(VIEWPORT_WIDTH, VIEWPORT_HEIGHT);
    root
}

fn find_text<'a>(node: &'a LayoutNode, needle: &str) -> Option<&'a LayoutNode> {
    if node.text.as_deref().is_some_and(|t| t.contains(needle)) {
        return Some(node);
    }
    for child in &node.children {
        if let Some(found) = find_text(child, needle) {
            return Some(found);
        }
    }
    None
}

fn find_parent_of<'a>(node: &'a LayoutNode, needle: &str) -> Option<&'a LayoutNode> {
    for child in &node.children {
        if child.text.as_deref().is_some_and(|t| t.contains(needle)) {
            return Some(node);
        }
        if let Some(found) = find_parent_of(child, needle) {
            return Some(found);
        }
    }
    None
}

fn find_parent_of_node<'a>(
    node: &'a LayoutNode,
    target: &*const LayoutNode,
) -> Option<&'a LayoutNode> {
    for child in &node.children {
        if std::ptr::eq(child, *target) {
            return Some(node);
        }
        if let Some(found) = find_parent_of_node(child, target) {
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

fn find_explicit_height(node: &LayoutNode, height: f32) -> Option<&LayoutNode> {
    if node.style.height == Dimension::Px(height) {
        return Some(node);
    }
    for child in &node.children {
        if let Some(found) = find_explicit_height(child, height) {
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

fn close(a: f32, b: f32, tolerance: f32) -> bool {
    (a - b).abs() <= tolerance
}

fn assert_close(actual: f32, expected: f32, tolerance: f32, label: &str) {
    assert!(
        close(actual, expected, tolerance),
        "{label}: {actual} not within {tolerance} of {expected}"
    );
}

fn s1_markup(shrink_zero: bool) -> String {
    let shrink_class = if shrink_zero { " flex-shrink-0" } else { "" };
    format!(
        r#"<div style="width: 320px;" class="flex flex-col">
  <div class="p-3 flex flex-col{shrink_class}">
    <div class="flex items-center">
      <div class="text-xs">Diagnostics</div>
      <div class="flex-1"></div>
      <div class="text-xs">engine 5.4</div>
    </div>
  </div>
</div>"#
    )
}

fn assert_s1(markup: &str) {
    let root = html_root(markup);

    let left = find_text(&root, "Diagnostics").expect("left label text");
    let right = find_text(&root, "engine 5.4").expect("right label text");

    let left_box = find_parent_of(&root, "Diagnostics").expect("left label box");
    let row = find_parent_of_node(&root, &(left_box as *const LayoutNode)).expect("inner row");

    assert_close(row.geometry.border_box.width, 296.0, 0.5, "inner row width");

    // The row is a flex row; its middle child is the intentional spacer.
    assert_eq!(row.children.len(), 3, "inner row must keep the spacer box");
    let spacer = &row.children[1];
    assert_close(spacer.geometry.border_box.width, 162.5, 1.0, "spacer width");

    assert_close(right.geometry.border_box.x, 244.2, 1.0, "right label x");
    assert!(
        left.geometry.border_box.width > 0.0 && left.geometry.border_box.height > 0.0,
        "left label text was not measured"
    );
    assert!(
        right.geometry.border_box.width > 0.0 && right.geometry.border_box.height > 0.0,
        "right label text was not measured"
    );
}

#[test]
fn s1_empty_flex_spacer_survives_and_keeps_geometry() {
    assert_s1(&s1_markup(true));
}

#[test]
fn s1_flex_shrink_zero_matches_default_cross_stretch() {
    assert_s1(&s1_markup(false));
}

#[test]
fn s2_span_text_is_measured_in_flex_rows_and_blocks() {
    let root = html_root(
        r#"<div class="flex items-center w-[320px]">
  <div class="text-xs">Kernel</div><div class="flex-1"></div><span class="text-xs">kernel rows value</span>
</div>
<div><span class="text-xs">plain span text</span></div>"#,
    );

    for needle in ["Kernel", "kernel rows value", "plain span text"] {
        let text = find_text(&root, needle).unwrap_or_else(|| panic!("missing {needle:?}"));
        assert!(
            text.geometry.border_box.width > 0.0 && text.geometry.border_box.height > 0.0,
            "text {needle:?} has empty bounds {:?}",
            text.geometry.border_box
        );
    }

    // The flex row must keep its spacer box between the label and span.
    let kernel_box = find_parent_of(&root, "Kernel").expect("kernel label box");
    let row = find_parent_of_node(&root, &(kernel_box as *const LayoutNode)).expect("kernel row");
    assert_eq!(row.children.len(), 3, "flex row must keep its spacer");
}

#[test]
fn s2_spans_with_multiple_text_children_and_nesting_are_measured() {
    let root =
        html_root(r#"<div><span class="text-xs">outer <span>inner text</span> tail</span></div>"#);

    for needle in ["outer ", "inner text", " tail"] {
        let text = find_text(&root, needle).unwrap_or_else(|| panic!("missing {needle:?}"));
        assert!(
            text.geometry.border_box.width > 0.0 && text.geometry.border_box.height > 0.0,
            "text {needle:?} has empty bounds {:?}",
            text.geometry.border_box
        );
    }

    let span = find_parent_of(&root, "outer ").expect("outer span");
    assert!(
        span.geometry.border_box.width > 0.0 && span.geometry.border_box.height > 0.0,
        "outer span has empty bounds {:?}",
        span.geometry.border_box
    );
}

#[test]
fn s3_empty_absolute_overlay_survives_and_centers_child() {
    let root = html_root(
        r#"<div style="width:1440px;height:900px" class="w-full h-full relative">
  <div class="absolute inset-0 flex items-center justify-center p-4">
    <div class="w-[520px] h-[320px]"></div>
  </div>
</div>"#,
    );

    let overlay = find_positioned(&root, Position::Absolute).expect("overlay");
    assert_close(overlay.geometry.border_box.x, 0.0, 0.5, "overlay x");
    assert_close(overlay.geometry.border_box.y, 0.0, 0.5, "overlay y");
    assert_close(
        overlay.geometry.border_box.width,
        1440.0,
        0.5,
        "overlay width",
    );
    assert_close(
        overlay.geometry.border_box.height,
        900.0,
        0.5,
        "overlay height",
    );

    let child = find_explicit_width(&root, 520.0).expect("centered child");
    assert_close(child.geometry.border_box.x, 460.0, 1.0, "centered child x");
    assert_close(child.geometry.border_box.y, 290.0, 1.0, "centered child y");
    assert_close(child.geometry.border_box.width, 520.0, 0.5, "child width");
    assert_close(child.geometry.border_box.height, 320.0, 0.5, "child height");
}

#[test]
fn s3_absolute_inset_overlay_tracks_flex_grown_parent_height() {
    let root = html_root(
        r#"<div style="width:1440px;height:900px" class="flex flex-col">
  <div class="flex-1 relative">
    <div class="absolute inset-0 flex items-center justify-center p-4">
      <div class="w-[520px] h-[320px]"></div>
    </div>
  </div>
</div>"#,
    );

    let overlay = find_positioned(&root, Position::Absolute).expect("overlay");
    assert_close(
        overlay.geometry.border_box.width,
        1440.0,
        0.5,
        "overlay width",
    );
    assert_close(
        overlay.geometry.border_box.height,
        900.0,
        0.5,
        "overlay height did not follow the flex-grown parent",
    );

    let child = find_explicit_width(&root, 520.0).expect("centered child");
    assert_close(child.geometry.border_box.x, 460.0, 1.0, "centered child x");
    assert_close(child.geometry.border_box.y, 290.0, 1.0, "centered child y");
}

#[test]
fn s4_truncate_renders_one_clipped_ellipsis_line() {
    let root = html_root(
        r#"<div class="truncate" style="width:288px;font-size:12px;line-height:16px;">a very long label that must not wrap onto several lines</div>"#,
    );

    let text = find_text(&root, "a very long label").expect("text node");
    assert_close(text.geometry.border_box.height, 16.0, 0.5, "line height");

    // In a block formatting context the glyph run is absorbed by the parent
    // inline formatting context, so the painted line box lives on the owner.
    let line_owner = if text.line_boxes.is_empty() {
        find_parent_of_node(&root, &(text as *const LayoutNode)).expect("truncating parent")
    } else {
        text
    };
    assert_eq!(
        line_owner.line_boxes.len(),
        1,
        "truncate must produce one line box"
    );

    let line = &line_owner.line_boxes[0];
    let frag_text: String = line.text_fragments.iter().map(|f| f.text.clone()).collect();
    assert!(
        frag_text.ends_with('\u{2026}'),
        "painted fragment does not end with an ellipsis: {frag_text:?}"
    );
    let fragment_width: f32 = line.text_fragments.iter().map(|f| f.bounds.width).sum();
    assert!(
        fragment_width <= 288.0 + 0.5,
        "truncated fragment width {fragment_width} exceeds 288"
    );
}

#[test]
fn s5_flex_one_respects_max_width_and_keeps_boxes_left_to_right() {
    let root = html_root(
        r#"<div class="flex w-[900px]">
  <div class="w-[200px]"></div>
  <div class="flex-1 max-w-md"></div>
  <div class="w-[100px]"></div>
</div>"#,
    );

    let row = find_explicit_width(&root, 900.0).expect("flex row");
    assert_eq!(row.children.len(), 3, "flex row must keep all three boxes");

    let first = &row.children[0];
    let middle = &row.children[1];
    let right = &row.children[2];

    assert_close(first.geometry.border_box.width, 200.0, 0.5, "first width");
    assert_close(middle.geometry.border_box.width, 448.0, 0.5, "middle width");
    assert_close(right.geometry.border_box.width, 100.0, 0.5, "right width");

    assert!(
        first.geometry.border_box.x < middle.geometry.border_box.x
            && middle.geometry.border_box.x < right.geometry.border_box.x,
        "boxes are not laid out left-to-right: {:?} {:?} {:?}",
        first.geometry.border_box,
        middle.geometry.border_box,
        right.geometry.border_box
    );

    // The reported snippet uses the default `justify-content: flex-start`, so
    // the 152px left over after the max-width clamp stays at the trailing edge.
    let right_edge = right.geometry.border_box.x + right.geometry.border_box.width;
    assert_close(right_edge, 748.0, 0.5, "right box edge");
}

#[test]
fn s5_justified_variant_anchors_the_right_cluster_to_the_row_edge() {
    // The consumer row anchors its right cluster to the row edge; reproducing
    // that geometry requires `justify-between`, which the reported snippet
    // omits. This variant keeps the edge assertion exercised.
    let root = html_root(
        r#"<div class="flex justify-between w-[900px]">
  <div class="w-[200px]"></div>
  <div class="flex-1 max-w-md"></div>
  <div class="w-[100px]"></div>
</div>"#,
    );

    let row = find_explicit_width(&root, 900.0).expect("flex row");
    let middle = &row.children[1];
    let right = &row.children[2];

    assert_close(middle.geometry.border_box.width, 448.0, 0.5, "middle width");
    let right_edge = right.geometry.border_box.x + right.geometry.border_box.width;
    assert_close(right_edge, 900.0, 0.5, "right box edge");
}

#[test]
fn s5_width_utilities_reach_layout_style_through_transpilation() {
    let root = html_root(
        r#"<div class="flex w-[900px]">
  <div class="w-max"></div>
  <div class="max-w-[420px]"></div>
</div>"#,
    );

    let row = find_explicit_width(&root, 900.0).expect("flex row");
    let w_max = &row.children[0];
    let max_w = &row.children[1];

    assert_eq!(
        w_max.style.width,
        Dimension::MaxContent,
        "w-max must reach the layout style as max-content"
    );
    assert_eq!(
        max_w.style.max_width,
        Dimension::Px(420.0),
        "max-w-[420px] must reach the layout style as a 420px cap"
    );

    // Keep the generic finders exercised so they cannot silently rot.
    assert!(find_width(&root, 900.0).is_some());
    assert!(find_explicit_height(&root, 900.0).is_none());
}
