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

    fn collect_hoisted_stacking_participants<'a>(
        bp: &'a NodeBlueprint,
        layout: &'a LayoutNode,
        steps: &[HitClipStep],
        negative_z: &mut Vec<(&'a NodeBlueprint, &'a LayoutNode, Vec<HitClipStep>)>,
        auto_or_zero_z: &mut Vec<(&'a NodeBlueprint, &'a LayoutNode, bool, Vec<HitClipStep>)>,
        positive_z: &mut Vec<(&'a NodeBlueprint, &'a LayoutNode, Vec<HitClipStep>)>,
        depth: usize,
    ) {
        if depth >= 256 {
            return;
        }
        let parent_display = Some(layout.style.display);
        for (_idx, child_bp, child_layout) in Self::paired_blueprint_children(bp, layout) {
            if child_layout.style.display == strata_style::types::Display::None
                || child_layout.is_top_layer
            {
                continue;
            }
            if Self::establishes_stacking_context_in(child_layout, parent_display, false) {
                let eff_steps = Self::effective_hit_clip_steps(child_layout, steps).to_vec();
                let z = child_layout.style.z_index.unwrap_or(0);
                if z < 0 {
                    negative_z.push((child_bp, child_layout, eff_steps));
                } else if z > 0 {
                    positive_z.push((child_bp, child_layout, eff_steps));
                } else {
                    auto_or_zero_z.push((child_bp, child_layout, true, eff_steps));
                }
            } else {
                let eff_for_child = Self::effective_hit_clip_steps(child_layout, steps);
                if child_layout.style.position != strata_style::types::Position::Static {
                    auto_or_zero_z.push((child_bp, child_layout, false, eff_for_child.to_vec()));
                }
                let mut child_steps = Vec::with_capacity(eff_for_child.len() + 1);
                child_steps.extend_from_slice(eff_for_child);
                let step = Self::hit_clip_step_for(child_layout);
                if step.clip.is_some() || step.is_abs_cb || step.is_fixed_cb {
                    child_steps.push(step);
                }
                Self::collect_hoisted_stacking_participants(
                    child_bp,
                    child_layout,
                    &child_steps,
                    negative_z,
                    auto_or_zero_z,
                    positive_z,
                    depth + 1,
                );
            }
        }
    }

    fn hit_test_direct_in_flow_children<'a, T, F>(
        bp: &'a NodeBlueprint,
        layout: &'a LayoutNode,
        px: f32,
        py: f32,
        depth: usize,
        test_self: &mut F,
    ) -> Option<T>
    where
        F: FnMut(&'a NodeBlueprint, &'a LayoutNode, f32, f32) -> Option<T>,
    {
        let paired = Self::paired_blueprint_children(bp, layout);
        if paired.is_empty() {
            return None;
        }

        let parent_display = Some(layout.style.display);
        let is_in_flow_non_sc = |c: &LayoutNode| {
            c.style.display != strata_style::types::Display::None
                && !c.is_top_layer
                && !Self::establishes_stacking_context_in(c, parent_display, false)
                && c.style.position == strata_style::types::Position::Static
        };
        let is_inline_level = |c: &LayoutNode| {
            matches!(
                c.style.display,
                strata_style::types::Display::Inline
                    | strata_style::types::Display::InlineBlock
                    | strata_style::types::Display::InlineFlex
                    | strata_style::types::Display::InlineGrid
            )
        };

        // Layer 5: In-flow inline-level descendants (front-to-back)
        for &(_idx, child_bp, child_layout) in paired.iter().rev() {
            if is_in_flow_non_sc(child_layout)
                && child_layout.style.float == strata_style::types::Float::None
                && is_inline_level(child_layout)
            {
                if let Some(hit) = Self::hit_test_in_flow_non_hoisted(
                    child_bp,
                    child_layout,
                    px,
                    py,
                    &[],
                    depth + 1,
                    test_self,
                ) {
                    return Some(hit);
                }
            }
        }

        // Layer 4: Non-positioned floats (front-to-back)
        for &(_idx, child_bp, child_layout) in paired.iter().rev() {
            if is_in_flow_non_sc(child_layout)
                && child_layout.style.float != strata_style::types::Float::None
            {
                if let Some(hit) = Self::hit_test_in_flow_non_hoisted(
                    child_bp,
                    child_layout,
                    px,
                    py,
                    &[],
                    depth + 1,
                    test_self,
                ) {
                    return Some(hit);
                }
            }
        }

        // Layer 3: In-flow block-level descendants (front-to-back)
        for &(_idx, child_bp, child_layout) in paired.iter().rev() {
            if is_in_flow_non_sc(child_layout)
                && child_layout.style.float == strata_style::types::Float::None
                && !is_inline_level(child_layout)
            {
                if let Some(hit) = Self::hit_test_in_flow_non_hoisted(
                    child_bp,
                    child_layout,
                    px,
                    py,
                    &[],
                    depth + 1,
                    test_self,
                ) {
                    return Some(hit);
                }
            }
        }

        None
    }

    fn hit_test_in_flow_non_hoisted<'a, T, F>(
        bp: &'a NodeBlueprint,
        layout: &'a LayoutNode,
        px: f32,
        py: f32,
        inherited_steps: &[HitClipStep],
        depth: usize,
        test_self: &mut F,
    ) -> Option<T>
    where
        F: FnMut(&'a NodeBlueprint, &'a LayoutNode, f32, f32) -> Option<T>,
    {
        if depth >= 256 || layout.style.display == strata_style::types::Display::None {
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
        if let Some(hit) =
            Self::hit_test_direct_in_flow_children(bp, layout, px, py, depth, test_self)
        {
            return Some(hit);
        }
        test_self(bp, layout, px, py)
    }

    fn hit_test_stacking_context_with<'a, T, F>(
        bp: &'a NodeBlueprint,
        layout: &'a LayoutNode,
        px: f32,
        py: f32,
        inherited_steps: &[HitClipStep],
        depth: usize,
        test_self: &mut F,
    ) -> Option<T>
    where
        F: FnMut(&'a NodeBlueprint, &'a LayoutNode, f32, f32) -> Option<T>,
    {
        if depth >= 256 || layout.style.display == strata_style::types::Display::None {
            return None;
        }

        if depth == 0 {
            let mut top_layer_portals = Vec::new();
            Self::collect_top_layer_portals(bp, layout, &mut top_layer_portals, 0);
            for &(p_bp, p_layout) in top_layer_portals.iter().rev() {
                if let Some(hit) =
                    Self::hit_test_stacking_context_with(p_bp, p_layout, px, py, &[], 1, test_self)
                {
                    return Some(hit);
                }
            }
        }

        let eff_inherited = Self::effective_hit_clip_steps(layout, inherited_steps);
        let inherited_allows = Self::hit_clip_steps_allow(eff_inherited, px, py);

        let (px, py) = Self::unproject_point(layout, px, py);

        let own_clips = layout.style.overflow_x != strata_style::types::Overflow::Visible
            || layout.style.overflow_y != strata_style::types::Overflow::Visible;
        let own_allows = !own_clips || layout.geometry.border_box.contains_point(px, py);

        let mut base_steps = Vec::with_capacity(eff_inherited.len() + 1);
        base_steps.extend_from_slice(eff_inherited);
        let own_step = Self::hit_clip_step_for(layout);
        if own_step.clip.is_some() || own_step.is_abs_cb || own_step.is_fixed_cb {
            base_steps.push(own_step);
        }

        let mut negative_z = Vec::new();
        let mut auto_or_zero_z = Vec::new();
        let mut positive_z = Vec::new();
        Self::collect_hoisted_stacking_participants(
            bp,
            layout,
            &base_steps,
            &mut negative_z,
            &mut auto_or_zero_z,
            &mut positive_z,
            depth + 1,
        );

        negative_z.sort_by_key(|(_, l, _)| l.style.z_index.unwrap_or(0));
        positive_z.sort_by_key(|(_, l, _)| l.style.z_index.unwrap_or(0));

        // Layer 7: Child stacking contexts with positive z-index (highest z-index first, reverse DOM on ties)
        for (c_bp, c_layout, c_steps) in positive_z.iter().rev() {
            if let Some(hit) = Self::hit_test_stacking_context_with(
                c_bp,
                c_layout,
                px,
                py,
                c_steps,
                depth + 1,
                test_self,
            ) {
                return Some(hit);
            }
        }

        // Layer 6: Positioned descendants with z-index: auto and child stacking contexts with z-index: 0
        for (c_bp, c_layout, is_sc, c_steps) in auto_or_zero_z.iter().rev() {
            let hit = if *is_sc {
                Self::hit_test_stacking_context_with(
                    c_bp,
                    c_layout,
                    px,
                    py,
                    c_steps,
                    depth + 1,
                    test_self,
                )
            } else {
                Self::hit_test_in_flow_non_hoisted(
                    c_bp,
                    c_layout,
                    px,
                    py,
                    c_steps,
                    depth + 1,
                    test_self,
                )
            };
            if hit.is_some() {
                return hit;
            }
        }

        // Layers 5, 4, 3: In-flow inline, float, and block descendants
        if inherited_allows && own_allows {
            if let Some(hit) =
                Self::hit_test_direct_in_flow_children(bp, layout, px, py, depth, test_self)
            {
                return Some(hit);
            }
        }

        // Layer 2: Child stacking contexts with negative z-index (closest to 0 first, reverse DOM on ties)
        for (c_bp, c_layout, c_steps) in negative_z.iter().rev() {
            if let Some(hit) = Self::hit_test_stacking_context_with(
                c_bp,
                c_layout,
                px,
                py,
                c_steps,
                depth + 1,
                test_self,
            ) {
                return Some(hit);
            }
        }

        // Layer 1: Stacking context root element itself
        if inherited_allows {
            test_self(bp, layout, px, py)
        } else {
            None
        }
    }

    /// Recursively hit tests a coordinate point against the blueprint and computed layout hierarchy,
    /// returning the nearest listener registered for `event_type`, its element border box, and its `EntityId`.
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
        Self::hit_test_stacking_context_with(
            blueprint,
            layout,
            px,
            py,
            &[],
            0,
            &mut |bp, node_layout, lx, ly| {
                if matches!(
                    node_layout.style.visibility,
                    strata_style::types::Visibility::Hidden
                ) || matches!(
                    node_layout.style.pointer_events,
                    strata_style::types::PointerEvents::None
                ) {
                    return None;
                }
                if let NodeBlueprint::Element { attributes, .. } = bp {
                    if node_layout.geometry.border_box.contains_point(lx, ly) {
                        for attr in attributes {
                            if let strata_dom::Attribute::EventListener { event: ev, handler } =
                                attr
                            {
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
            },
        )
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
        for len in (0..=path.len()).rev() {
            let prefix = &path[..len];
            let Some(bp) = Self::find_blueprint_at_path(blueprint, prefix) else {
                continue;
            };
            let mut node_layout = layout;
            let mut valid = true;
            for &idx in prefix {
                if let Some(child) = node_layout.children.get(idx) {
                    node_layout = child;
                } else {
                    valid = false;
                    break;
                }
            }
            if !valid {
                continue;
            }
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

    /// Recursively hit tests a coordinate point against the blueprint and computed layout hierarchy to determine cursor style.
    pub fn hit_test_cursor(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        px: f32,
        py: f32,
    ) -> strata_platform::window::CursorIcon {
        Self::hit_test_stacking_context_with(
            blueprint,
            layout,
            px,
            py,
            &[],
            0,
            &mut |bp, node_layout, lx, ly| {
                if matches!(
                    node_layout.style.visibility,
                    strata_style::types::Visibility::Hidden
                ) || matches!(
                    node_layout.style.pointer_events,
                    strata_style::types::PointerEvents::None
                ) {
                    return None;
                }
                if let NodeBlueprint::Element {
                    tag,
                    classes,
                    attributes,
                    ..
                } = bp
                {
                    if node_layout.geometry.border_box.contains_point(lx, ly) {
                        match node_layout.style.cursor {
                            strata_style::types::Cursor::Pointer => {
                                return Some(strata_platform::window::CursorIcon::Pointer)
                            }
                            strata_style::types::Cursor::Text => {
                                return Some(strata_platform::window::CursorIcon::Text)
                            }
                            strata_style::types::Cursor::Crosshair => {
                                return Some(strata_platform::window::CursorIcon::Crosshair)
                            }
                            strata_style::types::Cursor::Move => {
                                return Some(strata_platform::window::CursorIcon::Move)
                            }
                            strata_style::types::Cursor::NotAllowed => {
                                return Some(strata_platform::window::CursorIcon::NotAllowed)
                            }
                            strata_style::types::Cursor::Wait => {
                                return Some(strata_platform::window::CursorIcon::Wait)
                            }
                            strata_style::types::Cursor::Grab => {
                                return Some(strata_platform::window::CursorIcon::Grab)
                            }
                            strata_style::types::Cursor::Grabbing => {
                                return Some(strata_platform::window::CursorIcon::Grabbing)
                            }
                            strata_style::types::Cursor::ColResize => {
                                return Some(strata_platform::window::CursorIcon::ColResize)
                            }
                            strata_style::types::Cursor::RowResize => {
                                return Some(strata_platform::window::CursorIcon::RowResize)
                            }
                            strata_style::types::Cursor::Help => {
                                return Some(strata_platform::window::CursorIcon::Help)
                            }
                            strata_style::types::Cursor::None => {
                                return Some(strata_platform::window::CursorIcon::None)
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
                            return Some(strata_platform::window::CursorIcon::Pointer);
                        }

                        let has_text_cursor = matches!(tag, Tag::Input | Tag::Textarea)
                            || classes.iter().any(|c| {
                                c.split_whitespace()
                                    .any(|tok| tok == "cursor-text" || tok == "select-text")
                            });

                        if has_text_cursor {
                            return Some(strata_platform::window::CursorIcon::Text);
                        }
                    }
                }
                None
            },
        )
        .unwrap_or(strata_platform::window::CursorIcon::Default)
    }

    /// Finds the value of the deepest element property `key` whose border box contains the point.
    /// Elements are tested front-to-back according to CSS stacking contexts so overlays win over what they cover.
    pub fn hit_test_property(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        px: f32,
        py: f32,
        key: &str,
    ) -> Option<String> {
        Self::hit_test_stacking_context_with(
            blueprint,
            layout,
            px,
            py,
            &[],
            0,
            &mut |bp, node_layout, lx, ly| {
                if matches!(
                    node_layout.style.visibility,
                    strata_style::types::Visibility::Hidden
                ) || matches!(
                    node_layout.style.pointer_events,
                    strata_style::types::PointerEvents::None
                ) {
                    return None;
                }
                if let NodeBlueprint::Element { attributes, .. } = bp {
                    if node_layout.geometry.border_box.contains_point(lx, ly) {
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
            },
        )
    }
}
