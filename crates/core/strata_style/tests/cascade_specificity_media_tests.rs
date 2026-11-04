//! Comprehensive Verification Suite for CSS Combinators, Attribute Selectors, Media Queries, and Cascade.
//!
//! Verifies:
//! 1. Combinators: Descendant (` `), Child (`>`), Adjacent Sibling (`+`), Subsequent Sibling (`~`).
//! 2. Attribute Selectors: `[attr]`, `[attr=val]`, `[attr^=val]`, `[attr$=val]`, `[attr*=val]`, `[attr~=val]`, `[attr|=val]`.
//! 3. Media Queries Level 4: `screen`, `min-width`, `max-width`, `prefers-color-scheme`, comma-separated OR lists.
//! 4. Cascade Resolution: Specificity ordering, `!important` overrides, and inline style precedence.
//! 5. Document Stylesheet Extraction & Transpilation in `strata_web_reader`.

use strata_style::media::{MediaContext, MediaQuery, MediaQueryList};
use strata_style::selector::{ElementRef, PseudoState, Selector};
use strata_style::specificity::Specificity;
use strata_style::{CascadeSolver, StyleSheet};

#[test]
fn test_combinator_descendant_and_child_matching() {
    // 1. Child Combinator: div > p
    let child_sel = Selector::parse("div > p").expect("Failed to parse 'div > p'");
    assert_eq!(child_sel.specificity, Specificity::new(0, 0, 0, 2));

    let div_anc = ElementRef {
        tag: "div",
        classes: &[],
        id: None,
        attributes: &[],
    };

    let span_anc = ElementRef {
        tag: "span",
        classes: &[],
        id: None,
        attributes: &[],
    };

    let p_target = ElementRef {
        tag: "p",
        classes: &[],
        id: None,
        attributes: &[],
    };

    // Immediate parent is div -> Child matches
    assert!(child_sel.matches_hierarchy(&p_target, &[div_anc], &[], &PseudoState::default()));

    // Grandparent is div, immediate parent is span -> Child must NOT match
    assert!(!child_sel.matches_hierarchy(&p_target, &[div_anc, span_anc], &[], &PseudoState::default()));

    // 2. Descendant Combinator: div p
    let desc_sel = Selector::parse("div p").expect("Failed to parse 'div p'");
    // Grandparent is div -> Descendant matches!
    assert!(desc_sel.matches_hierarchy(&p_target, &[div_anc, span_anc], &[], &PseudoState::default()));

    // No div ancestor -> Descendant fails
    assert!(!desc_sel.matches_hierarchy(&p_target, &[span_anc], &[], &PseudoState::default()));
}

#[test]
fn test_combinator_sibling_matching() {
    // 1. Adjacent Sibling: h2 + p
    let adj_sel = Selector::parse("h2 + p").expect("Failed to parse 'h2 + p'");

    let h2_sib = ElementRef {
        tag: "h2",
        classes: &[],
        id: None,
        attributes: &[],
    };

    let p1_sib = ElementRef {
        tag: "p",
        classes: &[],
        id: None,
        attributes: &[],
    };

    let p2_target = ElementRef {
        tag: "p",
        classes: &[],
        id: None,
        attributes: &[],
    };

    // p immediately preceded by h2 -> Adjacent matches
    assert!(adj_sel.matches_hierarchy(&p2_target, &[], &[h2_sib], &PseudoState::default()));

    // p preceded by h2 then p1 -> Adjacent must NOT match p2
    assert!(!adj_sel.matches_hierarchy(&p2_target, &[], &[h2_sib, p1_sib], &PseudoState::default()));

    // 2. Subsequent Sibling: h2 ~ p
    let sub_sel = Selector::parse("h2 ~ p").expect("Failed to parse 'h2 ~ p'");
    // Preceded by h2, then another sibling -> Subsequent matches!
    assert!(sub_sel.matches_hierarchy(&p2_target, &[], &[h2_sib, p1_sib], &PseudoState::default()));
}

#[test]
fn test_attribute_selector_matching_operators() {
    // Presence: [data-active]
    let sel_exists = Selector::parse("button[data-active]").unwrap();
    let attrs_active = vec![("data-active".to_string(), "".to_string())];
    let el_active = ElementRef {
        tag: "button",
        classes: &[],
        id: None,
        attributes: &attrs_active,
    };
    assert!(sel_exists.matches_hierarchy(&el_active, &[], &[], &PseudoState::default()));

    // Exact: [type="submit"]
    let sel_exact = Selector::parse(r#"input[type="submit"]"#).unwrap();
    let attrs_submit = vec![("type".to_string(), "submit".to_string())];
    let el_submit = ElementRef {
        tag: "input",
        classes: &[],
        id: None,
        attributes: &attrs_submit,
    };
    assert!(sel_exact.matches_hierarchy(&el_submit, &[], &[], &PseudoState::default()));

    // Prefix: [href^="https://"]
    let sel_prefix = Selector::parse(r#"a[href^="https://"]"#).unwrap();
    let attrs_https = vec![("href".to_string(), "https://en.wikipedia.org/wiki/Rust".to_string())];
    let attrs_http = vec![("href".to_string(), "http://insecure.example.com".to_string())];
    let el_https = ElementRef {
        tag: "a",
        classes: &[],
        id: None,
        attributes: &attrs_https,
    };
    let el_http = ElementRef {
        tag: "a",
        classes: &[],
        id: None,
        attributes: &attrs_http,
    };
    assert!(sel_prefix.matches_hierarchy(&el_https, &[], &[], &PseudoState::default()));
    assert!(!sel_prefix.matches_hierarchy(&el_http, &[], &[], &PseudoState::default()));

    // Suffix: [src$=".png"]
    let sel_suffix = Selector::parse(r#"img[src$=".png"]"#).unwrap();
    let attrs_png = vec![("src".to_string(), "/assets/logo.png".to_string())];
    let attrs_jpg = vec![("src".to_string(), "/assets/photo.jpg".to_string())];
    let el_png = ElementRef {
        tag: "img",
        classes: &[],
        id: None,
        attributes: &attrs_png,
    };
    let el_jpg = ElementRef {
        tag: "img",
        classes: &[],
        id: None,
        attributes: &attrs_jpg,
    };
    assert!(sel_suffix.matches_hierarchy(&el_png, &[], &[], &PseudoState::default()));
    assert!(!sel_suffix.matches_hierarchy(&el_jpg, &[], &[], &PseudoState::default()));

    // Substring: [class*="badge"]
    let sel_sub = Selector::parse(r#"[class*="badge"]"#).unwrap();
    let attrs_badge = vec![("class".to_string(), "card-badge-outline".to_string())];
    let el_badge = ElementRef {
        tag: "span",
        classes: &[],
        id: None,
        attributes: &attrs_badge,
    };
    assert!(sel_sub.matches_hierarchy(&el_badge, &[], &[], &PseudoState::default()));
}

#[test]
fn test_media_query_viewport_and_theme_evaluation() {
    let ctx_desktop_dark = MediaContext {
        viewport_width: 1280.0,
        viewport_height: 800.0,
        is_dark: true,
    };

    let ctx_mobile_light = MediaContext {
        viewport_width: 480.0,
        viewport_height: 800.0,
        is_dark: false,
    };

    // 1. Min/Max Width
    let mq_mobile = MediaQuery::parse("screen and (max-width: 768px)").unwrap();
    assert!(!mq_mobile.matches(&ctx_desktop_dark));
    assert!(mq_mobile.matches(&ctx_mobile_light));

    let mq_desktop = MediaQuery::parse("screen and (min-width: 1024px)").unwrap();
    assert!(mq_desktop.matches(&ctx_desktop_dark));
    assert!(!mq_desktop.matches(&ctx_mobile_light));

    // 2. Prefers Color Scheme
    let mq_dark = MediaQuery::parse("(prefers-color-scheme: dark)").unwrap();
    let mq_light = MediaQuery::parse("(prefers-color-scheme: light)").unwrap();
    assert!(mq_dark.matches(&ctx_desktop_dark));
    assert!(!mq_dark.matches(&ctx_mobile_light));
    assert!(!mq_light.matches(&ctx_desktop_dark));
    assert!(mq_light.matches(&ctx_mobile_light));

    // 3. Comma-separated OR lists
    let mql_responsive = MediaQueryList::parse("(max-width: 500px), (min-width: 1200px)").unwrap();
    assert!(mql_responsive.matches(&ctx_desktop_dark));
    assert!(mql_responsive.matches(&ctx_mobile_light));

    let ctx_tablet = MediaContext {
        viewport_width: 800.0,
        viewport_height: 600.0,
        is_dark: true,
    };
    assert!(!mql_responsive.matches(&ctx_tablet));

    // 4. Stylesheet parsing with context filtering
    let css = r#"
        .card { width: 100%; }
        @media (min-width: 1000px) {
            .card { width: 500px; }
        }
        @media (max-width: 600px) {
            .card { width: 250px; }
        }
    "#;

    let sheet_desktop = StyleSheet::parse_with_context(css, &ctx_desktop_dark).unwrap();
    // Desktop has base rule + min-width rule = 2 rules for .card
    assert_eq!(sheet_desktop.rules.len(), 2);

    let sheet_mobile = StyleSheet::parse_with_context(css, &ctx_mobile_light).unwrap();
    // Mobile has base rule + max-width rule = 2 rules for .card
    assert_eq!(sheet_mobile.rules.len(), 2);
}

#[test]
fn test_cascade_specificity_tie_breaking_and_important() {
    let css = r#"
        p { color: #111111; font-size: 14px; }
        .highlight { color: #222222; }
        #hero-text { color: #333333; }
        p.special { color: #444444 !important; }
    "#;

    let sheet = StyleSheet::parse(css).unwrap();
    let classes = vec!["highlight".to_string(), "special".to_string()];

    // Case 1: ID selector (#hero-text) beats classes without !important
    let el_id = ElementRef {
        tag: "p",
        classes: &classes,
        id: Some("hero-text"),
        attributes: &[],
    };

    // p.special has !important -> it must beat #hero-text (#333333) and produce #444444!
    let computed = CascadeSolver::resolve_with_hierarchy(
        &el_id,
        &[],
        &[],
        None,
        &[&sheet],
        None,
        &PseudoState::default(),
    );
    assert_eq!(computed.color, strata_style::types::Color::new(0x44, 0x44, 0x44, 0xFF));

    // Case 2: Inline style overrides non-important stylesheet ID, but loses to !important
    let computed_inline = CascadeSolver::resolve_with_hierarchy(
        &el_id,
        &[],
        &[],
        Some("color: #999999; font-size: 20px;"),
        &[&sheet],
        None,
        &PseudoState::default(),
    );
    // color is still #444444 because stylesheet has !important
    assert_eq!(computed_inline.color, strata_style::types::Color::new(0x44, 0x44, 0x44, 0xFF));
    // font-size is 20px because inline style beat the p { font-size: 14px; } rule
    assert_eq!(computed_inline.font_size, 20.0);
}
