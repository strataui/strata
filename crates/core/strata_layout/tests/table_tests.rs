//! Comprehensive Verification Suite for W3C CSS Table Layout Engine.
//!
#![allow(clippy::field_reassign_with_default)]
//! Verifies:
//! 1. Column alignment across distinct rows.
//! 2. Colspan spanning multiple columns and track expansion.
//! 3. Rowspan spanning multiple rows and vertical track expansion.
//! 4. Content-based min/max track sizing (W3C §17.5.2).
//! 5. Vertical alignment (Baseline, Top, Middle, Bottom).
//! 6. Border-spacing and border-collapse spacing rules.

use strata_layout::LayoutNode;
use strata_style::types::{
    BorderCollapse, CaptionSide, Clear, ComputedStyle, Dimension, Display, EdgeInsets, Float,
    TableLayout, VerticalAlign,
};

#[test]
fn test_table_column_alignment_across_rows() {
    let mut table_style = ComputedStyle::default();
    table_style.display = Display::Table;
    table_style.table_layout = TableLayout::Auto;
    table_style.border_collapse = BorderCollapse::Separate;
    table_style.border_spacing = (8.0, 4.0);
    table_style.width = Dimension::Px(600.0);

    let mut row0_style = ComputedStyle::default();
    row0_style.display = Display::TableRow;

    let mut row1_style = ComputedStyle::default();
    row1_style.display = Display::TableRow;

    let mut c0_style = ComputedStyle::default();
    c0_style.display = Display::TableCell;
    let mut c1_style = ComputedStyle::default();
    c1_style.display = Display::TableCell;
    let mut c2_style = ComputedStyle::default();
    c2_style.display = Display::TableCell;

    let r0_c0 = LayoutNode::with_text(c0_style.clone(), "Col 1 Header".to_string());
    let r0_c1 = LayoutNode::with_text(c1_style.clone(), "Col 2 Wide Column Header Text".to_string());
    let r0_c2 = LayoutNode::with_text(c2_style.clone(), "Col 3".to_string());

    let r1_c0 = LayoutNode::with_text(c0_style.clone(), "Short".to_string());
    let r1_c1 = LayoutNode::with_text(c1_style.clone(), "Col 2 Detailed Content".to_string());
    let r1_c2 = LayoutNode::with_text(c2_style.clone(), "Small".to_string());

    let row0 = LayoutNode::with_children(row0_style, vec![r0_c0, r0_c1, r0_c2]);
    let row1 = LayoutNode::with_children(row1_style, vec![r1_c0, r1_c1, r1_c2]);

    let mut table = LayoutNode::with_children(table_style, vec![row0, row1]);
    table.compute_layout(600.0, 400.0);

    let r0 = &table.children[0];
    let r1 = &table.children[1];

    // Perfect column X position alignment across rows
    assert_eq!(r0.children[0].geometry.border_box.x, r1.children[0].geometry.border_box.x);
    assert_eq!(r0.children[1].geometry.border_box.x, r1.children[1].geometry.border_box.x);
    assert_eq!(r0.children[2].geometry.border_box.x, r1.children[2].geometry.border_box.x);

    // Perfect column width alignment across rows
    assert_eq!(r0.children[0].geometry.border_box.width, r1.children[0].geometry.border_box.width);
    assert_eq!(r0.children[1].geometry.border_box.width, r1.children[1].geometry.border_box.width);
    assert_eq!(r0.children[2].geometry.border_box.width, r1.children[2].geometry.border_box.width);

    // Left-to-right sequential layout with spacing
    assert!(r0.children[0].geometry.border_box.x + r0.children[0].geometry.border_box.width <= r0.children[1].geometry.border_box.x);
    assert!(r0.children[1].geometry.border_box.x + r0.children[1].geometry.border_box.width <= r0.children[2].geometry.border_box.x);
}

#[test]
fn test_table_colspan_spanning_multiple_tracks() {
    let mut table_style = ComputedStyle::default();
    table_style.display = Display::Table;
    table_style.border_collapse = BorderCollapse::Separate;
    table_style.border_spacing = (10.0, 5.0);
    table_style.width = Dimension::Px(600.0);

    let mut row0_style = ComputedStyle::default();
    row0_style.display = Display::TableRow;

    let mut row1_style = ComputedStyle::default();
    row1_style.display = Display::TableRow;

    // Row 0 has a cell spanning 2 columns, and a normal cell
    let mut span_style = ComputedStyle::default();
    span_style.display = Display::TableCell;
    span_style.colspan = 2;

    let mut cell_style = ComputedStyle::default();
    cell_style.display = Display::TableCell;

    let r0_c0 = LayoutNode::with_text(span_style, "Spanning Two Columns".to_string());
    let r0_c1 = LayoutNode::with_text(cell_style.clone(), "Third Column Header".to_string());

    // Row 1 has 3 individual single-span cells
    let r1_c0 = LayoutNode::with_text(cell_style.clone(), "Column 1".to_string());
    let r1_c1 = LayoutNode::with_text(cell_style.clone(), "Column 2".to_string());
    let r1_c2 = LayoutNode::with_text(cell_style.clone(), "Column 3".to_string());

    let row0 = LayoutNode::with_children(row0_style, vec![r0_c0, r0_c1]);
    let row1 = LayoutNode::with_children(row1_style, vec![r1_c0, r1_c1, r1_c2]);

    let mut table = LayoutNode::with_children(table_style, vec![row0, row1]);
    table.compute_layout(600.0, 400.0);

    let r0 = &table.children[0];
    let r1 = &table.children[1];

    let span_cell = &r0.children[0];
    let c0 = &r1.children[0];
    let c1 = &r1.children[1];
    let c2 = &r1.children[2];

    // Spanning cell width must equal c0.width + c1.width + horizontal border_spacing (10.0)
    let expected_span_w = c0.geometry.border_box.width + c1.geometry.border_box.width + 10.0;
    assert!((span_cell.geometry.border_box.width - expected_span_w).abs() < 0.01);

    // The cell following the span in row 0 must align with column 2 in row 1
    assert_eq!(r0.children[1].geometry.border_box.x, c2.geometry.border_box.x);
    assert_eq!(r0.children[1].geometry.border_box.width, c2.geometry.border_box.width);
}

#[test]
fn test_table_rowspan_spanning_multiple_tracks() {
    let mut table_style = ComputedStyle::default();
    table_style.display = Display::Table;
    table_style.border_collapse = BorderCollapse::Separate;
    table_style.border_spacing = (8.0, 6.0);
    table_style.width = Dimension::Px(400.0);

    let mut row0_style = ComputedStyle::default();
    row0_style.display = Display::TableRow;

    let mut row1_style = ComputedStyle::default();
    row1_style.display = Display::TableRow;

    // Row 0: Cell 0 spans 2 rows, Cell 1 spans 1 row
    let mut rowspan_style = ComputedStyle::default();
    rowspan_style.display = Display::TableCell;
    rowspan_style.rowspan = 2;

    let mut cell_style = ComputedStyle::default();
    cell_style.display = Display::TableCell;

    let r0_c0 = LayoutNode::with_text(rowspan_style, "Rowspan Cell".to_string());
    let r0_c1 = LayoutNode::with_text(cell_style.clone(), "Row 0 Cell 1".to_string());

    // Row 1: only has 1 cell (placed in column 1 because column 0 is taken by rowspan)
    let r1_c0 = LayoutNode::with_text(cell_style.clone(), "Row 1 Cell 1".to_string());

    let row0 = LayoutNode::with_children(row0_style, vec![r0_c0, r0_c1]);
    let row1 = LayoutNode::with_children(row1_style, vec![r1_c0]);

    let mut table = LayoutNode::with_children(table_style, vec![row0, row1]);
    table.compute_layout(400.0, 400.0);

    let r0 = &table.children[0];
    let r1 = &table.children[1];

    let rowspan_cell = &r0.children[0];
    let r0_c1_cell = &r0.children[1];
    let r1_c0_cell = &r1.children[0];

    // Row 1's cell must be in column 1, aligning with Row 0's column 1
    assert_eq!(r0_c1_cell.geometry.border_box.x, r1_c0_cell.geometry.border_box.x);

    // Rowspan cell height must equal row0.height + row1.height + vertical border_spacing (6.0)
    let expected_h = r0.geometry.border_box.height + r1.geometry.border_box.height + 6.0;
    assert!((rowspan_cell.geometry.border_box.height - expected_h).abs() < 0.01);
}

#[test]
fn test_table_auto_layout_content_based_sizing() {
    let mut table_style = ComputedStyle::default();
    table_style.display = Display::Table;
    table_style.table_layout = TableLayout::Auto;
    table_style.border_collapse = BorderCollapse::Collapse;
    table_style.width = Dimension::Px(800.0);

    let mut row_style = ComputedStyle::default();
    row_style.display = Display::TableRow;

    let mut c_style = ComputedStyle::default();
    c_style.display = Display::TableCell;

    let c0 = LayoutNode::with_text(c_style.clone(), "Tiny".to_string());
    let c1 = LayoutNode::with_text(
        c_style.clone(),
        "This is an exceptionally wide and detailed description of the item that takes much more space".to_string(),
    );

    let row = LayoutNode::with_children(row_style, vec![c0, c1]);
    let mut table = LayoutNode::with_children(table_style, vec![row]);
    table.compute_layout(800.0, 400.0);

    let r = &table.children[0];
    let w0 = r.children[0].geometry.border_box.width;
    let w1 = r.children[1].geometry.border_box.width;

    // The column with much wider content must receive substantially more track width
    assert!(w1 > w0 * 2.0);
    // Total width fills table width
    assert!((w0 + w1 - 800.0).abs() < 0.1);
}

#[test]
fn test_table_cell_vertical_alignment() {
    let mut table_style = ComputedStyle::default();
    table_style.display = Display::Table;
    table_style.border_collapse = BorderCollapse::Collapse;
    table_style.width = Dimension::Px(500.0);

    let mut row_style = ComputedStyle::default();
    row_style.display = Display::TableRow;

    // Tall cell (120px)
    let mut tall_style = ComputedStyle::default();
    tall_style.display = Display::TableCell;
    tall_style.height = Dimension::Px(120.0);
    let c0 = LayoutNode::new(tall_style);

    // Short cell (30px) with VerticalAlign::Middle
    let mut mid_style = ComputedStyle::default();
    mid_style.display = Display::TableCell;
    mid_style.height = Dimension::Px(30.0);
    mid_style.vertical_align = VerticalAlign::Middle;

    let mut child_style = ComputedStyle::default();
    child_style.display = Display::Block;
    child_style.height = Dimension::Px(30.0);
    let child = LayoutNode::new(child_style);
    let c1 = LayoutNode::with_children(mid_style, vec![child]);

    let row = LayoutNode::with_children(row_style, vec![c0, c1]);
    let mut table = LayoutNode::with_children(table_style, vec![row]);
    table.compute_layout(500.0, 400.0);

    let r = &table.children[0];
    // Both cells stretch to the row height of 120px
    assert_eq!(r.children[0].geometry.border_box.height, 120.0);
    assert_eq!(r.children[1].geometry.border_box.height, 120.0);

    // Middle vertical alignment shifts the child down by (120 - 30) / 2 = 45px
    let child_y = r.children[1].children[0].geometry.border_box.y;
    let cell_y = r.children[1].geometry.border_box.y;
    assert!((child_y - (cell_y + 45.0)).abs() < 0.01);
}

#[test]
fn test_table_border_collapse_and_separate() {
    let mut sep_style = ComputedStyle::default();
    sep_style.display = Display::Table;
    sep_style.border_collapse = BorderCollapse::Separate;
    sep_style.border_spacing = (12.0, 10.0);
    sep_style.width = Dimension::Px(400.0);

    let mut col_style = ComputedStyle::default();
    col_style.display = Display::Table;
    col_style.border_collapse = BorderCollapse::Collapse;
    col_style.width = Dimension::Px(400.0);

    let mut row_style = ComputedStyle::default();
    row_style.display = Display::TableRow;

    let mut cell_style = ComputedStyle::default();
    cell_style.display = Display::TableCell;

    let r0 = LayoutNode::with_children(
        row_style.clone(),
        vec![
            LayoutNode::with_text(cell_style.clone(), "A".to_string()),
            LayoutNode::with_text(cell_style.clone(), "B".to_string()),
        ],
    );
    let mut table_sep = LayoutNode::with_children(sep_style, vec![r0]);
    table_sep.compute_layout(400.0, 200.0);

    let r1 = LayoutNode::with_children(
        row_style,
        vec![
            LayoutNode::with_text(cell_style.clone(), "A".to_string()),
            LayoutNode::with_text(cell_style, "B".to_string()),
        ],
    );
    let mut table_col = LayoutNode::with_children(col_style, vec![r1]);
    table_col.compute_layout(400.0, 200.0);

    let sep_r = &table_sep.children[0];
    let sep_gap = sep_r.children[1].geometry.border_box.x
        - (sep_r.children[0].geometry.border_box.x + sep_r.children[0].geometry.border_box.width);
    assert!((sep_gap - 12.0).abs() < 0.01);

    let col_r = &table_col.children[0];
    let col_gap = col_r.children[1].geometry.border_box.x
        - (col_r.children[0].geometry.border_box.x + col_r.children[0].geometry.border_box.width);
    assert!(col_gap.abs() < 0.01);
}

#[test]
fn test_fixed_table_layout_algorithm() {
    // W3C CSS 2.1 §17.5.1 Fixed Table Layout test
    let mut table_style = ComputedStyle::default();
    table_style.display = Display::Table;
    table_style.table_layout = TableLayout::Fixed;
    table_style.width = Dimension::Px(600.0);

    let mut row_style = ComputedStyle::default();
    row_style.display = Display::TableRow;

    let mut c0_style = ComputedStyle::default();
    c0_style.display = Display::TableCell;
    c0_style.width = Dimension::Px(150.0);

    let mut c1_style = ComputedStyle::default();
    c1_style.display = Display::TableCell;
    c1_style.width = Dimension::Px(250.0);

    let mut c2_style = ComputedStyle::default();
    c2_style.display = Display::TableCell;
    // Auto width: should take remaining 600 - 400 = 200px

    let r0_c0 = LayoutNode::with_text(c0_style, "Fixed 150px".to_string());
    let r0_c1 = LayoutNode::with_text(c1_style, "Fixed 250px".to_string());
    let r0_c2 = LayoutNode::with_text(c2_style.clone(), "Auto column".to_string());

    let row0 = LayoutNode::with_children(row_style.clone(), vec![r0_c0, r0_c1, r0_c2]);

    // Row 1 has huge content in column 0, but in fixed layout column 0 stays 150px
    let r1_c0 = LayoutNode::with_text(c2_style.clone(), "Super extremely long text content that would expand auto tables".to_string());
    let r1_c1 = LayoutNode::with_text(c2_style.clone(), "Content 1".to_string());
    let r1_c2 = LayoutNode::with_text(c2_style, "Content 2".to_string());

    let row1 = LayoutNode::with_children(row_style, vec![r1_c0, r1_c1, r1_c2]);

    let mut table = LayoutNode::with_children(table_style, vec![row0, row1]);
    table.compute_layout(600.0, 400.0);

    let r0 = &table.children[0];
    let r1 = &table.children[1];

    assert_eq!(r0.children[0].geometry.border_box.width, 150.0);
    assert_eq!(r0.children[1].geometry.border_box.width, 250.0);
    assert_eq!(r0.children[2].geometry.border_box.width, 200.0);

    // Row 1 columns must match row 0 column widths exactly
    assert_eq!(r1.children[0].geometry.border_box.width, 150.0);
    assert_eq!(r1.children[1].geometry.border_box.width, 250.0);
    assert_eq!(r1.children[2].geometry.border_box.width, 200.0);
}

#[test]
fn test_table_caption_top_and_bottom_positioning() {
    // W3C CSS 2.1 §17.4 Table Caption test
    let mut table_style = ComputedStyle::default();
    table_style.display = Display::Table;
    table_style.width = Dimension::Px(400.0);

    let mut top_cap_style = ComputedStyle::default();
    top_cap_style.display = Display::TableCaption;
    top_cap_style.caption_side = CaptionSide::Top;
    top_cap_style.height = Dimension::Px(30.0);
    top_cap_style.margin = EdgeInsets { top: 5.0, right: 0.0, bottom: 5.0, left: 0.0 };

    let mut bot_cap_style = ComputedStyle::default();
    bot_cap_style.display = Display::TableCaption;
    bot_cap_style.caption_side = CaptionSide::Bottom;
    bot_cap_style.height = Dimension::Px(25.0);
    bot_cap_style.margin = EdgeInsets { top: 4.0, right: 0.0, bottom: 6.0, left: 0.0 };

    let mut row_style = ComputedStyle::default();
    row_style.display = Display::TableRow;

    let mut cell_style = ComputedStyle::default();
    cell_style.display = Display::TableCell;
    cell_style.height = Dimension::Px(60.0);

    let top_cap = LayoutNode::new(top_cap_style);
    let bot_cap = LayoutNode::new(bot_cap_style);
    let row = LayoutNode::with_children(row_style, vec![LayoutNode::new(cell_style)]);

    let mut table = LayoutNode::with_children(table_style, vec![top_cap, row, bot_cap]);
    table.compute_layout(400.0, 500.0);

    // Top caption positioned at y = 5.0 (origin + top margin)
    let tc = &table.children[0];
    assert_eq!(tc.geometry.border_box.y, 5.0);
    assert_eq!(tc.geometry.border_box.height, 30.0);

    // Table row grid starts after top caption (5 + 30 + 5 = 40.0)
    let r = &table.children[1];
    assert_eq!(r.geometry.border_box.y, 40.0);
    assert_eq!(r.geometry.border_box.height, 60.0);

    // Bottom caption positioned after grid (40 + 60 + 4 = 104.0)
    let bc = &table.children[2];
    assert_eq!(bc.geometry.border_box.y, 104.0);
    assert_eq!(bc.geometry.border_box.height, 25.0);

    // Total table height = 104 + 25 + 6 = 135.0
    assert_eq!(table.geometry.border_box.height, 135.0);
}

#[test]
fn test_table_baseline_alignment() {
    // W3C CSS 2.1 §17.5.3 Cell Baseline Alignment
    let mut table_style = ComputedStyle::default();
    table_style.display = Display::Table;
    table_style.width = Dimension::Px(500.0);

    let mut row_style = ComputedStyle::default();
    row_style.display = Display::TableRow;

    let mut c0_style = ComputedStyle::default();
    c0_style.display = Display::TableCell;
    c0_style.vertical_align = VerticalAlign::Baseline;
    c0_style.padding = EdgeInsets { top: 20.0, right: 10.0, bottom: 10.0, left: 10.0 };

    let mut c1_style = ComputedStyle::default();
    c1_style.display = Display::TableCell;
    c1_style.vertical_align = VerticalAlign::Baseline;
    c1_style.padding = EdgeInsets { top: 5.0, right: 10.0, bottom: 10.0, left: 10.0 };

    let mut c0 = LayoutNode::new(c0_style);
    let mut span0 = LayoutNode::new(ComputedStyle {
        display: Display::Inline,
        font_size: 16.0,
        ..Default::default()
    });
    span0.text = Some("Cell A with padding".to_string());
    c0.add_child(span0);

    let mut c1 = LayoutNode::new(c1_style);
    let mut span1 = LayoutNode::new(ComputedStyle {
        display: Display::Inline,
        font_size: 16.0,
        ..Default::default()
    });
    span1.text = Some("Cell B with small padding".to_string());
    c1.add_child(span1);

    let row = LayoutNode::with_children(row_style, vec![c0, c1]);
    let mut table = LayoutNode::with_children(table_style, vec![row]);
    table.compute_layout(500.0, 300.0);

    let r = &table.children[0];
    let cell0 = &r.children[0];
    let cell1 = &r.children[1];

    // Both cells must have computed baselines and valid layout
    assert!(!cell0.line_boxes.is_empty());
    assert!(!cell1.line_boxes.is_empty());

    let base0 = cell0.line_boxes[0].bounds.y + cell0.line_boxes[0].baseline;
    let base1 = cell1.line_boxes[0].bounds.y + cell1.line_boxes[0].baseline;

    // Baselines must align within 0.5px
    assert!((base0 - base1).abs() < 0.5, "base0 ({}) should align with base1 ({})", base0, base1);
}

#[test]
fn test_table_float_and_clearance_layout() {
    // Floated table parity test (Wikipedia Infobox pattern)
    let mut root_style = ComputedStyle::default();
    root_style.display = Display::Block;
    root_style.width = Dimension::Px(800.0);

    // Floated table (float: right, width: 300px, height: 150px)
    let mut table_style = ComputedStyle::default();
    table_style.display = Display::Table;
    table_style.float = Float::Right;
    table_style.width = Dimension::Px(300.0);
    table_style.margin = EdgeInsets { top: 0.0, right: 0.0, bottom: 20.0, left: 20.0 };

    let mut row_style = ComputedStyle::default();
    row_style.display = Display::TableRow;
    let mut cell_style = ComputedStyle::default();
    cell_style.display = Display::TableCell;
    cell_style.height = Dimension::Px(150.0);

    let row = LayoutNode::with_children(row_style, vec![LayoutNode::new(cell_style)]);
    let table = LayoutNode::with_children(table_style, vec![row]);

    // Paragraph wrapping around table
    let mut p_style = ComputedStyle::default();
    p_style.display = Display::Block;
    let mut p = LayoutNode::new(p_style);
    let mut p_span = LayoutNode::new(ComputedStyle {
        display: Display::Inline,
        font_size: 16.0,
        ..Default::default()
    });
    p_span.text = Some("This is paragraph text that wraps around the right-floated table infobox.".to_string());
    p.add_child(p_span);

    // Cleared block below the float
    let mut clear_style = ComputedStyle::default();
    clear_style.display = Display::Block;
    clear_style.clear = Clear::Both;
    clear_style.height = Dimension::Px(40.0);
    let clear_box = LayoutNode::new(clear_style);

    let mut container = LayoutNode::with_children(root_style, vec![table, p, clear_box]);
    container.compute_layout(800.0, 600.0);

    let tbl = &container.children[0];
    let paragraph = &container.children[1];
    let clr = &container.children[2];

    // Floated table is positioned at the right edge: 800 - (300 + 20) + 20 = 500
    assert_eq!(tbl.geometry.border_box.x, 500.0);
    assert_eq!(tbl.geometry.border_box.width, 300.0);

    // Paragraph wraps around float
    assert!(!paragraph.line_boxes.is_empty());
    // Line 0 width is constrained by the float
    assert!(paragraph.line_boxes[0].bounds.width <= 500.0);

    // Cleared box must be placed strictly below the float's bottom margin (150 + 20 = 170.0)
    assert!(clr.geometry.border_box.y >= 170.0, "Cleared box y ({}) must be >= 170.0", clr.geometry.border_box.y);
}

#[test]
fn test_table_collapsed_border_conflict_resolution_spec() {
    use strata_layout::table::{
        resolve_collapsed_border, CollapsedBorderEdge, ElementSource,
    };
    use strata_style::types::{BorderStyle, Color};

    // 1. 'border-style: hidden' takes precedence over all other conflicting borders and sets width = 0
    let hidden_edge = CollapsedBorderEdge {
        width: 10.0,
        style: BorderStyle::Hidden,
        color: Some(Color::rgb(255, 0, 0)),
        source: ElementSource::Cell,
    };
    let thick_solid = CollapsedBorderEdge {
        width: 20.0,
        style: BorderStyle::Solid,
        color: Some(Color::rgb(0, 0, 255)),
        source: ElementSource::Table,
    };
    let resolved = resolve_collapsed_border(hidden_edge, thick_solid);
    assert_eq!(resolved.style, BorderStyle::Hidden);
    assert_eq!(resolved.width, 0.0);
    assert!(resolved.color.is_none());

    // 2. 'border-style: none' has the lowest priority: any specified border wins over 'none'
    let none_edge = CollapsedBorderEdge {
        width: 0.0,
        style: BorderStyle::None,
        color: None,
        source: ElementSource::Cell,
    };
    let thin_solid = CollapsedBorderEdge {
        width: 1.0,
        style: BorderStyle::Solid,
        color: Some(Color::rgb(0, 255, 0)),
        source: ElementSource::Table,
    };
    let resolved = resolve_collapsed_border(none_edge, thin_solid);
    assert_eq!(resolved.style, BorderStyle::Solid);
    assert_eq!(resolved.width, 1.0);
    assert_eq!(resolved.color, Some(Color::rgb(0, 255, 0)));

    // 3. Wider border wins over narrower
    let wide = CollapsedBorderEdge {
        width: 4.0,
        style: BorderStyle::Dashed,
        color: Some(Color::rgb(10, 10, 10)),
        source: ElementSource::Cell,
    };
    let narrow = CollapsedBorderEdge {
        width: 2.0,
        style: BorderStyle::Double,
        color: Some(Color::rgb(20, 20, 20)),
        source: ElementSource::Cell,
    };
    let resolved = resolve_collapsed_border(narrow, wide);
    assert_eq!(resolved.width, 4.0);
    assert_eq!(resolved.style, BorderStyle::Dashed);

    // 4. If widths are identical, style priority: double > solid > dashed > dotted
    let double_edge = CollapsedBorderEdge {
        width: 3.0,
        style: BorderStyle::Double,
        color: Some(Color::rgb(1, 1, 1)),
        source: ElementSource::Cell,
    };
    let solid_edge = CollapsedBorderEdge {
        width: 3.0,
        style: BorderStyle::Solid,
        color: Some(Color::rgb(2, 2, 2)),
        source: ElementSource::Cell,
    };
    let dashed_edge = CollapsedBorderEdge {
        width: 3.0,
        style: BorderStyle::Dashed,
        color: Some(Color::rgb(3, 3, 3)),
        source: ElementSource::Cell,
    };
    let dotted_edge = CollapsedBorderEdge {
        width: 3.0,
        style: BorderStyle::Dotted,
        color: Some(Color::rgb(4, 4, 4)),
        source: ElementSource::Cell,
    };

    assert_eq!(resolve_collapsed_border(solid_edge, double_edge).style, BorderStyle::Double);
    assert_eq!(resolve_collapsed_border(dashed_edge, solid_edge).style, BorderStyle::Solid);
    assert_eq!(resolve_collapsed_border(dotted_edge, dashed_edge).style, BorderStyle::Dashed);

    // 5. If styles and widths are identical, element origin priority:
    // cell > row > row group > column > colgroup > table
    let cell_origin = CollapsedBorderEdge {
        width: 2.0,
        style: BorderStyle::Solid,
        color: Some(Color::rgb(100, 0, 0)),
        source: ElementSource::Cell,
    };
    let row_origin = CollapsedBorderEdge {
        width: 2.0,
        style: BorderStyle::Solid,
        color: Some(Color::rgb(0, 100, 0)),
        source: ElementSource::Row,
    };
    let table_origin = CollapsedBorderEdge {
        width: 2.0,
        style: BorderStyle::Solid,
        color: Some(Color::rgb(0, 0, 100)),
        source: ElementSource::Table,
    };
    assert_eq!(resolve_collapsed_border(row_origin, cell_origin).source, ElementSource::Cell);
    assert_eq!(resolve_collapsed_border(table_origin, row_origin).source, ElementSource::Row);

    // 6. When two elements of the same type have conflicting borders of the same
    // width and style, the top / left border wins
    let top_cell = CollapsedBorderEdge {
        width: 2.0,
        style: BorderStyle::Solid,
        color: Some(Color::rgb(123, 0, 0)),
        source: ElementSource::Cell,
    };
    let bottom_cell = CollapsedBorderEdge {
        width: 2.0,
        style: BorderStyle::Solid,
        color: Some(Color::rgb(0, 123, 0)),
        source: ElementSource::Cell,
    };
    let resolved = resolve_collapsed_border(top_cell, bottom_cell);
    assert_eq!(resolved.color, Some(Color::rgb(123, 0, 0)));
}

#[test]
fn test_table_border_grid_layout_and_hidden_suppression() {
    use strata_style::types::{BorderStyle, Color, EdgeInsets};

    let mut table_style = ComputedStyle::default();
    table_style.display = Display::Table;
    table_style.border_collapse = BorderCollapse::Collapse;
    table_style.width = Dimension::Px(400.0);
    table_style.border_width = EdgeInsets {
        top: 2.0,
        right: 2.0,
        bottom: 2.0,
        left: 2.0,
    };
    table_style.border_style = BorderStyle::Solid;
    table_style.border_color = Some(Color::rgb(0, 0, 0));

    let mut row_style = ComputedStyle::default();
    row_style.display = Display::TableRow;

    // Row 0 Cell 0: standard 1px solid
    let mut c00_style = ComputedStyle::default();
    c00_style.display = Display::TableCell;
    c00_style.border_width = EdgeInsets {
        top: 1.0,
        right: 1.0,
        bottom: 1.0,
        left: 1.0,
    };
    c00_style.border_style = BorderStyle::Solid;
    c00_style.border_color = Some(Color::rgb(50, 50, 50));

    // Row 0 Cell 1: hidden border
    let mut c01_style = ComputedStyle::default();
    c01_style.display = Display::TableCell;
    c01_style.border_width = EdgeInsets {
        top: 1.0,
        right: 1.0,
        bottom: 1.0,
        left: 1.0,
    };
    c01_style.border_style = BorderStyle::Hidden;

    // Row 1 Cell 0: 3px double
    let mut c10_style = ComputedStyle::default();
    c10_style.display = Display::TableCell;
    c10_style.border_width = EdgeInsets {
        top: 3.0,
        right: 3.0,
        bottom: 3.0,
        left: 3.0,
    };
    c10_style.border_style = BorderStyle::Double;
    c10_style.border_color = Some(Color::rgb(80, 80, 80));

    // Row 1 Cell 1: standard 1px solid
    let mut c11_style = ComputedStyle::default();
    c11_style.display = Display::TableCell;
    c11_style.border_width = EdgeInsets {
        top: 1.0,
        right: 1.0,
        bottom: 1.0,
        left: 1.0,
    };
    c11_style.border_style = BorderStyle::Solid;
    c11_style.border_color = Some(Color::rgb(50, 50, 50));

    let r0 = LayoutNode::with_children(
        row_style.clone(),
        vec![
            LayoutNode::with_text(c00_style, "C00".to_string()),
            LayoutNode::with_text(c01_style, "C01".to_string()),
        ],
    );
    let r1 = LayoutNode::with_children(
        row_style,
        vec![
            LayoutNode::with_text(c10_style, "C10".to_string()),
            LayoutNode::with_text(c11_style, "C11".to_string()),
        ],
    );

    let mut table = LayoutNode::with_children(table_style, vec![r0, r1]);
    table.compute_layout(400.0, 200.0);

    // Verify TableBorderGrid is populated
    assert!(table.table_grid.is_some(), "table_grid must be populated on collapsed tables");
    let grid = table.table_grid.as_ref().unwrap();
    assert_eq!(grid.num_rows, 2);
    assert_eq!(grid.num_cols, 2);

    // Boundary between (0,1) and (1,1): c01 has BorderStyle::Hidden, so horizontal edge at (1, 1) is Hidden!
    assert_eq!(grid.horizontal_edges[1][1].style, BorderStyle::Hidden);
    assert_eq!(grid.horizontal_edges[1][1].width, 0.0);

    // Boundary between (0,0) and (1,0): c10 has 3px double vs c00 1px solid -> c10 wins (wider: 3px)
    assert_eq!(grid.horizontal_edges[1][0].width, 3.0);
    assert_eq!(grid.horizontal_edges[1][0].style, BorderStyle::Double);
}


