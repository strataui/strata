//! W3C Flexible Box Layout Level 1 Multi-Pass Solver.

use crate::box_model::{clamp_dimension, resolve_dimension};
use crate::geometry::{LayoutGeometry, Rect};
use crate::tree::LayoutNode;
use strata_style::types::{
    AlignItems, AlignSelf, Dimension, FlexDirection, FlexWrap, JustifyContent, Position,
};

struct FlexItemMeta {
    index: usize,
    hypothetical_main: f32,
    hypothetical_cross: f32,
    resolved_main: f32,
    resolved_cross: f32,
    flex_grow: f32,
    flex_shrink: f32,
    align_self: AlignSelf,
    margin_main_start: f32,
    margin_main_end: f32,
    margin_cross_start: f32,
    margin_cross_end: f32,
}

struct FlexLine {
    items: Vec<FlexItemMeta>,
    cross_size: f32,
}

/// Solves W3C Flexbox Level 1 layout for a flex container and its children.
pub fn solve_flexbox(container: &mut LayoutNode, available_width: f32, available_height: f32) {
    let padding = container.style.padding;
    let border = container.style.border_width;
    let margin = container.style.margin;

    // 1. Resolve container dimensions
    let resolved_width = match resolve_dimension(container.style.width, available_width) {
        Some(w) => clamp_dimension(w, container.style.min_width, container.style.max_width, available_width),
        None => {
            let auto_w = (available_width - margin.left - margin.right).max(0.0);
            clamp_dimension(auto_w, container.style.min_width, container.style.max_width, available_width)
        }
    };

    let inner_width = (resolved_width - padding.left - padding.right - border.left - border.right).max(0.0);
    let resolved_height = resolve_dimension(container.style.height, available_height)
        .map(|h| clamp_dimension(h, container.style.min_height, container.style.max_height, available_height));
    let inner_height = resolved_height
        .or_else(|| {
            resolve_dimension(container.style.min_height, available_height)
        })
        .map(|h| (h - padding.top - padding.bottom).max(0.0));

    let is_row = matches!(
        container.style.flex_direction,
        FlexDirection::Row | FlexDirection::RowReverse
    );
    let is_reverse = matches!(
        container.style.flex_direction,
        FlexDirection::RowReverse | FlexDirection::ColumnReverse
    );

    let main_gap = if is_row { container.style.gap_col.max(container.style.gap) } else { container.style.gap_row.max(container.style.gap) };
    let cross_gap = if is_row { container.style.gap_row.max(container.style.gap) } else { container.style.gap_col.max(container.style.gap) };

    let container_main = if is_row { inner_width } else { inner_height.unwrap_or(0.0) };

    // 2. Collect in-flow flex items and determine hypothetical sizes
    let mut item_metas = Vec::new();
    for (idx, child) in container.children.iter_mut().enumerate() {
        if child.style.position == Position::Absolute || child.style.position == Position::Fixed {
            continue;
        }

        let child_w = resolve_dimension(child.style.width, inner_width)
            .unwrap_or_else(|| {
                // If width is auto, probe hypothetical content
                child.compute_layout(inner_width, inner_height.unwrap_or(available_height));
                child.geometry.border_box.width
            });
        let child_h = resolve_dimension(child.style.height, inner_height.unwrap_or(available_height))
            .unwrap_or_else(|| {
                child.compute_layout(inner_width, inner_height.unwrap_or(available_height));
                child.geometry.border_box.height
            });

        let (mut main_size, mut cross_size) = if is_row {
            (child_w, child_h)
        } else {
            (child_h, child_w)
        };

        // If flex-basis is specified, use flex-basis for hypothetical main size
        if child.style.flex_basis != Dimension::Auto {
            if let Some(basis_px) = resolve_dimension(child.style.flex_basis, container_main) {
                main_size = basis_px;
            }
        }

        // Clamp main and cross sizes
        if is_row {
            main_size = clamp_dimension(main_size, child.style.min_width, child.style.max_width, inner_width);
            cross_size = clamp_dimension(cross_size, child.style.min_height, child.style.max_height, inner_height.unwrap_or(available_height));
        } else {
            main_size = clamp_dimension(main_size, child.style.min_height, child.style.max_height, inner_height.unwrap_or(available_height));
            cross_size = clamp_dimension(cross_size, child.style.min_width, child.style.max_width, inner_width);
        }

        let (m_main_start, m_main_end, m_cross_start, m_cross_end) = if is_row {
            (child.style.margin.left, child.style.margin.right, child.style.margin.top, child.style.margin.bottom)
        } else {
            (child.style.margin.top, child.style.margin.bottom, child.style.margin.left, child.style.margin.right)
        };

        item_metas.push(FlexItemMeta {
            index: idx,
            hypothetical_main: main_size,
            hypothetical_cross: cross_size,
            resolved_main: main_size,
            resolved_cross: cross_size,
            flex_grow: child.style.flex_grow,
            flex_shrink: child.style.flex_shrink,
            align_self: child.style.align_self,
            margin_main_start: m_main_start,
            margin_main_end: m_main_end,
            margin_cross_start: m_cross_start,
            margin_cross_end: m_cross_end,
        });
    }

    // 3. Break flex items into flex lines (wrapping)
    let is_wrap = matches!(
        container.style.flex_wrap,
        FlexWrap::Wrap | FlexWrap::WrapReverse
    );
    let mut lines: Vec<FlexLine> = Vec::new();

    if !item_metas.is_empty() {
        if !is_wrap || container_main <= 0.0 {
            // Single line (nowrap)
            let mut max_cross = item_metas.iter().map(|it| it.hypothetical_cross + it.margin_cross_start + it.margin_cross_end).fold(0.0f32, f32::max);
            if is_row {
                if let Some(h) = inner_height {
                    max_cross = max_cross.max(h);
                }
            } else {
                max_cross = max_cross.max(inner_width);
            }
            lines.push(FlexLine {
                items: item_metas,
                cross_size: max_cross,
            });
        } else {
            // Multi-line wrapping
            let mut current_line_items = Vec::new();
            let mut current_line_main = 0.0;

            for it in item_metas {
                let item_len = it.hypothetical_main + it.margin_main_start + it.margin_main_end;
                let needed = if current_line_items.is_empty() {
                    item_len
                } else {
                    current_line_main + main_gap + item_len
                };

                if !current_line_items.is_empty() && needed > container_main {
                    let cross = current_line_items.iter().map(|x: &FlexItemMeta| x.hypothetical_cross + x.margin_cross_start + x.margin_cross_end).fold(0.0f32, f32::max);
                    lines.push(FlexLine {
                        items: current_line_items,
                        cross_size: cross,
                    });
                    current_line_items = vec![it];
                    current_line_main = item_len;
                } else {
                    current_line_main = needed;
                    current_line_items.push(it);
                }
            }

            if !current_line_items.is_empty() {
                let cross = current_line_items.iter().map(|x| x.hypothetical_cross + x.margin_cross_start + x.margin_cross_end).fold(0.0f32, f32::max);
                lines.push(FlexLine {
                    items: current_line_items,
                    cross_size: cross,
                });
            }
        }
    }

    // 4. Resolve flexible lengths on main axis for each line
    let is_main_definite = if is_row {
        true
    } else {
        resolved_height.is_some()
    };

    for line in &mut lines {
        let count = line.items.len();
        if count == 0 {
            continue;
        }

        let total_gaps = if count > 1 { (count - 1) as f32 * main_gap } else { 0.0 };
        let sum_hypothetical: f32 = line.items.iter().map(|it| it.hypothetical_main + it.margin_main_start + it.margin_main_end).sum();

        if is_main_definite {
            let free_space = container_main - sum_hypothetical - total_gaps;

            if free_space > 0.0 {
                // Expansion via flex-grow
                let total_grow: f32 = line.items.iter().map(|it| it.flex_grow).sum();
                if total_grow > 0.0 {
                    for it in &mut line.items {
                        if it.flex_grow > 0.0 {
                            let delta = (it.flex_grow / total_grow) * free_space;
                            it.resolved_main = it.hypothetical_main + delta;
                        }
                    }
                }
            } else if free_space < 0.0 {
                // Contraction via flex-shrink
                let total_scaled_shrink: f32 = line
                    .items
                    .iter()
                    .map(|it| it.flex_shrink * it.hypothetical_main)
                    .sum();

                if total_scaled_shrink > 0.0 {
                    let abs_free = free_space.abs();
                    for it in &mut line.items {
                        if it.flex_shrink > 0.0 {
                            let scaled = it.flex_shrink * it.hypothetical_main;
                            let reduction = (scaled / total_scaled_shrink) * abs_free;
                            it.resolved_main = (it.hypothetical_main - reduction).max(0.0);
                        }
                    }
                }
            }
        }

        // Recompute line cross size after main sizing
        let item_max_cross = line.items.iter().map(|it| it.hypothetical_cross + it.margin_cross_start + it.margin_cross_end).fold(0.0f32, f32::max);
        line.cross_size = line.cross_size.max(item_max_cross);
    }

    if lines.len() == 1 {
        if is_row {
            if let Some(h) = inner_height {
                lines[0].cross_size = lines[0].cross_size.max(h);
            }
        } else {
            lines[0].cross_size = lines[0].cross_size.max(inner_width);
        }
    }

    // 5. Total container height if Auto
    let total_lines_cross: f32 = lines.iter().map(|l| l.cross_size).sum();
    let total_cross_gaps = if lines.len() > 1 { (lines.len() - 1) as f32 * cross_gap } else { 0.0 };
    let content_cross_size = total_lines_cross + total_cross_gaps;

    let raw_height = match resolved_height {
        Some(h) => h,
        None => {
            if is_row {
                content_cross_size + padding.top + padding.bottom + border.top + border.bottom
            } else {
                let sum_main: f32 = lines.iter().flat_map(|l| l.items.iter().map(|i| i.resolved_main + i.margin_main_start + i.margin_main_end)).sum();
                let gaps = if lines.iter().flat_map(|l| &l.items).count() > 1 {
                    (lines.iter().flat_map(|l| &l.items).count() - 1) as f32 * main_gap
                } else {
                    0.0
                };
                sum_main + gaps + padding.top + padding.bottom + border.top + border.bottom
            }
        }
    };
    let final_height = clamp_dimension(raw_height, container.style.min_height, container.style.max_height, available_height);

    let border_box = Rect::new(
        container.geometry.border_box.x,
        container.geometry.border_box.y,
        resolved_width,
        final_height,
    );
    container.geometry = LayoutGeometry::from_border_box(border_box, padding, border, margin);

    // 6. Positioning flex items across lines
    let mut current_line_cross = if is_row {
        container.geometry.content_box.y
    } else {
        container.geometry.content_box.x
    };

    for line in lines {
        let count = line.items.len();
        if count == 0 {
            continue;
        }

        let total_item_main: f32 = line.items.iter().map(|it| it.resolved_main + it.margin_main_start + it.margin_main_end).sum();
        let total_gaps = if count > 1 { (count - 1) as f32 * main_gap } else { 0.0 };
        let remaining_main = if is_main_definite {
            (container_main - total_item_main - total_gaps).max(0.0)
        } else {
            0.0
        };

        // Main axis alignment distribution
        let (start_offset, step_between) = match container.style.justify_content {
            JustifyContent::FlexStart => (0.0, main_gap),
            JustifyContent::FlexEnd => (remaining_main, main_gap),
            JustifyContent::Center => (remaining_main / 2.0, main_gap),
            JustifyContent::SpaceBetween => {
                if count > 1 {
                    (0.0, main_gap + (remaining_main / (count - 1) as f32))
                } else {
                    (0.0, main_gap)
                }
            }
            JustifyContent::SpaceAround => {
                let item_space = remaining_main / count as f32;
                (item_space / 2.0, main_gap + item_space)
            }
            JustifyContent::SpaceEvenly => {
                let item_space = remaining_main / (count + 1) as f32;
                (item_space, main_gap + item_space)
            }
        };

        let mut current_main = if is_row {
            container.geometry.content_box.x + start_offset
        } else {
            container.geometry.content_box.y + start_offset
        };

        let mut ordered_items = line.items;
        if is_reverse {
            ordered_items.reverse();
        }

        for it in ordered_items {
            let child = &mut container.children[it.index];

            // Cross-axis alignment
            let effective_align = match it.align_self {
                AlignSelf::Auto => container.style.align_items,
                AlignSelf::Stretch => AlignItems::Stretch,
                AlignSelf::FlexStart => AlignItems::FlexStart,
                AlignSelf::FlexEnd => AlignItems::FlexEnd,
                AlignSelf::Center => AlignItems::Center,
                AlignSelf::Baseline => AlignItems::Baseline,
            };

            let mut final_cross_size = it.resolved_cross;
            let cross_offset = match effective_align {
                AlignItems::FlexStart => 0.0,
                AlignItems::FlexEnd => line.cross_size - (final_cross_size + it.margin_cross_start + it.margin_cross_end),
                AlignItems::Center => (line.cross_size - (final_cross_size + it.margin_cross_start + it.margin_cross_end)) / 2.0,
                AlignItems::Stretch => {
                    let has_auto_cross = if is_row {
                        child.style.height == Dimension::Auto
                    } else {
                        child.style.width == Dimension::Auto
                    };
                    if has_auto_cross {
                        final_cross_size = (line.cross_size - it.margin_cross_start - it.margin_cross_end).max(0.0);
                    }
                    0.0
                }
                AlignItems::Baseline => 0.0,
            };

            let (item_x, item_y, item_w, item_h) = if is_row {
                (
                    current_main + it.margin_main_start,
                    current_line_cross + it.margin_cross_start + cross_offset,
                    it.resolved_main,
                    final_cross_size,
                )
            } else {
                (
                    current_line_cross + it.margin_cross_start + cross_offset,
                    current_main + it.margin_main_start,
                    final_cross_size,
                    it.resolved_main,
                )
            };

            child.geometry = LayoutGeometry::from_border_box(
                Rect::new(item_x, item_y, item_w, item_h),
                child.style.padding,
                child.style.border_width,
                child.style.margin,
            );
            if !child.children.is_empty() {
                child.layout_children();
            }

            current_main += it.margin_main_start + it.resolved_main + it.margin_main_end + step_between;
        }

        current_line_cross += line.cross_size + cross_gap;
    }

    container.geometry.snap_to_pixel();
}
