//! Observed-Remove Set (OR-Set) with add-tag tracking.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use crate::clock::LamportClock;

/// Conflict-free set where elements are tagged with unique add timestamps.
/// An element is present if its add-tags have not all been observed as removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrSet<T: Eq + Hash + Clone> {
    elements: HashMap<T, HashSet<LamportClock>>,
    tombstones: HashSet<LamportClock>,
}

impl<T: Eq + Hash + Clone> Default for OrSet<T> {
    fn default() -> Self {
        Self {
            elements: HashMap::new(),
            tombstones: HashSet::new(),
        }
    }
}

impl<T: Eq + Hash + Clone> OrSet<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an element with a unique timestamp tag.
    pub fn add(&mut self, item: T, tag: LamportClock) {
        self.elements.entry(item).or_default().insert(tag);
    }

    /// Removes an element by moving all its current add-tags to the tombstone set.
    pub fn remove(&mut self, item: &T) {
        if let Some(tags) = self.elements.remove(item) {
            for tag in tags {
                self.tombstones.insert(tag);
            }
        }
    }

    /// Checks if the element is currently observed in the set.
    pub fn contains(&self, item: &T) -> bool {
        if let Some(tags) = self.elements.get(item) {
            tags.iter().any(|t| !self.tombstones.contains(t))
        } else {
            false
        }
    }

    /// Returns all elements currently present in the set.
    pub fn read(&self) -> Vec<T> {
        self.elements
            .iter()
            .filter(|(_, tags)| tags.iter().any(|t| !self.tombstones.contains(t)))
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// Merges another OR-Set state.
    pub fn merge(&mut self, other: &OrSet<T>) {
        // Union tombstones
        for &t in &other.tombstones {
            self.tombstones.insert(t);
        }

        // Union elements and tags
        for (item, tags) in &other.elements {
            let entry = self.elements.entry(item.clone()).or_default();
            for &t in tags {
                entry.insert(t);
            }
        }
    }
}
