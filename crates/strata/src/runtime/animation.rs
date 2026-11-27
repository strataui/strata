//! CSS keyframe animations, transitions, view transitions, and scroll animations.

use super::*;

fn fnv1a_mix(mut hash: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3u64);
    }
    hash
}

fn blueprint_explicit_identity_attrs(
    bp: Option<&NodeBlueprint>,
) -> Vec<(&str, &str)> {
    let Some(NodeBlueprint::Element { attributes, .. }) = bp else {
        return Vec::new();
    };
    attributes
        .iter()
        .filter_map(|attr| match attr {
            strata_dom::Attribute::Property { key, value }
                if (key == "id" || key == "key" || key == "data-key") && !value.is_empty() =>
            {
                Some((key.as_str(), value.as_str()))
            }
            _ => None,
        })
        .collect()
}

fn blueprint_has_explicit_identity(bp: Option<&NodeBlueprint>) -> bool {
    !blueprint_explicit_identity_attrs(bp).is_empty()
}

fn blueprint_node_identity_hash(
    parent_hash: u64,
    child_index: usize,
    bp: Option<&NodeBlueprint>,
) -> u64 {
    if let Some(NodeBlueprint::Element { tag, .. }) = bp {
        let explicit_keys = blueprint_explicit_identity_attrs(bp);
        if !explicit_keys.is_empty() {
            // Elements with explicit `key`, `data-key`, or `id` derive transition identity
            // strictly from `(parent_hash, tag, key_attrs)` — independent of `child_index` —
            // so list insertions, removals, and reversals keep each item's own transitions
            // instead of cross-animating onto neighboring elements.
            let mut hash = fnv1a_mix(parent_hash, &[0x11]);
            let mut tag_hasher = std::collections::hash_map::DefaultHasher::new();
            std::hash::Hash::hash(tag, &mut tag_hasher);
            hash = fnv1a_mix(hash, &std::hash::Hasher::finish(&tag_hasher).to_le_bytes());
            for (key, value) in explicit_keys {
                hash = fnv1a_mix(hash, key.as_bytes());
                hash = fnv1a_mix(hash, &[0xff]);
                hash = fnv1a_mix(hash, value.as_bytes());
                hash = fnv1a_mix(hash, &[0xfe]);
            }
            return hash;
        }
    }

    let mut hash = fnv1a_mix(parent_hash, &[0x10]);
    hash = fnv1a_mix(hash, &child_index.to_le_bytes());
    match bp {
        Some(NodeBlueprint::Element { tag, .. }) => {
            hash = fnv1a_mix(hash, &[1]);
            let mut tag_hasher = std::collections::hash_map::DefaultHasher::new();
            std::hash::Hash::hash(tag, &mut tag_hasher);
            hash = fnv1a_mix(hash, &std::hash::Hasher::finish(&tag_hasher).to_le_bytes());
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

fn flattened_blueprint_children(bp: Option<&NodeBlueprint>) -> Vec<&NodeBlueprint> {
    let mut flat = Vec::new();
    match bp {
        Some(NodeBlueprint::Element { children, .. }) | Some(NodeBlueprint::Fragment(children)) => {
            for c in children {
                AppRuntime::flatten_blueprints(c, &mut flat);
            }
        }
        _ => {}
    }
    flat
}

impl AppRuntime {
    /// Records current solved `border_box` dimensions for every tracked animation node ID
    /// before a DOM update/rebuild overwrites styles or geometries.
    pub(crate) fn capture_previous_node_sizes(&mut self) {
        if self.path_node_ids.is_empty() {
            return;
        }
        self.previous_node_sizes.clear();
        let mut path = Vec::new();
        Self::capture_node_sizes_rec(
            &self.layout_root,
            &mut path,
            &self.path_node_ids,
            &mut self.previous_node_sizes,
        );
    }

    fn capture_node_sizes_rec(
        node: &LayoutNode,
        path: &mut Vec<usize>,
        path_node_ids: &std::collections::HashMap<Vec<usize>, u64>,
        out: &mut std::collections::HashMap<u64, (f32, f32)>,
    ) {
        if let Some(&id) = path_node_ids.get(path) {
            out.insert(
                id,
                (
                    node.geometry.border_box.width,
                    node.geometry.border_box.height,
                ),
            );
        }
        for (idx, child) in node.children.iter().enumerate() {
            path.push(idx);
            Self::capture_node_sizes_rec(child, path, path_node_ids, out);
            path.pop();
        }
    }

    fn has_transition_to_auto_dim(
        node: &LayoutNode,
        blueprint: Option<&NodeBlueprint>,
        parent_hash: u64,
        child_index: usize,
        previous_styles: &std::collections::HashMap<u64, ComputedStyle>,
    ) -> bool {
        let node_id = blueprint_node_identity_hash(parent_hash, child_index, blueprint);
        if !node.style.transitions.is_empty() {
            if let Some(old) = previous_styles.get(&node_id) {
                let is_auto = |d| matches!(d, strata_style::types::Dimension::Auto);
                let width_changed_to_auto =
                    old.width != node.style.width && is_auto(node.style.width);
                let height_changed_to_auto =
                    old.height != node.style.height && is_auto(node.style.height);
                if width_changed_to_auto || height_changed_to_auto {
                    return true;
                }
            }
        }
        let flat_children = flattened_blueprint_children(blueprint);
        for (idx, child) in node.children.iter().enumerate() {
            if Self::has_transition_to_auto_dim(
                child,
                flat_children.get(idx).copied(),
                node_id,
                idx,
                previous_styles,
            ) {
                return true;
            }
        }
        false
    }

    fn collect_measured_auto_sizes(
        node: &LayoutNode,
        blueprint: Option<&NodeBlueprint>,
        parent_hash: u64,
        child_index: usize,
        out: &mut std::collections::HashMap<u64, (f32, f32)>,
    ) {
        let node_id = blueprint_node_identity_hash(parent_hash, child_index, blueprint);
        if !node.style.transitions.is_empty() {
            out.insert(
                node_id,
                (
                    node.geometry.border_box.width,
                    node.geometry.border_box.height,
                ),
            );
        }
        let flat_children = flattened_blueprint_children(blueprint);
        for (idx, child) in node.children.iter().enumerate() {
            Self::collect_measured_auto_sizes(
                child,
                flat_children.get(idx).copied(),
                node_id,
                idx,
                out,
            );
        }
    }

    /// Synchronizes running animations and transitions with the current layout tree.
    pub fn sync_tree_animations(&mut self, now_ms: f64) {
        let mut new_base_styles = std::collections::HashMap::new();
        let mut new_node_paths = std::collections::HashMap::new();
        let mut new_path_node_ids = std::collections::HashMap::new();
        let mut new_child_counts = std::collections::HashMap::new();
        let mut path_buf = Vec::new();

        let measured_auto_sizes = if Self::has_transition_to_auto_dim(
            &self.layout_root,
            Some(&self.root_blueprint),
            0xcbf29ce484222325u64,
            0,
            &self.previous_node_styles,
        ) {
            let mut probe = self.layout_root.clone();
            probe.compute_layout(self.viewport_width, self.viewport_height);
            let mut sizes = std::collections::HashMap::new();
            Self::collect_measured_auto_sizes(
                &probe,
                Some(&self.root_blueprint),
                0xcbf29ce484222325u64,
                0,
                &mut sizes,
            );
            Some(sizes)
        } else {
            None
        };

        let mut trans_events = Vec::new();
        let mut anim_events = Vec::new();

        Self::sync_tree_animations_rec(
            &self.layout_root,
            Some(&self.root_blueprint),
            0xcbf29ce484222325u64,
            0,
            false,
            &mut path_buf,
            now_ms,
            &self.previous_node_styles,
            &self.previous_node_sizes,
            &self.previous_child_counts,
            measured_auto_sizes.as_ref(),
            &mut new_base_styles,
            &mut new_node_paths,
            &mut new_path_node_ids,
            &mut new_child_counts,
            &mut self.transition_manager,
            &mut self.animation_manager,
            &mut trans_events,
            &mut anim_events,
        );

        self.base_node_styles = new_base_styles;
        self.previous_node_styles = self.base_node_styles.clone();
        self.previous_child_counts = new_child_counts;
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
        parent_child_count_changed: bool,
        path: &mut Vec<usize>,
        now_ms: f64,
        previous_styles: &std::collections::HashMap<u64, ComputedStyle>,
        previous_sizes: &std::collections::HashMap<u64, (f32, f32)>,
        previous_child_counts: &std::collections::HashMap<u64, usize>,
        measured_auto_sizes: Option<&std::collections::HashMap<u64, (f32, f32)>>,
        new_base_styles: &mut std::collections::HashMap<u64, ComputedStyle>,
        new_node_paths: &mut std::collections::HashMap<u64, Vec<usize>>,
        new_path_node_ids: &mut std::collections::HashMap<Vec<usize>, u64>,
        new_child_counts: &mut std::collections::HashMap<u64, usize>,
        transition_manager: &mut strata_style::transition::TransitionManager,
        animation_manager: &mut strata_style::animation::AnimationManager,
        trans_events: &mut Vec<strata_style::transition::TransitionLifecycleEvent>,
        anim_events: &mut Vec<strata_style::animation::AnimationLifecycleEvent>,
    ) {
        let has_explicit_id = blueprint_has_explicit_identity(blueprint);
        let node_id = blueprint_node_identity_hash(parent_hash, child_index, blueprint);
        new_base_styles.insert(node_id, node.style.clone());
        new_node_paths.insert(node_id, path.clone());
        new_path_node_ids.insert(path.clone(), node_id);
        new_child_counts.insert(node_id, node.children.len());

        let unkeyed_list_shifted = parent_child_count_changed && !has_explicit_id;
        if unkeyed_list_shifted && transition_manager.has_node_transitions(node_id) {
            trans_events.extend(transition_manager.cancel_node(node_id, now_ms));
        }

        // Check for transitions if we have a previous style or @starting-style.
        // Suppress positional diffing for unkeyed siblings when the parent's child count
        // changed (e.g. prepend/insert/remove in an unkeyed list) so neighboring elements
        // do not cross-animate each other's styles.
        let effective_old_style = if unkeyed_list_shifted {
            node.style.starting_style.as_deref()
        } else {
            match previous_styles.get(&node_id) {
                Some(old) if old.display != strata_style::Display::None => {
                    if old.transitions.is_empty()
                        && !node.style.transitions.is_empty()
                        && !has_explicit_id
                    {
                        node.style.starting_style.as_deref()
                    } else {
                        Some(old)
                    }
                }
                _ => node.style.starting_style.as_deref(),
            }
        };

        if let Some(old_style) = effective_old_style {
            if !node.style.transitions.is_empty()
                || transition_manager.has_node_transitions(node_id)
            {
                let mut resolved_old_buf: Option<ComputedStyle> = None;
                let mut resolved_new_buf: Option<ComputedStyle> = None;

                if let Some(&(prev_w, prev_h)) = previous_sizes.get(&node_id) {
                    if old_style.width == strata_style::types::Dimension::Auto
                        && old_style.width != node.style.width
                    {
                        resolved_old_buf
                            .get_or_insert_with(|| old_style.clone())
                            .width = strata_style::types::Dimension::Px(prev_w);
                    }
                    if old_style.height == strata_style::types::Dimension::Auto
                        && old_style.height != node.style.height
                    {
                        resolved_old_buf
                            .get_or_insert_with(|| old_style.clone())
                            .height = strata_style::types::Dimension::Px(prev_h);
                    }
                }
                if let Some(&(target_w, target_h)) =
                    measured_auto_sizes.and_then(|m| m.get(&node_id))
                {
                    if node.style.width == strata_style::types::Dimension::Auto
                        && old_style.width != node.style.width
                    {
                        resolved_new_buf
                            .get_or_insert_with(|| node.style.clone())
                            .width = strata_style::types::Dimension::Px(target_w);
                    }
                    if node.style.height == strata_style::types::Dimension::Auto
                        && old_style.height != node.style.height
                    {
                        resolved_new_buf
                            .get_or_insert_with(|| node.style.clone())
                            .height = strata_style::types::Dimension::Px(target_h);
                    }
                }

                let old_ref = resolved_old_buf.as_ref().unwrap_or(old_style);
                let new_ref = resolved_new_buf.as_ref().unwrap_or(&node.style);
                trans_events.extend(transition_manager.update_styles(
                    node_id,
                    old_ref,
                    new_ref,
                    now_ms,
                ));
            }
        }

        // Check for keyframe animations
        if !node.style.animations.is_empty() || animation_manager.has_active_animations() {
            anim_events.extend(animation_manager.update_styles(node_id, &node.style, now_ms));
        }

        let own_child_count_changed = previous_child_counts
            .get(&node_id)
            .is_some_and(|&prev| prev != node.children.len());
        let flat_children = flattened_blueprint_children(blueprint);

        for (idx, child) in node.children.iter().enumerate() {
            path.push(idx);
            let child_bp = flat_children.get(idx).copied();
            Self::sync_tree_animations_rec(
                child,
                child_bp,
                node_id,
                idx,
                own_child_count_changed,
                path,
                now_ms,
                previous_styles,
                previous_sizes,
                previous_child_counts,
                measured_auto_sizes,
                new_base_styles,
                new_node_paths,
                new_path_node_ids,
                new_child_counts,
                transition_manager,
                animation_manager,
                trans_events,
                anim_events,
            );
            path.pop();
        }
    }

    /// Synchronizes paint-only CSS transitions strictly within the restyled subtree at
    /// `subtree_path` without scanning the full document or disturbing active transitions
    /// on unrelated subtrees.
    pub(crate) fn sync_subtree_paint_transitions(&mut self, subtree_path: &[usize], now_ms: f64) {
        let mut node_opt: Option<&mut LayoutNode> = Some(&mut self.layout_root);
        for &idx in subtree_path {
            node_opt = node_opt.and_then(|n| n.children.get_mut(idx));
        }
        let Some(subtree_root) = node_opt else {
            return;
        };

        let mut path = subtree_path.to_vec();
        let mut trans_events = Vec::new();
        Self::sync_subtree_paint_transitions_rec(
            subtree_root,
            &mut path,
            now_ms,
            None,
            &self.path_node_ids,
            &mut self.base_node_styles,
            &mut self.previous_node_styles,
            &mut self.transition_manager,
            &self.animation_manager,
            &mut trans_events,
        );
        for ev in trans_events {
            self.dispatch_transition_event(ev);
        }
        if self.transition_manager.has_active_transitions()
            || self.animation_manager.has_active_animations()
        {
            self.had_active_animations = true;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn sync_subtree_paint_transitions_rec(
        node: &mut LayoutNode,
        path: &mut Vec<usize>,
        now_ms: f64,
        inherited_color_override: Option<(strata_style::types::Color, strata_style::types::Color)>,
        path_node_ids: &std::collections::HashMap<Vec<usize>, u64>,
        base_node_styles: &mut std::collections::HashMap<u64, ComputedStyle>,
        previous_node_styles: &mut std::collections::HashMap<u64, ComputedStyle>,
        transition_manager: &mut strata_style::transition::TransitionManager,
        animation_manager: &strata_style::animation::AnimationManager,
        trans_events: &mut Vec<strata_style::transition::TransitionLifecycleEvent>,
    ) {
        let own_base_color = node.style.color;
        if let Some(&node_id) = path_node_ids.get(path) {
            let old_style = base_node_styles
                .get(&node_id)
                .or_else(|| previous_node_styles.get(&node_id))
                .cloned();
            let new_base = node.style.clone();
            if let Some(old) = old_style {
                if !new_base.transitions.is_empty()
                    || transition_manager.has_node_transitions(node_id)
                {
                    trans_events.extend(transition_manager.update_styles(
                        node_id,
                        &old,
                        &new_base,
                        now_ms,
                    ));
                }
            }
            // Keep `previous_node_styles` in lockstep with `base_node_styles` so a
            // subsequent full `sync_tree_animations` (e.g. button click updating classes)
            // diffs against the post-hover base style rather than the stale pre-hover style.
            previous_node_styles.insert(node_id, new_base.clone());
            base_node_styles.insert(node_id, new_base);
            if let Some((anc_base_color, anc_anim_color)) = inherited_color_override {
                if own_base_color == anc_base_color {
                    node.style.color = anc_anim_color;
                }
            }
            transition_manager.apply_to_style(node_id, &mut node.style);
            animation_manager.apply_to_style(node_id, &mut node.style);
            node.active_transitions = transition_manager.has_node_transitions(node_id);
        } else if let Some((anc_base_color, anc_anim_color)) = inherited_color_override {
            if own_base_color == anc_base_color {
                node.style.color = anc_anim_color;
            }
        }
        let next_color_override = if node.style.color != own_base_color {
            Some((own_base_color, node.style.color))
        } else {
            None
        };
        for (idx, child) in node.children.iter_mut().enumerate() {
            path.push(idx);
            Self::sync_subtree_paint_transitions_rec(
                child,
                path,
                now_ms,
                next_color_override,
                path_node_ids,
                base_node_styles,
                previous_node_styles,
                transition_manager,
                animation_manager,
                trans_events,
            );
            path.pop();
        }
    }

    /// Restores the true post-cascade base styles from `self.base_node_styles` back onto
    /// `self.layout_root` and clears in-flight collapse-lock overrides so incremental
    /// reconciliation and subsequent animation syncs diff and reuse real target styles rather
    /// than in-flight interpolated values.
    pub(crate) fn restore_base_styles_to_layout_tree(&mut self) {
        if self.base_node_styles.is_empty() {
            return;
        }
        let mut path = Vec::new();
        Self::restore_base_styles_rec(
            &mut self.layout_root,
            &mut path,
            &self.path_node_ids,
            &self.base_node_styles,
        );
    }

    fn restore_base_styles_rec(
        node: &mut LayoutNode,
        path: &mut Vec<usize>,
        path_node_ids: &std::collections::HashMap<Vec<usize>, u64>,
        base_styles: &std::collections::HashMap<u64, ComputedStyle>,
    ) {
        let node_id = path_node_ids
            .get(path)
            .copied()
            .unwrap_or_else(|| path_to_node_id(path));
        if let Some(base) = base_styles.get(&node_id) {
            node.style = base.clone();
        }
        node.transition_layout_width_px = None;
        node.transition_layout_height_px = None;
        for (idx, child) in node.children.iter_mut().enumerate() {
            path.push(idx);
            Self::restore_base_styles_rec(child, path, path_node_ids, base_styles);
            path.pop();
        }
    }

    /// Updates `base_node_styles` and `previous_node_styles` for a restyled non-transitioning
    /// subtree so subsequent `apply_animations_to_layout_tree` / `restore_base_styles_to_layout_tree`
    /// passes preserve the newly cascaded hover/interaction styles while unrelated animations run.
    pub(crate) fn sync_subtree_base_styles(&mut self, subtree_path: &[usize]) {
        let mut node_opt: Option<&LayoutNode> = Some(&self.layout_root);
        for &idx in subtree_path {
            node_opt = node_opt.and_then(|n| n.children.get(idx));
        }
        let Some(subtree_root) = node_opt else {
            return;
        };
        let mut path = subtree_path.to_vec();
        Self::sync_subtree_base_styles_rec(
            subtree_root,
            &mut path,
            &self.path_node_ids,
            &mut self.base_node_styles,
            &mut self.previous_node_styles,
        );
    }

    fn sync_subtree_base_styles_rec(
        node: &LayoutNode,
        path: &mut Vec<usize>,
        path_node_ids: &std::collections::HashMap<Vec<usize>, u64>,
        base_node_styles: &mut std::collections::HashMap<u64, ComputedStyle>,
        previous_node_styles: &mut std::collections::HashMap<u64, ComputedStyle>,
    ) {
        if let Some(&node_id) = path_node_ids.get(path) {
            let s = node.style.clone();
            previous_node_styles.insert(node_id, s.clone());
            base_node_styles.insert(node_id, s);
        }
        for (idx, child) in node.children.iter().enumerate() {
            path.push(idx);
            Self::sync_subtree_base_styles_rec(
                child,
                path,
                path_node_ids,
                base_node_styles,
                previous_node_styles,
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
            (self.viewport_width, self.viewport_height),
            (self.viewport_width, self.viewport_height),
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_animations_rec(
        node: &mut LayoutNode,
        path: &mut Vec<usize>,
        path_node_ids: &std::collections::HashMap<Vec<usize>, u64>,
        base_styles: &std::collections::HashMap<u64, ComputedStyle>,
        transition_manager: &strata_style::transition::TransitionManager,
        animation_manager: &strata_style::animation::AnimationManager,
        inherited_color_override: Option<(strata_style::types::Color, strata_style::types::Color)>,
        parent_size: (f32, f32),
        viewport_size: (f32, f32),
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

        let (lock_w, lock_h) = transition_manager.active_collapse_lock_dimensions(
            node_id,
            parent_size.0,
            parent_size.1,
            viewport_size.0,
            viewport_size.1,
            node.style.font_size,
        );
        node.transition_layout_width_px = lock_w;
        node.transition_layout_height_px = lock_h;

        let next_color_override = if node.style.color != own_base_color {
            Some((own_base_color, node.style.color))
        } else {
            None
        };

        let child_parent_w = lock_w.unwrap_or(node.geometry.content_box.width).max(0.0);
        let child_parent_h = lock_h.unwrap_or(node.geometry.content_box.height).max(0.0);
        let child_parent = if child_parent_w > 0.0 || child_parent_h > 0.0 {
            (child_parent_w, child_parent_h)
        } else {
            parent_size
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
                child_parent,
                viewport_size,
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
        let now_ms = self.now_ms();
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

    /// Advances active animations and transitions by current runtime clock time.
    /// Returns true if any animation stepped and a redraw is needed.
    #[inline]
    pub fn poll_animations(&mut self) -> bool {
        let now = self.now();
        self.poll_animations_at(now)
    }

    /// Advances the virtual clock by `dt` and steps active animations/transitions at that instant.
    pub fn advance_animations(&mut self, dt: std::time::Duration) -> bool {
        let now = self.advance_clock(dt);
        self.poll_animations_at(now)
    }

    /// Advances active animations and transitions at an explicit monotonic `Instant`.
    /// Returns true if any animation stepped and a redraw is needed.
    pub fn poll_animations_at(&mut self, now: std::time::Instant) -> bool {
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

        let now_ms = now
            .saturating_duration_since(self.runtime_start_epoch)
            .as_secs_f64()
            * 1000.0;
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
