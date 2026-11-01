//! Reversible Time-Travel State Debugger and Deterministic Snapshot Ring Buffer.

/// Unique identifier for a recorded state snapshot.
pub type SnapshotId = u64;

/// Delta difference for a single signal between snapshots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotDelta {
    pub signal_id: usize,
    pub old_val: Option<String>,
    pub new_val: Option<String>,
}

/// Point-in-time deterministic snapshot of the application's reactive state and UI tree.
#[derive(Debug, Clone, PartialEq)]
pub struct StateSnapshot {
    pub id: SnapshotId,
    pub timestamp_us: u64,
    pub label: String,
    pub signal_values: Vec<(usize, String)>,
    pub ir_hash: u64,
}

impl StateSnapshot {
    pub fn new(
        id: SnapshotId,
        timestamp_us: u64,
        label: impl Into<String>,
        signal_values: Vec<(usize, String)>,
        ir_hash: u64,
    ) -> Self {
        Self {
            id,
            timestamp_us,
            label: label.into(),
            signal_values,
            ir_hash,
        }
    }

    /// Finds the recorded value for a given signal id.
    pub fn get_signal(&self, signal_id: usize) -> Option<&str> {
        self.signal_values
            .iter()
            .find(|(id, _)| *id == signal_id)
            .map(|(_, val)| val.as_str())
    }
}

/// Time-travel state manager supporting scrubbing, step-back, and step-forward.
#[derive(Debug, Clone)]
pub struct TimeTravelDebugger {
    snapshots: Vec<StateSnapshot>,
    current_index: usize,
    max_snapshots: usize,
    next_id: SnapshotId,
}

impl Default for TimeTravelDebugger {
    fn default() -> Self {
        Self::new(128)
    }
}

impl TimeTravelDebugger {
    pub fn new(max_snapshots: usize) -> Self {
        Self {
            snapshots: Vec::new(),
            current_index: 0,
            max_snapshots: max_snapshots.max(1),
            next_id: 1,
        }
    }

    /// Records a new snapshot into history. If time-traveled into the past, truncates future states.
    pub fn record_snapshot(
        &mut self,
        timestamp_us: u64,
        label: impl Into<String>,
        signal_values: Vec<(usize, String)>,
        ir_hash: u64,
    ) -> SnapshotId {
        let id = self.next_id;
        self.next_id += 1;

        if !self.snapshots.is_empty() && self.current_index < self.snapshots.len() - 1 {
            self.snapshots.truncate(self.current_index + 1);
        }

        let snapshot = StateSnapshot::new(id, timestamp_us, label, signal_values, ir_hash);
        self.snapshots.push(snapshot);

        if self.snapshots.len() > self.max_snapshots {
            self.snapshots.remove(0);
        }

        self.current_index = self.snapshots.len().saturating_sub(1);
        id
    }

    /// Total number of stored snapshots.
    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    /// Returns true if no snapshots have been recorded.
    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }

    /// Current active snapshot index in history.
    pub fn current_index(&self) -> usize {
        self.current_index
    }

    /// Current active state snapshot.
    pub fn current_snapshot(&self) -> Option<&StateSnapshot> {
        self.snapshots.get(self.current_index)
    }

    /// Can step backward into past history.
    pub fn can_step_back(&self) -> bool {
        self.current_index > 0
    }

    /// Can step forward toward recent history.
    pub fn can_step_forward(&self) -> bool {
        !self.snapshots.is_empty() && self.current_index < self.snapshots.len() - 1
    }

    /// Steps one snapshot backward.
    pub fn step_back(&mut self) -> Option<&StateSnapshot> {
        if self.can_step_back() {
            self.current_index -= 1;
            self.current_snapshot()
        } else {
            None
        }
    }

    /// Steps one snapshot forward.
    pub fn step_forward(&mut self) -> Option<&StateSnapshot> {
        if self.can_step_forward() {
            self.current_index += 1;
            self.current_snapshot()
        } else {
            None
        }
    }

    /// Scrubs directly to a specific snapshot ID.
    pub fn scrub_to(&mut self, target_id: SnapshotId) -> Option<&StateSnapshot> {
        if let Some(pos) = self.snapshots.iter().position(|s| s.id == target_id) {
            self.current_index = pos;
            Some(&self.snapshots[pos])
        } else {
            None
        }
    }

    /// Computes signal deltas between two snapshots in history.
    pub fn compute_delta(&self, from_id: SnapshotId, to_id: SnapshotId) -> Option<Vec<SnapshotDelta>> {
        let from_snap = self.snapshots.iter().find(|s| s.id == from_id)?;
        let to_snap = self.snapshots.iter().find(|s| s.id == to_id)?;

        let mut deltas = Vec::new();

        // Check for modified or removed signals from `from`
        for (sig_id, old_val) in &from_snap.signal_values {
            let new_val = to_snap.get_signal(*sig_id);
            if new_val != Some(old_val.as_str()) {
                deltas.push(SnapshotDelta {
                    signal_id: *sig_id,
                    old_val: Some(old_val.clone()),
                    new_val: new_val.map(|s| s.to_string()),
                });
            }
        }

        // Check for newly introduced signals in `to`
        for (sig_id, new_val) in &to_snap.signal_values {
            if from_snap.get_signal(*sig_id).is_none() {
                deltas.push(SnapshotDelta {
                    signal_id: *sig_id,
                    old_val: None,
                    new_val: Some(new_val.clone()),
                });
            }
        }

        Some(deltas)
    }

    /// List of all recorded snapshots.
    pub fn snapshots(&self) -> &[StateSnapshot] {
        &self.snapshots
    }
}
