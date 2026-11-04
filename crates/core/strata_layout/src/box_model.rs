//! CSS 2.1 Section 8.3.1 Box Model and Vertical Margin Collapsing Engine.

use crate::geometry::{LayoutGeometry, Rect};
use crate::tree::LayoutNode;
use strata_style::types::{BoxSizing, Dimension, Position};

/// Collapses two adjacent vertical margins according to CSS 2.1 §8.3.1.
///
/// Rules:
/// - If both are positive, the result is the maximum of the two: $\max(m_1, m_2)$.
/// - If both are negative, the result is the most negative: $-\max(|m_1|, |m_2|)$.
/// - If one is positive and one is negative, add the most negative to the most positive:
///   $\max(m_1, m_2, 0) + \min(m_1, m_2, 0)$.
pub fn collapse_margins(m1: f32, m2: f32) -> f32 {
    if m1 >= 0.0 && m2 >= 0.0 {
        m1.max(m2)
    } else if m1 < 0.0 && m2 < 0.0 {
        -(-m1).max(-m2)
    } else {
        let max_pos = m1.max(m2).max(0.0);
        let min_neg = m1.min(m2).min(0.0);
        max_pos + min_neg
    }
}

/// Resolves a CSS `Dimension` into concrete pixels relative to an available length.
pub fn resolve_dimension(dim: Dimension, available: f32) -> Option<f32> {
    match dim {
        Dimension::Auto => None,
        Dimension::Px(px) => Some(px),
        Dimension::Percent(pct) => Some(available * (pct / 100.0)),
        Dimension::Vw(vw) => Some(available * (vw / 100.0)),
        Dimension::Vh(vh) => Some(available * (vh / 100.0)),
        Dimension::Rem(rem) => Some(rem * 16.0),
        Dimension::Em(em) => Some(em * 16.0),
        Dimension::Calc(id) => {
            strata_style::calc::get_calc(id).map(|expr| {
                let ctx = strata_style::calc::CalcContext {
                    parent_size: available,
                    root_font_size: 16.0,
                    current_font_size: 16.0,
                    viewport_width: 1920.0,
                    viewport_height: 1080.0,
                };
                expr.eval(&ctx)
            })
        }
        Dimension::MinContent | Dimension::MaxContent | Dimension::FitContent(_) => None,
    }
}

/// Clamps a computed size against `min` and `max` constraints.
pub fn clamp_dimension(size: f32, min: Dimension, max: Dimension, available: f32) -> f32 {
    let mut clamped = size;
    if let Some(min_px) = resolve_dimension(min, available) {
        clamped = clamped.max(min_px);
    }
    if let Some(max_px) = resolve_dimension(max, available) {
        clamped = clamped.min(max_px);
    }
    clamped.max(0.0)
}

/// Converts a ComputedStyle into an InlineTextStyle for inline text shaping and typography.
pub fn inline_style_from_computed_style(style: &strata_style::types::ComputedStyle) -> crate::typography::InlineTextStyle {
    let font_family_str = strata_primitives::atom::resolve(style.font_family)
        .unwrap_or_else(|| "system-ui, sans-serif".to_string());
    let is_monospace = style.font_family == strata_primitives::AtomId::FONT_MONO
        || font_family_str.contains("mono")
        || font_family_str.contains("monospace")
        || font_family_str.contains("Courier")
        || font_family_str.contains("Consolas")
        || font_family_str.contains("DejaVu Sans Mono");

    crate::typography::InlineTextStyle {
        font_family: font_family_str,
        font_size: style.font_size,
        font_weight: style.font_weight,
        font_style: style.font_style,
        color: style.color,
        background_color: style.background_color,
        text_decoration: style.text_decoration,
        vertical_align: match style.vertical_align {
            strata_style::types::VerticalAlign::Baseline => crate::typography::VerticalAlignment::Baseline,
            strata_style::types::VerticalAlign::Top => crate::typography::VerticalAlignment::Top,
            strata_style::types::VerticalAlign::Bottom => crate::typography::VerticalAlignment::Bottom,
            strata_style::types::VerticalAlign::Middle => crate::typography::VerticalAlignment::Middle,
            strata_style::types::VerticalAlign::Sub => crate::typography::VerticalAlignment::Sub,
            strata_style::types::VerticalAlign::Super => crate::typography::VerticalAlignment::Super,
            strata_style::types::VerticalAlign::TextTop => crate::typography::VerticalAlignment::TextTop,
            strata_style::types::VerticalAlign::TextBottom => crate::typography::VerticalAlignment::TextBottom,
        },
        letter_spacing: style.letter_spacing,
        line_height: Some(style.font_size * style.line_height),
        is_monospace,
        href: style.get_custom_property(strata_primitives::AtomId::HREF)
            .and_then(strata_primitives::atom::resolve),
    }
}

/// Returns whether a child layout node is an inline phrasing element.
pub fn is_inline_phrasing_node(child: &LayoutNode) -> bool {
    matches!(
        child.style.display,
        strata_style::types::Display::Inline
            | strata_style::types::Display::InlineBlock
            | strata_style::types::Display::None
    ) || child.text.is_some()
}

/// Recursively flattens inline phrasing children into an InlineItem stream.
fn collect_inline_items_from_children(
    children: &[LayoutNode],
    parent_style: Option<&strata_style::types::ComputedStyle>,
    items: &mut Vec<crate::typography::InlineItem>,
    content_width: f32,
    available_height: f32,
) {
    for (idx, child) in children.iter().enumerate() {
        if child.style.display == strata_style::types::Display::None {
            continue;
        }
        if let Some(ref text) = child.text {
            let mut computed_style = child.style.clone();
            if let Some(p) = parent_style {
                if computed_style.font_family == strata_primitives::AtomId::FONT_SANS && p.font_family != strata_primitives::AtomId::FONT_SANS {
                    computed_style.font_family = p.font_family;
                }
            }
            let mut s = inline_style_from_computed_style(&computed_style);
            if s.background_color.is_none() {
                if let Some(p) = parent_style {
                    s.background_color = p.background_color;
                }
            }
            if s.text_decoration == strata_style::types::TextDecoration::None {
                if let Some(p) = parent_style {
                    if p.text_decoration != strata_style::types::TextDecoration::None {
                        s.text_decoration = p.text_decoration;
                    }
                }
            }
            if s.href.is_none() {
                if let Some(p) = parent_style {
                    s.href = p.get_custom_property(strata_primitives::AtomId::HREF)
                        .and_then(strata_primitives::atom::resolve);
                }
            }
            items.push(crate::typography::InlineItem::text(
                text.clone(),
                s,
            ));
        } else if child.style.display == strata_style::types::Display::Inline && !child.children.is_empty() {
            collect_inline_items_from_children(&child.children, Some(&child.style), items, content_width, available_height);
        } else {
            let mut w = resolve_dimension(child.style.width, content_width).unwrap_or(0.0);
            let mut h = resolve_dimension(child.style.height, available_height).unwrap_or(0.0);
            if w == 0.0 && child.geometry.border_box.width > 0.0 {
                w = child.geometry.border_box.width;
            }
            if h == 0.0 && child.geometry.border_box.height > 0.0 {
                h = child.geometry.border_box.height;
            }
            items.push(crate::typography::InlineItem::atomic(
                idx,
                w,
                h,
                child.style.margin.left,
                child.style.margin.right,
                crate::typography::VerticalAlignment::Baseline,
            ));
        }
    }
}

/// Recursively translates layout geometry and line box fragments of a node and its descendants.
pub fn translate_node_and_descendants(node: &mut LayoutNode, dx: f32, dy: f32) {
    if dx == 0.0 && dy == 0.0 {
        return;
    }
    node.geometry.translate(dx, dy);
    for line in &mut node.line_boxes {
        line.bounds.x += dx;
        line.bounds.y += dy;
        for frag in &mut line.text_fragments {
            frag.bounds.x += dx;
            frag.bounds.y += dy;
            frag.baseline_y += dy;
        }
        for at in &mut line.atomic_fragments {
            at.bounds.x += dx;
            at.bounds.y += dy;
        }
    }
    for child in &mut node.children {
        translate_node_and_descendants(child, dx, dy);
    }
}

/// Positions in-flow block children vertically with CSS 2.1 §8.3.1 margin collapsing, IFC, and floats.
pub fn solve_block_children(node: &mut LayoutNode, content_width: f32, available_height: f32) -> f32 {
    solve_block_children_with_exclusions(node, content_width, available_height, None, 0.0)
}

/// Positions in-flow block children with active exclusion space tracking and float dispatch.
pub fn solve_block_children_with_exclusions(
    node: &mut LayoutNode,
    content_width: f32,
    available_height: f32,
    parent_exclusions: Option<&crate::exclusions::ExclusionSpace>,
    bfc_offset_y: f32,
) -> f32 {
    let padding = node.style.padding;
    let border = node.style.border_width;

    // Check if this container establishes an Inline Formatting Context (IFC)
    let has_explicit_inlines = !node.inline_items.is_empty();
    let has_implicit_inlines = node.inline_items.is_empty()
        && !node.children.is_empty()
        && node.children.iter().all(is_inline_phrasing_node);

    if has_explicit_inlines || has_implicit_inlines {
        if has_implicit_inlines {
            for child in &mut node.children {
                if child.style.display != strata_style::types::Display::Inline && child.text.is_none() {
                    child.compute_layout(content_width, available_height);
                }
            }
            let mut items = Vec::new();
            collect_inline_items_from_children(&node.children, None, &mut items, content_width, available_height);
            node.inline_items = items;
        }

        let ifc = crate::typography::InlineFormattingContext::new();
        let opt = crate::typography::InlineLayoutOptions {
            font_size: node.style.font_size,
            line_height: Some(node.style.font_size * node.style.line_height),
            max_width: Some(content_width),
            white_space: match node.style.white_space {
                strata_style::types::WhiteSpace::Normal => crate::typography::WhiteSpaceMode::Normal,
                strata_style::types::WhiteSpace::NoWrap => crate::typography::WhiteSpaceMode::NoWrap,
                strata_style::types::WhiteSpace::Pre => crate::typography::WhiteSpaceMode::Pre,
                strata_style::types::WhiteSpace::PreWrap => crate::typography::WhiteSpaceMode::PreWrap,
                strata_style::types::WhiteSpace::PreLine => crate::typography::WhiteSpaceMode::PreLine,
            },
            vertical_align: match node.style.vertical_align {
                strata_style::types::VerticalAlign::Baseline => crate::typography::VerticalAlignment::Baseline,
                strata_style::types::VerticalAlign::Top => crate::typography::VerticalAlignment::Top,
                strata_style::types::VerticalAlign::Bottom => crate::typography::VerticalAlignment::Bottom,
                strata_style::types::VerticalAlign::Middle => crate::typography::VerticalAlignment::Middle,
                strata_style::types::VerticalAlign::Sub => crate::typography::VerticalAlignment::Sub,
                strata_style::types::VerticalAlign::Super => crate::typography::VerticalAlignment::Super,
                strata_style::types::VerticalAlign::TextTop => crate::typography::VerticalAlignment::TextTop,
                strata_style::types::VerticalAlign::TextBottom => crate::typography::VerticalAlignment::TextBottom,
            },
            is_monospace: crate::text::is_monospace_family(node.style.font_family),
            exclusion_space: parent_exclusions,
            bfc_offset_y: bfc_offset_y + padding.top + border.top,
        };

        let res = ifc.layout_inline_stream(&node.inline_items, &opt);
        let content_origin_x = node.geometry.border_box.x + border.left + padding.left;
        let content_origin_y = node.geometry.border_box.y + border.top + padding.top;

        let mut positioned_lines = res.lines;
        for line in &mut positioned_lines {
            let align_offset_x = match node.style.text_align {
                strata_style::types::TextAlign::Center => {
                    ((content_width - line.bounds.width) / 2.0).max(0.0)
                }
                strata_style::types::TextAlign::Right => {
                    (content_width - line.bounds.width).max(0.0)
                }
                _ => 0.0,
            };

            let total_shift_x = content_origin_x + align_offset_x;
            line.bounds.x += total_shift_x;
            line.bounds.y += content_origin_y;
            for frag in &mut line.text_fragments {
                frag.bounds.x += total_shift_x;
                frag.bounds.y += content_origin_y;
                frag.baseline_y += content_origin_y;
            }
            for at in &mut line.atomic_fragments {
                at.bounds.x += total_shift_x;
                at.bounds.y += content_origin_y;
                if at.node_index < node.children.len() {
                    let child = &mut node.children[at.node_index];
                    let dx = at.bounds.x - child.geometry.border_box.x;
                    let dy = at.bounds.y - child.geometry.border_box.y;
                    child.geometry.border_box.width = at.bounds.width;
                    child.geometry.border_box.height = at.bounds.height;
                    child.translate(dx, dy);
                }
            }
        }

        let total_h = res.total_height;
        node.line_boxes = positioned_lines;
        return total_h + padding.top + padding.bottom + border.top + border.bottom;
    }

    // Block formatting context maintaining local active float exclusions
    let mut exclusion_space = if let Some(parent) = parent_exclusions {
        let mut local = parent.clone();
        local.translate(0.0, -bfc_offset_y);
        local
    } else {
        crate::exclusions::ExclusionSpace::new()
    };
    let initial_exclusions_count = exclusion_space.exclusions().len();
    let mut current_y = border.top + padding.top;
    let mut prev_bottom_margin: Option<f32> = None;

    for child in &mut node.children {
        if child.style.position == Position::Absolute || child.style.position == Position::Fixed {
            continue;
        }

        // Handle CSS clear
        if child.style.clear != strata_style::types::Clear::None {
            let cleared_y = exclusion_space.clearance_offset(child.style.clear, current_y);
            if cleared_y > current_y {
                current_y = cleared_y;
                prev_bottom_margin = None;
            }
        }

        // Handle CSS float
        if child.style.float != strata_style::types::Float::None {
            // Layout float child to compute its dimensions
            child.compute_layout(content_width, available_height);
            let float_margin = child.style.margin;
            let float_w = child.geometry.border_box.width + float_margin.left + float_margin.right;
            let float_h = child.geometry.border_box.height + float_margin.top + float_margin.bottom;

            // Query line opportunity at current_y
            let mut opp = exclusion_space.line_opportunity(current_y, float_h, content_width);
            if opp.available_width < float_w && content_width >= float_w {
                // Drop down if float does not fit horizontally
                while let Some(next_y) = exclusion_space.next_float_bottom_after(current_y) {
                    current_y = next_y;
                    opp = exclusion_space.line_opportunity(current_y, float_h, content_width);
                    if opp.available_width >= float_w {
                        break;
                    }
                }
            }

            let (target_x, target_y) = match child.style.float {
                strata_style::types::Float::Left | strata_style::types::Float::InlineStart => {
                    let float_x = border.left + padding.left + opp.left_float_edge + float_margin.left;
                    let float_y = current_y + float_margin.top;
                    let excl_rect = crate::geometry::Rect::from_xywh(opp.left_float_edge, current_y, float_w, float_h);
                    exclusion_space.add_exclusion(excl_rect, crate::exclusions::ExclusionKind::FloatLeft);
                    (node.geometry.border_box.x + float_x, node.geometry.border_box.y + float_y)
                }
                strata_style::types::Float::Right | strata_style::types::Float::InlineEnd => {
                    let start_r = opp.right_float_edge;
                    let float_x = border.left + padding.left + start_r - float_w + float_margin.left;
                    let float_y = current_y + float_margin.top;
                    let excl_rect = crate::geometry::Rect::from_xywh(start_r - float_w, current_y, float_w, float_h);
                    exclusion_space.add_exclusion(excl_rect, crate::exclusions::ExclusionKind::FloatRight);
                    (node.geometry.border_box.x + float_x, node.geometry.border_box.y + float_y)
                }
                strata_style::types::Float::None => unreachable!(),
            };

            let old_x = child.geometry.border_box.x;
            let old_y = child.geometry.border_box.y;
            let dx = target_x - old_x;
            let dy = target_y - old_y;
            translate_node_and_descendants(child, dx, dy);

            // Floats do not advance in-flow block cursor or participate in margin collapsing
            continue;
        }

        let child_margin = child.style.margin;

        let collapsed_v_margin = match prev_bottom_margin {
            Some(prev_m) => collapse_margins(prev_m, child_margin.top),
            None => child_margin.top,
        };

        current_y += collapsed_v_margin;

        let child_x = border.left + padding.left + child_margin.left;
        child.geometry.border_box.x = node.geometry.border_box.x + child_x;
        child.geometry.border_box.y = node.geometry.border_box.y + current_y;

        // If active exclusions exist, pass them down to in-flow block container (e.g. <p>)
        if !exclusion_space.is_empty() {
            child.compute_layout_with_exclusions(content_width, available_height, Some(&exclusion_space), current_y);
        } else {
            child.compute_layout(content_width, available_height);
        }

        // Auto margin horizontal centering (mx-auto / ml-auto / mr-auto)
        if child.style.margin_auto_left && child.style.margin_auto_right {
            let free_space = content_width - child.geometry.border_box.width;
            let auto_m = (free_space / 2.0).max(0.0);
            let target_x = node.geometry.border_box.x + border.left + padding.left + auto_m;
            let dx = target_x - child.geometry.border_box.x;
            if dx != 0.0 {
                translate_node_and_descendants(child, dx, 0.0);
            }
        } else if child.style.margin_auto_left {
            let free_space = content_width - child.geometry.border_box.width - child_margin.right;
            let target_x = node.geometry.border_box.x + border.left + padding.left + free_space.max(0.0);
            let dx = target_x - child.geometry.border_box.x;
            if dx != 0.0 {
                translate_node_and_descendants(child, dx, 0.0);
            }
        } else if node.style.text_align == strata_style::types::TextAlign::Center {
            let free_space = content_width - child.geometry.border_box.width;
            if free_space > 0.0 {
                let offset_x = (free_space / 2.0).max(0.0);
                let target_x = node.geometry.border_box.x + border.left + padding.left + offset_x;
                let dx = target_x - child.geometry.border_box.x;
                if dx != 0.0 {
                    translate_node_and_descendants(child, dx, 0.0);
                }
            }
        } else if node.style.text_align == strata_style::types::TextAlign::Right {
            let free_space = content_width - child.geometry.border_box.width;
            if free_space > 0.0 {
                let target_x = node.geometry.border_box.x + border.left + padding.left + free_space;
                let dx = target_x - child.geometry.border_box.x;
                if dx != 0.0 {
                    translate_node_and_descendants(child, dx, 0.0);
                }
            }
        }

        current_y += child.geometry.border_box.height;
        prev_bottom_margin = Some(child_margin.bottom);
    }

    if let Some(last_margin) = prev_bottom_margin {
        current_y += last_margin;
    }

    // CSS 2.1 §10.6.7 BFC float containment (Clearfix)
    if exclusion_space.exclusions().len() > initial_exclusions_count {
        let max_float_bottom = exclusion_space.max_float_bottom();
        if max_float_bottom > current_y {
            current_y = max_float_bottom;
        }
    }

    current_y += padding.bottom + border.bottom;

    current_y
}

/// Executes normal-flow block layout with vertical margin collapsing.
pub fn solve_block(node: &mut LayoutNode, available_width: f32, available_height: f32) {
    solve_block_with_exclusions(node, available_width, available_height, None, 0.0);
}

/// Executes normal-flow block layout passing parent exclusion space for float obstructions.
pub fn solve_block_with_exclusions(
    node: &mut LayoutNode,
    available_width: f32,
    available_height: f32,
    parent_exclusions: Option<&crate::exclusions::ExclusionSpace>,
    bfc_offset_y: f32,
) {
    let padding = node.style.padding;
    let border = node.style.border_width;
    let margin = node.style.margin;

    // Content-Visibility: Hidden virtualization (W3C CSS Containment Level 2)
    if node.style.content_visibility == strata_style::types::ContentVisibility::Hidden {
        let intrinsic_w = node.style.contain_intrinsic_width
            .and_then(|dim| resolve_dimension(dim, available_width));
        let intrinsic_h = node.style.contain_intrinsic_height
            .and_then(|dim| resolve_dimension(dim, available_height))
            .unwrap_or(0.0);
        let border_box_width = match resolve_dimension(node.style.width, available_width).or(intrinsic_w) {
            Some(w) => clamp_dimension(w, node.style.min_width, node.style.max_width, available_width),
            None => (available_width - margin.left - margin.right).max(0.0),
        };
        let border_box_height = match resolve_dimension(node.style.height, available_height) {
            Some(h) => clamp_dimension(h, node.style.min_height, node.style.max_height, available_height),
            None => clamp_dimension(intrinsic_h, node.style.min_height, node.style.max_height, available_height),
        };
        let border_box = Rect::new(node.geometry.border_box.x, node.geometry.border_box.y, border_box_width, border_box_height);
        node.geometry = LayoutGeometry::from_border_box(border_box, padding, border, margin);
        node.geometry.snap_to_pixel();
        return;
    }

    let (text_w, text_h) = if !node.inline_items.is_empty() {
        let ifc = crate::typography::InlineFormattingContext::new();
        let opt = crate::typography::InlineLayoutOptions {
            font_size: node.style.font_size,
            line_height: Some(node.style.font_size * node.style.line_height),
            max_width: Some(available_width),
            white_space: match node.style.white_space {
                strata_style::types::WhiteSpace::Normal => crate::typography::WhiteSpaceMode::Normal,
                strata_style::types::WhiteSpace::NoWrap => crate::typography::WhiteSpaceMode::NoWrap,
                strata_style::types::WhiteSpace::Pre => crate::typography::WhiteSpaceMode::Pre,
                strata_style::types::WhiteSpace::PreWrap => crate::typography::WhiteSpaceMode::PreWrap,
                strata_style::types::WhiteSpace::PreLine => crate::typography::WhiteSpaceMode::PreLine,
            },
            vertical_align: crate::typography::VerticalAlignment::Baseline,
            is_monospace: crate::text::is_monospace_family(node.style.font_family),
            exclusion_space: parent_exclusions,
            bfc_offset_y,
        };
        let res = ifc.layout_inline_stream(&node.inline_items, &opt);
        (res.total_width, res.total_height)
    } else if let Some(ref text) = node.text {
        let is_mono = crate::text::is_monospace_family(node.style.font_family);
        crate::text::measure_text_bounds_ext(text, node.style.font_size, node.style.line_height, Some(available_width), is_mono)
    } else {
        (0.0, 0.0)
    };

    // 1. Resolve element width
    let resolved_width = match resolve_dimension(node.style.width, available_width) {
        Some(w) => clamp_dimension(w, node.style.min_width, node.style.max_width, available_width),
        None => {
            if node.text.is_some() || matches!(node.style.display, strata_style::types::Display::Inline | strata_style::types::Display::InlineBlock) {
                // Shrink-to-fit for text or inline elements
                let fit_w = text_w + padding.left + padding.right + border.left + border.right;
                clamp_dimension(fit_w, node.style.min_width, node.style.max_width, available_width)
            } else {
                // In CSS block layout, auto width fills available width minus margins
                let auto_w = (available_width - margin.left - margin.right).max(0.0);
                clamp_dimension(auto_w, node.style.min_width, node.style.max_width, available_width)
            }
        }
    };

    let border_box_width = match node.style.box_sizing {
        BoxSizing::BorderBox => resolved_width,
        BoxSizing::ContentBox => resolved_width + padding.left + padding.right + border.left + border.right,
    };

    let content_width = (border_box_width - padding.left - padding.right - border.left - border.right).max(0.0);

    // 2. Lay out in-flow children vertically with margin collapsing and floats
    let children_height = if node.children.is_empty() && node.text.is_some() {
        text_h + padding.top + padding.bottom + border.top + border.bottom
    } else {
        solve_block_children_with_exclusions(node, content_width, available_height, parent_exclusions, bfc_offset_y)
    };

    // 3. Resolve element height
    let resolved_height = match resolve_dimension(node.style.height, available_height) {
        Some(h) => {
            let clamped_h = clamp_dimension(h, node.style.min_height, node.style.max_height, available_height);
            match node.style.box_sizing {
                BoxSizing::BorderBox => clamped_h,
                BoxSizing::ContentBox => clamped_h + padding.top + padding.bottom + border.top + border.bottom,
            }
        }
        None => {
            // Auto height wraps children
            clamp_dimension(children_height, node.style.min_height, node.style.max_height, available_height)
        }
    };

    let border_box_height = resolved_height;

    // 4. Finalize concentric geometry boxes
    let border_box = Rect::new(
        node.geometry.border_box.x,
        node.geometry.border_box.y,
        border_box_width,
        border_box_height,
    );

    node.geometry = LayoutGeometry::from_border_box(border_box, padding, border, margin);
    node.geometry.snap_to_pixel();
}
