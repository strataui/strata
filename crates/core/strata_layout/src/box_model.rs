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
    let m1 = if m1.is_finite() {
        m1.clamp(-1e7, 1e7)
    } else {
        0.0
    };
    let m2 = if m2.is_finite() {
        m2.clamp(-1e7, 1e7)
    } else {
        0.0
    };
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
    const MAX_DIMENSION: f32 = 1e7;

    let avail = if available.is_finite() {
        available.clamp(-MAX_DIMENSION, MAX_DIMENSION)
    } else {
        0.0
    };

    match dim {
        Dimension::Auto => None,
        Dimension::Px(px) => {
            if px.is_finite() {
                Some(px.clamp(-MAX_DIMENSION, MAX_DIMENSION))
            } else {
                None
            }
        }
        Dimension::Percent(pct) => {
            if pct.is_finite() {
                let px = avail * (pct / 100.0);
                if px.is_finite() {
                    Some(px.clamp(-MAX_DIMENSION, MAX_DIMENSION))
                } else {
                    None
                }
            } else {
                None
            }
        }
        Dimension::Vw(vw) => {
            if vw.is_finite() {
                let px = avail * (vw / 100.0);
                if px.is_finite() {
                    Some(px.clamp(-MAX_DIMENSION, MAX_DIMENSION))
                } else {
                    None
                }
            } else {
                None
            }
        }
        Dimension::Vh(vh) => {
            if vh.is_finite() {
                let px = avail * (vh / 100.0);
                if px.is_finite() {
                    Some(px.clamp(-MAX_DIMENSION, MAX_DIMENSION))
                } else {
                    None
                }
            } else {
                None
            }
        }
        Dimension::Rem(rem) => {
            if rem.is_finite() {
                Some((rem * 16.0).clamp(-MAX_DIMENSION, MAX_DIMENSION))
            } else {
                None
            }
        }
        Dimension::Em(em) => {
            if em.is_finite() {
                Some((em * 16.0).clamp(-MAX_DIMENSION, MAX_DIMENSION))
            } else {
                None
            }
        }
        Dimension::Calc(id) => strata_style::calc::get_calc(id).and_then(|expr| {
            let ctx = strata_style::calc::CalcContext {
                parent_size: avail,
                root_font_size: 16.0,
                current_font_size: 16.0,
                viewport_width: 1920.0,
                viewport_height: 1080.0,
            };
            let val = expr.eval(&ctx);
            if val.is_finite() {
                Some(val.clamp(-MAX_DIMENSION, MAX_DIMENSION))
            } else {
                None
            }
        }),
        Dimension::MinContent | Dimension::MaxContent | Dimension::FitContent(_) => None,
    }
}

/// Clamps a computed size against `min` and `max` constraints.
pub fn clamp_dimension(size: f32, min: Dimension, max: Dimension, available: f32) -> f32 {
    let mut clamped = if size.is_finite() {
        size.clamp(0.0, 1e7)
    } else {
        0.0
    };
    if let Some(min_px) = resolve_dimension(min, available) {
        clamped = clamped.max(min_px.max(0.0));
    }
    if let Some(max_px) = resolve_dimension(max, available) {
        clamped = clamped.min(max_px.max(0.0));
    }
    clamped.clamp(0.0, 1e7)
}

/// Resolves a CSS `Dimension` into concrete pixels relative to an available length and node context,
/// evaluating intrinsic keywords (`min-content`, `max-content`, `fit-content`) via `compute_min_max_sizes`.
pub fn resolve_dimension_with_node(
    dim: Dimension,
    available: f32,
    node: &LayoutNode,
) -> Option<f32> {
    match dim {
        Dimension::MinContent => {
            let sizes = crate::intrinsic::compute_min_max_sizes(node, Some(available));
            let inline_insets = node.style.padding.left
                + node.style.padding.right
                + node.style.border_width.left
                + node.style.border_width.right;
            let val = match node.style.box_sizing {
                BoxSizing::BorderBox => sizes.min_content,
                BoxSizing::ContentBox => (sizes.min_content - inline_insets).max(0.0),
            };
            Some(val)
        }
        Dimension::MaxContent => {
            let sizes = crate::intrinsic::compute_min_max_sizes(node, Some(available));
            let inline_insets = node.style.padding.left
                + node.style.padding.right
                + node.style.border_width.left
                + node.style.border_width.right;
            let val = match node.style.box_sizing {
                BoxSizing::BorderBox => sizes.max_content,
                BoxSizing::ContentBox => (sizes.max_content - inline_insets).max(0.0),
            };
            Some(val)
        }
        Dimension::FitContent(arg) => {
            let sizes = crate::intrinsic::compute_min_max_sizes(node, Some(available));
            let inline_insets = node.style.padding.left
                + node.style.padding.right
                + node.style.border_width.left
                + node.style.border_width.right;
            let limit = if arg > 0.0 { arg } else { available };
            let fitted = sizes.shrink_to_fit(limit);
            let val = match node.style.box_sizing {
                BoxSizing::BorderBox => fitted,
                BoxSizing::ContentBox => (fitted - inline_insets).max(0.0),
            };
            Some(val)
        }
        other => resolve_dimension(other, available),
    }
}

/// Clamps a computed size against `min` and `max` constraints with node context.
pub fn clamp_dimension_with_node(
    size: f32,
    min: Dimension,
    max: Dimension,
    available: f32,
    node: &LayoutNode,
) -> f32 {
    let mut clamped = if size.is_finite() {
        size.clamp(0.0, 1e7)
    } else {
        0.0
    };
    if let Some(min_px) = resolve_dimension_with_node(min, available, node) {
        clamped = clamped.max(min_px.max(0.0));
    }
    if let Some(max_px) = resolve_dimension_with_node(max, available, node) {
        clamped = clamped.min(max_px.max(0.0));
    }
    clamped.clamp(0.0, 1e7)
}

/// Converts a ComputedStyle into an InlineTextStyle for inline text shaping and typography.
pub fn inline_style_from_computed_style(
    style: &strata_style::types::ComputedStyle,
) -> crate::typography::InlineTextStyle {
    let is_monospace = crate::text::is_monospace_family(style.font_family);

    crate::typography::InlineTextStyle {
        font_family: style.font_family,
        font_size: style.font_size,
        font_weight: style.font_weight,
        font_style: style.font_style,
        color: style.color,
        background_color: style.background_color,
        text_decoration: style.text_decoration,
        vertical_align: match style.vertical_align {
            strata_style::types::VerticalAlign::Baseline => {
                crate::typography::VerticalAlignment::Baseline
            }
            strata_style::types::VerticalAlign::Top => crate::typography::VerticalAlignment::Top,
            strata_style::types::VerticalAlign::Bottom => {
                crate::typography::VerticalAlignment::Bottom
            }
            strata_style::types::VerticalAlign::Middle => {
                crate::typography::VerticalAlignment::Middle
            }
            strata_style::types::VerticalAlign::Sub => crate::typography::VerticalAlignment::Sub,
            strata_style::types::VerticalAlign::Super => {
                crate::typography::VerticalAlignment::Super
            }
            strata_style::types::VerticalAlign::TextTop => {
                crate::typography::VerticalAlignment::TextTop
            }
            strata_style::types::VerticalAlign::TextBottom => {
                crate::typography::VerticalAlignment::TextBottom
            }
        },
        letter_spacing: style.letter_spacing,
        line_height: Some(style.font_size * style.line_height),
        is_monospace,
        href: style
            .get_custom_property(strata_primitives::AtomId::HREF)
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

/// Refreshes inline fragment paint styles after a style-only change.
///
/// Geometry and line boxes are preserved from a previous layout pass. When
/// hover toggles only paint properties (color, background, decoration), the
/// text runs still carry the previous inline style clones. This rebuilds the
/// inline item stream from the freshly styled children and patches each line
/// fragment's paint-only style fields by item index.
pub fn refresh_inline_fragment_styles(node: &mut LayoutNode) {
    let mut stack = vec![node];
    while let Some(curr) = stack.pop() {
        if !curr.line_boxes.is_empty() {
            if !curr.children.is_empty() {
                let mut items = Vec::new();
                collect_inline_items_from_children(
                    &curr.children,
                    None,
                    None,
                    &mut items,
                    curr.geometry.content_box.width,
                    curr.geometry.content_box.height,
                );
                curr.inline_items = items;
            }
            for line in &mut curr.line_boxes {
                for frag in &mut line.text_fragments {
                    if let Some(crate::typography::InlineItem::Text { style, .. }) =
                        curr.inline_items.get(frag.item_index)
                    {
                        frag.style.color = style.color;
                        frag.style.background_color = style.background_color;
                        frag.style.text_decoration = style.text_decoration;
                        frag.style.font_family = style.font_family;
                        frag.font_family = style.font_family;
                    } else if curr.inline_items.is_empty() && curr.text.is_some() {
                        let style = inline_style_from_computed_style(&curr.style);
                        frag.style.color = style.color;
                        frag.style.background_color = style.background_color;
                        frag.style.text_decoration = style.text_decoration;
                        frag.style.font_family = style.font_family;
                        frag.font_family = style.font_family;
                    }
                }
            }
        }
        for child in &mut curr.children {
            stack.push(child);
        }
    }
}

/// Recursively clears line boxes from inline phrasing descendants whose text is absorbed by parent IFC using an iterative heap stack.
pub fn clear_inline_descendant_line_boxes(node: &mut LayoutNode) {
    if node.style.display != strata_style::types::Display::Inline {
        return;
    }
    let mut stack = vec![node];
    while let Some(curr) = stack.pop() {
        if curr.style.display == strata_style::types::Display::Inline {
            curr.line_boxes.clear();
            for child in &mut curr.children {
                stack.push(child);
            }
        }
    }
}

/// Recursively flattens inline phrasing children into an InlineItem stream bounded by MAX_LAYOUT_DEPTH.
fn collect_inline_items_from_children(
    children: &[LayoutNode],
    parent_style: Option<&strata_style::types::ComputedStyle>,
    top_level_idx: Option<usize>,
    items: &mut Vec<crate::typography::InlineItem>,
    content_width: f32,
    available_height: f32,
) {
    collect_inline_items_from_children_guarded(
        children,
        parent_style,
        top_level_idx,
        items,
        content_width,
        available_height,
        0,
    );
}

fn collect_inline_items_from_children_guarded(
    children: &[LayoutNode],
    parent_style: Option<&strata_style::types::ComputedStyle>,
    top_level_idx: Option<usize>,
    items: &mut Vec<crate::typography::InlineItem>,
    content_width: f32,
    available_height: f32,
    depth: usize,
) {
    if depth > crate::tree::MAX_LAYOUT_DEPTH {
        return;
    }
    let _prof = crate::prof::Scope::new(crate::prof::Slot::StyleClone);
    for (idx, child) in children.iter().enumerate() {
        if child.style.display == strata_style::types::Display::None {
            continue;
        }
        let effective_top_idx = top_level_idx.unwrap_or(idx);
        if let Some(ref text) = child.text {
            let mut computed_style = child.style.clone();
            if let Some(p) = parent_style {
                if computed_style.font_family == strata_primitives::AtomId::FONT_SANS
                    && p.font_family != strata_primitives::AtomId::FONT_SANS
                {
                    computed_style.font_family = p.font_family;
                }
            }
            let mut s = inline_style_from_computed_style(&computed_style);
            if s.background_color.is_none() {
                if let Some(p) = parent_style {
                    if p.display == strata_style::types::Display::Inline {
                        s.background_color = p.background_color;
                    }
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
                    s.href = p
                        .get_custom_property(strata_primitives::AtomId::HREF)
                        .and_then(strata_primitives::atom::resolve);
                }
            }
            items.push(crate::typography::InlineItem::text(text.clone(), s));
        } else if child.style.display == strata_style::types::Display::Inline
            && !child.children.is_empty()
        {
            let mut merged_style = child.style.clone();
            if let Some(p) = parent_style {
                if merged_style.background_color.is_none()
                    && p.display == strata_style::types::Display::Inline
                {
                    merged_style.background_color = p.background_color;
                }
                if merged_style.text_decoration == strata_style::types::TextDecoration::None {
                    merged_style.text_decoration = p.text_decoration;
                }
                if merged_style
                    .get_custom_property(strata_primitives::AtomId::HREF)
                    .is_none()
                {
                    if let Some(h) = p.get_custom_property(strata_primitives::AtomId::HREF) {
                        merged_style.set_custom_property(strata_primitives::AtomId::HREF, h);
                    }
                }
            }
            collect_inline_items_from_children_guarded(
                &child.children,
                Some(&merged_style),
                Some(effective_top_idx),
                items,
                content_width,
                available_height,
                depth + 1,
            );
        } else {
            let mut w =
                resolve_dimension_with_node(child.style.width, content_width, child).unwrap_or(0.0);
            let mut h = resolve_dimension(child.style.height, available_height).unwrap_or(0.0);
            if w == 0.0 && child.geometry.border_box.width > 0.0 {
                w = child.geometry.border_box.width;
            }
            if h == 0.0 && child.geometry.border_box.height > 0.0 {
                h = child.geometry.border_box.height;
            }
            if h == 0.0 && w > 0.0 && child.children.is_empty() {
                // Under W3C CSS 2.1 §10.6.2, replaced/atomic elements with auto height default to 1:1 aspect ratio
                h = w;
            }
            items.push(crate::typography::InlineItem::atomic(
                effective_top_idx,
                w,
                h,
                child.style.margin.left,
                child.style.margin.right,
                crate::typography::VerticalAlignment::Baseline,
            ));
        }
    }
}

/// Recursively propagates atomic inline fragment bounds down through nested `Display::Inline` containers using an iterative heap stack.
fn propagate_atomic_bounds_to_inline_descendants(node: &mut LayoutNode, bounds: Rect) {
    if node.style.display != strata_style::types::Display::Inline {
        return;
    }
    let mut stack = vec![node];
    while let Some(curr) = stack.pop() {
        for child in &mut curr.children {
            child.geometry.border_box.x = bounds.x;
            child.geometry.border_box.y = bounds.y;
            if child.geometry.border_box.width == 0.0 {
                child.geometry.border_box.width = bounds.width;
            }
            if child.geometry.border_box.height == 0.0 {
                child.geometry.border_box.height = bounds.height;
            }
            if child.style.display == strata_style::types::Display::Inline {
                stack.push(child);
            }
        }
    }
}

/// Recursively translates layout geometry and line box fragments of a node and its descendants.
pub fn translate_node_and_descendants(node: &mut LayoutNode, dx: f32, dy: f32) {
    if dx == 0.0 && dy == 0.0 {
        return;
    }
    node.translate(dx, dy);
}

/// Applies `text-overflow: ellipsis` to the laid-out inline stream.
///
/// Only clipping containers are eligible. When a line's text extends past the
/// content width, the fragment that crosses the boundary is cut at the last
/// character boundary that leaves room for U+2026 HORIZONTAL ELLIPSIS, and the
/// remaining fragments on that line are discarded. This mirrors the single
/// line truncation produced by `overflow: hidden; text-overflow: ellipsis;
/// white-space: nowrap`.
fn truncate_inline_overflow(
    node: &LayoutNode,
    res: &mut crate::typography::InlineLayoutResult,
    content_width: f32,
) {
    use strata_style::types::{Overflow, TextOverflow};

    if node.style.text_overflow != TextOverflow::Ellipsis
        || node.style.overflow_x == Overflow::Visible
        || content_width <= 0.0
    {
        return;
    }

    for line in &mut res.lines {
        let mut new_fragments = Vec::with_capacity(line.text_fragments.len());
        let mut cut = false;
        for fragment in line.text_fragments.drain(..) {
            if cut {
                continue;
            }
            let frag_start = fragment.bounds.x;
            let frag_end = frag_start + fragment.bounds.width;
            if frag_end <= content_width + 1.0 {
                new_fragments.push(fragment);
                continue;
            }

            let available = content_width - frag_start;
            if available <= 0.0 {
                cut = true;
                continue;
            }

            let style = &fragment.style;
            let multiplier = match style.line_height {
                Some(line_h) if style.font_size > 0.0 => (line_h / style.font_size).max(0.01),
                _ => 1.2,
            };
            let measure = |text: &str| {
                crate::text::measure_text_bounds_styled_with_family(
                    text,
                    style.font_size,
                    multiplier,
                    None,
                    style.is_monospace,
                    style.font_family,
                    style.font_weight,
                    style.font_style,
                )
                .0
            };

            let ellipsis_width = measure("\u{2026}");
            if ellipsis_width > available {
                cut = true;
                continue;
            }

            // Binary search the largest character-boundary prefix that fits
            // alongside the ellipsis within the available width.
            let boundaries: Vec<usize> = fragment
                .text
                .char_indices()
                .map(|(idx, _)| idx)
                .chain(std::iter::once(fragment.text.len()))
                .collect();
            let mut low = 0usize;
            let mut high = boundaries.len().saturating_sub(1);
            let mut best = 0usize;
            while low <= high {
                let mid = (low + high) / 2;
                let end = boundaries[mid];
                let prefix = &fragment.text[..end];
                if measure(prefix) + ellipsis_width <= available + 0.01 {
                    best = mid;
                    low = mid + 1;
                } else if mid == 0 {
                    break;
                } else {
                    high = mid - 1;
                }
            }

            let end = boundaries[best];
            let mut text = fragment.text[..end].to_string();
            text.push('\u{2026}');
            let width = measure(&text);
            let mut truncated_fragment = fragment;
            truncated_fragment.text = text;
            truncated_fragment.bounds.width = width;
            new_fragments.push(truncated_fragment);
            cut = true;
        }

        if let Some(last) = new_fragments.last() {
            line.bounds.width = last.bounds.x + last.bounds.width;
        } else {
            line.bounds.width = 0.0;
        }
        line.text_fragments = new_fragments;
    }

    res.total_width = res
        .lines
        .iter()
        .map(|line| line.bounds.width)
        .fold(0.0f32, f32::max);
}

/// Positions in-flow block children vertically with CSS 2.1 §8.3.1 margin collapsing, IFC, and floats.
pub fn solve_block_children(
    node: &mut LayoutNode,
    content_width: f32,
    available_height: f32,
) -> f32 {
    solve_block_children_with_exclusions(node, content_width, available_height, None, 0.0)
}

/// Positions in-flow block children with active exclusion space tracking and float dispatch.
pub fn solve_block_children_with_exclusions(
    node: &mut LayoutNode,
    content_width: f32,
    available_height: f32,
    parent_exclusions: Option<&mut crate::exclusions::ExclusionSpace>,
    bfc_offset_y: f32,
) -> f32 {
    solve_block_children_with_exclusions_guarded(
        node,
        content_width,
        available_height,
        parent_exclusions,
        bfc_offset_y,
        crate::tree::LayoutDepthGuard::root(),
    )
}

/// Returns true if an in-flow block element has zero computed height, no padding or border,
/// and no in-flow content, allowing its top and bottom margins to collapse through under CSS 2.1 §8.3.1.
#[inline]
pub fn is_empty_collapsible_block(node: &LayoutNode) -> bool {
    node.children.is_empty()
        && node.text.is_none()
        && node.inline_items.is_empty()
        && node.style.border_width.top == 0.0
        && node.style.border_width.bottom == 0.0
        && node.style.padding.top == 0.0
        && node.style.padding.bottom == 0.0
        && matches!(node.style.height, Dimension::Auto | Dimension::Px(0.0))
        && matches!(node.style.min_height, Dimension::Auto | Dimension::Px(0.0))
        && node.style.clear == strata_style::types::Clear::None
        && node.style.float == strata_style::types::Float::None
        && node.style.overflow_x == strata_style::types::Overflow::Visible
        && node.style.overflow_y == strata_style::types::Overflow::Visible
}

/// Returns true if an element establishes an independent formatting context (BFC, FFC, GFC),
/// preventing margin collapsing across its boundaries under W3C CSS 2.1 §8.3.1 and CSS Display Level 3.
#[inline]
pub fn node_establishes_bfc(node: &LayoutNode) -> bool {
    node.style.float != strata_style::types::Float::None
        || matches!(node.style.position, Position::Absolute | Position::Fixed)
        || matches!(
            node.style.display,
            strata_style::types::Display::InlineBlock
                | strata_style::types::Display::TableCell
                | strata_style::types::Display::TableCaption
                | strata_style::types::Display::Flex
                | strata_style::types::Display::Grid
                | strata_style::types::Display::InlineFlex
                | strata_style::types::Display::InlineGrid
                | strata_style::types::Display::Table
                | strata_style::types::Display::InlineTable
        )
        || node.style.overflow_x != strata_style::types::Overflow::Visible
        || node.style.overflow_y != strata_style::types::Overflow::Visible
        || node.style.contain.layout
        || child_is_layout_boundary(node)
}

#[inline]
fn child_is_layout_boundary(node: &LayoutNode) -> bool {
    node.is_layout_boundary()
}

/// Positions in-flow block children with active exclusion space tracking and depth guard.
pub fn solve_block_children_with_exclusions_guarded(
    node: &mut LayoutNode,
    content_width: f32,
    available_height: f32,
    parent_exclusions: Option<&mut crate::exclusions::ExclusionSpace>,
    bfc_offset_y: f32,
    guard: crate::tree::LayoutDepthGuard,
) -> f32 {
    let _prof = crate::prof::Scope::new(crate::prof::Slot::Block);
    let padding = node.style.padding;
    let border = node.style.border_width;

    if guard.is_exhausted() {
        crate::tree::record_layout_overflow(guard.current_depth(), guard.max_depth());
        return padding.top + border.top + padding.bottom + border.bottom;
    }

    // A text leaf that clips with `text-overflow: ellipsis` owns its own inline
    // formatting context. Text leaves normally inherit line boxes from an
    // ancestor IFC, but a text leaf that is itself a flex item (or otherwise
    // formatted independently) has no ancestor IFC to truncate its glyphs.
    if node.children.is_empty()
        && node.inline_items.is_empty()
        && node.style.text_overflow == strata_style::types::TextOverflow::Ellipsis
    {
        if let Some(text) = node.text.clone() {
            let style = inline_style_from_computed_style(&node.style);
            let items = vec![crate::typography::InlineItem::text(text, style)];
            let opt = crate::typography::InlineLayoutOptions {
                font_size: node.style.font_size,
                line_height: Some(node.style.font_size * node.style.line_height),
                max_width: Some(content_width),
                white_space: match node.style.white_space {
                    strata_style::types::WhiteSpace::Normal => {
                        crate::typography::WhiteSpaceMode::Normal
                    }
                    strata_style::types::WhiteSpace::NoWrap => {
                        crate::typography::WhiteSpaceMode::NoWrap
                    }
                    strata_style::types::WhiteSpace::Pre => crate::typography::WhiteSpaceMode::Pre,
                    strata_style::types::WhiteSpace::PreWrap => {
                        crate::typography::WhiteSpaceMode::PreWrap
                    }
                    strata_style::types::WhiteSpace::PreLine => {
                        crate::typography::WhiteSpaceMode::PreLine
                    }
                },
                vertical_align: crate::typography::VerticalAlignment::Baseline,
                is_monospace: crate::text::is_monospace_family(node.style.font_family),
                exclusion_space: None,
                bfc_offset_y: 0.0,
            };
            let ifc = crate::typography::InlineFormattingContext::new();
            let mut res = ifc.layout_inline_stream(&items, &opt);
            truncate_inline_overflow(node, &mut res, content_width);

            let origin_x = node.geometry.border_box.x + border.left + padding.left;
            let origin_y = node.geometry.border_box.y + border.top + padding.top;
            let mut lines = res.lines;
            for line in &mut lines {
                line.bounds.x += origin_x;
                line.bounds.y += origin_y;
                for frag in &mut line.text_fragments {
                    frag.bounds.x += origin_x;
                    frag.bounds.y += origin_y;
                    frag.baseline_y += origin_y;
                }
                for atomic in &mut line.atomic_fragments {
                    atomic.bounds.x += origin_x;
                    atomic.bounds.y += origin_y;
                }
            }
            let total_h = res.total_height;
            node.inline_items = items;
            node.line_boxes = lines;
            return total_h + padding.top + padding.bottom + border.top + border.bottom;
        }
    }

    // Check if this container establishes an Inline Formatting Context (IFC)
    let has_implicit_inlines =
        !node.children.is_empty() && node.children.iter().all(is_inline_phrasing_node);
    let has_explicit_inlines = node.children.is_empty() && !node.inline_items.is_empty();

    if has_explicit_inlines || has_implicit_inlines {
        if has_implicit_inlines {
            let child_guard = guard.enter();
            for child in &mut node.children {
                // Measure every phrasing child, including text leaves. Text
                // leaves used to be skipped here because their glyph runs are
                // absorbed into the parent inline formatting context, which
                // left their physical `border_box` at 0x0.
                if let Some(cg) = child_guard {
                    child.compute_layout_guarded(content_width, available_height, None, 0.0, cg);
                } else {
                    child.compute_layout_guarded(content_width, available_height, None, 0.0, guard);
                }
                clear_inline_descendant_line_boxes(child);
            }
            let mut items = Vec::new();
            collect_inline_items_from_children(
                &node.children,
                None,
                None,
                &mut items,
                content_width,
                available_height,
            );
            node.inline_items = items;
        }

        let ifc = crate::typography::InlineFormattingContext::new();
        let opt = crate::typography::InlineLayoutOptions {
            font_size: node.style.font_size,
            line_height: Some(node.style.font_size * node.style.line_height),
            max_width: Some(content_width),
            white_space: match node.style.white_space {
                strata_style::types::WhiteSpace::Normal => {
                    crate::typography::WhiteSpaceMode::Normal
                }
                strata_style::types::WhiteSpace::NoWrap => {
                    crate::typography::WhiteSpaceMode::NoWrap
                }
                strata_style::types::WhiteSpace::Pre => crate::typography::WhiteSpaceMode::Pre,
                strata_style::types::WhiteSpace::PreWrap => {
                    crate::typography::WhiteSpaceMode::PreWrap
                }
                strata_style::types::WhiteSpace::PreLine => {
                    crate::typography::WhiteSpaceMode::PreLine
                }
            },
            vertical_align: match node.style.vertical_align {
                strata_style::types::VerticalAlign::Baseline => {
                    crate::typography::VerticalAlignment::Baseline
                }
                strata_style::types::VerticalAlign::Top => {
                    crate::typography::VerticalAlignment::Top
                }
                strata_style::types::VerticalAlign::Bottom => {
                    crate::typography::VerticalAlignment::Bottom
                }
                strata_style::types::VerticalAlign::Middle => {
                    crate::typography::VerticalAlignment::Middle
                }
                strata_style::types::VerticalAlign::Sub => {
                    crate::typography::VerticalAlignment::Sub
                }
                strata_style::types::VerticalAlign::Super => {
                    crate::typography::VerticalAlignment::Super
                }
                strata_style::types::VerticalAlign::TextTop => {
                    crate::typography::VerticalAlignment::TextTop
                }
                strata_style::types::VerticalAlign::TextBottom => {
                    crate::typography::VerticalAlignment::TextBottom
                }
            },
            is_monospace: crate::text::is_monospace_family(node.style.font_family),
            exclusion_space: parent_exclusions.as_deref(),
            bfc_offset_y: bfc_offset_y + padding.top + border.top,
        };

        let mut res = ifc.layout_inline_stream(&node.inline_items, &opt);
        truncate_inline_overflow(node, &mut res, content_width);
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

                    // If this child is an inline container (like <a> or <span><a>), recursively size and position its inline descendants
                    let target_bounds = child.geometry.border_box;
                    propagate_atomic_bounds_to_inline_descendants(child, target_bounds);
                }
            }
        }

        let total_h = res.total_height;
        for child in &mut node.children {
            if let Some(ref text) = child.text {
                let mut found = false;
                for line in &positioned_lines {
                    for frag in &line.text_fragments {
                        if frag.text.contains(text.as_str()) || text.contains(frag.text.as_str()) {
                            child.geometry.border_box = frag.bounds;
                            found = true;
                            break;
                        }
                    }
                    if found {
                        break;
                    }
                }
                if !found {
                    child.geometry.border_box.x = content_origin_x;
                    child.geometry.border_box.y = content_origin_y;
                }
            }
        }
        node.line_boxes = positioned_lines;
        return total_h + padding.top + padding.bottom + border.top + border.bottom;
    }

    // W3C CSS 2.1 §9.4.1 Block Formatting Context (BFC) root determination:
    // Normal in-flow block boxes with overflow: visible share their parent BFC's ExclusionSpace
    // so floats inside a <section> or <div> overhang into subsequent sibling sections.
    let establishes_bfc = node_establishes_bfc(node);

    let is_root_exclusion_context = parent_exclusions.is_none();
    let mut local_exclusions = crate::exclusions::ExclusionSpace::new();
    let (exclusion_space, base_bfc_y): (&mut crate::exclusions::ExclusionSpace, f32) =
        if establishes_bfc || is_root_exclusion_context {
            (&mut local_exclusions, 0.0)
        } else if let Some(pe) = parent_exclusions {
            (pe, bfc_offset_y)
        } else {
            (&mut local_exclusions, 0.0)
        };
    let initial_exclusions_count = exclusion_space.exclusions().len();
    let parent_top_adjoining = !establishes_bfc && border.top == 0.0 && padding.top == 0.0;
    let parent_bottom_adjoining = !establishes_bfc
        && border.bottom == 0.0
        && padding.bottom == 0.0
        && node.style.height == Dimension::Auto;

    let mut current_y = border.top + padding.top;
    let mut pending_margin: Option<f32> = None;
    let mut is_first_in_flow = true;
    let mut last_child_establishes_bfc = false;

    for child in &mut node.children {
        if child.style.position == Position::Absolute || child.style.position == Position::Fixed {
            continue;
        }

        // Handle CSS clear in BFC coordinate space for in-flow blocks
        if child.style.float == strata_style::types::Float::None
            && child.style.clear != strata_style::types::Clear::None
        {
            let cleared_bfc_y =
                exclusion_space.clearance_offset(child.style.clear, base_bfc_y + current_y);
            if cleared_bfc_y > base_bfc_y + current_y {
                current_y = cleared_bfc_y - base_bfc_y;
                pending_margin = None;
            }
        }

        // Handle CSS float
        if child.style.float != strata_style::types::Float::None {
            // Layout float child as an independent BFC to compute its dimensions
            let child_guard = guard.enter();
            if let Some(cg) = child_guard {
                child.compute_layout_guarded(content_width, available_height, None, 0.0, cg);
            } else {
                child.compute_layout_guarded(content_width, available_height, None, 0.0, guard);
            }
            let float_margin = child.style.margin;
            let float_w = child.geometry.border_box.width + float_margin.left + float_margin.right;
            let float_h = child.geometry.border_box.height + float_margin.top + float_margin.bottom;

            // Query line opportunity at float_bfc_y in BFC coordinate space, respecting float's own clearance
            let mut float_bfc_y = base_bfc_y + current_y;
            if child.style.clear != strata_style::types::Clear::None {
                let cleared_bfc_y =
                    exclusion_space.clearance_offset(child.style.clear, float_bfc_y);
                if cleared_bfc_y > float_bfc_y {
                    float_bfc_y = cleared_bfc_y;
                }
            }
            let mut opp = exclusion_space.line_opportunity(float_bfc_y, float_h, content_width);
            if opp.available_width < float_w && content_width >= float_w {
                // Drop down if float does not fit horizontally
                while let Some(next_bfc_y) = exclusion_space.next_float_bottom_after(float_bfc_y) {
                    float_bfc_y = next_bfc_y;
                    opp = exclusion_space.line_opportunity(float_bfc_y, float_h, content_width);
                    if opp.available_width >= float_w {
                        break;
                    }
                }
            }
            let local_float_y = float_bfc_y - base_bfc_y;

            let (target_x, target_y) = match child.style.float {
                strata_style::types::Float::Left | strata_style::types::Float::InlineStart => {
                    let float_x =
                        border.left + padding.left + opp.left_float_edge + float_margin.left;
                    let float_y = local_float_y + float_margin.top;
                    let excl_rect = crate::geometry::Rect::from_xywh(
                        opp.left_float_edge,
                        float_bfc_y,
                        float_w,
                        float_h,
                    );
                    exclusion_space
                        .add_exclusion(excl_rect, crate::exclusions::ExclusionKind::FloatLeft);
                    (
                        node.geometry.border_box.x + float_x,
                        node.geometry.border_box.y + float_y,
                    )
                }
                strata_style::types::Float::Right | strata_style::types::Float::InlineEnd => {
                    let start_r = opp.right_float_edge;
                    let float_x =
                        border.left + padding.left + start_r - float_w + float_margin.left;
                    let float_y = local_float_y + float_margin.top;
                    let excl_rect = crate::geometry::Rect::from_xywh(
                        start_r - float_w,
                        float_bfc_y,
                        float_w,
                        float_h,
                    );
                    exclusion_space
                        .add_exclusion(excl_rect, crate::exclusions::ExclusionKind::FloatRight);
                    (
                        node.geometry.border_box.x + float_x,
                        node.geometry.border_box.y + float_y,
                    )
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

        // CSS 2.1 §8.3.1: An empty block's top and bottom margins collapse through
        if is_empty_collapsible_block(child) {
            let through_margin = collapse_margins(child_margin.top, child_margin.bottom);
            let combined = match pending_margin {
                Some(pm) => collapse_margins(pm, through_margin),
                None => through_margin,
            };

            if is_first_in_flow && parent_top_adjoining {
                node.style.margin.top = collapse_margins(node.style.margin.top, combined);
                pending_margin = None;
            } else {
                pending_margin = Some(combined);
            }

            child.geometry.border_box.x =
                node.geometry.border_box.x + border.left + padding.left + child_margin.left;
            child.geometry.border_box.y =
                node.geometry.border_box.y + current_y + pending_margin.unwrap_or(0.0);
            child.geometry.border_box.width = content_width;
            child.geometry.border_box.height = 0.0;
            continue;
        }

        let child_establishes_bfc = node_establishes_bfc(child);

        let mut collapsed_v_margin = match pending_margin.take() {
            Some(prev_m) => collapse_margins(prev_m, child_margin.top),
            None => child_margin.top,
        };

        if is_first_in_flow {
            is_first_in_flow = false;
            if parent_top_adjoining && !child_establishes_bfc {
                node.style.margin.top = collapse_margins(node.style.margin.top, collapsed_v_margin);
                collapsed_v_margin = 0.0;
            }
        }

        current_y += collapsed_v_margin;

        let mut child_available_width = content_width;
        let mut child_x = border.left + padding.left + child_margin.left;

        // CSS 2.1 §9.5: An in-flow element that establishes a new BFC must not overlap the margin box of any floats in the same BFC
        if child_establishes_bfc && !exclusion_space.exclusions().is_empty() {
            let mut child_bfc_y = base_bfc_y + current_y;
            let mut opp = exclusion_space.line_opportunity(child_bfc_y, 0.0, content_width);

            if opp.available_width < content_width || opp.left_float_edge > 0.0 {
                let child_desired_w =
                    resolve_dimension_with_node(child.style.width, content_width, child);
                if let Some(w) = child_desired_w {
                    if w + child_margin.left + child_margin.right > opp.available_width {
                        // Element does not fit beside the float; clear by placing it below preceding floats
                        while let Some(next_y) =
                            exclusion_space.next_float_bottom_after(child_bfc_y)
                        {
                            child_bfc_y = next_y;
                            opp = exclusion_space.line_opportunity(child_bfc_y, 0.0, content_width);
                            if opp.available_width >= w + child_margin.left + child_margin.right {
                                break;
                            }
                        }
                        current_y = child_bfc_y - base_bfc_y;
                    }
                }

                child_available_width =
                    (opp.available_width - child_margin.left - child_margin.right).max(0.0);
                child_x = border.left + padding.left + opp.left_float_edge + child_margin.left;
            }
        }

        child.geometry.border_box.x = node.geometry.border_box.x + child_x;
        child.geometry.border_box.y = node.geometry.border_box.y + current_y;

        // Pass the active BFC exclusion space down to in-flow block children so floats can overhang across sections,
        // EXCEPT when child establishes its own BFC or exclusion space is currently empty.
        let passed_exclusions = if child_establishes_bfc || exclusion_space.exclusions().is_empty()
        {
            None
        } else {
            Some(&mut *exclusion_space)
        };

        let child_guard = guard.enter();
        if let Some(cg) = child_guard {
            child.compute_layout_guarded(
                child_available_width,
                available_height,
                passed_exclusions,
                base_bfc_y + current_y,
                cg,
            );
        } else {
            child.compute_layout_guarded(
                child_available_width,
                available_height,
                passed_exclusions,
                base_bfc_y + current_y,
                guard,
            );
        }

        // Auto margin horizontal centering (mx-auto / ml-auto / mr-auto)
        if child.style.margin_auto_left && child.style.margin_auto_right {
            let free_space = child_available_width - child.geometry.border_box.width;
            let auto_m = (free_space / 2.0).max(0.0);
            let target_x = node.geometry.border_box.x + child_x - child_margin.left + auto_m;
            let dx = target_x - child.geometry.border_box.x;
            if dx != 0.0 {
                translate_node_and_descendants(child, dx, 0.0);
            }
        } else if child.style.margin_auto_left {
            let free_space =
                child_available_width - child.geometry.border_box.width - child_margin.right;
            let target_x =
                node.geometry.border_box.x + child_x - child_margin.left + free_space.max(0.0);
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
        pending_margin = Some(child_margin.bottom);
        last_child_establishes_bfc = child_establishes_bfc;
    }

    if let Some(trailing_margin) = pending_margin {
        if parent_bottom_adjoining && !last_child_establishes_bfc {
            node.style.margin.bottom = collapse_margins(node.style.margin.bottom, trailing_margin);
        } else {
            current_y += trailing_margin;
        }
    }

    // CSS 2.1 §10.6.7 BFC float containment: BFC roots or layout tree root expand auto height to enclose floats
    if (establishes_bfc || is_root_exclusion_context)
        && exclusion_space.exclusions().len() > initial_exclusions_count
    {
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
    solve_block_with_exclusions_guarded(
        node,
        available_width,
        available_height,
        None,
        0.0,
        crate::tree::LayoutDepthGuard::root(),
    );
}

/// Executes normal-flow block layout passing parent exclusion space for float obstructions.
pub fn solve_block_with_exclusions(
    node: &mut LayoutNode,
    available_width: f32,
    available_height: f32,
    parent_exclusions: Option<&mut crate::exclusions::ExclusionSpace>,
    bfc_offset_y: f32,
) {
    solve_block_with_exclusions_guarded(
        node,
        available_width,
        available_height,
        parent_exclusions,
        bfc_offset_y,
        crate::tree::LayoutDepthGuard::root(),
    );
}

/// Executes normal-flow block layout with an active depth guard to prevent stack overflow.
pub fn solve_block_with_exclusions_guarded(
    node: &mut LayoutNode,
    available_width: f32,
    available_height: f32,
    parent_exclusions: Option<&mut crate::exclusions::ExclusionSpace>,
    bfc_offset_y: f32,
    guard: crate::tree::LayoutDepthGuard,
) {
    let padding = node.style.padding;
    let border = node.style.border_width;
    let margin = node.style.margin;

    if guard.is_exhausted() {
        crate::tree::record_layout_overflow(guard.current_depth(), guard.max_depth());
        let border_box_width =
            match resolve_dimension_with_node(node.style.width, available_width, node) {
                Some(w) => clamp_dimension_with_node(
                    w,
                    node.style.min_width,
                    node.style.max_width,
                    available_width,
                    node,
                ),
                None => (available_width - margin.left - margin.right).max(0.0),
            };
        let border_box_height = match resolve_dimension(node.style.height, available_height) {
            Some(h) => clamp_dimension(
                h,
                node.style.min_height,
                node.style.max_height,
                available_height,
            ),
            None => 0.0,
        };
        let border_box = Rect::new(
            node.geometry.border_box.x,
            node.geometry.border_box.y,
            border_box_width,
            border_box_height,
        );
        node.geometry = LayoutGeometry::from_border_box(border_box, padding, border, margin);
        node.geometry.snap_to_pixel();
        return;
    }

    // Content-Visibility: Hidden virtualization (W3C CSS Containment Level 2)
    if node.style.content_visibility == strata_style::types::ContentVisibility::Hidden {
        let intrinsic_w = node
            .style
            .contain_intrinsic_width
            .and_then(|dim| resolve_dimension_with_node(dim, available_width, node));
        let intrinsic_h = node
            .style
            .contain_intrinsic_height
            .and_then(|dim| resolve_dimension(dim, available_height))
            .unwrap_or(0.0);
        let border_box_width =
            match resolve_dimension_with_node(node.style.width, available_width, node)
                .or(intrinsic_w)
            {
                Some(w) => clamp_dimension_with_node(
                    w,
                    node.style.min_width,
                    node.style.max_width,
                    available_width,
                    node,
                ),
                None => (available_width - margin.left - margin.right).max(0.0),
            };
        let border_box_height = match resolve_dimension(node.style.height, available_height) {
            Some(h) => clamp_dimension(
                h,
                node.style.min_height,
                node.style.max_height,
                available_height,
            ),
            None => clamp_dimension(
                intrinsic_h,
                node.style.min_height,
                node.style.max_height,
                available_height,
            ),
        };
        let border_box = Rect::new(
            node.geometry.border_box.x,
            node.geometry.border_box.y,
            border_box_width,
            border_box_height,
        );
        node.geometry = LayoutGeometry::from_border_box(border_box, padding, border, margin);
        node.geometry.snap_to_pixel();
        return;
    }

    let (text_w, text_h) = if node.children.is_empty() && !node.inline_items.is_empty() {
        let ifc = crate::typography::InlineFormattingContext::new();
        let opt = crate::typography::InlineLayoutOptions {
            font_size: node.style.font_size,
            line_height: Some(node.style.font_size * node.style.line_height),
            max_width: Some(available_width),
            white_space: match node.style.white_space {
                strata_style::types::WhiteSpace::Normal => {
                    crate::typography::WhiteSpaceMode::Normal
                }
                strata_style::types::WhiteSpace::NoWrap => {
                    crate::typography::WhiteSpaceMode::NoWrap
                }
                strata_style::types::WhiteSpace::Pre => crate::typography::WhiteSpaceMode::Pre,
                strata_style::types::WhiteSpace::PreWrap => {
                    crate::typography::WhiteSpaceMode::PreWrap
                }
                strata_style::types::WhiteSpace::PreLine => {
                    crate::typography::WhiteSpaceMode::PreLine
                }
            },
            vertical_align: crate::typography::VerticalAlignment::Baseline,
            is_monospace: crate::text::is_monospace_family(node.style.font_family),
            exclusion_space: parent_exclusions.as_deref(),
            bfc_offset_y,
        };
        let res = ifc.layout_inline_stream(&node.inline_items, &opt);
        (res.total_width, res.total_height)
    } else if let Some(ref text) = node.text {
        let is_mono = crate::text::is_monospace_family(node.style.font_family);
        crate::text::measure_text_bounds_with_white_space(
            text,
            node.style.font_size,
            node.style.line_height,
            Some(available_width),
            is_mono,
            node.style.white_space,
        )
    } else {
        (0.0, 0.0)
    };

    // 1. Resolve element width
    let resolved_width = match resolve_dimension_with_node(node.style.width, available_width, node)
    {
        Some(w) => clamp_dimension_with_node(
            w,
            node.style.min_width,
            node.style.max_width,
            available_width,
            node,
        ),
        None => {
            if node.text.is_some()
                || matches!(
                    node.style.display,
                    strata_style::types::Display::Inline
                        | strata_style::types::Display::InlineBlock
                )
            {
                // Shrink-to-fit for text or inline-block elements per W3C CSS 2.1 §10.3.9:
                // shrink_to_fit = min(max(min_content, available_width), max_content)
                let base_w = if !node.children.is_empty() {
                    let min_max =
                        crate::intrinsic::compute_min_max_sizes(node, Some(available_width));
                    let avail_content_w = (available_width
                        - padding.left
                        - padding.right
                        - border.left
                        - border.right
                        - margin.left
                        - margin.right)
                        .max(0.0);
                    min_max
                        .min_content
                        .max(avail_content_w)
                        .min(min_max.max_content)
                        .max(text_w)
                } else {
                    text_w
                };
                let fit_w = base_w + padding.left + padding.right + border.left + border.right;
                clamp_dimension_with_node(
                    fit_w,
                    node.style.min_width,
                    node.style.max_width,
                    available_width,
                    node,
                )
            } else {
                // In CSS block layout, auto width fills available width minus margins
                let auto_w = (available_width - margin.left - margin.right).max(0.0);
                clamp_dimension_with_node(
                    auto_w,
                    node.style.min_width,
                    node.style.max_width,
                    available_width,
                    node,
                )
            }
        }
    };

    let border_box_width = match node.style.box_sizing {
        BoxSizing::BorderBox => resolved_width,
        BoxSizing::ContentBox => {
            resolved_width + padding.left + padding.right + border.left + border.right
        }
    };

    let content_width =
        (border_box_width - padding.left - padding.right - border.left - border.right).max(0.0);

    // 2. Lay out in-flow children vertically with margin collapsing and floats
    let children_height = if node.children.is_empty() && node.text.is_some() {
        text_h + padding.top + padding.bottom + border.top + border.bottom
    } else {
        solve_block_children_with_exclusions_guarded(
            node,
            content_width,
            available_height,
            parent_exclusions,
            bfc_offset_y,
            guard,
        )
    };

    // 3. Resolve element height
    let resolved_height = match resolve_dimension(node.style.height, available_height) {
        Some(h) => {
            let clamped_h = clamp_dimension(
                h,
                node.style.min_height,
                node.style.max_height,
                available_height,
            );
            match node.style.box_sizing {
                BoxSizing::BorderBox => clamped_h,
                BoxSizing::ContentBox => {
                    clamped_h + padding.top + padding.bottom + border.top + border.bottom
                }
            }
        }
        None => {
            // Auto height wraps children
            clamp_dimension(
                children_height,
                node.style.min_height,
                node.style.max_height,
                available_height,
            )
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

    node.geometry = LayoutGeometry::from_border_box(border_box, padding, border, node.style.margin);
    node.geometry.snap_to_pixel();

    // A text leaf that truncates must publish positioned line boxes so paint
    // renders the clipped ellipsis fragment instead of the full string.
    if node.children.is_empty()
        && node.style.text_overflow == strata_style::types::TextOverflow::Ellipsis
        && node.style.overflow_x != strata_style::types::Overflow::Visible
        && content_width > 0.0
    {
        if let Some(ref text) = node.text {
            let inline_style = inline_style_from_computed_style(&node.style);
            let item = crate::typography::InlineItem::text(text.clone(), inline_style);
            let ifc = crate::typography::InlineFormattingContext::new();
            let opt = crate::typography::InlineLayoutOptions {
                font_size: node.style.font_size,
                line_height: Some(node.style.font_size * node.style.line_height),
                max_width: Some(content_width),
                white_space: match node.style.white_space {
                    strata_style::types::WhiteSpace::Normal => {
                        crate::typography::WhiteSpaceMode::Normal
                    }
                    strata_style::types::WhiteSpace::NoWrap => {
                        crate::typography::WhiteSpaceMode::NoWrap
                    }
                    strata_style::types::WhiteSpace::Pre => crate::typography::WhiteSpaceMode::Pre,
                    strata_style::types::WhiteSpace::PreWrap => {
                        crate::typography::WhiteSpaceMode::PreWrap
                    }
                    strata_style::types::WhiteSpace::PreLine => {
                        crate::typography::WhiteSpaceMode::PreLine
                    }
                },
                vertical_align: crate::typography::VerticalAlignment::Baseline,
                is_monospace: crate::text::is_monospace_family(node.style.font_family),
                exclusion_space: None,
                bfc_offset_y: 0.0,
            };
            let mut res = ifc.layout_inline_stream(&[item], &opt);
            truncate_inline_overflow(node, &mut res, content_width);
            let origin_x = node.geometry.content_box.x;
            let origin_y = node.geometry.content_box.y;
            for line in &mut res.lines {
                let align_offset_x = match node.style.text_align {
                    strata_style::types::TextAlign::Center => {
                        ((content_width - line.bounds.width) / 2.0).max(0.0)
                    }
                    strata_style::types::TextAlign::Right => {
                        (content_width - line.bounds.width).max(0.0)
                    }
                    _ => 0.0,
                };
                line.bounds.x += origin_x + align_offset_x;
                line.bounds.y += origin_y;
                for frag in &mut line.text_fragments {
                    frag.bounds.x += origin_x + align_offset_x;
                    frag.bounds.y += origin_y;
                    frag.baseline_y += origin_y;
                }
            }
            node.line_boxes = res.lines;
        }
    }
}
