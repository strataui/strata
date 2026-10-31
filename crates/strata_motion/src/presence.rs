//! Exit Animation Lifecycle Coordinator (<AnimatePresence> equivalent).

use strata_dom::Signal;

/// Represents an item transitioning through mount, present, or unmount states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresenceItem {
    pub key: String,
    pub is_present: bool,
}

/// Coordinates exit animations, retaining unmounting items in the render tree until animations finish.
#[derive(Clone)]
pub struct AnimatePresenceState {
    pub items: Signal<Vec<PresenceItem>>,
}

impl Default for AnimatePresenceState {
    fn default() -> Self {
        Self::new()
    }
}

impl AnimatePresenceState {
    pub fn new() -> Self {
        Self {
            items: Signal::new(Vec::new()),
        }
    }

    /// Synchronizes incoming active keys. Keys removed from the input list are marked `is_present = false`
    /// rather than being dropped immediately, allowing exit animations to run.
    pub fn sync_keys(&self, active_keys: &[String]) {
        let current = self.items.get();
        let mut updated = Vec::new();

        // Retain existing items
        for item in &current {
            if active_keys.contains(&item.key) {
                updated.push(PresenceItem {
                    key: item.key.clone(),
                    is_present: true,
                });
            } else {
                // Key removed: mark as exiting!
                updated.push(PresenceItem {
                    key: item.key.clone(),
                    is_present: false,
                });
            }
        }

        // Add newly appeared keys
        for key in active_keys {
            if !current.iter().any(|it| &it.key == key) {
                updated.push(PresenceItem {
                    key: key.clone(),
                    is_present: true,
                });
            }
        }

        self.items.set(updated);
    }

    /// Called when an item's exit animation completes to remove it from the tree.
    pub fn on_exit_complete(&self, key: &str) {
        let mut current = self.items.get();
        current.retain(|it| it.key != key);
        self.items.set(current);
    }

    /// Checks if a key is currently playing its exit animation.
    pub fn is_exiting(&self, key: &str) -> bool {
        self.items.get().iter().any(|it| it.key == key && !it.is_present)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_presence_exit_retention() {
        let presence = AnimatePresenceState::new();

        // 1. Initial 2 items
        presence.sync_keys(&["card-1".to_string(), "card-2".to_string()]);
        assert_eq!(presence.items.get().len(), 2);
        assert!(!presence.is_exiting("card-1"));

        // 2. Remove card-2: should NOT disappear yet, but be marked exiting!
        presence.sync_keys(&["card-1".to_string()]);
        assert_eq!(presence.items.get().len(), 2);
        assert!(presence.is_exiting("card-2"));

        // 3. Exit animation completes
        presence.on_exit_complete("card-2");
        assert_eq!(presence.items.get().len(), 1);
        assert_eq!(presence.items.get()[0].key, "card-1");
    }
}
