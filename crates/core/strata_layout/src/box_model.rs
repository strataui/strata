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

/// Positions in-flow block children vertically with CSS 2.1 §8.3.1 margin collapsing.
pub fn solve_block_children(node: &mut LayoutNode, content_width: f32, available_height: f32) -> f32 {
    let padding = node.style.padding;
    let border = node.style.border_width;
    let mut current_y = border.top + padding.top;
    let mut prev_bottom_margin: Option<f32> = None;

    for child in &mut node.children {
        if child.style.position == Position::Absolute || child.style.position == Position::Fixed {
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

        child.compute_layout(content_width, available_height);

        current_y += child.geometry.border_box.height;
        prev_bottom_margin = Some(child_margin.bottom);
    }

    if let Some(last_margin) = prev_bottom_margin {
        current_y += last_margin;
    }
    current_y += padding.bottom + border.bottom;

    current_y
}

/// Executes normal-flow block layout with vertical margin collapsing.
pub fn solve_block(node: &mut LayoutNode, available_width: f32, available_height: f32) {
    let padding = node.style.padding;
    let border = node.style.border_width;
    let margin = node.style.margin;

    let (text_w, text_h) = if let Some(ref text) = node.text {
        crate::text::measure_text_bounds(text, node.style.font_size, node.style.line_height, Some(available_width))
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

    // 2. Lay out in-flow children vertically with margin collapsing
    let children_height = if node.children.is_empty() && node.text.is_some() {
        text_h + padding.top + padding.bottom + border.top + border.bottom
    } else {
        solve_block_children(node, content_width, available_height)
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
