//! Memory Allocator Inspector, Thread-Local Bump Arena Telemetry, and Atom Table Tracker.

/// Snapshot telemetry for a thread-local bump arena.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArenaTelemetrySample {
    pub arena_id: usize,
    pub allocated_bytes: usize,
    pub capacity_bytes: usize,
    pub high_water_mark_bytes: usize,
    pub reset_count: u64,
}

impl ArenaTelemetrySample {
    pub fn new(
        arena_id: usize,
        allocated_bytes: usize,
        capacity_bytes: usize,
        high_water_mark_bytes: usize,
        reset_count: u64,
    ) -> Self {
        Self {
            arena_id,
            allocated_bytes,
            capacity_bytes,
            high_water_mark_bytes,
            reset_count,
        }
    }

    /// Calculates current capacity utilization percentage (0.0 - 100.0).
    pub fn utilization_pct(&self) -> f32 {
        if self.capacity_bytes == 0 {
            0.0
        } else {
            (self.allocated_bytes as f32 / self.capacity_bytes as f32) * 100.0
        }
    }
}

/// Anomaly detected during arena inspection.
#[derive(Debug, Clone, PartialEq)]
pub enum ArenaAnomaly {
    NearCapacity { arena_id: usize, usage_pct: f32 },
    ResetSkipped { arena_id: usize, frame_count: u32 },
}

/// Inspector for monitoring thread-local bump arenas across frame boundaries.
#[derive(Debug, Clone, Default)]
pub struct ArenaInspector {
    warning_threshold_pct: f32,
    samples: Vec<ArenaTelemetrySample>,
}

impl ArenaInspector {
    pub fn new(warning_threshold_pct: f32) -> Self {
        Self {
            warning_threshold_pct,
            samples: Vec::new(),
        }
    }

    /// Records telemetry from an arena and checks for potential leaks or near-capacity events.
    pub fn record_sample(&mut self, sample: ArenaTelemetrySample) -> Option<ArenaAnomaly> {
        let usage = sample.utilization_pct();
        let anomaly = if usage >= self.warning_threshold_pct {
            Some(ArenaAnomaly::NearCapacity {
                arena_id: sample.arena_id,
                usage_pct: usage,
            })
        } else {
            None
        };

        self.samples.push(sample);
        anomaly
    }

    /// Returns the recorded samples.
    pub fn samples(&self) -> &[ArenaTelemetrySample] {
        &self.samples
    }
}

/// Telemetry inspector for string atom interning cache footprint and lookup metrics.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AtomTableInspector {
    pub atom_count: usize,
    pub total_bytes: usize,
    pub lookup_hits: u64,
    pub lookup_misses: u64,
}

impl AtomTableInspector {
    pub fn new(atom_count: usize, total_bytes: usize, lookup_hits: u64, lookup_misses: u64) -> Self {
        Self {
            atom_count,
            total_bytes,
            lookup_hits,
            lookup_misses,
        }
    }

    /// Computes atom lookup hit rate percentage (0.0 - 100.0).
    pub fn hit_rate(&self) -> f32 {
        let total = self.lookup_hits + self.lookup_misses;
        if total == 0 {
            100.0
        } else {
            (self.lookup_hits as f32 / total as f32) * 100.0
        }
    }
}

/// Memory budget health status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeapBudgetStatus {
    Normal,
    Warning(usize),
    Critical(usize),
}

/// Heap allocation tracker and leak detector.
#[derive(Debug, Clone)]
pub struct HeapAllocationTracker {
    pub warning_threshold_bytes: usize,
    pub critical_threshold_bytes: usize,
    history: Vec<usize>,
}

impl Default for HeapAllocationTracker {
    fn default() -> Self {
        Self {
            warning_threshold_bytes: 15 * 1024 * 1024,   // 15 MB
            critical_threshold_bytes: 30 * 1024 * 1024,  // 30 MB
            history: Vec::new(),
        }
    }
}

impl HeapAllocationTracker {
    pub fn new(warning_threshold_bytes: usize, critical_threshold_bytes: usize) -> Self {
        Self {
            warning_threshold_bytes,
            critical_threshold_bytes,
            history: Vec::new(),
        }
    }

    /// Records current heap consumption and returns budget status.
    pub fn record_allocation(&mut self, bytes: usize) -> HeapBudgetStatus {
        self.history.push(bytes);
        if bytes >= self.critical_threshold_bytes {
            HeapBudgetStatus::Critical(bytes)
        } else if bytes >= self.warning_threshold_bytes {
            HeapBudgetStatus::Warning(bytes)
        } else {
            HeapBudgetStatus::Normal
        }
    }

    /// Detects monotonic collection growth indicating potential memory leaks over `window_size` samples.
    pub fn is_monotonically_growing(&self, window_size: usize) -> bool {
        if self.history.len() < window_size || window_size < 2 {
            return false;
        }

        let slice = &self.history[self.history.len() - window_size..];
        for i in 1..slice.len() {
            if slice[i] <= slice[i - 1] {
                return false;
            }
        }
        true
    }
}
