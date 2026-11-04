//! CSS Positioning Scheme Engine (Relative, Absolute, Fixed, Sticky).
//!
//! Implements W3C CSS 2.1 Section 10.1 (Containing Block), Section 10.3.7 & 10.6.4
//! (Absolute Positioning Equations, Double-Anchored Inset Stretching, Auto-Margin Centering),
//! and CSS Positioned Layout Level 3 Section 6 (Sticky Constraints).

use crate::box_model::{clamp_dimension, resolve_dimension};
use crate::geometry::{LayoutGeometry, Rect};
use crate::tree::LayoutNode;
use strata_style::types::{Dimension, Position};

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
    pub const fn with_scroll(padding_box: Rect, viewport: Rect, scroll_x: f32, scroll_y: f32) -> Self {
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
    let padding_box = if container.geometry.padding_box.width > 0.0 || container.geometry.padding_box.height > 0.0 {
        container.geometry.padding_box
    } else {
        Rect::new(0.0, 0.0, viewport_width, viewport_height)
    };

    let initial_cb = ContainingBlock::new(
        padding_box,
        Rect::new(0.0, 0.0, viewport_width, viewport_height),
    );

    solve_positioning_children(container, initial_cb);
}

/// Recursively traverses child nodes, resolving positioning and propagating the containing block.
pub fn solve_positioning_children(container: &mut LayoutNode, cb: ContainingBlock) {
    for child in &mut container.children {
        match child.style.position {
            Position::Static => {
                // Static elements do not establish a containing block for absolute descendants.
                // Descendants continue using the ancestor containing block `cb`.
                if !child.children.is_empty() {
                    solve_positioning_children(child, cb);
                }
            }
            Position::Relative => {
                // 1. In-flow offset relative to normal position
                let mut dx = 0.0;
                let mut dy = 0.0;

                if let Some(top_px) = resolve_dimension(child.style.insets.top, cb.padding_box.height) {
                    dy += top_px;
                } else if let Some(bot_px) = resolve_dimension(child.style.insets.bottom, cb.padding_box.height) {
                    dy -= bot_px;
                }

                if let Some(left_px) = resolve_dimension(child.style.insets.left, cb.padding_box.width) {
                    dx += left_px;
                } else if let Some(right_px) = resolve_dimension(child.style.insets.right, cb.padding_box.width) {
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
                    solve_positioning_children(child, child_cb);
                }
            }
            Position::Absolute => {
                // Out-of-flow positioning relative to nearest non-static ancestor (cb.padding_box)
                solve_out_of_flow_box(child, cb.padding_box, cb.viewport, false);

                // Absolute elements establish a containing block for descendants
                let child_cb = ContainingBlock {
                    padding_box: child.geometry.padding_box,
                    viewport: cb.viewport,
                    scroll_offset: cb.scroll_offset,
                };

                if !child.children.is_empty() {
                    solve_positioning_children(child, child_cb);
                }
            }
            Position::Fixed => {
                // Out-of-flow positioning relative to the viewport
                solve_out_of_flow_box(child, cb.viewport, cb.viewport, true);

                // Fixed elements establish a containing block for descendants
                let child_cb = ContainingBlock {
                    padding_box: child.geometry.padding_box,
                    viewport: cb.viewport,
                    scroll_offset: (0.0, 0.0),
                };

                if !child.children.is_empty() {
                    solve_positioning_children(child, child_cb);
                }
            }
            Position::Sticky => {
                // Normal flow placement with dynamic scroll clamping against parent bounds
                solve_sticky_box(child, cb);

                let child_cb = ContainingBlock {
                    padding_box: child.geometry.padding_box,
                    viewport: cb.viewport,
                    scroll_offset: cb.scroll_offset,
                };

                if !child.children.is_empty() {
                    solve_positioning_children(child, child_cb);
                }
            }
        }
    }
}

/// Solves out-of-flow geometry for an absolute or fixed element according to W3C CSS 2.1 §10.3.7 & §10.6.4.
pub fn solve_out_of_flow_box(
    child: &mut LayoutNode,
    cb_rect: Rect,
    _viewport: Rect,
    _is_fixed: bool,
) {
    let left = resolve_dimension(child.style.insets.left, cb_rect.width);
    let right = resolve_dimension(child.style.insets.right, cb_rect.width);
    let top = resolve_dimension(child.style.insets.top, cb_rect.height);
    let bottom = resolve_dimension(child.style.insets.bottom, cb_rect.height);

    // 1. Horizontal Sizing & Inset Resolution (W3C CSS 2.1 §10.3.7)
    let item_w = match (left, right, child.style.width) {
        (Some(l), Some(r), Dimension::Auto) => {
            let available = (cb_rect.width - l - r - child.style.margin.left - child.style.margin.right).max(0.0);
            clamp_dimension(available, child.style.min_width, child.style.max_width, cb_rect.width)
        }
        _ => {
            let w = resolve_dimension(child.style.width, cb_rect.width)
                .unwrap_or(child.geometry.border_box.width);
            clamp_dimension(w, child.style.min_width, child.style.max_width, cb_rect.width)
        }
    };

    // 2. Vertical Sizing & Inset Resolution (W3C CSS 2.1 §10.6.4)
    let item_h = match (top, bottom, child.style.height) {
        (Some(t), Some(b), Dimension::Auto) => {
            let available = (cb_rect.height - t - b - child.style.margin.top - child.style.margin.bottom).max(0.0);
            clamp_dimension(available, child.style.min_height, child.style.max_height, cb_rect.height)
        }
        _ => {
            let h = resolve_dimension(child.style.height, cb_rect.height)
                .unwrap_or(child.geometry.border_box.height);
            clamp_dimension(h, child.style.min_height, child.style.max_height, cb_rect.height)
        }
    };

    // 3. Horizontal Position & Auto-Margin Centering (mx-auto / m-auto)
    let item_x = if left.is_some() && right.is_some() && child.style.margin_auto_left && child.style.margin_auto_right {
        let l = left.unwrap();
        let r = right.unwrap();
        let free_space = (cb_rect.width - l - r) - item_w;
        let auto_m = (free_space / 2.0).max(0.0);
        cb_rect.x + l + auto_m
    } else if left.is_some() && child.style.margin_auto_left {
        let l = left.unwrap();
        let free_space = (cb_rect.width - l) - item_w;
        cb_rect.x + l + free_space.max(0.0)
    } else {
        match (left, right) {
            (Some(l), _) => cb_rect.x + l + child.style.margin.left,
            (None, Some(r)) => cb_rect.x + cb_rect.width - r - child.style.margin.right - item_w,
            (None, None) => cb_rect.x + child.style.margin.left,
        }
    };

    // 4. Vertical Position & Auto-Margin Centering (my-auto / m-auto)
    let item_y = if top.is_some() && bottom.is_some() && child.style.margin_auto_top && child.style.margin_auto_bottom {
        let t = top.unwrap();
        let b = bottom.unwrap();
        let free_space = (cb_rect.height - t - b) - item_h;
        let auto_m = (free_space / 2.0).max(0.0);
        cb_rect.y + t + auto_m
    } else if top.is_some() && child.style.margin_auto_top {
        let t = top.unwrap();
        let free_space = (cb_rect.height - t) - item_h;
        cb_rect.y + t + free_space.max(0.0)
    } else {
        match (top, bottom) {
            (Some(t), _) => cb_rect.y + t + child.style.margin.top,
            (None, Some(b)) => cb_rect.y + cb_rect.height - b - child.style.margin.bottom - item_h,
            (None, None) => cb_rect.y + child.style.margin.top,
        }
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
        let max_sticky_y = cb.padding_box.y + cb.padding_box.height - child.geometry.border_box.height - child.style.margin.bottom;
        let clamped_y = sticky_y.min(max_sticky_y.max(normal_y));
        dy = clamped_y - normal_y;
    } else if let Some(bot_px) = resolve_dimension(child.style.insets.bottom, cb.viewport.height) {
        let target_y = cb.scroll_offset.1 + cb.viewport.height - bot_px - child.geometry.border_box.height;
        let sticky_y = normal_y.min(target_y);
        let min_sticky_y = cb.padding_box.y + child.style.margin.top;
        let clamped_y = sticky_y.max(min_sticky_y.min(normal_y));
        dy = clamped_y - normal_y;
    }

    // Horizontal sticky evaluation
    if let Some(left_px) = resolve_dimension(child.style.insets.left, cb.viewport.width) {
        let target_x = cb.scroll_offset.0 + left_px;
        let sticky_x = normal_x.max(target_x);
        let max_sticky_x = cb.padding_box.x + cb.padding_box.width - child.geometry.border_box.width - child.style.margin.right;
        let clamped_x = sticky_x.min(max_sticky_x.max(normal_x));
        dx = clamped_x - normal_x;
    } else if let Some(right_px) = resolve_dimension(child.style.insets.right, cb.viewport.width) {
        let target_x = cb.scroll_offset.0 + cb.viewport.width - right_px - child.geometry.border_box.width;
        let sticky_x = normal_x.min(target_x);
        let min_sticky_x = cb.padding_box.x + child.style.margin.left;
        let clamped_x = sticky_x.max(min_sticky_x.min(normal_x));
        dx = clamped_x - normal_x;
    }

    if dx != 0.0 || dy != 0.0 {
        child.translate(dx, dy);
    }
}
