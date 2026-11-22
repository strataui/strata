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

    /// Traverses the LayoutNode hierarchy down a slice of child indices in O(depth).
    pub fn get_layout_node<'a>(root: &'a LayoutNode, path: &[usize]) -> Option<&'a LayoutNode> {
        let mut current = root;
        for &idx in path {
            current = current.children.get(idx)?;
        }
        Some(current)
    }

    /// Restores any scrollable containers and anchored `<Portal>` nodes previously translated by
    /// `apply_scroll_offsets` back to their baseline unshifted coordinates so layout computations
    /// and metric measurements remain invariant.
    pub fn unapply_scroll_offsets(&mut self) {
        if !self.applied_portal_offsets.is_empty() {
            for (path, &(dx, dy)) in &self.applied_portal_offsets {
                if dx.abs() > 0.0001 || dy.abs() > 0.0001 {
                    if let Some(target) = Self::get_layout_node_mut(&mut self.layout_root, path) {
                        target.translate(-dx, -dy);
                    }
                }
            }
            self.applied_portal_offsets.clear();
        }

        if self.applied_scroll_offsets.is_empty() {
            return;
        }
        for (path, &(applied_x, applied_y)) in &self.applied_scroll_offsets {
            if applied_x.abs() > 0.0001 || applied_y.abs() > 0.0001 {
                if let Some(target) = Self::get_layout_node_mut(&mut self.layout_root, path) {
                    let target_fixed_cb = target.style.establishes_fixed_containing_block();
                    let target_abs_cb = target.style.position
                        != strata_style::types::Position::Static
                        || target_fixed_cb;
                    for child in &mut target.children {
                        child.translate_scrolled(
                            applied_x,
                            applied_y,
                            target_abs_cb,
                            target_fixed_cb,
                        );
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

    /// Discovers scrollable containers, clamps offsets, translates children, computes scrollbars,
    /// and anchors top-layer `<Portal>` overlays to their trigger elements' live post-scroll bounds.
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
        self.flush_pending_scroll_commands();
        self.sync_scroll_animations();
        self.resolve_top_layer_portals();
        self.rebuild_active_scrollbars();
    }

    /// Walks from `root` to `path`, accumulating CSS `style.transform` matrices and returning the
    /// screen-space axis-aligned bounding box of the target node.
    fn effective_screen_border_box(root: &LayoutNode, path: &[usize]) -> Option<strata_layout::Rect> {
        let mut mat = strata_primitives::geom::Matrix4x4::identity();
        let mut cur = root;
        for &idx in path {
            if cur.is_top_layer {
                mat = strata_primitives::geom::Matrix4x4::identity();
            }
            if cur.style.has_transform() {
                let bx = cur.geometry.border_box.x;
                let by = cur.geometry.border_box.y;
                let local_mat = cur.style.compute_transform_matrix(
                    cur.geometry.border_box.width,
                    cur.geometry.border_box.height,
                );
                let origin_mat = strata_primitives::geom::Matrix4x4::translation(bx, by, 0.0)
                    .multiply(&local_mat)
                    .multiply(&strata_primitives::geom::Matrix4x4::translation(-bx, -by, 0.0));
                mat = mat.multiply(&origin_mat);
            }
            cur = cur.children.get(idx)?;
        }
        if cur.is_top_layer {
            mat = strata_primitives::geom::Matrix4x4::identity();
        }
        let b = cur.geometry.border_box;
        if !cur.style.has_transform() && mat == strata_primitives::geom::Matrix4x4::identity() {
            return Some(b);
        }
        if cur.style.has_transform() {
            let local_mat = cur.style.compute_transform_matrix(b.width, b.height);
            let origin_mat = strata_primitives::geom::Matrix4x4::translation(b.x, b.y, 0.0)
                .multiply(&local_mat)
                .multiply(&strata_primitives::geom::Matrix4x4::translation(-b.x, -b.y, 0.0));
            mat = mat.multiply(&origin_mat);
        }
        let corners = [
            mat.transform_point(b.x, b.y),
            mat.transform_point(b.x + b.width, b.y),
            mat.transform_point(b.x, b.y + b.height),
            mat.transform_point(b.x + b.width, b.y + b.height),
        ];
        let min_x = corners.iter().map(|c| c.0).fold(f32::INFINITY, f32::min);
        let min_y = corners.iter().map(|c| c.1).fold(f32::INFINITY, f32::min);
        let max_x = corners.iter().map(|c| c.0).fold(f32::NEG_INFINITY, f32::max);
        let max_y = corners.iter().map(|c| c.1).fold(f32::NEG_INFINITY, f32::max);
        Some(strata_layout::Rect::new(
            min_x,
            min_y,
            (max_x - min_x).max(0.0),
            (max_y - min_y).max(0.0),
        ))
    }

    fn compute_container_scroll_extents(
        layout: &LayoutNode,
        applied_x: f32,
        applied_y: f32,
    ) -> (f32, f32, f32, f32) {
        let viewport_w = layout.geometry.content_box.width;
        let viewport_h = layout.geometry.content_box.height;

        let mut max_child_x = layout.geometry.content_box.x;
        let mut max_child_y = layout.geometry.content_box.y;

        let target_fixed_cb = layout.style.establishes_fixed_containing_block();
        let target_abs_cb =
            layout.style.position != strata_style::types::Position::Static || target_fixed_cb;

        for child in &layout.children {
            if child.is_top_layer
                || child.style.position == strata_style::types::Position::Fixed
                || (child.style.position == strata_style::types::Position::Absolute
                    && !target_abs_cb)
            {
                continue;
            }
            max_child_x =
                max_child_x.max(child.geometry.border_box.x + child.geometry.border_box.width + applied_x);
            max_child_y =
                max_child_y.max(child.geometry.border_box.y + child.geometry.border_box.height + applied_y);
        }
        for line in &layout.line_boxes {
            max_child_x = max_child_x.max(line.bounds.x + line.bounds.width + applied_x);
            max_child_y = max_child_y.max(line.bounds.y + line.bounds.height + applied_y);
        }

        let content_w = (max_child_x - layout.geometry.content_box.x).max(0.0);
        let content_h = (max_child_y - layout.geometry.content_box.y).max(0.0);
        let max_scroll_x = (content_w - viewport_w).max(0.0);
        let max_scroll_y = (content_h - viewport_h).max(0.0);
        (content_w, content_h, max_scroll_x, max_scroll_y)
    }

    /// Recomputes `active_scrollbars` in final post-scroll, post-portal coordinates along with
    /// each scrollbar's clipped viewport rect so top-layer `<Portal>` scrollbars render and
    /// hit-test in their translated, clipped coordinates.
    pub(crate) fn rebuild_active_scrollbars(&mut self) {
        self.active_scrollbars.clear();
        let viewport_clip = if self.viewport_width > 0.0 && self.viewport_height > 0.0 {
            Some(strata_layout::Rect::new(
                0.0,
                0.0,
                self.viewport_width,
                self.viewport_height,
            ))
        } else {
            None
        };
        let mut path = Vec::new();
        Self::collect_active_scrollbars_walk(
            &self.root_blueprint,
            &self.layout_root,
            &mut path,
            &[],
            false,
            viewport_clip,
            &self.scroll_offsets,
            &self.scroll_offsets_by_id,
            &self.applied_scroll_offsets,
            &mut self.active_scrollbars,
        );
        self.active_scrollbars.sort_by_key(|b| b.is_top_layer);
    }

    #[allow(clippy::too_many_arguments)]
    fn collect_active_scrollbars_walk(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        current_path: &mut Vec<usize>,
        inherited_steps: &[super::hit_test::HitClipStep],
        in_top_layer: bool,
        viewport_clip: Option<strata_layout::Rect>,
        scroll_offsets: &std::collections::HashMap<Vec<usize>, (f32, f32)>,
        scroll_offsets_by_id: &std::collections::HashMap<String, (f32, f32)>,
        applied_scroll_offsets: &std::collections::HashMap<Vec<usize>, (f32, f32)>,
        out: &mut Vec<ActiveScrollbar>,
    ) {
        if current_path.len() >= 256
            || matches!(layout.style.display, strata_style::types::Display::None)
        {
            return;
        }

        let node_in_top_layer = in_top_layer || layout.is_top_layer;
        let eff_inherited = if layout.is_top_layer {
            &[][..]
        } else {
            Self::effective_hit_clip_steps(layout, inherited_steps)
        };

        let is_scroll_y = matches!(
            layout.style.overflow_y,
            strata_style::types::Overflow::Auto | strata_style::types::Overflow::Scroll
        );
        let viewport_h = layout.geometry.content_box.height;
        if is_scroll_y && viewport_h > 0.0 {
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
            let (applied_x, applied_y) = applied_scroll_offsets
                .get(current_path)
                .copied()
                .unwrap_or((0.0, 0.0));
            let (_, content_h, _, max_scroll_y) =
                Self::compute_container_scroll_extents(layout, applied_x, applied_y);

            if max_scroll_y > 0.0 {
                let stored_y = if let Some(ref id) = elem_id {
                    scroll_offsets_by_id
                        .get(id)
                        .copied()
                        .or_else(|| scroll_offsets.get(current_path).copied())
                        .map(|(_, y)| y)
                        .unwrap_or(applied_y)
                } else {
                    scroll_offsets
                        .get(current_path)
                        .copied()
                        .map(|(_, y)| y)
                        .unwrap_or(applied_y)
                };
                let clamped_y = stored_y.clamp(0.0, max_scroll_y);

                let border_box = layout.geometry.border_box;
                let mut clip_bounds = border_box;
                if let Some(vp) = viewport_clip {
                    clip_bounds = clip_bounds
                        .intersect(vp)
                        .unwrap_or_else(|| strata_layout::Rect::new(0.0, 0.0, 0.0, 0.0));
                }
                for step in eff_inherited {
                    if let Some(c) = step.clip {
                        clip_bounds = clip_bounds
                            .intersect(c)
                            .unwrap_or_else(|| strata_layout::Rect::new(0.0, 0.0, 0.0, 0.0));
                    }
                }

                let track_width = 10.0;
                let track_x = border_box.x + border_box.width - track_width;
                let track_y = border_box.y;
                let track_h = border_box.height;
                let track_bounds =
                    strata_layout::Rect::new(track_x, track_y, track_width, track_h);

                let min_thumb = 24.0_f32.min(track_h);
                let thumb_h =
                    ((viewport_h / content_h.max(1.0)) * track_h).clamp(min_thumb, track_h);
                let available_travel = (track_h - thumb_h).max(0.0);
                let thumb_y = if max_scroll_y > 0.0 && available_travel > 0.0 {
                    track_y + (clamped_y / max_scroll_y) * available_travel
                } else {
                    track_y
                };
                let thumb_bounds =
                    strata_layout::Rect::new(track_x, thumb_y, track_width, thumb_h);

                out.push(ActiveScrollbar {
                    bounds: border_box,
                    clip_bounds,
                    border_radius: layout.style.border_radius,
                    is_top_layer: node_in_top_layer,
                    track_bounds,
                    thumb_bounds,
                    scroll_y: clamped_y,
                    max_scroll_y,
                    content_h,
                    is_id: elem_id.is_some(),
                    id: elem_id.unwrap_or_default(),
                    path: current_path.clone(),
                    available_travel,
                });
            }
        }

        let mut next_steps = Vec::with_capacity(eff_inherited.len() + 1);
        next_steps.extend_from_slice(eff_inherited);
        let own_step = Self::hit_clip_step_for(layout);
        if own_step.clip.is_some() || own_step.is_abs_cb || own_step.is_fixed_cb {
            next_steps.push(own_step);
        }

        for (i, child_bp, child_layout) in Self::paired_blueprint_children(blueprint, layout) {
            current_path.push(i);
            Self::collect_active_scrollbars_walk(
                child_bp,
                child_layout,
                current_path,
                &next_steps,
                node_in_top_layer,
                viewport_clip,
                scroll_offsets,
                scroll_offsets_by_id,
                applied_scroll_offsets,
                out,
            );
            current_path.pop();
        }
    }

    #[inline]
    fn matching_scrollbars_at(&self, px: f32, py: f32) -> Vec<ActiveScrollbar> {
        self.active_scrollbars
            .iter()
            .filter(|bar| bar.clip_bounds.contains_point(px, py))
            .cloned()
            .collect()
    }

    fn subtree_has_portal_anchor(node: &LayoutNode, depth: usize) -> bool {
        if depth >= 256 || matches!(node.style.display, strata_style::types::Display::None) {
            return false;
        }
        if node.portal_anchor.is_some() {
            return true;
        }
        node.children
            .iter()
            .any(|c| Self::subtree_has_portal_anchor(c, depth + 1))
    }

    fn insert_anchor_key(
        map: &mut std::collections::HashMap<String, Vec<usize>>,
        raw_key: &str,
        path: &[usize],
    ) {
        let trimmed = raw_key.trim();
        if trimmed.is_empty() {
            return;
        }
        let p = path.to_vec();
        map.insert(trimmed.to_string(), p.clone());
        if let Some(stripped) = trimmed.strip_prefix("--") {
            if !stripped.is_empty() {
                map.entry(stripped.to_string()).or_insert_with(|| p.clone());
            }
        } else {
            map.entry(format!("--{}", trimmed)).or_insert(p);
        }
    }

    fn collect_anchor_paths(
        node: &LayoutNode,
        current_path: &mut Vec<usize>,
        out: &mut std::collections::HashMap<String, Vec<usize>>,
    ) {
        if current_path.len() >= 256
            || matches!(node.style.display, strata_style::types::Display::None)
        {
            return;
        }
        if let Some(ref id) = node.element_id {
            Self::insert_anchor_key(out, id, current_path);
        }
        if let Some(ref aname) = node.anchor_name {
            Self::insert_anchor_key(out, aname, current_path);
        }
        for (idx, child) in node.children.iter().enumerate() {
            current_path.push(idx);
            Self::collect_anchor_paths(child, current_path, out);
            current_path.pop();
        }
    }

    fn collect_anchored_portal_paths(
        node: &LayoutNode,
        current_path: &mut Vec<usize>,
        out: &mut Vec<Vec<usize>>,
    ) {
        if current_path.len() >= 256
            || matches!(node.style.display, strata_style::types::Display::None)
        {
            return;
        }
        if node.portal_anchor.is_some() {
            out.push(current_path.clone());
        }
        for (idx, child) in node.children.iter().enumerate() {
            current_path.push(idx);
            Self::collect_anchored_portal_paths(child, current_path, out);
            current_path.pop();
        }
    }

    /// Resolves anchored `<Portal>` / CSS `position-anchor` nodes against their trigger elements'
    /// live post-scroll bounding boxes with automatic viewport collision flipping (`bottom-end <-> top-end`)
    /// and viewport margin shifting.
    pub fn resolve_top_layer_portals(&mut self) {
        let has_any = Self::subtree_has_portal_anchor(&self.layout_root, 0);
        self.has_anchored_portals = has_any;
        if !has_any {
            return;
        }

        let mut anchor_paths = std::collections::HashMap::new();
        let mut path_buf = Vec::new();
        Self::collect_anchor_paths(&self.layout_root, &mut path_buf, &mut anchor_paths);

        let mut portal_paths = Vec::new();
        path_buf.clear();
        Self::collect_anchored_portal_paths(&self.layout_root, &mut path_buf, &mut portal_paths);

        let viewport_rect =
            strata_layout::Rect::new(0.0, 0.0, self.viewport_width, self.viewport_height);

        for portal_path in portal_paths {
            let Some((spec, cur_box)) = Self::get_layout_node(&self.layout_root, &portal_path)
                .and_then(|n| {
                    n.portal_anchor
                        .as_deref()
                        .cloned()
                        .map(|s| (s, n.geometry.border_box))
                })
            else {
                continue;
            };

            let key = spec.anchor_id.trim();
            let anchor_path_opt = anchor_paths
                .get(key)
                .or_else(|| key.strip_prefix("--").and_then(|s| anchor_paths.get(s)));
            let Some(anchor_rect) = anchor_path_opt
                .and_then(|ap| Self::effective_screen_border_box(&self.layout_root, ap))
            else {
                continue;
            };

            let mut active_box = cur_box;
            let max_vp_width = (viewport_rect.width - 2.0 * spec.viewport_padding).max(0.0);
            if spec.match_width && (active_box.width - anchor_rect.width).abs() > 0.5 {
                let prior = self
                    .applied_portal_offsets
                    .remove(&portal_path)
                    .unwrap_or((0.0, 0.0));
                if let Some(target) = Self::get_layout_node_mut(&mut self.layout_root, &portal_path)
                {
                    if prior.0.abs() > 0.0001 || prior.1.abs() > 0.0001 {
                        target.translate(-prior.0, -prior.1);
                    }
                    target.style.width = strata_style::types::Dimension::Px(anchor_rect.width);
                    strata_layout::solve_out_of_flow_box(
                        target,
                        viewport_rect,
                        viewport_rect,
                        true,
                    );
                    active_box = target.geometry.border_box;
                }
            } else if !spec.match_width && spec.min_width_from_anchor {
                let target_min_w = if max_vp_width > 0.0 {
                    anchor_rect.width.min(max_vp_width)
                } else {
                    anchor_rect.width
                };
                let existing_min_px = match Self::get_layout_node_mut(&mut self.layout_root, &portal_path)
                    .map(|n| n.style.min_width)
                {
                    Some(strata_style::types::Dimension::Px(px)) => px,
                    _ => 0.0,
                };
                let needs_min_resize =
                    active_box.width + 0.5 < target_min_w || existing_min_px < target_min_w - 0.5;
                let needs_max_cap = max_vp_width > 0.0 && active_box.width > max_vp_width + 0.5;
                if needs_min_resize || needs_max_cap {
                    let prior = self
                        .applied_portal_offsets
                        .remove(&portal_path)
                        .unwrap_or((0.0, 0.0));
                    if let Some(target) =
                        Self::get_layout_node_mut(&mut self.layout_root, &portal_path)
                    {
                        if prior.0.abs() > 0.0001 || prior.1.abs() > 0.0001 {
                            target.translate(-prior.0, -prior.1);
                        }
                        if target_min_w > existing_min_px {
                            target.style.min_width =
                                strata_style::types::Dimension::Px(target_min_w);
                        }
                        if max_vp_width > 0.0
                            && matches!(
                                target.style.max_width,
                                strata_style::types::Dimension::Auto
                            )
                        {
                            target.style.max_width =
                                strata_style::types::Dimension::Px(max_vp_width);
                        }
                        strata_layout::solve_out_of_flow_box(
                            target,
                            viewport_rect,
                            viewport_rect,
                            true,
                        );
                        active_box = target.geometry.border_box;
                    }
                }
            }

            let (target_x, target_y, _resolved_placement) = strata_layout::solve_anchored_position(
                anchor_rect,
                active_box.width,
                active_box.height,
                spec.placement,
                spec.offset,
                spec.flip,
                spec.shift,
                spec.viewport_padding,
                viewport_rect,
            );

            let dx = target_x - active_box.x;
            let dy = target_y - active_box.y;
            if dx.abs() > 0.0001 || dy.abs() > 0.0001 {
                if let Some(target) = Self::get_layout_node_mut(&mut self.layout_root, &portal_path)
                {
                    target.translate(dx, dy);
                }
                let entry = self
                    .applied_portal_offsets
                    .entry(portal_path)
                    .or_insert((0.0, 0.0));
                entry.0 += dx;
                entry.1 += dy;
            }
        }
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
            let (_, _, max_scroll_x, max_scroll_y) =
                Self::compute_container_scroll_extents(layout, 0.0, 0.0);

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

            if let Some(ref id) = elem_id {
                strata_dom::hooks::set_live_scroll_offset(id, clamped_x, clamped_y);
            }

            if clamped_x > 0.0 || clamped_y > 0.0 {
                let target_fixed_cb = layout.style.establishes_fixed_containing_block();
                let target_abs_cb = layout.style.position != strata_style::types::Position::Static
                    || target_fixed_cb;
                for child in &mut layout.children {
                    child.translate_scrolled(
                        -clamped_x,
                        -clamped_y,
                        target_abs_cb,
                        target_fixed_cb,
                    );
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

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_scroll_xy_delta_to_container(
        &mut self,
        path: &[usize],
        is_id: bool,
        id: &str,
        new_scroll_x: f32,
        new_scroll_y: f32,
        dx: f32,
        dy: f32,
    ) {
        if is_id && !id.is_empty() {
            self.scroll_offsets_by_id
                .insert(id.to_string(), (new_scroll_x, new_scroll_y));
            strata_dom::hooks::set_live_scroll_offset(id, new_scroll_x, new_scroll_y);
        }
        self.scroll_offsets
            .insert(path.to_vec(), (new_scroll_x, new_scroll_y));
        self.applied_scroll_offsets
            .insert(path.to_vec(), (new_scroll_x, new_scroll_y));

        let mut client_w = 0.0_f32;
        let mut client_h = 0.0_f32;
        let mut content_h = 0.0_f32;
        if let Some(target) = Self::get_layout_node_mut(&mut self.layout_root, path) {
            client_w = target.geometry.content_box.width;
            client_h = target.geometry.content_box.height;
            let target_fixed_cb = target.style.establishes_fixed_containing_block();
            let target_abs_cb =
                target.style.position != strata_style::types::Position::Static || target_fixed_cb;
            for child in &mut target.children {
                child.translate_scrolled(-dx, -dy, target_abs_cb, target_fixed_cb);
            }
            for line in &mut target.line_boxes {
                line.bounds.x -= dx;
                line.bounds.y -= dy;
                for frag in &mut line.text_fragments {
                    frag.bounds.x -= dx;
                    frag.bounds.y -= dy;
                    frag.baseline_y -= dy;
                }
                for atomic in &mut line.atomic_fragments {
                    atomic.bounds.x -= dx;
                    atomic.bounds.y -= dy;
                }
            }
            let (_, ch, _, _) =
                Self::compute_container_scroll_extents(target, new_scroll_x, new_scroll_y);
            content_h = ch;
        }
        let has_descendant_scrollbar = self
            .active_scrollbars
            .iter()
            .any(|b| b.path.len() > path.len() && b.path.starts_with(path));
        if self.has_anchored_portals {
            self.resolve_top_layer_portals();
        }
        if self.has_anchored_portals || has_descendant_scrollbar {
            self.rebuild_active_scrollbars();
        } else {
            for bar in &mut self.active_scrollbars {
                let matches = if is_id && !id.is_empty() && bar.is_id {
                    bar.id == id
                } else {
                    bar.path == path
                };
                if matches {
                    let clamped_y = new_scroll_y.clamp(0.0, bar.max_scroll_y);
                    bar.scroll_y = clamped_y;
                    bar.thumb_bounds.y = if bar.max_scroll_y > 0.0 && bar.available_travel > 0.0 {
                        bar.track_bounds.y + (clamped_y / bar.max_scroll_y) * bar.available_travel
                    } else {
                        bar.track_bounds.y
                    };
                }
            }
        }
        if self.has_anchored_portals {
            self.spatial_index.sync_bounds(&self.layout_root);
        } else {
            self.spatial_index
                .sync_bounds_for_prefix(&self.layout_root, path);
        }
        self.fire_scroll_event(
            path,
            new_scroll_x,
            new_scroll_y,
            dx,
            dy,
            client_w,
            content_h,
            client_w,
            client_h,
        );
    }

    /// Helper translating children and line fragments in an overflowing container by `-dy` and recording offsets.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_scroll_delta_to_container(
        &mut self,
        path: &[usize],
        is_id: bool,
        id: &str,
        new_scroll_y: f32,
        dy: f32,
        _max_scroll_y: f32,
        _available_travel: f32,
        _bounds_y: f32,
    ) {
        let cur_x = if is_id && !id.is_empty() {
            self.scroll_offsets_by_id
                .get(id)
                .map(|(x, _)| *x)
                .unwrap_or_else(|| self.scroll_offsets.get(path).map(|(x, _)| *x).unwrap_or(0.0))
        } else {
            self.scroll_offsets
                .get(path)
                .map(|(x, _)| *x)
                .unwrap_or(0.0)
        };
        self.apply_scroll_xy_delta_to_container(path, is_id, id, cur_x, new_scroll_y, 0.0, dy);
    }

    pub(crate) fn fire_scroll_event(
        &self,
        container_path: &[usize],
        scroll_x: f32,
        scroll_y: f32,
        delta_x: f32,
        delta_y: f32,
        content_w: f32,
        content_h: f32,
        client_w: f32,
        client_h: f32,
    ) -> bool {
        if let Some((handler, target_box, _)) = Self::find_event_at_or_above_path(
            &self.root_blueprint,
            &self.layout_root,
            container_path,
            strata_dom::EventType::Scroll,
        ) {
            let mut ev = strata_dom::Event::new(strata_dom::EventType::Scroll);
            ev.scroll_left = scroll_x as f64;
            ev.scroll_top = scroll_y as f64;
            ev.scroll_width = content_w as f64;
            ev.scroll_height = content_h as f64;
            ev.client_width = client_w as f64;
            ev.client_height = client_h as f64;
            ev.delta_x = delta_x as f64;
            ev.delta_y = delta_y as f64;
            ev.width = target_box.width as f64;
            ev.height = target_box.height as f64;
            ev.value = format!("{:.1}", scroll_y);
            handler(ev);
            return true;
        }
        false
    }

    fn blueprint_has_wheel_listener(bp: &NodeBlueprint) -> bool {
        match bp {
            NodeBlueprint::Element {
                attributes,
                children,
                ..
            } => {
                attributes.iter().any(|a| {
                    matches!(
                        a,
                        strata_dom::Attribute::EventListener {
                            event: strata_dom::EventType::Wheel,
                            ..
                        }
                    )
                }) || children.iter().any(Self::blueprint_has_wheel_listener)
            }
            NodeBlueprint::Fragment(children) => {
                children.iter().any(Self::blueprint_has_wheel_listener)
            }
            _ => false,
        }
    }

    fn dispatch_dom_wheel_event(
        &mut self,
        px: f32,
        py: f32,
        delta_x: f32,
        delta_y: f32,
    ) -> (bool, bool) {
        if !Self::blueprint_has_wheel_listener(&self.root_blueprint) {
            return (false, false);
        }
        self.dispatch_bubbling_event_at(
            strata_dom::EventType::Wheel,
            px,
            py,
            0,
            0,
            |ev| {
                ev.delta_x = delta_x as f64;
                ev.delta_y = delta_y as f64;
            },
        )
    }

    /// Dispatches an immediate synchronous scroll wheel delta to the innermost scrollable container under (px, py).
    /// Cancels any active kinetic smooth scroll animation.
    /// Returns true if a scroll offset changed or a DOM wheel listener handled the event.
    pub fn dispatch_scroll(&mut self, px: f32, py: f32, delta_x: f32, delta_y: f32) -> bool {
        self.last_mouse_pos = (px, py);
        self.active_smooth_scroll = None;

        let (wheel_handled, default_prevented) =
            self.dispatch_dom_wheel_event(px, py, delta_x, delta_y);
        if default_prevented {
            return wheel_handled;
        }

        let matching_bars = self.matching_scrollbars_at(px, py);

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

        if delta_x.abs() >= 0.01 && self.dispatch_horizontal_wheel_at(px, py, delta_x) {
            return true;
        }

        wheel_handled
    }

    fn collect_horizontal_scroll_paths_at(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        px: f32,
        py: f32,
        current_path: &mut Vec<usize>,
        inherited_steps: &[super::hit_test::HitClipStep],
        in_top_layer: bool,
        out: &mut Vec<(Vec<usize>, Option<String>, f32, f32, bool)>,
    ) {
        if current_path.len() >= 256
            || matches!(layout.style.display, strata_style::types::Display::None)
        {
            return;
        }
        let node_in_top_layer = in_top_layer || layout.is_top_layer;
        let eff_inherited = if layout.is_top_layer {
            &[][..]
        } else {
            Self::effective_hit_clip_steps(layout, inherited_steps)
        };
        let is_scroll_x = matches!(
            layout.style.overflow_x,
            strata_style::types::Overflow::Auto | strata_style::types::Overflow::Scroll
        );
        if is_scroll_x
            && layout.geometry.border_box.contains_point(px, py)
            && Self::hit_clip_steps_allow(eff_inherited, px, py)
        {
            let elem_id = layout.element_id.clone().or_else(|| {
                if let NodeBlueprint::Element { attributes, .. } = blueprint {
                    attributes.iter().find_map(|a| {
                        if let strata_dom::Attribute::Property { key, value } = a {
                            if key == "id" {
                                Some(value.clone())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                } else {
                    None
                }
            });
            out.push((
                current_path.clone(),
                elem_id,
                0.0,
                0.0,
                node_in_top_layer,
            ));
        }

        let mut next_steps = Vec::with_capacity(eff_inherited.len() + 1);
        next_steps.extend_from_slice(eff_inherited);
        let own_step = Self::hit_clip_step_for(layout);
        if own_step.clip.is_some() || own_step.is_abs_cb || own_step.is_fixed_cb {
            next_steps.push(own_step);
        }

        for (i, child_bp, child_layout) in Self::paired_blueprint_children(blueprint, layout) {
            current_path.push(i);
            Self::collect_horizontal_scroll_paths_at(
                child_bp,
                child_layout,
                px,
                py,
                current_path,
                &next_steps,
                node_in_top_layer,
                out,
            );
            current_path.pop();
        }
    }

    fn dispatch_horizontal_wheel_at(&mut self, px: f32, py: f32, delta_x: f32) -> bool {
        let mut hits = Vec::new();
        let mut path_buf = Vec::new();
        Self::collect_horizontal_scroll_paths_at(
            &self.root_blueprint,
            &self.layout_root,
            px,
            py,
            &mut path_buf,
            &[],
            false,
            &mut hits,
        );
        hits.sort_by_key(|h| h.4);
        for (path, elem_id, _, _, _) in hits.into_iter().rev() {
            let (cur_x, cur_y) = if let Some(ref id) = elem_id {
                self.scroll_offsets_by_id
                    .get(id)
                    .copied()
                    .or_else(|| self.scroll_offsets.get(&path).copied())
                    .unwrap_or((0.0, 0.0))
            } else {
                self.scroll_offsets.get(&path).copied().unwrap_or((0.0, 0.0))
            };
            let (applied_x, applied_y) = self
                .applied_scroll_offsets
                .get(&path)
                .copied()
                .unwrap_or((0.0, 0.0));
            let Some(node) = Self::get_layout_node(&self.layout_root, &path) else {
                continue;
            };
            let (_, _, max_scroll_x, _) =
                Self::compute_container_scroll_extents(node, applied_x, applied_y);
            if max_scroll_x <= 0.0 {
                continue;
            }
            let target_x = (cur_x + delta_x).clamp(0.0, max_scroll_x);
            let dx = target_x - applied_x;
            if dx.abs() > 0.01 {
                self.last_scroll_interaction = Some(std::time::Instant::now());
                let is_id = elem_id.is_some();
                let id_str = elem_id.unwrap_or_default();
                self.apply_scroll_xy_delta_to_container(
                    &path,
                    is_id,
                    &id_str,
                    target_x,
                    cur_y,
                    dx,
                    0.0,
                );
                return true;
            }
        }
        false
    }

    /// Dispatches a smooth kinetic scroll impulse to the innermost scrollable container under (px, py).
    /// Accumulates velocity for rapid consecutive wheel events and initiates a critically damped
    /// kinetic transition curve with Chromium Blink parity.
    /// Dispatches a smooth scroll or DOM wheel event, returning `(wheel_handled, scroll_changed)`.
    pub fn dispatch_smooth_scroll_action(
        &mut self,
        px: f32,
        py: f32,
        delta_x: f32,
        delta_y: f32,
    ) -> (bool, bool) {
        self.last_mouse_pos = (px, py);
        if delta_y.abs() < 0.01 && delta_x.abs() < 0.01 {
            return (false, false);
        }

        let (wheel_handled, default_prevented) =
            self.dispatch_dom_wheel_event(px, py, delta_x, delta_y);
        if default_prevented {
            return (wheel_handled, false);
        }
        if delta_y.abs() < 0.01 {
            if delta_x.abs() >= 0.01 && self.dispatch_horizontal_wheel_at(px, py, delta_x) {
                return (wheel_handled, true);
            }
            return (wheel_handled, false);
        }

        let matching_bars = self.matching_scrollbars_at(px, py);
        if matching_bars.is_empty() && delta_x.abs() >= 0.01 {
            let horiz_changed = self.dispatch_horizontal_wheel_at(px, py, delta_x);
            return (wheel_handled, horiz_changed);
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
            return (wheel_handled, false);
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

        if self
            .active_smooth_scroll
            .as_ref()
            .is_some_and(|t| t.container_path == bar.path)
        {
            if let Some(mut tracker) = self.active_smooth_scroll.take() {
                let dt_flick = now.duration_since(tracker.last_event_time).as_secs_f32();
                let remaining = tracker.target_y - tracker.current_y;
                let same_dir = remaining == 0.0 || (remaining > 0.0) == (delta_y > 0.0);
                // Continuous touchpad / high-resolution streams emit <48px packets or arrive on a
                // 2-18ms (60-500Hz) HID cadence; only discrete wheel flicks receive momentum.
                let is_continuous_hid_cadence = dt_flick >= 0.002 && dt_flick < 0.018;
                let is_discrete_flick =
                    same_dir && delta_y.abs() >= 48.0 && !is_continuous_hid_cadence;
                let accel = if is_discrete_flick && dt_flick < 0.20 {
                    (1.0 + (0.05 / dt_flick.max(0.02)).min(1.3)).min(2.3)
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

                let catchup_dy = y_now - tracker.current_y;
                if catchup_dy.abs() > 0.001 {
                    self.apply_scroll_delta_to_container(
                        &tracker.container_path,
                        tracker.is_id,
                        &tracker.container_id,
                        y_now,
                        catchup_dy,
                        bar.max_scroll_y,
                        bar.available_travel,
                        bar.bounds.y,
                    );
                    tracker.current_y = y_now;
                }

                let impulse = delta_y * accel;
                let new_target = (tracker.target_y + impulse).clamp(0.0, bar.max_scroll_y);

                tracker.start_y = y_now;
                tracker.initial_velocity = v_now;
                tracker.target_y = new_target;
                tracker.start_time = now;
                tracker.last_event_time = now;
                tracker.max_scroll_y = bar.max_scroll_y;
                tracker.available_travel = bar.available_travel;
                tracker.bounds_y = bar.bounds.y;
                self.active_smooth_scroll = Some(tracker);
                return (wheel_handled, true);
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

        (wheel_handled, true)
    }

    /// Dispatches a smooth kinetic scroll impulse to the innermost scrollable container under (px, py).
    /// Returns `true` if either a DOM `wheel` listener handled the event or a scroll animation updated.
    pub fn dispatch_smooth_scroll(&mut self, px: f32, py: f32, delta_x: f32, delta_y: f32) -> bool {
        let (wheel_handled, scroll_changed) =
            self.dispatch_smooth_scroll_action(px, py, delta_x, delta_y);
        wheel_handled || scroll_changed
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
        let matching_bars = self.matching_scrollbars_at(px, py);

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
        let matching_bars = self.matching_scrollbars_at(px, py);

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

    /// Returns the live `(scroll_left, scroll_top)` offset for a scrollable element `id`.
    pub fn scroll_offset_by_id(&self, id: &str) -> (f32, f32) {
        if let Some(off) = self.scroll_offsets_by_id.get(id) {
            return *off;
        }
        if let Some(bar) = self
            .active_scrollbars
            .iter()
            .find(|b| b.is_id && b.id == id)
        {
            return (0.0, bar.scroll_y);
        }
        strata_dom::get_scroll_offset(id)
    }

    /// Scrolls the scroll container with element `id` (or the primary scroll container if `id` is empty)
    /// to `(scroll_x, scroll_y)`, clamping to the container's maximum scroll bounds and firing `EventType::Scroll`.
    pub fn scroll_to_by_id(&mut self, id: &str, scroll_x: f32, scroll_y: f32) -> bool {
        self.active_smooth_scroll = None;
        let target_path = if id.is_empty() {
            self.active_scrollbars
                .iter()
                .max_by(|a, b| {
                    (a.bounds.width * a.bounds.height)
                        .partial_cmp(&(b.bounds.width * b.bounds.height))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|b| b.path.clone())
        } else {
            self.active_scrollbars
                .iter()
                .find(|b| b.is_id && b.id == id)
                .map(|b| b.path.clone())
                .or_else(|| {
                    let mut buf = Vec::new();
                    Self::find_element_path_by_id(&self.root_blueprint, &self.layout_root, id, &mut buf)
                })
        };

        if let Some(path) = target_path {
            let (applied_x, applied_y) = self
                .applied_scroll_offsets
                .get(&path)
                .copied()
                .unwrap_or((0.0, 0.0));
            if let Some(node) = Self::get_layout_node(&self.layout_root, &path) {
                let is_scroll_x = matches!(
                    node.style.overflow_x,
                    strata_style::types::Overflow::Auto | strata_style::types::Overflow::Scroll
                );
                let is_scroll_y = matches!(
                    node.style.overflow_y,
                    strata_style::types::Overflow::Auto | strata_style::types::Overflow::Scroll
                );
                if is_scroll_x || is_scroll_y {
                    let (_, _, max_scroll_x, max_scroll_y) =
                        Self::compute_container_scroll_extents(node, applied_x, applied_y);
                    let clamped_x = if is_scroll_x {
                        scroll_x.clamp(0.0, max_scroll_x)
                    } else {
                        0.0
                    };
                    let clamped_y = if is_scroll_y {
                        scroll_y.clamp(0.0, max_scroll_y)
                    } else {
                        0.0
                    };
                    let dx = clamped_x - applied_x;
                    let dy = clamped_y - applied_y;
                    let is_id = !id.is_empty();
                    if is_id {
                        self.scroll_offsets_by_id
                            .insert(id.to_string(), (clamped_x, clamped_y));
                        strata_dom::hooks::set_live_scroll_offset(id, clamped_x, clamped_y);
                    }
                    if dx.abs() > 0.01 || dy.abs() > 0.01 {
                        self.last_scroll_interaction = Some(std::time::Instant::now());
                        self.apply_scroll_xy_delta_to_container(
                            &path,
                            is_id,
                            id,
                            clamped_x,
                            clamped_y,
                            dx,
                            dy,
                        );
                        return true;
                    }
                    return false;
                }
            }
        }

        if !id.is_empty() {
            let clamped_x = scroll_x.max(0.0);
            let clamped_y = scroll_y.max(0.0);
            self.scroll_offsets_by_id
                .insert(id.to_string(), (clamped_x, clamped_y));
            strata_dom::hooks::set_live_scroll_offset(id, clamped_x, clamped_y);
        }
        false
    }

    /// Scrolls the scroll container with element `id` (or the primary scroll container if `id` is empty) to the top.
    pub fn scroll_to_top_by_id(&mut self, id: &str) -> bool {
        if id.is_empty() {
            let (px, py) = self.last_mouse_pos;
            return self.scroll_to_top(px, py);
        }
        self.scroll_to_by_id(id, 0.0, 0.0)
    }

    /// Scrolls the scroll container with element `id` (or the primary scroll container if `id` is empty) to the bottom.
    pub fn scroll_to_bottom_by_id(&mut self, id: &str) -> bool {
        if id.is_empty() {
            let (px, py) = self.last_mouse_pos;
            return self.scroll_to_bottom(px, py);
        }
        if let Some(bar) = self
            .active_scrollbars
            .iter()
            .find(|b| b.is_id && b.id == id)
            .cloned()
        {
            return self.scroll_to_by_id(id, 0.0, bar.max_scroll_y);
        }
        false
    }

    pub(crate) fn find_element_path_by_id(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        target_id: &str,
        current_path: &mut Vec<usize>,
    ) -> Option<Vec<usize>> {
        if layout.element_id.as_deref() == Some(target_id) {
            return Some(current_path.clone());
        }
        if let NodeBlueprint::Element { attributes, .. } = blueprint {
            for attr in attributes {
                if let strata_dom::Attribute::Property { key, value } = attr {
                    if key == "id" && value == target_id {
                        return Some(current_path.clone());
                    }
                }
            }
        }
        for (idx, child_bp, child_layout) in Self::paired_blueprint_children(blueprint, layout) {
            current_path.push(idx);
            if let Some(found) =
                Self::find_element_path_by_id(child_bp, child_layout, target_id, current_path)
            {
                return Some(found);
            }
            current_path.pop();
        }
        None
    }

    /// Scrolls the nearest enclosing scroll container so that the descendant element with `target_id` is visible.
    pub fn scroll_into_view(&mut self, target_id: &str) -> bool {
        if target_id.is_empty() {
            return false;
        }
        let mut path_buf = Vec::new();
        let Some(target_path) = Self::find_element_path_by_id(
            &self.root_blueprint,
            &self.layout_root,
            target_id,
            &mut path_buf,
        ) else {
            return false;
        };

        let Some(target_box) =
            Self::get_layout_node(&self.layout_root, &target_path).map(|n| n.geometry.border_box)
        else {
            return false;
        };

        // Find the deepest active scrollbar that is a strict ancestor of `target_path`
        let ancestor_bar = self
            .active_scrollbars
            .iter()
            .filter(|b| target_path.len() > b.path.len() && target_path.starts_with(&b.path))
            .max_by_key(|b| b.path.len())
            .cloned();

        let Some(bar) = ancestor_bar else {
            return false;
        };

        let container_box = Self::get_layout_node(&self.layout_root, &bar.path)
            .map(|n| n.geometry.content_box)
            .unwrap_or(bar.bounds);

        let cur_y = bar.scroll_y;
        let new_y = if target_box.y < container_box.y {
            let dy = target_box.y - container_box.y;
            (cur_y + dy).clamp(0.0, bar.max_scroll_y)
        } else if target_box.y + target_box.height > container_box.y + container_box.height {
            let dy = (target_box.y + target_box.height) - (container_box.y + container_box.height);
            (cur_y + dy).clamp(0.0, bar.max_scroll_y)
        } else {
            cur_y
        };

        let dy = new_y - cur_y;
        if dy.abs() > 0.01 {
            self.active_smooth_scroll = None;
            self.last_scroll_interaction = Some(std::time::Instant::now());
            self.apply_scroll_delta_to_container(
                &bar.path,
                bar.is_id,
                &bar.id,
                new_y,
                dy,
                bar.max_scroll_y,
                bar.available_travel,
                bar.bounds.y,
            );
            return true;
        }
        false
    }

    /// Drains and executes any programmatic scroll commands queued via `strata_dom::scroll_to`,
    /// `scroll_to_top`, `scroll_to_bottom`, or `scroll_into_view`.
    pub fn flush_pending_scroll_commands(&mut self) -> bool {
        if !strata_dom::hooks::has_pending_scroll_commands() {
            return false;
        }
        let cmds = strata_dom::hooks::take_pending_scroll_commands();
        let mut any_changed = false;
        for cmd in cmds {
            match cmd {
                strata_dom::ProgrammaticScrollCommand::ScrollTo { id, x, y } => {
                    if self.scroll_to_by_id(&id, x, y) {
                        any_changed = true;
                    }
                }
                strata_dom::ProgrammaticScrollCommand::ScrollToTop { id } => {
                    if self.scroll_to_top_by_id(&id) {
                        any_changed = true;
                    }
                }
                strata_dom::ProgrammaticScrollCommand::ScrollToBottom { id } => {
                    if self.scroll_to_bottom_by_id(&id) {
                        any_changed = true;
                    }
                }
                strata_dom::ProgrammaticScrollCommand::ScrollIntoView { target_id } => {
                    if self.scroll_into_view(&target_id) {
                        any_changed = true;
                    }
                }
            }
        }
        any_changed
    }
}
