//! Visual scrollbar thumb rendering and drag interaction handling.

use super::*;

impl AppRuntime {
    /// Renders sleek semi-transparent scrollbar thumbs for all overflowing scroll containers.
    pub fn render_scrollbars(&mut self) {
        let (mx, my) = self.last_mouse_pos;
        let is_dragging = self.scrollbar_drag.is_some();
        let is_kinetic = self.active_smooth_scroll.is_some();
        let is_recent_scroll = self
            .last_scroll_interaction
            .is_some_and(|t| t.elapsed().as_millis() < 700);

        for bar in &self.active_scrollbars {
            if bar.max_scroll_y <= 1.0 {
                continue;
            }
            let is_hovered = bar.thumb_bounds.contains_point(mx, my)
                || bar.track_bounds.contains_point(mx, my)
                || (mx >= bar.track_bounds.x
                    && mx <= bar.bounds.x + bar.bounds.width + 1.0
                    && my >= bar.track_bounds.y
                    && my <= bar.track_bounds.y + bar.track_bounds.height);
            let is_active = is_dragging || is_hovered || is_kinetic || is_recent_scroll;
            let alpha = if is_active { 150u8 } else { 60u8 };

            let mut thumb_bounds = bar.thumb_bounds;
            if is_active {
                let expand_px = 2.0;
                thumb_bounds.x = (thumb_bounds.x - expand_px).max(bar.bounds.x);
                thumb_bounds.width += expand_px;
            }

            self.display_list
                .push(strata_render::compositor::DrawCommand::DrawRoundedRect {
                    bounds: thumb_bounds,
                    color: strata_style::types::Color::rgba(255, 255, 255, alpha),
                    border_radius: strata_style::types::BorderRadius::all(3.0),
                    border_width: strata_style::types::EdgeInsets::all(0.0),
                    border_color: None,
                });
        }
    }

    /// Checks if a pointer press landed on any active scrollbar thumb or track and starts dragging.
    pub fn check_scrollbar_thumb_press(&mut self, px: f32, py: f32) -> bool {
        for bar in &self.active_scrollbars {
            if bar.max_scroll_y <= 1.0 {
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

            if on_thumb {
                self.active_smooth_scroll = None;
                self.last_scroll_interaction = Some(std::time::Instant::now());
                self.scrollbar_drag = Some(ScrollbarDrag {
                    is_id: bar.is_id,
                    id: bar.id.clone(),
                    path: bar.path.clone(),
                    start_pointer_y: py,
                    start_scroll_y: bar.scroll_y,
                    available_travel: bar.available_travel,
                    max_scroll_y: bar.max_scroll_y,
                });
                return true;
            } else if on_track {
                // Click landed on track gutter (above or below thumb):
                // Jump thumb center to clicked position and start dragging immediately.
                self.active_smooth_scroll = None;
                self.last_scroll_interaction = Some(std::time::Instant::now());
                let click_travel = (py - bar.track_bounds.y - bar.thumb_bounds.height / 2.0)
                    .clamp(0.0, bar.available_travel);
                let target_scroll_y = if bar.available_travel > 0.0 {
                    (click_travel / bar.available_travel) * bar.max_scroll_y
                } else {
                    0.0
                };
                let dy = target_scroll_y - bar.scroll_y;
                let path = bar.path.clone();
                let is_id = bar.is_id;
                let id = bar.id.clone();
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
                return true;
            }
        }
        false
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

        if let Some(bar) = self.active_scrollbars.iter().find(|b| b.path == drag.path) {
            let cur_y = bar.scroll_y;
            let dy = new_scroll_y - cur_y;
            if dy.abs() > 0.01 {
                self.last_scroll_interaction = Some(std::time::Instant::now());
                self.apply_scroll_delta_to_container(
                    &drag.path,
                    drag.is_id,
                    &drag.id,
                    new_scroll_y,
                    dy,
                    drag.max_scroll_y,
                    drag.available_travel,
                    bar.bounds.y,
                );
                return true;
            }
        }

        false
    }

}
