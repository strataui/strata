//! Live Application Runtime Orchestrator connecting RSX Blueprints, Style Cascade, Layout, and Rendering.

use strata_dom::{NodeBlueprint, Tag, View};
use strata_layout::prelude::*;
use strata_platform::{
    EventLoop, EventLoopControl, LivePlatformWindow, PlatformEvent, PlatformWindow,
    SoftwareSurfacePresenter, SwapchainCompositor, SwapchainFormat, WindowConfig,
};
use strata_render::compositor::DisplayList;
use strata_render::swapchain::{PresentError, SurfacePresenter};
use strata_style::types::{ComputedStyle, Dimension, Display, FlexDirection, FontWeight};

/// Returns the typographic advance scaling factor for font weights in standard OpenType fonts.
fn font_weight_scale(weight: FontWeight) -> f32 {
    if strata_layout::text::get_text_measurer().is_some() {
        return match weight {
            FontWeight::SemiBold => 1.04,
            FontWeight::Bold => 1.06,
            FontWeight::ExtraBold => 1.08,
            FontWeight::Black => 1.10,
            _ => 1.0,
        };
    }
    match weight {
        FontWeight::Thin => 0.96,
        FontWeight::ExtraLight => 0.98,
        FontWeight::Light => 0.99,
        FontWeight::Normal => 1.00,
        FontWeight::Medium => 1.02,
        FontWeight::SemiBold => 1.03,
        FontWeight::Bold => 1.04,
        FontWeight::ExtraBold => 1.05,
        FontWeight::Black => 1.06,
    }
}

/// Hit-test target result representing a clickable text input element in the blueprint tree.
#[derive(Clone)]
pub struct InputHitTarget {
    pub input_index: usize,
    pub input_id: Option<String>,
    pub current_value: String,
    pub font_size: f32,
    pub line_height: f32,
    pub is_mono: bool,
    pub content_box_x: f32,
    pub content_box_y: f32,
    pub on_input: Option<std::rc::Rc<dyn Fn(strata_dom::Event)>>,
    pub on_change: Option<std::rc::Rc<dyn Fn(strata_dom::Event)>>,
    pub on_submit: Option<std::rc::Rc<dyn Fn(strata_dom::Event)>>,
    pub on_focus: Option<std::rc::Rc<dyn Fn(strata_dom::Event)>>,
    pub on_blur: Option<std::rc::Rc<dyn Fn(strata_dom::Event)>>,
}

/// Active text input state tracked by the runtime while an input has keyboard focus.
#[derive(Clone)]
pub struct ActiveInput {
    pub input_index: usize,
    pub input_id: Option<String>,
    pub state: strata_dom::text_input::TextInputState,
    pub on_input: Option<std::rc::Rc<dyn Fn(strata_dom::Event)>>,
    pub on_change: Option<std::rc::Rc<dyn Fn(strata_dom::Event)>>,
    pub on_submit: Option<std::rc::Rc<dyn Fn(strata_dom::Event)>>,
    pub on_blur: Option<std::rc::Rc<dyn Fn(strata_dom::Event)>>,
}

impl std::fmt::Debug for ActiveInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActiveInput")
            .field("text", &self.state.text.get())
            .field("cursor", &self.state.cursor_index.get())
            .finish()
    }
}

/// Active scrollbar state for an overflow container.
#[derive(Debug, Clone)]
pub struct ActiveScrollbar {
    pub bounds: strata_layout::Rect,
    pub thumb_bounds: strata_layout::Rect,
    pub scroll_y: f32,
    pub max_scroll_y: f32,
    pub content_h: f32,
    pub is_id: bool,
    pub id: String,
    pub path: Vec<usize>,
    pub available_travel: f32,
}

/// Active drag operation on a scrollbar thumb.
#[derive(Debug, Clone)]
pub struct ScrollbarDrag {
    pub is_id: bool,
    pub id: String,
    pub path: Vec<usize>,
    pub start_pointer_y: f32,
    pub start_scroll_y: f32,
    pub available_travel: f32,
    pub max_scroll_y: f32,
}

/// Active kinetic smooth scroll animation tracking state.
#[derive(Debug, Clone)]
pub struct SmoothScrollTracker {
    pub container_path: Vec<usize>,
    pub container_id: String,
    pub is_id: bool,
    pub max_scroll_y: f32,
    pub available_travel: f32,
    pub bounds_y: f32,
    /// Current interpolated scroll offset in pixels.
    pub current_y: f32,
    /// Target scroll position in pixels.
    pub target_y: f32,
    /// Initial position at the start of the current impulse.
    pub start_y: f32,
    /// Initial velocity in px/s at the start of the current impulse.
    pub initial_velocity: f32,
    /// Timestamp when the current impulse started.
    pub start_time: std::time::Instant,
    /// Timestamp of the last incoming wheel event (for flick velocity calculation).
    pub last_event_time: std::time::Instant,
    /// Critically damped angular frequency (rad/s).
    pub omega: f32,
}

/// Internal descriptor of a scrollable target container under the cursor.
#[derive(Debug, Clone)]
pub struct ScrollTarget {
    pub path: Vec<usize>,
    pub id: Option<String>,
    pub bounds: strata_layout::Rect,
    pub max_scroll_x: f32,
    pub max_scroll_y: f32,
}

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
    pub active_focus: Option<ActiveInput>,
    pub input_states: std::collections::HashMap<String, strata_dom::text_input::TextInputState>,
    pub caret_blink_epoch: std::time::Instant,
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
}

/// Computes a deterministic 64-bit ID from a hierarchical tree path.
pub fn path_to_node_id(path: &[usize]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for &idx in path {
        hash ^= idx as u64;
        hash = hash.wrapping_mul(0x100000001b3u64);
    }
    hash
}

/// Editing commands that can be applied to the focused text input from any UI surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditCommand {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
}

thread_local! {
    static EDIT_COMMAND_QUEUE: std::cell::RefCell<Vec<EditCommand>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Queues an edit command for the focused input; the runtime applies it right after the current event.
pub fn queue_edit_command(cmd: EditCommand) {
    EDIT_COMMAND_QUEUE.with(|q| q.borrow_mut().push(cmd));
}

impl AppRuntime {
    /// Creates and mounts a new application runtime from a root view.
    pub fn new(root_view: View, viewport_width: f32, viewport_height: f32, dpr: f32) -> Self {
        #[cfg(target_os = "linux")]
        strata_platform::init_pango_text_measurement();

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
            active_focus: None,
            input_states: std::collections::HashMap::new(),
            caret_blink_epoch: std::time::Instant::now(),
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
        };

        runtime.sync_tree_animations(0.0);
        runtime.apply_viewport_clamps();
        runtime.apply_scroll_offsets();
        runtime.compile_display_list();
        runtime
    }

    /// Resets the caret blink cycle so the insertion caret is immediately solid and visible.
    #[inline]
    pub fn reset_caret_blink(&mut self) {
        self.caret_blink_epoch = std::time::Instant::now();
    }

    /// Re-evaluates layout and compiles display commands for the current frame.
    pub fn tick_frame(&mut self) -> &DisplayList {
        self.frame_count += 1;
        self.unapply_scroll_offsets();
        self.layout_root.compute_layout(self.viewport_width, self.viewport_height);
        self.apply_viewport_clamps();
        self.apply_scroll_offsets();
        self.compile_display_list();
        &self.display_list
    }

    /// Updates the application root view, reconstructing the layout tree and compiling a new display list.
    pub fn update_view(&mut self, new_view: View) {
        self.applied_scroll_offsets.clear();
        self.root_blueprint = new_view.root;
        self.layout_root = Self::build_layout_tree_with_hover(
            &self.root_blueprint,
            self.current_hover_path.as_deref(),
        );
        let now_ms = self.runtime_start_epoch.elapsed().as_secs_f64() * 1000.0;
        self.sync_tree_animations(now_ms);
        self.tick_frame();
    }

    /// Synchronizes running animations and transitions with the current layout tree.
    pub fn sync_tree_animations(&mut self, now_ms: f64) {
        let mut new_base_styles = std::collections::HashMap::new();
        let mut new_node_paths = std::collections::HashMap::new();
        let mut path_buf = Vec::new();

        let mut trans_events = Vec::new();
        let mut anim_events = Vec::new();

        Self::sync_tree_animations_rec(
            &self.layout_root,
            &mut path_buf,
            now_ms,
            &self.previous_node_styles,
            &mut new_base_styles,
            &mut new_node_paths,
            &mut self.transition_manager,
            &mut self.animation_manager,
            &mut trans_events,
            &mut anim_events,
        );

        self.base_node_styles = new_base_styles;
        self.previous_node_styles = self.base_node_styles.clone();
        self.node_paths = new_node_paths;

        // Evaluate scroll-driven animations progress
        self.evaluate_scroll_animations(&mut anim_events);

        for ev in trans_events {
            self.dispatch_transition_event(ev);
        }
        for ev in anim_events {
            self.dispatch_animation_event(ev);
        }

        self.apply_animations_to_layout_tree();
    }

    #[allow(clippy::too_many_arguments)]
    fn sync_tree_animations_rec(
        node: &LayoutNode,
        path: &mut Vec<usize>,
        now_ms: f64,
        previous_styles: &std::collections::HashMap<u64, ComputedStyle>,
        new_base_styles: &mut std::collections::HashMap<u64, ComputedStyle>,
        new_node_paths: &mut std::collections::HashMap<u64, Vec<usize>>,
        transition_manager: &mut strata_style::transition::TransitionManager,
        animation_manager: &mut strata_style::animation::AnimationManager,
        trans_events: &mut Vec<strata_style::transition::TransitionLifecycleEvent>,
        anim_events: &mut Vec<strata_style::animation::AnimationLifecycleEvent>,
    ) {
        let node_id = path_to_node_id(path);
        new_base_styles.insert(node_id, node.style.clone());
        new_node_paths.insert(node_id, path.clone());

        // Check for transitions if we have a previous style or @starting-style
        let effective_old_style = match previous_styles.get(&node_id) {
            Some(old) if old.display != strata_style::Display::None => Some(old),
            _ => node.style.starting_style.as_deref(),
        };

        if let Some(old_style) = effective_old_style {
            if !node.style.transitions.is_empty() {
                trans_events.extend(transition_manager.update_styles(node_id, old_style, &node.style, now_ms));
            }
        }

        // Check for keyframe animations
        if !node.style.animations.is_empty() || animation_manager.has_active_animations() {
            anim_events.extend(animation_manager.update_styles(node_id, &node.style, now_ms));
        }

        for (idx, child) in node.children.iter().enumerate() {
            path.push(idx);
            Self::sync_tree_animations_rec(
                child,
                path,
                now_ms,
                previous_styles,
                new_base_styles,
                new_node_paths,
                transition_manager,
                animation_manager,
                trans_events,
                anim_events,
            );
            path.pop();
        }
    }

    /// Applies active transition and animation values directly onto the layout tree.
    pub fn apply_animations_to_layout_tree(&mut self) {
        let has_anims = self.transition_manager.has_active_transitions()
            || self.animation_manager.has_active_animations();
        if !has_anims && !self.had_active_animations {
            return;
        }
        self.had_active_animations = has_anims;

        let mut path = Vec::new();
        Self::apply_animations_rec(
            &mut self.layout_root,
            &mut path,
            &self.base_node_styles,
            &self.transition_manager,
            &self.animation_manager,
        );
    }

    fn apply_animations_rec(
        node: &mut LayoutNode,
        path: &mut Vec<usize>,
        base_styles: &std::collections::HashMap<u64, ComputedStyle>,
        transition_manager: &strata_style::transition::TransitionManager,
        animation_manager: &strata_style::animation::AnimationManager,
    ) {
        let node_id = path_to_node_id(path);
        if let Some(base) = base_styles.get(&node_id) {
            node.style = base.clone();
        }
        transition_manager.apply_to_style(node_id, &mut node.style);
        animation_manager.apply_to_style(node_id, &mut node.style);

        for (idx, child) in node.children.iter_mut().enumerate() {
            path.push(idx);
            Self::apply_animations_rec(child, path, base_styles, transition_manager, animation_manager);
            path.pop();
        }
    }

    /// Evaluates all active scroll-driven animations based on current container scroll positions.
    pub fn evaluate_scroll_animations(&mut self, anim_events: &mut Vec<strata_style::animation::AnimationLifecycleEvent>) {
        for (node_id, path) in &self.node_paths {
            if let Some(style) = self.base_node_styles.get(node_id) {
                for anim in &style.animations {
                    match &anim.timeline {
                        strata_style::AnimationTimeline::Scroll(src, _axis) => {
                            let scroll_progress = match src {
                                strata_style::ScrollTimelineSource::Root => {
                                    self.active_scrollbars.iter()
                                        .max_by(|a, b| (a.bounds.width * a.bounds.height).partial_cmp(&(b.bounds.width * b.bounds.height)).unwrap_or(std::cmp::Ordering::Equal))
                                        .map(|b| if b.max_scroll_y > 0.0 { (b.scroll_y / b.max_scroll_y).clamp(0.0, 1.0) } else { 0.0 })
                                        .unwrap_or(0.0)
                                }
                                strata_style::ScrollTimelineSource::Nearest | strata_style::ScrollTimelineSource::SelfNode => {
                                    let mut p = 0.0;
                                    let mut check_path = path.clone();
                                    while !check_path.is_empty() {
                                        if let Some((_, sy)) = self.scroll_offsets.get(&check_path) {
                                            if let Some(bar) = self.active_scrollbars.iter().find(|b| b.path == check_path) {
                                                if bar.max_scroll_y > 0.0 {
                                                    p = (*sy / bar.max_scroll_y).clamp(0.0, 1.0);
                                                    break;
                                                }
                                            }
                                        }
                                        check_path.pop();
                                    }
                                    p
                                }
                            };
                            anim_events.extend(self.animation_manager.step_node_scroll_progress(*node_id, scroll_progress, style));
                        }
                        strata_style::AnimationTimeline::View(_axis, _start, _end) => {
                            let p = 0.5;
                            anim_events.extend(self.animation_manager.step_node_scroll_progress(*node_id, p, style));
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// Evaluates scroll-driven animations synchronously during container scroll, consuming 0% CPU when stationary.
    pub fn sync_scroll_animations(&mut self) {
        let mut anim_events = Vec::new();
        self.evaluate_scroll_animations(&mut anim_events);
        for ev in anim_events {
            self.dispatch_animation_event(ev);
        }
        self.apply_animations_to_layout_tree();
    }

    /// Captures element layout bounding boxes for elements with `view-transition-name` before DOM mutation.
    pub fn capture_view_transitions(&mut self) {
        let mut named = Vec::new();
        Self::collect_view_transition_elements(&self.layout_root, &mut named);
        self.view_transition_manager.capture_old_state(&named);
    }

    /// Triggers a view transition after DOM mutation, pairing old bounds with new layout bounds.
    pub fn start_view_transition(&mut self, duration_ms: f32) {
        let mut named = Vec::new();
        Self::collect_view_transition_elements(&self.layout_root, &mut named);
        let now_ms = self.runtime_start_epoch.elapsed().as_secs_f64() * 1000.0;
        self.view_transition_manager.start_transition(&named, now_ms, duration_ms);
    }

    fn collect_view_transition_elements(node: &LayoutNode, out: &mut Vec<(String, strata_primitives::geom::Rect)>) {
        if let Some(name) = &node.style.view_transition_name {
            if name != "none" {
                out.push((name.clone(), node.geometry.border_box.into()));
            }
        }
        for child in &node.children {
            Self::collect_view_transition_elements(child, out);
        }
    }

    /// Returns true if there are any active transitions or keyframe animations running.
    pub fn has_active_animations(&self) -> bool {
        self.transition_manager.has_active_transitions()
            || self.animation_manager.has_time_based_animations()
            || self.view_transition_manager.has_active_transitions()
    }

    /// Advances active animations and transitions by current clock time.
    /// Returns true if any animation stepped and a redraw is needed.
    pub fn poll_animations(&mut self) -> bool {
        if !self.has_active_animations() {
            return false;
        }

        let now_ms = self.runtime_start_epoch.elapsed().as_secs_f64() * 1000.0;
        let trans_events = self.transition_manager.advance(now_ms);
        let anim_events = self.animation_manager.advance(now_ms, &self.base_node_styles);
        let _vt_active = self.view_transition_manager.step(now_ms);

        for ev in trans_events {
            self.dispatch_transition_event(ev);
        }
        for ev in anim_events {
            self.dispatch_animation_event(ev);
        }

        self.apply_animations_to_layout_tree();
        self.tick_frame();

        true
    }

    /// Dispatches a synthetic W3C animation lifecycle event to matching RSX element event listeners.
    pub fn dispatch_animation_event(&self, event: strata_style::animation::AnimationLifecycleEvent) {
        let (node_id, name, elapsed, event_type) = match event {
            strata_style::animation::AnimationLifecycleEvent::Start { node_id, name, elapsed_time_sec } => {
                (node_id, name, elapsed_time_sec, strata_dom::EventType::AnimationStart)
            }
            strata_style::animation::AnimationLifecycleEvent::Iteration { node_id, name, elapsed_time_sec } => {
                (node_id, name, elapsed_time_sec, strata_dom::EventType::AnimationIteration)
            }
            strata_style::animation::AnimationLifecycleEvent::End { node_id, name, elapsed_time_sec } => {
                (node_id, name, elapsed_time_sec, strata_dom::EventType::AnimationEnd)
            }
            strata_style::animation::AnimationLifecycleEvent::Cancel { node_id, name, elapsed_time_sec } => {
                (node_id, name, elapsed_time_sec, strata_dom::EventType::AnimationCancel)
            }
        };

        if let Some(path) = self.node_paths.get(&node_id) {
            if let Some(bp) = Self::find_blueprint_at_path(&self.root_blueprint, path) {
                if let NodeBlueprint::Element { attributes, .. } = bp {
                    for attr in attributes {
                        if let strata_dom::Attribute::EventListener { event: ev_type, handler } = attr {
                            if *ev_type == event_type {
                                let ev = strata_dom::Event::animation(event_type, name.clone(), elapsed);
                                handler(ev);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Dispatches a synthetic W3C transition lifecycle event to matching RSX element event listeners.
    pub fn dispatch_transition_event(&self, event: strata_style::transition::TransitionLifecycleEvent) {
        let (node_id, name, elapsed, event_type) = match event {
            strata_style::transition::TransitionLifecycleEvent::Run { node_id, property, elapsed_time_sec } => {
                (node_id, property, elapsed_time_sec, strata_dom::EventType::TransitionRun)
            }
            strata_style::transition::TransitionLifecycleEvent::Start { node_id, property, elapsed_time_sec } => {
                (node_id, property, elapsed_time_sec, strata_dom::EventType::TransitionStart)
            }
            strata_style::transition::TransitionLifecycleEvent::End { node_id, property, elapsed_time_sec } => {
                (node_id, property, elapsed_time_sec, strata_dom::EventType::TransitionEnd)
            }
            strata_style::transition::TransitionLifecycleEvent::Cancel { node_id, property, elapsed_time_sec } => {
                (node_id, property, elapsed_time_sec, strata_dom::EventType::TransitionCancel)
            }
        };

        if let Some(path) = self.node_paths.get(&node_id) {
            if let Some(bp) = Self::find_blueprint_at_path(&self.root_blueprint, path) {
                if let NodeBlueprint::Element { attributes, .. } = bp {
                    for attr in attributes {
                        if let strata_dom::Attribute::EventListener { event: ev_type, handler } = attr {
                            if *ev_type == event_type {
                                let ev = strata_dom::Event::transition(event_type, name.clone(), elapsed);
                                handler(ev);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Registers all parsed `@keyframes` rules from a stylesheet into the runtime animation registry.
    pub fn register_keyframes_stylesheet(&mut self, sheet: &strata_style::StyleSheet) {
        self.animation_manager.registry_mut().register_stylesheet(sheet);
    }

    /// Registers a single compiled `@keyframes` rule into the runtime animation registry.
    pub fn register_keyframe_rule(&mut self, rule: &strata_style::KeyframeRule) {
        self.animation_manager.registry_mut().register_rule(rule);
    }

    /// Locates a blueprint node given its hierarchical index path.
    pub fn find_blueprint_at_path<'a>(
        blueprint: &'a NodeBlueprint,
        path: &[usize],
    ) -> Option<&'a NodeBlueprint> {
        let mut cur = blueprint;
        for &idx in path {
            match cur {
                NodeBlueprint::Element { children, .. } => {
                    let mut flat = Vec::new();
                    for c in children {
                        Self::flatten_blueprints(c, &mut flat);
                    }
                    if idx >= flat.len() {
                        return None;
                    }
                    cur = flat[idx];
                }
                NodeBlueprint::Fragment(children) => {
                    let mut flat = Vec::new();
                    for c in children {
                        Self::flatten_blueprints(c, &mut flat);
                    }
                    if idx >= flat.len() {
                        return None;
                    }
                    cur = flat[idx];
                }
                _ => return None,
            }
        }
        Some(cur)
    }

    /// Recursively transforms an RSX `NodeBlueprint` into a `LayoutNode` hierarchy with styling and inheritance.
    pub fn build_layout_tree(blueprint: &NodeBlueprint) -> LayoutNode {
        Self::build_layout_tree_with_hover(blueprint, None)
    }

    /// Recursively transforms an RSX `NodeBlueprint` into a `LayoutNode` hierarchy with styling, inheritance, and hover state.
    pub fn build_layout_tree_with_hover(
        blueprint: &NodeBlueprint,
        hovered_path: Option<&[usize]>,
    ) -> LayoutNode {
        let mut path_buf = Vec::new();
        Self::build_layout_tree_with_context(blueprint, None, &mut path_buf, hovered_path)
    }

    /// Internal recursive tree builder passing parent ComputedStyle for W3C CSS property inheritance.
    pub fn build_layout_tree_with_parent(
        blueprint: &NodeBlueprint,
        parent_style: Option<&ComputedStyle>,
    ) -> LayoutNode {
        let mut path_buf = Vec::new();
        Self::build_layout_tree_with_context(blueprint, parent_style, &mut path_buf, None)
    }

    /// Internal recursive tree builder passing parent style, node path, and active hover state.
    pub fn build_layout_tree_with_context(
        blueprint: &NodeBlueprint,
        parent_style: Option<&ComputedStyle>,
        current_path: &mut Vec<usize>,
        hovered_path: Option<&[usize]>,
    ) -> LayoutNode {
        match blueprint {
            NodeBlueprint::Element {
                tag,
                classes,
                attributes,
                children,
                ..
            } => {
                let is_hovered = hovered_path.map_or(false, |h| h.starts_with(current_path));
                let mut style = match parent_style {
                    Some(p) => ComputedStyle::inherit_from(p),
                    None => ComputedStyle::default(),
                };
                Self::apply_tailwind_classes(&mut style, classes, is_hovered, false, false);

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
                        strata_layout::StrokeLineCap::Round,
                        strata_layout::StrokeLineJoin::Round,
                        Vec::new(),
                        strata_layout::FillRule::NonZero,
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
                    if style.cursor == strata_style::types::Cursor::Default {
                        style.cursor = strata_style::types::Cursor::Text;
                    }
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
                    let is_mono = strata_layout::is_monospace_family(style.font_family);
                    let char_w = if is_mono { style.font_size * 0.60 } else { style.font_size * 0.55 };
                    if style.height == Dimension::Auto {
                        let text_h = style.font_size * style.line_height;
                        style.height = Dimension::Px(text_h + style.padding.top + style.padding.bottom);
                    }
                    if let Some(val) = value_text {
                        let approx_width = (val.len() as f32 * char_w).max(40.0);
                        if style.width == Dimension::Auto && style.flex_grow <= 0.0 {
                            style.width = Dimension::Px(approx_width + style.padding.left + style.padding.right);
                        }
                        return LayoutNode::with_text(style, val);
                    } else if let Some(ph) = placeholder_text {
                        let approx_width = (ph.len() as f32 * char_w).max(40.0);
                        if style.width == Dimension::Auto && style.flex_grow <= 0.0 {
                            style.width = Dimension::Px(approx_width + style.padding.left + style.padding.right);
                        }
                        style.color = strata_style::types::Color::new(140, 145, 155, 255);
                        return LayoutNode::with_text(style, ph);
                    } else {
                        if style.width == Dimension::Auto && style.flex_grow <= 0.0 {
                            style.width = Dimension::Px(60.0 + style.padding.left + style.padding.right);
                        }
                        return LayoutNode::with_text(style, String::new());
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
                                | "table"
                                | "inline-table"
                                | "table-row-group"
                                | "table-header-group"
                                | "table-footer-group"
                                | "table-row"
                                | "table-cell"
                                | "table-column-group"
                                | "table-column"
                                | "table-caption"
                        )
                    })
                });

                if !has_display_class {
                    match tag {
                        Tag::Table => {
                            style.display = Display::Table;
                        }
                        Tag::Thead => {
                            style.display = Display::TableHeaderGroup;
                        }
                        Tag::Tbody => {
                            style.display = Display::TableRowGroup;
                        }
                        Tag::Tr => {
                            style.display = Display::TableRow;
                        }
                        Tag::Th | Tag::Td => {
                            style.display = Display::TableCell;
                        }
                        Tag::Div
                        | Tag::Main
                        | Tag::Section
                        | Tag::Header
                        | Tag::Footer
                        | Tag::Nav
                        | Tag::Aside
                        | Tag::Ul
                        | Tag::Ol
                        | Tag::Form => {
                            style.display = Display::Flex;
                            style.flex_direction = FlexDirection::Column;
                        }
                        Tag::P => {
                            style.display = Display::Block;
                        }
                        Tag::H1
                        | Tag::H2
                        | Tag::H3
                        | Tag::H4
                        | Tag::H5
                        | Tag::H6 => {
                            style.display = Display::Flex;
                            style.flex_direction = FlexDirection::Column;
                        }
                        Tag::Button => {
                            style.display = Display::InlineFlex;
                            style.align_items = strata_style::types::AlignItems::Center;
                            style.justify_content = strata_style::types::JustifyContent::Center;
                        }
                        Tag::Span => {
                            style.display = Display::Inline;
                        }
                        Tag::A => {
                            style.display = Display::Inline;
                            if style.text_decoration == strata_style::types::TextDecoration::None {
                                style.text_decoration = strata_style::types::TextDecoration::Underline;
                            }
                        }
                        _ => {
                            style.display = Display::Flex;
                        }
                    }
                }

                let mut has_direct_style = false;
                for attr in attributes {
                    match attr {
                        strata_dom::Attribute::DirectStyle(any) => {
                            if let Some(decls) = any.downcast_ref::<Vec<strata_style::Declaration>>() {
                                for decl in decls {
                                    strata_style::CascadeSolver::apply_declaration(&mut style, decl);
                                }
                                has_direct_style = true;
                            } else if let Some(cs) = any.downcast_ref::<strata_style::types::ComputedStyle>() {
                                style = cs.clone();
                                has_direct_style = true;
                            }
                        }
                        strata_dom::Attribute::Property { key, value } => {
                            if key.eq_ignore_ascii_case("href") && !value.is_empty() {
                                let val_atom = strata_primitives::atom::intern(value);
                                style.set_custom_property(strata_primitives::AtomId::HREF, val_atom);
                                if matches!(tag, Tag::A) && style.color == strata_style::types::Color::new(0, 0, 0, 255) {
                                    style.color = strata_style::types::Color::new(37, 99, 235, 255);
                                }
                            } else if key.eq_ignore_ascii_case("colspan") {
                                if let Ok(cs) = value.parse::<u32>() {
                                    style.colspan = cs.max(1);
                                }
                            } else if key.eq_ignore_ascii_case("rowspan") {
                                if let Ok(rs) = value.parse::<u32>() {
                                    style.rowspan = rs.max(1);
                                }
                            } else if key.eq_ignore_ascii_case("style") && !value.is_empty() {
                                if !has_direct_style {
                                    let decls = strata_style::parse_declarations(value);
                                    for decl in &decls {
                                        strata_style::CascadeSolver::apply_declaration(&mut style, decl);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }

                if matches!(tag, Tag::Button)
                    && style.display == Display::Flex
                    && style.width == Dimension::Auto
                    && style.flex_grow == 0.0
                {
                    style.display = Display::InlineFlex;
                }

                let is_inline = matches!(
                    style.display,
                    Display::Inline | Display::InlineFlex | Display::InlineBlock
                );

                // If inline element has auto width and contains only a text child, compute intrinsic text dimensions
                if is_inline && style.width == Dimension::Auto && children.len() == 1 {
                    if let NodeBlueprint::Text(t) = &children[0] {
                        let is_mono = strata_layout::is_monospace_family(style.font_family);
                        let weight_scale = font_weight_scale(style.font_weight);
                        let (raw_w, raw_h) = strata_layout::measure_text_bounds_ext(t, style.font_size, style.line_height, None, is_mono);
                        let text_w = raw_w * weight_scale;
                        let total_w = text_w
                            + style.padding.left
                            + style.padding.right
                            + style.border_width.left
                            + style.border_width.right;
                        style.width = Dimension::Px(total_w);
                        if style.height == Dimension::Auto {
                            let total_h = raw_h
                                + style.padding.top
                                + style.padding.bottom
                                + style.border_width.top
                                + style.border_width.bottom;
                            style.height = Dimension::Px(total_h);
                        }
                    }
                }

                let mut child_nodes = Vec::with_capacity(children.len());
                let mut child_counter = 0;
                for child in children {
                    Self::append_child_layout_nodes_with_context(
                        child,
                        Some(&style),
                        current_path,
                        &mut child_counter,
                        hovered_path,
                        &mut child_nodes,
                    );
                }

                LayoutNode::with_children(style, child_nodes)
            }
            NodeBlueprint::Text(text) => {
                let mut style = match parent_style {
                    Some(p) => ComputedStyle::inherit_from(p),
                    None => ComputedStyle::default(),
                };
                style.display = Display::Inline;
                let is_mono = strata_layout::is_monospace_family(style.font_family);
                let weight_scale = font_weight_scale(style.font_weight);
                let (raw_w, h) = strata_layout::measure_text_bounds_ext(text, style.font_size, style.line_height, None, is_mono);
                let w = raw_w * weight_scale;
                style.width = Dimension::Px(w);
                style.height = Dimension::Px(h);
                LayoutNode::with_text(style, text.clone())
            }
            NodeBlueprint::DynamicText(dyn_fn) => {
                let text = dyn_fn();
                let mut style = match parent_style {
                    Some(p) => ComputedStyle::inherit_from(p),
                    None => ComputedStyle::default(),
                };
                style.display = Display::Inline;
                let is_mono = strata_layout::is_monospace_family(style.font_family);
                let weight_scale = font_weight_scale(style.font_weight);
                let (raw_w, h) = strata_layout::measure_text_bounds_ext(&text, style.font_size, style.line_height, None, is_mono);
                let w = raw_w * weight_scale;
                style.width = Dimension::Px(w);
                style.height = Dimension::Px(h);
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
                let mut child_counter = 0;
                for child in children {
                    Self::append_child_layout_nodes_with_context(
                        child,
                        Some(&style),
                        current_path,
                        &mut child_counter,
                        hovered_path,
                        &mut child_nodes,
                    );
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

/// Helper to convert SVG points attribute into path commands.
fn parse_points_to_path(points_str: &str, closed: bool) -> Option<String> {
    let coords: Vec<f32> = points_str
        .replace(',', " ")
        .split_whitespace()
        .filter_map(|s| s.parse::<f32>().ok())
        .collect();
    if coords.len() < 2 {
        return None;
    }
    let mut path = format!("M {} {}", coords[0], coords[1]);
    for chunk in coords[2..].chunks(2) {
        if chunk.len() == 2 {
            path.push_str(&format!(" L {} {}", chunk[0], chunk[1]));
        }
    }
    if closed {
        path.push_str(" Z");
    }
    Some(path)
}

/// Helper function to extract all SVG vector shapes from a blueprint hierarchy.
fn collect_svg_items_from_blueprint(
    blueprint: &NodeBlueprint,
    default_view_box: (f32, f32),
    inherited_stroke: Option<strata_style::types::Color>,
    inherited_fill: Option<strata_style::types::Color>,
    inherited_stroke_width: f32,
    inherited_linecap: strata_layout::StrokeLineCap,
    inherited_linejoin: strata_layout::StrokeLineJoin,
    inherited_dasharray: Vec<f32>,
    inherited_fill_rule: strata_layout::FillRule,
    parent_color: strata_style::types::Color,
) -> Vec<strata_layout::SvgItem> {
    let mut items = Vec::new();
    match blueprint {
        NodeBlueprint::Element {
            tag,
            attributes,
            children,
            ..
        } => {
            let mut d = None;
            let mut stroke = inherited_stroke;
            let mut fill = inherited_fill;
            let mut stroke_width = inherited_stroke_width;
            let mut stroke_linecap = inherited_linecap;
            let mut stroke_linejoin = inherited_linejoin;
            let mut stroke_dasharray = inherited_dasharray.clone();
            let mut fill_rule = inherited_fill_rule;

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

            let mut points = None;
            let is_polygon = match tag {
                Tag::Custom(s) => *s == "polygon",
                _ => false,
            };

            for attr in attributes {
                if let strata_dom::Attribute::Property { key, value } = attr {
                    match key.as_str() {
                        "d" => d = Some(value.clone()),
                        "stroke" => {
                            if value == "currentColor" {
                                stroke = Some(parent_color);
                            } else if value == "none" {
                                stroke = None;
                            } else if let Some(c) = strata_style::parser::parse_color(value) {
                                stroke = Some(c);
                            }
                        }
                        "fill" => {
                            if value == "currentColor" {
                                fill = Some(parent_color);
                            } else if value == "none" {
                                fill = None;
                            } else if let Some(c) = strata_style::parser::parse_color(value) {
                                fill = Some(c);
                            }
                        }
                        "stroke-width" => {
                            if let Ok(w) = value.parse::<f32>() {
                                stroke_width = w;
                            }
                        }
                        "stroke-linecap" => match value.as_str() {
                            "round" => stroke_linecap = strata_layout::StrokeLineCap::Round,
                            "square" => stroke_linecap = strata_layout::StrokeLineCap::Square,
                            "butt" => stroke_linecap = strata_layout::StrokeLineCap::Butt,
                            _ => {}
                        },
                        "stroke-linejoin" => match value.as_str() {
                            "round" => stroke_linejoin = strata_layout::StrokeLineJoin::Round,
                            "bevel" => stroke_linejoin = strata_layout::StrokeLineJoin::Bevel,
                            "miter" => stroke_linejoin = strata_layout::StrokeLineJoin::Miter,
                            _ => {}
                        },
                        "stroke-dasharray" => {
                            stroke_dasharray = value
                                .replace(',', " ")
                                .split_whitespace()
                                .filter_map(|s| s.parse::<f32>().ok())
                                .collect();
                        }
                        "fill-rule" => match value.as_str() {
                            "evenodd" => fill_rule = strata_layout::FillRule::EvenOdd,
                            "nonzero" => fill_rule = strata_layout::FillRule::NonZero,
                            _ => {}
                        },
                        "points" => points = Some(value.clone()),
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
                    stroke_linecap,
                    stroke_linejoin,
                    stroke_dasharray: stroke_dasharray.clone(),
                    fill_rule,
                    view_box: default_view_box,
                });
            } else if let Some(pts) = points {
                if let Some(poly_path) = Self::parse_points_to_path(&pts, is_polygon) {
                    items.push(strata_layout::SvgItem {
                        path_data: poly_path,
                        stroke_color: stroke,
                        fill_color: fill,
                        stroke_width,
                        stroke_linecap,
                        stroke_linejoin,
                        stroke_dasharray: stroke_dasharray.clone(),
                        fill_rule,
                        view_box: default_view_box,
                    });
                }
            } else if has_line {
                items.push(strata_layout::SvgItem {
                    path_data: format!("M {} {} L {} {}", x1, y1, x2, y2),
                    stroke_color: stroke.or(Some(parent_color)),
                    fill_color: None,
                    stroke_width,
                    stroke_linecap,
                    stroke_linejoin,
                    stroke_dasharray: stroke_dasharray.clone(),
                    fill_rule,
                    view_box: default_view_box,
                });
            } else if has_circle && r > 0.0 {
                items.push(strata_layout::SvgItem {
                    path_data: format!("M {} {} a {} {} 0 1 0 {} 0 a {} {} 0 1 0 {} 0 Z", cx - r, cy, r, r, 2.0 * r, r, r, -2.0 * r),
                    stroke_color: stroke,
                    fill_color: fill,
                    stroke_width,
                    stroke_linecap,
                    stroke_linejoin,
                    stroke_dasharray: stroke_dasharray.clone(),
                    fill_rule,
                    view_box: default_view_box,
                });
            } else if has_ellipse && rx > 0.0 && ry > 0.0 {
                items.push(strata_layout::SvgItem {
                    path_data: format!("M {} {} a {} {} 0 1 0 {} 0 a {} {} 0 1 0 {} 0 Z", cx - rx, cy, rx, ry, 2.0 * rx, rx, ry, -2.0 * rx),
                    stroke_color: stroke,
                    fill_color: fill,
                    stroke_width,
                    stroke_linecap,
                    stroke_linejoin,
                    stroke_dasharray: stroke_dasharray.clone(),
                    fill_rule,
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
                    stroke_linecap,
                    stroke_linejoin,
                    stroke_dasharray: stroke_dasharray.clone(),
                    fill_rule,
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
                    stroke_linecap,
                    stroke_linejoin,
                    stroke_dasharray.clone(),
                    fill_rule,
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
    fn append_child_layout_nodes_with_context(
        blueprint: &NodeBlueprint,
        parent_style: Option<&ComputedStyle>,
        parent_path: &mut Vec<usize>,
        child_counter: &mut usize,
        hovered_path: Option<&[usize]>,
        out: &mut Vec<LayoutNode>,
    ) {
        if let NodeBlueprint::Fragment(children) = blueprint {
            for child in children {
                Self::append_child_layout_nodes_with_context(
                    child,
                    parent_style,
                    parent_path,
                    child_counter,
                    hovered_path,
                    out,
                );
            }
        } else {
            let this_idx = *child_counter;
            *child_counter += 1;
            parent_path.push(this_idx);
            let node = Self::build_layout_tree_with_context(
                blueprint,
                parent_style,
                parent_path,
                hovered_path,
            );
            parent_path.pop();
            out.push(node);
        }
    }

    /// Recursively flattens Fragment blueprints so child elements splice directly into parent layout contexts.
    pub fn append_child_layout_nodes(
        blueprint: &NodeBlueprint,
        parent_style: Option<&ComputedStyle>,
        out: &mut Vec<LayoutNode>,
    ) {
        let mut path_buf = Vec::new();
        let mut child_counter = out.len();
        Self::append_child_layout_nodes_with_context(
            blueprint,
            parent_style,
            &mut path_buf,
            &mut child_counter,
            None,
            out,
        );
    }

    /// Full Tailwind CSS class parser supporting hover, focus, and dark mode variants.
    pub fn apply_tailwind_classes(
        style: &mut ComputedStyle,
        classes: &[String],
        is_hovered: bool,
        is_focused: bool,
        is_dark: bool,
    ) {
        for cls in classes {
            for single_class in cls.split_whitespace() {
                strata_style::tailwind::TailwindEngine::apply_class_with_state(
                    style,
                    single_class,
                    is_hovered,
                    is_focused,
                    is_dark,
                );
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

    /// Unprojects a world pointer coordinate into an element's local coordinate system if transformed.
    #[inline]
    pub fn unproject_point(layout: &LayoutNode, px: f32, py: f32) -> (f32, f32) {
        if layout.style.has_transform() {
            let mat = layout.style.compute_transform_matrix(
                layout.geometry.border_box.width,
                layout.geometry.border_box.height,
            );
            if let Some(inv) = mat.inverse() {
                let lx = px - layout.geometry.border_box.x;
                let ly = py - layout.geometry.border_box.y;
                let (un_lx, un_ly) = inv.transform_point(lx, ly);
                (
                    layout.geometry.border_box.x + un_lx,
                    layout.geometry.border_box.y + un_ly,
                )
            } else {
                (px, py)
            }
        } else {
            (px, py)
        }
    }

    /// Recursively hit tests a coordinate point against the blueprint and computed layout hierarchy,
    /// returning the nearest (deepest, front-most) listener registered for `event_type`.
    /// Recursively hit tests a coordinate point against the blueprint and computed layout hierarchy,
    /// returning the nearest listener registered for `event_type` and its element border box.
    pub fn hit_test_event_with_box<'a>(
        blueprint: &'a NodeBlueprint,
        layout: &'a LayoutNode,
        px: f32,
        py: f32,
        event_type: strata_dom::EventType,
    ) -> Option<(std::rc::Rc<dyn Fn(strata_dom::Event)>, &'a strata_layout::Rect)> {
        let (px, py) = Self::unproject_point(layout, px, py);

        // If element clips overflow, coordinates outside its border box are clipped
        if layout.style.overflow_x != strata_style::types::Overflow::Visible
            || layout.style.overflow_y != strata_style::types::Overflow::Visible
        {
            if !layout.geometry.border_box.contains_point(px, py) {
                return None;
            }
        }

        match blueprint {
            NodeBlueprint::Element {
                attributes,
                children,
                ..
            } => {
                let has_fragments = children.iter().any(|c| matches!(c, NodeBlueprint::Fragment(_)));
                if !has_fragments {
                    for (child_bp, child_layout) in children.iter().zip(layout.children.iter()).rev() {
                        if let Some(res) = Self::hit_test_event_with_box(child_bp, child_layout, px, py, event_type) {
                            return Some(res);
                        }
                    }
                } else {
                    let mut flat_children = Vec::new();
                    for c in children {
                        Self::flatten_blueprints(c, &mut flat_children);
                    }

                    for (&child_bp, child_layout) in flat_children.iter().zip(layout.children.iter()).rev() {
                        if let Some(res) = Self::hit_test_event_with_box(child_bp, child_layout, px, py, event_type) {
                            return Some(res);
                        }
                    }
                }

                if layout.geometry.border_box.contains_point(px, py) {
                    for attr in attributes {
                        if let strata_dom::Attribute::EventListener {
                            event: ev,
                            handler,
                        } = attr
                        {
                            if *ev != event_type {
                                continue;
                            }
                            return Some((handler.clone(), &layout.geometry.border_box));
                        }
                    }
                }
                None
            }
            NodeBlueprint::Fragment(children) => {
                for (child_bp, child_layout) in children.iter().zip(layout.children.iter()).rev() {
                    if let Some(res) = Self::hit_test_event_with_box(child_bp, child_layout, px, py, event_type) {
                        return Some(res);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Recursively hit tests a coordinate point against the blueprint and computed layout hierarchy,
    /// returning the nearest (deepest, front-most) listener registered for `event_type`.
    pub fn hit_test_event(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        px: f32,
        py: f32,
        event_type: strata_dom::EventType,
    ) -> Option<std::rc::Rc<dyn Fn(strata_dom::Event)>> {
        Self::hit_test_event_with_box(blueprint, layout, px, py, event_type).map(|(h, _)| h)
    }

    /// Hit tests for the nearest `onclick` listener at a coordinate point.
    pub fn hit_test_click(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        px: f32,
        py: f32,
    ) -> Option<std::rc::Rc<dyn Fn(strata_dom::Event)>> {
        Self::hit_test_event(blueprint, layout, px, py, strata_dom::EventType::Click)
    }

    /// Recursively hit tests a coordinate point against the blueprint and computed layout hierarchy to determine cursor style.
    pub fn hit_test_cursor(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        px: f32,
        py: f32,
    ) -> strata_platform::window::CursorIcon {
        let (px, py) = Self::unproject_point(layout, px, py);

        // If element clips overflow, coordinates outside its border box are clipped
        if layout.style.overflow_x != strata_style::types::Overflow::Visible
            || layout.style.overflow_y != strata_style::types::Overflow::Visible
        {
            if !layout.geometry.border_box.contains_point(px, py) {
                return strata_platform::window::CursorIcon::Default;
            }
        }

        match blueprint {
            NodeBlueprint::Element {
                tag,
                classes,
                attributes,
                children,
            } => {
                let has_fragments = children.iter().any(|c| matches!(c, NodeBlueprint::Fragment(_)));
                if !has_fragments {
                    for (child_bp, child_layout) in children.iter().zip(layout.children.iter()).rev() {
                        let child_cur = Self::hit_test_cursor(child_bp, child_layout, px, py);
                        if child_cur != strata_platform::window::CursorIcon::Default {
                            return child_cur;
                        }
                    }
                } else {
                    let mut flat_children = Vec::new();
                    for c in children {
                        Self::flatten_blueprints(c, &mut flat_children);
                    }

                    for (&child_bp, child_layout) in flat_children.iter().zip(layout.children.iter()).rev() {
                        let child_cur = Self::hit_test_cursor(child_bp, child_layout, px, py);
                        if child_cur != strata_platform::window::CursorIcon::Default {
                            return child_cur;
                        }
                    }
                }

                if layout.geometry.border_box.contains_point(px, py) {
                    match layout.style.cursor {
                        strata_style::types::Cursor::Pointer => return strata_platform::window::CursorIcon::Pointer,
                        strata_style::types::Cursor::Text => return strata_platform::window::CursorIcon::Text,
                        strata_style::types::Cursor::Crosshair => return strata_platform::window::CursorIcon::Crosshair,
                        strata_style::types::Cursor::Move => return strata_platform::window::CursorIcon::Move,
                        strata_style::types::Cursor::NotAllowed => return strata_platform::window::CursorIcon::NotAllowed,
                        strata_style::types::Cursor::Wait => return strata_platform::window::CursorIcon::Wait,
                        strata_style::types::Cursor::Grab => return strata_platform::window::CursorIcon::Grab,
                        strata_style::types::Cursor::Grabbing => return strata_platform::window::CursorIcon::Grabbing,
                        strata_style::types::Cursor::ColResize => return strata_platform::window::CursorIcon::ColResize,
                        strata_style::types::Cursor::RowResize => return strata_platform::window::CursorIcon::RowResize,
                        strata_style::types::Cursor::Help => return strata_platform::window::CursorIcon::Help,
                        strata_style::types::Cursor::None => return strata_platform::window::CursorIcon::None,
                        strata_style::types::Cursor::Default => {}
                    }

                    let has_pointer = matches!(tag, Tag::Button | Tag::A)
                        || classes.iter().any(|c| c.split_whitespace().any(|tok| tok == "cursor-pointer" || tok == "pointer"))
                        || attributes.iter().any(|a| matches!(a, strata_dom::Attribute::EventListener { event: strata_dom::EventType::Click, .. }));

                    if has_pointer {
                        return strata_platform::window::CursorIcon::Pointer;
                    }

                    let has_text_cursor = matches!(tag, Tag::Input | Tag::Textarea)
                        || classes.iter().any(|c| c.split_whitespace().any(|tok| tok == "cursor-text" || tok == "select-text"));

                    if has_text_cursor {
                        return strata_platform::window::CursorIcon::Text;
                    }
                }

                strata_platform::window::CursorIcon::Default
            }
            NodeBlueprint::Fragment(children) => {
                for (child_bp, child_layout) in children.iter().zip(layout.children.iter()).rev() {
                    let child_cur = Self::hit_test_cursor(child_bp, child_layout, px, py);
                    if child_cur != strata_platform::window::CursorIcon::Default {
                        return child_cur;
                    }
                }
                strata_platform::window::CursorIcon::Default
            }
            _ => strata_platform::window::CursorIcon::Default,
        }
    }

    /// Recursively hit tests a coordinate point against the blueprint to find clickable text input elements.
    pub fn hit_test_input(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        px: f32,
        py: f32,
    ) -> Option<InputHitTarget> {
        let mut candidates = Vec::new();
        let mut counter = 0;
        Self::collect_input_candidates(blueprint, layout, &mut counter, &mut candidates);

        for (target, border_box) in candidates.into_iter().rev() {
            if border_box.contains_point(px, py) {
                return Some(target);
            }
        }
        None
    }

    fn collect_input_candidates(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        counter: &mut usize,
        out: &mut Vec<(InputHitTarget, strata_layout::Rect)>,
    ) {
        match blueprint {
            NodeBlueprint::Element {
                tag,
                attributes,
                children,
                ..
            } => {
                let is_input = matches!(tag, Tag::Input | Tag::Textarea);
                if is_input {
                    let this_idx = *counter;
                    *counter += 1;

                    let mut current_value = String::new();
                    let mut input_id = None;
                    let mut on_input = None;
                    let mut on_change = None;
                    let mut on_submit = None;
                    let mut on_focus = None;
                    let mut on_blur = None;

                    for attr in attributes {
                        match attr {
                            strata_dom::Attribute::Property { key, value } => {
                                if key == "value" {
                                    current_value = value.clone();
                                } else if key == "id" {
                                    input_id = Some(value.clone());
                                }
                            }
                            strata_dom::Attribute::EventListener { event, handler } => match event {
                                strata_dom::EventType::Input => on_input = Some(handler.clone()),
                                strata_dom::EventType::Change => on_change = Some(handler.clone()),
                                strata_dom::EventType::Submit => on_submit = Some(handler.clone()),
                                strata_dom::EventType::Focus => on_focus = Some(handler.clone()),
                                strata_dom::EventType::Blur => on_blur = Some(handler.clone()),
                                _ => {}
                            },
                            _ => {}
                        }
                    }

                    let is_mono = strata_layout::is_monospace_family(layout.style.font_family);
                    out.push((
                        InputHitTarget {
                            input_index: this_idx,
                            input_id,
                            current_value,
                            font_size: layout.style.font_size,
                            line_height: layout.style.line_height,
                            is_mono,
                            content_box_x: layout.geometry.content_box.x,
                            content_box_y: layout.geometry.content_box.y,
                            on_input,
                            on_change,
                            on_submit,
                            on_focus,
                            on_blur,
                        },
                        layout.geometry.border_box,
                    ));
                }

                let mut flat = Vec::new();
                for c in children {
                    Self::flatten_blueprints(c, &mut flat);
                }

                for (&child_bp, child_layout) in flat.iter().zip(layout.children.iter()) {
                    Self::collect_input_candidates(child_bp, child_layout, counter, out);
                }
            }
            NodeBlueprint::Fragment(children) => {
                for c in children {
                    let mut flat = Vec::new();
                    Self::flatten_blueprints(c, &mut flat);
                    for (&child_bp, child_layout) in flat.iter().zip(layout.children.iter()) {
                        Self::collect_input_candidates(child_bp, child_layout, counter, out);
                    }
                }
            }
            _ => {}
        }
    }

    /// Locates the LayoutNode corresponding to a target input index or id in document order.
    pub fn find_input_layout<'a>(
        blueprint: &NodeBlueprint,
        layout: &'a LayoutNode,
        target_index: usize,
        target_id: Option<&str>,
        counter: &mut usize,
    ) -> Option<&'a LayoutNode> {
        match blueprint {
            NodeBlueprint::Element {
                tag,
                attributes,
                children,
                ..
            } => {
                let is_input = matches!(tag, Tag::Input | Tag::Textarea);
                if is_input {
                    let this_idx = *counter;
                    *counter += 1;

                    let mut matches_id = false;
                    if let Some(tid) = target_id {
                        for attr in attributes {
                            if let strata_dom::Attribute::Property { key, value } = attr {
                                if key == "id" && value == tid {
                                    matches_id = true;
                                    break;
                                }
                            }
                        }
                    }

                    let is_match = if target_id.is_some() {
                        matches_id
                    } else {
                        this_idx == target_index
                    };

                    if is_match {
                        return Some(layout);
                    }
                }

                let mut flat = Vec::new();
                for c in children {
                    Self::flatten_blueprints(c, &mut flat);
                }

                for (&child_bp, child_layout) in flat.iter().zip(layout.children.iter()) {
                    if let Some(found) = Self::find_input_layout(child_bp, child_layout, target_index, target_id, counter) {
                        return Some(found);
                    }
                }

                None
            }
            NodeBlueprint::Fragment(children) => {
                for c in children {
                    let mut flat = Vec::new();
                    Self::flatten_blueprints(c, &mut flat);
                    for (&child_bp, child_layout) in flat.iter().zip(layout.children.iter()) {
                        if let Some(found) = Self::find_input_layout(child_bp, child_layout, target_index, target_id, counter) {
                            return Some(found);
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Calculates the exact character index from an x offset within an input using active font metrics and character advances.
    pub fn calculate_exact_char_index_at_offset(
        text: &str,
        rel_x: f32,
        font_size: f32,
        line_height: f32,
        is_mono: bool,
    ) -> usize {
        if rel_x <= 0.0 || text.is_empty() {
            return 0;
        }
        let chars: Vec<char> = text.chars().collect();
        let total = chars.len();

        let (full_w, _) = strata_layout::text::measure_text_bounds_ext(text, font_size, line_height, None, is_mono);
        if rel_x >= full_w {
            return total;
        }

        let mut prev_w = 0.0f32;
        for i in 1..=total {
            let prefix: String = chars[..i].iter().collect();
            let (w, _) = strata_layout::text::measure_text_bounds_ext(&prefix, font_size, line_height, None, is_mono);
            let mid = (prev_w + w) / 2.0;
            if rel_x < mid {
                return i - 1;
            } else if rel_x <= w {
                return i;
            }
            prev_w = w;
        }
        total
    }

    /// Dispatches pointer down at (px, py) coordinates to manage native input element focus, multi-click, and text selection.
    /// Returns true if focus or selection state changed.
    pub fn dispatch_pointer_down(&mut self, px: f32, py: f32) -> bool {
        // Surfaces marked `data-keep-focus` (context menus, toolbars) never steal focus from the
        // active text input, so commands such as Cut/Paste act on the field the user was editing.
        if Self::hit_test_property(&self.root_blueprint, &self.layout_root, px, py, "data-keep-focus").is_some() {
            self.drag_selection_active = false;
            return false;
        }

        let mut input_handled = false;
        if let Some(target) = Self::hit_test_input(&self.root_blueprint, &self.layout_root, px, py) {
            let target_key = target.input_id.clone().unwrap_or_else(|| format!("input:{}", target.input_index));
            let text_val = if let Some(ref active) = self.active_focus {
                let current_key = active.input_id.clone().unwrap_or_else(|| format!("input:{}", active.input_index));
                if current_key == target_key {
                    active.state.text.get()
                } else {
                    target.current_value.clone()
                }
            } else {
                target.current_value.clone()
            };
            let rel_x = (px - target.content_box_x).max(0.0);
            let char_idx = Self::calculate_exact_char_index_at_offset(
                &text_val,
                rel_x,
                target.font_size,
                target.line_height,
                target.is_mono,
            );

            let now = std::time::Instant::now();
            let click_count = if let Some((last_time, last_x, last_y, count)) = self.last_click_info {
                if now.duration_since(last_time).as_millis() < 400 && (px - last_x).abs() < 5.0 && (py - last_y).abs() < 5.0 {
                    count + 1
                } else {
                    1
                }
            } else {
                1
            };
            self.last_click_info = Some((now, px, py, click_count));
            self.reset_caret_blink();

            let target_key = target.input_id.clone().unwrap_or_else(|| format!("input:{}", target.input_index));

            // If the same input is already active, handle multi-click or reposition caret
            if let Some(ref mut active) = self.active_focus {
                let current_key = active.input_id.clone().unwrap_or_else(|| format!("input:{}", active.input_index));
                if current_key == target_key {
                    if click_count == 2 {
                        active.state.select_word_at_index(char_idx);
                        self.drag_selection_active = false;
                    } else if click_count >= 3 {
                        active.state.select_all();
                        self.drag_selection_active = false;
                    } else {
                        active.state.cursor_index.set(char_idx);
                        active.state.selection_anchor.set(Some(char_idx));
                        self.drag_selection_active = true;
                    }
                    return true;
                }

                // Switching away from previous input: persist its undo/redo history in input_states
                self.input_states.insert(current_key, active.state.clone());
                if let Some(ref blur_cb) = active.on_blur {
                    blur_cb(strata_dom::Event::new(strata_dom::EventType::Blur));
                }
            }

            // Restore existing input state (with its private undo/redo history) or initialize new one
            let state = self.input_states.remove(&target_key).unwrap_or_else(|| {
                strata_dom::text_input::TextInputState::with_text(&target.current_value)
            });

            // Sync external value if state was empty
            if state.text.get().is_empty() && !target.current_value.is_empty() {
                state.text.set(target.current_value.clone());
            }

            if click_count == 2 {
                state.select_word_at_index(char_idx);
                self.drag_selection_active = false;
            } else if click_count >= 3 {
                state.select_all();
                self.drag_selection_active = false;
            } else {
                state.cursor_index.set(char_idx);
                state.selection_anchor.set(Some(char_idx));
                self.drag_selection_active = true;
            }
            state.is_focused.set(true);

            if let Some(ref focus_cb) = target.on_focus {
                focus_cb(strata_dom::Event::new(strata_dom::EventType::Focus));
            }

            self.active_focus = Some(ActiveInput {
                input_index: target.input_index,
                input_id: target.input_id,
                state,
                on_input: target.on_input,
                on_change: target.on_change,
                on_submit: target.on_submit,
                on_blur: target.on_blur,
            });
            input_handled = true;
        } else if let Some(active) = self.active_focus.take() {
            let key = active.input_id.clone().unwrap_or_else(|| format!("input:{}", active.input_index));
            self.input_states.insert(key, active.state.clone());
            self.drag_selection_active = false;
            self.last_click_info = None;
            if let Some(ref blur_cb) = active.on_blur {
                blur_cb(strata_dom::Event::new(strata_dom::EventType::Blur));
            }
            input_handled = true;
        } else {
            self.drag_selection_active = false;
        }

        // Also dispatch PointerDown and MouseDown to DOM listeners
        let mut dom_handled = false;
        if let Some((handler, target_box)) = Self::hit_test_event_with_box(&self.root_blueprint, &self.layout_root, px, py, strata_dom::EventType::PointerDown) {
            let ev = self.create_mouse_event(strata_dom::EventType::PointerDown, px, py, 0, 1, Some(target_box));
            handler(ev);
            dom_handled = true;
        }
        if let Some((handler, target_box)) = Self::hit_test_event_with_box(&self.root_blueprint, &self.layout_root, px, py, strata_dom::EventType::MouseDown) {
            let ev = self.create_mouse_event(strata_dom::EventType::MouseDown, px, py, 0, 1, Some(target_box));
            handler(ev);
            dom_handled = true;
        }

        input_handled || dom_handled
    }

    /// Dispatches pointer move at (px, py) to update mouse drag text selection and DOM move listeners.
    pub fn dispatch_pointer_move(&mut self, px: f32, py: f32) -> bool {
        let mut handled = false;
        if self.drag_selection_active {
            if let Some(ref mut active) = self.active_focus {
                let mut current_idx = 0;
                let target_idx = active.input_index;
                let target_id = active.input_id.as_deref();

                if let Some(layout) = Self::find_input_layout(
                    &self.root_blueprint,
                    &self.layout_root,
                    target_idx,
                    target_id,
                    &mut current_idx,
                ) {
                    let rel_x = (px - layout.geometry.content_box.x).max(0.0);
                    let is_mono = strata_layout::is_monospace_family(layout.style.font_family);
                    let char_idx = Self::calculate_exact_char_index_at_offset(
                        &active.state.text.get(),
                        rel_x,
                        layout.style.font_size,
                        layout.style.line_height,
                        is_mono,
                    );
                    active.state.cursor_index.set(char_idx);
                    self.reset_caret_blink();
                    handled = true;
                }
            }
        }

        if let Some((handler, target_box)) = Self::hit_test_event_with_box(&self.root_blueprint, &self.layout_root, px, py, strata_dom::EventType::PointerMove) {
            let ev = self.create_mouse_event(strata_dom::EventType::PointerMove, px, py, 0, 0, Some(target_box));
            handler(ev);
            handled = true;
        }
        if let Some((handler, target_box)) = Self::hit_test_event_with_box(&self.root_blueprint, &self.layout_root, px, py, strata_dom::EventType::MouseMove) {
            let ev = self.create_mouse_event(strata_dom::EventType::MouseMove, px, py, 0, 0, Some(target_box));
            handler(ev);
            handled = true;
        }

        handled
    }

    /// Dispatches pointer up to finalize mouse drag text selection and notify DOM up listeners.
    pub fn dispatch_pointer_up(&mut self, px: f32, py: f32) -> bool {
        let mut handled = false;
        if self.drag_selection_active {
            self.drag_selection_active = false;
            if let Some(ref mut active) = self.active_focus {
                if let Some(anchor) = active.state.selection_anchor.get() {
                    if anchor == active.state.cursor_index.get() {
                        active.state.selection_anchor.set(None);
                    }
                }
            }
            handled = true;
        }

        if let Some((handler, target_box)) = Self::hit_test_event_with_box(&self.root_blueprint, &self.layout_root, px, py, strata_dom::EventType::PointerUp) {
            let ev = self.create_mouse_event(strata_dom::EventType::PointerUp, px, py, 0, 1, Some(target_box));
            handler(ev);
            handled = true;
        }
        if let Some((handler, target_box)) = Self::hit_test_event_with_box(&self.root_blueprint, &self.layout_root, px, py, strata_dom::EventType::MouseUp) {
            let ev = self.create_mouse_event(strata_dom::EventType::MouseUp, px, py, 0, 1, Some(target_box));
            handler(ev);
            handled = true;
        }

        handled
    }

    /// Re-evaluates styling with the current hover path, recomputes layout, and compiles display list.
    pub fn rebuild_layout_tree(&mut self) {
        self.applied_scroll_offsets.clear();
        self.layout_root = Self::build_layout_tree_with_hover(
            &self.root_blueprint,
            self.current_hover_path.as_deref(),
        );
        self.layout_root.compute_layout(self.viewport_width, self.viewport_height);
        self.apply_viewport_clamps();
        self.apply_scroll_offsets();
        self.compile_display_list();
    }

    /// Locates the blueprint and layout node at a hierarchical index path in the tree.
    pub fn find_node_by_path<'a>(
        blueprint: &'a NodeBlueprint,
        layout: &'a LayoutNode,
        path: &[usize],
    ) -> Option<(&'a NodeBlueprint, &'a LayoutNode)> {
        let mut cur_bp = blueprint;
        let mut cur_layout = layout;

        for &idx in path {
            match cur_bp {
                NodeBlueprint::Element { children, .. } => {
                    let mut flat = Vec::new();
                    for c in children {
                        Self::flatten_blueprints(c, &mut flat);
                    }
                    if idx >= flat.len() || idx >= cur_layout.children.len() {
                        return None;
                    }
                    cur_bp = flat[idx];
                    cur_layout = &cur_layout.children[idx];
                }
                NodeBlueprint::Fragment(children) => {
                    let mut flat = Vec::new();
                    for c in children {
                        Self::flatten_blueprints(c, &mut flat);
                    }
                    if idx >= flat.len() || idx >= cur_layout.children.len() {
                        return None;
                    }
                    cur_bp = flat[idx];
                    cur_layout = &cur_layout.children[idx];
                }
                _ => return None,
            }
        }

        Some((cur_bp, cur_layout))
    }

    /// Recursively hit tests a coordinate point against the blueprint and computed layout hierarchy,
    /// returning the hierarchical index path of the deepest matching element node.
    pub fn hit_test_node_path(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        px: f32,
        py: f32,
    ) -> Option<Vec<usize>> {
        let mut path = Vec::new();
        Self::hit_test_node_path_rec(blueprint, layout, px, py, &mut path)
    }

    fn hit_test_node_path_rec(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        px: f32,
        py: f32,
        current_path: &mut Vec<usize>,
    ) -> Option<Vec<usize>> {
        if matches!(layout.style.display, strata_style::types::Display::None) {
            return None;
        }
        if matches!(layout.style.visibility, strata_style::types::Visibility::Hidden) {
            return None;
        }
        if matches!(layout.style.pointer_events, strata_style::types::PointerEvents::None) {
            return None;
        }

        let (px, py) = Self::unproject_point(layout, px, py);

        // If element clips overflow, coordinates outside its border box are clipped
        if layout.style.overflow_x != strata_style::types::Overflow::Visible
            || layout.style.overflow_y != strata_style::types::Overflow::Visible
        {
            if !layout.geometry.border_box.contains_point(px, py) {
                return None;
            }
        }

        match blueprint {
            NodeBlueprint::Element { children, .. } => {
                let has_fragments = children.iter().any(|c| matches!(c, NodeBlueprint::Fragment(_)));
                if !has_fragments {
                    for (i, (child_bp, child_layout)) in children.iter().zip(layout.children.iter()).enumerate().rev() {
                        current_path.push(i);
                        if let Some(res) = Self::hit_test_node_path_rec(child_bp, child_layout, px, py, current_path) {
                            return Some(res);
                        }
                        current_path.pop();
                    }
                } else {
                    let mut flat_children = Vec::new();
                    for c in children {
                        Self::flatten_blueprints(c, &mut flat_children);
                    }

                    // Check children in reverse order (front-to-back stacking order)
                    for (i, (&child_bp, child_layout)) in flat_children.iter().zip(layout.children.iter()).enumerate().rev() {
                        current_path.push(i);
                        if let Some(res) = Self::hit_test_node_path_rec(child_bp, child_layout, px, py, current_path) {
                            return Some(res);
                        }
                        current_path.pop();
                    }
                }

                if layout.geometry.border_box.contains_point(px, py) {
                    Some(current_path.clone())
                } else {
                    None
                }
            }
            NodeBlueprint::Fragment(children) => {
                let mut flat_children = Vec::new();
                for c in children {
                    Self::flatten_blueprints(c, &mut flat_children);
                }

                for (i, (&child_bp, child_layout)) in flat_children.iter().zip(layout.children.iter()).enumerate().rev() {
                    current_path.push(i);
                    if let Some(res) = Self::hit_test_node_path_rec(child_bp, child_layout, px, py, current_path) {
                        return Some(res);
                    }
                    current_path.pop();
                }
                None
            }
            NodeBlueprint::Text(_) | NodeBlueprint::DynamicText(_) => {
                if layout.geometry.border_box.contains_point(px, py) {
                    Some(current_path.clone())
                } else {
                    None
                }
            }
            NodeBlueprint::Empty => None,
        }
    }

    /// Dispatches W3C boundary events (`mouseout`, `mouseleave`, `mouseover`, `mouseenter`)
    /// between the previous hover path and the newly active hover path.
    pub fn dispatch_boundary_events(
        &self,
        old_path: Option<&[usize]>,
        new_path: Option<&[usize]>,
        px: f32,
        py: f32,
    ) {
        let old_slice = old_path.unwrap_or(&[]);
        let new_slice = new_path.unwrap_or(&[]);

        let mut common_len = 0;
        if old_path.is_some() && new_path.is_some() {
            while common_len < old_slice.len() && common_len < new_slice.len() && old_slice[common_len] == new_slice[common_len] {
                common_len += 1;
            }
        }

        // Dispatches exit events for nodes no longer hovered (from deepest child up)
        if old_path.is_some() {
            let min_len = if new_path.is_some() { common_len + 1 } else { 0 };
            for len in (min_len..=old_slice.len()).rev() {
                let subpath = &old_slice[..len];
                if let Some((bp, layout)) = Self::find_node_by_path(&self.root_blueprint, &self.layout_root, subpath) {
                    self.dispatch_node_events(
                        bp,
                        layout,
                        &[strata_dom::EventType::MouseOut, strata_dom::EventType::MouseLeave],
                        px,
                        py,
                    );
                }
            }
        }

        // Dispatches enter events for newly hovered nodes (from shallowest ancestor down)
        if new_path.is_some() {
            let start_len = if old_path.is_some() { common_len + 1 } else { 0 };
            for len in start_len..=new_slice.len() {
                let subpath = &new_slice[..len];
                if let Some((bp, layout)) = Self::find_node_by_path(&self.root_blueprint, &self.layout_root, subpath) {
                    self.dispatch_node_events(
                        bp,
                        layout,
                        &[strata_dom::EventType::MouseOver, strata_dom::EventType::MouseEnter],
                        px,
                        py,
                    );
                }
            }
        }
    }

    /// Helper to invoke registered event listeners of specified types on a single blueprint node.
    fn dispatch_node_events(
        &self,
        bp: &NodeBlueprint,
        layout: &LayoutNode,
        event_types: &[strata_dom::EventType],
        px: f32,
        py: f32,
    ) {
        if let NodeBlueprint::Element { attributes, .. } = bp {
            for attr in attributes {
                if let strata_dom::Attribute::EventListener { event, handler } = attr {
                    if event_types.contains(event) {
                        let ev = self.create_mouse_event(*event, px, py, 0, 0, Some(&layout.geometry.border_box));
                        handler(ev);
                    }
                }
            }
        }
    }

    /// Checks whether any element along the index path has active hover styles or boundary handlers.
    pub fn node_path_has_hover_effects(blueprint: &NodeBlueprint, path: &[usize]) -> bool {
        let mut cur_bp = blueprint;
        if Self::node_has_hover_effects(cur_bp) {
            return true;
        }

        for &idx in path {
            match cur_bp {
                NodeBlueprint::Element { children, .. } => {
                    let mut flat = Vec::new();
                    for c in children {
                        Self::flatten_blueprints(c, &mut flat);
                    }
                    if idx >= flat.len() {
                        return false;
                    }
                    cur_bp = flat[idx];
                }
                NodeBlueprint::Fragment(children) => {
                    let mut flat = Vec::new();
                    for c in children {
                        Self::flatten_blueprints(c, &mut flat);
                    }
                    if idx >= flat.len() {
                        return false;
                    }
                    cur_bp = flat[idx];
                }
                _ => return false,
            }
            if Self::node_has_hover_effects(cur_bp) {
                return true;
            }
        }

        false
    }

    /// Checks if a single blueprint node has hover utility classes or boundary event listeners.
    pub fn node_has_hover_effects(node: &NodeBlueprint) -> bool {
        match node {
            NodeBlueprint::Element { classes, attributes, .. } => {
                let has_hover_class = classes.iter().any(|c| {
                    c.split_whitespace().any(|tok| tok.starts_with("hover:"))
                });
                if has_hover_class {
                    return true;
                }
                attributes.iter().any(|attr| match attr {
                    strata_dom::Attribute::EventListener { event, .. } => matches!(
                        event,
                        strata_dom::EventType::MouseEnter
                            | strata_dom::EventType::MouseLeave
                            | strata_dom::EventType::MouseOver
                            | strata_dom::EventType::MouseOut
                    ),
                    _ => false,
                })
            }
            _ => false,
        }
    }

    /// Continuously updates hover state from pointer coordinates.
    /// Dispatches boundary events and rebuilds layout tree if the hovered element changed and has hover effects.
    /// Returns true if the hover target changed and an interactive frame rebuild is required.
    pub fn update_hover(&mut self, px: f32, py: f32) -> bool {
        self.last_mouse_pos = (px, py);
        let new_path = Self::hit_test_node_path(&self.root_blueprint, &self.layout_root, px, py);
        if new_path == self.current_hover_path {
            return false;
        }

        let old_path = self.current_hover_path.take();
        self.current_hover_path = new_path.clone();

        self.dispatch_boundary_events(old_path.as_deref(), new_path.as_deref(), px, py);

        let old_has_effects = old_path.as_deref().map_or(false, |p| Self::node_path_has_hover_effects(&self.root_blueprint, p));
        let new_has_effects = new_path.as_deref().map_or(false, |p| Self::node_path_has_hover_effects(&self.root_blueprint, p));

        if old_has_effects || new_has_effects {
            self.rebuild_layout_tree();
            true
        } else {
            false
        }
    }

    /// Clears any active hover state when the pointer departs the window surface.
    pub fn clear_hover(&mut self) -> bool {
        if self.current_hover_path.is_none() {
            return false;
        }
        let old_path = self.current_hover_path.take();
        self.dispatch_boundary_events(old_path.as_deref(), None, self.last_mouse_pos.0, self.last_mouse_pos.1);
        let old_has_effects = old_path.as_deref().map_or(false, |p| Self::node_path_has_hover_effects(&self.root_blueprint, p));
        if old_has_effects {
            self.rebuild_layout_tree();
            true
        } else {
            false
        }
    }

    /// Dispatches keyboard events to the active focused input element.
    /// Returns true if the key event was consumed by the focused input.
    pub fn handle_key_event(&mut self, event: &PlatformEvent) -> bool {
        if self.active_focus.is_none() {
            if let PlatformEvent::KeyDown { key, .. } = event {
                let (px, py) = self.last_mouse_pos;
                match key {
                    strata_platform::KeyCode::ArrowDown => {
                        return self.dispatch_scroll(px, py, 0.0, 40.0);
                    }
                    strata_platform::KeyCode::ArrowUp => {
                        return self.dispatch_scroll(px, py, 0.0, -40.0);
                    }
                    strata_platform::KeyCode::PageDown => {
                        return self.dispatch_scroll(px, py, 0.0, 400.0);
                    }
                    strata_platform::KeyCode::PageUp => {
                        return self.dispatch_scroll(px, py, 0.0, -400.0);
                    }
                    strata_platform::KeyCode::Home => {
                        return self.scroll_to_top(px, py);
                    }
                    strata_platform::KeyCode::End => {
                        return self.scroll_to_bottom(px, py);
                    }
                    _ => return false,
                }
            }
            return false;
        }

        match event {
            PlatformEvent::ReceivedCharacter(ch) => {
                if !ch.is_control() {
                    self.reset_caret_blink();
                    let active = self.active_focus.as_mut().unwrap();
                    active.state.insert_text(&ch.to_string());
                    let val = active.state.text.get();
                    if let Some(ref on_input) = active.on_input {
                        on_input(strata_dom::Event::with_value(strata_dom::EventType::Input, val));
                    }
                    true
                } else {
                    false
                }
            }
            PlatformEvent::KeyDown { key, modifiers } => {
                let mut text_changed = false;
                let mut nav_or_sel = false;
                let mut submit = false;
                let mut blur = false;

                if modifiers.ctrl {
                    let cmd = match key {
                        strata_platform::KeyCode::KeyA => Some(EditCommand::SelectAll),
                        strata_platform::KeyCode::KeyC => Some(EditCommand::Copy),
                        strata_platform::KeyCode::KeyX => Some(EditCommand::Cut),
                        strata_platform::KeyCode::KeyV => Some(EditCommand::Paste),
                        strata_platform::KeyCode::KeyZ if modifiers.shift => Some(EditCommand::Redo),
                        strata_platform::KeyCode::KeyZ => Some(EditCommand::Undo),
                        strata_platform::KeyCode::KeyY => Some(EditCommand::Redo),
                        _ => None,
                    };
                    if let Some(cmd) = cmd {
                        return self.apply_edit_command(cmd);
                    }
                }

                let active = self.active_focus.as_mut().unwrap();

                if !text_changed && !nav_or_sel {
                    match key {
                        strata_platform::KeyCode::Backspace => {
                            active.state.delete_backward();
                            text_changed = true;
                        }
                        strata_platform::KeyCode::Delete => {
                            active.state.delete_forward();
                            text_changed = true;
                        }
                        strata_platform::KeyCode::ArrowLeft => {
                            active.state.move_left(modifiers.shift, modifiers.ctrl);
                            nav_or_sel = true;
                        }
                        strata_platform::KeyCode::ArrowRight => {
                            active.state.move_right(modifiers.shift, modifiers.ctrl);
                            nav_or_sel = true;
                        }
                        strata_platform::KeyCode::Home => {
                            active.state.move_home(modifiers.shift);
                            nav_or_sel = true;
                        }
                        strata_platform::KeyCode::End => {
                            active.state.move_end(modifiers.shift);
                            nav_or_sel = true;
                        }
                        strata_platform::KeyCode::Enter => {
                            submit = true;
                        }
                        strata_platform::KeyCode::Escape => {
                            blur = true;
                        }
                        _ => {}
                    }
                }

                if text_changed || nav_or_sel {
                    self.caret_blink_epoch = std::time::Instant::now();
                }

                if text_changed {
                    let val = active.state.text.get();
                    if let Some(ref on_input) = active.on_input {
                        on_input(strata_dom::Event::with_value(strata_dom::EventType::Input, val));
                    }
                    return true;
                }

                if nav_or_sel {
                    return true;
                }

                if submit {
                    let val = active.state.text.get();
                    if let Some(ref on_submit) = active.on_submit {
                        on_submit(strata_dom::Event::with_value(strata_dom::EventType::Submit, val.clone()));
                    }
                    if let Some(ref on_change) = active.on_change {
                        on_change(strata_dom::Event::with_value(strata_dom::EventType::Change, val));
                    }
                    return true;
                }

                if blur {
                    if let Some(ref on_blur) = active.on_blur {
                        on_blur(strata_dom::Event::new(strata_dom::EventType::Blur));
                    }
                    self.active_focus = None;
                    return true;
                }

                false
            }
            _ => false,
        }
    }

    /// Creates a fully populated synthetic W3C DOM Event for mouse and pointer interactions,
    /// with client, page, screen, and offset coordinates, button identifiers, click detail,
    /// and keyboard modifier flags.
    pub fn create_mouse_event(
        &self,
        event_type: strata_dom::EventType,
        px: f32,
        py: f32,
        button: i16,
        detail: i32,
        target_box: Option<&strata_layout::Rect>,
    ) -> strata_dom::Event {
        let mut ev = strata_dom::Event::new(event_type);
        ev.client_x = px as f64;
        ev.client_y = py as f64;
        ev.page_x = px as f64;
        ev.page_y = py as f64;
        ev.screen_x = px as f64;
        ev.screen_y = py as f64;
        ev.button = button;
        ev.buttons = match button {
            0 => 1,
            1 => 4,
            2 => 2,
            _ => 0,
        };
        ev.detail = detail;
        if let Some(tb) = target_box {
            ev.offset_x = (px - tb.x) as f64;
            ev.offset_y = (py - tb.y) as f64;
        } else {
            ev.offset_x = px as f64;
            ev.offset_y = py as f64;
        }
        #[cfg(target_os = "linux")]
        {
            let mods = strata_platform::gtk_current_modifiers();
            ev.shift_key = mods.shift;
            ev.ctrl_key = mods.ctrl;
            ev.alt_key = mods.alt;
            ev.meta_key = mods.meta;
        }
        ev
    }

    /// Dispatches a pointer click event at (x, y) coordinates into the application blueprint tree.
    /// Returns true if an event listener was invoked.
    pub fn dispatch_click(&self, px: f32, py: f32) -> bool {
        self.dispatch_mouse_click(px, py, 0, 1)
    }

    /// Dispatches a mouse click event with specific button (0 = Left, 1 = Middle, 2 = Right) and detail.
    pub fn dispatch_mouse_click(&self, px: f32, py: f32, button: i16, detail: i32) -> bool {
        let ev_type = if button == 0 {
            strata_dom::EventType::Click
        } else {
            strata_dom::EventType::AuxClick
        };

        if let Some((handler, target_box)) = Self::hit_test_event_with_box(&self.root_blueprint, &self.layout_root, px, py, ev_type) {
            let ev = self.create_mouse_event(ev_type, px, py, button, detail, Some(target_box));
            handler(ev);
            true
        } else if button != 0 {
            // Fallback to Click listener if AuxClick had no explicit listener
            if let Some((handler, target_box)) = Self::hit_test_event_with_box(&self.root_blueprint, &self.layout_root, px, py, strata_dom::EventType::Click) {
                let ev = self.create_mouse_event(strata_dom::EventType::Click, px, py, button, detail, Some(target_box));
                handler(ev);
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Dispatches a standard W3C `dblclick` event at (x, y) coordinates to the nearest element with an `ondblclick` listener.
    pub fn dispatch_dblclick(&self, px: f32, py: f32) -> bool {
        if let Some((handler, target_box)) = Self::hit_test_event_with_box(&self.root_blueprint, &self.layout_root, px, py, strata_dom::EventType::DblClick) {
            let ev = self.create_mouse_event(strata_dom::EventType::DblClick, px, py, 0, 2, Some(target_box));
            handler(ev);
            true
        } else {
            false
        }
    }

    /// Dispatches a W3C `contextmenu` event at (x, y) to the nearest element that registered
    /// an `oncontextmenu` listener. Returns false (and leaves the platform default untouched)
    /// when nothing at that point listens, so a context menu never appears on arbitrary surfaces.
    pub fn dispatch_context_menu(&self, px: f32, py: f32) -> bool {
        if let Some((handler, target_box)) = Self::hit_test_event_with_box(
            &self.root_blueprint,
            &self.layout_root,
            px,
            py,
            strata_dom::EventType::ContextMenu,
        ) {
            let ev = self.create_mouse_event(strata_dom::EventType::ContextMenu, px, py, 2, 1, Some(target_box));
            handler(ev);
            true
        } else {
            false
        }
    }

    /// Finds the value of the deepest element property `key` whose border box contains the point.
    /// Elements are tested front-to-back so overlays win over what they cover.
    pub fn hit_test_property(
        blueprint: &NodeBlueprint,
        layout: &LayoutNode,
        px: f32,
        py: f32,
        key: &str,
    ) -> Option<String> {
        let (px, py) = Self::unproject_point(layout, px, py);

        // If element clips overflow, coordinates outside its border box are clipped
        if layout.style.overflow_x != strata_style::types::Overflow::Visible
            || layout.style.overflow_y != strata_style::types::Overflow::Visible
        {
            if !layout.geometry.border_box.contains_point(px, py) {
                return None;
            }
        }
        match blueprint {
            NodeBlueprint::Element { attributes, children, .. } => {
                let has_fragments = children.iter().any(|c| matches!(c, NodeBlueprint::Fragment(_)));
                if !has_fragments {
                    for (child_bp, child_layout) in children.iter().zip(layout.children.iter()).rev() {
                        if let Some(v) = Self::hit_test_property(child_bp, child_layout, px, py, key) {
                            return Some(v);
                        }
                    }
                } else {
                    let mut flat_children = Vec::new();
                    for c in children {
                        Self::flatten_blueprints(c, &mut flat_children);
                    }
                    for (&child_bp, child_layout) in flat_children.iter().zip(layout.children.iter()).rev() {
                        if let Some(v) = Self::hit_test_property(child_bp, child_layout, px, py, key) {
                            return Some(v);
                        }
                    }
                }
                if layout.geometry.border_box.contains_point(px, py) {
                    for attr in attributes {
                        if let strata_dom::Attribute::Property { key: k, value } = attr {
                            if k == key {
                                return Some(value.clone());
                            }
                        }
                    }
                }
                None
            }
            NodeBlueprint::Fragment(children) => {
                for (child_bp, child_layout) in children.iter().zip(layout.children.iter()).rev() {
                    if let Some(v) = Self::hit_test_property(child_bp, child_layout, px, py, key) {
                        return Some(v);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Records a possible window drag when a primary press lands on a `data-app-region="drag"`
    /// surface (the equivalent of `-webkit-app-region: drag`). Interactive children opt out with
    /// `data-app-region="no-drag"`. A quick second press toggles maximize, as native title bars do.
    pub fn begin_window_drag_candidate(&mut self, px: f32, py: f32) -> bool {
        let region = Self::hit_test_property(&self.root_blueprint, &self.layout_root, px, py, "data-app-region");
        if region.as_deref() != Some("drag") {
            self.pending_window_drag = None;
            return false;
        }
        let now = std::time::Instant::now();
        if let Some((t, lx, ly)) = self.last_drag_click.take() {
            if now.duration_since(t).as_millis() < 400 && (px - lx).abs() < 5.0 && (py - ly).abs() < 5.0 {
                // Cancel pending drag candidate on double-click gesture so movement doesn't interfere
                self.pending_window_drag = None;
                return true;
            }
        }
        self.last_drag_click = Some((now, px, py));
        self.pending_window_drag = Some((px, py));
        false
    }

    /// Starts a native window move once the pointer travelled past the drag threshold.
    /// Returns true when the window's maximized state changed (such as unmaximizing on drag).
    pub fn poll_window_drag(&mut self, px: f32, py: f32) -> bool {
        if let Some((sx, sy)) = self.pending_window_drag {
            if (px - sx).abs() > 4.0 || (py - sy).abs() > 4.0 {
                self.pending_window_drag = None;
                self.last_drag_click = None;
                #[cfg(target_os = "linux")]
                {
                    let was_max = strata_platform::gtk_is_maximized();
                    strata_platform::gtk_begin_window_drag(sx / self.viewport_width.max(1.0), sy);
                    let now_max = strata_platform::gtk_is_maximized();
                    return was_max != now_max;
                }
            }
        }
        false
    }

    /// Applies one editing command to the focused text input. Returns true when something changed.
    pub fn apply_edit_command(&mut self, cmd: EditCommand) -> bool {
        let Some(active) = self.active_focus.as_mut() else {
            return false;
        };
        let mut text_changed = false;
        let mut handled = true;
        match cmd {
            EditCommand::SelectAll => active.state.select_all(),
            EditCommand::Copy => {
                let text = active.state.selected_text().unwrap_or_else(|| active.state.text.get());
                #[cfg(target_os = "linux")]
                strata_platform::gtk_set_clipboard_text(&text);
                let _ = text;
            }
            EditCommand::Cut => {
                let text = active.state.selected_text();
                if let Some(sel) = text {
                    #[cfg(target_os = "linux")]
                    strata_platform::gtk_set_clipboard_text(&sel);
                    let _ = sel;
                    active.state.delete_selection();
                } else {
                    #[cfg(target_os = "linux")]
                    strata_platform::gtk_set_clipboard_text(&active.state.text.get());
                    active.state.replace_all_text("");
                }
                text_changed = true;
            }
            EditCommand::Paste => {
                #[cfg(target_os = "linux")]
                if let Some(clip) = strata_platform::gtk_get_clipboard_text() {
                    active.state.insert_text(&clip);
                    text_changed = true;
                }
            }
            EditCommand::Undo => text_changed = active.state.undo(),
            EditCommand::Redo => text_changed = active.state.redo(),
        }
        let _ = &mut handled;
        self.caret_blink_epoch = std::time::Instant::now();
        if text_changed {
            let val = active.state.text.get();
            if let Some(ref on_input) = active.on_input {
                on_input(strata_dom::Event::with_value(strata_dom::EventType::Input, val));
            }
        }
        true
    }

    /// Runs every edit command queued through [`queue_edit_command`] (e.g. from a context menu).
    pub fn drain_edit_commands(&mut self) -> bool {
        let cmds: Vec<EditCommand> = EDIT_COMMAND_QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()));
        let mut changed = false;
        for c in cmds {
            if self.apply_edit_command(c) {
                changed = true;
            }
        }
        changed
    }

    /// Shifts every `data-clamp-viewport` element (popups such as context menus) back inside the
    /// viewport with an 8px margin, using the live viewport size rather than any hardcoded value.
    pub fn apply_viewport_clamps(&mut self) {
        let (vw, vh) = (self.viewport_width, self.viewport_height);
        Self::clamp_walk(&self.root_blueprint, &mut self.layout_root, vw, vh);
    }

    fn clamp_walk(blueprint: &NodeBlueprint, layout: &mut LayoutNode, vw: f32, vh: f32) {
        const MARGIN: f32 = 8.0;
        match blueprint {
            NodeBlueprint::Element { attributes, children, .. } => {
                let clamp = attributes.iter().any(|a| {
                    matches!(a, strata_dom::Attribute::Property { key, .. } if key == "data-clamp-viewport")
                });
                if clamp {
                    let b = layout.geometry.border_box;
                    let mut dx = 0.0;
                    let mut dy = 0.0;
                    if b.x + b.width > vw - MARGIN {
                        dx = vw - MARGIN - b.width - b.x;
                    }
                    if b.x + dx < MARGIN {
                        dx = MARGIN - b.x;
                    }
                    if b.y + b.height > vh - MARGIN {
                        dy = vh - MARGIN - b.height - b.y;
                    }
                    if b.y + dy < MARGIN {
                        dy = MARGIN - b.y;
                    }
                    strata_layout::box_model::translate_node_and_descendants(layout, dx, dy);
                    return;
                }
                let mut flat_children = Vec::new();
                for c in children {
                    Self::flatten_blueprints(c, &mut flat_children);
                }
                for (&child_bp, child_layout) in flat_children.iter().zip(layout.children.iter_mut()) {
                    Self::clamp_walk(child_bp, child_layout, vw, vh);
                }
            }
            NodeBlueprint::Fragment(children) => {
                for (child_bp, child_layout) in children.iter().zip(layout.children.iter_mut()) {
                    Self::clamp_walk(child_bp, child_layout, vw, vh);
                }
            }
            _ => {}
        }
    }

    /// Traverses the LayoutNode hierarchy down a slice of child indices in O(depth).
    pub fn get_layout_node_mut<'a>(root: &'a mut LayoutNode, path: &[usize]) -> Option<&'a mut LayoutNode> {
        let mut current = root;
        for &idx in path {
            if idx < current.children.len() {
                current = &mut current.children[idx];
            } else {
                return None;
            }
        }
        Some(current)
    }

    /// Restores any scrollable containers previously translated by `apply_scroll_offsets` back to their
    /// baseline unshifted coordinates so layout computations and metric measurements are invariant.
    pub fn unapply_scroll_offsets(&mut self) {
        if self.applied_scroll_offsets.is_empty() {
            return;
        }
        for (path, &(applied_x, applied_y)) in &self.applied_scroll_offsets {
            if applied_x.abs() > 0.0001 || applied_y.abs() > 0.0001 {
                if let Some(target) = Self::get_layout_node_mut(&mut self.layout_root, path) {
                    for child in &mut target.children {
                        child.translate(applied_x, applied_y);
                    }
                    for line in &mut target.line_boxes {
                        line.bounds.x += applied_x;
                        line.bounds.y += applied_y;
                        for frag in &mut line.text_fragments {
                            frag.bounds.x += applied_x;
                            frag.bounds.y += applied_y;
                            frag.baseline_y += applied_y;
                        }
                        for atomic in &mut line.atomic_fragments {
                            atomic.bounds.x += applied_x;
                            atomic.bounds.y += applied_y;
                        }
                    }
                }
            }
        }
        self.applied_scroll_offsets.clear();
    }

    /// Discovers scrollable containers, clamps offsets, translates children, and computes scrollbars.
    pub fn apply_scroll_offsets(&mut self) {
        self.active_scrollbars.clear();
        let mut path = Vec::new();
        Self::apply_scroll_walk(
            &self.root_blueprint,
            &mut self.layout_root,
            &mut path,
            &self.scroll_offsets,
            &self.scroll_offsets_by_id,
            &mut self.applied_scroll_offsets,
            &mut self.active_scrollbars,
        );
        self.sync_scroll_animations();
    }

    fn apply_scroll_walk(
        blueprint: &NodeBlueprint,
        layout: &mut LayoutNode,
        current_path: &mut Vec<usize>,
        scroll_offsets: &std::collections::HashMap<Vec<usize>, (f32, f32)>,
        scroll_offsets_by_id: &std::collections::HashMap<String, (f32, f32)>,
        applied_scroll_offsets: &mut std::collections::HashMap<Vec<usize>, (f32, f32)>,
        active_scrollbars: &mut Vec<ActiveScrollbar>,
    ) {
        if matches!(layout.style.display, strata_style::types::Display::None) {
            return;
        }

        let is_scroll_y = matches!(
            layout.style.overflow_y,
            strata_style::types::Overflow::Auto | strata_style::types::Overflow::Scroll
        );
        let is_scroll_x = matches!(
            layout.style.overflow_x,
            strata_style::types::Overflow::Auto | strata_style::types::Overflow::Scroll
        );

        let elem_id = match blueprint {
            NodeBlueprint::Element { attributes, .. } => {
                attributes.iter().find_map(|a| {
                    if let strata_dom::Attribute::Property { key, value } = a {
                        if key == "id" {
                            Some(value.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
            }
            _ => None,
        };

        if is_scroll_y || is_scroll_x {
            let viewport_w = layout.geometry.content_box.width;
            let viewport_h = layout.geometry.content_box.height;

            let mut max_child_x = layout.geometry.content_box.x;
            let mut max_child_y = layout.geometry.content_box.y;

            for child in &layout.children {
                if child.style.position == strata_style::types::Position::Absolute
                    || child.style.position == strata_style::types::Position::Fixed
                {
                    continue;
                }
                max_child_x = max_child_x.max(child.geometry.border_box.x + child.geometry.border_box.width);
                max_child_y = max_child_y.max(child.geometry.border_box.y + child.geometry.border_box.height);
            }
            for line in &layout.line_boxes {
                max_child_x = max_child_x.max(line.bounds.x + line.bounds.width);
                max_child_y = max_child_y.max(line.bounds.y + line.bounds.height);
            }

            let content_w = (max_child_x - layout.geometry.content_box.x).max(0.0);
            let content_h = (max_child_y - layout.geometry.content_box.y).max(0.0);
            let max_scroll_x = (content_w - viewport_w).max(0.0);
            let max_scroll_y = (content_h - viewport_h).max(0.0);

            let (stored_x, stored_y) = if let Some(ref id) = elem_id {
                scroll_offsets_by_id
                    .get(id)
                    .copied()
                    .or_else(|| scroll_offsets.get(current_path).copied())
                    .unwrap_or((0.0, 0.0))
            } else {
                scroll_offsets.get(current_path).copied().unwrap_or((0.0, 0.0))
            };

            let clamped_x = stored_x.clamp(0.0, max_scroll_x);
            let clamped_y = stored_y.clamp(0.0, max_scroll_y);

            if clamped_x > 0.0 || clamped_y > 0.0 {
                for child in &mut layout.children {
                    child.translate(-clamped_x, -clamped_y);
                }
                for line in &mut layout.line_boxes {
                    line.bounds.x -= clamped_x;
                    line.bounds.y -= clamped_y;
                    for frag in &mut line.text_fragments {
                        frag.bounds.x -= clamped_x;
                        frag.bounds.y -= clamped_y;
                        frag.baseline_y -= clamped_y;
                    }
                    for atomic in &mut line.atomic_fragments {
                        atomic.bounds.x -= clamped_x;
                        atomic.bounds.y -= clamped_y;
                    }
                }
                applied_scroll_offsets.insert(current_path.clone(), (clamped_x, clamped_y));
            }

            if is_scroll_y && max_scroll_y > 1.0 && viewport_h > 20.0 {
                let border_box = layout.geometry.border_box;
                let track_x = border_box.x + border_box.width - 8.0;
                let track_y = border_box.y + 2.0;
                let track_h = (border_box.height - 4.0).max(10.0);
                let thumb_h = ((viewport_h / content_h) * track_h).clamp(24.0, track_h);
                let available_travel = (track_h - thumb_h).max(0.0);
                let thumb_y = if max_scroll_y > 0.0 && available_travel > 0.0 {
                    track_y + (clamped_y / max_scroll_y) * available_travel
                } else {
                    track_y
                };

                let thumb_bounds = strata_layout::Rect::new(track_x, thumb_y, 6.0, thumb_h);
                active_scrollbars.push(ActiveScrollbar {
                    bounds: border_box,
                    thumb_bounds,
                    scroll_y: clamped_y,
                    max_scroll_y,
                    content_h,
                    is_id: elem_id.is_some(),
                    id: elem_id.clone().unwrap_or_default(),
                    path: current_path.clone(),
                    available_travel,
                });
            }
        }

        // Recursively walk children
        match blueprint {
            NodeBlueprint::Element { children, .. } => {
                let mut flat_children = Vec::new();
                for c in children {
                    Self::flatten_blueprints(c, &mut flat_children);
                }
                for (i, (&child_bp, child_layout)) in flat_children.iter().zip(layout.children.iter_mut()).enumerate() {
                    current_path.push(i);
                    Self::apply_scroll_walk(
                        child_bp,
                        child_layout,
                        current_path,
                        scroll_offsets,
                        scroll_offsets_by_id,
                        applied_scroll_offsets,
                        active_scrollbars,
                    );
                    current_path.pop();
                }
            }
            NodeBlueprint::Fragment(children) => {
                let mut flat_children = Vec::new();
                for c in children {
                    Self::flatten_blueprints(c, &mut flat_children);
                }
                for (i, (&child_bp, child_layout)) in flat_children.iter().zip(layout.children.iter_mut()).enumerate() {
                    current_path.push(i);
                    Self::apply_scroll_walk(
                        child_bp,
                        child_layout,
                        current_path,
                        scroll_offsets,
                        scroll_offsets_by_id,
                        applied_scroll_offsets,
                        active_scrollbars,
                    );
                    current_path.pop();
                }
            }
            _ => {}
        }
    }

    /// Helper translating children and line fragments in an overflowing container by `-dy` and recording offsets.
    fn apply_scroll_delta_to_container(
        &mut self,
        path: &[usize],
        is_id: bool,
        id: &str,
        new_scroll_y: f32,
        dy: f32,
        max_scroll_y: f32,
        available_travel: f32,
        bounds_y: f32,
    ) {
        if is_id {
            self.scroll_offsets_by_id.insert(id.to_string(), (0.0, new_scroll_y));
        }
        self.scroll_offsets.insert(path.to_vec(), (0.0, new_scroll_y));
        self.applied_scroll_offsets.insert(path.to_vec(), (0.0, new_scroll_y));

        if let Some(b) = self.active_scrollbars.iter_mut().find(|b| b.path == path) {
            b.scroll_y = new_scroll_y;
            if max_scroll_y > 0.0 && available_travel > 0.0 {
                b.thumb_bounds.y = bounds_y + 2.0 + (new_scroll_y / max_scroll_y) * available_travel;
            }
        }

        if let Some(target) = Self::get_layout_node_mut(&mut self.layout_root, path) {
            for child in &mut target.children {
                child.translate(0.0, -dy);
            }
            for line in &mut target.line_boxes {
                line.bounds.y -= dy;
                for frag in &mut line.text_fragments {
                    frag.bounds.y -= dy;
                    frag.baseline_y -= dy;
                }
                for atomic in &mut line.atomic_fragments {
                    atomic.bounds.y -= dy;
                }
            }
        }
    }

    /// Renders sleek semi-transparent scrollbar thumbs for all overflowing scroll containers.
    pub fn render_scrollbars(&mut self) {
        let (mx, my) = self.last_mouse_pos;
        let is_dragging = self.scrollbar_drag.is_some();
        let is_kinetic = self.active_smooth_scroll.is_some();
        let is_recent_scroll = self.last_scroll_interaction
            .map_or(false, |t| t.elapsed().as_millis() < 700);

        for bar in &self.active_scrollbars {
            if bar.max_scroll_y <= 1.0 {
                continue;
            }
            let is_hovered = bar.thumb_bounds.contains_point(mx, my);
            let is_active = is_dragging || is_hovered || is_kinetic || is_recent_scroll;
            let alpha = if is_active {
                140u8
            } else {
                56u8
            };

            let mut thumb_bounds = bar.thumb_bounds;
            if is_active {
                let expand_px = 2.0;
                thumb_bounds.x = (thumb_bounds.x - expand_px).max(bar.bounds.x);
                thumb_bounds.width += expand_px;
            }

            self.display_list.push(strata_render::compositor::DrawCommand::DrawRoundedRect {
                bounds: thumb_bounds,
                color: strata_style::types::Color::rgba(255, 255, 255, alpha),
                border_radius: strata_style::types::BorderRadius::all(3.5),
                border_width: strata_style::types::EdgeInsets::all(0.0),
                border_color: None,
            });
        }
    }

    /// Dispatches an immediate synchronous scroll wheel delta to the innermost scrollable container under (px, py).
    /// Cancels any active kinetic smooth scroll animation.
    /// Returns true if a scroll offset changed and redraw is required.
    pub fn dispatch_scroll(&mut self, px: f32, py: f32, _delta_x: f32, delta_y: f32) -> bool {
        self.last_mouse_pos = (px, py);
        self.active_smooth_scroll = None;

        let mut matching_bars: Vec<ActiveScrollbar> = self.active_scrollbars
            .iter()
            .filter(|bar| bar.bounds.contains_point(px, py))
            .cloned()
            .collect();

        // If mouse is not directly over an active scroll container, fallback to the largest scrollable container
        if matching_bars.is_empty() {
            if let Some(largest) = self.active_scrollbars.iter().max_by(|a, b| {
                (a.bounds.width * a.bounds.height)
                    .partial_cmp(&(b.bounds.width * b.bounds.height))
                    .unwrap_or(std::cmp::Ordering::Equal)
            }) {
                matching_bars.push(largest.clone());
            }
        }

        for bar in matching_bars.into_iter().rev() {
            if bar.max_scroll_y <= 0.0 {
                continue;
            }

            let cur_y = if bar.is_id {
                self.scroll_offsets_by_id
                    .get(&bar.id)
                    .copied()
                    .or_else(|| self.scroll_offsets.get(&bar.path).copied())
                    .map(|(_, y)| y)
                    .unwrap_or(bar.scroll_y)
            } else {
                self.scroll_offsets.get(&bar.path).copied().map(|(_, y)| y).unwrap_or(bar.scroll_y)
            };

            let candidate_y = (cur_y + delta_y).clamp(0.0, bar.max_scroll_y);
            let dy = candidate_y - cur_y;
            if dy.abs() > 0.01 {
                self.last_scroll_interaction = Some(std::time::Instant::now());
                self.apply_scroll_delta_to_container(
                    &bar.path,
                    bar.is_id,
                    &bar.id,
                    candidate_y,
                    dy,
                    bar.max_scroll_y,
                    bar.available_travel,
                    bar.bounds.y,
                );
                return true;
            }
        }

        false
    }

    /// Dispatches a smooth kinetic scroll impulse to the innermost scrollable container under (px, py).
    /// Accumulates velocity for rapid consecutive wheel events and initiates a critically damped
    /// kinetic transition curve with Chromium Blink parity.
    pub fn dispatch_smooth_scroll(&mut self, px: f32, py: f32, _delta_x: f32, delta_y: f32) -> bool {
        self.last_mouse_pos = (px, py);
        if delta_y.abs() < 0.01 {
            return false;
        }

        let mut matching_bars: Vec<ActiveScrollbar> = self.active_scrollbars
            .iter()
            .filter(|bar| bar.bounds.contains_point(px, py))
            .cloned()
            .collect();

        if matching_bars.is_empty() {
            if let Some(largest) = self.active_scrollbars.iter().max_by(|a, b| {
                (a.bounds.width * a.bounds.height)
                    .partial_cmp(&(b.bounds.width * b.bounds.height))
                    .unwrap_or(std::cmp::Ordering::Equal)
            }) {
                matching_bars.push(largest.clone());
            }
        }

        let Some(bar) = matching_bars.into_iter().rev().find(|b| b.max_scroll_y > 0.0) else {
            return false;
        };

        let now = std::time::Instant::now();
        self.last_scroll_interaction = Some(now);

        let cur_y = if bar.is_id {
            self.scroll_offsets_by_id
                .get(&bar.id)
                .copied()
                .or_else(|| self.scroll_offsets.get(&bar.path).copied())
                .map(|(_, y)| y)
                .unwrap_or(bar.scroll_y)
        } else {
            self.scroll_offsets.get(&bar.path).copied().map(|(_, y)| y).unwrap_or(bar.scroll_y)
        };

        if let Some(ref mut tracker) = self.active_smooth_scroll {
            if tracker.container_path == bar.path {
                let dt_flick = now.duration_since(tracker.last_event_time).as_secs_f32();
                let accel = if dt_flick < 0.20 {
                    (1.0 + (0.12 / dt_flick.max(0.01)).min(2.5)).min(3.5)
                } else {
                    1.0
                };

                let t_prev = now.duration_since(tracker.start_time).as_secs_f32();
                let omega = tracker.omega;
                let e0 = tracker.start_y - tracker.target_y;
                let c2 = tracker.initial_velocity + omega * e0;
                let decay = (-omega * t_prev).exp();
                let y_now = (tracker.target_y + (e0 + c2 * t_prev) * decay).clamp(0.0, bar.max_scroll_y);
                let v_now = (tracker.initial_velocity - omega * c2 * t_prev) * decay;

                let impulse = delta_y * accel;
                let new_target = (tracker.target_y + impulse).clamp(0.0, bar.max_scroll_y);

                tracker.start_y = y_now;
                tracker.current_y = y_now;
                tracker.initial_velocity = v_now;
                tracker.target_y = new_target;
                tracker.start_time = now;
                tracker.last_event_time = now;
                tracker.max_scroll_y = bar.max_scroll_y;
                tracker.available_travel = bar.available_travel;
                tracker.bounds_y = bar.bounds.y;
                return true;
            }
        }

        let new_target = (cur_y + delta_y).clamp(0.0, bar.max_scroll_y);
        self.active_smooth_scroll = Some(SmoothScrollTracker {
            container_path: bar.path,
            container_id: bar.id,
            is_id: bar.is_id,
            max_scroll_y: bar.max_scroll_y,
            available_travel: bar.available_travel,
            bounds_y: bar.bounds.y,
            current_y: cur_y,
            target_y: new_target,
            start_y: cur_y,
            initial_velocity: 0.0,
            start_time: now,
            last_event_time: now,
            omega: 18.0,
        });

        true
    }

    /// Polls and updates active kinetic smooth scroll animation at a specific timestamp.
    /// Returns true if a position changed and a redraw is required.
    pub fn poll_smooth_scroll_at(&mut self, now: std::time::Instant) -> bool {
        let Some(mut tracker) = self.active_smooth_scroll.take() else {
            return false;
        };

        let t = now.duration_since(tracker.start_time).as_secs_f32();
        let omega = tracker.omega;
        let e0 = tracker.start_y - tracker.target_y;
        let c2 = tracker.initial_velocity + omega * e0;
        let decay = (-omega * t).exp();

        let raw_y = tracker.target_y + (e0 + c2 * t) * decay;
        let clamped_y = raw_y.clamp(0.0, tracker.max_scroll_y);
        let vel = (tracker.initial_velocity - omega * c2 * t) * decay;

        let is_settled = t >= 0.40 || ((clamped_y - tracker.target_y).abs() < 0.25 && vel.abs() < 1.0);
        let new_y = if is_settled {
            tracker.target_y
        } else {
            clamped_y
        };

        let dy = new_y - tracker.current_y;
        let mut redraw = false;

        if dy.abs() > 0.001 {
            self.apply_scroll_delta_to_container(
                &tracker.container_path,
                tracker.is_id,
                &tracker.container_id,
                new_y,
                dy,
                tracker.max_scroll_y,
                tracker.available_travel,
                tracker.bounds_y,
            );
            tracker.current_y = new_y;
            redraw = true;
        }

        if !is_settled {
            self.active_smooth_scroll = Some(tracker);
            redraw = true;
        }

        redraw
    }

    /// Evaluates active smooth scroll animation at the current instant.
    #[inline]
    pub fn poll_smooth_scroll(&mut self) -> bool {
        self.poll_smooth_scroll_at(std::time::Instant::now())
    }

    /// Checks if a pointer press landed on any active scrollbar thumb and starts dragging.
    pub fn check_scrollbar_thumb_press(&mut self, px: f32, py: f32) -> bool {
        for bar in &self.active_scrollbars {
            if bar.max_scroll_y > 1.0 && bar.thumb_bounds.contains_point(px, py) {
                self.active_smooth_scroll = None;
                self.last_scroll_interaction = Some(std::time::Instant::now());
                self.scrollbar_drag = Some(ScrollbarDrag {
                    is_id: bar.is_id,
                    id: bar.id.clone(),
                    path: bar.path.clone(),
                    start_pointer_y: py,
                    start_scroll_y: bar.scroll_y,
                    available_travel: bar.available_travel,
                    max_scroll_y: bar.max_scroll_y,
                });
                return true;
            }
        }
        false
    }

    /// Updates active scrollbar thumb drag during pointer movement.
    pub fn poll_scrollbar_drag(&mut self, _px: f32, py: f32) -> bool {
        let Some(drag) = self.scrollbar_drag.clone() else {
            return false;
        };
        if drag.available_travel <= 0.0 {
            return false;
        }

        let dy_pointer = py - drag.start_pointer_y;
        let delta_scroll = (dy_pointer / drag.available_travel) * drag.max_scroll_y;
        let new_scroll_y = (drag.start_scroll_y + delta_scroll).clamp(0.0, drag.max_scroll_y);

        if let Some(bar) = self.active_scrollbars.iter().find(|b| b.path == drag.path) {
            let cur_y = bar.scroll_y;
            let dy = new_scroll_y - cur_y;
            if dy.abs() > 0.01 {
                self.last_scroll_interaction = Some(std::time::Instant::now());
                self.apply_scroll_delta_to_container(
                    &drag.path,
                    drag.is_id,
                    &drag.id,
                    new_scroll_y,
                    dy,
                    drag.max_scroll_y,
                    drag.available_travel,
                    bar.bounds.y,
                );
                return true;
            }
        }

        false
    }

    /// Scrolls the innermost scroll container under (px, py) to the very top.
    pub fn scroll_to_top(&mut self, px: f32, py: f32) -> bool {
        self.active_smooth_scroll = None;
        let mut matching_bars: Vec<ActiveScrollbar> = self.active_scrollbars
            .iter()
            .filter(|bar| bar.bounds.contains_point(px, py))
            .cloned()
            .collect();

        if matching_bars.is_empty() {
            if let Some(largest) = self.active_scrollbars.iter().max_by(|a, b| {
                (a.bounds.width * a.bounds.height)
                    .partial_cmp(&(b.bounds.width * b.bounds.height))
                    .unwrap_or(std::cmp::Ordering::Equal)
            }) {
                matching_bars.push(largest.clone());
            }
        }

        if let Some(bar) = matching_bars.into_iter().rev().next() {
            let cur_y = bar.scroll_y;
            let dy = 0.0 - cur_y;
            if dy.abs() > 0.01 {
                self.last_scroll_interaction = Some(std::time::Instant::now());
                self.apply_scroll_delta_to_container(
                    &bar.path,
                    bar.is_id,
                    &bar.id,
                    0.0,
                    dy,
                    bar.max_scroll_y,
                    bar.available_travel,
                    bar.bounds.y,
                );
                return true;
            }
        }
        false
    }

    /// Scrolls the innermost scroll container under (px, py) to the very bottom.
    pub fn scroll_to_bottom(&mut self, px: f32, py: f32) -> bool {
        self.active_smooth_scroll = None;
        let mut matching_bars: Vec<ActiveScrollbar> = self.active_scrollbars
            .iter()
            .filter(|bar| bar.bounds.contains_point(px, py))
            .cloned()
            .collect();

        if matching_bars.is_empty() {
            if let Some(largest) = self.active_scrollbars.iter().max_by(|a, b| {
                (a.bounds.width * a.bounds.height)
                    .partial_cmp(&(b.bounds.width * b.bounds.height))
                    .unwrap_or(std::cmp::Ordering::Equal)
            }) {
                matching_bars.push(largest.clone());
            }
        }

        if let Some(bar) = matching_bars.into_iter().rev().next() {
            if bar.max_scroll_y > 0.0 {
                let cur_y = bar.scroll_y;
                let dy = bar.max_scroll_y - cur_y;
                if dy.abs() > 0.01 {
                    self.last_scroll_interaction = Some(std::time::Instant::now());
                    self.apply_scroll_delta_to_container(
                        &bar.path,
                        bar.is_id,
                        &bar.id,
                        bar.max_scroll_y,
                        dy,
                        bar.max_scroll_y,
                        bar.available_travel,
                        bar.bounds.y,
                    );
                    return true;
                }
            }
        }
        false
    }

    /// Generates draw commands for all computed layout rectangles using the 7-layer CSS compositor with spatial viewport scissoring.
    pub fn compile_display_list(&mut self) {
        let viewport = if self.viewport_width > 0.0 && self.viewport_height > 0.0 {
            Some(strata_layout::Rect::new(0.0, 0.0, self.viewport_width, self.viewport_height))
        } else {
            None
        };
        self.display_list = strata_render::compositor::Compositor::build_display_list_with_viewport(&self.layout_root, viewport);
        self.render_scrollbars();
        self.render_active_input_caret();
    }

    /// Renders the visible insertion caret cursor and selection highlight for the currently focused text input.
    pub fn render_active_input_caret(&mut self) {
        let Some(ref active) = self.active_focus else {
            return;
        };

        let mut current_idx = 0;
        let target_idx = active.input_index;
        let target_id = active.input_id.as_deref();

        let Some(layout) = Self::find_input_layout(
            &self.root_blueprint,
            &self.layout_root,
            target_idx,
            target_id,
            &mut current_idx,
        ) else {
            return;
        };

        let content_box = layout.geometry.content_box;
        if content_box.width <= 0.0 || content_box.height <= 0.0 {
            return;
        }

        let font_size = layout.style.font_size;
        let line_height = layout.style.line_height;
        let is_mono = strata_layout::is_monospace_family(layout.style.font_family);
        let text_color = layout.style.color;
        let caret_color = layout.style.caret_color.unwrap_or(text_color);
        let text = active.state.text.get();
        let total_chars = text.chars().count();
        let cursor_idx = active.state.cursor_index.get().min(total_chars);

        // Alignment offset for text-align: center / right
        let align_offset = match layout.style.text_align {
            strata_style::types::TextAlign::Center => {
                let (tw, _) = strata_layout::text::measure_text_bounds_ext(&text, font_size, line_height, None, is_mono);
                ((content_box.width - tw) / 2.0).max(0.0)
            }
            strata_style::types::TextAlign::Right => {
                let (tw, _) = strata_layout::text::measure_text_bounds_ext(&text, font_size, line_height, None, is_mono);
                (content_box.width - tw).max(0.0)
            }
            _ => 0.0,
        };

        let caret_w = 2.0f32;
        let caret_h = (font_size * 1.2).min(content_box.height);
        let caret_y = content_box.y + ((content_box.height - caret_h) / 2.0).max(0.0);

        // 1. Selection range highlight (CSS ::selection)
        if let Some((start, end)) = active.state.selection_range() {
            let start_clamped = start.min(total_chars);
            let end_clamped = end.min(total_chars);
            if start_clamped < end_clamped {
                let start_prefix: String = text.chars().take(start_clamped).collect();
                let (start_w, _) = strata_layout::text::measure_text_bounds_ext(&start_prefix, font_size, line_height, None, is_mono);
                let sel_slice: String = text.chars().skip(start_clamped).take(end_clamped - start_clamped).collect();
                let (sel_w, _) = strata_layout::text::measure_text_bounds_ext(&sel_slice, font_size, line_height, None, is_mono);

                let sel_h = (font_size * 1.25).min(content_box.height);
                let sel_y = content_box.y + ((content_box.height - sel_h) / 2.0).max(0.0);
                let sel_x = content_box.x + align_offset + start_w;

                self.display_list.push(strata_render::compositor::DrawCommand::DrawRect {
                    bounds: strata_layout::Rect::new(sel_x, sel_y, sel_w, sel_h),
                    color: strata_style::types::Color::rgba(59, 130, 246, 120), // Standard CSS selection blue
                });
            }
        }

        // 2. Visible Caret Line Cursor (CSS caret-color, blinking, and insertion cursor)
        let has_sel = active.state.has_selection();
        let elapsed_ms = self.caret_blink_epoch.elapsed().as_millis();
        let is_blink_on = (elapsed_ms / 530) % 2 == 0;

        if !has_sel && is_blink_on {
            let prefix: String = text.chars().take(cursor_idx).collect();
            let (cursor_w, _) = strata_layout::text::measure_text_bounds_ext(&prefix, font_size, line_height, None, is_mono);
            let caret_x = (content_box.x + align_offset + cursor_w).min(content_box.x + content_box.width - caret_w);

            self.display_list.push(strata_render::compositor::DrawCommand::DrawRect {
                bounds: strata_layout::Rect::new(caret_x, caret_y, caret_w, caret_h),
                color: caret_color,
            });
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
            PlatformEvent::Scroll { delta_x, delta_y, .. } => {
                self.dispatch_smooth_scroll(self.last_mouse_pos.0, self.last_mouse_pos.1, *delta_x, *delta_y)
            }
            PlatformEvent::AnimationTick => {
                let scroll_changed = self.poll_smooth_scroll();
                let anim_changed = self.poll_animations();
                scroll_changed || anim_changed
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
                strata_platform::PlatformEvent::MouseMove { x, y } => {
                    if runtime.update_hover(x, y) {
                        needs_rebuild = true;
                    }
                }
                strata_platform::PlatformEvent::CursorLeft => {
                    if runtime.clear_hover() {
                        needs_rebuild = true;
                    }
                }
                _ => {}
            }
        }

        let anim_changed = runtime.poll_animations();
        let scroll_changed = runtime.poll_smooth_scroll();
        if anim_changed || scroll_changed {
            needs_redraw = true;
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

#[cfg(target_os = "linux")]
/// Launches an interactive native GTK3/Cairo desktop window on Linux with high-fidelity system typography.
pub fn launch_gtk<F>(root_fn: F, config: WindowConfig) -> Result<(), String>
where
    F: FnMut() -> View + 'static,
{
    launch_gtk_with_events(root_fn, config, |_, _| false)
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
        let size_changed = (rt.viewport_width - width).abs() > 0.01 || (rt.viewport_height - height).abs() > 0.01;
        rt.viewport_width = width;
        rt.viewport_height = height;
        if size_changed {
            rt.unapply_scroll_offsets();
            rt.layout_root.compute_layout(width, height);
            rt.apply_viewport_clamps();
            rt.apply_scroll_offsets();
        }
        rt.compile_display_list();

        // Fill background
        cr.set_source_rgb(0.035, 0.035, 0.043);
        let _ = cr.paint();

        strata_platform::render_display_list_to_cairo(cr, &rt.display_list);
    };

    let runtime_event = runtime.clone();
    let root_fn_event = root_fn_rc.clone();
    let event_handler_event = event_handler_rc.clone();
    let on_event = move |event: &strata_platform::PlatformEvent| -> bool {
        let mut needs_rebuild = false;
        let mut needs_redraw = false;
        {
            let mut rt = runtime_event.borrow_mut();
            if (event_handler_event.borrow_mut())(event, &mut rt) {
                needs_rebuild = true;
            }
            if let strata_platform::PlatformEvent::Scroll { delta_x, delta_y, .. } = event {
                let (px, py) = rt.last_mouse_pos;
                if rt.dispatch_smooth_scroll(px, py, *delta_x, *delta_y) {
                    needs_redraw = true;
                }
            }
            if let strata_platform::PlatformEvent::AnimationTick = event {
                let scroll_changed = rt.poll_smooth_scroll();
                let anim_changed = rt.poll_animations();
                if scroll_changed || anim_changed {
                    needs_redraw = true;
                }
            }
            if let strata_platform::PlatformEvent::DoubleClick { button, x, y } = event {
                let _btn_idx = button.to_button_index();
                if *button == strata_platform::MouseButton::Left {
                    let region = AppRuntime::hit_test_property(&rt.root_blueprint, &rt.layout_root, *x, *y, "data-app-region");
                    if region.as_deref() == Some("drag") {
                        #[cfg(target_os = "linux")]
                        {
                            let new_max = strata_platform::gtk_toggle_maximize();
                            let _ = (event_handler_event.borrow_mut())(&strata_platform::PlatformEvent::WindowMaximized(new_max), &mut rt);
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
                        if rt.dispatch_pointer_down(*x, *y) {
                            needs_rebuild = true;
                        }
                        if rt.dispatch_mouse_click(*x, *y, btn_idx, 1) {
                            needs_rebuild = true;
                        }
                        if rt.drain_edit_commands() {
                            needs_rebuild = true;
                        }
                    }
                } else if *button == strata_platform::MouseButton::Middle {
                    if rt.dispatch_pointer_down(*x, *y) {
                        needs_rebuild = true;
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
                if rt.dispatch_pointer_up(*x, *y) {
                    needs_rebuild = true;
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
                            let _ = (event_handler_event.borrow_mut())(&strata_platform::PlatformEvent::WindowMaximized(is_max), &mut rt);
                        }
                        needs_rebuild = true;
                    }
                    if rt.dispatch_pointer_move(*x, *y) {
                        needs_rebuild = true;
                    }
                    if rt.update_hover(*x, *y) {
                        needs_rebuild = true;
                    }
                    let cur = AppRuntime::hit_test_cursor(&rt.root_blueprint, &rt.layout_root, *x, *y);
                    strata_platform::gtk_set_cursor(cur);
                }
            }
            if matches!(event, strata_platform::PlatformEvent::CursorLeft) {
                if rt.clear_hover() {
                    needs_rebuild = true;
                }
            }
            if matches!(event, strata_platform::PlatformEvent::KeyDown { .. } | strata_platform::PlatformEvent::KeyUp { .. } | strata_platform::PlatformEvent::ReceivedCharacter(_)) {
                if rt.handle_key_event(event) {
                    needs_rebuild = true;
                }
            }
        }

        if needs_rebuild {
            #[cfg(target_os = "linux")]
            if strata_platform::gtk_is_closing() {
                return false;
            }
            let new_view = (root_fn_event.borrow_mut())();
            runtime_event.borrow_mut().update_view(new_view);
            true
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
    strata_platform::launch_gtk_window(&config.title, config.width, config.height, decorated, on_draw, on_event)
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
