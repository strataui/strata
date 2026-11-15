//! Native windowing backends for X11, Wayland, Win32, and macOS Cocoa.

use super::*;

/// Dispatches a native platform event into the AppRuntime, updating geometry, focus, input, and styling.
/// Returns `(needs_rebuild, needs_redraw, should_exit)`.
pub fn dispatch_native_platform_event<E>(
    event: &strata_platform::PlatformEvent,
    runtime: &mut AppRuntime,
    compositor: &mut SwapchainCompositor,
    presenter: &mut SoftwareSurfacePresenter,
    event_handler: &mut E,
) -> (bool, bool, bool)
where
    E: FnMut(&strata_platform::PlatformEvent, &mut AppRuntime) -> bool,
{
    let mut needs_rebuild = false;
    let mut needs_redraw = false;
    let mut should_exit = false;

    if event_handler(event, runtime) {
        needs_rebuild = true;
    }

    match event {
        strata_platform::PlatformEvent::WindowCloseRequested => {
            should_exit = true;
        }
        strata_platform::PlatformEvent::WindowResized {
            width: w,
            height: h,
        } => {
            compositor.resize(*w, *h, runtime.dpr);
            let _ = presenter.resize(*w, *h);
            runtime.viewport_width = *w as f32;
            runtime.viewport_height = *h as f32;
            needs_redraw = true;
        }
        strata_platform::PlatformEvent::ScaleFactorChanged { scale_factor } => {
            compositor.resize(
                runtime.viewport_width as u32,
                runtime.viewport_height as u32,
                *scale_factor,
            );
            runtime.dpr = *scale_factor;
            needs_redraw = true;
        }
        strata_platform::PlatformEvent::Scroll {
            delta_x, delta_y, ..
        } => {
            let (px, py) = runtime.last_mouse_pos;
            let (wheel_handled, scroll_changed) =
                runtime.dispatch_smooth_scroll_action(px, py, *delta_x, *delta_y);
            if wheel_handled {
                needs_rebuild = true;
            }
            if scroll_changed {
                needs_redraw = true;
            }
        }
        strata_platform::PlatformEvent::AnimationTick => {
            let scroll_changed = runtime.poll_smooth_scroll();
            let anim_changed = runtime.poll_animations();
            let caret_changed = runtime.poll_caret_blink();
            let mcp_changed = if let Some(ref mut c) = runtime.mcp_cursor {
                c.poll_tick()
            } else {
                false
            };
            if scroll_changed || anim_changed || caret_changed || mcp_changed {
                needs_redraw = true;
            }
        }
        strata_platform::PlatformEvent::DoubleClick { button, x, y } => {
            let _btn_idx = button.to_button_index();
            if *button == strata_platform::MouseButton::Left {
                let region = AppRuntime::hit_test_property(
                    &runtime.root_blueprint,
                    &runtime.layout_root,
                    *x,
                    *y,
                    "data-app-region",
                );
                if region.as_deref() == Some("drag") {
                    needs_rebuild = true;
                }
            }
            if runtime.dispatch_dblclick(*x, *y) {
                needs_rebuild = true;
            }
        }
        strata_platform::PlatformEvent::MouseDown { button, x, y } => {
            let btn_idx = button.to_button_index();
            if *button == strata_platform::MouseButton::Left {
                runtime.is_mouse_down = true;
                if runtime.check_scrollbar_thumb_press(*x, *y) {
                    needs_redraw = true;
                } else {
                    runtime.begin_window_drag_candidate(*x, *y);
                    let action = runtime.dispatch_pointer_down_action(*x, *y);
                    if action.needs_rebuild() {
                        needs_rebuild = true;
                    } else if action.needs_redraw() {
                        needs_redraw = true;
                    }
                    if runtime.dispatch_mouse_click(*x, *y, btn_idx, 1) {
                        needs_rebuild = true;
                    }
                    if runtime.drain_edit_commands() {
                        needs_rebuild = true;
                    }
                    if !needs_rebuild && runtime.restyle_for_active_change() {
                        needs_redraw = true;
                    }
                }
            } else if *button == strata_platform::MouseButton::Middle {
                let action = runtime.dispatch_pointer_down_action(*x, *y);
                if action.needs_rebuild() {
                    needs_rebuild = true;
                } else if action.needs_redraw() {
                    needs_redraw = true;
                }
                if runtime.dispatch_mouse_click(*x, *y, btn_idx, 1) {
                    needs_rebuild = true;
                }
            } else if *button == strata_platform::MouseButton::Right {
                if runtime.dispatch_context_menu(*x, *y) {
                    needs_rebuild = true;
                }
                if runtime.drain_edit_commands() {
                    needs_rebuild = true;
                }
            }
        }
        strata_platform::PlatformEvent::MouseUp { button, x, y } => {
            let was_mouse_down = runtime.is_mouse_down;
            if *button == strata_platform::MouseButton::Left {
                runtime.is_mouse_down = false;
            }
            if runtime.scrollbar_drag.is_some() {
                runtime.scrollbar_drag = None;
                needs_redraw = true;
            }
            runtime.pending_window_drag = None;
            let action = runtime.dispatch_pointer_up_action(*x, *y);
            if action.needs_rebuild() {
                needs_rebuild = true;
            } else if action.needs_redraw() {
                needs_redraw = true;
            }
            if was_mouse_down && !needs_rebuild && runtime.restyle_for_active_change() {
                needs_redraw = true;
            }
        }
        strata_platform::PlatformEvent::MouseMove { x, y } => {
            runtime.last_mouse_pos = (*x, *y);
            if runtime.scrollbar_drag.is_some() {
                if runtime.poll_scrollbar_drag(*x, *y) {
                    needs_redraw = true;
                }
            } else {
                if runtime.poll_window_drag(*x, *y) {
                    needs_rebuild = true;
                }
                let action = runtime.dispatch_pointer_move_action(*x, *y);
                if action.needs_rebuild() {
                    needs_rebuild = true;
                } else if action.needs_redraw() {
                    needs_redraw = true;
                }
                if runtime.update_hover(*x, *y) {
                    if runtime.hover_is_paint_only() {
                        needs_redraw = true;
                    } else {
                        needs_rebuild = true;
                    }
                }
                let is_over_scrollbar = runtime.active_scrollbars.iter().any(|bar| {
                    bar.max_scroll_y > 1.0
                        && (*x >= bar.track_bounds.x
                            && *x <= bar.bounds.x + bar.bounds.width + 1.0
                            && *y >= bar.track_bounds.y
                            && *y <= bar.track_bounds.y + bar.track_bounds.height)
                });
                let cur = if is_over_scrollbar {
                    strata_platform::window::CursorIcon::Default
                } else {
                    AppRuntime::hit_test_cursor(
                        &runtime.root_blueprint,
                        &runtime.layout_root,
                        *x,
                        *y,
                    )
                };
                runtime.current_cursor = cur;
            }
        }
        strata_platform::PlatformEvent::CursorLeft => {
            runtime.is_mouse_down = false;
            runtime.current_cursor = strata_platform::window::CursorIcon::Default;
            if runtime.clear_hover() {
                if runtime.hover_is_paint_only() {
                    needs_redraw = true;
                } else {
                    needs_rebuild = true;
                }
            }
        }
        strata_platform::PlatformEvent::KeyDown { .. }
        | strata_platform::PlatformEvent::KeyUp { .. }
        | strata_platform::PlatformEvent::ReceivedCharacter(_)
        | strata_platform::PlatformEvent::ImePreedit { .. }
        | strata_platform::PlatformEvent::ImeCommit(_) => {
            let res = runtime.dispatch_key_event(event);
            if res.needs_rebuild() {
                needs_rebuild = true;
            } else if res.needs_redraw() {
                needs_redraw = true;
            }
        }
        strata_platform::PlatformEvent::WindowMaximized(_) => {
            needs_rebuild = true;
            needs_redraw = true;
        }
        _ => {}
    }

    (needs_rebuild, needs_redraw, should_exit)
}

impl AppRuntime {
    /// Dynamically computes the appropriate thread sleep duration based on runtime activity state.
    /// Returns:
    /// - 16ms if animations, smooth scrolling, scrollbar dragging, window dragging, active interactions, or dirty signals are active.
    /// - 50ms if a text input currently has active focus (to service responsive caret blinking).
    /// - 200ms if the runtime is completely idle (dropping CPU consumption to ~0%).
    pub fn compute_idle_sleep(&self, was_active: bool) -> std::time::Duration {
        let base = if was_active
            || self.has_active_animations()
            || self.active_smooth_scroll.is_some()
            || self.scrollbar_drag.is_some()
            || self.pending_window_drag.is_some()
            || self.drag_selection_active
            || strata_dom::reactive::has_dirty_signals()
        {
            std::time::Duration::from_millis(16)
        } else if self.active_focus.is_some() {
            std::time::Duration::from_millis(50)
        } else {
            std::time::Duration::from_millis(200)
        };
        if let Some(next_timer) = strata_js::web::time_until_next_timer() {
            base.min(next_timer.max(std::time::Duration::from_millis(1)))
        } else {
            base
        }
    }
}

/// Runs an interactive native window event loop against any NativeSurfaceWindow implementation,
/// pacing frames, evaluating reactive updates, and blitting pixel buffers.
pub fn run_native_surface_window_loop<W, F, E>(
    mut window: W,
    mut root_fn: Option<F>,
    mut runtime: AppRuntime,
    mut compositor: SwapchainCompositor,
    mut presenter: SoftwareSurfacePresenter,
    mut event_handler: E,
    mut hooks: Option<&mut AppLifecycleHooks>,
) -> Result<(), String>
where
    W: strata_platform::NativeSurfaceWindow,
    F: FnMut() -> View + 'static,
    E: FnMut(&strata_platform::PlatformEvent, &mut AppRuntime) -> bool,
{
    // Initial frame presentation
    let _ = runtime.render_frame(&mut compositor, &mut presenter, true);
    let _ = window.present_surface(&presenter.backing_buffer, presenter.width, presenter.height);

    if let Some(ref mut h) = hooks {
        if let Some(ref mut on_start) = h.on_start {
            on_start(&mut runtime);
        }
        if let Some(ref mut on_frame) = h.on_frame {
            let info = AppFrameInfo {
                frame_index: runtime.frame_count,
                viewport_width: runtime.viewport_width,
                viewport_height: runtime.viewport_height,
                dpr: runtime.dpr,
                command_count: runtime.display_list.commands.len(),
                dt: std::time::Duration::ZERO,
            };
            on_frame(info, &mut runtime);
        }
    }

    println!(
        "[OK] Strata Native Window active: {}x{} physical pixels, {} commands presented.",
        presenter.width,
        presenter.height,
        runtime.display_list.commands.len()
    );

    let max_headless_frames = if std::env::var("STRATA_HEADLESS").is_ok() {
        Some(
            std::env::var("STRATA_HEADLESS_EXIT_AFTER_FRAMES")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(1),
        )
    } else {
        None
    };
    let mut frames_presented: u64 = 1;

    let mut running = true;
    while running {
        if let Some(max_f) = max_headless_frames {
            if frames_presented >= max_f {
                break;
            }
        }
        let events = window.poll_events();
        let mut needs_rebuild = false;
        let mut needs_redraw = false;

        for event in events {
            if let Some(ref mut h) = hooks {
                if let Some(ref mut on_ev) = h.on_event {
                    let mut control = strata_platform::EventLoopControl::Poll;
                    on_ev(&event, &mut control, &mut runtime);
                    if control == strata_platform::EventLoopControl::Exit {
                        running = false;
                        break;
                    }
                }
            }

            if let strata_platform::PlatformEvent::DoubleClick { button, x, y } = event {
                if button == strata_platform::MouseButton::Left {
                    let region = AppRuntime::hit_test_property(
                        &runtime.root_blueprint,
                        &runtime.layout_root,
                        x,
                        y,
                        "data-app-region",
                    );
                    let is_drag = region.as_deref() == Some("drag")
                        || runtime.csd_controller.hit_test(
                            runtime.viewport_width,
                            runtime.viewport_height,
                            x,
                            y,
                        ) == strata_platform::CsdHitZone::Titlebar;
                    if is_drag {
                        let is_max = window.toggle_maximize();
                        window.push_event(strata_platform::PlatformEvent::WindowMaximized(is_max));
                        needs_rebuild = true;
                    }
                }
            }

            if let strata_platform::PlatformEvent::MouseMove { x, y } = event {
                if runtime.poll_window_drag(x, y) {
                    window.begin_window_drag(x, y);
                }
            }

            let (reb, red, exit) = dispatch_native_platform_event(
                &event,
                &mut runtime,
                &mut compositor,
                &mut presenter,
                &mut event_handler,
            );
            if reb {
                needs_rebuild = true;
            }
            if red {
                needs_redraw = true;
            }
            if exit {
                running = false;
                break;
            }
        }

        if !running {
            break;
        }

        let anim_changed = runtime.poll_animations();
        let scroll_changed = runtime.poll_smooth_scroll();
        let caret_changed = runtime.poll_caret_blink();
        let timers_fired = strata_js::web::tick_timers() > 0;
        if timers_fired {
            needs_rebuild = true;
            needs_redraw = true;
        }
        if anim_changed || scroll_changed || caret_changed {
            needs_redraw = true;
        }

        if needs_rebuild {
            if runtime.sync_dynamic_text() {
                runtime.mark_paint_dirty();
                let _ = runtime.render_frame(&mut compositor, &mut presenter, false);
                let _ = window.present_surface(
                    &presenter.backing_buffer,
                    presenter.width,
                    presenter.height,
                );
            } else if let Some(ref mut f) = root_fn {
                let new_view = f();
                runtime.update_view(new_view);
                let _ = runtime.render_frame(&mut compositor, &mut presenter, true);
                let _ = window.present_surface(
                    &presenter.backing_buffer,
                    presenter.width,
                    presenter.height,
                );
            }
        } else if needs_redraw || window.is_redraw_requested() {
            // A per-frame draw hook edits the display list, so it forces a fresh compile.
            let has_draw_hook = hooks.as_ref().is_some_and(|h| h.on_draw.is_some());
            if needs_redraw || has_draw_hook {
                runtime.mark_paint_dirty();
            }
            if let Some(ref mut h) = hooks {
                if let Some(ref mut on_draw) = h.on_draw {
                    on_draw(&mut runtime.display_list);
                }
            }
            let _ = runtime.render_frame(&mut compositor, &mut presenter, false);
            let _ = window.present_surface(
                &presenter.backing_buffer,
                presenter.width,
                presenter.height,
            );
            if let Some(ref mut h) = hooks {
                if let Some(ref mut on_frame) = h.on_frame {
                    let info = AppFrameInfo {
                        frame_index: runtime.frame_count,
                        viewport_width: runtime.viewport_width,
                        viewport_height: runtime.viewport_height,
                        dpr: runtime.dpr,
                        command_count: runtime.display_list.commands.len(),
                        dt: std::time::Duration::ZERO,
                    };
                    on_frame(info, &mut runtime);
                }
            }
        }

        frames_presented += 1;
        if max_headless_frames.is_none() {
            let is_active = anim_changed
                || scroll_changed
                || caret_changed
                || timers_fired
                || needs_rebuild
                || needs_redraw;
            std::thread::sleep(runtime.compute_idle_sleep(is_active));
        }
    }

    if let Some(ref mut h) = hooks {
        if let Some(ref mut on_exit) = h.on_exit {
            on_exit(&mut runtime);
        }
    }

    Ok(())
}

#[cfg(target_os = "linux")]
pub fn launch_x11<F>(root_fn: F, config: WindowConfig) -> Result<(), String>
where
    F: FnMut() -> View + 'static,
{
    launch_x11_with_events(root_fn, config, |_, _| false)
}

#[cfg(target_os = "linux")]
/// Launches an interactive native X11 desktop window on Linux with a custom event hook.
pub fn launch_x11_with_events<F, E>(
    mut root_fn: F,
    config: WindowConfig,
    event_handler: E,
) -> Result<(), String>
where
    F: FnMut() -> View + 'static,
    E: FnMut(&strata_platform::PlatformEvent, &mut AppRuntime) -> bool + 'static,
{
    println!(
        "[+] Launching Strata Native Desktop Window on X11: \"{}\"...",
        config.title
    );
    let x11_window = strata_platform::x11_window::X11Window::new(config.clone())?;
    let view = root_fn();
    let width = config.width as f32;
    let height = config.height as f32;
    let runtime = AppRuntime::new(view, width, height, 1.0);
    let presenter =
        SoftwareSurfacePresenter::new(config.width, config.height, SwapchainFormat::Bgra8Unorm);
    let compositor = SwapchainCompositor::new(config.width, config.height, 1.0);

    run_native_surface_window_loop(
        x11_window,
        Some(root_fn),
        runtime,
        compositor,
        presenter,
        event_handler,
        None,
    )
}

#[cfg(target_os = "linux")]
/// Launches an interactive native Wayland desktop window on Linux displaying the Strata application.
pub fn launch_wayland<F>(root_fn: F, config: WindowConfig) -> Result<(), String>
where
    F: FnMut() -> View + 'static,
{
    launch_wayland_with_events(root_fn, config, |_, _| false)
}

#[cfg(target_os = "linux")]
/// Launches an interactive native Wayland desktop window on Linux with a custom event hook.
pub fn launch_wayland_with_events<F, E>(
    mut root_fn: F,
    config: WindowConfig,
    event_handler: E,
) -> Result<(), String>
where
    F: FnMut() -> View + 'static,
    E: FnMut(&strata_platform::PlatformEvent, &mut AppRuntime) -> bool + 'static,
{
    println!(
        "[+] Launching Strata Native Desktop Window on Wayland: \"{}\"...",
        config.title
    );
    let wayland_window = strata_platform::WaylandWindow::new(config.clone())?;
    let view = root_fn();
    let width = config.width as f32;
    let height = config.height as f32;
    let dpr = wayland_window.scale_factor();
    let runtime = AppRuntime::new(view, width, height, dpr);
    let presenter =
        SoftwareSurfacePresenter::new(config.width, config.height, SwapchainFormat::Bgra8Unorm);
    let compositor = SwapchainCompositor::new(config.width, config.height, dpr);

    run_native_surface_window_loop(
        wayland_window,
        Some(root_fn),
        runtime,
        compositor,
        presenter,
        event_handler,
        None,
    )
}

#[cfg(windows)]
/// Launches an interactive native Win32 desktop window on Windows displaying the Strata application.
pub fn launch_win32<F>(root_fn: F, config: WindowConfig) -> Result<(), String>
where
    F: FnMut() -> View + 'static,
{
    launch_win32_with_events(root_fn, config, |_, _| false)
}

#[cfg(windows)]
/// Launches an interactive native Win32 desktop window on Windows with a custom event hook.
pub fn launch_win32_with_events<F, E>(
    mut root_fn: F,
    config: WindowConfig,
    event_handler: E,
) -> Result<(), String>
where
    F: FnMut() -> View + 'static,
    E: FnMut(&strata_platform::PlatformEvent, &mut AppRuntime) -> bool + 'static,
{
    println!(
        "[+] Launching Strata Native Desktop Window on Windows (Win32): \"{}\"...",
        config.title
    );
    let win32_window = strata_platform::Win32Window::new(config.clone())?;
    let view = root_fn();
    let width = config.width as f32;
    let height = config.height as f32;
    let dpr = win32_window.scale_factor();
    let runtime = AppRuntime::new(view, width, height, dpr);
    let presenter =
        SoftwareSurfacePresenter::new(config.width, config.height, SwapchainFormat::Bgra8Unorm);
    let compositor = SwapchainCompositor::new(config.width, config.height, dpr);

    run_native_surface_window_loop(
        win32_window,
        Some(root_fn),
        runtime,
        compositor,
        presenter,
        event_handler,
        None,
    )
}

#[cfg(target_os = "macos")]
/// Launches an interactive native macOS Cocoa desktop window displaying the Strata application.
pub fn launch_macos<F>(root_fn: F, config: WindowConfig) -> Result<(), String>
where
    F: FnMut() -> View + 'static,
{
    launch_macos_with_events(root_fn, config, |_, _| false)
}

#[cfg(target_os = "macos")]
/// Launches an interactive native macOS Cocoa desktop window with a custom event hook.
pub fn launch_macos_with_events<F, E>(
    mut root_fn: F,
    config: WindowConfig,
    event_handler: E,
) -> Result<(), String>
where
    F: FnMut() -> View + 'static,
    E: FnMut(&strata_platform::PlatformEvent, &mut AppRuntime) -> bool + 'static,
{
    println!(
        "[+] Launching Strata Native Desktop Window on macOS (Cocoa): \"{}\"...",
        config.title
    );
    let macos_window = strata_platform::MacOsWindow::new(config.clone())?;
    let view = root_fn();
    let width = config.width as f32;
    let height = config.height as f32;
    let dpr = macos_window.scale_factor();
    let runtime = AppRuntime::new(view, width, height, dpr);
    let presenter =
        SoftwareSurfacePresenter::new(config.width, config.height, SwapchainFormat::Bgra8Unorm);
    let compositor = SwapchainCompositor::new(config.width, config.height, dpr);

    run_native_surface_window_loop(
        macos_window,
        Some(root_fn),
        runtime,
        compositor,
        presenter,
        event_handler,
        None,
    )
}

#[cfg(target_os = "android")]
/// Launches an interactive native Android application window via Android NDK NativeActivity.
pub fn launch_android<F>(
    native_window: *mut std::ffi::c_void,
    root_fn: F,
    config: WindowConfig,
) -> Result<(), String>
where
    F: FnMut() -> View + 'static,
{
    launch_android_with_events(native_window, root_fn, config, |_, _| false)
}

#[cfg(target_os = "android")]
/// Launches an interactive native Android application window with a custom event hook.
pub fn launch_android_with_events<F, E>(
    native_window: *mut std::ffi::c_void,
    mut root_fn: F,
    config: WindowConfig,
    event_handler: E,
) -> Result<(), String>
where
    F: FnMut() -> View + 'static,
    E: FnMut(&strata_platform::PlatformEvent, &mut AppRuntime) -> bool + 'static,
{
    let android_window = strata_platform::AndroidWindow::new(native_window, config.clone())?;
    let view = root_fn();
    let width = config.width as f32;
    let height = config.height as f32;
    let dpr = android_window.scale_factor();
    let runtime = AppRuntime::new(view, width, height, dpr);
    let presenter =
        SoftwareSurfacePresenter::new(config.width, config.height, SwapchainFormat::Bgra8Unorm);
    let compositor = SwapchainCompositor::new(config.width, config.height, dpr);

    run_native_surface_window_loop(
        android_window,
        Some(root_fn),
        runtime,
        compositor,
        presenter,
        event_handler,
        None,
    )
}

#[cfg(target_os = "ios")]
/// Launches an interactive native iOS application window via UIKit and CAMetalLayer.
pub fn launch_ios<F>(root_fn: F, config: WindowConfig) -> Result<(), String>
where
    F: FnMut() -> View + 'static,
{
    launch_ios_with_events(root_fn, config, |_, _| false)
}

#[cfg(target_os = "ios")]
/// Launches an interactive native iOS application window with a custom event hook.
pub fn launch_ios_with_events<F, E>(
    mut root_fn: F,
    config: WindowConfig,
    event_handler: E,
) -> Result<(), String>
where
    F: FnMut() -> View + 'static,
    E: FnMut(&strata_platform::PlatformEvent, &mut AppRuntime) -> bool + 'static,
{
    let ios_window = strata_platform::IosWindow::new(config.clone())?;
    let view = root_fn();
    let width = config.width as f32;
    let height = config.height as f32;
    let dpr = ios_window.scale_factor();
    let runtime = AppRuntime::new(view, width, height, dpr);
    let presenter =
        SoftwareSurfacePresenter::new(config.width, config.height, SwapchainFormat::Bgra8Unorm);
    let compositor = SwapchainCompositor::new(config.width, config.height, dpr);

    run_native_surface_window_loop(
        ios_window,
        Some(root_fn),
        runtime,
        compositor,
        presenter,
        event_handler,
        None,
    )
}
