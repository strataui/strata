//! Transpilation of Semantic DOM AST into native Strata `NodeBlueprint` trees.

use strata_dom::{Attribute, NodeBlueprint, Tag};
use crate::ast::SemanticNode;
use crate::reader_theme::ReaderTheme;

/// Transpiles semantic HTML AST nodes into native, styled Strata reactive blueprints.
pub struct WebTranspiler<'a> {
    theme: &'a ReaderTheme,
}

impl<'a> WebTranspiler<'a> {
    /// Creates a new transpiler with the specified theme settings.
    pub fn new(theme: &'a ReaderTheme) -> Self {
        Self { theme }
    }

    /// Transpiles a slice of top-level semantic nodes into a complete article container blueprint.
    pub fn transpile_document(&self, nodes: &[SemanticNode], title: Option<&str>) -> NodeBlueprint {
        let mut children = Vec::new();

        if let Some(t) = title {
            children.push(NodeBlueprint::element(
                Tag::H1,
                vec![format!(
                    "text-3xl sm:text-4xl font-extrabold tracking-tight mb-6 {}",
                    self.theme.heading_class
                )],
                Vec::new(),
                vec![NodeBlueprint::text(t)],
            ));
        }

        for node in nodes {
            if let Some(bp) = self.transpile_node(node) {
                children.push(bp);
            }
        }

        let container_style = format!(
            "w-full max-w-[{}px] mx-auto px-6 py-10 {} {}",
            self.theme.max_width_px, self.theme.bg_class, self.theme.text_class
        );

        NodeBlueprint::element(
            Tag::Div,
            vec![container_style],
            Vec::new(),
            children,
        )
    }

    /// Recursively transpiles a single `SemanticNode` into an optional `NodeBlueprint`.
    pub fn transpile_node(&self, node: &SemanticNode) -> Option<NodeBlueprint> {
        match node {
            SemanticNode::Text(text) => {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(NodeBlueprint::text(text.clone()))
                }
            }
            SemanticNode::Comment(_) => None,
            SemanticNode::Element {
                tag,
                attributes,
                children,
            } => {
                let child_blueprints: Vec<NodeBlueprint> = children
                    .iter()
                    .filter_map(|c| self.transpile_node(c))
                    .collect();

                let (strata_tag, classes, attrs) = self.map_element(tag, attributes);
                Some(NodeBlueprint::element(
                    strata_tag,
                    classes,
                    attrs,
                    child_blueprints,
                ))
            }
        }
    }

    fn map_element(
        &self,
        tag: &str,
        attributes: &[(String, String)],
    ) -> (Tag, Vec<String>, Vec<Attribute>) {
        let mut attrs = Vec::new();

        // Extract key standard attributes
        for (k, v) in attributes {
            match k.as_str() {
                "href" | "src" | "alt" | "title" | "id" => {
                    attrs.push(Attribute::property(k.clone(), v.clone()));
                }
                _ => {}
            }
        }

        let (mapped_tag, class_str) = match tag {
            "h1" => (
                Tag::H1,
                format!("text-3xl font-extrabold tracking-tight mt-8 mb-4 {}", self.theme.heading_class),
            ),
            "h2" => (
                Tag::H2,
                format!(
                    "text-2xl font-bold tracking-tight mt-8 mb-3 pb-2 border-b {} {}",
                    self.theme.border_class, self.theme.heading_class
                ),
            ),
            "h3" => (
                Tag::H3,
                format!("text-xl font-semibold mt-6 mb-2 {}", self.theme.heading_class),
            ),
            "h4" => (
                Tag::H4,
                format!("text-lg font-medium mt-4 mb-2 {}", self.theme.heading_class),
            ),
            "h5" => (
                Tag::H5,
                format!("text-base font-semibold mt-3 mb-1 {}", self.theme.heading_class),
            ),
            "h6" => (
                Tag::H6,
                format!("text-sm font-semibold uppercase tracking-wider mt-3 mb-1 {}", self.theme.heading_class),
            ),
            "p" => (
                Tag::P,
                format!("text-base leading-relaxed mb-4 {}", self.theme.text_class),
            ),
            "blockquote" => (
                Tag::Custom("blockquote"),
                format!(
                    "border-l-4 pl-4 py-2 italic my-4 rounded-r {} {} {}",
                    self.theme.border_class, self.theme.card_bg_class, self.theme.text_class
                ),
            ),
            "ul" => (
                Tag::Ul,
                format!("list-disc pl-6 mb-4 space-y-1.5 {}", self.theme.text_class),
            ),
            "ol" => (
                Tag::Ol,
                format!("list-decimal pl-6 mb-4 space-y-1.5 {}", self.theme.text_class),
            ),
            "li" => (
                Tag::Li,
                format!("leading-relaxed {}", self.theme.text_class),
            ),
            "a" => (
                Tag::A,
                format!("underline cursor-pointer {}", self.theme.link_class),
            ),
            "pre" | "code" => (
                Tag::Custom("pre"),
                format!(
                    "font-mono text-sm p-4 rounded-lg my-4 overflow-x-auto text-emerald-400 {}",
                    self.theme.code_bg_class
                ),
            ),
            "table" => (
                Tag::Table,
                format!("w-full border-collapse my-6 text-sm border {}", self.theme.border_class),
            ),
            "thead" => (
                Tag::Thead,
                format!("border-b {}", self.theme.border_class),
            ),
            "tbody" => (
                Tag::Tbody,
                format!("divide-y {}", self.theme.border_class),
            ),
            "tr" => (
                Tag::Tr,
                "hover:bg-slate-800/20 transition-colors".to_string(),
            ),
            "th" => (
                Tag::Th,
                format!(
                    "border p-2.5 font-semibold text-left {} {}",
                    self.theme.border_class, self.theme.heading_class
                ),
            ),
            "td" => (
                Tag::Td,
                format!(
                    "border p-2.5 text-left {} {}",
                    self.theme.border_class, self.theme.text_class
                ),
            ),
            "img" => (
                Tag::Img,
                "rounded-lg max-w-full h-auto my-4 shadow-md border border-slate-800/50".to_string(),
            ),
            "hr" => (
                Tag::Hr,
                format!("border-t my-8 {}", self.theme.border_class),
            ),
            "header" => (Tag::Header, "mb-6".to_string()),
            "nav" => (Tag::Nav, "my-4".to_string()),
            "main" => (Tag::Main, "w-full".to_string()),
            "section" => (Tag::Section, "my-6".to_string()),
            "aside" => (
                Tag::Aside,
                format!("p-4 rounded-lg my-4 border {} {}", self.theme.border_class, self.theme.card_bg_class),
            ),
            "footer" => (
                Tag::Footer,
                format!("mt-12 pt-6 border-t text-xs text-slate-500 {}", self.theme.border_class),
            ),
            "span" => (Tag::Span, "".to_string()),
            _ => (Tag::Div, "".to_string()),
        };

        let classes = if class_str.is_empty() {
            Vec::new()
        } else {
            vec![class_str]
        };

        (mapped_tag, classes, attrs)
    }
}
