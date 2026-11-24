//! In-Process Overlay HUD, Frame Profiler, Reactive DAG Inspector, Time-Travel Debugger, and Remote CDP Bridge.

#![allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::field_reassign_with_default
)]

pub mod cdp;
pub mod hud;
pub mod inspector;
pub mod memory;
pub mod profiler;
pub mod reactive_graph;
pub mod server;
pub mod time_travel;

pub use cdp::{
    build_cdp_tree_from_blueprint, build_cdp_tree_from_ir, format_computed_style, CdpDispatcher,
    CdpDomNode, CdpRequest, CdpResponse, DevToolsState, WebSocketCodec, WebSocketFrame,
    WebSocketOpcode,
};
pub use server::CdpServer;

pub use hud::{
    DevToolsEventInterceptor, HudController, HudDockMode, HudLiveTelemetry, HudState, HudTab,
};
pub use inspector::{
    BoxModelMetrics, ComponentMeta, LiveTailwindEditor, PropDescriptor, PropValue, ReflectProp,
    SourceLocation,
};
pub use memory::{
    ArenaAnomaly, ArenaInspector, ArenaTelemetrySample, AtomTableInspector, HeapAllocationTracker,
    HeapBudgetStatus,
};
pub use profiler::{
    current_thread_id_u64, FrameTimelineProfiler, FrameTimingSample, LayoutShiftDetector,
    StageTimer, TraceJsonExporter, WorkerTraceSpan,
};
pub use reactive_graph::{
    CausalTriggerTrace, ReRenderHeatmapLevel, ReactiveCycleError, ReactiveCycleGuard, ReactiveEdge,
    ReactiveGraphSnapshot, ReactiveNodeInfo, ReactiveNodeKind,
};
pub use time_travel::{SnapshotDelta, SnapshotId, StateSnapshot, TimeTravelDebugger};

/// Unified Developer Diagnostics Suite coordinator.
#[derive(Debug, Default)]
pub struct DevTools {
    pub hud: HudController,
    pub profiler: FrameTimelineProfiler,
    pub heap_tracker: HeapAllocationTracker,
    pub arena_inspector: ArenaInspector,
    pub time_travel: TimeTravelDebugger,
    pub cdp_dispatcher: CdpDispatcher,
    pub live_telemetry: HudLiveTelemetry,
}

impl DevTools {
    pub fn new() -> Self {
        Self {
            hud: HudController::new(),
            profiler: FrameTimelineProfiler::new(240, 60.0),
            heap_tracker: HeapAllocationTracker::default(),
            arena_inspector: ArenaInspector::new(85.0),
            time_travel: TimeTravelDebugger::new(128),
            cdp_dispatcher: CdpDispatcher::new(),
            live_telemetry: HudLiveTelemetry::default(),
        }
    }

    /// Renders the DevTools HUD overlay backed by live profiler, reactivity, memory, arena, and time-travel state.
    pub fn render_hud_overlay(&self, display_list: &mut strata_render::compositor::DisplayList) {
        self.hud.render_overlay_with_context(
            Some(&self.profiler),
            Some(&self.live_telemetry),
            Some(&self.arena_inspector),
            Some(&self.time_travel),
            display_list,
        );
    }
}
