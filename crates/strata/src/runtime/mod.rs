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
#[cfg(feature = "hot-reload")]
pub mod hmr_bridge;
pub mod launch;
pub mod media_host;
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
pub mod test_support;

pub use app::*;
pub use dom_hydration::*;
pub use edit::*;
#[cfg(feature = "hot-reload")]
pub use hmr_bridge::*;
pub(crate) use image::*;
pub use launch::*;
pub use media_host::*;
pub use platforms::*;
pub use profile::{
    take_frame_heap_alloc_delta, total_heap_alloc_stats, FrameProfile, FrameProfileHistory,
    FrameProfileHook, FrameProfileRecord, PerfPercentileStats, PerfProfileSummary,
    ProfilingAllocator,
};
pub use spatial::*;
pub use test_support::{MatchMode, NodeQuery};
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
    /// Resolved native tooltip text (`title` or button/image `alt`) for the hovered element.
    pub hover_tooltip_text: Option<String>,
    /// Tree path of the element that owns `hover_tooltip_text`.
    pub hover_tooltip_path: Option<Vec<usize>>,
    /// Pointer coordinates used to anchor the native hover tooltip.
    pub hover_tooltip_anchor: (f32, f32),
    /// Timestamp when pointer dwell began on the current tooltip target.
    pub hover_tooltip_since: Option<std::time::Instant>,
    /// Whether the native hover tooltip popup is currently displayed.
    pub hover_tooltip_visible: bool,
    /// Dwell delay before a native hover tooltip becomes visible.
    pub hover_tooltip_delay: std::time::Duration,
    /// Visual styling and offset configuration for native hover tooltips.
    pub hover_tooltip_style: strata_platform::HoverTooltipStyle,
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
    /// Previous frame's solved border-box `(width, height)` keyed by animation node ID
    /// so `Auto <-> 0px` transitions can interpolate smoothly between concrete pixel sizes.
    pub previous_node_sizes: std::collections::HashMap<u64, (f32, f32)>,
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
    /// True when only retained 3D state changed without invalidating 2D layout or display list.
    pub(crate) viewport_3d_dirty: bool,
    /// Retained 3D scene handles in the compiled display list paired with last presented generations.
    pub(crate) tracked_retained_3d: Vec<(strata_render::scene3d::RetainedScene3D, (u64, u64, u64))>,
    /// Number of display-list compilations skipped because nothing changed.
    pub(crate) compiles_skipped: u64,
    /// Optional observer invoked with the completed profile after each present.
    pub(crate) frame_profile_hook: Option<FrameProfileHook>,
    /// Cached `<style>` stylesheets collected from `root_blueprint`.
    pub(crate) cached_stylesheets: Vec<std::sync::Arc<strata_style::StyleSheet>>,
    /// Precomputed Bloom-filter and exact tag/class/id dependency index over `cached_stylesheets`.
    pub(crate) stylesheet_index: StylesheetSelectorIndex,
    /// Cached document-root inherited style (`:root` / `html` / `body` custom properties & defaults).
    pub(crate) cached_root_inherited_style: Option<ComputedStyle>,
    /// Whether the active rendering backend consumes compiled 2D GPU path segments.
    /// Defaults to `strata_platform::gtk_gpu_2d_enabled()` so Cairo software 2D frames
    /// skip redundant path tessellation.
    pub(crate) gpu_2d_paths_enabled: bool,
    /// Active pointer capture or drag target `(tree_path, explicit_capture)`.
    pub pointer_capture_target: Option<(Vec<usize>, bool)>,
    /// True when the current `root_blueprint` contains any `EventType::Resize` listeners.
    pub(crate) has_resize_listeners: bool,
    /// Last observed `(width, height)` for elements with `onresize` listeners, keyed by node ID.
    pub observed_element_sizes: std::collections::HashMap<u64, (f32, f32)>,
    /// Last pointer position `(x, y)` for computing `movement_x` / `movement_y`.
    pub(crate) last_pointer_pos: Option<(f32, f32)>,
    /// Target `(x, y)` of an in-flight cursor warp so synthetic warp deltas are filtered.
    pub(crate) pending_cursor_warp: Option<(f32, f32)>,
    /// True when the most recent `update_view` call reconciled the existing layout tree in place.
    pub(crate) last_update_reconciled: bool,
    /// Per-update reconciliation subtree reuse and node rebuild counters.
    pub(crate) last_reconcile_stats: ReconcileStats,
    /// Reason the most recent `update_view` fell back to a full layout tree rebuild, if any.
    pub(crate) last_reconcile_bailout: Option<String>,
    /// True when the most recent paint-only hover update succeeded via `restyle_hover_subtrees_fast`.
    pub(crate) last_hover_fast_path_used: bool,
    /// Reason the most recent paint-only hover update bailed out of `restyle_hover_subtrees_fast`, if any.
    pub(crate) last_hover_fast_path_bailout: Option<String>,
    /// Cumulative count of full layout tree rebuilds across the runtime's lifetime.
    pub(crate) full_rebuilds_count: u64,
    /// Cumulative count of incremental reconciled view updates across the runtime's lifetime.
    pub(crate) reconciled_updates_count: u64,
    /// Structured attribution for why the most recent frame ran work.
    pub(crate) last_rebuild_reason: RebuildReason,
    /// Monotonic epoch recorded when this `AppRuntime` was instantiated (anchors Perfetto timestamps).
    pub(crate) runtime_epoch: std::time::Instant,
    /// Rolling ring buffer of real per-frame telemetry records for host HUDs & MCP profiling gates.
    pub(crate) profile_history: FrameProfileHistory,
    /// Previous frame's top-level layout rectangles used to compute real Cumulative Layout Shift (CLS).
    pub(crate) prev_layout_cls_rects: Vec<(f32, f32, f32, f32)>,
    /// Active native DOM drag-and-drop operation (`ondragstart`, `ondragover`, `ondrop`, etc.).
    pub(crate) active_dom_drag: Option<ActiveDomDrag>,
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
    /// Builds a root [`View`] inside an implicit root component scope (when no outer `#[component]`
    /// scope is active) and mounts a new [`AppRuntime`].
    ///
    /// Prefer this over [`Self::new`] when rendering plain closures containing unkeyed top-level
    /// sibling components (`|| rsx! { <> <Child /> <Child /> </> }`) so siblings receive distinct
    /// hook slots and run `use_effect` unmount cleanups when toggled off.
    pub fn from_root<F: FnOnce() -> View>(
        root: F,
        viewport_width: f32,
        viewport_height: f32,
        dpr: f32,
    ) -> Self {
        let root_view = strata_dom::with_root_component_scope(root);
        Self::new(root_view, viewport_width, viewport_height, dpr)
    }

    /// Creates and mounts a new application runtime from a root view.
    pub fn new(root_view: View, viewport_width: f32, viewport_height: f32, dpr: f32) -> Self {
        static REGISTER_JS_FETCH: std::sync::Once = std::sync::Once::new();
        REGISTER_JS_FETCH.call_once(|| {
            strata_js::web::set_global_fetch_handler(std::sync::Arc::new(|req| {
                let mut web_req = strata_web_api::fetch::RequestInit {
                    method: req.method,
                    ..Default::default()
                };
                for (k, v) in req.headers {
                    web_req.headers.set(&k, &v);
                }
                if let Some(body) = req.body {
                    web_req = web_req.body(body);
                }
                let resp = strata_web_api::fetch::fetch(&req.url, Some(web_req))
                    .map_err(|e| e.to_string())?;
                Ok(strata_js::web::FetchResponse {
                    status: resp.status,
                    status_text: resp.status_text.clone(),
                    headers: resp
                        .headers
                        .all_entries()
                        .into_iter()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect(),
                    body: resp.bytes().to_vec(),
                })
            }));
        });

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
            hover_tooltip_text: None,
            hover_tooltip_path: None,
            hover_tooltip_anchor: (0.0, 0.0),
            hover_tooltip_since: None,
            hover_tooltip_visible: false,
            hover_tooltip_delay: std::time::Duration::from_millis(450),
            hover_tooltip_style: strata_platform::HoverTooltipStyle::default(),
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
            previous_node_sizes: std::collections::HashMap::new(),
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
            viewport_3d_dirty: false,
            tracked_retained_3d: Vec::new(),
            compiles_skipped: 0,
            frame_profile_hook: None,
            cached_stylesheets: Vec::new(),
            stylesheet_index: StylesheetSelectorIndex::default(),
            cached_root_inherited_style: None,
            gpu_2d_paths_enabled: strata_platform::gtk_gpu_2d_enabled(),
            pointer_capture_target: None,
            has_resize_listeners: false,
            observed_element_sizes: std::collections::HashMap::new(),
            last_pointer_pos: None,
            pending_cursor_warp: None,
            last_update_reconciled: false,
            last_reconcile_stats: ReconcileStats::default(),
            last_reconcile_bailout: None,
            last_hover_fast_path_used: false,
            last_hover_fast_path_bailout: None,
            full_rebuilds_count: 1,
            reconciled_updates_count: 0,
            last_rebuild_reason: RebuildReason::InitialMount,
            runtime_epoch: std::time::Instant::now(),
            profile_history: FrameProfileHistory::default(),
            prev_layout_cls_rects: Vec::new(),
            active_dom_drag: None,
        };

        runtime.has_resize_listeners =
            Self::blueprint_has_resize_listeners(&runtime.root_blueprint);
        let initial_sheets = Self::collect_stylesheets_from_blueprint(&runtime.root_blueprint);
        for sheet in &initial_sheets {
            runtime.register_keyframes_stylesheet(sheet);
        }
        runtime.stylesheet_index = StylesheetSelectorIndex::from_stylesheets(&initial_sheets);
        runtime.cached_root_inherited_style = if !initial_sheets.is_empty() {
            let ctx = runtime.interaction_context();
            Some(Self::resolve_document_root_inherited_style(
                &runtime.root_blueprint,
                &initial_sheets,
                &ctx,
            ))
        } else {
            None
        };
        runtime.cached_stylesheets = initial_sheets;
        runtime.sync_tree_animations(0.0);
        runtime.apply_viewport_clamps();
        runtime.apply_scroll_offsets();
        runtime.dispatch_resize_observers();
        runtime
            .spatial_index
            .rebuild(&runtime.root_blueprint, &runtime.layout_root);
        runtime.update_accessibility_tree();
        runtime.compile_display_list();
        runtime
    }

    /// Applies runtime-relevant preferences from a [`WindowConfig`] (such as native hover
    /// tooltip dwell delay and styling).
    pub fn apply_window_config(&mut self, config: &WindowConfig) -> &mut Self {
        self.hover_tooltip_delay = config.tooltip_delay;
        self.hover_tooltip_style = config.tooltip_style;
        if self.hover_tooltip_visible {
            self.paint_dirty = true;
        }
        self
    }

    /// Sets the pointer dwell delay before native hover tooltips (`title` / `alt`) become visible.
    pub fn set_hover_tooltip_delay(&mut self, delay: std::time::Duration) -> &mut Self {
        self.hover_tooltip_delay = delay;
        self
    }

    /// Sets the pointer dwell delay in milliseconds before native hover tooltips become visible.
    pub fn set_hover_tooltip_delay_ms(&mut self, delay_ms: u64) -> &mut Self {
        self.hover_tooltip_delay = std::time::Duration::from_millis(delay_ms);
        self
    }

    /// Returns the configured pointer dwell delay for native hover tooltips.
    pub fn hover_tooltip_delay(&self) -> std::time::Duration {
        self.hover_tooltip_delay
    }

    /// Sets visual styling and offset configuration for native hover tooltips.
    pub fn set_hover_tooltip_style(
        &mut self,
        style: strata_platform::HoverTooltipStyle,
    ) -> &mut Self {
        self.hover_tooltip_style = style;
        if self.hover_tooltip_visible {
            self.paint_dirty = true;
        }
        self
    }

    /// Returns the active visual styling configuration for native hover tooltips.
    pub fn hover_tooltip_style(&self) -> strata_platform::HoverTooltipStyle {
        self.hover_tooltip_style
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
        self.sync_active_input_layout();
        self.dispatch_resize_observers();
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

    /// Queries the laid-out border-box rectangle `(x, y, width, height)` for an element by `id`.
    pub fn query_element_rect(&self, element_id: &str) -> Option<strata_layout::Rect> {
        fn find_rec(node: &LayoutNode, target_id: &str) -> Option<strata_layout::Rect> {
            if node.element_id.as_deref() == Some(target_id) {
                return Some(node.geometry.border_box);
            }
            for child in &node.children {
                if let Some(rect) = find_rec(child, target_id) {
                    return Some(rect);
                }
            }
            None
        }
        find_rec(&self.layout_root, element_id)
    }

    /// Warps the OS pointer to window-local `(x, y)` and records the target so the
    /// resulting synthetic motion delta is filtered from `movement_x` / `movement_y`.
    pub fn warp_cursor(&mut self, x: f32, y: f32) {
        self.last_pointer_pos = Some((x, y));
        self.pending_cursor_warp = Some((x, y));
        #[cfg(target_os = "linux")]
        strata_platform::gtk_warp_cursor(x as f64, y as f64);
    }

    /// Alias for [`AppRuntime::warp_cursor`].
    pub fn set_cursor_position(&mut self, x: f32, y: f32) {
        self.warp_cursor(x, y);
    }

    /// Updates `flex_grow`, `flex_shrink`, and optional `flex_basis` on the layout node
    /// matching `element_id` in-place, invalidating only its ancestor chain's layout cache.
    pub fn update_element_flex(
        &mut self,
        element_id: &str,
        flex_grow: f32,
        flex_shrink: f32,
        flex_basis: Option<strata_style::types::Dimension>,
    ) -> bool {
        fn update_rec(
            node: &mut LayoutNode,
            target_id: &str,
            flex_grow: f32,
            flex_shrink: f32,
            flex_basis: Option<strata_style::types::Dimension>,
        ) -> bool {
            if node.element_id.as_deref() == Some(target_id) {
                node.style.flex_grow = flex_grow;
                node.style.flex_shrink = flex_shrink;
                if let Some(basis) = flex_basis {
                    node.style.flex_basis = basis;
                }
                node.invalidate_self_caches();
                return true;
            }
            let mut changed = false;
            for child in &mut node.children {
                if update_rec(child, target_id, flex_grow, flex_shrink, flex_basis) {
                    changed = true;
                }
            }
            if changed {
                node.invalidate_self_caches();
            }
            changed
        }
        update_rec(
            &mut self.layout_root,
            element_id,
            flex_grow,
            flex_shrink,
            flex_basis,
        )
    }

    /// Updates `flex_grow` ratios for multiple `element_id`s in-place and runs an isolated
    /// layout-only pass without rebuilding the DOM, stylesheets, or IR graph.
    pub fn update_split_ratios(&mut self, updates: &[(&str, f32)]) -> bool {
        let mut any_changed = false;
        for &(id, ratio) in updates {
            if self.update_element_flex(id, ratio, 1.0, None) {
                any_changed = true;
            }
        }
        if any_changed {
            self.relayout_in_place();
        }
        any_changed
    }

    /// Recomputes layout, scroll/viewport clamps, resize observers, spatial index, and
    /// display list in-place without rebuilding `layout_root` or `ir_graph`.
    pub fn relayout_in_place(&mut self) {
        self.unapply_scroll_offsets();
        let compute_start = std::time::Instant::now();
        self.layout_root
            .compute_layout(self.viewport_width, self.viewport_height);
        self.layout_computations += 1;
        self.frame_profile.layout += compute_start.elapsed();
        self.apply_viewport_clamps();
        self.apply_scroll_offsets();
        self.dispatch_resize_observers();
        self.spatial_index
            .rebuild(&self.root_blueprint, &self.layout_root);
        self.paint_dirty = true;
        self.compile_display_list();
    }

    /// Returns true if any element in `bp` registers an `EventType::Resize` listener.
    pub(crate) fn blueprint_has_resize_listeners(bp: &NodeBlueprint) -> bool {
        match bp {
            NodeBlueprint::Element {
                attributes,
                children,
                ..
            } => {
                attributes.iter().any(|a| {
                    matches!(
                        a,
                        strata_dom::Attribute::EventListener {
                            event: strata_dom::EventType::Resize,
                            ..
                        }
                    )
                }) || children.iter().any(Self::blueprint_has_resize_listeners)
            }
            NodeBlueprint::Fragment(children) => {
                children.iter().any(Self::blueprint_has_resize_listeners)
            }
            _ => false,
        }
    }

    /// Dispatches `EventType::Resize` (`onresize`) callbacks for any element whose laid-out
    /// `(width, height)` changed since the last observation. Returns `true` if any callback fired.
    pub fn dispatch_resize_observers(&mut self) -> bool {
        if !self.has_resize_listeners {
            return false;
        }
        let mut pending: Vec<(std::rc::Rc<dyn Fn(strata_dom::Event)>, strata_layout::Rect)> =
            Vec::new();
        let mut active_ids = std::collections::HashSet::new();
        let mut path = Vec::new();

        #[allow(clippy::too_many_arguments)]
        fn collect_rec(
            bp: &NodeBlueprint,
            layout: &LayoutNode,
            path: &mut Vec<usize>,
            path_node_ids: &std::collections::HashMap<Vec<usize>, u64>,
            observed: &mut std::collections::HashMap<u64, (f32, f32)>,
            active_ids: &mut std::collections::HashSet<u64>,
            pending: &mut Vec<(std::rc::Rc<dyn Fn(strata_dom::Event)>, strata_layout::Rect)>,
        ) {
            if let NodeBlueprint::Element {
                attributes,
                children,
                ..
            } = bp
            {
                let node_id = path_node_ids
                    .get(path)
                    .copied()
                    .unwrap_or_else(|| path_to_node_id(path));
                let mut resize_handlers = Vec::new();
                for attr in attributes {
                    if let strata_dom::Attribute::EventListener {
                        event: strata_dom::EventType::Resize,
                        handler,
                    } = attr
                    {
                        resize_handlers.push(handler.clone());
                    }
                }
                if !resize_handlers.is_empty() {
                    active_ids.insert(node_id);
                    let w = layout.geometry.border_box.width;
                    let h = layout.geometry.border_box.height;
                    let changed = match observed.get(&node_id) {
                        Some(&(pw, ph)) => (w - pw).abs() > 0.25 || (h - ph).abs() > 0.25,
                        None => true,
                    };
                    if changed {
                        observed.insert(node_id, (w, h));
                        for handler in resize_handlers {
                            pending.push((handler, layout.geometry.border_box));
                        }
                    }
                }
                for (idx, child_layout) in layout.children.iter().enumerate() {
                    if let Some(child_bp) = children.get(idx) {
                        path.push(idx);
                        collect_rec(
                            child_bp,
                            child_layout,
                            path,
                            path_node_ids,
                            observed,
                            active_ids,
                            pending,
                        );
                        path.pop();
                    }
                }
            }
        }

        collect_rec(
            &self.root_blueprint,
            &self.layout_root,
            &mut path,
            &self.path_node_ids,
            &mut self.observed_element_sizes,
            &mut active_ids,
            &mut pending,
        );
        self.observed_element_sizes
            .retain(|id, _| active_ids.contains(id));

        if pending.is_empty() {
            return false;
        }
        for (handler, rect) in pending {
            let mut ev = strata_dom::Event::new(strata_dom::EventType::Resize);
            ev.client_x = rect.x as f64;
            ev.client_y = rect.y as f64;
            ev.offset_x = rect.width as f64;
            ev.offset_y = rect.height as f64;
            ev.width = rect.width as f64;
            ev.height = rect.height as f64;
            handler(ev);
        }
        true
    }

    fn element_attr_value<'a>(
        attrs: &'a [strata_dom::Attribute],
        key_name: &str,
    ) -> Option<&'a str> {
        attrs.iter().find_map(|a| match a {
            strata_dom::Attribute::Property { key, value } if key == key_name => {
                Some(value.as_str())
            }
            _ => None,
        })
    }

    fn attributes_visually_equal(a: &[strata_dom::Attribute], b: &[strata_dom::Attribute]) -> bool {
        let mut iter_a = a.iter().filter(|attr| {
            !matches!(
                attr,
                strata_dom::Attribute::EventListener { .. } | strata_dom::Attribute::Entity(_)
            )
        });
        let mut iter_b = b.iter().filter(|attr| {
            !matches!(
                attr,
                strata_dom::Attribute::EventListener { .. } | strata_dom::Attribute::Entity(_)
            )
        });
        loop {
            match (iter_a.next(), iter_b.next()) {
                (None, None) => return true,
                (Some(x), Some(y)) => match (x, y) {
                    (
                        strata_dom::Attribute::Property { key: k1, value: v1 },
                        strata_dom::Attribute::Property { key: k2, value: v2 },
                    ) if k1 == k2 && v1 == v2 => {}
                    (
                        strata_dom::Attribute::DirectStyle(d1),
                        strata_dom::Attribute::DirectStyle(d2),
                    ) if std::rc::Rc::ptr_eq(d1, d2) => {}
                    (
                        strata_dom::Attribute::ScenePayload(s1),
                        strata_dom::Attribute::ScenePayload(s2),
                    ) if std::sync::Arc::ptr_eq(s1, s2) => {}
                    _ => return false,
                },
                _ => return false,
            }
        }
    }

    fn inherited_style_equal(
        a: &strata_style::ComputedStyle,
        b: &strata_style::ComputedStyle,
    ) -> bool {
        a.color == b.color
            && a.font_family == b.font_family
            && a.font_size == b.font_size
            && a.root_font_size == b.root_font_size
            && a.font_weight == b.font_weight
            && a.font_style == b.font_style
            && a.text_align == b.text_align
            && a.line_height == b.line_height
            && a.letter_spacing == b.letter_spacing
            && a.text_decoration == b.text_decoration
            && a.text_transform == b.text_transform
            && a.text_overflow == b.text_overflow
            && a.white_space == b.white_space
            && a.cursor == b.cursor
            && a.user_select == b.user_select
            && a.pointer_events == b.pointer_events
            && a.visibility == b.visibility
            && a.caret_color == b.caret_color
            && a.border_collapse == b.border_collapse
            && a.border_spacing == b.border_spacing
            && a.caption_side == b.caption_side
            && a.custom_properties == b.custom_properties
    }

    fn blueprint_visually_equal(a: &NodeBlueprint, b: &NodeBlueprint) -> bool {
        match (a, b) {
            (NodeBlueprint::Text(t1), NodeBlueprint::Text(t2)) => t1 == t2,
            (NodeBlueprint::Empty, NodeBlueprint::Empty) => true,
            (
                NodeBlueprint::Element {
                    tag: t1,
                    classes: cls1,
                    attributes: a1,
                    children: c1,
                },
                NodeBlueprint::Element {
                    tag: t2,
                    classes: cls2,
                    attributes: a2,
                    children: c2,
                },
            ) => {
                t1 == t2
                    && cls1 == cls2
                    && c1.len() == c2.len()
                    && Self::attributes_visually_equal(a1, a2)
                    && c1
                        .iter()
                        .zip(c2.iter())
                        .all(|(x, y)| Self::blueprint_visually_equal(x, y))
            }
            _ => false,
        }
    }

    /// Reconciles `layout_node` in-place from `old_bp` to `new_bp`, preserving W3C CSS property
    /// inheritance top-down from `parent_style`.
    /// Returns `None` if a structural mismatch requires a full subtree rebuild,
    /// `Some(true)` if any layout cache in this subtree was invalidated, or
    /// Materializes `CascadeElementMeta` for a single blueprint element at `node_path` on demand
    /// (called only when a node actually needs style re-resolution during reconciliation or fast hover).
    pub(crate) fn materialize_cascade_meta(
        bp: &NodeBlueprint,
        node_path: &[usize],
        ctx: &crate::runtime::InteractionContext,
        group_state: (bool, bool, bool),
    ) -> Option<CascadeElementMeta> {
        let NodeBlueprint::Element {
            tag,
            classes,
            attributes,
            ..
        } = bp
        else {
            return None;
        };
        if Self::is_style_tag(tag) {
            return None;
        }
        let is_hovered = ctx.hovered_path.is_some_and(|h| h.starts_with(node_path));
        let is_focused = ctx.focused_path.is_some_and(|f| f.starts_with(node_path));
        let is_active = is_hovered && ctx.is_mouse_down;
        let is_disabled = attributes.iter().any(|attr| match attr {
            strata_dom::Attribute::Property { key, value } => {
                key.eq_ignore_ascii_case("disabled") && !value.eq_ignore_ascii_case("false")
            }
            _ => false,
        });
        let (class_vec, id_val, attr_vec) = Self::extract_element_selector_meta(classes, attributes);
        Some(CascadeElementMeta {
            tag: Self::tag_as_str(tag),
            classes: class_vec,
            id: id_val,
            attributes: attr_vec,
            pseudo_state: strata_style::selector::PseudoState {
                is_hovered,
                is_focused,
                is_active,
                is_disabled,
                is_dark: ctx.is_dark_mode,
                is_group_hovered: group_state.0,
                is_group_focused: group_state.1,
                is_group_active: group_state.2,
            },
        })
    }

    pub(crate) fn materialize_ancestor_metas(
        ancestor_stack: &[(&NodeBlueprint, (bool, bool, bool))],
        path_buf: &[usize],
        ctx: &crate::runtime::InteractionContext,
        stylesheets: &[std::sync::Arc<strata_style::StyleSheet>],
        sheet_index: &StylesheetSelectorIndex,
    ) -> Vec<CascadeElementMeta> {
        if stylesheets.is_empty() || !sheet_index.has_descendant_combinator_rules {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(ancestor_stack.len());
        for (depth_idx, &(anc_bp, anc_group)) in ancestor_stack.iter().enumerate() {
            let anc_path = &path_buf[..depth_idx.min(path_buf.len())];
            if let Some(meta) = Self::materialize_cascade_meta(anc_bp, anc_path, ctx, anc_group) {
                out.push(meta);
            }
        }
        out
    }

    pub(crate) fn materialize_preceding_sibling_metas(
        siblings: &[NodeBlueprint],
        child_idx: usize,
        parent_path: &[usize],
        ctx: &crate::runtime::InteractionContext,
        group_state: (bool, bool, bool),
        stylesheets: &[std::sync::Arc<strata_style::StyleSheet>],
        sheet_index: &StylesheetSelectorIndex,
    ) -> Vec<CascadeElementMeta> {
        if stylesheets.is_empty()
            || !sheet_index.has_sibling_combinator_or_positional_rules
            || child_idx == 0
        {
            return Vec::new();
        }
        let mut sib_path = Vec::with_capacity(parent_path.len() + 1);
        sib_path.extend_from_slice(parent_path);
        sib_path.push(0);
        let mut out = Vec::with_capacity(child_idx);
        for (j, sib_bp) in siblings[..child_idx].iter().enumerate() {
            if let Some(last) = sib_path.last_mut() {
                *last = j;
            }
            if let Some(meta) = Self::materialize_cascade_meta(sib_bp, &sib_path, ctx, group_state) {
                out.push(meta);
            }
        }
        out
    }

    /// Incrementally reconciles `layout_node` in place against `old_bp` and `new_bp`.
    ///
    /// Supports static `<style>` sheets (`:root` variables, class/tag/combinator rules),
    /// active `:focus`/`:hover` interaction paths, and `group-*` markers while keeping
    /// unchanged subtrees O(1) zero-allocation.
    /// Returns `None` if this node cannot be reconciled in place (caller should rebuild it),
    /// `Some(true)` if any style or descendant changed (layout cache invalidated), or
    /// `Some(false)` if this entire subtree is visually and structurally identical.
    #[allow(clippy::too_many_arguments)]
    fn reconcile_layout_subtree<'a>(
        old_bp: &NodeBlueprint,
        new_bp: &'a NodeBlueprint,
        layout_node: &mut LayoutNode,
        parent_style: Option<&strata_style::ComputedStyle>,
        parent_inherited_changed: bool,
        ancestor_selector_changed: bool,
        force_restyle_self: bool,
        path_buf: &mut Vec<usize>,
        ancestor_stack: &mut Vec<(&'a NodeBlueprint, (bool, bool, bool))>,
        sibling_bps: &[NodeBlueprint],
        sibling_idx: usize,
        is_last_element_child: bool,
        group_state: (bool, bool, bool),
        stylesheets: &[std::sync::Arc<strata_style::StyleSheet>],
        sheet_index: &StylesheetSelectorIndex,
        ctx: &crate::runtime::InteractionContext,
        stats: &mut ReconcileStats,
    ) -> Option<bool> {
        match (old_bp, new_bp) {
            (NodeBlueprint::Text(t1), NodeBlueprint::Text(t2)) => {
                if t1 == t2 && !parent_inherited_changed && !force_restyle_self {
                    stats.reused_subtrees += 1;
                    Some(false)
                } else {
                    *layout_node = Self::build_text_leaf_layout_node(t2.clone(), parent_style);
                    stats.rebuilt_nodes += 1;
                    Some(true)
                }
            }
            (NodeBlueprint::Empty, NodeBlueprint::Empty) => {
                stats.reused_subtrees += 1;
                Some(false)
            }
            (
                NodeBlueprint::Element { tag: t1, .. },
                NodeBlueprint::Element { tag: t2, .. },
            ) if t1 == t2 && Self::is_style_tag(t1) => {
                if let Some(eid) = new_bp.entity_id() {
                    layout_node.entity_id = eid;
                }
                stats.reused_subtrees += 1;
                Some(false)
            }
            (
                NodeBlueprint::Element { tag: t1, .. },
                NodeBlueprint::Element { tag: t2, .. },
            ) if t1 == t2
                && matches!(
                    t1,
                    strata_dom::Tag::Svg
                        | strata_dom::Tag::Path
                        | strata_dom::Tag::Input
                        | strata_dom::Tag::Textarea
                        | strata_dom::Tag::Img
                        | strata_dom::Tag::Canvas
                        | strata_dom::Tag::Custom("Canvas3D")
                ) =>
            {
                if !parent_inherited_changed
                    && !ancestor_selector_changed
                    && !force_restyle_self
                    && Self::blueprint_visually_equal(old_bp, new_bp)
                {
                    if let Some(eid) = new_bp.entity_id() {
                        layout_node.entity_id = eid;
                    }
                    stats.reused_subtrees += 1;
                    Some(false)
                } else {
                    None
                }
            }
            (
                NodeBlueprint::Element {
                    tag: t1,
                    classes: cls1,
                    attributes: a1,
                    children: c1,
                },
                NodeBlueprint::Element {
                    tag: t2,
                    classes: cls2,
                    attributes: a2,
                    children: c2,
                },
            ) if t1 == t2
                && !layout_node.is_top_layer
                && layout_node.children.len() == c1.len()
                && Self::element_attr_value(a1, "key") == Self::element_attr_value(a2, "key")
                && Self::element_attr_value(a1, "id") == Self::element_attr_value(a2, "id") =>
            {
                let self_props_changed = cls1 != cls2 || !Self::attributes_visually_equal(a1, a2);
                let single_text_changed = c1.len() == 1
                    && c2.len() == 1
                    && matches!(
                        (&c1[0], &c2[0]),
                        (NodeBlueprint::Text(x), NodeBlueprint::Text(y)) if x != y
                    );

                let (self_rewritten, child_inherited_changed) = if self_props_changed
                    || parent_inherited_changed
                    || ancestor_selector_changed
                    || force_restyle_self
                    || single_text_changed
                {
                    let shallow_bp = NodeBlueprint::Element {
                        tag: *t2,
                        classes: cls2.clone(),
                        attributes: a2.clone(),
                        children: Vec::new(),
                    };
                    let mut ancestors_meta = Self::materialize_ancestor_metas(
                        ancestor_stack,
                        path_buf,
                        ctx,
                        stylesheets,
                        sheet_index,
                    );
                    let parent_path_len = path_buf.len().saturating_sub(1);
                    let preceding_sibs = if !path_buf.is_empty() {
                        Self::materialize_preceding_sibling_metas(
                            sibling_bps,
                            sibling_idx,
                            &path_buf[..parent_path_len],
                            ctx,
                            group_state,
                            stylesheets,
                            sheet_index,
                        )
                    } else {
                        Vec::new()
                    };
                    let mut updated = Self::build_layout_tree_full(
                        &shallow_bp,
                        parent_style,
                        path_buf,
                        ctx,
                        stylesheets,
                        &mut ancestors_meta,
                        &preceding_sibs,
                        is_last_element_child,
                        group_state,
                    );
                    Self::apply_single_text_inline_intrinsic_sizing(&mut updated.style, c2);
                    if let Some(eid) = new_bp.entity_id() {
                        updated.entity_id = eid;
                    }
                    let inh_changed =
                        !Self::inherited_style_equal(&layout_node.style, &updated.style);
                    updated.children = std::mem::take(&mut layout_node.children);
                    *layout_node = updated;
                    stats.rebuilt_nodes += 1;
                    (true, inh_changed)
                } else {
                    if let Some(eid) = new_bp.entity_id() {
                        layout_node.entity_id = eid;
                    }
                    (false, false)
                };

                let child_group_state = if Self::classes_have_group_marker(cls2) {
                    let self_hovered = ctx.hovered_path.is_some_and(|h| h.starts_with(path_buf));
                    let self_focused = ctx.focused_path.is_some_and(|f| f.starts_with(path_buf));
                    let self_active = self_hovered && ctx.is_mouse_down;
                    (
                        group_state.0 || self_hovered,
                        group_state.1 || self_focused,
                        group_state.2 || self_active,
                    )
                } else {
                    group_state
                };
                let old_group_marker = Self::classes_have_group_marker(cls1);
                let group_marker_changed = old_group_marker != Self::classes_have_group_marker(cls2);
                let next_ancestor_selector_changed = ancestor_selector_changed
                    || group_marker_changed
                    || (self_props_changed && sheet_index.has_descendant_combinator_rules);

                let parent_style_for_children = layout_node.style.clone();
                let child_count_changed = c1.len() != c2.len();
                let mut children_changed = child_count_changed;
                let common = c1.len().min(c2.len());
                let last_elem_idx = if !stylesheets.is_empty()
                    && sheet_index.has_sibling_combinator_or_positional_rules
                {
                    c2.iter().rposition(|bp| {
                        matches!(bp, NodeBlueprint::Element { tag, .. } if !Self::is_style_tag(tag))
                    })
                } else {
                    None
                };

                ancestor_stack.push((new_bp, group_state));
                let mut preceding_sibling_mutated = false;

                for i in 0..common {
                    path_buf.push(i);
                    let is_last_child = last_elem_idx.map_or(i + 1 == c2.len(), |l| l == i);
                    // If child count changed and a hover/focus path runs through this child,
                    // force re-style so shifted indices never keep stale pseudo styles.
                    let interaction_shifted = child_count_changed
                        && (ctx.hovered_path.is_some_and(|h| h.starts_with(path_buf))
                            || ctx.focused_path.is_some_and(|f| f.starts_with(path_buf)));
                    let positional_affected = sheet_index.has_sibling_combinator_or_positional_rules
                        && (preceding_sibling_mutated
                            || (child_count_changed && (i + 1 == common || is_last_child)));
                    let child_force_restyle = interaction_shifted || positional_affected;

                    match Self::reconcile_layout_subtree(
                        &c1[i],
                        &c2[i],
                        &mut layout_node.children[i],
                        Some(&parent_style_for_children),
                        child_inherited_changed,
                        next_ancestor_selector_changed,
                        child_force_restyle,
                        path_buf,
                        ancestor_stack,
                        c2,
                        i,
                        is_last_child,
                        child_group_state,
                        stylesheets,
                        sheet_index,
                        ctx,
                        stats,
                    ) {
                        Some(false) => {}
                        Some(true) => {
                            children_changed = true;
                            if sheet_index.has_sibling_combinator_or_positional_rules {
                                preceding_sibling_mutated = true;
                            }
                        }
                        None => {
                            let mut ancestors_meta = Self::materialize_ancestor_metas(
                                ancestor_stack,
                                path_buf,
                                ctx,
                                stylesheets,
                                sheet_index,
                            );
                            let parent_path_len = path_buf.len().saturating_sub(1);
                            let preceding_sibs = Self::materialize_preceding_sibling_metas(
                                c2,
                                i,
                                &path_buf[..parent_path_len],
                                ctx,
                                child_group_state,
                                stylesheets,
                                sheet_index,
                            );
                            layout_node.children[i] = Self::build_layout_tree_full(
                                &c2[i],
                                Some(&parent_style_for_children),
                                path_buf,
                                ctx,
                                stylesheets,
                                &mut ancestors_meta,
                                &preceding_sibs,
                                is_last_child,
                                child_group_state,
                            );
                            stats.rebuilt_nodes += 1;
                            children_changed = true;
                            if sheet_index.has_sibling_combinator_or_positional_rules {
                                preceding_sibling_mutated = true;
                            }
                        }
                    }
                    path_buf.pop();
                }

                if c2.len() < c1.len() {
                    layout_node.children.truncate(c2.len());
                } else if c2.len() > c1.len() {
                    for (offset, nc) in c2[common..].iter().enumerate() {
                        let idx = common + offset;
                        path_buf.push(idx);
                        let is_last_child = last_elem_idx.map_or(idx + 1 == c2.len(), |l| l == idx);
                        let mut ancestors_meta = Self::materialize_ancestor_metas(
                            ancestor_stack,
                            path_buf,
                            ctx,
                            stylesheets,
                            sheet_index,
                        );
                        let parent_path_len = path_buf.len().saturating_sub(1);
                        let preceding_sibs = Self::materialize_preceding_sibling_metas(
                            c2,
                            idx,
                            &path_buf[..parent_path_len],
                            ctx,
                            child_group_state,
                            stylesheets,
                            sheet_index,
                        );
                        layout_node.children.push(Self::build_layout_tree_full(
                            nc,
                            Some(&parent_style_for_children),
                            path_buf,
                            ctx,
                            stylesheets,
                            &mut ancestors_meta,
                            &preceding_sibs,
                            is_last_child,
                            child_group_state,
                        ));
                        stats.rebuilt_nodes += 1;
                        path_buf.pop();
                    }
                }
                ancestor_stack.pop();

                if self_rewritten || children_changed {
                    layout_node.inline_items.clear();
                    layout_node.line_boxes.clear();
                    layout_node.invalidate_self_caches();
                    Some(true)
                } else {
                    stats.reused_subtrees += 1;
                    Some(false)
                }
            }
            _ => None,
        }
    }

    /// Re-evaluates a root [`View`] closure inside an implicit root component scope (when no outer
    /// `#[component]` scope is active) and updates the runtime tree cleanly, preserving top-level
    /// unkeyed sibling isolation and triggering unmount cleanups.
    pub fn update_root<F: FnOnce() -> View>(&mut self, root: F) {
        let view_start = std::time::Instant::now();
        let new_view = strata_dom::with_root_component_scope(root);
        self.frame_profile.view_render += view_start.elapsed();
        self.update_view(new_view);
    }

    /// Updates the application root view, incrementally reconciling unchanged subtrees
    /// and compiling a new display list.
    pub fn update_view(&mut self, new_view: View) {
        self.unapply_scroll_offsets();
        self.applied_scroll_offsets.clear();
        self.applied_portal_offsets.clear();
        if self.had_active_animations || self.has_active_animations() {
            self.restore_base_styles_to_layout_tree();
        }
        let mut next_blueprint = new_view.root.flatten_fragments();
        next_blueprint.ensure_entity_ids();
        let old_blueprint = std::mem::replace(&mut self.root_blueprint, next_blueprint);
        self.has_resize_listeners = Self::blueprint_has_resize_listeners(&self.root_blueprint);
        self.ir_graph = Self::build_ir_graph(&self.root_blueprint);
        let sheets = Self::collect_stylesheets_from_blueprint(&self.root_blueprint);
        let sheets_unchanged = self.cached_stylesheets == sheets;
        if !sheets_unchanged {
            for sheet in &sheets {
                self.register_keyframes_stylesheet(sheet);
            }
            self.stylesheet_index = StylesheetSelectorIndex::from_stylesheets(&sheets);
            self.cached_stylesheets = sheets;
        }
        self.capture_previous_node_sizes();
        let build_start = std::time::Instant::now();
        let mut stats = ReconcileStats::default();
        let mut bailout_reason: Option<String> = None;

        let hovered_owned = self.current_hover_path.clone();
        let focused_owned = self.current_focus_path.clone();
        let ctx = crate::runtime::InteractionContext {
            hovered_path: hovered_owned.as_deref(),
            focused_path: focused_owned.as_deref(),
            is_mouse_down: self.is_mouse_down,
            is_dark_mode: self.is_dark_mode,
        };
        let next_root_inherited = if !self.cached_stylesheets.is_empty() {
            Some(Self::resolve_document_root_inherited_style(
                &self.root_blueprint,
                &self.cached_stylesheets,
                &ctx,
            ))
        } else {
            None
        };
        let root_inherited_changed = match (&self.cached_root_inherited_style, &next_root_inherited)
        {
            (Some(a), Some(b)) => !Self::inherited_style_equal(a, b),
            (None, None) => false,
            _ => true,
        };

        let reconciled = if sheets_unchanged {
            let mut path_buf = Vec::new();
            let mut ancestor_stack = Vec::new();
            let ok = Self::reconcile_layout_subtree(
                &old_blueprint,
                &self.root_blueprint,
                &mut self.layout_root,
                next_root_inherited.as_ref(),
                root_inherited_changed,
                false,
                false,
                &mut path_buf,
                &mut ancestor_stack,
                &[],
                0,
                true,
                (false, false, false),
                &self.cached_stylesheets,
                &self.stylesheet_index,
                &ctx,
                &mut stats,
            )
            .is_some();
            if !ok {
                bailout_reason = Some("RootStructuralMismatch".to_string());
            }
            ok
        } else {
            bailout_reason = Some("StylesheetsChanged".to_string());
            false
        };
        self.cached_root_inherited_style = next_root_inherited;

        if !reconciled {
            let ctx = self.interaction_context();
            self.layout_root = Self::build_layout_tree_with_interaction(&self.root_blueprint, &ctx);
            self.full_rebuilds_count += 1;
            if let Some(ref reason) = bailout_reason {
                self.last_rebuild_reason = RebuildReason::LayoutReconcileBailout(reason.clone());
            }
        } else {
            self.reconciled_updates_count += 1;
        }
        self.last_update_reconciled = reconciled;
        self.last_reconcile_stats = stats;
        self.last_reconcile_bailout = bailout_reason;
        self.frame_profile.reconcile += build_start.elapsed();
        if perf_logging_enabled() {
            perf_log_ms(
                "build_layout_tree_update",
                build_start,
                format!(
                    "{} blueprint nodes -> {} layout nodes (reconciled={}, reused={}, rebuilt={})",
                    perf_count_blueprint_nodes(&self.root_blueprint),
                    perf_count_layout_nodes(&self.layout_root),
                    reconciled,
                    stats.reused_subtrees,
                    stats.rebuilt_nodes
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
        if self.had_active_animations || self.has_active_animations() {
            strata_layout::box_model::refresh_inline_fragment_styles(&mut self.layout_root);
        }
        if perf_logging_enabled() {
            perf_log_ms("sync_tree_animations", sync_start, "update_view");
        }
        self.refresh_hover_tooltip_after_view_update();
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
                let mut node_updated = false;
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
                    node_updated = true;
                    changed = true;
                }
                if node_updated {
                    self.layout_root.invalidate_ancestors(&binding.path);
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

    /// Marks only 3D viewports dirty without invalidating 2D layout or recompiling the display list.
    pub fn mark_viewport_3d_dirty(&mut self) {
        self.viewport_3d_dirty = true;
    }

    /// Returns `true` when any retained 3D viewport has newer camera/geometry/overlay generations
    /// than what was last presented, or when `viewport_3d_dirty` was set explicitly.
    pub fn poll_retained_3d_dirty(&self) -> bool {
        if self.viewport_3d_dirty {
            return true;
        }
        self.tracked_retained_3d
            .iter()
            .any(|(handle, last_gens)| handle.generations() != *last_gens)
    }

    /// Records the current generations of all tracked [`RetainedScene3D`] handles as presented.
    pub fn acknowledge_retained_3d_presented(&mut self) {
        self.viewport_3d_dirty = false;
        for (handle, last_gens) in &mut self.tracked_retained_3d {
            *last_gens = handle.generations();
        }
    }

    /// Returns `true` if any `<Canvas3D>` in the current blueprint opted into uncapped presentation.
    pub fn has_uncapped_canvas3d(&self) -> bool {
        fn walk(node: &NodeBlueprint) -> bool {
            if let NodeBlueprint::Element {
                attributes,
                children,
                ..
            } = node
            {
                if attributes.iter().any(|a| {
                    matches!(
                        a,
                        strata_dom::Attribute::Property { key, value }
                            if key == "data-strata-uncapped" && value == "true"
                    )
                }) {
                    return true;
                }
                if children.iter().any(walk) {
                    return true;
                }
            }
            false
        }
        walk(&self.root_blueprint)
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
        let scrollbar_overlays = self.build_scrollbar_overlays();
        self.display_list =
            strata_render::compositor::Compositor::build_document_display_list_with_scrollbars(
                &self.layout_root,
                viewport,
                &scrollbar_overlays,
            );
        strata_render::compositor::Compositor::append_top_layer_portals_with_scrollbars(
            &self.layout_root,
            &mut self.display_list,
            viewport,
            &scrollbar_overlays,
        );
        self.tracked_retained_3d.clear();
        for cmd in &self.display_list.commands {
            if let strata_render::compositor::DrawCommand::DrawViewport3D { scene, .. } = cmd {
                if let Some(handle) = scene.retained_handle() {
                    self.tracked_retained_3d
                        .push((handle.clone(), handle.generations()));
                }
            }
        }
        self.render_active_input_caret();
        self.render_hover_tooltip();
        if self.gpu_2d_paths_enabled {
            self.gpu_path_pipeline
                .resize(self.viewport_width, self.viewport_height);
            self.gpu_path_pipeline
                .compile_frame(&self.display_list, None);
        }
        self.frame_profile.compile += compile_start.elapsed();
        self.paint_dirty = false;

        static COMPILE_FRAMES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let frame = COMPILE_FRAMES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if frame < 6 || frame.is_multiple_of(120) {
            perf_log_ms(
                "compile_display_list",
                compile_start,
                format!(
                    "frame {} {}x{} {} commands (gpu_2d_paths={})",
                    frame,
                    self.viewport_width,
                    self.viewport_height,
                    self.display_list.commands.len(),
                    self.gpu_2d_paths_enabled
                ),
            );
        }
    }

    /// Controls whether [`Self::compile_display_list`] tessellates 2D GPU path segments.
    /// Backends that rasterize 2D UI via Cairo software should leave this `false` unless
    /// `STRATA_GPU_2D=1` is active.
    pub fn set_gpu_2d_paths_enabled(&mut self, enabled: bool) {
        self.gpu_2d_paths_enabled = enabled;
    }

    /// Returns whether 2D GPU path compilation is active during display-list builds.
    pub fn gpu_2d_paths_enabled(&self) -> bool {
        self.gpu_2d_paths_enabled
    }

    /// Explicitly compiles 2D GPU path segments from the current display list on demand.
    pub fn compile_gpu_paths_now(&mut self) -> usize {
        self.gpu_path_pipeline
            .resize(self.viewport_width, self.viewport_height);
        self.gpu_path_pipeline
            .compile_frame(&self.display_list, None)
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
        let input_start = std::time::Instant::now();
        #[cfg(feature = "devtools")]
        if let Some(ref mut dt) = self.devtools {
            if strata_devtools::DevToolsEventInterceptor::intercept(&mut dt.hud, event) {
                self.frame_profile.input_dispatch += input_start.elapsed();
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
                let tooltip_dismissed = self.dismiss_hover_tooltip_and_repaint();
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
                    scroll_changed || tooltip_dismissed
                }
            }
            PlatformEvent::AnimationTick => {
                let scroll_changed = self.poll_smooth_scroll();
                let anim_changed = self.poll_animations();
                let caret_changed = self.poll_caret_blink();
                let tooltip_changed = self.poll_hover_tooltip();
                scroll_changed || anim_changed || caret_changed || tooltip_changed
            }
            PlatformEvent::MouseMove { x, y } => {
                self.last_mouse_pos = (*x, *y);
                let cur = Self::hit_test_cursor(&self.root_blueprint, &self.layout_root, *x, *y);
                self.current_cursor = cur;
                let hover_changed = self.update_hover(*x, *y);
                self.dispatch_pointer_move_action(*x, *y).needs_redraw() || hover_changed
            }
            PlatformEvent::Stylus {
                phase,
                pointer_id,
                tool,
                x,
                y,
                pressure,
                tilt_x,
                tilt_y,
                twist,
                tangential_pressure,
                hovering,
                buttons,
            } => {
                self.last_mouse_pos = (*x, *y);
                let hover_changed = self.update_hover(*x, *y);
                let dom_ev_type = match phase {
                    strata_platform::StylusPhase::Down => strata_dom::EventType::PointerDown,
                    strata_platform::StylusPhase::Move | strata_platform::StylusPhase::Hover => {
                        strata_dom::EventType::PointerMove
                    }
                    strata_platform::StylusPhase::Up | strata_platform::StylusPhase::Cancel => {
                        strata_dom::EventType::PointerUp
                    }
                };
                let effective_pressure = if *hovering { 0.0 } else { *pressure };
                let outcome = self.dispatch_bubbling_event_detailed_at(
                    dom_ev_type,
                    *x,
                    *y,
                    0,
                    1,
                    |ev| {
                        ev.pointer_id = *pointer_id as i32;
                        ev.pointer_type = tool.as_w3c_str();
                        ev.pressure = effective_pressure;
                        ev.tilt_x = *tilt_x;
                        ev.tilt_y = *tilt_y;
                        ev.twist = *twist;
                        ev.tangential_pressure = *tangential_pressure;
                        ev.buttons = *buttons;
                        ev.is_primary = true;
                    },
                );
                let dynamic_updated = self.sync_dynamic_text();
                if outcome.handled || dynamic_updated {
                    self.rebuild_layout_tree();
                    true
                } else {
                    hover_changed
                }
            }
            PlatformEvent::MouseDown { button, x, y } => {
                let tooltip_dismissed = self.dismiss_hover_tooltip_and_repaint();
                let btn_idx = button.to_button_index();
                if *button == strata_platform::MouseButton::Left {
                    self.is_mouse_down = true;
                }
                let action = self.dispatch_pointer_down_action_with_button_and_modifiers(
                    *x,
                    *y,
                    btn_idx,
                    Default::default(),
                );
                let click_invoked = if *button != strata_platform::MouseButton::Right {
                    self.dispatch_mouse_click(*x, *y, btn_idx, 1)
                } else {
                    self.dispatch_context_menu(*x, *y)
                };
                let dynamic_updated = self.sync_dynamic_text();
                if action.needs_rebuild() || click_invoked || dynamic_updated {
                    self.rebuild_layout_tree();
                    true
                } else {
                    tooltip_dismissed || action.needs_redraw() || self.restyle_for_active_change()
                }
            }
            PlatformEvent::MouseUp { button, x, y } => {
                let btn_idx = button.to_button_index();
                let was_mouse_down = self.is_mouse_down;
                if *button == strata_platform::MouseButton::Left {
                    self.is_mouse_down = false;
                }
                let action = self.dispatch_pointer_up_action_with_button(*x, *y, btn_idx);
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
                let tooltip_dismissed = matches!(event, PlatformEvent::KeyDown { .. })
                    && self.dismiss_hover_tooltip_and_repaint();
                let res = self.dispatch_key_event(event);
                let dynamic_updated = self.sync_dynamic_text();
                tooltip_dismissed || res.needs_redraw() || dynamic_updated
            }
            PlatformEvent::FileDrop(paths) => {
                let tooltip_dismissed = self.dismiss_hover_tooltip_and_repaint();
                let drop_handled = self.dispatch_file_drop(paths.clone());
                let dynamic_updated = self.sync_dynamic_text();
                if drop_handled || dynamic_updated {
                    self.rebuild_layout_tree();
                    true
                } else {
                    tooltip_dismissed
                }
            }
            _ => false,
        };

        if self.poll_dom_mutations(None) {
            needs_redraw = true;
        }

        if needs_redraw {
            self.paint_dirty = true;
        }
        self.frame_profile.input_dispatch += input_start.elapsed();
        needs_redraw
    }

    /// Number of full layout computations performed by this runtime.
    pub fn layout_computations(&self) -> u64 {
        self.layout_computations
    }

    /// True when the most recent `update_view` reconciled the existing layout tree in place.
    pub fn last_update_reconciled(&self) -> bool {
        self.last_update_reconciled
    }

    /// Per-update reconciliation statistics (reused subtrees vs rebuilt nodes).
    pub fn last_reconcile_stats(&self) -> ReconcileStats {
        self.last_reconcile_stats
    }

    /// Reason the most recent `update_view` bailed out of incremental reconciliation, if any.
    pub fn last_reconcile_bailout(&self) -> Option<&str> {
        self.last_reconcile_bailout.as_deref()
    }

    /// True when the most recent paint-only hover update succeeded via `restyle_hover_subtrees_fast`.
    pub fn last_hover_fast_path_used(&self) -> bool {
        self.last_hover_fast_path_used
    }

    /// Reason the most recent paint-only hover update bailed out of `restyle_hover_subtrees_fast`, if any.
    pub fn last_hover_fast_path_bailout(&self) -> Option<&str> {
        self.last_hover_fast_path_bailout.as_deref()
    }

    /// Cumulative number of full layout tree rebuilds performed by this runtime.
    pub fn full_rebuilds_count(&self) -> u64 {
        self.full_rebuilds_count
    }

    /// Cumulative number of incremental reconciled updates performed by this runtime.
    pub fn reconciled_updates_count(&self) -> u64 {
        self.reconciled_updates_count
    }

    /// Structured reason attributing why the latest frame ran work.
    pub fn last_rebuild_reason(&self) -> &RebuildReason {
        &self.last_rebuild_reason
    }

    /// Records the primary reason why the current frame was triggered.
    pub fn set_rebuild_reason(&mut self, reason: RebuildReason) {
        self.last_rebuild_reason = reason;
    }

    /// Phase timings accumulated since the last presented frame.
    pub fn frame_profile(&self) -> FrameProfile {
        self.frame_profile
    }

    /// Most recent completed per-frame telemetry record (for host HUDs & profiling overlays).
    pub fn last_frame_profile(&self) -> Option<&FrameProfileRecord> {
        self.profile_history.last()
    }

    /// Rolling p50/p95/p99 performance distribution and cache hit summary over recent frames.
    pub fn perf_profile_summary(&self) -> PerfProfileSummary {
        self.profile_history.summary(
            self.frame_count,
            self.full_rebuilds_count,
            self.reconciled_updates_count,
        )
    }

    /// Registers an observer called with each completed [`FrameProfile`].
    pub fn set_frame_profile_hook(&mut self, hook: Option<Box<dyn FnMut(&FrameProfile)>>) {
        self.frame_profile_hook = hook.map(FrameProfileHook);
    }

    /// Adds time spent dispatching platform or MCP input events.
    pub fn record_input_dispatch_time(&mut self, elapsed: std::time::Duration) {
        self.frame_profile.input_dispatch += elapsed;
    }

    /// Adds time spent draining reactive signals and flushing subscribers.
    pub fn record_signal_drain_time(&mut self, elapsed: std::time::Duration) {
        self.frame_profile.signal_drain += elapsed;
    }

    /// Adds time spent evaluating the host RSX view closure.
    pub fn record_view_render_time(&mut self, elapsed: std::time::Duration) {
        self.frame_profile.view_render += elapsed;
    }

    /// Adds time spent rasterizing, for presenting backends that rasterize outside the runtime.
    pub fn record_raster_time(&mut self, elapsed: std::time::Duration) {
        self.frame_profile.raster += elapsed;
    }

    /// Adds CPU time spent submitting wgpu 3D passes and compositing commands.
    pub fn record_wgpu_3d_submit_time(&mut self, elapsed: std::time::Duration) {
        self.frame_profile.wgpu_3d_submit += elapsed;
    }

    /// Adds time spent presenting, for backends that present outside the runtime.
    pub fn record_present_time(&mut self, elapsed: std::time::Duration) {
        self.frame_profile.present += elapsed;
    }

    /// Computes a real Cumulative Layout Shift (CLS) score against the previous frame's
    /// top-level layout rectangles and updates `self.prev_layout_cls_rects`.
    fn compute_real_layout_shift_score(&mut self) -> f32 {
        let mut cur_rects: Vec<(f32, f32, f32, f32)> = Vec::new();
        for child in self.layout_root.children.iter().take(32) {
            let b = child.geometry.border_box;
            cur_rects.push((b.x, b.y, b.width, b.height));
        }
        let mut max_shift = 0.0f32;
        let vw = self.viewport_width.max(1.0);
        let vh = self.viewport_height.max(1.0);
        let max_dim = vw.max(vh);
        let vp_area = (vw * vh).max(1.0);
        for (prev, cur) in self.prev_layout_cls_rects.iter().zip(cur_rects.iter()) {
            let dx = (cur.0 - prev.0).abs();
            let dy = (cur.1 - prev.1).abs();
            if dx > 0.5 || dy > 0.5 {
                let dist = (dx.max(dy) / max_dim).clamp(0.0, 1.0);
                let impact = ((prev.2 * prev.3).max(cur.2 * cur.3) / vp_area).clamp(0.0, 1.0);
                max_shift = max_shift.max(impact * dist);
            }
        }
        self.prev_layout_cls_rects = cur_rects;
        max_shift
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

    /// Finalizes a presented frame: snapshots all real monotonic CPU phase timings, GPU hardware
    /// timestamp queries, 64x64 tile cache stats, Scene3D offscreen cache stats, text/glyph cache
    /// hit rate, heap allocation deltas, and DevTools live HUD / Time-Travel / Perfetto state.
    pub fn finalize_frame_telemetry(&mut self, outcome: &'static str) -> FrameProfileRecord {
        let gpu_tel = strata_platform::gtk_last_gpu_telemetry();
        let (tiles_dirty, tiles_total) = strata_platform::gtk_last_tile_cache_stats();
        let tile_cache_hit_pct = if tiles_total > 0 {
            ((tiles_total.saturating_sub(tiles_dirty)) as f32 / tiles_total as f32) * 100.0
        } else {
            100.0
        };
        let s3d_total = gpu_tel.scene3d_cache_hits + gpu_tel.scene3d_cache_misses;
        let scene3d_cache_hit_pct = if s3d_total > 0 {
            (gpu_tel.scene3d_cache_hits as f32 / s3d_total as f32) * 100.0
        } else {
            100.0
        };
        let (text_hits, text_misses) = strata_layout::text_measure_cache_stats();
        let text_total = text_hits + text_misses;
        let glyph_atlas_hit_pct = if text_total > 0 {
            (text_hits as f32 / text_total as f32) * 100.0
        } else {
            100.0
        };

        self.frame_profile.gpu_3d_us = gpu_tel
            .gpu_shadow_pass_us
            .saturating_add(gpu_tel.gpu_3d_pass_us);
        self.frame_profile.gpu_2d_overlay_us = gpu_tel.gpu_2d_overlay_pass_us;
        self.frame_profile.gpu_timestamps_valid = gpu_tel.gpu_timestamps_supported;

        let done = self.finish_frame_profile();
        let cls_score = self.compute_real_layout_shift_score();
        let timestamp_us = self.runtime_epoch.elapsed().as_micros() as u64;
        let (heap_alloc_count, heap_alloc_bytes) = profile::take_frame_heap_alloc_delta();

        let dom_nodes = perf_count_blueprint_nodes(&self.root_blueprint);
        let layout_nodes = perf_count_layout_nodes(&self.layout_root);
        let display_cmds = self.display_list.len();

        let reconcile_us = if !done.reconcile.is_zero() {
            done.reconcile.as_micros() as u32
        } else {
            done.rebuild.as_micros() as u32
        };
        let style_cascade_us = done.style_cascade.as_micros() as u32;
        let input_dispatch_us = done.input_dispatch.as_micros() as u32;
        let signal_drain_us = done.signal_drain.as_micros() as u32;
        let view_render_us = done.view_render.as_micros() as u32;
        let layout_us = done.layout.as_micros() as u32;
        let display_list_build_us = done.compile.as_micros() as u32;
        let raster_us = done.raster.as_micros() as u32;
        let wgpu_3d_submit_us = done.wgpu_3d_submit.as_micros() as u32;
        let present_us = done.present.as_micros() as u32;
        let total_cpu_us = done.total().as_micros() as u32;

        let reason_str = self.last_rebuild_reason.to_string();
        let record = FrameProfileRecord {
            frame_index: self.frame_count,
            timestamp_us,
            rebuild_reason: reason_str.clone(),
            outcome: outcome.to_string(),
            input_dispatch_us,
            signal_drain_us,
            view_render_us,
            reconcile_us,
            style_cascade_us,
            layout_us,
            display_list_build_us,
            raster_us,
            wgpu_3d_submit_us,
            present_us,
            total_cpu_us,
            gpu_3d_us: done.gpu_3d_us,
            gpu_2d_overlay_us: done.gpu_2d_overlay_us,
            gpu_timestamps_valid: done.gpu_timestamps_valid,
            dom_nodes: dom_nodes as u32,
            layout_nodes: layout_nodes as u32,
            display_cmds: display_cmds as u32,
            tiles_dirty,
            tiles_total,
            tile_cache_hit_pct,
            reconciled_subtrees: self.last_reconcile_stats.reused_subtrees as u32,
            rebuilt_nodes: self.last_reconcile_stats.rebuilt_nodes as u32,
            full_rebuilds: self.full_rebuilds_count,
            reconciled: self.last_update_reconciled,
            hover_fast_path_used: self.last_hover_fast_path_used,
            hover_fast_path_bailout: self.last_hover_fast_path_bailout.clone(),
            reconcile_bailout: self.last_reconcile_bailout.clone(),
            scene3d_cache_hits: gpu_tel.scene3d_cache_hits,
            scene3d_cache_misses: gpu_tel.scene3d_cache_misses,
            scene3d_cache_hit_pct,
            glyph_atlas_hit_pct,
            gpu_memory_bytes: gpu_tel.gpu_memory_bytes,
            heap_alloc_count,
            heap_alloc_bytes,
            layout_shift_score: cls_score,
        };
        self.profile_history.push(record.clone());

        #[cfg(feature = "devtools")]
        if let Some(ref mut dt) = self.devtools {
            let ui_tid = strata_devtools::current_thread_id_u64();
            let gpu_tid = ui_tid.wrapping_add(1).max(2);
            let gpu_total_hw = gpu_tel
                .gpu_shadow_pass_us
                .saturating_add(gpu_tel.gpu_3d_pass_us)
                .saturating_add(gpu_tel.gpu_2d_overlay_pass_us);
            let sample = strata_devtools::FrameTimingSample {
                frame_index: self.frame_count,
                timestamp_us,
                pid: std::process::id(),
                ui_thread_id: ui_tid,
                worker_thread_id: 0,
                gpu_thread_id: gpu_tid,
                vsync_wait_us: 0,
                input_dispatch_us,
                signal_drain_us,
                view_render_us,
                parallel_reconcile_us: reconcile_us,
                style_cascade_us,
                layout_solve_us: layout_us,
                display_list_build_us,
                raster_2d_us: raster_us,
                gpu_submit_us: wgpu_3d_submit_us,
                present_us,
                gpu_raster_duration_us: if gpu_tel.gpu_timestamps_supported {
                    gpu_total_hw
                } else {
                    raster_us.saturating_add(wgpu_3d_submit_us)
                },
                gpu_shadow_pass_us: gpu_tel.gpu_shadow_pass_us,
                gpu_3d_pass_us: gpu_tel.gpu_3d_pass_us,
                gpu_2d_overlay_pass_us: gpu_tel.gpu_2d_overlay_pass_us,
                gpu_timestamps_valid: gpu_tel.gpu_timestamps_supported,
                total_cpu_frame_us: total_cpu_us,
                frame_budget_us: dt.profiler.frame_budget_us(),
                layout_shift_score: cls_score,
            };
            dt.profiler.record_sample(sample);

            let react_snap = strata_dom::reactive_telemetry_snapshot();
            let arena_est_bytes = dom_nodes
                .saturating_mul(std::mem::size_of::<NodeBlueprint>())
                .saturating_add(layout_nodes.saturating_mul(std::mem::size_of::<strata_layout::LayoutNode>()))
                .saturating_add(display_cmds.saturating_mul(std::mem::size_of::<strata_render::DrawCommand>()));
            let arena_cap_bytes = (arena_est_bytes.max(64 * 1024)).next_power_of_two();
            dt.arena_inspector
                .record_sample(strata_devtools::ArenaTelemetrySample::new(
                    0,
                    arena_est_bytes,
                    arena_cap_bytes,
                    arena_est_bytes,
                    self.frame_count,
                ));
            if heap_alloc_bytes > 0 {
                dt.heap_tracker.record_allocation(heap_alloc_bytes as usize);
            }
            dt.live_telemetry = strata_devtools::HudLiveTelemetry {
                reactive_active_signals: react_snap.active_signals,
                reactive_active_subscribers: react_snap.active_subscribers,
                reactive_total_reads: react_snap.total_reads,
                reactive_total_writes: react_snap.total_writes,
                reactive_total_flushes: react_snap.total_flushes,
                reactive_cycle_guard_trips: react_snap.cycle_guard_trips,
                last_rebuild_reason: reason_str.clone(),
                dirty_signal_names: react_snap.dirty_signal_names,
                top_signals: react_snap
                    .signals
                    .iter()
                    .take(8)
                    .map(|s| (s.name.clone(), s.write_count, s.read_count, s.subscriber_count))
                    .collect(),
                dom_node_count: dom_nodes,
                layout_node_count: layout_nodes,
                display_cmd_count: display_cmds,
                arena_allocated_bytes: arena_est_bytes,
                arena_capacity_bytes: arena_cap_bytes,
                heap_alloc_bytes,
                heap_alloc_count,
                gpu_memory_bytes: gpu_tel.gpu_memory_bytes,
                tiles_dirty,
                tiles_total,
                tile_cache_hit_pct,
                scene3d_cache_hit_pct,
                glyph_cache_hit_pct: glyph_atlas_hit_pct,
                reconciled_updates: self.reconciled_updates_count,
                full_rebuilds: self.full_rebuilds_count,
            };

            if outcome == "Dirty" || dt.time_travel.is_empty() {
                let mut ir_hash: u64 = 0xcbf2_9ce4_8422_2325;
                ir_hash ^= (dom_nodes as u64).wrapping_mul(0x100_0000_01b3);
                ir_hash ^= (layout_nodes as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
                ir_hash ^= (display_cmds as u64).wrapping_mul(0x517c_c1b7_2722_0a95);
                ir_hash ^= self.layout_computations;
                let sig_values: Vec<(usize, String)> = react_snap
                    .signals
                    .iter()
                    .take(16)
                    .map(|s| {
                        (
                            s.signal_id,
                            format!("{}:w{}r{}", s.name, s.write_count, s.read_count),
                        )
                    })
                    .collect();
                dt.time_travel
                    .record_snapshot(timestamp_us, reason_str, sig_values, ir_hash);
            }
            dt.cdp_dispatcher.update_from_ir(
                &self.ir_graph,
                &self.base_node_styles,
                self.frame_count,
                self.layout_computations,
            );
        }

        record
    }

    /// Renders the current frame and presents it to the given surface presenter via the swapchain compositor.
    pub fn render_frame<P: SurfacePresenter>(
        &mut self,
        compositor: &mut SwapchainCompositor,
        presenter: &mut P,
        force_redraw: bool,
    ) -> Result<bool, PresentError> {
        let retained_3d_dirty = self.poll_retained_3d_dirty();
        let did_dirty_tick = self.paint_dirty || self.frame_count == 0 || force_redraw;
        if did_dirty_tick {
            self.tick_frame();
        } else {
            self.compiles_skipped += 1;
            if retained_3d_dirty {
                self.frame_count += 1;
            }
        }

        #[cfg(feature = "devtools")]
        if let Some(ref mut dt) = self.devtools {
            if dt.hud.state.is_visible {
                dt.render_hud_overlay(&mut self.display_list);
            }
        }

        let present_start = std::time::Instant::now();
        let res = compositor.render_and_present(
            &self.display_list,
            presenter,
            force_redraw || retained_3d_dirty,
        );
        self.acknowledge_retained_3d_presented();
        self.frame_profile.present += present_start.elapsed();

        let outcome = if did_dirty_tick {
            "Dirty"
        } else if retained_3d_dirty {
            "RepaintCanvas3D"
        } else {
            "Clean"
        };
        self.finalize_frame_telemetry(outcome);

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

    #[cfg(feature = "pdf")]
    /// Compiles the current `AppRuntime` vector [`DisplayList`] directly into a PDF 1.7 byte buffer.
    pub fn export_pdf_bytes(&mut self, options: &strata_render::PdfExportOptions) -> Vec<u8> {
        self.compile_display_list();
        strata_render::export_display_list_to_pdf(
            &self.display_list,
            self.viewport_width,
            self.viewport_height,
            options,
        )
    }

    #[cfg(feature = "pdf")]
    /// Exports the current `AppRuntime` UI directly to a lossless Vector PDF 1.7 file on disk.
    pub fn export_pdf<P: AsRef<std::path::Path>>(
        &mut self,
        path: P,
        options: &strata_render::PdfExportOptions,
    ) -> Result<(), String> {
        let bytes = self.export_pdf_bytes(options);
        std::fs::write(path, bytes).map_err(|e| e.to_string())
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
