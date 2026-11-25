//! W3C CSS Grid Layout Level 2 Solver.

use crate::box_model::{
    clamp_dimension, clamp_dimension_with_node, resolve_dimension, resolve_dimension_with_node,
};
use crate::geometry::{LayoutGeometry, Rect};
use crate::tree::LayoutNode;
use strata_style::types::{Display, GridTrack, GridTrackBreadth, Position};

/// Maximum allowable tracks in either dimension per CSS Grid Level 1 specifications
/// (prevents unbounded memory allocation DoS on pathological or adversarial CSS).
pub const MAX_GRID_TRACKS: usize = 1000;

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
    if guard.is_exhausted() {
        crate::tree::record_layout_overflow(guard.current_depth(), guard.max_depth());
        return;
    }
    let padding = container.style.padding;
    let border = container.style.border_width;
    let margin = container.style.margin;

    // 1. Resolve container dimensions
    let resolved_width =
        match resolve_dimension_with_node(container.style.width, available_width, container) {
            Some(w) => clamp_dimension_with_node(
                w,
                container.style.min_width,
                container.style.max_width,
                available_width,
                container,
            ),
            None => {
                let auto_w = (available_width - margin.left - margin.right).max(0.0);
                clamp_dimension_with_node(
                    auto_w,
                    container.style.min_width,
                    container.style.max_width,
                    available_width,
                    container,
                )
            }
        };

    let base_inner_w =
        (resolved_width - padding.left - padding.right - border.left - border.right).max(0.0);
    let inner_width = container
        .transition_content_width()
        .map_or(base_inner_w, |tw| base_inner_w.max(tw));
    let resolved_height = resolve_dimension(container.style.height, available_height).map(|h| {
        clamp_dimension(
            h,
            container.style.min_height,
            container.style.max_height,
            available_height,
        )
    });
    let base_inner_h = resolved_height
        .map(|h| (h - padding.top - padding.bottom - border.top - border.bottom).max(0.0));
    let inner_height = match (base_inner_h, container.transition_content_height()) {
        (Some(h), Some(th)) => Some(h.max(th)),
        (other, _) => other,
    };

    let gap_col = container.style.gap_col.max(container.style.gap);
    let gap_row = container.style.gap_row.max(container.style.gap);

    // 2. Pre-calculate Column Bounds and Grid Dimensions (supporting named areas and implicit tracks)
    let area_rows = container.style.grid_template_areas.len();
    let area_cols = container
        .style
        .grid_template_areas
        .iter()
        .map(|r| r.len())
        .max()
        .unwrap_or(0);

    // Synthesize named grid areas into row/column lines (W3C CSS Grid Level 2 §7.3)
    if !container.style.grid_template_areas.is_empty() {
        for child in &mut container.children {
            if let Some(ref area_name) = child.style.grid_area {
                if child.style.grid_column_start.is_none() && child.style.grid_row_start.is_none() {
                    let mut min_r = None;
                    let mut max_r = None;
                    let mut min_c = None;
                    let mut max_c = None;
                    for (r_idx, row) in container.style.grid_template_areas.iter().enumerate() {
                        for (c_idx, cell) in row.iter().enumerate() {
                            if cell == area_name {
                                min_r = Some(min_r.map_or(r_idx, |m: usize| m.min(r_idx)));
                                max_r = Some(max_r.map_or(r_idx, |m: usize| m.max(r_idx)));
                                min_c = Some(min_c.map_or(c_idx, |m: usize| m.min(c_idx)));
                                max_c = Some(max_c.map_or(c_idx, |m: usize| m.max(c_idx)));
                            }
                        }
                    }
                    if let (Some(r_min), Some(r_max), Some(c_min), Some(c_max)) =
                        (min_r, max_r, min_c, max_c)
                    {
                        child.style.grid_row_start = Some((r_min + 1) as i32);
                        child.style.grid_row_span = Some((r_max - r_min + 1) as u32);
                        child.style.grid_column_start = Some((c_min + 1) as i32);
                        child.style.grid_column_span = Some((c_max - c_min + 1) as u32);
                    }
                }
            }
        }
    }

    let is_subgrid_cols =
        container.style.grid_template_columns.first() == Some(&GridTrack::Subgrid);
    let explicit_cols = if is_subgrid_cols {
        inherited_cols.map_or(1, |c| c.len().max(1))
    } else {
        container
            .style
            .grid_template_columns
            .len()
            .max(area_cols)
            .max(1)
    };
    let explicit_rows = container
        .style
        .grid_template_rows
        .len()
        .max(area_rows)
        .max(1);
    let mut num_cols = explicit_cols;

    for child in &container.children {
        if child.style.position == Position::Absolute
            || child.style.position == Position::Fixed
            || child.style.display == Display::None
        {
            continue;
        }
        let (c_start, c_span) = resolve_axis_placement(
            child.style.grid_column_start,
            child.style.grid_column_end,
            child.style.grid_column_span,
            explicit_cols,
        );
        if let Some(start) = c_start {
            num_cols = num_cols.max(start + c_span).min(MAX_GRID_TRACKS);
        } else {
            num_cols = num_cols.max(c_span).min(MAX_GRID_TRACKS);
        }
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
        let (c_opt, col_span) = resolve_axis_placement(
            child.style.grid_column_start,
            child.style.grid_column_end,
            child.style.grid_column_span,
            explicit_cols,
        );
        let (r_opt, row_span) = resolve_axis_placement(
            child.style.grid_row_start,
            child.style.grid_row_end,
            child.style.grid_row_span,
            explicit_rows,
        );
        if let (Some(c), Some(r)) = (c_opt, r_opt) {
            mark_cell_range_occupied(&mut grid_occupancy, r, c, row_span, col_span);
            placed[child_idx] = Some(PlacedItem {
                child_index: child_idx,
                col_start: c,
                col_span,
                row_start: r,
                row_span,
            });
            num_cols = num_cols.max(c + col_span).min(MAX_GRID_TRACKS);
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
        let (c_opt, col_span) = resolve_axis_placement(
            child.style.grid_column_start,
            child.style.grid_column_end,
            child.style.grid_column_span,
            explicit_cols,
        );
        let (r_opt, row_span) = resolve_axis_placement(
            child.style.grid_row_start,
            child.style.grid_row_end,
            child.style.grid_row_span,
            explicit_rows,
        );
        if let (None, Some(r)) = (c_opt, r_opt) {
            let mut c = 0;
            loop {
                if c + col_span > MAX_GRID_TRACKS {
                    c = MAX_GRID_TRACKS.saturating_sub(col_span);
                    mark_cell_range_occupied(&mut grid_occupancy, r, c, row_span, col_span);
                    placed[child_idx] = Some(PlacedItem {
                        child_index: child_idx,
                        col_start: c,
                        col_span,
                        row_start: r,
                        row_span,
                    });
                    num_cols = num_cols.max(c + col_span).min(MAX_GRID_TRACKS);
                    break;
                }
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
                    num_cols = num_cols.max(c + col_span).min(MAX_GRID_TRACKS);
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
        let (c_opt, col_span) = resolve_axis_placement(
            child.style.grid_column_start,
            child.style.grid_column_end,
            child.style.grid_column_span,
            explicit_cols,
        );
        let (r_opt, row_span) = resolve_axis_placement(
            child.style.grid_row_start,
            child.style.grid_row_end,
            child.style.grid_row_span,
            explicit_rows,
        );
        if let (Some(c), None) = (c_opt, r_opt) {
            let mut r = 0;
            loop {
                if r + row_span > MAX_GRID_TRACKS {
                    r = MAX_GRID_TRACKS.saturating_sub(row_span);
                    mark_cell_range_occupied(&mut grid_occupancy, r, c, row_span, col_span);
                    placed[child_idx] = Some(PlacedItem {
                        child_index: child_idx,
                        col_start: c,
                        col_span,
                        row_start: r,
                        row_span,
                    });
                    num_cols = num_cols.max(c + col_span).min(MAX_GRID_TRACKS);
                    break;
                }
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
                    num_cols = num_cols.max(c + col_span).min(MAX_GRID_TRACKS);
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
                        || (existing.row_start == cursor_row
                            && existing.col_start + existing.col_span > cursor_col)
                    {
                        cursor_row = existing.row_start;
                        cursor_col = existing.col_start + existing.col_span;
                    }
                } else if existing.col_start > cursor_col
                    || (existing.col_start == cursor_col
                        && existing.row_start + existing.row_span > cursor_row)
                {
                    cursor_col = existing.col_start;
                    cursor_row = existing.row_start + existing.row_span;
                }
            }
            continue;
        }

        let (_, col_span) = resolve_axis_placement(
            child.style.grid_column_start,
            child.style.grid_column_end,
            child.style.grid_column_span,
            explicit_cols,
        );
        let (_, row_span) = resolve_axis_placement(
            child.style.grid_row_start,
            child.style.grid_row_end,
            child.style.grid_row_span,
            explicit_rows,
        );

        if !is_col_flow {
            // Row-major auto placement
            let mut r = if is_dense { 0 } else { cursor_row };
            let mut c = if is_dense { 0 } else { cursor_col };

            loop {
                if r + row_span > MAX_GRID_TRACKS {
                    break;
                }
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
                if c + col_span > MAX_GRID_TRACKS {
                    break;
                }
                ensure_grid_dimensions(
                    &mut grid_occupancy,
                    max_rows.max(r + row_span),
                    c + col_span,
                );

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
                    num_cols = num_cols.max(c + col_span).min(MAX_GRID_TRACKS);
                    break;
                }

                r += 1;
            }
        }
    }

    let placed_items: Vec<PlacedItem> = placed.into_iter().flatten().collect();

    let total_cols = num_cols
        .max(grid_occupancy.first().map_or(1, |row| row.len()))
        .clamp(1, MAX_GRID_TRACKS);
    let total_rows = grid_occupancy.len().clamp(1, MAX_GRID_TRACKS);

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
        let has_col_fr = col_tracks.iter().any(|t| {
            matches!(
                t,
                GridTrack::Fr(_) | GridTrack::MinMax(_, GridTrackBreadth::Fr(_))
            )
        });
        let needs_col_intrinsic = col_tracks.iter().any(|t| {
            matches!(
                t,
                GridTrack::Auto
                    | GridTrack::MinContent
                    | GridTrack::MaxContent
                    | GridTrack::MinMax(
                        _,
                        GridTrackBreadth::Auto
                            | GridTrackBreadth::MinContent
                            | GridTrackBreadth::MaxContent
                    )
            )
        });
        if needs_col_intrinsic {
            let mut intrinsic_cols = vec![0.0f32; col_tracks.len()];
            for item in &placed_items {
                let child = &container.children[item.child_index];
                let mut track_spans_intrinsic = false;
                for c in 0..item.col_span {
                    let col = item.col_start + c;
                    if let Some(t) = col_tracks.get(col) {
                        if matches!(
                            t,
                            GridTrack::Auto
                                | GridTrack::MinContent
                                | GridTrack::MaxContent
                                | GridTrack::MinMax(
                                    _,
                                    GridTrackBreadth::Auto
                                        | GridTrackBreadth::MinContent
                                        | GridTrackBreadth::MaxContent
                                )
                        ) {
                            track_spans_intrinsic = true;
                            break;
                        }
                    }
                }
                if !track_spans_intrinsic {
                    continue;
                }
                let sizes = crate::intrinsic::compute_min_max_sizes(child, Some(inner_width));
                let margin_h = child.style.margin.left + child.style.margin.right;
                for c in 0..item.col_span {
                    let col = item.col_start + c;
                    let Some(track) = col_tracks.get(col) else {
                        continue;
                    };
                    let contrib = match track {
                        GridTrack::MinContent
                        | GridTrack::MinMax(_, GridTrackBreadth::MinContent) => {
                            sizes.min_content + margin_h
                        }
                        GridTrack::MaxContent
                        | GridTrack::MinMax(_, GridTrackBreadth::MaxContent) => {
                            sizes.max_content + margin_h
                        }
                        GridTrack::Auto | GridTrack::MinMax(_, GridTrackBreadth::Auto) => {
                            if has_col_fr {
                                (sizes.max_content + margin_h).min(inner_width)
                            } else {
                                (sizes.min_content + margin_h).min(inner_width)
                            }
                        }
                        _ => continue,
                    };
                    intrinsic_cols[col] = intrinsic_cols[col].max(contrib / item.col_span as f32);
                }
            }
            resolve_tracks(&col_tracks, inner_width, gap_col, Some(&intrinsic_cols), true)
        } else {
            resolve_tracks(&col_tracks, inner_width, gap_col, None, true)
        }
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

        let needs_intrinsic_rows = inner_height.is_none()
            || row_tracks.iter().any(|t| {
                matches!(
                    t,
                    GridTrack::Auto
                        | GridTrack::MinContent
                        | GridTrack::MaxContent
                        | GridTrack::MinMax(
                            _,
                            GridTrackBreadth::Auto
                                | GridTrackBreadth::MinContent
                                | GridTrackBreadth::MaxContent
                        )
                )
            });

        let mut intrinsic_rows = vec![0.0f32; row_tracks.len()];
        if needs_intrinsic_rows {
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
                    if row >= intrinsic_rows.len()
                        || matches!(
                            row_tracks.get(row),
                            Some(GridTrack::Px(_) | GridTrack::Percent(_))
                        )
                    {
                        continue;
                    }
                    intrinsic_rows[row] = intrinsic_rows[row].max(h_per_row);
                }
            }
        }

        if let Some(h) = inner_height {
            let stretch_auto_rows =
                container.style.overflow_y == strata_style::types::Overflow::Visible;
            resolve_tracks(
                &row_tracks,
                h,
                gap_row,
                Some(&intrinsic_rows),
                stretch_auto_rows,
            )
        } else {
            row_tracks
                .iter()
                .enumerate()
                .map(|(r, track)| match *track {
                    GridTrack::Px(px) => px.max(0.0),
                    _ => intrinsic_rows.get(r).copied().unwrap_or(0.0),
                })
                .collect()
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

    let natural_height =
        content_height + padding.top + padding.bottom + border.top + border.bottom;
    let final_height = resolved_height.unwrap_or_else(|| {
        clamp_dimension_with_node(
            natural_height,
            container.style.min_height,
            container.style.max_height,
            available_height,
            container,
        )
    });

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

        // Subgrid handling: if child is display: grid with subgrid columns or rows, inherit tracks
        if child.style.display == strata_style::types::Display::Grid {
            let is_child_subgrid_cols =
                child.style.grid_template_columns.first() == Some(&GridTrack::Subgrid);
            let is_child_subgrid_rows =
                child.style.grid_template_rows.first() == Some(&GridTrack::Subgrid);
            if is_child_subgrid_cols || is_child_subgrid_rows {
                child.geometry = LayoutGeometry::from_border_box(
                    Rect::new(item_x, item_y, item_w, item_h),
                    child.style.padding,
                    child.style.border_width,
                    child.style.margin,
                );
                child.applied_relative_offset = (0.0, 0.0);
                child.geometry.snap_to_pixel();
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
                    solve_grid_with_subgrid_guarded(child, item_w, item_h, sub_cols, sub_rows, cg);
                }
                continue;
            }
        }

        if let Some(cg) = child_guard {
            child.geometry.border_box.x = 0.0;
            child.geometry.border_box.y = 0.0;
            child.applied_relative_offset = (0.0, 0.0);
            child.compute_layout_guarded(span_width, span_height, None, 0.0, cg);

            let stretch_w = if child.style.width == strata_style::types::Dimension::Auto
                && (child.geometry.border_box.width > item_w + 0.5
                    || (matches!(
                        child.style.display,
                        Display::Block | Display::Flex | Display::Grid | Display::Table
                    ) && child.geometry.border_box.width < item_w))
            {
                clamp_dimension_with_node(
                    item_w,
                    child.style.min_width,
                    child.style.max_width,
                    span_width,
                    child,
                )
            } else {
                child.geometry.border_box.width
            };

            let can_stretch_h = child.style.height == strata_style::types::Dimension::Auto
                && !matches!(
                    child.style.align_self,
                    strata_style::types::AlignSelf::FlexStart
                        | strata_style::types::AlignSelf::FlexEnd
                        | strata_style::types::AlignSelf::Center
                        | strata_style::types::AlignSelf::Baseline
                );
            let stretch_h = if can_stretch_h && child.geometry.border_box.height < item_h {
                clamp_dimension_with_node(
                    item_h,
                    child.style.min_height,
                    child.style.max_height,
                    span_height,
                    child,
                )
            } else {
                child.geometry.border_box.height
            };

            if (stretch_w - child.geometry.border_box.width).abs() > 0.01
                || (stretch_h - child.geometry.border_box.height).abs() > 0.01
            {
                child.geometry = LayoutGeometry::from_border_box(
                    Rect::new(
                        child.geometry.border_box.x,
                        child.geometry.border_box.y,
                        stretch_w,
                        stretch_h,
                    ),
                    child.style.padding,
                    child.style.border_width,
                    child.style.margin,
                );
                child.applied_relative_offset = (0.0, 0.0);
                if !child.children.is_empty() {
                    child.layout_children_guarded(cg);
                } else {
                    child.update_scroll_metrics();
                }
            }

            child.translate(
                item_x - child.geometry.border_box.x,
                item_y - child.geometry.border_box.y,
            );
            child.geometry.snap_to_pixel();
        }
    }

    container.geometry.snap_to_pixel();
}

#[inline(always)]
fn snap_subpixel_grid(v: f32) -> f32 {
    (v * 64.0).round() / 64.0
}

fn resolve_tracks(
    tracks: &[GridTrack],
    available_space: f32,
    gap: f32,
    intrinsic_sizes: Option<&[f32]>,
    stretch_auto: bool,
) -> Vec<f32> {
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
    let has_fr = tracks.iter().any(|t| {
        matches!(
            t,
            GridTrack::Fr(_) | GridTrack::MinMax(_, GridTrackBreadth::Fr(_))
        )
    });
    let should_stretch_auto = stretch_auto && !has_fr;

    let mut resolved = vec![0.0f32; count];
    let mut total_fixed = 0.0;
    let mut total_fr = 0.0;

    for (i, track) in tracks.iter().enumerate() {
        let intrinsic = intrinsic_sizes
            .and_then(|s| s.get(i).copied())
            .unwrap_or(0.0)
            .max(0.0);
        match *track {
            GridTrack::Px(px) => {
                let val = if is_tui {
                    px.round()
                } else {
                    snap_subpixel_grid(px)
                };
                resolved[i] = val;
                total_fixed += val;
            }
            GridTrack::Percent(pct) => {
                let px = available_space * (pct / 100.0);
                let val = if is_tui {
                    px.round()
                } else {
                    snap_subpixel_grid(px)
                };
                resolved[i] = val;
                total_fixed += val;
            }
            GridTrack::Fr(fr) => {
                total_fr += fr;
            }
            GridTrack::Auto | GridTrack::Subgrid => {
                let val = if is_tui {
                    intrinsic.round()
                } else {
                    snap_subpixel_grid(intrinsic)
                };
                resolved[i] = val;
                total_fixed += val;
                if should_stretch_auto {
                    total_fr += 1.0;
                }
            }
            GridTrack::MinContent | GridTrack::MaxContent => {
                let val = if is_tui {
                    intrinsic.round()
                } else {
                    snap_subpixel_grid(intrinsic)
                };
                resolved[i] = val;
                total_fixed += val;
            }
            GridTrack::MinMax(min_b, max_b) => {
                let min_px = match min_b {
                    GridTrackBreadth::Px(px) => px,
                    GridTrackBreadth::Percent(pct) => available_space * (pct / 100.0),
                    _ => 0.0,
                };
                let min_px = if is_tui {
                    min_px.round()
                } else {
                    snap_subpixel_grid(min_px)
                };
                match max_b {
                    GridTrackBreadth::Fr(fr) => {
                        resolved[i] = min_px;
                        total_fixed += min_px;
                        total_fr += fr;
                    }
                    GridTrackBreadth::Px(px) => {
                        let sz = px.max(min_px);
                        let val = if is_tui {
                            sz.round()
                        } else {
                            snap_subpixel_grid(sz)
                        };
                        resolved[i] = val;
                        total_fixed += val;
                    }
                    GridTrackBreadth::Percent(pct) => {
                        let sz = (available_space * (pct / 100.0)).max(min_px);
                        let val = if is_tui {
                            sz.round()
                        } else {
                            snap_subpixel_grid(sz)
                        };
                        resolved[i] = val;
                        total_fixed += val;
                    }
                    GridTrackBreadth::Auto => {
                        let sz = intrinsic.max(min_px);
                        let val = if is_tui {
                            sz.round()
                        } else {
                            snap_subpixel_grid(sz)
                        };
                        resolved[i] = val;
                        total_fixed += val;
                        if should_stretch_auto {
                            total_fr += 1.0;
                        }
                    }
                    GridTrackBreadth::MaxContent | GridTrackBreadth::MinContent => {
                        let sz = intrinsic.max(min_px);
                        let val = if is_tui {
                            sz.round()
                        } else {
                            snap_subpixel_grid(sz)
                        };
                        resolved[i] = val;
                        total_fixed += val;
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
                GridTrack::Fr(_) => Some(i),
                GridTrack::Auto | GridTrack::Subgrid if should_stretch_auto => Some(i),
                GridTrack::MinMax(_, GridTrackBreadth::Fr(_)) => Some(i),
                GridTrack::MinMax(_, GridTrackBreadth::Auto) if should_stretch_auto => Some(i),
                _ => None,
            })
            .collect();

        if is_tui {
            let factors: Vec<f32> = fr_indices
                .iter()
                .map(|&i| match tracks[i] {
                    GridTrack::Fr(fr) | GridTrack::MinMax(_, GridTrackBreadth::Fr(fr)) => fr,
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
    let bounded_rows = rows.min(MAX_GRID_TRACKS);
    let bounded_cols = cols.min(MAX_GRID_TRACKS);
    let current_cols = grid
        .first()
        .map_or(bounded_cols, |r| r.len())
        .max(bounded_cols)
        .min(MAX_GRID_TRACKS);
    if grid.len() < bounded_rows {
        grid.resize_with(bounded_rows, || vec![false; current_cols]);
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
    for r in r_start..(r_start + r_span).min(MAX_GRID_TRACKS) {
        if r >= grid.len() {
            continue;
        }
        let row = &grid[r];
        for c in c_start..(c_start + c_span).min(MAX_GRID_TRACKS) {
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
    let r_end = (r_start + r_span).min(MAX_GRID_TRACKS);
    let c_end = (c_start + c_span).min(MAX_GRID_TRACKS);
    ensure_grid_dimensions(grid, r_end, c_end);
    for row in grid
        .iter_mut()
        .skip(r_start)
        .take(r_end.saturating_sub(r_start))
    {
        for cell in row
            .iter_mut()
            .skip(c_start)
            .take(c_end.saturating_sub(c_start))
        {
            *cell = true;
        }
    }
}

/// Resolves a 1-based CSS grid line (positive or negative) against explicit track count.
/// Negative line numbers count backwards from the end of the explicit grid (W3C CSS Grid Level 1 §8.3).
fn resolve_grid_line(line: i32, explicit_tracks: usize) -> usize {
    if line < 0 {
        // In W3C Grid Level 1 §8.3, -1 represents line (explicit_tracks + 1),
        // -2 represents line explicit_tracks, etc.
        let resolved = explicit_tracks as i32 + 2 + line;
        (resolved.clamp(1, MAX_GRID_TRACKS as i32) - 1) as usize
    } else {
        (line.clamp(1, MAX_GRID_TRACKS as i32) - 1) as usize
    }
}

/// Resolves start track index (0-based) and span for a grid item axis.
fn resolve_axis_placement(
    start: Option<i32>,
    end: Option<i32>,
    span: Option<u32>,
    explicit_tracks: usize,
) -> (Option<usize>, usize) {
    match (start, end) {
        (Some(s), Some(e)) => {
            let s_track = resolve_grid_line(s, explicit_tracks);
            let e_track = resolve_grid_line(e, explicit_tracks);
            if e_track > s_track {
                let track_span =
                    (e_track - s_track).clamp(1, MAX_GRID_TRACKS.saturating_sub(s_track).max(1));
                (Some(s_track), track_span)
            } else if s_track > e_track {
                let track_span =
                    (s_track - e_track).clamp(1, MAX_GRID_TRACKS.saturating_sub(e_track).max(1));
                (Some(e_track), track_span)
            } else {
                (Some(s_track), 1)
            }
        }
        (Some(s), None) => {
            let s_track = resolve_grid_line(s, explicit_tracks);
            let s_span = (span.unwrap_or(1).max(1) as usize)
                .min(MAX_GRID_TRACKS.saturating_sub(s_track).max(1));
            (Some(s_track), s_span)
        }
        (None, Some(e)) => {
            let e_track = resolve_grid_line(e, explicit_tracks);
            let s_span = (span.unwrap_or(1).max(1) as usize).min(MAX_GRID_TRACKS);
            let s_track = e_track.saturating_sub(s_span);
            (Some(s_track), s_span.max(1))
        }
        (None, None) => {
            let s_span = (span.unwrap_or(1).max(1) as usize).min(MAX_GRID_TRACKS);
            (None, s_span)
        }
    }
}
