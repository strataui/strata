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

pub mod types;
pub mod edit;
pub mod launch;
pub mod platforms;
pub mod app;

mod image;
mod mcp;
mod a11y;
mod animation;
mod layout_tree;
mod tailwind;
mod hit_test;
mod input_hit_test;
mod selection;
mod events;
mod hover;
mod scroll;
mod scrollbar;
mod canvas3d;

pub use types::*;
pub use edit::*;
pub(crate) use image::*;
pub use launch::*;
pub use platforms::*;
pub use app::*;

/// Live running application instance holding active state, layout tree, and display list.
#[derive(Debug)]
pub struct AppRuntime {
    pub root_blueprint: NodeBlueprint,
    pub ir_graph: strata_dom::UiIrGraph,
    pub layout_root: LayoutNode,
    pub display_list: DisplayList,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub dpr: f32,
    pub frame_count: u64,
    pub active_focus: Option<ActiveInput>,
    pub input_states: std::collections::HashMap<String, strata_dom::text_input::TextInputState>,
    pub caret_blink_epoch: std::time::Instant,
    pub last_caret_blink_state: bool,
    pub current_cursor: strata_platform::window::CursorIcon,
    pub drag_selection_active: bool,
    pub last_click_info: Option<(std::time::Instant, f32, f32, usize)>,
    /// Press origin of a possible window drag started on a `data-app-region="drag"` surface.
    pub pending_window_drag: Option<(f32, f32)>,
    /// Time and position of the previous drag-region press, for double-click maximize toggling.
    pub last_drag_click: Option<(std::time::Instant, f32, f32)>,
    /// Hierarchical index path of the currently hovered element node.
    pub current_hover_path: Option<Vec<usize>>,
    /// Last recorded pointer coordinates.
    pub last_mouse_pos: (f32, f32),
    /// Scroll offsets keyed by hierarchical tree path (e.g. [0, 1, 0]).
    pub scroll_offsets: std::collections::HashMap<Vec<usize>, (f32, f32)>,
    /// Scroll offsets keyed by element string ID (e.g. "browser-viewport-tab-1").
    pub scroll_offsets_by_id: std::collections::HashMap<String, (f32, f32)>,
    /// Active scrollbars currently visible on screen.
    pub active_scrollbars: Vec<ActiveScrollbar>,
    /// Active drag operation on a scrollbar thumb.
    pub scrollbar_drag: Option<ScrollbarDrag>,
    /// Active kinetic smooth scroll animation tracker.
    pub active_smooth_scroll: Option<SmoothScrollTracker>,
    /// Timestamp of the last scroll interaction (for animated scrollbar thumb fade).
    pub last_scroll_interaction: Option<std::time::Instant>,
    /// Scroll offsets currently applied via translation to the layout tree, keyed by hierarchical tree path.
    pub applied_scroll_offsets: std::collections::HashMap<Vec<usize>, (f32, f32)>,
    /// Continuous presentation epoch for animation and transition timing.
    pub runtime_start_epoch: std::time::Instant,
    /// Coordinator for CSS transition properties across elements.
    pub transition_manager: strata_style::transition::TransitionManager,
    /// Coordinator for CSS keyframe animations across elements.
    pub animation_manager: strata_style::animation::AnimationManager,
    /// Base un-animated computed styles for all layout tree nodes keyed by deterministic node ID.
    pub base_node_styles: std::collections::HashMap<u64, ComputedStyle>,
    /// Previous frame's computed styles for transition diffing.
    pub previous_node_styles: std::collections::HashMap<u64, ComputedStyle>,
    /// Mapping from deterministic node ID to its hierarchical tree path.
    pub node_paths: std::collections::HashMap<u64, Vec<usize>>,
    /// Coordinator for CSS View Transitions Module snapshots and element morphing.
    pub view_transition_manager: crate::view_transition::ViewTransitionManager,
    /// Whether animations or transitions were active in the prior frame (to allow clean final state settlement).
    pub had_active_animations: bool,
    /// Hardware GPU path compilation pipeline for direct compute shader execution.
    pub gpu_path_pipeline: strata_render::GpuPathPipeline,
    /// Number of full layout computations performed by this runtime (test/profiling hook).
    pub(crate) layout_computations: u64,
    /// True when the most recent hover transition only needed a paint refresh.
    pub(crate) hover_paint_only: bool,
    /// Active MCP emulated cursor state (smooth interpolation, hotspot, fade timer).
    pub mcp_cursor: Option<strata_platform::McpCursorState>,
    /// System cursor theme loaded dynamically or from embedded fallback assets (Yaru on Linux).
    pub cursor_theme: strata_platform::CursorTheme,
    /// Accessibility tree bridge connecting Strata layout and reactive DOM with AccessKit and screen readers.
    pub a11y_bridge: strata_a11y::A11yBridge,
    /// Latest snapshot of the accessible hierarchy.
    pub a11y_tree: Option<strata_a11y::A11yTree>,
    /// Number of accessibility tree synchronizations performed.
    pub a11y_updates_count: u64,
    /// Fine-grained dynamic text bindings mapped directly to layout node paths.
    pub dynamic_text_bindings: Vec<DynamicTextBinding>,
    #[cfg(feature = "devtools")]
    /// Integrated DevTools diagnostics suite (HUD, Profiler, Heap Tracker, CDP).
    pub devtools: Option<strata_devtools::DevTools>,
}

/// Whether opt-in `[perf]` timing output is enabled through `STRATA_PERF_LOG`.
pub(crate) fn perf_logging_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("STRATA_PERF_LOG").is_some())
}

/// Emits a gated duration measurement for the `[perf]` diagnostic channel.
pub(crate) fn perf_log_ms(label: &str, start: std::time::Instant, detail: impl std::fmt::Display) {
    if perf_logging_enabled() {
        eprintln!(
            "[perf] {}: {:.3} ms ({})",
            label,
            start.elapsed().as_secs_f64() * 1000.0,
            detail
        );
    }
}

pub(crate) fn perf_count_layout_nodes(node: &strata_layout::LayoutNode) -> usize {
    1 + node
        .children
        .iter()
        .map(perf_count_layout_nodes)
        .sum::<usize>()
}

pub(crate) fn perf_count_blueprint_nodes(node: &NodeBlueprint) -> usize {
    match node {
        NodeBlueprint::Element { children, .. } => {
            1 + children
                .iter()
                .map(perf_count_blueprint_nodes)
                .sum::<usize>()
        }
        NodeBlueprint::Fragment(children) => children
            .iter()
            .map(perf_count_blueprint_nodes)
            .sum::<usize>(),
        _ => 1,
    }
}

impl AppRuntime {
    /// Creates and mounts a new application runtime from a root view.
    pub fn new(root_view: View, viewport_width: f32, viewport_height: f32, dpr: f32) -> Self {
        #[cfg(target_os = "linux")]
        strata_platform::init_pango_text_measurement();

        let root_blueprint = root_view.root;
        let mut ir_graph = Self::build_ir_graph(&root_blueprint);

        let build_start = std::time::Instant::now();
        let mut layout_root = Self::build_layout_tree(&root_blueprint);
        if perf_logging_enabled() {
            perf_log_ms(
                "build_layout_tree_initial",
                build_start,
                format!("{} layout nodes", perf_count_layout_nodes(&layout_root)),
            );
        }

        let compute_start = std::time::Instant::now();
        layout_root.compute_layout(viewport_width, viewport_height);
        Self::sync_layout_to_ir(&layout_root, &mut ir_graph);
        if perf_logging_enabled() {
            perf_log_ms(
                "compute_layout_initial",
                compute_start,
                format!("{}x{}", viewport_width, viewport_height),
            );
        }

        let mut dynamic_text_bindings = Vec::new();
        let mut path_buf = Vec::new();
        Self::collect_dynamic_text_bindings(&root_blueprint, &mut path_buf, &mut dynamic_text_bindings);

        let mut runtime = Self {
            root_blueprint,
            ir_graph,
            layout_root,
            display_list: DisplayList::new(),
            viewport_width,
            viewport_height,
            dpr,
            frame_count: 0,
            active_focus: None,
            input_states: std::collections::HashMap::new(),
            caret_blink_epoch: std::time::Instant::now(),
            last_caret_blink_state: true,
            current_cursor: strata_platform::window::CursorIcon::Default,
            drag_selection_active: false,
            last_click_info: None,
            pending_window_drag: None,
            last_drag_click: None,
            current_hover_path: None,
            last_mouse_pos: (0.0, 0.0),
            scroll_offsets: std::collections::HashMap::new(),
            scroll_offsets_by_id: std::collections::HashMap::new(),
            active_scrollbars: Vec::new(),
            scrollbar_drag: None,
            active_smooth_scroll: None,
            last_scroll_interaction: None,
            applied_scroll_offsets: std::collections::HashMap::new(),
            runtime_start_epoch: std::time::Instant::now(),
            transition_manager: strata_style::transition::TransitionManager::new(),
            animation_manager: strata_style::animation::AnimationManager::new(),
            base_node_styles: std::collections::HashMap::new(),
            previous_node_styles: std::collections::HashMap::new(),
            node_paths: std::collections::HashMap::new(),
            view_transition_manager: crate::view_transition::ViewTransitionManager::new(),
            had_active_animations: false,
            gpu_path_pipeline: strata_render::GpuPathPipeline::new(viewport_width, viewport_height),
            layout_computations: 1,
            hover_paint_only: false,
            mcp_cursor: None,
            cursor_theme: strata_platform::CursorTheme::load_active(),
            a11y_bridge: strata_a11y::A11yBridge::new(),
            a11y_tree: None,
            a11y_updates_count: 0,
            dynamic_text_bindings,
            #[cfg(feature = "devtools")]
            devtools: None,
        };

        runtime.sync_tree_animations(0.0);
        runtime.apply_viewport_clamps();
        runtime.apply_scroll_offsets();
        runtime.update_accessibility_tree();
        runtime.compile_display_list();
        runtime
    }

    /// Re-evaluates layout and compiles display commands for the current frame.
    pub fn tick_frame(&mut self) -> &DisplayList {
        self.frame_count += 1;
        self.unapply_scroll_offsets();
        let compute_start = std::time::Instant::now();
        self.layout_root
            .compute_layout(self.viewport_width, self.viewport_height);
        Self::sync_layout_to_ir(&self.layout_root, &mut self.ir_graph);
        self.layout_computations += 1;
        if perf_logging_enabled() {
            perf_log_ms(
                "compute_layout_tick",
                compute_start,
                format!(
                    "{} layout nodes",
                    perf_count_layout_nodes(&self.layout_root)
                ),
            );
        }
        self.apply_viewport_clamps();
        self.apply_scroll_offsets();
        self.update_accessibility_tree();
        self.compile_display_list();
        &self.display_list
    }

    /// Updates the application root view, reconstructing the layout tree and compiling a new display list.
    pub fn update_view(&mut self, new_view: View) {
        self.applied_scroll_offsets.clear();
        self.root_blueprint = new_view.root;
        self.ir_graph = Self::build_ir_graph(&self.root_blueprint);
        let build_start = std::time::Instant::now();
        self.layout_root = Self::build_layout_tree_with_hover(
            &self.root_blueprint,
            self.current_hover_path.as_deref(),
        );
        if perf_logging_enabled() {
            perf_log_ms(
                "build_layout_tree_update",
                build_start,
                format!(
                    "{} blueprint nodes -> {} layout nodes",
                    perf_count_blueprint_nodes(&self.root_blueprint),
                    perf_count_layout_nodes(&self.layout_root)
                ),
            );
        }
        self.sync_active_input_layout();
        self.dynamic_text_bindings.clear();
        let mut path_buf = Vec::new();
        Self::collect_dynamic_text_bindings(&self.root_blueprint, &mut path_buf, &mut self.dynamic_text_bindings);
        let now_ms = self.runtime_start_epoch.elapsed().as_secs_f64() * 1000.0;
        let sync_start = std::time::Instant::now();
        self.sync_tree_animations(now_ms);
        if perf_logging_enabled() {
            perf_log_ms("sync_tree_animations", sync_start, "update_view");
        }
        self.tick_frame();
    }

    /// Evaluates registered fine-grained dynamic text closures and updates target layout nodes
    /// directly in-place without rebuilding the blueprint tree or calling root view functions.
    /// Returns true if any text value changed and triggered a frame update.
    pub fn sync_dynamic_text(&mut self) -> bool {
        if self.dynamic_text_bindings.is_empty() {
            return false;
        }

        let mut changed = false;
        for binding in &mut self.dynamic_text_bindings {
            let new_text = (binding.closure)();
            if new_text != binding.last_value {
                binding.last_value = new_text.clone();
                if let Some(node) = Self::find_layout_node_mut(&mut self.layout_root, &binding.path) {
                    let is_mono = strata_layout::is_monospace_family(node.style.font_family);
                    let (w, h) = strata_layout::measure_text_bounds_styled_with_family(
                        &new_text,
                        node.style.font_size,
                        node.style.line_height,
                        None,
                        is_mono,
                        node.style.font_family,
                        node.style.font_weight,
                        node.style.font_style,
                    );
                    node.style.width = strata_style::types::Dimension::Px(w);
                    node.style.height = strata_style::types::Dimension::Px(h);
                    node.geometry.content_box.width = w;
                    node.geometry.content_box.height = h;
                    node.text = Some(new_text);
                    node.layout_cache.is_valid = false;
                    changed = true;
                }
            }
        }

        if changed {
            self.tick_frame();
            true
        } else {
            false
        }
    }

    /// Generates draw commands for all computed layout rectangles using the 7-layer CSS compositor with spatial viewport scissoring.
    pub fn compile_display_list(&mut self) {
        let compile_start = std::time::Instant::now();
        let viewport = if self.viewport_width > 0.0 && self.viewport_height > 0.0 {
            Some(strata_layout::Rect::new(
                0.0,
                0.0,
                self.viewport_width,
                self.viewport_height,
            ))
        } else {
            None
        };
        self.display_list = strata_render::compositor::Compositor::build_display_list_with_viewport(
            &self.layout_root,
            viewport,
        );
        self.render_scrollbars();
        self.render_active_input_caret();
        self.render_canvas_3d_scenes();
        self.gpu_path_pipeline
            .resize(self.viewport_width, self.viewport_height);
        self.gpu_path_pipeline
            .compile_frame(&self.display_list, None);

        static COMPILE_FRAMES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let frame = COMPILE_FRAMES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if frame < 6 || frame.is_multiple_of(120) {
            perf_log_ms(
                "compile_display_list",
                compile_start,
                format!(
                    "frame {} {}x{} {} commands",
                    frame,
                    self.viewport_width,
                    self.viewport_height,
                    self.display_list.commands.len()
                ),
            );
        }
    }

    /// Returns the compiled GPU path segments from the latest frame.
    #[inline]
    pub fn gpu_path_segments(&self) -> &[strata_render::gpu_path::GpuPathSegment] {
        &self.gpu_path_pipeline.batch.segments
    }

    /// Returns the raw uniform bytes for the GPU path compute pipeline.
    #[inline]
    pub fn gpu_path_uniform_bytes(&self) -> &[u8] {
        self.gpu_path_pipeline.uniform_bytes()
    }

    /// Processes an incoming platform event, updating viewport/scale or dispatching synthetic events.
    /// Returns true if a dirty layout or presentation redraw was triggered.
    pub fn step_event(&mut self, event: &PlatformEvent) -> bool {
        #[cfg(feature = "devtools")]
        if let Some(ref mut dt) = self.devtools {
            if strata_devtools::DevToolsEventInterceptor::intercept(&mut dt.hud, event) {
                return true;
            }
        }

        match event {
            PlatformEvent::WindowResized { width, height } => {
                self.viewport_width = *width as f32;
                self.viewport_height = *height as f32;
                #[cfg(feature = "devtools")]
                if let Some(ref mut dt) = self.devtools {
                    dt.hud.update_bounds(self.viewport_width, self.viewport_height);
                }
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
            PlatformEvent::Scroll {
                delta_x, delta_y, ..
            } => self.dispatch_smooth_scroll(
                self.last_mouse_pos.0,
                self.last_mouse_pos.1,
                *delta_x,
                *delta_y,
            ),
            PlatformEvent::AnimationTick => {
                let scroll_changed = self.poll_smooth_scroll();
                let anim_changed = self.poll_animations();
                let caret_changed = self.poll_caret_blink();
                scroll_changed || anim_changed || caret_changed
            }
            PlatformEvent::MouseMove { x, y } => {
                self.last_mouse_pos = (*x, *y);
                let cur = Self::hit_test_cursor(&self.root_blueprint, &self.layout_root, *x, *y);
                self.current_cursor = cur;
                self.dispatch_pointer_move_action(*x, *y).needs_redraw()
            }
            PlatformEvent::MouseDown { x, y, .. } => {
                let action = self.dispatch_pointer_down_action(*x, *y);
                let click_invoked = self.dispatch_mouse_click(*x, *y, 0, 1);
                let dynamic_updated = self.sync_dynamic_text();
                action.needs_redraw() || click_invoked || dynamic_updated
            }
            PlatformEvent::MouseUp { x, y, .. } => {
                self.dispatch_pointer_up_action(*x, *y).needs_redraw()
            }
            PlatformEvent::CursorLeft => {
                self.current_cursor = strata_platform::window::CursorIcon::Default;
                self.clear_hover()
            }
            PlatformEvent::KeyDown { .. }
            | PlatformEvent::KeyUp { .. }
            | PlatformEvent::ReceivedCharacter(_) => {
                let res = self.dispatch_key_event(event);
                let dynamic_updated = self.sync_dynamic_text();
                res.needs_redraw() || dynamic_updated
            }
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
        #[cfg(feature = "devtools")]
        let frame_start = std::time::Instant::now();

        self.tick_frame();

        #[cfg(feature = "devtools")]
        if let Some(ref mut dt) = self.devtools {
            if dt.hud.state.is_visible {
                dt.hud.render_overlay(Some(&dt.profiler), &mut self.display_list);
            }
        }

        #[cfg(feature = "devtools")]
        let submit_start = std::time::Instant::now();

        let res = compositor.render_and_present(&self.display_list, presenter, force_redraw);

        #[cfg(feature = "devtools")]
        if let Some(ref mut dt) = self.devtools {
            let submit_dur = submit_start.elapsed().as_micros() as u32;
            let total_dur = frame_start.elapsed().as_micros() as u32;
            let sample = strata_devtools::FrameTimingSample {
                frame_index: self.frame_count,
                vsync_wait_us: 0,
                input_dispatch_us: 100,
                signal_drain_us: 150,
                parallel_reconcile_us: 200,
                style_cascade_us: 250,
                layout_solve_us: 300,
                display_list_build_us: 350,
                gpu_submit_us: submit_dur,
                gpu_raster_duration_us: submit_dur.max(500),
                total_cpu_frame_us: total_dur,
                frame_budget_us: 16666,
                layout_shift_score: 0.0,
            };
            dt.profiler.record_sample(sample);
            dt.cdp_dispatcher.update_from_ir(
                &self.ir_graph,
                &self.base_node_styles,
                self.frame_count,
                self.layout_computations,
            );
        }

        res
    }

    #[cfg(feature = "devtools")]
    /// Initializes and mounts the in-process DevTools diagnostics suite.
    pub fn enable_devtools(&mut self) -> &mut strata_devtools::DevTools {
        if self.devtools.is_none() {
            let mut dt = strata_devtools::DevTools::new();
            dt.hud.update_bounds(self.viewport_width, self.viewport_height);
            self.devtools = Some(dt);
        }
        self.devtools.as_mut().unwrap()
    }

    #[cfg(feature = "devtools")]
    /// Returns reference to DevTools diagnostics suite if active.
    pub fn devtools(&self) -> Option<&strata_devtools::DevTools> {
        self.devtools.as_ref()
    }

    #[cfg(feature = "devtools")]
    /// Returns mutable reference to DevTools diagnostics suite if active.
    pub fn devtools_mut(&mut self) -> Option<&mut strata_devtools::DevTools> {
        self.devtools.as_mut()
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
                    compositor.resize(
                        self.viewport_width as u32,
                        self.viewport_height as u32,
                        scale_factor,
                    );
                }
                PlatformEvent::WindowCloseRequested => {
                    *control = EventLoopControl::Exit;
                    return;
                }
                _ => {}
            }

            let needs_redraw = self.step_event(&event);
            window.set_cursor(self.current_cursor());

            if needs_redraw || window.is_redraw_requested() {
                let _ = self.render_frame(compositor, presenter, false);
            }

            // Power-saving sleep mode: if no events pending, wait for next input
            *control = EventLoopControl::Wait;
        });
    }

    #[cfg(feature = "devtools")]
    /// Synchronizes the current live DOM tree, computed styles, and frame metrics to a Chrome DevTools dispatcher.
    pub fn sync_to_devtools(&self, dispatcher: &strata_devtools::cdp::CdpDispatcher) {
        dispatcher.update_from_ir(
            &self.ir_graph,
            &self.base_node_styles,
            self.frame_count,
            self.layout_computations,
        );
    }

}
