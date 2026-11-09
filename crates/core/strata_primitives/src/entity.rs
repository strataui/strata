//! Entity identification primitive for cross-subsystem DOM and layout synchronization.

use std::sync::atomic::{AtomicU64, Ordering};

/// Globally unique 64-bit entity identifier associating reactive blueprints,
/// DOM elements, computed layout nodes, and spatial indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct EntityId(pub u64);

impl EntityId {
    /// Sentinel unassigned entity ID.
    pub const NONE: Self = Self(0);

    /// Constructs an entity ID from a raw 64-bit integer.
    #[inline]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Allocates the next sequentially unique entity ID across all threads.
    #[inline]
    pub fn next() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    /// Returns the raw 64-bit integer.
    #[inline]
    pub const fn raw(&self) -> u64 {
        self.0
    }

    /// Returns true if this entity ID is valid (non-zero).
    #[inline]
    pub const fn is_valid(&self) -> bool {
        self.0 != 0
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EntityId({})", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_id_basics() {
        assert_eq!(EntityId::NONE.raw(), 0);
        assert!(!EntityId::NONE.is_valid());

        let id1 = EntityId::new(42);
        assert_eq!(id1.raw(), 42);
        assert!(id1.is_valid());
        assert_eq!(format!("{}", id1), "EntityId(42)");

        let next1 = EntityId::next();
        let next2 = EntityId::next();
        assert!(next1.is_valid());
        assert!(next2.is_valid());
        assert_eq!(next2.raw(), next1.raw() + 1);
    }
}
