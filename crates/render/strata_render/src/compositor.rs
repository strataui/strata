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
            Self::DrawShadow { bounds, shadow } => {
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
        Self::paint_stacking_context_culled(root, &mut list, cull_rect);
        list
    }

    /// Determines if an element establishes a new Stacking Context.
    pub fn establishes_stacking_context(node: &LayoutNode, is_root: bool) -> bool {
        if is_root {
            return true;
        }
        let is_positioned = node.style.position != Position::Static;
        let has_z_index = node.style.z_index.is_some();
        let has_opacity = node.style.opacity < 0.999;
        let clips_overflow = node.style.overflow_x != Overflow::Visible
            || node.style.overflow_y != Overflow::Visible;
        let has_transform = node.style.has_transform();
        let has_effects = node.style.has_visual_effects();

        (is_positioned && has_z_index)
            || has_opacity
            || clips_overflow
            || has_transform
            || has_effects
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
        if node.style.display == Display::None {
            return;
        }

        if node.style.content_visibility == strata_style::types::ContentVisibility::Hidden {
            return;
        }

        let bounds = node.geometry.border_box;
        let border_radius = node.style.border_radius;
        let has_clip = border_radius.top_left > 0.0
            || border_radius.top_right > 0.0
            || border_radius.bottom_left > 0.0
            || border_radius.bottom_right > 0.0;
        let clips_overflow = node.style.overflow_x != Overflow::Visible
            || node.style.overflow_y != Overflow::Visible;
        let should_clip = has_clip || clips_overflow;

        // Resolve the child cull rect before emitting any stack push commands so
        // that an early return never leaves an unbalanced push on the display
        // list (Cairo groups, blend modes, filters, transforms).
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

        let has_isolation = node.style.isolation == Isolation::Isolate;
        if has_isolation {
            list.push(DrawCommand::PushIsolation { isolate: true });
        }

        let has_blend_mode = node.style.mix_blend_mode != BlendMode::Normal;
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

        let transform_matrix = if node.style.has_transform() {
            Some(
                node.style
                    .compute_transform_matrix(bounds.width, bounds.height),
            )
        } else {
            None
        };

        if let Some(mat) = transform_matrix {
            list.push(DrawCommand::PushTransform { matrix: mat });
        }

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

        let has_opacity = node.style.opacity < 0.999;
        if has_opacity {
            list.push(DrawCommand::PushOpacity {
                alpha: node.style.opacity,
            });
        }

        // Backdrop filter effect on pixels underneath this stacking context
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

        // Layer 1: Background and borders of the element establishing the context
        if is_visible {
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
            let rounded_border_color =
                if draw_border_separately
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
                    });
                }
            }

            // Solid / Rounded Background
            if let Some(bg_color) = node.style.background_color {
                if has_clip {
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

            // Apply clip for content and children if border radius or overflow clipping is active
            if should_clip {
                list.push(DrawCommand::PushClip {
                    bounds,
                    radius: border_radius,
                });
            }

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
                    let stroke = item.stroke_color.or_else(|| {
                        if item.fill_color.is_none() {
                            Some(node.style.color)
                        } else {
                            None
                        }
                    });
                    list.push(DrawCommand::DrawSvgPath {
                        bounds,
                        path_data: item.path_data.clone(),
                        stroke_color: stroke,
                        fill_color: item.fill_color,
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
        } // end if is_visible

        // Separate child nodes into the 7 layers
        let mut negative_z: Vec<&LayoutNode> = Vec::new();
        let mut in_flow_blocks: Vec<&LayoutNode> = Vec::new();
        let mut floats: Vec<&LayoutNode> = Vec::new();
        let mut in_flow_inlines: Vec<&LayoutNode> = Vec::new();
        let mut auto_or_zero_z: Vec<&LayoutNode> = Vec::new();
        let mut positive_z: Vec<&LayoutNode> = Vec::new();

        let has_line_boxes = !node.line_boxes.is_empty();
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

            // Spatial culling optimization: if in-flow child is strictly outside the viewport buffer, skip it
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

            if Self::establishes_stacking_context(child, false) {
                let z = child.style.z_index.unwrap_or(0);
                if z < 0 {
                    negative_z.push(child);
                } else if z > 0 {
                    positive_z.push(child);
                } else {
                    auto_or_zero_z.push(child);
                }
            } else if child.style.float != strata_style::types::Float::None {
                floats.push(child);
            } else {
                // In-flow children without independent stacking context
                match child.style.display {
                    Display::Inline
                    | Display::InlineBlock
                    | Display::InlineFlex
                    | Display::InlineGrid => {
                        in_flow_inlines.push(child);
                    }
                    _ => {
                        if child.style.position == Position::Static {
                            in_flow_blocks.push(child);
                        } else {
                            auto_or_zero_z.push(child);
                        }
                    }
                }
            }
        }

        // Sort negative z-index ascending (e.g. -10, then -5)
        negative_z.sort_by_key(|n| n.style.z_index.unwrap_or(0));

        // Sort positive z-index ascending (e.g. 1, then 10)
        positive_z.sort_by_key(|n| n.style.z_index.unwrap_or(0));

        // Layer 2: Child stacking contexts with negative z-index
        for child in negative_z {
            Self::paint_stacking_context_culled(child, list, child_cull);
        }

        // Layer 3: In-flow, non-inline, non-positioned block descendants
        for child in in_flow_blocks {
            Self::paint_node_content_culled(child, list, child_cull);
        }

        // Layer 4: Non-positioned floating descendants
        for child in floats {
            Self::paint_node_content_culled(child, list, child_cull);
        }

        // Layer 5: In-flow inline-level descendants
        for child in in_flow_inlines {
            Self::paint_node_content_culled(child, list, child_cull);
        }

        // Layer 6: Positioned descendants with z-index: auto or 0
        for child in auto_or_zero_z {
            Self::paint_stacking_context_culled(child, list, child_cull);
        }

        // Layer 7: Child stacking contexts with positive z-index
        for child in positive_z {
            Self::paint_stacking_context_culled(child, list, child_cull);
        }

        // Collapsed table boundaries paint once above cell content.
        Self::paint_collapsed_table_grid(node, list);

        if should_clip && is_visible {
            list.push(DrawCommand::PopClip);
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
        let has_clip = border_radius.top_left > 0.0;
        let should_clip = has_clip || clips_overflow;

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
            let rounded_border_color =
                if draw_border_separately
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
                    });
                }
            }

            // Solid / Rounded Background
            if let Some(bg_color) = node.style.background_color {
                if has_clip {
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

            // Border
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

            if should_clip {
                list.push(DrawCommand::PushClip {
                    bounds,
                    radius: border_radius,
                });
            }

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

            // Text content
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

            // SVG Vector Path content
            if !node.svg_items.is_empty() {
                for item in &node.svg_items {
                    let stroke = item.stroke_color.or_else(|| {
                        if item.fill_color.is_none() {
                            Some(node.style.color)
                        } else {
                            None
                        }
                    });
                    list.push(DrawCommand::DrawSvgPath {
                        bounds,
                        path_data: item.path_data.clone(),
                        stroke_color: stroke,
                        fill_color: item.fill_color,
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

        // Children
        let has_line_boxes = !node.line_boxes.is_empty();
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

            if Self::establishes_stacking_context(child, false) {
                Self::paint_stacking_context_culled(child, list, child_cull);
            } else {
                Self::paint_node_content_culled(child, list, child_cull);
            }
        }

        // Collapsed table boundaries paint once above cell content.
        Self::paint_collapsed_table_grid(node, list);

        if should_clip && is_visible {
            list.push(DrawCommand::PopClip);
        }
    }
}
