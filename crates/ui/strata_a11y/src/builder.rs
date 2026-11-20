//! Accessibility Tree Builder for Strata DOM Blueprints.
//!
//! Transforms fine-grained reactive `NodeBlueprint` trees into semantic `A11yTree` structures
//! with automatic role inference, accessible label derivation, live region detection,
//! and spatial layout bounds binding.

use crate::node::{A11yAction, A11yLivePoliteness, A11yNode, A11yNodeId, A11yRect, A11yTree};
use crate::role::A11yRole;
use std::collections::HashMap;
use strata_dom::{Attribute, NodeBlueprint, Tag};

/// Maximum recursion depth guard for DOM tree crawling to prevent stack exhaustion on adversarial inputs.
pub const MAX_A11Y_DEPTH: usize = 256;

/// Computes text content contained within a blueprint subtree.
pub fn extract_text_content(blueprint: &NodeBlueprint) -> String {
    let mut out = String::new();
    collect_text(blueprint, &mut out, 0);
    out.trim().to_string()
}

fn collect_text(node: &NodeBlueprint, out: &mut String, depth: usize) {
    if depth >= MAX_A11Y_DEPTH {
        return;
    }
    match node {
        NodeBlueprint::Text(t) => {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(t);
        }
        NodeBlueprint::DynamicText(getter) => {
            let val = getter();
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&val);
        }
        NodeBlueprint::Element { children, .. } => {
            for child in children {
                collect_text(child, out, depth + 1);
            }
        }
        NodeBlueprint::Fragment(children) => {
            for child in children {
                collect_text(child, out, depth + 1);
            }
        }
        NodeBlueprint::Empty => {}
    }
}

/// Builder that recursively crawls a `NodeBlueprint` hierarchy to produce an `A11yTree`.
pub struct A11yTreeBuilder {
    next_id: u64,
    nodes: HashMap<A11yNodeId, A11yNode>,
    focused_id: Option<A11yNodeId>,
}

impl Default for A11yTreeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl A11yTreeBuilder {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            nodes: HashMap::new(),
            focused_id: None,
        }
    }

    /// Sets the actively focused accessibility node ID.
    pub fn with_focused_node(mut self, focused: Option<A11yNodeId>) -> Self {
        self.focused_id = focused;
        self
    }

    /// Builds the complete `A11yTree` from the root blueprint.
    pub fn build(self, root_blueprint: &NodeBlueprint) -> A11yTree {
        self.build_with_layout(root_blueprint, None)
    }

    /// Builds the complete `A11yTree` with spatial bounding boxes from layout.
    pub fn build_with_layout(
        mut self,
        root_blueprint: &NodeBlueprint,
        layout_root: Option<&strata_layout::LayoutNode>,
    ) -> A11yTree {
        let root_id = A11yNodeId(self.next_id);
        self.next_id += 1;

        let mut root_node = A11yNode::new(root_id, A11yRole::RootWebArea);
        if let Some(l) = layout_root {
            root_node.bounds = Some(A11yRect::new(
                l.geometry.border_box.x,
                l.geometry.border_box.y,
                l.geometry.border_box.width,
                l.geometry.border_box.height,
            ));
        }

        let mut children_ids = Vec::new();
        self.process_child_with_layout(root_blueprint, layout_root, 0, &mut children_ids);
        root_node.children = children_ids;

        self.nodes.insert(root_id, root_node);

        // Apply focused state if target was registered
        if let Some(fid) = self.focused_id {
            if let Some(node) = self.nodes.get_mut(&fid) {
                node.focused = true;
            }
        }

        A11yTree {
            root_id,
            nodes: self.nodes,
        }
    }

    fn process_child_with_layout(
        &mut self,
        child: &NodeBlueprint,
        layout: Option<&strata_layout::LayoutNode>,
        depth: usize,
        out: &mut Vec<A11yNodeId>,
    ) {
        if depth >= MAX_A11Y_DEPTH {
            return;
        }
        match child {
            NodeBlueprint::Element {
                tag,
                attributes,
                children,
                ..
            } => {
                let id = A11yNodeId(self.next_id);
                self.next_id += 1;

                // 1. Extract ARIA and HTML properties

                let mut role_override: Option<String> = None;
                let mut aria_label: Option<String> = None;
                let mut alt_text: Option<String> = None;
                let mut title_text: Option<String> = None;
                let mut aria_describedby: Option<String> = None;
                let mut aria_live: Option<String> = None;
                let mut aria_atomic = false;
                let mut aria_busy = false;
                let mut disabled = false;
                let mut selected = false;
                let mut checked: Option<bool> = None;
                let mut expanded: Option<bool> = None;
                let mut has_focus_trap = false;
                let mut val_str: Option<String> = None;
                let mut num_val: Option<f64> = None;
                let mut min_val: Option<f64> = None;
                let mut max_val: Option<f64> = None;
                let mut step_val: Option<f64> = None;
                let mut is_tabindex_focusable = false;

                for attr in attributes {
                    if let Attribute::Property { key, value } = attr {
                        match key.as_str() {
                            "role" => role_override = Some(value.clone()),
                            "aria-label" => aria_label = Some(value.clone()),
                            "alt" => alt_text = Some(value.clone()),
                            "title" => title_text = Some(value.clone()),
                            "aria-describedby" => aria_describedby = Some(value.clone()),
                            "aria-live" => aria_live = Some(value.clone()),
                            "aria-atomic" => aria_atomic = value == "true",
                            "aria-busy" => aria_busy = value == "true",
                            "aria-disabled" => disabled = value == "true",
                            "disabled" => disabled = true,
                            "aria-selected" => selected = value == "true",
                            "aria-checked" => checked = Some(value == "true"),
                            "aria-expanded" => expanded = Some(value == "true"),
                            "data-focus-trap" => has_focus_trap = value == "true",
                            "value" => val_str = Some(value.clone()),
                            "aria-valuenow" => num_val = value.parse::<f64>().ok(),
                            "aria-valuemin" => min_val = value.parse::<f64>().ok(),
                            "aria-valuemax" => max_val = value.parse::<f64>().ok(),
                            "aria-valuestep" => step_val = value.parse::<f64>().ok(),
                            "tabindex" => {
                                if let Ok(idx) = value.parse::<i32>() {
                                    if idx >= 0 {
                                        is_tabindex_focusable = true;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }

                // 2. Resolve Role
                let tag_name = tag_to_str(tag);
                let role = A11yRole::from_tag_or_role(tag_name, role_override.as_deref());

                // 3. Resolve Accessible Name:
                //    non-empty `aria-label` -> non-empty `alt` -> non-empty child text -> non-empty `title`.
                let non_empty = |opt: Option<String>| -> Option<String> {
                    opt.and_then(|s| if s.trim().is_empty() { None } else { Some(s) })
                };
                let aria_label = non_empty(aria_label);
                let alt_text = non_empty(alt_text);
                let title_text = non_empty(title_text);
                let aria_describedby = non_empty(aria_describedby);

                let mut title_used_as_name = false;
                let name = if let Some(label) = aria_label {
                    Some(label)
                } else if let Some(alt) = alt_text {
                    Some(alt)
                } else {
                    let direct_text = extract_text_content(child);
                    if !direct_text.is_empty() {
                        Some(direct_text)
                    } else if let Some(title) = title_text.clone() {
                        title_used_as_name = true;
                        Some(title)
                    } else {
                        None
                    }
                };
                let description = if aria_describedby.is_some() {
                    aria_describedby
                } else if !title_used_as_name {
                    title_text
                } else {
                    None
                };

                // 4. Resolve Live Region Politeness
                let live_politeness = if let Some(ref l) = aria_live {
                    let pol = A11yLivePoliteness::from_str_name(l);
                    if pol != A11yLivePoliteness::Off {
                        Some(pol)
                    } else {
                        None
                    }
                } else if role == A11yRole::Alert || role == A11yRole::AlertDialog {
                    Some(A11yLivePoliteness::Assertive)
                } else if role == A11yRole::Status {
                    Some(A11yLivePoliteness::Polite)
                } else {
                    None
                };

                let mut node = A11yNode::new(id, role);
                if let Some(l) = layout {
                    node.bounds = Some(A11yRect::new(
                        l.geometry.border_box.x,
                        l.geometry.border_box.y,
                        l.geometry.border_box.width,
                        l.geometry.border_box.height,
                    ));
                }
                node.name = name;
                node.description = description;
                node.value = val_str;
                node.numeric_value = num_val;
                node.min_numeric_value = min_val;
                node.max_numeric_value = max_val;
                node.numeric_step = step_val;
                node.live = live_politeness;
                node.live_atomic = aria_atomic;
                node.live_busy = aria_busy;
                node.disabled = disabled;
                node.selected = selected;
                node.checked = checked;
                node.expanded = expanded;
                node.focusable = node.focusable || is_tabindex_focusable || has_focus_trap;

                // 5. Populate Supported Actions
                let mut actions = Vec::new();
                match role {
                    A11yRole::Button
                    | A11yRole::Checkbox
                    | A11yRole::Radio
                    | A11yRole::RadioButton
                    | A11yRole::Link
                    | A11yRole::Switch
                    | A11yRole::MenuItem => {
                        actions.push(A11yAction::Click);
                        actions.push(A11yAction::Focus);
                    }
                    A11yRole::TextInput => {
                        actions.push(A11yAction::Focus);
                        actions.push(A11yAction::Blur);
                        actions.push(A11yAction::SetValue(String::new()));
                    }
                    A11yRole::Slider => {
                        actions.push(A11yAction::Increment);
                        actions.push(A11yAction::Decrement);
                        actions.push(A11yAction::Focus);
                        actions.push(A11yAction::SetValue(String::new()));
                    }
                    A11yRole::Dialog | A11yRole::AlertDialog => {
                        actions.push(A11yAction::Focus);
                    }
                    _ => {
                        if node.focusable {
                            actions.push(A11yAction::Focus);
                        }
                    }
                }
                node.supported_actions = actions;

                // 6. Recursively build child nodes
                let mut inner_child_ids = Vec::new();
                let child_layouts = layout.map(|l| l.children.as_slice()).unwrap_or(&[]);
                let mut layout_idx = 0;
                for grandchild in children {
                    self.process_child_subtree_with_layout(
                        grandchild,
                        child_layouts,
                        &mut layout_idx,
                        depth + 1,
                        &mut inner_child_ids,
                    );
                }
                node.children = inner_child_ids;

                self.nodes.insert(id, node);
                out.push(id);
            }
            NodeBlueprint::Fragment(fragment_children) => {
                let child_layouts = layout.map(|l| l.children.as_slice()).unwrap_or(&[]);
                let mut layout_idx = 0;
                for frag_child in fragment_children {
                    self.process_child_subtree_with_layout(
                        frag_child,
                        child_layouts,
                        &mut layout_idx,
                        depth + 1,
                        out,
                    );
                }
            }
            _ => {}
        }
    }

    fn process_child_subtree_with_layout(
        &mut self,
        child: &NodeBlueprint,
        layouts: &[strata_layout::LayoutNode],
        layout_idx: &mut usize,
        depth: usize,
        out: &mut Vec<A11yNodeId>,
    ) {
        if depth >= MAX_A11Y_DEPTH {
            return;
        }
        match child {
            NodeBlueprint::Fragment(fragment_children) => {
                for frag_child in fragment_children {
                    self.process_child_subtree_with_layout(
                        frag_child,
                        layouts,
                        layout_idx,
                        depth + 1,
                        out,
                    );
                }
            }
            _ => {
                let layout = layouts.get(*layout_idx);
                *layout_idx += 1;
                self.process_child_with_layout(child, layout, depth + 1, out);
            }
        }
    }
}

fn tag_to_str(tag: &Tag) -> &'static str {
    match tag {
        Tag::Div => "div",
        Tag::Span => "span",
        Tag::Button => "button",
        Tag::Input => "input",
        Tag::Textarea => "textarea",
        Tag::Select => "select",
        Tag::A => "a",
        Tag::Img => "img",
        Tag::H1 => "h1",
        Tag::H2 => "h2",
        Tag::H3 => "h3",
        Tag::H4 => "h4",
        Tag::H5 => "h5",
        Tag::H6 => "h6",
        Tag::Ul => "ul",
        Tag::Ol => "ol",
        Tag::Li => "li",
        Tag::Nav => "nav",
        Tag::Header => "header",
        Tag::Footer => "footer",
        Tag::Main => "main",
        Tag::Aside => "aside",
        Tag::Form => "form",
        Tag::Table => "table",
        Tag::Tr => "tr",
        Tag::Td => "td",
        Tag::Th => "th",
        _ => "div",
    }
}

/// Convenience builder function to generate an `A11yTree` directly from a `NodeBlueprint`.
pub fn build_a11y_tree(root_blueprint: &NodeBlueprint) -> A11yTree {
    A11yTreeBuilder::new().build(root_blueprint)
}

/// Convenience builder function to generate an `A11yTree` with spatial bounding boxes from layout.
pub fn build_a11y_tree_with_layout(
    root_blueprint: &NodeBlueprint,
    layout_root: &strata_layout::LayoutNode,
    focused_id: Option<A11yNodeId>,
) -> A11yTree {
    A11yTreeBuilder::new()
        .with_focused_node(focused_id)
        .build_with_layout(root_blueprint, Some(layout_root))
}

/// Traverses a layout tree in parallel with accessibility node hierarchy,
/// attaching physical bounding boxes to accessibility nodes.
pub fn bind_layout_tree_bounds(layout_root: &strata_layout::LayoutNode, a11y_tree: &mut A11yTree) {
    let mut layout_boxes = Vec::new();
    collect_layout_boxes(layout_root, &mut layout_boxes);

    // Skip root web area and bind to child nodes in preorder traversal
    let mut visit_queue = Vec::new();
    visit_queue.push(a11y_tree.root_id);

    let mut a11y_node_order = Vec::new();
    while let Some(current_id) = visit_queue.pop() {
        if let Some(node) = a11y_tree.nodes.get(&current_id) {
            if current_id != a11y_tree.root_id {
                a11y_node_order.push(current_id);
            }
            for child_id in node.children.iter().rev() {
                visit_queue.push(*child_id);
            }
        }
    }

    for (node_id, bbox) in a11y_node_order.iter().zip(layout_boxes.iter()) {
        if let Some(node) = a11y_tree.nodes.get_mut(node_id) {
            node.bounds = Some(*bbox);
        }
    }
}

fn collect_layout_boxes(node: &strata_layout::LayoutNode, out: &mut Vec<A11yRect>) {
    let bbox = A11yRect::new(
        node.geometry.border_box.x,
        node.geometry.border_box.y,
        node.geometry.border_box.width,
        node.geometry.border_box.height,
    );
    out.push(bbox);

    for child in &node.children {
        collect_layout_boxes(child, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strata_dom::{Attribute, ClassList, NodeBlueprint, Tag};

    fn el(tag: Tag, props: &[(&str, &str)], children: Vec<NodeBlueprint>) -> NodeBlueprint {
        NodeBlueprint::Element {
            tag,
            classes: ClassList::default(),
            attributes: props
                .iter()
                .map(|(k, v)| Attribute::Property {
                    key: (*k).to_string(),
                    value: (*v).to_string(),
                })
                .collect(),
            children,
        }
    }

    #[test]
    fn accessible_name_reads_alt_guards_empty_aria_label_and_falls_back_to_title() {
        let root = el(
            Tag::Div,
            &[],
            vec![
                // 1. Empty aria-label="" must NOT clobber child text.
                el(
                    Tag::Button,
                    &[("aria-label", "")],
                    vec![NodeBlueprint::Text("Save Changes".to_string())],
                ),
                // 2. Image with `alt` gets accessible name from `alt`.
                el(Tag::Img, &[("alt", "Satellite Preview")], vec![]),
                // 3. Icon-only button with `alt` gets accessible name from `alt`.
                el(Tag::Button, &[("alt", "Toggle Layers")], vec![]),
                // 4. Icon-only button with only `title` falls back to `title` without duplicating description.
                el(Tag::Button, &[("title", "Reset Camera")], vec![]),
                // 5. Non-empty `aria-label` beats `alt`, child text, and `title` (and `title` remains description).
                el(
                    Tag::Button,
                    &[
                        ("aria-label", "Launch Mission"),
                        ("alt", "Ignored Alt"),
                        ("title", "Starts autonomous flight"),
                    ],
                    vec![NodeBlueprint::Text("Go".to_string())],
                ),
            ],
        );

        let tree = build_a11y_tree(&root);
        let root_div = tree.nodes[&tree.root_id].children[0];
        let kids = &tree.nodes[&root_div].children;
        assert_eq!(kids.len(), 5);

        assert_eq!(tree.nodes[&kids[0]].name.as_deref(), Some("Save Changes"));
        assert_eq!(tree.nodes[&kids[1]].name.as_deref(), Some("Satellite Preview"));
        assert_eq!(tree.nodes[&kids[2]].name.as_deref(), Some("Toggle Layers"));
        assert_eq!(tree.nodes[&kids[3]].name.as_deref(), Some("Reset Camera"));
        assert_eq!(tree.nodes[&kids[3]].description, None);
        assert_eq!(tree.nodes[&kids[4]].name.as_deref(), Some("Launch Mission"));
        assert_eq!(
            tree.nodes[&kids[4]].description.as_deref(),
            Some("Starts autonomous flight")
        );
    }
}

