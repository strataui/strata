//! Reactive Dependency Graph Visualizer, Causal Root Tracer, and Infinite Cascade Cycle Guard.

use crate::inspector::SourceLocation;

/// Node type in the reactive dependency graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReactiveNodeKind {
    Signal(usize),
    Memo(usize),
    Effect(usize),
    UiBinding(u32),
}

/// Directed edge representing dependency or trigger propagation between reactive nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReactiveEdge {
    pub from: ReactiveNodeKind,
    pub to: ReactiveNodeKind,
    pub propagation_count: u32,
}

/// Telemetry metadata for a single node in the reactive graph.
#[derive(Debug, Clone, PartialEq)]
pub struct ReactiveNodeInfo {
    pub id: ReactiveNodeKind,
    pub label: String,
    pub source: SourceLocation,
    pub version: u64,
    pub is_dirty: bool,
    pub updates_per_second: u32,
}

/// Snapshot of the complete reactive dependency graph at a point in time.
#[derive(Debug, Clone, PartialEq)]
pub struct ReactiveGraphSnapshot {
    pub timestamp_us: u64,
    pub nodes: Vec<ReactiveNodeInfo>,
    pub edges: Vec<ReactiveEdge>,
}

/// Causal trace identifying what triggered a specific render / update.
#[derive(Debug, Clone, PartialEq)]
pub struct CausalTriggerTrace {
    pub root_signal: usize,
    pub signal_name: String,
    pub source: SourceLocation,
    pub affected_memos: Vec<usize>,
    pub affected_effects: Vec<usize>,
    pub reconciled_elements: Vec<u32>,
    pub duration_us: u32,
}

/// Re-render frequency heatmap classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReRenderHeatmapLevel {
    /// Green: <= 1 update per second (Optimal)
    Optimal,
    /// Yellow: 2 - 10 updates per second (Expected during interaction)
    Expected,
    /// Orange: 11 - 59 updates per second (High frequency)
    High,
    /// Red (Flashing): >= 60 updates per second (Potential thrashing)
    Thrashing,
}

impl ReRenderHeatmapLevel {
    pub fn from_frequency(updates_per_second: u32) -> Self {
        match updates_per_second {
            0..=1 => Self::Optimal,
            2..=10 => Self::Expected,
            11..=59 => Self::High,
            _ => Self::Thrashing,
        }
    }
}

/// Infinite loop & circular dependency detection engine.
pub struct ReactiveCycleGuard {
    max_iterations: u32,
    active_trail: Vec<ReactiveNodeKind>,
}

impl Default for ReactiveCycleGuard {
    fn default() -> Self {
        Self::new(100)
    }
}

impl ReactiveCycleGuard {
    pub fn new(max_iterations: u32) -> Self {
        Self {
            max_iterations,
            active_trail: Vec::new(),
        }
    }

    /// Records entering a node during reactive propagation. Returns an error if a cycle is detected.
    pub fn enter_node(&mut self, node: ReactiveNodeKind) -> Result<(), ReactiveCycleError> {
        if self.active_trail.len() as u32 >= self.max_iterations {
            return Err(ReactiveCycleError {
                detected_cycle: self.active_trail.clone(),
                culprit: node,
            });
        }

        if let Some(pos) = self.active_trail.iter().position(|&n| n == node) {
            let cycle = self.active_trail[pos..].to_vec();
            return Err(ReactiveCycleError {
                detected_cycle: cycle,
                culprit: node,
            });
        }

        self.active_trail.push(node);
        Ok(())
    }

    /// Exits a node when its evaluation completes.
    pub fn exit_node(&mut self) {
        self.active_trail.pop();
    }

    /// Clears the active evaluation trail.
    pub fn reset(&mut self) {
        self.active_trail.clear();
    }
}

/// Error returned when an infinite cascade or circular dependency is detected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReactiveCycleError {
    pub detected_cycle: Vec<ReactiveNodeKind>,
    pub culprit: ReactiveNodeKind,
}
