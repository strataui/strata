//! W3C Flexible Box Layout Level 1 Multi-Pass Solver.

use crate::box_model::{
    clamp_dimension, clamp_dimension_with_node, resolve_dimension, resolve_dimension_with_node,
};
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
    auto_margin_main_start: bool,
    auto_margin_main_end: bool,
    auto_margin_cross_start: bool,
    auto_margin_cross_end: bool,
    baseline_offset: f32,
}

struct FlexLine {
    items: Vec<FlexItemMeta>,
    cross_size: f32,
    max_baseline_ascent: f32,
}

/// Computes the first baseline offset from the top border box of a layout node iteratively (O(1) call stack).
fn compute_first_baseline(node: &LayoutNode) -> f32 {
    let mut cur = node;
    let mut acc_y = 0.0f32;
    loop {
        if let Some(first_line) = cur.line_boxes.first() {
            return acc_y
                + (first_line.bounds.y - cur.geometry.border_box.y).max(0.0)
                + first_line.baseline;
        }
        if cur.text.is_some() {
            return acc_y
                + cur.style.padding.top
                + cur.style.border_width.top
                + cur.style.font_size * 0.8;
        }
        let next = cur
            .children
            .iter()
            .find(|c| c.style.position != Position::Absolute && c.style.position != Position::Fixed);
        if let Some(child) = next {
            acc_y += (child.geometry.border_box.y - cur.geometry.border_box.y).max(0.0);
            cur = child;
        } else {
            return acc_y + cur.geometry.border_box.height;
        }
    }
}

/// Computes the line cross size and maximum baseline ascent for a flex line.
fn compute_line_cross_and_baseline(
    items: &[FlexItemMeta],
    container_align_items: AlignItems,
    is_row: bool,
    use_resolved: bool,
) -> (f32, f32) {
    if items.is_empty() {
        return (0.0, 0.0);
    }
    let mut max_non_baseline_cross = 0.0f32;
    let mut max_baseline_ascent = 0.0f32;
    let mut max_baseline_descent = 0.0f32;
    let mut has_baseline = false;

    for it in items {
        let cross = if use_resolved {
            it.resolved_cross
        } else {
            it.hypothetical_cross
        };
        let effective_align = match it.align_self {
            AlignSelf::Auto => container_align_items,
            AlignSelf::Stretch => AlignItems::Stretch,
            AlignSelf::FlexStart => AlignItems::FlexStart,
            AlignSelf::FlexEnd => AlignItems::FlexEnd,
            AlignSelf::Center => AlignItems::Center,
            AlignSelf::Baseline => AlignItems::Baseline,
        };

        let is_baseline = is_row
            && effective_align == AlignItems::Baseline
            && !it.auto_margin_cross_start
            && !it.auto_margin_cross_end;

        if is_baseline {
            has_baseline = true;
            let ascent = it.margin_cross_start + it.baseline_offset;
            let descent = (cross - it.baseline_offset).max(0.0) + it.margin_cross_end;
            if ascent > max_baseline_ascent {
                max_baseline_ascent = ascent;
            }
            if descent > max_baseline_descent {
                max_baseline_descent = descent;
            }
        } else {
            let total_cross = cross + it.margin_cross_start + it.margin_cross_end;
            if total_cross > max_non_baseline_cross {
                max_non_baseline_cross = total_cross;
            }
        }
    }

    let cross_size = if has_baseline {
        max_non_baseline_cross.max(max_baseline_ascent + max_baseline_descent)
    } else {
        max_non_baseline_cross
    };

    (cross_size, max_baseline_ascent)
}

/// Solves W3C Flexbox Level 1 layout for a flex container and its children.
pub fn solve_flexbox(container: &mut LayoutNode, available_width: f32, available_height: f32) {
    solve_flexbox_guarded(
        container,
        available_width,
        available_height,
        crate::tree::LayoutDepthGuard::root(),
    );
}

/// Solves W3C Flexbox Level 1 layout with an active depth guard to prevent stack overflow.
pub fn solve_flexbox_guarded(
    container: &mut LayoutNode,
    available_width: f32,
    available_height: f32,
    guard: crate::tree::LayoutDepthGuard,
) {
    solve_flexbox_with_definite_cross_guarded(
        container,
        available_width,
        available_height,
        None,
        guard,
    );
}

/// Solves flexbox layout with an explicitly definite inner cross size.
///
/// Absolutely positioned boxes can derive their final height from `top`/`bottom`
/// insets after the normal flow pass. When that happens the flex algorithm must
/// treat the resolved inner height as definite, otherwise a single flex line is
/// only as tall as its items and `align-items: center` has no free space to
/// distribute.
pub fn solve_flexbox_with_definite_cross(
    container: &mut LayoutNode,
    available_width: f32,
    available_height: f32,
    definite_inner_cross: Option<f32>,
) {
    solve_flexbox_with_definite_cross_guarded(
        container,
        available_width,
        available_height,
        definite_inner_cross,
        crate::tree::LayoutDepthGuard::root(),
    );
}

/// Solves flexbox layout with definite inner cross size and an active depth guard.
pub fn solve_flexbox_with_definite_cross_guarded(
    container: &mut LayoutNode,
    available_width: f32,
    available_height: f32,
    definite_inner_cross: Option<f32>,
    guard: crate::tree::LayoutDepthGuard,
) {
    let _prof = crate::prof::Scope::new(crate::prof::Slot::Flex);
    if guard.is_exhausted() {
        crate::tree::record_layout_overflow(guard.current_depth(), guard.max_depth());
        return;
    }
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
                // Only inline-level flex containers shrink-to-fit. A block-level
                // flex container with `width: auto` fills its containing block
                // regardless of its own `flex-shrink`, which describes how the
                // container behaves as an item of its parent, not how it sizes
                // its content.
                let is_inline_or_shrink = container.style.display
                    == strata_style::types::Display::InlineFlex
                    || container.style.display == strata_style::types::Display::Inline;
                if is_inline_or_shrink {
                    let mut sum_children = 0.0f32;
                    for child in &mut container.children {
                        if child.style.position == Position::Absolute
                            || child.style.position == Position::Fixed
                        {
                            continue;
                        }
                        let cw =
                            resolve_dimension_with_node(child.style.width, available_width, child)
                                .unwrap_or_else(|| {
                                    if child.geometry.border_box.width > 0.0 {
                                        child.geometry.border_box.width
                                    } else {
                                        let min_max = crate::intrinsic::compute_min_max_sizes(
                                            child,
                                            Some(available_width),
                                        );
                                        min_max.max_content.min(available_width)
                                    }
                                });
                        let mw = child.style.margin.left + child.style.margin.right;
                        if is_row {
                            sum_children += cw + mw;
                        } else {
                            sum_children = sum_children.max(cw + mw);
                        }
                    }
                    let in_flow_count = container
                        .children
                        .iter()
                        .filter(|c| {
                            c.style.position != Position::Absolute
                                && c.style.position != Position::Fixed
                        })
                        .count();
                    let gap_val = if is_row {
                        container.style.gap_col.max(container.style.gap)
                    } else {
                        container.style.gap_row.max(container.style.gap)
                    };
                    let gaps = if is_row && in_flow_count > 1 {
                        (in_flow_count - 1) as f32 * gap_val
                    } else {
                        0.0
                    };
                    let fit_w = (sum_children
                        + gaps
                        + padding.left
                        + padding.right
                        + border.left
                        + border.right)
                        .min(available_width);
                    clamp_dimension(
                        fit_w,
                        container.style.min_width,
                        container.style.max_width,
                        available_width,
                    )
                } else {
                    let auto_w = (available_width - margin.left - margin.right).max(0.0);
                    clamp_dimension(
                        auto_w,
                        container.style.min_width,
                        container.style.max_width,
                        available_width,
                    )
                }
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

    let base_inner_h = match resolved_height {
        Some(h) => Some((h - padding.top - padding.bottom - border.top - border.bottom).max(0.0)),
        None => definite_inner_cross,
    };
    let inner_height = match (base_inner_h, container.transition_content_height()) {
        (Some(h), Some(th)) => Some(h.max(th)),
        (None, Some(th)) => Some(th),
        (other, _) => other,
    };

    let main_gap = if is_row {
        container.style.gap_col.max(container.style.gap)
    } else {
        container.style.gap_row.max(container.style.gap)
    };
    let cross_gap = if is_row {
        container.style.gap_row.max(container.style.gap)
    } else {
        container.style.gap_col.max(container.style.gap)
    };

    let container_main = if is_row {
        inner_width
    } else {
        inner_height.unwrap_or(0.0)
    };

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

        let item_w_opt = resolve_dimension_with_node(child.style.width, inner_width, child)
            .or_else(|| {
                if is_row && child.style.flex_basis != Dimension::Auto {
                    let basis =
                        resolve_dimension_with_node(child.style.flex_basis, inner_width, child)?;
                    if basis > 0.0 {
                        Some(basis)
                    } else {
                        // When flex-basis is 0 (e.g. flex: 1), the item will receive distributed space.
                        // For measuring its cross size, give it the available inner_width so text doesn't wrap to 0px.
                        None
                    }
                } else if is_row || effective_align != AlignItems::Stretch {
                    let min_max = crate::intrinsic::compute_min_max_sizes(child, Some(inner_width));
                    let avail_child_w =
                        (inner_width - child.style.margin.left - child.style.margin.right).max(0.0);
                    Some(min_max.max_content.min(avail_child_w))
                } else {
                    None
                }
            });

        // `text-overflow` is not inherited, but in flex layout the text is an
        // anonymous item of the flex container. Apply the container's clipping
        // and ellipsis mode to the text item so its glyph runs can be truncated
        // even though the class/style lives on the container.
        if child.text.is_some()
            && container.style.text_overflow == strata_style::types::TextOverflow::Ellipsis
            && container.style.overflow_x != Overflow::Visible
        {
            let mut mutated = false;
            if child.style.text_overflow != strata_style::types::TextOverflow::Ellipsis {
                child.style.text_overflow = strata_style::types::TextOverflow::Ellipsis;
                mutated = true;
            }
            if child.style.overflow_x != Overflow::Hidden {
                child.style.overflow_x = Overflow::Hidden;
                mutated = true;
            }
            if child.style.overflow_y != Overflow::Hidden {
                child.style.overflow_y = Overflow::Hidden;
                mutated = true;
            }
            if container.style.white_space == strata_style::types::WhiteSpace::NoWrap
                && child.style.white_space != strata_style::types::WhiteSpace::NoWrap
            {
                child.style.white_space = strata_style::types::WhiteSpace::NoWrap;
                mutated = true;
            }
            if mutated {
                child.invalidate_self_caches();
            }
        }

        let cg = guard.enter().unwrap_or(guard);
        let clamped_item_w = item_w_opt.map(|w| {
            clamp_dimension_with_node(
                w,
                child.style.min_width,
                child.style.max_width,
                inner_width,
                child,
            )
        });
        let resolved_item_h = resolve_dimension(child.style.height, target_avail_h).map(|h| {
            clamp_dimension(
                h,
                child.style.min_height,
                child.style.max_height,
                target_avail_h,
            )
        });
        let (mut child_w, mut child_h, cached_baseline_opt) =
            match (clamped_item_w, resolved_item_h) {
                (Some(w), Some(h)) => {
                    let saved_w = child.style.width;
                    let saved_min_w = child.style.min_width;
                    let saved_max_w = child.style.max_width;
                    let saved_h = child.style.height;
                    let saved_min_h = child.style.min_height;
                    let saved_max_h = child.style.max_height;
                    child.style.width = Dimension::Px(w);
                    child.style.min_width = Dimension::Auto;
                    child.style.max_width = Dimension::Auto;
                    child.style.height = Dimension::Px(h);
                    child.style.min_height = Dimension::Auto;
                    child.style.max_height = Dimension::Auto;
                    let pin_hash = crate::tree::layout_style_pin_hash(&child.style);
                    let res = if let Some((_, _, bl)) = child.lookup_measure_cache(w, h, pin_hash) {
                        (w, h, Some(bl))
                    } else {
                        child.compute_layout_guarded(w, h, None, 0.0, cg);
                        let bl = compute_first_baseline(child);
                        child.store_measure_cache(0, w, h, pin_hash, w, h, bl);
                        (w, h, Some(bl))
                    };
                    child.style.width = saved_w;
                    child.style.min_width = saved_min_w;
                    child.style.max_width = saved_max_w;
                    child.style.height = saved_h;
                    child.style.min_height = saved_min_h;
                    child.style.max_height = saved_max_h;
                    res
                }
                (Some(w), None) => {
                    let saved_w = child.style.width;
                    let saved_min_w = child.style.min_width;
                    let saved_max_w = child.style.max_width;
                    child.style.width = Dimension::Px(w);
                    child.style.min_width = Dimension::Auto;
                    child.style.max_width = Dimension::Auto;
                    let pin_hash = crate::tree::layout_style_pin_hash(&child.style);
                    let res = if let Some((_, mh, bl)) =
                        child.lookup_measure_cache(w, target_avail_h, pin_hash)
                    {
                        (w, mh, Some(bl))
                    } else {
                        child.compute_layout_guarded(w, target_avail_h, None, 0.0, cg);
                        let mh = child.geometry.border_box.height;
                        let bl = compute_first_baseline(child);
                        child.store_measure_cache(0, w, target_avail_h, pin_hash, w, mh, bl);
                        (w, mh, Some(bl))
                    };
                    child.style.width = saved_w;
                    child.style.min_width = saved_min_w;
                    child.style.max_width = saved_max_w;
                    res
                }
                (None, Some(h)) => {
                    let saved_h = child.style.height;
                    let saved_min_h = child.style.min_height;
                    let saved_max_h = child.style.max_height;
                    child.style.height = Dimension::Px(h);
                    child.style.min_height = Dimension::Auto;
                    child.style.max_height = Dimension::Auto;
                    let pin_hash = crate::tree::layout_style_pin_hash(&child.style);
                    let res = if let Some((mw, _, bl)) =
                        child.lookup_measure_cache(inner_width, h, pin_hash)
                    {
                        (mw, h, Some(bl))
                    } else {
                        child.compute_layout_guarded(inner_width, h, None, 0.0, cg);
                        let mw = child.geometry.border_box.width;
                        let bl = compute_first_baseline(child);
                        child.store_measure_cache(0, inner_width, h, pin_hash, mw, h, bl);
                        (mw, h, Some(bl))
                    };
                    child.style.height = saved_h;
                    child.style.min_height = saved_min_h;
                    child.style.max_height = saved_max_h;
                    res
                }
                (None, None) => {
                    let pin_hash = crate::tree::layout_style_pin_hash(&child.style);
                    if let Some((mw, mh, bl)) =
                        child.lookup_measure_cache(inner_width, target_avail_h, pin_hash)
                    {
                        (mw, mh, Some(bl))
                    } else {
                        child.compute_layout_guarded(inner_width, target_avail_h, None, 0.0, cg);
                        let mw = child.geometry.border_box.width;
                        let mh = child.geometry.border_box.height;
                        let bl = compute_first_baseline(child);
                        child.store_measure_cache(
                            0,
                            inner_width,
                            target_avail_h,
                            pin_hash,
                            mw,
                            mh,
                            bl,
                        );
                        (mw, mh, Some(bl))
                    }
                }
            };

        if let Some(ref text_str) = child.text {
            if inner_width > 0.0 && child_w > inner_width {
                let (_, wrapped_h) = crate::text::measure_text_bounds_with_white_space(
                    text_str,
                    child.style.font_size,
                    child.style.line_height,
                    Some(inner_width),
                    crate::text::is_monospace_family(child.style.font_family),
                    child.style.white_space,
                );
                child_w = inner_width;
                child_h = wrapped_h;
                let new_w = Dimension::Px(child_w);
                let new_h = Dimension::Px(child_h);
                if child.style.width != new_w || child.style.height != new_h {
                    child.style.width = new_w;
                    child.style.height = new_h;
                    child.invalidate_self_caches();
                }
            }
        }

        let (mut main_size, mut cross_size) = if is_row {
            (child_w, child_h)
        } else {
            (child_h, child_w)
        };

        let has_content =
            !child.children.is_empty() || child.text.is_some() || !child.inline_items.is_empty();

        let auto_min_main = if !has_content {
            0.0
        } else if is_row {
            crate::intrinsic::compute_min_max_sizes(child, Some(inner_width)).min_content
        } else {
            child_h
        };

        // If flex-basis is specified, use flex-basis for hypothetical main size.
        // Per W3C Flexbox Level 1 §9.2: If container main size is indefinite (column with height: auto),
        // percentage or 0 flex-basis falls back to content size.
        let mut has_explicit_basis = false;
        if child.style.flex_basis != Dimension::Auto && (is_row || inner_height.is_some()) {
            if let Some(basis_px) =
                resolve_dimension_with_node(child.style.flex_basis, container_main, child)
            {
                main_size = basis_px;
                has_explicit_basis = true;
            }
        }

        // Automatic minimum size per W3C Flexbox Section 4.5:
        // For a flex item with min-size: auto (the initial value) and visible overflow,
        // the minimum main size is clamped by its content size when flex-basis is auto.
        let is_auto_min = if is_row {
            child.style.min_width == Dimension::Auto && child.style.overflow_x == Overflow::Visible
        } else {
            child.style.min_height == Dimension::Auto && child.style.overflow_y == Overflow::Visible
        };
        if is_auto_min && !has_explicit_basis {
            main_size = main_size.max(auto_min_main);
        }

        // Clamp main and cross sizes
        if is_row {
            main_size = clamp_dimension(
                main_size,
                child.style.min_width,
                child.style.max_width,
                inner_width,
            );
            cross_size = clamp_dimension(
                cross_size,
                child.style.min_height,
                child.style.max_height,
                inner_height.unwrap_or(available_height),
            );
        } else {
            main_size = clamp_dimension(
                main_size,
                child.style.min_height,
                child.style.max_height,
                inner_height.unwrap_or(available_height),
            );
            cross_size = clamp_dimension(
                cross_size,
                child.style.min_width,
                child.style.max_width,
                inner_width,
            );
        }

        let (m_main_start, m_main_end, m_cross_start, m_cross_end) = if is_row {
            (
                child.style.margin.left,
                child.style.margin.right,
                child.style.margin.top,
                child.style.margin.bottom,
            )
        } else {
            (
                child.style.margin.top,
                child.style.margin.bottom,
                child.style.margin.left,
                child.style.margin.right,
            )
        };

        let (auto_m_main_start, auto_m_main_end, auto_m_cross_start, auto_m_cross_end) = if is_row {
            (
                child.style.margin_auto_left,
                child.style.margin_auto_right,
                child.style.margin_auto_top,
                child.style.margin_auto_bottom,
            )
        } else {
            (
                child.style.margin_auto_top,
                child.style.margin_auto_bottom,
                child.style.margin_auto_left,
                child.style.margin_auto_right,
            )
        };

        let baseline_offset = cached_baseline_opt.unwrap_or_else(|| compute_first_baseline(child));

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
            auto_margin_main_start: auto_m_main_start,
            auto_margin_main_end: auto_m_main_end,
            auto_margin_cross_start: auto_m_cross_start,
            auto_margin_cross_end: auto_m_cross_end,
            baseline_offset,
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
            let (cross_size, max_baseline) = compute_line_cross_and_baseline(
                &item_metas,
                container.style.align_items,
                is_row,
                false,
            );
            let cross_size = if is_row {
                if let Some(h) = inner_height {
                    h
                } else {
                    cross_size
                }
            } else {
                inner_width
            };
            lines.push(FlexLine {
                items: item_metas,
                cross_size,
                max_baseline_ascent: max_baseline,
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
                    let (cross, max_baseline) = compute_line_cross_and_baseline(
                        &current_line_items,
                        container.style.align_items,
                        is_row,
                        false,
                    );
                    lines.push(FlexLine {
                        items: current_line_items,
                        cross_size: cross,
                        max_baseline_ascent: max_baseline,
                    });
                    current_line_items = vec![it];
                    current_line_main = item_len;
                } else {
                    current_line_main = needed;
                    current_line_items.push(it);
                }
            }

            if !current_line_items.is_empty() {
                let (cross, max_baseline) = compute_line_cross_and_baseline(
                    &current_line_items,
                    container.style.align_items,
                    is_row,
                    false,
                );
                lines.push(FlexLine {
                    items: current_line_items,
                    cross_size: cross,
                    max_baseline_ascent: max_baseline,
                });
            }
        }
    }

    // 4. Resolve flexible lengths on main axis for each line
    let is_main_definite = if is_row { true } else { inner_height.is_some() };

    for line in &mut lines {
        let count = line.items.len();
        if count == 0 {
            continue;
        }

        let total_gaps = if count > 1 {
            (count - 1) as f32 * main_gap
        } else {
            0.0
        };
        let sum_hypothetical: f32 = line
            .items
            .iter()
            .map(|it| it.hypothetical_main + it.margin_main_start + it.margin_main_end)
            .sum();

        let min_container_main = if is_row {
            resolve_dimension_with_node(container.style.min_width, available_width, container)
        } else {
            resolve_dimension_with_node(container.style.min_height, available_height, container)
        };

        let item_min_max = |ch: &LayoutNode, hyp_main: f32, is_scroll_main: bool| -> (f32, f32) {
            let min_main = if is_row {
                if is_scroll_main {
                    let explicit_min = resolve_dimension(ch.style.min_width, inner_width).unwrap_or(0.0);
                    hyp_main.max(explicit_min)
                } else if ch.style.min_width == Dimension::Auto {
                    let ch_has_content =
                        !ch.children.is_empty() || ch.text.is_some() || !ch.inline_items.is_empty();
                    let content_size = if ch.style.overflow_x == Overflow::Visible && ch_has_content
                    {
                        crate::intrinsic::compute_min_max_sizes(ch, Some(inner_width)).min_content
                    } else {
                        0.0
                    };
                    // W3C CSS Flexbox §4.5: automatic minimum size = min(specified_size, content_size)
                    let specified_size = resolve_dimension_with_node(ch.style.width, inner_width, ch)
                        .map(|w| match ch.style.box_sizing {
                            strata_style::types::BoxSizing::BorderBox => w,
                            strata_style::types::BoxSizing::ContentBox => {
                                w + ch.style.padding.left
                                    + ch.style.padding.right
                                    + ch.style.border_width.left
                                    + ch.style.border_width.right
                            }
                        })
                        .unwrap_or(f32::INFINITY);
                    let mut auto_min = content_size.min(specified_size);
                    if let Some(max_w) =
                        resolve_dimension_with_node(ch.style.max_width, inner_width, ch)
                    {
                        auto_min = auto_min.min(max_w);
                    }
                    auto_min
                } else {
                    resolve_dimension_with_node(ch.style.min_width, inner_width, ch).unwrap_or(0.0)
                }
            } else if is_scroll_main {
                let explicit_min =
                    resolve_dimension(ch.style.min_height, target_avail_h).unwrap_or(0.0);
                hyp_main.max(explicit_min)
            } else if ch.style.min_height == Dimension::Auto {
                if ch.style.overflow_y == Overflow::Visible
                    && (!ch.children.is_empty()
                        || ch.text.is_some()
                        || !ch.inline_items.is_empty())
                {
                    hyp_main
                } else {
                    0.0
                }
            } else {
                resolve_dimension(ch.style.min_height, target_avail_h).unwrap_or(0.0)
            };

            let max_main = if is_row {
                resolve_dimension_with_node(ch.style.max_width, inner_width, ch)
                    .unwrap_or(f32::INFINITY)
            } else {
                resolve_dimension(ch.style.max_height, target_avail_h).unwrap_or(f32::INFINITY)
            };

            (min_main, max_main.max(min_main))
        };

        let distribute_grow =
            |items: &mut [FlexItemMeta], target_main: f32, bounds: &[(f32, f32)]| {
                let n = items.len();
                let mut frozen: Vec<bool> = items.iter().map(|it| it.flex_grow <= 0.0).collect();
                for _ in 0..=n {
                    let used_frozen: f32 = items
                        .iter()
                        .zip(&frozen)
                        .filter(|(_, &f)| f)
                        .map(|(it, _)| it.resolved_main + it.margin_main_start + it.margin_main_end)
                        .sum();
                    let used_unfrozen: f32 = items
                        .iter()
                        .zip(&frozen)
                        .filter(|(_, &f)| !f)
                        .map(|(it, _)| {
                            it.hypothetical_main + it.margin_main_start + it.margin_main_end
                        })
                        .sum();
                    let rem_free = target_main - total_gaps - used_frozen - used_unfrozen;
                    if rem_free <= 0.001 {
                        break;
                    }
                    let total_grow: f32 = items
                        .iter()
                        .zip(&frozen)
                        .filter(|(_, &f)| !f)
                        .map(|(it, _)| it.flex_grow)
                        .sum();
                    if total_grow <= 0.0 {
                        break;
                    }
                    if strata_primitives::target::is_tui_target() {
                        let grow_shares: Vec<f32> = items
                            .iter()
                            .zip(&frozen)
                            .map(|(it, &f)| if f { 0.0 } else { it.flex_grow })
                            .collect();
                        let free_cells = rem_free.round().max(0.0) as u16;
                        let quants =
                            strata_primitives::quantization::largest_remainder_quantization(
                                &grow_shares,
                                free_cells,
                            );
                        for (it, &q) in items.iter_mut().zip(&quants) {
                            if it.flex_grow > 0.0 {
                                it.resolved_main = it.hypothetical_main.round() + q as f32;
                            }
                        }
                        break;
                    }
                    let mut hit_max = false;
                    for i in 0..n {
                        if frozen[i] {
                            continue;
                        }
                        let delta = (items[i].flex_grow / total_grow) * rem_free;
                        let cand = items[i].hypothetical_main + delta;
                        let max_m = bounds[i].1;
                        if cand > max_m + 0.001 && items[i].hypothetical_main <= max_m + 0.001 {
                            items[i].resolved_main = max_m;
                            frozen[i] = true;
                            hit_max = true;
                        }
                    }
                    if !hit_max {
                        for i in 0..n {
                            if !frozen[i] {
                                let delta = (items[i].flex_grow / total_grow) * rem_free;
                                items[i].resolved_main = items[i].hypothetical_main + delta;
                            }
                        }
                        break;
                    }
                }
            };

        let is_scroll_main = if is_row {
            matches!(
                container.style.overflow_x,
                Overflow::Auto | Overflow::Scroll
            )
        } else {
            matches!(
                container.style.overflow_y,
                Overflow::Auto | Overflow::Scroll
            )
        };
        let bounds: Vec<(f32, f32)> = line
            .items
            .iter()
            .map(|it| {
                item_min_max(
                    &container.children[it.index],
                    it.hypothetical_main,
                    is_scroll_main,
                )
            })
            .collect();

        if is_main_definite {
            let free_space = container_main - sum_hypothetical - total_gaps;

            if free_space > 0.0 {
                distribute_grow(&mut line.items, container_main, &bounds);
            } else if free_space < 0.0 {
                // W3C CSS Flexbox §9.7 iterative contraction via flex-shrink
                let n = line.items.len();
                let mut frozen: Vec<bool> = line
                    .items
                    .iter()
                    .map(|it| it.flex_shrink <= 0.0 || it.hypothetical_main <= 0.0)
                    .collect();

                for _ in 0..=n {
                    let used_frozen: f32 = line
                        .items
                        .iter()
                        .zip(&frozen)
                        .filter(|(_, &f)| f)
                        .map(|(it, _)| it.resolved_main + it.margin_main_start + it.margin_main_end)
                        .sum();
                    let used_unfrozen: f32 = line
                        .items
                        .iter()
                        .zip(&frozen)
                        .filter(|(_, &f)| !f)
                        .map(|(it, _)| {
                            it.hypothetical_main + it.margin_main_start + it.margin_main_end
                        })
                        .sum();
                    let rem_free = container_main - total_gaps - used_frozen - used_unfrozen;
                    if rem_free >= -0.001 {
                        break;
                    }
                    let total_scaled_shrink: f32 = line
                        .items
                        .iter()
                        .zip(&frozen)
                        .filter(|(_, &f)| !f)
                        .map(|(it, _)| it.flex_shrink * it.hypothetical_main)
                        .sum();
                    if total_scaled_shrink <= 0.0 {
                        break;
                    }
                    let abs_free = -rem_free;
                    let mut clamped_any = false;
                    for i in 0..n {
                        if frozen[i] {
                            continue;
                        }
                        let scaled = line.items[i].flex_shrink * line.items[i].hypothetical_main;
                        let reduction = (scaled / total_scaled_shrink) * abs_free;
                        let cand = line.items[i].hypothetical_main - reduction;
                        let min_main = bounds[i].0;
                        if cand < min_main - 0.001 {
                            line.items[i].resolved_main = min_main;
                            frozen[i] = true;
                            clamped_any = true;
                        }
                    }
                    if !clamped_any {
                        for i in 0..n {
                            if !frozen[i] {
                                let scaled =
                                    line.items[i].flex_shrink * line.items[i].hypothetical_main;
                                let reduction = (scaled / total_scaled_shrink) * abs_free;
                                line.items[i].resolved_main =
                                    (line.items[i].hypothetical_main - reduction).max(bounds[i].0);
                            }
                        }
                        break;
                    }
                }
            }
        } else if let Some(min_main) = min_container_main {
            let free_space = min_main - sum_hypothetical - total_gaps;
            if free_space > 0.0 {
                distribute_grow(&mut line.items, min_main, &bounds);
            }
        }

        // Apply the item's resolved min/max constraints to the final flexed
        // size, not only to the hypothetical size. A `flex: 1` item with
        // `max-width` (for example `flex-1 max-w-md`) must stop growing at the
        // max size; the leftover free space stays available for alignment.
        for it in &mut line.items {
            let child = &container.children[it.index];
            if is_row {
                it.resolved_main = clamp_dimension(
                    it.resolved_main,
                    child.style.min_width,
                    child.style.max_width,
                    inner_width,
                );
                it.resolved_cross = clamp_dimension(
                    it.resolved_cross,
                    child.style.min_height,
                    child.style.max_height,
                    inner_height.unwrap_or(available_height),
                );
            } else {
                it.resolved_main = clamp_dimension(
                    it.resolved_main,
                    child.style.min_height,
                    child.style.max_height,
                    inner_height.unwrap_or(available_height),
                );
                it.resolved_cross = clamp_dimension(
                    it.resolved_cross,
                    child.style.min_width,
                    child.style.max_width,
                    inner_width,
                );
            }
        }

        // Recompute line cross size after main sizing (CSS Flexbox Level 1 §9.4)
        let cg = guard.enter().unwrap_or(guard);
        for it in &mut line.items {
            let prev_main = if is_row {
                container.children[it.index].geometry.border_box.width
            } else {
                container.children[it.index].geometry.border_box.height
            };
            if (it.resolved_main - it.hypothetical_main).abs() > 0.5
                || (is_row && (it.resolved_main - prev_main).abs() > 0.5)
            {
                let child = &mut container.children[it.index];
                if is_row {
                    let saved_w = child.style.width;
                    let saved_min_w = child.style.min_width;
                    let saved_max_w = child.style.max_width;
                    let saved_h = child.style.height;
                    child.style.width = Dimension::Px(it.resolved_main);
                    child.style.min_width = Dimension::Auto;
                    child.style.max_width = Dimension::Auto;
                    let pin_hash = crate::tree::layout_style_pin_hash(&child.style);
                    let (mut res_h, bl) = if let Some((_, mh, cached_bl)) =
                        child.lookup_measure_cache(it.resolved_main, target_avail_h, pin_hash)
                    {
                        (mh, cached_bl)
                    } else {
                        child.compute_layout_guarded(
                            it.resolved_main,
                            target_avail_h,
                            None,
                            0.0,
                            cg,
                        );
                        let mh = child.geometry.border_box.height;
                        let computed_bl = compute_first_baseline(child);
                        child.store_measure_cache(
                            1,
                            it.resolved_main,
                            target_avail_h,
                            pin_hash,
                            it.resolved_main,
                            mh,
                            computed_bl,
                        );
                        (mh, computed_bl)
                    };
                    if res_h <= 0.0 && child.children.is_empty() {
                        if let Some(ref text_str) = child.text {
                            let content_w = (it.resolved_main
                                - child.style.padding.left
                                - child.style.padding.right
                                - child.style.border_width.left
                                - child.style.border_width.right)
                                .max(0.0);
                            let (_, text_h) = crate::text::measure_text_bounds_with_white_space(
                                text_str,
                                child.style.font_size,
                                child.style.line_height,
                                if content_w > 0.0 {
                                    Some(content_w)
                                } else {
                                    None
                                },
                                crate::text::is_monospace_family(child.style.font_family),
                                child.style.white_space,
                            );
                            res_h = text_h
                                + child.style.padding.top
                                + child.style.padding.bottom
                                + child.style.border_width.top
                                + child.style.border_width.bottom;
                        }
                    }
                    child.style.width = saved_w;
                    child.style.min_width = saved_min_w;
                    child.style.max_width = saved_max_w;
                    child.style.height = saved_h;
                    it.resolved_cross = res_h;
                    it.baseline_offset = bl;
                } else {
                    let saved_h = child.style.height;
                    let saved_min_h = child.style.min_height;
                    let saved_max_h = child.style.max_height;
                    child.style.height = Dimension::Px(it.resolved_main);
                    child.style.min_height = Dimension::Auto;
                    child.style.max_height = Dimension::Auto;
                    let pin_hash = crate::tree::layout_style_pin_hash(&child.style);
                    let (res_w, bl) = if let Some((mw, _, cached_bl)) =
                        child.lookup_measure_cache(inner_width, it.resolved_main, pin_hash)
                    {
                        (mw, cached_bl)
                    } else {
                        child.compute_layout_guarded(inner_width, it.resolved_main, None, 0.0, cg);
                        let mw = child.geometry.border_box.width;
                        let computed_bl = compute_first_baseline(child);
                        child.store_measure_cache(
                            1,
                            inner_width,
                            it.resolved_main,
                            pin_hash,
                            mw,
                            it.resolved_main,
                            computed_bl,
                        );
                        (mw, computed_bl)
                    };
                    child.style.height = saved_h;
                    child.style.min_height = saved_min_h;
                    child.style.max_height = saved_max_h;
                    it.resolved_cross = res_w;
                    it.baseline_offset = bl;
                }
            }
        }
        let (recomputed_cross, max_baseline) =
            compute_line_cross_and_baseline(&line.items, container.style.align_items, is_row, true);
        line.max_baseline_ascent = max_baseline;
        line.cross_size = if is_row && inner_height.is_none() {
            recomputed_cross
        } else {
            line.cross_size.max(recomputed_cross)
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
    let total_cross_gaps = if lines.len() > 1 {
        (lines.len() - 1) as f32 * cross_gap
    } else {
        0.0
    };
    let content_cross_size = total_lines_cross + total_cross_gaps;

    let raw_height = match resolved_height {
        Some(h) => h,
        None => {
            if is_row {
                content_cross_size + padding.top + padding.bottom + border.top + border.bottom
            } else {
                let sum_main: f32 = lines
                    .iter()
                    .flat_map(|l| {
                        l.items
                            .iter()
                            .map(|i| i.resolved_main + i.margin_main_start + i.margin_main_end)
                    })
                    .sum();
                let gaps = if lines.iter().flat_map(|l| &l.items).count() > 1 {
                    (lines.iter().flat_map(|l| &l.items).count() - 1) as f32 * main_gap
                } else {
                    0.0
                };
                sum_main + gaps + padding.top + padding.bottom + border.top + border.bottom
            }
        }
    };
    let final_height = clamp_dimension(
        raw_height,
        container.style.min_height,
        container.style.max_height,
        available_height,
    );

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

        let mut line_items = line.items;
        let total_item_main: f32 = line_items
            .iter()
            .map(|it| it.resolved_main + it.margin_main_start + it.margin_main_end)
            .sum();
        let total_gaps = if count > 1 {
            (count - 1) as f32 * main_gap
        } else {
            0.0
        };
        let remaining_main = if is_main_definite {
            (container_main - total_item_main - total_gaps).max(0.0)
        } else {
            0.0
        };

        // Section 8.1: Distribute remaining positive free space to auto margins on main axis
        let num_auto_main: usize = line_items
            .iter()
            .map(|it| (it.auto_margin_main_start as usize) + (it.auto_margin_main_end as usize))
            .sum();

        let (effective_remaining_main, auto_margin_px) =
            if num_auto_main > 0 && remaining_main > 0.0 {
                (0.0, remaining_main / num_auto_main as f32)
            } else {
                (remaining_main, 0.0)
            };

        if num_auto_main > 0 {
            for it in &mut line_items {
                if it.auto_margin_main_start {
                    it.margin_main_start = auto_margin_px;
                }
                if it.auto_margin_main_end {
                    it.margin_main_end = auto_margin_px;
                }
            }
        }

        // Main axis alignment distribution
        let (start_offset, step_between) = match container.style.justify_content {
            JustifyContent::FlexStart => (0.0, main_gap),
            JustifyContent::FlexEnd => (effective_remaining_main, main_gap),
            JustifyContent::Center => (effective_remaining_main / 2.0, main_gap),
            JustifyContent::SpaceBetween => {
                if count > 1 {
                    (
                        0.0,
                        main_gap + (effective_remaining_main / (count - 1) as f32),
                    )
                } else {
                    (0.0, main_gap)
                }
            }
            JustifyContent::SpaceAround => {
                let item_space = effective_remaining_main / count as f32;
                (item_space / 2.0, main_gap + item_space)
            }
            JustifyContent::SpaceEvenly => {
                let item_space = effective_remaining_main / (count + 1) as f32;
                (item_space, main_gap + item_space)
            }
        };

        let mut current_main = if is_row {
            container.geometry.content_box.x + start_offset
        } else {
            container.geometry.content_box.y + start_offset
        };

        let mut ordered_items = line_items;
        if is_reverse {
            ordered_items.reverse();
        }

        for mut it in ordered_items {
            let child = &mut container.children[it.index];

            // Cross-axis auto margins and alignment per Section 8.1 and Section 9.5
            let has_cross_auto = it.auto_margin_cross_start || it.auto_margin_cross_end;
            let (cross_offset, final_cross_size, actual_m_cross_start) = if has_cross_auto {
                let remaining_cross = (line.cross_size
                    - it.resolved_cross
                    - (if it.auto_margin_cross_start {
                        0.0
                    } else {
                        it.margin_cross_start
                    })
                    - (if it.auto_margin_cross_end {
                        0.0
                    } else {
                        it.margin_cross_end
                    }))
                .max(0.0);
                let (m_start, m_end) = if it.auto_margin_cross_start && it.auto_margin_cross_end {
                    (remaining_cross / 2.0, remaining_cross / 2.0)
                } else if it.auto_margin_cross_start {
                    (remaining_cross, it.margin_cross_end)
                } else {
                    (it.margin_cross_start, remaining_cross)
                };
                it.margin_cross_start = m_start;
                it.margin_cross_end = m_end;
                (0.0, it.resolved_cross, m_start)
            } else {
                let effective_align = match it.align_self {
                    AlignSelf::Auto => container.style.align_items,
                    AlignSelf::Stretch => AlignItems::Stretch,
                    AlignSelf::FlexStart => AlignItems::FlexStart,
                    AlignSelf::FlexEnd => AlignItems::FlexEnd,
                    AlignSelf::Center => AlignItems::Center,
                    AlignSelf::Baseline => AlignItems::Baseline,
                };

                let mut size = it.resolved_cross;
                let offset = match effective_align {
                    AlignItems::FlexStart => 0.0,
                    AlignItems::FlexEnd => {
                        line.cross_size - (size + it.margin_cross_start + it.margin_cross_end)
                    }
                    AlignItems::Center => {
                        (line.cross_size - (size + it.margin_cross_start + it.margin_cross_end))
                            / 2.0
                    }
                    AlignItems::Stretch => {
                        let has_auto_cross = if is_row {
                            child.style.height == Dimension::Auto
                                || matches!(child.style.height, Dimension::Percent(_))
                        } else {
                            child.style.width == Dimension::Auto
                                || matches!(child.style.width, Dimension::Percent(_))
                        };
                        if has_auto_cross {
                            let stretched =
                                (line.cross_size - it.margin_cross_start - it.margin_cross_end)
                                    .max(0.0);
                            size = if is_row {
                                clamp_dimension(
                                    stretched,
                                    child.style.min_height,
                                    child.style.max_height,
                                    inner_height.unwrap_or(available_height),
                                )
                            } else {
                                clamp_dimension(
                                    stretched,
                                    child.style.min_width,
                                    child.style.max_width,
                                    inner_width,
                                )
                            };
                        }
                        0.0
                    }
                    AlignItems::Baseline => {
                        if is_row && line.max_baseline_ascent > 0.0 {
                            (line.max_baseline_ascent
                                - (it.margin_cross_start + it.baseline_offset))
                                .max(0.0)
                        } else {
                            0.0
                        }
                    }
                };
                (offset, size, it.margin_cross_start)
            };

            let (item_x, item_y, item_w, item_h) = if is_row {
                (
                    current_main + it.margin_main_start,
                    current_line_cross + actual_m_cross_start + cross_offset,
                    it.resolved_main,
                    final_cross_size,
                )
            } else {
                (
                    current_line_cross + actual_m_cross_start + cross_offset,
                    current_main + it.margin_main_start,
                    final_cross_size,
                    it.resolved_main,
                )
            };

            let mut child_margin = child.style.margin;
            if is_row {
                child_margin.left = it.margin_main_start;
                child_margin.right = it.margin_main_end;
                child_margin.top = it.margin_cross_start;
                child_margin.bottom = it.margin_cross_end;
            } else {
                child_margin.top = it.margin_main_start;
                child_margin.bottom = it.margin_main_end;
                child_margin.left = it.margin_cross_start;
                child_margin.right = it.margin_cross_end;
            }

            let old_x = child.geometry.border_box.x;
            let old_y = child.geometry.border_box.y;
            let old_w = child.geometry.border_box.width;
            let old_h = child.geometry.border_box.height;

            let desired_content_w = (item_w
                - child.style.padding.left
                - child.style.padding.right
                - child.style.border_width.left
                - child.style.border_width.right)
                .max(0.0);
            let desired_content_h = (item_h
                - child.style.padding.top
                - child.style.padding.bottom
                - child.style.border_width.top
                - child.style.border_width.bottom)
                .max(0.0);
            // A previous pass may have formatted the descendants for a
            // different content size while the border box already matches the
            // final placement size. Trusting the border box alone would leave
            // children laid out at their max-content width after a cross-axis
            // stretch. Re-run child layout whenever the content size changed,
            // regardless of `flex-shrink`.
            let children_current = child.children_layout_clean
                && child.last_child_layout_content.map_or(
                    child.children.is_empty()
                        && child.inline_items.is_empty()
                        && child.text.is_none(),
                    |(w, h)| {
                        (w - desired_content_w).abs() < 0.05 && (h - desired_content_h).abs() < 0.05
                    },
                );

            if children_current
                && old_w > 0.0
                && (item_w - old_w).abs() < 0.05
                && (item_h - old_h).abs() < 0.05
            {
                let saved_scroll_bounds = child.geometry.scroll_bounds;
                let saved_has_scroll = child.geometry.has_scroll;
                let saved_scroll_offset = child.geometry.scroll_offset;
                let mut new_geom = LayoutGeometry::from_border_box(
                    Rect::new(item_x, item_y, item_w, item_h),
                    child.style.padding,
                    child.style.border_width,
                    child_margin,
                );
                new_geom.snap_to_pixel();
                let dx = new_geom.border_box.x - old_x;
                let dy = new_geom.border_box.y - old_y;
                if dx.abs() > 0.001 || dy.abs() > 0.001 {
                    child.translate(dx, dy);
                }
                child.geometry = new_geom;
                child.applied_relative_offset = (0.0, 0.0);
                child.geometry.scroll_bounds = saved_scroll_bounds;
                child.geometry.has_scroll = saved_has_scroll;
                child.geometry.scroll_offset = saved_scroll_offset;
                child.last_child_layout_content = Some((desired_content_w, desired_content_h));
            } else {
                child.geometry = LayoutGeometry::from_border_box(
                    Rect::new(item_x, item_y, item_w, item_h),
                    child.style.padding,
                    child.style.border_width,
                    child_margin,
                );
                child.applied_relative_offset = (0.0, 0.0);
                child.geometry.snap_to_pixel();
                if !child.children.is_empty() {
                    if let Some(cg) = guard.enter() {
                        child.layout_children_guarded(cg);
                    }
                } else if !child.inline_items.is_empty() || child.text.is_some() {
                    let content_w = child.geometry.content_box.width;
                    let content_h = child.geometry.content_box.height;
                    if let Some(cg) = guard.enter() {
                        crate::box_model::solve_block_children_with_exclusions_guarded(
                            child, content_w, content_h, None, 0.0, cg,
                        );
                    }
                    child.subtree_hit_flags &= !crate::tree::SUBTREE_META_VALID;
                    child.update_scroll_metrics();
                    child.last_child_layout_content = Some((content_w, content_h));
                    child.children_layout_clean = true;
                } else {
                    child.subtree_hit_flags &= !crate::tree::SUBTREE_META_VALID;
                    child.update_scroll_metrics();
                    child.last_child_layout_content = Some((desired_content_w, desired_content_h));
                    child.children_layout_clean = true;
                }
            }

            current_main +=
                it.margin_main_start + it.resolved_main + it.margin_main_end + step_between;
        }

        current_line_cross += line.cross_size + cross_gap;
    }

    container.geometry.snap_to_pixel();
}
