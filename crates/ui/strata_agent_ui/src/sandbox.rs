//! Sandboxed execution environment applying validated UI patches to an isolated DOM tree.

use std::collections::HashMap;
use crate::delta::AgentPatch;
use crate::schema::AgentSchemaValidator;

/// An isolated virtual node in the agent-generated UI hierarchy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxedNode {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub tag: String,
    pub props: HashMap<String, String>,
    pub text: Option<String>,
    pub children: Vec<u64>,
}

/// Sandboxed container hosting agent-generated UI components.
#[derive(Debug, Clone)]
pub struct AgentUiSandbox {
    pub nodes: HashMap<u64, SandboxedNode>,
    pub root_id: Option<u64>,
    validator: AgentSchemaValidator,
}

impl Default for AgentUiSandbox {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentUiSandbox {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            root_id: None,
            validator: AgentSchemaValidator::new(),
        }
    }

    pub fn with_validator(validator: AgentSchemaValidator) -> Self {
        Self {
            nodes: HashMap::new(),
            root_id: None,
            validator,
        }
    }

    /// Validates and applies a streaming agent patch. Returns Err if patch violates security rules.
    pub fn apply_patch(&mut self, patch: AgentPatch) -> Result<(), String> {
        match patch {
            AgentPatch::InsertNode { node_id, parent_id, tag, props, text } => {
                // 1. Validate tag
                self.validator.validate_tag(&tag)?;

                // 2. Validate all props
                for (k, v) in &props {
                    self.validator.validate_attribute(k, v)?;
                }

                // 3. Register node
                let node = SandboxedNode {
                    id: node_id,
                    parent_id,
                    tag,
                    props,
                    text,
                    children: Vec::new(),
                };
                self.nodes.insert(node_id, node);

                // 4. Update parent or set root
                if let Some(parent) = parent_id {
                    if let Some(parent_node) = self.nodes.get_mut(&parent) {
                        if !parent_node.children.contains(&node_id) {
                            parent_node.children.push(node_id);
                        }
                    }
                } else if self.root_id.is_none() {
                    self.root_id = Some(node_id);
                }

                Ok(())
            }
            AgentPatch::MutateProps { node_id, key, value } => {
                self.validator.validate_attribute(&key, &value)?;
                if let Some(node) = self.nodes.get_mut(&node_id) {
                    node.props.insert(key, value);
                    Ok(())
                } else {
                    Err(format!("Node {} not found for MutateProps", node_id))
                }
            }
            AgentPatch::UpdateText { node_id, text } => {
                if let Some(node) = self.nodes.get_mut(&node_id) {
                    node.text = Some(text);
                    Ok(())
                } else {
                    Err(format!("Node {} not found for UpdateText", node_id))
                }
            }
            AgentPatch::RemoveNode { node_id } => {
                if let Some(node) = self.nodes.remove(&node_id) {
                    if let Some(parent_id) = node.parent_id {
                        if let Some(parent_node) = self.nodes.get_mut(&parent_id) {
                            parent_node.children.retain(|&c| c != node_id);
                        }
                    }
                    if self.root_id == Some(node_id) {
                        self.root_id = None;
                    }
                    Ok(())
                } else {
                    Err(format!("Node {} not found for RemoveNode", node_id))
                }
            }
        }
    }

    /// Renders sandbox tree to a simple debug HTML/RSX string.
    pub fn render_to_html(&self) -> String {
        if let Some(root) = self.root_id {
            self.render_node(root)
        } else {
            String::new()
        }
    }

    fn render_node(&self, id: u64) -> String {
        let node = match self.nodes.get(&id) {
            Some(n) => n,
            None => return String::new(),
        };

        let mut out = format!("<{}", node.tag);
        for (k, v) in &node.props {
            out.push_str(&format!(" {}=\"{}\"", k, v));
        }
        out.push('>');

        if let Some(text) = &node.text {
            out.push_str(text);
        }

        for child_id in &node.children {
            out.push_str(&self.render_node(*child_id));
        }

        out.push_str(&format!("</{}>", node.tag));
        out
    }
}
