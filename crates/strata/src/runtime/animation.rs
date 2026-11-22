//! CSS keyframe animations, transitions, view transitions, and scroll animations.

use super::*;

fn fnv1a_mix(mut hash: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3u64);
    }
    hash
}

fn blueprint_has_explicit_identity(bp: Option<&NodeBlueprint>) -> bool {
    let Some(NodeBlueprint::Element { attributes, .. }) = bp else {
        return false;
    };
    attributes.iter().any(|attr| {
        matches!(
            attr,
            strata_dom::Attribute::Property { key, .. }
                if key == "id" || key == "key" || key == "data-key"
        )
    })
}

fn blueprint_node_identity_hash(
    parent_hash: u64,
    child_index: usize,
    bp: Option<&NodeBlueprint>,
) -> u64 {
    let mut hash = fnv1a_mix(parent_hash, &child_index.to_le_bytes());
    match bp {
        Some(NodeBlueprint::Element {
            tag, attributes, ..
        }) => {
            hash = fnv1a_mix(hash, &[1]);
            let mut tag_hasher = std::collections::hash_map::DefaultHasher::new();
            std::hash::Hash::hash(tag, &mut tag_hasher);
            hash = fnv1a_mix(hash, &std::hash::Hasher::finish(&tag_hasher).to_le_bytes());
            for attr in attributes {
                if let strata_dom::Attribute::Property { key, value } = attr {
                    if key == "id" || key == "key" || key == "data-key" {
                        hash = fnv1a_mix(hash, key.as_bytes());
                        hash = fnv1a_mix(hash, &[0xff]);
                        hash = fnv1a_mix(hash, value.as_bytes());
                        hash = fnv1a_mix(hash, &[0xfe]);
                    }
                }
            }
        }
        Some(NodeBlueprint::Text(_)) => {
            hash = fnv1a_mix(hash, &[2]);
        }
        Some(NodeBlueprint::DynamicText(_)) => {
            hash = fnv1a_mix(hash, &[3]);
        }
        Some(NodeBlueprint::Fragment(_)) => {
            hash = fnv1a_mix(hash, &[4]);
        }
        Some(NodeBlueprint::Empty) => {
            hash = fnv1a_mix(hash, &[5]);
        }
        None => {
            hash = fnv1a_mix(hash, &[0]);
        }
    }
    hash
}

impl AppRuntime {
    /// Synchronizes running animations and transitions with the current layout tree.
    pub fn sync_tree_animations(&mut self, now_ms: f64) {
        let mut new_base_styles = std::collections::HashMap::new();
        let mut new_node_paths = std::collections::HashMap::new();
        let mut new_path_node_ids = std::collections::HashMap::new();
        let mut path_buf = Vec::new();

        let mut trans_events = Vec::new();
        let mut anim_events = Vec::new();

        Self::sync_tree_animations_rec(
            &self.layout_root,
            Some(&self.root_blueprint),
            0xcbf29ce484222325u64,
            0,
            &mut path_buf,
            now_ms,
            &self.previous_node_styles,
            &mut new_base_styles,
            &mut new_node_paths,
            &mut new_path_node_ids,
            &mut self.transition_manager,
            &mut self.animation_manager,
            &mut trans_events,
            &mut anim_events,
        );

        self.base_node_styles = new_base_styles;
        self.previous_node_styles = self.base_node_styles.clone();
        self.node_paths = new_node_paths;
        self.path_node_ids = new_path_node_ids;

        // Prune any transitions/animations belonging to unmounted or replaced elements
        // so dead nodes never leak state or keep frame ticks alive.
        self.transition_manager
            .retain_nodes(|id| self.base_node_styles.contains_key(id));
        self.animation_manager
            .retain_nodes(|id| self.base_node_styles.contains_key(id));

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
        blueprint: Option<&NodeBlueprint>,
        parent_hash: u64,
        child_index: usize,
        path: &mut Vec<usize>,
        now_ms: f64,
        previous_styles: &std::collections::HashMap<u64, ComputedStyle>,
        new_base_styles: &mut std::collections::HashMap<u64, ComputedStyle>,
        new_node_paths: &mut std::collections::HashMap<u64, Vec<usize>>,
        new_path_node_ids: &mut std::collections::HashMap<Vec<usize>, u64>,
        transition_manager: &mut strata_style::transition::TransitionManager,
        animation_manager: &mut strata_style::animation::AnimationManager,
        trans_events: &mut Vec<strata_style::transition::TransitionLifecycleEvent>,
        anim_events: &mut Vec<strata_style::animation::AnimationLifecycleEvent>,
    ) {
        let node_id = blueprint_node_identity_hash(parent_hash, child_index, blueprint);
        new_base_styles.insert(node_id, node.style.clone());
        new_node_paths.insert(node_id, path.clone());
        new_path_node_ids.insert(path.clone(), node_id);

        // Check for transitions if we have a previous style or @starting-style
        let effective_old_style = match previous_styles.get(&node_id) {
            Some(old) if old.display != strata_style::Display::None => {
                if old.transitions.is_empty()
                    && !node.style.transitions.is_empty()
                    && !blueprint_has_explicit_identity(blueprint)
                {
                    node.style.starting_style.as_deref()
                } else {
                    Some(old)
                }
            }
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

        let bp_children = match blueprint {
            Some(NodeBlueprint::Element { children, .. }) => Some(children.as_slice()),
            Some(NodeBlueprint::Fragment(children)) => Some(children.as_slice()),
            _ => None,
        };

        for (idx, child) in node.children.iter().enumerate() {
            path.push(idx);
            let child_bp = bp_children.and_then(|c| c.get(idx));
            Self::sync_tree_animations_rec(
                child,
                child_bp,
                node_id,
                idx,
                path,
                now_ms,
                previous_styles,
                new_base_styles,
                new_node_paths,
                new_path_node_ids,
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
            &self.path_node_ids,
            &self.base_node_styles,
            &self.transition_manager,
            &self.animation_manager,
            None,
        );
    }

    pub(crate) fn apply_animations_rec(
        node: &mut LayoutNode,
        path: &mut Vec<usize>,
        path_node_ids: &std::collections::HashMap<Vec<usize>, u64>,
        base_styles: &std::collections::HashMap<u64, ComputedStyle>,
        transition_manager: &strata_style::transition::TransitionManager,
        animation_manager: &strata_style::animation::AnimationManager,
        inherited_color_override: Option<(strata_style::types::Color, strata_style::types::Color)>,
    ) {
        let node_id = path_node_ids
            .get(path)
            .copied()
            .unwrap_or_else(|| path_to_node_id(path));
        if let Some(base) = base_styles.get(&node_id) {
            node.style = base.clone();
        }
        let own_base_color = node.style.color;
        if let Some((anc_base_color, anc_anim_color)) = inherited_color_override {
            if own_base_color == anc_base_color {
                node.style.color = anc_anim_color;
            }
        }
        transition_manager.apply_to_style(node_id, &mut node.style);
        animation_manager.apply_to_style(node_id, &mut node.style);
        node.active_transitions = transition_manager.has_node_transitions(node_id);

        let next_color_override = if node.style.color != own_base_color {
            Some((own_base_color, node.style.color))
        } else {
            None
        };

        for (idx, child) in node.children.iter_mut().enumerate() {
            path.push(idx);
            Self::apply_animations_rec(
                child,
                path,
                path_node_ids,
                base_styles,
                transition_manager,
                animation_manager,
                next_color_override,
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

    /// Returns true if any CSS property transitions or view transitions are currently active.
    pub fn has_active_transitions(&self) -> bool {
        self.transition_manager.has_active_transitions()
            || self.view_transition_manager.has_active_transitions()
            || self.layout_root.has_active_transitions()
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

        // Capture layout-affecting node IDs before advancing: a transition or animation that
        // finishes on this tick is removed/settled by `advance`, but the final layout value it
        // produced still has to be laid out once with invalidated ancestor/sibling caches.
        let mut layout_node_ids = self.transition_manager.layout_affecting_node_ids();
        layout_node_ids.extend(self.animation_manager.layout_affecting_node_ids());
        let layout_affecting = !layout_node_ids.is_empty()
            || self.transition_manager.has_layout_affecting_transitions()
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
            self.invalidate_layout_for_node_ids(&layout_node_ids);
            self.tick_frame();
        } else {
            strata_layout::box_model::refresh_inline_fragment_styles(&mut self.layout_root);
            self.compile_display_list();
        }

        true
    }

    /// Invalidates cached layout geometry for each layout-animating node's ancestors and
    /// parent container subtree so flex/grid/percentage siblings reflow smoothly on tick.
    pub(crate) fn invalidate_layout_for_node_ids(&mut self, node_ids: &[u64]) {
        if node_ids.is_empty() {
            self.layout_root.invalidate_cache();
            return;
        }

        let mut invalidated_parents: std::collections::HashSet<Vec<usize>> =
            std::collections::HashSet::with_capacity(node_ids.len());
        for &node_id in node_ids {
            let Some(path) = self.node_paths.get(&node_id).cloned() else {
                self.layout_root.invalidate_cache();
                return;
            };
            self.layout_root.invalidate_ancestors(&path);
            let parent_path: Vec<usize> = if path.is_empty() {
                Vec::new()
            } else {
                path[..path.len() - 1].to_vec()
            };
            if invalidated_parents.insert(parent_path.clone()) {
                if let Some(parent_node) =
                    Self::find_layout_node_mut(&mut self.layout_root, &parent_path)
                {
                    parent_node.invalidate_cache();
                } else {
                    self.layout_root.invalidate_cache();
                    return;
                }
            }
        }
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
