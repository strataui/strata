//! Generic, non-hardcoded HTML5-to-Strata DOM transpiler with W3C CSS cascade and IFC restructuring.
//!
//! Transforms unmutated semantic HTML5 DOM nodes into native, styled Strata reactive blueprints
//! with full W3C CSS cascade priority, property inheritance, relative unit resolution, and
//! Inline Formatting Context restructuring.

use crate::ast::SemanticNode;
use crate::reader_theme::ReaderTheme;
use strata_dom::{Attribute, NodeBlueprint, Tag};
use strata_style::types::*;
use strata_style::{CascadeSolver, ElementRef, MediaContext, PseudoState, StyleSheet};

/// Context of an element within the DOM hierarchy used for CSS selector matching.
#[derive(Debug, Clone)]
pub struct ElementContext {
    pub tag: String,
    pub classes: Vec<String>,
    pub id: Option<String>,
    pub attributes: Vec<(String, String)>,
}

/// Composites a canvas background color over the W3C user-agent white canvas.
/// Author backgrounds with partial alpha keep their Chromium-equivalent blend.
fn composite_over_white(color: Color) -> Color {
    if color.a == 255 {
        color
    } else {
        color.blend_over(Color::WHITE)
    }
}

fn intern_static_tag(tag: &str) -> &'static str {
    match tag {
        // Semantic phrasing and inline elements
        "blockquote" => "blockquote",
        "pre" => "pre",
        "code" => "code",
        "figure" => "figure",
        "figcaption" => "figcaption",
        "b" => "b",
        "strong" => "strong",
        "i" => "i",
        "em" => "em",
        "small" => "small",
        "sub" => "sub",
        "sup" => "sup",
        "mark" => "mark",
        "del" => "del",
        "s" => "s",
        "ins" => "ins",
        "u" => "u",
        "strike" => "strike",
        "dl" => "dl",
        "dt" => "dt",
        "dd" => "dd",
        "caption" => "caption",
        "tfoot" => "tfoot",
        "abbr" => "abbr",
        "cite" => "cite",
        "time" => "time",
        "kbd" => "kbd",
        "samp" => "samp",
        "var" => "var",
        "details" => "details",
        "summary" => "summary",
        "article" => "article",
        "section" => "section",
        "nav" => "nav",
        "aside" => "aside",
        "header" => "header",
        "footer" => "footer",
        "main" => "main",
        "address" => "address",
        "dfn" => "dfn",
        "q" => "q",
        "ruby" => "ruby",
        "rt" => "rt",
        "rp" => "rp",
        "bdi" => "bdi",
        "bdo" => "bdo",
        "wbr" => "wbr",
        "dialog" => "dialog",
        "meter" => "meter",
        "progress" => "progress",
        "output" => "output",
        "data" => "data",
        "datalist" => "datalist",
        "fieldset" => "fieldset",
        "legend" => "legend",
        "optgroup" => "optgroup",
        "menu" => "menu",
        "search" => "search",
        "slot" => "slot",
        "template" => "template",
        "portal" => "portal",
        "video" => "video",
        "audio" => "audio",
        "source" => "source",
        "track" => "track",
        "picture" => "picture",
        "col" => "col",
        "colgroup" => "colgroup",
        "area" => "area",
        "map" => "map",
        "embed" => "embed",
        "object" => "object",
        "param" => "param",
        // SVG elements
        "circle" => "circle",
        "ellipse" => "ellipse",
        "rect" => "rect",
        "g" => "g",
        "polygon" => "polygon",
        "polyline" => "polyline",
        "line" => "line",
        "text" => "text",
        "tspan" => "tspan",
        "defs" => "defs",
        "use" => "use",
        "symbol" => "symbol",
        "clipPath" => "clipPath",
        "mask" => "mask",
        "pattern" => "pattern",
        "linearGradient" => "linearGradient",
        "radialGradient" => "radialGradient",
        "stop" => "stop",
        "foreignObject" => "foreignObject",
        // Legacy HTML tags
        "iframe" => "iframe",
        "center" => "center",
        "font" => "font",
        "tt" => "tt",
        "big" => "big",
        "nobr" => "nobr",
        "marquee" => "marquee",
        "blink" => "blink",
        // MathML elements
        "math" => "math",
        "mrow" => "mrow",
        "mi" => "mi",
        "mn" => "mn",
        "mo" => "mo",
        "ms" => "ms",
        "mspace" => "mspace",
        "mtext" => "mtext",
        "mfrac" => "mfrac",
        "msqrt" => "msqrt",
        "mroot" => "mroot",
        "mstyle" => "mstyle",
        "merror" => "merror",
        "mpadded" => "mpadded",
        "mphantom" => "mphantom",
        "mtable" => "mtable",
        "mtr" => "mtr",
        "mtd" => "mtd",
        // Standard structural tags
        "html" => "html",
        "body" => "body",
        "head" => "head",
        "title" => "title",
        "meta" => "meta",
        "link" => "link",
        "style" => "style",
        "script" => "script",
        _ => "custom",
    }
}

fn map_tag_to_strata_tag(tag: &str) -> Tag {
    match tag {
        "html" => Tag::Div,
        "body" => Tag::Div,
        "div" => Tag::Div,
        "span" => Tag::Span,
        "p" => Tag::P,
        "h1" => Tag::H1,
        "h2" => Tag::H2,
        "h3" => Tag::H3,
        "h4" => Tag::H4,
        "h5" => Tag::H5,
        "h6" => Tag::H6,
        "button" => Tag::Button,
        "input" => Tag::Input,
        "select" => Tag::Select,
        "option" => Tag::Option,
        "textarea" => Tag::Textarea,
        "label" => Tag::Label,
        "form" => Tag::Form,
        "header" => Tag::Header,
        "nav" => Tag::Nav,
        "main" => Tag::Main,
        "section" => Tag::Section,
        "aside" => Tag::Aside,
        "footer" => Tag::Footer,
        "ul" => Tag::Ul,
        "ol" => Tag::Ol,
        "li" => Tag::Li,
        "a" => Tag::A,
        "img" => Tag::Img,
        "svg" => Tag::Svg,
        "path" => Tag::Path,
        "canvas" => Tag::Canvas,
        "table" => Tag::Table,
        "thead" => Tag::Thead,
        "tbody" => Tag::Tbody,
        "tr" => Tag::Tr,
        "th" => Tag::Th,
        "td" => Tag::Td,
        "hr" => Tag::Hr,
        "br" => Tag::Br,
        _ => Tag::Custom(intern_static_tag(tag)),
    }
}

fn is_blueprint_inline(bp: &NodeBlueprint) -> bool {
    match bp {
        NodeBlueprint::Text(_) | NodeBlueprint::DynamicText(_) => true,
        NodeBlueprint::Element {
            attributes,
            tag,
            classes,
            ..
        } => {
            for a in attributes {
                if let Attribute::DirectStyle(any) = a {
                    if let Some(cs) = any.downcast_ref::<ComputedStyle>() {
                        return matches!(
                            cs.display,
                            Display::Inline
                                | Display::InlineBlock
                                | Display::InlineFlex
                                | Display::InlineTable
                        );
                    }
                }
            }
            if classes.iter().any(|c| {
                c.split_whitespace()
                    .any(|tok| tok == "inline" || tok == "inline-block" || tok == "inline-flex")
            }) {
                return true;
            }
            matches!(
                tag,
                Tag::Span
                    | Tag::A
                    | Tag::Br
                    | Tag::Custom(
                        "b" | "i"
                            | "em"
                            | "strong"
                            | "small"
                            | "sub"
                            | "sup"
                            | "mark"
                            | "u"
                            | "s"
                            | "del"
                            | "ins"
                            | "code"
                            | "abbr"
                            | "cite"
                            | "kbd"
                            | "samp"
                            | "var"
                            | "time"
                            | "dfn"
                            | "q"
                    )
            )
        }
        _ => false,
    }
}

fn blueprint_ends_with_space(bp: &NodeBlueprint) -> bool {
    match bp {
        NodeBlueprint::Text(t) => t.ends_with(' '),
        NodeBlueprint::Element { children, .. } | NodeBlueprint::Fragment(children) => {
            children.last().is_some_and(blueprint_ends_with_space)
        }
        _ => false,
    }
}

/// Returns whether a character is an HTML standard ASCII whitespace character.
#[inline]
pub fn is_html_whitespace(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0C')
}

fn normalize_inline_text(text: &str) -> String {
    let first_non_ws = text.chars().find(|c| !is_html_whitespace(*c));
    let is_leading_punct = matches!(
        first_non_ws,
        Some(',' | '.' | ';' | ':' | '!' | '?' | ')' | ']' | '}')
    );
    let has_leading_space =
        text.chars().next().is_some_and(is_html_whitespace) && !is_leading_punct;
    let has_trailing_space = text.chars().last().is_some_and(is_html_whitespace);

    let mut out = String::with_capacity(text.len());
    if has_leading_space {
        out.push(' ');
    }

    let mut in_whitespace = false;
    let mut has_non_ws = false;

    for ch in text.chars() {
        if is_html_whitespace(ch) {
            if !in_whitespace && has_non_ws {
                out.push(' ');
            }
            in_whitespace = true;
        } else {
            out.push(ch);
            in_whitespace = false;
            has_non_ws = true;
        }
    }

    if !has_non_ws {
        return if has_leading_space || has_trailing_space {
            " ".to_string()
        } else {
            String::new()
        };
    }

    if has_trailing_space && !out.ends_with(' ') {
        out.push(' ');
    }

    out
}

fn format_computed_style_declarations(style: &ComputedStyle, inline_style: Option<&str>) -> String {
    let mut decls = Vec::new();
    if let Some(c) = style.background_color {
        decls.push(format!(
            "background-color: rgba({},{},{},{})",
            c.r,
            c.g,
            c.b,
            c.a as f32 / 255.0
        ));
    }
    if style.color != Color::new(0, 0, 0, 255) {
        decls.push(format!(
            "color: rgba({},{},{},{})",
            style.color.r,
            style.color.g,
            style.color.b,
            style.color.a as f32 / 255.0
        ));
    }
    if style.font_size != 16.0 {
        decls.push(format!("font-size: {}px", style.font_size));
    }
    if style.font_weight != FontWeight::Normal {
        decls.push(format!("font-weight: {}", style.font_weight as u16));
    }
    if style.border_width.top > 0.0
        || style.border_width.right > 0.0
        || style.border_width.bottom > 0.0
        || style.border_width.left > 0.0
    {
        decls.push(format!(
            "border-width: {}px {}px {}px {}px",
            style.border_width.top,
            style.border_width.right,
            style.border_width.bottom,
            style.border_width.left
        ));
    }
    if let Some(bc) = style.border_color {
        decls.push(format!(
            "border-color: rgba({},{},{},{})",
            bc.r,
            bc.g,
            bc.b,
            bc.a as f32 / 255.0
        ));
    }
    if style.float == Float::Right {
        decls.push("float: right".to_string());
    } else if style.float == Float::Left {
        decls.push("float: left".to_string());
    }
    if style.clear == Clear::Both {
        decls.push("clear: both".to_string());
    } else if style.clear == Clear::Left {
        decls.push("clear: left".to_string());
    } else if style.clear == Clear::Right {
        decls.push("clear: right".to_string());
    }
    if let Some(inl) = inline_style {
        let trimmed = inl.trim();
        if !trimmed.is_empty() {
            decls.insert(0, trimmed.trim_end_matches(';').to_string());
        }
    }
    decls.join("; ")
}

/// Transpiles semantic HTML AST nodes into native, styled Strata reactive blueprints.
pub struct WebTranspiler<'a> {
    theme: &'a ReaderTheme,
}

impl<'a> WebTranspiler<'a> {
    /// Creates a new transpiler with the specified theme settings.
    pub fn new(theme: &'a ReaderTheme) -> Self {
        Self { theme }
    }

    /// Extracts primary readable content from a raw HTML DOM tree (e.g. MediaWiki, article, main).
    pub fn extract_primary_content(nodes: &[SemanticNode]) -> &[SemanticNode] {
        for node in nodes {
            let parser_outputs = node.find_by_class("mw-parser-output");
            if let Some(po) = parser_outputs.into_iter().max_by_key(|n| match n {
                SemanticNode::Element { children, .. } => children.len(),
                _ => 0,
            }) {
                if let SemanticNode::Element { children, .. } = po {
                    if !children.is_empty() {
                        return children.as_slice();
                    }
                }
            }

            if let Some(article) = node.find_first("article") {
                if let SemanticNode::Element { children, .. } = article {
                    if !children.is_empty() {
                        return children.as_slice();
                    }
                }
            }

            if let Some(main) = node.find_first("main") {
                if let SemanticNode::Element { children, .. } = main {
                    if !children.is_empty() {
                        return children.as_slice();
                    }
                }
            }

            if let Some(c) = node
                .find_by_id("content")
                .or_else(|| node.find_by_id("bodyContent"))
            {
                if let SemanticNode::Element { children, .. } = c {
                    if !children.is_empty() {
                        return children.as_slice();
                    }
                }
            }

            if let Some(body) = node.find_first("body") {
                if let SemanticNode::Element { children, .. } = body {
                    if !children.is_empty() {
                        return children.as_slice();
                    }
                }
            }
        }

        nodes
    }

    /// Extracts and parses all `<style>` stylesheets within the document tree, evaluating media queries.
    pub fn extract_stylesheets(nodes: &[SemanticNode], context: &MediaContext) -> Vec<StyleSheet> {
        let mut sheets = Vec::new();
        Self::collect_stylesheets(nodes, context, &mut sheets);
        sheets
    }

    fn collect_stylesheets(
        nodes: &[SemanticNode],
        context: &MediaContext,
        out: &mut Vec<StyleSheet>,
    ) {
        for node in nodes {
            if let SemanticNode::Element { tag, children, .. } = node {
                if tag.eq_ignore_ascii_case("style") {
                    let mut text = String::new();
                    for child in children {
                        if let SemanticNode::Text(t) = child {
                            text.push_str(t);
                            text.push('\n');
                        }
                    }
                    if !text.trim().is_empty() {
                        if let Ok(sheet) = StyleSheet::parse_with_context(&text, context) {
                            if !sheet.rules.is_empty() {
                                out.push(sheet);
                            }
                        }
                    }
                } else {
                    Self::collect_stylesheets(children, context, out);
                }
            }
        }
    }

    /// Extracts all linked external stylesheet and image resource URLs from the DOM tree for asynchronous prefetching.
    pub fn extract_resource_urls(nodes: &[SemanticNode]) -> Vec<String> {
        let mut urls = Vec::new();
        Self::collect_resource_urls(nodes, &mut urls);
        urls
    }

    fn collect_resource_urls(nodes: &[SemanticNode], out: &mut Vec<String>) {
        for node in nodes {
            if let SemanticNode::Element {
                    tag,
                    attributes,
                    children,
                    ..
                } = node {
                let tag_lower = tag.to_ascii_lowercase();
                if tag_lower == "link" {
                    let is_stylesheet = attributes.iter().any(|(k, v)| {
                        k.eq_ignore_ascii_case("rel") && v.eq_ignore_ascii_case("stylesheet")
                    });
                    if is_stylesheet {
                        if let Some((_, href)) = attributes
                            .iter()
                            .find(|(k, _)| k.eq_ignore_ascii_case("href"))
                        {
                            if !href.trim().is_empty() {
                                out.push(href.trim().to_string());
                            }
                        }
                    }
                } else if tag_lower == "img" {
                    if let Some((_, src)) = attributes
                        .iter()
                        .find(|(k, _)| k.eq_ignore_ascii_case("src"))
                    {
                        if !src.trim().is_empty() {
                            out.push(src.trim().to_string());
                        }
                    }
                }
                Self::collect_resource_urls(children, out);
            }
        }
    }

    /// Extracts all linked external stylesheet URLs (<link rel="stylesheet" href="...">) from the DOM tree.
    pub fn extract_stylesheet_urls(nodes: &[SemanticNode]) -> Vec<String> {
        let mut urls = Vec::new();
        Self::collect_stylesheet_urls(nodes, &mut urls);
        urls
    }

    fn collect_stylesheet_urls(nodes: &[SemanticNode], out: &mut Vec<String>) {
        for node in nodes {
            if let SemanticNode::Element {
                    tag,
                    attributes,
                    children,
                    ..
                } = node {
                let tag_lower = tag.to_ascii_lowercase();
                if tag_lower == "link" {
                    let is_stylesheet = attributes.iter().any(|(k, v)| {
                        k.eq_ignore_ascii_case("rel")
                            && v.split_whitespace()
                                .any(|r| r.eq_ignore_ascii_case("stylesheet"))
                    });
                    if is_stylesheet {
                        if let Some((_, href)) = attributes
                            .iter()
                            .find(|(k, _)| k.eq_ignore_ascii_case("href"))
                        {
                            if !href.trim().is_empty() {
                                out.push(href.trim().to_string());
                            }
                        }
                    }
                }
                Self::collect_stylesheet_urls(children, out);
            }
        }
    }

    /// Extracts the active label to display for a <select> element based on selected/value attributes or first option.
    pub fn extract_select_label(
        children: &[SemanticNode],
        attributes: &[(String, String)],
    ) -> String {
        let current_val = attributes
            .iter()
            .find(|(k, _)| {
                k.eq_ignore_ascii_case("value") || k.eq_ignore_ascii_case("data-current-lang")
            })
            .map(|(_, v)| v.as_str());

        let mut first_label = None;
        let mut selected_label = None;

        for child in children {
            if let SemanticNode::Element {
                tag,
                attributes: opt_attrs,
                children: opt_children,
                ..
            } = child
            {
                if tag.eq_ignore_ascii_case("option") {
                    let mut text = String::new();
                    for c in opt_children {
                        if let SemanticNode::Text(t) = c {
                            text.push_str(t);
                        }
                    }
                    let label = if !text.trim().is_empty() {
                        text.trim().to_string()
                    } else if let Some((_, title)) = opt_attrs
                        .iter()
                        .find(|(k, _)| k.eq_ignore_ascii_case("title"))
                    {
                        title.clone()
                    } else if let Some((_, val)) = opt_attrs
                        .iter()
                        .find(|(k, _)| k.eq_ignore_ascii_case("value"))
                    {
                        val.clone()
                    } else {
                        String::new()
                    };

                    let is_selected = opt_attrs
                        .iter()
                        .any(|(k, _)| k.eq_ignore_ascii_case("selected"))
                        || current_val.is_some_and(|cv| {
                            opt_attrs.iter().any(|(k, v)| {
                                k.eq_ignore_ascii_case("value") && v.eq_ignore_ascii_case(cv)
                            })
                        });

                    if is_selected {
                        selected_label = Some(label.clone());
                    }
                    if first_label.is_none() && !label.is_empty() {
                        first_label = Some(label);
                    }
                }
            }
        }

        selected_label
            .or(first_label)
            .unwrap_or_else(|| "Select...".to_string())
    }

    /// Recursively locates the marked document `html`/`body` blueprints and
    /// records their painted background colors for canvas propagation.
    fn collect_canvas_backgrounds(
        blueprint: &NodeBlueprint,
        html_bg: &mut Option<Color>,
        body_bg: &mut Option<Color>,
    ) {
        match blueprint {
            NodeBlueprint::Element {
                attributes,
                children,
                ..
            } => {
                let mut marker = None;
                let mut background = None;
                for attr in attributes {
                    match attr {
                        Attribute::Property { key, value } if key == "data-strata-tag" => {
                            marker = Some(value.as_str());
                        }
                        Attribute::DirectStyle(any) => {
                            if let Some(style) = any.downcast_ref::<ComputedStyle>() {
                                background = style.background_color;
                            }
                        }
                        _ => {}
                    }
                }
                if let (Some(tag), Some(bg)) = (marker, background) {
                    if bg.a > 0 {
                        match tag {
                            "html" if html_bg.is_none() => *html_bg = Some(bg),
                            "body" if body_bg.is_none() => *body_bg = Some(bg),
                            _ => {}
                        }
                    }
                }
                for child in children {
                    Self::collect_canvas_backgrounds(child, html_bg, body_bg);
                }
            }
            NodeBlueprint::Fragment(children) => {
                for child in children {
                    Self::collect_canvas_backgrounds(child, html_bg, body_bg);
                }
            }
            NodeBlueprint::Text(_) | NodeBlueprint::DynamicText(_) | NodeBlueprint::Empty => {}
        }
    }

    /// Transpiles the complete unmutated DOM tree into a full-width Chromium-parity blueprint.
    pub fn transpile_full_document(
        &self,
        nodes: &[SemanticNode],
        media_context: &MediaContext,
    ) -> NodeBlueprint {
        let stylesheets = Self::extract_stylesheets(nodes, media_context);

        let is_dark = media_context.is_dark;
        let pseudo_state = PseudoState {
            is_dark,
            ..Default::default()
        };

        let mut root_style = ComputedStyle::default();
        root_style.display = Display::Block;
        root_style.width = Dimension::Percent(100.0);
        root_style.min_height = Dimension::Percent(100.0);
        // Normal page rendering uses the W3C user-agent canvas (white background,
        // black text), independent of any reader theme. Author CSS still wins.
        let page_canvas = Color::new(255, 255, 255, 255);
        root_style.color = Color::new(0, 0, 0, 255);
        root_style.background_color = Some(page_canvas);
        root_style.font_family = strata_primitives::AtomId::FONT_SANS;
        root_style.font_size = 16.0;
        // Chromium's CSS initial `line-height` is `normal`, which resolves to about
        // 1.2 for the pinned DejaVu Sans stack used by the parity harness. The
        // generic browser path must inherit that line box; the distraction-free
        // reader keeps its own 1.5 leading in `transpile_document_with_roots`.
        root_style.line_height = 1.2;

        let mut ancestors = Vec::new();
        let children = self.transpile_children_slice(
            nodes,
            &mut ancestors,
            &stylesheets,
            Some(&root_style),
            &pseudo_state,
            page_canvas,
            true,
        );

        // Chromium propagates the author `html` background, else the author `body`
        // background, onto the document canvas. The emitted child blueprints carry
        // a marker attribute so the exact cascaded color can be recovered here.
        let mut html_canvas_bg = None;
        let mut body_canvas_bg = None;
        for child in &children {
            Self::collect_canvas_backgrounds(child, &mut html_canvas_bg, &mut body_canvas_bg);
        }
        if let Some(canvas_bg) = html_canvas_bg.or(body_canvas_bg) {
            root_style.background_color = Some(composite_over_white(canvas_bg));
        }

        let root_attrs = vec![
            Attribute::property("class", "strata-html-root w-full min-h-full block"),
            Attribute::direct_style(root_style),
        ];

        NodeBlueprint::element(
            Tag::Div,
            vec!["w-full min-h-full block".to_string()],
            root_attrs,
            children,
        )
    }

    /// Transpiles a slice of top-level semantic nodes into a complete article container blueprint.
    pub fn transpile_document(&self, nodes: &[SemanticNode], title: Option<&str>) -> NodeBlueprint {
        self.transpile_document_with_roots(nodes, nodes, title)
    }

    /// Transpiles primary content nodes into an article blueprint while extracting stylesheets from the root document nodes.
    pub fn transpile_document_with_roots(
        &self,
        nodes: &[SemanticNode],
        root_nodes_for_styles: &[SemanticNode],
        title: Option<&str>,
    ) -> NodeBlueprint {
        let media_ctx = MediaContext::new(
            self.theme.max_width_px as f32,
            800.0,
            self.theme.name != "Sepia",
        );
        let mut stylesheets = Self::extract_stylesheets(root_nodes_for_styles, &media_ctx);
        if root_nodes_for_styles.as_ptr() != nodes.as_ptr() {
            let mut more = Self::extract_stylesheets(nodes, &media_ctx);
            stylesheets.append(&mut more);
        }

        let is_dark = self.theme.name != "Sepia";
        let pseudo_state = PseudoState {
            is_dark,
            ..Default::default()
        };

        let mut root_style = ComputedStyle::default();
        root_style.display = Display::Block;
        root_style.color = self.theme.text_color_rgba();
        root_style.background_color = Some(self.theme.bg_color_rgba());
        root_style.font_family = strata_primitives::AtomId::FONT_SANS;
        root_style.font_size = 16.0;
        root_style.line_height = 1.5;

        let mut children = Vec::new();

        if let Some(t) = title {
            let mut h1_style = ComputedStyle::inherit_from(&root_style);
            h1_style.display = Display::Block;
            h1_style.font_size = 32.0;
            h1_style.font_weight = FontWeight::Bold;
            h1_style.margin.bottom = 24.0;

            children.push(NodeBlueprint::element(
                Tag::H1,
                vec![format!(
                    "text-3xl sm:text-4xl font-extrabold tracking-tight mb-6 {}",
                    self.theme.heading_class
                )],
                vec![Attribute::direct_style(h1_style)],
                vec![NodeBlueprint::text(t)],
            ));
        }

        let mut ancestors = Vec::new();
        let mut transpiled_nodes = self.transpile_children_slice(
            nodes,
            &mut ancestors,
            &stylesheets,
            Some(&root_style),
            &pseudo_state,
            self.theme.bg_color_rgba(),
            false,
        );
        children.append(&mut transpiled_nodes);

        let final_children = if children.len() > 40 {
            let mut chunked = Vec::new();
            let mut current_chunk = Vec::new();
            for child in children {
                current_chunk.push(child);
                if current_chunk.len() >= 25 {
                    let mut chunk_style = ComputedStyle::inherit_from(&root_style);
                    chunk_style.display = Display::Block;
                    chunk_style.width = Dimension::Percent(100.0);

                    chunked.push(NodeBlueprint::element(
                        Tag::Div,
                        vec!["block w-full strata-content-chunk".to_string()],
                        vec![Attribute::direct_style(chunk_style)],
                        std::mem::take(&mut current_chunk),
                    ));
                }
            }
            if !current_chunk.is_empty() {
                let mut chunk_style = ComputedStyle::inherit_from(&root_style);
                chunk_style.display = Display::Block;
                chunk_style.width = Dimension::Percent(100.0);

                chunked.push(NodeBlueprint::element(
                    Tag::Div,
                    vec!["block w-full strata-content-chunk".to_string()],
                    vec![Attribute::direct_style(chunk_style)],
                    current_chunk,
                ));
            }
            chunked
        } else {
            children
        };

        let container_style = format!(
            "relative block w-full max-w-[{}px] mx-auto px-6 py-10 {} {}",
            self.theme.max_width_px, self.theme.bg_class, self.theme.text_class
        );

        let mut container_computed = root_style.clone();
        container_computed.position = Position::Relative;
        container_computed.max_width = Dimension::Px(self.theme.max_width_px as f32);
        container_computed.margin_auto_left = true;
        container_computed.margin_auto_right = true;
        container_computed.padding.left = 24.0;
        container_computed.padding.right = 24.0;
        container_computed.padding.top = 40.0;
        container_computed.padding.bottom = 40.0;

        NodeBlueprint::element(
            Tag::Div,
            vec![container_style],
            vec![Attribute::direct_style(container_computed)],
            final_children,
        )
    }

    #[allow(dead_code)]
    fn is_inline_phrasing(node: &SemanticNode) -> bool {
        match node {
            SemanticNode::Text(_) | SemanticNode::Comment(_) => true,
            SemanticNode::Element { tag, children, .. } => {
                matches!(
                    tag.as_str(),
                    "a" | "b"
                        | "strong"
                        | "i"
                        | "em"
                        | "u"
                        | "s"
                        | "span"
                        | "code"
                        | "small"
                        | "sub"
                        | "sup"
                        | "mark"
                        | "abbr"
                        | "cite"
                        | "q"
                        | "dfn"
                        | "var"
                        | "samp"
                        | "kbd"
                        | "time"
                        | "bdi"
                        | "bdo"
                        | "wbr"
                ) && children.iter().all(Self::is_inline_phrasing)
            }
        }
    }

    fn transpile_children_slice(
        &self,
        nodes: &[SemanticNode],
        ancestors: &mut Vec<ElementContext>,
        stylesheets: &[StyleSheet],
        parent_style: Option<&ComputedStyle>,
        state: &PseudoState,
        effective_bg: Color,
        accurate_document: bool,
    ) -> Vec<NodeBlueprint> {
        let mut raw_blueprints = Vec::new();
        let mut child_siblings: Vec<ElementContext> = Vec::new();

        for (idx, c) in nodes.iter().enumerate() {
            match c {
                SemanticNode::Text(t) => {
                    let is_pre = parent_style.is_some_and(|p| {
                        matches!(p.white_space, WhiteSpace::Pre | WhiteSpace::PreWrap)
                    });
                    if is_pre {
                        if !t.is_empty() {
                            raw_blueprints.push(NodeBlueprint::text(t.clone()));
                            child_siblings.push(ElementContext {
                                tag: "#text".to_string(),
                                classes: Vec::new(),
                                id: None,
                                attributes: Vec::new(),
                            });
                        }
                    } else if t.trim_matches(is_html_whitespace).is_empty() {
                        let is_inline_container = parent_style.is_some_and(|p| {
                            matches!(p.display, Display::Inline | Display::InlineBlock)
                        });
                        let prev_is_inline =
                            raw_blueprints.last().is_some_and(is_blueprint_inline);
                        let prev_ends_space = raw_blueprints
                            .last()
                            .is_some_and(blueprint_ends_with_space);
                        let next_is_inline =
                            nodes.get(idx + 1).is_some_and(Self::is_inline_phrasing);

                        if (is_inline_container && !t.is_empty())
                            || (prev_is_inline && next_is_inline && !prev_ends_space)
                        {
                            raw_blueprints.push(NodeBlueprint::text(" "));
                            child_siblings.push(ElementContext {
                                tag: "#text".to_string(),
                                classes: Vec::new(),
                                id: None,
                                attributes: Vec::new(),
                            });
                        }
                    } else {
                        let normalized = normalize_inline_text(t);
                        if !normalized.is_empty() {
                            raw_blueprints.push(NodeBlueprint::text(normalized));
                            child_siblings.push(ElementContext {
                                tag: "#text".to_string(),
                                classes: Vec::new(),
                                id: None,
                                attributes: Vec::new(),
                            });
                        }
                    }
                }
                SemanticNode::Comment(_) => {}
                SemanticNode::Element { .. } => {
                    if let Some((mut bp, ctx)) = self.transpile_node_internal(
                        c,
                        ancestors,
                        &child_siblings,
                        stylesheets,
                        parent_style,
                        state,
                        effective_bg,
                        accurate_document,
                    ) {
                        if let Some(parent_ctx) = ancestors.last() {
                            let is_cslist = parent_ctx
                                .classes
                                .iter()
                                .any(|cls| cls == "cslist" || cls == "cslist-embedded");
                            let is_sslist = parent_ctx.classes.iter().any(|cls| cls == "sslist");
                            let is_andlist = parent_ctx
                                .classes
                                .iter()
                                .any(|cls| cls == "andlist" || cls == "andlistoxford");

                            if (is_cslist || is_sslist || is_andlist)
                                && c.tag().is_some_and(|t| t.eq_ignore_ascii_case("li"))
                            {
                                let li_count_after = nodes[idx + 1..]
                                    .iter()
                                    .filter(|next_c| match next_c {
                                        SemanticNode::Element { tag: nt, .. } => {
                                            nt.eq_ignore_ascii_case("li")
                                        }
                                        _ => false,
                                    })
                                    .count();
                                if li_count_after > 0 {
                                    let sep = if is_sslist {
                                        "; "
                                    } else if is_andlist && li_count_after == 1 {
                                        ", and "
                                    } else {
                                        ", "
                                    };
                                    if let NodeBlueprint::Element {
                                            children: bp_children,
                                            ..
                                        } = &mut bp {
                                        bp_children.push(NodeBlueprint::text(sep));
                                    }
                                }
                            }
                        }
                        raw_blueprints.push(bp);
                        child_siblings.push(ctx);
                    }
                }
            }
        }

        // Apply W3C Inline Formatting Context (IFC) anonymous block restructuring:
        // When parent is a block-level container, and children are a mix of inlines and blocks,
        // group contiguous runs of inlines into anonymous block containers!
        let is_block_container = parent_style.is_none_or(|p| {
            matches!(
                p.display,
                Display::Block
                    | Display::ListItem
                    | Display::TableCell
                    | Display::TableHeaderGroup
                    | Display::TableRowGroup
                    | Display::TableFooterGroup
                    | Display::TableCaption
            )
        });

        if is_block_container && !raw_blueprints.is_empty() {
            let has_blocks = raw_blueprints.iter().any(|bp| !is_blueprint_inline(bp));
            let has_inlines = raw_blueprints.iter().any(is_blueprint_inline);

            if has_blocks && has_inlines {
                let mut structured = Vec::new();
                let mut current_inline_run: Vec<NodeBlueprint> = Vec::new();

                for bp in raw_blueprints {
                    if is_blueprint_inline(&bp) {
                        current_inline_run.push(bp);
                    } else {
                        if !current_inline_run.is_empty() {
                            let mut anon_style = match parent_style {
                                Some(p) => ComputedStyle::inherit_from(p),
                                None => ComputedStyle::default(),
                            };
                            anon_style.display = Display::Block;
                            anon_style.width = Dimension::Percent(100.0);
                            anon_style.margin = strata_primitives::geom::EdgeInsets::all(0.0);
                            anon_style.padding = strata_primitives::geom::EdgeInsets::all(0.0);
                            anon_style.border_width = strata_primitives::geom::EdgeInsets::all(0.0);

                            structured.push(NodeBlueprint::element(
                                Tag::Div,
                                vec!["block strata-anon-block".to_string()],
                                vec![Attribute::direct_style(anon_style)],
                                std::mem::take(&mut current_inline_run),
                            ));
                        }
                        structured.push(bp);
                    }
                }

                if !current_inline_run.is_empty() {
                    let mut anon_style = match parent_style {
                        Some(p) => ComputedStyle::inherit_from(p),
                        None => ComputedStyle::default(),
                    };
                    anon_style.display = Display::Block;
                    anon_style.width = Dimension::Percent(100.0);
                    anon_style.margin = strata_primitives::geom::EdgeInsets::all(0.0);
                    anon_style.padding = strata_primitives::geom::EdgeInsets::all(0.0);
                    anon_style.border_width = strata_primitives::geom::EdgeInsets::all(0.0);

                    structured.push(NodeBlueprint::element(
                        Tag::Div,
                        vec!["block strata-anon-block".to_string()],
                        vec![Attribute::direct_style(anon_style)],
                        current_inline_run,
                    ));
                }

                return structured;
            }
        }

        raw_blueprints
    }

    /// Recursively transpiles a single `SemanticNode` into an optional `NodeBlueprint`.
    pub fn transpile_node(&self, node: &SemanticNode) -> Option<NodeBlueprint> {
        let mut ancestors = Vec::new();
        let pseudo_state = PseudoState {
            is_dark: self.theme.name != "Sepia",
            ..Default::default()
        };
        self.transpile_node_internal(
            node,
            &mut ancestors,
            &[],
            &[],
            None,
            &pseudo_state,
            self.theme.bg_color_rgba(),
            false,
        )
        .map(|(bp, _)| bp)
    }

    fn transpile_node_internal(
        &self,
        node: &SemanticNode,
        ancestors: &mut Vec<ElementContext>,
        preceding_siblings: &[ElementContext],
        stylesheets: &[StyleSheet],
        parent_style: Option<&ComputedStyle>,
        state: &PseudoState,
        effective_bg: Color,
        accurate_document: bool,
    ) -> Option<(NodeBlueprint, ElementContext)> {
        match node {
            SemanticNode::Text(text) => {
                let normalized = normalize_inline_text(text);
                if normalized.is_empty() {
                    None
                } else {
                    Some((
                        NodeBlueprint::text(normalized),
                        ElementContext {
                            tag: "#text".to_string(),
                            classes: Vec::new(),
                            id: None,
                            attributes: Vec::new(),
                        },
                    ))
                }
            }
            SemanticNode::Comment(_) => None,
            SemanticNode::Element {
                tag,
                attributes,
                children,
            } => {
                let tag_lower = tag.to_ascii_lowercase();

                // Skip non-rendering document tags
                if matches!(
                    tag_lower.as_str(),
                    "style" | "script" | "meta" | "link" | "head" | "option" | "optgroup"
                ) {
                    return None;
                }

                // Skip hidden HTML elements
                for (k, v) in attributes {
                    if k.eq_ignore_ascii_case("hidden") {
                        return None;
                    }
                    if k.eq_ignore_ascii_case("style") {
                        let lower = v.to_lowercase().replace(' ', "");
                        if lower.contains("display:none") {
                            return None;
                        }
                    }
                    if k.eq_ignore_ascii_case("class") {
                        let has_hidden_class = v
                            .split_whitespace()
                            .any(|c| c == "hidden" || c == "sr-only" || c == "visually-hidden");
                        if has_hidden_class {
                            return None;
                        }
                    }
                }

                let mut class_tokens = Vec::new();
                let mut id_opt = None;
                let mut inline_style_opt = None;
                let mut align_attr = None;
                let mut valign_attr = None;
                let mut colspan_val = None;
                let mut rowspan_val = None;
                let mut width_attr = None;
                let mut height_attr = None;
                let mut bgcolor_attr = None;
                let mut border_attr = None;
                let mut cellpadding_attr = None;
                let mut cellspacing_attr = None;
                let mut color_attr = None;
                let mut size_attr = None;

                for (k, v) in attributes {
                    let k_lower = k.to_ascii_lowercase();
                    match k_lower.as_str() {
                        "class" => {
                            for tok in v.split_whitespace() {
                                class_tokens.push(tok.to_string());
                            }
                        }
                        "id" => id_opt = Some(v.as_str()),
                        "style" => inline_style_opt = Some(v.as_str()),
                        "align" => align_attr = Some(v.as_str()),
                        "valign" => valign_attr = Some(v.as_str()),
                        "colspan" => colspan_val = v.parse::<u32>().ok(),
                        "rowspan" => rowspan_val = v.parse::<u32>().ok(),
                        "width" => width_attr = Some(v.as_str()),
                        "height" => height_attr = Some(v.as_str()),
                        "bgcolor" => bgcolor_attr = Some(v.as_str()),
                        "border" => border_attr = Some(v.as_str()),
                        "cellpadding" => cellpadding_attr = v.parse::<f32>().ok(),
                        "cellspacing" => cellspacing_attr = v.parse::<f32>().ok(),
                        "color" => color_attr = Some(v.as_str()),
                        "size" => size_attr = Some(v.as_str()),
                        _ => {}
                    }
                }

                let cur_ref = ElementRef {
                    tag: tag_lower.as_str(),
                    classes: &class_tokens,
                    id: id_opt,
                    attributes,
                };

                let anc_refs: Vec<ElementRef> = ancestors
                    .iter()
                    .map(|a| ElementRef {
                        tag: a.tag.as_str(),
                        classes: &a.classes,
                        id: a.id.as_deref(),
                        attributes: &a.attributes,
                    })
                    .collect();

                let sib_slice = if preceding_siblings.len() > 8 {
                    &preceding_siblings[preceding_siblings.len() - 8..]
                } else {
                    preceding_siblings
                };
                let sib_refs: Vec<ElementRef> = sib_slice
                    .iter()
                    .map(|s| ElementRef {
                        tag: s.tag.as_str(),
                        classes: &s.classes,
                        id: s.id.as_deref(),
                        attributes: &s.attributes,
                    })
                    .collect();

                let sheet_refs: Vec<&StyleSheet> = stylesheets.iter().collect();

                // Compute exact styles using multi-origin cascade engine (UA + Author + Inline + Inheritance)
                let mut computed_style = CascadeSolver::resolve_with_hierarchy(
                    &cur_ref,
                    &anc_refs,
                    &sib_refs,
                    inline_style_opt,
                    &sheet_refs,
                    parent_style,
                    state,
                );

                // The transpiled blueprint carries a resolved `ComputedStyle`,
                // so emulated utility classes must be folded into it here.
                // Author inline declarations retain the highest precedence.
                for class_token in &class_tokens {
                    strata_style::tailwind::TailwindEngine::apply_class_with_state(
                        &mut computed_style,
                        class_token,
                        state.is_hovered,
                        state.is_focused,
                        state.is_dark,
                    );
                }
                if let Some(inline) = inline_style_opt {
                    for decl in strata_style::parse_declarations(inline) {
                        CascadeSolver::apply_declaration_with_parent(
                            &mut computed_style,
                            &decl,
                            parent_style,
                        );
                    }
                }

                if computed_style.display == Display::None {
                    return None;
                }

                // Handle presentational HTML attributes
                if let Some(cs) = colspan_val {
                    computed_style.colspan = cs.max(1);
                }
                if let Some(rs) = rowspan_val {
                    computed_style.rowspan = rs.max(1);
                }
                if let Some(align) = align_attr {
                    if align.eq_ignore_ascii_case("center") {
                        computed_style.text_align = TextAlign::Center;
                    } else if align.eq_ignore_ascii_case("right") {
                        computed_style.text_align = TextAlign::Right;
                    } else if align.eq_ignore_ascii_case("left") {
                        computed_style.text_align = TextAlign::Left;
                    }
                }
                if let Some(valign) = valign_attr {
                    if valign.eq_ignore_ascii_case("top") {
                        computed_style.vertical_align = VerticalAlign::Top;
                    } else if valign.eq_ignore_ascii_case("middle") {
                        computed_style.vertical_align = VerticalAlign::Middle;
                    } else if valign.eq_ignore_ascii_case("bottom") {
                        computed_style.vertical_align = VerticalAlign::Bottom;
                    }
                }

                // Generic width & height attribute resolution for all elements (px and percentage)
                if let Some(w_str) = width_attr {
                    if computed_style.width == Dimension::Auto {
                        let trimmed = w_str.trim();
                        if trimmed.ends_with('%') {
                            if let Ok(pct) = trimmed.trim_end_matches('%').parse::<f32>() {
                                computed_style.width = Dimension::Percent(pct);
                            }
                        } else if let Ok(w) = trimmed.trim_end_matches("px").parse::<f32>() {
                            computed_style.width = Dimension::Px(w);
                        }
                    }
                }
                if let Some(h_str) = height_attr {
                    if computed_style.height == Dimension::Auto {
                        let trimmed = h_str.trim();
                        if trimmed.ends_with('%') {
                            if let Ok(pct) = trimmed.trim_end_matches('%').parse::<f32>() {
                                computed_style.height = Dimension::Percent(pct);
                            }
                        } else if let Ok(h) = trimmed.trim_end_matches("px").parse::<f32>() {
                            computed_style.height = Dimension::Px(h);
                        }
                    }
                }

                // Handle bgcolor and ensure text contrast
                if let Some(bg_str) = bgcolor_attr {
                    if let Some(c) = strata_style::parse_color(bg_str) {
                        computed_style.background_color = Some(c);
                        let lum =
                            0.299 * (c.r as f32) + 0.587 * (c.g as f32) + 0.114 * (c.b as f32);
                        if lum > 140.0
                            && computed_style.color.r > 200
                            && computed_style.color.g > 200
                            && computed_style.color.b > 200
                        {
                            computed_style.color = Color::new(34, 34, 34, 255);
                        }
                    }
                }

                // Handle <font color="..."> and <font size="...">
                if let Some(col_str) = color_attr {
                    if let Some(c) = strata_style::parse_color(col_str) {
                        computed_style.color = c;
                    }
                }
                if tag_lower == "font" {
                    if let Some(sz_str) = size_attr {
                        let px = match sz_str.trim() {
                            "1" => 10.0,
                            "2" => 13.0,
                            "3" => 16.0,
                            "4" => 18.0,
                            "5" => 24.0,
                            "6" => 32.0,
                            "7" => 48.0,
                            other => other.trim_end_matches("px").parse::<f32>().unwrap_or(16.0),
                        };
                        computed_style.font_size = px;
                    }
                }

                // Handle border, cellpadding, and cellspacing
                if let Some(b_str) = border_attr {
                    if let Ok(b) = b_str.trim_end_matches("px").parse::<f32>() {
                        if b > 0.0 {
                            computed_style.border_width =
                                strata_primitives::geom::EdgeInsets::all(b);
                            if computed_style.border_color.is_none() {
                                computed_style.border_color = Some(Color::new(180, 180, 180, 255));
                            }
                        } else {
                            computed_style.border_width = strata_primitives::geom::EdgeInsets::ZERO;
                        }
                    }
                }
                if let Some(sp) = cellspacing_attr {
                    computed_style.border_spacing = (sp, sp);
                }
                if tag_lower == "table" {
                    if let Some(pad) = cellpadding_attr {
                        computed_style.padding = strata_primitives::geom::EdgeInsets::all(pad);
                    }
                }
                if tag_lower == "td" || tag_lower == "th" {
                    if let Some(table_anc) = ancestors.iter().rev().find(|a| a.tag == "table") {
                        if let Some((_, cp)) = table_anc
                            .attributes
                            .iter()
                            .find(|(k, _)| k.eq_ignore_ascii_case("cellpadding"))
                        {
                            if let Ok(pad) = cp.parse::<f32>() {
                                if computed_style.padding
                                    == strata_primitives::geom::EdgeInsets::ZERO
                                    || computed_style.padding
                                        == strata_primitives::geom::EdgeInsets::all(1.0)
                                {
                                    computed_style.padding =
                                        strata_primitives::geom::EdgeInsets::all(pad);
                                }
                            }
                        }
                    }
                }

                // Handle <center>
                if tag_lower == "center" {
                    computed_style.display = Display::Block;
                    computed_style.text_align = TextAlign::Center;
                    computed_style.margin_auto_left = true;
                    computed_style.margin_auto_right = true;
                }

                let is_infobox = class_tokens
                    .iter()
                    .any(|c| c.eq_ignore_ascii_case("infobox"));

                let is_float_right = is_infobox
                    || tag_lower == "aside"
                    || (matches!(tag_lower.as_str(), "table" | "img")
                        && align_attr.is_some_and(|a| a.eq_ignore_ascii_case("right")))
                    || class_tokens.iter().any(|c| {
                        matches!(
                            c.to_ascii_lowercase().as_str(),
                            "tright" | "floatright" | "float-right"
                        )
                    })
                    || (tag_lower == "figure"
                        && !class_tokens.iter().any(|c| {
                            matches!(
                                c.to_ascii_lowercase().as_str(),
                                "tleft" | "floatleft" | "float-left"
                            )
                        }));

                let is_float_left = (matches!(tag_lower.as_str(), "table" | "img")
                    && align_attr.is_some_and(|a| a.eq_ignore_ascii_case("left")))
                    || class_tokens.iter().any(|c| {
                        matches!(
                            c.to_ascii_lowercase().as_str(),
                            "tleft" | "floatleft" | "float-left"
                        )
                    });

                let is_clear_both = class_tokens.iter().any(|c| {
                    matches!(
                        c.to_ascii_lowercase().as_str(),
                        "clear" | "visualclear" | "clearfix"
                    )
                });

                if is_float_right && computed_style.float == Float::None {
                    computed_style.float = Float::Right;
                }
                if is_float_left && computed_style.float == Float::None {
                    computed_style.float = Float::Left;
                }
                if is_clear_both && computed_style.clear == Clear::None {
                    computed_style.clear = Clear::Both;
                }
                if is_infobox && computed_style.width == Dimension::Auto {
                    computed_style.width = Dimension::Px(352.0);
                }

                // Contrast and Theme Adaptation for Reader Mode
                let is_infobox_part = is_infobox
                    || class_tokens
                        .iter()
                        .any(|c| c.to_ascii_lowercase().contains("infobox"));
                let is_code_part = matches!(tag_lower.as_str(), "code" | "pre" | "samp" | "kbd")
                    || class_tokens.iter().any(|c| {
                        let cl = c.to_ascii_lowercase();
                        cl.contains("highlight") || cl.contains("mw-code") || cl.contains("syntax")
                    });
                let is_wikitable_part = class_tokens
                    .iter()
                    .any(|c| c.to_ascii_lowercase().contains("wikitable"));
                if state.is_dark {
                    if let Some(bg) = computed_style.background_color {
                        let bg_lum =
                            0.299 * (bg.r as f32) + 0.587 * (bg.g as f32) + 0.114 * (bg.b as f32);
                        // In dark reader mode, adapt light infobox/code cards to dark reader palette
                        if (is_infobox_part || is_code_part || is_wikitable_part)
                            && bg_lum > 180.0
                            && bg.a > 128
                        {
                            let new_bg = if tag_lower == "th" {
                                Color::new(30, 41, 59, 255) // slate-800
                            } else if is_code_part {
                                Color::new(24, 32, 47, 255) // slate-900
                            } else {
                                Color::new(17, 24, 39, 255) // neutral-900
                            };
                            computed_style.background_color = Some(new_bg);
                            computed_style.color = Color::new(241, 245, 249, 255);
                            // slate-100
                        }
                    }
                }

                let my_effective_bg = match computed_style.background_color {
                    Some(bg) if bg.a > 50 => bg,
                    _ => effective_bg,
                };
                let eff_bg_lum = 0.299 * (my_effective_bg.r as f32)
                    + 0.587 * (my_effective_bg.g as f32)
                    + 0.114 * (my_effective_bg.b as f32);
                let text_lum = 0.299 * (computed_style.color.r as f32)
                    + 0.587 * (computed_style.color.g as f32)
                    + 0.114 * (computed_style.color.b as f32);

                // Full-document rendering must preserve author CSS and the W3C UA
                // canvas; readability recoloring applies only to reader mode.
                if !accurate_document {
                    if eff_bg_lum > 140.0 {
                        // Light effective background: if text is light, darken for readability
                        if text_lum > 160.0 {
                            computed_style.color = Color::new(34, 34, 34, 255);
                        }
                    } else {
                        // Dark effective background: if text is dark, brighten for readability
                        if state.is_dark && text_lum < 80.0 {
                            if matches!(tag_lower.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
                            {
                                computed_style.color = Color::new(241, 245, 249, 255);
                            // slate-100
                            } else {
                                computed_style.color = self.theme.text_color_rgba();
                            }
                        } else if !state.is_dark && eff_bg_lum < 100.0 && text_lum < 120.0 {
                            computed_style.color = Color::new(248, 250, 252, 255);
                        }
                    }
                }

                let strata_tag = map_tag_to_strata_tag(&tag_lower);

                // Build class list: original classes + utility markers for test and inspection parity
                let mut classes = class_tokens.clone();
                if matches!(strata_tag, Tag::Li) && !classes.iter().any(|c| c == "list-item") {
                    classes.push("list-item".to_string());
                }
                if matches!(
                    computed_style.display,
                    Display::Inline | Display::InlineBlock | Display::InlineFlex
                ) && !classes
                    .iter()
                    .any(|c| c == "inline" || c == "inline-block" || c == "inline-flex")
                {
                    classes.push("inline".to_string());
                }
                if computed_style.float == Float::Right
                    && !classes.iter().any(|c| c == "float-right")
                {
                    classes.push("float-right".to_string());
                } else if computed_style.float == Float::Left
                    && !classes.iter().any(|c| c == "float-left")
                {
                    classes.push("float-left".to_string());
                }
                if is_infobox && !classes.iter().any(|c| c == "w-80") {
                    classes.push("w-80".to_string());
                }
                if tag_lower == "figure" && !classes.iter().any(|c| c.contains("max-w-")) {
                    classes.push("max-w-[300px]".to_string());
                }
                if computed_style.clear == Clear::Both && !classes.iter().any(|c| c == "clear-both")
                {
                    classes.push("clear-both".to_string());
                } else if computed_style.clear == Clear::Left
                    && !classes.iter().any(|c| c == "clear-left")
                {
                    classes.push("clear-left".to_string());
                } else if computed_style.clear == Clear::Right
                    && !classes.iter().any(|c| c == "clear-right")
                {
                    classes.push("clear-right".to_string());
                }

                // Build attributes: original properties + formatted style string + direct typed style
                let mut dom_attrs = Vec::with_capacity(attributes.len() + 2);
                let mut has_style_attr = false;
                for (k, v) in attributes {
                    let k_lower = k.to_ascii_lowercase();
                    if k_lower == "style" {
                        has_style_attr = true;
                        let formatted =
                            format_computed_style_declarations(&computed_style, Some(v));
                        dom_attrs.push(Attribute::property("style", formatted));
                    } else {
                        dom_attrs.push(Attribute::property(k_lower, v.clone()));
                    }
                }
                if !has_style_attr {
                    let formatted = format_computed_style_declarations(&computed_style, None);
                    if !formatted.is_empty() {
                        dom_attrs.push(Attribute::property("style", formatted));
                    }
                }
                dom_attrs.push(Attribute::direct_style(computed_style.clone()));

                // Canvas background propagation markers: only the document root
                // elements need them, so downstream code can identify the author
                // `html`/`body` backgrounds without changing the blueprint shape.
                if tag_lower == "html" || tag_lower == "body" {
                    dom_attrs.push(Attribute::property("data-strata-tag", tag_lower.clone()));
                }

                // An empty element may be a layout box, spacer, overlay, divider,
                // or hit target. Only drop it when it is genuinely bare: no
                // classes, no id, and no author attributes at all.
                let is_bare = class_tokens.is_empty() && id_opt.is_none() && attributes.is_empty();

                let my_context = ElementContext {
                    tag: tag_lower.clone(),
                    classes: class_tokens,
                    id: id_opt.map(String::from),
                    attributes: attributes.to_vec(),
                };

                let final_tag = strata_tag;
                let child_blueprints = if tag_lower == "select" {
                    computed_style.white_space = WhiteSpace::NoWrap;
                    let label = Self::extract_select_label(children, attributes);
                    vec![NodeBlueprint::text(label)]
                } else if tag_lower == "img" && children.is_empty() {
                    // Replaced element: the runtime decodes `src` data URIs.
                    Vec::new()
                } else {
                    ancestors.push(my_context.clone());
                    let res = self.transpile_children_slice(
                        children,
                        ancestors,
                        stylesheets,
                        Some(&computed_style),
                        state,
                        my_effective_bg,
                        accurate_document,
                    );
                    ancestors.pop();
                    res
                };

                // Empty non-void element elimination
                let is_void_or_replaced = matches!(
                    final_tag,
                    Tag::Img
                        | Tag::Hr
                        | Tag::Br
                        | Tag::Input
                        | Tag::Select
                        | Tag::Svg
                        | Tag::Path
                        | Tag::Canvas
                ) || computed_style.float != Float::None
                    || computed_style.clear != Clear::None
                    || computed_style.height != Dimension::Auto
                    || computed_style.min_height != Dimension::Auto;

                if child_blueprints.is_empty()
                    && !is_void_or_replaced
                    && is_bare
                    && matches!(
                        tag_lower.as_str(),
                        "div" | "p" | "span" | "b" | "strong" | "i" | "em" | "li" | "a" | "center"
                    )
                {
                    return None;
                }

                let bp = NodeBlueprint::element(final_tag, classes, dom_attrs, child_blueprints);
                Some((bp, my_context))
            }
        }
    }
}
