//! Accessibility tree generation, text extraction, and assistive technology action dispatch.

use super::*;

impl AppRuntime {
    /// Recursively extracts concatenated text content from a blueprint subtree.
    pub fn extract_text_content(bp: &NodeBlueprint) -> String {
        match bp {
            NodeBlueprint::Text(s) => s.clone(),
            NodeBlueprint::DynamicText(f) => f(),
            NodeBlueprint::Element { children, .. } | NodeBlueprint::Fragment(children) => children
                .iter()
                .map(Self::extract_text_content)
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" "),
            NodeBlueprint::Empty => String::new(),
        }
    }

    /// Re-evaluates accessibility tree matching current blueprint and layout, emitting incremental diffs to the bridge.
    pub fn update_accessibility_tree(&mut self) -> strata_a11y::A11yTreeUpdate {
        let focused_id = self
            .active_focus
            .as_ref()
            .map(|f| strata_a11y::A11yNodeId(f.input_index as u64 + 1));
        let new_tree = strata_a11y::build_a11y_tree_with_layout(
            &self.root_blueprint,
            &self.layout_root,
            focused_id,
        );

        let update = self.a11y_bridge.update(new_tree.clone());
        self.a11y_tree = Some(new_tree);
        self.a11y_updates_count += 1;
        update
    }

    /// Returns a full accessibility tree update snapshot containing all current nodes.
    pub fn full_accessibility_update(&self) -> strata_a11y::A11yTreeUpdate {
        if let Some(ref tree) = self.a11y_tree {
            strata_a11y::A11yTreeUpdate::diff(None, tree)
        } else {
            strata_a11y::A11yTreeUpdate::empty()
        }
    }

    /// Dispatches an assistive technology action (e.g. from screen reader) to the target node.
    pub fn dispatch_accessibility_action(
        &mut self,
        target_node: strata_a11y::A11yNodeId,
        action: strata_a11y::A11yAction,
    ) -> bool {
        if let Some(ref tree) = self.a11y_tree {
            if let Some(node) = tree.get_node(target_node) {
                match &action {
                    strata_a11y::A11yAction::Click => {
                        if let Some(bounds) = node.bounds {
                            let cx = bounds.x + bounds.width * 0.5;
                            let cy = bounds.y + bounds.height * 0.5;
                            self.dispatch_click(cx, cy);
                            self.a11y_bridge.dispatch_action(target_node, action);
                            return true;
                        }
                    }
                    strata_a11y::A11yAction::Focus => {
                        if let Some(bounds) = node.bounds {
                            let cx = bounds.x + bounds.width * 0.5;
                            let cy = bounds.y + bounds.height * 0.5;
                            self.dispatch_pointer_down(cx, cy);
                            self.a11y_bridge.dispatch_action(target_node, action);
                            return true;
                        }
                    }
                    strata_a11y::A11yAction::SetValue(val) => {
                        if let Some(ref focus) = self.active_focus {
                            focus.state.text.set(val.clone());
                            if let Some(ref on_input) = focus.on_input {
                                on_input(strata_dom::Event {
                                    value: val.clone(),
                                    ..Default::default()
                                });
                            }
                            self.a11y_bridge.dispatch_action(target_node, action);
                            return true;
                        }
                    }
                    _ => {
                        self.a11y_bridge.dispatch_action(target_node, action);
                        return true;
                    }
                }
            }
        }
        self.a11y_bridge.dispatch_action(target_node, action);
        false
    }

}
