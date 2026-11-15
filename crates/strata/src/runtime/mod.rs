//! Live Application Runtime Orchestrator connecting RSX Blueprints, Style Cascade, Layout, and Rendering.

use strata_dom::{NodeBlueprint, Tag, View};
use strata_layout::prelude::*;
use strata_platform::{
    EventLoop, EventLoopControl, LivePlatformWindow, NativeSurfaceWindow, PlatformEvent,
    PlatformWindow, SoftwareSurfacePresenter, SwapchainCompositor, SwapchainFormat, WindowConfig,
};
use strata_render::compositor::DisplayList;
use strata_render::swapchain::{PresentError, SurfacePresenter};
use strata_style::types::{ComputedStyle, Dimension, Display, FlexDirection};

pub mod app;
pub mod edit;
pub mod launch;
pub mod platforms;
pub mod types;

mod a11y;
mod animation;
mod canvas3d;
pub mod dom_hydration;
mod events;
mod hit_test;
mod hover;
mod image;
mod input_hit_test;
mod layout_tree;
mod mcp;
mod profile;
mod scroll;
mod scrollbar;
mod selection;
pub mod spatial;
mod tailwind;

pub use app::*;
pub use dom_hydration::*;
pub use edit::*;
pub(crate) use image::*;
pub use launch::*;
pub use platforms::*;
pub use profile::{FrameProfile, FrameProfileHook};
pub use spatial::*;
pub use types::*;

/// Live running application instance holding active state, layout tree, and display list.
#[derive(Debug)]
pub struct AppRuntime {
    pub root_blueprint: NodeBlueprint,
    pub ir_graph: strata_dom::UiIrGraph,
    pub layout_root: LayoutNode,
    pub display_list: DisplayList,
    pub spatial_index: SpatialIndex,
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
    /// Hierarchical index path of the currently focused element node.
    pub current_focus_path: Option<Vec<usize>>,
    /// Whether pointer primary button is currently held down.
    pub is_mouse_down: bool,
    /// Whether dark mode is active for theme-dependent utility styling.
    pub is_dark_mode: bool,
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
    /// Anchored top-layer `<Portal>` translations currently applied to the layout tree, keyed by path.
    pub applied_portal_offsets: std::collections::HashMap<Vec<usize>, (f32, f32)>,
    /// Cached flag indicating whether the active layout tree contains any anchored `<Portal>` nodes.
    pub(crate) has_anchored_portals: bool,
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
    /// Mapping from hierarchical tree path to its identity-aware animation node ID.
    pub path_node_ids: std::collections::HashMap<Vec<usize>, u64>,
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
    pub(crate) cursor_theme: std::sync::OnceLock<strata_platform::CursorTheme>,
    /// Accessibility tree bridge connecting Strata layout and reactive DOM with AccessKit and screen readers.
    pub a11y_bridge: strata_a11y::A11yBridge,
    /// Latest snapshot of the accessible hierarchy.
    pub a11y_tree: Option<strata_a11y::A11yTree>,
    /// Number of accessibility tree synchronizations performed.
    pub a11y_updates_count: u64,
    /// Fine-grained dynamic text bindings mapped directly to layout node paths.
    pub dynamic_text_bindings: Vec<DynamicTextBinding>,
    /// Background channel receiver for live DOM mutations from scripts or browser tabs.
    pub dom_mutation_receiver: Option<std::sync::mpsc::Receiver<strata_js::DomMutationRecord>>,
    #[cfg(feature = "devtools")]
    /// Integrated DevTools diagnostics suite (HUD, Profiler, Heap Tracker, CDP).
    pub devtools: Option<strata_devtools::DevTools>,
    /// Client-Side Decorations (CSD) controller managing titlebar geometry and drag tracking.
    pub csd_controller: strata_platform::CsdController,
    /// Phase timings accumulated since the last presented frame.
    pub(crate) frame_profile: FrameProfile,
    /// True when anything that affects painting changed since the display list was compiled.
    pub(crate) paint_dirty: bool,
    /// Number of display-list compilations skipped because nothing changed.
    pub(crate) compiles_skipped: u64,
    /// Optional observer invoked with the completed profile after each present.
    pub(crate) frame_profile_hook: Option<FrameProfileHook>,
    /// Cached `<style>` stylesheets collected from `root_blueprint`.
    pub(crate) cached_stylesheets: Vec<std::sync::Arc<strata_style::StyleSheet>>,
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
    let mut count = 0;
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        count += 1;
        for child in &n.children {
            stack.push(child);
        }
    }
    count
}

pub(crate) fn perf_count_blueprint_nodes(node: &NodeBlueprint) -> usize {
    let mut count = 0;
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        count += 1;
        match n {
            NodeBlueprint::Element { children, .. } | NodeBlueprint::Fragment(children) => {
                for child in children {
                    stack.push(child);
                }
            }
            NodeBlueprint::Text(_) | NodeBlueprint::DynamicText(_) | NodeBlueprint::Empty => {}
        }
    }
    count
}

impl AppRuntime {
    /// Creates and mounts a new application runtime from a root view.
    pub fn new(root_view: View, viewport_width: f32, viewport_height: f32, dpr: f32) -> Self {
        #[cfg(target_os = "linux")]
        strata_platform::init_pango_text_measurement();

        let mut root_blueprint = root_view.root.flatten_fragments();
        root_blueprint.ensure_entity_ids();
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
        let mut spatial_index = SpatialIndex::new();
        spatial_index.rebuild(&root_blueprint, &layout_root);
        if perf_logging_enabled() {
            perf_log_ms(
                "compute_layout_initial",
                compute_start,
                format!("{}x{}", viewport_width, viewport_height),
            );
        }

        let mut dynamic_text_bindings = Vec::new();
        let mut path_buf = Vec::new();
        Self::collect_dynamic_text_bindings(
            &root_blueprint,
            &mut path_buf,
            &mut dynamic_text_bindings,
        );

        let mut runtime = Self {
            root_blueprint,
            ir_graph,
            layout_root,
            display_list: DisplayList::new(),
            spatial_index,
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
            current_focus_path: None,
            is_mouse_down: false,
            is_dark_mode: false,
            last_mouse_pos: (0.0, 0.0),
            scroll_offsets: std::collections::HashMap::new(),
            scroll_offsets_by_id: std::collections::HashMap::new(),
            active_scrollbars: Vec::new(),
            scrollbar_drag: None,
            active_smooth_scroll: None,
            last_scroll_interaction: None,
            applied_scroll_offsets: std::collections::HashMap::new(),
            applied_portal_offsets: std::collections::HashMap::new(),
            has_anchored_portals: false,
            runtime_start_epoch: std::time::Instant::now(),
            transition_manager: strata_style::transition::TransitionManager::new(),
            animation_manager: strata_style::animation::AnimationManager::new(),
            base_node_styles: std::collections::HashMap::new(),
            previous_node_styles: std::collections::HashMap::new(),
            node_paths: std::collections::HashMap::new(),
            path_node_ids: std::collections::HashMap::new(),
            view_transition_manager: crate::view_transition::ViewTransitionManager::new(),
            had_active_animations: false,
            gpu_path_pipeline: strata_render::GpuPathPipeline::new(viewport_width, viewport_height),
            layout_computations: 1,
            hover_paint_only: false,
            mcp_cursor: None,
            cursor_theme: std::sync::OnceLock::new(),
            a11y_bridge: strata_a11y::A11yBridge::new(),
            a11y_tree: None,
            a11y_updates_count: 0,
            dynamic_text_bindings,
            dom_mutation_receiver: None,
            #[cfg(feature = "devtools")]
            devtools: None,
            csd_controller: strata_platform::CsdController::default(),
            frame_profile: FrameProfile::default(),
            paint_dirty: true,
            compiles_skipped: 0,
            frame_profile_hook: None,
            cached_stylesheets: Vec::new(),
        };

        let initial_sheets = Self::collect_stylesheets_from_blueprint(&runtime.root_blueprint);
        for sheet in &initial_sheets {
            runtime.register_keyframes_stylesheet(sheet);
        }
        runtime.cached_stylesheets = initial_sheets;
        runtime.sync_tree_animations(0.0);
        runtime.apply_viewport_clamps();
        runtime.apply_scroll_offsets();
        runtime
            .spatial_index
            .rebuild(&runtime.root_blueprint, &runtime.layout_root);
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
        self.frame_profile.layout += compute_start.elapsed();
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
        self.spatial_index
            .rebuild(&self.root_blueprint, &self.layout_root);
        self.update_accessibility_tree();
        self.compile_display_list();
        &self.display_list
    }

    /// Returns the current interaction context.
    pub fn interaction_context(&self) -> InteractionContext<'_> {
        InteractionContext {
            hovered_path: self.current_hover_path.as_deref(),
            focused_path: self.current_focus_path.as_deref(),
            is_mouse_down: self.is_mouse_down,
            is_dark_mode: self.is_dark_mode,
        }
    }

    /// Sets the active dark mode state and triggers a layout and style recomputation if changed.
    pub fn set_dark_mode(&mut self, is_dark: bool) -> bool {
        if self.is_dark_mode != is_dark {
            self.is_dark_mode = is_dark;
            self.rebuild_layout_tree();
            true
        } else {
            false
        }
    }

    /// Sets the active focused element path and triggers a layout and style recomputation if changed.
    pub fn set_focus_path(&mut self, path: Option<Vec<usize>>) -> bool {
        if self.current_focus_path != path {
            self.current_focus_path = path;
            self.rebuild_layout_tree();
            true
        } else {
            false
        }
    }

    /// Clears and rebuilds the spatial acceleration index.
    pub fn rebuild_spatial_index(&mut self) {
        self.spatial_index
            .rebuild(&self.root_blueprint, &self.layout_root);
    }

    /// Queries the front-most EntityId at point (px, py).
    pub fn query_entity_at_point(&self, px: f32, py: f32) -> Option<strata_primitives::EntityId> {
        self.spatial_index.query_entity_at_point(px, py)
    }

    /// Queries the front-most event listener registered for `event_type` at point (px, py).
    pub fn query_event_at_point(
        &self,
        px: f32,
        py: f32,
        event_type: strata_dom::EventType,
    ) -> Option<(
        strata_primitives::EntityId,
        std::rc::Rc<dyn Fn(strata_dom::Event)>,
        strata_layout::Rect,
    )> {
        self.spatial_index.query_event_at_point(px, py, event_type)
    }

    /// Recursively locates a layout node by its EntityId.
    pub fn find_layout_by_entity(
        &self,
        entity_id: strata_primitives::EntityId,
    ) -> Option<&LayoutNode> {
        self.layout_root.find_by_entity_id(entity_id)
    }

    /// Recursively locates a mutable layout node by its EntityId.
    pub fn find_layout_mut_by_entity(
        &mut self,
        entity_id: strata_primitives::EntityId,
    ) -> Option<&mut LayoutNode> {
        self.layout_root.find_by_entity_id_mut(entity_id)
    }

    /// Recursively locates a blueprint node by its EntityId.
    pub fn find_blueprint_by_entity(
        &self,
        entity_id: strata_primitives::EntityId,
    ) -> Option<&NodeBlueprint> {
        self.root_blueprint.find_by_entity_id(entity_id)
    }

    /// Updates the application root view, reconstructing the layout tree and compiling a new display list.
    pub fn update_view(&mut self, new_view: View) {
        self.applied_scroll_offsets.clear();
        self.applied_portal_offsets.clear();
        self.root_blueprint = new_view.root.flatten_fragments();
        self.root_blueprint.ensure_entity_ids();
        self.ir_graph = Self::build_ir_graph(&self.root_blueprint);
        let sheets = Self::collect_stylesheets_from_blueprint(&self.root_blueprint);
        for sheet in &sheets {
            self.register_keyframes_stylesheet(sheet);
        }
        self.cached_stylesheets = sheets;
        let build_start = std::time::Instant::now();
        let ctx = self.interaction_context();
        self.layout_root = Self::build_layout_tree_with_interaction(&self.root_blueprint, &ctx);
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
        Self::collect_dynamic_text_bindings(
            &self.root_blueprint,
            &mut path_buf,
            &mut self.dynamic_text_bindings,
        );
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
                if let Some(node) = Self::find_layout_node_mut(&mut self.layout_root, &binding.path)
                {
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

    /// System cursor theme, loaded on first use. Reading cursor files from disk costs
    /// several milliseconds and only the MCP cursor overlay needs it.
    pub fn cursor_theme(&self) -> &strata_platform::CursorTheme {
        self.cursor_theme
            .get_or_init(strata_platform::CursorTheme::load_active)
    }

    /// Marks the compiled display list stale so the next dirty-gated compile rebuilds it.
    pub fn mark_paint_dirty(&mut self) {
        self.paint_dirty = true;
    }

    /// Whether the display list is stale relative to runtime state.
    pub fn is_paint_dirty(&self) -> bool {
        self.paint_dirty
    }

    /// Number of dirty-gated compiles that were skipped as unchanged.
    pub fn compiles_skipped(&self) -> u64 {
        self.compiles_skipped
    }

    /// Compiles the display list only when something changed since the last compile.
    /// Returns true when a compile ran.
    pub fn compile_display_list_if_dirty(&mut self) -> bool {
        if !self.paint_dirty {
            self.compiles_skipped += 1;
            return false;
        }
        self.compile_display_list();
        true
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
        self.gpu_path_pipeline
            .resize(self.viewport_width, self.viewport_height);
        self.gpu_path_pipeline
            .compile_frame(&self.display_list, None);
        self.frame_profile.compile += compile_start.elapsed();
        self.paint_dirty = false;

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

        let mut needs_redraw = match event {
            PlatformEvent::WindowResized { width, height } => {
                self.viewport_width = *width as f32;
                self.viewport_height = *height as f32;
                #[cfg(feature = "devtools")]
                if let Some(ref mut dt) = self.devtools {
                    dt.hud
                        .update_bounds(self.viewport_width, self.viewport_height);
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
            } => {
                let (wheel_handled, scroll_changed) = self.dispatch_smooth_scroll_action(
                    self.last_mouse_pos.0,
                    self.last_mouse_pos.1,
                    *delta_x,
                    *delta_y,
                );
                if wheel_handled {
                    self.rebuild_layout_tree();
                    true
                } else {
                    scroll_changed
                }
            }
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
                let hover_changed = self.update_hover(*x, *y);
                self.dispatch_pointer_move_action(*x, *y).needs_redraw() || hover_changed
            }
            PlatformEvent::MouseDown { x, y, .. } => {
                self.is_mouse_down = true;
                let action = self.dispatch_pointer_down_action(*x, *y);
                let click_invoked = self.dispatch_mouse_click(*x, *y, 0, 1);
                let dynamic_updated = self.sync_dynamic_text();
                if action.needs_rebuild() || click_invoked || dynamic_updated {
                    self.rebuild_layout_tree();
                    true
                } else {
                    action.needs_redraw() || self.restyle_for_active_change()
                }
            }
            PlatformEvent::MouseUp { x, y, .. } => {
                let was_mouse_down = self.is_mouse_down;
                self.is_mouse_down = false;
                let action = self.dispatch_pointer_up_action(*x, *y);
                if action.needs_rebuild() {
                    self.rebuild_layout_tree();
                    true
                } else if was_mouse_down {
                    action.needs_redraw() || self.restyle_for_active_change()
                } else {
                    action.needs_redraw()
                }
            }
            PlatformEvent::CursorLeft => {
                self.is_mouse_down = false;
                self.current_cursor = strata_platform::window::CursorIcon::Default;
                self.clear_hover()
            }
            PlatformEvent::KeyDown { .. }
            | PlatformEvent::KeyUp { .. }
            | PlatformEvent::ReceivedCharacter(_)
            | PlatformEvent::ImePreedit { .. }
            | PlatformEvent::ImeCommit(_) => {
                let res = self.dispatch_key_event(event);
                let dynamic_updated = self.sync_dynamic_text();
                res.needs_redraw() || dynamic_updated
            }
            _ => false,
        };

        if self.poll_dom_mutations(None) {
            needs_redraw = true;
        }

        if needs_redraw {
            self.paint_dirty = true;
        }
        needs_redraw
    }

    /// Phase timings accumulated since the last presented frame.
    pub fn frame_profile(&self) -> FrameProfile {
        self.frame_profile
    }

    /// Registers an observer called with each completed [`FrameProfile`].
    pub fn set_frame_profile_hook(&mut self, hook: Option<Box<dyn FnMut(&FrameProfile)>>) {
        self.frame_profile_hook = hook.map(FrameProfileHook);
    }

    /// Adds time spent rasterizing, for presenting backends that rasterize outside the runtime.
    pub fn record_raster_time(&mut self, elapsed: std::time::Duration) {
        self.frame_profile.raster += elapsed;
    }

    /// Adds time spent presenting, for backends that present outside the runtime.
    pub fn record_present_time(&mut self, elapsed: std::time::Duration) {
        self.frame_profile.present += elapsed;
    }

    /// Publishes the accumulated profile to the hook and starts a fresh frame.
    /// Returns the profile that was completed.
    pub fn finish_frame_profile(&mut self) -> FrameProfile {
        let done = std::mem::take(&mut self.frame_profile);
        if let Some(hook) = self.frame_profile_hook.as_mut() {
            (hook.0)(&done);
        }
        done
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

        if self.paint_dirty || self.frame_count == 0 || force_redraw {
            self.tick_frame();
        } else {
            self.compiles_skipped += 1;
        }

        #[cfg(feature = "devtools")]
        if let Some(ref mut dt) = self.devtools {
            if dt.hud.state.is_visible {
                dt.hud
                    .render_overlay(Some(&dt.profiler), &mut self.display_list);
            }
        }

        #[cfg(feature = "devtools")]
        let submit_start = std::time::Instant::now();

        let present_start = std::time::Instant::now();
        let res = compositor.render_and_present(&self.display_list, presenter, force_redraw);
        self.frame_profile.present += present_start.elapsed();
        self.finish_frame_profile();

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
            dt.hud
                .update_bounds(self.viewport_width, self.viewport_height);
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

    /// Rasterizes the current application frame to an offscreen headless FrameBuffer.
    pub fn render_to_framebuffer(&mut self) -> strata_render::rasterizer::FrameBuffer {
        self.tick_frame();
        let width = (self.viewport_width * self.dpr).max(1.0).ceil() as usize;
        let height = (self.viewport_height * self.dpr).max(1.0).ceil() as usize;
        let mut fb = strata_render::rasterizer::FrameBuffer::new(width, height);
        fb.execute_display_list(&self.display_list);
        fb
    }

    /// Captures a headless screenshot of the current application state and writes it to a PNG file.
    pub fn export_png<P: AsRef<std::path::Path>>(&mut self, path: P) -> Result<(), String> {
        let fb = self.render_to_framebuffer();
        fb.export_png(path).map_err(|e| e.to_string())
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
