//! Integration tests for strata_devtools.

use strata_devtools::*;
use strata_platform::{KeyCode, Modifiers, MouseButton, PlatformEvent};
use strata_primitives::geom::EdgeInsets;

#[test]
fn test_hud_toggle_and_event_interception() {
    let mut devtools = DevTools::new();
    devtools.hud.update_bounds(1280.0, 720.0);

    // Initial state: hidden
    assert!(!devtools.hud.state.is_visible);

    // F12 key press toggles visibility
    let f12_event = PlatformEvent::KeyDown {
        key: KeyCode::F12,
        modifiers: Modifiers::default(),
    };
    let consumed = DevToolsEventInterceptor::intercept(&mut devtools.hud, &f12_event);
    assert!(consumed);
    assert!(devtools.hud.state.is_visible);

    // Mouse move outside HUD bounds should not be consumed
    let outside_mouse = PlatformEvent::MouseMove { x: 100.0, y: 100.0 };
    assert!(!DevToolsEventInterceptor::intercept(&mut devtools.hud, &outside_mouse));

    // Mouse move inside HUD bounds (bottom dock at y >= 468.0) should be consumed
    let inside_mouse = PlatformEvent::MouseMove { x: 100.0, y: 600.0 };
    assert!(DevToolsEventInterceptor::intercept(&mut devtools.hud, &inside_mouse));

    // Inspect element toggle
    assert!(!devtools.hud.state.is_inspecting_element);
    let ctrl_shift_c = PlatformEvent::KeyDown {
        key: KeyCode::KeyC,
        modifiers: Modifiers {
            ctrl: true,
            shift: true,
            alt: false,
            meta: false,
        },
    };
    assert!(DevToolsEventInterceptor::intercept(&mut devtools.hud, &ctrl_shift_c));
    assert!(devtools.hud.state.is_inspecting_element);

    // Click in inspect mode selects and exits inspect mode
    let click = PlatformEvent::MouseDown {
        x: 200.0,
        y: 200.0,
        button: MouseButton::Left,
    };
    assert!(DevToolsEventInterceptor::intercept(&mut devtools.hud, &click));
    assert!(!devtools.hud.state.is_inspecting_element);
}

#[test]
fn test_box_model_metrics_and_tailwind_editor() {
    let metrics = BoxModelMetrics::new(
        100.0,
        150.0,
        200.0,
        100.0,
        EdgeInsets::all(10.0),
        EdgeInsets::all(2.0),
        EdgeInsets::all(20.0),
    );

    let content = metrics.content_box().to_rect_f32();
    assert_eq!(content.size.width, 200.0);
    assert_eq!(content.size.height, 100.0);

    let padding = metrics.padding_box().to_rect_f32();
    assert_eq!(padding.origin.x, 90.0);
    assert_eq!(padding.size.width, 220.0);

    let border = metrics.border_box().to_rect_f32();
    assert_eq!(border.origin.x, 88.0);
    assert_eq!(border.size.width, 224.0);

    let margin = metrics.margin_box().to_rect_f32();
    assert_eq!(margin.origin.x, 68.0);
    assert_eq!(margin.size.width, 264.0);

    // Live Tailwind Editor mutation
    let mut classes = vec!["p-4".to_string(), "bg-blue-500".to_string()];
    let mutated = LiveTailwindEditor::apply_class_mutation(&mut classes, "-bg-blue-500 bg-red-500 rounded-lg");
    assert!(mutated);
    assert!(classes.contains(&"p-4".to_string()));
    assert!(classes.contains(&"bg-red-500".to_string()));
    assert!(classes.contains(&"rounded-lg".to_string()));
    assert!(!classes.contains(&"bg-blue-500".to_string()));
}

#[test]
fn test_reactive_graph_and_cycle_guard() {
    let mut guard = ReactiveCycleGuard::new(5);

    // Push non-cyclical nodes
    assert!(guard.enter_node(ReactiveNodeKind::Signal(1)).is_ok());
    assert!(guard.enter_node(ReactiveNodeKind::Memo(1)).is_ok());
    assert!(guard.enter_node(ReactiveNodeKind::Effect(1)).is_ok());

    // Cycle detected when visiting Signal(1) again
    let err = guard.enter_node(ReactiveNodeKind::Signal(1)).unwrap_err();
    assert_eq!(err.culprit, ReactiveNodeKind::Signal(1));
    assert!(err.detected_cycle.len() >= 3);

    // Heatmap levels
    assert_eq!(ReRenderHeatmapLevel::from_frequency(0), ReRenderHeatmapLevel::Optimal);
    assert_eq!(ReRenderHeatmapLevel::from_frequency(5), ReRenderHeatmapLevel::Expected);
    assert_eq!(ReRenderHeatmapLevel::from_frequency(30), ReRenderHeatmapLevel::High);
    assert_eq!(ReRenderHeatmapLevel::from_frequency(80), ReRenderHeatmapLevel::Thrashing);
}

#[test]
fn test_frame_profiler_cls_and_trace_export() {
    let mut profiler = FrameTimelineProfiler::new(60, 60.0);

    let mut sample = FrameTimingSample {
        frame_index: 1,
        vsync_wait_us: 100,
        input_dispatch_us: 200,
        signal_drain_us: 300,
        parallel_reconcile_us: 400,
        style_cascade_us: 500,
        layout_solve_us: 600,
        display_list_build_us: 700,
        gpu_submit_us: 800,
        gpu_raster_duration_us: 1000,
        total_cpu_frame_us: 0,
        frame_budget_us: 16666,
        layout_shift_score: 0.05,
    };
    sample.calculate_total_cpu();
    assert_eq!(sample.total_cpu_frame_us, 3500);
    assert!(!sample.is_jank());

    profiler.record_sample(sample);
    assert_eq!(profiler.sample_count(), 1);
    assert_eq!(profiler.jank_frame_count(), 0);

    // CLS calculation test
    let cls = LayoutShiftDetector::calculate_shift_score(
        0.0, 0.0, 100.0, 100.0,
        0.0, 50.0, 100.0, 100.0,
        1000.0, 1000.0,
    );
    assert!(cls > 0.0);

    // Trace JSON exporter
    let trace_json = TraceJsonExporter::export_trace_json(profiler.samples());
    assert!(trace_json.contains("\"traceEvents\":["));
    assert!(trace_json.contains("\"name\":\"LayoutSolve\""));
}

#[test]
fn test_memory_arena_and_heap_tracker() {
    let mut inspector = ArenaInspector::new(85.0);

    // Normal sample: 50% utilization
    let sample1 = ArenaTelemetrySample::new(1, 512, 1024, 512, 1);
    assert_eq!(inspector.record_sample(sample1), None);

    // Near capacity sample: 90% utilization
    let sample2 = ArenaTelemetrySample::new(1, 920, 1024, 920, 2);
    let anomaly = inspector.record_sample(sample2);
    assert!(matches!(anomaly, Some(ArenaAnomaly::NearCapacity { .. })));

    // Atom table hit rate
    let atom_inspector = AtomTableInspector::new(500, 32000, 950, 50);
    assert_eq!(atom_inspector.hit_rate(), 95.0);

    // Heap allocation tracker
    let mut heap_tracker = HeapAllocationTracker::new(10 * 1024 * 1024, 20 * 1024 * 1024);
    assert_eq!(heap_tracker.record_allocation(5 * 1024 * 1024), HeapBudgetStatus::Normal);
    assert_eq!(heap_tracker.record_allocation(15 * 1024 * 1024), HeapBudgetStatus::Warning(15 * 1024 * 1024));
    assert_eq!(heap_tracker.record_allocation(25 * 1024 * 1024), HeapBudgetStatus::Critical(25 * 1024 * 1024));

    // Monotonic growth leak detection
    assert!(heap_tracker.is_monotonically_growing(3));
}

#[test]
fn test_time_travel_debugger_and_delta() {
    let mut tt = TimeTravelDebugger::new(10);

    let id1 = tt.record_snapshot(1000, "Initial", vec![(1, "Alice".to_string()), (2, "10".to_string())], 0xAAA);
    let id2 = tt.record_snapshot(2000, "Name change", vec![(1, "Bob".to_string()), (2, "10".to_string())], 0xBBB);
    let id3 = tt.record_snapshot(3000, "Age change", vec![(1, "Bob".to_string()), (2, "20".to_string())], 0xCCC);

    assert_eq!(tt.len(), 3);
    assert_eq!(tt.current_snapshot().unwrap().id, id3);

    // Delta between id1 and id2
    let deltas = tt.compute_delta(id1, id2).unwrap();
    assert_eq!(deltas.len(), 1);
    assert_eq!(deltas[0].signal_id, 1);
    assert_eq!(deltas[0].old_val.as_deref(), Some("Alice"));
    assert_eq!(deltas[0].new_val.as_deref(), Some("Bob"));

    // Step back
    let prev = tt.step_back().unwrap();
    assert_eq!(prev.id, id2);

    let prev2 = tt.step_back().unwrap();
    assert_eq!(prev2.id, id1);
    assert!(!tt.can_step_back());

    // Step forward
    let next = tt.step_forward().unwrap();
    assert_eq!(next.id, id2);

    // Scrub to id3
    let scrubbed = tt.scrub_to(id3).unwrap();
    assert_eq!(scrubbed.id, id3);
}

#[test]
fn test_websocket_framing_and_cdp_dispatcher() {
    let payload = b"{\"id\":10,\"method\":\"DOM.getDocument\"}";
    let frame = WebSocketCodec::encode_frame(WebSocketOpcode::Text, payload);

    let (decoded, consumed) = WebSocketCodec::decode_frame(&frame).unwrap();
    assert_eq!(consumed, frame.len());
    assert!(decoded.fin);
    assert_eq!(decoded.opcode, WebSocketOpcode::Text);
    assert_eq!(decoded.payload, payload);

    let dispatcher = CdpDispatcher::new();
    let resp_frame = dispatcher.handle_ws_frame(&decoded).expect("Expected response frame");
    let resp_text = std::str::from_utf8(&resp_frame.payload).unwrap();
    assert!(resp_text.contains("\"id\":10"));
    assert!(resp_text.contains("\"nodeName\":\"#document\""));

    // Test CDP metrics
    let metrics_req = CdpRequest::parse("{\"id\": 42, \"method\": \"Performance.getMetrics\"}").unwrap();
    assert_eq!(metrics_req.id, 42);
    assert_eq!(metrics_req.method, "Performance.getMetrics");

    let metrics_resp = dispatcher.handle_request(&metrics_req);
    assert_eq!(metrics_resp.id, 42);
    let json_resp = metrics_resp.to_json();
    assert!(json_resp.contains("LayoutCount"));
}
