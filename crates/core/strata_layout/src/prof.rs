//! Opt-in layout profiling counters gated behind `STRATA_PERF_LOG`.
//!
//! The counters accumulate inside one root `compute_layout` invocation and are
//! printed as a ranked table when the outermost call completes. All entry
//! points are cheap no-ops unless the environment variable is present.

use std::cell::RefCell;

/// Individual layout work category tracked by the profiler.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Slot {
    Block,
    Flex,
    Grid,
    Table,
    Inline,
    Intrinsic,
    StyleClone,
    Geometry,
    Exclusions,
}

impl Slot {
    fn index(self) -> usize {
        match self {
            Slot::Block => 0,
            Slot::Flex => 1,
            Slot::Grid => 2,
            Slot::Table => 3,
            Slot::Inline => 4,
            Slot::Intrinsic => 5,
            Slot::StyleClone => 6,
            Slot::Geometry => 7,
            Slot::Exclusions => 8,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Slot::Block => "block_layout",
            Slot::Flex => "flex_solver",
            Slot::Grid => "grid_solver",
            Slot::Table => "table_solver",
            Slot::Inline => "inline_formatting",
            Slot::Intrinsic => "intrinsic_min_max",
            Slot::StyleClone => "style_clone",
            Slot::Geometry => "geometry_apply",
            Slot::Exclusions => "exclusion_space",
        }
    }
}

const SLOT_COUNT: usize = 9;

#[derive(Clone, Copy, Default)]
struct Sample {
    count: u64,
    nanos: u128,
}

struct State {
    enabled: bool,
    depth: usize,
    max_depth: usize,
    max_width: usize,
    total_nodes: usize,
    samples: [Sample; SLOT_COUNT],
}

impl Default for State {
    fn default() -> Self {
        Self {
            enabled: std::env::var_os("STRATA_PERF_LOG").is_some(),
            depth: 0,
            max_depth: 0,
            max_width: 0,
            total_nodes: 0,
            samples: [Sample::default(); SLOT_COUNT],
        }
    }
}

struct Frame {
    slot: Slot,
    start: std::time::Instant,
    child_nanos: u128,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
    static FRAMES: RefCell<Vec<Frame>> = const { RefCell::new(Vec::new()) };
}

/// Whether layout profiling is enabled.
pub fn enabled() -> bool {
    STATE.with(|s| s.borrow().enabled)
}

/// Marks the beginning of a root layout call. Returns true when this call is
/// the outermost layout invocation and therefore owns the report lifecycle.
pub fn enter_layout() -> bool {
    if !enabled() {
        return false;
    }
    STATE.with(|s| {
        let mut state = s.borrow_mut();
        let is_root = state.depth == 0;
        if is_root {
            state.samples = [Sample::default(); SLOT_COUNT];
            state.max_depth = 0;
            state.max_width = 0;
            state.total_nodes = 0;
            FRAMES.with(|frames| frames.borrow_mut().clear());
        }
        state.depth += 1;
        if state.depth > state.max_depth {
            state.max_depth = state.depth;
        }
        is_root
    })
}

/// Marks the end of a layout call, printing the ranked report at the root.
pub fn leave_layout(is_root: bool) {
    if !enabled() {
        return;
    }
    if is_root {
        report();
    }
    STATE.with(|s| {
        let mut state = s.borrow_mut();
        state.depth = state.depth.saturating_sub(1);
    });
}

/// Records one child count observation for the widest container report.
pub fn observe_width(width: usize) {
    if !enabled() {
        return;
    }
    STATE.with(|s| {
        let mut state = s.borrow_mut();
        state.total_nodes += 1;
        if width > state.max_width {
            state.max_width = width;
        }
    });
}

/// Adds a measured duration to a profiling slot.
pub fn record(slot: Slot, elapsed: std::time::Duration) {
    if !enabled() {
        return;
    }
    record_self(slot, elapsed.as_nanos());
}

fn record_self(slot: Slot, nanos: u128) {
    STATE.with(|s| {
        let mut state = s.borrow_mut();
        let sample = &mut state.samples[slot.index()];
        sample.count += 1;
        sample.nanos += nanos;
    });
}

fn report() {
    let (mut entries, total, max_depth, max_width) = STATE.with(|s| {
        let state = s.borrow();
        let mut entries: Vec<(Slot, Sample)> = Vec::with_capacity(SLOT_COUNT);
        for slot in [
            Slot::Block,
            Slot::Flex,
            Slot::Grid,
            Slot::Table,
            Slot::Inline,
            Slot::Intrinsic,
            Slot::StyleClone,
            Slot::Geometry,
            Slot::Exclusions,
        ] {
            entries.push((slot, state.samples[slot.index()]));
        }
        (entries, state.total_nodes, state.max_depth, state.max_width)
    });
    if total < 500 {
        return;
    }
    entries.sort_by_key(|a| std::cmp::Reverse(a.1.nanos));
    let mut lines = String::new();
    for (index, (slot, sample)) in entries.iter().enumerate().take(5) {
        lines.push_str(&format!(
            "\n[layout-prof] #{} {}: {:.3} ms ({} calls)",
            index + 1,
            slot.label(),
            sample.nanos as f64 / 1_000_000.0,
            sample.count
        ));
    }
    let total_ms: f64 = entries.iter().map(|(_, s)| s.nanos as f64).sum::<f64>() / 1_000_000.0;
    eprintln!(
        "[layout-prof] root layout: entries={} max_depth={} max_children={} total={:.3} ms{}",
        total, max_depth, max_width, total_ms, lines
    );
}

/// Guard that owns the root layout report lifecycle and unwinds on every exit path.
pub struct LayoutGuard {
    is_root: bool,
}

impl LayoutGuard {
    /// Enters a layout scope and returns a guard that reports on drop.
    pub fn new() -> Self {
        Self {
            is_root: enter_layout(),
        }
    }
}

impl Default for LayoutGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for LayoutGuard {
    fn drop(&mut self) {
        leave_layout(self.is_root);
    }
}

/// Guard that records elapsed time into a slot for its lexical scope.
pub struct Scope {
    enabled: bool,
}

impl Scope {
    /// Starts timing a slot when profiling is enabled.
    pub fn new(slot: Slot) -> Self {
        let enabled = enabled();
        if enabled {
            FRAMES.with(|frames| {
                frames.borrow_mut().push(Frame {
                    slot,
                    start: std::time::Instant::now(),
                    child_nanos: 0,
                });
            });
        }
        Self { enabled }
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        if !self.enabled {
            return;
        }
        let popped = FRAMES.with(|frames| frames.borrow_mut().pop());
        if let Some(frame) = popped {
            let elapsed = frame.start.elapsed().as_nanos();
            let exclusive = elapsed.saturating_sub(frame.child_nanos);
            record_self(frame.slot, exclusive);
            FRAMES.with(|frames| {
                if let Some(parent) = frames.borrow_mut().last_mut() {
                    parent.child_nanos += elapsed;
                }
            });
        }
    }
}
