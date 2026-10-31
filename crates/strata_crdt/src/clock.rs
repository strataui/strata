//! Logical Lamport timestamps and Vector clocks for causal ordering.

use std::cmp::Ordering;
use std::collections::HashMap;

/// Peer identifier in a distributed CRDT mesh.
pub type PeerId = u64;

/// Lamport timestamp guaranteeing monotonic causal total ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct LamportClock {
    pub counter: u64,
    pub peer: PeerId,
}

impl LamportClock {
    pub const fn new(counter: u64, peer: PeerId) -> Self {
        Self { counter, peer }
    }

    /// Advances the clock upon a local event.
    pub fn tick(&mut self) -> Self {
        self.counter += 1;
        *self
    }

    /// Merges an incoming remote clock, ensuring monotonicity.
    pub fn witness(&mut self, remote: LamportClock) -> Self {
        self.counter = self.counter.max(remote.counter) + 1;
        *self
    }
}

impl PartialOrd for LamportClock {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LamportClock {
    fn cmp(&self, other: &Self) -> Ordering {
        self.counter.cmp(&other.counter).then_with(|| self.peer.cmp(&other.peer))
    }
}

/// Vector clock tracking causal dependencies across multiple peers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VectorClock {
    pub entries: HashMap<PeerId, u64>,
}

impl VectorClock {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn inc(&mut self, peer: PeerId) -> u64 {
        let val = self.entries.entry(peer).or_insert(0);
        *val += 1;
        *val
    }

    pub fn get(&self, peer: PeerId) -> u64 {
        self.entries.get(&peer).copied().unwrap_or(0)
    }

    /// Merges remote vector clock by taking component-wise maximum.
    pub fn merge(&mut self, other: &VectorClock) {
        for (&peer, &clock) in &other.entries {
            let current = self.entries.entry(peer).or_insert(0);
            *current = (*current).max(clock);
        }
    }

    /// Returns true if `self` dominates or equals `other` across all peers.
    pub fn dominates(&self, other: &VectorClock) -> bool {
        for (&peer, &remote_clock) in &other.entries {
            if self.get(peer) < remote_clock {
                return false;
            }
        }
        true
    }
}
