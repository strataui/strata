//! Last-Write-Wins (LWW) Register with deterministic conflict resolution.

use crate::clock::LamportClock;

/// Conflict-free register where the update with the highest Lamport clock wins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LwwRegister<T> {
    value: T,
    timestamp: LamportClock,
}

impl<T: Clone> LwwRegister<T> {
    pub fn new(initial: T, timestamp: LamportClock) -> Self {
        Self {
            value: initial,
            timestamp,
        }
    }

    pub fn get(&self) -> &T {
        &self.value
    }

    pub fn timestamp(&self) -> LamportClock {
        self.timestamp
    }

    /// Sets a new value if the provided timestamp is strictly greater than current.
    pub fn set(&mut self, value: T, timestamp: LamportClock) -> bool {
        if timestamp > self.timestamp {
            self.value = value;
            self.timestamp = timestamp;
            true
        } else {
            false
        }
    }

    /// Merges another LWW register, converging deterministically to the newer state.
    pub fn merge(&mut self, other: &LwwRegister<T>) -> bool {
        if other.timestamp > self.timestamp {
            self.value = other.value.clone();
            self.timestamp = other.timestamp;
            true
        } else {
            false
        }
    }
}
