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
        graph.root_id = if root_id.is_null() {
            None
        } else {
            Some(root_id)
        };
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

    /// Maps a DOM `Tag` to its canonical lowercase HTML element name for CSS selector matching.
    pub(crate) fn tag_as_str(tag: &Tag) -> &'static str {
        match tag {
            Tag::Div => "div",
            Tag::Span => "span",
            Tag::Button => "button",
            Tag::Input => "input",
            Tag::Select => "select",
            Tag::Option => "option",
            Tag::Textarea => "textarea",
            Tag::Label => "label",
            Tag::Form => "form",
            Tag::Header => "header",
            Tag::Nav => "nav",
            Tag::Main => "main",
            Tag::Section => "section",
            Tag::Aside => "aside",
            Tag::Footer => "footer",
            Tag::H1 => "h1",
            Tag::H2 => "h2",
            Tag::H3 => "h3",
            Tag::H4 => "h4",
            Tag::H5 => "h5",
            Tag::H6 => "h6",
            Tag::P => "p",
            Tag::Ul => "ul",
            Tag::Ol => "ol",
            Tag::Li => "li",
            Tag::A => "a",
            Tag::Img => "img",
            Tag::Audio => "audio",
            Tag::Video => "video",
            Tag::Svg => "svg",
            Tag::Path => "path",
            Tag::Canvas => "canvas",
            Tag::Table => "table",
            Tag::Thead => "thead",
            Tag::Tbody => "tbody",
            Tag::Tr => "tr",
            Tag::Th => "th",
            Tag::Td => "td",
            Tag::Hr => "hr",
            Tag::Br => "br",
            Tag::Custom(name) => name,
        }
    }

    /// Returns true if `tag` represents a `<style>` element.
    #[inline]
    pub(crate) fn is_style_tag(tag: &Tag) -> bool {
        matches!(tag, Tag::Custom(name) if name.eq_ignore_ascii_case("style"))
    }

    /// Extracts concatenated text content inside a `<style>` element blueprint.
    pub(crate) fn extract_style_element_text(children: &[NodeBlueprint], out: &mut String) {
        for child in children {
            match child {
                NodeBlueprint::Text(t) => out.push_str(t),
                NodeBlueprint::DynamicText(f) => out.push_str(&f()),
                NodeBlueprint::Fragment(inner) => Self::extract_style_element_text(inner, out),
                _ => {}
            }
        }
    }

    /// Parses and caches a CSS stylesheet string so hover/focus restyles avoid re-parsing unchanged `<style>` blocks.
    pub(crate) fn parse_cached_stylesheet(
        css: &str,
    ) -> Option<std::sync::Arc<strata_style::StyleSheet>> {
        thread_local! {
            static STYLE_SHEET_CACHE: std::cell::RefCell<
                std::collections::HashMap<String, std::sync::Arc<strata_style::StyleSheet>>
            > = std::cell::RefCell::new(std::collections::HashMap::new());
        }
        let trimmed = css.trim();
        if trimmed.is_empty() {
            return None;
        }
        STYLE_SHEET_CACHE.with(|cache| {
            if let Some(existing) = cache.borrow().get(trimmed) {
                return Some(existing.clone());
            }
            let parsed = strata_style::StyleSheet::parse(trimmed).ok()?;
            let arc = std::sync::Arc::new(parsed);
            let mut map = cache.borrow_mut();
            if map.len() >= 64 {
                map.clear();
            }
            map.insert(trimmed.to_string(), arc.clone());
            Some(arc)
        })
    }

    /// Collects all `<style>` elements mounted inside `blueprint` in document order.
    pub(crate) fn collect_stylesheets_from_blueprint(
        blueprint: &NodeBlueprint,
    ) -> Vec<std::sync::Arc<strata_style::StyleSheet>> {
        let mut sheets = Vec::new();
        Self::collect_stylesheets_rec(blueprint, &mut sheets, 0);
        sheets
    }

    fn collect_stylesheets_rec(
        blueprint: &NodeBlueprint,
        out: &mut Vec<std::sync::Arc<strata_style::StyleSheet>>,
        depth: usize,
    ) {
        if depth >= 256 {
            return;
        }
        match blueprint {
            NodeBlueprint::Element { tag, children, .. } => {
                if Self::is_style_tag(tag) {
                    let mut css = String::new();
                    Self::extract_style_element_text(children, &mut css);
                    if let Some(sheet) = Self::parse_cached_stylesheet(&css) {
                        out.push(sheet);
                    }
                } else {
                    for child in children {
                        Self::collect_stylesheets_rec(child, out, depth + 1);
                    }
                }
            }
            NodeBlueprint::Fragment(children) => {
                for child in children {
                    Self::collect_stylesheets_rec(child, out, depth + 1);
                }
            }
            _ => {}
        }
    }

    /// Extracts tokenized class names, `id`, and key-value `Property` attributes from a blueprint element.
    pub(crate) fn extract_element_selector_meta(
        classes: &strata_dom::ClassList,
        attributes: &[strata_dom::Attribute],
    ) -> (Vec<String>, Option<String>, Vec<(String, String)>) {
        let mut class_vec = Vec::new();
        for cls in classes.iter() {
            for tok in strata_style::tailwind::tokenize_classes(cls) {
                if !tok.is_empty() {
                    class_vec.push(tok.to_string());
                }
            }
        }
        let mut id_val = None;
        let mut attr_vec = Vec::new();
        for attr in attributes {
            if let strata_dom::Attribute::Property { key, value } = attr {
                if key.eq_ignore_ascii_case("id") && !value.is_empty() {
                    id_val = Some(value.clone());
                }
                attr_vec.push((key.clone(), value.clone()));
            }
        }
        (class_vec, id_val, attr_vec)
    }

    /// Resolves `:root`, `html`, and `body` rules from mounted `<style>` sheets so root custom properties
    /// (`--*`) and document typography/color defaults cascade to the entire RSX tree.
    pub(crate) fn resolve_document_root_inherited_style(
        blueprint: &NodeBlueprint,
        stylesheets: &[std::sync::Arc<strata_style::StyleSheet>],
        ctx: &InteractionContext,
    ) -> ComputedStyle {
        let mut doc_style = ComputedStyle::default();
        let (root_classes, root_id, root_attrs) = match blueprint {
            NodeBlueprint::Element {
                classes,
                attributes,
                ..
            } => Self::extract_element_selector_meta(classes, attributes),
            _ => (Vec::new(), None, Vec::new()),
        };
        let pseudo_state = strata_style::selector::PseudoState {
            is_dark: ctx.is_dark_mode,
            ..Default::default()
        };

        for synthetic_tag in [":root", "html", "body"] {
            let elem_ref = strata_style::selector::ElementRef::new(
                synthetic_tag,
                &root_classes,
                root_id.as_deref(),
                &root_attrs,
            );
            let mut matched = Vec::new();
            let mut order = 0usize;
            for sheet in stylesheets {
                for rule in &sheet.rules {
                    let layer = rule.layer_order.unwrap_or(u32::MAX);
                    for sel in &rule.selectors {
                        order += 1;
                        if sel.matches_hierarchy_full(
                            &elem_ref,
                            &[],
                            &[],
                            Some(true),
                            Some(true),
                            &pseudo_state,
                        ) {
                            matched.push((layer, sel.specificity, order, &rule.declarations));
                        }
                    }
                }
            }
            matched.sort_by_key(|&(layer, spec, ord, _)| (layer, spec, ord));
            for &(_, _, _, decls) in &matched {
                for decl in decls {
                    if decl.property.starts_with("--") && !decl.important {
                        strata_style::CascadeSolver::apply_declaration(&mut doc_style, decl);
                    }
                }
            }
            for &(_, _, _, decls) in &matched {
                for decl in decls {
                    if !decl.property.starts_with("--") && !decl.important {
                        strata_style::CascadeSolver::apply_declaration(&mut doc_style, decl);
                    }
                }
            }
            for &(_, _, _, decls) in &matched {
                for decl in decls {
                    if decl.important {
                        strata_style::CascadeSolver::apply_declaration(&mut doc_style, decl);
                    }
                }
            }
        }

        ComputedStyle::inherit_from(&doc_style)
    }

    /// Recursively transforms an RSX `NodeBlueprint` into a `LayoutNode` hierarchy with styling and inheritance.
    pub fn build_layout_tree(blueprint: &NodeBlueprint) -> LayoutNode {
        let ctx = InteractionContext::default();
        Self::build_layout_tree_with_interaction(blueprint, &ctx)
    }

    /// Recursively transforms an RSX `NodeBlueprint` into a `LayoutNode` hierarchy with styling, inheritance, and interaction state.
    pub fn build_layout_tree_with_interaction(
        blueprint: &NodeBlueprint,
        ctx: &InteractionContext,
    ) -> LayoutNode {
        let mut path_buf = Vec::new();
        let stylesheets = Self::collect_stylesheets_from_blueprint(blueprint);
        let doc_inherited = if !stylesheets.is_empty() {
            Some(Self::resolve_document_root_inherited_style(
                blueprint,
                &stylesheets,
                ctx,
            ))
        } else {
            None
        };
        let mut ancestors = Vec::new();
        Self::build_layout_tree_full(
            blueprint,
            doc_inherited.as_ref(),
            &mut path_buf,
            ctx,
            &stylesheets,
            &mut ancestors,
            &[],
            true,
            (false, false, false),
        )
    }

    /// Recursively transforms an RSX `NodeBlueprint` into a `LayoutNode` hierarchy with styling, inheritance, and hover state.
    pub fn build_layout_tree_with_hover(
        blueprint: &NodeBlueprint,
        hovered_path: Option<&[usize]>,
    ) -> LayoutNode {
        let ctx = InteractionContext {
            hovered_path,
            ..Default::default()
        };
        Self::build_layout_tree_with_interaction(blueprint, &ctx)
    }

    /// Internal recursive tree builder passing parent ComputedStyle for W3C CSS property inheritance.
    pub fn build_layout_tree_with_parent(
        blueprint: &NodeBlueprint,
        parent_style: Option<&ComputedStyle>,
    ) -> LayoutNode {
        let mut path_buf = Vec::new();
        let ctx = InteractionContext::default();
        Self::build_layout_tree_with_interaction_context(
            blueprint,
            parent_style,
            &mut path_buf,
            &ctx,
        )
    }

    /// Internal recursive tree builder passing parent style, node path, and active hover state.
    pub fn build_layout_tree_with_context(
        blueprint: &NodeBlueprint,
        parent_style: Option<&ComputedStyle>,
        current_path: &mut Vec<usize>,
        hovered_path: Option<&[usize]>,
    ) -> LayoutNode {
        let ctx = InteractionContext {
            hovered_path,
            ..Default::default()
        };
        Self::build_layout_tree_with_interaction_context(
            blueprint,
            parent_style,
            current_path,
            &ctx,
        )
    }

    /// Internal recursive tree builder passing parent style, node path, and full interaction context.
    pub fn build_layout_tree_with_interaction_context(
        blueprint: &NodeBlueprint,
        parent_style: Option<&ComputedStyle>,
        current_path: &mut Vec<usize>,
        ctx: &InteractionContext,
    ) -> LayoutNode {
        let stylesheets = Self::collect_stylesheets_from_blueprint(blueprint);
        let doc_inherited = if parent_style.is_none() && !stylesheets.is_empty() {
            Some(Self::resolve_document_root_inherited_style(
                blueprint,
                &stylesheets,
                ctx,
            ))
        } else {
            None
        };
        let mut ancestors = Vec::new();
        Self::build_layout_tree_full(
            blueprint,
            parent_style.or(doc_inherited.as_ref()),
            current_path,
            ctx,
            &stylesheets,
            &mut ancestors,
            &[],
            true,
            (false, false, false),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn build_layout_tree_full(
        blueprint: &NodeBlueprint,
        parent_style: Option<&ComputedStyle>,
        current_path: &mut Vec<usize>,
        ctx: &InteractionContext,
        stylesheets: &[std::sync::Arc<strata_style::StyleSheet>],
        ancestors: &mut Vec<CascadeElementMeta>,
        preceding_siblings: &[CascadeElementMeta],
        is_last_element_child: bool,
        group_state: (bool, bool, bool),
    ) -> LayoutNode {
        let max_build_depth = if cfg!(debug_assertions) { 128 } else { 256 };
        if current_path.len() >= max_build_depth {
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
                let entity_id = blueprint
                    .entity_id()
                    .unwrap_or(strata_primitives::EntityId::NONE);

                // <style> tags are registered in `stylesheets` and emit a hidden placeholder node
                // so 1-to-1 blueprint/layout child index correspondence is preserved.
                if Self::is_style_tag(tag) {
                    let mut style = ComputedStyle::default();
                    style.display = Display::None;
                    return LayoutNode::new(style).with_entity_id(entity_id);
                }

                let (mut style, current_meta_opt) = Self::resolve_element_cascade_style(
                    tag,
                    classes,
                    attributes,
                    parent_style,
                    current_path,
                    ctx,
                    stylesheets,
                    ancestors,
                    preceding_siblings,
                    is_last_element_child,
                    group_state,
                );

                Self::apply_top_layer_position_override(tag, attributes, &mut style);

                // 1. Vector SVG handling (runs AFTER full cascade so inherited `color`, `w-*`/`h-*`,
                // custom properties, and `:hover` overrides apply to `<svg stroke="currentColor">`).
                if matches!(tag, Tag::Svg | Tag::Path) {
                    if let Some(mut svg_node) =
                        Self::try_build_svg_leaf_node(blueprint, attributes, &mut style, entity_id)
                    {
                        Self::apply_portal_and_anchor_metadata(&mut svg_node, tag, attributes);
                        return svg_node;
                    }
                }

                // 2. Input / Textarea handling: synthesize text from value, children, or placeholder
                if matches!(tag, Tag::Input | Tag::Textarea) {
                    let mut input_node = Self::build_input_leaf_node(
                        blueprint,
                        matches!(tag, Tag::Textarea),
                        attributes,
                        *style,
                        entity_id,
                    );
                    Self::apply_portal_and_anchor_metadata(&mut input_node, tag, attributes);
                    return input_node;
                }

                // Replaced raster images: decode a base64 PNG data URI and carry
                // its intrinsic dimensions on the layout node for painting.
                if matches!(tag, Tag::Img) {
                    if let Some(mut img_node) =
                        Self::try_build_img_leaf_node(attributes, &mut style, entity_id)
                    {
                        Self::apply_portal_and_anchor_metadata(&mut img_node, tag, attributes);
                        return img_node;
                    }
                }

                // 2D / 3D Canvas Viewport handling: Canvas / Canvas2D / Canvas3D (or any element carrying a Canvas2D ScenePayload)
                let canvas2d_payload = attributes.iter().find_map(|attr| {
                    if let strata_dom::Attribute::ScenePayload(payload) = attr {
                        if payload
                            .downcast_ref::<Vec<strata_render::canvas2d::CanvasCommand>>()
                            .is_some()
                            || payload
                                .downcast_ref::<std::sync::Arc<
                                    Vec<strata_render::canvas2d::CanvasCommand>,
                                >>()
                                .is_some()
                            || payload
                                .downcast_ref::<strata_render::canvas2d::Canvas2DPaintCallback>()
                                .is_some()
                        {
                            return Some(std::sync::Arc::clone(payload));
                        }
                    }
                    None
                });
                if (canvas2d_payload.is_some()
                    || matches!(tag, Tag::Canvas)
                    || matches!(tag, Tag::Custom("Canvas3D" | "Canvas2D")))
                    && Self::apply_canvas3d_sizing_and_check_leaf(attributes, children, &mut style)
                {
                    let mut canvas_node = LayoutNode::new(*style).with_entity_id(entity_id);
                    if let Some(payload) = canvas2d_payload {
                        canvas_node.scene_3d = Some(strata_layout::ViewportSceneHandle(payload));
                    } else if let Some(scene_arc) =
                        strata_render::scene3d::extract_scene3d_arc_from_blueprint(blueprint)
                    {
                        canvas_node.scene_3d =
                            Some(strata_layout::ViewportSceneHandle(scene_arc));
                    }
                    Self::apply_portal_and_anchor_metadata(&mut canvas_node, tag, attributes);
                    return canvas_node;
                }

                Self::apply_single_text_inline_intrinsic_sizing(&mut style, children);

                let child_group_state = if Self::classes_have_group_marker(classes) {
                    let self_hovered = ctx
                        .hovered_path
                        .is_some_and(|h| h.starts_with(current_path));
                    let self_focused = ctx
                        .focused_path
                        .is_some_and(|f| f.starts_with(current_path));
                    let self_active = self_hovered && ctx.is_mouse_down;
                    (
                        group_state.0 || self_hovered,
                        group_state.1 || self_focused,
                        group_state.2 || self_active,
                    )
                } else {
                    group_state
                };

                let pushed_ancestor = if let Some(meta) = current_meta_opt {
                    ancestors.push(meta);
                    true
                } else {
                    false
                };

                let child_nodes = Self::build_flattened_children_full(
                    children,
                    Some(&style),
                    current_path,
                    ctx,
                    stylesheets,
                    ancestors,
                    child_group_state,
                );

                if pushed_ancestor {
                    ancestors.pop();
                }

                let mut node =
                    LayoutNode::with_children(*style, child_nodes).with_entity_id(entity_id);
                if let Some(payload) = canvas2d_payload {
                    node.scene_3d = Some(strata_layout::ViewportSceneHandle(payload));
                } else if matches!(tag, Tag::Canvas | Tag::Custom("Canvas3D")) {
                    if let Some(scene_arc) =
                        strata_render::scene3d::extract_scene3d_arc_from_blueprint(blueprint)
                    {
                        node.scene_3d = Some(strata_layout::ViewportSceneHandle(scene_arc));
                    }
                } else {
                    for attr in attributes {
                        if let strata_dom::Attribute::ScenePayload(payload) = attr {
                            node.scene_3d = Some(strata_layout::ViewportSceneHandle(
                                std::sync::Arc::clone(payload),
                            ));
                            break;
                        }
                    }
                }
                Self::apply_portal_and_anchor_metadata(&mut node, tag, attributes);
                node
            }
            NodeBlueprint::Text(text) => {
                Self::build_text_leaf_layout_node(text.clone(), parent_style)
            }
            NodeBlueprint::DynamicText(dyn_fn) => {
                Self::build_text_leaf_layout_node(dyn_fn(), parent_style)
            }
            NodeBlueprint::Fragment(children) => {
                let mut style = Box::new(match parent_style {
                    Some(p) => ComputedStyle::inherit_from(p),
                    None => ComputedStyle::default(),
                });
                style.display = Display::Flex;
                style.flex_direction = FlexDirection::Column;

                let child_nodes = Self::build_flattened_children_full(
                    children,
                    Some(&style),
                    current_path,
                    ctx,
                    stylesheets,
                    ancestors,
                    group_state,
                );

                LayoutNode::with_children(*style, child_nodes)
            }
            NodeBlueprint::Empty => Self::build_empty_leaf_layout_node(),
        }
    }

    #[inline(never)]
    pub(crate) fn build_text_leaf_layout_node(
        text: String,
        parent_style: Option<&ComputedStyle>,
    ) -> LayoutNode {
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
        style.width = Dimension::Px(w.ceil());
        style.height = Dimension::Px(h);
        LayoutNode::with_text(style, text)
    }

    #[inline(never)]
    fn build_empty_leaf_layout_node() -> LayoutNode {
        let mut style = ComputedStyle::default();
        style.display = Display::None;
        LayoutNode::new(style)
    }

    #[allow(clippy::too_many_arguments)]
    fn build_flattened_children_full(
        children: &[NodeBlueprint],
        parent_style: Option<&ComputedStyle>,
        parent_path: &mut Vec<usize>,
        ctx: &InteractionContext,
        stylesheets: &[std::sync::Arc<strata_style::StyleSheet>],
        ancestors: &mut Vec<CascadeElementMeta>,
        group_state: (bool, bool, bool),
    ) -> Vec<LayoutNode> {
        if parent_path.len() >= 256 {
            return Vec::new();
        }
        let mut flat = Vec::with_capacity(children.len());
        for child in children {
            Self::flatten_blueprints(child, &mut flat);
        }

        let last_elem_flat_idx = if !stylesheets.is_empty() {
            flat.iter().rposition(
                |bp| matches!(bp, NodeBlueprint::Element { tag, .. } if !Self::is_style_tag(tag)),
            )
        } else {
            None
        };

        let mut out = Vec::with_capacity(flat.len());
        let mut preceding_siblings: Vec<CascadeElementMeta> = Vec::new();

        for (idx, child_bp) in flat.into_iter().enumerate() {
            parent_path.push(idx);
            let is_last_elem = last_elem_flat_idx == Some(idx);
            let node = Self::build_layout_tree_full(
                child_bp,
                parent_style,
                parent_path,
                ctx,
                stylesheets,
                ancestors,
                &preceding_siblings,
                is_last_elem,
                group_state,
            );
            if !stylesheets.is_empty() {
                if let NodeBlueprint::Element {
                    tag,
                    classes,
                    attributes,
                    ..
                } = child_bp
                {
                    if !Self::is_style_tag(tag) {
                        let is_hovered =
                            ctx.hovered_path.is_some_and(|h| h.starts_with(parent_path));
                        let is_focused =
                            ctx.focused_path.is_some_and(|f| f.starts_with(parent_path));
                        let is_active = is_hovered && ctx.is_mouse_down;
                        let is_disabled = attributes.iter().any(|attr| match attr {
                            strata_dom::Attribute::Property { key, value } => {
                                key.eq_ignore_ascii_case("disabled")
                                    && !value.eq_ignore_ascii_case("false")
                            }
                            _ => false,
                        });
                        let (class_vec, id_val, attr_vec) =
                            Self::extract_element_selector_meta(classes, attributes);
                        preceding_siblings.push(CascadeElementMeta {
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
                        });
                    }
                }
            }
            parent_path.pop();
            out.push(node);
        }
        out
    }

    /// Recursively flattens Fragment blueprints so child elements splice directly into parent layout contexts.
    pub(crate) fn append_child_layout_nodes_with_interaction_context(
        blueprint: &NodeBlueprint,
        parent_style: Option<&ComputedStyle>,
        parent_path: &mut Vec<usize>,
        child_counter: &mut usize,
        ctx: &InteractionContext,
        out: &mut Vec<LayoutNode>,
    ) {
        if parent_path.len() >= 256 {
            return;
        }
        if let NodeBlueprint::Fragment(children) = blueprint {
            for child in children {
                Self::append_child_layout_nodes_with_interaction_context(
                    child,
                    parent_style,
                    parent_path,
                    child_counter,
                    ctx,
                    out,
                );
            }
        } else {
            let this_idx = *child_counter;
            *child_counter += 1;
            parent_path.push(this_idx);
            let node = Self::build_layout_tree_with_interaction_context(
                blueprint,
                parent_style,
                parent_path,
                ctx,
            );
            parent_path.pop();
            out.push(node);
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
        let ctx = InteractionContext {
            hovered_path,
            ..Default::default()
        };
        Self::append_child_layout_nodes_with_interaction_context(
            blueprint,
            parent_style,
            parent_path,
            child_counter,
            &ctx,
            out,
        );
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

    fn extract_inline_style_decl(style_raw: &str, target_prop: &str) -> Option<String> {
        for decl in style_raw.split(';') {
            if let Some((k, v)) = decl.split_once(':') {
                if k.trim().eq_ignore_ascii_case(target_prop) {
                    let val = v.trim().trim_end_matches("!important").trim();
                    if !val.is_empty() && !val.eq_ignore_ascii_case("none") {
                        return Some(val.trim_matches('"').trim_matches('\'').to_string());
                    }
                }
            }
        }
        None
    }

    #[inline(never)]
    fn apply_top_layer_position_override(
        tag: &Tag,
        attributes: &[strata_dom::Attribute],
        style: &mut ComputedStyle,
    ) {
        if matches!(tag, Tag::Custom(name) if name.eq_ignore_ascii_case("portal")) {
            style.position = strata_style::types::Position::Fixed;
            return;
        }
        for attr in attributes {
            if let strata_dom::Attribute::Property { key, value } = attr {
                let k = key.trim();
                let v = value.trim();
                match k {
                    "data-portal" => {
                        if !matches!(
                            v.to_ascii_lowercase().as_str(),
                            "false" | "0" | "no" | "off"
                        ) {
                            style.position = strata_style::types::Position::Fixed;
                            return;
                        }
                    }
                    "popover" => {
                        style.position = strata_style::types::Position::Fixed;
                        return;
                    }
                    "data-portal-anchor" | "anchor" | "position-anchor" if !v.is_empty() => {
                        style.position = strata_style::types::Position::Fixed;
                        return;
                    }
                    "style" => {
                        if Self::extract_inline_style_decl(v, "position-anchor").is_some() {
                            style.position = strata_style::types::Position::Fixed;
                            return;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    #[inline(never)]
    pub(crate) fn apply_portal_and_anchor_metadata(
        node: &mut LayoutNode,
        tag: &Tag,
        attributes: &[strata_dom::Attribute],
    ) {
        let is_portal_tag = matches!(tag, Tag::Custom(name) if name.eq_ignore_ascii_case("portal"));
        if !is_portal_tag && attributes.is_empty() {
            return;
        }

        let mut element_id: Option<String> = None;
        let mut anchor_name: Option<String> = None;
        let mut is_portal_attr = false;
        let mut anchor_target: Option<String> = None;
        let mut placement_raw: Option<String> = None;
        let mut offset_opt: Option<f32> = None;
        let mut flip_opt: Option<bool> = None;
        let mut shift_opt: Option<bool> = None;
        let mut padding_opt: Option<f32> = None;
        let mut match_width_opt: Option<bool> = None;
        let mut min_width_from_anchor_opt: Option<bool> = None;
        let mut inline_style_raw: Option<&str> = None;

        for attr in attributes {
            if let strata_dom::Attribute::Property { key, value } = attr {
                let k = key.trim();
                let v = value.trim();
                match k {
                    "id" | "data-anchor-id" => {
                        if !v.is_empty() {
                            element_id = Some(v.to_string());
                        }
                    }
                    "data-anchor-name" | "anchor-name" => {
                        if !v.is_empty() {
                            anchor_name = Some(v.to_string());
                        }
                    }
                    "data-portal" => {
                        is_portal_attr = !matches!(
                            v.to_ascii_lowercase().as_str(),
                            "false" | "0" | "no" | "off"
                        );
                    }
                    "popover" => {
                        is_portal_attr = true;
                    }
                    "data-portal-anchor" | "anchor" | "position-anchor" => {
                        if !v.is_empty() {
                            anchor_target = Some(v.to_string());
                        }
                    }
                    "data-portal-placement" | "placement" | "position-area" | "inset-area" => {
                        if !v.is_empty() {
                            placement_raw = Some(v.to_string());
                        }
                    }
                    "data-portal-offset" => {
                        if let Ok(parsed) = v.trim_end_matches("px").trim().parse::<f32>() {
                            offset_opt = Some(parsed);
                        }
                    }
                    "data-portal-flip" => {
                        flip_opt = Some(!matches!(
                            v.to_ascii_lowercase().as_str(),
                            "false" | "0" | "no" | "off"
                        ));
                    }
                    "data-portal-shift" => {
                        shift_opt = Some(!matches!(
                            v.to_ascii_lowercase().as_str(),
                            "false" | "0" | "no" | "off"
                        ));
                    }
                    "data-portal-padding" => {
                        if let Ok(parsed) = v.trim_end_matches("px").trim().parse::<f32>() {
                            padding_opt = Some(parsed);
                        }
                    }
                    "data-portal-match-width" => {
                        match_width_opt = Some(matches!(
                            v.to_ascii_lowercase().as_str(),
                            "true" | "1" | "yes" | "on"
                        ));
                    }
                    "data-portal-min-width" => {
                        min_width_from_anchor_opt = Some(matches!(
                            v.to_ascii_lowercase().as_str(),
                            "true" | "1" | "yes" | "on"
                        ));
                    }
                    "style" => {
                        inline_style_raw = Some(v);
                    }
                    _ => {}
                }
            }
        }

        if let Some(style_str) = inline_style_raw {
            if anchor_name.is_none() {
                anchor_name = Self::extract_inline_style_decl(style_str, "anchor-name");
            }
            if anchor_target.is_none() {
                anchor_target = Self::extract_inline_style_decl(style_str, "position-anchor");
            }
            if placement_raw.is_none() {
                placement_raw = Self::extract_inline_style_decl(style_str, "position-area")
                    .or_else(|| Self::extract_inline_style_decl(style_str, "inset-area"));
            }
        }

        let portal_anchor = anchor_target.map(|anchor_id| {
            let placement = placement_raw
                .as_deref()
                .and_then(strata_layout::AnchorPlacement::parse)
                .unwrap_or(strata_layout::AnchorPlacement::BottomEnd);
            Box::new(strata_layout::PortalAnchorSpec {
                anchor_id,
                placement,
                offset: offset_opt.unwrap_or(4.0),
                flip: flip_opt.unwrap_or(true),
                shift: shift_opt.unwrap_or(true),
                viewport_padding: padding_opt.unwrap_or(8.0),
                match_width: match_width_opt.unwrap_or(false),
                min_width_from_anchor: min_width_from_anchor_opt.unwrap_or(false),
            })
        });

        node.element_id = element_id;
        node.anchor_name = anchor_name;
        node.is_top_layer = is_portal_tag || is_portal_attr || portal_anchor.is_some();
        node.portal_anchor = portal_anchor;
    }

    /// Re-evaluates styling with the current interaction state, recomputes layout, and compiles display list.
    pub fn rebuild_layout_tree(&mut self) {
        self.applied_scroll_offsets.clear();
        self.applied_portal_offsets.clear();
        let stylesheets = Self::collect_stylesheets_from_blueprint(&self.root_blueprint);
        for sheet in &stylesheets {
            self.register_keyframes_stylesheet(sheet);
        }
        let rebuild_start = std::time::Instant::now();
        let ctx = self.interaction_context();
        self.layout_root = Self::build_layout_tree_with_interaction(&self.root_blueprint, &ctx);
        self.last_built_is_mouse_down = self.is_mouse_down;
        self.sync_active_input_layout();
        let now_ms = self.now_ms();
        self.sync_tree_animations(now_ms);
        let compute_start = std::time::Instant::now();
        self.frame_profile.rebuild += compute_start.duration_since(rebuild_start);
        self.layout_root
            .compute_layout(self.viewport_width, self.viewport_height);
        Self::sync_layout_to_ir(&self.layout_root, &mut self.ir_graph);
        self.layout_computations += 1;
        self.frame_profile.layout += compute_start.elapsed();
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
        self.dispatch_resize_observers();
        self.spatial_index
            .rebuild(&self.root_blueprint, &self.layout_root);
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
        if parent_path.len() >= 256 {
            return;
        }
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

    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    fn resolve_element_cascade_style(
        tag: &Tag,
        classes: &strata_dom::ClassList,
        attributes: &[strata_dom::Attribute],
        parent_style: Option<&ComputedStyle>,
        current_path: &[usize],
        ctx: &InteractionContext,
        stylesheets: &[std::sync::Arc<strata_style::StyleSheet>],
        ancestors: &[CascadeElementMeta],
        preceding_siblings: &[CascadeElementMeta],
        is_last_element_child: bool,
        group_state: (bool, bool, bool),
    ) -> (Box<ComputedStyle>, Option<CascadeElementMeta>) {
        let is_hovered = ctx
            .hovered_path
            .is_some_and(|h| h.starts_with(current_path));
        let is_focused = ctx
            .focused_path
            .is_some_and(|f| f.starts_with(current_path));
        let is_active = is_hovered && ctx.is_mouse_down;
        let is_disabled = attributes.iter().any(|attr| match attr {
            strata_dom::Attribute::Property { key, value } => {
                key.eq_ignore_ascii_case("disabled") && !value.eq_ignore_ascii_case("false")
            }
            _ => false,
        });
        let is_dark = ctx.is_dark_mode;
        let pseudo_state = strata_style::selector::PseudoState {
            is_hovered,
            is_focused,
            is_active,
            is_disabled,
            is_dark,
            is_group_hovered: group_state.0,
            is_group_focused: group_state.1,
            is_group_active: group_state.2,
        };
        let mut style = Box::new(match parent_style {
            Some(p) => ComputedStyle::inherit_from(p),
            None => ComputedStyle::default(),
        });

        let mut has_direct_style = false;
        let mut direct_computed_override: Option<&strata_style::types::ComputedStyle> = None;
        let mut inline_decls: Vec<strata_style::Declaration> = Vec::new();
        for attr in attributes {
            if let strata_dom::Attribute::DirectStyle(any) = attr {
                if let Some(decls) = any.downcast_ref::<Vec<strata_style::Declaration>>() {
                    inline_decls.extend(decls.iter().cloned());
                    has_direct_style = true;
                } else if let Some(cs) = any.downcast_ref::<strata_style::types::ComputedStyle>() {
                    direct_computed_override = Some(cs);
                    has_direct_style = true;
                }
            }
        }
        if !has_direct_style {
            for attr in attributes {
                if let strata_dom::Attribute::Property { key, value } = attr {
                    if key.eq_ignore_ascii_case("style") && !value.is_empty() {
                        inline_decls.extend(strata_style::parse_declarations(value));
                    }
                }
            }
        }

        let tag_name = Self::tag_as_str(tag);
        let mut matched_base_rules: Vec<(
            u32,
            strata_style::Specificity,
            usize,
            &[strata_style::Declaration],
        )> = Vec::new();
        let mut matched_interactive_rules: Vec<(
            u32,
            strata_style::Specificity,
            usize,
            &[strata_style::Declaration],
        )> = Vec::new();
        let mut current_meta_opt: Option<CascadeElementMeta> = None;

        if !stylesheets.is_empty() {
            let (class_vec, id_val, attr_vec) =
                Self::extract_element_selector_meta(classes, attributes);
            let elem_ref = strata_style::selector::ElementRef::new(
                tag_name,
                &class_vec,
                id_val.as_deref(),
                &attr_vec,
            );
            let ancestor_refs: Vec<strata_style::selector::ElementRef<'_>> = ancestors
                .iter()
                .map(|a| {
                    strata_style::selector::ElementRef::new(
                        a.tag,
                        &a.classes,
                        a.id.as_deref(),
                        &a.attributes,
                    )
                })
                .collect();
            let ancestor_states: Vec<strata_style::selector::PseudoState> =
                ancestors.iter().map(|a| a.pseudo_state).collect();
            let sib_refs: Vec<strata_style::selector::ElementRef<'_>> = preceding_siblings
                .iter()
                .map(|s| {
                    strata_style::selector::ElementRef::new(
                        s.tag,
                        &s.classes,
                        s.id.as_deref(),
                        &s.attributes,
                    )
                })
                .collect();
            let sib_states: Vec<strata_style::selector::PseudoState> =
                preceding_siblings.iter().map(|s| s.pseudo_state).collect();

            let mut order_counter = 0usize;
            for sheet in stylesheets {
                for rule in &sheet.rules {
                    let layer = rule.layer_order.unwrap_or(u32::MAX);
                    for sel in &rule.selectors {
                        order_counter += 1;
                        if sel.matches_hierarchy_with_ancestor_states(
                            &elem_ref,
                            &ancestor_refs,
                            Some(&ancestor_states),
                            &sib_refs,
                            Some(&sib_states),
                            Some(is_last_element_child),
                            None,
                            &pseudo_state,
                        ) {
                            if sel.has_any_interactive_pseudo() {
                                matched_interactive_rules.push((
                                    layer,
                                    sel.specificity,
                                    order_counter,
                                    &rule.declarations,
                                ));
                            } else {
                                matched_base_rules.push((
                                    layer,
                                    sel.specificity,
                                    order_counter,
                                    &rule.declarations,
                                ));
                            }
                        }
                    }
                }
            }
            matched_base_rules.sort_by_key(|&(l, s, o, _)| (l, s, o));
            matched_interactive_rules.sort_by_key(|&(l, s, o, _)| (l, s, o));

            for sheet in stylesheets {
                for srule in &sheet.starting_styles {
                    if srule.selectors.iter().any(|sel| {
                        sel.matches_hierarchy_with_ancestor_states(
                            &elem_ref,
                            &ancestor_refs,
                            Some(&ancestor_states),
                            &sib_refs,
                            Some(&sib_states),
                            Some(is_last_element_child),
                            None,
                            &pseudo_state,
                        )
                    }) {
                        if style.starting_style.is_none() {
                            style.starting_style = Some(Box::new((*style).clone()));
                        }
                        if let Some(start_box) = style.starting_style.as_mut() {
                            for decl in &srule.declarations {
                                strata_style::CascadeSolver::apply_declaration(start_box, decl);
                            }
                        }
                    }
                }
            }

            current_meta_opt = Some(CascadeElementMeta {
                tag: tag_name,
                classes: class_vec,
                id: id_val,
                attributes: attr_vec,
                pseudo_state,
            });
        }

        let has_display_class = classes.iter().any(|c| {
            strata_style::tailwind::tokenize_classes(c)
                .into_iter()
                .any(|tok| {
                    let mut base = tok;
                    while let (Some(_), rest) = strata_style::tailwind::split_modifier(base) {
                        base = rest;
                    }
                    matches!(
                        base,
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
        let has_explicit_display_decl = inline_decls
            .iter()
            .any(|d| d.property.eq_ignore_ascii_case("display"))
            || matched_base_rules.iter().any(|(_, _, _, decls)| {
                decls
                    .iter()
                    .any(|d| d.property.eq_ignore_ascii_case("display"))
            })
            || matched_interactive_rules.iter().any(|(_, _, _, decls)| {
                decls
                    .iter()
                    .any(|d| d.property.eq_ignore_ascii_case("display"))
            });

        if !has_display_class && !has_explicit_display_decl {
            match tag {
                Tag::Svg | Tag::Path | Tag::Input | Tag::Textarea => {}
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
                        style.text_decoration = strata_style::types::TextDecoration::Underline;
                    }
                }
                _ => {
                    if let Tag::Custom(name) = tag {
                        if matches!(
                            name.to_ascii_lowercase().as_str(),
                            "code"
                                | "strong"
                                | "em"
                                | "b"
                                | "i"
                                | "u"
                                | "s"
                                | "small"
                                | "mark"
                                | "kbd"
                                | "samp"
                                | "var"
                                | "abbr"
                                | "cite"
                                | "q"
                                | "sub"
                                | "sup"
                                | "del"
                                | "ins"
                                | "time"
                        ) {
                            style.display = Display::Inline;
                        } else {
                            style.display = Display::Flex;
                            if name.eq_ignore_ascii_case("portal") {
                                style.flex_direction = FlexDirection::Column;
                            }
                        }
                    } else {
                        style.display = Display::Flex;
                    }
                }
            }
        } else if matches!(tag, Tag::Custom(name) if name.eq_ignore_ascii_case("portal"))
            && matches!(style.display, Display::Flex | Display::InlineFlex)
        {
            let has_explicit_flex_dir = classes.iter().any(|c| {
                c.split_whitespace().any(|tok| {
                    matches!(
                        tok,
                        "flex-row" | "flex-row-reverse" | "flex-col" | "flex-col-reverse"
                    )
                })
            }) || inline_decls
                .iter()
                .any(|d| d.property.eq_ignore_ascii_case("flex-direction"));
            if !has_explicit_flex_dir {
                style.flex_direction = FlexDirection::Column;
            }
        }

        for &(_, _, _, decls) in &matched_base_rules {
            for decl in decls {
                if decl.property.starts_with("--") && !decl.important {
                    strata_style::CascadeSolver::apply_declaration(&mut style, decl);
                }
            }
        }
        for decl in &inline_decls {
            if decl.property.starts_with("--") && !decl.important {
                strata_style::CascadeSolver::apply_declaration(&mut style, decl);
            }
        }

        for &(_, _, _, decls) in &matched_base_rules {
            for decl in decls {
                if !decl.property.starts_with("--") && !decl.important {
                    strata_style::CascadeSolver::apply_declaration(&mut style, decl);
                }
            }
        }

        Self::apply_base_tailwind_classes(&mut style, classes, &pseudo_state);

        if let Some(cs) = direct_computed_override {
            *style = cs.clone();
        }
        for attr in attributes {
            if let strata_dom::Attribute::Property { key, value } = attr {
                if key.eq_ignore_ascii_case("href") && !value.is_empty() {
                    let val_atom = strata_primitives::atom::intern(value);
                    style.set_custom_property(strata_primitives::AtomId::HREF, val_atom);
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
                }
            }
        }
        for decl in &inline_decls {
            if !decl.property.starts_with("--") && !decl.important {
                strata_style::CascadeSolver::apply_declaration(&mut style, decl);
            }
        }

        for &(_, _, _, decls) in &matched_interactive_rules {
            for decl in decls {
                if decl.property.starts_with("--") && !decl.important {
                    strata_style::CascadeSolver::apply_declaration(&mut style, decl);
                }
            }
        }
        for &(_, _, _, decls) in &matched_interactive_rules {
            for decl in decls {
                if !decl.property.starts_with("--") && !decl.important {
                    strata_style::CascadeSolver::apply_declaration(&mut style, decl);
                }
            }
        }

        Self::apply_interactive_tailwind_classes(&mut style, classes, &pseudo_state);

        if !matched_base_rules.is_empty() || !matched_interactive_rules.is_empty() {
            let mut important_rules: Vec<(
                u32,
                strata_style::Specificity,
                usize,
                &[strata_style::Declaration],
            )> = Vec::new();
            for &entry in matched_base_rules
                .iter()
                .chain(matched_interactive_rules.iter())
            {
                if entry.3.iter().any(|d| d.important) {
                    important_rules.push(entry);
                }
            }
            important_rules.sort_by_key(|&(l, s, o, _)| (l, s, o));
            for &(_, _, _, decls) in &important_rules {
                for decl in decls {
                    if decl.important && decl.property.starts_with("--") {
                        strata_style::CascadeSolver::apply_declaration(&mut style, decl);
                    }
                }
            }
            for &(_, _, _, decls) in &important_rules {
                for decl in decls {
                    if decl.important && !decl.property.starts_with("--") {
                        strata_style::CascadeSolver::apply_declaration(&mut style, decl);
                    }
                }
            }
        }
        for decl in &inline_decls {
            if decl.important && decl.property.starts_with("--") {
                strata_style::CascadeSolver::apply_declaration(&mut style, decl);
            }
        }
        for decl in &inline_decls {
            if decl.important && !decl.property.starts_with("--") {
                strata_style::CascadeSolver::apply_declaration(&mut style, decl);
            }
        }

        if matches!(tag, Tag::Button)
            && style.display == Display::Flex
            && style.width == Dimension::Auto
            && style.flex_grow == 0.0
        {
            style.display = Display::InlineFlex;
        }

        (style, current_meta_opt)
    }

    #[inline(never)]
    fn try_build_svg_leaf_node(
        blueprint: &NodeBlueprint,
        attributes: &[strata_dom::Attribute],
        style: &mut ComputedStyle,
        entity_id: strata_primitives::EntityId,
    ) -> Option<LayoutNode> {
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

        if svg_items.is_empty() {
            return None;
        }

        if style.width == Dimension::Auto {
            style.width = Dimension::Px(explicit_svg_w.unwrap_or(if view_box.0 > 24.0 {
                view_box.0.min(80.0)
            } else {
                16.0
            }));
        }
        if style.height == Dimension::Auto {
            style.height = Dimension::Px(explicit_svg_h.unwrap_or(if view_box.1 > 24.0 {
                view_box.1.min(80.0)
            } else {
                16.0
            }));
        }
        Some(LayoutNode::with_svg_items(style.clone(), svg_items).with_entity_id(entity_id))
    }

    /// Standard bullet glyph (`U+2022`) rendered in place of characters for `<input type="password">`.
    pub(crate) const PASSWORD_MASK_CHAR: char = '\u{2022}';

    /// Returns `true` when an element's attributes include `type="password"` (case-insensitive).
    pub(crate) fn is_password_input_attrs(attributes: &[strata_dom::Attribute]) -> bool {
        attributes.iter().any(|attr| {
            matches!(
                attr,
                strata_dom::Attribute::Property { key, value }
                    if key.eq_ignore_ascii_case("type")
                        && value.trim().eq_ignore_ascii_case("password")
            )
        })
    }

    /// Replaces each Unicode scalar value in `text` with [`Self::PASSWORD_MASK_CHAR`] (`•`),
    /// preserving exact 1-to-1 character indexing with `TextInputState`.
    pub(crate) fn mask_password_text(text: &str) -> String {
        let count = text.chars().count();
        let mut masked = String::with_capacity(count * Self::PASSWORD_MASK_CHAR.len_utf8());
        for _ in 0..count {
            masked.push(Self::PASSWORD_MASK_CHAR);
        }
        masked
    }

    #[inline(never)]
    fn build_input_leaf_node(
        blueprint: &NodeBlueprint,
        is_textarea: bool,
        attributes: &[strata_dom::Attribute],
        mut style: ComputedStyle,
        entity_id: strata_primitives::EntityId,
    ) -> LayoutNode {
        if style.cursor == strata_style::types::Cursor::Default {
            style.cursor = strata_style::types::Cursor::Text;
        }
        style.white_space = if is_textarea {
            strata_style::types::WhiteSpace::PreWrap
        } else {
            strata_style::types::WhiteSpace::NoWrap
        };
        style.overflow_x = strata_style::types::Overflow::Hidden;
        if is_textarea {
            if style.overflow_y == strata_style::types::Overflow::Visible {
                style.overflow_y = strata_style::types::Overflow::Auto;
            }
        } else {
            style.overflow_y = strata_style::types::Overflow::Hidden;
        }
        let is_password = !is_textarea && Self::is_password_input_attrs(attributes);
        let mut value_text = None;
        let mut placeholder_text = None;
        let mut rows_attr: Option<usize> = None;
        let mut cols_attr: Option<usize> = None;
        for attr in attributes {
            if let strata_dom::Attribute::Property { key, value } = attr {
                if key == "value" && !value.is_empty() {
                    value_text = Some(value.clone());
                } else if key == "placeholder" && !value.is_empty() {
                    placeholder_text = Some(value.clone());
                } else if key == "rows" {
                    rows_attr = value.trim().parse::<usize>().ok().filter(|&r| r > 0);
                } else if key == "cols" {
                    cols_attr = value.trim().parse::<usize>().ok().filter(|&c| c > 0);
                }
            }
        }
        if is_textarea && value_text.is_none() {
            let child_text = Self::extract_text_content(blueprint);
            if !child_text.is_empty() {
                value_text = Some(child_text);
            }
        }
        let is_mono = strata_layout::is_monospace_family(style.font_family);
        let char_w = if is_mono {
            style.font_size * 0.60
        } else {
            style.font_size * 0.55
        };
        if style.height == Dimension::Auto {
            let line_count = if is_textarea {
                rows_attr.unwrap_or(3) as f32
            } else {
                1.0
            };
            let text_h = style.font_size * style.line_height * line_count;
            style.height = Dimension::Px(
                text_h
                    + style.padding.top
                    + style.padding.bottom
                    + style.border_width.top
                    + style.border_width.bottom,
            );
        }
        let border_h = style.border_width.left + style.border_width.right;
        if let Some(val) = value_text {
            let display_val = if is_password {
                Self::mask_password_text(&val)
            } else {
                val
            };
            let max_chars = if is_textarea {
                display_val
                    .lines()
                    .map(|l| l.chars().count())
                    .max()
                    .unwrap_or(0)
            } else {
                display_val.chars().count()
            };
            let approx_width = if is_textarea {
                cols_attr
                    .map(|c| c as f32 * char_w)
                    .unwrap_or_else(|| (max_chars as f32 * char_w).max(200.0))
            } else {
                (max_chars as f32 * char_w).max(40.0)
            };
            if style.width == Dimension::Auto && style.flex_grow <= 0.0 {
                style.width = Dimension::Px(
                    approx_width + style.padding.left + style.padding.right + border_h,
                );
            }
            LayoutNode::with_text(style, display_val).with_entity_id(entity_id)
        } else if let Some(ph) = placeholder_text {
            let approx_width = if is_textarea {
                cols_attr
                    .map(|c| c as f32 * char_w)
                    .unwrap_or_else(|| (ph.chars().count() as f32 * char_w).max(200.0))
            } else {
                (ph.chars().count() as f32 * char_w).max(40.0)
            };
            if style.width == Dimension::Auto && style.flex_grow <= 0.0 {
                style.width = Dimension::Px(
                    approx_width + style.padding.left + style.padding.right + border_h,
                );
            }
            style.color = strata_style::types::Color::new(140, 145, 155, 255);
            LayoutNode::with_text(style, ph).with_entity_id(entity_id)
        } else {
            let default_w = if is_textarea {
                cols_attr.map(|c| c as f32 * char_w).unwrap_or(200.0)
            } else {
                60.0
            };
            if style.width == Dimension::Auto && style.flex_grow <= 0.0 {
                style.width =
                    Dimension::Px(default_w + style.padding.left + style.padding.right + border_h);
            }
            LayoutNode::with_text(style, String::new()).with_entity_id(entity_id)
        }
    }

    #[inline(never)]
    fn try_build_img_leaf_node(
        attributes: &[strata_dom::Attribute],
        style: &mut ComputedStyle,
        entity_id: strata_primitives::EntityId,
    ) -> Option<LayoutNode> {
        let mut src_value = None;
        for attr in attributes {
            if let strata_dom::Attribute::Property { key, value } = attr {
                if key.eq_ignore_ascii_case("src") && !value.is_empty() {
                    src_value = Some(value.as_str());
                }
            }
        }
        let src = src_value?;
        let payload = data_uri_base64_payload(src)?;
        let decoded = decode_base64(payload)?;
        let (width, height) = png_dimensions(&decoded)?;
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
        Some(LayoutNode::with_image(style.clone(), image).with_entity_id(entity_id))
    }

    #[inline(never)]
    fn apply_canvas3d_sizing_and_check_leaf(
        attributes: &[strata_dom::Attribute],
        children: &[NodeBlueprint],
        style: &mut ComputedStyle,
    ) -> bool {
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
        let is_declarative_3d_child = |c: &NodeBlueprint| match c {
            NodeBlueprint::Element {
                tag: Tag::Custom(name),
                ..
            } => matches!(
                *name,
                "PerspectiveCamera"
                    | "OrthographicCamera"
                    | "AmbientLight"
                    | "DirectionalLight"
                    | "PointLight"
                    | "Mesh"
                    | "BoxGeometry"
                    | "SphereGeometry"
                    | "PlaneGeometry"
                    | "CylinderGeometry"
                    | "StandardMaterial"
                    | "ShaderMaterial"
            ),
            NodeBlueprint::Empty => true,
            _ => false,
        };
        let has_dom_overlay_children = children.iter().any(|c| !is_declarative_3d_child(c));
        if has_dom_overlay_children {
            return false;
        }
        attributes.iter().any(|a| {
            matches!(a, strata_dom::Attribute::ScenePayload(_))
                || matches!(a, strata_dom::Attribute::Property { key, value } if key == "data-strata-canvas3d" && value == "true")
        }) || children.iter().any(|c| match c {
            NodeBlueprint::Element { .. } => is_declarative_3d_child(c),
            _ => false,
        })
    }

    #[inline(never)]
    pub(crate) fn apply_single_text_inline_intrinsic_sizing(
        style: &mut ComputedStyle,
        children: &[NodeBlueprint],
    ) {
        // Non-replaced `display: inline` elements (e.g. `<span>`, `<a>`, `<label>`)
        // must remain `width: auto; height: auto` so their inline text wraps to
        // whatever width their containing block, flex item (`flex-1 min-w-0`), or
        // grid track resolves to. Only atomic inline containers (`inline-flex` /
        // `inline-block`) that are not flexible (`flex-grow == 0`) or explicitly
        // `min-w-0` should receive eager single-text intrinsic dimensions.
        let is_atomic_inline = matches!(
            style.display,
            Display::InlineFlex | Display::InlineBlock
        );
        let is_flexible_or_min_zero =
            style.flex_grow > 0.0 || matches!(style.min_width, Dimension::Px(px) if px == 0.0);
        if is_atomic_inline
            && !is_flexible_or_min_zero
            && style.width == Dimension::Auto
            && children.len() == 1
        {
            let single_text: Option<std::borrow::Cow<'_, str>> = match &children[0] {
                NodeBlueprint::Text(t) => Some(std::borrow::Cow::Borrowed(t.as_str())),
                NodeBlueprint::DynamicText(dyn_fn) => Some(std::borrow::Cow::Owned(dyn_fn())),
                _ => None,
            };
            if let Some(t) = single_text {
                let is_mono = strata_layout::is_monospace_family(style.font_family);
                let (text_w, raw_h) = strata_layout::measure_text_bounds_styled_with_family(
                    &t,
                    style.font_size,
                    style.line_height,
                    None,
                    is_mono,
                    style.font_family,
                    style.font_weight,
                    style.font_style,
                );
                let total_w = (text_w
                    + style.padding.left
                    + style.padding.right
                    + style.border_width.left
                    + style.border_width.right)
                    .ceil();
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
    }
}
