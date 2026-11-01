//! Live Application Runtime Orchestrator connecting RSX Blueprints, Style Cascade, Layout, and Rendering.

use strata_dom::{NodeBlueprint, Tag, View};
use strata_layout::prelude::*;
use strata_platform::{
    EventLoop, EventLoopControl, LivePlatformWindow, PlatformEvent, PlatformWindow,
    SoftwareSurfacePresenter, SwapchainCompositor, SwapchainFormat, WindowConfig,
};
use strata_render::compositor::DisplayList;
use strata_render::swapchain::{PresentError, SurfacePresenter};
use strata_style::types::{ComputedStyle, Dimension, Display, FlexDirection};

/// Live running application instance holding active state, layout tree, and display list.
#[derive(Debug)]
pub struct AppRuntime {
    pub root_blueprint: NodeBlueprint,
    pub layout_root: LayoutNode,
    pub display_list: DisplayList,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub dpr: f32,
    pub frame_count: u64,
}

impl AppRuntime {
    /// Creates and mounts a new application runtime from a root view.
    pub fn new(root_view: View, viewport_width: f32, viewport_height: f32, dpr: f32) -> Self {
        let root_blueprint = root_view.root;
        let mut layout_root = Self::build_layout_tree(&root_blueprint);

        layout_root.compute_layout(viewport_width, viewport_height);

        let mut runtime = Self {
            root_blueprint,
            layout_root,
            display_list: DisplayList::new(),
            viewport_width,
            viewport_height,
            dpr,
            frame_count: 0,
        };

        runtime.compile_display_list();
        runtime
    }

    /// Re-evaluates layout and compiles display commands for the current frame.
    pub fn tick_frame(&mut self) -> &DisplayList {
        self.frame_count += 1;
        self.layout_root.compute_layout(self.viewport_width, self.viewport_height);
        self.compile_display_list();
        &self.display_list
    }

    /// Recursively transforms an RSX `NodeBlueprint` into a `LayoutNode` hierarchy with styling and inheritance.
    pub fn build_layout_tree(blueprint: &NodeBlueprint) -> LayoutNode {
        Self::build_layout_tree_with_parent(blueprint, None)
    }

    /// Internal recursive tree builder passing parent ComputedStyle for W3C CSS property inheritance.
    pub fn build_layout_tree_with_parent(
        blueprint: &NodeBlueprint,
        parent_style: Option<&ComputedStyle>,
    ) -> LayoutNode {
        match blueprint {
            NodeBlueprint::Element {
                tag,
                classes,
                children,
                ..
            } => {
                let mut style = match parent_style {
                    Some(p) => ComputedStyle::inherit_from(p),
                    None => ComputedStyle::default(),
                };
                Self::apply_tailwind_classes(&mut style, classes);

                // Default display based on tag if not explicitly set by classes
                let has_display_class = classes.iter().any(|c| {
                    c.split_whitespace().any(|tok| {
                        matches!(
                            tok,
                            "flex"
                                | "grid"
                                | "block"
                                | "inline-flex"
                                | "inline-block"
                                | "inline"
                                | "hidden"
                        )
                    })
                });

                if !has_display_class {
                    match tag {
                        Tag::Div
                        | Tag::Main
                        | Tag::Section
                        | Tag::Header
                        | Tag::Footer
                        | Tag::Nav
                        | Tag::Aside => {
                            style.display = Display::Flex;
                            style.flex_direction = FlexDirection::Column;
                        }
                        Tag::Button => {
                            style.display = Display::InlineFlex;
                            style.height = Dimension::Px(40.0);
                        }
                        Tag::Span => {
                            style.display = Display::InlineFlex;
                        }
                        Tag::P | Tag::H1 | Tag::H2 | Tag::H3 | Tag::H4 | Tag::H5 | Tag::H6 => {
                            style.display = Display::InlineBlock;
                        }
                        _ => {
                            style.display = Display::Flex;
                        }
                    }
                }

                // If element has auto width and contains only a text child, compute intrinsic text dimensions
                if style.width == Dimension::Auto && children.len() == 1 {
                    if let NodeBlueprint::Text(t) = &children[0] {
                        let char_w = style.font_size * 0.55;
                        let text_w = t.len() as f32 * char_w;
                        let total_w = text_w
                            + style.padding.left
                            + style.padding.right
                            + style.border_width.left
                            + style.border_width.right;
                        style.width = Dimension::Px(total_w);
                    }
                }

                let mut child_nodes = Vec::with_capacity(children.len());
                for child in children {
                    child_nodes.push(Self::build_layout_tree_with_parent(child, Some(&style)));
                }

                LayoutNode::with_children(style, child_nodes)
            }
            NodeBlueprint::Text(text) => {
                let mut style = match parent_style {
                    Some(p) => ComputedStyle::inherit_from(p),
                    None => ComputedStyle::default(),
                };
                style.display = Display::Block;
                let char_w = style.font_size * 0.55;
                let approx_width = (text.len() as f32 * char_w).max(16.0);
                let line_h = (style.font_size * style.line_height).max(style.font_size);
                style.width = Dimension::Px(approx_width);
                style.height = Dimension::Px(line_h);
                LayoutNode::with_text(style, text.clone())
            }
            NodeBlueprint::DynamicText(dyn_fn) => {
                let text = dyn_fn();
                let mut style = match parent_style {
                    Some(p) => ComputedStyle::inherit_from(p),
                    None => ComputedStyle::default(),
                };
                style.display = Display::Block;
                let char_w = style.font_size * 0.55;
                let approx_width = (text.len() as f32 * char_w).max(16.0);
                let line_h = (style.font_size * style.line_height).max(style.font_size);
                style.width = Dimension::Px(approx_width);
                style.height = Dimension::Px(line_h);
                LayoutNode::with_text(style, text)
            }
            NodeBlueprint::Fragment(children) => {
                let mut style = match parent_style {
                    Some(p) => ComputedStyle::inherit_from(p),
                    None => ComputedStyle::default(),
                };
                style.display = Display::Flex;
                style.flex_direction = FlexDirection::Column;

                let mut child_nodes = Vec::with_capacity(children.len());
                for child in children {
                    child_nodes.push(Self::build_layout_tree_with_parent(child, Some(&style)));
                }

                LayoutNode::with_children(style, child_nodes)
            }
            NodeBlueprint::Empty => {
                let mut style = ComputedStyle::default();
                style.display = Display::None;
                LayoutNode::new(style)
            }
        }
    }

    /// Full Tailwind CSS class parser using strata_style's Tailwind JIT compiler.
    fn apply_tailwind_classes(style: &mut ComputedStyle, classes: &[String]) {
        for cls in classes {
            for single_class in cls.split_whitespace() {
                strata_style::tailwind::TailwindEngine::apply_class(style, single_class);
            }
        }
    }

    /// Generates draw commands for all computed layout rectangles using the 7-layer CSS compositor.
    fn compile_display_list(&mut self) {
        self.display_list = strata_render::compositor::Compositor::build_display_list(&self.layout_root);
    }

    /// Processes an incoming platform event, updating viewport/scale or dispatching synthetic events.
    /// Returns true if a dirty layout or presentation redraw was triggered.
    pub fn step_event(&mut self, event: &PlatformEvent) -> bool {
        match event {
            PlatformEvent::WindowResized { width, height } => {
                self.viewport_width = *width as f32;
                self.viewport_height = *height as f32;
                self.tick_frame();
                true
            }
            PlatformEvent::ScaleFactorChanged { scale_factor } => {
                self.dpr = *scale_factor;
                self.tick_frame();
                true
            }
            PlatformEvent::WindowFocused(_) => false,
            PlatformEvent::WindowCloseRequested => false,
            _ => false,
        }
    }

    /// Renders the current frame and presents it to the given surface presenter via the swapchain compositor.
    pub fn render_frame<P: SurfacePresenter>(
        &mut self,
        compositor: &mut SwapchainCompositor,
        presenter: &mut P,
        force_redraw: bool,
    ) -> Result<bool, PresentError> {
        self.tick_frame();
        compositor.render_and_present(&self.display_list, presenter, force_redraw)
    }

    /// Runs the reactive event loop against a live platform window and surface presenter.
    pub fn run_with_window<W, E, P>(
        &mut self,
        window: &mut W,
        event_loop: &mut E,
        compositor: &mut SwapchainCompositor,
        presenter: &mut P,
    ) where
        W: PlatformWindow,
        E: EventLoop,
        P: SurfacePresenter,
    {
        // Initial frame presentation
        let _ = self.render_frame(compositor, presenter, true);

        event_loop.run(|event, control| {
            match event {
                PlatformEvent::WindowResized { width, height } => {
                    compositor.resize(width, height, self.dpr);
                    let _ = presenter.resize(width, height);
                }
                PlatformEvent::ScaleFactorChanged { scale_factor } => {
                    compositor.resize(self.viewport_width as u32, self.viewport_height as u32, scale_factor);
                }
                PlatformEvent::WindowCloseRequested => {
                    *control = EventLoopControl::Exit;
                    return;
                }
                _ => {}
            }

            let needs_redraw = self.step_event(&event);

            if needs_redraw || window.is_redraw_requested() {
                let _ = self.render_frame(compositor, presenter, false);
            }

            // Power-saving sleep mode: if no events pending, wait for next input
            *control = EventLoopControl::Wait;
        });
    }
}

/// Creates a fluent low-level Strata application builder.
pub fn app() -> StrataAppBuilder {
    StrataApp::builder()
}

/// Launches a live Strata application with the specified root component.
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
pub fn launch_window<F>(root: F, config: WindowConfig) -> (AppRuntime, LivePlatformWindow, SoftwareSurfacePresenter)
where
    F: FnOnce() -> View + 'static,
{
    println!("[+] Launching Strata Live Native Window: \"{}\"...", config.title);
    let view = root();
    let width = config.width as f32;
    let height = config.height as f32;
    let mut runtime = AppRuntime::new(view, width, height, 1.0);

    let window = LivePlatformWindow::new(config.clone());
    let mut presenter = SoftwareSurfacePresenter::new(config.width, config.height, SwapchainFormat::Bgra8Unorm);
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
pub struct AppLifecycleHooks {
    pub on_start: Option<Box<dyn FnMut(&mut AppRuntime)>>,
    pub on_event: Option<Box<dyn FnMut(&PlatformEvent, &mut EventLoopControl, &mut AppRuntime)>>,
    pub on_frame: Option<Box<dyn FnMut(AppFrameInfo, &mut AppRuntime)>>,
    pub on_draw: Option<Box<dyn FnMut(&mut DisplayList)>>,
    pub on_exit: Option<Box<dyn FnMut(&mut AppRuntime)>>,
}

impl Default for AppLifecycleHooks {
    fn default() -> Self {
        Self {
            on_start: None,
            on_event: None,
            on_frame: None,
            on_draw: None,
            on_exit: None,
        }
    }
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
        self.compositor.render_and_present(&self.runtime.display_list, &mut self.presenter, true)?;
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
        println!("[+] Launching Strata Native App: \"{}\"...", self.window_config.title);
        let window = LivePlatformWindow::new(self.window_config.clone());

        if let Some(ref mut on_start) = self.hooks.on_start {
            on_start(&mut self.runtime);
        }

        let _ = self.runtime.render_frame(&mut self.compositor, &mut self.presenter, true);

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
                    compositor.resize(runtime.viewport_width as u32, runtime.viewport_height as u32, scale_factor);
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

    /// Runs the application using the default MockEventLoop.
    pub fn run(self) {
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

    pub fn on_event<F: FnMut(&PlatformEvent, &mut EventLoopControl, &mut AppRuntime) + 'static>(mut self, cb: F) -> Self {
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
        let presenter = SoftwareSurfacePresenter::new(self.window_config.width, self.window_config.height, SwapchainFormat::Bgra8Unorm);
        let compositor = SwapchainCompositor::new(self.window_config.width, self.window_config.height, self.dpr);

        StrataApp {
            runtime,
            window_config: self.window_config,
            hooks: self.hooks,
            compositor,
            presenter,
        }
    }
}
