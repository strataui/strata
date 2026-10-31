//! Live Application Runtime Orchestrator connecting RSX Blueprints, Style Cascade, Layout, and Rendering.

use strata_dom::{NodeBlueprint, Tag, View};
use strata_layout::prelude::*;
use strata_platform::{
    EventLoop, EventLoopControl, LivePlatformWindow, PlatformEvent, PlatformWindow,
    SoftwareSurfacePresenter, SwapchainCompositor, SwapchainFormat, WindowConfig,
};
use strata_render::compositor::{DisplayList, DrawCommand};
use strata_render::swapchain::{PresentError, SurfacePresenter};
use strata_style::types::{Color, ComputedStyle, Dimension, Display, FlexDirection};

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

    /// Recursively transforms an RSX `NodeBlueprint` into a `LayoutNode` hierarchy with styling.
    pub fn build_layout_tree(blueprint: &NodeBlueprint) -> LayoutNode {
        match blueprint {
            NodeBlueprint::Element {
                tag,
                classes,
                children,
                ..
            } => {
                let mut style = ComputedStyle::default();
                Self::apply_tailwind_classes(&mut style, classes);

                // Default display based on tag if not explicitly set by classes
                if classes.is_empty() {
                    match tag {
                        Tag::Div | Tag::Main | Tag::Section | Tag::Header | Tag::Footer | Tag::Nav | Tag::Aside => {
                            style.display = Display::Flex;
                            style.flex_direction = FlexDirection::Column;
                        }
                        Tag::Button => {
                            style.display = Display::Flex;
                            style.height = Dimension::Px(40.0);
                        }
                        Tag::Span | Tag::P | Tag::H1 | Tag::H2 | Tag::H3 | Tag::H4 | Tag::H5 | Tag::H6 => {
                            style.display = Display::Block;
                        }
                        _ => {
                            style.display = Display::Flex;
                        }
                    }
                }

                let mut child_nodes = Vec::with_capacity(children.len());
                for child in children {
                    child_nodes.push(Self::build_layout_tree(child));
                }

                LayoutNode::with_children(style, child_nodes)
            }
            NodeBlueprint::Text(text) => {
                let mut style = ComputedStyle::default();
                style.display = Display::Block;
                // Intrinsic size heuristic for text: ~8px per character width, 20px line height
                let approx_width = (text.len() as f32 * 8.0).max(16.0);
                style.width = Dimension::Px(approx_width);
                style.height = Dimension::Px(20.0);
                LayoutNode::new(style)
            }
            NodeBlueprint::DynamicText(dyn_fn) => {
                let text = dyn_fn();
                let mut style = ComputedStyle::default();
                style.display = Display::Block;
                let approx_width = (text.len() as f32 * 8.0).max(16.0);
                style.width = Dimension::Px(approx_width);
                style.height = Dimension::Px(20.0);
                LayoutNode::new(style)
            }
            NodeBlueprint::Fragment(children) => {
                let mut style = ComputedStyle::default();
                style.display = Display::Flex;
                style.flex_direction = FlexDirection::Column;

                let mut child_nodes = Vec::with_capacity(children.len());
                for child in children {
                    child_nodes.push(Self::build_layout_tree(child));
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

    /// Basic fast Tailwind class parser mapping common classes to layout dimensions and display modes.
    fn apply_tailwind_classes(style: &mut ComputedStyle, classes: &[String]) {
        for cls in classes {
            match cls.as_str() {
                "flex" => style.display = Display::Flex,
                "grid" => style.display = Display::Grid,
                "block" => style.display = Display::Block,
                "hidden" => style.display = Display::None,
                "flex-row" => style.flex_direction = FlexDirection::Row,
                "flex-col" => style.flex_direction = FlexDirection::Column,
                "flex-1" => style.flex_grow = 1.0,
                "h-screen" => style.height = Dimension::Percent(100.0),
                "w-screen" | "w-full" => style.width = Dimension::Percent(100.0),
                "h-full" => style.height = Dimension::Percent(100.0),
                "overflow-hidden" => {}
                _ => {}
            }
        }
    }

    /// Generates draw commands for all computed layout rectangles.
    fn compile_display_list(&mut self) {
        self.display_list.clear();
        Self::emit_display_commands(&self.layout_root, &mut self.display_list);
    }

    fn emit_display_commands(node: &LayoutNode, dl: &mut DisplayList) {
        let rect = node.geometry.border_box;
        if rect.width > 0.0 && rect.height > 0.0 {
            dl.push(DrawCommand::DrawRect {
                bounds: rect,
                color: Color::rgba(25, 25, 25, 255), // Neutral base
            });
        }

        for child in &node.children {
            Self::emit_display_commands(child, dl);
        }
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
