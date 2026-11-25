//! Regression tests for hot paths: scene sharing, click rebuilds, path batch
//! compilation and frame profiling.

use std::sync::Arc;

use strata::prelude::*;
use strata_platform::PlatformEvent;
use strata_render::compositor::DrawCommand;

fn scene_arc() -> Arc<Scene3D> {
    let mut scene = Scene3D::new();
    scene.add_mesh(Mesh3D::new(
        Geometry3D::Box {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        },
        Material3D::default(),
    ));
    Arc::new(scene)
}

fn viewport_scenes(runtime: &AppRuntime) -> Vec<Arc<Scene3D>> {
    runtime
        .display_list
        .commands
        .iter()
        .filter_map(|cmd| match cmd {
            DrawCommand::DrawViewport3D { scene, .. } => Some(Arc::clone(scene)),
            _ => None,
        })
        .collect()
}

#[test]
fn draw_viewport_3d_shares_the_scene_arc_across_compiles() {
    let shared = scene_arc();
    let view = View::new(NodeBlueprint::element(
        Tag::Div,
        vec!["w-full h-full".to_string()],
        vec![],
        vec![NodeBlueprint::element(
            Tag::Canvas,
            vec!["w-full h-full".to_string()],
            vec![Attribute::scene_payload(Arc::clone(&shared))],
            vec![],
        )],
    ));
    let mut runtime = AppRuntime::new(view, 400.0, 300.0, 1.0);
    runtime.tick_frame();
    let first = viewport_scenes(&runtime);
    runtime.compile_display_list();
    runtime.compile_display_list();
    let second = viewport_scenes(&runtime);

    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 1);
    assert!(
        Arc::ptr_eq(&first[0], &shared),
        "the command must hold the caller's Arc, not a deep copy"
    );
    assert!(Arc::ptr_eq(&first[0], &second[0]));
}

#[test]
fn click_without_handlers_does_not_rebuild_the_layout_tree() {
    let view = rsx! {
        <div class="w-full h-full">
            <div class="w-32 h-32 bg-red-500">"static"</div>
        </div>
    };
    let mut runtime = AppRuntime::new(view, 400.0, 300.0, 1.0);
    runtime.tick_frame();
    runtime.finish_frame_profile();

    runtime.step_event(&PlatformEvent::MouseDown {
        button: strata_platform::MouseButton::Left,
        x: 10.0,
        y: 10.0,
    });
    runtime.step_event(&PlatformEvent::MouseUp {
        button: strata_platform::MouseButton::Left,
        x: 10.0,
        y: 10.0,
    });
    let profile = runtime.finish_frame_profile();
    assert!(
        profile.rebuild.is_zero(),
        "press and release on an inert element must not rebuild, spent {:?}",
        profile.rebuild
    );
}

#[test]
fn frame_profile_records_phases_and_invokes_the_hook() {
    let view = rsx! { <div class="w-full h-full">"hello"</div> };
    let mut runtime = AppRuntime::new(view, 400.0, 300.0, 1.0);
    let seen = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let seen_hook = std::rc::Rc::clone(&seen);
    runtime.set_frame_profile_hook(Some(Box::new(move |_| {
        seen_hook.set(seen_hook.get() + 1);
    })));
    runtime.rebuild_layout_tree();
    runtime.tick_frame();
    let profile = runtime.frame_profile();
    assert!(!profile.layout.is_zero());
    assert!(!profile.compile.is_zero());
    let done = runtime.finish_frame_profile();
    assert_eq!(done, profile);
    assert_eq!(seen.get(), 1);
    assert!(runtime.frame_profile().total().is_zero());
}

#[test]
fn path_batch_is_skipped_when_the_frame_has_no_paths() {
    let view = rsx! { <div class="w-full h-full bg-slate-900" /> };
    let mut runtime = AppRuntime::new(view, 400.0, 300.0, 1.0);
    runtime.tick_frame();
    let frames_before = runtime.gpu_path_pipeline.compiled_frames;
    runtime.compile_display_list();
    runtime.compile_display_list();
    assert_eq!(
        runtime.gpu_path_pipeline.compiled_frames, frames_before,
        "no path commands means no path compile"
    );
    assert!(runtime.gpu_path_segments().is_empty());
}

#[test]
fn dirty_gated_compile_skips_unchanged_frames_and_recompiles_on_change() {
    let view = rsx! { <div class="w-full h-full bg-slate-900">"static"</div> };
    let mut runtime = AppRuntime::new(view, 400.0, 300.0, 1.0);
    runtime.tick_frame();
    assert!(!runtime.is_paint_dirty());

    let skipped = runtime.compiles_skipped();
    assert!(!runtime.compile_display_list_if_dirty());
    assert!(!runtime.compile_display_list_if_dirty());
    assert_eq!(runtime.compiles_skipped(), skipped + 2);

    runtime.mark_paint_dirty();
    assert!(runtime.compile_display_list_if_dirty());
    assert!(!runtime.is_paint_dirty());
}

#[test]
fn input_that_requests_a_redraw_marks_the_list_dirty() {
    let inert = rsx! { <div class="w-full h-full">"x"</div> };
    let mut inert_rt = AppRuntime::new(inert, 400.0, 300.0, 1.0);
    inert_rt.tick_frame();
    assert!(!inert_rt.is_paint_dirty());
    let inert_redraw = inert_rt.step_event(&PlatformEvent::MouseDown {
        button: strata_platform::MouseButton::Left,
        x: 5.0,
        y: 5.0,
    });
    assert!(!inert_redraw, "inert click should not request a redraw");
    assert!(!inert_rt.is_paint_dirty());

    let view =
        rsx! { <div class="w-full h-full hover:bg-neutral-800 active:bg-neutral-700">"x"</div> };
    let mut runtime = AppRuntime::new(view, 400.0, 300.0, 1.0);
    runtime.tick_frame();
    let _ = runtime.step_event(&PlatformEvent::MouseMove { x: 5.0, y: 5.0 });
    runtime.tick_frame();
    assert!(!runtime.is_paint_dirty());
    let redraw = runtime.step_event(&PlatformEvent::MouseDown {
        button: strata_platform::MouseButton::Left,
        x: 5.0,
        y: 5.0,
    });
    assert!(redraw);
    assert!(runtime.is_paint_dirty());
}

#[test]
fn incremental_reconcile_works_with_design_system_stylesheet_and_focused_input() {
    let label = strata_dom::Signal::named("Initial".to_string(), "status_label");
    let build = {
        let label = label.clone();
        move || {
            let txt = label.get();
            rsx! {
                <div class="w-full h-full flex flex-col gap-2 p-4">
                    <style>{":root { --accent: #3b82f6; } .tool-btn { background: #1e293b; } .tool-btn:hover { background: #2563eb; } .resizer { width: 4px; }"}</style>
                    <div class="flex gap-2">
                        <button class="tool-btn w-24 h-8">"Tool A"</button>
                        <button class="tool-btn w-24 h-8">"Tool B"</button>
                        <div class="resizer h-8" />
                    </div>
                    <input id="cmd_input" type="text" class="w-64 h-8 px-2" />
                    <div class="text-sm">{txt}</div>
                </div>
            }
        }
    };

    let mut rt = AppRuntime::from_root(&build, 800.0, 600.0, 1.0);
    // Find the <input> node's actual border_box center and click it so focus is active
    let input_rect = rt
        .layout_root
        .children
        .iter()
        .find(|n| (n.geometry.border_box.width - 256.0).abs() < 2.0)
        .expect("w-64 input should exist in layout_root")
        .geometry
        .border_box;
    let _ = rt.step_event(&PlatformEvent::MouseDown {
        button: strata_platform::MouseButton::Left,
        x: input_rect.x + input_rect.width * 0.5,
        y: input_rect.y + input_rect.height * 0.5,
    });
    assert!(
        rt.active_focus.is_some() || rt.current_focus_path.is_some(),
        "input should be focused"
    );

    let _ = strata_dom::take_dirty_signals();
    let full_before = rt.full_rebuilds_count();
    label.set("Updated".to_string());
    assert_eq!(
        strata_dom::peek_dirty_signal_names(),
        vec!["status_label".to_string()],
        "named signal write must be attributed cleanly"
    );
    let _ = strata_dom::take_dirty_signals();

    rt.update_root(&build);
    assert!(
        rt.last_update_reconciled(),
        "static design-system <style> sheet + focused <input> must NOT disable incremental reconciliation; bailout={:?}",
        rt.last_reconcile_bailout()
    );
    assert!(
        rt.last_reconcile_stats().reused_subtrees > 0,
        "unrelated subtrees must be reused incrementally"
    );
    assert_eq!(
        rt.full_rebuilds_count(),
        full_before,
        "incremental update must not increment full_rebuilds_count"
    );
}

#[test]
fn fast_hover_restyle_works_with_stylesheet_and_active_transition() {
    let view = rsx! {
        <div class="w-full h-full flex flex-col gap-2 p-4">
            <style>{":root { --bg: #0f172a; } .tool-btn { background: #1e293b; color: #e2e8f0; } .tool-btn:hover { background: #2563eb; color: #ffffff; }"}</style>
            <button class="tool-btn w-32 h-10 transition-colors duration-300">"Hover Me"</button>
            <div class="w-32 h-10 bg-slate-800 hover:bg-emerald-600 transition-colors duration-300">"Other"</div>
        </div>
    };
    let mut rt = AppRuntime::new(view, 600.0, 400.0, 1.0);
    rt.tick_frame();

    let layouts_before = rt.layout_computations();
    let full_before = rt.full_rebuilds_count();

    // Hover button 1 (starts a color transition AND matches .tool-btn:hover from <style>!)
    let redraw1 = rt.step_event(&PlatformEvent::MouseMove { x: 40.0, y: 30.0 });
    assert!(redraw1);
    assert!(
        rt.last_hover_fast_path_used(),
        "hovering stylesheet .tool-btn:hover must use fast paint-only hover path; bailout={:?}",
        rt.last_hover_fast_path_bailout()
    );
    assert_eq!(
        rt.layout_computations(),
        layouts_before,
        "paint-only hover must not run full layout"
    );
    assert_eq!(
        rt.full_rebuilds_count(),
        full_before,
        "paint-only hover must not trigger a full layout tree rebuild"
    );

    // Move hover to button 2 WHILE button 1's transition is still active!
    let redraw2 = rt.step_event(&PlatformEvent::MouseMove { x: 40.0, y: 80.0 });
    assert!(redraw2);
    assert!(
        rt.last_hover_fast_path_used(),
        "active transition must NOT globally disable fast hover restyle; bailout={:?}",
        rt.last_hover_fast_path_bailout()
    );
    assert_eq!(rt.layout_computations(), layouts_before);
}

#[test]
fn gpu_2d_path_compilation_is_gated_when_disabled() {
    let view = rsx! {
        <div class="w-full h-full p-4">
            <svg width="100" height="100">
                <path d="M 10 10 L 90 90" stroke="#ffffff" stroke_width="2" />
            </svg>
        </div>
    };
    let mut rt = AppRuntime::new(view, 400.0, 300.0, 1.0);
    rt.set_gpu_2d_paths_enabled(false);
    rt.gpu_path_pipeline.compiled_frames = 0;
    rt.gpu_path_pipeline.batch.segments.clear();

    rt.compile_display_list();
    assert_eq!(
        rt.gpu_path_pipeline.compiled_frames, 0,
        "GPU 2D path tessellation must not run when gpu_2d_paths_enabled == false"
    );
    assert!(rt.gpu_path_segments().is_empty());

    rt.set_gpu_2d_paths_enabled(true);
    rt.compile_display_list();
    assert!(
        rt.gpu_path_pipeline.compiled_frames >= 1,
        "GPU 2D path tessellation must run once opted in"
    );
    assert!(!rt.gpu_path_segments().is_empty());
}

#[test]
fn real_frame_profile_summary_and_reactive_telemetry_work_end_to_end() {
    let mut rt = AppRuntime::new(
        rsx! {
            <div class="w-full h-full flex flex-col p-4">
                <div class="text-white">"Telemetry Test"</div>
            </div>
        },
        400.0,
        300.0,
        1.0,
    );
    #[cfg(feature = "devtools")]
    {
        rt.enable_devtools();
    }

    let mut compositor = strata_platform::SwapchainCompositor::new(400, 300, 1.0);
    let mut presenter = strata_platform::SoftwareSurfacePresenter::new(
        400,
        300,
        strata_platform::SwapchainFormat::Rgba8Unorm,
    );

    rt.render_frame(&mut compositor, &mut presenter, true)
        .expect("render_frame 1");
    rt.render_frame(&mut compositor, &mut presenter, true)
        .expect("render_frame 2");

    let last = rt
        .last_frame_profile()
        .expect("last_frame_profile must be populated");
    assert!(last.frame_index >= 2);
    assert!(last.dom_nodes > 0);
    assert!(last.layout_nodes > 0);
    assert!(last.display_cmds > 0);

    let summary = rt.perf_profile_summary();
    assert_eq!(summary.window_frames, 2);
    assert!(summary.total_frames >= 2);
    assert!(summary.last_frame.is_some());

    #[cfg(feature = "devtools")]
    {
        let dt = rt.devtools().expect("devtools enabled");
        let sample = dt.profiler.samples().last().expect("real timing sample");
        // Verify hardcoded fake profiler signature (100, 150, 200, 250, 300, 350) is completely gone!
        assert!(
            !(sample.input_dispatch_us == 100
                && sample.signal_drain_us == 150
                && sample.parallel_reconcile_us == 200
                && sample.style_cascade_us == 250
                && sample.layout_solve_us == 300
                && sample.display_list_build_us == 350),
            "FrameTimingSample must record real measured timings, not synthetic mock constants"
        );
        assert!(sample.pid > 0);
        assert!(sample.ui_thread_id > 0);
        let trace_json = dt.profiler.export_trace_json();
        assert!(trace_json.contains("\"name\":\"thread_name\""));
        assert!(!dt.time_travel.is_empty());
        assert_eq!(dt.live_telemetry.dom_node_count, last.dom_nodes as usize);
    }
}

#[test]
fn mcp_key_press_and_mouse_wheel_invoke_user_event_hook_and_preserve_hover_fast_path() {
    use serde_json::json;
    use strata_platform::{KeyCode, Modifiers, PlatformEvent};

    let mut rt = AppRuntime::new(
        rsx! {
            <div class="w-[400px] h-[300px] p-4 bg-slate-900">
                <button class="w-[120px] h-[40px] bg-blue-600 hover:bg-blue-400">"Action"</button>
            </div>
        },
        400.0,
        300.0,
        1.0,
    );
    rt.tick_frame();

    let received = std::cell::RefCell::new(Vec::<PlatformEvent>::new());
    let mut hook = |ev: &PlatformEvent, _rt: &mut AppRuntime| -> bool {
        received.borrow_mut().push(ev.clone());
        matches!(
            ev,
            PlatformEvent::KeyDown {
                key: KeyCode::F3
                    | KeyCode::Comma
                    | KeyCode::Slash
                    | KeyCode::KeyK
                    | KeyCode::Space
                    | KeyCode::Escape,
                ..
            } | PlatformEvent::Scroll { .. }
        )
    };

    // 1. Key presses matching global host hotkeys (F3, Ctrl+,, Ctrl+/, Ctrl+K, Ctrl+Shift+Space, Escape)
    let combos = [
        ("F3", json!({})),
        (",", json!({ "ctrl": true })),
        ("/", json!({ "ctrl": true })),
        ("k", json!({ "ctrl": true })),
        ("Space", json!({ "ctrl": true, "shift": true })),
        ("Escape", json!({})),
    ];
    for (key_str, mods) in combos {
        let (res, outcome) = strata::mcp_bridge::execute_mcp_command_with_hook(
            &mut rt,
            "keyboard_key",
            json!({ "key": key_str, "modifiers": mods }),
            &mut hook,
        );
        assert!(res.is_ok(), "key_press {key_str} must succeed: {res:?}");
        assert!(
            outcome.needs_rebuild && outcome.needs_redraw,
            "host event hook returning true for {key_str} must request rebuild + redraw"
        );
    }

    assert!(received.borrow().iter().any(|e| matches!(
        e,
        PlatformEvent::KeyDown {
            key: KeyCode::F3,
            ..
        }
    )));
    assert!(received.borrow().iter().any(|e| matches!(
        e,
        PlatformEvent::KeyDown {
            key: KeyCode::Comma,
            modifiers: Modifiers { ctrl: true, .. }
        }
    )));
    assert!(received.borrow().iter().any(|e| matches!(
        e,
        PlatformEvent::KeyDown {
            key: KeyCode::Slash,
            modifiers: Modifiers { ctrl: true, .. }
        }
    )));
    assert!(received.borrow().iter().any(|e| matches!(
        e,
        PlatformEvent::KeyDown {
            key: KeyCode::KeyK,
            modifiers: Modifiers { ctrl: true, .. }
        }
    )));
    assert!(received.borrow().iter().any(|e| matches!(
        e,
        PlatformEvent::KeyDown {
            key: KeyCode::Space,
            modifiers: Modifiers {
                ctrl: true,
                shift: true,
                ..
            }
        }
    )));
    assert!(received.borrow().iter().any(|e| matches!(
        e,
        PlatformEvent::KeyDown {
            key: KeyCode::Escape,
            ..
        }
    )));

    // 2. Mouse wheel scroll reaches host event hook
    received.borrow_mut().clear();
    let (scroll_res, scroll_outcome) = strata::mcp_bridge::execute_mcp_command_with_hook(
        &mut rt,
        "mouse_wheel",
        json!({ "delta_x": 0.0, "delta_y": -120.0, "x": 200.0, "y": 150.0 }),
        &mut hook,
    );
    assert!(scroll_res.is_ok());
    assert!(scroll_outcome.needs_rebuild);
    assert!(received.borrow().iter().any(|e| matches!(
        e,
        PlatformEvent::Scroll { delta_y, .. } if (*delta_y + 120.0).abs() < 0.01
    )));

    // 3. Paint-only hover mouse_move where hook returns false MUST keep needs_rebuild == false
    received.borrow_mut().clear();
    let (mv_res, mv_outcome) = strata::mcp_bridge::execute_mcp_command_with_hook(
        &mut rt,
        "mouse_move",
        json!({ "x": 50.0, "y": 30.0, "smooth": false }),
        &mut hook,
    );
    assert!(mv_res.is_ok());
    assert!(received
        .borrow()
        .iter()
        .any(|e| matches!(e, PlatformEvent::MouseMove { .. })));
    assert!(
        !mv_outcome.needs_rebuild,
        "hover mouse_move when hook returns false must preserve paint-only fast path"
    );
    assert!(mv_outcome.needs_redraw);
}

#[test]
fn gpu_present_timings_record_raster_wgpu_and_present_separately() {
    let mut rt = AppRuntime::new(
        rsx! {
            <div class="w-[320px] h-[200px] bg-slate-900 text-white p-2">"Hybrid Timings"</div>
        },
        320.0,
        200.0,
        1.0,
    );
    rt.tick_frame();

    let timings = strata_platform::GpuPresentTimings {
        presented: true,
        raster_2d: std::time::Duration::from_micros(420),
        wgpu_3d_submit: std::time::Duration::from_micros(950),
        swapchain_present: std::time::Duration::from_micros(180),
    };
    assert!(timings.presented);
    rt.record_raster_time(timings.raster_2d);
    rt.record_wgpu_3d_submit_time(timings.wgpu_3d_submit);
    rt.record_present_time(timings.swapchain_present);
    rt.finalize_frame_telemetry("Dirty");

    let last = rt.last_frame_profile().expect("frame profile recorded");
    assert_eq!(last.raster_us, 420);
    assert_eq!(last.wgpu_3d_submit_us, 950);
    assert_eq!(last.present_us, 180);

    let summary = rt.perf_profile_summary();
    assert_eq!(summary.raster.max_us, 420);
    assert_eq!(summary.wgpu_3d_submit.max_us, 950);
    assert_eq!(summary.present.max_us, 180);

    // Verify rolling PerfPercentileStats for input_dispatch, view_render, style_cascade, and gpu_2d_overlay
    rt.record_input_dispatch_time(std::time::Duration::from_micros(310));
    rt.record_view_render_time(std::time::Duration::from_micros(540));
    rt.update_view(rsx! {
        <div class="w-[320px] h-[200px] bg-slate-800 text-white p-3">"Updated"</div>
    });
    rt.finalize_frame_telemetry("Dirty");
    let summary2 = rt.perf_profile_summary();
    assert_eq!(summary2.input_dispatch.max_us, 310);
    assert_eq!(summary2.view_render.max_us, 540);
    assert_eq!(summary2.window_frames, 2);

    let json_val = serde_json::to_value(&summary2).expect("PerfProfileSummary JSON serialization");
    for field in [
        "input_dispatch",
        "view_render",
        "style_cascade",
        "gpu_2d_overlay",
    ] {
        assert!(
            json_val.get(field).and_then(|v| v.get("p95_us")).is_some(),
            "PerfProfileSummary JSON must expose rolling percentiles for `{field}`"
        );
    }
}

#[test]
fn incremental_reconcile_preserves_clean_wrapped_log_rows_across_compute_layout() {
    fn build_log_app(counter: usize) -> strata_dom::View {
        let mut rows = Vec::with_capacity(150);
        for i in 0..150 {
            let tag = format!("[{:03}]", i);
            let msg = format!(
                "Worker telemetry event #{i}: orbital packet synchronized across node cluster alpha-beta-gamma with deterministic checksum verification."
            );
            rows.push(rsx! {
                <div class="flex flex-row items-start gap-2 w-full py-1">
                    <span class="shrink-0 text-xs text-blue-400">{tag}</span>
                    <span class="flex-1 min-w-0 text-xs text-slate-200">{msg}</span>
                </div>
            });
        }
        let header = format!("Frames: {}", counter);
        rsx! {
            <div class="w-[1000px] h-[700px] flex flex-col bg-slate-950 p-4 gap-2">
                <div class="flex flex-row items-center justify-between h-[32px] shrink-0">
                    <span class="text-sm text-white">{header}</span>
                </div>
                <div class="flex-1 min-h-0 flex flex-col overflow-y-auto">
                    {rows}
                </div>
            </div>
        }
    }

    let mut rt = AppRuntime::new(build_log_app(0), 1000.0, 700.0, 1.0);
    rt.tick_frame();
    let cold_log_container = &rt.layout_root.children[1];
    assert_eq!(cold_log_container.children.len(), 150);
    assert!(cold_log_container.children_layout_clean);
    let baseline_row_75 = cold_log_container.children[75].geometry.border_box;
    let baseline_row_149 = cold_log_container.children[149].geometry.border_box;
    assert!(baseline_row_75.height > 0.0);
    assert!(baseline_row_149.y > baseline_row_75.y);

    // Mutate only the header counter (tiny subtree change; all 150 wrapped log rows are unchanged).
    rt.update_root(|| build_log_app(1));
    assert!(
        rt.last_update_reconciled(),
        "header update must reconcile incrementally"
    );
    let stats = rt.last_reconcile_stats();
    assert!(
        stats.reused_subtrees >= 150,
        "all 150 untouched log rows must be reused, got reused={}",
        stats.reused_subtrees
    );

    let after_log_container = &rt.layout_root.children[1];
    assert!(after_log_container.children_layout_clean);
    assert!(
        after_log_container.measure_cache[0].is_valid
            || after_log_container.measure_cache[1].is_valid
            || after_log_container.layout_cache.is_valid,
        "untouched log container must retain valid layout/measure cache across incremental updates"
    );
    assert!(
        (after_log_container.children[75].geometry.border_box.y - baseline_row_75.y).abs() < 0.05
            && (after_log_container.children[75].geometry.border_box.height - baseline_row_75.height)
                .abs()
                < 0.05,
        "cached row 75 geometry must stay identical"
    );
    assert!(
        (after_log_container.children[149].geometry.border_box.y - baseline_row_149.y).abs() < 0.05,
        "cached row 149 geometry must stay identical"
    );
}

