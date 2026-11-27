//! Visual scrollbar thumb rendering and drag interaction handling.

use super::*;

impl AppRuntime {
    fn resolve_layout_node_at_path(&self, path: &[usize]) -> Option<&LayoutNode> {
        let mut cur_bp = &self.root_blueprint;
        let mut cur_layout = &self.layout_root;
        for &idx in path {
            let (_, next_bp, next_layout) = Self::paired_blueprint_children(cur_bp, cur_layout)
                .into_iter()
                .find(|(i, _, _)| *i == idx)?;
            cur_bp = next_bp;
            cur_layout = next_layout;
        }
        Some(cur_layout)
    }

    /// Returns `true` when `(px, py)` is owned by `bar` at the top of the CSS stacking order
    /// (i.e., not occluded by a higher z-index sibling/floating window, a Top-Layer `<Portal>`,
    /// or a positive z-index overlay mounted inside `bar`).
    pub(crate) fn scrollbar_owns_point(
        &self,
        bar: &ActiveScrollbar,
        px: f32,
        py: f32,
        cached_hit: &mut Option<Option<Vec<usize>>>,
    ) -> bool {
        // Clamp probe inside `bar.bounds` so Fitts's law +1px right-edge hits still test
        // against the true top-most stacking context at the right edge of the scroll container.
        let probe_x = px.clamp(
            bar.bounds.x,
            (bar.bounds.x + bar.bounds.width).max(bar.bounds.x),
        );
        let probe_y = py.clamp(
            bar.bounds.y,
            (bar.bounds.y + bar.bounds.height).max(bar.bounds.y),
        );
        let direct_probe;
        let hit_path_opt = if (probe_x - px).abs() < 1e-4 && (probe_y - py).abs() < 1e-4 {
            cached_hit
                .get_or_insert_with(|| {
                    Self::hit_test_node_path(&self.root_blueprint, &self.layout_root, px, py)
                })
                .as_ref()
        } else {
            direct_probe =
                Self::hit_test_node_path(&self.root_blueprint, &self.layout_root, probe_x, probe_y);
            direct_probe.as_ref()
        };

        let Some(hit_path) = hit_path_opt else {
            return false;
        };
        if !hit_path.starts_with(&bar.path) {
            return false;
        }

        // Verify no descendant strictly inside `bar` along `hit_path` established a Top-Layer
        // portal or positive z-index stacking context that paints above `bar`'s scrollbar.
        let mut cur_bp = &self.root_blueprint;
        let mut cur_layout = &self.layout_root;
        for (depth, &idx) in hit_path.iter().enumerate() {
            let parent_display = Some(cur_layout.style.display);
            let Some((_, next_bp, next_layout)) =
                Self::paired_blueprint_children(cur_bp, cur_layout)
                    .into_iter()
                    .find(|(i, _, _)| *i == idx)
            else {
                break;
            };
            cur_bp = next_bp;
            cur_layout = next_layout;
            if depth + 1 > bar.path.len() {
                if cur_layout.is_top_layer {
                    return false;
                }
                if cur_layout.establishes_stacking_context_in(parent_display, false)
                    && cur_layout.style.z_index.unwrap_or(0) > 0
                {
                    return false;
                }
            }
        }
        true
    }

    #[inline]
    fn scrollbar_geom_contains_hover(bar: &ActiveScrollbar, mx: f32, my: f32) -> bool {
        bar.clip_bounds.contains_point(mx, my)
            && (bar.thumb_bounds.contains_point(mx, my)
                || bar.track_bounds.contains_point(mx, my)
                || (mx >= bar.track_bounds.x
                    && mx <= bar.bounds.x + bar.bounds.width + 1.0
                    && my >= bar.track_bounds.y
                    && my <= bar.track_bounds.y + bar.track_bounds.height))
    }

    /// Builds inline [`strata_render::compositor::ScrollbarOverlay`] descriptors bound to each
    /// overflowing scroll container node so the compositor paints every scrollbar thumb inside its
    /// owning stacking context and clip bounds.
    pub(crate) fn build_scrollbar_overlays(
        &self,
    ) -> Vec<strata_render::compositor::ScrollbarOverlay> {
        if self.active_scrollbars.is_empty() {
            return Vec::new();
        }
        let (mx, my) = self.last_mouse_pos;
        let is_recent_scroll = self
            .last_scroll_interaction
            .is_some_and(|t| self.elapsed_since(t).as_millis() < 700);
        let mut cached_hit: Option<Option<Vec<usize>>> = None;
        let mut overlays = Vec::with_capacity(self.active_scrollbars.len());

        for bar in &self.active_scrollbars {
            if bar.max_scroll_y <= 1.0
                || bar.clip_bounds.width <= 0.0
                || bar.clip_bounds.height <= 0.0
            {
                continue;
            }
            let Some(node) = self.resolve_layout_node_at_path(&bar.path) else {
                continue;
            };
            let is_dragging = self
                .scrollbar_drag
                .as_ref()
                .is_some_and(|d| Self::scrollbar_drag_matches_bar(d, bar));
            let is_kinetic = self
                .active_smooth_scroll
                .as_ref()
                .is_some_and(|s| Self::smooth_scroll_matches_bar(s, bar));
            let is_hovered = Self::scrollbar_geom_contains_hover(bar, mx, my)
                && self.scrollbar_owns_point(bar, mx, my, &mut cached_hit);
            let is_active = is_dragging || is_hovered || is_kinetic || is_recent_scroll;
            let alpha = if is_active { 150u8 } else { 60u8 };

            let local_border = node.geometry.border_box;
            let local_padding = node.geometry.padding_box;
            let untransformed = (local_border.x - bar.bounds.x).abs() < 0.5
                && (local_border.y - bar.bounds.y).abs() < 0.5
                && (local_border.width - bar.bounds.width).abs() < 0.5
                && (local_border.height - bar.bounds.height).abs() < 0.5;

            let (mut thumb_bounds, track_bounds, container_bounds, clip_bounds) = if untransformed {
                (bar.thumb_bounds, bar.track_bounds, bar.bounds, bar.clip_bounds)
            } else {
                let track_width = 10.0;
                let inner_box = if local_padding.width > 0.0 && local_padding.height > 0.0 {
                    local_padding
                } else {
                    local_border
                };
                let track_x = (inner_box.x + inner_box.width - track_width).max(inner_box.x);
                let track_y = inner_box.y;
                let track_h = inner_box.height;
                let local_track = strata_layout::Rect::new(track_x, track_y, track_width, track_h);
                let min_thumb = 24.0_f32.min(track_h);
                let viewport_h = node.geometry.content_box.height.max(1.0);
                let thumb_h =
                    ((viewport_h / bar.content_h.max(1.0)) * track_h).clamp(min_thumb, track_h);
                let avail = (track_h - thumb_h).max(0.0);
                let thumb_y = if bar.max_scroll_y > 0.0 && avail > 0.0 {
                    track_y + (bar.scroll_y / bar.max_scroll_y) * avail
                } else {
                    track_y
                };
                let local_thumb = strata_layout::Rect::new(track_x, thumb_y, track_width, thumb_h);
                (local_thumb, local_track, local_border, local_border)
            };

            if is_active {
                let expand_px = 2.0;
                thumb_bounds.x = (thumb_bounds.x - expand_px).max(container_bounds.x);
                thumb_bounds.width = (thumb_bounds.width + expand_px)
                    .min((container_bounds.x + container_bounds.width) - thumb_bounds.x);
            }
            let min_y = track_bounds.y;
            let max_y = track_bounds.y + track_bounds.height;
            let clamped_top = thumb_bounds.y.clamp(min_y, max_y);
            let clamped_bot = (thumb_bounds.y + thumb_bounds.height).clamp(clamped_top, max_y);
            thumb_bounds.y = clamped_top;
            thumb_bounds.height = clamped_bot - clamped_top;
            if thumb_bounds.width <= 0.0 || thumb_bounds.height <= 0.0 {
                continue;
            }

            overlays.push(strata_render::compositor::ScrollbarOverlay {
                node_ptr: node as *const LayoutNode as usize,
                clip_bounds,
                clip_radius: bar.border_radius,
                thumb_bounds,
                color: strata_style::types::Color::rgba(255, 255, 255, alpha),
                thumb_radius: strata_style::types::BorderRadius::all(3.0),
            });
        }

        overlays
    }

    /// Renders sleek semi-transparent scrollbar thumbs for all overflowing scroll containers.
    pub fn render_scrollbars(&mut self) {
        self.render_scrollbars_filtered(None);
    }

    /// Renders overflowing scrollbar thumbs optionally filtered to document (`Some(false)`) or Top-Layer (`Some(true)`).
    pub fn render_scrollbars_filtered(&mut self, top_layer: Option<bool>) {
        let (mx, my) = self.last_mouse_pos;
        let is_recent_scroll = self
            .last_scroll_interaction
            .is_some_and(|t| self.elapsed_since(t).as_millis() < 700);
        let mut cached_hit: Option<Option<Vec<usize>>> = None;

        for bar in &self.active_scrollbars {
            if let Some(tl) = top_layer {
                if bar.is_top_layer != tl {
                    continue;
                }
            }
            if bar.max_scroll_y <= 1.0
                || bar.clip_bounds.width <= 0.0
                || bar.clip_bounds.height <= 0.0
            {
                continue;
            }
            let is_dragging = self
                .scrollbar_drag
                .as_ref()
                .is_some_and(|d| Self::scrollbar_drag_matches_bar(d, bar));
            let is_kinetic = self
                .active_smooth_scroll
                .as_ref()
                .is_some_and(|s| Self::smooth_scroll_matches_bar(s, bar));
            let is_hovered = Self::scrollbar_geom_contains_hover(bar, mx, my)
                && self.scrollbar_owns_point(bar, mx, my, &mut cached_hit);
            let is_active = is_dragging || is_hovered || is_kinetic || is_recent_scroll;
            let alpha = if is_active { 150u8 } else { 60u8 };

            let mut thumb_bounds = bar.thumb_bounds;
            if is_active {
                let expand_px = 2.0;
                thumb_bounds.x = (thumb_bounds.x - expand_px).max(bar.bounds.x);
                thumb_bounds.width = (thumb_bounds.width + expand_px)
                    .min((bar.bounds.x + bar.bounds.width) - thumb_bounds.x);
            }
            let min_y = bar.track_bounds.y;
            let max_y = bar.track_bounds.y + bar.track_bounds.height;
            let clamped_top = thumb_bounds.y.clamp(min_y, max_y);
            let clamped_bot = (thumb_bounds.y + thumb_bounds.height).clamp(clamped_top, max_y);
            thumb_bounds.y = clamped_top;
            thumb_bounds.height = clamped_bot - clamped_top;
            if thumb_bounds.width <= 0.0 || thumb_bounds.height <= 0.0 {
                continue;
            }

            self.display_list
                .push(strata_render::compositor::DrawCommand::PushClip {
                    bounds: bar.clip_bounds,
                    radius: bar.border_radius,
                });
            self.display_list
                .push(strata_render::compositor::DrawCommand::DrawRoundedRect {
                    bounds: thumb_bounds,
                    color: strata_style::types::Color::rgba(255, 255, 255, alpha),
                    border_radius: strata_style::types::BorderRadius::all(3.0),
                    border_width: strata_style::types::EdgeInsets::all(0.0),
                    border_color: None,
                });
            self.display_list
                .push(strata_render::compositor::DrawCommand::PopClip);
        }
    }

    /// Checks if a pointer press landed on any unoccluded active scrollbar thumb or track and starts dragging.
    pub fn check_scrollbar_thumb_press(&mut self, px: f32, py: f32) -> bool {
        let mut cached_hit: Option<Option<Vec<usize>>> = None;
        let mut matched: Option<(ActiveScrollbar, bool)> = None;

        for bar in self.active_scrollbars.iter().rev() {
            if bar.max_scroll_y <= 1.0
                || bar.clip_bounds.width <= 0.0
                || bar.clip_bounds.height <= 0.0
                || !bar.clip_bounds.contains_point(px, py)
            {
                continue;
            }

            // Hit test check with edge-right pixel snapping tolerance (+1.0px for window edge)
            let in_x_thumb =
                px >= bar.thumb_bounds.x && px <= bar.bounds.x + bar.bounds.width + 1.0;
            let in_y_thumb =
                py >= bar.thumb_bounds.y && py <= bar.thumb_bounds.y + bar.thumb_bounds.height;
            let on_thumb = in_x_thumb && in_y_thumb;

            let in_x_track =
                px >= bar.track_bounds.x && px <= bar.bounds.x + bar.bounds.width + 1.0;
            let in_y_track =
                py >= bar.track_bounds.y && py <= bar.track_bounds.y + bar.track_bounds.height;
            let on_track = in_x_track && in_y_track;

            if !(on_thumb || on_track) {
                continue;
            }
            if !self.scrollbar_owns_point(bar, px, py, &mut cached_hit) {
                continue;
            }
            matched = Some((bar.clone(), on_thumb));
            break;
        }

        let Some((bar, on_thumb)) = matched else {
            return false;
        };

        if on_thumb {
            self.active_smooth_scroll = None;
            self.last_scroll_interaction = Some(self.now());
            self.scrollbar_drag = Some(ScrollbarDrag {
                is_id: bar.is_id,
                id: bar.id,
                path: bar.path,
                start_pointer_y: py,
                start_scroll_y: bar.scroll_y,
                available_travel: bar.available_travel,
                max_scroll_y: bar.max_scroll_y,
            });
            true
        } else {
            // Click landed on track gutter (above or below thumb):
            // Jump thumb center to clicked position and start dragging immediately.
            self.active_smooth_scroll = None;
            self.last_scroll_interaction = Some(self.now());
            let click_travel = (py - bar.track_bounds.y - bar.thumb_bounds.height / 2.0)
                .clamp(0.0, bar.available_travel);
            let target_scroll_y = if bar.available_travel > 0.0 {
                (click_travel / bar.available_travel) * bar.max_scroll_y
            } else {
                0.0
            };
            let dy = target_scroll_y - bar.scroll_y;
            let path = bar.path;
            let is_id = bar.is_id;
            let id = bar.id;
            let max_scroll_y = bar.max_scroll_y;
            let available_travel = bar.available_travel;
            let bounds_y = bar.bounds.y;

            self.apply_scroll_delta_to_container(
                &path,
                is_id,
                &id,
                target_scroll_y,
                dy,
                max_scroll_y,
                available_travel,
                bounds_y,
            );

            self.scrollbar_drag = Some(ScrollbarDrag {
                is_id,
                id,
                path,
                start_pointer_y: py,
                start_scroll_y: target_scroll_y,
                available_travel,
                max_scroll_y,
            });
            true
        }
    }

    /// Updates active scrollbar thumb drag during pointer movement.
    pub fn poll_scrollbar_drag(&mut self, _px: f32, py: f32) -> bool {
        let Some(drag) = self.scrollbar_drag.clone() else {
            return false;
        };
        if drag.available_travel <= 0.0 {
            return false;
        }

        let dy_pointer = py - drag.start_pointer_y;
        let delta_scroll = (dy_pointer / drag.available_travel) * drag.max_scroll_y;
        let new_scroll_y = (drag.start_scroll_y + delta_scroll).clamp(0.0, drag.max_scroll_y);

        if let Some((bar_path, cur_y, bounds_y, max_scroll_y, available_travel)) = self
            .active_scrollbars
            .iter()
            .find(|b| Self::scrollbar_drag_matches_bar(&drag, b))
            .map(|b| {
                (
                    b.path.clone(),
                    b.scroll_y,
                    b.bounds.y,
                    b.max_scroll_y,
                    b.available_travel,
                )
            })
        {
            if let Some(ref mut active_drag) = self.scrollbar_drag {
                active_drag.path = bar_path.clone();
            }
            let clamped_scroll_y = new_scroll_y.clamp(0.0, max_scroll_y);
            let dy = clamped_scroll_y - cur_y;
            if dy.abs() > 0.01 {
                self.last_scroll_interaction = Some(self.now());
                self.apply_scroll_delta_to_container(
                    &bar_path,
                    drag.is_id,
                    &drag.id,
                    clamped_scroll_y,
                    dy,
                    max_scroll_y,
                    available_travel,
                    bounds_y,
                );
                return true;
            }
        }

        false
    }
}
