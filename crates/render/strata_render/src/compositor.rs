//! CSS 2.1 Appendix E 7-Layer Stacking Context Compositor & Display List Generator.

use crate::scene3d::Scene3D;
use strata_layout::{LayoutNode, Rect};
use strata_primitives::geom::EdgeInsets;
use strata_style::effects::{BackdropFilter, ClipPath};
use strata_style::types::{
    BlendMode, BorderCollapse, BorderRadius, BorderStyle, BoxShadow, Color, ColorStop, Display,
    FontWeight, Isolation, Overflow, Position,
};

/// Low-level drawing primitives emitted to GPU command buffers.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    DrawRect {
        bounds: Rect,
        color: Color,
    },
    DrawRoundedRect {
        bounds: Rect,
        color: Color,
        border_radius: BorderRadius,
        border_width: EdgeInsets<f32>,
        border_color: Option<Color>,
    },
    DrawShadow {
        bounds: Rect,
        shadow: BoxShadow,
        border_radius: BorderRadius,
    },
    DrawInsetShadow {
        bounds: Rect,
        shadow: BoxShadow,
        border_radius: BorderRadius,
    },
    DrawLinearGradient {
        bounds: Rect,
        angle_deg: f32,
        stops: Vec<ColorStop>,
        border_radius: BorderRadius,
    },
    DrawRadialGradient {
        bounds: Rect,
        center: (f32, f32),
        stops: Vec<ColorStop>,
        border_radius: BorderRadius,
    },
    PushBackdropFilter {
        bounds: Rect,
        radius: BorderRadius,
        filter: BackdropFilter,
    },
    PopBackdropFilter,
    PushFilter {
        bounds: Rect,
        filter: BackdropFilter,
    },
    PopFilter,
    PushClipPath {
        bounds: Rect,
        clip_path: ClipPath,
    },
    PopClipPath,
    PushBlendMode {
        mode: BlendMode,
    },
    PopBlendMode,
    PushIsolation {
        isolate: bool,
    },
    PopIsolation,
    PushOpacity {
        alpha: f32,
    },
    PopOpacity,
    DrawBorder {
        bounds: Rect,
        widths: EdgeInsets<f32>,
        color: Color,
        border_radius: BorderRadius,
        style: BorderStyle,
    },
    DrawText {
        origin: (f32, f32),
        bounds_width: Option<f32>,
        bounds_height: Option<f32>,
        text_align: strata_style::TextAlign,
        text: String,
        font_size: f32,
        line_height: f32,
        font_weight: FontWeight,
        color: Color,
        is_monospace: bool,
        font_family: strata_primitives::AtomId,
    },
    DrawTextFragments {
        fragments: Vec<strata_layout::TextFragment>,
    },
    DrawViewport3D {
        bounds: Rect,
        scene: Scene3D,
    },
    DrawCanvas2D {
        bounds: Rect,
        commands: Vec<crate::canvas2d::CanvasCommand>,
    },
    DrawSvgPath {
        bounds: Rect,
        path_data: String,
        stroke_color: Option<Color>,
        fill_color: Option<Color>,
        stroke_width: f32,
        stroke_linecap: strata_layout::StrokeLineCap,
        stroke_linejoin: strata_layout::StrokeLineJoin,
        stroke_dasharray: Vec<f32>,
        fill_rule: strata_layout::FillRule,
        view_box: (f32, f32),
    },
    DrawGpuPath {
        bounds: Rect,
        path: strata_gpu_path::Path,
        stroke: Option<strata_gpu_path::StrokeOptions>,
        fill_color: Option<Color>,
        stroke_color: Option<Color>,
        transform: Option<strata_primitives::geom::Matrix4x4>,
    },
    DrawImage {
        bounds: Rect,
        image: strata_layout::ImageResource,
    },
    PushClip {
        bounds: Rect,
        radius: BorderRadius,
    },
    PopClip,
    PushTransform {
        matrix: strata_primitives::geom::Matrix4x4,
    },
    PopTransform,
}

impl DrawCommand {
    /// Returns true if this command is a clipping push or pop instruction.
    #[inline]
    pub fn is_clip(&self) -> bool {
        matches!(
            self,
            Self::PushClip { .. }
                | Self::PopClip
                | Self::PushBackdropFilter { .. }
                | Self::PopBackdropFilter
                | Self::PushClipPath { .. }
                | Self::PopClipPath
        )
    }

    /// Returns true if this command mutates the render stack (push or pop).
    ///
    /// Stack commands must never be spatially culled, otherwise a pop would be
    /// executed without its matching push (for example a Cairo group pop with
    /// no group to pop).
    #[inline]
    pub fn is_state_command(&self) -> bool {
        matches!(
            self,
            Self::PushClip { .. }
                | Self::PopClip
                | Self::PushBackdropFilter { .. }
                | Self::PopBackdropFilter
                | Self::PushClipPath { .. }
                | Self::PopClipPath
                | Self::PushFilter { .. }
                | Self::PopFilter
                | Self::PushBlendMode { .. }
                | Self::PopBlendMode
                | Self::PushIsolation { .. }
                | Self::PopIsolation
                | Self::PushOpacity { .. }
                | Self::PopOpacity
                | Self::PushTransform { .. }
                | Self::PopTransform
        )
    }

    /// Returns the bounding rectangle for this drawing command, if applicable.
    pub fn bounds(&self) -> Option<Rect> {
        match self {
            Self::DrawRect { bounds, .. } => Some(*bounds),
            Self::DrawRoundedRect { bounds, .. } => Some(*bounds),
            Self::DrawShadow { bounds, shadow, .. } => {
                let spread = shadow.spread_radius + shadow.blur_radius;
                Some(Rect::new(
                    bounds.x + shadow.offset_x - spread,
                    bounds.y + shadow.offset_y - spread,
                    bounds.width + spread * 2.0,
                    bounds.height + spread * 2.0,
                ))
            }
            Self::DrawInsetShadow { bounds, .. } => Some(*bounds),
            Self::DrawLinearGradient { bounds, .. } => Some(*bounds),
            Self::DrawRadialGradient { bounds, .. } => Some(*bounds),
            Self::PushBackdropFilter { bounds, .. } => Some(*bounds),
            Self::PopBackdropFilter => None,
            Self::PushFilter { bounds, .. } => Some(*bounds),
            Self::PopFilter => None,
            Self::PushClipPath { bounds, .. } => Some(*bounds),
            Self::PopClipPath => None,
            Self::PushBlendMode { .. } | Self::PopBlendMode => None,
            Self::PushIsolation { .. } | Self::PopIsolation => None,
            Self::PushOpacity { .. } | Self::PopOpacity => None,
            Self::DrawBorder { bounds, .. } => Some(*bounds),
            Self::DrawText {
                origin,
                text,
                font_size,
                line_height,
                ..
            } => {
                let h = line_height.max(*font_size);
                let w = text.len() as f32 * font_size * 0.6;
                Some(Rect::new(origin.0, origin.1, w, h))
            }
            Self::DrawTextFragments { fragments } => {
                if fragments.is_empty() {
                    None
                } else {
                    let mut min_x = f32::MAX;
                    let mut min_y = f32::MAX;
                    let mut max_x = f32::MIN;
                    let mut max_y = f32::MIN;
                    for f in fragments {
                        min_x = min_x.min(f.bounds.x);
                        min_y = min_y.min(f.bounds.y);
                        max_x = max_x.max(f.bounds.x + f.bounds.width);
                        max_y = max_y.max(f.bounds.y + f.bounds.height);
                    }
                    Some(Rect::new(
                        min_x,
                        min_y,
                        (max_x - min_x).max(0.0),
                        (max_y - min_y).max(0.0),
                    ))
                }
            }
            Self::DrawViewport3D { bounds, .. } => Some(*bounds),
            Self::DrawCanvas2D { bounds, .. } => Some(*bounds),
            Self::DrawSvgPath { bounds, .. } => Some(*bounds),
            Self::DrawGpuPath { bounds, .. } => Some(*bounds),
            Self::DrawImage { bounds, .. } => Some(*bounds),
            Self::PushClip { bounds, .. } => Some(*bounds),
            Self::PopClip => None,
            Self::PushTransform { .. } | Self::PopTransform => None,
        }
    }
}

/// A serialized sequence of render instructions for a single frame.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct DisplayList {
    pub commands: Vec<DrawCommand>,
}

impl DisplayList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, cmd: DrawCommand) {
        self.commands.push(cmd);
    }

    pub fn commands(&self) -> &[DrawCommand] {
        &self.commands
    }

    pub fn clear(&mut self) {
        self.commands.clear();
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

/// Tracks damaged (dirty) screen regions across frames to minimize redraw overhead.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct DamageTracker {
    pub dirty_rects: Vec<Rect>,
}

impl DamageTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_damage(&mut self, rect: Rect) {
        if rect.width > 0.0 && rect.height > 0.0 {
            self.dirty_rects.push(rect);
        }
    }

    /// Computes the minimal bounding rectangle covering all dirty regions.
    pub fn bounding_damage(&self) -> Option<Rect> {
        if self.dirty_rects.is_empty() {
            return None;
        }

        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;

        for r in &self.dirty_rects {
            min_x = min_x.min(r.x);
            min_y = min_y.min(r.y);
            max_x = max_x.max(r.max_x());
            max_y = max_y.max(r.max_y());
        }

        Some(Rect::new(min_x, min_y, max_x - min_x, max_y - min_y))
    }

    /// Hierarchically clusters dirty rects into isolated non-overlapping bounding regions.
    ///
    /// Prevents disjoint localized damage (e.g., caret blink at top-left, button ripple at bottom-right)
    /// from inflating into a catastrophic full-canvas bounding box.
    pub fn cluster_damage_rects(&self) -> Vec<Rect> {
        if self.dirty_rects.is_empty() {
            return Vec::new();
        }

        let mut clusters: Vec<Rect> = Vec::new();
        for &r in &self.dirty_rects {
            let mut merged = false;
            for cluster in &mut clusters {
                // If rects intersect or are within a close margin (16px), merge them
                let expanded = Rect::new(
                    cluster.x - 16.0,
                    cluster.y - 16.0,
                    cluster.width + 32.0,
                    cluster.height + 32.0,
                );
                if expanded.intersect(r).is_some() {
                    let min_x = cluster.x.min(r.x);
                    let min_y = cluster.y.min(r.y);
                    let max_x = cluster.max_x().max(r.max_x());
                    let max_y = cluster.max_y().max(r.max_y());
                    *cluster = Rect::new(min_x, min_y, max_x - min_x, max_y - min_y);
                    merged = true;
                    break;
                }
            }
            if !merged {
                clusters.push(r);
            }
        }
        clusters
    }

    pub fn reset(&mut self) {
        self.dirty_rects.clear();
    }
}

/// Compiles a `LayoutNode` tree into a `DisplayList` following CSS 2.1 Appendix E.
pub struct Compositor;

impl Compositor {
    /// Compiles a layout tree root into an ordered DisplayList.
    pub fn build_display_list(root: &LayoutNode) -> DisplayList {
        Self::build_display_list_with_viewport(root, None)
    }

    /// Compiles a layout tree root into an ordered DisplayList with spatial viewport culling.
    /// Elements outside the active viewport buffer zone are culled from command emission.
    pub fn build_display_list_with_viewport(
        root: &LayoutNode,
        viewport: Option<Rect>,
    ) -> DisplayList {
        let mut list = DisplayList::new();
        let cull_rect = viewport.map(|vp| {
            let buffer_y = 1200.0;
            let buffer_x = 300.0;
            Rect::new(
                vp.x - buffer_x,
                vp.y - buffer_y,
                vp.width + buffer_x * 2.0,
                vp.height + buffer_y * 2.0,
            )
        });
        Self::paint_stacking_context_inner(root, &mut list, cull_rect, cull_rect, &[], true, 0);
        list
    }

    /// Determines if an element establishes a new Stacking Context (without parent display context).
    ///
    /// For flex/grid item `z-index` awareness, tree traversals use
    /// [`LayoutNode::establishes_stacking_context_in`].
    #[inline]
    pub fn establishes_stacking_context(node: &LayoutNode, is_root: bool) -> bool {
        node.establishes_stacking_context_in(None, is_root)
    }

    /// Collects the border boxes of every cell in a table subtree.
    fn collect_table_cells<'a>(node: &'a LayoutNode, out: &mut Vec<&'a LayoutNode>) {
        if node.style.display == Display::TableCell {
            out.push(node);
            return;
        }
        for child in &node.children {
            Self::collect_table_cells(child, out);
        }
    }

    /// Paints the shared grid of a collapsed-border table once.
    ///
    /// Each cell paints its own border box, so adjacent rows would otherwise
    /// draw the shared horizontal border twice (one row per side) and adjacent
    /// columns would draw parallel strokes. In the collapsed model every
    /// boundary is a single line snapped to whole device pixels, matching the
    /// Chromium reference. Cell borders are suppressed separately.
    fn paint_collapsed_table_grid(node: &LayoutNode, list: &mut DisplayList) {
        if node.style.display != Display::Table
            || node.style.border_collapse != BorderCollapse::Collapse
        {
            return;
        }

        if let Some(ref grid) = node.table_grid {
            // CSS 2.1 §17.6.2: Draw collapsed horizontal border segments centered on grid lines
            for r in 0..=grid.num_rows {
                let y = grid.row_positions[r];
                for c in 0..grid.num_cols {
                    let edge = &grid.horizontal_edges[r][c];
                    if edge.style == BorderStyle::Hidden
                        || edge.style == BorderStyle::None
                        || edge.width <= 0.0
                    {
                        continue;
                    }
                    let x0 = grid.col_positions[c];
                    let x1 = grid.col_positions[c + 1];
                    let w = (x1 - x0).max(1.0);
                    let color = edge.color.unwrap_or(node.style.color);
                    let thickness = edge.width.round().max(1.0);
                    let draw_y = (y - 0.5 * thickness).round();
                    list.push(DrawCommand::DrawRect {
                        bounds: Rect::new(x0.round(), draw_y, w.round(), thickness),
                        color,
                    });
                }
            }

            // CSS 2.1 §17.6.2: Draw collapsed vertical border segments centered on grid lines
            for r in 0..grid.num_rows {
                let y0 = grid.row_positions[r];
                let y1 = grid.row_positions[r + 1];
                let h = (y1 - y0).max(1.0);
                for c in 0..=grid.num_cols {
                    let edge = &grid.vertical_edges[r][c];
                    if edge.style == BorderStyle::Hidden
                        || edge.style == BorderStyle::None
                        || edge.width <= 0.0
                    {
                        continue;
                    }
                    let x = grid.col_positions[c];
                    let color = edge.color.unwrap_or(node.style.color);
                    let thickness = edge.width.round().max(1.0);
                    let draw_x = (x - 0.5 * thickness).round();
                    list.push(DrawCommand::DrawRect {
                        bounds: Rect::new(draw_x, y0.round(), thickness, h.round()),
                        color,
                    });
                }
            }
            return;
        }

        let mut cells: Vec<&LayoutNode> = Vec::new();
        Self::collect_table_cells(node, &mut cells);
        cells.retain(|cell| {
            cell.geometry.border_box.width > 0.0 && cell.geometry.border_box.height > 0.0
        });
        if cells.is_empty() {
            return;
        }

        let mut stroke_width = 0.0f32;
        let mut stroke_color: Option<Color> = None;
        for cell in &cells {
            let widths = cell.style.border_width;
            let width = widths
                .top
                .max(widths.right)
                .max(widths.bottom)
                .max(widths.left);
            if width > stroke_width {
                stroke_width = width;
                stroke_color = cell.style.border_color;
            }
        }
        if stroke_width <= 0.0 {
            return;
        }
        let color = stroke_color.unwrap_or(node.style.color);

        let mut xs: Vec<f32> = Vec::new();
        let mut ys: Vec<f32> = Vec::new();
        for cell in &cells {
            let b = cell.geometry.border_box;
            xs.push(b.x);
            xs.push(b.x + b.width);
            ys.push(b.y);
            ys.push(b.y + b.height);
        }
        let snap = |v: f32| v.round();
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        xs.dedup_by(|a, b| (*a - *b).abs() < 0.01);
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        ys.dedup_by(|a, b| (*a - *b).abs() < 0.01);

        let top = snap(*ys.first().unwrap_or(&0.0));
        let bottom = snap(*ys.last().unwrap_or(&0.0));
        let line_h = (bottom - top + 1.0).max(1.0);
        let left = snap(node.geometry.border_box.x);
        let right = snap(*xs.last().unwrap_or(&0.0));
        let line_w = (right - left).max(1.0);
        let thickness = stroke_width.round().max(1.0);

        for x in xs {
            let snapped = snap(x);
            // The final right edge paints inside the table's right boundary.
            let draw_x = if (snapped - right).abs() < 0.01 {
                snapped - thickness
            } else {
                snapped
            };
            list.push(DrawCommand::DrawRect {
                bounds: Rect::new(draw_x, top, thickness, line_h),
                color,
            });
        }

        for y in ys {
            let snapped = snap(y);
            list.push(DrawCommand::DrawRect {
                bounds: Rect::new(left, snapped, line_w, thickness),
                color,
            });
        }
    }

    /// Evaluates if an inline element or its descendants have paintable atomic/SVG non-text visual content.
    fn inline_has_paintable_non_text_content(node: &LayoutNode) -> bool {
        if !node.svg_items.is_empty() || node.svg_path.is_some() {
            return true;
        }
        for c in &node.children {
            if c.style.display != Display::Inline || Self::inline_has_paintable_non_text_content(c)
            {
                return true;
            }
        }
        false
    }

    /// Maximum allowable recursion depth for stacking context and layout node display list composition.
    pub const MAX_COMPOSITOR_DEPTH: usize = 1024;

    /// Paints a stacking context according to the 7-layer CSS 2.1 Appendix E algorithm.
    pub fn paint_stacking_context(node: &LayoutNode, list: &mut DisplayList) {
        Self::paint_stacking_context_culled(node, list, None);
    }

    /// Paints a stacking context with optional spatial viewport culling.
    pub fn paint_stacking_context_culled(
        node: &LayoutNode,
        list: &mut DisplayList,
        cull: Option<Rect>,
    ) {
        Self::paint_stacking_context_guarded(node, list, cull, 0);
    }

    /// Paints a stacking context with optional spatial viewport culling and active recursion depth guard.
    pub fn paint_stacking_context_guarded(
        node: &LayoutNode,
        list: &mut DisplayList,
        cull: Option<Rect>,
        depth: usize,
    ) {
        Self::paint_stacking_context_inner(node, list, cull, cull, &[], depth == 0, depth);
    }

    fn node_clip_scroll_step(node: &LayoutNode) -> Option<ClipScrollStep> {
        let bounds = node.geometry.border_box;
        let border_radius = node.style.border_radius;
        let has_any_radius = border_radius.top_left > 0.0
            || border_radius.top_right > 0.0
            || border_radius.bottom_left > 0.0
            || border_radius.bottom_right > 0.0;
        let clips_overflow = node.style.overflow_x != Overflow::Visible
            || node.style.overflow_y != Overflow::Visible;
        let should_clip = clips_overflow || (has_any_radius && node.image.is_some());
        let clip = if should_clip {
            let clip_bounds = if clips_overflow
                && node.geometry.padding_box.width > 0.0
                && node.geometry.padding_box.height > 0.0
            {
                node.geometry.padding_box
            } else {
                bounds
            };
            Some((clip_bounds, border_radius))
        } else {
            None
        };
        let scroll_offset = (node.geometry.scroll_offset.x, node.geometry.scroll_offset.y);
        let is_fixed_cb = node.style.establishes_fixed_containing_block();
        let is_abs_cb = node.style.position != Position::Static || is_fixed_cb;

        if clip.is_some()
            || scroll_offset.0 != 0.0
            || scroll_offset.1 != 0.0
            || is_abs_cb
            || is_fixed_cb
        {
            Some(ClipScrollStep {
                clip,
                scroll_offset,
                is_abs_cb,
                is_fixed_cb,
            })
        } else {
            None
        }
    }

    #[inline]
    fn effective_steps(position: Position, steps: &[ClipScrollStep]) -> &[ClipScrollStep] {
        match position {
            Position::Fixed => match steps.iter().rposition(|s| s.is_fixed_cb) {
                Some(idx) => &steps[..=idx],
                None => &[],
            },
            Position::Absolute => match steps.iter().rposition(|s| s.is_abs_cb) {
                Some(idx) => &steps[..=idx],
                None => &[],
            },
            Position::Static | Position::Relative | Position::Sticky => steps,
        }
    }

    fn push_clip_scroll_steps(steps: &[ClipScrollStep], list: &mut DisplayList) {
        for step in steps {
            if let Some((bounds, radius)) = step.clip {
                list.push(DrawCommand::PushClip { bounds, radius });
            }
            if step.scroll_offset.0 != 0.0 || step.scroll_offset.1 != 0.0 {
                let mat = strata_primitives::geom::Matrix4x4::translation(
                    -step.scroll_offset.0,
                    -step.scroll_offset.1,
                    0.0,
                );
                list.push(DrawCommand::PushTransform { matrix: mat });
            }
        }
    }

    fn pop_clip_scroll_steps(steps: &[ClipScrollStep], list: &mut DisplayList) {
        for step in steps.iter().rev() {
            if step.scroll_offset.0 != 0.0 || step.scroll_offset.1 != 0.0 {
                list.push(DrawCommand::PopTransform);
            }
            if step.clip.is_some() {
                list.push(DrawCommand::PopClip);
            }
        }
    }

    fn cull_with_steps(base_cull: Option<Rect>, steps: &[ClipScrollStep]) -> Option<Rect> {
        let mut current = base_cull;
        for step in steps {
            if let Some((clip_bounds, _)) = step.clip {
                if clip_bounds.width > 0.0 && clip_bounds.height > 0.0 {
                    if let Some(c) = current {
                        current = Some(
                            clip_bounds
                                .intersect(c)
                                .unwrap_or_else(|| Rect::new(0.0, 0.0, -1.0, -1.0)),
                        );
                    }
                }
            }
        }
        current
    }

    fn subtree_has_fixed_escape(node: &LayoutNode, depth: usize) -> bool {
        if depth >= Self::MAX_COMPOSITOR_DEPTH || node.style.display == Display::None {
            return false;
        }
        if node.style.position == Position::Fixed {
            return true;
        }
        if node.style.establishes_fixed_containing_block() {
            return false;
        }
        node.children
            .iter()
            .any(|c| Self::subtree_has_fixed_escape(c, depth + 1))
    }

    fn collect_hoisted_participants<'a>(
        parent: &'a LayoutNode,
        current_steps: &mut Vec<ClipScrollStep>,
        negative_z: &mut Vec<HoistedParticipant<'a>>,
        auto_or_zero_z: &mut Vec<HoistedParticipant<'a>>,
        positive_z: &mut Vec<HoistedParticipant<'a>>,
        depth: usize,
    ) {
        if depth >= Self::MAX_COMPOSITOR_DEPTH {
            return;
        }
        let has_line_boxes = !parent.line_boxes.is_empty();
        let parent_display = parent.style.display;

        for child in &parent.children {
            if child.style.display == Display::None
                || child.style.content_visibility == strata_style::types::ContentVisibility::Hidden
            {
                continue;
            }
            if has_line_boxes
                && (child.text.is_some()
                    || (matches!(child.style.display, Display::Inline)
                        && !Self::inline_has_paintable_non_text_content(child)))
            {
                continue;
            }

            if child.establishes_stacking_context_in(Some(parent_display), false) {
                let z = child.style.z_index.unwrap_or(0);
                let entry = HoistedParticipant {
                    node: child,
                    is_sc: true,
                    steps: current_steps.clone(),
                };
                if z < 0 {
                    negative_z.push(entry);
                } else if z > 0 {
                    positive_z.push(entry);
                } else {
                    auto_or_zero_z.push(entry);
                }
            } else {
                if child.style.position != Position::Static {
                    auto_or_zero_z.push(HoistedParticipant {
                        node: child,
                        is_sc: false,
                        steps: current_steps.clone(),
                    });
                }
                if !child.children.is_empty() {
                    let step = Self::node_clip_scroll_step(child);
                    let pushed = step.is_some();
                    if let Some(s) = step {
                        current_steps.push(s);
                    }
                    Self::collect_hoisted_participants(
                        child,
                        current_steps,
                        negative_z,
                        auto_or_zero_z,
                        positive_z,
                        depth + 1,
                    );
                    if pushed {
                        current_steps.pop();
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_stacking_context_inner(
        node: &LayoutNode,
        list: &mut DisplayList,
        cull: Option<Rect>,
        root_cull: Option<Rect>,
        inherited_steps: &[ClipScrollStep],
        is_root: bool,
        depth: usize,
    ) {
        if depth >= Self::MAX_COMPOSITOR_DEPTH {
            return;
        }
        if node.style.display == Display::None {
            return;
        }
        if node.style.content_visibility == strata_style::types::ContentVisibility::Hidden {
            return;
        }

        let node_eff_steps = Self::effective_steps(node.style.position, inherited_steps);
        let base_cull = if node.style.position == Position::Fixed
            && !inherited_steps.iter().any(|s| s.is_fixed_cb)
        {
            root_cull
        } else {
            Self::cull_with_steps(cull, node_eff_steps)
        };

        if let Some(c) = base_cull {
            if (c.width < 0.0 || c.height < 0.0) && !Self::subtree_has_fixed_escape(node, depth) {
                return;
            }
        }

        let bounds = node.geometry.border_box;
        let border_radius = node.style.border_radius;
        let has_any_radius = border_radius.top_left > 0.0
            || border_radius.top_right > 0.0
            || border_radius.bottom_left > 0.0
            || border_radius.bottom_right > 0.0;
        let clips_overflow = node.style.overflow_x != Overflow::Visible
            || node.style.overflow_y != Overflow::Visible;
        let should_clip = clips_overflow || (has_any_radius && node.image.is_some());

        let mut static_cull_empty =
            base_cull.is_some_and(|c| c.width < 0.0 || c.height < 0.0);

        let child_cull = if clips_overflow
            && bounds.width > 0.0
            && bounds.height > 0.0
            && node.style.position != Position::Fixed
        {
            match base_cull {
                Some(c) => {
                    if c.width < 0.0 || c.height < 0.0 {
                        Some(c)
                    } else {
                        match bounds.intersect(c) {
                            Some(ic) => Some(ic),
                            None => {
                                if !Self::subtree_has_fixed_escape(node, depth) {
                                    return;
                                }
                                static_cull_empty = true;
                                Some(Rect::new(0.0, 0.0, -1.0, -1.0))
                            }
                        }
                    }
                }
                None => None,
            }
        } else {
            base_cull
        };

        let is_visible = !static_cull_empty
            && match base_cull {
                Some(c) => {
                    if node.style.position == Position::Fixed {
                        true
                    } else if bounds.width > 0.0 && bounds.height > 0.0 {
                        bounds.intersect(c).is_some()
                    } else {
                        true
                    }
                }
                None => true,
            };

        let has_isolation = node.style.isolation == Isolation::Isolate;
        let has_blend_mode = node.style.mix_blend_mode != BlendMode::Normal;
        let has_filter_cfg = node.style.filter.as_ref().is_some_and(|f| !f.is_empty());
        let has_clip_path_cfg = node.style.clip_path.as_ref().is_some_and(|cp| !cp.is_none());
        let transform_matrix = if node.style.has_transform() {
            Some(
                node.style
                    .compute_transform_matrix(bounds.width, bounds.height),
            )
        } else {
            None
        };
        let has_opacity = node.style.opacity < 0.999;
        let has_backdrop_cfg = node
            .style
            .backdrop_filter
            .as_ref()
            .is_some_and(|bf| !bf.is_empty());

        let has_group_effect = has_isolation
            || has_blend_mode
            || has_filter_cfg
            || has_clip_path_cfg
            || transform_matrix.is_some()
            || has_opacity
            || has_backdrop_cfg;

        let wrap_inherited_outside_group = has_group_effect
            && (node.style.establishes_fixed_containing_block()
                || !Self::subtree_has_fixed_escape(node, depth));

        if wrap_inherited_outside_group {
            Self::push_clip_scroll_steps(node_eff_steps, list);
        }

        if has_isolation {
            list.push(DrawCommand::PushIsolation { isolate: true });
        }
        if has_blend_mode {
            list.push(DrawCommand::PushBlendMode {
                mode: node.style.mix_blend_mode,
            });
        }
        let has_filter = if let Some(ref f) = node.style.filter {
            if !f.is_empty() {
                list.push(DrawCommand::PushFilter {
                    bounds,
                    filter: f.clone(),
                });
                true
            } else {
                false
            }
        } else {
            false
        };
        let has_clip_path = if let Some(ref cp) = node.style.clip_path {
            if !cp.is_none() {
                list.push(DrawCommand::PushClipPath {
                    bounds,
                    clip_path: cp.clone(),
                });
                true
            } else {
                false
            }
        } else {
            false
        };
        if let Some(mat) = transform_matrix {
            list.push(DrawCommand::PushTransform { matrix: mat });
        }
        if has_opacity {
            list.push(DrawCommand::PushOpacity {
                alpha: node.style.opacity,
            });
        }
        let has_backdrop_filter = if let Some(ref bf) = node.style.backdrop_filter {
            if !bf.is_empty() {
                list.push(DrawCommand::PushBackdropFilter {
                    bounds,
                    radius: border_radius,
                    filter: bf.clone(),
                });
                true
            } else {
                false
            }
        } else {
            false
        };

        // Build base clip/scroll step chain for hoisted descendants of this stacking context.
        let mut hoisted_base_steps: Vec<ClipScrollStep> = if wrap_inherited_outside_group {
            Vec::new()
        } else {
            node_eff_steps.to_vec()
        };
        let own_clip_bounds = if clips_overflow
            && node.geometry.padding_box.width > 0.0
            && node.geometry.padding_box.height > 0.0
        {
            node.geometry.padding_box
        } else {
            bounds
        };
        let own_is_fixed_cb = node.style.establishes_fixed_containing_block();
        let own_is_abs_cb = node.style.position != Position::Static || own_is_fixed_cb || is_root;
        let has_scroll_offset =
            node.geometry.scroll_offset.x != 0.0 || node.geometry.scroll_offset.y != 0.0;
        let own_clip_opt = if should_clip {
            Some((own_clip_bounds, border_radius))
        } else {
            None
        };
        if own_clip_opt.is_some() || has_scroll_offset || own_is_abs_cb || own_is_fixed_cb {
            hoisted_base_steps.push(ClipScrollStep {
                clip: own_clip_opt,
                scroll_offset: (node.geometry.scroll_offset.x, node.geometry.scroll_offset.y),
                is_abs_cb: own_is_abs_cb,
                is_fixed_cb: own_is_fixed_cb,
            });
        }

        // Collect hoisted stacking participants across non-stacking-context descendants (CSS 2.1 Appendix E)
        let mut negative_z: Vec<HoistedParticipant<'_>> = Vec::new();
        let mut auto_or_zero_z: Vec<HoistedParticipant<'_>> = Vec::new();
        let mut positive_z: Vec<HoistedParticipant<'_>> = Vec::new();
        Self::collect_hoisted_participants(
            node,
            &mut hoisted_base_steps,
            &mut negative_z,
            &mut auto_or_zero_z,
            &mut positive_z,
            depth + 1,
        );

        // Stable ascending z-index sort preserves DOM order on ties
        negative_z.sort_by_key(|p| p.node.style.z_index.unwrap_or(0));
        positive_z.sort_by_key(|p| p.node.style.z_index.unwrap_or(0));

        // Direct in-flow static non-stacking-context children (Layers 3, 4, 5)
        let mut in_flow_blocks: Vec<&LayoutNode> = Vec::new();
        let mut floats: Vec<&LayoutNode> = Vec::new();
        let mut in_flow_inlines: Vec<&LayoutNode> = Vec::new();

        if !static_cull_empty {
            let has_line_boxes = !node.line_boxes.is_empty();
            let parent_display = node.style.display;
            for child in &node.children {
                if child.style.display == Display::None
                    || child.style.content_visibility
                        == strata_style::types::ContentVisibility::Hidden
                {
                    continue;
                }
                if has_line_boxes
                    && (child.text.is_some()
                        || (matches!(child.style.display, Display::Inline)
                            && !Self::inline_has_paintable_non_text_content(child)))
                {
                    continue;
                }
                if child.establishes_stacking_context_in(Some(parent_display), false)
                    || child.style.position != Position::Static
                {
                    continue;
                }
                if let Some(c) = child_cull {
                    if child.geometry.border_box.y > c.y + c.height {
                        continue;
                    }
                    if child.geometry.border_box.height > 0.0
                        && child.geometry.border_box.y + child.geometry.border_box.height < c.y
                    {
                        continue;
                    }
                }
                if child.style.float != strata_style::types::Float::None {
                    floats.push(child);
                } else {
                    match child.style.display {
                        Display::Inline
                        | Display::InlineBlock
                        | Display::InlineFlex
                        | Display::InlineGrid => {
                            in_flow_inlines.push(child);
                        }
                        _ => {
                            in_flow_blocks.push(child);
                        }
                    }
                }
            }
        }

        let inner_inherited_steps: &[ClipScrollStep] = if wrap_inherited_outside_group {
            &[]
        } else {
            node_eff_steps
        };

        // Layer 1: Background and borders of the element establishing the stacking context
        if !inner_inherited_steps.is_empty() {
            Self::push_clip_scroll_steps(inner_inherited_steps, list);
        }
        if is_visible {
            Self::paint_node_box_decorations(node, list);
        }
        if !negative_z.is_empty() && !inner_inherited_steps.is_empty() {
            Self::pop_clip_scroll_steps(inner_inherited_steps, list);
        }

        // Layer 2: Child stacking contexts with negative z-index
        let child_root_cull = if own_is_fixed_cb {
            child_cull
        } else {
            root_cull
        };
        for entry in &negative_z {
            Self::paint_stacking_context_inner(
                entry.node,
                list,
                root_cull,
                child_root_cull,
                &entry.steps,
                false,
                depth + 1,
            );
        }

        if !negative_z.is_empty() && !inner_inherited_steps.is_empty() {
            Self::push_clip_scroll_steps(inner_inherited_steps, list);
        }

        // Own content clip + replaced/text/SVG content + Layers 3..5 (static in-flow descendants)
        if should_clip && is_visible {
            list.push(DrawCommand::PushClip {
                bounds: own_clip_bounds,
                radius: border_radius,
            });
        }
        if is_visible {
            Self::paint_node_foreground_content(node, list, base_cull);
        }

        if has_scroll_offset && !static_cull_empty {
            let mat = strata_primitives::geom::Matrix4x4::translation(
                -node.geometry.scroll_offset.x,
                -node.geometry.scroll_offset.y,
                0.0,
            );
            list.push(DrawCommand::PushTransform { matrix: mat });
        }

        // Layer 3: In-flow, non-inline, non-positioned block descendants
        for child in in_flow_blocks {
            Self::paint_node_content_inner(child, list, child_cull, true, depth + 1);
        }

        // Layer 4: Non-positioned floating descendants
        for child in floats {
            Self::paint_node_content_inner(child, list, child_cull, true, depth + 1);
        }

        // Layer 5: In-flow inline-level descendants
        for child in in_flow_inlines {
            Self::paint_node_content_inner(child, list, child_cull, true, depth + 1);
        }

        // Collapsed table boundaries paint once above cell content.
        if !static_cull_empty {
            Self::paint_collapsed_table_grid(node, list);
        }

        if has_scroll_offset && !static_cull_empty {
            list.push(DrawCommand::PopTransform);
        }
        if should_clip && is_visible {
            list.push(DrawCommand::PopClip);
        }
        if !inner_inherited_steps.is_empty() {
            Self::pop_clip_scroll_steps(inner_inherited_steps, list);
        }

        // Layer 6: Positioned descendants with z-index: auto and child stacking contexts with z-index: 0
        for entry in &auto_or_zero_z {
            if entry.is_sc {
                Self::paint_stacking_context_inner(
                    entry.node,
                    list,
                    root_cull,
                    child_root_cull,
                    &entry.steps,
                    false,
                    depth + 1,
                );
            } else {
                let eff = Self::effective_steps(entry.node.style.position, &entry.steps);
                let entry_cull = Self::cull_with_steps(child_root_cull, eff);
                if entry_cull.is_none_or(|c| c.width >= 0.0 && c.height >= 0.0) {
                    Self::push_clip_scroll_steps(eff, list);
                    Self::paint_node_content_inner(entry.node, list, entry_cull, true, depth + 1);
                    Self::pop_clip_scroll_steps(eff, list);
                }
            }
        }

        // Layer 7: Child stacking contexts with positive z-index
        for entry in &positive_z {
            Self::paint_stacking_context_inner(
                entry.node,
                list,
                root_cull,
                child_root_cull,
                &entry.steps,
                false,
                depth + 1,
            );
        }

        if has_backdrop_filter {
            list.push(DrawCommand::PopBackdropFilter);
        }
        if transform_matrix.is_some() {
            list.push(DrawCommand::PopTransform);
        }
        if has_clip_path {
            list.push(DrawCommand::PopClipPath);
        }
        if has_filter {
            list.push(DrawCommand::PopFilter);
        }
        if has_blend_mode {
            list.push(DrawCommand::PopBlendMode);
        }
        if has_isolation {
            list.push(DrawCommand::PopIsolation);
        }
        if has_opacity {
            list.push(DrawCommand::PopOpacity);
        }
        if wrap_inherited_outside_group {
            Self::pop_clip_scroll_steps(node_eff_steps, list);
        }
    }

    fn paint_node_box_decorations(node: &LayoutNode, list: &mut DisplayList) {
        let bounds = node.geometry.border_box;
        let border_radius = node.style.border_radius;
        let has_any_radius = border_radius.top_left > 0.0
            || border_radius.top_right > 0.0
            || border_radius.bottom_left > 0.0
            || border_radius.bottom_right > 0.0;
        let has_any_border_width = node.style.border_width.top > 0.0
            || node.style.border_width.right > 0.0
            || node.style.border_width.bottom > 0.0
            || node.style.border_width.left > 0.0;
        let draw_border_separately = node.style.border_style != BorderStyle::None
            && node.style.border_style != BorderStyle::Hidden
            && has_any_border_width
            && node.style.border_color.is_some()
            && !(node.style.display == Display::TableCell
                && node.style.border_collapse == BorderCollapse::Collapse);
        let rounded_border_color = if draw_border_separately
            || node.style.border_style == BorderStyle::None
            || node.style.border_style == BorderStyle::Hidden
        {
            None
        } else {
            node.style.border_color
        };

        // Outer Box Shadows (rendered behind background)
        for shadow in &node.style.box_shadow {
            if !shadow.inset {
                list.push(DrawCommand::DrawShadow {
                    bounds,
                    shadow: shadow.clone(),
                    border_radius,
                });
            }
        }

        // Solid / Rounded Background
        if let Some(bg_color) = node.style.background_color {
            if has_any_radius {
                list.push(DrawCommand::DrawRoundedRect {
                    bounds,
                    color: bg_color,
                    border_radius,
                    border_width: node.style.border_width,
                    border_color: rounded_border_color,
                });
            } else if bg_color.a > 0 {
                list.push(DrawCommand::DrawRect {
                    bounds,
                    color: bg_color,
                });
            }
        }

        // Gradient Background
        if let Some(ref grad) = node.style.background_gradient {
            match grad {
                strata_style::types::Gradient::Linear { angle_deg, stops } => {
                    list.push(DrawCommand::DrawLinearGradient {
                        bounds,
                        angle_deg: *angle_deg,
                        stops: stops.clone(),
                        border_radius,
                    });
                }
                strata_style::types::Gradient::Radial { center, stops } => {
                    list.push(DrawCommand::DrawRadialGradient {
                        bounds,
                        center: *center,
                        stops: stops.clone(),
                        border_radius,
                    });
                }
                strata_style::types::Gradient::Conic {
                    angle_deg,
                    center: _,
                    stops,
                } => {
                    list.push(DrawCommand::DrawLinearGradient {
                        bounds,
                        angle_deg: *angle_deg,
                        stops: stops.clone(),
                        border_radius,
                    });
                }
            }
        }

        // Inset Box Shadows (rendered over background)
        for shadow in &node.style.box_shadow {
            if shadow.inset {
                list.push(DrawCommand::DrawInsetShadow {
                    bounds,
                    shadow: shadow.clone(),
                    border_radius,
                });
            }
        }

        // Borders emitted independently so the style parser can resolve
        // solid, dashed, dotted, and double strokes.
        if draw_border_separately {
            if let Some(border_color) = node.style.border_color {
                list.push(DrawCommand::DrawBorder {
                    bounds,
                    widths: node.style.border_width,
                    color: border_color,
                    border_radius,
                    style: node.style.border_style,
                });
            }
        }

        // CSS outlines sit at the outside edge of the border box.
        if node.style.outline_width > 0.0 && node.style.outline_style != BorderStyle::None {
            if let Some(outline_color) = node.style.outline_color {
                let offset = node.style.outline_offset;
                let outline_radius = BorderRadius {
                    top_left: (border_radius.top_left + offset).max(0.0),
                    top_right: (border_radius.top_right + offset).max(0.0),
                    bottom_left: (border_radius.bottom_left + offset).max(0.0),
                    bottom_right: (border_radius.bottom_right + offset).max(0.0),
                };
                list.push(DrawCommand::DrawBorder {
                    bounds: Rect::new(
                        bounds.x - offset,
                        bounds.y - offset,
                        bounds.width + offset * 2.0,
                        bounds.height + offset * 2.0,
                    ),
                    widths: EdgeInsets::all(node.style.outline_width),
                    color: outline_color,
                    border_radius: outline_radius,
                    style: node.style.outline_style,
                });
            }
        }
    }

    fn paint_node_foreground_content(
        node: &LayoutNode,
        list: &mut DisplayList,
        cull: Option<Rect>,
    ) {
        let bounds = node.geometry.border_box;

        // Replaced raster content paints inside the content box.
        if let Some(ref image) = node.image {
            let image_bounds = if node.geometry.content_box.width > 0.0
                && node.geometry.content_box.height > 0.0
            {
                node.geometry.content_box
            } else {
                bounds
            };
            list.push(DrawCommand::DrawImage {
                bounds: image_bounds,
                image: image.clone(),
            });
        }

        // Text content on this node
        if !node.line_boxes.is_empty() {
            let mut fragments = Vec::new();
            for line in &node.line_boxes {
                if let Some(c) = cull {
                    if line.bounds.height > 0.0
                        && (line.bounds.y > c.y + c.height
                            || line.bounds.y + line.bounds.height < c.y)
                    {
                        continue;
                    }
                }
                fragments.extend(line.text_fragments.clone());
            }
            if !fragments.is_empty() {
                list.push(DrawCommand::DrawTextFragments { fragments });
            }
        } else if let Some(ref text) = node.text {
            let font_size = node.style.font_size;
            let line_height = node.style.line_height;
            let font_weight = node.style.font_weight;
            let color = node.style.color;
            let bounds_width = if node.geometry.content_box.width > 0.0 {
                Some(node.geometry.content_box.width)
            } else {
                None
            };
            let bounds_height = if node.geometry.content_box.height > 0.0 {
                Some(node.geometry.content_box.height)
            } else {
                None
            };
            let text_align = node.style.text_align;
            let is_monospace = strata_layout::is_monospace_family(node.style.font_family);
            list.push(DrawCommand::DrawText {
                origin: (node.geometry.content_box.x, node.geometry.content_box.y),
                bounds_width,
                bounds_height,
                text_align,
                text: text.clone(),
                font_size,
                line_height,
                font_weight,
                color,
                is_monospace,
                font_family: node.style.font_family,
            });
        }

        // SVG Vector Path content on this node
        if !node.svg_items.is_empty() {
            for item in &node.svg_items {
                let fill = if item.fill_is_current_color {
                    Some(node.style.color)
                } else {
                    item.fill_color
                };
                let stroke = if item.stroke_is_current_color {
                    Some(node.style.color)
                } else {
                    item.stroke_color.or_else(|| {
                        if fill.is_none() {
                            Some(node.style.color)
                        } else {
                            None
                        }
                    })
                };
                list.push(DrawCommand::DrawSvgPath {
                    bounds,
                    path_data: item.path_data.clone(),
                    stroke_color: stroke,
                    fill_color: fill,
                    stroke_width: item.stroke_width,
                    stroke_linecap: item.stroke_linecap,
                    stroke_linejoin: item.stroke_linejoin,
                    stroke_dasharray: item.stroke_dasharray.clone(),
                    fill_rule: item.fill_rule,
                    view_box: item.view_box,
                });
            }
        } else if let Some(ref path_data) = node.svg_path {
            list.push(DrawCommand::DrawSvgPath {
                bounds,
                path_data: path_data.clone(),
                stroke_color: Some(node.style.color),
                fill_color: None,
                stroke_width: 2.0,
                stroke_linecap: strata_layout::StrokeLineCap::Round,
                stroke_linejoin: strata_layout::StrokeLineJoin::Round,
                stroke_dasharray: Vec::new(),
                fill_rule: strata_layout::FillRule::NonZero,
                view_box: (24.0, 24.0),
            });
        }
    }

    /// Paints an in-flow non-stacking-context node and its recursive children.
    pub fn paint_node_content(node: &LayoutNode, list: &mut DisplayList) {
        Self::paint_node_content_culled(node, list, None);
    }

    /// Paints an in-flow non-stacking-context node with optional spatial viewport culling.
    pub fn paint_node_content_culled(
        node: &LayoutNode,
        list: &mut DisplayList,
        cull: Option<Rect>,
    ) {
        Self::paint_node_content_guarded(node, list, cull, 0);
    }

    /// Paints an in-flow non-stacking-context node with optional spatial viewport culling and active recursion depth guard.
    pub fn paint_node_content_guarded(
        node: &LayoutNode,
        list: &mut DisplayList,
        cull: Option<Rect>,
        depth: usize,
    ) {
        Self::paint_node_content_inner(node, list, cull, false, depth);
    }

    fn paint_node_content_inner(
        node: &LayoutNode,
        list: &mut DisplayList,
        cull: Option<Rect>,
        skip_hoisted: bool,
        depth: usize,
    ) {
        if depth >= Self::MAX_COMPOSITOR_DEPTH {
            return;
        }
        if node.style.display == Display::None {
            return;
        }
        if node.style.content_visibility == strata_style::types::ContentVisibility::Hidden {
            return;
        }

        let bounds = node.geometry.border_box;
        let clips_overflow = node.style.overflow_x != Overflow::Visible
            || node.style.overflow_y != Overflow::Visible;
        let border_radius = node.style.border_radius;
        let has_any_radius = border_radius.top_left > 0.0
            || border_radius.top_right > 0.0
            || border_radius.bottom_left > 0.0
            || border_radius.bottom_right > 0.0;
        let should_clip = clips_overflow || (has_any_radius && node.image.is_some());

        let child_cull = if clips_overflow
            && bounds.width > 0.0
            && bounds.height > 0.0
            && node.style.position != Position::Fixed
        {
            match cull {
                Some(c) => match bounds.intersect(c) {
                    Some(ic) => Some(ic),
                    None => return,
                },
                None => None,
            }
        } else {
            cull
        };

        let is_visible = match cull {
            Some(c) => {
                if node.style.position == Position::Fixed {
                    true
                } else if bounds.width > 0.0 && bounds.height > 0.0 {
                    bounds.intersect(c).is_some()
                } else {
                    true
                }
            }
            None => true,
        };

        if is_visible {
            Self::paint_node_box_decorations(node, list);

            if should_clip {
                let clip_bounds = if clips_overflow
                    && node.geometry.padding_box.width > 0.0
                    && node.geometry.padding_box.height > 0.0
                {
                    node.geometry.padding_box
                } else {
                    bounds
                };
                list.push(DrawCommand::PushClip {
                    bounds: clip_bounds,
                    radius: border_radius,
                });
            }

            Self::paint_node_foreground_content(node, list, cull);
        }

        let has_scroll_offset =
            node.geometry.scroll_offset.x != 0.0 || node.geometry.scroll_offset.y != 0.0;
        if has_scroll_offset {
            let mat = strata_primitives::geom::Matrix4x4::translation(
                -node.geometry.scroll_offset.x,
                -node.geometry.scroll_offset.y,
                0.0,
            );
            list.push(DrawCommand::PushTransform { matrix: mat });
        }

        let has_line_boxes = !node.line_boxes.is_empty();
        let parent_display = node.style.display;
        for child in &node.children {
            if child.style.display == Display::None {
                continue;
            }
            if has_line_boxes
                && (child.text.is_some()
                    || (matches!(child.style.display, Display::Inline)
                        && !Self::inline_has_paintable_non_text_content(child)))
            {
                continue;
            }

            let child_is_sc = child.establishes_stacking_context_in(Some(parent_display), false);
            if skip_hoisted && (child_is_sc || child.style.position != Position::Static) {
                continue;
            }

            // In-flow child culling check: if child is strictly outside the cull rect, skip it
            if let Some(c) = child_cull {
                if child.style.position == Position::Static {
                    if child.geometry.border_box.y > c.y + c.height {
                        continue;
                    }
                    if child.geometry.border_box.height > 0.0
                        && child.geometry.border_box.y + child.geometry.border_box.height < c.y
                    {
                        continue;
                    }
                }
            }

            if child_is_sc {
                Self::paint_stacking_context_guarded(child, list, child_cull, depth + 1);
            } else {
                Self::paint_node_content_inner(child, list, child_cull, skip_hoisted, depth + 1);
            }
        }

        // Collapsed table boundaries paint once above cell content.
        Self::paint_collapsed_table_grid(node, list);

        if has_scroll_offset {
            list.push(DrawCommand::PopTransform);
        }

        if should_clip && is_visible {
            list.push(DrawCommand::PopClip);
        }
    }
}

#[derive(Clone, Copy)]
struct ClipScrollStep {
    clip: Option<(Rect, BorderRadius)>,
    scroll_offset: (f32, f32),
    is_abs_cb: bool,
    is_fixed_cb: bool,
}

struct HoistedParticipant<'a> {
    node: &'a LayoutNode,
    is_sc: bool,
    steps: Vec<ClipScrollStep>,
}
