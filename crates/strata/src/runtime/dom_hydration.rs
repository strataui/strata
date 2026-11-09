//! Live Client-Side DOM Mutation and Dynamic Hydration Engine.
//!
//! Synchronizes in-memory ECMAScript `DomNode` mutations from `strata_js`
//! into the live `AppRuntime` presentation and layout tree without page reloads.

use std::sync::mpsc::{channel, Sender};
use strata_dom::{Attribute, ClassList, EntityId, NodeBlueprint, Tag};
use strata_js::{DomMutationRecord, DomMutationType, DomNode, DomNodeType};
use strata_layout::LayoutNode;

use super::AppRuntime;

/// Maps a tag string to a strongly-typed Strata DOM `Tag`.
pub fn parse_dom_tag(tag: &str) -> Tag {
    match tag.to_ascii_lowercase().as_str() {
        "div" | "html" | "body" => Tag::Div,
        "span" => Tag::Span,
        "p" => Tag::P,
        "h1" => Tag::H1,
        "h2" => Tag::H2,
        "h3" => Tag::H3,
        "h4" => Tag::H4,
        "h5" => Tag::H5,
        "h6" => Tag::H6,
        "button" => Tag::Button,
        "input" => Tag::Input,
        "select" => Tag::Select,
        "option" => Tag::Option,
        "textarea" => Tag::Textarea,
        "label" => Tag::Label,
        "form" => Tag::Form,
        "header" => Tag::Header,
        "nav" => Tag::Nav,
        "main" => Tag::Main,
        "section" => Tag::Section,
        "aside" => Tag::Aside,
        "footer" => Tag::Footer,
        "ul" => Tag::Ul,
        "ol" => Tag::Ol,
        "li" => Tag::Li,
        "a" => Tag::A,
        "img" => Tag::Img,
        "svg" => Tag::Svg,
        "path" => Tag::Path,
        "canvas" => Tag::Canvas,
        "table" => Tag::Table,
        "thead" => Tag::Thead,
        "tbody" => Tag::Tbody,
        "tr" => Tag::Tr,
        "th" => Tag::Th,
        "td" => Tag::Td,
        "hr" => Tag::Hr,
        "br" => Tag::Br,
        _ => Tag::Div,
    }
}

/// Recursively converts a `strata_js::DomNode` into a native Strata `NodeBlueprint`
/// assigning valid EntityIds and retaining DOM node UIDs.
pub fn dom_node_to_blueprint(node: &DomNode) -> NodeBlueprint {
    let inner = node.inner.borrow();
    match &inner.node_type {
        DomNodeType::Text(text) => NodeBlueprint::Text(text.clone()),
        DomNodeType::Comment(_) => NodeBlueprint::Empty,
        DomNodeType::Element {
            tag,
            attributes,
            style,
        } => {
            let entity_id = EntityId::next();
            let mut attrs: Vec<Attribute> = Vec::with_capacity(attributes.len() + 3);
            attrs.push(Attribute::Entity(entity_id));
            attrs.push(Attribute::property(
                "data-strata-node-uid",
                node.uid().to_string(),
            ));

            let mut class_str = String::new();
            for (k, v) in attributes {
                if k == "class" {
                    class_str = v.clone();
                } else if k != "data-strata-node-uid" {
                    attrs.push(Attribute::property(k.clone(), v.clone()));
                }
            }

            if !style.is_empty() {
                let formatted_style = style
                    .iter()
                    .map(|(k, v)| format!("{k}: {v};"))
                    .collect::<Vec<_>>()
                    .join(" ");
                attrs.push(Attribute::property("style", formatted_style));
            }

            let children: Vec<NodeBlueprint> =
                inner.children.iter().map(dom_node_to_blueprint).collect();

            let classes = if class_str.is_empty() {
                ClassList::empty()
            } else {
                ClassList::from(
                    class_str
                        .split_whitespace()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>(),
                )
            };

            NodeBlueprint::Element {
                tag: parse_dom_tag(tag.as_str()),
                classes,
                attributes: attrs,
                children,
            }
        }
    }
}

/// Recursively resolves a mutable reference to a blueprint node along an index path.
pub(crate) fn find_blueprint_node_mut<'a>(
    root: &'a mut NodeBlueprint,
    path: &[usize],
) -> Option<&'a mut NodeBlueprint> {
    let mut curr = root;
    for &idx in path {
        match curr {
            NodeBlueprint::Element { children, .. } | NodeBlueprint::Fragment(children) => {
                if idx >= children.len() {
                    return None;
                }
                curr = &mut children[idx];
            }
            _ => return None,
        }
    }
    Some(curr)
}

impl AppRuntime {
    /// Registers a background channel receiver for DOM mutations and returns the transmitting sender.
    pub fn register_dom_mutation_channel(&mut self) -> Sender<DomMutationRecord> {
        let (tx, rx) = channel();
        self.dom_mutation_receiver = Some(rx);
        tx
    }

    /// Polls pending DOM mutations from the registered channel and hydrates them into the presentation tree.
    /// Returns `true` if any mutations were processed and a frame redraw is required.
    pub fn poll_dom_mutations(&mut self, document: Option<&strata_js::Document>) -> bool {
        let records = if let Some(ref rx) = self.dom_mutation_receiver {
            let mut list = Vec::new();
            while let Ok(rec) = rx.try_recv() {
                list.push(rec);
            }
            list
        } else {
            Vec::new()
        };

        if records.is_empty() {
            false
        } else {
            self.hydrate_dom_mutations(&records, document)
        }
    }

    /// Hydrates a sequence of fine-grained DOM mutation records directly into the live layout tree.
    pub fn hydrate_dom_mutations(
        &mut self,
        records: &[DomMutationRecord],
        document: Option<&strata_js::Document>,
    ) -> bool {
        let mut any_hydrated = false;
        for record in records {
            if self.hydrate_dom_mutation(record, document) {
                any_hydrated = true;
            }
        }
        any_hydrated
    }

    /// Hydrates an individual DOM mutation record in-place without triggering a full page transpile.
    pub fn hydrate_dom_mutation(
        &mut self,
        record: &DomMutationRecord,
        document: Option<&strata_js::Document>,
    ) -> bool {
        match &record.mutation {
            DomMutationType::TextContentChanged { node_uid, new_text } => {
                let path = match self.root_blueprint.find_path_by_dom_uid(*node_uid) {
                    Some(p) => p,
                    None => return false,
                };

                // 1. Update blueprint tree node
                if let Some(bp) = find_blueprint_node_mut(&mut self.root_blueprint, &path) {
                    if let NodeBlueprint::Element { ref mut children, .. } = bp {
                        children.clear();
                        children.push(NodeBlueprint::Text(new_text.clone()));
                    }
                }

                // 2. Update layout tree node and measure text bounds
                if let Some(node) = Self::find_layout_node_mut(&mut self.layout_root, &path) {
                    node.text = Some(new_text.clone());
                    node.children.clear();
                    let is_mono = strata_layout::is_monospace_family(node.style.font_family);
                    let (w, h) = strata_layout::measure_text_bounds_styled_with_family(
                        new_text,
                        node.style.font_size,
                        node.style.line_height,
                        None,
                        is_mono,
                        node.style.font_family,
                        node.style.font_weight,
                        node.style.font_style,
                    );
                    node.style.width = strata_style::types::Dimension::Px(w);
                    node.style.height = strata_style::types::Dimension::Px(h);
                    node.geometry.content_box.width = w;
                    node.geometry.content_box.height = h;
                    node.geometry.padding_box.width = w;
                    node.geometry.padding_box.height = h;
                    node.geometry.border_box.width = w;
                    node.geometry.border_box.height = h;
                }

                // 3. Mark layout cache invalid along path and perform incremental relayout
                self.incremental_relayout(&path)
            }

            DomMutationType::AttributeSet {
                node_uid,
                name,
                value,
            } => {
                let path = match self.root_blueprint.find_path_by_dom_uid(*node_uid) {
                    Some(p) => p,
                    None => return false,
                };

                if let Some(bp) = find_blueprint_node_mut(&mut self.root_blueprint, &path) {
                    if let NodeBlueprint::Element {
                        ref mut attributes,
                        ref mut classes,
                        ..
                    } = bp
                    {
                        if name == "class" {
                            let class_list = if value.is_empty() {
                                ClassList::empty()
                            } else {
                                ClassList::from(
                                    value
                                        .split_whitespace()
                                        .map(|s| s.to_string())
                                        .collect::<Vec<_>>(),
                                )
                            };
                            *classes = class_list;
                        } else if let Some(pos) = attributes.iter().position(|a| match a {
                            Attribute::Property { key, .. } => key == name,
                            _ => false,
                        }) {
                            attributes[pos] = Attribute::property(name.clone(), value.clone());
                        } else {
                            attributes.push(Attribute::property(name.clone(), value.clone()));
                        }
                    }
                }

                if let Some(bp) = self.root_blueprint.find_by_dom_uid(*node_uid).cloned() {
                    let ctx = self.interaction_context();
                    let rebuilt = Self::build_layout_tree_with_interaction(&bp, &ctx);
                    if let Some(target) = Self::find_layout_node_mut(&mut self.layout_root, &path) {
                        target.style = rebuilt.style.clone();
                    }
                }

                self.incremental_relayout(&path)
            }

            DomMutationType::AttributeRemoved { node_uid, name } => {
                let path = match self.root_blueprint.find_path_by_dom_uid(*node_uid) {
                    Some(p) => p,
                    None => return false,
                };

                if let Some(bp) = find_blueprint_node_mut(&mut self.root_blueprint, &path) {
                    if let NodeBlueprint::Element {
                        ref mut attributes,
                        ref mut classes,
                        ..
                    } = bp
                    {
                        if name == "class" {
                            *classes = ClassList::empty();
                        } else {
                            attributes.retain(|a| match a {
                                Attribute::Property { key, .. } => key != name,
                                _ => true,
                            });
                        }
                    }
                }

                if let Some(bp) = self.root_blueprint.find_by_dom_uid(*node_uid).cloned() {
                    let ctx = self.interaction_context();
                    let rebuilt = Self::build_layout_tree_with_interaction(&bp, &ctx);
                    if let Some(target) = Self::find_layout_node_mut(&mut self.layout_root, &path) {
                        target.style = rebuilt.style.clone();
                    }
                }

                self.incremental_relayout(&path)
            }

            DomMutationType::StyleChanged {
                node_uid,
                property: _,
                value: _,
            } => {
                let path = match self.root_blueprint.find_path_by_dom_uid(*node_uid) {
                    Some(p) => p,
                    None => return false,
                };

                if let Some(doc) = document {
                    if let Some(dom_node) = doc.get_element_by_uid(*node_uid) {
                        let new_bp = dom_node_to_blueprint(&dom_node);
                        let ctx = self.interaction_context();
                        let rebuilt = Self::build_layout_tree_with_interaction(&new_bp, &ctx);
                        if let Some(target) = Self::find_layout_node_mut(&mut self.layout_root, &path) {
                            target.style = rebuilt.style.clone();
                        }
                    }
                }

                self.incremental_relayout(&path)
            }

            DomMutationType::ChildAppended {
                parent_uid,
                child_uid,
            } => {
                let parent_path = match self.root_blueprint.find_path_by_dom_uid(*parent_uid) {
                    Some(p) => p,
                    None => return false,
                };

                let child_bp = if let Some(doc) = document {
                    if let Some(child_dom) = doc.get_element_by_uid(*child_uid) {
                        dom_node_to_blueprint(&child_dom)
                    } else {
                        return false;
                    }
                } else {
                    return false;
                };

                // Append to blueprint
                if let Some(parent_bp) = find_blueprint_node_mut(&mut self.root_blueprint, &parent_path) {
                    if let NodeBlueprint::Element { ref mut children, .. } = parent_bp {
                        children.push(child_bp.clone());
                    }
                }

                // Append to layout tree
                let parent_style = Self::find_layout_node_mut(&mut self.layout_root, &parent_path)
                    .map(|n| n.style.clone());
                let child_layout =
                    Self::build_layout_tree_with_parent(&child_bp, parent_style.as_ref());
                if let Some(parent_layout) = Self::find_layout_node_mut(&mut self.layout_root, &parent_path) {
                    parent_layout.children.push(child_layout);
                }

                self.incremental_relayout(&parent_path)
            }

            DomMutationType::ChildPrepended {
                parent_uid,
                child_uid,
            } => {
                let parent_path = match self.root_blueprint.find_path_by_dom_uid(*parent_uid) {
                    Some(p) => p,
                    None => return false,
                };

                let child_bp = if let Some(doc) = document {
                    if let Some(child_dom) = doc.get_element_by_uid(*child_uid) {
                        dom_node_to_blueprint(&child_dom)
                    } else {
                        return false;
                    }
                } else {
                    return false;
                };

                // Prepend to blueprint
                if let Some(parent_bp) = find_blueprint_node_mut(&mut self.root_blueprint, &parent_path) {
                    if let NodeBlueprint::Element { ref mut children, .. } = parent_bp {
                        children.insert(0, child_bp.clone());
                    }
                }

                // Prepend to layout tree
                let parent_style = Self::find_layout_node_mut(&mut self.layout_root, &parent_path)
                    .map(|n| n.style.clone());
                let child_layout =
                    Self::build_layout_tree_with_parent(&child_bp, parent_style.as_ref());
                if let Some(parent_layout) = Self::find_layout_node_mut(&mut self.layout_root, &parent_path) {
                    parent_layout.children.insert(0, child_layout);
                }

                self.incremental_relayout(&parent_path)
            }

            DomMutationType::ChildInserted {
                parent_uid,
                child_uid,
                before_uid,
            } => {
                let parent_path = match self.root_blueprint.find_path_by_dom_uid(*parent_uid) {
                    Some(p) => p,
                    None => return false,
                };

                let child_bp = if let Some(doc) = document {
                    if let Some(child_dom) = doc.get_element_by_uid(*child_uid) {
                        dom_node_to_blueprint(&child_dom)
                    } else {
                        return false;
                    }
                } else {
                    return false;
                };

                // Find insertion index
                let insert_idx = match before_uid {
                    Some(b_uid) => {
                        let mut idx_opt = None;
                        if let Some(parent_bp) = self.root_blueprint.find_by_dom_uid(*parent_uid) {
                            if let NodeBlueprint::Element { children, .. } = parent_bp {
                                idx_opt = children.iter().position(|c| c.dom_uid() == Some(*b_uid));
                            }
                        }
                        idx_opt
                    }
                    None => None,
                };

                // Insert into blueprint
                if let Some(parent_bp) = find_blueprint_node_mut(&mut self.root_blueprint, &parent_path) {
                    if let NodeBlueprint::Element { ref mut children, .. } = parent_bp {
                        if let Some(idx) = insert_idx {
                            if idx < children.len() {
                                children.insert(idx, child_bp.clone());
                            } else {
                                children.push(child_bp.clone());
                            }
                        } else {
                            children.push(child_bp.clone());
                        }
                    }
                }

                // Insert into layout tree
                let parent_style = Self::find_layout_node_mut(&mut self.layout_root, &parent_path)
                    .map(|n| n.style.clone());
                let child_layout =
                    Self::build_layout_tree_with_parent(&child_bp, parent_style.as_ref());
                if let Some(parent_layout) = Self::find_layout_node_mut(&mut self.layout_root, &parent_path) {
                    if let Some(idx) = insert_idx {
                        if idx < parent_layout.children.len() {
                            parent_layout.children.insert(idx, child_layout);
                        } else {
                            parent_layout.children.push(child_layout);
                        }
                    } else {
                        parent_layout.children.push(child_layout);
                    }
                }

                self.incremental_relayout(&parent_path)
            }

            DomMutationType::ChildRemoved {
                parent_uid,
                child_uid,
            } => {
                let parent_path = match self.root_blueprint.find_path_by_dom_uid(*parent_uid) {
                    Some(p) => p,
                    None => return false,
                };

                // Remove from blueprint
                let mut removed_idx = None;
                if let Some(parent_bp) = find_blueprint_node_mut(&mut self.root_blueprint, &parent_path) {
                    if let NodeBlueprint::Element { ref mut children, .. } = parent_bp {
                        if let Some(pos) = children.iter().position(|c| c.dom_uid() == Some(*child_uid)) {
                            children.remove(pos);
                            removed_idx = Some(pos);
                        }
                    }
                }

                // Remove from layout tree
                if let Some(idx) = removed_idx {
                    if let Some(parent_layout) = Self::find_layout_node_mut(&mut self.layout_root, &parent_path) {
                        if idx < parent_layout.children.len() {
                            parent_layout.children.remove(idx);
                        }
                    }
                }

                self.incremental_relayout(&parent_path)
            }

            DomMutationType::ChildReplaced {
                parent_uid,
                new_child_uid,
                old_child_uid,
            } => {
                let parent_path = match self.root_blueprint.find_path_by_dom_uid(*parent_uid) {
                    Some(p) => p,
                    None => return false,
                };

                let new_child_bp = if let Some(doc) = document {
                    if let Some(child_dom) = doc.get_element_by_uid(*new_child_uid) {
                        dom_node_to_blueprint(&child_dom)
                    } else {
                        return false;
                    }
                } else {
                    return false;
                };

                let mut replaced_idx = None;
                if let Some(parent_bp) = find_blueprint_node_mut(&mut self.root_blueprint, &parent_path) {
                    if let NodeBlueprint::Element { ref mut children, .. } = parent_bp {
                        if let Some(pos) = children.iter().position(|c| c.dom_uid() == Some(*old_child_uid)) {
                            children[pos] = new_child_bp.clone();
                            replaced_idx = Some(pos);
                        }
                    }
                }

                if let Some(idx) = replaced_idx {
                    let parent_style = Self::find_layout_node_mut(&mut self.layout_root, &parent_path)
                        .map(|n| n.style.clone());
                    let new_child_layout =
                        Self::build_layout_tree_with_parent(&new_child_bp, parent_style.as_ref());
                    if let Some(parent_layout) = Self::find_layout_node_mut(&mut self.layout_root, &parent_path) {
                        if idx < parent_layout.children.len() {
                            parent_layout.children[idx] = new_child_layout;
                        }
                    }
                }

                self.incremental_relayout(&parent_path)
            }

            DomMutationType::InnerHtmlChanged {
                node_uid,
                html: _,
            } => {
                let path = match self.root_blueprint.find_path_by_dom_uid(*node_uid) {
                    Some(p) => p,
                    None => return false,
                };

                let new_children_bp: Vec<NodeBlueprint> = if let Some(doc) = document {
                    if let Some(dom_node) = doc.get_element_by_uid(*node_uid) {
                        dom_node
                            .children()
                            .iter()
                            .map(dom_node_to_blueprint)
                            .collect()
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                };

                // Update blueprint
                if let Some(bp) = find_blueprint_node_mut(&mut self.root_blueprint, &path) {
                    if let NodeBlueprint::Element { ref mut children, .. } = bp {
                        *children = new_children_bp.clone();
                    }
                }

                // Update layout tree
                let parent_style = Self::find_layout_node_mut(&mut self.layout_root, &path)
                    .map(|n| n.style.clone());
                let new_layout_children: Vec<LayoutNode> = new_children_bp
                    .iter()
                    .map(|c| Self::build_layout_tree_with_parent(c, parent_style.as_ref()))
                    .collect();

                if let Some(node) = Self::find_layout_node_mut(&mut self.layout_root, &path) {
                    node.children = new_layout_children;
                }

                self.incremental_relayout(&path)
            }
        }
    }

    /// Performs incremental layout recomputation along a dirty tree path,
    /// leveraging layout boundaries or ancestor cache invalidation.
    pub fn incremental_relayout(&mut self, dirty_path: &[usize]) -> bool {
        // Invalidate ancestor layout caches so compute_layout traverses down the dirty branch
        self.layout_root.invalidate_ancestors(dirty_path);

        // Attempt isolated subgrid/boundary layout first if an ancestor layout boundary exists
        let handled_locally = self.layout_root.compute_subgrid_or_boundary_layout(dirty_path);
        if !handled_locally {
            // Full incremental traversal: untouched sibling branches hit layout_cache.is_valid and return in O(1)
            self.layout_root
                .compute_layout(self.viewport_width, self.viewport_height);
        }

        Self::sync_layout_to_ir(&self.layout_root, &mut self.ir_graph);
        self.spatial_index
            .rebuild(&self.root_blueprint, &self.layout_root);
        self.tick_frame();
        true
    }
}
