//! Hit-testing and candidate collection specifically for interactive text inputs.

use super::*;

impl AppRuntime {
    /// Recursively hit tests a coordinate point against the blueprint to find clickable text input elements.
    pub fn hit_test_input(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        px: f32,
        py: f32,
    ) -> Option<InputHitTarget> {
        let mut candidates = Vec::new();
        let mut counter = 0;
        Self::collect_input_candidates(blueprint, layout, &mut counter, &mut candidates);

        for (target, border_box) in candidates.into_iter().rev() {
            if border_box.contains_point(px, py) {
                return Some(target);
            }
        }
        None
    }

    pub(crate) fn collect_input_candidates(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        counter: &mut usize,
        out: &mut Vec<(InputHitTarget, strata_layout::Rect)>,
    ) {
        Self::collect_input_candidates_guarded(blueprint, layout, counter, out, 0);
    }

    pub(crate) fn collect_input_candidates_guarded(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        counter: &mut usize,
        out: &mut Vec<(InputHitTarget, strata_layout::Rect)>,
        depth: usize,
    ) {
        if depth >= 256 {
            return;
        }
        match blueprint {
            NodeBlueprint::Element {
                tag,
                classes,
                attributes,
                children,
                ..
            } => {
                let is_input = matches!(tag, Tag::Input | Tag::Textarea);
                if is_input {
                    let this_idx = *counter;
                    *counter += 1;

                    let mut current_value = String::new();
                    let mut input_id = None;
                    let mut on_input = None;
                    let mut on_change = None;
                    let mut on_submit = None;
                    let mut on_focus = None;
                    let mut on_blur = None;

                    for attr in attributes {
                        match attr {
                            strata_dom::Attribute::Property { key, value } => {
                                if key == "value" {
                                    current_value = value.clone();
                                } else if key == "id" {
                                    input_id = Some(value.clone());
                                }
                            }
                            strata_dom::Attribute::EventListener { event, handler } => {
                                match event {
                                    strata_dom::EventType::Input => {
                                        on_input = Some(handler.clone())
                                    }
                                    strata_dom::EventType::Change => {
                                        on_change = Some(handler.clone())
                                    }
                                    strata_dom::EventType::Submit => {
                                        on_submit = Some(handler.clone())
                                    }
                                    strata_dom::EventType::Focus => {
                                        on_focus = Some(handler.clone())
                                    }
                                    strata_dom::EventType::Blur => on_blur = Some(handler.clone()),
                                    _ => {}
                                }
                            }
                            _ => {}
                        }
                    }

                    let is_mono = strata_layout::is_monospace_family(layout.style.font_family);
                    out.push((
                        InputHitTarget {
                            input_index: this_idx,
                            input_id,
                            current_value,
                            font_size: layout.style.font_size,
                            line_height: layout.style.line_height,
                            is_mono,
                            font_family: layout.style.font_family,
                            content_box_x: layout.geometry.content_box.x,
                            content_box_y: layout.geometry.content_box.y,
                            content_box_width: layout.geometry.content_box.width,
                            on_input,
                            on_change,
                            on_submit,
                            on_focus,
                            on_blur,
                        },
                        layout.geometry.border_box,
                    ));
                }

                let start_idx = out.len();
                let mut flat = Vec::new();
                for c in children {
                    Self::flatten_blueprints(c, &mut flat);
                }

                for (&child_bp, child_layout) in flat.iter().zip(layout.children.iter()) {
                    Self::collect_input_candidates_guarded(child_bp, child_layout, counter, out, depth + 1);
                }

                // If this is a container with cursor-text or wrapping an input, register a fallback candidate
                if !is_input && out.len() > start_idx {
                    let has_cursor_text = classes
                        .iter()
                        .any(|c| c.split_whitespace().any(|tok| tok == "cursor-text"))
                        || layout.style.cursor == strata_style::types::Cursor::Text
                        || matches!(tag, Tag::Label);
                    if has_cursor_text {
                        let child_target = out[start_idx].0.clone();
                        out.insert(start_idx, (child_target, layout.geometry.border_box));
                    }
                }
            }
            NodeBlueprint::Fragment(children) => {
                for c in children {
                    let mut flat = Vec::new();
                    Self::flatten_blueprints(c, &mut flat);
                    for (&child_bp, child_layout) in flat.iter().zip(layout.children.iter()) {
                        Self::collect_input_candidates_guarded(child_bp, child_layout, counter, out, depth + 1);
                    }
                }
            }
            _ => {}
        }
    }

    /// Locates the LayoutNode corresponding to a target input index or id in document order.
    pub fn find_input_layout<'a>(
        blueprint: &NodeBlueprint,
        layout: &'a LayoutNode,
        target_index: usize,
        target_id: Option<&str>,
        counter: &mut usize,
    ) -> Option<&'a LayoutNode> {
        Self::find_input_layout_guarded(blueprint, layout, target_index, target_id, counter, 0)
    }

    pub(crate) fn find_input_layout_guarded<'a>(
        blueprint: &NodeBlueprint,
        layout: &'a LayoutNode,
        target_index: usize,
        target_id: Option<&str>,
        counter: &mut usize,
        depth: usize,
    ) -> Option<&'a LayoutNode> {
        if depth >= 256 {
            return None;
        }
        match blueprint {
            NodeBlueprint::Element {
                tag,
                attributes,
                children,
                ..
            } => {
                let is_input = matches!(tag, Tag::Input | Tag::Textarea);
                if is_input {
                    let this_idx = *counter;
                    *counter += 1;

                    let mut matches_id = false;
                    if let Some(tid) = target_id {
                        for attr in attributes {
                            if let strata_dom::Attribute::Property { key, value } = attr {
                                if key == "id" && value == tid {
                                    matches_id = true;
                                    break;
                                }
                            }
                        }
                    }

                    let is_match = if target_id.is_some() {
                        matches_id
                    } else {
                        this_idx == target_index
                    };

                    if is_match {
                        return Some(layout);
                    }
                }

                let mut flat = Vec::new();
                for c in children {
                    Self::flatten_blueprints(c, &mut flat);
                }

                for (&child_bp, child_layout) in flat.iter().zip(layout.children.iter()) {
                    if let Some(found) = Self::find_input_layout_guarded(
                        child_bp,
                        child_layout,
                        target_index,
                        target_id,
                        counter,
                        depth + 1,
                    ) {
                        return Some(found);
                    }
                }

                None
            }
            NodeBlueprint::Fragment(children) => {
                for c in children {
                    let mut flat = Vec::new();
                    Self::flatten_blueprints(c, &mut flat);
                    for (&child_bp, child_layout) in flat.iter().zip(layout.children.iter()) {
                        if let Some(found) = Self::find_input_layout_guarded(
                            child_bp,
                            child_layout,
                            target_index,
                            target_id,
                            counter,
                            depth + 1,
                        ) {
                            return Some(found);
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Locates the mutable LayoutNode corresponding to a target input index or id in document order.
    pub fn find_input_layout_mut<'a>(
        blueprint: &NodeBlueprint,
        layout: &'a mut LayoutNode,
        target_index: usize,
        target_id: Option<&str>,
        counter: &mut usize,
    ) -> Option<&'a mut LayoutNode> {
        Self::find_input_layout_mut_guarded(blueprint, layout, target_index, target_id, counter, 0)
    }

    pub(crate) fn find_input_layout_mut_guarded<'a>(
        blueprint: &NodeBlueprint,
        layout: &'a mut LayoutNode,
        target_index: usize,
        target_id: Option<&str>,
        counter: &mut usize,
        depth: usize,
    ) -> Option<&'a mut LayoutNode> {
        if depth >= 256 {
            return None;
        }
        match blueprint {
            NodeBlueprint::Element {
                tag,
                attributes,
                children,
                ..
            } => {
                let is_input = matches!(tag, Tag::Input | Tag::Textarea);
                if is_input {
                    let this_idx = *counter;
                    *counter += 1;

                    let mut matches_id = false;
                    if let Some(tid) = target_id {
                        for attr in attributes {
                            if let strata_dom::Attribute::Property { key, value } = attr {
                                if key == "id" && value == tid {
                                    matches_id = true;
                                    break;
                                }
                            }
                        }
                    }

                    let is_match = if target_id.is_some() {
                        matches_id
                    } else {
                        this_idx == target_index
                    };

                    if is_match {
                        return Some(layout);
                    }
                }

                let mut flat = Vec::new();
                for c in children {
                    Self::flatten_blueprints(c, &mut flat);
                }

                for (&child_bp, child_layout) in flat.iter().zip(layout.children.iter_mut()) {
                    if let Some(found) = Self::find_input_layout_mut_guarded(
                        child_bp,
                        child_layout,
                        target_index,
                        target_id,
                        counter,
                        depth + 1,
                    ) {
                        return Some(found);
                    }
                }

                None
            }
            NodeBlueprint::Fragment(children) => {
                let mut flat = Vec::new();
                for c in children {
                    Self::flatten_blueprints(c, &mut flat);
                }
                for (&child_bp, child_layout) in flat.iter().zip(layout.children.iter_mut()) {
                    if let Some(found) = Self::find_input_layout_mut_guarded(
                        child_bp,
                        child_layout,
                        target_index,
                        target_id,
                        counter,
                        depth + 1,
                    ) {
                        return Some(found);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Locates the blueprint element corresponding to a target input index or ID.
    pub fn find_input_blueprint<'a>(
        blueprint: &'a NodeBlueprint,
        target_index: usize,
        target_id: Option<&str>,
        counter: &mut usize,
    ) -> Option<&'a NodeBlueprint> {
        Self::find_input_blueprint_guarded(blueprint, target_index, target_id, counter, 0)
    }

    pub(crate) fn find_input_blueprint_guarded<'a>(
        blueprint: &'a NodeBlueprint,
        target_index: usize,
        target_id: Option<&str>,
        counter: &mut usize,
        depth: usize,
    ) -> Option<&'a NodeBlueprint> {
        if depth >= 256 {
            return None;
        }
        match blueprint {
            NodeBlueprint::Element {
                tag,
                attributes,
                children,
                ..
            } => {
                let is_input = matches!(tag, Tag::Input | Tag::Textarea);
                if is_input {
                    let this_idx = *counter;
                    *counter += 1;

                    let mut matches_id = false;
                    if let Some(tid) = target_id {
                        for attr in attributes {
                            if let strata_dom::Attribute::Property { key, value } = attr {
                                if key == "id" && value == tid {
                                    matches_id = true;
                                    break;
                                }
                            }
                        }
                    }

                    let is_match = if target_id.is_some() {
                        matches_id
                    } else {
                        this_idx == target_index
                    };

                    if is_match {
                        return Some(blueprint);
                    }
                }

                let mut flat = Vec::new();
                for c in children {
                    Self::flatten_blueprints(c, &mut flat);
                }

                for child_bp in flat {
                    if let Some(found) =
                        Self::find_input_blueprint_guarded(child_bp, target_index, target_id, counter, depth + 1)
                    {
                        return Some(found);
                    }
                }

                None
            }
            NodeBlueprint::Fragment(children) => {
                for c in children {
                    let mut flat = Vec::new();
                    Self::flatten_blueprints(c, &mut flat);
                    for child_bp in flat {
                        if let Some(found) =
                            Self::find_input_blueprint_guarded(child_bp, target_index, target_id, counter, depth + 1)
                        {
                            return Some(found);
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

}
