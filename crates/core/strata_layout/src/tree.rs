//! Layout tree node definition and top-level formatting context dispatcher.

use crate::geometry::LayoutGeometry;
use std::cell::{Cell, RefCell};
use strata_style::types::{ComputedStyle, Display};

/// SVG stroke line cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StrokeLineCap {
    #[default]
    Butt,
    Round,
    Square,
}

/// SVG stroke line join.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StrokeLineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

/// SVG fill rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

/// An individual SVG vector item within a layout node.
#[derive(Debug, Clone, PartialEq)]
pub struct SvgItem {
    pub path_data: String,
    pub stroke_color: Option<strata_style::types::Color>,
    pub fill_color: Option<strata_style::types::Color>,
    pub stroke_is_current_color: bool,
    pub fill_is_current_color: bool,
    pub stroke_width: f32,
    pub stroke_linecap: StrokeLineCap,
    pub stroke_linejoin: StrokeLineJoin,
    pub stroke_dasharray: Vec<f32>,
    pub fill_rule: FillRule,
    pub view_box: (f32, f32),
}

impl SvgItem {
    pub fn new(path_data: String, view_box: (f32, f32)) -> Self {
        Self {
            path_data,
            stroke_color: None,
            fill_color: None,
            stroke_is_current_color: false,
            fill_is_current_color: false,
            stroke_width: 1.0,
            stroke_linecap: StrokeLineCap::Butt,
            stroke_linejoin: StrokeLineJoin::Miter,
            stroke_dasharray: Vec::new(),
            fill_rule: FillRule::NonZero,
            view_box,
        }
    }
}

use crate::typography::{InlineItem, LineBox};

/// An encoded raster image carried on a layout node.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageResource {
    pub data: std::sync::Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
}

/// Compact layout result cache for O(1) subtree re-entry during multi-pass formatting.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LayoutCacheEntry {
    pub available_w: f32,
    pub available_h: f32,
    pub result_w: f32,
    pub result_h: f32,
    pub origin_x: f32,
    pub origin_y: f32,
    pub is_valid: bool,
}

impl LayoutCacheEntry {
    #[inline]
    pub const fn invalid() -> Self {
        Self {
            available_w: 0.0,
            available_h: 0.0,
            result_w: 0.0,
            result_h: 0.0,
            origin_x: 0.0,
            origin_y: 0.0,
            is_valid: false,
        }
    }
}

/// Maximum allowable call-stack recursion depth for layout tree formatting.
/// Bounded to 512 frames to guarantee that total call-stack consumption stays
/// safely bounded within default OS and test-runner stack budgets (2 MB) across all
/// platforms and unoptimized debug builds while supporting deep enterprise DOM trees.
pub const MAX_LAYOUT_DEPTH: usize = 512;

/// Lightweight Copy-context passed through layout formatting contexts to track recursion budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutDepthGuard {
    depth: usize,
    max_depth: usize,
}

impl LayoutDepthGuard {
    /// Constructs a root guard with default MAX_LAYOUT_DEPTH.
    #[inline]
    pub const fn root() -> Self {
        Self {
            depth: 0,
            max_depth: MAX_LAYOUT_DEPTH,
        }
    }

    /// Constructs a guard with a custom maximum depth limit (e.g. for testing).
    #[inline]
    pub const fn with_max_depth(max_depth: usize) -> Self {
        Self {
            depth: 0,
            max_depth,
        }
    }

    /// Advances the guard into a child node frame. Returns `None` if the depth budget is exhausted.
    #[inline]
    pub fn enter(&self) -> Option<Self> {
        if self.depth >= self.max_depth {
            None
        } else {
            Some(Self {
                depth: self.depth + 1,
                max_depth: self.max_depth,
            })
        }
    }

    #[inline]
    pub fn current_depth(&self) -> usize {
        self.depth
    }

    #[inline]
    pub fn max_depth(&self) -> usize {
        self.max_depth
    }

    #[inline]
    pub fn is_exhausted(&self) -> bool {
        self.depth >= self.max_depth
    }
}

type LayoutOverflowCallback = Box<dyn Fn(usize, usize) + Send + Sync>;

thread_local! {
    static LAYOUT_OVERFLOW_CALLBACK: RefCell<Option<LayoutOverflowCallback>> = const { RefCell::new(None) };
    static LAST_LAYOUT_OVERFLOW: Cell<Option<(usize, usize)>> = const { Cell::new(None) };
}

/// Registers an event callback triggered when layout depth exceeds MAX_LAYOUT_DEPTH.
pub fn set_on_layout_overflow<F: Fn(usize, usize) + Send + Sync + 'static>(cb: Option<F>) {
    LAYOUT_OVERFLOW_CALLBACK.with(|c| {
        *c.borrow_mut() = cb.map(|f| Box::new(f) as LayoutOverflowCallback);
    });
}

/// Returns the last recorded layout depth overflow as (depth, max_depth), if any.
pub fn last_layout_overflow() -> Option<(usize, usize)> {
    LAST_LAYOUT_OVERFLOW.with(|c| c.get())
}

/// Clears any recorded layout depth overflow.
pub fn clear_layout_overflow() {
    LAST_LAYOUT_OVERFLOW.with(|c| c.set(None));
}

/// Records a layout depth overflow occurrence, updates diagnostics, and fires the registered callback.
pub fn record_layout_overflow(depth: usize, max_depth: usize) {
    LAST_LAYOUT_OVERFLOW.with(|c| c.set(Some((depth, max_depth))));
    LAYOUT_OVERFLOW_CALLBACK.with(|cb| {
        if let Some(ref f) = *cb.borrow() {
            f(depth, max_depth);
        }
    });
}

/// Placement of an anchored top-layer portal or floating element relative to its anchor box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AnchorPlacement {
    Bottom,
    BottomStart,
    #[default]
    BottomEnd,
    Top,
    TopStart,
    TopEnd,
    Left,
    LeftStart,
    LeftEnd,
    Right,
    RightStart,
    RightEnd,
}

impl AnchorPlacement {
    /// Parses a CSS/Floating-UI placement string such as `"bottom-end"`, `"top-start"`, `"bottom"`, etc.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "bottom" | "block-end" => Some(Self::Bottom),
            "bottom-start" | "bottom_start" | "bottom-left" => Some(Self::BottomStart),
            "bottom-end" | "bottom_end" | "bottom-right" => Some(Self::BottomEnd),
            "top" | "block-start" => Some(Self::Top),
            "top-start" | "top_start" | "top-left" => Some(Self::TopStart),
            "top-end" | "top_end" | "top-right" => Some(Self::TopEnd),
            "left" | "inline-start" => Some(Self::Left),
            "left-start" | "left_start" | "left-top" => Some(Self::LeftStart),
            "left-end" | "left_end" | "left-bottom" => Some(Self::LeftEnd),
            "right" | "inline-end" => Some(Self::Right),
            "right-start" | "right_start" | "right-top" => Some(Self::RightStart),
            "right-end" | "right_end" | "right-bottom" => Some(Self::RightEnd),
            _ => None,
        }
    }

    /// Canonical kebab-case string representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bottom => "bottom",
            Self::BottomStart => "bottom-start",
            Self::BottomEnd => "bottom-end",
            Self::Top => "top",
            Self::TopStart => "top-start",
            Self::TopEnd => "top-end",
            Self::Left => "left",
            Self::LeftStart => "left-start",
            Self::LeftEnd => "left-end",
            Self::Right => "right",
            Self::RightStart => "right-start",
            Self::RightEnd => "right-end",
        }
    }

    /// Returns the opposite primary-axis placement preserving cross-axis alignment (`bottom-end <-> top-end`).
    pub const fn opposite(self) -> Self {
        match self {
            Self::Bottom => Self::Top,
            Self::BottomStart => Self::TopStart,
            Self::BottomEnd => Self::TopEnd,
            Self::Top => Self::Bottom,
            Self::TopStart => Self::BottomStart,
            Self::TopEnd => Self::BottomEnd,
            Self::Left => Self::Right,
            Self::LeftStart => Self::RightStart,
            Self::LeftEnd => Self::RightEnd,
            Self::Right => Self::Left,
            Self::RightStart => Self::LeftStart,
            Self::RightEnd => Self::LeftEnd,
        }
    }

    #[inline]
    fn raw_coords(
        self,
        anchor: crate::geometry::Rect,
        floating_w: f32,
        floating_h: f32,
        offset: f32,
    ) -> (f32, f32) {
        match self {
            Self::Bottom => (
                anchor.x + (anchor.width - floating_w) * 0.5,
                anchor.y + anchor.height + offset,
            ),
            Self::BottomStart => (anchor.x, anchor.y + anchor.height + offset),
            Self::BottomEnd => (
                anchor.x + anchor.width - floating_w,
                anchor.y + anchor.height + offset,
            ),
            Self::Top => (
                anchor.x + (anchor.width - floating_w) * 0.5,
                anchor.y - floating_h - offset,
            ),
            Self::TopStart => (anchor.x, anchor.y - floating_h - offset),
            Self::TopEnd => (
                anchor.x + anchor.width - floating_w,
                anchor.y - floating_h - offset,
            ),
            Self::Left => (
                anchor.x - floating_w - offset,
                anchor.y + (anchor.height - floating_h) * 0.5,
            ),
            Self::LeftStart => (anchor.x - floating_w - offset, anchor.y),
            Self::LeftEnd => (
                anchor.x - floating_w - offset,
                anchor.y + anchor.height - floating_h,
            ),
            Self::Right => (
                anchor.x + anchor.width + offset,
                anchor.y + (anchor.height - floating_h) * 0.5,
            ),
            Self::RightStart => (anchor.x + anchor.width + offset, anchor.y),
            Self::RightEnd => (
                anchor.x + anchor.width + offset,
                anchor.y + anchor.height - floating_h,
            ),
        }
    }
}

/// Computes the viewport-aware anchored `(x, y, resolved_placement)` for a floating/top-layer element,
/// performing automatic primary-axis collision flipping (`bottom-end <-> top-end`, `left <-> right`)
/// and cross-axis viewport shift clamping.
#[allow(clippy::too_many_arguments)]
pub fn solve_anchored_position(
    anchor: crate::geometry::Rect,
    floating_w: f32,
    floating_h: f32,
    placement: AnchorPlacement,
    offset: f32,
    flip: bool,
    shift: bool,
    padding: f32,
    viewport: crate::geometry::Rect,
) -> (f32, f32, AnchorPlacement) {
    let min_x = viewport.x + padding;
    let min_y = viewport.y + padding;
    let max_x = (viewport.x + viewport.width - padding).max(min_x);
    let max_y = (viewport.y + viewport.height - padding).max(min_y);

    let mut chosen = placement;
    let (mut x, mut y) = chosen.raw_coords(anchor, floating_w, floating_h, offset);

    if flip {
        match placement {
            AnchorPlacement::Bottom | AnchorPlacement::BottomStart | AnchorPlacement::BottomEnd => {
                if y + floating_h > max_y {
                    let opp = placement.opposite();
                    let (ox, oy) = opp.raw_coords(anchor, floating_w, floating_h, offset);
                    let space_above = anchor.y - min_y;
                    let space_below = max_y - (anchor.y + anchor.height);
                    if oy >= min_y || space_above > space_below {
                        chosen = opp;
                        x = ox;
                        y = oy;
                    }
                }
            }
            AnchorPlacement::Top | AnchorPlacement::TopStart | AnchorPlacement::TopEnd => {
                if y < min_y {
                    let opp = placement.opposite();
                    let (ox, oy) = opp.raw_coords(anchor, floating_w, floating_h, offset);
                    let space_above = anchor.y - min_y;
                    let space_below = max_y - (anchor.y + anchor.height);
                    if oy + floating_h <= max_y || space_below > space_above {
                        chosen = opp;
                        x = ox;
                        y = oy;
                    }
                }
            }
            AnchorPlacement::Right | AnchorPlacement::RightStart | AnchorPlacement::RightEnd => {
                if x + floating_w > max_x {
                    let opp = placement.opposite();
                    let (ox, oy) = opp.raw_coords(anchor, floating_w, floating_h, offset);
                    let space_left = anchor.x - min_x;
                    let space_right = max_x - (anchor.x + anchor.width);
                    if ox >= min_x || space_left > space_right {
                        chosen = opp;
                        x = ox;
                        y = oy;
                    }
                }
            }
            AnchorPlacement::Left | AnchorPlacement::LeftStart | AnchorPlacement::LeftEnd => {
                if x < min_x {
                    let opp = placement.opposite();
                    let (ox, oy) = opp.raw_coords(anchor, floating_w, floating_h, offset);
                    let space_left = anchor.x - min_x;
                    let space_right = max_x - (anchor.x + anchor.width);
                    if ox + floating_w <= max_x || space_right > space_left {
                        chosen = opp;
                        x = ox;
                        y = oy;
                    }
                }
            }
        }
    }

    if shift {
        let upper_x = (max_x - floating_w).max(min_x);
        let upper_y = (max_y - floating_h).max(min_y);
        x = x.clamp(min_x, upper_x);
        y = y.clamp(min_y, upper_y);
    }

    (x, y, chosen)
}

/// Configuration attaching a top-layer portal or CSS anchor-positioned box to a trigger element.
#[derive(Debug, Clone, PartialEq)]
pub struct PortalAnchorSpec {
    pub anchor_id: String,
    pub placement: AnchorPlacement,
    pub offset: f32,
    pub flip: bool,
    pub shift: bool,
    pub viewport_padding: f32,
    pub match_width: bool,
}

impl PortalAnchorSpec {
    pub fn new(anchor_id: impl Into<String>, placement: AnchorPlacement) -> Self {
        Self {
            anchor_id: anchor_id.into(),
            placement,
            offset: 4.0,
            flip: true,
            shift: true,
            viewport_padding: 8.0,
            match_width: false,
        }
    }
}

/// Thread-safe type-erased handle to an embedded 3D scene (`Arc<Scene3D>`) carried on a layout node
/// so the 2D CSS compositor can emit `DrawViewport3D` at the exact stacking-context paint order.
#[derive(Clone)]
pub struct ViewportSceneHandle(pub std::sync::Arc<dyn std::any::Any + Send + Sync>);

impl std::fmt::Debug for ViewportSceneHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ViewportSceneHandle")
            .field(&"<scene_3d>")
            .finish()
    }
}

impl PartialEq for ViewportSceneHandle {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}

/// A node participating in the layout tree.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutNode {
    pub style: ComputedStyle,
    pub geometry: LayoutGeometry,
    pub children: Vec<LayoutNode>,
    pub text: Option<String>,
    pub svg_path: Option<String>,
    pub svg_items: Vec<SvgItem>,
    pub image: Option<ImageResource>,
    pub scene_3d: Option<ViewportSceneHandle>,
    pub inline_items: Vec<InlineItem>,
    pub line_boxes: Vec<LineBox>,
    pub layout_cache: LayoutCacheEntry,
    /// Content-box size used the last time this node formatted its descendants.
    pub last_child_layout_content: Option<(f32, f32)>,
    /// Table border grid matrix for collapsed border rendering (CSS 2.1 §17.6.2).
    pub table_grid: Option<crate::table::TableBorderGrid>,
    /// Unique 64-bit entity identifier associating this layout node with its blueprint element.
    pub entity_id: strata_primitives::EntityId,
    /// True when one or more CSS `transition-*` interpolations are currently in-flight on this node.
    pub active_transitions: bool,
    /// DOM `id` or `data-anchor-id` attribute used for live anchor lookup.
    pub element_id: Option<String>,
    /// CSS `anchor-name` (e.g. `--card-menu-trigger`) declared on this element.
    pub anchor_name: Option<String>,
    /// True when this element renders in the root Top Layer (`<Portal>` / `create_portal`),
    /// escaping all ancestor `overflow`, `transform`, `filter`, and stacking-context boundaries.
    pub is_top_layer: bool,
    /// Live anchor positioning specification resolved against post-scroll trigger bounds.
    pub portal_anchor: Option<Box<PortalAnchorSpec>>,
}

impl LayoutNode {
    /// Constructs a leaf layout node with specified computed styles.
    pub fn new(style: ComputedStyle) -> Self {
        Self {
            style,
            geometry: LayoutGeometry::default(),
            children: Vec::new(),
            text: None,
            svg_path: None,
            svg_items: Vec::new(),
            image: None,
            scene_3d: None,
            inline_items: Vec::new(),
            line_boxes: Vec::new(),
            layout_cache: LayoutCacheEntry::invalid(),
            last_child_layout_content: None,
            table_grid: None,
            entity_id: strata_primitives::EntityId::NONE,
            active_transitions: false,
            element_id: None,
            anchor_name: None,
            is_top_layer: false,
            portal_anchor: None,
        }
    }

    /// Constructs a layout node with child nodes.
    pub fn with_children(style: ComputedStyle, children: Vec<LayoutNode>) -> Self {
        Self {
            style,
            geometry: LayoutGeometry::default(),
            children,
            text: None,
            svg_path: None,
            svg_items: Vec::new(),
            image: None,
            scene_3d: None,
            inline_items: Vec::new(),
            line_boxes: Vec::new(),
            layout_cache: LayoutCacheEntry::invalid(),
            last_child_layout_content: None,
            table_grid: None,
            entity_id: strata_primitives::EntityId::NONE,
            active_transitions: false,
            element_id: None,
            anchor_name: None,
            is_top_layer: false,
            portal_anchor: None,
        }
    }

    /// Constructs a text leaf node with computed styles and string content.
    pub fn with_text(style: ComputedStyle, text: String) -> Self {
        Self {
            style,
            geometry: LayoutGeometry::default(),
            children: Vec::new(),
            text: Some(text),
            svg_path: None,
            svg_items: Vec::new(),
            image: None,
            scene_3d: None,
            inline_items: Vec::new(),
            line_boxes: Vec::new(),
            layout_cache: LayoutCacheEntry::invalid(),
            last_child_layout_content: None,
            table_grid: None,
            entity_id: strata_primitives::EntityId::NONE,
            active_transitions: false,
            element_id: None,
            anchor_name: None,
            is_top_layer: false,
            portal_anchor: None,
        }
    }

    /// Constructs an inline phrasing container with an inline item stream.
    pub fn with_inline_items(style: ComputedStyle, inline_items: Vec<InlineItem>) -> Self {
        Self {
            style,
            geometry: LayoutGeometry::default(),
            children: Vec::new(),
            text: None,
            svg_path: None,
            svg_items: Vec::new(),
            image: None,
            scene_3d: None,
            inline_items,
            line_boxes: Vec::new(),
            layout_cache: LayoutCacheEntry::invalid(),
            last_child_layout_content: None,
            table_grid: None,
            entity_id: strata_primitives::EntityId::NONE,
            active_transitions: false,
            element_id: None,
            anchor_name: None,
            is_top_layer: false,
            portal_anchor: None,
        }
    }

    /// Constructs an SVG vector path leaf node with computed styles and path string.
    pub fn with_svg_path(style: ComputedStyle, svg_path: String) -> Self {
        Self {
            style,
            geometry: LayoutGeometry::default(),
            children: Vec::new(),
            text: None,
            svg_path: Some(svg_path.clone()),
            svg_items: vec![SvgItem {
                path_data: svg_path,
                stroke_color: None,
                fill_color: None,
                stroke_is_current_color: true,
                fill_is_current_color: false,
                stroke_width: 2.0,
                stroke_linecap: StrokeLineCap::Round,
                stroke_linejoin: StrokeLineJoin::Round,
                stroke_dasharray: Vec::new(),
                fill_rule: FillRule::NonZero,
                view_box: (24.0, 24.0),
            }],
            image: None,
            scene_3d: None,
            inline_items: Vec::new(),
            line_boxes: Vec::new(),
            layout_cache: LayoutCacheEntry::invalid(),
            last_child_layout_content: None,
            table_grid: None,
            entity_id: strata_primitives::EntityId::NONE,
            active_transitions: false,
            element_id: None,
            anchor_name: None,
            is_top_layer: false,
            portal_anchor: None,
        }
    }

    /// Constructs an SVG layout node with multiple styled vector items and viewBox.
    pub fn with_svg_items(style: ComputedStyle, svg_items: Vec<SvgItem>) -> Self {
        let first_path = svg_items.first().map(|i| i.path_data.clone());
        Self {
            style,
            geometry: LayoutGeometry::default(),
            children: Vec::new(),
            text: None,
            svg_path: first_path,
            svg_items,
            image: None,
            scene_3d: None,
            inline_items: Vec::new(),
            line_boxes: Vec::new(),
            layout_cache: LayoutCacheEntry::invalid(),
            last_child_layout_content: None,
            table_grid: None,
            entity_id: strata_primitives::EntityId::NONE,
            active_transitions: false,
            element_id: None,
            anchor_name: None,
            is_top_layer: false,
            portal_anchor: None,
        }
    }

    /// Constructs a replaced raster image leaf node with intrinsic dimensions.
    pub fn with_image(style: ComputedStyle, image: ImageResource) -> Self {
        Self {
            style,
            geometry: LayoutGeometry::default(),
            children: Vec::new(),
            text: None,
            svg_path: None,
            svg_items: Vec::new(),
            image: Some(image),
            scene_3d: None,
            inline_items: Vec::new(),
            line_boxes: Vec::new(),
            layout_cache: LayoutCacheEntry::invalid(),
            last_child_layout_content: None,
            table_grid: None,
            entity_id: strata_primitives::EntityId::NONE,
            active_transitions: false,
            element_id: None,
            anchor_name: None,
            is_top_layer: false,
            portal_anchor: None,
        }
    }

    /// Returns `true` if this node or any of its descendants currently has an active CSS transition.
    pub fn has_active_transitions(&self) -> bool {
        if self.active_transitions {
            return true;
        }
        self.children.iter().any(|c| c.has_active_transitions())
    }

    /// Attaches a 3D viewport scene handle to this layout node.
    pub fn with_scene_3d(mut self, handle: ViewportSceneHandle) -> Self {
        self.scene_3d = Some(handle);
        self
    }

    /// Mutably sets or clears the 3D viewport scene handle on this layout node.
    pub fn set_scene_3d(&mut self, handle: Option<ViewportSceneHandle>) {
        self.scene_3d = handle;
    }

    /// Sets the EntityId on this layout node.
    pub fn with_entity_id(mut self, entity_id: strata_primitives::EntityId) -> Self {
        self.entity_id = entity_id;
        self
    }

    /// Mutably assigns the EntityId on this layout node.
    pub fn set_entity_id(&mut self, entity_id: strata_primitives::EntityId) {
        self.entity_id = entity_id;
    }

    /// Recursively locates a layout node by its EntityId.
    pub fn find_by_entity_id(&self, target_id: strata_primitives::EntityId) -> Option<&LayoutNode> {
        if !target_id.is_valid() {
            return None;
        }
        if self.entity_id == target_id {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find_by_entity_id(target_id) {
                return Some(found);
            }
        }
        None
    }

    /// Recursively locates a mutable layout node by its EntityId.
    pub fn find_by_entity_id_mut(
        &mut self,
        target_id: strata_primitives::EntityId,
    ) -> Option<&mut LayoutNode> {
        if !target_id.is_valid() {
            return None;
        }
        if self.entity_id == target_id {
            return Some(self);
        }
        for child in &mut self.children {
            if let Some(found) = child.find_by_entity_id_mut(target_id) {
                return Some(found);
            }
        }
        None
    }

    /// Appends a child layout node.
    pub fn add_child(&mut self, child: LayoutNode) {
        self.children.push(child);
    }

    /// Evaluates if this layout node establishes an atomic CSS stacking context within `parent_display`.
    #[inline]
    pub fn establishes_stacking_context_in(
        &self,
        parent_display: Option<Display>,
        is_root: bool,
    ) -> bool {
        self.style
            .establishes_stacking_context_in(parent_display, is_root)
    }

    /// Translates this node's geometry and all descendants by (dx, dy) using an iterative heap stack (O(1) call stack).
    pub fn translate(&mut self, dx: f32, dy: f32) {
        if dx == 0.0 && dy == 0.0 {
            return;
        }

        let mut stack: Vec<&mut LayoutNode> = vec![self];
        while let Some(node) = stack.pop() {
            node.geometry.translate(dx, dy);
            node.layout_cache.origin_x += dx;
            node.layout_cache.origin_y += dy;
            for line in &mut node.line_boxes {
                line.bounds.x += dx;
                line.bounds.y += dy;
                for frag in &mut line.text_fragments {
                    frag.bounds.x += dx;
                    frag.bounds.y += dy;
                    frag.baseline_y += dy;
                }
                for atomic in &mut line.atomic_fragments {
                    atomic.bounds.x += dx;
                    atomic.bounds.y += dy;
                }
            }
            if let Some(ref mut grid) = node.table_grid {
                grid.translate(dx, dy);
            }
            for child in &mut node.children {
                stack.push(child);
            }
        }
    }

    /// Translates descendants of a scroll container by `(dx, dy)` while honoring CSS containing-block
    /// escape semantics:
    /// - `Position::Fixed` subtrees do not scroll unless an ancestor at or inside the scroll container
    ///   established a fixed containing block (`transform`, `filter`, `backdrop-filter`).
    /// - `Position::Absolute` subtrees do not scroll unless an ancestor at or inside the scroll container
    ///   established an absolute containing block (`position != Static` or fixed CB).
    pub fn translate_scrolled(
        &mut self,
        dx: f32,
        dy: f32,
        parent_is_abs_cb: bool,
        parent_is_fixed_cb: bool,
    ) {
        if dx == 0.0 && dy == 0.0 {
            return;
        }

        let mut stack: Vec<(&mut LayoutNode, bool, bool)> =
            vec![(self, parent_is_abs_cb, parent_is_fixed_cb)];
        while let Some((node, abs_cb_inside, fixed_cb_inside)) = stack.pop() {
            if node.is_top_layer {
                continue;
            }
            match node.style.position {
                strata_style::types::Position::Fixed if !fixed_cb_inside => {
                    continue;
                }
                strata_style::types::Position::Absolute if !abs_cb_inside => {
                    continue;
                }
                _ => {}
            }

            node.geometry.translate(dx, dy);
            node.layout_cache.origin_x += dx;
            node.layout_cache.origin_y += dy;
            for line in &mut node.line_boxes {
                line.bounds.x += dx;
                line.bounds.y += dy;
                for frag in &mut line.text_fragments {
                    frag.bounds.x += dx;
                    frag.bounds.y += dy;
                    frag.baseline_y += dy;
                }
                for atomic in &mut line.atomic_fragments {
                    atomic.bounds.x += dx;
                    atomic.bounds.y += dy;
                }
            }
            if let Some(ref mut grid) = node.table_grid {
                grid.translate(dx, dy);
            }

            let child_fixed_cb = fixed_cb_inside || node.style.establishes_fixed_containing_block();
            let child_abs_cb = abs_cb_inside
                || node.style.position != strata_style::types::Position::Static
                || child_fixed_cb;

            for child in &mut node.children {
                stack.push((child, child_abs_cb, child_fixed_cb));
            }
        }
    }

    /// Formats the children of this node within its already-resolved content box.
    pub fn layout_children(&mut self) {
        self.layout_children_guarded(LayoutDepthGuard::root());
    }

    /// Formats the children of this node with an active depth guard.
    pub fn layout_children_guarded(&mut self, guard: LayoutDepthGuard) {
        self.layout_children_inner_guarded(guard, true);
    }

    pub(crate) fn layout_children_inner_guarded(
        &mut self,
        guard: LayoutDepthGuard,
        resolve_positioned: bool,
    ) {
        if (self.children.is_empty() && self.inline_items.is_empty()) || guard.is_exhausted() {
            return;
        }

        let content_w = self.geometry.content_box.width;
        let content_h = self.geometry.content_box.height;
        let preserved_geom = self.geometry;

        let saved_w = self.style.width;
        let saved_h = self.style.height;
        let saved_min_w = self.style.min_width;
        let saved_max_w = self.style.max_width;
        let saved_min_h = self.style.min_height;
        let saved_max_h = self.style.max_height;

        let pin_w = preserved_geom.border_box.width > 0.0
            || matches!(saved_w, strata_style::types::Dimension::Percent(_));
        let pin_h = preserved_geom.border_box.height > 0.0
            || matches!(saved_h, strata_style::types::Dimension::Percent(_));

        let container_avail_w = if pin_w {
            self.style.width = strata_style::types::Dimension::Px(preserved_geom.border_box.width);
            self.style.min_width = strata_style::types::Dimension::Auto;
            self.style.max_width = strata_style::types::Dimension::Auto;
            preserved_geom.border_box.width
        } else {
            preserved_geom.border_box.width + self.style.margin.left + self.style.margin.right
        };

        let container_avail_h = if pin_h {
            self.style.height =
                strata_style::types::Dimension::Px(preserved_geom.border_box.height);
            self.style.min_height = strata_style::types::Dimension::Auto;
            self.style.max_height = strata_style::types::Dimension::Auto;
            preserved_geom.border_box.height
        } else {
            preserved_geom.border_box.height + self.style.margin.top + self.style.margin.bottom
        };

        match self.style.display {
            Display::Flex | Display::InlineFlex => {
                let definite_cross = if content_h > 0.0 && !pin_h {
                    Some(content_h)
                } else {
                    None
                };
                crate::flexbox::solve_flexbox_with_definite_cross_guarded(
                    self,
                    container_avail_w,
                    container_avail_h,
                    definite_cross,
                    guard,
                );
            }
            Display::Grid | Display::InlineGrid => {
                crate::grid::solve_grid_guarded(self, container_avail_w, container_avail_h, guard);
            }
            Display::Table | Display::InlineTable => {
                crate::table::solve_table_guarded(
                    self,
                    container_avail_w,
                    container_avail_h,
                    guard,
                );
            }
            Display::Block
            | Display::ListItem
            | Display::Inline
            | Display::InlineBlock
            | Display::TableRowGroup
            | Display::TableHeaderGroup
            | Display::TableFooterGroup
            | Display::TableRow
            | Display::TableCell
            | Display::TableCaption
            | Display::TableColumnGroup
            | Display::TableColumn => {
                let block_h = crate::box_model::solve_block_children_with_exclusions_guarded(
                    self, content_w, content_h, None, 0.0, guard,
                );
                if saved_h == strata_style::types::Dimension::Auto {
                    self.geometry.border_box.height = crate::box_model::clamp_dimension(
                        block_h,
                        saved_min_h,
                        saved_max_h,
                        content_h,
                    );
                }
            }
            Display::None => {}
        }

        self.style.width = saved_w;
        self.style.height = saved_h;
        self.style.min_width = saved_min_w;
        self.style.max_width = saved_max_w;
        self.style.min_height = saved_min_h;
        self.style.max_height = saved_max_h;

        let clips_y = self.style.overflow_y != strata_style::types::Overflow::Visible;
        let final_h = if self.style.height == strata_style::types::Dimension::Auto && !clips_y {
            self.geometry
                .border_box
                .height
                .max(preserved_geom.border_box.height)
        } else if preserved_geom.border_box.height > 0.0 {
            preserved_geom.border_box.height
        } else {
            self.geometry.border_box.height
        };

        let clips_x = self.style.overflow_x != strata_style::types::Overflow::Visible;
        let final_w = if self.style.width == strata_style::types::Dimension::Auto && !clips_x {
            self.geometry
                .border_box
                .width
                .max(preserved_geom.border_box.width)
        } else if preserved_geom.border_box.width > 0.0 {
            preserved_geom.border_box.width
        } else {
            self.geometry.border_box.width
        };

        self.geometry = LayoutGeometry::from_border_box(
            crate::geometry::Rect::new(
                preserved_geom.border_box.x,
                preserved_geom.border_box.y,
                final_w,
                final_h,
            ),
            self.style.padding,
            self.style.border_width,
            self.style.margin,
        );

        if resolve_positioned && self.style.position != strata_style::types::Position::Static {
            crate::positioning::solve_positioning_guarded(self, content_w, content_h, guard);
        }
        self.geometry.snap_to_pixel();
        self.update_scroll_metrics();
        self.last_child_layout_content = Some((content_w, content_h));
        self.layout_cache.is_valid = false;
    }

    /// Computes layout geometry for this node and all descendants given available bounds.
    pub fn compute_layout(&mut self, available_width: f32, available_height: f32) {
        let session_root = crate::intrinsic::begin_layout_session();
        let _guard = crate::prof::LayoutGuard::new();
        let guard = LayoutDepthGuard::root();
        self.compute_layout_guarded(available_width, available_height, None, 0.0, guard);
        crate::intrinsic::end_layout_session(session_root);
    }

    /// Computes layout geometry passing an active exclusion space for float obstructions.
    pub fn compute_layout_with_exclusions(
        &mut self,
        available_width: f32,
        available_height: f32,
        exclusions: Option<&mut crate::exclusions::ExclusionSpace>,
        bfc_offset_y: f32,
    ) {
        let guard = LayoutDepthGuard::root();
        self.compute_layout_guarded(
            available_width,
            available_height,
            exclusions,
            bfc_offset_y,
            guard,
        );
    }

    /// Computes layout geometry passing an active exclusion space and depth guard.
    pub fn compute_layout_guarded(
        &mut self,
        available_width: f32,
        available_height: f32,
        exclusions: Option<&mut crate::exclusions::ExclusionSpace>,
        bfc_offset_y: f32,
        guard: LayoutDepthGuard,
    ) {
        let _guard = crate::prof::LayoutGuard::new();
        crate::prof::observe_width(self.children.len());

        let available_width = if available_width.is_finite() && available_width >= 0.0 {
            available_width.min(1e7)
        } else {
            0.0
        };
        let available_height = if available_height.is_finite() && available_height >= 0.0 {
            available_height.min(1e7)
        } else {
            0.0
        };

        if self.style.display == Display::None {
            self.geometry = LayoutGeometry::default();
            self.layout_cache.is_valid = false;
            return;
        }

        if guard.is_exhausted() {
            record_layout_overflow(guard.current_depth(), guard.max_depth());
            let padding = self.style.padding;
            let border = self.style.border_width;
            let margin = self.style.margin;

            let w = crate::box_model::resolve_dimension_with_node(
                self.style.width,
                available_width,
                self,
            )
            .unwrap_or(available_width)
            .max(0.0);
            let h = crate::box_model::resolve_dimension(self.style.height, available_height)
                .unwrap_or_else(|| {
                    if self.text.is_some() {
                        self.style.font_size * self.style.line_height
                    } else {
                        0.0
                    }
                })
                .max(0.0);

            self.geometry = LayoutGeometry::from_border_box(
                crate::geometry::Rect::new(
                    self.geometry.border_box.x,
                    self.geometry.border_box.y,
                    w,
                    h,
                ),
                padding,
                border,
                margin,
            );
            self.geometry.snap_to_pixel();
            self.layout_cache.is_valid = false;
            return;
        }

        let is_unconstrained_root = exclusions
            .as_ref()
            .is_none_or(|e| e.exclusions().is_empty());

        // Subtree layout cache hit: skip re-traversal if available dimensions are unchanged
        let height_independent = self.style.height == strata_style::types::Dimension::Auto
            && self.style.flex_basis == strata_style::types::Dimension::Auto
            && self.style.flex_grow == 0.0
            && !matches!(
                self.style.min_height,
                strata_style::types::Dimension::Percent(_)
            )
            && !matches!(
                self.style.max_height,
                strata_style::types::Dimension::Percent(_)
            )
            && !(matches!(
                self.style.position,
                strata_style::types::Position::Absolute | strata_style::types::Position::Fixed
            ) && (!matches!(
                self.style.insets.bottom,
                strata_style::types::Dimension::Auto
            ) || matches!(
                self.style.insets.top,
                strata_style::types::Dimension::Percent(_)
            )))
            && !self.has_height_dependent_descendant();
        let height_matches = (available_height - self.layout_cache.available_h).abs() < 0.01
            || (height_independent
                && (self.style.content_visibility == strata_style::types::ContentVisibility::Auto
                    || self.layout_cache.is_valid));

        if is_unconstrained_root
            && self.layout_cache.is_valid
            && (available_width - self.layout_cache.available_w).abs() < 0.01
            && height_matches
        {
            let dx = self.geometry.border_box.x - self.layout_cache.origin_x;
            let dy = self.geometry.border_box.y - self.layout_cache.origin_y;
            if dx.abs() > 0.001 || dy.abs() > 0.001 {
                for line in &mut self.line_boxes {
                    line.bounds.x += dx;
                    line.bounds.y += dy;
                    for frag in &mut line.text_fragments {
                        frag.bounds.x += dx;
                        frag.bounds.y += dy;
                        frag.baseline_y += dy;
                    }
                    for at in &mut line.atomic_fragments {
                        at.bounds.x += dx;
                        at.bounds.y += dy;
                    }
                }
                for child in &mut self.children {
                    child.translate(dx, dy);
                }
                self.layout_cache.origin_x = self.geometry.border_box.x;
                self.layout_cache.origin_y = self.geometry.border_box.y;
            }
            return;
        }

        // Dispatch according to container display formatting context
        match self.style.display {
            Display::Flex | Display::InlineFlex => {
                crate::flexbox::solve_flexbox_guarded(
                    self,
                    available_width,
                    available_height,
                    guard,
                );
            }
            Display::Grid | Display::InlineGrid => {
                crate::grid::solve_grid_guarded(self, available_width, available_height, guard);
            }
            Display::Table | Display::InlineTable => {
                crate::table::solve_table_guarded(self, available_width, available_height, guard);
            }
            Display::Block
            | Display::ListItem
            | Display::Inline
            | Display::InlineBlock
            | Display::TableRowGroup
            | Display::TableHeaderGroup
            | Display::TableFooterGroup
            | Display::TableRow
            | Display::TableCell
            | Display::TableCaption
            | Display::TableColumnGroup
            | Display::TableColumn => {
                crate::box_model::solve_block_with_exclusions_guarded(
                    self,
                    available_width,
                    available_height,
                    exclusions,
                    bfc_offset_y,
                    guard,
                );
            }
            Display::None => unreachable!(),
        }

        // Resolve relative and out-of-flow absolute/fixed positioned children
        crate::positioning::solve_positioning_guarded(
            self,
            available_width,
            available_height,
            guard,
        );

        // Subpixel snapping and geometry sanitization
        self.geometry.snap_to_pixel();
        self.geometry.sanitize();

        // Update scrollable overflow metrics
        self.update_scroll_metrics();

        if !self.children.is_empty() || !self.inline_items.is_empty() || self.text.is_some() {
            self.last_child_layout_content = Some((
                self.geometry.content_box.width,
                self.geometry.content_box.height,
            ));
        }

        // Record layout result in cache for subsequent multi-pass probes
        if is_unconstrained_root {
            self.layout_cache = LayoutCacheEntry {
                available_w: available_width,
                available_h: available_height,
                result_w: self.geometry.border_box.width,
                result_h: self.geometry.border_box.height,
                origin_x: self.geometry.border_box.x,
                origin_y: self.geometry.border_box.y,
                is_valid: true,
            };
        }
    }

    /// Returns true if any descendant (up to an explicit pixel-height boundary) depends on parent height.
    fn has_height_dependent_descendant(&self) -> bool {
        use strata_style::types::{Dimension, Position};
        let mut stack: Vec<&LayoutNode> = self.children.iter().collect();
        while let Some(node) = stack.pop() {
            if matches!(node.style.height, Dimension::Percent(_))
                || matches!(node.style.min_height, Dimension::Percent(_))
                || matches!(node.style.max_height, Dimension::Percent(_))
                || matches!(node.style.flex_basis, Dimension::Percent(_))
            {
                return true;
            }
            if matches!(node.style.position, Position::Absolute | Position::Fixed)
                && (!matches!(node.style.insets.bottom, Dimension::Auto)
                    || matches!(node.style.insets.top, Dimension::Percent(_)))
            {
                return true;
            }
            if !matches!(node.style.height, Dimension::Px(_)) {
                for grand in &node.children {
                    stack.push(grand);
                }
            }
        }
        false
    }

    /// Invalidates layout cache on this node and all descendants using an iterative heap stack.
    pub fn invalidate_cache(&mut self) {
        let mut stack: Vec<&mut LayoutNode> = vec![self];
        while let Some(node) = stack.pop() {
            node.layout_cache.is_valid = false;
            for child in &mut node.children {
                stack.push(child);
            }
        }
    }

    /// Returns true if this node forms an isolated layout boundary (W3C CSS Containment Level 2 §3.1).
    ///
    /// An element is a layout boundary if internal descendant changes can never affect
    /// the layout of ancestor or sibling elements outside this boundary:
    /// 1. Explicit CSS `contain: strict` or `contain: content`
    /// 2. Explicit CSS `contain: layout` combined with `contain: size` or definite pixel dimensions
    /// 3. CSS `content-visibility: auto` or `hidden` with definite pixel dimensions
    /// 4. Out-of-flow positioned boxes with clipping and definite pixel dimensions
    pub fn is_layout_boundary(&self) -> bool {
        // 1. Explicit contain: strict or contain: content
        if self.style.contain == strata_style::types::Containment::STRICT
            || self.style.contain == strata_style::types::Containment::CONTENT
        {
            return true;
        }

        // 2. Explicit contain: layout with contain: size
        if self.style.contain.layout && self.style.contain.size {
            return true;
        }

        // 3. Explicit contain: layout with definite width and height
        if self.style.contain.layout && self.has_definite_dimensions() {
            return true;
        }

        // 4. Content-visibility containment with definite dimensions
        if self.style.content_visibility != strata_style::types::ContentVisibility::Visible
            && self.has_definite_dimensions()
        {
            return true;
        }

        // 5. Out-of-flow positioned boxes with clipping and definite dimensions
        if matches!(
            self.style.position,
            strata_style::types::Position::Absolute | strata_style::types::Position::Fixed
        ) && self.has_definite_dimensions()
            && self.style.overflow_x != strata_style::types::Overflow::Visible
            && self.style.overflow_y != strata_style::types::Overflow::Visible
        {
            return true;
        }

        false
    }

    /// Checks if both width and height are definite non-auto pixel dimensions.
    #[inline]
    pub fn has_definite_dimensions(&self) -> bool {
        matches!(self.style.width, strata_style::types::Dimension::Px(_))
            && matches!(self.style.height, strata_style::types::Dimension::Px(_))
    }

    /// Recursively searches for the nearest ancestor layout boundary containing the given index path.
    /// Returns the prefix of `path` that terminates at the nearest layout boundary container.
    pub fn find_layout_boundary_path<'a>(&self, path: &'a [usize]) -> Option<&'a [usize]> {
        let mut best_boundary_len = if self.is_layout_boundary() {
            Some(0)
        } else {
            None
        };

        let mut curr = self;
        for (i, &child_idx) in path.iter().enumerate() {
            if child_idx >= curr.children.len() {
                break;
            }
            curr = &curr.children[child_idx];
            if curr.is_layout_boundary() {
                best_boundary_len = Some(i + 1);
            }
        }

        best_boundary_len.map(|len| &path[..len])
    }

    /// Recomputes layout for the subtree rooted at `path` within its containing layout boundary.
    ///
    /// If an ancestor layout boundary exists along `path`, layout is recomputed ONLY for that
    /// boundary container and its descendants, guaranteeing zero re-layouts on the rest of the tree.
    /// Returns `true` if a layout boundary was found and laid out locally, or `false` if root
    /// layout recomputation is required.
    pub fn compute_subgrid_or_boundary_layout(&mut self, path: &[usize]) -> bool {
        let boundary_len = match self.find_layout_boundary_path(path) {
            Some(subpath) => subpath.len(),
            None => return false,
        };

        if boundary_len == 0 {
            // Root itself is the layout boundary
            self.layout_children();
            return true;
        }

        // Navigate to the boundary node mutably
        let mut curr = self;
        for &idx in &path[..boundary_len] {
            if idx >= curr.children.len() {
                return false;
            }
            curr = &mut curr.children[idx];
        }

        // Recompute layout strictly within the isolated boundary
        curr.layout_children();
        true
    }

    /// Invalidates layout cache only for nodes along `path` under the nearest layout boundary.
    pub fn invalidate_layout_boundary(&mut self, path: &[usize]) -> bool {
        let boundary_len = match self.find_layout_boundary_path(path) {
            Some(subpath) => subpath.len(),
            None => {
                self.invalidate_cache();
                return false;
            }
        };

        let mut curr = self;
        for &idx in &path[..boundary_len] {
            if idx >= curr.children.len() {
                return false;
            }
            curr = &mut curr.children[idx];
        }

        curr.invalidate_cache();
        true
    }

    /// Recursively calculates the index path to the layout node with matching EntityId.
    pub fn find_path_by_entity_id(
        &self,
        target_id: strata_primitives::EntityId,
    ) -> Option<Vec<usize>> {
        if !target_id.is_valid() {
            return None;
        }
        let mut path = Vec::new();
        if self.find_path_by_entity_id_internal(target_id, &mut path) {
            Some(path)
        } else {
            None
        }
    }

    fn find_path_by_entity_id_internal(
        &self,
        target_id: strata_primitives::EntityId,
        path: &mut Vec<usize>,
    ) -> bool {
        if self.entity_id == target_id {
            return true;
        }
        for (i, child) in self.children.iter().enumerate() {
            path.push(i);
            if child.find_path_by_entity_id_internal(target_id, path) {
                return true;
            }
            path.pop();
        }
        false
    }

    /// Invalidates layout cache on self and all ancestors leading down to `path`.
    pub fn invalidate_ancestors(&mut self, path: &[usize]) {
        self.layout_cache.is_valid = false;
        let mut curr = self;
        for &idx in path {
            if idx >= curr.children.len() {
                break;
            }
            curr = &mut curr.children[idx];
            curr.layout_cache.is_valid = false;
        }
    }

    /// Evaluates child geometry to compute `scroll_bounds` and `has_scroll` for this container.
    pub fn update_scroll_metrics(&mut self) {
        let origin_x = self.geometry.padding_box.x;
        let origin_y = self.geometry.padding_box.y;
        let mut max_extent_x = self.geometry.content_box.width;
        let mut max_extent_y = self.geometry.content_box.height;

        for child in &self.children {
            if child.style.display == strata_style::types::Display::None
                || child.style.position == strata_style::types::Position::Fixed
            {
                continue;
            }
            let child_end_x = child.geometry.border_box.max_x() - origin_x;
            let child_end_y = child.geometry.border_box.max_y() - origin_y;
            if child_end_x.is_finite() && child_end_x > max_extent_x {
                max_extent_x = child_end_x;
            }
            if child_end_y.is_finite() && child_end_y > max_extent_y {
                max_extent_y = child_end_y;
            }
        }

        for line in &self.line_boxes {
            let line_end_x = line.bounds.max_x() - origin_x;
            let line_end_y = line.bounds.max_y() - origin_y;
            if line_end_x.is_finite() && line_end_x > max_extent_x {
                max_extent_x = line_end_x;
            }
            if line_end_y.is_finite() && line_end_y > max_extent_y {
                max_extent_y = line_end_y;
            }
        }

        self.geometry.scroll_bounds = crate::geometry::Size::new(max_extent_x, max_extent_y);

        let has_scroll_x = match self.style.overflow_x {
            strata_style::types::Overflow::Scroll => true,
            strata_style::types::Overflow::Auto => max_extent_x > self.geometry.padding_box.width,
            _ => false,
        };
        let has_scroll_y = match self.style.overflow_y {
            strata_style::types::Overflow::Scroll => true,
            strata_style::types::Overflow::Auto => max_extent_y > self.geometry.padding_box.height,
            _ => false,
        };
        self.geometry.has_scroll = (has_scroll_x, has_scroll_y);
        self.geometry.scroll_offset = self
            .geometry
            .clamp_scroll_offset(self.geometry.scroll_offset);
    }
}

impl Drop for LayoutNode {
    fn drop(&mut self) {
        // Drain descendants iteratively onto a heap stack to prevent recursion stack overflow
        // during deallocation of deeply nested trees (e.g. 10,000+ levels).
        let mut stack = std::mem::take(&mut self.children);
        while let Some(mut node) = stack.pop() {
            if !node.children.is_empty() {
                let grandchildren = std::mem::take(&mut node.children);
                stack.extend(grandchildren);
            }
        }
    }
}
