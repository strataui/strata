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
        strata_platform::PlatformEvent::WindowResized { width: w, height: h } => {
            compositor.resize(*w, *h, runtime.dpr);
            let _ = presenter.resize(*w, *h);
            runtime.viewport_width = *w as f32;
            runtime.viewport_height = *h as f32;
            needs_redraw = true;
        }
        strata_platform::PlatformEvent::ScaleFactorChanged { scale_factor } => {
            compositor.resize(runtime.viewport_width as u32, runtime.viewport_height as u32, *scale_factor);
            runtime.dpr = *scale_factor;
            needs_redraw = true;
        }
        strata_platform::PlatformEvent::Scroll { delta_x, delta_y, .. } => {
            let (px, py) = runtime.last_mouse_pos;
            if runtime.dispatch_smooth_scroll(px, py, *delta_x, *delta_y) {
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
        strata_platform::PlatformEvent::MouseUp { x, y, .. } => {
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
                    AppRuntime::hit_test_cursor(&runtime.root_blueprint, &runtime.layout_root, *x, *y)
                };
                runtime.current_cursor = cur;
            }
        }
        strata_platform::PlatformEvent::CursorLeft => {
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
        | strata_platform::PlatformEvent::ReceivedCharacter(_) => {
            let res = runtime.dispatch_key_event(event);
            if res.needs_rebuild() {
                needs_rebuild = true;
            } else if res.needs_redraw() {
                needs_redraw = true;
            }
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
        if was_active
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
        }
    }
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
    mut event_handler: E,
) -> Result<(), String>
where
    F: FnMut() -> View + 'static,
    E: FnMut(&strata_platform::PlatformEvent, &mut AppRuntime) -> bool + 'static,
{
    println!(
        "[+] Launching Strata Native Desktop Window on X11: \"{}\"...",
        config.title
    );
    let mut x11_window = strata_platform::x11_window::X11Window::new(config.clone())?;
    let view = root_fn();
    let width = config.width as f32;
    let height = config.height as f32;
    let mut runtime = AppRuntime::new(view, width, height, 1.0);
    let mut presenter =
        SoftwareSurfacePresenter::new(config.width, config.height, SwapchainFormat::Bgra8Unorm);
    let mut compositor = SwapchainCompositor::new(config.width, config.height, 1.0);

    // Initial frame presentation
    let _ = runtime.render_frame(&mut compositor, &mut presenter, true);
    let _ = x11_window.present_bgra(&presenter.backing_buffer, presenter.width, presenter.height);

    println!(
        "[OK] Strata Native Desktop Window active on X11 display: {}x{} physical pixels, {} commands presented.",
        presenter.width,
        presenter.height,
        runtime.display_list.commands.len()
    );

    let mut running = true;
    while running {
        let events = x11_window.poll_native_events();
        let mut needs_rebuild = false;
        let mut needs_redraw = false;

        for event in events {
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

        let anim_changed = runtime.poll_animations();
        let scroll_changed = runtime.poll_smooth_scroll();
        let caret_changed = runtime.poll_caret_blink();
        if anim_changed || scroll_changed || caret_changed {
            needs_redraw = true;
        }

        if needs_rebuild {
            if runtime.sync_dynamic_text() {
                let _ = runtime.render_frame(&mut compositor, &mut presenter, false);
                let _ = x11_window.present_bgra(
                    &presenter.backing_buffer,
                    presenter.width,
                    presenter.height,
                );
            } else {
                let new_view = root_fn();
                runtime.update_view(new_view);
                let _ = runtime.render_frame(&mut compositor, &mut presenter, true);
                let _ = x11_window.present_bgra(
                    &presenter.backing_buffer,
                    presenter.width,
                    presenter.height,
                );
            }
        } else if needs_redraw || x11_window.redraw_requested {
            let _ = runtime.render_frame(&mut compositor, &mut presenter, false);
            let _ = x11_window.present_bgra(
                &presenter.backing_buffer,
                presenter.width,
                presenter.height,
            );
        }

        let is_active =
            anim_changed || scroll_changed || caret_changed || needs_rebuild || needs_redraw;
        std::thread::sleep(runtime.compute_idle_sleep(is_active));
    }

    Ok(())
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
    mut event_handler: E,
) -> Result<(), String>
where
    F: FnMut() -> View + 'static,
    E: FnMut(&strata_platform::PlatformEvent, &mut AppRuntime) -> bool + 'static,
{
    println!(
        "[+] Launching Strata Native Desktop Window on Wayland: \"{}\"...",
        config.title
    );
    let mut wayland_window = strata_platform::WaylandWindow::new(config.clone())?;
    let view = root_fn();
    let width = config.width as f32;
    let height = config.height as f32;
    let mut runtime = AppRuntime::new(view, width, height, wayland_window.scale_factor());
    let mut presenter =
        SoftwareSurfacePresenter::new(config.width, config.height, SwapchainFormat::Bgra8Unorm);
    let mut compositor = SwapchainCompositor::new(config.width, config.height, wayland_window.scale_factor());

    let _ = runtime.render_frame(&mut compositor, &mut presenter, true);
    let _ = wayland_window.present_bgra(&presenter.backing_buffer, presenter.width, presenter.height);

    println!(
        "[OK] Strata Native Desktop Window active on Wayland compositor: {}x{} physical pixels, {} commands presented.",
        presenter.width,
        presenter.height,
        runtime.display_list.commands.len()
    );

    let mut running = true;
    while running {
        let events = wayland_window.poll_native_events();
        let mut needs_rebuild = false;
        let mut needs_redraw = false;

        for event in events {
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

        let anim_changed = runtime.poll_animations();
        let scroll_changed = runtime.poll_smooth_scroll();
        let caret_changed = runtime.poll_caret_blink();
        if anim_changed || scroll_changed || caret_changed {
            needs_redraw = true;
        }

        if needs_rebuild {
            if runtime.sync_dynamic_text() {
                let _ = runtime.render_frame(&mut compositor, &mut presenter, false);
                let _ = wayland_window.present_bgra(
                    &presenter.backing_buffer,
                    presenter.width,
                    presenter.height,
                );
            } else {
                let new_view = root_fn();
                runtime.update_view(new_view);
                let _ = runtime.render_frame(&mut compositor, &mut presenter, true);
                let _ = wayland_window.present_bgra(
                    &presenter.backing_buffer,
                    presenter.width,
                    presenter.height,
                );
            }
        } else if needs_redraw || wayland_window.is_redraw_requested() {
            let _ = runtime.render_frame(&mut compositor, &mut presenter, false);
            let _ = wayland_window.present_bgra(
                &presenter.backing_buffer,
                presenter.width,
                presenter.height,
            );
        }

        let is_active =
            anim_changed || scroll_changed || caret_changed || needs_rebuild || needs_redraw;
        std::thread::sleep(runtime.compute_idle_sleep(is_active));
    }

    Ok(())
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
    mut event_handler: E,
) -> Result<(), String>
where
    F: FnMut() -> View + 'static,
    E: FnMut(&strata_platform::PlatformEvent, &mut AppRuntime) -> bool + 'static,
{
    println!(
        "[+] Launching Strata Native Desktop Window on Windows (Win32): \"{}\"...",
        config.title
    );
    let mut win32_window = strata_platform::Win32Window::new(config.clone())?;
    let view = root_fn();
    let width = config.width as f32;
    let height = config.height as f32;
    let mut runtime = AppRuntime::new(view, width, height, win32_window.scale_factor());
    let mut presenter =
        SoftwareSurfacePresenter::new(config.width, config.height, SwapchainFormat::Bgra8Unorm);
    let mut compositor = SwapchainCompositor::new(config.width, config.height, win32_window.scale_factor());

    let _ = runtime.render_frame(&mut compositor, &mut presenter, true);
    let _ = win32_window.present_bgra(&presenter.backing_buffer, presenter.width, presenter.height);

    println!(
        "[OK] Strata Native Desktop Window active on Win32: {}x{} physical pixels, {} commands presented.",
        presenter.width,
        presenter.height,
        runtime.display_list.commands.len()
    );

    let mut running = true;
    while running {
        let events = win32_window.poll_native_events();
        let mut needs_rebuild = false;
        let mut needs_redraw = false;

        for event in events {
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

        let anim_changed = runtime.poll_animations();
        let scroll_changed = runtime.poll_smooth_scroll();
        let caret_changed = runtime.poll_caret_blink();
        if anim_changed || scroll_changed || caret_changed {
            needs_redraw = true;
        }

        if needs_rebuild {
            if runtime.sync_dynamic_text() {
                let _ = runtime.render_frame(&mut compositor, &mut presenter, false);
                let _ = win32_window.present_bgra(
                    &presenter.backing_buffer,
                    presenter.width,
                    presenter.height,
                );
            } else {
                let new_view = root_fn();
                runtime.update_view(new_view);
                let _ = runtime.render_frame(&mut compositor, &mut presenter, true);
                let _ = win32_window.present_bgra(
                    &presenter.backing_buffer,
                    presenter.width,
                    presenter.height,
                );
            }
        } else if needs_redraw || win32_window.is_redraw_requested() {
            let _ = runtime.render_frame(&mut compositor, &mut presenter, false);
            let _ = win32_window.present_bgra(
                &presenter.backing_buffer,
                presenter.width,
                presenter.height,
            );
        }

        let is_active =
            anim_changed || scroll_changed || caret_changed || needs_rebuild || needs_redraw;
        std::thread::sleep(runtime.compute_idle_sleep(is_active));
    }

    Ok(())
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
    mut event_handler: E,
) -> Result<(), String>
where
    F: FnMut() -> View + 'static,
    E: FnMut(&strata_platform::PlatformEvent, &mut AppRuntime) -> bool + 'static,
{
    println!(
        "[+] Launching Strata Native Desktop Window on macOS (Cocoa): \"{}\"...",
        config.title
    );
    let mut macos_window = strata_platform::MacOsWindow::new(config.clone())?;
    let view = root_fn();
    let width = config.width as f32;
    let height = config.height as f32;
    let mut runtime = AppRuntime::new(view, width, height, macos_window.scale_factor());
    let mut presenter =
        SoftwareSurfacePresenter::new(config.width, config.height, SwapchainFormat::Bgra8Unorm);
    let mut compositor = SwapchainCompositor::new(config.width, config.height, macos_window.scale_factor());

    let _ = runtime.render_frame(&mut compositor, &mut presenter, true);
    let _ = macos_window.present_bgra(&presenter.backing_buffer, presenter.width, presenter.height);

    println!(
        "[OK] Strata Native Desktop Window active on macOS Cocoa: {}x{} physical pixels, {} commands presented.",
        presenter.width,
        presenter.height,
        runtime.display_list.commands.len()
    );

    let mut running = true;
    while running {
        let events = macos_window.poll_native_events();
        let mut needs_rebuild = false;
        let mut needs_redraw = false;

        for event in events {
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

        let anim_changed = runtime.poll_animations();
        let scroll_changed = runtime.poll_smooth_scroll();
        let caret_changed = runtime.poll_caret_blink();
        if anim_changed || scroll_changed || caret_changed {
            needs_redraw = true;
        }

        if needs_rebuild {
            if runtime.sync_dynamic_text() {
                let _ = runtime.render_frame(&mut compositor, &mut presenter, false);
                let _ = macos_window.present_bgra(
                    &presenter.backing_buffer,
                    presenter.width,
                    presenter.height,
                );
            } else {
                let new_view = root_fn();
                runtime.update_view(new_view);
                let _ = runtime.render_frame(&mut compositor, &mut presenter, true);
                let _ = macos_window.present_bgra(
                    &presenter.backing_buffer,
                    presenter.width,
                    presenter.height,
                );
            }
        } else if needs_redraw || macos_window.is_redraw_requested() {
            let _ = runtime.render_frame(&mut compositor, &mut presenter, false);
            let _ = macos_window.present_bgra(
                &presenter.backing_buffer,
                presenter.width,
                presenter.height,
            );
        }

        let is_active =
            anim_changed || scroll_changed || caret_changed || needs_rebuild || needs_redraw;
        std::thread::sleep(runtime.compute_idle_sleep(is_active));
    }

    Ok(())
}

#[cfg(target_os = "android")]
/// Launches an interactive native Android application window via Android NDK NativeActivity.
pub fn launch_android<F>(native_window: *mut std::ffi::c_void, root_fn: F, config: WindowConfig) -> Result<(), String>
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
    mut event_handler: E,
) -> Result<(), String>
where
    F: FnMut() -> View + 'static,
    E: FnMut(&strata_platform::PlatformEvent, &mut AppRuntime) -> bool + 'static,
{
    let mut android_window = strata_platform::AndroidWindow::new(native_window, config.clone())?;
    let view = root_fn();
    let width = config.width as f32;
    let height = config.height as f32;
    let mut runtime = AppRuntime::new(view, width, height, android_window.scale_factor());
    let mut presenter =
        SoftwareSurfacePresenter::new(config.width, config.height, SwapchainFormat::Bgra8Unorm);
    let mut compositor = SwapchainCompositor::new(config.width, config.height, android_window.scale_factor());

    let _ = runtime.render_frame(&mut compositor, &mut presenter, true);
    let _ = android_window.present_bgra(&presenter.backing_buffer, presenter.width, presenter.height);

    let mut running = true;
    while running {
        let events = android_window.poll_events();
        let mut needs_rebuild = false;
        let mut needs_redraw = false;

        for event in events {
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

        let anim_changed = runtime.poll_animations();
        let scroll_changed = runtime.poll_smooth_scroll();
        let caret_changed = runtime.poll_caret_blink();
        if anim_changed || scroll_changed || caret_changed {
            needs_redraw = true;
        }

        if needs_rebuild {
            if runtime.sync_dynamic_text() {
                let _ = runtime.render_frame(&mut compositor, &mut presenter, false);
                let _ = android_window.present_bgra(
                    &presenter.backing_buffer,
                    presenter.width,
                    presenter.height,
                );
            } else {
                let new_view = root_fn();
                runtime.update_view(new_view);
                let _ = runtime.render_frame(&mut compositor, &mut presenter, true);
                let _ = android_window.present_bgra(
                    &presenter.backing_buffer,
                    presenter.width,
                    presenter.height,
                );
            }
        } else if needs_redraw || android_window.is_redraw_requested() {
            let _ = runtime.render_frame(&mut compositor, &mut presenter, false);
            let _ = android_window.present_bgra(
                &presenter.backing_buffer,
                presenter.width,
                presenter.height,
            );
        }

        let is_active =
            anim_changed || scroll_changed || caret_changed || needs_rebuild || needs_redraw;
        std::thread::sleep(runtime.compute_idle_sleep(is_active));
    }

    Ok(())
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
    mut event_handler: E,
) -> Result<(), String>
where
    F: FnMut() -> View + 'static,
    E: FnMut(&strata_platform::PlatformEvent, &mut AppRuntime) -> bool + 'static,
{
    let mut ios_window = strata_platform::IosWindow::new(config.clone())?;
    let view = root_fn();
    let width = config.width as f32;
    let height = config.height as f32;
    let mut runtime = AppRuntime::new(view, width, height, ios_window.scale_factor());
    let mut presenter =
        SoftwareSurfacePresenter::new(config.width, config.height, SwapchainFormat::Bgra8Unorm);
    let mut compositor = SwapchainCompositor::new(config.width, config.height, ios_window.scale_factor());

    let _ = runtime.render_frame(&mut compositor, &mut presenter, true);
    let _ = ios_window.present_bgra(&presenter.backing_buffer, presenter.width, presenter.height);

    let mut running = true;
    while running {
        let events = ios_window.poll_events();
        let mut needs_rebuild = false;
        let mut needs_redraw = false;

        for event in events {
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

        let anim_changed = runtime.poll_animations();
        let scroll_changed = runtime.poll_smooth_scroll();
        let caret_changed = runtime.poll_caret_blink();
        if anim_changed || scroll_changed || caret_changed {
            needs_redraw = true;
        }

        if needs_rebuild {
            if runtime.sync_dynamic_text() {
                let _ = runtime.render_frame(&mut compositor, &mut presenter, false);
                let _ = ios_window.present_bgra(
                    &presenter.backing_buffer,
                    presenter.width,
                    presenter.height,
                );
            } else {
                let new_view = root_fn();
                runtime.update_view(new_view);
                let _ = runtime.render_frame(&mut compositor, &mut presenter, true);
                let _ = ios_window.present_bgra(
                    &presenter.backing_buffer,
                    presenter.width,
                    presenter.height,
                );
            }
        } else if needs_redraw || ios_window.is_redraw_requested() {
            let _ = runtime.render_frame(&mut compositor, &mut presenter, false);
            let _ = ios_window.present_bgra(
                &presenter.backing_buffer,
                presenter.width,
                presenter.height,
            );
        }

        let is_active =
            anim_changed || scroll_changed || caret_changed || needs_rebuild || needs_redraw;
        std::thread::sleep(runtime.compute_idle_sleep(is_active));
    }

    Ok(())
}


