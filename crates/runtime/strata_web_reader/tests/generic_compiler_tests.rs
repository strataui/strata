//! Milestone 51 Integration Tests: Generic Non-Hardcoded HTML DOM to Strata Component Compiler.
//!
//! Validates:
//! 1. Full unmutated HTML5 DOM tree ingestion without scraping heuristics or tag skipping
//! 2. Zero-hardcoding dynamic CSS cascade and property inheritance on all elements
//! 3. Inline Formatting Context restructuring (anonymous block grouping for mixed inline/block children)
//! 4. Full preservation of element IDs, classes, navigation anchors, and SVG graphics
//! 5. Author stylesheets and inline styles priority over W3C UA defaults

use strata_dom::{Attribute, NodeBlueprint, Tag};
use strata_style::types::*;
use strata_style::MediaContext;
use strata_web_reader::prelude::*;

fn get_direct_style(node: &NodeBlueprint) -> Option<ComputedStyle> {
    match node {
        NodeBlueprint::Element { attributes, .. } => attributes.iter().find_map(|a| match a {
            Attribute::DirectStyle(any) => any.downcast_ref::<ComputedStyle>().cloned(),
            _ => None,
        }),
        _ => None,
    }
}

fn get_attribute_value<'a>(node: &'a NodeBlueprint, attr_name: &str) -> Option<&'a str> {
    match node {
        NodeBlueprint::Element { attributes, .. } => attributes.iter().find_map(|a| match a {
            Attribute::Property { key, value } if key.eq_ignore_ascii_case(attr_name) => {
                Some(value.as_str())
            }
            _ => None,
        }),
        _ => None,
    }
}

#[test]
fn test_generic_non_hardcoded_html_transpilation() {
    let html = r##"
        <!DOCTYPE html>
        <html>
            <head>
                <style>
                    .custom-card {
                        background-color: #0f172a;
                        color: #f8fafc;
                        font-size: 18px;
                        border-width: 2px;
                        border-color: #38bdf8;
                    }
                    .custom-title {
                        color: #a855f7;
                        font-weight: 800;
                    }
                    p.lead {
                        margin-block-start: 1.5em;
                        line-height: 1.75;
                    }
                </style>
            </head>
            <body>
                <div id="card-1" class="custom-card" style="padding: 16px;">
                    <h2 class="custom-title">Cloud Infrastructure</h2>
                    <p class="lead">Autonomous telemetry orchestrator.</p>
                    <svg viewBox="0 0 24 24" width="24" height="24">
                        <circle cx="12" cy="12" r="10" fill="#38bdf8" />
                    </svg>
                </div>
            </body>
        </html>
    "##;

    let nodes = HtmlParser::parse(html);
    assert_eq!(nodes.len(), 1, "Must parse single html root");

    let theme = ReaderTheme::dark();
    let transpiler = WebTranspiler::new(&theme);
    let media_ctx = MediaContext::new(1280.0, 800.0, true);

    let doc = transpiler.transpile_full_document(&nodes, &media_ctx);

    // Verify root is div container
    match &doc {
        NodeBlueprint::Element { tag, children, .. } => {
            assert_eq!(*tag, Tag::Div);
            assert!(!children.is_empty());
        }
        _ => panic!("Expected element document root"),
    }

    // Find card element
    fn find_by_id<'a>(bp: &'a NodeBlueprint, id: &str) -> Option<&'a NodeBlueprint> {
        match bp {
            NodeBlueprint::Element {
                attributes,
                children,
                ..
            } => {
                for a in attributes {
                    if let Attribute::Property { key, value } = a {
                        if key.eq_ignore_ascii_case("id") && value == id {
                            return Some(bp);
                        }
                    }
                }
                for c in children {
                    if let Some(f) = find_by_id(c, id) {
                        return Some(f);
                    }
                }
                None
            }
            NodeBlueprint::Fragment(children) => {
                for c in children {
                    if let Some(f) = find_by_id(c, id) {
                        return Some(f);
                    }
                }
                None
            }
            _ => None,
        }
    }

    let card = find_by_id(&doc, "card-1").expect("card-1 element must be found in blueprint");
    let card_style =
        get_direct_style(card).expect("card-1 must have direct ComputedStyle attached");

    // Verify dynamic author style was applied without hardcoding
    assert_eq!(
        card_style.background_color,
        Some(Color::new(15, 23, 42, 255))
    );
    assert_eq!(card_style.color, Color::new(248, 250, 252, 255));
    assert_eq!(card_style.font_size, 18.0);
    assert_eq!(card_style.border_width.top, 2.0);
    assert_eq!(card_style.border_color, Some(Color::new(56, 189, 248, 255)));
    // Verify inline style padding: 16px was merged
    assert_eq!(card_style.padding.top, 16.0);
    assert_eq!(card_style.padding.left, 16.0);

    // Verify H2 inside card inherited and cascaded styles
    let h2 = match card {
        NodeBlueprint::Element { children, .. } => children
            .iter()
            .find(|c| matches!(c, NodeBlueprint::Element { tag: Tag::H2, .. }))
            .expect("H2 child"),
        _ => panic!("Expected card element"),
    };
    let h2_style = get_direct_style(h2).expect("H2 must have ComputedStyle");
    assert_eq!(h2_style.color, Color::new(168, 85, 247, 255));
    assert_eq!(h2_style.font_weight, FontWeight::ExtraBold); // 800

    // Verify SVG was NOT stripped
    let svg = match card {
        NodeBlueprint::Element { children, .. } => children
            .iter()
            .find(|c| matches!(c, NodeBlueprint::Element { tag: Tag::Svg, .. }))
            .expect("SVG must be preserved"),
        _ => panic!("Expected card element"),
    };
    assert_eq!(get_attribute_value(svg, "viewBox"), Some("0 0 24 24"));
}

#[test]
fn test_inline_formatting_context_restructuring() {
    // Parent div has mixed inline and block children:
    // [Text("Prefix "), Span("highlight"), Text(" suffix"), Div("block child"), Text("After block")]
    let html = r#"
        <div class="mixed-container">
            Prefix <span>highlight</span> suffix
            <div class="inner-block">Block section</div>
            Trailing inline text
        </div>
    "#;

    let nodes = HtmlParser::parse(html);
    let theme = ReaderTheme::dark();
    let transpiler = WebTranspiler::new(&theme);
    let media_ctx = MediaContext::new(1280.0, 800.0, true);

    let doc = transpiler.transpile_full_document(&nodes, &media_ctx);

    fn find_by_class<'a>(bp: &'a NodeBlueprint, class_name: &str) -> Option<&'a NodeBlueprint> {
        match bp {
            NodeBlueprint::Element {
                classes, children, ..
            } => {
                if classes
                    .iter()
                    .any(|c| c.split_whitespace().any(|tok| tok == class_name))
                {
                    return Some(bp);
                }
                for c in children {
                    if let Some(f) = find_by_class(c, class_name) {
                        return Some(f);
                    }
                }
                None
            }
            NodeBlueprint::Fragment(children) => {
                for c in children {
                    if let Some(f) = find_by_class(c, class_name) {
                        return Some(f);
                    }
                }
                None
            }
            _ => None,
        }
    }

    let mixed = find_by_class(&doc, "mixed-container").expect("mixed-container must be found");

    match mixed {
        NodeBlueprint::Element { children, .. } => {
            // Children should have 3 items:
            // 1. Anonymous block wrapping the inlines [Prefix, <span>highlight</span>, suffix]
            // 2. The inner block div
            // 3. Anonymous block wrapping the trailing inline text
            assert_eq!(
                children.len(),
                3,
                "Mixed inlines and blocks must be restructured into anonymous blocks"
            );

            // Child 0: Anonymous block containing inlines
            match &children[0] {
                NodeBlueprint::Element {
                    classes,
                    children: anon_children,
                    ..
                } => {
                    assert!(classes.iter().any(|c| c.contains("strata-anon-block")));
                    assert!(anon_children.len() >= 2);
                }
                _ => panic!("Expected anonymous block element for first inline run"),
            }

            // Child 1: Inner block
            match &children[1] {
                NodeBlueprint::Element { classes, .. } => {
                    assert!(classes.iter().any(|c| c.contains("inner-block")));
                }
                _ => panic!("Expected inner-block element"),
            }

            // Child 2: Anonymous block containing trailing inline text
            match &children[2] {
                NodeBlueprint::Element {
                    classes,
                    children: anon_children,
                    ..
                } => {
                    assert!(classes.iter().any(|c| c.contains("strata-anon-block")));
                    assert_eq!(anon_children.len(), 1);
                }
                _ => panic!("Expected anonymous block element for trailing inline run"),
            }
        }
        _ => panic!("Expected element"),
    }
}

#[test]
fn test_element_id_classes_and_anchors_preservation() {
    let html = r##"
        <div id="section-toc" class="toc-navigation">
            <h3 id="heading-toc">Table of Contents</h3>
            <ul>
                <li id="toc-item-1"><a href="#introduction" class="nav-link">Introduction</a></li>
                <li id="toc-item-2"><a href="#architecture" class="nav-link">Architecture</a></li>
            </ul>
        </div>
        <section id="introduction">
            <h2>Introduction</h2>
            <p>Welcome to Strata.</p>
        </section>
        <section id="architecture">
            <h2>Architecture</h2>
            <p>Built with zero Chromium overhead.</p>
        </section>
    "##;

    let nodes = HtmlParser::parse(html);
    let theme = ReaderTheme::dark();
    let transpiler = WebTranspiler::new(&theme);
    let media_ctx = MediaContext::new(1280.0, 800.0, true);

    let doc = transpiler.transpile_full_document(&nodes, &media_ctx);

    fn collect_all_ids(bp: &NodeBlueprint, ids: &mut Vec<String>) {
        if let NodeBlueprint::Element {
            attributes,
            children,
            ..
        } = bp
        {
            for a in attributes {
                if let Attribute::Property { key, value } = a {
                    if key.eq_ignore_ascii_case("id") {
                        ids.push(value.clone());
                    }
                }
            }
            for c in children {
                collect_all_ids(c, ids);
            }
        }
    }

    fn collect_all_hrefs(bp: &NodeBlueprint, hrefs: &mut Vec<String>) {
        if let NodeBlueprint::Element {
            attributes,
            children,
            ..
        } = bp
        {
            for a in attributes {
                if let Attribute::Property { key, value } = a {
                    if key.eq_ignore_ascii_case("href") {
                        hrefs.push(value.clone());
                    }
                }
            }
            for c in children {
                collect_all_hrefs(c, hrefs);
            }
        }
    }

    let mut ids = Vec::new();
    collect_all_ids(&doc, &mut ids);

    assert!(ids.contains(&"section-toc".to_string()));
    assert!(ids.contains(&"heading-toc".to_string()));
    assert!(ids.contains(&"toc-item-1".to_string()));
    assert!(ids.contains(&"toc-item-2".to_string()));
    assert!(ids.contains(&"introduction".to_string()));
    assert!(ids.contains(&"architecture".to_string()));

    let mut hrefs = Vec::new();
    collect_all_hrefs(&doc, &mut hrefs);

    assert!(hrefs.contains(&"#introduction".to_string()));
    assert!(hrefs.contains(&"#architecture".to_string()));
}

#[test]
fn test_rust_lang_org_hero_and_hidden_elements() {
    let html = r##"
        <!DOCTYPE html>
        <html>
            <head>
                <style>
                    .hidden {
                        border: 0;
                        clip: rect(0 0 0 0);
                        height: 1px;
                        margin: -1px;
                        overflow: hidden;
                        padding: 0;
                        position: absolute;
                        width: 1px;
                        white-space: nowrap;
                    }
                    div.brand img {
                        width: 80px;
                        margin-top: -3px;
                    }
                    header .button.button-download {
                        color: #111111;
                        background-color: #ffc832;
                        border-color: #ffc832;
                        width: 100%;
                        padding: 20px;
                        height: auto;
                        font-size: 2.25rem;
                        margin-top: 20px;
                    }
                </style>
            </head>
            <body>
                <div class="brand">
                    <a href="/"><img src="logo.svg" alt="Rust Logo" /></a>
                </div>
                <div class="select">
                    <label for="lang" class="hidden">Language</label>
                    <select id="lang">
                        <option value="en">English</option>
                    </select>
                </div>
                <header>
                    <div>
                        <a class="button button-download" href="/get-started">Get Started</a>
                    </div>
                </header>
            </body>
        </html>
    "##;

    let theme = ReaderTheme::dark();
    let media = MediaContext::new(1280.0, 800.0, true);
    let nodes = HtmlParser::parse(html);
    let transpiler = WebTranspiler::new(&theme);
    let doc = transpiler.transpile_full_document(&nodes, &media);

    fn find_element<'a>(
        node: &'a NodeBlueprint,
        predicate: &impl Fn(&NodeBlueprint) -> bool,
    ) -> Option<&'a NodeBlueprint> {
        if predicate(node) {
            return Some(node);
        }
        if let NodeBlueprint::Element { children, .. } = node {
            for c in children {
                if let Some(found) = find_element(c, predicate) {
                    return Some(found);
                }
            }
        }
        None
    }

    // Find the button
    let button = find_element(&doc, &|n| {
        if let NodeBlueprint::Element { classes, .. } = n {
            classes.iter().any(|c| c == "button-download")
        } else {
            false
        }
    })
    .expect("button-download must be found");

    let button_style = get_direct_style(button).expect("button must have direct style");
    println!(
        "Button computed style: bg={:?}, color={:?}, padding={:?}",
        button_style.background_color, button_style.color, button_style.padding
    );

    // Find hidden label
    let hidden_label = find_element(&doc, &|n| {
        if let NodeBlueprint::Element { classes, .. } = n {
            classes.iter().any(|c| c == "hidden")
        } else {
            false
        }
    });
    println!("Hidden label element: {:?}", hidden_label.is_some());
    if let Some(lbl) = hidden_label {
        let lbl_style = get_direct_style(lbl).expect("label style");
        println!(
            "Hidden label style: width={:?}, height={:?}, overflow={:?}/{:?}, position={:?}",
            lbl_style.width,
            lbl_style.height,
            lbl_style.overflow_x,
            lbl_style.overflow_y,
            lbl_style.position
        );
    }

    // Find brand img
    let img = find_element(&doc, &|n| {
        if let NodeBlueprint::Element { tag, .. } = n {
            matches!(tag, Tag::Img | Tag::Svg)
        } else {
            false
        }
    })
    .expect("img must be found");
    let img_style = get_direct_style(img).expect("img style");
    println!(
        "Img computed style: width={:?}, height={:?}, margin={:?}",
        img_style.width, img_style.height, img_style.margin
    );
}

fn find_tag(node: &NodeBlueprint, tag: Tag) -> Option<&NodeBlueprint> {
    match node {
        NodeBlueprint::Element {
            tag: node_tag,
            children,
            ..
        } => {
            if *node_tag == tag {
                return Some(node);
            }
            for child in children {
                if let Some(found) = find_tag(child, tag) {
                    return Some(found);
                }
            }
            None
        }
        NodeBlueprint::Fragment(children) => children.iter().find_map(|c| find_tag(c, tag)),
        _ => None,
    }
}

#[test]
fn test_generic_document_root_line_height_matches_css_normal() {
    let html = r##"
        <!DOCTYPE html>
        <html>
            <head><title>Line height defaults</title></head>
            <body>
                <p>A paragraph without an author line-height declaration.</p>
                <table>
                    <thead><tr><th>Header</th></tr></thead>
                    <tbody><tr><td>Body cell</td></tr></tbody>
                </table>
            </body>
        </html>
    "##;

    let nodes = HtmlParser::parse(html);
    let theme = ReaderTheme::dark();
    let transpiler = WebTranspiler::new(&theme);
    let media_ctx = MediaContext::new(1280.0, 800.0, true);

    let doc = transpiler.transpile_full_document(&nodes, &media_ctx);

    let root_style = get_direct_style(&doc).expect("root wrapper must carry a direct style");
    assert_eq!(
        root_style.line_height, 1.2,
        "generic document root must use the CSS normal line-height"
    );

    let paragraph = find_tag(&doc, Tag::P).expect("<p> must be transpiled");
    let paragraph_style = get_direct_style(paragraph).expect("<p> must carry a direct style");
    assert_eq!(
        paragraph_style.line_height, 1.2,
        "<p> must inherit the CSS normal line-height from the root wrapper"
    );

    let table = find_tag(&doc, Tag::Table).expect("<table> must be transpiled");
    let table_style = get_direct_style(table).expect("<table> must carry a direct style");
    assert_eq!(
        table_style.line_height, 1.2,
        "<table> must inherit the CSS normal line-height from the root wrapper"
    );
}

#[test]
fn test_author_line_height_overrides_generic_document_normal() {
    let html = r##"
        <!DOCTYPE html>
        <html>
            <head>
                <style>
                    p { line-height: 1.75; }
                </style>
            </head>
            <body>
                <p>Author line-height must win.</p>
            </body>
        </html>
    "##;

    let nodes = HtmlParser::parse(html);
    let theme = ReaderTheme::dark();
    let transpiler = WebTranspiler::new(&theme);
    let media_ctx = MediaContext::new(1280.0, 800.0, true);

    let doc = transpiler.transpile_full_document(&nodes, &media_ctx);

    let root_style = get_direct_style(&doc).expect("root wrapper must carry a direct style");
    assert_eq!(root_style.line_height, 1.2);

    let paragraph = find_tag(&doc, Tag::P).expect("<p> must be transpiled");
    let paragraph_style = get_direct_style(paragraph).expect("<p> must carry a direct style");
    assert_eq!(
        paragraph_style.line_height, 1.75,
        "author CSS line-height must override the inherited CSS normal value"
    );
}

#[test]
fn test_full_document_uses_ua_white_canvas_regardless_of_reader_theme() {
    let html = r##"
        <!DOCTYPE html>
        <html>
            <head><title>Plain page</title></head>
            <body>
                <p>A plain page with no author colors.</p>
            </body>
        </html>
    "##;

    let nodes = HtmlParser::parse(html);

    for theme in [
        ReaderTheme::dark(),
        ReaderTheme::amoled(),
        ReaderTheme::sepia(),
        ReaderTheme::light(),
    ] {
        let theme_name = theme.name.clone();
        let transpiler = WebTranspiler::new(&theme);
        let media_ctx = MediaContext::new(1280.0, 800.0, false);

        let doc = transpiler.transpile_full_document(&nodes, &media_ctx);
        let root_style = get_direct_style(&doc).expect("root wrapper must carry a direct style");

        assert_eq!(
            root_style.background_color,
            Some(Color::new(255, 255, 255, 255)),
            "theme {theme_name} must not tint the document canvas"
        );
        assert_eq!(
            root_style.color,
            Color::new(0, 0, 0, 255),
            "theme {theme_name} must not rewrite the document text color"
        );

        // A paragraph with no author colors inherits the black UA text color.
        let paragraph = find_tag(&doc, Tag::P).expect("<p> must be transpiled");
        let paragraph_style = get_direct_style(paragraph).expect("<p> must carry a direct style");
        assert_eq!(
            paragraph_style.color,
            Color::new(0, 0, 0, 255),
            "theme {theme_name} must not recolor inherited text"
        );
    }
}

#[test]
fn test_full_document_preserves_author_background_and_color() {
    let html = r##"
        <!DOCTYPE html>
        <html>
            <head>
                <style>
                    .panel { background-color: #0f172a; color: #f8fafc; }
                </style>
            </head>
            <body>
                <div id="panel" class="panel">Dark author panel.</div>
            </body>
        </html>
    "##;

    let nodes = HtmlParser::parse(html);
    let theme = ReaderTheme::light();
    let transpiler = WebTranspiler::new(&theme);
    let media_ctx = MediaContext::new(1280.0, 800.0, false);

    let doc = transpiler.transpile_full_document(&nodes, &media_ctx);

    fn find_by_id<'a>(bp: &'a NodeBlueprint, id: &str) -> Option<&'a NodeBlueprint> {
        match bp {
            NodeBlueprint::Element {
                attributes,
                children,
                ..
            } => {
                if attributes.iter().any(|a| {
                    matches!(
                        a,
                        Attribute::Property { key, value }
                            if key.eq_ignore_ascii_case("id") && value == id
                    )
                }) {
                    return Some(bp);
                }
                children.iter().find_map(|c| find_by_id(c, id))
            }
            NodeBlueprint::Fragment(children) => children.iter().find_map(|c| find_by_id(c, id)),
            _ => None,
        }
    }

    let panel = find_by_id(&doc, "panel").expect("author panel must be transpiled");
    let panel_style = get_direct_style(panel).expect("panel must carry a direct style");

    assert_eq!(
        panel_style.background_color,
        Some(Color::new(15, 23, 42, 255)),
        "author background must override the white UA canvas"
    );
    assert_eq!(
        panel_style.color,
        Color::new(248, 250, 252, 255),
        "author text color must not be rewritten on a light reader theme"
    );
}

fn find_marker<'a>(node: &'a NodeBlueprint, marker: &str) -> Option<&'a NodeBlueprint> {
    match node {
        NodeBlueprint::Element {
            attributes,
            children,
            ..
        } => {
            let is_marker = attributes.iter().any(|a| {
                matches!(
                    a,
                    Attribute::Property { key, value }
                        if key == "data-strata-tag" && value == marker
                )
            });
            if is_marker {
                return Some(node);
            }
            children.iter().find_map(|c| find_marker(c, marker))
        }
        NodeBlueprint::Fragment(children) => children.iter().find_map(|c| find_marker(c, marker)),
        _ => None,
    }
}

fn full_document_canvas(css: &str) -> Option<Color> {
    let html = format!(
        r##"<!DOCTYPE html>
            <html>
                <head><style>{css}</style></head>
                <body><p>Canvas probe.</p></body>
            </html>"##
    );
    let nodes = HtmlParser::parse(&html);
    let theme = ReaderTheme::dark();
    let transpiler = WebTranspiler::new(&theme);
    let media_ctx = MediaContext::new(1280.0, 720.0, true);
    let doc = transpiler.transpile_full_document(&nodes, &media_ctx);
    let root_style = get_direct_style(&doc).expect("root wrapper must carry a direct style");
    root_style.background_color
}

#[test]
fn test_full_document_propagates_author_body_background_to_canvas() {
    let html = r##"
        <!DOCTYPE html>
        <html>
            <head>
                <style>
                    body { background: #09090b; color: #f4f4f5; }
                </style>
            </head>
            <body>
                <div class="card">Dark canvas.</div>
            </body>
        </html>
    "##;

    let nodes = HtmlParser::parse(html);
    let theme = ReaderTheme::dark();
    let transpiler = WebTranspiler::new(&theme);
    let media_ctx = MediaContext::new(1280.0, 720.0, true);

    let doc = transpiler.transpile_full_document(&nodes, &media_ctx);
    let root_style = get_direct_style(&doc).expect("root wrapper must carry a direct style");

    assert_eq!(
        root_style.background_color,
        Some(Color::new(9, 9, 11, 255)),
        "author body background must propagate to the document canvas, not white"
    );
    assert_ne!(
        root_style.background_color,
        Some(Color::new(255, 255, 255, 255)),
        "the white UA canvas must not show through a dark author body"
    );
    assert_eq!(
        root_style.color,
        Color::new(0, 0, 0, 255),
        "canvas text default stays black unless author CSS overrides it at the element level"
    );

    let body = find_marker(&doc, "body").expect("body marker must be emitted");
    let body_style = get_direct_style(body).expect("body must carry a direct style");
    assert_eq!(
        body_style.background_color,
        Some(Color::new(9, 9, 11, 255)),
        "the author body background itself must be preserved"
    );
    assert_eq!(
        body_style.color,
        Color::new(244, 244, 245, 255),
        "author body text color must be preserved"
    );
}

#[test]
fn test_full_document_prefers_html_background_over_body() {
    let canvas =
        full_document_canvas("html { background: #111827; } body { background: #09090b; }");
    assert_eq!(
        canvas,
        Some(Color::new(17, 24, 39, 255)),
        "the author html background wins over the author body background"
    );
}

#[test]
fn test_full_document_transparent_body_background_falls_back_to_white() {
    let canvas = full_document_canvas("body { background: transparent; color: #f4f4f5; }");
    assert_eq!(
        canvas,
        Some(Color::new(255, 255, 255, 255)),
        "a transparent author body background must fall back to the white UA canvas"
    );
}

#[test]
fn test_full_document_without_author_background_stays_white() {
    let canvas = full_document_canvas("body { margin: 0; }");
    assert_eq!(
        canvas,
        Some(Color::new(255, 255, 255, 255)),
        "documents without an author canvas background keep the white UA canvas"
    );
}

fn transpile_fragment(html: &str) -> NodeBlueprint {
    let nodes = HtmlParser::parse(html);
    let theme = ReaderTheme::dark();
    let transpiler = WebTranspiler::new(&theme);
    let media = MediaContext::new(1440.0, 900.0, true);
    transpiler.transpile_full_document(&nodes, &media)
}

fn document_children(document: &NodeBlueprint) -> &[NodeBlueprint] {
    match document {
        NodeBlueprint::Element { children, .. } => children,
        _ => panic!("transpiled document must be an element"),
    }
}

fn element_has_class(node: &NodeBlueprint, class: &str) -> bool {
    match node {
        NodeBlueprint::Element { classes, .. } => classes.iter().any(|c| c == class),
        _ => false,
    }
}

fn element_attribute<'a>(node: &'a NodeBlueprint, key: &str) -> Option<&'a str> {
    match node {
        NodeBlueprint::Element { attributes, .. } => attributes.iter().find_map(|a| match a {
            Attribute::Property { key: k, value } if k.eq_ignore_ascii_case(key) => {
                Some(value.as_str())
            }
            _ => None,
        }),
        _ => None,
    }
}

#[test]
fn test_empty_div_with_flex_one_class_survives_transpilation() {
    let document = transpile_fragment(r#"<div class="flex-1"></div>"#);
    let children = document_children(&document);
    assert_eq!(children.len(), 1, "the flex-1 spacer must survive");
    assert!(element_has_class(&children[0], "flex-1"));
}

#[test]
fn test_empty_absolute_overlay_with_classes_survives_transpilation() {
    let document = transpile_fragment(
        r#"<div class="absolute inset-0 flex items-center justify-center p-4"></div>"#,
    );
    let children = document_children(&document);
    assert_eq!(children.len(), 1, "the absolute overlay must survive");
    assert!(element_has_class(&children[0], "absolute"));
    assert!(element_has_class(&children[0], "inset-0"));
}

#[test]
fn test_bare_empty_div_is_still_eliminated() {
    let document = transpile_fragment("<div></div>");
    assert!(
        document_children(&document).is_empty(),
        "a bare empty div must still be dropped to avoid node bloat"
    );
}

#[test]
fn test_empty_div_with_id_or_data_attribute_survives_transpilation() {
    let with_id = transpile_fragment(r#"<div id="x"></div>"#);
    let id_children = document_children(&with_id);
    assert_eq!(id_children.len(), 1, "an identified element must survive");
    assert_eq!(element_attribute(&id_children[0], "id"), Some("x"));

    let with_data = transpile_fragment(r#"<div data-app-region="drag"></div>"#);
    let data_children = document_children(&with_data);
    assert_eq!(
        data_children.len(),
        1,
        "a data-attribute hit target must survive"
    );
    assert_eq!(
        element_attribute(&data_children[0], "data-app-region"),
        Some("drag")
    );
}

#[test]
fn test_custom_tag_bounded_cache_prevents_dos() {
    let mut html = String::new();
    for i in 0..1000 {
        html.push_str(&format!("<synth-tag-{}>content</synth-tag-{}>", i, i));
    }
    let document = transpile_fragment(&html);
    let children = document_children(&document);
    assert_eq!(children.len(), 1000);

    let mut custom_fallback_count = 0;
    for child in children {
        if let NodeBlueprint::Element { tag, .. } = child {
            if *tag == Tag::Custom("custom") {
                custom_fallback_count += 1;
            }
        }
    }
    // We generated 1000 unique synthetic tags, capacity is 512.
    // The overflow tags MUST fall back to "custom" to prevent unbounded heap memory leakage.
    assert!(
        custom_fallback_count >= 400,
        "Expected overflow tags to fallback to static custom variant, got {}",
        custom_fallback_count
    );
}
