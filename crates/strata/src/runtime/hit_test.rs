//! Spatial hit-testing for pointer events, clicks, cursors, and CSS properties.

use super::*;

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

    /// Returns true if `node` establishes an atomic stacking context for paint and hit-testing.
    #[inline]
    pub(crate) fn establishes_stacking_context(node: &LayoutNode, is_root: bool) -> bool {
        if is_root {
            return true;
        }
        let has_z_index = node.style.z_index.is_some();
        let has_opacity = node.style.opacity < 0.999;
        let clips_overflow = node.style.overflow_x != strata_style::types::Overflow::Visible
            || node.style.overflow_y != strata_style::types::Overflow::Visible;
        let has_transform = node.style.has_transform();
        let has_effects = node.style.has_visual_effects();

        has_z_index || has_opacity || clips_overflow || has_transform || has_effects
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

    fn collect_hoisted_stacking_participants<'a>(
        bp: &'a NodeBlueprint,
        layout: &'a LayoutNode,
        negative_z: &mut Vec<(&'a NodeBlueprint, &'a LayoutNode)>,
        auto_or_zero_z: &mut Vec<(&'a NodeBlueprint, &'a LayoutNode, bool)>,
        positive_z: &mut Vec<(&'a NodeBlueprint, &'a LayoutNode)>,
        depth: usize,
    ) {
        if depth >= 256 {
            return;
        }
        for (_idx, child_bp, child_layout) in Self::paired_blueprint_children(bp, layout) {
            if child_layout.style.display == strata_style::types::Display::None {
                continue;
            }
            if Self::establishes_stacking_context(child_layout, false) {
                let z = child_layout.style.z_index.unwrap_or(0);
                if z < 0 {
                    negative_z.push((child_bp, child_layout));
                } else if z > 0 {
                    positive_z.push((child_bp, child_layout));
                } else {
                    auto_or_zero_z.push((child_bp, child_layout, true));
                }
            } else {
                if child_layout.style.position != strata_style::types::Position::Static {
                    auto_or_zero_z.push((child_bp, child_layout, false));
                }
                Self::collect_hoisted_stacking_participants(
                    child_bp,
                    child_layout,
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

        let is_in_flow_non_sc = |c: &LayoutNode| {
            c.style.display != strata_style::types::Display::None
                && !Self::establishes_stacking_context(c, false)
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
        depth: usize,
        test_self: &mut F,
    ) -> Option<T>
    where
        F: FnMut(&'a NodeBlueprint, &'a LayoutNode, f32, f32) -> Option<T>,
    {
        if depth >= 256 || layout.style.display == strata_style::types::Display::None {
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
        depth: usize,
        test_self: &mut F,
    ) -> Option<T>
    where
        F: FnMut(&'a NodeBlueprint, &'a LayoutNode, f32, f32) -> Option<T>,
    {
        if depth >= 256 || layout.style.display == strata_style::types::Display::None {
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
        Self::collect_hoisted_stacking_participants(
            bp,
            layout,
            &mut negative_z,
            &mut auto_or_zero_z,
            &mut positive_z,
            depth + 1,
        );

        negative_z.sort_by_key(|(_, l)| l.style.z_index.unwrap_or(0));
        positive_z.sort_by_key(|(_, l)| l.style.z_index.unwrap_or(0));

        // Layer 7: Child stacking contexts with positive z-index (highest z-index first, reverse DOM on ties)
        for &(c_bp, c_layout) in positive_z.iter().rev() {
            if let Some(hit) = Self::hit_test_stacking_context_with(
                c_bp,
                c_layout,
                px,
                py,
                depth + 1,
                test_self,
            ) {
                return Some(hit);
            }
        }

        // Layer 6: Positioned descendants with z-index: auto and child stacking contexts with z-index: 0
        for &(c_bp, c_layout, is_sc) in auto_or_zero_z.iter().rev() {
            let hit = if is_sc {
                Self::hit_test_stacking_context_with(c_bp, c_layout, px, py, depth + 1, test_self)
            } else {
                Self::hit_test_in_flow_non_hoisted(c_bp, c_layout, px, py, depth + 1, test_self)
            };
            if hit.is_some() {
                return hit;
            }
        }

        // Layers 5, 4, 3: In-flow inline, float, and block descendants
        if let Some(hit) =
            Self::hit_test_direct_in_flow_children(bp, layout, px, py, depth, test_self)
        {
            return Some(hit);
        }

        // Layer 2: Child stacking contexts with negative z-index (closest to 0 first, reverse DOM on ties)
        for &(c_bp, c_layout) in negative_z.iter().rev() {
            if let Some(hit) = Self::hit_test_stacking_context_with(
                c_bp,
                c_layout,
                px,
                py,
                depth + 1,
                test_self,
            ) {
                return Some(hit);
            }
        }

        // Layer 1: Stacking context root element itself
        test_self(bp, layout, px, py)
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
        Self::hit_test_stacking_context_with(
            blueprint,
            layout,
            px,
            py,
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
