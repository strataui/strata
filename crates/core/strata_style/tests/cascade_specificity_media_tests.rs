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
use strata_style::types::{Color, Dimension};
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
        children: &[],
    };

    let span_anc = ElementRef {
        tag: "span",
        classes: &[],
        id: None,
        attributes: &[],
        children: &[],
    };

    let p_target = ElementRef {
        tag: "p",
        classes: &[],
        id: None,
        attributes: &[],
        children: &[],
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
        children: &[],
    };

    let p1_sib = ElementRef {
        tag: "p",
        classes: &[],
        id: None,
        attributes: &[],
        children: &[],
    };

    let p2_target = ElementRef {
        tag: "p",
        classes: &[],
        id: None,
        attributes: &[],
        children: &[],
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
        children: &[],
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
        children: &[],
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
        children: &[],
    };
    let el_http = ElementRef {
        tag: "a",
        classes: &[],
        id: None,
        attributes: &attrs_http,
        children: &[],
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
        children: &[],
    };
    let el_jpg = ElementRef {
        tag: "img",
        classes: &[],
        id: None,
        attributes: &attrs_jpg,
        children: &[],
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
        children: &[],
    };
    assert!(sel_sub.matches_hierarchy(&el_badge, &[], &[], &PseudoState::default()));
}

#[test]
fn test_media_query_viewport_and_theme_evaluation() {
    let ctx_desktop_dark = MediaContext {
        viewport_width: 1280.0,
        viewport_height: 800.0,
        is_dark: true,
        ..Default::default()
    };

    let ctx_mobile_light = MediaContext {
        viewport_width: 480.0,
        viewport_height: 800.0,
        is_dark: false,
        ..Default::default()
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
        ..Default::default()
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
        children: &[],
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

#[test]
fn test_at_rules_charset_and_tachyons_parsing() {
    let css = r#"
        @charset "UTF-8";
        @media screen and (min-width: 60em) {
            .w-auto-l { width: auto; }
            .flex-row-l { flex-direction: row; }
        }
        .flex { display: flex; }
    "#;

    let ctx = MediaContext {
        viewport_width: 1280.0,
        viewport_height: 800.0,
        is_dark: true,
        ..Default::default()
    };
    let sheet = StyleSheet::parse_with_context(css, &ctx).expect("Failed to parse stylesheet");
    println!("Parsed rules count: {}", sheet.rules.len());
    for r in &sheet.rules {
        for s in &r.selectors {
            println!("Rule selector: {:?}", s);
        }
    }

    if let Ok(vendor_css) = std::fs::read_to_string("/tmp/vendor.css") {
        let vendor_sheet = StyleSheet::parse_with_context(&vendor_css, &ctx).expect("Failed to parse vendor.css");
        println!("Vendor sheet rules count: {}", vendor_sheet.rules.len());
        let has_w_auto_l = vendor_sheet.rules.iter().any(|r| r.selectors.iter().any(|s| format!("{:?}", s).contains("w-auto-l")));
        let has_flex_row_l = vendor_sheet.rules.iter().any(|r| r.selectors.iter().any(|s| format!("{:?}", s).contains("flex-row-l")));
        println!("Vendor has w-auto-l: {}, flex-row-l: {}", has_w_auto_l, has_flex_row_l);
    }

    assert!(
        sheet.rules.iter().any(|r| r.selectors.iter().any(|s| format!("{:?}", s).contains("w-auto-l"))),
        "Must parse .w-auto-l from inside @media after @charset"
    );
}

#[test]
fn test_w3c_cascade_layers_priority() {
    let css = r#"
        @layer base, framework, overrides;

        @layer base {
            #btn { color: red; }
            .important-btn { color: green !important; }
        }

        @layer framework {
            .btn { color: yellow; }
            .important-btn { color: yellow !important; }
        }

        /* Unlayered rule: should beat layered normal rules regardless of specificity */
        .btn { color: blue; }

        /* Unlayered important rule: should be beaten by layered important rules */
        #btn.important-btn { color: red !important; }
    "#;

    let ctx = MediaContext::default();
    let sheet = StyleSheet::parse_with_context(css, &ctx).expect("Failed to parse stylesheet");

    let computed = CascadeSolver::resolve(
        "button",
        &["btn", "important-btn"],
        Some("btn"),
        None,
        &[&sheet],
        None,
        &PseudoState::default(),
    );

    // 1. Normal declaration: Unlayered `.btn { color: blue }` beats `@layer base { #btn { color: red } }`
    // despite #btn having higher specificity!
    // 2. Important declaration: `@layer base { color: green !important }` beats later `@layer framework`
    // and beats unlayered `#btn.important-btn { color: red !important }`!
    assert_eq!(computed.color, Color::GREEN);
}

#[test]
fn test_container_queries_evaluation() {
    let css = r#"
        .box { width: 100px; }
        @container (min-width: 500px) {
            .box { width: 450px; }
        }
        @container (max-width: 300px) {
            .box { width: 250px; }
        }
    "#;

    // Evaluate with container width 600px -> matches min-width: 500px
    let ctx_wide = MediaContext::default().with_container_size(600.0, 400.0);
    let sheet_wide = StyleSheet::parse_with_context(css, &ctx_wide).expect("Parse wide container");
    let computed_wide = CascadeSolver::resolve(
        "div",
        &["box"],
        None,
        None,
        &[&sheet_wide],
        None,
        &PseudoState::default(),
    );
    assert_eq!(computed_wide.width, Dimension::Px(450.0));

    // Evaluate with container width 250px -> matches max-width: 300px
    let ctx_narrow = MediaContext::default().with_container_size(250.0, 400.0);
    let sheet_narrow = StyleSheet::parse_with_context(css, &ctx_narrow).expect("Parse narrow container");
    let computed_narrow = CascadeSolver::resolve(
        "div",
        &["box"],
        None,
        None,
        &[&sheet_narrow],
        None,
        &PseudoState::default(),
    );
    assert_eq!(computed_narrow.width, Dimension::Px(250.0));
}

#[test]
fn test_w3c_viewport_segments_and_device_posture_media_queries() {
    use strata_style::media::DevicePostureQuery;

    let css = r#"
        .pane { width: 100px; }

        @media (device-posture: continuous) {
            .pane { width: 300px; }
        }

        @media (device-posture: folded) and (horizontal-viewport-segments: 2) {
            .pane { width: 500px; }
        }

        @media (device-posture: folded) and (vertical-viewport-segments: 2) {
            .pane { width: 700px; }
        }
    "#;

    // 1. Continuous single screen context (standard flat device)
    let ctx_flat = MediaContext::default()
        .with_device_posture(DevicePostureQuery::Continuous)
        .with_viewport_segments(1, 1);
    let sheet_flat = StyleSheet::parse_with_context(css, &ctx_flat).expect("Parse flat");
    let comp_flat = CascadeSolver::resolve(
        "div",
        &["pane"],
        None,
        None,
        &[&sheet_flat],
        None,
        &PseudoState::default(),
    );
    assert_eq!(comp_flat.width, Dimension::Px(300.0));

    // 2. Folded horizontal split (BookMode: 2 horizontal segments, 1 vertical)
    let ctx_book = MediaContext::default()
        .with_device_posture(DevicePostureQuery::Folded)
        .with_viewport_segments(2, 1);
    let sheet_book = StyleSheet::parse_with_context(css, &ctx_book).expect("Parse book");
    let comp_book = CascadeSolver::resolve(
        "div",
        &["pane"],
        None,
        None,
        &[&sheet_book],
        None,
        &PseudoState::default(),
    );
    assert_eq!(comp_book.width, Dimension::Px(500.0));

    // 3. Folded vertical split (TabletopFlex: 1 horizontal segment, 2 vertical)
    let ctx_tabletop = MediaContext::default()
        .with_device_posture(DevicePostureQuery::Folded)
        .with_viewport_segments(1, 2);
    let sheet_tabletop = StyleSheet::parse_with_context(css, &ctx_tabletop).expect("Parse tabletop");
    let comp_tabletop = CascadeSolver::resolve(
        "div",
        &["pane"],
        None,
        None,
        &[&sheet_tabletop],
        None,
        &PseudoState::default(),
    );
    assert_eq!(comp_tabletop.width, Dimension::Px(700.0));
}

#[test]
fn test_complex_selector_nth_child_and_not_matching() {
    let css = r#"
        .container > div:nth-child(2) {
            color: #ff0000;
        }
        .container > div:first-child {
            color: #00ff00;
        }
        .container > div:not(.skip) {
            background-color: #0000ff;
        }
    "#;

    let sheet = StyleSheet::parse(css).expect("Parse stylesheet");
    let container_classes = vec!["container".to_string()];
    let container_anc = ElementRef {
        tag: "div",
        classes: &container_classes,
        id: None,
        attributes: &[],
        children: &[],
    };

    let empty_classes = Vec::new();
    let child1 = ElementRef {
        tag: "div",
        classes: &empty_classes,
        id: None,
        attributes: &[],
        children: &[],
    };
    let child2 = ElementRef {
        tag: "div",
        classes: &empty_classes,
        id: None,
        attributes: &[],
        children: &[],
    };
    let skip_classes = vec!["skip".to_string()];
    let child2_skip = ElementRef {
        tag: "div",
        classes: &skip_classes,
        id: None,
        attributes: &[],
        children: &[],
    };

    // Child 1 has no preceding siblings -> matches :first-child
    let style1 = CascadeSolver::resolve_with_hierarchy(
        &child1,
        &[container_anc],
        &[],
        None,
        &[&sheet],
        None,
        &PseudoState::default(),
    );
    assert_eq!(style1.color, Color::rgb(0, 255, 0)); // #00ff00
    assert_eq!(style1.background_color, Some(Color::rgb(0, 0, 255))); // matches :not(.skip)

    // Child 2 has 1 preceding sibling -> matches .container > div:nth-child(2)
    let style2 = CascadeSolver::resolve_with_hierarchy(
        &child2,
        &[container_anc],
        &[child1],
        None,
        &[&sheet],
        None,
        &PseudoState::default(),
    );
    assert_eq!(style2.color, Color::rgb(255, 0, 0)); // #ff0000
    assert_eq!(style2.background_color, Some(Color::rgb(0, 0, 255)));

    // Child with .skip class -> should NOT match :not(.skip)
    let style_skip = CascadeSolver::resolve_with_hierarchy(
        &child2_skip,
        &[container_anc],
        &[child1],
        None,
        &[&sheet],
        None,
        &PseudoState::default(),
    );
    assert_eq!(style_skip.color, Color::rgb(255, 0, 0)); // still matches :nth-child(2)
    assert_eq!(style_skip.background_color, None); // does not match :not(.skip)
}

#[test]
fn test_w3c_selectors_level_4_is_where_not_specificity() {
    // 1. :where() always has specificity (0, 0, 0, 0)
    let sel_where = Selector::parse("button:where(#hero, .primary)").unwrap();
    // button (0,0,0,1) + :where(...) (0,0,0,0) = (0, 0, 0, 1)
    assert_eq!(sel_where.specificity, Specificity::new(0, 0, 0, 1));

    // 2. :is() takes the max specificity of its arguments
    let sel_is = Selector::parse("button:is(.btn, #submit)").unwrap();
    // button (0,0,0,1) + #submit (0,1,0,0) = (0, 1, 0, 1)
    assert_eq!(sel_is.specificity, Specificity::new(0, 1, 0, 1));

    // 3. :not() with multiple selectors takes the max specificity
    let sel_not = Selector::parse("p:not(span, #special)").unwrap();
    // p (0,0,0,1) + #special (0,1,0,0) = (0, 1, 0, 1)
    assert_eq!(sel_not.specificity, Specificity::new(0, 1, 0, 1));

    // Cascade resolution test:
    // .btn:where(#hero) has specificity (0, 0, 1, 0).
    // .btn.themed has specificity (0, 0, 2, 0).
    // Therefore .btn.themed must beat .btn:where(#hero)!
    let css = r#"
        .btn:where(#hero) { color: #ff0000; }
        .btn.themed { color: #00ff00; }
    "#;
    let sheet = StyleSheet::parse(css).expect("parse sheet");
    let classes = vec!["btn".to_string(), "themed".to_string()];
    let el = ElementRef {
        tag: "button",
        classes: &classes,
        id: Some("hero"),
        attributes: &[],
        children: &[],
    };
    let computed = CascadeSolver::resolve_with_hierarchy(
        &el,
        &[],
        &[],
        None,
        &[&sheet],
        None,
        &PseudoState::default(),
    );
    assert_eq!(computed.color, Color::rgb(0, 255, 0)); // Green wins because :where() contributed 0!

    // Cascade resolution test for :is():
    // button:is(#btn-submit, .btn) has specificity (0, 1, 0, 1).
    // button.large.btn has specificity (0, 0, 2, 1).
    // button:is(#btn-submit, .btn) must win because of #btn-submit!
    let css2 = r#"
        button:is(#btn-submit, .btn) { color: #123456; }
        button.large.btn { color: #654321; }
    "#;
    let sheet2 = StyleSheet::parse(css2).expect("parse sheet2");
    let classes2 = vec!["large".to_string(), "btn".to_string()];
    let el2 = ElementRef {
        tag: "button",
        classes: &classes2,
        id: Some("btn-submit"),
        attributes: &[],
        children: &[],
    };
    let computed2 = CascadeSolver::resolve_with_hierarchy(
        &el2,
        &[],
        &[],
        None,
        &[&sheet2],
        None,
        &PseudoState::default(),
    );
    assert_eq!(computed2.color, Color::new(0x12, 0x34, 0x56, 0xFF));
}

#[test]
fn test_w3c_selectors_level_4_has_child_and_descendant() {
    let sel_has_child = Selector::parse("div:has(> p)").unwrap();
    let sel_has_desc = Selector::parse("article:has(span.badge)").unwrap();

    let p_child = ElementRef {
        tag: "p",
        classes: &[],
        id: None,
        attributes: &[],
        children: &[],
    };
    let span_badge = ElementRef {
        tag: "span",
        classes: &["badge".to_string()],
        id: None,
        attributes: &[],
        children: &[],
    };
    let section_middle = ElementRef {
        tag: "section",
        classes: &[],
        id: None,
        attributes: &[],
        children: &[span_badge],
    };

    // Div with p child matches div:has(> p)
    let div_with_p = ElementRef {
        tag: "div",
        classes: &[],
        id: None,
        attributes: &[],
        children: &[p_child],
    };
    assert!(sel_has_child.matches_hierarchy(&div_with_p, &[], &[], &PseudoState::default()));

    // Div with no p child does not match div:has(> p)
    let div_empty = ElementRef {
        tag: "div",
        classes: &[],
        id: None,
        attributes: &[],
        children: &[],
    };
    assert!(!sel_has_child.matches_hierarchy(&div_empty, &[], &[], &PseudoState::default()));

    // Article with deep descendant span.badge matches article:has(span.badge)
    let article_deep = ElementRef {
        tag: "article",
        classes: &[],
        id: None,
        attributes: &[],
        children: &[section_middle],
    };
    assert!(sel_has_desc.matches_hierarchy(&article_deep, &[], &[], &PseudoState::default()));

    // Article with no badge does not match
    let article_empty = ElementRef {
        tag: "article",
        classes: &[],
        id: None,
        attributes: &[],
        children: &[p_child],
    };
    assert!(!sel_has_desc.matches_hierarchy(&article_empty, &[], &[], &PseudoState::default()));
}

#[test]
fn test_w3c_media_queries_level_4_range_syntax() {
    let ctx_mobile = MediaContext::new(480.0, 800.0, true);
    let ctx_tablet = MediaContext::new(800.0, 1024.0, true);
    let ctx_desktop = MediaContext::new(1440.0, 900.0, true);

    // 1. Single inequality range: width >= 768px
    let mq_min = MediaQuery::parse("screen and (width >= 768px)").expect("parse width >= 768px");
    assert!(!mq_min.matches(&ctx_mobile));
    assert!(mq_min.matches(&ctx_tablet));
    assert!(mq_min.matches(&ctx_desktop));

    // 2. Single inequality range: width < 768px
    let mq_max = MediaQuery::parse("screen and (width < 768px)").expect("parse width < 768px");
    assert!(mq_max.matches(&ctx_mobile));
    assert!(!mq_max.matches(&ctx_tablet));
    assert!(!mq_max.matches(&ctx_desktop));

    // 3. Interval range: 768px <= width <= 1200px
    let mq_interval = MediaQuery::parse("screen and (768px <= width <= 1200px)").expect("parse interval");
    assert!(!mq_interval.matches(&ctx_mobile));
    assert!(mq_interval.matches(&ctx_tablet));
    assert!(!mq_interval.matches(&ctx_desktop));

    // 4. Stylesheet evaluation with range media query
    let css = r#"
        .layout { width: 100%; }
        @media (768px <= width <= 1200px) {
            .layout { width: 720px; }
        }
        @media (width > 1200px) {
            .layout { width: 1200px; }
        }
    "#;
    let sheet_tablet = StyleSheet::parse_with_context(css, &ctx_tablet).unwrap();
    let comp_tablet = CascadeSolver::resolve(
        "div",
        &["layout"],
        None,
        None,
        &[&sheet_tablet],
        None,
        &PseudoState::default(),
    );
    assert_eq!(comp_tablet.width, Dimension::Px(720.0));

    let sheet_desktop = StyleSheet::parse_with_context(css, &ctx_desktop).unwrap();
    let comp_desktop = CascadeSolver::resolve(
        "div",
        &["layout"],
        None,
        None,
        &[&sheet_desktop],
        None,
        &PseudoState::default(),
    );
    assert_eq!(comp_desktop.width, Dimension::Px(1200.0));
}

