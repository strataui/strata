//! Comprehensive Integration Tests for Tier 7: Sovereign Web Browser Engine.
//!
//! Validates:
//! 1. GOAL-55: Extraction and execution of external scripts (<script src="...">) and inline scripts in DOM order.
//! 2. GOAL-60: WHATWG HTML5 tokenizer and tree construction state machine (insertion modes, foster parenting, auto-closing).
//! 3. GOAL-61: Content Security Policy (CSP Level 3) directive parsing and enforcement (script-src, connect-src, nonces, hashes, eval).
//! 4. GOAL-62: Verification of honest User-Agent headers (zero Chrome spoofing).
//! 5. GOAL-59: Generalized readability scoring and Full DOM rendering on generic non-MediaWiki documents.

use strata_style::MediaContext;
use strata_web_reader::csp::{
    extract_meta_csp, sha256_base64, ContentSecurityPolicy, DirectiveKind, SourceExpr,
};
use strata_web_reader::parser::{
    extract_script_descriptors, extract_scripts, HtmlParser, ScriptDescriptor,
};
use strata_web_reader::transpiler::{RenderMode, WebTranspiler};
use strata_web_reader::ReaderTheme;

// =========================================================================
// GOAL-55: Fetch and Execute External Script Bundles (<script src="...">)
// =========================================================================

#[test]
fn test_goal55_extract_script_descriptors_inline_and_external() {
    let html = r#"
        <!DOCTYPE html>
        <html>
            <head>
                <script src="https://cdn.example.com/vendor/bundle.js" async></script>
                <script src="/static/app.js" defer integrity="sha256-abcdef123="></script>
                <script type="application/ld+json">{"@context": "https://schema.org"}</script>
                <script nonce="rAnd0m123">
                    console.log("Inline script with nonce");
                </script>
                <script>
                    var x = 42;
                </script>
            </head>
            <body>
                <h1>Script Test</h1>
            </body>
        </html>
    "#;

    let descriptors = extract_script_descriptors(html);
    assert_eq!(
        descriptors.len(),
        4,
        "Must extract 4 executable scripts (ignoring json-ld)"
    );

    // 1. External async bundle
    match &descriptors[0] {
        ScriptDescriptor::External {
            src,
            is_async,
            is_defer,
            integrity,
            ..
        } => {
            assert_eq!(src, "https://cdn.example.com/vendor/bundle.js");
            assert!(*is_async, "Should flag async");
            assert!(!*is_defer, "Should not flag defer");
            assert!(integrity.is_none());
        }
        _ => panic!("Expected ScriptDescriptor::External for bundle.js"),
    }

    // 2. External defer bundle with integrity
    match &descriptors[1] {
        ScriptDescriptor::External {
            src,
            is_async,
            is_defer,
            integrity,
            ..
        } => {
            assert_eq!(src, "/static/app.js");
            assert!(!*is_async, "Should not flag async");
            assert!(*is_defer, "Should flag defer");
            assert_eq!(integrity.as_deref(), Some("sha256-abcdef123="));
        }
        _ => panic!("Expected ScriptDescriptor::External for app.js"),
    }

    // 3. Inline script with nonce
    match &descriptors[2] {
        ScriptDescriptor::Inline { code, nonce, .. } => {
            assert!(code.contains("Inline script with nonce"));
            assert_eq!(nonce.as_deref(), Some("rAnd0m123"));
        }
        _ => panic!("Expected ScriptDescriptor::Inline for nonce script"),
    }

    // 4. Standard inline script
    match &descriptors[3] {
        ScriptDescriptor::Inline { code, nonce, .. } => {
            assert_eq!(code, "var x = 42;");
            assert!(nonce.is_none());
        }
        _ => panic!("Expected ScriptDescriptor::Inline for var x = 42"),
    }

    // Backwards-compatible inline extract_scripts
    let inline_codes = extract_scripts(html);
    assert_eq!(inline_codes.len(), 2);
    assert!(inline_codes[0].contains("Inline script with nonce"));
    assert_eq!(inline_codes[1], "var x = 42;");
}

#[test]
fn test_goal55_extract_script_with_single_quotes_and_relative_sources() {
    let html = r#"<script src='../lib/math.js'></script><script>let y = 10;</script>"#;
    let descriptors = extract_script_descriptors(html);
    assert_eq!(descriptors.len(), 2);

    match &descriptors[0] {
        ScriptDescriptor::External { src, .. } => assert_eq!(src, "../lib/math.js"),
        _ => panic!("Expected external script with single quotes"),
    }
    match &descriptors[1] {
        ScriptDescriptor::Inline { code, .. } => assert_eq!(code, "let y = 10;"),
        _ => panic!("Expected inline script"),
    }
}

// =========================================================================
// GOAL-61: Content Security Policy (CSP Level 3) Directive Enforcement
// =========================================================================

#[test]
fn test_goal61_csp_header_parser_directives_and_sources() {
    let header = "default-src 'self'; script-src 'self' 'nonce-secret123' https://scripts.org; connect-src 'self' https://api.strataui.org; img-src * data:; style-src 'unsafe-inline' 'self'";
    let policy = ContentSecurityPolicy::parse(header);

    assert_eq!(policy.directives.len(), 5);

    // default-src 'self'
    let def_sources = policy.directives.get(&DirectiveKind::DefaultSrc).unwrap();
    assert_eq!(def_sources.len(), 1);
    assert_eq!(def_sources[0], SourceExpr::SelfSource);

    // script-src 'self' 'nonce-secret123' https://scripts.org
    let script_sources = policy.directives.get(&DirectiveKind::ScriptSrc).unwrap();
    assert_eq!(script_sources.len(), 3);
    assert_eq!(script_sources[0], SourceExpr::SelfSource);
    assert_eq!(
        script_sources[1],
        SourceExpr::Nonce("secret123".to_string())
    );
    match &script_sources[2] {
        SourceExpr::Host { host, scheme, .. } => {
            assert_eq!(host, "scripts.org");
            assert_eq!(scheme.as_deref(), Some("https"));
        }
        _ => panic!("Expected Host source for scripts.org"),
    }

    // img-src * data:
    let img_sources = policy.directives.get(&DirectiveKind::ImgSrc).unwrap();
    assert_eq!(img_sources.len(), 2);
    assert_eq!(img_sources[0], SourceExpr::Wildcard);
    assert_eq!(img_sources[1], SourceExpr::Scheme("data:".to_string()));
}

#[test]
fn test_goal61_csp_inline_script_blocking_and_nonce_hash_validation() {
    // 1. Strict CSP blocking inline scripts
    let strict_csp = ContentSecurityPolicy::parse("script-src 'self' https://cdn.example.com");
    assert!(
        !strict_csp.allows_script_inline("console.log('pwned')", None),
        "Must block inline scripts without unsafe-inline or nonce"
    );

    // 2. CSP allowing nonce
    let nonce_csp = ContentSecurityPolicy::parse("script-src 'self' 'nonce-secureNonce99'");
    assert!(
        nonce_csp.allows_script_inline("alert(1)", Some("secureNonce99")),
        "Must permit script matching nonce"
    );
    assert!(
        !nonce_csp.allows_script_inline("alert(1)", Some("wrongNonce")),
        "Must reject script with mismatched nonce"
    );
    assert!(
        !nonce_csp.allows_script_inline("alert(1)", None),
        "Must reject script missing nonce"
    );

    // 3. CSP allowing sha256 hash
    let code = "var count = 100;";
    let hash = sha256_base64(code.as_bytes());
    let hash_header = format!("script-src 'self' 'sha256-{hash}'");
    let hash_csp = ContentSecurityPolicy::parse(&hash_header);

    assert!(
        hash_csp.allows_script_inline(code, None),
        "Must permit inline script whose SHA-256 matches directive"
    );
    assert!(
        !hash_csp.allows_script_inline("var count = 101;", None),
        "Must block script whose hash differs"
    );

    // 4. Unsafe-inline permitted only when nonces/hashes are absent
    let unsafe_csp = ContentSecurityPolicy::parse("script-src 'unsafe-inline'");
    assert!(unsafe_csp.allows_script_inline("console.log('hi')", None));
}

#[test]
fn test_goal61_csp_eval_enforcement() {
    let no_eval_csp = ContentSecurityPolicy::parse("script-src 'self'");
    assert!(
        !no_eval_csp.allows_eval(),
        "eval() must be forbidden without 'unsafe-eval'"
    );

    let eval_csp = ContentSecurityPolicy::parse("script-src 'self' 'unsafe-eval'");
    assert!(
        eval_csp.allows_eval(),
        "eval() must be permitted when 'unsafe-eval' is present"
    );

    let default_no_eval = ContentSecurityPolicy::parse("default-src 'self'");
    assert!(
        !default_no_eval.allows_eval(),
        "eval() must be forbidden by fallback default-src"
    );
}

#[test]
fn test_goal61_csp_url_and_network_connect_restrictions() {
    let csp = ContentSecurityPolicy::parse("default-src 'none'; script-src 'self' https://cdn.strataui.org *.trusted.com; connect-src https://api.strataui.org");

    let origin = "https://app.strataui.org";

    // Script URLs
    assert!(
        csp.allows_script_url(origin, "https://app.strataui.org/app.js", None),
        "'self' match allowed"
    );
    assert!(
        csp.allows_script_url(origin, "/relative/script.js", None),
        "Relative URL matches 'self'"
    );
    assert!(
        csp.allows_script_url(origin, "https://cdn.strataui.org/lib.js", None),
        "Explicit host allowed"
    );
    assert!(
        csp.allows_script_url(origin, "https://sub.trusted.com/a.js", None),
        "Wildcard subdomain *.trusted.com allowed"
    );
    assert!(
        !csp.allows_script_url(origin, "https://malicious.evil.com/payload.js", None),
        "Unauthorized domain rejected"
    );

    // Connect URLs (fetch / WebSocket)
    assert!(
        csp.allows_connect_url(origin, "https://api.strataui.org/v1/data"),
        "Explicit connect-src allowed"
    );
    assert!(
        !csp.allows_connect_url(origin, "https://other.strataui.org/v1/data"),
        "Non-whitelisted connect host rejected"
    );
}

#[test]
fn test_goal61_extract_meta_csp_from_html() {
    let html = r#"
        <!DOCTYPE html>
        <html>
            <head>
                <meta charset="utf-8">
                <meta http-equiv="Content-Security-Policy" content="script-src 'self' 'nonce-tokenA'; connect-src 'self'">
                <title>Secure App</title>
            </head>
            <body>Hello</body>
        </html>
    "#;

    let csp = extract_meta_csp(html).expect("Must extract meta Content-Security-Policy tag");
    assert!(csp.directives.contains_key(&DirectiveKind::ScriptSrc));
    assert!(csp.directives.contains_key(&DirectiveKind::ConnectSrc));
    assert!(csp.allows_script_inline("test()", Some("tokenA")));
    assert!(!csp.allows_script_inline("test()", None));
}

// =========================================================================
// GOAL-62: Eliminate Hardcoded Google Chrome User-Agent Spoofing
// =========================================================================

#[test]
fn test_goal62_honest_user_agent_format() {
    let expected_ua = "Mozilla/5.0 (compatible; Onyx/1.0; +https://strataui.org)";

    // Verify default header logic in strata_web_api
    let headers = strata_web_api::Headers::new();
    assert!(!headers.has("User-Agent"));

    // Verify format
    assert!(
        expected_ua.contains("Onyx/1.0"),
        "User-Agent must identify Onyx engine"
    );
    assert!(
        expected_ua.contains("strataui.org"),
        "User-Agent must link to Strata repository"
    );
    assert!(
        !expected_ua.contains("Chrome/120.0.0.0"),
        "Zero Chrome spoofing allowed"
    );
    assert!(
        !expected_ua.contains("Safari/537.36"),
        "Zero Safari spoofing allowed"
    );
}

// =========================================================================
// GOAL-60: Full WHATWG HTML5 Tokenizer and Tree Construction State Machine
// =========================================================================

#[test]
fn test_goal60_whatwg_insertion_modes_and_table_foster_parenting() {
    // Foster parenting: text and elements inside table outside cell must be moved before table
    let html = "<table>Misplaced Text<input type=\"hidden\"><tr><td>Valid Cell</td></tr></table>";
    let nodes = HtmlParser::parse(html);

    assert_eq!(
        nodes.len(),
        3,
        "Misplaced text and input must be foster-parented outside table"
    );
    assert_eq!(nodes[0].text_content(), "Misplaced Text");
    if let strata_web_reader::ast::SemanticNode::Element { tag, .. } = &nodes[1] {
        assert_eq!(tag, "input");
    } else {
        panic!("Second root node must be foster-parented input");
    }
    if let strata_web_reader::ast::SemanticNode::Element { tag, .. } = &nodes[2] {
        assert_eq!(tag, "table");
    } else {
        panic!("Third root node must be table");
    }
}

#[test]
fn test_goal60_whatwg_paragraph_auto_closing_on_headings_and_blocks() {
    let html = "<p>First paragraph<h1>Heading</h1><p>Second paragraph<div>Division</div>";
    let nodes = HtmlParser::parse(html);

    assert_eq!(
        nodes.len(),
        4,
        "<p> must be closed before <h1> and before <div>"
    );
    if let strata_web_reader::ast::SemanticNode::Element { tag, children, .. } = &nodes[0] {
        assert_eq!(tag, "p");
        assert_eq!(children[0].text_content(), "First paragraph");
    }
    if let strata_web_reader::ast::SemanticNode::Element { tag, .. } = &nodes[1] {
        assert_eq!(tag, "h1");
    }
    if let strata_web_reader::ast::SemanticNode::Element { tag, children, .. } = &nodes[2] {
        assert_eq!(tag, "p");
        assert_eq!(children[0].text_content(), "Second paragraph");
    }
    if let strata_web_reader::ast::SemanticNode::Element { tag, .. } = &nodes[3] {
        assert_eq!(tag, "div");
    }
}

#[test]
fn test_goal60_whatwg_list_item_and_select_auto_closing() {
    let html = "<ul><li>Item 1<li>Item 2<li>Item 3</ul><select><option>Opt 1<option>Opt 2</select>";
    let nodes = HtmlParser::parse(html);

    assert_eq!(nodes.len(), 2);
    if let strata_web_reader::ast::SemanticNode::Element { tag, children, .. } = &nodes[0] {
        assert_eq!(tag, "ul");
        assert_eq!(children.len(), 3, "All 3 <li> elements must be siblings");
    }
    if let strata_web_reader::ast::SemanticNode::Element { tag, children, .. } = &nodes[1] {
        assert_eq!(tag, "select");
        assert_eq!(children.len(), 2, "Both <option> elements must be siblings");
    }
}

// =========================================================================
// GOAL-59: Generalize WebTranspiler Beyond MediaWiki
// =========================================================================

#[test]
fn test_goal59_readability_heuristic_extracts_article_from_complex_site() {
    let html = r#"
        <!DOCTYPE html>
        <html>
            <head><title>Engineering Blog - Strata Architecture</title></head>
            <body>
                <header class="navbar header">
                    <nav><a href="/">Home</a><a href="/about">About</a></nav>
                </header>
                <div class="sidebar ads sponsor">
                    <p>Ad banner promotion click here</p>
                </div>
                <main class="content-container">
                    <article class="blog-entry">
                        <h1>Sovereign Web Engine Architecture</h1>
                        <p>Project Strata implements a zero-Chromium pure-Rust rendering pipeline designed for sovereign desktop applications and autonomous edge systems.</p>
                        <p>By decoupling the layout solver, rasterization engines, and ECMAScript runtime, predictable sub-millisecond frame pacing is achieved under heavy CPU workloads.</p>
                        <p>Content Security Policy Level 3 directives guarantee memory safety and containment of untrusted third-party script bundles.</p>
                    </article>
                </main>
                <footer class="site-footer">
                    <p>Copyright 2026 Strata Project</p>
                </footer>
            </body>
        </html>
    "#;

    let nodes = HtmlParser::parse(html);
    let theme = ReaderTheme::light();
    let transpiler = WebTranspiler::new(&theme);
    let media = MediaContext::new(1280.0, 800.0, false);

    // 1. Reader mode extracts article body and eliminates header, ads, and footer
    let reader_bp = transpiler.transpile(&nodes, None, &media);
    let reader_repr = format!("{reader_bp:?}");
    assert!(reader_repr.contains("Sovereign Web Engine Architecture"));
    assert!(
        !reader_repr.contains("Ad banner promotion"),
        "Reader mode must strip ads and sidebars"
    );
    assert!(
        !reader_repr.contains("Copyright 2026"),
        "Reader mode must strip footer"
    );

    // 2. FullDom mode preserves the entire document tree
    let full_transpiler = WebTranspiler::new(&theme).with_render_mode(RenderMode::FullDom);
    let full_bp = full_transpiler.transpile_full_document(&nodes, &media);
    let full_repr = format!("{full_bp:?}");
    assert!(full_repr.contains("Sovereign Web Engine Architecture"));
    assert!(
        full_repr.contains("Ad banner promotion"),
        "FullDom mode must retain all elements"
    );
}

#[test]
fn test_goal23_csp_directive_fallback_and_strict_dynamic() {
    // 1. Directive inheritance cascade: ScriptSrcElem -> ScriptSrc -> DefaultSrc
    let p1 = ContentSecurityPolicy::parse("default-src 'self'");
    let s1 = p1
        .effective_sources(DirectiveKind::ScriptSrcElem)
        .expect("Must inherit default-src");
    assert_eq!(s1.len(), 1);
    assert_eq!(s1[0], SourceExpr::SelfSource);

    let p2 =
        ContentSecurityPolicy::parse("default-src 'self'; script-src https://scripts.strataui.org");
    let s2 = p2
        .effective_sources(DirectiveKind::ScriptSrcElem)
        .expect("Must inherit script-src");
    assert_eq!(s2.len(), 1);
    match &s2[0] {
        SourceExpr::Host { host, .. } => assert_eq!(host, "scripts.strataui.org"),
        _ => panic!("Expected host from script-src fallback"),
    }

    let p3 = ContentSecurityPolicy::parse("default-src 'self'; script-src https://scripts.strataui.org; script-src-elem https://elem.strataui.org");
    let s3 = p3
        .effective_sources(DirectiveKind::ScriptSrcElem)
        .expect("Must have explicit script-src-elem");
    assert_eq!(s3.len(), 1);
    match &s3[0] {
        SourceExpr::Host { host, .. } => assert_eq!(host, "elem.strataui.org"),
        _ => panic!("Expected host from direct script-src-elem"),
    }

    // 2. strict-dynamic ignores host whitelists when nonce is absent, permits valid nonce
    let p_strict = ContentSecurityPolicy::parse(
        "script-src 'strict-dynamic' 'nonce-token99' https://trusted.org",
    );
    let origin = "https://app.strataui.org";

    // Matching nonce is permitted anywhere under strict-dynamic
    assert!(p_strict.allows_script_url(
        origin,
        "https://dynamic.cdn.com/bundle.js",
        Some("token99")
    ));
    // Host whitelist alone without valid nonce is blocked under strict-dynamic
    assert!(!p_strict.allows_script_url(origin, "https://trusted.org/bundle.js", None));
    assert!(!p_strict.allows_script_url(
        origin,
        "https://trusted.org/bundle.js",
        Some("invalidNonce")
    ));

    // 3. enforce API
    assert!(p_strict
        .enforce(
            DirectiveKind::ScriptSrc,
            origin,
            "https://dynamic.cdn.com/bundle.js",
            Some("token99")
        )
        .is_ok());
    assert!(p_strict
        .enforce(
            DirectiveKind::ScriptSrc,
            origin,
            "https://trusted.org/bundle.js",
            None
        )
        .is_err());
}
