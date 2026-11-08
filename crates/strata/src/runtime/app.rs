//! High-level declarative application entry point and builder.

use super::*;

pub fn app() -> StrataAppBuilder {
    StrataApp::builder()
}
/// Frame timing and metrics information passed to frame lifecycle callbacks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AppFrameInfo {
    pub frame_index: u64,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub dpr: f32,
    pub command_count: usize,
}

/// Low-level lifecycle callbacks.
#[derive(Default)]
pub struct AppLifecycleHooks {
    pub on_start: Option<Box<dyn FnMut(&mut AppRuntime)>>,
    pub on_event: Option<Box<dyn FnMut(&PlatformEvent, &mut EventLoopControl, &mut AppRuntime)>>,
    pub on_frame: Option<Box<dyn FnMut(AppFrameInfo, &mut AppRuntime)>>,
    pub on_draw: Option<Box<dyn FnMut(&mut DisplayList)>>,
    pub on_exit: Option<Box<dyn FnMut(&mut AppRuntime)>>,
}


/// Low-level application orchestrator with fine-grained control over windowing, swapchain presentation,
/// lifecycle hooks, and manual frame pacing.
pub struct StrataApp {
    pub runtime: AppRuntime,
    pub window_config: WindowConfig,
    pub hooks: AppLifecycleHooks,
    pub compositor: SwapchainCompositor,
    pub presenter: SoftwareSurfacePresenter,
}

impl StrataApp {
    /// Creates a fluent low-level builder for configuring and launching a Strata app.
    pub fn builder() -> StrataAppBuilder {
        StrataAppBuilder::new()
    }

    /// Advances the runtime by a single frame and returns the generated display commands.
    pub fn step_frame(&mut self) -> &DisplayList {
        self.runtime.tick_frame();
        if let Some(ref mut on_draw) = self.hooks.on_draw {
            on_draw(&mut self.runtime.display_list);
        }
        &self.runtime.display_list
    }

    /// Dispatches a platform event into the runtime and lifecycle hooks.
    pub fn dispatch_event(&mut self, event: &PlatformEvent) -> bool {
        let mut control = EventLoopControl::Poll;
        if let Some(ref mut on_event) = self.hooks.on_event {
            on_event(event, &mut control, &mut self.runtime);
        }
        self.runtime.step_event(event)
    }

    /// Renders current frame to internal software surface buffer and returns immutable slice of BGRA pixels.
    pub fn render_to_buffer(&mut self) -> Result<&[u8], PresentError> {
        self.step_frame();
        self.compositor.render_and_present(
            &self.runtime.display_list,
            &mut self.presenter,
            true,
        )?;
        Ok(&self.presenter.backing_buffer)
    }

    /// Runs the application headlessly for a specified number of frames.
    pub fn run_headless(&mut self, frame_count: u64) -> Vec<AppFrameInfo> {
        if let Some(ref mut on_start) = self.hooks.on_start {
            on_start(&mut self.runtime);
        }
        let mut metrics = Vec::with_capacity(frame_count as usize);
        for _ in 0..frame_count {
            self.step_frame();
            let info = AppFrameInfo {
                frame_index: self.runtime.frame_count,
                viewport_width: self.runtime.viewport_width,
                viewport_height: self.runtime.viewport_height,
                dpr: self.runtime.dpr,
                command_count: self.runtime.display_list.commands.len(),
            };
            if let Some(ref mut on_frame) = self.hooks.on_frame {
                on_frame(info, &mut self.runtime);
            }
            metrics.push(info);
        }
        if let Some(ref mut on_exit) = self.hooks.on_exit {
            on_exit(&mut self.runtime);
        }
        metrics
    }

    /// Runs the platform window event loop with surface presentation.
    pub fn run_with_event_loop<E: EventLoop>(mut self, event_loop: &mut E) {
        println!(
            "[+] Launching Strata Native App: \"{}\"...",
            self.window_config.title
        );
        let window = LivePlatformWindow::new(self.window_config.clone());

        if let Some(ref mut on_start) = self.hooks.on_start {
            on_start(&mut self.runtime);
        }

        let _ = self
            .runtime
            .render_frame(&mut self.compositor, &mut self.presenter, true);

        let hooks = &mut self.hooks;
        let runtime = &mut self.runtime;
        let compositor = &mut self.compositor;
        let presenter = &mut self.presenter;

        event_loop.run(|event, control| {
            if let Some(ref mut on_event) = hooks.on_event {
                on_event(&event, control, runtime);
            }

            match event {
                PlatformEvent::WindowResized { width, height } => {
                    compositor.resize(width, height, runtime.dpr);
                    let _ = presenter.resize(width, height);
                }
                PlatformEvent::ScaleFactorChanged { scale_factor } => {
                    compositor.resize(
                        runtime.viewport_width as u32,
                        runtime.viewport_height as u32,
                        scale_factor,
                    );
                }
                PlatformEvent::WindowCloseRequested => {
                    *control = EventLoopControl::Exit;
                    return;
                }
                _ => {}
            }

            let needs_redraw = runtime.step_event(&event);

            if needs_redraw || window.is_redraw_requested() {
                if let Some(ref mut on_draw) = hooks.on_draw {
                    on_draw(&mut runtime.display_list);
                }
                let _ = runtime.render_frame(compositor, presenter, false);
                if let Some(ref mut on_frame) = hooks.on_frame {
                    let info = AppFrameInfo {
                        frame_index: runtime.frame_count,
                        viewport_width: runtime.viewport_width,
                        viewport_height: runtime.viewport_height,
                        dpr: runtime.dpr,
                        command_count: runtime.display_list.commands.len(),
                    };
                    on_frame(info, runtime);
                }
            }

            *control = EventLoopControl::Wait;
        });

        if let Some(ref mut on_exit) = self.hooks.on_exit {
            on_exit(&mut self.runtime);
        }
    }

    /// Runs the application using the native platform event loop (Wayland/X11 on Linux,
    /// Win32 on Windows, Cocoa on macOS), or MockEventLoop on headless/mock targets.
    pub fn run(self) {
        #[cfg(target_os = "linux")]
        {
            if std::env::var("WAYLAND_DISPLAY").is_ok() {
                if let Ok(window) = strata_platform::WaylandWindow::new(self.window_config.clone()) {
                    let mut event_loop = strata_platform::WaylandEventLoop::new(window);
                    self.run_with_event_loop(&mut event_loop);
                    return;
                }
            }
            if std::env::var("DISPLAY").is_ok() {
                if let Ok(window) = strata_platform::X11Window::new(self.window_config.clone()) {
                    let mut event_loop = strata_platform::X11EventLoop::new(window);
                    self.run_with_event_loop(&mut event_loop);
                    return;
                }
            }
        }
        #[cfg(windows)]
        {
            if let Ok(window) = strata_platform::Win32Window::new(self.window_config.clone()) {
                let mut event_loop = strata_platform::Win32EventLoop::new(window);
                self.run_with_event_loop(&mut event_loop);
                return;
            }
        }
        #[cfg(target_os = "macos")]
        {
            if let Ok(window) = strata_platform::MacOsWindow::new(self.window_config.clone()) {
                let mut event_loop = strata_platform::MacOsEventLoop::new(window);
                self.run_with_event_loop(&mut event_loop);
                return;
            }
        }
        let mut event_loop = strata_platform::MockEventLoop::new();
        self.run_with_event_loop(&mut event_loop);
    }
}

/// Fluent builder for low-level application configuration and initialization.
pub struct StrataAppBuilder {
    window_config: WindowConfig,
    dpr: f32,
    hooks: AppLifecycleHooks,
}

impl Default for StrataAppBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl StrataAppBuilder {
    pub fn new() -> Self {
        Self {
            window_config: WindowConfig::default(),
            dpr: 1.0,
            hooks: AppLifecycleHooks::default(),
        }
    }

    pub fn with_window(mut self, config: WindowConfig) -> Self {
        self.window_config = config;
        self
    }

    pub fn with_dpr(mut self, dpr: f32) -> Self {
        self.dpr = dpr;
        self
    }

    pub fn on_start<F: FnMut(&mut AppRuntime) + 'static>(mut self, cb: F) -> Self {
        self.hooks.on_start = Some(Box::new(cb));
        self
    }

    pub fn on_event<F: FnMut(&PlatformEvent, &mut EventLoopControl, &mut AppRuntime) + 'static>(
        mut self,
        cb: F,
    ) -> Self {
        self.hooks.on_event = Some(Box::new(cb));
        self
    }

    pub fn on_frame<F: FnMut(AppFrameInfo, &mut AppRuntime) + 'static>(mut self, cb: F) -> Self {
        self.hooks.on_frame = Some(Box::new(cb));
        self
    }

    pub fn on_draw<F: FnMut(&mut DisplayList) + 'static>(mut self, cb: F) -> Self {
        self.hooks.on_draw = Some(Box::new(cb));
        self
    }

    pub fn on_exit<F: FnMut(&mut AppRuntime) + 'static>(mut self, cb: F) -> Self {
        self.hooks.on_exit = Some(Box::new(cb));
        self
    }

    pub fn build<F: FnOnce() -> View>(self, root: F) -> StrataApp {
        let view = root();
        let width = self.window_config.width as f32;
        let height = self.window_config.height as f32;
        let runtime = AppRuntime::new(view, width, height, self.dpr);
        let presenter = SoftwareSurfacePresenter::new(
            self.window_config.width,
            self.window_config.height,
            SwapchainFormat::Bgra8Unorm,
        );
        let compositor = SwapchainCompositor::new(
            self.window_config.width,
            self.window_config.height,
            self.dpr,
        );

        StrataApp {
            runtime,
            window_config: self.window_config,
            hooks: self.hooks,
            compositor,
            presenter,
        }
    }
}

