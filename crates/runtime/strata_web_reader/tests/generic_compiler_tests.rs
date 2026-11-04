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
        NodeBlueprint::Element { attributes, .. } => {
            attributes.iter().find_map(|a| match a {
                Attribute::DirectStyle(any) => any.downcast_ref::<ComputedStyle>().cloned(),
                _ => None,
            })
        }
        _ => None,
    }
}

fn get_attribute_value<'a>(node: &'a NodeBlueprint, attr_name: &str) -> Option<&'a str> {
    match node {
        NodeBlueprint::Element { attributes, .. } => {
            attributes.iter().find_map(|a| match a {
                Attribute::Property { key, value } if key.eq_ignore_ascii_case(attr_name) => {
                    Some(value.as_str())
                }
                _ => None,
            })
        }
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
    let media_ctx = MediaContext {
        viewport_width: 1280.0,
        viewport_height: 800.0,
        is_dark: true,
    };

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
            NodeBlueprint::Element { attributes, children, .. } => {
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
    let card_style = get_direct_style(card).expect("card-1 must have direct ComputedStyle attached");

    // Verify dynamic author style was applied without hardcoding
    assert_eq!(card_style.background_color, Some(Color::new(15, 23, 42, 255)));
    assert_eq!(card_style.color, Color::new(248, 250, 252, 255));
    assert_eq!(card_style.font_size, 18.0);
    assert_eq!(card_style.border_width.top, 2.0);
    assert_eq!(card_style.border_color, Some(Color::new(56, 189, 248, 255)));
    // Verify inline style padding: 16px was merged
    assert_eq!(card_style.padding.top, 16.0);
    assert_eq!(card_style.padding.left, 16.0);

    // Verify H2 inside card inherited and cascaded styles
    let h2 = match card {
        NodeBlueprint::Element { children, .. } => {
            children.iter().find(|c| matches!(c, NodeBlueprint::Element { tag: Tag::H2, .. })).expect("H2 child")
        }
        _ => panic!("Expected card element"),
    };
    let h2_style = get_direct_style(h2).expect("H2 must have ComputedStyle");
    assert_eq!(h2_style.color, Color::new(168, 85, 247, 255));
    assert_eq!(h2_style.font_weight, FontWeight::ExtraBold); // 800

    // Verify SVG was NOT stripped
    let svg = match card {
        NodeBlueprint::Element { children, .. } => {
            children.iter().find(|c| matches!(c, NodeBlueprint::Element { tag: Tag::Svg, .. })).expect("SVG must be preserved")
        }
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
    let media_ctx = MediaContext {
        viewport_width: 1280.0,
        viewport_height: 800.0,
        is_dark: true,
    };

    let doc = transpiler.transpile_full_document(&nodes, &media_ctx);

    fn find_by_class<'a>(bp: &'a NodeBlueprint, class_name: &str) -> Option<&'a NodeBlueprint> {
        match bp {
            NodeBlueprint::Element { classes, children, .. } => {
                if classes.iter().any(|c| c.split_whitespace().any(|tok| tok == class_name)) {
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
            assert_eq!(children.len(), 3, "Mixed inlines and blocks must be restructured into anonymous blocks");

            // Child 0: Anonymous block containing inlines
            match &children[0] {
                NodeBlueprint::Element { classes, children: anon_children, .. } => {
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
                NodeBlueprint::Element { classes, children: anon_children, .. } => {
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
    let media_ctx = MediaContext {
        viewport_width: 1280.0,
        viewport_height: 800.0,
        is_dark: true,
    };

    let doc = transpiler.transpile_full_document(&nodes, &media_ctx);

    fn collect_all_ids<'a>(bp: &'a NodeBlueprint, ids: &mut Vec<String>) {
        if let NodeBlueprint::Element { attributes, children, .. } = bp {
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

    fn collect_all_hrefs<'a>(bp: &'a NodeBlueprint, hrefs: &mut Vec<String>) {
        if let NodeBlueprint::Element { attributes, children, .. } = bp {
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
