//! Differential tree updates for low-overhead screen-reader synchronization.

use crate::node::{A11yNode, A11yNodeId, A11yTree};

/// Incremental tree update payload dispatched to OS screen readers (AccessKit / UIA / NSAccessibility / AT-SPI).
#[derive(Debug, Clone, PartialEq)]
pub struct A11yTreeUpdate {
    /// Newly added or mutated nodes.
    pub nodes: Vec<(A11yNodeId, A11yNode)>,
    /// Node IDs removed from the accessibility hierarchy.
    pub removed_nodes: Vec<A11yNodeId>,
    /// Currently focused node, if any.
    pub focus: Option<A11yNodeId>,
}

impl A11yTreeUpdate {
    pub fn empty() -> Self {
        Self {
            nodes: Vec::new(),
            removed_nodes: Vec::new(),
            focus: None,
        }
    }

    /// Computes minimal diff between two successive trees.
    pub fn diff(old_tree: Option<&A11yTree>, new_tree: &A11yTree) -> Self {
        let mut updated_nodes = Vec::new();
        let mut removed_nodes = Vec::new();

        if let Some(old) = old_tree {
            // Find mutated or added nodes
            for (id, new_node) in &new_tree.nodes {
                match old.nodes.get(id) {
                    Some(old_node) if old_node == new_node => {
                        // Unchanged, skip
                    }
                    _ => {
                        updated_nodes.push((*id, new_node.clone()));
                    }
                }
            }

            // Find removed nodes
            for id in old.nodes.keys() {
                if !new_tree.nodes.contains_key(id) {
                    removed_nodes.push(*id);
                }
            }
        } else {
            // Initial full tree push
            for (id, node) in &new_tree.nodes {
                updated_nodes.push((*id, node.clone()));
            }
        }

        // Detect focused node
        let focus = new_tree
            .nodes
            .values()
            .find(|n| n.focused)
            .map(|n| n.id);

        Self {
            nodes: updated_nodes,
            removed_nodes,
            focus,
        }
    }
}
