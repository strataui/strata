//! Collapsed-border table row rhythm parity checks.
//!
#![allow(clippy::field_reassign_with_default)]
//! A collapsed-border table shares each internal horizontal border between the
//! bottom edge of one row and the top edge of the next. Counting both borders
//! makes every internal boundary drift one pixel per row. These tests pin the
//! collapsed row rhythm used by the Chromium reference capture.

use strata_layout::LayoutNode;
use strata_style::types::{
    BorderCollapse, ComputedStyle, Dimension, Display, EdgeInsets, FontWeight,
};

fn cell(text: &str, header: bool) -> LayoutNode {
    let mut style = ComputedStyle::default();
    style.display = Display::TableCell;
    style.padding = EdgeInsets {
        top: 8.0,
        right: 10.0,
        bottom: 8.0,
        left: 10.0,
    };
    style.border_width = EdgeInsets {
        top: 1.0,
        right: 1.0,
        bottom: 1.0,
        left: 1.0,
    };
    style.font_size = 14.0;
    style.line_height = 20.0 / 14.0;
    if header {
        style.font_weight = FontWeight::Bold;
    }
    LayoutNode::with_text(style, text.to_string())
}

fn row(cells: Vec<LayoutNode>) -> LayoutNode {
    let mut style = ComputedStyle::default();
    style.display = Display::TableRow;
    LayoutNode::with_children(style, cells)
}

fn build_table() -> LayoutNode {
    let mut table_style = ComputedStyle::default();
    table_style.display = Display::Table;
    table_style.table_layout = strata_style::types::TableLayout::Auto;
    table_style.border_collapse = BorderCollapse::Collapse;
    table_style.width = Dimension::Px(1000.0);

    let header = row(vec![
        cell("Region", true),
        cell("Requests", true),
        cell("Latency", true),
        cell("Status", true),
    ]);
    let body: Vec<LayoutNode> = [
        ("us-east-1", "420,110", "12 ms", "Healthy"),
        ("eu-central-1", "218,540", "18 ms", "Healthy"),
        ("ap-northeast-1", "176,202", "24 ms", "Degraded"),
    ]
    .into_iter()
    .map(|(region, requests, latency, status)| {
        row(vec![
            cell(region, false),
            cell(requests, false),
            cell(latency, false),
            cell(status, false),
        ])
    })
    .collect();

    let mut rows = vec![header];
    rows.extend(body);
    LayoutNode::with_children(table_style, rows)
}

#[test]
fn collapsed_internal_horizontal_borders_count_once() {
    let mut table = build_table();
    table.compute_layout(1280.0, 720.0);

    let rows = &table.children;
    assert_eq!(rows.len(), 4, "header plus three body rows");

    let boundaries: Vec<f32> = rows
        .iter()
        .map(|row| row.geometry.border_box.y)
        .chain(std::iter::once(
            rows[3].geometry.border_box.y + rows[3].geometry.border_box.height,
        ))
        .collect();

    let relative: Vec<f32> = boundaries
        .iter()
        .map(|y| y - table.geometry.border_box.y)
        .collect();

    assert_eq!(relative, vec![0.0, 37.0, 74.0, 111.0, 148.0]);

    for (idx, row) in rows.iter().enumerate() {
        assert_eq!(
            row.geometry.border_box.height, 37.0,
            "row {} height must count the shared collapsed border once",
            idx
        );
    }
}
