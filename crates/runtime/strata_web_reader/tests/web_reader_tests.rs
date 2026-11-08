//! Integration Tests: Semantic HTML5 Tokenizer, Web Transpiler, Wikipedia Engine & Offline Cache.

#![allow(clippy::type_complexity)]

use strata_dom::{NodeBlueprint, Tag};
use strata_web_reader::prelude::*;

#[test]
fn test_html_tokenizer_and_entity_decoding() {
    let html = r#"
        <!DOCTYPE html>
        <!-- System Architecture Documentation -->
        <div id="main" class="article-container">
            <h1 title="Header &amp; Title">Cloud &mdash; Architecture &amp; Scale</h1>
            <p>Latency is &lt; 5ms with &nbsp; zero jitter &copy; 2026.</p>
            <img src="/img/cluster.png" alt="Node Cluster" />
            <br>
        </div>
    "#;

    let mut tokenizer = HtmlTokenizer::new(html);
    let tokens = tokenizer.tokenize();

    // Verify DocType emitted
    assert!(tokens.iter().any(|t| matches!(t, HtmlToken::DocType(_))));

    // Verify Comment emitted
    assert!(tokens.iter().any(|t| matches!(t, HtmlToken::Comment(c) if c.contains("System Architecture"))));

    // Verify entity decoding in StartTag attribute and Text
    let h1_start = tokens.iter().find(|t| match t {
        HtmlToken::StartTag { name, .. } => name == "h1",
        _ => false,
    }).expect("h1 tag must exist");

    if let HtmlToken::StartTag { attributes, .. } = h1_start {
        let title_attr = attributes.iter().find(|(k, _)| k == "title").unwrap();
        assert_eq!(title_attr.1, "Header & Title");
    }

    // Verify decoded mdash and amp in text
    let text_token = tokens.iter().find(|t| match t {
        HtmlToken::Text(s) => s.contains("Cloud — Architecture & Scale"),
        _ => false,
    });
    assert!(text_token.is_some(), "Text with &mdash; and &amp; must be decoded");

    // Verify self-closing img tag
    let img_token = tokens.iter().find(|t| match t {
        HtmlToken::StartTag { name, self_closing, .. } => name == "img" && *self_closing,
        _ => false,
    });
    assert!(img_token.is_some(), "img must be self closing");
}

#[test]
fn test_semantic_html_parser_tree_builder() {
    let html = r#"
        <div class="content">
            <h2>Telemetry Services</h2>
            <script>console.log("malicious telemetry tracking");</script>
            <style>.telemetry { display: none; }</style>
            <p>The cluster is healthy.</p>
            <ul>
                <li>Node Alpha</li>
                <li>Node Beta</li>
            </ul>
        </div>
    "#;

    let nodes = HtmlParser::parse(html);
    assert_eq!(nodes.len(), 1);

    let root = &nodes[0];
    assert_eq!(root.tag(), Some("div"));
    assert!(root.has_class("content"));

    // Ensure <script> was stripped and <style> was preserved for CSS extraction
    assert!(root.find_first("script").is_none(), "Scripts must be stripped");
    assert!(root.find_first("style").is_some(), "Styles must be preserved");

    // Verify children
    let h2 = root.find_first("h2").expect("h2 should exist");
    assert_eq!(h2.text_content().trim(), "Telemetry Services");

    let lis = root.find_all("li");
    assert_eq!(lis.len(), 2);
    assert_eq!(lis[0].text_content().trim(), "Node Alpha");
    assert_eq!(lis[1].text_content().trim(), "Node Beta");
}

#[test]
fn test_web_transpiler_node_blueprint_generation() {
    let html = r#"
        <article>
            <h1>Distributed Storage</h1>
            <p>Bitcask provides <strong>append-only</strong> logging.</p>
            <blockquote cite="https://larvance.dev">Simplicity is a prerequisite for reliability.</blockquote>
            <pre><code>fn write_record() {}</code></pre>
            <a href="https://github.com/larvance/strata">View Codebase</a>
        </article>
    "#;

    let nodes = HtmlParser::parse(html);
    let theme = ReaderTheme::dark();
    let transpiler = WebTranspiler::new(&theme);

    let blueprint = transpiler.transpile_document(&nodes, Some("Distributed Storage Architecture"));

    // Verify root is div with max reading width styling
    match blueprint {
        strata_dom::NodeBlueprint::Element { tag, classes, children, .. } => {
            assert_eq!(tag, Tag::Div);
            assert!(classes.iter().any(|c| c.contains("max-w-[760px]")));
            assert!(classes.iter().any(|c| c.contains("bg-neutral-950")));

            // Check H1 title injected
            assert!(children.len() >= 2);
            match &children[0] {
                strata_dom::NodeBlueprint::Element { tag, .. } => assert_eq!(*tag, Tag::H1),
                _ => panic!("Expected H1 element"),
            }
        }
        _ => panic!("Expected element root"),
    }
}

#[test]
fn test_web_transpiler_infobox_and_float_exclusions() {
    let html = r#"
        <article>
            <table class="infobox vevent">
                <tr><th>Language</th><td>Rust</td></tr>
            </table>
            <p>Rust is a systems programming language focused on safety.</p>
            <figure class="thumb tright">
                <img src="diagram.svg" alt="Diagram">
                <figcaption>Compiler Architecture</figcaption>
            </figure>
            <div style="float: left; width: 200px">
                <span>Left callout</span>
            </div>
            <div class="visualClear"></div>
            <p>Subsequent paragraph below the clear.</p>
        </article>
    "#;

    let nodes = HtmlParser::parse(html);
    let theme = ReaderTheme::dark();
    let transpiler = WebTranspiler::new(&theme);
    let doc = transpiler.transpile_document(&nodes, None);

    match doc {
        strata_dom::NodeBlueprint::Element { children, .. } => {
            let article = &children[0];
            match article {
                strata_dom::NodeBlueprint::Element { children: article_children, .. } => {
                    // 1. Infobox table has float-right and width constraint
                    let infobox = &article_children[0];
                    match infobox {
                        strata_dom::NodeBlueprint::Element { classes, tag, .. } => {
                            assert_eq!(*tag, Tag::Table);
                            assert!(classes.iter().any(|c| c.contains("float-right")));
                            assert!(classes.iter().any(|c| c.contains("w-80")));
                        }
                        _ => panic!("Expected table element"),
                    }

                    // 2. Figure has float-right
                    let figure = &article_children[2];
                    match figure {
                        strata_dom::NodeBlueprint::Element { classes, .. } => {
                            assert!(classes.iter().any(|c| c.contains("float-right")));
                            assert!(classes.iter().any(|c| c.contains("max-w-[300px]")));
                        }
                        _ => panic!("Expected figure element"),
                    }

                    // 3. Left callout has float-left
                    let left_callout = &article_children[3];
                    match left_callout {
                        strata_dom::NodeBlueprint::Element { classes, .. } => {
                            assert!(classes.iter().any(|c| c.contains("float-left")));
                        }
                        _ => panic!("Expected left callout element"),
                    }

                    // 4. Clearing div has clear-both
                    let clear_div = &article_children[4];
                    match clear_div {
                        strata_dom::NodeBlueprint::Element { classes, .. } => {
                            assert!(classes.iter().any(|c| c.contains("clear-both")));
                        }
                        _ => panic!("Expected clear element"),
                    }
                }
                _ => panic!("Expected article element"),
            }
        }
        _ => panic!("Expected doc root element"),
    }
}

#[test]
fn test_wikipedia_article_engine_infobox_and_toc() {
    let wikipedia_html = r##"
        <!DOCTYPE html>
        <html>
        <body>
            <h1 id="firstHeading">Rust (programming language)</h1>
            <table class="infobox vevent">
                <caption class="infobox-title">Rust</caption>
                <tr>
                    <td colspan="2" class="infobox-image">
                        <img src="//upload.wikimedia.org/wikipedia/commons/d/d5/Rust_programming_language_black_logo.svg" alt="Rust Logo">
                    </td>
                </tr>
                <tr>
                    <th scope="row">Designed by</th>
                    <td>Graydon Hoare</td>
                </tr>
                <tr>
                    <th scope="row">First appeared</th>
                    <td>July 7, 2010</td>
                </tr>
                <tr>
                    <th scope="row">Typing discipline</th>
                    <td>Static, affine, inferred</td>
                </tr>
            </table>

            <div id="toc" class="toc">
                <div class="toctitle"><h2>Contents</h2></div>
                <ul>
                    <li class="toclevel-1"><a href="#History"><span class="toctext">1 History</span></a></li>
                    <li class="toclevel-1"><a href="#Syntax"><span class="toctext">2 Syntax and semantics</span></a></li>
                </ul>
            </div>

            <p>Rust is a multi-paradigm, general-purpose programming language that emphasizes performance, type safety, and concurrency.</p>

            <h2>History</h2>
            <div class="mw-editsection"><a href="#">edit</a></div>
            <p>The language grew out of a personal project begun in 2006 by Mozilla employee Graydon Hoare.</p>

            <h2>Syntax and semantics</h2>
            <p>Rust is intended to be a concurrent, safe, practical language.</p>
        </body>
        </html>
    "##;

    let article = WikipediaEngine::parse(wikipedia_html);

    // Verify extracted title
    assert_eq!(article.title, "Rust (programming language)");

    // Verify extracted lead summary
    assert!(article.summary.contains("multi-paradigm, general-purpose programming language"));

    // Verify infobox extraction
    let infobox = article.infobox.as_ref().expect("Infobox must be extracted");
    assert_eq!(infobox.title, "Rust");
    assert!(infobox.image_url.as_ref().unwrap().starts_with("https://upload.wikimedia.org"));
    assert_eq!(infobox.rows.len(), 3);
    assert_eq!(infobox.rows[0].label, "Designed by");
    assert_eq!(infobox.rows[0].value, "Graydon Hoare");
    assert_eq!(infobox.rows[1].label, "First appeared");
    assert_eq!(infobox.rows[1].value, "July 7, 2010");

    // Verify Table of Contents
    assert_eq!(article.toc.len(), 2);
    assert_eq!(article.toc[0].anchor, "History");
    assert_eq!(article.toc[1].anchor, "Syntax");

    // Verify Sections
    assert!(article.sections.len() >= 2);

    // Render to native Strata blueprint
    let theme = ReaderTheme::sepia();
    let view = WikipediaEngine::render_to_blueprint(&article, &theme);

    match view {
        strata_dom::NodeBlueprint::Element { classes, children, .. } => {
            assert!(classes.iter().any(|c| c.contains("bg-[#fbf0d9]")), "Must use sepia background");
            assert!(children.len() >= 3, "Must contain title, infobox, and sections");
        }
        _ => panic!("Expected element root"),
    }
}

#[test]
fn test_reader_theme_and_color_palettes() {
    let dark = ReaderTheme::dark();
    assert_eq!(dark.name, "Dark");
    assert!(dark.bg_class.contains("bg-neutral-950"));

    let amoled = ReaderTheme::amoled();
    assert_eq!(amoled.name, "Amoled");
    assert_eq!(amoled.bg_class, "bg-black");

    let sepia = ReaderTheme::sepia();
    assert_eq!(sepia.name, "Sepia");
    assert!(sepia.bg_class.contains("fbf0d9"));

    let light = ReaderTheme::light();
    assert_eq!(light.name, "Light");
    assert_eq!(light.bg_class, "bg-white");

    let solarized = ReaderTheme::solarized();
    assert_eq!(solarized.name, "Solarized");
    assert!(solarized.bg_class.contains("002b36"));
}

#[test]
fn test_offline_article_cache_roundtrip() {
    let tmp_dir = std::env::temp_dir().join(format!("strata_reader_cache_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    std::fs::create_dir_all(&tmp_dir).unwrap();
    let wal_path = tmp_dir.join("test_articles.wal");

    let cache = ArticleCache::open(&wal_path).expect("Must open article cache");

    let slug = "rust-concurrency";
    let html = "<html><body><h1>Concurrency</h1><p>Fearless concurrency in Rust.</p></body></html>";

    assert!(!cache.contains(slug));

    cache.store(slug, html);
    assert!(cache.contains(slug));

    let fetched = cache.fetch(slug).expect("Must fetch stored article");
    assert_eq!(fetched, html);

    let all_slugs = cache.cached_articles();
    assert!(all_slugs.contains(&slug.to_string()));

    cache.remove(slug);
    assert!(!cache.contains(slug));

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn test_document_style_extraction_and_cascade_transpilation() {
    let html = r#"
        <html>
            <head>
                <style>
                    .card-box { background-color: #1e293b; border-color: #334155; }
                    .article-header { color: #3b82f6; font-size: 24px; font-weight: bold; }
                    .card-box > p { color: #94a3b8; }
                </style>
            </head>
            <body>
                <div class="card-box">
                    <h2 class="article-header">Kernel Architecture</h2>
                    <p>Microkernel design in Rust.</p>
                </div>
            </body>
        </html>
    "#;

    let nodes = HtmlParser::parse(html);
    let theme = ReaderTheme::dark();
    let transpiler = WebTranspiler::new(&theme);
    let blueprint = transpiler.transpile_document(&nodes, None);

    fn find_blueprint_element<'a>(
        node: &'a strata_dom::NodeBlueprint,
        predicate: &dyn Fn(strata_dom::Tag, &strata_dom::ClassList, &[strata_dom::Attribute]) -> bool,
    ) -> Option<&'a strata_dom::NodeBlueprint> {
        match node {
            strata_dom::NodeBlueprint::Element {
                tag,
                classes,
                attributes,
                children,
            } => {
                if predicate(*tag, classes, attributes) {
                    return Some(node);
                }
                for child in children {
                    if let Some(found) = find_blueprint_element(child, predicate) {
                        return Some(found);
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn get_style_attr(node: &strata_dom::NodeBlueprint) -> Option<&str> {
        match node {
            strata_dom::NodeBlueprint::Element { attributes, .. } => {
                attributes.iter().find_map(|a| match a {
                    strata_dom::Attribute::Property { key, value }
                        if key.eq_ignore_ascii_case("style") =>
                    {
                        Some(value.as_str())
                    }
                    _ => None,
                })
            }
            _ => None,
        }
    }

    // Verify card-box received cascaded background color rgba(30,41,59,1) and border color rgba(51,65,85,1)
    let card_node = find_blueprint_element(&blueprint, &|_tag, classes, _attrs| {
        classes.iter().any(|c| c.contains("card-box"))
    })
    .expect("card-box must be transpiled into blueprint");

    let card_style = get_style_attr(card_node).expect("card-box must have style attribute");
    assert!(
        card_style.contains("background-color: rgba(30,41,59,1)"),
        "card-box style: {}",
        card_style
    );
    assert!(
        card_style.contains("border-color: rgba(51,65,85,1)"),
        "card-box style: {}",
        card_style
    );

    // Verify article-header h2 received cascaded color, font size, and font weight
    let h2_node = find_blueprint_element(card_node, &|tag, _classes, _attrs| tag == Tag::H2)
        .expect("article-header h2 must be in blueprint");

    let h2_style = get_style_attr(h2_node).expect("h2 must have style attribute");
    assert!(
        h2_style.contains("color: rgba(59,130,246,1)"),
        "h2 style: {}",
        h2_style
    );
    assert!(
        h2_style.contains("font-size: 24px"),
        "h2 style: {}",
        h2_style
    );
    assert!(
        h2_style.contains("font-weight: 700"),
        "h2 style: {}",
        h2_style
    );

    // Verify child combinator (.card-box > p) matched p child with color rgba(148,163,184,1)
    let p_node = find_blueprint_element(card_node, &|tag, _classes, _attrs| tag == Tag::P)
        .expect("p must be in blueprint");

    let p_style = get_style_attr(p_node).expect("p must have style attribute");
    assert!(
        p_style.contains("color: rgba(148,163,184,1)"),
        "p style: {}",
        p_style
    );
}

#[test]
fn test_wikipedia_citation_inline_phrasing_and_flow() {
    let citation_html = r##"
        <li id="cite_note-1" class="reference-item">
            <span class="mw-cite-backlink"><a href="#ref-1"><span class="mw-linkback-text">↑</span></a></span>
            <span class="mw-reference-text reference-text">
                <cite class="citation web cs1">Tung, Liam (2020-04-30). <a href="https://example.com" class="external text">"Microsoft Project Verona"</a>. <i>The Register</i>. <a href="https://archive.org" class="external text">Archived</a> from the original on 2020-05-01<span class="reference-accessdate">. Retrieved <span class="nowrap">2020-05-01</span></span>.</cite>
            </span>
        </li>
    "##;

    let theme = ReaderTheme::dark();
    let transpiler = WebTranspiler::new(&theme);
    let parsed_nodes = HtmlParser::parse(citation_html);
    let blueprint = transpiler.transpile_node(&parsed_nodes[0]).expect("Must transpile li");

    // 1. Verify li is NOT a flex container with wrapping (which broke citations)
    match &blueprint {
        NodeBlueprint::Element { tag, classes, children, .. } => {
            assert_eq!(*tag, Tag::Li);
            let class_str = classes.join(" ");
            assert!(
                !class_str.contains("flex-wrap"),
                "li must not be a flex-wrap container, was: {}",
                class_str
            );
            assert!(
                class_str.contains("list-item"),
                "li must be styled as list-item, was: {}",
                class_str
            );

            // 2. Verify all phrasing children have inline class
            let mut all_spans_are_inline = true;
            fn check_inline(bp: &NodeBlueprint, all_inline: &mut bool) {
                if let NodeBlueprint::Element { tag, classes, children, .. } = bp {
                    let cls = classes.join(" ");
                    if matches!(tag, Tag::Span | Tag::A)
                        && !cls.contains("inline") {
                            *all_inline = false;
                        }
                    for c in children {
                        check_inline(c, all_inline);
                    }
                }
            }
            for c in children {
                check_inline(c, &mut all_spans_are_inline);
            }
            assert!(
                all_spans_are_inline,
                "All phrasing elements (span, a, cite, i) must contain 'inline' class"
            );
        }
        _ => panic!("Expected element blueprint for li"),
    }
}

#[test]
fn test_wikipedia_article_inline_whitespace_and_word_spacing() {
    let html = r#"
        <p>memory safety. Rust supports multiple <a rel="mw:WikiLink" href="./Programming_paradigm" title="Programming paradigm">programming paradigms</a>. It was influenced by ideas from <a rel="mw:WikiLink" href="./Functional_programming" title="Functional programming">functional programming</a>, including <a rel="mw:WikiLink" href="./Immutable_object" title="Immutable object">immutability</a>, <a rel="mw:WikiLink" href="./Higher-order_function" title="Higher-order function">higher-order functions</a>, <a rel="mw:WikiLink" href="./Algebraic_data_type" title="Algebraic data type">algebraic data types</a>, and <a rel="mw:WikiLink" href="./Pattern_matching" title="Pattern matching">pattern matching</a>.</p>
    "#;
    let theme = ReaderTheme::dark();
    let transpiler = WebTranspiler::new(&theme);
    let parsed_nodes = HtmlParser::parse(html);
    let blueprint = transpiler.transpile_node(&parsed_nodes[0]).expect("Must transpile p");

    if let NodeBlueprint::Element { children, .. } = blueprint {
        let texts: Vec<String> = children.iter().filter_map(|c| match c {
            NodeBlueprint::Text(t) => Some(t.clone()),
            _ => None,
        }).collect();
        assert!(texts.iter().any(|t| t.ends_with("supports multiple ")), "Must preserve trailing space before link");
        assert!(texts.iter().any(|t| t.starts_with(". It was influenced by ideas from ")), "Must preserve leading space and period after link");
        assert!(texts.iter().any(|t| t.starts_with(", including ")), "Must preserve comma and trailing space");
    } else {
        panic!("Expected element");
    }
}

#[test]
fn test_stream_tokenizer_chunk_boundary_preservation() {
    let mut stream = StreamTokenizer::new();

    // Chunk 1: open tag and text
    let t1 = stream.feed_chunk("<div class=\"hero\">Hello ");
    assert_eq!(t1.len(), 2); // <div class="hero"> and "Hello " emitted
    match &t1[0] {
        HtmlToken::StartTag { name, attributes, .. } => {
            assert_eq!(name, "div");
            assert_eq!(attributes[0], ("class".to_string(), "hero".to_string()));
        }
        _ => panic!("Expected StartTag"),
    }
    assert_eq!(t1[1], HtmlToken::Text("Hello ".to_string()));

    // Chunk 2: split HTML entity across chunk boundary: `&g` + `e;`
    let t2 = stream.feed_chunk("&g");
    assert!(t2.is_empty(), "Incomplete entity should remain buffered");

    // Chunk 3: entity completion and closing tag
    let t3 = stream.feed_chunk("e; 100</div>");
    assert_eq!(t3.len(), 2);
    assert_eq!(t3[0], HtmlToken::Text("≥ 100".to_string()));
    assert_eq!(t3[1], HtmlToken::EndTag { name: "div".to_string() });

    // Stream finish
    let t4 = stream.finish();
    assert!(t4.is_empty());
}

#[test]
fn test_resource_urls_prefetch_extraction() {
    let html = r#"
        <!DOCTYPE html>
        <html>
        <head>
            <link rel="stylesheet" href="https://cdn.strata.dev/theme.css">
            <link rel="icon" href="/favicon.ico">
        </head>
        <body>
            <img src="https://images.strata.dev/logo.png" alt="Logo">
            <p>Some text</p>
            <img src="/static/chart.svg" alt="Chart">
        </body>
        </html>
    "#;
    let nodes = HtmlParser::parse(html);
    let urls = WebTranspiler::extract_resource_urls(&nodes);

    assert_eq!(urls.len(), 3);
    assert_eq!(urls[0], "https://cdn.strata.dev/theme.css");
    assert_eq!(urls[1], "https://images.strata.dev/logo.png");
    assert_eq!(urls[2], "/static/chart.svg");
}

