//! 9-Stage microsecond frame profiler, CLS spatial jank detector, and trace.json exporter.

use std::time::Instant;

/// Returns a stable non-zero numeric identifier for the calling OS thread.
pub fn current_thread_id_u64() -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::thread::current().id().hash(&mut hasher);
    (hasher.finish() & 0x7fff_ffff_ffff_ffff).max(1)
}

/// Optional custom span recorded from background worker threads (e.g. Earth tile/terrain worker,
/// async PNG screenshot encoder, or GPU submit worker) for multi-track Perfetto export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerTraceSpan {
    pub name: String,
    pub category: String,
    pub thread_name: String,
    pub thread_id: u64,
    pub start_us: u64,
    pub duration_us: u32,
}

/// Detailed microsecond timing metrics across all discrete frame pipeline stages,
/// anchored to a real monotonic `Instant` epoch with OS process/thread IDs and GPU hardware passes.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FrameTimingSample {
    pub frame_index: u64,
    /// Real monotonic microseconds elapsed since profiler creation when this frame began.
    pub timestamp_us: u64,
    /// Real OS process ID.
    pub pid: u32,
    /// Real numeric thread ID for the UI main thread.
    pub ui_thread_id: u64,
    /// Optional worker thread ID associated with background work on this frame.
    pub worker_thread_id: u64,
    /// Dedicated Perfetto track ID for GPU hardware passes / submission.
    pub gpu_thread_id: u64,
    pub vsync_wait_us: u32,
    pub input_dispatch_us: u32,
    pub signal_drain_us: u32,
    pub view_render_us: u32,
    pub parallel_reconcile_us: u32,
    pub style_cascade_us: u32,
    pub layout_solve_us: u32,
    pub display_list_build_us: u32,
    pub raster_2d_us: u32,
    pub gpu_submit_us: u32,
    pub present_us: u32,
    pub gpu_raster_duration_us: u32,
    /// Hardware `wgpu::Features::TIMESTAMP_QUERY` shadow pass duration in microseconds.
    pub gpu_shadow_pass_us: u32,
    /// Hardware `wgpu::Features::TIMESTAMP_QUERY` 3D scene pass duration in microseconds.
    pub gpu_3d_pass_us: u32,
    /// Hardware `wgpu::Features::TIMESTAMP_QUERY` 2D overlay composite pass duration in microseconds.
    pub gpu_2d_overlay_pass_us: u32,
    /// True when GPU pass durations were measured via hardware `TIMESTAMP_QUERY`.
    pub gpu_timestamps_valid: bool,
    pub total_cpu_frame_us: u32,
    pub frame_budget_us: u32,
    pub layout_shift_score: f32,
}

impl FrameTimingSample {
    /// Returns true if this frame exceeded its budget (dropping a frame / causing jank).
    pub fn is_jank(&self) -> bool {
        self.frame_budget_us > 0 && self.total_cpu_frame_us > self.frame_budget_us
    }

    /// Calculates and populates total_cpu_frame_us from the individual CPU pipeline stages.
    pub fn calculate_total_cpu(&mut self) {
        self.total_cpu_frame_us = self
            .input_dispatch_us
            .saturating_add(self.signal_drain_us)
            .saturating_add(self.view_render_us)
            .saturating_add(self.parallel_reconcile_us)
            .saturating_add(self.style_cascade_us)
            .saturating_add(self.layout_solve_us)
            .saturating_add(self.display_list_build_us)
            .saturating_add(self.raster_2d_us)
            .saturating_add(self.gpu_submit_us)
            .saturating_add(self.present_us);
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
    worker_spans: Vec<WorkerTraceSpan>,
    capacity: usize,
    target_hz: f32,
    epoch: Instant,
    last_timestamp_us: u64,
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
            worker_spans: Vec::new(),
            capacity: capacity.max(1),
            target_hz: target_hz.max(1.0),
            epoch: Instant::now(),
            last_timestamp_us: 0,
        }
    }

    /// Monotonic microseconds elapsed since this profiler was constructed.
    pub fn now_us(&self) -> u64 {
        self.epoch.elapsed().as_micros() as u64
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
        if sample.pid == 0 {
            sample.pid = std::process::id();
        }
        if sample.ui_thread_id == 0 {
            sample.ui_thread_id = current_thread_id_u64();
        }
        if sample.gpu_thread_id == 0 {
            sample.gpu_thread_id = sample.ui_thread_id.wrapping_add(1).max(2);
        }
        if sample.timestamp_us == 0 {
            let now = self.now_us();
            sample.timestamp_us = if now <= self.last_timestamp_us && !self.samples.is_empty() {
                self.last_timestamp_us.saturating_add(1)
            } else {
                now
            };
        }
        self.last_timestamp_us = self.last_timestamp_us.max(sample.timestamp_us);

        if self.samples.len() >= self.capacity {
            self.samples.remove(0);
        }
        self.samples.push(sample);
    }

    /// Records a real worker-thread trace span (e.g. Earth tile/terrain worker or background encoder).
    pub fn record_worker_span(
        &mut self,
        name: impl Into<String>,
        category: impl Into<String>,
        thread_name: impl Into<String>,
        thread_id: u64,
        start_us: u64,
        duration_us: u32,
    ) {
        let max_spans = self.capacity.saturating_mul(8);
        if self.worker_spans.len() >= max_spans {
            self.worker_spans.remove(0);
        }
        self.worker_spans.push(WorkerTraceSpan {
            name: name.into(),
            category: category.into(),
            thread_name: thread_name.into(),
            thread_id: thread_id.max(1),
            start_us,
            duration_us,
        });
    }

    pub fn worker_spans(&self) -> &[WorkerTraceSpan] {
        &self.worker_spans
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
        let sum: u64 = self
            .samples
            .iter()
            .map(|s| s.total_cpu_frame_us as u64)
            .sum();
        sum as f32 / self.samples.len() as f32
    }

    pub fn jank_frame_count(&self) -> usize {
        self.samples.iter().filter(|s| s.is_jank()).count()
    }

    pub fn percentile_cpu_time_us(&self, pct: f32) -> u32 {
        if self.samples.is_empty() {
            return 0;
        }
        let mut times: Vec<u32> = self.samples.iter().map(|s| s.total_cpu_frame_us).collect();
        times.sort_unstable();
        let idx = (((times.len() - 1) as f32) * pct.clamp(0.0, 1.0)).round() as usize;
        times[idx.min(times.len() - 1)]
    }

    pub fn p50_cpu_time_us(&self) -> u32 {
        self.percentile_cpu_time_us(0.50)
    }

    pub fn p95_cpu_time_us(&self) -> u32 {
        self.percentile_cpu_time_us(0.95)
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

    /// Exports both frame timing samples and worker-thread spans to Chrome Trace / Perfetto JSON.
    pub fn export_trace_json(&self) -> String {
        TraceJsonExporter::export_trace_json_with_spans(&self.samples, &self.worker_spans)
    }
}

fn escape_json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

/// Chromium trace.json event exporter for Perfetto, chrome://tracing, and Speedscope.
pub struct TraceJsonExporter;

impl TraceJsonExporter {
    pub fn export_trace_json(samples: &[FrameTimingSample]) -> String {
        Self::export_trace_json_with_spans(samples, &[])
    }

    /// Exports real Instant-anchored frame samples and multi-thread worker/GPU spans
    /// in Chromium Trace Event / Perfetto JSON format.
    pub fn export_trace_json_with_spans(
        samples: &[FrameTimingSample],
        worker_spans: &[WorkerTraceSpan],
    ) -> String {
        let mut events: Vec<String> = Vec::with_capacity(samples.len() * 12 + worker_spans.len() + 6);
        let default_pid = samples
            .first()
            .map(|s| if s.pid != 0 { s.pid } else { std::process::id() })
            .unwrap_or_else(std::process::id);

        // Emit thread_name metadata events for distinct UI / GPU / Worker tracks.
        let mut named_threads = std::collections::BTreeSet::<(u32, u64)>::new();
        for s in samples {
            let pid = if s.pid != 0 { s.pid } else { default_pid };
            let ui_tid = if s.ui_thread_id != 0 {
                s.ui_thread_id
            } else {
                current_thread_id_u64()
            };
            let gpu_tid = if s.gpu_thread_id != 0 {
                s.gpu_thread_id
            } else {
                ui_tid.wrapping_add(1).max(2)
            };
            if named_threads.insert((pid, ui_tid)) {
                events.push(format!(
                    r#"  {{"name":"thread_name","ph":"M","pid":{},"tid":{},"args":{{"name":"UI Thread"}}}}"#,
                    pid, ui_tid
                ));
            }
            if named_threads.insert((pid, gpu_tid)) {
                events.push(format!(
                    r#"  {{"name":"thread_name","ph":"M","pid":{},"tid":{},"args":{{"name":"GPU Submit & Passes"}}}}"#,
                    pid, gpu_tid
                ));
            }
            if s.worker_thread_id != 0 && named_threads.insert((pid, s.worker_thread_id)) {
                events.push(format!(
                    r#"  {{"name":"thread_name","ph":"M","pid":{},"tid":{},"args":{{"name":"Earth / Terrain Worker"}}}}"#,
                    pid, s.worker_thread_id
                ));
            }
        }
        for sp in worker_spans {
            if named_threads.insert((default_pid, sp.thread_id)) {
                events.push(format!(
                    r#"  {{"name":"thread_name","ph":"M","pid":{},"tid":{},"args":{{"name":"{}"}}}}"#,
                    default_pid,
                    sp.thread_id,
                    escape_json_str(&sp.thread_name)
                ));
            }
        }

        let mut cursor_fallback_ts: u64 = 0;
        for s in samples {
            let pid = if s.pid != 0 { s.pid } else { default_pid };
            let ui_tid = if s.ui_thread_id != 0 {
                s.ui_thread_id
            } else {
                current_thread_id_u64()
            };
            let gpu_tid = if s.gpu_thread_id != 0 {
                s.gpu_thread_id
            } else {
                ui_tid.wrapping_add(1).max(2)
            };

            let start_ts = if s.timestamp_us > 0 {
                cursor_fallback_ts = s.timestamp_us.saturating_add(s.total_cpu_frame_us as u64);
                s.timestamp_us
            } else {
                let ts = cursor_fallback_ts;
                cursor_fallback_ts = ts.saturating_add(s.total_cpu_frame_us.max(1) as u64);
                ts
            };

            events.push(format!(
                r#"  {{"name":"FrameTick","cat":"frame","ph":"X","ts":{},"dur":{},"pid":{},"tid":{},"args":{{"frame_index":{},"cls":{:.4}}}}}"#,
                start_ts, s.total_cpu_frame_us, pid, ui_tid, s.frame_index, s.layout_shift_score
            ));

            let mut phase_ts = start_ts;
            let cpu_phases: [(&str, u32); 10] = [
                ("InputDispatch", s.input_dispatch_us),
                ("SignalDrain", s.signal_drain_us),
                ("ViewRender", s.view_render_us),
                ("ParallelReconcile", s.parallel_reconcile_us),
                ("StyleCascade", s.style_cascade_us),
                ("LayoutSolve", s.layout_solve_us),
                ("DisplayListBuild", s.display_list_build_us),
                ("Raster2D", s.raster_2d_us),
                ("GpuSubmit", s.gpu_submit_us),
                ("Present", s.present_us),
            ];
            for (name, dur) in cpu_phases {
                if dur > 0 {
                    events.push(format!(
                        r#"  {{"name":"{}","cat":"cpu","ph":"X","ts":{},"dur":{},"pid":{},"tid":{}}}"#,
                        name, phase_ts, dur, pid, ui_tid
                    ));
                    phase_ts = phase_ts.saturating_add(dur as u64);
                }
            }

            // Emit real hardware GPU pass tracks on the GPU thread track.
            let mut gpu_ts = start_ts.saturating_add(
                (s.input_dispatch_us
                    + s.signal_drain_us
                    + s.view_render_us
                    + s.parallel_reconcile_us
                    + s.style_cascade_us
                    + s.layout_solve_us
                    + s.display_list_build_us) as u64,
            );
            if s.gpu_timestamps_valid {
                for (name, dur) in [
                    ("GpuShadowPass", s.gpu_shadow_pass_us),
                    ("Gpu3DPass", s.gpu_3d_pass_us),
                    ("Gpu2DOverlayPass", s.gpu_2d_overlay_pass_us),
                ] {
                    if dur > 0 {
                        events.push(format!(
                            r#"  {{"name":"{}","cat":"gpu","ph":"X","ts":{},"dur":{},"pid":{},"tid":{},"args":{{"hw_timestamp":true}}}}"#,
                            name, gpu_ts, dur, pid, gpu_tid
                        ));
                        gpu_ts = gpu_ts.saturating_add(dur as u64);
                    }
                }
            } else if s.gpu_raster_duration_us > 0 {
                events.push(format!(
                    r#"  {{"name":"GpuRaster","cat":"gpu","ph":"X","ts":{},"dur":{},"pid":{},"tid":{},"args":{{"hw_timestamp":false}}}}"#,
                    gpu_ts, s.gpu_raster_duration_us, pid, gpu_tid
                ));
            }
        }

        for sp in worker_spans {
            events.push(format!(
                r#"  {{"name":"{}","cat":"{}","ph":"X","ts":{},"dur":{},"pid":{},"tid":{}}}"#,
                escape_json_str(&sp.name),
                escape_json_str(&sp.category),
                sp.start_us,
                sp.duration_us,
                default_pid,
                sp.thread_id
            ));
        }

        let mut json = String::with_capacity(events.len() * 120 + 32);
        json.push_str("{\"traceEvents\":[\n");
        json.push_str(&events.join(",\n"));
        json.push_str("\n]}\n");
        json
    }
}
