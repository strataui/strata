//! Integration Tests: Semantic HTML5 Tokenizer, Web Transpiler, Wikipedia Engine & Offline Cache.

use strata_dom::Tag;
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

    // Ensure <script> and <style> were stripped
    assert!(root.find_first("script").is_none(), "Scripts must be stripped");
    assert!(root.find_first("style").is_none(), "Styles must be stripped");

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
            assert!(classes[0].contains("max-w-[760px]"));
            assert!(classes[0].contains("bg-neutral-950"));

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
            assert!(classes[0].contains("bg-[#fbf0d9]"), "Must use sepia background");
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
