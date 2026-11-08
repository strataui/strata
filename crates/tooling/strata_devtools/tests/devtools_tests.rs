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

#[test]
fn test_cdp_live_dom_and_computed_styles_inspection() {
    use std::collections::HashMap;
    use strata_dom::{FixedRect, IrTag, UiIrGraph};
    use strata_style::types::{Color, Dimension, Display, Position};
    use strata_style::ComputedStyle;

    let mut graph = UiIrGraph::new();
    let root = graph.alloc_node(IrTag::Div);
    let button = graph.alloc_node(IrTag::Button);
    let span = graph.alloc_node(IrTag::Span);

    graph.append_child(root, button);
    graph.append_child(root, span);

    if let Some(r) = graph.get_mut(root) {
        r.layout_rect = FixedRect::from_f32(0.0, 0.0, 800.0, 600.0);
    }
    if let Some(b) = graph.get_mut(button) {
        b.layout_rect = FixedRect::from_f32(20.0, 20.0, 120.0, 40.0);
    }
    if let Some(s) = graph.get_mut(span) {
        s.layout_rect = FixedRect::from_f32(160.0, 20.0, 200.0, 40.0);
    }

    let mut styles = HashMap::new();
    let root_style = ComputedStyle {
        display: Display::Flex,
        position: Position::Relative,
        width: Dimension::Px(800.0),
        height: Dimension::Px(600.0),
        background_color: Some(Color::rgb(30, 30, 30)),
        ..Default::default()
    };
    styles.insert(2u64, root_style);

    let dispatcher = CdpDispatcher::new();
    dispatcher.update_from_ir(&graph, &styles, 120, 45);
    dispatcher.set_metrics(120, 45, 0.0025, 0.0010, 1024 * 1024);

    // 1. Verify DOM.getDocument returns real tree
    let doc_req = CdpRequest::parse(r#"{"id":1,"method":"DOM.getDocument"}"#).unwrap();
    let doc_resp = dispatcher.handle_request(&doc_req);
    assert_eq!(doc_resp.id, 1);
    let doc_json = doc_resp.to_json();
    assert!(doc_json.contains("\"nodeName\":\"#document\""));
    assert!(doc_json.contains("\"nodeName\":\"DIV\""));
    assert!(doc_json.contains("\"nodeName\":\"BUTTON\""));
    assert!(doc_json.contains("\"nodeName\":\"SPAN\""));
    assert!(doc_json.contains("\"data-rect\""));

    // 2. Verify CSS.getComputedStyleForNode returns real styles
    let css_req =
        CdpRequest::parse(r#"{"id":2,"method":"CSS.getComputedStyleForNode","params":{"nodeId":2}}"#)
            .unwrap();
    let css_resp = dispatcher.handle_request(&css_req);
    assert_eq!(css_resp.id, 2);
    let css_json = css_resp.to_json();
    assert!(css_json.contains("\"name\":\"display\",\"value\":\"flex\""));
    assert!(css_json.contains("\"name\":\"width\",\"value\":\"800.0px\""));
    assert!(css_json.contains("\"name\":\"height\",\"value\":\"600.0px\""));
    assert!(css_json.contains("\"name\":\"background-color\""));

    // 3. Verify Performance.getMetrics returns real metrics
    let perf_req = CdpRequest::parse(r#"{"id":3,"method":"Performance.getMetrics"}"#).unwrap();
    let perf_resp = dispatcher.handle_request(&perf_req);
    let perf_json = perf_resp.to_json();
    assert!(perf_json.contains("\"name\":\"Frames\",\"value\":120"));
    assert!(perf_json.contains("\"name\":\"LayoutCount\",\"value\":45"));
    assert!(perf_json.contains("\"name\":\"LayoutDuration\",\"value\":0.0025"));
}

#[test]
fn test_cdp_http_discovery_and_live_websocket_server() {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    let dispatcher = CdpDispatcher::new();
    let mut server = CdpServer::start(dispatcher, 0).expect("Failed to start CDP server");
    let port = server.port();
    assert!(port > 0);

    // 1. Test HTTP GET /json/version
    {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("Connect HTTP");
        stream
            .set_read_timeout(Some(Duration::from_millis(500)))
            .unwrap();
        stream
            .write_all(b"GET /json/version HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
            .unwrap();
        let mut resp = String::new();
        stream.read_to_string(&mut resp).unwrap();
        assert!(resp.starts_with("HTTP/1.1 200 OK"));
        assert!(resp.contains("\"Browser\":\"Strata/0.1.0\""));
        assert!(resp.contains("webSocketDebuggerUrl"));
    }

    // 2. Test HTTP GET /json
    {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("Connect HTTP");
        stream
            .set_read_timeout(Some(Duration::from_millis(500)))
            .unwrap();
        stream
            .write_all(b"GET /json HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
            .unwrap();
        let mut resp = String::new();
        stream.read_to_string(&mut resp).unwrap();
        assert!(resp.starts_with("HTTP/1.1 200 OK"));
        assert!(resp.contains("devtoolsFrontendUrl"));
    }

    // 3. Test RFC 6455 WebSocket Upgrade & JSON-RPC Exchange
    {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("Connect WS");
        stream
            .set_read_timeout(Some(Duration::from_millis(1000)))
            .unwrap();
        let upgrade_req = "GET /devtools/page/1 HTTP/1.1\r\n\
                           Host: 127.0.0.1\r\n\
                           Upgrade: websocket\r\n\
                           Connection: Upgrade\r\n\
                           Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
                           Sec-WebSocket-Version: 13\r\n\r\n";
        stream.write_all(upgrade_req.as_bytes()).unwrap();

        let mut buf = [0u8; 1024];
        let n = stream.read(&mut buf).unwrap();
        let resp_str = String::from_utf8_lossy(&buf[..n]);
        assert!(resp_str.starts_with("HTTP/1.1 101 Switching Protocols"));
        assert!(resp_str.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo="));

        // Client sends masked WebSocket text frame
        let json_msg = b"{\"id\":100,\"method\":\"Performance.getMetrics\"}";
        let mask = [0x12, 0x34, 0x56, 0x78];
        let mut client_frame = vec![0x81, 0x80 | (json_msg.len() as u8)];
        client_frame.extend_from_slice(&mask);
        for (i, &b) in json_msg.iter().enumerate() {
            client_frame.push(b ^ mask[i % 4]);
        }

        stream.write_all(&client_frame).unwrap();

        // Read server response frame
        let mut server_buf = [0u8; 2048];
        let m = stream.read(&mut server_buf).unwrap();
        let (frame, _) = WebSocketCodec::decode_frame(&server_buf[..m]).expect("Decode server WS");
        assert_eq!(frame.opcode, WebSocketOpcode::Text);
        let resp_text = std::str::from_utf8(&frame.payload).unwrap();
        assert!(resp_text.contains("\"id\":100"));
        assert!(resp_text.contains("\"name\":\"LayoutCount\""));
    }

    server.stop();
}

#[test]
fn test_cdp_bidirectional_dom_and_css_live_mutation_and_runtime_signals() {
    use std::collections::HashMap;
    use strata_dom::{FixedRect, IrTag, UiIrGraph};
    use strata_style::types::{Color, Dimension, Display, Position};
    use strata_style::ComputedStyle;

    let mut graph = UiIrGraph::new();
    let root = graph.alloc_node(IrTag::Div);
    let button = graph.alloc_node(IrTag::Button);
    let span = graph.alloc_node(IrTag::Span);

    graph.append_child(root, button);
    graph.append_child(root, span);

    if let Some(r) = graph.get_mut(root) {
        r.layout_rect = FixedRect::from_f32(0.0, 0.0, 800.0, 600.0);
    }
    if let Some(b) = graph.get_mut(button) {
        b.layout_rect = FixedRect::from_f32(10.0, 10.0, 100.0, 40.0);
    }
    if let Some(s) = graph.get_mut(span) {
        s.layout_rect = FixedRect::from_f32(120.0, 10.0, 80.0, 40.0);
    }

    let mut styles = HashMap::new();
    styles.insert(
        2u64,
        ComputedStyle {
            display: Display::Flex,
            position: Position::Relative,
            width: Dimension::Px(800.0),
            height: Dimension::Px(600.0),
            color: Color::rgb(255, 255, 255),
            ..Default::default()
        },
    );

    let dispatcher = CdpDispatcher::new();
    dispatcher.update_from_ir(&graph, &styles, 1, 1);

    // 1. Query selector for "button"
    let q_req = CdpRequest::parse(r#"{"id":10,"method":"DOM.querySelector","params":{"selector":"button"}}"#).unwrap();
    let q_res = dispatcher.handle_request(&q_req);
    assert_eq!(q_res.id, 10);
    let q_json = q_res.to_json();
    assert!(q_json.contains("\"nodeId\":3")); // root is 2, button is 3

    // 2. Query selector all
    let qa_req = CdpRequest::parse(r#"{"id":11,"method":"DOM.querySelectorAll","params":{"selector":"div"}}"#).unwrap();
    let qa_res = dispatcher.handle_request(&qa_req);
    assert!(qa_res.to_json().contains("\"nodeIds\":[2]"));

    // 3. Live DOM mutation via DOM.setNodeValue
    let set_val_req = CdpRequest::parse(r#"{"id":12,"method":"DOM.setNodeValue","params":{"nodeId":3,"value":"Click Me"}}"#).unwrap();
    let set_val_res = dispatcher.handle_request(&set_val_req);
    assert_eq!(set_val_res.id, 12);

    let doc_req = CdpRequest::parse(r#"{"id":13,"method":"DOM.getDocument"}"#).unwrap();
    let doc_res = dispatcher.handle_request(&doc_req);
    assert!(doc_res.to_json().contains("\"nodeValue\":\"Click Me\""));

    // 4. Live Attribute mutation via DOM.setAttributeValue
    let set_attr_req = CdpRequest::parse(r#"{"id":14,"method":"DOM.setAttributeValue","params":{"nodeId":3,"name":"class","value":"btn-active p-8"}}"#).unwrap();
    let set_attr_res = dispatcher.handle_request(&set_attr_req);
    assert_eq!(set_attr_res.id, 14);

    let doc_res2 = dispatcher.handle_request(&doc_req);
    assert!(doc_res2.to_json().contains("\"btn-active p-8\""));

    // 5. Live CSS mutation via CSS.setStyleTexts
    let css_edit_req = CdpRequest::parse(r#"{"id":15,"method":"CSS.setStyleTexts","params":{"nodeId":2,"text":"color: rgb(255, 0, 0); padding: 20px;"}}"#).unwrap();
    let css_edit_res = dispatcher.handle_request(&css_edit_req);
    assert_eq!(css_edit_res.id, 15);
    assert!(css_edit_res.to_json().contains("rgb(255, 0, 0)"));

    let get_css = CdpRequest::parse(r#"{"id":16,"method":"CSS.getComputedStyleForNode","params":{"nodeId":2}}"#).unwrap();
    let get_css_res = dispatcher.handle_request(&get_css);
    assert!(get_css_res.to_json().contains("rgb(255, 0, 0)"));
    assert!(get_css_res.to_json().contains("20px"));

    // 6. Runtime signal inspection via Runtime.getProperties
    dispatcher.update_runtime_signal("activeSignalCount", "42");
    dispatcher.update_runtime_signal("theme", "dark");

    let runtime_props_req = CdpRequest::parse(r#"{"id":17,"method":"Runtime.getProperties","params":{"objectId":"runtime"}}"#).unwrap();
    let runtime_props_res = dispatcher.handle_request(&runtime_props_req);
    assert_eq!(runtime_props_res.id, 17);
    let runtime_json = runtime_props_res.to_json();
    assert!(runtime_json.contains("\"name\":\"activeSignalCount\""));
    assert!(runtime_json.contains("\"value\":\"42\""));
    assert!(runtime_json.contains("\"name\":\"theme\""));
    assert!(runtime_json.contains("\"value\":\"dark\""));
}

#[test]
fn test_hud_overlay_rendering_with_9_stage_flamegraph_and_live_timings() {
    let mut hud = HudController::new();
    hud.update_bounds(1280.0, 720.0);

    // 1. When hidden, HUD overlay emits 0 commands
    assert!(!hud.state.is_visible);
    let hidden_commands = hud.render_overlay_commands(None);
    assert!(hidden_commands.is_empty());

    // 2. Toggle HUD visibility via F12 simulation
    hud.toggle_visibility();
    assert!(hud.state.is_visible);

    // Build sample frame timeline with 9 stages populated
    let mut profiler = FrameTimelineProfiler::new(60, 60.0);
    let sample = FrameTimingSample {
        frame_index: 42,
        vsync_wait_us: 120,
        input_dispatch_us: 250,
        signal_drain_us: 310,
        parallel_reconcile_us: 450,
        style_cascade_us: 520,
        layout_solve_us: 680,
        display_list_build_us: 750,
        gpu_submit_us: 890,
        gpu_raster_duration_us: 1200,
        total_cpu_frame_us: 3850,
        frame_budget_us: 16666,
        layout_shift_score: 0.001,
    };
    profiler.record_sample(sample);

    // 3. Render performance tab overlay
    hud.set_active_tab(HudTab::Performance);
    let perf_commands = hud.render_overlay_commands(Some(&profiler));
    assert!(!perf_commands.is_empty());

    // Verify background, nav bar, and accent lines were emitted
    assert!(perf_commands.len() > 15);

    // Search for performance summary text and stage names in emitted draw commands
    let mut has_summary = false;
    let mut has_reconcile_legend = false;
    let mut has_gpu_submit_legend = false;
    let mut has_gpu_raster_legend = false;

    for cmd in &perf_commands {
        if let strata_render::compositor::DrawCommand::DrawText { text, .. } = cmd {
            if text.contains("Avg CPU:") && text.contains("Jank:") {
                has_summary = true;
            }
            if text.contains("Reconcile:") {
                has_reconcile_legend = true;
            }
            if text.contains("GPU Submit:") {
                has_gpu_submit_legend = true;
            }
            if text.contains("GPU Raster:") {
                has_gpu_raster_legend = true;
            }
        }
    }

    assert!(has_summary, "HUD Performance tab must render CPU & Jank frame summary");
    assert!(has_reconcile_legend, "HUD Performance tab must render Reconcile stage breakdown");
    assert!(has_gpu_submit_legend, "HUD Performance tab must render GPU Submit timing");
    assert!(has_gpu_raster_legend, "HUD Performance tab must render GPU Raster timing");

    // 4. Inspect Element highlight overlay
    let mut dl = strata_render::compositor::DisplayList::new();
    let inspect_bounds = strata_layout::Rect {
        x: 100.0,
        y: 150.0,
        width: 300.0,
        height: 80.0,
    };
    hud.render_inspect_highlight(inspect_bounds, &mut dl);
    assert_eq!(dl.len(), 2, "Inspect highlight emits translucent overlay and border");
}


