//! Replicated Growable Array (RGA) for collaborative rich-text and list editing.

use crate::clock::LamportClock;

/// Unique item in a collaborative sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgaNode<T> {
    pub id: LamportClock,
    pub value: T,
    pub deleted: bool,
}

/// Collaborative sequence CRDT supporting concurrent inserts and deletes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgaSequence<T> {
    nodes: Vec<RgaNode<T>>,
}

impl<T: Clone> Default for RgaSequence<T> {
    fn default() -> Self {
        Self { nodes: Vec::new() }
    }
}

impl<T: Clone> RgaSequence<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts an item after `after_id` (or at the beginning if None).
    pub fn insert(&mut self, after_id: Option<LamportClock>, id: LamportClock, value: T) {
        let new_node = RgaNode {
            id,
            value,
            deleted: false,
        };

        if let Some(target) = after_id {
            if let Some(pos) = self.nodes.iter().position(|n| n.id == target) {
                // Find appropriate insertion point maintaining total ordering for concurrent edits
                let mut insert_pos = pos + 1;
                while insert_pos < self.nodes.len() && self.nodes[insert_pos].id > id {
                    insert_pos += 1;
                }
                self.nodes.insert(insert_pos, new_node);
                return;
            }
        }

        // Insert at beginning, respecting timestamp order among other root elements
        let mut insert_pos = 0;
        while insert_pos < self.nodes.len() && self.nodes[insert_pos].id > id {
            insert_pos += 1;
        }
        self.nodes.insert(insert_pos, new_node);
    }

    /// Tombstones an element with the given ID.
    pub fn delete(&mut self, id: LamportClock) -> bool {
        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == id) {
            node.deleted = true;
            true
        } else {
            false
        }
    }

    /// Returns non-deleted elements in sequence order.
    pub fn read(&self) -> Vec<T> {
        self.nodes
            .iter()
            .filter(|n| !n.deleted)
            .map(|n| n.value.clone())
            .collect()
    }

    /// Merges another RGA sequence state.
    pub fn merge(&mut self, other: &RgaSequence<T>) {
        for other_node in &other.nodes {
            if let Some(local_node) = self.nodes.iter_mut().find(|n| n.id == other_node.id) {
                if other_node.deleted {
                    local_node.deleted = true;
                }
            } else {
                // Insert node maintaining timestamp order
                let mut insert_pos = 0;
                while insert_pos < self.nodes.len() && self.nodes[insert_pos].id > other_node.id {
                    insert_pos += 1;
                }
                self.nodes.insert(insert_pos, other_node.clone());
            }
        }
    }
}
