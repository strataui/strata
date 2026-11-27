//! Platform event dispatching, pointer gestures, keyboard events, and window drag candidates.

use super::*;

#[derive(Default)]
pub(crate) struct BubblingOutcome {
    pub handled: bool,
    pub default_prevented: bool,
    pub propagation_stopped: bool,
    pub capture_requested: Option<bool>,
    pub capture_entity: Option<strata_primitives::EntityId>,
    pub cursor_warp_requested: Option<(f64, f64)>,
    pub target_entity: Option<strata_primitives::EntityId>,
    pub chain_entities: Vec<strata_primitives::EntityId>,
    pub invoked_entities: Vec<strata_primitives::EntityId>,
}

fn blueprint_nonempty_attr<'a>(bp: &'a NodeBlueprint, attr_name: &str) -> Option<&'a str> {
    let NodeBlueprint::Element { attributes, .. } = bp else {
        return None;
    };
    attributes.iter().find_map(|attr| match attr {
        strata_dom::Attribute::Property { key, value }
            if key == attr_name && !value.is_empty() =>
        {
            Some(value.as_str())
        }
        _ => None,
    })
}

fn blueprint_explicit_key(bp: &NodeBlueprint) -> Option<(String, String)> {
    if let Some(val) = blueprint_nonempty_attr(bp, "key") {
        return Some(("key".to_string(), val.to_string()));
    }
    if let Some(val) = blueprint_nonempty_attr(bp, "data-key") {
        return Some(("data-key".to_string(), val.to_string()));
    }
    None
}

fn blueprint_tag(bp: &NodeBlueprint) -> Option<strata_dom::Tag> {
    match bp {
        NodeBlueprint::Element { tag, .. } => Some(*tag),
        _ => None,
    }
}

fn anchor_has_explicit_identity(anchor: &CapturedAncestorAnchor) -> bool {
    anchor.element_id.is_some() || anchor.dom_uid.is_some() || anchor.element_key.is_some()
}

fn anchor_matches_node(
    anchor: &CapturedAncestorAnchor,
    bp: &NodeBlueprint,
    layout: &LayoutNode,
) -> bool {
    if let Some(ref target_id) = anchor.element_id {
        if blueprint_nonempty_attr(bp, "id") == Some(target_id.as_str()) {
            return true;
        }
    }
    if let Some(target_uid) = anchor.dom_uid {
        if bp.dom_uid() == Some(target_uid) {
            return true;
        }
    }
    if let Some((ref key_attr, ref key_val)) = anchor.element_key {
        if blueprint_nonempty_attr(bp, key_attr) == Some(key_val.as_str())
            && (anchor.tag.is_none() || blueprint_tag(bp) == anchor.tag)
        {
            return true;
        }
    }
    if let Some(eid) = anchor.entity_id {
        if layout.entity_id.is_valid() && layout.entity_id == eid {
            return true;
        }
    }
    false
}

impl AppRuntime {
    /// Returns the currently active platform mouse cursor icon.
    pub fn current_cursor(&self) -> strata_platform::window::CursorIcon {
        self.current_cursor
    }

    /// Registers a window/application-level `pointerup` / `mouseup` listener on this runtime.
    ///
    /// Guaranteed to fire on every pointer release even if the capturing DOM node unmounted,
    /// shifted sibling indices, or moved across portals. Returns a listener ID that can be
    /// passed to [`AppRuntime::remove_pointer_up_listener`].
    pub fn on_pointer_up<F: Fn(strata_dom::Event) + 'static>(&mut self, handler: F) -> u64 {
        self.window_pointer_up_listeners.add(handler)
    }

    /// Unregisters a window-level `pointerup` listener previously added with [`AppRuntime::on_pointer_up`].
    pub fn remove_pointer_up_listener(&mut self, id: u64) -> bool {
        self.window_pointer_up_listeners.remove(id)
    }

    /// Registers a window/application-level `pointercancel` listener on this runtime.
    ///
    /// Fires whenever an active pointer gesture is interrupted (for example on window blur /
    /// focus loss, touch/stylus cancellation, or explicit [`AppRuntime::dispatch_pointer_cancel`]).
    /// Returns a listener ID that can be passed to [`AppRuntime::remove_pointer_cancel_listener`].
    pub fn on_pointer_cancel<F: Fn(strata_dom::Event) + 'static>(&mut self, handler: F) -> u64 {
        self.window_pointer_cancel_listeners.add(handler)
    }

    /// Unregisters a window-level `pointercancel` listener previously added with [`AppRuntime::on_pointer_cancel`].
    pub fn remove_pointer_cancel_listener(&mut self, id: u64) -> bool {
        self.window_pointer_cancel_listeners.remove(id)
    }

    /// Returns `true` when an active pointer capture, scrollbar drag, native DOM drag, or
    /// input selection drag is currently in progress.
    #[inline]
    pub fn has_active_pointer_capture_or_drag(&self) -> bool {
        self.pointer_capture_target.is_some()
            || self.scrollbar_drag.is_some()
            || self.active_dom_drag.is_some()
            || self.drag_selection_active
            || self.pending_window_drag.is_some()
    }

    /// Iteratively collects all layout-indexed paths whose `(NodeBlueprint, LayoutNode)` satisfies `pred`.
    pub(crate) fn collect_matching_layout_paths(
        &self,
        mut pred: impl FnMut(&NodeBlueprint, &LayoutNode) -> bool,
    ) -> Vec<Vec<usize>> {
        let mut out = Vec::new();
        if pred(&self.root_blueprint, &self.layout_root) {
            out.push(Vec::new());
        }
        let root_children = Self::paired_blueprint_children(&self.root_blueprint, &self.layout_root);
        if root_children.is_empty() {
            return out;
        }
        let mut path = Vec::new();
        let mut stack: Vec<(Vec<(usize, &NodeBlueprint, &LayoutNode)>, usize)> =
            vec![(root_children, 0)];
        while let Some((children, next)) = stack.last_mut() {
            if *next >= children.len() {
                stack.pop();
                path.pop();
                continue;
            }
            let (child_idx, child_bp, child_layout) = children[*next];
            *next += 1;
            path.push(child_idx);
            if pred(child_bp, child_layout) {
                out.push(path.clone());
            }
            if !child_layout.children.is_empty() && stack.len() < 256 {
                let grand = Self::paired_blueprint_children(child_bp, child_layout);
                if !grand.is_empty() {
                    stack.push((grand, 0));
                    continue;
                }
            }
            path.pop();
        }
        out
    }

    fn node_has_pointer_move_or_up_listener(bp: &NodeBlueprint) -> bool {
        matches!(
            bp,
            NodeBlueprint::Element { attributes, .. }
                if attributes.iter().any(|a| matches!(
                    a,
                    strata_dom::Attribute::EventListener {
                        event: strata_dom::EventType::PointerMove
                            | strata_dom::EventType::MouseMove
                            | strata_dom::EventType::PointerUp
                            | strata_dom::EventType::MouseUp,
                        ..
                    }
                ))
        )
    }

    fn subtree_has_pointer_move_or_up_listener(bp: &NodeBlueprint) -> bool {
        let mut stack = vec![bp];
        while let Some(node) = stack.pop() {
            if Self::node_has_pointer_move_or_up_listener(node) {
                return true;
            }
            match node {
                NodeBlueprint::Element { children, .. } | NodeBlueprint::Fragment(children) => {
                    for child in children.iter().rev() {
                        stack.push(child);
                    }
                }
                _ => {}
            }
        }
        false
    }

    fn compute_listener_rank(
        anc_bp: &NodeBlueprint,
        anc_layout: &LayoutNode,
        child_idx: usize,
    ) -> (Option<usize>, usize) {
        let paired = Self::paired_blueprint_children(anc_bp, anc_layout);
        let mut rank = None;
        let mut total = 0usize;
        for (idx, child_bp, _) in paired {
            if Self::subtree_has_pointer_move_or_up_listener(child_bp) {
                if idx == child_idx {
                    rank = Some(total);
                }
                total += 1;
            }
        }
        (rank, total)
    }

    fn candidate_matches_captured_signature(
        &self,
        path: &[usize],
        cap: &PointerCaptureTarget,
    ) -> bool {
        let chain = Self::resolve_chain_for_path(&self.root_blueprint, &self.layout_root, path);
        if chain.len() != path.len() + 1 {
            return false;
        }
        let Some(&(target_bp, _)) = chain.last() else {
            return false;
        };
        if let Some(expected_tag) = cap.tag {
            if blueprint_tag(target_bp) != Some(expected_tag) {
                return false;
            }
        }
        if cap.had_move_or_up_listener
            && !chain
                .iter()
                .any(|(bp, _)| Self::node_has_pointer_move_or_up_listener(bp))
        {
            return false;
        }
        true
    }

    fn resolve_tail_under_ancestor(
        &self,
        anc_path: &[usize],
        anchor: &CapturedAncestorAnchor,
        cap: &PointerCaptureTarget,
    ) -> CaptureResolution {
        let mut direct_path = anc_path.to_vec();
        direct_path.extend_from_slice(&anchor.relative_tail);
        if self.candidate_matches_captured_signature(&direct_path, cap) {
            return CaptureResolution::Resolved(direct_path);
        }

        // If a non-listening sibling overlay (e.g. snap preview or drag ghost) was inserted or
        // removed directly under this ancestor before an unkeyed captured child/subtree, recover
        // the shifted sibling using its rank among listener-bearing children under this ancestor.
        if let (Some(expected_rank), Some(first_tail_idx)) =
            (anchor.listener_rank, anchor.relative_tail.first().copied())
        {
            let anc_chain =
                Self::resolve_chain_for_path(&self.root_blueprint, &self.layout_root, anc_path);
            if anc_chain.len() == anc_path.len() + 1 {
                if let Some(&(anc_bp, anc_layout)) = anc_chain.last() {
                    let paired = Self::paired_blueprint_children(anc_bp, anc_layout);
                    let listener_children: Vec<usize> = paired
                        .into_iter()
                        .filter_map(|(idx, child_bp, _)| {
                            Self::subtree_has_pointer_move_or_up_listener(child_bp).then_some(idx)
                        })
                        .collect();
                    // Only shift when the count of listener-bearing siblings is preserved so an
                    // unmounted captured sibling never jumps onto a different listener sibling.
                    if listener_children.len() == anchor.total_listener_siblings {
                        if let Some(&shifted_child_idx) = listener_children.get(expected_rank) {
                            if shifted_child_idx != first_tail_idx {
                                let mut shifted_path = anc_path.to_vec();
                                shifted_path.push(shifted_child_idx);
                                shifted_path.extend_from_slice(&anchor.relative_tail[1..]);
                                if self.candidate_matches_captured_signature(&shifted_path, cap) {
                                    return CaptureResolution::Resolved(shifted_path);
                                }
                            }
                        }
                    }
                }
            }
        }

        CaptureResolution::Unmounted
    }

    /// Captures stable DOM identity metadata for a newly acquired pointer capture target.
    pub(crate) fn build_pointer_capture_target(
        &self,
        target_path: Vec<usize>,
        explicit: bool,
        cap_eid: strata_primitives::EntityId,
        button: i16,
    ) -> PointerCaptureTarget {
        let chain =
            Self::resolve_chain_for_path(&self.root_blueprint, &self.layout_root, &target_path);
        let (element_id, element_key, dom_uid, entity_id, tag) =
            if let Some(&(bp, layout)) = chain.last() {
                (
                    blueprint_nonempty_attr(bp, "id").map(str::to_string),
                    blueprint_explicit_key(bp),
                    bp.dom_uid(),
                    if layout.entity_id.is_valid() {
                        Some(layout.entity_id)
                    } else if cap_eid.is_valid() {
                        Some(cap_eid)
                    } else {
                        None
                    },
                    blueprint_tag(bp),
                )
            } else {
                (
                    None,
                    None,
                    None,
                    cap_eid.is_valid().then_some(cap_eid),
                    None,
                )
            };

        let had_move_or_up_listener = chain
            .iter()
            .any(|(bp, _)| Self::node_has_pointer_move_or_up_listener(bp));
        let lost_capture_handler = Self::find_event_at_or_above_path(
            &self.root_blueprint,
            &self.layout_root,
            &target_path,
            strata_dom::EventType::LostPointerCapture,
        )
        .map(|(h, _, _)| h);

        let key_was_unique_at_capture = if let Some((ref key_attr, ref key_val)) = element_key {
            let matches = self.collect_matching_layout_paths(|bp, _| {
                blueprint_nonempty_attr(bp, key_attr) == Some(key_val.as_str())
                    && (tag.is_none() || blueprint_tag(bp) == tag)
            });
            matches.len() <= 1
        } else {
            false
        };

        let mut ancestor_anchors = Vec::new();
        if chain.len() > 1 {
            for (depth, &(anc_bp, anc_layout)) in chain[..chain.len() - 1].iter().enumerate() {
                let anc_id = blueprint_nonempty_attr(anc_bp, "id").map(str::to_string);
                let anc_key = blueprint_explicit_key(anc_bp);
                let anc_uid = anc_bp.dom_uid();
                let anc_eid = anc_layout.entity_id.is_valid().then_some(anc_layout.entity_id);
                let relative_tail = target_path[depth..].to_vec();
                let (listener_rank, total_listener_siblings) =
                    if let Some(&first_child_idx) = relative_tail.first() {
                        Self::compute_listener_rank(anc_bp, anc_layout, first_child_idx)
                    } else {
                        (None, 0)
                    };
                if anc_id.is_some()
                    || anc_key.is_some()
                    || anc_uid.is_some()
                    || anc_eid.is_some()
                    || listener_rank.is_some()
                {
                    ancestor_anchors.push(CapturedAncestorAnchor {
                        element_id: anc_id,
                        element_key: anc_key,
                        dom_uid: anc_uid,
                        entity_id: anc_eid,
                        tag: blueprint_tag(anc_bp),
                        relative_tail,
                        listener_rank,
                        total_listener_siblings,
                    });
                }
            }
        }

        PointerCaptureTarget {
            path: target_path,
            explicit,
            entity_id,
            element_id,
            element_key,
            dom_uid,
            tag,
            key_was_unique_at_capture,
            ancestor_anchors,
            pointer_id: 1,
            button,
            had_move_or_up_listener,
            lost_capture_handler,
        }
    }

    fn score_candidate_path(
        &self,
        cand_path: &[usize],
        expected_tag: Option<strata_dom::Tag>,
        expected_ancestors: &[CapturedAncestorAnchor],
        fallback_path: &[usize],
    ) -> (usize, usize, usize) {
        let chain =
            Self::resolve_chain_for_path(&self.root_blueprint, &self.layout_root, cand_path);
        let tag_bonus = usize::from(
            expected_tag.is_some()
                && chain
                    .last()
                    .and_then(|(bp, _)| blueprint_tag(bp))
                    == expected_tag,
        );
        let cand_ancestors = &chain[..chain.len().saturating_sub(1)];
        let mut explicit_anc_score = 0usize;
        // Weight nearest explicit ancestor matches highest so duplicate keys in distinct subtrees disambiguate cleanly.
        for (weight_idx, anchor) in expected_ancestors.iter().enumerate() {
            if !anchor_has_explicit_identity(anchor) {
                continue;
            }
            if cand_ancestors
                .iter()
                .any(|&(bp, layout)| anchor_matches_node(anchor, bp, layout))
            {
                explicit_anc_score += (weight_idx + 1) * 4;
            }
        }
        let shared_prefix = cand_path
            .iter()
            .zip(fallback_path.iter())
            .take_while(|(a, b)| a == b)
            .count();
        (explicit_anc_score, tag_bonus, shared_prefix)
    }

    fn resolve_ancestor_anchor(
        &self,
        anchor: &CapturedAncestorAnchor,
        outer_ancestors: &[CapturedAncestorAnchor],
        fallback_path: &[usize],
    ) -> Option<CaptureResolution> {
        // 1. Surviving EntityId on this ancestor
        if let Some(eid) = anchor.entity_id {
            if let Some(path) = self.layout_root.find_path_by_entity_id(eid) {
                if Self::resolve_chain_for_path(&self.root_blueprint, &self.layout_root, &path)
                    .len()
                    == path.len() + 1
                {
                    return Some(CaptureResolution::Resolved(path));
                }
            }
        }
        // 2. Explicit id
        if let Some(ref anc_id) = anchor.element_id {
            let candidates = self.collect_matching_layout_paths(|bp, _| {
                blueprint_nonempty_attr(bp, "id") == Some(anc_id.as_str())
            });
            return Some(match candidates.len() {
                0 => CaptureResolution::Unmounted,
                1 => CaptureResolution::Resolved(candidates.into_iter().next().unwrap()),
                _ => {
                    let best = candidates
                        .into_iter()
                        .max_by_key(|p| {
                            self.score_candidate_path(p, anchor.tag, outer_ancestors, fallback_path)
                        })
                        .unwrap();
                    CaptureResolution::Resolved(best)
                }
            });
        }
        // 3. Explicit dom_uid
        if let Some(uid) = anchor.dom_uid {
            let candidates =
                self.collect_matching_layout_paths(|bp, _| bp.dom_uid() == Some(uid));
            return Some(
                candidates
                    .into_iter()
                    .next()
                    .map_or(CaptureResolution::Unmounted, CaptureResolution::Resolved),
            );
        }
        // 4. Explicit key / data-key
        if let Some((ref key_attr, ref key_val)) = anchor.element_key {
            let candidates = self.collect_matching_layout_paths(|bp, _| {
                blueprint_nonempty_attr(bp, key_attr) == Some(key_val.as_str())
                    && (anchor.tag.is_none() || blueprint_tag(bp) == anchor.tag)
            });
            return Some(match candidates.len() {
                0 => CaptureResolution::Unmounted,
                1 => CaptureResolution::Resolved(candidates.into_iter().next().unwrap()),
                _ => {
                    let best = candidates
                        .into_iter()
                        .max_by_key(|p| {
                            self.score_candidate_path(p, anchor.tag, outer_ancestors, fallback_path)
                        })
                        .unwrap();
                    CaptureResolution::Resolved(best)
                }
            });
        }
        // No explicit identity and EntityId was replaced on rebuild -> keep searching outward
        None
    }

    /// Resolves a [`PointerCaptureTarget`] to its current layout path using stable identity:
    /// 1. Surviving `EntityId`
    /// 2. Explicit `id="..."`
    /// 3. Explicit `data-strata-node-uid`
    /// 4. Explicit `key="..."` / `data-key="..."` (disambiguated by ancestor chain)
    /// 5. Nearest stable ancestor anchor + relative child tail (with non-listening overlay shift recovery)
    /// 6. Raw `path` / root listener-rank fallback ONLY when no stable identity was recorded
    pub(crate) fn resolve_pointer_capture(
        &self,
        cap: &PointerCaptureTarget,
    ) -> CaptureResolution {
        // 1. Surviving EntityId
        if let Some(eid) = cap.entity_id {
            if let Some(path) = self.layout_root.find_path_by_entity_id(eid) {
                if Self::resolve_chain_for_path(&self.root_blueprint, &self.layout_root, &path)
                    .len()
                    == path.len() + 1
                {
                    return CaptureResolution::Resolved(path);
                }
            }
        }

        // 2. Explicit id="..."
        if let Some(ref target_id) = cap.element_id {
            let candidates = self.collect_matching_layout_paths(|bp, _| {
                blueprint_nonempty_attr(bp, "id") == Some(target_id.as_str())
            });
            return match candidates.len() {
                0 => CaptureResolution::Unmounted,
                1 => CaptureResolution::Resolved(candidates.into_iter().next().unwrap()),
                _ => {
                    let best = candidates
                        .into_iter()
                        .max_by_key(|p| {
                            self.score_candidate_path(p, cap.tag, &cap.ancestor_anchors, &cap.path)
                        })
                        .unwrap();
                    CaptureResolution::Resolved(best)
                }
            };
        }

        // 3. Explicit data-strata-node-uid
        if let Some(uid) = cap.dom_uid {
            let candidates =
                self.collect_matching_layout_paths(|bp, _| bp.dom_uid() == Some(uid));
            return candidates
                .into_iter()
                .next()
                .map_or(CaptureResolution::Unmounted, CaptureResolution::Resolved);
        }

        // 4. Explicit key / data-key
        if let Some((ref key_attr, ref key_val)) = cap.element_key {
            let mut candidates = self.collect_matching_layout_paths(|bp, _| {
                blueprint_nonempty_attr(bp, key_attr) == Some(key_val.as_str())
                    && (cap.tag.is_none() || blueprint_tag(bp) == cap.tag)
            });
            if candidates.is_empty() && cap.tag.is_some() {
                candidates = self.collect_matching_layout_paths(|bp, _| {
                    blueprint_nonempty_attr(bp, key_attr) == Some(key_val.as_str())
                });
            }
            if candidates.is_empty() {
                return CaptureResolution::Unmounted;
            }
            if candidates.len() == 1 && cap.key_was_unique_at_capture {
                return CaptureResolution::Resolved(candidates.into_iter().next().unwrap());
            }
            // Disambiguate duplicate keys (or verify non-unique key did not vanish from its still-mounted container)
            let nearest_explicit_anc = cap
                .ancestor_anchors
                .iter()
                .enumerate()
                .rev()
                .find(|(_, a)| anchor_has_explicit_identity(a));
            let nearest_anc_still_mounted = nearest_explicit_anc.is_some_and(|(idx, anc)| {
                matches!(
                    self.resolve_ancestor_anchor(anc, &cap.ancestor_anchors[..idx], &cap.path),
                    Some(CaptureResolution::Resolved(_))
                )
            });
            let (best_path, best_score) = candidates
                .into_iter()
                .map(|p| {
                    let s =
                        self.score_candidate_path(&p, cap.tag, &cap.ancestor_anchors, &cap.path);
                    (p, s)
                })
                .max_by_key(|(_, s)| *s)
                .unwrap();
            if !cap.key_was_unique_at_capture && nearest_anc_still_mounted && best_score.0 == 0 {
                return CaptureResolution::Unmounted;
            }
            return CaptureResolution::Resolved(best_path);
        }

        // 5. Nearest stable ancestor anchor + relative tail (unkeyed descendant inside an identified ancestor)
        for idx in (0..cap.ancestor_anchors.len()).rev() {
            let anchor = &cap.ancestor_anchors[idx];
            let Some(res) =
                self.resolve_ancestor_anchor(anchor, &cap.ancestor_anchors[..idx], &cap.path)
            else {
                continue;
            };
            match res {
                CaptureResolution::Unmounted => return CaptureResolution::Unmounted,
                CaptureResolution::Resolved(anc_path) => {
                    return self.resolve_tail_under_ancestor(&anc_path, anchor, cap);
                }
            }
        }

        // 6. Fallback to raw path (plus unkeyed root overlay shift recovery) only when no stable identity existed
        if self.candidate_matches_captured_signature(&cap.path, cap) {
            return CaptureResolution::Resolved(cap.path.clone());
        }
        if let Some(root_anchor) = cap.ancestor_anchors.first() {
            if root_anchor.relative_tail == cap.path {
                return self.resolve_tail_under_ancestor(&[], root_anchor, cap);
            }
        }
        CaptureResolution::Unmounted
    }

    /// Reconciles active `pointer_capture_target` after `update_view` rebuilds or reconciles
    /// the DOM/layout tree: refreshes `path`, `entity_id`, ancestor anchors, and handlers when
    /// the target shifted siblings or moved across portals, or clears capture and delivers
    /// W3C `lostpointercapture` immediately if an explicitly captured subtree unmounted.
    pub(crate) fn reconcile_pointer_capture_after_view_update(&mut self) {
        let Some(cap) = self.pointer_capture_target.clone() else {
            return;
        };
        match self.resolve_pointer_capture(&cap) {
            CaptureResolution::Resolved(resolved_path) => {
                let refreshed_eid = Self::resolve_chain_for_path(
                    &self.root_blueprint,
                    &self.layout_root,
                    &resolved_path,
                )
                .last()
                .map(|(_, l)| l.entity_id)
                .unwrap_or(strata_primitives::EntityId::NONE);
                let mut refreshed = self.build_pointer_capture_target(
                    resolved_path,
                    cap.explicit,
                    refreshed_eid,
                    cap.button,
                );
                refreshed.pointer_id = cap.pointer_id;
                if cap.element_key.is_some() {
                    refreshed.key_was_unique_at_capture = cap.key_was_unique_at_capture;
                }
                self.pointer_capture_target = Some(refreshed);
            }
            CaptureResolution::Unmounted => {
                let lost = self.pointer_capture_target.take();
                if let Some(cap) = lost {
                    if self.is_mouse_down || cap.explicit {
                        self.capture_lost_while_mouse_down = true;
                    }
                    if cap.explicit {
                        if let Some(handler) = cap.lost_capture_handler {
                            let (px, py) = self.last_mouse_pos;
                            let mut ev = self.create_mouse_event(
                                strata_dom::EventType::LostPointerCapture,
                                px,
                                py,
                                cap.button,
                                0,
                                None,
                            );
                            ev.pointer_id = cap.pointer_id;
                            ev.data = Some("target_unmounted".to_string());
                            handler(ev);
                        }
                    }
                }
            }
        }
    }

    /// Dispatches `EventType::LostPointerCapture` at or above `path` if a listener exists.
    pub(crate) fn dispatch_lost_pointer_capture_at_path(
        &self,
        path: &[usize],
        px: f32,
        py: f32,
        button: i16,
        pointer_id: i32,
    ) -> bool {
        if let Some((handler, target_box, _)) = Self::find_event_at_or_above_path(
            &self.root_blueprint,
            &self.layout_root,
            path,
            strata_dom::EventType::LostPointerCapture,
        ) {
            let target_box_copy = *target_box;
            let mut ev = self.create_mouse_event(
                strata_dom::EventType::LostPointerCapture,
                px,
                py,
                button,
                0,
                Some(&target_box_copy),
            );
            ev.pointer_id = pointer_id;
            handler(ev);
            return true;
        }
        false
    }

    /// Invokes all registered runtime window-level and global `on_pointer_up` listeners.
    /// Returns `true` if at least one listener was invoked.
    pub(crate) fn dispatch_window_and_global_pointer_up(&self, event: &strata_dom::Event) -> bool {
        let mut invoked = false;
        if !self.window_pointer_up_listeners.is_empty() {
            for cb in self.window_pointer_up_listeners.snapshot() {
                cb(event.clone());
                invoked = true;
            }
        }
        if strata_dom::hooks::dispatch_global_pointer_up(event) {
            invoked = true;
        }
        invoked
    }

    /// Invokes all registered runtime window-level and global `on_pointer_cancel` listeners.
    /// Returns `true` if at least one listener was invoked.
    pub(crate) fn dispatch_window_and_global_pointer_cancel(
        &self,
        event: &strata_dom::Event,
    ) -> bool {
        let mut invoked = false;
        if !self.window_pointer_cancel_listeners.is_empty() {
            for cb in self.window_pointer_cancel_listeners.snapshot() {
                cb(event.clone());
                invoked = true;
            }
        }
        if strata_dom::hooks::dispatch_global_pointer_cancel(event) {
            invoked = true;
        }
        invoked
    }

    /// Explicitly dispatches a `PointerCancel` event at `(px, py)`, releasing any active
    /// pointer capture, drag state, and invoking element + window/global `on_pointer_cancel` hooks.
    pub fn dispatch_pointer_cancel(&mut self, px: f32, py: f32) -> bool {
        self.dispatch_pointer_cancel_with_meta(px, py, 1, "mouse", 0, "explicit_cancel")
    }

    /// Cancels any active pointer gesture (capture, press, DOM drag, scrollbar drag, or
    /// text selection drag) on window focus loss or system interruption.
    ///
    /// Returns `false` immediately without firing listeners when no pointer gesture is active.
    pub fn cancel_active_pointer_gesture(&mut self, reason: &str) -> bool {
        let had_active = self.is_mouse_down
            || self.has_active_pointer_capture_or_drag()
            || self.capture_lost_while_mouse_down
            || self.pending_click.is_some();
        if !had_active {
            return false;
        }
        let (px, py) = self.last_mouse_pos;
        let (pointer_id, button) = self
            .pointer_capture_target
            .as_ref()
            .map(|c| (c.pointer_id, c.button))
            .unwrap_or((1, 0));
        self.dispatch_pointer_cancel_with_meta(px, py, pointer_id, "mouse", button, reason)
    }

    /// Internal shared implementation for `PointerCancel` dispatch and capture/drag cleanup.
    pub(crate) fn dispatch_pointer_cancel_with_meta(
        &mut self,
        px: f32,
        py: f32,
        pointer_id: i32,
        pointer_type: &'static str,
        button: i16,
        reason: &str,
    ) -> bool {
        self.last_mouse_pos = (px, py);
        let was_mouse_down = self.is_mouse_down;
        let had_active_state = was_mouse_down
            || self.has_active_pointer_capture_or_drag()
            || self.capture_lost_while_mouse_down
            || self.pending_click.is_some();

        self.is_mouse_down = false;
        self.capture_lost_while_mouse_down = false;
        self.drag_selection_active = false;
        self.scrollbar_drag = None;
        self.pending_window_drag = None;
        self.cancel_pending_click();
        let capture = self.pointer_capture_target.take();
        let active_drag = self.active_dom_drag.take();

        let mut handled = false;
        if let Some(ref cap) = capture {
            if let CaptureResolution::Resolved(resolved_path) = self.resolve_pointer_capture(cap) {
                let outcome = self.dispatch_bubbling_event_from_path(
                    strata_dom::EventType::PointerCancel,
                    &resolved_path,
                    px,
                    py,
                    cap.button,
                    0,
                    |ev| {
                        ev.pointer_id = cap.pointer_id;
                        ev.pointer_type = pointer_type;
                        ev.data = Some(reason.to_string());
                    },
                );
                if outcome.handled {
                    handled = true;
                }
                if cap.explicit
                    && self.dispatch_lost_pointer_capture_at_path(
                        &resolved_path,
                        px,
                        py,
                        cap.button,
                        cap.pointer_id,
                    )
                {
                    handled = true;
                }
            }
        } else {
            let outcome = self.dispatch_bubbling_event_detailed_at(
                strata_dom::EventType::PointerCancel,
                px,
                py,
                button,
                0,
                |ev| {
                    ev.pointer_id = pointer_id;
                    ev.pointer_type = pointer_type;
                    ev.data = Some(reason.to_string());
                },
            );
            if outcome.handled {
                handled = true;
            }
        }

        if let Some(drag) = active_drag {
            if drag.started {
                let (invoked, _) = self.dispatch_drag_event_at_path(
                    &drag.source_path,
                    strata_dom::EventType::DragEnd,
                    px,
                    py,
                    &drag.data_transfer,
                );
                if invoked {
                    handled = true;
                }
            }
        }

        let mut cancel_ev = self.create_mouse_event(
            strata_dom::EventType::PointerCancel,
            px,
            py,
            button,
            0,
            None,
        );
        cancel_ev.pointer_id = pointer_id;
        cancel_ev.pointer_type = pointer_type;
        cancel_ev.data = Some(reason.to_string());
        if self.dispatch_window_and_global_pointer_cancel(&cancel_ev) {
            handled = true;
        }

        if was_mouse_down && self.restyle_for_active_change() {
            handled = true;
        }

        handled || had_active_state
    }

    /// Dispatches pointer down at (px, py) coordinates with default modifiers.
    /// Returns the detailed action result (CaretMoved, FocusChanged, DomListenerInvoked, or Ignored).
    pub fn dispatch_pointer_down_action(&mut self, px: f32, py: f32) -> PointerDownAction {
        self.dispatch_pointer_down_action_with_modifiers(
            px,
            py,
            strata_platform::Modifiers::default(),
        )
    }

    /// Dispatches pointer down at (px, py) coordinates with no active modifier keys.
    /// Returns true if focus or selection state changed.
    pub fn dispatch_pointer_down(&mut self, px: f32, py: f32) -> bool {
        self.dispatch_pointer_down_action(px, py).is_handled()
    }

    /// Dispatches pointer down at (px, py) coordinates to manage native input element focus, multi-click,
    /// Shift+click range extension, and text selection.
    /// Returns true if focus or selection state changed.
    pub fn dispatch_pointer_down_with_modifiers(
        &mut self,
        px: f32,
        py: f32,
        modifiers: strata_platform::Modifiers,
    ) -> bool {
        self.dispatch_pointer_down_action_with_modifiers(px, py, modifiers)
            .is_handled()
    }

    /// Dispatches pointer down with modifier keys, returning whether the interaction only moved the caret
    /// in the already-active input, changed focus, or invoked a DOM listener.
    pub fn dispatch_pointer_down_action_with_modifiers(
        &mut self,
        px: f32,
        py: f32,
        modifiers: strata_platform::Modifiers,
    ) -> PointerDownAction {
        self.dispatch_pointer_down_action_with_button_and_modifiers(px, py, 0, modifiers)
    }

    /// Dispatches pointer down with explicit mouse button (`0` = Left, `1` = Middle, `2` = Right)
    /// and modifier keys, managing text input focus/caret on primary clicks and invoking DOM
    /// `PointerDown` / `MouseDown` listeners with accurate `button` / `buttons` metadata.
    pub fn dispatch_pointer_down_action_with_button_and_modifiers(
        &mut self,
        px: f32,
        py: f32,
        button: i16,
        modifiers: strata_platform::Modifiers,
    ) -> PointerDownAction {
        // Surfaces marked `data-keep-focus` (context menus, toolbars) never steal focus from the
        // active text input, so commands such as Cut/Paste act on the field the user was editing.
        if Self::hit_test_property(
            &self.root_blueprint,
            &self.layout_root,
            px,
            py,
            "data-keep-focus",
        )
        .is_some()
        {
            self.drag_selection_active = false;
            return PointerDownAction::Ignored;
        }

        let mut input_handled = false;
        if button == 0 {
            if let Some(target) =
                Self::hit_test_input(&self.root_blueprint, &self.layout_root, px, py)
            {
                let target_key = target
                    .input_id
                    .clone()
                    .unwrap_or_else(|| format!("input:{}", target.input_index));
                let text_val = if let Some(ref active) = self.active_focus {
                    let current_key = active
                        .input_id
                        .clone()
                        .unwrap_or_else(|| format!("input:{}", active.input_index));
                    if current_key == target_key {
                        active.state.text.get()
                    } else {
                        target.current_value.clone()
                    }
                } else {
                    target.current_value.clone()
                };
                let is_password = !target.is_multiline
                    && self.input_is_password(
                        target.input_index,
                        target.input_id.as_deref(),
                        target.entity_id,
                    );
                let measure_val = if is_password {
                    Self::mask_password_text(&text_val)
                } else {
                    text_val
                };
                let (active_scroll_x, active_scroll_y) = self
                    .active_focus
                    .as_ref()
                    .and_then(|active| {
                        let current_key = active
                            .input_id
                            .clone()
                            .unwrap_or_else(|| format!("input:{}", active.input_index));
                        if current_key == target_key {
                            Some((active.state.scroll_x.get(), active.state.scroll_y.get()))
                        } else {
                            None
                        }
                    })
                    .or_else(|| {
                        if target.is_multiline {
                            target
                                .input_id
                                .as_deref()
                                .filter(|id| !id.is_empty())
                                .and_then(|id| self.scroll_offsets_by_id.get(id).copied())
                                .or_else(|| {
                                    Self::find_layout_for_focus(
                                        &self.root_blueprint,
                                        &self.layout_root,
                                        target.input_index,
                                        target.input_id.as_deref(),
                                        target.entity_id,
                                    )
                                    .map(|n| (n.geometry.scroll_offset.x, n.geometry.scroll_offset.y))
                                })
                                .filter(|(_, y)| *y > 0.0)
                        } else {
                            None
                        }
                    })
                    .or_else(|| {
                        self.input_states
                            .get(&target_key)
                            .map(|s| (s.scroll_x.get(), s.scroll_y.get()))
                    })
                    .unwrap_or((0.0, 0.0));
                let char_idx = Self::cursor_index_at_input_point(
                    &measure_val,
                    target.is_multiline,
                    px,
                    py,
                    target.content_box_x,
                    target.content_box_y,
                    target.content_box_width,
                    active_scroll_x,
                    active_scroll_y,
                    target.font_size,
                    target.line_height,
                    target.is_mono,
                    target.font_family,
                );

                let now = self.now();
                let click_count = if let Some((last_time, last_x, last_y, count)) =
                    self.last_click_info
                {
                    if now.duration_since(last_time).as_millis() < 400
                        && (px - last_x).abs() < 5.0
                        && (py - last_y).abs() < 5.0
                    {
                        count + 1
                    } else {
                        1
                    }
                } else {
                    1
                };
                self.last_click_info = Some((now, px, py, click_count));
                self.reset_caret_blink();

                let target_key = target
                    .input_id
                    .clone()
                    .unwrap_or_else(|| format!("input:{}", target.input_index));

                // If the same input is already active, handle multi-click or reposition caret
                if let Some(ref mut active) = self.active_focus {
                    let current_key = active
                        .input_id
                        .clone()
                        .unwrap_or_else(|| format!("input:{}", active.input_index));
                    if current_key == target_key {
                        if modifiers.shift {
                            // Shift+click extends the existing selection from its anchor to the clicked index.
                            active.state.update_selection_drag(char_idx);
                            self.drag_selection_active = true;
                        } else if click_count == 2 {
                            if is_password {
                                active.state.select_all();
                            } else {
                                active.state.select_word_at_index(char_idx);
                            }
                            self.drag_selection_active = false;
                        } else if click_count == 3 {
                            active.state.select_line_at_index(char_idx);
                            self.drag_selection_active = false;
                        } else if click_count >= 4 {
                            active.state.select_all();
                            self.drag_selection_active = false;
                        } else {
                            active.state.cursor_index.set(char_idx);
                            active.state.selection_anchor.set(Some(char_idx));
                            self.drag_selection_active = true;
                        }
                        self.sync_active_input_layout_and_scroll_caret();
                        return PointerDownAction::CaretMoved;
                    }

                    // Switching away from previous input: persist its undo/redo history in input_states
                    self.input_states
                        .insert(current_key.clone(), active.state.clone());
                    if let Some(prev_prop) = self.active_input_last_prop_value.take() {
                        self.input_last_prop_values.insert(current_key, prev_prop);
                    }
                    if let Some(ref blur_cb) = active.on_blur {
                        blur_cb(strata_dom::Event::new(strata_dom::EventType::Blur));
                    }
                }

                // Restore existing input state (with its private undo/redo history) or initialize new one
                let state = self.input_states.remove(&target_key).unwrap_or_else(|| {
                    strata_dom::text_input::TextInputState::with_text(&target.current_value)
                });
                let prev_saved_prop = self.input_last_prop_values.remove(&target_key);

                // Reconcile external controlled `value` prop changes that occurred while blurred
                // (including programmatic `value=""` clears), or initialize an empty state.
                let prop_changed_while_blurred = target.has_value_prop
                    && prev_saved_prop
                        .as_ref()
                        .is_some_and(|prev| prev != &target.current_value);
                if prop_changed_while_blurred
                    || (state.text.get().is_empty() && !target.current_value.is_empty())
                {
                    let new_char_len = target.current_value.chars().count();
                    state.text.set(target.current_value.clone());
                    state
                        .cursor_index
                        .set(state.cursor_index.get().min(new_char_len));
                    if new_char_len == 0 {
                        state.selection_anchor.set(None);
                        state.scroll_x.set(0.0);
                        state.scroll_y.set(0.0);
                    }
                }
                if target.is_multiline {
                    state.scroll_y.set(active_scroll_y);
                }

                let clamped_char_idx = char_idx.min(state.text.get().chars().count());
                if click_count == 2 {
                    if is_password {
                        state.select_all();
                    } else {
                        state.select_word_at_index(clamped_char_idx);
                    }
                    self.drag_selection_active = false;
                } else if click_count == 3 {
                    state.select_line_at_index(clamped_char_idx);
                    self.drag_selection_active = false;
                } else if click_count >= 4 {
                    state.select_all();
                    self.drag_selection_active = false;
                } else {
                    state.cursor_index.set(clamped_char_idx);
                    state.selection_anchor.set(Some(clamped_char_idx));
                    self.drag_selection_active = true;
                }
                state.is_focused.set(true);

                if let Some(ref focus_cb) = target.on_focus {
                    focus_cb(strata_dom::Event::new(strata_dom::EventType::Focus));
                }

                self.active_input_last_prop_value = if target.has_value_prop {
                    Some(target.current_value)
                } else {
                    None
                };
                self.active_focus = Some(ActiveInput {
                    input_index: target.input_index,
                    input_id: target.input_id,
                    entity_id: target.entity_id,
                    is_multiline: target.is_multiline,
                    state,
                    on_input: target.on_input,
                    on_change: target.on_change,
                    on_submit: target.on_submit,
                    on_blur: target.on_blur,
                });
                self.sync_active_input_layout_and_scroll_caret();
                input_handled = true;
            } else if let Some(active) = self.active_focus.take() {
                if !active.is_multiline {
                    active.state.scroll_x.set(0.0);
                    if let Some(layout) = Self::find_layout_for_focus_mut(
                        &self.root_blueprint,
                        &mut self.layout_root,
                        active.input_index,
                        active.input_id.as_deref(),
                        active.entity_id,
                    ) {
                        layout.geometry.scroll_offset.x = 0.0;
                    }
                }
                let key = active
                    .input_id
                    .clone()
                    .unwrap_or_else(|| format!("input:{}", active.input_index));
                self.input_states.insert(key.clone(), active.state.clone());
                if let Some(prev_prop) = self.active_input_last_prop_value.take() {
                    self.input_last_prop_values.insert(key, prev_prop);
                }
                self.drag_selection_active = false;
                self.last_click_info = None;
                if let Some(ref blur_cb) = active.on_blur {
                    blur_cb(strata_dom::Event::new(strata_dom::EventType::Blur));
                }
                self.sync_active_input_layout();
                input_handled = true;
            } else {
                self.drag_selection_active = false;
            }
        }

        // Also dispatch PointerDown and MouseDown to DOM listeners
        let pre_open_outside_ids = strata_dom::hooks::snapshot_open_click_outside_ids();
        let mut inside_click_stopped = self.point_hits_non_backdrop_top_layer_portal(px, py);
        self.pointer_capture_target = None;
        self.capture_lost_while_mouse_down = false;
        let now_click = self.now();
        let dom_click_detail =
            if let Some((last_t, lx, ly, last_btn, last_cnt)) = self.last_dom_click {
                if last_btn == button
                    && now_click.duration_since(last_t).as_millis() <= 450
                    && (px - lx).abs() <= 6.0
                    && (py - ly).abs() <= 6.0
                {
                    if last_cnt >= 3 {
                        1
                    } else {
                        last_cnt + 1
                    }
                } else {
                    1
                }
            } else {
                1
            };
        self.last_dom_click = Some((now_click, px, py, button, dom_click_detail));
        let mut dom_handled = false;
        let mut captured_target: Option<PointerCaptureTarget> = None;
        let pd_outcome = self.dispatch_bubbling_event_detailed_at(
            strata_dom::EventType::PointerDown,
            px,
            py,
            button,
            dom_click_detail,
            |_| {},
        );
        if pd_outcome.handled {
            dom_handled = true;
            if pd_outcome.propagation_stopped || pd_outcome.default_prevented {
                inside_click_stopped = true;
            }
            if let Some(cap_eid) = pd_outcome.capture_entity.or(pd_outcome.target_entity) {
                if let Some(target_path) = self.layout_root.find_path_by_entity_id(cap_eid) {
                    match pd_outcome.capture_requested {
                        Some(true) => {
                            captured_target = Some(self.build_pointer_capture_target(
                                target_path,
                                true,
                                cap_eid,
                                button,
                            ));
                        }
                        Some(false) => {}
                        None => {
                            captured_target = Some(self.build_pointer_capture_target(
                                target_path,
                                false,
                                cap_eid,
                                button,
                            ));
                        }
                    }
                }
            }
        }
        let md_outcome = self.dispatch_bubbling_event_detailed_at(
            strata_dom::EventType::MouseDown,
            px,
            py,
            button,
            dom_click_detail,
            |_| {},
        );
        if md_outcome.handled {
            dom_handled = true;
            if md_outcome.propagation_stopped || md_outcome.default_prevented {
                inside_click_stopped = true;
            }
            if let Some(cap_eid) = md_outcome.capture_entity.or(md_outcome.target_entity) {
                if let Some(target_path) = self.layout_root.find_path_by_entity_id(cap_eid) {
                    match md_outcome.capture_requested {
                        Some(true) => {
                            captured_target = Some(self.build_pointer_capture_target(
                                target_path,
                                true,
                                cap_eid,
                                button,
                            ));
                        }
                        Some(false) => captured_target = None,
                        None if captured_target.is_none() => {
                            captured_target = Some(self.build_pointer_capture_target(
                                target_path,
                                false,
                                cap_eid,
                                button,
                            ));
                        }
                        None => {}
                    }
                }
            }
        }
        self.pointer_capture_target = captured_target;

        if button == 0 {
            self.active_dom_drag = self.find_draggable_path_at(px, py).map(|source_path| {
                ActiveDomDrag {
                    source_path,
                    start_pos: (px, py),
                    started: false,
                    last_over_path: None,
                    data_transfer: Default::default(),
                }
            });
        }

        if !pre_open_outside_ids.is_empty()
            && !inside_click_stopped
            && strata_dom::hooks::dispatch_click_outside_for_ids(&pre_open_outside_ids)
        {
            dom_handled = true;
        }

        let focus_changed = if button == 0 {
            let focus_path = self.find_focus_path_at(px, py);
            self.set_focus_path(focus_path)
        } else {
            false
        };

        if dom_handled {
            PointerDownAction::DomListenerInvoked
        } else if input_handled || focus_changed {
            PointerDownAction::FocusChanged
        } else {
            PointerDownAction::Ignored
        }
    }

    /// Dispatches pointer move at (px, py), returning detailed action result.
    pub fn dispatch_pointer_move_action(&mut self, px: f32, py: f32) -> PointerMoveAction {
        let mut selection_dragged = false;
        if self.drag_selection_active {
            let is_password = self
                .active_focus
                .as_ref()
                .is_some_and(|a| !a.is_multiline && self.active_input_is_password(a));
            if let Some(ref mut active) = self.active_focus {
                let layout_opt = Self::find_layout_for_focus(
                    &self.root_blueprint,
                    &self.layout_root,
                    active.input_index,
                    active.input_id.as_deref(),
                    active.entity_id,
                );

                if let Some(layout) = layout_opt {
                    let is_mono = strata_layout::is_monospace_family(layout.style.font_family);
                    let raw_text = active.state.text.get();
                    let measure_text = if is_password {
                        Self::mask_password_text(&raw_text)
                    } else {
                        raw_text
                    };
                    let char_idx = Self::cursor_index_at_input_point(
                        &measure_text,
                        active.is_multiline,
                        px,
                        py,
                        layout.geometry.content_box.x,
                        layout.geometry.content_box.y,
                        layout.geometry.content_box.width,
                        active.state.scroll_x.get(),
                        active.state.scroll_y.get(),
                        layout.style.font_size,
                        layout.style.line_height,
                        is_mono,
                        layout.style.font_family,
                    );
                    active.state.cursor_index.set(char_idx);
                    self.reset_caret_blink();
                    self.sync_active_input_layout_and_scroll_caret();
                    self.mark_paint_dirty();
                    selection_dragged = true;
                }
            }
        }

        let (movement_x, movement_y) = if let Some((wx, wy)) = self.pending_cursor_warp {
            if (px - wx).abs() <= 2.5 && (py - wy).abs() <= 2.5 {
                self.pending_cursor_warp = None;
                self.last_pointer_pos = Some((px, py));
                (0.0, 0.0)
            } else {
                self.pending_cursor_warp = None;
                let delta = self
                    .last_pointer_pos
                    .map(|(lx, ly)| ((px - lx) as f64, (py - ly) as f64))
                    .unwrap_or((0.0, 0.0));
                self.last_pointer_pos = Some((px, py));
                delta
            }
        } else {
            let delta = self
                .last_pointer_pos
                .map(|(lx, ly)| ((px - lx) as f64, (py - ly) as f64))
                .unwrap_or((0.0, 0.0));
            self.last_pointer_pos = Some((px, py));
            delta
        };

        let mut dom_handled = false;
        let mut requested_warp: Option<(f64, f64)> = None;
        let mut resolved_capture: Option<(Vec<usize>, bool, i16, i32)> =
            if let Some(ref cap) = self.pointer_capture_target.clone() {
                match self.resolve_pointer_capture(cap) {
                    CaptureResolution::Resolved(resolved_path) => {
                        if let Some(ref mut live_cap) = self.pointer_capture_target {
                            if live_cap.path != resolved_path {
                                live_cap.path = resolved_path.clone();
                            }
                            if let Some((_, l)) = Self::resolve_chain_for_path(
                                &self.root_blueprint,
                                &self.layout_root,
                                &resolved_path,
                            )
                            .last()
                            {
                                if l.entity_id.is_valid() {
                                    live_cap.entity_id = Some(l.entity_id);
                                }
                            }
                        }
                        Some((resolved_path, cap.explicit, cap.button, cap.pointer_id))
                    }
                    CaptureResolution::Unmounted => {
                        let lost = self.pointer_capture_target.take();
                        if let Some(lost_cap) = lost {
                            if self.is_mouse_down || lost_cap.explicit {
                                self.capture_lost_while_mouse_down = true;
                            }
                            if lost_cap.explicit {
                                if let Some(handler) = lost_cap.lost_capture_handler {
                                    let mut ev = self.create_mouse_event(
                                        strata_dom::EventType::LostPointerCapture,
                                        px,
                                        py,
                                        lost_cap.button,
                                        0,
                                        None,
                                    );
                                    ev.pointer_id = lost_cap.pointer_id;
                                    ev.data = Some("target_unmounted".to_string());
                                    handler(ev);
                                    dom_handled = true;
                                }
                            }
                        }
                        None
                    }
                }
            } else {
                None
            };
        let move_button: i16 = if self.is_mouse_down { 0 } else { -1 };
        let mut released_capture_meta: Option<(Vec<usize>, i16, i32)> = None;
        for ev_type in [
            strata_dom::EventType::PointerMove,
            strata_dom::EventType::MouseMove,
        ] {
            match resolved_capture {
                Some((ref cap_path, true, cap_btn, cap_pid)) => {
                    if let Some((handler, target_box, _)) = Self::find_event_at_or_above_path(
                        &self.root_blueprint,
                        &self.layout_root,
                        cap_path,
                        ev_type,
                    ) {
                        let target_box_copy = *target_box;
                        let mut ev = self.create_mouse_event(
                            ev_type,
                            px,
                            py,
                            move_button,
                            0,
                            Some(&target_box_copy),
                        );
                        ev.movement_x = movement_x;
                        ev.movement_y = movement_y;
                        ev.pointer_id = cap_pid;
                        let capture_req = ev.pointer_capture_requested.clone();
                        let warp_req = ev.cursor_warp_requested.clone();
                        handler(ev);
                        dom_handled = true;
                        if capture_req.get() == Some(false) {
                            self.pointer_capture_target = None;
                            released_capture_meta = Some((cap_path.clone(), cap_btn, cap_pid));
                            resolved_capture = None;
                        }
                        if let Some(pos) = warp_req.get() {
                            requested_warp = Some(pos);
                        }
                    }
                }
                Some((ref cap_path, false, _, cap_pid)) => {
                    let outcome = self.dispatch_bubbling_event_detailed_at(
                        ev_type,
                        px,
                        py,
                        move_button,
                        0,
                        |ev| {
                            ev.movement_x = movement_x;
                            ev.movement_y = movement_y;
                        },
                    );
                    if outcome.handled {
                        dom_handled = true;
                        if let Some(pos) = outcome.cursor_warp_requested {
                            requested_warp = Some(pos);
                        }
                    }
                    if let Some((handler, target_box, cap_eid)) = Self::find_event_at_or_above_path(
                        &self.root_blueprint,
                        &self.layout_root,
                        cap_path,
                        ev_type,
                    ) {
                        let suppressed_by_bubble_stop = outcome.propagation_stopped
                            && outcome.chain_entities.contains(&cap_eid);
                        if !outcome.invoked_entities.contains(&cap_eid) && !suppressed_by_bubble_stop
                        {
                            let target_box_copy = *target_box;
                            let mut ev = self.create_mouse_event(
                                ev_type,
                                px,
                                py,
                                move_button,
                                0,
                                Some(&target_box_copy),
                            );
                            ev.movement_x = movement_x;
                            ev.movement_y = movement_y;
                            ev.pointer_id = cap_pid;
                            let capture_req = ev.pointer_capture_requested.clone();
                            let warp_req = ev.cursor_warp_requested.clone();
                            handler(ev);
                            dom_handled = true;
                            if capture_req.get() == Some(false) {
                                self.pointer_capture_target = None;
                                resolved_capture = None;
                            }
                            if let Some(pos) = warp_req.get() {
                                requested_warp = Some(pos);
                            }
                        }
                    }
                }
                None => {
                    let outcome = self.dispatch_bubbling_event_detailed_at(
                        ev_type,
                        px,
                        py,
                        move_button,
                        0,
                        |ev| {
                            ev.movement_x = movement_x;
                            ev.movement_y = movement_y;
                        },
                    );
                    if outcome.handled {
                        dom_handled = true;
                        if let Some(pos) = outcome.cursor_warp_requested {
                            requested_warp = Some(pos);
                        }
                    }
                }
            }
        }
        if let Some((ref cap_path, cap_btn, cap_pid)) = released_capture_meta {
            let _ = self.dispatch_lost_pointer_capture_at_path(cap_path, px, py, cap_btn, cap_pid);
        }
        if let Some((wx, wy)) = requested_warp {
            self.warp_cursor(wx as f32, wy as f32);
        }

        if let Some(mut drag) = self.active_dom_drag.take() {
            let mut cancel_drag = false;
            if !drag.started {
                let (sx, sy) = drag.start_pos;
                if (px - sx).abs() > 3.0 || (py - sy).abs() > 3.0 {
                    drag.started = true;
                    self.cancel_pending_click();
                    let (invoked, prevented) = self.dispatch_drag_event_at_path(
                        &drag.source_path,
                        strata_dom::EventType::DragStart,
                        sx,
                        sy,
                        &drag.data_transfer,
                    );
                    if invoked {
                        dom_handled = true;
                    }
                    if prevented {
                        cancel_drag = true;
                    }
                }
            }

            if drag.started && !cancel_drag {
                let (drag_invoked, _) = self.dispatch_drag_event_at_path(
                    &drag.source_path,
                    strata_dom::EventType::Drag,
                    px,
                    py,
                    &drag.data_transfer,
                );
                if drag_invoked {
                    dom_handled = true;
                }

                let over_path = self.find_drop_target_path_at(px, py);
                if over_path != drag.last_over_path {
                    if let Some(ref old_path) = drag.last_over_path {
                        let (leave_invoked, _) = self.dispatch_drag_event_at_path(
                            old_path,
                            strata_dom::EventType::DragLeave,
                            px,
                            py,
                            &drag.data_transfer,
                        );
                        if leave_invoked {
                            dom_handled = true;
                        }
                    }
                    if let Some(ref new_path) = over_path {
                        let (enter_invoked, _) = self.dispatch_drag_event_at_path(
                            new_path,
                            strata_dom::EventType::DragEnter,
                            px,
                            py,
                            &drag.data_transfer,
                        );
                        if enter_invoked {
                            dom_handled = true;
                        }
                    }
                    drag.last_over_path = over_path.clone();
                }

                if let Some(ref cur_over) = over_path {
                    let (over_invoked, _) = self.dispatch_drag_event_at_path(
                        cur_over,
                        strata_dom::EventType::DragOver,
                        px,
                        py,
                        &drag.data_transfer,
                    );
                    if over_invoked {
                        dom_handled = true;
                    }
                }
            }

            if !cancel_drag {
                self.active_dom_drag = Some(drag);
            }
        }

        if dom_handled {
            PointerMoveAction::DomListenerInvoked
        } else if selection_dragged {
            PointerMoveAction::SelectionDragged
        } else {
            PointerMoveAction::Ignored
        }
    }

    /// Dispatches pointer move at (px, py) to update mouse drag text selection and DOM move listeners.
    pub fn dispatch_pointer_move(&mut self, px: f32, py: f32) -> bool {
        self.dispatch_pointer_move_action(px, py).is_handled()
    }

    /// Dispatches pointer up with primary button (`0`), returning detailed action result.
    pub fn dispatch_pointer_up_action(&mut self, px: f32, py: f32) -> PointerUpAction {
        self.dispatch_pointer_up_action_with_button(px, py, 0)
    }

    /// Dispatches pointer up with explicit mouse button (`0` = Left, `1` = Middle, `2` = Right).
    pub fn dispatch_pointer_up_action_with_button(
        &mut self,
        px: f32,
        py: f32,
        button: i16,
    ) -> PointerUpAction {
        let mut selection_ended = false;
        if button == 0 && self.drag_selection_active {
            self.drag_selection_active = false;
            if let Some(ref mut active) = self.active_focus {
                if let Some(anchor) = active.state.selection_anchor.get() {
                    if anchor == active.state.cursor_index.get() {
                        active.state.selection_anchor.set(None);
                    }
                }
            }
            selection_ended = true;
        }

        let lost_capture_during_press = std::mem::take(&mut self.capture_lost_while_mouse_down);
        let capture = self.pointer_capture_target.take();
        let resolved_capture: Option<(Vec<usize>, bool, i32)> = capture.as_ref().and_then(|cap| {
            match self.resolve_pointer_capture(cap) {
                CaptureResolution::Resolved(path) => Some((path, cap.explicit, cap.pointer_id)),
                CaptureResolution::Unmounted => None,
            }
        });
        let mut dom_handled = false;
        for ev_type in [
            strata_dom::EventType::PointerUp,
            strata_dom::EventType::MouseUp,
        ] {
            match resolved_capture {
                Some((ref cap_path, true, cap_pid)) => {
                    if let Some((handler, target_box, _)) = Self::find_event_at_or_above_path(
                        &self.root_blueprint,
                        &self.layout_root,
                        cap_path,
                        ev_type,
                    ) {
                        let mut ev =
                            self.create_mouse_event(ev_type, px, py, button, 1, Some(target_box));
                        ev.pointer_id = cap_pid;
                        handler(ev);
                        dom_handled = true;
                    }
                }
                Some((ref cap_path, false, cap_pid)) => {
                    let outcome = self.dispatch_bubbling_event_detailed_at(
                        ev_type,
                        px,
                        py,
                        button,
                        1,
                        |_| {},
                    );
                    if outcome.handled {
                        dom_handled = true;
                    }
                    if let Some((handler, target_box, cap_eid)) = Self::find_event_at_or_above_path(
                        &self.root_blueprint,
                        &self.layout_root,
                        cap_path,
                        ev_type,
                    ) {
                        let suppressed_by_bubble_stop = outcome.propagation_stopped
                            && outcome.chain_entities.contains(&cap_eid);
                        if !outcome.invoked_entities.contains(&cap_eid) && !suppressed_by_bubble_stop
                        {
                            let mut ev = self.create_mouse_event(
                                ev_type,
                                px,
                                py,
                                button,
                                1,
                                Some(target_box),
                            );
                            ev.pointer_id = cap_pid;
                            handler(ev);
                            dom_handled = true;
                        }
                    }
                }
                None => {
                    let outcome = self.dispatch_bubbling_event_detailed_at(
                        ev_type,
                        px,
                        py,
                        button,
                        1,
                        |_| {},
                    );
                    if outcome.handled {
                        dom_handled = true;
                    }
                }
            }
        }

        if let Some((ref cap_path, true, cap_pid)) = resolved_capture {
            if self.dispatch_lost_pointer_capture_at_path(cap_path, px, py, button, cap_pid) {
                dom_handled = true;
            }
        } else if let Some(ref cap) = capture {
            if cap.explicit {
                if let Some(ref handler) = cap.lost_capture_handler {
                    let mut ev = self.create_mouse_event(
                        strata_dom::EventType::LostPointerCapture,
                        px,
                        py,
                        button,
                        0,
                        None,
                    );
                    ev.pointer_id = cap.pointer_id;
                    ev.data = Some("target_unmounted".to_string());
                    handler(ev);
                    dom_handled = true;
                }
            }
        }

        if button == 0 {
            if let Some(drag) = self.active_dom_drag.take() {
                if drag.started {
                    let drop_path = self.find_drop_target_path_at(px, py).or(drag.last_over_path);
                    let mut drop_dispatched = false;
                    if let Some(ref target_path) = drop_path {
                        let (invoked, _) = self.dispatch_drag_event_at_path(
                            target_path,
                            strata_dom::EventType::Drop,
                            px,
                            py,
                            &drag.data_transfer,
                        );
                        if invoked {
                            drop_dispatched = true;
                            dom_handled = true;
                        }
                    }
                    if !drop_dispatched {
                        if let Some((handler, target_box)) = Self::hit_test_event_with_box(
                            &self.root_blueprint,
                            &self.layout_root,
                            px,
                            py,
                            strata_dom::EventType::Drop,
                        ) {
                            let target_box_copy = *target_box;
                            let mut ev = self.create_mouse_event(
                                strata_dom::EventType::Drop,
                                px,
                                py,
                                0,
                                1,
                                Some(&target_box_copy),
                            );
                            ev.data_transfer = std::rc::Rc::clone(&drag.data_transfer);
                            ev.data = drag
                                .data_transfer
                                .borrow()
                                .get("text/plain")
                                .cloned()
                                .or_else(|| drag.data_transfer.borrow().values().next().cloned());
                            handler(ev);
                            dom_handled = true;
                        }
                    }

                    let (end_invoked, _) = self.dispatch_drag_event_at_path(
                        &drag.source_path,
                        strata_dom::EventType::DragEnd,
                        px,
                        py,
                        &drag.data_transfer,
                    );
                    if end_invoked {
                        dom_handled = true;
                    }
                }
            }
        }

        // Document/window-level pointer-up hooks fire unconditionally even if the capturing
        // element unmounted or moved across portals mid-gesture.
        if !self.window_pointer_up_listeners.is_empty()
            || strata_dom::hooks::has_global_pointer_up_listeners()
        {
            let mut up_ev = self.create_mouse_event(
                strata_dom::EventType::PointerUp,
                px,
                py,
                button,
                1,
                None,
            );
            if let Some(ref cap) = capture {
                up_ev.pointer_id = cap.pointer_id;
            }
            let global_invoked = self.dispatch_window_and_global_pointer_up(&up_ev);
            if global_invoked
                && (capture.is_some()
                    || lost_capture_during_press
                    || strata_dom::reactive::has_dirty_signals())
            {
                dom_handled = true;
            }
        }

        if dom_handled {
            PointerUpAction::DomListenerInvoked
        } else if selection_ended {
            PointerUpAction::SelectionEnded
        } else {
            PointerUpAction::Ignored
        }
    }

    /// Dispatches pointer up to finalize mouse drag text selection and notify DOM up listeners.
    pub fn dispatch_pointer_up(&mut self, px: f32, py: f32) -> bool {
        self.dispatch_pointer_up_action(px, py).is_handled()
    }

    /// Finds the deepest focusable element path at `(px, py)`.
    pub(crate) fn find_focus_path_at(&self, px: f32, py: f32) -> Option<Vec<usize>> {
        let hit_path = Self::hit_test_node_path(&self.root_blueprint, &self.layout_root, px, py)?;
        let mut cur = &self.root_blueprint;
        let is_focusable = |bp: &NodeBlueprint| -> bool {
            if let NodeBlueprint::Element {
                tag, attributes, ..
            } = bp
            {
                let tag_lower = Self::tag_as_str(tag).to_ascii_lowercase();
                if matches!(
                    tag_lower.as_str(),
                    "button" | "input" | "textarea" | "select" | "a"
                ) {
                    return true;
                }
                for attr in attributes {
                    match attr {
                        strata_dom::Attribute::EventListener { event, .. }
                            if *event == strata_dom::EventType::KeyDown
                                || *event == strata_dom::EventType::KeyUp =>
                        {
                            return true;
                        }
                        strata_dom::Attribute::Property { key, .. }
                            if key == "tabindex" || key == "role" =>
                        {
                            return true;
                        }
                        _ => {}
                    }
                }
            }
            false
        };
        let mut best: Option<Vec<usize>> = if is_focusable(cur) {
            Some(Vec::new())
        } else {
            None
        };
        let mut prefix = Vec::with_capacity(hit_path.len());
        for &idx in &hit_path {
            let mut flat = Vec::new();
            match cur {
                NodeBlueprint::Element { children, .. } | NodeBlueprint::Fragment(children) => {
                    for c in children {
                        Self::flatten_blueprints(c, &mut flat);
                    }
                }
                _ => break,
            }
            if idx >= flat.len() {
                break;
            }
            cur = flat[idx];
            prefix.push(idx);
            if is_focusable(cur) {
                best = Some(prefix.clone());
            }
        }
        best
    }

    fn keycode_to_dom_key(key: strata_platform::KeyCode, shift: bool) -> String {
        use strata_platform::KeyCode;
        match key {
            KeyCode::ArrowDown => "ArrowDown".to_string(),
            KeyCode::ArrowUp => "ArrowUp".to_string(),
            KeyCode::ArrowLeft => "ArrowLeft".to_string(),
            KeyCode::ArrowRight => "ArrowRight".to_string(),
            KeyCode::Home => "Home".to_string(),
            KeyCode::End => "End".to_string(),
            KeyCode::PageUp => "PageUp".to_string(),
            KeyCode::PageDown => "PageDown".to_string(),
            KeyCode::Enter => "Enter".to_string(),
            KeyCode::Escape => "Escape".to_string(),
            KeyCode::Tab => "Tab".to_string(),
            KeyCode::Space => " ".to_string(),
            KeyCode::Backspace => "Backspace".to_string(),
            KeyCode::Delete => "Delete".to_string(),
            KeyCode::Digit0 => if shift { ")" } else { "0" }.to_string(),
            KeyCode::Digit1 => if shift { "!" } else { "1" }.to_string(),
            KeyCode::Digit2 => if shift { "@" } else { "2" }.to_string(),
            KeyCode::Digit3 => if shift { "#" } else { "3" }.to_string(),
            KeyCode::Digit4 => if shift { "$" } else { "4" }.to_string(),
            KeyCode::Digit5 => if shift { "%" } else { "5" }.to_string(),
            KeyCode::Digit6 => if shift { "^" } else { "6" }.to_string(),
            KeyCode::Digit7 => if shift { "&" } else { "7" }.to_string(),
            KeyCode::Digit8 => if shift { "*" } else { "8" }.to_string(),
            KeyCode::Digit9 => if shift { "(" } else { "9" }.to_string(),
            KeyCode::Slash => if shift { "?" } else { "/" }.to_string(),
            KeyCode::Comma => if shift { "<" } else { "," }.to_string(),
            KeyCode::Period => if shift { ">" } else { "." }.to_string(),
            KeyCode::Minus => if shift { "_" } else { "-" }.to_string(),
            KeyCode::Equals => if shift { "+" } else { "=" }.to_string(),
            KeyCode::BracketLeft => if shift { "{" } else { "[" }.to_string(),
            KeyCode::BracketRight => if shift { "}" } else { "]" }.to_string(),
            KeyCode::Semicolon => if shift { ":" } else { ";" }.to_string(),
            KeyCode::Quote => if shift { "\"" } else { "'" }.to_string(),
            KeyCode::Backslash => if shift { "|" } else { "\\" }.to_string(),
            KeyCode::Backquote => if shift { "~" } else { "`" }.to_string(),
            other => {
                let dbg = format!("{:?}", other);
                if let Some(letter) = dbg.strip_prefix("Key") {
                    if shift {
                        letter.to_string()
                    } else {
                        letter.to_ascii_lowercase()
                    }
                } else {
                    dbg
                }
            }
        }
    }

    fn dispatch_global_shortcut_for_key(
        key: strata_platform::KeyCode,
        modifiers: strata_platform::Modifiers,
    ) -> bool {
        let dom_key = Self::keycode_to_dom_key(key, false);
        strata_dom::hooks::dispatch_global_keydown(
            &dom_key,
            modifiers.ctrl,
            modifiers.shift,
            modifiers.alt,
            modifiers.meta,
        )
    }

    /// Dispatches a bubbling DOM event along the active focus path (or the focused `<input>`'s
    /// path when `current_focus_path` is unset), returning `(handled, default_prevented, propagation_stopped)`.
    fn dispatch_dom_event_along_focus_path(&self, ev: strata_dom::Event) -> (bool, bool, bool) {
        let fallback_path;
        let path: &[usize] = if let Some(ref p) = self.current_focus_path {
            p.as_slice()
        } else if let Some(ref active) = self.active_focus {
            if let Some(p) = active
                .entity_id
                .and_then(|eid| self.layout_root.find_path_by_entity_id(eid))
            {
                fallback_path = p;
                &fallback_path
            } else {
                return (false, false, false);
            }
        } else {
            return (false, false, false);
        };

        let chain = Self::resolve_chain_for_path(&self.root_blueprint, &self.layout_root, path);
        let event_type = ev.event_type;
        let mut handled = false;
        let mut default_prevented = false;
        for (bp, _) in chain.into_iter().rev() {
            if let NodeBlueprint::Element { attributes, .. } = bp {
                for attr in attributes {
                    if let strata_dom::Attribute::EventListener { event, handler } = attr {
                        if *event == event_type {
                            let call_ev = ev.clone();
                            let prevented_cell = call_ev.default_prevented.clone();
                            let stopped_cell = call_ev.propagation_stopped.clone();
                            handler(call_ev);
                            handled = true;
                            if prevented_cell.get() {
                                default_prevented = true;
                            }
                            if stopped_cell.get() {
                                return (true, default_prevented, true);
                            }
                        }
                    }
                }
            }
        }
        (handled, default_prevented, false)
    }

    fn dispatch_dom_key_event(
        &self,
        event_type: strata_dom::EventType,
        key: strata_platform::KeyCode,
        modifiers: strata_platform::Modifiers,
    ) -> (bool, bool, bool) {
        let dom_key = Self::keycode_to_dom_key(key, modifiers.shift);
        let mut ev = strata_dom::Event::new(event_type);
        ev.key = dom_key;
        ev.ctrl_key = modifiers.ctrl;
        ev.shift_key = modifiers.shift;
        ev.alt_key = modifiers.alt;
        ev.meta_key = modifiers.meta;
        self.dispatch_dom_event_along_focus_path(ev)
    }

    /// Dispatches keyboard events to the active focused input element, returning a fine-grained KeyDispatchResult.
    pub fn dispatch_key_event(&mut self, event: &PlatformEvent) -> KeyDispatchResult {
        if self.active_focus.is_none() {
            match event {
                PlatformEvent::KeyDown { key, modifiers } => {
                    self.last_dom_keydown_consumed_char = None;
                    let (dom_handled, dom_default_prevented, dom_stopped) = self
                        .dispatch_dom_key_event(strata_dom::EventType::KeyDown, *key, *modifiers);
                    if dom_handled && !modifiers.ctrl && !modifiers.alt && !modifiers.meta {
                        let dom_key = Self::keycode_to_dom_key(*key, modifiers.shift);
                        let mut chars = dom_key.chars();
                        if let (Some(ch), None) = (chars.next(), chars.next()) {
                            if !ch.is_control() {
                                self.last_dom_keydown_consumed_char = Some((ch, self.now()));
                            }
                        }
                    }
                    if dom_default_prevented || dom_stopped {
                        return KeyDispatchResult::NeedsRebuild;
                    }
                    if Self::dispatch_global_shortcut_for_key(*key, *modifiers) || dom_handled {
                        return KeyDispatchResult::NeedsRebuild;
                    }
                    let (px, py) = self.last_mouse_pos;
                    let scrolled = match key {
                        strata_platform::KeyCode::ArrowDown => {
                            self.dispatch_scroll(px, py, 0.0, 40.0)
                        }
                        strata_platform::KeyCode::ArrowUp => {
                            self.dispatch_scroll(px, py, 0.0, -40.0)
                        }
                        strata_platform::KeyCode::PageDown => {
                            self.dispatch_scroll(px, py, 0.0, 400.0)
                        }
                        strata_platform::KeyCode::PageUp => {
                            self.dispatch_scroll(px, py, 0.0, -400.0)
                        }
                        strata_platform::KeyCode::Home => self.scroll_to_top(px, py),
                        strata_platform::KeyCode::End => self.scroll_to_bottom(px, py),
                        _ => false,
                    };
                    if scrolled {
                        return KeyDispatchResult::RedrawOnly;
                    }
                }
                PlatformEvent::KeyUp { key, modifiers } => {
                    self.last_dom_keydown_consumed_char = None;
                    if self
                        .dispatch_dom_key_event(strata_dom::EventType::KeyUp, *key, *modifiers)
                        .0
                    {
                        return KeyDispatchResult::NeedsRebuild;
                    }
                }
                PlatformEvent::ReceivedCharacter(ch) => {
                    if !ch.is_control() && self.current_focus_path.is_some() {
                        if let Some((prev, ts)) = self.last_dom_keydown_consumed_char.take() {
                            if prev == *ch
                                && self.elapsed_since(ts) < std::time::Duration::from_millis(50)
                            {
                                return KeyDispatchResult::Ignored;
                            }
                        }
                        let text = ch.to_string();
                        let mut key_ev = strata_dom::Event::new(strata_dom::EventType::KeyDown);
                        key_ev.key = text.clone();
                        key_ev.data = Some(text.clone());
                        key_ev.input_type = "insertText".to_string();
                        let (key_handled, _, _) =
                            self.dispatch_dom_event_along_focus_path(key_ev);
                        if key_handled {
                            return KeyDispatchResult::NeedsRebuild;
                        }
                        let mut input_ev =
                            strata_dom::Event::with_value(strata_dom::EventType::Input, text.clone());
                        input_ev.data = Some(text);
                        input_ev.input_type = "insertText".to_string();
                        if self.dispatch_dom_event_along_focus_path(input_ev).0 {
                            return KeyDispatchResult::NeedsRebuild;
                        }
                    }
                }
                PlatformEvent::ImeCommit(text) => {
                    self.last_dom_keydown_consumed_char = None;
                    if !text.is_empty() && self.current_focus_path.is_some() {
                        let mut input_ev = strata_dom::Event::with_value(
                            strata_dom::EventType::Input,
                            text.clone(),
                        );
                        input_ev.data = Some(text.clone());
                        input_ev.input_type = "insertCompositionText".to_string();
                        if self.dispatch_dom_event_along_focus_path(input_ev).0 {
                            return KeyDispatchResult::NeedsRebuild;
                        }
                        let mut any_handled = false;
                        for ch in text.chars() {
                            if ch.is_control() {
                                continue;
                            }
                            let s = ch.to_string();
                            let mut key_ev = strata_dom::Event::new(strata_dom::EventType::KeyDown);
                            key_ev.key = s.clone();
                            key_ev.data = Some(s);
                            key_ev.input_type = "insertCompositionText".to_string();
                            if self.dispatch_dom_event_along_focus_path(key_ev).0 {
                                any_handled = true;
                            }
                        }
                        if any_handled {
                            return KeyDispatchResult::NeedsRebuild;
                        }
                    }
                }
                _ => {}
            }
            return KeyDispatchResult::Ignored;
        }

        match event {
            PlatformEvent::ImeCommit(text) => {
                if !text.is_empty() {
                    self.reset_caret_blink();
                    let active = self.active_focus.as_mut().unwrap();
                    active.state.insert_text(text);
                    let val = active.state.text.get();
                    if let Some(ref on_input) = active.on_input {
                        on_input(strata_dom::Event::with_value(
                            strata_dom::EventType::Input,
                            val,
                        ));
                    }
                    self.sync_active_input_layout_and_scroll_caret();
                    KeyDispatchResult::NeedsRebuild
                } else {
                    KeyDispatchResult::Ignored
                }
            }
            PlatformEvent::ImePreedit { .. } => KeyDispatchResult::RedrawOnly,
            PlatformEvent::ReceivedCharacter(ch) => {
                if !ch.is_control() {
                    self.reset_caret_blink();
                    let active = self.active_focus.as_mut().unwrap();
                    active.state.insert_text(&ch.to_string());
                    let val = active.state.text.get();
                    if let Some(ref on_input) = active.on_input {
                        on_input(strata_dom::Event::with_value(
                            strata_dom::EventType::Input,
                            val,
                        ));
                    }
                    self.sync_active_input_layout_and_scroll_caret();
                    KeyDispatchResult::NeedsRebuild
                } else {
                    KeyDispatchResult::Ignored
                }
            }
            PlatformEvent::KeyUp { key, modifiers } => {
                if self
                    .dispatch_dom_key_event(strata_dom::EventType::KeyUp, *key, *modifiers)
                    .0
                {
                    KeyDispatchResult::NeedsRebuild
                } else {
                    KeyDispatchResult::Ignored
                }
            }
            PlatformEvent::KeyDown { key, modifiers } => {
                let (dom_handled, dom_default_prevented, _) =
                    self.dispatch_dom_key_event(strata_dom::EventType::KeyDown, *key, *modifiers);
                if dom_default_prevented {
                    return KeyDispatchResult::NeedsRebuild;
                }

                let mut text_changed = false;
                let mut nav_or_sel = false;
                let mut submit = false;
                let mut blur = false;

                let is_cmd = modifiers.ctrl || modifiers.meta;
                if is_cmd && !modifiers.alt {
                    let cmd = match key {
                        strata_platform::KeyCode::KeyA => Some(EditCommand::SelectAll),
                        strata_platform::KeyCode::KeyC => Some(EditCommand::Copy),
                        strata_platform::KeyCode::KeyX => Some(EditCommand::Cut),
                        strata_platform::KeyCode::KeyV => Some(EditCommand::Paste),
                        strata_platform::KeyCode::KeyZ if modifiers.shift => {
                            Some(EditCommand::Redo)
                        }
                        strata_platform::KeyCode::KeyZ => Some(EditCommand::Undo),
                        strata_platform::KeyCode::KeyY => Some(EditCommand::Redo),
                        _ => None,
                    };
                    if let Some(cmd) = cmd {
                        let res = self.apply_edit_command(cmd);
                        if matches!(cmd, EditCommand::Copy | EditCommand::SelectAll) {
                            self.sync_active_input_layout();
                        } else {
                            self.sync_active_input_layout_and_scroll_caret();
                        }
                        if res {
                            return match cmd {
                                EditCommand::SelectAll | EditCommand::Copy => {
                                    self.mark_paint_dirty();
                                    KeyDispatchResult::RedrawOnly
                                }
                                _ => KeyDispatchResult::NeedsRebuild,
                            };
                        } else if dom_handled {
                            return KeyDispatchResult::NeedsRebuild;
                        } else {
                            return KeyDispatchResult::Ignored;
                        }
                    }
                }

                let is_global_candidate = is_cmd
                    || modifiers.alt
                    || matches!(
                        key,
                        strata_platform::KeyCode::Escape
                            | strata_platform::KeyCode::F1
                            | strata_platform::KeyCode::F2
                            | strata_platform::KeyCode::F3
                            | strata_platform::KeyCode::F4
                            | strata_platform::KeyCode::F5
                            | strata_platform::KeyCode::F6
                            | strata_platform::KeyCode::F7
                            | strata_platform::KeyCode::F8
                            | strata_platform::KeyCode::F9
                            | strata_platform::KeyCode::F10
                            | strata_platform::KeyCode::F11
                            | strata_platform::KeyCode::F12
                    );
                if is_global_candidate && Self::dispatch_global_shortcut_for_key(*key, *modifiers) {
                    return KeyDispatchResult::NeedsRebuild;
                }

                let is_multiline = self.active_focus.as_ref().is_some_and(|a| a.is_multiline);
                if is_multiline {
                    match key {
                        strata_platform::KeyCode::ArrowUp => {
                            self.move_active_input_vertical(-1, false, modifiers.shift);
                            nav_or_sel = true;
                        }
                        strata_platform::KeyCode::ArrowDown => {
                            self.move_active_input_vertical(1, false, modifiers.shift);
                            nav_or_sel = true;
                        }
                        strata_platform::KeyCode::PageUp => {
                            self.move_active_input_vertical(-1, true, modifiers.shift);
                            nav_or_sel = true;
                        }
                        strata_platform::KeyCode::PageDown => {
                            self.move_active_input_vertical(1, true, modifiers.shift);
                            nav_or_sel = true;
                        }
                        strata_platform::KeyCode::Home if !is_cmd => {
                            self.move_active_input_line_edge(false, modifiers.shift);
                            nav_or_sel = true;
                        }
                        strata_platform::KeyCode::End if !is_cmd => {
                            self.move_active_input_line_edge(true, modifiers.shift);
                            nav_or_sel = true;
                        }
                        _ => {}
                    }
                }

                if !text_changed && !nav_or_sel {
                    let active = self.active_focus.as_mut().unwrap();
                    match key {
                        strata_platform::KeyCode::Backspace => {
                            if is_cmd {
                                active.state.delete_word_backward();
                            } else {
                                active.state.delete_backward();
                            }
                            text_changed = true;
                        }
                        strata_platform::KeyCode::Delete => {
                            if is_cmd {
                                active.state.delete_word_forward();
                            } else {
                                active.state.delete_forward();
                            }
                            text_changed = true;
                        }
                        strata_platform::KeyCode::ArrowLeft => {
                            active.state.move_left(modifiers.shift, is_cmd);
                            nav_or_sel = true;
                        }
                        strata_platform::KeyCode::ArrowRight => {
                            active.state.move_right(modifiers.shift, is_cmd);
                            nav_or_sel = true;
                        }
                        strata_platform::KeyCode::Home => {
                            active.state.move_home(modifiers.shift);
                            nav_or_sel = true;
                        }
                        strata_platform::KeyCode::End => {
                            active.state.move_end(modifiers.shift);
                            nav_or_sel = true;
                        }
                        strata_platform::KeyCode::Enter => {
                            if active.is_multiline && !is_cmd {
                                active.state.insert_text("\n");
                                text_changed = true;
                            } else {
                                submit = true;
                            }
                        }
                        strata_platform::KeyCode::Escape => {
                            blur = true;
                        }
                        _ => {}
                    }
                }

                if text_changed || nav_or_sel {
                    self.caret_blink_epoch = self.now();
                }

                if text_changed {
                    let active = self.active_focus.as_ref().unwrap();
                    let val = active.state.text.get();
                    if let Some(ref on_input) = active.on_input {
                        on_input(strata_dom::Event::with_value(
                            strata_dom::EventType::Input,
                            val,
                        ));
                    }
                    self.sync_active_input_layout_and_scroll_caret();
                    return KeyDispatchResult::NeedsRebuild;
                }

                if nav_or_sel {
                    self.sync_active_input_layout_and_scroll_caret();
                    self.mark_paint_dirty();
                    return KeyDispatchResult::RedrawOnly;
                }

                if submit {
                    let active = self.active_focus.as_ref().unwrap();
                    let val = active.state.text.get();
                    if let Some(ref on_submit) = active.on_submit {
                        on_submit(strata_dom::Event::with_value(
                            strata_dom::EventType::Submit,
                            val.clone(),
                        ));
                    }
                    if let Some(ref on_change) = active.on_change {
                        on_change(strata_dom::Event::with_value(
                            strata_dom::EventType::Change,
                            val,
                        ));
                    }
                    return KeyDispatchResult::NeedsRebuild;
                }

                if blur {
                    let active = self.active_focus.take().unwrap();
                    if !active.is_multiline {
                        active.state.scroll_x.set(0.0);
                    }
                    let key = active
                        .input_id
                        .clone()
                        .unwrap_or_else(|| format!("input:{}", active.input_index));
                    self.input_states.insert(key.clone(), active.state.clone());
                    if let Some(prev_prop) = self.active_input_last_prop_value.take() {
                        self.input_last_prop_values.insert(key, prev_prop);
                    }
                    if let Some(ref on_blur) = active.on_blur {
                        on_blur(strata_dom::Event::new(strata_dom::EventType::Blur));
                    }
                    return KeyDispatchResult::NeedsRebuild;
                }

                if Self::dispatch_global_shortcut_for_key(*key, *modifiers) || dom_handled {
                    return KeyDispatchResult::NeedsRebuild;
                }

                KeyDispatchResult::Ignored
            }
            _ => KeyDispatchResult::Ignored,
        }
    }

    /// Dispatches keyboard events to the active focused input element.
    /// Returns true if the key event was consumed by the focused input.
    pub fn handle_key_event(&mut self, event: &PlatformEvent) -> bool {
        self.dispatch_key_event(event).is_handled()
    }

    /// Creates a fully populated synthetic W3C DOM Event for mouse and pointer interactions,
    /// with client, page, screen, and offset coordinates, button identifiers, click detail,
    /// and keyboard modifier flags.
    pub fn create_mouse_event(
        &self,
        event_type: strata_dom::EventType,
        px: f32,
        py: f32,
        button: i16,
        detail: i32,
        target_box: Option<&strata_layout::Rect>,
    ) -> strata_dom::Event {
        let mut ev = strata_dom::Event::new(event_type);
        ev.client_x = px as f64;
        ev.client_y = py as f64;
        ev.page_x = px as f64;
        ev.page_y = py as f64;
        ev.screen_x = px as f64;
        ev.screen_y = py as f64;
        ev.button = button;
        ev.buttons = match button {
            0 => 1,
            1 => 4,
            2 => 2,
            _ => 0,
        };
        ev.detail = detail;
        if let Some(tb) = target_box {
            ev.offset_x = (px - tb.x) as f64;
            ev.offset_y = (py - tb.y) as f64;
            ev.width = tb.width as f64;
            ev.height = tb.height as f64;
        } else {
            ev.offset_x = px as f64;
            ev.offset_y = py as f64;
        }
        #[cfg(target_os = "linux")]
        {
            let mods = strata_platform::gtk_current_modifiers();
            ev.shift_key = mods.shift;
            ev.ctrl_key = mods.ctrl;
            ev.alt_key = mods.alt;
            ev.meta_key = mods.meta;
        }
        ev
    }

    /// Returns `true` when the active pointer capture target or element at `(px, py)` belongs to
    /// a retained `<Canvas3D>` viewport tracked by this runtime.
    pub fn target_is_retained_canvas3d(&self, px: f32, py: f32) -> bool {
        if self.tracked_retained_3d.is_empty() {
            return false;
        }
        let path_is_canvas = |path: &[usize]| -> bool {
            let mut current = &self.root_blueprint;
            if let NodeBlueprint::Element { tag, .. } = current {
                if matches!(tag, Tag::Canvas | Tag::Custom("Canvas3D")) {
                    return true;
                }
            }
            for &idx in path {
                let mut flat = Vec::new();
                match current {
                    NodeBlueprint::Element { children, .. }
                    | NodeBlueprint::Fragment(children) => {
                        for child in children {
                            Self::flatten_blueprints(child, &mut flat);
                        }
                    }
                    _ => return false,
                }
                if idx >= flat.len() {
                    return false;
                }
                current = flat[idx];
                if let NodeBlueprint::Element { tag, .. } = current {
                    if matches!(tag, Tag::Canvas | Tag::Custom("Canvas3D")) {
                        return true;
                    }
                }
            }
            false
        };
        if let Some(ref cap) = self.pointer_capture_target {
            if let CaptureResolution::Resolved(cap_path) = self.resolve_pointer_capture(cap) {
                if path_is_canvas(&cap_path) {
                    return true;
                }
            }
        }
        Self::hit_test_node_path(&self.root_blueprint, &self.layout_root, px, py)
            .as_deref()
            .is_some_and(path_is_canvas)
    }

    /// Resolves the root-to-target `(NodeBlueprint, LayoutNode)` ancestor chain at `(px, py)`
    /// matching W3C DOM hit-testing: resolves the single front-most visual `pointer-events != none`
    /// target first, then truncates to the deepest ancestor along that path registering `event_type`
    /// (preserving target-relative `offset_x`/`offset_y` without ever piercing opaque higher-z overlays).
    pub(crate) fn hit_test_event_chain_at(
        &self,
        px: f32,
        py: f32,
        event_type: strata_dom::EventType,
    ) -> Option<Vec<(&NodeBlueprint, &LayoutNode)>> {
        let visual_path =
            Self::hit_test_node_path(&self.root_blueprint, &self.layout_root, px, py)?;
        let mut chain =
            Self::resolve_chain_for_path(&self.root_blueprint, &self.layout_root, &visual_path);
        let listener_idx = chain.iter().rposition(|(bp, _)| {
            matches!(
                bp,
                NodeBlueprint::Element { attributes, .. }
                    if attributes.iter().any(|a| matches!(
                        a,
                        strata_dom::Attribute::EventListener { event, .. } if *event == event_type
                    ))
            )
        })?;
        chain.truncate(listener_idx + 1);
        Some(chain)
    }

    /// Dispatches a bubbling DOM event wave across an already-resolved root-to-target chain.
    #[allow(clippy::too_many_arguments)]
    fn dispatch_bubbling_event_on_chain(
        &self,
        chain: Vec<(&NodeBlueprint, &LayoutNode)>,
        event_type: strata_dom::EventType,
        px: f32,
        py: f32,
        button: i16,
        detail: i32,
        configure: impl Fn(&mut strata_dom::Event),
    ) -> BubblingOutcome {
        let Some((_, target_layout)) = chain.last() else {
            return BubblingOutcome::default();
        };
        let chain_entities: Vec<strata_primitives::EntityId> = chain
            .iter()
            .filter_map(|(_, l)| l.entity_id.is_valid().then_some(l.entity_id))
            .collect();
        let mut ev = self.create_mouse_event(
            event_type,
            px,
            py,
            button,
            detail,
            Some(&target_layout.geometry.border_box),
        );
        let target_entity = if target_layout.entity_id.is_valid() {
            ev.target_id = u32::try_from(target_layout.entity_id.0).ok();
            Some(target_layout.entity_id)
        } else {
            None
        };
        configure(&mut ev);

        let mut handled = false;
        let mut default_prevented = false;
        let mut capture_requested = None;
        let mut capture_entity = target_entity;
        let mut invoked_entities = Vec::new();
        let stopped_cell = ev.propagation_stopped.clone();
        let prevented_cell = ev.default_prevented.clone();
        let capture_cell = ev.pointer_capture_requested.clone();
        let warp_cell = ev.cursor_warp_requested.clone();

        for &(bp, node_layout) in chain.iter().rev() {
            if let NodeBlueprint::Element { attributes, .. } = bp {
                let current_eid = if node_layout.entity_id.is_valid() {
                    u32::try_from(node_layout.entity_id.0).ok()
                } else {
                    None
                };
                let mut node_invoked = false;
                for attr in attributes {
                    if let strata_dom::Attribute::EventListener { event, handler } = attr {
                        if *event == event_type {
                            let prev_cap = capture_cell.get();
                            let call_ev = ev.clone();
                            call_ev.current_target_id.set(current_eid);
                            handler(call_ev);
                            handled = true;
                            node_invoked = true;
                            if prevented_cell.get() {
                                default_prevented = true;
                            }
                            if let Some(cap) = capture_cell.get() {
                                if capture_cell.get() != prev_cap || capture_requested.is_none() {
                                    capture_requested = Some(cap);
                                    if node_layout.entity_id.is_valid() {
                                        capture_entity = Some(node_layout.entity_id);
                                    }
                                }
                            }
                        }
                    }
                }
                if node_invoked && node_layout.entity_id.is_valid() {
                    invoked_entities.push(node_layout.entity_id);
                }
                if stopped_cell.get() {
                    break;
                }
            }
        }
        BubblingOutcome {
            handled,
            default_prevented,
            propagation_stopped: stopped_cell.get(),
            capture_requested,
            capture_entity,
            cursor_warp_requested: warp_cell.get(),
            target_entity,
            chain_entities,
            invoked_entities,
        }
    }

    /// Dispatches a bubbling DOM event starting at the deepest node along `path` registering `event_type`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn dispatch_bubbling_event_from_path(
        &self,
        event_type: strata_dom::EventType,
        path: &[usize],
        px: f32,
        py: f32,
        button: i16,
        detail: i32,
        configure: impl Fn(&mut strata_dom::Event),
    ) -> BubblingOutcome {
        let mut chain =
            Self::resolve_chain_for_path(&self.root_blueprint, &self.layout_root, path);
        let Some(listener_idx) = chain.iter().rposition(|(bp, _)| {
            matches!(
                bp,
                NodeBlueprint::Element { attributes, .. }
                    if attributes.iter().any(|a| matches!(
                        a,
                        strata_dom::Attribute::EventListener { event, .. } if *event == event_type
                    ))
            )
        }) else {
            return BubblingOutcome::default();
        };
        chain.truncate(listener_idx + 1);
        self.dispatch_bubbling_event_on_chain(
            chain, event_type, px, py, button, detail, configure,
        )
    }

    /// Detailed outcome from a W3C bubbling DOM dispatch wave, preserving side-channel flags
    /// (`pointer_capture_requested`, `cursor_warp_requested`, `stop_propagation`, `prevent_default`)
    /// and invoked `EntityId`s for pointer-capture deduplication.
    pub(crate) fn dispatch_bubbling_event_detailed_at(
        &self,
        event_type: strata_dom::EventType,
        px: f32,
        py: f32,
        button: i16,
        detail: i32,
        configure: impl Fn(&mut strata_dom::Event),
    ) -> BubblingOutcome {
        let Some(chain) = self.hit_test_event_chain_at(px, py, event_type) else {
            return BubblingOutcome::default();
        };
        self.dispatch_bubbling_event_on_chain(
            chain, event_type, px, py, button, detail, configure,
        )
    }

    /// Dispatches a synthetic mouse/wheel event for `event_type` at `(px, py)` starting from
    /// the front-most matching listener element and bubbling up through its ancestors, honoring
    /// `ev.stop_propagation()` and `ev.prevent_default()`.
    /// Returns `(any_listener_invoked, default_prevented)`.
    pub(crate) fn dispatch_bubbling_event_at(
        &self,
        event_type: strata_dom::EventType,
        px: f32,
        py: f32,
        button: i16,
        detail: i32,
        configure: impl Fn(&mut strata_dom::Event),
    ) -> (bool, bool) {
        let outcome =
            self.dispatch_bubbling_event_detailed_at(event_type, px, py, button, detail, configure);
        (outcome.handled, outcome.default_prevented)
    }

    /// Records the press target and coordinates on `MouseDown` so `onclick` / `onauxclick`
    /// fires on `MouseUp` when the pointer releases on the same target (or shared clickable
    /// ancestor) within drag tolerance.
    pub(crate) fn record_pending_click(&mut self, button: i16, px: f32, py: f32) {
        let down_path = Self::hit_test_node_path(&self.root_blueprint, &self.layout_root, px, py);
        let (clickable_entity, clickable_id, clickable_key) = down_path
            .as_ref()
            .and_then(|path| {
                let chain =
                    Self::resolve_chain_for_path(&self.root_blueprint, &self.layout_root, path);
                chain.iter().rev().find_map(|(bp, layout)| {
                    let has_click = matches!(
                        bp,
                        NodeBlueprint::Element { attributes, .. }
                            if attributes.iter().any(|a| matches!(
                                a,
                                strata_dom::Attribute::EventListener {
                                    event: strata_dom::EventType::Click
                                        | strata_dom::EventType::AuxClick,
                                    ..
                                }
                            ))
                    );
                    if has_click {
                        Some((
                            layout.entity_id.is_valid().then_some(layout.entity_id),
                            blueprint_nonempty_attr(bp, "id").map(str::to_string),
                            blueprint_explicit_key(bp),
                        ))
                    } else {
                        None
                    }
                })
            })
            .unwrap_or((None, None, None));
        self.pending_click = Some(PendingMouseClick {
            button,
            down_x: px,
            down_y: py,
            down_path,
            clickable_entity,
            clickable_id,
            clickable_key,
            dragged: false,
        });
    }

    /// Marks the active click candidate as dragged if pointer travel exceeds click drag tolerance.
    pub(crate) fn mark_pending_click_dragged_if_moved(&mut self, px: f32, py: f32) {
        if let Some(ref mut pending) = self.pending_click {
            if !pending.dragged
                && (px - pending.down_x).hypot(py - pending.down_y) > CLICK_DRAG_TOLERANCE_PX
            {
                pending.dragged = true;
            }
        }
    }

    /// Cancels any pending click candidate (e.g. when a drag starts or the cursor leaves the window).
    pub(crate) fn cancel_pending_click(&mut self) {
        self.pending_click = None;
    }

    /// Returns `true` when an active drag gesture (scrollbar drag, DOM drag-and-drop, or
    /// non-empty text selection drag) should suppress `onclick` on `MouseUp`.
    pub(crate) fn is_pointer_drag_suppressing_click(&self) -> bool {
        if self.scrollbar_drag.is_some() {
            return true;
        }
        if self.active_dom_drag.as_ref().is_some_and(|d| d.started) {
            return true;
        }
        if self.drag_selection_active
            && self.active_focus.as_ref().is_some_and(|a| {
                a.state
                    .selection_anchor
                    .get()
                    .is_some_and(|anc| anc != a.state.cursor_index.get())
            })
        {
            return true;
        }
        false
    }

    /// Completes a click on `MouseUp` if the matching `MouseDown` started and ended within
    /// drag tolerance on the same target or a shared clickable ancestor.
    pub(crate) fn dispatch_click_on_mouse_up(
        &mut self,
        button: i16,
        px: f32,
        py: f32,
        suppressed_by_drag: bool,
    ) -> bool {
        let Some(pending) = self.pending_click.take() else {
            return false;
        };
        if suppressed_by_drag
            || pending.button != button
            || pending.dragged
            || (px - pending.down_x).hypot(py - pending.down_y) > CLICK_DRAG_TOLERANCE_PX
        {
            return false;
        }
        let Some(down_path) = pending.down_path else {
            return false;
        };
        let Some(up_path) =
            Self::hit_test_node_path(&self.root_blueprint, &self.layout_root, px, py)
        else {
            return false;
        };

        // W3C UI Events: click dispatches on the lowest common inclusive ancestor of
        // the MouseDown and MouseUp targets (preventing click misfires when sliding from
        // one sibling button onto another, while allowing child-to-child wobble inside a button).
        let common_len = down_path
            .iter()
            .zip(up_path.iter())
            .take_while(|(a, b)| a == b)
            .count();
        let mut dispatch_path = down_path[..common_len].to_vec();

        // If an `onmousedown` rebuild shifted a sibling index before `MouseUp`, recover the
        // clickable ancestor via its surviving `EntityId`, explicit `id`, or `key` when it
        // still contains `up_path`.
        let recovered_path = pending
            .clickable_entity
            .and_then(|eid| self.layout_root.find_path_by_entity_id(eid))
            .or_else(|| {
                pending.clickable_id.as_deref().and_then(|id| {
                    self.collect_matching_layout_paths(|bp, _| {
                        blueprint_nonempty_attr(bp, "id") == Some(id)
                    })
                    .into_iter()
                    .find(|p| up_path.starts_with(p))
                })
            })
            .or_else(|| {
                pending
                    .clickable_key
                    .as_ref()
                    .and_then(|(key_attr, key_val)| {
                        self.collect_matching_layout_paths(|bp, _| {
                            blueprint_nonempty_attr(bp, key_attr) == Some(key_val.as_str())
                        })
                        .into_iter()
                        .find(|p| up_path.starts_with(p))
                    })
            });
        if let Some(ent_path) = recovered_path {
            if up_path.starts_with(&ent_path) && ent_path.len() > dispatch_path.len() {
                dispatch_path = ent_path;
            }
        }

        if button == 0 {
            self.dispatch_bubbling_event_from_path(
                strata_dom::EventType::Click,
                &dispatch_path,
                px,
                py,
                button,
                1,
                |_| {},
            )
            .handled
        } else {
            let aux_handled = self
                .dispatch_bubbling_event_from_path(
                    strata_dom::EventType::AuxClick,
                    &dispatch_path,
                    px,
                    py,
                    button,
                    1,
                    |_| {},
                )
                .handled;
            if aux_handled {
                return true;
            }
            self.dispatch_bubbling_event_from_path(
                strata_dom::EventType::Click,
                &dispatch_path,
                px,
                py,
                button,
                1,
                |_| {},
            )
            .handled
        }
    }

    /// Dispatches a pointer click event at (x, y) coordinates into the application blueprint tree.
    /// Returns true if an event listener was invoked.
    pub fn dispatch_click(&self, px: f32, py: f32) -> bool {
        self.dispatch_mouse_click(px, py, 0, 1)
    }

    /// Dispatches a mouse click event with specific button (0 = Left, 1 = Middle, 2 = Right) and detail,
    /// bubbling from the hit target up its ancestor chain and honoring `stop_propagation()`.
    pub fn dispatch_mouse_click(&self, px: f32, py: f32, button: i16, detail: i32) -> bool {
        if button == 0 {
            self.dispatch_bubbling_event_at(
                strata_dom::EventType::Click,
                px,
                py,
                button,
                detail,
                |_| {},
            )
            .0
        } else {
            let (aux_handled, _) = self.dispatch_bubbling_event_at(
                strata_dom::EventType::AuxClick,
                px,
                py,
                button,
                detail,
                |_| {},
            );
            if aux_handled {
                return true;
            }
            // Fallback to Click listener if AuxClick had no explicit listener on the ancestor chain
            self.dispatch_bubbling_event_at(
                strata_dom::EventType::Click,
                px,
                py,
                button,
                detail,
                |_| {},
            )
            .0
        }
    }

    /// Dispatches a standard W3C `dblclick` event at (x, y) coordinates, bubbling from the
    /// hit target up its ancestor chain and honoring `stop_propagation()`.
    pub fn dispatch_dblclick(&self, px: f32, py: f32) -> bool {
        self.dispatch_bubbling_event_at(strata_dom::EventType::DblClick, px, py, 0, 2, |_| {})
            .0
    }

    /// Dispatches a W3C `contextmenu` event at (x, y), bubbling from the hit target up its
    /// ancestor chain and honoring `stop_propagation()`. Returns false when nothing listens,
    /// leaving platform default behavior untouched.
    pub fn dispatch_context_menu(&self, px: f32, py: f32) -> bool {
        self.dispatch_bubbling_event_at(
            strata_dom::EventType::ContextMenu,
            px,
            py,
            2,
            1,
            |_| {},
        )
        .0
    }

    /// Records a possible window drag when a primary press lands on a `data-app-region="drag"`
    /// surface (the equivalent of `-webkit-app-region: drag`). Interactive children opt out with
    /// `data-app-region="no-drag"`. A quick second press toggles maximize, as native title bars do.
    pub fn begin_window_drag_candidate(&mut self, px: f32, py: f32) -> bool {
        let region = Self::hit_test_property(
            &self.root_blueprint,
            &self.layout_root,
            px,
            py,
            "data-app-region",
        );
        let is_drag = match region.as_deref() {
            Some("no-drag") => false,
            Some("drag") => true,
            _ => {
                self.csd_controller
                    .hit_test(self.viewport_width, self.viewport_height, px, py)
                    == strata_platform::CsdHitZone::Titlebar
            }
        };

        if !is_drag {
            self.pending_window_drag = None;
            return false;
        }
        let now = self.now();
        if let Some((t, lx, ly)) = self.last_drag_click.take() {
            if now.duration_since(t).as_millis() < 400
                && (px - lx).abs() < 5.0
                && (py - ly).abs() < 5.0
            {
                // Cancel pending drag candidate on double-click gesture so movement doesn't interfere
                self.pending_window_drag = None;
                return true;
            }
        }
        self.last_drag_click = Some((now, px, py));
        self.pending_window_drag = Some((px, py));
        false
    }

    /// Starts a native window move once the pointer travelled past the drag threshold.
    /// Returns true when the window's maximized state changed (such as unmaximizing on drag).
    pub fn poll_window_drag(&mut self, px: f32, py: f32) -> bool {
        if let Some((sx, sy)) = self.pending_window_drag {
            if (px - sx).abs() > 4.0 || (py - sy).abs() > 4.0 {
                self.pending_window_drag = None;
                self.last_drag_click = None;
                self.cancel_pending_click();
                #[cfg(target_os = "linux")]
                {
                    let was_max = strata_platform::gtk_is_maximized();
                    strata_platform::gtk_begin_window_drag(sx / self.viewport_width.max(1.0), sy);
                    let now_max = strata_platform::gtk_is_maximized();
                    return was_max != now_max;
                }
                #[cfg(not(target_os = "linux"))]
                {
                    return true;
                }
            }
        }
        false
    }

    /// Finds the deepest ancestor path at `(px, py)` that is draggable (`draggable="true"` or has an `ondragstart` listener).
    pub(crate) fn find_draggable_path_at(&self, px: f32, py: f32) -> Option<Vec<usize>> {
        let hit_path = Self::hit_test_node_path(&self.root_blueprint, &self.layout_root, px, py)?;
        for len in (0..=hit_path.len()).rev() {
            let prefix = &hit_path[..len];
            let Some(NodeBlueprint::Element { attributes, .. }) =
                Self::find_blueprint_at_path(&self.root_blueprint, prefix)
            else {
                continue;
            };
            let mut explicit_draggable: Option<bool> = None;
            let mut has_drag_listener = false;
            for attr in attributes {
                match attr {
                    strata_dom::Attribute::Property { key, value } if key == "draggable" => {
                        explicit_draggable = Some(!value.eq_ignore_ascii_case("false"));
                    }
                    strata_dom::Attribute::EventListener {
                        event: strata_dom::EventType::DragStart | strata_dom::EventType::Drag,
                        ..
                    } => {
                        has_drag_listener = true;
                    }
                    _ => {}
                }
            }
            if explicit_draggable == Some(true)
                || (explicit_draggable.is_none() && has_drag_listener)
            {
                return Some(prefix.to_vec());
            }
        }
        None
    }

    /// Resolves the drop-target element path at `(px, py)`, preferring the nearest ancestor registering
    /// a `DragEnter`, `DragOver`, `DragLeave`, or `Drop` listener so moving across child nodes inside a drop zone
    /// does not emit spurious `dragleave` / `dragenter` transitions.
    pub(crate) fn find_drop_target_path_at(&self, px: f32, py: f32) -> Option<Vec<usize>> {
        let hit_path = Self::hit_test_node_path(&self.root_blueprint, &self.layout_root, px, py)?;
        for len in (0..=hit_path.len()).rev() {
            let prefix = &hit_path[..len];
            let Some(NodeBlueprint::Element { attributes, .. }) =
                Self::find_blueprint_at_path(&self.root_blueprint, prefix)
            else {
                continue;
            };
            let is_drop_zone = attributes.iter().any(|attr| {
                matches!(
                    attr,
                    strata_dom::Attribute::EventListener {
                        event: strata_dom::EventType::DragEnter
                            | strata_dom::EventType::DragOver
                            | strata_dom::EventType::DragLeave
                            | strata_dom::EventType::Drop,
                        ..
                    }
                )
            });
            if is_drop_zone {
                return Some(prefix.to_vec());
            }
        }
        Some(hit_path)
    }

    /// Dispatches a drag-and-drop `EventType` (`DragStart`, `Drag`, `DragEnter`, `DragOver`, `DragLeave`, `Drop`, `DragEnd`)
    /// at or above `path`, returning `(listener_invoked, default_prevented)`.
    pub(crate) fn dispatch_drag_event_at_path(
        &self,
        path: &[usize],
        event_type: strata_dom::EventType,
        px: f32,
        py: f32,
        data_transfer: &std::rc::Rc<
            std::cell::RefCell<std::collections::HashMap<String, String>>,
        >,
    ) -> (bool, bool) {
        if let Some((handler, target_box, _)) = Self::find_event_at_or_above_path(
            &self.root_blueprint,
            &self.layout_root,
            path,
            event_type,
        ) {
            let target_box_copy = *target_box;
            let mut ev =
                self.create_mouse_event(event_type, px, py, 0, 1, Some(&target_box_copy));
            ev.data_transfer = std::rc::Rc::clone(data_transfer);
            ev.data = data_transfer
                .borrow()
                .get("text/plain")
                .cloned()
                .or_else(|| data_transfer.borrow().values().next().cloned());
            let prevented_cell = ev.default_prevented.clone();
            handler(ev);
            return (true, prevented_cell.get());
        }
        (false, false)
    }

    pub(crate) fn find_first_event_listener<'a>(
        blueprint: &'a NodeBlueprint,
        layout: &'a LayoutNode,
        event_type: strata_dom::EventType,
    ) -> Option<(
        std::rc::Rc<dyn Fn(strata_dom::Event)>,
        &'a strata_layout::Rect,
    )> {
        if let NodeBlueprint::Element { attributes, .. } = blueprint {
            for attr in attributes {
                if let strata_dom::Attribute::EventListener { event, handler } = attr {
                    if *event == event_type {
                        return Some((handler.clone(), &layout.geometry.border_box));
                    }
                }
            }
        }
        for (_idx, child_bp, child_layout) in Self::paired_blueprint_children(blueprint, layout) {
            if let Some(found) =
                Self::find_first_event_listener(child_bp, child_layout, event_type)
            {
                return Some(found);
            }
        }
        None
    }

    /// Dispatches an OS file drop (`PlatformEvent::FileDrop`) to the drop target under `last_mouse_pos`
    /// (or the primary `ondrop` listener in the tree), populating `event.files` and `event.data`.
    pub fn dispatch_file_drop(&mut self, paths: Vec<std::path::PathBuf>) -> bool {
        if paths.is_empty() {
            return false;
        }
        let (px, py) = self.last_mouse_pos;
        let hit = Self::hit_test_event_with_box(
            &self.root_blueprint,
            &self.layout_root,
            px,
            py,
            strata_dom::EventType::Drop,
        )
        .or_else(|| {
            Self::find_first_event_listener(
                &self.root_blueprint,
                &self.layout_root,
                strata_dom::EventType::Drop,
            )
        });

        if let Some((handler, target_box)) = hit {
            let target_box_copy = *target_box;
            let mut ev = self.create_mouse_event(
                strata_dom::EventType::Drop,
                px,
                py,
                0,
                1,
                Some(&target_box_copy),
            );
            if let Some(first) = paths.first() {
                let first_str = first.to_string_lossy().into_owned();
                ev.data = Some(first_str.clone());
                ev.set_data("text/uri-list", first_str.clone());
                ev.set_data("text/plain", first_str);
            }
            ev.files = paths;
            handler(ev);
            return true;
        }
        false
    }
}
