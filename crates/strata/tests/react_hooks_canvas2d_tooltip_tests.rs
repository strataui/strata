use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;
use strata::prelude::*;
use strata::render::canvas2d::{CanvasCommand, CanvasPathVerb};
use strata::render::compositor::DrawCommand;
use strata::render::{FrameBuffer, GpuBatch, GpuPathPipeline, GpuSlugTextPipeline};
use strata::style::types::Color;
use strata_ui::canvas::{
    Canvas2D, Canvas2DContext, CanvasLineCap, CanvasLineJoin, CanvasTextAlign, CanvasTextBaseline,
};

// ---------------------------------------------------------------------------
// 1. Generic #[component], Option<String> literals, #[prop(default)], {..props}
// ---------------------------------------------------------------------------

#[component]
fn GenericList<T: std::fmt::Display + Clone + 'static>(
    items: Vec<T>,
    heading: Option<String>,
    #[prop(default = 4)] gap: usize,
) -> View {
    let gap_cls = format!("flex flex-col gap-{}", gap);
    let rows: Vec<NodeBlueprint> = items
        .into_iter()
        .map(|item| {
            let text = item.to_string();
            rsx! { <span class="text-xs">{text}</span> }.root
        })
        .collect();

    rsx! {
        <div class={gap_cls}>
            {if let Some(h) = heading {
                rsx! { <h3 class="text-sm">{h}</h3> }.root
            } else {
                NodeBlueprint::Fragment(vec![])
            }}
            {NodeBlueprint::Fragment(rows)}
        </div>
    }
}

#[test]
fn test_generic_component_option_string_default_and_spread_in_runtime() {
    let base_props = GenericList::<&'static str>::builder()
        .items(vec!["VCC", "GND", "CLK"])
        .heading("Power Nets");

    let view = rsx! {
        <div class="w-[400px] h-[200px] flex flex-col">
            <GenericList::<&'static str> {..base_props} gap={2} />
            <GenericList::<u32> items={vec![10, 20]} heading={None} />
        </div>
    };

    let rt = AppRuntime::new(view, 400.0, 200.0, 1.0);
    assert!(rt.find_by_text("Power Nets").is_some());
    assert!(rt.find_by_text("VCC").is_some());
    assert!(rt.find_by_text("CLK").is_some());
    assert!(rt.find_by_text("10").is_some());
    assert!(rt.find_by_text("20").is_some());
}

// ---------------------------------------------------------------------------
// 2. Component-scoped hooks, use_persistent_signal, Context, ErrorBoundary
// ---------------------------------------------------------------------------

#[derive(Clone, PartialEq)]
struct ThemeCtx {
    accent: &'static str,
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
enum CounterAction {
    Inc(i32),
    Reset,
}

#[component]
fn ContextConsumer(ctx: Context<ThemeCtx>, observed_id: Ref<Option<String>>) -> View {
    let theme = use_context(&ctx);
    let id = use_id();
    observed_id.set(Some(id.clone()));
    let label = format!("accent={} id={}", theme.accent, id);
    rsx! { <span>{label}</span> }
}

#[component]
fn StatefulCounter(
    theme_ctx: Context<ThemeCtx>,
    observed_id: Ref<Option<String>>,
    callback_addr: Ref<Option<usize>>,
    effect_runs: Rc<Cell<usize>>,
    effect_cleanups: Rc<Cell<usize>>,
    crash_flag: Signal<bool>,
) -> View {
    let (count, set_count) = use_state(0i32);
    let (total, dispatch) = use_reducer(
        |state: &i32, action: CounterAction| match action {
            CounterAction::Inc(delta) => *state + delta,
            CounterAction::Reset => 0,
        },
        0i32,
    );
    let render_ref = use_ref(0usize);
    render_ref.with_mut(|n| *n += 1);
    let renders = render_ref.get();

    let cb = use_callback((), {
        let set_count = set_count.clone();
        let dispatch = dispatch.clone();
        move |_: Event| {
            set_count.update(|v| *v += 1);
            dispatch(CounterAction::Inc(10));
        }
    });
    callback_addr.set(Some(Rc::as_ptr(&cb) as usize));

    let runs = effect_runs.clone();
    let cleanups = effect_cleanups.clone();
    let count_read = count.clone();
    use_effect(move || {
        let _ = count_read.get();
        runs.set(runs.get() + 1);
        let c = cleanups.clone();
        Some(Box::new(move || {
            c.set(c.get() + 1);
        }) as Box<dyn FnOnce()>)
    });

    provide_context(
        &theme_ctx,
        ThemeCtx {
            accent: "amber-500",
        },
    );

    let summary = format!(
        "count={} total={} renders={}",
        count.get(),
        total.get(),
        renders
    );
    let on_click = move |e: Event| (cb)(e);

    rsx! {
        <div class="flex flex-col w-[400px] h-[300px] p-2">
            <button class="w-[140px] h-[36px]" onclick=on_click>{summary}</button>
            <ContextConsumer ctx={theme_ctx} observed_id={observed_id} />
            <ErrorBoundary
                render={move || {
                    if crash_flag.get() {
                        panic!("simulated schematic fault");
                    }
                    rsx! { <span>"Healthy Circuit"</span> }
                }}
                fallback={move |info: ErrorBoundaryInfo| {
                    let msg = format!("Recovered: {}", info.message);
                    let reset = info.reset;
                    rsx! {
                        <div class="flex flex-col">
                            <span>{msg}</span>
                            <button class="w-[100px] h-[30px]" onclick={move |_: Event| reset()}>"Reset Fault"</button>
                        </div>
                    }
                }}
            />
        </div>
    }
}

#[test]
fn test_component_scoped_hooks_context_error_boundary_and_prelude_persistent_signal() {
    reset_component_hooks();
    clear_persistent_signals();

    // Verify use_persistent_signal is available directly from strata::prelude::*.
    let persisted = use_persistent_signal("test:netlist:zoom", || 125u32);
    assert_eq!(persisted.get(), 125);
    persisted.set(200);
    assert_eq!(
        use_persistent_signal("test:netlist:zoom", || 1u32).get(),
        200
    );

    let theme_ctx = create_context(ThemeCtx {
        accent: "slate-400",
    });
    let observed_id = Ref::new(None);
    let callback_addr = Ref::new(None);
    let effect_runs = Rc::new(Cell::new(0));
    let effect_cleanups = Rc::new(Cell::new(0));
    let crash_flag = Signal::new(false);

    let build_root = {
        let theme_ctx = theme_ctx.clone();
        let observed_id = observed_id.clone();
        let callback_addr = callback_addr.clone();
        let effect_runs = effect_runs.clone();
        let effect_cleanups = effect_cleanups.clone();
        let crash_flag = crash_flag.clone();
        move || {
            rsx! {
                <StatefulCounter
                    theme_ctx={theme_ctx.clone()}
                    observed_id={observed_id.clone()}
                    callback_addr={callback_addr.clone()}
                    effect_runs={effect_runs.clone()}
                    effect_cleanups={effect_cleanups.clone()}
                    crash_flag={crash_flag.clone()}
                />
            }
        }
    };

    let mut rt = AppRuntime::new(build_root(), 400.0, 300.0, 1.0);
    let first_id = observed_id.get().expect("use_id should emit an id");
    let first_cb_addr = callback_addr.get().expect("use_callback should run");
    assert!(rt.find_by_text("count=0 total=0 renders=1").is_some());
    assert!(rt
        .find_by_text(&format!("accent=amber-500 id={}", first_id))
        .is_some());
    assert!(rt.find_by_text("Healthy Circuit").is_some());
    assert_eq!(effect_runs.get(), 1);
    assert_eq!(effect_cleanups.get(), 0);

    // Click the stateful button -> updates use_state + use_reducer, then rebuild root view.
    assert!(rt.click_text("count=0 total=0 renders=1"));
    rt.update_view(build_root());

    // State persisted across full root rebuild without thread-local globals!
    assert!(rt.find_by_text("count=1 total=10 renders=2").is_some());
    assert_eq!(observed_id.get().as_deref(), Some(first_id.as_str()));
    assert_eq!(callback_addr.get(), Some(first_cb_addr));
    assert_eq!(effect_runs.get(), 2);
    assert_eq!(effect_cleanups.get(), 1);

    // Click again to verify continued persistence.
    assert!(rt.click_text("count=1 total=10 renders=2"));
    rt.update_view(build_root());
    assert!(rt.find_by_text("count=2 total=20 renders=3").is_some());

    // Trigger ErrorBoundary panic inside child render and verify fallback + recovery.
    crash_flag.set(true);
    rt.update_view(build_root());
    assert!(rt
        .find_by_text("Recovered: simulated schematic fault")
        .is_some());
    assert!(rt.find_by_text("Healthy Circuit").is_none());

    crash_flag.set(false);
    assert!(rt.click_text("Reset Fault"));
    rt.update_view(build_root());
    assert!(rt.find_by_text("Healthy Circuit").is_some());

    // Clear component hooks cleanly at end of test.
    reset_component_hooks();
    clear_persistent_signals();
}

// ---------------------------------------------------------------------------
// 3. Canvas2DContext: text, dashed strokes, cubic/quadratic Beziers, clip()
// ---------------------------------------------------------------------------

#[test]
fn test_canvas2d_text_dashed_strokes_beziers_and_clip_end_to_end() {
    let view = rsx! {
        <div class="w-[320px] h-[240px]">
            <Canvas2D
                width={240.0}
                height={160.0}
                on_draw={|ctx: &mut Canvas2DContext| {
                    // Clipped solid fill: only [20..80, 20..80] should be painted
                    ctx.save();
                    ctx.begin_path();
                    ctx.rect(20.0, 20.0, 60.0, 60.0);
                    ctx.clip();
                    ctx.set_fill_style(Color::new(220, 40, 40, 255));
                    ctx.fill_rect(0.0, 0.0, 240.0, 160.0);
                    ctx.restore();

                    // Dashed oscilloscope cursor line
                    ctx.set_stroke_style(Color::new(56, 189, 248, 255));
                    ctx.set_line_width(2.0);
                    ctx.set_line_dash(&[6.0, 4.0]);
                    ctx.begin_path();
                    ctx.move_to(10.0, 100.0);
                    ctx.line_to(220.0, 100.0);
                    ctx.stroke();

                    // Cubic & quadratic Bezier schematic trace
                    ctx.set_line_dash(&[]);
                    ctx.set_stroke_style(Color::new(34, 197, 94, 255));
                    ctx.begin_path();
                    ctx.move_to(10.0, 140.0);
                    ctx.bezier_curve_to(60.0, 80.0, 140.0, 150.0, 190.0, 90.0);
                    ctx.quadratic_curve_to(210.0, 60.0, 230.0, 120.0);
                    ctx.stroke();

                    // Net labels via fill_text & stroke_text
                    ctx.set_font("bold 12px monospace");
                    ctx.set_text_align(CanvasTextAlign::Center);
                    ctx.set_text_baseline(CanvasTextBaseline::Middle);
                    ctx.set_fill_style(Color::new(248, 250, 252, 255));
                    ctx.fill_text("CLK_OUT", 120.0, 30.0);
                    ctx.stroke_text("R1", 120.0, 50.0);
                }}
            />
        </div>
    };

    let rt = AppRuntime::new(view, 320.0, 240.0, 1.0);

    // 1. Inspect emitted CanvasCommand variants in DisplayList
    let canvas_cmds = rt
        .display_list
        .commands()
        .iter()
        .find_map(|cmd| match cmd {
            DrawCommand::DrawCanvas2D { commands, .. } => Some(commands),
            _ => None,
        })
        .expect("DisplayList should include DrawCanvas2D");

    assert!(canvas_cmds.iter().any(|c| matches!(c, CanvasCommand::Save)));
    assert!(canvas_cmds
        .iter()
        .any(|c| matches!(c, CanvasCommand::Restore)));
    assert!(canvas_cmds
        .iter()
        .any(|c| matches!(c, CanvasCommand::ClipPath { .. })));
    assert!(canvas_cmds.iter().any(|c| matches!(
        c,
        CanvasCommand::SetLineDash { segments, .. } if segments == &[6.0, 4.0]
    )));
    assert!(canvas_cmds.iter().any(|c| matches!(
        c,
        CanvasCommand::DrawPath { verbs, .. }
            if verbs.iter().any(|v| matches!(v, CanvasPathVerb::BezierCurveTo { .. }))
            && verbs.iter().any(|v| matches!(v, CanvasPathVerb::QuadraticCurveTo { .. }))
    )));
    assert!(canvas_cmds.iter().any(|c| matches!(
        c,
        CanvasCommand::FillText { text, font_weight: 700, .. } if text == "CLK_OUT"
    )));
    assert!(canvas_cmds.iter().any(|c| matches!(
        c,
        CanvasCommand::StrokeText { text, .. } if text == "R1"
    )));

    // 2. Compile through GPU path + Slug text pipelines
    let mut path_pipe = GpuPathPipeline::new(320.0, 240.0);
    let seg_count = path_pipe.compile_frame(&rt.display_list, None);
    assert!(
        seg_count > 10,
        "GPU path pipeline should expand dashed lines and cubic/quadratic Beziers into segments, got {seg_count}"
    );

    let mut text_pipe = GpuSlugTextPipeline::new();
    let glyph_count = text_pipe.compile_display_list(&rt.display_list, 320.0, 240.0, None);
    assert!(
        glyph_count >= 5,
        "GPU Slug text pipeline should compile Canvas2D FillText/StrokeText glyphs, got {glyph_count}"
    );

    // 3. Software rasterization honors clip()
    let mut fb = FrameBuffer::new(320, 240);
    fb.execute_display_list(&rt.display_list);
    let inside = fb.get_pixel(40, 40);
    let outside = fb.get_pixel(5, 5);
    assert_eq!(inside.r, 220, "Red fill inside clip rect must render");
    assert_ne!(
        outside.r, 220,
        "Red fill outside clip rect must be clipped out"
    );
}

// ---------------------------------------------------------------------------
// 4. Hover tooltip delay & styling configuration
// ---------------------------------------------------------------------------

#[test]
fn test_hover_tooltip_delay_and_style_via_window_config_and_builder() {
    let custom_style = HoverTooltipStyle::default()
        .with_background_color(Color::new(10, 25, 47, 255))
        .with_border_color(Color::new(100, 255, 218, 255))
        .with_text_color(Color::new(230, 241, 255, 255))
        .with_shadow_color(Color::new(0, 0, 0, 0))
        .with_font_size(11.0)
        .with_padding(6.0, 3.0)
        .with_corner_radius(3.0)
        .with_border_width(2.0);

    let cfg = WindowConfig {
        width: 400,
        height: 200,
        ..Default::default()
    }
    .with_tooltip_delay_ms(0)
    .with_tooltip_style(custom_style);

    assert_eq!(cfg.tooltip_delay, Duration::from_millis(0));
    assert_eq!(cfg.tooltip_style, custom_style);

    let mut app = StrataApp::builder().with_window(cfg).build(|| {
        rsx! {
            <div class="w-[400px] h-[200px] p-4">
                <button class="w-[120px] h-[36px]" title="Route Trace (W)">"Route"</button>
            </div>
        }
    });

    assert_eq!(app.runtime.hover_tooltip_delay(), Duration::from_millis(0));
    assert_eq!(app.runtime.hover_tooltip_style(), custom_style);

    // Move pointer onto button (16..136, 16..52)
    app.runtime.update_hover(40.0, 30.0);
    assert!(
        app.runtime.poll_hover_tooltip(),
        "0ms tooltip delay should activate immediately on poll_hover_tooltip"
    );
    assert!(app.runtime.hover_tooltip_visible);

    // Verify custom style was emitted to DisplayList (and 0-alpha shadow omitted)
    let has_shadow = app
        .runtime
        .display_list
        .commands()
        .iter()
        .any(|c| matches!(c, DrawCommand::DrawShadow { .. }));
    assert!(
        !has_shadow,
        "shadow_color alpha 0 should omit DrawShadow command"
    );

    let tooltip_rect = app.runtime.display_list.commands().iter().find(|c| {
        matches!(
            c,
            DrawCommand::DrawRoundedRect {
                color,
                border_color: Some(bc),
                border_width,
                border_radius,
                ..
            } if *color == Color::new(10, 25, 47, 255)
                && *bc == Color::new(100, 255, 218, 255)
                && (border_width.top - 2.0).abs() < 1e-3
                && (border_radius.top_left - 3.0).abs() < 1e-3
        )
    });
    assert!(
        tooltip_rect.is_some(),
        "Tooltip background/border/radius should match custom HoverTooltipStyle"
    );

    let tooltip_text = app.runtime.display_list.commands().iter().find(|c| {
        matches!(
            c,
            DrawCommand::DrawText {
                text,
                font_size,
                color,
                ..
            } if text == "Route Trace (W)"
                && (*font_size - 11.0).abs() < 1e-3
                && *color == Color::new(230, 241, 255, 255)
        )
    });
    assert!(
        tooltip_text.is_some(),
        "Tooltip text should match custom font_size and text_color"
    );

    // Verify StrataAppBuilder direct methods + AppRuntime runtime setters also work
    let mut app2 = StrataApp::builder()
        .with_tooltip_delay_ms(500)
        .with_tooltip_style(HoverTooltipStyle::default().with_border_width(0.0))
        .build(|| {
            rsx! {
                <div class="w-[400px] h-[200px] p-4">
                    <button class="w-[120px] h-[36px]" title="Inspect Net">"Net"</button>
                </div>
            }
        });

    assert_eq!(
        app2.runtime.hover_tooltip_delay(),
        Duration::from_millis(500)
    );
    app2.runtime.update_hover(40.0, 30.0);
    assert!(
        !app2.runtime.poll_hover_tooltip(),
        "500ms tooltip delay should NOT activate immediately"
    );

    // Lowering delay at runtime via helper should allow immediate activation.
    app2.runtime.set_hover_tooltip_delay_ms(0);
    assert!(app2.runtime.poll_hover_tooltip());
}

// ---------------------------------------------------------------------------
// 7. Top-level unkeyed sibling isolation & unmount cleanup without outer #[component]
// ---------------------------------------------------------------------------

#[component]
fn UnkeyedSiblingCounter(
    initial: i32,
    state_out: Rc<Cell<i32>>,
    setter_out: Rc<std::cell::RefCell<Option<WriteSignal<i32>>>>,
    cleanups: Rc<Cell<usize>>,
) -> View {
    let (val, set_val) = use_state(|| initial);
    state_out.set(val.get());
    *setter_out.borrow_mut() = Some(set_val);

    use_effect(move || {
        let c = cleanups.clone();
        Some(Box::new(move || {
            c.set(c.get() + 1);
        }) as Box<dyn FnOnce()>)
    });

    let label = format!("count:{}", val.get());
    rsx! { <span class="text-xs">{label}</span> }
}

#[test]
fn top_level_unkeyed_siblings_isolate_hooks_and_run_unmount_cleanup() {
    reset_component_hooks();

    let s1_out = Rc::new(Cell::new(0));
    let s2_out = Rc::new(Cell::new(0));
    let set1_slot = Rc::new(std::cell::RefCell::new(None::<WriteSignal<i32>>));
    let set2_slot = Rc::new(std::cell::RefCell::new(None::<WriteSignal<i32>>));
    let cleanups1 = Rc::new(Cell::new(0usize));
    let cleanups2 = Rc::new(Cell::new(0usize));
    let show_second = Rc::new(Cell::new(true));

    let build_root = {
        let s1 = s1_out.clone();
        let s2 = s2_out.clone();
        let set1 = set1_slot.clone();
        let set2 = set2_slot.clone();
        let c1 = cleanups1.clone();
        let c2 = cleanups2.clone();
        let show = show_second.clone();
        move || {
            rsx! {
                <div class="w-[300px] h-[100px] flex flex-row">
                    <UnkeyedSiblingCounter
                        initial={10}
                        state_out={s1.clone()}
                        setter_out={set1.clone()}
                        cleanups={c1.clone()}
                    />
                    {if show.get() {
                        rsx! {
                            <UnkeyedSiblingCounter
                                initial={99}
                                state_out={s2.clone()}
                                setter_out={set2.clone()}
                                cleanups={c2.clone()}
                            />
                        }.root
                    } else {
                        NodeBlueprint::Fragment(vec![])
                    }}
                </div>
            }
        }
    };

    // Plain closure passed to AppRuntime::from_root (no outer #[component])
    let mut rt = strata::AppRuntime::from_root(&build_root, 300.0, 100.0, 1.0);
    assert_eq!(s1_out.get(), 10);
    assert_eq!(s2_out.get(), 99);
    assert_eq!(cleanups1.get(), 0);
    assert_eq!(cleanups2.get(), 0);

    // Mutate only the first sibling's state and re-render via update_root
    set1_slot.borrow().as_ref().unwrap().set(15);
    rt.update_root(&build_root);
    assert_eq!(s1_out.get(), 15, "first sibling state updated");
    assert_eq!(
        s2_out.get(),
        99,
        "second unkeyed top-level sibling must not collide with first sibling's hook slot"
    );
    assert_eq!(cleanups1.get(), 0);
    assert_eq!(cleanups2.get(), 0);

    // Toggle second sibling off -> its unmount cleanup runs immediately, first stays mounted
    show_second.set(false);
    rt.update_root(&build_root);
    assert_eq!(cleanups1.get(), 0);
    assert_eq!(
        cleanups2.get(),
        1,
        "toggling off top-level unkeyed sibling must run use_effect unmount cleanup"
    );

    // Also verify launch_headless isolates unkeyed top-level siblings
    reset_component_hooks();
    let h1 = Rc::new(Cell::new(0));
    let h2 = Rc::new(Cell::new(0));
    let h1c = h1.clone();
    let h2c = h2.clone();
    let dummy_s1 = Rc::new(std::cell::RefCell::new(None));
    let dummy_s2 = Rc::new(std::cell::RefCell::new(None));
    let dummy_c1 = Rc::new(Cell::new(0));
    let dummy_c2 = Rc::new(Cell::new(0));
    let _rt2 = strata::launch_headless(
        move || {
            rsx! {
                <>
                    <UnkeyedSiblingCounter
                        initial={7}
                        state_out={h1c.clone()}
                        setter_out={dummy_s1.clone()}
                        cleanups={dummy_c1.clone()}
                    />
                    <UnkeyedSiblingCounter
                        initial={42}
                        state_out={h2c.clone()}
                        setter_out={dummy_s2.clone()}
                        cleanups={dummy_c2.clone()}
                    />
                </>
            }
        },
        200.0,
        100.0,
    );
    assert_eq!(h1.get(), 7);
    assert_eq!(h2.get(), 42);
    reset_component_hooks();
}

// ---------------------------------------------------------------------------
// 8. use_reducer preserves Rc::ptr_eq dispatch identity across renders
// ---------------------------------------------------------------------------

#[component]
fn ReducerIdentityTester(
    multiplier: i32,
    dispatches: Rc<std::cell::RefCell<Vec<Rc<dyn Fn(i32)>>>>,
    state_out: Rc<Cell<i32>>,
) -> View {
    let (val, dispatch) = use_reducer(
        move |state: &i32, delta: i32| *state + delta * multiplier,
        0,
    );
    dispatches.borrow_mut().push(dispatch);
    state_out.set(val.get());
    rsx! { <span>{format!("{}", val.get())}</span> }
}

#[test]
fn use_reducer_preserves_dispatch_identity_and_uses_latest_reducer_closure() {
    reset_component_hooks();

    let dispatches: Rc<std::cell::RefCell<Vec<Rc<dyn Fn(i32)>>>> =
        Rc::new(std::cell::RefCell::new(Vec::new()));
    let state_out = Rc::new(Cell::new(0));

    let _ = with_root_component_scope(|| {
        rsx! {
            <ReducerIdentityTester
                multiplier={1}
                dispatches={dispatches.clone()}
                state_out={state_out.clone()}
            />
        }
    });
    assert_eq!(state_out.get(), 0);

    let _ = with_root_component_scope(|| {
        rsx! {
            <ReducerIdentityTester
                multiplier={10}
                dispatches={dispatches.clone()}
                state_out={state_out.clone()}
            />
        }
    });

    {
        let ds = dispatches.borrow();
        assert_eq!(ds.len(), 2);
        assert!(
            Rc::ptr_eq(&ds[0], &ds[1]),
            "dispatch Rc identity must stay stable across re-renders"
        );
        // Dispatching through the original handle invokes the updated multiplier=10 reducer.
        ds[0](3);
    }

    let _ = with_root_component_scope(|| {
        rsx! {
            <ReducerIdentityTester
                multiplier={10}
                dispatches={dispatches.clone()}
                state_out={state_out.clone()}
            />
        }
    });
    assert_eq!(state_out.get(), 30);
    reset_component_hooks();
}

// ---------------------------------------------------------------------------
// 9. use_memo_with_deps memoization over plain props
// ---------------------------------------------------------------------------

#[component]
fn MemoDepsComponent(
    a: i32,
    b: i32,
    runs: Rc<Cell<usize>>,
    value_out: Rc<Cell<i32>>,
) -> View {
    let computed = use_memo_with_deps((a, b), |&(x, y)| {
        runs.set(runs.get() + 1);
        x * 100 + y
    });
    value_out.set(computed);
    rsx! { <span>{format!("{computed}")}</span> }
}

#[test]
fn use_memo_with_deps_caches_and_recomputes_on_dep_change() {
    reset_component_hooks();
    let runs = Rc::new(Cell::new(0));
    let out = Rc::new(Cell::new(0));

    let _ = with_root_component_scope(|| {
        rsx! { <MemoDepsComponent a={2} b={5} runs={runs.clone()} value_out={out.clone()} /> }
    });
    assert_eq!(runs.get(), 1);
    assert_eq!(out.get(), 205);

    // Same deps -> cached
    let _ = with_root_component_scope(|| {
        rsx! { <MemoDepsComponent a={2} b={5} runs={runs.clone()} value_out={out.clone()} /> }
    });
    assert_eq!(runs.get(), 1);
    assert_eq!(out.get(), 205);

    // Different deps -> recomputed once
    let _ = with_root_component_scope(|| {
        rsx! { <MemoDepsComponent a={2} b={9} runs={runs.clone()} value_out={out.clone()} /> }
    });
    assert_eq!(runs.get(), 2);
    assert_eq!(out.get(), 209);

    let _ = with_root_component_scope(|| {
        rsx! { <MemoDepsComponent a={2} b={9} runs={runs.clone()} value_out={out.clone()} /> }
    });
    assert_eq!(runs.get(), 2);
    assert_eq!(out.get(), 209);
    reset_component_hooks();
}

// ---------------------------------------------------------------------------
// 10. <ErrorBoundary><Child /></ErrorBoundary> lazy child catching + non-'static borrow
// ---------------------------------------------------------------------------

#[component]
fn PanickingBorrowChild(tag: String, should_panic: bool) -> View {
    if should_panic {
        panic!("lazy-child-boom:{tag}");
    }
    rsx! { <span>{tag}</span> }
}

#[test]
fn error_boundary_lazily_catches_direct_jsx_children_without_render_prop() {
    let local_suffix = String::from("pin-42");

    let view = rsx! {
        <ErrorBoundary
            fallback={Rc::new(|info: ErrorBoundaryInfo| {
                let msg = format!("caught:{}", info.message);
                rsx! { <span class="text-red-400">{msg}</span> }
            })}
        >
            <PanickingBorrowChild
                tag={format!("node-{}", local_suffix.as_str())}
                should_panic={true}
            />
        </ErrorBoundary>
    };

    let rt = AppRuntime::new(view, 300.0, 100.0, 1.0);
    assert!(
        rt.find_by_text("caught:lazy-child-boom:node-pin-42").is_some(),
        "expected ErrorBoundary direct children to be lowered lazily and catch panic"
    );
}

// ---------------------------------------------------------------------------
// 11. Canvas2D measure_text, round_rect, line_cap/line_join & GpuBatch dashed StrokeRect
// ---------------------------------------------------------------------------

#[test]
fn canvas2d_extended_apis_and_gpu_batch_dashed_stroke_rect_parity() {
    let view = rsx! {
        <div class="w-[320px] h-[160px]">
            <Canvas2D
                width={300.0}
                height={140.0}
                on_draw={Some(Rc::new(move |ctx: &mut Canvas2DContext| {
                    ctx.set_font("bold 12px monospace");
                    let m = ctx.measure_text("CLK_OUT");
                    assert!(m.width > 0.0 && m.height > 0.0);

                    ctx.set_line_cap(CanvasLineCap::Round);
                    ctx.set_line_join(CanvasLineJoin::Bevel);
                    ctx.set_fill_style(Color::rgba(30, 41, 59, 255));
                    ctx.begin_path();
                    ctx.round_rect(8.0, 8.0, m.width + 12.0, m.height + 6.0, 6.0);
                    ctx.fill();

                    ctx.save();
                    ctx.set_line_dash(&[8.0, 4.0]);
                    ctx.set_stroke_style(Color::rgb(56, 189, 248));
                    ctx.set_line_width(2.0);
                    ctx.stroke_rect(20.0, 40.0, 120.0, 60.0);
                    ctx.restore();
                }))}
            />
        </div>
    };

    let rt = AppRuntime::new(view, 320.0, 160.0, 1.0);
    let batch = GpuBatch::from_display_list(&rt.display_list, 320.0, 160.0);
    // Container div + dashed StrokeRect edges emit multiple dash quad instances in GpuBatch
    assert!(
        batch.instances.len() > 4,
        "GpuBatch should expand dashed CanvasCommand::StrokeRect into dash quads, got {}",
        batch.instances.len()
    );

    // GpuPathPipeline must honor SetLineCap(Round) & SetLineJoin(Bevel)
    let mut path_pipe = GpuPathPipeline::new(320.0, 160.0);
    let seg_count = path_pipe.compile_frame(&rt.display_list, None);
    assert!(seg_count > 0);
}

#[test]
fn gpu_pipelines_apply_set_line_cap_join_and_non_rectangular_clip_paths() {
    let view = rsx! {
        <div class="w-[240px] h-[240px]">
            <Canvas2D
                width={240.0}
                height={240.0}
                on_draw={Some(Rc::new(|ctx: &mut Canvas2DContext| {
                    // Open Round-capped / Bevel-joined CAD trace
                    ctx.set_stroke_style(Color::rgb(0, 255, 180));
                    ctx.set_line_width(8.0);
                    ctx.set_line_cap(CanvasLineCap::Round);
                    ctx.set_line_join(CanvasLineJoin::Bevel);
                    ctx.begin_path();
                    ctx.move_to(20.0, 40.0);
                    ctx.line_to(90.0, 40.0);
                    ctx.line_to(90.0, 110.0);
                    ctx.stroke();

                    // Circular CAD aperture clip
                    ctx.save();
                    ctx.begin_path();
                    ctx.arc(160.0, 160.0, 40.0, 0.0, std::f32::consts::TAU, false);
                    ctx.clip();
                    // Outside circle inside its AABB [120..200, 120..200] -> culled
                    ctx.set_fill_style(Color::rgb(255, 0, 0));
                    ctx.fill_rect(121.0, 121.0, 6.0, 6.0);
                    // Crosses circular boundary -> stencil masked
                    ctx.set_fill_style(Color::rgb(0, 140, 255));
                    ctx.fill_rect(120.0, 120.0, 80.0, 80.0);
                    ctx.restore();
                }))}
            />
        </div>
    };

    let rt = AppRuntime::new(view, 240.0, 240.0, 1.0);
    let batch = GpuBatch::from_display_list(&rt.display_list, 240.0, 240.0);
    assert!(batch.has_stencil_clips());
    let (r_red, g_red, b_red, a_red) = Color::rgb(255, 0, 0).to_f32();
    let red_c = [r_red, g_red, b_red, a_red];
    assert_eq!(
        batch.instances.iter().filter(|i| i.color == red_c).count(),
        0,
        "red rect strictly outside circular clip aperture must be culled"
    );
    let (br, bg, bb, ba) = Color::rgb(0, 140, 255).to_f32();
    let blue_idx = batch
        .instances
        .iter()
        .position(|i| i.color == [br, bg, bb, ba])
        .expect("crossing blue FillRect must be emitted");
    let mask = batch
        .clip_mask_for_instance(blue_idx)
        .expect("circle clip quad must bind stencil mask");
    assert!(mask.contains_point(160.0, 160.0));
    assert!(!mask.contains_point(122.0, 122.0));

    let mut path_pipe = GpuPathPipeline::new(240.0, 240.0);
    let n = path_pipe.compile_frame(&rt.display_list, None);
    assert!(n >= 3);
    assert!(path_pipe.batch.segments.iter().any(|s| {
        (s.flags & strata::render::gpu_path::FLAG_CAP_ROUND) != 0
            && (s.flags & strata::render::gpu_path::FLAG_JOIN_BEVEL) != 0
    }));
    assert!(path_pipe
        .batch
        .segments
        .iter()
        .any(|s| (s.flags & strata::render::gpu_path::FLAG_TRIANGLE) != 0));
}

// ---------------------------------------------------------------------------
// 12. Responsive Canvas2D post-layout on_paint inside split/flex layout
// ---------------------------------------------------------------------------

#[test]
fn test_responsive_canvas2d_on_paint_and_split_pane_resize() {
    use std::cell::RefCell;
    use strata_dom::ClassList;
    use strata_platform::PlatformEvent;

    let painted_bounds: Rc<RefCell<Vec<(f32, f32)>>> = Rc::new(RefCell::new(Vec::new()));
    let painted_bounds_cb = Rc::clone(&painted_bounds);
    let sidebar_width = Signal::new(200.0_f32);
    let accent_color = Signal::new(Color::rgb(56, 189, 248));

    let view = {
        let sidebar_width = sidebar_width.clone();
        let accent_color = accent_color.clone();
        let painted_bounds_cb = Rc::clone(&painted_bounds_cb);
        move || {
            let side_style = format!("width: {}px; height: 100%;", sidebar_width.get());
            let cls = ClassList::from(vec!["flex-1 h-full".to_string(), "min-w-0".to_string()]);
            let pb = Rc::clone(&painted_bounds_cb);
            let accent = accent_color.clone();

            rsx! {
                <div class="w-full h-full flex">
                    <div id="left-pane" style={side_style} />
                    <div class="flex-1 h-full flex">
                        <Canvas2D
                            id="cad-canvas"
                            class={cls}
                            responsive
                            on_paint={move |ctx: &mut Canvas2DContext, w: f32, h: f32| {
                                pb.borrow_mut().push((w, h));
                                ctx.set_fill_style(accent.get());
                                ctx.fill_rect(0.0, 0.0, w, h);
                                ctx.set_stroke_style(Color::rgb(255, 255, 255));
                                ctx.stroke_rect(2.0, 2.0, (w - 4.0).max(1.0), (h - 4.0).max(1.0));
                            }}
                        />
                    </div>
                </div>
            }
        }
    };

    let mut rt = AppRuntime::from_root(&view, 800.0, 400.0, 1.0);

    // 1. Flex layout resolves Canvas2D to full 600x400 instead of 400x300 fixed fallback
    let rect = rt
        .query_element_rect("cad-canvas")
        .expect("cad-canvas rect should exist");
    assert!(
        (rect.width - 600.0).abs() < 0.5 && (rect.height - 400.0).abs() < 0.5,
        "Expected 600x400 responsive canvas bounds, got {}x{}",
        rect.width,
        rect.height
    );

    let find_canvas_cmd = |runtime: &AppRuntime| -> Option<(f32, f32, Color)> {
        runtime.display_list.commands().iter().find_map(|c| match c {
            DrawCommand::DrawCanvas2D { bounds, commands } => {
                let fill = commands.iter().find_map(|cc| match cc {
                    CanvasCommand::FillRect { color, .. } => Some(*color),
                    _ => None,
                })?;
                Some((bounds.width, bounds.height, fill))
            }
            _ => None,
        })
    };

    let (w0, h0, c0) = find_canvas_cmd(&rt).expect("Expected initial DrawCanvas2D command");
    assert!((w0 - 598.0).abs() < 0.5 && (h0 - 398.0).abs() < 0.5);
    assert_eq!(c0, Color::rgb(56, 189, 248));
    assert_eq!(painted_bounds.borrow().last().copied(), Some((598.0, 398.0)));

    // 2. Omitted fixed dimensions + window resize updates post-layout paint bounds
    assert!(rt.step_event(&PlatformEvent::WindowResized {
        width: 520,
        height: 300,
    }));
    let rect_resized = rt
        .query_element_rect("cad-canvas")
        .expect("cad-canvas rect after resize");
    assert!(
        (rect_resized.width - 320.0).abs() < 0.5 && (rect_resized.height - 300.0).abs() < 0.5,
        "Expected 320x300 after WindowResized, got {}x{}",
        rect_resized.width,
        rect_resized.height
    );
    let (w1, h1, _) = find_canvas_cmd(&rt).expect("Expected DrawCanvas2D after resize");
    assert!((w1 - 318.0).abs() < 0.5 && (h1 - 298.0).abs() < 0.5);

    // 3. Changing reactive sidebar width & accent color dynamically resizes and repaints
    sidebar_width.set(120.0);
    accent_color.set(Color::rgb(250, 204, 21));
    rt.update_view(strata_dom::with_root_component_scope(&view));
    let (w2, h2, c2) = find_canvas_cmd(&rt).expect("Expected DrawCanvas2D after split resize");
    assert!((w2 - 398.0).abs() < 0.5 && (h2 - 298.0).abs() < 0.5);
    assert_eq!(c2, Color::rgb(250, 204, 21));
}

#[test]
fn test_omitted_width_height_canvas2d_respects_flex_without_400x300_inline_override() {
    let seen = Rc::new(Cell::new((0.0_f32, 0.0_f32)));
    let seen_cb = Rc::clone(&seen);

    let view = rsx! {
        <div class="w-[500px] h-[220px] flex">
            <Canvas2D
                id="flex-canvas"
                class="flex-1 h-full border-0"
                on_paint={move |ctx: &mut Canvas2DContext, w: f32, h: f32| {
                    seen_cb.set((w, h));
                    ctx.fill_rect(0.0, 0.0, w, h);
                }}
            />
        </div>
    };

    let rt = AppRuntime::new(view, 500.0, 220.0, 1.0);
    let rect = rt.query_element_rect("flex-canvas").unwrap();
    assert!((rect.width - 500.0).abs() < 0.5);
    assert!((rect.height - 220.0).abs() < 0.5);
    assert_eq!(seen.get(), (500.0, 220.0));
}

#[component]
fn E2ePanelShell(
    title: String,
    onclick: Option<Rc<dyn Fn(strata_dom::Event)>>,
    children: View,
) -> View {
    let shortcut_flag = use_signal(false);
    let sf = shortcut_flag.clone();
    use_shortcut("Ctrl+Shift+M", move || sf.set(!sf.get()));

    let active_cls = if shortcut_flag.get() { "active" } else { "idle" };
    if let Some(cb) = onclick {
        rsx! {
            <div class="flex flex-col">
                <button id="panel-btn" class={active_cls} onclick={cb}>{title}</button>
                <div id="panel-body">{children}</div>
            </div>
        }
    } else {
        rsx! {
            <div class="flex flex-col">
                <button id="panel-btn" class={active_cls}>{title}</button>
                <div id="panel-body">{children}</div>
            </div>
        }
    }
}

#[test]
fn test_component_children_view_typed_onclick_and_shortcut_unmount_in_app_runtime() {
    use strata_dom::hooks::dispatch_global_keydown;
    use strata_dom::reactive::reset_component_hooks;

    reset_component_hooks();
    let mounted = Signal::new(true);
    let clicked = Rc::new(Cell::new(0.0_f64));
    let clicked_cb = Rc::clone(&clicked);
    let mounted_view = mounted.clone();

    let build_root = move || {
        let cb = Rc::clone(&clicked_cb);
        if mounted_view.get() {
            rsx! {
                <div class="w-[300px] h-[160px]">
                    <E2ePanelShell
                        title="Run"
                        onclick={move |ev: strata_dom::Event| cb.set(ev.client_x)}
                    >
                        <span>{"Child A"}</span>
                        <span>{"Child B"}</span>
                    </E2ePanelShell>
                </div>
            }
        } else {
            rsx! { <div class="w-[300px] h-[160px]"><span>{"Unmounted"}</span></div> }
        }
    };

    let mut rt = AppRuntime::from_root(&build_root, 300.0, 160.0, 1.0);

    assert!(rt.find_by_text("Run").is_some());
    assert!(rt.find_by_text("Child A").is_some());
    assert!(rt.find_by_text("Child B").is_some());

    // Typed Event onclick fires with real pointer coordinates
    assert!(rt.click_text("Run"));
    assert!(clicked.get() > 0.0);

    // Shortcut stays registered across re-render & unregisters cleanly when E2ePanelShell unmounts
    rt.update_view(strata_dom::with_root_component_scope(&build_root));
    assert!(dispatch_global_keydown("M", true, true, false, false));
    mounted.set(false);
    rt.update_view(strata_dom::with_root_component_scope(&build_root));
    assert!(rt.find_by_text("Unmounted").is_some());
    assert!(!dispatch_global_keydown("M", true, true, false, false));
    reset_component_hooks();
}

// ---------------------------------------------------------------------------
// 13. Canvas2D title, contextmenu, dblclick, mouseenter, mouseleave
// ---------------------------------------------------------------------------

#[test]
fn test_canvas2d_contextmenu_dblclick_mouseenter_mouseleave_and_title() {
    let enters = Rc::new(Cell::new(0usize));
    let leaves = Rc::new(Cell::new(0usize));
    let dbls = Rc::new(Cell::new(0usize));
    let menus = Rc::new(Cell::new(0usize));

    let enters_cb = Rc::clone(&enters);
    let leaves_cb = Rc::clone(&leaves);
    let dbls_cb = Rc::clone(&dbls);
    let menus_cb = Rc::clone(&menus);

    let view = rsx! {
        <div class="w-[400px] h-[240px] flex">
            <Canvas2D
                id="cad2d-events"
                title="2D Schematic"
                class="w-[240px] h-[160px]"
                onmouseenter={move |_ev: strata_dom::Event| enters_cb.set(enters_cb.get() + 1)}
                onmouseleave={move |_ev: strata_dom::Event| leaves_cb.set(leaves_cb.get() + 1)}
                ondblclick={move |_ev: strata_dom::Event| dbls_cb.set(dbls_cb.get() + 1)}
                oncontextmenu={move |_ev: strata_dom::Event| menus_cb.set(menus_cb.get() + 1)}
            />
        </div>
    };

    let mut rt = AppRuntime::new(view, 400.0, 240.0, 1.0);
    rt.set_hover_tooltip_delay(Duration::ZERO);

    rt.update_hover(80.0, 60.0);
    assert_eq!(enters.get(), 1);
    assert!(rt.poll_hover_tooltip());
    assert!(rt.display_list.commands().iter().any(|c| matches!(
        c,
        DrawCommand::DrawText { text, .. } if text == "2D Schematic"
    )));

    assert!(rt.dispatch_dblclick(80.0, 60.0));
    assert_eq!(dbls.get(), 1);

    assert!(rt.dispatch_context_menu(80.0, 60.0));
    assert_eq!(menus.get(), 1);

    rt.update_hover(350.0, 200.0);
    assert_eq!(leaves.get(), 1);
}

// ---------------------------------------------------------------------------
// 14. W3C bubbling and stop_propagation() across Click, DblClick, ContextMenu, Wheel
// ---------------------------------------------------------------------------

#[test]
fn test_w3c_bubbling_and_stop_propagation_across_click_dblclick_contextmenu_and_wheel() {
    use std::cell::RefCell;

    let trace: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let outer_scroll_wheel_hit = Rc::new(Cell::new(0usize));

    let t_outer_w = Rc::clone(&trace);
    let outer_scroll_cb = Rc::clone(&outer_scroll_wheel_hit);

    let t_c_clk = Rc::clone(&trace);
    let t_c_aux = Rc::clone(&trace);
    let t_c_dbl = Rc::clone(&trace);
    let t_c_ctx = Rc::clone(&trace);
    let t_c_whl = Rc::clone(&trace);

    let t_b_clk = Rc::clone(&trace);
    let t_b_aux = Rc::clone(&trace);
    let t_b_dbl = Rc::clone(&trace);
    let t_b_ctx = Rc::clone(&trace);
    let t_b_whl = Rc::clone(&trace);

    let t_s_clk = Rc::clone(&trace);
    let t_s_aux = Rc::clone(&trace);
    let t_s_dbl = Rc::clone(&trace);
    let t_s_ctx = Rc::clone(&trace);
    let t_s_whl = Rc::clone(&trace);

    let view = rsx! {
        <div
            class="w-[400px] h-[300px] overflow-y-auto"
            onwheel={move |_ev: strata_dom::Event| {
                outer_scroll_cb.set(outer_scroll_cb.get() + 1);
                t_outer_w.borrow_mut().push("outer-wheel");
            }}
        >
            <div class="w-[400px] h-[600px]">
                <Canvas2D
                    id="cad-parent"
                    class="w-[360px] h-[240px] flex flex-col gap-2 p-2"
                    onclick={move |_ev: strata_dom::Event| t_c_clk.borrow_mut().push("canvas-click")}
                    ondblclick={move |_ev: strata_dom::Event| t_c_dbl.borrow_mut().push("canvas-dbl")}
                    oncontextmenu={move |_ev: strata_dom::Event| t_c_ctx.borrow_mut().push("canvas-ctx")}
                    onwheel={move |_ev: strata_dom::Event| t_c_whl.borrow_mut().push("canvas-wheel")}
                >
                    <div
                        class="w-[300px] h-[60px]"
                        onauxclick={move |_ev: strata_dom::Event| t_c_aux.borrow_mut().push("wrap-aux")}
                    >
                        <button
                            id="bubbling-child"
                            class="w-[200px] h-[50px]"
                            onclick={move |_ev: strata_dom::Event| t_b_clk.borrow_mut().push("child-click")}
                            onauxclick={move |_ev: strata_dom::Event| t_b_aux.borrow_mut().push("child-aux")}
                            ondblclick={move |_ev: strata_dom::Event| t_b_dbl.borrow_mut().push("child-dbl")}
                            oncontextmenu={move |_ev: strata_dom::Event| t_b_ctx.borrow_mut().push("child-ctx")}
                            onwheel={move |_ev: strata_dom::Event| t_b_whl.borrow_mut().push("child-wheel")}
                        >
                            "Bubble Child"
                        </button>
                    </div>
                    <button
                        id="stopping-child"
                        class="w-[200px] h-[50px]"
                        onclick={move |ev: strata_dom::Event| {
                            ev.stop_propagation();
                            t_s_clk.borrow_mut().push("stop-click");
                        }}
                        onauxclick={move |ev: strata_dom::Event| {
                            ev.stop_propagation();
                            t_s_aux.borrow_mut().push("stop-aux");
                        }}
                        ondblclick={move |ev: strata_dom::Event| {
                            ev.stop_propagation();
                            t_s_dbl.borrow_mut().push("stop-dbl");
                        }}
                        oncontextmenu={move |ev: strata_dom::Event| {
                            ev.stop_propagation();
                            t_s_ctx.borrow_mut().push("stop-ctx");
                        }}
                        onwheel={move |ev: strata_dom::Event| {
                            ev.stop_propagation();
                            ev.prevent_default();
                            t_s_whl.borrow_mut().push("stop-wheel");
                        }}
                    >
                        "Stop Child"
                    </button>
                </Canvas2D>
            </div>
        </div>
    };

    let mut rt = AppRuntime::new(view, 400.0, 300.0, 1.0);
    let bubble_box = rt.query_element_rect("bubbling-child").unwrap();
    let stop_box = rt.query_element_rect("stopping-child").unwrap();
    let bx = bubble_box.x + 20.0;
    let by = bubble_box.y + 20.0;
    let sx = stop_box.x + 20.0;
    let sy = stop_box.y + 20.0;

    // 1. Without stop_propagation(), Click / AuxClick / DblClick / ContextMenu / Wheel bubble cleanly
    assert!(rt.dispatch_mouse_click(bx, by, 0, 1));
    assert!(rt.dispatch_mouse_click(bx, by, 1, 1));
    assert!(rt.dispatch_dblclick(bx, by));
    assert!(rt.dispatch_context_menu(bx, by));
    assert!(rt.dispatch_scroll(bx, by, 0.0, 24.0));
    assert_eq!(
        trace.borrow().as_slice(),
        &[
            "child-click",
            "canvas-click",
            "child-aux",
            "wrap-aux",
            "child-dbl",
            "canvas-dbl",
            "child-ctx",
            "canvas-ctx",
            "child-wheel",
            "canvas-wheel",
            "outer-wheel",
        ]
    );
    assert_eq!(outer_scroll_wheel_hit.get(), 1);

    // 2. With stop_propagation() (and prevent_default() on Wheel), parent listeners stay quiet
    trace.borrow_mut().clear();
    assert!(rt.dispatch_mouse_click(sx, sy, 0, 1));
    assert!(rt.dispatch_mouse_click(sx, sy, 1, 1));
    assert!(rt.dispatch_dblclick(sx, sy));
    assert!(rt.dispatch_context_menu(sx, sy));
    assert!(rt.dispatch_scroll(sx, sy, 0.0, 24.0));
    assert_eq!(
        trace.borrow().as_slice(),
        &[
            "stop-click",
            "stop-aux",
            "stop-dbl",
            "stop-ctx",
            "stop-wheel",
        ]
    );
    assert_eq!(outer_scroll_wheel_hit.get(), 1);
}

#[test]
fn test_w3c_bubbling_and_stop_propagation_for_pointer_and_mouse_down_move_up() {
    use std::cell::RefCell;

    let trace: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));

    let t = |tag: &'static str, trace: &Rc<RefCell<Vec<&'static str>>>, stop: bool, cap: bool| {
        let tr = Rc::clone(trace);
        move |ev: strata_dom::Event| {
            if stop {
                ev.stop_propagation();
            }
            if cap {
                ev.set_pointer_capture(ev.pointer_id);
            }
            tr.borrow_mut().push(tag);
        }
    };

    let view = rsx! {
        <div class="w-[400px] h-[300px]">
            <Canvas2D
                id="cad-pointer"
                class="w-[360px] h-[260px] flex flex-col gap-2 p-2"
                onpointerdown={t("canvas-pd", &trace, false, false)}
                onmousedown={t("canvas-md", &trace, false, false)}
                onpointermove={t("canvas-pm", &trace, false, false)}
                onmousemove={t("canvas-mm", &trace, false, false)}
                onpointerup={t("canvas-pu", &trace, false, false)}
                onmouseup={t("canvas-mu", &trace, false, false)}
            >
                <button
                    id="ptr-bubble"
                    class="w-[180px] h-[40px]"
                    onpointerdown={t("child-pd", &trace, false, false)}
                    onmousedown={t("child-md", &trace, false, false)}
                    onpointermove={t("child-pm", &trace, false, false)}
                    onmousemove={t("child-mm", &trace, false, false)}
                    onpointerup={t("child-pu", &trace, false, false)}
                    onmouseup={t("child-mu", &trace, false, false)}
                >
                    "Ptr Bubble"
                </button>
                <button
                    id="ptr-stop"
                    class="w-[180px] h-[40px]"
                    onpointerdown={t("stop-pd", &trace, true, false)}
                    onmousedown={t("stop-md", &trace, true, false)}
                    onpointermove={t("stop-pm", &trace, true, false)}
                    onmousemove={t("stop-mm", &trace, true, false)}
                    onpointerup={t("stop-pu", &trace, true, false)}
                    onmouseup={t("stop-mu", &trace, true, false)}
                >
                    "Ptr Stop"
                </button>
                <button
                    id="ptr-capture"
                    class="w-[180px] h-[40px]"
                    onpointerdown={t("cap-pd", &trace, true, true)}
                    onpointermove={t("cap-pm", &trace, true, false)}
                    onpointerup={t("cap-pu", &trace, true, false)}
                >
                    "Ptr Capture"
                </button>
            </Canvas2D>
        </div>
    };

    let mut rt = AppRuntime::new(view, 400.0, 300.0, 1.0);
    let b_box = rt.query_element_rect("ptr-bubble").unwrap();
    let s_box = rt.query_element_rect("ptr-stop").unwrap();
    let c_box = rt.query_element_rect("ptr-capture").unwrap();

    // 1. Non-captured PointerMove / MouseMove bubbles from child to parent Canvas2D
    assert!(rt.dispatch_pointer_move(b_box.x + 10.0, b_box.y + 10.0));
    assert_eq!(
        trace.borrow().as_slice(),
        &["child-pm", "canvas-pm", "child-mm", "canvas-mm"]
    );
    trace.borrow_mut().clear();

    // 2. PointerDown -> PointerMove -> PointerUp bubbles cleanly without duplicate implicit-capture firing
    assert!(rt.dispatch_pointer_down(b_box.x + 10.0, b_box.y + 10.0));
    assert_eq!(
        trace.borrow().as_slice(),
        &["child-pd", "canvas-pd", "child-md", "canvas-md"]
    );
    trace.borrow_mut().clear();

    assert!(rt.dispatch_pointer_move(b_box.x + 20.0, b_box.y + 15.0));
    assert_eq!(
        trace.borrow().as_slice(),
        &["child-pm", "canvas-pm", "child-mm", "canvas-mm"]
    );
    trace.borrow_mut().clear();

    assert!(rt.dispatch_pointer_up(b_box.x + 20.0, b_box.y + 15.0));
    assert_eq!(
        trace.borrow().as_slice(),
        &["child-pu", "canvas-pu", "child-mu", "canvas-mu"]
    );
    trace.borrow_mut().clear();

    // 3. stop_propagation() on nested child halts bubbling on down/move/up
    assert!(rt.dispatch_pointer_down(s_box.x + 10.0, s_box.y + 10.0));
    assert!(rt.dispatch_pointer_move(s_box.x + 20.0, s_box.y + 15.0));
    assert!(rt.dispatch_pointer_up(s_box.x + 20.0, s_box.y + 15.0));
    assert_eq!(
        trace.borrow().as_slice(),
        &["stop-pd", "stop-md", "stop-pm", "stop-mm", "stop-pu", "stop-mu"]
    );
    trace.borrow_mut().clear();

    // 4. Explicit set_pointer_capture on child preserves capture precedence even over #ptr-stop bounds:
    // #ptr-stop receives nothing; #ptr-capture receives Pointer* and falls through to #cad-pointer for Mouse*.
    assert!(rt.dispatch_pointer_down(c_box.x + 10.0, c_box.y + 10.0));
    assert!(rt.dispatch_pointer_move(s_box.x + 25.0, s_box.y + 20.0));
    assert!(rt.dispatch_pointer_up(s_box.x + 25.0, s_box.y + 20.0));
    assert_eq!(
        trace.borrow().as_slice(),
        &["cap-pd", "canvas-md", "cap-pm", "canvas-mm", "cap-pu", "canvas-mu"]
    );
}



