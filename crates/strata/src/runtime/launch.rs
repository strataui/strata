//! Window launching, headless execution, and Linux GTK platform lifecycle hooks.

use super::*;

/// Standard public entry point launching an interactive Strata desktop window.
/// Automatically detects operating system display server, opens native window,
/// and runs the event pump until the window is closed.
pub fn launch<F>(root: F)
where
    F: FnMut() -> View + 'static,
{
    let config = WindowConfig::default();
    if let Err(e) = launch_with_config(root, config) {
        eprintln!("[Strata] Fatal runtime launch error: {}", e);
    }
}

/// Standard public entry point launching with custom window configuration.
pub fn launch_with_config<F>(root: F, config: WindowConfig) -> Result<(), String>
where
    F: FnMut() -> View + 'static,
{
    launch_with_events(root, config, |_, _| false)
}

/// Launches a Strata window with custom window configuration and low-level platform event hook.
pub fn launch_with_events<F, E>(
    root: F,
    config: WindowConfig,
    event_handler: E,
) -> Result<(), String>
where
    F: FnMut() -> View + 'static,
    E: FnMut(&strata_platform::PlatformEvent, &mut AppRuntime) -> bool + 'static,
{
    launch_auto(root, config, event_handler)
}

/// Auto-detects the host platform display server and launches an interactive desktop window.
pub fn launch_auto<F, E>(root_fn: F, config: WindowConfig, event_handler: E) -> Result<(), String>
where
    F: FnMut() -> View + 'static,
    E: FnMut(&strata_platform::PlatformEvent, &mut AppRuntime) -> bool + 'static,
{
    // 1. Explicit or detected headless execution
    if std::env::var("STRATA_HEADLESS").is_ok() {
        return launch_headless_pump(root_fn, config, event_handler);
    }

    let root_rc = std::rc::Rc::new(std::cell::RefCell::new(root_fn));
    let event_rc = std::rc::Rc::new(std::cell::RefCell::new(event_handler));

    #[cfg(target_os = "linux")]
    {
        let backend = std::env::var("STRATA_BACKEND")
            .unwrap_or_default()
            .to_lowercase();
        if backend == "x11" {
            let r = root_rc.clone();
            let ev = event_rc.clone();
            return launch_x11_with_events(
                move || (r.borrow_mut())(),
                config,
                move |event, rt| (ev.borrow_mut())(event, rt),
            );
        } else if backend == "wayland" {
            let r = root_rc.clone();
            let ev = event_rc.clone();
            return launch_wayland_with_events(
                move || (r.borrow_mut())(),
                config,
                move |event, rt| (ev.borrow_mut())(event, rt),
            );
        } else if backend == "gtk" {
            let r = root_rc.clone();
            let ev = event_rc.clone();
            return launch_gtk_with_events(
                move || (r.borrow_mut())(),
                config,
                move |event, rt| (ev.borrow_mut())(event, rt),
            );
        }

        // Automatic Linux detection:
        // If DISPLAY or WAYLAND_DISPLAY is present, attempt GTK first for Cairo/Pango typography.
        if std::env::var("DISPLAY").is_ok() || std::env::var("WAYLAND_DISPLAY").is_ok() {
            let r = root_rc.clone();
            let ev = event_rc.clone();
            let gtk_res = launch_gtk_with_events(
                move || (r.borrow_mut())(),
                config.clone(),
                move |event, rt| (ev.borrow_mut())(event, rt),
            );
            match gtk_res {
                Ok(()) => return Ok(()),
                Err(e) => {
                    eprintln!("[Strata] GTK display launch failed: {}. Falling back to native display servers...", e);
                }
            }

            if std::env::var("WAYLAND_DISPLAY").is_ok() {
                let r = root_rc.clone();
                let ev = event_rc.clone();
                if let Ok(()) = launch_wayland_with_events(
                    move || (r.borrow_mut())(),
                    config.clone(),
                    move |event, rt| (ev.borrow_mut())(event, rt),
                ) {
                    return Ok(());
                }
            }

            if std::env::var("DISPLAY").is_ok() {
                let r = root_rc.clone();
                let ev = event_rc.clone();
                if let Ok(()) = launch_x11_with_events(
                    move || (r.borrow_mut())(),
                    config.clone(),
                    move |event, rt| (ev.borrow_mut())(event, rt),
                ) {
                    return Ok(());
                }
            }
        }

        // Fallback to headless pump if no display servers are connected
        let r = root_rc.clone();
        let ev = event_rc.clone();
        launch_headless_pump(
            move || (r.borrow_mut())(),
            config,
            move |event, rt| (ev.borrow_mut())(event, rt),
        )
    }

    #[cfg(windows)]
    {
        let r = root_rc.clone();
        let ev = event_rc.clone();
        launch_win32_with_events(
            move || (r.borrow_mut())(),
            config,
            move |event, rt| (ev.borrow_mut())(event, rt),
        )
    }

    #[cfg(target_os = "macos")]
    {
        let r = root_rc.clone();
        let ev = event_rc.clone();
        launch_macos_with_events(
            move || (r.borrow_mut())(),
            config,
            move |event, rt| (ev.borrow_mut())(event, rt),
        )
    }

    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    {
        let r = root_rc.clone();
        let ev = event_rc.clone();
        launch_headless_pump(
            move || (r.borrow_mut())(),
            config,
            move |event, rt| (ev.borrow_mut())(event, rt),
        )
    }
}

/// Launches a Strata application with an event hook and a periodic `on_frame` callback.
///
/// On Linux GTK windows, `on_frame` is driven by the unified window scheduler at
/// `interval` (or ~16 ms when `None`). Returning [`FrameOutcome::Dirty`] re-invokes
/// `root_fn` and repaints; returning [`FrameOutcome::Unchanged`] skips rebuild and paint.
pub fn launch_with_frame<F, E, H>(
    root_fn: F,
    config: WindowConfig,
    event_handler: E,
    interval: Option<std::time::Duration>,
    on_frame: H,
) -> Result<(), String>
where
    F: FnMut() -> View + 'static,
    E: FnMut(&strata_platform::PlatformEvent, &mut AppRuntime) -> bool + 'static,
    H: FnMut(AppFrameInfo, &mut AppRuntime) -> FrameOutcome + 'static,
{
    #[cfg(target_os = "linux")]
    {
        let backend = std::env::var("STRATA_BACKEND")
            .unwrap_or_default()
            .to_lowercase();
        if (backend.is_empty() || backend == "gtk")
            && (std::env::var("DISPLAY").is_ok() || std::env::var("WAYLAND_DISPLAY").is_ok())
        {
            return launch_gtk_with_frame(root_fn, config, event_handler, interval, on_frame);
        }
        launch_with_events(root_fn, config, event_handler)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (interval, on_frame);
        launch_with_events(root_fn, config, event_handler)
    }
}

#[inline]
pub(crate) fn primary_refresh_interval() -> std::time::Duration {
    #[cfg(target_os = "linux")]
    {
        strata_platform::gtk_primary_refresh_interval()
    }
    #[cfg(not(target_os = "linux"))]
    {
        std::time::Duration::from_millis(16)
    }
}

pub(crate) fn compute_next_wakeup(
    rt: &AppRuntime,
    frame_hook_due_in: Option<std::time::Duration>,
    uncapped_present: bool,
) -> Option<std::time::Duration> {
    let mut best = strata_js::web::time_until_next_timer();
    let mut consider = |d: std::time::Duration| {
        best = Some(match best {
            Some(cur) => cur.min(d),
            None => d,
        });
    };
    if let Some(d) = frame_hook_due_in {
        consider(d);
    } else if rt.poll_retained_3d_dirty() {
        consider(std::time::Duration::ZERO);
    }
    if let Some(d) = rt.next_hover_tooltip_due_in() {
        consider(d);
    }
    if strata_dom::reactive::has_dirty_signals() {
        consider(std::time::Duration::ZERO);
    }
    let uncapped = uncapped_present || rt.has_uncapped_canvas3d();
    if rt.has_active_animations()
        || rt.active_smooth_scroll.is_some()
        || rt.scrollbar_drag.is_some()
        || rt.pending_window_drag.is_some()
        || rt.drag_selection_active
        || rt.mcp_cursor.is_some()
    {
        let anim_interval = if uncapped {
            std::time::Duration::ZERO
        } else {
            primary_refresh_interval()
        };
        consider(anim_interval);
    } else if rt.active_focus.is_some() {
        consider(std::time::Duration::from_millis(250));
    }
    best
}

/// Runs a deterministic headless event pump for testing, CI automation, and non-display environments.
pub fn launch_headless_pump<F, E>(
    mut root_fn: F,
    config: WindowConfig,
    mut event_handler: E,
) -> Result<(), String>
where
    F: FnMut() -> View + 'static,
    E: FnMut(&strata_platform::PlatformEvent, &mut AppRuntime) -> bool + 'static,
{
    println!(
        "[+] Launching Strata Headless Engine: \"{}\"...",
        config.title
    );
    let mut window = strata_platform::MockPlatformWindow::new(config.clone());
    let width = config.width as f32;
    let height = config.height as f32;
    let mut runtime = AppRuntime::from_root(|| root_fn(), width, height, 1.0);
    runtime.apply_window_config(&config);
    let mut presenter =
        SoftwareSurfacePresenter::new(config.width, config.height, SwapchainFormat::Bgra8Unorm);
    let mut compositor = SwapchainCompositor::new(config.width, config.height, 1.0);

    let max_frames = std::env::var("STRATA_HEADLESS_EXIT_AFTER_FRAMES")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(1);

    let mut frames_rendered = 0u64;

    while frames_rendered < max_frames {
        let _ = runtime.render_frame(&mut compositor, &mut presenter, frames_rendered == 0);
        let _ =
            window.present_surface(&presenter.backing_buffer, presenter.width, presenter.height);
        frames_rendered += 1;

        let events = window.poll_events();
        let mut should_exit = false;
        for ev in events {
            let (_, _, exit) = platforms::dispatch_native_platform_event(
                &ev,
                &mut runtime,
                &mut compositor,
                &mut presenter,
                &mut event_handler,
            );
            if exit {
                should_exit = true;
                break;
            }
        }
        if should_exit {
            break;
        }
    }

    println!(
        "[OK] Strata Headless execution complete: {} physical frames presented.",
        frames_rendered
    );
    Ok(())
}

/// Headless runner returning the initialized `AppRuntime` for testing and layout inspection.
pub fn launch_headless<F>(root: F, width: f32, height: f32) -> AppRuntime
where
    F: FnOnce() -> View + 'static,
{
    AppRuntime::from_root(root, width, height, 1.0)
}

/// Launches a live interactive Strata window with surface presentation and frame pacing.
pub fn launch_window<F>(
    root: F,
    config: WindowConfig,
) -> (AppRuntime, LivePlatformWindow, SoftwareSurfacePresenter)
where
    F: FnOnce() -> View + 'static,
{
    println!(
        "[+] Launching Strata Live Native Window: \"{}\"...",
        config.title
    );
    let width = config.width as f32;
    let height = config.height as f32;
    let mut runtime = AppRuntime::from_root(root, width, height, 1.0);
    runtime.apply_window_config(&config);

    let mut window = LivePlatformWindow::new(config.clone());
    let mut presenter =
        SoftwareSurfacePresenter::new(config.width, config.height, SwapchainFormat::Bgra8Unorm);
    let mut compositor = SwapchainCompositor::new(config.width, config.height, 1.0);

    let _ = runtime.render_frame(&mut compositor, &mut presenter, true);
    let _ = window.present_surface(&presenter.backing_buffer, presenter.width, presenter.height);

    println!(
        "[OK] Strata Live Window active: {}x{} physical pixels, {} commands presented.",
        presenter.width,
        presenter.height,
        runtime.display_list.commands.len()
    );

    (runtime, window, presenter)
}

#[cfg(target_os = "linux")]
/// Launches an interactive native GTK3/Cairo desktop window on Linux with high-fidelity system typography.
pub fn launch_gtk<F>(root_fn: F, config: WindowConfig) -> Result<(), String>
where
    F: FnMut() -> View + 'static,
{
    launch_gtk_with_events(root_fn, config, |_, _| false)
}

/// Invokes the application's platform event hook and returns its redraw/rebuild decision.
/// This is the single dispatch seam used by the native window event pumps, so tray,
/// single-instance, and window events all flow through the same user callback.
pub fn invoke_user_event_hook<E>(
    event: &strata_platform::PlatformEvent,
    runtime: &mut AppRuntime,
    hook: &mut E,
) -> bool
where
    E: FnMut(&strata_platform::PlatformEvent, &mut AppRuntime) -> bool,
{
    hook(event, runtime)
}

#[cfg(target_os = "linux")]
type FrameHookFn = Box<dyn FnMut(AppFrameInfo, &mut AppRuntime) -> FrameOutcome>;

#[cfg(target_os = "linux")]
/// Launches an interactive native GTK3/Cairo desktop window on Linux with a custom event hook.
pub fn launch_gtk_with_events<F, E>(
    root_fn: F,
    config: WindowConfig,
    event_handler: E,
) -> Result<(), String>
where
    F: FnMut() -> View + 'static,
    E: FnMut(&strata_platform::PlatformEvent, &mut AppRuntime) -> bool + 'static,
{
    launch_gtk_with_frame_inner(root_fn, config, event_handler, None)
}

#[cfg(target_os = "linux")]
/// Launches an interactive native GTK3/Cairo desktop window on Linux with a custom
/// event hook and a periodic `on_frame` callback.
pub fn launch_gtk_with_frame<F, E, H>(
    root_fn: F,
    config: WindowConfig,
    event_handler: E,
    interval: Option<std::time::Duration>,
    on_frame: H,
) -> Result<(), String>
where
    F: FnMut() -> View + 'static,
    E: FnMut(&strata_platform::PlatformEvent, &mut AppRuntime) -> bool + 'static,
    H: FnMut(AppFrameInfo, &mut AppRuntime) -> FrameOutcome + 'static,
{
    launch_gtk_with_frame_inner(
        root_fn,
        config,
        event_handler,
        Some((interval, Box::new(on_frame))),
    )
}

#[cfg(target_os = "linux")]
fn launch_gtk_with_frame_inner<F, E>(
    root_fn: F,
    config: WindowConfig,
    event_handler: E,
    on_frame_opt: Option<(Option<std::time::Duration>, FrameHookFn)>,
) -> Result<(), String>
where
    F: FnMut() -> View + 'static,
    E: FnMut(&strata_platform::PlatformEvent, &mut AppRuntime) -> bool + 'static,
{
    let _single_instance_lock = if let Some(ref app_id) = config.single_instance_id {
        let args: Vec<String> = std::env::args().collect();
        match strata_platform::SingleInstanceLock::acquire(app_id, &args, |forwarded_args| {
            #[cfg(target_os = "linux")]
            {
                strata_platform::gtk_focus_window();
                strata_platform::post_platform_event(
                    strata_platform::PlatformEvent::SingleInstanceTriggered(forwarded_args),
                );
            }
        }) {
            Ok(strata_platform::SingleInstanceStatus::Primary(lock)) => Some(lock),
            Ok(strata_platform::SingleInstanceStatus::SecondaryExited) => {
                return Ok(());
            }
            Err(e) => {
                eprintln!("[Strata] SingleInstanceLock warning: {}", e);
                None
            }
        }
    } else {
        None
    };

    let root_fn_rc = std::rc::Rc::new(std::cell::RefCell::new(root_fn));
    let event_handler_rc = std::rc::Rc::new(std::cell::RefCell::new(event_handler));

    let _ = strata_dom::reactive::take_dirty_signals();
    let mut initial_runtime = AppRuntime::from_root(
        || (root_fn_rc.borrow_mut())(),
        config.width as f32,
        config.height as f32,
        1.0,
    );
    initial_runtime.apply_window_config(&config);
    let runtime = std::rc::Rc::new(std::cell::RefCell::new(initial_runtime));

    let transparent_bg = config.transparent
        || config.decoration == strata_platform::WindowDecoration::FramelessTransparent;
    let allow_maximize = config.resizable && !config.is_docked();

    let runtime_draw = runtime.clone();
    let root_fn_draw = root_fn_rc.clone();
    let on_draw = move |width: f32, height: f32, cr: &strata_platform::cairo::Context| {
        let mut resize_fired = false;
        {
            let mut rt = runtime_draw.borrow_mut();
            let size_changed = (rt.viewport_width - width).abs() > 0.01
                || (rt.viewport_height - height).abs() > 0.01;
            rt.viewport_width = width;
            rt.viewport_height = height;
            if size_changed {
                {
                    let rt = &mut *rt;
                    rt.unapply_scroll_offsets();
                    rt.layout_root.compute_layout(width, height);
                    AppRuntime::sync_layout_to_ir(&rt.layout_root, &mut rt.ir_graph);
                    rt.apply_viewport_clamps();
                    rt.apply_scroll_offsets();
                    rt.spatial_index
                        .rebuild(&rt.root_blueprint, &rt.layout_root);
                }
                resize_fired = rt.dispatch_resize_observers();
                rt.mark_paint_dirty();
            }
        }
        if resize_fired {
            let _ = strata_dom::reactive::take_dirty_signals();
            runtime_draw
                .borrow_mut()
                .update_root(|| (root_fn_draw.borrow_mut())());
        }
        let mut rt = runtime_draw.borrow_mut();
        // Repaints with no state change (expose, focus) reuse the compiled list.
        let dl_recompiled = rt.compile_display_list_if_dirty();
        let mcp_overlay_active = rt
            .mcp_cursor
            .as_ref()
            .is_some_and(|c| c.fade_opacity > 0.001);
        let overlay_dirty = dl_recompiled || resize_fired || mcp_overlay_active;

        let cursor_tuple = rt
            .mcp_cursor
            .as_ref()
            .map(|c| c as *const strata_platform::McpCursorState);
        let theme_ref = if cursor_tuple.is_some() {
            Some(rt.cursor_theme() as *const strata_platform::CursorTheme)
        } else {
            None
        };
        let gpu_presented = {
            let cursor_pair = match (cursor_tuple, theme_ref) {
                // SAFETY: Both pointers borrow disjoint fields of `rt` for the duration of the call.
                (Some(c), Some(t)) => Some(unsafe { (&*c, &*t) }),
                _ => None,
            };
            strata_platform::try_present_display_list_gpu(
                &rt.display_list,
                width,
                height,
                overlay_dirty,
                transparent_bg,
                cursor_pair,
            )
        };

        if gpu_presented {
            rt.acknowledge_retained_3d_presented();
            return;
        }

        // Clear or fill window background
        if transparent_bg {
            cr.set_operator(strata_platform::cairo::Operator::Source);
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
            let _ = cr.paint();
            cr.set_operator(strata_platform::cairo::Operator::Over);
        } else {
            cr.set_source_rgb(0.035, 0.035, 0.043);
            let _ = cr.paint();
        }

        strata_platform::render_display_list_to_cairo(cr, &rt.display_list);
        rt.acknowledge_retained_3d_presented();

        // Render MCP emulated cursor overlay if active and visible
        if let Some(ref mcp_cursor) = rt.mcp_cursor {
            if mcp_cursor.fade_opacity > 0.001 {
                rt.cursor_theme().render_cairo(
                    cr,
                    mcp_cursor.cursor_icon,
                    mcp_cursor.current_x,
                    mcp_cursor.current_y,
                    mcp_cursor.fade_opacity,
                    mcp_cursor.is_down,
                );
            }
        }
    };

    let frame_hook_state = std::rc::Rc::new(std::cell::RefCell::new(on_frame_opt.map(
        |(explicit_interval, cb)| {
            let now = std::time::Instant::now();
            (explicit_interval, now, None::<std::time::Instant>, cb)
        },
    )));
    let frame_hook_tick = frame_hook_state.clone();

    let runtime_event = runtime.clone();
    let root_fn_event = root_fn_rc.clone();
    let event_handler_event = event_handler_rc.clone();
    let runtime_dirty = runtime.clone();
    let on_event_inner = move |event: &strata_platform::PlatformEvent| -> (bool, bool) {
        let mut needs_rebuild = false;
        let mut needs_redraw = false;
        {
            let mut rt = runtime_event.borrow_mut();
            if crate::mcp_bridge::drain_and_execute_mcp_requests(&mut rt) {
                needs_rebuild = true;
            }
            let hook_signals_before = strata_dom::reactive::has_dirty_signals();
            if invoke_user_event_hook(event, &mut rt, &mut *event_handler_event.borrow_mut())
                || (!hook_signals_before && strata_dom::reactive::has_dirty_signals())
            {
                needs_rebuild = true;
                needs_redraw = true;
            }
            if let strata_platform::PlatformEvent::WindowResized { width, height } = event {
                let wf = (*width).max(1) as f32;
                let hf = (*height).max(1) as f32;
                let size_changed = (rt.viewport_width - wf).abs() > 0.01
                    || (rt.viewport_height - hf).abs() > 0.01;
                rt.viewport_width = wf;
                rt.viewport_height = hf;
                if size_changed {
                    let signals_before = strata_dom::reactive::has_dirty_signals();
                    {
                        let rt = &mut *rt;
                        rt.unapply_scroll_offsets();
                        rt.layout_root.compute_layout(wf, hf);
                        AppRuntime::sync_layout_to_ir(&rt.layout_root, &mut rt.ir_graph);
                        rt.apply_viewport_clamps();
                        rt.apply_scroll_offsets();
                        rt.spatial_index
                            .rebuild(&rt.root_blueprint, &rt.layout_root);
                    }
                    let resize_fired = rt.dispatch_resize_observers();
                    if resize_fired
                        || (!signals_before && strata_dom::reactive::has_dirty_signals())
                    {
                        needs_rebuild = true;
                    }
                    rt.mark_paint_dirty();
                }
                needs_redraw = true;
            }
            if let strata_platform::PlatformEvent::Scroll {
                delta_x, delta_y, ..
            } = event
            {
                let (px, py) = rt.last_mouse_pos;
                let signals_before = strata_dom::reactive::has_dirty_signals();
                let (wheel_handled, scroll_changed) =
                    rt.dispatch_smooth_scroll_action(px, py, *delta_x, *delta_y);
                if wheel_handled {
                    let retained_only = !signals_before
                        && !strata_dom::reactive::has_dirty_signals()
                        && rt.target_is_retained_canvas3d(px, py);
                    if !retained_only {
                        needs_rebuild = true;
                    }
                }
                if rt.dismiss_hover_tooltip_and_repaint() {
                    needs_redraw = true;
                }
                if scroll_changed {
                    needs_redraw = true;
                }
            }
            if let strata_platform::PlatformEvent::AnimationTick = event {
                let scroll_changed = rt.poll_smooth_scroll();
                let anim_changed = rt.poll_animations();
                let caret_changed = rt.poll_caret_blink();
                let tooltip_changed = rt.poll_hover_tooltip();
                let mcp_changed = if let Some(ref mut c) = rt.mcp_cursor {
                    c.poll_tick()
                } else {
                    false
                };
                if scroll_changed
                    || anim_changed
                    || caret_changed
                    || tooltip_changed
                    || mcp_changed
                {
                    needs_redraw = true;
                }
                if strata_js::web::tick_timers() > 0 {
                    needs_rebuild = true;
                    needs_redraw = true;
                }
                if let Some((
                    explicit_interval,
                    ref mut last_called,
                    ref mut next_deadline_opt,
                    ref mut cb,
                )) = *frame_hook_tick.borrow_mut()
                {
                    let interval = explicit_interval.unwrap_or_else(primary_refresh_interval);
                    let now = std::time::Instant::now();
                    let deadline = *next_deadline_opt.get_or_insert_with(|| *last_called + interval);
                    let tolerance = std::time::Duration::from_micros(600).min(interval / 3);
                    if now + tolerance >= deadline {
                        let elapsed = now.saturating_duration_since(*last_called);
                        *last_called = now;
                        let mut next_dl = deadline + interval;
                        if next_dl <= now {
                            next_dl = now + interval;
                        }
                        *next_deadline_opt = Some(next_dl);
                        let info = AppFrameInfo {
                            frame_index: rt.frame_count,
                            viewport_width: rt.viewport_width,
                            viewport_height: rt.viewport_height,
                            dpr: rt.dpr,
                            command_count: rt.display_list.commands.len(),
                            dt: elapsed,
                        };
                        if cb(info, &mut rt).is_dirty() {
                            needs_rebuild = true;
                            needs_redraw = true;
                        }
                    }
                }
                if strata_dom::reactive::has_dirty_signals() {
                    needs_rebuild = true;
                    needs_redraw = true;
                }
            }
            if let strata_platform::PlatformEvent::DoubleClick { button, x, y } = event {
                let _btn_idx = button.to_button_index();
                if *button == strata_platform::MouseButton::Left && allow_maximize {
                    let region = AppRuntime::hit_test_property(
                        &rt.root_blueprint,
                        &rt.layout_root,
                        *x,
                        *y,
                        "data-app-region",
                    );
                    if region.as_deref() == Some("drag") {
                        #[cfg(target_os = "linux")]
                        {
                            let new_max = strata_platform::gtk_toggle_maximize();
                            let _ = invoke_user_event_hook(
                                &strata_platform::PlatformEvent::WindowMaximized(new_max),
                                &mut rt,
                                &mut *event_handler_event.borrow_mut(),
                            );
                        }
                        needs_rebuild = true;
                    }
                }
                if rt.dispatch_dblclick(*x, *y) {
                    needs_rebuild = true;
                }
            }
            if let strata_platform::PlatformEvent::MouseDown { x, y, button } = event {
                rt.last_mouse_pos = (*x, *y);
                if rt.dismiss_hover_tooltip_and_repaint() {
                    needs_redraw = true;
                }
                let btn_idx = button.to_button_index();
                let signals_before = strata_dom::reactive::has_dirty_signals();
                if *button == strata_platform::MouseButton::Left {
                    rt.is_mouse_down = true;
                    if rt.check_scrollbar_thumb_press(*x, *y) {
                        needs_redraw = true;
                    } else {
                        rt.begin_window_drag_candidate(*x, *y);
                        let action = rt.dispatch_pointer_down_action_with_button_and_modifiers(
                            *x,
                            *y,
                            btn_idx,
                            strata_platform::gtk_current_modifiers(),
                        );
                        let retained_only = action == PointerDownAction::DomListenerInvoked
                            && !signals_before
                            && !strata_dom::reactive::has_dirty_signals()
                            && rt.target_is_retained_canvas3d(*x, *y);
                        if action.needs_rebuild() && !retained_only {
                            needs_rebuild = true;
                        } else if action.needs_redraw() && !retained_only {
                            needs_redraw = true;
                        }
                        if rt.dispatch_mouse_click(*x, *y, btn_idx, 1) {
                            needs_rebuild = true;
                        }
                        if rt.drain_edit_commands() {
                            needs_rebuild = true;
                        }
                        if !needs_rebuild && rt.restyle_for_active_change() {
                            needs_redraw = true;
                        }
                    }
                } else if *button == strata_platform::MouseButton::Middle {
                    let action = rt.dispatch_pointer_down_action_with_button_and_modifiers(
                        *x,
                        *y,
                        btn_idx,
                        strata_platform::gtk_current_modifiers(),
                    );
                    let retained_only = action == PointerDownAction::DomListenerInvoked
                        && !signals_before
                        && !strata_dom::reactive::has_dirty_signals()
                        && rt.target_is_retained_canvas3d(*x, *y);
                    if action.needs_rebuild() && !retained_only {
                        needs_rebuild = true;
                    } else if action.needs_redraw() && !retained_only {
                        needs_redraw = true;
                    }
                    if rt.dispatch_mouse_click(*x, *y, btn_idx, 1) {
                        needs_rebuild = true;
                    }
                } else if *button == strata_platform::MouseButton::Right {
                    let action = rt.dispatch_pointer_down_action_with_button_and_modifiers(
                        *x,
                        *y,
                        btn_idx,
                        strata_platform::gtk_current_modifiers(),
                    );
                    let retained_only = action == PointerDownAction::DomListenerInvoked
                        && !signals_before
                        && !strata_dom::reactive::has_dirty_signals()
                        && rt.target_is_retained_canvas3d(*x, *y);
                    if action.needs_rebuild() && !retained_only {
                        needs_rebuild = true;
                    } else if action.needs_redraw() && !retained_only {
                        needs_redraw = true;
                    }
                    if rt.dispatch_context_menu(*x, *y) {
                        needs_rebuild = true;
                    }
                    if rt.drain_edit_commands() {
                        needs_rebuild = true;
                    }
                }
            }
            if let strata_platform::PlatformEvent::MouseUp { x, y, button } = event {
                rt.last_mouse_pos = (*x, *y);
                let btn_idx = button.to_button_index();
                let was_mouse_down = rt.is_mouse_down;
                if *button == strata_platform::MouseButton::Left {
                    rt.is_mouse_down = false;
                }
                if rt.scrollbar_drag.is_some() {
                    rt.scrollbar_drag = None;
                    needs_redraw = true;
                }
                rt.pending_window_drag = None;
                let signals_before = strata_dom::reactive::has_dirty_signals();
                let was_retained_canvas = rt.target_is_retained_canvas3d(*x, *y);
                let action = rt.dispatch_pointer_up_action_with_button(*x, *y, btn_idx);
                let retained_only = action == PointerUpAction::DomListenerInvoked
                    && !signals_before
                    && !strata_dom::reactive::has_dirty_signals()
                    && was_retained_canvas;
                if action.needs_rebuild() && !retained_only {
                    needs_rebuild = true;
                } else if action.needs_redraw() && !retained_only {
                    needs_redraw = true;
                }
                if was_mouse_down && !needs_rebuild && rt.restyle_for_active_change() {
                    needs_redraw = true;
                }
            }
            if let strata_platform::PlatformEvent::MouseMove { x, y } = event {
                if rt.scrollbar_drag.is_some() {
                    if rt.poll_scrollbar_drag(*x, *y) {
                        needs_redraw = true;
                    }
                } else {
                    if rt.poll_window_drag(*x, *y) {
                        #[cfg(target_os = "linux")]
                        {
                            let is_max = strata_platform::gtk_is_maximized();
                            let _ = invoke_user_event_hook(
                                &strata_platform::PlatformEvent::WindowMaximized(is_max),
                                &mut rt,
                                &mut *event_handler_event.borrow_mut(),
                            );
                        }
                        needs_rebuild = true;
                    }
                    let signals_before = strata_dom::reactive::has_dirty_signals();
                    let is_retained_canvas = rt.target_is_retained_canvas3d(*x, *y);
                    let action = rt.dispatch_pointer_move_action(*x, *y);
                    let retained_only = action == PointerMoveAction::DomListenerInvoked
                        && !signals_before
                        && !strata_dom::reactive::has_dirty_signals()
                        && is_retained_canvas;
                    if action.needs_rebuild() && !retained_only {
                        needs_rebuild = true;
                    } else if action.needs_redraw() && !retained_only {
                        needs_redraw = true;
                    }
                    if rt.update_hover(*x, *y) {
                        if rt.hover_is_paint_only() {
                            needs_redraw = true;
                        } else {
                            needs_rebuild = true;
                        }
                    }
                    let cur = super::platforms::resolved_cursor_at(&rt, *x, *y);
                    rt.current_cursor = cur;
                    strata_platform::gtk_set_cursor(cur);
                }
            }
            if matches!(event, strata_platform::PlatformEvent::CursorLeft) {
                rt.is_mouse_down = false;
                rt.current_cursor = strata_platform::window::CursorIcon::Default;
                strata_platform::gtk_set_cursor(strata_platform::window::CursorIcon::Default);
                if rt.clear_hover() {
                    if rt.hover_is_paint_only() {
                        needs_redraw = true;
                    } else {
                        needs_rebuild = true;
                    }
                }
            }
            if matches!(
                event,
                strata_platform::PlatformEvent::KeyDown { .. }
                    | strata_platform::PlatformEvent::KeyUp { .. }
                    | strata_platform::PlatformEvent::ReceivedCharacter(_)
                    | strata_platform::PlatformEvent::ImePreedit { .. }
                    | strata_platform::PlatformEvent::ImeCommit(_)
            ) {
                if matches!(event, strata_platform::PlatformEvent::KeyDown { .. })
                    && rt.dismiss_hover_tooltip_and_repaint()
                {
                    needs_redraw = true;
                }
                let res = rt.dispatch_key_event(event);
                if res.needs_rebuild() {
                    needs_rebuild = true;
                } else if res.needs_redraw() {
                    needs_redraw = true;
                }
            }
            if let strata_platform::PlatformEvent::FileDrop(paths) = event {
                if rt.dismiss_hover_tooltip_and_repaint() {
                    needs_redraw = true;
                }
                if rt.dispatch_file_drop(paths.clone()) {
                    needs_rebuild = true;
                }
            }
        }

        if perf_logging_enabled() && needs_rebuild {
            eprintln!("[perf] rebuild_trigger: {:?}", event);
        }

        let defer_3d_to_frame_hook = frame_hook_tick.borrow().is_some()
            && matches!(
                event,
                strata_platform::PlatformEvent::MouseMove { .. }
                    | strata_platform::PlatformEvent::Scroll { .. }
            );
        let retained_3d_dirty = !needs_rebuild
            && !needs_redraw
            && !defer_3d_to_frame_hook
            && runtime_event.borrow().poll_retained_3d_dirty();

        let refresh_pointer_cursor = |rt: &mut AppRuntime| {
            let (px, py) = rt.last_mouse_pos;
            let cur = super::platforms::resolved_cursor_at(rt, px, py);
            rt.current_cursor = cur;
            strata_platform::gtk_set_cursor(cur);
        };

        if needs_rebuild {
            #[cfg(target_os = "linux")]
            if strata_platform::gtk_is_closing() {
                return (false, false);
            }
            if runtime_event.borrow_mut().sync_dynamic_text() {
                let _ = strata_dom::reactive::take_dirty_signals();
                if !matches!(event, strata_platform::PlatformEvent::CursorLeft)
                    && (matches!(
                        event,
                        strata_platform::PlatformEvent::MouseDown { .. }
                            | strata_platform::PlatformEvent::MouseUp { .. }
                            | strata_platform::PlatformEvent::MouseMove { .. }
                    ) || runtime_event.borrow().current_hover_path.is_some())
                {
                    refresh_pointer_cursor(&mut runtime_event.borrow_mut());
                }
                (true, true)
            } else {
                let _ = strata_dom::reactive::take_dirty_signals();
                let new_view =
                    strata_dom::with_root_component_scope(|| (root_fn_event.borrow_mut())());
                let mut rt = runtime_event.borrow_mut();
                rt.update_view(new_view);
                if !matches!(event, strata_platform::PlatformEvent::CursorLeft)
                    && (matches!(
                        event,
                        strata_platform::PlatformEvent::MouseDown { .. }
                            | strata_platform::PlatformEvent::MouseUp { .. }
                            | strata_platform::PlatformEvent::MouseMove { .. }
                    ) || rt.current_hover_path.is_some())
                {
                    refresh_pointer_cursor(&mut rt);
                }
                (true, true)
            }
        } else if needs_redraw {
            #[cfg(target_os = "linux")]
            if strata_platform::gtk_is_closing() {
                return (false, false);
            }
            if matches!(
                event,
                strata_platform::PlatformEvent::MouseDown { .. }
                    | strata_platform::PlatformEvent::MouseUp { .. }
            ) {
                refresh_pointer_cursor(&mut runtime_event.borrow_mut());
            }
            (true, true)
        } else if retained_3d_dirty {
            #[cfg(target_os = "linux")]
            if strata_platform::gtk_is_closing() {
                return (false, false);
            }
            if matches!(
                event,
                strata_platform::PlatformEvent::MouseDown { .. }
                    | strata_platform::PlatformEvent::MouseUp { .. }
            ) {
                refresh_pointer_cursor(&mut runtime_event.borrow_mut());
            }
            (true, false)
        } else {
            if matches!(
                event,
                strata_platform::PlatformEvent::MouseDown { .. }
                    | strata_platform::PlatformEvent::MouseUp { .. }
            ) {
                refresh_pointer_cursor(&mut runtime_event.borrow_mut());
            }
            (false, false)
        }
    };

    let perf_launch = perf_logging_enabled();
    let mut last_present_instant = std::time::Instant::now();
    // Only 2D/UI updates invalidate the compiled display list; 3D-only retained updates keep it cached.
    let on_event = move |event: &strata_platform::PlatformEvent| -> bool {
        let t_ev_start = if perf_launch {
            Some(std::time::Instant::now())
        } else {
            None
        };
        let (redraw, paint_dirty) = on_event_inner(event);
        let t_after_inner = t_ev_start.map(|_| std::time::Instant::now());
        if paint_dirty {
            let mut rt = runtime_dirty.borrow_mut();
            let rebuild_done = matches!(event, strata_platform::PlatformEvent::AnimationTick)
                && !rt.is_paint_dirty();
            if !rebuild_done {
                rt.mark_paint_dirty();
            }
            if redraw && strata_platform::gtk_gpu_surface_is_active() {
                let dl_recompiled = rt.compile_display_list_if_dirty();
                let mcp_active = rt
                    .mcp_cursor
                    .as_ref()
                    .is_some_and(|c| c.fade_opacity > 0.001);
                let overlay_dirty = dl_recompiled || rebuild_done || mcp_active;
                let (w, h) = (rt.viewport_width, rt.viewport_height);
                let cursor_tuple = rt
                    .mcp_cursor
                    .as_ref()
                    .map(|c| c as *const strata_platform::McpCursorState);
                let theme_ref =
                    cursor_tuple.map(|_| rt.cursor_theme() as *const strata_platform::CursorTheme);
                let cursor_pair = match (cursor_tuple, theme_ref) {
                    (Some(c), Some(t)) => Some(unsafe { (&*c, &*t) }),
                    _ => None,
                };
                if strata_platform::try_present_display_list_gpu(
                    &rt.display_list,
                    w,
                    h,
                    overlay_dirty,
                    transparent_bg,
                    cursor_pair,
                ) {
                    rt.acknowledge_retained_3d_presented();
                    if let (Some(t0), Some(t1)) = (t_ev_start, t_after_inner) {
                        let now = std::time::Instant::now();
                        let gap_ms = now.duration_since(last_present_instant).as_secs_f64() * 1000.0;
                        last_present_instant = now;
                        let inner_ms = t1.duration_since(t0).as_secs_f64() * 1000.0;
                        let pres_ms = now.duration_since(t1).as_secs_f64() * 1000.0;
                        if gap_ms > 10.5 || inner_ms > 3.5 || pres_ms > 5.0 {
                            eprintln!(
                                "[perf] slow_frame({:?}, ov={}) gap={:.2}ms inner={:.2}ms gpu_ov_pres={:.2}ms",
                                event, overlay_dirty, gap_ms, inner_ms, pres_ms
                            );
                        }
                    }
                    return false;
                }
            }
            return redraw;
        }
        if redraw
            && !runtime_dirty.borrow().is_paint_dirty()
            && strata_platform::gtk_present_active_gpu_surface_now()
        {
            runtime_dirty
                .borrow_mut()
                .acknowledge_retained_3d_presented();
            if let (Some(t0), Some(t1)) = (t_ev_start, t_after_inner) {
                let now = std::time::Instant::now();
                let gap_ms = now.duration_since(last_present_instant).as_secs_f64() * 1000.0;
                last_present_instant = now;
                let inner_ms = t1.duration_since(t0).as_secs_f64() * 1000.0;
                let pres_ms = now.duration_since(t1).as_secs_f64() * 1000.0;
                if gap_ms > 10.5 || inner_ms > 3.5 || pres_ms > 5.0 {
                    eprintln!(
                        "[perf] slow_frame({:?}, active_3d) gap={:.2}ms inner={:.2}ms pres={:.2}ms",
                        event, gap_ms, inner_ms, pres_ms
                    );
                }
            }
            return false;
        }
        redraw
    };

    let uncapped_present = matches!(
        config.present_mode,
        strata_platform::PresentMode::Immediate | strata_platform::PresentMode::Mailbox
    );
    let runtime_wakeup = runtime.clone();
    let frame_hook_wakeup = frame_hook_state;
    let next_wakeup = move || -> Option<std::time::Duration> {
        let hook_due = frame_hook_wakeup
            .borrow_mut()
            .as_mut()
            .map(|(explicit_interval, last_called, next_deadline_opt, _)| {
                let interval = explicit_interval.unwrap_or_else(primary_refresh_interval);
                let now = std::time::Instant::now();
                let deadline = *next_deadline_opt.get_or_insert_with(|| *last_called + interval);
                deadline.saturating_duration_since(now)
            });
        compute_next_wakeup(&runtime_wakeup.borrow(), hook_due, uncapped_present)
    };

    let _ = crate::mcp_bridge::start_mcp_dev_listener(None);
    strata_platform::launch_gtk_window_with_scheduler(&config, on_draw, on_event, next_wakeup)
}

#[cfg(test)]
mod tests {
    use super::compute_next_wakeup;
    use std::time::Duration;
    use strata_dom::{mark_dirty, reactive::take_dirty_signals, reset_component_hooks, Signal};
    use strata_macro::rsx;

    #[test]
    fn compute_next_wakeup_wakes_immediately_on_dirty_signals_and_mark_dirty() {
        reset_component_hooks();
        let _ = take_dirty_signals();

        let sig = Signal::new(0i32);
        let rt = crate::AppRuntime::from_root(
            || {
                let v = sig.get();
                rsx! { <div>{format!("{v}")}</div> }
            },
            200.0,
            100.0,
            1.0,
        );

        let _ = take_dirty_signals();
        assert_eq!(
            compute_next_wakeup(&rt, None, false),
            None,
            "idle runtime without dirty signals or timers should not schedule wakeup"
        );

        // Programmatic signal mutation must request an immediate wakeup even with a future frame deadline.
        sig.set(42);
        assert_eq!(
            compute_next_wakeup(&rt, Some(Duration::from_secs(5)), false),
            Some(Duration::ZERO),
            "Signal::set must override future idle/frame deadlines with Duration::ZERO"
        );

        assert!(take_dirty_signals());
        assert_eq!(compute_next_wakeup(&rt, None, false), None);

        // Explicit mark_dirty() must also request immediate wakeup.
        mark_dirty();
        assert_eq!(
            compute_next_wakeup(&rt, None, false),
            Some(Duration::ZERO)
        );
        let _ = take_dirty_signals();
    }
}
