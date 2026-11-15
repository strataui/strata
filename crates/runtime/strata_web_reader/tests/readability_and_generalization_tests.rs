//! Readability Heuristics and Generalized Reader Transpilation Integration Tests.
//!
//! Validates:
//! 1. Readability score calculation for generic HTML articles, blogs, and documentation pages
//! 2. Primary content container extraction across non-Wikipedia and non-MediaWiki layouts
//! 3. Link density penalty enforcement on navigational sidebars and header chrome
//! 4. Generic article metadata extraction (title, author, publication date, description)
//! 5. Seamless switching between Reader and FullDom rendering modes

use strata_dom::NodeBlueprint;
use strata_style::MediaContext;
use strata_web_reader::prelude::*;

fn contains_text(node: &NodeBlueprint, needle: &str) -> bool {
    match node {
        NodeBlueprint::Text(t) => t.contains(needle),
        NodeBlueprint::Element { children, .. } | NodeBlueprint::Fragment(children) => {
            children.iter().any(|c| contains_text(c, needle))
        }
        _ => false,
    }
}

#[test]
fn test_readability_score_heuristics() {
    // 1. Substantive article container
    let article_html = r#"
        <article class="post-content">
            <h1>Autonomous Robotics Systems</h1>
            <p>State estimation algorithms require tightly coupled inertial measurement units and visual odometry data streams.</p>
            <p>Nonlinear factor graph optimization computes the maximum a posteriori trajectory across receding horizons.</p>
        </article>
    "#;
    let article_node = &HtmlParser::parse(article_html)[0];
    let article_score = WebTranspiler::calculate_readability_score(article_node);
    assert!(
        article_score > 70.0,
        "Article container must receive high readability score, got {}",
        article_score
    );

    // 2. High link-density navigation menu (should receive negative score)
    let nav_html = r#"
        <nav class="sidebar-menu">
            <ul>
                <li><a href="/docs/intro">Getting Started</a></li>
                <li><a href="/docs/installation">Installation</a></li>
                <li><a href="/docs/architecture">Architecture</a></li>
                <li><a href="/docs/api">API Reference</a></li>
                <li><a href="/docs/faq">Frequently Asked Questions</a></li>
            </ul>
        </nav>
    "#;
    let nav_node = &HtmlParser::parse(nav_html)[0];
    let nav_score = WebTranspiler::calculate_readability_score(nav_node);
    assert!(
        nav_score < 0.0,
        "Navigation container must receive negative readability score due to tag and link density, got {}",
        nav_score
    );

    // 3. ARIA role main boost
    let role_main_html = r#"
        <div role="main" class="entry">
            <p>Substantive content paragraph detailing distributed consensus protocols and leader election algorithms.</p>
            <p>Raft decomposes consensus into leader election, log replication, and safety invariants.</p>
        </div>
    "#;
    let role_main_node = &HtmlParser::parse(role_main_html)[0];
    let role_main_score = WebTranspiler::calculate_readability_score(role_main_node);
    assert!(
        role_main_score > 60.0,
        "Container with role='main' must receive high readability score, got {}",
        role_main_score
    );
}

#[test]
fn test_generic_news_article_reader_extraction() {
    let html = r#"
        <!DOCTYPE html>
        <html>
            <head>
                <title>Quantum Computing Breakthrough</title>
                <meta property="og:title" content="Quantum Computing Breakthrough Announced" />
                <meta name="author" content="Dr. Evelyn Vance" />
                <meta property="article:published_time" content="2026-11-09T08:00:00Z" />
                <meta name="description" content="Researchers unveil room-temperature coherence in silicon quantum dots." />
            </head>
            <body>
                <header class="site-header">
                    <nav class="navbar">
                        <a href="/">Home</a>
                        <a href="/tech">Technology</a>
                        <a href="/science">Science</a>
                    </nav>
                </header>
                <main class="page-container">
                    <div class="sidebar ads">
                        <p>Sponsored Advertisement</p>
                        <a href="/promo">Buy Now</a>
                    </div>
                    <article class="news-story">
                        <h1>Quantum Computing Breakthrough Announced</h1>
                        <p class="lead">Researchers at the Institute of Advanced Physics have demonstrated sustained room-temperature quantum coherence in silicon quantum dot arrays.</p>
                        <p>The breakthrough overcomes decades-old cryogenic limitations, paving the way for commercial modular quantum accelerators integrated into standard server racks.</p>
                        <p>Error-corrected logical qubits were maintained with a 99.98% two-qubit gate fidelity over continuous multi-hour runs.</p>
                    </article>
                </main>
                <footer class="site-footer">
                    <p>&copy; 2026 Global Science News. All rights reserved.</p>
                    <nav><a href="/privacy">Privacy Policy</a><a href="/terms">Terms</a></nav>
                </footer>
            </body>
        </html>
    "#;

    let nodes = HtmlParser::parse(html);
    let theme = ReaderTheme::dark();
    let media = MediaContext::new(1280.0, 800.0, true);

    // Verify metadata extraction
    let meta = WebTranspiler::extract_metadata(&nodes);
    assert_eq!(
        meta.title.as_deref(),
        Some("Quantum Computing Breakthrough Announced")
    );
    assert_eq!(meta.author.as_deref(), Some("Dr. Evelyn Vance"));
    assert_eq!(meta.date.as_deref(), Some("2026-11-09T08:00:00Z"));
    assert_eq!(
        meta.description.as_deref(),
        Some("Researchers unveil room-temperature coherence in silicon quantum dots.")
    );

    // Verify primary content extraction
    let primary = WebTranspiler::extract_primary_content(&nodes);
    assert!(
        !primary.is_empty(),
        "Primary content slice must not be empty"
    );

    // The primary content must contain the article text and NOT site header or footer
    let transpiler = WebTranspiler::new(&theme).with_render_mode(RenderMode::Reader);
    let blueprint = transpiler.transpile(&nodes, None, &media);

    assert!(
        contains_text(&blueprint, "Quantum Computing Breakthrough Announced"),
        "Reader view must include the article headline"
    );
    assert!(
        contains_text(&blueprint, "room-temperature quantum coherence"),
        "Reader view must include substantive article paragraph text"
    );
    assert!(
        !contains_text(&blueprint, "Sponsored Advertisement"),
        "Reader view must strip out ads and sidebar clutter"
    );
    assert!(
        !contains_text(&blueprint, "Privacy Policy"),
        "Reader view must strip out site footer navigation"
    );
}

#[test]
fn test_generic_blog_without_article_tag() {
    // Non-semantic blog that uses styled div containers instead of <article>
    let html = r#"
        <div class="site-wrapper">
            <div class="top-nav">
                <a href="/">Index</a>
                <a href="/archive">Archive</a>
            </div>
            <div class="main-body">
                <div class="story-body">
                    <h2>Compiling WebAssembly with Zero-Cost Cranelift Backends</h2>
                    <p>Cranelift generates machine code through SSA intermediate representations designed for rapid single-pass compilation.</p>
                    <p>By eliminating heavyweight register allocation loops, compile times drop below 15 nanoseconds per instruction.</p>
                </div>
                <div class="social-share">
                    <a href="/share">Share on Mastodon</a>
                </div>
            </div>
        </div>
    "#;

    let nodes = HtmlParser::parse(html);
    let theme = ReaderTheme::dark();
    let media = MediaContext::new(1280.0, 800.0, true);

    let transpiler = WebTranspiler::new(&theme).with_render_mode(RenderMode::Reader);
    let blueprint = transpiler.transpile(&nodes, None, &media);

    assert!(
        contains_text(
            &blueprint,
            "Compiling WebAssembly with Zero-Cost Cranelift Backends"
        ),
        "Must extract story heading from generic story-body div"
    );
    assert!(
        contains_text(&blueprint, "single-pass compilation"),
        "Must extract story paragraph content"
    );
}

#[test]
fn test_render_mode_switching() {
    let html = r#"
        <!DOCTYPE html>
        <html>
            <body style="background-color: #1e293b; color: #f1f5f9;">
                <nav id="header-nav"><a href="/">Home</a></nav>
                <article>
                    <h1>Distributed Consensus Architecture</h1>
                    <p>Paxos and Raft provide linearizable state machine replication across unreliable networks.</p>
                </article>
            </body>
        </html>
    "#;

    let nodes = HtmlParser::parse(html);
    let theme = ReaderTheme::dark();
    let media = MediaContext::new(1280.0, 800.0, true);

    // 1. FullDom mode: renders full document tree including nav
    let full_transpiler = WebTranspiler::new(&theme).with_render_mode(RenderMode::FullDom);
    let full_doc = full_transpiler.transpile(&nodes, None, &media);
    assert!(
        contains_text(&full_doc, "Home"),
        "FullDom mode must render header nav link"
    );
    assert!(
        contains_text(&full_doc, "Distributed Consensus Architecture"),
        "FullDom mode must render article heading"
    );

    // 2. Reader mode: extracts primary content and excludes nav
    let reader_transpiler = WebTranspiler::new(&theme).with_render_mode(RenderMode::Reader);
    let reader_doc = reader_transpiler.transpile(&nodes, None, &media);
    assert!(
        !contains_text(&reader_doc, "Home"),
        "Reader mode must extract only article content and omit header nav"
    );
    assert!(
        contains_text(&reader_doc, "Distributed Consensus Architecture"),
        "Reader mode must preserve article content"
    );
}

#[test]
fn test_wikipedia_backward_compatibility() {
    let html = r#"
        <div id="content" class="mw-body">
            <h1 id="firstHeading">Rust (programming language)</h1>
            <div id="bodyContent">
                <div class="mw-parser-output">
                    <p>Rust is a multi-paradigm, high-performance general-purpose programming language.</p>
                    <p>It emphasizes memory safety and concurrency through compile-time ownership tracking.</p>
                </div>
            </div>
        </div>
    "#;

    let nodes = HtmlParser::parse(html);
    let theme = ReaderTheme::dark();
    let media = MediaContext::new(1280.0, 800.0, true);

    let transpiler = WebTranspiler::new(&theme).with_render_mode(RenderMode::Reader);
    let blueprint = transpiler.transpile(&nodes, None, &media);

    assert!(
        contains_text(&blueprint, "Rust is a multi-paradigm"),
        "Wikipedia content must be cleanly extracted via generalized scoring"
    );
    assert!(
        contains_text(&blueprint, "compile-time ownership tracking"),
        "Wikipedia paragraphs must be present in output blueprint"
    );
}
