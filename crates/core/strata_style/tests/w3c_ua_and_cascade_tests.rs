//! Comprehensive Integration Tests for W3C User-Agent Default Stylesheet and Multi-Origin Cascade Engine.

#![allow(clippy::field_reassign_with_default)]

use strata_style::prelude::*;

#[test]
fn test_w3c_ua_stylesheet_defaults() {
    let state = PseudoState::default();

    // 1. Heading 1 (h1): display: block, font-size: 2em (32px), margin-block: 0.67em (21.44px), font-weight: bold
    let h1_style = CascadeSolver::resolve("h1", &[], None, None, &[], None, &state);
    assert_eq!(h1_style.display, Display::Block);
    assert_eq!(h1_style.font_size, 32.0);
    assert_eq!(h1_style.font_weight, FontWeight::Bold);
    assert!((h1_style.margin.top - 21.44).abs() < 0.1);
    assert!((h1_style.margin.bottom - 21.44).abs() < 0.1);

    // 2. Heading 2 (h2): display: block, font-size: 1.5em (24px), font-weight: bold
    let h2_style = CascadeSolver::resolve("h2", &[], None, None, &[], None, &state);
    assert_eq!(h2_style.display, Display::Block);
    assert_eq!(h2_style.font_size, 24.0);
    assert_eq!(h2_style.font_weight, FontWeight::Bold);

    // 3. Paragraph (p): display: block, margin-block: 1em (16px)
    let p_style = CascadeSolver::resolve("p", &[], None, None, &[], None, &state);
    assert_eq!(p_style.display, Display::Block);
    assert_eq!(p_style.margin.top, 16.0);
    assert_eq!(p_style.margin.bottom, 16.0);

    // 4. Phrasing element (span): display: inline
    let span_style = CascadeSolver::resolve("span", &[], None, None, &[], None, &state);
    assert_eq!(span_style.display, Display::Inline);

    // 5. Unordered List and List Item (ul, li)
    let ul_style = CascadeSolver::resolve("ul", &[], None, None, &[], None, &state);
    assert_eq!(ul_style.display, Display::Block);
    assert_eq!(ul_style.margin.top, 16.0);
    assert_eq!(ul_style.padding.left, 40.0);

    let li_style = CascadeSolver::resolve("li", &[], None, None, &[], None, &state);
    assert_eq!(li_style.display, Display::ListItem);

    // 6. Tabular primitives (table, tr, td, th)
    let table_style = CascadeSolver::resolve("table", &[], None, None, &[], None, &state);
    assert_eq!(table_style.display, Display::Table);
    assert_eq!(table_style.border_collapse, BorderCollapse::Separate);
    assert_eq!(table_style.border_spacing, (2.0, 2.0));

    let tr_style = CascadeSolver::resolve("tr", &[], None, None, &[], None, &state);
    assert_eq!(tr_style.display, Display::TableRow);

    let td_style = CascadeSolver::resolve("td", &[], None, None, &[], None, &state);
    assert_eq!(td_style.display, Display::TableCell);
    assert_eq!(td_style.padding.top, 1.0);

    let th_style = CascadeSolver::resolve("th", &[], None, None, &[], None, &state);
    assert_eq!(th_style.display, Display::TableCell);
    assert_eq!(th_style.font_weight, FontWeight::Bold);
    assert_eq!(th_style.text_align, TextAlign::Center);

    // 7. Inline Formatting (b, strong, i, em, del, ins, a)
    let b_style = CascadeSolver::resolve("b", &[], None, None, &[], None, &state);
    assert_eq!(b_style.font_weight, FontWeight::Bold);

    let i_style = CascadeSolver::resolve("i", &[], None, None, &[], None, &state);
    assert_eq!(i_style.font_style, FontStyle::Italic);

    let del_style = CascadeSolver::resolve("del", &[], None, None, &[], None, &state);
    assert_eq!(del_style.text_decoration, TextDecoration::LineThrough);

    let ins_style = CascadeSolver::resolve("ins", &[], None, None, &[], None, &state);
    assert_eq!(ins_style.text_decoration, TextDecoration::Underline);

    let a_style = CascadeSolver::resolve("a", &[], None, None, &[], None, &state);
    assert_eq!(a_style.color, Color::new(59, 130, 246, 255));
    assert_eq!(a_style.text_decoration, TextDecoration::Underline);
    assert_eq!(a_style.cursor, Cursor::Pointer);

    // 8. Hidden element [hidden]: display: none !important
    let hidden_target = ElementRef {
        tag: "div",
        classes: &[],
        id: None,
        attributes: &[("hidden".to_string(), "".to_string())],
    };
    let hidden_style = CascadeSolver::resolve_with_hierarchy(&hidden_target, &[], &[], None, &[], None, &state);
    assert_eq!(hidden_style.display, Display::None);
}

#[test]
fn test_multi_origin_cascade_order() {
    let state = PseudoState::default();

    // Case 1: Author normal beats UA normal (even when same specificity)
    let author_css = "p { margin-block: 0px; margin: 0px; }";
    let author_sheet = StyleSheet::parse(author_css).unwrap();
    let p_style = CascadeSolver::resolve("p", &[], None, None, &[&author_sheet], None, &state);
    assert_eq!(p_style.margin.top, 0.0);
    assert_eq!(p_style.margin.bottom, 0.0);

    // Case 2: Inline normal beats Author normal
    let author_css2 = "p.intro { color: #ff0000; }";
    let author_sheet2 = StyleSheet::parse(author_css2).unwrap();
    let p_style2 = CascadeSolver::resolve(
        "p",
        &["intro"],
        None,
        Some("color: #00ff00;"),
        &[&author_sheet2],
        None,
        &state,
    );
    assert_eq!(p_style2.color, Color::new(0, 255, 0, 255));

    // Case 3: Author !important beats Inline normal
    let author_css3 = "p.intro { color: #0000ff !important; }";
    let author_sheet3 = StyleSheet::parse(author_css3).unwrap();
    let p_style3 = CascadeSolver::resolve(
        "p",
        &["intro"],
        None,
        Some("color: #00ff00;"),
        &[&author_sheet3],
        None,
        &state,
    );
    assert_eq!(p_style3.color, Color::new(0, 0, 255, 255));

    // Case 4: Inline !important beats Author !important
    let p_style4 = CascadeSolver::resolve(
        "p",
        &["intro"],
        None,
        Some("color: #ffff00 !important;"),
        &[&author_sheet3],
        None,
        &state,
    );
    assert_eq!(p_style4.color, Color::new(255, 255, 0, 255));

    // Case 5: UA !important beats Author !important
    let author_css5 = "[hidden] { display: block !important; }";
    let author_sheet5 = StyleSheet::parse(author_css5).unwrap();
    let hidden_target = ElementRef {
        tag: "div",
        classes: &[],
        id: None,
        attributes: &[("hidden".to_string(), "".to_string())],
    };
    let style5 = CascadeSolver::resolve_with_hierarchy(
        &hidden_target,
        &[],
        &[],
        None,
        &[&author_sheet5],
        None,
        &state,
    );
    assert_eq!(style5.display, Display::None);
}

#[test]
fn test_relative_unit_cascade_resolution() {
    let state = PseudoState::default();
    let mut parent_style = ComputedStyle::default();
    parent_style.font_size = 20.0;

    let author_css = "div.box { font-size: 1.5em; margin-block: 1.0em; padding-inline: 0.5em; }";
    let sheet = StyleSheet::parse(author_css).unwrap();

    let computed = CascadeSolver::resolve(
        "div",
        &["box"],
        None,
        None,
        &[&sheet],
        Some(&parent_style),
        &state,
    );

    // Child font-size: 1.5 * 20px = 30px
    assert_eq!(computed.font_size, 30.0);
    // Margin resolves against element's own font-size: 1.0 * 30px = 30px
    assert_eq!(computed.margin.top, 30.0);
    assert_eq!(computed.margin.bottom, 30.0);
    // Padding resolves against element's own font-size: 0.5 * 30px = 15px
    assert_eq!(computed.padding.left, 15.0);
    assert_eq!(computed.padding.right, 15.0);
}

#[test]
fn test_keyword_font_size_and_font_weight() {
    let state = PseudoState::default();

    let css = r#"
        .huge { font-size: xx-large; font-weight: bolder; }
        .tiny { font-size: x-small; font-weight: lighter; }
    "#;
    let sheet = StyleSheet::parse(css).unwrap();

    let huge_style = CascadeSolver::resolve("span", &["huge"], None, None, &[&sheet], None, &state);
    assert_eq!(huge_style.font_size, 32.0);
    assert_eq!(huge_style.font_weight, FontWeight::ExtraBold);

    let tiny_style = CascadeSolver::resolve("span", &["tiny"], None, None, &[&sheet], None, &state);
    assert_eq!(tiny_style.font_size, 10.0);
    assert_eq!(tiny_style.font_weight, FontWeight::Light);
}

#[test]
fn test_line_height_normal_and_unitless() {
    let state = PseudoState::default();

    let css = r#"
        .lead { line-height: 1.6; }
        .fixed { font-size: 16px; line-height: 24px; }
        .standard { line-height: normal; }
    "#;
    let sheet = StyleSheet::parse(css).unwrap();

    let lead_style = CascadeSolver::resolve("p", &["lead"], None, None, &[&sheet], None, &state);
    assert_eq!(lead_style.line_height, 1.6);

    let fixed_style = CascadeSolver::resolve("p", &["fixed"], None, None, &[&sheet], None, &state);
    assert_eq!(fixed_style.line_height, 1.5); // 24 / 16 = 1.5

    let standard_style = CascadeSolver::resolve("p", &["standard"], None, None, &[&sheet], None, &state);
    assert_eq!(standard_style.line_height, 1.2);
}
