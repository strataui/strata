//! Spatial hit-testing for pointer events, clicks, cursors, and CSS properties.

use super::*;

#[derive(Clone, Copy, Debug)]
pub(crate) struct HitClipStep {
    pub clip: Option<strata_layout::Rect>,
    pub is_abs_cb: bool,
    pub is_fixed_cb: bool,
}

impl AppRuntime {
    /// Unprojects a world pointer coordinate into an element's local coordinate system if transformed.
    #[inline]
    pub fn unproject_point(layout: &LayoutNode, px: f32, py: f32) -> (f32, f32) {
        if layout.style.has_transform() {
            let mat = layout.style.compute_transform_matrix(
                layout.geometry.border_box.width,
                layout.geometry.border_box.height,
            );
            if let Some(inv) = mat.inverse() {
                let lx = px - layout.geometry.border_box.x;
                let ly = py - layout.geometry.border_box.y;
                let (un_lx, un_ly) = inv.transform_point(lx, ly);
                (
                    layout.geometry.border_box.x + un_lx,
                    layout.geometry.border_box.y + un_ly,
                )
            } else {
                (px, py)
            }
        } else {
            (px, py)
        }
    }

    /// Ancestor clip step tracked along the DOM path so `position: fixed` and `position: absolute`
    /// elements escape clips from ancestors that do not contain their CSS containing block.
    #[inline]
    pub(crate) fn hit_clip_step_for(layout: &LayoutNode) -> HitClipStep {
        let is_fixed_cb = layout.style.establishes_fixed_containing_block();
        let is_abs_cb =
            layout.style.position != strata_style::types::Position::Static || is_fixed_cb;
        let clips = layout.style.overflow_x != strata_style::types::Overflow::Visible
            || layout.style.overflow_y != strata_style::types::Overflow::Visible;
        HitClipStep {
            clip: if clips {
                Some(layout.geometry.border_box)
            } else {
                None
            },
            is_abs_cb,
            is_fixed_cb,
        }
    }

    #[inline]
    pub(crate) fn effective_hit_clip_steps<'a>(
        node: &LayoutNode,
        steps: &'a [HitClipStep],
    ) -> &'a [HitClipStep] {
        match node.style.position {
            strata_style::types::Position::Fixed => {
                if let Some(last_idx) = steps.iter().rposition(|s| s.is_fixed_cb) {
                    &steps[..=last_idx]
                } else {
                    &[]
                }
            }
            strata_style::types::Position::Absolute => {
                if let Some(last_idx) = steps.iter().rposition(|s| s.is_abs_cb) {
                    &steps[..=last_idx]
                } else {
                    &[]
                }
            }
            _ => steps,
        }
    }

    #[inline]
    pub(crate) fn hit_clip_steps_allow(steps: &[HitClipStep], px: f32, py: f32) -> bool {
        steps
            .iter()
            .all(|s| s.clip.is_none_or(|r| r.contains_point(px, py)))
    }

    /// Returns true if `node` establishes an atomic stacking context for paint and hit-testing.
    #[inline]
    pub fn establishes_stacking_context(node: &LayoutNode, is_root: bool) -> bool {
        node.establishes_stacking_context_in(None, is_root)
    }

    /// Returns true if `node` establishes an atomic stacking context given `parent_display`.
    #[inline]
    pub fn establishes_stacking_context_in(
        node: &LayoutNode,
        parent_display: Option<strata_style::types::Display>,
        is_root: bool,
    ) -> bool {
        node.establishes_stacking_context_in(parent_display, is_root)
    }

    /// Pairs flattened blueprint children with `node_layout.children`, preserving flattened child indices
    /// and resolving keyed `EntityId` children when reordered.
    pub(crate) fn paired_blueprint_children<'a>(
        node_bp: &'a NodeBlueprint,
        node_layout: &'a LayoutNode,
    ) -> Vec<(usize, &'a NodeBlueprint, &'a LayoutNode)> {
        let mut flat_children = Vec::new();
        match node_bp {
            NodeBlueprint::Element { children, .. } | NodeBlueprint::Fragment(children) => {
                for c in children {
                    Self::flatten_blueprints(c, &mut flat_children);
                }
            }
            _ => {}
        }

        let mut out = Vec::with_capacity(node_layout.children.len());
        for (idx, child_layout) in node_layout.children.iter().enumerate() {
            let resolved_bp = if child_layout.entity_id.is_valid() {
                if let Some(&cand) = flat_children.get(idx) {
                    if cand.entity_id() == Some(child_layout.entity_id) {
                        Some(cand)
                    } else {
                        node_bp
                            .find_by_entity_id(child_layout.entity_id)
                            .or(Some(cand))
                    }
                } else {
                    node_bp.find_by_entity_id(child_layout.entity_id)
                }
            } else {
                flat_children.get(idx).copied()
            };

            if let Some(child_bp) = resolved_bp {
                out.push((idx, child_bp, child_layout));
            }
        }
        out
    }

    fn collect_top_layer_portals<'a>(
        bp: &'a NodeBlueprint,
        layout: &'a LayoutNode,
        out: &mut Vec<(&'a NodeBlueprint, &'a LayoutNode)>,
        depth: usize,
    ) {
        if depth >= 256 {
            return;
        }
        for (_idx, child_bp, child_layout) in Self::paired_blueprint_children(bp, layout) {
            if child_layout.style.display == strata_style::types::Display::None {
                continue;
            }
            if child_layout.is_top_layer {
                out.push((child_bp, child_layout));
            }
            if !child_layout.children.is_empty() {
                Self::collect_top_layer_portals(child_bp, child_layout, out, depth + 1);
            }
        }
    }

    /// Resolves the root-to-target `(NodeBlueprint, LayoutNode)` ancestor chain along `path`,
    /// using `paired_blueprint_children` so keyed `EntityId` reorders resolve identically to hit-testing.
    pub(crate) fn resolve_chain_for_path<'a>(
        blueprint: &'a NodeBlueprint,
        layout: &'a LayoutNode,
        path: &[usize],
    ) -> Vec<(&'a NodeBlueprint, &'a LayoutNode)> {
        let mut chain = Vec::with_capacity(path.len() + 1);
        let mut cur_bp = blueprint;
        let mut cur_layout = layout;
        chain.push((cur_bp, cur_layout));

        for &idx in path {
            let paired = Self::paired_blueprint_children(cur_bp, cur_layout);
            let Some((_, next_bp, next_layout)) =
                paired.into_iter().find(|(child_idx, _, _)| *child_idx == idx)
            else {
                break;
            };
            cur_bp = next_bp;
            cur_layout = next_layout;
            chain.push((cur_bp, cur_layout));
        }
        chain
    }

    pub(crate) fn is_portal_backdrop_bp(bp: &NodeBlueprint) -> bool {
        if let NodeBlueprint::Element { attributes, .. } = bp {
            attributes.iter().any(|a| {
                matches!(
                    a,
                    strata_dom::Attribute::Property { key, value }
                        if key == "data-portal-backdrop" && value == "true"
                )
            })
        } else {
            false
        }
    }

    pub(crate) fn top_layer_portal_node_is_visual_content(
        bp: &NodeBlueprint,
        layout: &LayoutNode,
        px: f32,
        py: f32,
        depth: usize,
    ) -> bool {
        if depth >= 256
            || layout.style.display == strata_style::types::Display::None
            || layout.style.visibility == strata_style::types::Visibility::Hidden
            || layout.style.pointer_events == strata_style::types::PointerEvents::None
        {
            return false;
        }
        if Self::is_portal_backdrop_bp(bp) {
            return false;
        }
        let (px, py) = Self::unproject_point(layout, px, py);
        let in_box = layout.geometry.border_box.contains_point(px, py);
        if in_box {
            for (_idx, child_bp, child_layout) in Self::paired_blueprint_children(bp, layout) {
                if Self::top_layer_portal_node_is_visual_content(
                    child_bp,
                    child_layout,
                    px,
                    py,
                    depth + 1,
                ) {
                    return true;
                }
            }
            if depth > 0
                || layout.style.background_color.is_some_and(|c| c.a > 0)
                || !layout.style.box_shadow.is_empty()
            {
                return true;
            }
        } else {
            for (_idx, child_bp, child_layout) in Self::paired_blueprint_children(bp, layout) {
                if Self::top_layer_portal_node_is_visual_content(
                    child_bp,
                    child_layout,
                    px,
                    py,
                    depth + 1,
                ) {
                    return true;
                }
            }
        }
        false
    }

    pub(crate) fn point_hits_non_backdrop_top_layer_portal(&self, px: f32, py: f32) -> bool {
        let mut top_layer_portals = Vec::new();
        Self::collect_top_layer_portals(
            &self.root_blueprint,
            &self.layout_root,
            &mut top_layer_portals,
            0,
        );
        top_layer_portals.into_iter().rev().any(|(p_bp, p_layout)| {
            !Self::is_portal_backdrop_bp(p_bp)
                && Self::top_layer_portal_node_is_visual_content(p_bp, p_layout, px, py, 0)
        })
    }

    /// Hit tests `(px, py)` matching W3C DOM semantics: first resolves the single front-most
    /// visual `pointer-events != none` target path, then walks strictly up its DOM ancestor chain
    /// to find the nearest listener registered for `event_type`, its border box, and its `EntityId`.
    pub fn hit_test_event_with_box_and_entity<'a>(
        blueprint: &'a NodeBlueprint,
        layout: &'a LayoutNode,
        px: f32,
        py: f32,
        event_type: strata_dom::EventType,
    ) -> Option<(
        std::rc::Rc<dyn Fn(strata_dom::Event)>,
        &'a strata_layout::Rect,
        strata_primitives::EntityId,
    )> {
        let visual_path = Self::hit_test_node_path(blueprint, layout, px, py)?;
        Self::find_event_at_or_above_path(blueprint, layout, &visual_path, event_type)
    }

    /// Recursively hit tests a coordinate point against the blueprint and computed layout hierarchy,
    /// returning the nearest listener registered for `event_type` and its element border box.
    pub fn hit_test_event_with_box<'a>(
        blueprint: &'a NodeBlueprint,
        layout: &'a LayoutNode,
        px: f32,
        py: f32,
        event_type: strata_dom::EventType,
    ) -> Option<(
        std::rc::Rc<dyn Fn(strata_dom::Event)>,
        &'a strata_layout::Rect,
    )> {
        Self::hit_test_event_with_box_and_entity(blueprint, layout, px, py, event_type)
            .map(|(h, r, _)| (h, r))
    }

    /// Walks from `path` up toward the root to find the nearest ancestor (or self)
    /// registering a listener for `event_type`, returning `(handler, border_box, entity_id)`.
    pub fn find_event_at_or_above_path<'a>(
        blueprint: &'a NodeBlueprint,
        layout: &'a LayoutNode,
        path: &[usize],
        event_type: strata_dom::EventType,
    ) -> Option<(
        std::rc::Rc<dyn Fn(strata_dom::Event)>,
        &'a strata_layout::Rect,
        strata_primitives::EntityId,
    )> {
        let chain = Self::resolve_chain_for_path(blueprint, layout, path);
        for (bp, node_layout) in chain.into_iter().rev() {
            if let NodeBlueprint::Element { attributes, .. } = bp {
                for attr in attributes {
                    if let strata_dom::Attribute::EventListener { event: ev, handler } = attr {
                        if *ev == event_type {
                            return Some((
                                handler.clone(),
                                &node_layout.geometry.border_box,
                                node_layout.entity_id,
                            ));
                        }
                    }
                }
            }
        }
        None
    }

    /// Recursively hit tests a coordinate point against the blueprint and computed layout hierarchy,
    /// returning the nearest (deepest, front-most) listener registered for `event_type`.
    pub fn hit_test_event(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        px: f32,
        py: f32,
        event_type: strata_dom::EventType,
    ) -> Option<std::rc::Rc<dyn Fn(strata_dom::Event)>> {
        Self::hit_test_event_with_box(blueprint, layout, px, py, event_type).map(|(h, _)| h)
    }

    /// Hit tests for the nearest `onclick` listener at a coordinate point.
    pub fn hit_test_click(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        px: f32,
        py: f32,
    ) -> Option<std::rc::Rc<dyn Fn(strata_dom::Event)>> {
        Self::hit_test_event(blueprint, layout, px, py, strata_dom::EventType::Click)
    }

    /// Hit tests `(px, py)` against the front-most visual target and its DOM ancestors
    /// to determine cursor style without piercing through opaque higher-z overlays.
    pub fn hit_test_cursor(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        px: f32,
        py: f32,
    ) -> strata_platform::window::CursorIcon {
        let Some(visual_path) = Self::hit_test_node_path(blueprint, layout, px, py) else {
            return strata_platform::window::CursorIcon::Default;
        };
        let chain = Self::resolve_chain_for_path(blueprint, layout, &visual_path);
        for (bp, node_layout) in chain.into_iter().rev() {
            if let NodeBlueprint::Element {
                tag,
                classes,
                attributes,
                ..
            } = bp
            {
                match node_layout.style.cursor {
                    strata_style::types::Cursor::Pointer => {
                        return strata_platform::window::CursorIcon::Pointer
                    }
                    strata_style::types::Cursor::Text => {
                        return strata_platform::window::CursorIcon::Text
                    }
                    strata_style::types::Cursor::Crosshair => {
                        return strata_platform::window::CursorIcon::Crosshair
                    }
                    strata_style::types::Cursor::Move => {
                        return strata_platform::window::CursorIcon::Move
                    }
                    strata_style::types::Cursor::NotAllowed => {
                        return strata_platform::window::CursorIcon::NotAllowed
                    }
                    strata_style::types::Cursor::Wait => {
                        return strata_platform::window::CursorIcon::Wait
                    }
                    strata_style::types::Cursor::Grab => {
                        return strata_platform::window::CursorIcon::Grab
                    }
                    strata_style::types::Cursor::Grabbing => {
                        return strata_platform::window::CursorIcon::Grabbing
                    }
                    strata_style::types::Cursor::ColResize => {
                        return strata_platform::window::CursorIcon::ColResize
                    }
                    strata_style::types::Cursor::RowResize => {
                        return strata_platform::window::CursorIcon::RowResize
                    }
                    strata_style::types::Cursor::Help => {
                        return strata_platform::window::CursorIcon::Help
                    }
                    strata_style::types::Cursor::None => {
                        return strata_platform::window::CursorIcon::None
                    }
                    strata_style::types::Cursor::Default => {}
                }

                let has_pointer = matches!(tag, Tag::Button | Tag::A)
                    || classes.iter().any(|c| {
                        c.split_whitespace()
                            .any(|tok| tok == "cursor-pointer" || tok == "pointer")
                    })
                    || attributes.iter().any(|a| {
                        matches!(
                            a,
                            strata_dom::Attribute::EventListener {
                                event: strata_dom::EventType::Click,
                                ..
                            }
                        )
                    });

                if has_pointer {
                    return strata_platform::window::CursorIcon::Pointer;
                }

                let has_text_cursor = matches!(tag, Tag::Input | Tag::Textarea)
                    || classes.iter().any(|c| {
                        c.split_whitespace()
                            .any(|tok| tok == "cursor-text" || tok == "select-text")
                    });

                if has_text_cursor {
                    return strata_platform::window::CursorIcon::Text;
                }
            }
        }
        strata_platform::window::CursorIcon::Default
    }

    /// Finds the value of the nearest element property `key` along the front-most visual target
    /// and its DOM ancestors at `(px, py)` without piercing through opaque higher-z overlays.
    pub fn hit_test_property(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        px: f32,
        py: f32,
        key: &str,
    ) -> Option<String> {
        let visual_path = Self::hit_test_node_path(blueprint, layout, px, py)?;
        let chain = Self::resolve_chain_for_path(blueprint, layout, &visual_path);
        for (bp, _) in chain.into_iter().rev() {
            if let NodeBlueprint::Element { attributes, .. } = bp {
                for attr in attributes {
                    if let strata_dom::Attribute::Property { key: k, value } = attr {
                        if k == key {
                            return Some(value.clone());
                        }
                    }
                }
            }
        }
        None
    }
}
