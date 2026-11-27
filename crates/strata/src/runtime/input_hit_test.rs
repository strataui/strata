//! Hit-testing and candidate collection specifically for interactive text inputs.

use super::*;

impl AppRuntime {
    /// Hit tests `(px, py)` in W3C stacking order to find a non-occluded clickable `<input>` or `<textarea>`.
    ///
    /// First resolves the single front-most visual target path via [`Self::hit_test_node_path`] so
    /// higher-z `<Portal>` menus, modal backdrops, dialogs, and floating windows never allow clicks
    /// to pierce through and focus occluded background inputs underneath.
    pub fn hit_test_input(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        px: f32,
        py: f32,
    ) -> Option<InputHitTarget> {
        let visual_path = Self::hit_test_node_path(blueprint, layout, px, py)?;
        let chain = Self::resolve_chain_for_path(blueprint, layout, &visual_path);

        let mut matched_input_path: Option<Vec<usize>> = None;
        for depth_idx in (0..chain.len()).rev() {
            let (bp, node_layout) = chain[depth_idx];
            if let NodeBlueprint::Element {
                tag,
                classes,
                attributes,
                ..
            } = bp
            {
                // 1. Direct hit on an `<input>` or `<textarea>` (or a descendant inside it).
                if matches!(tag, Tag::Input | Tag::Textarea) {
                    matched_input_path = Some(visual_path[..depth_idx].to_vec());
                    break;
                }

                // 2. Non-occluded `<label>` or `cursor-text` wrapper containing an in-flow input.
                let is_input_wrapper = matches!(tag, Tag::Label)
                    || node_layout.style.cursor == strata_style::types::Cursor::Text
                    || classes
                        .iter()
                        .any(|c| c.split_whitespace().any(|tok| tok == "cursor-text"));
                if is_input_wrapper {
                    let wrapper_path = &visual_path[..depth_idx];
                    if let Some(child_input_path) =
                        Self::find_first_non_overlay_descendant_input_path(
                            bp,
                            node_layout,
                            wrapper_path,
                        )
                    {
                        matched_input_path = Some(child_input_path);
                        break;
                    }
                }

                // 3. Stop walking at top-layer portals, out-of-flow overlays, or interactive
                // controls so clicks on overlays/buttons never escape upward into an outer
                // wrapper or occluded background input.
                let is_overlay_or_interactive = node_layout.is_top_layer
                    || matches!(
                        node_layout.style.position,
                        strata_style::types::Position::Fixed
                            | strata_style::types::Position::Absolute
                    )
                    || matches!(tag, Tag::Button | Tag::A)
                    || attributes.iter().any(|a| {
                        matches!(
                            a,
                            strata_dom::Attribute::EventListener {
                                event: strata_dom::EventType::Click
                                    | strata_dom::EventType::MouseDown
                                    | strata_dom::EventType::PointerDown,
                                ..
                            }
                        )
                    });
                if is_overlay_or_interactive {
                    break;
                }
            }
        }

        let target_path = matched_input_path?;
        let mut cur_path = Vec::with_capacity(target_path.len());
        let mut counter = 0usize;
        Self::find_input_target_at_path_guarded(
            blueprint,
            layout,
            &target_path,
            &mut cur_path,
            &mut counter,
            0,
        )
    }

    fn find_first_non_overlay_descendant_input_path(
        bp: &NodeBlueprint,
        layout: &LayoutNode,
        base_path: &[usize],
    ) -> Option<Vec<usize>> {
        fn walk(
            bp: &NodeBlueprint,
            layout: &LayoutNode,
            path: &mut Vec<usize>,
            depth: usize,
        ) -> Option<Vec<usize>> {
            if depth >= 256 {
                return None;
            }
            for (idx, child_bp, child_layout) in AppRuntime::paired_blueprint_children(bp, layout) {
                if child_layout.is_top_layer
                    || child_layout.style.display == strata_style::types::Display::None
                    || child_layout.style.visibility == strata_style::types::Visibility::Hidden
                {
                    continue;
                }
                path.push(idx);
                if let NodeBlueprint::Element { tag, .. } = child_bp {
                    if matches!(tag, Tag::Input | Tag::Textarea) {
                        return Some(path.clone());
                    }
                }
                if let Some(found) = walk(child_bp, child_layout, path, depth + 1) {
                    return Some(found);
                }
                path.pop();
            }
            None
        }
        let mut path = base_path.to_vec();
        walk(bp, layout, &mut path, 0)
    }

    pub(crate) fn build_input_hit_target(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        input_index: usize,
    ) -> Option<InputHitTarget> {
        let NodeBlueprint::Element {
            tag, attributes, ..
        } = blueprint
        else {
            return None;
        };
        if !matches!(tag, Tag::Input | Tag::Textarea) {
            return None;
        }
        let is_multiline = matches!(tag, Tag::Textarea);

        let mut current_value = String::new();
        let mut has_value_prop = false;
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
                        has_value_prop = true;
                    } else if key == "id" && !value.is_empty() {
                        input_id = Some(value.clone());
                    }
                }
                strata_dom::Attribute::EventListener { event, handler } => match event {
                    strata_dom::EventType::Input => on_input = Some(handler.clone()),
                    strata_dom::EventType::Change => on_change = Some(handler.clone()),
                    strata_dom::EventType::Submit => on_submit = Some(handler.clone()),
                    strata_dom::EventType::Focus => on_focus = Some(handler.clone()),
                    strata_dom::EventType::Blur => on_blur = Some(handler.clone()),
                    _ => {}
                },
                _ => {}
            }
        }

        if is_multiline && current_value.is_empty() {
            let child_text = Self::extract_text_content(blueprint);
            if !child_text.is_empty() {
                current_value = child_text;
                has_value_prop = true;
            }
        }

        let is_mono = strata_layout::is_monospace_family(layout.style.font_family);
        Some(InputHitTarget {
            input_index,
            input_id,
            entity_id: if layout.entity_id.is_valid() {
                Some(layout.entity_id)
            } else {
                None
            },
            is_multiline,
            current_value,
            has_value_prop,
            font_size: layout.style.font_size,
            line_height: layout.style.line_height,
            is_mono,
            font_family: layout.style.font_family,
            content_box_x: layout.geometry.content_box.x,
            content_box_y: layout.geometry.content_box.y,
            content_box_width: layout.geometry.content_box.width,
            content_box_height: layout.geometry.content_box.height,
            on_input,
            on_change,
            on_submit,
            on_focus,
            on_blur,
        })
    }

    fn find_input_target_at_path_guarded(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        target_path: &[usize],
        cur_path: &mut Vec<usize>,
        counter: &mut usize,
        depth: usize,
    ) -> Option<InputHitTarget> {
        if depth >= 256 {
            return None;
        }
        if let NodeBlueprint::Element { tag, .. } = blueprint {
            if matches!(tag, Tag::Input | Tag::Textarea) {
                let this_idx = *counter;
                *counter += 1;
                if cur_path.as_slice() == target_path {
                    return Self::build_input_hit_target(blueprint, layout, this_idx);
                }
            }
        }
        for (idx, child_bp, child_layout) in Self::paired_blueprint_children(blueprint, layout) {
            cur_path.push(idx);
            let found = Self::find_input_target_at_path_guarded(
                child_bp,
                child_layout,
                target_path,
                cur_path,
                counter,
                depth + 1,
            );
            cur_path.pop();
            if found.is_some() {
                return found;
            }
        }
        None
    }

    /// Locates the current [`InputHitTarget`] metadata (value prop, `has_value_prop`, callbacks,
    /// and document-order index) for an active focus entry across view rebuilds.
    pub(crate) fn find_input_target_for_focus(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        target_index: usize,
        target_id: Option<&str>,
        target_entity_id: Option<strata_primitives::EntityId>,
    ) -> Option<InputHitTarget> {
        fn walk(
            blueprint: &NodeBlueprint,
            layout: &LayoutNode,
            target_index: usize,
            target_id: Option<&str>,
            target_entity_id: Option<strata_primitives::EntityId>,
            counter: &mut usize,
            fallback: &mut Option<InputHitTarget>,
            depth: usize,
        ) -> Option<InputHitTarget> {
            if depth >= 256 {
                return None;
            }
            if let NodeBlueprint::Element {
                tag, attributes, ..
            } = blueprint
            {
                if matches!(tag, Tag::Input | Tag::Textarea) {
                    let this_idx = *counter;
                    *counter += 1;
                    if let Some(eid) = target_entity_id {
                        if eid.is_valid() && layout.entity_id == eid {
                            return AppRuntime::build_input_hit_target(blueprint, layout, this_idx);
                        }
                    }
                    if fallback.is_none()
                        && AppRuntime::input_blueprint_matches_target(
                            attributes,
                            this_idx,
                            target_index,
                            target_id,
                        )
                    {
                        *fallback = AppRuntime::build_input_hit_target(blueprint, layout, this_idx);
                    }
                }
            }
            for (_idx, child_bp, child_layout) in
                AppRuntime::paired_blueprint_children(blueprint, layout)
            {
                if let Some(exact) = walk(
                    child_bp,
                    child_layout,
                    target_index,
                    target_id,
                    target_entity_id,
                    counter,
                    fallback,
                    depth + 1,
                ) {
                    return Some(exact);
                }
            }
            None
        }

        let mut counter = 0usize;
        let mut fallback = None;
        walk(
            blueprint,
            layout,
            target_index,
            target_id,
            target_entity_id,
            &mut counter,
            &mut fallback,
            0,
        )
        .or(fallback)
    }

    pub(crate) fn input_blueprint_matches_target(
        attributes: &[strata_dom::Attribute],
        this_idx: usize,
        target_index: usize,
        target_id: Option<&str>,
    ) -> bool {
        if let Some(tid) = target_id.filter(|s| !s.is_empty()) {
            for attr in attributes {
                if let strata_dom::Attribute::Property { key, value } = attr {
                    if key == "id" && !value.is_empty() && value == tid {
                        return true;
                    }
                }
            }
            false
        } else {
            this_idx == target_index
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

                    if Self::input_blueprint_matches_target(
                        attributes,
                        this_idx,
                        target_index,
                        target_id,
                    ) {
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

                    if Self::input_blueprint_matches_target(
                        attributes,
                        this_idx,
                        target_index,
                        target_id,
                    ) {
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

                    if Self::input_blueprint_matches_target(
                        attributes,
                        this_idx,
                        target_index,
                        target_id,
                    ) {
                        return Some(blueprint);
                    }
                }

                let mut flat = Vec::new();
                for c in children {
                    Self::flatten_blueprints(c, &mut flat);
                }

                for child_bp in flat {
                    if let Some(found) = Self::find_input_blueprint_guarded(
                        child_bp,
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
                    for child_bp in flat {
                        if let Some(found) = Self::find_input_blueprint_guarded(
                            child_bp,
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
}
