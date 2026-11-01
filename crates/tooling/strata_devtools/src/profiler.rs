//! 9-Stage microsecond frame profiler, CLS spatial jank detector, and trace.json exporter.

use std::time::Instant;

/// Detailed microsecond timing metrics across all 9 discrete frame pipeline stages.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FrameTimingSample {
    pub frame_index: u64,
    pub vsync_wait_us: u32,
    pub input_dispatch_us: u32,
    pub signal_drain_us: u32,
    pub parallel_reconcile_us: u32,
    pub style_cascade_us: u32,
    pub layout_solve_us: u32,
    pub display_list_build_us: u32,
    pub gpu_submit_us: u32,
    pub gpu_raster_duration_us: u32,
    pub total_cpu_frame_us: u32,
    pub frame_budget_us: u32,
    pub layout_shift_score: f32,
}

impl FrameTimingSample {
    /// Returns true if this frame exceeded its budget (dropping a frame / causing jank).
    pub fn is_jank(&self) -> bool {
        self.total_cpu_frame_us > self.frame_budget_us
    }

    /// Calculates and populates total_cpu_frame_us from the individual stages.
    pub fn calculate_total_cpu(&mut self) {
        self.total_cpu_frame_us = self.input_dispatch_us
            + self.signal_drain_us
            + self.parallel_reconcile_us
            + self.style_cascade_us
            + self.layout_solve_us
            + self.display_list_build_us
            + self.gpu_submit_us;
    }
}

/// Nanosecond-precision stage stopwatch.
pub struct StageTimer {
    start: Instant,
}

impl Default for StageTimer {
    fn default() -> Self {
        Self::start()
    }
}

impl StageTimer {
    pub fn start() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    pub fn stop_us(self) -> u32 {
        self.start.elapsed().as_micros() as u32
    }
}

/// Cumulative Layout Shift (CLS) spatial jank detector.
pub struct LayoutShiftDetector;

impl LayoutShiftDetector {
    /// Computes the layout shift score between two consecutive frame positions.
    pub fn calculate_shift_score(
        prev_x: f32,
        prev_y: f32,
        prev_w: f32,
        prev_h: f32,
        cur_x: f32,
        cur_y: f32,
        cur_w: f32,
        cur_h: f32,
        viewport_w: f32,
        viewport_h: f32,
    ) -> f32 {
        let delta_x = (cur_x - prev_x).abs();
        let delta_y = (cur_y - prev_y).abs();

        if delta_x == 0.0 && delta_y == 0.0 {
            return 0.0;
        }

        let max_dimension = viewport_w.max(viewport_h).max(1.0);
        let distance_fraction = (delta_x.max(delta_y) / max_dimension).clamp(0.0, 1.0);

        let prev_area = prev_w * prev_h;
        let cur_area = cur_w * cur_h;
        let viewport_area = (viewport_w * viewport_h).max(1.0);
        let union_area = prev_area.max(cur_area);
        let impact_fraction = (union_area / viewport_area).clamp(0.0, 1.0);

        impact_fraction * distance_fraction
    }
}

/// Rolling ring buffer collecting frame timing samples and computing performance percentiles.
#[derive(Debug, Clone)]
pub struct FrameTimelineProfiler {
    samples: Vec<FrameTimingSample>,
    capacity: usize,
    target_hz: f32,
}

impl Default for FrameTimelineProfiler {
    fn default() -> Self {
        Self::new(300, 120.0) // 300 frames @ 120 FPS
    }
}

impl FrameTimelineProfiler {
    pub fn new(capacity: usize, target_hz: f32) -> Self {
        Self {
            samples: Vec::with_capacity(capacity),
            capacity,
            target_hz,
        }
    }

    pub fn frame_budget_us(&self) -> u32 {
        (1_000_000.0 / self.target_hz) as u32
    }

    pub fn record_sample(&mut self, mut sample: FrameTimingSample) {
        if sample.frame_budget_us == 0 {
            sample.frame_budget_us = self.frame_budget_us();
        }
        if sample.total_cpu_frame_us == 0 {
            sample.calculate_total_cpu();
        }

        if self.samples.len() >= self.capacity {
            self.samples.remove(0);
        }
        self.samples.push(sample);
    }

    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    pub fn samples(&self) -> &[FrameTimingSample] {
        &self.samples
    }

    pub fn average_cpu_time_us(&self) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let sum: u64 = self.samples.iter().map(|s| s.total_cpu_frame_us as u64).sum();
        sum as f32 / self.samples.len() as f32
    }

    pub fn jank_frame_count(&self) -> usize {
        self.samples.iter().filter(|s| s.is_jank()).count()
    }

    pub fn p99_cpu_time_us(&self) -> u32 {
        if self.samples.is_empty() {
            return 0;
        }
        let mut times: Vec<u32> = self.samples.iter().map(|s| s.total_cpu_frame_us).collect();
        times.sort_unstable();
        let idx = ((times.len() as f32 * 0.99) as usize).min(times.len() - 1);
        times[idx]
    }
}

/// Chromium trace.json event exporter for Perfetto, chrome://tracing, and Speedscope.
pub struct TraceJsonExporter;

impl TraceJsonExporter {
    pub fn export_trace_json(samples: &[FrameTimingSample]) -> String {
        let mut json = String::new();
        json.push_str("{\"traceEvents\":[\n");

        for (i, s) in samples.iter().enumerate() {
            let start_ts = s.frame_index * 8333; // Mock microsecond anchor

            json.push_str(&format!(
                r#"  {{"name":"FrameTick","ph":"X","ts":{},"dur":{},"pid":1,"tid":1}},
  {{"name":"SignalDrain","ph":"X","ts":{},"dur":{},"pid":1,"tid":1}},
  {{"name":"ParallelReconcile","ph":"X","ts":{},"dur":{},"pid":1,"tid":1}},
  {{"name":"LayoutSolve","ph":"X","ts":{},"dur":{},"pid":1,"tid":1}},
  {{"name":"DisplayListBuild","ph":"X","ts":{},"dur":{},"pid":1,"tid":1}},
  {{"name":"GpuSubmit","ph":"X","ts":{},"dur":{},"pid":1,"tid":1}}"#,
                start_ts,
                s.total_cpu_frame_us,
                start_ts + s.input_dispatch_us as u64,
                s.signal_drain_us,
                start_ts + (s.input_dispatch_us + s.signal_drain_us) as u64,
                s.parallel_reconcile_us,
                start_ts + (s.input_dispatch_us + s.signal_drain_us + s.parallel_reconcile_us + s.style_cascade_us) as u64,
                s.layout_solve_us,
                start_ts + (s.input_dispatch_us + s.signal_drain_us + s.parallel_reconcile_us + s.style_cascade_us + s.layout_solve_us) as u64,
                s.display_list_build_us,
                start_ts + (s.input_dispatch_us + s.signal_drain_us + s.parallel_reconcile_us + s.style_cascade_us + s.layout_solve_us + s.display_list_build_us) as u64,
                s.gpu_submit_us
            ));

            if i + 1 < samples.len() {
                json.push(',');
            }
            json.push('\n');
        }

        json.push_str("]}\n");
        json
    }
}
