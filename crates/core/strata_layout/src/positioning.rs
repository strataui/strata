//! CSS Positioning Scheme Engine (Relative, Absolute, Fixed, Sticky).
//!
//! Implements W3C CSS 2.1 Section 10.1 (Containing Block), Section 10.3.7 & 10.6.4
//! (Absolute Positioning Equations, Double-Anchored Inset Stretching, Auto-Margin Centering),
//! and CSS Positioned Layout Level 3 Section 6 (Sticky Constraints).

use crate::box_model::{clamp_dimension, resolve_dimension};
use crate::geometry::{LayoutGeometry, Rect};
use crate::tree::LayoutNode;
use strata_style::types::{
    AlignItems, AlignSelf, Dimension, Display, FlexDirection, JustifyContent, Position,
};

/// Active containing block context propagated down the layout hierarchy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContainingBlock {
    /// The padding box of the containing block in global coordinates.
    pub padding_box: Rect,
    /// The viewport bounds (for position: fixed).
    pub viewport: Rect,
    /// Scroll offset of the viewport or scroll container (scroll_x, scroll_y).
    pub scroll_offset: (f32, f32),
}

impl ContainingBlock {
    /// Creates a containing block from a padding box and viewport.
    pub const fn new(padding_box: Rect, viewport: Rect) -> Self {
        Self {
            padding_box,
            viewport,
            scroll_offset: (0.0, 0.0),
        }
    }

    /// Creates an initial containing block matching the viewport.
    pub const fn from_viewport(viewport_width: f32, viewport_height: f32) -> Self {
        let vp = Rect::new(0.0, 0.0, viewport_width, viewport_height);
        Self {
            padding_box: vp,
            viewport: vp,
            scroll_offset: (0.0, 0.0),
        }
    }

    /// Creates a containing block with an explicit scroll offset.
    pub const fn with_scroll(
        padding_box: Rect,
        viewport: Rect,
        scroll_x: f32,
        scroll_y: f32,
    ) -> Self {
        Self {
            padding_box,
            viewport,
            scroll_offset: (scroll_x, scroll_y),
        }
    }
}

/// Resolves out-of-flow and relative positioning offsets for a container's children
/// and recursively propagates containing blocks down through all descendants.
pub fn solve_positioning(container: &mut LayoutNode, viewport_width: f32, viewport_height: f32) {
    solve_positioning_guarded(
        container,
        viewport_width,
        viewport_height,
        crate::tree::LayoutDepthGuard::root(),
    );
}

/// Resolves positioning with an active depth guard to prevent call-stack exhaustion.
pub fn solve_positioning_guarded(
    container: &mut LayoutNode,
    viewport_width: f32,
    viewport_height: f32,
    guard: crate::tree::LayoutDepthGuard,
) {
    let padding_box = if container.geometry.padding_box.width > 0.0
        || container.geometry.padding_box.height > 0.0
    {
        container.geometry.padding_box
    } else {
        Rect::new(0.0, 0.0, viewport_width, viewport_height)
    };

    let initial_cb = ContainingBlock::new(
        padding_box,
        Rect::new(0.0, 0.0, viewport_width, viewport_height),
    );

    solve_positioning_children_guarded(container, initial_cb, guard);

    // W3C CSS Flexbox Level 1 §4.1: the static position of an absolutely
    // positioned child of a flex container is resolved as if the child were
    // the sole flex item, honoring `justify-content` on the main axis and
    // `align-self`/`align-items` on the cross axis. Only axes whose insets
    // are `auto` use the static position.
    if matches!(container.style.display, Display::Flex | Display::InlineFlex) {
        apply_flex_static_positions(container);
    }
}

/// Repositions absolutely positioned flex children that rely on the static
/// position on one or both axes.
fn apply_flex_static_positions(container: &mut LayoutNode) {
    let content = container.geometry.content_box;
    if content.width <= 0.0 && content.height <= 0.0 {
        return;
    }
    let is_row = matches!(
        container.style.flex_direction,
        FlexDirection::Row | FlexDirection::RowReverse
    );

    for child in &mut container.children {
        if child.style.position != Position::Absolute {
            continue;
        }
        let left_auto = matches!(child.style.insets.left, Dimension::Auto);
        let right_auto = matches!(child.style.insets.right, Dimension::Auto);
        let top_auto = matches!(child.style.insets.top, Dimension::Auto);
        let bottom_auto = matches!(child.style.insets.bottom, Dimension::Auto);
        if !(left_auto || right_auto || top_auto || bottom_auto) {
            continue;
        }

        let item_w =
            child.geometry.border_box.width + child.style.margin.left + child.style.margin.right;
        let item_h =
            child.geometry.border_box.height + child.style.margin.top + child.style.margin.bottom;

        let main_offset = if is_row {
            justify_offset(container.style.justify_content, content.width, item_w)
        } else {
            justify_offset(container.style.justify_content, content.height, item_h)
        };

        let effective_align = match child.style.align_self {
            AlignSelf::Auto => container.style.align_items,
            AlignSelf::Stretch => AlignItems::Stretch,
            AlignSelf::FlexStart => AlignItems::FlexStart,
            AlignSelf::FlexEnd => AlignItems::FlexEnd,
            AlignSelf::Center => AlignItems::Center,
            AlignSelf::Baseline => AlignItems::Baseline,
        };
        let cross_offset = if is_row {
            align_offset(effective_align, content.height, item_h)
        } else {
            align_offset(effective_align, content.width, item_w)
        };

        let static_x = if is_row {
            content.x + main_offset + child.style.margin.left
        } else {
            content.x + cross_offset + child.style.margin.left
        };
        let static_y = if is_row {
            content.y + cross_offset + child.style.margin.top
        } else {
            content.y + main_offset + child.style.margin.top
        };

        // A specified inset wins; only `auto` insets fall back to the
        // flex-derived static position.
        let dx = if left_auto {
            static_x - child.geometry.border_box.x
        } else {
            0.0
        };
        let dy = if top_auto {
            static_y - child.geometry.border_box.y
        } else {
            0.0
        };
        if dx != 0.0 || dy != 0.0 {
            child.translate(dx, dy);
        }
    }
}

/// Resolves a `justify-content` offset for a single flex item.
fn justify_offset(justify: JustifyContent, available: f32, item: f32) -> f32 {
    let free = (available - item).max(0.0);
    match justify {
        JustifyContent::FlexStart => 0.0,
        JustifyContent::FlexEnd => free,
        JustifyContent::Center => free / 2.0,
        JustifyContent::SpaceBetween => 0.0,
        JustifyContent::SpaceAround | JustifyContent::SpaceEvenly => free / 2.0,
    }
}

/// Resolves a cross-axis `align-items` offset for a single flex item.
fn align_offset(align: AlignItems, available: f32, item: f32) -> f32 {
    let free = (available - item).max(0.0);
    match align {
        AlignItems::FlexStart | AlignItems::Stretch | AlignItems::Baseline => 0.0,
        AlignItems::FlexEnd => free,
        AlignItems::Center => free / 2.0,
    }
}

/// Recursively traverses child nodes, resolving positioning and propagating the containing block.
pub fn solve_positioning_children(container: &mut LayoutNode, cb: ContainingBlock) {
    solve_positioning_children_guarded(container, cb, crate::tree::LayoutDepthGuard::root());
}

/// Recursively traverses child nodes with an active depth guard to prevent stack overflow.
pub fn solve_positioning_children_guarded(
    container: &mut LayoutNode,
    cb: ContainingBlock,
    guard: crate::tree::LayoutDepthGuard,
) {
    if guard.is_exhausted() {
        return;
    }
    let child_guard = match guard.enter() {
        Some(g) => g,
        None => return,
    };
    for child in &mut container.children {
        match child.style.position {
            Position::Static => {
                // Static elements do not establish a containing block for absolute descendants.
                // Descendants continue using the ancestor containing block `cb`.
                if !child.children.is_empty() {
                    solve_positioning_children_guarded(child, cb, child_guard);
                }
            }
            Position::Relative => {
                // 1. In-flow offset relative to normal position
                let mut dx = 0.0;
                let mut dy = 0.0;

                if let Some(top_px) =
                    resolve_dimension(child.style.insets.top, cb.padding_box.height)
                {
                    dy += top_px;
                } else if let Some(bot_px) =
                    resolve_dimension(child.style.insets.bottom, cb.padding_box.height)
                {
                    dy -= bot_px;
                }

                if let Some(left_px) =
                    resolve_dimension(child.style.insets.left, cb.padding_box.width)
                {
                    dx += left_px;
                } else if let Some(right_px) =
                    resolve_dimension(child.style.insets.right, cb.padding_box.width)
                {
                    dx -= right_px;
                }

                if dx != 0.0 || dy != 0.0 {
                    child.translate(dx, dy);
                }

                // 2. Relative elements establish a containing block for absolute descendants
                let child_cb = ContainingBlock {
                    padding_box: child.geometry.padding_box,
                    viewport: cb.viewport,
                    scroll_offset: cb.scroll_offset,
                };

                if !child.children.is_empty() {
                    solve_positioning_children_guarded(child, child_cb, child_guard);
                }
            }
            Position::Absolute => {
                // Out-of-flow positioning relative to nearest non-static ancestor (cb.padding_box)
                solve_out_of_flow_box_guarded(child, cb.padding_box, cb.viewport, false, child_guard);

                // Absolute elements establish a containing block for descendants
                let child_cb = ContainingBlock {
                    padding_box: child.geometry.padding_box,
                    viewport: cb.viewport,
                    scroll_offset: cb.scroll_offset,
                };

                if !child.children.is_empty() {
                    solve_positioning_children_guarded(child, child_cb, child_guard);
                }
            }
            Position::Fixed => {
                // Out-of-flow positioning relative to the viewport
                solve_out_of_flow_box_guarded(child, cb.viewport, cb.viewport, true, child_guard);

                // Fixed elements establish a containing block for descendants
                let child_cb = ContainingBlock {
                    padding_box: child.geometry.padding_box,
                    viewport: cb.viewport,
                    scroll_offset: (0.0, 0.0),
                };

                if !child.children.is_empty() {
                    solve_positioning_children_guarded(child, child_cb, child_guard);
                }
            }
            Position::Sticky => {
                // In CSS Positioned Layout Level 3 §6, the containing block for a sticky element
                // is established by its nearest block container ancestor (`container`), not the
                // nearest positioned ancestor.
                let parent_box = if container.geometry.padding_box.width > 0.0
                    || container.geometry.padding_box.height > 0.0
                {
                    container.geometry.padding_box
                } else {
                    cb.padding_box
                };
                let sticky_cb = ContainingBlock {
                    padding_box: parent_box,
                    viewport: cb.viewport,
                    scroll_offset: cb.scroll_offset,
                };
                solve_sticky_box(child, sticky_cb);

                let child_cb = ContainingBlock {
                    padding_box: child.geometry.padding_box,
                    viewport: cb.viewport,
                    scroll_offset: cb.scroll_offset,
                };

                if !child.children.is_empty() {
                    solve_positioning_children_guarded(child, child_cb, child_guard);
                }
            }
        }
    }
}

/// Solves out-of-flow geometry for an absolute or fixed element according to W3C CSS 2.1 §10.3.7 & §10.6.4.
pub fn solve_out_of_flow_box(
    child: &mut LayoutNode,
    cb_rect: Rect,
    viewport: Rect,
    is_fixed: bool,
) {
    solve_out_of_flow_box_guarded(
        child,
        cb_rect,
        viewport,
        is_fixed,
        crate::tree::LayoutDepthGuard::root(),
    );
}

/// Solves out-of-flow geometry with an active depth guard.
pub fn solve_out_of_flow_box_guarded(
    child: &mut LayoutNode,
    cb_rect: Rect,
    _viewport: Rect,
    _is_fixed: bool,
    guard: crate::tree::LayoutDepthGuard,
) {
    let left = resolve_dimension(child.style.insets.left, cb_rect.width);
    let right = resolve_dimension(child.style.insets.right, cb_rect.width);
    let top = resolve_dimension(child.style.insets.top, cb_rect.height);
    let bottom = resolve_dimension(child.style.insets.bottom, cb_rect.height);

    // 1. Horizontal Sizing & Inset Resolution (W3C CSS 2.1 §10.3.7)
    let item_w = match (left, right, child.style.width) {
        (Some(l), Some(r), Dimension::Auto) => {
            let available =
                (cb_rect.width - l - r - child.style.margin.left - child.style.margin.right)
                    .max(0.0);
            clamp_dimension(
                available,
                child.style.min_width,
                child.style.max_width,
                cb_rect.width,
            )
        }
        _ => {
            let w = resolve_dimension(child.style.width, cb_rect.width)
                .unwrap_or(child.geometry.border_box.width);
            clamp_dimension(
                w,
                child.style.min_width,
                child.style.max_width,
                cb_rect.width,
            )
        }
    };

    // 2. Vertical Sizing & Inset Resolution (W3C CSS 2.1 §10.6.4)
    let item_h = match (top, bottom, child.style.height) {
        (Some(t), Some(b), Dimension::Auto) => {
            let available =
                (cb_rect.height - t - b - child.style.margin.top - child.style.margin.bottom)
                    .max(0.0);
            clamp_dimension(
                available,
                child.style.min_height,
                child.style.max_height,
                cb_rect.height,
            )
        }
        _ => {
            let h = resolve_dimension(child.style.height, cb_rect.height)
                .unwrap_or(child.geometry.border_box.height);
            clamp_dimension(
                h,
                child.style.min_height,
                child.style.max_height,
                cb_rect.height,
            )
        }
    };

    // 3. Horizontal Position & Auto-Margin Centering (mx-auto / m-auto)
    let item_x = match (left, right) {
        (Some(l), Some(r)) if child.style.margin_auto_left && child.style.margin_auto_right => {
            let free_space = (cb_rect.width - l - r) - item_w;
            let auto_m = (free_space / 2.0).max(0.0);
            cb_rect.x + l + auto_m
        }
        (Some(l), _) if child.style.margin_auto_left => {
            let free_space = (cb_rect.width - l) - item_w;
            cb_rect.x + l + free_space.max(0.0)
        }
        (Some(l), _) => cb_rect.x + l + child.style.margin.left,
        (None, Some(r)) => cb_rect.x + cb_rect.width - r - child.style.margin.right - item_w,
        (None, None) => cb_rect.x + child.style.margin.left,
    };

    // 4. Vertical Position & Auto-Margin Centering (my-auto / m-auto)
    let item_y = match (top, bottom) {
        (Some(t), Some(b)) if child.style.margin_auto_top && child.style.margin_auto_bottom => {
            let free_space = (cb_rect.height - t - b) - item_h;
            let auto_m = (free_space / 2.0).max(0.0);
            cb_rect.y + t + auto_m
        }
        (Some(t), _) if child.style.margin_auto_top => {
            let free_space = (cb_rect.height - t) - item_h;
            cb_rect.y + t + free_space.max(0.0)
        }
        (Some(t), _) => cb_rect.y + t + child.style.margin.top,
        (None, Some(b)) => cb_rect.y + cb_rect.height - b - child.style.margin.bottom - item_h,
        (None, None) => cb_rect.y + child.style.margin.top,
    };

    child.geometry = LayoutGeometry::from_border_box(
        Rect::new(item_x, item_y, item_w, item_h),
        child.style.padding,
        child.style.border_width,
        child.style.margin,
    );

    if !child.children.is_empty() {
        child.layout_children_guarded(guard);
    }
}

/// Solves sticky positioning by evaluating insets against scroll offset and clamping to parent bounds.
fn solve_sticky_box(child: &mut LayoutNode, cb: ContainingBlock) {
    let normal_x = child.geometry.border_box.x;
    let normal_y = child.geometry.border_box.y;

    let mut dx = 0.0;
    let mut dy = 0.0;

    // Vertical sticky evaluation
    if let Some(top_px) = resolve_dimension(child.style.insets.top, cb.viewport.height) {
        let target_y = cb.scroll_offset.1 + top_px;
        let sticky_y = normal_y.max(target_y);
        let max_sticky_y = cb.padding_box.y + cb.padding_box.height
            - child.geometry.border_box.height
            - child.style.margin.bottom;
        let clamped_y = sticky_y.min(max_sticky_y.max(normal_y));
        dy = clamped_y - normal_y;
    } else if let Some(bot_px) = resolve_dimension(child.style.insets.bottom, cb.viewport.height) {
        let target_y =
            cb.scroll_offset.1 + cb.viewport.height - bot_px - child.geometry.border_box.height;
        let sticky_y = normal_y.min(target_y);
        let min_sticky_y = cb.padding_box.y + child.style.margin.top;
        let clamped_y = sticky_y.max(min_sticky_y.min(normal_y));
        dy = clamped_y - normal_y;
    }

    // Horizontal sticky evaluation
    if let Some(left_px) = resolve_dimension(child.style.insets.left, cb.viewport.width) {
        let target_x = cb.scroll_offset.0 + left_px;
        let sticky_x = normal_x.max(target_x);
        let max_sticky_x = cb.padding_box.x + cb.padding_box.width
            - child.geometry.border_box.width
            - child.style.margin.right;
        let clamped_x = sticky_x.min(max_sticky_x.max(normal_x));
        dx = clamped_x - normal_x;
    } else if let Some(right_px) = resolve_dimension(child.style.insets.right, cb.viewport.width) {
        let target_x =
            cb.scroll_offset.0 + cb.viewport.width - right_px - child.geometry.border_box.width;
        let sticky_x = normal_x.min(target_x);
        let min_sticky_x = cb.padding_box.x + child.style.margin.left;
        let clamped_x = sticky_x.max(min_sticky_x.min(normal_x));
        dx = clamped_x - normal_x;
    }

    if dx != 0.0 || dy != 0.0 {
        child.translate(dx, dy);
    }
}
