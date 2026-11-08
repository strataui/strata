//! W3C CSS Grid Layout Level 2 Solver.

use crate::box_model::{clamp_dimension, resolve_dimension};
use crate::geometry::{LayoutGeometry, Rect};
use crate::tree::LayoutNode;
use strata_style::types::{Display, GridTrack, GridTrackBreadth, Position};

/// Solves W3C CSS Grid Layout Level 2 for a container and its items.
pub fn solve_grid(container: &mut LayoutNode, available_width: f32, available_height: f32) {
    solve_grid_guarded(
        container,
        available_width,
        available_height,
        crate::tree::LayoutDepthGuard::root(),
    );
}

/// Solves W3C CSS Grid Layout Level 2 with an active depth guard to prevent stack overflow.
pub fn solve_grid_guarded(
    container: &mut LayoutNode,
    available_width: f32,
    available_height: f32,
    guard: crate::tree::LayoutDepthGuard,
) {
    solve_grid_with_subgrid_guarded(
        container,
        available_width,
        available_height,
        None,
        None,
        guard,
    );
}

/// Solves W3C CSS Grid Layout Level 2 with subgrid track inheritance and active depth guard.
pub fn solve_grid_with_subgrid_guarded(
    container: &mut LayoutNode,
    available_width: f32,
    available_height: f32,
    inherited_cols: Option<&[f32]>,
    inherited_rows: Option<&[f32]>,
    guard: crate::tree::LayoutDepthGuard,
) {
    let _prof = crate::prof::Scope::new(crate::prof::Slot::Grid);
    let padding = container.style.padding;
    let border = container.style.border_width;
    let margin = container.style.margin;

    // 1. Resolve container dimensions
    let resolved_width = match resolve_dimension(container.style.width, available_width) {
        Some(w) => clamp_dimension(
            w,
            container.style.min_width,
            container.style.max_width,
            available_width,
        ),
        None => {
            let auto_w = (available_width - margin.left - margin.right).max(0.0);
            clamp_dimension(
                auto_w,
                container.style.min_width,
                container.style.max_width,
                available_width,
            )
        }
    };

    let inner_width =
        (resolved_width - padding.left - padding.right - border.left - border.right).max(0.0);
    let resolved_height = resolve_dimension(container.style.height, available_height).map(|h| {
        clamp_dimension(
            h,
            container.style.min_height,
            container.style.max_height,
            available_height,
        )
    });
    let inner_height = resolved_height
        .map(|h| (h - padding.top - padding.bottom - border.top - border.bottom).max(0.0));

    let gap_col = container.style.gap_col.max(container.style.gap);
    let gap_row = container.style.gap_row.max(container.style.gap);

    // 2. Pre-calculate Column Bounds and Grid Dimensions (supporting implicit tracks)
    let is_subgrid_cols = container.style.grid_template_columns.first() == Some(&GridTrack::Subgrid);
    let explicit_cols = if is_subgrid_cols {
        inherited_cols.map_or(1, |c| c.len().max(1))
    } else {
        container.style.grid_template_columns.len().max(1)
    };
    let mut num_cols = explicit_cols;

    for child in &container.children {
        if child.style.position == Position::Absolute
            || child.style.position == Position::Fixed
            || child.style.display == Display::None
        {
            continue;
        }
        let span = child.style.grid_column_span.unwrap_or(1).max(1) as usize;
        if let Some(line) = child.style.grid_column_start {
            if line >= 1 {
                let start = (line - 1) as usize;
                num_cols = num_cols.max(start + span);
            }
        }
        num_cols = num_cols.max(span);
    }

    // 3. Grid Item Multi-Pass Placement (W3C CSS Grid Layout Level 2 §8.5)
    #[derive(Clone, Copy)]
    struct PlacedItem {
        child_index: usize,
        col_start: usize,
        col_span: usize,
        row_start: usize,
        row_span: usize,
    }

    let mut placed: Vec<Option<PlacedItem>> = vec![None; container.children.len()];
    let mut grid_occupancy: Vec<Vec<bool>> = Vec::new(); // grid_occupancy[row][col]

    let is_dense = container.style.grid_auto_flow.is_dense();
    let is_col_flow = !container.style.grid_auto_flow.is_row();

    // Step 1: Position items with definite row and definite column
    for (child_idx, child) in container.children.iter().enumerate() {
        if child.style.position == Position::Absolute
            || child.style.position == Position::Fixed
            || child.style.display == Display::None
        {
            continue;
        }
        if let (Some(r_line), Some(c_line)) = (child.style.grid_row_start, child.style.grid_column_start) {
            let r = (r_line.max(1) - 1) as usize;
            let c = (c_line.max(1) - 1) as usize;
            let col_span = child.style.grid_column_span.unwrap_or(1).max(1) as usize;
            let row_span = child.style.grid_row_span.unwrap_or(1) as usize;

            mark_cell_range_occupied(&mut grid_occupancy, r, c, row_span, col_span);
            placed[child_idx] = Some(PlacedItem {
                child_index: child_idx,
                col_start: c,
                col_span,
                row_start: r,
                row_span,
            });
            num_cols = num_cols.max(c + col_span);
        }
    }

    // Step 2: Position items with definite row, auto column
    for (child_idx, child) in container.children.iter().enumerate() {
        if child.style.position == Position::Absolute
            || child.style.position == Position::Fixed
            || child.style.display == Display::None
        {
            continue;
        }
        if let (Some(r_line), None) = (child.style.grid_row_start, child.style.grid_column_start) {
            let r = (r_line.max(1) - 1) as usize;
            let col_span = child.style.grid_column_span.unwrap_or(1).max(1) as usize;
            let row_span = child.style.grid_row_span.unwrap_or(1) as usize;

            let mut c = 0;
            loop {
                ensure_grid_dimensions(&mut grid_occupancy, r + row_span, c + col_span);
                if is_cell_range_free(&grid_occupancy, r, c, row_span, col_span) {
                    mark_cell_range_occupied(&mut grid_occupancy, r, c, row_span, col_span);
                    placed[child_idx] = Some(PlacedItem {
                        child_index: child_idx,
                        col_start: c,
                        col_span,
                        row_start: r,
                        row_span,
                    });
                    num_cols = num_cols.max(c + col_span);
                    break;
                }
                c += 1;
            }
        }
    }

    // Step 3: Position items with definite column, auto row
    for (child_idx, child) in container.children.iter().enumerate() {
        if child.style.position == Position::Absolute
            || child.style.position == Position::Fixed
            || child.style.display == Display::None
        {
            continue;
        }
        if let (None, Some(c_line)) = (child.style.grid_row_start, child.style.grid_column_start) {
            let c = (c_line.max(1) - 1) as usize;
            let col_span = child.style.grid_column_span.unwrap_or(1).max(1) as usize;
            let row_span = child.style.grid_row_span.unwrap_or(1) as usize;

            let mut r = 0;
            loop {
                ensure_grid_dimensions(&mut grid_occupancy, r + row_span, c + col_span);
                if is_cell_range_free(&grid_occupancy, r, c, row_span, col_span) {
                    mark_cell_range_occupied(&mut grid_occupancy, r, c, row_span, col_span);
                    placed[child_idx] = Some(PlacedItem {
                        child_index: child_idx,
                        col_start: c,
                        col_span,
                        row_start: r,
                        row_span,
                    });
                    num_cols = num_cols.max(c + col_span);
                    break;
                }
                r += 1;
            }
        }
    }

    // Step 4: Position remaining auto items (and advance cursor in DOM order)
    let mut cursor_row = 0;
    let mut cursor_col = 0;

    for (child_idx, child) in container.children.iter().enumerate() {
        if child.style.position == Position::Absolute
            || child.style.position == Position::Fixed
            || child.style.display == Display::None
        {
            continue;
        }

        if let Some(existing) = placed[child_idx] {
            if !is_dense {
                if !is_col_flow {
                    if existing.row_start > cursor_row
                        || (existing.row_start == cursor_row && existing.col_start + existing.col_span > cursor_col)
                    {
                        cursor_row = existing.row_start;
                        cursor_col = existing.col_start + existing.col_span;
                    }
                } else if existing.col_start > cursor_col
                    || (existing.col_start == cursor_col && existing.row_start + existing.row_span > cursor_row)
                {
                    cursor_col = existing.col_start;
                    cursor_row = existing.row_start + existing.row_span;
                }
            }
            continue;
        }

        let col_span = child.style.grid_column_span.unwrap_or(1).max(1) as usize;
        let row_span = child.style.grid_row_span.unwrap_or(1) as usize;

        if !is_col_flow {
            // Row-major auto placement
            let mut r = if is_dense { 0 } else { cursor_row };
            let mut c = if is_dense { 0 } else { cursor_col };

            loop {
                ensure_grid_dimensions(&mut grid_occupancy, r + row_span, num_cols);

                if c + col_span > num_cols {
                    r += 1;
                    c = 0;
                    continue;
                }

                if is_cell_range_free(&grid_occupancy, r, c, row_span, col_span) {
                    mark_cell_range_occupied(&mut grid_occupancy, r, c, row_span, col_span);
                    placed[child_idx] = Some(PlacedItem {
                        child_index: child_idx,
                        col_start: c,
                        col_span,
                        row_start: r,
                        row_span,
                    });
                    if !is_dense {
                        cursor_row = r;
                        cursor_col = c + col_span;
                    }
                    break;
                }

                c += 1;
            }
        } else {
            // Column-major auto placement
            let explicit_rows = container.style.grid_template_rows.len();
            let max_rows = if explicit_rows == 0 {
                grid_occupancy.len().max(1)
            } else {
                explicit_rows
            };

            let mut c = if is_dense { 0 } else { cursor_col };
            let mut r = if is_dense { 0 } else { cursor_row };

            loop {
                ensure_grid_dimensions(&mut grid_occupancy, max_rows.max(r + row_span), c + col_span);

                if r + row_span > max_rows && max_rows > 0 {
                    c += 1;
                    r = 0;
                    continue;
                }

                if is_cell_range_free(&grid_occupancy, r, c, row_span, col_span) {
                    mark_cell_range_occupied(&mut grid_occupancy, r, c, row_span, col_span);
                    placed[child_idx] = Some(PlacedItem {
                        child_index: child_idx,
                        col_start: c,
                        col_span,
                        row_start: r,
                        row_span,
                    });
                    if !is_dense {
                        cursor_col = c;
                        cursor_row = r + row_span;
                    }
                    num_cols = num_cols.max(c + col_span);
                    break;
                }

                r += 1;
            }
        }
    }

    let placed_items: Vec<PlacedItem> = placed.into_iter().flatten().collect();

    let total_cols = num_cols.max(grid_occupancy.first().map_or(1, |row| row.len())).max(1);
    let total_rows = grid_occupancy.len().max(1);

    // 4. Resolve Column Tracks (with Subgrid and implicit track expansion)
    let col_sizes = if let (true, Some(cols)) = (is_subgrid_cols, inherited_cols) {
        cols.to_vec()
    } else {
        let mut col_tracks = if container.style.grid_template_columns.is_empty() {
            vec![GridTrack::Fr(1.0)]
        } else {
            container.style.grid_template_columns.clone()
        };
        while col_tracks.len() < total_cols {
            col_tracks.push(GridTrack::Auto);
        }
        resolve_tracks(&col_tracks, inner_width, gap_col)
    };

    // 5. Resolve Row Tracks (with Subgrid and implicit track expansion)
    let is_subgrid_rows = container.style.grid_template_rows.first() == Some(&GridTrack::Subgrid);
    let row_sizes = if let (true, Some(rows)) = (is_subgrid_rows, inherited_rows) {
        rows.to_vec()
    } else {
        let mut row_tracks = container.style.grid_template_rows.clone();
        while row_tracks.len() < total_rows {
            row_tracks.push(GridTrack::Auto);
        }

        if let Some(h) = inner_height {
            resolve_tracks(&row_tracks, h, gap_row)
        } else {
            // Explicit absolute row tracks keep their authored size even when the
            // container height is indefinite; remaining rows size to their content.
            let mut sizes: Vec<f32> = row_tracks
                .iter()
                .map(|track| match *track {
                    GridTrack::Px(px) => px.max(0.0),
                    _ => 0.0,
                })
                .collect();
            let cg = guard.enter().unwrap_or(guard);
            for item in &placed_items {
                let child = &mut container.children[item.child_index];
                let item_width =
                    calculate_span_size(&col_sizes, item.col_start, item.col_span, gap_col);
                child.compute_layout_guarded(item_width, available_height, None, 0.0, cg);
                let item_h = child.geometry.border_box.height
                    + child.style.margin.top
                    + child.style.margin.bottom;
                let h_per_row = item_h / item.row_span as f32;
                for r in 0..item.row_span {
                    let row = item.row_start + r;
                    if row >= sizes.len() || matches!(row_tracks.get(row), Some(GridTrack::Px(_))) {
                        continue;
                    }
                    sizes[row] = sizes[row].max(h_per_row);
                }
            }
            sizes
        }
    };

    // 5. Finalize container geometry
    let total_rows_h: f32 = row_sizes.iter().sum();
    let total_row_gaps = if total_rows > 1 {
        (total_rows - 1) as f32 * gap_row
    } else {
        0.0
    };
    let content_height = total_rows_h + total_row_gaps;

    let final_height = resolved_height
        .unwrap_or(content_height + padding.top + padding.bottom + border.top + border.bottom);

    let border_box = Rect::new(
        container.geometry.border_box.x,
        container.geometry.border_box.y,
        resolved_width,
        final_height,
    );
    container.geometry = LayoutGeometry::from_border_box(border_box, padding, border, margin);

    // 6. Layout and position placed grid items (recursively solving subgrids)
    let child_guard = guard.enter();
    for item in placed_items {
        let child = &mut container.children[item.child_index];

        let offset_x = calculate_offset(&col_sizes, item.col_start, gap_col);
        let span_width = calculate_span_size(&col_sizes, item.col_start, item.col_span, gap_col);

        let offset_y = calculate_offset(&row_sizes, item.row_start, gap_row);
        let span_height = calculate_span_size(&row_sizes, item.row_start, item.row_span, gap_row);

        let child_margin = child.style.margin;
        let item_w = (span_width - child_margin.left - child_margin.right).max(0.0);
        let item_h = (span_height - child_margin.top - child_margin.bottom).max(0.0);
        let item_x = container.geometry.content_box.x + offset_x + child_margin.left;
        let item_y = container.geometry.content_box.y + offset_y + child_margin.top;

        child.geometry = LayoutGeometry::from_border_box(
            Rect::new(item_x, item_y, item_w, item_h),
            child.style.padding,
            child.style.border_width,
            child.style.margin,
        );

        // Subgrid handling: if child is display: grid with subgrid columns or rows, inherit tracks
        if child.style.display == strata_style::types::Display::Grid {
            let is_child_subgrid_cols =
                child.style.grid_template_columns.first() == Some(&GridTrack::Subgrid);
            let is_child_subgrid_rows =
                child.style.grid_template_rows.first() == Some(&GridTrack::Subgrid);
            if is_child_subgrid_cols || is_child_subgrid_rows {
                let sub_cols = if is_child_subgrid_cols {
                    let end = (item.col_start + item.col_span).min(col_sizes.len());
                    Some(&col_sizes[item.col_start..end])
                } else {
                    None
                };
                let sub_rows = if is_child_subgrid_rows {
                    let end = (item.row_start + item.row_span).min(row_sizes.len());
                    Some(&row_sizes[item.row_start..end])
                } else {
                    None
                };
                if let Some(cg) = child_guard {
                    solve_grid_with_subgrid_guarded(
                        child,
                        item_w,
                        item_h,
                        sub_cols,
                        sub_rows,
                        cg,
                    );
                }
                continue;
            }
        }

        if !child.children.is_empty() {
            if let Some(cg) = child_guard {
                child.layout_children_guarded(cg);
            }
        }
    }

    container.geometry.snap_to_pixel();
}

fn resolve_tracks(tracks: &[GridTrack], available_space: f32, gap: f32) -> Vec<f32> {
    let count = tracks.len();
    if count == 0 {
        return Vec::new();
    }

    let is_tui = strata_primitives::target::is_tui_target();

    let total_gaps = if count > 1 {
        (count - 1) as f32 * gap
    } else {
        0.0
    };
    let mut resolved = vec![0.0f32; count];
    let mut total_fixed = 0.0;
    let mut total_fr = 0.0;

    for (i, track) in tracks.iter().enumerate() {
        match *track {
            GridTrack::Px(px) => {
                let val = if is_tui { px.round() } else { px };
                resolved[i] = val;
                total_fixed += val;
            }
            GridTrack::Percent(pct) => {
                let px = available_space * (pct / 100.0);
                let val = if is_tui { px.round() } else { px };
                resolved[i] = val;
                total_fixed += val;
            }
            GridTrack::Fr(fr) => {
                total_fr += fr;
            }
            GridTrack::Auto
            | GridTrack::MinContent
            | GridTrack::MaxContent
            | GridTrack::Subgrid => {
                total_fr += 1.0;
            }
            GridTrack::MinMax(min_b, max_b) => {
                let min_px = match min_b {
                    GridTrackBreadth::Px(px) => px,
                    GridTrackBreadth::Percent(pct) => available_space * (pct / 100.0),
                    _ => 0.0,
                };
                let min_px = if is_tui { min_px.round() } else { min_px };
                match max_b {
                    GridTrackBreadth::Fr(fr) => {
                        resolved[i] = min_px;
                        total_fixed += min_px;
                        total_fr += fr;
                    }
                    GridTrackBreadth::Px(px) => {
                        let sz = px.max(min_px);
                        let val = if is_tui { sz.round() } else { sz };
                        resolved[i] = val;
                        total_fixed += val;
                    }
                    GridTrackBreadth::Percent(pct) => {
                        let sz = (available_space * (pct / 100.0)).max(min_px);
                        let val = if is_tui { sz.round() } else { sz };
                        resolved[i] = val;
                        total_fixed += val;
                    }
                    GridTrackBreadth::Auto
                    | GridTrackBreadth::MaxContent
                    | GridTrackBreadth::MinContent => {
                        resolved[i] = min_px;
                        total_fixed += min_px;
                        total_fr += 1.0;
                    }
                }
            }
        }
    }

    let remaining_space = (available_space - total_fixed - total_gaps).max(0.0);
    if total_fr > 0.0 {
        let fr_indices: Vec<usize> = tracks
            .iter()
            .enumerate()
            .filter_map(|(i, t)| match *t {
                GridTrack::Fr(_)
                | GridTrack::Auto
                | GridTrack::MinContent
                | GridTrack::MaxContent
                | GridTrack::Subgrid => Some(i),
                GridTrack::MinMax(
                    _,
                    GridTrackBreadth::Fr(_)
                    | GridTrackBreadth::Auto
                    | GridTrackBreadth::MaxContent
                    | GridTrackBreadth::MinContent,
                ) => Some(i),
                _ => None,
            })
            .collect();

        if is_tui {
            let factors: Vec<f32> = fr_indices
                .iter()
                .map(|&i| match tracks[i] {
                    GridTrack::Fr(fr)
                    | GridTrack::MinMax(_, GridTrackBreadth::Fr(fr)) => fr,
                    _ => 1.0,
                })
                .collect();
            let total_avail = remaining_space.round().max(0.0) as u16;
            let quants = strata_primitives::quantization::largest_remainder_quantization(
                &factors,
                total_avail,
            );
            for (idx, &i) in fr_indices.iter().enumerate() {
                resolved[i] += quants[idx] as f32;
            }
        } else {
            let mut allocated = 0.0f32;
            let fr_count = fr_indices.len();
            for (idx, &i) in fr_indices.iter().enumerate() {
                let factor = match tracks[i] {
                    GridTrack::Fr(fr) => fr,
                    GridTrack::Auto
                    | GridTrack::MinContent
                    | GridTrack::MaxContent
                    | GridTrack::Subgrid => 1.0,
                    GridTrack::MinMax(_, max_b) => match max_b {
                        GridTrackBreadth::Fr(fr) => fr,
                        _ => 1.0,
                    },
                    _ => continue,
                };
                if idx == fr_count - 1 {
                    // Chromium FindFrSize residue absorption: guarantee exact space conservation on final flex track
                    resolved[i] += (remaining_space - allocated).max(0.0);
                } else {
                    let size = ((factor / total_fr) * remaining_space * 64.0).round() / 64.0;
                    resolved[i] += size;
                    allocated += size;
                }
            }
        }
    }

    resolved
}

fn calculate_offset(sizes: &[f32], index: usize, gap: f32) -> f32 {
    let mut offset = 0.0;
    for &size in &sizes[..index.min(sizes.len())] {
        offset += size + gap;
    }
    offset
}

fn calculate_span_size(sizes: &[f32], start: usize, span: usize, gap: f32) -> f32 {
    let mut total = 0.0;
    let end = (start + span).min(sizes.len());
    for &size in &sizes[start..end] {
        total += size;
    }
    if span > 1 {
        total += (span - 1) as f32 * gap;
    }
    total
}

fn ensure_grid_dimensions(grid: &mut Vec<Vec<bool>>, rows: usize, cols: usize) {
    let current_cols = grid.first().map_or(cols, |r| r.len()).max(cols);
    if grid.len() < rows {
        grid.resize_with(rows, || vec![false; current_cols]);
    }
    for row in grid.iter_mut() {
        if row.len() < current_cols {
            row.resize(current_cols, false);
        }
    }
}

fn is_cell_range_free(
    grid: &[Vec<bool>],
    r_start: usize,
    c_start: usize,
    r_span: usize,
    c_span: usize,
) -> bool {
    for r in r_start..(r_start + r_span) {
        if r >= grid.len() {
            continue;
        }
        let row = &grid[r];
        for c in c_start..(c_start + c_span) {
            if c < row.len() && row[c] {
                return false;
            }
        }
    }
    true
}

fn mark_cell_range_occupied(
    grid: &mut Vec<Vec<bool>>,
    r_start: usize,
    c_start: usize,
    r_span: usize,
    c_span: usize,
) {
    ensure_grid_dimensions(grid, r_start + r_span, c_start + c_span);
    for row in grid.iter_mut().skip(r_start).take(r_span) {
        for cell in row.iter_mut().skip(c_start).take(c_span) {
            *cell = true;
        }
    }
}

