//! Window launching, headless execution, and Linux GTK platform lifecycle hooks.

use super::*;

pub fn launch<F>(root: F)
where
    F: FnOnce() -> View + 'static,
{
    println!("[+] Initializing Strata Universal Frontend Runtime...");
    let view = root();
    let runtime = AppRuntime::new(view, 1280.0, 800.0, 1.0);
    println!(
        "[OK] Strata Runtime active: viewport {}x{}, rendered {} display commands.",
        runtime.viewport_width,
        runtime.viewport_height,
        runtime.display_list.commands.len()
    );
}

/// Headless runner returning the initialized `AppRuntime` for testing and layout inspection.
pub fn launch_headless<F>(root: F, width: f32, height: f32) -> AppRuntime
where
    F: FnOnce() -> View + 'static,
{
    let view = root();
    AppRuntime::new(view, width, height, 1.0)
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
    let view = root();
    let width = config.width as f32;
    let height = config.height as f32;
    let mut runtime = AppRuntime::new(view, width, height, 1.0);

    let window = LivePlatformWindow::new(config.clone());
    let mut presenter =
        SoftwareSurfacePresenter::new(config.width, config.height, SwapchainFormat::Bgra8Unorm);
    let mut compositor = SwapchainCompositor::new(config.width, config.height, 1.0);

    let _ = runtime.render_frame(&mut compositor, &mut presenter, true);

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

    let initial_view = (root_fn_rc.borrow_mut())();
    let runtime = std::rc::Rc::new(std::cell::RefCell::new(AppRuntime::new(
        initial_view,
        config.width as f32,
        config.height as f32,
        1.0,
    )));

    let runtime_draw = runtime.clone();
    let on_draw = move |width: f32, height: f32, cr: &strata_platform::cairo::Context| {
        let mut rt = runtime_draw.borrow_mut();
        let size_changed =
            (rt.viewport_width - width).abs() > 0.01 || (rt.viewport_height - height).abs() > 0.01;
        rt.viewport_width = width;
        rt.viewport_height = height;
        if size_changed {
            let rt = &mut *rt;
            rt.unapply_scroll_offsets();
            rt.layout_root.compute_layout(width, height);
            AppRuntime::sync_layout_to_ir(&rt.layout_root, &mut rt.ir_graph);
            rt.apply_viewport_clamps();
            rt.apply_scroll_offsets();
        }
        rt.compile_display_list();

        // Fill background
        cr.set_source_rgb(0.035, 0.035, 0.043);
        let _ = cr.paint();

        strata_platform::render_display_list_to_cairo(cr, &rt.display_list);

        // Render MCP emulated cursor overlay if active and visible
        if let Some(ref mcp_cursor) = rt.mcp_cursor {
            if mcp_cursor.fade_opacity > 0.001 {
                rt.cursor_theme.render_cairo(
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

    let runtime_event = runtime.clone();
    let root_fn_event = root_fn_rc.clone();
    let event_handler_event = event_handler_rc.clone();
    let on_event = move |event: &strata_platform::PlatformEvent| -> bool {
        let mut needs_rebuild = false;
        let mut needs_redraw = false;
        {
            let mut rt = runtime_event.borrow_mut();
            if crate::mcp_bridge::drain_and_execute_mcp_requests(&mut rt) {
                needs_rebuild = true;
            }
            if invoke_user_event_hook(event, &mut rt, &mut *event_handler_event.borrow_mut()) {
                needs_rebuild = true;
            }
            if let strata_platform::PlatformEvent::Scroll {
                delta_x, delta_y, ..
            } = event
            {
                let (px, py) = rt.last_mouse_pos;
                if rt.dispatch_smooth_scroll(px, py, *delta_x, *delta_y) {
                    needs_redraw = true;
                }
            }
            if let strata_platform::PlatformEvent::AnimationTick = event {
                let scroll_changed = rt.poll_smooth_scroll();
                let anim_changed = rt.poll_animations();
                let caret_changed = rt.poll_caret_blink();
                let mcp_changed = if let Some(ref mut c) = rt.mcp_cursor {
                    c.poll_tick()
                } else {
                    false
                };
                if scroll_changed || anim_changed || caret_changed || mcp_changed {
                    needs_redraw = true;
                }
            }
            if let strata_platform::PlatformEvent::DoubleClick { button, x, y } = event {
                let _btn_idx = button.to_button_index();
                if *button == strata_platform::MouseButton::Left {
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
                let btn_idx = button.to_button_index();
                if *button == strata_platform::MouseButton::Left {
                    if rt.check_scrollbar_thumb_press(*x, *y) {
                        needs_redraw = true;
                    } else {
                        rt.begin_window_drag_candidate(*x, *y);
                        let action = rt.dispatch_pointer_down_action_with_modifiers(
                            *x,
                            *y,
                            strata_platform::gtk_current_modifiers(),
                        );
                        if action.needs_rebuild() {
                            needs_rebuild = true;
                        } else if action.needs_redraw() {
                            needs_redraw = true;
                        }
                        if rt.dispatch_mouse_click(*x, *y, btn_idx, 1) {
                            needs_rebuild = true;
                        }
                        if rt.drain_edit_commands() {
                            needs_rebuild = true;
                        }
                    }
                } else if *button == strata_platform::MouseButton::Middle {
                    let action = rt.dispatch_pointer_down_action(*x, *y);
                    if action.needs_rebuild() {
                        needs_rebuild = true;
                    } else if action.needs_redraw() {
                        needs_redraw = true;
                    }
                    if rt.dispatch_mouse_click(*x, *y, btn_idx, 1) {
                        needs_rebuild = true;
                    }
                } else if *button == strata_platform::MouseButton::Right {
                    if rt.dispatch_context_menu(*x, *y) {
                        needs_rebuild = true;
                    }
                    if rt.drain_edit_commands() {
                        needs_rebuild = true;
                    }
                }
            }
            if let strata_platform::PlatformEvent::MouseUp { x, y, button: _ } = event {
                if rt.scrollbar_drag.is_some() {
                    rt.scrollbar_drag = None;
                    needs_redraw = true;
                }
                rt.pending_window_drag = None;
                let action = rt.dispatch_pointer_up_action(*x, *y);
                if action.needs_rebuild() {
                    needs_rebuild = true;
                } else if action.needs_redraw() {
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
                    let action = rt.dispatch_pointer_move_action(*x, *y);
                    if action.needs_rebuild() {
                        needs_rebuild = true;
                    } else if action.needs_redraw() {
                        needs_redraw = true;
                    }
                    if rt.update_hover(*x, *y) {
                        if rt.hover_is_paint_only() {
                            needs_redraw = true;
                        } else {
                            needs_rebuild = true;
                        }
                    }
                    let is_over_scrollbar = rt.active_scrollbars.iter().any(|bar| {
                        bar.max_scroll_y > 1.0
                            && (*x >= bar.track_bounds.x
                                && *x <= bar.bounds.x + bar.bounds.width + 1.0
                                && *y >= bar.track_bounds.y
                                && *y <= bar.track_bounds.y + bar.track_bounds.height)
                    });
                    let cur = if is_over_scrollbar {
                        strata_platform::window::CursorIcon::Default
                    } else {
                        AppRuntime::hit_test_cursor(&rt.root_blueprint, &rt.layout_root, *x, *y)
                    };
                    strata_platform::gtk_set_cursor(cur);
                }
            }
            if matches!(event, strata_platform::PlatformEvent::CursorLeft) {
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
            ) {
                let res = rt.dispatch_key_event(event);
                if res.needs_rebuild() {
                    needs_rebuild = true;
                } else if res.needs_redraw() {
                    needs_redraw = true;
                }
            }
        }

        if perf_logging_enabled() && needs_rebuild {
            eprintln!("[perf] rebuild_trigger: {:?}", event);
        }

        if needs_rebuild {
            #[cfg(target_os = "linux")]
            if strata_platform::gtk_is_closing() {
                return false;
            }
            if runtime_event.borrow_mut().sync_dynamic_text() {
                true
            } else {
                let new_view = (root_fn_event.borrow_mut())();
                runtime_event.borrow_mut().update_view(new_view);
                true
            }
        } else if needs_redraw {
            #[cfg(target_os = "linux")]
            if strata_platform::gtk_is_closing() {
                return false;
            }
            true
        } else {
            false
        }
    };

    let decorated = config.decoration == strata_platform::WindowDecoration::NativeOs;
    let _ = crate::mcp_bridge::start_mcp_dev_listener(None);
    strata_platform::launch_gtk_window(
        &config.title,
        config.width,
        config.height,
        decorated,
        on_draw,
        on_event,
    )
}


