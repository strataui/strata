//! Hierarchical hover path tracking, boundary event dispatch, and restyling.

use super::*;

impl AppRuntime {
    /// Locates the blueprint and layout node at a hierarchical index path in the tree.
    pub fn find_node_by_path<'a>(
        blueprint: &'a NodeBlueprint,
        layout: &'a LayoutNode,
        path: &[usize],
    ) -> Option<(&'a NodeBlueprint, &'a LayoutNode)> {
        let mut cur_bp = blueprint;
        let mut cur_layout = layout;

        for &idx in path {
            match cur_bp {
                NodeBlueprint::Element { children, .. } => {
                    let mut flat = Vec::new();
                    for c in children {
                        Self::flatten_blueprints(c, &mut flat);
                    }
                    if idx >= flat.len() || idx >= cur_layout.children.len() {
                        return None;
                    }
                    cur_bp = flat[idx];
                    cur_layout = &cur_layout.children[idx];
                }
                NodeBlueprint::Fragment(children) => {
                    let mut flat = Vec::new();
                    for c in children {
                        Self::flatten_blueprints(c, &mut flat);
                    }
                    if idx >= flat.len() || idx >= cur_layout.children.len() {
                        return None;
                    }
                    cur_bp = flat[idx];
                    cur_layout = &cur_layout.children[idx];
                }
                _ => return None,
            }
        }

        Some((cur_bp, cur_layout))
    }

    /// Recursively hit tests a coordinate point against the blueprint and computed layout hierarchy,
    /// returning the hierarchical index path of the deepest matching element node.
    pub fn hit_test_node_path(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        px: f32,
        py: f32,
    ) -> Option<Vec<usize>> {
        let mut path = Vec::new();
        Self::hit_test_node_path_rec(blueprint, layout, px, py, &mut path)
    }

    fn collect_top_layer_hover_portals<'a>(
        bp: &'a NodeBlueprint,
        layout: &'a LayoutNode,
        current_path: &mut Vec<usize>,
        out: &mut Vec<(&'a NodeBlueprint, &'a LayoutNode, Vec<usize>)>,
    ) {
        if current_path.len() >= 256 {
            return;
        }
        for (idx, child_bp, child_layout) in Self::paired_blueprint_children(bp, layout) {
            if matches!(
                child_layout.style.display,
                strata_style::types::Display::None
            ) {
                continue;
            }
            current_path.push(idx);
            if child_layout.is_top_layer {
                out.push((child_bp, child_layout, current_path.clone()));
            }
            if !child_layout.children.is_empty()
                && ((child_layout.subtree_hit_flags & strata_layout::tree::SUBTREE_META_VALID)
                    == 0
                    || (child_layout.subtree_hit_flags & strata_layout::tree::SUBTREE_HAS_TOP_LAYER)
                        != 0)
            {
                Self::collect_top_layer_hover_portals(child_bp, child_layout, current_path, out);
            }
            current_path.pop();
        }
    }

    pub(crate) fn hit_test_node_path_rec(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        px: f32,
        py: f32,
        current_path: &mut Vec<usize>,
    ) -> Option<Vec<usize>> {
        if current_path.is_empty()
            && ((layout.subtree_hit_flags & strata_layout::tree::SUBTREE_META_VALID) == 0
                || (layout.subtree_hit_flags & strata_layout::tree::SUBTREE_HAS_TOP_LAYER) != 0)
        {
            let mut portals = Vec::new();
            let mut scan_path = Vec::new();
            Self::collect_top_layer_hover_portals(blueprint, layout, &mut scan_path, &mut portals);
            for (p_bp, p_layout, mut p_path) in portals.into_iter().rev() {
                if let Some(hit) =
                    Self::hit_test_node_path_sc(p_bp, p_layout, px, py, &[], &mut p_path)
                {
                    return Some(hit);
                }
            }
        }
        Self::hit_test_node_path_sc(blueprint, layout, px, py, &[], current_path)
    }

    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    fn collect_hoisted_hover_participants<'a>(
        bp: &'a NodeBlueprint,
        layout: &'a LayoutNode,
        steps: &[super::hit_test::HitClipStep],
        current_path: &mut Vec<usize>,
        negative_z: &mut Vec<(
            &'a NodeBlueprint,
            &'a LayoutNode,
            Vec<usize>,
            Vec<super::hit_test::HitClipStep>,
        )>,
        auto_or_zero_z: &mut Vec<(
            &'a NodeBlueprint,
            &'a LayoutNode,
            Vec<usize>,
            bool,
            Vec<super::hit_test::HitClipStep>,
        )>,
        positive_z: &mut Vec<(
            &'a NodeBlueprint,
            &'a LayoutNode,
            Vec<usize>,
            Vec<super::hit_test::HitClipStep>,
        )>,
    ) {
        if current_path.len() >= 256 {
            return;
        }
        let parent_display = Some(layout.style.display);
        for (idx, child_bp, child_layout) in Self::paired_blueprint_children(bp, layout) {
            if child_layout.is_top_layer
                || matches!(
                    child_layout.style.display,
                    strata_style::types::Display::None
                )
                || matches!(
                    child_layout.style.visibility,
                    strata_style::types::Visibility::Hidden
                )
            {
                continue;
            }
            current_path.push(idx);
            if Self::establishes_stacking_context_in(child_layout, parent_display, false) {
                let eff_steps = Self::effective_hit_clip_steps(child_layout, steps).to_vec();
                let z = child_layout.style.z_index.unwrap_or(0);
                if z < 0 {
                    negative_z.push((child_bp, child_layout, current_path.clone(), eff_steps));
                } else if z > 0 {
                    positive_z.push((child_bp, child_layout, current_path.clone(), eff_steps));
                } else {
                    auto_or_zero_z.push((
                        child_bp,
                        child_layout,
                        current_path.clone(),
                        true,
                        eff_steps,
                    ));
                }
            } else {
                let eff_for_child = Self::effective_hit_clip_steps(child_layout, steps);
                if child_layout.style.position != strata_style::types::Position::Static {
                    auto_or_zero_z.push((
                        child_bp,
                        child_layout,
                        current_path.clone(),
                        false,
                        eff_for_child.to_vec(),
                    ));
                }
                let skip_recurse = child_layout.children.is_empty()
                    || ((child_layout.subtree_hit_flags & strata_layout::tree::SUBTREE_META_VALID)
                        != 0
                        && (child_layout.subtree_hit_flags
                            & strata_layout::tree::SUBTREE_HAS_HOISTED)
                            == 0);
                if !skip_recurse {
                    let step = Self::hit_clip_step_for(child_layout);
                    if step.clip.is_some() || step.is_abs_cb || step.is_fixed_cb {
                        let mut child_steps = Vec::with_capacity(eff_for_child.len() + 1);
                        child_steps.extend_from_slice(eff_for_child);
                        child_steps.push(step);
                        Self::collect_hoisted_hover_participants(
                            child_bp,
                            child_layout,
                            &child_steps,
                            current_path,
                            negative_z,
                            auto_or_zero_z,
                            positive_z,
                        );
                    } else {
                        Self::collect_hoisted_hover_participants(
                            child_bp,
                            child_layout,
                            eff_for_child,
                            current_path,
                            negative_z,
                            auto_or_zero_z,
                            positive_z,
                        );
                    }
                }
            }
            current_path.pop();
        }
    }

    fn hit_test_node_path_in_flow(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        px: f32,
        py: f32,
        inherited_steps: &[super::hit_test::HitClipStep],
        current_path: &mut Vec<usize>,
    ) -> Option<Vec<usize>> {
        if current_path.len() >= 256
            || matches!(layout.style.display, strata_style::types::Display::None)
            || matches!(
                layout.style.visibility,
                strata_style::types::Visibility::Hidden
            )
        {
            return None;
        }

        let eff_inherited = Self::effective_hit_clip_steps(layout, inherited_steps);
        if !Self::hit_clip_steps_allow(eff_inherited, px, py) {
            return None;
        }
        if (layout.style.overflow_x != strata_style::types::Overflow::Visible
            || layout.style.overflow_y != strata_style::types::Overflow::Visible)
            && !layout.geometry.border_box.contains_point(px, py)
        {
            return None;
        }
        if (layout.subtree_hit_flags
            & (strata_layout::tree::SUBTREE_META_VALID
                | strata_layout::tree::SUBTREE_HAS_TRANSFORM))
            == strata_layout::tree::SUBTREE_META_VALID
            && !layout.subtree_bounds.contains_point(px, py)
        {
            return None;
        }

        let parent_display = Some(layout.style.display);
        let paired = Self::paired_blueprint_children(blueprint, layout);
        for &(idx, child_bp, child_layout) in paired.iter().rev() {
            if child_layout.is_top_layer
                || matches!(
                    child_layout.style.display,
                    strata_style::types::Display::None
                )
                || Self::establishes_stacking_context_in(child_layout, parent_display, false)
                || child_layout.style.position != strata_style::types::Position::Static
            {
                continue;
            }
            if (child_layout.subtree_hit_flags
                & (strata_layout::tree::SUBTREE_META_VALID
                    | strata_layout::tree::SUBTREE_HAS_TRANSFORM))
                == strata_layout::tree::SUBTREE_META_VALID
                && !child_layout.subtree_bounds.contains_point(px, py)
            {
                continue;
            }
            current_path.push(idx);
            if let Some(res) =
                Self::hit_test_node_path_in_flow(child_bp, child_layout, px, py, &[], current_path)
            {
                return Some(res);
            }
            current_path.pop();
        }

        if !matches!(
            layout.style.pointer_events,
            strata_style::types::PointerEvents::None
        ) {
            match blueprint {
                NodeBlueprint::Element { .. }
                | NodeBlueprint::Text(_)
                | NodeBlueprint::DynamicText(_) => {
                    if layout.geometry.border_box.contains_point(px, py) {
                        return Some(current_path.clone());
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn hit_test_node_path_sc(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        px: f32,
        py: f32,
        inherited_steps: &[super::hit_test::HitClipStep],
        current_path: &mut Vec<usize>,
    ) -> Option<Vec<usize>> {
        if current_path.len() >= 256
            || matches!(layout.style.display, strata_style::types::Display::None)
            || matches!(
                layout.style.visibility,
                strata_style::types::Visibility::Hidden
            )
        {
            return None;
        }

        let eff_inherited = Self::effective_hit_clip_steps(layout, inherited_steps);
        let inherited_allows = Self::hit_clip_steps_allow(eff_inherited, px, py);

        let (px, py) = Self::unproject_point(layout, px, py);

        let own_clips = layout.style.overflow_x != strata_style::types::Overflow::Visible
            || layout.style.overflow_y != strata_style::types::Overflow::Visible;
        let own_allows = !own_clips || layout.geometry.border_box.contains_point(px, py);

        let need_hoisted = (layout.subtree_hit_flags & strata_layout::tree::SUBTREE_META_VALID) == 0
            || layout.children.iter().any(|c| {
                (c.subtree_hit_flags & strata_layout::tree::SUBTREE_HAS_HOISTED) != 0
            });
        let mut negative_z = Vec::new();
        if need_hoisted {
            let mut base_steps = Vec::with_capacity(eff_inherited.len() + 1);
            base_steps.extend_from_slice(eff_inherited);
            let own_step = Self::hit_clip_step_for(layout);
            if own_step.clip.is_some() || own_step.is_abs_cb || own_step.is_fixed_cb {
                base_steps.push(own_step);
            }

            let mut auto_or_zero_z = Vec::new();
            let mut positive_z = Vec::new();
            Self::collect_hoisted_hover_participants(
                blueprint,
                layout,
                &base_steps,
                current_path,
                &mut negative_z,
                &mut auto_or_zero_z,
                &mut positive_z,
            );

            negative_z.sort_by_key(|(_, l, _, _)| l.style.z_index.unwrap_or(0));
            positive_z.sort_by_key(|(_, l, _, _)| l.style.z_index.unwrap_or(0));

            // Layer 7: positive z-index child stacking contexts
            for (c_bp, c_layout, c_path, c_steps) in positive_z.into_iter().rev() {
                let mut path_buf = c_path;
                if let Some(res) =
                    Self::hit_test_node_path_sc(c_bp, c_layout, px, py, &c_steps, &mut path_buf)
                {
                    return Some(res);
                }
            }

            // Layer 6: positioned auto/0 z-index descendants
            for (c_bp, c_layout, c_path, is_sc, c_steps) in auto_or_zero_z.into_iter().rev() {
                let mut path_buf = c_path;
                let hit = if is_sc {
                    Self::hit_test_node_path_sc(c_bp, c_layout, px, py, &c_steps, &mut path_buf)
                } else {
                    Self::hit_test_node_path_in_flow(c_bp, c_layout, px, py, &c_steps, &mut path_buf)
                };
                if hit.is_some() {
                    return hit;
                }
            }
        }

        // Layers 5, 4, 3: direct static non-stacking-context children
        if inherited_allows && own_allows {
            let parent_display = Some(layout.style.display);
            let paired = Self::paired_blueprint_children(blueprint, layout);
            for &(idx, child_bp, child_layout) in paired.iter().rev() {
                if child_layout.is_top_layer
                    || matches!(
                        child_layout.style.display,
                        strata_style::types::Display::None
                    )
                    || Self::establishes_stacking_context_in(child_layout, parent_display, false)
                    || child_layout.style.position != strata_style::types::Position::Static
                    || ((child_layout.subtree_hit_flags
                        & (strata_layout::tree::SUBTREE_META_VALID
                            | strata_layout::tree::SUBTREE_HAS_TRANSFORM))
                        == strata_layout::tree::SUBTREE_META_VALID
                        && !child_layout.subtree_bounds.contains_point(px, py))
                {
                    continue;
                }
                current_path.push(idx);
                if let Some(res) = Self::hit_test_node_path_in_flow(
                    child_bp,
                    child_layout,
                    px,
                    py,
                    &[],
                    current_path,
                ) {
                    return Some(res);
                }
                current_path.pop();
            }
        }

        // Layer 2: negative z-index child stacking contexts
        for (c_bp, c_layout, c_path, c_steps) in negative_z.into_iter().rev() {
            let mut path_buf = c_path;
            if let Some(res) =
                Self::hit_test_node_path_sc(c_bp, c_layout, px, py, &c_steps, &mut path_buf)
            {
                return Some(res);
            }
        }

        // Layer 1: stacking context root itself
        if inherited_allows
            && !matches!(
                layout.style.pointer_events,
                strata_style::types::PointerEvents::None
            )
        {
            match blueprint {
                NodeBlueprint::Element { .. }
                | NodeBlueprint::Text(_)
                | NodeBlueprint::DynamicText(_) => {
                    if layout.geometry.border_box.contains_point(px, py) {
                        Some(current_path.clone())
                    } else {
                        None
                    }
                }
                _ => None,
            }
        } else {
            None
        }
    }

    /// Dispatches W3C boundary events (`mouseout`, `mouseleave`, `mouseover`, `mouseenter`)
    /// between the previous hover path and the newly active hover path.
    pub fn dispatch_boundary_events(
        &self,
        old_path: Option<&[usize]>,
        new_path: Option<&[usize]>,
        px: f32,
        py: f32,
    ) {
        let old_slice = old_path.unwrap_or(&[]);
        let new_slice = new_path.unwrap_or(&[]);

        let mut common_len = 0;
        if old_path.is_some() && new_path.is_some() {
            while common_len < old_slice.len()
                && common_len < new_slice.len()
                && old_slice[common_len] == new_slice[common_len]
            {
                common_len += 1;
            }
        }

        // Dispatches exit events for nodes no longer hovered (from deepest child up)
        if old_path.is_some() {
            let min_len = if new_path.is_some() {
                common_len + 1
            } else {
                0
            };
            for len in (min_len..=old_slice.len()).rev() {
                let subpath = &old_slice[..len];
                if let Some((bp, layout)) =
                    Self::find_node_by_path(&self.root_blueprint, &self.layout_root, subpath)
                {
                    self.dispatch_node_events(
                        bp,
                        layout,
                        &[
                            strata_dom::EventType::MouseOut,
                            strata_dom::EventType::MouseLeave,
                        ],
                        px,
                        py,
                    );
                }
            }
        }

        // Dispatches enter events for newly hovered nodes (from shallowest ancestor down)
        if new_path.is_some() {
            let start_len = if old_path.is_some() {
                common_len + 1
            } else {
                0
            };
            for len in start_len..=new_slice.len() {
                let subpath = &new_slice[..len];
                if let Some((bp, layout)) =
                    Self::find_node_by_path(&self.root_blueprint, &self.layout_root, subpath)
                {
                    self.dispatch_node_events(
                        bp,
                        layout,
                        &[
                            strata_dom::EventType::MouseOver,
                            strata_dom::EventType::MouseEnter,
                        ],
                        px,
                        py,
                    );
                }
            }
        }
    }

    /// Helper to invoke registered event listeners of specified types on a single blueprint node.
    pub(crate) fn dispatch_node_events(
        &self,
        bp: &NodeBlueprint,
        layout: &LayoutNode,
        event_types: &[strata_dom::EventType],
        px: f32,
        py: f32,
    ) {
        if let NodeBlueprint::Element { attributes, .. } = bp {
            for attr in attributes {
                if let strata_dom::Attribute::EventListener { event, handler } = attr {
                    if event_types.contains(event) {
                        let ev = self.create_mouse_event(
                            *event,
                            px,
                            py,
                            0,
                            0,
                            Some(&layout.geometry.border_box),
                        );
                        handler(ev);
                    }
                }
            }
        }
    }

    /// Checks whether any element along the index path has active hover styles (Tailwind or `<style>`) or boundary handlers.
    pub fn node_path_has_hover_effects(blueprint: &NodeBlueprint, path: &[usize]) -> bool {
        let stylesheets = Self::collect_stylesheets_from_blueprint(blueprint);
        Self::node_path_has_hover_effects_with_sheets(blueprint, path, &stylesheets)
    }

    pub(crate) fn node_path_has_hover_effects_with_sheets(
        blueprint: &NodeBlueprint,
        path: &[usize],
        stylesheets: &[std::sync::Arc<strata_style::StyleSheet>],
    ) -> bool {
        let mut path_nodes: Vec<&NodeBlueprint> = Vec::with_capacity(path.len() + 1);
        let mut cur_bp = blueprint;
        if Self::node_has_hover_effects(cur_bp) {
            return true;
        }
        path_nodes.push(cur_bp);

        for &idx in path {
            match cur_bp {
                NodeBlueprint::Element { children, .. } => {
                    let mut flat = Vec::new();
                    for c in children {
                        Self::flatten_blueprints(c, &mut flat);
                    }
                    if idx >= flat.len() {
                        break;
                    }
                    cur_bp = flat[idx];
                }
                NodeBlueprint::Fragment(children) => {
                    let mut flat = Vec::new();
                    for c in children {
                        Self::flatten_blueprints(c, &mut flat);
                    }
                    if idx >= flat.len() {
                        break;
                    }
                    cur_bp = flat[idx];
                }
                _ => break,
            }
            if Self::node_has_hover_effects(cur_bp) {
                return true;
            }
            path_nodes.push(cur_bp);
        }

        if stylesheets.is_empty() {
            return false;
        }

        Self::path_nodes_match_stylesheet_hover(&path_nodes, stylesheets)
    }

    /// Returns true if any element in `path_nodes` matches a `:hover`-bearing compound selector in `stylesheets`.
    pub(crate) fn path_nodes_match_stylesheet_hover(
        path_nodes: &[&NodeBlueprint],
        stylesheets: &[std::sync::Arc<strata_style::StyleSheet>],
    ) -> bool {
        let compound_has_hover = |comp: &strata_style::selector::CompoundSelector| {
            comp.parts.iter().any(|p| match p {
                strata_style::selector::SelectorPart::PseudoClass(name) => {
                    let n = name.trim().to_ascii_lowercase();
                    n == "hover" || n.contains(":hover")
                }
                _ => false,
            })
        };

        let probe_state = strata_style::selector::PseudoState {
            is_hovered: true,
            ..Default::default()
        };
        let probe_state_dark = strata_style::selector::PseudoState {
            is_hovered: true,
            is_dark: true,
            ..Default::default()
        };

        for bp in path_nodes {
            if let NodeBlueprint::Element {
                tag,
                classes,
                attributes,
                ..
            } = bp
            {
                if Self::is_style_tag(tag) {
                    continue;
                }
                let tag_str = Self::tag_as_str(tag);
                let (class_vec, id_val, attr_vec) =
                    Self::extract_element_selector_meta(classes, attributes);
                for sheet in stylesheets {
                    for rule in &sheet.rules {
                        for sel in &rule.selectors {
                            if !sel.has_any_hover_pseudo() {
                                continue;
                            }
                            if compound_has_hover(&sel.target)
                                && (sel.target.matches_with_context(
                                    tag_str,
                                    &class_vec,
                                    id_val.as_deref(),
                                    &attr_vec,
                                    &probe_state,
                                    &[],
                                    Some(true),
                                    Some(true),
                                    &[],
                                ) || sel.target.matches_with_context(
                                    tag_str,
                                    &class_vec,
                                    id_val.as_deref(),
                                    &attr_vec,
                                    &probe_state_dark,
                                    &[],
                                    Some(true),
                                    Some(true),
                                    &[],
                                ))
                            {
                                return true;
                            }
                            for (_, step_comp) in &sel.steps {
                                if compound_has_hover(step_comp)
                                    && (step_comp.matches_with_context(
                                        tag_str,
                                        &class_vec,
                                        id_val.as_deref(),
                                        &attr_vec,
                                        &probe_state,
                                        &[],
                                        Some(true),
                                        Some(true),
                                        &[],
                                    ) || step_comp.matches_with_context(
                                        tag_str,
                                        &class_vec,
                                        id_val.as_deref(),
                                        &attr_vec,
                                        &probe_state_dark,
                                        &[],
                                        Some(true),
                                        Some(true),
                                        &[],
                                    ))
                                {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// Returns true when `node` carries the Tailwind `"group"` class and any descendant uses `group-hover:`.
    pub(crate) fn subtree_has_group_hover_effects(node: &NodeBlueprint) -> bool {
        fn check(bp: &NodeBlueprint) -> bool {
            match bp {
                NodeBlueprint::Element {
                    classes, children, ..
                } => {
                    let has_gh = classes.iter().any(|c| {
                        strata_style::tailwind::tokenize_classes(c)
                            .into_iter()
                            .any(|tok| {
                                let mut cur = tok;
                                while let (Some(m), rest) =
                                    strata_style::tailwind::split_modifier(cur)
                                {
                                    if m == "group-hover" {
                                        return true;
                                    }
                                    cur = rest;
                                }
                                false
                            })
                    });
                    has_gh || children.iter().any(check)
                }
                NodeBlueprint::Fragment(children) => children.iter().any(check),
                _ => false,
            }
        }
        match node {
            NodeBlueprint::Element {
                classes, children, ..
            } if Self::classes_have_group_marker(classes) => children.iter().any(check),
            _ => false,
        }
    }

    /// Checks if a single blueprint node has hover utility classes (including chained modifiers like `dark:hover:*`),
    /// `.group` descendants with `group-hover:*`, or boundary event listeners.
    pub fn node_has_hover_effects(node: &NodeBlueprint) -> bool {
        match node {
            NodeBlueprint::Element {
                classes,
                attributes,
                ..
            } => {
                let has_hover_class = classes.iter().any(|c| {
                    strata_style::tailwind::tokenize_classes(c)
                        .into_iter()
                        .any(|tok| {
                            let mut cur = tok;
                            while let (Some(m), rest) = strata_style::tailwind::split_modifier(cur)
                            {
                                if m == "hover" {
                                    return true;
                                }
                                cur = rest;
                            }
                            false
                        })
                });
                if has_hover_class {
                    return true;
                }
                if Self::subtree_has_group_hover_effects(node) {
                    return true;
                }
                attributes.iter().any(|attr| match attr {
                    strata_dom::Attribute::EventListener { event, .. } => matches!(
                        event,
                        strata_dom::EventType::MouseEnter
                            | strata_dom::EventType::MouseLeave
                            | strata_dom::EventType::MouseOver
                            | strata_dom::EventType::MouseOut
                    ),
                    _ => false,
                })
            }
            _ => false,
        }
    }

    /// Continuously updates hover state from pointer coordinates.
    /// Dispatches boundary events and rebuilds layout tree if the hovered element changed and has hover effects.
    /// Returns true if the hover target changed and an interactive frame rebuild is required.
    pub fn update_hover(&mut self, px: f32, py: f32) -> bool {
        self.last_mouse_pos = (px, py);
        let new_path = Self::hit_test_node_path(&self.root_blueprint, &self.layout_root, px, py);
        if new_path == self.current_hover_path {
            if self.hover_tooltip_since.is_some() && !self.hover_tooltip_visible {
                self.hover_tooltip_anchor = (px, py);
            }
            return false;
        }

        let old_path = self.current_hover_path.take();
        self.current_hover_path = new_path.clone();

        self.dispatch_boundary_events(old_path.as_deref(), new_path.as_deref(), px, py);
        let tooltip_changed = self.sync_hover_tooltip(px, py);

        let old_has_effects = old_path.as_deref().is_some_and(|p| {
            Self::node_path_has_hover_effects_with_index(
                &self.root_blueprint,
                p,
                &self.cached_stylesheets,
                &self.stylesheet_index,
            )
        });
        let new_has_effects = new_path.as_deref().is_some_and(|p| {
            Self::node_path_has_hover_effects_with_index(
                &self.root_blueprint,
                p,
                &self.cached_stylesheets,
                &self.stylesheet_index,
            )
        });

        if old_has_effects || new_has_effects {
            let has_listeners = old_path
                .as_deref()
                .is_some_and(|p| Self::path_has_hover_listeners(&self.root_blueprint, p))
                || new_path
                    .as_deref()
                    .is_some_and(|p| Self::path_has_hover_listeners(&self.root_blueprint, p));
            let layout_required = old_path.as_deref().is_some_and(|p| {
                Self::path_hover_affects_layout_with_sheets(
                    &self.root_blueprint,
                    p,
                    &self.cached_stylesheets,
                    &self.stylesheet_index,
                )
            }) || new_path.as_deref().is_some_and(|p| {
                Self::path_hover_affects_layout_with_sheets(
                    &self.root_blueprint,
                    p,
                    &self.cached_stylesheets,
                    &self.stylesheet_index,
                )
            });
            if layout_required || has_listeners {
                self.hover_paint_only = false;
                self.last_hover_fast_path_used = false;
                let reason = if layout_required {
                    "HoverAffectsLayout"
                } else {
                    "HoverListenersPresent"
                };
                self.last_hover_fast_path_bailout = Some(reason.to_string());
                self.last_rebuild_reason = RebuildReason::HoverFastPathFailed(reason.to_string());
                self.rebuild_layout_tree();
            } else {
                self.hover_paint_only = true;
                let hover_start = std::time::Instant::now();
                let fast_ok = self.restyle_hover_subtrees_fast(
                    if old_has_effects {
                        old_path.as_deref()
                    } else {
                        None
                    },
                    if new_has_effects {
                        new_path.as_deref()
                    } else {
                        None
                    },
                );
                self.last_hover_fast_path_used = fast_ok;
                if fast_ok {
                    self.last_hover_fast_path_bailout = None;
                    self.last_rebuild_reason = RebuildReason::HoverFastPath;
                    self.frame_profile.style_cascade += hover_start.elapsed();
                } else {
                    if let Some(ref reason) = self.last_hover_fast_path_bailout {
                        self.last_rebuild_reason =
                            RebuildReason::HoverFastPathFailed(reason.clone());
                    }
                    self.restyle_for_hover_paint();
                    self.frame_profile.style_cascade += hover_start.elapsed();
                }
                if perf_logging_enabled() {
                    perf_log_ms(
                        "hover_repaint",
                        hover_start,
                        if fast_ok {
                            "paint-only hover (fast)"
                        } else {
                            "paint-only hover (full)"
                        },
                    );
                }
            }
            true
        } else if tooltip_changed {
            self.hover_paint_only = true;
            self.compile_display_list();
            true
        } else {
            self.hover_paint_only = false;
            false
        }
    }

    /// Re-applies hover/active styles, synchronizes CSS transitions, and recompiles paint commands without re-running layout when paint-only.
    ///
    /// Paint-only hover variants (`hover:bg-*`, `hover:text-*`, `hover:underline`,
    /// shadows, opacity, `<style>` `:hover` color/background rules) do not change box geometry.
    /// Rebuilding the tree carries the new styles; the previous geometry, line boxes, and inline item streams
    /// are transplanted so the display list can be recompiled immediately.
    pub fn restyle_for_hover_paint(&mut self) {
        self.capture_previous_node_sizes();
        let ctx = self.interaction_context();
        let new_tree = Self::build_layout_tree_with_interaction(&self.root_blueprint, &ctx);
        let mut previous = std::mem::replace(&mut self.layout_root, new_tree);
        let transplanted = Self::transplant_layout_state(&mut previous, &mut self.layout_root);
        let now_ms = self.runtime_start_epoch.elapsed().as_secs_f64() * 1000.0;
        self.sync_tree_animations(now_ms);
        let mut layout_node_ids = self.transition_manager.layout_affecting_node_ids();
        layout_node_ids.extend(self.animation_manager.layout_affecting_node_ids());
        let has_layout_transition = self.transition_manager.has_layout_affecting_transitions()
            || self.animation_manager.has_layout_affecting_animations();
        if transplanted && !has_layout_transition {
            strata_layout::box_model::refresh_inline_fragment_styles(&mut self.layout_root);
        } else {
            self.applied_scroll_offsets.clear();
            self.applied_portal_offsets.clear();
            if has_layout_transition {
                self.invalidate_layout_for_node_ids(&layout_node_ids);
            }
            let localized_layout = if let Some(ref path) = self.current_hover_path {
                self.layout_root.compute_subgrid_or_boundary_layout(path)
            } else {
                false
            };
            if !localized_layout {
                self.layout_root
                    .compute_layout(self.viewport_width, self.viewport_height);
            }
            Self::sync_layout_to_ir(&self.layout_root, &mut self.ir_graph);
            self.layout_computations += 1;
            self.apply_viewport_clamps();
            self.apply_scroll_offsets();
        }
        self.sync_active_input_layout();
        self.compile_display_list();
    }

    /// Returns true if a single blueprint element matches any `:hover` rule in `stylesheets`
    /// (fast-rejected in O(1) via `index`'s Bloom filter).
    pub(crate) fn single_node_matches_stylesheet_hover(
        bp: &NodeBlueprint,
        stylesheets: &[std::sync::Arc<strata_style::StyleSheet>],
        index: &StylesheetSelectorIndex,
    ) -> bool {
        if stylesheets.is_empty() || !index.has_any_hover_rule {
            return false;
        }
        let NodeBlueprint::Element {
            tag,
            classes,
            attributes,
            ..
        } = bp
        else {
            return false;
        };
        if Self::is_style_tag(tag) {
            return false;
        }
        let tag_str = Self::tag_as_str(tag);
        let (class_vec, id_val, _) = Self::extract_element_selector_meta(classes, attributes);
        if !index.node_may_match_hover(tag_str, id_val.as_deref(), &class_vec) {
            return false;
        }
        Self::path_nodes_match_stylesheet_hover(&[bp], stylesheets)
    }

    /// Bloom-filter-accelerated variant of [`Self::node_path_has_hover_effects_with_sheets`].
    pub(crate) fn node_path_has_hover_effects_with_index(
        blueprint: &NodeBlueprint,
        path: &[usize],
        stylesheets: &[std::sync::Arc<strata_style::StyleSheet>],
        index: &StylesheetSelectorIndex,
    ) -> bool {
        let mut path_nodes: Vec<&NodeBlueprint> = Vec::with_capacity(path.len() + 1);
        let mut cur_bp = blueprint;
        if Self::node_has_hover_effects(cur_bp) {
            return true;
        }
        path_nodes.push(cur_bp);

        for &idx in path {
            match cur_bp {
                NodeBlueprint::Element { children, .. } | NodeBlueprint::Fragment(children) => {
                    let mut flat = Vec::new();
                    for c in children {
                        Self::flatten_blueprints(c, &mut flat);
                    }
                    if idx >= flat.len() {
                        break;
                    }
                    cur_bp = flat[idx];
                }
                _ => break,
            }
            if Self::node_has_hover_effects(cur_bp) {
                return true;
            }
            path_nodes.push(cur_bp);
        }

        if stylesheets.is_empty() || !index.has_any_hover_rule {
            return false;
        }

        for bp in &path_nodes {
            if Self::single_node_matches_stylesheet_hover(bp, stylesheets, index) {
                return true;
            }
        }
        false
    }

    /// Fast paint-only hover restyle that updates only the diverging subtrees along
    /// `old_path` and `new_path`, supporting static `<style>` sheets (`:root` variables,
    /// `.class:hover`, `.parent:hover .child`), `group-hover:`, and unrelated active transitions.
    pub(crate) fn restyle_hover_subtrees_fast(
        &mut self,
        old_path: Option<&[usize]>,
        new_path: Option<&[usize]>,
    ) -> bool {
        if Self::node_has_hover_effects(&self.root_blueprint)
            || Self::single_node_matches_stylesheet_hover(
                &self.root_blueprint,
                &self.cached_stylesheets,
                &self.stylesheet_index,
            )
        {
            self.last_hover_fast_path_bailout = Some("RootHasHoverEffects".to_string());
            return false;
        }
        if self.stylesheet_index.has_sibling_hover_rule {
            self.last_hover_fast_path_bailout =
                Some("StylesheetSiblingHoverCombinator".to_string());
            return false;
        }

        fn subtree_has_animations(node: &LayoutNode) -> bool {
            if !node.style.animations.is_empty() {
                return true;
            }
            node.children.iter().any(subtree_has_animations)
        }

        fn subtree_has_transitions(node: &LayoutNode) -> bool {
            if !node.style.transitions.is_empty() || node.active_transitions {
                return true;
            }
            node.children.iter().any(subtree_has_transitions)
        }

        #[allow(clippy::too_many_arguments)]
        fn restyle_one_path(
            bp_root: &NodeBlueprint,
            layout_root: &mut LayoutNode,
            target_path: &[usize],
            ctx: &InteractionContext,
            stylesheets: &[std::sync::Arc<strata_style::StyleSheet>],
            sheet_index: &StylesheetSelectorIndex,
            path_node_ids: &std::collections::HashMap<Vec<usize>, u64>,
            base_node_styles: &std::collections::HashMap<u64, ComputedStyle>,
            restyled_paths: &mut Vec<(Vec<usize>, bool)>,
            bailout: &mut Option<String>,
        ) -> bool {
            let mut cur_bp = bp_root;
            let mut cur_layout = layout_root;
            let mut prefix = Vec::with_capacity(target_path.len());
            let mut ancestor_stack: Vec<(&NodeBlueprint, (bool, bool, bool))> =
                Vec::with_capacity(target_path.len() + 1);
            let mut group_state = (false, false, false);

            for &idx in target_path {
                // Push current node onto ancestor stack before descending to its child.
                let next_group_state = if let NodeBlueprint::Element { classes, .. } = cur_bp {
                    if AppRuntime::classes_have_group_marker(classes) {
                        let self_hovered =
                            ctx.hovered_path.is_some_and(|h| h.starts_with(&prefix));
                        let self_focused =
                            ctx.focused_path.is_some_and(|f| f.starts_with(&prefix));
                        let self_active = self_hovered && ctx.is_mouse_down;
                        (
                            group_state.0 || self_hovered,
                            group_state.1 || self_focused,
                            group_state.2 || self_active,
                        )
                    } else {
                        group_state
                    }
                } else {
                    group_state
                };
                ancestor_stack.push((cur_bp, group_state));

                let mut flat = Vec::new();
                match cur_bp {
                    NodeBlueprint::Element { children, .. } | NodeBlueprint::Fragment(children) => {
                        for c in children {
                            AppRuntime::flatten_blueprints(c, &mut flat);
                        }
                    }
                    _ => {
                        *bailout = Some("PathNotElement".to_string());
                        return false;
                    }
                }
                if idx >= flat.len() || idx >= cur_layout.children.len() {
                    *bailout = Some("PathIndexOutOfBounds".to_string());
                    return false;
                }
                let child_bp = flat[idx];
                let parent_base_style = path_node_ids
                    .get(&prefix)
                    .and_then(|id| base_node_styles.get(id))
                    .cloned();
                prefix.push(idx);

                // If an ancestor of this path was already restyled in this pass (e.g. shared
                // hover ancestor between old_path and new_path), we are already up to date.
                if restyled_paths
                    .iter()
                    .any(|(rp, _)| prefix.starts_with(rp.as_slice()))
                {
                    return true;
                }

                let node_has_hover = AppRuntime::node_has_hover_effects(child_bp)
                    || AppRuntime::single_node_matches_stylesheet_hover(
                        child_bp,
                        stylesheets,
                        sheet_index,
                    );
                if node_has_hover {
                    let parent_style =
                        parent_base_style.unwrap_or_else(|| cur_layout.style.clone());
                    let child_layout = &mut cur_layout.children[idx];
                    if subtree_has_animations(child_layout) {
                        *bailout = Some("SubtreeHasKeyframeAnimation".to_string());
                        return false;
                    }
                    let has_trans = subtree_has_transitions(child_layout);
                    let mut ancestors_meta = AppRuntime::materialize_ancestor_metas(
                        &ancestor_stack,
                        &prefix,
                        ctx,
                        stylesheets,
                        sheet_index,
                    );
                    let preceding_sibs = if !stylesheets.is_empty()
                        && sheet_index.has_sibling_combinator_or_positional_rules
                        && idx > 0
                    {
                        let parent_path = &prefix[..prefix.len() - 1];
                        let mut sib_path = Vec::with_capacity(prefix.len());
                        sib_path.extend_from_slice(parent_path);
                        sib_path.push(0);
                        let mut sibs = Vec::with_capacity(idx);
                        for (j, sib_bp) in flat[..idx].iter().enumerate() {
                            if let Some(last) = sib_path.last_mut() {
                                *last = j;
                            }
                            if let Some(meta) = AppRuntime::materialize_cascade_meta(
                                sib_bp,
                                &sib_path,
                                ctx,
                                next_group_state,
                            ) {
                                sibs.push(meta);
                            }
                        }
                        sibs
                    } else {
                        Vec::new()
                    };
                    let is_last_elem = flat.iter().rposition(|bp| {
                        matches!(bp, NodeBlueprint::Element { tag, .. } if !AppRuntime::is_style_tag(tag))
                    }) == Some(idx);

                    let mut rebuilt = AppRuntime::build_layout_tree_full(
                        child_bp,
                        Some(&parent_style),
                        &mut prefix,
                        ctx,
                        stylesheets,
                        &mut ancestors_meta,
                        &preceding_sibs,
                        is_last_elem,
                        next_group_state,
                    );
                    if !AppRuntime::transplant_layout_state(child_layout, &mut rebuilt) {
                        *bailout = Some("TransplantMismatch".to_string());
                        return false;
                    }
                    strata_layout::box_model::refresh_inline_fragment_styles(&mut rebuilt);
                    *child_layout = rebuilt;
                    restyled_paths.push((prefix.clone(), has_trans));
                    return true;
                }
                cur_bp = child_bp;
                cur_layout = &mut cur_layout.children[idx];
                group_state = next_group_state;
            }
            true
        }

        let hovered_owned = self.current_hover_path.clone();
        let focused_owned = self.current_focus_path.clone();
        let ctx = InteractionContext {
            hovered_path: hovered_owned.as_deref(),
            focused_path: focused_owned.as_deref(),
            is_mouse_down: self.is_mouse_down,
            is_dark_mode: self.is_dark_mode,
        };
        let mut restyled_paths: Vec<(Vec<usize>, bool)> = Vec::new();
        let mut bailout: Option<String> = None;

        if let Some(p) = old_path {
            if !restyle_one_path(
                &self.root_blueprint,
                &mut self.layout_root,
                p,
                &ctx,
                &self.cached_stylesheets,
                &self.stylesheet_index,
                &self.path_node_ids,
                &self.base_node_styles,
                &mut restyled_paths,
                &mut bailout,
            ) {
                self.last_hover_fast_path_bailout = bailout;
                return false;
            }
        }
        if let Some(p) = new_path {
            if !restyle_one_path(
                &self.root_blueprint,
                &mut self.layout_root,
                p,
                &ctx,
                &self.cached_stylesheets,
                &self.stylesheet_index,
                &self.path_node_ids,
                &self.base_node_styles,
                &mut restyled_paths,
                &mut bailout,
            ) {
                self.last_hover_fast_path_bailout = bailout;
                return false;
            }
        }

        // Sync paint-only transitions strictly on restyled subtrees so hover transitions
        // animate smoothly without full-tree rebuilds or disturbing unrelated active transitions.
        let now_ms = self.runtime_start_epoch.elapsed().as_secs_f64() * 1000.0;
        let anims_active = self.had_active_animations || self.has_active_animations();
        for (path, has_trans) in &restyled_paths {
            if *has_trans {
                self.sync_subtree_paint_transitions(path, now_ms);
            } else if anims_active {
                self.sync_subtree_base_styles(path);
            }
        }

        self.last_hover_fast_path_bailout = None;
        self.sync_active_input_layout();
        self.compile_display_list();
        true
    }

    /// Returns `true` when the currently hovered path (or any stylesheet rule)
    /// can visually respond to `:active` state changes on `MouseDown` / `MouseUp`.
    pub(crate) fn path_has_active_styles(&self) -> bool {
        let Some(ref path) = self.current_hover_path else {
            return false;
        };
        let mut cur_bp = &self.root_blueprint;
        let has_active_class = |bp: &NodeBlueprint| -> bool {
            if let NodeBlueprint::Element { classes, .. } = bp {
                classes
                    .iter()
                    .any(|c| c.split_whitespace().any(|tok| tok.contains("active:")))
            } else {
                false
            }
        };
        if has_active_class(cur_bp) {
            return true;
        }
        for &idx in path {
            let mut flat = Vec::new();
            match cur_bp {
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
            cur_bp = flat[idx];
            if has_active_class(cur_bp) {
                return true;
            }
        }
        for sheet in &self.cached_stylesheets {
            for rule in &sheet.rules {
                for sel in &rule.selectors {
                    let has_active_pseudo = sel.target.parts.iter().any(|p| {
                        matches!(
                            p,
                            strata_style::selector::SelectorPart::PseudoClass(name)
                                if name.eq_ignore_ascii_case("active")
                        )
                    });
                    if has_active_pseudo {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Re-applies `:active` styles only when the hovered path can visually react
    /// to pointer press/release, returning `true` if a repaint is needed.
    pub fn restyle_for_active_change(&mut self) -> bool {
        if !self.path_has_active_styles() {
            return false;
        }
        self.restyle_for_hover_paint();
        true
    }

    /// Copies resolved geometry and text state from a previous tree into a newly styled tree.
    pub(crate) fn transplant_layout_state(
        previous: &mut LayoutNode,
        next: &mut LayoutNode,
    ) -> bool {
        if previous.children.len() != next.children.len() {
            return false;
        }
        next.geometry = previous.geometry;
        next.line_boxes = std::mem::take(&mut previous.line_boxes);
        next.inline_items = std::mem::take(&mut previous.inline_items);
        next.layout_cache = previous.layout_cache;
        next.measure_cache = previous.measure_cache;
        next.intrinsic_cache = previous.intrinsic_cache.clone();
        next.last_child_layout_content = previous.last_child_layout_content;
        next.children_layout_clean = previous.children_layout_clean;
        next.last_preserved_box = previous.last_preserved_box;
        next.subtree_hit_flags = previous.subtree_hit_flags;
        next.subtree_bounds = previous.subtree_bounds;
        for (prev_child, next_child) in previous.children.iter_mut().zip(next.children.iter_mut()) {
            if !Self::transplant_layout_state(prev_child, next_child) {
                return false;
            }
        }
        true
    }

    /// Returns true when any element on the path has mouse boundary listeners.
    pub(crate) fn path_has_hover_listeners(blueprint: &NodeBlueprint, path: &[usize]) -> bool {
        let mut current = blueprint;
        if Self::node_has_hover_listeners(current) {
            return true;
        }
        for &idx in path {
            let mut flat = Vec::new();
            match current {
                NodeBlueprint::Element { children, .. } | NodeBlueprint::Fragment(children) => {
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
            if Self::node_has_hover_listeners(current) {
                return true;
            }
        }
        false
    }

    pub(crate) fn node_has_hover_listeners(node: &NodeBlueprint) -> bool {
        match node {
            NodeBlueprint::Element { attributes, .. } => attributes.iter().any(|attr| match attr {
                strata_dom::Attribute::EventListener { event, .. } => matches!(
                    event,
                    strata_dom::EventType::MouseEnter
                        | strata_dom::EventType::MouseLeave
                        | strata_dom::EventType::MouseOver
                        | strata_dom::EventType::MouseOut
                ),
                _ => false,
            }),
            _ => false,
        }
    }

    /// Clears any active hover state when the pointer departs the window surface.
    pub fn clear_hover(&mut self) -> bool {
        if self.current_hover_path.is_none() {
            if self.dismiss_hover_tooltip() {
                self.hover_paint_only = true;
                self.compile_display_list();
                return true;
            }
            return false;
        }
        let old_path = self.current_hover_path.take();
        self.dispatch_boundary_events(
            old_path.as_deref(),
            None,
            self.last_mouse_pos.0,
            self.last_mouse_pos.1,
        );
        let tooltip_dismissed = self.dismiss_hover_tooltip();
        let old_has_effects = old_path.as_deref().is_some_and(|p| {
            Self::node_path_has_hover_effects_with_index(
                &self.root_blueprint,
                p,
                &self.cached_stylesheets,
                &self.stylesheet_index,
            )
        });
        if old_has_effects {
            let has_listeners = old_path
                .as_deref()
                .is_some_and(|p| Self::path_has_hover_listeners(&self.root_blueprint, p));
            let layout_required = old_path.as_deref().is_some_and(|p| {
                Self::path_hover_affects_layout_with_sheets(
                    &self.root_blueprint,
                    p,
                    &self.cached_stylesheets,
                    &self.stylesheet_index,
                )
            });
            if layout_required || has_listeners {
                self.hover_paint_only = false;
                self.last_hover_fast_path_used = false;
                self.rebuild_layout_tree();
            } else {
                self.hover_paint_only = true;
                let hover_start = std::time::Instant::now();
                let fast_ok = self.restyle_hover_subtrees_fast(old_path.as_deref(), None);
                self.last_hover_fast_path_used = fast_ok;
                if !fast_ok {
                    self.restyle_for_hover_paint();
                }
                self.frame_profile.style_cascade += hover_start.elapsed();
                if perf_logging_enabled() {
                    perf_log_ms(
                        "hover_repaint",
                        hover_start,
                        if fast_ok {
                            "paint-only clear_hover (fast)"
                        } else {
                            "paint-only clear_hover (full)"
                        },
                    );
                }
            }
            true
        } else if tooltip_dismissed {
            self.hover_paint_only = true;
            self.compile_display_list();
            true
        } else {
            self.hover_paint_only = false;
            false
        }
    }

    /// Resolves the nearest native tooltip text along `path` (deepest hovered element up to root).
    /// Prefers non-empty `title` attributes, falling back to non-empty `alt` attributes on
    /// `<button>` and `<img>` elements when no `title` is present along the hovered chain.
    pub(crate) fn resolve_hover_tooltip(
        blueprint: &NodeBlueprint,
        path: &[usize],
    ) -> Option<(Vec<usize>, String)> {
        let mut chain: Vec<(Vec<usize>, &NodeBlueprint)> = Vec::with_capacity(path.len() + 1);
        let mut current = blueprint;
        let mut prefix = Vec::with_capacity(path.len());
        chain.push((prefix.clone(), current));
        for &idx in path {
            let mut flat = Vec::new();
            match current {
                NodeBlueprint::Element { children, .. } | NodeBlueprint::Fragment(children) => {
                    for child in children {
                        Self::flatten_blueprints(child, &mut flat);
                    }
                }
                _ => break,
            }
            if idx >= flat.len() {
                break;
            }
            current = flat[idx];
            prefix.push(idx);
            chain.push((prefix.clone(), current));
        }

        // Pass 1: explicit non-empty `title` attribute (deepest wins).
        for (node_path, node) in chain.iter().rev() {
            if let NodeBlueprint::Element { attributes, .. } = node {
                for attr in attributes {
                    if let strata_dom::Attribute::Property { key, value } = attr {
                        if key.eq_ignore_ascii_case("title") {
                            let trimmed = value.trim();
                            if !trimmed.is_empty() {
                                return Some((node_path.clone(), trimmed.to_string()));
                            }
                        }
                    }
                }
            }
        }

        // Pass 2: fallback to non-empty `alt` attribute on `<button>` or `<img>`.
        for (node_path, node) in chain.iter().rev() {
            if let NodeBlueprint::Element {
                tag, attributes, ..
            } = node
            {
                let supports_alt_tooltip = match tag {
                    strata_dom::Tag::Img | strata_dom::Tag::Button => true,
                    strata_dom::Tag::Custom(name) => {
                        name.eq_ignore_ascii_case("img") || name.eq_ignore_ascii_case("button")
                    }
                    _ => false,
                };
                if !supports_alt_tooltip {
                    continue;
                }
                for attr in attributes {
                    if let strata_dom::Attribute::Property { key, value } = attr {
                        if key.eq_ignore_ascii_case("alt") {
                            let trimmed = value.trim();
                            if !trimmed.is_empty() {
                                return Some((node_path.clone(), trimmed.to_string()));
                            }
                        }
                    }
                }
            }
        }

        None
    }

    /// Synchronizes hover tooltip state with `self.current_hover_path`.
    /// Returns `true` when an active visible tooltip was hidden or changed and requires repainting.
    pub(crate) fn sync_hover_tooltip(&mut self, px: f32, py: f32) -> bool {
        let resolved = self
            .current_hover_path
            .as_deref()
            .and_then(|p| Self::resolve_hover_tooltip(&self.root_blueprint, p));

        match resolved {
            Some((owner_path, text)) => {
                if self.hover_tooltip_path.as_ref() == Some(&owner_path)
                    && self.hover_tooltip_text.as_deref() == Some(text.as_str())
                {
                    if self.hover_tooltip_since.is_some() && !self.hover_tooltip_visible {
                        self.hover_tooltip_anchor = (px, py);
                    }
                    false
                } else {
                    let was_visible = self.hover_tooltip_visible;
                    self.hover_tooltip_path = Some(owner_path);
                    self.hover_tooltip_text = Some(text);
                    self.hover_tooltip_anchor = (px, py);
                    self.hover_tooltip_since = if self.is_mouse_down {
                        None
                    } else {
                        Some(std::time::Instant::now())
                    };
                    self.hover_tooltip_visible = false;
                    was_visible
                }
            }
            None => self.dismiss_hover_tooltip(),
        }
    }

    /// Re-validates the active hover tooltip after `root_blueprint` is replaced by `update_view`.
    pub(crate) fn refresh_hover_tooltip_after_view_update(&mut self) {
        let Some(ref path) = self.current_hover_path else {
            self.dismiss_hover_tooltip();
            return;
        };
        match Self::resolve_hover_tooltip(&self.root_blueprint, path) {
            Some((owner_path, text)) => {
                if self.hover_tooltip_path.as_ref() == Some(&owner_path) {
                    self.hover_tooltip_text = Some(text);
                } else {
                    self.hover_tooltip_path = Some(owner_path);
                    self.hover_tooltip_text = Some(text);
                    self.hover_tooltip_anchor = self.last_mouse_pos;
                    self.hover_tooltip_since = if self.is_mouse_down {
                        None
                    } else {
                        Some(std::time::Instant::now())
                    };
                    self.hover_tooltip_visible = false;
                }
            }
            None => {
                self.dismiss_hover_tooltip();
            }
        }
    }

    /// Clears all pending/visible native hover tooltip state.
    /// Returns `true` if a visible tooltip was dismissed.
    pub fn dismiss_hover_tooltip(&mut self) -> bool {
        let was_visible = self.hover_tooltip_visible;
        self.hover_tooltip_text = None;
        self.hover_tooltip_path = None;
        self.hover_tooltip_since = None;
        self.hover_tooltip_visible = false;
        was_visible
    }

    /// Dismisses any visible or pending native hover tooltip and recompiles the display list if
    /// the tooltip was visible on screen.
    pub fn dismiss_hover_tooltip_and_repaint(&mut self) -> bool {
        let was_visible = self.hover_tooltip_visible;
        self.hover_tooltip_since = None;
        self.hover_tooltip_visible = false;
        if was_visible {
            self.paint_dirty = true;
            self.compile_display_list();
        }
        was_visible
    }

    /// Returns the remaining duration until a pending native hover tooltip should appear.
    pub fn next_hover_tooltip_due_in(&self) -> Option<std::time::Duration> {
        if self.hover_tooltip_visible || self.is_mouse_down {
            return None;
        }
        let (_, Some(since)) = (&self.hover_tooltip_text, self.hover_tooltip_since) else {
            return None;
        };
        Some(self.hover_tooltip_delay.saturating_sub(since.elapsed()))
    }

    /// Advances the native hover tooltip dwell timer. Returns `true` when the tooltip transitions
    /// to visible and updates the display list.
    pub fn poll_hover_tooltip(&mut self) -> bool {
        if self.hover_tooltip_visible || self.is_mouse_down {
            return false;
        }
        let (Some(_), Some(since)) = (&self.hover_tooltip_text, self.hover_tooltip_since) else {
            return false;
        };
        if since.elapsed() < self.hover_tooltip_delay {
            return false;
        }
        self.hover_tooltip_visible = true;
        self.paint_dirty = true;
        self.compile_display_list();
        true
    }

    /// Appends native hover tooltip draw commands on top of the compiled display list when active.
    pub fn render_hover_tooltip(&mut self) {
        if !self.hover_tooltip_visible {
            return;
        }
        let Some(ref text) = self.hover_tooltip_text else {
            return;
        };
        if text.is_empty() || self.viewport_width <= 0.0 || self.viewport_height <= 0.0 {
            return;
        }

        let style = self.hover_tooltip_style;
        let font_size = style.font_size.max(6.0);
        let line_height = style.line_height.max(1.0);
        let font_family = strata_primitives::AtomId::FONT_SANS;
        let font_weight = strata_style::types::FontWeight::Normal;
        let pad_x = style.padding_x.max(0.0);
        let pad_y = style.padding_y.max(0.0);
        let margin = 6.0_f32;
        let max_allowed_w = style.max_width.max(24.0);
        let max_text_w = (self.viewport_width - pad_x * 2.0 - margin * 2.0)
            .clamp(24.0_f32.min(max_allowed_w), max_allowed_w);

        let (tw, th) = strata_layout::measure_text_bounds_styled_with_family(
            text,
            font_size,
            line_height,
            Some(max_text_w),
            false,
            font_family,
            font_weight,
            strata_style::types::FontStyle::Normal,
        );
        let text_w = tw.max(12.0).min(max_text_w);
        let text_h = th.max(font_size * line_height);
        let box_w = (text_w + pad_x * 2.0)
            .ceil()
            .min((self.viewport_width - margin * 2.0).max(24.0));
        let box_h = (text_h + pad_y * 2.0)
            .ceil()
            .min((self.viewport_height - margin * 2.0).max(22.0));

        let (ax, ay) = self.hover_tooltip_anchor;
        let mut x = ax + style.cursor_offset_x;
        let mut y = ay + style.cursor_offset_y;
        if x + box_w > self.viewport_width - margin {
            x = (self.viewport_width - margin - box_w).max(margin);
        }
        if y + box_h > self.viewport_height - margin {
            y = (ay - box_h - 10.0).max(margin);
        }
        x = x.max(margin);
        y = y.max(margin);

        let bounds = strata_layout::Rect::new(x, y, box_w, box_h);
        let radius = strata_style::types::BorderRadius::all(style.corner_radius.max(0.0));

        if style.shadow_color.a > 0 {
            self.display_list
                .push(strata_render::compositor::DrawCommand::DrawShadow {
                    bounds,
                    shadow: strata_style::types::BoxShadow {
                        offset_x: 0.0,
                        offset_y: 4.0,
                        blur_radius: 12.0,
                        spread_radius: 0.0,
                        color: style.shadow_color,
                        inset: false,
                    },
                    border_radius: radius,
                });
        }
        let border_w = style.border_width.max(0.0);
        self.display_list
            .push(strata_render::compositor::DrawCommand::DrawRoundedRect {
                bounds,
                color: style.background_color,
                border_radius: radius,
                border_width: strata_primitives::geom::EdgeInsets::all(border_w),
                border_color: if border_w > 0.0 {
                    Some(style.border_color)
                } else {
                    None
                },
            });
        self.display_list
            .push(strata_render::compositor::DrawCommand::DrawText {
                origin: (x + pad_x, y + pad_y),
                bounds_width: Some((box_w - pad_x * 2.0).max(1.0)),
                bounds_height: Some((box_h - pad_y * 2.0).max(1.0)),
                text_align: strata_style::TextAlign::Left,
                text: text.clone(),
                font_size,
                line_height,
                font_weight,
                color: style.text_color,
                is_monospace: false,
                font_family,
            });
    }
}
