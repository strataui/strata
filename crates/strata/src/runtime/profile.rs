//! Per-phase frame profiling: input, signals, view render, reconcile, style cascade,
//! layout, display-list compile, raster, 3D submit, present, and rolling p50/p95/p99 telemetry.

use serde::{Deserialize, Serialize};
use std::alloc::{GlobalAlloc, Layout};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

#[cfg(feature = "profiling")]
static FRAME_HEAP_ALLOC_COUNT: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "profiling")]
static FRAME_HEAP_ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
static TOTAL_HEAP_ALLOC_COUNT: AtomicU64 = AtomicU64::new(0);
static TOTAL_HEAP_ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);

/// Optional zero-overhead-when-unused counting allocator wrapper that host apps (or `--features profiling`
/// builds) can install as `#[global_allocator]` to measure real per-frame heap allocation count and bytes.
///
/// Example:
/// ```ignore
/// #[global_allocator]
/// static ALLOC: strata::ProfilingAllocator<std::alloc::System> =
///     strata::ProfilingAllocator::new(std::alloc::System);
/// ```
pub struct ProfilingAllocator<A> {
    inner: A,
}

impl<A> ProfilingAllocator<A> {
    pub const fn new(inner: A) -> Self {
        Self { inner }
    }
}

unsafe impl<A: GlobalAlloc> GlobalAlloc for ProfilingAllocator<A> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = self.inner.alloc(layout);
        if !ptr.is_null() {
            record_heap_alloc(layout.size() as u64);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.inner.dealloc(ptr, layout);
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = self.inner.alloc_zeroed(layout);
        if !ptr.is_null() {
            record_heap_alloc(layout.size() as u64);
        }
        ptr
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let out = self.inner.realloc(ptr, layout, new_size);
        if !out.is_null() && new_size > layout.size() {
            record_heap_alloc((new_size - layout.size()) as u64);
        }
        out
    }
}

/// Records a heap allocation event for per-frame telemetry.
#[inline]
pub fn record_heap_alloc(bytes: u64) {
    #[cfg(feature = "profiling")]
    {
        FRAME_HEAP_ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        FRAME_HEAP_ALLOC_BYTES.fetch_add(bytes, Ordering::Relaxed);
        TOTAL_HEAP_ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        TOTAL_HEAP_ALLOC_BYTES.fetch_add(bytes, Ordering::Relaxed);
    }
    #[cfg(not(feature = "profiling"))]
    {
        let _ = bytes;
    }
}

/// Drains and returns `(alloc_count, alloc_bytes)` recorded since the previous frame.
#[inline]
pub fn take_frame_heap_alloc_delta() -> (u64, u64) {
    #[cfg(feature = "profiling")]
    {
        let count = FRAME_HEAP_ALLOC_COUNT.swap(0, Ordering::Relaxed);
        let bytes = FRAME_HEAP_ALLOC_BYTES.swap(0, Ordering::Relaxed);
        (count, bytes)
    }
    #[cfg(not(feature = "profiling"))]
    {
        (0, 0)
    }
}

/// Returns cumulative `(total_alloc_count, total_alloc_bytes)` tracked when `profiling` is active.
#[inline]
pub fn total_heap_alloc_stats() -> (u64, u64) {
    (
        TOTAL_HEAP_ALLOC_COUNT.load(Ordering::Relaxed),
        TOTAL_HEAP_ALLOC_BYTES.load(Ordering::Relaxed),
    )
}

/// Wall-clock cost of each frame phase since the last [`AppRuntime::finish_frame_profile`].
///
/// Phases accumulate, so a frame that rebuilds twice reports both costs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameProfile {
    /// Blueprint to layout-tree rebuild, including style resolution.
    pub rebuild: Duration,
    /// Layout computation.
    pub layout: Duration,
    /// Display-list compilation (compositor, scrollbars, path batch).
    pub compile: Duration,
    /// Rasterization, recorded by the presenting backend (Cairo or GPU 2D).
    pub raster: Duration,
    /// Presentation to the surface, recorded by the presenting backend.
    pub present: Duration,
    /// Platform/MCP input event dispatch duration.
    pub input_dispatch: Duration,
    /// Reactive signal drain and effect flushing duration.
    pub signal_drain: Duration,
    /// Host RSX view closure evaluation duration.
    pub view_render: Duration,
    /// Incremental layout-tree reconciliation duration.
    pub reconcile: Duration,
    /// Style cascade / hover subtree restyle duration.
    pub style_cascade: Duration,
    /// wgpu 3D command encoding and queue submission duration on CPU.
    pub wgpu_3d_submit: Duration,
    /// Hardware GPU timestamp query duration for 3D scene passes (microseconds; 0 when unavailable).
    pub gpu_3d_us: u32,
    /// Hardware GPU timestamp query duration for 2D overlay composite pass (microseconds; 0 when unavailable).
    pub gpu_2d_overlay_us: u32,
    /// Whether hardware `TIMESTAMP_QUERY` measurements populated `gpu_*_us`.
    pub gpu_timestamps_valid: bool,
}

impl FrameProfile {
    /// Sum of primary CPU phases (preserving non-overlapping accounting).
    pub fn total(&self) -> Duration {
        let tree_cost = if !self.rebuild.is_zero() {
            self.rebuild
        } else {
            self.reconcile + self.style_cascade
        };
        self.input_dispatch
            + self.signal_drain
            + self.view_render
            + tree_cost
            + self.layout
            + self.compile
            + self.raster
            + self.wgpu_3d_submit
            + self.present
    }
}

/// Complete per-frame telemetry snapshot exposed to host apps (e.g. debug HUD) and MCP tools.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FrameProfileRecord {
    pub frame_index: u64,
    pub timestamp_us: u64,
    pub rebuild_reason: String,
    pub outcome: String,
    pub input_dispatch_us: u32,
    pub signal_drain_us: u32,
    pub view_render_us: u32,
    pub reconcile_us: u32,
    pub style_cascade_us: u32,
    pub layout_us: u32,
    pub display_list_build_us: u32,
    pub raster_us: u32,
    pub wgpu_3d_submit_us: u32,
    pub present_us: u32,
    pub total_cpu_us: u32,
    pub gpu_3d_us: u32,
    pub gpu_2d_overlay_us: u32,
    pub gpu_timestamps_valid: bool,
    pub dom_nodes: u32,
    pub layout_nodes: u32,
    pub display_cmds: u32,
    pub tiles_dirty: u32,
    pub tiles_total: u32,
    pub tile_cache_hit_pct: f32,
    pub reconciled_subtrees: u32,
    pub rebuilt_nodes: u32,
    pub full_rebuilds: u64,
    pub reconciled: bool,
    pub hover_fast_path_used: bool,
    pub hover_fast_path_bailout: Option<String>,
    pub reconcile_bailout: Option<String>,
    pub scene3d_cache_hits: u64,
    pub scene3d_cache_misses: u64,
    pub scene3d_cache_hit_pct: f32,
    pub glyph_atlas_hit_pct: f32,
    pub gpu_memory_bytes: u64,
    pub heap_alloc_count: u64,
    pub heap_alloc_bytes: u64,
    pub layout_shift_score: f32,
}

/// Rolling percentile distribution (in microseconds) over a ring buffer of frames.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct PerfPercentileStats {
    pub p50_us: u32,
    pub p95_us: u32,
    pub p99_us: u32,
    pub mean_us: f32,
    pub max_us: u32,
}

impl PerfPercentileStats {
    pub fn from_samples(mut values: Vec<u32>) -> Self {
        if values.is_empty() {
            return Self::default();
        }
        values.sort_unstable();
        let n = values.len();
        let sum: u64 = values.iter().map(|&v| v as u64).sum();
        let pct = |p: f32| -> u32 {
            let idx = (((n - 1) as f32) * p).round() as usize;
            values[idx.min(n - 1)]
        };
        Self {
            p50_us: pct(0.50),
            p95_us: pct(0.95),
            p99_us: pct(0.99),
            mean_us: sum as f32 / n as f32,
            max_us: *values.last().unwrap_or(&0),
        }
    }
}

/// Rolling performance summary across the active frame ring buffer (for host HUDs & CI gates).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PerfProfileSummary {
    pub window_frames: usize,
    pub total_frames: u64,
    pub last_frame: Option<FrameProfileRecord>,
    pub total_cpu: PerfPercentileStats,
    pub input_dispatch: PerfPercentileStats,
    pub view_render: PerfPercentileStats,
    pub reconcile: PerfPercentileStats,
    pub style_cascade: PerfPercentileStats,
    pub layout: PerfPercentileStats,
    pub display_list: PerfPercentileStats,
    pub raster: PerfPercentileStats,
    pub wgpu_3d_submit: PerfPercentileStats,
    pub gpu_3d: PerfPercentileStats,
    pub gpu_2d_overlay: PerfPercentileStats,
    pub present: PerfPercentileStats,
    pub avg_tile_cache_hit_pct: f32,
    pub avg_scene3d_cache_hit_pct: f32,
    pub reconcile_hit_pct: f32,
    pub full_rebuilds: u64,
    pub reconciled_updates: u64,
    pub gpu_memory_bytes: u64,
    pub heap_alloc_bytes_peak: u64,
}

/// Bounded ring buffer of [`FrameProfileRecord`] snapshots.
#[derive(Debug, Clone)]
pub struct FrameProfileHistory {
    records: VecDeque<FrameProfileRecord>,
    capacity: usize,
    heap_alloc_bytes_peak: u64,
}

impl Default for FrameProfileHistory {
    fn default() -> Self {
        Self::new(300)
    }
}

impl FrameProfileHistory {
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.max(1);
        Self {
            records: VecDeque::with_capacity(cap),
            capacity: cap,
            heap_alloc_bytes_peak: 0,
        }
    }

    pub fn push(&mut self, record: FrameProfileRecord) {
        self.heap_alloc_bytes_peak = self.heap_alloc_bytes_peak.max(record.heap_alloc_bytes);
        if self.records.len() >= self.capacity {
            self.records.pop_front();
        }
        self.records.push_back(record);
    }

    pub fn last(&self) -> Option<&FrameProfileRecord> {
        self.records.back()
    }

    pub fn records(&self) -> &VecDeque<FrameProfileRecord> {
        &self.records
    }

    pub fn summary(
        &self,
        total_frames: u64,
        full_rebuilds: u64,
        reconciled_updates: u64,
    ) -> PerfProfileSummary {
        let n = self.records.len();
        let last_frame = self.records.back().cloned();
        let tile_sum: f32 = self.records.iter().map(|r| r.tile_cache_hit_pct).sum();
        let s3d_sum: f32 = self.records.iter().map(|r| r.scene3d_cache_hit_pct).sum();
        let total_updates = full_rebuilds + reconciled_updates;
        let reconcile_hit_pct = if total_updates > 0 {
            (reconciled_updates as f32 / total_updates as f32) * 100.0
        } else {
            100.0
        };
        PerfProfileSummary {
            window_frames: n,
            total_frames,
            gpu_memory_bytes: last_frame.as_ref().map_or(0, |r| r.gpu_memory_bytes),
            last_frame,
            total_cpu: PerfPercentileStats::from_samples(
                self.records.iter().map(|r| r.total_cpu_us).collect(),
            ),
            input_dispatch: PerfPercentileStats::from_samples(
                self.records.iter().map(|r| r.input_dispatch_us).collect(),
            ),
            view_render: PerfPercentileStats::from_samples(
                self.records.iter().map(|r| r.view_render_us).collect(),
            ),
            reconcile: PerfPercentileStats::from_samples(
                self.records.iter().map(|r| r.reconcile_us).collect(),
            ),
            style_cascade: PerfPercentileStats::from_samples(
                self.records.iter().map(|r| r.style_cascade_us).collect(),
            ),
            layout: PerfPercentileStats::from_samples(
                self.records.iter().map(|r| r.layout_us).collect(),
            ),
            display_list: PerfPercentileStats::from_samples(
                self.records
                    .iter()
                    .map(|r| r.display_list_build_us)
                    .collect(),
            ),
            raster: PerfPercentileStats::from_samples(
                self.records.iter().map(|r| r.raster_us).collect(),
            ),
            wgpu_3d_submit: PerfPercentileStats::from_samples(
                self.records.iter().map(|r| r.wgpu_3d_submit_us).collect(),
            ),
            gpu_3d: PerfPercentileStats::from_samples(
                self.records.iter().map(|r| r.gpu_3d_us).collect(),
            ),
            gpu_2d_overlay: PerfPercentileStats::from_samples(
                self.records.iter().map(|r| r.gpu_2d_overlay_us).collect(),
            ),
            present: PerfPercentileStats::from_samples(
                self.records.iter().map(|r| r.present_us).collect(),
            ),
            avg_tile_cache_hit_pct: if n > 0 { tile_sum / n as f32 } else { 100.0 },
            avg_scene3d_cache_hit_pct: if n > 0 { s3d_sum / n as f32 } else { 100.0 },
            reconcile_hit_pct,
            full_rebuilds,
            reconciled_updates,
            heap_alloc_bytes_peak: self.heap_alloc_bytes_peak,
        }
    }
}

/// Receives a completed [`FrameProfile`] each time a frame is presented.
pub struct FrameProfileHook(pub Box<dyn FnMut(&FrameProfile)>);

impl std::fmt::Debug for FrameProfileHook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FrameProfileHook(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_computes_rolling_percentiles_for_all_stage_fields() {
        let mut hist = FrameProfileHistory::new(16);
        for &(inp, view, style, g2d) in &[
            (100u32, 200u32, 50u32, 80u32),
            (200, 400, 150, 160),
            (400, 800, 350, 320),
        ] {
            hist.push(FrameProfileRecord {
                input_dispatch_us: inp,
                view_render_us: view,
                style_cascade_us: style,
                gpu_2d_overlay_us: g2d,
                ..Default::default()
            });
        }
        let s = hist.summary(3, 0, 3);
        assert_eq!(s.input_dispatch.p50_us, 200);
        assert_eq!(s.input_dispatch.p95_us, 400);
        assert_eq!(s.input_dispatch.max_us, 400);
        assert_eq!(s.view_render.p50_us, 400);
        assert_eq!(s.view_render.max_us, 800);
        assert_eq!(s.style_cascade.p50_us, 150);
        assert_eq!(s.style_cascade.max_us, 350);
        assert_eq!(s.gpu_2d_overlay.p50_us, 160);
        assert_eq!(s.gpu_2d_overlay.max_us, 320);
    }
}
