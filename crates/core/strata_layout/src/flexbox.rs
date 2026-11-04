//! W3C Flexible Box Layout Level 1 Multi-Pass Solver.

use crate::box_model::{clamp_dimension, resolve_dimension};
use crate::geometry::{LayoutGeometry, Rect};
use crate::tree::LayoutNode;
use strata_style::types::{
    AlignItems, AlignSelf, Dimension, FlexDirection, FlexWrap, JustifyContent, Overflow, Position,
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

    let is_row = matches!(
        container.style.flex_direction,
        FlexDirection::Row | FlexDirection::RowReverse
    );
    let is_reverse = matches!(
        container.style.flex_direction,
        FlexDirection::RowReverse | FlexDirection::ColumnReverse
    );

    // 1. Resolve container dimensions
    let resolved_width = match resolve_dimension(container.style.width, available_width) {
        Some(w) => clamp_dimension(w, container.style.min_width, container.style.max_width, available_width),
        None => {
            let is_inline_or_shrink = container.style.display == strata_style::types::Display::InlineFlex
                || container.style.display == strata_style::types::Display::Inline
                || container.style.flex_shrink == 0.0;
            if is_inline_or_shrink {
                let mut sum_children = 0.0f32;
                for child in &mut container.children {
                    if child.style.position == Position::Absolute || child.style.position == Position::Fixed {
                        continue;
                    }
                    let cw = resolve_dimension(child.style.width, available_width)
                        .unwrap_or_else(|| {
                            if child.geometry.border_box.width > 0.0 {
                                child.geometry.border_box.width
                            } else {
                                child.compute_layout(available_width, available_height);
                                child.geometry.border_box.width
                            }
                        });
                    let mw = child.style.margin.left + child.style.margin.right;
                    if is_row {
                        sum_children += cw + mw;
                    } else {
                        sum_children = sum_children.max(cw + mw);
                    }
                }
                let in_flow_count = container.children.iter().filter(|c| c.style.position != Position::Absolute && c.style.position != Position::Fixed).count();
                let gap_val = if is_row { container.style.gap_col.max(container.style.gap) } else { container.style.gap_row.max(container.style.gap) };
                let gaps = if is_row && in_flow_count > 1 {
                    (in_flow_count - 1) as f32 * gap_val
                } else {
                    0.0
                };
                let fit_w = (sum_children + gaps + padding.left + padding.right + border.left + border.right).min(available_width);
                clamp_dimension(fit_w, container.style.min_width, container.style.max_width, available_width)
            } else {
                let auto_w = (available_width - margin.left - margin.right).max(0.0);
                clamp_dimension(auto_w, container.style.min_width, container.style.max_width, available_width)
            }
        }
    };

    let inner_width = (resolved_width - padding.left - padding.right - border.left - border.right).max(0.0);
    let resolved_height = resolve_dimension(container.style.height, available_height)
        .map(|h| clamp_dimension(h, container.style.min_height, container.style.max_height, available_height));

    let inner_height = resolved_height
        .or_else(|| {
            let mh = resolve_dimension(container.style.min_height, available_height)?;
            if mh > 0.0 { Some(mh) } else { None }
        })
        .map(|h| (h - padding.top - padding.bottom).max(0.0));

    let main_gap = if is_row { container.style.gap_col.max(container.style.gap) } else { container.style.gap_row.max(container.style.gap) };
    let cross_gap = if is_row { container.style.gap_row.max(container.style.gap) } else { container.style.gap_col.max(container.style.gap) };

    let container_main = if is_row { inner_width } else { inner_height.unwrap_or(0.0) };

    let target_avail_h = inner_height.unwrap_or(available_height);

    // 2. Collect in-flow flex items and determine hypothetical sizes
    let mut item_metas = Vec::new();
    for (idx, child) in container.children.iter_mut().enumerate() {
        if child.style.position == Position::Absolute || child.style.position == Position::Fixed {
            continue;
        }

        let effective_align = match child.style.align_self {
            AlignSelf::Auto => container.style.align_items,
            AlignSelf::Stretch => AlignItems::Stretch,
            AlignSelf::FlexStart => AlignItems::FlexStart,
            AlignSelf::FlexEnd => AlignItems::FlexEnd,
            AlignSelf::Center => AlignItems::Center,
            AlignSelf::Baseline => AlignItems::Baseline,
        };

        let should_shrink_child = if is_row {
            child.style.width == Dimension::Auto
                && child.style.flex_basis == Dimension::Auto
                && child.style.flex_grow == 0.0
        } else {
            child.style.width == Dimension::Auto
                && effective_align != AlignItems::Stretch
        };

        if should_shrink_child && child.style.display == strata_style::types::Display::Flex {
            child.style.display = strata_style::types::Display::InlineFlex;
        }

        let (mut child_w, mut child_h) = match (
            resolve_dimension(child.style.width, inner_width),
            resolve_dimension(child.style.height, target_avail_h),
        ) {
            (Some(w), Some(h)) => (w, h),
            (Some(w), None) => {
                child.compute_layout(w, target_avail_h);
                (w, child.geometry.border_box.height)
            }
            (None, Some(h)) => {
                child.compute_layout(inner_width, h);
                (child.geometry.border_box.width, h)
            }
            (None, None) => {
                child.compute_layout(inner_width, target_avail_h);
                (child.geometry.border_box.width, child.geometry.border_box.height)
            }
        };

        if let Some(ref text_str) = child.text {
            if inner_width > 0.0 && child_w > inner_width {
                let (_, wrapped_h) = crate::text::measure_text_bounds(
                    text_str,
                    child.style.font_size,
                    child.style.line_height,
                    Some(inner_width),
                );
                child_w = inner_width;
                child_h = wrapped_h;
                child.style.width = Dimension::Px(child_w);
                child.style.height = Dimension::Px(child_h);
            }
        }

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
            let cross_size = if is_row {
                if let Some(h) = inner_height {
                    h
                } else {
                    item_metas.iter().map(|it| it.hypothetical_cross + it.margin_cross_start + it.margin_cross_end).fold(0.0f32, f32::max)
                }
            } else {
                inner_width
            };
            lines.push(FlexLine {
                items: item_metas,
                cross_size,
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
                // Contraction via flex-shrink along main axis (row or column)
                let total_scaled_shrink: f32 = line
                    .items
                    .iter()
                    .map(|it| it.flex_shrink * it.hypothetical_main)
                    .sum();

                let is_scroll_main = if is_row {
                    matches!(container.style.overflow_x, Overflow::Auto | Overflow::Scroll)
                } else {
                    matches!(container.style.overflow_y, Overflow::Auto | Overflow::Scroll)
                };

                if total_scaled_shrink > 0.0 {
                    let abs_free = free_space.abs();
                    for it in &mut line.items {
                        if it.flex_shrink > 0.0 {
                            let scaled = it.flex_shrink * it.hypothetical_main;
                            let reduction = (scaled / total_scaled_shrink) * abs_free;
                            let min_main = if is_row {
                                if is_scroll_main {
                                    let explicit_min = resolve_dimension(container.children[it.index].style.min_width, inner_width).unwrap_or(0.0);
                                    it.hypothetical_main.max(explicit_min)
                                } else if container.children[it.index].style.min_width == Dimension::Auto {
                                    0.0
                                } else {
                                    resolve_dimension(container.children[it.index].style.min_width, inner_width).unwrap_or(0.0)
                                }
                            } else {
                                if is_scroll_main {
                                    let explicit_min = resolve_dimension(container.children[it.index].style.min_height, target_avail_h).unwrap_or(0.0);
                                    it.hypothetical_main.max(explicit_min)
                                } else if container.children[it.index].style.min_height == Dimension::Auto {
                                    0.0
                                } else {
                                    resolve_dimension(container.children[it.index].style.min_height, target_avail_h).unwrap_or(0.0)
                                }
                            };
                            it.resolved_main = (it.hypothetical_main - reduction).max(min_main);
                        }
                    }
                }
            }
        }

        // Recompute line cross size after main sizing (CSS Flexbox Level 1 §9.4)
        for it in &mut line.items {
            if (it.resolved_main - it.hypothetical_main).abs() > 0.5 {
                let child = &mut container.children[it.index];
                if is_row {
                    child.compute_layout(it.resolved_main, target_avail_h);
                    it.resolved_cross = child.geometry.border_box.height;
                } else {
                    child.compute_layout(inner_width, it.resolved_main);
                    it.resolved_cross = child.geometry.border_box.width;
                }
            }
        }
        let item_max_cross = line.items.iter().map(|it| it.resolved_cross + it.margin_cross_start + it.margin_cross_end).fold(0.0f32, f32::max);
        line.cross_size = if is_row && inner_height.is_none() {
            item_max_cross
        } else {
            line.cross_size.max(item_max_cross)
        };
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
                        child.style.height == Dimension::Auto || matches!(child.style.height, Dimension::Percent(_))
                    } else {
                        child.style.width == Dimension::Auto || matches!(child.style.width, Dimension::Percent(_))
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

            let old_x = child.geometry.border_box.x;
            let old_y = child.geometry.border_box.y;
            let old_w = child.geometry.border_box.width;
            let old_h = child.geometry.border_box.height;

            if old_w > 0.0
                && (item_w - old_w).abs() < 0.05
                && (item_h - old_h).abs() < 0.05
            {
                let dx = item_x - old_x;
                let dy = item_y - old_y;
                if dx.abs() > 0.001 || dy.abs() > 0.001 {
                    child.translate(dx, dy);
                }
                child.geometry = LayoutGeometry::from_border_box(
                    Rect::new(item_x, item_y, item_w, item_h),
                    child.style.padding,
                    child.style.border_width,
                    child.style.margin,
                );
            } else {
                child.geometry = LayoutGeometry::from_border_box(
                    Rect::new(item_x, item_y, item_w, item_h),
                    child.style.padding,
                    child.style.border_width,
                    child.style.margin,
                );
                if !child.children.is_empty() {
                    child.layout_children();
                } else if !child.inline_items.is_empty() || child.text.is_some() {
                    let content_w = child.geometry.content_box.width;
                    let content_h = child.geometry.content_box.height;
                    crate::box_model::solve_block_children(child, content_w, content_h);
                }
            }

            current_main += it.margin_main_start + it.resolved_main + it.margin_main_end + step_between;
        }

        current_line_cross += line.cross_size + cross_gap;
    }

    container.geometry.snap_to_pixel();
}
