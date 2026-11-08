//! CSS keyframe animations, transitions, view transitions, and scroll animations.

use super::*;

impl AppRuntime {
    /// Synchronizes running animations and transitions with the current layout tree.
    pub fn sync_tree_animations(&mut self, now_ms: f64) {
        let mut new_base_styles = std::collections::HashMap::new();
        let mut new_node_paths = std::collections::HashMap::new();
        let mut path_buf = Vec::new();

        let mut trans_events = Vec::new();
        let mut anim_events = Vec::new();

        Self::sync_tree_animations_rec(
            &self.layout_root,
            &mut path_buf,
            now_ms,
            &self.previous_node_styles,
            &mut new_base_styles,
            &mut new_node_paths,
            &mut self.transition_manager,
            &mut self.animation_manager,
            &mut trans_events,
            &mut anim_events,
        );

        self.base_node_styles = new_base_styles;
        self.previous_node_styles = self.base_node_styles.clone();
        self.node_paths = new_node_paths;

        // Evaluate scroll-driven animations progress
        self.evaluate_scroll_animations(&mut anim_events);

        for ev in trans_events {
            self.dispatch_transition_event(ev);
        }
        for ev in anim_events {
            self.dispatch_animation_event(ev);
        }

        self.apply_animations_to_layout_tree();
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn sync_tree_animations_rec(
        node: &LayoutNode,
        path: &mut Vec<usize>,
        now_ms: f64,
        previous_styles: &std::collections::HashMap<u64, ComputedStyle>,
        new_base_styles: &mut std::collections::HashMap<u64, ComputedStyle>,
        new_node_paths: &mut std::collections::HashMap<u64, Vec<usize>>,
        transition_manager: &mut strata_style::transition::TransitionManager,
        animation_manager: &mut strata_style::animation::AnimationManager,
        trans_events: &mut Vec<strata_style::transition::TransitionLifecycleEvent>,
        anim_events: &mut Vec<strata_style::animation::AnimationLifecycleEvent>,
    ) {
        let node_id = path_to_node_id(path);
        new_base_styles.insert(node_id, node.style.clone());
        new_node_paths.insert(node_id, path.clone());

        // Check for transitions if we have a previous style or @starting-style
        let effective_old_style = match previous_styles.get(&node_id) {
            Some(old) if old.display != strata_style::Display::None => Some(old),
            _ => node.style.starting_style.as_deref(),
        };

        if let Some(old_style) = effective_old_style {
            if !node.style.transitions.is_empty() {
                trans_events.extend(transition_manager.update_styles(
                    node_id,
                    old_style,
                    &node.style,
                    now_ms,
                ));
            }
        }

        // Check for keyframe animations
        if !node.style.animations.is_empty() || animation_manager.has_active_animations() {
            anim_events.extend(animation_manager.update_styles(node_id, &node.style, now_ms));
        }

        for (idx, child) in node.children.iter().enumerate() {
            path.push(idx);
            Self::sync_tree_animations_rec(
                child,
                path,
                now_ms,
                previous_styles,
                new_base_styles,
                new_node_paths,
                transition_manager,
                animation_manager,
                trans_events,
                anim_events,
            );
            path.pop();
        }
    }

    /// Applies active transition and animation values directly onto the layout tree.
    pub fn apply_animations_to_layout_tree(&mut self) {
        let has_anims = self.transition_manager.has_active_transitions()
            || self.animation_manager.has_active_animations();
        if !has_anims && !self.had_active_animations {
            return;
        }
        self.had_active_animations = has_anims;

        let mut path = Vec::new();
        Self::apply_animations_rec(
            &mut self.layout_root,
            &mut path,
            &self.base_node_styles,
            &self.transition_manager,
            &self.animation_manager,
        );
    }

    pub(crate) fn apply_animations_rec(
        node: &mut LayoutNode,
        path: &mut Vec<usize>,
        base_styles: &std::collections::HashMap<u64, ComputedStyle>,
        transition_manager: &strata_style::transition::TransitionManager,
        animation_manager: &strata_style::animation::AnimationManager,
    ) {
        let node_id = path_to_node_id(path);
        if let Some(base) = base_styles.get(&node_id) {
            node.style = base.clone();
        }
        transition_manager.apply_to_style(node_id, &mut node.style);
        animation_manager.apply_to_style(node_id, &mut node.style);

        for (idx, child) in node.children.iter_mut().enumerate() {
            path.push(idx);
            Self::apply_animations_rec(
                child,
                path,
                base_styles,
                transition_manager,
                animation_manager,
            );
            path.pop();
        }
    }

    /// Evaluates all active scroll-driven animations based on current container scroll positions.
    pub fn evaluate_scroll_animations(
        &mut self,
        anim_events: &mut Vec<strata_style::animation::AnimationLifecycleEvent>,
    ) {
        for (node_id, path) in &self.node_paths {
            if let Some(style) = self.base_node_styles.get(node_id) {
                for anim in &style.animations {
                    match &anim.timeline {
                        strata_style::AnimationTimeline::Scroll(src, _axis) => {
                            let scroll_progress = match src {
                                strata_style::ScrollTimelineSource::Root => self
                                    .active_scrollbars
                                    .iter()
                                    .max_by(|a, b| {
                                        (a.bounds.width * a.bounds.height)
                                            .partial_cmp(&(b.bounds.width * b.bounds.height))
                                            .unwrap_or(std::cmp::Ordering::Equal)
                                    })
                                    .map(|b| {
                                        if b.max_scroll_y > 0.0 {
                                            (b.scroll_y / b.max_scroll_y).clamp(0.0, 1.0)
                                        } else {
                                            0.0
                                        }
                                    })
                                    .unwrap_or(0.0),
                                strata_style::ScrollTimelineSource::Nearest
                                | strata_style::ScrollTimelineSource::SelfNode => {
                                    let mut p = 0.0;
                                    let mut check_path = path.clone();
                                    while !check_path.is_empty() {
                                        if let Some((_, sy)) = self.scroll_offsets.get(&check_path)
                                        {
                                            if let Some(bar) = self
                                                .active_scrollbars
                                                .iter()
                                                .find(|b| b.path == check_path)
                                            {
                                                if bar.max_scroll_y > 0.0 {
                                                    p = (*sy / bar.max_scroll_y).clamp(0.0, 1.0);
                                                    break;
                                                }
                                            }
                                        }
                                        check_path.pop();
                                    }
                                    p
                                }
                            };
                            anim_events.extend(self.animation_manager.step_node_scroll_progress(
                                *node_id,
                                scroll_progress,
                                style,
                            ));
                        }
                        strata_style::AnimationTimeline::View(_axis, _start, _end) => {
                            let p = 0.5;
                            anim_events.extend(
                                self.animation_manager
                                    .step_node_scroll_progress(*node_id, p, style),
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// Evaluates scroll-driven animations synchronously during container scroll, consuming 0% CPU when stationary.
    pub fn sync_scroll_animations(&mut self) {
        let mut anim_events = Vec::new();
        self.evaluate_scroll_animations(&mut anim_events);
        for ev in anim_events {
            self.dispatch_animation_event(ev);
        }
        self.apply_animations_to_layout_tree();
    }

    /// Captures element layout bounding boxes for elements with `view-transition-name` before DOM mutation.
    pub fn capture_view_transitions(&mut self) {
        let mut named = Vec::new();
        Self::collect_view_transition_elements(&self.layout_root, &mut named);
        self.view_transition_manager.capture_old_state(&named);
    }

    /// Triggers a view transition after DOM mutation, pairing old bounds with new layout bounds.
    pub fn start_view_transition(&mut self, duration_ms: f32) {
        let mut named = Vec::new();
        Self::collect_view_transition_elements(&self.layout_root, &mut named);
        let now_ms = self.runtime_start_epoch.elapsed().as_secs_f64() * 1000.0;
        self.view_transition_manager
            .start_transition(&named, now_ms, duration_ms);
    }

    pub(crate) fn collect_view_transition_elements(
        node: &LayoutNode,
        out: &mut Vec<(String, strata_primitives::geom::Rect)>,
    ) {
        if let Some(name) = &node.style.view_transition_name {
            if name != "none" {
                out.push((name.clone(), node.geometry.border_box.into()));
            }
        }
        for child in &node.children {
            Self::collect_view_transition_elements(child, out);
        }
    }

    /// Returns true if there are any active transitions or keyframe animations running.
    pub fn has_active_animations(&self) -> bool {
        self.transition_manager.has_active_transitions()
            || self.animation_manager.has_time_based_animations()
            || self.view_transition_manager.has_active_transitions()
    }

    /// Advances active animations and transitions by current clock time.
    /// Returns true if any animation stepped and a redraw is needed.
    pub fn poll_animations(&mut self) -> bool {
        if !self.has_active_animations() {
            return false;
        }

        // Capture this before advancing: a transition or animation that finishes on this
        // tick is removed/settled by `advance`, but the final layout value it produced still
        // has to be laid out once.
        let layout_affecting = self.transition_manager.has_layout_affecting_transitions()
            || self.animation_manager.has_layout_affecting_animations();

        let now_ms = self.runtime_start_epoch.elapsed().as_secs_f64() * 1000.0;
        let trans_events = self.transition_manager.advance(now_ms);
        let anim_events = self
            .animation_manager
            .advance(now_ms, &self.base_node_styles);
        let _vt_active = self.view_transition_manager.step(now_ms);

        for ev in trans_events {
            self.dispatch_transition_event(ev);
        }
        for ev in anim_events {
            self.dispatch_animation_event(ev);
        }

        self.apply_animations_to_layout_tree();

        // Paint-only animations (colour, opacity, transform, shadow, ...) write new style
        // values onto the existing layout tree. Only properties that feed box layout require
        // a fresh layout pass; everything else just needs the display list recompiled.
        if layout_affecting {
            self.tick_frame();
        } else {
            self.compile_display_list();
        }

        true
    }

    /// Dispatches a synthetic W3C animation lifecycle event to matching RSX element event listeners.
    pub fn dispatch_animation_event(
        &self,
        event: strata_style::animation::AnimationLifecycleEvent,
    ) {
        let (node_id, name, elapsed, event_type) = match event {
            strata_style::animation::AnimationLifecycleEvent::Start {
                node_id,
                name,
                elapsed_time_sec,
            } => (
                node_id,
                name,
                elapsed_time_sec,
                strata_dom::EventType::AnimationStart,
            ),
            strata_style::animation::AnimationLifecycleEvent::Iteration {
                node_id,
                name,
                elapsed_time_sec,
            } => (
                node_id,
                name,
                elapsed_time_sec,
                strata_dom::EventType::AnimationIteration,
            ),
            strata_style::animation::AnimationLifecycleEvent::End {
                node_id,
                name,
                elapsed_time_sec,
            } => (
                node_id,
                name,
                elapsed_time_sec,
                strata_dom::EventType::AnimationEnd,
            ),
            strata_style::animation::AnimationLifecycleEvent::Cancel {
                node_id,
                name,
                elapsed_time_sec,
            } => (
                node_id,
                name,
                elapsed_time_sec,
                strata_dom::EventType::AnimationCancel,
            ),
        };

        if let Some(path) = self.node_paths.get(&node_id) {
            if let Some(bp) = Self::find_blueprint_at_path(&self.root_blueprint, path) {
                if let NodeBlueprint::Element { attributes, .. } = bp {
                    for attr in attributes {
                        if let strata_dom::Attribute::EventListener {
                            event: ev_type,
                            handler,
                        } = attr
                        {
                            if *ev_type == event_type {
                                let ev =
                                    strata_dom::Event::animation(event_type, name.clone(), elapsed);
                                handler(ev);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Dispatches a synthetic W3C transition lifecycle event to matching RSX element event listeners.
    pub fn dispatch_transition_event(
        &self,
        event: strata_style::transition::TransitionLifecycleEvent,
    ) {
        let (node_id, name, elapsed, event_type) = match event {
            strata_style::transition::TransitionLifecycleEvent::Run {
                node_id,
                property,
                elapsed_time_sec,
            } => (
                node_id,
                property,
                elapsed_time_sec,
                strata_dom::EventType::TransitionRun,
            ),
            strata_style::transition::TransitionLifecycleEvent::Start {
                node_id,
                property,
                elapsed_time_sec,
            } => (
                node_id,
                property,
                elapsed_time_sec,
                strata_dom::EventType::TransitionStart,
            ),
            strata_style::transition::TransitionLifecycleEvent::End {
                node_id,
                property,
                elapsed_time_sec,
            } => (
                node_id,
                property,
                elapsed_time_sec,
                strata_dom::EventType::TransitionEnd,
            ),
            strata_style::transition::TransitionLifecycleEvent::Cancel {
                node_id,
                property,
                elapsed_time_sec,
            } => (
                node_id,
                property,
                elapsed_time_sec,
                strata_dom::EventType::TransitionCancel,
            ),
        };

        if let Some(path) = self.node_paths.get(&node_id) {
            if let Some(bp) = Self::find_blueprint_at_path(&self.root_blueprint, path) {
                if let NodeBlueprint::Element { attributes, .. } = bp {
                    for attr in attributes {
                        if let strata_dom::Attribute::EventListener {
                            event: ev_type,
                            handler,
                        } = attr
                        {
                            if *ev_type == event_type {
                                let ev = strata_dom::Event::transition(
                                    event_type,
                                    name.clone(),
                                    elapsed,
                                );
                                handler(ev);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Registers all parsed `@keyframes` rules from a stylesheet into the runtime animation registry.
    pub fn register_keyframes_stylesheet(&mut self, sheet: &strata_style::StyleSheet) {
        self.animation_manager
            .registry_mut()
            .register_stylesheet(sheet);
    }

    /// Registers a single compiled `@keyframes` rule into the runtime animation registry.
    pub fn register_keyframe_rule(&mut self, rule: &strata_style::KeyframeRule) {
        self.animation_manager.registry_mut().register_rule(rule);
    }

}
