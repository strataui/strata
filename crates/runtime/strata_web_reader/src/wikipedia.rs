//! Dedicated Wikipedia Article Engine.
//!
//! Parses, normalizes, and extracts structured metadata from Wikipedia HTML articles:
//! - Infobox extraction and normalization into modern summary cards
//! - Table of Contents (TOC) hierarchy tree extraction
//! - Lead summary extraction
//! - Distraction-free article sectioning and responsive layout compilation

use strata_dom::{Attribute, NodeBlueprint, Tag};
use crate::ast::SemanticNode;
use crate::parser::HtmlParser;
use crate::reader_theme::ReaderTheme;
use crate::transpiler::WebTranspiler;

/// Key-value metadata row extracted from a Wikipedia infobox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfoboxRow {
    pub label: String,
    pub value: String,
}

/// Normalized representation of a Wikipedia metadata infobox card.
#[derive(Debug, Clone, PartialEq)]
pub struct WikipediaInfobox {
    pub title: String,
    pub image_url: Option<String>,
    pub image_caption: Option<String>,
    pub rows: Vec<InfoboxRow>,
}

/// A heading entry in the Wikipedia Table of Contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikipediaTocItem {
    pub level: usize,
    pub number: String,
    pub title: String,
    pub anchor: String,
}

/// A structured section of an article under a heading.
#[derive(Debug, Clone, PartialEq)]
pub struct WikipediaSection {
    pub title: Option<String>,
    pub level: usize,
    pub nodes: Vec<SemanticNode>,
}

/// Full structured model of a parsed Wikipedia article.
#[derive(Debug, Clone, PartialEq)]
pub struct WikipediaArticle {
    pub title: String,
    pub summary: String,
    pub infobox: Option<WikipediaInfobox>,
    pub toc: Vec<WikipediaTocItem>,
    pub sections: Vec<WikipediaSection>,
}

/// Engine for compiling Wikipedia articles into optimized Strata reactive views.
pub struct WikipediaEngine;

impl WikipediaEngine {
    /// Parses an HTML Wikipedia document into a structured `WikipediaArticle`.
    pub fn parse(html: &str) -> WikipediaArticle {
        let root_nodes = HtmlParser::parse(html);

        // 1. Title extraction
        let mut title = "Untitled Article".to_string();
        for node in &root_nodes {
            if let Some(h1) = node.find_first("h1") {
                let t = h1.text_content().trim().to_string();
                if !t.is_empty() {
                    title = t;
                    break;
                }
            }
        }

        // 2. Infobox extraction
        let mut infobox = None;
        for node in &root_nodes {
            let infobox_tables = node.find_by_class("infobox");
            if let Some(table) = infobox_tables.first() {
                infobox = Self::extract_infobox(table, &title);
                break;
            }
        }

        // 3. Table of Contents extraction
        let mut toc = Vec::new();
        for node in &root_nodes {
            let tocs = node.find_by_class("toc");
            if let Some(toc_node) = tocs.first() {
                toc = Self::extract_toc(toc_node);
                break;
            }
        }

        // 4. Clean body nodes and extract lead summary
        let mut source_nodes: Vec<SemanticNode> = Vec::new();
        for node in &root_nodes {
            let parser_outputs = node.find_by_class("mw-parser-output");
            if let Some(po) = parser_outputs
                .into_iter()
                .max_by_key(|n| match n {
                    SemanticNode::Element { children, .. } => children.len(),
                    _ => 0,
                })
            {
                if let SemanticNode::Element { children, .. } = po {
                    if !children.is_empty() {
                        source_nodes = children.clone();
                        break;
                    }
                }
            }
        }
        if source_nodes.is_empty() {
            source_nodes = root_nodes;
        }

        let mut cleaned_nodes = Vec::new();
        for node in source_nodes {
            Self::sanitize_node(&node, &mut cleaned_nodes);
        }

        let unwrapped_nodes = Self::unwrap_containers(cleaned_nodes);

        let mut summary = String::new();
        for node in &unwrapped_nodes {
            if node.tag() == Some("p") {
                let text = node.text_content().trim().to_string();
                if text.len() > 30 {
                    summary = text;
                    break;
                }
            }
        }
        if summary.is_empty() {
            for node in &unwrapped_nodes {
                for p in node.find_all("p") {
                    let text = p.text_content().trim().to_string();
                    if text.len() > 30 {
                        summary = text;
                        break;
                    }
                }
                if !summary.is_empty() {
                    break;
                }
            }
        }

        // 5. Partition into sections
        let sections = Self::partition_sections(unwrapped_nodes);

        if toc.is_empty() {
            let mut idx = 1;
            for sec in &sections {
                if sec.level == 2 {
                    if let Some(ref t) = sec.title {
                        toc.push(WikipediaTocItem {
                            level: 2,
                            number: format!("{}", idx),
                            title: t.clone(),
                            anchor: t.to_lowercase().replace(' ', "_"),
                        });
                        idx += 1;
                    }
                }
            }
        }

        WikipediaArticle {
            title,
            summary,
            infobox,
            toc,
            sections,
        }
    }

    fn unwrap_containers(nodes: Vec<SemanticNode>) -> Vec<SemanticNode> {
        let mut current = nodes;
        loop {
            if current.len() == 1 {
                let tag = current[0].tag().unwrap_or("");
                if tag == "html" || tag == "body" || tag == "article" || tag == "main" {
                    if let Some(SemanticNode::Element { children, .. }) = current.pop() {
                        current = children;
                        continue;
                    }
                }
            }
            if let Some(pos) = current.iter().position(|n| n.tag() == Some("body")) {
                if let SemanticNode::Element { children, .. } = current.remove(pos) {
                    current = children;
                    continue;
                }
            }
            break;
        }
        current
    }

    fn extract_infobox(table: &SemanticNode, default_title: &str) -> Option<WikipediaInfobox> {
        let mut infobox_title = default_title.to_string();
        let mut image_url = None;
        let mut image_caption = None;
        let mut rows = Vec::new();

        // Check for infobox title in <caption>, .infobox-title, .infobox-above
        let title_nodes = table.find_by_class("infobox-title");
        let above_nodes = table.find_by_class("infobox-above");
        if let Some(caption) = table.find_first("caption") {
            let t = caption.text_content().trim().to_string();
            if !t.is_empty() {
                infobox_title = t;
            }
        } else if let Some(th) = title_nodes.first().or_else(|| above_nodes.first()) {
            let t = th.text_content().trim().to_string();
            if !t.is_empty() {
                infobox_title = t;
            }
        }

        // Check for primary image
        if let Some(img) = table.find_first("img") {
            if let Some(src) = img.attr("src") {
                let full_url = if src.starts_with("//") {
                    format!("https:{}", src)
                } else {
                    src.to_string()
                };
                image_url = Some(full_url);
            }
            if let Some(alt) = img.attr("alt") {
                image_caption = Some(alt.to_string());
            }
        }

        // Extract key-value rows: <tr> with <th> and <td>
        for tr in table.find_all("tr") {
            let ths = tr.find_all("th");
            let tds = tr.find_all("td");

            if let (Some(th), Some(td)) = (ths.first(), tds.first()) {
                let label = th.text_content().trim().to_string();
                let value = td.text_content().trim().to_string();
                if !label.is_empty() && !value.is_empty() {
                    rows.push(InfoboxRow { label, value });
                }
            }
        }

        Some(WikipediaInfobox {
            title: infobox_title,
            image_url,
            image_caption,
            rows,
        })
    }

    fn extract_toc(toc_node: &SemanticNode) -> Vec<WikipediaTocItem> {
        let mut items = Vec::new();

        for li in toc_node.find_all("li") {
            let text = li.text_content().trim().to_string();
            if text.is_empty() {
                continue;
            }

            let mut anchor = String::new();
            if let Some(a) = li.find_first("a") {
                if let Some(href) = a.attr("href") {
                    anchor = href.trim_start_matches('#').to_string();
                }
            }

            // Parse optional number prefix (e.g. "1.2 Early History")
            let mut parts = text.splitn(2, ' ');
            let first = parts.next().unwrap_or("");
            let rest = parts.next().unwrap_or(first);

            let (number, item_title) = if first.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                (first.to_string(), rest.to_string())
            } else {
                (String::new(), text)
            };

            items.push(WikipediaTocItem {
                level: 2,
                number,
                title: item_title,
                anchor,
            });
        }

        items
    }

    fn sanitize_node(node: &SemanticNode, out: &mut Vec<SemanticNode>) {
        match node {
            SemanticNode::Element {
                tag,
                attributes,
                children,
            } => {
                // Strip Wikipedia noise, editing links, and navboxes
                if tag == "style" || tag == "script" || tag == "h1" {
                    return;
                }
                if let Some(classes) = node.attr("class") {
                    if classes.contains("mw-editsection")
                        || classes.contains("navbox")
                        || classes.contains("infobox")
                        || classes.contains("toc")
                        || classes.contains("mw-empty-elt")
                        || classes.contains("reference")
                        || classes.contains("noprint")
                        || classes.contains("shortdescription")
                        || classes.contains("hatnote")
                        || classes.contains("ambox")
                        || classes.contains("sistersitebox")
                        || classes.contains("catlinks")
                        || classes.contains("printfooter")
                        || classes.contains("reflist")
                    {
                        return;
                    }
                }
                if let Some(id) = node.attr("id") {
                    if id == "toc" || id == "coordinates" || id == "siteSub" || id == "contentSub" {
                        return;
                    }
                }

                let mut cleaned_children = Vec::new();
                for child in children {
                    Self::sanitize_node(child, &mut cleaned_children);
                }

                out.push(SemanticNode::Element {
                    tag: tag.clone(),
                    attributes: attributes.clone(),
                    children: cleaned_children,
                });
            }
            SemanticNode::Text(text) => {
                out.push(SemanticNode::Text(text.clone()));
            }
            SemanticNode::Comment(_) => {}
        }
    }

    fn partition_sections(nodes: Vec<SemanticNode>) -> Vec<WikipediaSection> {
        let mut sections = Vec::new();
        let mut current_title: Option<String> = None;
        let mut current_level = 1;
        let mut current_nodes = Vec::new();

        for node in nodes {
            let mut is_heading = match node.tag() {
                Some("h2") => Some((node.text_content().trim().to_string(), 2)),
                Some("h3") => Some((node.text_content().trim().to_string(), 3)),
                Some("h4") => Some((node.text_content().trim().to_string(), 4)),
                _ => None,
            };

            if is_heading.is_none() && node.has_class("mw-heading") {
                if let Some(h2) = node.find_first("h2") {
                    is_heading = Some((h2.text_content().trim().to_string(), 2));
                } else if let Some(h3) = node.find_first("h3") {
                    is_heading = Some((h3.text_content().trim().to_string(), 3));
                } else if let Some(h4) = node.find_first("h4") {
                    is_heading = Some((h4.text_content().trim().to_string(), 4));
                }
            }

            if let Some((heading_title, level)) = is_heading {
                if !current_nodes.is_empty() || current_title.is_some() {
                    sections.push(WikipediaSection {
                        title: current_title,
                        level: current_level,
                        nodes: current_nodes,
                    });
                    current_nodes = Vec::new();
                }
                current_title = Some(heading_title);
                current_level = level;
            } else {
                current_nodes.push(node);
            }
        }

        if !current_nodes.is_empty() || current_title.is_some() {
            sections.push(WikipediaSection {
                title: current_title,
                level: current_level,
                nodes: current_nodes,
            });
        }

        sections
    }

    /// Compiles a `WikipediaArticle` into a native Strata `NodeBlueprint` layout.
    pub fn render_to_blueprint(article: &WikipediaArticle, theme: &ReaderTheme) -> NodeBlueprint {
        let transpiler = WebTranspiler::new(theme);
        let mut root_children = Vec::new();

        // 1. Top Header: Title
        root_children.push(NodeBlueprint::element(
            Tag::Div,
            vec!["mb-5 pb-4 border-b ".to_string() + &theme.border_class],
            Vec::new(),
            vec![
                NodeBlueprint::element(
                    Tag::H1,
                    vec![format!("text-4xl sm:text-5xl font-black tracking-tight {}", theme.heading_class)],
                    Vec::new(),
                    vec![NodeBlueprint::text(article.title.clone())],
                ),
            ],
        ));

        // 2. Lead Summary Paragraph (if present)
        if !article.summary.is_empty() {
            root_children.push(NodeBlueprint::element(
                Tag::P,
                vec![format!("text-base leading-relaxed mb-6 {}", theme.text_class)],
                Vec::new(),
                vec![NodeBlueprint::text(article.summary.clone())],
            ));
        }

        // 3. Infobox Card (if present)
        if let Some(ref infobox) = article.infobox {
            root_children.push(Self::render_infobox_card(infobox, theme));
        }

        // 4. Table of Contents Card (if items exist)
        if !article.toc.is_empty() {
            root_children.push(Self::render_toc_card(&article.toc, theme));
        }

        // 5. Main Body Sections
        for section in &article.sections {
            let mut section_children = Vec::new();

            if let Some(ref heading) = section.title {
                let heading_tag = match section.level {
                    2 => Tag::H2,
                    3 => Tag::H3,
                    _ => Tag::H4,
                };
                let heading_class = match section.level {
                    2 => format!("text-2xl font-bold tracking-tight mt-10 mb-4 pb-2 border-b {} {}", theme.border_class, theme.heading_class),
                    3 => format!("text-xl font-semibold mt-8 mb-3 {}", theme.heading_class),
                    _ => format!("text-lg font-medium mt-6 mb-2 {}", theme.heading_class),
                };

                section_children.push(NodeBlueprint::element(
                    heading_tag,
                    vec![heading_class],
                    Vec::new(),
                    vec![NodeBlueprint::text(heading.clone())],
                ));
            }

            for node in &section.nodes {
                if let Some(bp) = transpiler.transpile_node(node) {
                    section_children.push(bp);
                }
            }

            root_children.push(NodeBlueprint::element(
                Tag::Section,
                vec!["mb-8".to_string()],
                Vec::new(),
                section_children,
            ));
        }

        // 6. Container wrapper
        let container_style = format!(
            "w-full max-w-[{}px] mx-auto px-6 py-6 {} {}",
            theme.max_width_px, theme.bg_class, theme.text_class
        );

        NodeBlueprint::element(
            Tag::Div,
            vec![container_style],
            Vec::new(),
            root_children,
        )
    }

    fn render_infobox_card(infobox: &WikipediaInfobox, theme: &ReaderTheme) -> NodeBlueprint {
        let mut card_children = Vec::new();

        card_children.push(NodeBlueprint::element(
            Tag::H3,
            vec![format!("text-lg font-bold pb-2 mb-2 border-b {} {}", theme.border_class, theme.heading_class)],
            Vec::new(),
            vec![NodeBlueprint::text(infobox.title.clone())],
        ));

        let mut row_nodes = Vec::new();
        for row in infobox.rows.iter().take(8) {
            row_nodes.push(NodeBlueprint::element(
                Tag::Tr,
                vec!["w-full flex flex-row items-baseline border-b last:border-b-0 ".to_string() + &theme.border_class],
                Vec::new(),
                vec![
                    NodeBlueprint::element(
                        Tag::Td,
                        vec![format!("py-1.5 pr-4 text-xs font-semibold w-44 {}", theme.heading_class)],
                        Vec::new(),
                        vec![NodeBlueprint::text(row.label.clone())],
                    ),
                    NodeBlueprint::element(
                        Tag::Td,
                        vec![format!("py-1.5 text-xs leading-relaxed flex-1 {}", theme.text_class)],
                        Vec::new(),
                        vec![NodeBlueprint::text(row.value.clone())],
                    ),
                ],
            ));
        }

        card_children.push(NodeBlueprint::element(
            Tag::Table,
            vec!["w-full text-xs".to_string()],
            Vec::new(),
            vec![NodeBlueprint::element(
                Tag::Tbody,
                vec!["divide-y ".to_string() + &theme.border_class],
                Vec::new(),
                row_nodes,
            )],
        ));

        NodeBlueprint::element(
            Tag::Aside,
            vec![format!(
                "w-full mb-6 p-4 rounded-xl border shadow-lg {} {}",
                theme.border_class, theme.card_bg_class
            )],
            Vec::new(),
            card_children,
        )
    }

    fn render_toc_card(toc: &[WikipediaTocItem], theme: &ReaderTheme) -> NodeBlueprint {
        let mut list_items = Vec::new();

        for item in toc {
            let label = if !item.number.is_empty() {
                format!("{} {}", item.number, item.title)
            } else {
                item.title.clone()
            };

            list_items.push(NodeBlueprint::element(
                Tag::Li,
                vec!["py-0.5".to_string()],
                Vec::new(),
                vec![NodeBlueprint::element(
                    Tag::A,
                    vec![format!("text-xs font-medium cursor-pointer {}", theme.link_class)],
                    vec![Attribute::property("href", format!("#{}", item.anchor))],
                    vec![NodeBlueprint::text(label)],
                )],
            ));
        }

        NodeBlueprint::element(
            Tag::Nav,
            vec![format!(
                "mb-8 p-4 rounded-xl border {} {}",
                theme.border_class, theme.card_bg_class
            )],
            Vec::new(),
            vec![
                NodeBlueprint::element(
                    Tag::H4,
                    vec![format!("text-xs font-bold uppercase tracking-wider mb-2 {}", theme.heading_class)],
                    Vec::new(),
                    vec![NodeBlueprint::text("Contents")],
                ),
                NodeBlueprint::element(
                    Tag::Ul,
                    vec!["space-y-1 list-none pl-0".to_string()],
                    Vec::new(),
                    list_items,
                ),
            ],
        )
    }
}
