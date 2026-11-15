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

    pub(crate) fn hit_test_node_path_rec(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        px: f32,
        py: f32,
        current_path: &mut Vec<usize>,
    ) -> Option<Vec<usize>> {
        Self::hit_test_node_path_sc(blueprint, layout, px, py, current_path)
    }

    fn collect_hoisted_hover_participants<'a>(
        bp: &'a NodeBlueprint,
        layout: &'a LayoutNode,
        current_path: &mut Vec<usize>,
        negative_z: &mut Vec<(&'a NodeBlueprint, &'a LayoutNode, Vec<usize>)>,
        auto_or_zero_z: &mut Vec<(&'a NodeBlueprint, &'a LayoutNode, Vec<usize>, bool)>,
        positive_z: &mut Vec<(&'a NodeBlueprint, &'a LayoutNode, Vec<usize>)>,
    ) {
        if current_path.len() >= 256 {
            return;
        }
        for (idx, child_bp, child_layout) in Self::paired_blueprint_children(bp, layout) {
            if matches!(child_layout.style.display, strata_style::types::Display::None)
                || matches!(
                    child_layout.style.visibility,
                    strata_style::types::Visibility::Hidden
                )
                || matches!(
                    child_layout.style.pointer_events,
                    strata_style::types::PointerEvents::None
                )
            {
                continue;
            }
            current_path.push(idx);
            if Self::establishes_stacking_context(child_layout, false) {
                let z = child_layout.style.z_index.unwrap_or(0);
                if z < 0 {
                    negative_z.push((child_bp, child_layout, current_path.clone()));
                } else if z > 0 {
                    positive_z.push((child_bp, child_layout, current_path.clone()));
                } else {
                    auto_or_zero_z.push((child_bp, child_layout, current_path.clone(), true));
                }
            } else {
                if child_layout.style.position != strata_style::types::Position::Static {
                    auto_or_zero_z.push((child_bp, child_layout, current_path.clone(), false));
                }
                Self::collect_hoisted_hover_participants(
                    child_bp,
                    child_layout,
                    current_path,
                    negative_z,
                    auto_or_zero_z,
                    positive_z,
                );
            }
            current_path.pop();
        }
    }

    fn hit_test_node_path_in_flow(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        px: f32,
        py: f32,
        current_path: &mut Vec<usize>,
    ) -> Option<Vec<usize>> {
        if current_path.len() >= 256
            || matches!(layout.style.display, strata_style::types::Display::None)
            || matches!(
                layout.style.visibility,
                strata_style::types::Visibility::Hidden
            )
            || matches!(
                layout.style.pointer_events,
                strata_style::types::PointerEvents::None
            )
        {
            return None;
        }

        let paired = Self::paired_blueprint_children(blueprint, layout);
        for &(idx, child_bp, child_layout) in paired.iter().rev() {
            if matches!(child_layout.style.display, strata_style::types::Display::None)
                || Self::establishes_stacking_context(child_layout, false)
                || child_layout.style.position != strata_style::types::Position::Static
            {
                continue;
            }
            current_path.push(idx);
            if let Some(res) =
                Self::hit_test_node_path_in_flow(child_bp, child_layout, px, py, current_path)
            {
                return Some(res);
            }
            current_path.pop();
        }

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
    }

    fn hit_test_node_path_sc(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        px: f32,
        py: f32,
        current_path: &mut Vec<usize>,
    ) -> Option<Vec<usize>> {
        if current_path.len() >= 256
            || matches!(layout.style.display, strata_style::types::Display::None)
            || matches!(
                layout.style.visibility,
                strata_style::types::Visibility::Hidden
            )
            || matches!(
                layout.style.pointer_events,
                strata_style::types::PointerEvents::None
            )
        {
            return None;
        }

        let (px, py) = Self::unproject_point(layout, px, py);

        // If element clips overflow, coordinates outside its border box are clipped
        if (layout.style.overflow_x != strata_style::types::Overflow::Visible
            || layout.style.overflow_y != strata_style::types::Overflow::Visible)
            && !layout.geometry.border_box.contains_point(px, py)
        {
            return None;
        }

        let mut negative_z = Vec::new();
        let mut auto_or_zero_z = Vec::new();
        let mut positive_z = Vec::new();
        Self::collect_hoisted_hover_participants(
            blueprint,
            layout,
            current_path,
            &mut negative_z,
            &mut auto_or_zero_z,
            &mut positive_z,
        );

        negative_z.sort_by_key(|(_, l, _)| l.style.z_index.unwrap_or(0));
        positive_z.sort_by_key(|(_, l, _)| l.style.z_index.unwrap_or(0));

        // Layer 7: positive z-index child stacking contexts
        for (c_bp, c_layout, c_path) in positive_z.into_iter().rev() {
            let mut path_buf = c_path;
            if let Some(res) =
                Self::hit_test_node_path_sc(c_bp, c_layout, px, py, &mut path_buf)
            {
                return Some(res);
            }
        }

        // Layer 6: positioned auto/0 z-index descendants
        for (c_bp, c_layout, c_path, is_sc) in auto_or_zero_z.into_iter().rev() {
            let mut path_buf = c_path;
            let hit = if is_sc {
                Self::hit_test_node_path_sc(c_bp, c_layout, px, py, &mut path_buf)
            } else {
                Self::hit_test_node_path_in_flow(c_bp, c_layout, px, py, &mut path_buf)
            };
            if hit.is_some() {
                return hit;
            }
        }

        // Layers 5, 4, 3: direct static non-stacking-context children
        let paired = Self::paired_blueprint_children(blueprint, layout);
        for &(idx, child_bp, child_layout) in paired.iter().rev() {
            if matches!(child_layout.style.display, strata_style::types::Display::None)
                || Self::establishes_stacking_context(child_layout, false)
                || child_layout.style.position != strata_style::types::Position::Static
            {
                continue;
            }
            current_path.push(idx);
            if let Some(res) =
                Self::hit_test_node_path_in_flow(child_bp, child_layout, px, py, current_path)
            {
                return Some(res);
            }
            current_path.pop();
        }

        // Layer 2: negative z-index child stacking contexts
        for (c_bp, c_layout, c_path) in negative_z.into_iter().rev() {
            let mut path_buf = c_path;
            if let Some(res) =
                Self::hit_test_node_path_sc(c_bp, c_layout, px, py, &mut path_buf)
            {
                return Some(res);
            }
        }

        // Layer 1: stacking context root itself
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

        let stylesheets = Self::collect_stylesheets_from_blueprint(blueprint);
        if stylesheets.is_empty() {
            return false;
        }

        Self::path_nodes_match_stylesheet_hover(&path_nodes, &stylesheets)
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
            is_focused: false,
            is_active: false,
            is_disabled: false,
            is_dark: false,
        };
        let probe_state_dark = strata_style::selector::PseudoState {
            is_hovered: true,
            is_focused: false,
            is_active: false,
            is_disabled: false,
            is_dark: true,
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

    /// Checks if a single blueprint node has hover utility classes (including chained modifiers like `dark:hover:*`) or boundary event listeners.
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
            return false;
        }

        let old_path = self.current_hover_path.take();
        self.current_hover_path = new_path.clone();

        self.dispatch_boundary_events(old_path.as_deref(), new_path.as_deref(), px, py);

        let old_has_effects = old_path.as_deref().is_some_and(|p| {
            Self::node_path_has_hover_effects(&self.root_blueprint, p)
        });
        let new_has_effects = new_path.as_deref().is_some_and(|p| {
            Self::node_path_has_hover_effects(&self.root_blueprint, p)
        });

        if old_has_effects || new_has_effects {
            let has_listeners = old_path.as_deref().is_some_and(|p| {
                Self::path_has_hover_listeners(&self.root_blueprint, p)
            }) || new_path.as_deref().is_some_and(|p| {
                Self::path_has_hover_listeners(&self.root_blueprint, p)
            });
            let layout_required = old_path.as_deref().is_some_and(|p| {
                Self::path_hover_affects_layout(&self.root_blueprint, p)
            }) || new_path.as_deref().is_some_and(|p| {
                Self::path_hover_affects_layout(&self.root_blueprint, p)
            });
            if layout_required || has_listeners {
                self.hover_paint_only = false;
                self.rebuild_layout_tree();
            } else {
                self.hover_paint_only = true;
                let hover_start = std::time::Instant::now();
                self.restyle_for_hover_paint();
                if perf_logging_enabled() {
                    perf_log_ms("hover_repaint", hover_start, "paint-only hover");
                }
            }
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
        let stylesheets = Self::collect_stylesheets_from_blueprint(&self.root_blueprint);
        for sheet in &stylesheets {
            self.register_keyframes_stylesheet(sheet);
        }
        let ctx = self.interaction_context();
        let new_tree = Self::build_layout_tree_with_interaction(
            &self.root_blueprint,
            &ctx,
        );
        let mut previous = std::mem::replace(&mut self.layout_root, new_tree);
        let transplanted = Self::transplant_layout_state(&mut previous, &mut self.layout_root);
        let now_ms = self.runtime_start_epoch.elapsed().as_secs_f64() * 1000.0;
        self.sync_tree_animations(now_ms);
        if transplanted && !self.transition_manager.has_layout_affecting_transitions() {
            strata_layout::box_model::refresh_inline_fragment_styles(&mut self.layout_root);
        } else {
            self.applied_scroll_offsets.clear();
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

    /// Copies resolved geometry and text state from a previous tree into a newly styled tree.
    pub(crate) fn transplant_layout_state(previous: &mut LayoutNode, next: &mut LayoutNode) -> bool {
        if previous.children.len() != next.children.len() {
            return false;
        }
        next.geometry = previous.geometry;
        next.line_boxes = std::mem::take(&mut previous.line_boxes);
        next.inline_items = std::mem::take(&mut previous.inline_items);
        next.layout_cache = previous.layout_cache;
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
            return false;
        }
        let old_path = self.current_hover_path.take();
        self.dispatch_boundary_events(
            old_path.as_deref(),
            None,
            self.last_mouse_pos.0,
            self.last_mouse_pos.1,
        );
        let old_has_effects = old_path.as_deref().is_some_and(|p| {
            Self::node_path_has_hover_effects(&self.root_blueprint, p)
        });
        if old_has_effects {
            let has_listeners = old_path.as_deref().is_some_and(|p| {
                Self::path_has_hover_listeners(&self.root_blueprint, p)
            });
            let layout_required = old_path.as_deref().is_some_and(|p| {
                Self::path_hover_affects_layout(&self.root_blueprint, p)
            });
            if layout_required || has_listeners {
                self.hover_paint_only = false;
                self.rebuild_layout_tree();
            } else {
                self.hover_paint_only = true;
                let hover_start = std::time::Instant::now();
                self.restyle_for_hover_paint();
                if perf_logging_enabled() {
                    perf_log_ms("hover_repaint", hover_start, "paint-only hover");
                }
            }
            true
        } else {
            self.hover_paint_only = false;
            false
        }
    }

}
