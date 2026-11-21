//! Integration tests for native periodic `on_frame` callbacks, `set_interval` /
//! `set_timeout` scheduling, and zero-wakeup idle behavior.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use strata::prelude::*;

fn counter_view(count: usize) -> View {
    let label = format!("tick:{count}");
    rsx! {
        <div class="w-40 h-20 bg-neutral-900 text-white">
            <span>{label}</span>
        </div>
    }
}

#[test]
fn native_event_loop_runs_10ms_interval_for_200ms_and_reinvokes_root() {
    let ticks = Arc::new(AtomicUsize::new(0));
    let ticks_cb = ticks.clone();
    let timer_id = strata_js::web::set_interval(
        move || {
            ticks_cb.fetch_add(1, Ordering::SeqCst);
        },
        10,
    );

    let root_calls = Rc::new(Cell::new(0usize));
    let root_calls_cb = root_calls.clone();
    let ticks_root = ticks.clone();
    let make_view = move || {
        root_calls_cb.set(root_calls_cb.get() + 1);
        counter_view(ticks_root.load(Ordering::SeqCst))
    };

    let mut runtime = AppRuntime::new(make_view(), 320.0, 200.0, 1.0);
    let mut compiles = 0usize;

    let start = Instant::now();
    let target = Duration::from_millis(205);
    while start.elapsed() < target {
        let fired = strata_js::web::tick_timers();
        if fired > 0 {
            let new_view = make_view();
            runtime.update_view(new_view);
            runtime.mark_paint_dirty();
            if runtime.compile_display_list_if_dirty() {
                compiles += 1;
            }
        }
        let sleep = runtime
            .compute_idle_sleep(fired > 0)
            .min(target.saturating_sub(start.elapsed()));
        if !sleep.is_zero() {
            std::thread::sleep(sleep);
        }
    }

    strata_js::web::clear_interval(timer_id);

    let total_ticks = ticks.load(Ordering::SeqCst);
    let total_roots = root_calls.get();

    assert!(
        (14..=26).contains(&total_ticks),
        "expected ~20 10ms interval callbacks in 200ms, got {total_ticks}"
    );
    assert_eq!(
        total_roots,
        total_ticks + 1,
        "root should be re-invoked once initially plus once per timer tick batch"
    );
    assert!(
        compiles >= total_ticks,
        "each dirty tick must compile a frame (compiles={compiles}, ticks={total_ticks})"
    );
    let rendered = format!("{:?}", runtime.root_blueprint);
    assert!(
        rendered.contains(&format!("tick:{total_ticks}")),
        "rendered DOM must reflect final timer state tick:{total_ticks}"
    );
}

#[test]
fn idle_loop_without_timers_or_input_compiles_zero_frames() {
    let mut runtime = AppRuntime::new(counter_view(0), 320.0, 200.0, 1.0);
    // Clear initial dirty state with one compile.
    let _ = runtime.compile_display_list_if_dirty();
    let baseline_skipped = runtime.compiles_skipped();

    for _ in 0..10 {
        assert_eq!(strata_js::web::tick_timers(), 0);
        let sleep = runtime.compute_idle_sleep(false);
        assert_eq!(
            sleep,
            Duration::from_millis(200),
            "idle runtime without timers or focus must use maximum idle sleep"
        );
        assert!(
            !runtime.compile_display_list_if_dirty(),
            "idle frame must not recompile display list"
        );
    }

    assert_eq!(runtime.compiles_skipped(), baseline_skipped + 10);
    assert_eq!(strata_js::web::time_until_next_timer(), None);
}

#[test]
fn frame_outcome_unchanged_skips_rebuild_and_compile() {
    let mut runtime = AppRuntime::new(counter_view(0), 320.0, 200.0, 1.0);
    let _ = runtime.compile_display_list_if_dirty();
    let baseline_skipped = runtime.compiles_skipped();

    let mut root_calls = 0usize;
    let on_frame =
        |_info: AppFrameInfo, _rt: &mut AppRuntime| -> FrameOutcome { FrameOutcome::Unchanged };

    for i in 1..=5 {
        let info = AppFrameInfo {
            frame_index: i,
            viewport_width: 320.0,
            viewport_height: 200.0,
            dpr: 1.0,
            command_count: runtime.display_list.commands.len(),
            dt: Duration::from_millis(16),
        };
        if on_frame(info, &mut runtime).is_dirty() {
            root_calls += 1;
            runtime.update_view(counter_view(root_calls));
            runtime.mark_paint_dirty();
        }
        assert!(
            !runtime.compile_display_list_if_dirty(),
            "FrameOutcome::Unchanged must not trigger compile"
        );
    }

    assert_eq!(root_calls, 0);
    assert_eq!(runtime.compiles_skipped(), baseline_skipped + 5);

    // Now verify Dirty triggers rebuild + compile.
    let on_frame_dirty = |info: AppFrameInfo, _rt: &mut AppRuntime| -> FrameOutcome {
        assert!(info.dt > Duration::ZERO);
        FrameOutcome::Dirty
    };
    let info = AppFrameInfo {
        frame_index: 6,
        viewport_width: 320.0,
        viewport_height: 200.0,
        dpr: 1.0,
        command_count: runtime.display_list.commands.len(),
        dt: Duration::from_millis(16),
    };
    if on_frame_dirty(info, &mut runtime).is_dirty() {
        root_calls += 1;
        runtime.update_view(counter_view(root_calls));
        runtime.mark_paint_dirty();
    }
    assert!(
        runtime.compile_display_list_if_dirty(),
        "FrameOutcome::Dirty must trigger compile"
    );
    assert_eq!(root_calls, 1);
}

#[cfg(target_os = "linux")]
#[test]
fn gtk_launch_with_frame_runs_10ms_interval_and_reinvokes_root() {
    if std::env::var("DISPLAY").is_err() && std::env::var("WAYLAND_DISPLAY").is_err() {
        return;
    }

    let interval_ticks = Arc::new(AtomicUsize::new(0));
    let interval_cb = interval_ticks.clone();
    let timer_id = strata_js::web::set_interval(
        move || {
            interval_cb.fetch_add(1, Ordering::SeqCst);
        },
        10,
    );

    let frame_callbacks = Rc::new(Cell::new(0usize));
    let root_invocations = Rc::new(Cell::new(0usize));
    let window_start = Rc::new(Cell::new(None::<Instant>));

    let root_count = root_invocations.clone();
    let ticks_for_root = interval_ticks.clone();
    let config = strata_platform::WindowConfig {
        title: "native_timers_gtk_test".to_string(),
        width: 240,
        height: 160,
        ..Default::default()
    };

    let frame_cb_count = frame_callbacks.clone();
    let res = launch_gtk_with_frame(
        move || {
            root_count.set(root_count.get() + 1);
            counter_view(ticks_for_root.load(Ordering::SeqCst))
        },
        config,
        |_ev, _rt| false,
        Some(Duration::from_millis(10)),
        move |info, _rt| {
            let started = match window_start.get() {
                Some(t) => t,
                None => {
                    let t = Instant::now();
                    window_start.set(Some(t));
                    t
                }
            };
            frame_cb_count.set(frame_cb_count.get() + 1);
            assert!(info.dt >= Duration::from_millis(1));
            if started.elapsed() >= Duration::from_millis(300) {
                strata_platform::gtk_close();
            }
            FrameOutcome::Unchanged
        },
    );
    strata_js::web::clear_interval(timer_id);
    assert!(res.is_ok(), "GTK launch failed: {:?}", res);

    let total_ticks = interval_ticks.load(Ordering::SeqCst);
    let total_frames = frame_callbacks.get();
    let total_roots = root_invocations.get();
    assert!(
        (8..=35).contains(&total_ticks),
        "expected ~20-30 10ms set_interval ticks in live GTK loop over 300ms, got {total_ticks}"
    );
    assert!(
        (8..=35).contains(&total_frames),
        "expected ~20-30 10ms on_frame callbacks in live GTK loop over 300ms, got {total_frames}"
    );
    assert!(
        total_roots >= total_ticks,
        "root must be re-invoked for timer state changes (roots={total_roots}, ticks={total_ticks})"
    );

    // Phase 2 (same thread, since gtk-rs requires a single thread for gtk::init):
    // Verify an idle GTK window with no periodic on_frame or interval wakes only once
    // for the 400ms close timeout and never re-invokes root.
    let idle_roots = Rc::new(Cell::new(0usize));
    let idle_ticks = Rc::new(Cell::new(0usize));
    let close_timer = strata_js::web::set_timeout(
        || {
            strata_platform::gtk_close();
        },
        400,
    );

    let idle_root_count = idle_roots.clone();
    let idle_tick_count = idle_ticks.clone();
    let idle_config = strata_platform::WindowConfig {
        title: "idle_wakeup_gtk_test".to_string(),
        width: 240,
        height: 160,
        ..Default::default()
    };

    let idle_res = launch_gtk_with_events(
        move || {
            idle_root_count.set(idle_root_count.get() + 1);
            counter_view(0)
        },
        idle_config,
        move |ev, _rt| {
            if matches!(ev, strata_platform::PlatformEvent::AnimationTick) {
                idle_tick_count.set(idle_tick_count.get() + 1);
            }
            false
        },
    );
    strata_js::web::clear_timeout(close_timer);
    assert!(idle_res.is_ok(), "GTK idle launch failed: {:?}", idle_res);

    assert_eq!(
        idle_roots.get(),
        1,
        "idle GTK window must only invoke root once initially"
    );
    assert!(
        idle_ticks.get() <= 2,
        "idle GTK window should wake only for the scheduled close timeout, got {} ticks",
        idle_ticks.get()
    );

    // Phase 3: Programmatic Signal::set inside on_frame returning FrameOutcome::Unchanged
    // and standalone mark_dirty() must wake immediately and rebuild root via AnimationTick.
    let _ = strata_dom::reactive::take_dirty_signals();
    let frame_sig = Signal::new(0usize);
    let sig_root_calls = Rc::new(Cell::new(0usize));
    let step = Rc::new(Cell::new(0usize));

    let phase3_root = {
        let sig = frame_sig.clone();
        let calls = sig_root_calls.clone();
        move || {
            calls.set(calls.get() + 1);
            counter_view(sig.get())
        }
    };
    let phase3_config = strata_platform::WindowConfig {
        title: "signal_wakeup_gtk_test".to_string(),
        width: 240,
        height: 160,
        ..Default::default()
    };
    let phase3_res = launch_gtk_with_frame(
        phase3_root,
        phase3_config,
        |_ev, _rt| false,
        Some(Duration::from_millis(20)),
        move |_info, _rt| {
            let n = step.get();
            step.set(n + 1);
            match n {
                0 => frame_sig.set(7),
                1 => strata_dom::reactive::mark_dirty(),
                _ => strata_platform::gtk_close(),
            }
            FrameOutcome::Unchanged
        },
    );
    assert!(phase3_res.is_ok(), "GTK signal wakeup test failed: {:?}", phase3_res);
    assert!(
        sig_root_calls.get() >= 3,
        "initial build + Signal::set + mark_dirty with FrameOutcome::Unchanged must trigger rebuilds, got {} root calls",
        sig_root_calls.get()
    );
}

#[test]
fn native_animation_tick_rebuilds_on_programmatic_signal_and_mark_dirty() {
    let _ = strata_dom::reactive::take_dirty_signals();
    let sig = Signal::new(1i32);
    let mut rt = AppRuntime::from_root(
        || counter_view(sig.get() as usize),
        200.0,
        100.0,
        1.0,
    );
    let _ = strata_dom::reactive::take_dirty_signals();

    let mut compositor = strata_render::SwapchainCompositor::new(200, 100, 1.0);
    let mut presenter = strata_render::SoftwareSurfacePresenter::new(
        200,
        100,
        strata_render::SwapchainFormat::Bgra8Unorm,
    );
    let mut eh = |_: &strata_platform::PlatformEvent, _: &mut AppRuntime| false;

    // Idle AnimationTick must NOT request rebuild
    let (reb0, _, _) = strata::runtime::platforms::dispatch_native_platform_event(
        &strata_platform::PlatformEvent::AnimationTick,
        &mut rt,
        &mut compositor,
        &mut presenter,
        &mut eh,
    );
    assert!(!reb0);

    // Mutate signal outside any user input -> AnimationTick must request rebuild
    sig.set(5);
    let (reb1, red1, _) = strata::runtime::platforms::dispatch_native_platform_event(
        &strata_platform::PlatformEvent::AnimationTick,
        &mut rt,
        &mut compositor,
        &mut presenter,
        &mut eh,
    );
    assert!(reb1 && red1);

    let _ = strata_dom::reactive::take_dirty_signals();
    strata_dom::reactive::mark_dirty();
    let (reb2, red2, _) = strata::runtime::platforms::dispatch_native_platform_event(
        &strata_platform::PlatformEvent::AnimationTick,
        &mut rt,
        &mut compositor,
        &mut presenter,
        &mut eh,
    );
    assert!(reb2 && red2);
    let _ = strata_dom::reactive::take_dirty_signals();
}
