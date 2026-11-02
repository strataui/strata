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

    /// Updates the application root view, reconstructing the layout tree and compiling a new display list.
    pub fn update_view(&mut self, new_view: View) {
        self.root_blueprint = new_view.root;
        self.layout_root = Self::build_layout_tree(&self.root_blueprint);
        self.tick_frame();
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
                attributes,
                children,
                ..
            } => {
                let mut style = match parent_style {
                    Some(p) => ComputedStyle::inherit_from(p),
                    None => ComputedStyle::default(),
                };
                Self::apply_tailwind_classes(&mut style, classes);

                // 1. Vector SVG handling: extract path data from d attribute or child path
                if matches!(tag, Tag::Svg | Tag::Path) {
                    let mut view_box = (24.0f32, 24.0f32);
                    for attr in attributes {
                        if let strata_dom::Attribute::Property { key, value } = attr {
                            if key == "viewBox" {
                                let parts: Vec<f32> = value
                                    .split(|c: char| c.is_whitespace() || c == ',')
                                    .filter(|s| !s.is_empty())
                                    .filter_map(|s| s.parse::<f32>().ok())
                                    .collect();
                                if parts.len() >= 4 {
                                    view_box = (parts[2], parts[3]);
                                }
                            }
                        }
                    }

                    let svg_items = Self::collect_svg_items_from_blueprint(
                        blueprint,
                        view_box,
                        None,
                        None,
                        2.0,
                        style.color,
                    );

                    if !svg_items.is_empty() {
                        if style.width == Dimension::Auto {
                            style.width = Dimension::Px(16.0);
                        }
                        if style.height == Dimension::Auto {
                            style.height = Dimension::Px(16.0);
                        }
                        return LayoutNode::with_svg_items(style, svg_items);
                    }
                }

                // 2. Input handling: synthesize text from value or placeholder attributes
                if matches!(tag, Tag::Input) {
                    let mut value_text = None;
                    let mut placeholder_text = None;
                    for attr in attributes {
                        if let strata_dom::Attribute::Property { key, value } = attr {
                            if key == "value" && !value.is_empty() {
                                value_text = Some(value.clone());
                            } else if key == "placeholder" && !value.is_empty() {
                                placeholder_text = Some(value.clone());
                            }
                        }
                    }
                    if let Some(val) = value_text {
                        let char_w = style.font_size * 0.55;
                        let approx_width = (val.len() as f32 * char_w).max(40.0);
                        if style.width == Dimension::Auto && style.flex_grow <= 0.0 {
                            style.width = Dimension::Px(approx_width + style.padding.left + style.padding.right);
                        }
                        return LayoutNode::with_text(style, val);
                    } else if let Some(ph) = placeholder_text {
                        let char_w = style.font_size * 0.55;
                        let approx_width = (ph.len() as f32 * char_w).max(40.0);
                        if style.width == Dimension::Auto && style.flex_grow <= 0.0 {
                            style.width = Dimension::Px(approx_width + style.padding.left + style.padding.right);
                        }
                        style.color = strata_style::types::Color::new(140, 145, 155, 255);
                        return LayoutNode::with_text(style, ph);
                    }
                }

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
                            style.align_items = strata_style::types::AlignItems::Center;
                            style.justify_content = strata_style::types::JustifyContent::Center;
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

                let is_inline = matches!(
                    style.display,
                    Display::Inline | Display::InlineFlex | Display::InlineBlock
                ) || matches!(tag, Tag::Span | Tag::Button);

                // If inline element has auto width and contains only a text child, compute intrinsic text dimensions
                if is_inline && style.width == Dimension::Auto && children.len() == 1 {
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
                    Self::append_child_layout_nodes(child, Some(&style), &mut child_nodes);
                }

                LayoutNode::with_children(style, child_nodes)
            }
            NodeBlueprint::Text(text) => {
                let mut style = match parent_style {
                    Some(p) => ComputedStyle::inherit_from(p),
                    None => ComputedStyle::default(),
                };
                style.display = Display::Inline;
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
                style.display = Display::Inline;
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

/// Helper function to extract all SVG vector shapes from a blueprint hierarchy.
fn collect_svg_items_from_blueprint(
    blueprint: &NodeBlueprint,
    default_view_box: (f32, f32),
    inherited_stroke: Option<strata_style::types::Color>,
    inherited_fill: Option<strata_style::types::Color>,
    inherited_stroke_width: f32,
    parent_color: strata_style::types::Color,
) -> Vec<strata_layout::SvgItem> {
    let mut items = Vec::new();
    match blueprint {
        NodeBlueprint::Element {
            attributes,
            children,
            ..
        } => {
            let mut d = None;
            let mut stroke = inherited_stroke;
            let mut fill = inherited_fill;
            let mut stroke_width = inherited_stroke_width;

            let mut x1 = 0.0f32;
            let mut y1 = 0.0f32;
            let mut x2 = 0.0f32;
            let mut y2 = 0.0f32;
            let mut has_line = false;

            let mut cx = 0.0f32;
            let mut cy = 0.0f32;
            let mut r = 0.0f32;
            let mut rx = 0.0f32;
            let mut ry = 0.0f32;
            let mut has_circle = false;
            let mut has_ellipse = false;

            let mut rect_x = 0.0f32;
            let mut rect_y = 0.0f32;
            let mut rect_w = 0.0f32;
            let mut rect_h = 0.0f32;
            let mut rect_rx = 0.0f32;
            let mut has_rect = false;

            for attr in attributes {
                if let strata_dom::Attribute::Property { key, value } = attr {
                    match key.as_str() {
                        "d" => d = Some(value.clone()),
                        "stroke" => {
                            if value == "currentColor" {
                                stroke = Some(parent_color);
                            } else if value == "none" {
                                stroke = None;
                            } else if let Some(c) = strata_style::types::Color::parse_hex(value) {
                                stroke = Some(c);
                            }
                        }
                        "fill" => {
                            if value == "currentColor" {
                                fill = Some(parent_color);
                            } else if value == "none" {
                                fill = None;
                            } else if let Some(c) = strata_style::types::Color::parse_hex(value) {
                                fill = Some(c);
                            }
                        }
                        "stroke-width" => {
                            if let Ok(w) = value.parse::<f32>() {
                                stroke_width = w;
                            }
                        }
                        "x1" => { x1 = value.parse().unwrap_or(0.0); has_line = true; }
                        "y1" => { y1 = value.parse().unwrap_or(0.0); has_line = true; }
                        "x2" => { x2 = value.parse().unwrap_or(0.0); has_line = true; }
                        "y2" => { y2 = value.parse().unwrap_or(0.0); has_line = true; }
                        "cx" => { cx = value.parse().unwrap_or(0.0); }
                        "cy" => { cy = value.parse().unwrap_or(0.0); }
                        "r" => { r = value.parse().unwrap_or(0.0); has_circle = true; }
                        "rx" => {
                            let v = value.parse().unwrap_or(0.0);
                            rx = v;
                            rect_rx = v;
                        }
                        "ry" => { ry = value.parse().unwrap_or(0.0); has_ellipse = true; }
                        "x" => { rect_x = value.parse().unwrap_or(0.0); }
                        "y" => { rect_y = value.parse().unwrap_or(0.0); }
                        "width" => { rect_w = value.parse().unwrap_or(0.0); has_rect = true; }
                        "height" => { rect_h = value.parse().unwrap_or(0.0); has_rect = true; }
                        _ => {}
                    }
                }
            }

            if let Some(path_str) = d {
                items.push(strata_layout::SvgItem {
                    path_data: path_str,
                    stroke_color: stroke,
                    fill_color: fill,
                    stroke_width,
                    view_box: default_view_box,
                });
            } else if has_line {
                items.push(strata_layout::SvgItem {
                    path_data: format!("M {} {} L {} {}", x1, y1, x2, y2),
                    stroke_color: stroke.or(Some(parent_color)),
                    fill_color: None,
                    stroke_width,
                    view_box: default_view_box,
                });
            } else if has_circle && r > 0.0 {
                items.push(strata_layout::SvgItem {
                    path_data: format!("M {} {} a {} {} 0 1 0 {} 0 a {} {} 0 1 0 {} 0 Z", cx - r, cy, r, r, 2.0 * r, r, r, -2.0 * r),
                    stroke_color: stroke,
                    fill_color: fill,
                    stroke_width,
                    view_box: default_view_box,
                });
            } else if has_ellipse && rx > 0.0 && ry > 0.0 {
                items.push(strata_layout::SvgItem {
                    path_data: format!("M {} {} a {} {} 0 1 0 {} 0 a {} {} 0 1 0 {} 0 Z", cx - rx, cy, rx, ry, 2.0 * rx, rx, ry, -2.0 * rx),
                    stroke_color: stroke,
                    fill_color: fill,
                    stroke_width,
                    view_box: default_view_box,
                });
            } else if has_rect && rect_w > 0.0 && rect_h > 0.0 {
                let rect_path = if rect_rx > 0.0 {
                    format!(
                        "M {} {} h {} a {} {} 0 0 1 {} {} v {} a {} {} 0 0 1 {} {} h {} a {} {} 0 0 1 {} {} v {} a {} {} 0 0 1 {} {} Z",
                        rect_x + rect_rx, rect_y,
                        rect_w - 2.0 * rect_rx,
                        rect_rx, rect_rx, rect_rx, rect_rx,
                        rect_h - 2.0 * rect_rx,
                        rect_rx, rect_rx, -rect_rx, rect_rx,
                        -(rect_w - 2.0 * rect_rx),
                        rect_rx, rect_rx, -rect_rx, -rect_rx,
                        -(rect_h - 2.0 * rect_rx),
                        rect_rx, rect_rx, rect_rx, -rect_rx,
                    )
                } else {
                    format!("M {} {} h {} v {} h {} Z", rect_x, rect_y, rect_w, rect_h, -rect_w)
                };
                items.push(strata_layout::SvgItem {
                    path_data: rect_path,
                    stroke_color: stroke,
                    fill_color: fill,
                    stroke_width,
                    view_box: default_view_box,
                });
            }

            // Recurse for children (e.g. <g>, or nested paths)
            for child in children {
                let child_items = Self::collect_svg_items_from_blueprint(
                    child,
                    default_view_box,
                    stroke,
                    fill,
                    stroke_width,
                    parent_color,
                );
                items.extend(child_items);
            }
        }
        _ => {}
    }
    items
}

    /// Recursively flattens Fragment blueprints so child elements splice directly into parent layout contexts.
    fn append_child_layout_nodes(
        blueprint: &NodeBlueprint,
        parent_style: Option<&ComputedStyle>,
        out: &mut Vec<LayoutNode>,
    ) {
        if let NodeBlueprint::Fragment(children) = blueprint {
            for child in children {
                Self::append_child_layout_nodes(child, parent_style, out);
            }
        } else {
            out.push(Self::build_layout_tree_with_parent(blueprint, parent_style));
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

    /// Recursively flattens Fragment blueprints for linear child index alignment.
    fn flatten_blueprints<'a>(bp: &'a NodeBlueprint, out: &mut Vec<&'a NodeBlueprint>) {
        if let NodeBlueprint::Fragment(children) = bp {
            for c in children {
                Self::flatten_blueprints(c, out);
            }
        } else {
            out.push(bp);
        }
    }

    /// Recursively hit tests a coordinate point against the blueprint and computed layout hierarchy.
    pub fn hit_test_click(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        px: f32,
        py: f32,
    ) -> Option<std::rc::Rc<dyn Fn(strata_dom::Event)>> {
        if !layout.geometry.border_box.contains_point(px, py) {
            return None;
        }

        match blueprint {
            NodeBlueprint::Element {
                attributes,
                children,
                ..
            } => {
                let mut flat_children = Vec::new();
                for c in children {
                    Self::flatten_blueprints(c, &mut flat_children);
                }

                for (&child_bp, child_layout) in flat_children.iter().zip(layout.children.iter()).rev() {
                    if let Some(handler) = Self::hit_test_click(child_bp, child_layout, px, py) {
                        return Some(handler);
                    }
                }

                for attr in attributes {
                    if let strata_dom::Attribute::EventListener {
                        event: strata_dom::EventType::Click,
                        handler,
                    } = attr
                    {
                        return Some(handler.clone());
                    }
                }
                None
            }
            NodeBlueprint::Fragment(children) => {
                for (child_bp, child_layout) in children.iter().zip(layout.children.iter()).rev() {
                    if let Some(handler) = Self::hit_test_click(child_bp, child_layout, px, py) {
                        return Some(handler);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Dispatches a pointer click event at (x, y) coordinates into the application blueprint tree.
    /// Returns true if an event listener was invoked.
    pub fn dispatch_click(&self, px: f32, py: f32) -> bool {
        if let Some(handler) = Self::hit_test_click(&self.root_blueprint, &self.layout_root, px, py) {
            handler(strata_dom::Event::new(strata_dom::EventType::Click));
            true
        } else {
            false
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

#[cfg(target_os = "linux")]
/// Launches an interactive native X11 desktop window on Linux displaying the Strata application.
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
    println!("[+] Launching Strata Native Desktop Window on X11: \"{}\"...", config.title);
    let mut x11_window = strata_platform::x11_window::X11Window::new(config.clone())?;
    let view = root_fn();
    let width = config.width as f32;
    let height = config.height as f32;
    let mut runtime = AppRuntime::new(view, width, height, 1.0);
    let mut presenter = SoftwareSurfacePresenter::new(config.width, config.height, SwapchainFormat::Bgra8Unorm);
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
            if event_handler(&event, &mut runtime) {
                needs_rebuild = true;
            }

            match event {
                strata_platform::PlatformEvent::WindowCloseRequested => {
                    running = false;
                    break;
                }
                strata_platform::PlatformEvent::WindowResized { width: w, height: h } => {
                    compositor.resize(w, h, runtime.dpr);
                    let _ = presenter.resize(w, h);
                    runtime.viewport_width = w as f32;
                    runtime.viewport_height = h as f32;
                    needs_redraw = true;
                }
                strata_platform::PlatformEvent::MouseDown { x, y, button: strata_platform::MouseButton::Left } => {
                    if runtime.dispatch_click(x, y) {
                        needs_rebuild = true;
                    }
                }
                _ => {}
            }
        }

        if needs_rebuild {
            let new_view = root_fn();
            runtime.update_view(new_view);
            let _ = runtime.render_frame(&mut compositor, &mut presenter, true);
            let _ = x11_window.present_bgra(&presenter.backing_buffer, presenter.width, presenter.height);
        } else if needs_redraw || x11_window.redraw_requested {
            let _ = runtime.render_frame(&mut compositor, &mut presenter, false);
            let _ = x11_window.present_bgra(&presenter.backing_buffer, presenter.width, presenter.height);
        }

        std::thread::sleep(std::time::Duration::from_millis(16));
    }

    Ok(())
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
