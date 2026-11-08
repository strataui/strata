//! W3C CSS 2.1 Chapter 17 Table Layout Engine.
//!
//! Implements W3C CSS 2.1 §17.5 (Table Layout), §17.5.1 (Fixed & Auto Sizing),
//! §17.5.2 (Auto Table Sizing Algorithm), §17.5.3 (Cell Vertical Alignment),
//! §17.4 (Table Captions & Wrapper Box), and §17.6 (Borders: Separated vs Collapsed).

#![allow(clippy::needless_range_loop)]

use crate::box_model::{clamp_dimension, resolve_dimension};
use crate::geometry::{LayoutGeometry, Rect};
use crate::intrinsic::compute_min_max_sizes;
use crate::tree::LayoutNode;
use strata_style::types::{
    BorderCollapse, BorderStyle, CaptionSide, Color, Display, Position, TableLayout,
    VerticalAlign,
};

/// Origin element type of a border edge in the collapsing border model.
/// Priority order per CSS 2.1 §17.6.2.1:
/// cell > row > row group > column > colgroup > table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ElementSource {
    Table = 0,
    ColGroup = 1,
    Col = 2,
    RowGroup = 3,
    Row = 4,
    Cell = 5,
}

/// A resolved or candidate border edge in the collapsing border model (CSS 2.1 §17.6.2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CollapsedBorderEdge {
    pub width: f32,
    pub style: BorderStyle,
    pub color: Option<Color>,
    pub source: ElementSource,
}

impl Default for CollapsedBorderEdge {
    fn default() -> Self {
        Self {
            width: 0.0,
            style: BorderStyle::None,
            color: None,
            source: ElementSource::Table,
        }
    }
}

/// Computes the priority ranking of a border style per CSS 2.1 §17.6.2.1:
/// 'double' > 'solid' > 'dashed' > 'dotted' > 'none'/'hidden'.
#[inline]
pub fn border_style_priority(style: BorderStyle) -> u8 {
    match style {
        BorderStyle::Double => 4,
        BorderStyle::Solid => 3,
        BorderStyle::Dashed => 2,
        BorderStyle::Dotted => 1,
        BorderStyle::None | BorderStyle::Hidden => 0,
    }
}

/// Resolves the winning border between two conflicting edges meeting at a grid boundary (CSS 2.1 §17.6.2.1).
///
/// `first` is the edge from the top or left element.
/// `second` is the edge from the bottom or right element.
pub fn resolve_collapsed_border(
    first: CollapsedBorderEdge,
    second: CollapsedBorderEdge,
) -> CollapsedBorderEdge {
    // 1. 'border-style: hidden' takes precedence over all other conflicting borders
    // and suppresses border rendering entirely (width = 0).
    if first.style == BorderStyle::Hidden || second.style == BorderStyle::Hidden {
        return CollapsedBorderEdge {
            width: 0.0,
            style: BorderStyle::Hidden,
            color: None,
            source: if first.style == BorderStyle::Hidden {
                first.source
            } else {
                second.source
            },
        };
    }

    // 2. 'border-style: none' has the lowest priority: any specified border wins over 'none'.
    let first_is_none = first.style == BorderStyle::None || first.width <= 0.0;
    let second_is_none = second.style == BorderStyle::None || second.width <= 0.0;

    if first_is_none && !second_is_none {
        return second;
    }
    if !first_is_none && second_is_none {
        return first;
    }
    if first_is_none && second_is_none {
        return CollapsedBorderEdge::default();
    }

    // 3. Wider border wins over narrower.
    const EPSILON: f32 = 0.001;
    if (first.width - second.width).abs() > EPSILON {
        return if first.width > second.width {
            first
        } else {
            second
        };
    }

    // 4. If widths are identical, style priority: double > solid > dashed > dotted
    let prio_first = border_style_priority(first.style);
    let prio_second = border_style_priority(second.style);
    if prio_first != prio_second {
        return if prio_first > prio_second {
            first
        } else {
            second
        };
    }

    // 5. If styles and widths are identical, element origin priority:
    // cell > row > row group > column > colgroup > table.
    if first.source != second.source {
        return if first.source > second.source {
            first
        } else {
            second
        };
    }

    // 6. When two elements of the same type have conflicting borders of the same
    // width and style, the border from the cell to the left / top wins.
    first
}

/// 2D grid matrix storing resolved collapsed borders and boundary coordinates (CSS 2.1 §17.6.2).
#[derive(Debug, Clone, PartialEq)]
pub struct TableBorderGrid {
    pub num_rows: usize,
    pub num_cols: usize,
    /// Horizontal boundary edges: `(num_rows + 1)` rows by `num_cols` columns.
    pub horizontal_edges: Vec<Vec<CollapsedBorderEdge>>,
    /// Vertical boundary edges: `num_rows` rows by `(num_cols + 1)` columns.
    pub vertical_edges: Vec<Vec<CollapsedBorderEdge>>,
    /// Y coordinates of the `num_rows + 1` horizontal grid lines.
    pub row_positions: Vec<f32>,
    /// X coordinates of the `num_cols + 1` vertical grid lines.
    pub col_positions: Vec<f32>,
}

impl TableBorderGrid {
    pub fn new(num_rows: usize, num_cols: usize) -> Self {
        Self {
            num_rows,
            num_cols,
            horizontal_edges: vec![vec![CollapsedBorderEdge::default(); num_cols]; num_rows + 1],
            vertical_edges: vec![vec![CollapsedBorderEdge::default(); num_cols + 1]; num_rows],
            row_positions: vec![0.0; num_rows + 1],
            col_positions: vec![0.0; num_cols + 1],
        }
    }

    pub fn translate(&mut self, dx: f32, dy: f32) {
        for y in &mut self.row_positions {
            *y += dy;
        }
        for x in &mut self.col_positions {
            *x += dx;
        }
    }
}


/// Location of a table cell within the container's hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellLocation {
    pub group_idx: Option<usize>,
    pub row_idx: usize,
    pub cell_idx: usize,
}

/// A cell placed within the 2D table grid matrix.
#[derive(Debug, Clone)]
struct PlacedCell {
    loc: CellLocation,
    row_start: usize,
    col_start: usize,
    col_span: usize,
    row_span: usize,
    min_w: f32,
    max_w: f32,
    baseline: f32,
}

/// Reference to a row within the table hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RowLocation {
    group_idx: Option<usize>,
    row_idx: usize,
}

/// Solves W3C CSS Table Layout for a table container and its rows/cells.
pub fn solve_table(container: &mut LayoutNode, available_width: f32, available_height: f32) {
    solve_table_guarded(
        container,
        available_width,
        available_height,
        crate::tree::LayoutDepthGuard::root(),
    );
}

/// Solves W3C CSS Table Layout with an active depth guard to prevent stack overflow.
pub fn solve_table_guarded(
    container: &mut LayoutNode,
    available_width: f32,
    available_height: f32,
    guard: crate::tree::LayoutDepthGuard,
) {
    let _prof = crate::prof::Scope::new(crate::prof::Slot::Table);
    let padding = container.style.padding;
    let border = container.style.border_width;
    let margin = container.style.margin;

    let (h_spacing, v_spacing) = match container.style.border_collapse {
        BorderCollapse::Collapse => (0.0, 0.0),
        BorderCollapse::Separate => container.style.border_spacing,
    };

    // 1. Discover Table Captions and Rows
    let mut top_captions: Vec<usize> = Vec::new();
    let mut bottom_captions: Vec<usize> = Vec::new();
    let mut rows: Vec<RowLocation> = Vec::new();

    for (i, child) in container.children.iter().enumerate() {
        match child.style.display {
            Display::TableCaption => {
                if child.style.caption_side == CaptionSide::Bottom {
                    bottom_captions.push(i);
                } else {
                    top_captions.push(i);
                }
            }
            Display::TableRowGroup | Display::TableHeaderGroup | Display::TableFooterGroup => {
                for (r, row) in child.children.iter().enumerate() {
                    if row.style.display == Display::TableRow {
                        rows.push(RowLocation {
                            group_idx: Some(i),
                            row_idx: r,
                        });
                    }
                }
            }
            Display::TableRow => {
                rows.push(RowLocation {
                    group_idx: None,
                    row_idx: i,
                });
            }
            _ => {}
        }
    }

    let child_guard = guard.enter();

    if rows.is_empty() {
        let mut total_h = padding.top + padding.bottom + border.top + border.bottom;
        let w = resolve_dimension(container.style.width, available_width).unwrap_or(0.0);
        let table_origin_x = container.geometry.border_box.x;
        let table_origin_y = container.geometry.border_box.y;

        let cg = child_guard.unwrap_or(guard);
        for &cap_idx in top_captions.iter().chain(bottom_captions.iter()) {
            let cap = &mut container.children[cap_idx];
            cap.compute_layout_guarded(w, available_height, None, 0.0, cg);
            let cap_h = cap.geometry.border_box.height;
            let cap_m = cap.style.margin;
            cap.geometry = LayoutGeometry::from_border_box(
                Rect::new(
                    table_origin_x + border.left + padding.left + cap_m.left,
                    table_origin_y + total_h + cap_m.top,
                    w,
                    cap_h,
                ),
                cap.style.padding,
                cap.style.border_width,
                cap_m,
            );
            total_h += cap_m.top + cap_h + cap_m.bottom;
        }

        container.geometry = LayoutGeometry::from_border_box(
            Rect::new(table_origin_x, table_origin_y, w, total_h),
            padding,
            border,
            margin,
        );
        return;
    }

    // 2. Build 2D Grid Matrix and place cells considering colspans and rowspans
    let mut occupied: Vec<Vec<bool>> = Vec::new();
    let mut placed_cells: Vec<PlacedCell> = Vec::new();

    for (r_idx, row_loc) in rows.iter().enumerate() {
        if occupied.len() <= r_idx {
            occupied.resize(r_idx + 1, Vec::new());
        }

        let cell_count = get_row(container, *row_loc).children.len();
        let mut col_cursor = 0;

        for c_idx in 0..cell_count {
            let (col_span, row_span) = {
                let cell = get_cell(
                    container,
                    CellLocation {
                        group_idx: row_loc.group_idx,
                        row_idx: row_loc.row_idx,
                        cell_idx: c_idx,
                    },
                );
                if cell.style.display == Display::None
                    || cell.style.position == Position::Absolute
                    || cell.style.position == Position::Fixed
                {
                    continue;
                }
                (
                    cell.style.colspan.max(1) as usize,
                    cell.style.rowspan.max(1) as usize,
                )
            };

            // Advance cursor past occupied cells
            while col_cursor < occupied[r_idx].len() && occupied[r_idx][col_cursor] {
                col_cursor += 1;
            }

            let start_col = col_cursor;
            let end_col = start_col + col_span;
            let end_row = r_idx + row_span;

            // Expand occupancy matrix
            if occupied.len() < end_row {
                occupied.resize(end_row, Vec::new());
            }
            for r in r_idx..end_row {
                if occupied[r].len() < end_col {
                    occupied[r].resize(end_col, false);
                }
                for c in start_col..end_col {
                    occupied[r][c] = true;
                }
            }

            placed_cells.push(PlacedCell {
                loc: CellLocation {
                    group_idx: row_loc.group_idx,
                    row_idx: row_loc.row_idx,
                    cell_idx: c_idx,
                },
                row_start: r_idx,
                col_start: start_col,
                col_span,
                row_span,
                min_w: 0.0,
                max_w: 0.0,
                baseline: 0.0,
            });

            col_cursor = end_col;
        }
    }

    let num_cols = occupied.iter().map(|r| r.len()).max().unwrap_or(0);
    let num_rows = occupied.len();

    if num_cols == 0 || num_rows == 0 {
        return;
    }

    // Resolve shared horizontal and vertical borders (CSS 2.1 §17.6.2).
    let collapsed = container.style.border_collapse == BorderCollapse::Collapse;
    let mut table_grid = if collapsed {
        Some(TableBorderGrid::new(num_rows, num_cols))
    } else {
        None
    };

    if collapsed {
        // Propagate border_collapse = Collapse to cells and rows
        for row_loc in &rows {
            let row = get_row_mut(container, *row_loc);
            row.style.border_collapse = BorderCollapse::Collapse;
            for cell in &mut row.children {
                if cell.style.display == Display::TableCell {
                    cell.style.border_collapse = BorderCollapse::Collapse;
                }
            }
        }

        // Build 2D occupancy mapping for O(1) cell lookup at boundaries
        let mut cell_at = vec![vec![None; num_cols]; num_rows];
        for (i, pc) in placed_cells.iter().enumerate() {
            let end_r = (pc.row_start + pc.row_span).min(num_rows);
            let end_c = (pc.col_start + pc.col_span).min(num_cols);
            for r in pc.row_start..end_r {
                for c in pc.col_start..end_c {
                    cell_at[r][c] = Some(i);
                }
            }
        }

        if let Some(ref mut grid) = table_grid {
            // Resolve horizontal edges (num_rows + 1 boundaries, num_cols columns)
            for r in 0..=num_rows {
                for c in 0..num_cols {
                    // 1. Top candidate (from elements above boundary r)
                    let top_candidate = if r == 0 {
                        // Table top border
                        if container.style.border_width.top > 0.0
                            || container.style.border_style == BorderStyle::Hidden
                        {
                            CollapsedBorderEdge {
                                width: container.style.border_width.top,
                                style: container.style.border_style,
                                color: container.style.border_color,
                                source: ElementSource::Table,
                            }
                        } else {
                            CollapsedBorderEdge::default()
                        }
                    } else {
                        // Check cell above: cell ending at row r
                        let cell_edge = if let Some(cell_idx) = cell_at[r - 1][c] {
                            let pc = &placed_cells[cell_idx];
                            if pc.row_start + pc.row_span == r {
                                let cell = get_cell(container, pc.loc);
                                CollapsedBorderEdge {
                                    width: cell.style.border_width.bottom,
                                    style: cell.style.border_style,
                                    color: cell.style.border_color,
                                    source: ElementSource::Cell,
                                }
                            } else {
                                CollapsedBorderEdge::default()
                            }
                        } else {
                            CollapsedBorderEdge::default()
                        };

                        // Check row above: row r - 1 bottom border
                        let row = get_row(container, rows[r - 1]);
                        let row_edge = if row.style.border_width.bottom > 0.0
                            || row.style.border_style == BorderStyle::Hidden
                        {
                            CollapsedBorderEdge {
                                width: row.style.border_width.bottom,
                                style: row.style.border_style,
                                color: row.style.border_color,
                                source: ElementSource::Row,
                            }
                        } else {
                            CollapsedBorderEdge::default()
                        };

                        resolve_collapsed_border(row_edge, cell_edge)
                    };

                    // 2. Bottom candidate (from elements below boundary r)
                    let bottom_candidate = if r == num_rows {
                        // Table bottom border
                        if container.style.border_width.bottom > 0.0
                            || container.style.border_style == BorderStyle::Hidden
                        {
                            CollapsedBorderEdge {
                                width: container.style.border_width.bottom,
                                style: container.style.border_style,
                                color: container.style.border_color,
                                source: ElementSource::Table,
                            }
                        } else {
                            CollapsedBorderEdge::default()
                        }
                    } else {
                        // Check cell below: cell starting at row r
                        let cell_edge = if let Some(cell_idx) = cell_at[r][c] {
                            let pc = &placed_cells[cell_idx];
                            if pc.row_start == r {
                                let cell = get_cell(container, pc.loc);
                                CollapsedBorderEdge {
                                    width: cell.style.border_width.top,
                                    style: cell.style.border_style,
                                    color: cell.style.border_color,
                                    source: ElementSource::Cell,
                                }
                            } else {
                                CollapsedBorderEdge::default()
                            }
                        } else {
                            CollapsedBorderEdge::default()
                        };

                        // Check row below: row r top border
                        let row = get_row(container, rows[r]);
                        let row_edge = if row.style.border_width.top > 0.0
                            || row.style.border_style == BorderStyle::Hidden
                        {
                            CollapsedBorderEdge {
                                width: row.style.border_width.top,
                                style: row.style.border_style,
                                color: row.style.border_color,
                                source: ElementSource::Row,
                            }
                        } else {
                            CollapsedBorderEdge::default()
                        };

                        resolve_collapsed_border(row_edge, cell_edge)
                    };

                    grid.horizontal_edges[r][c] =
                        resolve_collapsed_border(top_candidate, bottom_candidate);
                }
            }

            // Resolve vertical edges (num_rows rows, num_cols + 1 boundaries)
            for r in 0..num_rows {
                for c in 0..=num_cols {
                    // 1. Left candidate (from elements left of boundary c)
                    let left_candidate = if c == 0 {
                        // Table left border
                        if container.style.border_width.left > 0.0
                            || container.style.border_style == BorderStyle::Hidden
                        {
                            CollapsedBorderEdge {
                                width: container.style.border_width.left,
                                style: container.style.border_style,
                                color: container.style.border_color,
                                source: ElementSource::Table,
                            }
                        } else {
                            CollapsedBorderEdge::default()
                        }
                    } else {
                        // Check cell left: cell ending at column c
                        if let Some(cell_idx) = cell_at[r][c - 1] {
                            let pc = &placed_cells[cell_idx];
                            if pc.col_start + pc.col_span == c {
                                let cell = get_cell(container, pc.loc);
                                CollapsedBorderEdge {
                                    width: cell.style.border_width.right,
                                    style: cell.style.border_style,
                                    color: cell.style.border_color,
                                    source: ElementSource::Cell,
                                }
                            } else {
                                CollapsedBorderEdge::default()
                            }
                        } else {
                            CollapsedBorderEdge::default()
                        }
                    };

                    // 2. Right candidate (from elements right of boundary c)
                    let right_candidate = if c == num_cols {
                        // Table right border
                        if container.style.border_width.right > 0.0
                            || container.style.border_style == BorderStyle::Hidden
                        {
                            CollapsedBorderEdge {
                                width: container.style.border_width.right,
                                style: container.style.border_style,
                                color: container.style.border_color,
                                source: ElementSource::Table,
                            }
                        } else {
                            CollapsedBorderEdge::default()
                        }
                    } else {
                        // Check cell right: cell starting at column c
                        if let Some(cell_idx) = cell_at[r][c] {
                            let pc = &placed_cells[cell_idx];
                            if pc.col_start == c {
                                let cell = get_cell(container, pc.loc);
                                CollapsedBorderEdge {
                                    width: cell.style.border_width.left,
                                    style: cell.style.border_style,
                                    color: cell.style.border_color,
                                    source: ElementSource::Cell,
                                }
                            } else {
                                CollapsedBorderEdge::default()
                            }
                        } else {
                            CollapsedBorderEdge::default()
                        }
                    };

                    grid.vertical_edges[r][c] =
                        resolve_collapsed_border(left_candidate, right_candidate);
                }
            }
        }
    }

    let mut row_boundary = vec![0.0f32; num_rows + 1];
    if collapsed {
        if let Some(ref grid) = table_grid {
            for r in 0..=num_rows {
                let max_w = grid.horizontal_edges[r]
                    .iter()
                    .map(|edge| edge.width)
                    .fold(0.0f32, f32::max);
                row_boundary[r] = max_w;
            }
        }
    }

    // 3. Resolve Target Inner Width
    let total_h_spacing = (num_cols.saturating_sub(1)) as f32 * h_spacing;
    let table_h_extras =
        padding.left + padding.right + border.left + border.right + total_h_spacing;

    let is_fixed = container.style.table_layout == TableLayout::Fixed;

    let col_w = if is_fixed {
        // W3C CSS 2.1 §17.5.1 Fixed Table Layout
        let target_inner_w = match resolve_dimension(container.style.width, available_width) {
            Some(w) => {
                let clamped = clamp_dimension(
                    w,
                    container.style.min_width,
                    container.style.max_width,
                    available_width,
                );
                (clamped - table_h_extras).max(0.0)
            }
            None => (available_width - margin.left - margin.right - table_h_extras).max(0.0),
        };

        let mut fixed_col_w = vec![0.0f32; num_cols];
        let mut assigned = vec![false; num_cols];
        let mut assigned_sum = 0.0f32;
        let mut unassigned_count = 0;

        // Inspect cells in row 0 for explicit column widths
        for pc in &placed_cells {
            if pc.row_start == 0 && pc.col_span == 1 {
                let cell = get_cell(container, pc.loc);
                let col = pc.col_start;
                if let Some(explicit_w) = resolve_dimension(cell.style.width, target_inner_w) {
                    fixed_col_w[col] = explicit_w;
                    assigned[col] = true;
                    assigned_sum += explicit_w;
                }
            }
        }

        for c in 0..num_cols {
            if !assigned[c] {
                unassigned_count += 1;
            }
        }

        if unassigned_count > 0 {
            let remaining_w = (target_inner_w - assigned_sum).max(0.0);
            let per_col = remaining_w / unassigned_count as f32;
            for c in 0..num_cols {
                if !assigned[c] {
                    fixed_col_w[c] = per_col;
                }
            }
        } else if assigned_sum > 0.0 && assigned_sum < target_inner_w {
            let extra = target_inner_w - assigned_sum;
            for c in 0..num_cols {
                fixed_col_w[c] += extra * (fixed_col_w[c] / assigned_sum);
            }
        } else if target_inner_w > 0.0 && assigned_sum == 0.0 {
            let equal_w = target_inner_w / num_cols as f32;
            for c in 0..num_cols {
                fixed_col_w[c] = equal_w;
            }
        }

        fixed_col_w
    } else {
        // W3C CSS 2.1 §17.5.2 Auto Table Layout Pass
        let mut col_min = vec![0.0f32; num_cols];
        let mut col_max = vec![0.0f32; num_cols];

        for pc in &mut placed_cells {
            let cell = get_cell_mut(container, pc.loc);
            let min_max = compute_min_max_sizes(cell, None);
            let cell_padding = cell.style.padding;
            let cell_border = cell.style.border_width;
            let h_extra =
                cell_padding.left + cell_padding.right + cell_border.left + cell_border.right;

            let mut min_w = min_max.min_content + h_extra;
            let mut max_w = min_max.max_content + h_extra;

            if let Some(explicit_w) = resolve_dimension(cell.style.width, available_width) {
                min_w = min_w.max(explicit_w);
                max_w = max_w.max(explicit_w);
            }

            pc.min_w = min_w;
            pc.max_w = max_w;
        }

        // Step 3a: Non-spanning cells (colspan == 1)
        for pc in &placed_cells {
            if pc.col_span == 1 {
                let col = pc.col_start;
                col_min[col] = col_min[col].max(pc.min_w);
                col_max[col] = col_max[col].max(pc.max_w);
            }
        }

        // Step 3b: Spanning cells (colspan > 1) - distribute excess
        for pc in &placed_cells {
            if pc.col_span > 1 {
                let span_cols = pc.col_start..pc.col_start + pc.col_span;
                let current_span_min: f32 = span_cols.clone().map(|c| col_min[c]).sum();
                if pc.min_w > current_span_min {
                    let excess = pc.min_w - current_span_min;
                    let per_col = excess / pc.col_span as f32;
                    for c in span_cols.clone() {
                        col_min[c] += per_col;
                    }
                }

                let current_span_max: f32 = span_cols.clone().map(|c| col_max[c]).sum();
                if pc.max_w > current_span_max {
                    let excess = pc.max_w - current_span_max;
                    let per_col = excess / pc.col_span as f32;
                    for c in span_cols {
                        col_max[c] += per_col;
                    }
                }
            }
        }

        for c in 0..num_cols {
            col_max[c] = col_max[c].max(col_min[c]);
        }

        let target_inner_w = match resolve_dimension(container.style.width, available_width) {
            Some(w) => {
                let clamped = clamp_dimension(
                    w,
                    container.style.min_width,
                    container.style.max_width,
                    available_width,
                );
                (clamped - table_h_extras).max(0.0)
            }
            None => {
                let avail_inner =
                    (available_width - margin.left - margin.right - table_h_extras).max(0.0);
                let sum_max: f32 = col_max.iter().sum();
                if sum_max <= avail_inner {
                    sum_max
                } else {
                    avail_inner
                }
            }
        };

        let sum_col_min: f32 = col_min.iter().sum();
        let sum_col_max: f32 = col_max.iter().sum();
        let mut auto_col_w = vec![0.0f32; num_cols];

        if target_inner_w <= sum_col_min || sum_col_min == 0.0 {
            if sum_col_min > 0.0 {
                for c in 0..num_cols {
                    auto_col_w[c] = col_min[c] * (target_inner_w / sum_col_min);
                }
            } else {
                let equal_w = target_inner_w / num_cols as f32;
                for c in 0..num_cols {
                    auto_col_w[c] = equal_w;
                }
            }
        } else if target_inner_w >= sum_col_max {
            let extra = target_inner_w - sum_col_max;
            if sum_col_max > 0.0 {
                for c in 0..num_cols {
                    auto_col_w[c] = col_max[c] + extra * (col_max[c] / sum_col_max);
                }
            } else {
                let equal_w = target_inner_w / num_cols as f32;
                for c in 0..num_cols {
                    auto_col_w[c] = equal_w;
                }
            }
        } else {
            let factor = (target_inner_w - sum_col_min) / (sum_col_max - sum_col_min);
            for c in 0..num_cols {
                auto_col_w[c] = col_min[c] + factor * (col_max[c] - col_min[c]);
            }
        }

        auto_col_w
    };

    // 4. Measure Row Heights with Computed Column Widths
    let mut col_x = vec![0.0f32; num_cols];
    let mut current_col_x = border.left + padding.left;
    for c in 0..num_cols {
        col_x[c] = current_col_x;
        current_col_x += col_w[c] + h_spacing;
    }

    let mut row_h = vec![0.0f32; num_rows];
    let mut row_content_h = vec![0.0f32; num_rows];

    // Measure unspanned cells first
    for pc in &mut placed_cells {
        if pc.row_span == 1 {
            let span_cols = pc.col_start..pc.col_start + pc.col_span;
            let cell_width: f32 =
                span_cols.map(|c| col_w[c]).sum::<f32>() + (pc.col_span - 1) as f32 * h_spacing;

            let cell = get_cell_mut(container, pc.loc);
            let cg = child_guard.unwrap_or(guard);
            cell.compute_layout_guarded(cell_width, available_height, None, 0.0, cg);
            let measured_h = cell.geometry.border_box.height;
            if collapsed {
                let border = cell.style.border_width;
                let content_h = (measured_h - border.top - border.bottom).max(0.0);
                row_content_h[pc.row_start] = row_content_h[pc.row_start].max(content_h);
            } else {
                row_h[pc.row_start] = row_h[pc.row_start].max(measured_h);
            }

            // Compute baseline per W3C CSS 2.1 §17.5.3
            pc.baseline = if !cell.line_boxes.is_empty() {
                (cell.line_boxes[0].bounds.y - cell.geometry.border_box.y)
                    + cell.line_boxes[0].baseline
            } else {
                measured_h - cell.style.padding.bottom - cell.style.border_width.bottom
            };
        }
    }

    if collapsed {
        // Split each resolved boundary evenly between the two rows it joins, so
        // the shared border contributes its width once to the row pitch.
        for r in 0..num_rows {
            row_h[r] = row_content_h[r] + 0.5 * (row_boundary[r] + row_boundary[r + 1]);
        }
    }

    // Measure spanned cells and distribute excess height
    for pc in &mut placed_cells {
        if pc.row_span > 1 {
            let span_cols = pc.col_start..pc.col_start + pc.col_span;
            let cell_width: f32 =
                span_cols.map(|c| col_w[c]).sum::<f32>() + (pc.col_span - 1) as f32 * h_spacing;

            let cell = get_cell_mut(container, pc.loc);
            let cg = child_guard.unwrap_or(guard);
            cell.compute_layout_guarded(cell_width, available_height, None, 0.0, cg);
            let measured_h = cell.geometry.border_box.height;

            pc.baseline = if !cell.line_boxes.is_empty() {
                (cell.line_boxes[0].bounds.y - cell.geometry.border_box.y)
                    + cell.line_boxes[0].baseline
            } else {
                measured_h - cell.style.padding.bottom - cell.style.border_width.bottom
            };

            let required_h = if collapsed {
                let border = cell.style.border_width;
                let end_row = pc.row_start + pc.row_span;
                (measured_h - border.top - border.bottom).max(0.0)
                    + 0.5 * (row_boundary[pc.row_start] + row_boundary[end_row])
            } else {
                measured_h
            };

            let span_rows = pc.row_start..pc.row_start + pc.row_span;
            let current_span_h: f32 = span_rows.clone().map(|r| row_h[r]).sum::<f32>()
                + (pc.row_span - 1) as f32 * v_spacing;
            if required_h > current_span_h {
                let excess = required_h - current_span_h;
                let per_row = excess / pc.row_span as f32;
                for r in span_rows {
                    row_h[r] += per_row;
                }
            }
        }
    }

    // Baseline alignment pass (§17.5.3): calculate row maximum baseline distance
    let mut row_max_baseline = vec![0.0f32; num_rows];
    for pc in &placed_cells {
        let cell = get_cell(container, pc.loc);
        if matches!(cell.style.vertical_align, VerticalAlign::Baseline) {
            row_max_baseline[pc.row_start] = row_max_baseline[pc.row_start].max(pc.baseline);
        }
    }

    // Expand row heights if baseline shifting increases the required height
    for pc in &placed_cells {
        let cell = get_cell(container, pc.loc);
        if matches!(cell.style.vertical_align, VerticalAlign::Baseline) && pc.row_span == 1 {
            let shift = row_max_baseline[pc.row_start] - pc.baseline;
            let required_h = if collapsed {
                let border = cell.style.border_width;
                (cell.geometry.border_box.height - border.top - border.bottom).max(0.0)
                    + 0.5 * (row_boundary[pc.row_start] + row_boundary[pc.row_start + 1])
            } else {
                cell.geometry.border_box.height
            };
            row_h[pc.row_start] = row_h[pc.row_start].max(shift + required_h);
        }
    }

    // 5. Position Rows, Captions, and Cells
    let mut row_y = vec![0.0f32; num_rows];
    let mut current_rel_y = 0.0f32;
    for r in 0..num_rows {
        row_y[r] = current_rel_y;
        current_rel_y += row_h[r] + v_spacing;
    }

    let table_origin_x = container.geometry.border_box.x;
    let table_origin_y = container.geometry.border_box.y;
    let table_inner_w = col_w.iter().sum::<f32>() + total_h_spacing;
    let final_table_w = table_inner_w + padding.left + padding.right + border.left + border.right;

    let mut current_offset_y = border.top + padding.top;

    // Layout top captions (§17.4)
    for &cap_idx in &top_captions {
        let cap = &mut container.children[cap_idx];
        let cg = child_guard.unwrap_or(guard);
        cap.compute_layout_guarded(final_table_w, available_height, None, 0.0, cg);
        let cap_h = cap.geometry.border_box.height;
        let cap_m = cap.style.margin;
        let cap_x = table_origin_x + border.left + padding.left + cap_m.left;
        let cap_y = table_origin_y + current_offset_y + cap_m.top;
        let dx = cap_x - cap.geometry.border_box.x;
        let dy = cap_y - cap.geometry.border_box.y;
        cap.translate(dx, dy);
        cap.geometry.border_box.x = cap_x;
        cap.geometry.border_box.y = cap_y;
        current_offset_y += cap_m.top + cap_h + cap_m.bottom;
    }

    let grid_origin_y = current_offset_y;

    // Position each cell
    for pc in &placed_cells {
        let span_cols = pc.col_start..pc.col_start + pc.col_span;
        let cell_w: f32 =
            span_cols.map(|c| col_w[c]).sum::<f32>() + (pc.col_span - 1) as f32 * h_spacing;

        let span_rows = pc.row_start..pc.row_start + pc.row_span;
        let cell_h: f32 =
            span_rows.map(|r| row_h[r]).sum::<f32>() + (pc.row_span - 1) as f32 * v_spacing;

        let target_x = table_origin_x + col_x[pc.col_start];
        let target_y = table_origin_y + grid_origin_y + row_y[pc.row_start];

        let cell = get_cell_mut(container, pc.loc);
        let old_x = cell.geometry.border_box.x;
        let old_y = cell.geometry.border_box.y;
        let dx = target_x - old_x;
        let dy = target_y - old_y;
        cell.translate(dx, dy);

        let cell_padding = cell.style.padding;
        let cell_border = cell.style.border_width;
        let cell_margin = cell.style.margin;

        let measured_content_h = cell.geometry.content_box.height;
        cell.geometry = LayoutGeometry::from_border_box(
            Rect::new(target_x, target_y, cell_w, cell_h),
            cell_padding,
            cell_border,
            cell_margin,
        );

        // Apply cell vertical alignment (§17.5.3)
        let inner_avail_h = cell.geometry.content_box.height;
        let shift_y = match cell.style.vertical_align {
            VerticalAlign::Baseline => (row_max_baseline[pc.row_start] - pc.baseline).max(0.0),
            VerticalAlign::Middle => ((inner_avail_h - measured_content_h) / 2.0).max(0.0),
            VerticalAlign::Bottom => (inner_avail_h - measured_content_h).max(0.0),
            _ => 0.0,
        };

        if shift_y > 0.0 {
            for line in &mut cell.line_boxes {
                line.bounds.y += shift_y;
                for frag in &mut line.text_fragments {
                    frag.bounds.y += shift_y;
                    frag.baseline_y += shift_y;
                }
                for atomic in &mut line.atomic_fragments {
                    atomic.bounds.y += shift_y;
                }
            }
            for child in &mut cell.children {
                child.translate(0.0, shift_y);
            }
        }
    }

    // Set row container geometries
    for (r_idx, row_loc) in rows.iter().enumerate() {
        let r_y = table_origin_y + grid_origin_y + row_y[r_idx];
        let r_h = row_h[r_idx];
        let row = get_row_mut(container, *row_loc);
        row.geometry = LayoutGeometry::from_border_box(
            Rect::new(
                table_origin_x + border.left + padding.left,
                r_y,
                table_inner_w,
                r_h,
            ),
            row.style.padding,
            row.style.border_width,
            row.style.margin,
        );
    }

    // Set row group geometries
    for child in &mut container.children {
        if matches!(
            child.style.display,
            Display::TableRowGroup | Display::TableHeaderGroup | Display::TableFooterGroup
        ) {
            let mut min_y = f32::MAX;
            let mut max_y = f32::MIN;
            for row in &child.children {
                if row.geometry.border_box.height > 0.0 {
                    min_y = min_y.min(row.geometry.border_box.y);
                    max_y = max_y.max(row.geometry.border_box.y + row.geometry.border_box.height);
                }
            }
            let group_y = if min_y < f32::MAX {
                min_y
            } else {
                table_origin_y + grid_origin_y
            };
            let group_h = if max_y > f32::MIN {
                (max_y - min_y).max(0.0)
            } else {
                0.0
            };
            child.geometry = LayoutGeometry::from_border_box(
                Rect::new(
                    table_origin_x + border.left + padding.left,
                    group_y,
                    table_inner_w,
                    group_h,
                ),
                child.style.padding,
                child.style.border_width,
                child.style.margin,
            );
        }
    }

    let total_v_spacing = (num_rows.saturating_sub(1)) as f32 * v_spacing;
    let grid_h = row_h.iter().sum::<f32>() + total_v_spacing;
    current_offset_y = grid_origin_y + grid_h;

    // Layout bottom captions (§17.4)
    for &cap_idx in &bottom_captions {
        let cap = &mut container.children[cap_idx];
        let cg = child_guard.unwrap_or(guard);
        cap.compute_layout_guarded(final_table_w, available_height, None, 0.0, cg);
        let cap_h = cap.geometry.border_box.height;
        let cap_m = cap.style.margin;
        let cap_x = table_origin_x + border.left + padding.left + cap_m.left;
        let cap_y = table_origin_y + current_offset_y + cap_m.top;
        let dx = cap_x - cap.geometry.border_box.x;
        let dy = cap_y - cap.geometry.border_box.y;
        cap.translate(dx, dy);
        cap.geometry.border_box.x = cap_x;
        cap.geometry.border_box.y = cap_y;
        current_offset_y += cap_m.top + cap_h + cap_m.bottom;
    }

    let final_table_h = current_offset_y + padding.bottom + border.bottom;

    if collapsed {
        if let Some(mut grid) = table_grid {
            grid.row_positions = vec![0.0f32; num_rows + 1];
            for r in 0..num_rows {
                grid.row_positions[r] = table_origin_y + grid_origin_y + row_y[r];
            }
            grid.row_positions[num_rows] =
                table_origin_y + grid_origin_y + row_y[num_rows - 1] + row_h[num_rows - 1];

            grid.col_positions = vec![0.0f32; num_cols + 1];
            for c in 0..num_cols {
                grid.col_positions[c] = table_origin_x + col_x[c];
            }
            grid.col_positions[num_cols] =
                table_origin_x + col_x[num_cols - 1] + col_w[num_cols - 1];

            container.table_grid = Some(grid);
        }
    } else {
        container.table_grid = None;
    }

    container.geometry = LayoutGeometry::from_border_box(
        Rect::new(table_origin_x, table_origin_y, final_table_w, final_table_h),
        padding,
        border,
        margin,
    );
}

#[inline(always)]
fn get_row(container: &LayoutNode, loc: RowLocation) -> &LayoutNode {
    match loc.group_idx {
        Some(g) => &container.children[g].children[loc.row_idx],
        None => &container.children[loc.row_idx],
    }
}

#[inline(always)]
fn get_row_mut(container: &mut LayoutNode, loc: RowLocation) -> &mut LayoutNode {
    match loc.group_idx {
        Some(g) => &mut container.children[g].children[loc.row_idx],
        None => &mut container.children[loc.row_idx],
    }
}

#[inline(always)]
fn get_cell(container: &LayoutNode, loc: CellLocation) -> &LayoutNode {
    match loc.group_idx {
        Some(g) => &container.children[g].children[loc.row_idx].children[loc.cell_idx],
        None => &container.children[loc.row_idx].children[loc.cell_idx],
    }
}

#[inline(always)]
fn get_cell_mut(container: &mut LayoutNode, loc: CellLocation) -> &mut LayoutNode {
    match loc.group_idx {
        Some(g) => &mut container.children[g].children[loc.row_idx].children[loc.cell_idx],
        None => &mut container.children[loc.row_idx].children[loc.cell_idx],
    }
}
