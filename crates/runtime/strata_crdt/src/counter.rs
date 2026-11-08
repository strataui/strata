//! Distributed Positive-Negative (PN) Counter.

use std::collections::HashMap;
use crate::clock::PeerId;

/// Conflict-free counter supporting distributed increments and decrements.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PnCounter {
    positive: HashMap<PeerId, u64>,
    negative: HashMap<PeerId, u64>,
}

impl PnCounter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Increments the counter on the given peer.
    pub fn inc(&mut self, peer: PeerId, amount: u64) {
        let entry = self.positive.entry(peer).or_insert(0);
        *entry += amount;
    }

    /// Decrements the counter on the given peer.
    pub fn dec(&mut self, peer: PeerId, amount: u64) {
        let entry = self.negative.entry(peer).or_insert(0);
        *entry += amount;
    }

    /// Computes the net aggregate counter value: sum(positive) - sum(negative).
    pub fn value(&self) -> i64 {
        let pos_sum: u64 = self.positive.values().sum();
        let neg_sum: u64 = self.negative.values().sum();
        pos_sum as i64 - neg_sum as i64
    }

    /// Merges another PN-counter by taking component-wise max of positive and negative maps.
    pub fn merge(&mut self, other: &PnCounter) {
        for (&peer, &val) in &other.positive {
            let entry = self.positive.entry(peer).or_insert(0);
            *entry = (*entry).max(val);
        }
        for (&peer, &val) in &other.negative {
            let entry = self.negative.entry(peer).or_insert(0);
            *entry = (*entry).max(val);
        }
    }

    /// Returns the positive and negative tallies for a specific peer.
    pub fn peer_values(&self, peer: PeerId) -> (u64, u64) {
        let pos = self.positive.get(&peer).copied().unwrap_or(0);
        let neg = self.negative.get(&peer).copied().unwrap_or(0);
        (pos, neg)
    }
}
