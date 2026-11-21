use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;
use strata::prelude::*;
use strata::render::canvas2d::{CanvasCommand, CanvasPathVerb};
use strata::render::compositor::DrawCommand;
use strata::render::{FrameBuffer, GpuPathPipeline, GpuSlugTextPipeline};
use strata::style::types::Color;
use strata_ui::canvas::{Canvas2D, Canvas2DContext, CanvasTextAlign, CanvasTextBaseline};

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
