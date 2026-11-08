//! Viewport clamping, scroll offsets, smooth scrolling, and scroll event dispatch.

use super::*;

impl AppRuntime {
    /// Shifts every `data-clamp-viewport` element (popups such as context menus) back inside the
    /// viewport with an 8px margin, using the live viewport size rather than any hardcoded value.
    pub fn apply_viewport_clamps(&mut self) {
        let (vw, vh) = (self.viewport_width, self.viewport_height);
        Self::clamp_walk(&self.root_blueprint, &mut self.layout_root, vw, vh);
    }

    pub(crate) fn clamp_walk(blueprint: &NodeBlueprint, layout: &mut LayoutNode, vw: f32, vh: f32) {
        const MARGIN: f32 = 8.0;
        match blueprint {
            NodeBlueprint::Element {
                attributes,
                children,
                ..
            } => {
                let clamp = attributes.iter().any(|a| {
                    matches!(a, strata_dom::Attribute::Property { key, .. } if key == "data-clamp-viewport")
                });
                if clamp {
                    let b = layout.geometry.border_box;
                    let mut dx = 0.0;
                    let mut dy = 0.0;
                    if b.x + b.width > vw - MARGIN {
                        dx = vw - MARGIN - b.width - b.x;
                    }
                    if b.x + dx < MARGIN {
                        dx = MARGIN - b.x;
                    }
                    if b.y + b.height > vh - MARGIN {
                        dy = vh - MARGIN - b.height - b.y;
                    }
                    if b.y + dy < MARGIN {
                        dy = MARGIN - b.y;
                    }
                    strata_layout::box_model::translate_node_and_descendants(layout, dx, dy);
                    return;
                }
                let mut flat_children = Vec::new();
                for c in children {
                    Self::flatten_blueprints(c, &mut flat_children);
                }
                for (&child_bp, child_layout) in
                    flat_children.iter().zip(layout.children.iter_mut())
                {
                    Self::clamp_walk(child_bp, child_layout, vw, vh);
                }
            }
            NodeBlueprint::Fragment(children) => {
                for (child_bp, child_layout) in children.iter().zip(layout.children.iter_mut()) {
                    Self::clamp_walk(child_bp, child_layout, vw, vh);
                }
            }
            _ => {}
        }
    }

    /// Traverses the LayoutNode hierarchy down a slice of child indices in O(depth).
    pub fn get_layout_node_mut<'a>(
        root: &'a mut LayoutNode,
        path: &[usize],
    ) -> Option<&'a mut LayoutNode> {
        let mut current = root;
        for &idx in path {
            if idx < current.children.len() {
                current = &mut current.children[idx];
            } else {
                return None;
            }
        }
        Some(current)
    }

    /// Restores any scrollable containers previously translated by `apply_scroll_offsets` back to their
    /// baseline unshifted coordinates so layout computations and metric measurements are invariant.
    pub fn unapply_scroll_offsets(&mut self) {
        if self.applied_scroll_offsets.is_empty() {
            return;
        }
        for (path, &(applied_x, applied_y)) in &self.applied_scroll_offsets {
            if applied_x.abs() > 0.0001 || applied_y.abs() > 0.0001 {
                if let Some(target) = Self::get_layout_node_mut(&mut self.layout_root, path) {
                    for child in &mut target.children {
                        child.translate(applied_x, applied_y);
                    }
                    for line in &mut target.line_boxes {
                        line.bounds.x += applied_x;
                        line.bounds.y += applied_y;
                        for frag in &mut line.text_fragments {
                            frag.bounds.x += applied_x;
                            frag.bounds.y += applied_y;
                            frag.baseline_y += applied_y;
                        }
                        for atomic in &mut line.atomic_fragments {
                            atomic.bounds.x += applied_x;
                            atomic.bounds.y += applied_y;
                        }
                    }
                }
            }
        }
        self.applied_scroll_offsets.clear();
    }

    /// Discovers scrollable containers, clamps offsets, translates children, and computes scrollbars.
    pub fn apply_scroll_offsets(&mut self) {
        self.active_scrollbars.clear();
        let mut path = Vec::new();
        Self::apply_scroll_walk(
            &self.root_blueprint,
            &mut self.layout_root,
            &mut path,
            &self.scroll_offsets,
            &self.scroll_offsets_by_id,
            &mut self.applied_scroll_offsets,
            &mut self.active_scrollbars,
        );
        self.sync_scroll_animations();
    }

    pub(crate) fn apply_scroll_walk(
        blueprint: &NodeBlueprint,
        layout: &mut LayoutNode,
        current_path: &mut Vec<usize>,
        scroll_offsets: &std::collections::HashMap<Vec<usize>, (f32, f32)>,
        scroll_offsets_by_id: &std::collections::HashMap<String, (f32, f32)>,
        applied_scroll_offsets: &mut std::collections::HashMap<Vec<usize>, (f32, f32)>,
        active_scrollbars: &mut Vec<ActiveScrollbar>,
    ) {
        if matches!(layout.style.display, strata_style::types::Display::None) {
            return;
        }

        let is_scroll_y = matches!(
            layout.style.overflow_y,
            strata_style::types::Overflow::Auto | strata_style::types::Overflow::Scroll
        );
        let is_scroll_x = matches!(
            layout.style.overflow_x,
            strata_style::types::Overflow::Auto | strata_style::types::Overflow::Scroll
        );

        let elem_id = match blueprint {
            NodeBlueprint::Element { attributes, .. } => attributes.iter().find_map(|a| {
                if let strata_dom::Attribute::Property { key, value } = a {
                    if key == "id" {
                        Some(value.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            }),
            _ => None,
        };

        if is_scroll_y || is_scroll_x {
            let viewport_w = layout.geometry.content_box.width;
            let viewport_h = layout.geometry.content_box.height;

            let mut max_child_x = layout.geometry.content_box.x;
            let mut max_child_y = layout.geometry.content_box.y;

            for child in &layout.children {
                if child.style.position == strata_style::types::Position::Absolute
                    || child.style.position == strata_style::types::Position::Fixed
                {
                    continue;
                }
                max_child_x =
                    max_child_x.max(child.geometry.border_box.x + child.geometry.border_box.width);
                max_child_y =
                    max_child_y.max(child.geometry.border_box.y + child.geometry.border_box.height);
            }
            for line in &layout.line_boxes {
                max_child_x = max_child_x.max(line.bounds.x + line.bounds.width);
                max_child_y = max_child_y.max(line.bounds.y + line.bounds.height);
            }

            let content_w = (max_child_x - layout.geometry.content_box.x).max(0.0);
            let content_h = (max_child_y - layout.geometry.content_box.y).max(0.0);
            let max_scroll_x = (content_w - viewport_w).max(0.0);
            let max_scroll_y = (content_h - viewport_h).max(0.0);

            let (stored_x, stored_y) = if let Some(ref id) = elem_id {
                scroll_offsets_by_id
                    .get(id)
                    .copied()
                    .or_else(|| scroll_offsets.get(current_path).copied())
                    .unwrap_or((0.0, 0.0))
            } else {
                scroll_offsets
                    .get(current_path)
                    .copied()
                    .unwrap_or((0.0, 0.0))
            };

            let clamped_x = stored_x.clamp(0.0, max_scroll_x);
            let clamped_y = stored_y.clamp(0.0, max_scroll_y);

            if clamped_x > 0.0 || clamped_y > 0.0 {
                for child in &mut layout.children {
                    child.translate(-clamped_x, -clamped_y);
                }
                for line in &mut layout.line_boxes {
                    line.bounds.x -= clamped_x;
                    line.bounds.y -= clamped_y;
                    for frag in &mut line.text_fragments {
                        frag.bounds.x -= clamped_x;
                        frag.bounds.y -= clamped_y;
                        frag.baseline_y -= clamped_y;
                    }
                    for atomic in &mut line.atomic_fragments {
                        atomic.bounds.x -= clamped_x;
                        atomic.bounds.y -= clamped_y;
                    }
                }
                applied_scroll_offsets.insert(current_path.clone(), (clamped_x, clamped_y));
            }

            if is_scroll_y && max_scroll_y > 1.0 && viewport_h > 20.0 {
                let border_box = layout.geometry.border_box;
                let track_width = 10.0;
                let track_x = border_box.x + border_box.width - track_width;
                let track_y = border_box.y;
                let track_h = border_box.height;
                let track_bounds = strata_layout::Rect::new(track_x, track_y, track_width, track_h);

                let thumb_h = ((viewport_h / content_h) * track_h).clamp(24.0, track_h);
                let available_travel = (track_h - thumb_h).max(0.0);
                let thumb_y = if max_scroll_y > 0.0 && available_travel > 0.0 {
                    track_y + (clamped_y / max_scroll_y) * available_travel
                } else {
                    track_y
                };

                let thumb_bounds = strata_layout::Rect::new(track_x, thumb_y, track_width, thumb_h);
                active_scrollbars.push(ActiveScrollbar {
                    bounds: border_box,
                    track_bounds,
                    thumb_bounds,
                    scroll_y: clamped_y,
                    max_scroll_y,
                    content_h,
                    is_id: elem_id.is_some(),
                    id: elem_id.clone().unwrap_or_default(),
                    path: current_path.clone(),
                    available_travel,
                });
            }
        }

        // Recursively walk children
        match blueprint {
            NodeBlueprint::Element { children, .. } => {
                let mut flat_children = Vec::new();
                for c in children {
                    Self::flatten_blueprints(c, &mut flat_children);
                }
                for (i, (&child_bp, child_layout)) in flat_children
                    .iter()
                    .zip(layout.children.iter_mut())
                    .enumerate()
                {
                    current_path.push(i);
                    Self::apply_scroll_walk(
                        child_bp,
                        child_layout,
                        current_path,
                        scroll_offsets,
                        scroll_offsets_by_id,
                        applied_scroll_offsets,
                        active_scrollbars,
                    );
                    current_path.pop();
                }
            }
            NodeBlueprint::Fragment(children) => {
                let mut flat_children = Vec::new();
                for c in children {
                    Self::flatten_blueprints(c, &mut flat_children);
                }
                for (i, (&child_bp, child_layout)) in flat_children
                    .iter()
                    .zip(layout.children.iter_mut())
                    .enumerate()
                {
                    current_path.push(i);
                    Self::apply_scroll_walk(
                        child_bp,
                        child_layout,
                        current_path,
                        scroll_offsets,
                        scroll_offsets_by_id,
                        applied_scroll_offsets,
                        active_scrollbars,
                    );
                    current_path.pop();
                }
            }
            _ => {}
        }
    }

    /// Helper translating children and line fragments in an overflowing container by `-dy` and recording offsets.
    pub(crate) fn apply_scroll_delta_to_container(
        &mut self,
        path: &[usize],
        is_id: bool,
        id: &str,
        new_scroll_y: f32,
        dy: f32,
        max_scroll_y: f32,
        available_travel: f32,
        bounds_y: f32,
    ) {
        if is_id {
            self.scroll_offsets_by_id
                .insert(id.to_string(), (0.0, new_scroll_y));
        }
        self.scroll_offsets
            .insert(path.to_vec(), (0.0, new_scroll_y));
        self.applied_scroll_offsets
            .insert(path.to_vec(), (0.0, new_scroll_y));

        if let Some(b) = self.active_scrollbars.iter_mut().find(|b| b.path == path) {
            b.scroll_y = new_scroll_y;
            if max_scroll_y > 0.0 && available_travel > 0.0 {
                b.thumb_bounds.y = bounds_y + (new_scroll_y / max_scroll_y) * available_travel;
            }
        }

        if let Some(target) = Self::get_layout_node_mut(&mut self.layout_root, path) {
            for child in &mut target.children {
                child.translate(0.0, -dy);
            }
            for line in &mut target.line_boxes {
                line.bounds.y -= dy;
                for frag in &mut line.text_fragments {
                    frag.bounds.y -= dy;
                    frag.baseline_y -= dy;
                }
                for atomic in &mut line.atomic_fragments {
                    atomic.bounds.y -= dy;
                }
            }
        }
    }

    /// Dispatches an immediate synchronous scroll wheel delta to the innermost scrollable container under (px, py).
    /// Cancels any active kinetic smooth scroll animation.
    /// Returns true if a scroll offset changed and redraw is required.
    pub fn dispatch_scroll(&mut self, px: f32, py: f32, _delta_x: f32, delta_y: f32) -> bool {
        self.last_mouse_pos = (px, py);
        self.active_smooth_scroll = None;

        let mut matching_bars: Vec<ActiveScrollbar> = self
            .active_scrollbars
            .iter()
            .filter(|bar| bar.bounds.contains_point(px, py))
            .cloned()
            .collect();

        // If mouse is not directly over an active scroll container, fallback to the largest scrollable container
        if matching_bars.is_empty() {
            if let Some(largest) = self.active_scrollbars.iter().max_by(|a, b| {
                (a.bounds.width * a.bounds.height)
                    .partial_cmp(&(b.bounds.width * b.bounds.height))
                    .unwrap_or(std::cmp::Ordering::Equal)
            }) {
                matching_bars.push(largest.clone());
            }
        }

        for bar in matching_bars.into_iter().rev() {
            if bar.max_scroll_y <= 0.0 {
                continue;
            }

            let cur_y = if bar.is_id {
                self.scroll_offsets_by_id
                    .get(&bar.id)
                    .copied()
                    .or_else(|| self.scroll_offsets.get(&bar.path).copied())
                    .map(|(_, y)| y)
                    .unwrap_or(bar.scroll_y)
            } else {
                self.scroll_offsets
                    .get(&bar.path)
                    .copied()
                    .map(|(_, y)| y)
                    .unwrap_or(bar.scroll_y)
            };

            let candidate_y = (cur_y + delta_y).clamp(0.0, bar.max_scroll_y);
            let dy = candidate_y - cur_y;
            if dy.abs() > 0.01 {
                self.last_scroll_interaction = Some(std::time::Instant::now());
                self.apply_scroll_delta_to_container(
                    &bar.path,
                    bar.is_id,
                    &bar.id,
                    candidate_y,
                    dy,
                    bar.max_scroll_y,
                    bar.available_travel,
                    bar.bounds.y,
                );
                return true;
            }
        }

        false
    }

    /// Dispatches a smooth kinetic scroll impulse to the innermost scrollable container under (px, py).
    /// Accumulates velocity for rapid consecutive wheel events and initiates a critically damped
    /// kinetic transition curve with Chromium Blink parity.
    pub fn dispatch_smooth_scroll(
        &mut self,
        px: f32,
        py: f32,
        _delta_x: f32,
        delta_y: f32,
    ) -> bool {
        self.last_mouse_pos = (px, py);
        if delta_y.abs() < 0.01 {
            return false;
        }

        let mut matching_bars: Vec<ActiveScrollbar> = self
            .active_scrollbars
            .iter()
            .filter(|bar| bar.bounds.contains_point(px, py))
            .cloned()
            .collect();

        if matching_bars.is_empty() {
            if let Some(largest) = self.active_scrollbars.iter().max_by(|a, b| {
                (a.bounds.width * a.bounds.height)
                    .partial_cmp(&(b.bounds.width * b.bounds.height))
                    .unwrap_or(std::cmp::Ordering::Equal)
            }) {
                matching_bars.push(largest.clone());
            }
        }

        // Scroll chaining: target the innermost container that can still absorb the delta.
        // When every matching container is already at its limit, fall back to the outermost
        // container so the event is not trapped by the innermost saturated scroller.
        let target_bar = matching_bars
            .iter()
            .rev()
            .find(|b| {
                if b.max_scroll_y <= 0.0 {
                    return false;
                }
                let cur_y = if b.is_id {
                    self.scroll_offsets_by_id
                        .get(&b.id)
                        .copied()
                        .or_else(|| self.scroll_offsets.get(&b.path).copied())
                        .map(|(_, y)| y)
                        .unwrap_or(b.scroll_y)
                } else {
                    self.scroll_offsets
                        .get(&b.path)
                        .copied()
                        .map(|(_, y)| y)
                        .unwrap_or(b.scroll_y)
                };
                if delta_y < 0.0 {
                    cur_y > 0.01
                } else {
                    cur_y < b.max_scroll_y - 0.01
                }
            })
            .cloned()
            .or_else(|| matching_bars.iter().find(|b| b.max_scroll_y > 0.0).cloned());

        let Some(bar) = target_bar else {
            return false;
        };

        let now = std::time::Instant::now();
        self.last_scroll_interaction = Some(now);

        let cur_y = if bar.is_id {
            self.scroll_offsets_by_id
                .get(&bar.id)
                .copied()
                .or_else(|| self.scroll_offsets.get(&bar.path).copied())
                .map(|(_, y)| y)
                .unwrap_or(bar.scroll_y)
        } else {
            self.scroll_offsets
                .get(&bar.path)
                .copied()
                .map(|(_, y)| y)
                .unwrap_or(bar.scroll_y)
        };

        if let Some(ref mut tracker) = self.active_smooth_scroll {
            if tracker.container_path == bar.path {
                let dt_flick = now.duration_since(tracker.last_event_time).as_secs_f32();
                let accel = if dt_flick < 0.20 {
                    (1.0 + (0.12 / dt_flick.max(0.01)).min(2.5)).min(3.5)
                } else {
                    1.0
                };

                let t_prev = now.duration_since(tracker.start_time).as_secs_f32();
                let omega = tracker.omega;
                let e0 = tracker.start_y - tracker.target_y;
                let c2 = tracker.initial_velocity + omega * e0;
                let decay = (-omega * t_prev).exp();
                let y_now =
                    (tracker.target_y + (e0 + c2 * t_prev) * decay).clamp(0.0, bar.max_scroll_y);
                let v_now = (tracker.initial_velocity - omega * c2 * t_prev) * decay;

                let impulse = delta_y * accel;
                let new_target = (tracker.target_y + impulse).clamp(0.0, bar.max_scroll_y);

                tracker.start_y = y_now;
                tracker.current_y = y_now;
                tracker.initial_velocity = v_now;
                tracker.target_y = new_target;
                tracker.start_time = now;
                tracker.last_event_time = now;
                tracker.max_scroll_y = bar.max_scroll_y;
                tracker.available_travel = bar.available_travel;
                tracker.bounds_y = bar.bounds.y;
                return true;
            }
        }

        let new_target = (cur_y + delta_y).clamp(0.0, bar.max_scroll_y);
        self.active_smooth_scroll = Some(SmoothScrollTracker {
            container_path: bar.path,
            container_id: bar.id,
            is_id: bar.is_id,
            max_scroll_y: bar.max_scroll_y,
            available_travel: bar.available_travel,
            bounds_y: bar.bounds.y,
            current_y: cur_y,
            target_y: new_target,
            start_y: cur_y,
            initial_velocity: 0.0,
            start_time: now,
            last_event_time: now,
            omega: 18.0,
        });

        true
    }

    /// Polls and updates active kinetic smooth scroll animation at a specific timestamp.
    /// Returns true if a position changed and a redraw is required.
    pub fn poll_smooth_scroll_at(&mut self, now: std::time::Instant) -> bool {
        let Some(mut tracker) = self.active_smooth_scroll.take() else {
            return false;
        };

        let t = now.duration_since(tracker.start_time).as_secs_f32();
        let omega = tracker.omega;
        let e0 = tracker.start_y - tracker.target_y;
        let c2 = tracker.initial_velocity + omega * e0;
        let decay = (-omega * t).exp();

        let raw_y = tracker.target_y + (e0 + c2 * t) * decay;
        let clamped_y = raw_y.clamp(0.0, tracker.max_scroll_y);
        let vel = (tracker.initial_velocity - omega * c2 * t) * decay;

        let is_settled =
            t >= 0.40 || ((clamped_y - tracker.target_y).abs() < 0.25 && vel.abs() < 1.0);
        let new_y = if is_settled {
            tracker.target_y
        } else {
            clamped_y
        };

        let dy = new_y - tracker.current_y;
        let mut redraw = false;

        if dy.abs() > 0.001 {
            self.apply_scroll_delta_to_container(
                &tracker.container_path,
                tracker.is_id,
                &tracker.container_id,
                new_y,
                dy,
                tracker.max_scroll_y,
                tracker.available_travel,
                tracker.bounds_y,
            );
            tracker.current_y = new_y;
            redraw = true;
        }

        if !is_settled {
            self.active_smooth_scroll = Some(tracker);
            redraw = true;
        }

        redraw
    }

    /// Evaluates active smooth scroll animation at the current instant.
    #[inline]
    pub fn poll_smooth_scroll(&mut self) -> bool {
        self.poll_smooth_scroll_at(std::time::Instant::now())
    }

    /// Scrolls the innermost scroll container under (px, py) to the very top.
    pub fn scroll_to_top(&mut self, px: f32, py: f32) -> bool {
        self.active_smooth_scroll = None;
        let mut matching_bars: Vec<ActiveScrollbar> = self
            .active_scrollbars
            .iter()
            .filter(|bar| bar.bounds.contains_point(px, py))
            .cloned()
            .collect();

        if matching_bars.is_empty() {
            if let Some(largest) = self.active_scrollbars.iter().max_by(|a, b| {
                (a.bounds.width * a.bounds.height)
                    .partial_cmp(&(b.bounds.width * b.bounds.height))
                    .unwrap_or(std::cmp::Ordering::Equal)
            }) {
                matching_bars.push(largest.clone());
            }
        }

        if let Some(bar) = matching_bars.into_iter().next_back() {
            let cur_y = bar.scroll_y;
            let dy = 0.0 - cur_y;
            if dy.abs() > 0.01 {
                self.last_scroll_interaction = Some(std::time::Instant::now());
                self.apply_scroll_delta_to_container(
                    &bar.path,
                    bar.is_id,
                    &bar.id,
                    0.0,
                    dy,
                    bar.max_scroll_y,
                    bar.available_travel,
                    bar.bounds.y,
                );
                return true;
            }
        }
        false
    }

    /// Scrolls the innermost scroll container under (px, py) to the very bottom.
    pub fn scroll_to_bottom(&mut self, px: f32, py: f32) -> bool {
        self.active_smooth_scroll = None;
        let mut matching_bars: Vec<ActiveScrollbar> = self
            .active_scrollbars
            .iter()
            .filter(|bar| bar.bounds.contains_point(px, py))
            .cloned()
            .collect();

        if matching_bars.is_empty() {
            if let Some(largest) = self.active_scrollbars.iter().max_by(|a, b| {
                (a.bounds.width * a.bounds.height)
                    .partial_cmp(&(b.bounds.width * b.bounds.height))
                    .unwrap_or(std::cmp::Ordering::Equal)
            }) {
                matching_bars.push(largest.clone());
            }
        }

        if let Some(bar) = matching_bars.into_iter().next_back() {
            if bar.max_scroll_y > 0.0 {
                let cur_y = bar.scroll_y;
                let dy = bar.max_scroll_y - cur_y;
                if dy.abs() > 0.01 {
                    self.last_scroll_interaction = Some(std::time::Instant::now());
                    self.apply_scroll_delta_to_container(
                        &bar.path,
                        bar.is_id,
                        &bar.id,
                        bar.max_scroll_y,
                        dy,
                        bar.max_scroll_y,
                        bar.available_travel,
                        bar.bounds.y,
                    );
                    return true;
                }
            }
        }
        false
    }

}
