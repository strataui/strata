//! W3C CSS Grid Layout Level 2 Solver.

use crate::box_model::{clamp_dimension, resolve_dimension};
use crate::geometry::{LayoutGeometry, Rect};
use crate::tree::LayoutNode;
use strata_style::types::{GridTrack, Position};

/// Solves W3C CSS Grid Layout Level 2 for a container and its items.
pub fn solve_grid(container: &mut LayoutNode, available_width: f32, available_height: f32) {
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

    // 2. Resolve Column Tracks
    let col_tracks = if container.style.grid_template_columns.is_empty() {
        vec![GridTrack::Fr(1.0)]
    } else {
        container.style.grid_template_columns.clone()
    };

    let col_sizes = resolve_tracks(&col_tracks, inner_width, gap_col);
    let num_cols = col_sizes.len();

    // 3. Grid Item Auto-Placement
    struct PlacedItem {
        child_index: usize,
        col_start: usize,
        col_span: usize,
        row_start: usize,
        row_span: usize,
    }

    let mut placed_items = Vec::new();
    let mut grid_occupancy: Vec<Vec<bool>> = Vec::new(); // grid_occupancy[row][col]

    let mut cursor_row = 0;
    let mut cursor_col = 0;

    for (child_idx, child) in container.children.iter().enumerate() {
        if child.style.position == Position::Absolute || child.style.position == Position::Fixed {
            continue;
        }

        let span_col = (child.style.grid_column_span.unwrap_or(1) as usize)
            .min(num_cols)
            .max(1);
        let span_row = child.style.grid_row_span.unwrap_or(1) as usize;

        // Explicit grid placement overrides the current auto-placement cursor.
        // CSS grid lines are 1-based, so line 1 maps to track index 0.
        if let Some(line) = child.style.grid_column_start {
            if line >= 1 {
                cursor_col = ((line - 1) as usize).min(num_cols.saturating_sub(1));
            }
        }
        if let Some(line) = child.style.grid_row_start {
            if line >= 1 {
                cursor_row = (line - 1) as usize;
                while grid_occupancy.len() < cursor_row + span_row {
                    grid_occupancy.push(vec![false; num_cols]);
                }
            }
        }

        // Auto placement: find first available slot
        loop {
            if cursor_row >= grid_occupancy.len() {
                grid_occupancy.push(vec![false; num_cols]);
            }

            if cursor_col + span_col > num_cols {
                cursor_row += 1;
                cursor_col = 0;
                continue;
            }

            // Check if cells are free
            let mut fits = true;
            for r in 0..span_row {
                let target_r = cursor_row + r;
                while target_r >= grid_occupancy.len() {
                    grid_occupancy.push(vec![false; num_cols]);
                }
                for c in 0..span_col {
                    if grid_occupancy[target_r][cursor_col + c] {
                        fits = false;
                        break;
                    }
                }
                if !fits {
                    break;
                }
            }

            if fits {
                // Mark occupied
                for r in 0..span_row {
                    for c in 0..span_col {
                        grid_occupancy[cursor_row + r][cursor_col + c] = true;
                    }
                }

                placed_items.push(PlacedItem {
                    child_index: child_idx,
                    col_start: cursor_col,
                    col_span: span_col,
                    row_start: cursor_row,
                    row_span: span_row,
                });

                cursor_col += span_col;
                break;
            } else {
                cursor_col += 1;
            }
        }
    }

    let total_rows = grid_occupancy.len().max(1);

    // 4. Resolve Row Tracks
    let mut row_tracks = container.style.grid_template_rows.clone();
    while row_tracks.len() < total_rows {
        row_tracks.push(GridTrack::Auto);
    }

    let row_sizes = if let Some(h) = inner_height {
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
        for item in &placed_items {
            let child = &mut container.children[item.child_index];
            let item_width =
                calculate_span_size(&col_sizes, item.col_start, item.col_span, gap_col);
            child.compute_layout(item_width, available_height);
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

    // 6. Layout and position placed grid items
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
        if !child.children.is_empty() {
            child.layout_children();
        }
    }

    container.geometry.snap_to_pixel();
}

fn resolve_tracks(tracks: &[GridTrack], available_space: f32, gap: f32) -> Vec<f32> {
    let count = tracks.len();
    if count == 0 {
        return Vec::new();
    }

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
                resolved[i] = px;
                total_fixed += px;
            }
            GridTrack::Percent(pct) => {
                let px = available_space * (pct / 100.0);
                resolved[i] = px;
                total_fixed += px;
            }
            GridTrack::Fr(fr) => {
                total_fr += fr;
            }
            GridTrack::Auto | GridTrack::MinContent | GridTrack::MaxContent => {
                // Auto track fallback
                total_fr += 1.0;
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
                | GridTrack::MaxContent => Some(i),
                _ => None,
            })
            .collect();

        let mut allocated = 0.0f32;
        let fr_count = fr_indices.len();
        for (idx, &i) in fr_indices.iter().enumerate() {
            let factor = match tracks[i] {
                GridTrack::Fr(fr) => fr,
                GridTrack::Auto | GridTrack::MinContent | GridTrack::MaxContent => 1.0,
                _ => continue,
            };
            if idx == fr_count - 1 {
                // Chromium FindFrSize residue absorption: guarantee exact space conservation on final flex track
                resolved[i] = (remaining_space - allocated).max(0.0);
            } else {
                let size = ((factor / total_fr) * remaining_space * 64.0).round() / 64.0;
                resolved[i] = size;
                allocated += size;
            }
        }
    }

    resolved
}

fn calculate_offset(sizes: &[f32], index: usize, gap: f32) -> f32 {
    let mut offset = 0.0;
    for i in 0..index.min(sizes.len()) {
        offset += sizes[i] + gap;
    }
    offset
}

fn calculate_span_size(sizes: &[f32], start: usize, span: usize, gap: f32) -> f32 {
    let mut total = 0.0;
    let end = (start + span).min(sizes.len());
    for i in start..end {
        total += sizes[i];
    }
    if span > 1 {
        total += (span - 1) as f32 * gap;
    }
    total
}
