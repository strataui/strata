//! Layout tree node definition and top-level formatting context dispatcher.

use crate::flexbox::solve_flexbox;
use crate::geometry::LayoutGeometry;
use crate::grid::solve_grid;
use crate::positioning::solve_positioning;
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
    pub inline_items: Vec<InlineItem>,
    pub line_boxes: Vec<LineBox>,
    pub layout_cache: LayoutCacheEntry,
    /// Content-box size used the last time this node formatted its children.
    ///
    /// Multi-pass flex layout can stretch or shrink an item's border box after a
    /// previous pass formatted its descendants. This records the content size
    /// the descendants were actually laid out for, so the final placement pass
    /// can detect stale child geometry and re-run `layout_children` even when
    /// the border box itself did not change.
    pub last_child_layout_content: Option<(f32, f32)>,
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
            inline_items: Vec::new(),
            line_boxes: Vec::new(),
            layout_cache: LayoutCacheEntry::invalid(),
            last_child_layout_content: None,
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
            inline_items: Vec::new(),
            line_boxes: Vec::new(),
            layout_cache: LayoutCacheEntry::invalid(),
            last_child_layout_content: None,
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
            inline_items: Vec::new(),
            line_boxes: Vec::new(),
            layout_cache: LayoutCacheEntry::invalid(),
            last_child_layout_content: None,
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
            inline_items,
            line_boxes: Vec::new(),
            layout_cache: LayoutCacheEntry::invalid(),
            last_child_layout_content: None,
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
                stroke_width: 2.0,
                stroke_linecap: StrokeLineCap::Round,
                stroke_linejoin: StrokeLineJoin::Round,
                stroke_dasharray: Vec::new(),
                fill_rule: FillRule::NonZero,
                view_box: (24.0, 24.0),
            }],
            image: None,
            inline_items: Vec::new(),
            line_boxes: Vec::new(),
            layout_cache: LayoutCacheEntry::invalid(),
            last_child_layout_content: None,
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
            inline_items: Vec::new(),
            line_boxes: Vec::new(),
            layout_cache: LayoutCacheEntry::invalid(),
            last_child_layout_content: None,
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
            inline_items: Vec::new(),
            line_boxes: Vec::new(),
            layout_cache: LayoutCacheEntry::invalid(),
            last_child_layout_content: None,
        }
    }

    /// Appends a child layout node.
    pub fn add_child(&mut self, child: LayoutNode) {
        self.children.push(child);
    }

    /// Translates this node's geometry and all descendants recursively by (dx, dy).
    pub fn translate(&mut self, dx: f32, dy: f32) {
        self.geometry.translate(dx, dy);
        self.layout_cache.origin_x += dx;
        self.layout_cache.origin_y += dy;
        for line in &mut self.line_boxes {
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

        let container_avail_w =
            preserved_geom.border_box.width + self.style.margin.left + self.style.margin.right;
        let container_avail_h =
            preserved_geom.border_box.height + self.style.margin.top + self.style.margin.bottom;

        match self.style.display {
            Display::Flex | Display::InlineFlex => {
                // An absolutely positioned flex container whose height is
                // derived from `top`/`bottom` insets already has its final
                // border box here. Treat that inner cross size as definite so
                // the single flex line can distribute cross-axis free space.
                let inset_stretched = matches!(
                    self.style.position,
                    strata_style::types::Position::Absolute | strata_style::types::Position::Fixed
                ) && self.style.height
                    == strata_style::types::Dimension::Auto
                    && !matches!(self.style.insets.top, strata_style::types::Dimension::Auto)
                    && !matches!(
                        self.style.insets.bottom,
                        strata_style::types::Dimension::Auto
                    )
                    && content_h > 0.0;
                let definite_cross = if inset_stretched {
                    Some(content_h)
                } else {
                    None
                };
                crate::flexbox::solve_flexbox_with_definite_cross(
                    self,
                    container_avail_w,
                    container_avail_h,
                    definite_cross,
                );
            }
            Display::Grid | Display::InlineGrid => {
                solve_grid(self, container_avail_w, container_avail_h);
            }
            Display::Table | Display::InlineTable => {
                crate::table::solve_table(self, container_avail_w, container_avail_h);
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
                crate::box_model::solve_block_children(self, content_w, content_h);
            }
            Display::None => {}
        }

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

        if self.style.position != strata_style::types::Position::Static {
            solve_positioning(self, content_w, content_h);
        }
        self.geometry.snap_to_pixel();
        self.last_child_layout_content = Some((content_w, content_h));
        self.layout_cache.is_valid = false;
    }

    /// Computes layout geometry for this node and all descendants given available bounds.
    pub fn compute_layout(&mut self, available_width: f32, available_height: f32) {
        let session_root = crate::intrinsic::begin_layout_session();
        let _guard = crate::prof::LayoutGuard::new();
        self.compute_layout_with_exclusions(available_width, available_height, None, 0.0);
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
        let _guard = crate::prof::LayoutGuard::new();
        crate::prof::observe_width(self.children.len());
        if self.style.display == Display::None {
            self.geometry = LayoutGeometry::default();
            self.layout_cache.is_valid = false;
            return;
        }

        let is_unconstrained_root = exclusions.is_none();

        // Subtree layout cache hit: skip re-traversal if available dimensions are unchanged
        let height_independent = self.style.height == strata_style::types::Dimension::Auto
            && !matches!(
                self.style.min_height,
                strata_style::types::Dimension::Percent(_)
            )
            && !matches!(
                self.style.max_height,
                strata_style::types::Dimension::Percent(_)
            );
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
                solve_flexbox(self, available_width, available_height);
            }
            Display::Grid | Display::InlineGrid => {
                solve_grid(self, available_width, available_height);
            }
            Display::Table | Display::InlineTable => {
                crate::table::solve_table(self, available_width, available_height);
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
                crate::box_model::solve_block_with_exclusions(
                    self,
                    available_width,
                    available_height,
                    exclusions,
                    bfc_offset_y,
                );
            }
            Display::None => unreachable!(),
        }

        // Resolve relative and out-of-flow absolute/fixed positioned children
        solve_positioning(self, available_width, available_height);

        // Subpixel snapping
        self.geometry.snap_to_pixel();

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

    /// Invalidates layout cache on this node and all descendants.
    pub fn invalidate_cache(&mut self) {
        self.layout_cache.is_valid = false;
        for child in &mut self.children {
            child.invalidate_cache();
        }
    }
}
