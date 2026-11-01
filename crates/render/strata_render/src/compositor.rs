//! CSS 2.1 Appendix E 7-Layer Stacking Context Compositor & Display List Generator.

use crate::scene3d::Scene3D;
use strata_layout::{LayoutNode, Rect};
use strata_primitives::geom::EdgeInsets;
use strata_style::types::{BorderRadius, BoxShadow, Color, Display, FontWeight, Position};

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
    DrawBorder {
        bounds: Rect,
        widths: EdgeInsets<f32>,
        color: Color,
        border_radius: BorderRadius,
    },
    DrawText {
        origin: (f32, f32),
        text: String,
        font_size: f32,
        line_height: f32,
        font_weight: FontWeight,
        color: Color,
    },
    DrawViewport3D {
        bounds: Rect,
        scene: Scene3D,
    },
    DrawCanvas2D {
        bounds: Rect,
        commands: Vec<crate::canvas2d::CanvasCommand>,
    },
    PushClip {
        bounds: Rect,
        radius: BorderRadius,
    },
    PopClip,
}

impl DrawCommand {
    /// Returns true if this command is a clipping push or pop instruction.
    #[inline]
    pub fn is_clip(&self) -> bool {
        matches!(self, Self::PushClip { .. } | Self::PopClip)
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
            Self::DrawViewport3D { bounds, .. } => Some(*bounds),
            Self::DrawCanvas2D { bounds, .. } => Some(*bounds),
            Self::PushClip { bounds, .. } => Some(*bounds),
            Self::PopClip => None,
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
        let mut list = DisplayList::new();
        Self::paint_stacking_context(root, &mut list);
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

        (is_positioned && has_z_index) || has_opacity
    }

    /// Paints a stacking context according to the 7-layer CSS 2.1 Appendix E algorithm.
    pub fn paint_stacking_context(node: &LayoutNode, list: &mut DisplayList) {
        if node.style.display == Display::None {
            return;
        }

        let bounds = node.geometry.border_box;
        let border_radius = node.style.border_radius;
        let has_clip = border_radius.top_left > 0.0
            || border_radius.top_right > 0.0
            || border_radius.bottom_left > 0.0
            || border_radius.bottom_right > 0.0;

        // Layer 1: Background and borders of the element establishing the context
        // Box Shadows (rendered behind background)
        for shadow in &node.style.box_shadow {
            list.push(DrawCommand::DrawShadow {
                bounds,
                shadow: shadow.clone(),
            });
        }

        // Solid / Rounded Background
        if let Some(bg_color) = node.style.background_color {
            if has_clip {
                list.push(DrawCommand::DrawRoundedRect {
                    bounds,
                    color: bg_color,
                    border_radius,
                    border_width: node.style.border_width,
                    border_color: node.style.border_color,
                });
            } else {
                list.push(DrawCommand::DrawRect {
                    bounds,
                    color: bg_color,
                });
            }
        }

        // Borders if not already painted by DrawRoundedRect
        if !has_clip {
            if let Some(border_color) = node.style.border_color {
                if node.style.border_width.top > 0.0
                    || node.style.border_width.right > 0.0
                    || node.style.border_width.bottom > 0.0
                    || node.style.border_width.left > 0.0
                {
                    list.push(DrawCommand::DrawBorder {
                        bounds,
                        widths: node.style.border_width,
                        color: border_color,
                        border_radius,
                    });
                }
            }
        }

        // Text content on this node
        if let Some(ref text) = node.text {
            let font_size = node.style.font_size;
            let line_height = node.style.line_height;
            let font_weight = node.style.font_weight;
            let color = node.style.color;
            list.push(DrawCommand::DrawText {
                origin: (node.geometry.content_box.x, node.geometry.content_box.y),
                text: text.clone(),
                font_size,
                line_height,
                font_weight,
                color,
            });
        }

        // Apply clip for children if border radius is present
        if has_clip {
            list.push(DrawCommand::PushClip {
                bounds,
                radius: border_radius,
            });
        }

        // Separate child nodes into the 7 layers
        let mut negative_z: Vec<&LayoutNode> = Vec::new();
        let mut in_flow_blocks: Vec<&LayoutNode> = Vec::new();
        let mut in_flow_inlines: Vec<&LayoutNode> = Vec::new();
        let mut auto_or_zero_z: Vec<&LayoutNode> = Vec::new();
        let mut positive_z: Vec<&LayoutNode> = Vec::new();

        for child in &node.children {
            if child.style.display == Display::None {
                continue;
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
            } else {
                // In-flow children without independent stacking context
                match child.style.display {
                    Display::Inline | Display::InlineBlock | Display::InlineFlex | Display::InlineGrid => {
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
            Self::paint_stacking_context(child, list);
        }

        // Layer 3: In-flow, non-inline, non-positioned block descendants
        for child in in_flow_blocks {
            Self::paint_node_content(child, list);
        }

        // Layer 4: Floats (skipped in modern layout)

        // Layer 5: In-flow inline-level descendants
        for child in in_flow_inlines {
            Self::paint_node_content(child, list);
        }

        // Layer 6: Positioned descendants with z-index: auto or 0
        for child in auto_or_zero_z {
            Self::paint_stacking_context(child, list);
        }

        // Layer 7: Child stacking contexts with positive z-index
        for child in positive_z {
            Self::paint_stacking_context(child, list);
        }

        if has_clip {
            list.push(DrawCommand::PopClip);
        }
    }

    /// Paints an in-flow non-stacking-context node and its recursive children.
    fn paint_node_content(node: &LayoutNode, list: &mut DisplayList) {
        if node.style.display == Display::None {
            return;
        }

        let bounds = node.geometry.border_box;
        let border_radius = node.style.border_radius;
        let has_clip = border_radius.top_left > 0.0;

        // Background
        if let Some(bg_color) = node.style.background_color {
            if has_clip {
                list.push(DrawCommand::DrawRoundedRect {
                    bounds,
                    color: bg_color,
                    border_radius,
                    border_width: node.style.border_width,
                    border_color: node.style.border_color,
                });
            } else {
                list.push(DrawCommand::DrawRect {
                    bounds,
                    color: bg_color,
                });
            }
        }

        // Border
        if !has_clip {
            if let Some(border_color) = node.style.border_color {
                if node.style.border_width.top > 0.0
                    || node.style.border_width.right > 0.0
                    || node.style.border_width.bottom > 0.0
                    || node.style.border_width.left > 0.0
                {
                    list.push(DrawCommand::DrawBorder {
                        bounds,
                        widths: node.style.border_width,
                        color: border_color,
                        border_radius,
                    });
                }
            }
        }

        // Text content
        if let Some(ref text) = node.text {
            let font_size = node.style.font_size;
            let line_height = node.style.line_height;
            let font_weight = node.style.font_weight;
            let color = node.style.color;
            list.push(DrawCommand::DrawText {
                origin: (node.geometry.content_box.x, node.geometry.content_box.y),
                text: text.clone(),
                font_size,
                line_height,
                font_weight,
                color,
            });
        }

        // Children
        for child in &node.children {
            if Self::establishes_stacking_context(child, false) {
                Self::paint_stacking_context(child, list);
            } else {
                Self::paint_node_content(child, list);
            }
        }
    }
}
