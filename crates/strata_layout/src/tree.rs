//! Layout tree node definition and top-level formatting context dispatcher.

use crate::box_model::solve_block;
use crate::flexbox::solve_flexbox;
use crate::geometry::LayoutGeometry;
use crate::grid::solve_grid;
use crate::positioning::solve_positioning;
use strata_style::types::{ComputedStyle, Display};

/// A node participating in the layout tree.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutNode {
    pub style: ComputedStyle,
    pub geometry: LayoutGeometry,
    pub children: Vec<LayoutNode>,
    pub text: Option<String>,
}

impl LayoutNode {
    /// Constructs a leaf layout node with specified computed styles.
    pub fn new(style: ComputedStyle) -> Self {
        Self {
            style,
            geometry: LayoutGeometry::default(),
            children: Vec::new(),
            text: None,
        }
    }

    /// Constructs a layout node with child nodes.
    pub fn with_children(style: ComputedStyle, children: Vec<LayoutNode>) -> Self {
        Self {
            style,
            geometry: LayoutGeometry::default(),
            children,
            text: None,
        }
    }

    /// Constructs a text leaf node with computed styles and string content.
    pub fn with_text(style: ComputedStyle, text: String) -> Self {
        Self {
            style,
            geometry: LayoutGeometry::default(),
            children: Vec::new(),
            text: Some(text),
        }
    }

    /// Translates this node's geometry and all descendants recursively by (dx, dy).
    pub fn translate(&mut self, dx: f32, dy: f32) {
        self.geometry.translate(dx, dy);
        for child in &mut self.children {
            child.translate(dx, dy);
        }
    }

    /// Formats the children of this node within its already-resolved content box.
    pub fn layout_children(&mut self) {
        if self.children.is_empty() {
            return;
        }

        let content_w = self.geometry.content_box.width;
        let content_h = self.geometry.content_box.height;
        let preserved_geom = self.geometry.clone();

        let container_avail_w = preserved_geom.border_box.width + self.style.margin.left + self.style.margin.right;
        let container_avail_h = preserved_geom.border_box.height + self.style.margin.top + self.style.margin.bottom;

        match self.style.display {
            Display::Flex | Display::InlineFlex => {
                solve_flexbox(self, container_avail_w, container_avail_h);
            }
            Display::Grid | Display::InlineGrid => {
                solve_grid(self, container_avail_w, container_avail_h);
            }
            Display::Block | Display::Inline | Display::InlineBlock => {
                crate::box_model::solve_block_children(self, content_w, content_h);
            }
            Display::None => {}
        }

        let final_h = if self.style.height == strata_style::types::Dimension::Auto {
            self.geometry.border_box.height.max(preserved_geom.border_box.height)
        } else {
            preserved_geom.border_box.height
        };

        self.geometry = LayoutGeometry::from_border_box(
            crate::geometry::Rect::new(
                preserved_geom.border_box.x,
                preserved_geom.border_box.y,
                preserved_geom.border_box.width,
                final_h,
            ),
            self.style.padding,
            self.style.border_width,
            self.style.margin,
        );

        solve_positioning(self, content_w, content_h);
        self.geometry.snap_to_pixel();
    }

    /// Computes layout geometry for this node and all descendants given available bounds.
    pub fn compute_layout(&mut self, available_width: f32, available_height: f32) {
        if self.style.display == Display::None {
            self.geometry = LayoutGeometry::default();
            return;
        }

        // Dispatch according to container display formatting context
        match self.style.display {
            Display::Flex | Display::InlineFlex => {
                solve_flexbox(self, available_width, available_height);
            }
            Display::Grid | Display::InlineGrid => {
                solve_grid(self, available_width, available_height);
            }
            Display::Block | Display::Inline | Display::InlineBlock => {
                solve_block(self, available_width, available_height);
            }
            Display::None => unreachable!(),
        }

        // Resolve relative and out-of-flow absolute/fixed positioned children
        solve_positioning(self, available_width, available_height);

        // Subpixel snapping
        self.geometry.snap_to_pixel();
    }
}
