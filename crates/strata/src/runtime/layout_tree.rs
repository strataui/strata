//! Layout tree construction, blueprint flattening, and UI IR graph synchronization.

use super::*;

impl AppRuntime {
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

    /// Compiles a reactive blueprint tree into a 64-byte cache-aligned UiIrGraph.
    pub fn build_ir_graph(blueprint: &NodeBlueprint) -> strata_dom::UiIrGraph {
        let mut graph = strata_dom::UiIrGraph::new();
        let root_id = blueprint.compile_to_ir(&mut graph);
        graph.root_id = if root_id.is_null() { None } else { Some(root_id) };
        graph.recompute_subtree_spans();
        graph
    }

    /// Synchronizes computed 2D geometry from LayoutNode hierarchy to UiIrGraph FixedRect coordinates.
    pub fn sync_layout_to_ir(layout_root: &LayoutNode, ir_graph: &mut strata_dom::UiIrGraph) {
        if let Some(root_id) = ir_graph.root_id {
            Self::sync_layout_to_ir_recursive(layout_root, ir_graph, root_id);
        }
    }

    pub(crate) fn sync_layout_to_ir_recursive(
        layout_node: &LayoutNode,
        ir_graph: &mut strata_dom::UiIrGraph,
        ir_id: strata_dom::NodeId,
    ) {
        if ir_id.is_null() {
            return;
        }
        if let Some(node) = ir_graph.get_mut(ir_id) {
            node.layout_rect = strata_dom::FixedRect::from_f32(
                layout_node.geometry.border_box.x,
                layout_node.geometry.border_box.y,
                layout_node.geometry.border_box.width,
                layout_node.geometry.border_box.height,
            );
        }

        let first_child = ir_graph
            .get(ir_id)
            .map(|n| n.first_child)
            .unwrap_or(strata_dom::NodeId::NULL);
        let mut curr_ir_child = first_child;
        let mut layout_child_idx = 0;
        while !curr_ir_child.is_null() && layout_child_idx < layout_node.children.len() {
            let next_ir_child = ir_graph
                .get(curr_ir_child)
                .map(|n| n.next_sibling)
                .unwrap_or(strata_dom::NodeId::NULL);
            Self::sync_layout_to_ir_recursive(
                &layout_node.children[layout_child_idx],
                ir_graph,
                curr_ir_child,
            );
            curr_ir_child = next_ir_child;
            layout_child_idx += 1;
        }
    }

    /// Reconstructs a LayoutNode hierarchy directly from a UiIrGraph.
    pub fn build_layout_from_ir(
        graph: &strata_dom::UiIrGraph,
        node_id: strata_dom::NodeId,
    ) -> LayoutNode {
        let ir_node = match graph.get(node_id) {
            Some(n) => n,
            None => return LayoutNode::new(ComputedStyle::default()),
        };

        let mut style = ComputedStyle::default();
        match ir_node.tag {
            strata_dom::IrTag::FlexContainer => {
                style.display = Display::Flex;
            }
            strata_dom::IrTag::GridContainer => {
                style.display = Display::Grid;
            }
            strata_dom::IrTag::Span | strata_dom::IrTag::Text => {
                style.display = Display::Inline;
            }
            _ => {
                style.display = Display::Block;
            }
        }

        let mut children = Vec::with_capacity(ir_node.child_count as usize);
        let mut child_id = ir_node.first_child;
        while !child_id.is_null() {
            let next_sibling = graph
                .get(child_id)
                .map(|n| n.next_sibling)
                .unwrap_or(strata_dom::NodeId::NULL);
            children.push(Self::build_layout_from_ir(graph, child_id));
            child_id = next_sibling;
        }

        let mut layout_node = LayoutNode::with_children(style, children);
        let (x, y, w, h) = ir_node.layout_rect.to_f32();
        if w > 0.0 || h > 0.0 {
            let rect = strata_layout::Rect::new(x, y, w, h);
            layout_node.geometry.border_box = rect;
            layout_node.geometry.content_box = rect;
            layout_node.geometry.padding_box = rect;
            layout_node.geometry.margin_box = rect;
        }
        layout_node
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
        if current_path.len() >= 256 {
            let style = match parent_style {
                Some(p) => ComputedStyle::inherit_from(p),
                None => ComputedStyle::default(),
            };
            return LayoutNode::new(style);
        }
        match blueprint {
            NodeBlueprint::Element {
                tag,
                classes,
                attributes,
                children,
                ..
            } => {
                let is_hovered = hovered_path.is_some_and(|h| h.starts_with(current_path));
                let mut style = match parent_style {
                    Some(p) => ComputedStyle::inherit_from(p),
                    None => ComputedStyle::default(),
                };
                Self::apply_tailwind_classes(&mut style, classes, is_hovered, false, false);

                // 1. Vector SVG handling: extract path data from d attribute or child path
                if matches!(tag, Tag::Svg | Tag::Path) {
                    let mut view_box = (24.0f32, 24.0f32);
                    let mut explicit_svg_w = None;
                    let mut explicit_svg_h = None;
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
                            } else if key == "width" {
                                if let Ok(w) = value.trim_end_matches("px").parse::<f32>() {
                                    explicit_svg_w = Some(w);
                                }
                            } else if key == "height" {
                                if let Ok(h) = value.trim_end_matches("px").parse::<f32>() {
                                    explicit_svg_h = Some(h);
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
                            style.width =
                                Dimension::Px(explicit_svg_w.unwrap_or(if view_box.0 > 24.0 {
                                    view_box.0.min(80.0)
                                } else {
                                    16.0
                                }));
                        }
                        if style.height == Dimension::Auto {
                            style.height =
                                Dimension::Px(explicit_svg_h.unwrap_or(if view_box.1 > 24.0 {
                                    view_box.1.min(80.0)
                                } else {
                                    16.0
                                }));
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
                    let char_w = if is_mono {
                        style.font_size * 0.60
                    } else {
                        style.font_size * 0.55
                    };
                    if style.height == Dimension::Auto {
                        let text_h = style.font_size * style.line_height;
                        style.height =
                            Dimension::Px(text_h + style.padding.top + style.padding.bottom);
                    }
                    if let Some(val) = value_text {
                        let approx_width = (val.len() as f32 * char_w).max(40.0);
                        if style.width == Dimension::Auto && style.flex_grow <= 0.0 {
                            style.width = Dimension::Px(
                                approx_width + style.padding.left + style.padding.right,
                            );
                        }
                        return LayoutNode::with_text(style, val);
                    } else if let Some(ph) = placeholder_text {
                        let approx_width = (ph.len() as f32 * char_w).max(40.0);
                        if style.width == Dimension::Auto && style.flex_grow <= 0.0 {
                            style.width = Dimension::Px(
                                approx_width + style.padding.left + style.padding.right,
                            );
                        }
                        style.color = strata_style::types::Color::new(140, 145, 155, 255);
                        return LayoutNode::with_text(style, ph);
                    } else {
                        if style.width == Dimension::Auto && style.flex_grow <= 0.0 {
                            style.width =
                                Dimension::Px(60.0 + style.padding.left + style.padding.right);
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
                        Tag::H1 | Tag::H2 | Tag::H3 | Tag::H4 | Tag::H5 | Tag::H6 => {
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
                        Tag::Img => {
                            style.display = Display::InlineBlock;
                        }
                        Tag::A => {
                            style.display = Display::Inline;
                            if style.text_decoration == strata_style::types::TextDecoration::None {
                                style.text_decoration =
                                    strata_style::types::TextDecoration::Underline;
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
                            if let Some(decls) =
                                any.downcast_ref::<Vec<strata_style::Declaration>>()
                            {
                                for decl in decls {
                                    strata_style::CascadeSolver::apply_declaration(
                                        &mut style, decl,
                                    );
                                }
                                has_direct_style = true;
                            } else if let Some(cs) =
                                any.downcast_ref::<strata_style::types::ComputedStyle>()
                            {
                                style = cs.clone();
                                has_direct_style = true;
                            }
                        }
                        strata_dom::Attribute::Property { key, value } => {
                            if key.eq_ignore_ascii_case("href") && !value.is_empty() {
                                let val_atom = strata_primitives::atom::intern(value);
                                style
                                    .set_custom_property(strata_primitives::AtomId::HREF, val_atom);
                                if matches!(tag, Tag::A)
                                    && style.color == strata_style::types::Color::new(0, 0, 0, 255)
                                {
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
                            } else if key.eq_ignore_ascii_case("style") && !value.is_empty()
                                && !has_direct_style {
                                    let decls = strata_style::parse_declarations(value);
                                    for decl in &decls {
                                        strata_style::CascadeSolver::apply_declaration(
                                            &mut style, decl,
                                        );
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

                // Replaced raster images: decode a base64 PNG data URI and carry
                // its intrinsic dimensions on the layout node for painting.
                if matches!(tag, Tag::Img) {
                    let mut src_value = None;
                    for attr in attributes {
                        if let strata_dom::Attribute::Property { key, value } = attr {
                            if key.eq_ignore_ascii_case("src") && !value.is_empty() {
                                src_value = Some(value.as_str());
                            }
                        }
                    }
                    if let Some(src) = src_value {
                        if let Some(payload) = data_uri_base64_payload(src) {
                            if let Some(decoded) = decode_base64(payload) {
                                if let Some((width, height)) = png_dimensions(&decoded) {
                                    if style.width == Dimension::Auto {
                                        style.width = Dimension::Px(width as f32);
                                    }
                                    if style.height == Dimension::Auto {
                                        style.height = Dimension::Px(height as f32);
                                    }
                                    if style.display == Display::Inline {
                                        style.display = Display::InlineBlock;
                                    }
                                    let image = strata_layout::ImageResource {
                                        data: std::sync::Arc::new(decoded),
                                        width,
                                        height,
                                    };
                                    return LayoutNode::with_image(style, image);
                                }
                            }
                        }
                    }
                }

                // 3D Canvas Viewport handling: Canvas3D is a spatial viewport leaf in the 2D layout tree
                if matches!(tag, Tag::Canvas) || matches!(tag, Tag::Custom("Canvas3D")) {
                    for attr in attributes {
                        if let strata_dom::Attribute::Property { key, value } = attr {
                            if key == "width" && style.width == Dimension::Auto {
                                if let Ok(w) = value.trim_end_matches("px").parse::<f32>() {
                                    style.width = Dimension::Px(w);
                                }
                            } else if key == "height" && style.height == Dimension::Auto {
                                if let Ok(h) = value.trim_end_matches("px").parse::<f32>() {
                                    style.height = Dimension::Px(h);
                                }
                            }
                        }
                    }
                    let is_3d_canvas = attributes.iter().any(|a| {
                        matches!(a, strata_dom::Attribute::Property { key, value } if key == "data-strata-canvas3d" && value == "true")
                    }) || children.iter().any(|c| match c {
                        NodeBlueprint::Element { tag: Tag::Custom(name), .. } => {
                            matches!(*name, "PerspectiveCamera" | "OrthographicCamera" | "AmbientLight" | "DirectionalLight" | "PointLight" | "Mesh" | "BoxGeometry" | "SphereGeometry" | "PlaneGeometry" | "CylinderGeometry" | "StandardMaterial" | "ShaderMaterial")
                        }
                        _ => false,
                    });
                    if is_3d_canvas {
                        return LayoutNode::new(style);
                    }
                }

                let is_inline = matches!(
                    style.display,
                    Display::Inline | Display::InlineFlex | Display::InlineBlock
                );

                // If inline element has auto width and contains only a text child, compute intrinsic text dimensions
                if is_inline && style.width == Dimension::Auto && children.len() == 1 {
                    if let NodeBlueprint::Text(t) = &children[0] {
                        let is_mono = strata_layout::is_monospace_family(style.font_family);
                        let (text_w, raw_h) =
                            strata_layout::measure_text_bounds_styled_with_family(
                                t,
                                style.font_size,
                                style.line_height,
                                None,
                                is_mono,
                                style.font_family,
                                style.font_weight,
                                style.font_style,
                            );
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
                let (w, h) = strata_layout::measure_text_bounds_styled_with_family(
                    text,
                    style.font_size,
                    style.line_height,
                    None,
                    is_mono,
                    style.font_family,
                    style.font_weight,
                    style.font_style,
                );
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
                let (w, h) = strata_layout::measure_text_bounds_styled_with_family(
                    &text,
                    style.font_size,
                    style.line_height,
                    None,
                    is_mono,
                    style.font_family,
                    style.font_weight,
                    style.font_style,
                );
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

    /// Recursively flattens Fragment blueprints so child elements splice directly into parent layout contexts.
    pub(crate) fn append_child_layout_nodes_with_context(
        blueprint: &NodeBlueprint,
        parent_style: Option<&ComputedStyle>,
        parent_path: &mut Vec<usize>,
        child_counter: &mut usize,
        hovered_path: Option<&[usize]>,
        out: &mut Vec<LayoutNode>,
    ) {
        if parent_path.len() >= 256 {
            return;
        }
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

    /// Iteratively flattens Fragment blueprints for linear child index alignment without call-stack recursion.
    pub(crate) fn flatten_blueprints<'a>(bp: &'a NodeBlueprint, out: &mut Vec<&'a NodeBlueprint>) {
        let mut stack = vec![bp];
        while let Some(current) = stack.pop() {
            if let NodeBlueprint::Fragment(children) = current {
                for c in children.iter().rev() {
                    stack.push(c);
                }
            } else {
                out.push(current);
            }
        }
    }

    /// Re-evaluates styling with the current hover path, recomputes layout, and compiles display list.
    pub fn rebuild_layout_tree(&mut self) {
        self.applied_scroll_offsets.clear();
        self.layout_root = Self::build_layout_tree_with_hover(
            &self.root_blueprint,
            self.current_hover_path.as_deref(),
        );
        self.sync_active_input_layout();
        let compute_start = std::time::Instant::now();
        self.layout_root
            .compute_layout(self.viewport_width, self.viewport_height);
        Self::sync_layout_to_ir(&self.layout_root, &mut self.ir_graph);
        self.layout_computations += 1;
        if perf_logging_enabled() {
            perf_log_ms(
                "compute_layout_rebuild",
                compute_start,
                format!(
                    "{} layout nodes",
                    perf_count_layout_nodes(&self.layout_root)
                ),
            );
        }
        self.apply_viewport_clamps();
        self.apply_scroll_offsets();
        self.compile_display_list();
    }

    /// Number of full layout computations performed by this runtime.
    pub fn layout_compute_count(&self) -> u64 {
        self.layout_computations
    }

    /// Locates a mutable LayoutNode in the layout hierarchy by its hierarchical tree path.
    pub fn find_layout_node_mut<'a>(
        root: &'a mut LayoutNode,
        path: &[usize],
    ) -> Option<&'a mut LayoutNode> {
        let mut cur = root;
        for &idx in path {
            if idx >= cur.children.len() {
                return None;
            }
            cur = &mut cur.children[idx];
        }
        Some(cur)
    }

    /// Recursively collects dynamic text node bindings with their exact hierarchical paths.
    pub fn collect_dynamic_text_bindings(
        blueprint: &NodeBlueprint,
        parent_path: &mut Vec<usize>,
        bindings: &mut Vec<DynamicTextBinding>,
    ) {
        match blueprint {
            NodeBlueprint::Element { children, .. } => {
                let mut child_counter = 0;
                for child in children {
                    Self::collect_child_dynamic_text_bindings(
                        child,
                        parent_path,
                        &mut child_counter,
                        bindings,
                    );
                }
            }
            NodeBlueprint::Fragment(children) => {
                let mut child_counter = 0;
                for child in children {
                    Self::collect_child_dynamic_text_bindings(
                        child,
                        parent_path,
                        &mut child_counter,
                        bindings,
                    );
                }
            }
            NodeBlueprint::DynamicText(f) => {
                let val = f();
                bindings.push(DynamicTextBinding {
                    path: parent_path.clone(),
                    closure: f.clone(),
                    last_value: val,
                });
            }
            _ => {}
        }
    }

    fn collect_child_dynamic_text_bindings(
        blueprint: &NodeBlueprint,
        parent_path: &mut Vec<usize>,
        child_counter: &mut usize,
        bindings: &mut Vec<DynamicTextBinding>,
    ) {
        if let NodeBlueprint::Fragment(children) = blueprint {
            for child in children {
                Self::collect_child_dynamic_text_bindings(
                    child,
                    parent_path,
                    child_counter,
                    bindings,
                );
            }
        } else {
            let this_idx = *child_counter;
            *child_counter += 1;
            parent_path.push(this_idx);
            Self::collect_dynamic_text_bindings(blueprint, parent_path, bindings);
            parent_path.pop();
        }
    }
}
