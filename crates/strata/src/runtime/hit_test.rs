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

    /// Recursively hit tests a coordinate point against the blueprint and computed layout hierarchy,
    /// returning the nearest (deepest, front-most) listener registered for `event_type`.
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
        let (px, py) = Self::unproject_point(layout, px, py);

        // If element clips overflow, coordinates outside its border box are clipped
        if (layout.style.overflow_x != strata_style::types::Overflow::Visible
            || layout.style.overflow_y != strata_style::types::Overflow::Visible)
            && !layout.geometry.border_box.contains_point(px, py) {
                return None;
            }

        match blueprint {
            NodeBlueprint::Element {
                attributes,
                children,
                ..
            } => {
                let has_fragments = children
                    .iter()
                    .any(|c| matches!(c, NodeBlueprint::Fragment(_)));
                if !has_fragments {
                    for (child_bp, child_layout) in
                        children.iter().zip(layout.children.iter()).rev()
                    {
                        if let Some(res) = Self::hit_test_event_with_box(
                            child_bp,
                            child_layout,
                            px,
                            py,
                            event_type,
                        ) {
                            return Some(res);
                        }
                    }
                } else {
                    let mut flat_children = Vec::new();
                    for c in children {
                        Self::flatten_blueprints(c, &mut flat_children);
                    }

                    for (&child_bp, child_layout) in
                        flat_children.iter().zip(layout.children.iter()).rev()
                    {
                        if let Some(res) = Self::hit_test_event_with_box(
                            child_bp,
                            child_layout,
                            px,
                            py,
                            event_type,
                        ) {
                            return Some(res);
                        }
                    }
                }

                if layout.geometry.border_box.contains_point(px, py) {
                    for attr in attributes {
                        if let strata_dom::Attribute::EventListener { event: ev, handler } = attr {
                            if *ev != event_type {
                                continue;
                            }
                            return Some((handler.clone(), &layout.geometry.border_box));
                        }
                    }
                }
                None
            }
            NodeBlueprint::Fragment(children) => {
                for (child_bp, child_layout) in children.iter().zip(layout.children.iter()).rev() {
                    if let Some(res) =
                        Self::hit_test_event_with_box(child_bp, child_layout, px, py, event_type)
                    {
                        return Some(res);
                    }
                }
                None
            }
            _ => None,
        }
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
        let (px, py) = Self::unproject_point(layout, px, py);

        // If element clips overflow, coordinates outside its border box are clipped
        if (layout.style.overflow_x != strata_style::types::Overflow::Visible
            || layout.style.overflow_y != strata_style::types::Overflow::Visible)
            && !layout.geometry.border_box.contains_point(px, py) {
                return strata_platform::window::CursorIcon::Default;
            }

        match blueprint {
            NodeBlueprint::Element {
                tag,
                classes,
                attributes,
                children,
            } => {
                let has_fragments = children
                    .iter()
                    .any(|c| matches!(c, NodeBlueprint::Fragment(_)));
                if !has_fragments {
                    for (child_bp, child_layout) in
                        children.iter().zip(layout.children.iter()).rev()
                    {
                        let child_cur = Self::hit_test_cursor(child_bp, child_layout, px, py);
                        if child_cur != strata_platform::window::CursorIcon::Default {
                            return child_cur;
                        }
                    }
                } else {
                    let mut flat_children = Vec::new();
                    for c in children {
                        Self::flatten_blueprints(c, &mut flat_children);
                    }

                    for (&child_bp, child_layout) in
                        flat_children.iter().zip(layout.children.iter()).rev()
                    {
                        let child_cur = Self::hit_test_cursor(child_bp, child_layout, px, py);
                        if child_cur != strata_platform::window::CursorIcon::Default {
                            return child_cur;
                        }
                    }
                }

                if layout.geometry.border_box.contains_point(px, py) {
                    match layout.style.cursor {
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

                strata_platform::window::CursorIcon::Default
            }
            NodeBlueprint::Fragment(children) => {
                for (child_bp, child_layout) in children.iter().zip(layout.children.iter()).rev() {
                    let child_cur = Self::hit_test_cursor(child_bp, child_layout, px, py);
                    if child_cur != strata_platform::window::CursorIcon::Default {
                        return child_cur;
                    }
                }
                strata_platform::window::CursorIcon::Default
            }
            _ => strata_platform::window::CursorIcon::Default,
        }
    }

    /// Finds the value of the deepest element property `key` whose border box contains the point.
    /// Elements are tested front-to-back so overlays win over what they cover.
    pub fn hit_test_property(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        px: f32,
        py: f32,
        key: &str,
    ) -> Option<String> {
        let (px, py) = Self::unproject_point(layout, px, py);

        // If element clips overflow, coordinates outside its border box are clipped
        if (layout.style.overflow_x != strata_style::types::Overflow::Visible
            || layout.style.overflow_y != strata_style::types::Overflow::Visible)
            && !layout.geometry.border_box.contains_point(px, py) {
                return None;
            }
        match blueprint {
            NodeBlueprint::Element {
                attributes,
                children,
                ..
            } => {
                let has_fragments = children
                    .iter()
                    .any(|c| matches!(c, NodeBlueprint::Fragment(_)));
                if !has_fragments {
                    for (child_bp, child_layout) in
                        children.iter().zip(layout.children.iter()).rev()
                    {
                        if let Some(v) =
                            Self::hit_test_property(child_bp, child_layout, px, py, key)
                        {
                            return Some(v);
                        }
                    }
                } else {
                    let mut flat_children = Vec::new();
                    for c in children {
                        Self::flatten_blueprints(c, &mut flat_children);
                    }
                    for (&child_bp, child_layout) in
                        flat_children.iter().zip(layout.children.iter()).rev()
                    {
                        if let Some(v) =
                            Self::hit_test_property(child_bp, child_layout, px, py, key)
                        {
                            return Some(v);
                        }
                    }
                }
                if layout.geometry.border_box.contains_point(px, py) {
                    for attr in attributes {
                        if let strata_dom::Attribute::Property { key: k, value } = attr {
                            if k == key {
                                return Some(value.clone());
                            }
                        }
                    }
                }
                None
            }
            NodeBlueprint::Fragment(children) => {
                for (child_bp, child_layout) in children.iter().zip(layout.children.iter()).rev() {
                    if let Some(v) = Self::hit_test_property(child_bp, child_layout, px, py, key) {
                        return Some(v);
                    }
                }
                None
            }
            _ => None,
        }
    }

}
