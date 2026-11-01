//! CSS Positioning Scheme Engine (Relative, Absolute, Fixed).

use crate::box_model::{clamp_dimension, resolve_dimension};
use crate::geometry::{LayoutGeometry, Rect};
use crate::tree::LayoutNode;
use strata_style::types::{Dimension, Position};

/// Resolves out-of-flow and relative positioning offsets for a container's children.
pub fn solve_positioning(container: &mut LayoutNode, viewport_width: f32, viewport_height: f32) {
    let container_padding_box = container.geometry.padding_box;

    for child in &mut container.children {
        match child.style.position {
            Position::Static => {
                // In-flow normal position, no offset
            }
            Position::Relative => {
                // Offset normal flow position without affecting siblings
                let mut dx = 0.0;
                let mut dy = 0.0;

                if let Some(top_px) = resolve_dimension(child.style.insets.top, container_padding_box.height) {
                    dy += top_px;
                } else if let Some(bot_px) = resolve_dimension(child.style.insets.bottom, container_padding_box.height) {
                    dy -= bot_px;
                }

                if let Some(left_px) = resolve_dimension(child.style.insets.left, container_padding_box.width) {
                    dx += left_px;
                } else if let Some(right_px) = resolve_dimension(child.style.insets.right, container_padding_box.width) {
                    dx -= right_px;
                }

                child.translate(dx, dy);
            }
            Position::Absolute => {
                // Out-of-flow positioning relative to containing block (container padding box)
                let cb = container_padding_box;

                let left = resolve_dimension(child.style.insets.left, cb.width);
                let right = resolve_dimension(child.style.insets.right, cb.width);
                let top = resolve_dimension(child.style.insets.top, cb.height);
                let bottom = resolve_dimension(child.style.insets.bottom, cb.height);

                // Compute width
                let item_w = match (left, right, child.style.width) {
                    (Some(l), Some(r), Dimension::Auto) => {
                        (cb.width - l - r).max(0.0)
                    }
                    _ => {
                        let w = resolve_dimension(child.style.width, cb.width)
                            .unwrap_or(child.geometry.border_box.width);
                        clamp_dimension(w, child.style.min_width, child.style.max_width, cb.width)
                    }
                };

                // Compute height
                let item_h = match (top, bottom, child.style.height) {
                    (Some(t), Some(b), Dimension::Auto) => {
                        (cb.height - t - b).max(0.0)
                    }
                    _ => {
                        let h = resolve_dimension(child.style.height, cb.height)
                            .unwrap_or(child.geometry.border_box.height);
                        clamp_dimension(h, child.style.min_height, child.style.max_height, cb.height)
                    }
                };

                // Compute x position
                let item_x = match (left, right) {
                    (Some(l), _) => cb.x + l,
                    (None, Some(r)) => cb.x + cb.width - r - item_w,
                    (None, None) => cb.x,
                };

                // Compute y position
                let item_y = match (top, bottom) {
                    (Some(t), _) => cb.y + t,
                    (None, Some(b)) => cb.y + cb.height - b - item_h,
                    (None, None) => cb.y,
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
            Position::Fixed => {
                // Out-of-flow positioning relative to viewport
                let left = resolve_dimension(child.style.insets.left, viewport_width);
                let right = resolve_dimension(child.style.insets.right, viewport_width);
                let top = resolve_dimension(child.style.insets.top, viewport_height);
                let bottom = resolve_dimension(child.style.insets.bottom, viewport_height);

                let item_w = match (left, right, child.style.width) {
                    (Some(l), Some(r), Dimension::Auto) => (viewport_width - l - r).max(0.0),
                    _ => {
                        let w = resolve_dimension(child.style.width, viewport_width)
                            .unwrap_or(child.geometry.border_box.width);
                        clamp_dimension(w, child.style.min_width, child.style.max_width, viewport_width)
                    }
                };

                let item_h = match (top, bottom, child.style.height) {
                    (Some(t), Some(b), Dimension::Auto) => (viewport_height - t - b).max(0.0),
                    _ => {
                        let h = resolve_dimension(child.style.height, viewport_height)
                            .unwrap_or(child.geometry.border_box.height);
                        clamp_dimension(h, child.style.min_height, child.style.max_height, viewport_height)
                    }
                };

                let item_x = match (left, right) {
                    (Some(l), _) => l,
                    (None, Some(r)) => viewport_width - r - item_w,
                    (None, None) => 0.0,
                };

                let item_y = match (top, bottom) {
                    (Some(t), _) => t,
                    (None, Some(b)) => viewport_height - b - item_h,
                    (None, None) => 0.0,
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
            Position::Sticky => {
                // Normal flow placement (sticky evaluation occurs during scroll in compositor)
            }
        }
    }
}
